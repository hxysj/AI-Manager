import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import {
  parse,
  compileScript,
  compileTemplate,
  compileStyleAsync
} from "@vue/compiler-sfc"
import { transformSync } from "esbuild"

// 使用真实 Vue 响应式、模板事件和主界面的保存队列，只模拟原生 IPC。
const require = createRequire(import.meta.url)
const vue = require("vue")
const pets = [
  { id: "first", name: "第一只" },
  { id: "previous", name: "上次桌宠" }
]
let selectedDirectory = null
let logStatsResponse = { bytes: 1536, fileCount: 2 }
let logStatsFailure = false
const logStatsPolls = new Map()
// 仅驱动组件轮询回调，不启动真实计时器。
globalThis.window = {
  setInterval(callback) {
    const id = Symbol()
    logStatsPolls.set(id, callback)
    return id
  },
  clearInterval(id) {
    logStatsPolls.delete(id)
  }
}
const stub = { render: () => vue.h("span") }
const components = new Map()
const modelListeners = new Set()
let installedTranslationModels = []
let translationModelsFailure = false
let assistantDesktopStatus = {}
const assistantStatusListeners = new Set()

async function loadComponent(filename) {
  const { descriptor, errors } = parse(readFileSync(filename, "utf8"), {
    filename
  })
  assert.deepEqual(errors, [])
  const script = compileScript(descriptor, {
    id: filename,
    genDefaultAs: "__component"
  })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename,
    id: filename,
    compilerOptions: { bindingMetadata: script.bindings, hoistStatic: false }
  })
  assert.deepEqual(template.errors, [])
  for (const style of descriptor.styles) {
    const result = await compileStyleAsync({
      source: style.content,
      filename,
      id: filename,
      scoped: style.scoped,
      preprocessLang: style.lang
    })
    assert.deepEqual(result.errors, [])
  }
  const module = { exports: {} }
  new Function(
    "require",
    "module",
    "exports",
    transformSync(
      `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
      { format: "cjs" }
    ).code
  )(
    (name) => {
      if (name === "@/api")
        return {
          appApi: {
            getAppLogStats: async () => {
              if (logStatsFailure) throw new Error("统计读取失败")
              return logStatsResponse
            }
          },
          assistantApi: {
            desktopStatus: async () => assistantDesktopStatus,
            onDesktopStatus: async (callback) => {
              assistantStatusListeners.add(callback)
              return () => assistantStatusListeners.delete(callback)
            }
          },
          systemApi: { selectDirectory: async () => selectedDirectory },
          translationApi: {
            desktopStatus: async () => ({}),
            onDesktopStatus: () => () => {}
          }
        }
      if (name === "@/api/modules/local-models")
        return {
          localModelsApi: {
            installed: async (capability) => {
              assert.equal(capability, "translation")
              if (translationModelsFailure)
                throw new Error("模拟翻译模型读取失败")
              return { models: installedTranslationModels }
            }
          }
        }
      if (name === "@/utils/shortcuts") {
        const imported = { exports: {} }
        new Function(
          "module",
          "exports",
          transformSync(readFileSync("src/utils/shortcuts.js", "utf8"), {
            format: "cjs"
          }).code
        )(imported, imported.exports)
        return imported.exports
      }
      if (name === "@/api/request")
        return {
          subscribeReady: async (event, callback) => {
            assert.equal(event, "local-models:changed")
            modelListeners.add(callback)
            return () => modelListeners.delete(callback)
          }
        }
      if (name === "@/features/tools/constant")
        return {
          getToolItems: () =>
            ["git", "lan-share"].map((id) => ({ id, label: id, icon: stub }))
        }
      if (name === "@/constants/desktop-pets")
        return {
          desktopPets: pets,
          getDesktopPet: (id) => pets.find((pet) => pet.id === id) || pets[0]
        }
      if (name === "lucide-vue-next") return new Proxy({}, { get: () => stub })
      if (name === "element-plus")
        return { ElOption: stub, ElOptionGroup: stub, ElSelect: stub }
      if (name.includes("/style/css")) return {}
      if (components.has(name))
        return { __esModule: true, default: components.get(name) }
      if (name.endsWith(".vue"))
        return {
          __esModule: true,
          default: {
            render() {
              return vue.h("div", this.$slots.default?.())
            }
          }
        }
      return require(name)
    },
    module,
    module.exports
  )
  return module.exports.default
}

globalThis.Document = class Document {}
globalThis.ShadowRoot = class ShadowRoot {}
globalThis.document = Object.assign(new Document(), { activeElement: null })
function node(tag, text = "") {
  return {
    tagName: tag.toUpperCase(),
    text,
    type: "",
    children: [],
    parent: null,
    props: {},
    listeners: {},
    value: "",
    checked: false,
    addEventListener(name, handler) {
      ;(this.listeners[name] ||= []).push(handler)
    },
    removeEventListener() {},
    getRootNode: () => document,
    reportValidity: () => true,
    setAttribute(name, value) {
      this.props[name] = value
    },
    removeAttribute(name) {
      delete this.props[name]
    }
  }
}
const renderer = vue.createRenderer({
  createElement: node,
  createText: (text) => node("#text", text),
  createComment: (text) => node("#comment", text),
  setText: (item, text) => {
    item.text = text
  },
  setElementText: (item, text) => {
    item.children = []
    item.text = text
  },
  parentNode: (item) => item.parent,
  nextSibling: (item) =>
    item.parent?.children[item.parent.children.indexOf(item) + 1] || null,
  patchProp(item, key, previous, value) {
    item.props[key] = value
    if (["value", "type", "checked"].includes(key)) item[key] = value
  },
  insert(item, parent, anchor) {
    if (item.parent)
      item.parent.children.splice(item.parent.children.indexOf(item), 1)
    item.parent = parent
    const index = anchor ? parent.children.indexOf(anchor) : -1
    if (index < 0) parent.children.push(item)
    else parent.children.splice(index, 0, item)
  },
  remove(item) {
    if (item.parent)
      item.parent.children.splice(item.parent.children.indexOf(item), 1)
  }
})
const runtimeErrors = []
function mount(component, props) {
  const root = node("root")
  const vnode = vue.h({ render: () => vue.h(component, props) })
  vnode.appContext = { ...vue.createApp({})._context }
  vnode.appContext.config.errorHandler = (error) => runtimeErrors.push(error)
  vnode.appContext.components["el-input"] = stub
  renderer.render(vnode, root)
  return {
    root,
    state: vnode.component.subTree.component.setupState,
    unmount: () => renderer.render(null, root)
  }
}
function find(root, predicate) {
  return [
    root,
    ...root.children.flatMap((item) => find(item, predicate))
  ].filter(predicate)
}
function input(root, name, value) {
  return find(
    root,
    (item) => item.props.name === name && item.props.value === value
  )[0]
}
function trigger(item, name) {
  assert.ok(item, `找不到用于触发 ${name} 的控件`)
  const event = { target: item, stopPropagation() {}, preventDefault() {} }
  for (const listener of item.listeners[name] || []) listener(event)
  const handler = item.props[`on${name[0].toUpperCase()}${name.slice(1)}`]
  for (const listener of [handler].flat().filter(Boolean)) listener(event)
}
async function tick() {
  for (let index = 0; index < 5; index++) {
    await Promise.resolve()
    await vue.nextTick()
  }
}

for (const name of [
  "DesktopPetList",
  "DesktopPetDetail",
  "SystemSettings",
  "AssistantExtension"
]) {
  const filename = `src/features/settings/components/${name}.vue`
  components.set(`@/${filename.slice(4)}`, await loadComponent(filename))
}
const Settings = await loadComponent("src/features/settings/index.vue")
const TranslationConfig = await loadComponent(
  "src/features/settings/components/TranslationConfigModal.vue"
)
const calls = []
const notifications = []
let server = {
  dataPath: "D:/data",
  defaultDataPath: "D:/default",
  cliConfigPaths: { claude: "D:/claude", codex: "D:/codex" },
  defaultCliConfigPaths: {
    claude: "D:/default-claude",
    codex: "D:/default-codex"
  },
  desktop: { enabled: false, petId: "" },
  system: {
    autoLaunchEnabled: false,
    closeAction: "ask",
    quickSwitchVisible: true
  },
  cloudSync: { username: "saved-user" },
  agents: {
    translation: { enabled: false, targetId: "provider", model: "test-model" }
  }
}
const props = vue.reactive({
  appSettings: structuredClone(server),
  cliTargets: [],
  pending: false
})
const appSource = parse(readFileSync("src/App.vue", "utf8")).descriptor
  .scriptSetup.content
const queueSource = appSource.slice(
  appSource.indexOf("let settingsSaveQueue"),
  appSource.indexOf("async function checkForAppUpdates")
)
const saveSettings = new Function(
  "settingsApi",
  "updateState",
  "showSuccessMessage",
  "showErrorMessage",
  "refreshLocalBackups",
  `${queueSource}\nreturn saveSettings`
)(
  {
    saveSettings: (patch) =>
      new Promise((resolve, reject) => calls.push({ patch, resolve, reject }))
  },
  (state) => {
    props.appSettings = state.appSettings
  },
  (text) => notifications.push({ success: true, text }),
  (error) => notifications.push({ success: false, text: error.message }),
  async () => {}
)
props.saveSettings = saveSettings
const settings = mount(Settings, props)
await tick()
assert.equal(calls.length, 0, "初始化不提交设置")
assert.equal(
  find(settings.root, (item) => item.props.class === "settings-view__save")
    .length,
  0
)

function merge(target, patch) {
  for (const [key, value] of Object.entries(patch)) {
    if (value && typeof value === "object") merge((target[key] ||= {}), value)
    else target[key] = value
  }
}
async function finish(index, error) {
  const call = calls[index]
  assert.ok(call, `缺少第 ${index + 1} 次保存请求`)
  if (error) call.reject(new Error(error))
  else {
    merge(server, call.patch)
    call.resolve({ appSettings: structuredClone(server) })
  }
  await tick()
}

// 连续开启、关闭同一设置时，较早的响应不能改回用户刚关闭的状态。
settings.state.activeTab = "system"
await tick()
const autoLaunchSwitch = () =>
  find(settings.root, (item) => item.props.name === "auto-launch-enabled")[0]
trigger(autoLaunchSwitch(), "click")
await tick()
trigger(autoLaunchSwitch(), "click")
await tick()
assert.equal(calls.length, 1, "保存请求串行执行")
assert.deepEqual(calls[0].patch, { system: { autoLaunchEnabled: true } })
assert.equal(notifications.length, 0, "落盘前不提示保存成功")
settings.state.draft.cloudSync.username = "尚未完成的输入"
await finish(0)
assert.equal(autoLaunchSwitch().props["aria-checked"], false)
assert.equal(settings.state.draft.cloudSync.username, "尚未完成的输入")
assert.deepEqual(calls[1].patch, { system: { autoLaunchEnabled: false } })
await finish(1)
assert.equal(server.system.autoLaunchEnabled, false)
assert.equal(server.cloudSync.username, "saved-user")
assert.equal(notifications.filter((item) => item.success).length, 2)

// 单个工具只提交自己的开关；失败时恢复已保存值，后续修改仍能继续保存。
const tool = find(
  settings.root,
  (item) => item.tagName === "INPUT" && item.props.type === "checkbox"
)[0]
tool.checked = false
trigger(tool, "change")
await tick()
assert.deepEqual(calls[2].patch, {
  system: { quickSwitchTools: { git: false } }
})
await finish(2, "模拟磁盘写入失败")
assert.equal(tool.checked, true)
assert.equal(notifications.at(-1).success, false)
trigger(input(settings.root, "close-action", "minimize"), "click")
await tick()
assert.deepEqual(calls[3].patch, { system: { closeAction: "minimize" } })
await finish(3)
assert.equal(server.system.closeAction, "minimize")

// 文本输入完成才保存当前字段，同时保留另一个云服务的未提交草稿。
settings.state.activeTab = "data"
await tick()
settings.state.draft.koofrSync.username = "另一个服务的草稿"
const username = find(
  settings.root,
  (item) => item.tagName === "INPUT" && item.value === "尚未完成的输入"
)[0]
username.value = "new-user"
trigger(username, "input")
assert.equal(calls.length, 4)
trigger(username, "change")
await tick()
assert.deepEqual(calls[4].patch, { cloudSync: { username: "new-user" } })
await finish(4)
assert.equal(settings.state.draft.koofrSync.username, "另一个服务的草稿")
const interval = find(
  settings.root,
  (item) => item.tagName === "INPUT" && item.props.type === "number"
)[0]
interval.value = "30"
trigger(interval, "input")
trigger(interval, "change")
await tick()
assert.deepEqual(calls[5].patch, { localBackup: { intervalMinutes: 30 } })
await finish(5)

// 目录选择和重置只修改对应路径，不提交另一种 CLI 的目录。
selectedDirectory = "D:/new-codex"
await settings.state.selectDirectory("codex", "D:/codex")
await tick()
assert.deepEqual(calls[6].patch, { cliConfigPaths: { codex: "D:/new-codex" } })
await finish(6)
settings.state.resetDataPath()
await tick()
assert.deepEqual(calls[7].patch, { dataPath: "D:/default" })
await finish(7)

// 桌宠总开关使用上次启动记录，浏览详情和其他区域的草稿不会一并保存。
settings.state.handleToggleMaster(true)
await tick()
assert.deepEqual(calls[8].patch, {
  desktop: { enabled: true, petId: "first" },
  system: { quickSwitchVisible: true }
})
await finish(8)
server.desktop = { enabled: false, petId: "previous" }
props.appSettings = structuredClone(server)
await tick()
settings.state.activeDesktopPetId = "first"
settings.state.handleToggleMaster(true)
await tick()
assert.equal(calls[9].patch.desktop.petId, "previous")
assert.equal(calls[9].patch.cloudSync, undefined)
await finish(9)

// 离开设置页面后，已提交的修改仍会完成落盘和成功提示。
settings.state.saveTranslationChanges({ targetLanguage: "English" })
await tick()
assert.deepEqual(calls[10].patch, {
  agents: { translation: { targetLanguage: "English" } }
})
settings.unmount()
await finish(10)
assert.equal(server.agents.translation.targetLanguage, "English")
assert.equal(notifications.at(-1).text, "修改已保存")

// 桌宠总开关和详情开关在请求返回前也能连续切换，旧响应不覆盖最新选择。
const desktopSettings = mount(Settings, props)
desktopSettings.state.activeTab = "desktop"
desktopSettings.state.activeDesktopPetId = "previous"
await tick()
const switches = () =>
  find(desktopSettings.root, (item) => item.props.role === "switch")
assert.equal(switches().length, 2)
for (const [offset, index] of [0, 1].entries()) {
  const requestIndex = 11 + offset * 2
  trigger(switches()[index], "click")
  await tick()
  assert.equal(switches()[index].props["aria-checked"], false)
  trigger(switches()[index], "click")
  await tick()
  assert.equal(switches()[index].props["aria-checked"], true)
  assert.deepEqual(calls[requestIndex].patch, { desktop: { enabled: false } })
  await finish(requestIndex)
  assert.equal(switches()[index].props["aria-checked"], true)
  assert.deepEqual(calls[requestIndex + 1].patch, {
    desktop: { enabled: true, petId: "previous" },
    system: { quickSwitchVisible: true }
  })
  await finish(requestIndex + 1)
}
desktopSettings.unmount()

// 系统组件可独立维护设置；页面卸载不丢保存，程序操作仍交给已有流程。
const systemActions = []
const systemProps = vue.reactive({
  get settings() {
    return props.appSettings.system
  },
  pending: false,
  saveSettings,
  onQuitApp: () => systemActions.push("quit"),
  onUninstallWithoutTrace: () => systemActions.push("uninstall")
})
const systemSettings = mount(
  components.get("@/features/settings/components/SystemSettings.vue"),
  systemProps
)
await tick()
assert.equal(calls.length, 15, "单独挂载系统设置不会提交配置")
const floatingSwitch = () =>
  find(
    systemSettings.root,
    (item) => item.props.name === "quick-switch-visible"
  )[0]
trigger(floatingSwitch(), "click")
await tick()
assert.deepEqual(calls[15].patch, { system: { quickSwitchVisible: false } })
assert.equal(
  find(
    systemSettings.root,
    (item) => item.props.class === "system-settings-tools"
  ).length,
  0,
  "关闭悬浮窗后隐藏工具配置"
)
// 其他状态刷新时保留正在保存的选择。
props.appSettings = structuredClone(server)
await tick()
assert.equal(floatingSwitch().props["aria-checked"], false)
await finish(15, "模拟保存悬浮窗设置失败")
assert.equal(floatingSwitch().props["aria-checked"], true)
const actionButtons = find(
  systemSettings.root,
  (item) => item.props.class === "system-settings-danger-button"
)
for (const button of actionButtons) trigger(button, "click")
assert.deepEqual(systemActions, ["quit", "uninstall"])
trigger(floatingSwitch(), "click")
await tick()
systemSettings.unmount()
await finish(16)
assert.equal(server.system.quickSwitchVisible, false)

// 日志开关沿用局部保存与失败恢复，存储统计独立更新，卸载后停止轮询。
const loggingSettings = mount(
  components.get("@/features/settings/components/SystemSettings.vue"),
  systemProps
)
await tick()
assert.equal(loggingSettings.state.logStorageText, "1.50 KB")
const logSwitch = () =>
  find(loggingSettings.root, (item) => item.props.name === "log-enabled")[0]
assert.equal(logSwitch().props["aria-checked"], true)
trigger(logSwitch(), "click")
await tick()
assert.deepEqual(calls[17].patch, { system: { logEnabled: false } })
await finish(17)
assert.equal(logSwitch().props["aria-checked"], false)
assert.equal(server.system.logEnabled, false)
trigger(logSwitch(), "click")
await tick()
await finish(18, "模拟保存日志开关失败")
assert.equal(logSwitch().props["aria-checked"], false)
logStatsResponse = { bytes: 2 * 1024 * 1024, fileCount: 7 }
await loggingSettings.state.loadLogStats()
assert.equal(loggingSettings.state.logStorageText, "2.00 MB")
logStatsFailure = true
await loggingSettings.state.loadLogStats()
assert.equal(loggingSettings.state.logStorageText, "读取失败，请刷新")
logStatsFailure = false
loggingSettings.unmount()
assert.equal(logStatsPolls.size, 0)

// 翻译弹窗按改动字段立即保存，重复快捷键不会被提交。
const translationChanges = []
const translationProps = vue.reactive({
  modelValue: {
    targetId: "provider",
    model: "test-model",
    targetLanguage: "简体中文",
    shortcut: "Ctrl+Alt+T",
    clipboardShortcut: "Ctrl+Alt+Shift+T"
  },
  "onUpdate:modelValue": (value) => {
    translationProps.modelValue = value
  },
  onSave: (changes) => translationChanges.push(changes)
})
const translation = mount(TranslationConfig, translationProps)
translation.state.update({ targetLanguage: "日本語" })
assert.deepEqual(translationChanges.at(-1), { targetLanguage: "日本語" })
translation.state.update({ shortcut: "Ctrl+Alt+Shift+T" })
assert.equal(translationChanges.length, 1)
assert.ok(translation.state.shortcutError.includes("不能相同"))
translation.state.selectTarget("account:test")
assert.deepEqual(translationChanges.at(-1), {
  targetId: "account:test",
  model: "test-model"
})
await tick()
assert.equal(translation.state.targetGroups[0].items[0].disabled, true)
const beforeLocalSelection = translationChanges.length
translation.state.selectTarget("local")
assert.equal(
  translationChanges.length,
  beforeLocalSelection,
  "未下载翻译模型时不能选择本地来源"
)
const translationModelFixtures = [
  {
    id: "lmt-60-0.6b",
    name: "LMT-60-0.6B",
    languages: [
      { code: "zh", label: "简体中文" },
      { code: "en", label: "English" },
      { code: "ja", label: "日本語" }
    ]
  },
  {
    id: "test-translation-secondary",
    name: "翻译测试模型 B",
    languages: [
      { code: "en", label: "English" },
      { code: "fr", label: "Français" }
    ]
  }
]
installedTranslationModels = translationModelFixtures
await Promise.all([...modelListeners].map((callback) => callback()))
translation.state.selectTarget("local")
await tick()
assert.equal(translationProps.modelValue.model, "lmt-60-0.6b")
assert.equal(translationProps.modelValue.sourceLanguage, "auto")
assert.equal(translation.state.local, true)
assert.deepEqual(
  translation.state.modelOptions.map((model) => model.name),
  ["LMT-60-0.6B", "翻译测试模型 B"]
)
assert.equal(translation.state.invalidTarget, false)
translation.state.update({ sourceLanguage: "ja" })
assert.deepEqual(translationChanges.at(-1), { sourceLanguage: "ja" })
assert.ok(
  !translation.state.languages.includes("繁體中文"),
  "不把繁简转换冒充模型独立支持的语言"
)
translation.state.selectModel("test-translation-secondary")
await tick()
assert.deepEqual(translationChanges.at(-1), {
  targetId: "local",
  model: "test-translation-secondary",
  sourceLanguage: "auto",
  targetLanguage: "English"
})
assert.deepEqual(
  [...translation.state.languages],
  ["English", "Français"],
  "切换模型后使用该模型自己的语言范围"
)
translation.state.update({ sourceLanguage: "en", targetLanguage: "Français" })
await tick()
await Promise.all([...modelListeners].map((callback) => callback()))
assert.equal(
  translationProps.modelValue.model,
  "test-translation-secondary",
  "刷新不能改回第一个模型"
)
translation.state.selectTarget("local")
await tick()
assert.equal(translationProps.modelValue.model, "test-translation-secondary")
assert.equal(translationProps.modelValue.sourceLanguage, "en")
assert.equal(translationProps.modelValue.targetLanguage, "Français")
installedTranslationModels = [translationModelFixtures[1]]
await Promise.all([...modelListeners].map((callback) => callback()))
assert.equal(
  translation.state.invalidTarget,
  false,
  "删除其他模型不影响当前选择"
)
translation.unmount()
assert.equal(modelListeners.size, 0, "翻译配置卸载时清理模型监听")
const translationReopened = mount(TranslationConfig, translationProps)
await tick()
assert.equal(
  translationProps.modelValue.model,
  "test-translation-secondary",
  "重开配置保留实际选中的模型 ID"
)
installedTranslationModels = [translationModelFixtures[0]]
await Promise.all([...modelListeners].map((callback) => callback()))
assert.equal(
  translationReopened.state.invalidTarget,
  true,
  "选中模型删除后失效，即使仍有其他模型"
)
assert.equal(
  translationProps.modelValue.model,
  "test-translation-secondary",
  "不能静默替换已删除的模型"
)
translationReopened.state.selectModel("lmt-60-0.6b")
await tick()
assert.equal(translationReopened.state.invalidTarget, false)
translationModelsFailure = true
await Promise.all([...modelListeners].map((callback) => callback()))
assert.equal(
  translationReopened.state.invalidTarget,
  true,
  "读取失败后不能沿用旧模型列表"
)
assert.equal(translationReopened.state.modelOptions.length, 0)
assert.match(translationReopened.state.localModelsError, /翻译模型读取失败/)
translationReopened.unmount()
assert.equal(modelListeners.size, 0, "翻译配置卸载时清理模型监听")

// 助手开关与模型复用自动保存队列，失败恢复且不提交其他功能的草稿。
props.assistantRequest = 1
const assistantSettings = mount(Settings, props)
await tick()
assert.equal(assistantSettings.state.activeTab, "agents")
const assistantSwitch = () =>
  find(
    assistantSettings.root,
    (item) => item.props["aria-label"] === "启用应用助手"
  )[0]
assert.equal(assistantSwitch().props["aria-checked"], true)
assistantSettings.state.draft.agents.translation.model = "未提交的翻译模型"
let assistantRequestIndex = calls.length
trigger(assistantSwitch(), "click")
await tick()
assert.deepEqual(calls[assistantRequestIndex].patch, {
  agents: { assistant: { enabled: false } }
})
await finish(assistantRequestIndex)
assert.equal(assistantSwitch().props["aria-checked"], false)
assistantRequestIndex = calls.length
trigger(assistantSwitch(), "click")
await tick()
await finish(assistantRequestIndex, "模拟助手设置保存失败")
assert.equal(assistantSwitch().props["aria-checked"], false)
assert.equal(
  assistantSettings.state.draft.agents.translation.model,
  "未提交的翻译模型"
)
assistantRequestIndex = calls.length
const saveAssistant = assistantSettings.state.saveChanges({
  agents: {
    assistant: { targetId: "provider", model: "agent-model", protocol: "chat" }
  }
})
await tick()
assert.deepEqual(calls[assistantRequestIndex].patch, {
  agents: {
    assistant: { targetId: "provider", model: "agent-model", protocol: "chat" }
  }
})
await finish(assistantRequestIndex)
await saveAssistant
assert.equal(server.agents.assistant.model, "agent-model")
assert.equal(server.agents.assistant.enabled, false)
const shortcutInput = () => find(
  assistantSettings.root,
  item => item.props["aria-label"] === "唤出输入框快捷键"
)[0]
assert.equal(shortcutInput().props.value, "Ctrl+Alt+Space")
assistantRequestIndex = calls.length
shortcutInput().props.onKeydown({
  key: "a", code: "KeyA", ctrlKey: true, shiftKey: true, preventDefault() {}
})
await tick()
assert.deepEqual(calls[assistantRequestIndex].patch, {
  agents: { assistant: { shortcut: "Ctrl+Shift+A" } }
})
await finish(assistantRequestIndex)
assert.equal(shortcutInput().props.value, "Ctrl+Shift+A")
assert.equal(server.agents.assistant.model, "agent-model")
const invalidShortcutIndex = calls.length
shortcutInput().props.onKeydown({ key: "Escape", preventDefault() {} })
await tick()
assert.equal(calls.length, invalidShortcutIndex)
assert.match(find(assistantSettings.root, item => item.props.role === "alert")
  .map(item => item.text).join(""), /请使用字母/)
shortcutInput().props.onKeydown({
  key: "a", ctrlKey: true, shiftKey: true, preventDefault() {}
})
await tick()
if (calls.length > invalidShortcutIndex) await finish(invalidShortcutIndex)
assistantDesktopStatus = { error: "助手快捷键已被占用，请更换快捷键" }
await Promise.all([...assistantStatusListeners].map(callback => callback()))
await tick()
assert.match(find(assistantSettings.root, item => item.props.role === "alert")
  .map(item => item.text).join(""), /已被占用/)
assistantSettings.unmount()
assert.equal(assistantStatusListeners.size, 0)
assert.deepEqual(runtimeErrors, [])
console.log(
  "设置自动保存：字段隔离、失败恢复、助手快捷键录制与冲突提示、日志、桌宠和本地翻译配置验证通过"
)

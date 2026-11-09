import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { parse, compileScript, compileTemplate } from "@vue/compiler-sfc"
import { transformSync } from "esbuild"
import * as vue from "vue"

const require = createRequire(import.meta.url)
const filename = "src/features/screenshot/index.vue"
const { descriptor, errors } = parse(readFileSync(filename, "utf8"), {
  filename
})
assert.deepEqual(errors, [])
const script = compileScript(descriptor, {
  id: "screenshot-settings-test",
  genDefaultAs: "__component"
})
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: "screenshot-settings-test",
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])

let storedSettings = {
  enabled: true,
  hotkeys: true,
  shortcutStyle: "alt",
  restorePins: false,
  historyLimit: 25,
  ocr: { useLargeModel: false, targetId: "", model: "" },
  shortcuts: {
    captureAutoCopy: "Alt+F6",
    capture: "Ctrl+Q",
    clipboard: "F9",
    toggle: "Shift+F9",
    switchGroup: "Ctrl+F9",
    resetInput: "Ctrl+Alt+F10",
    captureLong: "Ctrl+Alt+F12"
  }
}
const listeners = new Map()
const reads = []
const writes = []
const notifications = []
let earlyRefresh = true
let deferReads = true
let installedModels = [
  { id: "minicpm-ocr-1.0", name: "MiniCPM-OCR 1.0", capabilities: ["ocr"] }
]
let localModelsFailure = false
const response = () => ({
  entries: [],
  settings: structuredClone(storedSettings),
  errors: [],
  hotkeyErrors: [],
  dataPath: "test/screenshots"
})
const stub = { render: () => null }
const module = { exports: {} }
new Function(
  "require",
  "module",
  "exports",
  transformSync(`${script.content}\nexport default __component`, {
    format: "cjs"
  }).code
)(
  (name) => {
    if (name === "vue") return vue
    if (name === "lucide-vue-next") return new Proxy({}, { get: () => stub })
    if (name === "element-plus") return { ElOption: stub, ElSelect: stub }
    if (name.includes("/style/css")) return {}
    if (name === "@/components/BaseModal.vue") {
      return { __esModule: true, default: stub }
    }
    if (name === "@/api/modules/system") return { systemApi: {} }
    if (name === "@/api/modules/local-models") {
      return {
        localModelsApi: {
          installed: async (capability) => {
            assert.equal(capability, "ocr")
            if (localModelsFailure) throw new Error("模拟本地模型读取失败")
            return { models: structuredClone(installedModels) }
          }
        }
      }
    }
    if (name === "@/utils/message") {
      return {
        createMessage: { success: (message) => notifications.push(message) }
      }
    }
    if (name === "@/api/request") {
      return {
        subscribeReady: async (event, callback) => {
          listeners.set(event, callback)
          // 原生快捷键在启动或暂停时发出通知，可能抢先开始首次读取。
          if (event === "screenshot:changed" && earlyRefresh) callback()
          return () => listeners.delete(event)
        }
      }
    }
    if (name === "@/api/modules/screenshot") {
      return {
        screenshotApi: {
          call: async (action, payload) => {
            if (action === "list") {
              if (!deferReads) return response()
              return new Promise((resolve, reject) =>
                reads.push({ resolve, reject })
              )
            }
            if (action === "settings") {
              writes.push(structuredClone(payload))
              storedSettings = structuredClone(payload)
            }
          }
        }
      }
    }
    return require(name)
  },
  module,
  module.exports
)

// 使用真实 Vue 挂载和生命周期，省略与本次异步状态回归无关的 DOM。
const component = { ...module.exports.default, render: () => null }
const renderer = vue.createRenderer({
  createElement: () => ({}),
  createText: () => ({}),
  createComment: () => ({}),
  setText() {},
  setElementText() {},
  patchProp() {},
  insert() {},
  remove() {},
  parentNode: () => null,
  nextSibling: () => null
})
const runtimeErrors = []
function mount() {
  const app = renderer.createApp(component, { settingsOnly: true })
  app.config.errorHandler = (error) => runtimeErrors.push(error)
  app.mount({})
  return { app, state: app._instance.setupState }
}
async function tick() {
  for (let index = 0; index < 10; index++) {
    await Promise.resolve()
    await vue.nextTick()
  }
}

const first = mount()
await tick()
assert.equal(reads.length, 1, "初始化和事件刷新应共用正在进行的读取")
assert.equal(first.state.ready, false)
await first.state.saveSettings()
assert.equal(writes.length, 0, "读取完成前不能保存默认草稿")
deferReads = false
reads.shift().resolve(response())
await tick()
assert.equal(first.state.settings.shortcuts.capture, "Ctrl+Q")
assert.equal(
  first.state.settingsDraft.shortcuts.capture,
  "Ctrl+Q",
  "事件先触发读取时，设置草稿仍须使用已保存的快捷键"
)
assert.deepEqual(first.state.settingsDraft, storedSettings)

first.state.settingsDraft.shortcuts.capture = "Ctrl+Shift+F8"
await listeners.get("screenshot:changed")()
assert.equal(
  first.state.settingsDraft.shortcuts.capture,
  "Ctrl+Shift+F8",
  "后续刷新不能覆盖正在编辑的草稿"
)
await first.state.saveSettings()
assert.equal(writes.length, 1)
assert.equal(writes[0].shortcuts.capture, "Ctrl+Shift+F8")
assert.equal(writes[0].shortcuts.clipboard, "F9")
assert.equal(writes[0].historyLimit, 25)
assert.equal(writes[0].restorePins, false)
assert.ok(notifications.includes("截图设置已保存"))
first.app.unmount()

earlyRefresh = false
const reopened = mount()
await tick()
assert.deepEqual(reopened.state.settingsDraft, storedSettings)
reopened.app.unmount()

// 首次读取失败后，重试成功仍须初始化草稿，不能保存失败前的默认值。
earlyRefresh = true
deferReads = true
const retried = mount()
await tick()
reads.shift().reject(new Error("模拟读取失败"))
await tick()
assert.equal(retried.state.ready, false)
assert.match(retried.state.error, /读取失败/)
await retried.state.saveSettings()
assert.equal(writes.length, 1)
const refreshing = listeners.get("screenshot:changed")()
reads.shift().resolve(response())
await refreshing
await tick()
assert.deepEqual(retried.state.settingsDraft, storedSettings)
assert.equal(retried.state.ready, true)
assert.equal(retried.state.error, "")
retried.app.unmount()

const disposed = mount()
await tick()
disposed.app.unmount()
reads.shift().resolve(response())
await tick()
assert.equal(disposed.state.ready, false, "关闭弹框后不能继续初始化设置")
assert.equal(writes.length, 1)
assert.deepEqual(runtimeErrors, [])
assert.equal(listeners.size, 0)

// 本地模型使用实际下载状态，删除后禁止保存，读取失败也不能沿用旧列表。
deferReads = false
earlyRefresh = false
const local = mount()
await tick()
local.state.settingsDraft.ocr.useLargeModel = true
local.state.settingsDraft.ocr.targetId = "local"
local.state.selectOcrTarget()
assert.equal(local.state.settingsDraft.ocr.model, "minicpm-ocr-1.0")
assert.equal(local.state.invalidOcrTarget, false)
assert.equal(local.state.invalidLocalModel, false)
assert.deepEqual([...local.state.ocrModelOptions], ["minicpm-ocr-1.0"])
await local.state.saveSettings()
assert.deepEqual(storedSettings.ocr, {
  useLargeModel: true,
  targetId: "local",
  model: "minicpm-ocr-1.0"
})
local.app.unmount()
const localReopened = mount()
await tick()
assert.equal(localReopened.state.settingsDraft.ocr.targetId, "local")
assert.equal(localReopened.state.settingsDraft.ocr.model, "minicpm-ocr-1.0")
installedModels = []
await listeners.get("local-models:changed")()
assert.deepEqual([...localReopened.state.ocrModelOptions], [])
await localReopened.state.saveSettings()
assert.match(localReopened.state.error, /请选择已下载的本地 OCR 模型/)
localModelsFailure = true
await listeners.get("local-models:changed")()
await localReopened.state.saveSettings()
assert.match(localReopened.state.error, /读取本地模型失败/)
localReopened.app.unmount()
assert.equal(listeners.size, 0)
assert.deepEqual(runtimeErrors, [])
console.log(
  "截图设置初始化、本地模型选择保存、删除校验、失败重试和监听清理验证通过"
)

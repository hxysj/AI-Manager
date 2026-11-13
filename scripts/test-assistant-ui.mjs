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

// 真实挂载 Vue 助手和 JSON 工具，仅模拟 IPC 与浏览器宿主。
const require = createRequire(import.meta.url)
const vue = require("vue")
const listeners = new Set()
const editorListeners = new Set()
const localListeners = new Set()
const settingsListeners = new Set()
const calls = []
const replies = []
const claimed = new Set()
const messageResolvers = new Map()
let delayedMessages = false
let deferListener
let threadCounter = 2
let pendingEditors = []
const threads = [
  { id: "t1", title: "已有对话", updatedAt: "2026-11-13T00:00:00Z" },
  { id: "t2", title: "另一个对话", updatedAt: "2026-11-13T00:00:00Z" }
]
const baseTask = {
  runId: "old-run",
  threadId: "t1",
  status: "completed",
  message: "重复指令",
  text: "已完成",
  tools: []
}
const history = [
  {
    id: "earlier-user",
    role: "user",
    content: { parts: [{ type: "text", text: "重复指令" }] }
  },
  {
    id: "earlier-answer",
    role: "assistant",
    content: { parts: [{ type: "text", text: "上一次回答" }] }
  },
  {
    id: "old-run",
    role: "user",
    content: { parts: [{ type: "text", text: "重复指令" }] }
  },
  {
    id: "old-answer",
    role: "assistant",
    content: { parts: [{ type: "text", text: "已完成" }] }
  }
]
const emit = (event) => {
  for (const callback of listeners) callback(event)
}
const assistantApi = {
  onEvent: (callback) =>
    deferListener
      ? new Promise((resolve) => {
          deferListener.finish = () => {
            listeners.add(callback)
            resolve(() => listeners.delete(callback))
          }
        })
      : Promise.resolve(
          (listeners.add(callback), () => listeners.delete(callback))
        ),
  bootstrap: async () => ({
    threads: { threads: [...threads], hasMore: false },
    state: { ...baseTask },
    settings: {
      targetId: "codex-provider",
      model: "agent-model",
      protocol: "auto"
    }
  }),
  messages: async (id) => {
    if (delayedMessages)
      return new Promise((resolve) => messageResolvers.set(id, resolve))
    return { messages: id === "t1" ? history : [], hasMore: false }
  },
  listThreads: async () => ({ threads: [...threads], hasMore: false }),
  createThread: async () => ({
    id: `t${++threadCounter}`,
    title: "新对话",
    updatedAt: "2026-11-13T00:00:00Z"
  }),
  deleteThread: async (id) => calls.push(["delete", id]),
  send: async (payload) => {
    calls.push(["send", payload])
    const task = {
      ...baseTask,
      runId: "new-run",
      status: "thinking",
      message: payload.message,
      text: "",
      tools: []
    }
    emit({ type: "state", state: task })
    emit({ type: "text-delta", runId: task.runId, text: "先到达的回复" })
    return { ...task, text: "" }
  },
  cancel: async (runId) => {
    calls.push(["cancel", runId])
    emit({ type: "state", state: { ...baseTask, runId, status: "cancelled" } })
  },
  onEditorRequest: async (callback) => {
    editorListeners.add(callback)
    return () => editorListeners.delete(callback)
  },
  editorPending: async () => pendingEditors,
  editorClaim: async (payload) => {
    if (claimed.has(payload.id) || payload.runId === "cancelled") return false
    claimed.add(payload.id)
    return true
  },
  editorResult: async (payload) => replies.push(payload)
}
const settingsApi = {
  saveSettings: async (patch) => calls.push(["settings", patch])
}
const appApi = {
  openToolWindow: async (payload) => calls.push(["window", payload]),
  showMainPanel: async (payload) => calls.push(["main", payload]),
  onStateChangedReady: async (callback) => {
    settingsListeners.add(callback)
    return () => settingsListeners.delete(callback)
  }
}
globalThis.window = {
  innerWidth: 1000,
  innerHeight: 800,
  setTimeout,
  clearTimeout,
  addEventListener() {},
  removeEventListener() {}
}
globalThis.localStorage = { getItem: () => null, setItem() {} }
globalThis.Document = class {
  activeElement = null
}
globalThis.document = new Document()
function host(type, text = "") {
  return {
    type,
    text,
    children: [],
    parent: null,
    props: {},
    value: "",
    scrollTop: 0,
    scrollHeight: 500,
    addEventListener() {},
    removeEventListener() {},
    focus() {},
    setAttribute() {},
    removeAttribute() {},
    get options() {
      return this.children.filter((node) => node.type === "option")
    },
    getRootNode: () => document
  }
}
const renderer = vue.createRenderer({
  createElement: host,
  createText: (text) => host("#text", text),
  createComment: (text) => host("#comment", text),
  setText: (node, text) => {
    node.text = text
  },
  setElementText: (node, text) => {
    node.children = []
    node.text = text
  },
  parentNode: (node) => node.parent,
  nextSibling: (node) =>
    node.parent?.children[node.parent.children.indexOf(node) + 1] || null,
  patchProp: (node, key, previous, value) => {
    node.props[key] = value
    if (key === "value") node.value = value
  },
  insert(node, parent, anchor) {
    if (node.parent)
      node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = parent
    const index = anchor ? parent.children.indexOf(anchor) : -1
    if (index < 0) parent.children.push(node)
    else parent.children.splice(index, 0, node)
  },
  remove(node) {
    if (node.parent)
      node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = null
  }
})
async function compile(filename) {
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
    compilerOptions: { bindingMetadata: script.bindings }
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
        return { assistantApi, settingsApi, appApi, toolboxApi: {} }
      if (name === "@/api/modules/local-models")
        return {
          localModelsApi: {
            installed: async (capability) => {
              assert.equal(capability, "assistant")
              return { models: [] }
            }
          }
        }
      if (name === "@/api/request")
        return {
          subscribeReady: async (event, callback) => {
            localListeners.add(callback)
            return () => localListeners.delete(callback)
          }
        }
      if (name === "element-plus") {
        const stub = {
          setup:
            (_, { slots }) =>
            () =>
              vue.h("div", slots.default?.())
        }
        return { ElOption: stub, ElOptionGroup: stub, ElSelect: stub }
      }
      if (name.endsWith(".css") || name.startsWith("element-plus/")) return {}
      if (name === "@/utils/formatters") {
        const result = { exports: {} }
        new Function(
          "module",
          "exports",
          transformSync(readFileSync("src/utils/formatters.js", "utf8"), {
            format: "cjs"
          }).code
        )(result, result.exports)
        return result.exports
      }
      if (name === "@/utils/message")
        return {
          createMessage: { error: (text) => calls.push(["error", text]) }
        }
      if (name === "@langchain/openai") return { ChatOpenAI: class {} }
      if (name === "langchain") return { createAgent() {}, tool() {} }
      if (name === "@tauri-apps/api/event")
        return { listen: async () => () => {} }
      if (name.endsWith(".vue"))
        return { __esModule: true, default: { render: () => vue.h("div") } }
      return require(name)
    },
    module,
    module.exports
  )
  return module.exports.default
}
function mount(component, props = {}) {
  const root = host("root")
  const vnode = vue.h(component, props)
  const errors = []
  vnode.appContext = { ...vue.createApp({})._context }
  vnode.appContext.config.errorHandler = (error) => errors.push(error)
  renderer.render(vnode, root)
  return {
    state: vnode.component.setupState,
    root,
    errors,
    unmount: () => renderer.render(null, root)
  }
}
async function tick() {
  for (let index = 0; index < 16; index++) {
    await Promise.resolve()
    await vue.nextTick()
  }
}
const [Assistant, JsonEditor, AssistantExtension] = await Promise.all([
  compile("src/features/tools/assistant/index.vue"),
  compile("src/features/tools/jsonAgent/index.vue"),
  compile("src/features/settings/components/AssistantExtension.vue")
])
const view = mount(Assistant, {
  providers: [
    {
      id: "codex-provider",
      cli: "codex",
      name: "API",
      runtimeConfig: { mainModel: "agent-model" }
    },
    {
      id: "claude-provider",
      cli: "claude",
      name: "Claude",
      runtimeConfig: { mainModel: "claude-model" }
    }
  ]
})
await tick()
assert.equal(view.state.initialized, true)
assert.equal(view.state.threadId, "t1")
assert.equal(view.state.visibleMessages.length, 3, "同名指令的旧轮次不能被隐藏")
view.state.draft = "打开 JSON 工具"
assert.equal(view.state.canSend, true)
await view.state.sendMessage()
await tick()
assert.equal(
  calls.some((call) => call[0] === "settings"),
  false,
  "发送读取已保存模型，不修改设置"
)
assert.equal(
  view.state.task.text,
  "先到达的回复",
  "IPC 初始回包不能覆盖流式事件"
)
assert.equal(view.state.busy, true)
await view.state.stop()
await tick()
assert.equal(view.state.busy, false)
assert.equal(calls.find((call) => call[0] === "cancel")[1], "new-run")
view.state.settings.targetId = "local"
view.state.draft = "本地模型"
await tick()
assert.equal(view.state.canSend, false, "没有本地助手模型时禁止发送")
for (const callback of settingsListeners)
  callback({
    appSettings: {
      agents: {
        assistant: {
          enabled: true,
          targetId: "claude-provider",
          model: "claude-model",
          protocol: "messages"
        }
      }
    }
  })
await tick()
assert.equal(
  view.state.settings.model,
  "claude-model",
  "设置保存后同步当前模型"
)
await view.state.openSettings()
assert.deepEqual(calls.at(-1), ["main", { toolId: "assistant-settings" }])
await view.state.openWindow()
assert.deepEqual(calls.at(-1), ["window", { toolId: "assistant" }])
await view.state.openMainPanel()
assert.deepEqual(calls.at(-1), ["main", { toolId: "assistant" }])
await view.state.openJsonEditor()
assert.deepEqual(calls.at(-1), ["window", { toolId: "json-agent" }])

// 新布局中的模型切换只保存模型字段，失败保留当前配置。
await view.state.selectModel("claude-new")
assert.deepEqual(calls.at(-1), ["settings", { agents: { assistant: { model: "claude-new" } } }])
assert.equal(view.state.settings.model, "claude-new")
assert.equal(view.state.settings.targetId, "claude-provider")
assert.equal(view.state.settings.protocol, "messages")
const originalSaveSettings = settingsApi.saveSettings
settingsApi.saveSettings = async () => { throw new Error("模型保存失败") }
await view.state.selectModel("not-saved")
assert.equal(view.state.settings.model, "claude-new")
assert.equal(view.state.errorMessage, "模型保存失败")
settingsApi.saveSettings = originalSaveSettings

const copiedTexts = []
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { clipboard: { writeText: async text => copiedTexts.push(text) } }
})
const answer = view.state.displayMessages.find(message => message.role === "assistant" && message.text)
await view.state.copyMessage(answer)
assert.equal(copiedTexts.at(-1), answer.text)
assert.equal(view.state.copiedMessageId, answer.id)
view.state.rateMessage(answer.id, "up")
assert.equal(view.state.messageRatings[answer.id], "up")
view.state.rateMessage(answer.id, "up")
assert.equal(view.state.messageRatings[answer.id], "")
view.state.rateMessage(answer.id, "down")
assert.equal(view.state.messageRatings[answer.id], "down")
view.state.draft = "请检查附件"
const attachment = { value: "test.json", files: [{ name: "test.json", size: 15, text: async () => '{"enabled":true}' }] }
await view.state.attachTextFile({ target: attachment })
assert.equal(view.state.draft, '请检查附件\n\n文件：test.json\n{"enabled":true}')
assert.equal(attachment.value, "")
const validDraft = view.state.draft
await view.state.attachTextFile({ target: { value: "large.txt", files: [{ name: "large.txt", size: 120001 }] } })
assert.match(view.state.errorMessage, /附件过大/)
assert.equal(view.state.draft, validDraft, "附件读取失败不能覆盖草稿")
delayedMessages = true
const first = view.state.selectThread("t2")
const latest = view.state.selectThread("t1")
messageResolvers.get("t1")({ messages: history, hasMore: false })
await latest
messageResolvers.get("t2")({ messages: [], hasMore: false })
await first
assert.equal(view.state.messages.length, 4, "过期历史请求不能覆盖当前会话")
delayedMessages = false
view.unmount()
assert.equal(listeners.size, 0)
assert.equal(localListeners.size, 0)
assert.equal(settingsListeners.size, 0)
assert.deepEqual(view.errors, [])

// 配置组件只提交修改字段，Provider、协议和本地占位共用设置保存流程。
const savedPatches = []
const configModel = vue.reactive({
  enabled: true,
  targetId: "codex-provider",
  model: "agent-model",
  protocol: "auto"
})
const config = mount(AssistantExtension, {
  modelValue: configModel,
  providers: [
    {
      id: "codex-provider",
      cli: "codex",
      name: "API",
      runtimeConfig: { mainModel: "agent-model" }
    },
    {
      id: "claude-provider",
      cli: "claude",
      name: "Claude",
      runtimeConfig: { mainModel: "claude-model" }
    },
    { id: "disabled-provider", cli: "codex", name: "停用", enabled: false }
  ],
  openRequest: 1,
  onSave: (patch) => {
    savedPatches.push(patch)
    Object.assign(configModel, patch)
  }
})
await tick()
assert.equal(config.state.expanded, true)
config.state.selectTarget("claude-provider")
await tick()
assert.deepEqual(savedPatches.at(-1), {
  targetId: "claude-provider",
  model: "claude-model",
  protocol: "auto"
})
assert.equal(config.state.protocolOptions.at(-1).id, "messages")
config.state.selectTarget("local")
await tick()
assert.equal(config.state.local, true)
assert.deepEqual(config.state.modelOptions, [])
config.state.selectTarget("account:test")
await tick()
assert.deepEqual(
  config.state.protocolOptions.map((option) => option.id),
  ["auto", "responses"]
)
config.state.selectTarget("disabled-provider")
await tick()
assert.equal(config.state.invalidTarget, true)
config.unmount()
assert.deepEqual(config.errors, [])

const originalBootstrap = assistantApi.bootstrap
let createdDisabledThread = false
const originalCreateThread = assistantApi.createThread
assistantApi.createThread = async () => {
  createdDisabledThread = true
  return originalCreateThread()
}
assistantApi.bootstrap = async () => ({
  threads: { threads: [], hasMore: false },
  state: { status: "idle", enabled: false },
  settings: {
    enabled: false,
    targetId: "codex-provider",
    model: "agent-model",
    protocol: "auto"
  }
})
const disabledView = mount(Assistant)
await tick()
assert.equal(disabledView.state.initialized, true)
assert.equal(disabledView.state.statusText, "已关闭")
assert.equal(disabledView.state.canSend, false)
assert.equal(createdDisabledThread, false)
disabledView.unmount()
assistantApi.bootstrap = originalBootstrap
assistantApi.createThread = originalCreateThread

deferListener = {}
const abandoned = mount(Assistant)
abandoned.unmount()
deferListener.finish()
await tick()
assert.equal(listeners.size, 0, "初始化中卸载也必须清理监听")
deferListener = null

const edit = {
  id: "edit-1",
  runId: "run-1",
  operation: "write",
  source: "原始文本",
  result: '{"enabled":true,"items":[1,2]}'
}
pendingEditors = [edit]
const editor = mount(JsonEditor)
await tick()
assert.equal(editor.state.sourceText, "原始文本")
assert.equal(editor.state.parsedJson.enabled, true)
assert.equal(editor.state.historyEntries.length, 1)
assert.equal(
  replies.at(-1).result.result,
  JSON.stringify(JSON.parse(edit.result), null, 2)
)
const secondEditor = mount(JsonEditor)
await tick()
for (const callback of editorListeners) callback(edit)
await tick()
assert.equal(
  replies.filter((reply) => reply.id === edit.id).length,
  1,
  "事件、快照与多个窗口不能重复执行"
)
for (const callback of editorListeners)
  callback({ id: "read-1", runId: "run-1", operation: "read" })
await tick()
assert.equal(replies.at(-1).result.source, "原始文本")
for (const callback of editorListeners)
  callback({
    id: "bad-1",
    runId: "run-1",
    operation: "write",
    result: "{invalid"
  })
await tick()
assert.ok(replies.at(-1).error)
assert.equal(
  editor.state.parsedJson.enabled,
  true,
  "无效 JSON 不覆盖现有编辑内容"
)
for (const callback of editorListeners)
  callback({
    ...edit,
    id: "cancel-1",
    runId: "cancelled",
    result: '{"enabled":false}'
  })
await tick()
assert.equal(editor.state.parsedJson.enabled, true, "已取消的写入不能执行")
editor.unmount()
secondEditor.unmount()
assert.equal(editorListeners.size, 0)
assert.deepEqual(editor.errors, [])
assert.deepEqual(secondEditor.errors, [])
console.log(
  "助手界面验证通过：流式恢复、历史竞态、Provider / 本地来源、停止、监听清理、JSON 写入回读、去重和取消保护"
)

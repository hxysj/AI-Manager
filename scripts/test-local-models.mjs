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
import * as vue from "vue"

const require = createRequire(import.meta.url)
const filename = "src/features/settings/components/LocalModelExtension.vue"
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

const listeners = new Set()
const calls = []
const messages = []
const reads = []
let deferReads = false
let failAction = ""
let deferSubscription
const sources = [{ id: "modelscope", name: "ModelScope（官方）" }, { id: "huggingface", name: "Hugging Face（官方）" }]
const compatibility = {
  allowed: true,
  validation: { passed: null, detail: "下载后可进行本机运行校验" },
  checks: [
    { label: "系统与架构", passed: true, detail: "Windows x64 · CPU 推理" },
    { label: "可用内存", passed: true, detail: "满足本模型的应用预估内存预算" },
    { label: "存储空间", passed: true, detail: "空间充足" }
  ]
}
let data = {
  models: [
    {
      id: "minicpm-ocr-1.0",
      name: "MiniCPM-OCR 1.0",
      publisher: "OpenBMB / 面壁智能",
      description: "中英文文档识别，支持文字、表格、公式和图表。",
      downloadBytes: 1430124352,
      downloadedBytes: 0,
      percent: 0,
      files: [{ name: "ocr-model.gguf", bytes: 1430124352, localBytes: 0 }],
      sources,
      capabilities: ["ocr"],
      phase: "not-downloaded",
      usedBytes: 0,
      compatibility
    }
  ],
  usedBytes: 0,
  installedCount: 0,
  storagePath: "test/workspace/local-ai/models"
}
const response = () => structuredClone(data)
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
    if (name === "@/utils/message")
      return {
        createMessage: { error: (message) => messages.push(message) }
      }
    if (name === "@/utils/formatters")
      return { formatDateTime: (value) => String(value) }
    if (name === "lucide-vue-next") return new Proxy({}, { get: () => stub })
    if (name === "@/components/BaseModal.vue")
      return { __esModule: true, default: stub }
    if (name === "@/api/request")
      return {
        subscribeReady: async (event, callback) => {
          assert.equal(event, "local-models:changed")
          if (deferSubscription)
            await new Promise((resolve) => {
              deferSubscription = resolve
            })
          listeners.add(callback)
          return () => listeners.delete(callback)
        }
      }
    if (name === "@/api/modules/local-models")
      return {
        localModelsApi: {
          list: async () =>
            deferReads
              ? new Promise((resolve, reject) =>
                  reads.push({ resolve, reject })
                )
              : response(),
          validate: async (id) => {
            calls.push({ action: "validate", id })
            const passed = failAction !== "validate"
            const result = {
              passed,
              detail: passed ? "本机运行校验通过" : "内存分配失败",
              checkedAt: 2
            }
            data.models.find(model => model.id === id).compatibility.validation = result
            return result
          },
          download: async (payload) => {
            calls.push({ action: "download", payload })
            if (failAction === "download") throw new Error("下载源连接失败")
            data.models.find(model => model.id === payload.id).phase = "downloading"
          },
          cancel: async (id) => {
            calls.push({ action: "cancel", id })
            data.models.find(model => model.id === id).phase = "paused"
            data.usedBytes = 10
            data.models.find(model => model.id === id).usedBytes = 10
          },
          delete: async (id) => {
            calls.push({ action: "delete", id })
            if (failAction === "delete") throw new Error("文件被占用")
            data.usedBytes = 0
            data.installedCount = 0
            const index = data.models.findIndex(model => model.id === id)
            data.models[index] = {
              ...data.models[index],
              id,
              phase: "not-downloaded",
              usedBytes: 0,
              compatibility
            }
          }
        }
      }
    return require(name)
  },
  module,
  module.exports
)
const viewModule = { exports: {} }
new Function(
  "require",
  "module",
  "exports",
  transformSync(template.code, { format: "cjs" }).code
)(require, viewModule, viewModule.exports)

function downloadButtons(state, text = [], elements = []) {
  const buttons = []
  function visit(node) {
    if (Array.isArray(node)) return node.forEach(visit)
    if (typeof node === "string") {
      text.push(node)
      return
    }
    if (!node || typeof node !== "object") return
    if (node.type) elements.push(node)
    if (
      node.type === "button" &&
      node.props?.class?.includes("primary-button")
    ) {
      buttons.push(node.props)
    }
    visit(node.children)
    if (typeof node.children?.default === "function")
      visit(node.children.default())
    if (typeof node.children?.header === "function")
      visit(node.children.header())
  }
  const app = renderer.createApp({
    render() {
      visit(viewModule.exports.render({}, [], {}, state))
      return null
    }
  })
  app.mount({})
  app.unmount()
  return buttons
}

// 用真实 Vue 生命周期验证异步竞争、动作和监听清理，文件传输另由 Rust 小文件测试覆盖。
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
function mount() {
  const app = renderer.createApp({
    ...module.exports.default,
    render: () => null
  })
  app.mount({})
  return { app, state: app._instance.setupState }
}
async function tick() {
  for (let index = 0; index < 8; index++) {
    await Promise.resolve()
    await vue.nextTick()
  }
}
async function emitChanged() {
  await Promise.all([...listeners].map((callback) => callback()))
  await tick()
}

const first = mount()
assert.equal(listeners.size, 0, "未打开模型列表时不订阅进度")
await first.state.openManager()
await first.state.openManager()
assert.equal(listeners.size, 1, "重复点击入口不能创建重复监听")
assert.equal(first.state.ready, true)
assert.equal(first.state.snapshot.usedBytes, 0)
assert.equal(first.state.formatBytes(1430124352), "1.33 GiB")
assert.equal(first.state.formatBytes(0), "0 B")

// 首次下载只依赖资源检查，不要求本地已有权重或成功运行记录。
const initialText = []
const initialElements = []
assert.equal(downloadButtons(first.state, initialText, initialElements)[0].disabled, false)
assert.ok(!initialText.includes("下载前资源检查通过"), "默认只显示模型摘要，检测详情收起")
assert.ok(initialText.includes("MiniCPM-OCR 1.0"))
assert.ok(initialText.includes("1.33 GiB"))
assert.ok(!initialElements.some(node => node.props?.class === "model-details"))
assert.ok(!initialElements.some(node => node.props?.class === "manager-toolbar"), "统计不再单独占用工具栏")
const heading = initialElements.find(node => node.props?.class === "manager-heading")
assert.deepEqual(heading.children.map(node => node.props?.class), ["manager-title", "storage-summary"])
const headerActions = initialElements.find(node => node.props?.class === "manager-header-actions")
assert.deepEqual(headerActions.children.map(node => node.props?.class), ["model-button", "manager-close"])
const toggle = initialElements.find(node => node.props?.class === "model-toggle")
assert.equal(toggle.props["aria-expanded"], false)
toggle.props.onClick()
const expandedText = []
const expandedElements = []
downloadButtons(first.state, expandedText, expandedElements)
assert.ok(expandedText.includes("下载前资源检查通过"))
assert.ok(expandedText.includes("ocr-model.gguf"))
assert.equal(expandedElements.find(node => node.props?.class === "model-toggle").props["aria-expanded"], true)
const sourceSelect = expandedElements.find(node => node.type === "select")
sourceSelect.props.onChange({ target: { value: "huggingface" } })
assert.equal(first.state.sources["minicpm-ocr-1.0"], "huggingface")
assert.equal(first.state.expandedModels["minicpm-ocr-1.0"], true, "选择下载源不能收起详情")
delete first.state.sources["minicpm-ocr-1.0"]
await emitChanged()
assert.equal(first.state.expandedModels["minicpm-ocr-1.0"], true, "进度刷新应保留展开状态")
toggle.props.onClick()
assert.equal(first.state.expandedModels["minicpm-ocr-1.0"], false)
assert.equal(downloadButtons(first.state)[0].disabled, false, "收起后仍能下载")
assert.equal(
  first.state.snapshot.models[0].compatibility.validation.passed,
  null
)
assert.equal(calls.filter((item) => item.action === "validate").length, 0)
assert.ok(!initialText.includes("符合使用条件"))

// 硬件条件不通过或检测结果缺失时，点击下载也不能发出请求。
data.models[0].compatibility = {
  allowed: false,
  validation: { passed: true, detail: "上次运行校验通过" },
  checks: [{ label: "可用内存", passed: false, detail: "当前可用内存不足" }]
}
await emitChanged()
const callsBeforeBlocked = calls.length
// 同一台电脑的模型列表允许出现不同检测结果，按钮必须逐个按对应模型判断。
const perModelButtons = downloadButtons({
  ...first.state,
  snapshot: {
    ...first.state.snapshot,
    models: [
      first.state.snapshot.models[0],
      { ...first.state.snapshot.models[0], id: "other-model", compatibility }
    ]
  }
})
assert.deepEqual(
  perModelButtons.map((button) => button.disabled),
  [true, false]
)
await first.state.perform("download", first.state.snapshot.models[0])
assert.equal(calls.length, callsBeforeBlocked)
delete data.models[0].compatibility
await emitChanged()
await first.state.perform("download", first.state.snapshot.models[0])
assert.equal(calls.length, callsBeforeBlocked)
data.models[0].compatibility = compatibility
await first.state.load(true)
await first.state.perform("download", first.state.snapshot.models[0])
assert.equal(first.state.snapshot.models[0].phase, "downloading")
assert.deepEqual(calls.at(-1), {
  action: "download",
  payload: { id: "minicpm-ocr-1.0", source: "modelscope" }
})
first.state.closeManager()
assert.deepEqual(first.state.expandedModels, {}, "关闭后再次打开默认显示收起的列表")
assert.equal(
  calls.filter((item) => item.action === "cancel").length,
  0,
  "关闭列表不能取消后台下载"
)
data.usedBytes = 10
data.models[0].compatibility = { ...compatibility, allowed: false }
await first.state.openManager()
assert.equal(first.state.snapshot.usedBytes, 10)
await first.state.perform("cancel", first.state.snapshot.models[0])
assert.equal(first.state.snapshot.models[0].phase, "paused")
assert.equal(calls.at(-1).action, "cancel", "条件不足时仍能暂停下载")
first.state.sources["minicpm-ocr-1.0"] = "huggingface"
data.models[0].compatibility = { ...compatibility, allowed: false }
await emitChanged()
const callsBeforeResume = calls.length
await first.state.perform("download", first.state.snapshot.models[0])
assert.equal(calls.length, callsBeforeResume, "继续下载同样需要满足运行条件")
data.models[0].compatibility = compatibility
await first.state.load(true)
await first.state.perform("download", first.state.snapshot.models[0])
assert.equal(calls.at(-1).payload.source, "huggingface")

data.models[0].phase = "error"
data.models[0].error = "下载源拒绝请求（HTTP 403），请重试或切换下载源"
await emitChanged()
await emitChanged()
assert.equal(
  messages.filter((message) => message.includes("HTTP 403")).length,
  1,
  "后台失败使用消息提示，重复刷新不能反复弹出"
)
await first.state.perform("download", first.state.snapshot.models[0])
data.models[0].phase = "error"
await emitChanged()
assert.equal(
  messages.filter((message) => message.includes("HTTP 403")).length,
  2,
  "重新下载后相同的失败也需要提示"
)

data.installedCount = 1
data.models[0].phase = "downloaded"
await emitChanged()
assert.equal(first.state.snapshot.installedCount, 1)
failAction = "validate"
await first.state.perform("validate", first.state.snapshot.models[0])
assert.equal(first.state.snapshot.models[0].compatibility.allowed, true)
assert.equal(
  first.state.snapshot.models[0].compatibility.validation.passed,
  false
)
assert.match(messages.at(-1), /内存分配失败/, "运行校验失败必须展示实际原因")
failAction = ""
await first.state.perform("validate", first.state.snapshot.models[0])
assert.equal(
  first.state.snapshot.models[0].compatibility.validation.passed,
  true
)
assert.equal(first.state.snapshot.models[0].compatibility.allowed, true)
failAction = "delete"
first.state.deleteId = data.models[0].id
await first.state.perform("delete", first.state.snapshot.models[0])
assert.match(first.state.error, /文件被占用/)
assert.match(messages.at(-1), /文件被占用/, "删除失败必须使用统一消息提示")
assert.equal(
  first.state.snapshot.installedCount,
  1,
  "删除失败必须保留已下载状态"
)
assert.equal(
  first.state.deleteId,
  data.models[0].id,
  "失败后保留确认入口供重试"
)
failAction = ""
data.models[0].compatibility = { ...compatibility, allowed: false }
await emitChanged()
await first.state.perform("delete", first.state.snapshot.models[0])
assert.equal(first.state.snapshot.usedBytes, 0)
assert.equal(first.state.snapshot.installedCount, 0)
assert.equal(first.state.deleteId, "")

// 两个模型分别使用自己的下载源，翻译模型的动作不能改动 OCR 卡片。
data.models.push({
  id: "lmt-60-0.6b", name: "LMT-60-0.6B", capabilities: ["translation"],
  sources: [{ id: "hf-mirror", name: "HF Mirror（社区量化镜像）" }, { id: "huggingface", name: "Hugging Face（社区量化）" }],
  phase: "not-downloaded", usedBytes: 0, compatibility
})
await emitChanged()
assert.equal(first.state.snapshot.models.length, 2)
const modelElements = []
downloadButtons(first.state, [], modelElements)
const modelToggles = modelElements.filter(node => node.props?.class === "model-toggle")
modelToggles[1].props.onClick()
assert.equal(first.state.expandedModels["lmt-60-0.6b"], true)
assert.equal(Boolean(first.state.expandedModels["minicpm-ocr-1.0"]), false, "展开一个模型不影响另一个")
modelToggles[0].props.onClick()
assert.equal(first.state.expandedModels["minicpm-ocr-1.0"], true)
modelToggles[1].props.onClick()
assert.equal(first.state.expandedModels["lmt-60-0.6b"], false)
assert.equal(first.state.expandedModels["minicpm-ocr-1.0"], true)
await first.state.perform("download", first.state.snapshot.models[1])
assert.deepEqual(calls.at(-1).payload, { id: "lmt-60-0.6b", source: "hf-mirror" })
assert.equal(data.models[0].phase, "not-downloaded")
first.state.sources["lmt-60-0.6b"] = "huggingface"
await first.state.perform("cancel", first.state.snapshot.models[1])
await first.state.perform("download", first.state.snapshot.models[1])
assert.equal(calls.at(-1).payload.source, "huggingface")
assert.equal(first.state.sources["minicpm-ocr-1.0"], "huggingface")
await first.state.perform("cancel", first.state.snapshot.models[1])
await first.state.perform("delete", first.state.snapshot.models[1])
assert.equal(data.models[0].phase, "not-downloaded")

deferReads = true
const old = first.state.load()
const latest = first.state.load()
reads[1].resolve({ ...response(), usedBytes: 30 })
await latest
reads[0].resolve({ ...response(), usedBytes: 5 })
await old
reads.length = 0
assert.equal(first.state.snapshot.usedBytes, 30, "较早返回的读取不能覆盖新进度")
const closedRead = first.state.load()
first.state.closeManager()
reads.shift().resolve({ ...response(), usedBytes: 90 })
await closedRead
assert.equal(first.state.snapshot.usedBytes, 30, "关闭后的读取不能更新列表")
first.app.unmount()
assert.equal(listeners.size, 0)

const retry = mount()
const opening = retry.state.openManager()
await tick()
reads.shift().reject(new Error("统计读取失败"))
await opening
assert.equal(retry.state.ready, false)
assert.match(retry.state.error, /统计读取失败/)
assert.match(messages.at(-1), /统计读取失败/, "读取失败必须使用统一消息提示")
deferReads = false
await retry.state.load(true)
assert.equal(retry.state.ready, true)
assert.equal(retry.state.error, "")
retry.app.unmount()

deferSubscription = true
const disposed = mount()
const subscription = disposed.state.openManager()
await tick()
disposed.app.unmount()
deferSubscription()
await subscription
assert.equal(listeners.size, 0, "卸载期间才注册成功的监听也必须清理")
console.log(
  "本地模型使用条件、下载拦截、下载源、暂停续传、删除失败及重试、统计刷新、异步竞争和监听清理验证通过"
)

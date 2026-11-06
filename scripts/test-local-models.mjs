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
let data = {
  models: [{ id: "minicpm-ocr-1.0", phase: "not-downloaded", usedBytes: 0 }],
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
          download: async (payload) => {
            calls.push({ action: "download", payload })
            if (failAction === "download") throw new Error("下载源连接失败")
            data.models[0].phase = "downloading"
          },
          cancel: async (id) => {
            calls.push({ action: "cancel", id })
            data.models[0].phase = "paused"
            data.usedBytes = 10
            data.models[0].usedBytes = 10
          },
          delete: async (id) => {
            calls.push({ action: "delete", id })
            if (failAction === "delete") throw new Error("文件被占用")
            data.usedBytes = 0
            data.installedCount = 0
            data.models[0] = { id, phase: "not-downloaded", usedBytes: 0 }
          }
        }
      }
    return require(name)
  },
  module,
  module.exports
)

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

await first.state.perform("download", first.state.snapshot.models[0])
assert.equal(first.state.snapshot.models[0].phase, "downloading")
assert.deepEqual(calls.at(-1), {
  action: "download",
  payload: { id: "minicpm-ocr-1.0", source: "modelscope" }
})
first.state.closeManager()
assert.equal(
  calls.filter((item) => item.action === "cancel").length,
  0,
  "关闭列表不能取消后台下载"
)
data.usedBytes = 10
await first.state.openManager()
assert.equal(first.state.snapshot.usedBytes, 10)
await first.state.perform("cancel", first.state.snapshot.models[0])
assert.equal(first.state.snapshot.models[0].phase, "paused")
first.state.source = "huggingface"
await first.state.perform("download", first.state.snapshot.models[0])
assert.equal(calls.at(-1).payload.source, "huggingface")

data.models[0].phase = "error"
data.models[0].error = "下载源拒绝请求（HTTP 403），请重试或切换下载源"
await emitChanged()
await emitChanged()
assert.equal(messages.filter(message => message.includes("HTTP 403")).length, 1, "后台失败使用消息提示，重复刷新不能反复弹出")
await first.state.perform("download", first.state.snapshot.models[0])
data.models[0].phase = "error"
await emitChanged()
assert.equal(messages.filter(message => message.includes("HTTP 403")).length, 2, "重新下载后相同的失败也需要提示")

data.installedCount = 1
data.models[0].phase = "downloaded"
await emitChanged()
assert.equal(first.state.snapshot.installedCount, 1)
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
await first.state.perform("delete", first.state.snapshot.models[0])
assert.equal(first.state.snapshot.usedBytes, 0)
assert.equal(first.state.snapshot.installedCount, 0)
assert.equal(first.state.deleteId, "")

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
  "本地模型列表、下载源、暂停续传、删除失败及重试、统计刷新、异步竞争和监听清理验证通过"
)

import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { parse, compileScript, compileTemplate, compileStyleAsync } from "@vue/compiler-sfc"
import { transformSync } from "esbuild"
import * as vue from "vue"

const require = createRequire(import.meta.url)
const filename = "src/features/screenshot/components/ScreenshotWindow/components/OcrResultPanel.vue"
const { descriptor, errors } = parse(readFileSync(filename, "utf8"), { filename })
assert.deepEqual(errors, [])
const script = compileScript(descriptor, { id: filename, genDefaultAs: "__component" })
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: filename,
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
for (const style of descriptor.styles) {
  const compiled = await compileStyleAsync({
    source: style.content, filename, id: filename, scoped: true, preprocessLang: style.lang
  })
  assert.deepEqual(compiled.errors, [])
}

const calls = []
const listeners = new Map()
let finishRecognition
globalThis.Document = class {}
globalThis.ShadowRoot = class {}
globalThis.window = {
  document: { createElement: () => ({ getContext: () => ({
    measureText: text => ({ width: text.length * 10 })
  }) }) },
  addEventListener: (name, callback) => listeners.set(name, callback),
  removeEventListener: name => listeners.delete(name)
}
globalThis.FileReader = class {
  readAsDataURL() {
    this.result = "data:image/png;base64,test"
    this.onload()
  }
}
const module = { exports: {} }
new Function("require", "module", "exports", transformSync(
  `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
  { format: "cjs" }
).code)(
  name => {
    if (name === "vue") return vue
    if (name === "@/api") return { translationApi: { desktopStatus: async () => ({ enabled: false }) } }
    if (name === "@/api/modules/screenshot") return {
      screenshotApi: {
        image: async id => {
          assert.equal(id, "bound-pin")
          return new Uint8Array([1])
        },
        call: async (action, payload) => {
          calls.push({ action, payload })
          if (action === "ocr-result") return { documentId: "bound-pin" }
          if (action === "status") return { ocr: { useLargeModel: true } }
          if (action === "ocr") return new Promise(resolve => { finishRecognition = resolve })
        }
      }
    }
    return require(name)
  },
  module,
  module.exports
)

// 挂载真实模板，验证收起按钮、头部内容与异步识别期间的状态保留。
const node = type => ({
  type, tagName: type.toUpperCase(), props: {}, children: [], text: "", parent: null,
  addEventListener() {},
  getRootNode: () => window.document,
  async decode() {}
})
const renderer = vue.createRenderer({
  createElement: node,
  createText: text => ({ ...node("#text"), text }),
  createComment: () => node("#comment"),
  setText: (element, text) => { element.text = text },
  setElementText: (element, text) => { element.text = text; element.children = [] },
  patchProp: (element, key, previous, value) => { element.props[key] = value },
  insert(element, parent, anchor) {
    if (element.parent) element.parent.children.splice(element.parent.children.indexOf(element), 1)
    const index = anchor ? parent.children.indexOf(anchor) : -1
    parent.children.splice(index < 0 ? parent.children.length : index, 0, element)
    element.parent = parent
  },
  remove(element) {
    if (element.parent) element.parent.children.splice(element.parent.children.indexOf(element), 1)
    element.parent = null
  },
  parentNode: element => element.parent,
  nextSibling: element => element.parent?.children[element.parent.children.indexOf(element) + 1] || null
})
function find(element, predicate) {
  if (predicate(element)) return element
  for (const child of element.children) {
    const found = find(child, predicate)
    if (found) return found
  }
}
function text(element) {
  return element.text + element.children.map(text).join("")
}
async function tick() {
  for (let index = 0; index < 16; index++) {
    await Promise.resolve()
    await vue.nextTick()
  }
}
const root = node("root")
const failures = []
const app = renderer.createApp(module.exports.default, { resultId: "bound-result" })
app.config.errorHandler = error => failures.push(error)
app.mount(root)
await tick()
const state = app._instance.setupState
assert.equal(state.loading, true)
assert.equal(typeof finishRecognition, "function")
const header = () => find(root, element => element.props.class === "ocr-result-head")
assert.equal(text(header()).replace(/\s/g, ""), "识别结果−×")
const minimize = find(root, element => element.props["aria-label"] === "收起识别结果")
await minimize.props.onClick()
assert.deepEqual(calls.at(-1), {
  action: "ocr-result-action", payload: { id: "bound-result", action: "minimize" }
})
assert.equal(state.loading, true)
finishRecognition({ text: "Hello OCR 12345", engine: "不应显示在头部的模型名称".repeat(50) })
await tick()
assert.equal(state.loading, false)
assert.equal(state.result.text, "Hello OCR 12345")
assert.equal(text(header()).replace(/\s/g, ""), "识别结果−×")
assert.equal(calls.find(call => call.payload?.action === "resize").payload.width, 420)
state.result.text = "已编辑的识别内容"
await minimize.props.onClick()
assert.equal(state.result.text, "已编辑的识别内容")
assert.equal(calls.filter(call => call.action === "ocr").length, 1)
await find(root, element => element.props["aria-label"] === "关闭识别结果").props.onClick()
assert.equal(calls.at(-1).payload.action, "close")
app.unmount()
assert.equal(listeners.size, 0)
assert.deepEqual(failures, [])

// 真实贴图模板验证恢复入口，同时覆盖旧元数据晚于收起事件返回的竞争。
globalThis.location = { search: "?mode=pin&id=bound-pin" }
const pinFilename = "src/features/screenshot/components/ScreenshotWindow/index.vue"
const pinDescriptor = parse(readFileSync(pinFilename, "utf8"), { filename: pinFilename }).descriptor
const pinScript = compileScript(pinDescriptor, { id: pinFilename, genDefaultAs: "__component" })
const pinTemplate = compileTemplate({
  source: pinDescriptor.template.content, filename: pinFilename, id: pinFilename,
  compilerOptions: { bindingMetadata: pinScript.bindings }
})
assert.deepEqual(pinTemplate.errors, [])
const pinModule = { exports: {} }
const pinListeners = new Map()
const pinCalls = []
let finishDocument
new Function("require", "module", "exports", transformSync(
  `${pinScript.content}\n${pinTemplate.code}\n__component.render = render\nexport default __component`,
  { format: "cjs" }
).code)(
  name => {
    if (name === "vue") return vue
    if (name.endsWith(".vue")) return { __esModule: true, default: { render: () => null } }
    if (name === "@/api/modules/system") return { systemApi: {} }
    if (name === "../../renderer" || name === "../../long-screenshot") return {}
    if (name === "../../rectangle-detector") return { disposeRectangleDetector() {} }
    if (name === "@/api/request") return {
      subscribeReady: async (event, callback) => {
        pinListeners.set(event, callback)
        return () => pinListeners.delete(event)
      }
    }
    if (name === "@/api/modules/screenshot") return {
      screenshotApi: {
        image: async () => new Uint8Array([1]),
        call: async (action, payload) => {
          pinCalls.push({ action, payload })
          if (action === "document") return new Promise(resolve => { finishDocument = resolve })
          if (action === "pin-show") return { visible: false }
          if (payload?.action === "restore") pinListeners.get("screenshot:ocr-result")({
            documentId: "bound-pin", result: { id: "bound-result", collapsed: false }
          })
        }
      }
    }
    return require(name)
  },
  pinModule,
  pinModule.exports
)
const pinRoot = node("root")
const pinApp = renderer.createApp(pinModule.exports.default)
pinApp.config.errorHandler = error => failures.push(error)
pinApp.mount(pinRoot)
await tick()
const notifyPin = pinListeners.get("screenshot:ocr-result")
assert.equal(typeof finishDocument, "function")
notifyPin({ documentId: "bound-pin", result: { id: "bound-result", collapsed: true } })
finishDocument({ pin: { width: 800, height: 600 }, ocrResult: null })
await tick()
const restore = find(pinRoot, element => element.props["aria-label"] === "显示识别结果")
assert.ok(restore, "旧文档读取不能覆盖收起事件中的恢复入口")
notifyPin({ documentId: "other-pin", result: null })
assert.equal(pinApp._instance.setupState.ocrResult.id, "bound-result")
await restore.props.onClick()
await tick()
assert.deepEqual(pinCalls.at(-1), {
  action: "ocr-result-action", payload: { id: "bound-result", action: "restore" }
})
assert.equal(find(pinRoot, element => element.props["aria-label"] === "显示识别结果"), undefined)
pinApp.unmount()
assert.equal(pinListeners.size, 0)
assert.equal(listeners.size, 0)
assert.deepEqual(failures, [])
console.log("OCR 头部、收起与唤回、识别和编辑状态保留、贴图绑定及异步事件竞争验证通过")

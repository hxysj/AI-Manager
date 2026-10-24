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
import {
  canvasSource,
  thumbnailSource,
  selectionRect,
  renderDocument
} from "../src/features/screenshot/renderer.js"

const require = createRequire(import.meta.url)
// 贴图和编辑原图使用 Blob 地址，实际入口的安全策略也必须允许加载。
const imageSources = readFileSync("index.html", "utf8")
  .match(/\bimg-src\s+([^;"\n]+)/)?.[1]
  .split(/\s+/)
assert.ok(imageSources?.includes("blob:"), "页面安全策略必须允许贴图 Blob 图片")
assert.match(
  readFileSync("index.html", "utf8"),
  /script-src[^;]+\bwasm-unsafe-eval\b/,
  "OCR 的 WASM 编译必须被页面安全策略允许"
)
const filename = "src/features/screenshot/components/ScreenshotWindow.vue"
const { descriptor, errors } = parse(readFileSync(filename, "utf8"), {
  filename
})
assert.deepEqual(errors, [])
const script = compileScript(descriptor, {
  id: "screenshot-window",
  genDefaultAs: "__component"
})
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: "screenshot-window",
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
for (const style of descriptor.styles) {
  const result = await compileStyleAsync({
    source: style.content,
    filename,
    id: "screenshot-window",
    scoped: true,
    preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}
const requests = []
const listeners = new Map()
let captureFrame = null
let saveFailure = false
let captureLong = false
let pendingSmartSelection = null
let pendingLongCapture = null
let pendingPinMenuAction = null
const imageRequests = []
const imageUrls = new Set()
const imageBlobs = new Map()
const createObjectURL = URL.createObjectURL.bind(URL)
const revokeObjectURL = URL.revokeObjectURL.bind(URL)
URL.createObjectURL = (blob) => {
  const url = createObjectURL(blob)
  imageUrls.add(url)
  imageBlobs.set(url, blob)
  return url
}
URL.revokeObjectURL = (url) => {
  imageUrls.delete(url)
  imageBlobs.delete(url)
  revokeObjectURL(url)
}
// 与原生二进制响应保持一致，验证框选直接使用 RGBA 而不加载图片。
function frameResponse() {
  if (!captureFrame) return new ArrayBuffer(0)
  const { left, top, width, height, autoCopy } = captureFrame
  const bytes = new ArrayBuffer(20 + width * height * 4)
  const header = new DataView(bytes)
  header.setInt32(0, left, true)
  header.setInt32(4, top, true)
  header.setUint32(8, width, true)
  header.setUint32(12, height, true)
  header.setUint32(16, Number(autoCopy === true), true)
  new Uint8ClampedArray(bytes, 20, 4).set([10, 20, 30, 255])
  return bytes
}
// 使用真实 PNG 和 API 转换代码覆盖 Tauri 的两种 IPC 返回格式。
const pngBytes = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAFElEQVR4AQEJAPb/AABaZAA8WmRkCAECHYDE5XcAAAAASUVORK5CYII=",
  "base64"
)
let nativeImageResponse = null
let nativeFrameResponse = null
const apiModule = { exports: {} }
new Function(
  "require",
  "module",
  "exports",
  transformSync(readFileSync("src/api/modules/screenshot.js", "utf8"), {
    format: "cjs"
  }).code
)(
  (name) => {
    if (name === "@tauri-apps/api/core")
      return {
        invoke: async (command) =>
          command === "screenshot_image"
            ? nativeImageResponse
            : (nativeFrameResponse ?? frameResponse())
      }
    if (name === "@/api/request") return { request() {} }
    return require(name)
  },
  apiModule,
  apiModule.exports
)
const binaryApi = apiModule.exports.screenshotApi
const paddedPng = new Uint8Array(pngBytes.length + 8)
paddedPng.set(pngBytes, 4)
for (const response of [
  Uint8Array.from(pngBytes).buffer,
  Array.from(pngBytes),
  new Uint8Array(paddedPng.buffer, 4, pngBytes.length),
  new DataView(paddedPng.buffer, 4, pngBytes.length)
]) {
  nativeImageResponse = response
  nativeFrameResponse = response
  const image = await binaryApi.image("test-pin", "output")
  assert.deepEqual(Buffer.from(await new Blob([image]).arrayBuffer()), pngBytes)
  assert.deepEqual(Buffer.from(await binaryApi.frame()), pngBytes)
}
nativeImageResponse = { invalid: true }
await assert.rejects(binaryApi.image("test-pin", "output"), /数据格式无效/)
nativeImageResponse = Array.from(pngBytes)
nativeFrameResponse = null
const module = { exports: {} }
const localRequire = (name) => {
  if (name === "@/api/modules/screenshot")
    return {
      screenshotApi: {
        image: async (id, kind) => {
          imageRequests.push({ id, kind })
          return binaryApi.image(id, kind)
        },
        frame: async () => {
          requests.push({ action: "frame" })
          return binaryApi.frame()
        },
        call: async (action, payload) => {
          requests.push({ action, payload })
          if (
            action === "pin-action" &&
            payload.action.startsWith("context-menu-") &&
            pendingPinMenuAction
          )
            return pendingPinMenuAction(payload)
          if (action === "capture-mode") return { long: captureLong }
          if (action === "smart-selection")
            return pendingSmartSelection
              ? pendingSmartSelection(payload)
              : { regions: [] }
          if (action === "document") {
            assert.equal(payload.metadataOnly, true)
            return {
              annotations: [],
              width: 800,
              height: 600,
              pin: { width: 800, height: 600 }
            }
          }
          if (action === "save-document") {
            if (saveFailure) throw new Error("保存失败")
            return "saved-pin"
          }
          if (action === "pin-show")
            return { visible: true, width: 1600, height: 1200 }
          if (action === "cancel") listeners.get("screenshot:reset")?.()
          // 原生贴图首帧就绪后才结束框选，模拟对应的重置通知。
          if (action === "pin" && payload?.capture)
            listeners.get("screenshot:reset")?.()
        }
      }
    }
  if (name === "@/api/request")
    return {
      subscribeReady: async (event, handler) => {
        listeners.set(event, handler)
        return () => listeners.delete(event)
      }
    }
  if (name === "@/api/modules/system")
    return { systemApi: { selectFile: async () => null } }
  if (name === "./ImageEditor.vue")
    return {
      __esModule: true,
      default: { render: () => vue.h("div", { class: "editor" }) }
    }
  if (name === "../renderer")
    return {
      loadImage: async () => ({ naturalWidth: 1600, naturalHeight: 1200 }),
      canvasSource,
      thumbnailSource,
      selectionRect,
      detectPageRegions: () => []
    }
  if (name === "../ocr")
    return {
      recognizePpocr: async () => ({
        text: "识别成功",
        lines: [],
        engine: "测试"
      })
    }
  if (name === "../long-screenshot")
    return { captureLongScreenshot: (options) => pendingLongCapture(options) }
  return require(name)
}
new Function(
  "require",
  "module",
  "exports",
  transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
    { format: "cjs" }
  ).code
)(localRequire, module, module.exports)

globalThis.location = { search: "?mode=pin&id=test-pin" }
globalThis.innerWidth = 800
globalThis.innerHeight = 600
globalThis.devicePixelRatio = 2
let animationFrameId = 0
const animationFrames = new Map()
globalThis.requestAnimationFrame = (callback) => {
  const id = ++animationFrameId
  animationFrames.set(id, callback)
  return id
}
globalThis.cancelAnimationFrame = (id) => animationFrames.delete(id)
globalThis.Document = class {}
globalThis.ShadowRoot = class {}
const copiedColors = []
// 记录复制内容，验证切换格式后复制的文本与取色面板一致。
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { clipboard: { writeText: async (value) => copiedColors.push(value) } }
})
globalThis.ImageData = class {
  constructor(data, width, height) {
    this.data = data
    this.width = width
    this.height = height
  }
}
// 模拟浏览器异步 PNG 编码，覆盖大图贴图按钮的非阻塞路径。
globalThis.FileReader = class {
  readAsDataURL(blob) {
    this.result = blob.data
    this.onload?.()
  }
}
const context = () => ({
  drawImage() {},
  clearRect() {},
  strokeRect() {},
  getImageData: () => ({ data: [10, 20, 30, 255] })
})
globalThis.window = {
  setTimeout,
  clearTimeout,
  addEventListener() {},
  removeEventListener() {},
  confirm: () => true,
  document: {
    createElement: () => ({
      width: 0,
      height: 0,
      getContext: context,
      toDataURL: () => {
        throw new Error("截图不应同步编码")
      },
      toBlob: (callback) => callback({ data: "crop.png" })
    })
  }
}
globalThis.document = window.document
const node = (type) => {
  const element = {
    type,
    props: {},
    style: {},
    children: [],
    parent: null,
    offsetWidth: 220,
    offsetHeight: 300,
    async decode() {
      const blob = imageBlobs.get(this.props.src)
      assert.ok(blob, "贴图解码必须使用仍有效的图片 URL")
      assert.deepEqual(Buffer.from(await blob.arrayBuffer()), pngBytes)
    },
    setPointerCapture() {},
    hasPointerCapture: () => false,
    releasePointerCapture() {},
    getContext: () => canvasContext,
    toDataURL: () => "edited.png",
    toBlob: (callback) => callback({ data: "edited.png" }),
    focus() {
      document.activeElement = this
    },
    addEventListener() {},
    removeEventListener() {},
    getRootNode: () => ({}),
    get options() {
      return this.children.filter((child) => child.type === "option")
    },
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 })
  }
  // 模拟 Vue 修改画布尺寸会清空内容，捕获首帧被下一次渲染抹掉的回归。
  const canvasContext = {
    ...context(),
    putImageData(image) {
      element.pixels = image
    },
    clearRect() {
      element.pixels = null
    }
  }
  return element
}
const teleportTarget = node("body")
const renderer = vue.createRenderer({
  querySelector: () => teleportTarget,
  createElement: node,
  createText: () => node("text"),
  createComment: () => node("comment"),
  setText() {},
  setElementText() {},
  parentNode: (element) => element.parent,
  nextSibling: (element) =>
    element.parent?.children[element.parent.children.indexOf(element) + 1] ||
    null,
  patchProp: (element, key, previous, value) => {
    element.props[key] = value
    if (key === "width" || key === "height") {
      element[key] = Number(value)
      element.pixels = null
    }
  },
  insert(element, parent, anchor) {
    if (element.parent)
      element.parent.children.splice(
        element.parent.children.indexOf(element),
        1
      )
    element.parent = parent
    const index = anchor ? parent.children.indexOf(anchor) : -1
    parent.children.splice(
      index < 0 ? parent.children.length : index,
      0,
      element
    )
  },
  remove(element) {
    element.parent.children.splice(element.parent.children.indexOf(element), 1)
    element.parent = null
  }
})
const root = node("root")
const app = renderer.createApp(module.exports.default)
const failures = []
app.config.errorHandler = (error) => failures.push(String(error))
globalThis.innerWidth = 400
app.mount(root)
const tick = async () => {
  for (let index = 0; index < 10; index += 1) {
    await vue.nextTick()
    const callbacks = [...animationFrames.values()]
    animationFrames.clear()
    for (const callback of callbacks) callback()
  }
}
await tick()
// 图片解码完成也不能按错误视口露出，实际尺寸同步并绘制后才结束框选。
assert.equal(app._instance.setupState.pinReady, false)
assert.equal(
  requests.some((request) => request.action === "pin-show"),
  true
)
assert.equal(
  requests.some((request) => request.action === "pin-ready"),
  false
)
globalThis.innerWidth = 800
await tick()
assert.equal(app._instance.setupState.pinReady, true)
assert.equal(requests.at(-1).action, "pin-ready")
assert.deepEqual(imageRequests, [{ id: "test-pin", kind: "output" }])
assert.equal(imageUrls.size, 1)
const pin = root.children.find(
  (element) =>
    typeof element.props.class === "string" &&
    element.props.class.includes("pin-window")
)
const pinState = app._instance.setupState
assert.equal(pin.props.tabindex, "0")
// 方向键移动与复制只作用于普通贴图，不接管输入、编辑或菜单操作。
const pinKey = (key, options = {}) => ({
  key,
  target: { tagName: "DIV" },
  preventDefault() {
    this.prevented = true
  },
  ...options
})
for (const [key, direction] of [
  ["ArrowUp", "up"],
  ["ArrowDown", "down"],
  ["ArrowLeft", "left"],
  ["ArrowRight", "right"]
]) {
  const event = pinKey(key)
  await pinState.keydown(event)
  assert.equal(event.prevented, true)
  assert.deepEqual(requests.at(-1).payload, {
    id: "test-pin",
    action: "move",
    direction
  })
}
await pinState.keydown(pinKey("ArrowUp", { repeat: true }))
assert.equal(requests.at(-1).payload.direction, "up")
await pinState.keydown(pinKey("c", { ctrlKey: true }))
assert.deepEqual(requests.at(-1), {
  action: "copy",
  payload: { id: "test-pin" }
})
const afterPinKeys = requests.length
for (const event of [
  pinKey("c", { ctrlKey: true, repeat: true }),
  pinKey("c", { ctrlKey: true, target: { tagName: "TEXTAREA" } }),
  pinKey("ArrowUp", { target: { isContentEditable: true } }),
  pinKey("ArrowUp", { ctrlKey: true }),
  pinKey("ArrowUp", { isComposing: true })
])
  await pinState.keydown(event)
assert.equal(requests.length, afterPinKeys)
const menu = () =>
  pin.children.find((element) => element.props.class === "pin-menu")
const pinImage = pin.children.find(
  (element) => element.props.class === "pin-image"
)
async function openMenu(clientX, clientY) {
  let prevented = false
  await pin.props.onContextmenu({
    clientX,
    clientY,
    preventDefault() {
      prevented = true
    }
  })
  await tick()
  assert.equal(prevented, true)
}
// 原生扩窗先于 API 返回完成，图片与可见状态也必须保持不变。
pendingPinMenuAction = ({ action }) => {
  if (action === "context-menu-open") {
    assert.equal(pinImage.style.width, "800px")
    assert.equal(pinImage.style.height, "600px")
    assert.equal(pin.props.style.opacity, 1)
    globalThis.innerWidth = 1080
    globalThis.innerHeight = 950
    return { width: 1600, height: 1200 }
  }
}
await openMenu(120, 90)
assert.deepEqual(menu().props.style, { left: "120px", top: "90px" })
assert.equal(pinImage.props.style.width, "800px")
assert.equal(pinImage.props.style.height, "600px")
await openMenu(250, 150)
assert.deepEqual(menu().props.style, { left: "250px", top: "150px" })
await openMenu(790, 590)
assert.deepEqual(menu().props.style, { left: "790px", top: "590px" })
await openMenu(0, 0)
assert.deepEqual(menu().props.style, { left: "0px", top: "0px" })
const beforeMenuKey = requests.length
await pinState.keydown(pinKey("ArrowUp"))
assert.equal(requests.length, beforeMenuKey)
// 取消阴影后同时取消边框，重新开启时两者一起恢复。
assert.deepEqual(pinState.pinWindowClass, {
  "has-shadow": true,
  "has-border": true
})
const shadowToggle = menu().children.find(
  (element) => element.props["aria-label"] === "窗口阴影"
)
shadowToggle.props.onClick()
assert.deepEqual(pinState.pinWindowClass, {
  "has-shadow": false,
  "has-border": false
})
shadowToggle.props.onClick()
assert.deepEqual(pinState.pinWindowClass, {
  "has-shadow": true,
  "has-border": true
})
let stopped = false
menu().props.onDblclick({
  stopPropagation() {
    stopped = true
  }
})
assert.equal(stopped, true)
assert.equal(
  requests.some((request) => request.payload?.action === "destroy"),
  false
)
await pin.props.onPointerdown({ button: 0, detail: 1 })
await tick()
assert.equal(Boolean(menu()), false)
// 关闭的 API 已返回，视口尚未缩回时仍保持图片尺寸，避免再次铺满扩展区域。
assert.equal(pinState.menuExpanded, true)
assert.equal(pinImage.props.style.width, "800px")
assert.equal(pinImage.props.style.height, "600px")
globalThis.innerWidth = 800
globalThis.innerHeight = 600
await tick()
assert.equal(pinState.menuExpanded, false)
assert.equal(pinImage.props.style.width, "100%")
assert.equal(pinImage.props.style.height, "100%")
pendingPinMenuAction = null
assert.equal(
  requests.some((request) => request.payload?.action === "drag"),
  false
)
await pin.props.onPointerdown({ button: 0, detail: 2 })
assert.equal(
  requests.some((request) => request.payload?.action === "drag"),
  false
)
await openMenu(80, 80)
// 工具栏样式调整后按动作进入编辑，避免依赖菜单按钮的排列顺序。
const editPin = app._instance.setupState.pinAction("edit")
await tick()
await editPin
const beforeEditorKeys = requests.length
await pinState.keydown(pinKey("ArrowUp"))
await pinState.keydown(pinKey("c", { ctrlKey: true }))
assert.equal(requests.length, beforeEditorKeys)
assert.deepEqual(imageRequests.at(-1), { id: "test-pin", kind: "source" })
assert.equal(imageUrls.size, 2)
await pinState.stopEditing()
await pinState.pinAction("edit")
assert.equal(
  imageRequests.filter((request) => request.kind === "source").length,
  1
)
await pin.props.onDblclick({})
await openMenu(80, 80)
assert.equal(Boolean(menu()), false)
assert.equal(
  requests.some((request) => request.payload?.action === "destroy"),
  false
)
await pinState.loadDocument()
assert.equal(imageUrls.size, 1)
app.unmount()
assert.equal(imageUrls.size, 0)

const secondApp = renderer.createApp(module.exports.default)
secondApp.config.errorHandler = (error) => failures.push(String(error))
secondApp.mount(root)
await tick()
const secondPin = root.children.find(
  (element) =>
    typeof element.props.class === "string" &&
    element.props.class.includes("pin-window")
)
const dragRequests = () =>
  requests.filter((request) => request.payload?.action === "drag").length
const beforeDrag = dragRequests()
await secondPin.props.onPointerdown({
  button: 0,
  detail: 1,
  clientX: 20,
  clientY: 20
})
assert.equal(dragRequests(), beforeDrag)
await secondPin.props.onPointermove({ buttons: 1, clientX: 22, clientY: 21 })
assert.equal(dragRequests(), beforeDrag)
await secondPin.props.onPointermove({ buttons: 1, clientX: 28, clientY: 25 })
assert.equal(dragRequests(), beforeDrag + 1)
await secondPin.props.onPointermove({ buttons: 1, clientX: 35, clientY: 30 })
assert.equal(dragRequests(), beforeDrag + 1)
secondPin.props.onPointerup()
await secondPin.props.onPointerdown({
  button: 0,
  detail: 1,
  clientX: 50,
  clientY: 50
})
secondPin.props.onPointerup()
await secondPin.props.onPointerdown({
  button: 0,
  detail: 2,
  clientX: 50,
  clientY: 50
})
secondPin.props.onPointerup()
assert.equal(dragRequests(), beforeDrag + 1)
await secondPin.props.onDblclick({})
assert.deepEqual(requests.at(-1), {
  action: "pin-action",
  payload: { id: "test-pin", action: "destroy" }
})
assert.equal(
  requests.some((request) => request.action === "delete"),
  false
)
assert.deepEqual(failures, [])
await tick()
const cancelledPaint = secondApp._instance.setupState.waitForPinPaint({
  width: 1,
  height: 1
})
secondApp.unmount()
assert.equal(await cancelledPaint, false)
assert.equal(imageUrls.size, 0)
assert.equal(animationFrames.size, 0)
globalThis.location.search = "?mode=capture"
const captureApp = renderer.createApp(module.exports.default)
captureApp.config.errorHandler = (error) => failures.push(String(error))
captureApp.mount(root)
await tick()
assert.equal(
  requests.some((request) => request.action === "frame"),
  true
)
assert.equal(requests.at(-1).action, "capture-window-ready")
captureFrame = {
  source: "desktop.png",
  left: -1920,
  top: -100,
  width: 1600,
  height: 1200
}
await listeners.get("screenshot:frame")()
await tick()
assert.equal(requests.at(-1).action, "ready")
const state = captureApp._instance.setupState
state.startSelection({ button: 0, pointerId: 1, clientX: 100, clientY: 100 })
await state.movePointer({ clientX: 363.5, clientY: 257 })
await tick()
// 取色面板挂到 body，与工具栏共用层叠上下文。
assert.equal(
  teleportTarget.children.some(
    (element) => element.props.class === "capture-magnifier"
  ),
  true
)
assert.deepEqual(state.selection, { x: 200, y: 200, width: 527, height: 314 })
await state.finishSelection()
await tick()
// 工具栏及边框悬停时隐藏取色；离开后恢复，且不销毁取色画布。
const captureMagnifier = teleportTarget.children.find(
  (element) => element.props.class === "capture-magnifier"
)
const captureEditor = findElement(
  root,
  (element) => element.props.class === "editor"
)
captureEditor.props.onToolbarHover(true)
await tick()
assert.equal(state.magnifierVisible, false)
assert.equal(captureMagnifier.style.display, "none")
captureEditor.props.onToolbarHover(false)
await tick()
assert.equal(state.magnifierVisible, true)
assert.equal(captureMagnifier.style.display, undefined)
const selectionHandle = findElement(root, (element) =>
  element.props.class?.includes("handle-nw")
)
selectionHandle.props.onPointerenter()
assert.equal(state.magnifierVisible, false)
selectionHandle.props.onPointerleave()
assert.equal(state.magnifierVisible, true)
assert.equal(state.colorValue, "10, 20, 30")
const colorKey = (key, modifiers = {}) => ({
  key,
  target: { tagName: "DIV" },
  preventDefault() {},
  ...modifiers
})
await state.keydown(colorKey("c"))
assert.equal(copiedColors.at(-1), "10, 20, 30")
await state.keydown(colorKey("Shift"))
assert.equal(state.colorValue, "#0A141E")
await state.keydown(colorKey("Shift", { repeat: true }))
assert.equal(state.colorValue, "#0A141E")
await state.keydown(colorKey("c"))
assert.equal(copiedColors.at(-1), "#0A141E")
await state.keydown(colorKey("Shift"))
assert.equal(state.colorValue, "10, 20, 30")
const colorCopyCount = copiedColors.length
await state.keydown(colorKey("c", { ctrlKey: true }))
await state.keydown(colorKey("Shift", { target: { tagName: "TEXTAREA" } }))
assert.equal(copiedColors.length, colorCopyCount)
assert.equal(state.colorValue, "10, 20, 30")
await state.movePointer({ clientX: 790, clientY: 590 })
assert.deepEqual(state.magnifierStyle, { left: "582px", top: "342px" })
// 保存失败时保留选区并恢复按钮，允许直接重试。
saveFailure = true
await state.output({
  source: "crop.png",
  output: "crop.png",
  annotations: [],
  action: "pin"
})
assert.equal(state.busy, false)
assert.ok(state.selection)
assert.match(state.error, /保存失败/)
assert.equal(
  requests.findLast((request) => request.action === "save-document").payload
    .output,
  null
)
saveFailure = false
await state.output({
  source: "crop.png",
  output: "annotated.png",
  annotations: [],
  action: "pin"
})
const saved = requests.findLast((request) => request.action === "save-document")
assert.deepEqual(saved.payload.origin, { x: -1720, y: 100 })
assert.equal(
  requests.findLast((request) => request.action === "pin").payload.id,
  "saved-pin"
)
assert.equal(state.frame, null)
assert.equal(state.selection, null)
captureFrame = { ...captureFrame, source: "next-desktop.png", left: 0, top: 0 }
await listeners.get("screenshot:frame")()
await tick()
assert.equal(state.frame.left, 0)
assert.equal(state.colorFormat, "rgb")
assert.equal(state.selection, null)
assert.equal(requests.at(-1).action, "ready")
const captureCanvas = state.captureBackground
assert.deepEqual(
  Array.from(captureCanvas.pixels.data.slice(0, 4)),
  [10, 20, 30, 255]
)
assert.equal(captureCanvas.width, 1600)
assert.equal(captureCanvas.height, 1200)
// 四条完整边线都能拉伸，拖动期间沿用方向光标并保持取色隐藏。
const edgeCases = [
  {
    edge: "n",
    point: { clientX: 200, clientY: 80 },
    expected: { x: 200, y: 160, width: 400, height: 340 }
  },
  {
    edge: "e",
    point: { clientX: 330, clientY: 200 },
    expected: { x: 200, y: 200, width: 460, height: 300 }
  },
  {
    edge: "s",
    point: { clientX: 200, clientY: 280 },
    expected: { x: 200, y: 200, width: 400, height: 360 }
  },
  {
    edge: "w",
    point: { clientX: 80, clientY: 200 },
    expected: { x: 160, y: 200, width: 440, height: 300 }
  }
]
for (const item of edgeCases) {
  state.selection = { x: 200, y: 200, width: 400, height: 300 }
  await tick()
  const edge = findElement(root, (element) =>
    element.props.class?.includes(`edge-${item.edge}`)
  )
  edge.props.onPointerenter()
  assert.equal(state.magnifierVisible, false)
  edge.props.onPointerdown({ pointerId: 3, stopPropagation() {} })
  edge.props.onPointerleave()
  assert.equal(state.magnifierVisible, false)
  await state.movePointer(item.point)
  await tick()
  assert.deepEqual(state.selection, item.expected)
  assert.equal(state.desktop.props.style.cursor, state.resizeCursors[item.edge])
  await state.finishSelection()
  await tick()
  assert.equal(state.magnifierVisible, true)
}
// 异步编码不能把已取消的截图带回来，也不能覆盖后一次框选结果。
const createCanvas = document.createElement
const cropCallbacks = []
document.createElement = () => ({
  ...createCanvas(),
  toBlob: (callback) => cropCallbacks.push(callback)
})
state.cancelDrag()
state.startSelection({ button: 0, pointerId: 4, clientX: 20, clientY: 30 })
await state.movePointer({ clientX: 100, clientY: 90 })
const oldCrop = state.finishSelection()
state.cancelDrag()
state.startSelection({ button: 0, pointerId: 5, clientX: 30, clientY: 40 })
await state.movePointer({ clientX: 120, clientY: 100 })
const newCrop = state.finishSelection()
cropCallbacks[1]({ data: "new-crop.png" })
await newCrop
cropCallbacks[0]({ data: "old-crop.png" })
await oldCrop
assert.equal(state.editorSource, "new-crop.png")
assert.deepEqual(state.selection, { x: 60, y: 80, width: 180, height: 120 })
document.createElement = createCanvas
await state.cancel()
assert.equal(captureCanvas.width, 1600)
assert.equal(captureCanvas.height, 1200)
assert.equal(captureCanvas.pixels, null)
// 自动复制模式仍使用原始像素坐标，框选后完成保存、复制与取消。
captureFrame = { ...captureFrame, autoCopy: true }
await listeners.get("screenshot:frame")()
await tick()
assert.equal(state.captureBackground, captureCanvas)
state.startSelection({ button: 0, pointerId: 2, clientX: 20, clientY: 30 })
await state.movePointer({ clientX: 100, clientY: 90 })
await state.finishSelection()
await tick()
assert.equal(
  requests.findLast((request) => request.action === "copy").payload.id,
  "saved-pin"
)
assert.equal(state.frame, null)
assert.deepEqual(failures, [])
// 智能识别直接使用正式选框；新位置先清旧框，旧异步响应不能覆盖新位置。
captureFrame = { ...captureFrame, autoCopy: false }
await listeners.get("screenshot:frame")()
await tick()
const smartReplies = []
pendingSmartSelection = payload => new Promise(resolve => smartReplies.push({ payload, resolve }))
await state.movePointer({ clientX: 50, clientY: 50 })
await new Promise(resolve => setTimeout(resolve, 55))
smartReplies[0].resolve({ regions: [{ x: 20, y: 20, width: 200, height: 200 }] })
await tick()
assert.deepEqual(state.activeRegion, { x: 20, y: 20, width: 200, height: 200 })
assert.ok(findElement(root, element => element.props.class === "capture-selection"))
assert.equal(findElement(root, element => element.props.class === "capture-auto-region"), undefined)
await state.movePointer({ clientX: 200, clientY: 50 })
assert.equal(state.activeRegion, null)
await new Promise(resolve => setTimeout(resolve, 55))
await state.movePointer({ clientX: 350, clientY: 50 })
smartReplies[1].resolve({ regions: [{ x: 300, y: 0, width: 600, height: 200 }] })
await tick()
assert.equal(state.activeRegion, null)
await new Promise(resolve => setTimeout(resolve, 55))
smartReplies[2].resolve({ regions: [{ x: 650, y: 20, width: 200, height: 200 }] })
await tick()
assert.equal(state.activeRegion.x, 650)
pendingSmartSelection = null
state.cancelDrag()

// 长截图入口框选后直接开始，隐藏取色及其快捷键；Esc 和原生事件都保存当前结果。
for (const finishMethod of ["escape", "native", "button"]) {
  captureLong = true
  await listeners.get("screenshot:frame")()
  await tick()
  assert.equal(state.longCaptureRequested, true)
  let longOptions
  pendingLongCapture = async options => {
    longOptions = options
    await options.onProgress({ width: 200, height: 600 }, 3)
    await new Promise(resolve => options.signal.addEventListener("abort", resolve, { once: true }))
    if (options.signal.reason !== "finish") throw new DOMException("已取消", "AbortError")
    return { source: "long.png", thumbnail: "long-thumb.png", width: 200, height: 600 }
  }
  state.startSelection({ button: 0, pointerId: 6, clientX: 20, clientY: 30 })
  await state.movePointer({ clientX: 120, clientY: 130 })
  const captured = state.finishSelection()
  await tick()
  assert.equal(state.longBusy, true)
  assert.equal(state.magnifierVisible, false)
  assert.ok(findElement(root, element => element.props.class === "long-screenshot-preview"))
  assert.equal(teleportTarget.children.some(element => element.props.class === "capture-magnifier"), false)
  assert.deepEqual(longOptions.rect, { x: 40, y: 60, width: 200, height: 200 })
  assert.deepEqual(state.longSize, { width: 200, height: 600 })
  assert.equal(state.longProgress, 3)
  const scrollButton = findElement(root, element => element.props.class === "long-scroll-button")
  const finishButton = findElement(root, element => element.props.class === "long-finish-button")
  assert.ok(finishButton)
  assert.equal(longOptions.shouldAutoScroll(), false)
  scrollButton.props.onClick({ stopPropagation() {} })
  await tick()
  assert.equal(longOptions.shouldAutoScroll(), true)
  assert.equal(scrollButton.props["aria-pressed"], true)
  scrollButton.props.onClick({ stopPropagation() {} })
  assert.equal(longOptions.shouldAutoScroll(), false)
  assert.ok(requests.some(request => request.action === "long-preview" && request.payload.width > 0))
  const copiedBefore = copiedColors.length
  await state.keydown(colorKey("c"))
  await state.keydown(colorKey("Shift"))
  assert.equal(copiedColors.length, copiedBefore)
  assert.equal(state.colorFormat, "rgb")
  const cancelledBefore = requests.filter(request => request.action === "cancel").length
  scrollButton.props.onClick({ stopPropagation() {} })
  if (finishMethod === "native") listeners.get("screenshot:long-finish")()
  else if (finishMethod === "button") finishButton.props.onClick({ stopPropagation() {} })
  else await state.keydown(colorKey("Escape"))
  assert.equal(longOptions.signal.reason, "finish")
  assert.equal(longOptions.shouldAutoScroll(), false)
  await captured
  await tick()
  assert.equal(requests.findLast(request => request.action === "save-document").payload.source, "long.png")
  assert.ok(requests.filter(request => request.action === "cancel").length > cancelledBefore)
  assert.equal(state.longBusy, false)
  assert.equal(state.frame, null)
}
// 取消会话后不能把尚在采集的旧长图保存回来，后续普通截图也不会沿用长截图模式。
await listeners.get("screenshot:frame")()
await tick()
state.startSelection({ button: 0, pointerId: 7, clientX: 20, clientY: 30 })
await state.movePointer({ clientX: 120, clientY: 130 })
const cancelledLong = state.finishSelection()
await tick()
const savedBeforeCancel = requests.filter(request => request.action === "save-document").length
await state.cancel()
await cancelledLong
assert.equal(requests.filter(request => request.action === "save-document").length, savedBeforeCancel)
captureLong = false
await listeners.get("screenshot:frame")()
await tick()
assert.equal(state.longCaptureRequested, false)
assert.equal(state.magnifierVisible, true)
assert.deepEqual(failures, [])
captureApp.unmount()
assert.equal(listeners.size, 0)
async function loadTestComponent(filename, mocks) {
  const { descriptor, errors } = parse(readFileSync(filename, "utf8"), {
    filename
  })
  assert.deepEqual(errors, [])
  for (const style of descriptor.styles) {
    const result = await compileStyleAsync({
      source: style.content,
      filename,
      id: filename,
      scoped: true,
      preprocessLang: style.lang
    })
    assert.deepEqual(result.errors, [])
  }
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
  const module = { exports: {} }
  const code = transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
    { format: "cjs" }
  ).code
  new Function("require", "module", "exports", code)(
    (name) => mocks[name] || require(name),
    module,
    module.exports
  )
  return module.exports.default
}

let storedSettings = {
  hotkeys: true,
  shortcutStyle: "shift",
  restorePins: true
}
let storedEntries = [{ id: "new" }, { id: "middle" }, { id: "old" }]
const settingsRequests = []
const ScreenshotPage = await loadTestComponent(
  "src/features/screenshot/index.vue",
  {
    "@/api/modules/screenshot": {
      screenshotApi: {
        call: async (action, payload) => {
          settingsRequests.push({ action, payload })
          if (action === "list")
            return {
              settings: storedSettings,
              entries: storedEntries,
              errors: [],
              hotkeyErrors: [],
              dataPath: "test/screenshots"
            }
          if (action === "settings") {
            storedSettings = payload
            storedEntries = storedEntries.slice(0, payload.historyLimit)
          }
        }
      }
    },
    "@/api/modules/system": { systemApi: {} },
    "@/api/request": { subscribeReady: async () => () => {} },
    "@/utils/message": { createMessage: { success() {} } },
    "@/components/BaseModal.vue": {
      __esModule: true,
      default: {
        render() {
          return vue.h("div", this.$slots.default?.())
        }
      }
    }
  }
)
const pageApp = renderer.createApp(ScreenshotPage)
pageApp.config.errorHandler = (error) => failures.push(String(error))
pageApp.mount(node("root"))
await tick()
const page = pageApp._instance.setupState
assert.equal(page.settings.historyLimit, 50)
assert.equal(page.captureShortcut, "Shift+F1")
assert.deepEqual(Object.values(page.settings.shortcuts), [
  "Ctrl+F1",
  "Shift+F1",
  "F3",
  "Shift+F3",
  "Ctrl+F3",
  "Ctrl+Alt+R",
  "Ctrl+Alt+F1"
])
page.openSettings()
await tick()
assert.deepEqual(settingsRequests.at(-1), {
  action: "pause-hotkeys",
  payload: { paused: true }
})
page.recordShortcut(
  { key: "F8", ctrlKey: true, shiftKey: true, preventDefault() {} },
  "capture"
)
assert.equal(page.settingsDraft.shortcuts.capture, "Ctrl+Shift+F8")
assert.equal(page.captureShortcut, "Shift+F1")
page.settingsDraft.shortcuts.clipboard = "Ctrl+Shift+F8"
await page.saveSettings()
assert.match(page.error, /不能重复/)
page.settingsDraft.shortcuts.clipboard = "Ctrl+Alt+P"
page.settingsDraft.historyLimit = 0
await page.saveSettings()
assert.match(page.error, /1 到 500/)
page.settingsDraft.historyLimit = 2
window.confirm = () => false
await page.saveSettings()
assert.equal(
  settingsRequests.some((request) => request.action === "settings"),
  false
)
window.confirm = () => true
await page.saveSettings()
await tick()
assert.equal(storedSettings.historyLimit, 2)
assert.equal(page.captureShortcut, "Ctrl+Shift+F8")
assert.equal(page.entries.length, 2)
assert.equal(page.showSettings, false)
assert.deepEqual(settingsRequests.at(-1), {
  action: "pause-hotkeys",
  payload: { paused: false }
})
page.openSettings()
await tick()
pageApp.unmount()
assert.deepEqual(settingsRequests.at(-1), {
  action: "pause-hotkeys",
  payload: { paused: false }
})

// 扩展独立启停；配置直接显示设置弹框，详情只显示截图操作和历史。
let extensionEnabled = true
let finishExtensionToggle
let extensionChanged
let extensionUnsubscribed = 0
const extensionRequests = []
const ScreenshotExtension = await loadTestComponent(
  "src/features/settings/components/ScreenshotExtension.vue",
  {
    vue: { ...vue, defineAsyncComponent: () => ScreenshotPage },
    "@/api/modules/screenshot": {
      screenshotApi: {
        call: async (action, payload) => {
          extensionRequests.push({ action, payload })
          if (action === "status") return { enabled: extensionEnabled }
          assert.equal(action, "set-enabled")
          return new Promise((resolve, reject) => {
            finishExtensionToggle = (error) => {
              if (error) reject(new Error(error))
              else {
                extensionEnabled = payload.enabled
                storedSettings.enabled = extensionEnabled
                resolve({ enabled: extensionEnabled })
              }
            }
          })
        }
      }
    },
    "@/api/request": {
      subscribeReady: async (_, callback) => {
        extensionChanged = callback
        return () => { extensionUnsubscribed++ }
      }
    },
    "@/components/BaseModal.vue": {
      __esModule: true,
      default: { render() { return vue.h("div", this.$slots.default?.()) } }
    }
  }
)
const extensionRoot = node("root")
const extensionApp = renderer.createApp(ScreenshotExtension)
extensionApp.config.errorHandler = error => failures.push(String(error))
extensionApp.mount(extensionRoot)
await tick()
const extension = extensionApp._instance.setupState
assert.equal(extension.enabled, true)
assert.equal(extensionRequests.filter(request => request.action === "set-enabled").length, 0)
const stopping = extension.toggleEnabled()
await tick()
assert.equal(extension.pending, true)
assert.equal(findElement(extensionRoot, element => element.props.role === "switch").props.disabled, true)
await extension.toggleEnabled()
assert.equal(extensionRequests.filter(request => request.action === "set-enabled").length, 1)
finishExtensionToggle()
await stopping
await tick()
assert.equal(extension.enabled, false)
assert.equal(findElement(extensionRoot, element => element.props.role === "switch").props["aria-checked"], false)
const failedStart = extension.toggleEnabled()
finishExtensionToggle("模拟截图设置写入失败")
await failedStart
assert.equal(extension.enabled, false)
assert.match(extension.error, /写入失败/)
extension.openManager(true)
await tick()
assert.equal(extension.showSettings, true)
assert.equal(extension.showManager, false)
assert.ok(findElement(extensionRoot, element => element.props.class === "screenshot-settings-modal"))
assert.equal(findElement(extensionRoot, element => element.props.class === "screenshot-actions"), undefined)
findElement(extensionRoot, element => element.props["aria-label"] === "取消截图设置").props.onClick()
await tick()
assert.equal(extension.showSettings, false)
assert.deepEqual(settingsRequests.at(-1), { action: "pause-hotkeys", payload: { paused: false } })
extension.openManager()
await tick()
assert.equal(extension.showManager, true)
assert.ok(findElement(extensionRoot, element => element.props.class === "screenshot-actions"))
assert.equal(findElement(extensionRoot, element => element.props["aria-label"] === "截图设置"), undefined)
assert.equal(findElement(extensionRoot, element => element.props.class === "screenshot-primary").props.disabled, true)
extensionEnabled = true
await extensionChanged()
await tick()
assert.equal(extension.enabled, true)
extensionApp.unmount()
assert.equal(extensionUnsubscribed, 1)

let editorPaints = 0
const Editor = await loadTestComponent(
  "src/features/screenshot/components/ImageEditor.vue",
  {
    "../renderer": {
      loadImage: async () => ({ naturalWidth: 800, naturalHeight: 600 }),
      canvasSource,
      thumbnailSource,
      selectionRect,
      renderDocument(canvas, image) {
        editorPaints += 1
        canvas.width = image.naturalWidth
        canvas.height = image.naturalHeight
      }
    }
  }
)
const outputs = []
const editorErrors = []
const toolbarHovers = []
const editorApp = renderer.createApp(Editor, {
  source: "image.png",
  reselect: true,
  onOutput: (value) => outputs.push(value),
  onError: (value) => editorErrors.push(value),
  onToolbarHover: (value) => toolbarHovers.push(value)
})
editorApp.config.errorHandler = (error) => failures.push(String(error))
editorApp.mount(node("root"))
await tick()
const editor = editorApp._instance.setupState
// 没有标注时直接复用源图，点击贴图不再重绘整张画布。
const initialPaints = editorPaints
await editor.output("pin")
assert.equal(editorPaints, initialPaints)
assert.equal(outputs.at(-1).output, "image.png")
const editorToolbar = teleportTarget.children.find(
  (element) => element.props.class === "screenshot-toolbar"
)
editorToolbar.props.onPointerenter()
editorToolbar.props.onPointerleave()
assert.deepEqual(toolbarHovers, [true, false])
assert.deepEqual(
  editor.tools.map((tool) => tool.id),
  [
    "select",
    "rectangle",
    "ellipse",
    "line",
    "pen",
    "highlighter",
    "mosaic",
    "text",
    "eraser"
  ]
)
assert.deepEqual(
  editor.outputs.map((action) => action.id),
  ["pin", "ocr", "export", "copy"]
)
const drawingEvent = (clientX, clientY) => ({
  button: 0,
  pointerId: 1,
  clientX,
  clientY,
  stopPropagation() {}
})
// 矩形画完默认聚焦，四个角点固定对角缩放，滚轮只修改当前草稿。
editor.setTool("rectangle")
await editor.startDrawing(drawingEvent(100, 100))
editor.finishDrawing(drawingEvent(300, 240))
await tick()
assert.equal(editor.annotations.length, 0)
assert.equal(document.activeElement, editor.rectangleSelection)
assert.equal(
  editor.rectangleSelection.children.filter(
    (element) => element.props.class === "rectangle-handle"
  ).length,
  4
)
const originalRectangle = [
  { x: 100, y: 100 },
  { x: 300, y: 240 }
]
for (const [corner, x, y, anchor] of [
  ["nw", 80, 70, { x: 300, y: 240 }],
  ["ne", 340, 70, { x: 100, y: 240 }],
  ["se", 340, 280, { x: 100, y: 100 }],
  ["sw", 80, 280, { x: 300, y: 100 }]
]) {
  editor.activeRectangle.points = structuredClone(originalRectangle)
  const handle = node("button")
  const event = { ...drawingEvent(x, y), currentTarget: handle }
  editor.startRectangleResize(event, corner)
  editor.finishRectangleResize(event)
  assert.deepEqual(editor.activeRectangle.points, [anchor, { x, y }])
}
const resizedRectangle = structuredClone(
  vue.toRaw(editor.activeRectangle.points)
)
const cornerEvent = { ...drawingEvent(10, 10), currentTarget: node("button") }
editor.startRectangleResize(cornerEvent, "nw")
editor.resizeRectangle(cornerEvent)
editor.cancelRectangleResize(cornerEvent)
assert.deepEqual(editor.activeRectangle.points, resizedRectangle)
let wheelPrevented = false
const rectangleWheel = {
  deltaY: -120,
  preventDefault() {
    wheelPrevented = true
  },
  stopPropagation() {}
}
editor.resizeRectangleWidth(rectangleWheel)
assert.equal(wheelPrevented, true)
assert.equal(editor.activeRectangle.width, 5)
assert.equal(editor.width, 5)
editor.rectangleSelection.props.onBlur()
await tick()
assert.equal(editor.activeRectangle, null)
assert.equal(editor.rectangleSelection, null)
assert.equal(editor.annotations.length, 1)
assert.equal(editor.annotations[0].width, 5)
assert.deepEqual(editor.annotations[0].points, resizedRectangle)
// 失焦后的原矩形不再参与控制点或滚轮调整，撤销与重做仍视为一次绘制。
editor.resizeRectangleWidth(rectangleWheel)
editor.startRectangleResize(cornerEvent, "nw")
assert.equal(editor.annotations[0].width, 5)
assert.equal(editor.activeRectangle, null)
editor.undo()
assert.equal(editor.annotations.length, 0)
editor.redo()
assert.equal(editor.annotations.length, 1)
editor.undo()
await editor.startDrawing(drawingEvent(20, 30))
editor.finishDrawing(drawingEvent(80, 90))
await tick()
await editor.output("pin")
assert.equal(editor.activeRectangle, null)
assert.equal(outputs.at(-1).annotations.length, 1)
assert.equal(outputs.at(-1).annotations[0].type, "rectangle")
assert.deepEqual(outputs.at(-1).annotations[0].points, [
  { x: 20, y: 30 },
  { x: 80, y: 90 }
])
editor.undo()
editor.width = 4
editor.setTool("line")
await editor.startDrawing(drawingEvent(10, 10))
editor.finishDrawing(drawingEvent(50, 50))
assert.equal(editor.annotations[0].type, "line")
editor.setTool("highlighter")
await editor.startDrawing(drawingEvent(100, 100))
editor.draw(drawingEvent(120, 110))
editor.finishDrawing(drawingEvent(140, 100))
assert.equal(editor.annotations[1].type, "highlighter")
assert.equal(editor.annotations[1].points.length, 4)
editor.setTool("eraser")
await editor.startDrawing(drawingEvent(120, 105))
editor.draw(drawingEvent(130, 110))
editor.finishDrawing(drawingEvent(140, 115))
assert.equal(editor.annotations.length, 3)
assert.equal(editor.annotations[2].type, "eraser")
editor.undo()
assert.equal(editor.annotations.length, 2)
editor.redo()
assert.equal(editor.annotations.length, 3)
// 从真实工具栏事件切换图标选项，验证绘制命令使用当前宽度、线型和箭头类型。
function findElement(element, predicate) {
  if (predicate(element)) return element
  for (const child of element.children) {
    const found = findElement(child, predicate)
    if (found) return found
  }
}
const control = (label) =>
  findElement(
    teleportTarget,
    (element) => element.props["aria-label"] === label
  )
editor.setTool("line")
await tick()
assert.equal(editor.lineOptions.type, "line")
control("线宽").props["onUpdate:modelValue"](10)
control("线条样式").props.onClick()
await tick()
assert.equal(editor.dropdown, "style")
assert.equal(
  control("虚线").children.some((element) => element.type === "svg"),
  true
)
control("虚线").props.onClick()
await tick()
assert.equal(editor.dropdown, "")
control("直线或箭头").props.onClick()
await tick()
control("箭头").props.onClick()
await editor.startDrawing(drawingEvent(10, 20))
editor.finishDrawing(drawingEvent(80, 90))
assert.equal(editor.annotations.at(-1).type, "arrow")
assert.equal(editor.annotations.at(-1).style, "dashed")
assert.equal(editor.annotations.at(-1).width, 10)
await tick()
control("直线或箭头").props.onClick()
await tick()
control("直线").props.onClick()
await editor.startDrawing(drawingEvent(20, 30))
editor.finishDrawing(drawingEvent(90, 100))
assert.equal(editor.annotations.at(-1).type, "line")
editor.dropdown = "style"
editor.closeDropdown({ target: { closest: () => null } })
assert.equal(editor.dropdown, "")

// 文字默认一行，普通回车和中文输入不提交，确认后保留完整换行。
editor.setTool("text")
editor.width = 4
await editor.startDrawing(drawingEvent(80, 90))
await tick()
assert.equal(editor.textInput.props.rows, "1")
assert.equal(editor.textInput.props.wrap, "off")
const rowHeight = Number.parseFloat(editor.textInput.props.style.height)
assert.equal(editor.textInput.props.style.fontSize, "28px")
editor.textInput.props["onUpdate:modelValue"]("第一行\n第二行")
await tick()
assert.equal(
  Number.parseFloat(editor.textInput.props.style.height),
  rowHeight * 2
)
let textPrevented = false
editor.textKeydown({
  key: "Enter",
  preventDefault: () => {
    textPrevented = true
  }
})
assert.equal(textPrevented, false)
editor.textKeydown({ key: "Enter", ctrlKey: true, isComposing: true })
assert.equal(Boolean(editor.textPosition), true)
editor.textKeydown({ key: "Enter", ctrlKey: true, preventDefault() {} })
assert.equal(editor.textPosition, null)
assert.equal(editor.annotations.at(-1).text, "第一行\n第二行")
assert.deepEqual(editor.annotations.at(-1).points, [{ x: 80, y: 90 }])
editor.undo()
assert.equal(editor.annotations.at(-1).type, "line")
editor.redo()
assert.equal(editor.annotations.at(-1).text, "第一行\n第二行")
await editor.output("pin")
assert.equal(outputs.at(-1).action, "pin")
assert.equal(outputs.at(-1).output, "edited.png")
assert.equal(outputs.at(-1).thumbnail, "crop.png")
const encodeEditor = editor.canvas.toBlob
editor.canvas.toBlob = (callback) => callback(null)
await editor.output("pin")
assert.deepEqual(editorErrors, ["截图编码失败"])
assert.equal(editor.outputting, false)
editor.canvas.toBlob = encodeEditor
await editor.output("pin")
assert.equal(outputs.at(-1).output, "edited.png")
editorApp.unmount()
assert.equal(toolbarHovers.at(-1), false)
assert.deepEqual(failures, [])
// 未标注的大图只绘制原图，且不保留无用的全尺寸透明标注缓冲区。
const layer = { width: 7680, height: 4320 }
const largeCanvas = {
  width: 7680,
  height: 4320,
  annotationLayer: layer,
  getContext: context
}
renderDocument(largeCanvas, { naturalWidth: 7680, naturalHeight: 4320 }, [])
assert.equal(largeCanvas.annotationLayer, null)
assert.deepEqual(layer, { width: 0, height: 0 })
let previewSize = null
document.createElement = () => {
  const canvas = createCanvas()
  canvas.toBlob = (callback) => {
    previewSize = [canvas.width, canvas.height]
    callback({ data: "preview.png" })
  }
  return canvas
}
assert.equal(
  await thumbnailSource({ width: 7680, height: 4320 }),
  "preview.png"
)
assert.deepEqual(previewSize, [320, 180])
document.createElement = createCanvas
console.log("截图设置、历史上限、原位坐标和标注工具回归通过")
console.log(
  "Screenshot window: cursor menu positioning, overflow, editor guards and double-click removal passed"
)

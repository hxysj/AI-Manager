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
  selectionRect,
  renderDocument
} from "../src/features/tools/screenshot/renderer.js"

const require = createRequire(import.meta.url)
const filename = "src/features/tools/screenshot/components/ScreenshotWindow.vue"
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
const module = { exports: {} }
const localRequire = (name) => {
  if (name === "@/api/modules/screenshot")
    return {
      screenshotApi: {
        frame: async () => {
          requests.push({ action: "frame" })
          return frameResponse()
        },
        call: async (action, payload) => {
          requests.push({ action, payload })
          if (action === "document")
            return {
              output: "image.png",
              source: "image.png",
              annotations: [],
              pin: { width: 800, height: 600 }
            }
          if (action === "save-document") return "saved-pin"
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
      selectionRect
    }
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
  addEventListener() {},
  removeEventListener() {},
  confirm: () => true,
  document: {
    createElement: () => ({
      width: 0,
      height: 0,
      getContext: context,
      toDataURL: () => "crop.png"
    })
  }
}
const node = (type) => {
  const element = {
    type,
    props: {},
    style: {},
    children: [],
    parent: null,
    offsetWidth: 220,
    offsetHeight: 300,
    decode: async () => {},
    setPointerCapture() {},
    hasPointerCapture: () => false,
    releasePointerCapture() {},
    getContext: () => canvasContext,
    toDataURL: () => "edited.png",
    toBlob: (callback) => callback({ data: "edited.png" }),
    focus() {},
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
app.mount(root)
const tick = async () => {
  for (let index = 0; index < 10; index += 1) await vue.nextTick()
}
await tick()
const pin = root.children.find(
  (element) =>
    typeof element.props.class === "string" &&
    element.props.class.includes("pin-window")
)
const menu = () =>
  pin.children.find((element) => element.props.class === "pin-menu")
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
await openMenu(120, 90)
assert.deepEqual(menu().props.style, { left: "120px", top: "90px" })
await openMenu(250, 150)
assert.deepEqual(menu().props.style, { left: "250px", top: "150px" })
await openMenu(790, 590)
assert.deepEqual(menu().props.style, { left: "790px", top: "590px" })
await openMenu(0, 0)
assert.deepEqual(menu().props.style, { left: "0px", top: "0px" })
// 取消阴影后同时取消边框，重新开启时两者一起恢复。
const pinState = app._instance.setupState
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
await app._instance.setupState.pinAction("edit")
await tick()
await pin.props.onDblclick({})
await openMenu(80, 80)
assert.equal(Boolean(menu()), false)
assert.equal(
  requests.some((request) => request.payload?.action === "destroy"),
  false
)
app.unmount()

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
secondApp.unmount()
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
state.finishSelection()
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
  state.finishSelection()
  await tick()
  assert.equal(state.magnifierVisible, true)
}
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
state.finishSelection()
await tick()
assert.equal(
  requests.findLast((request) => request.action === "copy").payload.id,
  "saved-pin"
)
assert.equal(state.frame, null)
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
  "src/features/tools/screenshot/index.vue",
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
  "Ctrl+Alt+R"
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

const Editor = await loadTestComponent(
  "src/features/tools/screenshot/components/ImageEditor.vue",
  {
    "../renderer": {
      loadImage: async () => ({ naturalWidth: 800, naturalHeight: 600 }),
      renderDocument() {}
    }
  }
)
const outputs = []
const toolbarHovers = []
const editorApp = renderer.createApp(Editor, {
  source: "image.png",
  reselect: true,
  onOutput: (value) => outputs.push(value),
  onToolbarHover: (value) => toolbarHovers.push(value)
})
editorApp.config.errorHandler = (error) => failures.push(String(error))
editorApp.mount(node("root"))
await tick()
const editor = editorApp._instance.setupState
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
  ["pin", "export", "copy"]
)
const drawingEvent = (clientX, clientY) => ({
  button: 0,
  pointerId: 1,
  clientX,
  clientY,
  stopPropagation() {}
})
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
editorApp.unmount()
assert.equal(toolbarHovers.at(-1), false)
assert.deepEqual(failures, [])
console.log("截图设置、历史上限、原位坐标和标注工具回归通过")
console.log(
  "Screenshot window: cursor menu positioning, overflow, editor guards and double-click removal passed"
)

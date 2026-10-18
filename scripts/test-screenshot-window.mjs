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
const module = { exports: {} }
const localRequire = name => {
  if (name === "@/api/modules/screenshot")
    return {
      screenshotApi: {
        call: async (action, payload) => {
          requests.push({ action, payload })
          if (action === "document")
            return {
              output: "image.png",
              source: "image.png",
              annotations: [],
              pin: { width: 800, height: 600 }
            }
          if (action === "frame") return captureFrame
          if (action === "save-document") return "saved-pin"
          if (action === "cancel") listeners.get("screenshot:reset")?.()
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
const context = () => ({
  drawImage() {},
  clearRect() {},
  strokeRect() {},
  getImageData: () => ({ data: [10, 20, 30, 255] })
})
globalThis.window = {
  addEventListener() {},
  removeEventListener() {},
  document: {
    createElement: () => ({
      width: 0,
      height: 0,
      getContext: context,
      toDataURL: () => "crop.png"
    })
  }
}
const node = type => ({
  type,
  props: {},
  children: [],
  parent: null,
  offsetWidth: 220,
  offsetHeight: 300,
  decode: async () => {},
  setPointerCapture() {},
  getContext: context,
  toDataURL: () => "edited.png",
  focus() {},
  addEventListener() {},
  removeEventListener() {},
  getRootNode: () => ({}),
  get options() {
    return this.children.filter(child => child.type === "option")
  },
  getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 })
})
const teleportTarget = node("body")
const renderer = vue.createRenderer({
  querySelector: () => teleportTarget,
  createElement: node,
  createText: () => node("text"),
  createComment: () => node("comment"),
  setText() {},
  setElementText() {},
  parentNode: element => element.parent,
  nextSibling: element =>
    element.parent?.children[element.parent.children.indexOf(element) + 1] ||
    null,
  patchProp: (element, key, previous, value) => {
    element.props[key] = value
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
app.config.errorHandler = error => failures.push(error)
app.mount(root)
const tick = async () => {
  for (let index = 0; index < 10; index += 1) await vue.nextTick()
}
await tick()
const pin = root.children.find(element => element.props.class === "pin-window")
const menu = () =>
  pin.children.find(element => element.props.class === "pin-menu")
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
assert.deepEqual(menu().props.style, { left: "580px", top: "300px" })
await openMenu(0, 0)
assert.deepEqual(menu().props.style, { left: "0px", top: "0px" })
let stopped = false
menu().props.onDblclick({
  stopPropagation() {
    stopped = true
  }
})
assert.equal(stopped, true)
assert.equal(
  requests.some(request => request.payload?.action === "destroy"),
  false
)
await pin.props.onPointerdown({ button: 0, detail: 1 })
await tick()
assert.equal(menu(), undefined)
assert.equal(
  requests.some(request => request.payload?.action === "drag"),
  false
)
await pin.props.onPointerdown({ button: 0, detail: 2 })
assert.equal(
  requests.some(request => request.payload?.action === "drag"),
  false
)
await openMenu(80, 80)
await menu()
  .children.filter(element => element.type === "button")[2]
  .props.onClick()
await tick()
await pin.props.onDblclick({})
await openMenu(80, 80)
assert.equal(menu(), undefined)
assert.equal(
  requests.some(request => request.payload?.action === "destroy"),
  false
)
app.unmount()

const secondApp = renderer.createApp(module.exports.default)
secondApp.config.errorHandler = error => failures.push(error)
secondApp.mount(root)
await tick()
const secondPin = root.children.find(
  element => element.props.class === "pin-window"
)
const dragRequests = () => requests.filter(request => request.payload?.action === "drag").length
const beforeDrag = dragRequests()
await secondPin.props.onPointerdown({ button: 0, detail: 1, clientX: 20, clientY: 20 })
assert.equal(dragRequests(), beforeDrag)
await secondPin.props.onPointermove({ buttons: 1, clientX: 22, clientY: 21 })
assert.equal(dragRequests(), beforeDrag)
await secondPin.props.onPointermove({ buttons: 1, clientX: 28, clientY: 25 })
assert.equal(dragRequests(), beforeDrag + 1)
await secondPin.props.onPointermove({ buttons: 1, clientX: 35, clientY: 30 })
assert.equal(dragRequests(), beforeDrag + 1)
secondPin.props.onPointerup()
await secondPin.props.onPointerdown({ button: 0, detail: 1, clientX: 50, clientY: 50 })
secondPin.props.onPointerup()
await secondPin.props.onPointerdown({ button: 0, detail: 2, clientX: 50, clientY: 50 })
secondPin.props.onPointerup()
assert.equal(dragRequests(), beforeDrag + 1)
await secondPin.props.onDblclick({})
assert.deepEqual(requests.at(-1), {
  action: "pin-action",
  payload: { id: "test-pin", action: "destroy" }
})
assert.equal(
  requests.some(request => request.action === "delete"),
  false
)
assert.deepEqual(failures, [])
secondApp.unmount()
globalThis.location.search = "?mode=capture"
const captureApp = renderer.createApp(module.exports.default)
captureApp.config.errorHandler = error => failures.push(error)
captureApp.mount(root)
await tick()
assert.equal(requests.at(-1).action, "frame")
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
assert.deepEqual(state.selection, { x: 200, y: 200, width: 527, height: 314 })
state.finishSelection()
await state.output({
  source: "crop.png",
  output: "annotated.png",
  annotations: [],
  action: "pin"
})
const saved = requests.findLast(request => request.action === "save-document")
assert.deepEqual(saved.payload.origin, { x: -1720, y: 100 })
assert.equal(
  requests.findLast(request => request.action === "pin").payload.id,
  "saved-pin"
)
assert.equal(state.frame, null)
assert.equal(state.selection, null)
captureFrame = { ...captureFrame, source: "next-desktop.png", left: 0, top: 0 }
await listeners.get("screenshot:frame")()
await tick()
assert.equal(state.frame.source, "next-desktop.png")
assert.equal(state.selection, null)
assert.equal(requests.at(-1).action, "ready")
assert.deepEqual(failures, [])
captureApp.unmount()
assert.equal(listeners.size, 0)
function loadTestComponent(filename, mocks) {
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
  const module = { exports: {} }
  const code = transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
    { format: "cjs" }
  ).code
  new Function("require", "module", "exports", code)(
    name => mocks[name] || require(name),
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
const ScreenshotPage = loadTestComponent(
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
pageApp.config.errorHandler = error => failures.push(error)
pageApp.mount(node("root"))
await tick()
const page = pageApp._instance.setupState
assert.equal(page.settings.historyLimit, 50)
assert.equal(page.captureShortcut, "Ctrl+Shift+A")
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
assert.equal(page.captureShortcut, "Ctrl+Shift+A")
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
  settingsRequests.some(request => request.action === "settings"),
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

const Editor = loadTestComponent(
  "src/features/tools/screenshot/components/ImageEditor.vue",
  {
    "../renderer": {
      loadImage: async () => ({ naturalWidth: 800, naturalHeight: 600 }),
      renderDocument() {}
    }
  }
)
const outputs = []
const editorApp = renderer.createApp(Editor, {
  source: "image.png",
  reselect: true,
  onOutput: value => outputs.push(value)
})
editorApp.config.errorHandler = error => failures.push(error)
editorApp.mount(node("root"))
await tick()
const editor = editorApp._instance.setupState
assert.deepEqual(
  editor.tools.map(tool => tool.id),
  [
    "select",
    "rectangle",
    "ellipse",
    "line",
    "arrow",
    "pen",
    "highlighter",
    "mosaic",
    "text",
    "eraser"
  ]
)
assert.deepEqual(
  editor.outputs.map(action => action.id),
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
assert.equal(editor.annotations.length, 1)
editor.undo()
assert.equal(editor.annotations.length, 2)
editor.redo()
assert.equal(editor.annotations.length, 1)
editor.output("pin")
assert.equal(outputs.at(-1).action, "pin")
assert.equal(outputs.at(-1).output, "edited.png")
editorApp.unmount()
assert.deepEqual(failures, [])
console.log("截图设置、历史上限、原位坐标和标注工具回归通过")
console.log(
  "Screenshot window: cursor menu positioning, edge clamping, editor guards and double-click removal passed"
)

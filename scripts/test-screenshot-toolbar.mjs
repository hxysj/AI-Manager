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
const components = new Map()
const listeners = new Map()
const observers = new Set()
const failures = []
let desktopBounds = { left: 0, top: 0, width: 800, height: 600 }
let toolbarBounds = { width: 530, height: 36 }
globalThis.location = { search: "?mode=capture" }
globalThis.innerWidth = 800
globalThis.innerHeight = 600
globalThis.devicePixelRatio = 2
globalThis.window = {
  addEventListener(name, callback) {
    if (!listeners.has(name)) listeners.set(name, new Set())
    listeners.get(name).add(callback)
  },
  removeEventListener(name, callback) {
    listeners.get(name)?.delete(callback)
    if (!listeners.get(name)?.size) listeners.delete(name)
  }
}
globalThis.ResizeObserver = class {
  constructor(callback) {
    this.callback = callback
  }
  observe(element, options) {
    assert.equal(element.props.class, "screenshot-toolbar")
    assert.equal(options.box, "border-box")
    observers.add(this)
  }
  disconnect() {
    observers.delete(this)
  }
}

// 编译并挂载真实三级组件，验证尺寸事件能一路驱动截图窗口重新定位。
for (const [name, filename] of [
  [
    "./components/EditorToolbar.vue",
    "src/features/screenshot/components/ImageEditor/components/EditorToolbar.vue"
  ],
  [
    "../ImageEditor/index.vue",
    "src/features/screenshot/components/ImageEditor/index.vue"
  ],
  ["window", "src/features/screenshot/components/ScreenshotWindow/index.vue"]
]) {
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
    const compiled = await compileStyleAsync({
      source: style.content,
      filename,
      id: filename,
      scoped: true,
      preprocessLang: style.lang
    })
    assert.deepEqual(compiled.errors, [])
    if (name === "./components/EditorToolbar.vue") {
      assert.match(compiled.code, /box-sizing: border-box/)
      assert.match(compiled.code, /width: max-content/)
    }
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
    (dependency) => {
      if (dependency === "vue") return vue
      if (components.has(dependency))
        return { __esModule: true, default: components.get(dependency) }
      if (dependency.endsWith(".vue"))
        return { __esModule: true, default: { render: () => null } }
      if (dependency === "../../renderer")
        return {
          loadImage: async () => ({ naturalWidth: 100, naturalHeight: 100 }),
          renderDocument() {}
        }
      if (dependency === "@/api/modules/screenshot")
        return {
          screenshotApi: {
            frame: async () => new ArrayBuffer(0),
            call: async () => ({})
          }
        }
      if (dependency === "@/api/modules/system") return { systemApi: {} }
      if (dependency === "@/api/request")
        return { subscribeReady: async () => () => {} }
      if (dependency === "../../long-screenshot") return {}
      if (dependency === "../../rectangle-detector")
        return { disposeRectangleDetector() {} }
      return require(dependency)
    },
    module,
    module.exports
  )
  components.set(name, module.exports.default)
}

const node = (type) => ({
  type,
  props: {},
  style: {},
  children: [],
  parent: null,
  getBoundingClientRect() {
    return this.props.class === "screenshot-toolbar"
      ? toolbarBounds
      : desktopBounds
  },
  getContext: () => ({ clearRect() {} })
})
const body = node("body")
const renderer = vue.createRenderer({
  createElement: node,
  createText: () => node("#text"),
  createComment: () => node("#comment"),
  setText() {},
  setElementText(element) {
    element.children = []
  },
  patchProp(element, key, previous, value) {
    element.props[key] = value
  },
  insert(element, parent, anchor) {
    if (element.parent)
      element.parent.children.splice(
        element.parent.children.indexOf(element),
        1
      )
    const index = anchor ? parent.children.indexOf(anchor) : -1
    parent.children.splice(
      index < 0 ? parent.children.length : index,
      0,
      element
    )
    element.parent = parent
  },
  remove(element) {
    if (element.parent)
      element.parent.children.splice(
        element.parent.children.indexOf(element),
        1
      )
    element.parent = null
  },
  parentNode: (element) => element.parent,
  nextSibling: (element) =>
    element.parent?.children[element.parent.children.indexOf(element) + 1] ||
    null,
  querySelector: () => body
})
async function tick() {
  for (let index = 0; index < 8; index++) {
    await Promise.resolve()
    await vue.nextTick()
  }
}
const app = renderer.createApp(components.get("window"))
app.config.errorHandler = (error) => failures.push(error)
app.mount(node("root"))
await tick()
const state = app._instance.setupState
state.frame = { width: 1600, height: 1200 }
state.selection = { x: 0, y: 100, width: 100, height: 100 }
state.editorSource = "data:image/png;base64,test"
await tick()
const toolbar = () =>
  body.children.find((element) => element.props.class === "screenshot-toolbar")
function assertVisible() {
  const style = toolbar().props.style
  const right = parseFloat(style.right)
  const top = parseFloat(style.top)
  assert.ok(right >= 4, "工具栏不能越过屏幕右侧")
  assert.ok(
    innerWidth - right - toolbarBounds.width >= 4,
    "工具栏不能越过屏幕左侧"
  )
  assert.ok(top >= 4, "工具栏不能越过屏幕顶部")
  assert.ok(
    top + toolbarBounds.height <= innerHeight - 4,
    "工具栏不能越过屏幕底部"
  )
  assert.equal(style.maxWidth, "calc(100vw - 8px)")
  return { right, top }
}
assert.deepEqual(state.toolbarSize, toolbarBounds)
assert.equal(observers.size, 1)
assert.deepEqual(
  assertVisible(),
  { right: 266, top: 108 },
  "左边缘窄选区必须容纳完整工具栏"
)

// 四角、四边和全屏选区，同时覆盖原图按 2 倍比例映射到 CSS 像素。
for (const rectangle of [
  [0, 0, 100, 100],
  [1500, 0, 100, 100],
  [0, 1100, 100, 100],
  [1500, 1100, 100, 100],
  [600, 0, 300, 100],
  [600, 1100, 300, 100],
  [0, 500, 100, 100],
  [1500, 500, 100, 100],
  [0, 0, 1600, 1200]
]) {
  const [x, y, width, height] = rectangle
  state.selection = { x, y, width, height }
  await tick()
  assertVisible()
}
assert.equal(assertVisible().top, 556, "全屏选区时工具栏应移到选区内部")
state.selection = { x: 1000, y: 1000, width: 300, height: 100 }
await tick()
assert.equal(assertVisible().top, 558, "下方有空间时保持 8px 间距")

// 点击真实工具按钮后模拟浏览器尺寸通知，选项展开必须立即触发上翻。
await toolbar()
  .children.find((element) => element.props.class === "toolbar-buttons")
  .children.find((element) => element.props["aria-label"] === "直线 / 箭头")
  .props.onClick()
await tick()
assert.ok(
  toolbar().children.some(
    (element) => element.props.class === "toolbar-options"
  )
)
toolbarBounds = { width: 580, height: 72 }
for (const observer of observers) observer.callback()
await tick()
assert.deepEqual(state.toolbarSize, toolbarBounds)
assert.deepEqual(assertVisible(), { right: 150, top: 420 })

state.selection = { x: 0, y: 0, width: 1600, height: 100 }
await tick()
assert.equal(assertVisible().top, 58, "顶部选区的展开工具栏保留在下方")

// 桌面存在偏移、非等比显示时仍按实际边界缩放；窄窗口使用限制后的外框。
desktopBounds = { left: 20, top: 10, width: 720, height: 480 }
state.selection = { x: 1000, y: 100, width: 300, height: 100 }
await tick()
assert.deepEqual(assertVisible(), { right: 195, top: 98 })
globalThis.innerWidth = 320
globalThis.innerHeight = 240
desktopBounds = { left: 0, top: 0, width: 320, height: 240 }
toolbarBounds = { width: 312, height: 104 }
state.selection = { x: 1500, y: 1100, width: 100, height: 100 }
for (const observer of observers) observer.callback()
await tick()
assert.deepEqual(assertVisible(), { right: 4, top: 108 })
app.unmount()
assert.equal(observers.size, 0, "截图编辑器销毁时必须释放尺寸监听")
assert.equal(listeners.size, 0)

// 贴图编辑沿用底部居中的默认样式，尺寸事件不能覆盖其原有定位。
const pinApp = renderer.createApp(components.get("../ImageEditor/index.vue"), {
  source: "test-pin"
})
pinApp.config.errorHandler = (error) => failures.push(error)
pinApp.mount(node("root"))
await tick()
assert.deepEqual(toolbar().props.style, {
  left: "50%",
  bottom: "12px",
  maxWidth: "calc(100vw - 8px)",
  transform: "translateX(-50%)"
})
pinApp.unmount()
assert.equal(observers.size, 0)
assert.equal(listeners.size, 0)
assert.deepEqual(failures, [])
console.log(
  "截图工具栏四边四角、全屏选区、动态尺寸、缩放偏移、窄窗口、贴图默认定位和监听清理验证通过"
)

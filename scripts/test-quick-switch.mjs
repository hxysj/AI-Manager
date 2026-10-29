import assert from "node:assert/strict"
import { readFileSync, readdirSync } from "node:fs"
import { createRequire } from "node:module"
import { resolve } from "node:path"
import {
  parse,
  compileScript,
  compileTemplate,
  compileStyleAsync
} from "@vue/compiler-sfc"
import { transformSync } from "esbuild"

// 使用真实 Vue 响应式和事件流程验证悬浮窗，只模拟原生窗口 IPC。
const require = createRequire(import.meta.url)
const vue = require("vue")
const calls = []
const messages = []
let handlers = {}
const visibilityListeners = new Set()
const lanShareListeners = new Set()
let lanShareState = { service: {}, totalUnread: 0 }
let nativeLayout = { centerX: 64, centerY: 64, edge: "" }
const api = new Proxy(
  {},
  {
    get: (_, name) => async (payload) => {
      calls.push({ name, payload })
      assert.ok(handlers[name], `缺少接口模拟：${name}`)
      const result = await handlers[name](payload)
      if (name === "setQuickSwitchCollapsed")
        nativeLayout = Number.isFinite(result?.centerX)
          ? result
          : {
              centerX: payload.collapsed ? 64 : 280,
              centerY: payload.collapsed ? 64 : 280,
              edge: ""
            }
      return result
    }
  }
)
const modules = new Map()
const styles = []

function loadComponent(path) {
  path = resolve(path)
  if (modules.has(path)) return modules.get(path)
  const { descriptor, errors } = parse(readFileSync(path, "utf8"), {
    filename: path
  })
  assert.deepEqual(errors, [])
  const script = compileScript(descriptor, {
    id: path,
    genDefaultAs: "__component"
  })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename: path,
    id: path,
    compilerOptions: { bindingMetadata: script.bindings, hoistStatic: false }
  })
  assert.deepEqual(template.errors, [])
  for (const style of descriptor.styles) styles.push({ path, style })
  const code = transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
    { format: "cjs" }
  ).code
  const module = { exports: {} }
  const localRequire = (name) => {
    if (name === "@/api")
      return {
        appApi: api,
        lanShareApi: {
          onStateChanged: (callback) => {
            lanShareListeners.add(callback)
            return () => lanShareListeners.delete(callback)
          },
          getState: async () => lanShareState
        },
        usageApi: api,
        systemApi: api
      }
    if (name === "@/utils/message")
      return {
        createMessage: {
          success: (text) => messages.push({ success: true, text }),
          error: (text) => messages.push({ success: false, text })
        }
      }
    if (
      name === "@/utils/formatters" ||
      name === "@/features/tools/constant" ||
      name === "@/constants/desktop-pets"
    ) {
      if (modules.has(name)) return modules.get(name)
      const imported = { exports: {} }
      const code = transformSync(
        readFileSync(
          resolve(name.replace("@/", "src/") + ".js"),
          "utf8"
        ).replaceAll("import.meta.glob", "globAssets"),
        { format: "cjs" }
      ).code
      new Function("require", "module", "exports", "globAssets", code)(
        require,
        imported,
        imported.exports,
        (pattern) => {
          // 模拟 Vite 资源 URL，目录和动作元数据仍读取用户的真实素材。
          const assets = {}
          for (const folder of readdirSync(resolve("src/assets/pets"), {
            withFileTypes: true
          })) {
            if (!folder.isDirectory()) continue
            const directory = `src/assets/pets/${folder.name}`
            for (const file of readdirSync(resolve(directory))) {
              if (pattern.endsWith(`/${file}`) && file.endsWith(".json"))
                assets[`/${directory}/${file}`] = JSON.parse(
                  readFileSync(resolve(directory, file), "utf8")
                )
              if (pattern.endsWith("/*.webp") && file.endsWith(".webp"))
                assets[`/${directory}/${file}`] = `/${directory}/${file}`
            }
          }
          return assets
        }
      )
      modules.set(name, imported.exports)
      return imported.exports
    }
    if (
      [
        "@/components/DesktopPet.vue",
        "@/components/BaseModal.vue",
        "@/features/settings/components/DesktopPetList.vue",
        "@/features/settings/components/SystemSettings.vue",
        "@/features/settings/components/DesktopPetDetail.vue"
      ].includes(name)
    )
      return {
        default: loadComponent(name.replace("@/", "src/")),
        __esModule: true
      }
    if (name.endsWith("?url")) return { default: "logo.png", __esModule: true }
    if (name.endsWith(".vue"))
      return { default: { render: () => vue.h("div") }, __esModule: true }
    return require(name)
  }
  new Function("require", "module", "exports", code)(
    localRequire,
    module,
    module.exports
  )
  modules.set(path, module.exports.default)
  return module.exports.default
}

globalThis.window = {
  setTimeout,
  clearTimeout,
  setInterval,
  clearInterval,
  addEventListener() {},
  removeEventListener() {}
}
globalThis.Document = class {
  activeElement = null
}
globalThis.document = new Document()
const preloadedPetImages = []
let loadPetImage = async (url) => preloadedPetImages.push(url)
globalThis.Image = class {
  set src(url) {
    loadPetImage(url).then(() => this.onload())
  }
}
function hostNode(type, text = "") {
  return {
    type,
    tagName: type.toUpperCase(),
    text,
    children: [],
    parent: null,
    props: {},
    listeners: {},
    value: "",
    clientHeight: 400,
    addEventListener(name, handler) {
      ;(this.listeners[name] ||= []).push(handler)
    },
    removeEventListener() {},
    setAttribute(name, value) {
      this.props[name] = value
    },
    removeAttribute(name) {
      delete this.props[name]
    },
    getRootNode() {
      return document
    },
    focus() {},
    getBoundingClientRect() {
      return { left: 0, top: 0, right: 560, bottom: 560 }
    },
    get options() {
      return this.children.filter((node) => node.type === "option")
    }
  }
}

const renderer = vue.createRenderer({
  createElement: (type) => hostNode(type),
  createText: (text) => hostNode("#text", text),
  createComment: (text) => hostNode("#comment", text),
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
  patchProp: (node, name, previous, value) => {
    node.props[name] = value
    if (name === "value") node.value = value
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

function mount(component, props, slots) {
  const root = hostNode("root")
  const vnode = vue.h(component, props, slots)
  const warnings = []
  const errors = []
  vnode.appContext = { ...vue.createApp({})._context }
  vnode.appContext.components["ElInput"] = { render: () => vue.h("input") }
  vnode.appContext.config.warnHandler = (message) => warnings.push(message)
  vnode.appContext.config.errorHandler = (error) => {
    errors.push(error)
  }
  renderer.render(vnode, root)
  return {
    root,
    component: vnode.component,
    state: vnode.component.setupState,
    warnings,
    errors,
    unmount: () => renderer.render(null, root)
  }
}

function findNodes(root, className) {
  const result = []
  function visit(node) {
    if (
      String(node.props.class || "")
        .split(" ")
        .includes(className)
    )
      result.push(node)
    node.children.forEach(visit)
  }
  visit(root)
  return result
}

function assertNoHoverTitles(node) {
  if (
    String(node.props.class || "")
      .split(" ")
      .includes("quick-switch-unread-badge")
  )
    return
  assert.equal(node.props.title, undefined)
  node.children.forEach(assertNoHoverTitles)
}

const tick = async () => {
  for (let index = 0; index < 6; index += 1) {
    await Promise.resolve()
    await vue.nextTick()
  }
}
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

const tickDelay = async (ms) => {
  await delay(ms)
  await tick()
}
const fixture = () =>
  vue.reactive({
    cliTargets: [
      { id: "codex", name: "Codex", installed: true },
      { id: "claude", name: "Claude", installed: true }
    ],
    runtimeConfigSchemas: {
      codex: { enabled: true },
      claude: { enabled: true }
    },
    providers: [
      {
        id: "p1",
        name: "测试 Provider",
        runtimeConfig: { mainModel: "gpt-test" }
      }
    ],
    runtimeProfiles: [{ cli: "codex", providerId: "p1", model: "gpt-current" }],
    runtimeModels: [],
    codexAccounts: [],
    repos: [],
    usage: { logs: [] },
    appSettings: { system: {} }
  })
handlers.setQuickSwitchCollapsed = () => true
handlers.moveQuickSwitchBy = () => true
handlers.showMainPanel = () => true
handlers.openToolWindow = () => true
handlers.onQuickSwitchVisibilityChanged = (callback) => {
  visibilityListeners.add(callback)
  return () => visibilityListeners.delete(callback)
}
handlers.markQuickSwitchReady = () => {
  const callback = [...visibilityListeners].at(-1)
  callback({ visible: true, ...nativeLayout })
  return { visible: true }
}
handlers.getUsageStats = (payload) => ({
  data: {
    summary: {
      actualTokens: payload.appType === "codex" ? 5000 : 25000,
      totalCostUsd: 0.25
    }
  }
})
handlers.getReleaseAnnouncement = () => null

const Quick = loadComponent("src/components/QuickSwitchPanel.vue")
const state = fixture()
const panel = mount(Quick, { state })
await tick()
assert.equal(panel.state.expanded, false)
assert.equal(findNodes(panel.root, "quick-switch-tool").length, 0)
assert.equal(panel.state.centerPages.length, 3)
assert.equal(panel.state.usageSummary.actualTokens, 25000)
assertNoHoverTitles(findNodes(panel.root, "quick-switch-center")[0])
lanShareState = { service: {}, totalUnread: 5 }
lanShareListeners.forEach((callback) => callback(lanShareState))
await tick()
assert.equal(findNodes(panel.root, "quick-switch-unread-badge").length, 1)
assert.equal(findNodes(panel.root, "quick-switch-unread-badge-right").length, 0)
panel.state.screenEdge = "left"
await tick()
assert.equal(findNodes(panel.root, "quick-switch-unread-badge-left").length, 1)
panel.state.screenEdge = "right"
await tick()
assert.equal(findNodes(panel.root, "quick-switch-unread-badge-right").length, 1)
const usageCall = calls.find((call) => call.name === "getUsageStats").payload
assert.equal(usageCall.appType, "all")
assert.equal(new Date(usageCall.startAt).getHours(), 0)
assert.equal(new Date(usageCall.endAt).getHours(), 23)
assert.equal(usageCall.logPageSize, 1)

// 自动轮换所有可用页；鼠标停留、拖动和隐藏窗口时都暂停。
panel.state.rotateCenterPage()
await tick()
assert.equal(panel.state.selectedPage, "codex")
panel.state.rotateCenterPage()
await tick()
assert.equal(panel.state.selectedPage, "claude")
panel.state.rotateCenterPage()
await tick()
assert.equal(panel.state.selectedPage, "home")
document.hidden = true
panel.state.rotateCenterPage()
assert.equal(panel.state.selectedPage, "home")
document.hidden = false

panel.state.enterPanel()
panel.state.rotateCenterPage()
assert.equal(panel.state.selectedPage, "home")
await tickDelay(130)
assert.equal(panel.state.expanded, true)
assert.equal(findNodes(panel.root, "quick-switch-tool").length, 10)
assert.deepEqual(
  calls.find(
    (call) => call.name === "setQuickSwitchCollapsed" && !call.payload.collapsed
  ).payload,
  { collapsed: false }
)

// 中心贴近屏幕边缘时，工具改用剩余空间排布并保持互不遮挡。
for (const center of [
  { x: 64, y: 280 },
  { x: 496, y: 280 },
  { x: 280, y: 64 },
  { x: 280, y: 496 },
  { x: 64, y: 64 },
  { x: 496, y: 64 },
  { x: 64, y: 496 },
  { x: 496, y: 496 }
]) {
  panel.state.centerPosition = center
  await tick()
  assert.equal(panel.state.orbitLayout, "adaptive")
  assert.equal(findNodes(panel.root, "quick-switch-tools-adaptive").length, 1)
  assert.equal(findNodes(panel.root, "quick-switch-connections").length, 0)
  const tray = panel.state.toolTray
  assert.ok(
    tray.x >= 12 &&
      tray.y >= 12 &&
      tray.x + tray.width <= 548 &&
      tray.y + tray.height <= 548
  )
  assert.deepEqual(
    panel.state.orbitTools.map((tool) => tool.id),
    panel.state.visibleTools.map((tool) => tool.id)
  )
  const edgeCenter = {
    left: center.x - 56,
    top: center.y - 56,
    right: center.x + 56,
    bottom: center.y + 56
  }
  const edgeTools = panel.state.orbitTools.map((tool) => ({
    left: tool.x - 42,
    top: tool.y - 35,
    right: tool.x + 42,
    bottom: tool.y + 35
  }))
  for (const tool of edgeTools) {
    assert.ok(
      tool.left >= 0 && tool.top >= 0 && tool.right <= 560 && tool.bottom <= 560
    )
    assert.ok(
      tool.right <= edgeCenter.left ||
        edgeCenter.right <= tool.left ||
        tool.bottom <= edgeCenter.top ||
        edgeCenter.bottom <= tool.top
    )
  }
  for (let index = 0; index < edgeTools.length; index += 1) {
    for (let next = index + 1; next < edgeTools.length; next += 1) {
      const left = edgeTools[index]
      const right = edgeTools[next]
      assert.ok(
        left.right <= right.left ||
          right.right <= left.left ||
          left.bottom <= right.top ||
          right.bottom <= left.top
      )
    }
  }
  for (const y of new Set(panel.state.orbitTools.map((tool) => tool.y))) {
    const row = panel.state.orbitTools.filter((tool) => tool.y === y)
    assert.equal((row[0].x + row.at(-1).x) / 2, tray.x + tray.width / 2)
  }
}
panel.state.centerPosition = { x: 280, y: 280 }
await tick()
panel.state.leavePanel()
await tickDelay(100)
assert.equal(panel.state.expanded, true)
panel.state.enterPanel()
await tickDelay(280)
assert.equal(panel.state.expanded, true)
panel.state.leavePanel()
await tickDelay(300)
assert.equal(panel.state.expanded, false)

state.appSettings.system.quickSwitchTools = { git: false }
await tick()
await panel.state.setExpanded(true)
await tick()
assert.equal(findNodes(panel.root, "quick-switch-tool").length, 9)
assert.ok(!panel.state.orbitTools.some((tool) => tool.id === "git"))
state.appSettings.system.quickSwitchTools = Object.fromEntries(
  panel.state.visibleTools
    .map((tool) => [tool.id, false])
    .concat([["git", false]])
)
await tick()
assert.equal(panel.state.expanded, false)
await panel.state.setExpanded(true)
assert.equal(panel.state.expanded, false)
state.appSettings.system.quickSwitchTools = {}
await tick()

// 快速进出时串行完成展开和收起，最终尺寸与最新悬浮状态一致。
let finishResize
handlers.setQuickSwitchCollapsed = (payload) =>
  payload.collapsed
    ? true
    : new Promise((resolve) => {
        finishResize = resolve
      })
const resizing = panel.state.setExpanded(true)
await tick()
panel.state.setExpanded(false)
finishResize(true)
await resizing
assert.equal(panel.state.expanded, false)
assert.deepEqual(
  calls
    .filter((call) => call.name === "setQuickSwitchCollapsed")
    .slice(-2)
    .map((call) => call.payload),
  [{ collapsed: false }, { collapsed: true }]
)
handlers.setQuickSwitchCollapsed = () => true

panel.state.selectedPage = "codex"
await tick()
assert.equal(panel.state.activeName, "测试 Provider")
assert.equal(panel.state.activeModel, "gpt-current")
assert.equal(panel.state.usageSummary.actualTokens, 5000)
state.codexProxyState = { enabled: true, activeProviderId: "p1" }
await tick()
assert.equal(panel.state.activeName, "Proxy · 测试 Provider")
assert.equal(panel.state.activeModel, "gpt-test")

// 切换中心内容时丢弃迟到的旧用量结果。
const usageRequests = []
handlers.getUsageStats = (payload) =>
  new Promise((resolve) => usageRequests.push({ payload, resolve }))
panel.state.selectedPage = "claude"
await tick()
panel.state.selectedPage = "codex"
await tick()
usageRequests[1].resolve({ data: { summary: { actualTokens: 42 } } })
await tick()
usageRequests[0].resolve({ data: { summary: { actualTokens: 1000 } } })
await tick()
assert.equal(panel.state.usageSummary.actualTokens, 42)
handlers.getUsageStats = () => ({
  data: { summary: { actualTokens: 0, totalCostUsd: 0 } }
})

const beforeDrag = calls.filter((call) => call.name === "showMainPanel").length
panel.state.startDrag({
  button: 0,
  screenX: 0,
  screenY: 0,
  pointerId: 1,
  preventDefault() {},
  currentTarget: { setPointerCapture() {} }
})
assert.equal(panel.state.drag.active, true)
await tick()
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 1)
const resizeCountDuringDrag = calls.filter(
  (call) => call.name === "setQuickSwitchCollapsed"
).length
await panel.state.setExpanded(true)
panel.state.leavePanel()
panel.state.enterPanel()
await tickDelay(300)
panel.state.rotateCenterPage()
assert.equal(panel.state.selectedPage, "codex")
assert.equal(panel.state.expanded, false)
assert.equal(
  calls.filter((call) => call.name === "setQuickSwitchCollapsed").length,
  resizeCountDuringDrag
)
panel.state.moveDrag({ screenX: 30, screenY: 20 })
panel.state.stopDrag()
assert.equal(panel.state.drag.active, false)
await tick()
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 0)
panel.state.handleMainClick({ detail: 1 })
await tick()
assert.equal(
  calls.filter((call) => call.name === "showMainPanel").length,
  beforeDrag
)
// 松开后恢复悬浮展开，再次拖动保持原生窗口尺寸稳定。
await tickDelay(130)
assert.equal(panel.state.expanded, true)
const expandedResizeCount = calls.filter(
  (call) => call.name === "setQuickSwitchCollapsed"
).length
handlers.moveQuickSwitchBy = () => ({ centerX: 64, centerY: 280 })
panel.state.startDrag({
  button: 0,
  screenX: 30,
  screenY: 20,
  pointerId: 2,
  preventDefault() {},
  currentTarget: { setPointerCapture() {} }
})
await tick()
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 1)
const previousTools = panel.state.orbitTools.map((tool) => ({
  x: tool.x,
  y: tool.y
}))
await panel.state.moveDrag({ screenX: 40, screenY: 20 })
await tick()
assert.deepEqual(panel.state.centerPosition, { x: 64, y: 280 })
assert.equal(panel.state.orbitLayout, "adaptive")
assert.notDeepEqual(
  panel.state.orbitTools.map((tool) => ({ x: tool.x, y: tool.y })),
  previousTools
)
panel.state.leavePanel()
await tickDelay(300)
assert.equal(panel.state.expanded, true)
assert.equal(
  calls.filter((call) => call.name === "setQuickSwitchCollapsed").length,
  expandedResizeCount
)
panel.state.stopDrag()
await tickDelay(300)
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 0)
assert.equal(panel.state.expanded, false)

// 多次移动按顺序同步；收起等待最后一次移动，避免位置回跳。
await panel.state.setExpanded(true)
const moveRequests = []
handlers.moveQuickSwitchBy = (payload) =>
  new Promise((resolve) => moveRequests.push({ payload, resolve }))
panel.state.startDrag({
  button: 0,
  screenX: 0,
  screenY: 0,
  pointerId: 3,
  preventDefault() {},
  currentTarget: { setPointerCapture() {} }
})
const firstMove = panel.state.moveDrag({ screenX: 10, screenY: 0 })
const secondMove = panel.state.moveDrag({ screenX: 20, screenY: 0 })
await tick()
assert.equal(moveRequests.length, 1)
panel.state.stopDrag()
const collapseAfterMove = panel.state.setExpanded(false)
const beforePendingMove = calls.filter(
  (call) => call.name === "setQuickSwitchCollapsed"
).length
moveRequests[0].resolve({ centerX: 100, centerY: 280 })
await firstMove
await tick()
assert.equal(moveRequests.length, 2)
assert.equal(
  calls.filter((call) => call.name === "setQuickSwitchCollapsed").length,
  beforePendingMove
)
moveRequests[1].resolve({ centerX: 110, centerY: 280 })
await secondMove
await collapseAfterMove
await tick()
assert.deepEqual(panel.state.centerPosition, { x: 64, y: 64 })
assert.equal(panel.state.expanded, false)
await panel.state.openToolWindow("git")
assert.deepEqual(calls.at(-1), {
  name: "openToolWindow",
  payload: { toolId: "git" }
})
panel.state.handleMainClick({ detail: 0 })
await tick()
assert.equal(calls.at(-1).name, "showMainPanel")
assert.equal(calls.at(-1).payload, undefined)
panel.unmount()
const pageAfterUnmount = panel.state.selectedPage
panel.state.rotateCenterPage()
assert.equal(panel.state.selectedPage, pageAfterUnmount)
assert.deepEqual(panel.errors, [])
assert.deepEqual(panel.warnings, [])
assert.equal(visibilityListeners.size, 0)

// Wayland 以局部坐标判断拖动，仅请求一次合成器拖动。
const normalReady = handlers.markQuickSwitchReady
handlers.markQuickSwitchReady = () => ({ visible: true, nativeDrag: true })
let completeNativeDrag
handlers.moveQuickSwitchBy = () => new Promise(resolve => { completeNativeDrag = resolve })
const waylandPanel = mount(Quick, { state: fixture() })
await tick()
const nativeMovesBefore = calls.filter(call => call.name === "moveQuickSwitchBy").length
waylandPanel.state.startDrag({
  button: 0, clientX: 20, clientY: 20, pointerId: 10,
  preventDefault() {}, currentTarget: { setPointerCapture() {} }
})
waylandPanel.state.moveDrag({ clientX: 22, clientY: 20 })
assert.equal(calls.filter(call => call.name === "moveQuickSwitchBy").length, nativeMovesBefore)
const nativeMove = waylandPanel.state.moveDrag({ clientX: 26, clientY: 20 })
waylandPanel.state.moveDrag({ clientX: 32, clientY: 20 })
assert.deepEqual(calls.at(-1), { name: "moveQuickSwitchBy", payload: { native: true } })
assert.equal(calls.filter(call => call.name === "moveQuickSwitchBy").length, nativeMovesBefore + 1)
completeNativeDrag(true)
await nativeMove
assert.equal(waylandPanel.state.drag.active, false)
waylandPanel.unmount()
assert.deepEqual(waylandPanel.errors, [])
assert.deepEqual(waylandPanel.warnings, [])
handlers.markQuickSwitchReady = normalReady
handlers.moveQuickSwitchBy = () => true

// 隐藏后丢弃迟到的展开和拖动结果；复用窗口时恢复收起状态且只初始化一次。
const reusable = mount(Quick, { state: fixture() })
await tick()
const reusableVisibility = [...visibilityListeners].at(-1)
const readyCallCount = calls.filter(
  (call) => call.name === "markQuickSwitchReady"
).length
let finishHiddenResize
handlers.setQuickSwitchCollapsed = () =>
  new Promise((resolve) => {
    finishHiddenResize = resolve
  })
const hiddenResize = reusable.state.setExpanded(true)
await tick()
reusable.state.hovered = true
reusableVisibility({ visible: false })
finishHiddenResize({ centerX: 280, centerY: 280 })
await hiddenResize
assert.equal(reusable.state.expanded, false)
assert.equal(reusable.state.hovered, false)
const hiddenUsageCount = calls.filter(
  (call) => call.name === "getUsageStats"
).length
await reusable.state.loadTodayUsage()
reusable.state.enterPanel()
assert.equal(reusable.state.hovered, false)
assert.equal(
  calls.filter((call) => call.name === "getUsageStats").length,
  hiddenUsageCount
)
reusableVisibility({ visible: true, centerX: 64, centerY: 64, edge: "right" })
await tick()
assert.deepEqual(reusable.state.centerPosition, { x: 64, y: 64 })
assert.equal(reusable.state.screenEdge, "right")
assert.equal(reusable.state.expanded, false)
assert.equal(
  calls.filter((call) => call.name === "markQuickSwitchReady").length,
  readyCallCount
)
let finishHiddenMove
let hiddenMoveCount = 0
handlers.moveQuickSwitchBy = () => {
  hiddenMoveCount += 1
  return new Promise((resolve) => {
    finishHiddenMove = resolve
  })
}
reusable.state.startDrag({
  button: 0,
  screenX: 0,
  screenY: 0,
  pointerId: 10,
  preventDefault() {},
  currentTarget: { setPointerCapture() {} }
})
const hiddenMove = reusable.state.moveDrag({ screenX: 10, screenY: 0 })
const queuedHiddenMove = reusable.state.moveDrag({ screenX: 20, screenY: 0 })
await tick()
reusableVisibility({ visible: false })
assert.equal(reusable.state.drag.active, false)
reusableVisibility({ visible: true, centerX: 64, centerY: 64, edge: "left" })
finishHiddenMove({ centerX: 200, centerY: 280, edge: "" })
await hiddenMove
await queuedHiddenMove
assert.equal(hiddenMoveCount, 1)
assert.deepEqual(reusable.state.centerPosition, { x: 64, y: 64 })
reusable.unmount()
assert.equal(visibilityListeners.size, 0)
assert.deepEqual(reusable.errors, [])
assert.deepEqual(reusable.warnings, [])
handlers.setQuickSwitchCollapsed = () => true
handlers.moveQuickSwitchBy = () => true

// 复用中的窗口重载前端时不一定有显示事件，初始化返回值恢复真实可见状态。
const originalMarkReady = handlers.markQuickSwitchReady
handlers.markQuickSwitchReady = () => ({ visible: true })
const reloaded = mount(Quick, { state: fixture() })
await tick()
assert.equal(reloaded.state.windowVisible, true)
reloaded.state.enterPanel()
await tickDelay(130)
assert.equal(reloaded.state.expanded, true)
reloaded.unmount()
assert.deepEqual(reloaded.errors, [])
handlers.markQuickSwitchReady = () => ({ visible: false })
const preheated = mount(Quick, { state: fixture() })
await tick()
assert.equal(preheated.state.layoutReady, true)
assert.equal(preheated.state.windowVisible, false)
preheated.state.enterPanel()
assert.equal(preheated.state.hovered, false)
preheated.unmount()
handlers.markQuickSwitchReady = originalMarkReady

// 配置和位置尚未加载时保持透明，首帧直接展示已保存的桌宠和贴边动作。
const startupState = fixture()
const startup = mount(Quick, { state: startupState, ready: false })
await tick()
assert.equal(findNodes(startup.root, "quick-switch-center").length, 0)
startup.state.rotateCenterPage()
assert.equal(startup.state.selectedPage, "home")
startupState.appSettings.desktop = { enabled: true, petId: "monkey_animations" }
const pendingPetImages = []
loadPetImage = (url) =>
  new Promise((resolve) => pendingPetImages.push({ url, resolve }))
const readyBeforeImages = calls.filter(
  (call) => call.name === "markQuickSwitchReady"
).length
let finishInitialLayout
handlers.setQuickSwitchCollapsed = () =>
  new Promise((resolve) => {
    finishInitialLayout = resolve
  })
startup.component.props.ready = true
await tick()
assert.equal(findNodes(startup.root, "quick-switch-center").length, 0)
assert.equal(pendingPetImages.length, 16)
assert.ok(pendingPetImages.some((image) => image.url.endsWith("/salute.webp")))
assert.equal(finishInitialLayout, undefined)
assert.equal(
  calls.filter((call) => call.name === "markQuickSwitchReady").length,
  readyBeforeImages
)
for (const image of pendingPetImages.slice(0, -1)) image.resolve()
await tick()
assert.equal(finishInitialLayout, undefined)
pendingPetImages.at(-1).resolve()
await tick()
loadPetImage = async (url) => preloadedPetImages.push(url)
finishInitialLayout({ centerX: 64, centerY: 64, edge: "left" })
await tick()
assert.equal(findNodes(startup.root, "quick-switch-main").length, 0)
assert.ok(
  findNodes(startup.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/peek.webp"
  )
)
assert.equal(startup.state.petAction, "peek")
assert.deepEqual(startup.state.centerLayoutStyle, {
  "--center-x": "64px",
  "--center-y": "64px",
  "--pet-offset-x": "-20px"
})
startup.unmount()
assert.deepEqual(startup.errors, [])
assert.deepEqual(startup.warnings, [])
handlers.setQuickSwitchCollapsed = () => true

// 桌宠替换原内容，默认/失效选择使用第一只，关闭恢复信息和页面轮换。
const petState = fixture()
const petPanel = mount(Quick, { state: petState })
await tick()
assert.equal(findNodes(petPanel.root, "desktop-pet-animation").length, 0)
assert.equal(findNodes(petPanel.root, "quick-switch-main").length, 1)
petState.appSettings.desktop = { enabled: true, petId: "" }
await tick()
assert.equal(petPanel.state.desktopPet.id, "monkey_animations")
assert.equal(petPanel.state.desktopPet.animations.length, 16)
assert.equal(findNodes(petPanel.root, "quick-switch-center-pet").length, 1)
assert.equal(findNodes(petPanel.root, "desktop-pet-animation").length, 1)
assert.equal(findNodes(petPanel.root, "quick-switch-main").length, 0)
assert.equal(findNodes(petPanel.root, "quick-switch-usage").length, 0)
assertNoHoverTitles(findNodes(petPanel.root, "quick-switch-center")[0])
petPanel.state.rotateCenterPage()
assert.equal(petPanel.state.selectedPage, "home")
petState.appSettings.desktop.petId = "removed-pet"
await tick()
assert.equal(petPanel.state.desktopPet.id, "monkey_animations")
petPanel.state.startDrag({
  button: 0,
  screenX: 0,
  screenY: 0,
  pointerId: 4,
  preventDefault() {},
  currentTarget: { setPointerCapture() {} }
})
await tick()
assert.ok(
  findNodes(petPanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/walk.webp"
  )
)
petPanel.state.stopDrag()
await tick()
assert.ok(
  findNodes(petPanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/idle.webp"
  )
)
await petPanel.state.setExpanded(true)
await tick()
assert.equal(findNodes(petPanel.root, "quick-switch-tool").length, 10)
petPanel.state.handleMainClick({ detail: 0 })
await tick()
assert.equal(calls.at(-1).name, "showMainPanel")
petState.appSettings.desktop.enabled = false
await tick()
assert.equal(findNodes(petPanel.root, "desktop-pet-animation").length, 0)
assert.equal(findNodes(petPanel.root, "quick-switch-main").length, 1)
assert.equal(findNodes(petPanel.root, "quick-switch-usage").length, 1)
petPanel.state.leavePanel()
await tickDelay(300)
petPanel.state.rotateCenterPage()
assert.equal(petPanel.state.selectedPage, "codex")
petPanel.unmount()
assert.deepEqual(petPanel.errors, [])
assert.deepEqual(petPanel.warnings, [])

// 四个边缘偷看时保持在窗口内；悬浮时敬礼，拖动时完整显示行走动作。
const edgeState = fixture()
edgeState.appSettings.desktop = { enabled: true, petId: "monkey_animations" }
let currentEdge = "left"
handlers.setQuickSwitchCollapsed = (payload) => ({
  centerX: 64,
  centerY: payload.collapsed ? 64 : 280,
  edge: currentEdge
})
handlers.moveQuickSwitchBy = () => ({
  centerX: 64,
  centerY: 64,
  edge: currentEdge
})
const edgePanel = mount(Quick, { state: edgeState })
await tick()
assert.equal(edgePanel.state.screenEdge, "left")
for (const edge of ["left", "right", "top", "bottom"]) {
  currentEdge = edge
  edgePanel.state.startDrag({
    button: 0,
    screenX: 0,
    screenY: 0,
    pointerId: 8,
    preventDefault() {},
    currentTarget: { setPointerCapture() {} }
  })
  await edgePanel.state.moveDrag({ screenX: 5, screenY: 5 })
  await tick()
  assert.equal(edgePanel.state.petAction, "walk")
  assert.ok(
    findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
      "/walk.webp"
    )
  )
  edgePanel.state.stopDrag()
  await tick()
  const edgeAction = edge === "left" || edge === "right" ? "peek" : ""
  assert.equal(edgePanel.state.petAction, edgeAction)
  assert.deepEqual(edgePanel.state.centerLayoutStyle, {
    "--center-x": "64px",
    "--center-y": "64px",
    "--pet-offset-x": `${edge === "left" ? -20 : edge === "right" ? 20 : 0}px`
  })
  assert.equal(
    findNodes(edgePanel.root, "quick-switch-pet-mirrored").length,
    edge === "right" ? 1 : 0
  )
  assert.ok(
    findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
      edgeAction ? "/peek.webp" : "/idle.webp"
    )
  )
  edgePanel.state.enterPanel()
  await tick()
  assert.equal(edgePanel.state.petAction, "salute")
  assert.equal(findNodes(edgePanel.root, "quick-switch-pet-mirrored").length, 0)
  assert.ok(
    findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
      "/salute.webp"
    )
  )
  await tickDelay(130)
  assert.equal(edgePanel.state.expanded, true)
  assert.equal(edgePanel.state.screenEdge, edge)
  assert.equal(findNodes(edgePanel.root, "quick-switch-tool").length, 10)
  edgePanel.state.leavePanel()
  await tickDelay(300)
  assert.equal(edgePanel.state.expanded, false)
  assert.equal(edgePanel.state.petAction, edgeAction)
}
// 贴边时遗漏进入事件，松开后按当前指针位置恢复悬浮，无需先移出窗口。
currentEdge = "left"
edgePanel.state.startDrag({
  button: 0,
  screenX: 0,
  screenY: 0,
  pointerId: 11,
  preventDefault() {},
  currentTarget: { setPointerCapture() {} }
})
await edgePanel.state.moveDrag({ screenX: 5, screenY: 5 })
edgePanel.state.hovered = false
edgePanel.state.stopDrag({ clientX: 64, clientY: 64 })
await tickDelay(130)
assert.equal(edgePanel.state.hovered, true)
assert.equal(edgePanel.state.petAction, "salute")
assert.equal(edgePanel.state.expanded, true)
edgePanel.state.leavePanel()
await tickDelay(300)
assert.equal(edgePanel.state.petAction, "peek")
edgePanel.state.handlePointerMove()
await tickDelay(130)
assert.equal(edgePanel.state.expanded, true)
assert.equal(edgePanel.state.petAction, "salute")
edgePanel.state.leavePanel()
await tickDelay(300)
currentEdge = ""
edgePanel.state.startDrag({
  button: 0,
  screenX: 0,
  screenY: 0,
  pointerId: 9,
  preventDefault() {},
  currentTarget: { setPointerCapture() {} }
})
await edgePanel.state.moveDrag({ screenX: 5, screenY: 5 })
edgePanel.state.stopDrag()
await tick()
assert.equal(edgePanel.state.screenEdge, "")
assert.equal(edgePanel.state.petAction, "")
assert.ok(
  findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/idle.webp"
  )
)
edgePanel.state.enterPanel()
await tick()
assert.ok(
  findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/salute.webp"
  )
)
edgeState.appSettings.desktop.enabled = false
await tick()
assert.equal(findNodes(edgePanel.root, "quick-switch-center-pet").length, 0)
assertNoHoverTitles(findNodes(edgePanel.root, "quick-switch-center")[0])
edgePanel.unmount()
assert.deepEqual(edgePanel.errors, [])
assert.deepEqual(edgePanel.warnings, [])
handlers.setQuickSwitchCollapsed = () => true
handlers.moveQuickSwitchBy = () => true

// 完整周期轮换、拖动暂停、切换桌宠和卸载清理计时器。
const { desktopPets, getDesktopPet } = modules.get("@/constants/desktop-pets")
const spriteConfig = JSON.parse(
  readFileSync("src/assets/pets/monkey_animations/sprite_config.json", "utf8")
)
for (const animation of desktopPets[0].animations) {
  assert.ok(animation.url.endsWith('/' + animation.id + '.webp'))
  assert.ok(spriteConfig.actions[animation.id])
}
assert.equal(getDesktopPet("").id, desktopPets[0].id)
assert.ok(
  desktopPets[0].animations.every(
    (animation) => animation.url && animation.duration > 0
  )
)
const Pet = loadComponent("src/components/DesktopPet.vue")
const timers = new Map()
const frameTimers = new Map()
const originalSetTimeout = globalThis.setTimeout
const originalClearTimeout = globalThis.clearTimeout
const originalSetInterval = globalThis.setInterval
const originalClearInterval = globalThis.clearInterval
let nextTimer = 1
globalThis.setTimeout = (callback, ms) => {
  const id = nextTimer++
  timers.set(id, { callback, ms })
  return id
}
globalThis.clearTimeout = (id) => timers.delete(id)
globalThis.setInterval = (callback, ms) => {
  const id = nextTimer++
  frameTimers.set(id, { callback, ms })
  return id
}
globalThis.clearInterval = (id) => frameTimers.delete(id)
const pet = mount(Pet, { pet: desktopPets[0] })
await tick()
const firstAnimation = pet.state.animation
findNodes(pet.root, "desktop-pet-sheet")[0].props.onLoad()
assert.equal(timers.size, 1)
assert.equal(frameTimers.size, 1)
// 图片在隐藏时保留，暂停后不推进帧；恢复播放不需要再次加载图片。
pet.component.props.paused = true
await tick()
assert.equal(timers.size, 0)
assert.equal(frameTimers.size, 0)
const hiddenFrame = pet.state.frameIndex
pet.state.startAnimation()
pet.state.rotateAnimation()
assert.equal(pet.state.frameIndex, hiddenFrame)
assert.equal(pet.state.animation.id, firstAnimation.id)
assert.equal(frameTimers.size, 0)
pet.component.props.paused = false
await tick()
assert.equal(timers.size, 1)
assert.equal(frameTimers.size, 1)
const timer = [...timers.values()][0]
assert.ok(timer.ms >= 4000)
assert.equal(timer.ms % firstAnimation.duration, 0)
timer.callback()
await tick()
assert.equal(pet.state.animation.id, desktopPets[0].animations[1].id)
assert.equal(timers.size, 0)
assert.equal(frameTimers.size, 0)
for (let index = 1; index < desktopPets[0].animations.length; index++) {
  pet.state.rotateAnimation()
  await tick()
}
assert.equal(pet.state.animation.id, firstAnimation.id)
pet.component.props.dragging = true
await tick()
assert.equal(pet.state.animation.id, "walk")
pet.state.scheduleAnimation()
assert.equal(timers.size, 0)
const pausedIndex = pet.state.animationIndex
pet.state.rotateAnimation()
assert.equal(pet.state.animationIndex, pausedIndex)
pet.component.props.dragging = false
await tick()
pet.state.scheduleAnimation()
assert.equal(timers.size, 1)
pet.state.rotateAnimation()
await tick()
assert.equal(timers.size, 0)
pet.state.animationIndex = desktopPets[0].animations.findIndex(
  (animation) => animation.id === "walk"
)
await tick()
assert.equal(pet.state.animation.id, "walk")
pet.state.scheduleAnimation()
assert.equal(timers.size, 1)
pet.component.props.dragging = true
await tick()
assert.equal(timers.size, 0)
assert.equal(frameTimers.size, 0)
pet.component.props.dragging = false
await tick()
assert.equal(timers.size, 1)
pet.component.props.pet = { ...desktopPets[0], id: "another-pet" }
await tick()
assert.equal(pet.state.animationIndex, 0)
pet.state.scheduleAnimation()
assert.equal(timers.size, 1)
pet.unmount()
assert.equal(timers.size, 0)
pet.state.rotateAnimation()
assert.equal(pet.state.animationIndex, 0)
for (const animation of desktopPets[0].animations) {
  const preview = mount(Pet, { pet: desktopPets[0], action: animation.id })
  await tick()
  assert.equal(
    findNodes(preview.root, "desktop-pet-sheet")[0].props.href,
    animation.url
  )
  // 逐帧跨行播放且完整循环，裁切区域始终只对应一帧。
  findNodes(preview.root, "desktop-pet-sheet")[0].props.onLoad()
  assert.equal(frameTimers.size, 1)
  const frameTimer = [...frameTimers.values()][0]
  assert.equal(frameTimer.ms, animation.frameDuration)
  assert.equal(
    frameTimer.ms,
    spriteConfig.actions[animation.id].frameDurationMs
  )
  const wall = findNodes(preview.root, "desktop-pet-static-frame")
  assert.equal(wall.length, animation.id === "peek" ? 1 : 0)
  const wallProps = wall.length ? { ...wall[0].props } : null
  for (const [index, frame] of animation.frames.entries()) {
    assert.equal(preview.state.frameIndex, index)
    assert.equal(
      findNodes(preview.root, "desktop-pet-frame")[0].props.viewBox,
      `${frame.x} ${frame.y} ${frame.width} ${frame.height}`
    )
    assert.ok(frame.x >= 0 && frame.x + frame.width <= animation.sheetWidth)
    assert.ok(frame.y >= 0 && frame.y + frame.height <= animation.sheetHeight)
    if (wallProps) {
      // 墙壁像素和位置保持不变，角色播放区域始终在墙壁右侧。
      assert.deepEqual(wall[0].props, wallProps)
      assert.equal(
        findNodes(preview.root, "desktop-pet-frame")[0].props.x,
        animation.staticFrame.width
      )
    }
    frameTimer.callback()
    await tick()
  }
  assert.equal(preview.state.frameIndex, 0)
  preview.state.scheduleAnimation()
  assert.equal(timers.size, 0)
  preview.state.rotateAnimation()
  await tick()
  assert.equal(preview.state.animation.id, animation.id)
  preview.unmount()
  assert.equal(frameTimers.size, 0)
  assert.deepEqual(preview.errors, [])
  assert.deepEqual(preview.warnings, [])
}
const lockedPreview = mount(Pet, { pet: desktopPets[0], action: "idle" })
await tick()
lockedPreview.component.props.action = ""
await tick()
assert.equal(timers.size, 1)
lockedPreview.component.props.action = "salute"
await tick()
assert.equal(timers.size, 0)
assert.equal(lockedPreview.state.animation.id, "salute")
findNodes(lockedPreview.root, "desktop-pet-sheet")[0].props.onLoad()
const saluteFrameTimer = [...frameTimers.values()][0]
saluteFrameTimer.callback()
await tick()
assert.equal(lockedPreview.state.frameIndex, 1)
lockedPreview.component.props.action = "peek"
await tick()
assert.equal(lockedPreview.state.frameIndex, 0)
assert.equal(frameTimers.size, 0)
lockedPreview.unmount()
globalThis.setTimeout = originalSetTimeout
globalThis.clearTimeout = originalClearTimeout
globalThis.setInterval = originalSetInterval
globalThis.clearInterval = originalClearInterval
assert.deepEqual(pet.errors, [])
assert.deepEqual(pet.warnings, [])

// 设置与桌宠的完整保存交互由 test-settings.mjs 验证，这里只检查共用工具枚举。
const SystemSettings = loadComponent("src/features/settings/components/SystemSettings.vue")
const floatingSettings = { quickSwitchVisible: true, quickSwitchTools: {} }
const savedSettings = []
const settings = mount(SystemSettings, {
  settings: floatingSettings,
  cliTargets: fixture().cliTargets,
  pending: false,
  saveSettings: async patch => {
    savedSettings.push(patch)
    Object.assign(floatingSettings.quickSwitchTools, patch.system.quickSwitchTools)
    return true
  }
})
await tick()
assert.equal(findNodes(settings.root, "system-settings-tool").length, 10)
assert.ok(!settings.state.floatingToolItems.some(tool => tool.id === "screenshot"))
await settings.state.saveSetting("quickSwitchTools", false, "git")
assert.deepEqual(savedSettings.at(-1), { system: { quickSwitchTools: { git: false } } })
settings.component.props.settings = { ...floatingSettings, quickSwitchVisible: false }
await tick()
assert.equal(findNodes(settings.root, "system-settings-tool").length, 0)
settings.component.props.settings = { ...floatingSettings, quickSwitchVisible: true }
await tick()
assert.equal(findNodes(settings.root, "system-settings-tool").length, 10)
assert.equal(settings.state.draft.quickSwitchTools.git, false)
settings.unmount()
assert.deepEqual(settings.errors, [])
assert.deepEqual(settings.warnings, [])

const Tools = loadComponent("src/features/tools/index.vue")
const detailChanges = []
let handledRequests = 0
const tools = mount(Tools, {
  toolRequest: { toolId: "git" },
  cliTargets: fixture().cliTargets,
  onDetailChange: (value) => detailChanges.push(value),
  onToolRequestHandled: () => {
    handledRequests += 1
  }
})
await tick()
assert.equal(tools.state.activeTool, "git")
assert.equal(detailChanges.at(-1), true)
assert.equal(handledRequests, 1)
tools.component.props.toolRequest = null
await tick()
assert.equal(tools.state.activeTool, "git")
tools.state.closeTool()
await tick()
tools.component.props.repos = [{ id: "new" }]
await tick()
assert.equal(tools.state.activeTool, "")
tools.component.props.toolRequest = { toolId: "git" }
await tick()
assert.equal(tools.state.activeTool, "git")
assert.equal(handledRequests, 2)
tools.component.props.toolRequest = { toolId: "missing" }
await tick()
assert.equal(tools.state.activeTool, "git")
tools.unmount()
assert.deepEqual(tools.errors, [])
assert.deepEqual(tools.warnings, [])

for (const { path, style } of styles) {
  const result = await compileStyleAsync({
    source: style.content,
    filename: path,
    id: path,
    scoped: style.scoped,
    preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}
// 根组件向浮窗传递初始化状态，模板和绑定由真实 Vue 编译器检查。
const { descriptor: appDescriptor, errors: appErrors } = parse(
  readFileSync("src/App.vue", "utf8")
)
assert.deepEqual(appErrors, [])
const appScript = compileScript(appDescriptor, { id: "app-check" })
const appTemplate = compileTemplate({
  source: appDescriptor.template.content,
  filename: "src/App.vue",
  id: "app-check",
  compilerOptions: { bindingMetadata: appScript.bindings }
})
assert.deepEqual(appTemplate.errors, [])

// 执行根组件的实际初始化函数，验证浮窗只拿快照、跳过备份，并持续接收状态。
const bootstrapSource = appDescriptor.scriptSetup.content.slice(
  appDescriptor.scriptSetup.content.indexOf("async function bootstrap()"),
  appDescriptor.scriptSetup.content.indexOf("function updateState(nextState)")
)
const createBootstrap = new Function(
  "isQuickSwitchPanel",
  "isToolWindow",
  "releaseAnnouncement",
  "appApi",
  "updateState",
  "quickSwitchReady",
  "withGlobalLoading",
  "refreshLocalBackups",
  "showErrorMessage",
  `let unsubscribe = null\n${bootstrapSource}\nreturn { bootstrap, stop: () => unsubscribe?.() }`
)
const bootstrapCalls = []
const snapshots = []
let stateListener
let finishStateSubscription
const quickReady = { value: false }
const initialized = createBootstrap(
  true,
  false,
  { value: null },
  {
    onStateChangedReady: (callback) => {
      stateListener = callback
      return new Promise((resolve) => {
        finishStateSubscription = () =>
          resolve(() => bootstrapCalls.push("stop"))
      })
    },
    bootstrapQuickSwitch: async () => {
      bootstrapCalls.push("snapshot")
      return { appSettings: { desktop: { enabled: true } } }
    },
    bootstrap: () => assert.fail("浮窗不应重复全量初始化")
  },
  (snapshot) => snapshots.push(snapshot),
  quickReady,
  () => assert.fail("浮窗不需要全局加载流程"),
  () => assert.fail("浮窗不应加载备份列表"),
  (error) => {
    throw error
  }
)
const initializing = initialized.bootstrap()
await tick()
assert.equal(quickReady.value, false)
assert.deepEqual(bootstrapCalls, [])
finishStateSubscription()
await initializing
assert.equal(quickReady.value, true)
assert.deepEqual(bootstrapCalls, ["snapshot"])
assert.equal(snapshots[0].appSettings.desktop.enabled, true)
stateListener({ appSettings: { desktop: { enabled: false } } })
assert.equal(snapshots.at(-1).appSettings.desktop.enabled, false)
initialized.stop()
assert.deepEqual(bootstrapCalls, ["snapshot", "stop"])

const mainReady = { value: false }
let mainLoading = false
let finishBackupListing
let mainBootstrapped = false
const mainInitialized = createBootstrap(
  false,
  false,
  { value: null },
  {
    bootstrap: async () => ({ appSettings: {} }),
    getReleaseAnnouncement: async () => null,
    onStateChanged: () => () => {}
  },
  () => {},
  mainReady,
  async (action) => {
    mainLoading = true
    try {
      return await action()
    } finally {
      mainLoading = false
    }
  },
  () =>
    new Promise((resolve) => {
      finishBackupListing = resolve
    }),
  (error) => {
    throw error
  }
)
const mainInitializing = mainInitialized.bootstrap().then(() => {
  mainBootstrapped = true
})
await tick()
assert.equal(mainReady.value, true)
assert.equal(mainLoading, false)
assert.equal(mainBootstrapped, true)
assert.equal(typeof finishBackupListing, "function")
finishBackupListing()
await mainInitializing

console.log(
  "浮窗预热初始化、复用与暂停、贴边偷看、悬浮敬礼、拖动、全部动画预览、设置和页面跳转检查通过"
)

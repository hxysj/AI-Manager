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
const api = new Proxy(
  {},
  {
    get: (_, name) => async (payload) => {
      calls.push({ name, payload })
      assert.ok(handlers[name], `缺少接口模拟：${name}`)
      return handlers[name](payload)
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
    if (name === "@/api") return { appApi: api, usageApi: api, systemApi: api }
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
    if (name === "@/components/DesktopPet.vue")
      return {
        default: loadComponent("src/components/DesktopPet.vue"),
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
  addEventListener() {},
  removeEventListener() {}
}
globalThis.Document = class {
  activeElement = null
}
globalThis.document = new Document()
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
  assert.equal(node.props.title, undefined)
  node.children.forEach(assertNoHoverTitles)
}

const tick = async () => {
  await Promise.resolve()
  await vue.nextTick()
  await Promise.resolve()
  await vue.nextTick()
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
handlers.getUsageStats = (payload) => ({
  data: {
    summary: {
      actualTokens: payload.appType === "codex" ? 5000 : 25000,
      totalCostUsd: 0.25
    }
  }
})

const Quick = loadComponent("src/components/QuickSwitchPanel.vue")
const state = fixture()
const panel = mount(Quick, { state })
await tick()
assert.equal(panel.state.expanded, false)
assert.equal(findNodes(panel.root, "quick-switch-tool").length, 0)
assert.equal(panel.state.centerPages.length, 3)
assert.equal(panel.state.usageSummary.actualTokens, 25000)
assertNoHoverTitles(findNodes(panel.root, "quick-switch-center")[0])
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
await panel.state.showMainPanel("git")
assert.deepEqual(calls.at(-1), {
  name: "showMainPanel",
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

// 配置和位置尚未加载时保持透明，首帧直接展示已保存的桌宠和贴边动作。
const startupState = fixture()
const startup = mount(Quick, { state: startupState, ready: false })
await tick()
assert.equal(findNodes(startup.root, "quick-switch-center").length, 0)
startup.state.rotateCenterPage()
assert.equal(startup.state.selectedPage, "home")
startupState.appSettings.desktop = { enabled: true, petId: "monkey_animations" }
let finishInitialLayout
handlers.setQuickSwitchCollapsed = () =>
  new Promise((resolve) => {
    finishInitialLayout = resolve
  })
startup.component.props.ready = true
await tick()
assert.equal(findNodes(startup.root, "quick-switch-center").length, 0)
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
  "--pet-offset-x": "-2px",
  "--pet-offset-y": "0px"
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
  assert.equal(edgePanel.state.petAction, "peek")
  assert.deepEqual(edgePanel.state.centerLayoutStyle, {
    "--center-x": "64px",
    "--center-y": "64px",
    "--pet-offset-x": `${edge === "left" ? -2 : edge === "right" ? 2 : 0}px`,
    "--pet-offset-y": `${edge === "top" ? -2 : edge === "bottom" ? 2 : 0}px`
  })
  assert.equal(
    findNodes(edgePanel.root, "quick-switch-pet-mirrored").length,
    edge === "right" ? 1 : 0
  )
  assert.ok(
    findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
      "/peek.webp"
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
  assert.equal(edgePanel.state.petAction, "peek")
}
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
  assert.equal(frameTimer.ms, spriteConfig.actions[animation.id].frameDurationMs)
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

const Settings = loadComponent("src/features/settings/index.vue")
const savedSettings = []
const settings = mount(Settings, {
  appSettings: { system: {} },
  cliTargets: fixture().cliTargets,
  pending: false,
  onSave: (payload) => savedSettings.push(payload)
})
settings.state.activeTab = "system"
await tick()
assert.equal(findNodes(settings.root, "settings-floating-tool").length, 10)
assert.ok(
  Object.values(settings.state.draft.system.quickSwitchTools).every(Boolean)
)
settings.state.draft.system.quickSwitchTools.git = false
// 隐藏浮窗时收起工具配置，重新显示仍保留之前的选择。
settings.state.draft.system.quickSwitchVisible = false
await tick()
assert.equal(findNodes(settings.root, "settings-floating-tools-card").length, 0)
assert.equal(findNodes(settings.root, "settings-floating-tool").length, 0)
settings.state.draft.system.quickSwitchVisible = true
await tick()
assert.equal(findNodes(settings.root, "settings-floating-tool").length, 10)
assert.equal(settings.state.draft.system.quickSwitchTools.git, false)
settings.state.submitSettings()
assert.equal(savedSettings.at(-1).system.quickSwitchTools.git, false)
settings.component.props.appSettings = {
  system: { quickSwitchTools: { git: false } }
}
await tick()
assert.equal(settings.state.draft.system.quickSwitchTools.git, false)
assert.equal(
  settings.state.draft.system.quickSwitchTools["image-workbench"],
  true
)

settings.state.activeTab = "desktop"
await tick()
assert.equal(findNodes(settings.root, "settings-desktop-panel").length, 1)
assert.equal(settings.state.draft.desktop.enabled, false)
assert.equal(settings.state.draft.desktop.petId, desktopPets[0].id)
assert.equal(findNodes(settings.root, "settings-desktop-pet").length, 0)
settings.state.draft.desktop.enabled = true
await tick()
assert.equal(findNodes(settings.root, "settings-desktop-pet-active").length, 1)
assert.equal(findNodes(settings.root, "desktop-pet-animation").length, 17)
const animationCards = findNodes(settings.root, "settings-desktop-animation")
assert.equal(animationCards.length, desktopPets[0].animations.length)
animationCards.forEach((card, index) => {
  assert.equal(
    findNodes(card, "desktop-pet-sheet")[0].props.href,
    desktopPets[0].animations[index].url
  )
  assert.equal(
    findNodes(card, "settings-desktop-animation-name")[0].text,
    desktopPets[0].animations[index].name
  )
})
settings.state.submitSettings()
assert.deepEqual(savedSettings.at(-1).desktop, {
  enabled: true,
  petId: desktopPets[0].id
})
// 加入另一只选择夹具，验证保存后重载确实恢复上次选择。
desktopPets.push({
  ...desktopPets[0],
  id: "test-second-pet",
  name: "第二只桌宠"
})
settings.state.draft.desktop.petId = "test-second-pet"
settings.state.submitSettings()
assert.equal(savedSettings.at(-1).desktop.petId, "test-second-pet")
settings.component.props.appSettings = savedSettings.at(-1)
await tick()
assert.equal(settings.state.draft.desktop.petId, "test-second-pet")
assert.equal(
  findNodes(settings.root, "settings-desktop-pet-active")[0]
    .children.at(-1)
    .children.at(-1).text,
  "第二只桌宠"
)
settings.state.draft.desktop.enabled = false
settings.state.submitSettings()
assert.deepEqual(savedSettings.at(-1).desktop, {
  enabled: false,
  petId: "test-second-pet"
})
settings.component.props.appSettings = savedSettings.at(-1)
await tick()
assert.equal(findNodes(settings.root, "settings-desktop-pet").length, 0)
assert.equal(findNodes(settings.root, "settings-desktop-animation").length, 0)
settings.state.draft.desktop.enabled = true
await tick()
assert.equal(settings.state.draft.desktop.petId, "test-second-pet")
assert.equal(settings.state.selectedDesktopPet.id, "test-second-pet")
settings.component.props.appSettings = {
  desktop: { enabled: true, petId: "removed-pet" }
}
await tick()
assert.equal(settings.state.draft.desktop.petId, desktopPets[0].id)
settings.unmount()
desktopPets.pop()
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
console.log(
  "浮窗初始化、贴边偷看、悬浮敬礼、拖动、全部动画预览、设置和页面跳转检查通过"
)

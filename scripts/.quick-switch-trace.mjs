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
const originalEqual = assert.equal
process.stderr.write("checkpoint 15\n")
assert.equal = (actual, expected, message) => {
  if (
    actual !== expected &&
    ((actual && typeof actual === "object") ||
      (expected && typeof expected === "object"))
  )
    throw new Error(
      "Object identity mismatch: " +
        (message || "") +
        " actual=" +
        actual?.type +
        " expected=" +
        expected?.type
    )
  return originalEqual(actual, expected, message)
}
const require = createRequire(import.meta.url)
const vue = require("vue")
const markdownModules = {
  micromark: await import("micromark"),
  "micromark-extension-gfm": await import("micromark-extension-gfm")
}
const calls = []
const messages = []
let handlers = {}
const visibilityListeners = new Set()
const lanShareListeners = new Set()
const assistantListeners = new Set()
const noteListeners = new Set()
let noteSnapshot = {}
const quickNotesApi = {
  onAssistant: async (callback) => {
    noteListeners.add(callback)
    return () => noteListeners.delete(callback)
  },
  invoke: async (action, payload = {}) => {
    calls.push({ name: `quick-notes:${action}`, payload })
    if (action === "assistant-state") return noteSnapshot
    if (action === "capture-cancel") {
      noteSnapshot = {
        ...noteSnapshot,
        status: "cancelled",
        error: "已取消整理，原文已保留"
      }
      for (const callback of noteListeners) callback(noteSnapshot)
    }
    return { ok: true }
  }
}
const desktopOpenListeners = new Set()
const appStateListeners = new Set()
let assistantSnapshot = { status: "idle" }
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
    if (name === "@/api/modules/quick-notes") return { quickNotesApi }
    if (markdownModules[name]) return markdownModules[name]
    if (name === "@/api")
      return {
        appApi: api,
        assistantApi: {
          onEvent: async (callback) => {
            assistantListeners.add(callback)
            return () => assistantListeners.delete(callback)
          },
          getState: async () => assistantSnapshot,
          catalog: async () => ({
            providers: [],
            codexAccounts: [],
            runtimeModels: []
          }),
          desktopSend: api.desktopSend,
          desktopReady: api.desktopReady,
          desktopClose: api.desktopClose,
          onDesktopOpen: async (callback) => {
            desktopOpenListeners.add(callback)
            return () => desktopOpenListeners.delete(callback)
          },
          createThread: api.createThread,
          send: api.send,
          cancel: api.cancel
        },
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
      name === "@/features/assistant/attachments" ||
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
        "@/components/MarkdownContent.vue",
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

const desktopTimers = []
globalThis.window = {
  setTimeout(callback, milliseconds) {
    const handle = setTimeout(callback, milliseconds)
    if (milliseconds === 10000) desktopTimers.push({ callback, handle })
    return handle
  },
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
    focus() {
      document.activeElement = this
    },
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

function nodeText(node) {
  return (
    node.text +
    (node.props.innerHTML || "") +
    node.children.map(nodeText).join("")
  )
}

function assertNoHoverTitles(node) {
  if (
    String(node.props.class || "")
      .split(" ")
      .includes("quick-switch-unread-badge") ||
    String(node.props.class || "")
      .split(" ")
      .includes("quick-switch-assistant-bubble")
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

process.stderr.write("checkpoint 422\n")
const Quick = loadComponent("src/components/QuickSwitchPanel.vue")
const state = fixture()
const panel = mount(Quick, { state })
await tick()
process.stderr.write("checkpoint 426\n")
assert.equal(panel.state.expanded, false)
process.stderr.write("checkpoint 427\n")
assert.equal(findNodes(panel.root, "quick-switch-tool").length, 0)
process.stderr.write("checkpoint 428\n")
assert.equal(panel.state.centerPages.length, 3)
process.stderr.write("checkpoint 429\n")
assert.equal(panel.state.usageSummary.actualTokens, 25000)
assertNoHoverTitles(findNodes(panel.root, "quick-switch-center")[0])
lanShareState = { service: {}, totalUnread: 5 }
lanShareListeners.forEach((callback) => callback(lanShareState))
await tick()
process.stderr.write("checkpoint 434\n")
assert.equal(findNodes(panel.root, "quick-switch-unread-badge").length, 1)
process.stderr.write("checkpoint 435\n")
assert.equal(findNodes(panel.root, "quick-switch-unread-badge-right").length, 0)
panel.state.screenEdge = "left"
await tick()
process.stderr.write("checkpoint 438\n")
assert.equal(findNodes(panel.root, "quick-switch-unread-badge-left").length, 1)
panel.state.screenEdge = "right"
await tick()
process.stderr.write("checkpoint 441\n")
assert.equal(findNodes(panel.root, "quick-switch-unread-badge-right").length, 1)
const usageCall = calls.find((call) => call.name === "getUsageStats").payload
process.stderr.write("checkpoint 443\n")
assert.equal(usageCall.appType, "all")
process.stderr.write("checkpoint 444\n")
assert.equal(new Date(usageCall.startAt).getHours(), 0)
process.stderr.write("checkpoint 445\n")
assert.equal(new Date(usageCall.endAt).getHours(), 23)
process.stderr.write("checkpoint 446\n")
assert.equal(usageCall.logPageSize, 1)

// 自动轮换所有可用页；鼠标停留、拖动和隐藏窗口时都暂停。
panel.state.rotateCenterPage()
await tick()
process.stderr.write("checkpoint 451\n")
assert.equal(panel.state.selectedPage, "codex")
panel.state.rotateCenterPage()
await tick()
process.stderr.write("checkpoint 454\n")
assert.equal(panel.state.selectedPage, "claude")
panel.state.rotateCenterPage()
await tick()
process.stderr.write("checkpoint 457\n")
assert.equal(panel.state.selectedPage, "home")
document.hidden = true
panel.state.rotateCenterPage()
process.stderr.write("checkpoint 460\n")
assert.equal(panel.state.selectedPage, "home")
document.hidden = false

panel.state.enterPanel()
panel.state.rotateCenterPage()
process.stderr.write("checkpoint 465\n")
assert.equal(panel.state.selectedPage, "home")
await tickDelay(130)
process.stderr.write("checkpoint 467\n")
assert.equal(panel.state.expanded, true)
process.stderr.write("checkpoint 468\n")
assert.equal(
  findNodes(panel.root, "quick-switch-tool").length,
  panel.state.visibleTools.length
)
process.stderr.write("checkpoint 472\n")
assert.ok(
  !panel.state.visibleTools.some((tool) => tool.id === "assistant"),
  "助手不属于工具中心"
)
process.stderr.write("checkpoint 476\n")
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
process.stderr.write("checkpoint 554\n")
assert.equal(panel.state.expanded, true)
panel.state.enterPanel()
await tickDelay(280)
process.stderr.write("checkpoint 557\n")
assert.equal(panel.state.expanded, true)
panel.state.leavePanel()
await tickDelay(300)
process.stderr.write("checkpoint 560\n")
assert.equal(panel.state.expanded, false)

state.appSettings.system.quickSwitchTools = { git: false }
await tick()
await panel.state.setExpanded(true)
await tick()
process.stderr.write("checkpoint 566\n")
assert.equal(
  findNodes(panel.root, "quick-switch-tool").length,
  panel.state.visibleTools.length
)
process.stderr.write("checkpoint 570\n")
assert.ok(!panel.state.orbitTools.some((tool) => tool.id === "git"))
state.appSettings.system.quickSwitchTools = Object.fromEntries(
  panel.state.visibleTools
    .map((tool) => [tool.id, false])
    .concat([["git", false]])
)
await tick()
process.stderr.write("checkpoint 577\n")
assert.equal(panel.state.expanded, false)
await panel.state.setExpanded(true)
process.stderr.write("checkpoint 579\n")
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
process.stderr.write("checkpoint 596\n")
assert.equal(panel.state.expanded, false)
process.stderr.write("checkpoint 597\n")
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
process.stderr.write("checkpoint 608\n")
assert.equal(panel.state.activeName, "测试 Provider")
process.stderr.write("checkpoint 609\n")
assert.equal(panel.state.activeModel, "gpt-current")
process.stderr.write("checkpoint 610\n")
assert.equal(panel.state.usageSummary.actualTokens, 5000)
state.codexProxyState = { enabled: true, activeProviderId: "p1" }
await tick()
process.stderr.write("checkpoint 613\n")
assert.equal(panel.state.activeName, "Proxy · 测试 Provider")
process.stderr.write("checkpoint 614\n")
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
process.stderr.write("checkpoint 628\n")
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
process.stderr.write("checkpoint 642\n")
assert.equal(panel.state.drag.active, true)
await tick()
process.stderr.write("checkpoint 644\n")
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 1)
const resizeCountDuringDrag = calls.filter(
  (call) => call.name === "setQuickSwitchCollapsed"
).length
await panel.state.setExpanded(true)
panel.state.leavePanel()
panel.state.enterPanel()
await tickDelay(300)
panel.state.rotateCenterPage()
process.stderr.write("checkpoint 653\n")
assert.equal(panel.state.selectedPage, "codex")
process.stderr.write("checkpoint 654\n")
assert.equal(panel.state.expanded, false)
process.stderr.write("checkpoint 655\n")
assert.equal(
  calls.filter((call) => call.name === "setQuickSwitchCollapsed").length,
  resizeCountDuringDrag
)
panel.state.moveDrag({ screenX: 30, screenY: 20 })
panel.state.stopDrag()
process.stderr.write("checkpoint 661\n")
assert.equal(panel.state.drag.active, false)
await tick()
process.stderr.write("checkpoint 663\n")
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 0)
panel.state.handleMainClick({ detail: 1 })
await tick()
process.stderr.write("checkpoint 666\n")
assert.equal(
  calls.filter((call) => call.name === "showMainPanel").length,
  beforeDrag
)
// 松开后恢复悬浮展开，再次拖动保持原生窗口尺寸稳定。
await tickDelay(130)
process.stderr.write("checkpoint 672\n")
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
process.stderr.write("checkpoint 686\n")
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 1)
const previousTools = panel.state.orbitTools.map((tool) => ({
  x: tool.x,
  y: tool.y
}))
await panel.state.moveDrag({ screenX: 40, screenY: 20 })
await tick()
process.stderr.write("checkpoint 693\n")
assert.deepEqual(panel.state.centerPosition, { x: 64, y: 280 })
process.stderr.write("checkpoint 694\n")
assert.equal(panel.state.orbitLayout, "adaptive")
process.stderr.write("checkpoint 695\n")
assert.notDeepEqual(
  panel.state.orbitTools.map((tool) => ({ x: tool.x, y: tool.y })),
  previousTools
)
panel.state.leavePanel()
await tickDelay(300)
process.stderr.write("checkpoint 701\n")
assert.equal(panel.state.expanded, true)
process.stderr.write("checkpoint 702\n")
assert.equal(
  calls.filter((call) => call.name === "setQuickSwitchCollapsed").length,
  expandedResizeCount
)
panel.state.stopDrag()
await tickDelay(300)
process.stderr.write("checkpoint 708\n")
assert.equal(findNodes(panel.root, "quick-switch-panel-dragging").length, 0)
process.stderr.write("checkpoint 709\n")
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
process.stderr.write("checkpoint 727\n")
assert.equal(moveRequests.length, 1)
panel.state.stopDrag()
const collapseAfterMove = panel.state.setExpanded(false)
const beforePendingMove = calls.filter(
  (call) => call.name === "setQuickSwitchCollapsed"
).length
moveRequests[0].resolve({ centerX: 100, centerY: 280 })
await firstMove
await tick()
process.stderr.write("checkpoint 736\n")
assert.equal(moveRequests.length, 2)
process.stderr.write("checkpoint 737\n")
assert.equal(
  calls.filter((call) => call.name === "setQuickSwitchCollapsed").length,
  beforePendingMove
)
moveRequests[1].resolve({ centerX: 110, centerY: 280 })
await secondMove
await collapseAfterMove
await tick()
process.stderr.write("checkpoint 745\n")
assert.deepEqual(panel.state.centerPosition, { x: 64, y: 64 })
process.stderr.write("checkpoint 746\n")
assert.equal(panel.state.expanded, false)
await panel.state.openToolWindow("git")
process.stderr.write("checkpoint 748\n")
assert.deepEqual(calls.at(-1), {
  name: "openToolWindow",
  payload: { toolId: "git" }
})
panel.state.handleMainClick({ detail: 0 })
await tick()
process.stderr.write("checkpoint 754\n")
assert.equal(calls.at(-1).name, "showMainPanel")
process.stderr.write("checkpoint 755\n")
assert.equal(calls.at(-1).payload, undefined)
panel.unmount()
const pageAfterUnmount = panel.state.selectedPage
panel.state.rotateCenterPage()
process.stderr.write("checkpoint 759\n")
assert.equal(panel.state.selectedPage, pageAfterUnmount)
process.stderr.write("checkpoint 760\n")
assert.deepEqual(panel.errors, [])
process.stderr.write("checkpoint 761\n")
assert.deepEqual(panel.warnings, [])
process.stderr.write("checkpoint 762\n")
assert.equal(visibilityListeners.size, 0)

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
process.stderr.write("checkpoint 782\n")
assert.equal(reusable.state.expanded, false)
process.stderr.write("checkpoint 783\n")
assert.equal(reusable.state.hovered, false)
const hiddenUsageCount = calls.filter(
  (call) => call.name === "getUsageStats"
).length
await reusable.state.loadTodayUsage()
reusable.state.enterPanel()
process.stderr.write("checkpoint 789\n")
assert.equal(reusable.state.hovered, false)
process.stderr.write("checkpoint 790\n")
assert.equal(
  calls.filter((call) => call.name === "getUsageStats").length,
  hiddenUsageCount
)
reusableVisibility({ visible: true, centerX: 64, centerY: 64, edge: "right" })
await tick()
process.stderr.write("checkpoint 796\n")
assert.deepEqual(reusable.state.centerPosition, { x: 64, y: 64 })
process.stderr.write("checkpoint 797\n")
assert.equal(reusable.state.screenEdge, "right")
process.stderr.write("checkpoint 798\n")
assert.equal(reusable.state.expanded, false)
process.stderr.write("checkpoint 799\n")
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
process.stderr.write("checkpoint 823\n")
assert.equal(reusable.state.drag.active, false)
reusableVisibility({ visible: true, centerX: 64, centerY: 64, edge: "left" })
finishHiddenMove({ centerX: 200, centerY: 280, edge: "" })
await hiddenMove
await queuedHiddenMove
process.stderr.write("checkpoint 828\n")
assert.equal(hiddenMoveCount, 1)
process.stderr.write("checkpoint 829\n")
assert.deepEqual(reusable.state.centerPosition, { x: 64, y: 64 })
reusable.unmount()
process.stderr.write("checkpoint 831\n")
assert.equal(visibilityListeners.size, 0)
process.stderr.write("checkpoint 832\n")
assert.deepEqual(reusable.errors, [])
process.stderr.write("checkpoint 833\n")
assert.deepEqual(reusable.warnings, [])
handlers.setQuickSwitchCollapsed = () => true
handlers.moveQuickSwitchBy = () => true

// 复用中的窗口重载前端时不一定有显示事件，初始化返回值恢复真实可见状态。
const originalMarkReady = handlers.markQuickSwitchReady
handlers.markQuickSwitchReady = () => ({ visible: true })
const reloaded = mount(Quick, { state: fixture() })
await tick()
process.stderr.write("checkpoint 842\n")
assert.equal(reloaded.state.windowVisible, true)
reloaded.state.enterPanel()
await tickDelay(130)
process.stderr.write("checkpoint 845\n")
assert.equal(reloaded.state.expanded, true)
reloaded.unmount()
process.stderr.write("checkpoint 847\n")
assert.deepEqual(reloaded.errors, [])
handlers.markQuickSwitchReady = () => ({ visible: false })
const preheated = mount(Quick, { state: fixture() })
await tick()
process.stderr.write("checkpoint 851\n")
assert.equal(preheated.state.layoutReady, true)
process.stderr.write("checkpoint 852\n")
assert.equal(preheated.state.windowVisible, false)
preheated.state.enterPanel()
process.stderr.write("checkpoint 854\n")
assert.equal(preheated.state.hovered, false)
preheated.unmount()
handlers.markQuickSwitchReady = originalMarkReady

// 配置和位置尚未加载时保持透明，首帧直接展示已保存的桌宠和贴边动作。
const startupState = fixture()
const startup = mount(Quick, { state: startupState, ready: false })
await tick()
process.stderr.write("checkpoint 862\n")
assert.equal(findNodes(startup.root, "quick-switch-center").length, 0)
startup.state.rotateCenterPage()
process.stderr.write("checkpoint 864\n")
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
process.stderr.write("checkpoint 879\n")
assert.equal(findNodes(startup.root, "quick-switch-center").length, 0)
process.stderr.write("checkpoint 880\n")
assert.equal(pendingPetImages.length, 16)
process.stderr.write("checkpoint 881\n")
assert.ok(pendingPetImages.some((image) => image.url.endsWith("/salute.webp")))
process.stderr.write("checkpoint 882\n")
assert.equal(finishInitialLayout, undefined)
process.stderr.write("checkpoint 883\n")
assert.equal(
  calls.filter((call) => call.name === "markQuickSwitchReady").length,
  readyBeforeImages
)
for (const image of pendingPetImages.slice(0, -1)) image.resolve()
await tick()
process.stderr.write("checkpoint 889\n")
assert.equal(finishInitialLayout, undefined)
pendingPetImages.at(-1).resolve()
await tick()
loadPetImage = async (url) => preloadedPetImages.push(url)
finishInitialLayout({ centerX: 64, centerY: 64, edge: "left" })
await tick()
process.stderr.write("checkpoint 895\n")
assert.equal(findNodes(startup.root, "quick-switch-main").length, 0)
process.stderr.write("checkpoint 896\n")
assert.ok(
  findNodes(startup.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/peek.webp"
  )
)
process.stderr.write("checkpoint 901\n")
assert.equal(startup.state.petAction, "peek")
process.stderr.write("checkpoint 902\n")
assert.deepEqual(startup.state.centerLayoutStyle, {
  "--center-x": "64px",
  "--center-y": "64px",
  "--pet-offset-x": "-20px"
})
startup.unmount()
process.stderr.write("checkpoint 908\n")
assert.deepEqual(startup.errors, [])
process.stderr.write("checkpoint 909\n")
assert.deepEqual(startup.warnings, [])
handlers.setQuickSwitchCollapsed = () => true

// 桌宠替换原内容，默认/失效选择使用第一只，关闭恢复信息和页面轮换。
const petState = fixture()
const petPanel = mount(Quick, { state: petState })
await tick()
process.stderr.write("checkpoint 916\n")
assert.equal(findNodes(petPanel.root, "desktop-pet-animation").length, 0)
process.stderr.write("checkpoint 917\n")
assert.equal(findNodes(petPanel.root, "quick-switch-main").length, 1)
petState.appSettings.desktop = { enabled: true, petId: "" }
await tick()
process.stderr.write("checkpoint 920\n")
assert.equal(petPanel.state.desktopPet.id, "monkey_animations")
process.stderr.write("checkpoint 921\n")
assert.equal(petPanel.state.desktopPet.animations.length, 16)
process.stderr.write("checkpoint 922\n")
assert.equal(findNodes(petPanel.root, "quick-switch-center-pet").length, 1)
process.stderr.write("checkpoint 923\n")
assert.equal(findNodes(petPanel.root, "desktop-pet-animation").length, 1)
process.stderr.write("checkpoint 924\n")
assert.equal(findNodes(petPanel.root, "quick-switch-main").length, 0)
process.stderr.write("checkpoint 925\n")
assert.equal(findNodes(petPanel.root, "quick-switch-usage").length, 0)
assertNoHoverTitles(findNodes(petPanel.root, "quick-switch-center")[0])
// 后台状态和明确动作驱动桌宠，拖动仍由原有逻辑优先接管。
for (const callback of assistantListeners)
  callback({ type: "state", state: { status: "thinking" } })
await tick()
process.stderr.write("checkpoint 931\n")
assert.equal(petPanel.state.petAction, "think")
for (const callback of assistantListeners)
  callback({
    type: "state",
    state: {
      status: "working",
      petAction: "read_book",
      petMessage: "正在读书"
    }
  })
await tick()
process.stderr.write("checkpoint 942\n")
assert.equal(petPanel.state.petAction, "read_book")
for (const callback of assistantListeners)
  callback({ type: "state", state: { status: "idle" } })
await tick()
petPanel.state.rotateCenterPage()
process.stderr.write("checkpoint 947\n")
assert.equal(petPanel.state.selectedPage, "home")
petState.appSettings.desktop.petId = "removed-pet"
await tick()
process.stderr.write("checkpoint 950\n")
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
process.stderr.write("checkpoint 960\n")
assert.ok(
  findNodes(petPanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/walk.webp"
  )
)
petPanel.state.stopDrag()
await tick()
process.stderr.write("checkpoint 967\n")
assert.ok(
  findNodes(petPanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/idle.webp"
  )
)
await petPanel.state.setExpanded(true)
await tick()
process.stderr.write("checkpoint 974\n")
assert.equal(
  findNodes(petPanel.root, "quick-switch-tool").length,
  petPanel.state.visibleTools.length
)
petPanel.state.handleMainClick({ detail: 0 })
await tick()
process.stderr.write("checkpoint 980\n")
assert.equal(calls.at(-1).name, "showMainPanel")
petState.appSettings.desktop.enabled = false
await tick()
process.stderr.write("checkpoint 983\n")
assert.equal(findNodes(petPanel.root, "desktop-pet-animation").length, 0)
process.stderr.write("checkpoint 984\n")
assert.equal(findNodes(petPanel.root, "quick-switch-main").length, 1)
process.stderr.write("checkpoint 985\n")
assert.equal(findNodes(petPanel.root, "quick-switch-usage").length, 1)
petPanel.state.leavePanel()
await tickDelay(300)
petPanel.state.rotateCenterPage()
process.stderr.write("checkpoint 989\n")
assert.equal(petPanel.state.selectedPage, "codex")
petPanel.unmount()
process.stderr.write("checkpoint 991\n")
assert.deepEqual(petPanel.errors, [])
process.stderr.write("checkpoint 992\n")
assert.deepEqual(petPanel.warnings, [])

// 全局输入窗发送后，由桌宠消费任务与流事件，悬浮区不再提供聊天按钮。
const chatState = fixture()
chatState.appSettings.desktop = { enabled: true, petId: "monkey_animations" }
chatState.appSettings.agents = {
  assistant: { enabled: true, targetId: "p1", model: "agent-model" }
}
const emitAssistant = (event) => {
  if (event.type === "state") assistantSnapshot = event.state
  for (const callback of assistantListeners) callback(event)
}
let chatRun = 0
let lastChatTask
function beginChat(extra = {}) {
  lastChatTask = {
    runId: `chat-${++chatRun}`,
    threadId: "desktop-thread",
    status: "thinking",
    surface: "desktop",
    replyAtPet: true,
    text: "",
    ...extra
  }
  emitAssistant({ type: "state", state: { ...lastChatTask } })
}
const completeChat = (text) =>
  emitAssistant({
    type: "state",
    state: { ...lastChatTask, status: "completed", text }
  })
handlers.cancel = () =>
  emitAssistant({
    type: "state",
    state: { ...lastChatTask, status: "cancelled", text: "部分回复" }
  })
const chat = mount(Quick, { state: chatState })
await tick()
chat.state.enterPanel()
await tick()
process.stderr.write("checkpoint 1032\n")
assert.equal(findNodes(chat.root, "quick-switch-assistant-bubble").length, 0)
beginChat({ surface: "panel" })
await tick()
process.stderr.write("checkpoint 1035\n")
assert.equal(chat.state.desktopChatVisible, false, "完整面板任务不弹桌宠气泡")
beginChat()
await tick()
process.stderr.write("checkpoint 1038\n")
assert.equal(chat.state.expanded, true)
process.stderr.write("checkpoint 1039\n")
assert.equal(findNodes(chat.root, "quick-switch-speech-thinking").length, 1)
process.stderr.write("checkpoint 1040\n")
assert.match(
  nodeText(findNodes(chat.root, "quick-switch-thought")[0]),
  /思考中/
)
process.stderr.write("checkpoint 1044\n")
assert.equal(chat.state.petAction, "think")
emitAssistant({ type: "state", state: { ...lastChatTask, status: "working" } })
await tick()
process.stderr.write("checkpoint 1047\n")
assert.match(
  nodeText(findNodes(chat.root, "quick-switch-thought")[0]),
  /正在处理/
)
emitAssistant({
  type: "text-delta",
  runId: lastChatTask.runId,
  text: "流式回复"
})
emitAssistant({ type: "text-delta", runId: "unrelated", text: "其他会话" })
await tick()
process.stderr.write("checkpoint 1058\n")
assert.equal(chat.state.desktopReply, "流式回复")
process.stderr.write("checkpoint 1059\n")
assert.equal(findNodes(chat.root, "quick-switch-speech-thinking").length, 0)
process.stderr.write("checkpoint 1060\n")
assert.equal(findNodes(chat.root, "quick-switch-composer").length, 0)
process.stderr.write("checkpoint 1061\n")
assert.equal(desktopTimers.length, 0, "生成过程中不开始十秒计时")
completeChat("流式回复完成")
await tick()
process.stderr.write("checkpoint 1064\n")
assert.equal(chat.state.desktopReply, "流式回复完成")
process.stderr.write("checkpoint 1065\n")
assert.equal(desktopTimers.length, 1)
completeChat("流式回复完成")
process.stderr.write("checkpoint 1067\n")
assert.equal(desktopTimers.length, 1, "重复终态不延长计时")
chat.state.pauseDesktopReply()
desktopTimers[0].callback()
process.stderr.write("checkpoint 1070\n")
assert.equal(
  chat.state.desktopReply,
  "流式回复完成",
  "悬浮期间已排队的计时器也不能隐藏回复"
)
chat.state.resumeDesktopReply()
process.stderr.write("checkpoint 1076\n")
assert.equal(desktopTimers.length, 2, "离开后重新计满十秒")
chat.state.leavePanel()
await tickDelay(300)
clearTimeout(desktopTimers.at(-1).handle)
desktopTimers.at(-1).callback()
await tick()
process.stderr.write("checkpoint 1082\n")
assert.equal(chat.state.desktopChatVisible, false)
process.stderr.write("checkpoint 1083\n")
assert.equal(chat.state.expanded, false)
process.stderr.write("checkpoint 1084\n")
assert.ok(
  !calls.some((call) => call.name === "desktopDismiss"),
  "气泡收起不能隐藏原生桌宠"
)
completeChat("迟到的完成快照")
await tick()
process.stderr.write("checkpoint 1090\n")
assert.equal(chat.state.desktopChatVisible, false)
beginChat()
completeChat("第二次回复")
await tick()
const staleReplyTimer = desktopTimers.at(-1)
beginChat()
await tick()
process.stderr.write("checkpoint 1097\n")
assert.equal(chat.state.desktopReply, "", "下一条立即替换旧回复")
process.stderr.write("checkpoint 1098\n")
assert.equal(findNodes(chat.root, "quick-switch-speech-thinking").length, 1)
emitAssistant({
  type: "text-delta",
  runId: lastChatTask.runId,
  text: "第三次回复"
})
staleReplyTimer.callback()
process.stderr.write("checkpoint 1105\n")
assert.equal(chat.state.desktopReply, "第三次回复", "旧计时器不能清空新任务")
await chat.state.stopDesktopChat()
await tick()
process.stderr.write("checkpoint 1108\n")
assert.equal(chat.state.desktopBusy, false)
process.stderr.write("checkpoint 1109\n")
assert.match(
  nodeText(findNodes(chat.root, "quick-switch-speech")[0]),
  /已停止回复/
)
chat.state.closeDesktopChat()
completeChat("关闭后迟到的消息")
await tick()
process.stderr.write("checkpoint 1116\n")
assert.equal(chat.state.desktopChatVisible, false)
beginChat()
await tick()
// 四角与侧边中部的气泡都在原生窗口内，侧边气泡底部高于桌宠中心。
for (const center of [
  { x: 64, y: 64 },
  { x: 496, y: 64 },
  { x: 64, y: 496 },
  { x: 496, y: 496 },
  { x: 64, y: 280 },
  { x: 496, y: 280 },
  { x: 280, y: 280 }
]) {
  chat.state.centerPosition = center
  await tick()
  const { side, style } = chat.state.desktopChatLayout
  const width = parseFloat(style.width)
  const height = parseFloat(style.maxHeight)
  const left = parseFloat(style.left)
  const top = style.top
    ? parseFloat(style.top)
    : 560 - parseFloat(style.bottom) - height
  assert.ok(left >= 12 && left + width <= 548)
  assert.ok(top >= 12 && top + height <= 548)
  if (["left", "right"].includes(side) && center.y >= 104)
    assert.ok(top + height <= center.y - 32, "气泡抬到头部斜上方")
}
// 桌面速记接入相同的思考动作和回复气泡，取消走速记接口，不占用聊天运行状态。
noteSnapshot = {
  runId: "note-1",
  requestId: "note-request",
  operation: "quick-note",
  status: "thinking",
  surface: "desktop",
  replyAtPet: true,
  text: ""
}
for (const callback of noteListeners) callback(noteSnapshot)
await tick()
process.stderr.write("checkpoint 1147\n")
assert.equal(chat.state.petAction, "think")
process.stderr.write("checkpoint 1148\n")
assert.equal(chat.state.desktopBusy, true)
process.stderr.write("checkpoint 1149\n")
assert.equal(chat.state.desktopRunId, "note-1")
const savedAssistantState = chat.state.assistantState
emitAssistant({
  type: "state",
  state: { ...lastChatTask, text: "旧聊天更新", status: "working" }
})
await tick()
process.stderr.write("checkpoint 1153\n")
assert.equal(chat.state.desktopRunId, "note-1", "旧聊天快照不能抢占速记气泡")
await chat.state.stopDesktopChat()
await tick()
process.stderr.write("checkpoint 1156\n")
assert.equal(chat.state.desktopBusy, false)
process.stderr.write("checkpoint 1157\n")
assert.ok(
  calls.some(
    (call) =>
      call.name === "quick-notes:capture-cancel" &&
      call.payload.requestId === "note-request"
  )
)
process.stderr.write("checkpoint 1158\n")
assert.match(chat.state.desktopChatError, /原文已保留/)
noteSnapshot = { ...noteSnapshot, runId: "note-2", status: "thinking" }
for (const callback of noteListeners) callback(noteSnapshot)
await tick()
noteSnapshot = {
  ...noteSnapshot,
  status: "completed",
  text: "已记录：排查提醒\n分类：问题排查",
  noteId: "saved-note",
  finishedAt: Date.now()
}
for (const callback of noteListeners) callback(noteSnapshot)
await tick()
process.stderr.write("checkpoint 1165\n")
assert.equal(chat.state.desktopReply, noteSnapshot.text)
process.stderr.write("checkpoint 1166\n")
assert.equal(chat.state.desktopBusy, false)
process.stderr.write("checkpoint 1167\n")
assert.match(
  nodeText(findNodes(chat.root, "quick-switch-speech")[0]),
  /查看便签/
)
process.stderr.write("checkpoint 1168\n")
assert.equal(
  chat.state.assistantState.operation,
  savedAssistantState.operation,
  "速记不能覆盖聊天状态"
)

chatState.appSettings.agents.assistant.enabled = false
await tick()
process.stderr.write("checkpoint 1172\n")
assert.equal(chat.state.desktopChatVisible, false)
chatState.appSettings.agents.assistant.enabled = true
chat.state.handleVisibilityChange({ visible: false })
beginChat()
await tick()
process.stderr.write("checkpoint 1177\n")
assert.equal(chat.state.desktopChatVisible, false, "隐藏期间不展开")
chat.unmount()
process.stderr.write("checkpoint 1179\n")
assert.deepEqual(chat.errors, [])
process.stderr.write("checkpoint 1180\n")
assert.deepEqual(chat.warnings, [])
assistantSnapshot = { status: "idle" }

// 独立输入窗支持键盘、模型校验、流式回复与关闭竞态。
handlers.bootstrapQuickSwitch = () => chatState
handlers.onStateChangedReady = (callback) => {
  appStateListeners.add(callback)
  return () => appStateListeners.delete(callback)
}
handlers.desktopReady = () => ({ visible: true })
handlers.desktopClose = () => true
handlers.desktopSend = (message) => {
  beginChat()
  emitAssistant({
    type: "text-delta",
    runId: lastChatTask.runId,
    text: "先到的流"
  })
  return { ...lastChatTask }
}
process.stderr.write("checkpoint 1200\n")
const DesktopInput = loadComponent(
  "src/features/assistant/components/DesktopInput.vue"
)
const inputWindow = mount(DesktopInput)
await tick()
process.stderr.write("checkpoint 1205\n")
assert.equal(inputWindow.state.visible, true)
process.stderr.write("checkpoint 1206\n")
assert.equal(
  vue.toRaw(document.activeElement),
  findNodes(inputWindow.root, "desktop-assistant-textarea")[0]
)
inputWindow.state.draft = "你好"
const sendCount = () =>
  calls.filter((call) => call.name === "desktopSend").length
let count = sendCount()
for (const flags of [{ isComposing: true }, { shiftKey: true }]) {
  inputWindow.state.handleKeydown({
    key: "Enter",
    ...flags,
    preventDefault() {
      assert.fail("应保留输入法或换行")
    }
  })
}
process.stderr.write("checkpoint 1223\n")
assert.equal(sendCount(), count)
chatState.appSettings.agents.assistant.targetId = "local"
await inputWindow.state.send()
process.stderr.write("checkpoint 1226\n")
assert.match(inputWindow.state.error, /本地助手模型待添加/)
process.stderr.write("checkpoint 1227\n")
assert.equal(sendCount(), count)
chatState.appSettings.agents.assistant.targetId = "p1"
await inputWindow.state.send()
await tick()
process.stderr.write("checkpoint 1231\n")
assert.equal(inputWindow.state.visible, true, "发送后保留输入窗，用户手动关闭")
process.stderr.write("checkpoint 1232\n")
assert.equal(inputWindow.state.draft, "")
process.stderr.write("checkpoint 1233\n")
assert.equal(inputWindow.state.task.text, "先到的流", "发送回包不能覆盖流")
completeChat("后台继续完成")
await tick()
for (const callback of desktopOpenListeners) await callback()
process.stderr.write("checkpoint 1237\n")
assert.equal(inputWindow.state.visible, true)
handlers.desktopSend = () => {
  throw new Error("连接暂时不可用")
}
inputWindow.state.draft = "失败后保留"
await inputWindow.state.send()
process.stderr.write("checkpoint 1243\n")
assert.equal(inputWindow.state.draft, "失败后保留")
process.stderr.write("checkpoint 1244\n")
assert.match(inputWindow.state.error, /连接暂时不可用/)
let finishSend
handlers.desktopSend = () =>
  new Promise((resolve) => {
    finishSend = resolve
  })
const pendingSend = inputWindow.state.send()
await tick()
await inputWindow.state.close()
for (const callback of desktopOpenListeners) await callback()
finishSend({ status: "thinking", runId: "stale", replyAtPet: false })
await pendingSend
process.stderr.write("checkpoint 1256\n")
assert.equal(
  inputWindow.state.showResponse,
  false,
  "关闭重开后旧回包不切换界面"
)
process.stderr.write("checkpoint 1261\n")
assert.equal(inputWindow.state.draft, "失败后保留")
handlers.desktopSend = () => {
  beginChat({ replyAtPet: false })
  return { ...lastChatTask }
}
await inputWindow.state.send()
process.stderr.write("checkpoint 1267\n")
assert.equal(inputWindow.state.showResponse, true)
process.stderr.write("checkpoint 1268\n")
assert.equal(
  findNodes(inputWindow.root, "desktop-assistant-textarea").length,
  1,
  "无桌宠回复时也保留输入框"
)
emitAssistant({
  type: "text-delta",
  runId: lastChatTask.runId,
  text: "无桌宠时也流式显示"
})
await tick()
process.stderr.write("checkpoint 1279\n")
assert.match(
  nodeText(findNodes(inputWindow.root, "desktop-assistant-response")[0]),
  /无桌宠时也流式显示/
)
completeChat("轻量回复完成")
await tick()
const timerCount = desktopTimers.length
await tick()
process.stderr.write("checkpoint 1287\n")
assert.equal(desktopTimers.length, timerCount, "输入窗不注册自动关闭计时器")
process.stderr.write("checkpoint 1288\n")
assert.equal(inputWindow.state.visible, true)
await inputWindow.state.close()
process.stderr.write("checkpoint 1290\n")
assert.equal(inputWindow.state.visible, false)
inputWindow.unmount()
process.stderr.write("checkpoint 1292\n")
assert.equal(desktopOpenListeners.size, 0)
process.stderr.write("checkpoint 1293\n")
assert.equal(appStateListeners.size, 0)
process.stderr.write("checkpoint 1294\n")
assert.deepEqual(inputWindow.errors, [])
process.stderr.write("checkpoint 1295\n")
assert.deepEqual(inputWindow.warnings, [])
for (const timer of desktopTimers) clearTimeout(timer.handle)
assistantSnapshot = { status: "idle" }

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
process.stderr.write("checkpoint 1315\n")
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
  assert.equal(
    findNodes(edgePanel.root, "quick-switch-tool").length,
    edgePanel.state.visibleTools.length
  )
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
process.stderr.write("checkpoint 1387\n")
assert.equal(edgePanel.state.hovered, true)
process.stderr.write("checkpoint 1388\n")
assert.equal(edgePanel.state.petAction, "salute")
process.stderr.write("checkpoint 1389\n")
assert.equal(edgePanel.state.expanded, true)
edgePanel.state.leavePanel()
await tickDelay(300)
process.stderr.write("checkpoint 1392\n")
assert.equal(edgePanel.state.petAction, "peek")
edgePanel.state.handlePointerMove()
await tickDelay(130)
process.stderr.write("checkpoint 1395\n")
assert.equal(edgePanel.state.expanded, true)
process.stderr.write("checkpoint 1396\n")
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
process.stderr.write("checkpoint 1411\n")
assert.equal(edgePanel.state.screenEdge, "")
process.stderr.write("checkpoint 1412\n")
assert.equal(edgePanel.state.petAction, "")
process.stderr.write("checkpoint 1413\n")
assert.ok(
  findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/idle.webp"
  )
)
edgePanel.state.enterPanel()
await tick()
process.stderr.write("checkpoint 1420\n")
assert.ok(
  findNodes(edgePanel.root, "desktop-pet-sheet")[0].props.href.endsWith(
    "/salute.webp"
  )
)
edgeState.appSettings.desktop.enabled = false
await tick()
process.stderr.write("checkpoint 1427\n")
assert.equal(findNodes(edgePanel.root, "quick-switch-center-pet").length, 0)
assertNoHoverTitles(findNodes(edgePanel.root, "quick-switch-center")[0])
edgePanel.unmount()
process.stderr.write("checkpoint 1430\n")
assert.deepEqual(edgePanel.errors, [])
process.stderr.write("checkpoint 1431\n")
assert.deepEqual(edgePanel.warnings, [])
handlers.setQuickSwitchCollapsed = () => true
handlers.moveQuickSwitchBy = () => true

// 完整周期轮换、拖动暂停、切换桌宠和卸载清理计时器。
const { desktopPets, getDesktopPet } = modules.get("@/constants/desktop-pets")
const spriteConfig = JSON.parse(
  readFileSync("src/assets/pets/monkey_animations/sprite_config.json", "utf8")
)
for (const animation of desktopPets[0].animations) {
  assert.ok(animation.url.endsWith("/" + animation.id + ".webp"))
  assert.ok(spriteConfig.actions[animation.id])
}
process.stderr.write("checkpoint 1444\n")
assert.equal(getDesktopPet("").id, desktopPets[0].id)
process.stderr.write("checkpoint 1445\n")
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
process.stderr.write("checkpoint 1474\n")
assert.equal(timers.size, 1)
process.stderr.write("checkpoint 1475\n")
assert.equal(frameTimers.size, 1)
// 图片在隐藏时保留，暂停后不推进帧；恢复播放不需要再次加载图片。
pet.component.props.paused = true
await tick()
process.stderr.write("checkpoint 1479\n")
assert.equal(timers.size, 0)
process.stderr.write("checkpoint 1480\n")
assert.equal(frameTimers.size, 0)
const hiddenFrame = pet.state.frameIndex
pet.state.startAnimation()
pet.state.rotateAnimation()
process.stderr.write("checkpoint 1484\n")
assert.equal(pet.state.frameIndex, hiddenFrame)
process.stderr.write("checkpoint 1485\n")
assert.equal(pet.state.animation.id, firstAnimation.id)
process.stderr.write("checkpoint 1486\n")
assert.equal(frameTimers.size, 0)
pet.component.props.paused = false
await tick()
process.stderr.write("checkpoint 1489\n")
assert.equal(timers.size, 1)
process.stderr.write("checkpoint 1490\n")
assert.equal(frameTimers.size, 1)
const timer = [...timers.values()][0]
process.stderr.write("checkpoint 1492\n")
assert.ok(timer.ms >= 4000)
process.stderr.write("checkpoint 1493\n")
assert.equal(timer.ms % firstAnimation.duration, 0)
timer.callback()
await tick()
process.stderr.write("checkpoint 1496\n")
assert.equal(pet.state.animation.id, desktopPets[0].animations[1].id)
process.stderr.write("checkpoint 1497\n")
assert.equal(timers.size, 0)
process.stderr.write("checkpoint 1498\n")
assert.equal(frameTimers.size, 0)
for (let index = 1; index < desktopPets[0].animations.length; index++) {
  pet.state.rotateAnimation()
  await tick()
}
process.stderr.write("checkpoint 1503\n")
assert.equal(pet.state.animation.id, firstAnimation.id)
pet.component.props.dragging = true
await tick()
process.stderr.write("checkpoint 1506\n")
assert.equal(pet.state.animation.id, "walk")
pet.state.scheduleAnimation()
process.stderr.write("checkpoint 1508\n")
assert.equal(timers.size, 0)
const pausedIndex = pet.state.animationIndex
pet.state.rotateAnimation()
process.stderr.write("checkpoint 1511\n")
assert.equal(pet.state.animationIndex, pausedIndex)
pet.component.props.dragging = false
await tick()
pet.state.scheduleAnimation()
process.stderr.write("checkpoint 1515\n")
assert.equal(timers.size, 1)
pet.state.rotateAnimation()
await tick()
process.stderr.write("checkpoint 1518\n")
assert.equal(timers.size, 0)
pet.state.animationIndex = desktopPets[0].animations.findIndex(
  (animation) => animation.id === "walk"
)
await tick()
process.stderr.write("checkpoint 1523\n")
assert.equal(pet.state.animation.id, "walk")
pet.state.scheduleAnimation()
process.stderr.write("checkpoint 1525\n")
assert.equal(timers.size, 1)
pet.component.props.dragging = true
await tick()
process.stderr.write("checkpoint 1528\n")
assert.equal(timers.size, 0)
process.stderr.write("checkpoint 1529\n")
assert.equal(frameTimers.size, 0)
pet.component.props.dragging = false
await tick()
process.stderr.write("checkpoint 1532\n")
assert.equal(timers.size, 1)
pet.component.props.pet = { ...desktopPets[0], id: "another-pet" }
await tick()
process.stderr.write("checkpoint 1535\n")
assert.equal(pet.state.animationIndex, 0)
pet.state.scheduleAnimation()
process.stderr.write("checkpoint 1537\n")
assert.equal(timers.size, 1)
pet.unmount()
process.stderr.write("checkpoint 1539\n")
assert.equal(timers.size, 0)
pet.state.rotateAnimation()
process.stderr.write("checkpoint 1541\n")
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
process.stderr.write("checkpoint 1595\n")
assert.equal(timers.size, 1)
lockedPreview.component.props.action = "salute"
await tick()
process.stderr.write("checkpoint 1598\n")
assert.equal(timers.size, 0)
process.stderr.write("checkpoint 1599\n")
assert.equal(lockedPreview.state.animation.id, "salute")
findNodes(lockedPreview.root, "desktop-pet-sheet")[0].props.onLoad()
const saluteFrameTimer = [...frameTimers.values()][0]
saluteFrameTimer.callback()
await tick()
process.stderr.write("checkpoint 1604\n")
assert.equal(lockedPreview.state.frameIndex, 1)
lockedPreview.component.props.action = "peek"
await tick()
process.stderr.write("checkpoint 1607\n")
assert.equal(lockedPreview.state.frameIndex, 0)
process.stderr.write("checkpoint 1608\n")
assert.equal(frameTimers.size, 0)
lockedPreview.unmount()
globalThis.setTimeout = originalSetTimeout
globalThis.clearTimeout = originalClearTimeout
globalThis.setInterval = originalSetInterval
globalThis.clearInterval = originalClearInterval
process.stderr.write("checkpoint 1614\n")
assert.deepEqual(pet.errors, [])
process.stderr.write("checkpoint 1615\n")
assert.deepEqual(pet.warnings, [])

// 设置与桌宠的完整保存交互由 test-settings.mjs 验证，这里只检查共用工具枚举。
const SystemSettings = loadComponent(
  "src/features/settings/components/SystemSettings.vue"
)
const floatingSettings = { quickSwitchVisible: true, quickSwitchTools: {} }
const savedSettings = []
const settings = mount(SystemSettings, {
  settings: floatingSettings,
  cliTargets: fixture().cliTargets,
  pending: false,
  saveSettings: async (patch) => {
    savedSettings.push(patch)
    Object.assign(
      floatingSettings.quickSwitchTools,
      patch.system.quickSwitchTools
    )
    return true
  }
})
await tick()
process.stderr.write("checkpoint 1637\n")
assert.equal(
  findNodes(settings.root, "system-settings-tool").length,
  settings.state.floatingToolItems.length
)
process.stderr.write("checkpoint 1641\n")
assert.ok(
  !settings.state.floatingToolItems.some((tool) => tool.id === "screenshot")
)
await settings.state.saveSetting("quickSwitchTools", false, "git")
process.stderr.write("checkpoint 1645\n")
assert.deepEqual(savedSettings.at(-1), {
  system: { quickSwitchTools: { git: false } }
})
settings.component.props.settings = {
  ...floatingSettings,
  quickSwitchVisible: false
}
await tick()
process.stderr.write("checkpoint 1653\n")
assert.equal(findNodes(settings.root, "system-settings-tool").length, 0)
settings.component.props.settings = {
  ...floatingSettings,
  quickSwitchVisible: true
}
await tick()
process.stderr.write("checkpoint 1659\n")
assert.equal(
  findNodes(settings.root, "system-settings-tool").length,
  settings.state.floatingToolItems.length
)
process.stderr.write("checkpoint 1663\n")
assert.equal(settings.state.draft.quickSwitchTools.git, false)
settings.unmount()
process.stderr.write("checkpoint 1665\n")
assert.deepEqual(settings.errors, [])
process.stderr.write("checkpoint 1666\n")
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
process.stderr.write("checkpoint 1680\n")
assert.equal(tools.state.activeTool, "git")
process.stderr.write("checkpoint 1681\n")
assert.equal(detailChanges.at(-1), true)
process.stderr.write("checkpoint 1682\n")
assert.equal(handledRequests, 1)
tools.component.props.toolRequest = null
await tick()
process.stderr.write("checkpoint 1685\n")
assert.equal(tools.state.activeTool, "git")
tools.state.closeTool()
await tick()
tools.component.props.repos = [{ id: "new" }]
await tick()
process.stderr.write("checkpoint 1690\n")
assert.equal(tools.state.activeTool, "")
tools.component.props.toolRequest = { toolId: "git" }
await tick()
process.stderr.write("checkpoint 1693\n")
assert.equal(tools.state.activeTool, "git")
process.stderr.write("checkpoint 1694\n")
assert.equal(handledRequests, 2)
tools.component.props.toolRequest = { toolId: "missing" }
await tick()
process.stderr.write("checkpoint 1697\n")
assert.equal(tools.state.activeTool, "git")
tools.unmount()
process.stderr.write("checkpoint 1699\n")
assert.deepEqual(tools.errors, [])
process.stderr.write("checkpoint 1700\n")
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
process.stderr.write("checkpoint 1716\n")
assert.deepEqual(appErrors, [])
const appScript = compileScript(appDescriptor, { id: "app-check" })
const appTemplate = compileTemplate({
  source: appDescriptor.template.content,
  filename: "src/App.vue",
  id: "app-check",
  compilerOptions: { bindingMetadata: appScript.bindings }
})
process.stderr.write("checkpoint 1724\n")
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
process.stderr.write("checkpoint 1776\n")
assert.equal(quickReady.value, false)
process.stderr.write("checkpoint 1777\n")
assert.deepEqual(bootstrapCalls, [])
finishStateSubscription()
await initializing
process.stderr.write("checkpoint 1780\n")
assert.equal(quickReady.value, true)
process.stderr.write("checkpoint 1781\n")
assert.deepEqual(bootstrapCalls, ["snapshot"])
process.stderr.write("checkpoint 1782\n")
assert.equal(snapshots[0].appSettings.desktop.enabled, true)
stateListener({ appSettings: { desktop: { enabled: false } } })
process.stderr.write("checkpoint 1784\n")
assert.equal(snapshots.at(-1).appSettings.desktop.enabled, false)
initialized.stop()
process.stderr.write("checkpoint 1786\n")
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
process.stderr.write("checkpoint 1823\n")
assert.equal(mainReady.value, true)
process.stderr.write("checkpoint 1824\n")
assert.equal(mainLoading, false)
process.stderr.write("checkpoint 1825\n")
assert.equal(mainBootstrapped, true)
process.stderr.write("checkpoint 1826\n")
assert.equal(typeof finishBackupListing, "function")
finishBackupListing()
await mainInitializing

console.log(
  "浮窗预热、桌宠气泡位置与十秒收起、独立输入窗键盘与关闭竞态、流式回复、拖动及页面跳转检查通过"
)

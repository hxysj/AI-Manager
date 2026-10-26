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

// 使用真实主界面和公告组件验证启动、关闭与重开，仅模拟原生接口。
const require = createRequire(import.meta.url)
const vue = require("vue")
const calls = []
const errors = []
const renderErrors = []
const subscriptions = new Map()
let read = false
let saveError = false
let bootError = false
let finishSave = null
let updateResponse = { phase: "not-available", manual: true }
let finishUpdateStatus = null
const announcement = {
  version: "9.0.0",
  releaseNotes: [
    "## 9.0.0 变更说明",
    "",
    "### 截图改进",
    "- 优化截图速度。",
    "- 保存到 `screenshots` 目录。",
    "### 界面修复",
    "- <img src=x onerror=alert(1)>"
  ].join("\n")
}
const api = new Proxy(
  {},
  {
    get: (_, name) => {
      if (name.startsWith("on"))
        return (callback) => {
          subscriptions.set(name, callback)
          return () => subscriptions.delete(name)
        }
      return async () => {
        calls.push(name)
        if (name === "bootstrap" && bootError) throw new Error("启动失败")
        if (name === "getReleaseAnnouncement") return read ? null : announcement
        if (name === "dismissReleaseAnnouncement") {
          if (saveError) throw new Error("已读状态保存失败")
          if (finishSave)
            await new Promise((resolve) => {
              finishSave.resolve = resolve
            })
          read = true
          return null
        }
        if (name === "getUpdateStatus")
          return finishUpdateStatus
            ? new Promise((resolve) => {
                finishUpdateStatus.resolve = resolve
              })
            : { phase: "idle" }
        if (name === "checkForUpdates") return updateResponse
        if (name === "downloadUpdate")
          return { phase: "downloaded", version: "9.0.1", manual: true }
        return {}
      }
    }
  }
)
const placeholder = { render: () => vue.h("div") }
const modules = new Map()
const styles = []
function loadComponent(filename) {
  if (modules.has(filename)) return modules.get(filename)
  const { descriptor, errors: parseErrors } = parse(
    readFileSync(filename, "utf8"),
    { filename }
  )
  assert.deepEqual(parseErrors, [])
  const script = compileScript(descriptor, {
    id: filename,
    genDefaultAs: "__component"
  })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename,
    id: filename,
    compilerOptions: { bindingMetadata: script.bindings, hoistStatic: false }
  })
  assert.deepEqual(template.errors, [])
  styles.push(...descriptor.styles.map((style) => ({ filename, style })))
  const code = transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`.replace(
      /import\(("[^"]+")\)/g,
      "loadAsync($1)"
    ),
    { format: "cjs" }
  ).code
  const localRequire = (name) => {
    if (name === "@/api") return new Proxy({}, { get: () => api })
    if (name === "@/utils/global-loading")
      return {
        useGlobalLoading: () => ({
          loading: vue.ref(false),
          withGlobalLoading: (action) => action()
        })
      }
    if (name === "@/utils/message")
      return { createMessage: { error: (error) => errors.push(error) } }
    if (name === "@tauri-apps/api/core") return { isTauri: () => false }
    if (name === "@tauri-apps/api/app") return { setTheme: async () => {} }
    if (name.endsWith(".vue"))
      return {
        __esModule: true,
        default: name.endsWith("/BaseModal.vue")
          ? loadComponent("src/components/BaseModal.vue")
          : name.endsWith("/AppUpdateModal.vue")
            ? loadComponent("src/components/AppUpdateModal.vue")
            : placeholder
      }
    return require(name)
  }
  const module = { exports: {} }
  new Function("require", "module", "exports", "loadAsync", code)(
    localRequire,
    module,
    module.exports,
    async (name) =>
      name.endsWith("/AppAnnouncementModal.vue")
        ? loadComponent("src/components/AppAnnouncementModal.vue")
        : placeholder
  )
  modules.set(filename, module.exports.default)
  return module.exports.default
}

function walk(node, predicate) {
  return [
    ...(predicate(node) ? [node] : []),
    ...node.children.flatMap((child) => walk(child, predicate))
  ]
}
function hostNode(type, text = "") {
  return {
    type,
    text,
    children: [],
    parent: null,
    props: {},
    isConnected: true,
    classList: { add() {}, remove() {} },
    addEventListener() {},
    focus() {
      document.activeElement = this
    },
    querySelectorAll() {
      return walk(
        this,
        (node) =>
          (node.type === "button" && !node.props.disabled) ||
          node.props.tabindex === "0"
      )
    }
  }
}
const body = hostNode("body")
globalThis.document = {
  body,
  documentElement: hostNode("html"),
  activeElement: null
}
document.documentElement.dataset = {}
globalThis.window = {
  location: { search: "" },
  localStorage: { getItem: () => null },
  addEventListener() {},
  removeEventListener() {}
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
  },
  querySelector: (target) => (target === "body" ? body : null),
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
const App = loadComponent("src/App.vue")
const tick = async () => {
  for (let index = 0; index < 20; index++) {
    await Promise.resolve()
    await vue.nextTick()
  }
  assert.deepEqual(renderErrors, [])
}
let mountedRoot = null
let mountedState = null
function mount(search = "") {
  window.location.search = search
  calls.length = 0
  const root = hostNode("root")
  const app = renderer.createApp(App)
  app.config.errorHandler = (error) => renderErrors.push(error.stack)
  app.mount(root)
  mountedRoot = root
  mountedState = app._instance.setupState
  return () => app.unmount()
}
const find = (className) =>
  walk(body, (node) =>
    String(node.props.class || "")
      .split(" ")
      .includes(className)
  )
const findUpdate = (className) =>
  walk(mountedRoot, (node) =>
    String(node.props.class || "")
      .split(" ")
      .includes(className)
  )

// 首次进入显示公告，展示本身不会标记已读；未确认退出后仍会提示。
let unmount = mount()
await tick()
assert.equal(find("app-announcement-modal").length, 1)
assert.equal(find("announcement-section-title").length, 2)
assert.ok(find("announcement-item").some((node) => node.text.includes("截图")))
assert.ok(
  find("announcement-item").some(
    (node) => node.text === "<img src=x onerror=alert(1)>"
  )
)
assert.equal(walk(body, (node) => node.type === "img").length, 0)
assert.equal(calls.includes("dismissReleaseAnnouncement"), false)
unmount()
await tick()
assert.equal(find("app-announcement-modal").length, 0)
unmount = mount()
await tick()

// 保存失败保留公告，并允许重试；重复点击只保存一次。
saveError = true
find("announcement-confirm")[0].props.onClick()
await tick()
assert.equal(find("app-announcement-modal").length, 1)
assert.ok(errors.includes("已读状态保存失败"))
saveError = false
finishSave = {}
const beforeSave = calls.filter(
  (name) => name === "dismissReleaseAnnouncement"
).length
find("announcement-confirm")[0].props.onClick()
find("announcement-close")[0].props.onClick()
await tick()
assert.equal(
  calls.filter((name) => name === "dismissReleaseAnnouncement").length,
  beforeSave + 1
)
assert.equal(find("announcement-confirm")[0].props.disabled, true)
finishSave.resolve()
finishSave = null
await tick()
assert.equal(find("app-announcement-modal").length, 0)
unmount()
unmount = mount()
await tick()
assert.equal(find("app-announcement-modal").length, 0)
unmount()

// 关闭按钮、遮罩和 Esc 与确认具有相同的已读语义。
for (const action of ["close", "backdrop", "escape"]) {
  read = false
  unmount = mount()
  await tick()
  if (action === "close") find("announcement-close")[0].props.onClick()
  if (action === "backdrop") find("base-modal__backdrop")[0].props.onClick()
  if (action === "escape")
    find("app-announcement-modal")[0].props.onKeydown({
      key: "Escape",
      preventDefault() {},
      stopPropagation() {}
    })
  await tick()
  assert.equal(read, true)
  assert.equal(find("app-announcement-modal").length, 0)
  unmount()
}

// 其他模态弹框临时出现时不能误标记已读。
read = false
unmount = mount()
await tick()
subscriptions.get("onCloseRequested")()
await tick()
assert.equal(find("app-announcement-modal").length, 0)
assert.equal(read, false)
unmount()

// 工具窗口、悬浮窗、初始化失败均不查询公告。
for (const search of ["?panel=tool", "?panel=quick-switch", ""]) {
  bootError = !search
  unmount = mount(search)
  await tick()
  assert.equal(calls.includes("getReleaseAnnouncement"), false)
  assert.equal(find("app-announcement-modal").length, 0)
  unmount()
}

// 原生后台推送新版本即可弹框，无更新与自动检测失败均不打扰用户。
bootError = false
read = false
unmount = mount()
await tick()
const updateStatus = (status) => subscriptions.get("onUpdateStatus")(status)
const beforeUpdateErrors = errors.length
for (const phase of ["checking", "not-available", "error"]) {
  updateStatus({ phase, manual: false, message: "自动检测结果" })
  await tick()
  assert.equal(findUpdate("app-update-modal").length, 0)
}
assert.equal(errors.length, beforeUpdateErrors)
updateStatus({
  phase: "available",
  version: "9.0.0",
  manual: false,
  releaseNotes: "新版本说明"
})
await tick()
assert.equal(findUpdate("app-update-modal").length, 1)
assert.equal(findUpdate("app-update-notes")[0].text, "新版本说明")
assert.equal(find("app-announcement-modal").length, 0)
assert.equal(read, false)
assert.equal(calls.includes("checkForUpdates"), false)

// “稍后”抑制本次运行中同一版本的自动提示，但更新的版本仍可再次提醒。
findUpdate("app-update-button")[0].props.onClick()
await tick()
assert.equal(calls.includes("dismissUpdate"), true)
assert.equal(findUpdate("app-update-modal").length, 0)
updateStatus({ phase: "available", version: "9.0.0", manual: false })
await tick()
assert.equal(findUpdate("app-update-modal").length, 0)
updateStatus({ phase: "available", version: "9.0.1", manual: false })
await tick()
assert.equal(findUpdate("app-update-modal").length, 1)
findUpdate("app-update-primary-button")[0].props.onClick()
await tick()
assert.equal(calls.includes("downloadUpdate"), true)
assert.equal(mountedState.updateDialog.phase, "downloaded")
assert.equal(
  findUpdate("app-update-primary-button")[0].text.trim(),
  "立即重启更新"
)
await mountedState.closeUpdateDialog()

// 手动检查仍能重新查看已暂缓的版本，并明确显示当前已是最新版本的结果。
updateResponse = { phase: "available", version: "9.0.0", manual: true }
await mountedState.checkForAppUpdates()
await tick()
assert.equal(findUpdate("app-update-modal").length, 1)
assert.equal(mountedState.updateDialog.version, "9.0.0")
await mountedState.closeUpdateDialog()
updateResponse = { phase: "not-available", manual: true }
await mountedState.checkForAppUpdates()
await tick()
assert.equal(findUpdate("app-update-modal").length, 1)
assert.equal(mountedState.updateDialog.phase, "not-available")
unmount()

// 启动快照迟到时，不能把自动推送的新版本弹框覆盖成 idle。
finishUpdateStatus = {}
unmount = mount()
await tick()
updateStatus({ phase: "available", version: "9.0.2", manual: false })
finishUpdateStatus.resolve({ phase: "idle" })
finishUpdateStatus = null
await tick()
assert.equal(findUpdate("app-update-modal").length, 1)
assert.equal(mountedState.updateDialog.version, "9.0.2")
unmount()

// 标题每累计点击十次切换日志入口，隐藏不清空日志或打断其他页面。
read = true
unmount = mount()
await tick()
assert.equal(mountedState.showLogsTab, false)
for (let index = 0; index < 9; index++)
  await mountedState.handleSidebarTitleClick()
assert.equal(mountedState.showLogsTab, false)
await mountedState.handleSidebarTitleClick()
await tick()
assert.equal(mountedState.showLogsTab, true)
assert.equal(mountedState.activeView, "logs")
assert.ok(mountedState.navItems.some((item) => item.id === "logs"))
assert.equal(calls.filter((name) => name === "getAppLogs").length, 1)
for (let index = 0; index < 9; index++)
  await mountedState.handleSidebarTitleClick()
assert.equal(mountedState.showLogsTab, true)
await mountedState.handleSidebarTitleClick()
await tick()
assert.equal(mountedState.showLogsTab, false)
assert.equal(mountedState.activeView, "providers")
assert.equal(
  mountedState.navItems.some((item) => item.id === "logs"),
  false
)
assert.equal(calls.filter((name) => name === "getAppLogs").length, 1)
assert.equal(calls.includes("clearAppLogs"), false)
for (let index = 0; index < 10; index++)
  await mountedState.handleSidebarTitleClick()
assert.equal(mountedState.showLogsTab, true)
assert.equal(calls.filter((name) => name === "getAppLogs").length, 2)
mountedState.activeView = "settings"
for (let index = 0; index < 10; index++)
  await mountedState.handleSidebarTitleClick()
await tick()
assert.equal(mountedState.showLogsTab, false)
assert.equal(mountedState.activeView, "settings")
assert.equal(mountedState.sidebarTitleClickCount, 0)
unmount()

for (const { filename, style } of styles) {
  const result = await compileStyleAsync({
    source: style.content,
    filename,
    id: filename,
    scoped: style.scoped,
    preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}
console.log(
  "公告与更新测试通过：公告已读、自动更新提示、同版本暂缓、后续版本提醒、手动检测、下载入口、日志入口切换及样式编译"
)

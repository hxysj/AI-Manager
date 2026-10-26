import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { resolve } from "node:path"
import { webcrypto } from "node:crypto"
import { createContext, runInContext } from "node:vm"
import {
  parse,
  compileScript,
  compileTemplate,
  compileStyleAsync
} from "@vue/compiler-sfc"
import { transformSync } from "esbuild"

// 用 Vue 原生运行态验证连接边界，替换 IPC，不扫描真实局域网或发起实际配对。
const require = createRequire(import.meta.url)
const vue = require("vue")
const calls = []
const listeners = {}
const candidate = {
  id: "candidate",
  name: "待连接设备",
  ip: "192.168.1.8",
  port: 17631,
  online: true,
  paired: false,
  groupSupported: true
}
const known = {
  id: "known",
  name: "已连接设备",
  ip: "192.168.1.9",
  port: 17631,
  online: true,
  paired: true,
  groupSupported: true
}
const legacy = {
  id: "legacy",
  name: "旧版客户端",
  ip: "192.168.1.10",
  port: 17631,
  online: true,
  paired: true,
  groupSupported: false
}
const fixture = {
  service: {
    running: true,
    lanIp: "192.168.1.1",
    port: 17631,
    lanIps: [],
    accessUrl: "",
    qrSvg: ""
  },
  native: {
    peers: [candidate, known, legacy],
    hidden: false,
    scanning: false
  },
  devices: [
    { id: "known", name: "已连接设备", native: true },
    { id: "legacy", name: "旧版客户端", native: true },
    { id: "old-unpaired", name: "旧未配对记录", native: true },
    { id: "browser", name: "已接入网页", native: false }
  ],
  sessions: [],
  groups: [],
  messages: []
}
const api = {
  getState: async () => structuredClone(fixture),
  scanDevices: async () => {
    calls.push("scan")
  },
  connectPeer: async (payload) => {
    calls.push({ connect: payload })
    fixture.native.peers[0].pairingCode = "123456"
    return structuredClone(fixture)
  },
  setHidden: async ({ hidden }) => {
    fixture.native.hidden = hidden
    return structuredClone(fixture.native)
  },
  deleteDevice: async (payload) => {
    calls.push({ delete: payload })
    return structuredClone(fixture)
  },
  onStateChanged: (callback) => {
    listeners.state = callback
    return () => {
      delete listeners.state
    }
  },
  onDevicesChanged: (callback) => {
    listeners.devices = callback
    return () => {
      delete listeners.devices
    }
  },
  onNativeChanged: (callback) => {
    listeners.native = callback
    return () => {
      delete listeners.native
    }
  }
}
const path = resolve("src/features/lanShare/index.vue")
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
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
for (const style of descriptor.styles) {
  const result = await compileStyleAsync({
    source: style.content,
    filename: path,
    id: path,
    scoped: style.scoped,
    preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}
const module = { exports: {} }
const code = transformSync(
  `${script.content}\n__component.render = () => null\nexport default __component`,
  { format: "cjs" }
).code
new Function("require", "module", "exports", code)(
  (name) => {
    if (name === "@/api") return { lanShareApi: api, systemApi: {} }
    if (name === "@/utils/message")
      return { createMessage: { success() {}, error() {}, info() {} } }
    if (name === "./utils")
      return {
        fileUrl() {
          return ""
        }
      }
    if (name.endsWith(".vue") || name.includes("/style/css")) return {}
    if (name === "element-plus") return {}
    return require(name)
  },
  module,
  module.exports
)
const renderer = vue.createRenderer({
  createElement: () => ({}),
  createText: () => ({}),
  createComment: () => ({}),
  setText() {},
  setElementText() {},
  parentNode() {},
  nextSibling() {},
  patchProp() {},
  insert() {},
  remove() {}
})
const vnode = vue.h(module.exports.default)
const errorsDuringRuntime = []
vnode.appContext = { ...vue.createApp({})._context }
vnode.appContext.config.errorHandler = (error) =>
  errorsDuringRuntime.push(error)
const root = {}
renderer.render(vnode, root)
const tick = async () => {
  await Promise.resolve()
  await vue.nextTick()
  await Promise.resolve()
  await vue.nextTick()
}
await tick()
const state = vnode.component.setupState
assert.deepEqual(state.connectedDevices.map((device) => device.id).sort(), [
  "browser",
  "known",
  "legacy"
])
assert.deepEqual(calls, ["scan"], "进入列表后主动探测设备在线状态")

// 网络地址切换后同步二维码；断网时清空旧二维码，恢复后使用新地址。
fixture.service.accessUrl = "http://192.168.1.1:17631/?token=stable"
fixture.service.qrSvg = "<svg>old-network</svg>"
listeners.state(structuredClone(fixture))
await state.showAccessDialog()
assert.equal(state.accessQrSvg, "<svg>old-network</svg>")
fixture.service.lanIp = "192.168.2.10"
fixture.service.lanIps = ["192.168.2.10"]
fixture.service.accessUrl = "http://192.168.2.10:17631/?token=stable"
fixture.service.qrSvg = "<svg>new-network</svg>"
listeners.state(structuredClone(fixture))
assert.equal(state.state.service.lanIp, "192.168.2.10")
assert.equal(state.accessQrSvg, "<svg>new-network</svg>")
fixture.service.lanIp = ""
fixture.service.lanIps = []
fixture.service.accessUrl = ""
fixture.service.qrSvg = ""
listeners.state(structuredClone(fixture))
assert.equal(state.accessQrSvg, "", "断网后不能继续显示旧网络的二维码")
fixture.service.lanIp = "192.168.3.10"
fixture.service.lanIps = ["192.168.3.10"]
fixture.service.accessUrl = "http://192.168.3.10:17631/?token=stable"
fixture.service.qrSvg = "<svg>reconnected-network</svg>"
listeners.state(structuredClone(fixture))
assert.equal(state.accessQrSvg, "<svg>reconnected-network</svg>")
state.accessDialogOpen = false

await state.openConnectDialog()
assert.equal(state.connectDialogOpen, true)
assert.deepEqual(calls, ["scan", "scan"])
assert.equal(state.discoveredPeers.length, 1)
state.connectIpFilter = "1.9"
assert.equal(state.discoveredPeers.length, 0)
state.connectIpFilter = "1.8"
assert.equal(state.discoveredPeers[0].id, "candidate")
await state.connectDiscoveredPeer(state.discoveredPeers[0])
assert.equal(calls.at(-1).connect.peerId, "candidate")
assert.equal(state.discoveredPeers[0].pairingCode, "123456")
assert.equal(
  state.connectedDevices.some((device) => device.id === "candidate"),
  false
)
assert.equal(state.connectDialogOpen, true)
const count = calls.length
await state.connectDiscoveredPeer(state.discoveredPeers[0])
assert.equal(calls.length, count)

// 拒绝、超时都不产生已连接节点；只有对端确认事件才能把候选转入主列表。
delete fixture.native.peers[0].pairingCode
fixture.native.error = "对方拒绝了连接请求"
listeners.state(structuredClone(fixture))
assert.equal(
  state.connectedDevices.some((device) => device.id === "candidate"),
  false
)
fixture.native.error = "连接确认已过期"
listeners.state(structuredClone(fixture))
assert.equal(
  state.connectedDevices.some((device) => device.id === "candidate"),
  false
)
fixture.native.peers.push({
  id: "removed",
  name: "已被对方删除",
  ip: "192.168.1.10",
  port: 17631,
  online: false,
  paired: false,
  remoteDeleted: true
})
listeners.state(structuredClone(fixture))
assert.equal(
  state.connectedDevices.some((device) => device.id === "removed"),
  true,
  "已被对方删除的设备仍保留本地历史入口"
)
fixture.native.peers[0].paired = true
fixture.devices.push({ id: "candidate", name: "新连接设备", native: true })
listeners.state(structuredClone(fixture))
assert.equal(
  state.connectedDevices.filter((device) => device.id === "candidate").length,
  1
)
assert.equal(state.discoveredPeers.length, 0)

await state.toggleHidden()
assert.equal(state.state.native.hidden, true)
assert.equal(
  state.connectedDevices.some((device) => device.id === "known"),
  true
)
await state.toggleHidden()
assert.equal(state.state.native.hidden, false)

// 高频发现只更新节点运行态，不能清空聊天记录或触发工作台历史重新加载。
state.state.messages = [{ id: "history" }]
const version = state.stateVersion
state.state.groups = [
  {
    id: "group",
    members: [
      { deviceId: "known", online: false },
      { deviceId: "browser", online: true }
    ]
  }
]
listeners.native({ ...structuredClone(fixture.native), hidden: true })
assert.equal(state.state.messages[0].id, "history")
assert.equal(state.stateVersion, version)
assert.equal(state.state.groups[0].members[0].online, true)
assert.equal(state.state.groups[0].members[1].online, true)
state.requestDeleteDevice(known)
assert.equal(state.deleteDeviceTarget.id, known.id)
await state.confirmDeleteDevice()
assert.equal(calls.at(-1).delete.deleteHistory, true)
assert.equal(state.deleteDeviceTarget, null)
state.requestDeleteDevice(known)
state.closeDeleteDevice()
assert.equal(state.deleteDeviceTarget, null)

// 验证创建群聊面板交互与新版原生客户端群聊能力。
state.openCreateGroup()
assert.equal(state.createGroupOpen, true)
assert.equal(state.newGroup.name, "新的群聊")
assert.deepEqual(state.newGroup.deviceIds, [])
const nativeGroupDevice = state.connectedDevices.find(
  (device) => device.id === "known"
)
const legacyGroupDevice = state.connectedDevices.find(
  (device) => device.id === "legacy"
)
state.toggleGroupDevice(legacyGroupDevice)
assert.deepEqual(state.newGroup.deviceIds, [], "旧版原生客户端仍然不能加入群聊")
state.toggleGroupDevice(nativeGroupDevice)
assert.deepEqual(
  state.newGroup.deviceIds,
  ["known"],
  "新版原生客户端可以加入群聊"
)
assert.deepEqual(state.selectedGroupDeviceIds, ["known"])
const browserGroupDevice = state.connectedDevices.find(
  (device) => device.id === "browser"
)
state.toggleGroupDevice(browserGroupDevice)
assert.deepEqual(state.newGroup.deviceIds, ["known", "browser"])
assert.deepEqual(state.selectedGroupDeviceIds, ["browser", "known"])
state.toggleGroupDevice(browserGroupDevice)
state.toggleGroupDevice(nativeGroupDevice)
assert.deepEqual(state.newGroup.deviceIds, [])
state.createGroupOpen = false

// 验证文件保存成功弹框与路径剪贴板交互
let copiedPath = ""
Object.defineProperty(globalThis.navigator, "clipboard", {
  value: {
    writeText: async (text) => {
      copiedPath = text
    }
  },
  configurable: true
})
state.savedFile = {
  name: "opencode-dev.zip",
  path: "D:\\GithubObject\\opencode-dev.zip",
  directory: "D:\\GithubObject"
}
assert.equal(state.savedPathCopied, false)
await state.copySavedFilePath()
assert.equal(state.savedPathCopied, true)
assert.equal(copiedPath, "D:\\GithubObject\\opencode-dev.zip")
state.savedFile = null

renderer.render(null, root)
assert.deepEqual(Object.keys(listeners), [])
assert.deepEqual(errorsDuringRuntime, [])

// 编译全局确认弹框入口及发送面板，并验证对端确认与“已送达”的边界。
for (const fileName of [
  "src/App.vue",
  "src/features/providers/components/ProviderDeleteConfirmModal.vue",
  "src/features/lanShare/components/LanShareMessagesPanel.vue",
  "src/features/lanShare/components/LanSharePreviewDialog.vue"
]) {
  const { descriptor, errors } = parse(readFileSync(fileName, "utf8"), {
    filename: fileName
  })
  assert.deepEqual(errors, [])
  const script = compileScript(descriptor, {
    id: fileName,
    genDefaultAs: "__component"
  })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename: fileName,
    id: fileName,
    compilerOptions: { bindingMetadata: script.bindings }
  })
  assert.deepEqual(template.errors, [])
  if (!fileName.endsWith("LanShareMessagesPanel.vue")) continue
  for (const style of descriptor.styles) {
    const result = await compileStyleAsync({
      source: style.content,
      filename: fileName,
      id: fileName,
      scoped: style.scoped,
      preprocessLang: style.lang
    })
    assert.deepEqual(result.errors, [])
  }
  let finishSend
  api.listMessages = async () => []
  api.sendMessage = () =>
    new Promise((resolve) => {
      finishSend = resolve
    })
  api.onTransferProgress = (callback) => {
    listeners.transfer = callback
    return () => {
      delete listeners.transfer
    }
  }
  api.onMessageCreated = (callback) => {
    listeners.message = callback
    return () => {
      delete listeners.message
    }
  }
  const module = { exports: {} }
  const code = transformSync(
    `${script.content}\n__component.render = () => null\nexport default __component`,
    { format: "cjs" }
  ).code
  new Function("require", "module", "exports", code)(
    (name) => {
      if (name === "@/api") return { lanShareApi: api, systemApi: {} }
      if (name === "@tauri-apps/api/core") return { isTauri: () => false }
      if (name === "@/utils/message")
        return {
          createMessage: {
            success() {},
            error: (error) => assert.fail(error),
            info() {}
          }
        }
      if (name === "@/utils/formatters") return { formatDateTime: () => "" }
      if (name === "@/features/lanShare/utils")
        return { formatFileSize: (value) => String(value) }
      if (
        name.endsWith(".vue") ||
        name.includes("/style/css") ||
        name === "element-plus"
      )
        return {}
      return require(name)
    },
    module,
    module.exports
  )
  const vnode = vue.h(module.exports.default, {
    currentDevice: { id: known.id, native: true },
    currentSessionId: "session",
    service: { running: true }
  })
  vnode.appContext = { ...vue.createApp({})._context }
  vnode.appContext.config.errorHandler = (error) =>
    errorsDuringRuntime.push(error)
  renderer.render(vnode, root)
  await tick()
  const panel = vnode.component.setupState
  assert.equal(
    panel.sendBlocked,
    false,
    "普通离线或已配对状态不能误判为对方删除"
  )
  panel.addPaths(["D:/example.bin"])
  panel.sendMessage()
  await tick()
  const task = panel.sendingTasks[0]
  assert.equal(task.status, "sending")
  assert.equal(task.progress, 0)
  assert.equal(task.files[0].transferred, false)
  listeners.transfer({
    messageId: task.id,
    fileIndex: 0,
    fileComplete: false,
    loadedBytes: 50,
    totalBytes: 100,
    bytesPerSecond: 25
  })
  assert.equal(task.progress, 50)
  assert.equal(task.bytesPerSecond, 25)
  listeners.transfer({ messageId: "other", loadedBytes: 100, totalBytes: 100 })
  assert.equal(task.progress, 50)
  listeners.transfer({
    messageId: task.id,
    fileIndex: 0,
    fileComplete: true,
    loadedBytes: 100,
    totalBytes: 100,
    bytesPerSecond: 25
  })
  assert.equal(task.files[0].transferred, true)
  assert.equal(task.progress, 99)
  assert.equal(task.status, "sending")
  finishSend([{ id: task.id, delivered: false }])
  await tick()
  assert.equal(task.statusText, "已保存，待对方接收")
  renderer.render(null, root)
  assert.deepEqual(Object.keys(listeners), [])

  // 对方删除本机后，输入和附件入口都不能创建发送任务。
  const blockedVnode = vue.h(module.exports.default, {
    currentDevice: { id: known.id, native: true, remoteDeleted: true },
    currentSessionId: "session",
    service: { running: true }
  })
  blockedVnode.appContext = { ...vue.createApp({})._context }
  renderer.render(blockedVnode, root)
  await tick()
  const blockedPanel = blockedVnode.component.setupState
  assert.equal(blockedPanel.sendBlocked, true)
  blockedPanel.addPaths(["D:/blocked.bin"])
  blockedPanel.sendMessage()
  assert.equal(blockedPanel.sendingTasks.length, 0)
  renderer.render(null, root)
}
assert.deepEqual(errorsDuringRuntime, [])

// 执行后端内嵌的扫码页脚本，沿用真实 HTTP 响应结构验证注册、上线和双向消息。
const guestPageSource = readFileSync(
  resolve("src-tauri/src/api/lan_share.rs"),
  "utf8"
)
  .split("fn mobile_page_html()")[1]
  .split("fn boxed_full(")[0]
const guestScript = guestPageSource.match(/<script>([\s\S]*?)<\/script>/)[1]

async function openGuestPage({
  groupsFail = false,
  registerFails = false,
  holdHistory = false
} = {}) {
  const requests = []
  const sockets = []
  const timers = new Map()
  const storage = new Map()
  const elements = new Map()
  const events = {}
  const viewportProperties = new Map()
  const copiedTexts = []
  let timerId = 0
  let failMessage = false
  let copyAllowed = true
  let selection = null
  let releaseHistory
  const element = () => ({
    value: "",
    textContent: "",
    style: {},
    dataset: {},
    children: [],
    disabled: false,
    classList: { add() {}, remove() {}, toggle() {} },
    append(...children) {
      this.children.push(...children)
    },
    appendChild(child) {
      this.children.push(child)
    },
    replaceChildren(...children) {
      this.children = children
    },
    events: {},
    addEventListener(name, callback) {
      this.events[name] = callback
    },
    setAttribute() {},
    select() {
      selection = this.value
    },
    setSelectionRange() {},
    remove() {},
    focus() {},
    click() {
      return this.onclick?.()
    },
    set innerHTML(value) {
      this.children = []
      this.html = value
    },
    get innerHTML() {
      return this.html || ""
    }
  })
  for (const [, id] of guestPageSource.matchAll(/\bid="([^"]+)"/g)) {
    elements.set(id, element())
  }
  const guest = createContext({
    URLSearchParams,
    Uint8Array,
    crypto: webcrypto,
    location: {
      search: "?token=guest-token",
      protocol: "http:",
      host: "192.168.1.1:17631",
      hostname: "192.168.1.1",
      port: "17631"
    },
    navigator: { userAgent: "Android Chrome" },
    localStorage: {
      getItem: (key) => storage.get(key) || null,
      setItem: (key, value) => storage.set(key, value)
    },
    document: {
      getElementById: (id) => {
        assert.ok(elements.has(id), `扫码页缺少元素 ${id}`)
        return elements.get(id)
      },
      querySelectorAll: () => [],
      createElement: element,
      documentElement: {
        setAttribute() {},
        style: {
          setProperty: (key, value) => viewportProperties.set(key, value)
        }
      },
      body: element(),
      execCommand: () => {
        if (copyAllowed) copiedTexts.push(selection)
        return copyAllowed
      },
      addEventListener: (name, callback) => {
        events[name] = callback
      }
    },
    innerHeight: 844,
    addEventListener: (name, callback) => {
      events[name] = callback
    },
    matchMedia: () => ({ matches: false }),
    setInterval: (callback) => {
      timers.set(++timerId, callback)
      return timerId
    },
    clearInterval: (id) => timers.delete(id),
    setTimeout: (callback) => {
      timers.set(++timerId, callback)
      return timerId
    },
    clearTimeout: (id) => timers.delete(id),
    WebSocket: class {
      static OPEN = 1
      constructor(url) {
        this.url = url
        this.sent = []
        this.readyState = 0
        sockets.push(this)
        Promise.resolve().then(() => {
          this.readyState = 1
          this.onopen?.()
        })
      }
      send(payload) {
        this.sent.push(JSON.parse(payload))
      }
      close() {
        this.readyState = 3
        this.onclose?.()
      }
    },
    fetch: async (path, options = {}) => {
      const body =
        options.body && typeof options.body === "string"
          ? JSON.parse(options.body)
          : options.body
      requests.push({ path, method: options.method || "GET", body })
      let data
      let message = ""
      if (path === "/api/devices") {
        data = { id: "guest-device", name: "扫码手机" }
        if (registerFails) message = "访问已失效，请重新扫码。"
      } else if (path === "/api/sessions") {
        data = {
          device: { id: "guest-device" },
          currentSession: { id: "new-guest-session" }
        }
      } else if (path.startsWith("/api/groups?")) {
        data = []
        if (groupsFail) message = "群聊列表暂不可用"
      } else if (path.startsWith("/api/files/upload?")) {
        data = { id: "guest-file" }
      } else if (path.startsWith("/api/files?")) {
        data = []
      } else if (
        path.startsWith("/api/messages?") &&
        options.method === "POST"
      ) {
        if (failMessage) message = "消息发送失败"
        data = {
          id: body.messageId,
          sessionId: body.sessionId,
          direction: "mobile-to-desktop",
          content: body.content
        }
      } else if (path.startsWith("/api/messages?")) {
        if (holdHistory)
          await new Promise((resolve) => {
            releaseHistory = resolve
          })
        data = { currentSession: { id: "guest-session" }, messages: [] }
      } else {
        assert.fail(`扫码页发起了未覆盖的请求：${path}`)
      }
      // 业务失败也返回 HTTP 200，确认页面按 status 判断结果并保留草稿。
      const response = {
        status: message ? "error" : "success",
        data: message ? null : data,
        message
      }
      return {
        ok: true,
        text: async () => JSON.stringify(response)
      }
    }
  })
  guest.window = guest
  runInContext(guestScript, guest)
  const settle = async () => {
    for (let index = 0; index < 30; index++) await Promise.resolve()
  }
  await settle()
  return {
    guest,
    requests,
    sockets,
    timers,
    storage,
    elements,
    events,
    viewportProperties,
    copiedTexts,
    settle,
    setMessageFailure(value) {
      failMessage = value
    },
    setCopyAllowed(value) {
      copyAllowed = value
    },
    releaseHistory() {
      releaseHistory()
    }
  }
}

const guestPage = await openGuestPage()
assert.equal(guestPage.storage.get("lanShareDeviceId"), "guest-device")
assert.equal(guestPage.elements.get("deviceName").value, "扫码手机")
assert.equal(guestPage.sockets.length, 1, "扫码访问后应自动建立在线连接")
assert.match(guestPage.sockets[0].url, /deviceId=guest-device/)
assert.equal(runInContext("currentSessionId", guestPage.guest), "guest-session")
assert.equal(guestPage.elements.get("status").textContent, "已连接电脑端服务")
const registrationIndex = guestPage.requests.findIndex(
  (item) => item.path === "/api/devices"
)
const sessionIndex = guestPage.requests.findIndex((item) =>
  item.path.startsWith("/api/messages?")
)
assert.ok(
  registrationIndex < sessionIndex,
  "注册后使用服务端确认的身份建立会话"
)
guestPage.sockets[0].onmessage({
  data: JSON.stringify({ type: "presence-request" })
})
assert.equal(guestPage.sockets[0].sent.at(-1).type, "presence")

guestPage.elements.get("messageInput").value = "手机发送消息"
await guestPage.elements.get("sendMessage").click()
const sentMessage = guestPage.requests.find(
  (item) => item.method === "POST" && item.path.startsWith("/api/messages?")
)
assert.equal(sentMessage.body.deviceId, "guest-device")
assert.equal(sentMessage.body.sessionId, "guest-session")
assert.equal(sentMessage.body.content, "手机发送消息")
assert.equal(guestPage.elements.get("messages").children.length, 1)
assert.equal(guestPage.elements.get("messageInput").value, "")
guestPage.sockets[0].onmessage({
  data: JSON.stringify({
    type: "message",
    message: { id: sentMessage.body.messageId, sessionId: "guest-session" }
  })
})
assert.equal(
  guestPage.elements.get("messages").children.length,
  1,
  "发送结果和实时回执不能重复显示"
)
guestPage.sockets[0].onmessage({
  data: JSON.stringify({
    type: "message",
    message: {
      id: "desktop-message",
      sessionId: "guest-session",
      content: "电脑回复"
    }
  })
})
assert.equal(
  guestPage.elements.get("messages").children[1].children[0].textContent,
  "电脑回复"
)

// 群聊加载失败时，扫码设备仍应上线并可发送单聊消息和附件。
const guestWithGroupFailure = await openGuestPage({ groupsFail: true })
assert.equal(guestWithGroupFailure.sockets.length, 1)
assert.equal(
  runInContext("currentSessionId", guestWithGroupFailure.guest),
  "guest-session"
)
runInContext(
  'addPendingAttachments([{ name: "扫码附件.txt" }])',
  guestWithGroupFailure.guest
)
guestWithGroupFailure.elements.get("messageInput").value = "附带文件"
await guestWithGroupFailure.elements.get("sendMessage").click()
const attachmentMessage = guestWithGroupFailure.requests.find(
  (item) => item.method === "POST" && item.path.startsWith("/api/messages?")
)
assert.deepEqual([...attachmentMessage.body.attachmentIds], ["guest-file"])
assert.equal(
  guestWithGroupFailure.elements.get("pendingAttachments").children.length,
  0
)

// 发送失败保留文本和同一个消息标识，重试成功后再清空草稿。
guestPage.setMessageFailure(true)
guestPage.elements.get("messageInput").value = "失败后重试"
await guestPage.elements.get("sendMessage").click()
assert.equal(guestPage.elements.get("messageInput").value, "失败后重试")
assert.match(
  guestPage.elements.get("status").textContent,
  /发送失败，草稿已保留/
)
const failedMessageId = guestPage.requests.at(-1).body.messageId
guestPage.setMessageFailure(false)
await guestPage.elements.get("sendMessage").click()
assert.equal(guestPage.requests.at(-1).body.messageId, failedMessageId)
assert.equal(guestPage.elements.get("messageInput").value, "")

// HTTP 访问、Clipboard API 拒绝和复制失败都应有正确反馈。
const incomingCopyButton =
  guestPage.elements.get("messages").children[1].children[1].children[0]
await incomingCopyButton.click()
assert.deepEqual(guestPage.copiedTexts, ["电脑回复"])
assert.equal(
  guestPage.elements.get("mobileToast").textContent,
  "已复制到剪贴板"
)
guestPage.guest.navigator.clipboard = {
  writeText: async () => {
    throw new Error("未授予剪贴板权限")
  }
}
await incomingCopyButton.click()
assert.deepEqual(guestPage.copiedTexts, ["电脑回复", "电脑回复"])
guestPage.setCopyAllowed(false)
await incomingCopyButton.click()
assert.equal(
  guestPage.elements.get("mobileToast").textContent,
  "复制失败，请长按消息文字选择复制"
)
guestPage.guest.navigator.clipboard.writeText = async (text) =>
  guestPage.copiedTexts.push(text)
await incomingCopyButton.click()
assert.equal(guestPage.copiedTexts.length, 3)

// 新建会话后只显示新会话的消息，不夹带上一会话的气泡。
await guestPage.elements.get("createSession").click()
assert.equal(
  runInContext("currentSessionId", guestPage.guest),
  "new-guest-session"
)
assert.equal(guestPage.elements.get("messages").children.length, 0)
guestPage.elements.get("messageInput").value = "新会话的消息"
await guestPage.elements.get("sendMessage").click()
assert.equal(guestPage.requests.at(-1).body.sessionId, "new-guest-session")
assert.equal(guestPage.elements.get("messages").children.length, 1)

// 手机回车用于换行，组合键发送；视口缩小时同步输入区可用高度。
let preventedEnter = false
guestPage.elements.get("messageInput").events.keydown({
  key: "Enter",
  preventDefault() {
    preventedEnter = true
  }
})
assert.equal(preventedEnter, false)
assert.equal(guestPage.viewportProperties.get("--viewport-height"), "844px")
guestPage.guest.visualViewport = { height: 420 }
guestPage.events.resize()
assert.equal(guestPage.viewportProperties.get("--viewport-height"), "420px")
guestPage.elements.get("messageInput").scrollHeight = 180
guestPage.elements.get("messageInput").events.input()
assert.equal(guestPage.elements.get("messageInput").style.height, "120px")
guestPage.events.pageshow()
assert.equal(guestPage.sockets.length, 1, "手机恢复前台时不能重复建立在线连接")
guestPage.sockets[0].close()
guestPage.events.pageshow()
await guestPage.settle()
assert.equal(guestPage.sockets.length, 2)
assert.match(guestPage.sockets[1].url, /deviceId=guest-device/)
guestPage.sockets[1].onmessage({
  data: JSON.stringify({ type: "deviceRemoved" })
})
assert.equal(guestPage.elements.get("sendMessage").disabled, true)
const requestCountAfterRemoval = guestPage.requests.length
guestPage.elements.get("messageInput").value = "已移除设备不能发送"
await guestPage.elements.get("sendMessage").click()
guestPage.events.pageshow()
assert.equal(guestPage.requests.length, requestCountAfterRemoval)
assert.equal(guestPage.sockets.length, 2)

const invalidGuest = await openGuestPage({ registerFails: true })
assert.equal(invalidGuest.sockets.length, 0, "鉴权失败不能登记设备上线")
assert.equal(invalidGuest.elements.get("sendMessage").disabled, true)
assert.equal(
  invalidGuest.elements.get("status").textContent,
  "访问已失效，请重新扫码。"
)
const guestWithSlowHistory = await openGuestPage({ holdHistory: true })
assert.equal(
  guestWithSlowHistory.sockets.length,
  1,
  "历史加载尚未完成时也应及时上线"
)
assert.equal(guestWithSlowHistory.elements.get("sendMessage").disabled, true)
guestWithSlowHistory.releaseHistory()
await guestWithSlowHistory.settle()
assert.equal(guestWithSlowHistory.elements.get("sendMessage").disabled, false)
console.log(
  "设备快传连接确认、IP 过滤、隐藏、删除、发送进度、扫码上线、双向消息、手机复制与输入、重连及 Vue/Less 编译检查通过"
)

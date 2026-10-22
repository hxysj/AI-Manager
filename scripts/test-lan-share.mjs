import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { resolve } from "node:path"
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
  deleteDevice: async payload => {
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
assert.equal(calls.length, 0)

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
assert.deepEqual(calls, ["scan"])
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
state.state.groups = [{
  id: "group",
  members: [
    { deviceId: "known", online: false },
    { deviceId: "browser", online: true }
  ]
}]
listeners.native({ ...structuredClone(fixture.native), hidden: true })
assert.equal(state.state.messages[0].id, "history")
assert.equal(state.stateVersion, version)
assert.equal(state.state.groups[0].members[0].online, true)
assert.equal(state.state.groups[0].members[1].online, true)
state.requestDeleteDevice(known)
assert.equal(state.deleteDeviceWithHistory, false)
await state.confirmDeleteDevice()
assert.equal(calls.at(-1).delete.deleteHistory, false)
state.requestDeleteDevice(known)
state.deleteDeviceWithHistory = true
await state.confirmDeleteDevice()
assert.equal(calls.at(-1).delete.deleteHistory, true)
state.requestDeleteDevice(known)
assert.equal(state.deleteDeviceWithHistory, false)
state.closeDeleteDevice()

// 验证创建群聊面板交互与新版原生客户端群聊能力。
state.openCreateGroup()
assert.equal(state.createGroupOpen, true)
assert.equal(state.newGroup.name, "新的群聊")
assert.deepEqual(state.newGroup.deviceIds, [])
const nativeGroupDevice = state.connectedDevices.find(device => device.id === "known")
const legacyGroupDevice = state.connectedDevices.find(device => device.id === "legacy")
state.toggleGroupDevice(legacyGroupDevice)
assert.deepEqual(state.newGroup.deviceIds, [], "旧版原生客户端仍然不能加入群聊")
state.toggleGroupDevice(nativeGroupDevice)
assert.deepEqual(state.newGroup.deviceIds, ["known"], "新版原生客户端可以加入群聊")
assert.deepEqual(state.selectedGroupDeviceIds, ["known"])
const browserGroupDevice = state.connectedDevices.find(device => device.id === "browser")
state.toggleGroupDevice(browserGroupDevice)
assert.deepEqual(state.newGroup.deviceIds, ["known", "browser"])
assert.deepEqual(state.selectedGroupDeviceIds, ["browser", "known"])
state.toggleGroupDevice(browserGroupDevice)
state.toggleGroupDevice(nativeGroupDevice)
assert.deepEqual(state.newGroup.deviceIds, [])
state.createGroupOpen = false

renderer.render(null, root)
assert.deepEqual(Object.keys(listeners), [])
assert.deepEqual(errorsDuringRuntime, [])

// 编译全局确认弹框入口及发送面板，并验证对端确认与“已送达”的边界。
for (const fileName of [
  "src/App.vue",
  "src/features/providers/components/ProviderDeleteConfirmModal.vue",
  "src/features/lanShare/components/LanShareMessagesPanel.vue"
]) {
  const { descriptor, errors } = parse(readFileSync(fileName, "utf8"), { filename: fileName })
  assert.deepEqual(errors, [])
  const script = compileScript(descriptor, { id: fileName, genDefaultAs: "__component" })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename: fileName,
    id: fileName,
    compilerOptions: { bindingMetadata: script.bindings }
  })
  assert.deepEqual(template.errors, [])
  if (!fileName.endsWith("LanShareMessagesPanel.vue")) continue
  for (const style of descriptor.styles) {
    const result = await compileStyleAsync({ source: style.content, filename: fileName, id: fileName, scoped: style.scoped, preprocessLang: style.lang })
    assert.deepEqual(result.errors, [])
  }
  let finishSend
  api.listMessages = async () => []
  api.sendMessage = () => new Promise(resolve => { finishSend = resolve })
  api.onTransferProgress = callback => {
    listeners.transfer = callback
    return () => { delete listeners.transfer }
  }
  api.onMessageCreated = callback => {
    listeners.message = callback
    return () => { delete listeners.message }
  }
  const module = { exports: {} }
  const code = transformSync(`${script.content}\n__component.render = () => null\nexport default __component`, { format: "cjs" }).code
  new Function("require", "module", "exports", code)(name => {
    if (name === "@/api") return { lanShareApi: api, systemApi: {} }
    if (name === "@tauri-apps/api/core") return { isTauri: () => false }
    if (name === "@/utils/message") return { createMessage: { success() {}, error: error => assert.fail(error), info() {} } }
    if (name === "@/utils/formatters") return { formatDateTime: () => "" }
    if (name === "@/features/lanShare/utils") return { formatFileSize: value => String(value) }
    if (name.endsWith(".vue") || name.includes("/style/css") || name === "element-plus") return {}
    return require(name)
  }, module, module.exports)
  const vnode = vue.h(module.exports.default, {
    currentDevice: { id: known.id, native: true },
    currentSessionId: "session",
    service: { running: true }
  })
  vnode.appContext = { ...vue.createApp({})._context }
  vnode.appContext.config.errorHandler = error => errorsDuringRuntime.push(error)
  renderer.render(vnode, root)
  await tick()
  const panel = vnode.component.setupState
  assert.equal(panel.sendBlocked, false, "普通离线或已配对状态不能误判为对方删除")
  panel.addPaths(["D:/example.bin"])
  panel.sendMessage()
  await tick()
  const task = panel.sendingTasks[0]
  assert.equal(task.status, "sending")
  assert.equal(task.progress, 0)
  assert.equal(task.files[0].transferred, false)
  listeners.transfer({ messageId: task.id, fileIndex: 0, fileComplete: false, loadedBytes: 50, totalBytes: 100, bytesPerSecond: 25 })
  assert.equal(task.progress, 50)
  assert.equal(task.bytesPerSecond, 25)
  listeners.transfer({ messageId: "other", loadedBytes: 100, totalBytes: 100 })
  assert.equal(task.progress, 50)
  listeners.transfer({ messageId: task.id, fileIndex: 0, fileComplete: true, loadedBytes: 100, totalBytes: 100, bytesPerSecond: 25 })
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
console.log(
  "设备快传连接确认、IP 过滤、隐藏、删除选项、真实发送进度及 Vue/Less 编译检查通过"
)

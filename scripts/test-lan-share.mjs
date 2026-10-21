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
  paired: false
}
const known = {
  id: "known",
  name: "已连接设备",
  ip: "192.168.1.9",
  port: 17631,
  online: true,
  paired: true
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
    peers: [candidate, known],
    hidden: false,
    scanning: false
  },
  devices: [
    { id: "known", name: "已连接设备", native: true },
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
  "known"
])
assert.equal(calls.length, 0)

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
}
assert.deepEqual(errorsDuringRuntime, [])
console.log(
  "设备快传连接确认、IP 过滤、隐藏、删除选项、真实发送进度及 Vue/Less 编译检查通过"
)

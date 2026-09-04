<template>
  <section class="lan-share-view">
    <LanShareToolbar
      :service="state.service"
      :loading="loading"
      :service-summary="serviceSummary"
      @start="startService"
      @show-access="showAccessDialog"
      @stop="stopService"
    />

    <div class="lan-share-mode-switch">
      <button
        :class="[
          'lan-share-mode-switch__button',
          { 'lan-share-mode-switch__button--active': chatMode === 'direct' }
        ]"
        type="button"
        @click="switchChatMode('direct')"
      >
        单聊模式
      </button>
      <button
        :class="[
          'lan-share-mode-switch__button',
          { 'lan-share-mode-switch__button--active': chatMode === 'group' }
        ]"
        type="button"
        @click="switchChatMode('group')"
      >
        群聊模式
      </button>
    </div>

    <LanShareDevicesPanel
      v-if="navigationMode === 'devices' && chatMode === 'direct'"
      class="lan-share-devices-area"
      :devices="state.devices"
      :sessions="directSessions"
      :selected-device-id="selectedDeviceId"
      :online-devices="state.service.onlineDevices || 0"
      @open-device="openDeviceSessions"
      @create-session="createDeviceSession"
      @delete-history="deleteDeviceHistory"
    />

    <LanShareSessionWorkspace
      v-else
      class="lan-share-detail-area"
      :chat-mode="chatMode"
      :groups="state.groups"
      :sessions="deviceSessions"
      :group-sessions="groupSessions"
      :current-group="currentGroup"
      :current-device="currentDevice"
      :selected-session-id="selectedSessionId"
      :current-session="currentSession"
      :current-session-id="currentSessionId"
      :service-running="state.service.running"
      :state-version="stateVersion"
      @back-devices="backToDevices"
      @switch-mode="switchChatMode"
      @select-session="selectSession"
      @select-group="selectGroup"
      @delete-session="deleteSession"
      @create-session="createNewSession"
      @create-group="createGroup"
      @update-group="updateGroup"
      @remove-group-member="removeGroupMember"
      @clear-group-messages="clearGroupMessages"
      @delete-group="deleteGroup"
      @delete-history="deleteSelectedDeviceHistory"
      @refresh-state="loadState"
      @preview-file="openPreviewDialog"
      @copy-text="copyText"
    />

    <LanShareAccessDialog
      v-if="accessDialogOpen"
      :qr-svg="accessQrSvg"
      :access-url="state.service.accessUrl"
      @close="accessDialogOpen = false"
      @copy-url="copyAccessUrl"
      @stop-service="stopService"
    />

    <LanSharePreviewDialog
      v-if="previewDialog.open"
      :file="previewDialog.file"
      :preview-url="previewDialog.previewUrl"
      :preview-kind="previewDialog.previewKind"
      :text-content="previewDialog.textContent"
      @close="closePreviewDialog"
      @download="downloadPreviewFile"
    />
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue"
import { lanShareApi } from "@/api"
import { createMessage } from "@/utils/message"
import LanShareAccessDialog from "./components/LanShareAccessDialog.vue"
import LanShareDevicesPanel from "./components/LanShareDevicesPanel.vue"
import LanSharePreviewDialog from "./components/LanSharePreviewDialog.vue"
import LanShareSessionWorkspace from "./components/LanShareSessionWorkspace.vue"
import LanShareToolbar from "./components/LanShareToolbar.vue"

const state = reactive({
  service: {
    running: false,
    accessUrl: "",
    qrSvg: "",
    lanIp: "",
    port: 0,
    onlineDevices: 0
  },
  devices: [],
  sessions: [],
  groups: []
})
const loading = ref(false)
const accessDialogOpen = ref(false)
const accessQrSvg = ref("")
const selectedDeviceId = ref("")
const selectedSessionId = ref("")
const selectedGroupId = ref("")
const chatMode = ref("direct")
const navigationMode = ref("devices")
const stateVersion = ref(0)
const previewDialog = reactive({
  open: false,
  file: null,
  previewUrl: "",
  previewKind: "unsupported",
  textContent: ""
})
let stopStateListener = null
let stopDevicesListener = null
let initialSessionResolved = false

const serviceSummary = computed(() => {
  if (!state.service.running) {
    return "服务未启动"
  }

  return `${state.service.lanIp}:${state.service.port} · ${
    state.service.onlineDevices || 0
  } 台在线`
})

const currentSessionId = computed(() => {
  return currentSession.value?.id || ""
})

const currentSession = computed(() => {
  const session =
    state.sessions.find((item) => item.id === selectedSessionId.value) || null

  if (chatMode.value === "direct") {
    return isDirectSession(session) ? session : null
  }
  if (chatMode.value === "group") {
    return session?.mode === "group" ? session : null
  }

  return session
})

const currentDevice = computed(() => {
  return (
    state.devices.find((device) => device.id === selectedDeviceId.value) || null
  )
})

const currentGroup = computed(() => {
  return state.groups.find((group) => group.id === selectedGroupId.value) || null
})

const deviceSessions = computed(() => {
  return directSessions.value.filter((session) => {
    return selectedDeviceId.value && isDirectDeviceSession(session)
  })
})

const directSessions = computed(() => {
  return state.sessions.filter((session) => isDirectSession(session))
})

const groupSessions = computed(() => {
  return state.sessions.filter((session) => session.mode === "group")
})

onMounted(() => {
  loadState()
  stopStateListener = lanShareApi.onStateChanged(applyState)
  stopDevicesListener = lanShareApi.onDevicesChanged((devices) => {
    state.devices = devices || []
  })
})

watch(selectedDeviceId, (deviceId) => {
  if (chatMode.value !== "direct") {
    return
  }

  if (!deviceId) {
    selectedSessionId.value = ""
    return
  }

  if (
    selectedSessionId.value &&
    (!isDirectDeviceSession(currentSession.value) ||
      currentSession.value?.deviceId !== deviceId)
  ) {
    selectedSessionId.value = ""
  }

  if (!selectedSessionId.value) {
    selectedSessionId.value = findDirectSessionId(deviceId)
  }
})

watch(selectedSessionId, (sessionId) => {
  const session = state.sessions.find((item) => item.id === sessionId)

  if (
    session &&
    chatMode.value === "direct" &&
    isDirectSession(session) &&
    selectedDeviceId.value !== session.deviceId
  ) {
    selectedDeviceId.value = session.deviceId
  }
})

onBeforeUnmount(() => {
  if (stopStateListener) stopStateListener()
  if (stopDevicesListener) stopDevicesListener()
})

function applyState(payload) {
  const nextState = payload?.service ? payload : unwrapData(payload) || {}

  state.service = nextState.service || state.service
  accessQrSvg.value = state.service.qrSvg || accessQrSvg.value
  state.devices = nextState.devices || []
  state.sessions = nextState.sessions || []
  state.groups = nextState.groups || []
  stateVersion.value += 1

  if (nextState.currentSession?.id) {
    if (nextState.currentSession.mode === "group") {
      selectedGroupId.value = nextState.currentSession.groupId || ""
      if (chatMode.value === "group") {
        selectedSessionId.value = nextState.currentSession.id
        selectedDeviceId.value = ""
        navigationMode.value = "detail"
      }
    } else if (chatMode.value === "direct") {
      selectedSessionId.value = nextState.currentSession.id
      selectedDeviceId.value = nextState.currentSession.deviceId || ""
      selectedGroupId.value = ""
    }
    initialSessionResolved = true
  } else if (!initialSessionResolved) {
    initialSessionResolved = true
  }
}

function unwrapData(result) {
  return result?.status && "data" in result ? result.data : result
}

function isDirectSession(session) {
  return Boolean(session) && session.mode !== "group"
}

function isDirectDeviceSession(session, deviceId = selectedDeviceId.value) {
  return (
    isDirectSession(session) &&
    Boolean(deviceId) &&
    session.deviceId === deviceId
  )
}

function findDirectSessionId(deviceId) {
  return (
    state.sessions.find((session) => {
      return isDirectDeviceSession(session, deviceId)
    })?.id || ""
  )
}

async function runAction(action, successMessage) {
  loading.value = true

  try {
    const result = unwrapData(await action())

    if (result?.service) {
      applyState(result)
    }
    if (successMessage) {
      createMessage.success(successMessage)
    }

    return result
  } catch (error) {
    createMessage.error(error?.message || String(error))
    return null
  } finally {
    loading.value = false
  }
}

async function loadState() {
  await runAction(async () => lanShareApi.getState())
}

async function startService() {
  const result = await runAction(
    async () => lanShareApi.startService({}),
    "设备快传服务已启动。"
  )

  if (result) {
    state.service = {
      ...state.service,
      ...result
    }
    showAccessDialog()
    await loadState()
  }
}

async function showAccessDialog() {
  if (!state.service.running) {
    return
  }

  if (!accessQrSvg.value) {
    await loadState()
  }

  accessQrSvg.value = state.service.qrSvg || accessQrSvg.value
  accessDialogOpen.value = true
}

async function stopService() {
  const result = await runAction(
    async () => lanShareApi.stopService(),
    "设备快传服务已关闭。"
  )

  if (result !== null) {
    accessDialogOpen.value = false
    accessQrSvg.value = ""
    closePreviewDialog()
    await loadState()
  }
}

async function createNewSession() {
  if (!selectedDeviceId.value) {
    return
  }

  await runAction(
    async () => lanShareApi.createSession({ deviceId: selectedDeviceId.value }),
    "新会话已创建。"
  )
  navigationMode.value = "detail"
}

async function createGroup(payload) {
  const result = await runAction(
    async () => lanShareApi.createGroup(payload),
    "群聊已创建。"
  )

  if (result?.currentSession) {
    selectedSessionId.value = result.currentSession.id
    selectedGroupId.value = result.currentSession.groupId || ""
  }
  chatMode.value = "group"
  navigationMode.value = "detail"
}

async function updateGroup(payload) {
  await runAction(
    async () => lanShareApi.updateGroup(payload),
    "群聊设置已更新。"
  )
  await loadState()
}

async function removeGroupMember(payload) {
  await runAction(
    async () => lanShareApi.removeGroupMember(payload),
    "群成员已移出。"
  )
  await loadState()
}

async function clearGroupMessages(groupId) {
  if (!groupId || !window.confirm("确认清空该群聊的全部消息吗？")) {
    return
  }

  await runAction(
    async () => lanShareApi.clearGroupMessages({ groupId }),
    "群消息已清空。"
  )
  await loadState()
}

async function deleteGroup(groupId) {
  if (!groupId || !window.confirm("确认解散该群聊吗？")) {
    return
  }

  const result = await runAction(
    async () => lanShareApi.deleteGroup({ groupId }),
    "群聊已解散。"
  )

  if (result) {
    selectedGroupId.value = ""
    selectedSessionId.value = ""
  }
  await loadState()
}

async function deleteSelectedDeviceHistory() {
  if (!selectedDeviceId.value) {
    return
  }

  await deleteDeviceHistory(selectedDeviceId.value)
}

async function copyAccessUrl() {
  await copyText(state.service.accessUrl)
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text || "")
    createMessage.success("已复制。")
  } catch (error) {
    createMessage.error(error?.message || "复制失败。")
  }
}

async function selectSession(sessionId) {
  selectedSessionId.value = sessionId

  if (!selectedSessionId.value) {
    return
  }

  await runAction(async () =>
    lanShareApi.activateSession({ sessionId: selectedSessionId.value })
  )
}

async function deleteSession(sessionId) {
  if (!sessionId) {
    return
  }

  const deletedSession = state.sessions.find(
    (session) => session.id === sessionId
  )
  const result = await runAction(
    async () => lanShareApi.deleteSession({ sessionId }),
    "会话已删除。"
  )

  if (!result) {
    return
  }

  const nextSessions = Array.isArray(result.sessions)
    ? result.sessions
    : state.sessions
  const deviceId = deletedSession?.deviceId || selectedDeviceId.value

  if (selectedSessionId.value === sessionId) {
    const nextSessionId =
      nextSessions.find((session) => {
        return session.mode !== "group" && session.deviceId === deviceId
      })?.id || ""

    selectedSessionId.value = nextSessionId
    if (nextSessionId) {
      await runAction(async () =>
        lanShareApi.activateSession({ sessionId: nextSessionId })
      )
    }
  }

  await loadState()
}

function openDeviceSessions(deviceId) {
  selectedDeviceId.value = deviceId
  navigationMode.value = "detail"

  if (isDirectDeviceSession(currentSession.value, deviceId)) {
    return
  }

  selectedSessionId.value = findDirectSessionId(deviceId)
}

function backToDevices() {
  navigationMode.value = chatMode.value === "group" ? "detail" : "devices"
}

function switchChatMode(mode) {
  chatMode.value = mode
  navigationMode.value = mode === "group" ? "detail" : "devices"

  if (mode === "group") {
    selectedDeviceId.value = ""
    selectedGroupId.value = selectedGroupId.value || state.groups[0]?.id || ""
    selectGroup(selectedGroupId.value)
    return
  }

  selectedGroupId.value = ""
  selectedSessionId.value = findDirectSessionId(selectedDeviceId.value)
  if (selectedSessionId.value) {
    selectSession(selectedSessionId.value)
  }
}

async function selectGroup(groupId) {
  selectedGroupId.value = groupId
  chatMode.value = "group"
  navigationMode.value = "detail"

  const groupSession =
    groupSessions.value.find((session) => {
      return session.groupId === groupId && !session.deviceId
    }) ||
    groupSessions.value.find((session) => session.groupId === groupId)

  selectedSessionId.value = groupSession?.id || ""

  if (selectedSessionId.value) {
    await runAction(async () =>
      lanShareApi.activateSession({ sessionId: selectedSessionId.value })
    )
  }
}

async function createDeviceSession(deviceId) {
  selectedDeviceId.value = deviceId
  navigationMode.value = "detail"
  await createNewSession()
}

async function deleteDeviceHistory(deviceId) {
  if (!deviceId) {
    return
  }

  await runAction(
    async () => lanShareApi.deleteDeviceHistory({ deviceId }),
    "设备历史已删除。"
  )

  if (selectedDeviceId.value === deviceId) {
    selectedDeviceId.value = ""
    selectedSessionId.value = ""
    navigationMode.value = "devices"
  }

  await loadState()
}

function openPreviewDialog(file) {
  if (!state.service.running) {
    createMessage.warning("请先启动服务后再预览共享文件。")
    return
  }

  previewDialog.file = file
  previewDialog.previewKind = previewKind(file)
  previewDialog.previewUrl = fileServiceUrl(file, "preview")
  previewDialog.textContent = ""
  previewDialog.open = true
}

function closePreviewDialog() {
  previewDialog.open = false
  previewDialog.file = null
  previewDialog.previewUrl = ""
  previewDialog.previewKind = "unsupported"
  previewDialog.textContent = ""
}

function downloadPreviewFile(file) {
  const url = fileServiceUrl(file, "download")

  if (url) {
    const link = document.createElement("a")

    link.href = url
    link.download = file?.name || "download"
    document.body.appendChild(link)
    link.click()
    link.remove()
  }
}

function fileServiceUrl(file, action) {
  const service = accessServiceInfo()

  if (!service || !file?.id) {
    return ""
  }

  const params = new URLSearchParams({
    id: file.id,
    token: service.token,
    deviceId: "desktop",
    sessionId: currentSessionId.value
  })

  return `${service.origin}/api/files/${action}?${params.toString()}`
}

function accessServiceInfo() {
  try {
    const url = new URL(state.service.accessUrl)

    return {
      origin: url.origin,
      token: url.searchParams.get("token") || ""
    }
  } catch (error) {
    return null
  }
}

function previewKind(file) {
  const name = String(file?.name || "").toLowerCase()
  const mimeType = String(file?.mimeType || "").toLowerCase()

  if (mimeType.startsWith("image/")) {
    return "image"
  }
  if (mimeType.startsWith("video/")) {
    return "video"
  }
  if (mimeType.startsWith("audio/")) {
    return "audio"
  }
  if (mimeType === "application/pdf" || name.endsWith(".pdf")) {
    return "pdf"
  }
  if (mimeType.startsWith("text/") || isTextPreviewFile(name, mimeType)) {
    return "text"
  }

  return "unsupported"
}

function isTextPreviewFile(name, mimeType) {
  const textMimeTypes = [
    "application/json",
    "application/xml",
    "application/javascript",
    "application/x-javascript",
    "application/xhtml+xml",
    "image/svg+xml"
  ]
  const textExtensions = [
    ".txt",
    ".md",
    ".json",
    ".xml",
    ".csv",
    ".log",
    ".js",
    ".ts",
    ".css",
    ".html",
    ".vue",
    ".rs",
    ".py",
    ".java",
    ".c",
    ".cpp",
    ".h",
    ".go",
    ".yaml",
    ".yml",
    ".toml",
    ".ini",
    ".conf",
    ".sql",
    ".sh",
    ".ps1"
  ]

  return (
    textMimeTypes.includes(mimeType) ||
    textExtensions.some((extension) => name.endsWith(extension))
  )
}
</script>

<style scoped lang="less">
.lan-share-view {
  position: relative;
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;

  .lan-share-devices-area,
  .lan-share-detail-area {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }

  .lan-share-mode-switch {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 6px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);

    &__button {
      display: inline-flex;
      height: 30px;
      align-items: center;
      justify-content: center;
      padding: 0 12px;
      border: 1px solid transparent;
      border-radius: 7px;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      font-weight: 800;
    }

    &__button--active {
      border-color: var(--color-info-line);
      background: var(--color-panel);
      color: var(--color-primary);
      box-shadow: 0 6px 18px rgba(42, 67, 101, 0.08);
    }
  }
}
</style>

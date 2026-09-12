<template>
  <section class="lan-share-view">
    <header class="drop-header">
      <div class="drop-brand">
        <span class="drop-title">设备快传</span
        ><span class="drop-local-name">{{
          state.native.deviceName || "本机"
        }}</span>
      </div>
      <div class="drop-header-actions">
        <span
          class="drop-status"
          :class="{ 'drop-status-online': state.service.running }"
          ><span class="drop-status-dot"></span
          >{{
            state.service.running ? "已上线 · 自动发现设备" : "已离线"
          }}</span
        >
        <button
          class="drop-header-button"
          type="button"
          :disabled="loading"
          @click="
            state.service.running ? (connectDialogOpen = true) : startService()
          "
        >
          <Link :size="14" />{{ state.service.running ? "连接设备" : "上线" }}
        </button>
        <button
          class="drop-header-button"
          type="button"
          :disabled="!state.service.running"
          @click="showAccessDialog"
        >
          <QrCode :size="14" />访客访问
        </button>
        <el-dropdown trigger="click"
          ><button
            class="drop-menu-button"
            type="button"
            aria-label="快传网络设置"
          >
            <Settings2 :size="16" /></button
          ><template #dropdown
            ><el-dropdown-menu
              ><el-dropdown-item disabled>{{ serviceSummary }}</el-dropdown-item
              ><el-dropdown-item
                v-for="ip in state.service.lanIps || []"
                :key="ip"
                :disabled="ip === state.service.lanIp || loading"
                @click="setAccessIp(ip)"
                >二维码使用 {{ ip }}</el-dropdown-item
              ><el-dropdown-item
                v-if="state.service.running"
                @click="stopService"
                >暂停接收并离线</el-dropdown-item
              ><el-dropdown-item v-else @click="startService"
                >重新上线</el-dropdown-item
              ></el-dropdown-menu
            ></template
          ></el-dropdown
        >
      </div>
    </header>

    <div class="drop-body">
      <aside class="drop-sidebar">
        <div class="drop-sidebar-tools">
          <div class="drop-tabs">
            <button
              class="drop-tab"
              :class="{ 'drop-tab-active': chatMode === 'direct' }"
              type="button"
              @click="switchChatMode('direct')"
            >
              设备</button
            ><button
              class="drop-tab"
              :class="{ 'drop-tab-active': chatMode === 'group' }"
              type="button"
              @click="switchChatMode('group')"
            >
              群聊
            </button>
          </div>
          <button
            v-if="chatMode === 'group'"
            class="drop-create"
            type="button"
            title="创建群聊"
            aria-label="创建群聊"
            @click="openCreateGroup"
          >
            <Plus :size="16" />
          </button>
        </div>
        <div class="drop-search">
          <Search :size="14" /><input
            v-model="deviceKeyword"
            class="drop-search-input"
            :placeholder="chatMode === 'group' ? '搜索群聊' : '搜索设备或 IP'"
            aria-label="搜索设备或群聊"
          />
        </div>
        <nav class="drop-conversations" aria-label="快传会话">
          <template v-if="chatMode === 'direct'">
            <div
              v-for="device in visibleDevices"
              :key="device.id"
              class="drop-device"
              :class="{ 'drop-device-active': selectedDeviceId === device.id }"
            >
              <button
                class="drop-device-select"
                type="button"
                :disabled="deleteDevicePending"
                @click="chooseDevice(device)"
              >
                <span class="drop-device-icon"
                  ><MonitorSmartphone v-if="device.native" :size="20" /><Globe
                    v-else
                    :size="20" /><span
                    v-if="device.online"
                    class="drop-device-online"
                  ></span
                ></span>
                <span class="drop-device-copy"
                  ><span class="drop-device-name">{{
                    device.name || device.autoName || "未命名设备"
                  }}</span
                  ><span class="drop-device-preview">{{
                    device.pairingCode
                      ? `等待确认 · ${device.pairingCode}`
                      : device.requiresPairing
                        ? "发现客户端 · 点击连接"
                        : deviceLastMessage(device.id)
                  }}</span
                  ><span class="drop-device-address"
                    >{{ device.ip || "历史设备" }} ·
                    {{ device.native ? "客户端" : "网页访客" }}</span
                  ></span
                >
              </button>
              <button
                class="drop-device-delete"
                type="button"
                title="删除设备"
                :aria-label="`删除设备 ${device.name || device.autoName || '未命名设备'}`"
                :disabled="deleteDevicePending"
                @click="requestDeleteDevice(device)"
              >
                <Trash2 :size="15" />
              </button>
            </div>
            <div v-if="!visibleDevices.length" class="drop-sidebar-empty">
              <MonitorSmartphone :size="26" :stroke-width="1.4" /><span
                >等待附近设备</span
              ><small class="drop-sidebar-empty-hint"
                >两台电脑打开设备快传即可发现彼此，无需打开网页。</small
              >
            </div>
          </template>
          <template v-else>
            <button
              v-for="group in visibleGroups"
              :key="group.id"
              class="drop-device"
              :class="{ 'drop-device-active': selectedGroupId === group.id }"
              type="button"
              @click="selectGroup(group.id)"
            >
              <span class="drop-device-icon"><Users :size="20" /></span
              ><span class="drop-device-copy"
                ><span class="drop-device-name">{{ group.name }}</span
                ><span class="drop-device-preview"
                  >{{ group.members?.length || 0 }} 位成员</span
                ></span
              >
            </button>
            <div v-if="!visibleGroups.length" class="drop-sidebar-empty">
              <Users :size="26" :stroke-width="1.4" /><span>还没有群聊</span
              ><small class="drop-sidebar-empty-hint"
                >点击右上角加号创建群聊。</small
              >
            </div>
          </template>
        </nav>
        <div class="drop-sidebar-note">
          {{
            state.native.error ||
            "已安装软件：直接连接设备。未安装软件：使用访客链接或扫码。"
          }}
        </div>
      </aside>

      <LanShareSessionWorkspace
        :chat-mode="chatMode"
        :sessions="deviceSessions"
        :current-group="currentGroup"
        :current-device="currentDevice"
        :selected-session-id="selectedSessionId"
        :current-session="currentSession"
        :current-session-id="currentSessionId"
        :service="state.service"
        :state-version="stateVersion"
        @select-session="selectSession"
        @delete-session="deleteSession"
        @create-session="createNewSession"
        @update-group="updateGroup"
        @remove-group-member="removeGroupMember"
        @clear-group-messages="clearGroupMessages"
        @delete-group="deleteGroup"
        @delete-history="deleteSelectedDeviceHistory"
        @delete-device="requestDeleteDevice(currentDevice)"
        @refresh-state="loadState"
        @preview-file="openPreviewDialog"
        @copy-text="copyText"
      />
    </div>

    <DeleteConfirmModal
      v-if="deleteDeviceTarget"
      title="删除设备"
      :name="deleteDeviceTarget.name"
      :description="
        deleteDeviceTarget.native
          ? '删除此设备及其本机单聊记录，并解除客户端配对。移除后不会自动发现回来，可通过地址重新连接。不会删除磁盘上的原始文件。'
          : '删除此设备及其本机单聊记录，并移除群聊成员关联。群聊历史和其他设备不受影响，不会删除磁盘上的原始文件；对方重新访问后可再次加入。'
      "
      :pending="deleteDevicePending"
      :error="deleteDeviceError"
      @close="closeDeleteDevice"
      @confirm="confirmDeleteDevice"
    />

    <LanShareAccessDialog
      v-if="accessDialogOpen"
      :qr-svg="accessQrSvg"
      :access-url="state.service.accessUrl"
      @close="accessDialogOpen = false"
      @copy-url="copyAccessUrl"
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

    <BaseModal
      v-if="connectDialogOpen"
      class="drop-connect-modal"
      title="连接客户端"
      @close="connectDialogOpen = false"
    >
      <div class="drop-connect-content">
        <p class="drop-connect-description">
          优先点击左侧自动发现的客户端。未发现时，也可以输入对方的局域网地址或设备快传链接，直接在软件内连接。
        </p>
        <el-input
          v-model="connectAddress"
          placeholder="例如：192.168.1.8:17631"
          @keyup.enter="connectByAddress"
        /><span class="drop-connect-hint"
          >首次连接需要对方在软件内确认，不需要打开网页。</span
        ><el-button
          type="primary"
          :loading="loading"
          :disabled="!connectAddress.trim()"
          @click="connectByAddress"
          >请求连接</el-button
        >
      </div>
    </BaseModal>
    <BaseModal
      v-if="pendingPair"
      class="drop-connect-modal"
      title="确认设备连接"
      @close="respondPairing(false)"
    >
      <div class="drop-connect-content">
        <span
          >{{ pendingPair.name }}（{{ pendingPair.ip }}）希望连接此设备。</span
        ><span class="drop-pair-code">{{ pendingPair.code }}</span>
        <p class="drop-connect-description">
          请核对两台电脑显示的确认码。允许后，该设备可以直接发送消息与文件；仅在可信局域网中使用。
        </p>
        <div class="drop-pair-actions">
          <el-button :disabled="loading" @click="respondPairing(false)"
            >拒绝</el-button
          ><el-button
            type="primary"
            :loading="loading"
            @click="respondPairing(true)"
            >允许连接</el-button
          >
        </div>
      </div>
    </BaseModal>
    <BaseModal
      v-if="createGroupOpen"
      class="drop-group-modal"
      title="创建群聊"
      @close="createGroupOpen = false"
    >
      <form class="drop-group-form" @submit.prevent="createGroup">
        <label class="drop-group-field"
          ><span>群名称</span
          ><el-input
            v-model="newGroup.name"
            placeholder="输入群名称"
            :disabled="loading"
        /></label>
        <div class="drop-group-selection">
          <span>邀请设备 · 已选 {{ selectedGroupDeviceIds.length }} 台</span
          ><span class="drop-group-hint"
            >至少选择一台设备，才会创建群聊。离线的网页设备重新连接后可查看邀请。</span
          >
        </div>
        <div class="drop-group-devices">
          <el-checkbox
            v-for="device in state.devices"
            :key="device.id"
            v-model="newGroup.deviceIds"
            :value="device.id"
            :disabled="loading || device.native"
            class="drop-group-device"
          >
            <span class="drop-group-device-copy"
              ><span>{{ device.name || device.autoName }}</span
              ><span class="drop-group-device-meta"
                >{{ device.ip }} ·
                {{
                  device.native
                    ? "客户端暂仅支持单聊"
                    : device.online
                      ? "在线"
                      : "离线"
                }}</span
              ></span
            >
          </el-checkbox>
          <span v-if="!state.devices.length" class="drop-group-empty"
            >暂无可邀请设备，请先让至少一台网页设备通过访客访问连接。</span
          >
        </div>
        <div class="drop-group-actions">
          <el-button :disabled="loading" @click="createGroupOpen = false"
            >取消</el-button
          ><el-button
            native-type="submit"
            type="primary"
            :loading="loading"
            :disabled="!selectedGroupDeviceIds.length || !newGroup.name.trim()"
            >创建并邀请</el-button
          >
        </div>
      </form>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue"
import {
  ElButton,
  ElCheckbox,
  ElDropdown,
  ElDropdownItem,
  ElDropdownMenu,
  ElInput
} from "element-plus"
import "element-plus/es/components/button/style/css"
import "element-plus/es/components/checkbox/style/css"
import "element-plus/es/components/dropdown/style/css"
import "element-plus/es/components/input/style/css"
import {
  Globe,
  Link,
  MonitorSmartphone,
  Plus,
  QrCode,
  Search,
  Settings2,
  Trash2,
  Users
} from "lucide-vue-next"
import { lanShareApi } from "@/api"
import { createMessage } from "@/utils/message"
import BaseModal from "@/components/BaseModal.vue"
import DeleteConfirmModal from "@/features/providers/components/ProviderDeleteConfirmModal.vue"
import { fileUrl } from "./utils"
import LanShareAccessDialog from "./components/LanShareAccessDialog.vue"
import LanSharePreviewDialog from "./components/LanSharePreviewDialog.vue"
import LanShareSessionWorkspace from "./components/LanShareSessionWorkspace.vue"

const state = reactive({
  service: {
    running: false,
    accessUrl: "",
    qrSvg: "",
    lanIp: "",
    lanIps: [],
    port: 0,
    onlineDevices: 0
  },
  devices: [],
  sessions: [],
  groups: [],
  native: { deviceName: "", peers: [], pairingRequests: [], error: "" },
  messages: []
})
const loading = ref(false)
const deleteDeviceTarget = ref(null)
const deleteDevicePending = ref(false)
const deleteDeviceError = ref("")
const createGroupOpen = ref(false)
const newGroup = reactive({ name: "新的群聊", deviceIds: [] })
const selectedGroupDeviceIds = computed(() =>
  state.devices
    .filter(
      (device) => !device.native && newGroup.deviceIds.includes(device.id)
    )
    .map((device) => device.id)
)
const deviceKeyword = ref("")
const connectDialogOpen = ref(false)
const connectAddress = ref("")
const accessDialogOpen = ref(false)
const accessQrSvg = ref("")
const selectedDeviceId = ref("")
const selectedSessionId = ref("")
const selectedGroupId = ref("")
const chatMode = ref("direct")
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
  return (
    state.groups.find((group) => group.id === selectedGroupId.value) || null
  )
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

const pendingPair = computed(() => state.native.pairingRequests?.[0] || null)
const visibleGroups = computed(() =>
  state.groups.filter((group) =>
    group.name.toLowerCase().includes(deviceKeyword.value.trim().toLowerCase())
  )
)
const visibleDevices = computed(() => {
  const devices = new Map(
    state.devices.map((device) => [
      device.id,
      { ...device, requiresPairing: false }
    ])
  )
  for (const peer of state.native.peers || []) {
    devices.set(peer.id, {
      ...devices.get(peer.id),
      ...peer,
      native: true,
      requiresPairing: !peer.paired
    })
  }
  const query = deviceKeyword.value.trim().toLowerCase()
  return [...devices.values()]
    .filter((device) =>
      `${device.name || device.autoName || ""} ${device.ip || ""}`
        .toLowerCase()
        .includes(query)
    )
    .sort((left, right) => Number(right.online) - Number(left.online))
})

function deviceLastMessage(deviceId) {
  const sessionIds = new Set(
    state.sessions
      .filter(
        (session) => session.deviceId === deviceId && session.mode !== "group"
      )
      .map((session) => session.id)
  )
  const message = state.messages.find((message) =>
    sessionIds.has(message.sessionId)
  )
  if (!message) return "发送文件或消息"
  return (
    message.content ||
    (message.attachments?.length
      ? `[${message.attachments.length} 个附件] ${message.attachments[0].name}`
      : "文件消息")
  )
}

async function chooseDevice(device) {
  if (device.requiresPairing) {
    if (device.pairingCode || loading.value) return
    await runAction(() => lanShareApi.connectPeer({ peerId: device.id }))
    return
  }
  await openDeviceSessions(device.id)
}

async function connectByAddress() {
  if (!connectAddress.value.trim() || loading.value) return
  const result = await runAction(() =>
    lanShareApi.connectPeer({ address: connectAddress.value.trim() })
  )
  if (result) {
    connectDialogOpen.value = false
    connectAddress.value = ""
  }
}

async function respondPairing(accept) {
  if (!pendingPair.value || loading.value) return
  const requestId = pendingPair.value.id
  await runAction(() => lanShareApi.respondPairing({ requestId, accept }))
}

onMounted(async () => {
  stopStateListener = lanShareApi.onStateChanged(applyState)
  stopDevicesListener = lanShareApi.onDevicesChanged((devices) => {
    state.devices = devices || []
  })
  await loadState()
  if (!state.service.running) await startService()
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
  state.messages = nextState.messages || []
  state.native = nextState.native || state.native
  stateVersion.value += 1

  if (nextState.currentSession?.id) {
    if (nextState.currentSession.mode === "group") {
      selectedGroupId.value = nextState.currentSession.groupId || ""
      if (chatMode.value === "group") {
        selectedSessionId.value = nextState.currentSession.id
        selectedDeviceId.value = ""
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
  if (
    chatMode.value === "direct" &&
    !selectedDeviceId.value &&
    state.devices.length
  ) {
    selectedDeviceId.value = state.devices[0].id
    selectedSessionId.value = findDirectSessionId(selectedDeviceId.value)
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
  const result = await runAction(async () => lanShareApi.startService({}))

  if (result) {
    state.service = {
      ...state.service,
      ...result
    }
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

async function setAccessIp(lanIp) {
  if (!lanIp || lanIp === state.service.lanIp || loading.value) {
    return
  }

  // 只切换二维码和访问链接中的 IP，不重启快传服务。
  await runAction(
    () => lanShareApi.setAccessIp({ lanIp }),
    `二维码已切换到 ${lanIp}`
  )
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
}

function openCreateGroup() {
  newGroup.name = "新的群聊"
  newGroup.deviceIds = []
  createGroupOpen.value = true
}

async function createGroup() {
  if (
    loading.value ||
    !selectedGroupDeviceIds.value.length ||
    !newGroup.name.trim()
  )
    return
  const payload = {
    name: newGroup.name.trim(),
    messageVisibility: "all",
    deviceIds: [...selectedGroupDeviceIds.value]
  }
  const result = await runAction(
    async () => lanShareApi.createGroup(payload),
    "群聊已创建。"
  )

  if (result?.currentSession) {
    selectedSessionId.value = result.currentSession.id
    selectedGroupId.value = result.currentSession.groupId || ""
    createGroupOpen.value = false
    chatMode.value = "group"
  }
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

async function openDeviceSessions(deviceId) {
  selectedDeviceId.value = deviceId

  if (isDirectDeviceSession(currentSession.value, deviceId)) {
    return
  }

  selectedSessionId.value = findDirectSessionId(deviceId)
  if (!selectedSessionId.value) await createNewSession()
  else await selectSession(selectedSessionId.value)
}

function switchChatMode(mode) {
  chatMode.value = mode

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

  const groupSession =
    groupSessions.value.find((session) => {
      return session.groupId === groupId && !session.deviceId
    }) || groupSessions.value.find((session) => session.groupId === groupId)

  selectedSessionId.value = groupSession?.id || ""

  if (selectedSessionId.value) {
    await runAction(async () =>
      lanShareApi.activateSession({ sessionId: selectedSessionId.value })
    )
  }
}

async function deleteDeviceHistory(deviceId) {
  if (!deviceId || !window.confirm("删除这个设备的本机会话历史？")) {
    return
  }

  await runAction(
    async () => lanShareApi.deleteDeviceHistory({ deviceId }),
    "设备历史已删除。"
  )

  if (selectedDeviceId.value === deviceId) {
    selectedDeviceId.value = ""
    selectedSessionId.value = ""
  }

  await loadState()
}

function requestDeleteDevice(device) {
  if (!device?.id || deleteDevicePending.value || loading.value) return
  deleteDeviceTarget.value = {
    id: device.id,
    name: device.name || device.autoName || "未命名设备",
    native: Boolean(device.native)
  }
  deleteDeviceError.value = ""
}

function closeDeleteDevice() {
  if (deleteDevicePending.value) return
  deleteDeviceTarget.value = null
  deleteDeviceError.value = ""
}

async function confirmDeleteDevice() {
  if (!deleteDeviceTarget.value || deleteDevicePending.value) return
  const deviceId = deleteDeviceTarget.value.id
  deleteDevicePending.value = true
  deleteDeviceError.value = ""
  try {
    const result = unwrapData(await lanShareApi.deleteDevice({ deviceId }))
    if (selectedDeviceId.value === deviceId) {
      selectedDeviceId.value = ""
      selectedSessionId.value = ""
    }
    applyState(result)
    deleteDeviceTarget.value = null
    createMessage.success("设备已删除。")
  } catch (error) {
    deleteDeviceError.value = error?.message || String(error)
  } finally {
    deleteDevicePending.value = false
  }
}

function openPreviewDialog(file) {
  if (!state.service.running) {
    createMessage.warning("请先启动服务后再预览共享文件。")
    return
  }

  previewDialog.file = {
    ...file,
    sessionId: file.sessionId || currentSessionId.value
  }
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
  return fileUrl(
    state.service,
    file,
    file.sessionId || currentSessionId.value,
    action
  )
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
  display: flex;
  height: 100%;
  min-height: 0;
  min-width: 0;
  flex-direction: column;
  overflow: hidden;
  .drop-header {
    display: flex;
    min-width: 0;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 0 10px;
    .drop-brand {
      display: flex;
      min-width: 0;
      align-items: baseline;
      gap: 10px;
      .drop-title {
        flex: none;
        color: var(--color-text);
        font-size: var(--font-size-lg);
      }
      .drop-local-name {
        overflow: hidden;
        color: var(--color-text-muted);
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--font-size-sm);
      }
    }
    .drop-header-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 8px;
      .drop-status {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        .drop-status-dot {
          width: 5px;
          height: 5px;
          border-radius: 50%;
          background: currentColor;
        }
        &.drop-status-online {
          color: var(--color-success);
        }
      }
      .drop-header-button {
        display: inline-flex;
        height: 29px;
        align-items: center;
        gap: 5px;
        padding: 0 9px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: var(--font-size-base);
      }
      .drop-menu-button {
        display: grid;
        width: 28px;
        height: 29px;
        place-items: center;
        padding: 0;
        border: 0;
        background: transparent;
        color: var(--color-text-muted);
      }
    }
  }
  .drop-body {
    display: flex;
    min-height: 0;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    .drop-sidebar {
      display: flex;
      width: clamp(170px, 23%, 230px);
      min-width: 0;
      flex: none;
      flex-direction: column;
      border-right: 1px solid var(--color-line);
      background: var(--color-panel);
      .drop-sidebar-tools {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        padding: 12px 12px 8px;
        .drop-tabs {
          display: flex;
          gap: 14px;
          .drop-tab {
            padding: 4px 1px 8px;
            border: 0;
            border-bottom: 2px solid transparent;
            background: transparent;
            color: var(--color-text-muted);
            font-size: var(--font-size-base);
            &.drop-tab-active {
              border-bottom-color: var(--color-primary);
              color: var(--color-primary);
            }
          }
        }
        .drop-create {
          display: grid;
          width: 26px;
          height: 26px;
          place-items: center;
          padding: 0;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: transparent;
          color: var(--color-primary);
        }
      }
      .drop-search {
        display: flex;
        flex: none;
        align-items: center;
        gap: 6px;
        margin: 0 10px 10px;
        padding: 7px 8px;
        border-radius: 6px;
        background: var(--color-panel-soft);
        color: var(--color-text-soft);
        .drop-search-input {
          width: 100%;
          min-width: 0;
          padding: 0;
          border: 0;
          outline: none;
          background: transparent;
          color: var(--color-text);
          font-size: var(--font-size-base);
        }
      }
      .drop-conversations {
        min-height: 0;
        flex: 1;
        padding: 0 7px;
        overflow-y: auto;
        scrollbar-width: thin;
        .drop-device {
          display: flex;
          width: 100%;
          align-items: center;
          gap: 9px;
          margin-bottom: 4px;
          padding: 11px 8px;
          border: 1px solid transparent;
          border-radius: 8px;
          background: transparent;
          color: var(--color-text);
          text-align: left;
          &:hover {
            background: var(--color-panel-soft);
          }
          &.drop-device-active {
            border-color: var(--color-info-line);
            background: var(--color-primary-soft);
          }
          .drop-device-select {
            display: flex;
            min-width: 0;
            flex: 1;
            align-items: center;
            gap: 9px;
            padding: 0;
            border: 0;
            color: inherit;
            background: transparent;
            text-align: left;
          }
          .drop-device-delete {
            display: flex;
            width: 28px;
            height: 28px;
            flex: none;
            align-items: center;
            justify-content: center;
            padding: 0;
            border: 0;
            border-radius: 6px;
            color: var(--color-text-muted);
            background: transparent;
            &:hover:not(:disabled) {
              color: var(--color-danger);
              background: var(--color-danger-soft);
            }
          }
          .drop-device-icon {
            position: relative;
            display: grid;
            width: 32px;
            height: 36px;
            flex: none;
            place-items: center;
            color: var(--color-primary);
            .drop-device-online {
              position: absolute;
              right: 0;
              bottom: 2px;
              width: 7px;
              height: 7px;
              border: 2px solid var(--color-panel);
              border-radius: 50%;
              background: var(--color-success);
            }
          }
          .drop-device-copy {
            display: flex;
            min-width: 0;
            flex: 1;
            flex-direction: column;
            gap: 5px;
            .drop-device-name {
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
              font-size: var(--font-size-base);
            }
            .drop-device-preview {
              overflow: hidden;
              color: var(--color-text-muted);
              text-overflow: ellipsis;
              white-space: nowrap;
              font-size: var(--font-size-sm);
            }
            .drop-device-address {
              color: var(--color-text-soft);
              font-size: var(--font-size-sm);
            }
          }
        }
        .drop-sidebar-empty {
          display: flex;
          align-items: center;
          flex-direction: column;
          gap: 12px;
          padding: 36px 12px;
          color: var(--color-text-muted);
          font-size: var(--font-size-base);
          text-align: center;
          line-height: 1.7;

          .drop-sidebar-empty-hint {
            font-size: var(--font-size-sm);
          }
        }
      }
      .drop-sidebar-note {
        flex: none;
        padding: 12px;
        border-top: 1px solid var(--color-line);
        color: var(--color-text-soft);
        font-size: var(--font-size-sm);
        line-height: 1.7;
      }
    }
  }
}
.drop-connect-modal.base-modal {
  :deep(.base-modal__panel) {
    width: min(460px, calc(100vw - 48px));
  }
  .drop-connect-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 10px;
    font-size: var(--font-size-base);
    .drop-connect-description {
      margin: 0;
      color: var(--color-text-muted);
      line-height: 1.8;
      font-size: var(--font-size-base);
    }
    .drop-connect-hint {
      color: var(--color-text-soft);
      font-size: var(--font-size-sm);
    }
    .drop-pair-code {
      padding: 16px;
      border-radius: 8px;
      background: var(--color-primary-soft);
      color: var(--color-primary);
      font-size: var(--font-size-xl);
      letter-spacing: 6px;
      text-align: center;
    }
    .drop-pair-actions {
      display: flex;
      justify-content: flex-end;
      gap: 8px;
    }
  }
}
.drop-group-modal.base-modal {
  :deep(.base-modal__panel) {
    width: min(480px, calc(100vw - 48px));
  }
  .drop-group-form {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 18px;
    padding-top: 10px;
    font-size: var(--font-size-base);
    .drop-group-field {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .drop-group-selection {
      display: flex;
      flex-direction: column;
      gap: 8px;
      .drop-group-hint {
        color: var(--color-text-muted);
        line-height: 1.6;
        font-size: var(--font-size-base);
      }
    }
    .drop-group-devices {
      display: flex;
      min-height: 0;
      max-height: 220px;
      flex-direction: column;
      gap: 8px;
      overflow-y: auto;
      .drop-group-device {
        height: auto;
        margin-right: 0;
        padding: 10px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        .drop-group-device-copy {
          display: flex;
          flex-direction: column;
          gap: 5px;
          .drop-group-device-meta {
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
          }
        }
      }
      .drop-group-empty {
        padding: 18px 0;
        color: var(--color-text-muted);
        line-height: 1.7;
      }
    }
    .drop-group-actions {
      display: flex;
      justify-content: flex-end;
      gap: 8px;
    }
  }
}
</style>

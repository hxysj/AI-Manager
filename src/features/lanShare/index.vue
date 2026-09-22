<template>
  <section class="lan-share-view">
    <!-- 科技感顶部控制中枢 HUD -->
    <header class="cyber-header">
      <div class="cyber-brand">
        <div class="cyber-logo-box">
          <Share2 :size="17" class="cyber-logo-icon" />
          <div class="cyber-logo-ring"></div>
        </div>
        <div class="cyber-brand-text">
          <div class="cyber-brand-title">
            <span class="brand-main">设备快传</span>
            <span class="brand-tag">LAN CYBER DROP</span>
          </div>
          <span class="cyber-local-name">
            NODE // {{ state.native.deviceName || "本机节点" }}
          </span>
        </div>
      </div>

      <div class="cyber-header-actions">
        <!-- 运行状态指示 -->
        <div
          class="cyber-status-chip"
          :class="{ 'cyber-status-online': state.service.running }"
        >
          <span
            class="cyber-pulse-dot"
            :class="
              state.service.running
                ? 'cyber-pulse-dot--emerald'
                : 'cyber-pulse-dot--gray'
            "
          ></span>
          <span>{{
            state.service.running ? "ONLINE // 节点在线" : "OFFLINE // 节点离线"
          }}</span>
        </div>

        <!-- 自动发现广播开关 -->
        <button
          class="cyber-header-btn"
          :class="{ 'cyber-header-btn--active': state.native.autoDiscovery }"
          type="button"
          :disabled="togglingAutoDiscovery || !state.service.running"
          :title="
            state.native.autoDiscovery
              ? '自动广播开启中：定时发射 UDP 探测包'
              : '静默模式：已停止自动广播，可手动点击雷达扫描'
          "
          @click="toggleAutoDiscovery"
        >
          <Wifi
            v-if="state.native.autoDiscovery"
            :size="13"
            class="cyber-glow-icon"
          />
          <WifiOff v-else :size="13" />
          <span>{{
            state.native.autoDiscovery ? "自动广播: 开启" : "自动广播: 静音"
          }}</span>
        </button>

        <!-- 手动雷达扫描 -->
        <button
          class="cyber-header-btn cyber-header-btn--radar"
          :class="{ 'is-scanning': scanning }"
          type="button"
          :disabled="scanning || !state.service.running"
          title="立即向局域网广播探测，检索在线设备"
          @click="scanNearby"
        >
          <Radio :size="14" :class="{ 'cyber-spin': scanning }" />
          <span>{{ scanning ? "雷达探测中..." : "雷达扫描" }}</span>
        </button>

        <!-- 连接设备 -->
        <button
          class="cyber-header-btn"
          type="button"
          :disabled="loading"
          @click="
            state.service.running ? (connectDialogOpen = true) : startService()
          "
        >
          <Link :size="13" />
          <span>{{ state.service.running ? "连接设备" : "上线" }}</span>
        </button>

        <!-- 访客访问 -->
        <button
          class="cyber-header-btn cyber-header-btn--primary"
          type="button"
          :disabled="!state.service.running"
          @click="showAccessDialog"
        >
          <QrCode :size="13" />
          <span>访客扫码</span>
        </button>

        <!-- 设置下拉菜单 -->
        <el-dropdown trigger="click">
          <button
            class="cyber-menu-btn"
            type="button"
            aria-label="快传网络设置"
          >
            <Settings2 :size="15" />
          </button>
          <template #dropdown>
            <el-dropdown-menu class="cyber-dropdown-menu">
              <el-dropdown-item disabled>{{ serviceSummary }}</el-dropdown-item>
              <el-dropdown-item
                v-for="ip in state.service.lanIps || []"
                :key="ip"
                :disabled="
                  !state.service.running ||
                  ip === state.service.lanIp ||
                  loading
                "
                @click="setAccessIp(ip)"
              >
                二维码使用 {{ ip }}
              </el-dropdown-item>
              <el-dropdown-item
                v-if="state.service.running"
                @click="stopService"
              >
                暂停接收并离线
              </el-dropdown-item>
              <el-dropdown-item v-else @click="startService">
                重新上线
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
      </div>
    </header>

    <!-- 中枢主体区 -->
    <div class="cyber-body">
      <!-- 侧边节点导航栏 -->
      <aside class="cyber-sidebar">
        <!-- 切换 Tab: 设备与群聊 -->
        <div class="cyber-sidebar-tools">
          <div class="cyber-nav-tabs">
            <button
              class="cyber-nav-tab"
              :class="{ 'cyber-nav-tab--active': chatMode === 'direct' }"
              type="button"
              @click="switchChatMode('direct')"
            >
              <span class="tab-label">设备节点</span>
              <span class="tab-badge">{{ visibleDevices.length }}</span>
            </button>
            <button
              class="cyber-nav-tab"
              :class="{ 'cyber-nav-tab--active': chatMode === 'group' }"
              type="button"
              @click="switchChatMode('group')"
            >
              <span class="tab-label">加密群聊</span>
              <span class="tab-badge">{{ state.groups.length }}</span>
            </button>
          </div>
          <button
            v-if="chatMode === 'group'"
            class="cyber-icon-action-btn"
            type="button"
            title="创建群聊"
            aria-label="创建群聊"
            @click="openCreateGroup"
          >
            <Plus :size="15" />
          </button>
        </div>

        <!-- 搜索框 -->
        <div class="cyber-search-wrapper">
          <Search :size="13" class="cyber-search-icon" />
          <input
            v-model="deviceKeyword"
            class="cyber-search-input"
            :placeholder="
              chatMode === 'group' ? '搜索群聊信道...' : '搜索节点名称或 IP...'
            "
            aria-label="搜索设备或群聊"
          />
        </div>

        <!-- 节点列表 -->
        <nav class="cyber-conversations" aria-label="快传会话">
          <template v-if="chatMode === 'direct'">
            <div
              v-for="device in visibleDevices"
              :key="device.id"
              class="cyber-device-card"
              :class="{
                'cyber-device-card--active': selectedDeviceId === device.id
              }"
            >
              <button
                class="cyber-device-select"
                type="button"
                :disabled="deleteDevicePending"
                @click="chooseDevice(device)"
              >
                <!-- 节点图标 -->
                <div
                  class="cyber-device-avatar"
                  :class="{ 'is-online': device.online }"
                >
                  <MonitorSmartphone v-if="device.native" :size="17" />
                  <Globe v-else :size="17" />
                  <span v-if="device.online" class="cyber-online-dot"></span>
                </div>

                <!-- 节点文字信息 -->
                <div class="cyber-device-meta">
                  <div class="cyber-device-head">
                    <span
                      class="cyber-device-name"
                      :title="device.name || device.autoName"
                    >
                      {{ device.name || device.autoName || "未命名设备" }}
                    </span>
                    <span
                      class="cyber-node-tag"
                      :class="device.native ? 'tag-client' : 'tag-web'"
                    >
                      {{ device.native ? "P2P" : "WEB" }}
                    </span>
                  </div>

                  <div
                    class="cyber-device-preview"
                    :title="devicePreviewText(device)"
                  >
                    {{ devicePreviewText(device) }}
                  </div>

                  <div class="cyber-device-telemetry">
                    <span class="telemetry-ip">{{
                      device.ip || "历史设备"
                    }}</span>
                    <span
                      v-if="device.requiresPairing"
                      class="telemetry-pairing-action"
                    >
                      点击连接
                    </span>
                  </div>
                </div>
              </button>

              <button
                class="cyber-device-delete-btn"
                type="button"
                title="删除设备"
                :aria-label="`删除设备 ${device.name || device.autoName || '未命名设备'}`"
                :disabled="deleteDevicePending"
                @click="requestDeleteDevice(device)"
              >
                <Trash2 :size="13" />
              </button>
            </div>

            <!-- 空状态：雷达旋转动画 -->
            <div v-if="!visibleDevices.length" class="cyber-sidebar-empty">
              <div class="cyber-radar-hud">
                <div class="radar-circle radar-circle--1"></div>
                <div class="radar-circle radar-circle--2"></div>
                <div class="radar-sweep-hand"></div>
                <Radio :size="22" class="radar-center-icon" />
              </div>
              <span class="cyber-empty-title">SEARCHING FOR NODES</span>
              <small class="cyber-empty-hint">
                两台电脑打开设备快传即可发现彼此。点击上方「雷达扫描」发起主动探测。
              </small>
            </div>
          </template>

          <template v-else>
            <button
              v-for="group in visibleGroups"
              :key="group.id"
              class="cyber-device-card"
              :class="{
                'cyber-device-card--active': selectedGroupId === group.id
              }"
              type="button"
              @click="selectGroup(group.id)"
            >
              <div class="cyber-device-avatar">
                <Users :size="17" />
              </div>
              <div class="cyber-device-meta">
                <div class="cyber-device-head">
                  <span class="cyber-device-name">{{ group.name }}</span>
                </div>
                <div class="cyber-device-telemetry">
                  <span class="telemetry-ip"
                    >{{ group.members?.length || 0 }} 位成员</span
                  >
                </div>
              </div>
            </button>

            <div v-if="!visibleGroups.length" class="cyber-sidebar-empty">
              <div class="cyber-radar-hud">
                <div class="radar-circle radar-circle--1"></div>
                <div class="radar-circle radar-circle--2"></div>
                <Users :size="22" class="radar-center-icon" />
              </div>
              <span class="cyber-empty-title">NO ACTIVE GROUPS</span>
              <small class="cyber-empty-hint">
                点击右上角「+」即可创建加密传输群聊。
              </small>
            </div>
          </template>
        </nav>

        <!-- 底部网络遥测 -->
        <div class="cyber-sidebar-telemetry">
          <div class="telemetry-row">
            <span class="telemetry-label">CHANNEL:</span>
            <span class="telemetry-val">P2P DIRECT ENCRYPTED</span>
          </div>
          <div class="telemetry-row">
            <span class="telemetry-label">SECURITY:</span>
            <span class="telemetry-val">ED25519 // AUTH VERIFIED</span>
          </div>
        </div>
      </aside>

      <!-- 消息工作台 -->
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
        @download-file="downloadFile"
        @copy-text="copyText"
      />
    </div>

    <!-- 删除设备确认弹窗 -->
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

    <!-- 访客访问二维码弹窗 -->
    <LanShareAccessDialog
      v-if="accessDialogOpen"
      :qr-svg="accessQrSvg"
      :access-url="state.service.accessUrl"
      @close="accessDialogOpen = false"
      @copy-url="copyAccessUrl"
    />

    <!-- 文件预览弹窗 -->
    <LanSharePreviewDialog
      v-if="previewDialog.open"
      :file="previewDialog.file"
      :preview-url="previewDialog.previewUrl"
      :preview-kind="previewDialog.previewKind"
      :text-content="previewDialog.textContent"
      :saving="savingFile"
      @close="closePreviewDialog"
      @download="downloadFile"
    />

    <!-- 文件保存成功提示 -->
    <BaseModal
      v-if="savedFile"
      class="drop-save-success-modal"
      title="文件保存成功"
      @close="savedFile = null"
    >
      <div class="drop-save-success-content">
        <p>“{{ savedFile.name }}”已保存到本地。</p>
        <span class="drop-save-success-path" :title="savedFile.path">
          {{ savedFile.path }}
        </span>
        <div class="drop-save-success-actions">
          <el-button @click="savedFile = null">关闭</el-button>
          <el-button type="primary" @click="openSavedFileDirectory">
            打开所在目录
          </el-button>
        </div>
      </div>
    </BaseModal>

    <!-- 手动输入地址连接弹窗 -->
    <BaseModal
      v-if="connectDialogOpen"
      class="drop-connect-modal"
      title="手动连接节点"
      @close="connectDialogOpen = false"
    >
      <div class="drop-connect-content">
        <p class="drop-connect-description">
          输入对方局域网节点的 IP
          与端口（或设备快传访问链接），向其发起安全直连配对请求。
        </p>
        <el-input
          v-model="connectAddress"
          placeholder="例如：192.168.1.8:17631"
          @keyup.enter="connectByAddress"
        />
        <span class="drop-connect-hint"
          >连接后两台电脑屏幕上将显示安全校验码，确认后即建立可信信道。</span
        >
        <div class="drop-connect-actions">
          <el-button @click="connectDialogOpen = false">取消</el-button>
          <el-button
            type="primary"
            :loading="loading"
            :disabled="!connectAddress.trim()"
            @click="connectByAddress"
          >
            发起直连请求
          </el-button>
        </div>
      </div>
    </BaseModal>

    <!-- 创建群聊弹窗 -->
    <BaseModal
      v-if="createGroupOpen"
      class="drop-group-modal"
      title="创建加密群聊信道"
      @close="createGroupOpen = false"
    >
      <form class="drop-group-form" @submit.prevent="createGroup">
        <label class="drop-group-field">
          <span>群名称</span>
          <el-input
            v-model="newGroup.name"
            placeholder="输入群名称"
            :disabled="loading"
          />
        </label>
        <div class="drop-group-selection">
          <span>邀请设备 · 已选 {{ selectedGroupDeviceIds.length }} 台</span>
          <span class="drop-group-hint"
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
            <span class="drop-group-device-copy">
              <span>{{ device.name || device.autoName }}</span>
              <span class="drop-group-device-meta">
                {{ device.ip }} ·
                {{
                  device.native
                    ? "客户端暂仅支持单聊"
                    : device.online
                      ? "在线"
                      : "离线"
                }}
              </span>
            </span>
          </el-checkbox>
          <span v-if="!state.devices.length" class="drop-group-empty">
            暂无可邀请设备，请先让至少一台网页设备通过访客访问连接。
          </span>
        </div>
        <div class="drop-group-actions">
          <el-button :disabled="loading" @click="createGroupOpen = false">
            取消
          </el-button>
          <el-button
            native-type="submit"
            type="primary"
            :loading="loading"
            :disabled="!selectedGroupDeviceIds.length || !newGroup.name.trim()"
          >
            创建并邀请
          </el-button>
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
  Radio,
  Search,
  Settings2,
  Share2,
  Trash2,
  Users,
  Wifi,
  WifiOff
} from "lucide-vue-next"
import { lanShareApi, systemApi } from "@/api"
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
  native: {
    deviceName: "",
    peers: [],
    pairingRequests: [],
    error: "",
    autoDiscovery: false
  },
  messages: []
})

const loading = ref(false)
const scanning = ref(false)
const togglingAutoDiscovery = ref(false)
const savingFile = ref(false)
const savedFile = ref(null)
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

function devicePreviewText(device) {
  if (device.pairingCode) {
    return `等待对方确认 · ${device.pairingCode}`
  }
  if (device.requiresPairing) {
    return "发现局域网客户端 · 点击连接"
  }
  return deviceLastMessage(device.id)
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

async function toggleAutoDiscovery() {
  if (togglingAutoDiscovery.value) return
  togglingAutoDiscovery.value = true
  try {
    const next = !state.native.autoDiscovery
    await lanShareApi.setAutoDiscovery({ enabled: next })
    state.native.autoDiscovery = next
    if (next) {
      createMessage.success("已开启后台自动广播发现")
    } else {
      createMessage.info(
        "已切换为静默模式 (已停止后台广播，可手动点击雷达扫描)"
      )
    }
  } catch (err) {
    createMessage.error(err?.message || "切换自动广播失败")
  } finally {
    togglingAutoDiscovery.value = false
  }
}

async function scanNearby() {
  if (scanning.value) return
  scanning.value = true
  try {
    await lanShareApi.scanDevices()
    createMessage.info("已发射雷达探测波，正在检索局域网节点...")
    setTimeout(() => {
      scanning.value = false
    }, 2000)
  } catch (err) {
    createMessage.error(err?.message || "雷达探测失败")
    scanning.value = false
  }
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
  if (
    !state.service.running ||
    !lanIp ||
    lanIp === state.service.lanIp ||
    loading.value
  ) {
    return
  }
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

async function downloadFile(file) {
  if (savingFile.value) return
  savingFile.value = true
  try {
    const targetPath = await systemApi.saveFile({
      title: "保存快传文件",
      defaultPath: file.name
    })
    if (!targetPath) return
    await lanShareApi.saveFile({ fileId: file.id, targetPath })
    savedFile.value = {
      name: file.name,
      path: targetPath,
      directory: targetPath.replace(/[\\/][^\\/]*$/, "") || targetPath
    }
  } catch (error) {
    createMessage.error(`保存失败：${error?.message || error}`)
  } finally {
    savingFile.value = false
  }
}

async function openSavedFileDirectory() {
  if (!savedFile.value?.directory) return
  try {
    await systemApi.openPath({ targetPath: savedFile.value.directory })
  } catch (error) {
    createMessage.error(`打开目录失败：${error?.message || error}`)
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
  if (mimeType.startsWith("image/")) return "image"
  if (mimeType.startsWith("video/")) return "video"
  if (mimeType.startsWith("audio/")) return "audio"
  if (mimeType === "application/pdf" || name.endsWith(".pdf")) return "pdf"
  if (mimeType.startsWith("text/") || isTextPreviewFile(name, mimeType))
    return "text"
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
  background: var(--color-page);
  padding: 10px 14px 14px;
  box-sizing: border-box;

  /* 顶部科技感控制中枢 HUD */
  .cyber-header {
    display: flex;
    min-width: 0;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 10px 16px;
    margin-bottom: 12px;
    border-radius: 10px;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    box-shadow: var(--shadow-panel);

    .cyber-brand {
      display: flex;
      align-items: center;
      gap: 12px;

      .cyber-logo-box {
        position: relative;
        display: grid;
        width: 36px;
        height: 36px;
        place-items: center;
        border-radius: 8px;
        background: var(--color-primary-soft);
        border: 1px solid var(--color-info-line);
        color: var(--color-primary);
        box-shadow: 0 0 10px var(--color-primary-soft);

        .cyber-logo-ring {
          position: absolute;
          inset: -3px;
          border-radius: 10px;
          border: 1px dashed var(--color-primary);
          opacity: 0.5;
          animation: cyberSpin 15s linear infinite;
        }
      }

      .cyber-brand-text {
        display: flex;
        flex-direction: column;
        gap: 2px;

        .cyber-brand-title {
          display: flex;
          align-items: center;
          gap: 8px;

          .brand-main {
            color: var(--color-text);
            font-size: 16px;
            font-weight: 700;
            letter-spacing: 0.5px;
          }

          .brand-tag {
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 9.5px;
            padding: 1px 6px;
            border-radius: 3px;
            background: var(--color-primary-soft);
            border: 1px solid var(--color-info-line);
            color: var(--color-primary);
            letter-spacing: 1px;
            font-weight: 600;
          }
        }

        .cyber-local-name {
          color: var(--color-text-muted);
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11px;
          letter-spacing: 0.5px;
        }
      }
    }

    .cyber-header-actions {
      display: flex;
      align-items: center;
      gap: 8px;

      .cyber-status-chip {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 5px 10px;
        border-radius: 6px;
        background: var(--color-panel-soft);
        border: 1px solid var(--color-line);
        font-family:
          ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        font-size: 11px;
        color: var(--color-text-muted);

        &.cyber-status-online {
          background: var(--color-success-soft);
          border-color: var(--color-success-line);
          color: var(--color-success);
        }
      }

      .cyber-header-btn {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        height: 31px;
        padding: 0 11px;
        border-radius: 6px;
        border: 1px solid var(--color-line);
        background: var(--color-panel-soft);
        color: var(--color-text);
        font-size: 12.5px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s;

        &:hover:not(:disabled) {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }

        &--active {
          border-color: var(--color-success-line);
          background: var(--color-success-soft);
          color: var(--color-success);
        }

        &--radar {
          &.is-scanning {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            color: var(--color-primary);
            box-shadow: 0 0 10px var(--color-primary-soft);
          }
        }

        &--primary {
          border-color: var(--color-primary);
          background: var(--color-primary-solid);
          color: #ffffff;

          &:hover:not(:disabled) {
            background: var(--color-primary);
            box-shadow: 0 2px 10px rgba(0, 0, 0, 0.12);
          }
        }

        &:disabled {
          opacity: 0.4;
          cursor: not-allowed;
        }
      }

      .cyber-menu-btn {
        display: grid;
        width: 31px;
        height: 31px;
        place-items: center;
        padding: 0;
        border-radius: 6px;
        border: 1px solid var(--color-line);
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s;

        &:hover {
          color: var(--color-primary);
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
        }
      }
    }
  }

  /* 主体双栏区域 */
  .cyber-body {
    display: flex;
    min-height: 0;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel);
    box-shadow: var(--shadow-panel);

    /* 侧边节点栏 */
    .cyber-sidebar {
      display: flex;
      width: clamp(230px, 24%, 280px);
      min-width: 0;
      flex: none;
      flex-direction: column;
      border-right: 1px solid var(--color-line);
      background: var(--color-panel-soft);

      .cyber-sidebar-tools {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        padding: 12px 14px 10px;

        .cyber-nav-tabs {
          display: flex;
          gap: 6px;

          .cyber-nav-tab {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            padding: 5px 9px;
            border-radius: 5px;
            border: 1px solid transparent;
            background: transparent;
            color: var(--color-text-muted);
            font-size: 12.5px;
            font-weight: 500;
            cursor: pointer;
            transition: all 0.2s;

            .tab-badge {
              font-family:
                ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
              font-size: 10px;
              padding: 0 5px;
              border-radius: 3px;
              background: var(--color-panel);
              border: 1px solid var(--color-line);
              color: var(--color-text-soft);
            }

            &:hover {
              color: var(--color-text);
            }

            &--active {
              color: var(--color-primary);
              background: var(--color-panel);
              border-color: var(--color-line);
              box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);

              .tab-badge {
                background: var(--color-primary-soft);
                border-color: var(--color-info-line);
                color: var(--color-primary);
              }
            }
          }
        }

        .cyber-icon-action-btn {
          display: grid;
          width: 26px;
          height: 26px;
          place-items: center;
          padding: 0;
          border-radius: 5px;
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          color: var(--color-primary);
          cursor: pointer;
          transition: all 0.2s;

          &:hover {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
          }
        }
      }

      .cyber-search-wrapper {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;
        margin: 0 12px 10px;
        padding: 6px 10px;
        border-radius: 6px;
        background: var(--color-panel);
        border: 1px solid var(--color-line);
        transition: all 0.2s;

        &:focus-within {
          border-color: var(--color-primary);
          box-shadow: 0 0 0 2px var(--color-primary-soft);
        }

        .cyber-search-icon {
          color: var(--color-text-soft);
          flex-shrink: 0;
        }

        .cyber-search-input {
          width: 100%;
          min-width: 0;
          padding: 0;
          border: 0;
          outline: none;
          background: transparent;
          color: var(--color-text);
          font-size: 12px;

          &::placeholder {
            color: var(--color-text-soft);
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 11px;
          }
        }
      }

      .cyber-conversations {
        min-height: 0;
        flex: 1;
        padding: 0 8px;
        overflow-y: auto;
        scrollbar-width: thin;
        scrollbar-color: var(--color-line) transparent;

        .cyber-device-card {
          display: flex;
          align-items: center;
          gap: 6px;
          margin-bottom: 6px;
          padding: 8px 10px;
          border-radius: 7px;
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          transition: all 0.2s;

          &:hover {
            border-color: var(--color-line-strong);
            background: var(--color-panel-soft);
          }

          &--active {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            box-shadow: 0 1px 4px rgba(0, 0, 0, 0.04);
          }

          .cyber-device-select {
            display: flex;
            min-width: 0;
            flex: 1;
            align-items: center;
            gap: 10px;
            padding: 0;
            border: 0;
            background: transparent;
            color: inherit;
            text-align: left;
            cursor: pointer;

            .cyber-device-avatar {
              position: relative;
              display: grid;
              width: 34px;
              height: 34px;
              flex: none;
              place-items: center;
              border-radius: 7px;
              background: var(--color-panel-soft);
              border: 1px solid var(--color-line);
              color: var(--color-text-muted);

              &.is-online {
                color: var(--color-success);
                border-color: var(--color-success-line);
                background: var(--color-success-soft);
                box-shadow: 0 0 6px var(--color-success-soft);
              }

              .cyber-online-dot {
                position: absolute;
                right: -2px;
                bottom: -2px;
                width: 7px;
                height: 7px;
                border-radius: 50%;
                background: var(--color-success);
                box-shadow: 0 0 6px var(--color-success);
                border: 1.5px solid var(--color-panel);
              }
            }

            .cyber-device-meta {
              display: flex;
              min-width: 0;
              flex: 1;
              flex-direction: column;
              gap: 3px;

              .cyber-device-head {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 6px;

                .cyber-device-name {
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                  font-size: 13px;
                  font-weight: 600;
                  color: var(--color-text);
                }

                .cyber-node-tag {
                  font-family:
                    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                    monospace;
                  font-size: 9px;
                  padding: 1px 4px;
                  border-radius: 3px;
                  font-weight: 500;

                  &.tag-client {
                    background: var(--color-primary-soft);
                    border: 1px solid var(--color-info-line);
                    color: var(--color-primary);
                  }

                  &.tag-web {
                    background: var(--color-warning-soft);
                    border: 1px solid var(--color-warning-line);
                    color: var(--color-warning);
                  }
                }
              }

              .cyber-device-preview {
                overflow: hidden;
                color: var(--color-text-muted);
                text-overflow: ellipsis;
                white-space: nowrap;
                font-size: 11.5px;
                line-height: 1.35;
              }

              .cyber-device-telemetry {
                display: flex;
                align-items: center;
                gap: 6px;
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;
                font-size: 10px;

                .telemetry-ip {
                  color: var(--color-text-soft);
                }

                .telemetry-pairing-action {
                  color: var(--color-primary);
                  text-decoration: underline;
                }
              }
            }
          }

          .cyber-device-delete-btn {
            display: grid;
            width: 24px;
            height: 24px;
            place-items: center;
            border: 0;
            border-radius: 4px;
            background: transparent;
            color: var(--color-text-soft);
            cursor: pointer;
            opacity: 0;
            transition: all 0.15s;

            &:hover:not(:disabled) {
              color: var(--color-danger);
              background: var(--color-danger-soft);
            }
          }

          &:hover .cyber-device-delete-btn {
            opacity: 1;
          }
        }

        .cyber-sidebar-empty {
          display: flex;
          align-items: center;
          flex-direction: column;
          gap: 12px;
          padding: 36px 16px;
          text-align: center;

          .cyber-radar-hud {
            position: relative;
            width: 70px;
            height: 70px;
            display: grid;
            place-items: center;

            .radar-circle {
              position: absolute;
              border-radius: 50%;
              border: 1px solid var(--color-line);

              &--1 {
                width: 46px;
                height: 46px;
                border-style: dashed;
                border-color: var(--color-primary);
                opacity: 0.55;
                animation: cyberSpin 14s linear infinite;
              }

              &--2 {
                width: 68px;
                height: 68px;
                border-color: var(--color-line-strong);
              }
            }

            .radar-sweep-hand {
              position: absolute;
              top: 0;
              left: 50%;
              width: 50%;
              height: 50%;
              transform-origin: 0% 100%;
              background: linear-gradient(
                135deg,
                var(--color-primary),
                transparent 70%
              );
              opacity: 0.25;
              border-radius: 0 100% 0 0;
              animation: cyberSweep 3.5s linear infinite;
              pointer-events: none;
            }

            .radar-center-icon {
              color: var(--color-primary);
            }
          }

          .cyber-empty-title {
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 11px;
            letter-spacing: 1.5px;
            color: var(--color-primary);
            font-weight: 600;
          }

          .cyber-empty-hint {
            font-size: 11.5px;
            color: var(--color-text-muted);
            line-height: 1.6;
          }
        }
      }

      .cyber-sidebar-telemetry {
        flex: none;
        padding: 10px 14px;
        border-top: 1px solid var(--color-line);
        background: var(--color-panel-soft);
        display: flex;
        flex-direction: column;
        gap: 4px;

        .telemetry-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 9.5px;

          .telemetry-label {
            color: var(--color-text-soft);
          }

          .telemetry-val {
            color: var(--color-primary);
          }
        }
      }
    }
  }
}

/* 弹窗通用样式 */
.drop-connect-modal.base-modal,
.drop-group-modal.base-modal {
  .drop-connect-content,
  .drop-group-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-top: 6px;
    color: var(--color-text);

    .drop-connect-description {
      margin: 0;
      color: var(--color-text-muted);
      font-size: 12.5px;
      line-height: 1.6;
    }

    .drop-connect-hint {
      color: var(--color-text-soft);
      font-size: 11.5px;
    }

    .drop-connect-actions,
    .drop-group-actions {
      display: flex;
      justify-content: flex-end;
      gap: 10px;
      margin-top: 8px;
    }
  }

  .drop-group-devices {
    display: flex;
    max-height: 200px;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;

    .drop-group-device {
      height: auto;
      margin-right: 0;
      padding: 8px 10px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);
      border-radius: 6px;
      color: var(--color-text);
    }
  }
}

/* 动效 */
.cyber-pulse-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;

  &--emerald {
    background: var(--color-success);
    box-shadow: 0 0 6px var(--color-success);
    animation: cyberPulse 1.8s ease-in-out infinite;
  }

  &--gray {
    background: var(--color-text-soft);
  }
}

@keyframes cyberSpin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@keyframes cyberSweep {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@keyframes cyberPulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.5;
    transform: scale(0.85);
  }
}

.cyber-spin {
  animation: cyberSpin 0.9s linear infinite;
}
</style>

<template>
  <section class="lan-share-view">
    <!-- 科技感顶部控制中枢 HUD -->
    <header class="cyber-header">
      <div class="cyber-brand">
        <div class="cyber-logo-box">
          <Share2 :size="18" class="cyber-logo-icon" />
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

        <!-- 连接设备 -->
        <button
          class="cyber-header-btn"
          type="button"
          :disabled="loading"
          @click="openConnectDialog"
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

        <!-- 设置改为居中弹框，避免下拉菜单被顶部区域裁剪。 -->
        <button
          class="cyber-menu-btn"
          type="button"
          aria-label="快传网络设置"
          :aria-expanded="settingsOpen"
          @click="settingsOpen = true"
        >
          <Settings2 :size="15" />
        </button>
      </div>
    </header>

    <BaseModal
      v-if="settingsOpen"
      class="drop-settings-modal"
      title="设备快传设置"
      description="管理传输、节点可见性和访问地址"
      @close="settingsOpen = false"
    >
      <div class="drop-settings-content">
        <div
          class="drop-settings-status"
          :class="{ 'is-online': state.service.running }"
        >
          <span class="drop-settings-status-dot"></span>
          <div class="drop-settings-status-copy">
            <span>{{ state.service.running ? "服务在线" : "服务已暂停" }}</span>
            <small>{{ serviceSummary }}</small>
          </div>
        </div>

        <div class="drop-settings-list">
          <button
            class="drop-settings-item"
            type="button"
            @click="openTransferSettings"
          >
            <span class="drop-settings-item-icon"
              ><Settings2 :size="17"
            /></span>
            <span class="drop-settings-item-copy">
              <span>传输限速与并发</span>
              <small>控制文件传输速度和同时处理的文件数</small>
            </span>
            <ChevronRight :size="16" class="drop-settings-item-arrow" />
          </button>

          <button
            class="drop-settings-item"
            type="button"
            :disabled="togglingHidden"
            @click="toggleHidden"
          >
            <span class="drop-settings-item-icon"
              ><ShieldCheck :size="17"
            /></span>
            <span class="drop-settings-item-copy">
              <span>{{
                state.native.hidden ? "恢复节点可见" : "隐藏本机节点"
              }}</span>
              <small>{{
                state.native.hidden
                  ? "已配对设备仍可连接本机"
                  : "隐藏后不会回应新的发现和配对请求"
              }}</small>
            </span>
            <ChevronRight :size="16" class="drop-settings-item-arrow" />
          </button>
        </div>

        <div class="drop-settings-addresses">
          <div class="drop-settings-section-title">
            <QrCode :size="15" />
            <span>二维码访问地址</span>
          </div>
          <div class="drop-settings-address-list">
            <button
              v-for="ip in state.service.lanIps || []"
              :key="ip"
              class="drop-settings-address"
              :class="{ 'is-active': ip === state.service.lanIp }"
              type="button"
              :disabled="
                !state.service.running || ip === state.service.lanIp || loading
              "
              @click="chooseAccessIp(ip)"
            >
              <span>{{ ip }}</span>
              <span>{{
                ip === state.service.lanIp ? "当前使用" : "切换"
              }}</span>
            </button>
            <span
              v-if="!(state.service.lanIps || []).length"
              class="drop-settings-empty"
            >
              服务上线后显示局域网地址
            </span>
          </div>
        </div>

        <button
          v-if="state.service.running"
          class="drop-settings-offline"
          type="button"
          @click="stopService"
        >
          <Power :size="16" />
          <span>暂停接收并离线</span>
        </button>
        <button
          v-else
          class="drop-settings-offline"
          type="button"
          @click="startService"
        >
          <Power :size="16" />
          <span>重新上线</span>
        </button>
      </div>
    </BaseModal>

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
                点击「连接设备」扫描当前局域网，等待对方同意后建立连接。
              </small>
            </div>
          </template>

          <template v-else>
            <div
              v-for="group in visibleGroups"
              :key="group.id"
              class="cyber-device-card"
              :class="{
                'cyber-device-card--active': selectedGroupId === group.id
              }"
            >
              <button
                class="cyber-device-select"
                type="button"
                @click="selectGroup(group.id)"
              >
                <div class="cyber-device-avatar cyber-device-avatar--group">
                  <Users :size="17" />
                </div>
                <div class="cyber-device-meta">
                  <div class="cyber-device-head">
                    <span class="cyber-device-name" :title="group.name">{{
                      group.name
                    }}</span>
                    <span class="cyber-node-tag tag-group">GROUP</span>
                  </div>

                  <div
                    class="cyber-device-preview"
                    :title="groupLastMessage(group.id)"
                  >
                    {{ groupLastMessage(group.id) }}
                  </div>

                  <div class="cyber-device-telemetry">
                    <span class="telemetry-ip"
                      >{{ group.members?.length || 0 }} 位成员</span
                    >
                  </div>
                </div>
              </button>

              <button
                class="cyber-device-delete-btn"
                type="button"
                title="解散群聊"
                :aria-label="`解散群聊 ${group.name}`"
                @click="deleteGroup(group.id)"
              >
                <Trash2 :size="13" />
              </button>
            </div>

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
            <span class="telemetry-val">
              <Lock :size="12" class="telemetry-icon" />
              <span>P2P DIRECT ENCRYPTED</span>
            </span>
          </div>
          <div class="telemetry-row">
            <span class="telemetry-label">SECURITY:</span>
            <span class="telemetry-val">
              <ShieldCheck :size="12" class="telemetry-icon" />
              <span>ED25519 // AUTH VERIFIED</span>
            </span>
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
      class="drop-delete-device-modal"
      title="删除设备"
      :name="deleteDeviceTarget.name"
      :description="
        deleteDeviceTarget.native
          ? '解除此设备的配对并从节点列表移除。聊天记录默认保留，重新连接后可以继续查看。'
          : '移除此设备及其群聊成员关联。聊天记录默认保留，重新接入后可以继续查看。'
      "
      :pending="deleteDevicePending"
      :error="deleteDeviceError"
      @close="closeDeleteDevice"
      @confirm="confirmDeleteDevice"
    >
      <el-checkbox
        v-model="deleteDeviceWithHistory"
        :disabled="deleteDevicePending"
      >
        同时彻底删除聊天记录和接收文件
      </el-checkbox>
      <span v-if="deleteDeviceWithHistory" class="drop-delete-warning">
        将永久删除本机与该设备的单聊记录、接收文件及未完成的接收缓存。发送的原始文件保留。
      </span>
    </DeleteConfirmModal>

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

    <!-- 扫描候选只在连接弹窗展示，双方确认后才加入设备节点列表。 -->
    <BaseModal
      v-if="connectDialogOpen"
      class="drop-connect-modal"
      title="连接设备"
      @close="connectDialogOpen = false"
    >
      <div class="drop-connect-content">
        <p class="drop-connect-description">
          选择局域网内发现的设备，核对安全校验码并等待对方同意后建立连接。
        </p>
        <el-input
          v-model="connectIpFilter"
          placeholder="按 IP 地址过滤，例如：192.168.1"
          aria-label="按 IP 地址过滤发现的设备"
          clearable
        />
        <div class="drop-connect-list" aria-label="发现的设备">
          <button
            v-for="peer in discoveredPeers"
            :key="peer.id"
            class="drop-connect-device"
            type="button"
            :disabled="loading || !!peer.pairingCode"
            @click="connectDiscoveredPeer(peer)"
          >
            <MonitorSmartphone :size="18" />
            <span class="drop-connect-device-info">
              <span class="drop-connect-device-name">{{
                peer.name || "未命名设备"
              }}</span>
              <span class="drop-connect-device-ip"
                >{{ peer.ip }}:{{ peer.port }}</span
              >
            </span>
            <span class="drop-connect-device-status">
              {{ peer.pairingCode ? `等待同意 · ${peer.pairingCode}` : "连接" }}
            </span>
          </button>
          <span v-if="!discoveredPeers.length" class="drop-connect-empty">
            {{
              scanning || state.native.scanning
                ? "正在扫描局域网设备…"
                : connectIpFilter.trim()
                  ? "没有匹配该 IP 的设备"
                  : "暂无可连接设备，请确认对方已上线且未隐藏节点"
            }}
          </span>
        </div>
        <p v-if="state.native.error" class="drop-connect-error" role="alert">
          {{ state.native.error }}
        </p>
        <span class="drop-connect-hint"
          >未确认、已拒绝和超时的设备不会加入设备节点列表。隐藏节点仍可被已配对设备识别。</span
        >
        <div class="drop-connect-actions">
          <el-button @click="connectDialogOpen = false">关闭</el-button>
          <el-button
            type="primary"
            :loading="scanning || state.native.scanning"
            :disabled="loading || scanning || state.native.scanning"
            @click="scanNearby"
          >
            重新扫描
          </el-button>
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-if="transferSettingsOpen"
      class="drop-transfer-modal"
      title="传输限速与并发"
      @close="transferSettingsOpen = false"
    >
      <form class="drop-transfer-form" @submit.prevent="saveTransferSettings">
        <p class="drop-transfer-hint">
          限制本设备发送和接收文件的总速度，客户端和网页传输均生效。 默认 2
          MiB/s，Wi-Fi 卡顿时可继续降低限速。
        </p>
        <label class="drop-transfer-field">
          <span>文件传输总限速（MiB/s）</span>
          <el-input-number
            v-model="transferSettings.sendMibPerSecond"
            :min="1"
            :max="1024"
            :precision="0"
            :disabled="loading"
            aria-label="文件传输总限速，单位 MiB 每秒"
          />
        </label>
        <label class="drop-transfer-field">
          <span>每个方向同时传输的文件数</span>
          <el-input-number
            v-model="transferSettings.maxConcurrentFiles"
            :min="1"
            :max="8"
            :precision="0"
            :disabled="loading"
            aria-label="每个方向同时传输的文件数"
          />
        </label>
        <p class="drop-transfer-hint">
          保存后立即生效并保留设置。降低并发时，正在传输的文件继续完成。
        </p>
        <div class="drop-transfer-actions">
          <el-button :disabled="loading" @click="transferSettingsOpen = false">
            取消
          </el-button>
          <el-button
            type="primary"
            native-type="submit"
            :loading="loading"
            :disabled="
              !transferSettings.sendMibPerSecond ||
              !transferSettings.maxConcurrentFiles
            "
          >
            保存设置
          </el-button>
        </div>
      </form>
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
            >群聊至少需要 3 人（包含本机），请至少选择两台设备。离线的网页设备重新连接后可查看邀请。</span
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
            :disabled="selectedGroupDeviceIds.length < 2 || !newGroup.name.trim()"
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
  ElInput,
  ElInputNumber
} from "element-plus"
import "element-plus/es/components/button/style/css"
import "element-plus/es/components/checkbox/style/css"
import "element-plus/es/components/input/style/css"
import "element-plus/es/components/input-number/style/css"
import {
  ChevronRight,
  Globe,
  Link,
  Lock,
  MonitorSmartphone,
  Plus,
  QrCode,
  Radio,
  Search,
  Settings2,
  Share2,
  ShieldCheck,
  Trash2,
  Users,
  Power
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
    hidden: false,
    scanning: false,
    transfer: { sendMibPerSecond: 2, maxConcurrentFiles: 1 }
  },
  messages: []
})

const loading = ref(false)
const scanning = ref(false)
const togglingHidden = ref(false)
const savingFile = ref(false)
const savedFile = ref(null)
const deleteDeviceTarget = ref(null)
const deleteDevicePending = ref(false)
const deleteDeviceError = ref("")
const deleteDeviceWithHistory = ref(false)
const createGroupOpen = ref(false)
const transferSettingsOpen = ref(false)
// 控制设备快传设置弹框。
const settingsOpen = ref(false)
// 默认使用较低的总速率，避免文件收发挤占同一无线网络。
const transferSettings = reactive({
  sendMibPerSecond: 2,
  maxConcurrentFiles: 1
})
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
const connectIpFilter = ref("")
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
let stopNativeListener = null
let scanTimer = null
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
    connectedDevices.value.find(
      (device) => device.id === selectedDeviceId.value
    ) || null
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

// 扫描候选和已配对设备使用同一发现状态，但主列表仅展示已确认的连接。
const connectedDevices = computed(() => {
  const devices = new Map(
    state.devices
      .filter((device) => !device.native)
      .map((device) => [device.id, device])
  )
  for (const peer of state.native.peers || []) {
    if (!peer.paired) continue
    devices.set(peer.id, {
      ...state.devices.find((device) => device.id === peer.id),
      ...peer,
      native: true
    })
  }
  return [...devices.values()]
})

const visibleDevices = computed(() => {
  const query = deviceKeyword.value.trim().toLowerCase()
  return connectedDevices.value
    .filter((device) =>
      `${device.name || device.autoName || ""} ${device.ip || ""}`
        .toLowerCase()
        .includes(query)
    )
    .sort((left, right) => Number(right.online) - Number(left.online))
})

const discoveredPeers = computed(() => {
  const ip = connectIpFilter.value.trim().toLowerCase()
  return (state.native.peers || [])
    .filter(
      (peer) =>
        !peer.paired &&
        (peer.online || peer.pairingCode) &&
        peer.ip.toLowerCase().includes(ip)
    )
    .sort((left, right) =>
      left.ip.localeCompare(right.ip, undefined, { numeric: true })
    )
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

function groupLastMessage(groupId) {
  const session = groupSessions.value.find((item) => item.groupId === groupId)
  if (!session) return "群聊传输信道"
  const message = state.messages.find((item) => item.sessionId === session.id)
  if (!message) return "发送群文件或消息"
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
  return deviceLastMessage(device.id)
}

async function chooseDevice(device) {
  await openDeviceSessions(device.id)
}

async function openConnectDialog() {
  if (!state.service.running) await startService()
  if (!state.service.running) return
  connectIpFilter.value = ""
  connectDialogOpen.value = true
  await scanNearby()
}

async function connectDiscoveredPeer(peer) {
  if (loading.value || peer.pairingCode || peer.paired) return
  await runAction(() => lanShareApi.connectPeer({ peerId: peer.id }))
}

async function toggleHidden() {
  if (togglingHidden.value) return
  settingsOpen.value = false
  togglingHidden.value = true
  try {
    const result = unwrapData(
      await lanShareApi.setHidden({ hidden: !state.native.hidden })
    )
    state.native = result
    createMessage.success(
      result.hidden ? "节点已隐藏，已配对设备仍可连接" : "节点已恢复可见"
    )
  } catch (error) {
    createMessage.error(error?.message || "设置节点可见性失败")
  } finally {
    togglingHidden.value = false
  }
}

// 打开时读取已保存设置，后台状态事件不会覆盖正在编辑的数值。
function openTransferSettings() {
  settingsOpen.value = false
  Object.assign(transferSettings, state.native.transfer)
  transferSettingsOpen.value = true
}

async function saveTransferSettings() {
  if (loading.value) return
  const result = await runAction(
    () => lanShareApi.setTransferSettings({ ...transferSettings }),
    "传输设置已生效"
  )
  if (result) transferSettingsOpen.value = false
}

async function scanNearby() {
  if (scanning.value) return
  scanning.value = true
  try {
    await lanShareApi.scanDevices()
    clearTimeout(scanTimer)
    scanTimer = setTimeout(() => {
      scanning.value = false
    }, 2000)
  } catch (err) {
    createMessage.error(err?.message || "设备扫描失败")
    scanning.value = false
  }
}

onMounted(async () => {
  stopStateListener = lanShareApi.onStateChanged(applyState)
  stopDevicesListener = lanShareApi.onDevicesChanged((devices) => {
    state.devices = devices || []
  })
  stopNativeListener = lanShareApi.onNativeChanged((native) => {
    state.native = native
    // 原生设备的心跳独立推送，收到后同步群聊成员在线状态，避免重新加载聊天记录。
    const onlineById = new Map(
      (native.peers || []).map((peer) => [peer.id, Boolean(peer.online)])
    )
    state.groups = state.groups.map((group) => ({
      ...group,
      members: (group.members || []).map((member) =>
        onlineById.has(member.deviceId)
          ? { ...member, online: onlineById.get(member.deviceId) }
          : member
      )
    }))
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
  if (stopNativeListener) stopNativeListener()
  clearTimeout(scanTimer)
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
    connectedDevices.value.length
  ) {
    selectedDeviceId.value = connectedDevices.value[0].id
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
  settingsOpen.value = false
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
  settingsOpen.value = false
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

function chooseAccessIp(lanIp) {
  settingsOpen.value = false
  return setAccessIp(lanIp)
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
    selectedGroupDeviceIds.value.length < 2 ||
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
  deleteDeviceWithHistory.value = false
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
    const result = unwrapData(
      await lanShareApi.deleteDevice({
        deviceId,
        deleteHistory: deleteDeviceWithHistory.value
      })
    )
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
        flex-shrink: 0;
        align-items: center;
        gap: 6px;
        height: 31px;
        padding: 0 11px;
        white-space: nowrap;
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
          color: #ffffff !important;
          min-width: 82px;

          svg,
          span {
            color: #ffffff !important;
          }

          &:hover:not(:disabled) {
            background: var(--color-primary);
            color: #ffffff !important;
            box-shadow: 0 2px 10px rgba(0, 0, 0, 0.12);
          }

          &:hover:not(:disabled) svg,
          &:hover:not(:disabled) span { color: #ffffff !important; }
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
    gap: 12px;

    /* 侧边节点栏独立卡片 */
    .cyber-sidebar {
      display: flex;
      width: clamp(230px, 24%, 280px);
      min-width: 0;
      flex: none;
      flex-direction: column;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      box-shadow: var(--shadow-panel);
      overflow: hidden;

      .cyber-sidebar-tools {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        padding: 0 14px;
        height: 44px;
        border-bottom: 1px solid var(--color-line);

        .cyber-nav-tabs {
          display: flex;
          gap: 16px;
          height: 100%;

          .cyber-nav-tab {
            position: relative;
            display: inline-flex;
            align-items: center;
            gap: 6px;
            height: 100%;
            padding: 0 2px;
            margin-bottom: -1px;
            border: 0;
            border-bottom: 2.5px solid transparent;
            background: transparent;
            color: var(--color-text-muted);
            font-size: 13px;
            font-weight: 500;
            cursor: pointer;
            transition: all 0.2s;

            .tab-label {
              line-height: 1;
            }

            .tab-badge {
              font-family:
                ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
              font-size: 10px;
              line-height: 1.2;
              padding: 1px 6px;
              border-radius: 4px;
              background: var(--color-panel-soft);
              border: 1px solid var(--color-line);
              color: var(--color-text-soft);
              font-weight: 500;
              transition: all 0.2s;
            }

            &:hover {
              color: var(--color-text);
            }

            &--active {
              color: var(--color-primary);
              font-weight: 600;
              border-bottom-color: var(--color-primary);

              .tab-badge {
                background: var(--color-primary-soft);
                border-color: var(--color-info-line);
                color: var(--color-primary);
                font-weight: 600;
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
        margin: 12px 12px 8px;
        padding: 6px 10px;
        border-radius: 7px;
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
            font-family: inherit;
            font-size: 12px;
          }
        }
      }

      .cyber-conversations {
        min-height: 0;
        flex: 1;
        padding: 4px 10px 10px;
        overflow-y: auto;
        scrollbar-width: thin;
        scrollbar-color: var(--color-line) transparent;

        .cyber-device-card {
          display: flex;
          width: 100%;
          box-sizing: border-box;
          align-items: center;
          gap: 6px;
          margin-bottom: 6px;
          padding: 8px 10px;
          border-radius: 8px;
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          transition: all 0.2s;

          &:hover {
            border-color: var(--color-line-strong);
            background: var(--color-panel-soft);
          }

          &--active {
            border-color: var(--color-primary);
            background: #f8fafc;
            box-shadow: 0 1px 4px rgba(37, 99, 235, 0.06);
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
              width: 38px;
              height: 38px;
              flex: none;
              place-items: center;
              border-radius: 8px;
              background: var(--color-panel-soft);
              border: 1px solid var(--color-line);
              color: var(--color-text-muted);

              &.is-online {
                color: #10b981;
                border-color: #86efac;
                background: #dcfce7;
              }

              &--group {
                color: var(--color-primary);
                border-color: var(--color-info-line);
                background: var(--color-primary-soft);
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
                  font-size: 9.5px;
                  padding: 1px 5px;
                  border-radius: 3px;
                  font-weight: 600;

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

                  &.tag-group {
                    background: var(--color-primary-soft);
                    border: 1px solid var(--color-info-line);
                    color: var(--color-primary);
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
                font-size: 10.5px;

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
        background: var(--color-panel);
        display: flex;
        flex-direction: column;
        gap: 5px;

        .telemetry-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 10px;

          .telemetry-label {
            color: var(--color-text-soft);
            font-weight: 500;
          }

          .telemetry-val {
            color: var(--color-text);
            display: inline-flex;
            align-items: center;
            gap: 5px;

            .telemetry-icon {
              color: var(--color-text-muted);
              flex-shrink: 0;
            }
          }
        }
      }
    }
  }
}

/* 弹窗通用样式 */
.drop-settings-modal.base-modal {
  :deep(.base-modal__panel) {
    width: 460px;
    max-width: calc(100vw - 48px);
  }

  .drop-settings-content {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-top: 4px;
    color: var(--color-text);

    .drop-settings-status {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 11px 12px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);

      .drop-settings-status-dot {
        width: 8px;
        height: 8px;
        flex: none;
        border-radius: 50%;
        background: var(--color-text-soft);
      }

      &.is-online {
        border-color: var(--color-success-line);
        background: var(--color-success-soft);

        .drop-settings-status-dot {
          background: var(--color-success);
          box-shadow: 0 0 0 3px var(--color-success-soft);
        }
      }

      .drop-settings-status-copy {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 3px;

        span { font-size: 13px; }
        small { color: var(--color-text-muted); font-size: 11.5px; }
      }
    }

    .drop-settings-list {
      display: flex;
      flex-direction: column;
      gap: 7px;
    }

    .drop-settings-item {
      display: flex;
      width: 100%;
      align-items: center;
      gap: 10px;
      padding: 10px 11px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);
      text-align: left;
      cursor: pointer;

      &:hover:not(:disabled) {
        border-color: var(--color-primary);
        background: var(--color-primary-soft);
      }

      &:disabled { cursor: wait; opacity: 0.6; }

      .drop-settings-item-icon {
        display: grid;
        width: 30px;
        height: 30px;
        flex: none;
        place-items: center;
        border-radius: 7px;
        background: var(--color-primary-soft);
        color: var(--color-primary);
      }

      .drop-settings-item-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 3px;

        span { font-size: 12.5px; }
        small {
          overflow: hidden;
          color: var(--color-text-muted);
          font-size: 11px;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }

      .drop-settings-item-arrow {
        flex: none;
        color: var(--color-text-soft);
      }
    }

    .drop-settings-addresses {
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding-top: 2px;

      .drop-settings-section-title {
        display: flex;
        align-items: center;
        gap: 6px;
        color: var(--color-text-muted);
        font-size: 11.5px;
      }

      .drop-settings-address-list {
        display: flex;
        flex-wrap: wrap;
        gap: 7px;

        .drop-settings-address {
          display: inline-flex;
          align-items: center;
          gap: 8px;
          padding: 7px 9px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
          color: var(--color-text);
          font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11px;
          cursor: pointer;

          span:last-child { color: var(--color-text-muted); font-family: inherit; }
          &:hover:not(:disabled), &.is-active {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            color: var(--color-primary);
          }
          &:disabled { cursor: default; opacity: 0.72; }
        }

        .drop-settings-empty { color: var(--color-text-soft); font-size: 11.5px; }
      }
    }

    .drop-settings-offline {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 7px;
      min-height: 34px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      cursor: pointer;

      &:hover {
        border-color: var(--color-danger-line, var(--color-line-strong));
        color: var(--color-danger);
      }
    }
  }
}

.drop-transfer-modal.base-modal {
  .drop-transfer-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 6px;
    color: var(--color-text);

    .drop-transfer-hint {
      margin: 0;
      color: var(--color-text-muted);
      font-size: 12.5px;
      line-height: 1.6;
    }

    .drop-transfer-field {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
    }

    .drop-transfer-actions {
      display: flex;
      justify-content: flex-end;
      gap: 10px;
    }
  }
}

.drop-delete-device-modal {
  .drop-delete-warning {
    color: var(--color-danger);
    font-size: var(--font-size-xs);
    line-height: 1.6;
  }
}

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

    .drop-connect-list {
      display: flex;
      max-height: 320px;
      flex-direction: column;
      gap: 6px;
      overflow-y: auto;

      .drop-connect-device {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 10px 12px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text);
        text-align: left;
        cursor: pointer;

        &:hover:not(:disabled) {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
        }

        &:disabled {
          cursor: default;
          color: var(--color-text-muted);
        }

        .drop-connect-device-info {
          display: flex;
          min-width: 0;
          flex: 1;
          flex-direction: column;
          gap: 4px;

          .drop-connect-device-name {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }

          .drop-connect-device-ip {
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
          }
        }

        .drop-connect-device-status {
          flex: none;
          color: var(--color-primary);
          font-size: var(--font-size-sm);
        }
      }

      .drop-connect-empty {
        padding: 24px 12px;
        color: var(--color-text-muted);
        text-align: center;
      }
    }

    .drop-connect-error {
      margin: 0;
      color: var(--color-danger);
      font-size: var(--font-size-sm);
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

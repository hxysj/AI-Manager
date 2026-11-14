<template>
  <section
    ref="panelElement"
    v-if="ready && layoutReady"
    class="quick-switch-panel"
    :style="centerLayoutStyle"
    :class="{
      'quick-switch-panel-expanded': expanded,
      'quick-switch-panel-dragging': drag.active
    }"
    @pointerenter="enterPanel"
    @pointerleave="leavePanel"
    @pointermove="handlePointerMove"
    @focusin="enterPanel"
    @focusout="handleFocusOut"
    @keydown.esc="closeDesktopChat"
    @dragstart.prevent
  >
    <template v-if="expanded && !desktopChatVisible">
      <svg
        v-if="orbitLayout === 'radial'"
        class="quick-switch-connections"
        viewBox="0 0 560 560"
        aria-hidden="true"
      >
        <line
          v-for="tool in orbitTools"
          :key="tool.id"
          :x1="centerPosition.x"
          :y1="centerPosition.y"
          :x2="tool.x"
          :y2="tool.y"
        />
      </svg>
      <div
        v-if="orbitLayout === 'radial'"
        class="quick-switch-orbit-ring"
        aria-hidden="true"
      ></div>
      <nav
        class="quick-switch-tools"
        :class="{ 'quick-switch-tools-adaptive': toolTray }"
        :style="
          toolTray && {
            '--tray-left': `${toolTray.x}px`,
            '--tray-top': `${toolTray.y}px`,
            '--tray-width': `${toolTray.width}px`,
            '--tray-height': `${toolTray.height}px`
          }
        "
        aria-label="悬浮工具"
      >
        <button
          v-for="tool in orbitTools"
          :key="tool.id"
          class="quick-switch-tool"
          :class="`quick-switch-tool-${tool.theme}`"
          :style="{
            '--tool-left': `${tool.x}px`,
            '--tool-top': `${tool.y}px`,
            '--tool-order': tool.order
          }"
          type="button"
          :title="tool.summary"
          @click="openToolWindow(tool.id)"
        >
          <span class="quick-switch-tool-icon">
            <component :is="tool.icon" :size="24" :stroke-width="1.7" />
          </span>
          <span class="quick-switch-tool-name">{{ tool.label }}</span>
        </button>
      </nav>
    </template>

    <article
      class="quick-switch-center"
      :class="{ 'quick-switch-center-pet': desktopPet }"
      @pointerdown="startDrag"
      @click="handleMainClick"
      @contextmenu.prevent="openAssistant"
    >
      <button
        v-if="desktopPet"
        class="quick-switch-pet-button"
        :class="{
          'quick-switch-pet-mirrored':
            screenEdge === 'right' && petAction === 'peek'
        }"
        type="button"
        :aria-label="`${desktopPet.name} · 点击打开主界面，拖动移动悬浮窗`"
      >
        <DesktopPet
          :pet="desktopPet"
          :dragging="drag.active"
          :action="petAction"
          :paused="!windowVisible"
        />
      </button>
      <template v-else>
        <button
          class="quick-switch-main"
          type="button"
          aria-label="点击打开主界面，拖动移动悬浮窗"
        >
          <span
            class="quick-switch-avatar"
            :class="{ 'quick-switch-avatar-home': !activeCli }"
          >
            <AiIcon
              v-if="activeCli"
              :name="activeCli.id"
              :alt="activeCli.name"
              class-name="quick-switch-cli-icon"
            />
            <img
              v-else
              class="quick-switch-logo"
              :src="logoUrl"
              alt="AI Manager"
            />
          </span>
          <span class="quick-switch-name">{{
            activeCli?.name || "AI Manager"
          }}</span>
          <span class="quick-switch-provider">{{ activeName }}</span>
          <span v-if="activeCli" class="quick-switch-model">{{
            activeModel
          }}</span>
          <span v-else class="quick-switch-model">{{
            visibleTools.length ? "悬浮工具 · 拖动移动" : "拖动移动 · 点击打开"
          }}</span>
        </button>
        <div
          class="quick-switch-usage"
          :aria-label="usageError || '今日用量'"
          aria-live="polite"
        >
          <span class="quick-switch-usage-item">
            <span class="quick-switch-usage-label">今日 Token</span>
            <span class="quick-switch-usage-value">{{
              usageSummary ? tokenCount.compact : "—"
            }}</span>
          </span>
          <span class="quick-switch-usage-item">
            <span class="quick-switch-usage-label">费用</span>
            <span class="quick-switch-usage-value">{{
              usageSummary ? formatCost(usageSummary.totalCostUsd) : "—"
            }}</span>
          </span>
        </div>
        <nav
          class="quick-switch-pages"
          aria-label="中心节点内容"
          @pointerdown.stop
          @click.stop
        >
          <button
            v-for="page in centerPages"
            :key="page.id"
            class="quick-switch-page"
            :class="{ 'quick-switch-page-active': selectedPage === page.id }"
            type="button"
            :aria-label="page.name"
            :aria-pressed="selectedPage === page.id"
            @click="selectedPage = page.id"
          ></button>
        </nav>
      </template>
      <button
        v-if="lanShareUnreadCount"
        class="quick-switch-unread-badge"
        :class="{
          'quick-switch-unread-badge-left': screenEdge === 'left',
          'quick-switch-unread-badge-right': screenEdge === 'right'
        }"
        type="button"
        :title="`设备快传有 ${lanShareUnreadCount} 条未读消息`"
        @pointerdown.stop
        @click.stop="openLanShare"
      >
        {{ displayUnreadCount }}
      </button>
    </article>
    <aside
      v-if="expanded && desktopChatVisible"
      class="quick-switch-chat"
      :style="desktopChatLayout.style"
      :data-side="desktopChatLayout.side"
      aria-label="桌宠对话"
      @pointerenter="pauseDesktopReply"
      @pointerleave="resumeDesktopReply"
      @focusin="pauseDesktopReply"
      @focusout="resumeDesktopReply"
      @pointerdown.stop
      @click.stop
    >
      <div
        class="quick-switch-speech"
        :class="{
          'quick-switch-speech-thinking': desktopThinking
        }"
        :aria-label="desktopThinking ? '桌宠正在思考' : '桌宠说话'"
        aria-live="polite"
      >
        <span v-if="desktopThinking" class="quick-switch-thought">
          {{ desktopThoughtLabel }}
          <span class="quick-switch-thinking-dots" aria-hidden="true">
            <i class="quick-switch-thinking-dot"></i>
            <i class="quick-switch-thinking-dot"></i>
            <i class="quick-switch-thinking-dot"></i>
          </span>
        </span>
        <MarkdownContent
          v-if="desktopReply"
          ref="desktopReplyElement"
          class="quick-switch-speech-text"
          :class="{ 'quick-switch-speech-text-streaming': desktopBusy }"
          :text="desktopReply"
        />
        <span
          v-if="desktopChatError"
          class="quick-switch-speech-error-text"
          role="alert"
        >
          {{ desktopChatError }}
        </span>
        <div class="quick-switch-speech-actions">
          <button
            v-if="desktopBusy"
            class="quick-switch-speech-button"
            type="button"
            aria-label="停止回复"
            title="停止回复"
            :disabled="!desktopRunId || assistantState.status === 'cancelling'"
            @click="stopDesktopChat"
          >
            <Square :size="11" />
          </button>
          <button
            class="quick-switch-speech-button"
            type="button"
            aria-label="关闭桌宠对话"
            title="收起气泡"
            @click="closeDesktopChat"
          >
            <X :size="13" />
          </button>
        </div>
      </div>
    </aside>
  </section>
</template>

<script setup>
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch
} from "vue"
import { Square, X } from "lucide-vue-next"
import { appApi, assistantApi, lanShareApi, usageApi } from "@/api"
import AiIcon from "@/components/AiIcon.vue"
import DesktopPet from "@/components/DesktopPet.vue"
import MarkdownContent from "@/components/MarkdownContent.vue"
import { getDesktopPet } from "@/constants/desktop-pets"
import logoUrl from "@/assets/ai-manager-logo.png?url"
import { getToolItems } from "@/features/tools/constant"
import { formatTokenCountParts } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  state: { type: Object, required: true },
  ready: { type: Boolean, default: true }
})
const expanded = ref(false)
const panelElement = ref(null)
const layoutReady = ref(false)
const windowVisible = ref(false)
const screenEdge = ref("")
const hovered = ref(false)
const selectedPage = ref("home")
const usageSummary = ref(null)
const usageError = ref("")
const lanShareUnreadCount = ref(0)
let unsubscribeLanShare = null
const assistantState = ref({})
let unsubscribeAssistant
const assistantEnabled = computed(
  () => props.state.appSettings?.agents?.assistant?.enabled !== false
)
const desktopReplyElement = ref(null)
const desktopRunId = ref("")
const desktopReply = ref("")
const desktopChatError = ref("")
const chatWaiting = ref(false)
let desktopReplyTimer = null
let assistantStateVersion = 0
let desktopLastRunId = ""
let desktopReplyPaused = false
let desktopTimerVersion = 0
const desktopBusy = computed(() => chatWaiting.value)
const desktopThinking = computed(
  () => desktopBusy.value && !desktopReply.value && !desktopChatError.value
)
const desktopThoughtLabel = computed(
  () =>
    ({
      working: "正在处理",
      cancelling: "正在停止"
    })[assistantState.value.status] || "思考中"
)
const desktopChatVisible = computed(() =>
  Boolean(
    desktopPet.value &&
    assistantEnabled.value &&
    (desktopReply.value || desktopChatError.value || desktopBusy.value)
  )
)
// 对话窗口独立保留，主面板切换工具时仍能继续交流。
function openAssistant() {
  appApi.openToolWindow({ toolId: "assistant" }).catch((error) => {
    createMessage.error(error.message || String(error))
  })
}

// 侧边气泡抬到头部斜上方；顶部空间不足时留在屏幕内，尾巴仍指向头部。
const desktopChatLayout = computed(() => {
  const { x, y } = centerPosition.value
  // 气泡宽度按内容收缩，最大宽度受桌宠所在一侧的可用空间限制。
  let width = 440
  let left = Math.max(12, Math.min(548 - width, x - width / 2))
  let top
  let bottom
  let height = 280
  let side
  if (548 - x - 68 >= 320) {
    width = Math.min(width, 548 - x - 68)
    side = "right"
    left = x + 68
  } else if (x - 68 - 12 >= 320) {
    width = Math.min(width, x - 68 - 12)
    side = "left"
    left = x - 68 - width
  } else if (y >= 280) {
    side = "top"
    height = Math.min(height, y - 80)
    bottom = 560 - y + 68
  } else {
    side = "bottom"
    height = Math.min(height, 480 - y)
    top = y + 68
  }
  if (side === "left" || side === "right") {
    if (y >= 104) {
      bottom = 560 - y + 32
      height = Math.min(height, y - 44)
    } else top = 12
  }
  return {
    side,
    style: {
      left: `${left}px`,
      top: top === undefined ? undefined : `${top}px`,
      bottom: bottom === undefined ? undefined : `${bottom}px`,
      width: `${width}px`,
      maxHeight: `${height}px`,
      "--chat-height": `${height}px`,
      "--speech-tail-y":
        y >= 104 ? "calc(100% - 14px)" : `${Math.max(18, y - 40)}px`
    }
  }
})

let desktopReplyPointer = false
let desktopReplyFocused = false
function pauseDesktopReply(event) {
  if (event?.type === "focusin") desktopReplyFocused = true
  else desktopReplyPointer = true
  desktopReplyPaused = true
  desktopTimerVersion += 1
  clearTimeout(desktopReplyTimer)
  desktopReplyTimer = null
}

function resumeDesktopReply(event) {
  if (
    event?.type === "focusout" &&
    event.currentTarget?.contains(event.relatedTarget)
  )
    return
  if (event?.type === "focusout") desktopReplyFocused = false
  else desktopReplyPointer = false
  desktopReplyPaused = desktopReplyPointer || desktopReplyFocused
  if (!desktopReplyPaused && !desktopBusy.value && desktopChatVisible.value)
    startDesktopReplyTimer()
}

// 计时只收起气泡；移出后重新计满十秒，过期回调不能清除新回复。
function startDesktopReplyTimer(delay = 10000) {
  const version = ++desktopTimerVersion
  clearTimeout(desktopReplyTimer)
  if (desktopReplyPaused) return
  const runId = desktopRunId.value
  desktopReplyTimer = window.setTimeout(() => {
    if (
      disposed ||
      version !== desktopTimerVersion ||
      desktopRunId.value !== runId
    )
      return
    desktopReplyTimer = null
    desktopReply.value = ""
    desktopChatError.value = ""
    desktopRunId.value = ""
    if (!hovered.value) setExpanded(false)
  }, delay)
}

function closeDesktopChat() {
  clearTimeout(hoverTimer)
  hoverTimer = null
  clearTimeout(desktopReplyTimer)
  desktopReplyTimer = null
  desktopTimerVersion += 1
  desktopReplyPaused = false
  desktopReplyPointer = false
  desktopReplyFocused = false
  desktopReply.value = ""
  desktopChatError.value = ""
  desktopRunId.value = ""
  chatWaiting.value = false
  setExpanded(false)
}

function applyDesktopTask(snapshot) {
  desktopRunId.value = snapshot.runId
  desktopLastRunId = snapshot.runId
  desktopReply.value = snapshot.text || ""
  const waiting = ["thinking", "working", "replying", "cancelling"].includes(
    snapshot.status
  )
  // 只在本轮结束时计时，配置同步等重复快照不会延长回复展示。
  const finished = chatWaiting.value && !waiting
  chatWaiting.value = waiting
  if (!finished) return
  desktopChatError.value =
    snapshot.error || (snapshot.status === "cancelled" ? "已停止回复" : "")
  startDesktopReplyTimer(
    snapshot.finishedAt
      ? Math.max(0, snapshot.finishedAt + 10000 - Date.now())
      : 10000
  )
}

function applyAssistantState(snapshot) {
  assistantState.value = snapshot
  const current = desktopRunId.value && snapshot.runId === desktopRunId.value
  const fresh =
    snapshot.surface === "desktop" &&
    snapshot.replyAtPet &&
    snapshot.runId !== desktopLastRunId &&
    windowVisible.value &&
    (["thinking", "working", "replying", "cancelling"].includes(
      snapshot.status
    ) ||
      snapshot.finishedAt > Date.now() - 10000)
  if (!assistantEnabled.value || !desktopPet.value || (!current && !fresh))
    return
  if (!current) {
    clearTimeout(desktopReplyTimer)
    desktopReplyTimer = null
    desktopTimerVersion += 1
    desktopReply.value = ""
    desktopChatError.value = ""
    chatWaiting.value = true
  }
  applyDesktopTask(snapshot)
  if (desktopChatVisible.value) setExpanded(true)
}

async function stopDesktopChat() {
  if (!desktopRunId.value) return
  try {
    await assistantApi.cancel(desktopRunId.value)
  } catch (error) {
    desktopChatError.value = error.message || String(error)
  }
}

// 统一使用后端快照，避免主窗口和悬浮窗各自累加产生偏差。
function applyLanShareState(payload) {
  const data = payload?.service ? payload : payload?.data || {}
  lanShareUnreadCount.value = Number(data.totalUnread || 0)
}

// 气泡点击直接打开设备快传工具窗口。
function openLanShare() {
  appApi.showMainPanel({ toolId: "lan-share" }).catch((error) => {
    createMessage.error(error.message || String(error))
  })
}

const displayUnreadCount = computed(() =>
  lanShareUnreadCount.value > 99 ? "99+" : lanShareUnreadCount.value
)
const desktopPet = computed(() =>
  props.state.appSettings?.desktop?.enabled
    ? getDesktopPet(props.state.appSettings.desktop.petId)
    : null
)
const cliTargets = computed(() =>
  (props.state.cliTargets || []).filter(
    (cli) => props.state.runtimeConfigSchemas?.[cli.id]?.enabled
  )
)
const centerPages = computed(() => [
  { id: "home", name: "AI Manager" },
  ...cliTargets.value
])
const activeCli = computed(
  () => cliTargets.value.find((cli) => cli.id === selectedPage.value) || null
)
const activeProfile = computed(() =>
  (props.state.runtimeProfiles || []).find(
    (profile) => profile.cli === activeCli.value?.id
  )
)
const activeProxy = computed(() =>
  activeCli.value?.id === "claude"
    ? props.state.claudeProxyState
    : activeCli.value?.id === "codex"
      ? props.state.codexProxyState
      : null
)
const proxyTarget = computed(() =>
  activeProxy.value?.enabled ? activeProxy.value.activeProviderId || "" : ""
)
const activeProvider = computed(() =>
  (props.state.providers || []).find(
    (provider) =>
      provider.id === (proxyTarget.value || activeProfile.value?.providerId)
  )
)
const activeAccount = computed(() => {
  if (activeCli.value?.id !== "codex") return null
  return (props.state.codexAccounts || []).find((account) =>
    proxyTarget.value
      ? proxyTarget.value === `account:${account.id}`
      : account.active
  )
})
const activeName = computed(() => {
  if (!activeCli.value) return "你的桌面工具中心"
  const name =
    activeAccount.value?.email ||
    activeAccount.value?.accountId ||
    activeProvider.value?.name ||
    "未启用 Provider"
  return proxyTarget.value ? `Proxy · ${name}` : name
})
const activeModel = computed(() => {
  const providerModel =
    activeProvider.value?.runtimeConfig?.mainModel ||
    (props.state.runtimeModels || []).find(
      (model) => model.providerId === activeProvider.value?.id
    )?.name
  return (
    (proxyTarget.value
      ? providerModel
      : activeProfile.value?.model || providerModel) ||
    (activeAccount.value ? "官方账号" : "未选择模型")
  )
})
const visibleTools = computed(() =>
  getToolItems(props.state).filter(
    (tool) =>
      props.state.appSettings?.system?.quickSwitchTools?.[tool.id] !== false
  )
)
const centerPosition = ref({ x: 64, y: 64 })
// 偷看素材已包含墙壁遮挡，贴边只切换动作，不再把桌宠移出窗口。
const petAction = computed(() => {
  if (!desktopPet.value) return ""
  const action = drag.active
    ? "walk"
    : assistantState.value.petAction ||
      (["thinking", "replying"].includes(assistantState.value.status)
        ? "think"
        : "") ||
      (assistantState.value.status === "working" ? "work_laptop" : "") ||
      (hovered.value
        ? "salute"
        : screenEdge.value === "left" || screenEdge.value === "right"
          ? "peek"
          : "")
  return desktopPet.value.animations.some(
    (animation) => animation.id === action
  )
    ? action
    : ""
})
const centerLayoutStyle = computed(() => ({
  "--center-x": `${centerPosition.value.x}px`,
  "--center-y": `${centerPosition.value.y}px`,
  // 左右贴边时靠近边缘 20px，上下贴边保持原位。
  "--pet-offset-x": `${desktopPet.value ? (screenEdge.value === "left" ? -20 : screenEdge.value === "right" ? 20 : 0) : 0}px`
}))
// 边缘空间不足时改用可用网格，工具不再强行围绕中心节点。
const orbitLayout = computed(() => {
  const { x, y } = centerPosition.value
  return x >= 200 && x <= 360 && y >= 200 && y <= 360 ? "radial" : "adaptive"
})
const toolTray = computed(() => {
  if (orbitLayout.value === "radial" || !visibleTools.value.length) return null
  const { x, y } = centerPosition.value
  // 工具集中在空间较大的一侧，保持等距排列和固定阅读顺序。
  const regions = [
    {
      side: "right",
      x: x + 68,
      y: 12,
      width: 480 - x,
      height: 536,
      columns: 3
    },
    { side: "left", x: 12, y: 12, width: x - 80, height: 536, columns: 3 },
    {
      side: "bottom",
      x: 12,
      y: y + 68,
      width: 536,
      height: 480 - y,
      columns: 4
    },
    { side: "top", x: 12, y: 12, width: 536, height: y - 80, columns: 4 }
  ]
  const candidates = regions
    .map((region) => {
      const columns = Math.min(
        visibleTools.value.length,
        region.columns,
        Math.floor((region.width - 10) / 94)
      )
      const rows =
        columns > 0 ? Math.ceil(visibleTools.value.length / columns) : 0
      return {
        ...region,
        columns,
        trayWidth: columns * 94 + 10,
        trayHeight: rows * 80 + 10
      }
    })
    .filter(
      (region) => region.columns > 0 && region.trayHeight <= region.height
    )
  const region = candidates.sort(
    (a, b) => b.width * b.height - a.width * a.height
  )[0]
  const width = region.trayWidth
  const height = region.trayHeight
  let left = Math.max(12, Math.min(548 - width, x - width / 2))
  let top = Math.max(12, Math.min(548 - height, y - height / 2))
  if (region.side === "left") left = x - 68 - width
  if (region.side === "right") left = x + 68
  if (region.side === "top") top = y - 68 - height
  if (region.side === "bottom") top = y + 68
  return { x: left, y: top, width, height, columns: region.columns }
})
const orbitTools = computed(() => {
  const tools = visibleTools.value
  if (!tools.length) return []
  if (orbitLayout.value === "radial") {
    return tools.map((tool, index) => {
      const angle = -Math.PI / 2 + (index * Math.PI * 2) / tools.length
      return {
        ...tool,
        x: Math.min(
          515,
          Math.max(45, centerPosition.value.x + Math.cos(angle) * 205)
        ),
        y: Math.min(
          519,
          Math.max(41, centerPosition.value.y + Math.sin(angle) * 205)
        ),
        order: index
      }
    })
  }

  const tray = toolTray.value
  return tools.map((tool, index) => {
    const row = Math.floor(index / tray.columns)
    const rowCount = Math.min(tray.columns, tools.length - row * tray.columns)
    return {
      ...tool,
      x:
        tray.x +
        52 +
        (index % tray.columns) * 94 +
        (tray.columns - rowCount) * 47,
      y: tray.y + 45 + row * 80,
      order: index
    }
  })
})
const tokenCount = computed(() =>
  formatTokenCountParts(usageSummary.value?.actualTokens)
)
let hoverTimer = null
let resizing = false
let wantedExpanded = false
let disposed = false
let usageSequence = 0
let usageKey = ""
let moveRequest = Promise.resolve()
let unsubscribeVisibility = null
let visibilitySequence = 0
// 拖动期间锁住悬浮样式，避免移动时误触发工具效果。
const drag = reactive({ active: false, moved: false, x: 0, y: 0, distance: 0 })

function formatCost(value) {
  const cost = Number(value || 0)
  return `$${cost === 0 ? "0" : cost.toFixed(cost >= 1 ? 2 : 4)}`
}

// 原生窗口按中心扩缩；串行处理，避免快速进出时旧请求覆盖新状态。
async function setExpanded(value) {
  wantedExpanded =
    value && (visibleTools.value.length > 0 || desktopChatVisible.value)
  if (!windowVisible.value || drag.active || resizing || disposed) return
  resizing = true
  try {
    // 先完成拖动，避免迟到的坐标覆盖展开或收起后的中心位置。
    await moveRequest
    while (
      !disposed &&
      windowVisible.value &&
      !drag.active &&
      wantedExpanded !== expanded.value
    ) {
      const next = wantedExpanded
      const sequence = visibilitySequence
      const layout = await appApi.setQuickSwitchCollapsed({ collapsed: !next })
      if (!disposed && sequence === visibilitySequence) {
        expanded.value =
          typeof layout?.collapsed === "boolean" ? !layout.collapsed : next
        if (expanded.value !== next) wantedExpanded = expanded.value
        const fallback = expanded.value ? 280 : 64
        const centerX = Number(layout?.centerX)
        const centerY = Number(layout?.centerY)
        centerPosition.value = {
          x: Number.isFinite(centerX) ? centerX : fallback,
          y: Number.isFinite(centerY) ? centerY : fallback
        }
        screenEdge.value = layout?.edge || ""
      }
    }
  } catch (error) {
    wantedExpanded = expanded.value
    createMessage.error(error.message || String(error))
  } finally {
    resizing = false
  }
}

function enterPanel() {
  if (!windowVisible.value) return
  hovered.value = true
  clearTimeout(hoverTimer)
  if (drag.active) return
  hoverTimer = setTimeout(() => {
    hoverTimer = null
    setExpanded(true)
  }, 100)
}

// 原生窗口贴边移动后，鼠标可能仍在窗口内而没有新的进入事件。
function handlePointerMove() {
  if (drag.active || resizing) return
  if (!hovered.value || (!expanded.value && hoverTimer === null)) enterPanel()
}

function leavePanel() {
  hovered.value = false
  clearTimeout(hoverTimer)
  hoverTimer = null
  // 给鼠标从中心经过透明间隔到工具节点留出时间。
  if (windowVisible.value && !drag.active && !desktopChatVisible.value)
    hoverTimer = setTimeout(() => {
      hoverTimer = null
      setExpanded(false)
    }, 260)
}

function handleFocusOut(event) {
  if (!event.currentTarget.contains(event.relatedTarget)) leavePanel()
}

async function openToolWindow(toolId) {
  try {
    await appApi.openToolWindow({ toolId })
    await setExpanded(false)
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

async function showMainPanel() {
  try {
    await appApi.showMainPanel()
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

function handleMainClick(event) {
  if (!drag.moved || event.detail === 0) showMainPanel()
}

function startDrag(event) {
  if (!windowVisible.value || event.button !== 0) return
  event.preventDefault()
  clearTimeout(hoverTimer)
  hoverTimer = null
  wantedExpanded = expanded.value
  drag.active = true
  drag.moved = false
  drag.distance = 0
  drag.x = event.screenX
  drag.y = event.screenY
  event.currentTarget.setPointerCapture(event.pointerId)
  window.addEventListener("pointermove", moveDrag)
  window.addEventListener("pointerup", stopDrag)
  window.addEventListener("pointercancel", stopDrag)
}

function moveDrag(event) {
  const x = event.screenX - drag.x
  const y = event.screenY - drag.y
  drag.x = event.screenX
  drag.y = event.screenY
  drag.distance += Math.abs(x) + Math.abs(y)
  if (drag.distance > 3) drag.moved = true
  if (drag.moved && (x || y)) {
    const sequence = visibilitySequence
    // 串行同步实际中心位置，贴边时让工具自动移到剩余空间。
    moveRequest = moveRequest
      .then(async () => {
        if (disposed || sequence !== visibilitySequence) return
        const layout = await appApi.moveQuickSwitchBy({ x, y })
        if (
          !disposed &&
          sequence === visibilitySequence &&
          Number.isFinite(layout?.centerX) &&
          Number.isFinite(layout?.centerY)
        ) {
          centerPosition.value = { x: layout.centerX, y: layout.centerY }
          screenEdge.value = layout.edge || ""
        }
      })
      .catch((error) => createMessage.error(error.message || String(error)))
    return moveRequest
  }
}

function stopDrag(event) {
  const wasActive = drag.active
  drag.active = false
  window.removeEventListener("pointermove", moveDrag)
  window.removeEventListener("pointerup", stopDrag)
  window.removeEventListener("pointercancel", stopDrag)
  if (wasActive && !disposed) {
    // 等待最后一次移动完成后再保存，避免保存到拖动前的位置。
    moveRequest = moveRequest
      .then(() => appApi.saveQuickSwitchPosition())
      .catch((error) => createMessage.error(error.message || String(error)))
  }
  if (wasActive && !disposed && windowVisible.value) {
    // 松开时使用实际指针位置，修正拖动和窗口贴边造成的过期悬浮状态。
    if (Number.isFinite(event?.clientX) && Number.isFinite(event?.clientY)) {
      const bounds = panelElement.value.getBoundingClientRect()
      hovered.value =
        event.clientX >= bounds.left &&
        event.clientX < bounds.right &&
        event.clientY >= bounds.top &&
        event.clientY < bounds.bottom
    }
    hovered.value ? enterPanel() : leavePanel()
  }
}

// 复用窗口时清掉上次悬浮和拖动状态，显示位置由原生窗口一次性同步。
function handleVisibilityChange(layout) {
  visibilitySequence += 1
  closeDesktopChat()
  // 先停止交互，再恢复可见性，避免拖动结束时重新安排悬浮计时。
  windowVisible.value = false
  clearTimeout(hoverTimer)
  hoverTimer = null
  hovered.value = false
  stopDrag()
  drag.moved = false
  wantedExpanded = false
  expanded.value = false
  if (layout.visible) {
    centerPosition.value = { x: layout.centerX, y: layout.centerY }
    screenEdge.value = layout.edge || ""
    lanShareApi
      .getState()
      .then(applyLanShareState)
      .catch(() => {})
  }
  windowVisible.value = layout.visible
}

// 每五秒轮换中心内容，查看或拖动时暂停，手动切换仍保留。
function rotateCenterPage() {
  if (
    !props.ready ||
    !windowVisible.value ||
    desktopPet.value ||
    hovered.value ||
    drag.active ||
    disposed ||
    document.hidden
  )
    return
  const pages = centerPages.value
  const index = pages.findIndex((page) => page.id === selectedPage.value)
  selectedPage.value = pages[(index + 1) % pages.length].id
}

// 主状态仅带最近 200 条记录，今日汇总复用统计接口，避免漏算。
async function loadTodayUsage() {
  if (!props.ready || !windowVisible.value || desktopPet.value || disposed)
    return
  const sequence = ++usageSequence
  const start = new Date()
  start.setHours(0, 0, 0, 0)
  const end = new Date(start)
  end.setDate(end.getDate() + 1)
  const nextKey = `${activeCli.value?.id || "all"}:${start.getTime()}`
  if (usageKey !== nextKey) usageSummary.value = null
  usageKey = nextKey
  usageError.value = ""
  try {
    const result = await usageApi.getUsageStats({
      appType: activeCli.value?.id || "all",
      startAt: start.getTime(),
      endAt: end.getTime() - 1,
      logPageSize: 1
    })
    if (!disposed && sequence === usageSequence)
      usageSummary.value = result.data.summary
  } catch (error) {
    if (!disposed && sequence === usageSequence)
      usageError.value = `今日用量加载失败：${error.message || error}`
  }
}

watch(cliTargets, () => {
  if (!centerPages.value.some((page) => page.id === selectedPage.value))
    selectedPage.value = "home"
})
watch(visibleTools, () => {
  if (!visibleTools.value.length && !desktopChatVisible.value)
    setExpanded(false)
})
watch([assistantEnabled, desktopPet], () => {
  if (!assistantEnabled.value || !desktopPet.value) closeDesktopChat()
})
watch(windowVisible, async (visible) => {
  if (!visible) return
  const version = assistantStateVersion
  try {
    const snapshot = await assistantApi.getState()
    if (!disposed && version === assistantStateVersion && windowVisible.value)
      applyAssistantState(snapshot)
  } catch {
    // 预热中的窗口等待后续助手事件，不阻塞桌宠。
  }
})
watch(desktopReply, () =>
  nextTick(() => {
    if (desktopReplyElement.value)
      desktopReplyElement.value.$el.scrollTop =
        desktopReplyElement.value.$el.scrollHeight
  })
)
// 初始化同步保存的屏幕位置和贴边状态，配置和布局就绪后才显示主节点。
watch(
  () => props.ready,
  async (ready) => {
    if (!ready) {
      layoutReady.value = false
      return
    }
    try {
      if (!unsubscribeVisibility) {
        const unsubscribe = await appApi.onQuickSwitchVisibilityChanged(
          handleVisibilityChange
        )
        if (disposed) {
          unsubscribe()
          return
        }
        unsubscribeVisibility = unsubscribe
      }
      // 配置和 DOM 就绪不代表素材已加载；提前缓存动作，避免首次显示或敬礼切换时空白。
      if (desktopPet.value) {
        await Promise.all(
          desktopPet.value.animations.map((animation) => {
            return new Promise((resolve, reject) => {
              const image = new Image()
              image.onload = resolve
              image.onerror = () =>
                reject(new Error(`桌宠动作素材加载失败：${animation.name}`))
              image.src = animation.url
            })
          })
        )
        if (disposed || !props.ready) return
      }
      const layout = await appApi.setQuickSwitchCollapsed({
        collapsed: !expanded.value
      })
      if (disposed || !props.ready) return
      if (Number.isFinite(layout?.centerX) && Number.isFinite(layout?.centerY))
        centerPosition.value = { x: layout.centerX, y: layout.centerY }
      screenEdge.value = layout?.edge || ""
      layoutReady.value = true
      // 先提交首帧节点，再允许原生窗口显示，首次切换也不会闪出错误布局。
      await nextTick()
      if (!disposed && props.ready) {
        const sequence = visibilitySequence
        const status = await appApi.markQuickSwitchReady()
        // 已显示窗口重载时没有新显示事件；较新的可见性事件仍优先。
        if (!disposed && props.ready && sequence === visibilitySequence)
          windowVisible.value = status.visible
      }
    } catch (error) {
      if (!disposed) createMessage.error(error.message || String(error))
    }
  },
  { immediate: true }
)
watch(
  () => [
    props.ready,
    windowVisible.value,
    desktopPet.value,
    activeCli.value?.id,
    props.state.usage
  ],
  loadTodayUsage,
  {
    immediate: true
  }
)
// 跨日和后台更新时刷新今日统计，不额外触发全量扫描。
const usageTimer = setInterval(loadTodayUsage, 60000)
const pageTimer = setInterval(rotateCenterPage, 5000)

onBeforeUnmount(() => {
  disposed = true
  usageSequence += 1
  clearTimeout(hoverTimer)
  clearTimeout(desktopReplyTimer)
  clearInterval(usageTimer)
  clearInterval(pageTimer)
  unsubscribeVisibility?.()
  unsubscribeLanShare?.()
  unsubscribeAssistant?.()
  stopDrag()
})

onMounted(async () => {
  // 悬浮窗单独加载设备快传状态，主窗口隐藏时也能及时更新气泡。
  unsubscribeLanShare = lanShareApi.onStateChanged(applyLanShareState)
  lanShareApi
    .getState()
    .then(applyLanShareState)
    .catch(() => {})
  try {
    let updated = false
    const unsubscribe = await assistantApi.onEvent((event) => {
      if (disposed) return
      if (event.type === "state") {
        updated = true
        assistantStateVersion += 1
        applyAssistantState(event.state)
      } else if (
        event.type === "text-delta" &&
        event.runId === desktopRunId.value &&
        chatWaiting.value
      ) {
        desktopReply.value += event.text || ""
      }
    })
    if (disposed) {
      unsubscribe()
      return
    }
    unsubscribeAssistant = unsubscribe
    const snapshot = await assistantApi.getState()
    if (!disposed && !updated) applyAssistantState(snapshot)
  } catch {
    // 助手尚未启动时不干扰悬浮窗既有功能。
  }
})
</script>

<style scoped lang="less">
.quick-switch-panel {
  position: relative;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  color: var(--color-text);
  user-select: none;

  .quick-switch-chat {
    position: absolute;
    z-index: 2;
    display: flex;
    flex-direction: column;
    align-items: center;
    font-size: var(--font-size-sm);
    user-select: text;

    .quick-switch-speech {
      position: relative;
      display: flex;
      flex-direction: column;
      gap: 6px;
      box-sizing: border-box;
      width: max-content;
      min-width: 200px;
      max-width: 100%;
      min-height: 60px;
      max-height: var(--chat-height);
      padding: 12px 16px;
      border: 1px solid var(--color-line);
      border-radius: 16px;
      background: var(--color-panel);
      box-shadow: 0 5px 18px rgba(29, 71, 129, 0.12);
      line-height: 1.8;
      animation: quick-switch-speech-enter 180ms ease-out both;

      &::before,
      &::after {
        position: absolute;
        box-sizing: border-box;
        background: var(--color-panel);
        pointer-events: none;
      }

      &::before {
        width: 14px;
        height: 14px;
        transform: rotate(45deg);
        content: "";
      }

      .quick-switch-speech-text {
        min-height: 0;
        overflow: auto;
        overflow-wrap: anywhere;
        white-space: normal;
        scrollbar-width: thin;

        &.quick-switch-speech-text-streaming::after {
          display: inline-block;
          width: 3px;
          height: 13px;
          margin-left: 4px;
          border-radius: 2px;
          background: var(--color-primary);
          vertical-align: -1px;
          animation: quick-switch-thinking 1.2s ease-in-out infinite;
          content: "";
        }
      }

      .quick-switch-speech-error-text {
        overflow: auto;
        color: var(--color-danger);
        overflow-wrap: anywhere;
      }

      .quick-switch-thought {
        display: flex;
        align-items: center;
        gap: 4px;
        color: var(--color-text-muted);

        .quick-switch-thinking-dots {
          display: inline-flex;
          align-items: center;
          gap: 3px;

          .quick-switch-thinking-dot {
            width: 3px;
            height: 3px;
            border-radius: 50%;
            background: currentColor;
            animation: quick-switch-thinking 1.2s ease-in-out infinite;

            &:nth-child(2) {
              animation-delay: 150ms;
            }

            &:nth-child(3) {
              animation-delay: 300ms;
            }
          }
        }
      }

      .quick-switch-speech-actions {
        position: absolute;
        top: -10px;
        right: 8px;
        display: flex;
        gap: 4px;
        opacity: 0;
        pointer-events: none;
        transition: opacity 120ms;

        .quick-switch-speech-button {
          display: flex;
          width: 22px;
          height: 22px;
          align-items: center;
          justify-content: center;
          padding: 0;
          border: 1px solid var(--color-line);
          border-radius: 50%;
          background: var(--color-panel);
          color: var(--color-text-muted);
          cursor: pointer;

          &:hover,
          &:focus-visible {
            color: var(--color-primary);
          }

          &:disabled {
            opacity: 0.45;
            cursor: default;
          }
        }
      }

      &:hover,
      &:focus-within {
        .quick-switch-speech-actions {
          opacity: 1;
          pointer-events: auto;
        }
      }

      &.quick-switch-speech-thinking {
        border-radius: 30px;

        &::before,
        &::after {
          border: 1px solid var(--color-line);
          border-radius: 50%;
          transform: none;
          content: "";
        }

        &::before {
          width: 10px;
          height: 10px;
        }

        &::after {
          width: 5px;
          height: 5px;
        }
      }
    }

    &[data-side="right"] {
      align-items: flex-start;

      .quick-switch-speech {
        &::before {
          top: calc(var(--speech-tail-y) - 7px);
          left: -7px;
          border-bottom: 1px solid var(--color-line);
          border-left: 1px solid var(--color-line);
        }

        &.quick-switch-speech-thinking {
          &::before {
            top: calc(var(--speech-tail-y) - 2px);
            left: -15px;
          }

          &::after {
            top: calc(var(--speech-tail-y) + 9px);
            left: -25px;
          }
        }
      }
    }

    &[data-side="left"] {
      align-items: flex-end;

      .quick-switch-speech {
        &::before {
          top: calc(var(--speech-tail-y) - 7px);
          right: -7px;
          border-top: 1px solid var(--color-line);
          border-right: 1px solid var(--color-line);
        }

        &.quick-switch-speech-thinking {
          &::before {
            top: calc(var(--speech-tail-y) - 2px);
            right: -15px;
          }

          &::after {
            top: calc(var(--speech-tail-y) + 9px);
            right: -25px;
          }
        }
      }
    }

    &[data-side="top"] {
      .quick-switch-speech {
        &::before {
          bottom: -7px;
          left: calc(50% - 7px);
          border-right: 1px solid var(--color-line);
          border-bottom: 1px solid var(--color-line);
        }

        &.quick-switch-speech-thinking {
          &::before {
            bottom: -15px;
            left: calc(50% + 2px);
          }

          &::after {
            bottom: -25px;
            left: calc(50% - 7px);
          }
        }
      }
    }

    &[data-side="bottom"] {
      .quick-switch-speech {
        &::before {
          top: -7px;
          left: calc(50% - 7px);
          border-top: 1px solid var(--color-line);
          border-left: 1px solid var(--color-line);
        }

        &.quick-switch-speech-thinking {
          &::before {
            top: -15px;
            left: calc(50% + 2px);
          }

          &::after {
            top: -25px;
            left: calc(50% - 7px);
          }
        }
      }
    }
  }

  .quick-switch-connections {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    stroke: rgba(81, 148, 235, 0.14);
    stroke-width: 1;
  }

  .quick-switch-orbit-ring {
    position: absolute;
    top: var(--center-y);
    left: var(--center-x);
    width: 410px;
    height: 410px;
    border: 1px solid rgba(81, 148, 235, 0.22);
    border-radius: 50%;
    transform: translate(-50%, -50%);
    pointer-events: none;
  }

  .quick-switch-tools {
    position: absolute;
    inset: 0;
    .quick-switch-tool {
      --tool-color: #448bea;
      position: absolute;
      top: var(--tool-top);
      left: var(--tool-left);
      display: flex;
      width: 90px;
      height: 82px;
      align-items: center;
      justify-content: center;
      flex-direction: column;
      gap: 7px;
      padding: 8px 3px;
      border: 1px solid rgba(122, 164, 219, 0.38);
      border-radius: 21px;
      background: color-mix(in srgb, var(--color-panel) 96%, transparent);
      box-shadow:
        0 4px 16px rgba(29, 71, 129, 0.11),
        inset 0 1px 0 rgba(255, 255, 255, 0.65);
      color: var(--color-text);
      cursor: pointer;
      transform: translate(-50%, -50%);
      animation: quick-switch-tool-enter 280ms cubic-bezier(0.2, 0.72, 0.3, 1)
        both;
      animation-delay: calc(var(--tool-order) * 16ms);
      transition:
        transform 180ms ease,
        border-color 140ms,
        box-shadow 180ms ease;
      &.quick-switch-tool-green {
        --tool-color: #29876c;
      }
      &.quick-switch-tool-orange {
        --tool-color: #c08332;
      }
      &.quick-switch-tool-purple,
      &.quick-switch-tool-violet {
        --tool-color: #8d6bb7;
      }
      &.quick-switch-tool-rose {
        --tool-color: #c27382;
      }
      &:hover,
      &:focus-visible {
        transform: translate(-50%, -50%) scale(1.08);
        border-color: var(--tool-color);
        box-shadow: 0 5px 22px rgba(43, 103, 180, 0.2);
        outline: none;
      }
      .quick-switch-tool-icon {
        display: flex;
        width: 34px;
        height: 34px;
        align-items: center;
        justify-content: center;
        border-radius: 11px;
        background: color-mix(
          in srgb,
          var(--tool-color) 10%,
          var(--color-panel)
        );
        color: var(--tool-color);
        transition:
          transform 180ms ease,
          background-color 180ms ease;
      }
      &:hover .quick-switch-tool-icon,
      &:focus-visible .quick-switch-tool-icon {
        transform: translateY(-2px) scale(1.06);
      }
      .quick-switch-tool-name {
        font-size: 11px;
        line-height: 16px;
        white-space: nowrap;
      }
    }

    &.quick-switch-tools-adaptive {
      inset: auto;
      top: var(--tray-top);
      left: var(--tray-left);
      width: var(--tray-width);
      height: var(--tray-height);
      box-sizing: border-box;
      border: 1px solid rgba(122, 164, 219, 0.3);
      border-radius: 20px;
      background: color-mix(in srgb, var(--color-panel) 97%, transparent);
      box-shadow: 0 6px 20px rgba(29, 71, 129, 0.14);

      .quick-switch-tool {
        top: calc(var(--tool-top) - var(--tray-top));
        left: calc(var(--tool-left) - var(--tray-left));
        width: 84px;
        height: 70px;
        gap: 4px;
        padding: 4px 2px;
        border-color: transparent;
        border-radius: 14px;
        background: transparent;
        box-shadow: none;

        &:hover,
        &:focus-visible {
          border-color: transparent;
          background: color-mix(
            in srgb,
            var(--tool-color) 8%,
            var(--color-panel)
          );
          box-shadow: none;
        }

        .quick-switch-tool-icon {
          width: 28px;
          height: 28px;
          border-radius: 9px;
        }

        .quick-switch-tool-name {
          font-size: 10px;
        }
      }
    }
  }

  &.quick-switch-panel-dragging {
    .quick-switch-tools {
      pointer-events: none;

      .quick-switch-tool,
      .quick-switch-tool:hover,
      .quick-switch-tool:focus-visible {
        pointer-events: none;
        opacity: 1;
        transform: translate(-50%, -50%);
        border-color: rgba(122, 164, 219, 0.38);
        box-shadow:
          0 4px 16px rgba(29, 71, 129, 0.11),
          inset 0 1px 0 rgba(255, 255, 255, 0.65);
        animation: none;
        transition: none;
        outline: none;

        .quick-switch-tool-icon {
          transform: none;
          transition: none;
        }
      }

      &.quick-switch-tools-adaptive {
        .quick-switch-tool {
          border-color: transparent;
          background: transparent;
          box-shadow: none;
        }
      }
    }
  }

  .quick-switch-center {
    position: absolute;
    top: var(--center-y);
    left: var(--center-x);
    display: flex;
    width: 112px;
    height: 112px;
    box-sizing: border-box;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    padding: 5px 6px;
    border: 1px solid rgba(111, 160, 225, 0.62);
    border-radius: 24px;
    background: color-mix(in srgb, var(--color-panel) 96%, transparent);
    box-shadow:
      0 4px 10px rgba(29, 71, 129, 0.13),
      inset 0 0 0 4px rgba(108, 166, 244, 0.05);
    transform: translate(-50%, -50%);
    cursor: grab;
    touch-action: none;
    &:active {
      cursor: grabbing;
    }
    &.quick-switch-center-pet {
      width: 88px;
      height: 88px;
      padding: 0;
      border: 0;
      background: transparent;
      box-shadow: none;
      transform: translate(calc(-50% + var(--pet-offset-x)), -50%);
    }
    .quick-switch-pet-button {
      width: 100%;
      height: 100%;
      padding: 0;
      border: 0;
      border-radius: inherit;
      background: transparent;
      cursor: inherit;
      &.quick-switch-pet-mirrored {
        transform: scaleX(-1);
      }
      &:focus-visible {
        outline: 1px solid #448bea;
        outline-offset: -3px;
      }
    }
    .quick-switch-main {
      display: flex;
      width: 100%;
      min-width: 0;
      align-items: center;
      flex-direction: column;
      padding: 0;
      border: 0;
      background: transparent;
      color: inherit;
      cursor: inherit;
      text-align: center;
      &:focus-visible {
        outline: 1px solid #448bea;
        outline-offset: 3px;
        border-radius: 12px;
      }
      .quick-switch-avatar {
        display: flex;
        width: 20px;
        height: 20px;
        align-items: center;
        justify-content: center;
        margin-bottom: 1px;
        .quick-switch-logo {
          width: 24px;
          height: 24px;
          object-fit: contain;
        }
        :deep(.quick-switch-cli-icon) {
          width: 18px;
          height: 18px;
          object-fit: contain;
        }
      }
      .quick-switch-name {
        font-size: 10px;
        line-height: 13px;
        letter-spacing: 0.1px;
      }
      .quick-switch-provider,
      .quick-switch-model {
        max-width: 100%;
        overflow: hidden;
        color: var(--color-text-muted);
        font-size: 8px;
        line-height: 9px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .quick-switch-provider {
        color: var(--color-text-secondary);
      }
    }
    .quick-switch-unread-badge {
      position: absolute;
      top: -7px;
      right: -7px;
      min-width: 20px;
      height: 20px;
      box-sizing: border-box;
      padding: 0 5px;
      border: 2px solid var(--color-panel);
      border-radius: 10px;
      background: #ef4444;
      color: #ffffff;
      cursor: pointer;
      font-size: 10px;
      line-height: 16px;
      text-align: center;
      box-shadow: 0 2px 8px rgba(185, 28, 28, 0.28);
      &:hover,
      &:focus-visible {
        background: #dc2626;
        outline: none;
      }
      &.quick-switch-unread-badge-right {
        right: 1px;
        left: auto;
      }
      &.quick-switch-unread-badge-left {
        right: auto;
        left: 1px;
      }
    }
    .quick-switch-usage {
      display: flex;
      width: 100%;
      justify-content: center;
      margin-top: 2px;
      padding-top: 2px;
      border-top: 1px solid rgba(111, 160, 225, 0.2);
      .quick-switch-usage-item {
        display: flex;
        min-width: 0;
        flex: 1;
        align-items: center;
        flex-direction: column;
        gap: 0;
        .quick-switch-usage-label {
          color: var(--color-text-muted);
          font-size: 7px;
          line-height: 8px;
        }
        .quick-switch-usage-value {
          color: var(--color-text);
          font-size: 10px;
          line-height: 12px;
          font-variant-numeric: tabular-nums;
        }
      }
    }
    .quick-switch-pages {
      display: flex;
      align-items: center;
      justify-content: center;
      margin-top: 1px;
      .quick-switch-page {
        display: flex;
        width: 12px;
        height: 10px;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 0;
        background: transparent;
        cursor: pointer;
        &::after {
          width: 4px;
          height: 4px;
          border-radius: 50%;
          background: rgba(124, 146, 176, 0.35);
          content: "";
        }
        &.quick-switch-page-active::after {
          width: 8px;
          border-radius: 4px;
          background: #5495eb;
        }
        &:focus-visible {
          outline: 1px solid #448bea;
          border-radius: 4px;
        }
      }
    }
  }
}

@keyframes quick-switch-tool-enter {
  from {
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.72);
  }
  to {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
}

@keyframes quick-switch-speech-enter {
  from {
    opacity: 0;
    transform: translateY(4px) scale(0.97);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

@keyframes quick-switch-thinking {
  0%,
  70%,
  100% {
    opacity: 0.3;
  }
  35% {
    opacity: 1;
  }
}
</style>

<template>
  <section class="application-assistant" @paste="pasteImages">
    <header class="assistant-header">
      <div class="assistant-heading">
        <h1 class="assistant-title">应用助手</h1>
        <p class="assistant-description">
          管理应用配置、使用工具，与桌宠互动。
        </p>
      </div>
      <div class="assistant-header-actions">
        <span
          class="assistant-connection"
          :class="{
            'assistant-connection-ready':
              initialized && settings.enabled !== false
          }"
          role="status"
        >
          <span class="assistant-connection-dot"></span
          >{{
            loading
              ? "连接中"
              : settings.enabled === false
                ? "已关闭"
                : initialized
                  ? "已连接"
                  : "未连接"
          }}
        </span>
        <button
          v-if="standalone"
          class="assistant-icon-button"
          type="button"
          title="打开主面板"
          aria-label="打开主面板"
          @click="openMainPanel"
        >
          <Monitor :size="16" />
        </button>
        <button
          class="assistant-button"
          type="button"
          aria-label="历史对话"
          :aria-expanded="historyOpen"
          @click="historyOpen = true"
        >
          <History :size="16" />历史对话
        </button>
        <button
          class="assistant-button"
          type="button"
          :disabled="loading || settings.enabled === false"
          @click="newThread"
        >
          <Plus :size="16" />新建对话
        </button>
        <button
          class="assistant-icon-button"
          type="button"
          title="技能与工具"
          aria-label="技能与工具"
          @click="capabilitiesOpen = true"
        >
          <Wrench :size="16" />
        </button>
        <button
          v-if="!standalone"
          class="assistant-icon-button"
          type="button"
          title="独立窗口"
          aria-label="独立窗口"
          @click="openWindow"
        >
          <ExternalLink :size="16" />
        </button>
        <button
          class="assistant-icon-button"
          type="button"
          title="应用配置"
          aria-label="应用配置"
          @click="openSettings"
        >
          <Settings :size="16" />
        </button>
      </div>
    </header>

    <div class="assistant-layout">
      <div class="assistant-workspace">
        <div ref="messageList" class="assistant-messages" aria-live="polite">
          <button
            v-if="hasMoreMessages"
            class="assistant-button assistant-earlier"
            type="button"
            :disabled="historyLoading"
            @click="loadMessages(true)"
          >
            查看更早消息
          </button>
          <div
            v-if="!messages.length && !currentTask && !loading"
            class="assistant-welcome"
          >
            <span class="assistant-welcome-icon"
              ><Bot :size="32" :stroke-width="1.5"
            /></span>
            <h2 class="assistant-welcome-title">你好，有什么需要我帮忙的？</h2>
            <p class="assistant-welcome-description">
              查询应用配置、处理 JSON、打开工具，或让桌宠换个动作。
            </p>
            <div class="assistant-examples">
              <button
                v-for="example in examples"
                :key="example"
                class="assistant-example"
                type="button"
                @click="draft = example"
              >
                {{ example }}<ArrowUpRight :size="15" />
              </button>
            </div>
          </div>
          <article
            v-for="message in displayMessages"
            :key="message.id"
            class="assistant-message"
            :class="{ 'assistant-message-user': message.role === 'user' }"
          >
            <div class="assistant-message-main">
              <header class="assistant-message-header">
                <span class="assistant-message-label">{{
                  message.role === "user" ? "你" : "应用助手"
                }}</span>
                <span
                  v-if="
                    message.role !== 'user' && message.status !== 'completed'
                  "
                  class="assistant-message-status"
                  :class="{
                    'assistant-message-status-failed':
                      message.status === 'failed'
                  }"
                >
                  <LoaderCircle
                    v-if="message.pending"
                    class="assistant-loading-icon"
                    :size="13"
                  />
                  <CircleAlert v-else :size="13" />
                  {{ message.statusLabel }}
                </span>
                <time v-if="message.createdAt" class="assistant-message-date">{{
                  formatDateTime(message.createdAt)
                }}</time>
              </header>
              <div class="assistant-message-body">
                <AssistantImages :images="message.images || []" />
                <MarkdownContent
                  v-if="message.text && message.role !== 'user'"
                  class="assistant-message-text"
                  :text="message.text"
                />
                <div v-else-if="message.text" class="assistant-message-text">
                  {{ message.text }}
                </div>
                <details
                  v-for="tool in message.tools"
                  :key="tool.id"
                  class="assistant-tool"
                >
                  <summary class="assistant-tool-summary">
                    <Wrench :size="14" />{{ toolNames[tool.name] || tool.name
                    }}<span>{{
                      tool.status === "running"
                        ? "执行中"
                        : tool.status === "failed"
                          ? "失败"
                          : "已完成"
                    }}</span>
                  </summary>
                  <pre class="assistant-tool-detail">{{
                    tool.error || formatTool(tool.output ?? tool.input)
                  }}</pre>
                </details>
                <span
                  v-if="
                    message.pending && !message.text && !message.tools?.length
                  "
                  class="assistant-pending"
                  >正在思考…</span
                >
                <p v-if="message.error" class="assistant-error">
                  {{ message.error }}
                </p>
                <button
                  v-if="message.text"
                  class="assistant-icon-button assistant-message-copy"
                  :class="{
                    'assistant-message-copied': copiedMessageId === message.id
                  }"
                  type="button"
                  :title="
                    copiedMessageId === message.id ? '已复制' : '复制消息'
                  "
                  :aria-label="
                    copiedMessageId === message.id ? '已复制' : '复制消息'
                  "
                  @click="copyMessage(message)"
                >
                  <Check
                    v-if="copiedMessageId === message.id"
                    :size="15"
                  /><Copy v-else :size="15" />
                </button>
              </div>
            </div>
          </article>
        </div>

        <div class="assistant-composer-wrap">
          <AssistantMemory
            v-if="initialized && settings.enabled !== false"
            :thread-id="threadId"
            :settings="settings"
            :busy="busy"
            :compressing="
              task.contextPhase === 'compressing' ||
              (busy && task.operation === 'compress')
            "
            :refresh-key="`${task.runId || ''}:${task.status || ''}`"
            @compress="compressContext"
          />
          <div v-if="errorMessage" class="assistant-error" role="alert">
            {{ errorMessage
            }}<button
              v-if="!initialized"
              class="assistant-sidebar-link"
              type="button"
              :disabled="loading"
              @click="initialize"
            >
              重新连接
            </button>
          </div>
          <p v-if="local" class="assistant-hint">
            本地助手模型待添加，请在应用配置中选择 Provider 模型。
          </p>
          <p v-if="settings.enabled === false" class="assistant-hint">
            应用助手已关闭，请在设置 → 功能扩展中启用。
          </p>
          <p v-if="invalidTarget && !local" class="assistant-error">
            模型来源已删除或停用，请重新选择。
          </p>
          <p v-if="busy && task.threadId !== threadId" class="assistant-hint">
            另一个会话正在执行任务
          </p>
          <div class="assistant-composer">
            <AssistantImages
              :images="images"
              editable
              @remove="images = images.filter((image) => image.id !== $event)"
            />
            <textarea
              v-model="draft"
              class="assistant-input"
              aria-label="给应用助手的指令"
              placeholder="描述你想做的事..."
              rows="2"
              maxlength="30000"
              :disabled="!initialized || settings.enabled === false"
              @keydown="handleKeydown"
            />
            <div class="assistant-composer-foot">
              <div class="assistant-composer-tools">
                <button
                  class="assistant-icon-button"
                  type="button"
                  title="新建对话"
                  aria-label="新建对话"
                  :disabled="loading || settings.enabled === false"
                  @click="newThread"
                >
                  <Plus :size="16" />
                </button>
                <button
                  class="assistant-icon-button"
                  type="button"
                  title="添加文本或 JSON 文件"
                  aria-label="添加文本或 JSON 文件"
                  :disabled="!initialized || busy"
                  @click="attachmentInput?.click()"
                >
                  <Paperclip :size="16" />
                </button>
                <input
                  ref="attachmentInput"
                  class="assistant-file-input"
                  type="file"
                  accept=".txt,.md,.json,.csv,.log,text/plain,application/json"
                  @change="attachTextFile"
                />
                <button
                  class="assistant-icon-button"
                  type="button"
                  title="打开 JSON 编辑器"
                  aria-label="打开 JSON 编辑器"
                  @click="openJsonEditor"
                >
                  <Braces :size="16" />
                </button>
              </div>
              <AssistantModelSelect
                class="assistant-model-select"
                :settings="settings"
                :providers="providers"
                :codex-accounts="codexAccounts"
                :runtime-models="runtimeModels"
                :disabled="busy"
                @change="settings = $event"
                @saving="modelSaving = $event"
                @error="errorMessage = $event"
              />
              <button
                v-if="busy"
                class="assistant-button assistant-stop"
                type="button"
                :disabled="task.status === 'cancelling'"
                @click="stop"
              >
                <Square :size="17" />{{
                  task.status === "cancelling" ? "正在停止…" : "停止"
                }}
              </button>
              <button
                v-else
                class="assistant-button assistant-send"
                type="button"
                :disabled="!canSend"
                @click="sendMessage"
              >
                <Send :size="16" />发送
              </button>
            </div>
          </div>
          <p class="assistant-keyboard-hint">
            Enter 发送 · Shift + Enter 换行 · 支持粘贴图片
          </p>
        </div>
      </div>
    </div>
    <el-drawer
      v-model="historyOpen"
      title="历史对话"
      size="360px"
      :lock-scroll="false"
      class="assistant-history-drawer"
    >
      <div class="assistant-history">
        <div class="assistant-sidebar-head">
          <h2 class="assistant-sidebar-title">对话记录</h2>
          <button
            class="assistant-icon-button assistant-new-thread"
            type="button"
            title="新建对话"
            aria-label="新建对话"
            :disabled="loading || settings.enabled === false"
            @click="newThread"
          >
            <Plus :size="21" />
          </button>
        </div>
        <div class="assistant-thread-list">
          <div
            v-for="thread in threads"
            :key="thread.id"
            class="assistant-thread"
            :class="{ 'assistant-thread-active': thread.id === threadId }"
          >
            <button
              class="assistant-thread-select"
              type="button"
              :aria-current="thread.id === threadId ? 'true' : undefined"
              @click="selectThread(thread.id)"
            >
              <span class="assistant-thread-title">{{ thread.title }}</span>
              <span class="assistant-thread-date">{{
                formatDateTime(thread.updatedAt)
              }}</span>
            </button>
            <details class="assistant-thread-menu">
              <summary
                class="assistant-thread-menu-trigger"
                aria-label="对话操作"
                title="对话操作"
              >
                <EllipsisVertical :size="19" />
              </summary>
              <button
                class="assistant-thread-delete"
                type="button"
                :disabled="busy && task.threadId === thread.id"
                @click="removeThread(thread.id)"
              >
                <Trash2 :size="14" />删除对话
              </button>
            </details>
          </div>
          <p v-if="!threads.length && !loading" class="assistant-thread-empty">
            还没有对话，点击 + 开始。
          </p>
          <button
            v-if="hasMoreThreads"
            class="assistant-button"
            type="button"
            :disabled="loading"
            @click="loadMoreThreads"
          >
            加载更多
          </button>
        </div>
      </div>
    </el-drawer>
    <el-drawer
      v-model="capabilitiesOpen"
      title="技能与工具"
      size="520px"
      :lock-scroll="false"
      class="assistant-history-drawer"
      ><AssistantCapabilities
        :settings="settings"
        :busy="busy"
        @change="settings = $event"
    /></el-drawer>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  ArrowUpRight,
  Braces,
  Bot,
  Check,
  CircleAlert,
  Copy,
  EllipsisVertical,
  ExternalLink,
  History,
  LoaderCircle,
  Monitor,
  Paperclip,
  Plus,
  Send,
  Square,
  Settings,
  Trash2,
  Wrench
} from "lucide-vue-next"
import { ElDrawer } from "element-plus"
import "element-plus/es/components/drawer/style/css"
import { appApi, assistantApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"
import MarkdownContent from "@/components/MarkdownContent.vue"
import AssistantImages from "@/features/assistant/components/AssistantImages.vue"
import { readAssistantImages } from "@/features/assistant/attachments"
import AssistantModelSelect from "@/features/assistant/components/AssistantModelSelect.vue"
import AssistantCapabilities from "@/features/assistant/components/AssistantCapabilities.vue"
import AssistantMemory from "@/features/assistant/components/AssistantMemory.vue"

const props = defineProps({
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] },
  standalone: { type: Boolean, default: false }
})
const threads = ref([])
const threadId = ref("")
const messages = ref([])
const task = ref({})
const settings = ref({
  enabled: true,
  targetId: "",
  model: "",
  protocol: "auto"
})
const local = computed(() => settings.value.targetId === "local")
const draft = ref("")
const images = ref([])
const loading = ref(false)
const initialized = ref(false)
const historyLoading = ref(false)
const submitting = ref(false)
const errorMessage = ref("")
const messageList = ref(null)
const attachmentInput = ref(null)
const modelSaving = ref(false)
const copiedMessageId = ref("")
const historyOpen = ref(false)
const capabilitiesOpen = ref(false)
let copyTimer
const hasMoreThreads = ref(false)
const hasMoreMessages = ref(false)
let threadPage = 0
let messagePage = 0
let historySequence = 0
let eventVersion = 0
let settingsVersion = 0
let disposed = false
let unsubscribe
let unsubscribeSettings
const examples = [
  "查看当前桌宠和应用设置",
  "打开 JSON 工具，读取当前内容",
  "让桌宠读书，并告诉我现在的状态"
]
const toolNames = {
  read_app_config: "读取应用配置",
  update_app_settings: "修改应用设置",
  open_app_window: "打开窗口",
  set_pet_action: "控制桌宠",
  read_json_editor: "读取 JSON 编辑器",
  write_json_editor: "写入 JSON 编辑器"
}
const busy = computed(
  () =>
    submitting.value ||
    ["thinking", "working", "replying", "cancelling"].includes(
      task.value.status
    )
)
const currentTask = computed(
  () =>
    task.value.threadId === threadId.value &&
    Boolean(task.value.runId) &&
    task.value.operation !== "compress"
)
const statusText = computed(() =>
  settings.value.enabled === false
    ? "已关闭"
    : {
        thinking: "思考中",
        working: "调用工具中",
        replying: "正在回复",
        cancelling: "正在停止",
        completed: "已完成",
        failed: "执行失败",
        cancelled: "已停止"
      }[task.value.status] || "就绪"
)
const invalidTarget = computed(
  () =>
    settings.value.targetId &&
    !local.value &&
    !props.providers.some(
      (item) => item.id === settings.value.targetId && item.enabled !== false
    ) &&
    !props.codexAccounts.some(
      (item) =>
        `account:${item.id}` === settings.value.targetId &&
        item.disabled !== true
    )
)
const canSend = computed(
  () =>
    initialized.value &&
    settings.value.enabled !== false &&
    !busy.value &&
    !modelSaving.value &&
    !historyLoading.value &&
    !local.value &&
    Boolean(
      threadId.value &&
      (draft.value.trim() || images.value.length) &&
      settings.value.targetId &&
      settings.value.model
    ) &&
    !invalidTarget.value
)

// 活动任务由快照显示，历史中只隐藏本轮消息，避免重进窗口时重复渲染。
const visibleMessages = computed(() => {
  if (!currentTask.value) return messages.value
  const index = messages.value.findLastIndex(
    (message) => message.id === task.value.runId
  )
  const history = index < 0 ? messages.value : messages.value.slice(0, index)
  return [
    ...history,
    {
      id: `user-${task.value.runId}`,
      role: "user",
      text: task.value.message,
      images: task.value.images || [],
      createdAt: messages.value.find(
        (message) => message.id === task.value.runId
      )?.createdAt,
      tools: []
    }
  ]
})
// 历史和当前流式消息共用卡片，保留本轮去重与工具进度的既有规则。
const displayMessages = computed(() => [
  ...visibleMessages.value.map((message) => ({
    ...message,
    status: "completed",
    statusLabel: "已完成"
  })),
  ...(currentTask.value
    ? [
        {
          id: `task-${task.value.runId}`,
          role: "assistant",
          text: task.value.text,
          tools: task.value.tools || [],
          error: task.value.error,
          status: task.value.status,
          statusLabel: statusText.value,
          pending: ["thinking", "working", "replying", "cancelling"].includes(
            task.value.status
          )
        }
      ]
    : [])
])

// 只拦截图片粘贴，普通文字继续由输入框处理。
async function pasteImages(event) {
  const files = Array.from(event.clipboardData?.files || [])
  if (!files.some((file) => file.type.startsWith("image/"))) return
  event.preventDefault()
  try {
    images.value = await readAssistantImages(files, images.value)
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}

async function copyMessage(message) {
  try {
    await navigator.clipboard.writeText(message.text)
    if (disposed) return
    copiedMessageId.value = message.id
    clearTimeout(copyTimer)
    copyTimer = window.setTimeout(() => {
      copiedMessageId.value = ""
    }, 2000)
  } catch (error) {
    errorMessage.value = `复制失败：${error.message || error}`
  }
}

// 文本附件进入可编辑草稿，发送前可检查内容，并遵守现有消息长度限制。
async function attachTextFile(event) {
  const input = event.target
  const file = input.files?.[0]
  if (!file) return
  errorMessage.value = ""
  try {
    if (file.size > 120000)
      throw new Error("附件过大，请选择 30,000 个字符以内的文本或 JSON 文件")
    const text = await file.text()
    if (disposed) return
    if (text.includes("\0")) throw new Error("请选择文本或 JSON 文件")
    const next = [draft.value.trimEnd(), `文件：${file.name}\n${text}`]
      .filter(Boolean)
      .join("\n\n")
    if (next.length > 30000)
      throw new Error("消息与附件总长度不能超过 30,000 个字符")
    draft.value = next
  } catch (error) {
    if (!disposed) errorMessage.value = error.message || String(error)
  } finally {
    input.value = ""
  }
}

async function openMainPanel() {
  try {
    await appApi.showMainPanel({ toolId: "assistant" })
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}

async function openJsonEditor() {
  try {
    await appApi.openToolWindow({ toolId: "json-agent" })
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}

function normalizeMessage(message) {
  const content = message.content
  const parts = content?.parts || []
  return {
    id: message.id,
    role: message.role,
    createdAt: message.createdAt,
    // Mastra 将多模态图片持久化为 file part，刷新历史时恢复为图片附件。
    images: parts
      .filter(
        (part) => part.type === "file" && part.mimeType?.startsWith("image/")
      )
      .map((part) => ({
        dataUrl: part.data.startsWith("data:")
          ? part.data
          : `data:${part.mimeType};base64,${part.data}`,
        mimeType: part.mimeType,
        name: part.filename
      })),
    text:
      typeof content === "string"
        ? content
        : parts
            .filter((part) => part.type === "text")
            .map((part) => part.text)
            .join("") ||
          content?.content ||
          "",
    tools: parts
      .filter((part) => part.type === "tool-invocation")
      .map((part) => {
        const tool = part.toolInvocation
        return {
          id: tool.toolCallId,
          name: tool.toolName,
          input: tool.args,
          output: tool.result,
          status: tool.state === "result" ? "completed" : "running"
        }
      })
  }
}
function formatTool(value) {
  return typeof value === "string" ? value : JSON.stringify(value, null, 2)
}
async function loadMessages(older = false) {
  const sequence = ++historySequence
  const id = threadId.value
  const page = older ? messagePage + 1 : 0
  historyLoading.value = true
  try {
    const result = await assistantApi.messages(id, page)
    if (disposed || sequence !== historySequence || id !== threadId.value)
      return
    const items = (result.messages || []).map(normalizeMessage)
    messages.value = older ? [...items, ...messages.value] : items
    messagePage = page
    hasMoreMessages.value = result.hasMore || false
    if (!older) scrollToEnd()
  } catch (error) {
    if (!disposed && sequence === historySequence)
      errorMessage.value = error.message || String(error)
  } finally {
    if (!disposed && sequence === historySequence) historyLoading.value = false
  }
}
async function selectThread(id) {
  // 选择后收起历史抽屉，消息区继续展示当前会话。
  historyOpen.value = false
  threadId.value = id
  messages.value = []
  errorMessage.value = ""
  await loadMessages()
}
async function refreshThreads() {
  const result = await assistantApi.listThreads(0)
  if (disposed) return
  threads.value = result.threads
  hasMoreThreads.value = result.hasMore
  threadPage = 0
}
async function loadMoreThreads() {
  try {
    const result = await assistantApi.listThreads(threadPage + 1)
    if (disposed) return
    threads.value = [...threads.value, ...result.threads]
    hasMoreThreads.value = result.hasMore
    threadPage += 1
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}
async function newThread() {
  try {
    const thread = await assistantApi.createThread()
    if (disposed) return
    threads.value.unshift(thread)
    await selectThread(thread.id)
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}
async function removeThread(id) {
  try {
    await assistantApi.deleteThread(id)
    if (disposed) return
    threads.value = threads.value.filter((thread) => thread.id !== id)
    if (threadId.value === id) {
      if (threads.value.length) await selectThread(threads.value[0].id)
      else await newThread()
    }
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}
async function sendMessage() {
  if (!canSend.value) return
  const message = draft.value.trim()
  const id = threadId.value
  submitting.value = true
  errorMessage.value = ""
  try {
    const sentImages = images.value
    const snapshot = await assistantApi.send({
      threadId: id,
      message,
      images: sentImages
    })
    if (disposed) return
    // 事件可能先于 IPC 回包到达，已有进度不能被最初的 thinking 快照覆盖。
    if (task.value.runId !== snapshot.runId) task.value = snapshot
    draft.value = ""
    images.value = images.value.filter((image) => !sentImages.includes(image))
  } catch (error) {
    errorMessage.value = error.message || String(error)
  } finally {
    submitting.value = false
  }
}
async function stop() {
  try {
    await assistantApi.cancel(task.value.runId)
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}
async function compressContext() {
  if (busy.value || !threadId.value) return
  submitting.value = true
  errorMessage.value = ""
  try {
    const snapshot = await assistantApi.compress(threadId.value)
    if (!disposed && task.value.runId !== snapshot.runId) task.value = snapshot
  } catch (error) {
    if (!disposed) errorMessage.value = error.message || String(error)
  } finally {
    submitting.value = false
  }
}
function handleKeydown(event) {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault()
    sendMessage()
  }
}
async function openWindow() {
  try {
    await appApi.openToolWindow({ toolId: "assistant" })
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}
async function openSettings() {
  try {
    await appApi.showMainPanel({ toolId: "assistant-settings" })
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}
function scrollToEnd() {
  nextTick(() => {
    if (messageList.value)
      messageList.value.scrollTop = messageList.value.scrollHeight
  })
}
function applyEvent(event) {
  if (disposed) return
  if (event.type === "state") {
    eventVersion += 1
    const previous = task.value
    task.value = event.state
    if (
      previous.status !== task.value.status &&
      ["completed", "failed", "cancelled"].includes(task.value.status)
    ) {
      eventVersion += 1
      refreshThreads().catch((error) => {
        errorMessage.value = error.message || String(error)
      })
      if (threadId.value === task.value.threadId) loadMessages()
      if (task.value.operation === "compress" && task.value.error)
        errorMessage.value = task.value.error
    }
  } else if (event.type === "text-delta" && event.runId === task.value.runId) {
    task.value.text = (task.value.text || "") + (event.text || "")
    if (task.value.status !== "cancelling") task.value.status = "replying"
  }
}
async function initialize() {
  if (loading.value) return
  loading.value = true
  errorMessage.value = ""
  try {
    // 先监听再取快照，后台正在运行时也能恢复文本和工具进度。
    if (!unsubscribe) {
      const listener = await assistantApi.onEvent(applyEvent)
      if (disposed) {
        listener()
        return
      }
      unsubscribe = listener
    }
    if (!unsubscribeSettings) {
      const listener = await appApi.onStateChangedReady((snapshot) => {
        if (disposed || !snapshot.appSettings?.agents?.assistant) return
        settingsVersion += 1
        const wasDisabled = settings.value.enabled === false
        settings.value = {
          protocol: "auto",
          ...snapshot.appSettings.agents.assistant
        }
        if (wasDisabled && settings.value.enabled !== false) initialize()
      })
      if (disposed) {
        listener()
        return
      }
      unsubscribeSettings = listener
    }
    const version = eventVersion
    const configVersion = settingsVersion
    const result = await assistantApi.bootstrap()
    if (disposed) return
    if (configVersion === settingsVersion)
      settings.value = { protocol: "auto", ...result.settings }
    if (version === eventVersion) task.value = result.state
    threads.value = result.threads.threads
    hasMoreThreads.value = result.threads.hasMore
    initialized.value = true
    const id =
      threads.value.find((item) => item.id === task.value.threadId)?.id ||
      threads.value[0]?.id
    if (id) await selectThread(id)
    else if (settings.value.enabled !== false) await newThread()
  } catch (error) {
    if (!disposed) errorMessage.value = error.message || String(error)
  } finally {
    if (!disposed) loading.value = false
  }
}
watch(
  () => [task.value.text, task.value.tools?.length, messages.value.length],
  scrollToEnd
)
onMounted(initialize)
onBeforeUnmount(() => {
  disposed = true
  historySequence += 1
  unsubscribe?.()
  unsubscribeSettings?.()
  clearTimeout(copyTimer)
})
</script>

<style scoped lang="less">
.application-assistant {
  position: relative;
  display: flex;
  height: 100%;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--font-size-base);
  .assistant-button,
  .assistant-icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    gap: 6px;
    min-height: 36px;
    padding: 7px 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition:
      background-color 150ms,
      border-color 150ms;
    &:hover {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
    }
    &:focus-visible {
      outline: 2px solid var(--color-primary);
      outline-offset: 2px;
    }
    &:disabled {
      opacity: 0.45;
      cursor: default;
    }
  }
  .assistant-icon-button {
    width: 36px;
    height: 36px;
    padding: 0;
  }
  .assistant-sidebar-link {
    border: 0;
    padding: 0 4px;
    background: transparent;
    color: var(--color-primary);
    font: inherit;
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .assistant-header {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    .assistant-heading {
      flex: 1;
      min-width: 0;
      .assistant-title {
        margin: 0;
        font-size: var(--font-size-lg);
        line-height: 1.3;
      }
      .assistant-description {
        margin: 3px 0 0;
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        line-height: 1.5;
      }
    }
    .assistant-header-actions {
      display: flex;
      align-items: center;
      flex: none;
      gap: 8px;
      .assistant-connection {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        margin-right: 8px;
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        .assistant-connection-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: var(--color-text-soft);
        }
        &.assistant-connection-ready .assistant-connection-dot {
          background: var(--color-success);
        }
      }
    }
  }
  .assistant-layout {
    display: flex;
    flex: 1;
    min-height: 0;
    .assistant-workspace {
      display: flex;
      flex: 1;
      min-width: 0;
      min-height: 0;
      flex-direction: column;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      overflow: hidden;
      .assistant-messages {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 28px 52px 8px;
        scrollbar-gutter: stable;
        .assistant-earlier {
          display: flex;
          margin: 0 auto 24px;
        }
        .assistant-welcome {
          display: flex;
          max-width: 600px;
          flex-direction: column;
          align-items: center;
          margin: 36px auto;
          text-align: center;
          .assistant-welcome-icon {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 52px;
            height: 52px;
            border-radius: 12px;
            background: var(--color-primary-soft);
            color: var(--color-primary);
          }
          .assistant-welcome-title {
            margin: 18px 0 8px;
            font-size: var(--font-size-lg);
          }
          .assistant-welcome-description {
            margin: 0;
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
            line-height: 1.7;
          }
          .assistant-examples {
            display: flex;
            width: 100%;
            flex-direction: column;
            gap: 8px;
            margin-top: 20px;
            .assistant-example {
              display: flex;
              align-items: center;
              justify-content: space-between;
              gap: 16px;
              padding: 10px 14px;
              border: 1px solid var(--color-line);
              border-radius: 8px;
              background: var(--color-panel);
              color: var(--color-text);
              font: inherit;
              font-size: var(--font-size-sm);
              text-align: left;
              cursor: pointer;
              &:hover {
                background: var(--color-panel-soft);
                border-color: var(--color-line-strong);
              }
            }
          }
        }
        .assistant-message {
          display: flex;
          align-items: flex-start;
          width: 100%;
          max-width: 960px;
          margin: 0 auto 28px;
          .assistant-message-main {
            width: fit-content;
            max-width: 100%;
            min-width: 0;
            .assistant-message-header {
              display: flex;
              align-items: center;
              flex-wrap: wrap;
              gap: 8px;
              margin-bottom: 8px;
              .assistant-message-label {
                font-size: var(--font-size-sm);
              }
              .assistant-message-date {
                color: var(--color-text-soft);
                font-size: var(--font-size-xs);
              }
              .assistant-message-status {
                display: inline-flex;
                align-items: center;
                gap: 4px;
                color: var(--color-text-muted);
                font-size: var(--font-size-xs);
                &.assistant-message-status-failed {
                  color: var(--color-danger);
                }
                .assistant-loading-icon {
                  animation: assistant-spin 1s linear infinite;
                }
              }
            }
            .assistant-message-body {
              position: relative;
              min-width: 0;
              .assistant-message-text {
                overflow-wrap: anywhere;
                font-size: var(--font-size-base);
                line-height: 1.75;
              }
              .assistant-tool {
                margin-top: 10px;
                border: 1px solid var(--color-line);
                border-radius: 6px;
                .assistant-tool-summary {
                  display: flex;
                  align-items: center;
                  gap: 8px;
                  padding: 8px 10px;
                  color: var(--color-text-muted);
                  font-size: var(--font-size-xs);
                  cursor: pointer;
                }
                .assistant-tool-detail {
                  max-height: 240px;
                  margin: 0;
                  overflow: auto;
                  padding: 10px;
                  border-top: 1px solid var(--color-line);
                  white-space: pre-wrap;
                  overflow-wrap: anywhere;
                  font-size: var(--font-size-xs);
                }
              }
              .assistant-pending {
                color: var(--color-text-muted);
                font-size: var(--font-size-sm);
              }
              .assistant-message-copy {
                position: absolute;
                top: 0;
                left: calc(100% + 8px);
                width: 28px;
                height: 28px;
                min-height: 28px;
                border-color: transparent;
                opacity: 0;
                pointer-events: none;
                &.assistant-message-copied {
                  color: var(--color-success);
                  opacity: 1;
                  pointer-events: auto;
                }
                &:focus-visible {
                  opacity: 1;
                  pointer-events: auto;
                }
              }
            }
          }
          &:hover,
          &:focus-within {
            .assistant-message-main
              .assistant-message-body
              .assistant-message-copy {
              opacity: 1;
              pointer-events: auto;
            }
          }
          &.assistant-message-user {
            justify-content: flex-end;
            .assistant-message-main {
              .assistant-message-header {
                justify-content: flex-end;
              }
              .assistant-message-body {
                padding: 10px 14px;
                border-radius: 12px;
                background: var(--color-primary-soft);
                .assistant-message-copy {
                  right: calc(100% + 8px);
                  left: auto;
                }
                .assistant-message-text {
                  white-space: pre-wrap;
                }
              }
            }
          }
        }
      }
      .assistant-composer-wrap {
        flex: none;
        padding: 8px 24px 12px;
        .assistant-hint {
          margin: 0 0 8px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
          line-height: 1.6;
        }
        .assistant-composer {
          padding: 12px;
          border: 1px solid var(--color-line-strong);
          border-radius: 10px;
          background: var(--color-panel);
          &:focus-within {
            border-color: var(--color-primary);
          }
          .assistant-input {
            display: block;
            width: 100%;
            height: 60px;
            margin: 0 0 10px;
            padding: 0 2px;
            resize: none;
            border: 0;
            outline: none;
            background: transparent;
            color: var(--color-text);
            font: inherit;
            font-size: var(--font-size-base);
            line-height: 1.6;
            &::placeholder {
              color: var(--color-text-soft);
            }
          }
          .assistant-composer-foot {
            display: flex;
            align-items: center;
            gap: 8px;
            .assistant-composer-tools {
              display: flex;
              gap: 4px;
              .assistant-icon-button {
                border-color: transparent;
                color: var(--color-text-muted);
              }
              .assistant-file-input {
                display: none;
              }
            }
            .assistant-model-select {
              width: 200px;
              min-width: 160px;
              margin-left: auto;
            }
            .assistant-send,
            .assistant-stop {
              min-width: 84px;
            }
            .assistant-send {
              border-color: var(--color-primary-solid);
              background: var(--color-primary-solid);
              color: #fff;
            }
            .assistant-stop {
              border-color: var(--color-danger-line);
              background: var(--color-danger-soft);
              color: var(--color-danger);
            }
          }
        }
        .assistant-keyboard-hint {
          margin: 6px 0 0;
          text-align: right;
          color: var(--color-text-soft);
          font-size: var(--font-size-xs);
        }
      }
      .assistant-error {
        margin: 8px 0;
        color: var(--color-danger);
        overflow-wrap: anywhere;
        font-size: var(--font-size-sm);
        line-height: 1.6;
      }
    }
  }
  :deep(.el-overlay) {
    position: absolute;
  }
  :deep(.assistant-history-drawer) {
    max-width: 100%;
    background: var(--color-panel);
    .el-drawer__header {
      margin-bottom: 0;
      padding: 18px;
      border-bottom: 1px solid var(--color-line);
      color: var(--color-text);
    }
    .el-drawer__title {
      font-size: var(--font-size-lg);
    }
    .el-drawer__body {
      min-height: 0;
      padding: 0;
      display: flex;
      flex-direction: column;
    }
  }
  .assistant-history {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    .assistant-sidebar-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      flex: none;
      padding: 14px 18px;
      .assistant-sidebar-title {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-muted);
      }
    }
    .assistant-thread-list {
      flex: 1;
      min-height: 0;
      overflow: auto;
      padding: 0 10px 16px;
      .assistant-thread {
        position: relative;
        display: flex;
        align-items: center;
        margin-bottom: 4px;
        border-radius: 7px;
        &.assistant-thread-active {
          background: var(--color-primary-soft);
        }
        &:hover {
          background: var(--color-panel-soft);
        }
        .assistant-thread-select {
          display: flex;
          flex: 1;
          min-width: 0;
          flex-direction: column;
          gap: 5px;
          padding: 12px;
          border: 0;
          border-radius: inherit;
          background: transparent;
          color: var(--color-text);
          text-align: left;
          font: inherit;
          cursor: pointer;
          .assistant-thread-title {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            font-size: var(--font-size-base);
          }
          .assistant-thread-date {
            color: var(--color-text-muted);
            font-size: var(--font-size-xs);
          }
        }
        .assistant-thread-menu {
          flex: none;
          margin-right: 8px;
          .assistant-thread-menu-trigger {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 28px;
            height: 30px;
            border-radius: 6px;
            color: var(--color-text-muted);
            cursor: pointer;
            list-style: none;
            &::-webkit-details-marker {
              display: none;
            }
            &:hover {
              background: var(--color-panel);
            }
          }
          .assistant-thread-delete {
            position: absolute;
            top: 48px;
            right: 8px;
            z-index: 2;
            display: flex;
            align-items: center;
            gap: 6px;
            padding: 9px 12px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);
            box-shadow: var(--shadow-panel);
            color: var(--color-danger);
            font: inherit;
            font-size: var(--font-size-sm);
            cursor: pointer;
            &:disabled {
              opacity: 0.45;
              cursor: default;
            }
          }
        }
      }
      .assistant-thread-empty {
        margin: 18px 8px;
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }
    }
  }
}
@keyframes assistant-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

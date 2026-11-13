<template>
  <section class="application-assistant">
    <header class="assistant-header">
      <div class="assistant-heading">
        <h1 class="assistant-title">应用助手</h1>
        <p class="assistant-description">与 Mastra 助手对话、查询配置、操作工具接口、编辑 JSON 和控制桌宠。</p>
      </div>
      <div class="assistant-header-actions">
        <span class="assistant-connection" :class="{ 'assistant-connection-ready': initialized && settings.enabled !== false }" role="status">
          <span class="assistant-connection-dot"></span>{{ loading ? '连接中' : settings.enabled === false ? '已关闭' : initialized ? '已连接' : '未连接' }}
        </span>
        <button class="assistant-button" type="button" @click="openMainPanel"><Monitor :size="19" />打开主面板</button>
        <button class="assistant-button" type="button" @click="openSettings"><Settings :size="19" />应用配置</button>
      </div>
    </header>

    <div class="assistant-layout">
      <aside class="assistant-sidebar">
        <div class="assistant-model-field">
          <label class="assistant-model-label" for="assistant-model">当前模型</label>
          <div class="assistant-model-control">
            <Box :size="21" class="assistant-model-icon" />
            <el-select
              id="assistant-model"
              :model-value="settings.model || undefined"
              class="assistant-model-select"
              placeholder="请先配置模型"
              filterable
              allow-create
              default-first-option
              :disabled="modelSaving || busy || local || !settings.targetId || invalidTarget"
              @update:model-value="selectModel"
            >
              <el-option v-for="model in modelOptions" :key="model" :value="model" :label="model" />
            </el-select>
          </div>
        </div>
        <div class="assistant-sidebar-head">
          <h2 class="assistant-sidebar-title">对话记录</h2>
          <button class="assistant-icon-button assistant-new-thread" type="button" title="新建对话" aria-label="新建对话" :disabled="loading || settings.enabled === false" @click="newThread"><Plus :size="21" /></button>
        </div>
        <div class="assistant-thread-list">
          <div v-for="thread in threads" :key="thread.id" class="assistant-thread" :class="{ 'assistant-thread-active': thread.id === threadId }">
            <button class="assistant-thread-select" type="button" :aria-current="thread.id === threadId ? 'true' : undefined" @click="selectThread(thread.id)">
              <span class="assistant-thread-title">{{ thread.title }}</span>
              <span class="assistant-thread-date">{{ formatDateTime(thread.updatedAt) }}</span>
            </button>
            <details class="assistant-thread-menu">
              <summary class="assistant-thread-menu-trigger" aria-label="对话操作" title="对话操作"><EllipsisVertical :size="19" /></summary>
              <button class="assistant-thread-delete" type="button" :disabled="busy && task.threadId === thread.id" @click="removeThread(thread.id)"><Trash2 :size="14" />删除对话</button>
            </details>
          </div>
          <p v-if="!threads.length && !loading" class="assistant-thread-empty">还没有对话，点击 + 开始。</p>
          <button v-if="hasMoreThreads" class="assistant-button" type="button" :disabled="loading" @click="loadMoreThreads">加载更多</button>
        </div>
        <footer v-if="!standalone" class="assistant-sidebar-foot">
          <button class="assistant-sidebar-link" type="button" @click="$emit('back')"><ArrowLeft :size="15" />工具列表</button>
          <button class="assistant-sidebar-link" type="button" @click="openWindow"><ExternalLink :size="15" />独立窗口</button>
        </footer>
      </aside>

      <div class="assistant-workspace">
        <div ref="messageList" class="assistant-messages" aria-live="polite">
          <button v-if="hasMoreMessages" class="assistant-button assistant-earlier" type="button" :disabled="historyLoading" @click="loadMessages(true)">查看更早消息</button>
          <div v-if="!messages.length && !currentTask && !loading" class="assistant-welcome">
            <span class="assistant-welcome-icon"><Bot :size="32" :stroke-width="1.5" /></span>
            <h2 class="assistant-welcome-title">你好，有什么需要我帮忙的？</h2>
            <p class="assistant-welcome-description">查询应用配置、处理 JSON、打开工具，或让桌宠换个动作。</p>
            <div class="assistant-examples">
              <button v-for="example in examples" :key="example" class="assistant-example" type="button" @click="draft = example">{{ example }}<ArrowUpRight :size="15" /></button>
            </div>
          </div>
          <article v-for="message in displayMessages" :key="message.id" class="assistant-message" :class="{ 'assistant-message-user': message.role === 'user' }">
            <span v-if="message.role === 'user'" class="assistant-user-avatar" aria-hidden="true"><UserRound :size="25" :stroke-width="1.6" /></span>
            <div class="assistant-message-main">
              <header class="assistant-message-header">
                <span class="assistant-message-label">{{ message.role === 'user' ? '你' : '应用助手' }}</span>
                <span v-if="message.role !== 'user'" class="assistant-message-status" :class="{ 'assistant-message-status-completed': message.status === 'completed', 'assistant-message-status-failed': message.status === 'failed' }">
                  <CircleCheck v-if="message.status === 'completed'" :size="13" />
                  <LoaderCircle v-else-if="message.pending" class="assistant-loading-icon" :size="13" />
                  <CircleAlert v-else :size="13" />
                  {{ message.statusLabel }}
                </span>
                <time v-if="message.createdAt" class="assistant-message-date">{{ formatDateTime(message.createdAt) }}</time>
              </header>
              <div class="assistant-message-body">
                <div v-if="message.text" class="assistant-message-text">{{ message.text }}</div>
                <details v-for="tool in message.tools" :key="tool.id" class="assistant-tool">
                  <summary class="assistant-tool-summary"><Wrench :size="14" />{{ toolNames[tool.name] || tool.name }}<span>{{ tool.status === 'running' ? '执行中' : tool.status === 'failed' ? '失败' : '已完成' }}</span></summary>
                  <pre class="assistant-tool-detail">{{ tool.error || formatTool(tool.output ?? tool.input) }}</pre>
                </details>
                <span v-if="message.pending && !message.text && !message.tools?.length" class="assistant-pending">正在思考…</span>
                <p v-if="message.error" class="assistant-error">{{ message.error }}</p>
                <div v-if="message.role !== 'user' && message.text" class="assistant-message-actions">
                  <button class="assistant-icon-button" type="button" :title="copiedMessageId === message.id ? '已复制' : '复制回复'" :aria-label="copiedMessageId === message.id ? '已复制' : '复制回复'" @click="copyMessage(message)"><Check v-if="copiedMessageId === message.id" :size="18" /><Copy v-else :size="18" /></button>
                  <button v-for="rating in ['up', 'down']" :key="rating" class="assistant-icon-button" :class="{ 'assistant-rating-active': messageRatings[message.id] === rating }" type="button" :title="rating === 'up' ? '有帮助' : '需要改进'" :aria-label="rating === 'up' ? '有帮助' : '需要改进'" :aria-pressed="messageRatings[message.id] === rating" @click="rateMessage(message.id, rating)"><ThumbsUp v-if="rating === 'up'" :size="18" /><ThumbsDown v-else :size="18" /></button>
                </div>
              </div>
            </div>
          </article>
        </div>

        <div class="assistant-composer-wrap">
          <div v-if="errorMessage" class="assistant-error" role="alert">{{ errorMessage }}<button v-if="!initialized" class="assistant-sidebar-link" type="button" :disabled="loading" @click="initialize">重新连接</button></div>
          <p v-if="local" class="assistant-hint">本地助手模型待添加，请在应用配置中选择 Provider 模型。</p>
          <p v-if="settings.enabled === false" class="assistant-hint">应用助手已关闭，请在设置 → 功能扩展中启用。</p>
          <p v-if="invalidTarget && !local" class="assistant-error">模型来源已删除或停用，请重新选择。</p>
          <p v-if="busy && task.threadId !== threadId" class="assistant-hint">另一个会话正在执行任务</p>
          <div class="assistant-composer">
            <textarea v-model="draft" class="assistant-input" aria-label="给应用助手的指令" placeholder="描述你想做的事..." rows="2" maxlength="30000" :disabled="!initialized || settings.enabled === false" @keydown="handleKeydown" />
            <div class="assistant-composer-foot">
              <div class="assistant-composer-tools">
                <button class="assistant-icon-button" type="button" title="新建对话" aria-label="新建对话" :disabled="loading || settings.enabled === false" @click="newThread"><Plus :size="22" /></button>
                <button class="assistant-icon-button" type="button" title="添加文本或 JSON 文件" aria-label="添加文本或 JSON 文件" :disabled="!initialized || busy" @click="attachmentInput?.click()"><Paperclip :size="21" /></button>
                <input ref="attachmentInput" class="assistant-file-input" type="file" accept=".txt,.md,.json,.csv,.log,text/plain,application/json" @change="attachTextFile" />
                <button class="assistant-icon-button" type="button" title="打开 JSON 编辑器" aria-label="打开 JSON 编辑器" @click="openJsonEditor"><Braces :size="21" /></button>
              </div>
              <span class="assistant-keyboard-hint">Enter 发送 <span class="assistant-keyboard-divider">|</span> Shift + Enter 换行</span>
              <button v-if="busy" class="assistant-button assistant-stop" type="button" :disabled="task.status === 'cancelling'" @click="stop"><Square :size="17" />{{ task.status === 'cancelling' ? '正在停止…' : '停止' }}</button>
              <button v-else class="assistant-button assistant-send" type="button" :disabled="!canSend" @click="sendMessage"><Send :size="19" />发送</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  ArrowLeft,
  ArrowUpRight,
  Box,
  Braces,
  Bot,
  Check,
  CircleAlert,
  CircleCheck,
  Copy,
  EllipsisVertical,
  ExternalLink,
  LoaderCircle,
  Monitor,
  Paperclip,
  Plus,
  Send,
  Square,
  Settings,
  ThumbsDown,
  ThumbsUp,
  Trash2,
  UserRound,
  Wrench
} from "lucide-vue-next"
import { ElOption, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"
import { appApi, assistantApi, settingsApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] },
  standalone: { type: Boolean, default: false }
})
defineEmits(["back"])
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
const loading = ref(false)
const initialized = ref(false)
const historyLoading = ref(false)
const submitting = ref(false)
const errorMessage = ref("")
const messageList = ref(null)
const attachmentInput = ref(null)
const modelSaving = ref(false)
const copiedMessageId = ref("")
const messageRatings = ref({})
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
  () => task.value.threadId === threadId.value && Boolean(task.value.runId)
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
      draft.value.trim() &&
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
      createdAt: messages.value.find(message => message.id === task.value.runId)?.createdAt,
      tools: []
    }
  ]
})
// 历史和当前流式消息共用卡片，保留本轮去重与工具进度的既有规则。
const displayMessages = computed(() => [
  ...visibleMessages.value.map(message => ({
    ...message,
    status: "completed",
    statusLabel: "已完成"
  })),
  ...(currentTask.value ? [{
    id: `task-${task.value.runId}`,
    role: "assistant",
    text: task.value.text,
    tools: task.value.tools || [],
    error: task.value.error,
    status: task.value.status,
    statusLabel: statusText.value,
    pending: ["thinking", "working", "replying", "cancelling"].includes(task.value.status)
  }] : [])
])
const modelOptions = computed(() => [
  ...new Set([
    settings.value.model,
    props.providers.find(provider => provider.id === settings.value.targetId)?.runtimeConfig?.mainModel,
    ...props.runtimeModels.filter(model => model.providerId === settings.value.targetId
      || settings.value.targetId?.startsWith("account:") && props.providers.some(
        provider => provider.id === model.providerId && provider.cli === "codex"
      )).map(model => model.name || model.id)
  ].filter(Boolean))
])

// 仅切换当前来源的模型；来源和协议继续统一在应用配置中管理。
async function selectModel(model) {
  if (!model?.trim() || model === settings.value.model || modelSaving.value || busy.value || local.value) return
  const targetId = settings.value.targetId
  const version = ++settingsVersion
  modelSaving.value = true
  errorMessage.value = ""
  try {
    const result = await settingsApi.saveSettings({ agents: { assistant: { model: model.trim() } } })
    if (!disposed && version === settingsVersion && targetId === settings.value.targetId) {
      settings.value = { ...settings.value, ...(result?.appSettings?.agents?.assistant || { model: model.trim() }) }
    }
  } catch (error) {
    if (!disposed) errorMessage.value = error.message || String(error)
  } finally {
    modelSaving.value = false
  }
}

async function copyMessage(message) {
  try {
    await navigator.clipboard.writeText(message.text)
    if (disposed) return
    copiedMessageId.value = message.id
    clearTimeout(copyTimer)
    copyTimer = window.setTimeout(() => { copiedMessageId.value = "" }, 2000)
  } catch (error) {
    errorMessage.value = `复制失败：${error.message || error}`
  }
}

function rateMessage(id, rating) {
  messageRatings.value[id] = messageRatings.value[id] === rating ? "" : rating
}

// 文本附件进入可编辑草稿，发送前可检查内容，并遵守现有消息长度限制。
async function attachTextFile(event) {
  const input = event.target
  const file = input.files?.[0]
  if (!file) return
  errorMessage.value = ""
  try {
    if (file.size > 120000) throw new Error("附件过大，请选择 30,000 个字符以内的文本或 JSON 文件")
    const text = await file.text()
    if (disposed) return
    if (text.includes("\0")) throw new Error("请选择文本或 JSON 文件")
    const next = [draft.value.trimEnd(), `文件：${file.name}\n${text}`].filter(Boolean).join("\n\n")
    if (next.length > 30000) throw new Error("消息与附件总长度不能超过 30,000 个字符")
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
    const snapshot = await assistantApi.send({ threadId: id, message })
    if (disposed) return
    // 事件可能先于 IPC 回包到达，已有进度不能被最初的 thinking 快照覆盖。
    if (task.value.runId !== snapshot.runId) task.value = snapshot
    draft.value = ""
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
  --assistant-accent: #0076ee;
  --assistant-line: color-mix(in srgb, var(--assistant-accent) 9%, var(--color-line));
  --assistant-wash: color-mix(in srgb, var(--assistant-accent) 4%, var(--color-page));
  display: flex;
  flex: 1;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  gap: 24px;
  padding: 18px 18px 38px;
  overflow: hidden;
  background: var(--assistant-wash);
  color: var(--color-text);

  .assistant-button,
  .assistant-icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    gap: 10px;
    border: 1px solid var(--assistant-line);
    background: var(--color-panel);
    color: var(--color-text);
    box-shadow: 0 2px 3px rgba(30, 70, 120, 0.04);
    font: inherit;
    cursor: pointer;
    transition: background-color 150ms, border-color 150ms;

    &:hover {
      border-color: color-mix(in srgb, var(--assistant-accent) 40%, var(--assistant-line));
      background: var(--color-panel-soft);
    }
    &:focus-visible {
      outline: 2px solid var(--assistant-accent);
      outline-offset: 3px;
    }
    &:disabled {
      opacity: 0.45;
      cursor: default;
    }
  }
  .assistant-button {
    min-height: 48px;
    padding: 9px 24px;
    border-radius: 19px;
    font-size: 16px;
  }
  .assistant-icon-button {
    width: 42px;
    height: 42px;
    padding: 0;
    border-radius: 13px;
  }
  .assistant-sidebar-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    &:hover { color: var(--assistant-accent); }
  }
  .assistant-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    gap: 24px;
    padding: 0 20px;
    .assistant-heading {
      flex: 1;
      min-width: 0;
      .assistant-title {
        margin: 0 0 2px;
        font-size: 28px;
        line-height: 1.35;
        letter-spacing: -0.5px;
      }
      .assistant-description {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 14px;
        line-height: 1.65;
      }
    }
    .assistant-header-actions {
      display: flex;
      align-items: center;
      flex: none;
      gap: 22px;
      .assistant-connection {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 10px;
        min-height: 48px;
        padding: 9px 22px;
        border: 1px solid var(--assistant-line);
        border-radius: 19px;
        color: var(--color-text-muted);
        font-size: 16px;
        .assistant-connection-dot {
          width: 9px;
          height: 9px;
          border-radius: 50%;
          background: var(--color-text-soft);
        }
        &.assistant-connection-ready {
          color: var(--color-text);
          .assistant-connection-dot { background: var(--color-success); }
        }
      }
    }
  }
  .assistant-layout {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: 14px;
    .assistant-sidebar,
    .assistant-workspace {
      border: 1px solid color-mix(in srgb, var(--assistant-line) 30%, var(--color-panel));
      border-radius: 21px;
      background: var(--color-panel);
      box-shadow: 0 4px 14px rgba(34, 75, 130, 0.035);
    }
    .assistant-sidebar {
      display: flex;
      width: 342px;
      min-height: 0;
      flex: none;
      flex-direction: column;
      .assistant-model-field {
        display: flex;
        flex-direction: column;
        flex: none;
        gap: 10px;
        padding: 18px 18px 16px;
        border-bottom: 1px solid var(--assistant-line);
        .assistant-model-label { font-size: 16px; }
        .assistant-model-control {
          display: flex;
          align-items: center;
          min-height: 46px;
          border: 1px solid var(--assistant-line);
          border-radius: 15px;
          box-shadow: 0 2px 3px rgba(30, 70, 120, 0.04);
          .assistant-model-icon {
            flex: none;
            margin-left: 14px;
            color: var(--color-text);
          }
          .assistant-model-select {
            flex: 1;
            min-width: 0;
            :deep(.el-select__wrapper) {
              min-height: 44px;
              border-radius: 15px;
              background: transparent;
              box-shadow: none;
              padding-left: 13px;
              font-size: 16px;
            }
            :deep(.el-select__selected-item) { color: var(--color-text); }
            :deep(.el-select__placeholder.is-transparent) { color: var(--color-text-muted); }
          }
        }
      }
      .assistant-sidebar-head {
        display: flex;
        justify-content: space-between;
        align-items: center;
        flex: none;
        padding: 14px 18px 10px;
        .assistant-sidebar-title { margin: 0; font-size: 19px; }
        .assistant-new-thread {
          width: 38px;
          height: 38px;
          border-color: var(--assistant-accent);
          background: var(--assistant-accent);
          color: #fff;
        }
      }
      .assistant-thread-list {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 0 14px 48px;
        .assistant-thread {
          position: relative;
          display: flex;
          align-items: center;
          margin-bottom: 7px;
          border-radius: 16px;
          &.assistant-thread-active {
            background: color-mix(in srgb, var(--assistant-accent) 10%, var(--color-panel));
          }
          &:hover { background: color-mix(in srgb, var(--assistant-accent) 6%, var(--color-panel)); }
          .assistant-thread-select {
            display: flex;
            flex: 1;
            min-width: 0;
            flex-direction: column;
            gap: 7px;
            border: 0;
            border-radius: inherit;
            padding: 13px 15px;
            background: transparent;
            color: inherit;
            font: inherit;
            text-align: left;
            cursor: pointer;
            .assistant-thread-title {
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
              font-size: 16px;
            }
            .assistant-thread-date { color: var(--color-text-muted); font-size: 14px; }
          }
          .assistant-thread-menu {
            flex: none;
            margin-right: 8px;
            .assistant-thread-menu-trigger {
              display: flex;
              align-items: center;
              justify-content: center;
              width: 30px;
              height: 34px;
              border-radius: 8px;
              color: var(--assistant-accent);
              cursor: pointer;
              list-style: none;
              &::-webkit-details-marker { display: none; }
              &:hover { background: var(--color-panel); }
            }
            .assistant-thread-delete {
              position: absolute;
              top: 58px;
              right: 8px;
              z-index: 2;
              display: flex;
              align-items: center;
              gap: 8px;
              padding: 10px 14px;
              border: 1px solid var(--assistant-line);
              border-radius: 10px;
              background: var(--color-panel);
              box-shadow: var(--shadow-panel);
              color: var(--color-danger);
              font: inherit;
              font-size: 13px;
              white-space: nowrap;
              cursor: pointer;
              &:disabled { opacity: 0.45; cursor: default; }
            }
          }
        }
        .assistant-thread-empty { margin: 18px 8px; color: var(--color-text-muted); font-size: 13px; }
      }
      .assistant-sidebar-foot {
        display: flex;
        justify-content: space-between;
        gap: 12px;
        flex: none;
        padding: 12px 18px;
        border-top: 1px solid var(--assistant-line);
      }
    }
    .assistant-workspace {
      display: flex;
      flex: 1;
      min-width: 0;
      min-height: 0;
      flex-direction: column;
      .assistant-messages {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 24px 26px;
        .assistant-earlier { display: flex; margin: 0 auto 24px; }
        .assistant-welcome {
          display: flex;
          max-width: 580px;
          flex-direction: column;
          align-items: center;
          margin: 8vh auto 24px;
          text-align: center;
          .assistant-welcome-icon {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 64px;
            height: 64px;
            border: 1px solid var(--assistant-line);
            border-radius: 21px;
            background: var(--assistant-wash);
            color: var(--assistant-accent);
          }
          .assistant-welcome-title { margin: 18px 0 8px; font-size: 23px; }
          .assistant-welcome-description { margin: 0; color: var(--color-text-muted); font-size: 14px; line-height: 1.8; }
          .assistant-examples {
            display: flex;
            width: 100%;
            flex-direction: column;
            gap: 10px;
            margin-top: 24px;
            .assistant-example {
              display: flex;
              align-items: center;
              justify-content: space-between;
              gap: 16px;
              padding: 13px 16px;
              border: 1px solid var(--assistant-line);
              border-radius: 12px;
              background: var(--color-panel);
              color: var(--color-text);
              font: inherit;
              font-size: 14px;
              cursor: pointer;
              text-align: left;
              &:hover { border-color: var(--assistant-accent); background: var(--assistant-wash); }
            }
          }
        }
        .assistant-message {
          display: flex;
          align-items: flex-start;
          gap: 20px;
          margin-bottom: 56px;
          padding-left: 56px;
          .assistant-user-avatar {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 44px;
            height: 44px;
            flex: none;
            margin-top: 10px;
            border-radius: 50%;
            background: var(--color-primary-soft);
            color: var(--color-text-muted);
          }
          .assistant-message-main {
            flex: 1;
            min-width: 0;
            .assistant-message-header {
              display: flex;
              align-items: center;
              gap: 12px;
              margin-bottom: 10px;
              .assistant-message-label { font-size: 17px; }
              .assistant-message-date { margin-left: auto; color: var(--color-text-muted); font-size: 14px; }
              .assistant-message-status {
                display: inline-flex;
                align-items: center;
                gap: 5px;
                padding: 3px 8px;
                border-radius: 14px;
                background: var(--color-primary-soft);
                color: var(--color-text-muted);
                font-size: 12px;
                &.assistant-message-status-completed { background: var(--color-success-soft); color: var(--color-success); }
                &.assistant-message-status-failed { background: var(--color-danger-soft); color: var(--color-danger); }
                .assistant-loading-icon { animation: assistant-spin 1s linear infinite; }
              }
            }
            .assistant-message-body {
              padding: 14px 17px;
              border: 1px solid var(--assistant-line);
              border-radius: 18px;
              background: color-mix(in srgb, var(--assistant-accent) 2%, var(--color-panel));
              .assistant-message-text { white-space: pre-wrap; overflow-wrap: anywhere; font-size: 18px; line-height: 1.8; }
              .assistant-tool {
                margin-top: 10px;
                border: 1px solid var(--assistant-line);
                border-radius: 10px;
                .assistant-tool-summary {
                  display: flex;
                  align-items: center;
                  gap: 8px;
                  padding: 10px 12px;
                  color: var(--color-text-muted);
                  font-size: 12px;
                  cursor: pointer;
                }
                .assistant-tool-detail {
                  max-height: 240px;
                  margin: 0;
                  overflow: auto;
                  padding: 12px;
                  border-top: 1px solid var(--assistant-line);
                  white-space: pre-wrap;
                  overflow-wrap: anywhere;
                  font-size: 12px;
                }
              }
              .assistant-pending { color: var(--color-text-muted); font-size: 14px; }
              .assistant-message-actions {
                display: flex;
                gap: 10px;
                margin-top: 12px;
                .assistant-icon-button { width: 38px; height: 38px; border-radius: 12px; }
                .assistant-rating-active { border-color: var(--assistant-accent); background: var(--assistant-wash); color: var(--assistant-accent); }
              }
            }
          }
          &.assistant-message-user {
            padding-left: 0;
            .assistant-message-main {
              padding: 14px 18px;
              border-radius: 18px;
              background: color-mix(in srgb, var(--assistant-accent) 8%, var(--color-panel));
              .assistant-message-header { margin-bottom: 4px; }
              .assistant-message-body { padding: 0; border: 0; background: transparent; }
            }
          }
        }
      }
      .assistant-composer-wrap {
        flex: none;
        padding: 0 20px 28px;
        .assistant-hint { margin: 0 0 10px; color: var(--color-text-muted); font-size: 12px; line-height: 1.6; }
        .assistant-composer {
          padding: 12px;
          border: 1px solid var(--assistant-line);
          border-radius: 20px;
          background: var(--color-panel);
          box-shadow: 0 2px 3px rgba(30, 70, 120, 0.035);
          &:focus-within { border-color: color-mix(in srgb, var(--assistant-accent) 45%, var(--assistant-line)); }
          .assistant-input {
            display: block;
            width: 100%;
            height: 52px;
            margin: 0 0 12px;
            padding: 4px 6px;
            box-sizing: border-box;
            resize: none;
            border: 0;
            outline: none;
            background: transparent;
            color: var(--color-text);
            font: inherit;
            font-size: 18px;
            line-height: 1.6;
            &::placeholder { color: var(--color-text-muted); }
          }
          .assistant-composer-foot {
            display: flex;
            align-items: center;
            gap: 18px;
            .assistant-composer-tools {
              display: flex;
              gap: 10px;
              .assistant-file-input { display: none; }
            }
            .assistant-keyboard-hint {
              min-width: 0;
              margin-left: auto;
              color: var(--color-text-muted);
              font-size: 13px;
              .assistant-keyboard-divider { margin: 0 8px; color: var(--color-text-soft); }
            }
            .assistant-send,
            .assistant-stop { min-width: 128px; min-height: 46px; border-radius: 18px; }
            .assistant-send { border-color: var(--assistant-accent); background: var(--assistant-accent); color: #fff; }
            .assistant-stop { border-color: var(--color-danger-line); background: var(--color-danger-soft); color: var(--color-danger); }
          }
        }
      }
      .assistant-error { margin: 8px 0; color: var(--color-danger); overflow-wrap: anywhere; font-size: 12px; line-height: 1.6; }
    }
  }
}

@keyframes assistant-spin {
  to { transform: rotate(360deg); }
}
</style>

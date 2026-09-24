<template>
  <section
    ref="panelRef"
    class="lan-chat"
    :class="{ 'is-panel-dragover': isDragOver }"
    @dragenter="handleDragEnter"
    @dragover="handleDragOver"
    @dragleave="handleDragLeave"
    @drop="handleDrop"
  >
    <div ref="messageListRef" class="chat-timeline" aria-label="聊天消息">
      <div v-if="!displayMessages.length" class="chat-empty">
        <div class="cyber-empty-hud">
          <div class="cyber-empty-ring ring-1"></div>
          <div class="cyber-empty-ring ring-2"></div>
          <MessagesSquare :size="32" class="cyber-empty-icon" />
        </div>
        <span class="cyber-empty-title">WAITING FOR TRANSMISSION</span>
        <small class="chat-empty-hint"
          >信道已建立。键入文字、拖拽图片或添加文件，直接启动局域网极速直连。</small
        >
      </div>

      <article
        v-for="message in displayMessages"
        :key="message.id"
        :data-message-id="message.id"
        class="chat-message"
        :class="{
          'chat-message-self': message.direction === 'desktop-to-mobile',
          'chat-message-pending': message.isPending,
          'chat-message-error': message.status === 'error'
        }"
      >
        <div class="chat-message-meta">
          <span class="chat-sender-name">{{
            message.direction === "desktop-to-mobile"
              ? "本机节点"
              : message.deviceName || currentDevice?.name || "目标节点"
          }}</span>
          <time class="chat-timestamp">{{
            formatDateTime(message.createdAt)
          }}</time>

          <!-- 如果是后台正在发送的任务 -->
          <template v-if="message.isPending">
            <div
              v-if="message.status === 'error'"
              class="chat-task-status chat-task-status--error"
            >
              <AlertCircle :size="12" />
              <span class="chat-task-status-text">发送失败</span>
              <button
                class="chat-task-action-btn"
                type="button"
                title="重新发送"
                @click="retryTask(message)"
              >
                <RotateCw :size="10" />
                <span>重试</span>
              </button>
              <button
                class="chat-task-action-btn"
                type="button"
                title="取消并移除"
                @click="cancelTask(message.id)"
              >
                <X :size="10" />
              </button>
            </div>
            <div v-else class="chat-task-status chat-task-status--sending">
              <LoaderCircle :size="12" class="cyber-spin" />
              <span class="chat-task-status-text">{{
                message.statusText || "正在发送..."
              }}</span>
              <span
                v-if="message.progress > 0 && message.progress < 100"
                class="chat-task-percent"
              >
                {{ Math.round(message.progress) }}%
              </span>
            </div>
          </template>

          <!-- 离线/待接收状态指示与重试操作 (服务端已存储消息) -->
          <template v-else>
            <div
              v-if="
                message.direction === 'desktop-to-mobile' && !message.delivered
              "
              class="chat-offline-badge"
            >
              <span class="cyber-pulse-dot cyber-pulse-dot--amber"></span>
              <span class="chat-delivery-text">待对方接收 (离线)</span>
              <button
                class="chat-retry-btn"
                type="button"
                :disabled="retryingMessageId === message.id"
                title="立即向对方重试发送"
                @click="retryMessage(message.id)"
              >
                <RotateCw
                  :size="11"
                  :class="{ 'cyber-spin': retryingMessageId === message.id }"
                />
                <span>{{
                  retryingMessageId === message.id ? "重试中" : "重试"
                }}</span>
              </button>
            </div>
            <span
              v-else-if="message.direction === 'desktop-to-mobile'"
              class="chat-delivered-badge"
            >
              <span class="cyber-dot-emerald"></span>
              <span>已送达</span>
            </span>
          </template>
        </div>

        <div class="chat-message-body">
          <div
            class="chat-bubble"
            :class="{
              'chat-bubble-attachments':
                !message.content?.trim() &&
                (message.attachments?.length || message.files?.length),
              'chat-bubble-pending': message.isPending
            }"
          >
            <!-- 文本内容 -->
            <p v-if="message.content?.trim()" class="chat-text">
              {{ message.content }}
            </p>

            <!-- 服务端已确认存储的附件 -->
            <LanShareAttachmentGallery
              v-if="!message.isPending && message.attachments?.length"
              :files="message.attachments"
              :service="service"
              :session-id="currentSessionId"
              @preview="$emit('preview-file', $event)"
              @download="$emit('download-file', $event)"
            />

            <!-- 后台正在发送的文件与进度卡片 -->
            <div
              v-if="message.isPending && message.files?.length"
              class="chat-sending-attachments"
            >
              <div class="chat-sending-files-list">
                <div
                  v-for="file in message.files"
                  :key="file.id"
                  class="chat-sending-file-item"
                >
                  <div class="sending-file-preview">
                    <img
                      v-if="file.previewUrl"
                      :src="file.previewUrl"
                      :alt="file.name"
                      class="sending-file-thumb"
                    />
                    <div v-else class="sending-file-icon">
                      <File :size="18" />
                    </div>
                  </div>
                  <div class="sending-file-meta">
                    <span class="sending-file-name" :title="file.name">{{
                      file.name
                    }}</span>
                    <span class="sending-file-size">{{
                      formatFileSize(file.size)
                    }}</span>
                  </div>
                  <div class="sending-file-state">
                    <span
                      v-if="file.uploadedId || file.path"
                      class="state-done"
                      title="已准备完成"
                    >
                      <Check :size="13" />
                    </span>
                    <span
                      v-else-if="message.status === 'error'"
                      class="state-error"
                      title="失败"
                    >
                      <AlertCircle :size="13" />
                    </span>
                    <span v-else class="state-uploading" title="传输中">
                      <LoaderCircle :size="13" class="cyber-spin" />
                    </span>
                  </div>
                </div>
              </div>

              <!-- 传输进度条 -->
              <div class="chat-sending-progress-shell">
                <div class="progress-bar-track">
                  <div
                    class="progress-bar-fill"
                    :class="{
                      'is-error': message.status === 'error',
                      'is-indeterminate':
                        message.status !== 'error' && message.progress === 0
                    }"
                    :style="{
                      width: `${Math.max(4, Math.min(100, message.progress))}%`
                    }"
                  ></div>
                </div>
                <div class="progress-info-row">
                  <span class="progress-status-desc">
                    {{
                      message.status === "error"
                        ? message.errorMessage || "发送失败"
                        : message.statusText || "正在发送附件..."
                    }}
                  </span>
                  <span class="progress-ratio">
                    <template
                      v-if="message.totalBytes > 0 && message.loadedBytes > 0"
                    >
                      {{ formatFileSize(message.loadedBytes) }} /
                      {{ formatFileSize(message.totalBytes) }} ({{
                        Math.round(message.progress)
                      }}%)
                    </template>
                    <template v-else-if="message.progress > 0">
                      {{ Math.round(message.progress) }}%
                    </template>
                  </span>
                </div>
              </div>
            </div>
          </div>

          <!-- 消息操作按钮 -->
          <div v-if="!message.isPending" class="chat-message-actions">
            <button
              v-if="message.content"
              class="chat-icon-button"
              type="button"
              title="复制消息"
              aria-label="复制消息"
              @click="copyMessage(message)"
            >
              <Copy :size="12" />
            </button>
            <button
              class="chat-icon-button"
              type="button"
              title="删除本机消息"
              aria-label="删除本机消息"
              @click="deleteMessages([message.id])"
            >
              <Trash2 :size="12" />
            </button>
          </div>
          <div v-else class="chat-message-actions">
            <button
              v-if="message.status === 'error'"
              class="chat-icon-button"
              type="button"
              title="重新发送"
              aria-label="重新发送"
              @click="retryTask(message)"
            >
              <RotateCw :size="12" />
            </button>
            <button
              class="chat-icon-button"
              type="button"
              title="取消发送"
              aria-label="取消发送"
              @click="cancelTask(message.id)"
            >
              <Trash2 :size="12" />
            </button>
          </div>
        </div>
      </article>
    </div>

    <!-- 科技感底部输入区 -->
    <footer class="chat-composer" :aria-busy="sending">
      <div
        v-if="currentDraft.files.length"
        class="chat-draft-files"
        aria-label="待发送附件"
      >
        <div
          v-for="item in currentDraft.files"
          :key="item.id"
          class="chat-draft-file"
        >
          <img
            v-if="item.previewUrl"
            class="chat-draft-thumbnail"
            :src="item.previewUrl"
            :alt="item.name"
          />
          <File v-else class="chat-draft-icon" :size="20" />
          <span class="chat-draft-caption">
            <span class="chat-draft-name" :title="item.name">{{
              item.name
            }}</span>
            <small class="chat-draft-size">{{
              formatFileSize(item.size)
            }}</small>
          </span>
          <button
            class="chat-icon-button"
            type="button"
            :aria-label="`移除 ${item.name}`"
            @click="removeAttachment(item)"
          >
            <X :size="13" />
          </button>
        </div>
      </div>

      <div
        ref="inputWrapperRef"
        class="chat-input-wrapper"
        :class="{ 'is-dragover': isDragOver }"
      >
        <div v-if="isDragOver" class="chat-drag-overlay">
          <CloudUpload :size="22" class="cyber-bounce-icon" />
          <span class="chat-drag-text">释放以添加文件或图片</span>
        </div>
        <textarea
          ref="composerRef"
          v-model="currentDraft.content"
          class="chat-input"
          :disabled="!currentSessionId"
          rows="2"
          placeholder="输入消息内容，或直接拖拽/粘贴文件、图片至此..."
          aria-label="聊天输入框"
          @paste="pasteFiles"
          @keydown="composerKeydown"
        ></textarea>
      </div>

      <div class="chat-composer-footer">
        <div class="chat-compose-tools">
          <button
            class="chat-attach-button"
            type="button"
            :disabled="!currentSessionId"
            title="添加文件或图片，可多选"
            @click="pickFiles"
          >
            <Paperclip :size="15" />
            <span>添加附件</span>
          </button>
          <span class="chat-compose-hint">{{
            activeSendingCount > 0
              ? `后台正在发送 ${activeSendingCount} 条消息...`
              : currentDraft.files.length
                ? `${currentDraft.files.length} 个附件 · 合并发送`
                : "Enter 发送 • Shift + Enter 换行"
          }}</span>
        </div>
        <button
          class="chat-send-button"
          type="button"
          :disabled="
            !service.running ||
            !currentSessionId ||
            (!currentDraft.content.trim() && !currentDraft.files.length)
          "
          @click="sendMessage"
        >
          <Send :size="14" />
          <span>发送</span>
        </button>
      </div>
      <input
        ref="fileInputRef"
        class="chat-file-input"
        type="file"
        multiple
        aria-label="选择附件"
        @change="chooseBrowserFiles"
      />
    </footer>

    <!-- 侧边记录搜索抽屉 -->
    <el-drawer
      v-model="searchOpen"
      title="聊天记录搜索"
      size="420px"
      append-to-body
      class="cyber-drawer"
    >
      <div class="chat-search-drawer">
        <el-input
          v-model="keyword"
          placeholder="搜索文字或附件名称"
          clearable
        />
        <el-select v-model="timeFilter" aria-label="消息时间范围">
          <el-option label="全部时间" value="all" />
          <el-option label="今天" value="today" />
          <el-option label="最近 7 天" value="week" />
        </el-select>
        <div class="chat-search-actions">
          <span>匹配 {{ filteredMessages.length }} 条记录</span>
          <div class="chat-search-btns">
            <el-button size="small" @click="selectAll">
              {{ allSelected ? "取消全选" : "全选" }}
            </el-button>
            <el-button
              size="small"
              type="danger"
              plain
              :disabled="!selectedIds.length"
              @click="deleteMessages(selectedIds)"
            >
              删除所选
            </el-button>
          </div>
        </div>
        <div class="chat-search-results">
          <div
            v-for="message in filteredMessages"
            :key="message.id"
            class="chat-search-result"
          >
            <el-checkbox
              v-model="selectedIds"
              :value="message.id"
              :aria-label="`选择 ${message.content || '附件消息'}`"
            />
            <button
              class="chat-search-jump"
              type="button"
              @click="jumpToMessage(message.id)"
            >
              <small class="chat-search-time">{{
                formatDateTime(message.createdAt)
              }}</small>
              <span>{{
                message.content ||
                message.attachments?.map((file) => file.name).join("、") ||
                "文件消息"
              }}</span>
            </button>
          </div>
          <span v-if="!filteredMessages.length" class="chat-search-empty">
            没有匹配的聊天记录
          </span>
        </div>
      </div>
    </el-drawer>
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
import {
  ElButton,
  ElCheckbox,
  ElDrawer,
  ElInput,
  ElOption,
  ElSelect
} from "element-plus"
import "element-plus/es/components/button/style/css"
import "element-plus/es/components/checkbox/style/css"
import "element-plus/es/components/drawer/style/css"
import "element-plus/es/components/input/style/css"
import "element-plus/es/components/select/style/css"
import { isTauri } from "@tauri-apps/api/core"
import { getCurrentWebview } from "@tauri-apps/api/webview"
import {
  AlertCircle,
  Check,
  CloudUpload,
  Copy,
  File,
  LoaderCircle,
  MessagesSquare,
  Paperclip,
  RotateCw,
  Send,
  Trash2,
  X
} from "lucide-vue-next"
import { lanShareApi, systemApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"
import { formatFileSize } from "@/features/lanShare/utils"
import LanShareAttachmentGallery from "./LanShareAttachmentGallery.vue"

const props = defineProps({
  currentDevice: { type: Object, default: null },
  currentSessionId: { type: String, default: "" },
  currentSession: { type: Object, default: null },
  chatMode: { type: String, default: "direct" },
  service: { type: Object, default: () => ({}) },
  stateVersion: { type: Number, default: 0 }
})
const emit = defineEmits(["refresh-state", "preview-file", "download-file"])

const panelRef = ref(null)
const messageListRef = ref(null)
const composerRef = ref(null)
const inputWrapperRef = ref(null)
const fileInputRef = ref(null)
const isDragOver = ref(false)
let dragDepth = 0
const messages = ref([])
const drafts = reactive({})
const sendingTasks = ref([])
const sendStatus = ref("")
const retryingMessageId = ref("")
const searchOpen = ref(false)
const keyword = ref("")
const timeFilter = ref("all")
const selectedIds = ref([])
let stopMessageListener = null
let stopDropListener = null
let disposed = false
let loadSeed = 0

const activeSendingCount = computed(
  () =>
    sendingTasks.value.filter(
      (task) =>
        task.sessionId === props.currentSessionId &&
        (task.status === "uploading" || task.status === "sending")
    ).length
)
const sending = computed(() => activeSendingCount.value > 0)

const displayMessages = computed(() => {
  const serverList = messages.value
  const serverIds = new Set(serverList.map((m) => m.id))
  const pendingTasks = sendingTasks.value.filter(
    (task) =>
      task.sessionId === props.currentSessionId && !serverIds.has(task.id)
  )
  return [...serverList, ...pendingTasks].sort(
    (left, right) => left.createdAt - right.createdAt
  )
})

watch(
  () => props.currentSessionId,
  (sessionId) => {
    if (!drafts[sessionId])
      drafts[sessionId] = { content: "", files: [], messageId: "" }
    messages.value = []
    selectedIds.value = []
    loadMessages()
  },
  { immediate: true }
)

const currentDraft = computed(
  () =>
    drafts[props.currentSessionId] || { content: "", files: [], messageId: "" }
)

const filteredMessages = computed(() => {
  const since =
    timeFilter.value === "today"
      ? new Date().setHours(0, 0, 0, 0)
      : timeFilter.value === "week"
        ? Date.now() - 7 * 86400000
        : 0
  const query = keyword.value.trim().toLowerCase()
  return messages.value.filter(
    (message) =>
      message.createdAt >= since &&
      `${message.content || ""} ${(message.attachments || []).map((file) => file.name).join(" ")}`
        .toLowerCase()
        .includes(query)
  )
})

const allSelected = computed(
  () =>
    filteredMessages.value.length > 0 &&
    filteredMessages.value.every((message) =>
      selectedIds.value.includes(message.id)
    )
)

watch(() => props.stateVersion, loadMessages)

onMounted(async () => {
  stopMessageListener = lanShareApi.onMessageCreated((message) => {
    if (
      message.sessionId === props.currentSessionId ||
      props.chatMode === "group"
    )
      loadMessages()
  })
  if (isTauri()) {
    try {
      const unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload
        if (payload.type === "leave") {
          isDragOver.value = false
          return
        }

        const position = payload.position
        const scale = window.devicePixelRatio || 1
        const x = position.x / scale
        const y = position.y / scale

        const bounds = panelRef.value?.getBoundingClientRect()
        const isOverPanel =
          bounds &&
          x >= bounds.left &&
          x <= bounds.right &&
          y >= bounds.top &&
          y <= bounds.bottom

        if (payload.type === "enter" || payload.type === "over") {
          isDragOver.value = Boolean(isOverPanel)
        } else if (payload.type === "drop") {
          isDragOver.value = false
          if (isOverPanel) {
            addPaths(payload.paths)
          }
        }
      })
      if (disposed) unlisten()
      else stopDropListener = unlisten
    } catch (error) {
      createMessage.error(`拖拽监听失败：${error?.message || error}`)
    }
  }
})

onBeforeUnmount(() => {
  disposed = true
  loadSeed++
  stopMessageListener?.()
  stopDropListener?.()
  for (const draft of Object.values(drafts)) {
    for (const file of draft.files)
      if (file.previewUrl) URL.revokeObjectURL(file.previewUrl)
    discardUploads(draft.files)
  }
  for (const task of sendingTasks.value) {
    if (task.activeXhr) {
      try {
        task.activeXhr.abort()
      } catch {}
    }
    for (const file of task.files) {
      if (file.previewUrl) URL.revokeObjectURL(file.previewUrl)
    }
  }
})

function unwrap(result) {
  return result?.status && "data" in result ? result.data : result
}

async function loadMessages() {
  const seed = ++loadSeed
  if (!props.currentSessionId) return
  const list = messageListRef.value
  const shouldFollow =
    !messages.value.length ||
    !list ||
    list.scrollHeight - list.scrollTop - list.clientHeight < 80
  try {
    const result = unwrap(
      await lanShareApi.listMessages({
        sessionId: props.currentSessionId,
        deviceId:
          props.chatMode === "direct" ? props.currentDevice?.id || "" : ""
      })
    )
    if (disposed || seed !== loadSeed) return
    const nextMessages = (Array.isArray(result) ? result : []).sort(
      (left, right) => left.createdAt - right.createdAt
    )
    const changed = nextMessages.at(-1)?.id !== messages.value.at(-1)?.id
    messages.value = nextMessages
    selectedIds.value = selectedIds.value.filter((id) =>
      nextMessages.some((message) => message.id === id)
    )

    // 清理并在服务端已确认的消息中对齐后台发送任务
    const serverIdSet = new Set(nextMessages.map((m) => m.id))
    sendingTasks.value = sendingTasks.value.filter((task) => {
      if (serverIdSet.has(task.id)) {
        for (const item of task.files) {
          if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
        }
        return false
      }
      return true
    })

    if (changed && shouldFollow) {
      await nextTick()
      if (messageListRef.value)
        messageListRef.value.scrollTop = messageListRef.value.scrollHeight
    }
  } catch (error) {
    if (seed === loadSeed && !disposed)
      createMessage.error(error?.message || String(error))
  }
}

async function retryMessage(messageId) {
  if (retryingMessageId.value) return
  retryingMessageId.value = messageId
  try {
    const result = await lanShareApi.retryMessage({ messageId })
    const data = unwrap(result)
    if (data?.delivered) {
      createMessage.success("消息已成功送达对方")
    } else {
      createMessage.info("已尝试重新投递，对方目前仍处于离线状态")
    }
    await loadMessages()
    emit("refresh-state")
  } catch (err) {
    createMessage.error(err?.message || "重试发送失败")
  } finally {
    retryingMessageId.value = ""
  }
}

function appendFiles(files) {
  if (!props.currentSessionId) return
  const remaining = 100 - currentDraft.value.files.length
  if (files.length > remaining)
    createMessage.warning("一条消息最多添加 100 个附件。")
  for (const file of files.slice(0, remaining)) {
    if (file.size > 10 * 1024 ** 3) {
      createMessage.warning(`${file.name} 超过单文件 10 GiB 限制。`)
      continue
    }
    currentDraft.value.files.push({
      id: crypto.randomUUID(),
      name: file.name || `剪贴板-${Date.now()}.png`,
      size: file.size,
      file,
      previewUrl: file.type.startsWith("image/")
        ? URL.createObjectURL(file)
        : ""
    })
  }
}

function addPaths(paths) {
  if (!props.currentSessionId) return
  for (const path of paths) {
    if (currentDraft.value.files.some((item) => item.path === path)) continue
    if (currentDraft.value.files.length >= 100) {
      createMessage.warning("一条消息最多添加 100 个附件。")
      break
    }
    currentDraft.value.files.push({
      id: crypto.randomUUID(),
      path,
      name: path.split(/[/\\]/).pop() || path,
      size: 0
    })
  }
}

function chooseBrowserFiles(event) {
  appendFiles(Array.from(event.target.files || []))
  event.target.value = ""
}

async function pickFiles() {
  if (!props.currentSessionId) return
  if (isTauri()) {
    try {
      const selected = await systemApi.selectFiles({
        title: "选择快传文件",
        multiple: true
      })
      if (Array.isArray(selected)) addPaths(selected)
      else if (selected) addPaths([selected])
    } catch (error) {
      createMessage.error(`选择文件失败：${error?.message || error}`)
    }
    return
  }
  fileInputRef.value?.click()
}

function pasteFiles(event) {
  const items = Array.from(event.clipboardData?.items || [])
  const files = items
    .filter((item) => item.kind === "file")
    .map((item) => item.getAsFile())
    .filter(Boolean)
  if (files.length) {
    event.preventDefault()
    appendFiles(files)
  }
}

function handleDragEnter(event) {
  event.preventDefault()
  dragDepth++
  isDragOver.value = true
}

function handleDragOver(event) {
  event.preventDefault()
  if (!isDragOver.value) isDragOver.value = true
}

function handleDragLeave(event) {
  event.preventDefault()
  dragDepth--
  if (dragDepth <= 0) {
    dragDepth = 0
    isDragOver.value = false
  }
}

function handleDrop(event) {
  event.preventDefault()
  dragDepth = 0
  isDragOver.value = false
  dropFiles(event)
}

function dropFiles(event) {
  const files = Array.from(event.dataTransfer?.files || [])
  if (files.length) appendFiles(files)
}

function composerKeydown(event) {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault()
    sendMessage()
  }
}

async function discardUploads(files) {
  const ids = files.map((file) => file.uploadedId).filter(Boolean)
  if (!ids.length || !props.service.running) return
  try {
    await lanShareApi.discardUploads({ attachmentIds: ids })
  } catch (error) {
    console.error("discardUploads error:", error)
  }
}

function removeAttachment(item) {
  currentDraft.value.files = currentDraft.value.files.filter(
    (file) => file.id !== item.id
  )
  if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
  discardUploads([item])
}

function scrollToBottom() {
  nextTick(() => {
    if (messageListRef.value) {
      messageListRef.value.scrollTop = messageListRef.value.scrollHeight
    }
  })
}

function uploadFileWithProgress(url, file, onProgress, onXhrCreated) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest()
    onXhrCreated?.(xhr)
    xhr.open("PUT", url, true)

    if (xhr.upload && onProgress) {
      xhr.upload.onprogress = (event) => {
        if (event.lengthComputable) {
          onProgress(event.loaded, event.total)
        }
      }
    }

    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try {
          const payload = JSON.parse(xhr.responseText)
          if (payload.status === "success") {
            resolve(payload.data)
          } else {
            reject(new Error(payload.message || "上传失败"))
          }
        } catch {
          reject(new Error("解析上传响应失败"))
        }
      } else {
        try {
          const payload = JSON.parse(xhr.responseText)
          reject(new Error(payload.message || `HTTP ${xhr.status}`))
        } catch {
          reject(new Error(`上传失败 (HTTP ${xhr.status})`))
        }
      }
    }

    xhr.onerror = () => {
      reject(new Error("网络连接失败，上传中断"))
    }

    xhr.onabort = () => {
      reject(new Error("已取消上传"))
    }

    xhr.ontimeout = () => {
      reject(new Error("上传请求超时"))
    }

    xhr.send(file)
  })
}

async function executeSendingTask(task) {
  task.status = "uploading"
  task.errorMessage = ""
  task.progress = 0

  try {
    const filesNeedingUpload = task.files.filter(
      (item) => !item.path && !item.uploadedId && item.file
    )

    if (filesNeedingUpload.length > 0) {
      if (!props.service.accessUrl) {
        throw new Error("局域网服务未就绪，缺少接入地址")
      }
      const access = new URL(props.service.accessUrl)
      const fileProgressMap = new Map()

      for (const item of task.files) {
        if (item.path || item.uploadedId) {
          fileProgressMap.set(item.id, item.size || 0)
        } else {
          fileProgressMap.set(item.id, 0)
        }
      }

      for (let i = 0; i < filesNeedingUpload.length; i++) {
        const item = filesNeedingUpload[i]
        task.statusText = `正在上传附件 (${i + 1}/${filesNeedingUpload.length})...`

        const url = new URL("/api/files/upload", access.origin)
        url.search = new URLSearchParams({
          token: access.searchParams.get("token") || "",
          sessionId: task.sessionId,
          name: item.name
        }).toString()

        const uploaded = await uploadFileWithProgress(
          url.toString(),
          item.file,
          (loaded) => {
            fileProgressMap.set(item.id, loaded)
            const overallLoaded = Array.from(fileProgressMap.values()).reduce(
              (sum, b) => sum + b,
              0
            )
            task.loadedBytes = overallLoaded
            if (task.totalBytes > 0) {
              task.progress = Math.min(
                88,
                Math.round((overallLoaded / task.totalBytes) * 88)
              )
            }
          },
          (xhr) => {
            task.activeXhr = xhr
          }
        )

        task.activeXhr = null
        item.uploadedId = uploaded.id
        fileProgressMap.set(item.id, item.size || 0)
      }
    }

    // 所有待上传文件处理完成，发送消息与本地路径至后端
    task.status = "sending"
    task.statusText = task.files.length
      ? "正在投递文件与消息..."
      : "正在发送..."
    task.progress = Math.max(task.progress, 90)

    await lanShareApi.sendMessage({
      sessionId: task.sessionId,
      deviceId: task.deviceId,
      messageId: task.id,
      content: task.content,
      paths: task.files.map((item) => item.path).filter(Boolean),
      attachmentIds: task.files.map((item) => item.uploadedId).filter(Boolean),
      attachmentOrder: task.files.map((item) =>
        item.path ? { path: item.path } : { id: item.uploadedId }
      )
    })

    task.status = "success"
    task.progress = 100
    task.statusText = "已送达"

    if (!disposed) {
      emit("refresh-state")
      await loadMessages()
      // 如果服务端已刷新包含此消息，从临时队列移除
      const foundInServer = messages.value.some((m) => m.id === task.id)
      if (foundInServer) {
        for (const item of task.files) {
          if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
        }
        sendingTasks.value = sendingTasks.value.filter((t) => t.id !== task.id)
      }
    }
  } catch (error) {
    if (disposed) return
    task.status = "error"
    task.errorMessage = error?.message || String(error)
    task.statusText = "发送失败"
    createMessage.error(`发送失败：${task.errorMessage}`)
  } finally {
    task.activeXhr = null
  }
}

function sendMessage() {
  if (!props.service.running || !props.currentSessionId) return
  const draft = currentDraft.value
  const content = draft.content.trim()
  const files = [...draft.files]
  if (!content && !files.length) return

  // 立即清空输入框和草稿，后台异步发送，完全不阻碍用户继续输入
  draft.content = ""
  draft.files = []
  draft.messageId = ""
  draft.fingerprint = ""

  // 保持输入框焦点，用户可立即键入下一条内容
  nextTick(() => {
    composerRef.value?.focus()
  })

  // 创建乐观后台发送任务并在时间轴中立即展示
  const taskId = crypto.randomUUID()
  const totalBytes = files.reduce((acc, f) => acc + (f.size || 0), 0)
  const task = reactive({
    id: taskId,
    sessionId: props.currentSessionId,
    deviceId: props.chatMode === "direct" ? props.currentDevice?.id || "" : "",
    direction: "desktop-to-mobile",
    content,
    createdAt: Date.now(),
    files,
    isPending: true,
    status: "uploading",
    progress: 0,
    loadedBytes: 0,
    totalBytes,
    statusText: files.length
      ? `准备发送 ${files.length} 个文件...`
      : "正在发送...",
    errorMessage: "",
    activeXhr: null
  })

  sendingTasks.value.push(task)
  scrollToBottom()
  executeSendingTask(task)
}

function retryTask(task) {
  task.status = "uploading"
  task.errorMessage = ""
  task.progress = 0
  task.statusText = "正在重新发送..."
  executeSendingTask(task)
}

function cancelTask(taskId) {
  const task = sendingTasks.value.find((t) => t.id === taskId)
  if (!task) return
  if (task.activeXhr) {
    try {
      task.activeXhr.abort()
    } catch (e) {
      console.warn("abort xhr error:", e)
    }
  }
  for (const item of task.files) {
    if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
  }
  discardUploads(task.files)
  sendingTasks.value = sendingTasks.value.filter((t) => t.id !== taskId)
}

async function copyMessage(message) {
  try {
    await navigator.clipboard.writeText(message.content || "")
    createMessage.success("已复制消息。")
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

async function deleteMessages(ids) {
  if (
    !ids.length ||
    !window.confirm(`仅删除本机的 ${ids.length} 条消息，是否继续？`)
  )
    return
  try {
    await lanShareApi.deleteMessages({ messageIds: [...ids] })
    selectedIds.value = []
    await loadMessages()
    emit("refresh-state")
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

async function clearCurrentSession() {
  if (
    !props.currentSessionId ||
    !window.confirm("清空当前会话的本机聊天记录？")
  )
    return
  try {
    await lanShareApi.clearSession({ sessionId: props.currentSessionId })
    for (const task of sendingTasks.value.filter(
      (t) => t.sessionId === props.currentSessionId
    )) {
      cancelTask(task.id)
    }
    await loadMessages()
    emit("refresh-state")
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

function selectAll() {
  selectedIds.value = allSelected.value
    ? []
    : filteredMessages.value.map((message) => message.id)
}

async function jumpToMessage(id) {
  searchOpen.value = false
  await nextTick()
  const target = [
    ...(messageListRef.value?.querySelectorAll("[data-message-id]") || [])
  ].find((element) => element.dataset.messageId === id)
  target?.scrollIntoView({ block: "center" })
}

defineExpose({
  openSearch: () => {
    searchOpen.value = true
  },
  clearCurrentSession
})
</script>

<style scoped lang="less">
.lan-chat {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  background: var(--color-panel);

  .chat-timeline {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 20px;
    padding: 24px;
    overflow-y: auto;
    background: var(--color-panel);
    scrollbar-width: thin;
    scrollbar-color: var(--color-line) transparent;

    .chat-empty {
      display: flex;
      min-height: 200px;
      flex: 1;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 14px;
      color: var(--color-text-muted);
      text-align: center;

      .cyber-empty-hud {
        position: relative;
        width: 80px;
        height: 80px;
        display: flex;
        align-items: center;
        justify-content: center;

        .cyber-empty-icon {
          color: var(--color-primary);
          z-index: 1;
        }

        .cyber-empty-ring {
          position: absolute;
          border-radius: 50%;
          border: 1px solid var(--color-line);

          &.ring-1 {
            width: 58px;
            height: 58px;
            border-style: dashed;
            border-color: var(--color-primary);
            opacity: 0.55;
            animation: cyberSpin 18s linear infinite;
          }

          &.ring-2 {
            width: 78px;
            height: 78px;
            border-color: var(--color-line-strong);
          }
        }
      }

      .cyber-empty-title {
        font-family:
          ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        font-size: 11px;
        letter-spacing: 2px;
        color: var(--color-primary);
        font-weight: 600;
      }

      .chat-empty-hint {
        max-width: 380px;
        font-size: 13px;
        color: var(--color-text-muted);
        line-height: 1.6;
      }
    }

    .chat-message {
      position: relative;
      display: flex;
      max-width: min(85%, 620px);
      min-width: 0;
      flex: none;
      flex-direction: column;
      align-self: flex-start;
      gap: 6px;

      .chat-message-meta {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 8px;
        font-family:
          ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        font-size: 11px;
        color: var(--color-text-muted);

        .chat-sender-name {
          color: var(--color-text);
          font-weight: 500;
        }

        .chat-timestamp {
          color: var(--color-text-soft);
        }

        .chat-task-status {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 2px 7px;
          border-radius: 4px;
          font-size: 11px;
          line-height: 1.2;

          &--sending {
            background: var(--color-primary-soft);
            border: 1px solid var(--color-info-line);
            color: var(--color-primary);

            .chat-task-percent {
              font-weight: 700;
              margin-left: 2px;
            }
          }

          &--error {
            background: var(--color-danger-soft);
            border: 1px solid var(--color-danger-line);
            color: var(--color-danger);
          }

          .chat-task-status-text {
            max-width: 140px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }

          .chat-task-action-btn {
            display: inline-flex;
            align-items: center;
            gap: 2px;
            padding: 1px 5px;
            background: var(--color-panel);
            border: 1px solid currentColor;
            border-radius: 3px;
            color: inherit;
            cursor: pointer;
            font-size: 10px;
            line-height: 1.2;
            font-family: inherit;
            transition: all 0.15s ease;

            &:hover {
              opacity: 0.85;
              transform: scale(1.04);
            }
          }
        }

        .chat-offline-badge {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          padding: 2px 7px;
          background: var(--color-warning-soft);
          border: 1px solid var(--color-warning-line);
          border-radius: 4px;
          color: var(--color-warning);

          .chat-retry-btn {
            display: inline-flex;
            align-items: center;
            gap: 3px;
            padding: 1px 5px;
            background: var(--color-panel);
            border: 1px solid var(--color-warning-line);
            border-radius: 3px;
            color: var(--color-warning);
            cursor: pointer;
            font-size: 10px;
            font-family: inherit;
            transition: all 0.2s;

            &:hover:not(:disabled) {
              background: var(--color-warning-soft);
              border-color: var(--color-warning);
            }

            &:disabled {
              opacity: 0.6;
              cursor: not-allowed;
            }
          }
        }

        .chat-delivered-badge {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          color: var(--color-success);
          font-size: 10px;

          .cyber-dot-emerald {
            width: 5px;
            height: 5px;
            border-radius: 50%;
            background: var(--color-success);
            box-shadow: 0 0 5px var(--color-success);
          }
        }
      }

      .chat-message-body {
        display: flex;
        min-width: 0;
        align-items: flex-end;
        gap: 6px;

        .chat-bubble {
          display: flex;
          min-width: 0;
          flex: 1;
          flex-direction: column;
          gap: 10px;
          box-sizing: border-box;

          &.chat-bubble-attachments {
            flex: 0 1 auto;
          }

          &.chat-bubble-pending {
            opacity: 0.96;
          }

          &:not(.chat-bubble-attachments) {
            padding: 11px 14px;
            border: 1px solid var(--color-line);
            border-left: 3px solid var(--color-primary);
            border-radius: 0 10px 10px 10px;
            background: var(--color-panel);
            box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);

            &.chat-bubble-pending {
              border-style: dashed;
              border-left-style: solid;
            }

            .chat-text {
              margin: 0;
              padding: 0;
              color: var(--color-text);
              white-space: pre-wrap;
              overflow-wrap: anywhere;
              line-height: 1.65;
              font-size: 13.5px;
            }
          }

          .chat-sending-attachments {
            display: flex;
            flex-direction: column;
            gap: 8px;
            width: 320px;
            max-width: 100%;
            padding: 8px;
            border-radius: 8px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);

            .chat-sending-files-list {
              display: flex;
              flex-direction: column;
              gap: 6px;
              max-height: 220px;
              overflow-y: auto;
              scrollbar-width: thin;

              .chat-sending-file-item {
                display: flex;
                align-items: center;
                gap: 8px;
                padding: 6px 8px;
                border-radius: 6px;
                background: var(--color-panel);
                border: 1px solid var(--color-line);
                box-shadow: 0 1px 2px rgba(0, 0, 0, 0.02);

                .sending-file-preview {
                  width: 34px;
                  height: 34px;
                  flex-shrink: 0;
                  border-radius: 6px;
                  overflow: hidden;
                  background: var(--color-panel-soft);
                  display: grid;
                  place-items: center;
                  border: 1px solid var(--color-line);

                  .sending-file-thumb {
                    width: 100%;
                    height: 100%;
                    object-fit: cover;
                  }

                  .sending-file-icon {
                    display: grid;
                    place-items: center;
                    color: var(--color-primary);
                  }
                }

                .sending-file-meta {
                  display: flex;
                  flex-direction: column;
                  min-width: 0;
                  flex: 1;
                  gap: 2px;

                  .sending-file-name {
                    font-size: 12px;
                    font-weight: 500;
                    color: var(--color-text);
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                  }

                  .sending-file-size {
                    font-size: 11px;
                    color: var(--color-text-soft);
                    font-family:
                      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                      monospace;
                  }
                }

                .sending-file-state {
                  display: grid;
                  place-items: center;
                  flex-shrink: 0;
                  width: 20px;
                  height: 20px;

                  .state-done {
                    color: var(--color-success);
                  }

                  .state-error {
                    color: var(--color-danger);
                  }

                  .state-uploading {
                    color: var(--color-primary);
                  }
                }
              }
            }

            .chat-sending-progress-shell {
              display: flex;
              flex-direction: column;
              gap: 5px;
              padding: 4px 2px 2px;

              .progress-bar-track {
                position: relative;
                width: 100%;
                height: 5px;
                border-radius: 9999px;
                background: var(--color-line);
                overflow: hidden;

                .progress-bar-fill {
                  height: 100%;
                  border-radius: 9999px;
                  background: var(--color-primary);
                  transition: width 0.2s ease;

                  &.is-error {
                    background: var(--color-danger);
                  }

                  &.is-indeterminate {
                    position: absolute;
                    top: 0;
                    bottom: 0;
                    width: 35% !important;
                    animation: progressIndeterminate 1.4s infinite ease-in-out;
                  }
                }
              }

              .progress-info-row {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 8px;
                font-size: 11px;
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;

                .progress-status-desc {
                  color: var(--color-text-muted);
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                  font-size: 11px;
                }

                .progress-ratio {
                  flex-shrink: 0;
                  color: var(--color-primary);
                  font-weight: 600;
                  font-size: 11px;
                }
              }
            }
          }
        }

        .chat-message-actions {
          display: flex;
          flex: none;
          gap: 4px;
          opacity: 0;
          transition: opacity 0.15s;

          .chat-icon-button {
            display: grid;
            width: 24px;
            height: 24px;
            place-items: center;
            padding: 0;
            border: 1px solid var(--color-line);
            border-radius: 4px;
            background: var(--color-panel);
            color: var(--color-text-muted);
            cursor: pointer;
            transition: all 0.15s;

            &:hover {
              color: var(--color-primary);
              border-color: var(--color-primary);
              background: var(--color-primary-soft);
            }
          }
        }
      }

      &:hover .chat-message-actions,
      &:focus-within .chat-message-actions {
        opacity: 1;
      }

      &.chat-message-self {
        align-self: flex-end;

        .chat-message-meta {
          justify-content: flex-end;

          .chat-sender-name {
            color: var(--color-primary);
          }
        }

        .chat-message-body {
          flex-direction: row-reverse;

          .chat-bubble:not(.chat-bubble-attachments) {
            border: 1px solid var(--color-info-line);
            border-right: 3px solid var(--color-primary);
            border-left: 1px solid var(--color-info-line);
            border-radius: 10px 0 10px 10px;
            background: var(--color-primary-soft);
            box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);

            &.chat-bubble-pending {
              border-style: dashed;
              border-right-style: solid;
            }

            .chat-text {
              color: var(--color-text);
            }
          }
        }
      }
    }
  }

  .chat-composer {
    flex: none;
    min-width: 0;
    padding: 12px 18px 14px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);

    .chat-draft-files {
      display: flex;
      max-height: 140px;
      gap: 10px;
      padding-bottom: 12px;
      overflow-x: auto;
      scrollbar-width: thin;

      .chat-draft-file {
        display: flex;
        width: 200px;
        flex: none;
        align-items: center;
        gap: 8px;
        padding: 8px 10px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel-soft);

        .chat-draft-thumbnail {
          width: 36px;
          height: 36px;
          flex: none;
          object-fit: cover;
          border-radius: 4px;
          border: 1px solid var(--color-line);
        }

        .chat-draft-icon {
          flex: none;
          color: var(--color-primary);
        }

        .chat-draft-caption {
          display: flex;
          min-width: 0;
          flex: 1;
          flex-direction: column;
          gap: 3px;

          .chat-draft-name {
            overflow: hidden;
            color: var(--color-text);
            text-overflow: ellipsis;
            white-space: nowrap;
            font-size: 12.5px;
            font-weight: 500;
          }

          .chat-draft-size {
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 10.5px;
            color: var(--color-text-soft);
          }
        }

        .chat-icon-button {
          display: grid;
          width: 22px;
          height: 22px;
          flex: none;
          place-items: center;
          padding: 0;
          border: 0;
          border-radius: 4px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          cursor: pointer;

          &:hover {
            color: var(--color-danger);
            background: var(--color-danger-soft);
          }
        }
      }
    }

    .chat-input-wrapper {
      position: relative;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      padding: 8px 12px;
      transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);

      &:focus-within {
        border-color: var(--color-primary);
        box-shadow: 0 0 0 2px var(--color-primary-soft);
      }

      &.is-dragover {
        border-color: var(--color-primary);
        border-style: dashed;
        background: var(--color-primary-soft);
        box-shadow:
          0 0 0 2px var(--color-info-line),
          0 0 16px var(--color-primary-soft);
      }

      .chat-drag-overlay {
        position: absolute;
        inset: 0;
        z-index: 10;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 10px;
        border-radius: 7px;
        background: var(--color-panel);
        border: 2px dashed var(--color-primary);
        color: var(--color-primary);
        pointer-events: none;
        backdrop-filter: blur(4px);
        animation: cyberFadeIn 0.15s ease-out;

        .cyber-bounce-icon {
          animation: cyberBounce 1.2s ease-in-out infinite;
        }

        .chat-drag-text {
          font-size: 13.5px;
          font-weight: 600;
          letter-spacing: 0.5px;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        }
      }
    }

    .chat-input {
      display: block;
      width: 100%;
      min-height: 60px;
      max-height: 160px;
      resize: vertical;
      padding: 2px 0;
      border: 0;
      outline: none;
      background: transparent;
      color: var(--color-text);
      font: inherit;
      font-size: 13px;
      line-height: 1.6;

      &::placeholder {
        color: var(--color-text-soft);
        font-family: inherit;
        font-size: 12.5px;
      }
    }

    .chat-composer-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      margin-top: 10px;

      .chat-compose-tools {
        display: flex;
        min-width: 0;
        align-items: center;
        gap: 12px;

        .chat-attach-button {
          display: inline-flex;
          flex: none;
          align-items: center;
          gap: 6px;
          height: 30px;
          padding: 0 12px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel);
          color: var(--color-text);
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.2s;

          &:hover:not(:disabled) {
            color: var(--color-text);
            border-color: var(--color-line-strong);
            background: var(--color-panel-soft);
          }

          &:disabled {
            opacity: 0.5;
            cursor: not-allowed;
          }
        }

        .chat-compose-hint {
          overflow: hidden;
          color: var(--color-text-muted);
          text-overflow: ellipsis;
          white-space: nowrap;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11px;
        }
      }

      .chat-send-button {
        display: inline-flex;
        height: 32px;
        flex: none;
        align-items: center;
        gap: 7px;
        padding: 0 16px;
        border-radius: 6px;
        background: var(--color-primary-solid);
        border: 1px solid var(--color-primary);
        color: #ffffff;
        font-size: 12.5px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s;

        &:hover:not(:disabled) {
          background: var(--color-primary);
          box-shadow: 0 2px 10px rgba(0, 0, 0, 0.12);
        }

        &:disabled {
          opacity: 0.45;
          cursor: not-allowed;
          box-shadow: none;
        }
      }
    }

    .chat-file-input {
      display: none;
    }
  }
}

.chat-search-drawer {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 14px;
  color: var(--color-text);

  .chat-search-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    color: var(--color-text-muted);

    .chat-search-btns {
      display: flex;
      gap: 6px;
    }
  }

  .chat-search-results {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;

    .chat-search-result {
      display: flex;
      align-items: flex-start;
      gap: 10px;
      padding: 10px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel-soft);

      .chat-search-jump {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 4px;
        padding: 0;
        border: 0;
        background: transparent;
        color: var(--color-text);
        text-align: left;
        cursor: pointer;

        .chat-search-time {
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 10.5px;
          color: var(--color-text-soft);
        }

        span {
          font-size: 12.5px;
          line-height: 1.4;
          word-break: break-all;
        }

        &:hover span {
          color: var(--color-primary);
        }
      }
    }

    .chat-search-empty {
      padding: 30px 0;
      text-align: center;
      color: var(--color-text-muted);
      font-size: 13px;
    }
  }
}

.cyber-pulse-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;

  &--amber {
    background: var(--color-warning);
    box-shadow: 0 0 6px var(--color-warning);
    animation: cyberPulse 1.6s ease-in-out infinite;
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

.cyber-spin {
  animation: cyberSpin 0.9s linear infinite;
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

@keyframes cyberBounce {
  0%,
  100% {
    transform: translateY(0);
  }
  50% {
    transform: translateY(-4px);
  }
}

@keyframes cyberFadeIn {
  from {
    opacity: 0;
    transform: scale(0.98);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

@keyframes progressIndeterminate {
  0% {
    transform: translateX(-100%);
  }
  50% {
    transform: translateX(120%);
  }
  100% {
    transform: translateX(300%);
  }
}
</style>

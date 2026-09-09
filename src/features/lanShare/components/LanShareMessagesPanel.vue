<template>
  <section
    ref="panelRef"
    class="lan-chat"
    @dragover.prevent
    @drop.prevent="dropFiles"
  >
    <div ref="messageListRef" class="chat-timeline" aria-label="聊天消息">
      <div v-if="!messages.length" class="chat-empty">
        <MessagesSquare :size="34" :stroke-width="1.3" />
        <span>从一句话或一个文件开始</span>
        <small class="chat-empty-hint"
          >图片、文件和文字都在这里查看，无需切换页面。</small
        >
      </div>
      <article
        v-for="message in messages"
        :key="message.id"
        :data-message-id="message.id"
        class="chat-message"
        :class="{
          'chat-message-self': message.direction === 'desktop-to-mobile'
        }"
      >
        <div class="chat-message-meta">
          <span>{{
            message.direction === "desktop-to-mobile"
              ? "我"
              : message.deviceName || currentDevice?.name || "对方"
          }}</span>
          <time>{{ formatDateTime(message.createdAt) }}</time>
          <span
            v-if="
              message.direction === 'desktop-to-mobile' && !message.delivered
            "
            class="chat-delivery"
            >待对方接收</span
          >
        </div>
        <div class="chat-message-body">
          <div
            class="chat-bubble"
            :class="{
              'chat-bubble-attachments': !message.content?.trim() && message.attachments?.length
            }"
          >
            <p v-if="message.content?.trim()" class="chat-text">
              {{ message.content }}
            </p>
            <LanShareAttachmentGallery
              v-if="message.attachments?.length"
              :files="message.attachments"
              :service="service"
              :session-id="currentSessionId"
              @preview="$emit('preview-file', $event)"
            />
          </div>
          <div class="chat-message-actions">
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
        </div>
      </article>
    </div>

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
          <File v-else class="chat-draft-icon" :size="22" />
          <span class="chat-draft-caption"
            ><span class="chat-draft-name" :title="item.name">{{
              item.name
            }}</span
            ><small class="chat-draft-size">{{
              formatFileSize(item.size)
            }}</small></span
          >
          <button
            class="chat-icon-button"
            type="button"
            :disabled="sending"
            :aria-label="`移除 ${item.name}`"
            @click="removeAttachment(item)"
          >
            <X :size="13" />
          </button>
        </div>
      </div>
      <textarea
        ref="composerRef"
        v-model="currentDraft.content"
        class="chat-input"
        :disabled="sending || !currentSessionId"
        rows="2"
        placeholder="输入消息，也可以直接粘贴图片、文件，或拖拽到这里…"
        aria-label="聊天输入框"
        @paste="pasteFiles"
        @keydown="composerKeydown"
      ></textarea>
      <div class="chat-composer-footer">
        <div class="chat-compose-tools">
          <button
            class="chat-attach-button"
            type="button"
            :disabled="sending || !currentSessionId"
            title="添加文件或图片，可多选"
            @click="pickFiles"
          >
            <Paperclip :size="17" /><span>添加附件</span>
          </button>
          <span class="chat-compose-hint">{{
            sending
              ? sendStatus
              : currentDraft.files.length
                ? `${currentDraft.files.length} 个附件 · 合并为一条消息`
                : "Enter 发送 · Shift + Enter 换行"
          }}</span>
        </div>
        <button
          class="chat-send-button"
          type="button"
          :disabled="
            sending ||
            !service.running ||
            !currentSessionId ||
            (!currentDraft.content.trim() && !currentDraft.files.length)
          "
          @click="sendMessage"
        >
          <Send :size="15" />{{ sending ? "发送中" : "发送" }}
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

    <el-drawer
      v-model="searchOpen"
      title="聊天记录"
      size="420px"
      append-to-body
    >
      <div class="chat-search-drawer">
        <el-input
          v-model="keyword"
          placeholder="搜索文字或附件名称"
          clearable
        />
        <el-select v-model="timeFilter" aria-label="消息时间范围"
          ><el-option label="全部时间" value="all" /><el-option
            label="今天"
            value="today" /><el-option label="最近 7 天" value="week"
        /></el-select>
        <div class="chat-search-actions">
          <span>{{ filteredMessages.length }} 条记录</span
          ><el-button size="small" @click="selectAll">{{
            allSelected ? "取消全选" : "全选"
          }}</el-button
          ><el-button
            size="small"
            type="danger"
            plain
            :disabled="!selectedIds.length"
            @click="deleteMessages(selectedIds)"
            >删除所选</el-button
          >
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
              }}</small
              ><span>{{
                message.content ||
                message.attachments?.map((file) => file.name).join("、") ||
                "文件消息"
              }}</span>
            </button>
          </div>
          <span v-if="!filteredMessages.length" class="chat-search-empty"
            >没有匹配的聊天记录</span
          >
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
  Copy,
  File,
  MessagesSquare,
  Paperclip,
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
const emit = defineEmits(["refresh-state", "preview-file"])
const panelRef = ref(null)
const messageListRef = ref(null)
const composerRef = ref(null)
const fileInputRef = ref(null)
const messages = ref([])
const drafts = reactive({})
const sending = ref(false)
const sendStatus = ref("")
const searchOpen = ref(false)
const keyword = ref("")
const timeFilter = ref("all")
const selectedIds = ref([])
let stopMessageListener = null
let stopDropListener = null
let disposed = false
let loadSeed = 0

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
const currentDraft = computed(() => drafts[props.currentSessionId])
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
        if (event.payload.type !== "drop") return
        const bounds = panelRef.value?.getBoundingClientRect()
        const position = event.payload.position
        const scale = window.devicePixelRatio || 1
        if (
          bounds &&
          position.x / scale >= bounds.left &&
          position.x / scale <= bounds.right &&
          position.y / scale >= bounds.top &&
          position.y / scale <= bounds.bottom
        )
          addPaths(event.payload.paths)
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
    if (!sending.value) discardUploads(draft.files)
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

function appendFiles(files) {
  if (sending.value || !props.currentSessionId) return
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
  if (sending.value || !props.currentSessionId) return
  for (const path of paths) {
    if (currentDraft.value.files.some((item) => item.path === path)) continue
    if (currentDraft.value.files.length >= 100) {
      createMessage.warning("一条消息最多添加 100 个附件。")
      break
    }
    currentDraft.value.files.push({
      id: crypto.randomUUID(),
      path,
      name: path.split(/[\\/]/).at(-1),
      size: null
    })
  }
}

async function pickFiles() {
  if (!isTauri()) {
    fileInputRef.value?.click()
    return
  }
  try {
    addPaths(
      (await systemApi.selectFiles({ title: "选择要发送的文件和图片" })) || []
    )
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

function chooseBrowserFiles(event) {
  appendFiles([...event.target.files])
  event.target.value = ""
}
async function pasteFiles(event) {
  const files = [...(event.clipboardData?.files || [])]
  if (files.length) {
    event.preventDefault()
    appendFiles(files)
    return
  }
  if (!isTauri() || event.clipboardData?.getData("text/plain") || sending.value)
    return
  const sessionId = props.currentSessionId
  try {
    const paths = unwrap(await lanShareApi.getClipboardFiles())
    if (sessionId === props.currentSessionId && !disposed) addPaths(paths || [])
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}
function dropFiles(event) {
  appendFiles([...event.dataTransfer.files])
}
function composerKeydown(event) {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault()
    sendMessage()
  }
}

async function discardUploads(files) {
  const identifiers = files.map((file) => file.uploadedId).filter(Boolean)
  if (identifiers.length) {
    try {
      await lanShareApi.discardUploads({ attachmentIds: identifiers })
    } catch (error) {
      if (!disposed)
        createMessage.error(`临时附件清理失败：${error?.message || error}`)
    }
  }
}

function removeAttachment(item) {
  if (sending.value) return
  currentDraft.value.files = currentDraft.value.files.filter(
    (file) => file.id !== item.id
  )
  if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
  discardUploads([item])
}

async function sendMessage() {
  if (sending.value || !props.service.running || !props.currentSessionId) return
  const draft = currentDraft.value
  if (!draft.content.trim() && !draft.files.length) return
  const sessionId = props.currentSessionId
  const deviceId =
    props.chatMode === "direct" ? props.currentDevice?.id || "" : ""
  const selected = [...draft.files]
  const fingerprint = JSON.stringify([
    draft.content,
    selected.map((item) => item.id)
  ])
  if (draft.fingerprint !== fingerprint || !draft.messageId)
    draft.messageId = crypto.randomUUID()
  draft.fingerprint = fingerprint
  sending.value = true
  try {
    const access = new URL(props.service.accessUrl)
    for (const [index, item] of selected.entries()) {
      sendStatus.value = `准备附件 ${index + 1} / ${selected.length}`
      if (item.path || item.uploadedId) continue
      const url = new URL("/api/files/upload", access.origin)
      url.search = new URLSearchParams({
        token: access.searchParams.get("token") || "",
        sessionId,
        name: item.name
      }).toString()
      const response = await fetch(url, { method: "PUT", body: item.file })
      const payload = await response.json()
      if (!response.ok || payload.status !== "success")
        throw new Error(payload.message || "附件上传失败")
      item.uploadedId = payload.data.id
    }
    sendStatus.value = selected.length
      ? `正在发送 ${selected.length} 个附件…`
      : "正在发送…"
    await lanShareApi.sendMessage({
      sessionId,
      deviceId,
      messageId: draft.messageId,
      content: draft.content.trim(),
      paths: selected.map((item) => item.path).filter(Boolean),
      attachmentIds: selected.map((item) => item.uploadedId).filter(Boolean),
      attachmentOrder: selected.map((item) =>
        item.path ? { path: item.path } : { id: item.uploadedId }
      )
    })
    for (const item of selected)
      if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
    draft.files = []
    draft.content = ""
    draft.messageId = ""
    if (!disposed) {
      emit("refresh-state")
      await loadMessages()
      await nextTick()
      composerRef.value?.focus()
    }
  } catch (error) {
    if (!disposed)
      createMessage.error(`发送失败，草稿已保留：${error?.message || error}`)
  } finally {
    sending.value = false
    sendStatus.value = ""
  }
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

  .chat-timeline {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 18px;
    padding: 22px 24px;
    overflow-y: auto;
    background: var(--color-panel-soft);
    scrollbar-width: thin;

    .chat-empty {
      display: flex;
      min-height: 160px;
      flex: 1;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 14px;
      color: var(--color-text-muted);
      font-size: var(--font-size-base);
      text-align: center;

      .chat-empty-hint {
        font-size: var(--font-size-sm);
      }
    }
    .chat-message {
      position: relative;
      display: flex;
      max-width: min(86%, 560px);
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
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        .chat-delivery {
          color: var(--color-warning);
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
          &.chat-bubble-attachments {
            flex: 0 1 auto;
          }
          &:not(.chat-bubble-attachments) {
            padding: 10px;
            border: 1px solid var(--color-line);
            border-radius: 0 12px 12px;
            background: var(--color-panel);
          }

          .chat-text {
            margin: 0;
            padding: 1px 3px;
            color: var(--color-text);
            white-space: pre-wrap;
            overflow-wrap: anywhere;
            line-height: 1.7;
            font-size: var(--font-size-base);
          }
        }
        .chat-message-actions {
          display: flex;
          flex: none;
          gap: 4px;
          opacity: 0;
          .chat-icon-button {
            display: grid;
            width: 24px;
            height: 22px;
            place-items: center;
            padding: 0;
            border: 0;
            background: transparent;
            color: var(--color-text-muted);
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
        }
        .chat-message-body {
          flex-direction: row-reverse;
          .chat-bubble:not(.chat-bubble-attachments) {
            border-radius: 12px 0 12px 12px;
            background: var(--color-primary-soft);
            border-color: var(--color-info-line);
          }
        }
      }
    }
  }

  .chat-composer {
    flex: none;
    min-width: 0;
    padding: 12px 16px 10px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);

    .chat-draft-files {
      display: flex;
      max-height: 140px;
      gap: 8px;
      padding-bottom: 10px;
      overflow: auto;

      .chat-draft-file {
        display: flex;
        width: 196px;
        flex: none;
        align-items: center;
        gap: 8px;
        padding: 7px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel-soft);
        .chat-draft-thumbnail {
          width: 36px;
          height: 36px;
          flex: none;
          object-fit: cover;
          border-radius: 4px;
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
          gap: 4px;
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
          .chat-draft-size {
            font-size: inherit;
          }
          .chat-draft-name {
            overflow: hidden;
            color: var(--color-text);
            text-overflow: ellipsis;
            white-space: nowrap;
            font-size: var(--font-size-base);
          }
        }
        .chat-icon-button {
          display: grid;
          width: 22px;
          height: 24px;
          flex: none;
          place-items: center;
          padding: 0;
          border: 0;
          background: transparent;
          color: var(--color-text-muted);
        }
      }
    }
    .chat-input {
      display: block;
      width: 100%;
      min-height: 58px;
      max-height: 170px;
      resize: vertical;
      padding: 3px 0;
      border: 0;
      outline: none;
      background: transparent;
      color: var(--color-text);
      font: inherit;
      font-size: var(--font-size-base);
      line-height: 1.7;
    }
    .chat-composer-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      .chat-compose-tools {
        display: flex;
        min-width: 0;
        align-items: center;
        gap: 10px;
        .chat-attach-button {
          display: inline-flex;
          flex: none;
          align-items: center;
          gap: 5px;
          padding: 5px 0;
          border: 0;
          background: transparent;
          color: var(--color-text-muted);
          font-size: var(--font-size-base);
        }
        .chat-compose-hint {
          overflow: hidden;
          color: var(--color-text-soft);
          text-overflow: ellipsis;
          white-space: nowrap;
          font-size: var(--font-size-sm);
        }
      }
      .chat-send-button {
        display: inline-flex;
        height: 32px;
        flex: none;
        align-items: center;
        gap: 6px;
        padding: 0 14px;
        border: 0;
        border-radius: 6px;
        background: var(--color-primary);
        color: var(--color-primary-contrast, #fff);
        font-size: var(--font-size-base);
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
  gap: 12px;
  .chat-search-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    color: var(--color-text-muted);
    font-size: var(--font-size-base);
  }
  .chat-search-results {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    .chat-search-result {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 0;
      border-bottom: 1px solid var(--color-line);
      .chat-search-jump {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 6px;
        padding: 5px 0;
        border: 0;
        background: transparent;
        color: var(--color-text);
        text-align: left;
        overflow-wrap: anywhere;
        font-size: var(--font-size-base);

        .chat-search-time {
          font-size: var(--font-size-sm);
        }
      }
    }
    .chat-search-empty {
      display: block;
      padding: 24px;
      color: var(--color-text-muted);
      text-align: center;
      font-size: var(--font-size-base);
    }
  }
}
</style>

<template>
  <section class="lan-share-messages-panel">
    <header class="lan-share-messages-head">
      <div class="lan-share-messages-title">
        <strong class="lan-share-messages-name">消息通信</strong>
        <span class="lan-share-messages-subtitle">
          {{ sessionSummary }}
        </span>
      </div>
      <div class="lan-share-messages-actions">
        <input
          v-model="keyword"
          class="lan-share-messages-search"
          placeholder="搜索消息"
          type="search"
        />
        <select v-model="timeFilter" class="lan-share-messages-select">
          <option value="all">全部时间</option>
          <option value="today">今天</option>
          <option value="week">最近 7 天</option>
        </select>
        <button
          class="lan-share-messages-mini-button"
          type="button"
          :disabled="!messages.length"
          @click="toggleSelectAllMessages"
        >
          {{ allMessagesSelected ? "取消全选" : "全选" }}
        </button>
        <button
          class="lan-share-messages-mini-button"
          type="button"
          :disabled="!selectedMessageIds.length || loading"
          @click="deleteSelectedMessages"
        >
          <Trash2 :size="13" />
          删除所选
        </button>
      </div>
    </header>
    <div ref="messageListRef" class="lan-share-messages-list">
      <article
        v-for="message in sortedMessages"
        :key="message.id"
        :class="[
          'lan-share-messages-item',
          {
            'lan-share-messages-item-desktop':
              message.direction === 'desktop-to-mobile',
            'lan-share-messages-item-mobile':
              message.direction !== 'desktop-to-mobile',
            'lan-share-messages-item-file': message.messageType === 'file'
          }
        ]"
      >
        <div class="lan-share-messages-item-head">
          <label class="lan-share-messages-check">
            <input
              v-model="selectedMessageIds"
              class="lan-share-messages-check-input"
              type="checkbox"
              :value="message.id"
            />
            <span class="lan-share-messages-check-mark"></span>
          </label>
          <span class="lan-share-messages-sender">
            {{ messageSenderName(message) }}
          </span>
          <button
            class="lan-share-messages-delete"
            type="button"
            title="删除消息"
            @click="deleteMessage(message)"
          >
            <Trash2 :size="12" />
          </button>
        </div>
        <span class="lan-share-messages-meta">
          {{ messageRelationText(message) }} ·
          {{ formatDateTime(message.createdAt) }}
        </span>
        <p
          class="lan-share-messages-content"
          title="点击复制消息"
          @click="copyMessageContent(message)"
        >
          <FileText
            v-if="message.messageType === 'file'"
            class="lan-share-messages-content-icon"
            :size="14"
          />
          {{ message.content }}
        </p>
      </article>
      <div v-if="!messages.length" class="lan-share-messages-empty">
        暂无消息。
      </div>
    </div>
    <footer class="lan-share-messages-composer">
      <input
        v-model="messageDraft"
        class="lan-share-messages-composer-input"
        type="text"
        placeholder="输入要发送到设备的消息"
        @keydown.enter="sendMessage"
      />
      <button
        class="lan-share-messages-button"
        type="button"
        :disabled="!currentSessionId || loading"
        @click="clearCurrentSession"
      >
        <Eraser :size="14" />
        清空会话
      </button>
      <button
        class="lan-share-messages-button lan-share-messages-button-primary"
        type="button"
        :disabled="!messageDraft.trim() || !currentSession || loading"
        @click="sendMessage"
      >
        <Send :size="14" />
        发送
      </button>
    </footer>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import { Eraser, FileText, Send, Trash2 } from "lucide-vue-next"
import { lanShareApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  currentDevice: {
    type: Object,
    default: null
  },
  currentSessionId: {
    type: String,
    default: ""
  },
  currentSession: {
    type: Object,
    default: null
  },
  chatMode: {
    type: String,
    default: "direct"
  },
  stateVersion: {
    type: Number,
    default: 0
  }
})

const emit = defineEmits(["refresh-state"])

const messageListRef = ref(null)
const messages = ref([])
const keyword = ref("")
const timeFilter = ref("all")
const messageDraft = ref("")
const selectedMessageIds = ref([])
const loading = ref(false)
let stopMessageListener = null
let loadSeed = 0

const sessionSummary = computed(() => {
  if (!props.currentSession) {
    return "请选择设备和会话"
  }

  return `${messages.value.length} 条消息 · ${formatDateTime(
    props.currentSession.updatedAt
  )}`
})

const sortedMessages = computed(() => {
  return [...messages.value].sort((left, right) => {
    return Number(left.createdAt || 0) - Number(right.createdAt || 0)
  })
})

const allMessagesSelected = computed(() => {
  return (
    Boolean(messages.value.length) &&
    messages.value.every((message) =>
      selectedMessageIds.value.includes(message.id)
    )
  )
})

onMounted(() => {
  loadMessages()
  stopMessageListener = lanShareApi.onMessageCreated((message) => {
    if (
      message.sessionId === props.currentSessionId ||
      props.chatMode === "group"
    ) {
      loadMessages()
      emit("refresh-state")
    }
  })
})

onBeforeUnmount(() => {
  if (stopMessageListener) stopMessageListener()
})

watch(
  () => [
    props.currentDevice?.id || "",
    props.currentSessionId,
    props.chatMode,
    props.stateVersion,
    keyword.value,
    timeFilter.value
  ],
  () => loadMessages()
)

watch(
  () => sortedMessages.value,
  () => scrollMessagesToBottom(),
  { deep: true }
)

async function loadMessages() {
  const seed = ++loadSeed

  if (!props.currentSessionId) {
    messages.value = []
    selectedMessageIds.value = []
    return
  }

  try {
    const result = unwrapData(
      await lanShareApi.listMessages({
        deviceId:
          props.chatMode === "direct" ? props.currentDevice?.id || "" : "",
        sessionId: props.currentSessionId,
        keyword: keyword.value,
        from: filterStartAt(),
        to: 0
      })
    )

    if (seed === loadSeed) {
      messages.value = Array.isArray(result) ? result : []
      selectedMessageIds.value = selectedMessageIds.value.filter(
        (messageId) => {
          return messages.value.some((message) => message.id === messageId)
        }
      )
    }
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

function unwrapData(result) {
  return result?.status && "data" in result ? result.data : result
}

function scrollMessagesToBottom() {
  nextTick(() => {
    const messageList = messageListRef.value

    if (messageList) {
      messageList.scrollTop = messageList.scrollHeight
    }
  })
}

async function runMessageAction(action, successMessage) {
  loading.value = true

  try {
    const result = unwrapData(await action())

    if (successMessage) {
      createMessage.success(successMessage)
    }

    await loadMessages()
    emit("refresh-state")
    return result
  } catch (error) {
    createMessage.error(error?.message || String(error))
    return null
  } finally {
    loading.value = false
  }
}

async function sendMessage() {
  const content = messageDraft.value.trim()

  if (!content || !props.currentSessionId) {
    if (content) {
      createMessage.warning("请先选择会话后再发送消息。")
    }
    return
  }

  const result = await runMessageAction(async () =>
    lanShareApi.sendMessage({
      deviceId:
        props.chatMode === "direct" ? props.currentDevice?.id || "" : "",
      sessionId: props.currentSessionId,
      content
    })
  )

  if (result) {
    messageDraft.value = ""
  }
}

async function deleteMessage(message) {
  await runMessageAction(async () =>
    lanShareApi.deleteMessage({ messageId: message.id })
  )
}

async function deleteSelectedMessages() {
  if (!selectedMessageIds.value.length) {
    return
  }

  const messageIds = [...selectedMessageIds.value]
  const result = await runMessageAction(
    async () =>
      lanShareApi.deleteMessages({
        messageIds,
        sessionId: props.currentSessionId
      }),
    "已删除所选消息。"
  )

  if (result) {
    selectedMessageIds.value = []
  }
}

function toggleSelectAllMessages() {
  if (allMessagesSelected.value) {
    selectedMessageIds.value = []
    return
  }

  selectedMessageIds.value = messages.value.map((message) => message.id)
}

async function clearCurrentSession() {
  if (!props.currentSessionId) {
    return
  }

  await runMessageAction(
    async () => lanShareApi.clearSession({ sessionId: props.currentSessionId }),
    "当前会话已清空。"
  )
}

async function copyMessageContent(message) {
  try {
    await navigator.clipboard.writeText(message.content || "")
    createMessage.success("消息已复制。")
  } catch (error) {
    createMessage.error(error?.message || "复制失败。")
  }
}

function messageSenderName(message) {
  if (message.direction === "desktop-to-mobile") {
    return "电脑端"
  }

  return message.deviceName || "未知设备"
}

function messageRelationText(message) {
  if (message.direction === "desktop-to-mobile") {
    return `发给 ${message.deviceName || "未知设备"}`
  }

  return "设备发送"
}

function filterStartAt() {
  const now = Date.now()

  if (timeFilter.value === "today") {
    return new Date().setHours(0, 0, 0, 0)
  }
  if (timeFilter.value === "week") {
    return now - 7 * 24 * 60 * 60 * 1000
  }

  return 0
}
</script>

<style scoped lang="less">
.lan-share-messages-panel {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);

  .lan-share-messages-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 48px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);

    .lan-share-messages-title {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 2px;

      .lan-share-messages-name {
        color: var(--color-text);
        font-size: 0.9rem;
      }

      .lan-share-messages-subtitle {
        color: var(--color-text-muted);
        font-size: 0.76rem;
      }
    }

    .lan-share-messages-actions {
      display: flex;
      min-width: 0;
      flex: none;
      align-items: center;
      gap: 8px;

      .lan-share-messages-search,
      .lan-share-messages-select {
        height: 32px;
        min-width: 0;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-text);
      }

      .lan-share-messages-search {
        width: 180px;
        padding: 0 10px;
      }

      .lan-share-messages-select {
        width: 138px;
        padding: 0 8px;
      }

      .lan-share-messages-mini-button {
        display: inline-flex;
        height: 32px;
        flex: none;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 0 10px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-primary);
        cursor: pointer;
        font-size: 0.76rem;
        font-weight: 700;
      }

      .lan-share-messages-mini-button:disabled {
        cursor: not-allowed;
        opacity: 0.45;
      }
    }
  }

  .lan-share-messages-list {
    display: flex;
    min-height: 0;
    height: 0;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding: 12px;
    background: linear-gradient(180deg, var(--color-panel) 0%, var(--color-panel-soft) 100%);

    .lan-share-messages-item {
      display: flex;
      width: fit-content;
      max-width: 72%;
      min-width: 180px;
      flex-direction: column;
      gap: 5px;
      padding: 9px 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      box-shadow: 0 6px 18px rgba(42, 67, 101, 0.08);

      .lan-share-messages-item-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        color: var(--color-text-muted);
        font-size: 0.74rem;

        .lan-share-messages-check {
          position: relative;
          display: inline-flex;
          width: 18px;
          height: 18px;
          flex: none;
          align-items: center;
          justify-content: center;
          cursor: pointer;

          .lan-share-messages-check-input {
            position: absolute;
            inset: 0;
            margin: 0;
            cursor: pointer;
            opacity: 0;
          }

          .lan-share-messages-check-mark {
            display: inline-flex;
            width: 16px;
            height: 16px;
            border: 1px solid var(--color-line);
            border-radius: 4px;
            background: var(--color-panel);
          }

          .lan-share-messages-check-input:checked
            + .lan-share-messages-check-mark {
            border-color: var(--color-primary);
            background: var(--color-primary-solid);
          }
        }

        .lan-share-messages-sender {
          min-width: 0;
          flex: 1;
          overflow: hidden;
          font-weight: 700;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .lan-share-messages-delete {
          display: inline-flex;
          width: 24px;
          height: 24px;
          flex: none;
          align-items: center;
          justify-content: center;
          border: 1px solid transparent;
          border-radius: 7px;
          background: transparent;
          color: var(--color-text-muted);
          cursor: pointer;
        }
      }

      .lan-share-messages-meta {
        color: var(--color-text-soft);
        font-size: 0.7rem;
      }

      .lan-share-messages-content {
        margin: 0;
        color: var(--color-text);
        cursor: pointer;
        font-size: 0.84rem;
        line-height: 1.55;
        word-break: break-word;
      }

      .lan-share-messages-content:hover {
        color: var(--color-primary);
      }
    }

    .lan-share-messages-item-desktop {
      align-self: flex-end;
      border-color: var(--color-success-line);
      background: var(--color-success-soft);
    }

    .lan-share-messages-item-mobile {
      align-self: flex-start;
    }

    .lan-share-messages-item-file {
      border-color: var(--color-line-strong);
      background: var(--color-primary-soft);

      .lan-share-messages-content {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        font-weight: 700;

        .lan-share-messages-content-icon {
          flex: none;
          color: var(--color-primary);
        }
      }
    }

    .lan-share-messages-empty {
      display: flex;
      min-height: 120px;
      align-items: center;
      justify-content: center;
      border: 1px dashed var(--color-line);
      border-radius: 8px;
      color: var(--color-text-muted);
      font-size: 0.82rem;
    }
  }

  .lan-share-messages-composer {
    display: flex;
    flex: none;
    gap: 8px;
    padding: 10px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel-soft);

    .lan-share-messages-composer-input {
      height: 32px;
      min-width: 0;
      flex: 1;
      padding: 0 10px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-text);
    }

    .lan-share-messages-button {
      display: inline-flex;
      height: 34px;
      align-items: center;
      justify-content: center;
      gap: 6px;
      padding: 0 12px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-primary);
      cursor: pointer;
      font-weight: 700;
    }

    .lan-share-messages-button-primary {
      border-color: var(--color-primary);
      background: var(--color-primary-solid);
      color: #ffffff;
    }

    .lan-share-messages-button:disabled {
      cursor: not-allowed;
      opacity: 0.5;
    }
  }
}
</style>

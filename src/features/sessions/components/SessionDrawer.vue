<template>
  <div v-if="session" class="session-drawer">
    <div class="session-drawer__overlay" @click="$emit('close')"></div>
    <aside class="session-drawer__panel">
      <header class="session-drawer__header">
        <div>
          <p>{{ session.cliName || session.cli }}</p>
          <h2>{{ session.title }}</h2>
          <span>{{ session.projectName || "未识别项目" }}</span>
        </div>
        <button
          class="session-drawer__close"
          type="button"
          @click="$emit('close')"
        >
          ×
        </button>
      </header>

      <div class="session-drawer__tabs">
        <button
          v-for="item in tabs"
          :key="item.id"
          :class="[
            'session-drawer__tab',
            { 'session-drawer__tab--active': activeTab === item.id }
          ]"
          type="button"
          @click="activeTab = item.id"
        >
          {{ item.label }}
        </button>
      </div>

      <div class="session-drawer__body">
        <div
          ref="contentRef"
          class="session-drawer__content"
          @scroll="updateActiveMessage"
        >
          <div v-if="pending" class="session-drawer__loading">
            正在加载 Session Messages...
          </div>

          <section
            v-else-if="activeTab === 'messages'"
            class="session-drawer__section"
          >
            <label class="session-drawer__message-search">
              <span>消息搜索</span>
              <input
                v-model.trim="messageKeyword"
                type="text"
                placeholder="搜索当前 Session 消息"
              />
            </label>

            <div class="session-drawer__message-workspace">
              <div class="session-drawer__message-list">
                <article
                  v-for="item in visibleMessages"
                  :id="messageTargetId(item.index)"
                  :key="`${item.message.role}-${item.index}`"
                  :data-message-index="item.index"
                  :class="[
                    'session-drawer__chat-message',
                    `session-drawer__chat-message--${item.message.role}`
                  ]"
                >
                  <div class="session-drawer__chat-avatar">
                    <component :is="roleIcon(item.message.role)" :size="16" />
                  </div>
                  <div class="session-drawer__chat-body">
                    <div class="session-drawer__chat-meta">
                      <span data-emphasis>{{ roleLabel(item.message.role) }}</span>
                      <span>{{ formatDateTime(item.message.timestamp) }}</span>
                      <button
                        type="button"
                        title="复制消息"
                        @click="copyText(item.message.content)"
                      >
                        <Copy :size="14" />
                      </button>
                    </div>
                    <div class="session-drawer__chat-bubble">
                      <p>{{ item.message.content }}</p>
                    </div>
                  </div>
                </article>

                <div
                  v-if="!visibleMessages.length"
                  class="session-drawer__empty"
                >
                  没有匹配的消息。
                </div>
              </div>
            </div>
          </section>

          <section
            v-else-if="activeTab === 'files'"
            class="session-drawer__section"
          >
            <article
              v-for="file in files"
              :key="file"
              class="session-drawer__info-card"
            >
              <span>File</span>
              <button type="button" @click="$emit('open-path', file)">
                {{ file }}
              </button>
            </article>
            <div v-if="!files.length" class="session-drawer__empty">
              当前 Session 未解析到引用文件。
            </div>
          </section>

          <section
            v-else-if="activeTab === 'tools'"
            class="session-drawer__section"
          >
            <details
              v-for="(tool, index) in toolCalls"
              :key="`${tool.name}-${index}`"
              class="session-drawer__tool"
            >
              <summary>{{ tool.name }}</summary>
              <div>
                <span>Arguments</span>
                <pre>{{ tool.arguments || "无" }}</pre>
              </div>
              <div>
                <span>Result</span>
                <pre>{{ tool.result || "无" }}</pre>
              </div>
            </details>
            <div v-if="!toolCalls.length" class="session-drawer__empty">
              当前 Session 未解析到 Tool Calls。
            </div>
          </section>

          <section
            v-else-if="activeTab === 'context'"
            class="session-drawer__section"
          >
            <article class="session-drawer__info-card">
              <span>Project</span>
              <button
                v-if="session.projectPath"
                type="button"
                @click="$emit('open-path', session.projectPath)"
              >
                {{ session.projectPath }}
              </button>
              <p v-else>未识别项目路径。</p>
            </article>
            <article class="session-drawer__info-card">
              <span>Summary</span>
              <p>{{ session.summary || "暂无摘要。" }}</p>
            </article>
          </section>

          <section v-else class="session-drawer__section">
            <div class="session-drawer__grid">
              <article>
                <span>CLI</span>
                <span data-emphasis>{{ session.cliName || session.cli }}</span>
              </article>
              <article>
                <span>Model</span>
                <span data-emphasis>{{ session.model || "未识别" }}</span>
              </article>
              <article>
                <span>CreatedAt</span>
                <span data-emphasis>{{ formatDateTime(session.createdAt) }}</span>
              </article>
              <article>
                <span>UpdatedAt</span>
                <span data-emphasis>{{ formatDateTime(session.updatedAt) }}</span>
              </article>
              <article>
                <span>Messages</span>
                <span data-emphasis>{{ session.messageCount }}</span>
              </article>
              <article>
                <span>RawPath</span>
                <button
                  type="button"
                  @click="$emit('open-path', session.rawPath)"
                >
                  {{ session.rawPath }}
                </button>
              </article>
            </div>
          </section>
        </div>

        <nav
          v-if="messageNavPositions.length"
          ref="messageNavRef"
          class="session-drawer__message-nav"
          aria-label="消息快速导航"
          @pointerdown="startMessageScrollDrag"
        >
          <span
            class="session-drawer__message-nav-thumb"
            :style="{
              top: `${messageScrollThumb.top}%`,
              height: `${messageScrollThumb.height}%`
            }"
            @pointerdown.stop="startMessageScrollDrag"
          ></span>
          <button
            v-for="item in messageNavPositions"
            :key="`nav-${item.index}`"
            :class="[
              'session-drawer__message-nav-item',
              `session-drawer__message-nav-item--${item.role}`,
              {
                'session-drawer__message-nav-item--active':
                  activeMessageIndex === item.index
              }
            ]"
            :style="{
              top: `${item.top}%`
            }"
            type="button"
            :title="`${item.index + 1}. ${roleLabel(item.role)}`"
            @pointerdown.stop
            @click="scrollToMessage(item.index)"
          >
            <component :is="roleIcon(item.role)" :size="10" />
          </button>
        </nav>
      </div>
    </aside>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import { Bot, Copy, Info, UserRound, Wrench } from "lucide-vue-next"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  messages: {
    type: Array,
    required: true
  },
  pending: {
    type: Boolean,
    required: true
  },
  session: {
    type: Object,
    default: null
  }
})

defineEmits(["close", "open-path"])

const tabs = [
  { id: "messages", label: "Messages" },
  { id: "files", label: "Files" },
  { id: "tools", label: "Tools" },
  { id: "context", label: "Context" },
  { id: "metadata", label: "Metadata" }
]

const activeTab = ref("messages")
const messageKeyword = ref("")
const activeMessageIndex = ref(null)
const contentRef = ref(null)
const messageNavRef = ref(null)
const messageNavPositions = ref([])
const messageScrollThumb = ref({
  top: 0,
  height: 100
})
const messageScrollDragOffset = ref(0)

const visibleMessages = computed(() => {
  const keyword = messageKeyword.value.toLowerCase()

  return props.messages
    .map((message, index) => {
      return {
        message,
        index
      }
    })
    .filter((item) => {
      if (!keyword) {
        return true
      }

      return [item.message.role, item.message.content]
        .join(" ")
        .toLowerCase()
        .includes(keyword)
    })
})

const files = computed(() => {
  return Array.from(new Set(props.messages.flatMap((item) => item.files || [])))
})

const toolCalls = computed(() => {
  return props.messages.flatMap((item) => item.toolCalls || [])
})

function roleLabel(role) {
  const labels = {
    user: "用户",
    assistant: "AI 助手",
    tool: "工具",
    system: "系统"
  }

  return labels[role] || role || "消息"
}

function roleIcon(role) {
  const icons = {
    user: UserRound,
    assistant: Bot,
    tool: Wrench,
    system: Info
  }

  return icons[role] || Info
}

function messageTargetId(index) {
  return `session-message-${index}`
}

function scrollToMessage(index) {
  const content = contentRef.value
  const messageElement = document.getElementById(messageTargetId(index))

  if (!content || !messageElement) {
    return
  }

  activeMessageIndex.value = index
  content.scrollTo({
    top: Math.max(messageElement.offsetTop - 16, 0),
    behavior: "smooth"
  })
}

function updateMessageNavPositions() {
  const content = contentRef.value

  if (!content || activeTab.value !== "messages") {
    messageNavPositions.value = []
    return
  }

  const messageElements = Array.from(
    content.querySelectorAll(".session-drawer__chat-message")
  )

  if (!messageElements.length) {
    messageNavPositions.value = []
    return
  }

  const maxScrollTop = Math.max(content.scrollHeight - content.clientHeight, 1)

  const positions = messageElements.map((item) => {
    const index = Number(item.dataset.messageIndex)
    const message = props.messages[index] || {}
    const targetTop = Math.min(item.offsetTop, maxScrollTop)

    return {
      index,
      role: message.role,
      top: Math.min(Math.max((targetTop / maxScrollTop) * 100, 4), 96)
    }
  })

  let previousKeptTop = null
  const filteredPositions = positions.filter((item, index) => {
    const keep =
      index === 0 ||
      index === positions.length - 1 ||
      previousKeptTop === null ||
      item.top - previousKeptTop >= 2.2

    if (keep) {
      previousKeptTop = item.top
    }

    return keep
  })

  messageNavPositions.value = filteredPositions

  updateActiveMessage()
  updateMessageScrollThumb()
}

function updateActiveMessage() {
  const content = contentRef.value

  if (!content || activeTab.value !== "messages") {
    return
  }

  const current = messageNavPositions.value
    .slice()
    .reverse()
    .find((item) => {
      const messageElement = document.getElementById(
        messageTargetId(item.index)
      )

      return (
        messageElement && messageElement.offsetTop <= content.scrollTop + 24
      )
    })

  activeMessageIndex.value = current?.index ?? activeMessageIndex.value
  updateMessageScrollThumb()
}

function updateMessageScrollThumb() {
  const content = contentRef.value

  if (!content || activeTab.value !== "messages") {
    messageScrollThumb.value = {
      top: 0,
      height: 100
    }
    return
  }

  const maxScrollTop = Math.max(content.scrollHeight - content.clientHeight, 0)
  const height = Math.min(
    Math.max((content.clientHeight / content.scrollHeight) * 100, 6),
    100
  )

  messageScrollThumb.value = {
    top: maxScrollTop ? (content.scrollTop / maxScrollTop) * (100 - height) : 0,
    height
  }
}

function updateMessageScrollByPointer(event) {
  const content = contentRef.value
  const nav = messageNavRef.value

  if (!content || !nav) {
    return
  }

  const rect = nav.getBoundingClientRect()
  const maxScrollTop = Math.max(content.scrollHeight - content.clientHeight, 0)
  const thumbHeight = (messageScrollThumb.value.height / 100) * rect.height
  const maxThumbTop = Math.max(rect.height - thumbHeight, 1)
  const pointerTop = Math.min(
    Math.max(event.clientY - rect.top - messageScrollDragOffset.value, 0),
    maxThumbTop
  )

  content.scrollTop = (pointerTop / maxThumbTop) * maxScrollTop
  updateActiveMessage()
}

function startMessageScrollDrag(event) {
  const nav = messageNavRef.value

  if (!nav) {
    return
  }

  const navRect = nav.getBoundingClientRect()
  const thumbHeight = (messageScrollThumb.value.height / 100) * navRect.height

  event.preventDefault()
  messageScrollDragOffset.value =
    event.currentTarget === nav ? thumbHeight / 2 : event.offsetY
  updateMessageScrollByPointer(event)
  window.addEventListener("pointermove", updateMessageScrollByPointer)
  window.addEventListener("pointerup", stopMessageScrollDrag, { once: true })
}

function stopMessageScrollDrag() {
  window.removeEventListener("pointermove", updateMessageScrollByPointer)
}

function scheduleMessageNavPositions() {
  nextTick(() => {
    requestAnimationFrame(updateMessageNavPositions)
  })
}

watch(
  () => props.session?.id,
  () => {
    activeTab.value = "messages"
    messageKeyword.value = ""
    activeMessageIndex.value = null
    scheduleMessageNavPositions()
  }
)

watch(
  () => [props.messages, messageKeyword.value, activeTab.value],
  () => {
    scheduleMessageNavPositions()
  },
  { deep: true }
)

watch(
  () => props.pending,
  () => {
    scheduleMessageNavPositions()
  }
)

onMounted(() => {
  scheduleMessageNavPositions()
})

window.addEventListener("resize", scheduleMessageNavPositions)

onBeforeUnmount(() => {
  window.removeEventListener("resize", scheduleMessageNavPositions)
  window.removeEventListener("pointermove", updateMessageScrollByPointer)
  window.removeEventListener("pointerup", stopMessageScrollDrag)
})

async function copyText(value) {
  await navigator.clipboard.writeText(value || "")
}
</script>

<style scoped lang="less">
.session-drawer {
  position: fixed;
  inset: 0;
  z-index: 30;

  &__overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.2);
  }

  &__panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    display: flex;
    width: min(680px, 100%);
    flex-direction: column;
    border-left: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow: var(--shadow-panel);
  }

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 20px;
    padding: 24px 24px 18px;
    border-bottom: 1px solid var(--color-line);
  }

  &__header p {
    margin: 0 0 8px;
    color: var(--color-text-soft);
    font-size: 0.78rem;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  &__header h2 {
    margin: 0 0 10px;
    font-size: 1.6rem;
    line-height: 1.15;
  }

  &__header span:not([data-emphasis]) {
    color: var(--color-text-muted);
  }

  &__close {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 1.4rem;
    line-height: 1;
  }

  &__tabs {
    display: flex;
    gap: 8px;
    padding: 14px 24px 0;
    border-bottom: 1px solid var(--color-line);
  }

  &__tab {
    position: relative;
    padding: 12px 10px;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &__tab--active {
    color: var(--color-text);
  }

  &__tab--active::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    border-radius: 999px;
    background: var(--color-primary-solid);
  }

  &__body {
    position: relative;
    min-height: 0;
    flex: 1;
  }

  &__content {
    position: absolute;
    inset: 0;
    overflow: auto;
    padding: 24px 68px 24px 24px;
    scrollbar-color: transparent transparent;
    scrollbar-width: thin;
  }

  &__content::-webkit-scrollbar {
    width: 0;
    height: 0;
  }

  &__content::-webkit-scrollbar-thumb,
  &__content::-webkit-scrollbar-track {
    background: transparent;
  }

  &__section {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  &__message-search {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__message-search span:not([data-emphasis]),
  &__info-card span:not([data-emphasis]),
  &__tool span:not([data-emphasis]),
  &__grid span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: 0.76rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  &__message-search input {
    height: 40px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    padding: 0 12px;
  }

  &__message-nav {
    position: absolute;
    top: 24px;
    right: 12px;
    bottom: 24px;
    z-index: 4;
    width: 42px;
    border: 1px solid var(--color-line);
    border-radius: 999px;
    background: var(--color-panel-soft);
    cursor: pointer;
    user-select: none;
  }

  &__message-nav::before {
    content: "";
    position: absolute;
    top: 8px;
    right: 19px;
    bottom: 8px;
    width: 3px;
    border-radius: 999px;
    background: #9caec2;
  }

  &__message-nav-thumb {
    position: absolute;
    right: 15px;
    z-index: 1;
    width: 10px;
    min-height: 28px;
    border: 1px solid #ffffff;
    border-radius: 999px;
    background: rgba(68, 91, 115, 0.3);
    cursor: grab;
    box-shadow: 0 4px 12px rgba(34, 56, 83, 0.16);
  }

  &__message-nav-thumb:active {
    cursor: grabbing;
  }

  &__message-nav-item {
    position: absolute;
    right: 14px;
    z-index: 2;
    width: 14px;
    height: 14px;
    place-items: center;
    border: 1px solid #ffffff;
    border-radius: 50%;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    pointer-events: auto;
    transform: translateY(-50%);
    box-shadow: 0 3px 8px rgba(34, 56, 83, 0.12);
    display: grid;
    justify-content: center;
    align-items: center;
  }

  &__message-nav-item:hover {
    background: var(--color-panel-soft);
    color: var(--color-primary);
  }

  &__message-nav-item--user {
    background: var(--color-panel-soft);
    color: var(--color-primary);
  }

  &__message-nav-item--assistant {
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
  }

  &__message-nav-item--tool {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  &__message-nav-item--system {
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
  }

  &__message-nav-item--active {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 2px rgba(68, 91, 115, 0.18);
  }

  &__info-card,
  &__tool,
  &__grid article {
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__info-card button,
  &__grid button {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-primary);
    cursor: pointer;
    text-align: left;
  }

  &__chat-message {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  &__chat-message--user {
    flex-direction: row-reverse;
  }

  &__chat-avatar {
    display: grid;
    width: 32px;
    height: 32px;
    flex: 0 0 32px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 50%;
    background: var(--color-panel);
    color: var(--color-text-muted);
  }

  &__chat-message--user &__chat-avatar {
    border-color: var(--color-line-strong);
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  &__chat-message--assistant &__chat-avatar {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    color: var(--color-primary);
  }

  &__chat-message--tool &__chat-avatar {
    border-color: var(--color-warning-line);
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  &__chat-body {
    display: flex;
    min-width: 0;
    max-width: 86%;
    flex-direction: column;
    gap: 6px;
  }

  &__chat-message--user &__chat-body {
    align-items: flex-end;
  }

  &__chat-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-text-soft);
    font-size: 0.76rem;
  }

  &__chat-meta [data-emphasis] {
    color: var(--color-text-muted);
  }

  &__chat-meta button {
    display: grid;
    width: 24px;
    height: 24px;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-soft);
    cursor: pointer;
  }

  &__chat-meta button:hover {
    background: var(--color-panel-soft);
    color: var(--color-primary);
  }

  &__chat-bubble {
    padding: 12px 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 8px 20px rgba(34, 56, 83, 0.05);
  }

  &__chat-message--user &__chat-bubble {
    border-color: var(--color-line);
    background: var(--color-panel-soft);
  }

  &__chat-message--tool &__chat-bubble {
    border-color: var(--color-warning-line);
    background: var(--color-warning-soft);
  }

  &__chat-message--system &__chat-bubble {
    border-style: dashed;
    background: var(--color-panel-soft);
  }

  &__chat-bubble p {
    margin: 0;
    color: var(--color-text);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 0.84rem;
    line-height: 1.65;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__tool pre {
    overflow: auto;
    margin: 0;
    color: var(--color-text);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 0.84rem;
    line-height: 1.65;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__info-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  &__info-card p {
    margin: 0;
    line-height: 1.7;
  }

  &__tool summary {
    cursor: pointer;
  }

  &__tool div {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  &__grid article {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &__grid [data-emphasis],
  &__grid button {
    font-size: 0.92rem;
    line-height: 1.5;
    word-break: break-all;
  }

  &__loading,
  &__empty {
    display: grid;
    min-height: 180px;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    color: var(--color-text-muted);
  }
}
</style>

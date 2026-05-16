<template>
  <section class="sessions-view">
    <header class="sessions-view__toolbar">
      <div>
        <p class="sessions-view__eyebrow">AI Workflow Sessions</p>
        <h1>Sessions 管理</h1>
      </div>

      <div class="sessions-view__toolbar-actions">
        <button class="action-button" type="button" @click="openRecycle">
          <RotateCcw class="action-button__icon" :size="16" />
          回收站
        </button>
        <button class="action-button" type="button" @click="$emit('refresh')">
          <RefreshCw class="action-button__icon" :size="16" />
          刷新扫描
        </button>
      </div>
    </header>

    <div class="sessions-view__filters">
      <label class="sessions-view__search">
        <span>搜索</span>
        <input
          v-model.trim="searchQuery"
          type="text"
          placeholder="title / messages / project / tool calls / files"
          @keydown.enter="searchRemoteSessions"
        />
      </label>

      <label class="sessions-view__select">
        <span>CLI</span>
        <select v-model="cliFilter">
          <option value="all">全部</option>
          <option v-for="item in cliOptions" :key="item" :value="item">
            {{ item }}
          </option>
        </select>
      </label>
    </div>

    <div class="sessions-view__meta">
      <span
        >{{ filteredSessions.length }} / {{ sessions.length }} 个 Session</span
      >
      <span>Filesystem Aggregation + 按需加载 Messages</span>
    </div>

    <div v-if="filteredSessions.length" class="sessions-view__layout">
      <div class="sessions-view__list">
        <article
          v-for="session in filteredSessions"
          :key="session.id"
          class="sessions-view__card"
          @click="selectSession(session)"
        >
          <div class="sessions-view__card-main">
            <div class="sessions-view__title-row">
              <AiIcon
                v-if="iconMap[session.cli]"
                class="sessions-view__cli-icon"
                :name="iconMap[session.cli]"
                :alt="`${session.cliName} 图标`"
              />
              <h3>{{ session.title }}</h3>
              <span>{{ session.cliName || session.cli }}</span>
            </div>
            <p>{{ session.summary || "暂无摘要，点击查看工作流消息。" }}</p>
            <div class="sessions-view__card-meta">
              <span>{{ session.projectName || "未识别项目" }}</span>
              <span>{{ session.model || "未识别模型" }}</span>
              <span>{{ session.messageCount }} messages</span>
              <span>{{ formatDateTime(session.updatedAt) }}</span>
            </div>
          </div>

          <div class="sessions-view__card-actions">
            <button
              class="icon-button icon-button--danger"
              type="button"
              title="移动到回收站"
              @click.stop="deleteSession(session)"
            >
              <Trash2 :size="15" />
            </button>
          </div>
        </article>
      </div>

      <SessionDrawer
        :messages="selectedMessages"
        :pending="detailPending"
        :session="selectedSession"
        @close="selectedSession = null"
        @open-path="$emit('open-path', $event)"
      />
    </div>

    <div v-else class="sessions-view__empty">
      <h2>没有匹配的 Session</h2>
      <p>系统会扫描已检测 CLI 的本地 Session 目录，生成统一索引。</p>
    </div>

    <div v-if="showRecycle" class="sessions-view__modal">
      <div
        class="sessions-view__modal-overlay"
        @click="showRecycle = false"
      ></div>
      <section class="sessions-view__modal-panel">
        <header class="sessions-view__modal-header">
          <div>
            <h2>Session 回收站</h2>
            <p>{{ recycledSessions.length }} 个已移动的 Session</p>
          </div>
          <div class="sessions-view__modal-actions">
            <button
              v-if="paths.sessionRecycleDir"
              class="action-button"
              type="button"
              @click="$emit('open-path', paths.sessionRecycleDir)"
            >
              打开目录
            </button>
            <button
              class="icon-button"
              type="button"
              @click="showRecycle = false"
            >
              ×
            </button>
          </div>
        </header>

        <div v-if="recyclePending" class="sessions-view__recycle-empty">
          正在读取回收站...
        </div>
        <div
          v-else-if="!recycledSessions.length"
          class="sessions-view__recycle-empty"
        >
          回收站为空。
        </div>
        <div v-else class="sessions-view__recycle-list">
          <article
            v-for="session in recycledSessions"
            :key="session.id"
            class="sessions-view__recycle-card"
          >
            <div>
              <div class="sessions-view__title-row">
                <AiIcon
                  v-if="iconMap[session.cli]"
                  class="sessions-view__cli-icon"
                  :name="iconMap[session.cli]"
                  :alt="`${session.cliName} 图标`"
                />
                <h3>{{ session.title }}</h3>
                <span>{{ session.cliName || session.cli }}</span>
              </div>
              <p>{{ session.originalPath }}</p>
              <div class="sessions-view__card-meta">
                <span>{{ session.projectName || "未识别项目" }}</span>
                <span>{{ formatDateTime(session.recycledAt) }}</span>
              </div>
            </div>

            <div class="sessions-view__card-actions">
              <button
                class="action-button"
                type="button"
                @click="restoreRecycledSession(session)"
              >
                还原
              </button>
              <button
                class="icon-button icon-button--danger"
                type="button"
                title="永久删除"
                @click="purgeRecycledSession(session)"
              >
                <Trash2 :size="15" />
              </button>
            </div>
          </article>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup>
import Fuse from "fuse.js"
import { computed, ref, watch } from "vue"
import { RefreshCw, RotateCcw, Trash2 } from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import { formatDateTime } from "@/utils/formatters"
import SessionDrawer from "./components/SessionDrawer.vue"

const props = defineProps({
  paths: {
    type: Object,
    required: true
  },
  sessions: {
    type: Array,
    required: true
  }
})

const emit = defineEmits(["delete-session", "open-path", "refresh"])

const iconMap = {
  claude: "claude.svg",
  codex: "codex.svg",
  gemini: "geminicli.svg",
  opencode: "opencode.svg"
}

const searchQuery = ref("")
const cliFilter = ref("all")
const remoteSessions = ref(null)
const selectedSession = ref(null)
const selectedMessages = ref([])
const detailPending = ref(false)
const showRecycle = ref(false)
const recyclePending = ref(false)
const recycledSessions = ref([])

const cliOptions = computed(() => {
  return Array.from(
    new Set(props.sessions.map((item) => item.cliName || item.cli))
  )
})

const searchedSessions = computed(() => {
  if (remoteSessions.value) {
    return remoteSessions.value
  }

  if (!searchQuery.value) {
    return props.sessions
  }

  return new Fuse(props.sessions, {
    keys: [
      "title",
      "summary",
      "projectName",
      "projectPath",
      "model",
      "cliName"
    ],
    threshold: 0.36
  })
    .search(searchQuery.value)
    .map((item) => item.item)
})

const filteredSessions = computed(() => {
  return searchedSessions.value.filter((item) => {
    if (cliFilter.value === "all") {
      return true
    }

    return (item.cliName || item.cli) === cliFilter.value
  })
})

watch(searchQuery, (value) => {
  remoteSessions.value = null

  if (!value) {
    return
  }

  clearTimeout(searchRemoteSessions.timer)
  searchRemoteSessions.timer = setTimeout(() => {
    searchRemoteSessions()
  }, 360)
})

watch(
  () => props.sessions,
  () => {
    remoteSessions.value = null

    if (
      selectedSession.value &&
      !props.sessions.find((item) => item.id === selectedSession.value.id)
    ) {
      selectedSession.value = null
      selectedMessages.value = []
    }
  }
)

async function searchRemoteSessions() {
  if (!searchQuery.value) {
    remoteSessions.value = null
    return
  }

  remoteSessions.value = await window.aiManager.searchSessions({
    query: searchQuery.value
  })
}

async function selectSession(session) {
  selectedSession.value = session
  detailPending.value = true
  selectedMessages.value = []

  try {
    selectedMessages.value = await window.aiManager.loadSessionMessages({
      sessionId: session.id
    })
  } finally {
    detailPending.value = false
  }
}

function deleteSession(session) {
  const shouldContinue = window.confirm(
    "删除后会将 CLI 原始 Session 移动到 AI Manager 回收站，是否继续？"
  )

  if (shouldContinue) {
    emit("delete-session", session.id)
  }
}

async function openRecycle() {
  showRecycle.value = true
  await loadRecycle()
}

async function loadRecycle() {
  recyclePending.value = true

  try {
    recycledSessions.value = await window.aiManager.listRecycledSessions()
  } finally {
    recyclePending.value = false
  }
}

async function restoreRecycledSession(session) {
  await window.aiManager.restoreSession({ sessionId: session.id })
  await loadRecycle()
  emit("refresh")
}

async function purgeRecycledSession(session) {
  const shouldContinue = window.confirm("永久删除后无法恢复，是否继续？")

  if (!shouldContinue) {
    return
  }

  await window.aiManager.purgeSession({ sessionId: session.id })
  await loadRecycle()
}
</script>

<style scoped lang="less">
.sessions-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 10px;
  overflow: hidden;

  &__toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
  }

  &__eyebrow {
    margin: 0 0 5px;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  &__toolbar h1 {
    margin: 0;
    font-size: 1.38rem;
    line-height: 1.2;
  }

  &__toolbar-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  &__filters {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 220px;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  &__search,
  &__select {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  &__search span,
  &__select span {
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
  }

  &__search input,
  &__select select {
    height: 38px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    padding: 0 11px;
    color: var(--color-text);
    font: inherit;
    font-size: 0.88rem;
  }

  &__meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
  }

  &__layout {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  &__list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    overflow: hidden;
    background: var(--color-panel);
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  &__card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 14px;
    align-items: center;
    min-height: 62px;
    padding: 9px 14px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
    cursor: pointer;
  }

  &__card:hover {
    background: var(--color-panel-soft);
  }

  &__card-main {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 5px;
  }

  &__title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__title-row h3 {
    overflow: hidden;
    margin: 0;
    color: var(--color-text);
    font-size: 0.92rem;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__title-row span {
    flex: 0 0 auto;
    color: var(--color-text-soft);
    font-size: 0.76rem;
  }

  &__cli-icon {
    width: 20px;
    height: 20px;
    flex: 0 0 20px;
  }

  &__card p {
    overflow: hidden;
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__card-meta {
    display: flex;
    gap: 10px;
    overflow: hidden;
    color: var(--color-text-soft);
    font-size: 0.76rem;
    white-space: nowrap;
  }

  &__card-actions {
    display: flex;
    gap: 6px;
  }

  &__modal {
    position: fixed;
    inset: 0;
    z-index: 34;
  }

  &__modal-overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.2);
  }

  &__modal-panel {
    position: absolute;
    top: 72px;
    left: 50%;
    display: flex;
    width: 760px;
    max-height: 720px;
    flex-direction: column;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: var(--shadow-panel);
    transform: translateX(-50%);
  }

  &__modal-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 18px;
    border-bottom: 1px solid var(--color-line);
  }

  &__modal-header h2 {
    margin: 0 0 6px;
    font-size: 1.22rem;
  }

  &__modal-header p {
    margin: 0;
    color: var(--color-text-muted);
  }

  &__modal-actions {
    display: flex;
    gap: 8px;
  }

  &__recycle-list {
    display: flex;
    overflow: auto;
    flex-direction: column;
    padding: 12px;
  }

  &__recycle-card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 16px;
    align-items: center;
    padding: 14px;
    border-bottom: 1px solid var(--color-line);
  }

  &__recycle-card:last-child {
    border-bottom: 0;
  }

  &__recycle-card p {
    overflow: hidden;
    margin: 6px 0;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__recycle-empty {
    display: grid;
    min-height: 220px;
    place-items: center;
    color: var(--color-text-muted);
  }

  &__empty {
    display: grid;
    flex: 1;
    min-height: 0;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    text-align: center;
  }

  &__empty h2 {
    margin: 0 0 10px;
    font-size: 1.28rem;
  }

  &__empty p {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.88rem;
  }
}

.action-button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #fbfcfd;
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.88rem;
  font-weight: 600;

  &__icon {
    flex: 0 0 auto;
  }
}

.icon-button {
  display: inline-grid;
  width: 30px;
  height: 30px;
  place-items: center;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-text-muted);
  cursor: pointer;

  &--danger {
    color: var(--color-danger);
  }
}

.action-button:hover,
.icon-button:hover {
  border-color: #b9ccda;
  background: var(--color-primary-soft);
}
</style>

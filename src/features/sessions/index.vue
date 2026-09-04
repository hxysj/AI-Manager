<template>
  <section class="sessions-view">
    <header class="sessions-view-toolbar">
      <div class="sessions-view-heading">
        <p class="sessions-view-eyebrow">AI Workflow Sessions</p>
        <h1 class="sessions-view-title">Sessions 管理</h1>
      </div>

      <div class="sessions-view-toolbar-actions">
        <button class="action-button" type="button" @click="openRecycle">
          <RotateCcw class="action-button-icon" :size="16" />
          回收站
        </button>
        <button class="action-button" type="button" @click="$emit('refresh')">
          <RefreshCw class="action-button-icon" :size="16" />
          刷新扫描
        </button>
      </div>
    </header>

    <div class="sessions-view-filters">
      <label class="sessions-view-search">
        <span class="sessions-view-filter-label">搜索</span>
        <input
          v-model.trim="searchQuery"
          class="sessions-view-search-input"
          type="text"
          placeholder="title / messages / project / tool calls / files"
          @keydown.enter="searchRemoteSessions"
        />
      </label>

      <label class="sessions-view-select">
        <span class="sessions-view-filter-label">CLI</span>
        <select v-model="cliFilter" class="sessions-view-select-control">
          <option value="all">全部</option>
          <option v-for="item in cliOptions" :key="item" :value="item">
            {{ item }}
          </option>
        </select>
      </label>

      <div
        class="sessions-view-project-filter"
        @click.stop
      >
        <span class="sessions-view-filter-label">项目</span>
        <button
          class="sessions-view-project-button"
          type="button"
          @click="projectDropdownOpen = !projectDropdownOpen"
        >
          {{ selectedProjectOption.label }}
        </button>
        <div
          v-if="projectDropdownOpen"
          class="sessions-view-project-menu"
        >
          <button
            class="sessions-view-project-item"
            type="button"
            @click="selectProjectFilter('all')"
          >
            <strong class="sessions-view-project-name">全部项目</strong>
            <span class="sessions-view-project-path">
              显示当前范围内的全部 Session
            </span>
          </button>
          <button
            v-for="item in projectOptions"
            :key="item.value"
            class="sessions-view-project-item"
            type="button"
            @click="selectProjectFilter(item.value)"
          >
            <strong class="sessions-view-project-name">{{ item.name }}</strong>
            <span class="sessions-view-project-path">{{ item.path }}</span>
          </button>
        </div>
      </div>
    </div>

    <div class="sessions-view-meta">
      <span
        class="sessions-view-meta-text"
        >{{ filteredSessions.length }} / {{ sessions.length }} 个 Session</span
      >
      <span v-if="filteredSessions.length" class="sessions-view-meta-text"
        >第 {{ currentPage }} / {{ pageCount }} 页 · 当前 {{ pageStart }}-{{
          pageEnd
        }}</span
      >
      <span v-else class="sessions-view-meta-text">
        Filesystem Aggregation + 按需加载 Messages
      </span>
    </div>

    <div v-if="filteredSessions.length" class="sessions-view-layout">
      <div class="sessions-view-list">
        <article
          v-for="session in pagedSessions"
          :key="session.id"
          class="sessions-view-card"
          @click="selectSession(session)"
        >
          <div class="sessions-view-card-main">
            <div class="sessions-view-title-row">
              <AiIcon
                v-if="iconMap[session.cli]"
                class="sessions-view-cli-icon"
                :name="iconMap[session.cli]"
                :alt="`${session.cliName} 图标`"
              />
              <h3 class="sessions-view-card-title">{{ session.title }}</h3>
              <span class="sessions-view-card-cli">
                {{ session.cliName || session.cli }}
              </span>
            </div>
            <p class="sessions-view-card-summary">
              {{ session.summary || "暂无摘要，点击查看工作流消息。" }}
            </p>
            <div class="sessions-view-card-meta">
              <span class="sessions-view-card-meta-text">
                {{ session.projectName || "未识别项目" }}
              </span>
              <span class="sessions-view-card-meta-text">
                {{ session.model || "未识别模型" }}
              </span>
              <span class="sessions-view-card-meta-text">
                {{ session.messageCount }} messages
              </span>
              <span class="sessions-view-card-meta-text">
                {{ formatDateTime(session.updatedAt) }}
              </span>
            </div>
          </div>

          <div class="sessions-view-card-actions">
            <button
              class="icon-button icon-button-danger"
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

    <div v-if="filteredSessions.length" class="sessions-view-pagination">
      <label class="sessions-view-page-size">
        <span class="sessions-view-page-label">每页</span>
        <select v-model.number="pageSize" class="sessions-view-page-select">
          <option v-for="item in pageSizeOptions" :key="item" :value="item">
            {{ item }}
          </option>
        </select>
      </label>

      <div class="sessions-view-page-actions">
        <button
          class="icon-button"
          type="button"
          title="上一页"
          :disabled="currentPage === 1"
          @click="currentPage -= 1"
        >
          <ChevronLeft :size="15" />
        </button>
        <span class="sessions-view-page-label">{{ currentPage }} / {{ pageCount }}</span>
        <button
          class="icon-button"
          type="button"
          title="下一页"
          :disabled="currentPage === pageCount"
          @click="currentPage += 1"
        >
          <ChevronRight :size="15" />
        </button>
      </div>
    </div>

    <div v-else class="sessions-view-empty">
      <h2 class="sessions-view-empty-title">没有匹配的 Session</h2>
      <p class="sessions-view-empty-desc">
        系统会扫描已检测 CLI 的本地 Session 目录，生成统一索引。
      </p>
    </div>

    <div v-if="showRecycle" class="sessions-view-modal">
      <div
        class="sessions-view-modal-overlay"
        @click="showRecycle = false"
      ></div>
      <section class="sessions-view-modal-panel">
        <header class="sessions-view-modal-header">
          <div class="sessions-view-modal-title">
            <span class="sessions-view-modal-eyebrow">Recycle Bin</span>
            <h2 class="sessions-view-modal-heading">Session 回收站</h2>
            <p class="sessions-view-modal-count">
              <strong class="sessions-view-modal-count-value">
                {{ filteredRecycledSessions.length }}
              </strong>
              / {{ recycledSessions.length }} 个已移动的 Session
            </p>
          </div>
          <div class="sessions-view-modal-actions">
            <button
              v-if="paths.sessionRecycleDir"
              class="action-button"
              type="button"
              @click="$emit('open-path', paths.sessionRecycleDir)"
            >
              <FolderOpen class="action-button-icon" :size="15" />
              打开目录
            </button>
            <button
              class="icon-button"
              type="button"
              title="关闭"
              @click="showRecycle = false"
            >
              <X :size="15" />
            </button>
          </div>
        </header>

        <div
          v-if="!recyclePending && recycledSessions.length"
          class="sessions-view-recycle-tools"
        >
          <label class="sessions-view-recycle-search">
            <span class="sessions-view-recycle-filter-label">搜索</span>
            <input
              v-model.trim="recycleSearchQuery"
              class="sessions-view-recycle-search-input"
              type="text"
              placeholder="title / project / path / cli"
            />
          </label>

          <label class="sessions-view-recycle-select">
            <span class="sessions-view-recycle-filter-label">CLI</span>
            <select
              v-model="recycleCliFilter"
              class="sessions-view-recycle-select-control"
            >
              <option value="all">全部</option>
              <option
                v-for="item in recycleCliOptions"
                :key="item"
                :value="item"
              >
                {{ item }}
              </option>
            </select>
          </label>

          <label class="sessions-view-recycle-select">
            <span class="sessions-view-recycle-filter-label">项目</span>
            <select
              v-model="recycleProjectFilter"
              class="sessions-view-recycle-select-control"
            >
              <option value="all">全部项目</option>
              <option
                v-for="item in recycleProjectOptions"
                :key="item.value"
                :value="item.value"
              >
                {{ item.name }}
              </option>
            </select>
          </label>

          <button
            class="action-button"
            type="button"
            @click="resetRecycleFilters"
          >
            <RefreshCw class="action-button-icon" :size="15" />
            重置
          </button>
        </div>

        <div v-if="recyclePending" class="sessions-view-recycle-empty">
          <RotateCcw class="sessions-view-recycle-empty-icon" :size="24" />
          <span class="sessions-view-recycle-empty-text">
            正在读取回收站...
          </span>
        </div>
        <div
          v-else-if="!recycledSessions.length"
          class="sessions-view-recycle-empty"
        >
          <RotateCcw class="sessions-view-recycle-empty-icon" :size="24" />
          <span class="sessions-view-recycle-empty-text">回收站为空。</span>
        </div>
        <div
          v-else-if="!filteredRecycledSessions.length"
          class="sessions-view-recycle-empty"
        >
          <RotateCcw class="sessions-view-recycle-empty-icon" :size="24" />
          <span class="sessions-view-recycle-empty-text">
            没有匹配的回收站 Session。
          </span>
        </div>
        <div v-else class="sessions-view-recycle-list">
          <article
            v-for="session in pagedRecycledSessions"
            :key="session.id"
            class="sessions-view-recycle-card"
          >
            <div class="sessions-view-recycle-main">
              <div class="sessions-view-recycle-title">
                <AiIcon
                  v-if="iconMap[session.cli]"
                  class="sessions-view-cli-icon"
                  :name="iconMap[session.cli]"
                  :alt="`${session.cliName} 图标`"
                />
                <h3 class="sessions-view-recycle-heading">
                  {{ session.title }}
                </h3>
                <span class="sessions-view-recycle-cli">{{
                  session.cliName || session.cli
                }}</span>
              </div>
              <p
                class="sessions-view-recycle-path"
                :title="session.originalPath"
              >
                {{ session.originalPath }}
              </p>
              <div class="sessions-view-recycle-meta">
                <span class="sessions-view-recycle-meta-text">
                  {{ session.projectName || "未识别项目" }}
                </span>
                <span class="sessions-view-recycle-meta-text">
                  删除于 {{ formatDateTime(session.recycledAt) }}
                </span>
              </div>
            </div>

            <div class="sessions-view-recycle-actions">
              <button
                class="action-button"
                type="button"
                @click="restoreRecycledSession(session)"
              >
                <RotateCcw class="action-button-icon" :size="15" />
                还原
              </button>
              <button
                class="icon-button icon-button-danger"
                type="button"
                title="永久删除"
                @click="purgeRecycledSession(session)"
              >
                <Trash2 :size="15" />
              </button>
            </div>
          </article>
        </div>

        <div
          v-if="!recyclePending && filteredRecycledSessions.length"
          class="sessions-view-recycle-pagination"
        >
          <span class="sessions-view-recycle-page-info">
            第 {{ recycleCurrentPage }} / {{ recyclePageCount }} 页 · 当前
            {{ recyclePageStart }}-{{ recyclePageEnd }}
          </span>

          <div class="sessions-view-recycle-page-controls">
            <label class="sessions-view-recycle-page-size">
              <span class="sessions-view-recycle-page-label">每页</span>
              <select
                v-model.number="recyclePageSize"
                class="sessions-view-recycle-page-select"
              >
                <option
                  v-for="item in pageSizeOptions"
                  :key="item"
                  :value="item"
                >
                  {{ item }}
                </option>
              </select>
            </label>

            <div class="sessions-view-recycle-page-actions">
              <button
                class="icon-button"
                type="button"
                title="上一页"
                :disabled="recycleCurrentPage === 1"
                @click="recycleCurrentPage -= 1"
              >
                <ChevronLeft :size="15" />
              </button>
              <span class="sessions-view-recycle-page-label">
                {{ recycleCurrentPage }} / {{ recyclePageCount }}
              </span>
              <button
                class="icon-button"
                type="button"
                title="下一页"
                :disabled="recycleCurrentPage === recyclePageCount"
                @click="recycleCurrentPage += 1"
              >
                <ChevronRight :size="15" />
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup>
import Fuse from "fuse.js"
import { computed, onBeforeUnmount, ref, watch } from "vue"
import {
  ChevronLeft,
  ChevronRight,
  FolderOpen,
  RefreshCw,
  RotateCcw,
  Trash2,
  X
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import { sessionApi } from "@/api"
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
  codex: "codex.svg"
  // 当前版本暂不启用 Gemini 和 OpenCode。
  // gemini: "geminicli.svg",
  // opencode: "opencode.svg"
}

const searchQuery = ref("")
const cliFilter = ref("all")
const projectFilter = ref("all")
const projectDropdownOpen = ref(false)
const remoteSessions = ref(null)
const selectedSession = ref(null)
const selectedMessages = ref([])
const detailPending = ref(false)
const showRecycle = ref(false)
const recyclePending = ref(false)
const recycledSessions = ref([])
const recycleSearchQuery = ref("")
const recycleCliFilter = ref("all")
const recycleProjectFilter = ref("all")
const recycleCurrentPage = ref(1)
const recyclePageSize = ref(20)
const currentPage = ref(1)
const pageSize = ref(20)
const pageSizeOptions = [10, 20, 50, 100]

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

const cliFilteredSessions = computed(() => {
  return searchedSessions.value.filter((item) => {
    if (cliFilter.value === "all") {
      return true
    }

    return (item.cliName || item.cli) === cliFilter.value
  })
})

const projectOptions = computed(() => {
  const options = new Map()

  for (const session of cliFilteredSessions.value) {
    const value = getProjectFilterValue(session)

    if (!options.has(value)) {
      options.set(value, {
        value,
        name: resolveProjectName(session.projectPath),
        path: value === "unknown-project" ? "未识别项目路径" : value
      })
    }
  }

  return Array.from(options.values()).sort((left, right) =>
    left.name.localeCompare(right.name, "zh-Hans-CN")
  )
})

const selectedProjectOption = computed(() => {
  if (projectFilter.value === "all") {
    return { label: "全部项目" }
  }

  const option = projectOptions.value.find(
    item => item.value === projectFilter.value
  )

  return { label: option?.name || "全部项目" }
})

const filteredSessions = computed(() => {
  return cliFilteredSessions.value.filter((item) => {
    if (projectFilter.value === "all") {
      return true
    }

    return getProjectFilterValue(item) === projectFilter.value
  })
})

const pageCount = computed(() => {
  return Math.max(1, Math.ceil(filteredSessions.value.length / pageSize.value))
})

const pageStart = computed(() => {
  if (!filteredSessions.value.length) {
    return 0
  }

  return (currentPage.value - 1) * pageSize.value + 1
})

const pageEnd = computed(() => {
  return Math.min(
    currentPage.value * pageSize.value,
    filteredSessions.value.length
  )
})

const pagedSessions = computed(() => {
  return filteredSessions.value.slice(pageStart.value - 1, pageEnd.value)
})

const recycleCliOptions = computed(() => {
  return Array.from(
    new Set(recycledSessions.value.map(item => item.cliName || item.cli))
  )
})

const searchedRecycledSessions = computed(() => {
  if (!recycleSearchQuery.value) {
    return recycledSessions.value
  }

  return new Fuse(recycledSessions.value, {
    keys: [
      "title",
      "summary",
      "projectName",
      "projectPath",
      "originalPath",
      "model",
      "cliName",
      "cli"
    ],
    threshold: 0.36
  })
    .search(recycleSearchQuery.value)
    .map(item => item.item)
})

const cliFilteredRecycledSessions = computed(() => {
  return searchedRecycledSessions.value.filter(item => {
    if (recycleCliFilter.value === "all") {
      return true
    }

    return (item.cliName || item.cli) === recycleCliFilter.value
  })
})

const recycleProjectOptions = computed(() => {
  const options = new Map()

  for (const session of cliFilteredRecycledSessions.value) {
    const value = getProjectFilterValue(session)

    if (!options.has(value)) {
      options.set(value, {
        value,
        name: resolveProjectName(session.projectPath),
        path: value === "unknown-project" ? "未识别项目路径" : value
      })
    }
  }

  return Array.from(options.values()).sort((left, right) =>
    left.name.localeCompare(right.name, "zh-Hans-CN")
  )
})

const filteredRecycledSessions = computed(() => {
  return cliFilteredRecycledSessions.value.filter(item => {
    if (recycleProjectFilter.value === "all") {
      return true
    }

    return getProjectFilterValue(item) === recycleProjectFilter.value
  })
})

const recyclePageCount = computed(() => {
  return Math.max(
    1,
    Math.ceil(filteredRecycledSessions.value.length / recyclePageSize.value)
  )
})

const recyclePageStart = computed(() => {
  if (!filteredRecycledSessions.value.length) {
    return 0
  }

  return (recycleCurrentPage.value - 1) * recyclePageSize.value + 1
})

const recyclePageEnd = computed(() => {
  return Math.min(
    recycleCurrentPage.value * recyclePageSize.value,
    filteredRecycledSessions.value.length
  )
})

const pagedRecycledSessions = computed(() => {
  return filteredRecycledSessions.value.slice(
    recyclePageStart.value - 1,
    recyclePageEnd.value
  )
})

watch(searchQuery, (value) => {
  remoteSessions.value = null
  currentPage.value = 1

  if (!value) {
    return
  }

  clearTimeout(searchRemoteSessions.timer)
  searchRemoteSessions.timer = setTimeout(() => {
    searchRemoteSessions()
  }, 360)
})

watch([cliFilter, projectFilter], () => {
  currentPage.value = 1
})

watch(cliFilter, () => {
  projectDropdownOpen.value = false
})

watch(projectOptions, (options) => {
  if (projectFilter.value === "all") {
    return
  }

  if (!options.some((item) => item.value === projectFilter.value)) {
    projectFilter.value = "all"
  }
})

watch(pageSize, () => {
  currentPage.value = 1
})

watch([recycleSearchQuery, recycleCliFilter, recycleProjectFilter], () => {
  recycleCurrentPage.value = 1
})

watch(recycleProjectOptions, options => {
  if (recycleProjectFilter.value === "all") {
    return
  }

  if (!options.some(item => item.value === recycleProjectFilter.value)) {
    recycleProjectFilter.value = "all"
  }
})

watch(recyclePageSize, () => {
  recycleCurrentPage.value = 1
})

watch(
  () => filteredSessions.value.length,
  () => {
    if (currentPage.value > pageCount.value) {
      currentPage.value = pageCount.value
    }
  }
)

watch(
  () => filteredRecycledSessions.value.length,
  () => {
    if (recycleCurrentPage.value > recyclePageCount.value) {
      recycleCurrentPage.value = recyclePageCount.value
    }
  }
)

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

  remoteSessions.value = await sessionApi.searchSessions({
    query: searchQuery.value
  })
  currentPage.value = 1
}

function normalizeProjectPath(value) {
  return String(value || "")
    .trim()
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
}

function getProjectFilterValue(session) {
  return normalizeProjectPath(session.projectPath) || "unknown-project"
}

function resolveProjectName(projectPath) {
  const text = normalizeProjectPath(projectPath)
  const parts = text.split("/").filter(Boolean)

  return parts[parts.length - 1] || "未识别项目"
}

function selectProjectFilter(value) {
  projectFilter.value = value
  projectDropdownOpen.value = false
}

async function selectSession(session) {
  selectedSession.value = session
  detailPending.value = true
  selectedMessages.value = []

  try {
    selectedMessages.value = await sessionApi.loadSessionMessages({
      sessionId: session.id
    })
  } finally {
    detailPending.value = false
  }
}

function deleteSession(session) {
  const shouldContinue = window.confirm(
    "删除后会将 CLI 原始 Session 移动到 Monkey Thief 回收站，是否继续？"
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
    recycledSessions.value = await sessionApi.listRecycledSessions()
  } finally {
    recyclePending.value = false
  }
}

function resetRecycleFilters() {
  recycleSearchQuery.value = ""
  recycleCliFilter.value = "all"
  recycleProjectFilter.value = "all"
  recycleCurrentPage.value = 1
}

async function restoreRecycledSession(session) {
  await sessionApi.restoreSession({ sessionId: session.id })
  await loadRecycle()
  emit("refresh")
}

async function purgeRecycledSession(session) {
  const shouldContinue = window.confirm("永久删除后无法恢复，是否继续？")

  if (!shouldContinue) {
    return
  }

  await sessionApi.purgeSession({ sessionId: session.id })
  await loadRecycle()
}

function closeProjectDropdown() {
  projectDropdownOpen.value = false
}

document.addEventListener("click", closeProjectDropdown)

onBeforeUnmount(() => {
  document.removeEventListener("click", closeProjectDropdown)
})
</script>

<style scoped lang="less">
.sessions-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 10px;
  overflow: hidden;

  .sessions-view-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;

    .sessions-view-heading {
      .sessions-view-eyebrow {
        margin: 0 0 5px;
        color: var(--color-text-soft);
        font-size: 0.72rem;
        font-weight: 700;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .sessions-view-title {
        margin: 0;
        font-size: 1.38rem;
        line-height: 1.2;
      }
    }

    .sessions-view-toolbar-actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
      justify-content: flex-end;
    }
  }

  .sessions-view-filters {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 180px 280px;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);

    .sessions-view-search,
    .sessions-view-select,
    .sessions-view-project-filter {
      display: flex;
      position: relative;
      flex-direction: column;
      gap: 6px;
    }

    .sessions-view-filter-label {
      color: var(--color-text-muted);
      font-size: 0.74rem;
      font-weight: 700;
    }

    .sessions-view-search-input,
    .sessions-view-select-control,
    .sessions-view-project-button {
      height: 38px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      padding: 0 11px;
      color: var(--color-text);
      font: inherit;
      font-size: 0.88rem;
    }

    .sessions-view-project-filter {
      .sessions-view-project-button {
        overflow: hidden;
        cursor: pointer;
        text-align: left;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .sessions-view-project-menu {
        position: absolute;
        top: 64px;
        right: 0;
        z-index: 20;
        display: flex;
        width: 420px;
        max-height: 320px;
        flex-direction: column;
        overflow: auto;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        box-shadow: var(--shadow-panel);

        .sessions-view-project-item {
          display: flex;
          flex-direction: column;
          gap: 5px;
          padding: 10px 12px;
          border: 0;
          border-bottom: 1px solid var(--color-line);
          background: transparent;
          color: var(--color-text);
          cursor: pointer;
          text-align: left;

          .sessions-view-project-name {
            font-size: 0.86rem;
            line-height: 1.3;
          }

          .sessions-view-project-path {
            color: var(--color-text-muted);
            font-family: "Cascadia Code", Consolas, monospace;
            font-size: 0.76rem;
            line-height: 1.45;
            white-space: normal;
            word-break: break-all;
          }
        }

        .sessions-view-project-item:last-child {
          border-bottom: 0;
        }

        .sessions-view-project-item:hover {
          background: var(--color-panel-soft);
        }
      }
    }
  }

  .sessions-view-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
  }

  .sessions-view-layout {
    flex: 1;
    min-height: 0;
    overflow: auto;

    .sessions-view-list {
      display: flex;
      flex-direction: column;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      overflow: hidden;
      background: var(--color-panel);
      box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);

      .sessions-view-card {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 14px;
        align-items: center;
        min-height: 62px;
        padding: 9px 14px;
        border-bottom: 1px solid var(--color-line);
        background: var(--color-panel);
        cursor: pointer;

        .sessions-view-card-main {
          display: flex;
          min-width: 0;
          flex-direction: column;
          gap: 5px;

          .sessions-view-title-row {
            display: flex;
            align-items: center;
            gap: 8px;

            .sessions-view-cli-icon {
              width: 20px;
              height: 20px;
              flex: 0 0 20px;
            }

            .sessions-view-card-title {
              overflow: hidden;
              margin: 0;
              color: var(--color-text);
              font-size: 0.92rem;
              line-height: 1.2;
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .sessions-view-card-cli {
              flex: 0 0 auto;
              color: var(--color-text-soft);
              font-size: 0.76rem;
            }
          }

          .sessions-view-card-summary {
            overflow: hidden;
            margin: 0;
            color: var(--color-text-muted);
            font-size: 0.78rem;
            line-height: 1.35;
            text-overflow: ellipsis;
            white-space: nowrap;
          }

          .sessions-view-card-meta {
            display: flex;
            gap: 10px;
            overflow: hidden;
            color: var(--color-text-soft);
            font-size: 0.76rem;
            white-space: nowrap;
          }
        }

        .sessions-view-card-actions {
          display: flex;
          gap: 6px;
        }
      }

      .sessions-view-card:hover {
        background: var(--color-panel-soft);
      }
    }
  }

  .sessions-view-pagination {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);

    .sessions-view-page-size,
    .sessions-view-page-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      color: var(--color-text-muted);
      font-size: 0.8rem;
    }

    .sessions-view-page-label {
      font-weight: 700;
    }

    .sessions-view-page-size {
      .sessions-view-page-select {
        height: 30px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        padding: 0 8px;
        color: var(--color-text);
        font: inherit;
      }
    }
  }

  .sessions-view-modal {
    position: fixed;
    inset: 0;
    z-index: 34;

    .sessions-view-modal-overlay {
      position: absolute;
      inset: 0;
      background: rgba(15, 23, 42, 0.28);
      backdrop-filter: blur(2px);
    }

    .sessions-view-modal-panel {
      position: absolute;
      top: 58px;
      left: 50%;
      display: flex;
      width: 780px;
      max-height: 680px;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      box-shadow: var(--shadow-panel);
      transform: translateX(-50%);

      .sessions-view-modal-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 16px;
        padding: 16px 18px 14px;
        border-bottom: 1px solid var(--color-line);
        background: linear-gradient(180deg, var(--color-panel) 0%, var(--color-primary-soft) 100%);

        .sessions-view-modal-title {
          display: flex;
          min-width: 0;
          flex-direction: column;
          gap: 4px;

          .sessions-view-modal-eyebrow {
            color: var(--color-text-soft);
            font-size: 0.68rem;
            font-weight: 800;
            letter-spacing: 0.12em;
            line-height: 1;
            text-transform: uppercase;
          }

          .sessions-view-modal-heading {
            margin: 0;
            color: var(--color-text);
            font-size: 1.16rem;
            line-height: 1.2;
          }

          .sessions-view-modal-count {
            margin: 0;
            color: var(--color-text-muted);
            font-size: 0.82rem;

            .sessions-view-modal-count-value {
              color: var(--color-primary);
              font-weight: 800;
            }
          }
        }

        .sessions-view-modal-actions {
          display: flex;
          align-items: center;
          gap: 8px;
        }
      }

      .sessions-view-recycle-tools {
        display: grid;
        grid-template-columns: minmax(0, 1fr) 140px 180px auto;
        gap: 10px;
        align-items: end;
        padding: 12px;
        border-bottom: 1px solid var(--color-line);
        background: var(--color-panel-soft);

        .sessions-view-recycle-search,
        .sessions-view-recycle-select {
          display: flex;
          min-width: 0;
          flex-direction: column;
          gap: 6px;
        }

        .sessions-view-recycle-filter-label {
          color: var(--color-text-muted);
          font-size: 0.72rem;
          font-weight: 700;
        }

        .sessions-view-recycle-search-input,
        .sessions-view-recycle-select-control {
          height: 34px;
          min-width: 0;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel);
          padding: 0 10px;
          color: var(--color-text);
          font: inherit;
          font-size: 0.82rem;
        }
      }

      .sessions-view-recycle-list {
        display: flex;
        flex: 1;
        min-height: 0;
        overflow: auto;
        flex-direction: column;
        gap: 8px;
        padding: 12px;

        .sessions-view-recycle-card {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 14px;
          padding: 12px 14px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel);

          .sessions-view-recycle-main {
            display: flex;
            min-width: 0;
            flex: 1;
            flex-direction: column;
            gap: 6px;

            .sessions-view-recycle-title {
              display: flex;
              min-width: 0;
              align-items: center;
              gap: 7px;

              .sessions-view-cli-icon {
                width: 20px;
                height: 20px;
                flex: 0 0 20px;
              }

              .sessions-view-recycle-heading {
                overflow: hidden;
                margin: 0;
                color: var(--color-text);
                font-size: 0.9rem;
                line-height: 1.2;
                text-overflow: ellipsis;
                white-space: nowrap;
              }

              .sessions-view-recycle-cli {
                display: inline-flex;
                height: 20px;
                flex: none;
                align-items: center;
                padding: 0 7px;
                border-radius: 6px;
                background: var(--color-primary-soft);
                color: var(--color-primary);
                font-size: 0.7rem;
                font-weight: 700;
              }
            }

            .sessions-view-recycle-path {
              overflow: hidden;
              margin: 0;
              color: var(--color-text-muted);
              font-size: 0.8rem;
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .sessions-view-recycle-meta {
              display: flex;
              min-width: 0;
              gap: 12px;
              overflow: hidden;
              color: var(--color-text-soft);
              font-size: 0.74rem;
              font-weight: 600;
              white-space: nowrap;

              .sessions-view-recycle-meta-text {
                overflow: hidden;
                text-overflow: ellipsis;
              }
            }
          }

          .sessions-view-recycle-actions {
            display: flex;
            flex: none;
            align-items: center;
            gap: 8px;
          }
        }

        .sessions-view-recycle-card:hover {
          border-color: var(--color-info-line);
          background: var(--color-panel-soft);
        }
      }

      .sessions-view-recycle-pagination {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 10px 12px;
        border-top: 1px solid var(--color-line);
        background: var(--color-panel-soft);

        .sessions-view-recycle-page-info {
          color: var(--color-text-muted);
          font-size: 0.78rem;
          white-space: nowrap;
        }

        .sessions-view-recycle-page-controls {
          display: flex;
          align-items: center;
          gap: 10px;

          .sessions-view-recycle-page-size,
          .sessions-view-recycle-page-actions {
            display: flex;
            align-items: center;
            gap: 8px;
            color: var(--color-text-muted);
            font-size: 0.78rem;
          }

          .sessions-view-recycle-page-label {
            font-weight: 700;
          }

          .sessions-view-recycle-page-size {
            .sessions-view-recycle-page-select {
              height: 30px;
              border: 1px solid var(--color-line);
              border-radius: 8px;
              background: var(--color-panel);
              padding: 0 8px;
              color: var(--color-text);
              font: inherit;
            }
          }
        }
      }

      .sessions-view-recycle-empty {
        display: flex;
        min-height: 220px;
        align-items: center;
        justify-content: center;
        flex-direction: column;
        gap: 10px;
        color: var(--color-text-muted);
        font-size: 0.86rem;

        .sessions-view-recycle-empty-icon {
          color: var(--color-text-muted);
        }
      }
    }
  }

  .sessions-view-empty {
    display: grid;
    flex: 1;
    min-height: 0;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    text-align: center;

    .sessions-view-empty-title {
      margin: 0 0 10px;
      font-size: 1.28rem;
    }

    .sessions-view-empty-desc {
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
    background: var(--color-panel-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.88rem;
    font-weight: 600;

    .action-button-icon {
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
  }

  .icon-button.icon-button-danger {
    color: var(--color-danger);
  }

  .icon-button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .action-button:hover,
  .icon-button:not(:disabled):hover {
    border-color: var(--color-line-strong);
    background: var(--color-primary-soft);
  }
}
</style>

<template>
  <section class="git-branch-compare">
    <!-- 顶部独立标题栏（可选，用于独立页面/弹窗模式） -->
    <header v-if="showHeader" class="git-branch-compare-head">
      <div class="git-branch-compare-title-wrap">
        <div class="git-branch-compare-title-icon-box">
          <GitCompare :size="16" />
        </div>
        <div class="git-branch-compare-title-text-group">
          <span data-emphasis class="git-branch-compare-title">对比分支</span>
          <span class="git-branch-compare-subtitle"
            >查看两个分支之间的文件和行级差异</span
          >
        </div>
      </div>
      <button
        class="git-branch-compare-close"
        type="button"
        :disabled="loading"
        @click="emit('close')"
      >
        <X :size="14" />
        <span>返回提交记录</span>
      </button>
    </header>

    <!-- 分支选择与操作控制栏 -->
    <div class="git-branch-compare-controls">
      <div class="git-branch-compare-selectors-group">
        <div class="git-branch-compare-select-wrap">
          <span class="git-branch-compare-select-label">基准分支 (base)</span>
          <div class="git-branch-compare-select-box">
            <GitBranch :size="14" class="git-branch-compare-select-icon" />
            <select
              v-model="baseBranch"
              :disabled="busy || loading"
              class="git-branch-compare-native-select"
              title="选择基准分支"
            >
              <option value="" disabled>请选择基准分支</option>
              <option
                v-for="branch in branches"
                :key="`base:${branch.name}`"
                :value="branch.name"
              >
                {{ branch.name }}{{ branch.isCurrent ? " (当前)" : "" }}
              </option>
            </select>
            <ChevronDown :size="13" class="git-branch-compare-select-arrow" />
          </div>
        </div>

        <button
          class="git-branch-compare-swap-btn"
          type="button"
          :disabled="busy || loading || !baseBranch || !targetBranch"
          title="交换基准分支与对比分支"
          @click="swapBranches"
        >
          <ArrowLeftRight :size="14" />
        </button>

        <div class="git-branch-compare-select-wrap">
          <span class="git-branch-compare-select-label"
            >对比分支 (compare)</span
          >
          <div
            class="git-branch-compare-select-box git-branch-compare-select-box--target"
          >
            <GitBranch :size="14" class="git-branch-compare-select-icon" />
            <select
              v-model="targetBranch"
              :disabled="busy || loading"
              class="git-branch-compare-native-select"
              title="选择对比分支"
            >
              <option value="" disabled>请选择对比分支</option>
              <option
                v-for="branch in branches"
                :key="`target:${branch.name}`"
                :value="branch.name"
              >
                {{ branch.name }}{{ branch.isCurrent ? " (当前)" : "" }}
              </option>
            </select>
            <ChevronDown :size="13" class="git-branch-compare-select-arrow" />
          </div>
        </div>
      </div>

      <button
        class="git-branch-compare-submit"
        type="button"
        :disabled="busy || loading || !canCompare"
        @click="compareBranches"
      >
        <RefreshCw
          :size="14"
          :class="{ 'git-branch-compare-spinner': loading }"
        />
        <span>{{ loading ? "正在对比..." : "开始对比" }}</span>
      </button>
    </div>

    <!-- 错误状态提示 -->
    <div v-if="error" class="git-branch-compare-error-banner">
      <AlertCircle :size="16" class="git-branch-compare-error-icon" />
      <span class="git-branch-compare-error-msg">{{ error }}</span>
      <button
        class="git-branch-compare-retry-btn"
        type="button"
        @click="compareBranches"
      >
        重试
      </button>
    </div>

    <!-- 加载中状态 -->
    <div v-else-if="loading" class="git-branch-compare-loading-view">
      <div class="git-branch-compare-loader-card">
        <RefreshCw
          :size="24"
          class="git-branch-compare-spinner git-branch-compare-loader-icon"
        />
        <span class="git-branch-compare-loader-text">正在分析分支差异...</span>
        <span class="git-branch-compare-loader-subtext">
          对比 {{ baseBranch }} 与 {{ targetBranch }}
        </span>
      </div>
    </div>

    <!-- 对比结果展示 -->
    <template v-else-if="result">
      <!-- KPI 差异统计卡片行 -->
      <div class="git-branch-compare-stats-card">
        <div class="git-branch-compare-stats">
          <div class="git-branch-compare-stat git-branch-compare-stat-files">
            <div
              class="git-branch-compare-stat-icon-wrap git-branch-compare-stat-icon--blue"
            >
              <FolderGit2 :size="16" />
            </div>
            <div class="git-branch-compare-stat-content">
              <span class="git-branch-compare-stat-label">变更文件</span>
              <span data-emphasis class="git-branch-compare-stat-number">{{
                formatNumber(result.fileCount)
              }}</span>
            </div>
          </div>

          <div class="git-branch-compare-stat git-branch-compare-stat-add">
            <div
              class="git-branch-compare-stat-icon-wrap git-branch-compare-stat-icon--green"
            >
              <PlusCircle :size="16" />
            </div>
            <div class="git-branch-compare-stat-content">
              <span class="git-branch-compare-stat-label">新增行</span>
              <span data-emphasis class="git-branch-compare-stat-number"
                >+{{ result.additions }}</span
              >
            </div>
          </div>

          <div class="git-branch-compare-stat git-branch-compare-stat-delete">
            <div
              class="git-branch-compare-stat-icon-wrap git-branch-compare-stat-icon--red"
            >
              <MinusCircle :size="16" />
            </div>
            <div class="git-branch-compare-stat-content">
              <span class="git-branch-compare-stat-label">删除行</span>
              <span data-emphasis class="git-branch-compare-stat-number"
                >-{{ result.deletions }}</span
              >
            </div>
          </div>

          <div class="git-branch-compare-stat git-branch-compare-stat-total">
            <div
              class="git-branch-compare-stat-icon-wrap git-branch-compare-stat-icon--amber"
            >
              <GitCommitHorizontal :size="16" />
            </div>
            <div class="git-branch-compare-stat-content">
              <span class="git-branch-compare-stat-label">不同行数</span>
              <span data-emphasis class="git-branch-compare-stat-number">{{
                result.changedLines
              }}</span>
            </div>
          </div>
        </div>

        <!-- 差异变动比例条 -->
        <div
          v-if="result.changedLines > 0"
          class="git-branch-compare-ratio-bar-wrap"
        >
          <div class="git-branch-compare-ratio-bar">
            <div
              class="git-branch-compare-ratio-segment git-branch-compare-ratio-add"
              :style="{ width: `${additionRatio}%` }"
              :title="`新增 ${formatNumber(result.additions)} 行 (${additionRatio}%)`"
            ></div>
            <div
              class="git-branch-compare-ratio-segment git-branch-compare-ratio-delete"
              :style="{ width: `${deletionRatio}%` }"
              :title="`删除 ${formatNumber(result.deletions)} 行 (${deletionRatio}%)`"
            ></div>
          </div>
        </div>
      </div>

      <!-- 主体分栏：左侧文件差异树/列表，右侧代码 Diff -->
      <div v-if="result.files.length" class="git-branch-compare-body">
        <!-- 左侧文件列表侧边栏 -->
        <aside class="git-branch-compare-files">
          <!-- 文件侧栏头部 -->
          <div class="git-branch-compare-files-header">
            <div class="git-branch-compare-files-title-box">
              <span class="git-branch-compare-files-heading">文件列表</span>
              <span class="git-branch-compare-files-badge">{{
                filteredFiles.length !== result.files.length
                  ? `${filteredFiles.length}/${result.files.length}`
                  : result.files.length
              }}</span>
            </div>
            <div class="git-branch-compare-files-view-toggles">
              <button
                :class="[
                  'git-branch-compare-view-toggle',
                  {
                    'git-branch-compare-view-toggle--active':
                      viewMode === 'list'
                  }
                ]"
                type="button"
                title="平铺列表视图"
                @click="viewMode = 'list'"
              >
                <List :size="13" />
              </button>
              <button
                :class="[
                  'git-branch-compare-view-toggle',
                  {
                    'git-branch-compare-view-toggle--active':
                      viewMode === 'tree'
                  }
                ]"
                type="button"
                title="目录树视图"
                @click="viewMode = 'tree'"
              >
                <FolderTree :size="13" />
              </button>
            </div>
          </div>

          <!-- 搜索与筛选工具条 -->
          <div class="git-branch-compare-files-search-bar">
            <div class="git-branch-compare-files-search-input-box">
              <Search :size="13" class="git-branch-compare-files-search-icon" />
              <input
                v-model="searchQuery"
                type="text"
                placeholder="搜索文件..."
                class="git-branch-compare-files-search-input"
              />
              <button
                v-if="searchQuery"
                class="git-branch-compare-files-search-clear"
                type="button"
                title="清空搜索"
                @click="searchQuery = ''"
              >
                <X :size="12" />
              </button>
            </div>
            <!-- 状态快速筛选下拉/切换 -->
            <button
              :class="[
                'git-branch-compare-filter-btn',
                {
                  'git-branch-compare-filter-btn--active':
                    statusFilter !== 'all'
                }
              ]"
              type="button"
              :title="`筛选状态：${currentStatusFilterLabel}`"
              @click="cycleStatusFilter"
            >
              <Filter :size="13" />
              <span
                v-if="statusFilter !== 'all'"
                class="git-branch-compare-filter-tag"
                >{{ statusFilter }}</span
              >
            </button>
          </div>

          <!-- 文件列表区域 -->
          <div class="git-branch-compare-files-scroll-wrap">
            <div
              v-if="!filteredFiles.length"
              class="git-branch-compare-files-empty"
            >
              未找到匹配的文件
            </div>

            <!-- 平铺列表视图 -->
            <div
              v-else-if="viewMode === 'list'"
              class="git-branch-compare-files-list"
            >
              <button
                v-for="file in filteredFiles"
                :key="file.path"
                :class="[
                  'git-branch-compare-file',
                  {
                    'git-branch-compare-file--active':
                      file.path === activeFilePath
                  }
                ]"
                type="button"
                :title="file.path"
                @click="selectFile(file.path)"
              >
                <!-- 文件扩展名多彩图标 -->
                <span
                  :class="[
                    'git-branch-compare-file-ext-icon',
                    `git-branch-compare-file-ext-${getFileIconType(file.path)}`
                  ]"
                >
                  <component :is="getFileIcon(file.path)" :size="13" />
                </span>

                <span
                  :class="[
                    'git-branch-compare-file-status',
                    `git-branch-compare-file-status-${statusClass(file.status)}`
                  ]"
                >
                  {{ statusLabel(file.status) }}
                </span>

                <div class="git-branch-compare-file-name-group">
                  <span class="git-branch-compare-file-basename">{{
                    getFileName(file.path)
                  }}</span>
                  <span
                    v-if="getFileDir(file.path)"
                    class="git-branch-compare-file-dirname"
                    >{{ getFileDir(file.path) }}</span
                  >
                </div>

                <div class="git-branch-compare-file-tail">
                  <span
                    v-if="file.binary"
                    class="git-branch-compare-file-binary"
                    >bin</span
                  >
                  <span v-else class="git-branch-compare-file-diff-stat">
                    <span class="git-branch-compare-file-add"
                      >+{{ file.additions }}</span
                    >
                    <span class="git-branch-compare-file-delete"
                      >-{{ file.deletions }}</span
                    >
                  </span>
                </div>
              </button>
            </div>

            <!-- 目录树形视图 -->
            <div v-else class="git-branch-compare-files-tree">
              <template
                v-for="node in flattenedTreeNodes"
                :key="node.type + ':' + node.path"
              >
                <!-- 目录节点 -->
                <button
                  v-if="node.type === 'directory'"
                  class="git-branch-compare-tree-dir"
                  :style="{ paddingLeft: `${node.level * 14 + 10}px` }"
                  type="button"
                  :title="node.path"
                  @click="toggleDirectory(node.path)"
                >
                  <component
                    :is="node.isClosed ? ChevronRight : ChevronDown"
                    :size="12"
                    class="git-branch-compare-tree-caret"
                  />
                  <Folder :size="13" class="git-branch-compare-tree-dir-icon" />
                  <span class="git-branch-compare-tree-dir-name">{{
                    node.name
                  }}</span>
                  <span class="git-branch-compare-tree-dir-count">{{
                    node.count
                  }}</span>
                </button>

                <!-- 文件节点 -->
                <button
                  v-else
                  :class="[
                    'git-branch-compare-file',
                    'git-branch-compare-tree-file',
                    {
                      'git-branch-compare-file--active':
                        node.path === activeFilePath
                    }
                  ]"
                  :style="{ paddingLeft: `${node.level * 14 + 14}px` }"
                  type="button"
                  :title="node.path"
                  @click="selectFile(node.path)"
                >
                  <span
                    :class="[
                      'git-branch-compare-file-ext-icon',
                      `git-branch-compare-file-ext-${getFileIconType(node.path)}`
                    ]"
                  >
                    <component :is="getFileIcon(node.path)" :size="13" />
                  </span>
                  <span
                    v-if="node.file"
                    :class="[
                      'git-branch-compare-file-status',
                      `git-branch-compare-file-status-${statusClass(node.file.status)}`
                    ]"
                  >
                    {{ statusLabel(node.file.status) }}
                  </span>
                  <span class="git-branch-compare-file-basename">{{
                    node.name
                  }}</span>
                  <div class="git-branch-compare-file-tail">
                    <span
                      v-if="node.file?.binary"
                      class="git-branch-compare-file-binary"
                      >bin</span
                    >
                    <span v-else class="git-branch-compare-file-diff-stat">
                      <span class="git-branch-compare-file-add"
                        >+{{ node.file?.additions || 0 }}</span
                      >
                      <span class="git-branch-compare-file-delete"
                        >-{{ node.file?.deletions || 0 }}</span
                      >
                    </span>
                  </div>
                </button>
              </template>
            </div>
          </div>
        </aside>

        <!-- 右侧代码差异对比视口 -->
        <main class="git-branch-compare-diff-pane">
          <!-- 顶部吸顶文件工具栏 -->
          <div v-if="activeFile" class="git-branch-compare-diff-topbar">
            <div class="git-branch-compare-diff-file-info">
              <span
                :class="[
                  'git-branch-compare-file-ext-icon',
                  `git-branch-compare-file-ext-${getFileIconType(activeFile.path)}`
                ]"
              >
                <component :is="getFileIcon(activeFile.path)" :size="14" />
              </span>
              <span
                class="git-branch-compare-diff-file-name"
                :title="activeFile.path"
              >
                {{ activeFile.path }}
              </span>
              <span
                :class="[
                  'git-branch-compare-status-tag',
                  `git-branch-compare-status-tag--${statusClass(activeFile.status)}`
                ]"
              >
                {{ statusFullLabel(activeFile.status) }}
              </span>
              <div
                v-if="!activeFile.binary"
                class="git-branch-compare-file-diff-stat"
              >
                <span class="git-branch-compare-file-add"
                  >+{{ activeFile.additions }}</span
                >
                <span class="git-branch-compare-file-delete"
                  >-{{ activeFile.deletions }}</span
                >
              </div>
            </div>

            <div class="git-branch-compare-diff-actions">
              <!-- 上一个/下一个文件快速切换 -->
              <div class="git-branch-compare-nav-buttons">
                <button
                  class="git-branch-compare-icon-btn"
                  type="button"
                  title="上一个文件"
                  :disabled="isFirstFileActive"
                  @click="navigateFile('prev')"
                >
                  <ChevronUp :size="14" />
                </button>
                <button
                  class="git-branch-compare-icon-btn"
                  type="button"
                  title="下一个文件"
                  :disabled="isLastFileActive"
                  @click="navigateFile('next')"
                >
                  <ChevronDown :size="14" />
                </button>
              </div>

              <!-- 复制路径 -->
              <button
                class="git-branch-compare-icon-btn"
                type="button"
                title="复制文件路径"
                @click="copyText(activeFile.path, '文件路径')"
              >
                <Check
                  v-if="copiedTarget === '文件路径'"
                  :size="13"
                  class="git-branch-compare-copied-icon"
                />
                <Copy v-else :size="13" />
              </button>

              <!-- 查看原文件 -->
              <button
                class="git-branch-compare-raw-file-btn"
                type="button"
                title="在默认编辑器中打开原文件"
                @click="openRawFile(activeFile.path)"
              >
                <ExternalLink :size="13" />
                <span>查看原文件</span>
              </button>
            </div>
          </div>

          <!-- 差异代码虚拟滚动视口 -->
          <pre
            ref="diffViewport"
            class="git-branch-compare-diff"
            @scroll="handleDiffScroll"
          ><code :style="{
            paddingTop: `${visibleDiffRange.start * 20}px`,
            paddingBottom: `${(diffLines.length - visibleDiffRange.end) * 20}px`
          }"><span v-for="(line, index) in diffLines.slice(visibleDiffRange.start, visibleDiffRange.end)" :key="visibleDiffRange.start + index" :class="['git-branch-compare-line', `git-branch-compare-line-${lineClass(line)}`]" :data-line="lineNumbers[visibleDiffRange.start + index] || ''">{{ line || ' ' }}</span></code></pre>
        </main>
      </div>

      <!-- 无文件差异状态 -->
      <div v-else class="git-branch-compare-empty-view">
        <CheckCircle2
          :size="32"
          class="git-branch-compare-empty-success-icon"
        />
        <span class="git-branch-compare-empty-title">两个分支没有文件差异</span>
        <span class="git-branch-compare-empty-subtitle">
          分支 {{ result.baseBranch }} 与
          {{ result.targetBranch }} 的代码完全一致
        </span>
      </div>
    </template>

    <!-- 初始引导状态 -->
    <div v-else class="git-branch-compare-empty-view">
      <GitCompare
        :size="36"
        class="git-branch-compare-empty-placeholder-icon"
      />
      <span class="git-branch-compare-empty-title">准备对比分支</span>
      <span class="git-branch-compare-empty-subtitle">
        在上方选择基准分支和对比分支，点击「开始对比」查看差异详情
      </span>
    </div>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"
import {
  AlertCircle,
  ArrowLeftRight,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  ExternalLink,
  File,
  FileCode,
  FileJson,
  FileText,
  Filter,
  Folder,
  FolderGit2,
  FolderTree,
  GitBranch,
  GitCommitHorizontal,
  GitCompare,
  List,
  MinusCircle,
  PlusCircle,
  RefreshCw,
  Search,
  Settings,
  X
} from "lucide-vue-next"
import { gitToolApi, systemApi } from "@/api"
import { createMessage } from "@/utils/message"

const props = defineProps({
  repoId: { type: String, required: true },
  projectPath: { type: String, default: "" },
  branches: { type: Array, default: () => [] },
  currentBranch: { type: String, default: "" },
  selectedBranch: { type: String, default: "" },
  busy: { type: Boolean, default: false },
  showHeader: { type: Boolean, default: false }
})

const emit = defineEmits(["close"])

const baseBranch = ref("")
const targetBranch = ref("")
const loading = ref(false)
const error = ref("")
const result = ref(null)
const diffViewport = ref(null)
const diffScrollTop = ref(0)
const diffViewportHeight = ref(0)
const activeFilePath = ref("")
const viewMode = ref("list") // 'list' | 'tree'
const searchQuery = ref("")
const statusFilter = ref("all") // 'all' | 'M' | 'A' | 'D'
const closedDirectories = ref(new Set())
const copiedTarget = ref("")
let copyTimer = null
let diffResizeObserver = null
let requestSequence = 0

const canCompare = computed(() => {
  return Boolean(baseBranch.value && targetBranch.value)
})

const diffLines = computed(() => {
  if (!result.value?.patch) return []
  return result.value.patch.split("\n")
})

// 预计算文件差异起始行索引映射
const fileIndexMap = computed(() => {
  const map = new Map()
  const lines = diffLines.value
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    if (line.startsWith("diff --git ")) {
      const parts = line.slice(11).split(" ")
      if (parts.length >= 2) {
        const bPath = parts[1].replace(/^[ab]\//, "")
        const aPath = parts[0].replace(/^[ab]\//, "")
        map.set(bPath, i)
        map.set(aPath, i)
      }
    }
  }
  return map
})

// 预计算行号数组，兼容超大 diff
const lineNumbers = computed(() => {
  const lines = diffLines.value
  const nums = new Array(lines.length)
  let newLine = 0
  let oldLine = 0

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    if (line.startsWith("diff --git ")) {
      oldLine = 0
      newLine = 0
      nums[i] = ""
    } else if (line.startsWith("@@")) {
      const match = line.match(
        /@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@/
      )
      if (match) {
        oldLine = parseInt(match[1], 10)
        newLine = parseInt(match[3], 10)
      }
      nums[i] = ""
    } else if (line.startsWith("+") && !line.startsWith("+++")) {
      nums[i] = newLine > 0 ? String(newLine++) : String(i + 1)
    } else if (line.startsWith("-") && !line.startsWith("---")) {
      nums[i] = oldLine > 0 ? String(oldLine++) : String(i + 1)
    } else if (
      line.startsWith("index ") ||
      line.startsWith("--- ") ||
      line.startsWith("+++ ") ||
      line.startsWith("deleted file mode ") ||
      line.startsWith("new file mode ")
    ) {
      nums[i] = ""
    } else {
      nums[i] = newLine > 0 ? String(newLine++) : String(i + 1)
      if (oldLine > 0) oldLine++
    }
  }
  return nums
})

// 大型分支对比只渲染可见行及前后缓冲，保持虚拟滚动平滑
const visibleDiffRange = computed(() => ({
  start: Math.max(0, Math.floor(diffScrollTop.value / 20) - 20),
  end: Math.min(
    diffLines.value.length,
    Math.ceil((diffScrollTop.value + diffViewportHeight.value) / 20) + 20
  )
}))

// 差异变动比例计算
const additionRatio = computed(() => {
  if (!result.value?.changedLines) return 50
  return Math.round((result.value.additions / result.value.changedLines) * 100)
})

const deletionRatio = computed(() => {
  if (!result.value?.changedLines) return 50
  return 100 - additionRatio.value
})

// 筛选后的文件列表
const filteredFiles = computed(() => {
  const files = result.value?.files || []
  const query = searchQuery.value.trim().toLowerCase()
  const filter = statusFilter.value

  return files.filter((file) => {
    if (filter !== "all" && file.status !== filter) return false
    if (query && !file.path.toLowerCase().includes(query)) return false
    return true
  })
})

// 当前激活的文件对象
const activeFile = computed(() => {
  if (!result.value?.files?.length) return null
  return (
    result.value.files.find((f) => f.path === activeFilePath.value) ||
    filteredFiles.value[0] ||
    result.value.files[0]
  )
})

const isFirstFileActive = computed(() => {
  const files = filteredFiles.value
  return !files.length || files[0].path === activeFilePath.value
})

const isLastFileActive = computed(() => {
  const files = filteredFiles.value
  return !files.length || files[files.length - 1].path === activeFilePath.value
})

const currentStatusFilterLabel = computed(() => {
  const map = { all: "全部", M: "修改", A: "新增", D: "删除" }
  return map[statusFilter.value] || "全部"
})

// 目录树构建与扁平化
const rawFileTree = computed(() => {
  const files = filteredFiles.value
  const root = {
    name: "",
    path: "",
    type: "directory",
    count: 0,
    children: []
  }

  for (const file of files) {
    const parts = String(file.path || "")
      .split("/")
      .filter(Boolean)
    let current = root
    current.count += 1

    parts.forEach((part, index) => {
      const isFile = index === parts.length - 1
      const pathText = parts.slice(0, index + 1).join("/")
      let child = current.children.find(
        (item) =>
          item.name === part && item.type === (isFile ? "file" : "directory")
      )

      if (!child) {
        child = {
          name: part,
          path: pathText,
          type: isFile ? "file" : "directory",
          count: 0,
          file: isFile ? file : null,
          children: []
        }
        current.children.push(child)
      }
      child.count += 1
      current = child
    })
  }

  sortTree(root)
  return root
})

function sortTree(node) {
  if (!node.children) return
  node.children.sort((a, b) => {
    if (a.type !== b.type) return a.type === "directory" ? -1 : 1
    return a.name.localeCompare(b.name)
  })
  node.children.forEach(sortTree)
}

const flattenedTreeNodes = computed(() => {
  const resultList = []
  function traverse(node, level = 0) {
    if (!node.children) return
    for (const child of node.children) {
      const isDir = child.type === "directory"
      const isClosed = closedDirectories.value.has(child.path)
      resultList.push({
        ...child,
        level,
        isClosed
      })
      if (isDir && !isClosed) {
        traverse(child, level + 1)
      }
    }
  }
  traverse(rawFileTree.value, 0)
  return resultList
})

watch(
  diffViewport,
  (element) => {
    diffResizeObserver?.disconnect()
    if (!element) return
    diffViewportHeight.value = element.clientHeight
    diffResizeObserver = new ResizeObserver(() => {
      diffViewportHeight.value = element.clientHeight
    })
    diffResizeObserver.observe(element)
  },
  { flush: "post" }
)

watch(
  () => [props.branches, props.currentBranch, props.selectedBranch],
  () => {
    const names = props.branches.map((branch) => branch.name)
    const activeBranch =
      names.find((name) => name === props.currentBranch) ||
      props.branches.find((branch) => branch.isCurrent)?.name ||
      names[0] ||
      ""
    if (!names.includes(baseBranch.value)) {
      baseBranch.value = activeBranch
    }
    if (!names.includes(targetBranch.value)) {
      targetBranch.value = activeBranch
    }
  },
  { immediate: true }
)

async function compareBranches() {
  if (!canCompare.value) return
  const sequence = ++requestSequence
  loading.value = true
  error.value = ""
  try {
    const nextResult = await gitToolApi.getGitToolBranchDiff({
      repoId: props.repoId,
      baseBranch: baseBranch.value,
      targetBranch: targetBranch.value
    })
    if (sequence === requestSequence) {
      result.value = nextResult
      diffScrollTop.value = 0
      if (diffViewport.value) diffViewport.value.scrollTop = 0
      if (nextResult?.files?.length) {
        activeFilePath.value = nextResult.files[0].path
      }
    }
  } catch (requestError) {
    if (sequence === requestSequence) {
      error.value = String(requestError?.message || requestError)
    }
  } finally {
    if (sequence === requestSequence) loading.value = false
  }
}

function swapBranches() {
  if (props.busy || loading.value) return
  const temp = baseBranch.value
  baseBranch.value = targetBranch.value
  targetBranch.value = temp
  if (canCompare.value) {
    compareBranches()
  }
}

function handleDiffScroll(event) {
  const top = event.target.scrollTop
  diffScrollTop.value = top
  updateActiveFileOnScroll(top)
}

function updateActiveFileOnScroll(top) {
  if (!result.value?.files?.length) return
  const currentLine = Math.floor(top / 20)
  const map = fileIndexMap.value
  let bestFile = ""
  let bestLine = -1

  for (const file of result.value.files) {
    const lineIdx = map.get(file.path)
    if (lineIdx !== undefined && lineIdx <= currentLine && lineIdx > bestLine) {
      bestLine = lineIdx
      bestFile = file.path
    }
  }

  if (bestFile && bestFile !== activeFilePath.value) {
    activeFilePath.value = bestFile
  }
}

function selectFile(filePath) {
  activeFilePath.value = filePath
  const lineIdx = fileIndexMap.value.get(filePath)
  if (lineIdx !== undefined && diffViewport.value) {
    diffViewport.value.scrollTop = lineIdx * 20
    diffScrollTop.value = lineIdx * 20
  }
}

function navigateFile(direction) {
  const files = filteredFiles.value
  if (!files.length) return
  const currentIndex = files.findIndex((f) => f.path === activeFilePath.value)
  const targetIndex =
    direction === "next"
      ? Math.min(files.length - 1, currentIndex + 1)
      : Math.max(0, currentIndex - 1)
  if (targetIndex >= 0 && targetIndex < files.length) {
    selectFile(files[targetIndex].path)
  }
}

function toggleDirectory(dirPath) {
  const nextSet = new Set(closedDirectories.value)
  if (nextSet.has(dirPath)) {
    nextSet.delete(dirPath)
  } else {
    nextSet.add(dirPath)
  }
  closedDirectories.value = nextSet
}

function cycleStatusFilter() {
  const order = ["all", "M", "A", "D"]
  const nextIdx = (order.indexOf(statusFilter.value) + 1) % order.length
  statusFilter.value = order[nextIdx]
}

async function copyText(text, label = "内容") {
  if (!text) return
  try {
    await navigator.clipboard.writeText(String(text))
    createMessage.success(`${label}已复制到剪贴板`)
    copiedTarget.value = label
    clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      copiedTarget.value = ""
    }, 2000)
  } catch {
    createMessage.error("复制失败")
  }
}

async function openRawFile(filePath) {
  if (!filePath) return
  const fullPath = props.projectPath
    ? `${props.projectPath.replace(/[\\/]+$/, "")}/${filePath.replace(/^[\\/]+/, "")}`
    : filePath

  try {
    await systemApi.openPath({ path: fullPath })
    createMessage.success("已在编辑器中打开文件")
  } catch {
    await copyText(fullPath, "文件路径")
  }
}

function formatNumber(num) {
  if (num === undefined || num === null) return "0"
  return Number(num).toLocaleString()
}

function getFileName(path) {
  if (!path) return ""
  return path.split("/").pop() || path
}

function getFileDir(path) {
  if (!path) return ""
  const parts = path.split("/")
  if (parts.length <= 1) return ""
  parts.pop()
  return parts.join("/")
}

function getFileIconType(path) {
  if (!path) return "default"
  const filename = path.split("/").pop().toLowerCase()
  if (filename.endsWith(".vue")) return "vue"
  if (filename.endsWith(".ts") || filename.endsWith(".tsx")) return "ts"
  if (
    filename.endsWith(".js") ||
    filename.endsWith(".jsx") ||
    filename.endsWith(".mjs")
  )
    return "js"
  if (filename.endsWith(".json")) return "json"
  if (filename.endsWith(".md") || filename.endsWith(".markdown")) return "md"
  if (
    filename.endsWith(".css") ||
    filename.endsWith(".less") ||
    filename.endsWith(".scss")
  )
    return "css"
  if (
    filename.endsWith(".yml") ||
    filename.endsWith(".yaml") ||
    filename.includes("config")
  )
    return "config"
  return "default"
}

function getFileIcon(path) {
  const type = getFileIconType(path)
  if (type === "json") return FileJson
  if (type === "md") return FileText
  if (type === "config") return Settings
  if (type === "default") return File
  return FileCode
}

function statusLabel(status) {
  return (
    { A: "增", M: "改", D: "删", R: "移", C: "拷" }[status] || status || "-"
  )
}

function statusFullLabel(status) {
  return (
    { A: "新增", M: "已修改", D: "已删除", R: "重命名", C: "复制" }[status] ||
    "已修改"
  )
}

function statusClass(status) {
  return (
    { A: "add", M: "modify", D: "delete", R: "move", C: "copy" }[status] ||
    "modify"
  )
}

function lineClass(line) {
  if (line.startsWith("+") && !line.startsWith("+++")) return "add"
  if (line.startsWith("-") && !line.startsWith("---")) return "delete"
  if (line.startsWith("@@")) return "chunk"
  if (line.startsWith("diff --git") || line.startsWith("index ")) return "meta"
  return "normal"
}

// 切换项目后等待分支列表就绪，再自动比较当前分支自身。
watch(
  canCompare,
  (ready) => {
    if (ready) compareBranches()
  },
  { immediate: true }
)

onBeforeUnmount(() => {
  diffResizeObserver?.disconnect()
  clearTimeout(copyTimer)
})
</script>

<style scoped lang="less">
.git-branch-compare {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  background: var(--color-page);
  color: var(--color-text);

  /* 顶部标题栏 */
  .git-branch-compare-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 18px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  .git-branch-compare-title-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .git-branch-compare-title-icon-box {
    display: flex;
    width: 28px;
    height: 28px;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .git-branch-compare-title-text-group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .git-branch-compare-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .git-branch-compare-subtitle {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .git-branch-compare-close {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.2s ease;

    &:hover:not(:disabled) {
      background: var(--color-line);
      color: var(--color-text);
    }
  }

  /* 分支控制工具栏 */
  .git-branch-compare-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 18px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .git-branch-compare-selectors-group {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: flex-end;
    gap: 12px;
  }

  .git-branch-compare-select-wrap {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 5px;
  }

  .git-branch-compare-select-label {
    font-size: 11px;
    font-weight: 500;
    color: var(--color-text-muted);
  }

  .git-branch-compare-select-box {
    position: relative;
    display: flex;
    align-items: center;
    height: 34px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    transition: all 0.2s ease;

    &:hover {
      border-color: var(--color-line-strong);
    }

    &:focus-within {
      border-color: var(--color-primary);
      box-shadow: 0 0 0 2px var(--color-primary-soft);
    }

    &.git-branch-compare-select-box--target {
      border-color: var(--color-info-line, var(--color-line));
    }
  }

  .git-branch-compare-select-icon {
    position: absolute;
    left: 10px;
    pointer-events: none;
    color: var(--color-text-soft);
  }

  .git-branch-compare-select-arrow {
    position: absolute;
    right: 10px;
    pointer-events: none;
    color: var(--color-text-soft);
  }

  .git-branch-compare-native-select {
    width: 100%;
    height: 100%;
    padding: 0 28px 0 32px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    appearance: none;
    outline: none;

    &:disabled {
      cursor: not-allowed;
      opacity: 0.6;
    }

    option {
      background: var(--color-panel);
      color: var(--color-text);
    }
  }

  .git-branch-compare-swap-btn {
    display: flex;
    width: 32px;
    height: 34px;
    flex: none;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.25s ease;

    &:hover:not(:disabled) {
      border-color: var(--color-primary);
      background: var(--color-primary-soft);
      color: var(--color-primary);
      transform: rotate(180deg);
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.4;
    }
  }

  .git-branch-compare-submit {
    display: inline-flex;
    height: 34px;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 16px;
    border: 1px solid var(--color-primary-solid);
    border-radius: 6px;
    background: var(--color-primary-solid);
    color: #ffffff;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    box-shadow: 0 2px 6px rgba(47, 95, 145, 0.2);
    transition: all 0.2s ease;

    &:hover:not(:disabled) {
      filter: brightness(1.08);
      box-shadow: 0 3px 8px rgba(47, 95, 145, 0.3);
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.5;
      box-shadow: none;
    }
  }

  .git-branch-compare-spinner {
    animation: git-branch-compare-spin 0.9s linear infinite;
  }

  /* 错误横幅 */
  .git-branch-compare-error-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 14px 18px;
    padding: 10px 14px;
    border: 1px solid var(--color-danger-line);
    border-radius: 6px;
    background: var(--color-danger-soft);
    color: var(--color-danger);
    font-size: 12px;
  }

  .git-branch-compare-error-icon {
    flex: none;
  }

  .git-branch-compare-error-msg {
    flex: 1;
  }

  .git-branch-compare-retry-btn {
    padding: 3px 10px;
    border: 1px solid var(--color-danger);
    border-radius: 4px;
    background: transparent;
    color: var(--color-danger);
    cursor: pointer;
    font-size: 11px;
    font-weight: 500;

    &:hover {
      background: var(--color-danger);
      color: #fff;
    }
  }

  /* 加载与空白占位视图 */
  .git-branch-compare-loading-view,
  .git-branch-compare-empty-view {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px;
    color: var(--color-text-muted);
  }

  .git-branch-compare-loader-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .git-branch-compare-loader-icon {
    color: var(--color-primary);
  }

  .git-branch-compare-loader-text {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text);
  }

  .git-branch-compare-loader-subtext {
    font-size: 11px;
    color: var(--color-text-soft);
  }

  .git-branch-compare-empty-placeholder-icon {
    color: var(--color-text-soft);
    opacity: 0.6;
  }

  .git-branch-compare-empty-success-icon {
    color: var(--color-success);
  }

  .git-branch-compare-empty-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .git-branch-compare-empty-subtitle {
    font-size: 12px;
    color: var(--color-text-soft);
  }

  /* KPI 差异统计卡片 */
  .git-branch-compare-stats-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 18px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  .git-branch-compare-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
  }

  .git-branch-compare-stat {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    transition: all 0.2s ease;

    &:hover {
      border-color: var(--color-line-strong);
      transform: translateY(-1px);
      box-shadow: 0 2px 6px rgba(0, 0, 0, 0.04);
    }
  }

  .git-branch-compare-stat-icon-wrap {
    display: flex;
    width: 32px;
    height: 32px;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
  }

  .git-branch-compare-stat-icon--blue {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .git-branch-compare-stat-icon--green {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .git-branch-compare-stat-icon--red {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .git-branch-compare-stat-icon--amber {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .git-branch-compare-stat-content {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  .git-branch-compare-stat-label {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .git-branch-compare-stat-number {
    color: var(--color-text);
    font-family:
      "Bahnschrift", "JetBrains Mono", "Segoe UI Variable", sans-serif;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.2;
  }

  .git-branch-compare-stat-add .git-branch-compare-stat-number {
    color: var(--color-success);
  }

  .git-branch-compare-stat-delete .git-branch-compare-stat-number {
    color: var(--color-danger);
  }

  .git-branch-compare-stat-total {
    border-color: var(--color-line-strong);
  }

  /* 差异变动比例条 */
  .git-branch-compare-ratio-bar-wrap {
    display: flex;
    width: 100%;
    margin-top: 2px;
  }

  .git-branch-compare-ratio-bar {
    display: flex;
    width: 100%;
    height: 4px;
    overflow: hidden;
    border-radius: 2px;
    background: var(--color-line);
  }

  .git-branch-compare-ratio-segment {
    height: 100%;
    transition: width 0.3s ease;
  }

  .git-branch-compare-ratio-add {
    background: var(--color-success);
  }

  .git-branch-compare-ratio-delete {
    background: var(--color-danger);
  }

  /* 主体双栏区域 */
  .git-branch-compare-body {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }

  /* 左侧文件列表侧栏 */
  .git-branch-compare-files {
    display: flex;
    width: 320px;
    flex: 0 0 320px;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    border-right: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .git-branch-compare-files-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-line);
  }

  .git-branch-compare-files-title-box {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .git-branch-compare-files-heading {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text);
  }

  .git-branch-compare-files-badge {
    padding: 1px 6px;
    border-radius: 10px;
    background: var(--color-line);
    color: var(--color-text-muted);
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    font-weight: 600;
  }

  .git-branch-compare-files-view-toggles {
    display: flex;
    align-items: center;
    padding: 2px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    background: var(--color-panel);
  }

  .git-branch-compare-view-toggle {
    display: flex;
    width: 22px;
    height: 22px;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--color-text-soft);
    cursor: pointer;
    transition: all 0.15s ease;

    &:hover {
      color: var(--color-text);
    }

    &.git-branch-compare-view-toggle--active {
      background: var(--color-primary-soft);
      color: var(--color-primary);
    }
  }

  /* 搜索与过滤 */
  .git-branch-compare-files-search-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-line);
  }

  .git-branch-compare-files-search-input-box {
    position: relative;
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
  }

  .git-branch-compare-files-search-icon {
    position: absolute;
    left: 8px;
    pointer-events: none;
    color: var(--color-text-soft);
  }

  .git-branch-compare-files-search-input {
    width: 100%;
    height: 28px;
    padding: 0 24px 0 28px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: 11px;
    outline: none;
    transition: border-color 0.2s ease;

    &:focus {
      border-color: var(--color-primary);
    }
  }

  .git-branch-compare-files-search-clear {
    position: absolute;
    right: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    border: none;
    background: transparent;
    color: var(--color-text-soft);
    cursor: pointer;

    &:hover {
      color: var(--color-text);
    }
  }

  .git-branch-compare-filter-btn {
    display: inline-flex;
    height: 28px;
    flex: none;
    align-items: center;
    gap: 3px;
    padding: 0 7px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 11px;
    transition: all 0.2s ease;

    &:hover {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }

    &.git-branch-compare-filter-btn--active {
      border-color: var(--color-primary);
      background: var(--color-primary-soft);
      color: var(--color-primary);
    }
  }

  .git-branch-compare-filter-tag {
    font-weight: 600;
  }

  /* 文件列表容器 */
  .git-branch-compare-files-scroll-wrap {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .git-branch-compare-files-empty {
    padding: 24px 14px;
    text-align: center;
    font-size: 12px;
    color: var(--color-text-soft);
  }

  .git-branch-compare-files-list,
  .git-branch-compare-files-tree {
    display: flex;
    flex-direction: column;
  }

  /* 文件单项 */
  .git-branch-compare-file {
    position: relative;
    display: flex;
    width: 100%;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 5px 12px;
    border: none;
    border-bottom: 1px solid var(--color-line);
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background 0.15s ease;

    &:hover {
      background: var(--color-panel);
    }

    &.git-branch-compare-file--active {
      background: var(--color-panel);

      &::before {
        content: "";
        position: absolute;
        left: 0;
        top: 0;
        bottom: 0;
        width: 3px;
        background: var(--color-primary);
      }

      .git-branch-compare-file-basename {
        font-weight: 600;
        color: var(--color-primary);
      }
    }
  }

  .git-branch-compare-file-ext-icon {
    display: flex;
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
  }

  .git-branch-compare-file-status {
    display: inline-flex;
    width: 16px;
    height: 16px;
    flex: 0 0 16px;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 600;
  }

  .git-branch-compare-file-status-add {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .git-branch-compare-file-status-delete {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .git-branch-compare-file-status-modify,
  .git-branch-compare-file-status-move,
  .git-branch-compare-file-status-copy {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .git-branch-compare-file-ext-vue {
    color: #41b883;
  }

  .git-branch-compare-file-ext-ts {
    color: #3178c6;
  }

  .git-branch-compare-file-ext-js {
    color: #f7df1e;
  }

  .git-branch-compare-file-ext-json {
    color: #e34c26;
  }

  .git-branch-compare-file-ext-md {
    color: #083fa1;
  }

  .git-branch-compare-file-ext-css {
    color: #264de4;
  }

  .git-branch-compare-file-ext-config {
    color: var(--color-text-soft);
  }

  .git-branch-compare-file-ext-default {
    color: var(--color-text-soft);
  }

  .git-branch-compare-file-name-group {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 1px;
  }

  .git-branch-compare-file-basename {
    overflow: hidden;
    color: var(--color-text);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-branch-compare-file-dirname {
    overflow: hidden;
    color: var(--color-text-soft);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-branch-compare-file-tail {
    display: flex;
    flex: none;
    align-items: center;
    gap: 4px;
  }

  .git-branch-compare-file-binary,
  .git-branch-compare-file-diff-stat {
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 10px;
  }

  .git-branch-compare-file-binary {
    color: var(--color-text-soft);
  }

  .git-branch-compare-file-diff-stat {
    display: inline-flex;
    gap: 4px;
    font-weight: 500;
  }

  .git-branch-compare-file-add {
    color: var(--color-success);
  }

  .git-branch-compare-file-delete {
    color: var(--color-danger);
  }

  /* 目录树节点 */
  .git-branch-compare-tree-dir {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding-top: 4px;
    padding-bottom: 4px;
    padding-right: 12px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 12px;
    text-align: left;
    transition: background 0.15s ease;

    &:hover {
      background: var(--color-panel);
      color: var(--color-text);
    }
  }

  .git-branch-compare-tree-caret {
    flex: none;
    color: var(--color-text-soft);
  }

  .git-branch-compare-tree-dir-icon {
    flex: none;
    color: var(--color-warning);
  }

  .git-branch-compare-tree-dir-name {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-branch-compare-tree-dir-count {
    padding: 0 4px;
    border-radius: 8px;
    background: var(--color-line);
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
  }

  .git-branch-compare-tree-file {
    padding-right: 12px;
  }

  /* 右侧 Diff 区域 */
  .git-branch-compare-diff-pane {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-panel);
  }

  /* 吸顶文件工具栏 */
  .git-branch-compare-diff-topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 38px;
    padding: 0 14px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .git-branch-compare-diff-file-info {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 8px;
  }

  .git-branch-compare-diff-file-name {
    overflow: hidden;
    color: var(--color-text);
    font-family: "JetBrains Mono", monospace;
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-branch-compare-status-tag {
    display: inline-flex;
    align-items: center;
    height: 18px;
    padding: 0 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 500;
  }

  .git-branch-compare-status-tag--add {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .git-branch-compare-status-tag--modify {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .git-branch-compare-status-tag--delete {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .git-branch-compare-status-tag--move,
  .git-branch-compare-status-tag--copy {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .git-branch-compare-diff-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
  }

  .git-branch-compare-nav-buttons {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    background: var(--color-panel);
  }

  .git-branch-compare-icon-btn {
    display: flex;
    width: 26px;
    height: 26px;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: 4px;
    transition: all 0.15s ease;

    &:hover:not(:disabled) {
      background: var(--color-line);
      color: var(--color-text);
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.35;
    }
  }

  .git-branch-compare-copied-icon {
    color: var(--color-success);
  }

  .git-branch-compare-raw-file-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 11px;
    transition: all 0.15s ease;

    &:hover {
      border-color: var(--color-line-strong);
      color: var(--color-text);
    }
  }

  /* 代码差异视口 */
  .git-branch-compare-diff {
    display: block;
    min-width: 0;
    min-height: 0;
    flex: 1;
    margin: 0;
    overflow: auto;
    background: var(--color-page);
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 12px;
    line-height: 20px;

    code {
      display: block;
      min-width: 100%;
      width: max-content;
      box-sizing: border-box;
    }
  }

  /* 差异单行与行号 Gutter */
  .git-branch-compare-line {
    display: flex;
    height: 20px;
    line-height: 20px;
    padding-right: 12px;
    white-space: pre;
    box-sizing: border-box;

    &::before {
      content: attr(data-line);
      display: inline-block;
      width: 44px;
      min-width: 44px;
      flex: none;
      text-align: right;
      padding-right: 10px;
      margin-right: 10px;
      color: var(--color-text-soft);
      border-right: 1px solid var(--color-line);
      user-select: none;
      box-sizing: border-box;
    }
  }

  .git-branch-compare-line-add {
    background: var(--color-success-soft);
    color: var(--color-success);

    &::before {
      color: var(--color-success);
      border-right-color: var(--color-success-line, var(--color-line));
    }
  }

  .git-branch-compare-line-delete {
    background: var(--color-danger-soft);
    color: var(--color-danger);

    &::before {
      color: var(--color-danger);
      border-right-color: var(--color-danger-line, var(--color-line));
    }
  }

  .git-branch-compare-line-chunk {
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-weight: 500;
  }

  .git-branch-compare-line-meta {
    background: var(--color-panel-soft);
    color: var(--color-text-soft);
    font-style: italic;
  }
}

@keyframes git-branch-compare-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

<template>
  <div v-if="visible" class="git-commit-modal" @click="handleBackdropClick">
    <section
      :class="[
        'git-commit-modal-panel',
        { 'git-commit-modal-panel-fullscreen': isFullscreen }
      ]"
      @click.stop
    >
      <!-- 顶部导航栏 -->
      <header class="git-commit-modal-head">
        <div class="git-commit-modal-head-left">
          <button
            class="git-commit-modal-back-btn"
            type="button"
            @click="handleBack"
          >
            <ArrowLeft :size="15" class="git-commit-modal-back-icon" />
            <span>返回</span>
          </button>
          <div class="git-commit-modal-breadcrumb">
            <span class="git-commit-modal-breadcrumb-prefix">{{
              breadcrumbPrefix
            }}</span>
            <span class="git-commit-modal-breadcrumb-sep">/</span>
            <span
              class="git-commit-modal-breadcrumb-target"
              :title="branchName"
            >
              {{ branchName || "分支详情" }}
            </span>
          </div>
        </div>

        <div class="git-commit-modal-head-right">
          <div v-if="displayHash" class="git-commit-modal-hash-badge">
            <span class="git-commit-modal-hash-text">{{ displayHash }}</span>
            <button
              class="git-commit-modal-icon-btn"
              type="button"
              title="复制提交哈希"
              @click="copyText(displayFullHash || displayHash, '提交哈希')"
            >
              <Copy :size="13" />
            </button>
          </div>
          <button
            class="git-commit-modal-close-btn"
            type="button"
            title="关闭 (Esc)"
            @click="$emit('close')"
          >
            <X :size="16" />
          </button>
        </div>
      </header>

      <!-- 滚动容器：包含卡片、提交列表及代码详情 -->
      <div class="git-commit-modal-body">
        <!-- 项目概览卡片 -->
        <div class="git-commit-modal-project-card">
          <div class="git-commit-modal-overview-list">
            <div class="git-commit-modal-overview-item">
              <span class="git-commit-modal-overview-label">项目路径</span>
              <div
                class="git-commit-modal-overview-val"
                :title="projectPath || ''"
              >
                <Folder :size="14" class="git-commit-modal-overview-icon" />
                <span class="git-commit-modal-overview-text">{{
                  projectPath || "-"
                }}</span>
              </div>
            </div>
            <div class="git-commit-modal-overview-item">
              <span class="git-commit-modal-overview-label">分支名称</span>
              <div
                class="git-commit-modal-overview-val"
                :title="branchName || ''"
              >
                <GitBranchIcon
                  :size="14"
                  class="git-commit-modal-overview-icon"
                />
                <span class="git-commit-modal-overview-text">{{
                  branchName || "-"
                }}</span>
              </div>
            </div>
            <div class="git-commit-modal-overview-item">
              <span class="git-commit-modal-overview-label">{{
                type === "archive" ? "归档时间" : "提交时间"
              }}</span>
              <div class="git-commit-modal-overview-val">
                <Calendar :size="14" class="git-commit-modal-overview-icon" />
                <span class="git-commit-modal-overview-text">{{
                  formatFullDate(archivedAt || displayDate) || "-"
                }}</span>
              </div>
            </div>
            <div class="git-commit-modal-overview-item">
              <span class="git-commit-modal-overview-label">{{
                type === "archive" ? "归档提交" : "提交哈希"
              }}</span>
              <div class="git-commit-modal-overview-val">
                <GitCommitHorizontal
                  :size="14"
                  class="git-commit-modal-overview-icon"
                />
                <code class="git-commit-modal-overview-code">{{
                  displayHash || "-"
                }}</code>
                <span
                  v-if="displayAuthor"
                  class="git-commit-modal-overview-author"
                >
                  ({{ displayAuthor }})
                </span>
              </div>
            </div>
          </div>

          <div class="git-commit-modal-project-right">
            <button
              class="git-commit-modal-fullscreen-btn"
              type="button"
              @click="toggleFullscreen"
            >
              <component
                :is="isFullscreen ? Minimize2 : Maximize2"
                :size="14"
              />
              <span>{{ isFullscreen ? "退出全屏" : "查看完整变更" }}</span>
            </button>
          </div>
        </div>

        <!-- 提交历史信息表格（当有提交列表时展示，如归档详情） -->
        <div
          v-if="commits && commits.length > 0"
          class="git-commit-modal-commits-section"
        >
          <div class="git-commit-modal-commits-head">
            <span class="git-commit-modal-th git-commit-modal-th-subject"
              >提交信息</span
            >
            <span class="git-commit-modal-th git-commit-modal-th-date"
              >日期</span
            >
            <span class="git-commit-modal-th git-commit-modal-th-author"
              >作者</span
            >
            <span class="git-commit-modal-th git-commit-modal-th-hash"
              >提交</span
            >
          </div>
          <div class="git-commit-modal-commits-list">
            <button
              v-for="commit in commits"
              :key="commit.rowId || commit.hash"
              :class="[
                'git-commit-modal-commit-row',
                {
                  'git-commit-modal-commit-row-active':
                    commit.hash && commit.hash === currentSelectedHash
                }
              ]"
              type="button"
              :disabled="commit.isGraphOnly"
              @click="handleSelectCommit(commit)"
            >
              <span class="git-commit-modal-td git-commit-modal-td-subject">
                <span class="git-commit-modal-commit-dot"></span>
                <span
                  class="git-commit-modal-commit-subject-text"
                  :title="commit.subject"
                >
                  {{ commit.subject }}
                </span>
              </span>
              <span class="git-commit-modal-td git-commit-modal-td-date">
                {{ formatFullDate(commit.date) }}
              </span>
              <span class="git-commit-modal-td git-commit-modal-td-author">
                {{ commit.author }}
              </span>
              <span class="git-commit-modal-td git-commit-modal-td-hash">
                {{ commit.shortHash || formatHash(commit.hash) }}
              </span>
            </button>
          </div>
        </div>

        <!-- 当前提交详情与变更对比区域 -->
        <div class="git-commit-modal-detail-section">
          <!-- 提交信息头部 -->
          <div class="git-commit-modal-detail-header">
            <div class="git-commit-modal-detail-title-group">
              <h3
                class="git-commit-modal-detail-title"
                :title="activeDetail?.subject || '提交详情'"
              >
                {{ activeDetail?.subject || "提交详情" }}
              </h3>
              <div class="git-commit-modal-detail-meta">
                <span class="git-commit-modal-meta-item">
                  <span class="git-commit-modal-meta-label">完整 hash</span>
                  <code
                    class="git-commit-modal-meta-hash"
                    title="点击复制完整哈希"
                    @click="copyText(activeDetail?.hash, '完整哈希')"
                  >
                    {{ activeDetail?.hash || "-" }}
                  </code>
                </span>
                <span class="git-commit-modal-meta-item">
                  <span class="git-commit-modal-meta-label">作者</span>
                  <span class="git-commit-modal-meta-value">{{
                    activeDetail?.author || "-"
                  }}</span>
                </span>
                <span class="git-commit-modal-meta-item">
                  <span class="git-commit-modal-meta-label">提交时间</span>
                  <span class="git-commit-modal-meta-value">{{
                    formatDate(activeDetail?.date)
                  }}</span>
                </span>
              </div>
            </div>

            <div class="git-commit-modal-detail-actions">
              <span class="git-commit-modal-file-count">
                {{ filesCount }} 个文件发生变化
              </span>
              <button
                class="git-commit-modal-raw-btn"
                type="button"
                :disabled="!selectedFilePath"
                title="在默认编辑器中打开"
                @click="handleOpenRawFile"
              >
                <ChevronRight :size="14" />
                <span>查看原始文件</span>
              </button>
            </div>
          </div>

          <!-- 变更文件树与代码差异两栏布局 -->
          <div class="git-commit-modal-diff-layout">
            <!-- 左侧：变更文件列表树 -->
            <aside class="git-commit-modal-tree-panel">
              <div class="git-commit-modal-tree-head">
                <div class="git-commit-modal-tree-title">
                  <span>变更文件</span>
                  <span class="git-commit-modal-tree-count-badge">{{
                    filesCount
                  }}</span>
                </div>
                <button
                  class="git-commit-modal-icon-btn"
                  type="button"
                  :title="searchVisible ? '关闭搜索' : '搜索文件'"
                  @click="toggleSearch"
                >
                  <Search :size="13" />
                </button>
              </div>

              <!-- 搜索过滤框 -->
              <div v-if="searchVisible" class="git-commit-modal-tree-search">
                <input
                  ref="searchInputRef"
                  v-model="searchQuery"
                  type="text"
                  placeholder="搜索文件路径..."
                  class="git-commit-modal-tree-search-input"
                />
                <button
                  v-if="searchQuery"
                  class="git-commit-modal-tree-search-clear"
                  type="button"
                  @click="searchQuery = ''"
                >
                  <X :size="12" />
                </button>
              </div>

              <div class="git-commit-modal-tree-body">
                <div
                  v-if="loading && !flattenedTreeNodes.length"
                  class="git-commit-modal-tree-empty"
                >
                  正在加载文件列表...
                </div>
                <div
                  v-else-if="!flattenedTreeNodes.length"
                  class="git-commit-modal-tree-empty"
                >
                  {{ searchQuery ? "未找到匹配文件" : "暂无变更文件" }}
                </div>
                <div v-else class="git-commit-modal-tree-list">
                  <template
                    v-for="node in flattenedTreeNodes"
                    :key="node.type + ':' + node.path"
                  >
                    <!-- 目录项 -->
                    <button
                      v-if="node.type === 'directory'"
                      class="git-commit-modal-tree-dir"
                      :style="{ paddingLeft: `${node.level * 14 + 8}px` }"
                      type="button"
                      :title="node.path"
                      @click="toggleDirectory(node.path)"
                    >
                      <component
                        :is="node.isClosed ? ChevronRight : ChevronDown"
                        :size="12"
                        class="git-commit-modal-tree-dir-caret"
                      />
                      <Folder
                        :size="13"
                        class="git-commit-modal-tree-dir-icon"
                      />
                      <span class="git-commit-modal-tree-dir-name">{{
                        node.name
                      }}</span>
                      <span class="git-commit-modal-tree-dir-count">{{
                        node.count
                      }}</span>
                    </button>

                    <!-- 文件项 -->
                    <button
                      v-else
                      :class="[
                        'git-commit-modal-tree-file',
                        {
                          'git-commit-modal-tree-file-active':
                            node.path === selectedFilePath
                        }
                      ]"
                      :style="{ paddingLeft: `${node.level * 14 + 16}px` }"
                      type="button"
                      :title="
                        node.file?.oldPath
                          ? `${node.file.oldPath} -> ${node.path}`
                          : node.path
                      "
                      @click="handleSelectFile(node.path)"
                    >
                      <FileCode
                        :size="13"
                        class="git-commit-modal-tree-file-icon"
                      />
                      <span class="git-commit-modal-tree-file-name">{{
                        node.name
                      }}</span>
                      <span
                        v-if="node.file?.status"
                        :class="[
                          'git-commit-modal-file-status-badge',
                          `git-commit-modal-file-status-badge-${node.file.status.toLowerCase()}`
                        ]"
                      >
                        {{ getFileStatusLabel(node.file.status) }}
                      </span>
                    </button>
                  </template>
                </div>
              </div>
            </aside>

            <!-- 右侧：代码差异对比面板 -->
            <section class="git-commit-modal-diff-panel">
              <!-- Diff 头部工具条 -->
              <div class="git-commit-modal-diff-bar">
                <div class="git-commit-modal-diff-file-info">
                  <span
                    class="git-commit-modal-diff-file-path"
                    :title="selectedFilePath || '请选择文件'"
                  >
                    {{ selectedFilePath || "请选择文件" }}
                  </span>
                  <button
                    v-if="selectedFilePath"
                    class="git-commit-modal-icon-btn"
                    type="button"
                    title="复制文件路径"
                    @click="copyText(selectedFilePath, '文件路径')"
                  >
                    <Copy :size="13" />
                  </button>
                </div>

                <div class="git-commit-modal-diff-tools">
                  <!-- 统计 Badge -->
                  <div
                    v-if="diffStats.additions || diffStats.deletions"
                    class="git-commit-modal-diff-stats"
                  >
                    <span class="git-commit-modal-diff-stat-add"
                      >+{{ diffStats.additions }}</span
                    >
                    <span class="git-commit-modal-diff-stat-del"
                      >-{{ diffStats.deletions }}</span
                    >
                  </div>

                  <!-- 侧栏对比 / 单栏对比切换 -->
                  <button
                    class="git-commit-modal-diff-mode-btn"
                    type="button"
                    title="切换对比布局"
                    @click="toggleDiffMode"
                  >
                    <Columns :size="13" />
                    <span>{{
                      diffMode === "split" ? "单栏对比" : "侧栏对比"
                    }}</span>
                  </button>
                </div>
              </div>

              <!-- Diff 代码内容区 -->
              <div class="git-commit-modal-diff-content">
                <!-- 快捷复制完整 Diff -->
                <button
                  v-if="parsedDiff.lines.length"
                  class="git-commit-modal-diff-copy-float"
                  type="button"
                  title="复制完整 Patch 文本"
                  @click="copyText(activeDetail?.patch, '完整 Patch')"
                >
                  <Copy :size="13" />
                </button>

                <div v-if="loading" class="git-commit-modal-diff-empty">
                  正在读取差异数据...
                </div>

                <!-- 单栏统一对比 (Unified Mode) -->
                <div
                  v-else-if="diffMode === 'unified'"
                  class="git-commit-modal-unified-view"
                >
                  <table
                    v-if="parsedDiff.lines.length"
                    class="git-commit-modal-diff-table"
                  >
                    <tbody>
                      <tr
                        v-for="(line, idx) in parsedDiff.lines"
                        :key="idx"
                        :class="[
                          'git-commit-modal-diff-row',
                          `git-commit-modal-diff-row-${line.type}`
                        ]"
                      >
                        <!-- 旧行号 -->
                        <td
                          class="git-commit-modal-line-num git-commit-modal-line-num-old"
                        >
                          {{ line.oldLineNumber || "" }}
                        </td>
                        <!-- 新行号 -->
                        <td
                          class="git-commit-modal-line-num git-commit-modal-line-num-new"
                        >
                          {{ line.newLineNumber || "" }}
                        </td>
                        <!-- 前缀标识 (+ / - / 空) -->
                        <td class="git-commit-modal-line-prefix">
                          {{ line.prefix }}
                        </td>
                        <!-- 代码内容 -->
                        <td class="git-commit-modal-line-code">
                          <pre class="git-commit-modal-code-pre">{{
                            line.content
                          }}</pre>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                  <div v-else class="git-commit-modal-diff-empty">
                    暂无文件变更差异
                  </div>
                </div>

                <!-- 侧栏双栏对比 (Split Mode) -->
                <div v-else class="git-commit-modal-split-view">
                  <div
                    v-if="parsedDiff.splitRows.length"
                    class="git-commit-modal-split-table"
                  >
                    <div
                      v-for="(row, idx) in parsedDiff.splitRows"
                      :key="idx"
                      :class="[
                        'git-commit-modal-split-row',
                        { 'git-commit-modal-split-row-chunk': row.chunk }
                      ]"
                    >
                      <template v-if="row.chunk">
                        <div class="git-commit-modal-split-chunk">
                          {{ row.chunk.content }}
                        </div>
                      </template>
                      <template v-else>
                        <!-- 左栏：旧版本 -->
                        <div
                          :class="[
                            'git-commit-modal-split-cell git-commit-modal-split-cell-left',
                            row.left
                              ? `git-commit-modal-diff-row-${row.left.type}`
                              : 'git-commit-modal-diff-row-empty'
                          ]"
                        >
                          <span
                            class="git-commit-modal-line-num git-commit-modal-line-num-split"
                          >
                            {{ row.left?.oldLineNumber || "" }}
                          </span>
                          <span class="git-commit-modal-line-prefix">
                            {{ row.left?.prefix || "" }}
                          </span>
                          <pre class="git-commit-modal-code-pre">{{
                            row.left?.content || ""
                          }}</pre>
                        </div>

                        <!-- 右栏：新版本 -->
                        <div
                          :class="[
                            'git-commit-modal-split-cell git-commit-modal-split-cell-right',
                            row.right
                              ? `git-commit-modal-diff-row-${row.right.type}`
                              : 'git-commit-modal-diff-row-empty'
                          ]"
                        >
                          <span
                            class="git-commit-modal-line-num git-commit-modal-line-num-split"
                          >
                            {{ row.right?.newLineNumber || "" }}
                          </span>
                          <span class="git-commit-modal-line-prefix">
                            {{ row.right?.prefix || "" }}
                          </span>
                          <pre class="git-commit-modal-code-pre">{{
                            row.right?.content || ""
                          }}</pre>
                        </div>
                      </template>
                    </div>
                  </div>
                  <div v-else class="git-commit-modal-diff-empty">
                    暂无文件变更差异
                  </div>
                </div>
              </div>
            </section>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import {
  ArrowLeft,
  Calendar,
  ChevronDown,
  ChevronRight,
  Columns,
  Copy,
  FileCode,
  Folder,
  GitBranch as GitBranchIcon,
  GitCommitHorizontal,
  Maximize2,
  Minimize2,
  Search,
  X
} from "lucide-vue-next"
import { createMessage } from "@/utils/message"
import { systemApi } from "@/api"

const props = defineProps({
  visible: {
    type: Boolean,
    default: false
  },
  type: {
    type: String,
    default: "archive" // 'archive' | 'commit' | 'stash'
  },
  projectPath: {
    type: String,
    default: ""
  },
  branchName: {
    type: String,
    default: ""
  },
  archivedAt: {
    type: [Number, String],
    default: null
  },
  commitHash: {
    type: String,
    default: ""
  },
  author: {
    type: String,
    default: ""
  },
  commits: {
    type: Array,
    default: () => []
  },
  selectedCommitHash: {
    type: String,
    default: ""
  },
  commitDetail: {
    type: Object,
    default: null
  },
  loading: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits([
  "close",
  "back",
  "select-commit",
  "select-file",
  "open-raw-file"
])

// 响应式状态
const isFullscreen = ref(false)
const diffMode = ref("unified") // 'unified' | 'split'
const searchQuery = ref("")
const searchVisible = ref(false)
const searchInputRef = ref(null)
const closedDirectories = ref(new Set())

// 当前选中的提交哈希
const currentSelectedHash = computed(() => {
  return (
    props.selectedCommitHash ||
    props.commitDetail?.hash ||
    props.commits?.[0]?.hash ||
    props.commitHash ||
    ""
  )
})

// 展示的短哈希
const displayHash = computed(() => {
  const hash =
    currentSelectedHash.value ||
    props.commitDetail?.hash ||
    props.commitHash ||
    ""
  return formatHash(hash)
})

const displayFullHash = computed(() => {
  return (
    currentSelectedHash.value ||
    props.commitDetail?.hash ||
    props.commitHash ||
    ""
  )
})

// 展示的作者
const displayAuthor = computed(() => {
  return (
    props.commitDetail?.author ||
    props.commits?.[0]?.author ||
    props.author ||
    ""
  )
})

// 展示的日期
const displayDate = computed(() => {
  return (
    props.archivedAt ||
    props.commitDetail?.date ||
    props.commits?.[0]?.date ||
    Date.now()
  )
})

// 面包屑前缀文案
const breadcrumbPrefix = computed(() => {
  if (props.type === "archive") return "归档列表"
  if (props.type === "stash") return "Stash 记录"
  return "分支列表"
})

// 活跃的提交详情对象
const activeDetail = computed(() => {
  return props.commitDetail || null
})

// 当前选中的文件路径
const selectedFilePath = computed(() => {
  return props.commitDetail?.selectedFilePath || ""
})

// 文件总数
const filesCount = computed(() => {
  return props.commitDetail?.files?.length || 0
})

// 监听提交变化时重置折叠状态
watch(
  () => props.commitDetail?.hash,
  () => {
    closedDirectories.value = new Set()
  }
)

function toggleFullscreen() {
  isFullscreen.value = !isFullscreen.value
}

function toggleDiffMode() {
  diffMode.value = diffMode.value === "unified" ? "split" : "unified"
}

function toggleSearch() {
  searchVisible.value = !searchVisible.value
  if (searchVisible.value) {
    nextTick(() => {
      searchInputRef.value?.focus()
    })
  } else {
    searchQuery.value = ""
  }
}

function handleBack() {
  emit("back")
}

function handleBackdropClick() {
  emit("close")
}

function handleSelectCommit(commit) {
  emit("select-commit", commit)
}

function handleSelectFile(path) {
  emit("select-file", path)
}

async function handleOpenRawFile() {
  if (!selectedFilePath.value) return

  const fullPath = props.projectPath
    ? `${props.projectPath.replace(/[\\/]+$/, "")}/${selectedFilePath.value.replace(/^[\\/]+/, "")}`
    : selectedFilePath.value

  try {
    await systemApi.openPath({ path: fullPath })
    createMessage.success(`已在编辑器中打开文件`)
  } catch {
    await copyText(fullPath, "文件路径")
  }
}

async function copyText(text, label = "内容") {
  if (!text) return
  try {
    await navigator.clipboard.writeText(String(text))
    createMessage.success(`${label}已复制到剪贴板`)
  } catch {
    createMessage.error("复制失败")
  }
}

// 格式化函数
function formatHash(value) {
  return String(value || "").slice(0, 8)
}

function formatDate(value) {
  if (!value) return "-"
  const d = new Date(value)
  const month = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  const hour = String(d.getHours()).padStart(2, "0")
  const min = String(d.getMinutes()).padStart(2, "0")
  return `${month}/${day} ${hour}:${min}`
}

function formatFullDate(value) {
  if (!value) return "-"
  const d = new Date(value)
  const year = d.getFullYear()
  const month = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  const hour = String(d.getHours()).padStart(2, "0")
  const min = String(d.getMinutes()).padStart(2, "0")
  return `${year}/${month}/${day} ${hour}:${min}`
}

function getFileStatusLabel(status) {
  const map = {
    A: "增",
    M: "改",
    D: "删",
    R: "移",
    C: "拷"
  }
  return map[status] || status || "改"
}

// 文件树结构构建与扁平化
const rawFileTree = computed(() => {
  const files = props.commitDetail?.files || []
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
    if (a.type !== b.type) {
      return a.type === "directory" ? -1 : 1
    }
    return a.name.localeCompare(b.name)
  })
  node.children.forEach(sortTree)
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

// 扁平化展示节点，支持折叠与搜索
const flattenedTreeNodes = computed(() => {
  const query = searchQuery.value.trim().toLowerCase()
  const result = []

  function traverse(node, level = 0) {
    if (!node.children) return

    for (const child of node.children) {
      if (query && !matchNode(child, query)) {
        continue
      }

      const isDir = child.type === "directory"
      const isClosed = !query && closedDirectories.value.has(child.path)

      result.push({
        ...child,
        level,
        isClosed
      })

      if (isDir && !isClosed) {
        traverse(child, level + 1)
      }
    }
  }

  function matchNode(node, q) {
    if (node.path.toLowerCase().includes(q)) return true
    if (node.children) {
      return node.children.some((child) => matchNode(child, q))
    }
    return false
  }

  traverse(rawFileTree.value, 0)
  return result
})

// 解析 Git Patch 差异内容
const parsedDiff = computed(() => {
  const patch = props.commitDetail?.patch || ""
  if (!patch) {
    return { lines: [], splitRows: [], additions: 0, deletions: 0 }
  }

  const rawLines = patch.split("\n")
  const lines = []
  let oldLine = 0
  let newLine = 0
  let additions = 0
  let deletions = 0

  for (let i = 0; i < rawLines.length; i++) {
    const raw = rawLines[i]
    if (
      raw.startsWith("diff --git") ||
      raw.startsWith("index ") ||
      raw.startsWith("--- ") ||
      raw.startsWith("+++ ")
    ) {
      continue
    }

    if (raw.startsWith("@@")) {
      const match = raw.match(
        /@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@/
      )
      if (match) {
        oldLine = parseInt(match[1], 10)
        newLine = parseInt(match[3], 10)
      }
      lines.push({
        type: "chunk",
        oldLineNumber: oldLine,
        newLineNumber: newLine,
        prefix: "",
        content: raw
      })
      continue
    }

    if (raw.startsWith("+")) {
      additions++
      lines.push({
        type: "add",
        oldLineNumber: oldLine,
        newLineNumber: newLine++,
        prefix: "+",
        content: raw.slice(1)
      })
    } else if (raw.startsWith("-")) {
      deletions++
      lines.push({
        type: "delete",
        oldLineNumber: oldLine++,
        newLineNumber: newLine,
        prefix: "-",
        content: raw.slice(1)
      })
    } else {
      lines.push({
        type: "context",
        oldLineNumber: oldLine++,
        newLineNumber: newLine++,
        prefix: " ",
        content: raw.startsWith(" ") ? raw.slice(1) : raw
      })
    }
  }

  // 构建侧栏对比双列行结构
  const splitRows = []
  let delQueue = []
  let addQueue = []

  function flushQueues() {
    const maxLen = Math.max(delQueue.length, addQueue.length)
    for (let k = 0; k < maxLen; k++) {
      splitRows.push({
        left: delQueue[k] || null,
        right: addQueue[k] || null
      })
    }
    delQueue = []
    addQueue = []
  }

  for (const line of lines) {
    if (line.type === "chunk") {
      flushQueues()
      splitRows.push({ chunk: line })
    } else if (line.type === "delete") {
      delQueue.push(line)
    } else if (line.type === "add") {
      addQueue.push(line)
    } else {
      flushQueues()
      splitRows.push({
        left: line,
        right: line
      })
    }
  }
  flushQueues()

  return { lines, splitRows, additions, deletions }
})

const diffStats = computed(() => {
  return {
    additions: parsedDiff.value.additions,
    deletions: parsedDiff.value.deletions
  }
})
</script>

<style scoped lang="less">
.git-commit-modal {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(15, 23, 42, 0.4);
  backdrop-filter: blur(4px);
  animation: fadeIn 0.15s ease-out;

  .git-commit-modal-panel {
    display: flex;
    flex-direction: column;
    width: min(1360px, 96vw);
    height: min(920px, 94vh);
    max-height: 96vh;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 25px 50px -12px rgba(15, 23, 42, 0.25);
    overflow: hidden;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);

    &:where(.git-commit-modal-panel-fullscreen) {
      width: 100vw;
      height: 100vh;
      max-height: 100vh;
      border-radius: 0;
      border: none;
    }

    // 顶部导航栏
    .git-commit-modal-head {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      height: 48px;
      padding: 0 16px;
      border-bottom: 1px solid var(--color-line);
      background: var(--color-panel);

      .git-commit-modal-head-left {
        display: flex;
        align-items: center;
        gap: 12px;

        .git-commit-modal-back-btn {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 4px 8px;
          border: 0;
          border-radius: 6px;
          background: transparent;
          color: var(--color-text);
          font-size: 13.5px;
          cursor: pointer;
          transition: background 0.15s ease;

          &:hover {
            background: var(--color-panel-soft);
            color: var(--color-primary);
          }

          .git-commit-modal-back-icon {
            flex-shrink: 0;
          }
        }

        .git-commit-modal-breadcrumb {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          font-size: 13.5px;

          .git-commit-modal-breadcrumb-prefix {
            color: var(--color-text-muted);
          }

          .git-commit-modal-breadcrumb-sep {
            color: var(--color-line-strong);
          }

          .git-commit-modal-breadcrumb-target {
            color: var(--color-text);
            max-width: 420px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
        }
      }

      .git-commit-modal-head-right {
        display: flex;
        align-items: center;
        gap: 10px;

        .git-commit-modal-hash-badge {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          height: 28px;
          padding: 0 8px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          font-family: "JetBrains Mono", Consolas, monospace;
          font-size: 12.5px;

          .git-commit-modal-icon-btn {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            padding: 2px;
            border: 0;
            background: transparent;
            color: var(--color-text-muted);
            cursor: pointer;
            border-radius: 4px;

            &:hover {
              color: var(--color-primary);
              background: var(--color-primary-soft);
            }
          }
        }

        .git-commit-modal-close-btn {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          width: 30px;
          height: 30px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: transparent;
          color: var(--color-text-muted);
          cursor: pointer;
          transition: all 0.15s ease;

          &:hover {
            border-color: var(--color-line-strong);
            background: var(--color-panel-soft);
            color: var(--color-text);
          }
        }
      }
    }

    // 主体滚动区域
    .git-commit-modal-body {
      display: flex;
      flex: 1;
      min-height: 0;
      flex-direction: column;
      overflow-y: auto;
      background: var(--color-panel);
      padding: 12px 16px 16px;
      gap: 12px;

      // 项目概览卡片
      .git-commit-modal-project-card {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        padding: 14px 18px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);

        .git-commit-modal-overview-list {
          display: flex;
          gap: 16px;
          flex: 1;
          min-width: 0;

          .git-commit-modal-overview-item {
            flex: 1;
            display: flex;
            flex-direction: column;
            gap: 5px;
            min-width: 0;

            .git-commit-modal-overview-label {
              font-size: 11.5px;
              color: var(--color-text-muted);
            }

            .git-commit-modal-overview-val {
              display: flex;
              align-items: center;
              gap: 6px;
              font-size: 13px;
              color: var(--color-text);
              min-width: 0;

              .git-commit-modal-overview-icon {
                flex-shrink: 0;
                color: var(--color-primary);
              }

              .git-commit-modal-overview-text {
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
              }

              .git-commit-modal-overview-code {
                font-family: "JetBrains Mono", Consolas, monospace;
                font-size: 12.5px;
                color: var(--color-primary);
              }

              .git-commit-modal-overview-author {
                color: var(--color-text-muted);
                font-size: 12px;
              }
            }
          }
        }

        .git-commit-modal-project-right {
          .git-commit-modal-fullscreen-btn {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            height: 32px;
            padding: 0 12px;
            border: 1px solid #bfdbfe;
            border-radius: 6px;
            background: #eff6ff;
            color: #2563eb;
            font-size: 12.5px;
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover {
              border-color: #93c5fd;
              background: #dbeafe;
            }
          }
        }
      }

      // 提交表格区域
      .git-commit-modal-commits-section {
        display: flex;
        flex: none;
        flex-direction: column;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        overflow: hidden;
        background: var(--color-panel);

        .git-commit-modal-commits-head {
          display: grid;
          grid-template-columns: minmax(360px, 1fr) 160px 110px 90px;
          align-items: center;
          height: 34px;
          padding: 0 14px;
          border-bottom: 1px solid var(--color-line);
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          font-size: 12.5px;
        }

        .git-commit-modal-commits-list {
          max-height: 230px;
          overflow-y: auto;

          .git-commit-modal-commit-row {
            display: grid;
            grid-template-columns: minmax(360px, 1fr) 160px 110px 90px;
            align-items: center;
            width: 100%;
            height: 36px;
            padding: 0 14px;
            border: 0;
            border-bottom: 1px solid var(--color-line);
            background: var(--color-panel);
            color: var(--color-text);
            font-size: 13px;
            text-align: left;
            cursor: pointer;
            transition: background 0.12s ease;

            &:last-child {
              border-bottom: 0;
            }

            &:hover {
              background: var(--color-panel-soft);
            }

            &:where(.git-commit-modal-commit-row-active) {
              background: #eff6ff;
            }

            &.git-commit-modal-commit-row-active {
              .git-commit-modal-td {
                .git-commit-modal-commit-subject-text {
                  color: #2563eb;
                }
              }
            }

            .git-commit-modal-td {
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;

              &:where(.git-commit-modal-td-subject) {
                display: flex;
                align-items: center;
                gap: 8px;
              }

              &:where(.git-commit-modal-td-date) {
                color: var(--color-text-muted);
                font-size: 12.5px;
              }

              &:where(.git-commit-modal-td-author) {
                color: var(--color-text);
                font-size: 12.5px;
              }

              &:where(.git-commit-modal-td-hash) {
                font-family: "JetBrains Mono", Consolas, monospace;
                font-size: 12px;
                color: var(--color-text-muted);
              }

              .git-commit-modal-commit-dot {
                width: 7px;
                height: 7px;
                border-radius: 50%;
                background: #3b82f6;
                flex-shrink: 0;
              }
            }
          }
        }
      }

      // 提交详情及 Diff 面板区域
      .git-commit-modal-detail-section {
        display: flex;
        flex: 1;
        // 保留变更内容的阅读高度，小窗口由外层主体滚动，避免文件树和代码区被压扁。
        min-height: 360px;
        flex-direction: column;
        gap: 8px;

        .git-commit-modal-detail-header {
          display: flex;
          flex: none;
          align-items: flex-start;
          justify-content: space-between;
          gap: 16px;
          padding: 6px 2px;

          .git-commit-modal-detail-title-group {
            .git-commit-modal-detail-title {
              font: inherit;
              margin: 0;
              font-size: 16px;
              color: var(--color-text);
              line-height: 1.4;
            }

            .git-commit-modal-detail-meta {
              display: flex;
              flex-wrap: wrap;
              align-items: center;
              gap: 16px;
              margin-top: 4px;
              font-size: 12.5px;
              color: var(--color-text-muted);

              .git-commit-modal-meta-item {
                display: inline-flex;
                align-items: center;
                gap: 6px;

                .git-commit-modal-meta-label {
                  color: var(--color-text-muted);
                }

                .git-commit-modal-meta-hash {
                  font-family: "JetBrains Mono", Consolas, monospace;
                  font-size: 12px;
                  padding: 1px 6px;
                  border-radius: 4px;
                  background: var(--color-panel-soft);
                  border: 1px solid var(--color-line);
                  cursor: pointer;

                  &:hover {
                    border-color: #93c5fd;
                    color: #2563eb;
                  }
                }

                .git-commit-modal-meta-value {
                  color: var(--color-text);
                }
              }
            }
          }

          .git-commit-modal-detail-actions {
            display: flex;
            align-items: center;
            gap: 12px;
            flex-shrink: 0;

            .git-commit-modal-file-count {
              font-size: 12.5px;
              color: var(--color-text-muted);
            }

            .git-commit-modal-raw-btn {
              display: inline-flex;
              align-items: center;
              gap: 2px;
              border: 0;
              background: transparent;
              color: #2563eb;
              font-size: 13px;
              cursor: pointer;
              padding: 4px 6px;
              border-radius: 4px;

              &:hover:not(:disabled) {
                background: #eff6ff;
              }

              &:disabled {
                opacity: 0.5;
                cursor: not-allowed;
              }
            }
          }
        }

        // 左右分栏布局
        .git-commit-modal-diff-layout {
          display: flex;
          flex: 1;
          min-height: 260px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          overflow: hidden;
          background: var(--color-panel);

          // 左侧文件树
          .git-commit-modal-tree-panel {
            display: flex;
            flex-direction: column;
            width: 250px;
            flex-shrink: 0;
            border-right: 1px solid var(--color-line);
            background: var(--color-panel);

            .git-commit-modal-tree-head {
              display: flex;
              align-items: center;
              justify-content: space-between;
              height: 38px;
              padding: 0 12px;
              border-bottom: 1px solid var(--color-line);
              background: var(--color-panel-soft);

              .git-commit-modal-icon-btn {
                display: inline-flex;
                align-items: center;
                justify-content: center;
                padding: 2px;
                border: 0;
                background: transparent;
                color: var(--color-text-muted);
                cursor: pointer;
                border-radius: 4px;

                &:hover {
                  color: var(--color-primary);
                  background: var(--color-primary-soft);
                }
              }

              .git-commit-modal-tree-title {
                display: inline-flex;
                align-items: center;
                gap: 6px;
                font-size: 13px;
                color: var(--color-text);

                .git-commit-modal-tree-count-badge {
                  display: inline-flex;
                  align-items: center;
                  justify-content: center;
                  min-width: 18px;
                  height: 18px;
                  padding: 0 5px;
                  border-radius: 9px;
                  background: var(--color-line);
                  color: var(--color-text-muted);
                  font-size: 11px;
                }
              }
            }

            .git-commit-modal-tree-search {
              position: relative;
              padding: 6px 8px;
              border-bottom: 1px solid var(--color-line);
              background: var(--color-panel);

              .git-commit-modal-tree-search-input {
                width: 100%;
                height: 28px;
                padding: 0 24px 0 8px;
                border: 1px solid var(--color-line);
                border-radius: 4px;
                background: var(--color-panel-soft);
                color: var(--color-text);
                font-size: 12px;
                outline: none;

                &:focus {
                  border-color: #3b82f6;
                }
              }

              .git-commit-modal-tree-search-clear {
                position: absolute;
                right: 12px;
                top: 50%;
                transform: translateY(-50%);
                border: 0;
                background: transparent;
                color: var(--color-text-muted);
                cursor: pointer;
              }
            }

            .git-commit-modal-tree-body {
              flex: 1;
              min-height: 0;
              overflow-y: auto;
              padding: 4px 0;

              .git-commit-modal-tree-empty {
                padding: 24px 12px;
                text-align: center;
                color: var(--color-text-muted);
                font-size: 12px;
              }

              .git-commit-modal-tree-list {
                .git-commit-modal-tree-dir {
                  display: flex;
                  align-items: center;
                  gap: 6px;
                  width: 100%;
                  min-height: 28px;
                  border: 0;
                  background: transparent;
                  color: var(--color-text);
                  font-size: 12.5px;
                  text-align: left;
                  cursor: pointer;
                  border-radius: 4px;
                  transition: background 0.1s ease;

                  &:hover {
                    background: var(--color-panel-soft);
                  }

                  .git-commit-modal-tree-dir-caret {
                    flex-shrink: 0;
                    color: var(--color-text-muted);
                  }

                  .git-commit-modal-tree-dir-icon {
                    flex-shrink: 0;
                    color: #60a5fa;
                  }

                  .git-commit-modal-tree-dir-name {
                    flex: 1;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                  }

                  .git-commit-modal-tree-dir-count {
                    color: var(--color-text-muted);
                    font-size: 11px;
                    margin-right: 8px;
                  }
                }

                .git-commit-modal-tree-file {
                  display: flex;
                  align-items: center;
                  gap: 6px;
                  width: 100%;
                  min-height: 28px;
                  border: 0;
                  background: transparent;
                  color: var(--color-text);
                  font-size: 12.5px;
                  text-align: left;
                  cursor: pointer;
                  border-radius: 4px;
                  transition: background 0.1s ease;
                  margin: 1px 0;

                  &:hover {
                    background: var(--color-panel-soft);
                  }

                  &:where(.git-commit-modal-tree-file-active) {
                    background: #3b82f6;
                    color: #ffffff;
                    border-radius: 6px;
                  }

                  &.git-commit-modal-tree-file-active {
                    .git-commit-modal-tree-file-icon {
                      color: #ffffff;
                    }
                  }

                  &:where(.git-commit-modal-tree-file-active):hover {
                    background: #2563eb;
                  }

                  .git-commit-modal-tree-file-icon {
                    flex-shrink: 0;
                    color: var(--color-text-muted);
                  }

                  .git-commit-modal-tree-file-name {
                    flex: 1;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                  }

                  .git-commit-modal-file-status-badge {
                    padding: 1px 4px;
                    border-radius: 3px;
                    font-size: 11px;
                    margin-right: 8px;

                    &:where(.git-commit-modal-file-status-badge-m) {
                      background: #fef3c7;
                      color: #d97706;
                    }

                    &:where(.git-commit-modal-file-status-badge-a) {
                      background: #dcfce7;
                      color: #16a34a;
                    }

                    &:where(.git-commit-modal-file-status-badge-d) {
                      background: #fee2e2;
                      color: #dc2626;
                    }

                    &:where(.git-commit-modal-file-status-badge-r) {
                      background: #f3e8ff;
                      color: #9333ea;
                    }
                  }
                }
              }
            }
          }

          // 右侧 Diff 面板
          .git-commit-modal-diff-panel {
            display: flex;
            flex: 1;
            min-width: 0;
            flex-direction: column;
            background: var(--color-panel);

            .git-commit-modal-diff-bar {
              display: flex;
              align-items: center;
              justify-content: space-between;
              height: 38px;
              padding: 0 12px;
              border-bottom: 1px solid var(--color-line);
              background: var(--color-panel-soft);

              .git-commit-modal-diff-file-info {
                display: flex;
                align-items: center;
                gap: 8px;
                min-width: 0;

                .git-commit-modal-icon-btn {
                  display: inline-flex;
                  align-items: center;
                  justify-content: center;
                  padding: 2px;
                  border: 0;
                  background: transparent;
                  color: var(--color-text-muted);
                  cursor: pointer;
                  border-radius: 4px;

                  &:hover {
                    color: var(--color-primary);
                    background: var(--color-primary-soft);
                  }
                }

                .git-commit-modal-diff-file-path {
                  font-size: 13px;
                  color: var(--color-text);
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                }
              }

              .git-commit-modal-diff-tools {
                display: flex;
                align-items: center;
                gap: 10px;
                flex-shrink: 0;

                .git-commit-modal-diff-stats {
                  display: inline-flex;
                  align-items: center;
                  gap: 4px;
                  padding: 2px 6px;
                  border-radius: 4px;
                  font-size: 12px;
                  font-family: "JetBrains Mono", Consolas, monospace;
                  background: var(--color-panel);
                  border: 1px solid var(--color-line);

                  .git-commit-modal-diff-stat-add {
                    color: #16a34a;
                  }

                  .git-commit-modal-diff-stat-del {
                    color: #dc2626;
                  }
                }

                .git-commit-modal-diff-mode-btn {
                  display: inline-flex;
                  align-items: center;
                  gap: 4px;
                  height: 26px;
                  padding: 0 8px;
                  border: 1px solid var(--color-line);
                  border-radius: 4px;
                  background: var(--color-panel);
                  color: var(--color-text);
                  font-size: 12px;
                  cursor: pointer;

                  &:hover {
                    border-color: #93c5fd;
                    color: #2563eb;
                  }
                }
              }
            }

            .git-commit-modal-diff-content {
              position: relative;
              flex: 1;
              min-height: 0;
              overflow: auto;
              font-family: "JetBrains Mono", Consolas, "Courier New", monospace;
              font-size: 12.5px;
              line-height: 1.6;
              background: var(--color-panel);

              .git-commit-modal-diff-copy-float {
                position: absolute;
                top: 8px;
                right: 8px;
                z-index: 5;
                display: inline-flex;
                align-items: center;
                justify-content: center;
                width: 26px;
                height: 26px;
                border: 1px solid var(--color-line);
                border-radius: 4px;
                background: var(--color-panel);
                color: var(--color-text-muted);
                cursor: pointer;
                box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);

                &:hover {
                  border-color: #93c5fd;
                  color: #2563eb;
                }
              }

              .git-commit-modal-diff-empty {
                padding: 48px;
                text-align: center;
                color: var(--color-text-muted);
                font-size: 13px;
              }

              .git-commit-modal-unified-view {
                .git-commit-modal-diff-empty {
                  padding: 48px;
                  text-align: center;
                  color: var(--color-text-muted);
                  font-size: 13px;
                }

                // Unified 对比表格
                .git-commit-modal-diff-table {
                  width: 100%;
                  border-collapse: collapse;

                  .git-commit-modal-diff-row {
                    &:where(.git-commit-modal-diff-row-chunk) {
                      background: #f1f5f9;
                      color: #64748b;
                    }

                    &:where(.git-commit-modal-diff-row-add) {
                      background: #f0fdf4;
                      color: #15803d;
                    }

                    &.git-commit-modal-diff-row-add {
                      .git-commit-modal-line-prefix {
                        color: #16a34a;
                      }
                    }

                    &:where(.git-commit-modal-diff-row-delete) {
                      background: #fef2f2;
                      color: #b91c1c;
                    }

                    &.git-commit-modal-diff-row-delete {
                      .git-commit-modal-line-prefix {
                        color: #dc2626;
                      }
                    }

                    &:where(.git-commit-modal-diff-row-context) {
                      color: var(--color-text);
                    }

                    .git-commit-modal-line-num {
                      width: 44px;
                      padding: 0 6px;
                      text-align: right;
                      user-select: none;
                      color: #94a3b8;
                      font-size: 11.5px;
                      border-right: 1px solid var(--color-line);
                    }

                    .git-commit-modal-line-prefix {
                      width: 22px;
                      text-align: center;
                      user-select: none;
                    }

                    .git-commit-modal-line-code {
                      padding: 0 8px;
                      word-break: break-all;
                      white-space: pre-wrap;

                      .git-commit-modal-code-pre {
                        margin: 0;
                        font-family: inherit;
                        font-size: inherit;
                        line-height: inherit;
                        white-space: pre-wrap;
                        word-break: break-all;
                      }
                    }
                  }
                }
              }

              .git-commit-modal-split-view {
                .git-commit-modal-diff-empty {
                  padding: 48px;
                  text-align: center;
                  color: var(--color-text-muted);
                  font-size: 13px;
                }

                // Split 双栏模式
                .git-commit-modal-split-table {
                  display: flex;
                  flex-direction: column;
                  min-width: 800px;

                  .git-commit-modal-split-row {
                    display: flex;
                    width: 100%;
                    border-bottom: 1px solid rgba(0, 0, 0, 0.03);

                    &:where(.git-commit-modal-split-row-chunk) {
                      background: #f1f5f9;
                      color: #64748b;
                    }

                    .git-commit-modal-split-chunk {
                      width: 100%;
                      padding: 2px 12px;
                      font-size: 11.5px;
                    }

                    .git-commit-modal-split-cell {
                      display: flex;
                      align-items: flex-start;
                      width: 50%;
                      min-width: 0;
                      padding: 0 6px 0 0;

                      &:where(.git-commit-modal-diff-row-chunk) {
                        background: #f1f5f9;
                        color: #64748b;
                      }

                      &:where(.git-commit-modal-diff-row-add) {
                        background: #f0fdf4;
                        color: #15803d;
                      }

                      &.git-commit-modal-diff-row-add {
                        .git-commit-modal-line-prefix {
                          color: #16a34a;
                        }
                      }

                      &:where(.git-commit-modal-diff-row-delete) {
                        background: #fef2f2;
                        color: #b91c1c;
                      }

                      &.git-commit-modal-diff-row-delete {
                        .git-commit-modal-line-prefix {
                          color: #dc2626;
                        }
                      }

                      &:where(.git-commit-modal-diff-row-context) {
                        color: var(--color-text);
                      }

                      &:where(.git-commit-modal-split-cell-left) {
                        border-right: 1px solid var(--color-line);
                      }

                      &:where(.git-commit-modal-diff-row-empty) {
                        background: transparent;
                      }

                      .git-commit-modal-line-num {
                        width: 44px;
                        padding: 0 6px;
                        text-align: right;
                        user-select: none;
                        color: #94a3b8;
                        font-size: 11.5px;
                        border-right: 1px solid var(--color-line);

                        &:where(.git-commit-modal-line-num-split) {
                          width: 42px;
                          flex-shrink: 0;
                        }
                      }

                      .git-commit-modal-line-prefix {
                        width: 22px;
                        text-align: center;
                        user-select: none;
                      }

                      .git-commit-modal-code-pre {
                        margin: 0;
                        font-family: inherit;
                        font-size: inherit;
                        line-height: inherit;
                        white-space: pre-wrap;
                        word-break: break-all;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}

// 动画
@keyframes fadeIn {
  from {
    opacity: 0;
    transform: scale(0.99);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

// 暗色主题适配
:root[data-theme="dark"] {
  .git-commit-modal {
    background: rgba(0, 0, 0, 0.65);

    .git-commit-modal-panel {
      .git-commit-modal-body {
        .git-commit-modal-project-card {
          background: #1e293b;
          border-color: #334155;

          .git-commit-modal-project-right {
            .git-commit-modal-fullscreen-btn {
              background: #1e3a8a;
              border-color: #2563eb;
              color: #93c5fd;

              &:hover {
                background: #1d4ed8;
              }
            }
          }
        }

        .git-commit-modal-commits-section {
          .git-commit-modal-commits-list {
            .git-commit-modal-commit-row {
              &:where(.git-commit-modal-commit-row-active) {
                background: #1e293b;
              }

              &.git-commit-modal-commit-row-active {
                .git-commit-modal-td {
                  .git-commit-modal-commit-subject-text {
                    color: #60a5fa;
                  }
                }
              }
            }
          }
        }

        .git-commit-modal-detail-section {
          .git-commit-modal-detail-header {
            .git-commit-modal-detail-actions {
              .git-commit-modal-raw-btn {
                color: #60a5fa;

                &:hover:not(:disabled) {
                  background: #1e293b;
                }
              }
            }
          }

          .git-commit-modal-diff-layout {
            .git-commit-modal-tree-panel {
              .git-commit-modal-tree-body {
                .git-commit-modal-tree-list {
                  .git-commit-modal-tree-file {
                    &:where(.git-commit-modal-tree-file-active) {
                      background: #1e293b;
                      color: #60a5fa;
                    }
                  }
                }
              }
            }

            .git-commit-modal-diff-panel {
              .git-commit-modal-diff-content {
                .git-commit-modal-unified-view {
                  .git-commit-modal-diff-table {
                    .git-commit-modal-diff-row {
                      &:where(.git-commit-modal-diff-row-chunk) {
                        background: #1e293b;
                        color: #94a3b8;
                      }

                      &:where(.git-commit-modal-diff-row-add) {
                        background: rgba(34, 197, 94, 0.15);
                        color: #86efac;
                      }

                      &:where(.git-commit-modal-diff-row-delete) {
                        background: rgba(239, 68, 68, 0.15);
                        color: #fca5a5;
                      }
                    }
                  }
                }

                .git-commit-modal-split-view {
                  .git-commit-modal-split-table {
                    .git-commit-modal-split-row {
                      .git-commit-modal-split-cell {
                        &:where(.git-commit-modal-diff-row-chunk) {
                          background: #1e293b;
                          color: #94a3b8;
                        }

                        &:where(.git-commit-modal-diff-row-add) {
                          background: rgba(34, 197, 94, 0.15);
                          color: #86efac;
                        }

                        &:where(.git-commit-modal-diff-row-delete) {
                          background: rgba(239, 68, 68, 0.15);
                          color: #fca5a5;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
</style>

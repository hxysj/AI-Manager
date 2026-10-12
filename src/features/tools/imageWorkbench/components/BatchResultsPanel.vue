<template>
  <section class="batch-results-panel">
    <Transition name="workbench-fade">
      <div v-if="loading" class="workbench-loading-overlay">
        <LoaderCircle :size="32" class="spinning" />
        <span class="overlay-text">正在加载生成结果...</span>
      </div>
    </Transition>

    <header class="batch-results-header">
      <div class="batch-results-header-left">
        <h3 class="batch-results-title">生成结果</h3>
        <span class="batch-results-counter"
          >{{ filteredResultTasks.length }} 项</span
        >
      </div>

      <div class="batch-results-toolbar">
        <button
          class="batch-export-btn"
          type="button"
          :disabled="!exportableTaskIds.length || exporting"
          :title="
            exportableTaskIds.length
              ? `导出当前批量会话中的 ${exportableTaskIds.length} 个任务`
              : '暂无可导出的图片'
          "
          @click="emit('export-batch', exportableTaskIds)"
        >
          <Download :size="13" />
          <span>批量导出</span>
          <span v-if="exportableTaskIds.length"
            >({{ exportableTaskIds.length }})</span
          >
        </button>
        <div class="batch-search-box">
          <Search :size="13" class="search-icon" />
          <input
            v-model="search"
            type="text"
            class="batch-search-input"
            placeholder="搜索图片名称或提示词..."
          />
          <button
            v-if="search"
            type="button"
            class="search-clear-btn"
            title="清除搜索"
            @click="search = ''"
          >
            <X :size="12" />
          </button>
        </div>

        <div class="batch-view-toggle">
          <button
            type="button"
            class="view-toggle-btn"
            :class="{ 'is-active': viewMode === 'grid' }"
            title="网格视图"
            @click="viewMode = 'grid'"
          >
            <Grid :size="14" />
          </button>
          <button
            type="button"
            class="view-toggle-btn"
            :class="{ 'is-active': viewMode === 'list' }"
            title="列表视图"
            @click="viewMode = 'list'"
          >
            <List :size="14" />
          </button>
        </div>

        <div class="custom-select-wrapper is-mini status-filter-select">
          <select v-model="status" class="custom-select mini-select">
            <option value="completed">已完成</option>
            <option value="">全部状态</option>
            <option value="processing">进行中</option>
            <option value="queued">排队中</option>
            <option value="failed">失败</option>
          </select>
          <ChevronDown :size="12" class="custom-select-arrow" />
        </div>
      </div>
    </header>

    <div ref="resultScroll" class="batch-results-body">
      <div
        v-if="paginatedResultTasks.length && viewMode === 'grid'"
        class="batch-card-grid"
      >
        <article
          v-for="task in paginatedResultTasks"
          :key="task.id"
          class="batch-result-card"
        >
          <div
            class="result-card-media"
            @click="
              task.imageCount ? emit('show-detail', task, 'images') : null
            "
          >
            <el-image
              v-if="task.thumbnail && task.imageCount"
              :src="task.thumbnail"
              fit="cover"
              lazy
              class="result-card-img"
            />
            <div
              v-else-if="['queued', 'processing'].includes(task.status)"
              class="result-card-loading"
            >
              <LoaderCircle :size="24" class="spinning" />
              <span>正在绘制...</span>
            </div>
            <div v-else class="result-card-empty">
              <ImageOff :size="24" />
              <span>无图片</span>
            </div>

            <div
              class="result-card-badge"
              :class="`is-${batchStatusTone(task.status)}`"
            >
              <span class="badge-dot" />
              <span>{{ statusLabels[task.status] || task.status }}</span>
            </div>
          </div>

          <div class="result-card-content">
            <h4
              class="result-card-title"
              :title="task.request?.batchName || '未命名任务'"
            >
              {{ task.request?.batchName || "未命名任务" }}
            </h4>
            <p class="result-card-prompt" :title="task.request?.prompt">
              {{ task.request?.prompt }}
            </p>

            <div class="result-card-actions">
              <button
                class="card-act-btn"
                type="button"
                title="查看大图"
                :disabled="!task.imageCount"
                @click="emit('show-detail', task, 'images')"
              >
                <ZoomIn :size="13" />
              </button>
              <button
                class="card-act-btn"
                type="button"
                title="下载图片"
                :disabled="!task.imageCount"
                @click="emit('download-task', task)"
              >
                <Download :size="13" />
              </button>
              <button
                class="card-act-btn"
                type="button"
                title="复制提示词"
                @click="emit('copy-prompt', task.request?.prompt)"
              >
                <Copy :size="13" />
              </button>
              <button
                class="card-act-btn"
                type="button"
                title="查看参数"
                @click="emit('show-detail', task, 'parameters')"
              >
                <SlidersHorizontal :size="13" />
              </button>
            </div>
          </div>
        </article>
      </div>

      <div
        v-else-if="paginatedResultTasks.length && viewMode === 'list'"
        class="batch-results-list"
      >
        <article
          v-for="task in paginatedResultTasks"
          :key="task.id"
          class="batch-result-item"
          :class="task.status"
        >
          <div
            class="batch-result-thumb"
            @click="
              task.imageCount ? emit('show-detail', task, 'images') : null
            "
          >
            <el-image
              v-if="task.thumbnail && task.imageCount"
              :src="task.thumbnail"
              fit="cover"
              lazy
            />
            <LoaderCircle
              v-else-if="['queued', 'processing'].includes(task.status)"
              :size="22"
              class="batch-result-spinner spinning"
            />
            <ImageOff v-else :size="20" />
          </div>

          <div class="batch-result-main">
            <div class="batch-result-name-row">
              <span class="batch-result-name">{{
                task.request?.batchName || "未命名任务"
              }}</span>
              <span
                class="queue-status-pill"
                :class="`is-${batchStatusTone(task.status)}`"
              >
                <span class="pill-dot" />
                <span>{{ statusLabels[task.status] || task.status }}</span>
              </span>
            </div>
            <span class="batch-result-prompt">{{ task.request?.prompt }}</span>
          </div>

          <div class="batch-result-actions">
            <button
              v-if="task.imageCount"
              class="row-action-btn"
              type="button"
              title="下载图片"
              @click="emit('download-task', task)"
            >
              <Download :size="13" />
            </button>
            <button
              class="row-action-btn"
              type="button"
              title="复制提示词"
              @click="emit('copy-prompt', task.request?.prompt)"
            >
              <Copy :size="13" />
            </button>
            <button
              class="row-action-btn"
              type="button"
              title="查看详情"
              @click="emit('show-detail', task, 'parameters')"
            >
              <MoreHorizontal :size="13" />
            </button>
          </div>
        </article>
      </div>

      <div v-else class="batch-empty-results">
        <ImageIcon :size="40" class="empty-icon" />
        <h4>暂无生成结果</h4>
        <p>
          {{ search ? "未找到符合搜索条件的结果" : "等待批量图片任务生成完成" }}
        </p>
      </div>
    </div>

    <footer class="batch-results-footer">
      <div class="footer-left">
        <div class="custom-select-wrapper is-mini">
          <select v-model.number="pageSize" class="custom-select mini-select">
            <option :value="12">每页 12 条</option>
            <option :value="20">每页 20 条</option>
            <option :value="30">每页 30 条</option>
            <option :value="60">每页 60 条</option>
          </select>
          <ChevronDown :size="12" class="custom-select-arrow" />
        </div>
      </div>
      <div class="footer-right">
        <div class="batch-pagination">
          <button
            class="page-nav-btn"
            type="button"
            :disabled="page <= 1"
            title="上一页"
            @click="page--"
          >
            <ChevronLeft :size="13" />
          </button>
          <template
            v-for="(item, index) in getVisiblePages(page, totalPages)"
            :key="`${item}-${index}`"
          >
            <span v-if="item === '...'" class="page-ellipsis">...</span>
            <button
              v-else
              class="page-num-btn"
              :class="{ 'is-active': item === page }"
              type="button"
              @click="page = item"
            >
              {{ item }}
            </button>
          </template>
          <button
            class="page-nav-btn"
            type="button"
            :disabled="page >= totalPages"
            title="下一页"
            @click="page++"
          >
            <ChevronRight :size="13" />
          </button>
        </div>
      </div>
    </footer>
  </section>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  Grid,
  Image as ImageIcon,
  ImageOff,
  List,
  LoaderCircle,
  MoreHorizontal,
  Search,
  SlidersHorizontal,
  X,
  ZoomIn
} from "lucide-vue-next"
import { ElImage } from "element-plus"
import "element-plus/es/components/image/style/css"

const props = defineProps({
  tasks: { type: Array, default: () => [] },
  roundState: { type: Object, default: () => ({}) },
  loading: Boolean,
  exporting: Boolean,
  statusLabels: { type: Object, default: () => ({}) }
})

const search = defineModel("search", { type: String, default: "" })
const status = defineModel("status", { type: String, default: "completed" })
const viewMode = defineModel("viewMode", { type: String, default: "grid" })
const page = defineModel("page", { type: Number, default: 1 })
const pageSize = defineModel("pageSize", { type: Number, default: 20 })
const emit = defineEmits([
  "show-detail",
  "download-task",
  "copy-prompt",
  "export-batch"
])
const resultScroll = ref(null)

const exportableTaskIds = computed(() => {
  const ids = props.tasks
    .filter((task) => {
      const roundId = task.request?.roundId || task.id
      return (
        !props.roundState[roundId]?.replaced?.includes(task.id) &&
        ["completed", "partial"].includes(task.status) &&
        Number(task.imageCount) > 0
      )
    })
    .map((task) => task.id)
  return [...new Set(ids)]
})

// 重试记录继续保留在任务队列中，结果面板只展示当前有效记录。
const filteredResultTasks = computed(() => {
  let list = props.tasks.filter((task) => {
    const roundId = task.request?.roundId || task.id
    return !props.roundState[roundId]?.replaced?.includes(task.id)
  })

  if (status.value === "completed") {
    list = list.filter(
      (task) =>
        ["completed", "partial"].includes(task.status) && task.imageCount > 0
    )
  } else if (status.value) {
    list = list.filter((task) => task.status === status.value)
  }

  const query = search.value.trim().toLowerCase()
  if (query) {
    list = list.filter((task) => {
      const name = task.request?.batchName?.toLowerCase() || ""
      const prompt = task.request?.prompt?.toLowerCase() || ""
      return name.includes(query) || prompt.includes(query)
    })
  }
  return list
})

const totalPages = computed(() =>
  Math.max(
    1,
    Math.ceil(filteredResultTasks.value.length / Math.max(1, pageSize.value))
  )
)
const paginatedResultTasks = computed(() => {
  const start = (page.value - 1) * pageSize.value
  return filteredResultTasks.value.slice(start, start + pageSize.value)
})

function scrollToTop() {
  nextTick(() => resultScroll.value?.scrollTo({ top: 0 }))
}

// 筛选或分页变化后回到结果列表顶部，避免切页停留在旧滚动位置。
watch([search, status], () => {
  page.value = 1
  scrollToTop()
})
watch([page, pageSize], scrollToTop)
watch(totalPages, (value) => {
  if (page.value > value) page.value = value
})

function batchStatusTone(value) {
  if (["completed", "partial"].includes(value)) return "completed"
  if (value === "processing") return "processing"
  if (value === "queued") return "queued"
  if (["failed", "interrupted"].includes(value)) return "failed"
  return "queued"
}

function getVisiblePages(current, total) {
  if (total <= 7) return Array.from({ length: total }, (_, index) => index + 1)
  const pages = [1]
  if (current > 3) pages.push("...")
  for (
    let index = Math.max(2, current - 1);
    index <= Math.min(total - 1, current + 1);
    index++
  ) {
    pages.push(index)
  }
  if (current < total - 2) pages.push("...")
  pages.push(total)
  return pages
}
</script>

<style scoped lang="less">
.batch-results-panel {
  position: relative;
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;
  padding: 16px;
  border: 1px solid var(--color-line);
  border-radius: 12px;
  background: var(--color-panel);
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.03);
}

.workbench-loading-overlay {
  position: absolute;
  inset: 0;
  z-index: 60;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  text-align: center;
  background: color-mix(in srgb, var(--color-panel, #fff) 82%, transparent);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  color: var(--color-primary, #2563eb);
  border-radius: inherit;
  pointer-events: all;
  user-select: none;

  .overlay-text {
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 500;
  }
}

.workbench-fade-enter-active,
.workbench-fade-leave-active {
  transition: opacity 0.2s ease;
}

.workbench-fade-enter-from,
.workbench-fade-leave-to {
  opacity: 0;
}

.batch-results-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  flex-shrink: 0;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--color-line);
}

.batch-results-header-left {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.batch-results-title {
  margin: 0;
  color: var(--color-text);
  font-size: 15px;
  font-weight: 700;
}

.batch-results-counter {
  color: var(--color-text-muted);
  font-size: 12.5px;
}

.batch-results-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.batch-export-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  height: 28px;
  padding: 0 9px;
  border: 1px solid #2563eb;
  border-radius: 6px;
  background: #2563eb;
  color: #fff;
  cursor: pointer;
  font-size: 12px;
  transition:
    background-color 0.14s ease,
    border-color 0.14s ease;

  &:hover:not(:disabled) {
    border-color: #1d4ed8;
    background: #1d4ed8;
  }

  &:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
}

.batch-search-box {
  position: relative;
  display: flex;
  align-items: center;
  width: 180px;
  height: 28px;
  padding: 0 6px 0 26px;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel);
  transition: border-color 0.16s ease;

  &:focus-within {
    border-color: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.12);
  }

  .search-icon {
    position: absolute;
    left: 8px;
    color: var(--color-text-soft);
    pointer-events: none;
  }

  .batch-search-input {
    width: 100%;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--color-text);
    font-size: 12px;

    &::placeholder {
      color: var(--color-text-soft);
    }
  }

  .search-clear-btn {
    display: grid;
    width: 16px;
    height: 16px;
    padding: 0;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;

    &:hover {
      background: var(--color-panel-soft);
      color: var(--color-text);
    }
  }
}

.batch-view-toggle {
  display: inline-flex;
  align-items: center;
  padding: 1px;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel-soft);

  .view-toggle-btn {
    display: grid;
    width: 26px;
    height: 24px;
    place-items: center;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;

    &:hover {
      color: var(--color-text);
    }

    &.is-active {
      background: var(--color-panel);
      color: #2563eb;
      box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
    }
  }
}

.batch-results-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  scrollbar-gutter: stable;
}

.batch-card-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  padding-bottom: 8px;

  @media (max-width: 900px) {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  @media (max-width: 600px) {
    grid-template-columns: 1fr;
  }
}

.batch-result-card {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  transition: all 0.16s ease;

  &:hover {
    border-color: var(--color-line-strong);
    box-shadow: 0 4px 12px rgba(15, 23, 42, 0.06);
  }

  .result-card-media {
    position: relative;
    aspect-ratio: 16 / 10;
    overflow: hidden;
    background: var(--color-panel-soft);
    cursor: pointer;

    .result-card-img {
      width: 100%;
      height: 100%;
      transition: transform 0.2s ease;

      &:hover {
        transform: scale(1.03);
      }
    }

    .result-card-loading,
    .result-card-empty {
      display: flex;
      height: 100%;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 6px;
      color: var(--color-text-soft);
      font-size: 11px;
    }

    .result-card-badge {
      position: absolute;
      top: 6px;
      right: 6px;
      display: inline-flex;
      align-items: center;
      gap: 4px;
      height: 20px;
      padding: 0 7px;
      border-radius: 9999px;
      font-size: 10.5px;
      font-weight: 600;
      backdrop-filter: blur(8px);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);

      .badge-dot {
        width: 5px;
        height: 5px;
        border-radius: 50%;
      }

      &.is-completed {
        border: 1px solid rgba(220, 252, 231, 0.9);
        background: rgba(240, 253, 244, 0.9);
        color: #16a34a;
        .badge-dot {
          background: #22c55e;
        }
      }
      &.is-processing {
        border: 1px solid rgba(219, 234, 254, 0.9);
        background: rgba(239, 246, 255, 0.9);
        color: #2563eb;
        .badge-dot {
          background: #3b82f6;
        }
      }
      &.is-queued {
        border: 1px solid rgba(254, 240, 138, 0.9);
        background: rgba(254, 249, 195, 0.9);
        color: #854d0e;
        .badge-dot {
          background: #eab308;
        }
      }
      &.is-failed {
        border: 1px solid rgba(254, 226, 226, 0.9);
        background: rgba(254, 242, 242, 0.9);
        color: #dc2626;
        .badge-dot {
          background: #ef4444;
        }
      }
    }
  }

  .result-card-content {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    padding: 10px;

    .result-card-title {
      margin: 0;
      overflow: hidden;
      color: var(--color-text);
      font-size: 12.5px;
      font-weight: 600;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .result-card-prompt {
      display: -webkit-box;
      min-height: 30px;
      margin: 0;
      overflow: hidden;
      color: var(--color-text-muted);
      font-size: 11px;
      line-height: 1.4;
      -webkit-box-orient: vertical;
      -webkit-line-clamp: 2;
      line-clamp: 2;
    }

    .result-card-actions {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 6px;
      margin-top: auto;
      padding-top: 8px;
      border-top: 1px solid var(--color-line);
    }
  }
}

.card-act-btn,
.row-action-btn {
  display: grid;
  place-items: center;
  border: 1px solid var(--color-line);
  background: var(--color-panel);
  color: var(--color-text-muted);
  cursor: pointer;
  transition: all 0.14s ease;

  &:hover:not(:disabled) {
    border-color: #2563eb;
    background: var(--color-primary-soft);
    color: #2563eb;
  }

  &:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
}

.card-act-btn {
  width: 24px;
  height: 24px;
  border-radius: 5px;
}

.batch-results-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.batch-result-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  transition: background-color 0.14s ease;

  &:hover {
    background: var(--color-panel-soft);
  }
}

.batch-result-thumb {
  display: grid;
  width: 48px;
  height: 48px;
  flex: 0 0 48px;
  place-items: center;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel-soft);
  cursor: pointer;

  :deep(.el-image) {
    width: 100%;
    height: 100%;
  }
}

.batch-result-main {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 3px;
}

.batch-result-name-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.batch-result-name,
.batch-result-prompt {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.batch-result-name {
  color: var(--color-text);
  font-size: 13px;
  font-weight: 600;
}

.batch-result-prompt {
  color: var(--color-text-muted);
  font-size: 11.5px;
}

.batch-result-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.row-action-btn {
  width: 26px;
  height: 26px;
  border-radius: 5px;
}

.queue-status-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  height: 22px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: 9999px;
  font-size: 11px;
  font-weight: 500;
  white-space: nowrap;

  .pill-dot {
    width: 6px;
    height: 6px;
    flex-shrink: 0;
    border-radius: 50%;
  }

  &.is-completed {
    border-color: #bbf7d0;
    background: #f0fdf4;
    color: #15803d;
    .pill-dot {
      background: #22c55e;
    }
  }
  &.is-processing {
    border-color: #bfdbfe;
    background: #eff6ff;
    color: #2563eb;
    .pill-dot {
      background: #3b82f6;
    }
  }
  &.is-queued {
    border-color: #fef08a;
    background: #fef9c3;
    color: #854d0e;
    .pill-dot {
      background: #eab308;
    }
  }
  &.is-failed {
    border-color: #fecaca;
    background: #fef2f2;
    color: #dc2626;
    .pill-dot {
      background: #ef4444;
    }
  }
}

.batch-empty-results {
  display: flex;
  min-height: 100%;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 60px 20px;
  color: var(--color-text-muted);
  text-align: center;

  .empty-icon {
    margin-bottom: 12px;
    color: var(--color-text-soft);
  }

  h4 {
    margin: 0 0 4px;
    color: var(--color-text);
    font-size: 14px;
  }

  p {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 12px;
  }
}

.batch-results-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-shrink: 0;
  padding-top: 10px;
  border-top: 1px solid var(--color-line);
}

.footer-left,
.footer-right {
  display: flex;
  align-items: center;
}

.batch-pagination {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.page-nav-btn,
.page-num-btn {
  display: grid;
  height: 26px;
  place-items: center;
  border: 1px solid var(--color-line);
  border-radius: 5px;
  background: var(--color-panel);
  color: var(--color-text-muted);
  cursor: pointer;
  transition: all 0.14s ease;

  &:hover:not(:disabled) {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    color: var(--color-text);
  }

  &:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
}

.page-nav-btn {
  width: 26px;
}

.page-num-btn {
  min-width: 26px;
  padding: 0 6px;
  color: var(--color-text);
  font-family: "Bahnschrift", monospace;
  font-size: 11.5px;

  &:hover:not(:disabled) {
    border-color: #2563eb;
    color: #2563eb;
  }

  &.is-active {
    border-color: #2563eb;
    background: #2563eb;
    color: #fff;
    font-weight: 700;
  }
}

.page-ellipsis {
  display: grid;
  width: 20px;
  height: 26px;
  place-items: center;
  color: var(--color-text-soft);
  font-size: 12px;
}

.custom-select-wrapper.is-mini {
  position: relative;
  display: inline-flex;
  align-items: center;

  .mini-select {
    height: 26px;
    padding: 0 22px 0 8px;
    appearance: none;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    outline: none;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    font-size: 11.5px;

    &:focus {
      border-color: #2563eb;
    }
  }

  .custom-select-arrow {
    position: absolute;
    right: 6px;
    color: var(--color-text-muted);
    pointer-events: none;
  }
}

.spinning {
  animation: image-workbench-spin 1.5s linear infinite;
}

:global(:root[data-theme="dark"] .batch-results-panel),
:global(.tools-view--dark .batch-results-panel) {
  background: var(--color-panel);
  border-color: var(--color-line);
}

:global(:root[data-theme="dark"] .batch-results-panel .batch-result-card),
:global(.tools-view--dark .batch-results-panel .batch-result-card),
:global(:root[data-theme="dark"] .batch-results-panel .batch-result-item),
:global(.tools-view--dark .batch-results-panel .batch-result-item) {
  background: var(--color-panel);
  border-color: var(--color-line);
}

:global(:root[data-theme="dark"] .batch-results-panel .batch-result-card:hover),
:global(.tools-view--dark .batch-results-panel .batch-result-card:hover),
:global(:root[data-theme="dark"] .batch-results-panel .batch-result-item:hover),
:global(.tools-view--dark .batch-results-panel .batch-result-item:hover) {
  border-color: var(--color-line-strong);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.3);
}

:global(:root[data-theme="dark"] .batch-results-panel .result-card-media),
:global(.tools-view--dark .batch-results-panel .result-card-media),
:global(:root[data-theme="dark"] .batch-results-panel .batch-result-thumb),
:global(.tools-view--dark .batch-results-panel .batch-result-thumb) {
  background: var(--color-panel-soft);
}

:global(
  :root[data-theme="dark"] .batch-results-panel .result-card-badge.is-completed
),
:global(
  .tools-view--dark .batch-results-panel .result-card-badge.is-completed
) {
  background: rgba(20, 83, 45, 0.85);
  border-color: rgba(34, 197, 94, 0.4);
  color: #4ade80;
}

:global(
  :root[data-theme="dark"] .batch-results-panel .result-card-badge.is-processing
),
:global(
  .tools-view--dark .batch-results-panel .result-card-badge.is-processing
) {
  background: rgba(30, 58, 138, 0.85);
  border-color: rgba(59, 130, 246, 0.4);
  color: #60a5fa;
}

:global(
  :root[data-theme="dark"] .batch-results-panel .result-card-badge.is-queued
),
:global(.tools-view--dark .batch-results-panel .result-card-badge.is-queued) {
  background: rgba(113, 63, 18, 0.85);
  border-color: rgba(234, 179, 8, 0.4);
  color: #facc15;
}

:global(
  :root[data-theme="dark"] .batch-results-panel .result-card-badge.is-failed
),
:global(.tools-view--dark .batch-results-panel .result-card-badge.is-failed) {
  background: rgba(127, 29, 29, 0.85);
  border-color: rgba(239, 68, 68, 0.4);
  color: #f87171;
}

:global(
  :root[data-theme="dark"] .batch-results-panel .queue-status-pill.is-completed
),
:global(
  .tools-view--dark .batch-results-panel .queue-status-pill.is-completed
) {
  background: rgba(20, 83, 45, 0.35);
  border-color: rgba(34, 197, 94, 0.4);
  color: #4ade80;
}

:global(
  :root[data-theme="dark"] .batch-results-panel .queue-status-pill.is-processing
),
:global(
  .tools-view--dark .batch-results-panel .queue-status-pill.is-processing
) {
  background: rgba(30, 58, 138, 0.35);
  border-color: rgba(59, 130, 246, 0.4);
  color: #60a5fa;
}

:global(
  :root[data-theme="dark"] .batch-results-panel .queue-status-pill.is-queued
),
:global(.tools-view--dark .batch-results-panel .queue-status-pill.is-queued) {
  background: rgba(113, 63, 18, 0.35);
  border-color: rgba(234, 179, 8, 0.4);
  color: #facc15;
}

:global(
  :root[data-theme="dark"] .batch-results-panel .queue-status-pill.is-failed
),
:global(.tools-view--dark .batch-results-panel .queue-status-pill.is-failed) {
  background: rgba(127, 29, 29, 0.35);
  border-color: rgba(239, 68, 68, 0.4);
  color: #f87171;
}

@keyframes image-workbench-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

<template>
  <section class="batch-queue-panel">
    <Transition name="workbench-fade">
      <div v-if="loading" class="workbench-loading-overlay">
        <LoaderCircle :size="32" class="spinning" />
        <span class="overlay-text">正在加载任务队列...</span>
      </div>
    </Transition>

    <header class="batch-queue-header">
      <div class="batch-queue-title-row">
        <div class="batch-queue-title-group">
          <h3 class="batch-queue-title">任务队列</h3>
          <span class="batch-queue-counter">
            {{
              statusFilter !== "all"
                ? `${filteredTasks.length} 项`
                : `${completedCount} / ${totalCount}`
            }}
          </span>
        </div>
        <div class="batch-queue-status-pills">
          <button
            class="queue-status-pill is-filter-pill is-all"
            :class="{ 'is-active': statusFilter === 'all' }"
            type="button"
            title="查看全部状态任务"
            @click="statusFilter = 'all'"
          >
            <span>全部 {{ allTasks.length }}</span>
          </button>
          <button
            v-for="filter in statusFilters"
            :key="filter.value"
            class="queue-status-pill is-filter-pill"
            :class="[
              `is-${filter.value}`,
              { 'is-active': statusFilter === filter.value }
            ]"
            type="button"
            :title="`筛选${filter.label}任务`"
            @click="toggleStatusFilter(filter.value)"
          >
            <span class="pill-dot" />
            <span>{{ filter.label }} {{ filter.count }}</span>
          </button>
        </div>
      </div>

      <div class="batch-queue-toolbar">
        <div class="batch-queue-toolbar-left">
          <button
            class="batch-btn is-primary is-sm"
            type="button"
            :disabled="
              (paused ? !resumableTasks.length : !activeTasks.length) ||
              stopping
            "
            :title="paused ? '继续批量任务' : '暂停批量任务'"
            @click="$emit('toggle-pause')"
          >
            <Play v-if="paused" :size="13" fill="currentColor" />
            <Pause v-else :size="13" />
            <span>{{ paused ? "开始任务" : "暂停任务" }}</span>
          </button>
          <button
            class="batch-btn is-success-outline is-sm"
            type="button"
            :disabled="!resumableSelected.length || stopping"
            :title="`恢复或重试选中的已暂停或失败任务${resumableSelected.length ? ` (${resumableSelected.length})` : ''}`"
            @click="$emit('resume-selected', [...resumableSelected])"
          >
            <RotateCcw :size="13" />
            <span
              >恢复选中{{
                resumableSelected.length ? ` (${resumableSelected.length})` : ""
              }}</span
            >
          </button>
          <button
            class="batch-btn is-outline is-sm"
            type="button"
            :disabled="!cancellableSelected.length || stopping"
            title="取消选中的排队或生成任务"
            @click="$emit('cancel-selected', [...cancellableSelected])"
          >
            <X :size="13" />
            <span
              >取消选中{{
                cancellableSelected.length
                  ? ` (${cancellableSelected.length})`
                  : ""
              }}</span
            >
          </button>
          <button
            class="batch-btn is-danger-outline is-sm"
            type="button"
            :disabled="!selectedIds.length || deleting"
            title="删除选中的批量任务"
            @click="$emit('delete-selected', [...selectedIds])"
          >
            <Trash2 :size="13" />
            <span
              >删除选中{{
                selectedIds.length ? ` (${selectedIds.length})` : ""
              }}</span
            >
          </button>
          <div
            class="batch-concurrency-control"
            title="批量任务同时执行数量 (1–10)"
          >
            <div class="batch-concurrency-label-group">
              <SlidersHorizontal :size="12" class="concurrency-icon" />
              <span class="batch-concurrency-label">并发</span>
            </div>
            <div class="batch-concurrency-stepper">
              <button
                class="batch-concurrency-btn is-minus"
                type="button"
                title="减少并发数量 (最小 1)"
                :disabled="concurrency <= 1 || stopping"
                @click="$emit('set-concurrency', concurrency - 1)"
              >
                <Minus :size="12" />
              </button>
              <span
                class="batch-concurrency-value"
                :title="`当前并发任务数: ${concurrency}`"
              >
                {{ concurrency }}
              </span>
              <button
                class="batch-concurrency-btn is-plus"
                type="button"
                title="增加并发数量 (最大 10)"
                :disabled="concurrency >= 10 || stopping"
                @click="$emit('set-concurrency', concurrency + 1)"
              >
                <Plus :size="12" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </header>

    <div class="batch-queue-table-card">
      <div class="batch-table-head">
        <div class="col-check">
          <input
            v-model="allSelected"
            type="checkbox"
            :indeterminate="isIndeterminate"
            :disabled="!paginatedTasks.length"
          />
        </div>
        <div class="col-info">名称 / 提示词</div>
        <div class="col-status">
          <span>状态</span>
          <span v-if="statusFilter !== 'all'" class="col-status-filter-tag"
            >({{ statusFilterLabel }})</span
          >
        </div>
        <div class="col-actions">操作</div>
      </div>

      <div
        v-if="paginatedTasks.length"
        ref="scrollElement"
        class="batch-table-body"
      >
        <div
          v-for="task in paginatedTasks"
          :key="task.id"
          class="batch-table-row"
          :class="{ 'is-selected': selectedIds.includes(task.id) }"
        >
          <div class="col-check">
            <input v-model="selectedIds" type="checkbox" :value="task.id" />
          </div>
          <div class="col-info">
            <span
              class="task-row-name"
              :title="task.request?.batchName || '未命名任务'"
            >
              {{ task.request?.batchName || "未命名任务" }}
            </span>
            <span class="task-row-prompt" :title="task.request?.prompt">{{
              task.request?.prompt
            }}</span>
          </div>
          <div class="col-status">
            <template v-if="task.status === 'processing'">
              <div class="task-running-box">
                <div class="task-running-label">
                  <span class="running-text">正在生成</span>
                </div>
                <div class="task-running-track">
                  <div class="task-running-bar" />
                </div>
              </div>
            </template>
            <template v-else>
              <div
                class="queue-status-pill"
                :class="`is-${statusTone(task.status)}`"
              >
                <span class="pill-dot" />
                <span>{{ statusLabels[task.status] || task.status }}</span>
              </div>
              <span
                v-if="task.error?.message"
                class="task-error-tip"
                :title="task.error.message"
              >
                {{ task.error.message }}
              </span>
            </template>
          </div>
          <div class="col-actions">
            <button
              v-if="['queued', 'processing'].includes(task.status)"
              class="row-action-btn"
              type="button"
              title="取消任务"
              :disabled="stopping"
              @click="$emit('cancel-task', task.id)"
            >
              <Pause :size="13" />
            </button>
            <button
              v-else-if="canResumeTask(task)"
              class="row-action-btn"
              type="button"
              title="恢复任务"
              :disabled="resumingIds.includes(task.id)"
              @click="$emit('resume-task', task)"
            >
              <RotateCcw :size="13" />
            </button>
            <button
              class="row-action-btn"
              type="button"
              title="查看详情"
              @click="$emit('show-detail', task, 'parameters')"
            >
              <MoreHorizontal :size="13" />
            </button>
          </div>
        </div>
      </div>

      <div v-else class="batch-empty-table">
        <ImageIcon :size="32" class="empty-icon" />
        <p>
          {{
            statusFilter !== "all"
              ? `暂无${statusFilterLabel}任务`
              : "暂无队列任务"
          }}
        </p>
        <button
          v-if="statusFilter !== 'all'"
          class="batch-btn is-outline is-sm"
          type="button"
          @click="statusFilter = 'all'"
        >
          清除筛选
        </button>
      </div>

      <footer class="batch-table-footer">
        <div class="footer-left">
          <label class="footer-check-label">
            <input
              v-model="allSelected"
              type="checkbox"
              :indeterminate="isIndeterminate"
              :disabled="!paginatedTasks.length"
            />
            <span>全选</span>
          </label>
          <span class="footer-selected-count">
            已选 {{ selectedIds.length }} 项
            <template v-if="statusFilter !== 'all'">
              · 当前筛选共 {{ filteredTasks.length }} 项</template
            >
          </span>
        </div>
        <div class="footer-right">
          <div class="custom-select-wrapper is-mini">
            <select v-model.number="pageSize" class="custom-select mini-select">
              <option :value="10">每页 10 条</option>
              <option :value="20">每页 20 条</option>
              <option :value="50">每页 50 条</option>
              <option :value="100">每页 100 条</option>
            </select>
            <ChevronDown :size="12" class="custom-select-arrow" />
          </div>
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
              v-for="(item, index) in visiblePages"
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
    </div>
  </section>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Image as ImageIcon,
  LoaderCircle,
  Minus,
  MoreHorizontal,
  Pause,
  Play,
  Plus,
  RotateCcw,
  SlidersHorizontal,
  Trash2,
  X
} from "lucide-vue-next"

const props = defineProps({
  tasks: { type: Array, default: () => [] },
  expectedTotal: { type: Number, default: 0 },
  loading: Boolean,
  paused: Boolean,
  concurrency: { type: Number, default: 2 },
  stopping: Boolean,
  deleting: Boolean,
  resumingIds: { type: Array, default: () => [] },
  statusLabels: { type: Object, default: () => ({}) }
})

const selectedIds = defineModel("selectedIds", {
  type: Array,
  default: () => []
})
const statusFilter = defineModel("statusFilter", {
  type: String,
  default: "all"
})
const page = defineModel("page", { type: Number, default: 1 })
const pageSize = defineModel("pageSize", { type: Number, default: 20 })
defineEmits([
  "toggle-pause",
  "resume-selected",
  "cancel-selected",
  "delete-selected",
  "set-concurrency",
  "cancel-task",
  "resume-task",
  "show-detail"
])

const scrollElement = ref(null)
const allTasks = computed(() => props.tasks)
const completedCount = computed(
  () =>
    allTasks.value.filter((task) =>
      ["completed", "partial"].includes(task.status)
    ).length
)
const processingCount = computed(
  () => allTasks.value.filter((task) => task.status === "processing").length
)
const queuedCount = computed(
  () => allTasks.value.filter((task) => task.status === "queued").length
)
const failedCount = computed(
  () =>
    allTasks.value.filter((task) =>
      ["failed", "interrupted"].includes(task.status)
    ).length
)
const totalCount = computed(() =>
  Math.max(allTasks.value.length, props.expectedTotal)
)
const activeTasks = computed(() =>
  allTasks.value.filter((task) =>
    ["queued", "processing"].includes(task.status)
  )
)
const resumableTasks = computed(() =>
  allTasks.value.filter((task) => task.status === "interrupted")
)
const statusFilters = computed(() => [
  { value: "processing", label: "进行中", count: processingCount.value },
  { value: "queued", label: "排队中", count: queuedCount.value },
  { value: "completed", label: "已完成", count: completedCount.value },
  { value: "failed", label: "失败", count: failedCount.value }
])
const filteredTasks = computed(() => {
  if (statusFilter.value === "processing")
    return allTasks.value.filter((task) => task.status === "processing")
  if (statusFilter.value === "queued")
    return allTasks.value.filter((task) => task.status === "queued")
  if (statusFilter.value === "completed")
    return allTasks.value.filter((task) =>
      ["completed", "partial"].includes(task.status)
    )
  if (statusFilter.value === "failed")
    return allTasks.value.filter((task) =>
      ["failed", "interrupted"].includes(task.status)
    )
  return allTasks.value
})
const statusFilterLabel = computed(
  () =>
    ({
      all: "全部",
      processing: "进行中",
      queued: "排队中",
      completed: "已完成",
      failed: "失败/中断"
    })[statusFilter.value] || "全部"
)
const totalPages = computed(() =>
  Math.max(
    1,
    Math.ceil(filteredTasks.value.length / Math.max(1, pageSize.value))
  )
)
const paginatedTasks = computed(() => {
  const start = (page.value - 1) * pageSize.value
  return filteredTasks.value.slice(start, start + pageSize.value)
})
const cancellableSelected = computed(() =>
  selectedIds.value.filter((id) =>
    allTasks.value.some(
      (task) => task.id === id && ["queued", "processing"].includes(task.status)
    )
  )
)
const resumableSelected = computed(() =>
  selectedIds.value.filter((id) =>
    allTasks.value.some(
      (task) =>
        task.id === id && ["failed", "interrupted"].includes(task.status)
    )
  )
)
const allSelected = computed({
  get: () =>
    paginatedTasks.value.length > 0 &&
    paginatedTasks.value.every((task) => selectedIds.value.includes(task.id)),
  set: (value) => {
    const pageIds = paginatedTasks.value.map((task) => task.id)
    selectedIds.value = value
      ? Array.from(new Set([...selectedIds.value, ...pageIds]))
      : selectedIds.value.filter((id) => !pageIds.includes(id))
  }
})
const isIndeterminate = computed(() => {
  const selectedOnPage = paginatedTasks.value.filter((task) =>
    selectedIds.value.includes(task.id)
  ).length
  return selectedOnPage > 0 && selectedOnPage < paginatedTasks.value.length
})
const visiblePages = computed(() => {
  if (totalPages.value <= 7)
    return Array.from({ length: totalPages.value }, (_, index) => index + 1)
  const pages = [1]
  if (page.value > 3) pages.push("...")
  for (
    let index = Math.max(2, page.value - 1);
    index <= Math.min(totalPages.value - 1, page.value + 1);
    index++
  )
    pages.push(index)
  if (page.value < totalPages.value - 2) pages.push("...")
  pages.push(totalPages.value)
  return pages
})

function canResumeTask(task) {
  return ["failed", "interrupted"].includes(task?.status)
}

function statusTone(status) {
  if (["completed", "partial"].includes(status)) return "completed"
  if (status === "processing") return "processing"
  if (status === "queued") return "queued"
  if (["failed", "interrupted"].includes(status)) return "failed"
  return "queued"
}

function toggleStatusFilter(value) {
  statusFilter.value = statusFilter.value === value ? "all" : value
}

function scrollToTop() {
  nextTick(() => scrollElement.value?.scrollTo({ top: 0 }))
}

watch(statusFilter, () => {
  page.value = 1
  scrollToTop()
})
watch([page, pageSize], scrollToTop)
watch(totalPages, (value) => {
  if (page.value > value) page.value = value
})
watch(
  () => props.tasks,
  (tasks) => {
    const ids = new Set(tasks.map((task) => task.id))
    const next = selectedIds.value.filter((id) => ids.has(id))
    if (next.length !== selectedIds.value.length) selectedIds.value = next
  },
  { deep: true }
)
</script>

<style scoped lang="less">
.batch-queue-panel {
  position: relative;
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 0 0 auto;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;
  padding: 16px;
  border: 1px solid var(--color-line);
  border-radius: 12px;
  background: var(--color-panel);
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.03);
}

.batch-queue-header {
  display: flex;
  flex-direction: column;
  gap: 10px;
  flex-shrink: 0;
}
.batch-queue-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  flex-wrap: wrap;
}
.batch-queue-title-group {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.batch-queue-title {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
  color: var(--color-text);
}
.batch-queue-counter {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text-muted);
  font-family: "Bahnschrift", monospace;
}
.batch-queue-status-pills {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
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
  &.is-filter-pill {
    cursor: pointer;
    user-select: none;
    outline: none;
    font-family: inherit;
    transition: all 0.16s ease;
    &:hover {
      transform: translateY(-1px);
    }
    &:active {
      transform: translateY(0);
    }
  }
  .pill-dot {
    width: 6px;
    height: 6px;
    flex-shrink: 0;
    border-radius: 50%;
  }
  &.is-all {
    background: var(--color-panel-soft);
    border-color: var(--color-line);
    color: var(--color-text-muted);
    &.is-active {
      background: #2563eb;
      border-color: #2563eb;
      color: #fff;
    }
  }
  &.is-processing {
    background: #eff6ff;
    border-color: #dbeafe;
    color: #2563eb;
    .pill-dot {
      background: #3b82f6;
      animation: batch-dot-pulse 1.4s infinite;
    }
    &.is-active {
      background: #2563eb;
      border-color: #2563eb;
      color: #fff;
      .pill-dot {
        background: #fff;
        animation: none;
      }
    }
  }
  &.is-queued {
    background: #fef9c3;
    border-color: #fef08a;
    color: #854d0e;
    .pill-dot {
      background: #eab308;
    }
    &.is-active {
      background: #eab308;
      border-color: #ca8a04;
      color: #fff;
      .pill-dot {
        background: #fff;
      }
    }
  }
  &.is-completed {
    background: #f0fdf4;
    border-color: #dcfce7;
    color: #16a34a;
    .pill-dot {
      background: #22c55e;
    }
    &.is-active {
      background: #16a34a;
      border-color: #15803d;
      color: #fff;
      .pill-dot {
        background: #fff;
      }
    }
  }
  &.is-failed {
    background: #fef2f2;
    border-color: #fee2e2;
    color: #dc2626;
    .pill-dot {
      background: #ef4444;
    }
    &.is-active {
      background: #dc2626;
      border-color: #b91c1c;
      color: #fff;
      .pill-dot {
        background: #fff;
      }
    }
  }
}

.batch-queue-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}
.batch-queue-toolbar-left {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.batch-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 28px;
  padding: 0 12px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: var(--color-panel);
  color: var(--color-text);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  user-select: none;
  transition: all 0.16s ease;
  &:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  &.is-sm {
    height: 26px;
    padding: 0 10px;
    font-size: 11.5px;
  }
  &.is-primary {
    background: #2563eb;
    border-color: #2563eb;
    color: #fff;
  }
  &.is-outline {
    border-color: var(--color-line);
  }
  &.is-success-outline {
    border-color: var(--color-line);
    color: #16a34a;
  }
  &.is-danger-outline {
    border-color: var(--color-danger-line, #fecaca);
    background: var(--color-danger-soft, #fff5f5);
    color: var(--color-danger, #ef4444);
  }
}
.batch-concurrency-control {
  display: inline-flex;
  align-items: center;
  height: 28px;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel);
  user-select: none;
}
.batch-concurrency-label-group {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 100%;
  padding: 0 7px 0 8px;
  border-right: 1px solid var(--color-line);
  background: var(--color-panel-soft);
  color: var(--color-text-muted);
  font-size: 11.5px;
}
.concurrency-icon {
  color: var(--color-text-soft);
}
.batch-concurrency-stepper {
  display: inline-flex;
  align-items: center;
  height: 100%;
}
.batch-concurrency-btn {
  display: grid;
  width: 24px;
  height: 100%;
  padding: 0;
  border: 0;
  place-items: center;
  background: transparent;
  color: var(--color-text-muted);
  cursor: pointer;
  &:hover:not(:disabled) {
    background: var(--color-panel-soft);
    color: #2563eb;
  }
  &:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
}
.batch-concurrency-value {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 24px;
  height: 100%;
  padding: 0 4px;
  border-right: 1px solid var(--color-line);
  border-left: 1px solid var(--color-line);
  background: var(--color-panel);
  color: var(--color-text);
  font-size: 12px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.batch-queue-table-card {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
}
.batch-table-head,
.batch-table-row {
  display: grid;
  grid-template-columns: 32px minmax(0, 1fr) 110px 80px;
  align-items: center;
}
.batch-table-head {
  flex-shrink: 0;
  padding: 8px 12px;
  border-bottom: 1px solid var(--color-line);
  background: var(--color-panel-soft);
  color: var(--color-text-soft);
  font-size: 11.5px;
  font-weight: 600;
}
.batch-table-head .col-check,
.batch-table-row .col-check {
  display: flex;
  align-items: center;
  justify-content: center;
}
.batch-table-head .col-status {
  display: flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}
.col-status-filter-tag {
  color: #2563eb;
  font-weight: 700;
}
.batch-table-head .col-actions {
  text-align: right;
}
.batch-table-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  scrollbar-gutter: stable;
}
.batch-table-row {
  padding: 9px 12px;
  border-bottom: 1px solid var(--color-line);
  transition: background-color 0.14s ease;
  &:hover {
    background: color-mix(in srgb, var(--color-primary-soft) 30%, transparent);
  }
  &.is-selected {
    background: color-mix(in srgb, var(--color-primary-soft) 45%, transparent);
  }
}
.batch-table-row .col-info {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
  padding-right: 8px;
}
.task-row-name,
.task-row-prompt {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.task-row-name {
  color: var(--color-text);
  font-size: 12.5px;
  font-weight: 600;
}
.task-row-prompt {
  color: var(--color-text-muted);
  font-size: 11px;
}
.batch-table-row .col-status {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.task-error-tip {
  max-width: 100px;
  overflow: hidden;
  color: var(--color-danger, #ef4444);
  font-size: 10.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.batch-table-row .col-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
}
.task-running-box {
  display: flex;
  width: 90px;
  flex-direction: column;
  gap: 3px;
}
.running-text {
  color: #2563eb;
  font-size: 11px;
  font-weight: 600;
}
.task-running-track {
  width: 100%;
  height: 4px;
  overflow: hidden;
  border-radius: 9999px;
  background: #dbeafe;
}
.task-running-bar {
  width: 40%;
  height: 100%;
  border-radius: inherit;
  background: #2563eb;
  animation: batch-bar-slide 1.4s ease-in-out infinite;
}
.row-action-btn {
  display: grid;
  width: 24px;
  height: 24px;
  place-items: center;
  border: 1px solid var(--color-line);
  border-radius: 5px;
  background: var(--color-panel);
  color: var(--color-text-muted);
  cursor: pointer;
  &:hover:not(:disabled) {
    border-color: var(--color-line-strong);
    color: var(--color-text);
    background: var(--color-panel-soft);
  }
  &:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
}
.batch-empty-table {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 40px 16px;
  color: var(--color-text-soft);
  .empty-icon {
    opacity: 0.5;
  }
  p {
    margin: 0;
    font-size: 12px;
  }
}
.batch-table-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
  margin-top: auto;
  padding: 8px 12px;
  border-top: 1px solid var(--color-line);
  background: var(--color-panel-soft);
}
.footer-left,
.footer-right {
  display: flex;
  align-items: center;
  gap: 10px;
}
.footer-check-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--color-text);
  font-size: 12px;
  cursor: pointer;
  user-select: none;
  input[type="checkbox"] {
    width: 14px;
    height: 14px;
    margin: 0;
    accent-color: #2563eb;
    cursor: pointer;
  }
}
.footer-selected-count {
  color: var(--color-text-muted);
  font-size: 11.5px;
}
.custom-select-wrapper.is-mini {
  position: relative;
  display: inline-flex;
  align-items: center;
}
.mini-select {
  height: 26px;
  padding: 0 22px 0 8px;
  border: 1px solid var(--color-line);
  border-radius: 5px;
  background: var(--color-panel);
  color: var(--color-text);
  font-size: 11.5px;
  outline: none;
  cursor: pointer;
  appearance: none;
}
.custom-select-arrow {
  position: absolute;
  right: 6px;
  color: var(--color-text-muted);
  pointer-events: none;
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
}
.page-nav-btn {
  width: 26px;
  &:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
}
.page-num-btn {
  min-width: 26px;
  padding: 0 6px;
  color: var(--color-text);
  font-size: 11.5px;
  font-family: "Bahnschrift", monospace;
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

.workbench-loading-overlay {
  position: absolute;
  inset: 0;
  z-index: 60;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  border-radius: inherit;
  background: color-mix(in srgb, var(--color-panel, #fff) 82%, transparent);
  backdrop-filter: blur(4px);
  color: var(--color-primary, #2563eb);
  pointer-events: all;
  user-select: none;
}
.overlay-text {
  color: var(--color-text-muted);
  font-size: 13px;
  font-weight: 500;
}

:global(:root[data-theme="dark"]) .batch-queue-panel,
:global(.tools-view--dark) .batch-queue-panel {
  background: var(--color-panel);
  border-color: var(--color-line);
  .batch-queue-table-card {
    background: var(--color-panel);
    border-color: var(--color-line);
  }
  .batch-table-head {
    background: var(--color-panel-soft);
    border-color: var(--color-line);
  }
  .batch-table-row {
    border-color: var(--color-line);
    &:hover {
      background: rgba(37, 99, 235, 0.12);
    }
    &.is-selected {
      background: rgba(37, 99, 235, 0.18);
    }
  }
  .queue-status-pill.is-processing {
    background: rgba(37, 99, 235, 0.16);
    border-color: rgba(96, 165, 250, 0.3);
    color: #60a5fa;
  }
  .queue-status-pill.is-queued {
    background: rgba(234, 179, 8, 0.16);
    border-color: rgba(234, 179, 8, 0.3);
    color: #facc15;
  }
  .queue-status-pill.is-completed {
    background: rgba(34, 197, 94, 0.16);
    border-color: rgba(34, 197, 94, 0.3);
    color: #4ade80;
  }
  .queue-status-pill.is-failed {
    background: rgba(239, 68, 68, 0.16);
    border-color: rgba(239, 68, 68, 0.3);
    color: #f87171;
  }
  .col-status-filter-tag {
    color: #60a5fa;
  }
  .batch-concurrency-control {
    background: var(--color-panel);
    border-color: var(--color-line);
  }
  .task-running-track {
    background: rgba(37, 99, 235, 0.2);
  }
  .task-running-bar {
    background: #3b82f6;
  }
  .running-text {
    color: #60a5fa;
  }
}

@keyframes batch-bar-slide {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(250%);
  }
}
@keyframes batch-dot-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.45;
  }
}
@media (max-width: 700px) {
  .batch-table-head,
  .batch-table-row {
    grid-template-columns: 28px minmax(0, 1fr) 90px 64px;
    padding-right: 8px;
    padding-left: 8px;
  }
  .batch-queue-panel {
    padding: 10px;
  }
  .batch-table-footer {
    align-items: flex-start;
    flex-direction: column;
    gap: 8px;
  }
  .footer-right {
    width: 100%;
    justify-content: space-between;
  }
}
</style>

<template>
  <section class="logs-view">
    <header class="logs-view-header">
      <div class="logs-view-title">
        <span class="logs-view-mark">调试日志</span>
        <h1 class="logs-view-heading">调试日志</h1>
        <p class="logs-view-path">
          {{ filePath || "记录调用详情和前后端异常，按日期保存最近 7 天。" }}
        </p>
      </div>
      <div class="logs-view-actions">
        <button
          class="logs-view-action-button"
          type="button"
          @click="emit('refresh', selectedDate)"
        >
          <RefreshCw :size="15" />
          刷新
        </button>
        <button
          class="logs-view-action-button"
          type="button"
          :disabled="!filteredLogs.length"
          @click="exportLogs"
        >
          <Download :size="15" />
          导出筛选结果
        </button>
        <button
          class="logs-view-action-button"
          type="button"
          @click="emit('clear')"
        >
          清空
        </button>
      </div>
    </header>

    <div class="logs-view-filters">
      <label class="logs-view-filter">
        <span class="logs-view-filter-label">日期</span>
        <select
          :value="selectedDate"
          class="logs-view-select"
          aria-label="日期"
          @change="emit('refresh', $event.target.value)"
        >
          <option value="">最近 7 天</option>
          <option v-for="date in dates" :key="date" :value="date">
            {{ date }}
          </option>
        </select>
      </label>
      <label class="logs-view-filter">
        <span class="logs-view-filter-label">分类</span>
        <select
          v-model="scopeFilter"
          class="logs-view-select"
          aria-label="分类"
        >
          <option value="all">全部</option>
          <option v-for="scope in scopeOptions" :key="scope" :value="scope">
            {{ formatLogScope(scope) }}
          </option>
        </select>
      </label>
      <label class="logs-view-filter">
        <span class="logs-view-filter-label">服务</span>
        <select
          v-model="serviceFilter"
          class="logs-view-select"
          aria-label="服务"
        >
          <option value="all">全部</option>
          <option
            v-for="service in serviceOptions"
            :key="service"
            :value="service"
          >
            {{ service }}
          </option>
        </select>
      </label>
      <label class="logs-view-filter">
        <span class="logs-view-filter-label">状态</span>
        <select
          v-model="statusFilter"
          class="logs-view-select"
          aria-label="状态"
        >
          <option value="all">全部</option>
          <option value="success">成功</option>
          <option value="error">失败</option>
          <option value="pending">进行中</option>
        </select>
      </label>
      <span data-emphasis class="logs-view-count"
        >{{ filteredLogs.length }} 条</span
      >
    </div>

    <div class="logs-view-query">
      <input
        v-model="keyword"
        class="logs-view-search"
        type="search"
        aria-label="搜索调试日志"
        placeholder="搜索接口、错误、调用编号或参数内容"
      />
      <span class="logs-view-hint">
        {{
          hasMore
            ? "当前仅展示最近 1000 条，可按日期缩小范围；磁盘日志保留 7 天。"
            : "按日期存储，过期日志自动彻底删除。"
        }}
      </span>
    </div>

    <div v-if="filteredLogs.length" class="logs-view-list">
      <article
        v-for="item in pagedLogs"
        :key="item.id"
        :class="[
          'logs-view-item',
          { 'logs-view-item-error': item.status === 'error' }
        ]"
      >
        <div class="logs-view-item-head">
          <span data-emphasis class="logs-view-item-title">{{
            formatLogTitle(item)
          }}</span>
          <div class="logs-view-item-actions">
            <span class="logs-view-item-status">{{
              formatLogStatus(item.status)
            }}</span>
            <button
              class="logs-view-copy"
              type="button"
              aria-label="复制日志详情"
              @click="copyLog(item)"
            >
              <Copy :size="14" />
              复制
            </button>
          </div>
        </div>
        <p v-if="item.message" class="logs-view-item-message">
          {{ item.message }}
        </p>
        <div class="logs-view-meta">
          <span class="logs-view-meta-item">{{
            formatLogTime(item.createdAt)
          }}</span>
          <span class="logs-view-meta-item">{{
            formatLogScope(item.scope)
          }}</span>
          <span class="logs-view-meta-item">{{
            item.service || "未知服务"
          }}</span>
          <span class="logs-view-meta-item">{{
            item.method || item.channel
          }}</span>
          <span class="logs-view-meta-item">{{ item.action }}</span>
          <span class="logs-view-meta-item">{{ item.durationMs || 0 }}ms</span>
          <span class="logs-view-meta-item">{{ item.traceId }}</span>
          <span v-if="item.context?.window" class="logs-view-meta-item"
            >窗口：{{ item.context.window }}</span
          >
          <span v-if="item.context?.view" class="logs-view-meta-item"
            >页面：{{ item.context.view }}</span
          >
        </div>
        <details class="logs-view-details" :open="item.status === 'error'">
          <summary class="logs-view-detail-summary">
            参数、结果、错误与运行环境
          </summary>
          <template v-for="field in detailFields" :key="field.key">
            <template
              v-if="item[field.key] !== undefined && item[field.key] !== null"
            >
              <p class="logs-view-detail-label">{{ field.label }}</p>
              <pre class="logs-view-payload">{{
                formatLogPayload(item[field.key])
              }}</pre>
            </template>
          </template>
        </details>
      </article>
    </div>

    <div v-if="filteredLogs.length" class="logs-view-pagination">
      <span class="logs-view-page-range">
        {{ pageStart }}-{{ pageEnd }} / {{ filteredLogs.length }}
      </span>
      <select v-model.number="pageSize" class="logs-view-page-select">
        <option :value="20">20 条/页</option>
        <option :value="50">50 条/页</option>
        <option :value="100">100 条/页</option>
      </select>
      <button
        class="logs-view-page-button"
        type="button"
        :disabled="currentPage <= 1"
        @click="goPage(currentPage - 1)"
      >
        上一页
      </button>
      <span data-emphasis class="logs-view-page-current">
        {{ currentPage }} / {{ pageCount }}
      </span>
      <button
        class="logs-view-page-button"
        type="button"
        :disabled="currentPage >= pageCount"
        @click="goPage(currentPage + 1)"
      >
        下一页
      </button>
    </div>

    <div v-else class="logs-view-empty">暂无调用日志。</div>
  </section>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import { Copy, Download, RefreshCw } from "lucide-vue-next"
import { createMessage } from "@/utils/message"

const props = defineProps({
  logs: {
    type: Array,
    default: () => []
  },
  filePath: {
    type: String,
    default: ""
  },
  dates: { type: Array, default: () => [] },
  selectedDate: { type: String, default: "" },
  hasMore: { type: Boolean, default: false }
})

const emit = defineEmits(["refresh", "clear"])

const scopeFilter = ref("all")
const serviceFilter = ref("all")
const statusFilter = ref("all")
const page = ref(1)
const pageSize = ref(20)
const keyword = ref("")
const detailFields = [
  { key: "payload", label: "请求参数" },
  { key: "result", label: "返回结果（过大内容会注明截断）" },
  { key: "error", label: "错误详情" },
  { key: "context", label: "调用来源与运行环境" }
]

// 开始和结束共用调用编号，已结束的调用合成一条；中断的调用仍显示为进行中。
const callLogs = computed(() => {
  const completed = new Set(
    props.logs
      .filter((item) => item.action === "finish" && item.traceId)
      .map((item) => `${item.scope}:${item.traceId}`)
  )
  return props.logs.filter(
    (item) =>
      item.action !== "start" ||
      !item.traceId ||
      !completed.has(`${item.scope}:${item.traceId}`)
  )
})

const scopeOptions = computed(() =>
  [...new Set(props.logs.map((item) => item.scope || "backend"))].sort()
)

const serviceOptions = computed(() =>
  [
    ...new Set(
      props.logs.map((item) => item.service || "未知服务").filter(Boolean)
    )
  ].sort()
)

const filteredLogs = computed(() =>
  callLogs.value.filter((item) => {
    const scope = item.scope || "backend"
    const service = item.service || "未知服务"

    return (
      (scopeFilter.value === "all" || scopeFilter.value === scope) &&
      (serviceFilter.value === "all" || serviceFilter.value === service) &&
      (statusFilter.value === "all" || statusFilter.value === item.status) &&
      (!keyword.value.trim() ||
        JSON.stringify(item)
          .toLowerCase()
          .includes(keyword.value.trim().toLowerCase()))
    )
  })
)

const pageCount = computed(() =>
  Math.max(1, Math.ceil(filteredLogs.value.length / pageSize.value))
)

const currentPage = computed(() => Math.min(page.value, pageCount.value))

const pageStart = computed(() => {
  if (!filteredLogs.value.length) {
    return 0
  }

  return (currentPage.value - 1) * pageSize.value + 1
})

const pageEnd = computed(() =>
  Math.min(currentPage.value * pageSize.value, filteredLogs.value.length)
)

const pagedLogs = computed(() =>
  filteredLogs.value.slice(pageStart.value - 1, pageEnd.value)
)

watch(
  [
    scopeFilter,
    serviceFilter,
    statusFilter,
    keyword,
    pageSize,
    () => props.logs
  ],
  () => {
    page.value = 1
  }
)

function formatLogTime(value) {
  const date = new Date(Number(value || 0))
  if (Number.isNaN(date.getTime())) return "时间未知"
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    fractionalSecondDigits: 3
  }).format(date)
}

// 复制和导出都使用已落盘的脱敏内容，便于直接提供同一调用链的排查材料。
async function copyLog(item) {
  try {
    await navigator.clipboard.writeText(JSON.stringify(item, null, 2))
    createMessage.success("日志详情已复制")
  } catch {
    createMessage.error("日志复制失败，请使用导出功能")
  }
}

function exportLogs() {
  const blob = new Blob(
    [
      JSON.stringify(
        {
          exportedAt: new Date().toISOString(),
          date: props.selectedDate || "最近 7 天",
          logs: filteredLogs.value
        },
        null,
        2
      )
    ],
    { type: "application/json" }
  )
  const url = URL.createObjectURL(blob)
  const link = document.createElement("a")
  link.href = url
  link.download = `app-debug-${Date.now()}.json`
  link.click()
  URL.revokeObjectURL(url)
}

function formatLogPayload(value) {
  return JSON.stringify(value, null, 2)
}

function formatLogScope(value) {
  if (value === "backend") {
    return "后端"
  }

  if (value === "renderer") {
    return "渲染进程"
  }

  return value || "未知"
}

function formatLogStatus(value) {
  if (value === "success") {
    return "成功"
  }

  if (value === "error") {
    return "失败"
  }

  if (value === "pending") {
    return "进行中"
  }

  return value || "未知"
}

function formatLogTitle(item) {
  const method = item.method || item.channel || ""
  return method.startsWith(`${item.service}:`)
    ? method
    : [item.service, method].filter(Boolean).join(".")
}

function goPage(nextPage) {
  page.value = Math.min(Math.max(nextPage, 1), pageCount.value)
}
</script>

<style scoped lang="less">
.logs-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 14px;
  overflow: hidden;

  .logs-view-header {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);

    .logs-view-title {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 4px;

      .logs-view-mark {
        color: var(--color-text-soft);
        font-size: var(--font-size-xs);
        letter-spacing: 0.14em;
      }

      .logs-view-heading {
        margin: 0;
        color: var(--color-primary);
        font-size: var(--font-size-xl);
      }

      .logs-view-path {
        margin: 0;
        color: var(--color-text-muted);
        font-size: var(--font-size-base);
      }
    }

    .logs-view-actions {
      display: flex;
      gap: 8px;

      .logs-view-action-button {
        display: inline-flex;
        height: 36px;
        align-items: center;
        gap: 6px;
        padding: 0 12px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);
        color: var(--color-primary);
        cursor: pointer;

        &:disabled {
          cursor: not-allowed;
          opacity: 0.5;
        }
      }
    }
  }

  .logs-view-filters {
    display: flex;
    flex: none;
    align-items: end;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);

    .logs-view-filter {
      display: flex;
      flex-direction: column;
      gap: 6px;

      .logs-view-filter-label {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }

      .logs-view-select {
        width: 140px;
        height: 34px;
        padding: 0 10px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);
        color: var(--color-primary);
      }
    }

    .logs-view-count {
      margin-left: auto;
      color: var(--color-text-muted);
      font-size: var(--font-size-base);
    }
  }

  .logs-view-query {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;

    .logs-view-search {
      min-width: 0;
      height: 34px;
      flex: 1;
      padding: 0 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-primary);
    }

    .logs-view-hint {
      max-width: 360px;
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
  }

  .logs-view-list {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    padding-right: 4px;

    .logs-view-item {
      padding: 14px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);

      .logs-view-item-head,
      .logs-view-meta {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
      }

      .logs-view-item-title {
        color: var(--color-primary);
        font-size: var(--font-size-lg);
      }

      .logs-view-item-status {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }

      .logs-view-item-actions {
        display: flex;
        align-items: center;
        gap: 12px;

        .logs-view-copy {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 4px 8px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
          color: var(--color-primary);
          cursor: pointer;
        }
      }

      .logs-view-item-message {
        margin: 8px 0 0;
        color: var(--color-danger);
        font-size: var(--font-size-base);
      }

      .logs-view-meta {
        justify-content: flex-start;
        flex-wrap: wrap;
        margin-top: 8px;

        .logs-view-meta-item {
          color: var(--color-text-soft);
          font-size: var(--font-size-sm);
        }
      }

      .logs-view-details {
        margin-top: 10px;

        .logs-view-detail-summary {
          color: var(--color-primary);
          cursor: pointer;
          font-size: var(--font-size-sm);
        }

        .logs-view-detail-label {
          margin: 12px 0 4px;
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }

        .logs-view-payload {
          overflow: auto;
          max-height: 260px;
          margin: 0;
          padding: 10px;
          border-radius: 8px;
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
          line-height: 1.55;
        }
      }
    }

    .logs-view-item-error {
      border-color: var(--color-danger-line);
      background: var(--color-danger-soft);
    }
  }

  .logs-view-pagination {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    font-size: var(--font-size-base);

    .logs-view-page-select,
    .logs-view-page-button {
      height: 32px;
      padding: 0 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      color: var(--color-primary);
      cursor: pointer;
    }

    .logs-view-page-button {
      &:disabled {
        cursor: not-allowed;
        opacity: 0.48;
      }
    }
  }

  .logs-view-empty {
    display: flex;
    min-height: 300px;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
  }
}
</style>

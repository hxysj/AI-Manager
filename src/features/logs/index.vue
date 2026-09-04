<template>
  <section class="logs-view">
    <header class="logs-view-header">
      <div class="logs-view-title">
        <span class="logs-view-mark">调用日志</span>
        <h1 class="logs-view-heading">调用日志</h1>
        <p class="logs-view-path">
          {{ filePath || "记录所有后端服务调用过程。" }}
        </p>
      </div>
      <div class="logs-view-actions">
        <button class="logs-view-action-button" type="button" @click="emit('refresh')">
          <RefreshCw :size="15" />
          刷新
        </button>
        <button class="logs-view-action-button" type="button" @click="emit('clear')">
          清空
        </button>
      </div>
    </header>

    <div class="logs-view-filters">
      <label class="logs-view-filter">
        <span class="logs-view-filter-label">分类</span>
        <select v-model="scopeFilter" class="logs-view-select">
          <option value="all">全部</option>
          <option v-for="scope in scopeOptions" :key="scope" :value="scope">
            {{ formatLogScope(scope) }}
          </option>
        </select>
      </label>
      <label class="logs-view-filter">
        <span class="logs-view-filter-label">服务</span>
        <select v-model="serviceFilter" class="logs-view-select">
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
        <select v-model="statusFilter" class="logs-view-select">
          <option value="all">全部</option>
          <option value="success">成功</option>
          <option value="error">失败</option>
          <option value="pending">进行中</option>
        </select>
      </label>
      <strong class="logs-view-count">{{ filteredLogs.length }} 条</strong>
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
          <strong class="logs-view-item-title">{{ formatLogTitle(item) }}</strong>
          <span class="logs-view-item-status">{{
            formatLogStatus(item.status)
          }}</span>
        </div>
        <p v-if="item.message" class="logs-view-item-message">
          {{ item.message }}
        </p>
        <div class="logs-view-meta">
          <span class="logs-view-meta-item">{{ formatLogTime(item.createdAt) }}</span>
          <span class="logs-view-meta-item">{{ formatLogScope(item.scope) }}</span>
          <span class="logs-view-meta-item">{{ item.service || "未知服务" }}</span>
          <span class="logs-view-meta-item">{{ item.method || item.channel }}</span>
          <span class="logs-view-meta-item">{{ item.action }}</span>
          <span class="logs-view-meta-item">{{ item.durationMs || 0 }}ms</span>
          <span class="logs-view-meta-item">{{ item.traceId }}</span>
        </div>
        <pre v-if="item.payload" class="logs-view-payload">{{
          formatLogPayload(item.payload)
        }}</pre>
        <pre v-if="item.result" class="logs-view-payload">{{
          formatLogPayload(item.result)
        }}</pre>
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
      <strong class="logs-view-page-current">
        {{ currentPage }} / {{ pageCount }}
      </strong>
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
import { RefreshCw } from "lucide-vue-next"

const props = defineProps({
  logs: {
    type: Array,
    default: () => []
  },
  filePath: {
    type: String,
    default: ""
  }
})

const emit = defineEmits(["refresh", "clear"])

const scopeFilter = ref("all")
const serviceFilter = ref("all")
const statusFilter = ref("all")
const page = ref(1)
const pageSize = ref(20)

const scopeOptions = computed(() =>
  [...new Set(props.logs.map(item => item.scope || "backend"))].sort()
)

const serviceOptions = computed(() =>
  [
    ...new Set(
      props.logs.map(item => item.service || "未知服务").filter(Boolean)
    )
  ].sort()
)

const filteredLogs = computed(() =>
  props.logs.filter(item => {
    const scope = item.scope || "backend"
    const service = item.service || "未知服务"

    return (
      (scopeFilter.value === "all" || scopeFilter.value === scope) &&
      (serviceFilter.value === "all" || serviceFilter.value === service) &&
      (statusFilter.value === "all" || statusFilter.value === item.status)
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
  [scopeFilter, serviceFilter, statusFilter, pageSize, () => props.logs],
  () => {
    page.value = 1
  }
)

function formatLogTime(value) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(Number(value || 0)))
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
  return [item.service, item.method || item.channel].filter(Boolean).join(".")
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
        font-size: 0.72rem;
        font-weight: 700;
        letter-spacing: 0.14em;
      }

      .logs-view-heading {
        margin: 0;
        color: var(--color-primary);
        font-size: 1.35rem;
      }

      .logs-view-path {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 0.86rem;
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
        font-weight: 700;
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
        font-size: 0.78rem;
        font-weight: 700;
      }

      .logs-view-select {
        width: 180px;
        height: 34px;
        padding: 0 10px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);
        color: var(--color-primary);
        font-weight: 700;
      }
    }

    .logs-view-count {
      margin-left: auto;
      color: var(--color-text-muted);
      font-size: 0.84rem;
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
        font-size: 0.95rem;
      }

      .logs-view-item-status {
        color: var(--color-text-muted);
        font-size: 0.82rem;
        font-weight: 700;
      }

      .logs-view-item-message {
        margin: 8px 0 0;
        color: var(--color-danger);
        font-size: 0.86rem;
      }

      .logs-view-meta {
        justify-content: flex-start;
        flex-wrap: wrap;
        margin-top: 8px;

        .logs-view-meta-item {
          color: var(--color-text-soft);
          font-size: 0.78rem;
        }
      }

      .logs-view-payload {
        overflow: auto;
        max-height: 220px;
        margin: 10px 0 0;
        padding: 10px;
        border-radius: 8px;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: 0.78rem;
        line-height: 1.55;
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
    font-size: 0.84rem;
    font-weight: 700;

    .logs-view-page-select,
    .logs-view-page-button {
      height: 32px;
      padding: 0 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      color: var(--color-primary);
      cursor: pointer;
      font-weight: 700;
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

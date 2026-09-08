<template>
  <section class="key-usage" :data-status="status">
    <header class="key-usage-header">
      <span class="key-usage-title">请求概览</span>
      <span class="key-usage-status"
        ><span class="key-usage-status-dot"></span>{{ statusLabel }}</span
      >
    </header>
    <div v-if="pending" class="key-usage-empty">
      <KeyRound class="key-usage-empty-icon" :size="24" />
      <span class="key-usage-empty-title">保存后开始统计</span>
      <span>当前密钥尚未保存，不会沿用其他密钥的统计。</span>
    </div>
    <template v-else-if="usage?.requestCount">
      <div class="key-usage-metrics">
        <div
          v-for="metric in metrics"
          :key="metric.label"
          class="key-usage-metric"
        >
          <span class="key-usage-label" :title="metric.title">{{
            metric.label
          }}</span>
          <span class="key-usage-value">{{ metric.value }}</span>
        </div>
      </div>
      <footer class="key-usage-history">
        <span v-if="usage.lastStatusCode" class="key-usage-http"
          >最近结果
          <span class="key-usage-http-code"
            >HTTP {{ usage.lastStatusCode }}</span
          ></span
        >
        <span>最近请求：{{ formatDateTime(usage.lastUsedAt) }}</span>
        <span v-if="usage.lastFailureAt" class="key-usage-failure">
          最近失败：{{ formatDateTime(usage.lastFailureAt) }} ·
          {{ failureLabels[usage.lastFailureKind] || "请求失败" }}
        </span>
      </footer>
    </template>
    <div v-else class="key-usage-empty">
      <Activity class="key-usage-empty-icon" :size="24" />
      <span class="key-usage-empty-title">{{
        usage ? "暂无请求记录" : "统计数据尚未加载"
      }}</span>
      <span>{{
        usage
          ? "通过本项目发送请求后，这里会显示此 Key 的使用情况。"
          : "可点击刷新重新获取当前 Key 的使用情况。"
      }}</span>
    </div>
  </section>
</template>

<script setup>
import { computed } from "vue"
import { Activity, KeyRound } from "lucide-vue-next"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  usage: { type: Object, default: null },
  pending: { type: Boolean, default: false }
})

const failureLabels = {
  auth: "鉴权失败",
  rate_limit: "触发限流",
  upstream: "上游服务异常",
  http: "HTTP 请求失败",
  network: "网络连接失败",
  stream: "响应读取或流中断",
  api: "上游返回错误事件",
  cancelled: "请求取消或连接关闭"
}

const status = computed(() => {
  if (props.pending) return "pending"
  if (!props.usage) return "loading"
  if (props.usage.inFlightCount > 0) return "running"
  if (props.usage.lastOutcome === "failure") return "failure"
  if (props.usage.lastOutcome === "success") return "success"
  return "empty"
})

const statusLabel = computed(
  () =>
    ({
      pending: "待保存",
      loading: "未加载",
      running: "请求中",
      failure: "最近请求失败",
      success: "最近请求成功",
      empty: "尚无已完成请求"
    })[status.value]
)

const metrics = computed(() => {
  const usage = props.usage || {}
  const completed =
    Number(usage.successCount || 0) + Number(usage.failureCount || 0)
  const formatCount = (value) => Number(value || 0).toLocaleString("zh-CN")
  return [
    { label: "请求次数", value: formatCount(usage.requestCount) },
    { label: "成功", value: formatCount(usage.successCount) },
    { label: "失败", value: formatCount(usage.failureCount) },
    {
      label: "失败率",
      value: completed
        ? `${((usage.failureCount / completed) * 100).toFixed(1)}%`
        : "—",
      title: "失败次数 / 已完成请求，不包含进行中的请求"
    },
    { label: "连续失败", value: formatCount(usage.consecutiveFailures) },
    { label: "进行中", value: formatCount(usage.inFlightCount) }
  ]
})
</script>

<style scoped lang="less">
.key-usage {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  border: 1px solid var(--color-line);
  border-radius: 10px;
  background: var(--color-panel);
  font-size: 12px;

  .key-usage-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;

    .key-usage-title {
      color: var(--color-text);
      font-size: 13px;
    }

    .key-usage-status {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      padding: 3px 7px;
      border-radius: 5px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      font-size: 11px;

      .key-usage-status-dot {
        width: 5px;
        height: 5px;
        flex: none;
        border-radius: 50%;
        background: currentColor;
      }
    }
  }

  &[data-status="failure"] .key-usage-header .key-usage-status {
    color: var(--color-danger);
    background: var(--color-danger-soft);
  }

  &[data-status="running"] .key-usage-header .key-usage-status {
    color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  &[data-status="success"] .key-usage-header .key-usage-status {
    color: var(--color-success);
    background: var(--color-success-soft);
  }

  .key-usage-metrics {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;

    .key-usage-metric {
      display: flex;
      flex-direction: column;
      min-width: 0;
      gap: 5px;
      padding: 8px;
      border-radius: 7px;
      background: var(--color-panel-soft);

      .key-usage-label {
        color: var(--color-text-muted);
      }

      .key-usage-value {
        color: var(--color-text);
        font-size: 18px;
        line-height: 1.25;
        font-variant-numeric: tabular-nums;
        overflow-wrap: anywhere;
      }
    }
  }

  .key-usage-history {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 12px;
    border-top: 1px solid var(--color-line);
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
    font-size: 11px;

    .key-usage-http {
      display: flex;
      align-items: center;
      gap: 7px;

      .key-usage-http-code {
        color: var(--color-text);
        font-variant-numeric: tabular-nums;
      }
    }

    .key-usage-failure {
      color: var(--color-danger);
    }
  }

  .key-usage-empty {
    display: flex;
    align-items: center;
    flex-direction: column;
    gap: 9px;
    padding: 24px 10px;
    color: var(--color-text-muted);
    font-size: 12px;
    line-height: 1.7;
    text-align: center;

    .key-usage-empty-icon {
      box-sizing: content-box;
      padding: 12px;
      border-radius: 12px;
      background: var(--color-panel-soft);
      color: var(--color-text-soft);
    }

    .key-usage-empty-title {
      color: var(--color-text);
      font-size: 13px;
    }
  }

  @media (max-width: 480px) {
    .key-usage-metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
}
</style>

<template>
  <BaseModal
    class="translation-history-modal"
    role="dialog"
    aria-modal="true"
    @close="$emit('close')"
  >
    <template #header>
      <div class="translation-modal-header">
        <div class="translation-modal-title">
          <div class="translation-modal-icon">
            <Clock :size="18" />
          </div>
          <h3>翻译记录</h3>
        </div>
        <button
          class="translation-modal-close"
          type="button"
          aria-label="关闭"
          @click="$emit('close')"
        >
          <X :size="18" />
        </button>
      </div>
    </template>

    <div class="translation-history-body">
      <div class="translation-toolbar">
        <div class="translation-tabs">
          <button
            v-for="tab in tabs"
            :key="tab.id"
            type="button"
            :class="[
              'translation-tab-btn',
              { 'translation-tab-btn-active': kind === tab.id }
            ]"
            @click="changeTab(tab.id)"
          >
            <component :is="tab.icon" :size="14" />
            <span>{{ tab.label }}</span>
          </button>
        </div>
        <button
          class="translation-refresh-btn"
          type="button"
          :disabled="loading"
          @click="loadRecords"
        >
          <RotateCw :size="14" :class="{ 'spin-icon': loading }" />
          <span>{{ loading ? "刷新中…" : "刷新" }}</span>
        </button>
      </div>

      <div class="translation-summary-card">
        <div class="translation-summary-stats">
          <span
            >请求 <strong>{{ summary.requestCount || 0 }}</strong> 次</span
          >
          <span
            >输入
            <strong>{{ formatMetricTokens(summary.inputTokens) }}</strong></span
          >
          <span
            >输出
            <strong>{{
              formatMetricTokens(summary.outputTokens)
            }}</strong></span
          >
          <span
            >缓存命中
            <strong>{{
              formatMetricTokens(summary.cacheReadTokens)
            }}</strong></span
          >
          <span
            >推理
            <strong>{{
              formatMetricTokens(summary.reasoningTokens)
            }}</strong></span
          >
        </div>
        <div class="translation-summary-hint">
          缓存命中包含在输入中，推理包含在输出中。<template
            v-if="summary.unknownUsageCount"
            >{{ summary.unknownUsageCount }} 次请求未返回用量，未计入 token
            合计。</template
          >
        </div>
      </div>

      <p v-if="errorMessage" class="translation-error-banner">
        {{ errorMessage }}
      </p>

      <div v-if="!items.length && !loading" class="translation-empty-state">
        暂无{{ kind === "history" ? "翻译历史" : "消耗记录" }}
      </div>

      <div class="translation-list">
        <details
          v-for="item in items"
          :key="item.id"
          class="translation-record-card"
        >
          <summary class="translation-record-head">
            <div class="translation-record-left">
              <ChevronRight :size="15" class="record-chevron" />
              <span class="translation-record-title">
                {{
                  (kind === "history" ? item.sourceText : item.model) ||
                  "（无内容）"
                }}
              </span>
            </div>
            <div class="translation-record-header-meta">
              <span
                class="status-badge"
                :class="`status-${item.status || 'unknown'}`"
              >
                {{ statusLabels[item.status] || item.status || "未知" }}
              </span>
              <span class="time-text">{{
                formatDateTime(item.createdAt)
              }}</span>
            </div>
          </summary>
          <div class="translation-record-body">
            <div class="translation-record-info">
              {{ item.providerName }} · {{ item.model }} ·
              {{ item.engine === "codex" ? "Codex" : "LangChain" }} ·
              {{ item.durationMs || 0 }} ms
            </div>
            <div v-if="kind === 'history'" class="translation-texts">
              <span class="translation-subhint"
                >原文 → {{ item.targetLanguage }}</span
              >
              <pre class="translation-pre">{{ item.sourceText }}</pre>
              <pre
                v-if="item.translatedText"
                class="translation-pre translation-result-pre"
                >{{ item.translatedText }}</pre
              >
            </div>
            <p v-if="item.errorMessage" class="translation-item-error">
              {{ item.errorMessage }}
            </p>
            <div class="translation-record-meta">
              <template v-if="item.usageKnown">
                输入 {{ item.inputTokens }} · 输出 {{ item.outputTokens }} ·
                缓存命中 {{ item.cacheReadTokens }} · 推理
                {{ item.reasoningTokens }}
              </template>
              <template v-else>未返回完整用量</template>
              <span v-if="kind === 'usage'">
                · HTTP {{ item.statusCode || "—" }} · 翻译记录
                {{ item.translationId }}
              </span>
            </div>
          </div>
        </details>
      </div>

      <footer class="translation-pagination">
        <span class="pagination-info"
          >共 {{ total }} 条 · 第 {{ page }} 页</span
        >
        <div class="pagination-buttons">
          <button
            class="pagination-btn"
            type="button"
            :disabled="page <= 1 || loading"
            @click="changePage(-1)"
          >
            上一页
          </button>
          <button
            class="pagination-btn"
            type="button"
            :disabled="page * 20 >= total || loading"
            @click="changePage(1)"
          >
            下一页
          </button>
        </div>
      </footer>
    </div>
  </BaseModal>
</template>

<script setup>
import { onBeforeUnmount, onMounted, ref } from "vue"
import {
  BarChart2,
  ChevronRight,
  Clock,
  FileText,
  RotateCw,
  X
} from "lucide-vue-next"
import { translationApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"
import BaseModal from "@/components/BaseModal.vue"

defineEmits(["close"])

const tabs = [
  { id: "history", label: "翻译历史", icon: FileText },
  { id: "usage", label: "消耗记录", icon: BarChart2 }
]

const statusLabels = {
  running: "进行中",
  success: "成功",
  failed: "失败",
  interrupted: "已中断"
}

const kind = ref("history")
const items = ref([])
const summary = ref({})
const page = ref(1)
const total = ref(0)
const loading = ref(false)
const errorMessage = ref("")
let requestId = 0
let unsubscribe = null

function formatMetricTokens(val) {
  const num = Number(val || 0)
  if (!Number.isFinite(num) || num === 0) return "0"
  if (num >= 1000000) {
    return `${(num / 1000000).toFixed(2).replace(/\.00$/, "")}M (${num.toLocaleString("zh-CN")})`
  }
  if (num >= 1000) {
    return `${(num / 1000).toFixed(2).replace(/\.00$/, "")}k (${num.toLocaleString("zh-CN")})`
  }
  return `${num.toLocaleString("zh-CN")}`
}

async function loadRecords() {
  const current = ++requestId
  loading.value = true
  errorMessage.value = ""
  try {
    const result = await translationApi.list({
      kind: kind.value,
      page: page.value
    })
    if (current !== requestId) return
    items.value = result.items || []
    total.value = result.total || 0
    summary.value = result.summary || {}
  } catch (error) {
    if (current === requestId) {
      errorMessage.value = error.message || String(error)
    }
  } finally {
    if (current === requestId) {
      loading.value = false
    }
  }
}

function changeTab(value) {
  if (kind.value === value) return
  kind.value = value
  page.value = 1
  items.value = []
  loadRecords()
}

function changePage(offset) {
  page.value += offset
  loadRecords()
}

onMounted(() => {
  loadRecords()
  unsubscribe = translationApi.onChanged(loadRecords)
})

onBeforeUnmount(() => {
  requestId += 1
  unsubscribe?.()
})
</script>

<style scoped lang="less">
.translation-history-modal {
  :deep(.base-modal__panel) {
    width: 820px;
    max-width: calc(100vw - 32px);
    height: 680px;
    max-height: calc(90vh - 32px);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  :deep(.base-modal__header) {
    padding: 0;
    border-bottom: none;
    flex-shrink: 0;
  }

  :deep(.base-modal__content) {
    padding: 0;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .translation-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 16px 24px 14px;
    border-bottom: 1px solid var(--color-line);
    flex-shrink: 0;

    .translation-modal-title {
      display: flex;
      align-items: center;
      gap: 10px;

      .translation-modal-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        color: #2563eb;
      }

      h3 {
        margin: 0;
        font-size: 17px;
        font-weight: 600;
        color: var(--color-text);
      }
    }

    .translation-modal-close {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 32px;
      height: 32px;
      border: none;
      border-radius: 6px;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      transition:
        background 0.15s ease,
        color 0.15s ease;

      &:hover {
        background: var(--color-panel-soft);
        color: var(--color-text);
      }
    }
  }

  .translation-history-body {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    padding: 16px 24px 20px;
    overflow: hidden;

    .translation-toolbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      flex-shrink: 0;

      .translation-tabs {
        display: flex;
        align-items: center;
        gap: 8px;

        .translation-tab-btn {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          padding: 6px 14px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: transparent;
          color: var(--color-text-muted);
          font-size: 13px;
          cursor: pointer;
          transition: all 0.15s ease;

          &:hover {
            border-color: var(--color-line-strong);
            color: var(--color-text);
          }

          &.translation-tab-btn-active {
            background: #2563eb;
            border-color: #2563eb;
            color: #ffffff;
            font-weight: 500;
          }
        }
      }

      .translation-refresh-btn {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 6px 14px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 13px;
        cursor: pointer;
        transition: all 0.15s ease;

        &:hover:not(:disabled) {
          background: var(--color-panel-soft);
          border-color: var(--color-line-strong);
        }

        &:disabled {
          opacity: 0.6;
          cursor: default;
        }

        .spin-icon {
          animation: spin 1s linear infinite;
        }
      }
    }

    .translation-summary-card {
      margin-top: 14px;
      padding: 12px 18px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      flex-shrink: 0;

      .translation-summary-stats {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 20px;
        font-size: 13px;
        color: var(--color-text);

        strong {
          font-weight: 600;
        }
      }

      .translation-summary-hint {
        margin-top: 6px;
        color: var(--color-text-muted);
        font-size: 12px;
        line-height: 1.5;
      }
    }

    .translation-error-banner {
      margin: 12px 0 0;
      padding: 10px 14px;
      border-radius: 6px;
      background: var(--color-danger-soft);
      color: var(--color-danger);
      font-size: 13px;
      flex-shrink: 0;
    }

    .translation-empty-state {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 40px;
      text-align: center;
      color: var(--color-text-muted);
      font-size: 14px;
    }

    .translation-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin-top: 14px;
      flex: 1;
      min-height: 0;
      overflow-y: auto;
      padding-right: 4px;
      scrollbar-width: thin;

      .translation-record-card {
        flex-shrink: 0;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);
        overflow: hidden;
        transition:
          border-color 0.15s ease,
          background-color 0.15s ease;

        &[open] {
          border-color: var(--color-line-strong);
          background: var(--color-panel);

          .record-chevron {
            transform: rotate(90deg);
            color: #2563eb;
          }
        }

        .translation-record-head {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 16px;
          min-height: 44px;
          padding: 10px 16px;
          cursor: pointer;
          user-select: none;
          list-style: none;

          &::-webkit-details-marker {
            display: none;
          }

          &::marker {
            display: none;
            content: "";
          }

          .translation-record-left {
            display: flex;
            align-items: center;
            gap: 8px;
            flex: 1;
            min-width: 0;

            .record-chevron {
              flex-shrink: 0;
              color: var(--color-text-muted);
              transition:
                transform 0.2s ease,
                color 0.2s ease;
            }

            .translation-record-title {
              flex: 1;
              min-width: 0;
              overflow: hidden;
              white-space: nowrap;
              text-overflow: ellipsis;
              font-size: 13.5px;
              font-weight: 500;
              color: var(--color-text);
            }
          }

          .translation-record-header-meta {
            display: flex;
            align-items: center;
            gap: 10px;
            flex-shrink: 0;

            .status-badge {
              display: inline-flex;
              align-items: center;
              padding: 2px 8px;
              border-radius: 4px;
              font-size: 12px;
              font-weight: 500;
              line-height: 1.2;

              &.status-success {
                background: #ecfdf5;
                color: #059669;
              }

              &.status-failed {
                background: #fef2f2;
                color: #dc2626;
              }

              &.status-running {
                background: #eff6ff;
                color: #2563eb;
              }

              &.status-interrupted {
                background: #fffbeb;
                color: #d97706;
              }
            }

            .time-text {
              font-size: 12px;
              color: var(--color-text-muted);
            }
          }
        }

        .translation-record-body {
          display: flex;
          flex-direction: column;
          gap: 10px;
          padding: 14px 16px 16px;
          border-top: 1px solid var(--color-line);
          background: var(--color-panel);

          .translation-record-info {
            font-size: 12px;
            color: var(--color-text-muted);
          }

          .translation-texts {
            display: flex;
            flex-direction: column;
            gap: 6px;

            .translation-subhint {
              font-size: 12px;
              color: var(--color-text-muted);
            }

            .translation-pre {
              margin: 0;
              padding: 10px 12px;
              border-radius: 6px;
              background: var(--color-panel-soft);
              border: 1px solid var(--color-line);
              font-family: inherit;
              font-size: 13.5px;
              line-height: 1.6;
              white-space: pre-wrap;
              word-break: break-word;
            }

            .translation-result-pre {
              background: #eff6ff;
              border-color: #bfdbfe;
              color: var(--color-text);
              font-weight: 500;
            }
          }

          .translation-item-error {
            margin: 0;
            color: var(--color-danger);
            font-size: 12.5px;
          }

          .translation-record-meta {
            font-size: 12px;
            color: var(--color-text-muted);
          }
        }
      }
    }

    .translation-pagination {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 16px;
      margin-top: 14px;
      padding-top: 12px;
      border-top: 1px solid var(--color-line);
      flex-shrink: 0;

      .pagination-info {
        font-size: 13px;
        color: var(--color-text-muted);
      }

      .pagination-buttons {
        display: flex;
        align-items: center;
        gap: 8px;

        .pagination-btn {
          padding: 6px 14px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel);
          color: var(--color-text);
          font-size: 13px;
          cursor: pointer;
          transition: all 0.15s ease;

          &:hover:not(:disabled) {
            background: var(--color-panel-soft);
            border-color: var(--color-line-strong);
          }

          &:disabled {
            opacity: 0.5;
            cursor: default;
          }
        }
      }
    }
  }
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>

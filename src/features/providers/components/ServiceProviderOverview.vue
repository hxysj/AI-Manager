<template>
  <div class="service-overview">
    <section class="service-summary">
      <div class="service-identity">
        <div class="service-avatar">
          <AiIcon
            v-if="provider.icon"
            class="service-icon"
            :name="provider.icon"
            :alt="provider.name"
          />
          <Server v-else :size="26" />
        </div>
        <div class="service-identity-text">
          <span class="service-caption">OpenAI 兼容服务商</span>
          <h3 class="service-name">{{ provider.name }}</h3>
          <span class="service-endpoint">{{
            provider.baseUrl || "尚未配置基础地址"
          }}</span>
        </div>
        <span class="service-protocol">{{ protocolLabel }}</span>
      </div>
      <div class="service-status-row">
        <span
          class="service-badge"
          :class="{ 'service-badge-muted': !provider.enabled }"
        >
          <span class="service-status-dot"></span
          >{{ provider.enabled ? "已启用" : "已停用" }}
        </span>
        <span
          class="service-badge"
          :class="{ 'service-badge-warning': !provider.hasCredential }"
        >
          <KeyRound :size="13" />{{
            provider.hasCredential ? "已配置凭据" : "待配置 API Key"
          }}
        </span>
      </div>
      <div class="service-metrics">
        <div
          v-for="metric in metrics"
          :key="metric.label"
          class="service-metric"
        >
          <span class="service-metric-value">{{ metric.value }}</span>
          <span class="service-caption">{{ metric.label }}</span>
        </div>
      </div>
    </section>

    <div class="service-info-grid">
      <section class="service-section">
        <div class="service-section-heading">
          <h3 class="service-title"><Network :size="16" />连接配置</h3>
        </div>
        <dl class="service-fields">
          <div
            v-for="field in connectionFields"
            :key="field.label"
            class="service-field"
          >
            <dt class="service-field-label">{{ field.label }}</dt>
            <dd class="service-field-value">{{ field.value }}</dd>
          </div>
        </dl>
      </section>
      <section class="service-section">
        <div class="service-section-heading">
          <h3 class="service-title"><KeyRound :size="16" />凭据与使用</h3>
          <button
            class="service-text-button"
            type="button"
            :disabled="busy"
            @click="$emit('keys')"
          >
            管理 API Key<ChevronRight :size="14" />
          </button>
        </div>
        <dl class="service-fields">
          <div class="service-field">
            <dt class="service-field-label">当前 Key</dt>
            <dd class="service-field-value">
              {{
                provider.hasCredential
                  ? activeKey?.name || activeKey?.logicalId || "已配置"
                  : "尚未配置"
              }}
            </dd>
          </div>
          <div class="service-field">
            <dt class="service-field-label">Key 指纹</dt>
            <dd class="service-field-value">
              {{ activeKey?.fingerprint || "未记录" }}
            </dd>
          </div>
          <div class="service-field">
            <dt class="service-field-label">可用 Key</dt>
            <dd class="service-field-value">
              {{ usableKeyCount }} / {{ keys.length }} 个
            </dd>
          </div>
          <div class="service-field">
            <dt class="service-field-label">正在使用的 Agent</dt>
            <dd class="service-field-value service-agent-list">
              <span
                v-for="agent in runningAgents"
                :key="agent"
                class="service-agent-tag"
                ><Bot :size="13" />{{ agent }}</span
              >
              <span v-if="!runningAgents.length" class="service-caption"
                >暂无 Agent 使用</span
              >
            </dd>
          </div>
        </dl>
        <p v-if="!provider.hasCredential" class="service-key-hint">
          添加 API Key 后可用于模型请求。
        </p>
      </section>
    </div>

    <section class="service-section">
      <div class="service-section-heading">
        <div class="service-heading-text">
          <h3 class="service-title">
            <Boxes :size="16" />模型目录<span class="service-model-count"
              >{{ models.length }} 个</span
            >
          </h3>
          <span class="service-caption"
            >已启用 {{ enabledModelCount }} 个 · 价格按每 100 万 Tokens
            展示</span
          >
        </div>
        <button
          class="btn service-overview-button"
          type="button"
          :disabled="busy"
          :aria-busy="busy"
          @click="$emit('fetch')"
        >
          <RefreshCw :size="14" :class="{ 'service-spinning': busy }" />{{
            busy ? "处理中…" : "获取模型"
          }}
        </button>
      </div>
      <div
        v-if="models.length"
        class="service-model-scroll"
        tabindex="0"
        aria-label="服务商模型目录"
      >
        <table class="service-model-table">
          <thead class="service-model-head">
            <tr class="service-model-row">
              <th class="service-model-cell service-model-identity" scope="col">
                模型
              </th>
              <th class="service-model-cell" scope="col">上下文</th>
              <th
                v-for="column in priceColumns"
                :key="column.key"
                class="service-model-cell"
                scope="col"
              >
                {{ column.label }}
              </th>
              <th class="service-model-cell service-model-state" scope="col">
                状态
              </th>
            </tr>
          </thead>
          <tbody class="service-model-body">
            <tr
              v-for="model in models"
              :key="model.id"
              class="service-model-row"
            >
              <td class="service-model-cell service-model-identity">
                <span class="service-model-name">{{
                  model.displayName || model.upstreamModelId
                }}</span>
                <span
                  v-if="
                    model.displayName &&
                    model.displayName !== model.upstreamModelId
                  "
                  class="service-model-id"
                  >{{ model.upstreamModelId }}</span
                >
              </td>
              <td
                class="service-model-cell"
                :title="
                  model.contextWindow
                    ? `${model.contextWindow} Tokens`
                    : undefined
                "
              >
                {{
                  model.contextWindow
                    ? formatTokenCountParts(model.contextWindow).compact
                    : "未提供"
                }}
              </td>
              <td
                v-for="column in priceColumns"
                :key="column.key"
                class="service-model-cell"
                :class="{ 'service-model-unset': model[column.key] == null }"
              >
                {{ formatPrice(model[column.key], model.currency) }}
              </td>
              <td class="service-model-cell service-model-state">
                <span
                  class="service-model-status"
                  :class="{ 'service-model-disabled': !model.enabled }"
                  >{{ model.enabled ? "已启用" : "已停用" }}</span
                >
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else class="service-empty" role="status">
        <Boxes :size="24" />
        <div class="service-empty-text">
          <span>尚未配置模型</span>
          <span class="service-caption"
            >可以获取上游模型，或在编辑配置中添加。</span
          >
        </div>
      </div>
      <p class="service-caption service-model-hint">
        模型与价格在编辑配置中维护，各 Agent 的默认模型和映射在 Agent
        配置中设置。
      </p>
    </section>

    <section v-if="provider.note" class="service-note">
      <span class="service-caption">备注</span>
      <p class="service-note-text">{{ provider.note }}</p>
    </section>
    <footer class="service-overview-actions">
      <button
        class="btn btn-primary service-overview-button"
        type="button"
        :disabled="busy"
        @click="$emit('edit')"
      >
        <SquarePen :size="15" />编辑配置
      </button>
      <span class="service-actions-spacer"></span>
      <button
        class="btn service-overview-button"
        type="button"
        :disabled="busy"
        @click="$emit('keys')"
      >
        <KeyRound :size="15" />管理 API Key
      </button>
      <button
        class="btn service-overview-button"
        type="button"
        :disabled="busy"
        @click="$emit('manage')"
      >
        <Bot :size="15" />管理 Agent
      </button>
    </footer>
  </div>
</template>

<script setup>
import { computed } from "vue"
import {
  Bot,
  Boxes,
  ChevronRight,
  KeyRound,
  Network,
  RefreshCw,
  Server,
  SquarePen
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import { formatDateTime, formatTokenCountParts } from "@/utils/formatters"

const props = defineProps({
  provider: { type: Object, required: true },
  models: { type: Array, default: () => [] },
  keys: { type: Array, default: () => [] },
  runningAgents: { type: Array, default: () => [] },
  busy: { type: Boolean, default: false }
})
defineEmits(["fetch", "edit", "keys", "manage"])

// 使用现有公开状态，只展示当前选中且未撤销的 Key 名称与指纹。
const activeKey = computed(() =>
  props.keys.find((key) => key.isActive && key.status === "active")
)
const usableKeyCount = computed(
  () => props.keys.filter((key) => key.status === "active").length
)
const enabledModelCount = computed(
  () => props.models.filter((model) => model.enabled).length
)
const protocolLabel = computed(() =>
  props.provider.upstreamProtocol === "chat" ? "Chat Completions" : "Responses"
)
const metrics = computed(() => [
  { label: "已启用模型", value: enabledModelCount.value },
  { label: "可用 API Key", value: usableKeyCount.value },
  { label: "正在使用的 Agent", value: props.runningAgents.length }
])
const connectionFields = computed(() => [
  { label: "基础地址", value: props.provider.baseUrl || "尚未配置" },
  { label: "上游协议", value: protocolLabel.value },
  { label: "网络代理", value: props.provider.proxy || "使用系统网络设置" },
  { label: "创建时间", value: formatDateTime(props.provider.createdAt) },
  { label: "配置更新时间", value: formatDateTime(props.provider.updatedAt) }
])
const priceColumns = [
  { key: "inputPriceMicros", label: "输入价格" },
  { key: "cachedInputPriceMicros", label: "缓存输入" },
  { key: "outputPriceMicros", label: "输出价格" }
]

// 存储价格为每百万 Tokens 的微货币单位，缺失价格与零价格分别展示。
function formatPrice(value, currency) {
  return value == null ? "未配置" : `${value / 1000000} ${currency || "USD"}`
}
</script>

<style scoped lang="less">
.service-overview {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
  .service-caption {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }
  .service-summary {
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);
    .service-identity {
      display: flex;
      align-items: center;
      gap: 14px;
      .service-avatar {
        display: flex;
        flex: none;
        width: 52px;
        height: 52px;
        align-items: center;
        justify-content: center;
        border: 1px solid var(--color-line);
        border-radius: 12px;
        background: var(--color-panel);
        color: var(--color-primary);
        .service-icon {
          width: 30px;
          height: 30px;
        }
      }
      .service-identity-text {
        display: flex;
        flex: 1;
        flex-direction: column;
        min-width: 0;
        gap: 3px;
        .service-name {
          margin: 0;
          font-size: var(--font-size-lg);
          overflow-wrap: anywhere;
        }
        .service-endpoint {
          font-size: var(--font-size-sm);
          color: var(--color-text-muted);
          overflow-wrap: anywhere;
        }
      }
      .service-protocol {
        flex: none;
        padding: 5px 10px;
        border: 1px solid var(--color-line-strong);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-primary);
        font-size: var(--font-size-sm);
      }
    }
    .service-status-row {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-top: 14px;
      .service-badge {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 3px 8px;
        border-radius: 4px;
        background: var(--color-success-soft);
        color: var(--color-success);
        font-size: var(--font-size-xs);
        .service-status-dot {
          width: 5px;
          height: 5px;
          border-radius: 50%;
          background: currentColor;
        }
        &.service-badge-muted {
          color: var(--color-text-muted);
          background: var(--color-panel);
        }
        &.service-badge-warning {
          color: var(--color-warning);
          background: var(--color-warning-soft);
        }
      }
    }
    .service-metrics {
      display: flex;
      margin-top: 16px;
      padding-top: 14px;
      border-top: 1px solid var(--color-line);
      .service-metric {
        display: flex;
        flex: 1;
        align-items: baseline;
        gap: 10px;
        padding: 0 18px;
        border-right: 1px solid var(--color-line);
        &:first-child {
          padding-left: 0;
        }
        &:last-child {
          border-right: 0;
        }
        .service-metric-value {
          color: var(--color-primary);
          font-size: 24px;
          line-height: 1.3;
          font-variant-numeric: tabular-nums;
        }
      }
    }
  }
  .service-info-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
  }
  .service-section {
    min-width: 0;
    padding: 18px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    .service-section-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      margin-bottom: 16px;
      .service-heading-text {
        display: flex;
        flex-direction: column;
        gap: 5px;
      }
      .service-title {
        display: flex;
        align-items: center;
        gap: 7px;
        margin: 0;
        font-size: var(--font-size-base);
        .service-model-count {
          margin-left: 3px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
        }
      }
      .service-text-button {
        display: inline-flex;
        align-items: center;
        padding: 0;
        border: 0;
        background: transparent;
        color: var(--color-primary);
        font: inherit;
        font-size: var(--font-size-xs);
        cursor: pointer;
        &:disabled {
          opacity: 0.5;
          cursor: default;
        }
      }
    }
    .service-fields {
      display: flex;
      flex-direction: column;
      gap: 14px;
      margin: 0;
      .service-field {
        display: flex;
        align-items: baseline;
        gap: 12px;
        .service-field-label {
          flex: none;
          width: 100px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
        }
        .service-field-value {
          flex: 1;
          min-width: 0;
          margin: 0;
          font-size: var(--font-size-sm);
          overflow-wrap: anywhere;
          &.service-agent-list {
            display: flex;
            flex-wrap: wrap;
            gap: 6px;
            .service-agent-tag {
              display: inline-flex;
              align-items: center;
              gap: 5px;
              padding: 3px 7px;
              border-radius: 4px;
              background: var(--color-primary-soft);
              color: var(--color-primary);
              font-size: var(--font-size-xs);
            }
          }
        }
      }
    }
    .service-key-hint {
      margin: 14px 0 0;
      color: var(--color-warning);
      font-size: var(--font-size-xs);
    }
    .service-model-scroll {
      max-height: 300px;
      overflow: auto;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      .service-model-table {
        width: 100%;
        border-collapse: separate;
        border-spacing: 0;
        table-layout: fixed;
        font-size: var(--font-size-sm);
        .service-model-head,
        .service-model-body {
          .service-model-row {
            .service-model-cell {
              padding: 11px 12px;
              text-align: left;
              vertical-align: top;
              overflow-wrap: anywhere;
              font-variant-numeric: tabular-nums;
              &.service-model-identity {
                width: 31%;
                .service-model-name,
                .service-model-id {
                  display: block;
                }
                .service-model-id {
                  margin-top: 4px;
                  color: var(--color-text-muted);
                  font-size: var(--font-size-xs);
                }
              }
              &.service-model-state {
                width: 76px;
                .service-model-status {
                  color: var(--color-success);
                  font-size: var(--font-size-xs);
                  &.service-model-disabled {
                    color: var(--color-text-muted);
                  }
                }
              }
              &.service-model-unset {
                color: var(--color-text-muted);
              }
            }
          }
        }
        .service-model-head {
          .service-model-row {
            .service-model-cell {
              position: sticky;
              top: 0;
              z-index: 1;
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
              font-size: var(--font-size-xs);
            }
          }
        }
        .service-model-body {
          .service-model-row {
            .service-model-cell {
              border-top: 1px solid var(--color-line);
            }
          }
        }
      }
    }
    .service-empty {
      display: flex;
      min-height: 120px;
      align-items: center;
      justify-content: center;
      gap: 14px;
      border: 1px dashed var(--color-line);
      border-radius: 8px;
      color: var(--color-text-muted);
      .service-empty-text {
        display: flex;
        flex-direction: column;
        gap: 6px;
        font-size: var(--font-size-sm);
      }
    }
    .service-model-hint {
      margin: 12px 0 0;
      line-height: 1.7;
    }
  }
  .service-note {
    padding: 0 2px;
    .service-note-text {
      margin: 5px 0 0;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      font-size: var(--font-size-sm);
    }
  }
  .service-overview-actions {
    position: sticky;
    bottom: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 0 2px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
    .service-actions-spacer {
      flex: 1;
    }
  }
  .service-overview-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    flex: none;
    .service-spinning {
      animation: service-model-fetch-spin 1s linear infinite;
    }
  }
}
@keyframes service-model-fetch-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

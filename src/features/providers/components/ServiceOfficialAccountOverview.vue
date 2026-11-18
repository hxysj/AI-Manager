<template>
  <div class="official-overview">
    <section class="official-summary">
      <div class="official-identity">
        <div class="official-avatar">
          <AiIcon
            class="official-icon"
            :name="provider.icon || (isGoogle ? 'google.svg' : 'codex.svg')"
            :alt="accountType"
          />
        </div>
        <div class="official-identity-text">
          <span class="official-eyebrow">{{ accountType }}</span>
          <h3 class="official-email">{{ account?.email || displayName }}</h3>
          <span
            v-if="account?.email && displayName !== account.email"
            class="official-caption"
          >
            {{ displayName }}
          </span>
        </div>
        <span class="official-plan" :title="plan || '官方尚未返回套餐信息'">
          {{ planLabel }}
        </span>
      </div>
      <div class="official-status-row">
        <span
          class="official-status"
          :class="{ 'official-status-muted': !provider.enabled }"
        >
          <span class="official-status-dot"></span>
          {{ provider.enabled ? "已启用" : "已停用" }}
        </span>
        <span
          class="official-status"
          :class="{
            'official-status-warning':
              !provider.hasCredential || issue?.requiresReauth
          }"
        >
          <ShieldCheck :size="14" />
          {{
            issue?.requiresReauth
              ? "授权已失效"
              : provider.hasCredential
                ? "已配置凭据"
                : "待登录"
          }}
        </span>
        <span class="official-caption">使用官方模型列表</span>
      </div>
    </section>

    <section class="official-section">
      <div class="official-section-heading">
        <div class="official-heading-text">
          <h3 class="official-title">
            <ChartNoAxesColumn :size="16" />官方额度
          </h3>
          <span class="official-caption">
            {{
              metadata.updatedAt
                ? `更新于 ${displayTime(metadata.updatedAt)}`
                : "展示最近获取的官方额度"
            }}
          </span>
        </div>
        <button
          class="btn official-button"
          type="button"
          :disabled="
            busy ||
            refreshing ||
            !provider.enabled ||
            !provider.hasCredential ||
            issue?.requiresReauth
          "
          @click="$emit('refresh')"
        >
          <RefreshCw :size="14" :class="{ 'official-spinning': refreshing }" />
          {{ refreshing ? "刷新中…" : "刷新官方额度" }}
        </button>
      </div>
      <p
        v-if="
          issue ||
          metadata.message ||
          metadata.quotaWarning ||
          account?.refreshStatus === 'failed'
        "
        class="official-notice"
        role="status"
      >
        <Info :size="15" />
        <span>{{
          issue?.message ||
          metadata.message ||
          metadata.quotaWarning ||
          "上次凭据刷新失败，请重新登录后刷新额度。"
        }}</span>
      </p>
      <div
        v-if="quotaItems.length"
        class="official-quota-grid"
        :aria-busy="refreshing"
      >
        <article
          v-for="item in quotaItems"
          :key="item.id"
          class="official-quota-card"
          :class="{
            'official-quota-warning':
              item.remaining != null && item.remaining <= 30,
            'official-quota-danger':
              item.remaining != null && item.remaining <= 10
          }"
        >
          <span class="official-quota-name">{{ item.name }}</span>
          <div class="official-quota-value">
            <span class="official-quota-number">{{
              item.remaining == null ? "未知" : `${item.remaining}%`
            }}</span>
            <span class="official-caption">{{
              item.remaining == null ? "官方未返回数值" : "剩余额度"
            }}</span>
          </div>
          <div
            class="official-quota-track"
            :class="{
              'official-quota-unknown': item.remaining == null,
              'official-quota-refreshing': refreshing
            }"
            :aria-busy="refreshing"
            role="progressbar"
            :aria-label="`${item.name}剩余额度`"
            :aria-valuenow="item.remaining == null ? undefined : item.remaining"
            :aria-valuetext="
              item.remaining == null ? '额度未知' : `${item.remaining}%`
            "
            :aria-valuemin="0"
            :aria-valuemax="100"
          >
            <span
              class="official-quota-fill"
              :style="{ width: `${item.remaining ?? 0}%` }"
            ></span>
          </div>
          <span class="official-quota-reset"
            ><Clock3 :size="13" />{{ resetLabel(item.resetAt) }}</span
          >
        </article>
      </div>
      <div v-else class="official-empty">
        <ChartNoAxesColumn :size="24" />
        <div class="official-empty-text">
          <span>{{
            provider.hasCredential
              ? isGoogle
                ? "暂无共享额度数据"
                : "暂无官方额度数据"
              : "登录后查看官方额度"
          }}</span>
          <span class="official-caption">{{
            provider.hasCredential
              ? isGoogle
                ? "Google 尚未返回共享额度分组，可刷新后重试。"
                : "刷新后展示官方返回的额度和重置时间。"
              : "完成官方登录后，可查看账号信息和额度。"
          }}</span>
        </div>
      </div>
      <p class="official-footnote">
        {{
          isGoogle
            ? "同组模型共用额度，各周期分别计算。"
            : "各周期分别计算，以官方返回的额度为准。"
        }}
        官方额度与网关请求用量独立统计。
      </p>
    </section>

    <div class="official-info-grid">
      <section class="official-section">
        <div class="official-section-heading">
          <h3 class="official-title"><UserRound :size="16" />账号与授权</h3>
        </div>
        <dl class="official-fields">
          <div
            v-for="field in identityFields"
            :key="field.label"
            class="official-field"
          >
            <dt class="official-field-label">{{ field.label }}</dt>
            <dd class="official-field-value">{{ field.value }}</dd>
          </div>
        </dl>
        <p v-if="provider.hasCredential" class="official-footnote">
          访问令牌到期后会在请求时尝试自动刷新；刷新失败时需重新登录。
        </p>
      </section>
      <section class="official-section">
        <div class="official-section-heading">
          <h3 class="official-title"><Network :size="16" />连接与使用</h3>
        </div>
        <dl class="official-fields">
          <div class="official-field">
            <dt class="official-field-label">正在使用的 Agent</dt>
            <dd class="official-field-value official-agent-list">
              <span
                v-for="agent in runningAgents"
                :key="agent"
                class="official-agent-tag"
                ><Bot :size="13" />{{ agent }}</span
              >
              <span v-if="!runningAgents.length" class="official-caption"
                >暂无 Agent 使用</span
              >
            </dd>
          </div>
          <div
            v-for="field in connectionFields"
            :key="field.label"
            class="official-field"
          >
            <dt class="official-field-label">{{ field.label }}</dt>
            <dd class="official-field-value">{{ field.value }}</dd>
          </div>
        </dl>
      </section>
    </div>

    <section v-if="provider.note" class="official-note">
      <span class="official-caption">备注</span>
      <p class="official-note-text">{{ provider.note }}</p>
    </section>
    <footer class="official-actions">
      <button
        class="btn btn-primary official-button"
        type="button"
        :disabled="busy || loginDisabled"
        @click="$emit('login')"
      >
        <LogIn :size="15" />{{
          issue?.requiresReauth
            ? "重新授权"
            : isGoogle
              ? "Google 登录"
              : "官方登录"
        }}
      </button>
      <button
        v-if="!isGoogle"
        class="btn official-button"
        type="button"
        :disabled="busy"
        @click="$emit('import')"
      >
        <FileUp :size="15" />导入 auth.json
      </button>
      <span class="official-actions-spacer"></span>
      <button
        class="btn official-button"
        type="button"
        :disabled="busy"
        @click="$emit('manage')"
      >
        <Bot :size="15" />管理 Agent
      </button>
      <button
        class="btn official-button"
        type="button"
        :disabled="busy"
        @click="$emit('edit')"
      >
        <SquarePen :size="15" />编辑配置
      </button>
    </footer>
  </div>
</template>

<script setup>
import { computed } from "vue"
import {
  Bot,
  ChartNoAxesColumn,
  Clock3,
  FileUp,
  Info,
  LogIn,
  Network,
  RefreshCw,
  ShieldCheck,
  SquarePen,
  UserRound
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  provider: { type: Object, required: true },
  displayName: { type: String, default: "" },
  account: { type: Object, default: null },
  quotaWindows: { type: Array, default: () => [] },
  runningAgents: { type: Array, default: () => [] },
  busy: { type: Boolean, default: false },
  refreshing: { type: Boolean, default: false },
  issue: { type: Object, default: null },
  loginDisabled: { type: Boolean, default: false }
})
defineEmits(["refresh", "login", "import", "manage", "edit"])

const isGoogle = computed(() => props.provider.kind === "google_account")
const accountType = computed(() =>
  isGoogle.value ? "Google 官方账号" : "GPT / Codex 官方账号"
)
const metadata = computed(() => props.account?.metadata || {})
const plan = computed(() =>
  isGoogle.value ? metadata.value.plan : props.account?.quota?.plan_type
)
const planLabel = computed(
  () =>
    ({
      free: "免费版",
      plus: "Plus",
      pro: "Pro",
      team: "Team",
      business: "Business",
      enterprise: "Enterprise",
      "g1-pro-tier": "Google AI Pro",
      "g1-ultra-tier": "Google AI Ultra",
      "free-tier": "免费版",
      FREE: "免费版"
    })[plan.value] ||
    plan.value ||
    "套餐待获取"
)

// 只格式化已公开的账号资料；访问令牌过期不等于官方登录已失效。
const displayTime = (value) =>
  value && Number.isFinite(new Date(value).getTime())
    ? formatDateTime(value)
    : "未记录"
const identityFields = computed(() => [
  { label: "账号邮箱", value: props.account?.email || "官方未提供" },
  {
    label: isGoogle.value ? "Google 账号 ID" : "账号 ID",
    value:
      (isGoogle.value ? props.account?.subject : props.account?.accountId) ||
      "待登录后获取"
  },
  { label: "订阅套餐", value: planLabel.value },
  {
    label: "凭据状态",
    value: props.issue?.requiresReauth
      ? "授权已失效"
      : props.provider.hasCredential
        ? "已配置"
        : "待登录"
  },
  { label: "访问令牌到期", value: displayTime(props.account?.expiresAt) },
  ...(isGoogle.value
    ? [
        {
          label: "授权状态",
          value:
            {
              ready: "已就绪",
              setup_required: "待开通服务",
              reauth_required: "需重新登录",
              error: "最近刷新失败"
            }[metadata.value.status] || "尚未获取"
        }
      ]
    : [])
])
const connectionFields = computed(() => [
  { label: "网络代理", value: props.provider.proxy || "使用系统网络设置" },
  { label: "模型来源", value: "使用官方模型列表" },
  ...(isGoogle.value
    ? [
        {
          label: "Cloud Code 项目",
          value: metadata.value.project || "尚未获取"
        }
      ]
    : []),
  { label: "创建时间", value: displayTime(props.provider.createdAt) },
  { label: "配置更新时间", value: displayTime(props.provider.updatedAt) }
])

// Codex 复用列表的周期数据，Google 按官方共享额度分组展示，不生成可编辑模型目录。
const quotaItems = computed(() =>
  isGoogle.value
    ? (metadata.value.quotaGroups || []).flatMap((group, groupIndex) =>
        (group.buckets || []).map((bucket, index) => {
          const window = String(bucket.window || "")
            .replace(/^WINDOW_/i, "")
            .toLowerCase()
          const label =
            { "5h": "5 小时额度", weekly: "周额度" }[window] ||
            bucket.displayName ||
            bucket.window ||
            "额度"
          const remaining = bucket.remainingFraction
          return {
            id: `${groupIndex}-${index}`,
            name: `${group.displayName || `共享额度 ${groupIndex + 1}`} · ${label}`,
            remaining:
              typeof remaining === "number" && Number.isFinite(remaining)
                ? Math.round(Math.max(0, Math.min(1, remaining)) * 1000) / 10
                : null,
            resetAt: bucket.resetTime
          }
        })
      )
    : props.quotaWindows.map((window, index) => ({
        ...window,
        id: index,
        resetAt: window.resetAt ? window.resetAt * 1000 : null
      }))
)

function resetLabel(value) {
  if (!value || !Number.isFinite(new Date(value).getTime()))
    return "官方未提供重置时间"
  if (new Date(value).getTime() <= Date.now()) return "周期已结束，请刷新额度"
  return `${displayTime(value)} 重置`
}
</script>

<style scoped lang="less">
.official-overview {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
  .official-caption,
  .official-footnote {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }
  .official-footnote {
    margin: 12px 0 0;
    line-height: 1.7;
  }
  .official-summary {
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);
    .official-identity {
      display: flex;
      align-items: center;
      gap: 14px;
      .official-avatar {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 52px;
        height: 52px;
        flex: none;
        border: 1px solid var(--color-line);
        border-radius: 12px;
        background: var(--color-panel);
        .official-icon {
          width: 30px;
          height: 30px;
        }
      }
      .official-identity-text {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        gap: 3px;
        .official-eyebrow {
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
        }
        .official-email {
          margin: 0;
          font-size: var(--font-size-lg);
          overflow-wrap: anywhere;
        }
      }
      .official-plan {
        flex: none;
        max-width: 180px;
        padding: 5px 12px;
        border: 1px solid var(--color-line-strong);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-primary);
        font-size: var(--font-size-sm);
        overflow-wrap: anywhere;
      }
    }
    .official-status-row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 10px;
      margin-top: 16px;
      .official-status {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 3px 8px;
        border-radius: 4px;
        background: var(--color-success-soft);
        color: var(--color-success);
        font-size: var(--font-size-xs);
        .official-status-dot {
          width: 5px;
          height: 5px;
          border-radius: 50%;
          background: currentColor;
        }
        &.official-status-muted {
          color: var(--color-text-muted);
          background: var(--color-panel);
        }
        &.official-status-warning {
          color: var(--color-warning);
          background: var(--color-warning-soft);
        }
      }
    }
  }
  .official-section {
    min-width: 0;
    padding: 18px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    .official-section-heading {
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 12px;
      margin-bottom: 16px;
      .official-heading-text {
        display: flex;
        flex-direction: column;
        gap: 5px;
      }
      .official-title {
        display: flex;
        align-items: center;
        gap: 7px;
        margin: 0;
        font-size: var(--font-size-base);
      }
    }
    .official-notice {
      display: flex;
      align-items: flex-start;
      gap: 8px;
      margin: 0 0 14px;
      padding: 10px 12px;
      border-radius: 6px;
      background: var(--color-warning-soft);
      color: var(--color-warning);
      font-size: var(--font-size-sm);
      overflow-wrap: anywhere;
    }
    .official-quota-grid {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 12px;
      .official-quota-card {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 10px;
        padding: 16px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);
        .official-quota-name {
          font-size: var(--font-size-sm);
          overflow-wrap: anywhere;
        }
        .official-quota-value {
          display: flex;
          align-items: baseline;
          gap: 8px;
          .official-quota-number {
            color: var(--color-primary);
            font-size: 28px;
            line-height: 1.3;
            font-variant-numeric: tabular-nums;
          }
        }
        .official-quota-track {
          height: 6px;
          overflow: hidden;
          border-radius: 3px;
          background: var(--color-line);
          &.official-quota-refreshing {
            background-image: linear-gradient(
              100deg,
              transparent 20%,
              var(--color-primary) 50%,
              transparent 80%
            );
            background-size: 200% 100%;
            animation: official-quota-loading 1.2s linear infinite;
            .official-quota-fill {
              opacity: 0.45;
            }
          }
          .official-quota-fill {
            transition: width 0.45s ease;
            display: block;
            height: 100%;
            border-radius: inherit;
            background: var(--color-primary);
          }
          &.official-quota-unknown {
            background: repeating-linear-gradient(
              120deg,
              var(--color-line),
              var(--color-line) 4px,
              var(--color-panel) 4px,
              var(--color-panel) 8px
            );
          }
        }
        .official-quota-reset {
          display: flex;
          align-items: center;
          gap: 5px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
        }
        &.official-quota-warning {
          .official-quota-value {
            .official-quota-number {
              color: var(--color-warning);
            }
          }
          .official-quota-track {
            .official-quota-fill {
              background: var(--color-warning);
            }
          }
        }
        &.official-quota-danger {
          .official-quota-value {
            .official-quota-number {
              color: var(--color-danger);
            }
          }
          .official-quota-track {
            .official-quota-fill {
              background: var(--color-danger);
            }
          }
        }
      }
    }
    .official-empty {
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 14px;
      min-height: 120px;
      border: 1px dashed var(--color-line);
      border-radius: 8px;
      color: var(--color-text-muted);
      .official-empty-text {
        display: flex;
        flex-direction: column;
        gap: 6px;
        font-size: var(--font-size-sm);
      }
    }
    .official-fields {
      display: flex;
      flex-direction: column;
      gap: 14px;
      margin: 0;
      .official-field {
        display: flex;
        align-items: baseline;
        gap: 12px;
        .official-field-label {
          flex: none;
          width: 100px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
        }
        .official-field-value {
          flex: 1;
          min-width: 0;
          margin: 0;
          font-size: var(--font-size-sm);
          overflow-wrap: anywhere;
          &.official-agent-list {
            display: flex;
            flex-wrap: wrap;
            gap: 6px;
            .official-agent-tag {
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
  }
  .official-info-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
  }
  .official-note {
    padding: 0 2px;
    .official-note-text {
      margin: 5px 0 0;
      font-size: var(--font-size-sm);
      white-space: pre-wrap;
      overflow-wrap: anywhere;
    }
  }
  .official-actions {
    position: sticky;
    bottom: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 0 2px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
    .official-actions-spacer {
      flex: 1;
    }
  }
  .official-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    flex: none;
    .official-spinning {
      animation: official-refresh-spin 1s linear infinite;
    }
  }
}
@keyframes official-refresh-spin {
  to {
    transform: rotate(360deg);
  }
}
@keyframes official-quota-loading {
  from {
    background-position: 200% 0;
  }
  to {
    background-position: -200% 0;
  }
}
</style>

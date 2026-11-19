<template>
  <BaseModal
    class="gateway-modal"
    title="网关与 Agent 管理"
    @close="$emit('close')"
  >
    <div class="gateway-manager">
      <div class="gateway-navigation">
        <div class="gateway-tabs" role="tablist" aria-label="网关管理">
          <button
            v-for="item in tabs"
            :id="`gateway-tab-${item.id}`"
            :key="item.id"
            class="gateway-tab"
            :class="{ 'gateway-tab-active': tab === item.id }"
            type="button"
            role="tab"
            :aria-selected="tab === item.id"
            :aria-controls="`gateway-panel-${item.id}`"
            :tabindex="tab === item.id ? 0 : -1"
            @click="$emit('update:tab', item.id)"
            @keydown="moveTab($event, item.id)"
          >
            <component :is="item.icon" :size="16" />
            {{ item.label }}
            <span v-if="item.count" class="gateway-tab-count">{{
              item.count
            }}</span>
          </button>
        </div>
        <button
          class="btn gateway-refresh"
          type="button"
          :disabled="busy"
          @click="$emit('refresh')"
        >
          <RefreshCw :size="15" />刷新状态
        </button>
      </div>

      <section
        :id="`gateway-panel-${tab}`"
        class="gateway-panel"
        role="tabpanel"
        :aria-labelledby="`gateway-tab-${tab}`"
      >
        <template v-if="tab === 'agents'">
          <div v-if="loginState?.status" class="gateway-notice">
            <ShieldCheck :size="16" />
            <span class="notice-message"
              >官方登录：{{
                statusLabels[loginState.status] || loginState.status
              }}
              {{ loginState.message }}</span
            >
            <button
              v-if="loginState.status === 'pending'"
              class="btn"
              type="button"
              :disabled="busy"
              @click="$emit('cancel-login')"
            >
              取消登录
            </button>
          </div>
          <div class="panel-heading">
            <div class="panel-intro">
              <h3 class="panel-title">配置文件接管</h3>
              <p class="panel-description">
                查看配置同步状态，处理冲突或恢复接管前的文件。
              </p>
            </div>
            <span class="panel-count">{{ agentInstances.length }} 条记录</span>
          </div>
          <div v-if="agentInstances.length" class="instance-list">
            <article
              v-for="instance in agentInstances"
              :key="instance.id"
              class="instance-card"
            >
              <div class="instance-header">
                <span class="instance-icon">
                  <AiIcon
                    v-if="agentIcons[instance.agentType]"
                    :name="agentIcons[instance.agentType]"
                    :alt="agentLabel(instance.agentType)"
                    class="agent-icon"
                  />
                  <Bot v-else :size="20" />
                </span>
                <div class="instance-heading">
                  <span class="instance-name">{{
                    instance.name || agentLabel(instance.agentType)
                  }}</span>
                  <span class="instance-source"
                    >{{ agentLabel(instance.agentType) }} ·
                    {{ providerName(instance.providerId) }}</span
                  >
                </div>
                <span
                  class="status-badge"
                  :data-tone="
                    instance.status === 'active'
                      ? 'success'
                      : instance.status === 'error'
                        ? 'danger'
                        : 'muted'
                  "
                  >{{ statusLabels[instance.status] || instance.status }}</span
                >
                <span
                  class="status-badge"
                  :data-tone="
                    instance.syncStatus === 'conflict' ? 'warning' : 'muted'
                  "
                  >{{
                    (instance.syncStatus === "pending"
                      ? "待同步"
                      : statusLabels[instance.syncStatus]) ||
                    instance.syncStatus ||
                    "待同步"
                  }}</span
                >
              </div>
              <dl class="instance-fields">
                <div class="instance-field instance-path">
                  <dt class="field-label">配置位置</dt>
                  <dd class="field-value field-code">
                    {{ instance.configPath || "未记录" }}
                  </dd>
                </div>
                <div class="instance-field">
                  <dt class="field-label">最近同步</dt>
                  <dd class="field-value">
                    {{ formatDateTime(instance.lastSyncedAt) }}
                  </dd>
                </div>
                <div class="instance-field">
                  <dt class="field-label">客户端协议</dt>
                  <dd class="field-value">
                    {{
                      protocolLabels[instance.clientProtocol] ||
                      instance.clientProtocol ||
                      "未记录"
                    }}
                  </dd>
                </div>
              </dl>
              <p v-if="instance.lastError" class="instance-error">
                <CircleAlert :size="15" />{{ instance.lastError }}
              </p>
              <div
                v-for="file in conflictFiles(instance.id)"
                :key="file.id"
                class="file-conflict"
              >
                <CircleAlert :size="16" />
                <div class="conflict-description">
                  <span class="conflict-path">{{ file.filePath }}</span>
                  <span class="conflict-message">{{
                    file.errorMessage ||
                    "文件已被外部修改，请对照备份后重试恢复。"
                  }}</span>
                </div>
                <button
                  class="btn"
                  type="button"
                  :disabled="busy"
                  @click="$emit('show-backup', file.id)"
                >
                  <Eye :size="14" />查看加密备份
                </button>
              </div>
              <div class="instance-footer">
                <span class="instance-note">{{
                  instance.status === "detached"
                    ? "已结束接管，原始配置已恢复。"
                    : "解除接管时将校验文件并恢复备份。"
                }}</span>
                <button
                  class="btn"
                  type="button"
                  :disabled="busy || instance.status === 'detached'"
                  @click="$emit('detach', instance.id)"
                >
                  <RotateCcw :size="14" />
                  {{
                    instance.status === "detached"
                      ? "已恢复"
                      : instance.syncStatus === "conflict" ||
                          instance.status === "error"
                        ? "重试恢复"
                        : "解除接管"
                  }}
                </button>
              </div>
            </article>
          </div>
          <div v-else class="gateway-empty" role="status">
            <span class="empty-icon"><Bot :size="30" /></span>
            <h3 class="empty-title">尚无 Agent 接管记录</h3>
            <p class="empty-description">
              为已检测到的 Agent 选择使用来源，应用后可在这里管理配置与恢复。
            </p>
            <div v-if="agents.length" class="empty-actions">
              <button
                v-for="agent in agents"
                :key="agent.id"
                class="btn"
                type="button"
                :disabled="busy"
                @click="$emit('configure-agent', agent)"
              >
                <Settings2 :size="14" />配置 {{ agentLabel(agent.id) }}
              </button>
            </div>
          </div>
        </template>

        <template v-else-if="tab === 'usage'">
          <div class="usage-counters">
            <div
              v-for="item in counters"
              :key="item.label"
              class="usage-counter"
            >
              <span class="counter-label">{{ item.label }}</span>
              <span class="counter-value">{{ item.value ?? "未知" }}</span>
            </div>
          </div>
          <div class="usage-toolbar">
            <div class="usage-tabs" aria-label="统计内容">
              <button
                v-for="item in usageViews"
                :key="item.id"
                class="usage-tab"
                :class="{ 'usage-tab-active': usageView === item.id }"
                type="button"
                :aria-pressed="usageView === item.id"
                @click="usageView = item.id"
              >
                {{ item.label }}
              </button>
            </div>
            <label v-if="usageView === 'summary'" class="dimension-field">
              <span class="field-label">汇总维度</span>
              <span class="gateway-select">
                <select
                  v-model="usageDimension"
                  class="input"
                  aria-label="汇总维度"
                >
                  <option
                    v-for="item in usageDimensions"
                    :key="item.id"
                    :value="item.id"
                  >
                    {{ item.label }}
                  </option>
                </select>
                <ChevronDown class="select-arrow" :size="15" />
              </span>
            </label>
            <span v-else class="panel-count">最近 100 条请求</span>
          </div>
          <p class="usage-description">
            {{
              usageView === "summary"
                ? "汇总全部网关账本记录。缓存输入包含在输入 Token 中；不同币种分别展示。"
                : "请求到达共享网关时直接记账。上游未返回用量，或缺少价格时，对应数据保留为未知。"
            }}
          </p>
          <div v-if="usageRows.length" class="usage-table-wrap">
            <table class="usage-table">
              <thead class="table-head">
                <tr class="table-row">
                  <th
                    v-for="label in usageView === 'summary'
                      ? [
                          '分组',
                          '上游尝试 / 成功',
                          '输入 / 输出 / 缓存 Token',
                          '未知用量',
                          '估算费用 / 已计价次数'
                        ]
                      : [
                          '时间 / Agent',
                          '服务商 / Key',
                          '模型',
                          '结果',
                          '输入 / 输出 Token',
                          '估算费用'
                        ]"
                    :key="label"
                    class="table-cell"
                  >
                    {{ label }}
                  </th>
                </tr>
              </thead>
              <tbody v-if="usageView === 'summary'" class="table-body">
                <tr
                  v-for="item in usageRows"
                  :key="`${item.groupKey}:${item.currency}:${item.groupName}`"
                  class="table-row"
                >
                  <td class="table-cell">
                    {{
                      usageDimension === "byAgent"
                        ? agentLabel(item.groupName)
                        : item.groupName || item.groupKey
                    }}
                  </td>
                  <td class="table-cell table-number">
                    {{ item.attemptCount }} / {{ item.successCount }}
                  </td>
                  <td class="table-cell table-number">
                    {{ item.inputTokens ?? "未知" }} /
                    {{ item.outputTokens ?? "未知" }} /
                    {{ item.cachedInputTokens ?? "未知" }}
                  </td>
                  <td class="table-cell table-number">
                    {{ item.unknownUsageCount }}
                  </td>
                  <td class="table-cell table-number">
                    {{
                      item.estimatedCostMicros == null
                        ? "未知"
                        : `${item.estimatedCostMicros / 1000000} ${item.currency}`
                    }}
                    / {{ item.pricedCount }}
                  </td>
                </tr>
              </tbody>
              <tbody v-else class="table-body">
                <tr v-for="log in usageRows" :key="log.id" class="table-row">
                  <td class="table-cell">
                    <span class="cell-main">{{
                      formatDateTime(log.createdAt)
                    }}</span
                    ><span class="cell-secondary">{{
                      agentLabel(log.agentType)
                    }}</span>
                  </td>
                  <td class="table-cell">
                    <span class="cell-main">{{
                      log.providerName || "未路由"
                    }}</span
                    ><span class="cell-secondary">{{
                      log.credentialName || "—"
                    }}</span>
                  </td>
                  <td class="table-cell">
                    {{ log.upstreamModel || log.requestedModel || "—" }}
                    <span
                      v-if="
                        log.requestedReasoningEffort ||
                        log.sentReasoningEffort ||
                        log.sentReasoningBudget != null
                      "
                      class="cell-secondary"
                    >
                      思考
                      {{
                        reasoningEffortLabels[log.requestedReasoningEffort] ||
                        log.requestedReasoningEffort ||
                        "未指定"
                      }}
                      →
                      {{
                        log.sentReasoningBudget != null
                          ? `${log.sentReasoningBudget} tokens`
                          : reasoningEffortLabels[log.sentReasoningEffort] ||
                            log.sentReasoningEffort ||
                            "未发送"
                      }}
                    </span>
                  </td>
                  <td class="table-cell" :title="log.errorMessage">
                    <span
                      class="status-badge"
                      :data-tone="
                        log.state === 'completed'
                          ? 'success'
                          : ['failed', 'rejected'].includes(log.state)
                            ? 'danger'
                            : 'muted'
                      "
                      >{{ statusLabels[log.state] || log.state }}</span
                    ><span class="cell-secondary"
                      >HTTP {{ log.statusCode || "—" }}</span
                    >
                  </td>
                  <td class="table-cell table-number">
                    {{ log.inputTokens ?? "未知" }} /
                    {{ log.outputTokens ?? "未知" }}
                  </td>
                  <td class="table-cell table-number">
                    {{
                      log.estimatedCostMicros == null
                        ? "未知"
                        : `${log.estimatedCostMicros / 1000000} ${log.currency}`
                    }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else class="gateway-empty" role="status">
            <span class="empty-icon"><ChartNoAxesCombined :size="30" /></span>
            <h3 class="empty-title">
              {{
                usageView === "summary"
                  ? "暂无此维度的用量记录"
                  : "暂无网关请求"
              }}
            </h3>
            <p class="empty-description">
              通过共享网关调用模型后，可在这里查看请求与用量。
            </p>
          </div>
        </template>

        <template v-else-if="tab === 'diagnostics'">
          <div class="panel-heading">
            <div class="panel-intro">
              <h3 class="panel-title">迁移与兼容性</h3>
              <p class="panel-description">
                查看来源迁移结果和 Agent
                工具调用、工具结果及流式文本的兼容性测试。
              </p>
            </div>
          </div>
          <div class="source-toolbar">
            <label class="source-field">
              <span class="field-label">测试来源</span>
              <span class="gateway-select">
                <select
                  v-model="selectedProviderId"
                  class="input"
                  :disabled="busy"
                >
                  <option value="" disabled>选择服务商或官方账号</option>
                  <optgroup
                    v-for="group in sourceGroups"
                    :key="group.kind"
                    :label="group.label"
                  >
                    <option
                      v-for="provider in group.providers"
                      :key="provider.id"
                      :value="provider.id"
                    >
                      {{ formatServiceProviderName(provider, state) }}
                    </option>
                  </optgroup>
                </select>
                <ChevronDown class="select-arrow" :size="16" />
              </span>
            </label>
            <button
              class="btn btn-primary"
              type="button"
              :disabled="!sourceReady"
              @click="$emit('open-test', selectedProvider)"
            >
              <Settings2 :size="15" />兼容性测试
            </button>
          </div>
          <p
            v-if="
              !selectedProvider?.enabled || !selectedProvider?.hasCredential
            "
            class="gateway-notice"
          >
            <Info :size="15" />
            {{
              !selectedProvider
                ? "请先选择可用的服务商或官方账号。"
                : !selectedProvider.enabled
                  ? "此来源已停用，请选择其他来源。"
                  : "此来源尚无可用凭据，请先配置 API Key 或完成官方登录。"
            }}
          </p>
          <details
            v-for="report in reports"
            :key="report.id"
            class="diagnostic-report"
            open
          >
            <summary class="report-summary">
              <ChevronRight class="report-arrow" :size="16" />
              <component :is="report.icon" :size="17" />
              <span class="report-title">{{ report.label }}</span>
              <span class="panel-count">{{ report.items.length }} 项</span>
            </summary>
            <div v-if="report.items.length" class="report-list">
              <article
                v-for="item in report.items"
                :key="
                  report.id === 'migration'
                    ? `${item.sourceTable}:${item.sourceId}`
                    : item.id
                "
                class="report-row"
              >
                <div class="report-description">
                  <span class="report-name">{{
                    report.id === "migration"
                      ? item.targetId
                        ? providerName(item.targetId)
                        : item.sourceId
                      : `${providerName(item.providerId)} · ${agentLabel(item.agentType)}`
                  }}</span>
                  <span class="report-detail">{{
                    report.id === "migration"
                      ? `${item.sourceTable} · ${item.sourceId}${item.reason ? ` · ${item.reason}` : ""}`
                      : `配置修订 ${item.providerRevision} · ${formatDateTime(item.testedAt)}`
                  }}</span>
                </div>
                <span
                  class="status-badge"
                  :data-tone="
                    ['copied', 'passed', 'verified', 'success'].includes(
                      item.status
                    )
                      ? 'success'
                      : ['failed', 'unreadable'].includes(item.status)
                        ? 'danger'
                        : 'muted'
                  "
                  >{{ statusLabels[item.status] || item.status }}</span
                >
              </article>
            </div>
            <div v-else class="report-empty" role="status">
              <component :is="report.icon" :size="24" />
              <span>{{
                report.id === "migration"
                  ? "暂无迁移记录"
                  : "暂无兼容性测试记录"
              }}</span>
            </div>
          </details>
        </template>
      </section>

      <footer class="gateway-footer">
        <Info :size="14" />
        <span>{{ tabs.find((item) => item.id === tab)?.hint }}</span>
      </footer>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  Bot,
  ChartNoAxesCombined,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  ClipboardList,
  Eye,
  Info,
  RefreshCw,
  RotateCcw,
  Settings2,
  ShieldCheck
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import BaseModal from "@/components/BaseModal.vue"
import { reasoningEffortLabels } from "@/constants/reasoning-efforts"
import { formatDateTime, formatServiceProviderName } from "@/utils/formatters"

const props = defineProps({
  state: { type: Object, required: true },
  instances: { type: Array, default: () => [] },
  agents: { type: Array, default: () => [] },
  providerId: { type: String, default: "" },
  tab: { type: String, default: "agents" },
  busy: { type: Boolean, default: false },
  loginState: { type: Object, default: null }
})
const emit = defineEmits([
  "close",
  "refresh",
  "update:providerId",
  "update:tab",
  "open-test",
  "configure-agent",
  "detach",
  "show-backup",
  "cancel-login"
])
const agentIcons = {
  codex: "codex.svg",
  claude: "claudecode.svg",
  "claude-desktop": "claude.svg"
}
const agentLabels = {
  codex: "Codex",
  claude: "Claude Code",
  "claude-desktop": "Claude Desktop",
  gateway: "外部调用"
}
const protocolLabels = {
  responses: "Responses",
  messages: "Messages",
  chat: "Chat Completions",
  all: "多协议"
}
const statusLabels = {
  active: "接管中",
  detached: "已解除",
  error: "异常",
  pending: "处理中",
  synced: "已同步",
  conflict: "配置冲突",
  restored: "已恢复",
  completed: "成功",
  failed: "失败",
  rejected: "已拒绝",
  cancelled: "已取消",
  unknown: "未确认",
  copied: "已同步",
  skipped: "已跳过",
  unreadable: "无法读取",
  passed: "通过",
  verified: "通过",
  success: "成功",
  unsupported: "不支持",
  running: "进行中"
}
const selectedProviderId = computed({
  get: () => props.providerId,
  set: (value) => emit("update:providerId", value)
})
const selectedProvider = computed(() =>
  props.state.providers.find((item) => item.id === props.providerId)
)
const sourceReady = computed(
  () =>
    !props.busy &&
    selectedProvider.value?.enabled &&
    selectedProvider.value?.hasCredential
)
const sourceGroups = computed(() =>
  [
    { kind: "api", label: "API 服务商" },
    { kind: "codex_account", label: "GPT / Codex 官方账号" },
    { kind: "google_account", label: "Google 官方账号" }
  ]
    .map((group) => ({
      ...group,
      providers: props.state.providers.filter(
        (item) => item.enabled && item.kind === group.kind
      )
    }))
    .filter((group) => group.providers.length)
)
// 管理界面只展示本机 Agent；多协议接入继续由原有工具负责。
const agentInstances = computed(() =>
  props.instances.filter((item) => item.agentType !== "gateway")
)
const tabs = computed(() => [
  {
    id: "agents",
    label: "Agent 接管",
    icon: Bot,
    count: agentInstances.value.length,
    hint: "每个 Agent 同时使用一个来源，解除接管时按备份恢复配置。"
  },
  {
    id: "usage",
    label: "调用统计",
    icon: ChartNoAxesCombined,
    hint: "仅统计共享网关收到的请求，官方账号额度与此处调用量分别记录。"
  },
  {
    id: "diagnostics",
    label: "诊断记录",
    icon: ClipboardList,
    hint: "测试记录对应当时的服务商修订，修改配置后可重新执行兼容性测试。"
  }
])
const usageView = ref("summary")
const usageViews = [
  { id: "summary", label: "汇总明细" },
  { id: "requests", label: "最近请求" }
]
const usageDimension = ref("byProvider")
const usageDimensions = [
  { id: "byProvider", label: "服务商" },
  { id: "byAgent", label: "Agent" },
  { id: "byModel", label: "模型" },
  { id: "byKey", label: "上游 Key / 官方凭据版本" },
  { id: "byToken", label: "调用 Token" },
  { id: "byDay", label: "日期（本地时区）" }
]
const usageRows = computed(
  () =>
    props.state.usage?.[
      usageView.value === "requests" ? "logs" : usageDimension.value
    ] || []
)
const counters = computed(() => [
  { label: "入站请求", value: props.state.usage?.requests?.requestCount },
  { label: "成功", value: props.state.usage?.requests?.successCount ?? 0 },
  {
    label: "失败 / 拒绝",
    value: props.state.usage?.requests?.failureCount ?? 0
  },
  {
    label: "未确认结果",
    value: props.state.usage?.requests?.unknownCount ?? 0
  },
  {
    label: "客户端取消",
    value: props.state.usage?.requests?.cancelledCount ?? 0
  },
  { label: "上游尝试", value: props.state.usage?.attempts?.attemptCount },
  { label: "已确认用量", value: props.state.usage?.attempts?.measuredCount }
])
const reports = computed(() => [
  {
    id: "migration",
    label: "最近迁移报告",
    icon: ClipboardList,
    items: props.state.migrationItems || []
  },
  {
    id: "compatibility",
    label: "兼容性测试记录",
    icon: ShieldCheck,
    items: props.state.compatibility || []
  }
])
const agentLabel = (id) => agentLabels[id] || id || "未鉴权"
const providerName = (id) => {
  const provider = props.state.providers.find((item) => item.id === id)
  return provider
    ? formatServiceProviderName(provider, props.state)
    : id || "未关联来源"
}
const conflictFiles = (id) =>
  props.state.managedFiles.filter(
    (item) => item.instanceId === id && item.restoreStatus === "conflict"
  )

// 页签支持方向键切换，焦点跟随选中的页签。
function moveTab(event, id) {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return
  event.preventDefault()
  const index = tabs.value.findIndex((item) => item.id === id)
  const nextIndex =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? tabs.value.length - 1
        : (index + (event.key === "ArrowRight" ? 1 : -1) + tabs.value.length) %
          tabs.value.length
  emit("update:tab", tabs.value[nextIndex].id)
  event.currentTarget.parentElement
    .querySelectorAll('[role="tab"]')
    [nextIndex].focus()
}
</script>

<style scoped lang="less">
.gateway-modal {
  :deep(.base-modal__panel) {
    width: 1120px;
    max-width: calc(100vw - 48px);
    height: min(800px, calc(100vh - 48px));
    border-radius: 14px;
  }
  :deep(.base-modal__header) {
    align-items: center;
    padding: 10px 20px;
  }
  :deep(.base-modal__header h2) {
    font-size: var(--font-size-lg);
    line-height: 1.5;
  }
  :deep(.base-modal__close) {
    width: 30px;
    height: 30px;
  }
  :deep(.base-modal__content) {
    padding: 0;
  }
  .gateway-manager {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    .btn {
      display: inline-flex;
      flex: none;
      align-items: center;
      justify-content: center;
      gap: 7px;
      min-height: 34px;
      padding: 7px 12px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;
      font-size: 13px;
      white-space: nowrap;
      cursor: pointer;
      &:hover:not(:disabled) {
        border-color: var(--color-primary);
        color: var(--color-primary);
      }
      &:disabled {
        opacity: 0.5;
        cursor: default;
      }
      &.btn-primary {
        border-color: var(--color-primary-solid);
        background: var(--color-primary-solid);
        color: white;
      }
    }
    .input {
      min-height: 38px;
      padding: 8px 10px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;
      font-size: var(--font-size-sm);
      &:disabled {
        opacity: 0.5;
      }
    }
    .gateway-navigation {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      padding: 0 20px;
      border-bottom: 1px solid var(--color-line);
      .gateway-refresh {
        margin-bottom: 6px;
      }
      .gateway-tabs {
        display: flex;
        flex: none;
        gap: 22px;
        .gateway-tab {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 8px;
          padding: 12px 2px;
          border: 0;
          border-bottom: 2px solid transparent;
          background: transparent;
          color: var(--color-text-muted);
          font: inherit;
          cursor: pointer;
          &.gateway-tab-active {
            border-bottom-color: var(--color-primary);
            color: var(--color-primary);
          }
          .gateway-tab-count {
            min-width: 20px;
            padding: 0 5px;
            border-radius: 5px;
            background: var(--color-panel-soft);
            font-size: var(--font-size-xs);
            text-align: center;
          }
        }
      }
    }
    .gateway-panel {
      display: flex;
      flex: 1;
      min-height: 0;
      flex-direction: column;
      gap: 16px;
      padding: 16px 20px;
      overflow: auto;
      scrollbar-gutter: stable;
      .source-toolbar {
        display: flex;
        flex: none;
        align-items: flex-end;
        justify-content: space-between;
        gap: 20px;
        .source-field {
          display: flex;
          width: 380px;
          flex-direction: column;
          gap: 8px;
        }
      }
      .field-label {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }
      .gateway-select {
        position: relative;
        display: flex;
        min-width: 0;
        .input {
          width: 100%;
          padding-right: 38px;
          appearance: none;
          text-overflow: ellipsis;
        }
        .select-arrow {
          position: absolute;
          top: 50%;
          right: 12px;
          color: var(--color-text-muted);
          pointer-events: none;
          transform: translateY(-50%);
        }
      }
      .gateway-notice {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;
        margin: 0;
        padding: 10px 12px;
        border: 1px solid var(--color-warning-line);
        border-radius: 8px;
        background: var(--color-warning-soft);
        color: var(--color-warning);
        font-size: var(--font-size-sm);
        .notice-message {
          flex: 1;
        }
      }
      .panel-heading {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        .panel-intro {
          min-width: 0;
          .panel-title {
            margin: 0;
            font-size: var(--font-size-base);
          }
          .panel-description {
            margin: 5px 0 0;
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
          }
        }
      }
      .panel-count {
        flex: none;
        color: var(--color-text-soft);
        font-size: var(--font-size-sm);
      }
      .status-badge {
        display: inline-flex;
        flex: none;
        align-items: center;
        width: fit-content;
        padding: 3px 8px;
        border: 1px solid var(--color-line);
        border-radius: 5px;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        white-space: nowrap;
        &[data-tone="success"] {
          border-color: var(--color-success-line);
          background: var(--color-success-soft);
          color: var(--color-success);
        }
        &[data-tone="warning"] {
          border-color: var(--color-warning-line);
          background: var(--color-warning-soft);
          color: var(--color-warning);
        }
        &[data-tone="danger"] {
          border-color: var(--color-danger-line);
          background: var(--color-danger-soft);
          color: var(--color-danger);
        }
      }
      .instance-list {
        display: flex;
        flex-direction: column;
        gap: 14px;
        .instance-card {
          border: 1px solid var(--color-line);
          border-radius: 10px;
          overflow: hidden;
          .instance-header {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 16px 18px 12px;
            .instance-icon {
              display: flex;
              width: 38px;
              height: 38px;
              flex: none;
              align-items: center;
              justify-content: center;
              border: 1px solid var(--color-line);
              border-radius: 9px;
              background: var(--color-panel-soft);
              color: var(--color-primary);
              .agent-icon {
                width: 24px;
                height: 24px;
              }
            }
            .instance-heading {
              display: flex;
              flex: 1;
              min-width: 0;
              flex-direction: column;
              gap: 3px;
              .instance-name {
                overflow-wrap: anywhere;
              }
              .instance-source {
                color: var(--color-text-muted);
                font-size: var(--font-size-sm);
                overflow-wrap: anywhere;
              }
            }
          }
          .instance-fields {
            display: flex;
            gap: 28px;
            margin: 0;
            padding: 0 18px 16px 66px;
            .instance-field {
              display: flex;
              min-width: 100px;
              flex-direction: column;
              gap: 5px;
              &.instance-path {
                flex: 1;
                min-width: 0;
              }
              .field-value {
                margin: 0;
                font-size: var(--font-size-sm);
                overflow-wrap: anywhere;
                &.field-code {
                  font-family: Consolas, monospace;
                }
              }
            }
          }
          .instance-error {
            display: flex;
            align-items: flex-start;
            gap: 8px;
            margin: 0;
            padding: 0 18px 14px;
            color: var(--color-danger);
            font-size: var(--font-size-sm);
            overflow-wrap: anywhere;
          }
          .file-conflict {
            display: flex;
            align-items: center;
            gap: 10px;
            margin: 0 18px 14px;
            padding: 12px;
            border: 1px solid var(--color-warning-line);
            border-radius: 8px;
            background: var(--color-warning-soft);
            color: var(--color-warning);
            .conflict-description {
              display: flex;
              flex: 1;
              min-width: 0;
              flex-direction: column;
              gap: 4px;
              .conflict-path {
                font-family: Consolas, monospace;
                font-size: var(--font-size-sm);
                overflow-wrap: anywhere;
              }
              .conflict-message {
                font-size: var(--font-size-xs);
                overflow-wrap: anywhere;
              }
            }
          }
          .instance-footer {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 16px;
            padding: 10px 18px;
            border-top: 1px solid var(--color-line);
            background: var(--color-panel-soft);
            .instance-note {
              color: var(--color-text-muted);
              font-size: var(--font-size-xs);
            }
          }
        }
      }
      .gateway-empty {
        display: flex;
        flex: 1;
        min-height: 200px;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 10px;
        padding: 24px;
        text-align: center;
        .empty-icon {
          display: flex;
          width: 64px;
          height: 64px;
          align-items: center;
          justify-content: center;
          margin-bottom: 4px;
          border: 1px solid var(--color-line);
          border-radius: 16px;
          background: var(--color-panel-soft);
          color: var(--color-text-soft);
        }
        .empty-title {
          margin: 0;
          font-size: var(--font-size-base);
        }
        .empty-description {
          max-width: 480px;
          margin: 0;
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }
        .empty-actions {
          display: flex;
          justify-content: center;
          gap: 8px;
          margin-top: 8px;
        }
      }
      .usage-counters {
        display: flex;
        flex: none;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background: var(--color-panel-soft);
        .usage-counter {
          display: flex;
          flex: 1;
          min-width: 0;
          flex-direction: column;
          gap: 8px;
          margin: 14px 0;
          padding: 0 16px;
          border-right: 1px solid var(--color-line);
          &:last-child {
            border-right: 0;
          }
          .counter-label {
            color: var(--color-text-muted);
            font-size: var(--font-size-xs);
          }
          .counter-value {
            font-size: 22px;
            font-variant-numeric: tabular-nums;
            overflow-wrap: anywhere;
          }
        }
      }
      .usage-toolbar {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        .usage-tabs {
          display: flex;
          gap: 4px;
          padding: 3px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel-soft);
          .usage-tab {
            padding: 6px 12px;
            border: 1px solid transparent;
            border-radius: 5px;
            background: transparent;
            color: var(--color-text-muted);
            font: inherit;
            font-size: var(--font-size-sm);
            cursor: pointer;
            &.usage-tab-active {
              border-color: var(--color-line);
              background: var(--color-panel);
              color: var(--color-primary);
            }
          }
        }
        .dimension-field {
          display: flex;
          align-items: center;
          gap: 10px;
          .gateway-select {
            width: 228px;
          }
        }
      }
      .usage-description {
        flex: none;
        margin: 0;
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }
      .usage-table-wrap {
        flex: none;
        overflow: auto;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        .usage-table {
          width: 100%;
          border-collapse: collapse;
          font-size: var(--font-size-sm);
          .table-head {
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
          }
          .table-row {
            .table-cell {
              padding: 12px 14px;
              border-bottom: 1px solid var(--color-line);
              text-align: left;
              white-space: nowrap;
              &.table-number {
                font-variant-numeric: tabular-nums;
              }
              .cell-main {
                display: block;
              }
              .cell-secondary {
                display: block;
                margin-top: 4px;
                color: var(--color-text-muted);
                font-size: var(--font-size-xs);
              }
            }
          }
          .table-body {
            .table-row:last-child {
              .table-cell {
                border-bottom: 0;
              }
            }
          }
        }
      }
      .diagnostic-report {
        flex: none;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        overflow: hidden;
        .report-summary {
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 14px 16px;
          background: var(--color-panel-soft);
          cursor: pointer;
          list-style: none;
          &::-webkit-details-marker {
            display: none;
          }
          .report-arrow {
            color: var(--color-text-soft);
            transition: transform 0.15s ease;
          }
          .report-title {
            flex: 1;
          }
        }
        &[open] {
          .report-summary {
            .report-arrow {
              transform: rotate(90deg);
            }
          }
        }
        .report-list {
          .report-row {
            display: flex;
            align-items: center;
            gap: 16px;
            padding: 14px 18px;
            border-top: 1px solid var(--color-line);
            .report-description {
              display: flex;
              flex: 1;
              min-width: 0;
              flex-direction: column;
              gap: 4px;
              .report-name {
                font-size: var(--font-size-sm);
                overflow-wrap: anywhere;
              }
              .report-detail {
                color: var(--color-text-muted);
                font-size: var(--font-size-xs);
                overflow-wrap: anywhere;
              }
            }
          }
        }
        .report-empty {
          display: flex;
          min-height: 132px;
          align-items: center;
          justify-content: center;
          flex-direction: column;
          gap: 10px;
          color: var(--color-text-soft);
          font-size: var(--font-size-sm);
        }
      }
    }
    .gateway-footer {
      display: flex;
      flex: none;
      align-items: center;
      gap: 8px;
      padding: 13px 28px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
  }
}
</style>

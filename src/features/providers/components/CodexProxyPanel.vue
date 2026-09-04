<template>
  <section
    v-if="mode === 'manage' || proxyState.enabled"
    :class="[
      'codex-proxy-panel',
      { 'codex-proxy-panel-manage': mode === 'manage' }
    ]"
  >
    <div v-if="mode !== 'manage'" class="codex-proxy-panel-summary">
      <div class="codex-proxy-panel-main">
        <div class="codex-proxy-panel-title-row">
          <span data-emphasis>
            {{
              proxyState.enabled
                ? `${cliName} 代理正在接管`
                : `${cliName} 代理未接管`
            }}
          </span>
          <span class="codex-proxy-panel-running">运行中</span>
        </div>
        <!-- <div class="codex-proxy-panel-meta">
          <span>
            {{
              proxyState.enabled
                ? `当前激活：${activeProxyProvider?.name || "未知 Provider"}`
                : firstProxyProvider
                  ? `开启后使用：${firstProxyProvider.name}`
                  : "Provider 需要手动加入接管池后才会参与代理转发"
            }}
          </span>
          <small>{{ proxyState.localBaseUrl }}</small>
        </div> -->
      </div>
      <div class="codex-proxy-panel-actions">
        <button
          class="codex-proxy-panel-pool-button"
          type="button"
          :disabled="pending"
          @click="showProviderPicker = true"
        >
          <Plus :size="16" />
          加入接管池
        </button>
      </div>
    </div>

    <div class="codex-proxy-panel-pool">
      <div class="codex-proxy-panel-pool-head">
        <div class="codex-proxy-panel-pool-title">
          <span>接管池</span>
          <!-- <small>已加入 {{ proxyProviders.length }} 个 Provider</small> -->
        </div>
        <div class="codex-proxy-panel-pool-actions">
          <span data-emphasis>
            {{
              activeProxyProvider
                ? `当前激活：${activeProxyProvider.name}`
                : "当前激活：未启用"
            }}
          </span>
          <button
            v-if="mode === 'manage'"
            class="codex-proxy-panel-pool-button"
            type="button"
            :disabled="pending"
            @click="showProviderPicker = true"
          >
            <Plus :size="16" />
            加入接管池
          </button>
        </div>
      </div>
      <template v-if="proxyProviders.length">
        <div class="codex-proxy-panel-pool-list">
          <article
            v-for="provider in proxyProviders"
            :key="provider.id"
            :class="[
              'codex-proxy-panel-pool-item',
              {
                'codex-proxy-panel-pool-item-active':
                  provider.id === proxyState.activeProviderId,
                'codex-proxy-panel-pool-item-disabled': provider.disabled
              }
            ]"
          >
            <span class="codex-proxy-panel-avatar">
              <AiIcon
                v-if="provider.icon"
                class="codex-proxy-panel-avatar-icon"
                :name="provider.icon"
                :alt="`${provider.name} 图标`"
              />
              <ShieldCheck v-else-if="provider.type === 'account'" :size="18" />
              <template v-else>{{ provider.name.slice(0, 1) }}</template>
            </span>
            <div class="codex-proxy-panel-provider">
              <span data-emphasis>{{ provider.name }}</span>
              <span>{{ provider.description }}</span>
            </div>
            <span
              :class="[
                'codex-proxy-panel-status',
                {
                  'codex-proxy-panel-status-active':
                    provider.id === proxyState.activeProviderId
                }
              ]"
            >
              {{
                provider.disabled
                  ? "已禁用"
                  : provider.id === proxyState.activeProviderId
                    ? "当前激活"
                    : "备用"
              }}
            </span>
            <button
              v-if="provider.disabled"
              class="codex-proxy-panel-activate"
              type="button"
              :disabled="pending"
              @click="restoreTarget(provider)"
            >
              恢复
            </button>
            <button
              v-else
              class="codex-proxy-panel-activate"
              type="button"
              :disabled="
                pending ||
                provider.disabled ||
                (proxyState.enabled &&
                  provider.id === proxyState.activeProviderId)
              "
              @click="activateTarget(provider)"
            >
              {{ formatActivateText(provider) }}
            </button>
            <button
              class="codex-proxy-panel-remove"
              type="button"
              :disabled="
                pending ||
                (proxyState.enabled &&
                  provider.id === proxyState.activeProviderId)
              "
              @click="removeTarget(provider)"
            >
              <X :size="15" />
            </button>
          </article>
        </div>
        <div
          v-if="hasAccountTarget"
          class="codex-proxy-panel-account-model"
        >
          <span>官方账号模型</span>
          <input
            v-model.trim="accountModelDraft"
            type="text"
            placeholder="例如 gpt-5"
            :disabled="pending"
          />
          <button
            type="button"
            :disabled="
              pending || accountModelDraft === proxyState.accountModel
            "
            @click="saveAccountModel"
          >
            保存
          </button>
        </div>
      </template>
      <div v-else class="codex-proxy-panel-empty">
        接管池为空，请先加入 Provider。
      </div>
    </div>

    <div v-if="mode !== 'manage'" class="codex-proxy-panel-logs">
      <div class="codex-proxy-panel-logs-head">
        <div>
          <span>请求日志</span>
          <small>
            显示 {{ filteredProxyLogs.length }} / {{ proxyLogs.length }} 条
          </small>
        </div>
        <div class="codex-proxy-panel-logs-filter">
          <button
            type="button"
            :class="{
              'codex-proxy-panel-logs-filter-active': proxyLogFilter === 'all'
            }"
            @click="proxyLogFilter = 'all'"
          >
            全部
          </button>
          <button
            type="button"
            :class="{
              'codex-proxy-panel-logs-filter-active':
                proxyLogFilter === 'success'
            }"
            @click="proxyLogFilter = 'success'"
          >
            成功
          </button>
          <button
            type="button"
            :class="{
              'codex-proxy-panel-logs-filter-active': proxyLogFilter === 'error'
            }"
            @click="proxyLogFilter = 'error'"
          >
            错误
          </button>
        </div>
      </div>
      <div v-if="filteredProxyLogs.length" class="codex-proxy-panel-logs-body">
        <div class="codex-proxy-panel-logs-list">
          <article
            v-for="(log, index) in pagedProxyLogs"
            :key="
              log.id ||
              `${proxyLogStartIndex + index}-${log.createdAt}-${log.providerId}`
            "
            :class="[
              'codex-proxy-panel-log-item',
              { 'codex-proxy-panel-log-item-error': !log.ok }
            ]"
            role="button"
            tabindex="0"
            @click="openProxyLogDetail(log)"
            @keydown.enter="openProxyLogDetail(log)"
            @keydown.space.prevent="openProxyLogDetail(log)"
          >
            <div
              :class="[
                'codex-proxy-panel-log-code',
                { 'codex-proxy-panel-log-code-ok': log.ok }
              ]"
            >
              <span>{{ log.ok ? "OK" : "ERR" }}</span>
              <span data-emphasis>{{ log.statusCode || "--" }}</span>
            </div>
            <div class="codex-proxy-panel-log-main">
              <div class="codex-proxy-panel-log-title">
                <span data-emphasis>{{ logProviderName(log) }}</span>
                <span>{{ formatTargetType(log.targetType) }}</span>
                <span>{{ formatProxyLogSource(log) }}</span>
              </div>
              <div class="codex-proxy-panel-log-meta">
                <span>{{ log.method || "POST" }}</span>
                <span>{{ log.endpoint }}</span>
                <span>{{ formatProxyLogTime(log.createdAt) }}</span>
                <span>{{ log.latencyMs || 0 }} ms</span>
              </div>
              <small v-if="log.errorMessage">{{ log.errorMessage }}</small>
            </div>
            <span class="codex-proxy-panel-log-status"> 详情 </span>
          </article>
        </div>
        <div class="codex-proxy-panel-logs-pager">
          <div>
            <span>
              {{ proxyLogStartIndex + 1 }}-{{ proxyLogEndIndex }} /
              {{ filteredProxyLogs.length }}
            </span>
            <select v-model.number="proxyLogPageSize">
              <option :value="20">20 条/页</option>
              <option :value="50">50 条/页</option>
              <option :value="100">100 条/页</option>
            </select>
          </div>
          <div>
            <button
              type="button"
              :disabled="proxyLogPage <= 1"
              @click="prevProxyLogPage"
            >
              上一页
            </button>
            <span data-emphasis>第 {{ proxyLogPage }} / {{ proxyLogPageCount }} 页</span>
            <button
              type="button"
              :disabled="proxyLogPage >= proxyLogPageCount"
              @click="nextProxyLogPage"
            >
              下一页
            </button>
          </div>
        </div>
      </div>
      <div v-else class="codex-proxy-panel-empty">
        {{ proxyLogs.length ? "当前筛选暂无请求日志。" : "暂无代理请求日志。" }}
      </div>
    </div>
  </section>

  <div
    v-if="selectedProxyLog"
    class="codex-proxy-panel-log-mask"
    @click="closeProxyLogDetail"
  ></div>
  <aside v-if="selectedProxyLog" class="codex-proxy-panel-log-drawer">
    <header class="codex-proxy-panel-log-drawer-head">
      <div>
        <span data-emphasis>请求详情</span>
        <span>{{ formatProxyLogTime(selectedProxyLog.createdAt) }}</span>
      </div>
      <button
        class="codex-proxy-panel-log-drawer-close"
        type="button"
        @click="closeProxyLogDetail"
      >
        <X :size="16" />
      </button>
    </header>
    <section class="codex-proxy-panel-log-detail">
      <div class="codex-proxy-panel-log-detail-row">
        <span>Provider</span>
        <span data-emphasis>{{ logProviderName(selectedProxyLog) }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>Provider ID</span>
        <span data-emphasis>{{ selectedProxyLog.providerId }}</span>
      </div>
      <div
        v-if="selectedProxyLog.instanceProviderId"
        class="codex-proxy-panel-log-detail-row"
      >
        <span>实例 Provider</span>
        <span data-emphasis>{{ instanceProviderName(selectedProxyLog) }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>请求来源</span>
        <span data-emphasis>{{ formatProxyLogSource(selectedProxyLog) }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>类型</span>
        <span data-emphasis>{{ selectedProxyLog.targetType }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>Endpoint</span>
        <span data-emphasis>{{ selectedProxyLog.endpoint }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>Method</span>
        <span data-emphasis>{{ selectedProxyLog.method }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>请求地址</span>
        <span data-emphasis>{{ selectedProxyLog.requestUrl }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>上游地址</span>
        <span data-emphasis>{{ selectedProxyLog.upstreamUrl }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>状态码</span>
        <span data-emphasis>{{ selectedProxyLog.statusCode || "无" }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>状态</span>
        <span data-emphasis>{{ selectedProxyLog.ok ? "成功" : "失败" }}</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>耗时</span>
        <span data-emphasis>{{ selectedProxyLog.latencyMs }} ms</span>
      </div>
      <div class="codex-proxy-panel-log-detail-row">
        <span>响应大小</span>
        <span data-emphasis>{{ selectedProxyLog.responseSize || 0 }} bytes</span>
      </div>
      <div
        v-if="selectedProxyLog.errorMessage"
        class="codex-proxy-panel-log-detail-block"
      >
        <span>错误信息</span>
        <pre>{{ selectedProxyLog.errorMessage }}</pre>
      </div>
      <div
        v-if="selectedProxyLog.upstreamResponseText"
        class="codex-proxy-panel-log-detail-block"
      >
        <span>上游响应</span>
        <pre>{{ selectedProxyLog.upstreamResponseText }}</pre>
      </div>
    </section>
  </aside>

  <BaseModal
    v-if="showProviderPicker"
    class="codex-proxy-panel-modal"
    title="加入接管池"
    :description="`选择 ${cliName} Provider 加入代理故障转移池。`"
    @close="showProviderPicker = false"
  >
    <section class="codex-proxy-panel-picker">
      <button
        v-for="provider in availableTargets"
        :key="provider.id"
        class="codex-proxy-panel-picker-item"
        type="button"
        :disabled="pending"
        @click="addTarget(provider)"
      >
        <span class="codex-proxy-panel-avatar">
          <AiIcon
            v-if="provider.icon"
            class="codex-proxy-panel-avatar-icon"
            :name="provider.icon"
            :alt="`${provider.name} 图标`"
          />
          <ShieldCheck v-else-if="provider.type === 'account'" :size="18" />
          <template v-else>{{ provider.name.slice(0, 1) }}</template>
        </span>
        <span class="codex-proxy-panel-picker-main">
          <span data-emphasis>{{ provider.name }}</span>
          <small>{{ provider.description }}</small>
        </span>
        <Plus :size="16" />
      </button>
      <div v-if="!availableTargets.length" class="codex-proxy-panel-empty">
        没有可加入的 Provider。
      </div>
    </section>
  </BaseModal>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import { Plus, ShieldCheck, X } from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({
  accounts: {
    type: Array,
    required: true
  },
  cliName: {
    type: String,
    default: "Codex"
  },
  includeAccounts: {
    type: Boolean,
    default: true
  },
  pending: {
    type: Boolean,
    required: true
  },
  mode: {
    type: String,
    default: "panel"
  },
  providers: {
    type: Array,
    required: true
  },
  proxyState: {
    type: Object,
    required: true
  }
})

const emit = defineEmits([
  "account-model-save",
  "add-provider",
  "remove-provider",
  "activate-provider",
  "restore-account",
  "restore-provider"
])

const showProviderPicker = ref(false)
const selectedProxyLog = ref(null)
const accountModelDraft = ref("")
const proxyLogFilter = ref("all")
const proxyLogPage = ref(1)
const proxyLogPageSize = ref(20)
const proxyLogTimeFormatter = new Intl.DateTimeFormat("zh-CN", {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false
})

const proxyProviderIds = computed(() => {
  return props.proxyState.failoverProviderIds || []
})

const targetItems = computed(() => {
  return [
    ...props.providers.map(item => ({
      id: item.id,
      type: "provider",
      provider: item,
      name: item.name,
      icon: item.icon,
      description: item.note || item.baseUrl || "未配置备注",
      disabled: item.enabled === false
    })),
    ...(props.includeAccounts
      ? props.accounts.map(item => ({
          id: `account:${item.id}`,
          accountId: item.id,
          account: item,
          type: "account",
          name: item.email || item.accountId || "Codex 官方账号",
          icon: "",
          description: `${formatPlanName(item.plan)} · 官方账号`,
          disabled: Boolean(item.disabled)
        }))
      : [])
  ]
})

const activeProxyProvider = computed(() => {
  return (
    targetItems.value.find(
      item => item.id === props.proxyState.activeProviderId
    ) || null
  )
})

const proxyProviders = computed(() => {
  return proxyProviderIds.value
    .map(providerId => targetItems.value.find(item => item.id === providerId))
    .filter(Boolean)
})

const hasAccountTarget = computed(() => {
  return proxyProviders.value.some(item => item.type === "account")
})

const availableTargets = computed(() => {
  return targetItems.value.filter(
    item => !item.disabled && !proxyProviderIds.value.includes(item.id)
  )
})

const firstProxyProvider = computed(() => {
  return proxyProviders.value[0] || null
})

const proxyLogs = computed(() => {
  return props.proxyState.logs || []
})

const filteredProxyLogs = computed(() => {
  if (proxyLogFilter.value === "success") {
    return proxyLogs.value.filter(item => item.ok)
  }

  if (proxyLogFilter.value === "error") {
    return proxyLogs.value.filter(item => !item.ok)
  }

  return proxyLogs.value
})

const proxyLogPageCount = computed(() =>
  Math.max(
    1,
    Math.ceil(filteredProxyLogs.value.length / proxyLogPageSize.value)
  )
)

const proxyLogStartIndex = computed(
  () => (proxyLogPage.value - 1) * proxyLogPageSize.value
)

const proxyLogEndIndex = computed(() =>
  Math.min(
    proxyLogStartIndex.value + proxyLogPageSize.value,
    filteredProxyLogs.value.length
  )
)

const pagedProxyLogs = computed(() =>
  filteredProxyLogs.value.slice(
    proxyLogStartIndex.value,
    proxyLogEndIndex.value
  )
)

watch(proxyLogFilter, () => {
  proxyLogPage.value = 1
})

watch(proxyLogPageSize, () => {
  proxyLogPage.value = 1
})

watch(
  () => filteredProxyLogs.value.length,
  () => {
    if (proxyLogPage.value > proxyLogPageCount.value) {
      proxyLogPage.value = proxyLogPageCount.value
    }
  }
)

watch(
  () => props.proxyState.accountModel,
  value => {
    accountModelDraft.value = value || ""
  },
  { immediate: true }
)

function addTarget(target) {
  showProviderPicker.value = false

  if (target.type === "account") {
    emit("add-provider", {
      accountId: target.accountId
    })
    return
  }

  emit("add-provider", {
    providerId: target.id
  })
}

function activateTarget(target) {
  if (target.type === "account") {
    emit("activate-provider", {
      accountId: target.accountId
    })
    return
  }

  emit("activate-provider", {
    providerId: target.id
  })
}

function removeTarget(target) {
  if (target.type === "account") {
    emit("remove-provider", {
      accountId: target.accountId
    })
    return
  }

  emit("remove-provider", {
    providerId: target.id
  })
}

function restoreTarget(target) {
  if (target.type === "account") {
    emit("restore-account", {
      accountId: target.accountId
    })
    return
  }

  emit("restore-provider", {
    ...target.provider,
    enabled: true
  })
}

function saveAccountModel() {
  emit("account-model-save", {
    accountModel: accountModelDraft.value
  })
}

function formatPlanName(plan) {
  return String(plan || "free").toUpperCase()
}

function formatProxyLogTime(value) {
  return proxyLogTimeFormatter.format(new Date(Number(value || 0)))
}

function formatTargetType(value) {
  return value === "account" ? "官方账号" : "Provider"
}

function formatProxyLogSource(log) {
  if (log.requestSource === "provider-instance" || log.instanceProviderId) {
    return `独立实例：${instanceProviderName(log)}`
  }

  return "代理接管"
}

function formatActivateText(provider) {
  if (provider.disabled) {
    return "禁用中"
  }

  if (provider.id !== props.proxyState.activeProviderId) {
    return "激活"
  }

  return props.proxyState.enabled ? "使用中" : "开启"
}

function logProviderName(log) {
  return (
    log.providerName ||
    targetItems.value.find(item => item.id === log.providerId)?.name ||
    log.providerId ||
    "未知目标"
  )
}

function instanceProviderName(log) {
  return (
    log.instanceProviderName ||
    targetItems.value.find(item => item.id === log.instanceProviderId)?.name ||
    log.instanceProviderId ||
    "未知 Provider"
  )
}

function openProxyLogDetail(log) {
  selectedProxyLog.value = log
}

function closeProxyLogDetail() {
  selectedProxyLog.value = null
}

function prevProxyLogPage() {
  proxyLogPage.value = Math.max(1, proxyLogPage.value - 1)
}

function nextProxyLogPage() {
  proxyLogPage.value = Math.min(proxyLogPageCount.value, proxyLogPage.value + 1)
}

function openProviderPicker() {
  showProviderPicker.value = true
}

defineExpose({
  openProviderPicker
})
</script>

<style scoped lang="less">
.codex-proxy-panel {
  display: flex;
  flex: none;
  flex-direction: column;
  gap: 10px;
  margin: 14px 14px 0;
  padding: 12px 16px 14px;
  border: 1px solid var(--color-line);
  border-radius: 12px;
  background: var(--color-panel-soft);
  box-shadow: 0 8px 22px rgba(15, 23, 42, 0.06);

  &-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  &-manage {
    margin: 0;
    box-shadow: none;
  }

  &-main,
  &-provider,
  &-picker-main {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 5px;
  }

  &-main {
    flex: 1;
    gap: 8px;
  }

  &-title-row,
  &-meta,
  &-pool-title {
    display: flex;
    min-width: 0;
    align-items: center;
  }

  &-title-row {
    gap: 8px;
  }

  &-meta {
    gap: 14px;
  }

  &-pool-title {
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
  }

  &-running {
    flex: none;
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--color-success-soft);
    color: var(--color-success);
    font-size: 12px;
  }

  &-main [data-emphasis],
  &-provider [data-emphasis],
  &-picker-main [data-emphasis] {
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.95rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &-main span:not([data-emphasis]),
  &-main small,
  &-pool-title small,
  &-provider span:not([data-emphasis]),
  &-picker-main small {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  &-pool-button {
    display: inline-flex;
    height: 36px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.84rem;
  }

  &-pool-button:hover {
    border-color: var(--color-info-line);
    color: var(--color-primary);
  }

  &-pool-button:disabled,
  &-activate:disabled,
  &-remove:disabled,
  &-picker-item:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  &-pool {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &-pool-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
  }

  &-pool-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
  }

  &-pool-head [data-emphasis] {
    overflow: hidden;
    color: var(--color-primary);
    font-size: 0.82rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &-pool-list {
    display: grid;
    max-height: 160px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    overflow-y: auto;
    padding-right: 4px;
  }

  &-pool-item,
  &-picker-item {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &-pool-item {
    height: 56px;
    padding: 0 10px;
  }

  &-pool-item-active {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
    box-shadow: inset 3px 0 0 #2f6fed;
  }

  &-pool-item-disabled {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    opacity: 0.72;
  }

  &-pool-item-disabled:hover {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    box-shadow: 0 6px 16px rgba(15, 23, 42, 0.08);
  }

  &-avatar {
    display: grid;
    width: 32px;
    height: 32px;
    flex: none;
    place-items: center;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-primary);
  }

  &-avatar-icon {
    width: 22px;
    height: 22px;
  }

  &-provider,
  &-picker-main {
    flex: 1;
  }

  &-account-model {
    display: flex;
    min-width: 0;
    flex: none;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &-account-model span:not([data-emphasis]) {
    flex: none;
    color: var(--color-text-muted);
    font-size: 12px;
  }

  &-account-model input {
    min-width: 0;
    flex: 1;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
    font-size: 12px;
  }

  &-account-model button {
    height: 28px;
    flex: none;
    padding: 0 8px;
    border: 1px solid var(--color-info-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 12px;
  }

  &-account-model button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  &-status {
    flex: none;
    min-width: 68px;
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 12px;
    text-align: center;
  }

  &-status-active {
    background: var(--color-primary-soft);
    color: var(--color-primary);
    text-align: center;
  }

  &-remove {
    display: grid;
    height: 28px;
    flex: none;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &-activate {
    display: inline-flex;
    height: 28px;
    min-width: 48px;
    flex: none;
    align-items: center;
    justify-content: center;
    padding: 0 10px;
    border: 1px solid var(--color-info-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 12px;
  }

  &-remove {
    width: 28px;
  }

  &-empty {
    display: flex;
    min-height: 54px;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 0.84rem;
  }

  &-picker {
    display: flex;
    max-height: 420px;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }

  &-picker-item {
    height: 58px;
    padding: 0 12px;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }

  &-logs {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
  }

  &-logs-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
  }

  &-logs-head div:first-child {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  &-logs-head span:not([data-emphasis]) {
    color: var(--color-text);
  }

  &-logs-head small {
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  &-logs-filter {
    display: flex;
    flex: none;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &-logs-filter button {
    display: inline-flex;
    height: 30px;
    align-items: center;
    justify-content: center;
    padding: 0 12px;
    border: 0;
    border-right: 1px solid var(--color-line);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.78rem;
  }

  &-logs-filter button:last-child {
    border-right: 0;
  }

  &-logs-filter button:hover,
  &-logs-filter-active {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &-logs-body {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
  }

  &-logs-list {
    display: flex;
    min-height: 0;
    max-height: 260px;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    padding-right: 4px;
  }

  &-logs-pager {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.78rem;
  }

  &-logs-pager div {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &-logs-pager select {
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    font-size: 0.78rem;
  }

  &-logs-pager button {
    display: inline-flex;
    height: 30px;
    align-items: center;
    justify-content: center;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.78rem;
  }

  &-logs-pager button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  &-logs-pager [data-emphasis] {
    color: var(--color-text);
    font-size: 0.78rem;
    white-space: nowrap;
  }

  &-log-item {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 12px;
    padding: 9px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    cursor: pointer;
    box-shadow: 0 1px 2px rgba(15, 23, 42, 0.04);
  }

  &-log-item-error {
    border-color: var(--color-danger-line);
    background: var(--color-panel);
  }

  &-log-item:hover {
    border-color: var(--color-info-line);
    background: var(--color-panel-soft);
  }

  &-log-code {
    display: flex;
    width: 54px;
    height: 46px;
    flex: none;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    border: 1px solid var(--color-danger-line);
    border-radius: 8px;
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &-log-code-ok {
    border-color: var(--color-success-line);
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  &-log-code span:not([data-emphasis]) {
    font-size: 10px;
    line-height: 1;
  }

  &-log-code [data-emphasis] {
    font-size: 15px;
    line-height: 1.1;
  }

  &-log-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 5px;
  }

  &-log-title,
  &-log-meta {
    display: flex;
    min-width: 0;
    align-items: center;
  }

  &-log-title {
    gap: 8px;
  }

  &-log-meta {
    gap: 10px;
  }

  &-log-title [data-emphasis],
  &-log-meta span:not([data-emphasis]),
  &-log-main small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &-log-title [data-emphasis] {
    min-width: 0;
    color: var(--color-text);
    font-size: 0.84rem;
  }

  &-log-title span:not([data-emphasis]) {
    flex: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 11px;
  }

  &-log-meta span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  &-log-meta span:not([data-emphasis]):first-child {
    flex: none;
    color: var(--color-text);
  }

  &-log-meta span:not([data-emphasis]):nth-child(2) {
    min-width: 90px;
    color: var(--color-primary);
  }

  &-log-main small {
    padding-top: 1px;
    color: var(--color-danger);
    font-size: 0.75rem;
  }

  &-log-status {
    flex: none;
    min-width: 42px;
    padding: 5px 8px;
    border-radius: 999px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 12px;
    text-align: center;
  }

  &-log-mask {
    position: fixed;
    inset: 0;
    z-index: 150;
    background: rgba(15, 23, 42, 0.28);
  }

  &-log-drawer {
    position: fixed;
    top: 0;
    right: 0;
    z-index: 151;
    display: flex;
    width: 460px;
    height: 100vh;
    flex-direction: column;
    border-left: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow: -18px 0 40px rgba(15, 23, 42, 0.16);
  }

  &-log-drawer-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--color-line);
  }

  &-log-drawer-head div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }

  &-log-drawer-head [data-emphasis] {
    color: var(--color-text);
    font-size: 1rem;
  }

  &-log-drawer-head span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: 0.78rem;
  }

  &-log-drawer-close {
    display: grid;
    width: 32px;
    height: 32px;
    flex: none;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &-log-detail {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    padding: 14px 18px 18px;
  }

  &-log-detail-row,
  &-log-detail-block {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &-log-detail-row span:not([data-emphasis]),
  &-log-detail-block span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  &-log-detail-row [data-emphasis] {
    overflow-wrap: anywhere;
    color: var(--color-text);
    font-size: 0.84rem;
  }

  &-log-detail-block pre {
    overflow: visible;
    margin: 0;
    color: var(--color-text);
    font-family: Consolas, "SFMono-Regular", monospace;
    font-size: 0.78rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
}
</style>

<template>
  <section
    :class="[
      'quick-switch-panel',
      { 'quick-switch-panel-collapsed': collapsed }
    ]"
  >
    <header class="quick-switch-panel-header" @dblclick="showMainPanel">
      <button
        v-if="collapsed"
        class="quick-switch-panel-logo-button"
        type="button"
        title="展开快速切换"
        @click="handleLogoClick"
        @pointerdown="startLogoDrag"
      >
        <svg
          class="quick-switch-panel-logo-scene"
          viewBox="0 0 44 44"
          role="img"
          aria-label="AI Manager"
        >
          <defs>
            <clipPath id="quick-switch-logo-clip">
              <circle cx="22" cy="21" r="14"></circle>
            </clipPath>
            <linearGradient
              id="quick-switch-logo-ring"
              x1="8"
              x2="36"
              y1="7"
              y2="36"
              gradientUnits="userSpaceOnUse"
            >
              <stop offset="0" stop-color="#4da3ff"></stop>
              <stop offset="0.52" stop-color="#18a058"></stop>
              <stop offset="1" stop-color="#ffb84d"></stop>
            </linearGradient>
          </defs>
          <ellipse
            class="quick-switch-panel-logo-shadow"
            cx="22"
            cy="36"
            rx="10"
            ry="3"
          ></ellipse>
          <g class="quick-switch-panel-logo-mascot">
            <circle
              class="quick-switch-panel-logo-orbit"
              cx="22"
              cy="21"
              r="17"
            ></circle>
            <image
              class="quick-switch-panel-logo-core"
              :href="logoUrl"
              x="8"
              y="7"
              width="28"
              height="28"
              clip-path="url(#quick-switch-logo-clip)"
              preserveAspectRatio="xMidYMid slice"
            ></image>
            <path
              class="quick-switch-panel-logo-scan"
              d="M10 22a12 12 0 0 1 24 0"
            ></path>
            <circle
              class="quick-switch-panel-logo-eye quick-switch-panel-logo-eye-left"
              cx="18"
              cy="20"
              r="1.5"
            ></circle>
            <circle
              class="quick-switch-panel-logo-eye quick-switch-panel-logo-eye-right"
              cx="26"
              cy="20"
              r="1.5"
            ></circle>
          </g>
          <g class="quick-switch-panel-logo-sparks">
            <circle cx="8" cy="15" r="1.3"></circle>
            <circle cx="35" cy="14" r="1.1"></circle>
            <circle cx="33" cy="31" r="1.4"></circle>
          </g>
        </svg>
      </button>
      <template v-else>
        <div class="quick-switch-panel-title">
          <span class="quick-switch-panel-dot"></span>
          <strong class="quick-switch-panel-title-name">
            {{ activeCli?.name || "未选择" }}
          </strong>
          <small class="quick-switch-panel-title-desc">{{ activeName }}</small>
        </div>
        <div class="quick-switch-panel-actions">
          <button
            class="quick-switch-panel-icon-button"
            type="button"
            title="打开主界面"
            @click="showMainPanel"
          >
            <ExternalLink :size="14" />
          </button>
          <button
            class="quick-switch-panel-icon-button"
            type="button"
            title="收起"
            @click="toggleCollapsed"
          >
            <ChevronDown :size="15" />
          </button>
        </div>
      </template>
    </header>

    <template v-if="!collapsed">
      <nav class="quick-switch-panel-cli-tabs">
        <button
          v-for="cli in cliTargets"
          :key="cli.id"
          :class="[
            'quick-switch-panel-cli-tab',
            {
              'quick-switch-panel-cli-tab-active': cli.id === activeCli?.id
            }
          ]"
          type="button"
          @click="selectedCli = cli.id"
        >
          {{ cli.name }}
        </button>
      </nav>

      <section v-if="mode === 'usage'" class="quick-switch-panel-usage">
        <div class="quick-switch-panel-hero">
          <div class="quick-switch-panel-hero-copy">
            <span class="quick-switch-panel-hero-label">当前用量</span>
            <strong class="quick-switch-panel-hero-name">
              {{ activeCli?.name || "未选择" }}
            </strong>
            <small class="quick-switch-panel-hero-desc">{{ activeName }}</small>
          </div>
          <button
            class="quick-switch-panel-manage-button"
            type="button"
            @click="mode = 'provider'"
          >
            管理
          </button>
        </div>

        <div class="quick-switch-panel-metrics">
          <article class="quick-switch-panel-metric">
            <span class="quick-switch-panel-metric-label">请求</span>
            <strong class="quick-switch-panel-metric-value">
              {{ formatQuickNumber(usageSummary.requestCount) }}
            </strong>
          </article>
          <article class="quick-switch-panel-metric">
            <span class="quick-switch-panel-metric-label">Token</span>
            <strong class="quick-switch-panel-metric-value">
              <TokenCount :value="usageSummary.actualTokens" />
            </strong>
          </article>
          <article class="quick-switch-panel-metric">
            <span class="quick-switch-panel-metric-label">费用</span>
            <strong class="quick-switch-panel-metric-value">
              {{ formatQuickCost(usageSummary.totalCostUsd) }}
            </strong>
          </article>
        </div>

        <div class="quick-switch-panel-summary-row">
          <section class="quick-switch-panel-usage-panel">
            <div class="quick-switch-panel-usage-head">
              <strong class="quick-switch-panel-usage-title">最近用量</strong>
              <span class="quick-switch-panel-usage-count">
                {{ usageTrend.length }} 天
              </span>
            </div>
            <div v-if="usageTrend.length" class="quick-switch-panel-bars">
              <div
                v-for="item in usageTrend"
                :key="item.date"
                class="quick-switch-panel-bar"
                :title="`${item.date} · ${formatTokenCount(item.actualTokens)} Token`"
              >
                <span
                  class="quick-switch-panel-bar-fill"
                  :style="{ height: `${item.percent}%` }"
                ></span>
                <small class="quick-switch-panel-bar-label">
                  {{ item.label }}
                </small>
              </div>
            </div>
            <div v-else class="quick-switch-panel-empty">暂无用量统计</div>
          </section>

          <section
            class="quick-switch-panel-usage-panel quick-switch-panel-provider-panel"
          >
            <div class="quick-switch-panel-usage-head">
              <strong class="quick-switch-panel-usage-title">Provider</strong>
              <span class="quick-switch-panel-usage-count">
                {{ usageProviders.length }} 个
              </span>
            </div>
            <div
              v-if="usageProviders.length"
              class="quick-switch-panel-provider-bars"
            >
              <article
                v-for="item in usageProviders"
                :key="item.providerId"
                class="quick-switch-panel-provider-bar"
              >
                <div class="quick-switch-panel-provider-bar-head">
                  <strong class="quick-switch-panel-provider-name">
                    {{ item.providerName }}
                  </strong>
                  <span class="quick-switch-panel-provider-cost">
                    {{ formatQuickCost(item.totalCostUsd) }}
                  </span>
                </div>
                <div class="quick-switch-panel-provider-track">
                  <span
                    class="quick-switch-panel-provider-fill"
                    :style="{ width: `${item.percent}%` }"
                  ></span>
                </div>
              </article>
            </div>
            <div v-else class="quick-switch-panel-empty">暂无 Provider</div>
          </section>
        </div>
      </section>

      <section v-else class="quick-switch-panel-list">
        <div class="quick-switch-panel-manager-head">
          <div class="quick-switch-panel-manager-copy">
            <strong class="quick-switch-panel-manager-title">
              Provider 管理
            </strong>
            <span class="quick-switch-panel-manager-desc">
              {{ activeCli?.name || "未选择" }} · {{ activeName }}
            </span>
          </div>
          <button
            class="quick-switch-panel-manage-button"
            type="button"
            @click="mode = 'usage'"
          >
            统计
          </button>
        </div>
        <article
          v-for="item in items"
          :key="item.key"
          :class="[
            'quick-switch-panel-item',
            {
              'quick-switch-panel-item-active': item.active,
              'quick-switch-panel-item-account': item.type === 'account'
            }
          ]"
        >
          <span class="quick-switch-panel-item-copy">
            <strong class="quick-switch-panel-item-title">
              {{ item.label }}
            </strong>
            <small class="quick-switch-panel-item-desc">
              {{ item.description }}
            </small>
            <span
              v-if="item.type === 'account' && item.quotas.length"
              class="quick-switch-panel-quota-list"
            >
              <span
                v-for="quota in item.quotas"
                :key="quota.key"
                class="quick-switch-panel-quota-item"
                :title="quota.reset"
              >
                <span class="quick-switch-panel-quota-label">
                  {{ quota.label }}
                </span>
                <strong class="quick-switch-panel-quota-value">
                  {{ quota.remaining }}%
                </strong>
              </span>
            </span>
          </span>
          <span class="quick-switch-panel-item-actions">
            <button
              v-if="item.type === 'account' && !item.disabled"
              class="quick-switch-panel-item-icon-button"
              type="button"
              title="刷新额度"
              aria-label="刷新额度"
              @click.stop="refreshCodexAccount(item)"
            >
              <RefreshCw :size="14" />
            </button>
            <button
              class="quick-switch-panel-item-action"
              type="button"
              :disabled="item.active || item.disabled"
              @click.stop="selectItem(item)"
            >
              启用
            </button>
            <button
              class="quick-switch-panel-item-action quick-switch-panel-item-danger-action"
              type="button"
              :disabled="!item.active"
              @click.stop="clearActive"
            >
              取消启用
            </button>
          </span>
        </article>

        <div v-if="!items.length" class="quick-switch-panel-empty">
          暂无可切换项
        </div>
      </section>
    </template>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"
import { ChevronDown, ExternalLink, RefreshCw } from "lucide-vue-next"
import { accountApi, appApi, proxyApi, runtimeApi } from "@/api"
import logoUrl from "@/assets/ai-manager-logo.svg?url"
import TokenCount from "@/components/TokenCount.vue"
import { useGlobalLoading } from "@/utils/global-loading"
import { formatTokenCount } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  state: {
    type: Object,
    required: true
  }
})

const emit = defineEmits(["state-updated"])
const quickParams = new URLSearchParams(window.location.search)

const mode = ref("usage")
const selectedCli = ref("")
const collapsed = ref(quickParams.get("collapsed") === "1")
const logoDrag = {
  active: false,
  moved: false,
  lastX: 0,
  lastY: 0,
  totalX: 0,
  totalY: 0
}
const { withGlobalLoading } = useGlobalLoading()

const cliTargets = computed(() => {
  return (props.state.cliTargets || []).filter(item => {
    return props.state.runtimeConfigSchemas?.[item.id]?.enabled
  })
})

const activeCli = computed(() => {
  return (
    cliTargets.value.find(item => item.id === selectedCli.value) ||
    cliTargets.value[0] ||
    null
  )
})

const activeProfile = computed(() => {
  return (
    (props.state.runtimeProfiles || []).find(
      item => item.cli === activeCli.value?.id
    ) || null
  )
})

const activeProxyState = computed(() => {
  if (activeCli.value?.id === "claude") {
    return props.state.claudeProxyState
  }

  if (activeCli.value?.id === "codex") {
    return props.state.codexProxyState
  }

  return null
})

const proxyActiveTargetId = computed(() => {
  if (!activeProxyState.value?.enabled) {
    return ""
  }

  return activeProxyState.value.activeProviderId || ""
})

const proxyActiveProvider = computed(() => {
  if (!proxyActiveTargetId.value.startsWith("account:")) {
    return (
      (props.state.providers || []).find(
        item => item.id === proxyActiveTargetId.value
      ) || null
    )
  }

  return null
})

const proxyActiveAccount = computed(() => {
  if (!proxyActiveTargetId.value.startsWith("account:")) {
    return null
  }

  const accountId = proxyActiveTargetId.value.slice("account:".length)

  return (
    (props.state.codexAccounts || []).find(item => item.id === accountId) ||
    null
  )
})

const activeProvider = computed(() => {
  if (proxyActiveTargetId.value) {
    return proxyActiveProvider.value
  }

  return (
    (props.state.providers || []).find(
      item => item.id === activeProfile.value?.providerId
    ) || null
  )
})

const activeAccount = computed(() => {
  if (activeCli.value?.id !== "codex") {
    return null
  }

  if (proxyActiveTargetId.value) {
    return proxyActiveAccount.value
  }

  return (
    (props.state.codexAccounts || []).find(item => item.active) || null
  )
})

const activeName = computed(() => {
  if (proxyActiveTargetId.value) {
    return `Proxy 接管中：${
      activeProvider.value?.name ||
      activeAccount.value?.email ||
      activeAccount.value?.accountId ||
      "未激活"
    }`
  }

  if (activeAccount.value) {
    return (
      activeAccount.value.email ||
      activeAccount.value.accountId ||
      "Codex 官方账号"
    )
  }

  return activeProvider.value?.name || "未启用"
})

const items = computed(() => {
  if (!activeCli.value) {
    return []
  }

  const providerItems = (props.state.providers || [])
    .filter(item => {
      return item.cli === activeCli.value.id && item.enabled !== false
    })
    .map(provider => {
      const model = firstModelName(provider)
      const active = proxyActiveTargetId.value
        ? proxyActiveTargetId.value === provider.id
        : !activeAccount.value && activeProvider.value?.id === provider.id

      return {
        key: `provider:${provider.id}`,
        type: "provider",
        provider,
        model,
        label: provider.name,
        description: model || "缺少模型",
        active,
        disabled: !model || provider.enabled === false
      }
    })

  if (activeCli.value.id !== "codex") {
    return providerItems
  }

  return [
    ...providerItems,
    ...(props.state.codexAccounts || []).map(account => ({
      key: `account:${account.id}`,
      type: "account",
      account,
      label: account.email || account.accountId || "Codex 官方账号",
      description: formatAccountDescription(account),
      quotas: formatAccountQuotas(account),
      active: proxyActiveTargetId.value
        ? proxyActiveTargetId.value === `account:${account.id}`
        : account.active,
      disabled: Boolean(account.disabled)
    }))
  ]
})

const activeUsageLogs = computed(() => {
  return (props.state.usage?.logs || []).filter(item => {
    return item.appType === activeCli.value?.id
  })
})

const usageSummary = computed(() => {
  return activeUsageLogs.value.reduce(
    (result, item) => {
      result.requestCount += 1
      result.actualTokens += Number(item.actualTokens || 0)
      result.totalCostUsd += Number(item.totalCostUsd || 0)
      return result
    },
    {
      requestCount: 0,
      actualTokens: 0,
      totalCostUsd: 0
    }
  )
})

const usageTrend = computed(() => {
  const groups = new Map()

  for (const item of activeUsageLogs.value) {
    const date = new Date(Number(item.createdAt || 0))
    const key = date.toLocaleDateString("zh-CN")

    groups.set(key, {
      date: key,
      label: `${date.getMonth() + 1}/${date.getDate()}`,
      timestamp: new Date(
        date.getFullYear(),
        date.getMonth(),
        date.getDate()
      ).getTime(),
      actualTokens:
        (groups.get(key)?.actualTokens || 0) + Number(item.actualTokens || 0)
    })
  }

  const rows = Array.from(groups.values())
    .sort((left, right) => left.timestamp - right.timestamp)
    .slice(-7)
  const maxTokens = Math.max(...rows.map(item => item.actualTokens), 1)

  return rows.map(item => ({
    ...item,
    percent: Math.max(8, Math.round((item.actualTokens / maxTokens) * 100))
  }))
})

const usageProviders = computed(() => {
  const groups = new Map()

  for (const item of activeUsageLogs.value) {
    const key = item.providerId || item.providerName || "unknown"
    const previous = groups.get(key) || {
      providerId: key,
      providerName: item.providerName || "未知 Provider",
      actualTokens: 0,
      totalCostUsd: 0
    }

    previous.actualTokens += Number(item.actualTokens || 0)
    previous.totalCostUsd += Number(item.totalCostUsd || 0)
    groups.set(key, previous)
  }

  const rows = Array.from(groups.values())
    .sort((left, right) => right.actualTokens - left.actualTokens)
    .slice(0, 2)
  const maxTokens = Math.max(...rows.map(item => item.actualTokens), 1)

  return rows.map(item => ({
    ...item,
    percent: Math.max(4, Math.round((item.actualTokens / maxTokens) * 100))
  }))
})

function formatQuickNumber(value) {
  return new Intl.NumberFormat("zh-CN", {
    maximumFractionDigits: 0
  }).format(Number(value || 0))
}

function formatQuickCost(value) {
  const cost = Number(value || 0)

  if (!cost) {
    return "$0"
  }

  return `$${cost >= 1 ? cost.toFixed(2) : cost.toFixed(6)}`
}

function ensureSelectedCli() {
  if (
    selectedCli.value &&
    cliTargets.value.find(item => item.id === selectedCli.value)
  ) {
    return
  }

  selectedCli.value = cliTargets.value[0]?.id || ""
}

function emitState(nextState) {
  if (nextState && typeof nextState === "object") {
    emit("state-updated", nextState)
  }
}

async function runAction(action) {
  return withGlobalLoading(async () => {
    try {
      emitState(await action())
      return true
    } catch (error) {
      createMessage.error(error.message || String(error))
      return false
    }
  })
}

function getProxyState(cli) {
  if (cli === "claude") {
    return props.state.claudeProxyState
  }

  if (cli === "codex") {
    return props.state.codexProxyState
  }

  return null
}

function getProxyApi(cli) {
  if (cli === "claude") {
    return {
      disable: proxyApi.disableClaudeProxy
    }
  }

  if (cli === "codex") {
    return {
      disable: proxyApi.disableCodexProxy
    }
  }

  return null
}

function firstModelName(provider) {
  return (
    provider.runtimeConfig?.mainModel ||
    (props.state.runtimeModels || []).find(
      item => item.providerId === provider.id
    )?.name ||
    ""
  )
}

function formatAccountDescription(account) {
  const rateLimit = account.usage?.rate_limit
  const primaryWindow = rateLimit?.primary_window
  const remaining = primaryWindow
    ? `${Math.max(0, 100 - Number(primaryWindow.used_percent || 0))}%`
    : "额度未知"

  return `${account.plan || "free"} · ${remaining}`
}

function formatAccountQuotas(account) {
  const rateLimit = account.usage?.rate_limit

  if (!rateLimit) {
    return []
  }

  return [
    { key: "primary", window: rateLimit.primary_window },
    { key: "secondary", window: rateLimit.secondary_window }
  ]
    .filter(item => item.window)
    .map(item => {
      return {
        key: item.key,
        label: formatRateWindowName(item.key, item.window),
        remaining: Math.max(0, 100 - Number(item.window.used_percent || 0)),
        reset: formatResetText(item.window.reset_at)
      }
    })
}

function formatRateWindowName(key, window) {
  const seconds = Number(window.limit_window_seconds || 0)

  if (seconds === 604800) {
    return key === "secondary" ? "7天额度" : "周额度"
  }

  if (seconds % 86400 === 0) {
    return `${seconds / 86400}天额度`
  }

  if (seconds % 3600 === 0) {
    return `${seconds / 3600}小时额度`
  }

  return `${seconds}秒额度`
}

function formatResetText(value) {
  const timestamp = Number(value || 0)

  if (!timestamp) {
    return "重置时间未知"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(timestamp > 1e12 ? timestamp : timestamp * 1000))
}

async function showMainPanel() {
  try {
    await appApi.showMainPanel()
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

async function toggleCollapsed() {
  collapsed.value = !collapsed.value

  try {
    await appApi.setQuickSwitchCollapsed({
      collapsed: collapsed.value
    })
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

function startLogoDrag(event) {
  if (event.button !== 0) {
    return
  }

  event.preventDefault()
  logoDrag.active = true
  logoDrag.moved = false
  logoDrag.lastX = event.screenX
  logoDrag.lastY = event.screenY
  logoDrag.totalX = 0
  logoDrag.totalY = 0
  window.addEventListener("pointermove", moveLogoDrag)
  window.addEventListener("pointerup", stopLogoDrag)
  window.addEventListener("pointercancel", stopLogoDrag)
}

function moveLogoDrag(event) {
  if (!logoDrag.active) {
    return
  }

  const x = event.screenX - logoDrag.lastX
  const y = event.screenY - logoDrag.lastY
  logoDrag.lastX = event.screenX
  logoDrag.lastY = event.screenY
  logoDrag.totalX += Math.abs(x)
  logoDrag.totalY += Math.abs(y)

  if (logoDrag.totalX + logoDrag.totalY > 3) {
    logoDrag.moved = true
  }

  if (x || y) {
    appApi.moveQuickSwitchBy({ x, y })
  }
}

function stopLogoDrag() {
  logoDrag.active = false
  window.removeEventListener("pointermove", moveLogoDrag)
  window.removeEventListener("pointerup", stopLogoDrag)
  window.removeEventListener("pointercancel", stopLogoDrag)
}

async function handleLogoClick() {
  if (logoDrag.moved) {
    return
  }

  await showMainPanel()
}

function isCodexAccountRefreshError(error) {
  return Boolean(error)
}

async function refreshCodexAccount(item) {
  await withGlobalLoading(async () => {
    try {
      emitState(
        await accountApi.refreshCodexAccount({
          accountId: item.account.id,
          syncAuth: false
        })
      )
    } catch (error) {
      if (!isCodexAccountRefreshError(error)) {
        createMessage.error(error.message || String(error))
      }
    }
  })
}

async function selectItem(item) {
  if (item.type === "provider") {
    await runAction(async () => {
      const currentProxyApi = getProxyApi(activeCli.value?.id)
      const proxyState = getProxyState(activeCli.value?.id)

      if (proxyState?.enabled) {
        await currentProxyApi.disable()
      }

      if (activeCli.value?.id === "codex") {
        await accountApi.clearCodexAccount()
      }

      return runtimeApi.switchRuntime({
        cli: activeCli.value.id,
        providerId: item.provider.id,
        model: item.model
      })
    })
    return
  }

  await runAction(async () => {
    const currentProxyApi = getProxyApi(activeCli.value?.id)
    const proxyState = getProxyState(activeCli.value?.id)

    if (proxyState?.enabled) {
      await currentProxyApi.disable()
    }

    await runtimeApi.clearRuntime({
      cli: activeCli.value.id
    })
    return accountApi.enableCodexAccount({
      accountId: item.account.id
    })
  })
}

async function clearActive() {
  await runAction(async () => {
    const currentProxyApi = getProxyApi(activeCli.value?.id)
    const proxyState = getProxyState(activeCli.value?.id)

    if (proxyState?.enabled) {
      return currentProxyApi.disable()
    }

    if (activeCli.value?.id === "codex") {
      await accountApi.clearCodexAccount()
    }

    return runtimeApi.clearRuntime({
      cli: activeCli.value.id
    })
  })
}

watch(cliTargets, ensureSelectedCli, { immediate: true })

onBeforeUnmount(() => {
  stopLogoDrag()
})
</script>

<style scoped lang="less">
.quick-switch-panel {
  display: flex;
  height: 100vh;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-info-line);
  background: var(--color-primary-soft);
  color: var(--color-text);

  :deep(.token-count) {
    font-size: inherit;
  }

  :deep(.token-count-exact) {
    font-size: 0.7em;
  }

  .quick-switch-panel-header {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 34px;
    padding: 0 8px 0 10px;
    border-bottom: 1px solid var(--color-line-strong);
    background: var(--color-panel-soft);
    -webkit-app-region: drag;
  }

  .quick-switch-panel-title {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 7px;

    .quick-switch-panel-title-name {
      flex: none;
      font-size: 13px;
      line-height: 1;
    }

    .quick-switch-panel-title-desc {
      overflow: hidden;
      min-width: 0;
      color: var(--color-text-muted);
      font-size: 12px;
      line-height: 1;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .quick-switch-panel-dot {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 999px;
    background: var(--color-success);
    box-shadow: 0 0 0 3px #e3f5ec;
  }

  .quick-switch-panel-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 4px;
    -webkit-app-region: no-drag;
  }

  .quick-switch-panel-icon-button {
    display: inline-flex;
    width: 26px;
    height: 26px;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--color-primary);
    cursor: pointer;
  }

  .quick-switch-panel-icon-button:hover {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
  }

  .quick-switch-panel-logo-button {
    display: inline-flex;
    width: 44px;
    height: 44px;
    align-items: center;
    justify-content: center;
    border: 0;
    background: transparent;
    cursor: grab;
    touch-action: none;
    -webkit-app-region: no-drag;
  }

  .quick-switch-panel-logo-button:active {
    cursor: grabbing;
  }

  .quick-switch-panel-logo-scene {
    width: 42px;
    height: 42px;
    overflow: visible;
  }

  .quick-switch-panel-logo-shadow {
    animation: quick-switch-logo-shadow 2.4s ease-in-out infinite;
    fill: rgba(16, 24, 40, 0.2);
    transform-origin: 22px 36px;
  }

  .quick-switch-panel-logo-mascot {
    animation: quick-switch-logo-float 2.4s ease-in-out infinite;
    transform-origin: 22px 26px;
  }

  .quick-switch-panel-logo-orbit {
    animation: quick-switch-logo-pulse 2.4s ease-in-out infinite;
    fill: rgba(255, 255, 255, 0.84);
    stroke: url("#quick-switch-logo-ring");
    stroke-width: 1.8;
    transform-origin: 22px 21px;
  }

  .quick-switch-panel-logo-core {
    animation: quick-switch-logo-breathe 2.4s ease-in-out infinite;
    transform-origin: 22px 21px;
  }

  .quick-switch-panel-logo-scan {
    animation: quick-switch-logo-scan 1.8s linear infinite;
    fill: none;
    stroke: #ffffff;
    stroke-linecap: round;
    stroke-width: 2.2;
    transform-origin: 22px 22px;
  }

  .quick-switch-panel-logo-eye {
    animation: quick-switch-logo-blink 3.6s ease-in-out infinite;
    fill: #18a058;
    transform-origin: center;
  }

  .quick-switch-panel-logo-sparks {
    animation: quick-switch-logo-sparkle 2.2s ease-in-out infinite;
    fill: #ffb84d;
    transform-origin: 22px 22px;
  }

  .quick-switch-panel-cli-tabs {
    display: flex;
    flex: none;
    gap: 4px;
    padding: 6px 7px;
    background: var(--color-panel-soft);
  }

  .quick-switch-panel-cli-tab {
    height: 24px;
    flex: 1;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
  }

  .quick-switch-panel-cli-tab-active {
    border-color: var(--color-primary-solid);
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  .quick-switch-panel-usage {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
    padding: 0 7px 6px;
  }

  .quick-switch-panel-hero {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 34px;
    padding: 0 8px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);

    .quick-switch-panel-hero-copy {
      display: grid;
      min-width: 0;
      grid-template-columns: auto minmax(0, 1fr);
      gap: 2px 8px;
    }

    .quick-switch-panel-hero-label {
      grid-row: 1 / 3;
      align-self: center;
      padding: 2px 6px;
      border-radius: 5px;
      background: var(--color-primary-soft);
      color: var(--color-primary);
      font-size: 10px;
      font-weight: 800;
    }

    .quick-switch-panel-hero-name,
    .quick-switch-panel-hero-desc {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .quick-switch-panel-hero-name {
      font-size: 12px;
      line-height: 1.15;
    }

    .quick-switch-panel-hero-desc {
      color: var(--color-text-muted);
      font-size: 11px;
      line-height: 1.15;
    }
  }

  .quick-switch-panel-manage-button {
    display: inline-flex;
    height: 23px;
    flex: none;
    align-items: center;
    justify-content: center;
    padding: 0 10px;
    border: 1px solid var(--color-info-line);
    border-radius: 6px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 11px;
    font-weight: 800;
  }

  .quick-switch-panel-manage-button:hover {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
  }

  .quick-switch-panel-metrics {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 5px;

    .quick-switch-panel-metric {
      display: flex;
      min-width: 0;
      align-items: center;
      justify-content: space-between;
      gap: 6px;
      height: 28px;
      padding: 0 7px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
    }

    .quick-switch-panel-metric-label {
      color: var(--color-text-muted);
      font-size: 10px;
      font-weight: 700;
    }

    .quick-switch-panel-metric-value {
      overflow: hidden;
      color: var(--color-text);
      font-size: 12px;
      line-height: 1.2;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .quick-switch-panel-summary-row {
    display: grid;
    min-height: 0;
    flex: 1;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.9fr);
    gap: 5px;
  }

  .quick-switch-panel-usage-panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
    padding: 6px 7px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);

    .quick-switch-panel-usage-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }

    .quick-switch-panel-usage-title {
      color: var(--color-text);
      font-size: 11px;
    }

    .quick-switch-panel-usage-count {
      color: var(--color-text-muted);
      font-size: 10px;
      font-weight: 700;
    }
  }

  .quick-switch-panel-provider-panel {
    min-width: 0;
  }

  .quick-switch-panel-bars {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 4px;
    height: 48px;
    align-items: end;

    .quick-switch-panel-bar {
      display: flex;
      min-width: 0;
      height: 100%;
      flex-direction: column;
      justify-content: flex-end;
      gap: 4px;
    }

    .quick-switch-panel-bar-fill {
      display: block;
      min-height: 6px;
      border-radius: 4px 4px 2px 2px;
      background: var(--color-primary-solid);
    }

    .quick-switch-panel-bar-label {
      overflow: hidden;
      color: var(--color-text-muted);
      font-size: 9px;
      line-height: 1;
      text-align: center;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .quick-switch-panel-provider-bars {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 6px;
    overflow: hidden;

    .quick-switch-panel-provider-bar {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }

    .quick-switch-panel-provider-bar-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }

    .quick-switch-panel-provider-name,
    .quick-switch-panel-provider-cost {
      overflow: hidden;
      font-size: 10px;
      line-height: 1.2;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .quick-switch-panel-provider-name {
      color: var(--color-text);
    }

    .quick-switch-panel-provider-cost {
      flex: none;
      color: var(--color-text-muted);
      font-weight: 700;
    }

    .quick-switch-panel-provider-track {
      height: 6px;
      overflow: hidden;
      border-radius: 999px;
      background: var(--color-panel-soft);
    }

    .quick-switch-panel-provider-fill {
      display: block;
      height: 100%;
      border-radius: inherit;
      background: var(--color-success);
    }
  }

  .quick-switch-panel-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    overflow-x: hidden;
    overflow-y: auto;
    padding: 0 7px 7px;
  }

  .quick-switch-panel-manager-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 9px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);

    .quick-switch-panel-manager-copy {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 2px;
    }

    .quick-switch-panel-manager-title,
    .quick-switch-panel-manager-desc {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .quick-switch-panel-manager-title {
      color: var(--color-text);
      font-size: 12px;
      line-height: 1.2;
    }

    .quick-switch-panel-manager-desc {
      color: var(--color-text-muted);
      font-size: 11px;
      line-height: 1.2;
    }
  }

  .quick-switch-panel-item {
    display: flex;
    min-height: 50px;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 9px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    text-align: left;
    transition:
      border-color 0.18s ease,
      background 0.18s ease,
      box-shadow 0.18s ease,
      transform 0.18s ease;
  }

  .quick-switch-panel-item:hover {
    border-color: var(--color-info-line);
    background: var(--color-panel-soft);
    box-shadow: 0 7px 18px rgba(22, 119, 255, 0.12);
    transform: translateY(-1px);
  }

  .quick-switch-panel-item-active {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
    box-shadow: inset 3px 0 0 var(--color-primary-solid);
  }

  .quick-switch-panel-item-account {
    min-height: 68px;
  }

  .quick-switch-panel-item-copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;

    .quick-switch-panel-item-title,
    .quick-switch-panel-item-desc {
      overflow: hidden;
      max-width: 230px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .quick-switch-panel-item-title {
      font-size: 13px;
      line-height: 1.25;
    }

    .quick-switch-panel-item-desc {
      color: var(--color-text-muted);
      font-size: 12px;
    }
  }

  .quick-switch-panel-item-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 5px;
  }

  .quick-switch-panel-item-icon-button {
    display: inline-flex;
    width: 26px;
    height: 26px;
    flex: none;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-primary);
    cursor: pointer;
  }

  .quick-switch-panel-item-icon-button:hover {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
  }

  .quick-switch-panel-item-action {
    display: inline-flex;
    height: 26px;
    flex: none;
    align-items: center;
    justify-content: center;
    padding: 0 10px;
    border: 1px solid var(--color-info-line);
    border-radius: 6px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 12px;
    font-weight: 700;
  }

  .quick-switch-panel-item-action:hover {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
  }

  .quick-switch-panel-item-action:disabled {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    color: var(--color-text-soft);
    cursor: not-allowed;
  }

  .quick-switch-panel-item-danger-action {
    border-color: var(--color-danger-line);
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .quick-switch-panel-item-danger-action:hover {
    border-color: var(--color-danger-line);
    background: var(--color-danger-soft);
  }

  .quick-switch-panel-quota-list {
    display: flex;
    min-width: 0;
    gap: 5px;
    margin-top: 1px;
  }

  .quick-switch-panel-quota-item {
    display: inline-flex;
    height: 18px;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
    font-size: 11px;
    line-height: 18px;
    white-space: nowrap;

    .quick-switch-panel-quota-value {
      color: var(--color-primary);
      font-size: 11px;
      line-height: 18px;
    }
  }

  .quick-switch-panel-empty {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
  }
}

.quick-switch-panel-collapsed {
  border: 0;
  background: transparent;

  .quick-switch-panel-header {
    height: 100vh;
    justify-content: center;
    padding: 0;
    border-bottom: 0;
    background: transparent;
  }
}

@keyframes quick-switch-logo-float {
  0% {
    transform: translateY(0);
  }

  25% {
    transform: translateY(-3px);
  }

  50% {
    transform: translateY(1px);
  }

  75% {
    transform: translateY(-2px);
  }

  100% {
    transform: translateY(0);
  }
}

@keyframes quick-switch-logo-shadow {
  0%,
  100% {
    opacity: 0.42;
    transform: scaleX(0.86);
  }

  50% {
    opacity: 0.22;
    transform: scaleX(1.08);
  }
}

@keyframes quick-switch-logo-pulse {
  0%,
  100% {
    opacity: 0.88;
    transform: scale(0.95);
  }

  50% {
    opacity: 1;
    transform: scale(1.04);
  }
}

@keyframes quick-switch-logo-breathe {
  0%,
  100% {
    transform: scale(0.94);
  }

  50% {
    transform: scale(1.03);
  }
}

@keyframes quick-switch-logo-scan {
  0% {
    opacity: 0.2;
    transform: rotate(0deg);
  }

  45% {
    opacity: 0.78;
  }

  100% {
    opacity: 0.2;
    transform: rotate(360deg);
  }
}

@keyframes quick-switch-logo-blink {
  0%,
  88%,
  100% {
    transform: scaleY(1);
  }

  92%,
  96% {
    transform: scaleY(0.18);
  }
}

@keyframes quick-switch-logo-sparkle {
  0%,
  100% {
    opacity: 0.28;
    transform: rotate(0deg) scale(0.9);
  }

  50% {
    opacity: 0.95;
    transform: rotate(18deg) scale(1.08);
  }
}
</style>

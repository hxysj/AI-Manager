<template>
  <section class="usage-view">
    <header class="usage-view__toolbar">
      <div>
        <p class="usage-view__eyebrow">Token Usage</p>
        <h1>模型用量统计</h1>
      </div>
      <div class="usage-view__actions">
        <label class="usage-view__currency">
          <span>费用单位</span>
          <select v-model="displayCurrency">
            <option value="USD">$ 美元</option>
            <option value="CNY">￥ 人民币</option>
          </select>
        </label>
        <button type="button" @click="openPricingDialog">
          <Settings :size="16" />
          模型费用
        </button>
        <button type="button" :disabled="pending" @click="syncUsage">
          <RefreshCw :size="16" />
          {{ pending ? "同步中..." : "同步会话日志" }}
        </button>
      </div>
    </header>

    <section class="usage-view__filters">
      <label class="usage-view__field">
        <span>时间范围</span>
        <select v-model="rangeType">
          <option value="today">今天</option>
          <option value="7d">最近 7 天</option>
          <option value="30d">最近 30 天</option>
          <option value="all">全部</option>
        </select>
      </label>
      <label class="usage-view__field">
        <span>应用</span>
        <select v-model="appType" @change="resetScopedFilters">
          <option value="all">全部</option>
          <option v-for="item in appOptions" :key="item" :value="item">
            {{ formatAppName(item) }}
          </option>
        </select>
      </label>
      <label class="usage-view__field">
        <span>Provider</span>
        <select v-model="providerId">
          <option value="all">全部</option>
          <option
            v-for="item in providerOptions"
            :key="item.providerId"
            :value="item.providerId"
          >
            {{ item.providerName }}
          </option>
        </select>
      </label>
      <label class="usage-view__field">
        <span>模型</span>
        <select v-model="model">
          <option value="all">全部</option>
          <option v-for="item in modelOptions" :key="item" :value="item">
            {{ item }}
          </option>
        </select>
      </label>
    </section>

    <section class="usage-view__metrics">
      <article class="usage-view__metric">
        <span>真实消耗 Tokens</span>
        <strong>{{ formatNumber(summary.actualTokens) }}</strong>
        <small>新增输入 {{ formatNumber(summary.inputTokens) }}</small>
      </article>
      <article class="usage-view__metric">
        <span>输出 Tokens</span>
        <strong>{{ formatNumber(summary.outputTokens) }}</strong>
        <small>请求 {{ formatNumber(summary.requestCount) }} 次</small>
      </article>
      <article class="usage-view__metric">
        <span>缓存读取</span>
        <strong>{{ formatNumber(summary.cacheReadTokens) }}</strong>
        <small>命中率 {{ formatPercent(summary.cacheHitRate) }}</small>
      </article>
      <article class="usage-view__metric">
        <span>费用估算</span>
        <strong>{{ formatCost(summary.totalCostUsd) }}</strong>
        <small
          >{{ displayCurrencyLabel }} · 汇率
          {{ formatExchangeRate(exchangeRate) }}</small
        >
      </article>
    </section>

    <section class="usage-view__chart-grid">
      <section class="usage-view__trend">
        <div class="usage-view__section-header">
          <div>
            <h2>Token 趋势</h2>
            <span>{{ trendLabel }}</span>
          </div>
          <BarChart3 :size="18" />
        </div>
        <div
          v-show="trendStats.length"
          ref="trendChartRef"
          class="usage-view__chart"
        ></div>
        <div v-if="!trendStats.length" class="usage-view__empty">
          暂无用量趋势。
        </div>
      </section>

      <section class="usage-view__trend">
        <div class="usage-view__section-header">
          <div>
            <h2>Provider 占比</h2>
            <span>{{ providerStats.length }} 个来源</span>
          </div>
          <PieChart :size="18" />
        </div>
        <div
          v-show="providerStats.length"
          ref="providerPieRef"
          class="usage-view__chart"
        ></div>
        <div v-if="!providerStats.length" class="usage-view__empty">
          暂无 Provider 占比。
        </div>
      </section>
    </section>

    <section class="usage-view__grid">
      <section class="usage-view__panel">
        <div class="usage-view__section-header">
          <div>
            <h2>Provider 统计</h2>
            <span>{{ providerStats.length }} 个来源</span>
          </div>
          <Network :size="18" />
        </div>
        <div v-if="providerStats.length" class="usage-view__stat-list">
          <article
            v-for="item in providerStats"
            :key="item.providerId"
            class="usage-view__stat-card"
          >
            <div>
              <strong>{{ item.providerName }}</strong>
              <span>{{ item.providerType || "未识别类型" }}</span>
            </div>
            <div>
              <strong>{{ formatNumber(item.actualTokens) }}</strong>
              <span
                >{{ formatNumber(item.requestCount) }} 次 ·
                {{ formatCost(item.totalCostUsd) }}</span
              >
            </div>
          </article>
        </div>
        <div v-else class="usage-view__empty">暂无 Provider 统计。</div>
      </section>

      <section class="usage-view__panel">
        <div class="usage-view__section-header">
          <div>
            <h2>模型统计</h2>
            <span>{{ modelStats.length }} 个模型</span>
          </div>
          <Layers :size="18" />
        </div>
        <div v-if="modelStats.length" class="usage-view__stat-list">
          <article
            v-for="item in modelStats"
            :key="`${item.appType}-${item.model}`"
            class="usage-view__stat-card"
          >
            <div>
              <strong>{{ item.model }}</strong>
              <span
                >{{ formatAppName(item.appType) }} ·
                {{ item.providerName }}</span
              >
            </div>
            <div>
              <strong>{{ formatNumber(item.actualTokens) }}</strong>
              <span
                >缓存 {{ formatNumber(item.cacheReadTokens) }} ·
                {{ formatCost(item.totalCostUsd) }}</span
              >
            </div>
          </article>
        </div>
        <div v-else class="usage-view__empty">暂无模型统计。</div>
      </section>
    </section>

    <section class="usage-view__panel usage-view__logs">
      <div class="usage-view__section-header">
        <div>
          <h2>请求日志</h2>
          <span>{{ logs.length }} 条记录</span>
        </div>
        <Database :size="18" />
      </div>
      <div v-if="logs.length" class="usage-view__log-area">
        <div class="usage-view__table">
          <div class="usage-view__table-head">
            <span>时间</span>
            <span>应用</span>
            <span>Provider</span>
            <span>来源</span>
            <span>Session</span>
            <span>模型</span>
            <span>输入</span>
            <span>输出</span>
            <span>缓存</span>
            <span>总量</span>
            <span>费用</span>
          </div>
          <div
            v-for="item in paginatedLogs"
            :key="item.requestId"
            class="usage-view__table-row"
          >
            <span>{{ formatDateTime(item.createdAt) }}</span>
            <span>{{ formatAppName(item.appType) }}</span>
            <span :title="item.providerName">{{ item.providerName }}</span>
            <span>{{ formatDataSource(item.dataSource) }}</span>
            <span :title="item.sessionId || item.sessionTitle">
              {{ formatSessionLabel(item) }}
            </span>
            <span :title="item.model">{{ item.model || "未识别模型" }}</span>
            <span>{{ formatNumber(normalizeInput(item)) }}</span>
            <span>{{ formatNumber(item.outputTokens) }}</span>
            <span>{{ formatNumber(item.cacheReadTokens) }}</span>
            <span>{{ formatNumber(actualTokens(item)) }}</span>
            <span>{{ formatCost(item.totalCostUsd) }}</span>
          </div>
        </div>
        <div class="usage-view__pager">
          <div>
            <span>
              {{ logStartIndex + 1 }}-{{ logEndIndex }} / {{ logs.length }}
            </span>
            <select v-model.number="logPageSize">
              <option :value="20">20 条/页</option>
              <option :value="50">50 条/页</option>
              <option :value="100">100 条/页</option>
            </select>
          </div>
          <div>
            <button type="button" :disabled="logPage <= 1" @click="prevLogPage">
              <ChevronLeft :size="16" />
            </button>
            <strong>第 {{ logPage }} / {{ totalLogPages }} 页</strong>
            <button
              type="button"
              :disabled="logPage >= totalLogPages"
              @click="nextLogPage"
            >
              <ChevronRight :size="16" />
            </button>
          </div>
        </div>
      </div>
      <div v-else class="usage-view__empty">暂无用量日志。</div>
    </section>

    <div v-if="pricingDialogOpen" class="usage-view__modal">
      <section class="usage-view__dialog">
        <header class="usage-view__dialog-header">
          <div>
            <p class="usage-view__eyebrow">Model Pricing</p>
            <h2>模型费用设置</h2>
          </div>
          <button type="button" @click="closePricingDialog">
            <X :size="18" />
          </button>
        </header>

        <section class="usage-view__pricing-toolbar">
          <label class="usage-view__field">
            <span>人民币汇率</span>
            <input
              v-model.number="pricingDraft.exchangeRate"
              min="0"
              step="0.0001"
              type="number"
            />
          </label>
          <label class="usage-view__field">
            <span>类型筛选</span>
            <select v-model="pricingCategoryFilter">
              <option value="all">全部类型</option>
              <option
                v-for="option in pricingCategoryOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </label>
          <button type="button" @click="addPricingItem">
            <Plus :size="16" />
            新增模型费用
          </button>
        </section>

        <section class="usage-view__pricing-list">
          <div class="usage-view__pricing-head">
            <span>模型</span>
            <span>类别</span>
            <span>单位</span>
            <span>输入/百万</span>
            <span>输出/百万</span>
            <span>缓存读/百万</span>
            <span>缓存写/百万</span>
            <span></span>
          </div>
          <div
            v-for="item in pagedPricingItems"
            :key="item.id"
            class="usage-view__pricing-row"
          >
            <template v-if="pricingEditingId === item.id">
              <input
                v-model.trim="item.modelId"
                placeholder="如 claude-sonnet-4"
              />
              <select v-model="item.modelCategory">
                <option
                  v-for="option in pricingCategoryOptions"
                  :key="option.value"
                  :value="option.value"
                >
                  {{ option.label }}
                </option>
              </select>
              <select v-model="item.currency">
                <option value="USD">$</option>
                <option value="CNY">￥</option>
              </select>
              <input
                v-model.number="item.inputCostPerMillion"
                min="0"
                step="0.000001"
                type="number"
              />
              <input
                v-model.number="item.outputCostPerMillion"
                min="0"
                step="0.000001"
                type="number"
              />
              <input
                v-model.number="item.cacheReadCostPerMillion"
                min="0"
                step="0.000001"
                type="number"
              />
              <input
                v-model.number="item.cacheCreationCostPerMillion"
                min="0"
                step="0.000001"
                type="number"
              />
            </template>
            <template v-else>
              <span class="usage-view__pricing-value" :title="item.modelId">
                {{ item.modelId || "未填写" }}
              </span>
              <span
                :class="[
                  'usage-view__pricing-category',
                  `usage-view__pricing-category--${item.modelCategory}`
                ]"
              >
                {{ formatModelCategory(item.modelCategory) }}
              </span>
              <span class="usage-view__pricing-value">
                {{ item.currency === "CNY" ? "￥" : "$" }}
              </span>
              <span class="usage-view__pricing-value">
                {{ formatPricingAmount(item.inputCostPerMillion) }}
              </span>
              <span class="usage-view__pricing-value">
                {{ formatPricingAmount(item.outputCostPerMillion) }}
              </span>
              <span class="usage-view__pricing-value">
                {{ formatPricingAmount(item.cacheReadCostPerMillion) }}
              </span>
              <span class="usage-view__pricing-value">
                {{ formatPricingAmount(item.cacheCreationCostPerMillion) }}
              </span>
            </template>
            <div class="usage-view__pricing-actions">
              <template v-if="pricingEditingId === item.id">
                <button
                  class="usage-view__pricing-save"
                  type="button"
                  :disabled="pricingSaving"
                  @click="savePricingItem(item)"
                >
                  <Save :size="15" />
                  保存
                </button>
                <button
                  type="button"
                  title="取消"
                  :disabled="pricingSaving"
                  @click="cancelPricingItem(item.id)"
                >
                  <X :size="15" />
                </button>
              </template>
              <template v-else>
                <button
                  type="button"
                  title="编辑"
                  @click="editPricingItem(item.id)"
                >
                  <Pencil :size="15" />
                </button>
                <button
                  type="button"
                  title="删除"
                  @click="removePricingItem(item.id)"
                >
                  <Trash2 :size="15" />
                </button>
              </template>
            </div>
          </div>
          <div v-if="!pricingDraft.items.length" class="usage-view__empty">
            暂无模型费用，点击新增后填写模型名和每百万 Token 单价。
          </div>
          <div
            v-else-if="!filteredPricingItems.length"
            class="usage-view__empty"
          >
            当前类型暂无模型费用。
          </div>
        </section>

        <div
          v-if="pricingDraft.items.length && filteredPricingItems.length"
          class="usage-view__pricing-pagination"
        >
          <span>
            第 {{ pricingPage }} / {{ pricingPageCount }} 页 ·
            {{ pricingPageStart }}-{{ pricingPageEnd }} /
            {{ filteredPricingItems.length }}
          </span>
          <label>
            <span>每页</span>
            <select v-model.number="pricingPageSize">
              <option
                v-for="item in pricingPageSizeOptions"
                :key="item"
                :value="item"
              >
                {{ item }}
              </option>
            </select>
          </label>
          <button
            type="button"
            :disabled="pricingPage <= 1"
            @click="prevPricingPage"
          >
            <ChevronLeft :size="15" />
          </button>
          <button
            type="button"
            :disabled="pricingPage >= pricingPageCount"
            @click="nextPricingPage"
          >
            <ChevronRight :size="15" />
          </button>
        </div>

        <p v-if="pricingError" class="usage-view__error">{{ pricingError }}</p>

        <footer class="usage-view__dialog-actions">
          <button type="button" @click="closePricingDialog">取消</button>
          <button type="button" :disabled="pricingSaving" @click="savePricing">
            {{ pricingSaving ? "保存中..." : "保存费用配置" }}
          </button>
        </footer>
      </section>
    </div>
  </section>
</template>

<script setup>
import * as echarts from "echarts"
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  BarChart3,
  ChevronLeft,
  ChevronRight,
  Database,
  Layers,
  Network,
  Pencil,
  PieChart,
  Plus,
  RefreshCw,
  Save,
  Settings,
  Trash2,
  X
} from "lucide-vue-next"

const props = defineProps({
  usage: {
    type: Object,
    default: () => ({})
  }
})

const pending = ref(false)
const pricingSaving = ref(false)
const stats = ref(props.usage || {})
const rangeType = ref("today")
const appType = ref("all")
const providerId = ref("all")
const model = ref("all")
const displayCurrency = ref("USD")
const logPage = ref(1)
const logPageSize = ref(20)
const pricingDialogOpen = ref(false)
const pricingError = ref("")
const pricingDraft = ref(createEmptyPricingConfig())
const pricingEditingId = ref("")
const pricingCategoryFilter = ref("all")
const pricingPage = ref(1)
const pricingPageSize = ref(8)
const trendChartRef = ref(null)
const providerPieRef = ref(null)
let trendChart = null
let providerPie = null
const pricingPageSizeOptions = [8, 12, 20, 50]
const pricingCategoryOptions = [
  { value: "gpt", label: "GPT" },
  { value: "claude", label: "Claude" },
  { value: "qwen", label: "Qwen" },
  { value: "doubao", label: "Doubao" },
  { value: "deepseek", label: "DeepSeek" }
]

const summary = computed(() => stats.value.summary || createEmptySummary())
const pricingConfig = computed(
  () => stats.value.pricingConfig || createEmptyPricingConfig()
)
const exchangeRate = computed(() =>
  Number(pricingConfig.value.exchangeRate || 7.2)
)
const displayCurrencyLabel = computed(() =>
  displayCurrency.value === "CNY" ? "人民币显示" : "美元显示"
)
const providerStats = computed(() => stats.value.providerStats || [])
const modelStats = computed(() => stats.value.modelStats || [])
const trendStats = computed(() => stats.value.trends || [])
const trendLabel = computed(() =>
  rangeType.value === "today"
    ? `${trendStats.value.length} 个小时`
    : `${trendStats.value.length} 个本地日`
)
const logs = computed(() => stats.value.logs || [])
const totalLogPages = computed(() =>
  Math.max(1, Math.ceil(logs.value.length / logPageSize.value))
)
const logStartIndex = computed(() => (logPage.value - 1) * logPageSize.value)
const logEndIndex = computed(() =>
  Math.min(logStartIndex.value + logPageSize.value, logs.value.length)
)
const paginatedLogs = computed(() =>
  logs.value.slice(logStartIndex.value, logEndIndex.value)
)
const appOptions = computed(() => stats.value.filters?.appTypes || [])
const providerOptions = computed(() => stats.value.filters?.providers || [])
const modelOptions = computed(() => stats.value.filters?.models || [])
const filteredPricingItems = computed(() =>
  pricingDraft.value.items.filter((item) => {
    return (
      pricingCategoryFilter.value === "all" ||
      item.modelCategory === pricingCategoryFilter.value ||
      item.id === pricingEditingId.value
    )
  })
)
const pricingPageCount = computed(() =>
  Math.max(
    1,
    Math.ceil(filteredPricingItems.value.length / pricingPageSize.value)
  )
)
const pricingPageStart = computed(() => {
  if (!filteredPricingItems.value.length) {
    return 0
  }

  return (pricingPage.value - 1) * pricingPageSize.value + 1
})
const pricingPageEnd = computed(() =>
  Math.min(
    pricingPage.value * pricingPageSize.value,
    filteredPricingItems.value.length
  )
)
const pagedPricingItems = computed(() => {
  if (!filteredPricingItems.value.length) {
    return []
  }

  return filteredPricingItems.value.slice(
    pricingPageStart.value - 1,
    pricingPageEnd.value
  )
})

watch([rangeType, appType, providerId, model], () => {
  logPage.value = 1
  loadStats()
})

watch(logPageSize, () => {
  logPage.value = 1
})

watch(pricingPageSize, () => {
  pricingPage.value = 1
})

watch(pricingCategoryFilter, () => {
  pricingPage.value = 1
})

watch(
  () => filteredPricingItems.value.length,
  () => {
    if (pricingPage.value > pricingPageCount.value) {
      pricingPage.value = pricingPageCount.value
    }
  }
)

onMounted(() => {
  loadStats()
  window.addEventListener("resize", resizeCharts)
})

onBeforeUnmount(() => {
  window.removeEventListener("resize", resizeCharts)
  trendChart?.dispose()
  providerPie?.dispose()
})

function createEmptySummary() {
  return {
    requestCount: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    actualTokens: 0,
    cacheHitRate: 0,
    totalCostUsd: 0
  }
}

function createEmptyPricingConfig() {
  return {
    exchangeRate: 7.2,
    items: []
  }
}

function createPricingItem(input = {}) {
  return {
    id:
      input.id ||
      `pricing-${Date.now()}-${Math.random().toString(16).slice(2)}`,
    modelId: input.modelId || "",
    modelCategory: normalizeModelCategory(
      input.modelCategory || input.category,
      input.modelId || ""
    ),
    currency: input.currency === "CNY" ? "CNY" : "USD",
    inputCostPerMillion: Number(input.inputCostPerMillion || 0),
    outputCostPerMillion: Number(input.outputCostPerMillion || 0),
    cacheReadCostPerMillion: Number(input.cacheReadCostPerMillion || 0),
    cacheCreationCostPerMillion: Number(input.cacheCreationCostPerMillion || 0)
  }
}

function clonePricingConfig(input) {
  return {
    exchangeRate: Number(input?.exchangeRate || 7.2),
    items: (input?.items || []).map((item) => createPricingItem(item))
  }
}

function createFilterPayload() {
  const now = Date.now()
  const days = rangeType.value === "7d" ? 7 : 30
  const todayStart = new Date()

  todayStart.setHours(0, 0, 0, 0)

  return {
    appType: appType.value,
    providerId: providerId.value,
    model: model.value,
    trendMode: rangeType.value === "today" ? "hour" : "day",
    startAt:
      rangeType.value === "all"
        ? 0
        : rangeType.value === "today"
          ? todayStart.getTime()
          : now - days * 24 * 60 * 60 * 1000,
    endAt: now
  }
}

function resetScopedFilters() {
  providerId.value = "all"
  model.value = "all"
}

function inferModelCategory(modelId) {
  const value = String(modelId || "").toLowerCase()

  if (value.includes("claude")) {
    return "claude"
  }

  if (value.includes("qwen")) {
    return "qwen"
  }

  if (value.includes("doubao")) {
    return "doubao"
  }

  if (value.includes("deepseek")) {
    return "deepseek"
  }

  return "gpt"
}

function normalizeModelCategory(value, modelId) {
  const category = String(value || inferModelCategory(modelId)).toLowerCase()

  return pricingCategoryOptions.some((item) => item.value === category)
    ? category
    : inferModelCategory(modelId)
}

function prevLogPage() {
  logPage.value = Math.max(1, logPage.value - 1)
}

function nextLogPage() {
  logPage.value = Math.min(totalLogPages.value, logPage.value + 1)
}

function clampLogPage() {
  logPage.value = Math.min(logPage.value, totalLogPages.value)
}

function prevPricingPage() {
  pricingPage.value = Math.max(1, pricingPage.value - 1)
}

function nextPricingPage() {
  pricingPage.value = Math.min(pricingPageCount.value, pricingPage.value + 1)
}

function openPricingDialog() {
  pricingDraft.value = clonePricingConfig(pricingConfig.value)
  pricingEditingId.value = ""
  pricingCategoryFilter.value = "all"
  pricingPage.value = 1
  pricingError.value = ""
  pricingDialogOpen.value = true
}

function closePricingDialog() {
  pricingDialogOpen.value = false
  pricingEditingId.value = ""
  pricingError.value = ""
}

function addPricingItem() {
  const item = createPricingItem({
    modelCategory:
      pricingCategoryFilter.value === "all"
        ? "gpt"
        : pricingCategoryFilter.value
  })

  pricingDraft.value.items.push(item)
  pricingEditingId.value = item.id
  pricingPage.value = pricingPageCount.value
}

function editPricingItem(id) {
  pricingEditingId.value = id
  pricingError.value = ""
}

function cancelPricingItem(id) {
  const savedItem = pricingConfig.value.items.find((item) => item.id === id)

  if (savedItem) {
    const index = pricingDraft.value.items.findIndex((item) => item.id === id)
    pricingDraft.value.items.splice(index, 1, createPricingItem(savedItem))
  } else {
    pricingDraft.value.items = pricingDraft.value.items.filter(
      (item) => item.id !== id
    )
  }

  pricingEditingId.value = ""
  pricingError.value = ""
}

async function removePricingItem(id) {
  pricingDraft.value.items = pricingDraft.value.items.filter(
    (item) => item.id !== id
  )

  if (pricingEditingId.value === id) {
    pricingEditingId.value = ""
  }

  await savePricing({ closeDialog: false })
}

async function savePricingItem(item) {
  pricingEditingId.value = item.id

  if (await savePricing({ closeDialog: false })) {
    pricingEditingId.value = ""
  }
}

async function savePricing(options = {}) {
  const closeDialog = options.closeDialog !== false
  const exchangeRateValue = Number(pricingDraft.value.exchangeRate || 0)
  const modelNames = new Set()

  pricingError.value = ""

  if (exchangeRateValue <= 0) {
    pricingError.value = "汇率必须大于 0"
    return false
  }

  for (const item of pricingDraft.value.items) {
    const modelId = item.modelId.trim()

    if (!modelId) {
      pricingError.value = "模型名称不能为空"
      return false
    }

    if (modelNames.has(modelId.toLowerCase())) {
      pricingError.value = `模型 ${modelId} 已重复配置`
      return false
    }

    modelNames.add(modelId.toLowerCase())
  }

  pricingSaving.value = true

  try {
    await window.aiManager.saveUsagePricing({
      exchangeRate: exchangeRateValue,
      items: pricingDraft.value.items.map((item) => ({
        id: item.id,
        modelId: item.modelId.trim(),
        modelCategory: item.modelCategory,
        currency: item.currency,
        inputCostPerMillion: Number(item.inputCostPerMillion || 0),
        outputCostPerMillion: Number(item.outputCostPerMillion || 0),
        cacheReadCostPerMillion: Number(item.cacheReadCostPerMillion || 0),
        cacheCreationCostPerMillion: Number(
          item.cacheCreationCostPerMillion || 0
        )
      }))
    })
    if (closeDialog) {
      pricingDialogOpen.value = false
    }
    await loadStats()
  } catch (error) {
    pricingError.value = error.message
    return false
  } finally {
    pricingSaving.value = false
  }

  if (!closeDialog) {
    pricingDraft.value = clonePricingConfig(pricingConfig.value)
  }

  return true
}

async function loadStats() {
  pending.value = true

  try {
    const result = await window.aiManager.getUsageStats(createFilterPayload())
    stats.value = result?.data || createEmptySummary()
    clampLogPage()
    await nextTick()
    renderCharts()
  } finally {
    pending.value = false
  }
}

async function syncUsage() {
  pending.value = true

  try {
    await window.aiManager.syncUsage()
    await loadStats()
  } finally {
    pending.value = false
  }
}

function renderCharts() {
  renderTrendChart()
  renderProviderPie()
}

function renderTrendChart() {
  if (!trendChartRef.value || !trendStats.value.length) {
    return
  }

  trendChart = trendChart || echarts.init(trendChartRef.value)
  trendChart.setOption({
    color: ["#2f5f91", "#7a9bbb", "#b7c7d8"],
    tooltip: {
      trigger: "axis",
      valueFormatter: (value) => formatNumber(value)
    },
    grid: {
      top: 28,
      right: 18,
      bottom: 28,
      left: 48
    },
    legend: {
      top: 0,
      right: 0,
      itemWidth: 10,
      itemHeight: 10,
      textStyle: {
        color: "#5f7087",
        fontSize: 11
      }
    },
    xAxis: {
      type: "category",
      data: trendStats.value.map((item) => item.date),
      axisTick: { show: false },
      axisLine: { lineStyle: { color: "#dbe5ed" } },
      axisLabel: { color: "#5f7087" }
    },
    yAxis: {
      type: "value",
      splitLine: { lineStyle: { color: "#edf2f8" } },
      axisLabel: {
        color: "#5f7087",
        formatter: (value) => formatCompactNumber(value)
      }
    },
    series: [
      {
        name: "真实消耗",
        type: "bar",
        barMaxWidth: 28,
        data: trendStats.value.map((item) => item.actualTokens)
      },
      {
        name: "输出",
        type: "line",
        smooth: true,
        symbolSize: 6,
        data: trendStats.value.map((item) => item.outputTokens)
      },
      {
        name: "缓存读取",
        type: "line",
        smooth: true,
        symbolSize: 6,
        data: trendStats.value.map((item) => item.cacheReadTokens)
      }
    ]
  })
}

function renderProviderPie() {
  if (!providerPieRef.value || !providerStats.value.length) {
    return
  }

  providerPie = providerPie || echarts.init(providerPieRef.value)
  providerPie.setOption({
    color: ["#2f5f91", "#5d7fa4", "#8aa7c4", "#b9c9d8", "#d8e2ec"],
    tooltip: {
      trigger: "item",
      formatter: (item) => {
        return `${item.name}<br />${formatNumber(item.value)} Tokens · ${item.percent}%`
      }
    },
    legend: {
      orient: "vertical",
      right: 0,
      top: "middle",
      itemWidth: 10,
      itemHeight: 10,
      textStyle: {
        color: "#5f7087",
        fontSize: 11
      }
    },
    series: [
      {
        type: "pie",
        radius: ["42%", "68%"],
        center: ["36%", "52%"],
        avoidLabelOverlap: true,
        label: {
          color: "#14213a",
          formatter: "{d}%"
        },
        labelLine: {
          length: 10,
          length2: 8
        },
        data: providerStats.value.map((item) => ({
          name: item.providerName,
          value: item.actualTokens
        }))
      }
    ]
  })
}

function resizeCharts() {
  trendChart?.resize()
  providerPie?.resize()
}

function normalizeInput(item) {
  if (item.appType === "codex" || item.appType === "gemini") {
    return Math.max(0, item.inputTokens - item.cacheReadTokens)
  }

  return item.inputTokens
}

function actualTokens(item) {
  return (
    normalizeInput(item) +
    item.outputTokens +
    item.cacheReadTokens +
    item.cacheCreationTokens
  )
}

function formatNumber(value) {
  return new Intl.NumberFormat("zh-CN").format(Number(value || 0))
}

function formatPercent(value) {
  return `${(Number(value || 0) * 100).toFixed(1)}%`
}

function formatCost(value) {
  const cost =
    displayCurrency.value === "CNY"
      ? Number(value || 0) * exchangeRate.value
      : Number(value || 0)
  const symbol = displayCurrency.value === "CNY" ? "￥" : "$"

  if (!cost) {
    return `${symbol}0`
  }

  return `${symbol}${cost >= 1 ? cost.toFixed(2) : cost.toFixed(6)}`
}

function formatPricingAmount(value) {
  return Number(value || 0).toLocaleString("zh-CN", {
    maximumFractionDigits: 6
  })
}

function formatModelCategory(value) {
  return (
    pricingCategoryOptions.find((item) => item.value === value)?.label || "GPT"
  )
}

function formatExchangeRate(value) {
  return `1 $ = ￥${Number(value || 0).toFixed(4)}`
}

function formatCompactNumber(value) {
  const number = Number(value || 0)

  if (number >= 1000000) {
    return `${(number / 1000000).toFixed(1)}M`
  }

  if (number >= 1000) {
    return `${(number / 1000).toFixed(0)}K`
  }

  return String(number)
}

function formatDateTime(value) {
  if (!value) {
    return "未记录"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(value))
}

function formatAppName(value) {
  const names = {
    claude: "Claude",
    codex: "Codex",
    gemini: "Gemini"
  }

  return names[value] || value || "未知"
}

function formatDataSource(value) {
  const names = {
    proxy: "代理",
    session_log: "Claude 日志",
    codex_session: "Codex 日志",
    gemini_session: "Gemini 日志"
  }

  return names[value] || "旧数据"
}

function formatSessionLabel(item) {
  return item.sessionTitle || item.sessionId || "-"
}
</script>

<style scoped lang="less">
.usage-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  overflow-x: hidden;
  overflow-y: auto;
  padding-right: 4px;
  color: var(--color-text);

  &__toolbar {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin-right: -4px;
    padding: 0 0 10px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-page);
  }

  &__eyebrow {
    margin: 0 0 5px;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  &__toolbar h1 {
    margin: 0;
    font-size: 1.38rem;
    line-height: 1.2;
  }

  &__actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  &__actions button,
  &__currency select,
  &__field input,
  &__field select,
  &__pricing-row input,
  &__pricing-row select,
  &__pricing-pagination select {
    height: 36px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.86rem;
    font-weight: 600;
  }

  &__actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
  }

  &__currency {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
  }

  &__currency select {
    width: 104px;
    padding: 0 9px;
    color: var(--color-text);
  }

  &__actions button:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }

  &__filters {
    display: grid;
    grid-template-columns: 150px 150px minmax(0, 1fr) minmax(0, 1fr);
    flex: none;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  &__field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  &__field span {
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  &__field select {
    min-width: 0;
    padding: 0 10px;
    color: var(--color-text);
  }

  &__field input {
    min-width: 0;
    padding: 0 10px;
    color: var(--color-text);
  }

  &__metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    flex: none;
    gap: 12px;
  }

  &__metric,
  &__panel,
  &__trend {
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__metric {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 7px;
    padding: 14px;
  }

  &__metric span,
  &__metric small {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__metric strong {
    overflow: hidden;
    font-size: 1.32rem;
    line-height: 1.1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__trend {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
  }

  &__chart-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    flex: none;
    gap: 12px;
  }

  &__chart {
    width: 100%;
    height: 260px;
    min-width: 0;
  }

  &__section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__section-header h2 {
    margin: 0 0 4px;
    font-size: 0.98rem;
    line-height: 1.25;
  }

  &__section-header span {
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    min-height: 0;
    flex: none;
    gap: 12px;
  }

  &__panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
    padding: 14px;
  }

  &__stat-list {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding-right: 4px;
  }

  &__stat-card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 150px;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__stat-card div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }

  &__stat-card div:last-child {
    align-items: flex-end;
  }

  &__stat-card strong,
  &__stat-card span {
    overflow: hidden;
    max-width: 100%;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__stat-card strong {
    font-size: 0.86rem;
  }

  &__stat-card span {
    color: var(--color-text-muted);
    font-size: 0.74rem;
  }

  &__logs {
    flex: none;
    min-height: 360px;
  }

  &__log-area {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 10px;
  }

  &__table {
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
  }

  &__table-head,
  &__table-row {
    display: grid;
    grid-template-columns:
      100px 72px 150px 92px 130px minmax(160px, 1fr)
      86px 86px 86px 92px 92px;
    min-width: 1260px;
    gap: 10px;
    align-items: center;
    min-height: 34px;
    padding: 0 10px;
    border-bottom: 1px solid var(--color-line);
    font-size: 0.76rem;
  }

  &__table-head {
    position: sticky;
    top: 0;
    z-index: 1;
    background: #edf2f8;
    color: var(--color-text-muted);
    font-weight: 700;
  }

  &__table-row {
    background: #ffffff;
  }

  &__table-row:last-child {
    border-bottom: 0;
  }

  &__table-row span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__pager {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__pager div {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__pager select {
    height: 32px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-text);
    cursor: pointer;
    font-size: 0.78rem;
    font-weight: 700;
    padding: 0 8px;
  }

  &__pager button {
    display: inline-flex;
    width: 32px;
    height: 32px;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
  }

  &__pager button:disabled {
    cursor: not-allowed;
    opacity: 0.48;
  }

  &__pager strong {
    color: var(--color-text);
    font-size: 0.8rem;
    white-space: nowrap;
  }

  &__empty {
    display: grid;
    flex: 1;
    min-height: 110px;
    place-items: center;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 0.86rem;
  }

  &__modal {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: grid;
    place-items: center;
    background: rgba(15, 28, 46, 0.34);
  }

  &__dialog {
    display: flex;
    width: 1040px;
    max-height: 680px;
    flex-direction: column;
    gap: 14px;
    padding: 18px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel);
    box-shadow: 0 24px 70px rgba(15, 28, 46, 0.24);
  }

  &__dialog-header,
  &__dialog-actions,
  &__pricing-toolbar {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__dialog-header h2 {
    margin: 0;
    font-size: 1.18rem;
  }

  &__dialog-header button,
  &__pricing-toolbar button,
  &__dialog-actions button,
  &__pricing-row button,
  &__pricing-pagination button {
    display: inline-flex;
    height: 34px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &__dialog-header button,
  &__pricing-row button,
  &__pricing-pagination button {
    width: 34px;
    padding: 0;
  }

  &__pricing-toolbar {
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__pricing-toolbar .usage-view__field {
    width: 180px;
  }

  &__pricing-toolbar button,
  &__dialog-actions button {
    padding: 0 12px;
  }

  &__pricing-list {
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
  }

  &__pricing-head,
  &__pricing-row {
    display: grid;
    grid-template-columns: 190px 96px 70px 108px 108px 108px 108px 120px;
    gap: 8px;
    align-items: center;
  }

  &__pricing-head {
    position: sticky;
    top: 0;
    z-index: 1;
    min-height: 36px;
    padding: 0 12px;
    border-bottom: 1px solid var(--color-line);
    background: #edf2f8;
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  &__pricing-row {
    min-height: 42px;
    padding: 0 12px;
    border-bottom: 1px solid var(--color-line);
    background: #ffffff;
  }

  &__pricing-row:last-child {
    border-bottom: 0;
  }

  &__pricing-row input,
  &__pricing-row select {
    min-width: 0;
    padding: 0 9px;
    color: var(--color-text);
  }

  &__pricing-value {
    overflow: hidden;
    min-width: 0;
    padding: 0 2px;
    color: var(--color-text);
    font-size: 0.82rem;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__pricing-category {
    display: inline-flex;
    width: fit-content;
    max-width: 100%;
    height: 24px;
    align-items: center;
    padding: 0 9px;
    border-radius: 7px;
    background: #e8eef6;
    color: #28415f;
    font-size: 0.74rem;
    font-weight: 800;
  }

  &__pricing-category--claude {
    background: #fff0e8;
    color: #9a4a16;
  }

  &__pricing-category--qwen {
    background: #e8f3ff;
    color: #17569b;
  }

  &__pricing-category--doubao {
    background: #eaf7ef;
    color: #197447;
  }

  &__pricing-category--deepseek {
    background: #f0ecff;
    color: #5740a8;
  }

  &__pricing-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  &__pricing-row &__pricing-save {
    width: 72px;
  }

  &__pricing-pagination {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__pricing-pagination label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  &__pricing-pagination select {
    height: 32px;
    padding: 0 8px;
    color: var(--color-text);
  }

  &__dialog-actions {
    justify-content: flex-end;
  }

  &__dialog-actions button:last-child {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  &__dialog-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }

  &__error {
    margin: 0;
    color: #b42318;
    font-size: 0.82rem;
    font-weight: 700;
  }
}
</style>

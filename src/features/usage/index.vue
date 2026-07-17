<template>
  <section
    ref="usageReportRef"
    :class="['usage-view', { 'usage-view-loading-active': pageLoading }]"
  >
    <div v-if="pageLoading" class="usage-view-loading">
      <RefreshCw class="usage-view-loading-icon" :size="22" />
      <span>正在加载用量数据...</span>
    </div>

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
        <button
          class="usage-view__report-button"
          type="button"
          :disabled="pending || reportExporting"
          @click="exportUsageReport"
        >
          <Download :size="16" />
          {{ reportExporting ? "导出中..." : "导出长图" }}
        </button>
        <button type="button" :disabled="pending" @click="syncUsage">
          <RefreshCw :size="16" />
          {{ syncing ? "同步中..." : "同步会话日志" }}
        </button>
      </div>
    </header>

    <section class="usage-view__filters">
      <label class="usage-view__field">
        <span>时间范围</span>
        <el-date-picker
          v-model="dateTimeRange"
          type="datetimerange"
          :shortcuts="dateTimeShortcuts"
          :default-time="defaultDateTimeRange"
          :show-confirm="false"
          clearable
          end-placeholder="结束时间"
          range-separator="至"
          start-placeholder="开始时间"
        />
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
        <span>请求来源</span>
        <select v-model="requestSource">
          <option value="all">全部</option>
          <option
            v-for="item in requestSourceOptions"
            :key="item"
            :value="item"
          >
            {{ formatRequestSourceOption(item) }}
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
            <span>{{ tokenTrendLabel }}</span>
          </div>
          <div class="usage-view__chart-tabs" role="tablist">
            <button
              v-for="item in tokenTrendTabs"
              :key="item.id"
              type="button"
              :class="[
                'usage-view__chart-tab',
                { 'usage-view__chart-tab--active': tokenTrendMode === item.id }
              ]"
              role="tab"
              :aria-selected="tokenTrendMode === item.id"
              @click="tokenTrendMode = item.id"
            >
              {{ item.label }}
            </button>
          </div>
        </div>
        <div class="usage-view__chart-box">
          <div
            v-show="tokenTrendSeries.length"
            ref="trendChartRef"
            class="usage-view__chart"
            @wheel="handleTrendWheel"
          ></div>
          <div v-if="!tokenTrendSeries.length" class="usage-view__empty">
            {{ tokenTrendEmptyText }}
          </div>
        </div>
      </section>

      <section class="usage-view__trend">
        <div class="usage-view__section-header">
          <div>
            <h2>{{ usagePieTitle }}</h2>
            <span>{{ usagePieCountLabel }}</span>
          </div>
          <div class="usage-view__chart-tabs" role="tablist">
            <button
              v-for="item in usagePieTabs"
              :key="item.id"
              type="button"
              :class="[
                'usage-view__chart-tab',
                { 'usage-view__chart-tab--active': usagePieMode === item.id }
              ]"
              role="tab"
              :aria-selected="usagePieMode === item.id"
              @click="usagePieMode = item.id"
            >
              {{ item.label }}
            </button>
          </div>
        </div>
        <div class="usage-view__chart-box">
          <div
            v-show="usagePieStats.length"
            ref="providerPieRef"
            class="usage-view__chart"
          ></div>
          <div v-if="!usagePieStats.length" class="usage-view__empty">
            {{ usagePieEmptyText }}
          </div>
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
          <span>{{ logTotalCount }} 条记录</span>
        </div>
        <div class="usage-view__section-actions">
          <button
            type="button"
            :disabled="pending || !logTotalCount"
            @click="exportUsageLogsCsv"
          >
            <Download :size="15" />
            导出 CSV
          </button>
          <Database :size="18" />
        </div>
      </div>
      <div v-if="logs.length" class="usage-view__log-area">
        <div class="usage-view__table">
          <div class="usage-view__table-head">
            <span>时间</span>
            <span>应用</span>
            <span>Provider</span>
            <span>请求来源</span>
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
            <span :title="formatRequestSource(item)">
              {{ formatRequestSource(item) }}
            </span>
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
              {{ logStartIndex + 1 }}-{{ logEndIndex }} / {{ logTotalCount }}
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
          <div class="usage-view__pricing-toolbar-actions">
            <label class="usage-view__pricing-export">
              <span>导出格式</span>
              <select v-model="pricingExportFormat">
                <option value="json">JSON</option>
                <option value="csv">CSV</option>
              </select>
            </label>
            <button type="button" @click="exportPricingFile">
              <Download :size="16" />
              导出模型费用
            </button>
            <button type="button" @click="openPricingImportDialog">
              <Upload :size="16" />
              导入模型费用
            </button>
            <button type="button" @click="addPricingItem">
              <Plus :size="16" />
              新增模型费用
            </button>
          </div>
        </section>
        <datalist id="usage-pricing-category-options">
          <option
            v-for="option in pricingCategoryOptions"
            :key="option.value"
            :value="option.value"
          ></option>
        </datalist>

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
              <input
                v-model.trim="item.modelCategory"
                list="usage-pricing-category-options"
                placeholder="输入或选择类别"
              />
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
                class="usage-view__pricing-category"
                :title="formatModelCategory(item.modelCategory)"
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

    <div
      v-if="pricingImportOpen"
      class="usage-view__modal usage-view__modal--stack"
    >
      <section class="usage-view__import-dialog">
        <header class="usage-view__dialog-header">
          <div>
            <p class="usage-view__eyebrow">Pricing Import</p>
            <h2>批量导入模型费用</h2>
          </div>
          <button type="button" @click="closePricingImportDialog">
            <X :size="18" />
          </button>
        </header>

        <p class="usage-view__import-tip">
          支持导入 JSON / CSV 文件，也可以直接粘贴
          JSON；同名模型会自动覆盖去重。
        </p>
        <input
          ref="pricingImportFileRef"
          class="usage-view__file-input"
          type="file"
          accept=".json,.csv,application/json,text/csv"
          @change="importPricingFile"
        />
        <div class="usage-view__import-file">
          <button type="button" @click="selectPricingImportFile">
            <Upload :size="15" />
            选择 JSON/CSV 文件
          </button>
          <span>CSV 使用导出的表头即可再次导入。</span>
        </div>
        <textarea
          v-model="pricingImportText"
          class="usage-view__import-textarea"
          :placeholder="pricingImportPlaceholder"
          spellcheck="false"
        ></textarea>
        <p v-if="pricingImportMessage" class="usage-view__import-success">
          {{ pricingImportMessage }}
        </p>
        <p v-if="pricingError" class="usage-view__error">{{ pricingError }}</p>

        <footer class="usage-view__dialog-actions">
          <button type="button" @click="clearPricingImport">清空</button>
          <button type="button" @click="closePricingImportDialog">取消</button>
          <button type="button" @click="importPricingJson">
            <Upload :size="15" />
            导入粘贴 JSON
          </button>
        </footer>
      </section>
    </div>
  </section>
</template>

<script setup>
import {
  BarChart,
  LineChart,
  PieChart as EchartsPieChart
} from "echarts/charts"
import {
  GridComponent,
  LegendComponent,
  TooltipComponent
} from "echarts/components"
import * as echarts from "echarts/core"
import { CanvasRenderer } from "echarts/renderers"
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  BarChart3,
  ChevronLeft,
  ChevronRight,
  Database,
  Download,
  Layers,
  Network,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Settings,
  Trash2,
  Upload,
  X
} from "lucide-vue-next"
import { systemApi, usageApi } from "@/api"
import { createMessage } from "@/utils/message"

echarts.use([
  BarChart,
  LineChart,
  EchartsPieChart,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  CanvasRenderer
])

const props = defineProps({
  usage: {
    type: Object,
    default: () => ({})
  }
})

const usageSearchParams = new URLSearchParams(window.location.search)
const isReportExportWindow = usageSearchParams.get("export") === "usage-report"
const pending = ref(false)
const syncing = ref(false)
const reportExporting = ref(false)
const pricingSaving = ref(false)
const pageLoading = computed(() => pending.value && !isReportExportWindow)
const stats = ref(props.usage || {})
const rangeType = ref(usageSearchParams.get("rangeType") || "today")
const dateTimeRange = ref(createInitialDateTimeRange())
const appType = ref(usageSearchParams.get("appType") || "all")
const providerId = ref(usageSearchParams.get("providerId") || "all")
const requestSource = ref(usageSearchParams.get("requestSource") || "all")
const model = ref(usageSearchParams.get("model") || "all")
const displayCurrency = ref(usageSearchParams.get("displayCurrency") || "USD")
const logPage = ref(1)
const logPageSize = ref(20)
const pricingDialogOpen = ref(false)
const pricingError = ref("")
const pricingDraft = ref(createEmptyPricingConfig())
const pricingEditingId = ref("")
const pricingCategoryFilter = ref("all")
const pricingImportOpen = ref(false)
const pricingImportText = ref("")
const pricingImportMessage = ref("")
const pricingExportFormat = ref("json")
const pricingImportFileRef = ref(null)
const pricingPage = ref(1)
const pricingPageSize = ref(8)
const tokenTrendMode = ref("model")
const usagePieMode = ref("provider")
const trendModeOverride = ref("")
const usageReportRef = ref(null)
const trendChartRef = ref(null)
const providerPieRef = ref(null)
let trendChart = null
let providerPie = null
let usageSyncTimer = null
const trendModeLevels = ["day", "hour", "minute"]
const pricingPageSizeOptions = [8, 12, 20, 50]
const pricingImportPlaceholder = `[
  {
    "modelId": "gpt-5.5",
    "modelCategory": "OpenAI",
    "currency": "USD",
    "inputCostPerMillion": 5,
    "outputCostPerMillion": 30,
    "cacheReadCostPerMillion": 0.5,
    "cacheCreationCostPerMillion": 0
  }
]`
const pricingCsvHeaders = [
  "modelId",
  "modelCategory",
  "currency",
  "inputCostPerMillion",
  "outputCostPerMillion",
  "cacheReadCostPerMillion",
  "cacheCreationCostPerMillion"
]
const usageLogCsvColumns = [
  ["时间", (item) => formatExportDateTime(item.createdAt)],
  ["应用", (item) => formatAppName(item.appType)],
  ["Provider", (item) => item.providerName],
  ["请求来源", (item) => formatRequestSource(item)],
  ["来源", (item) => formatDataSource(item.dataSource)],
  ["Session", (item) => formatSessionLabel(item)],
  ["模型", (item) => item.model || "未识别模型"],
  ["输入", (item) => normalizeInput(item)],
  ["输出", (item) => item.outputTokens],
  ["缓存", (item) => item.cacheReadTokens],
  ["总量", (item) => actualTokens(item)],
  ["费用", (item) => formatCost(item.totalCostUsd)]
]
const defaultDateTimeRange = [
  new Date(2000, 0, 1, 0, 0, 0),
  new Date(2000, 0, 1, 23, 59, 59)
]
const dateTimeShortcuts = [
  {
    text: "当天",
    value: () => createPresetDateTimeRange("today")
  },
  {
    text: "最近一周",
    value: () => createPresetDateTimeRange("week")
  },
  {
    text: "最近一个月",
    value: () => createPresetDateTimeRange("month")
  },
  {
    text: "全部时间",
    value: null,
    onClick: () => (dateTimeRange.value = [])
  }
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
const tokenTrendTabs = [
  { id: "model", label: "模型" },
  { id: "provider", label: "Provider" }
]
const usagePieTabs = [
  { id: "provider", label: "Provider" },
  { id: "model", label: "模型" }
]
const modelPieStats = computed(() => {
  const groups = new Map()

  for (const item of modelStats.value) {
    const name = item.model || "未识别模型"

    if (!groups.has(name)) {
      groups.set(name, {
        name,
        value: 0
      })
    }

    groups.get(name).value += item.actualTokens
  }

  return Array.from(groups.values()).sort(
    (left, right) => right.value - left.value
  )
})
const usagePieStats = computed(() =>
  usagePieMode.value === "provider"
    ? providerStats.value.map((item) => ({
        name: item.providerName,
        value: item.actualTokens
      }))
    : modelPieStats.value
)
const usagePieTitle = computed(() =>
  usagePieMode.value === "provider" ? "Provider 占比" : "模型占比统计"
)
const usagePieCountLabel = computed(() =>
  usagePieMode.value === "provider"
    ? `${providerStats.value.length} 个来源`
    : `${modelPieStats.value.length} 个模型`
)
const usagePieEmptyText = computed(() =>
  usagePieMode.value === "provider" ? "暂无 Provider 占比。" : "暂无模型占比。"
)
const isSingleDayTrendRange = computed(() => {
  const [start, end] = dateTimeRange.value || []

  return start && end && start.toDateString() === end.toDateString()
})
const trendMode = computed(() => {
  if (trendModeOverride.value) {
    return trendModeOverride.value
  }

  return isSingleDayTrendRange.value ? "hour" : "day"
})
const trendLabel = computed(() => {
  if (trendMode.value === "minute") {
    return `${trendStats.value.length} 个分钟`
  }

  return trendMode.value === "hour"
    ? `${trendStats.value.length} 个小时`
    : `${trendStats.value.length} 个本地日`
})
const logs = computed(() => stats.value.logs || [])
const logTotalCount = computed(() => Number(stats.value.logTotalCount || 0))
const modelTrendSeries = computed(() => stats.value.trendSeries?.models || [])
const providerTrendSeries = computed(
  () => stats.value.trendSeries?.providers || []
)
const tokenTrendSeries = computed(() =>
  tokenTrendMode.value === "provider"
    ? providerTrendSeries.value
    : modelTrendSeries.value
)
const tokenTrendLabel = computed(
  () =>
    `${tokenTrendSeries.value.length} 个${
      tokenTrendMode.value === "provider" ? "来源" : "模型"
    } · ${trendLabel.value}`
)
const tokenTrendEmptyText = computed(() =>
  tokenTrendMode.value === "provider"
    ? "暂无 Provider 用量趋势。"
    : "暂无模型用量趋势。"
)
const totalLogPages = computed(() =>
  Math.max(1, Math.ceil(logTotalCount.value / logPageSize.value))
)
const logStartIndex = computed(() => (logPage.value - 1) * logPageSize.value)
const logEndIndex = computed(() =>
  Math.min(logStartIndex.value + logs.value.length, logTotalCount.value)
)
const paginatedLogs = computed(() => logs.value)
const appOptions = computed(() => stats.value.filters?.appTypes || [])
const providerOptions = computed(() => stats.value.filters?.providers || [])
const requestSourceOptions = computed(
  () => stats.value.filters?.requestSources || []
)
const modelOptions = computed(() => stats.value.filters?.models || [])
const rangeTypeLabel = computed(() => {
  const [start, end] = dateTimeRange.value || []

  return start && end
    ? `${formatFilterDateTime(start)} 至 ${formatFilterDateTime(end)}`
    : "全部时间"
})
const selectedAppLabel = computed(() =>
  appType.value === "all" ? "全部应用" : formatAppName(appType.value)
)
const selectedProviderLabel = computed(() => {
  if (providerId.value === "all") {
    return "全部Provider"
  }

  return (
    providerOptions.value.find((item) => item.providerId === providerId.value)
      ?.providerName || providerId.value
  )
})
const selectedRequestSourceLabel = computed(() =>
  requestSource.value === "all"
    ? "全部请求来源"
    : formatRequestSourceOption(requestSource.value)
)
const selectedModelLabel = computed(() =>
  model.value === "all" ? "全部模型" : model.value
)
const pricingCategoryOptions = computed(() => {
  const categories = new Map()

  for (const item of pricingDraft.value.items) {
    const category = normalizeModelCategory(item.modelCategory)
    const key = category.toLowerCase()

    if (category && !categories.has(key)) {
      categories.set(key, category)
    }
  }

  return Array.from(categories.values()).map((category) => ({
    value: category,
    label: category
  }))
})
const filteredPricingItems = computed(() =>
  pricingDraft.value.items.filter((item) => {
    const category = normalizeModelCategory(item.modelCategory)

    return (
      pricingCategoryFilter.value === "all" ||
      category === pricingCategoryFilter.value ||
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

watch([dateTimeRange, appType, providerId, requestSource, model], () => {
  logPage.value = 1
  loadStats()
})

watch(
  () => props.usage,
  () => {
    if (!pending.value) {
      loadStats()
    }
  }
)

watch(logPageSize, () => {
  if (logPage.value === 1) {
    loadStats()
    return
  }

  logPage.value = 1
  loadStats()
})

watch(pricingPageSize, () => {
  pricingPage.value = 1
})

watch(pricingCategoryFilter, () => {
  pricingPage.value = 1
})

watch(usagePieMode, async () => {
  await nextTick()
  renderProviderPie()
})

watch(tokenTrendMode, async () => {
  await nextTick()
  renderTrendChart()
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
  if (isReportExportWindow) {
    window.__usageReportReady = false
    document.body.classList.add("usage-report-exporting")
  }

  if (isReportExportWindow) {
    loadStats()
  } else {
    syncUsage()
    usageSyncTimer = window.setInterval(() => {
      if (!pending.value && !reportExporting.value) {
        loadStats()
      }
    }, 60000)
  }
  window.addEventListener("resize", resizeCharts)
})

onBeforeUnmount(() => {
  if (usageSyncTimer) {
    window.clearInterval(usageSyncTimer)
  }
  window.removeEventListener("resize", resizeCharts)
  if (isReportExportWindow) {
    document.body.classList.remove("usage-report-exporting")
  }
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
      input.modelCategory ?? input.category
    ),
    currency: normalizePricingCurrency(input.currency),
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

function createPresetDateTimeRange(type) {
  if (type === "all") {
    return []
  }

  const end = new Date()
  const start = new Date()

  // 预设范围持续包含当天后续同步的日志。
  end.setHours(23, 59, 59, 999)

  if (type === "today") {
    start.setHours(0, 0, 0, 0)
  } else {
    if (type === "week" || type === "7d") {
      start.setDate(start.getDate() - 7)
    } else {
      start.setMonth(start.getMonth() - 1)
    }
  }

  return [start, end]
}

function createInitialDateTimeRange() {
  const startAt = Number(usageSearchParams.get("startAt") || 0)
  const endAt = Number(usageSearchParams.get("endAt") || 0)

  if (startAt && endAt) {
    return [new Date(startAt), new Date(endAt)]
  }

  return createPresetDateTimeRange(rangeType.value)
}

function createFilterPayload(options = {}) {
  const [start, end] = dateTimeRange.value || []

  return {
    appType: appType.value,
    providerId: providerId.value,
    requestSource: requestSource.value,
    model: model.value,
    trendMode: trendMode.value,
    logPage: options.logPage || logPage.value,
    logPageSize: options.logPageSize || logPageSize.value,
    includeAllLogs: options.includeAllLogs === true,
    startAt: start ? start.getTime() : 0,
    endAt: end ? end.getTime() : 0
  }
}

function handleTrendWheel(event) {
  if (!event.ctrlKey || pending.value) {
    return
  }

  event.preventDefault()

  const currentIndex = trendModeLevels.indexOf(trendMode.value)
  const nextIndex =
    event.deltaY < 0
      ? Math.min(currentIndex + 1, trendModeLevels.length - 1)
      : Math.max(currentIndex - 1, 0)
  const nextMode = trendModeLevels[nextIndex]

  if (nextMode === trendMode.value) {
    return
  }

  trendModeOverride.value = nextMode
  loadStats()
}

function resetScopedFilters() {
  providerId.value = "all"
  model.value = "all"
}

function normalizeModelCategory(value) {
  return String(value || "").trim()
}

function normalizePricingCurrency(value) {
  const currency = String(value || "")
    .trim()
    .toUpperCase()

  return currency === "CNY" || currency === "RMB" || currency === "￥"
    ? "CNY"
    : "USD"
}

function prevLogPage() {
  logPage.value = Math.max(1, logPage.value - 1)
  loadStats()
}

function nextLogPage() {
  logPage.value = Math.min(totalLogPages.value, logPage.value + 1)
  loadStats()
}

function clampLogPage() {
  const nextPage = Math.min(logPage.value, totalLogPages.value)

  if (nextPage === logPage.value) {
    return false
  }

  logPage.value = nextPage
  return true
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
  pricingImportOpen.value = false
  pricingImportText.value = ""
  pricingImportMessage.value = ""
  pricingPage.value = 1
  pricingError.value = ""
  pricingDialogOpen.value = true
}

function closePricingDialog() {
  pricingDialogOpen.value = false
  pricingEditingId.value = ""
  pricingError.value = ""
  pricingImportOpen.value = false
  pricingImportText.value = ""
  pricingImportMessage.value = ""
}

function selectPricingImportFile() {
  pricingImportFileRef.value?.click()
}

function openPricingImportDialog() {
  pricingImportOpen.value = true
  pricingImportText.value = ""
  pricingImportMessage.value = ""
  pricingError.value = ""
}

function closePricingImportDialog() {
  pricingImportOpen.value = false
  pricingImportText.value = ""
  pricingImportMessage.value = ""
  pricingError.value = ""
}

function clearPricingImport() {
  pricingImportText.value = ""
  pricingImportMessage.value = ""
  pricingError.value = ""
}

function addPricingItem() {
  const item = createPricingItem({
    modelCategory:
      pricingCategoryFilter.value === "all" ? "" : pricingCategoryFilter.value
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

function normalizePricingHeader(value) {
  return String(value || "")
    .replace(/^\uFEFF/, "")
    .trim()
    .replace(/\s+/g, "")
    .toLowerCase()
}

function pickPricingImportValue(input, keys) {
  for (const key of keys) {
    if (input?.[key] !== undefined) {
      return input[key]
    }
  }

  return 0
}

function createPricingImportItem(input = {}) {
  return createPricingItem({
    modelId:
      input.modelId || input.model_id || input.model || input.modelName || "",
    modelCategory:
      input.modelCategory ?? input.model_category ?? input.category,
    currency: input.currency || input.unit,
    inputCostPerMillion: pickPricingImportValue(input, [
      "inputCostPerMillion",
      "input_cost_per_million",
      "input"
    ]),
    outputCostPerMillion: pickPricingImportValue(input, [
      "outputCostPerMillion",
      "output_cost_per_million",
      "output"
    ]),
    cacheReadCostPerMillion: pickPricingImportValue(input, [
      "cacheReadCostPerMillion",
      "cache_read_cost_per_million",
      "cacheRead"
    ]),
    cacheCreationCostPerMillion: pickPricingImportValue(input, [
      "cacheCreationCostPerMillion",
      "cache_creation_cost_per_million",
      "cacheCreation"
    ])
  })
}

function parsePricingJsonText(text) {
  const payload = JSON.parse(text)
  const rawItems = Array.isArray(payload)
    ? payload
    : Array.isArray(payload?.items)
      ? payload.items
      : []

  return {
    exchangeRate: Number(payload?.exchangeRate || 0),
    items: rawItems
  }
}

function parseCsvRows(text) {
  const rows = []
  let row = []
  let cell = ""
  let quoted = false

  for (let index = 0; index < text.length; index += 1) {
    const char = text[index]
    const nextChar = text[index + 1]

    if (char === '"' && quoted && nextChar === '"') {
      cell += '"'
      index += 1
      continue
    }

    if (char === '"') {
      quoted = !quoted
      continue
    }

    if (char === "," && !quoted) {
      row.push(cell)
      cell = ""
      continue
    }

    if ((char === "\n" || char === "\r") && !quoted) {
      if (char === "\r" && nextChar === "\n") {
        index += 1
      }

      row.push(cell)
      rows.push(row)
      row = []
      cell = ""
      continue
    }

    cell += char
  }

  row.push(cell)
  rows.push(row)

  return rows.filter((item) => item.some((value) => String(value || "").trim()))
}

function readPricingCsvValue(row, headers, aliases) {
  for (const alias of aliases) {
    const index = headers.indexOf(normalizePricingHeader(alias))

    if (index >= 0 && row[index] !== undefined) {
      return row[index]
    }
  }

  return ""
}

function parsePricingCsvText(text) {
  const rows = parseCsvRows(text)

  if (rows.length < 2) {
    throw new Error("CSV 至少需要表头和一条模型费用")
  }

  const headers = rows[0].map((item) => normalizePricingHeader(item))

  return rows.slice(1).map((row) => ({
    modelId: readPricingCsvValue(row, headers, [
      "modelId",
      "model_id",
      "modelName",
      "model",
      "模型"
    ]),
    modelCategory: readPricingCsvValue(row, headers, [
      "modelCategory",
      "model_category",
      "category",
      "类别"
    ]),
    currency: readPricingCsvValue(row, headers, ["currency", "unit", "单位"]),
    inputCostPerMillion: readPricingCsvValue(row, headers, [
      "inputCostPerMillion",
      "input_cost_per_million",
      "input",
      "输入/百万",
      "输入"
    ]),
    outputCostPerMillion: readPricingCsvValue(row, headers, [
      "outputCostPerMillion",
      "output_cost_per_million",
      "output",
      "输出/百万",
      "输出"
    ]),
    cacheReadCostPerMillion: readPricingCsvValue(row, headers, [
      "cacheReadCostPerMillion",
      "cache_read_cost_per_million",
      "cacheRead",
      "缓存读/百万",
      "缓存读"
    ]),
    cacheCreationCostPerMillion: readPricingCsvValue(row, headers, [
      "cacheCreationCostPerMillion",
      "cache_creation_cost_per_million",
      "cacheCreation",
      "缓存写/百万",
      "缓存写"
    ])
  }))
}

function mergePricingItems(rawItems, sourceLabel, exchangeRateValue = 0) {
  if (!rawItems.length) {
    pricingError.value = "没有可导入的模型费用"
    return
  }

  pricingError.value = ""
  pricingImportMessage.value = ""

  if (exchangeRateValue > 0) {
    pricingDraft.value.exchangeRate = exchangeRateValue
  }

  const currentItems = new Map()
  const importItems = new Map()
  let duplicateCount = 0

  for (const item of pricingDraft.value.items) {
    const key = item.modelId.trim().toLowerCase()

    if (key) {
      currentItems.set(key, createPricingItem(item))
    }
  }

  for (const rawItem of rawItems) {
    const item = createPricingImportItem(rawItem)
    const key = item.modelId.trim().toLowerCase()

    if (!key) {
      pricingError.value = "导入数据中存在未填写模型名称的项目"
      return
    }

    if (importItems.has(key)) {
      duplicateCount += 1
    }

    importItems.set(key, item)
  }

  let replaceCount = 0

  for (const [key, item] of importItems) {
    if (currentItems.has(key)) {
      replaceCount += 1
    }

    currentItems.set(key, item)
  }

  pricingDraft.value.items = Array.from(currentItems.values())
  pricingCategoryFilter.value = "all"
  pricingEditingId.value = ""
  pricingPage.value = Math.max(
    1,
    Math.ceil(pricingDraft.value.items.length / pricingPageSize.value)
  )
  pricingImportText.value = ""
  pricingImportMessage.value = `${sourceLabel}已导入 ${importItems.size} 个模型，覆盖 ${replaceCount} 个，去重 ${duplicateCount} 个。`
}

function importPricingJson() {
  const text = pricingImportText.value.trim()

  pricingError.value = ""
  pricingImportMessage.value = ""

  if (!text) {
    pricingError.value = "请先粘贴模型费用 JSON"
    return
  }

  try {
    const payload = parsePricingJsonText(text)

    if (!payload.items.length) {
      pricingError.value = "JSON 必须是模型数组，或包含 items 数组"
      return
    }

    mergePricingItems(payload.items, "粘贴 JSON ", payload.exchangeRate)
  } catch (error) {
    pricingError.value =
      error instanceof SyntaxError
        ? "JSON 格式不正确"
        : error.message || "JSON 导入失败"
  }
}

async function importPricingFile(event) {
  const file = event.target.files?.[0]

  pricingError.value = ""
  pricingImportMessage.value = ""

  if (!file) {
    return
  }

  try {
    const text = await file.text()
    const fileName = file.name.toLowerCase()

    if (fileName.endsWith(".csv")) {
      mergePricingItems(parsePricingCsvText(text), "CSV 文件 ")
    } else {
      const payload = parsePricingJsonText(text)

      if (!payload.items.length) {
        pricingError.value = "JSON 文件必须是模型数组，或包含 items 数组"
        return
      }

      mergePricingItems(payload.items, "JSON 文件 ", payload.exchangeRate)
    }
  } catch (error) {
    pricingError.value =
      error instanceof SyntaxError
        ? "JSON 文件格式不正确"
        : error.message || "文件导入失败"
  } finally {
    event.target.value = ""
  }
}

function createPricingExportItem(item) {
  return {
    modelId: item.modelId.trim(),
    modelCategory: normalizeModelCategory(item.modelCategory),
    currency: item.currency,
    inputCostPerMillion: Number(item.inputCostPerMillion || 0),
    outputCostPerMillion: Number(item.outputCostPerMillion || 0),
    cacheReadCostPerMillion: Number(item.cacheReadCostPerMillion || 0),
    cacheCreationCostPerMillion: Number(item.cacheCreationCostPerMillion || 0)
  }
}

function createCsvCell(value) {
  const text = String(value ?? "")

  return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text
}

function createPricingCsvText(items) {
  return [
    pricingCsvHeaders.join(","),
    ...items.map((item) =>
      pricingCsvHeaders.map((key) => createCsvCell(item[key])).join(",")
    )
  ].join("\n")
}

function createUsageLogCsvText(items) {
  return [
    usageLogCsvColumns.map(([label]) => createCsvCell(label)).join(","),
    ...items.map((item) =>
      usageLogCsvColumns
        .map(([, getter]) => createCsvCell(getter(item)))
        .join(",")
    )
  ].join("\n")
}

function createExportTimestamp() {
  const now = new Date()

  return (
    [
      now.getFullYear(),
      String(now.getMonth() + 1).padStart(2, "0"),
      String(now.getDate()).padStart(2, "0")
    ].join("") +
    `-${[
      String(now.getHours()).padStart(2, "0"),
      String(now.getMinutes()).padStart(2, "0"),
      String(now.getSeconds()).padStart(2, "0")
    ].join("")}`
  )
}

function downloadUsageFile(fileName, content, type) {
  const blob = new Blob([content], { type })
  const url = URL.createObjectURL(blob)
  const link = document.createElement("a")

  link.href = url
  link.download = fileName
  document.body.appendChild(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
}

function exportPricingFile() {
  const items = pricingDraft.value.items
    .filter((item) => item.modelId.trim())
    .map((item) => createPricingExportItem(item))

  if (!items.length) {
    pricingError.value = "没有可导出的模型费用"
    return
  }

  pricingError.value = ""

  if (pricingExportFormat.value === "csv") {
    downloadUsageFile(
      "model-pricing.csv",
      `\uFEFF${createPricingCsvText(items)}`,
      "text/csv;charset=utf-8"
    )
    return
  }

  downloadUsageFile(
    "model-pricing.json",
    `${JSON.stringify(
      {
        exchangeRate: Number(pricingDraft.value.exchangeRate || 7.2),
        items
      },
      null,
      2
    )}\n`,
    "application/json;charset=utf-8"
  )
}

async function exportUsageLogsCsv() {
  if (!logTotalCount.value) {
    createMessage.warning("没有可导出的请求日志。")
    return
  }

  pending.value = true

  try {
    const result = await usageApi.getUsageStats(
      createFilterPayload({ includeAllLogs: true })
    )
    const exportLogs = result?.data?.logs || []

    downloadUsageFile(
      `usage-request-logs-${createExportTimestamp()}.csv`,
      `\uFEFF${createUsageLogCsvText(exportLogs)}`,
      "text/csv;charset=utf-8"
    )
    createMessage.success(`已导出 ${exportLogs.length} 条请求日志。`)
  } finally {
    pending.value = false
  }
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
    await usageApi.saveUsagePricing({
      exchangeRate: exchangeRateValue,
      items: pricingDraft.value.items.map((item) => ({
        id: item.id,
        modelId: item.modelId.trim(),
        modelCategory: normalizeModelCategory(item.modelCategory),
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
  if (isReportExportWindow) {
    window.__usageReportReady = false
  }

  try {
    const result = await usageApi.getUsageStats(createFilterPayload())
    stats.value = result?.data || createEmptySummary()
    if (clampLogPage()) {
      await loadStats()
      return
    }
    await nextTick()
    renderCharts()
  } finally {
    pending.value = false
    if (isReportExportWindow) {
      await nextTick()
      resizeCharts()
      await waitReportFrame()
      window.__usageReportReady = true
    }
  }
}

async function syncUsage() {
  pending.value = true
  syncing.value = true

  try {
    const result = await usageApi.syncUsage(createFilterPayload())
    stats.value = result?.data || createEmptySummary()
    if (clampLogPage()) {
      await loadStats()
      return
    }
    await nextTick()
    renderCharts()
  } finally {
    pending.value = false
    syncing.value = false
  }
}

function waitReportFrame() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve))
  })
}

async function createUsageReportImageData() {
  if (!usageReportRef.value) {
    throw new Error("未找到用量报告内容。")
  }

  const shouldRestoreExportClass = !document.body.classList.contains(
    "usage-report-exporting"
  )

  if (shouldRestoreExportClass) {
    document.body.classList.add("usage-report-exporting")
  }

  try {
    await nextTick()
    renderCharts()
    resizeCharts()
    await waitReportFrame()
    return await renderUsageReportCanvas()
  } finally {
    if (shouldRestoreExportClass) {
      document.body.classList.remove("usage-report-exporting")
      await nextTick()
      resizeCharts()
    }
  }
}

async function renderUsageReportCanvas() {
  const width = 1180
  const padding = 36
  const gap = 18
  const contentWidth = width - padding * 2
  const chartWidth = (contentWidth - gap) / 2
  const chartHeight = 310
  const providerItems = createReportProviderItems()
  const modelItems = createReportModelItems()
  const providerListHeight = createReportListHeight(providerItems)
  const modelListHeight = createReportListHeight(modelItems)
  const height =
    padding +
    76 +
    42 +
    118 +
    gap +
    chartHeight +
    gap +
    Math.max(providerListHeight, modelListHeight) +
    padding
  const ratio = createExportPixelRatio(width, height)
  const canvas = document.createElement("canvas")
  const context = canvas.getContext("2d")

  if (!context) {
    throw new Error("用量报告图片画布创建失败。")
  }

  const [trendImage, pieImage] = await Promise.all([
    loadReportCanvasImage(trendChartRef.value),
    loadReportCanvasImage(providerPieRef.value)
  ])

  canvas.width = Math.ceil(width * ratio)
  canvas.height = Math.ceil(height * ratio)
  context.scale(ratio, ratio)
  context.fillStyle = "#f5f7fb"
  context.fillRect(0, 0, width, height)

  let y = padding

  drawReportHeader(context, padding, y, contentWidth)
  y += 76
  drawReportFilters(context, padding, y, contentWidth)
  y += 42
  drawReportMetrics(context, padding, y, contentWidth, gap)
  y += 118 + gap
  drawReportChart(
    context,
    padding,
    y,
    chartWidth,
    chartHeight,
    "Token 趋势",
    tokenTrendLabel.value,
    trendImage,
    tokenTrendEmptyText.value
  )
  drawReportChart(
    context,
    padding + chartWidth + gap,
    y,
    chartWidth,
    chartHeight,
    usagePieTitle.value,
    usagePieCountLabel.value,
    pieImage,
    usagePieEmptyText.value
  )
  y += chartHeight + gap
  drawReportList(
    context,
    padding,
    y,
    chartWidth,
    providerListHeight,
    "Provider 统计",
    `${providerStats.value.length} 个来源`,
    providerItems,
    "暂无 Provider 统计。"
  )
  drawReportList(
    context,
    padding + chartWidth + gap,
    y,
    chartWidth,
    modelListHeight,
    "模型统计",
    `${modelStats.value.length} 个模型`,
    modelItems,
    "暂无模型统计。"
  )

  return canvas.toDataURL("image/png")
}

function drawReportHeader(context, x, y, width) {
  drawReportText(context, "TOKEN USAGE", x, y + 14, {
    color: "#7a8da8",
    font: "700 12px Arial"
  })
  drawReportText(context, "模型用量统计", x, y + 48, {
    color: "#17233a",
    font: "700 30px Arial"
  })
  drawReportText(
    context,
    `生成时间 ${formatDateTime(Date.now())}`,
    x + width,
    y + 44,
    {
      align: "right",
      color: "#6c7d94",
      font: "600 13px Arial"
    }
  )
}

function drawReportFilters(context, x, y, width) {
  const filters = [
    rangeTypeLabel.value,
    selectedAppLabel.value,
    selectedProviderLabel.value,
    selectedRequestSourceLabel.value,
    selectedModelLabel.value
  ]
  let currentX = x

  for (const filter of filters) {
    const text = fitReportText(context, filter, 190, "600 13px Arial")
    const itemWidth = Math.min(210, context.measureText(text).width + 26)

    if (currentX + itemWidth > x + width) {
      break
    }

    drawReportRoundRect(
      context,
      currentX,
      y,
      itemWidth,
      30,
      15,
      "#ffffff",
      "#dce5ef"
    )
    drawReportText(context, text, currentX + 13, y + 20, {
      color: "#476179",
      font: "600 13px Arial"
    })
    currentX += itemWidth + 10
  }
}

function drawReportMetrics(context, x, y, width, gap) {
  const cardWidth = (width - gap * 3) / 4
  const items = [
    {
      label: "真实消耗 Tokens",
      value: formatNumber(summary.value.actualTokens),
      note: `新增输入 ${formatNumber(summary.value.inputTokens)}`
    },
    {
      label: "输出 Tokens",
      value: formatNumber(summary.value.outputTokens),
      note: `请求 ${formatNumber(summary.value.requestCount)} 次`
    },
    {
      label: "缓存读取",
      value: formatNumber(summary.value.cacheReadTokens),
      note: `命中率 ${formatPercent(summary.value.cacheHitRate)}`
    },
    {
      label: "费用估算",
      value: formatCost(summary.value.totalCostUsd),
      note: `${displayCurrencyLabel.value} · 汇率 ${formatExchangeRate(exchangeRate.value)}`
    }
  ]

  items.forEach((item, index) => {
    const cardX = x + index * (cardWidth + gap)

    drawReportRoundRect(
      context,
      cardX,
      y,
      cardWidth,
      100,
      14,
      "#ffffff",
      "#dde6f0"
    )
    drawReportText(context, item.label, cardX + 18, y + 28, {
      color: "#718197",
      font: "700 13px Arial"
    })
    drawReportText(
      context,
      fitReportText(context, item.value, cardWidth - 36, "700 26px Arial"),
      cardX + 18,
      y + 62,
      {
        color: "#19314d",
        font: "700 26px Arial"
      }
    )
    drawReportText(
      context,
      fitReportText(context, item.note, cardWidth - 36, "600 12px Arial"),
      cardX + 18,
      y + 84,
      {
        color: "#8290a4",
        font: "600 12px Arial"
      }
    )
  })
}

function drawReportChart(
  context,
  x,
  y,
  width,
  height,
  title,
  subtitle,
  image,
  emptyText
) {
  drawReportRoundRect(context, x, y, width, height, 14, "#ffffff", "#dde6f0")
  drawReportSectionHeader(context, x + 18, y + 20, width - 36, title, subtitle)

  if (image) {
    drawReportImage(context, image, x + 18, y + 62, width - 36, height - 82)
  } else {
    drawReportText(context, emptyText, x + width / 2, y + height / 2 + 12, {
      align: "center",
      color: "#8b98aa",
      font: "600 14px Arial"
    })
  }
}

function drawReportList(
  context,
  x,
  y,
  width,
  height,
  title,
  subtitle,
  items,
  emptyText
) {
  drawReportRoundRect(context, x, y, width, height, 14, "#ffffff", "#dde6f0")
  drawReportSectionHeader(context, x + 18, y + 20, width - 36, title, subtitle)

  if (!items.length) {
    drawReportText(context, emptyText, x + width / 2, y + 86, {
      align: "center",
      color: "#8b98aa",
      font: "600 14px Arial"
    })
    return
  }

  let currentY = y + 62

  items.forEach((item) => {
    drawReportRoundRect(
      context,
      x + 14,
      currentY,
      width - 28,
      58,
      12,
      "#f8fafc",
      "#e5edf5"
    )
    drawReportText(
      context,
      fitReportText(context, item.title, width - 210, "700 14px Arial"),
      x + 30,
      currentY + 23,
      {
        color: "#253852",
        font: "700 14px Arial"
      }
    )
    drawReportText(
      context,
      fitReportText(context, item.description, width - 210, "600 12px Arial"),
      x + 30,
      currentY + 43,
      {
        color: "#7a8799",
        font: "600 12px Arial"
      }
    )
    drawReportText(context, item.value, x + width - 30, currentY + 24, {
      align: "right",
      color: "#1c3450",
      font: "700 15px Arial"
    })
    drawReportText(context, item.note, x + width - 30, currentY + 43, {
      align: "right",
      color: "#7a8799",
      font: "600 12px Arial"
    })
    currentY += 66
  })
}

function drawReportSectionHeader(context, x, y, width, title, subtitle) {
  drawReportText(context, title, x, y, {
    color: "#1e334d",
    font: "700 18px Arial"
  })
  drawReportText(
    context,
    fitReportText(context, subtitle, width, "600 12px Arial"),
    x,
    y + 21,
    {
      color: "#7a8799",
      font: "600 12px Arial"
    }
  )
}

function drawReportImage(context, image, x, y, width, height) {
  const scale = Math.min(width / image.width, height / image.height)
  const imageWidth = image.width * scale
  const imageHeight = image.height * scale

  context.drawImage(
    image,
    x + (width - imageWidth) / 2,
    y + (height - imageHeight) / 2,
    imageWidth,
    imageHeight
  )
}

function drawReportText(context, text, x, y, options = {}) {
  context.fillStyle = options.color || "#1f2937"
  context.font = options.font || "14px Arial"
  context.textAlign = options.align || "left"
  context.textBaseline = "alphabetic"
  context.fillText(String(text || ""), x, y)
}

function drawReportRoundRect(
  context,
  x,
  y,
  width,
  height,
  radius,
  fill,
  stroke
) {
  context.beginPath()
  context.moveTo(x + radius, y)
  context.lineTo(x + width - radius, y)
  context.quadraticCurveTo(x + width, y, x + width, y + radius)
  context.lineTo(x + width, y + height - radius)
  context.quadraticCurveTo(
    x + width,
    y + height,
    x + width - radius,
    y + height
  )
  context.lineTo(x + radius, y + height)
  context.quadraticCurveTo(x, y + height, x, y + height - radius)
  context.lineTo(x, y + radius)
  context.quadraticCurveTo(x, y, x + radius, y)
  context.closePath()
  context.fillStyle = fill
  context.fill()

  if (stroke) {
    context.strokeStyle = stroke
    context.lineWidth = 1
    context.stroke()
  }
}

function fitReportText(context, text, maxWidth, font) {
  const value = String(text || "")

  context.font = font

  if (context.measureText(value).width <= maxWidth) {
    return value
  }

  let nextValue = value

  while (
    nextValue.length > 1 &&
    context.measureText(`${nextValue}...`).width > maxWidth
  ) {
    nextValue = nextValue.slice(0, -1)
  }

  return `${nextValue}...`
}

function createReportProviderItems() {
  const items = providerStats.value.slice(0, 24).map((item) => ({
    title: item.providerName || "未识别来源",
    description: item.providerType || "未识别类型",
    value: formatNumber(item.actualTokens),
    note: `${formatNumber(item.requestCount)} 次 · ${formatCost(item.totalCostUsd)}`
  }))

  if (providerStats.value.length > 24) {
    items.push({
      title: `还有 ${providerStats.value.length - 24} 个 Provider 未显示`,
      description: "可在页面筛选后导出更细的报告",
      value: "",
      note: ""
    })
  }

  return items
}

function createReportModelItems() {
  const items = modelStats.value.slice(0, 24).map((item) => ({
    title: item.model || "未识别模型",
    description: `${formatAppName(item.appType)} · ${item.providerName || "未识别来源"}`,
    value: formatNumber(item.actualTokens),
    note: `缓存 ${formatNumber(item.cacheReadTokens)} · ${formatCost(item.totalCostUsd)}`
  }))

  if (modelStats.value.length > 24) {
    items.push({
      title: `还有 ${modelStats.value.length - 24} 个模型未显示`,
      description: "可在页面筛选后导出更细的报告",
      value: "",
      note: ""
    })
  }

  return items
}

function createReportListHeight(items) {
  return items.length ? 82 + items.length * 66 + 12 : 136
}

function loadReportCanvasImage(element) {
  const canvas = element?.querySelector("canvas")

  if (!canvas) {
    return Promise.resolve(null)
  }

  return loadReportImage(canvas.toDataURL("image/png"))
}

function loadReportImage(dataUrl) {
  return new Promise((resolve, reject) => {
    const image = new Image()

    image.onload = () => resolve(image)
    image.onerror = () => reject(new Error("用量报告图表读取失败。"))
    image.src = dataUrl
  })
}

function createExportPixelRatio(width, height) {
  const pixelRatio = Math.min(window.devicePixelRatio || 1, 2)
  const maxPixels = 32000000
  const scaledPixels = width * height * pixelRatio * pixelRatio

  if (scaledPixels <= maxPixels) {
    return pixelRatio
  }

  return Math.max(1, Math.sqrt(maxPixels / (width * height)))
}

async function exportUsageReport() {
  reportExporting.value = true

  try {
    const filterPayload = createFilterPayload()
    const targetPath = await systemApi.saveFile({
      title: "保存用量报告长图",
      defaultPath: `usage-report-${createExportTimestamp()}.png`,
      filters: [{ name: "PNG 图片", extensions: ["png"] }]
    })

    if (!targetPath) {
      return
    }

    const imageData = await createUsageReportImageData()
    const result = await usageApi.exportUsageReportImage({
      targetPath,
      imageData,
      rangeType: filterPayload.startAt ? rangeType.value : "all",
      startAt: filterPayload.startAt,
      endAt: filterPayload.endAt,
      appType: appType.value,
      providerId: providerId.value,
      requestSource: requestSource.value,
      model: model.value,
      displayCurrency: displayCurrency.value,
      filters: [
        rangeTypeLabel.value,
        selectedAppLabel.value,
        selectedProviderLabel.value,
        selectedRequestSourceLabel.value,
        selectedModelLabel.value
      ]
    })

    if (!result?.data?.canceled) {
      createMessage.success("用量报告长图已导出。")
    }
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    reportExporting.value = false
  }
}

function renderCharts() {
  renderTrendChart()
  renderProviderPie()
}

function renderTrendChart() {
  if (!trendChartRef.value || !tokenTrendSeries.value.length) {
    return
  }

  trendChart = trendChart || echarts.init(trendChartRef.value)
  trendChart.setOption(
    {
      color: [
        "#2f5f91",
        "#4f8f7b",
        "#9f6b3d",
        "#7b6ea8",
        "#b05c5c",
        "#5d7fa4",
        "#8aa7c4",
        "#c2a64c"
      ],
      tooltip: {
        trigger: "axis",
        appendToBody: true,
        valueFormatter: (value) => formatNumber(value)
      },
      grid: {
        top: 28,
        right: 18,
        bottom: 28,
        left: 48
      },
      legend: {
        type: "scroll",
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
      series: tokenTrendSeries.value.map((item) => ({
        name: item.name,
        type: "line",
        smooth: true,
        symbolSize: 5,
        data: item.data
      }))
    },
    { notMerge: true }
  )
}

function renderProviderPie() {
  if (!providerPieRef.value || !usagePieStats.value.length) {
    return
  }

  providerPie = providerPie || echarts.init(providerPieRef.value)
  providerPie.setOption(
    {
      color: ["#2f5f91", "#5d7fa4", "#8aa7c4", "#b9c9d8", "#d8e2ec"],
      tooltip: {
        trigger: "item",
        appendToBody: true,
        formatter: (item) => {
          return `${item.name}<br />${formatNumber(item.value)} Tokens · ${item.percent}%`
        }
      },
      legend: {
        type: "scroll",
        orient: "vertical",
        right: 0,
        top: 0,
        itemWidth: 10,
        itemHeight: 10,
        formatter: (name) => {
          return name.length > 24 ? `${name.slice(0, 24)}...` : name
        },
        textStyle: {
          color: "#5f7087",
          fontSize: 11,
          width: 150,
          overflow: "truncate"
        }
      },
      series: [
        {
          type: "pie",
          right: 170,
          radius: ["42%", "68%"],
          center: ["50%", "52%"],
          avoidLabelOverlap: true,
          label: {
            color: "#14213a",
            formatter: "{d}%"
          },
          labelLine: {
            length: 10,
            length2: 8
          },
          data: usagePieStats.value
        }
      ]
    },
    { notMerge: true }
  )
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
  return String(value || "").trim() || "未分类"
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

function formatFilterDateTime(value) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(value))
}

function formatExportDateTime(value) {
  return value ? new Date(value).toLocaleString("zh-CN") : "未记录"
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

function formatRequestSourceOption(value) {
  const names = {
    "proxy-managed": "代理接管",
    "provider-instance": "独立实例",
    session: "会话日志"
  }

  return names[value] || value || "会话日志"
}

function formatRequestSource(item) {
  if (item.requestSource === "provider-instance" || item.instanceProviderId) {
    return `独立实例：${
      item.instanceProviderName ||
      item.providerName ||
      item.instanceProviderId ||
      "未知 Provider"
    }`
  }

  if (item.requestSource === "proxy-managed") {
    return "代理接管"
  }

  return "会话日志"
}

function formatSessionLabel(item) {
  return item.sessionTitle || item.sessionId || "-"
}
</script>

<style scoped lang="less">
:global(body.usage-report-exporting .app-shell) {
  display: block;
  width: auto;
  height: auto;
  min-height: 0;
  transform: none;
}

:global(body.usage-report-exporting .app-sidebar) {
  display: none;
}

:global(body.usage-report-exporting .app-shell__main),
:global(body.usage-report-exporting .app-shell__content) {
  height: auto;
  min-height: 0;
  overflow: visible;
}

:global(body.usage-report-exporting .app-shell__content) {
  padding-right: 0;
}

:global(body.usage-report-exporting .usage-view) {
  height: auto;
  min-height: 0;
  overflow: visible;
  padding-right: 0;
}

:global(body.usage-report-exporting .usage-view__toolbar) {
  position: static;
  margin-right: 0;
}

:global(body.usage-report-exporting .usage-view__report-button) {
  display: none;
}

:global(body.usage-report-exporting .usage-view__logs) {
  display: none;
}

:global(body.usage-report-exporting .usage-view__grid .usage-view__panel) {
  overflow: visible;
}

:global(body.usage-report-exporting .usage-view__stat-list) {
  max-height: none;
  overflow: visible;
  padding-right: 0;
}

.usage-view {
  position: relative;
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  overflow-x: hidden;
  overflow-y: auto;
  padding-right: 4px;
  color: var(--color-text);

  &.usage-view-loading-active {
    overflow-y: hidden;
  }

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
  &__pricing-export select,
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
    grid-template-columns: minmax(390px, 1.65fr) repeat(4, minmax(0, 1fr));
    align-items: flex-end;
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
    width: 100%;
    min-width: 0;
    padding: 0 10px;
    color: var(--color-text);
  }

  &__field input {
    min-width: 0;
    padding: 0 10px;
    color: var(--color-text);
  }

  &__field :deep(.el-date-editor.el-input__wrapper) {
    width: 100%;
    height: 36px;
    min-width: 0;
    border-radius: 8px;
    background: #fff;
    box-shadow: 0 0 0 1px var(--color-line) inset;
  }

  &__field :deep(.el-date-editor.el-input__wrapper.is-focus) {
    box-shadow: 0 0 0 1px var(--color-primary) inset;
  }

  &__field :deep(.el-range-input) {
    color: var(--color-text);
    font-size: 0.8rem;
  }

  &__field :deep(.el-range-separator) {
    flex: none;
    color: var(--color-text-muted);
    font-size: 0.78rem;
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

  &__chart-box {
    position: relative;
    height: 260px;
    min-width: 0;
  }

  &__chart-box &__empty {
    height: 260px;
    min-height: 0;
  }

  .usage-view-loading {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: rgba(248, 251, 255, 0.82);
    color: var(--color-primary);
    font-size: 0.92rem;
    font-weight: 700;

    .usage-view-loading-icon {
      animation: usage-loading-spin 0.8s linear infinite;
    }
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

  &__section-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__section-actions button {
    display: inline-flex;
    height: 32px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__section-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  &__chart-tabs {
    display: inline-flex;
    flex: none;
    gap: 4px;
    padding: 3px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__chart-tab {
    display: inline-flex;
    height: 26px;
    align-items: center;
    justify-content: center;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.74rem;
    font-weight: 800;
  }

  &__chart-tab--active {
    background: var(--color-primary);
    color: #ffffff;
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
    max-height: 320px;
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
      100px 72px 150px 150px 92px 130px minmax(160px, 1fr)
      86px 86px 86px 92px 92px;
    min-width: 1420px;
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

  &__modal--stack {
    z-index: 32;
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

  &__import-dialog {
    display: flex;
    width: 720px;
    max-height: 620px;
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

  &__pricing-toolbar-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__pricing-export {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
  }

  &__pricing-export select {
    width: 76px;
    padding: 0 8px;
    color: var(--color-text);
  }

  &__pricing-toolbar button,
  &__import-file button,
  &__dialog-actions button {
    padding: 0 12px;
  }

  &__file-input {
    display: none;
  }

  &__import-file {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  &__import-file button {
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

  &__import-file span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__import-tip {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.82rem;
    font-weight: 700;
  }

  &__import-textarea {
    height: 280px;
    min-height: 280px;
    resize: vertical;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    color: var(--color-text);
    font-family: Consolas, "Courier New", monospace;
    font-size: 0.78rem;
    line-height: 1.5;
    padding: 10px;
  }

  &__import-success {
    margin: 0;
    color: #197447;
    font-size: 0.82rem;
    font-weight: 800;
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
    overflow: hidden;
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
    text-overflow: ellipsis;
    white-space: nowrap;
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

@keyframes usage-loading-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

<template>
  <section class="skill-usage-view">
    <header class="skill-usage-view__toolbar">
      <div>
        <p class="skill-usage-view__eyebrow">Skill Usage</p>
        <h1>Skill 使用分析</h1>
      </div>

      <div class="skill-usage-view__actions">
        <button class="action-button" type="button" @click="$emit('back')">
          <ArrowLeft class="action-button__icon" :size="16" />
          返回 Skills
        </button>
        <button
          class="action-button"
          type="button"
          :disabled="pending"
          @click="loadStats"
        >
          <RefreshCw class="action-button__icon" :size="16" />
          {{ pending ? "分析中..." : "刷新分析" }}
        </button>
      </div>
    </header>

    <section class="skill-usage-view__filters">
      <label class="skill-usage-view__field">
        <span>时间范围</span>
        <el-date-picker
          v-model="dateTimeRange"
          type="datetimerange"
          :shortcuts="dateTimeShortcuts"
          :default-time="defaultDateTimeRange"
          clearable
          end-placeholder="结束时间"
          range-separator="至"
          start-placeholder="开始时间"
        />
      </label>
      <label class="skill-usage-view__field">
        <span>搜索</span>
        <input
          v-model.trim="searchQuery"
          type="text"
          placeholder="skill / provider / model"
        />
      </label>
      <label class="skill-usage-view__field">
        <span>CLI</span>
        <select v-model="cliFilter">
          <option value="all">全部</option>
          <option v-for="item in cliOptions" :key="item.id" :value="item.id">
            {{ item.name }}
          </option>
        </select>
      </label>
      <button
        class="action-button action-button--primary"
        type="button"
        :disabled="pending"
        @click="loadStats"
      >
        <BarChart3 class="action-button__icon" :size="16" />
        分析
      </button>
    </section>

    <div class="skill-usage-view__body-shell">
      <div v-if="pending" class="skill-usage-view__loading">
        <RefreshCw class="skill-usage-view__loading-icon" :size="22" />
        <span>正在分析 Skill 调用记录...</span>
      </div>
      <div class="skill-usage-view__body">
        <section class="skill-usage-view__metrics">
          <article class="skill-usage-view__metric">
            <span>Skill 总数</span>
            <strong>{{ formatNumber(summary.skillCount) }}</strong>
            <small>已使用 {{ formatNumber(summary.usedSkillCount) }} 个</small>
          </article>
          <article class="skill-usage-view__metric">
            <span>调用次数</span>
            <strong>{{ formatNumber(summary.usageCount) }}</strong>
            <small>命中请求 {{ formatNumber(summary.requestCount) }} 次</small>
          </article>
          <article class="skill-usage-view__metric">
            <span>Token 消耗</span>
            <strong>{{ formatNumber(summary.actualTokens) }}</strong>
            <small>按 Session 用量区间归因</small>
          </article>
          <article class="skill-usage-view__metric">
            <span>最近使用</span>
            <strong>{{ formatDate(summary.lastUsedAt) }}</strong>
            <small>来自 history / session 记录</small>
          </article>
        </section>

        <section class="skill-usage-view__chart-grid">
          <section class="skill-usage-view__chart-panel">
            <div class="skill-usage-view__section-header">
              <div>
                <h2>调用趋势</h2>
                <span>{{ skillTrendLabel }}</span>
              </div>
              <BarChart3 :size="18" />
            </div>
            <div
              v-show="skillTrendSeries.length"
              ref="trendChartRef"
              class="skill-usage-view__chart"
              @wheel="handleTrendWheel"
            ></div>
            <div v-if="!skillTrendSeries.length" class="skill-usage-view__empty">
              暂无 Skill 调用趋势。
            </div>
          </section>

          <section class="skill-usage-view__chart-panel">
            <div class="skill-usage-view__section-header">
              <div>
                <h2>Skill 调用占比</h2>
                <span>{{ skillPieLabel }}</span>
              </div>
              <PieChartIcon :size="18" />
            </div>
            <div
              v-show="skillPieStats.length"
              ref="skillPieRef"
              class="skill-usage-view__chart"
            ></div>
            <div v-if="!skillPieStats.length" class="skill-usage-view__empty">
              暂无 Skill 调用占比。
            </div>
          </section>
        </section>

        <div class="skill-usage-view__meta">
          <span>{{ filteredSkills.length }} / {{ rows.length }} 个 Skill</span>
          <span>{{ diagnostics.length }} 条解析诊断</span>
        </div>

        <section v-if="filteredSkills.length" class="skill-usage-view__table">
          <div class="skill-usage-view__table-head">
            <span>Skill</span>
            <span>CLI</span>
            <span>调用</span>
            <span>Token</span>
            <span>Provider</span>
            <span>模型</span>
            <span>最近使用</span>
          </div>
          <article
            v-for="item in filteredSkills"
            :key="item.name"
            class="skill-usage-view__table-row"
          >
            <span>
              <strong>{{ item.name }}</strong>
              <small :title="item.sourcePaths.join('\n')">
                {{ item.description || item.sourcePaths[0] || "未记录来源" }}
              </small>
            </span>
            <span>{{ formatCliList(item.cliTypes) }}</span>
            <span>{{ formatNumber(item.usageCount) }}</span>
            <span>
              <strong>{{ formatNumber(item.actualTokens) }}</strong>
              <small>{{ formatCost(item.totalCostUsd) }}</small>
            </span>
            <span
              class="skill-usage-view__usage-list"
              :title="formatProviderTitle(item.providers)"
            >
              <small
                v-for="provider in formatUsageItems(item.providers, 'provider')"
                :key="provider.key"
              >
                {{ provider.label }}
              </small>
            </span>
            <span
              class="skill-usage-view__usage-list"
              :title="formatModelTitle(item.models)"
            >
              <small
                v-for="model in formatUsageItems(item.models, 'model')"
                :key="model.key"
              >
                {{ model.label }}
              </small>
            </span>
            <span>{{ formatDate(item.lastUsedAt) }}</span>
          </article>
        </section>

        <div v-else class="skill-usage-view__empty">
          暂无匹配的 Skill 使用统计。
        </div>
      </div>
    </div>
  </section>
</template>

<script setup>
import { LineChart, PieChart as EchartsPieChart } from "echarts/charts"
import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components"
import * as echarts from "echarts/core"
import { CanvasRenderer } from "echarts/renderers"
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  ArrowLeft,
  BarChart3,
  PieChart as PieChartIcon,
  RefreshCw
} from "lucide-vue-next"
import { createMessage } from "@/utils/message"

echarts.use([
  LineChart,
  EchartsPieChart,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  CanvasRenderer
])

defineEmits(["back"])

const pending = ref(false)
const stats = ref({
  summary: {},
  skills: [],
  trends: [],
  filters: {
    clis: []
  },
  diagnostics: []
})
const searchQuery = ref("")
const cliFilter = ref("all")
const trendModeOverride = ref("")
const trendChartRef = ref(null)
const skillPieRef = ref(null)
let trendChart = null
let skillPie = null
const trendModeLevels = ["day", "hour", "minute"]
const dateTimeRange = ref(createPresetDateTimeRange("today"))
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

const rows = computed(() => stats.value.skills || [])
const summary = computed(() => stats.value.summary || {})
const trendStats = computed(() => stats.value.trends || [])
const diagnostics = computed(() => stats.value.diagnostics || [])
const cliOptions = computed(() => stats.value.filters?.clis || [])
const filteredSkills = computed(() => {
  const keyword = searchQuery.value.toLowerCase()

  if (!keyword) {
    return rows.value
  }

  return rows.value.filter(item => {
    const source = [
      item.name,
      item.description,
      formatCliList(item.cliTypes),
      formatProviderTitle(item.providers),
      formatModelTitle(item.models)
    ]
      .join(" ")
      .toLowerCase()

    return source.includes(keyword)
  })
})
const skillPieStats = computed(() =>
  filteredSkills.value
    .filter(item => item.usageCount > 0)
    .map(item => ({
      name: item.name,
      value: item.usageCount
    }))
)
const skillTrendNames = computed(() =>
  Array.from(
    new Set(
      trendStats.value.flatMap(item =>
        (item.skills || []).map(skill => skill.skillName)
      )
    )
  ).filter(name => filteredSkills.value.some(item => item.name === name))
)
const skillTrendSeries = computed(() =>
  skillTrendNames.value.map(name => ({
    name,
    type: "line",
    smooth: true,
    symbolSize: 5,
    data: trendStats.value.map(item => {
      const skill = (item.skills || []).find(
        skillItem => skillItem.skillName === name
      )

      return skill?.usageCount || 0
    })
  }))
)
const trendMode = computed(() => {
  if (trendModeOverride.value) {
    return trendModeOverride.value
  }

  const [start, end] = dateTimeRange.value || []

  return start && end && start.toDateString() === end.toDateString()
    ? "hour"
    : "day"
})
const skillTrendLabel = computed(() => {
  const skillCount = skillTrendNames.value.length

  if (trendMode.value === "minute") {
    return `${skillCount} 个 Skill · ${trendStats.value.length} 个分钟`
  }

  return trendMode.value === "hour"
    ? `${skillCount} 个 Skill · ${trendStats.value.length} 个小时`
    : `${skillCount} 个 Skill · ${trendStats.value.length} 个本地日`
})
const skillPieLabel = computed(
  () => `${skillPieStats.value.length} 个已使用 Skill`
)

onMounted(() => {
  loadStats()
  window.addEventListener("resize", resizeCharts)
})

onBeforeUnmount(() => {
  window.removeEventListener("resize", resizeCharts)
  trendChart?.dispose()
  skillPie?.dispose()
})

watch([dateTimeRange, cliFilter], () => {
  loadStats()
})

watch(searchQuery, async () => {
  await nextTick()
  renderCharts()
})

function createPresetDateTimeRange(type) {
  if (type === "all") {
    return []
  }

  const end = new Date()
  const start = new Date()

  if (type === "today") {
    start.setHours(0, 0, 0, 0)
  } else if (type === "week") {
    start.setDate(start.getDate() - 7)
  } else {
    start.setMonth(start.getMonth() - 1)
  }

  return [start, end]
}

function createPayload() {
  const [start, end] = dateTimeRange.value || []

  return {
    cli: cliFilter.value,
    trendMode: trendMode.value,
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

async function loadStats() {
  pending.value = true

  try {
    const result = await window.aiManager.getSkillUsageStats(createPayload())
    stats.value = result.data || stats.value
    await nextTick()
    renderCharts()
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    pending.value = false
  }
}

function renderCharts() {
  renderTrendChart()
  renderSkillPie()
}

function renderTrendChart() {
  if (!trendChartRef.value || !skillTrendSeries.value.length) {
    return
  }

  trendChart = trendChart || echarts.init(trendChartRef.value)
  trendChart.setOption(
    {
      color: ["#2f5f91", "#4f8f7b", "#9f6b3d", "#7b6ea8", "#b05c5c"],
      tooltip: {
        trigger: "axis",
        appendToBody: true,
        valueFormatter: value => `${formatNumber(value)} 次`
      },
      legend: {
        type: "scroll",
        top: 0,
        right: 8,
        itemWidth: 10,
        itemHeight: 10,
        textStyle: {
          color: "#5f7087",
          fontSize: 11
        }
      },
      grid: {
        top: 32,
        right: 18,
        bottom: 28,
        left: 44
      },
      xAxis: {
        type: "category",
        data: trendStats.value.map(item => item.date),
        axisTick: { show: false },
        axisLine: { lineStyle: { color: "#dbe5ed" } },
        axisLabel: { color: "#5f7087" }
      },
      yAxis: {
        type: "value",
        splitLine: { lineStyle: { color: "#edf2f8" } },
        axisLabel: {
          color: "#5f7087",
          formatter: value => formatCompactNumber(value)
        }
      },
      series: skillTrendSeries.value
    },
    { notMerge: true }
  )
}

function renderSkillPie() {
  if (!skillPieRef.value || !skillPieStats.value.length) {
    return
  }

  skillPie = skillPie || echarts.init(skillPieRef.value)
  skillPie.setOption(
    {
      color: ["#2f5f91", "#4f8f7b", "#9f6b3d", "#7b6ea8", "#b05c5c"],
      tooltip: {
        trigger: "item",
        appendToBody: true,
        formatter: item => {
          return `${item.name}<br />${formatNumber(item.value)} 次 · ${item.percent}%`
        }
      },
      legend: {
        type: "scroll",
        orient: "vertical",
        right: 0,
        top: 0,
        itemWidth: 10,
        itemHeight: 10,
        formatter: name => {
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
          data: skillPieStats.value
        }
      ]
    },
    { notMerge: true }
  )
}

function resizeCharts() {
  trendChart?.resize()
  skillPie?.resize()
}

function formatNumber(value) {
  return new Intl.NumberFormat("zh-CN").format(Number(value || 0))
}

function formatCost(value) {
  const cost = Number(value || 0)

  if (!cost) {
    return "$0"
  }

  return `$${cost >= 1 ? cost.toFixed(2) : cost.toFixed(6)}`
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

function formatDate(value) {
  const timestamp = Number(value || 0)

  if (!timestamp) {
    return "未使用"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(timestamp))
}

function formatCliList(items) {
  if (!items?.length) {
    return "集中管理"
  }

  return items.map(item => item.name || item.id).join(" / ")
}

function formatProviderTitle(items) {
  return (items || [])
    .map(item => `${item.providerName}：${formatNumber(item.actualTokens)}`)
    .join("\n")
}

function formatModelTitle(items) {
  return (items || [])
    .map(item => `${item.model}：${formatNumber(item.actualTokens)}`)
    .join("\n")
}

function formatUsageItems(items, type) {
  if (!items?.length) {
    return [
      {
        key: "empty",
        label: "未匹配"
      }
    ]
  }

  return items.map((item, index) => {
    const name = type === "provider" ? item.providerName : item.model

    return {
      key: `${name || "unknown"}-${index}`,
      label: `${name || "未匹配"}：${formatNumber(item.actualTokens)}`
    }
  })
}
</script>

<style scoped lang="less">
.skill-usage-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 10px;
  overflow: hidden;

  &__toolbar {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
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
    gap: 8px;
  }

  &__filters {
    display: grid;
    grid-template-columns: 320px minmax(0, 1fr) 150px 94px;
    flex: none;
    gap: 10px;
    align-items: end;
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
    font-size: 0.74rem;
    font-weight: 700;
  }

  &__field input,
  &__field select {
    height: 36px;
    min-width: 0;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    padding: 0 10px;
    color: var(--color-text);
    font: inherit;
    font-size: 0.84rem;
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

  &__body-shell {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  &__body {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    padding-right: 4px;
  }

  &__loading {
    position: absolute;
    inset: 0;
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: rgba(248, 251, 255, 0.78);
    color: var(--color-primary);
    font-size: 0.92rem;
    font-weight: 700;
  }

  &__loading-icon {
    animation: skill-usage-loading-spin 1s linear infinite;
  }

  &__metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    flex: none;
    gap: 10px;
  }

  &__metric {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
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
    font-size: 1.18rem;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__chart-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    flex: none;
    gap: 10px;
  }

  &__chart-panel {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__section-header h2 {
    margin: 0 0 4px;
    font-size: 0.95rem;
    line-height: 1.25;
  }

  &__section-header span {
    color: var(--color-text-muted);
    font-size: 0.74rem;
  }

  &__chart {
    width: 100%;
    height: 230px;
    min-width: 0;
  }

  &__meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
  }

  &__table {
    display: flex;
    flex: none;
    min-height: 260px;
    flex-direction: column;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__table-head,
  &__table-row {
    display: grid;
    box-sizing: border-box;
    grid-template-columns: 1fr 130px 82px 130px 220px 260px 120px;
    width: 100%;
    min-width: 1280px;
    gap: 10px;
    align-items: center;
    min-height: 42px;
    padding: 0 12px;
    border-bottom: 1px solid var(--color-line);
    font-size: 0.78rem;
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

  &__table-row > span {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__table-row strong,
  &__table-row small {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__table-row small {
    color: var(--color-text-muted);
    font-size: 0.72rem;
    line-height: 1.35;
  }

  &__usage-list {
    align-self: stretch;
    justify-content: center;
  }

  &__empty {
    display: grid;
    flex: none;
    min-height: 180px;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
  }
}

@keyframes skill-usage-loading-spin {
  from {
    transform: rotate(0deg);
  }

  to {
    transform: rotate(360deg);
  }
}

.action-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #fbfcfd;
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.88rem;
  font-weight: 600;

  &:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }

  &__icon {
    flex: 0 0 auto;
  }

  &--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  &--primary:hover {
    border-color: #2a4f6f;
    background: #2a4f6f;
  }
}
</style>

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
        @click="loadStats"
      >
        <BarChart3 class="action-button__icon" :size="16" />
        分析
      </button>
    </section>

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
        <span :title="formatProviderTitle(item.providers)">
          <small
            v-for="provider in formatUsageItems(item.providers, 'provider')"
            :key="provider.key"
          >
            {{ provider.label }}
          </small>
        </span>
        <span :title="formatModelTitle(item.models)">
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
  </section>
</template>

<script setup>
import { computed, onMounted, ref, watch } from "vue"
import { ArrowLeft, BarChart3, RefreshCw } from "lucide-vue-next"
import { createMessage } from "@/utils/message"

defineEmits(["back"])

const pending = ref(false)
const stats = ref({
  summary: {},
  skills: [],
  filters: {
    clis: []
  },
  diagnostics: []
})
const searchQuery = ref("")
const cliFilter = ref("all")
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

onMounted(() => {
  loadStats()
})

watch([dateTimeRange, cliFilter], () => {
  loadStats()
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
    startAt: start ? start.getTime() : 0,
    endAt: end ? end.getTime() : 0
  }
}

async function loadStats() {
  pending.value = true

  try {
    const result = await window.aiManager.getSkillUsageStats(createPayload())
    stats.value = result.data || stats.value
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    pending.value = false
  }
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
    flex: 1;
    min-height: 0;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__table-row small {
    color: var(--color-text-muted);
    font-size: 0.72rem;
  }

  &__empty {
    display: grid;
    flex: 1;
    min-height: 0;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
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

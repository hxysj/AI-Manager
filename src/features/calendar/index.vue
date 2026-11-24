<template>
  <section class="calendar-view" aria-label="日历">
    <QuickNoteCalendar
      :notes="entries.notes"
      :reminders="entries.reminders"
      :rules="entries.calendarRules"
      :usage-by-date="usageByDate"
      :usage-loading="usageLoading"
      :usage-error="usageError"
      :entries-loading="entriesLoading"
      :entries-error="entriesError"
      :public-holidays="publicHolidays"
      :assistant-settings="assistantSettings"
      :providers="providers"
      :codex-accounts="codexAccounts"
      :runtime-models="runtimeModels"
      @range-change="changeRange"
      @refresh="refresh"
      @open="$emit('open-notes', { id: $event.id })"
      @create="$emit('open-notes', { date: $event })"
      @smart-record="$emit('open-notes', { smart: true })"
    />
  </section>
</template>

<script setup>
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowReactive,
  shallowRef
} from "vue"
import { quickNotesApi } from "@/api/modules/quick-notes"
import { usageApi } from "@/api"
import QuickNoteCalendar from "@/features/quick-notes/components/QuickNoteCalendar.vue"
import {
  bundledPublicHolidayYear,
  loadPublicHolidayYear
} from "./public-holidays"

defineProps({
  assistantSettings: { type: Object, default: () => ({}) },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] }
})
defineEmits(["open-notes"])
const cacheLifetime = 30_000
const rangeCache = new Map()
const entries = shallowRef({ notes: [], reminders: [], calendarRules: [] })
const usageByDate = shallowRef(new Map())
const entriesLoading = ref(false)
const usageLoading = ref(false)
const entriesError = ref("")
const usageError = ref("")
const visibleYears = shallowRef([])
const holidayYears = shallowReactive(new Map())
const publicHolidays = computed(
  () =>
    new Map(
      visibleYears.value
        .flatMap(
          (year) => holidayYears.get(year) ?? bundledPublicHolidayYear(year)
        )
        .map((day) => [day.date, day])
    )
)
let range
let currentCache
let entriesRequest = 0
let usageRequest = 0
let holidayRequest = 0
let disposed = false
let unsubscribe

// 便签和用量独立读取，忽略快速翻月后迟到的结果。
async function loadEntries() {
  if (!range) return
  const request = ++entriesRequest
  entriesLoading.value = !currentCache.entries
  entriesError.value = ""
  try {
    const result = await quickNotesApi.invoke("calendar-entries", range)
    if (request === entriesRequest) {
      currentCache.entries = result
      currentCache.entriesAt = Date.now()
      entries.value = result
    }
  } catch (error) {
    if (request === entriesRequest)
      entriesError.value = error.message || String(error)
  } finally {
    if (request === entriesRequest) entriesLoading.value = false
  }
}

async function loadUsage() {
  const request = ++usageRequest
  usageLoading.value = !currentCache.usage
  usageError.value = ""
  try {
    const result = await usageApi.getUsageStats({
      statsScope: "calendar",
      startAt: range.startAt,
      endAt: range.endAt - 1
    })
    if (result.status !== "ok")
      throw new Error(result.message || "读取用量失败")
    if (request === usageRequest) {
      const usage = new Map(
        result.data.trends.map((item) => {
          const date = item.date
            .split("/")
            .map((part) => part.padStart(2, "0"))
            .join("-")
          return [date, item]
        })
      )
      currentCache.usage = usage
      currentCache.usageAt = Date.now()
      usageByDate.value = usage
    }
  } catch {
    if (request === usageRequest) usageError.value = "用量读取失败，可刷新重试"
  } finally {
    if (request === usageRequest) usageLoading.value = false
  }
}

function loadHolidays(reload = false) {
  // 每个年份独立更新，跨年时不会因为次年尚未发布而影响已知安排。
  const request = ++holidayRequest
  for (const year of visibleYears.value) {
    loadPublicHolidayYear(year, reload).then((days) => {
      if (days && request === holidayRequest && holidayYears.get(year) !== days)
        holidayYears.set(year, days)
    })
  }
}

function refresh() {
  if (!range) return
  loadEntries()
  loadUsage()
  loadHolidays(true)
}

function changeRange({ year, ...next }) {
  if (range?.startAt === next.startAt && range.endAt === next.endAt) return
  range = next
  entriesRequest++
  usageRequest++
  entriesLoading.value = false
  usageLoading.value = false
  entriesError.value = ""
  usageError.value = ""

  // 按所见日期范围保留最近八份快照，翻回时先显示已有数据。
  const key = `${next.startAt}:${next.endAt}`
  currentCache = rangeCache.get(key) || {}
  rangeCache.delete(key)
  rangeCache.set(key, currentCache)
  if (rangeCache.size > 8) rangeCache.delete(rangeCache.keys().next().value)
  usageByDate.value = currentCache.usage || new Map()
  entries.value = currentCache.entries || {
    notes: [],
    reminders: [],
    calendarRules: entries.value.calendarRules
  }
  const now = Date.now()
  if (!currentCache.entries || now - currentCache.entriesAt >= cacheLifetime)
    loadEntries()
  if (!currentCache.usage || now - currentCache.usageAt >= cacheLifetime)
    loadUsage()

  const years = [...new Set([Number(next.from.slice(0, 4)), year, year + 1])]
  if (years.join(",") !== visibleYears.value.join(",")) {
    visibleYears.value = years
    loadHolidays()
  }
}

function invalidateEntries() {
  // 写入后所有月份的便签快照失效，当前月份立即重新读取。
  for (const item of rangeCache.values()) delete item.entries
  loadEntries()
}

onMounted(async () => {
  try {
    const stop = await quickNotesApi.onChanged(invalidateEntries)
    if (disposed) stop()
    else unsubscribe = stop
  } catch (error) {
    if (!disposed) entriesError.value = error.message || String(error)
  }
})
onBeforeUnmount(() => {
  disposed = true
  entriesRequest++
  usageRequest++
  holidayRequest++
  unsubscribe?.()
})
</script>

<style scoped lang="less">
.calendar-view {
  display: flex;
  width: 100%;
  height: 100%;
  min-height: 0;
  container-type: inline-size;
}
</style>

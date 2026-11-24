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
      @range-change="changeRange"
      @refresh="refresh(true)"
      @open="$emit('open-notes', { id: $event.id })"
      @create="$emit('open-notes', { date: $event })"
      @smart-record="$emit('open-notes', { smart: true })"
    />
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, shallowReactive, shallowRef } from "vue"
import { quickNotesApi } from "@/api/modules/quick-notes"
import { usageApi } from "@/api"
import QuickNoteCalendar from "@/features/quick-notes/components/QuickNoteCalendar.vue"
import { bundledPublicHolidayYear, loadPublicHolidayYear } from "./public-holidays"

defineEmits(["open-notes"])
const entries = shallowRef({ notes: [], reminders: [], calendarRules: [] })
const usageByDate = shallowRef(new Map())
const entriesLoading = ref(false)
const usageLoading = ref(false)
const entriesError = ref("")
const usageError = ref("")
const visibleYears = shallowRef([])
const holidayYears = shallowReactive(new Map())
const publicHolidays = computed(() => new Map(
  visibleYears.value
    .flatMap((year) => holidayYears.get(year) ?? bundledPublicHolidayYear(year))
    .map((day) => [day.date, day])
))
let range
let entriesRequest = 0
let usageRequest = 0
let holidayRequest = 0
let disposed = false
let unsubscribe

// 便签和用量独立读取，忽略快速翻月后迟到的结果。
async function loadEntries() {
  if (!range) return
  const request = ++entriesRequest
  entriesLoading.value = true
  entriesError.value = ""
  try {
    const result = await quickNotesApi.invoke("calendar-entries", range)
    if (request === entriesRequest) entries.value = result
  } catch (error) {
    if (request === entriesRequest)
      entriesError.value = error.message || String(error)
  } finally {
    if (request === entriesRequest) entriesLoading.value = false
  }
}

async function loadUsage() {
  const request = ++usageRequest
  usageLoading.value = true
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
      usageByDate.value = new Map(
        result.data.trends.map((item) => {
          const date = item.date
            .split("/")
            .map((part) => part.padStart(2, "0"))
            .join("-")
          return [date, item]
        })
      )
    }
  } catch {
    if (request === usageRequest) usageError.value = "用量读取失败，可刷新重试"
  } finally {
    if (request === usageRequest) usageLoading.value = false
  }
}

function refresh(reloadHolidays = false) {
  loadEntries()
  loadUsage()
  // 每个年份独立更新，跨年时不会因为次年尚未发布而影响已知安排。
  const request = ++holidayRequest
  for (const year of visibleYears.value) {
    loadPublicHolidayYear(year, reloadHolidays).then((days) => {
      if (days && request === holidayRequest) holidayYears.set(year, days)
    })
  }
}

function changeRange({ year, ...next }) {
  range = next
  visibleYears.value = [...new Set([Number(next.from.slice(0, 4)), year, year + 1])]
  usageByDate.value = new Map()
  entries.value = {
    notes: [],
    reminders: [],
    calendarRules: entries.value.calendarRules
  }
  refresh()
}

onMounted(async () => {
  try {
    const stop = await quickNotesApi.onChanged(loadEntries)
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

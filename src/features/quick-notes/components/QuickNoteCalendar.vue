<template>
  <section class="note-calendar" aria-label="日历工作区">
    <div class="calendar-month">
      <header class="calendar-topbar">
        <h1 class="calendar-title"><CalendarDays :size="23" />日历</h1>
        <div class="calendar-modes" role="group" aria-label="日历视图">
          <button
            v-for="mode in modes"
            :key="mode.id"
            class="calendar-mode"
            :class="{ 'calendar-mode-active': calendarView === mode.id }"
            :aria-pressed="calendarView === mode.id"
            type="button"
            @click="calendarView = mode.id"
          >
            <component :is="mode.icon" :size="16" />{{ mode.label }}
          </button>
        </div>
      </header>
      <div class="calendar-toolbar">
        <div class="calendar-period">
          <button
            class="calendar-icon"
            type="button"
            aria-label="上个月"
            @click="moveMonth(-1)"
          >
            <ChevronLeft :size="17" />
          </button>
          <select
            v-model.number="year"
            class="calendar-select"
            aria-label="日历年份"
            @change="moveMonth(0)"
          >
            <option v-for="value in years" :key="value" :value="value">
              {{ value }} 年
            </option>
          </select>
          <select
            v-model.number="month"
            class="calendar-select"
            aria-label="日历月份"
            @change="moveMonth(0)"
          >
            <option v-for="value in 12" :key="value" :value="value - 1">
              {{ value }} 月
            </option>
          </select>
          <button
            class="calendar-icon"
            type="button"
            aria-label="下个月"
            @click="moveMonth(1)"
          >
            <ChevronRight :size="17" />
          </button>
        </div>
        <div class="calendar-top-actions">
          <button
            class="calendar-button"
            type="button"
            aria-label="刷新日历"
            title="刷新日历"
            @click="$emit('refresh')"
          >
            <RefreshCw :size="15" />
          </button>
          <button
            class="calendar-button"
            type="button"
            @click="rulesVisible = true"
          >
            <Umbrella :size="15" />休假规则
          </button>
          <button
            class="calendar-button calendar-today-button"
            type="button"
            @click="selectDate(today)"
          >
            今天
          </button>
          <button
            class="calendar-button"
            type="button"
            :aria-label="agendaVisible ? '收起当日事项' : '展开当日事项'"
            :title="agendaVisible ? '收起当日事项' : '展开当日事项'"
            :aria-expanded="agendaVisible"
            aria-controls="calendar-agenda"
            @click="agendaVisible = !agendaVisible"
          >
            <component
              :is="agendaVisible ? PanelRightClose : PanelRightOpen"
              :size="17"
            />
          </button>
        </div>
      </div>
      <p v-if="entriesError" class="calendar-error" role="alert">
        {{ entriesError }}
      </p>
      <div class="calendar-board">
        <div
          v-if="calendarView !== 'list'"
          class="calendar-week"
          aria-hidden="true"
        >
          <span
            v-for="(day, index) in weekdays"
            :key="day"
            class="calendar-weekday"
            :class="{ 'calendar-weekend': index > 4 }"
            >{{ day }}</span
          >
        </div>
        <div
          v-if="calendarView !== 'list'"
          class="calendar-days"
          :class="{ 'calendar-days-week': calendarView === 'week' }"
          :aria-label="`${year} 年 ${month + 1} 月`"
        >
          <button
            v-for="day in visibleDays"
            :key="day.key"
            class="calendar-day"
            :class="{
              'calendar-outside': day.outside,
              'calendar-weekend': day.schedule?.workStatus === 'rest',
              'calendar-day-leave': day.leaves.length > 0,
              'calendar-today': day.key === today,
              'calendar-selected': day.key === selectedDate
            }"
            type="button"
            :data-date="day.key"
            :style="
              day.schedule && day.schedule.kind !== 'work'
                ? { '--calendar-day-color': day.schedule.color }
                : {}
            "
            :aria-label="`${day.key}，农历${day.lunar}${day.scheduleLabel ? `，${day.scheduleLabel}` : ''}${day.schedule?.source === 'public' ? `，${day.schedule.title}` : ''}${day.leaves.length ? `，${leaveSummary(day)}` : ''}，${day.entries.length} 项事项${day.usage ? `，${day.usage.requestCount} 次模型请求` : ''}`"
            :aria-current="day.key === today ? 'date' : undefined"
            :aria-pressed="day.key === selectedDate"
            @click="selectDate(day.key)"
            @contextmenu.prevent="openDayMenu($event, day)"
            @keydown.shift.f10.prevent="openDayMenu($event, day)"
          >
            <span class="calendar-number">{{ day.number }}</span>
            <span class="calendar-lunar">{{ day.lunar }}</span>
            <span
              v-if="
                day.leaves.length ||
                day.scheduleLabel ||
                day.entries.length ||
                day.usage
              "
              class="calendar-corner"
            >
              <span
                v-if="day.leaves.length"
                class="calendar-leave-mark"
                :title="leaveSummary(day)"
              >
                请假
              </span>
              <span
                v-if="day.scheduleLabel"
                class="calendar-schedule-label"
                :style="{ '--calendar-schedule-color': day.schedule.color }"
                :title="day.schedule.title"
                >{{ day.scheduleLabel }}</span
              >
              <span
                v-if="day.entries.length || day.usage"
                class="calendar-markers"
              >
                <span
                  v-if="day.entries.some((item) => item.kind === 'note')"
                  class="calendar-dot calendar-dot-note"
                ></span>
                <span
                  v-if="day.entries.some((item) => item.kind === 'task')"
                  class="calendar-dot calendar-dot-task"
                ></span>
                <span
                  v-if="day.entries.some((item) => item.kind === 'reminder')"
                  class="calendar-dot calendar-dot-reminder"
                ></span>
                <span
                  v-if="day.usage"
                  class="calendar-dot calendar-dot-usage"
                ></span>
                <span v-if="day.entries.length" class="calendar-day-count">{{
                  day.entries.length
                }}</span>
              </span>
            </span>
            <span v-if="calendarView === 'week'" class="calendar-week-entries"
              ><span
                v-for="entry in day.entries.slice(0, 5)"
                :key="entry.id"
                class="calendar-week-entry"
                >{{ entry.title }}</span
              ></span
            >
          </button>
        </div>
        <div v-else class="calendar-list" aria-label="本月事项列表">
          <section
            v-for="day in listDays"
            :key="day.key"
            class="calendar-list-day"
          >
            <button
              class="calendar-list-date"
              type="button"
              @click="selectDate(day.key)"
              @contextmenu.prevent="openDayMenu($event, day)"
              @keydown.shift.f10.prevent="openDayMenu($event, day)"
            >
              {{ day.key.slice(5) }}<span>农历 {{ day.lunar }}</span>
            </button>
            <span v-if="day.usage" class="calendar-list-usage">
              <BarChart3 :size="14" />模型用量 ·
              {{ day.usage.requestCount }} 次请求
            </span>
            <span
              v-if="day.schedule?.source === 'public'"
              class="calendar-list-schedule"
            >
              <CalendarDays :size="14" />{{ day.schedule.title }} ·
              {{
                day.schedule.kind === "workday" ? "调休补班" : day.scheduleLabel
              }}
            </span>
            <button
              v-for="rule in day.leaves"
              :key="rule.id"
              class="calendar-list-entry calendar-leave-entry"
              type="button"
              @click="openLeave(day.key, rule, $event.currentTarget)"
            >
              <CalendarClock :size="14" /><span>请假</span
              ><span>{{ leaveDayLabel(rule, day.key) }}</span>
            </button>
            <button
              v-for="entry in day.entries"
              :key="entry.id"
              class="calendar-list-entry"
              type="button"
              @click="$emit('open', entry.note)"
            >
              <component
                :is="
                  entry.kind === 'task'
                    ? ListTodo
                    : entry.kind === 'reminder'
                      ? Bell
                      : NotebookPen
                "
                :size="14"
              /><span>{{ entry.title }}</span
              ><span>{{
                entry.at ? formatDateTime(entry.at).slice(-5) : "全天"
              }}</span>
            </button>
          </section>
          <p v-if="!listDays.length" class="calendar-list-empty">
            本月还没有安排，选一天写下新的记录。
          </p>
        </div>
        <footer class="calendar-legend">
          <span class="calendar-legend-item"
            ><i class="calendar-dot calendar-dot-note"></i>便签
            {{ monthCounts.note }}</span
          >
          <span class="calendar-legend-item"
            ><i class="calendar-dot calendar-dot-task"></i>待办
            {{ monthCounts.task }}</span
          >
          <span class="calendar-legend-item"
            ><i class="calendar-dot calendar-dot-reminder"></i>提醒
            {{ monthCounts.reminder }}</span
          >
          <span class="calendar-legend-item">
            <i class="calendar-dot calendar-dot-leave"></i>请假
            {{ leaveDayCount }} 天
          </span>
          <span class="calendar-legend-item">
            <i class="calendar-dot calendar-dot-usage"></i>模型用量
          </span>
          <span class="calendar-caption"
            >本月事项 · 共
            {{
              Object.values(monthCounts).reduce((sum, count) => sum + count, 0)
            }}
            项</span
          >
        </footer>
      </div>
    </div>
    <aside
      v-show="agendaVisible"
      id="calendar-agenda"
      class="calendar-agenda"
      aria-label="当日事项"
    >
      <header class="calendar-agenda-head">
        <span class="calendar-date-caption"
          >{{ selectedDate === today ? "今天" : "所选日期" }} ·
          {{ selectedWeekday }}</span
        >
        <time class="calendar-agenda-date" :datetime="selectedDate"
          >{{ Number(selectedDate.slice(5, 7)) }} 月
          {{ Number(selectedDate.slice(8)) }} 日</time
        >
        <span class="calendar-date-icon" aria-hidden="true">
          <CalendarDays :size="44" :stroke-width="1.5" />
        </span>
      </header>
      <CalendarAlmanac
        v-if="selectedDay"
        :lunar-date="selectedDay.lunarDate"
        :schedule="selectedDay.schedule"
        :next-holiday="nextHoliday"
      />
      <section class="calendar-quick-actions" aria-label="快捷操作">
        <h2 class="calendar-section-title"><Zap :size="16" />快捷操作</h2>
        <div class="calendar-agenda-actions">
          <button
            class="calendar-button"
            type="button"
            @click="$emit('smart-record')"
          >
            <Sparkles :size="13" />智能记录
          </button>
          <button
            class="calendar-button"
            type="button"
            aria-label="在这天新建便签"
            @click="$emit('create', selectedDate)"
          >
            <Plus :size="13" />新建便签
          </button>
          <button
            class="calendar-button"
            type="button"
            @click="openLeave(selectedDate, null, $event.currentTarget)"
          >
            <CalendarClock :size="13" />请假
          </button>
        </div>
      </section>
      <section
        class="calendar-usage"
        aria-label="当日模型用量"
        :aria-busy="usageLoading"
      >
        <h2 class="calendar-section-title"><BarChart3 :size="16" />模型用量</h2>
        <p
          v-if="usageLoading || usageError || !selectedDay?.usage"
          class="calendar-usage-message"
        >
          {{
            usageLoading ? "正在读取用量…" : usageError || "当天暂无用量记录"
          }}
        </p>
        <dl v-else class="calendar-usage-metrics">
          <div class="calendar-usage-metric">
            <dt>请求次数</dt>
            <dd>{{ selectedDay.usage.requestCount.toLocaleString() }}</dd>
          </div>
          <div class="calendar-usage-metric">
            <dt>实际 Token</dt>
            <dd :title="selectedDay.usage.actualTokens?.toLocaleString()">
              {{
                formatTokenCountParts(selectedDay.usage.actualTokens ?? null)
                  .compact
              }}
            </dd>
          </div>
          <div class="calendar-usage-metric">
            <dt>输出 Token</dt>
            <dd :title="selectedDay.usage.outputTokens?.toLocaleString()">
              {{
                formatTokenCountParts(selectedDay.usage.outputTokens ?? null)
                  .compact
              }}
            </dd>
          </div>
        </dl>
      </section>
      <section class="calendar-agenda-content" :aria-busy="entriesLoading">
        <header
          v-if="selectedEntries.length || selectedDay?.leaves.length"
          class="calendar-agenda-summary"
        >
          <h2 class="calendar-section-title">当日事项</h2>
          <span class="calendar-caption">
            {{ selectedEntries.length + (selectedDay?.leaves.length || 0) }} 项
          </span>
        </header>
        <div v-if="selectedDay?.leaves.length" class="calendar-agenda-leaves">
          <button
            v-for="rule in selectedDay?.leaves"
            :key="rule.id"
            class="calendar-agenda-leave"
            type="button"
            @click="openLeave(selectedDate, rule, $event.currentTarget)"
          >
            <CalendarClock :size="14" />请假 ·
            {{ leaveDayLabel(rule, selectedDate) }}
          </button>
        </div>
        <div v-if="selectedEntries.length" class="calendar-agenda-list">
          <button
            v-for="entry in selectedEntries"
            :key="entry.id"
            class="calendar-entry"
            :class="{ 'calendar-entry-done': entry.completed }"
            type="button"
            @click="$emit('open', entry.note)"
          >
            <span class="calendar-entry-type">
              <component
                :is="
                  entry.kind === 'reminder'
                    ? Bell
                    : entry.kind === 'task'
                      ? ListTodo
                      : NotebookPen
                "
                :size="13"
              />
              {{
                entry.kind === "reminder"
                  ? "提醒"
                  : entry.kind === "task"
                    ? entry.completed
                      ? "已完成"
                      : "待办截止"
                    : "便签"
              }}
              <span v-if="entry.at" class="calendar-entry-time">{{
                formatDateTime(entry.at).slice(-5)
              }}</span>
            </span>
            <span class="calendar-entry-title">{{ entry.title }}</span>
            <span class="calendar-entry-caption">{{
              entry.kind === "note" ? entry.note.content : entry.note.title
            }}</span>
            <span v-if="entry.note.tags?.length" class="calendar-entry-tags"
              ><span v-for="tag in entry.note.tags.slice(0, 3)" :key="tag"
                ># {{ tag }}</span
              ></span
            >
            <span class="calendar-entry-created"
              ><CalendarDays :size="12" />创建于
              {{ formatDateTime(entry.note.createdAt) }}</span
            >
          </button>
        </div>
        <div
          v-else-if="!selectedDay?.leaves.length"
          class="calendar-agenda-empty"
        >
          <span class="calendar-empty-icon" aria-hidden="true">
            <CalendarDays :size="30" :stroke-width="1.5" />
          </span>
          <span>{{
            entriesLoading ? "正在读取事项…" : "当天暂无便签、待办或提醒"
          }}</span>
        </div>
      </section>
    </aside>
    <span
      ref="dayMenuAnchor"
      class="calendar-menu-anchor"
      :style="dayMenuPosition"
      aria-hidden="true"
    ></span>
    <el-dropdown
      ref="dayMenu"
      virtual-triggering
      :virtual-ref="dayMenuAnchor"
      :trigger-keys="[]"
      trigger="contextmenu"
      placement="bottom-start"
      :show-arrow="false"
      @command="selectLeaveAction"
    >
      <template #dropdown>
        <el-dropdown-menu>
          <el-dropdown-item :icon="CalendarClock" command="new"
            >设置请假</el-dropdown-item
          >
          <el-dropdown-item
            v-for="rule in dayMenuDate?.leaves"
            :key="rule.id"
            :command="rule"
          >
            编辑请假 · {{ leaveDayLabel(rule, dayMenuDate.key) }}
          </el-dropdown-item>
        </el-dropdown-menu>
      </template>
    </el-dropdown>
    <Teleport to="body">
      <BaseModal
        v-if="rulesVisible"
        class="calendar-rules-modal"
        title="休假规则"
        role="dialog"
        aria-modal="true"
        aria-label="休假规则"
        @close="rulesVisible = false"
        @keydown.esc.stop="rulesVisible = false"
      >
        <CalendarRuleSettings
          :rules="rules"
          :assistant-settings="assistantSettings"
          :providers="providers"
          :codex-accounts="codexAccounts"
          :runtime-models="runtimeModels"
        />
      </BaseModal>
      <CalendarLeaveDialog
        v-if="leaveEditor"
        :date="leaveEditor.date"
        :rule="leaveEditor.rule"
        @close="closeLeave"
      />
    </Teleport>
  </section>
</template>

<script setup>
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch
} from "vue"
import {
  BarChart3,
  Bell,
  CalendarClock,
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  ListTodo,
  NotebookPen,
  Plus,
  Sparkles,
  CalendarRange,
  List,
  PanelRightClose,
  PanelRightOpen,
  RefreshCw,
  Umbrella,
  Zap
} from "lucide-vue-next"
import { ElDropdown, ElDropdownItem, ElDropdownMenu } from "element-plus"
import { Solar } from "lunar-javascript"
import { localInput } from "@/features/quick-notes/constant"
import { formatDateTime, formatTokenCountParts } from "@/utils/formatters"

import BaseModal from "@/components/BaseModal.vue"
import CalendarAlmanac from "./CalendarAlmanac.vue"
import CalendarRuleSettings from "./CalendarRuleSettings.vue"
import CalendarLeaveDialog from "./CalendarLeaveDialog.vue"
import { calendarDay, leaveDayLabel } from "../calendar-rules"
const rulesVisible = ref(false)
const agendaVisible = ref(true)
const props = defineProps({
  rules: { type: Array, default: () => [] },
  notes: { type: Array, default: () => [] },
  reminders: { type: Array, default: () => [] },
  usageByDate: { type: Map, default: () => new Map() },
  usageLoading: { type: Boolean, default: false },
  usageError: { type: String, default: "" },
  entriesLoading: { type: Boolean, default: false },
  entriesError: { type: String, default: "" },
  publicHolidays: { type: Map, default: () => new Map() },
  assistantSettings: { type: Object, default: () => ({}) },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] }
})
const emit = defineEmits([
  "open",
  "create",
  "smart-record",
  "range-change",
  "refresh"
])
const today = ref(localInput(Date.now()).slice(0, 10))
const selectedDate = ref(today.value)
const calendarView = ref("month")
const modes = [
  { id: "month", label: "月视图", icon: CalendarDays },
  { id: "week", label: "周视图", icon: CalendarRange },
  { id: "list", label: "列表", icon: List }
]
const year = ref(Number(today.value.slice(0, 4)))
const month = ref(Number(today.value.slice(5, 7)) - 1)
const dayMenu = ref(null)
const dayMenuAnchor = ref(null)
const dayMenuKey = ref("")
const dayMenuPosition = ref({})
const leaveEditor = shallowRef(null)
let leaveTrigger
const weekdays = ["一", "二", "三", "四", "五", "六", "日"]
const years = computed(() =>
  Array.from({ length: 21 }, (_, index) => year.value - 10 + index)
)

const firstGridDay = computed(() => {
  const date = new Date(year.value, month.value, 1)
  date.setDate(1 - ((date.getDay() + 6) % 7))
  return date
})
// 相邻月份的日期也在月格中，统一读取六周的本地日期范围。
watch(
  firstGridDay,
  (first) => {
    const until = new Date(
      first.getFullYear(),
      first.getMonth(),
      first.getDate() + 42
    )
    emit("range-change", {
      year: year.value,
      from: localInput(first.getTime()).slice(0, 10),
      until: localInput(until.getTime()).slice(0, 10),
      startAt: first.getTime(),
      endAt: until.getTime()
    })
  },
  { immediate: true }
)

// 便签、待办和提醒按本地日期归组。
const entriesByDate = computed(() => {
  const result = new Map()
  const append = (date, entry) => {
    if (!result.has(date)) result.set(date, [])
    result.get(date).push(entry)
  }
  const notes = new Map(props.notes.map((note) => [note.id, note]))
  for (const note of props.notes) {
    append(note.occurredOn || localInput(note.createdAt).slice(0, 10), {
      id: `note:${note.id}`,
      kind: "note",
      note,
      title: note.title,
      at: 0
    })
    for (const task of note.tasks || []) {
      if (task.status === "cancelled" || (!task.dueAt && !task.dueDate))
        continue
      append(task.dueAt ? localInput(task.dueAt).slice(0, 10) : task.dueDate, {
        id: `task:${note.id}:${task.id}`,
        kind: "task",
        note,
        title: task.title,
        at: task.dueAt || 0,
        completed: task.status === "completed"
      })
    }
  }
  for (const reminder of props.reminders) {
    const note = notes.get(reminder.noteId)
    if (!note || !reminder.enabled || !reminder.nextFireAt) continue
    append(localInput(reminder.nextFireAt).slice(0, 10), {
      id: `reminder:${reminder.id}`,
      kind: "reminder",
      note,
      title: note.title,
      at: reminder.nextFireAt
    })
  }
  for (const entries of result.values()) entries.sort((a, b) => a.at - b.at)
  return result
})
const days = computed(() => {
  const first = firstGridDay.value
  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(
      first.getFullYear(),
      first.getMonth(),
      first.getDate() + index,
      12
    )
    const key = localInput(date.getTime()).slice(0, 10)
    const lunarDate = Solar.fromYmd(
      date.getFullYear(),
      date.getMonth() + 1,
      date.getDate()
    ).getLunar()
    const { schedule, leaves } = calendarDay(
      date,
      props.rules,
      lunarDate,
      props.publicHolidays.get(key)
    )
    return {
      key,
      usage: props.usageByDate.get(key),
      schedule,
      leaves,
      // 日期格只展示简短状态，规则名称保留在标签悬浮提示和规则设置中。
      scheduleLabel:
        schedule && schedule.kind !== "work"
          ? {
              rest: "休息",
              holiday: schedule.source === "public" ? "放假" : "节假日",
              workday: "补班",
              makeup: schedule.workStatus === "work" ? "补班" : "补假"
            }[schedule.kind] || "标记"
          : "",
      number: date.getDate(),
      outside: date.getMonth() !== month.value,
      weekend: index % 7 > 4,
      lunarDate,
      lunar:
        lunarDate.getDay() === 1
          ? `${lunarDate.getMonthInChinese()}月`
          : lunarDate.getDayInChinese(),
      entries: entriesByDate.value.get(key) || []
    }
  })
})
const listDays = computed(() =>
  days.value.filter(
    (day) =>
      !day.outside &&
      (day.entries.length ||
        day.leaves.length ||
        day.usage ||
        day.schedule?.source === "public")
  )
)
const leaveDayCount = computed(
  () => days.value.filter((day) => !day.outside && day.leaves.length).length
)
const selectedDay = computed(() =>
  days.value.find((day) => day.key === selectedDate.value)
)
const nextHoliday = computed(() => {
  if (props.publicHolidays.get(selectedDate.value)?.kind === "holiday")
    return null
  const upcoming = [...props.publicHolidays.values()]
    .filter((day) => day.kind === "holiday" && day.date > selectedDate.value)
    .sort((a, b) => a.date.localeCompare(b.date))[0]
  if (!upcoming) return null
  // 公历日序相减，跨年或夏令时都按自然天计数。
  return {
    title: upcoming.title,
    year: upcoming.date.slice(0, 4),
    days:
      (Date.parse(upcoming.date) - Date.parse(selectedDate.value)) / 86400000
  }
})
const dayMenuDate = computed(() =>
  days.value.find((day) => day.key === dayMenuKey.value)
)
const monthCounts = computed(() => {
  const result = { note: 0, task: 0, reminder: 0 }
  for (const day of days.value) {
    if (!day.outside) for (const entry of day.entries) result[entry.kind]++
  }
  return result
})
const visibleDays = computed(() => {
  if (calendarView.value !== "week") return days.value
  const selected = days.value.findIndex((day) => day.key === selectedDate.value)
  return days.value.slice(
    Math.floor(selected / 7) * 7,
    Math.floor(selected / 7) * 7 + 7
  )
})
const selectedEntries = computed(
  () => entriesByDate.value.get(selectedDate.value) || []
)
const selectedWeekday = computed(() =>
  new Intl.DateTimeFormat("zh-CN", { weekday: "long" }).format(
    new Date(`${selectedDate.value}T12:00:00`)
  )
)

// 月份切换保留选中日，月底超出的日期收敛到当月最后一天。
function moveMonth(step) {
  const date = new Date(year.value, month.value + step, 1, 12)
  const last = new Date(date.getFullYear(), date.getMonth() + 1, 0).getDate()
  date.setDate(Math.min(Number(selectedDate.value.slice(8)), last))
  selectDate(localInput(date.getTime()).slice(0, 10))
}
function selectDate(date) {
  selectedDate.value = date
  year.value = Number(date.slice(0, 4))
  month.value = Number(date.slice(5, 7)) - 1
}

function leaveSummary(day) {
  return day.leaves
    .map((rule) => `请假 · ${leaveDayLabel(rule, day.key)}`)
    .join("；")
}

// 共用一个下拉菜单，定位到鼠标或键盘当前操作的日期。
async function openDayMenu(event, day) {
  const target = event.currentTarget
  const { left, bottom } = target.getBoundingClientRect()
  const pointer =
    event.type === "contextmenu" && (event.clientX || event.clientY)
  dayMenuPosition.value = {
    left: `${pointer ? event.clientX : left}px`,
    top: `${pointer ? event.clientY : bottom}px`
  }
  leaveTrigger = target
  dayMenuKey.value = day.key
  await nextTick()
  dayMenu.value?.handleOpen()
  dayMenu.value?.popperRef?.updatePopper()
}

function selectLeaveAction(rule) {
  if (dayMenuDate.value)
    openLeave(dayMenuDate.value.key, rule === "new" ? null : rule, leaveTrigger)
}

function openLeave(date, rule, trigger) {
  leaveTrigger = trigger
  leaveEditor.value = { date, rule }
}

function closeLeave() {
  leaveEditor.value = null
  leaveTrigger?.focus({ preventScroll: true })
}

let clock
onMounted(() => {
  clock = window.setInterval(() => {
    today.value = localInput(Date.now()).slice(0, 10)
  }, 60_000)
})
onBeforeUnmount(() => clearInterval(clock))
</script>

<style scoped lang="less">
.calendar-rules-modal {
  :deep(.base-modal__panel) {
    width: min(800px, 100%);
    height: min(590px, calc(100vh - 48px));
    border-radius: 12px;
  }
  :deep(.base-modal__header) {
    align-items: center;
    padding: 12px 18px;
    border-bottom: 1px solid var(--color-line);
  }
  :deep(.base-modal__header h2) {
    font: inherit;
    font-size: var(--font-size-lg);
  }
  :deep(.base-modal__close) {
    width: 28px;
    height: 28px;
    font-size: var(--font-size-lg);
  }
  :deep(.base-modal__content) {
    padding: 0;
  }
}

.note-calendar {
  --calendar-leave-color: #528bd5;
  --calendar-card-radius: 14px;
  --calendar-card-shadow: 0 3px 10px
    color-mix(in srgb, var(--color-primary-solid) 7%, transparent);
  --calendar-leave-text: color-mix(
    in srgb,
    var(--calendar-leave-color) 45%,
    var(--color-text)
  );
  --calendar-leave-tint: color-mix(
    in srgb,
    var(--calendar-leave-color) 7%,
    var(--color-panel)
  );
  --calendar-leave-chip: color-mix(
    in srgb,
    var(--calendar-leave-color) 28%,
    var(--color-panel)
  );
  display: flex;
  flex: 1;
  align-items: stretch;
  gap: 16px;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  scrollbar-width: thin;
  .calendar-error {
    margin: 0 0 8px;
    color: var(--color-danger);
    font-size: var(--font-size-sm);
  }
  .calendar-menu-anchor {
    position: fixed;
    width: 1px;
    height: 1px;
    pointer-events: none;
  }
  .calendar-button,
  .calendar-icon,
  .calendar-select {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--color-line);
    min-height: 34px;
    border-radius: 9px;
    padding: 6px 10px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: var(--font-size-sm);
    white-space: nowrap;
    cursor: pointer;
    &:hover {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }
    &:focus-visible {
      outline: 2px solid var(--color-primary);
      outline-offset: 3px;
    }
  }
  .calendar-icon {
    border-color: transparent;
    background: transparent;
    padding: 6px;
  }
  .calendar-caption {
    color: var(--color-text-muted);
    font-size: 11px;
  }
  .calendar-dot {
    display: inline-block;
    flex: none;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    &.calendar-dot-note {
      background: #6a9c88;
    }
    &.calendar-dot-task {
      background: #c49343;
    }
    &.calendar-dot-reminder {
      background: #7399c0;
    }
    &.calendar-dot-leave {
      background: var(--calendar-leave-color);
    }
    &.calendar-dot-usage {
      background: #9277ba;
    }
  }
  .calendar-month {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 464px;
    gap: 12px;
    .calendar-topbar,
    .calendar-toolbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      flex-wrap: wrap;
      flex: none;
      gap: 8px;
    }
    .calendar-topbar {
      min-height: 38px;
      .calendar-title {
        display: flex;
        align-items: center;
        gap: 9px;
        margin: 0;
        font-size: 23px;
        line-height: 1.25;
        :deep(svg) {
          color: var(--color-primary);
        }
      }
      .calendar-modes {
        display: inline-flex;
        gap: 2px;
        padding: 3px;
        border-radius: 10px;
        background: var(--color-panel-soft);
        .calendar-mode {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 5px;
          padding: 6px 9px;
          border: 0;
          border-radius: 7px;
          background: transparent;
          color: var(--color-text-muted);
          font: inherit;
          font-size: var(--font-size-sm);
          white-space: nowrap;
          cursor: pointer;
          &:hover,
          &.calendar-mode-active {
            color: var(--color-primary);
            background: color-mix(
              in srgb,
              var(--color-primary) 14%,
              var(--color-panel)
            );
          }
        }
      }
    }
    .calendar-toolbar {
      justify-content: flex-start;
      .calendar-period,
      .calendar-top-actions {
        display: flex;
        align-items: center;
        gap: 6px;
      }
      .calendar-top-actions {
        flex-wrap: wrap;
        margin-left: auto;
        .calendar-today-button {
          color: var(--color-primary);
          border-color: transparent;
          background: var(--color-primary-soft);
          &:hover {
            border-color: var(--color-primary);
          }
        }
      }
    }
    .calendar-board {
      display: flex;
      flex: 1;
      flex-direction: column;
      min-height: 0;
      padding: 8px 12px 12px;
      border: 1px solid var(--color-line);
      border-radius: var(--calendar-card-radius);
      background: var(--color-panel);
      .calendar-week,
      .calendar-days {
        display: grid;
        grid-template-columns: repeat(7, minmax(0, 1fr));
        gap: 5px;
      }
      .calendar-week {
        flex: none;
        margin-bottom: 5px;
        border-bottom: 1px solid var(--color-line);
        .calendar-weekday {
          padding: 6px 0 7px;
          color: var(--color-text);
          text-align: center;
          font-size: var(--font-size-sm);
          &.calendar-weekend {
            color: var(--color-danger);
          }
        }
      }
      .calendar-days {
        flex: 1;
        min-height: 0;
        grid-template-rows: repeat(6, minmax(0, 1fr));
        &.calendar-days-week {
          grid-template-rows: minmax(0, 1fr);
        }
        .calendar-day {
          position: relative;
          display: flex;
          align-items: center;
          flex-direction: column;
          min-width: 0;
          min-height: 0;
          gap: 3px;
          padding: clamp(5px, 1.3vh, 14px) 3px 3px;
          border: 1px solid transparent;
          border-radius: 9px;
          font: inherit;
          color: var(--calendar-day-color, var(--color-text));
          background: transparent;
          cursor: pointer;
          &:hover {
            background: var(--color-panel-soft);
          }
          &:focus-visible {
            outline: 2px solid var(--color-primary);
            outline-offset: -2px;
          }
          &.calendar-weekend {
            color: var(--calendar-day-color, var(--color-success));
          }
          &.calendar-outside {
            opacity: 0.53;
          }
          &.calendar-today {
            border-color: var(--color-line);
          }
          &.calendar-selected {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            color: var(--color-primary);
            opacity: 1;
          }
          &.calendar-day-leave {
            align-items: flex-start;
            padding-inline: 6px;
            background: var(--calendar-leave-tint);
            &:hover {
              background: color-mix(
                in srgb,
                var(--calendar-leave-color) 11%,
                var(--color-panel)
              );
            }
          }
          .calendar-number {
            font-size: clamp(17px, 2.1cqw, 23px);
            line-height: 1.16;
            font-variant-numeric: tabular-nums;
          }
          .calendar-lunar {
            font-size: 11px;
            line-height: 1.2;
            color: color-mix(
              in srgb,
              currentColor 52%,
              var(--color-text-muted)
            );
          }
          .calendar-week-entries {
            display: flex;
            flex-direction: column;
            align-self: stretch;
            min-height: 0;
            gap: 6px;
            margin-top: 8px;
            overflow: auto;
            .calendar-week-entry {
              padding: 7px 4px;
              background: var(--color-primary-soft);
              border-radius: 5px;
              color: var(--color-primary);
              font-size: 10px;
              line-height: 1.6;
              text-align: left;
              overflow-wrap: anywhere;
            }
          }
          .calendar-corner {
            position: absolute;
            top: 3px;
            right: 4px;
            display: flex;
            flex-direction: column;
            align-items: flex-end;
            gap: 2px;
            .calendar-leave-mark {
              padding: 0 4px;
              border-radius: 3px;
              color: var(--calendar-leave-text);
              background: var(--calendar-leave-chip);
              font-size: 9px;
              line-height: 13px;
              white-space: nowrap;
            }
            .calendar-schedule-label {
              padding: 0 3px;
              border-radius: 3px;
              color: var(--calendar-schedule-color);
              background: color-mix(
                in srgb,
                var(--calendar-schedule-color) 12%,
                var(--color-panel)
              );
              font-size: 9px;
              line-height: 13px;
              white-space: nowrap;
            }
          }
          .calendar-markers {
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 3px;
            .calendar-day-count {
              margin-left: 2px;
              font-size: 9px;
              color: var(--color-text-muted);
            }
          }
        }
      }
      .calendar-list {
        flex: 1;
        min-height: 0;
        overflow: auto;
        .calendar-list-day {
          padding: 12px 0;
          border-bottom: 1px solid var(--color-line);
          .calendar-list-usage,
          .calendar-list-schedule {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 8px 10px;
            color: var(--color-text-muted);
            font-size: 12px;
          }
          .calendar-list-date {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 6px 3px;
            border: 0;
            background: transparent;
            color: var(--color-primary);
            font: inherit;
            cursor: pointer;
            :deep(span) {
              color: var(--color-text-muted);
              font-size: 10px;
            }
          }
          .calendar-list-entry {
            display: flex;
            align-items: center;
            gap: 10px;
            width: 100%;
            padding: 10px;
            border: 0;
            border-radius: 6px;
            font: inherit;
            font-size: 12px;
            color: var(--color-text);
            background: transparent;
            cursor: pointer;
            :deep(span:first-of-type) {
              flex: 1;
              text-align: left;
            }
            &:hover {
              background: var(--color-panel-soft);
            }
            &.calendar-leave-entry {
              color: var(--calendar-leave-text);
            }
          }
        }
        .calendar-list-empty {
          padding: 30px 0;
          color: var(--color-text-muted);
          font-size: 12px;
        }
      }
      .calendar-legend {
        display: flex;
        flex: none;
        align-items: center;
        flex-wrap: wrap;
        gap: 5px 12px;
        margin-top: 10px;
        padding: 9px 10px;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background: var(--color-panel-soft);
        .calendar-legend-item {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
        }
        .calendar-caption {
          margin-left: auto;
        }
      }
    }
  }
  .calendar-agenda {
    display: flex;
    flex-direction: column;
    flex: 0 0 clamp(234px, 33cqw, 400px);
    min-width: 0;
    min-height: 464px;
    gap: 12px;
    .calendar-agenda-head {
      position: relative;
      display: flex;
      flex-direction: column;
      justify-content: center;
      flex: none;
      gap: 7px;
      min-height: clamp(98px, 15vh, 132px);
      padding: 14px 56px 14px 16px;
      overflow: hidden;
      isolation: isolate;
      border-radius: var(--calendar-card-radius);
      background:
        radial-gradient(ellipse at 100% 100%, #83b6ff 0%, transparent 64%),
        linear-gradient(121deg, #1252ad 0%, #256bc8 60%, #448cec 100%);
      color: #fff;
      .calendar-date-caption {
        font-size: var(--font-size-sm);
      }
      .calendar-agenda-date {
        font-size: clamp(22px, 3.1cqw, 31px);
        line-height: 1.25;
        white-space: nowrap;
        font-variant-numeric: tabular-nums;
      }
      .calendar-date-icon {
        position: absolute;
        z-index: -1;
        top: 50%;
        right: 18px;
        display: grid;
        place-items: center;
        width: 61px;
        height: 63px;
        transform: translateY(-50%) rotate(13deg);
        border: 1px solid #ffffff57;
        border-radius: 14px;
        background: linear-gradient(145deg, #eaf4ff4d, #eaf4ff0d);
        box-shadow: 4px 5px 0 #214e9926;
        color: #eaf4ffd9;
      }
    }
    .calendar-section-title {
      display: flex;
      align-items: center;
      gap: 7px;
      margin: 0;
      font-size: var(--font-size-sm);
      line-height: 1.35;
      :deep(svg) {
        color: var(--color-primary);
      }
    }
    .calendar-quick-actions,
    .calendar-usage {
      flex: none;
      padding: 12px;
      border: 1px solid var(--color-line);
      border-radius: var(--calendar-card-radius);
      background: var(--color-panel);
      box-shadow: var(--calendar-card-shadow);
    }
    .calendar-quick-actions {
      .calendar-agenda-actions {
        display: flex;
        flex-wrap: wrap;
        gap: 5px;
        margin-top: 10px;
        .calendar-button {
          flex: 1 0 auto;
          min-height: 31px;
          gap: 4px;
          padding: 6px 5px;
          background: var(--color-panel-soft);
          font-size: 11px;
          :deep(svg) {
            color: var(--color-primary);
          }
        }
      }
    }
    .calendar-usage {
      min-height: 91px;
      .calendar-usage-message {
        margin: 12px 0 0;
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
      }
      .calendar-usage-metrics {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 7px;
        margin: 11px 0 0;
        .calendar-usage-metric {
          min-width: 0;
          :deep(dt) {
            color: var(--color-text-muted);
            font-size: 11px;
          }
          :deep(dd) {
            margin: 4px 0 0;
            font-size: clamp(16px, 1.9cqw, 21px);
            line-height: 1.22;
            font-variant-numeric: tabular-nums;
            overflow-wrap: anywhere;
          }
        }
      }
    }
    .calendar-agenda-content {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-height: 105px;
      padding: 12px;
      border: 1px solid var(--color-line);
      border-radius: var(--calendar-card-radius);
      background: var(--color-panel-soft);
      overflow: auto;
      scrollbar-width: thin;
      .calendar-agenda-summary {
        display: flex;
        align-items: center;
        justify-content: space-between;
        flex: none;
        gap: 8px;
        margin-bottom: 10px;
      }
      .calendar-agenda-leaves {
        display: flex;
        flex: none;
        flex-wrap: wrap;
        gap: 6px;
        margin-bottom: 8px;
        .calendar-agenda-leave {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 5px 8px;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          background: var(--calendar-leave-tint);
          color: var(--calendar-leave-text);
          font: inherit;
          font-size: var(--font-size-xs);
          cursor: pointer;
          &:hover {
            border-color: var(--calendar-leave-color);
          }
        }
      }
      .calendar-agenda-list {
        display: flex;
        flex-direction: column;
        flex: none;
        gap: 7px;
        .calendar-entry {
          display: flex;
          flex-direction: column;
          flex: none;
          gap: 8px;
          padding: 13px;
          border: 1px solid var(--color-line);
          border-radius: 9px;
          color: var(--color-text);
          background: var(--color-panel);
          text-align: left;
          font: inherit;
          cursor: pointer;
          &:hover {
            border-color: var(--color-primary);
          }
          &.calendar-entry-done {
            .calendar-entry-title {
              text-decoration: line-through;
              opacity: 0.6;
            }
          }
          .calendar-entry-type {
            display: flex;
            align-items: center;
            gap: 5px;
            font-size: 10px;
            color: var(--color-text-muted);
            .calendar-entry-time {
              margin-left: auto;
            }
          }
          .calendar-entry-title {
            font-size: 14px;
            line-height: 1.6;
            overflow-wrap: anywhere;
          }
          .calendar-entry-tags {
            display: flex;
            flex-wrap: wrap;
            gap: 5px;
            :deep(span) {
              border-radius: 20px;
              padding: 4px 7px;
              color: var(--color-text-muted);
              background: var(--color-primary-soft);
              font-size: 9px;
            }
          }
          .calendar-entry-created {
            display: flex;
            align-items: center;
            gap: 6px;
            padding-top: 9px;
            border-top: 1px solid var(--color-line);
            font-size: 9px;
            color: var(--color-text-muted);
          }
          .calendar-entry-caption {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            font-size: 10px;
            color: var(--color-text-muted);
          }
        }
      }
      .calendar-agenda-empty {
        display: flex;
        align-items: center;
        justify-content: center;
        flex-direction: column;
        flex: 1;
        gap: 10px;
        min-height: 0;
        color: var(--color-text-muted);
        text-align: center;
        font-size: var(--font-size-sm);
        .calendar-empty-icon {
          display: grid;
          place-items: center;
          width: 46px;
          height: 46px;
          border-radius: 13px;
          color: var(--color-primary);
          background: var(--color-primary-soft);
        }
      }
    }
  }
}

@container (max-width: 690px) {
  .note-calendar {
    flex-direction: column;
    gap: 12px;
    .calendar-month {
      flex: none;
      min-height: 470px;
    }
    .calendar-agenda {
      flex: none;
      min-height: 0;
      .calendar-agenda-content {
        min-height: 140px;
        overflow: visible;
      }
    }
  }
}

@container (max-width: 450px) {
  .note-calendar {
    .calendar-month {
      .calendar-toolbar {
        .calendar-top-actions {
          margin-left: 0;
        }
      }
      .calendar-board {
        padding-inline: 6px;
      }
    }
  }
}
</style>

<template>
  <section class="note-calendar" aria-label="便签日历">
    <div class="calendar-month">
      <header class="calendar-toolbar">
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
        <button
          class="calendar-button"
          type="button"
          @click="selectDate(today)"
        >
          今天
        </button>
      </header>
      <div class="calendar-week" aria-hidden="true">
        <span
          v-for="(day, index) in weekdays"
          :key="day"
          class="calendar-weekday"
          :class="{ 'calendar-weekend': index > 4 }"
          >{{ day }}</span
        >
      </div>
      <div class="calendar-days" :aria-label="`${year} 年 ${month + 1} 月`">
        <button
          v-for="day in days"
          :key="day.key"
          class="calendar-day"
          :class="{
            'calendar-outside': day.outside,
            'calendar-weekend': day.weekend,
            'calendar-today': day.key === today,
            'calendar-selected': day.key === selectedDate
          }"
          type="button"
          :data-date="day.key"
          :aria-label="`${day.key}，农历${day.lunar}，${day.entries.length} 项事项`"
          :aria-current="day.key === today ? 'date' : undefined"
          :aria-pressed="day.key === selectedDate"
          @click="selectDate(day.key)"
        >
          <span class="calendar-day-head">
            <span class="calendar-number">{{ day.number }}</span>
            <span v-if="day.key === today" class="calendar-today-label"
              >今</span
            >
          </span>
          <span class="calendar-lunar">{{ day.lunar }}</span>
          <span v-if="day.entries.length" class="calendar-markers">
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
            <span class="calendar-day-count">{{ day.entries.length }}</span>
          </span>
        </button>
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
        <span class="calendar-caption">本月事项</span>
      </footer>
    </div>
    <aside class="calendar-agenda" aria-label="当日事项">
      <header class="calendar-agenda-head">
        <span class="calendar-caption"
          >{{ selectedDate === today ? "今天" : "所选日期" }} ·
          {{ selectedWeekday }}</span
        >
        <span class="calendar-agenda-date"
          >{{ Number(selectedDate.slice(5, 7)) }} 月
          {{ Number(selectedDate.slice(8)) }} 日</span
        >
        <span class="calendar-caption"
          >{{ selectedEntries.length }} 项事项</span
        >
      </header>
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
        </button>
      </div>
      <div v-else class="calendar-agenda-empty">
        <CalendarDays :size="30" />
        <span>这一天，留一点空白</span>
        <span class="calendar-caption">写下想法，或安排一件要做的事。</span>
      </div>
      <button
        class="calendar-button calendar-create"
        type="button"
        @click="$emit('create', selectedDate)"
      >
        <Plus :size="14" />在这天新建便签
      </button>
      <p class="calendar-agenda-foot">
        便签按事项日期归属，待办按截止日期归属；提醒显示下一次触发时间。
      </p>
    </aside>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import {
  Bell,
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  ListTodo,
  NotebookPen,
  Plus
} from "lucide-vue-next"
import { localInput } from "@/features/quick-notes/constant"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  notes: { type: Array, default: () => [] },
  reminders: { type: Array, default: () => [] },
  fromDate: { type: String, default: "" },
  toDate: { type: String, default: "" }
})
defineEmits(["open", "create"])
const today = ref(localInput(Date.now()).slice(0, 10))
const selectedDate = ref(today.value)
const year = ref(Number(today.value.slice(0, 4)))
const month = ref(Number(today.value.slice(5, 7)) - 1)
const weekdays = ["一", "二", "三", "四", "五", "六", "日"]
const lunarDays =
  "初一 初二 初三 初四 初五 初六 初七 初八 初九 初十 十一 十二 十三 十四 十五 十六 十七 十八 十九 二十 廿一 廿二 廿三 廿四 廿五 廿六 廿七 廿八 廿九 三十".split(
    " "
  )
const lunarFormatter = new Intl.DateTimeFormat("zh-CN-u-ca-chinese", {
  month: "long",
  day: "numeric"
})
const years = computed(() =>
  Array.from({ length: 21 }, (_, index) => year.value - 10 + index)
)

// 使用本地日期与浏览器农历数据，不将日期转为 UTC，也不推测节假日调休。
const entriesByDate = computed(() => {
  const result = new Map()
  const append = (date, entry) => {
    if ((props.fromDate && date < props.fromDate) || (props.toDate && date > props.toDate)) return
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
  const first = new Date(year.value, month.value, 1, 12)
  const offset = (first.getDay() + 6) % 7
  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(year.value, month.value, index - offset + 1, 12)
    const key = localInput(date.getTime()).slice(0, 10)
    const lunar = lunarFormatter.formatToParts(date)
    const lunarDay = Number(lunar.find((part) => part.type === "day").value)
    return {
      key,
      number: date.getDate(),
      outside: date.getMonth() !== month.value,
      weekend: index % 7 > 4,
      lunar:
        lunarDay === 1
          ? lunar.find((part) => part.type === "month").value
          : lunarDays[lunarDay - 1],
      entries: entriesByDate.value.get(key) || []
    }
  })
})
const monthCounts = computed(() => {
  const result = { note: 0, task: 0, reminder: 0 }
  for (const day of days.value) {
    if (!day.outside) for (const entry of day.entries) result[entry.kind]++
  }
  return result
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
let clock
onMounted(() => {
  clock = window.setInterval(() => {
    today.value = localInput(Date.now()).slice(0, 10)
  }, 60_000)
})
onBeforeUnmount(() => clearInterval(clock))
</script>

<style scoped lang="less">
.note-calendar {
  display: flex;
  align-items: stretch;
  gap: 20px;
  min-width: 0;
  .calendar-button,
  .calendar-icon,
  .calendar-select {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    padding: 7px 11px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
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
  }
  .calendar-month {
    flex: 1;
    min-width: 0;
    padding: 18px;
    border: 1px solid var(--color-line);
    border-radius: 13px;
    background: var(--color-panel);
    .calendar-toolbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      margin-bottom: 20px;
      .calendar-period {
        display: flex;
        align-items: center;
        gap: 5px;
      }
    }
    .calendar-week,
    .calendar-days {
      display: grid;
      grid-template-columns: repeat(7, minmax(0, 1fr));
      gap: 5px;
    }
    .calendar-week {
      margin-bottom: 8px;
      .calendar-weekday {
        padding: 7px 0;
        color: var(--color-text-muted);
        text-align: center;
        font-size: 11px;
        &.calendar-weekend {
          color: #c6817c;
        }
      }
    }
    .calendar-days {
      .calendar-day {
        display: flex;
        align-items: center;
        flex-direction: column;
        min-width: 0;
        height: 77px;
        gap: 3px;
        padding: 9px 3px 5px;
        border: 1px solid transparent;
        border-radius: 9px;
        font: inherit;
        color: var(--color-text);
        background: transparent;
        cursor: pointer;
        &:hover {
          background: var(--color-panel-soft);
        }
        &:focus-visible {
          outline: 2px solid var(--color-primary);
        }
        &.calendar-weekend {
          color: #c6817c;
        }
        &.calendar-outside {
          opacity: 0.32;
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
        .calendar-day-head {
          display: flex;
          align-items: center;
          justify-content: center;
          position: relative;
          .calendar-number {
            font-size: 19px;
            line-height: 24px;
            font-variant-numeric: tabular-nums;
          }
          .calendar-today-label {
            position: absolute;
            left: calc(100% + 4px);
            top: -3px;
            font-size: 9px;
            color: var(--color-primary);
          }
        }
        .calendar-lunar {
          font-size: 10px;
          opacity: 0.72;
        }
        .calendar-markers {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 3px;
          margin-top: auto;
          .calendar-day-count {
            margin-left: 2px;
            font-size: 9px;
            color: var(--color-text-muted);
          }
        }
      }
    }
    .calendar-legend {
      display: flex;
      align-items: center;
      gap: 17px;
      margin-top: 16px;
      padding-top: 14px;
      border-top: 1px solid var(--color-line);
      .calendar-legend-item {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        color: var(--color-text-muted);
        font-size: 11px;
      }
      .calendar-caption {
        margin-left: auto;
      }
    }
  }
  .calendar-agenda {
    display: flex;
    flex-direction: column;
    flex: 0 0 255px;
    min-width: 0;
    padding: 21px 17px 16px;
    border: 1px solid var(--color-line);
    border-radius: 13px;
    background: var(--color-panel-soft);
    .calendar-agenda-head {
      display: flex;
      flex-direction: column;
      gap: 9px;
      padding: 0 3px 18px;
      border-bottom: 1px solid var(--color-line);
      .calendar-agenda-date {
        font-size: 25px;
        letter-spacing: -0.7px;
      }
    }
    .calendar-agenda-list {
      display: flex;
      flex-direction: column;
      flex: 1;
      gap: 9px;
      padding: 16px 0;
      max-height: 384px;
      overflow: auto;
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
          font-size: 12px;
          line-height: 1.6;
          overflow-wrap: anywhere;
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
      gap: 12px;
      min-height: 210px;
      color: var(--color-text-muted);
      font-size: 12px;
    }
    .calendar-create {
      width: 100%;
      margin-top: auto;
      color: var(--color-primary);
    }
    .calendar-agenda-foot {
      margin: 12px 0 0;
      color: var(--color-text-muted);
      font-size: 10px;
      line-height: 1.8;
    }
  }
}
</style>

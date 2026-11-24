<template>
  <section class="calendar-almanac" aria-label="农历与黄历宜忌">
    <button
      class="almanac-toggle"
      type="button"
      :aria-expanded="expanded"
      :aria-label="expanded ? '收起完整宜忌' : '查看完整宜忌'"
      @click="expanded = !expanded"
    >
      <span class="almanac-date"
        >{{ lunarDate.getMonthInChinese() }}月{{
          lunarDate.getDayInChinese()
        }}</span
      >
      <span class="almanac-year"
        >{{ lunarDate.getYearInGanZhi() }}年 ·
        {{ lunarDate.getYearShengXiao() }}</span
      >
      <ChevronDown class="almanac-chevron" :size="15" />
    </button>
    <dl class="almanac-activities" :class="{ 'almanac-expanded': expanded }">
      <div class="almanac-row">
        <dt class="almanac-tag almanac-good">宜</dt>
        <dd class="almanac-text">{{ activities.good }}</dd>
      </div>
      <div class="almanac-row">
        <dt class="almanac-tag almanac-avoid">忌</dt>
        <dd class="almanac-text">{{ activities.avoid }}</dd>
      </div>
    </dl>
    <div v-if="schedule || nextHoliday" class="almanac-holiday">
      <span v-if="schedule" class="almanac-holiday-row">
        <CalendarDays :size="13" />
        {{ schedule.title
        }}<template v-if="schedule.source === 'public'">
          · {{ schedule.kind === "workday" ? "调休补班" : "放假" }}
        </template>
      </span>
      <span v-if="nextHoliday" class="almanac-holiday-row">
        <Clock3 :size="13" />
        距 {{ nextHoliday.year }} 年{{ nextHoliday.title }}还有
        {{ nextHoliday.days }} 天
      </span>
    </div>
  </section>
</template>

<script setup>
import { computed, ref } from "vue"
import { CalendarDays, ChevronDown, Clock3 } from "lucide-vue-next"

const props = defineProps({
  lunarDate: { type: Object, required: true },
  schedule: { type: Object, default: null },
  nextHoliday: { type: Object, default: null }
})
const expanded = ref(false)

// 黄历宜忌与公历日期一同切换，展开时可看完整条目。
const activities = computed(() => ({
  good: props.lunarDate.getDayYi().join("、"),
  avoid: props.lunarDate.getDayJi().join("、")
}))
</script>

<style scoped lang="less">
.calendar-almanac {
  flex: none;
  min-width: 0;
  padding: 9px 11px;
  border: 1px solid var(--color-line);
  border-radius: var(--calendar-card-radius);
  background: var(--color-panel-soft);
  .almanac-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    margin-bottom: 7px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text);
    text-align: left;
    font: inherit;
    cursor: pointer;
    .almanac-date {
      font-size: var(--font-size-sm);
    }
    .almanac-year {
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
    .almanac-chevron {
      flex: none;
      margin-left: auto;
      color: var(--color-primary);
    }
    &[aria-expanded="true"] {
      .almanac-chevron {
        transform: rotate(180deg);
      }
    }
  }
  .almanac-activities {
    display: grid;
    gap: 4px;
    margin: 0;
    .almanac-row {
      display: flex;
      align-items: baseline;
      min-width: 0;
      gap: 6px;
      .almanac-tag {
        padding: 0 4px;
        border-radius: 4px;
        font-size: var(--font-size-xs);
        &.almanac-good {
          background: var(--color-success-soft);
          color: var(--color-success);
        }
        &.almanac-avoid {
          background: var(--color-danger-soft);
          color: var(--color-danger);
        }
      }
      .almanac-text {
        margin: 0;
        color: var(--color-text);
        font-size: var(--font-size-xs);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }
    &.almanac-expanded {
      max-height: 130px;
      overflow: auto;
      .almanac-row {
        .almanac-text {
          white-space: normal;
        }
      }
    }
  }
  .almanac-holiday {
    display: grid;
    gap: 4px;
    margin-top: 7px;
    padding-top: 6px;
    border-top: 1px solid var(--color-line);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    .almanac-holiday-row {
      display: flex;
      align-items: center;
      gap: 5px;
      :deep(svg) {
        flex: none;
        color: var(--color-primary);
      }
    }
  }
}
</style>

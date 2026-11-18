<template>
  <section class="calendar-rules" aria-label="休假规则设置">
    <p>休息日绿色 · 节假日红色 · 补假紫色 · 补班橙色；指定日期优先。</p>
    <div v-for="rule in rules" :key="rule.id" class="rule-row">
      <span :style="{ color: rule.color }">{{ rule.title }}</span>
      <button type="button" :disabled="busy" @click="edit(rule)">编辑</button>
      <button type="button" :disabled="busy" @click="remove(rule)">删除</button>
    </div>
    <form @submit.prevent="save">
      <label>名称<input v-model="draft.title" required maxlength="80" /></label>
      <label
        >类型<select
          v-model="draft.kind"
          @change="
            draft.color = {
              rest: '#579b80',
              holiday: '#df6676',
              workday: '#cc893d',
              makeup: '#8b74bc'
            }[draft.kind]
          "
        >
          <option value="rest">休息</option>
          <option value="holiday">节假日</option>
          <option value="workday">补班</option>
          <option value="makeup">补假／调休</option>
        </select></label
      >
      <label
        >重复<select v-model="draft.repeat">
          <option value="once">指定日期</option>
          <option value="weekly">每周</option>
          <option value="alternating">大小周</option>
          <option value="monthly">每月固定日期</option>
          <option value="yearly">每年</option>
        </select></label
      >
      <label
        >开始日期<input
          v-model="draft.startDate"
          type="date"
          :required="draft.repeat === 'once'"
      /></label>
      <label>结束日期<input v-model="draft.endDate" type="date" /></label>
      <template v-if="['weekly', 'alternating'].includes(draft.repeat)">
        <label v-for="(day, index) in weekdays" :key="day"
          ><input v-model="draft.weekdays" type="checkbox" :value="index" />{{
            day
          }}</label
        >
        <label
          >日期单双号<select v-model="draft.dateParity">
            <option value="all">不限</option>
            <option value="odd">单号</option>
            <option value="even">双号</option>
          </select></label
        >
      </template>
      <template v-if="draft.repeat === 'alternating'">
        <label
          >起算周日期<input v-model="draft.anchorDate" type="date" required
        /></label>
        <label
          >生效周<select v-model.number="draft.cycleWeek">
            <option :value="0">起算周及隔周</option>
            <option :value="1">起算周的下一周及隔周</option>
          </select></label
        >
      </template>
      <label v-if="draft.repeat === 'monthly'"
        >每月日期（逗号分隔）<input
          v-model="monthDays"
          placeholder="1,15,28"
          required
      /></label>
      <template v-if="draft.repeat === 'yearly'"
        ><label
          >月份<input
            v-model.number="draft.month"
            type="number"
            min="1"
            max="12" /></label
        ><label
          >日期<input
            v-model.number="draft.day"
            type="number"
            min="1"
            max="31" /></label
      ></template>
      <label v-if="draft.kind === 'makeup'"
        >调休状态<select v-model="draft.workStatus">
          <option value="rest">补假休息</option>
          <option value="work">调休补班</option>
        </select></label
      >
      <label>颜色<input v-model="draft.color" type="color" /></label>
      <label
        >优先级<input
          v-model.number="draft.priority"
          type="number"
          min="0"
          max="100"
      /></label>
      <button :disabled="busy">保存规则</button
      ><button type="button" @click="reset">新规则</button>
    </form>
    <textarea
      v-model="description"
      placeholder="告诉模型排班规则，例如周六单号上班、双号休息；大小周请说明起算周。"
    />
    <input
      type="file"
      accept="image/png,image/jpeg,image/webp,image/gif"
      multiple
      @change="attach"
    />
    <span>{{ images.length }} 张图片</span
    ><button type="button" @click="images = []">清空图片</button>
    <button
      type="button"
      :disabled="busy || (!description.trim() && !images.length)"
      @click="analyze"
    >
      让模型设置
    </button>
    <p v-if="message" role="status">{{ message }}</p>
  </section>
</template>
<script setup>
import { ref } from "vue"
import { quickNotesApi } from "@/api/modules/quick-notes"
import { readAssistantImages } from "@/features/assistant/attachments"
defineProps({ rules: { type: Array, default: () => [] } })
const weekdays = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
const draft = ref({})
const monthDays = ref("")
const busy = ref(false)
const description = ref("")
const images = ref([])
const message = ref("")
function reset() {
  draft.value = {
    title: "",
    kind: "rest",
    calendar: "solar",
    repeat: "weekly",
    weekdays: [5, 6],
    monthDays: [],
    dateParity: "all",
    anchorDate: "",
    cycleWeek: 0,
    startDate: "",
    endDate: "",
    month: 1,
    day: 1,
    color: "#579b80",
    workStatus: "rest",
    priority: 10,
    enabled: true
  }
  monthDays.value = ""
}
reset()
function edit(rule) {
  draft.value = JSON.parse(JSON.stringify(rule))
  monthDays.value = rule.monthDays.join(",")
}
async function run(action, payload) {
  busy.value = true
  message.value = ""
  try {
    const result = await quickNotesApi.invoke(action, payload)
    message.value = result.question || "设置已保存"
    return result
  } catch (error) {
    message.value = error.message || String(error)
  } finally {
    busy.value = false
  }
}
async function save() {
  const result = await run("calendar-rule-save", {
    ...draft.value,
    monthDays: monthDays.value.split(/[,，]/).filter(Boolean).map(Number)
  })
  if (result) reset()
}
async function remove(rule) {
  await run("calendar-rule-delete", { id: rule.id, revision: rule.revision })
}
async function attach(event) {
  try {
    images.value = await readAssistantImages(event.target.files, images.value)
  } catch (error) {
    message.value = error.message
  }
  event.target.value = ""
}
async function analyze() {
  await run("calendar-analyze", {
    text: description.value,
    images: images.value
  })
}
</script>
<style scoped lang="less">
.calendar-rules {
  max-height: 360px;
  overflow: auto;
  padding: 12px;
  border: 1px solid var(--color-line);
  color: var(--color-text);
  background: var(--color-panel-soft);
  form,
  .rule-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 12px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  input,
  select,
  textarea,
  button {
    font: inherit;
    color: inherit;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    border-radius: 5px;
    padding: 5px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    min-height: 65px;
  }
}
</style>

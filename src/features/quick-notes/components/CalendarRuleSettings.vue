<template>
  <section class="calendar-rules" aria-label="休假规则设置">
    <aside class="rules-sidebar" aria-label="已有规则">
      <header class="rules-list-heading">
        <span>排班规则</span>
        <button class="rules-add" type="button" :disabled="busy" @click="reset">
          <Plus :size="14" />新建
        </button>
      </header>
      <div class="rules-list">
        <button
          v-for="rule in scheduleRules"
          :key="rule.id"
          class="rules-item"
          :class="{
            'rules-item-active': mode === 'manual' && draft.id === rule.id
          }"
          :aria-pressed="mode === 'manual' && draft.id === rule.id"
          :disabled="busy"
          type="button"
          @click="edit(rule)"
        >
          <span
            class="rules-swatch"
            :style="{
              background:
                rule.kind === 'work' ? 'var(--color-text)' : rule.color
            }"
          ></span>
          <span class="rules-item-content"
            ><span class="rules-item-title">{{ rule.title }}</span
            ><span class="rules-item-summary">{{
              ruleSummary(rule)
            }}</span></span
          >
          <span v-if="!rule.enabled" class="rules-disabled">停用</span
          ><ChevronRight v-else :size="14" class="rules-item-arrow" />
        </button>
        <div v-if="!scheduleRules.length" class="rules-empty">
          <CalendarDays :size="20" /><span>还没有排班规则</span>
        </div>
      </div>
      <footer class="rules-sidebar-foot">
        <p class="rules-caption">请假记录可在日历中右键编辑。</p>
      </footer>
    </aside>

    <div class="rules-workspace">
      <div class="rules-tabs" role="tablist" aria-label="设置方式">
        <button
          id="rules-manual-tab"
          class="rules-tab"
          :class="{ 'rules-tab-active': mode === 'manual' }"
          type="button"
          role="tab"
          :aria-selected="mode === 'manual'"
          aria-controls="rules-manual-panel"
          :tabindex="mode === 'manual' ? 0 : -1"
          :disabled="busy"
          @click="mode = 'manual'"
          @keydown.right.prevent="switchTab('ai')"
          @keydown.left.prevent="switchTab('ai')"
        >
          <SlidersHorizontal :size="15" />手动设置
        </button>
        <button
          id="rules-ai-tab"
          class="rules-tab"
          :class="{ 'rules-tab-active': mode === 'ai' }"
          type="button"
          role="tab"
          :aria-selected="mode === 'ai'"
          aria-controls="rules-ai-panel"
          :tabindex="mode === 'ai' ? 0 : -1"
          :disabled="busy"
          @click="mode = 'ai'"
          @keydown.right.prevent="switchTab('manual')"
          @keydown.left.prevent="switchTab('manual')"
        >
          <Sparkles :size="15" />AI 设置
        </button>
      </div>

      <form
        v-show="mode === 'manual'"
        id="rules-manual-panel"
        class="rules-pane"
        role="tabpanel"
        aria-labelledby="rules-manual-tab"
        @submit.prevent="save"
      >
        <div class="rules-scroll">
          <header class="rules-editor-heading">
            <div>
              <h3 class="rules-title">
                {{ draft.id ? "编辑规则" : "新建休假规则" }}
              </h3>
            </div>
            <span
              class="rules-preview"
              :style="{
                color: draft.kind === 'work' ? 'var(--color-text)' : draft.color
              }"
              ><i
                class="rules-swatch"
                :style="{
                  background:
                    draft.kind === 'work' ? 'var(--color-text)' : draft.color
                }"
              ></i
              >{{
                kinds.find((kind) => kind.value === draft.kind)?.label ||
                "日历标签"
              }}</span
            >
          </header>
          <fieldset class="rules-fields" :disabled="busy">
            <section class="rules-section" aria-label="基本信息">
              <h4 class="rules-section-title">基本信息</h4>
              <div class="rules-field-row">
                <label class="rules-field rules-field-wide"
                  ><span>规则名称<span class="rules-required"> *</span></span
                  ><input
                    v-model="draft.title"
                    class="rules-input"
                    required
                    maxlength="80"
                    placeholder="例如：周末双休、春节假期"
                /></label>
                <label class="rules-field"
                  ><span>日期类型</span
                  ><select
                    v-model="draft.kind"
                    class="rules-input"
                    @change="
                      draft.color = kinds.find(
                        (kind) => kind.value === draft.kind
                      ).color
                    "
                  >
                    <option
                      v-for="kind in kinds"
                      :key="kind.value"
                      :value="kind.value"
                    >
                      {{ kind.label }}
                    </option>
                  </select></label
                >
              </div>
              <label v-if="draft.kind === 'makeup'" class="rules-field"
                ><span>调休安排</span
                ><select v-model="draft.workStatus" class="rules-input">
                  <option value="rest">补假休息</option>
                  <option value="work">调休补班</option>
                </select></label
              >
            </section>

            <section class="rules-section" aria-label="时间安排">
              <h4 class="rules-section-title">时间安排</h4>
              <label class="rules-field"
                ><span>重复方式</span
                ><select v-model="draft.repeat" class="rules-input">
                  <option
                    v-for="repeat in repeats"
                    :key="repeat.value"
                    :value="repeat.value"
                  >
                    {{ repeat.label }}
                  </option>
                </select></label
              >
              <fieldset
                v-if="['weekly', 'alternating'].includes(draft.repeat)"
                class="rules-weekdays"
              >
                <legend class="rules-field-label">选择星期</legend>
                <div class="rules-weekday-options">
                  <label
                    v-for="(day, index) in weekdays"
                    :key="day"
                    class="rules-weekday"
                    :class="{
                      'rules-weekday-selected': draft.weekdays.includes(index)
                    }"
                    ><input
                      v-model="draft.weekdays"
                      class="rules-weekday-input"
                      type="checkbox"
                      :value="index"
                    /><span>{{ day }}</span></label
                  >
                </div>
              </fieldset>
              <div
                v-if="draft.repeat === 'alternating'"
                class="rules-field-row"
              >
                <label class="rules-field"
                  ><span>起算周日期<span class="rules-required"> *</span></span
                  ><input
                    v-model="draft.anchorDate"
                    class="rules-input"
                    type="date"
                    required
                /></label>
                <label class="rules-field"
                  ><span>生效周</span
                  ><select v-model.number="draft.cycleWeek" class="rules-input">
                    <option :value="0">起算周及隔周</option>
                    <option :value="1">下一周及隔周</option>
                  </select></label
                >
              </div>
              <label
                v-if="['weekly', 'alternating'].includes(draft.repeat)"
                class="rules-field"
                ><span>日期单双号</span
                ><select v-model="draft.dateParity" class="rules-input">
                  <option value="all">不限日期</option>
                  <option value="odd">仅单号日期</option>
                  <option value="even">仅双号日期</option></select
                ><span class="rules-caption"
                  >按每月的日期区分单双号，例如 1、3、5 日为单号。</span
                ></label
              >
              <label v-if="draft.repeat === 'monthly'" class="rules-field"
                ><span>每月生效日期<span class="rules-required"> *</span></span
                ><input
                  v-model="monthDays"
                  class="rules-input"
                  placeholder="例如：1, 15, 28"
                  required
                /><span class="rules-caption"
                  >填写 1–31，用逗号分隔；当月不存在的日期将跳过。</span
                ></label
              >
              <div v-if="draft.repeat === 'yearly'" class="rules-field-row">
                <label class="rules-field"
                  ><span>月份</span
                  ><input
                    v-model.number="draft.month"
                    class="rules-input"
                    type="number"
                    min="1"
                    max="12"
                    required
                /></label>
                <label class="rules-field"
                  ><span>日期</span
                  ><input
                    v-model.number="draft.day"
                    class="rules-input"
                    type="number"
                    min="1"
                    max="31"
                    required
                /></label>
              </div>
              <div class="rules-date-range">
                <div class="rules-field-row">
                  <label class="rules-field"
                    ><span
                      >{{
                        draft.repeat === "once"
                          ? "开始日期"
                          : "生效起日（可选）"
                      }}<span
                        v-if="draft.repeat === 'once'"
                        class="rules-required"
                      >
                        *</span
                      ></span
                    ><input
                      v-model="draft.startDate"
                      class="rules-input"
                      type="date"
                      :required="draft.repeat === 'once' || !!draft.endDate"
                  /></label>
                  <span class="rules-date-separator">至</span>
                  <label class="rules-field"
                    ><span>{{
                      draft.repeat === "once"
                        ? "结束日期（可选）"
                        : "生效止日（可选）"
                    }}</span
                    ><input
                      v-model="draft.endDate"
                      class="rules-input"
                      type="date"
                      :min="draft.startDate || undefined"
                  /></label>
                </div>
                <p class="rules-caption">
                  {{
                    draft.repeat === "once"
                      ? "单日安排只需填写开始日期。"
                      : "不填日期时长期生效；设置止日时需同时填写起日。"
                  }}
                </p>
              </div>
            </section>

            <details class="rules-advanced">
              <summary class="rules-advanced-toggle">
                <Palette :size="15" /><span>显示与优先级</span
                ><ChevronDown :size="14" class="rules-advanced-arrow" />
              </summary>
              <div class="rules-advanced-fields">
                <div class="rules-field-row">
                  <label class="rules-field"
                    ><span>日历颜色</span
                    ><span class="rules-color-control"
                      ><input
                        v-model="draft.color"
                        class="rules-color-input"
                        type="color"
                      /><span>{{ draft.color.toUpperCase() }}</span></span
                    ></label
                  >
                  <label class="rules-field"
                    ><span>优先级</span
                    ><input
                      v-model.number="draft.priority"
                      class="rules-input"
                      type="number"
                      min="0"
                      max="100"
                      required
                  /></label>
                </div>
                <p class="rules-caption">
                  自定义节假日、补班和调休优先于公共放假和补班，公共安排优先于大小周和双休。同一档内先比较是否指定日期，再比较优先级数值。
                </p>
                <label class="rules-enabled"
                  ><input
                    v-model="draft.enabled"
                    type="checkbox"
                  />启用此规则</label
                >
              </div>
            </details>
          </fieldset>
        </div>
        <div
          v-if="message"
          class="rules-message"
          :class="`rules-message-${messageType}`"
          :role="messageType === 'error' ? 'alert' : 'status'"
        >
          <component
            :is="messageType === 'error' ? CircleAlert : CheckCircle2"
            :size="16"
          /><span>{{ message }}</span>
        </div>
        <footer class="rules-actions">
          <button
            v-if="draft.id"
            class="rules-button rules-delete"
            type="button"
            :disabled="busy"
            @click="remove"
          >
            <Trash2 :size="14" />删除规则</button
          ><span v-else class="rules-caption">保存后立即应用到日历</span>
          <button
            class="rules-button rules-primary"
            :disabled="busy"
            type="submit"
          >
            <LoaderCircle v-if="busy" :size="15" class="rules-spinner" /><Check
              v-else
              :size="15"
            />{{ busy ? "保存中…" : "保存规则" }}
          </button>
        </footer>
      </form>

      <section
        v-show="mode === 'ai'"
        id="rules-ai-panel"
        class="rules-pane"
        role="tabpanel"
        aria-labelledby="rules-ai-tab"
        @paste="pasteImages"
      >
        <div class="rules-scroll">
          <header class="rules-editor-heading">
            <div>
              <h3 class="rules-title">把排班交给 AI</h3>
              <p class="rules-caption">
                描述排班规律、请假日期，或上传排班表。
              </p>
            </div>
            <Sparkles :size="21" class="rules-ai-symbol" />
          </header>
          <label class="rules-field"
            ><span>你的排班或请假安排</span
            ><textarea
              v-model="description"
              class="rules-input rules-description"
              :disabled="busy"
              placeholder="例如：周日休息，周六单号上班、双号休息。&#10;也可以输入：下周一到周三请假三天。"
            />
          </label>
          <div class="rules-examples" aria-label="排班示例">
            <span class="rules-caption">试试这样说</span
            ><button
              v-for="example in examples"
              :key="example.label"
              class="rules-example"
              type="button"
              :disabled="busy"
              @click="description = example.text"
            >
              {{ example.label }}<ArrowUpRight :size="12" />
            </button>
          </div>
          <input
            ref="fileInput"
            class="rules-file-input"
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            multiple
            :disabled="busy"
            tabindex="-1"
            aria-label="上传排班图片"
            @change="attach"
          />
          <button
            class="rules-upload"
            :class="{ 'rules-upload-over': dragging }"
            type="button"
            :disabled="busy"
            @click="fileInput.click()"
            @dragover.prevent="dragging = true"
            @dragleave.prevent="dragging = false"
            @drop.prevent="dropImages"
          >
            <span class="rules-upload-icon"><ImagePlus :size="22" /></span
            ><span class="rules-upload-copy"
              ><span>添加放假通知或排班表</span
              ><span class="rules-caption"
                >点击上传、拖入或粘贴图片，最多 4 张</span
              ></span
            ><Plus :size="18" />
          </button>
          <div v-if="images.length" class="rules-attachments">
            <div class="rules-attachment-heading">
              <span class="rules-caption"
                >已添加 {{ images.length }} 张图片</span
              ><button
                class="rules-text-button"
                type="button"
                :disabled="busy"
                @click="images = []"
              >
                清空
              </button>
            </div>
            <AssistantImages
              :images="images"
              :editable="!busy"
              @remove="images = images.filter((image) => image.id !== $event)"
            />
          </div>
          <div class="rules-ai-note">
            <Info :size="15" /><span
              >大小周请说明起算周。信息不明确时，AI 会先询问你。</span
            >
          </div>
        </div>
        <div
          v-if="message"
          class="rules-message"
          :class="`rules-message-${messageType}`"
          :role="messageType === 'error' ? 'alert' : 'status'"
        >
          <component
            :is="
              messageType === 'error'
                ? CircleAlert
                : messageType === 'question'
                  ? Info
                  : CheckCircle2
            "
            :size="16"
          /><span>{{ message }}</span>
        </div>
        <footer class="rules-actions">
          <span class="rules-caption">识别后自动保存到休假规则</span
          ><button
            class="rules-button rules-primary"
            type="button"
            :disabled="busy || (!description.trim() && !images.length)"
            @click="analyze"
          >
            <LoaderCircle
              v-if="busy"
              :size="15"
              class="rules-spinner"
            /><Sparkles v-else :size="15" />{{
              busy ? "正在识别…" : "识别并应用"
            }}
          </button>
        </footer>
      </section>
    </div>
  </section>
</template>

<script setup>
import { computed, nextTick, ref } from "vue"
import {
  ArrowUpRight,
  CalendarDays,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  ImagePlus,
  Info,
  LoaderCircle,
  Palette,
  Plus,
  SlidersHorizontal,
  Sparkles,
  Trash2
} from "lucide-vue-next"
import { quickNotesApi } from "@/api/modules/quick-notes"
import { readAssistantImages } from "@/features/assistant/attachments"
import AssistantImages from "@/features/assistant/components/AssistantImages.vue"

const props = defineProps({ rules: { type: Array, default: () => [] } })
// 请假按实际日期编辑，规则列表管理周期排班和节假日。
const scheduleRules = computed(() =>
  props.rules.filter((rule) => rule.kind !== "leave")
)
const weekdays = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
const kinds = [
  { value: "work", label: "正常上班日", color: "#14213a" },
  { value: "rest", label: "休息日", color: "#579b80" },
  { value: "holiday", label: "节假日", color: "#df6676" },
  { value: "workday", label: "补班", color: "#cc893d" },
  {
    value: "makeup",
    label: "补假 / 调休",
    color: "#8b74bc"
  }
]
const repeats = [
  { value: "once", label: "指定日期" },
  { value: "weekly", label: "每周重复" },
  { value: "alternating", label: "大小周 · 隔周重复" },
  { value: "monthly", label: "每月固定日期" },
  { value: "yearly", label: "每年重复" }
]
const examples = [
  { label: "周末双休", text: "每周六、周日休息，其他日期正常上班。" },
  { label: "单双号周六", text: "周日休息，周六单号上班、双号休息。" },
  {
    label: "固定月休",
    text: "每月固定 5 日、15 日、25 日休息，其他日期正常上班。"
  }
]
const mode = ref("manual")
const draft = ref({})
const monthDays = ref("")
const busy = ref(false)
const description = ref("")
const images = ref([])
const fileInput = ref(null)
const dragging = ref(false)
const message = ref("")
const messageType = ref("success")

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
  mode.value = "manual"
  message.value = ""
}

function edit(rule) {
  draft.value = JSON.parse(JSON.stringify(rule))
  monthDays.value = rule.monthDays.join(", ")
  mode.value = "manual"
  message.value = ""
}

reset()
if (scheduleRules.value.length) edit(scheduleRules.value[0])

function ruleSummary(rule) {
  if (rule.repeat === "once")
    return rule.endDate && rule.endDate !== rule.startDate
      ? `${rule.startDate} 至 ${rule.endDate}`
      : rule.startDate
  if (rule.repeat === "monthly") return `每月 ${rule.monthDays.join("、")} 日`
  if (rule.repeat === "yearly") return `每年 ${rule.month} 月 ${rule.day} 日`
  const days = rule.weekdays.map((day) => weekdays[day]).join("、")
  const parity =
    rule.dateParity === "odd"
      ? " · 单号"
      : rule.dateParity === "even"
        ? " · 双号"
        : ""
  return `${rule.repeat === "alternating" ? "隔周" : "每周"} · ${days}${parity}`
}

async function switchTab(value) {
  if (busy.value) return
  mode.value = value
  await nextTick()
  document.getElementById(`rules-${value}-tab`)?.focus()
}

async function run(action, payload) {
  if (busy.value) return
  busy.value = true
  message.value = ""
  try {
    const result = await quickNotesApi.invoke(action, payload)
    messageType.value = result.question ? "question" : "success"
    message.value = result.question || "规则已保存，日历已更新。"
    return result
  } catch (error) {
    messageType.value = "error"
    message.value = error.message || String(error)
  } finally {
    busy.value = false
  }
}

async function save() {
  if (
    ["weekly", "alternating"].includes(draft.value.repeat) &&
    !draft.value.weekdays.length
  ) {
    messageType.value = "error"
    message.value = "请至少选择一个星期。"
    return
  }
  const result = await run("calendar-rule-save", {
    ...draft.value,
    monthDays: monthDays.value
      .split(/[,，]/)
      .map((day) => day.trim())
      .filter(Boolean)
      .map(Number)
  })
  // 保留后端返回的 revision，继续编辑同一条规则时使用最新版本。
  if (result) draft.value = result
}

async function remove() {
  const result = await run("calendar-rule-delete", {
    id: draft.value.id,
    revision: draft.value.revision
  })
  if (result) {
    reset()
    messageType.value = "success"
    message.value = "规则已删除，日历已更新。"
  }
}

async function addImages(files) {
  if (busy.value) return
  try {
    images.value = await readAssistantImages(files, images.value)
  } catch (error) {
    messageType.value = "error"
    message.value = error.message
  }
}

async function attach(event) {
  await addImages(event.target.files)
  event.target.value = ""
}

async function dropImages(event) {
  dragging.value = false
  await addImages(event.dataTransfer.files)
}

async function pasteImages(event) {
  if (!event.clipboardData?.files.length) return
  await addImages(event.clipboardData.files)
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
  display: flex;
  flex: 1;
  min-height: 0;
  color: var(--color-text);
  font-size: var(--font-size-sm);
  .rules-caption {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    line-height: 1.7;
  }
  .rules-swatch {
    display: inline-block;
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 3px;
  }
  .rules-sidebar {
    display: flex;
    flex: 0 0 clamp(148px, 27%, 212px);
    flex-direction: column;
    min-height: 0;
    padding: 12px;
    border-right: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    .rules-list-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      padding: 0 2px;
      margin-bottom: 10px;
      .rules-add {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        min-height: 29px;
        padding: 0 7px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-text);
        font: inherit;
        font-size: var(--font-size-xs);
        white-space: nowrap;
        cursor: pointer;
        &:hover {
          border-color: var(--color-primary);
          color: var(--color-primary);
        }
      }
    }
    .rules-list {
      flex: 1;
      min-height: 0;
      overflow: auto;
      .rules-item {
        display: flex;
        align-items: center;
        gap: 10px;
        width: 100%;
        margin-bottom: 5px;
        padding: 8px;
        border: 1px solid transparent;
        border-radius: 8px;
        background: transparent;
        color: var(--color-text);
        text-align: left;
        font: inherit;
        cursor: pointer;
        &:hover {
          background: var(--color-panel);
        }
        &.rules-item-active {
          color: var(--color-primary);
          background: var(--color-primary-soft);
        }
        .rules-item-content {
          display: flex;
          flex: 1;
          flex-direction: column;
          min-width: 0;
          gap: 3px;
          .rules-item-title {
            overflow-wrap: anywhere;
          }
          .rules-item-summary {
            color: var(--color-text-muted);
            font-size: 11px;
            line-height: 1.6;
            overflow-wrap: anywhere;
          }
        }
        .rules-item-arrow {
          flex: none;
          color: var(--color-text-soft);
        }
        .rules-disabled {
          color: var(--color-text-muted);
          font-size: 10px;
        }
      }
      .rules-empty {
        display: flex;
        align-items: center;
        flex-direction: column;
        gap: 10px;
        padding: 36px 0;
        color: var(--color-text-muted);
        text-align: center;
      }
    }
    .rules-sidebar-foot {
      flex: none;
      margin-top: 10px;
      padding: 9px 2px 0;
      border-top: 1px solid var(--color-line);
    }
  }
  .rules-workspace {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--color-panel);
    .rules-tabs {
      display: flex;
      flex: none;
      align-self: flex-start;
      gap: 2px;
      margin: 12px 16px 0;
      padding: 3px;
      border: 1px solid var(--color-line);
      border-radius: 9px;
      background: var(--color-panel-soft);
      .rules-tab {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        min-height: 30px;
        padding: 0 9px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--color-text-muted);
        font: inherit;
        cursor: pointer;
        &.rules-tab-active {
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
      }
    }
    .rules-pane {
      display: flex;
      flex: 1;
      flex-direction: column;
      min-height: 0;
      margin: 0;
      .rules-scroll {
        flex: 1;
        min-height: 0;
        overflow: auto;
        padding: 14px 16px;
        scrollbar-gutter: stable;
        .rules-editor-heading {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 12px;
          margin-bottom: 12px;
          .rules-title {
            margin: 0 0 5px;
            font: inherit;
            font-size: var(--font-size-lg);
          }
          .rules-preview {
            display: inline-flex;
            align-items: center;
            flex: none;
            gap: 6px;
            padding: 4px 8px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            font-size: 11px;
          }
          .rules-ai-symbol {
            flex: none;
            color: var(--color-primary);
          }
        }
        .rules-fields {
          min-width: 0;
          margin: 0;
          padding: 0;
          border: 0;
          .rules-section {
            display: flex;
            flex-direction: column;
            gap: 11px;
            padding-bottom: 13px;
            margin-bottom: 13px;
            border-bottom: 1px solid var(--color-line);
            .rules-section-title {
              margin: 0;
              color: var(--color-text);
              font: inherit;
              font-size: var(--font-size-xs);
            }
            .rules-weekdays {
              min-width: 0;
              margin: 0;
              padding: 0;
              border: 0;
              .rules-field-label {
                margin-bottom: 8px;
                padding: 0;
                color: var(--color-text-muted);
                font-size: var(--font-size-xs);
              }
              .rules-weekday-options {
                display: flex;
                gap: 5px;
                .rules-weekday {
                  position: relative;
                  display: flex;
                  align-items: center;
                  justify-content: center;
                  flex: 1;
                  min-width: 0;
                  height: 32px;
                  border: 1px solid var(--color-line);
                  border-radius: 8px;
                  color: var(--color-text-muted);
                  cursor: pointer;
                  font-size: var(--font-size-xs);
                  &.rules-weekday-selected {
                    border-color: var(--color-primary);
                    color: var(--color-primary);
                    background: var(--color-primary-soft);
                  }
                  &:focus-within {
                    outline: 2px solid var(--color-primary);
                    outline-offset: 2px;
                  }
                  .rules-weekday-input {
                    position: absolute;
                    width: 1px;
                    height: 1px;
                    opacity: 0;
                  }
                }
              }
            }
            .rules-date-range {
              .rules-date-separator {
                align-self: flex-end;
                padding-bottom: 9px;
                color: var(--color-text-soft);
                font-size: 11px;
              }
              .rules-caption {
                margin-top: 7px;
              }
            }
          }
          .rules-advanced {
            .rules-advanced-toggle {
              display: flex;
              align-items: center;
              gap: 8px;
              padding: 0 0 1px;
              color: var(--color-text-muted);
              list-style: none;
              cursor: pointer;
              font-size: var(--font-size-xs);
              &::-webkit-details-marker {
                display: none;
              }
              .rules-advanced-arrow {
                margin-left: auto;
              }
            }
            &[open] .rules-advanced-arrow {
              transform: rotate(180deg);
            }
            .rules-advanced-fields {
              padding-top: 16px;
              .rules-caption {
                margin-top: 8px;
              }
              .rules-color-control {
                display: flex;
                align-items: center;
                gap: 9px;
                height: 34px;
                padding: 0 10px;
                border: 1px solid var(--color-line);
                border-radius: 8px;
                color: var(--color-text-muted);
                font-size: 11px;
                .rules-color-input {
                  width: 23px;
                  height: 23px;
                  padding: 0;
                  border: 0;
                  background: transparent;
                  cursor: pointer;
                }
              }
              .rules-enabled {
                display: inline-flex;
                align-items: center;
                gap: 8px;
                margin-top: 12px;
                color: var(--color-text-muted);
                font-size: var(--font-size-xs);
              }
            }
          }
        }
        .rules-field-row {
          display: flex;
          gap: 12px;
          .rules-field {
            flex: 1;
          }
          .rules-field-wide {
            flex: 1.5;
          }
        }
        .rules-field {
          display: flex;
          flex-direction: column;
          min-width: 0;
          gap: 5px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
          .rules-required {
            color: var(--color-danger);
          }
        }
        .rules-input {
          box-sizing: border-box;
          width: 100%;
          min-width: 0;
          height: 34px;
          padding: 5px 9px;
          border: 1px solid var(--color-line-strong);
          border-radius: 8px;
          background: var(--color-panel);
          color: var(--color-text);
          outline: none;
          font: inherit;
          font-size: var(--font-size-sm);
          transition: border-color 0.15s;
          &:focus {
            border-color: var(--color-primary);
            box-shadow: 0 0 0 2px var(--color-primary-soft);
          }
          &::placeholder {
            color: var(--color-text-soft);
          }
          &.rules-description {
            min-height: 105px;
            height: 105px;
            resize: vertical;
            line-height: 1.9;
            padding: 12px;
          }
        }
        .rules-examples {
          display: flex;
          align-items: center;
          flex-wrap: wrap;
          gap: 8px;
          margin: 10px 0 14px;
          .rules-example {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            padding: 4px 7px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            font: inherit;
            font-size: 11px;
            cursor: pointer;
            &:hover {
              border-color: var(--color-primary);
              color: var(--color-primary);
            }
          }
        }
        .rules-file-input {
          display: none;
        }
        .rules-upload {
          display: flex;
          align-items: center;
          gap: 13px;
          width: 100%;
          padding: 12px;
          border: 1px dashed var(--color-line-strong);
          border-radius: 9px;
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          text-align: left;
          font: inherit;
          cursor: pointer;
          &:hover,
          &.rules-upload-over {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
          }
          .rules-upload-icon {
            display: flex;
            align-items: center;
            justify-content: center;
            flex: none;
            color: var(--color-primary);
          }
          .rules-upload-copy {
            display: flex;
            flex: 1;
            flex-direction: column;
            gap: 4px;
          }
        }
        .rules-attachments {
          margin-top: 14px;
          .rules-attachment-heading {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 8px;
            .rules-text-button {
              border: 0;
              background: transparent;
              color: var(--color-text-muted);
              font: inherit;
              font-size: var(--font-size-xs);
              cursor: pointer;
            }
          }
        }
        .rules-ai-note {
          display: flex;
          align-items: flex-start;
          gap: 8px;
          margin-top: 20px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
          line-height: 1.7;
        }
      }
      .rules-message {
        display: flex;
        align-items: flex-start;
        flex: none;
        gap: 8px;
        max-height: 100px;
        overflow: auto;
        margin: 0 16px 8px;
        padding: 10px 12px;
        border-radius: 8px;
        color: var(--color-success);
        background: var(--color-success-soft);
        font-size: var(--font-size-xs);
        line-height: 1.7;
        overflow-wrap: anywhere;
        &.rules-message-error {
          color: var(--color-danger);
          background: var(--color-danger-soft);
        }
        &.rules-message-question {
          color: var(--color-primary);
          background: var(--color-primary-soft);
        }
      }
      .rules-actions {
        display: flex;
        align-items: center;
        justify-content: space-between;
        flex: none;
        gap: 12px;
        min-height: 54px;
        padding: 9px 16px;
        border-top: 1px solid var(--color-line);
        .rules-button {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 8px;
          min-height: 34px;
          padding: 0 12px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel);
          color: var(--color-text);
          font: inherit;
          cursor: pointer;
          &.rules-primary {
            border-color: var(--color-primary-solid);
            background: var(--color-primary-solid);
            color: #fff;
          }
          &.rules-delete {
            padding-left: 0;
            border-color: transparent;
            background: transparent;
            color: var(--color-text-muted);
          }
          &.rules-delete:hover {
            color: var(--color-danger);
          }
          .rules-spinner {
            animation: rules-spin 1s linear infinite;
          }
        }
      }
    }
  }
  :deep(button:disabled),
  :deep(input:disabled),
  :deep(select:disabled),
  :deep(textarea:disabled) {
    opacity: 0.5;
    cursor: default;
  }
  :deep(button:focus-visible),
  :deep(summary:focus-visible) {
    outline: 2px solid var(--color-primary);
    outline-offset: 3px;
  }
  :deep(input[type="checkbox"]) {
    accent-color: var(--color-primary);
  }
}
@keyframes rules-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

<template>
  <component
    :is="inline ? 'section' : BaseModal"
    class="quick-note-editor"
    :class="{ 'note-editor-inline': inline }"
    @close="$emit('close')"
    @keydown.esc.stop="$emit('close')"
  >
    <form class="note-form" @submit.prevent="save">
      <header class="note-editor-header">
        <div class="editor-heading">
          <span class="editor-eyebrow"
            >QUICK NOTES / {{ note.id ? "便签详情" : "新的记录" }}</span
          >
          <h2 class="editor-title">
            <NotebookPen :size="24" />{{ note.id ? "便签详情" : "新建便签" }}
          </h2>
        </div>
        <div class="editor-actions">
          <button class="note-button" type="button" @click="$emit('close')">
            <ArrowLeft :size="15" />{{ inline ? "返回列表" : "取消" }}
          </button>
          <button
            v-if="note.id"
            class="note-button"
            type="button"
            @click="share"
          >
            <Share2 :size="15" />复制分享
          </button>
          <button
            v-if="note.id && note.lifecycle === 'active'"
            class="note-button"
            type="button"
            @click="archive"
          >
            <Archive :size="15" />归档
          </button>
          <button
            v-if="inline"
            class="note-button note-primary"
            :disabled="saving"
          >
            <Check :size="15" />{{ saving ? "保存中…" : "保存修改" }}
          </button>
          <button
            v-else
            class="editor-close"
            type="button"
            aria-label="关闭便签编辑器"
            @click="$emit('close')"
          >
            <X :size="19" />
          </button>
        </div>
      </header>
      <div class="note-editor-columns">
        <section class="note-editor-main">
          <label class="note-field editor-title-field"
            >标题<input
              v-model="form.title"
              placeholder="输入便签标题"
              maxlength="160"
          /></label>
          <QuickNoteBodyEditor v-model="form.content" />
          <div class="editor-quick-actions">
            <button class="note-button" type="button" @click="smartOpen = true">
              <Sparkles :size="15" />智能整理
            </button>
            <button class="note-button" type="button" @click="addTask">
              <ListTodo :size="15" />添加待办
            </button>
            <button class="note-button" type="button" @click="addReminder">
              <Bell :size="15" />添加提醒
            </button>
            <button class="note-button" type="button" @click="tagInput.focus()">
              <Tag :size="15" />插入标签
            </button>
          </div>
          <div class="note-fields">
            <label class="note-field"
              >类型<select v-model="form.type">
                <option
                  v-for="type in noteTypes"
                  :key="type.id"
                  :value="type.id"
                >
                  {{ type.label }}
                </option>
              </select></label
            >
            <label class="note-field"
              >项目分组<select v-model="form.groupId">
                <option
                  v-for="folder in groups"
                  :key="folder.id"
                  :value="folder.id"
                >
                  {{ folder.name }}
                </option>
              </select></label
            >
            <label class="note-field"
              >分类<select v-model="form.categoryId">
                <option
                  v-for="folder in categories"
                  :key="folder.id"
                  :value="folder.id"
                >
                  {{ folder.name }}
                </option>
              </select></label
            >
          </div>
          <div class="note-fields">
            <label class="note-field"
              >事项 / 工作日期<input v-model="form.occurredOn" type="date"
            /></label>
            <label class="note-field"
              >标签<input
                ref="tagInput"
                v-model="tags"
                placeholder="使用逗号分隔"
            /></label>
            <label class="note-field"
              >便签颜色<select v-model="form.color">
                <option
                  v-for="(label, color) in colors"
                  :key="color"
                  :value="color"
                >
                  {{ label }}
                </option>
              </select></label
            >
          </div>
          <div v-if="form.type === 'worklog'" class="note-worklog">
            <label
              v-for="(label, field) in workFields"
              :key="field"
              class="note-field"
              >{{ label
              }}<textarea
                v-model="form.worklog[field]"
                rows="2"
                :placeholder="`${label}尚未记录时可以留空`"
              ></textarea>
            </label>
          </div>
          <section ref="reminderSection" class="note-section">
            <div class="note-section-head">
              <span><Bell :size="15" />定时提醒</span
              ><button
                type="button"
                class="note-text-button"
                @click="addReminder"
              >
                <Plus :size="14" />添加提醒
              </button>
            </div>
            <p v-if="form.pendingTime" class="note-warning">
              {{ form.pendingTime }}
              <button
                type="button"
                class="note-text-button"
                @click="form.pendingTime = ''"
              >
                已确认
              </button>
            </p>
            <div
              v-for="(plan, index) in form.reminders"
              :key="plan.id || index"
              class="note-reminder"
            >
              <div class="note-fields">
                <label class="note-field"
                  >提醒时间<input
                    v-model="plan.localAt"
                    type="datetime-local"
                    :min="
                      plan.enabled ? localInput(Date.now() + 60000) : undefined
                    "
                    :required="plan.enabled"
                /></label>
                <label class="note-field"
                  >关联事项<select v-model="plan.taskId">
                    <option value="">整条便签</option>
                    <option
                      v-for="task in form.tasks"
                      :key="task.id"
                      :value="task.id"
                    >
                      {{ task.title }}
                    </option>
                  </select></label
                >
                <label class="note-field"
                  >重复<select v-model="plan.schedule.repeat">
                    <option
                      v-for="option in repeatOptions"
                      :key="option.id"
                      :value="option.id"
                    >
                      {{ option.label }}
                    </option>
                  </select></label
                >
              </div>
              <div class="note-fields">
                <label class="note-field"
                  >结束日期<input v-model="plan.localEnd" type="date"
                /></label>
                <label class="note-field"
                  >最多次数<input
                    v-model.number="plan.schedule.maxCount"
                    type="number"
                    min="1"
                    placeholder="不限"
                /></label>
                <label class="note-check"
                  ><input
                    v-model="plan.enabled"
                    type="checkbox"
                  />启用提醒</label
                >
                <label class="note-check"
                  ><input
                    v-model="plan.schedule.sound"
                    type="checkbox"
                  />声音提醒</label
                >
                <button
                  type="button"
                  class="note-text-button"
                  @click="form.reminders.splice(index, 1)"
                >
                  <Trash2 :size="14" />移除
                </button>
              </div>
              <div
                v-if="plan.schedule.repeat === 'weekly'"
                class="note-weekdays"
              >
                <label
                  v-for="(day, weekday) in [
                    '一',
                    '二',
                    '三',
                    '四',
                    '五',
                    '六',
                    '日'
                  ]"
                  :key="weekday"
                  class="note-check"
                  ><input
                    v-model="plan.schedule.weekdays"
                    type="checkbox"
                    :value="weekday"
                  />周{{ day }}</label
                >
              </div>
              <label
                v-if="plan.schedule.repeat === 'monthly'"
                class="note-field"
                >每月日期<input
                  v-model.number="plan.schedule.day"
                  type="number"
                  min="1"
                  max="31"
                  placeholder="默认使用提醒时间的日期"
              /></label>
              <label class="note-field"
                >相对截止时间提前提醒<select
                  v-model.number="plan.schedule.offsetMinutes"
                  :disabled="
                    !form.tasks.find((task) => task.id === plan.taskId)
                      ?.localDueAt
                  "
                  @change="updateRelative(plan)"
                >
                  <option :value="0">独立指定时间</option>
                  <option
                    v-for="minutes in [5, 15, 30, 60, 1440]"
                    :key="minutes"
                    :value="minutes"
                  >
                    截止前 {{ minutes === 1440 ? "1 天" : `${minutes} 分钟` }}
                  </option>
                </select></label
              >
              <div class="note-shortcuts">
                <span>快捷设置</span
                ><button
                  v-for="minutes in [10, 30, 60]"
                  :key="minutes"
                  type="button"
                  class="note-text-button"
                  @click="
                    plan.localAt = localInput(Date.now() + minutes * 60000)
                  "
                >
                  {{ minutes }} 分钟后</button
                ><button
                  type="button"
                  class="note-text-button"
                  :disabled="
                    !form.tasks.find((task) => task.id === plan.taskId)?.dueDate
                  "
                  @click="beforeDeadline(plan)"
                >
                  截止前一天 09:00
                </button>
              </div>
            </div>
            <p class="note-caption">
              提醒时间必须晚于当前时间。按本机时区提醒。保留应用在托盘运行可按时通知；完全退出后，下次启动补发。归档或删除会暂停提醒。
            </p>
          </section>
          <div class="editor-created">
            <Info :size="14" />{{
              note.createdAt
                ? `创建于 ${formatDateTime(note.createdAt)}`
                : "新便签保存在本机"
            }}<span v-if="note.updatedAt"
              >更新于 {{ formatDateTime(note.updatedAt) }}</span
            >
          </div>
        </section>
        <aside class="note-editor-aside">
          <section class="editor-ai-panel">
            <div class="editor-panel-title">
              <Sparkles :size="17" />AI 建议<span class="editor-badge"
                >基于内容分析</span
              >
            </div>
            <p class="editor-muted">
              整理正文、提取待办和标签，分析结果由你确认后应用。
            </p>
            <button
              class="note-button"
              type="button"
              :disabled="!form.content.trim()"
              @click="smartOpen = true"
            >
              <Sparkles :size="14" />分析这条便签
            </button>
            <p v-if="analysisFeedback" class="editor-success">
              {{ analysisFeedback }}
            </p>
          </section>
          <section class="note-section">
            <div class="note-section-head">
              <span>待办事项 · {{ form.tasks.length }}</span
              ><button type="button" class="note-text-button" @click="addTask">
                <Plus :size="14" />添加待办
              </button>
            </div>
            <div
              v-for="(task, index) in form.tasks"
              :key="task.id"
              class="note-task"
            >
              <input
                v-model="task.title"
                aria-label="待办内容"
                placeholder="需要完成什么？"
              />
              <select v-model="task.status" aria-label="任务状态">
                <option
                  v-for="state in taskStates"
                  :key="state.id"
                  :value="state.id"
                >
                  {{ state.label }}
                </option>
              </select>
              <select v-model="task.priority" aria-label="优先级">
                <option value="normal">普通</option>
                <option value="high">优先</option>
                <option value="low">较低</option>
              </select>
              <input v-model="task.dueDate" type="date" aria-label="截止日期" />
              <input
                v-model="task.localDueAt"
                type="datetime-local"
                aria-label="精确截止时间（可选）"
              />
              <button
                type="button"
                class="note-text-button"
                aria-label="移除待办"
                @click="removeTask(index)"
              >
                <X :size="15" />
              </button>
            </div>
          </section>
          <section class="editor-history">
            <div class="editor-panel-title">
              <Clock3 :size="17" />操作记录<span class="editor-badge">{{
                note.history?.length || 0
              }}</span>
            </div>
            <div v-if="!note.history?.length" class="editor-muted">
              保存后将在这里记录修改历史。
            </div>
            <article
              v-for="entry in note.history || []"
              :key="entry.id"
              class="editor-history-item"
            >
              <span class="history-dot"></span>
              <div>
                <span
                  >{{ formatDateTime(entry.createdAt) }} ·
                  {{ entry.actor === "ai" ? "AI 整理" : "我" }}</span
                >
                <p>
                  {{
                    entry.operation === "create"
                      ? "创建了便签"
                      : "更新了便签内容"
                  }}
                </p>
              </div>
            </article>
          </section>
          <details v-if="note.captures?.length" class="note-section">
            <summary>原文与来源 · {{ note.captures.length }}</summary>
            <div
              v-for="capture in note.captures"
              :key="capture.id"
              class="note-source"
            >
              <span
                >{{ formatDateTime(capture.capturedAt) }} ·
                {{ capture.source?.type || "速记" }} ·
                {{ capture.model?.model || "手动" }}</span
              >
              <pre>{{ capture.sourceText }}</pre>
            </div>
          </details>
          <details v-if="note.history?.length" class="note-section">
            <summary>修改历史 · {{ note.history.length }}</summary>
            <div
              v-for="entry in note.history"
              :key="entry.id"
              class="note-source"
            >
              <span
                >{{ formatDateTime(entry.createdAt) }} ·
                {{ entry.actor === "ai" ? "AI 整理" : "编辑记录" }}</span
              >
              <pre>{{ entry.after?.content }}</pre>
              <div v-for="task in entry.after?.tasks || []" :key="task.id">
                {{
                  taskStates.find((state) => state.id === task.status)?.label
                }}
                · {{ task.title
                }}{{
                  task.completedAt
                    ? ` · 完成于 ${formatDateTime(task.completedAt)}`
                    : ""
                }}
              </div>
            </div>
          </details>
        </aside>
      </div>
      <p v-if="error" class="note-error" role="alert">{{ error }}</p>
      <footer v-if="!inline" class="note-footer">
        <span class="note-caption">{{ feedback || "内容尚未保存" }}</span
        ><button type="button" class="note-button" @click="$emit('close')">
          取消</button
        ><button
          v-if="!note.id"
          type="button"
          class="note-button"
          @click="saveDraft"
        >
          存为草稿</button
        ><button class="note-button note-primary" :disabled="saving">
          {{ saving ? "保存中…" : "保存便签" }}
        </button>
      </footer>
    </form>
    <QuickNoteSmartCapture
      :open="smartOpen"
      :initial-text="form.content"
      apply-only
      :folders="folders"
      @close="smartOpen = false"
      @apply="applyAnalysis"
    />
  </component>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import {
  Bell,
  Plus,
  Trash2,
  X,
  NotebookPen,
  ArrowLeft,
  Share2,
  Archive,
  Check,
  Sparkles,
  ListTodo,
  Tag,
  Info,
  Clock3
} from "lucide-vue-next"
import QuickNoteBodyEditor from "@/features/quick-notes/components/QuickNoteBodyEditor.vue"
import QuickNoteSmartCapture from "@/features/quick-notes/components/QuickNoteSmartCapture.vue"
import BaseModal from "@/components/BaseModal.vue"
import { quickNotesApi } from "@/api/modules/quick-notes"
import { formatDateTime } from "@/utils/formatters"
import {
  emptyNote,
  localInput,
  noteTypes,
  repeatOptions,
  taskStates
} from "@/features/quick-notes/constant"
const props = defineProps({
  inline: Boolean,
  note: { type: Object, default: () => ({}) },
  folders: { type: Array, default: () => [] }
})
const emit = defineEmits(["close", "saved"])
// 编辑草稿与详情隔离，取消不会改动卡片，提醒保留原始规则与下一次时间。
const form = ref(JSON.parse(JSON.stringify({ ...emptyNote(), ...props.note })))
// 新建草稿预先固定 ID，保存回包丢失后重试也不会新增重复记录。
if (!form.value.id) {
  form.value.id = crypto.randomUUID()
  form.value.revision = 0
}
form.value.reminders = (form.value.reminders || []).map((plan) => ({
  ...plan,
  enabled: Boolean(plan.enabled),
  schedule: { weekdays: [], sound: true, ...plan.schedule },
  localAt: localInput(plan.nextFireAt),
  localEnd: plan.schedule.endAt
    ? localInput(plan.schedule.endAt).slice(0, 10)
    : ""
}))
form.value.tasks = form.value.tasks.map((task) => ({
  ...task,
  localDueAt: localInput(task.dueAt)
}))
const tags = ref((form.value.tags || []).join("，"))
const saving = ref(false)
const error = ref("")
const reminderSection = ref(null)
const tagInput = ref(null)
const smartOpen = ref(false)
const analysisFeedback = ref("")
const feedback = ref("")
// 草稿仅恢复未保存的新便签，正式便签继续通过 revision 校验保存。
if (!props.note.id && !props.note.content) {
  const draft = localStorage.getItem("quick-notes:new-draft")
  if (draft) {
    const savedDraft = JSON.parse(draft)
    Object.assign(form.value, savedDraft)
    tags.value = (form.value.tags || []).join("，")
    if (props.note.occurredOn) form.value.occurredOn = props.note.occurredOn
  }
}
function saveDraft() {
  localStorage.setItem(
    "quick-notes:new-draft",
    JSON.stringify({
      ...form.value,
      tags: tags.value
        .split(/[,，]/)
        .map((value) => value.trim())
        .filter(Boolean)
    })
  )
  feedback.value = "草稿已保存在本机"
}
function applyAnalysis(note) {
  for (const field of [
    "title",
    "content",
    "type",
    "worklog",
    "occurredOn",
    "pendingTime"
  ]) {
    if (note[field] !== undefined) form.value[field] = note[field]
  }
  // 应用建议时保留已有任务与提醒，重名任务复用原 ID。
  const taskIds = new Map()
  for (const task of note.tasks || []) {
    const existing = form.value.tasks.find(
      (item) => item.title.trim() === task.title.trim()
    )
    taskIds.set(task.id, existing?.id || task.id)
    if (!existing)
      form.value.tasks.push({ ...task, localDueAt: localInput(task.dueAt) })
  }
  tags.value = [
    ...new Set(
      [...tags.value.split(/[,，]/), ...(note.tags || [])]
        .map((tag) => tag.trim())
        .filter(Boolean)
    )
  ].join("，")
  for (const [field, fallback] of [
    ["groupId", "inbox"],
    ["categoryId", "uncategorized"]
  ]) {
    if (form.value[field] === fallback && note[field])
      form.value[field] = note[field]
  }
  form.value.reminders.push(
    ...(note.reminders || []).map((plan) => ({
      ...plan,
      taskId: taskIds.get(plan.taskId) || plan.taskId,
      localAt: localInput(plan.nextFireAt),
      localEnd: ""
    }))
  )
  analysisFeedback.value = "已应用分析结果，保存修改后生效。"
}
async function share() {
  try {
    await navigator.clipboard.writeText(
      `# ${form.value.title}\n\n${form.value.content}`
    )
    feedback.value = "已复制便签内容"
  } catch (failure) {
    error.value = failure.message || String(failure)
  }
}
async function archive() {
  try {
    const result = await quickNotesApi.invoke("archive", {
      id: props.note.id,
      revision: props.note.revision
    })
    emit("saved", result)
  } catch (failure) {
    error.value = failure.message || String(failure)
  }
}
const groups = computed(() =>
  props.folders.filter(
    (folder) =>
      folder.kind === "group" &&
      (!folder.archived || folder.id === form.value.groupId)
  )
)
const categories = computed(() =>
  props.folders.filter(
    (folder) =>
      folder.kind === "category" &&
      (!folder.archived || folder.id === form.value.categoryId)
  )
)
const colors = {
  sand: "麦黄",
  mint: "薄荷",
  sky: "天青",
  rose: "浅粉",
  lavender: "雾紫"
}
const workFields = {
  process: "处理过程",
  result: "实际结果",
  blockers: "问题与阻塞",
  next: "下一步计划"
}
function addTask() {
  form.value.tasks.push({
    id: crypto.randomUUID(),
    title: "",
    status: "pending",
    priority: "normal",
    dueDate: ""
  })
}
// 删除待办同时移除其提醒，避免保存一个失去关联的通知计划。
function removeTask(index) {
  const task = form.value.tasks[index]
  form.value.reminders = form.value.reminders.filter(
    (plan) => plan.taskId !== task.id
  )
  form.value.tasks.splice(index, 1)
}
function addReminder() {
  form.value.reminders.push({
    taskId: "",
    enabled: true,
    localAt: localInput(Date.now() + 600000),
    localEnd: "",
    schedule: {
      weekdays: [],
      sound: true,
      repeat: "once",
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone
    }
  })
}
// 卡片提醒入口直接定位编辑器中的提醒；没有启用的计划时预填十分钟后的新提醒。
async function openReminders() {
  if (!form.value.reminders.some((plan) => plan.enabled)) addReminder()
  await nextTick()
  const index = form.value.reminders.findIndex((plan) => plan.enabled)
  const input = reminderSection.value.querySelectorAll(
    'input[type="datetime-local"]'
  )[index]
  input.scrollIntoView({ block: "center" })
  input.focus({ preventScroll: true })
}
defineExpose({ openReminders })
// 相对提醒同步展示实际时间，后端也按任务截止时间重新校验。
function updateRelative(plan) {
  const task = form.value.tasks.find((task) => task.id === plan.taskId)
  if (plan.schedule.offsetMinutes && task?.localDueAt)
    plan.localAt = localInput(
      new Date(task.localDueAt).getTime() - plan.schedule.offsetMinutes * 60000
    )
}
watch(
  () =>
    form.value.tasks.map((task) => `${task.id}:${task.localDueAt}`).join("|"),
  () => {
    for (const plan of form.value.reminders) updateRelative(plan)
  }
)
function beforeDeadline(plan) {
  const task = form.value.tasks.find((item) => item.id === plan.taskId)
  if (task?.dueDate) {
    const at = new Date(`${task.dueDate}T09:00`)
    at.setDate(at.getDate() - 1)
    plan.localAt = localInput(at.getTime())
  }
}
async function save() {
  if (saving.value) return
  error.value = ""
  // 提交时重新核对当前时间，避免编辑期间时间已过或提前提醒落在过去。
  if (
    form.value.reminders.some(
      (plan) => plan.enabled && !(new Date(plan.localAt).getTime() > Date.now())
    )
  ) {
    error.value = "提醒时间必须晚于当前时间，请重新选择"
    return
  }
  saving.value = true
  try {
    const payload = {
      ...form.value,
      tags: [
        ...new Set(
          tags.value
            .split(/[,，]/)
            .map((value) => value.trim())
            .filter(Boolean)
        )
      ],
      occurredOn: form.value.occurredOn || null,
      tasks: form.value.tasks.map(({ localDueAt, ...task }) => ({
        ...task,
        dueAt: localDueAt ? new Date(localDueAt).getTime() : null
      })),
      reminders: form.value.reminders.map((plan) => ({
        id: plan.id,
        taskId: plan.taskId,
        enabled: plan.enabled,
        nextFireAt: new Date(plan.localAt).getTime(),
        schedule: {
          ...plan.schedule,
          at: new Date(plan.localAt).getTime(),
          endAt: plan.localEnd
            ? new Date(`${plan.localEnd}T23:59:59`).getTime()
            : null,
          maxCount: plan.schedule.maxCount || null
        }
      }))
    }
    const saved = await quickNotesApi.invoke("save", payload)
    if (!props.note.id) localStorage.removeItem("quick-notes:new-draft")
    emit("saved", saved)
  } catch (failure) {
    error.value = failure.message || String(failure)
  } finally {
    saving.value = false
  }
}
</script>

<style scoped lang="less">
.quick-note-editor {
  padding: 0;
  display: flex;
  justify-content: flex-end;
  :deep(> .base-modal__panel) {
    width: min(800px, calc(100vw - 32px));
    height: 100%;
    max-height: 100%;
    border-radius: 22px 0 0 22px;
  }
  :deep(> .base-modal__panel > .base-modal__content) {
    padding: 0;
  }
  &.note-editor-inline {
    position: relative;
    flex: 1;
    min-height: 0;
    width: 100%;
    height: 100%;
    background: transparent;
  }
  .note-button,
  .note-text-button,
  .editor-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    border: 1px solid var(--color-line);
    border-radius: 9px;
    background: var(--color-panel);
    color: var(--color-text);
    padding: 8px 13px;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
    &:hover {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }
    &:disabled {
      opacity: 0.5;
      cursor: default;
    }
  }
  .note-primary {
    background: linear-gradient(135deg, #3c7dbf, #245c98);
    border-color: #3471ae;
    color: white;
  }
  .note-text-button {
    border: 0;
    background: transparent;
    color: var(--color-primary);
    padding: 4px;
    font-size: 11px;
  }
  .note-caption,
  .editor-muted {
    color: var(--color-text-muted);
    font-size: 11px;
    line-height: 1.8;
  }
  .note-error {
    color: var(--color-danger);
    padding: 0 24px;
    font-size: 12px;
  }
  .note-form {
    display: flex;
    flex-direction: column;
    width: 100%;
    min-height: 0;
    height: 100%;
    color: var(--color-text);
    font-size: 12px;
    .note-editor-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      flex: none;
      gap: 15px;
      padding: 22px 26px;
      .editor-heading {
        .editor-eyebrow {
          color: var(--color-text-muted);
          font-size: 10px;
          letter-spacing: 1.2px;
        }
        .editor-title {
          display: flex;
          align-items: center;
          gap: 9px;
          margin: 8px 0 0;
          font-size: 24px;
        }
      }
      .editor-actions {
        display: flex;
        align-items: center;
        gap: 9px;
      }
    }
    .note-editor-columns {
      display: flex;
      flex: 1;
      gap: 16px;
      min-height: 0;
      padding: 0 25px 22px;
      overflow: auto;
      .note-editor-main {
        flex: 1.8;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: 17px;
        padding: 21px;
        border: 1px solid var(--color-line);
        border-radius: 15px;
        background: var(--color-panel);
        .editor-title-field {
          font-size: 12px;
          :deep(input) {
            font-size: 15px;
            padding: 11px 13px;
          }
        }
        :deep(.note-body-editor) {
          min-height: 270px;
          :deep(.body-input) {
            min-height: 200px;
          }
        }
        .editor-quick-actions {
          display: flex;
          flex-wrap: wrap;
          gap: 7px;
          .note-button {
            flex: 1;
            padding: 8px;
            font-size: 11px;
          }
        }
        .editor-created {
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          gap: 8px;
          padding-top: 12px;
          border-top: 1px solid var(--color-line);
          color: var(--color-text-muted);
          font-size: 10px;
        }
      }
      .note-editor-aside {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-width: 240px;
        gap: 13px;
        .editor-ai-panel,
        .editor-history,
        .note-section {
          padding: 17px;
          border: 1px solid var(--color-line);
          border-radius: 14px;
          background: var(--color-panel);
        }
        .editor-panel-title {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 14px;
          margin-bottom: 12px;
          .editor-badge {
            margin-left: auto;
            padding: 3px 7px;
            border-radius: 12px;
            color: var(--color-primary);
            background: var(--color-primary-soft);
            font-size: 10px;
          }
        }
        .editor-success {
          font-size: 11px;
          color: var(--color-success);
        }
        .editor-history {
          flex: 1;
          .editor-history-item {
            display: flex;
            gap: 12px;
            padding: 10px 0;
            color: var(--color-text-muted);
            font-size: 11px;
            line-height: 1.8;
            .history-dot {
              flex: none;
              width: 8px;
              height: 8px;
              margin-top: 5px;
              border-radius: 50%;
              background: var(--color-primary);
            }
            :deep(p) {
              margin: 3px 0 0;
              color: var(--color-text);
            }
          }
        }
      }
    }
    .note-field {
      display: flex;
      flex: 1;
      min-width: 0;
      flex-direction: column;
      gap: 7px;
      color: var(--color-text-muted);
      font-size: 11px;
    }
    .note-fields {
      display: flex;
      gap: 10px;
      align-items: center;
    }
    .note-fields,
    .note-worklog,
    .note-section {
      :deep(input:not([type="checkbox"])),
      :deep(textarea),
      :deep(select) {
        width: 100%;
        min-width: 0;
        padding: 8px 9px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        color: var(--color-text);
        background: var(--color-panel);
        font: inherit;
        font-size: 11px;
      }
    }
    .note-field {
      :deep(input),
      :deep(select) {
        padding: 9px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-text);
        font: inherit;
      }
    }
    .note-worklog {
      display: flex;
      gap: 10px;
      flex-wrap: wrap;
      .note-field {
        flex-basis: 40%;
      }
    }
    .note-section {
      display: flex;
      flex-direction: column;
      gap: 12px;
      padding-top: 14px;
      border-top: 1px solid var(--color-line);
      .note-section-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        font-size: 13px;
        :deep(span) {
          display: inline-flex;
          align-items: center;
          gap: 7px;
        }
      }
      .note-task {
        display: flex;
        flex-wrap: wrap;
        gap: 7px;
        padding: 9px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        :deep(input:first-child) {
          flex-basis: 100%;
        }
        :deep(select),
        :deep(input[type="date"]) {
          width: calc(50% - 4px);
        }
      }
      .note-reminder {
        display: flex;
        flex-direction: column;
        gap: 10px;
        padding: 12px;
        border-radius: 9px;
        background: var(--color-panel-soft);
        .note-fields {
          flex-wrap: wrap;
          .note-field {
            flex-basis: 110px;
          }
        }
      }
      .note-check {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        font-size: 10px;
      }
      .note-weekdays,
      .note-shortcuts {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 8px;
        font-size: 10px;
      }
      .note-warning {
        color: var(--color-warning);
        font-size: 11px;
      }
      .note-source {
        color: var(--color-text-muted);
        font-size: 10px;
        :deep(pre) {
          white-space: pre-wrap;
          line-height: 1.7;
          font: inherit;
        }
      }
    }
    .note-footer {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      flex: none;
      gap: 12px;
      padding: 18px 26px;
      border-top: 1px solid var(--color-line);
      .note-caption {
        margin-right: auto;
      }
    }
  }
  &:not(.note-editor-inline) {
    .note-form {
      .note-editor-columns {
        flex-direction: column;
        .note-editor-aside {
          min-width: 0;
        }
      }
    }
  }
}
</style>

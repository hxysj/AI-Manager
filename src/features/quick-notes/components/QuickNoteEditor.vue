<template>
  <BaseModal
    class="quick-note-editor"
    :title="note.id ? '编辑便签' : '新建便签'"
    @close="$emit('close')"
  >
    <form class="note-form" @submit.prevent="save">
      <div class="note-fields">
        <label class="note-field"
          >类型<select v-model="form.type">
            <option v-for="type in noteTypes" :key="type.id" :value="type.id">
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
      <label class="note-field"
        >标题<input
          v-model="form.title"
          placeholder="留空时使用正文首行"
          maxlength="160"
      /></label>
      <label class="note-field"
        >内容<textarea
          v-model="form.content"
          rows="7"
          placeholder="记下事情、想法、进展或下一步…"
          maxlength="100000"
        ></textarea>
      </label>
      <div class="note-fields">
        <label class="note-field"
          >事项 / 工作日期<input v-model="form.occurredOn" type="date"
        /></label>
        <label class="note-field"
          >标签<input v-model="tags" placeholder="使用逗号分隔"
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
      <section ref="reminderSection" class="note-section">
        <div class="note-section-head">
          <span><Bell :size="15" />定时提醒</span
          ><button type="button" class="note-text-button" @click="addReminder">
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
                :min="plan.enabled ? localInput(Date.now() + 60000) : undefined"
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
              ><input v-model="plan.enabled" type="checkbox" />启用提醒</label
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
          <div v-if="plan.schedule.repeat === 'weekly'" class="note-weekdays">
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
          <label v-if="plan.schedule.repeat === 'monthly'" class="note-field"
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
                !form.tasks.find((task) => task.id === plan.taskId)?.localDueAt
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
              @click="plan.localAt = localInput(Date.now() + minutes * 60000)"
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
          按本机时区提醒。保留应用在托盘运行可按时通知；完全退出后，下次启动补发。归档或删除会暂停提醒。
        </p>
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
        <div v-for="entry in note.history" :key="entry.id" class="note-source">
          <span
            >{{ formatDateTime(entry.createdAt) }} ·
            {{ entry.actor === "ai" ? "AI 整理" : "编辑记录" }}</span
          >
          <pre>{{ entry.after?.content }}</pre>
          <div v-for="task in entry.after?.tasks || []" :key="task.id">
            {{ taskStates.find((state) => state.id === task.status)?.label }} ·
            {{ task.title
            }}{{
              task.completedAt
                ? ` · 完成于 ${formatDateTime(task.completedAt)}`
                : ""
            }}
          </div>
        </div>
      </details>
      <p v-if="error" class="note-error" role="alert">{{ error }}</p>
      <footer class="note-footer">
        <span class="note-caption">{{
          note.createdAt
            ? `首次记录 ${formatDateTime(note.createdAt)}`
            : "内容保存在本机"
        }}</span
        ><button type="button" class="note-button" @click="$emit('close')">
          取消</button
        ><button class="note-button note-primary" :disabled="saving">
          {{ saving ? "保存中…" : "保存便签" }}
        </button>
      </footer>
    </form>
  </BaseModal>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import { Bell, Plus, Trash2, X } from "lucide-vue-next"
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
    emit("saved", await quickNotesApi.invoke("save", payload))
  } catch (failure) {
    error.value = failure.message || String(failure)
  } finally {
    saving.value = false
  }
}
</script>

<style scoped lang="less">
.quick-note-editor {
  :deep(.base-modal__panel) {
    width: 850px;
    max-height: calc(100vh - 48px);
  }
  .note-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow: auto;
    max-height: calc(100vh - 160px);
    padding: 4px 4px 0;
    .note-fields,
    .note-task,
    .note-footer,
    .note-shortcuts {
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .note-field {
      display: flex;
      flex: 1;
      min-width: 0;
      flex-direction: column;
      gap: 7px;
      font-size: 12px;
      color: var(--color-text-muted);
    }
    :deep(input:not([type="checkbox"])),
    :deep(textarea),
    :deep(select) {
      width: 100%;
      min-width: 0;
      padding: 9px 10px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      color: var(--color-text);
      background: var(--color-panel);
      font: inherit;
      box-sizing: border-box;
    }
    :deep(textarea) {
      resize: vertical;
      line-height: 1.7;
    }
    .note-check {
      display: flex;
      align-items: center;
      gap: 6px;
      white-space: nowrap;
    }
    .note-worklog {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      .note-field {
        flex-basis: 45%;
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
        font-size: 13px;
        :deep(span) {
          display: inline-flex;
          align-items: center;
          gap: 7px;
        }
      }
      .note-task {
        flex-wrap: wrap;
        :deep(input:first-child) {
          flex: 2;
        }
        :deep(select),
        :deep(input[type="date"]) {
          flex: 1;
        }
      }
      .note-weekdays {
        display: flex;
        align-items: center;
        gap: 10px;
      }
      .note-reminder {
        display: flex;
        flex-direction: column;
        gap: 10px;
        padding: 12px;
        background: var(--color-panel-soft);
        border-radius: 8px;
      }
      .note-source {
        font-size: 12px;
        color: var(--color-text-muted);
        :deep(pre) {
          white-space: pre-wrap;
          color: var(--color-text);
          font: inherit;
          line-height: 1.7;
        }
      }
    }
    .note-button,
    .note-text-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      flex: none;
      gap: 5px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      padding: 7px 11px;
      font: inherit;
      cursor: pointer;
      background: var(--color-panel);
      color: var(--color-text);
      &:disabled {
        opacity: 0.5;
        cursor: default;
      }
    }
    .note-text-button {
      border: 0;
      padding: 4px;
      color: var(--color-primary);
      background: transparent;
      font-size: 12px;
    }
    .note-primary {
      background: var(--color-primary-solid);
      border-color: var(--color-primary-solid);
      color: white;
    }
    .note-caption {
      color: var(--color-text-muted);
      font-size: 11px;
      line-height: 1.7;
      margin: 0;
    }
    .note-warning {
      color: var(--color-warning);
      font-size: 12px;
      margin: 0;
    }
    .note-error {
      color: var(--color-danger);
      font-size: 12px;
      margin: 0;
    }
    .note-footer {
      position: sticky;
      bottom: 0;
      padding: 12px 0;
      background: var(--color-panel);
      border-top: 1px solid var(--color-line);
      .note-caption {
        flex: 1;
      }
    }
  }
}
</style>

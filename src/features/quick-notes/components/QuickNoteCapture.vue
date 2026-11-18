<template>
  <div class="note-capture" @pointerdown.stop>
    <button
      v-if="modeToggle"
      class="capture-toggle"
      :class="{ 'capture-toggle-selected': modelValue }"
      type="button"
      :aria-pressed="modelValue"
      :title="modelValue ? '已开启速记，发送时保存为便签' : '开启速记模式'"
      @click="$emit('update:modelValue', !modelValue)"
    >
      <StickyNote :size="14" />速记
    </button>
    <el-dropdown
      v-else
      class="capture-control"
      split-button
      size="small"
      trigger="click"
      placement="bottom-end"
      :disabled="starting || running"
      title="将内容整理为便签"
      @click="capture"
      @command="selectScope"
    >
      <StickyNote :size="14" />{{ running ? "整理中…" : "速记" }}
      <template #dropdown
        ><el-dropdown-menu class="capture-menu"
          ><el-dropdown-item
            v-for="option in scopes"
            :key="option.id"
            :command="option.id"
            :class="{ 'capture-option-selected': scope === option.id }"
            ><span class="capture-option-mark"
              ><Check v-if="scope === option.id" :size="13" /></span
            >{{ option.label }}</el-dropdown-item
          ></el-dropdown-menu
        ></template
      >
    </el-dropdown>
    <div
      v-if="(feedback || error) && !petFeedback"
      class="capture-feedback"
      role="status"
    >
      <span :class="{ 'capture-error': error }">{{ error || feedback }}</span>
      <button v-if="running" type="button" class="capture-link" @click="cancel">
        取消
      </button>
      <template v-if="saved"
        ><button type="button" class="capture-link" @click="editSaved">
          编辑</button
        ><button type="button" class="capture-link" @click="viewSaved">
          查看</button
        ><button type="button" class="capture-link" @click="undo">
          撤销
        </button></template
      >
      <button
        v-if="error && sourceText"
        type="button"
        class="capture-link"
        @click="manualSource"
      >
        手动保存原文
      </button>
      <button
        v-if="!running"
        type="button"
        class="capture-link"
        aria-label="收起速记提示"
        @click="clearFeedback"
      >
        <X :size="12" />
      </button>
    </div>
    <QuickNoteEditor
      v-if="editing"
      :note="editing"
      :folders="folders"
      @close="editing = null"
      @saved="manualSaved"
    />
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"
import { Check, StickyNote, X } from "lucide-vue-next"
import { ElDropdown, ElDropdownItem, ElDropdownMenu } from "element-plus"
import "element-plus/es/components/dropdown/style/css"
import { quickNotesApi } from "@/api/modules/quick-notes"
import QuickNoteEditor from "@/features/quick-notes/components/QuickNoteEditor.vue"
const props = defineProps({
  draft: { type: String, default: "" },
  messages: { type: Array, default: () => [] },
  model: { type: Object, default: () => ({}) },
  followModel: Boolean,
  modeToggle: Boolean,
  modelValue: Boolean,
  threadId: { type: String, default: "" },
  surface: { type: String, default: "assistant" }
})
const emit = defineEmits(["captured", "update:modelValue", "busy"])
// 提示独立收起，不改变后台任务和已保存的便签。
function clearFeedback() {
  feedback.value = ""
  error.value = ""
}
const scope = ref("auto")
// 速记范围使用项目已有的组合下拉按钮，保留明确箭头与当前选中项。
const scopes = [
  { id: "auto", label: "自动范围" },
  { id: "input", label: "当前输入" },
  { id: "last", label: "最近一轮" },
  { id: "conversation", label: "当前会话" },
  { id: "split", label: "按事项拆分" },
  { id: "manual", label: "手动记录" }
]
function selectScope(value) {
  scope.value = value
}
const starting = ref(false)
const task = ref(null)
const saved = ref(null)
const feedback = ref("")
const error = ref("")
const sourceText = ref("")
const editing = ref(null)
const folders = ref([])
const running = computed(() => task.value?.status === "running")
// 桌面速记由桌宠或输入框既有回复区域反馈，不额外浮出进度面板。
const petFeedback = computed(
  () => props.surface === "desktop" && Boolean(task.value?.id)
)
watch([starting, running], () => emit("busy", starting.value || running.value))
defineExpose({ capture })
let poll
let disposed = false
let draftSnapshot = ""
// 点击时捕获内容和来源，不读取其他会话；后台保存结束后才清理相同的输入草稿。
function contents() {
  const selection = window.getSelection()?.toString().trim()
  if (scope.value === "auto" && selection)
    return { text: selection, ids: [], fromInput: false }
  if (
    scope.value === "input" ||
    (["auto", "split"].includes(scope.value) && props.draft.trim())
  )
    return { text: props.draft, ids: [], fromInput: true }
  const complete = props.messages.filter(
    (message) => !message.pending && message.text && !message.error
  )
  const lastReply = complete.findLastIndex(
    (message) => message.role === "assistant"
  )
  const last = complete
    .slice(0, lastReply + 1)
    .findLastIndex((message) => message.role === "user")
  const selected =
    scope.value === "conversation"
      ? complete
      : lastReply < 0
        ? []
        : complete.slice(last < 0 ? lastReply : last, lastReply + 1)
  return {
    text: selected
      .map(
        (message) =>
          `${message.role === "user" ? "用户" : "助手"}：${message.text}`
      )
      .join("\n\n"),
    ids: selected.map((message) => message.id).filter(Boolean),
    fromInput: false
  }
}
async function capture() {
  if (starting.value || running.value) return
  const selected = contents()
  if (scope.value === "manual" || !selected.text.trim())
    return manualSource(props.draft)
  starting.value = true
  error.value = ""
  saved.value = null
  feedback.value = "正在保存原文并整理…"
  sourceText.value = selected.text
  draftSnapshot = selected.fromInput ? props.draft : ""
  try {
    task.value = await quickNotesApi.invoke("capture", {
      requestId: crypto.randomUUID(),
      text: selected.text,
      model: props.followModel
        ? undefined
        : {
            targetId: props.model.targetId,
            model: props.model.model,
            protocol: props.model.protocol
          },
      source: {
        type: props.surface,
        threadId: props.threadId,
        messageIds: selected.ids,
        scope: scope.value,
        timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone
      }
    })
    if (disposed) return
    poll = window.setInterval(check, 1200)
    await check()
  } catch (failure) {
    error.value = failure.message || String(failure)
    feedback.value = ""
  } finally {
    starting.value = false
  }
}
async function check() {
  if (disposed || !task.value) return
  try {
    const result = await quickNotesApi.invoke("capture-state", {
      requestId: task.value.requestId
    })
    if (disposed) return
    task.value = result
    if (result.status === "running") return
    clearInterval(poll)
    if (result.status === "completed") {
      saved.value = result.result
      const catalog = await quickNotesApi.invoke("list", { limit: 1 })
      if (disposed) return
      const group =
        catalog.folders.find((item) => item.id === result.result.groupId)
          ?.name || "收件箱"
      feedback.value = result.result.notes
        ? `已按事项保存 ${result.result.notes.length} 条便签`
        : `已保存到 ${group} · ${result.result.tasks?.length || 0} 项待办`
      emit("captured", draftSnapshot)
    } else {
      error.value = result.error || "整理未完成，原文已保留"
      feedback.value = ""
    }
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
  }
}
async function cancel() {
  await quickNotesApi.invoke("capture-cancel", {
    requestId: task.value.requestId
  })
  await check()
}
async function manualSource(text) {
  draftSnapshot = typeof text === "string" && text === props.draft ? text : ""
  if (props.surface === "desktop")
    return quickNotesApi.invoke("new", {
      content: typeof text === "string" ? text : sourceText.value
    })
  const data = await quickNotesApi.invoke("list", { limit: 1 })
  folders.value = data.folders
  editing.value = {
    content: typeof text === "string" ? text : sourceText.value
  }
}
async function editSaved() {
  if (props.surface === "desktop") return viewSaved()
  const [note, data] = await Promise.all([
    quickNotesApi.invoke("detail", { id: saved.value.id }),
    quickNotesApi.invoke("list", { limit: 1 })
  ])
  editing.value = note
  folders.value = data.folders
}
async function viewSaved() {
  await quickNotesApi.invoke("open", { id: saved.value.id })
}
async function undo() {
  try {
    for (const note of saved.value.notes || [saved.value])
      await quickNotesApi.invoke("trash", {
        id: note.id,
        revision: note.revision
      })
    saved.value = null
    feedback.value = "已移至回收站，提醒已暂停"
  } catch (failure) {
    error.value = failure.message || String(failure)
  }
}
function manualSaved(note) {
  editing.value = null
  saved.value = note
  error.value = ""
  feedback.value = "便签已保存"
  emit("captured", draftSnapshot)
}
onBeforeUnmount(() => {
  disposed = true
  clearInterval(poll)
})
</script>

<style scoped lang="less">
.note-capture {
  position: relative;
  display: inline-flex;
  flex: none;
  align-items: center;
  .capture-toggle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 8px;
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    border-radius: 7px;
    &.capture-toggle-selected {
      border-color: var(--color-primary);
      color: var(--color-primary);
      background: var(--color-primary-soft);
    }
    &:disabled {
      opacity: 0.5;
      cursor: default;
    }
  }
  .capture-control {
    :deep(.el-button) {
      height: 30px;
      padding: 6px 9px;
      color: var(--color-text);
      background: var(--color-panel);
      border-color: var(--color-line);
      font: inherit;
      font-size: 12px;
    }
    :deep(.el-dropdown__caret-button) {
      padding: 6px;
      border-left: 1px solid var(--color-line);
    }
    :deep(.el-button > span) {
      gap: 5px;
    }
  }
  .capture-feedback {
    position: absolute;
    z-index: 20;
    bottom: calc(100% + 10px);
    right: 0;
    display: flex;
    align-items: center;
    gap: 9px;
    flex-wrap: wrap;
    width: 310px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 9px;
    box-shadow: var(--shadow-panel);
    background: var(--color-panel);
    color: var(--color-text);
    font-size: 12px;
    line-height: 1.6;
    .capture-error {
      color: var(--color-danger);
    }
    .capture-link {
      padding: 0;
      border: 0;
      background: transparent;
      color: var(--color-primary);
      font: inherit;
      cursor: pointer;
    }
  }
}
.capture-menu {
  .capture-option-mark {
    display: inline-flex;
    width: 15px;
    align-items: center;
    margin-right: 7px;
  }
  :deep(.el-dropdown-menu__item) {
    font-size: 12px;
    line-height: 30px;
    color: var(--color-text);
  }
  :deep(.capture-option-selected) {
    color: var(--color-primary);
    background: var(--color-primary-soft);
  }
}
</style>

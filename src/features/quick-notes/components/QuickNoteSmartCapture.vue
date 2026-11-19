<template>
  <BaseModal
    v-show="open"
    class="smart-record-modal"
    @close="$emit('close')"
    @keydown.esc.stop="$emit('close')"
  >
    <div class="smart-record">
      <header class="smart-head">
        <span class="smart-symbol"><Sparkles :size="28" /></span>
        <div class="smart-heading">
          <h2 class="smart-title">智能记录</h2>
          <p class="smart-subtitle">
            粘贴原始内容，AI 将为你整理便签、待办与提醒。
          </p>
        </div>
        <button
          class="smart-icon"
          type="button"
          aria-label="关闭智能记录"
          @click="$emit('close')"
        >
          <X :size="19" />
        </button>
      </header>
      <div class="smart-columns">
        <section class="smart-source">
          <div class="smart-section-head">
            <span>原始内容</span
            ><button
              class="smart-link"
              type="button"
              :disabled="busy"
              @click="clearSource"
            >
              <Trash2 :size="13" />清空
            </button>
          </div>
          <textarea
            ref="input"
            v-model="text"
            class="smart-input"
            placeholder="输入想法、工作进展或待办事项，例如：明天下午三点提醒我提交发布检查。"
            maxlength="100000"
            aria-label="智能记录原文"
            :disabled="busy"
          ></textarea>
          <div class="smart-source-foot">
            <span>Aa · 共 {{ text.length }} 字</span
            ><label class="smart-import"
              ><Upload :size="14" />导入文本<input
                type="file"
                :disabled="busy"
                accept=".txt,.md,text/plain,text/markdown"
                @change="importText"
            /></label>
          </div>
          <div class="smart-model">
            <select
              v-model="customModel"
              class="smart-select"
              aria-label="智能记录模型模式"
              :disabled="busy"
            >
              <option :value="false">跟随助手模型</option>
              <option :value="true">单独选择模型</option>
            </select>
            <AssistantModelSelect
              v-if="customModel"
              :settings="model"
              :providers="providers"
              :codex-accounts="codexAccounts"
              :runtime-models="runtimeModels"
              :persist="false"
              @change="model = $event"
              @error="error = $event"
            />
            <button
              class="smart-button"
              type="button"
              :disabled="!text.trim() || busy"
              @click="analyze"
            >
              <Sparkles :size="14" />{{
                busy ? "分析中…" : preview ? "重新分析" : "分析内容"
              }}
            </button>
          </div>
        </section>
        <div class="smart-results">
          <section class="smart-analysis">
            <div class="smart-section-head">
              <span class="smart-section-title"
                ><Sparkles :size="18" />AI 分析结果</span
              ><span class="smart-badge">基于内容智能提取</span>
            </div>
            <template v-if="preview">
              <label class="smart-field"
                ><FileText :size="15" /><span>建议标题</span
                ><input v-model="preview.title" maxlength="160"
              /></label>
              <label class="smart-field"
                ><Layers :size="15" /><span>建议分类</span
                ><select
                  v-model="preview.categoryId"
                  @change="preview.newCategory = null"
                >
                  <option
                    v-for="folder in categories"
                    :key="folder.id"
                    :value="folder.id"
                  >
                    {{ folder.name }}
                  </option>
                </select></label
              >
              <p v-if="preview.newCategory?.name" class="smart-proposal">
                将新建分类：{{ preview.newCategory.name }}
              </p>
              <p v-if="preview.newGroup?.name" class="smart-proposal">
                将新建项目分组：{{ preview.newGroup.name }}
              </p>
              <div class="smart-tasks">
                <div class="smart-section-head">
                  <span
                    ><ListTodo :size="15" />提取待办 ·
                    {{ preview.tasks.length }} 项</span
                  ><button
                    class="smart-link"
                    type="button"
                    @click="addTaskSuggestion"
                  >
                    <Plus :size="13" />添加
                  </button>
                </div>
                <div
                  v-for="(task, index) in preview.tasks"
                  :key="task.id"
                  class="smart-task"
                >
                  <input
                    v-model="task.title"
                    placeholder="填写待办事项"
                  /><button
                    class="smart-icon"
                    type="button"
                    aria-label="移除建议待办"
                    @click="removeTaskSuggestion(index)"
                  >
                    <X :size="13" />
                  </button>
                </div>
              </div>
              <label class="smart-field"
                ><Tags :size="15" /><span>标签</span
                ><input v-model="tags" placeholder="使用逗号分隔"
              /></label>
              <div
                v-for="(plan, index) in preview.reminders"
                :key="index"
                class="smart-field"
              >
                <Bell :size="15" /><span>建议提醒</span
                ><input
                  v-model="plan.localAt"
                  type="datetime-local"
                  step="1"
                  :min="localInput(Date.now() + 1000, true)"
                /><button
                  class="smart-icon"
                  type="button"
                  aria-label="移除建议提醒"
                  @click="preview.reminders.splice(index, 1)"
                >
                  <X :size="13" />
                </button>
              </div>
              <p v-if="preview.pendingTime" class="smart-proposal">
                {{ preview.pendingTime }}
              </p>
            </template>
            <div v-else class="smart-empty">
              <Sparkles :size="34" /><span>{{
                busy ? "正在理解内容，提取待办和提醒…" : "从一段原始记录开始"
              }}</span>
              <p>分析后可以修改标题、分类、待办与时间，确认后再保存。</p>
            </div>
          </section>
          <section class="smart-preview">
            <div class="smart-section-head">
              <span class="smart-section-title"
                ><NotebookPen :size="17" />便签生成预览</span
              ><span class="smart-badge">{{
                preview ? "可编辑正文" : "等待分析"
              }}</span>
            </div>
            <template v-if="preview"
              ><span class="smart-preview-title">{{ preview.title }}</span
              ><textarea
                v-model="preview.content"
                class="smart-preview-content"
                aria-label="整理后正文"
              />
              <div class="smart-preview-meta">
                待办 {{ preview.tasks.length }} 项 <span>·</span> 提醒
                {{ preview.reminders.length }} 项 <span>·</span>
                {{ tags || "暂无标签" }}
              </div></template
            >
            <p v-else class="smart-preview-placeholder">
              整理后的内容会显示在这里，原始记录会一同保留。
            </p>
          </section>
        </div>
      </div>
      <p v-if="error" class="smart-error" role="alert">{{ error }}</p>
      <p v-if="feedback" class="smart-feedback" role="status">{{ feedback }}</p>
      <footer class="smart-footer">
        <button class="smart-button" type="button" @click="$emit('close')">
          取消
        </button>
        <button v-if="busy" class="smart-button" type="button" @click="cancel">
          停止分析
        </button>
        <button
          v-if="!applyOnly"
          class="smart-button"
          type="button"
          :disabled="busy || saving || !text.trim()"
          @click="saveOriginal"
        >
          仅保存原文
        </button>
        <button
          class="smart-button smart-primary"
          type="button"
          :disabled="busy || saving || !text.trim()"
          @click="preview ? confirm() : analyze()"
        >
          <Sparkles :size="16" />{{
            saving
              ? "保存中…"
              : preview
                ? applyOnly
                  ? "应用到便签"
                  : "确认并保存"
                : "分析并预览"
          }}
        </button>
      </footer>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue"
import {
  Sparkles,
  X,
  Trash2,
  Upload,
  FileText,
  Layers,
  ListTodo,
  Plus,
  Tags,
  Bell,
  NotebookPen
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import AssistantModelSelect from "@/features/assistant/components/AssistantModelSelect.vue"
import { quickNotesApi } from "@/api/modules/quick-notes"
import { localInput, reminderAt } from "@/features/quick-notes/constant"
const props = defineProps({
  open: Boolean,
  applyOnly: Boolean,
  initialText: { type: String, default: "" },
  folders: { type: Array, default: () => [] },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] }
})
const emit = defineEmits(["close", "saved", "apply"])
const text = ref("")
const input = ref(null)
const preview = ref(null)
const tags = ref("")
const model = ref({})
const customModel = ref(false)
const task = ref(null)
const busy = ref(false)
const saving = ref(false)
const error = ref("")
const feedback = ref("")
const categories = computed(() =>
  props.folders.filter(
    (folder) => folder.kind === "category" && !folder.archived
  )
)
let timer
let disposed = false
let snapshot = ""
function clearSource() {
  text.value = ""
  preview.value = null
}
watch(
  () => props.open,
  async (open) => {
    if (!open) return
    if (props.applyOnly && !busy.value) {
      text.value = props.initialText
      preview.value = null
    }
    await nextTick()
    input.value.focus()
    if (!customModel.value) {
      try {
        const options = await quickNotesApi.invoke("options")
        model.value = options.model
      } catch (failure) {
        error.value = failure.message || String(failure)
      }
    }
  }
)
watch(text, (value) => {
  if (preview.value && value !== snapshot) preview.value = null
})
// 预览请求沿用后台捕获，关闭弹框仍可完成；正式便签在确认之前不会创建。
async function analyze() {
  if (busy.value || !text.value.trim()) return
  busy.value = true
  error.value = ""
  feedback.value = ""
  preview.value = null
  snapshot = text.value
  try {
    task.value = await quickNotesApi.invoke("capture", {
      requestId: crypto.randomUUID(),
      text: snapshot,
      preview: true,
      model: customModel.value ? model.value : undefined,
      source: {
        type: "tool-preview",
        timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone
      }
    })
    await check()
  } catch (failure) {
    error.value = failure.message || String(failure)
    busy.value = false
  }
}
async function check() {
  if (disposed) return
  try {
    const result = await quickNotesApi.invoke("capture-state", {
      requestId: task.value.requestId
    })
    if (disposed) return
    task.value = result
    if (result.status === "running") {
      timer = window.setTimeout(check, 1000)
      return
    }
    busy.value = false
    if (result.status === "preview") {
      preview.value = structuredClone(result.result)
      preview.value.reminders = (preview.value.reminders || []).map((plan) => ({
        ...plan,
        localAt: localInput(plan.nextFireAt, true)
      }))
      tags.value = (preview.value.tags || []).join("，")
    } else {
      error.value = result.error || "分析未完成，原文已保留"
    }
  } catch (failure) {
    if (!disposed) {
      error.value = failure.message || String(failure)
      busy.value = false
    }
  }
}
async function confirm(original = false) {
  if (saving.value) return
  error.value = ""
  const note = original
    ? { content: text.value, tasks: [], reminders: [] }
    : {
        ...preview.value,
        tags: tags.value
          .split(/[,，]/)
          .map((value) => value.trim())
          .filter(Boolean),
        reminders: preview.value.reminders.map((plan) => {
          const at = reminderAt(plan)
          const { localAt, ...value } = plan
          return {
            ...value,
            nextFireAt: at,
            schedule: { ...plan.schedule, at }
          }
        })
      }
  if (note.reminders.some((plan) => !(plan.nextFireAt > Date.now()))) {
    error.value = "提醒时间必须晚于当前时间，请重新选择"
    return
  }
  saving.value = true
  try {
    if (props.applyOnly) {
      await quickNotesApi.invoke("capture-discard", { id: task.value.id })
      emit("apply", note)
      emit("close")
      preview.value = null
      task.value = null
      return
    }
    const saved =
      task.value?.status === "preview"
        ? await quickNotesApi.invoke("capture-save", {
            id: task.value.id,
            note
          })
        : await quickNotesApi.invoke("save", note)
    if (text.value === snapshot || original) text.value = ""
    preview.value = null
    task.value = null
    feedback.value = "便签已保存"
    emit("saved", saved)
    emit("close")
  } catch (failure) {
    error.value = failure.message || String(failure)
  } finally {
    saving.value = false
  }
}
// 任务与提醒复用同一 ID，移除建议任务时一起移除其关联提醒。
function addTaskSuggestion() {
  preview.value.tasks.push({
    id: crypto.randomUUID(),
    title: "",
    status: "pending"
  })
}
function removeTaskSuggestion(index) {
  const task = preview.value.tasks[index]
  preview.value.reminders = preview.value.reminders.filter(
    (plan) => plan.taskId !== task.id
  )
  preview.value.tasks.splice(index, 1)
}
function saveOriginal() {
  return confirm(true)
}
async function cancel() {
  await quickNotesApi.invoke("capture-cancel", {
    requestId: task.value.requestId
  })
  clearTimeout(timer)
  busy.value = false
}
async function importText(event) {
  const file = event.target.files[0]
  if (file) {
    const value = await file.text()
    text.value = value.slice(0, 100000)
    preview.value = null
  }
  event.target.value = ""
}
onBeforeUnmount(() => {
  disposed = true
  clearTimeout(timer)
})
</script>

<style scoped lang="less">
.smart-record-modal {
  :deep(.base-modal__panel) {
    width: min(1160px, calc(100vw - 64px));
    border-radius: 24px;
  }
  :deep(.base-modal__content) {
    padding: 0;
    overflow: auto;
  }
  .smart-record {
    padding: 26px 28px 24px;
    .smart-head {
      display: flex;
      align-items: center;
      gap: 16px;
      margin-bottom: 22px;
      .smart-symbol {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 55px;
        height: 55px;
        border: 1px solid var(--color-line);
        border-radius: 15px;
        background: var(--color-primary-soft);
        color: var(--color-primary);
      }
      .smart-heading {
        flex: 1;
        .smart-title {
          margin: 0 0 6px;
          font-size: 24px;
        }
        .smart-subtitle {
          margin: 0;
          font-size: 13px;
          color: var(--color-text-muted);
        }
      }
    }
    .smart-button,
    .smart-icon,
    .smart-link,
    .smart-select {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
      padding: 8px 13px;
      border: 1px solid var(--color-line);
      border-radius: 9px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;
      cursor: pointer;
      &:hover {
        border-color: var(--color-primary);
        color: var(--color-primary);
      }
      &:disabled {
        opacity: 0.5;
        cursor: default;
      }
    }
    .smart-icon {
      padding: 7px;
    }
    .smart-link {
      padding: 4px 7px;
      font-size: 11px;
    }
    .smart-primary {
      background: linear-gradient(135deg, #3b7abe, #245b96);
      color: white;
      border-color: #3671af;
      padding: 10px 23px;
      &:hover {
        color: white;
        border-color: #3671af;
      }
    }
    .smart-section-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      margin-bottom: 13px;
      font-size: 14px;
      .smart-section-title {
        display: inline-flex;
        gap: 8px;
        align-items: center;
      }
    }
    .smart-badge {
      border: 1px solid var(--color-line);
      border-radius: 20px;
      padding: 4px 9px;
      color: var(--color-text-muted);
      font-size: 10px;
    }
    .smart-columns {
      display: flex;
      align-items: stretch;
      gap: 18px;
      min-height: 420px;
      .smart-source {
        display: flex;
        flex-direction: column;
        flex: 0.9;
        min-width: 0;
        padding: 16px;
        border: 1px solid var(--color-line);
        border-radius: 16px;
        background: var(--color-panel-soft);
        .smart-input {
          flex: 1;
          width: 100%;
          min-height: 250px;
          padding: 14px;
          border: 1px solid var(--color-line);
          border-radius: 12px;
          color: var(--color-text);
          background: var(--color-panel);
          font: inherit;
          line-height: 1.9;
          resize: none;
          outline-color: var(--color-primary);
        }
        .smart-source-foot {
          display: flex;
          justify-content: space-between;
          align-items: center;
          gap: 10px;
          margin-top: 12px;
          color: var(--color-text-muted);
          font-size: 11px;
          .smart-import {
            display: flex;
            align-items: center;
            gap: 6px;
            cursor: pointer;
            :deep(input) {
              display: none;
            }
          }
        }
        .smart-model {
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          gap: 8px;
          margin-top: 15px;
          font-size: 11px;
        }
      }
      .smart-results {
        display: flex;
        flex-direction: column;
        flex: 1.15;
        min-width: 0;
        gap: 15px;
        .smart-analysis,
        .smart-preview {
          padding: 17px;
          border: 1px solid var(--color-line);
          border-radius: 16px;
          background: var(--color-panel-soft);
        }
        .smart-analysis {
          flex: 1;
          .smart-field {
            display: flex;
            align-items: center;
            gap: 9px;
            min-width: 0;
            padding: 9px 10px;
            margin-bottom: 6px;
            border: 1px solid var(--color-line);
            border-radius: 9px;
            background: var(--color-panel);
            font-size: 12px;
            :deep(input),
            :deep(select) {
              min-width: 0;
              flex: 1;
              width: 100%;
              border: 0;
              padding: 4px;
              background: transparent;
              color: var(--color-text);
              font: inherit;
            }
          }
          .smart-tasks {
            padding: 10px;
            border: 1px solid var(--color-line);
            border-radius: 9px;
            margin-bottom: 6px;
            background: var(--color-panel);
            .smart-section-head {
              font-size: 12px;
              :deep(span) {
                display: flex;
                align-items: center;
                gap: 7px;
              }
            }
            .smart-task {
              display: flex;
              align-items: center;
              gap: 8px;
              margin-bottom: 4px;
              :deep(input) {
                flex: 1;
                min-width: 0;
                padding: 6px;
                border: 1px solid var(--color-line);
                border-radius: 6px;
                font: inherit;
                font-size: 12px;
                color: var(--color-text);
                background: transparent;
              }
            }
          }
          .smart-proposal {
            color: var(--color-warning);
            font-size: 11px;
          }
          .smart-empty {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 15px;
            min-height: 240px;
            color: var(--color-text-muted);
            font-size: 13px;
            :deep(p) {
              max-width: 260px;
              text-align: center;
              font-size: 12px;
              line-height: 1.8;
            }
          }
        }
        .smart-preview {
          .smart-preview-title {
            display: block;
            margin-bottom: 8px;
            font-size: 14px;
          }
          .smart-preview-content {
            display: block;
            width: 100%;
            min-height: 75px;
            border: 0;
            background: transparent;
            color: var(--color-text-muted);
            font: inherit;
            font-size: 12px;
            line-height: 1.8;
            resize: vertical;
          }
          .smart-preview-meta {
            display: flex;
            gap: 12px;
            padding-top: 10px;
            border-top: 1px solid var(--color-line);
            font-size: 10px;
            color: var(--color-text-muted);
          }
          .smart-preview-placeholder {
            margin: 0;
            color: var(--color-text-muted);
            font-size: 12px;
            line-height: 1.8;
          }
        }
      }
    }
    .smart-error {
      color: var(--color-danger);
      font-size: 12px;
    }
    .smart-feedback {
      color: var(--color-success);
      font-size: 12px;
    }
    .smart-footer {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding-top: 24px;
    }
  }
}
</style>

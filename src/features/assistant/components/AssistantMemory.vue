<template>
  <details class="assistant-memory">
    <summary class="assistant-memory-summary">
      <Brain :size="15" />上下文与公共记忆
      <span class="assistant-memory-usage">{{
        compressing
          ? "正在压缩…"
          : context
            ? `约 ${usage.toLocaleString()} / ${context.contextWindow.toLocaleString()} Token`
            : "查看"
      }}</span>
    </summary>
    <div class="assistant-memory-content">
      <div class="assistant-memory-context">
        <span
          >自动压缩{{ settings.autoCompress === false ? "已关闭" : "已开启" }} ·
          阈值 {{ settings.compressionThreshold || 75 }}%</span
        >
        <button
          class="assistant-memory-button"
          type="button"
          :disabled="
            busy ||
            loading ||
            !threadId ||
            !settings.model ||
            settings.targetId === 'local'
          "
          @click="$emit('compress')"
        >
          立即压缩
        </button>
      </div>
      <p class="assistant-memory-hint">
        摘要和活动上下文会在重启后恢复，原始历史仍可查看。Token
        为本地估算，已预留指令、工具和回复空间。
      </p>
      <details v-if="context?.summary" class="assistant-memory-checkpoint">
        <summary>查看当前会话摘要</summary>
        <MarkdownContent
          class="assistant-memory-checkpoint-text"
          :text="context.summary"
        />
      </details>
      <label class="assistant-memory-field">
        <span
          >公共记忆 ·
          {{
            settings.sharedMemoryEnabled === false
              ? "已关闭注入"
              : "所有会话共用"
          }}</span
        >
        <textarea
          v-model="draft"
          class="assistant-memory-input"
          aria-label="公共记忆"
          maxlength="12000"
          rows="3"
          :disabled="saving"
          placeholder="例如：请用简体中文回复。仅在你明确要求记住时，助手才会更新这里。"
        ></textarea>
      </label>
      <div class="assistant-memory-actions">
        <span class="assistant-memory-count"
          >{{ draft.length }} / 12000{{ dirty ? " · 未保存" : "" }}</span
        >
        <button
          class="assistant-memory-button"
          type="button"
          :disabled="busy || saving || loading"
          @click="reloadMemory"
        >
          重新加载
        </button>
        <button
          class="assistant-memory-button"
          type="button"
          :disabled="busy || saving || loading || !original"
          @click="save('')"
        >
          清空记忆
        </button>
        <button
          class="assistant-memory-button"
          type="button"
          :disabled="busy || saving || loading || !dirty"
          @click="save(draft)"
        >
          {{ saving ? "保存中…" : "保存记忆" }}
        </button>
      </div>
      <p v-if="error" class="assistant-memory-error" role="alert">
        {{ error }}
      </p>
    </div>
  </details>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"
import { Brain } from "lucide-vue-next"
import { assistantApi } from "@/api"
import MarkdownContent from "@/components/MarkdownContent.vue"

const props = defineProps({
  threadId: { type: String, default: "" },
  busy: { type: Boolean, default: false },
  compressing: { type: Boolean, default: false },
  refreshKey: { type: String, default: "" },
  settings: { type: Object, default: () => ({}) }
})
defineEmits(["compress"])
const context = ref(null)
const draft = ref("")
const original = ref("")
const loading = ref(false)
const saving = ref(false)
const error = ref("")
const dirty = computed(() => draft.value !== original.value)
const usage = computed(() =>
  context.value
    ? context.value.pendingTokens +
      context.value.summaryTokens +
      context.value.promptReserve
    : 0
)
let sequence = 0
let disposed = false

// 公共记忆草稿跨会话保留，刷新上下文时不会覆盖尚未保存的编辑。
async function load() {
  const current = ++sequence
  if (!props.threadId) {
    context.value = null
    return
  }
  loading.value = true
  try {
    const result = await assistantApi.context(props.threadId)
    if (disposed || current !== sequence) return
    context.value = result
    if (!dirty.value && !saving.value)
      draft.value = original.value = result.sharedMemory.text
    error.value = ""
  } catch (failure) {
    if (!disposed && current === sequence)
      error.value = failure.message || String(failure)
  } finally {
    if (!disposed && current === sequence) loading.value = false
  }
}
async function reloadMemory() {
  loading.value = true
  try {
    const result = await assistantApi.publicMemory()
    if (!disposed) draft.value = original.value = result.text
    error.value = ""
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
  } finally {
    if (!disposed) loading.value = false
  }
}
async function save(text) {
  if (props.busy || saving.value || loading.value) return
  saving.value = true
  error.value = ""
  try {
    const result = await assistantApi.savePublicMemory(text, original.value)
    if (disposed) return
    draft.value = original.value = result.text
    await load()
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
  } finally {
    if (!disposed) saving.value = false
  }
}
watch(() => [props.threadId, props.refreshKey, props.settings], load, {
  immediate: true
})
onBeforeUnmount(() => {
  disposed = true
  sequence += 1
})
</script>

<style scoped lang="less">
.assistant-memory {
  margin-bottom: 10px;
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
  .assistant-memory-summary {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 0;
    cursor: pointer;
    list-style: none;
    .assistant-memory-usage {
      margin-left: auto;
      font-size: var(--font-size-xs);
    }
  }
  .assistant-memory-content {
    display: flex;
    flex-direction: column;
    gap: 9px;
    max-height: 300px;
    overflow: auto;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    .assistant-memory-context,
    .assistant-memory-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      flex-wrap: wrap;
    }
    .assistant-memory-context {
      justify-content: space-between;
    }
    .assistant-memory-hint {
      margin: 0;
      font-size: var(--font-size-xs);
      line-height: 1.7;
    }
    .assistant-memory-checkpoint {
      .assistant-memory-checkpoint-text {
        max-height: 140px;
        overflow: auto;
        padding-top: 8px;
      }
    }
    .assistant-memory-field {
      display: flex;
      flex-direction: column;
      gap: 7px;
      .assistant-memory-input {
        min-height: 58px;
        resize: vertical;
        padding: 8px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel-soft);
        color: var(--color-text);
        font: inherit;
        line-height: 1.7;
      }
    }
    .assistant-memory-button {
      padding: 5px 9px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel-soft);
      color: var(--color-text);
      cursor: pointer;
      font: inherit;
      &:disabled {
        opacity: 0.45;
        cursor: default;
      }
    }
    .assistant-memory-actions {
      justify-content: flex-end;
      .assistant-memory-count {
        margin-right: auto;
        font-size: var(--font-size-xs);
      }
    }
    .assistant-memory-error {
      margin: 0;
      color: var(--color-danger);
    }
  }
}
</style>

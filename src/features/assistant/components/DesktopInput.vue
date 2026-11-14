<template>
  <section
    v-if="visible"
    class="desktop-assistant-input"
    aria-label="和助手说话"
    tabindex="-1"
    @keydown.esc.stop.prevent="close"
    @paste="pasteImages"
    @pointerdown="dragWindow"
  >
    <div class="desktop-assistant-composer">
      <button
        class="desktop-assistant-close"
        type="button"
        aria-label="收起输入框"
        title="收起 · Esc"
        @click="close"
      >
        <X :size="14" />
      </button>
      <div
        v-if="showResponse"
        class="desktop-assistant-response"
        aria-live="polite"
      >
        <MarkdownContent
          :text="task.text || task.error || (busy ? '思考中…' : '已停止回复')"
        />
      </div>
      <AssistantImages
        :images="images"
        editable
        @remove="images = images.filter((image) => image.id !== $event)"
      />
      <textarea
        ref="input"
        v-model="draft"
        class="desktop-assistant-textarea"
        aria-label="给助手发消息"
        placeholder="想和我说些什么？支持粘贴图片"
        maxlength="30000"
        rows="2"
        @keydown="handleKeydown"
      ></textarea>
      <p v-if="error" class="desktop-assistant-error" role="alert">
        {{ error }}
        <button
          class="desktop-assistant-settings"
          type="button"
          @click="openSettings"
        >
          设置模型
        </button>
      </p>
      <div class="desktop-assistant-actions">
        <span
          class="desktop-assistant-hint"
          title="Enter 发送 · Shift+Enter 换行"
          >{{
            busy ? "回复中，可先输入下一条" : "Enter 发送 · Shift+Enter 换行"
          }}</span
        >
        <AssistantModelSelect
          class="desktop-assistant-model"
          compact
          :settings="settings"
          :providers="catalog.providers || []"
          :codex-accounts="catalog.codexAccounts || []"
          :runtime-models="catalog.runtimeModels || []"
          :disabled="busy || submitting"
          @change="settings = $event"
          @saving="modelSaving = $event"
          @error="error = $event"
        />
        <button
          class="desktop-assistant-send"
          type="button"
          :aria-label="busy ? '停止回复' : '发送消息'"
          :title="busy ? '停止回复' : '发送消息'"
          :disabled="busy ? task.status === 'cancelling' : !canSend"
          @click="busy ? stop() : send()"
        >
          <Square v-if="busy" :size="13" /><ArrowUp v-else :size="17" />
        </button>
      </div>
    </div>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue"
import { ArrowUp, Square, X } from "lucide-vue-next"
import { appApi, assistantApi } from "@/api"
import AssistantModelSelect from "@/features/assistant/components/AssistantModelSelect.vue"
import AssistantImages from "@/features/assistant/components/AssistantImages.vue"
import { readAssistantImages } from "@/features/assistant/attachments"
import MarkdownContent from "@/components/MarkdownContent.vue"

const visible = ref(false)
const draft = ref("")
const error = ref("")
const input = ref(null)
const catalog = ref({})
const images = ref([])
const modelSaving = ref(false)
const settings = ref({})
const task = ref({})
const submitting = ref(false)
const busy = computed(() =>
  ["thinking", "working", "replying", "cancelling"].includes(task.value.status)
)
const showResponse = computed(
  () =>
    task.value.surface === "desktop" &&
    task.value.replyAtPet === false &&
    Boolean(task.value.runId)
)
const canSend = computed(
  () =>
    (draft.value.trim() || images.value.length) &&
    !modelSaving.value &&
    !submitting.value &&
    !busy.value &&
    settings.value.enabled !== false
)
let sequence = 0
let eventVersion = 0
let disposed = false
const listeners = []

function applyTask(snapshot) {
  task.value = snapshot
}

// 图片和未发送草稿一起保留，关闭输入框不会取消正在回复的任务。
async function pasteImages(event) {
  const files = Array.from(event.clipboardData?.files || [])
  if (!files.some((file) => file.type.startsWith("image/"))) return
  event.preventDefault()
  try {
    images.value = await readAssistantImages(files, images.value)
  } catch (failure) {
    error.value = failure.message || String(failure)
  }
}

async function open() {
  sequence += 1
  submitting.value = false
  visible.value = true
  error.value = ""
  await nextTick()
  input.value?.focus()
}

async function close() {
  sequence += 1
  visible.value = false
  await assistantApi.desktopClose()
}

// 只让输入框的空白区域拖动原生窗口，避免影响文本选择和按钮操作。
async function dragWindow(event) {
  if (
    event.button !== 0 ||
    event.target?.closest?.(
      "button, textarea, input, select, a, [role='button']"
    )
  )
    return
  try {
    await assistantApi.desktopDrag()
  } catch (failure) {
    error.value = failure.message || String(failure)
  }
}

async function send() {
  if (!canSend.value) return
  if (
    !settings.value.targetId ||
    !settings.value.model ||
    settings.value.targetId === "local"
  ) {
    error.value =
      settings.value.targetId === "local"
        ? "本地助手模型待添加，请先选择 Provider 模型。"
        : "请先在设置 → 功能扩展中选择助手模型。"
    return
  }
  const current = sequence
  const version = eventVersion
  const sentDraft = draft.value
  const message = sentDraft.trim()
  error.value = ""
  submitting.value = true
  try {
    const sentImages = images.value
    const snapshot = await assistantApi.desktopSend(message, sentImages)
    if (disposed || current !== sequence) return
    if (version === eventVersion) applyTask(snapshot)
    // 输入窗由用户手动关闭，发送期间新输入的草稿继续保留。
    if (draft.value === sentDraft) draft.value = ""
    images.value = images.value.filter((image) => !sentImages.includes(image))
    await nextTick()
    if (!disposed && current === sequence) input.value?.focus()
  } catch (failure) {
    if (!disposed && current === sequence)
      error.value = failure.message || String(failure)
  } finally {
    if (!disposed && current === sequence) submitting.value = false
  }
}

async function stop() {
  try {
    await assistantApi.cancel(task.value.runId)
  } catch (failure) {
    error.value = failure.message || String(failure)
  }
}

function handleKeydown(event) {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault()
    send()
  }
}

async function openSettings() {
  try {
    await appApi.showMainPanel({ toolId: "assistant-settings" })
    await close()
  } catch (failure) {
    error.value = failure.message || String(failure)
  }
}

onMounted(async () => {
  try {
    for (const subscribe of [
      () => assistantApi.onDesktopOpen(open),
      () =>
        assistantApi.onEvent((event) => {
          if (disposed) return
          if (event.type === "state") {
            eventVersion += 1
            applyTask(event.state)
          } else if (
            event.type === "text-delta" &&
            event.runId === task.value.runId &&
            busy.value
          ) {
            eventVersion += 1
            task.value = {
              ...task.value,
              status: "replying",
              text: (task.value.text || "") + event.text
            }
          }
        }),
      () =>
        appApi.onStateChangedReady((state) => {
          settings.value = state.appSettings?.agents?.assistant || {}
          if (state.providers) catalog.value = state
          if (settings.value.enabled === false) visible.value = false
        })
    ]) {
      const unsubscribe = await subscribe()
      if (disposed) {
        unsubscribe()
        return
      }
      listeners.push(unsubscribe)
    }
    catalog.value = await assistantApi.catalog()
    const state = await appApi.bootstrapQuickSwitch()
    if (disposed) return
    settings.value = state.appSettings?.agents?.assistant || {}
    const version = eventVersion
    const snapshot = await assistantApi.getState()
    if (disposed) return
    if (version === eventVersion) applyTask(snapshot)
    visible.value = true
    await nextTick()
    const result = await assistantApi.desktopReady()
    if (!disposed) {
      visible.value = result.visible
      if (result.visible) await open()
    }
  } catch (failure) {
    if (!disposed) {
      error.value = failure.message || String(failure)
      visible.value = true
      try {
        const result = await assistantApi.desktopReady()
        if (!disposed) visible.value = result.visible
      } catch {
        // 初始化错误保留在输入框中，下次快捷键可重新唤出。
      }
    }
  }
})

onBeforeUnmount(() => {
  disposed = true
  sequence += 1
  listeners.forEach((unsubscribe) => unsubscribe())
})
</script>

<style scoped lang="less">
.desktop-assistant-input {
  height: 100vh;
  padding: 10px;
  outline: none;
  .desktop-assistant-composer {
    position: relative;
    display: flex;
    max-height: 100%;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 14px;
    background: var(--color-panel);
    box-shadow: 0 4px 12px rgba(29, 71, 129, 0.14);
    color: var(--color-text);
    cursor: move;
    font-size: var(--font-size-sm);
    &:focus-within {
      border-color: var(--color-primary);
    }
    .desktop-assistant-close {
      position: absolute;
      top: 8px;
      right: 8px;
      z-index: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      width: 24px;
      height: 24px;
      padding: 0;
      border: 0;
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      cursor: pointer;
      &:hover {
        background: var(--color-panel-soft);
      }
    }
    .desktop-assistant-textarea {
      width: 100%;
      min-height: 48px;
      flex: none;
      padding: 4px 24px 4px 2px;
      resize: none;
      border: 0;
      outline: none;
      background: transparent;
      color: inherit;
      font: inherit;
      line-height: 1.6;
      &::placeholder {
        color: var(--color-text-muted);
      }
    }
    .desktop-assistant-error,
    .desktop-assistant-response {
      min-height: 0;
      margin: 0;
      overflow: auto;
      overflow-wrap: anywhere;
      line-height: 1.6;
    }
    .desktop-assistant-error {
      max-height: 42px;
      color: var(--color-danger);
      .desktop-assistant-settings {
        padding: 0 4px;
        border: 0;
        background: transparent;
        color: var(--color-primary);
        cursor: pointer;
        font: inherit;
      }
    }
    .desktop-assistant-response {
      flex: 1;
      max-height: 64px;
      padding: 0 24px 6px 0;
      border-bottom: 1px solid var(--color-line);
    }
    .desktop-assistant-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      flex: none;
      .desktop-assistant-model {
        width: 144px;
        min-width: 0;
        flex: none;
      }
      .desktop-assistant-hint {
        flex: 1;
        min-width: 0;
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }
      .desktop-assistant-send {
        display: flex;
        width: 32px;
        height: 32px;
        align-items: center;
        justify-content: center;
        flex: none;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: var(--color-primary-solid);
        color: #fff;
        cursor: pointer;
        &:disabled {
          opacity: 0.4;
          cursor: default;
        }
      }
    }
  }
}
</style>

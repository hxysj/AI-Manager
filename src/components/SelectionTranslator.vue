<template>
  <Teleport to="body">
    <div
      v-if="menu.visible"
      class="translation-context-menu"
      :style="{ left: `${menu.x}px`, top: `${menu.y}px` }"
      @pointerdown.stop
    >
      <button
        class="translation-menu-item"
        type="button"
        @click="translateSelection"
      >
        <Languages :size="15" />翻译
      </button>
      <button
        class="translation-menu-item"
        type="button"
        @click="copySelection"
      >
        <Copy :size="15" />复制
      </button>
    </div>
    <aside
      v-if="visible"
      ref="panel"
      class="selection-translator"
      :style="{ left: `${position.x}px`, top: `${position.y}px` }"
      aria-label="翻译结果"
      @pointerdown.stop
    >
      <header class="translator-header" @pointerdown="startDrag">
        <span data-emphasis
          >翻译 · {{ settings.targetLanguage || "简体中文" }}</span
        >
        <button
          class="translator-close"
          type="button"
          aria-label="关闭翻译"
          @pointerdown.stop
          @click="closeTranslator"
        >
          <X :size="16" />
        </button>
      </header>
      <div class="translator-body">
        <p class="translator-source">{{ sourceText }}</p>
        <p v-if="loading" class="translator-loading">翻译中…</p>
        <p v-else-if="errorMessage" class="translator-error">
          {{ errorMessage }}
        </p>
        <template v-else>
          <p class="translator-result">{{ result.translatedText }}</p>
          <div class="translator-footer">
            <span class="translator-meta"
              >{{ result.providerName }} · {{ result.model }}<br />{{
                result.usageKnown
                  ? `输入 ${result.inputTokens} · 输出 ${result.outputTokens} tokens`
                  : "未返回完整用量"
              }}</span
            >
            <button class="translator-copy" type="button" @click="copyResult">
              {{ copied ? "已复制" : "复制译文" }}
            </button>
          </div>
        </template>
      </div>
    </aside>
  </Teleport>
</template>

<script setup>
import { nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue"
import { Copy, Languages, X } from "lucide-vue-next"
import { translationApi } from "@/api"

const props = defineProps({
  activeView: { type: String, default: "" },
  settings: { type: Object, default: () => ({ enabled: false }) }
})
const menu = reactive({ visible: false, x: 0, y: 0, text: "" })
const visible = ref(false)
const loading = ref(false)
const sourceText = ref("")
const result = ref({})
const errorMessage = ref("")
const copied = ref(false)
const panel = ref(null)
const position = reactive({ x: 0, y: 0 })
let requestId = 0
let menuRequestId = 0
let monacoEditor = null
const dragging = reactive({ active: false, offsetX: 0, offsetY: 0 })

// 仅启用翻译后加载编辑器入口，与差异弹框共用同一个 Monaco 实例。
watch(
  () => props.settings.enabled,
  async (enabled) => {
    if (enabled)
      monacoEditor = (await import("monaco-editor/esm/vs/editor/editor.api"))
        .editor
  },
  { immediate: true }
)

// 捕获阶段统一处理普通文本、输入框和 Monaco，防止页面自己的菜单覆盖翻译。
function openContextMenu(event) {
  closeMenu()
  if (!props.settings.enabled) return
  const target = event.target
  if (!(target instanceof Element)) return
  if (target.closest('input[type="password"], [data-translation-disabled]'))
    return
  let text = ""
  if (target.closest(".monaco-editor")) {
    const active = monacoEditor
      ?.getEditors()
      .find((item) => item.getDomNode()?.contains(target))
    const selection = active?.getSelection()
    text = selection ? active.getModel()?.getValueInRange(selection) || "" : ""
  } else if (
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLInputElement
  ) {
    if (target.selectionStart != null && target.selectionEnd != null) {
      text = target.value.slice(target.selectionStart, target.selectionEnd)
    }
  } else {
    text = window.getSelection()?.toString() || ""
  }
  if (!text.trim()) return
  event.preventDefault()
  event.stopPropagation()
  menu.text = text
  menu.x = Math.max(8, Math.min(event.clientX, window.innerWidth - 156))
  menu.y = Math.max(8, Math.min(event.clientY, window.innerHeight - 90))
  menu.visible = true
}

async function translateSelection() {
  if (!props.settings.enabled || !menu.text.trim()) return
  const current = ++requestId
  sourceText.value = menu.text
  result.value = {}
  errorMessage.value = ""
  copied.value = false
  position.x = Math.max(12, Math.min(menu.x, window.innerWidth - 452))
  position.y = Math.max(12, Math.min(menu.y, window.innerHeight - 472))
  closeMenu()
  visible.value = true
  loading.value = true
  try {
    const translated = await translationApi.translateText({
      text: sourceText.value
    })
    if (current === requestId) result.value = translated
  } catch (error) {
    if (current === requestId)
      errorMessage.value = error.message || String(error)
  } finally {
    if (current === requestId) {
      loading.value = false
      await nextTick()
      fitPanel()
    }
  }
}

async function copySelection() {
  try {
    await navigator.clipboard.writeText(menu.text)
  } finally {
    closeMenu()
  }
}

async function copyResult() {
  try {
    await navigator.clipboard.writeText(result.value.translatedText)
    copied.value = true
  } catch (error) {
    errorMessage.value = `复制失败：${error.message || String(error)}`
  }
}

function closeMenu() {
  menuRequestId += 1
  menu.visible = false
}

function closeTranslator() {
  requestId += 1
  visible.value = false
  loading.value = false
  closeMenu()
}

function startDrag(event) {
  if (event.button !== 0 || !panel.value) return
  const rect = panel.value.getBoundingClientRect()
  dragging.active = true
  dragging.offsetX = event.clientX - rect.left
  dragging.offsetY = event.clientY - rect.top
  document.body.style.userSelect = "none"
  window.addEventListener("pointermove", moveDrag)
  window.addEventListener("pointerup", stopDrag, { once: true })
}

function moveDrag(event) {
  if (!dragging.active || !panel.value) return
  const rect = panel.value.getBoundingClientRect()
  position.x = Math.max(
    12,
    Math.min(
      event.clientX - dragging.offsetX,
      window.innerWidth - rect.width - 12
    )
  )
  position.y = Math.max(
    12,
    Math.min(
      event.clientY - dragging.offsetY,
      window.innerHeight - rect.height - 12
    )
  )
}

function stopDrag() {
  dragging.active = false
  document.body.style.userSelect = ""
  window.removeEventListener("pointermove", moveDrag)
}

function fitPanel() {
  closeMenu()
  const rect = panel.value?.getBoundingClientRect()
  if (!rect) return
  position.x = Math.max(
    12,
    Math.min(position.x, window.innerWidth - rect.width - 12)
  )
  position.y = Math.max(
    12,
    Math.min(position.y, window.innerHeight - rect.height - 12)
  )
}

function handleKey(event) {
  if (event.key === "Escape") closeTranslator()
}

watch(() => [props.activeView, props.settings.enabled], closeTranslator)
onMounted(() => {
  document.addEventListener("contextmenu", openContextMenu, true)
  document.addEventListener("pointerdown", closeMenu)
  document.addEventListener("scroll", closeMenu, true)
  document.addEventListener("keydown", handleKey)
  window.addEventListener("resize", fitPanel)
})
onBeforeUnmount(() => {
  closeTranslator()
  stopDrag()
  document.removeEventListener("contextmenu", openContextMenu, true)
  document.removeEventListener("pointerdown", closeMenu)
  document.removeEventListener("scroll", closeMenu, true)
  document.removeEventListener("keydown", handleKey)
  window.removeEventListener("resize", fitPanel)
})
</script>

<style scoped lang="less">
.translation-context-menu {
  position: fixed;
  z-index: 5000;
  width: 148px;
  padding: 4px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  box-shadow: var(--shadow-panel);
  .translation-menu-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border: 0;
    border-radius: 4px;
    color: var(--color-text);
    background: transparent;
    cursor: pointer;
    &:hover {
      background: var(--color-panel-soft);
    }
  }
}
.selection-translator {
  position: fixed;
  z-index: 4999;
  display: flex;
  flex-direction: column;
  width: 440px;
  max-width: calc(100vw - 24px);
  max-height: min(460px, calc(100vh - 24px));
  overflow: hidden;
  border: 1px solid var(--color-line-strong);
  border-radius: 8px;
  background: var(--color-panel);
  box-shadow: var(--shadow-panel);
  .translator-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    cursor: grab;
    .translator-close {
      display: flex;
      padding: 4px;
      border: 0;
      border-radius: 4px;
      color: var(--color-text-muted);
      background: transparent;
      cursor: pointer;
    }
  }
  .translator-body {
    min-height: 0;
    overflow: auto;
    padding: 14px;
    .translator-source,
    .translator-result,
    .translator-error,
    .translator-loading {
      margin: 0 0 12px;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      line-height: 1.7;
    }
    .translator-source {
      max-height: 110px;
      overflow: auto;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
    }
    .translator-result {
      border-top: 1px solid var(--color-line);
      padding-top: 12px;
    }
    .translator-error {
      color: var(--color-danger);
    }
    .translator-footer {
      display: flex;
      align-items: center;
      gap: 12px;
      .translator-meta {
        flex: 1;
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        overflow-wrap: anywhere;
      }
      .translator-copy {
        padding: 6px 10px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text);
        cursor: pointer;
        white-space: nowrap;
      }
    }
  }
}
</style>

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
      :class="[
        'selection-translator',
        { 'selection-translator-desktop': desktop }
      ]"
      :style="
        desktop ? {} : { left: `${position.x}px`, top: `${position.y}px` }
      "
      aria-label="翻译结果"
      @pointerdown.stop
    >
      <div class="translator-card">
        <!-- Header -->
        <header class="translator-header" @pointerdown="startDrag">
          <div class="translator-header-left">
            <div class="translator-header-icon">
              <Languages :size="18" />
            </div>
            <span class="translator-header-title">
              翻译 · {{ targetLanguageText }}
            </span>
          </div>
          <button
            class="translator-close"
            type="button"
            aria-label="关闭翻译"
            @pointerdown.stop
            @click="closeTranslator"
          >
            <X :size="18" />
          </button>
        </header>

        <!-- Body -->
        <div class="translator-body">
          <!-- 原文 Section -->
          <div class="translator-section">
            <div class="section-label">原文</div>
            <div class="source-box">
              <div class="source-content">{{ sourceText }}</div>
              <button
                class="box-copy-btn"
                type="button"
                :title="copiedSource ? '已复制' : '复制原文'"
                @click="copySource"
              >
                <Check v-if="copiedSource" :size="15" />
                <Copy v-else :size="15" />
              </button>
            </div>
          </div>

          <!-- 译文 Section -->
          <div class="translator-section">
            <div class="section-label-row">
              <span class="section-label">译文</span>
              <div v-if="loading" class="status-badge status-loading">
                <Loader2 :size="14" class="spin-icon" />
                <span>正在翻译…</span>
              </div>
              <div v-else-if="errorMessage" class="status-badge status-error">
                <AlertCircle :size="14" />
                <span>翻译失败</span>
              </div>
              <div
                v-else-if="result.translatedText"
                class="status-badge status-success"
              >
                <CheckCircle2 :size="15" />
                <span>翻译完成</span>
              </div>
            </div>

            <div
              class="result-box"
              :class="{
                'is-loading': loading,
                'has-error': Boolean(errorMessage)
              }"
            >
              <div v-if="loading" class="result-placeholder">
                正在翻译中，请稍候…
              </div>
              <div v-else-if="errorMessage" class="result-error-msg">
                {{ errorMessage }}
              </div>
              <div v-else class="result-content">
                {{ result.translatedText }}
              </div>
              <button
                v-if="result.translatedText"
                class="box-copy-btn box-copy-btn-result"
                type="button"
                :title="copied ? '已复制' : '复制译文'"
                @click="copyResult"
              >
                <Check v-if="copied" :size="15" />
                <Copy v-else :size="15" />
              </button>
            </div>
          </div>

          <!-- Footer -->
          <footer class="translator-footer">
            <div class="model-meta">
              <div class="model-icon">
                <Bot :size="18" />
              </div>
              <div class="model-text">
                <div class="model-name">{{ modelDisplayName }}</div>
                <div class="model-tokens">{{ tokenUsageText }}</div>
              </div>
            </div>
            <button
              class="copy-result-btn"
              type="button"
              :disabled="!result.translatedText || loading"
              @click="copyResult"
            >
              <Check v-if="copied" :size="15" />
              <Copy v-else :size="15" />
              <span>{{ copied ? "已复制" : "复制译文" }}</span>
            </button>
          </footer>
        </div>
      </div>
    </aside>
  </Teleport>
</template>

<script setup>
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch
} from "vue"
import {
  AlertCircle,
  Bot,
  Check,
  CheckCircle2,
  Copy,
  Languages,
  Loader2,
  X
} from "lucide-vue-next"
import { translationApi } from "@/api"

const props = defineProps({
  activeView: { type: String, default: "" },
  desktop: { type: Boolean, default: false },
  settings: { type: Object, default: () => ({ enabled: false }) }
})

const desktopSettings = reactive({ enabled: true, targetLanguage: "简体中文" })
const settings = computed(() =>
  props.desktop ? desktopSettings : props.settings
)
const menu = reactive({ visible: false, x: 0, y: 0, text: "" })
const visible = ref(false)
const loading = ref(false)
const sourceText = ref("")
const result = ref({})
const errorMessage = ref("")
const copied = ref(false)
const copiedSource = ref(false)
const panel = ref(null)
const position = reactive({ x: 0, y: 0 })
let requestId = 0
let menuRequestId = 0
let monacoEditor = null
let unsubscribeDesktop = null
let disposed = false
const dragging = reactive({ active: false, offsetX: 0, offsetY: 0 })

const targetLanguageText = computed(() => {
  return (
    result.value.targetLanguage || settings.value.targetLanguage || "简体中文"
  )
})

const modelDisplayName = computed(() => {
  if (result.value.providerName && result.value.model) {
    return `${result.value.providerName} · ${result.value.model}`
  }
  if (result.value.model) {
    return result.value.model
  }
  return settings.value.model || "Codex"
})

const tokenUsageText = computed(() => {
  if (result.value.usageKnown) {
    return `输入 ${result.value.inputTokens} · 输出 ${result.value.outputTokens} tokens`
  }
  if (loading.value) {
    return "计算用量中…"
  }
  return "未返回完整用量"
})

// 仅启用翻译后加载编辑器入口，与差异弹框共用同一个 Monaco 实例。
watch(
  () => props.settings.enabled,
  async (enabled) => {
    if (enabled && !props.desktop) {
      monacoEditor = (await import("monaco-editor/esm/vs/editor/editor.api"))
        .editor
    }
  },
  { immediate: true }
)

// 捕获阶段统一处理普通文本、输入框和 Monaco，防止页面自己的菜单覆盖翻译。
function openContextMenu(event) {
  closeMenu()
  if (props.desktop || !settings.value.enabled) return
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
  if (!settings.value.enabled || !menu.text.trim()) return
  const current = ++requestId
  sourceText.value = menu.text
  result.value = {}
  errorMessage.value = ""
  copied.value = false
  copiedSource.value = false
  position.x = Math.max(12, Math.min(menu.x, window.innerWidth - 472))
  position.y = Math.max(12, Math.min(menu.y, window.innerHeight - 492))
  closeMenu()
  visible.value = true
  loading.value = true
  try {
    const translated = await translationApi.translateText({
      text: sourceText.value
    })
    if (current === requestId) result.value = translated
  } catch (error) {
    if (current === requestId) {
      errorMessage.value = error.message || String(error)
    }
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

async function copySource() {
  if (!sourceText.value) return
  try {
    await navigator.clipboard.writeText(sourceText.value)
    copiedSource.value = true
    setTimeout(() => {
      copiedSource.value = false
    }, 2000)
  } catch (error) {
    console.error("复制原文失败:", error)
  }
}

async function copyResult() {
  if (!result.value.translatedText) return
  try {
    await navigator.clipboard.writeText(result.value.translatedText)
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 2000)
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
  if (props.desktop) {
    translationApi.desktopClose().catch((error) => {
      errorMessage.value = error.message || String(error)
      visible.value = true
    })
  }
}

function startDrag(event) {
  if (event.button !== 0 || !panel.value) return
  if (props.desktop) {
    translationApi.desktopDrag().catch((error) => {
      errorMessage.value = error.message || String(error)
    })
    return
  }
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
  if (props.desktop) return
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

// 原生窗口只传选区和错误；实际翻译继续走现有 API、历史和用量链路。
function showDesktopSelection(selection) {
  if (selection.targetLanguage) {
    desktopSettings.targetLanguage = selection.targetLanguage
  }
  if (selection.error) {
    requestId += 1
    sourceText.value = ""
    result.value = {}
    loading.value = false
    errorMessage.value = selection.error
    visible.value = true
    return
  }
  menu.text = selection.text || ""
  translateSelection()
}

watch(
  () => [props.activeView, props.settings.enabled],
  () => {
    if (!props.desktop) closeTranslator()
  }
)

onMounted(async () => {
  if (props.desktop) {
    document.addEventListener("keydown", handleKey)
    try {
      unsubscribeDesktop =
        await translationApi.onDesktopSelection(showDesktopSelection)
      if (disposed) {
        unsubscribeDesktop()
        return
      }
      const selection = await translationApi.desktopReady()
      if (!disposed) showDesktopSelection(selection)
    } catch (error) {
      errorMessage.value = error.message || String(error)
      visible.value = true
    }
    return
  }
  document.addEventListener("contextmenu", openContextMenu, true)
  document.addEventListener("pointerdown", closeMenu)
  document.addEventListener("scroll", closeMenu, true)
  document.addEventListener("keydown", handleKey)
  window.addEventListener("resize", fitPanel)
})

onBeforeUnmount(() => {
  disposed = true
  requestId += 1
  unsubscribeDesktop?.()
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
  width: 460px;
  max-width: calc(100vw - 24px);
  border-radius: 12px;
  box-shadow:
    0 16px 40px rgba(0, 0, 0, 0.16),
    0 4px 12px rgba(0, 0, 0, 0.08);
  background: var(--color-panel);
  border: 1px solid var(--color-line);
  overflow: hidden;

  &.selection-translator-desktop {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    max-width: none;
    max-height: none;
    border-radius: 0;
    border: none;
    box-shadow: none;
    background: transparent;
    padding: 8px;
    box-sizing: border-box;

    .translator-card {
      width: 100%;
      height: 100%;
      border-radius: 12px;
      border: 1px solid var(--color-line);
      box-shadow:
        0 16px 40px rgba(0, 0, 0, 0.16),
        0 4px 12px rgba(0, 0, 0, 0.08);
      background: var(--color-panel);
    }
  }

  .translator-card {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--color-panel);
    overflow: hidden;

    .translator-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 14px 18px;
      border-bottom: 1px solid var(--color-line);
      background: var(--color-panel);
      cursor: grab;
      user-select: none;

      .translator-header-left {
        display: flex;
        align-items: center;
        gap: 10px;

        .translator-header-icon {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 32px;
          height: 32px;
          border-radius: 8px;
          background: #eff6ff;
          color: #2563eb;
        }

        .translator-header-title {
          font-size: 16px;
          font-weight: 600;
          color: var(--color-text);
        }
      }

      .translator-close {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 30px;
        height: 30px;
        border: none;
        border-radius: 6px;
        color: var(--color-text-muted);
        background: transparent;
        cursor: pointer;
        transition:
          background 0.15s ease,
          color 0.15s ease;

        &:hover {
          background: var(--color-panel-soft);
          color: var(--color-text);
        }
      }
    }

    .translator-body {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-height: 0;
      padding: 16px 20px 20px;
      overflow-y: auto;

      .translator-section {
        display: flex;
        flex-direction: column;

        .section-label {
          font-size: 13px;
          font-weight: 500;
          color: var(--color-text-muted);
          margin-bottom: 6px;
        }

        .section-label-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-top: 14px;
          margin-bottom: 6px;

          .status-badge {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            font-size: 13px;
            font-weight: 500;

            &.status-loading {
              color: #2563eb;
            }

            &.status-success {
              color: #16a34a;
            }

            &.status-error {
              color: #ef4444;
            }

            .spin-icon {
              animation: spin 1s linear infinite;
            }
          }
        }

        .source-box {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 10px;
          padding: 12px 14px;
          border-radius: 8px;
          border: 1px solid var(--color-line);
          background: var(--color-panel-soft);
          min-height: 48px;

          .source-content {
            flex: 1;
            font-size: 15px;
            font-weight: 500;
            color: var(--color-text);
            line-height: 1.5;
            white-space: pre-wrap;
            word-break: break-word;
            max-height: 100px;
            overflow-y: auto;
          }
        }

        .result-box {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 10px;
          padding: 14px 16px;
          border-radius: 8px;
          border: 1px solid #bfdbfe;
          background: #eff6ff;
          min-height: 64px;

          &.has-error {
            border-color: var(--color-danger-line);
            background: var(--color-danger-soft);
          }

          .result-placeholder {
            font-size: 14px;
            color: #93c5fd;
            line-height: 1.5;
          }

          .result-error-msg {
            font-size: 14px;
            color: var(--color-danger);
            line-height: 1.5;
            word-break: break-word;
          }

          .result-content {
            flex: 1;
            font-size: 16px;
            font-weight: 600;
            color: var(--color-text);
            line-height: 1.5;
            white-space: pre-wrap;
            word-break: break-word;
            max-height: 140px;
            overflow-y: auto;
          }
        }

        .box-copy-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          padding: 4px;
          border: none;
          border-radius: 4px;
          background: transparent;
          color: #94a3b8;
          cursor: pointer;
          flex-shrink: 0;
          transition:
            background 0.15s ease,
            color 0.15s ease;

          &:hover {
            background: var(--color-line);
            color: var(--color-text);
          }

          &.box-copy-btn-result {
            color: #93c5fd;

            &:hover {
              background: #dbeafe;
              color: #2563eb;
            }
          }
        }
      }

      .translator-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        margin-top: 16px;
        padding-top: 4px;

        .model-meta {
          display: flex;
          align-items: center;
          gap: 10px;

          .model-icon {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 36px;
            height: 36px;
            border-radius: 8px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            color: var(--color-text-muted);
          }

          .model-text {
            display: flex;
            flex-direction: column;
            gap: 2px;

            .model-name {
              font-size: 13px;
              font-weight: 500;
              color: var(--color-text);
            }

            .model-tokens {
              font-size: 12px;
              color: var(--color-text-muted);
            }
          }
        }

        .copy-result-btn {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          padding: 8px 18px;
          border: none;
          border-radius: 8px;
          background: #2563eb;
          color: #ffffff;
          font-size: 13px;
          font-weight: 500;
          cursor: pointer;
          box-shadow: 0 1px 3px rgba(37, 99, 235, 0.2);
          transition:
            background 0.15s ease,
            opacity 0.15s ease;

          &:hover:not(:disabled) {
            background: #1d4ed8;
          }

          &:disabled {
            opacity: 0.6;
            cursor: not-allowed;
          }
        }
      }
    }
  }
}

:root[data-theme="dark"] {
  .selection-translator {
    .translator-card {
      .translator-header {
        .translator-header-left {
          .translator-header-icon {
            background: rgba(37, 99, 235, 0.2);
            color: #60a5fa;
          }
        }
      }

      .translator-body {
        .translator-section {
          .result-box {
            background: rgba(37, 99, 235, 0.15);
            border-color: rgba(96, 165, 250, 0.3);

            .result-placeholder {
              color: #60a5fa;
            }
          }

          .box-copy-btn.box-copy-btn-result {
            color: #60a5fa;

            &:hover {
              background: rgba(37, 99, 235, 0.25);
              color: #93c5fd;
            }
          }
        }
      }
    }
  }
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>

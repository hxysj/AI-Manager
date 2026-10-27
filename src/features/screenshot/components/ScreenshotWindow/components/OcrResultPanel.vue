<template>
  <section class="ocr-result-dialog" role="dialog" aria-label="OCR 识别结果">
    <header class="ocr-result-head" @pointerdown="dragResultWindow">
      <span class="ocr-result-title">识别结果</span>
      <span class="ocr-result-engine">{{
        result?.engine || (loading ? "正在识别" : "识别失败")
      }}</span>
      <button
        type="button"
        class="ocr-result-close"
        aria-label="关闭识别结果"
        @pointerdown.stop
        @click="closeResultWindow"
      >
        ×
      </button>
    </header>
    <div
      v-if="loading"
      class="ocr-result-loading"
      role="status"
      aria-live="polite"
    >
      <span class="ocr-loading-spinner" aria-hidden="true" />
      <span class="ocr-loading-title">正在识别文字</span>
      <span class="ocr-loading-description">正在分析贴图内容，请稍候…</span>
    </div>
    <div
      v-else-if="result && translatedLines.length"
      class="ocr-translation-list"
    >
      <div
        v-for="(line, index) in sourceLines"
        :key="index"
        class="ocr-translation-row"
      >
        <div class="ocr-source-line">{{ line }}</div>
        <div class="ocr-translated-line">{{ translatedLines[index] }}</div>
      </div>
    </div>
    <textarea
      v-else-if="result"
      v-model="result.text"
      class="ocr-result-text"
      spellcheck="false"
      aria-label="识别文字"
    />
    <div v-else class="ocr-result-failed">未能完成文字识别</div>
    <footer v-if="!loading" class="ocr-result-actions">
      <span v-if="result && !result.text" class="ocr-result-empty"
        >没有识别到文字</span
      >
      <span v-else-if="result">{{ sourceLines.length }} 行文字</span>
      <span v-else>识别失败</span>
      <button
        v-if="translationEnabled && result?.text"
        type="button"
        class="ocr-result-button"
        :disabled="translating"
        @click="translateResult"
      >
        {{
          translating ? "翻译中…" : translatedLines.length ? "重新翻译" : "翻译"
        }}
      </button>
      <button
        type="button"
        class="ocr-result-button"
        :disabled="!result?.text"
        @click="copyResultText"
      >
        复制文字
      </button>
      <button
        type="button"
        class="ocr-result-button primary"
        @click="closeResultWindow"
      >
        关闭
      </button>
    </footer>
    <div v-if="translationError" class="ocr-result-error" role="alert">
      {{ translationError }}
    </div>
    <div v-if="error" class="ocr-result-error" role="alert">{{ error }}</div>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue"
import { translationApi } from "@/api"
import { screenshotApi } from "@/api/modules/screenshot"

const props = defineProps({
  resultId: {
    type: String,
    required: true
  }
})

const loading = ref(true)
const result = ref(null)
const error = ref("")
const translationEnabled = ref(false)
const translating = ref(false)
const translatedLines = ref([])
const translationError = ref("")
const sourceLines = computed(() => {
  const text = String(result.value?.text || "").replace(/\r\n/g, "\n")
  return text ? text.split("\n") : []
})
let disposed = false

async function resizeResultWindow(value) {
  try {
    // 结果窗从最小宽度起步，仅在文本或模型名称需要时向外扩展。
    const canvas = window.document.createElement("canvas")
    const context = canvas.getContext("2d")
    if (!context) throw new Error("无法测量识别结果宽度")
    context.font = '14px Consolas, "Microsoft YaHei", sans-serif'
    const textWidth = String(value.text || "")
      .split("\n")
      .reduce(
        (width, line) => Math.max(width, context.measureText(line).width),
        0
      )
    const engineWidth = context.measureText(value.engine || "").width
    await screenshotApi.call("ocr-result-action", {
      id: props.resultId,
      action: "resize",
      width: Math.ceil(Math.max(420, textWidth + 64, engineWidth + 160))
    })
  } catch (cause) {
    if (!disposed) error.value = `调整识别结果窗口失败：${String(cause)}`
  }
}

async function recognizeDocument() {
  let sourceUrl = ""
  try {
    const task = await screenshotApi.call("ocr-result", {
      id: props.resultId
    })
    if (disposed) return
    await nextTick()
    // loading 首帧完成后再显示窗口，避免弹出空白内容。
    await screenshotApi.call("ocr-result-action", {
      id: props.resultId,
      action: "ready"
    })
    const bytes = await screenshotApi.image(task.documentId, "output")
    const blob = new Blob([bytes], { type: "image/png" })
    sourceUrl = URL.createObjectURL(blob)
    const settings = await screenshotApi.call("status")
    let recognized
    if (settings?.ocr?.useLargeModel) {
      const source = await new Promise((resolve, reject) => {
        const reader = new FileReader()
        reader.onload = () => resolve(reader.result)
        reader.onerror = () =>
          reject(reader.error || new Error("OCR 图片读取失败"))
        reader.readAsDataURL(blob)
      })
      recognized = await screenshotApi.call("ocr", { source })
    } else {
      // 本地模型按需加载到 OCR 窗口，截图页和贴图页不承担推理开销。
      const { recognizePpocr } = await import("@/features/screenshot/ocr")
      recognized = await recognizePpocr(sourceUrl)
    }
    if (disposed) return
    result.value = recognized
    loading.value = false
    await nextTick()
    await resizeResultWindow(recognized)
  } catch (cause) {
    if (!disposed) error.value = `文字识别失败：${String(cause)}`
  } finally {
    if (sourceUrl) URL.revokeObjectURL(sourceUrl)
    if (!disposed) loading.value = false
  }
}

async function dragResultWindow(event) {
  if (event.button !== 0) return
  try {
    await screenshotApi.call("ocr-result-action", {
      id: props.resultId,
      action: "drag"
    })
  } catch (cause) {
    error.value = `移动识别结果失败：${String(cause)}`
  }
}

async function closeResultWindow() {
  try {
    await screenshotApi.call("ocr-result-action", {
      id: props.resultId,
      action: "close"
    })
  } catch (cause) {
    error.value = `关闭识别结果失败：${String(cause)}`
  }
}

async function copyResultText() {
  if (!result.value?.text) return
  try {
    await navigator.clipboard.writeText(result.value.text)
    error.value = ""
  } catch (cause) {
    error.value = `复制识别文字失败：${String(cause)}`
  }
}

async function keydown(event) {
  if (event.key !== "Escape" || event.isComposing) return
  event.preventDefault()
  await closeResultWindow()
}

async function loadTranslationStatus() {
  try {
    const status = await translationApi.desktopStatus()
    if (!disposed) translationEnabled.value = Boolean(status?.enabled)
  } catch {
    // 状态读取失败时隐藏翻译入口，不影响 OCR 结果使用。
    if (!disposed) translationEnabled.value = false
  }
}

async function translateResult() {
  if (translating.value || !result.value?.text) return
  const lines = [...sourceLines.value]
  translating.value = true
  translationError.value = ""
  try {
    const translated = await translationApi.translateText({
      text: lines.join("\n")
    })
    let values = String(translated?.translatedText || "")
      .replace(/\r\n/g, "\n")
      .split("\n")
    if (values.length !== lines.length) {
      // 翻译服务未保留行结构时逐行补译，保证原文和译文稳定对应。
      values = await Promise.all(
        lines.map(async (line) => {
          if (!line.trim()) return ""
          const item = await translationApi.translateText({ text: line })
          return String(item?.translatedText || "")
        })
      )
    }
    if (disposed) return
    translatedLines.value = values
    await nextTick()
    await resizeResultWindow({
      text: lines
        .flatMap((line, index) => [line, translatedLines.value[index] || ""])
        .join("\n"),
      engine: result.value.engine
    })
  } catch (cause) {
    if (!disposed) {
      translationError.value = `翻译失败：${cause?.message || String(cause)}`
    }
  } finally {
    if (!disposed) translating.value = false
  }
}

onMounted(() => {
  window.addEventListener("keydown", keydown)
  loadTranslationStatus()
  recognizeDocument()
})

onBeforeUnmount(() => {
  disposed = true
  window.removeEventListener("keydown", keydown)
})
</script>

<style scoped lang="less">
.ocr-result-dialog {
  display: flex;
  width: 100vw;
  height: 100vh;
  flex-direction: column;
  box-sizing: border-box;
  overflow: hidden;
  border: 1px solid #b7c4d4;
  border-radius: 10px;
  background: #fff;
  color: #1f2937;
  box-shadow: 0 18px 60px #0005;
  .ocr-result-head,
  .ocr-result-actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
  }
  .ocr-result-head {
    border-bottom: 1px solid #e5e7eb;
    cursor: move;
    user-select: none;
    .ocr-result-title {
      flex-shrink: 0;
      font-size: 15px;
    }
    .ocr-result-engine {
      overflow: hidden;
      color: #64748b;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-size: 12px;
    }
    .ocr-result-close {
      margin-left: auto;
      border: 0;
      background: transparent;
      color: #64748b;
      cursor: pointer;
      font-size: 24px;
      line-height: 20px;
    }
  }
  .ocr-result-loading,
  .ocr-result-failed {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 9px;
    padding: 24px;
    box-sizing: border-box;
    color: #475569;
    text-align: center;
    font-size: 13px;
    .ocr-loading-spinner {
      width: 20px;
      height: 20px;
      flex: 0 0 auto;
      border: 2px solid #cbd5e1;
      border-top-color: #2563eb;
      border-radius: 50%;
      animation: ocr-loading-spin 0.8s linear infinite;
    }
    .ocr-loading-description {
      color: #94a3b8;
      font-size: 12px;
    }
  }
  .ocr-result-failed {
    color: #b45309;
  }
  .ocr-result-text {
    min-height: 0;
    flex: 1;
    padding: 14px 16px;
    border: 0;
    outline: 0;
    resize: none;
    color: #111827;
    font:
      14px/1.7 Consolas,
      "Microsoft YaHei",
      sans-serif;
  }
  .ocr-translation-list {
    min-height: 0;
    flex: 1;
    overflow: auto;
    padding: 8px 16px 14px;
    box-sizing: border-box;
    font:
      14px/1.7 Consolas,
      "Microsoft YaHei",
      sans-serif;
    .ocr-translation-row {
      padding: 8px 0;
      border-bottom: 1px solid #e5e7eb;
      &:last-child {
        border-bottom: 0;
      }
      .ocr-source-line,
      .ocr-translated-line {
        min-height: 24px;
        overflow-wrap: anywhere;
        white-space: pre-wrap;
      }
      .ocr-source-line {
        color: #111827;
      }
      .ocr-translated-line {
        margin-top: 3px;
        color: #2563eb;
      }
    }
  }
  .ocr-result-actions {
    border-top: 1px solid #e5e7eb;
    color: #64748b;
    font-size: 12px;
    .ocr-result-empty {
      color: #b45309;
    }
    .ocr-result-button {
      padding: 6px 12px;
      border: 1px solid #cbd5e1;
      border-radius: 6px;
      background: #fff;
      color: #334155;
      cursor: pointer;
      &:disabled {
        cursor: not-allowed;
        opacity: 0.45;
      }
      &.primary {
        margin-left: auto;
        border-color: #2563eb;
        background: #2563eb;
        color: #fff;
      }
    }
  }
  .ocr-result-error {
    padding: 8px 16px;
    color: #b45309;
    font-size: 12px;
  }
}

@keyframes ocr-loading-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

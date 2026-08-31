<template>
  <section class="image-extractor">
    <section class="image-extractor-panel image-extractor-input-panel">
      <header class="image-extractor-panel-head">
        <div class="image-extractor-panel-title">
          <strong class="image-extractor-title">字符串内容</strong>
          <span class="image-extractor-subtitle"
            >文本 / Markdown / HTML / JSON</span
          >
        </div>
        <div class="image-extractor-actions">
          <button
            class="image-extractor-button"
            type="button"
            @click="loadSample"
          >
            <FileText :size="15" />
            示例
          </button>
          <button
            class="image-extractor-button"
            type="button"
            @click="clearContent"
          >
            <Trash2 :size="15" />
            清空
          </button>
        </div>
      </header>
      <textarea
        ref="sourceInput"
        v-model="sourceText"
        class="image-extractor-textarea"
        spellcheck="false"
        placeholder="在这里输入包含图片链接的字符串"
      ></textarea>
    </section>

    <section class="image-extractor-panel image-extractor-result-panel">
      <header class="image-extractor-panel-head">
        <div class="image-extractor-panel-title">
          <strong class="image-extractor-title">图片内容</strong>
          <span class="image-extractor-subtitle">
            已提取 {{ links.length }} 个，已选择 {{ selectedLinks.length }} 个
          </span>
        </div>
        <div class="image-extractor-export-actions">
          <button
            class="image-extractor-button"
            type="button"
            :disabled="!links.length || exporting"
            @click="toggleAll"
          >
            <component :is="allSelected ? Square : CheckSquare2" :size="15" />
            {{ allSelected ? "取消全选" : "全选" }}
          </button>
          <select
            v-model="exportFormat"
            class="image-extractor-format"
            aria-label="导出格式"
            :disabled="!links.length || exporting"
          >
            <option value="pdf">PDF</option>
            <option value="zip">ZIP</option>
          </select>
          <button
            class="image-extractor-button image-extractor-button-primary"
            type="button"
            :disabled="!selectedLinks.length || exporting"
            @click="exportSelectedImages"
          >
            <Download :size="15" />
            {{ exporting ? "导出中..." : "导出所选" }}
          </button>
        </div>
      </header>

      <div
        v-if="notice.message"
        :class="[
          'image-extractor-notice',
          notice.type === 'success'
            ? 'image-extractor-notice-success'
            : notice.type === 'error'
              ? 'image-extractor-notice-error'
              : ''
        ]"
        role="status"
      >
        {{ notice.message }}
      </div>

      <div class="image-extractor-result">
        <div v-if="!sourceText.trim()" class="image-extractor-empty">
          输入内容后自动提取图片链接
        </div>
        <div v-else-if="!links.length" class="image-extractor-empty">
          没有匹配到图片链接
        </div>

        <template v-else>
          <article
            v-for="(link, index) in links"
            :key="link"
            :class="[
              'image-extractor-card',
              { 'image-extractor-card-selected': isSelected(link) }
            ]"
          >
            <label class="image-extractor-select" title="选择图片">
              <input
                type="checkbox"
                :checked="isSelected(link)"
                @change="toggleSelection(link)"
              />
              <span class="image-extractor-select-label"
                >选择图片 {{ index + 1 }}</span
              >
            </label>

            <button
              class="image-extractor-preview-button"
              type="button"
              :disabled="isBroken(link)"
              :title="isBroken(link) ? '图片加载失败' : '预览图片'"
              @click="openPreview(link)"
            >
              <img
                v-if="!isBroken(link)"
                class="image-extractor-image"
                :src="link"
                :alt="`提取到的图片 ${index + 1}`"
                loading="lazy"
                @error="markImageBroken(link)"
              />
              <span v-else class="image-extractor-broken">
                <ImageOff :size="24" />
                加载失败
              </span>
            </button>

            <footer class="image-extractor-card-meta">
              <strong class="image-extractor-index">#{{ index + 1 }}</strong>
              <button
                class="image-extractor-link"
                type="button"
                :title="link"
                @click="openExternal(link)"
              >
                <span class="image-extractor-link-text">{{ link }}</span>
                <ExternalLink :size="13" />
              </button>
            </footer>
          </article>
        </template>
      </div>
    </section>

    <div v-if="previewUrl" class="image-extractor-modal">
      <button
        class="image-extractor-backdrop"
        type="button"
        aria-label="关闭预览"
        @click="closePreview"
      ></button>
      <section class="image-extractor-dialog" role="dialog" aria-modal="true">
        <header class="image-extractor-dialog-head">
          <div class="image-extractor-dialog-heading">
            <strong class="image-extractor-dialog-title">图片预览</strong>
            <span class="image-extractor-dialog-count">
              {{ previewIndex + 1 }} / {{ links.length }}
            </span>
          </div>
          <button
            class="image-extractor-icon-button"
            type="button"
            aria-label="关闭预览"
            title="关闭"
            @click="closePreview"
          >
            <X :size="17" />
          </button>
        </header>
        <div class="image-extractor-dialog-viewer">
          <button
            class="image-extractor-navigation image-extractor-navigation-previous"
            type="button"
            :disabled="!canPreviewPrevious"
            aria-label="上一张"
            title="上一张"
            @click="showPreviousPreview"
          >
            <ChevronLeft :size="20" />
          </button>

          <div
            ref="previewBody"
            :class="[
              'image-extractor-dialog-body',
              {
                'image-extractor-dialog-body-zoomed': previewScale > 1
              }
            ]"
            @pointercancel="stopPreviewDrag"
            @pointerdown="startPreviewDrag"
            @pointermove="movePreviewDrag"
            @pointerup="stopPreviewDrag"
            @wheel.prevent="handlePreviewWheel"
          >
            <div
              class="image-extractor-dialog-stage"
              :style="previewStageStyle"
            >
              <img
                class="image-extractor-dialog-image"
                :src="previewUrl"
                :alt="`图片预览 ${previewIndex + 1}`"
                :draggable="false"
                :style="previewImageStyle"
              />
            </div>
          </div>

          <button
            class="image-extractor-navigation image-extractor-navigation-next"
            type="button"
            :disabled="!canPreviewNext"
            aria-label="下一张"
            title="下一张"
            @click="showNextPreview"
          >
            <ChevronRight :size="20" />
          </button>

          <div
            class="image-extractor-zoom-toolbar"
            role="toolbar"
            aria-label="图片缩放"
          >
            <button
              class="image-extractor-zoom-button"
              type="button"
              :disabled="previewScale <= 0.25"
              aria-label="缩小"
              title="缩小"
              @click="changePreviewScale(-0.25)"
            >
              <ZoomOut :size="17" />
            </button>
            <span class="image-extractor-zoom-value">
              {{ Math.round(previewScale * 100) }}%
            </span>
            <button
              class="image-extractor-zoom-button"
              type="button"
              :disabled="previewScale >= 4"
              aria-label="放大"
              title="放大"
              @click="changePreviewScale(0.25)"
            >
              <ZoomIn :size="17" />
            </button>
            <span class="image-extractor-zoom-divider"></span>
            <button
              class="image-extractor-zoom-button"
              type="button"
              :disabled="previewScale === 1"
              aria-label="恢复适应"
              title="恢复适应"
              @click="resetPreviewScale"
            >
              <RotateCcw :size="16" />
            </button>
          </div>
        </div>
        <button
          class="image-extractor-dialog-link"
          type="button"
          :title="previewUrl"
          @click="openExternal(previewUrl)"
        >
          <span class="image-extractor-dialog-link-text">{{ previewUrl }}</span>
          <ExternalLink :size="14" />
        </button>
      </section>
    </div>
  </section>
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
  CheckSquare2,
  ChevronLeft,
  ChevronRight,
  Download,
  ExternalLink,
  FileText,
  ImageOff,
  RotateCcw,
  Square,
  Trash2,
  X,
  ZoomIn,
  ZoomOut
} from "lucide-vue-next"
import { systemApi, toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

const imageUrlPattern =
  /https?:\/\/[^\s"'<>()[\]{}]+?\.(?:png|jpe?g|gif|webp|bmp|svg|avif)(?:\?[^\s"'<>()[\]{}]*)?(?:#[^\s"'<>()[\]{}]*)?/giu

const sampleText = `文章封面：https://images.pexels.com/photos/1181671/pexels-photo-1181671.jpeg?auto=compress&cs=tinysrgb&w=900
Markdown 图片：![工作台](https://dummyimage.com/960x540/16685f/ffffff.png&text=Utility+Toolbox)
HTML 图片：<img src="https://www.gstatic.com/webp/gallery/1.webp">
普通链接：https://example.com/page 不会被识别为图片`

const sourceInput = ref(null)
const sourceText = ref("")
const exportFormat = ref("pdf")
const selectedLinks = ref([])
const brokenLinks = ref([])
const exporting = ref(false)
const previewUrl = ref("")
const previewBody = ref(null)
const previewScale = ref(1)
const notice = reactive({ message: "", type: "" })
let previewDrag = null

const links = computed(() => extractImageLinks(sourceText.value))
const allSelected = computed(
  () =>
    Boolean(links.value.length) &&
    selectedLinks.value.length === links.value.length
)
const previewIndex = computed(() => links.value.indexOf(previewUrl.value))
const canPreviewPrevious = computed(() => previewIndex.value > 0)
const canPreviewNext = computed(
  () => previewIndex.value >= 0 && previewIndex.value < links.value.length - 1
)
const previewStageStyle = computed(() => {
  const layoutScale = Math.max(previewScale.value, 1)
  return {
    width: `${layoutScale * 100}%`,
    height: `${layoutScale * 100}%`
  }
})
const previewImageStyle = computed(() => ({
  transform: previewScale.value < 1 ? `scale(${previewScale.value})` : "none"
}))

// 输入改变时仅保留仍然存在的选择和加载状态。
watch(links, (currentLinks) => {
  selectedLinks.value = selectedLinks.value.filter((link) =>
    currentLinks.includes(link)
  )
  brokenLinks.value = brokenLinks.value.filter((link) =>
    currentLinks.includes(link)
  )
  if (previewUrl.value && !currentLinks.includes(previewUrl.value)) {
    closePreview()
  }
  notice.message = ""
  notice.type = ""
})

function extractImageLinks(text) {
  const found = []
  const seen = new Set()

  for (const match of text.matchAll(imageUrlPattern)) {
    if (!seen.has(match[0])) {
      seen.add(match[0])
      found.push(match[0])
    }
  }
  return found
}

function isSelected(link) {
  return selectedLinks.value.includes(link)
}

function toggleSelection(link) {
  selectedLinks.value = isSelected(link)
    ? selectedLinks.value.filter((item) => item !== link)
    : [...selectedLinks.value, link]
  notice.message = ""
  notice.type = ""
}

function toggleAll() {
  selectedLinks.value = allSelected.value ? [] : [...links.value]
}

function isBroken(link) {
  return brokenLinks.value.includes(link)
}

function markImageBroken(link) {
  if (!isBroken(link)) {
    brokenLinks.value = [...brokenLinks.value, link]
  }
}

function loadSample() {
  sourceText.value = sampleText
}

function clearContent() {
  sourceText.value = ""
  selectedLinks.value = []
  closePreview()
  sourceInput.value?.focus()
}

function openPreview(link) {
  previewUrl.value = link
  resetPreviewScale()
}

function closePreview() {
  previewUrl.value = ""
  previewScale.value = 1
  previewDrag = null
}

function showPreviewAt(index) {
  if (index < 0 || index >= links.value.length) {
    return
  }

  previewUrl.value = links.value[index]
  resetPreviewScale()
}

function showPreviousPreview() {
  showPreviewAt(previewIndex.value - 1)
}

function showNextPreview() {
  showPreviewAt(previewIndex.value + 1)
}

// 缩放时保持鼠标所在位置稳定，连续查看长图时不需要反复寻找焦点。
function setPreviewScale(nextScale, anchorEvent = null) {
  const currentScale = previewScale.value
  const scale = Math.min(4, Math.max(0.25, nextScale))

  if (scale === currentScale) {
    return
  }

  const body = previewBody.value
  const bodyRect = body?.getBoundingClientRect()
  const anchorX =
    anchorEvent && bodyRect
      ? anchorEvent.clientX - bodyRect.left
      : (body?.clientWidth || 0) / 2
  const anchorY =
    anchorEvent && bodyRect
      ? anchorEvent.clientY - bodyRect.top
      : (body?.clientHeight || 0) / 2
  const contentX = (body?.scrollLeft || 0) + anchorX
  const contentY = (body?.scrollTop || 0) + anchorY
  const layoutRatio = Math.max(scale, 1) / Math.max(currentScale, 1)

  previewScale.value = scale
  nextTick(() => {
    if (!body) {
      return
    }
    if (scale <= 1) {
      body.scrollLeft = 0
      body.scrollTop = 0
      return
    }

    body.scrollLeft = contentX * layoutRatio - anchorX
    body.scrollTop = contentY * layoutRatio - anchorY
  })
}

function changePreviewScale(offset) {
  setPreviewScale(previewScale.value + offset)
}

function resetPreviewScale() {
  setPreviewScale(1)
  nextTick(() => {
    if (previewBody.value) {
      previewBody.value.scrollLeft = 0
      previewBody.value.scrollTop = 0
    }
  })
}

function handlePreviewWheel(event) {
  setPreviewScale(previewScale.value + (event.deltaY < 0 ? 0.25 : -0.25), event)
}

// 图片放大后允许直接拖动画布，避免滚轮被缩放占用后无法快速平移。
function startPreviewDrag(event) {
  if (previewScale.value <= 1 || event.button !== 0 || !previewBody.value) {
    return
  }

  previewDrag = {
    pointerId: event.pointerId,
    clientX: event.clientX,
    clientY: event.clientY,
    scrollLeft: previewBody.value.scrollLeft,
    scrollTop: previewBody.value.scrollTop
  }
  previewBody.value.setPointerCapture(event.pointerId)
}

function movePreviewDrag(event) {
  if (
    !previewDrag ||
    previewDrag.pointerId !== event.pointerId ||
    !previewBody.value
  ) {
    return
  }

  previewBody.value.scrollLeft =
    previewDrag.scrollLeft - (event.clientX - previewDrag.clientX)
  previewBody.value.scrollTop =
    previewDrag.scrollTop - (event.clientY - previewDrag.clientY)
}

function stopPreviewDrag(event) {
  if (!previewDrag || previewDrag.pointerId !== event.pointerId) {
    return
  }

  if (previewBody.value?.hasPointerCapture(event.pointerId)) {
    previewBody.value.releasePointerCapture(event.pointerId)
  }
  previewDrag = null
}

async function openExternal(url) {
  try {
    await systemApi.openExternal({ url })
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

async function exportSelectedImages() {
  if (!selectedLinks.value.length || exporting.value) {
    return
  }

  exporting.value = true
  notice.message = `正在生成 ${exportFormat.value.toUpperCase()} 文件...`
  notice.type = ""

  try {
    const timestamp = new Date()
      .toISOString()
      .replace(/[-:T]/g, "")
      .slice(0, 14)
    const targetPath = await systemApi.saveFile({
      title: "导出所选图片",
      defaultPath: `images-${timestamp}.${exportFormat.value}`,
      filters: [
        exportFormat.value === "pdf"
          ? { name: "PDF 文档", extensions: ["pdf"] }
          : { name: "ZIP 压缩包", extensions: ["zip"] }
      ]
    })

    if (!targetPath) {
      notice.message = ""
      return
    }

    // 下载、格式转换和文件生成全部交给桌面端，避免浏览器跨域与内存复制。
    await toolboxApi.exportImages({
      format: exportFormat.value,
      urls: links.value.filter((link) => selectedLinks.value.includes(link)),
      targetPath
    })
    notice.message = `已导出 ${selectedLinks.value.length} 张图片。`
    notice.type = "success"
    createMessage.success("所选图片已导出。")
  } catch (error) {
    notice.message = error.message || String(error)
    notice.type = "error"
    createMessage.error(notice.message)
  } finally {
    exporting.value = false
  }
}

function handleKeydown(event) {
  if (event.key === "Escape") {
    closePreview()
  } else if (previewUrl.value && event.key === "ArrowLeft") {
    event.preventDefault()
    showPreviousPreview()
  } else if (previewUrl.value && event.key === "ArrowRight") {
    event.preventDefault()
    showNextPreview()
  }
}

onMounted(() => window.addEventListener("keydown", handleKeydown))
onBeforeUnmount(() => window.removeEventListener("keydown", handleKeydown))
</script>

<style scoped lang="less">
.image-extractor {
  position: relative;
  display: grid;
  min-height: 0;
  flex: 1;
  grid-template-columns: minmax(350px, 0.84fr) minmax(0, 1.16fr);
  gap: 10px;

  .image-extractor-panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;

    .image-extractor-panel-head {
      display: flex;
      min-height: 50px;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 8px 10px;
      border-bottom: 1px solid var(--color-line);
      background: #f8fafc;

      .image-extractor-panel-title {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 2px;

        .image-extractor-title {
          color: var(--color-text);
          font-size: 0.8rem;
        }

        .image-extractor-subtitle {
          overflow: hidden;
          color: var(--color-text-muted);
          font-size: 0.68rem;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }

      .image-extractor-actions,
      .image-extractor-export-actions {
        display: flex;
        flex: none;
        align-items: center;
        gap: 6px;
      }
    }

    .image-extractor-button {
      display: inline-flex;
      height: 32px;
      align-items: center;
      justify-content: center;
      gap: 5px;
      padding: 0 9px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: #ffffff;
      color: var(--color-primary);
      cursor: pointer;
      font-size: 0.72rem;
      font-weight: 700;

      &:hover:not(:disabled) {
        border-color: #b9ccda;
        background: #f3f7fb;
      }

      &:disabled {
        cursor: not-allowed;
        opacity: 0.48;
      }
    }

    .image-extractor-button-primary {
      border-color: var(--color-primary);
      background: var(--color-primary);
      color: #ffffff;

      &:hover:not(:disabled) {
        border-color: #263f63;
        background: #263f63;
      }
    }

    .image-extractor-format {
      height: 32px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      outline: 0;
      padding: 0 24px 0 8px;
      background: #ffffff;
      color: var(--color-text);
      cursor: pointer;
      font-size: 0.72rem;
      font-weight: 700;

      &:focus-visible {
        outline: 2px solid rgba(47, 70, 104, 0.22);
        outline-offset: 2px;
      }

      &:disabled {
        cursor: not-allowed;
        opacity: 0.48;
      }
    }
  }

  .image-extractor-input-panel {
    .image-extractor-textarea {
      display: block;
      width: 100%;
      min-height: 0;
      flex: 1;
      resize: none;
      border: 0;
      outline: 0;
      padding: 12px;
      background: #ffffff;
      color: var(--color-text);
      font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
      font-size: 0.76rem;
      line-height: 1.58;

      &:focus {
        box-shadow: inset 0 0 0 2px rgba(47, 95, 145, 0.18);
      }
    }
  }

  .image-extractor-result-panel {
    .image-extractor-notice {
      flex: none;
      padding: 7px 10px;
      border-bottom: 1px solid #d7e2ec;
      background: #edf4fa;
      color: var(--color-primary);
      font-size: 0.7rem;
      font-weight: 700;
      line-height: 1.4;
    }

    .image-extractor-notice-success {
      border-color: #cde9d5;
      background: #eaf8ef;
      color: #177a3b;
    }

    .image-extractor-notice-error {
      border-color: #f1d0ca;
      background: #fdebea;
      color: #a32920;
    }

    .image-extractor-result {
      display: grid;
      min-height: 0;
      flex: 1;
      align-content: start;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 9px;
      overflow: auto;
      padding: 10px;

      .image-extractor-empty {
        display: grid;
        min-height: 210px;
        grid-column: 1 / -1;
        place-items: center;
        border: 1px dashed var(--color-line-strong);
        border-radius: 8px;
        background: #f8fafc;
        color: var(--color-text-muted);
        font-size: 0.78rem;
      }

      .image-extractor-card {
        position: relative;
        display: flex;
        min-width: 0;
        align-self: start;
        flex-direction: column;
        overflow: hidden;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: #ffffff;
        transition:
          border-color 0.18s ease,
          box-shadow 0.18s ease;

        &:hover {
          border-color: #b8c9d8;
          box-shadow: 0 8px 20px rgba(34, 56, 83, 0.08);
        }

        .image-extractor-select {
          position: absolute;
          z-index: 2;
          top: 8px;
          left: 8px;
          display: inline-flex;
          width: 28px;
          height: 28px;
          align-items: center;
          justify-content: center;
          border: 1px solid rgba(200, 214, 224, 0.92);
          border-radius: 7px;
          background: rgba(255, 255, 255, 0.94);
          box-shadow: 0 4px 10px rgba(20, 33, 58, 0.12);
          cursor: pointer;

          .image-extractor-select-label {
            position: absolute;
            width: 1px;
            height: 1px;
            overflow: hidden;
            clip: rect(0 0 0 0);
            white-space: nowrap;
          }
        }

        .image-extractor-preview-button {
          display: grid;
          padding: 0;
          overflow: hidden;
          border: 0;
          border-bottom: 1px solid var(--color-line);
          background:
            linear-gradient(45deg, #eef3f7 25%, transparent 25%),
            linear-gradient(-45deg, #eef3f7 25%, transparent 25%),
            linear-gradient(45deg, transparent 75%, #eef3f7 75%),
            linear-gradient(-45deg, transparent 75%, #eef3f7 75%), #ffffff;
          background-position:
            0 0,
            0 9px,
            9px -9px,
            -9px 0;
          background-size: 18px 18px;
          cursor: zoom-in;
          place-items: center;

          &:disabled {
            cursor: default;
          }

          .image-extractor-image {
            display: block;
            width: 100%;
            height: auto;
          }

          .image-extractor-broken {
            display: flex;
            width: 100%;
            min-height: 154px;
            align-items: center;
            justify-content: center;
            flex-direction: column;
            gap: 7px;
            color: var(--color-text-soft);
            font-size: 0.72rem;
            font-weight: 700;
          }
        }

        .image-extractor-card-meta {
          display: flex;
          min-width: 0;
          align-items: center;
          gap: 8px;
          padding: 8px;

          .image-extractor-index {
            flex: none;
            color: var(--color-primary);
            font-size: 0.7rem;
          }

          .image-extractor-link {
            display: flex;
            min-width: 0;
            flex: 1;
            align-items: center;
            gap: 5px;
            padding: 0;
            border: 0;
            background: transparent;
            color: var(--color-text-muted);
            cursor: pointer;
            font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
            text-align: left;

            &:hover {
              color: var(--color-primary);
            }

            .image-extractor-link-text {
              min-width: 0;
              flex: 1;
              overflow: hidden;
              font-size: 0.68rem;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
          }
        }
      }

      .image-extractor-card-selected {
        border-color: var(--color-primary);
        box-shadow: 0 0 0 2px rgba(47, 95, 145, 0.12);

        &:hover {
          border-color: var(--color-primary);
          box-shadow: 0 0 0 2px rgba(47, 95, 145, 0.12);
        }
      }
    }
  }

  .image-extractor-modal {
    position: fixed;
    z-index: 30;
    inset: 0;
    display: grid;
    padding: 28px;
    place-items: center;

    .image-extractor-backdrop {
      position: absolute;
      inset: 0;
      border: 0;
      background: rgba(15, 23, 42, 0.42);
      cursor: zoom-out;
    }

    .image-extractor-dialog {
      position: relative;
      display: flex;
      width: min(1040px, calc(100vw - 72px));
      height: min(780px, calc(100vh - 72px));
      min-height: 0;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid rgba(255, 255, 255, 0.45);
      border-radius: 8px;
      background: #ffffff;
      box-shadow: 0 24px 68px rgba(15, 23, 42, 0.24);

      .image-extractor-dialog-head {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 9px 10px;
        border-bottom: 1px solid var(--color-line);
        background: #f8fafc;

        .image-extractor-dialog-heading {
          display: flex;
          min-width: 0;
          align-items: center;
          gap: 8px;

          .image-extractor-dialog-title {
            color: var(--color-text);
            font-size: 0.8rem;
          }

          .image-extractor-dialog-count {
            min-width: 44px;
            color: var(--color-text-soft);
            font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
            font-size: 0.68rem;
          }
        }

        .image-extractor-icon-button {
          display: grid;
          width: 30px;
          height: 30px;
          padding: 0;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          background: #ffffff;
          color: var(--color-primary);
          cursor: pointer;
          place-items: center;
        }
      }

      .image-extractor-dialog-viewer {
        position: relative;
        display: flex;
        min-height: 0;
        flex: 1;
        overflow: hidden;
        background: #f3f6fa;

        .image-extractor-dialog-body {
          width: 100%;
          height: 100%;
          min-height: 0;
          overflow: auto;
          padding: 12px 58px 60px;
          cursor: default;
          overscroll-behavior: contain;
          user-select: none;

          .image-extractor-dialog-stage {
            display: flex;
            min-width: 0;
            min-height: 0;
            align-items: center;
            justify-content: center;

            .image-extractor-dialog-image {
              width: 100%;
              height: 100%;
              object-fit: contain;
              transform-origin: center;
            }
          }
        }

        .image-extractor-dialog-body-zoomed {
          cursor: grab;

          &:active {
            cursor: grabbing;
          }
        }

        .image-extractor-navigation {
          position: absolute;
          z-index: 2;
          top: 50%;
          display: grid;
          width: 36px;
          height: 42px;
          padding: 0;
          border: 1px solid #cdd9e4;
          border-radius: 7px;
          background: rgba(255, 255, 255, 0.94);
          box-shadow: 0 7px 18px rgba(34, 56, 83, 0.14);
          color: var(--color-primary);
          cursor: pointer;
          place-items: center;
          transform: translateY(-50%);

          &:hover:not(:disabled) {
            border-color: var(--color-primary);
            background: #ffffff;
          }

          &:disabled {
            cursor: not-allowed;
            opacity: 0.34;
          }
        }

        .image-extractor-navigation-previous {
          left: 12px;
        }

        .image-extractor-navigation-next {
          right: 12px;
        }

        .image-extractor-zoom-toolbar {
          position: absolute;
          z-index: 3;
          bottom: 12px;
          left: 50%;
          display: flex;
          height: 40px;
          align-items: center;
          gap: 2px;
          padding: 4px;
          border: 1px solid rgba(255, 255, 255, 0.18);
          border-radius: 7px;
          background: #2d3e55;
          box-shadow: 0 10px 24px rgba(15, 23, 42, 0.24);
          color: #ffffff;
          transform: translateX(-50%);

          .image-extractor-zoom-button {
            display: grid;
            width: 31px;
            height: 31px;
            padding: 0;
            border: 0;
            border-radius: 5px;
            background: transparent;
            color: inherit;
            cursor: pointer;
            place-items: center;

            &:hover:not(:disabled) {
              background: rgba(255, 255, 255, 0.14);
            }

            &:disabled {
              cursor: not-allowed;
              opacity: 0.34;
            }
          }

          .image-extractor-zoom-value {
            width: 52px;
            color: #ffffff;
            font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
            font-size: 0.72rem;
            font-weight: 800;
            text-align: center;
          }

          .image-extractor-zoom-divider {
            width: 1px;
            height: 20px;
            margin: 0 2px;
            background: rgba(255, 255, 255, 0.22);
          }
        }
      }

      .image-extractor-dialog-link {
        display: flex;
        min-width: 0;
        flex: none;
        align-items: center;
        gap: 7px;
        padding: 9px 11px;
        border: 0;
        border-top: 1px solid var(--color-line);
        background: #ffffff;
        color: var(--color-primary);
        cursor: pointer;
        font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
        text-align: left;

        .image-extractor-dialog-link-text {
          min-width: 0;
          flex: 1;
          overflow: hidden;
          font-size: 0.7rem;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }
    }
  }
}
</style>

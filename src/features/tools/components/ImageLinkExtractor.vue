<template>
  <section class="image-extractor">
    <!-- 左侧卡片：字符串内容 -->
    <section class="image-extractor-card-panel image-extractor-input-card">
      <header class="extractor-card-head">
        <div class="card-head-left">
          <div class="card-icon-box">
            <FileText :size="18" />
          </div>
          <div class="card-title-group">
            <span class="card-title">字符串内容</span>
            <span class="card-subtitle">文本 / Markdown / HTML / JSON</span>
          </div>
        </div>

        <div class="card-actions">
          <button
            class="card-action-btn"
            type="button"
            title="载入示例数据"
            @click="loadSample"
          >
            <FileText :size="14" />
            <span>示例</span>
          </button>
          <button
            class="card-action-btn"
            type="button"
            title="清空输入内容"
            @click="clearContent"
          >
            <Trash2 :size="14" />
            <span>清空</span>
          </button>
        </div>
      </header>

      <!-- 代码编辑器：集成行号槽与等宽字体 -->
      <div class="editor-container">
        <div
          ref="gutterRef"
          class="editor-gutter"
          aria-hidden="true"
          :style="{ width: `${gutterWidth}px` }"
        >
          <div
            v-for="line in lineNumbers"
            :key="line"
            class="editor-gutter-line"
          >
            {{ line }}
          </div>
        </div>

        <textarea
          ref="sourceInput"
          v-model="sourceText"
          class="editor-textarea"
          wrap="off"
          spellcheck="false"
          placeholder="在这里输入包含图片链接的文本、Markdown、HTML 或 JSON 字符串..."
          @scroll="syncScroll"
        ></textarea>
      </div>
    </section>

    <!-- 右侧卡片：图片内容 -->
    <section class="image-extractor-card-panel image-extractor-result-card">
      <header class="extractor-card-head">
        <div class="card-head-left">
          <div class="card-icon-box">
            <ImageIcon :size="18" />
          </div>
          <div class="card-title-group">
            <span class="card-title">图片内容</span>
            <span class="card-subtitle">
              已提取 <strong class="stat-count">{{ links.length }}</strong> 个，已选择
              <strong class="stat-count stat-count--blue">{{ selectedLinks.length }}</strong> 个
            </span>
          </div>
        </div>

        <div class="card-actions card-export-actions">
          <button
            class="card-action-btn select-all-btn"
            type="button"
            :disabled="!links.length || exporting"
            @click="toggleAll"
          >
            <CheckSquare2
              v-if="allSelected"
              :size="15"
              class="action-icon action-icon--active"
            />
            <Square v-else :size="15" class="action-icon" />
            <span>{{ allSelected ? "取消全选" : "全选" }}</span>
          </button>

          <div class="format-select-wrapper">
            <select
              v-model="exportFormat"
              class="format-select"
              aria-label="导出格式"
              :disabled="!links.length || exporting"
            >
              <option value="pdf">PDF</option>
              <option value="zip">ZIP</option>
            </select>
            <ChevronDown :size="13" class="format-select-arrow" />
          </div>

          <button
            class="card-action-btn export-btn-primary"
            type="button"
            :disabled="!selectedLinks.length || exporting"
            @click="exportSelectedImages"
          >
            <LoaderCircle v-if="exporting" :size="14" class="spinning" />
            <Download v-else :size="14" />
            <span>{{ exporting ? "导出中..." : "导出所选" }}</span>
          </button>
        </div>
      </header>

      <!-- 操作提示横幅 -->
      <div
        v-if="notice.message"
        :class="[
          'extractor-notice-banner',
          notice.type === 'success'
            ? 'is-success'
            : notice.type === 'error'
              ? 'is-error'
              : 'is-info'
        ]"
        role="status"
      >
        <span class="notice-text">{{ notice.message }}</span>
      </div>

      <!-- 图片网格展示区 -->
      <div class="extractor-result-body">
        <div v-if="!sourceText.trim()" class="extractor-empty-state">
          <div class="empty-icon-box">
            <ImageIcon :size="28" />
          </div>
          <span class="empty-title">输入内容后自动提取图片链接</span>
          <span class="empty-desc">
            支持从文本、Markdown、HTML 或 JSON 字符串中自动识别图片 URL
          </span>
        </div>

        <div v-else-if="!links.length" class="extractor-empty-state">
          <div class="empty-icon-box">
            <Search :size="28" />
          </div>
          <span class="empty-title">未检测到图片链接</span>
          <span class="empty-desc">
            未发现以 .png, .jpg, .jpeg, .webp, .gif, .svg, .bmp, .avif 结尾的合法图片地址
          </span>
        </div>

        <!-- 2列图片卡片网格 -->
        <div v-else class="image-cards-grid">
          <article
            v-for="(link, index) in links"
            :key="link"
            :class="[
              'image-grid-card',
              { 'is-selected': isSelected(link) }
            ]"
          >
            <!-- 左上角勾选按钮 -->
            <button
              class="card-select-checkbox"
              :class="{ 'is-checked': isSelected(link) }"
              type="button"
              :title="isSelected(link) ? '取消选择' : '选择图片'"
              @click.stop="toggleSelection(link)"
            >
              <Check v-if="isSelected(link)" :size="12" :stroke-width="2.6" />
            </button>

            <!-- 居中图片预览缩略图 -->
            <div
              class="card-thumb-stage"
              role="button"
              tabindex="0"
              :title="isBroken(link) ? '图片加载失败' : '点击查看大图预览'"
              @click="openPreview(link)"
              @keydown.enter="openPreview(link)"
            >
              <img
                v-if="!isBroken(link)"
                class="card-thumb-image"
                :src="link"
                :alt="`提取图片 #${index + 1}`"
                loading="lazy"
                @error="markImageBroken(link)"
              />
              <div v-else class="card-broken-stage">
                <ImageOff :size="22" />
                <span>图片加载失败</span>
              </div>
            </div>

            <!-- 卡片底栏信息：#序号、截断URL、外部打开链接按钮 -->
            <footer class="card-footer-meta">
              <span class="card-index-label">#{{ index + 1 }}</span>
              <span class="card-url-text" :title="link">{{ link }}</span>
              <button
                class="card-ext-link-btn"
                type="button"
                :title="`在外部浏览器打开: ${link}`"
                @click.stop="openExternal(link)"
              >
                <ExternalLink :size="13" />
              </button>
            </footer>
          </article>
        </div>
      </div>
    </section>

    <!-- 高保真大图预览弹窗 -->
    <div v-if="previewUrl" class="image-preview-modal">
      <button
        class="preview-backdrop"
        type="button"
        aria-label="关闭预览"
        @click="closePreview"
      ></button>

      <section class="preview-dialog" role="dialog" aria-modal="true">
        <header class="preview-dialog-head">
          <div class="preview-heading">
            <div class="preview-icon-box">
              <ImageIcon :size="16" />
            </div>
            <span class="preview-title">图片预览</span>
            <span class="preview-counter">
              {{ previewIndex + 1 }} / {{ links.length }}
            </span>
          </div>

          <button
            class="preview-close-btn"
            type="button"
            aria-label="关闭预览"
            title="关闭 (Esc)"
            @click="closePreview"
          >
            <X :size="16" />
          </button>
        </header>

        <div class="preview-dialog-viewer">
          <!-- 上一张按钮 -->
          <button
            class="preview-nav-btn preview-nav-prev"
            type="button"
            :disabled="!canPreviewPrevious"
            aria-label="上一张 (←)"
            title="上一张 (←)"
            @click="showPreviousPreview"
          >
            <ChevronLeft :size="20" />
          </button>

          <!-- 缩放与平移舞台 -->
          <div
            ref="previewBody"
            :class="[
              'preview-stage-container',
              { 'is-zoomed': previewScale > 1 }
            ]"
            @pointercancel="stopPreviewDrag"
            @pointerdown="startPreviewDrag"
            @pointermove="movePreviewDrag"
            @pointerup="stopPreviewDrag"
            @wheel.prevent="handlePreviewWheel"
          >
            <div
              class="preview-zoom-stage"
              :style="previewStageStyle"
            >
              <img
                class="preview-zoom-image"
                :src="previewUrl"
                :alt="`图片预览 ${previewIndex + 1}`"
                :draggable="false"
                :style="previewImageStyle"
              />
            </div>
          </div>

          <!-- 下一张按钮 -->
          <button
            class="preview-nav-btn preview-nav-next"
            type="button"
            :disabled="!canPreviewNext"
            aria-label="下一张 (→)"
            title="下一张 (→)"
            @click="showNextPreview"
          >
            <ChevronRight :size="20" />
          </button>

          <!-- 悬浮缩放控制栏 -->
          <div
            class="preview-zoom-bar"
            role="toolbar"
            aria-label="图片缩放"
          >
            <button
              class="zoom-action-btn"
              type="button"
              :disabled="previewScale <= 0.25"
              aria-label="缩小"
              title="缩小"
              @click="changePreviewScale(-0.25)"
            >
              <ZoomOut :size="15" />
            </button>
            <span class="zoom-value-label">
              {{ Math.round(previewScale * 100) }}%
            </span>
            <button
              class="zoom-action-btn"
              type="button"
              :disabled="previewScale >= 4"
              aria-label="放大"
              title="放大"
              @click="changePreviewScale(0.25)"
            >
              <ZoomIn :size="15" />
            </button>
            <span class="zoom-divider"></span>
            <button
              class="zoom-action-btn"
              type="button"
              :disabled="previewScale === 1"
              aria-label="重置缩放"
              title="重置缩放"
              @click="resetPreviewScale"
            >
              <RotateCcw :size="14" />
            </button>
          </div>
        </div>

        <!-- 弹窗底栏链接 -->
        <footer class="preview-dialog-footer">
          <button
            class="preview-external-link"
            type="button"
            :title="`在外部浏览器打开: ${previewUrl}`"
            @click="openExternal(previewUrl)"
          >
            <span class="preview-url-text">{{ previewUrl }}</span>
            <ExternalLink :size="13" />
          </button>
        </footer>
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
  Check,
  CheckSquare2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Download,
  ExternalLink,
  FileText,
  Image as ImageIcon,
  ImageOff,
  LoaderCircle,
  RotateCcw,
  Search,
  Square,
  Trash2,
  X,
  ZoomIn,
  ZoomOut
} from "lucide-vue-next"
import { systemApi, toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

// 常见图片扩展名正则匹配
const imageUrlPattern =
  /https?:\/\/[^\s"'<>()[\]{}]+?\.(?:png|jpe?g|gif|webp|bmp|svg|avif)(?:\?[^\s"'<>()[\]{}]*)?(?:#[^\s"'<>()[\]{}]*)?/giu

const sampleText = `# 图片链接提取示例

文章封面：https://images.pexels.com/photos/1181671/pexels-photo-1181671.jpeg?auto=compress&cs=tinysrgb&w=900
Markdown 图片：![工作台](https://dummyimage.com/960x540/16685f/ffffff.png&text=Utility+Toolbox)
HTML 图片：<img src="https://www.gstatic.com/webp/gallery/1.webp" alt="WebP 示例" />

普通超链接（非图片，将被忽略）：
https://example.com/blog/article-1024

JSON 数据结构示例：
{
  "project": "AI Manager",
  "banner": "https://picsum.photos/id/1018/1000/600.jpg",
  "thumbnail": "https://picsum.photos/id/1025/400/400.jpg",
  "assets": [
    "https://picsum.photos/id/1035/800/600.jpg",
    "https://picsum.photos/id/1036/800/600.jpg",
    "https://picsum.photos/id/1037/800/600.jpg",
    "https://picsum.photos/id/1038/800/600.jpg"
  ]
}`

const sourceInput = ref(null)
const gutterRef = ref(null)
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

// 根据换行符计算总行数与行号数组
const lineNumbers = computed(() => {
  const count = (sourceText.value.match(/\n/g)?.length ?? 0) + 1
  return Array.from({ length: Math.max(1, count) }, (_, i) => i + 1)
})

// 根据最大行号位数自适应行号槽宽度
const gutterWidth = computed(() => {
  const digits = String(lineNumbers.value.length).length
  return Math.max(46, digits * 9 + 22)
})

// 提取唯一图片链接
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

// 输入变动时同步清理失效的已选与失效状态
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

function syncScroll(event) {
  if (gutterRef.value) {
    gutterRef.value.scrollTop = event.target.scrollTop
  }
}

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
    if (!body) return
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
  grid-template-columns: minmax(360px, 0.9fr) minmax(420px, 1.1fr);
  gap: 14px;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--font-size-base);

  /* 双列主卡片通用样式 */
  .image-extractor-card-panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 14px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);

    .extractor-card-head {
      display: flex;
      min-height: 54px;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 10px 16px;
      border-bottom: 1px solid var(--color-line);
      background: var(--color-panel);

      .card-head-left {
        display: flex;
        align-items: center;
        gap: 11px;
        min-width: 0;

        .card-icon-box {
          display: grid;
          width: 36px;
          height: 36px;
          flex-shrink: 0;
          place-items: center;
          border-radius: 9px;
          background: #eff6ff;
          color: #2563eb;
          border: 1px solid #dbeafe;
        }

        .card-title-group {
          display: flex;
          flex-direction: column;
          gap: 2px;
          min-width: 0;

          .card-title {
            color: var(--color-text);
            font-size: 15px;
            font-weight: 700;
            line-height: 1.25;
          }

          .card-subtitle {
            overflow: hidden;
            color: var(--color-text-muted);
            font-size: 12px;
            text-overflow: ellipsis;
            white-space: nowrap;
            line-height: 1.35;

            .stat-count {
              color: var(--color-text);
              font-weight: 700;

              &--blue {
                color: #2563eb;
              }
            }
          }
        }
      }

      .card-actions {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;

        .card-action-btn {
          display: inline-flex;
          height: 32px;
          align-items: center;
          justify-content: center;
          gap: 6px;
          padding: 0 12px;
          border: 1px solid #bfdbfe;
          border-radius: 8px;
          background: #ffffff;
          color: #2563eb;
          cursor: pointer;
          font-size: 13px;
          font-weight: 500;
          transition: all 0.18s ease;

          &:hover:not(:disabled) {
            border-color: #93c5fd;
            background: #eff6ff;
            color: #1d4ed8;
          }

          &:disabled {
            cursor: not-allowed;
            opacity: 0.48;
          }
        }
      }

      .card-export-actions {
        .select-all-btn {
          border-color: var(--color-line);
          background: var(--color-panel-soft);
          color: var(--color-text);

          &:hover:not(:disabled) {
            border-color: var(--color-line-strong);
            background: var(--color-panel);
          }

          .action-icon {
            color: var(--color-text-muted);

            &--active {
              color: #2563eb;
            }
          }
        }

        .format-select-wrapper {
          position: relative;
          display: inline-flex;
          align-items: center;

          .format-select {
            height: 32px;
            padding: 0 26px 0 10px;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            outline: 0;
            background: var(--color-panel);
            color: var(--color-text);
            cursor: pointer;
            font-size: 13px;
            font-weight: 500;
            appearance: none;
            transition: all 0.18s ease;

            &:hover:not(:disabled) {
              border-color: var(--color-line-strong);
            }

            &:focus {
              border-color: #2563eb;
              box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.12);
            }

            &:disabled {
              cursor: not-allowed;
              opacity: 0.48;
            }
          }

          .format-select-arrow {
            position: absolute;
            right: 8px;
            pointer-events: none;
            color: var(--color-text-soft);
          }
        }

        .export-btn-primary {
          border: 0;
          background: #2563eb;
          color: #ffffff;
          box-shadow: 0 1px 3px rgba(37, 99, 235, 0.25);
          font-weight: 600;

          &:hover:not(:disabled) {
            background: #1d4ed8;
            border-color: transparent;
            color: #ffffff;
            box-shadow: 0 3px 8px rgba(37, 99, 235, 0.35);
          }

          &:disabled {
            background: #94a3b8;
            box-shadow: none;
            color: #ffffff;
          }
        }
      }
    }
  }

  /* 左侧代码编辑器容器 */
  .image-extractor-input-card {
    .editor-container {
      position: relative;
      display: flex;
      min-height: 0;
      flex: 1;
      overflow: hidden;
      background: var(--color-panel);

      .editor-gutter {
        flex: none;
        overflow: hidden;
        padding: 14px 0;
        border-right: 1px solid var(--color-line);
        background: var(--color-panel-soft);
        color: var(--color-text-soft);
        user-select: none;
        pointer-events: none;

        .editor-gutter-line {
          height: 22px;
          padding-right: 12px;
          font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 13px;
          line-height: 22px;
          text-align: right;
        }
      }

      .editor-textarea {
        display: block;
        flex: 1;
        min-width: 0;
        height: 100%;
        padding: 14px 16px;
        border: 0;
        outline: none;
        resize: none;
        background: transparent;
        color: var(--color-text);
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        font-size: 13px;
        line-height: 22px;
        tab-size: 2;
        white-space: pre;
        overflow: auto;

        &::placeholder {
          color: var(--color-text-soft);
        }

        &:focus {
          box-shadow: inset 0 0 0 2px rgba(37, 99, 235, 0.08);
        }
      }
    }
  }

  /* 右侧图片展示卡片 */
  .image-extractor-result-card {
    .extractor-notice-banner {
      display: flex;
      align-items: center;
      padding: 8px 16px;
      font-size: 12.5px;
      line-height: 1.4;
      border-bottom: 1px solid var(--color-line);

      &.is-info {
        background: #eff6ff;
        color: #2563eb;
        border-bottom-color: #bfdbfe;
      }

      &.is-success {
        background: #ecfdf5;
        color: #059669;
        border-bottom-color: #a7f3d0;
      }

      &.is-error {
        background: #fef2f2;
        color: #dc2626;
        border-bottom-color: #fecaca;
      }
    }

    .extractor-result-body {
      display: flex;
      flex-direction: column;
      min-height: 0;
      flex: 1;
      overflow: hidden;

      .extractor-empty-state {
        display: flex;
        min-height: 240px;
        flex: 1;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 10px;
        padding: 28px;
        color: var(--color-text-muted);

        .empty-icon-box {
          display: grid;
          width: 54px;
          height: 54px;
          place-items: center;
          border-radius: 50%;
          background: var(--color-panel-soft);
          border: 1px solid var(--color-line);
          color: var(--color-text-soft);
        }

        .empty-title {
          font-size: 14px;
          font-weight: 600;
          color: var(--color-text);
        }

        .empty-desc {
          max-width: 360px;
          font-size: 12.5px;
          color: var(--color-text-muted);
          text-align: center;
          line-height: 1.5;
        }
      }

      .image-cards-grid {
        display: grid;
        min-height: 0;
        flex: 1;
        align-content: start;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        grid-auto-rows: max-content;
        gap: 12px;
        overflow-y: auto;
        padding: 14px;

        .image-grid-card {
          position: relative;
          display: flex;
          min-width: 0;
          flex-direction: column;
          overflow: hidden;
          border: 1px solid var(--color-line);
          border-radius: 10px;
          background: var(--color-panel);
          transition:
            border-color 0.18s ease,
            box-shadow 0.18s ease,
            transform 0.18s ease;

          &:hover {
            border-color: #93c5fd;
            box-shadow: 0 6px 16px rgba(37, 99, 235, 0.08);
            transform: translateY(-1px);
          }

          &.is-selected {
            border-color: #2563eb;
            box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.16);
          }

          /* 左上角勾选按钮 */
          .card-select-checkbox {
            position: absolute;
            top: 8px;
            left: 8px;
            z-index: 2;
            display: grid;
            width: 22px;
            height: 22px;
            place-items: center;
            border-radius: 6px;
            border: 1.5px solid #cbd5e1;
            background: rgba(255, 255, 255, 0.9);
            backdrop-filter: blur(4px);
            box-shadow: 0 2px 6px rgba(15, 23, 42, 0.12);
            color: #ffffff;
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover:not(.is-checked) {
              border-color: #2563eb;
              background: #ffffff;
            }

            &.is-checked {
              background: #2563eb;
              border-color: #2563eb;
              color: #ffffff;
            }
          }

          /* 居中缩略图展示区 */
          .card-thumb-stage {
            display: flex;
            align-items: center;
            justify-content: center;
            height: 142px;
            overflow: hidden;
            background:
              linear-gradient(45deg, #f1f5f9 25%, transparent 25%),
              linear-gradient(-45deg, #f1f5f9 25%, transparent 25%),
              linear-gradient(45deg, transparent 75%, #f1f5f9 75%),
              linear-gradient(-45deg, transparent 75%, #f1f5f9 75%),
              #ffffff;
            background-position: 0 0, 0 8px, 8px -8px, -8px 0;
            background-size: 16px 16px;
            cursor: zoom-in;

            .card-thumb-image {
              display: block;
              max-width: 100%;
              max-height: 100%;
              object-fit: contain;
              transition: transform 0.2s ease;
            }

            &:hover .card-thumb-image {
              transform: scale(1.03);
            }

            .card-broken-stage {
              display: flex;
              width: 100%;
              height: 100%;
              flex-direction: column;
              align-items: center;
              justify-content: center;
              gap: 6px;
              color: var(--color-text-soft);
              font-size: 12px;
            }
          }

          /* 底栏元数据 */
          .card-footer-meta {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 8px 10px;
            border-top: 1px solid var(--color-line);
            background: var(--color-panel);

            .card-index-label {
              flex: none;
              color: #2563eb;
              font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
              font-size: 13px;
              font-weight: 700;
            }

            .card-url-text {
              flex: 1;
              min-width: 0;
              overflow: hidden;
              color: var(--color-text-muted);
              font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
              font-size: 12px;
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .card-ext-link-btn {
              display: grid;
              width: 24px;
              height: 24px;
              flex: none;
              place-items: center;
              border: 0;
              border-radius: 5px;
              background: transparent;
              color: var(--color-text-soft);
              cursor: pointer;
              transition: all 0.15s ease;

              &:hover {
                color: #2563eb;
                background: #eff6ff;
              }
            }
          }
        }
      }
    }
  }

  /* 大图全屏预览弹窗 */
  .image-preview-modal {
    position: fixed;
    z-index: 99;
    inset: 0;
    display: grid;
    padding: 24px;
    place-items: center;

    .preview-backdrop {
      position: absolute;
      inset: 0;
      border: 0;
      background: rgba(15, 23, 42, 0.65);
      backdrop-filter: blur(5px);
      cursor: zoom-out;
    }

    .preview-dialog {
      position: relative;
      display: flex;
      width: min(1040px, calc(100vw - 64px));
      height: min(780px, calc(100vh - 64px));
      min-height: 0;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid var(--color-line);
      border-radius: 16px;
      background: var(--color-panel);
      box-shadow: 0 24px 64px rgba(15, 23, 42, 0.28);

      .preview-dialog-head {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 12px 18px;
        border-bottom: 1px solid var(--color-line);
        background: var(--color-panel);

        .preview-heading {
          display: flex;
          align-items: center;
          gap: 9px;
          min-width: 0;

          .preview-icon-box {
            display: grid;
            width: 30px;
            height: 30px;
            place-items: center;
            border-radius: 7px;
            background: #eff6ff;
            color: #2563eb;
          }

          .preview-title {
            color: var(--color-text);
            font-size: 15px;
            font-weight: 700;
          }

          .preview-counter {
            padding: 2px 8px;
            border-radius: 9999px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            color: var(--color-text-muted);
            font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 12px;
          }
        }

        .preview-close-btn {
          display: grid;
          width: 32px;
          height: 32px;
          place-items: center;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          cursor: pointer;
          transition: all 0.15s ease;

          &:hover {
            border-color: var(--color-line-strong);
            background: var(--color-panel-soft);
            color: var(--color-text);
          }
        }
      }

      .preview-dialog-viewer {
        position: relative;
        display: flex;
        min-height: 0;
        flex: 1;
        overflow: hidden;
        background: #0f172a;

        .preview-stage-container {
          width: 100%;
          height: 100%;
          min-height: 0;
          overflow: auto;
          padding: 16px 58px 64px;
          cursor: default;
          user-select: none;
          overscroll-behavior: contain;

          .preview-zoom-stage {
            display: flex;
            min-width: 0;
            min-height: 0;
            align-items: center;
            justify-content: center;

            .preview-zoom-image {
              width: 100%;
              height: 100%;
              object-fit: contain;
              transform-origin: center;
            }
          }

          &.is-zoomed {
            cursor: grab;

            &:active {
              cursor: grabbing;
            }
          }
        }

        .preview-nav-btn {
          position: absolute;
          top: 50%;
          z-index: 2;
          display: grid;
          width: 38px;
          height: 44px;
          place-items: center;
          border: 1px solid rgba(255, 255, 255, 0.15);
          border-radius: 8px;
          background: rgba(15, 23, 42, 0.72);
          backdrop-filter: blur(4px);
          box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
          color: #ffffff;
          cursor: pointer;
          transform: translateY(-50%);
          transition: all 0.18s ease;

          &:hover:not(:disabled) {
            background: #2563eb;
            border-color: #2563eb;
          }

          &:disabled {
            cursor: not-allowed;
            opacity: 0.28;
          }

          &.preview-nav-prev {
            left: 14px;
          }

          &.preview-nav-next {
            right: 14px;
          }
        }

        .preview-zoom-bar {
          position: absolute;
          bottom: 16px;
          left: 50%;
          z-index: 3;
          display: flex;
          height: 38px;
          align-items: center;
          gap: 3px;
          padding: 4px 8px;
          border: 1px solid rgba(255, 255, 255, 0.15);
          border-radius: 8px;
          background: rgba(15, 23, 42, 0.85);
          backdrop-filter: blur(8px);
          box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
          color: #ffffff;
          transform: translateX(-50%);

          .zoom-action-btn {
            display: grid;
            width: 30px;
            height: 30px;
            place-items: center;
            border: 0;
            border-radius: 6px;
            background: transparent;
            color: inherit;
            cursor: pointer;
            transition: background-color 0.15s ease;

            &:hover:not(:disabled) {
              background: rgba(255, 255, 255, 0.15);
            }

            &:disabled {
              cursor: not-allowed;
              opacity: 0.35;
            }
          }

          .zoom-value-label {
            width: 52px;
            color: #ffffff;
            font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 12.5px;
            text-align: center;
          }

          .zoom-divider {
            width: 1px;
            height: 18px;
            margin: 0 3px;
            background: rgba(255, 255, 255, 0.2);
          }
        }
      }

      .preview-dialog-footer {
        display: flex;
        align-items: center;
        padding: 10px 16px;
        border-top: 1px solid var(--color-line);
        background: var(--color-panel);

        .preview-external-link {
          display: flex;
          flex: 1;
          min-width: 0;
          align-items: center;
          gap: 7px;
          padding: 0;
          border: 0;
          background: transparent;
          color: #2563eb;
          cursor: pointer;
          font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          text-align: left;

          &:hover {
            text-decoration: underline;
          }

          .preview-url-text {
            flex: 1;
            min-width: 0;
            overflow: hidden;
            font-size: 12.5px;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
        }
      }
    }
  }

  /* 暗色模式适配 */
  :global(:root[data-theme="dark"]),
  .tools-view--dark & {
    .image-extractor-card-panel {
      background: var(--color-panel);
      border-color: var(--color-line);

      .extractor-card-head {
        background: var(--color-panel);
        border-bottom-color: var(--color-line);

        .card-head-left .card-icon-box {
          background: rgba(37, 99, 235, 0.16);
          border-color: rgba(96, 165, 250, 0.3);
          color: #60a5fa;
        }

        .card-head-left .card-title-group .card-subtitle .stat-count--blue {
          color: #60a5fa;
        }

        .card-actions .card-action-btn {
          border-color: rgba(96, 165, 250, 0.35);
          background: rgba(37, 99, 235, 0.14);
          color: #60a5fa;

          &:hover:not(:disabled) {
            background: rgba(37, 99, 235, 0.24);
            border-color: #60a5fa;
          }
        }

        .card-export-actions {
          .select-all-btn {
            border-color: var(--color-line);
            background: var(--color-panel-soft);
            color: var(--color-text);

            .action-icon--active {
              color: #60a5fa;
            }
          }

          .format-select-wrapper .format-select {
            background: var(--color-panel-soft);
            border-color: var(--color-line);
            color: var(--color-text);

            &:focus {
              border-color: #60a5fa;
            }
          }

          .export-btn-primary {
            background: #2563eb;
            color: #ffffff;

            &:hover:not(:disabled) {
              background: #1d4ed8;
            }
          }
        }
      }
    }

    .image-extractor-input-card .editor-container {
      background: var(--color-panel);

      .editor-gutter {
        background: rgba(0, 0, 0, 0.25);
        border-right-color: var(--color-line);
        color: #64748b;
      }

      .editor-textarea {
        color: #f1f5f9;

        &:focus {
          box-shadow: inset 0 0 0 2px rgba(96, 165, 250, 0.15);
        }
      }
    }

    .image-extractor-result-card {
      .extractor-notice-banner {
        &.is-info {
          background: rgba(37, 99, 235, 0.16);
          color: #93c5fd;
          border-bottom-color: rgba(96, 165, 250, 0.3);
        }

        &.is-success {
          background: rgba(16, 185, 129, 0.16);
          color: #6ee7b7;
          border-bottom-color: rgba(52, 211, 153, 0.3);
        }

        &.is-error {
          background: rgba(239, 68, 68, 0.16);
          color: #fca5a5;
          border-bottom-color: rgba(248, 113, 113, 0.3);
        }
      }

      .extractor-result-body .image-cards-grid .image-grid-card {
        background: var(--color-panel);
        border-color: var(--color-line);

        &:hover {
          border-color: rgba(96, 165, 250, 0.45);
          box-shadow: 0 6px 16px rgba(0, 0, 0, 0.25);
        }

        &.is-selected {
          border-color: #3b82f6;
          box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.25);
        }

        .card-select-checkbox {
          border-color: #475569;
          background: rgba(30, 41, 59, 0.85);

          &.is-checked {
            background: #2563eb;
            border-color: #2563eb;
          }
        }

        .card-thumb-stage {
          background:
            linear-gradient(45deg, #1e293b 25%, transparent 25%),
            linear-gradient(-45deg, #1e293b 25%, transparent 25%),
            linear-gradient(45deg, transparent 75%, #1e293b 75%),
            linear-gradient(-45deg, transparent 75%, #1e293b 75%),
            #0f172a;
          background-position: 0 0, 0 8px, 8px -8px, -8px 0;
          background-size: 16px 16px;
        }

        .card-footer-meta {
          background: var(--color-panel);
          border-top-color: var(--color-line);

          .card-index-label {
            color: #60a5fa;
          }

          .card-ext-link-btn:hover {
            color: #60a5fa;
            background: rgba(37, 99, 235, 0.16);
          }
        }
      }
    }

    .image-preview-modal .preview-dialog {
      background: var(--color-panel);
      border-color: var(--color-line);

      .preview-dialog-head {
        background: var(--color-panel);
        border-bottom-color: var(--color-line);

        .preview-heading .preview-icon-box {
          background: rgba(37, 99, 235, 0.16);
          color: #60a5fa;
        }
      }

      .preview-dialog-footer {
        background: var(--color-panel);
        border-top-color: var(--color-line);

        .preview-external-link {
          color: #60a5fa;
        }
      }
    }
  }

  .spinning {
    animation: extractor-spin 0.8s linear infinite;
  }
}

@keyframes extractor-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 880px) {
  .image-extractor {
    grid-template-columns: 1fr;
    overflow-y: auto;
  }
}
</style>

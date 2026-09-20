<template>
  <BaseModal
    class="image-drawing-dialog"
    :title="annotating ? '标注重绘区域' : '绘制草图'"
    :description="
      annotating
        ? '涂抹需要修改的位置，标记区域将生成透明蒙版。'
        : '画出构图，应用后配合文字描述生成完整画面。'
    "
    @close="$emit('close')"
  >
    <section class="image-drawing">
      <div class="drawing-toolbar">
        <button
          class="drawing-button"
          :class="{ active: !erasing }"
          type="button"
          @click="erasing = false"
        >
          {{ annotating ? "标记" : "画笔" }}
        </button>
        <button
          class="drawing-button"
          :class="{ active: erasing }"
          type="button"
          @click="erasing = true"
        >
          橡皮
        </button>
        <button class="drawing-button" type="button" @click="clearCanvas">
          清空
        </button>
        <label class="brush-control"
          >笔刷 {{ brush
          }}<input
            v-model.number="brush"
            type="range"
            :min="annotating ? 16 : 4"
            :max="annotating ? 180 : 64"
        /></label>
      </div>
      <p v-if="error" class="drawing-error">{{ error }}</p>
      <div class="drawing-stage">
        <canvas
          ref="canvas"
          class="drawing-canvas"
          aria-label="绘图画布"
          @pointerdown="startStroke"
          @pointermove="moveStroke"
          @pointerup="endStroke"
          @pointercancel="endStroke"
          @lostpointercapture="endStroke"
        ></canvas>
      </div>
      <footer class="drawing-footer">
        <button class="drawing-button" type="button" @click="$emit('close')">
          取消
        </button>
        <button
          class="drawing-button active"
          type="button"
          :disabled="!dirty || !ready"
          @click="applyDrawing"
        >
          {{ annotating ? "应用标注" : "使用草图" }}
        </button>
      </footer>
    </section>
  </BaseModal>
</template>

<script setup>
import { computed, onMounted, ref } from "vue"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({ source: { type: String, default: "" } })
const emit = defineEmits(["close", "apply"])
const annotating = computed(() => Boolean(props.source))
const brush = ref(annotating.value ? 64 : 18)
const erasing = ref(false)
const canvas = ref(null)
const ready = ref(false)
const dirty = ref(false)
const error = ref("")
let sourceImage
let layer
let overlay
let previous
let pointerId

// 蒙版独立于预览：白色保留原图，透明区域交给上游重绘。
function renderCanvas() {
  const context = canvas.value.getContext("2d")
  context.clearRect(0, 0, canvas.value.width, canvas.value.height)
  if (annotating.value) {
    context.drawImage(sourceImage, 0, 0)
    const overlayContext = overlay.getContext("2d")
    overlayContext.globalCompositeOperation = "source-over"
    overlayContext.clearRect(0, 0, layer.width, layer.height)
    overlayContext.fillStyle = "rgba(70, 70, 70, 0.55)"
    overlayContext.fillRect(0, 0, layer.width, layer.height)
    overlayContext.globalCompositeOperation = "destination-out"
    overlayContext.drawImage(layer, 0, 0)
    context.drawImage(overlay, 0, 0)
  } else {
    context.drawImage(layer, 0, 0)
  }
}

function clearCanvas() {
  if (!layer) return
  const context = layer.getContext("2d")
  context.globalCompositeOperation = "source-over"
  context.fillStyle = "#fff"
  context.fillRect(0, 0, layer.width, layer.height)
  dirty.value = false
  renderCanvas()
}

function point(event) {
  const rect = canvas.value.getBoundingClientRect()
  return {
    x: ((event.clientX - rect.left) * layer.width) / rect.width,
    y: ((event.clientY - rect.top) * layer.height) / rect.height
  }
}

function startStroke(event) {
  if (!ready.value || event.button !== 0 || pointerId !== undefined) return
  event.preventDefault()
  pointerId = event.pointerId
  canvas.value.setPointerCapture(pointerId)
  previous = point(event)
  moveStroke(event)
}

function moveStroke(event) {
  if (event.pointerId !== pointerId || !previous) return
  const next = point(event)
  const context = layer.getContext("2d")
  context.globalCompositeOperation =
    annotating.value && !erasing.value ? "destination-out" : "source-over"
  context.strokeStyle = context.fillStyle =
    annotating.value || erasing.value ? "#fff" : "#202020"
  context.lineWidth = brush.value
  context.lineCap = context.lineJoin = "round"
  context.beginPath()
  context.moveTo(previous.x, previous.y)
  context.lineTo(next.x, next.y)
  context.stroke()
  context.beginPath()
  context.arc(next.x, next.y, brush.value / 2, 0, Math.PI * 2)
  context.fill()
  previous = next
  dirty.value = true
  renderCanvas()
}

function endStroke(event) {
  if (event.pointerId !== pointerId) return
  previous = null
  pointerId = undefined
  if (canvas.value.hasPointerCapture(event.pointerId))
    canvas.value.releasePointerCapture(event.pointerId)
  // 全部擦除后不能提交空白草图或未标记蒙版。
  const pixels = layer
    .getContext("2d")
    .getImageData(0, 0, layer.width, layer.height).data
  dirty.value = false
  for (let index = 0; index < pixels.length; index += 4) {
    if (annotating.value ? pixels[index + 3] < 255 : pixels[index] < 250) {
      dirty.value = true
      break
    }
  }
}

function applyDrawing() {
  if (dirty.value && ready.value)
    emit("apply", {
      name: annotating.value ? "edit-mask.png" : "sketch.png",
      url: layer.toDataURL("image/png"),
      source: props.source
    })
}

onMounted(async () => {
  try {
    if (annotating.value) {
      sourceImage = new Image()
      sourceImage.src = props.source
      await sourceImage.decode()
    }
    canvas.value.width = sourceImage?.naturalWidth || 1024
    canvas.value.height = sourceImage?.naturalHeight || 1024
    layer = document.createElement("canvas")
    layer.width = canvas.value.width
    layer.height = canvas.value.height
    if (annotating.value) {
      overlay = document.createElement("canvas")
      overlay.width = layer.width
      overlay.height = layer.height
    }
    clearCanvas()
    ready.value = true
  } catch {
    error.value = "图片加载失败，请关闭后重新打开"
  }
})
</script>

<style scoped lang="less">
.image-drawing-dialog {
  :deep(.base-modal__panel) {
    max-width: calc(100vw - 48px);
  }
}
.image-drawing {
  display: flex;
  flex-direction: column;
  min-height: 0;
  gap: 12px;
  .drawing-toolbar,
  .drawing-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    .drawing-button {
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel-soft);
      color: var(--color-text);
      padding: 7px 12px;
      cursor: pointer;
      &.active {
        background: var(--color-primary-solid);
        color: #fff;
      }
      &:disabled {
        opacity: 0.5;
        cursor: default;
      }
    }
    .brush-control {
      display: flex;
      align-items: center;
      gap: 10px;
      margin-left: auto;
    }
  }
  .drawing-stage {
    display: flex;
    justify-content: center;
    overflow: auto;
    min-height: 0;
    background: var(--color-panel-soft);
    .drawing-canvas {
      max-width: 100%;
      max-height: 55vh;
      object-fit: contain;
      touch-action: none;
      cursor: crosshair;
    }
  }
  .drawing-error {
    color: var(--color-danger);
  }
  .drawing-footer {
    justify-content: flex-end;
  }
}
</style>

<template>
  <div class="screenshot-editor">
    <canvas
      ref="canvas"
      class="editor-canvas"
      @pointerdown="startDrawing"
      @pointermove="draw"
      @pointerup="finishDrawing"
      @pointercancel="cancelDrawing"
    />
    <textarea
      v-if="textPosition"
      ref="textInput"
      v-model="text"
      class="editor-text"
      :style="textStyle"
      placeholder="输入文字，Ctrl+Enter 确认"
      @pointerdown.stop
      @keydown.stop="textKeydown"
    />
    <Teleport to="body">
      <div class="screenshot-toolbar" :style="toolbarStyle" @pointerdown.stop>
        <div
          v-if="!['select', 'eraser'].includes(tool)"
          class="toolbar-options"
        >
          <input
            v-model="color"
            class="toolbar-color"
            type="color"
            title="标注颜色"
            aria-label="标注颜色"
            :disabled="busy"
          />
          <select
            v-model.number="width"
            class="toolbar-width"
            title="线宽 / 文字大小"
            aria-label="线宽 / 文字大小"
            :disabled="busy"
          >
            <option v-for="size in [2, 4, 6, 10]" :key="size" :value="size">
              {{ size }}
            </option>
          </select>
        </div>
        <div class="toolbar-buttons">
          <button
            v-for="item in tools"
            :key="item.id"
            type="button"
            class="toolbar-button"
            :class="{ active: tool === item.id }"
            :title="item.label"
            :aria-label="item.label"
            :aria-pressed="tool === item.id"
            :disabled="busy"
            @click="setTool(item.id)"
          >
            <component :is="item.icon" :size="17" />
          </button>
          <span class="toolbar-separator" />
          <button
            class="toolbar-button"
            type="button"
            title="取消 Esc"
            aria-label="取消"
            :disabled="busy"
            @click="$emit('cancel')"
          >
            <X :size="17" />
          </button>
          <button
            class="toolbar-button"
            type="button"
            title="撤销 Ctrl+Z"
            aria-label="撤销"
            :disabled="!undoStack.length || busy"
            @click="undo"
          >
            <Undo2 :size="17" />
          </button>
          <button
            class="toolbar-button"
            type="button"
            title="重做 Ctrl+Shift+Z"
            aria-label="重做"
            :disabled="!redoStack.length || busy"
            @click="redo"
          >
            <Redo2 :size="17" />
          </button>
          <button
            v-if="reselect"
            class="toolbar-button"
            type="button"
            title="重新选择区域"
            aria-label="重新选择区域"
            :disabled="busy"
            @click="$emit('reselect', annotations.length > 0)"
          >
            <Crop :size="17" />
          </button>
          <span class="toolbar-separator" />
          <button
            v-for="action in outputs"
            :key="action.id"
            class="toolbar-button"
            type="button"
            :title="action.label"
            :aria-label="action.label"
            :disabled="busy"
            @click="output(action.id)"
          >
            <component :is="action.icon" :size="17" />
          </button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  ArrowUpRight,
  Circle,
  Copy,
  Crop,
  Eraser,
  Grid2X2,
  Highlighter,
  Minus,
  MousePointer2,
  PenLine,
  Pin,
  Redo2,
  Save,
  Square,
  Type,
  Undo2,
  X
} from "lucide-vue-next"
import { loadImage, renderDocument } from "../renderer"

const props = defineProps({
  source: { type: String, required: true },
  initialAnnotations: { type: Array, default: () => [] },
  toolbarStyle: {
    type: Object,
    default: () => ({
      left: "50%",
      bottom: "12px",
      maxWidth: "calc(100vw - 8px)",
      transform: "translateX(-50%)"
    })
  },
  busy: Boolean,
  reselect: Boolean
})
const emit = defineEmits([
  "output",
  "reselect",
  "cancel",
  "error",
  "selection-start"
])
const canvas = ref(null)
const tool = ref(props.reselect ? "select" : "pen")
const color = ref("#f43f5e")
const width = ref(4)
const annotations = ref(JSON.parse(JSON.stringify(props.initialAnnotations)))
const undoStack = ref([])
const redoStack = ref([])
const textPosition = ref(null)
const text = ref("")
const textInput = ref(null)
let image = null
let pending = null
let sourceGeneration = 0
const tools = [
  { id: "select", label: "调整选区", icon: MousePointer2 },
  { id: "rectangle", label: "矩形", icon: Square },
  { id: "ellipse", label: "椭圆", icon: Circle },
  { id: "line", label: "直线", icon: Minus },
  { id: "arrow", label: "箭头", icon: ArrowUpRight },
  { id: "pen", label: "画笔", icon: PenLine },
  { id: "highlighter", label: "荧光笔", icon: Highlighter },
  { id: "mosaic", label: "马赛克", icon: Grid2X2 },
  { id: "text", label: "文字", icon: Type },
  { id: "eraser", label: "橡皮擦（删除标注）", icon: Eraser }
].filter(item => props.reselect || item.id !== "select")
const outputs = [
  { id: "pin", label: "贴到桌面", icon: Pin },
  { id: "export", label: "保存 PNG", icon: Save },
  { id: "copy", label: "复制图片 Enter", icon: Copy }
]
const textStyle = computed(() => ({
  left: `${(textPosition.value.x / image.naturalWidth) * 100}%`,
  top: `${(textPosition.value.y / image.naturalHeight) * 100}%`,
  color: color.value
}))

function repaint() {
  if (image && canvas.value)
    renderDocument(canvas.value, image, [
      ...annotations.value,
      ...(pending ? [pending] : [])
    ])
}

function point(event) {
  const rect = canvas.value.getBoundingClientRect()
  return {
    x: Math.max(
      0,
      Math.min(
        image.naturalWidth,
        ((event.clientX - rect.left) * image.naturalWidth) / rect.width
      )
    ),
    y: Math.max(
      0,
      Math.min(
        image.naturalHeight,
        ((event.clientY - rect.top) * image.naturalHeight) / rect.height
      )
    )
  }
}

function commitText() {
  if (textPosition.value && text.value.trim()) {
    undoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
    annotations.value.push({
      type: "text",
      points: [textPosition.value],
      text: text.value,
      color: color.value,
      width: width.value
    })
    redoStack.value = []
  }
  textPosition.value = null
  text.value = ""
  repaint()
}

function setTool(value) {
  commitText()
  tool.value = value
}

async function startDrawing(event) {
  if (!image || props.busy || event.button !== 0) return
  if (tool.value === "select") {
    emit("selection-start", event, annotations.value.length > 0)
    return
  }
  event.stopPropagation()
  commitText()
  if (tool.value === "eraser") {
    const position = point(event)
    const index = annotations.value.findLastIndex(item => {
      const padding = Math.max(8, item.width * 3)
      const xs = item.points.map(point => point.x)
      const ys = item.points.map(point => point.y)
      const textWidth =
        item.type === "text"
          ? Math.max(...item.text.split("\n").map(line => line.length)) *
            Math.max(14, item.width * 7)
          : 0
      const textHeight =
        item.type === "text"
          ? item.text.split("\n").length * Math.max(14, item.width * 7) * 1.35
          : 0
      return (
        position.x >= Math.min(...xs) - padding &&
        position.x <= Math.max(...xs) + textWidth + padding &&
        position.y >= Math.min(...ys) - padding &&
        position.y <= Math.max(...ys) + textHeight + padding
      )
    })
    if (index >= 0) {
      undoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
      annotations.value.splice(index, 1)
      redoStack.value = []
      repaint()
    }
    return
  }
  if (tool.value === "text") {
    textPosition.value = point(event)
    await nextTick()
    textInput.value?.focus()
    return
  }
  canvas.value.setPointerCapture(event.pointerId)
  const start = point(event)
  pending = {
    type: tool.value,
    points: [start, start],
    color: color.value,
    width: width.value
  }
}

function draw(event) {
  if (!pending) return
  const value = point(event)
  if (["pen", "highlighter"].includes(pending.type)) pending.points.push(value)
  else pending.points[1] = value
  repaint()
}

function finishDrawing(event) {
  if (!pending) return
  draw(event)
  // 一次拖动只提交一条命令，撤销不复制整张原图。
  undoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
  annotations.value.push(pending)
  pending = null
  redoStack.value = []
  repaint()
}

function cancelDrawing() {
  pending = null
  repaint()
}
function undo() {
  commitText()
  const previous = undoStack.value.pop()
  if (previous) {
    redoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
    annotations.value = previous
  }
  repaint()
}
function redo() {
  const next = redoStack.value.pop()
  if (next) {
    undoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
    annotations.value = next
  }
  repaint()
}

function output(action) {
  commitText()
  if (!image || pending) return
  emit("output", {
    action,
    source: props.source,
    output: canvas.value.toDataURL("image/png"),
    annotations: JSON.parse(JSON.stringify(annotations.value))
  })
}

function textKeydown(event) {
  if (event.isComposing) return
  if (event.key === "Enter" && event.ctrlKey) {
    event.preventDefault()
    commitText()
  }
  if (event.key === "Escape") {
    textPosition.value = null
    text.value = ""
  }
}

function keydown(event) {
  if (
    props.busy ||
    event.isComposing ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(event.target.tagName)
  )
    return
  if (event.ctrlKey && event.key.toLowerCase() === "z") {
    event.preventDefault()
    event.shiftKey ? redo() : undo()
  }
  if (event.ctrlKey && event.key.toLowerCase() === "y") {
    event.preventDefault()
    redo()
  }
  if (event.key === "Enter") {
    event.preventDefault()
    output("copy")
  }
}

watch(
  () => props.source,
  async source => {
    const generation = ++sourceGeneration
    try {
      const loaded = await loadImage(source)
      if (generation !== sourceGeneration) return
      image = loaded
      annotations.value = JSON.parse(JSON.stringify(props.initialAnnotations))
      undoStack.value = annotations.value.map((_, index) =>
        JSON.parse(JSON.stringify(annotations.value.slice(0, index)))
      )
      redoStack.value = []
      pending = null
      textPosition.value = null
      await nextTick()
      repaint()
    } catch (error) {
      emit("error", error.message)
    }
  },
  { immediate: true }
)
onMounted(() => window.addEventListener("keydown", keydown))
onBeforeUnmount(() => {
  sourceGeneration++
  window.removeEventListener("keydown", keydown)
})
</script>

<style scoped lang="less">
.screenshot-editor {
  position: relative;
  width: 100%;
  height: 100%;
  .editor-canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: crosshair;
    touch-action: none;
  }
  .editor-text {
    position: absolute;
    width: 240px;
    min-height: 70px;
    padding: 4px;
    border: 1px solid #38bdf8;
    background: #18222de6;
    font: inherit;
    font-size: var(--font-size-base);
    resize: both;
  }
}
.screenshot-toolbar {
  .toolbar-buttons {
    display: flex;
    align-items: center;
    gap: 2px;
    overflow-x: auto;
    max-width: 100%;
  }
  .toolbar-options {
    position: absolute;
    bottom: calc(100% + 5px);
    left: 0;
    display: flex;
    gap: 8px;
    padding: 5px 8px;
    border: 1px solid #aeb4bc;
    border-radius: 3px;
    background: #f5f5f5;
    box-shadow: 0 2px 5px #0002;
  }
  .toolbar-button,
  .toolbar-color,
  .toolbar-width,
  .toolbar-separator {
    flex-shrink: 0;
  }
  position: fixed;
  z-index: 1000;
  display: flex;
  gap: 2px;
  align-items: center;
  padding: 4px 6px;
  border: 1px solid #aeb4bc;
  border-radius: 3px;
  background: #f5f5f5;
  color: #353535;
  box-shadow: 0 2px 6px #0003;
  .toolbar-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 31px;
    height: 31px;
    padding: 0;
    border: 0;
    border-radius: 2px;
    background: transparent;
    color: inherit;
    cursor: pointer;
    &:hover,
    &.active {
      background: #dcecff;
      color: #1478d4;
    }
    &:disabled {
      opacity: 0.35;
      cursor: default;
    }
  }
  .toolbar-color {
    width: 27px;
    height: 27px;
    padding: 2px;
    border: 0;
    background: transparent;
  }
  .toolbar-width {
    width: 38px;
    height: 27px;
    border: 1px solid #b8bec6;
    border-radius: 4px;
    background: #fff;
    color: inherit;
    font-size: var(--font-size-xs);
  }
  .toolbar-separator {
    width: 1px;
    height: 22px;
    margin: 0 3px;
    background: #b8bec6;
  }
}
</style>

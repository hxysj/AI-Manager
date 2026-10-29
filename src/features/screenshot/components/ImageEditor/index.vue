<template>
  <div class="screenshot-editor">
    <canvas
      ref="canvas"
      class="editor-canvas"
      @pointerdown="startDrawing"
      @pointermove="draw"
      @pointerup="finishDrawing"
      @pointercancel="cancelDrawing"
      @wheel="resizeRectangleWidth"
    />
    <div
      v-if="activeRectangle"
      ref="rectangleSelection"
      class="rectangle-selection"
      tabindex="0"
      aria-label="调整矩形，滚轮调整线宽"
      :style="rectangleStyle"
      @blur="commitRectangle"
      @wheel="resizeRectangleWidth"
    >
      <button
        v-for="corner in ['nw', 'ne', 'se', 'sw']"
        :key="corner"
        type="button"
        class="rectangle-handle"
        tabindex="-1"
        :aria-label="`调整矩形 ${corner}`"
        :style="{
          left: corner.includes('w') ? '0' : '100%',
          top: corner.includes('n') ? '0' : '100%',
          cursor: ['nw', 'se'].includes(corner) ? 'nwse-resize' : 'nesw-resize'
        }"
        :disabled="busy || outputting"
        @pointerdown.stop.prevent="startRectangleResize($event, corner)"
        @pointermove.stop="resizeRectangle"
        @pointerup.stop="finishRectangleResize"
        @pointercancel.stop="cancelRectangleResize"
      />
    </div>
    <textarea
      v-if="textPosition"
      ref="textInput"
      v-model="text"
      class="editor-text"
      :style="textStyle"
      rows="1"
      wrap="off"
      placeholder="输入文字"
      aria-label="截图文字"
      title="Enter 换行，Ctrl+Enter 确认"
      @pointerdown.stop
      @keydown.stop="textKeydown"
    />
    <EditorToolbar
      :toolbar-style="toolbarStyle"
      :tool="tool"
      :color="color"
      :width="width"
      :line-options="lineOptions"
      :busy="busy"
      :outputting="outputting"
      :can-undo="Boolean(undoStack.length)"
      :can-redo="Boolean(redoStack.length)"
      :reselect="reselect"
      :has-annotations="annotations.length > 0"
      @update:color="color = $event"
      @update:width="width = $event"
      @tool="setTool"
      @line-option="selectLineOption"
      @cancel="$emit('cancel')"
      @undo="undo"
      @redo="redo"
      @reselect="$emit('reselect', $event)"
      @output="output"
      @hover="$emit('toolbar-hover', $event)"
    />
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import EditorToolbar from "./components/EditorToolbar.vue"
import {
  canvasSource,
  loadImage,
  renderDocument,
  selectionRect,
  thumbnailSource
} from "../../renderer"

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
  "selection-start",
  "toolbar-hover"
])
const canvas = ref(null)
const tool = ref(props.reselect ? "select" : "pen")
const outputting = ref(false)
const color = ref("#f43f5e")
const width = ref(4)
// 直线和箭头共用一个入口，首次使用默认绘制直线。
const lineOptions = ref({ style: "solid", type: "line" })
const annotations = ref(JSON.parse(JSON.stringify(props.initialAnnotations)))
const undoStack = ref([])
const redoStack = ref([])
const textPosition = ref(null)
const text = ref("")
const textInput = ref(null)
const activeRectangle = ref(null)
const rectangleSelection = ref(null)
let image = null
let pending = null
let rectangleResize = null
let sourceGeneration = 0
const textStyle = computed(() => {
  // 输入框按原图字号缩放，默认只占一行，换行后随内容增高。
  const scale = canvas.value.getBoundingClientRect().width / image.naturalWidth
  const size = Math.max(14, width.value * 7) * scale
  return {
    left: `${(textPosition.value.x / image.naturalWidth) * 100}%`,
    top: `${(textPosition.value.y / image.naturalHeight) * 100}%`,
    maxWidth: `calc(${100 - (textPosition.value.x / image.naturalWidth) * 100}% - 2px)`,
    height: `${text.value.split("\n").length * size * 1.35}px`,
    fontSize: `${size}px`,
    color: color.value
  }
})
const rectangleStyle = computed(() => {
  const [start, end] = activeRectangle.value.points
  return {
    left: `${(Math.min(start.x, end.x) / image.naturalWidth) * 100}%`,
    top: `${(Math.min(start.y, end.y) / image.naturalHeight) * 100}%`,
    width: `${(Math.abs(end.x - start.x) / image.naturalWidth) * 100}%`,
    height: `${(Math.abs(end.y - start.y) / image.naturalHeight) * 100}%`
  }
})

function repaint() {
  if (image && canvas.value)
    renderDocument(canvas.value, image, [
      ...annotations.value,
      ...(activeRectangle.value ? [activeRectangle.value] : []),
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
  if (!textPosition.value) return
  const changed = Boolean(text.value.trim())
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
  if (changed) repaint()
}

function setTool(value) {
  commitRectangle()
  commitText()
  tool.value = value
}

function selectLineOption(key, value) {
  lineOptions.value[key] = value
}

// 点击选区外时提交当前矩形，工具栏下拉状态由子组件自行维护。
function commitRectangleOnOutsidePointer(event) {
  if (!event.target.closest(".rectangle-selection")) commitRectangle()
}

async function startDrawing(event) {
  if (!image || props.busy || outputting.value || event.button !== 0) return
  commitRectangle()
  if (tool.value === "select") {
    emit("selection-start", event, annotations.value.length > 0)
    return
  }
  event.stopPropagation()
  commitText()
  if (tool.value === "eraser") {
    // 保留擦除路径，渲染器只在标注层局部擦除。
    canvas.value.setPointerCapture(event.pointerId)
    const start = point(event)
    pending = {
      type: "eraser",
      points: [start, start],
      width: width.value
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
    type: tool.value === "line" ? lineOptions.value.type : tool.value,
    points: [start, start],
    color: color.value,
    width: width.value,
    style: lineOptions.value.style
  }
}

function draw(event) {
  if (!pending) return
  const value = point(event)
  if (["pen", "highlighter", "eraser"].includes(pending.type))
    pending.points.push(value)
  else pending.points[1] = value
  repaint()
}

function finishDrawing(event) {
  if (!pending) return
  draw(event)
  if (pending.type === "rectangle") {
    // 新矩形先保留为草稿，四个角点和线宽只在本次聚焦期间可调整。
    const rect = selectionRect(
      pending.points[0],
      pending.points[1],
      image.naturalWidth,
      image.naturalHeight
    )
    pending.points = [
      { x: rect.x, y: rect.y },
      { x: rect.x + rect.width, y: rect.y + rect.height }
    ]
    activeRectangle.value = pending
    pending = null
    repaint()
    nextTick(() => rectangleSelection.value?.focus({ preventScroll: true }))
    if (canvas.value.hasPointerCapture(event.pointerId))
      canvas.value.releasePointerCapture(event.pointerId)
    return
  }
  // 一次拖动只提交一条命令，撤销不复制整张原图。
  undoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
  annotations.value.push(pending)
  pending = null
  redoStack.value = []
  repaint()
  if (canvas.value.hasPointerCapture(event.pointerId))
    canvas.value.releasePointerCapture(event.pointerId)
}

function cancelDrawing(event) {
  if (event && canvas.value.hasPointerCapture(event.pointerId))
    canvas.value.releasePointerCapture(event.pointerId)
  pending = null
  repaint()
}

function commitRectangle() {
  if (!activeRectangle.value) return
  // 失焦后合入标注层，后续点击不再命中这一个矩形；撤销仍按一次绘制处理。
  undoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
  annotations.value.push(activeRectangle.value)
  activeRectangle.value = null
  rectangleResize = null
  redoStack.value = []
  repaint()
}

function startRectangleResize(event, corner) {
  if (
    !activeRectangle.value ||
    props.busy ||
    outputting.value ||
    event.button !== 0
  )
    return
  const [start, end] = activeRectangle.value.points
  rectangleResize = {
    corner,
    points: JSON.parse(JSON.stringify(activeRectangle.value.points)),
    anchor: {
      x: corner.includes("w")
        ? Math.max(start.x, end.x)
        : Math.min(start.x, end.x),
      y: corner.includes("n")
        ? Math.max(start.y, end.y)
        : Math.min(start.y, end.y)
    }
  }
  event.currentTarget.setPointerCapture(event.pointerId)
}

function resizeRectangle(event) {
  if (!rectangleResize || !activeRectangle.value) return
  const { corner, anchor } = rectangleResize
  const end = point(event)
  // 固定对角点，避免拖过对角后控制点跳到另一侧。
  end.x = corner.includes("w")
    ? Math.min(end.x, anchor.x - 1)
    : Math.max(end.x, anchor.x + 1)
  end.y = corner.includes("n")
    ? Math.min(end.y, anchor.y - 1)
    : Math.max(end.y, anchor.y + 1)
  activeRectangle.value.points = [anchor, end]
  repaint()
}

function finishRectangleResize(event) {
  if (!rectangleResize) return
  resizeRectangle(event)
  rectangleResize = null
  if (event.currentTarget.hasPointerCapture(event.pointerId))
    event.currentTarget.releasePointerCapture(event.pointerId)
}

function cancelRectangleResize(event) {
  if (!rectangleResize) return
  activeRectangle.value.points = rectangleResize.points
  rectangleResize = null
  if (event.currentTarget.hasPointerCapture(event.pointerId))
    event.currentTarget.releasePointerCapture(event.pointerId)
  repaint()
}

function resizeRectangleWidth(event) {
  if (!activeRectangle.value || props.busy || outputting.value || !event.deltaY)
    return
  event.preventDefault()
  event.stopPropagation()
  // 滚轮逐像素调整，沿用标注线宽，导出不包含定位点。
  activeRectangle.value.width = Math.max(
    1,
    Math.min(64, activeRectangle.value.width - Math.sign(event.deltaY))
  )
  width.value = activeRectangle.value.width
  repaint()
}

function undo() {
  commitRectangle()
  commitText()
  const previous = undoStack.value.pop()
  if (previous) {
    redoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
    annotations.value = previous
  }
  repaint()
}
function redo() {
  commitRectangle()
  const next = redoStack.value.pop()
  if (next) {
    undoStack.value.push(JSON.parse(JSON.stringify(annotations.value)))
    annotations.value = next
  }
  repaint()
}

async function output(action) {
  if (!image || pending || outputting.value) return
  outputting.value = true
  commitRectangle()
  commitText()
  const generation = sourceGeneration
  try {
    const items = JSON.parse(JSON.stringify(annotations.value))
    const [output, thumbnail] = await Promise.all([
      items.length ? canvasSource(canvas.value) : props.source,
      thumbnailSource(canvas.value)
    ])
    if (generation !== sourceGeneration) return
    emit("output", {
      action,
      source: props.source,
      output,
      thumbnail,
      annotations: items
    })
  } catch (error) {
    if (generation === sourceGeneration) emit("error", error.message)
  } finally {
    outputting.value = false
  }
}

function textKeydown(event) {
  if (event.isComposing) return
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
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
    outputting.value ||
    event.isComposing ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(event.target.tagName)
  )
    return
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
    event.preventDefault()
    event.shiftKey ? redo() : undo()
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") {
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
  async (source) => {
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
      activeRectangle.value = null
      rectangleResize = null
      textPosition.value = null
      await nextTick()
      repaint()
    } catch (error) {
      emit("error", error.message)
    }
  },
  { immediate: true }
)
onMounted(() => {
  window.addEventListener("keydown", keydown)
  window.addEventListener("pointerdown", commitRectangleOnOutsidePointer, true)
  window.addEventListener("blur", commitRectangle)
})
onBeforeUnmount(() => {
  sourceGeneration++
  window.removeEventListener("keydown", keydown)
  window.removeEventListener(
    "pointerdown",
    commitRectangleOnOutsidePointer,
    true
  )
  window.removeEventListener("blur", commitRectangle)
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
    box-sizing: content-box;
    width: 240px;
    min-height: 0;
    padding: 0;
    border: 1px solid #38bdf8;
    outline: none;
    background: transparent;
    font-family: "Microsoft YaHei", sans-serif;
    line-height: 1.35;
    text-align: left;
    resize: none;
    overflow: hidden;
  }
  .rectangle-selection {
    position: absolute;
    outline: none;
    pointer-events: none;
    .rectangle-handle {
      position: absolute;
      width: 9px;
      height: 9px;
      padding: 0;
      border: 1px solid #0284c7;
      background: #fff;
      transform: translate(-50%, -50%);
      pointer-events: auto;
      touch-action: none;
    }
  }
}
</style>

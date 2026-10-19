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
      rows="1"
      wrap="off"
      placeholder="输入文字"
      aria-label="截图文字"
      title="Enter 换行，Ctrl+Enter 确认"
      @pointerdown.stop
      @keydown.stop="textKeydown"
    />
    <Teleport to="body">
      <div
        class="screenshot-toolbar"
        :style="toolbarStyle"
        @pointerdown.stop
        @pointerenter="$emit('toolbar-hover', true)"
        @pointerleave="$emit('toolbar-hover', false)"
      >
        <div v-if="tool !== 'select'" class="toolbar-options">
          <div
            v-if="!['eraser', 'mosaic'].includes(tool)"
            class="toolbar-group"
          >
            <span class="toolbar-label">颜色</span>
            <button
              v-for="preset in colorPresets"
              :key="preset"
              type="button"
              class="toolbar-color-preset"
              :class="{ active: color === preset }"
              :style="{ backgroundColor: preset }"
              :title="`颜色 ${preset}`"
              :aria-label="`颜色 ${preset}`"
              :disabled="busy"
              @click="color = preset"
            />
            <input
              v-model="color"
              class="toolbar-color"
              type="color"
              title="自定义颜色"
              aria-label="自定义颜色"
              :disabled="busy"
            />
          </div>
          <div class="toolbar-group">
            <span class="toolbar-label">{{
              tool === "text" ? "字号" : "线宽"
            }}</span>
            <select
              v-model.number="width"
              class="toolbar-select"
              :aria-label="tool === 'text' ? '字号' : '线宽'"
              :disabled="busy"
            >
              <option v-for="size in lineWidths" :key="size" :value="size">
                {{ tool === "text" ? Math.max(14, size * 7) : size }} px
              </option>
            </select>
          </div>
          <template v-if="['rectangle', 'ellipse', 'line'].includes(tool)">
            <div
              v-for="control in lineControls"
              v-show="control.id === 'style' || tool === 'line'"
              :key="control.id"
              class="toolbar-group"
            >
              <div
                class="toolbar-dropdown"
                @keydown.stop
                @keydown.esc.prevent="dropdown = ''"
                @focusout="closeDropdownOnBlur"
              >
                <button
                  type="button"
                  class="toolbar-dropdown-toggle"
                  :title="control.label"
                  :aria-label="control.label"
                  :aria-expanded="dropdown === control.id"
                  :disabled="busy"
                  @click="dropdown = dropdown === control.id ? '' : control.id"
                >
                  <component
                    :is="
                      lineOptions[control.id] === 'arrow' ? ArrowUpRight : Minus
                    "
                    :size="20"
                    :stroke-dasharray="
                      lineOptions[control.id] === 'dashed' ? '4 3' : undefined
                    "
                  />
                  <ChevronDown :size="12" />
                </button>
                <div
                  v-if="dropdown === control.id"
                  class="toolbar-dropdown-menu"
                >
                  <button
                    v-for="option in control.options"
                    :key="option.value"
                    type="button"
                    class="toolbar-dropdown-option"
                    :class="{
                      active: lineOptions[control.id] === option.value
                    }"
                    :title="option.label"
                    :aria-label="option.label"
                    :aria-pressed="lineOptions[control.id] === option.value"
                    :disabled="busy"
                    @click="selectLineOption(control.id, option.value)"
                  >
                    <component
                      :is="option.value === 'arrow' ? ArrowUpRight : Minus"
                      :size="20"
                      :stroke-dasharray="
                        option.value === 'dashed' ? '4 3' : undefined
                      "
                    />
                  </button>
                </div>
              </div>
            </div>
          </template>
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
            <component
              :is="
                item.id === 'line' && lineOptions.type === 'arrow'
                  ? ArrowUpRight
                  : item.icon
              "
              :size="17"
            />
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
            :disabled="busy || outputting"
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
  ChevronDown,
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
const dropdown = ref("")
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
  { id: "line", label: "直线 / 箭头", icon: Minus },
  { id: "pen", label: "画笔", icon: PenLine },
  { id: "highlighter", label: "荧光笔", icon: Highlighter },
  { id: "mosaic", label: "马赛克", icon: Grid2X2 },
  { id: "text", label: "文字", icon: Type },
  { id: "eraser", label: "橡皮擦", icon: Eraser }
].filter((item) => props.reselect || item.id !== "select")
const outputs = [
  { id: "pin", label: "贴到桌面", icon: Pin },
  { id: "export", label: "保存 PNG", icon: Save },
  { id: "copy", label: "复制图片 Enter", icon: Copy }
]
// 子工具栏按当前工具提供常用颜色和尺寸，避免打断绘制流程。
const colorPresets = [
  "#f43f5e",
  "#ef4444",
  "#f59e0b",
  "#facc15",
  "#22c55e",
  "#06b6d4",
  "#3b82f6",
  "#8b5cf6",
  "#ffffff",
  "#111827"
]
const lineWidths = [2, 4, 6, 10]
const lineControls = [
  {
    id: "style",
    label: "线条样式",
    options: [
      { value: "solid", label: "实线" },
      { value: "dashed", label: "虚线" }
    ]
  },
  {
    id: "type",
    label: "直线或箭头",
    options: [
      { value: "line", label: "直线" },
      { value: "arrow", label: "箭头" }
    ]
  }
]
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
  dropdown.value = ""
  tool.value = value
}

function selectLineOption(key, value) {
  lineOptions.value[key] = value
  dropdown.value = ""
}

// 图标菜单在点击外部、移出键盘焦点时关闭，不拦截画布操作。
function closeDropdown(event) {
  if (!event.target.closest(".toolbar-dropdown")) dropdown.value = ""
}

function closeDropdownOnBlur(event) {
  if (!event.currentTarget.contains(event.relatedTarget)) dropdown.value = ""
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

async function output(action) {
  if (!image || pending || outputting.value) return
  outputting.value = true
  commitText()
  try {
    let output = props.source
    if (annotations.value.length) {
      // 异步编码大图，避免点击贴图按钮时同步阻塞渲染线程。
      const blob = await new Promise((resolve, reject) => {
        canvas.value.toBlob(
          (value) =>
            value ? resolve(value) : reject(new Error("截图编码失败")),
          "image/png"
        )
      })
      output = await new Promise((resolve, reject) => {
        const reader = new FileReader()
        reader.onload = () => resolve(reader.result)
        reader.onerror = () => reject(reader.error || new Error("截图读取失败"))
        reader.readAsDataURL(blob)
      })
    }
    emit("output", {
      action,
      source: props.source,
      output,
      annotations: JSON.parse(JSON.stringify(annotations.value))
    })
  } catch (error) {
    emit("error", error.message)
  } finally {
    outputting.value = false
  }
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
      textPosition.value = null
      dropdown.value = ""
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
  window.addEventListener("pointerdown", closeDropdown, true)
})
onBeforeUnmount(() => {
  emit("toolbar-hover", false)
  sourceGeneration++
  window.removeEventListener("keydown", keydown)
  window.removeEventListener("pointerdown", closeDropdown, true)
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
}
.screenshot-toolbar {
  position: fixed;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 0;
  padding: 2px;
  border: 1px solid #8f9aa7;
  border-radius: 1px;
  background: #f8fafc;
  color: #1f2933;
  box-shadow: 0 1px 3px #0005;
  overflow: visible;
  .toolbar-buttons {
    order: 1;
    display: flex;
    align-items: center;
    gap: 0;
    overflow-x: auto;
    max-width: 100%;
    min-height: 30px;
  }
  .toolbar-options {
    order: 2;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    min-height: 28px;
    padding: 2px 4px;
    border-top: 1px solid #b8c1cb;
    background: #eef2f6;
    .toolbar-group {
      .toolbar-select {
        flex-shrink: 0;
        height: 23px;
        min-width: 56px;
        padding: 0 3px;
        border: 1px solid #8f9aa7;
        border-radius: 0;
        background: #fff;
        color: inherit;
        color-scheme: light;
        font-size: var(--font-size-xs);
        cursor: pointer;
        &:disabled {
          opacity: 0.35;
          cursor: default;
        }
      }
      .toolbar-dropdown {
        position: relative;
        .toolbar-dropdown-toggle {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 2px;
          height: 23px;
          padding: 0 4px;
          border: 1px solid #8f9aa7;
          border-radius: 0;
          background: #fff;
          color: inherit;
          cursor: pointer;
          &:hover,
          &[aria-expanded="true"] {
            border-color: #1677d2;
            background: #d6e9fb;
          }
          &:disabled {
            opacity: 0.35;
            cursor: default;
          }
        }
        .toolbar-dropdown-menu {
          position: absolute;
          right: 0;
          bottom: calc(100% + 4px);
          z-index: 1;
          display: flex;
          padding: 3px;
          border: 1px solid #8f9aa7;
          background: #f8fafc;
          box-shadow: 0 1px 3px #0005;
          .toolbar-dropdown-option {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 34px;
            height: 26px;
            padding: 0;
            border: 1px solid transparent;
            background: transparent;
            color: inherit;
            cursor: pointer;
            &:hover,
            &.active {
              border-color: #1677d2;
              background: #d6e9fb;
            }
            &:disabled {
              opacity: 0.35;
              cursor: default;
            }
          }
        }
      }
    }
  }
  .toolbar-group {
    display: flex;
    align-items: center;
    gap: 3px;
    padding-right: 5px;
    border-right: 1px solid #c8d0d8;
    &:last-child {
      padding-right: 0;
      border-right: 0;
    }
  }
  .toolbar-label {
    margin-right: 2px;
    color: #69717a;
    font-size: var(--font-size-xs);
  }
  .toolbar-button,
  .toolbar-color,
  .toolbar-color-preset,
  .toolbar-separator {
    flex-shrink: 0;
  }
  .toolbar-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 29px;
    height: 29px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    &:hover,
    &.active {
      border-color: #1677d2;
      background: #1677d2;
      color: #fff;
    }
    &:disabled {
      opacity: 0.35;
      cursor: default;
    }
  }
  .toolbar-color {
    width: 23px;
    height: 23px;
    padding: 1px;
    border: 1px solid #8f9aa7;
    background: transparent;
  }
  .toolbar-color-preset {
    width: 15px;
    height: 15px;
    padding: 0;
    border: 1px solid #7f8994;
    border-radius: 0;
    cursor: pointer;
    &:hover,
    &.active {
      outline: 2px solid #38bdf8;
      outline-offset: 1px;
    }
    &:disabled {
      opacity: 0.35;
      cursor: default;
    }
  }
  .toolbar-separator {
    width: 1px;
    height: 23px;
    margin: 0 2px;
    background: #b8c1cb;
  }
}
</style>

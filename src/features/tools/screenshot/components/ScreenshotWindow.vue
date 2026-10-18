<template>
  <div
    v-if="mode === 'capture'"
    ref="desktop"
    class="capture-window"
    @pointerdown="startSelection"
    @pointermove="movePointer"
    @pointerup="finishSelection"
    @pointercancel="cancelDrag"
    @contextmenu.prevent="reselect"
  >
    <img
      v-if="frame"
      :src="frame.source"
      class="capture-background"
      draggable="false"
    />
    <div v-if="!selection" class="capture-instructions">
      拖动选择截图区域 · Esc 取消 · C 复制当前像素颜色
    </div>
    <div v-if="selection" class="capture-selection" :style="selectionStyle">
      <ImageEditor
        v-if="editorSource && !drag"
        :source="editorSource"
        :toolbar-style="toolbarStyle"
        :busy="busy"
        reselect
        @output="output"
        @cancel="cancel"
        @error="error = $event"
        @reselect="reselect"
        @selection-start="startMove"
      />
      <div class="selection-size">
        {{ selection.width }} × {{ selection.height }}
      </div>
      <button
        v-for="handle in handles"
        :key="handle"
        class="selection-handle"
        :class="`handle-${handle}`"
        :aria-label="`调整选区 ${handle}`"
        @pointerdown.stop="startResize($event, handle)"
      />
    </div>
    <div
      v-if="frame && pointer && !editorSource"
      class="capture-magnifier"
      :style="magnifierStyle"
    >
      <canvas
        ref="magnifier"
        class="magnifier-canvas"
        width="132"
        height="88"
      />
      <span class="magnifier-text"
        >{{ pixelColor }} · {{ Math.floor(pointer.x) + frame.left }},
        {{ Math.floor(pointer.y) + frame.top }}</span
      >
    </div>
    <div v-if="error" class="capture-error" role="alert">{{ error }}</div>
  </div>
  <div
    v-else
    class="pin-window"
    :style="{ opacity }"
    @pointerdown="dragPin"
    @wheel.prevent="zoomPin"
    @contextmenu.prevent="menu = !menu"
    @dblclick="!editing && pinAction('edit')"
  >
    <ImageEditor
      v-if="editing && document"
      :source="document.source"
      :initial-annotations="document.annotations"
      :busy="busy"
      @output="output"
      @cancel="stopEditing"
      @error="error = $event"
    />
    <img
      v-else-if="document"
      :src="document.output"
      class="pin-image"
      draggable="false"
    />
    <div
      v-if="menu && !editing"
      class="pin-menu"
      @pointerdown.stop
      @dblclick.stop
    >
      <button
        v-for="action in pinActions"
        :key="action.id"
        class="pin-menu-button"
        type="button"
        @click="pinAction(action.id)"
      >
        {{ action.label }}
      </button>
      <label class="pin-opacity"
        >透明度
        <input
          class="opacity-input"
          type="range"
          min="0.15"
          max="1"
          step="0.05"
          :value="opacity"
          @input="setOpacity"
      /></label>
      <span class="pin-help">滚轮缩放 · 拖动移动 · 右键菜单</span>
    </div>
    <div v-if="error" class="pin-error" role="alert" @pointerdown.stop>
      {{ error }}
    </div>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue"
import { screenshotApi } from "@/api/modules/screenshot"
import { subscribeReady } from "@/api/request"
import ImageEditor from "./ImageEditor.vue"
import { loadImage, selectionRect } from "../renderer"

const query = new URLSearchParams(location.search)
const mode = query.get("mode")
const id = query.get("id")
const frame = ref(null)
const desktop = ref(null)
const selection = ref(null)
const editorSource = ref("")
const pointer = ref(null)
const magnifier = ref(null)
const pixelColor = ref("#000000")
const busy = ref(false)
const error = ref("")
const document = ref(null)
const editing = ref(false)
const menu = ref(false)
const opacity = ref(1)
const drag = ref(null)
const handles = ["nw", "n", "ne", "e", "se", "s", "sw", "w"]
const pinActions = [
  { id: "copy", label: "复制图片" },
  { id: "export", label: "保存 PNG" },
  { id: "edit", label: "编辑标注" },
  { id: "hide", label: "隐藏" },
  { id: "transparent", label: "鼠标穿透（托盘可取消）" },
  { id: "close", label: "关闭（可从历史恢复）" },
  { id: "destroy", label: "销毁贴图（保留截图历史）" }
]
let originalImage
let pixelCanvas
let unlisten
let resizeFrame
let pinWidth = 0
let pinHeight = 0
let currentZoom = 1
let lastViewport
let savedOutput = null
const selectionStyle = computed(() => {
  const rect = selection.value
  return {
    left: `${(rect.x / frame.value.width) * 100}%`,
    top: `${(rect.y / frame.value.height) * 100}%`,
    width: `${(rect.width / frame.value.width) * 100}%`,
    height: `${(rect.height / frame.value.height) * 100}%`
  }
})
const toolbarStyle = computed(() => {
  const r = selection.value
  const x = (r.x / frame.value.width) * innerWidth
  const y = ((r.y + r.height) / frame.value.height) * innerHeight
  return {
    left: `${Math.max(4, Math.min(x, innerWidth - 640))}px`,
    top: `${Math.max(4, Math.min(y + 10, innerHeight - 58))}px`,
    maxWidth: "calc(100vw - 8px)"
  }
})
const magnifierStyle = computed(() => ({
  left: `${Math.max(0, Math.min(pointer.value.clientX + 22, innerWidth - 240))}px`,
  top: `${Math.max(0, Math.min(pointer.value.clientY + 22, innerHeight - 125))}px`
}))

function desktopPoint(event) {
  const bounds = desktop.value.getBoundingClientRect()
  return {
    // 保留浮点指针，选区边界统一由 selectionRect 执行 floor/ceil。
    x: Math.max(
      0,
      Math.min(
        frame.value.width,
        ((event.clientX - bounds.left) * frame.value.width) / bounds.width
      )
    ),
    y: Math.max(
      0,
      Math.min(
        frame.value.height,
        ((event.clientY - bounds.top) * frame.value.height) / bounds.height
      )
    ),
    clientX: event.clientX,
    clientY: event.clientY
  }
}

function startSelection(event) {
  if (!frame.value || busy.value || event.button !== 0) return
  if (selection.value && editorSource.value) return
  desktop.value.setPointerCapture(event.pointerId)
  const start = desktopPoint(event)
  drag.value = { kind: "select", start }
  selection.value = selectionRect(
    start,
    start,
    frame.value.width,
    frame.value.height
  )
}

function startMove(event, hasAnnotations) {
  if (
    busy.value ||
    (hasAnnotations && !window.confirm("移动选区会清除当前标注，继续移动？"))
  )
    return
  desktop.value.setPointerCapture(event.pointerId)
  drag.value = {
    kind: "move",
    start: desktopPoint(event),
    original: { ...selection.value }
  }
  editorSource.value = ""
}

function startResize(event, handle) {
  if (busy.value) return
  if (
    editorSource.value &&
    !window.confirm("调整区域会清除当前标注，继续调整？")
  )
    return
  desktop.value.setPointerCapture(event.pointerId)
  drag.value = { kind: "resize", handle, original: { ...selection.value } }
  editorSource.value = ""
}

async function movePointer(event) {
  if (!frame.value) return
  const point = desktopPoint(event)
  pointer.value = point
  if (drag.value) {
    if (drag.value.kind === "select")
      selection.value = selectionRect(
        drag.value.start,
        point,
        frame.value.width,
        frame.value.height
      )
    else if (drag.value.kind === "move") {
      const { start, original: r } = drag.value
      selection.value = {
        ...r,
        x: Math.max(
          0,
            Math.min(frame.value.width - r.width, Math.round(r.x + point.x - start.x))
        ),
        y: Math.max(
          0,
            Math.min(frame.value.height - r.height, Math.round(r.y + point.y - start.y))
        )
      }
    } else {
      const { original: r, handle } = drag.value
      const start = {
        x: handle.includes("w") ? point.x : r.x,
        y: handle.includes("n") ? point.y : r.y
      }
      const end = {
        x: handle.includes("e") ? point.x : r.x + r.width,
        y: handle.includes("s") ? point.y : r.y + r.height
      }
      selection.value = selectionRect(
        start,
        end,
        frame.value.width,
        frame.value.height
      )
    }
  }
  if (pixelCanvas && !editorSource.value) {
    const data = pixelCanvas
      .getContext("2d")
      .getImageData(
        Math.min(point.x, frame.value.width - 1),
        Math.min(point.y, frame.value.height - 1),
        1,
        1
      ).data
    pixelColor.value = `#${[...data]
      .slice(0, 3)
      .map(channel => channel.toString(16).padStart(2, "0"))
      .join("")
      .toUpperCase()}`
    await nextTick()
    if (magnifier.value) {
      const ctx = magnifier.value.getContext("2d")
      ctx.clearRect(0, 0, 132, 88)
      ctx.imageSmoothingEnabled = false
      ctx.drawImage(
        pixelCanvas,
        point.x - 16,
        point.y - 11,
        33,
        22,
        0,
        0,
        132,
        88
      )
      ctx.strokeStyle = "#38bdf8"
      ctx.strokeRect(64, 40, 4, 4)
    }
  }
}

function finishSelection() {
  if (!drag.value || !selection.value) return
  drag.value = null
  const rect = selection.value
  const canvas = window.document.createElement("canvas")
  canvas.width = rect.width
  canvas.height = rect.height
  canvas
    .getContext("2d")
    .drawImage(
      originalImage,
      rect.x,
      rect.y,
      rect.width,
      rect.height,
      0,
      0,
      rect.width,
      rect.height
    )
  editorSource.value = canvas.toDataURL("image/png")
}

function cancelDrag() {
  drag.value = null
  selection.value = null
  editorSource.value = ""
}

function reselect(hasAnnotations = true) {
  if (
    busy.value ||
    (editorSource.value &&
      hasAnnotations &&
      !window.confirm("重新选择会清除当前标注，继续选择？"))
  )
    return
  cancelDrag()
}

async function cancel() {
  if (busy.value) return
  try {
    await screenshotApi.call("cancel")
  } catch (cause) {
    error.value = String(cause)
  }
}

async function output(data) {
  if (busy.value) return
  busy.value = true
  error.value = ""
  try {
    if (savedOutput?.output !== data.output) {
      const newId = await screenshotApi.call("save-document", {
        source: data.source,
        output: data.output,
        annotations: data.annotations
      })
      savedOutput = { id: newId, output: data.output }
    }
    const newId = savedOutput.id
    if (data.action === "copy") await screenshotApi.call("copy", { id: newId })
    if (data.action === "export") {
      const result = await screenshotApi.call("export", { id: newId })
      if (!result.saved) return
    }
    if (data.action === "pin") await screenshotApi.call("pin", { id: newId })
    if (mode === "capture") await screenshotApi.call("cancel")
    else {
      await stopEditing()
      if (data.action !== "pin") await screenshotApi.call("pin", { id: newId })
      await screenshotApi.call("pin-action", { id, action: "close" })
    }
  } catch (cause) {
    error.value = `操作失败：${String(cause)}`
  } finally {
    busy.value = false
  }
}

async function pinAction(action) {
  menu.value = false
  error.value = ""
  try {
    if (action === "copy" || action === "export")
      await screenshotApi.call(action, { id })
    else if (action === "edit") {
      editing.value = true
      await screenshotApi.call("pin-action", { id, action: "edit" })
    } else await screenshotApi.call("pin-action", { id, action })
  } catch (cause) {
    error.value = String(cause)
  }
}

async function stopEditing() {
  editing.value = false
  await screenshotApi.call("pin-action", { id, action: "display" })
}

async function dragPin(event) {
  if (event.button !== 0 || editing.value || menu.value) return
  try {
    await screenshotApi.call("pin-action", { id, action: "drag" })
  } catch (cause) {
    error.value = String(cause)
  }
}

function zoomPin(event) {
  if (editing.value || menu.value) return
  currentZoom = Math.max(
    0.1,
    Math.min(8, currentZoom * Math.exp(-event.deltaY * 0.001))
  )
  if (resizeFrame) cancelAnimationFrame(resizeFrame)
  const anchorX = event.clientX / innerWidth
  const anchorY = event.clientY / innerHeight
  resizeFrame = requestAnimationFrame(async () => {
    try {
      await screenshotApi.call("pin-action", {
        id,
        action: "resize",
        width: pinWidth * currentZoom,
        height: pinHeight * currentZoom,
        anchorX,
        anchorY
      })
    } catch (cause) {
      error.value = String(cause)
    }
  })
}

async function setOpacity(event) {
  opacity.value = Number(event.target.value)
  try {
    await screenshotApi.call("pin-action", {
      id,
      action: "opacity",
      opacity: opacity.value
    })
  } catch (cause) {
    error.value = String(cause)
  }
}

async function keydown(event) {
  if (
    event.isComposing ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(event.target.tagName)
  )
    return
  if (event.key === "Escape")
    mode === "capture"
      ? await cancel()
      : editing.value
        ? await stopEditing()
        : await pinAction("close")
  if (
    mode === "capture" &&
    !editorSource.value &&
    event.key.toLowerCase() === "c"
  ) {
    try {
      await navigator.clipboard.writeText(pixelColor.value)
    } catch (cause) {
      error.value = String(cause)
    }
  }
}

function checkViewport() {
  // 采集期间屏幕布局变化时终止本次会话，不继续使用旧的像素映射。
  const size = `${innerWidth}:${innerHeight}:${devicePixelRatio}`
  if (mode === "capture" && lastViewport && size !== lastViewport) cancel()
  lastViewport = size
}

onMounted(async () => {
  window.addEventListener("keydown", keydown)
  window.addEventListener("resize", checkViewport)
  try {
    if (mode === "capture") {
      frame.value = await screenshotApi.call("frame")
      originalImage = await loadImage(frame.value.source)
      pixelCanvas = window.document.createElement("canvas")
      pixelCanvas.width = frame.value.width
      pixelCanvas.height = frame.value.height
      pixelCanvas
        .getContext("2d", { willReadFrequently: true })
        .drawImage(originalImage, 0, 0)
      await screenshotApi.call("ready")
      checkViewport()
    } else {
      document.value = await screenshotApi.call("document", { id })
      opacity.value = document.value.pin?.opacity ?? 1
      pinWidth = document.value.pin.width
      pinHeight = document.value.pin.height
      unlisten = await subscribeReady("screenshot:opacity", value => {
        opacity.value = value
      })
    }
  } catch (cause) {
    error.value = String(cause)
  }
})
onBeforeUnmount(() => {
  window.removeEventListener("keydown", keydown)
  window.removeEventListener("resize", checkViewport)
  unlisten?.()
  if (resizeFrame) cancelAnimationFrame(resizeFrame)
})
</script>

<style scoped lang="less">
.capture-window {
  position: fixed;
  inset: 0;
  overflow: hidden;
  cursor: crosshair;
  touch-action: none;
  background: #000;
  user-select: none;
  .capture-background {
    position: absolute;
    width: 100%;
    height: 100%;
    object-fit: fill;
    pointer-events: none;
  }
  .capture-instructions {
    position: absolute;
    top: 24px;
    left: 50%;
    transform: translateX(-50%);
    padding: 10px 20px;
    border-radius: 8px;
    color: #fff;
    background: #17212bec;
    font-size: var(--font-size-sm);
    white-space: nowrap;
  }
  .capture-selection {
    position: absolute;
    outline: 1px solid #38bdf8;
    box-shadow: 0 0 0 10000px #0007;
    .selection-size {
      position: absolute;
      top: 0;
      left: 0;
      padding: 2px 7px;
      color: #fff;
      background: #17212be6;
      font-size: var(--font-size-xs);
      pointer-events: none;
      transform: translateY(-100%);
      white-space: nowrap;
    }
    .selection-handle {
      position: absolute;
      width: 8px;
      height: 8px;
      padding: 0;
      border: 1px solid #0e7490;
      background: #fff;
      transform: translate(-50%, -50%);
      z-index: 5;
      &.handle-nw {
        left: 0;
        top: 0;
        cursor: nwse-resize;
      }
      &.handle-n {
        left: 50%;
        top: 0;
        cursor: ns-resize;
      }
      &.handle-ne {
        left: 100%;
        top: 0;
        cursor: nesw-resize;
      }
      &.handle-e {
        left: 100%;
        top: 50%;
        cursor: ew-resize;
      }
      &.handle-se {
        left: 100%;
        top: 100%;
        cursor: nwse-resize;
      }
      &.handle-s {
        left: 50%;
        top: 100%;
        cursor: ns-resize;
      }
      &.handle-sw {
        left: 0;
        top: 100%;
        cursor: nesw-resize;
      }
      &.handle-w {
        left: 0;
        top: 50%;
        cursor: ew-resize;
      }
    }
  }
  .capture-magnifier {
    position: fixed;
    display: flex;
    flex-direction: column;
    align-items: center;
    border: 1px solid #59718a;
    border-radius: 5px;
    background: #17212b;
    color: #fff;
    pointer-events: none;
    .magnifier-canvas {
      width: 132px;
      height: 88px;
    }
    .magnifier-text {
      padding: 4px 8px;
      font-size: var(--font-size-xs);
    }
  }
  .capture-error {
    position: absolute;
    top: 70px;
    left: 20px;
    padding: 10px;
    border-radius: 6px;
    background: #7f1d1d;
    color: #fff;
    font-size: var(--font-size-sm);
  }
}
.pin-window {
  position: fixed;
  inset: 0;
  overflow: hidden;
  user-select: none;
  .pin-image {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    cursor: move;
  }
  .pin-menu {
    position: absolute;
    inset: 4px auto auto 4px;
    z-index: 10;
    display: flex;
    flex-direction: column;
    padding: 5px;
    border: 1px solid #526479;
    border-radius: 7px;
    background: #17212bf5;
    color: #fff;
    max-height: calc(100% - 8px);
    overflow: auto;
    .pin-menu-button {
      padding: 6px 10px;
      border: 0;
      border-radius: 4px;
      background: transparent;
      color: inherit;
      text-align: left;
      white-space: nowrap;
      cursor: pointer;
      font-size: var(--font-size-sm);
      &:hover {
        background: #33556d;
      }
    }
    .pin-opacity {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 6px 10px;
      font-size: var(--font-size-xs);
      .opacity-input {
        width: 100px;
      }
    }
    .pin-help {
      padding: 4px 10px;
      color: #93a2b3;
      font-size: var(--font-size-xs);
    }
  }
  .pin-error {
    position: absolute;
    top: 0;
    left: 0;
    padding: 6px;
    background: #7f1d1d;
    color: #fff;
    font-size: var(--font-size-xs);
  }
}
</style>

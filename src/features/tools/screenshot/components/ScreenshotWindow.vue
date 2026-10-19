<template>
  <div
    v-if="mode === 'capture'"
    ref="desktop"
    class="capture-window"
    :style="{
      cursor: drag?.kind === 'resize' ? resizeCursors[drag.handle] : undefined
    }"
    @pointerdown="startSelection"
    @pointermove="movePointer"
    @pointerup="finishSelection"
    @pointercancel="cancelDrag"
    @contextmenu.prevent="reselect"
  >
    <canvas
      ref="captureBackground"
      width="1"
      height="1"
      class="capture-background"
    />
    <div v-if="frame && !selection" class="capture-instructions">
      拖动选择截图区域 · Esc 取消 · C 复制当前像素颜色
    </div>
    <div
      v-if="frame && selection"
      class="capture-selection"
      :class="{ 'is-resizing': drag?.kind === 'resize' }"
      :style="selectionStyle"
    >
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
        @toolbar-hover="toolbarHovered = $event"
      />
      <div class="selection-size">
        {{ selection.width }} × {{ selection.height }}
      </div>
      <button
        v-for="edge in ['n', 'e', 's', 'w']"
        :key="`edge-${edge}`"
        type="button"
        class="selection-edge"
        :class="`edge-${edge}`"
        :style="{ cursor: resizeCursors[edge] }"
        :aria-label="`拖动选区边框 ${edge}`"
        @pointerenter="resizeHovered = true"
        @pointerleave="resizeHovered = false"
        @pointerdown.stop="startResize($event, edge)"
      />
      <button
        v-for="handle in handles"
        :key="handle"
        type="button"
        class="selection-handle"
        :class="`handle-${handle}`"
        :style="{ cursor: resizeCursors[handle] }"
        :aria-label="`调整选区 ${handle}`"
        @pointerenter="resizeHovered = true"
        @pointerleave="resizeHovered = false"
        @pointerdown.stop="startResize($event, handle)"
      />
    </div>
    <Teleport to="body">
      <div
        v-if="frame && pointer"
        v-show="magnifierVisible"
        class="capture-magnifier"
        :style="magnifierStyle"
      >
        <div class="magnifier-preview">
          <canvas
            ref="magnifier"
            class="magnifier-canvas"
            width="168"
            height="120"
          />
          <span class="magnifier-crosshair" />
          <span class="magnifier-pixel" :style="{ background: pixelColor }" />
        </div>
        <span class="magnifier-coordinates"
          >({{ Math.min(frame.width - 1, Math.floor(pointer.x)) + frame.left }},
          {{
            Math.min(frame.height - 1, Math.floor(pointer.y)) + frame.top
          }})</span
        >
        <div class="magnifier-color">
          <span class="magnifier-swatch" :style="{ background: pixelColor }" />
          <span class="magnifier-value">{{ colorValue }}</span>
        </div>
        <span class="magnifier-hint">按 C 复制颜色值</span>
        <span class="magnifier-hint">按 Shift 切换 RGB/HEX</span>
      </div>
    </Teleport>
    <div v-if="error" class="capture-error" role="alert">{{ error }}</div>
  </div>
  <div
    v-else
    class="pin-window"
    :class="pinWindowClass"
    :style="{ opacity: pinReady ? opacity : 0 }"
    @pointerdown="dragPin"
    @pointermove="movePin"
    @pointerup="cancelPinDrag"
    @pointercancel="cancelPinDrag"
    @wheel.prevent="zoomPin"
    @contextmenu.prevent="openPinMenu"
    @dblclick="!editing && pinAction('destroy')"
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
      ref="pinImage"
      :src="document.output"
      class="pin-image"
      :style="{
        width: menuExpanded ? `${menuSize.width}px` : '100%',
        height: menuExpanded ? `${menuSize.height}px` : '100%',
        imageRendering: smoothScaling ? 'auto' : 'pixelated'
      }"
      draggable="false"
    />
    <div
      v-if="menu && !editing"
      ref="pinMenu"
      class="pin-menu"
      :style="{ left: `${menuPosition.x}px`, top: `${menuPosition.y}px` }"
      @pointerdown.stop
      @dblclick.stop
      @pointerleave="submenu = ''"
    >
      <button class="pin-menu-button" type="button" @click="pinAction('copy')">
        复制图像
      </button>
      <button
        class="pin-menu-button"
        type="button"
        @click="pinAction('export')"
      >
        图像另存为...
      </button>
      <span class="pin-menu-separator" />
      <button
        class="pin-menu-button"
        type="button"
        @click="toolbarVisible = !toolbarVisible"
      >
        <span class="pin-menu-check">{{ toolbarVisible ? "✓" : "" }}</span>
        显示工具条
      </button>
      <button
        class="pin-menu-button"
        type="button"
        aria-label="窗口阴影"
        :aria-pressed="windowShadow"
        @click="windowShadow = !windowShadow"
      >
        <span class="pin-menu-check">{{ windowShadow ? "✓" : "" }}</span>
        窗口阴影
      </button>
      <span class="pin-menu-separator" />
      <div class="pin-menu-parent" @pointerenter="submenu = 'zoom'">
        <button class="pin-menu-button" type="button">
          <span class="pin-menu-check" />
          缩放 <span class="pin-menu-arrow">›</span>
        </button>
        <div
          v-if="submenu === 'zoom'"
          class="pin-submenu"
          :class="{ 'submenu-left': submenuSide === 'left' }"
        >
          <button
            v-for="item in zoomOptions"
            :key="item.value"
            class="pin-menu-button"
            type="button"
            @click="setZoom(item.value)"
          >
            {{ item.label }}
          </button>
          <span class="pin-menu-separator" />
          <button class="pin-menu-button" type="button" @click="setZoom(1)">
            <span class="pin-menu-check">{{
              isCurrentZoom(1) ? "✓" : ""
            }}</span>
            100% <span class="pin-menu-note">当前</span>
          </button>
          <button
            class="pin-menu-button"
            type="button"
            @click="smoothScaling = !smoothScaling"
          >
            <span class="pin-menu-check">{{ smoothScaling ? "✓" : "" }}</span>
            平滑处理
          </button>
        </div>
      </div>
      <div class="pin-menu-parent" @pointerenter="submenu = 'processing'">
        <button class="pin-menu-button" type="button">
          <span class="pin-menu-check" />
          图像处理 <span class="pin-menu-arrow">›</span>
        </button>
        <div
          v-if="submenu === 'processing'"
          class="pin-submenu"
          :class="{ 'submenu-left': submenuSide === 'left' }"
        >
          <button
            class="pin-menu-button"
            type="button"
            @click="processImage('rotate-left')"
          >
            向左旋转
          </button>
          <button
            class="pin-menu-button"
            type="button"
            @click="processImage('rotate-right')"
          >
            向右旋转
          </button>
          <button
            class="pin-menu-button"
            type="button"
            @click="processImage('flip-horizontal')"
          >
            水平翻转
          </button>
          <button
            class="pin-menu-button"
            type="button"
            @click="processImage('flip-vertical')"
          >
            垂直翻转
          </button>
        </div>
      </div>
      <span class="pin-menu-separator" />
      <button class="pin-menu-button" type="button" @click="pinAction('paste')">
        粘贴
      </button>
      <button class="pin-menu-button" type="button" @click="replaceImage">
        替换为文件...
      </button>
      <div class="pin-menu-parent" @pointerenter="submenu = 'group'">
        <button class="pin-menu-button" type="button">
          <span class="pin-menu-check" />
          移动到分组 <span class="pin-menu-arrow">›</span>
        </button>
        <div
          v-if="submenu === 'group'"
          class="pin-submenu"
          :class="{ 'submenu-left': submenuSide === 'left' }"
        >
          <button
            v-for="group in pinGroups"
            :key="group"
            class="pin-menu-button"
            type="button"
            @click="moveToGroup(group)"
          >
            <span class="pin-menu-check">{{
              pinGroup === group ? "✓" : ""
            }}</span>
            {{ group }}
          </button>
          <span class="pin-menu-separator" />
          <button class="pin-menu-button" type="button" @click="createGroup">
            <span class="pin-menu-check" />
            新分组...
          </button>
        </div>
      </div>
      <button class="pin-menu-button" type="button" @click="openSourceFolder">
        在文件夹中查看
      </button>
      <span class="pin-menu-separator" />
      <button class="pin-menu-button" type="button" @click="pinAction('close')">
        关闭
      </button>
      <button
        class="pin-menu-button"
        type="button"
        @click="pinAction('destroy')"
      >
        销毁
      </button>
      <span class="pin-menu-separator" />
      <div class="pin-menu-parent" @pointerenter="submenu = 'info'">
        <button class="pin-menu-button pin-menu-size" type="button">
          {{ document ? document.width : 0 }} ×
          {{ document ? document.height : 0 }}
          <span class="pin-menu-arrow">›</span>
        </button>
        <div
          v-if="submenu === 'info'"
          class="pin-submenu pin-info-submenu"
          :class="{ 'submenu-left': submenuSide === 'left' }"
        >
          <span>所在组: {{ pinGroup }}</span>
          <span>缩放: {{ Math.round(currentZoom * 1000) / 10 }}%</span>
          <span>旋转: 0°</span>
          <label class="pin-info-opacity">
            不透明度: {{ Math.round(opacity * 100) }}%
            <input
              class="opacity-input"
              type="range"
              min="0.15"
              max="1"
              step="0.05"
              :value="opacity"
              @input="setOpacity"
            />
          </label>
          <span>水平镜像: 否</span>
          <span>垂直镜像: 否</span>
          <span>灰度显示: 否</span>
          <span>反色显示: 否</span>
        </div>
      </div>
    </div>
    <div
      v-if="toolbarVisible && !editing"
      class="pin-toolbar"
      @pointerdown.stop
    >
      <button
        class="pin-toolbar-button"
        type="button"
        @click="pinAction('copy')"
      >
        复制
      </button>
      <button
        class="pin-toolbar-button"
        type="button"
        @click="pinAction('export')"
      >
        保存
      </button>
      <button
        class="pin-toolbar-button"
        type="button"
        @click="pinAction('edit')"
      >
        编辑
      </button>
      <button
        class="pin-toolbar-button"
        type="button"
        @click="setZoom(currentZoom === 1 ? 2 : 1)"
      >
        缩放 {{ Math.round(currentZoom * 1000) / 10 }}%
      </button>
    </div>
    <div v-if="error" class="pin-error" role="alert" @pointerdown.stop>
      {{ error }}
    </div>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue"
import { screenshotApi } from "@/api/modules/screenshot"
import { systemApi } from "@/api/modules/system"
import { subscribeReady } from "@/api/request"
import ImageEditor from "./ImageEditor.vue"
import { canvasSource, selectionRect, thumbnailSource } from "../renderer"

const query = new URLSearchParams(location.search)
const mode = query.get("mode")
const id = query.get("id")
const dismissCapture = query.get("capture") === "1"
const frame = ref(null)
const autoCopy = ref(false)
const desktop = ref(null)
const captureBackground = ref(null)
const pinImage = ref(null)
const selection = ref(null)
const editorSource = ref("")
const pointer = ref(null)
const magnifier = ref(null)
const toolbarHovered = ref(false)
const resizeHovered = ref(false)
const pixelColor = ref("#000000")
// 取色模式每次截图默认使用 RGB，显示和复制始终共享同一个格式值。
const colorFormat = ref("rgb")
const colorValue = computed(() =>
  colorFormat.value === "hex"
    ? pixelColor.value
    : [1, 3, 5]
        .map((offset) =>
          parseInt(pixelColor.value.slice(offset, offset + 2), 16)
        )
        .join(", ")
)
const busy = ref(false)
const error = ref("")
const document = ref(null)
const editing = ref(false)
const menu = ref(false)
const menuExpanded = ref(false)
const submenu = ref("")
const submenuSide = ref("right")
const pinMenu = ref(null)
const menuPosition = ref({ x: 0, y: 0 })
const menuSize = ref({ width: 0, height: 0 })
const opacity = ref(1)
const pinReady = ref(mode !== "pin")
const windowShadow = ref(true)
const toolbarVisible = ref(false)
const smoothScaling = ref(true)
const pinGroup = ref("Default")
const pinGroups = ref(["Default"])
const drag = ref(null)
let selectionGeneration = 0
let documentGeneration = 0
let disposed = false
let cancelPinPaint = null
// 边框、控制点和拖动期间使用相同的方向光标，取色仅在画面区域显示。
const resizeCursors = {
  nw: "nwse-resize",
  n: "ns-resize",
  ne: "nesw-resize",
  e: "ew-resize",
  se: "nwse-resize",
  s: "ns-resize",
  sw: "nesw-resize",
  w: "ew-resize"
}
const handles = Object.keys(resizeCursors)
const magnifierVisible = computed(
  () =>
    !toolbarHovered.value &&
    !resizeHovered.value &&
    drag.value?.kind !== "resize"
)
let originalImage
let pixelCanvas
let unlisten
let unlistenFrame
let unlistenReset
let frameGeneration = 0
let resizeFrame
let pinWidth = 0
let pinDragStart = null
let pinHeight = 0
let imageWidth = 0
let imageHeight = 0
const currentZoom = ref(1)
let lastViewport
let savedOutput = null
const zoomOptions = [
  { label: "33.3%", value: 0.333 },
  { label: "50%", value: 0.5 },
  { label: "100%", value: 1 },
  { label: "200%", value: 2 }
]
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
  const bounds = desktop.value?.getBoundingClientRect()
  const scaleX = bounds
    ? bounds.width / frame.value.width
    : innerWidth / frame.value.width
  const scaleY = bounds
    ? bounds.height / frame.value.height
    : innerHeight / frame.value.height
  const right = (bounds?.left ?? 0) + (r.x + r.width) * scaleX
  const bottom = (bounds?.top ?? 0) + (r.y + r.height) * scaleY
  const toolbarHeight = 82
  const gap = 8
  const top = bottom + gap
  const fallbackTop = (bounds?.top ?? 0) + r.y * scaleY - toolbarHeight - gap
  return {
    // 用 right 锚定，工具栏实际宽度变化时仍能和选区右边缘对齐。
    right: `${Math.max(4, Math.min(innerWidth - right, innerWidth - 4))}px`,
    top: `${Math.max(
      4,
      Math.min(
        top + toolbarHeight <= innerHeight - 4 ? top : fallbackTop,
        innerHeight - toolbarHeight - 4
      )
    )}px`,
    maxWidth: "calc(100vw - 8px)"
  }
})
const magnifierStyle = computed(() => {
  // 靠近屏幕边缘时翻到鼠标另一侧，保持预览和操作提示完整可见。
  const { clientX, clientY } = pointer.value
  const width = 186
  const height = 226
  const gap = 22
  return {
    left: `${Math.max(0, Math.min(clientX + gap + width <= innerWidth ? clientX + gap : clientX - width - gap, innerWidth - width))}px`,
    top: `${Math.max(0, Math.min(clientY + gap + height <= innerHeight ? clientY + gap : clientY - height - gap, innerHeight - height))}px`
  }
})
// 阴影开关同时控制边框颜色，保留占位避免图片尺寸跳动。
const pinWindowClass = computed(() => ({
  "has-shadow": windowShadow.value,
  "has-border": windowShadow.value
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
  selectionGeneration += 1
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
  selectionGeneration += 1
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
  selectionGeneration += 1
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
          Math.min(
            frame.value.width - r.width,
            Math.round(r.x + point.x - start.x)
          )
        ),
        y: Math.max(
          0,
          Math.min(
            frame.value.height - r.height,
            Math.round(r.y + point.y - start.y)
          )
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
  if (!magnifierVisible.value) return
  if (pixelCanvas) {
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
      .map((channel) => channel.toString(16).padStart(2, "0"))
      .join("")
      .toUpperCase()}`
    // 放大镜已挂载后直接同步绘制，避免每次移动都等待 Vue 调度。
    if (!magnifier.value) await nextTick()
    if (magnifier.value) {
      const ctx = magnifier.value.getContext("2d")
      ctx.clearRect(0, 0, 168, 120)
      ctx.imageSmoothingEnabled = false
      ctx.drawImage(
        pixelCanvas,
        Math.min(frame.value.width - 1, Math.floor(point.x)) - 10,
        Math.min(frame.value.height - 1, Math.floor(point.y)) - 7,
        21,
        15,
        0,
        0,
        168,
        120
      )
    }
  }
}

async function finishSelection() {
  if (!drag.value || !selection.value) return
  drag.value = null
  const generation = ++selectionGeneration
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
  try {
    const [source, thumbnail] = await Promise.all([
      canvasSource(canvas),
      autoCopy.value ? thumbnailSource(canvas) : null
    ])
    // 大图编码期间可能已取消或重新框选，旧结果不能覆盖新选区。
    if (disposed || generation !== selectionGeneration) return
    if (autoCopy.value) {
      await output({
        action: "copy",
        source,
        output: source,
        thumbnail,
        annotations: []
      })
      return
    }
    editorSource.value = source
  } catch (cause) {
    if (generation === selectionGeneration) error.value = String(cause)
  } finally {
    canvas.width = 0
    canvas.height = 0
  }
}

function cancelDrag() {
  selectionGeneration += 1
  drag.value = null
  resizeHovered.value = false
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
        output: data.output === data.source ? null : data.output,
        thumbnail: data.thumbnail,
        annotations: data.annotations,
        editingPinId: mode === "capture" ? null : id,
        origin:
          mode === "capture"
            ? {
                x: frame.value.left + selection.value.x,
                y: frame.value.top + selection.value.y
              }
            : { x: document.value.pin.x, y: document.value.pin.y }
      })
      savedOutput = { id: newId, output: data.output }
    }
    const newId = savedOutput.id
    if (data.action === "copy") await screenshotApi.call("copy", { id: newId })
    if (data.action === "export") {
      const result = await screenshotApi.call("export", { id: newId })
      if (!result.saved) return
    }
    if (data.action === "pin")
      await screenshotApi.call("pin", {
        id: newId,
        capture: mode === "capture"
      })
    if (mode === "capture") {
      if (data.action !== "pin") await screenshotApi.call("cancel")
    } else {
      if (data.action !== "pin") await screenshotApi.call("pin", { id: newId })
      await screenshotApi.call("pin-action", { id, action: "close" })
    }
  } catch (cause) {
    error.value = `操作失败：${String(cause)}`
  } finally {
    busy.value = false
  }
}

async function openPinMenu(event) {
  if (editing.value) return
  submenu.value = ""
  submenuSide.value = "right"
  menuPosition.value = { x: event.clientX, y: event.clientY }
  // 先扩展原生窗口，给菜单和子菜单预留可点击的透明区域。
  if (!menuExpanded.value) {
    const imageBounds = pinImage.value?.getBoundingClientRect()
    try {
      const originalSize = await screenshotApi.call("pin-action", {
        id,
        action: "context-menu-open",
        extraWidth: 560,
        extraHeight: 700
      })
      menuSize.value = {
        // 原生尺寸是物理像素，优先使用图片当前的 CSS 尺寸避免 DPI 缩放变形。
        width:
          imageBounds?.width ||
          (originalSize?.width || pinWidth) / (devicePixelRatio || 1),
        height:
          imageBounds?.height ||
          (originalSize?.height || pinHeight) / (devicePixelRatio || 1)
      }
      menuExpanded.value = true
    } catch (cause) {
      error.value = String(cause)
      return
    }
  }
  menu.value = true
  await nextTick()
  if (!pinMenu.value) return
  submenuSide.value =
    event.clientX + pinMenu.value.offsetWidth + 190 > innerWidth
      ? "left"
      : "right"
}

async function closePinMenu() {
  const expanded = menuExpanded.value
  menu.value = false
  submenu.value = ""
  if (!expanded) return
  // 菜单关闭后恢复原生窗口，避免临时尺寸进入贴图会话数据。
  try {
    await screenshotApi.call("pin-action", {
      id,
      action: "context-menu-close"
    })
  } catch (cause) {
    error.value = String(cause)
  } finally {
    menuExpanded.value = false
  }
}

async function pinAction(action) {
  await closePinMenu()
  error.value = ""
  try {
    if (action === "copy" || action === "export")
      await screenshotApi.call(action, { id })
    else if (action === "paste") await screenshotApi.call("restore-previous")
    else if (action === "edit") {
      if (busy.value) return
      busy.value = true
      try {
        // 显示贴图不读取原图，首次编辑时才加载可编辑源图。
        if (!document.value.source) {
          const generation = documentGeneration
          const bytes = await screenshotApi.image(id, "source")
          if (disposed || generation !== documentGeneration) return
          document.value.source = URL.createObjectURL(
            new Blob([bytes], { type: "image/png" })
          )
        }
        editing.value = true
        await screenshotApi.call("pin-action", { id, action: "edit" })
      } finally {
        busy.value = false
      }
    } else await screenshotApi.call("pin-action", { id, action })
  } catch (cause) {
    error.value = String(cause)
  }
}

function releaseDocumentImages(value) {
  for (const source of new Set([value?.source, value?.output])) {
    if (source) URL.revokeObjectURL(source)
  }
}

async function loadDocument() {
  const generation = ++documentGeneration
  const metadata = await screenshotApi.call("document", {
    id,
    metadataOnly: true
  })
  const bytes = await screenshotApi.image(id, "output")
  if (disposed || generation !== documentGeneration) return false
  const previous = document.value
  document.value = {
    ...metadata,
    source: "",
    output: URL.createObjectURL(new Blob([bytes], { type: "image/png" }))
  }
  await nextTick()
  releaseDocumentImages(previous)
  return true
}

function waitForPinPaint(size) {
  return new Promise((resolve, reject) => {
    let frame = 0
    let matching = 0
    const finish = (ready, cause) => {
      cancelAnimationFrame(frame)
      clearTimeout(timeout)
      cancelPinPaint = null
      if (cause) reject(cause)
      else resolve(ready)
    }
    const timeout = setTimeout(
      () => finish(false, new Error("贴图窗口尺寸未就绪，请重新贴图")),
      2000
    )
    cancelPinPaint = () => finish(false)
    const check = () => {
      if (disposed) return finish(false)
      const scale = devicePixelRatio || 1
      // 原生窗口是物理像素；等 WebView 的视口和 DPI 同步后再露出图片。
      const matches =
        Math.abs(innerWidth * scale - size.width) <= scale &&
        Math.abs(innerHeight * scale - size.height) <= scale
      matching = matches ? matching + 1 : 0
      if (matching >= 2) finish(true)
      else frame = requestAnimationFrame(check)
    }
    frame = requestAnimationFrame(check)
  })
}

function isCurrentZoom(value) {
  return Math.abs(currentZoom.value - value) < 0.01
}

// 缩放始终以原图尺寸为基准，避免连续缩放重复放大窗口。
function syncImageMetrics() {
  imageWidth = Number(document.value?.width) || pinWidth
  imageHeight = Number(document.value?.height) || pinHeight
  currentZoom.value = imageWidth ? pinWidth / imageWidth : 1
}

async function setZoom(value) {
  currentZoom.value = value
  await closePinMenu()
  try {
    await screenshotApi.call("pin-action", {
      id,
      action: "resize",
      width: Math.round(imageWidth * currentZoom.value),
      height: Math.round(imageHeight * currentZoom.value),
      anchorX: 0,
      anchorY: 0
    })
  } catch (cause) {
    error.value = String(cause)
  }
}

async function processImage(operation) {
  await closePinMenu()
  try {
    await screenshotApi.call("process-image", { id, operation })
    if (!(await loadDocument())) return
    pinWidth = document.value.pin.width
    pinHeight = document.value.pin.height
    syncImageMetrics()
  } catch (cause) {
    error.value = `图像处理失败：${String(cause)}`
  }
}

async function replaceImage() {
  await closePinMenu()
  try {
    const path = await systemApi.selectFile({
      title: "替换贴图文件",
      filters: [
        { name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }
      ]
    })
    if (!path) return
    await screenshotApi.call("replace-file", { id, path })
    if (!(await loadDocument())) return
    pinWidth = document.value.pin.width
    pinHeight = document.value.pin.height
    syncImageMetrics()
  } catch (cause) {
    error.value = `替换图片失败：${String(cause)}`
  }
}

async function moveToGroup(group) {
  pinGroup.value = group
  await closePinMenu()
  try {
    await screenshotApi.call("pin-action", { id, action: "group", group })
  } catch (cause) {
    error.value = String(cause)
  }
}

async function createGroup() {
  const group = window.prompt("新分组名称", "新分组")?.trim()
  if (!group || pinGroups.value.includes(group)) return
  pinGroups.value.push(group)
  await moveToGroup(group)
}

async function openSourceFolder() {
  await closePinMenu()
  try {
    await screenshotApi.call("open-folder", { id })
  } catch (cause) {
    error.value = `打开文件夹失败：${String(cause)}`
  }
}

async function stopEditing() {
  editing.value = false
  await screenshotApi.call("pin-action", { id, action: "display" })
}

function dragPin(event) {
  if (event.button !== 0 || event.detail > 1 || editing.value) return
  if (menu.value) {
    closePinMenu()
    return
  }
  pinDragStart = { x: event.clientX, y: event.clientY }
}

function cancelPinDrag() {
  pinDragStart = null
}

async function movePin(event) {
  if (!pinDragStart) return
  if (!(event.buttons & 1) || editing.value || menu.value) {
    cancelPinDrag()
    return
  }
  if (
    Math.hypot(event.clientX - pinDragStart.x, event.clientY - pinDragStart.y) <
    4
  )
    return
  cancelPinDrag()
  try {
    await screenshotApi.call("pin-action", { id, action: "drag" })
  } catch (cause) {
    error.value = String(cause)
  }
}

function zoomPin(event) {
  if (editing.value || menu.value) return
  currentZoom.value = Math.max(
    0.1,
    Math.min(8, currentZoom.value * Math.exp(-event.deltaY * 0.001))
  )
  if (resizeFrame) cancelAnimationFrame(resizeFrame)
  resizeFrame = requestAnimationFrame(async () => {
    try {
      await screenshotApi.call("pin-action", {
        id,
        action: "resize",
        width: Math.round(imageWidth * currentZoom.value),
        height: Math.round(imageHeight * currentZoom.value),
        anchorX: 0,
        anchorY: 0
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
  if (mode !== "capture" || !frame.value || !pointer.value) return
  // 松开框选后仍可切换和复制颜色；输入文字和带修饰键的编辑操作不受影响。
  if (
    event.key === "Shift" &&
    !event.repeat &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.metaKey
  ) {
    event.preventDefault()
    colorFormat.value = colorFormat.value === "rgb" ? "hex" : "rgb"
  }
  if (
    event.key.toLowerCase() === "c" &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.metaKey
  ) {
    event.preventDefault()
    try {
      await navigator.clipboard.writeText(colorValue.value)
    } catch (cause) {
      error.value = String(cause)
    }
  }
}

function checkViewport() {
  // 采集期间屏幕布局变化时终止本次会话，不继续使用旧的像素映射。
  const size = `${innerWidth}:${innerHeight}:${devicePixelRatio}`
  if (
    mode === "capture" &&
    frame.value &&
    lastViewport &&
    size !== lastViewport
  )
    cancel()
  lastViewport = size
}

function resetCapture() {
  frameGeneration += 1
  // 仅清空像素，保留画布缓冲区，下一次唤出不再反复缩小和扩容。
  if (captureBackground.value) {
    const background = captureBackground.value
    background
      .getContext("2d")
      .clearRect(0, 0, background.width, background.height)
  }
  frame.value = null
  autoCopy.value = false
  colorFormat.value = "rgb"
  originalImage = null
  pixelCanvas = null
  pointer.value = null
  toolbarHovered.value = false
  resizeHovered.value = false
  savedOutput = null
  error.value = ""
  lastViewport = null
  cancelDrag()
}

async function loadFrame() {
  resetCapture()
  const generation = frameGeneration
  const bytes = await screenshotApi.frame()
  if (!bytes.byteLength || generation !== frameGeneration) return
  // 帧头和像素来自同一响应，避免连续截图时坐标与图像错配。
  const header = new DataView(bytes, 0, 20)
  const current = {
    left: header.getInt32(0, true),
    top: header.getInt32(4, true),
    width: header.getUint32(8, true),
    height: header.getUint32(12, true)
  }
  if (bytes.byteLength !== 20 + current.width * current.height * 4)
    throw new Error("截图像素数据不完整")
  const background = captureBackground.value
  if (!background) return
  // 尺寸只在显示器布局变化时更新，避免 Vue 重设 width/height 清掉已绘制的首帧。
  if (background.width !== current.width) background.width = current.width
  if (background.height !== current.height) background.height = current.height
  const context = background.getContext("2d", { willReadFrequently: true })
  context.putImageData(
    new ImageData(
      new Uint8ClampedArray(bytes, 20),
      current.width,
      current.height
    ),
    0,
    0
  )
  originalImage = background
  pixelCanvas = background
  frame.value = current
  autoCopy.value = header.getUint32(16, true) === 1
  if (generation !== frameGeneration) return
  checkViewport()
  await screenshotApi.call("ready")
}

onMounted(async () => {
  window.addEventListener("keydown", keydown)
  window.addEventListener("resize", checkViewport)
  try {
    if (mode === "capture") {
      unlistenFrame = await subscribeReady("screenshot:frame", () => {
        loadFrame().catch((cause) => {
          error.value = String(cause)
        })
      })
      unlistenReset = await subscribeReady("screenshot:reset", resetCapture)
      await loadFrame()
      if (!frame.value) await screenshotApi.call("capture-window-ready")
    } else {
      if (!(await loadDocument())) return
      opacity.value = document.value.pin?.opacity ?? 1
      pinGroup.value = document.value.pin?.group || "Default"
      if (!pinGroups.value.includes(pinGroup.value))
        pinGroups.value.push(pinGroup.value)
      pinWidth = document.value.pin.width
      pinHeight = document.value.pin.height
      syncImageMetrics()
      unlisten = await subscribeReady("screenshot:opacity", (value) => {
        opacity.value = value
      })
      if (disposed) {
        unlisten?.()
        return
      }
      await nextTick()
      await pinImage.value?.decode()
      if (disposed) return
      // 先以透明内容激活窗口，让浏览器真正完成最终尺寸的布局和绘制。
      const size = await screenshotApi.call("pin-show", { id })
      if (size.visible && !(await waitForPinPaint(size))) return
      pinReady.value = true
      await nextTick()
      if (size.visible && !(await waitForPinPaint(size))) return
      await screenshotApi.call("pin-ready", { id, dismissCapture })
    }
  } catch (cause) {
    error.value = String(cause)
    if (mode === "pin") pinReady.value = true
    if (mode === "pin")
      await screenshotApi
        .call("pin-ready", { id, dismissCapture: false })
        .catch(() => {})
  }
})
onBeforeUnmount(() => {
  disposed = true
  cancelPinPaint?.()
  selectionGeneration += 1
  documentGeneration += 1
  releaseDocumentImages(document.value)
  frameGeneration += 1
  window.removeEventListener("keydown", keydown)
  window.removeEventListener("resize", checkViewport)
  unlisten?.()
  unlistenFrame?.()
  unlistenReset?.()
  if (resizeFrame) cancelAnimationFrame(resizeFrame)
  if (menuExpanded.value)
    screenshotApi
      .call("pin-action", { id, action: "context-menu-close" })
      .catch(() => {})
})
</script>

<style scoped lang="less">
.capture-window {
  position: fixed;
  inset: 0;
  overflow: hidden;
  cursor: crosshair;
  touch-action: none;
  background: transparent;
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
    outline: 2px solid #38bdf8;
    box-shadow: 0 0 0 10000px #0007;
    &.is-resizing {
      outline-color: #7dd3fc;
    }
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
    .selection-edge {
      position: absolute;
      z-index: 4;
      padding: 0;
      border: 0;
      background: transparent;
      touch-action: none;
      &:hover {
        background: #38bdf844;
      }
      &.edge-n,
      &.edge-s {
        left: 5px;
        right: 5px;
        height: 8px;
      }
      &.edge-n {
        top: -4px;
      }
      &.edge-s {
        bottom: -4px;
      }
      &.edge-e,
      &.edge-w {
        top: 5px;
        bottom: 5px;
        width: 8px;
      }
      &.edge-e {
        right: -4px;
      }
      &.edge-w {
        left: -4px;
      }
    }
    .selection-handle {
      position: absolute;
      width: 10px;
      height: 10px;
      padding: 0;
      border: 2px solid #0284c7;
      border-radius: 1px;
      background: #fff;
      transform: translate(-50%, -50%);
      z-index: 5;
      touch-action: none;
      &:hover {
        border-color: #fff;
        background: #38bdf8;
      }
      &.handle-nw {
        left: 0;
        top: 0;
      }
      &.handle-n {
        left: 50%;
        top: 0;
      }
      &.handle-ne {
        left: 100%;
        top: 0;
      }
      &.handle-e {
        left: 100%;
        top: 50%;
      }
      &.handle-se {
        left: 100%;
        top: 100%;
      }
      &.handle-s {
        left: 50%;
        top: 100%;
      }
      &.handle-sw {
        left: 0;
        top: 100%;
      }
      &.handle-w {
        left: 0;
        top: 50%;
      }
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
.capture-magnifier {
  position: fixed;
  z-index: 1100;
  display: flex;
  flex-direction: column;
  align-items: center;
  box-sizing: border-box;
  width: 186px;
  padding: 8px;
  gap: 4px;
  border: 1px solid #b7b7b7;
  background: #1b1b1bf2;
  color: #fff;
  font-family: Consolas, "Microsoft YaHei", monospace;
  font-size: 12px;
  line-height: 18px;
  text-align: center;
  pointer-events: none;
  .magnifier-preview {
    position: relative;
    width: 168px;
    height: 120px;
    overflow: hidden;
    background: #252525;
    .magnifier-canvas {
      display: block;
      width: 168px;
      height: 120px;
      image-rendering: pixelated;
    }
    .magnifier-crosshair {
      position: absolute;
      inset: 0;
      &::before,
      &::after {
        position: absolute;
        content: "";
        background: #4aa3df66;
      }
      &::before {
        left: 80px;
        top: 0;
        width: 8px;
        height: 100%;
      }
      &::after {
        left: 0;
        top: 56px;
        width: 100%;
        height: 8px;
      }
    }
    .magnifier-pixel {
      position: absolute;
      left: 80px;
      top: 56px;
      box-sizing: border-box;
      width: 8px;
      height: 8px;
      border: 1px solid #fff;
      outline: 1px solid #111;
    }
  }
  .magnifier-coordinates,
  .magnifier-hint {
    white-space: nowrap;
  }
  .magnifier-color {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    .magnifier-swatch {
      box-sizing: border-box;
      width: 14px;
      height: 14px;
      border: 1px solid #ddd;
    }
    .magnifier-value {
      white-space: nowrap;
    }
  }
  .magnifier-hint {
    color: #e6e6e6;
  }
}
.pin-window {
  position: fixed;
  inset: 0;
  overflow: visible;
  user-select: none;
  &.has-shadow {
    filter: drop-shadow(0 2px 7px #0008);
  }
  &.has-border {
    .pin-image {
      border-color: #627080;
    }
  }
  .pin-image {
    box-sizing: border-box;
    display: block;
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    border: 2px solid transparent;
    object-fit: contain;
    cursor: move;
  }
  .pin-menu {
    position: absolute;
    box-sizing: border-box;
    z-index: 10;
    display: flex;
    flex-direction: column;
    min-width: 192px;
    padding: 4px;
    border: 1px solid #a7a7a7;
    border-radius: 7px;
    background: #f8f8f8;
    color: #1d1d1d;
    box-shadow: 0 3px 14px #0004;
    overflow: visible;
    .pin-menu-parent {
      position: relative;
    }
    .pin-menu-button {
      position: relative;
      display: flex;
      align-items: center;
      width: 100%;
      min-height: 27px;
      padding: 3px 8px 3px 26px;
      border: 0;
      border-radius: 3px;
      background: transparent;
      color: inherit;
      justify-content: flex-start;
      text-align: left;
      white-space: nowrap;
      cursor: pointer;
      font-size: 13px;
      &:hover {
        background: #c7e4ff;
        color: #111;
      }
      .pin-menu-check {
        position: absolute;
        left: 8px;
        width: 18px;
        text-align: center;
      }
    }
    .pin-menu-arrow {
      margin-left: auto;
      padding-left: 18px;
      font-size: 18px;
      line-height: 12px;
    }
    .pin-menu-note {
      margin-left: auto;
      color: #777;
      font-size: 12px;
    }
    .pin-menu-separator {
      display: block;
      height: 1px;
      margin: 4px 3px;
      background: #d7d7d7;
    }
    .pin-submenu {
      position: absolute;
      top: -5px;
      left: calc(100% - 2px);
      min-width: 174px;
      padding: 4px;
      border: 1px solid #a7a7a7;
      border-radius: 7px;
      background: #f8f8f8;
      box-shadow: 0 3px 14px #0004;
      color: #1d1d1d;
      &.submenu-left {
        right: calc(100% - 2px);
        left: auto;
      }
    }
    .pin-info-submenu {
      display: flex;
      flex-direction: column;
      gap: 5px;
      min-width: 170px;
      padding: 8px 10px;
      color: #656565;
      font-size: 12px;
      white-space: nowrap;
      .pin-info-opacity {
        display: flex;
        flex-direction: column;
        gap: 3px;
      }
      .opacity-input {
        width: 130px;
      }
    }
    .pin-menu-size {
      justify-content: space-between;
    }
  }
  .pin-toolbar {
    position: absolute;
    top: 8px;
    left: 50%;
    z-index: 8;
    display: flex;
    gap: 2px;
    padding: 4px;
    border: 1px solid #a7a7a7;
    border-radius: 5px;
    background: #f8f8f8;
    box-shadow: 0 2px 8px #0004;
    transform: translateX(-50%);
    .pin-toolbar-button {
      min-width: 45px;
      padding: 4px 8px;
      border: 0;
      border-radius: 3px;
      background: transparent;
      color: #1d1d1d;
      cursor: pointer;
      font-size: 12px;
      &:hover {
        background: #c7e4ff;
      }
    }
  }
  .pin-error {
    position: absolute;
    z-index: 20;
    top: 0;
    left: 0;
    padding: 6px;
    background: #7f1d1d;
    color: #fff;
    font-size: var(--font-size-xs);
  }
}
</style>

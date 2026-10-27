<template>
  <Teleport to="body">
    <div v-show="visible" class="capture-magnifier" :style="panelStyle">
      <div class="magnifier-preview">
        <canvas
          ref="canvas"
          class="magnifier-canvas"
          width="168"
          height="120"
        />
        <span class="magnifier-crosshair" />
        <span class="magnifier-pixel" :style="{ background: pixelColor }" />
      </div>
      <span class="magnifier-coordinates">
        ({{ Math.min(frame.width - 1, Math.floor(pointer.x)) + frame.left }},
        {{ Math.min(frame.height - 1, Math.floor(pointer.y)) + frame.top }})
      </span>
      <div class="magnifier-color">
        <span class="magnifier-swatch" :style="{ background: pixelColor }" />
        <span class="magnifier-value">{{ colorValue }}</span>
      </div>
      <span class="magnifier-hint">按 C 复制颜色值</span>
      <span class="magnifier-hint">按 Shift 切换 RGB/HEX</span>
    </div>
  </Teleport>
</template>

<script setup>
import { computed, ref } from "vue"

const props = defineProps({
  frame: { type: Object, required: true },
  pointer: { type: Object, required: true },
  pixelColor: { type: String, required: true },
  colorValue: { type: String, required: true },
  visible: Boolean
})

const canvas = ref(null)
const panelStyle = computed(() => {
  // 靠近屏幕边缘时翻到鼠标另一侧，保持预览和操作提示完整可见。
  const width = 186
  const height = 226
  const gap = 22
  const { clientX, clientY } = props.pointer
  return {
    left: `${Math.max(0, Math.min(clientX + gap + width <= innerWidth ? clientX + gap : clientX - width - gap, innerWidth - width))}px`,
    top: `${Math.max(0, Math.min(clientY + gap + height <= innerHeight ? clientY + gap : clientY - height - gap, innerHeight - height))}px`
  }
})

function draw(source, point, frame) {
  if (!canvas.value) return false
  const context = canvas.value.getContext("2d")
  context.clearRect(0, 0, 168, 120)
  context.imageSmoothingEnabled = false
  context.drawImage(
    source,
    Math.min(frame.width - 1, Math.floor(point.x)) - 10,
    Math.min(frame.height - 1, Math.floor(point.y)) - 7,
    21,
    15,
    0,
    0,
    168,
    120
  )
  return true
}

defineExpose({ draw })
</script>

<style scoped lang="less">
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
</style>

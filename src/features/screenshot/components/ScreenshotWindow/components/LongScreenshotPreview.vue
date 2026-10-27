<template>
  <aside
    ref="panel"
    class="long-screenshot-preview"
    :style="panelStyle"
    aria-label="长截图实时预览"
    @pointerdown.stop
  >
    <div class="long-preview-head">
      <span class="long-preview-title">拼接预览</span>
      <span class="long-preview-count">{{ progress }} 屏</span>
    </div>
    <div class="long-preview-image">
      <canvas ref="canvas" class="long-preview-canvas" />
    </div>
    <span class="long-preview-size">{{ size.width }} × {{ size.height }}</span>
    <div class="long-preview-actions">
      <button
        class="long-scroll-button"
        :aria-pressed="autoScroll"
        @click.stop="$emit('update:autoScroll', !autoScroll)"
      >
        {{ autoScroll ? "暂停滚动" : "自动滚动" }}
      </button>
      <button class="long-finish-button" @click.stop="$emit('finish')">
        完成截图
      </button>
    </div>
    <span class="long-preview-hint">按 Esc 也可完成</span>
  </aside>
</template>

<script setup>
import { nextTick, ref } from "vue"

defineProps({
  panelStyle: { type: Object, required: true },
  progress: { type: Number, required: true },
  size: { type: Object, required: true },
  autoScroll: Boolean
})
defineEmits(["update:autoScroll", "finish"])

const panel = ref(null)
const canvas = ref(null)

async function draw(source) {
  if (!panel.value || !canvas.value) return null
  // 子组件统一缩绘预览，并在布局稳定后返回原生窗口需要的边界。
  const bounds = panel.value.getBoundingClientRect()
  const scale = Math.min(
    174 / source.width,
    Math.min(260, Math.max(40, innerHeight - bounds.top - 164)) / source.height
  )
  canvas.value.width = Math.max(1, Math.round(source.width * scale))
  canvas.value.height = Math.max(1, Math.round(source.height * scale))
  canvas.value
    .getContext("2d")
    .drawImage(source, 0, 0, canvas.value.width, canvas.value.height)
  await nextTick()
  return panel.value?.getBoundingClientRect() || null
}

defineExpose({ draw })
</script>

<style scoped lang="less">
.long-screenshot-preview {
  position: absolute;
  z-index: 8;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  width: 196px;
  padding: 10px;
  gap: 8px;
  border: 1px solid #334155;
  border-radius: 9px;
  background: #111827f5;
  color: #e2e8f0;
  box-shadow: 0 6px 24px #0005;
  font-size: 12px;
  pointer-events: auto;
  .long-preview-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    .long-preview-title {
      color: #f8fafc;
    }
    .long-preview-count {
      color: #86efac;
    }
  }
  .long-preview-image {
    display: flex;
    justify-content: center;
    overflow: hidden;
    border-radius: 3px;
    background: #020617;
    .long-preview-canvas {
      display: block;
      max-width: 100%;
    }
  }
  .long-preview-size {
    align-self: center;
    color: #94a3b8;
  }
  .long-preview-actions {
    display: flex;
    gap: 6px;
    .long-scroll-button,
    .long-finish-button {
      flex: 1;
      padding: 7px 0;
      border: 1px solid #475569;
      border-radius: 5px;
      background: #1e293b;
      color: #e2e8f0;
      font-size: 12px;
      cursor: pointer;
      &:hover {
        background: #334155;
      }
    }
    .long-scroll-button[aria-pressed="true"] {
      border-color: #7dd3fc;
      color: #7dd3fc;
    }
    .long-finish-button {
      border-color: #4ade80;
      background: #166534;
      color: #f0fdf4;
      &:hover {
        background: #15803d;
      }
    }
  }
  .long-preview-hint {
    padding-top: 8px;
    border-top: 1px solid #334155;
    color: #bbf7d0;
    text-align: center;
  }
}
</style>

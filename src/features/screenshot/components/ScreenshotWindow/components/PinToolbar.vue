<template>
  <div class="pin-toolbar" @pointerdown.stop>
    <button
      v-for="item in actions"
      :key="item.id"
      class="pin-toolbar-button"
      type="button"
      @click="$emit('action', item.id)"
    >
      {{ item.label }}
    </button>
    <button
      class="pin-toolbar-button"
      type="button"
      @click="$emit('toggleZoom')"
    >
      缩放 {{ Math.round(currentZoom * 1000) / 10 }}%
    </button>
  </div>
</template>

<script setup>
defineProps({
  currentZoom: { type: Number, required: true }
})
defineEmits(["action", "toggleZoom"])

// 工具条只声明快捷操作，具体贴图行为继续由窗口入口统一处理。
const actions = [
  { id: "copy", label: "复制" },
  { id: "export", label: "保存" },
  { id: "edit", label: "编辑" }
]
</script>

<style scoped lang="less">
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
</style>

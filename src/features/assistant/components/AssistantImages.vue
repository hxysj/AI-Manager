<template>
  <div v-if="images.length" class="assistant-images">
    <div
      v-for="image in images"
      :key="image.id || image.dataUrl"
      class="assistant-image"
    >
      <img
        class="assistant-image-preview"
        :src="image.dataUrl"
        :alt="image.name || '消息图片'"
      />
      <button
        v-if="editable"
        class="assistant-image-remove"
        type="button"
        :aria-label="'移除图片 ' + image.name"
        @click="$emit('remove', image.id)"
      >
        <X :size="12" />
      </button>
    </div>
  </div>
</template>
<script setup>
import { X } from "lucide-vue-next"
defineProps({ images: { type: Array, default: () => [] }, editable: Boolean })
defineEmits(["remove"])
</script>
<style scoped lang="less">
.assistant-images {
  display: flex;
  flex: none;
  gap: 8px;
  overflow: auto;
  padding: 2px;
  .assistant-image {
    position: relative;
    flex: none;
    height: 48px;
    width: 64px;
    .assistant-image-preview {
      width: 100%;
      height: 100%;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      object-fit: cover;
    }
    .assistant-image-remove {
      position: absolute;
      top: 2px;
      right: 2px;
      display: flex;
      align-items: center;
      justify-content: center;
      width: 18px;
      height: 18px;
      padding: 0;
      border: 1px solid var(--color-line);
      border-radius: 50%;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
    }
  }
}
</style>

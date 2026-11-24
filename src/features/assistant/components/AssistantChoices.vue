<template>
  <div
    v-if="options.length"
    class="assistant-choices"
    role="group"
    :aria-label="choice.output.question"
  >
    <button
      v-for="option in options"
      :key="option"
      class="assistant-choice"
      type="button"
      :disabled="disabled"
      @click="$emit('choose', option)"
    >
      {{ option }}
    </button>
  </div>
</template>

<script setup>
import { computed } from "vue"

const props = defineProps({
  tools: { type: Array, default: () => [] },
  disabled: Boolean
})
defineEmits(["choose"])

// 流式回复和会话历史都使用工具结果恢复选项。
const choice = computed(() =>
  props.tools.findLast(
    (tool) =>
      tool.name === "ask_user_choice" && Array.isArray(tool.output?.options)
  )
)
const options = computed(() => choice.value?.output.options || [])
</script>

<style scoped lang="less">
.assistant-choices {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;

  .assistant-choice {
    padding: 5px 10px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
    line-height: 1.5;
    text-align: left;
    overflow-wrap: anywhere;
    cursor: pointer;

    &:hover:not(:disabled),
    &:focus-visible {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }

    &:disabled {
      opacity: 0.55;
      cursor: default;
    }
  }
}
</style>

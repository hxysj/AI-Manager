<template>
  <div class="work-tracking-fields">
    <label class="tracking-field">
      工作状态
      <select
        class="tracking-input"
        :value="modelValue?.status || ''"
        :disabled="disabled"
        @change="setStatus"
      >
        <option
          v-if="allowClear || !modelValue"
          value=""
          :disabled="!allowClear"
        >
          未跟踪
        </option>
        <option v-for="state in workStates" :key="state.id" :value="state.id">
          {{ state.label }}
        </option>
      </select>
    </label>
    <label class="tracking-field">
      完成进度（%）
      <input
        class="tracking-input"
        type="number"
        min="0"
        :max="modelValue?.status === 'completed' ? 100 : 99"
        step="1"
        placeholder="未知可留空"
        :value="modelValue?.progress ?? ''"
        :disabled="disabled || !modelValue || modelValue.status === 'completed'"
        @input="setProgress"
      />
    </label>
  </div>
</template>

<script setup>
import { workStates } from "../constant"

const props = defineProps({
  modelValue: { type: Object, default: null },
  allowClear: { type: Boolean, default: true },
  disabled: Boolean
})
const emit = defineEmits(["update:modelValue"])

// 只有整体完成对应 100%，重新开始时将进度留给用户填写。
function setStatus(event) {
  const status = event.target.value
  if (!status) {
    emit("update:modelValue", null)
    return
  }
  let progress = props.modelValue?.progress ?? null
  if (status === "completed") progress = 100
  else if (progress === 100) progress = null
  emit("update:modelValue", { status, progress })
}

function setProgress(event) {
  const { value, valueAsNumber, validity } = event.target
  emit("update:modelValue", {
    ...props.modelValue,
    progress: value === "" && !validity.badInput ? null : valueAsNumber
  })
}
</script>

<style scoped lang="less">
.work-tracking-fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  .tracking-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 7px;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    .tracking-input {
      width: 100%;
      min-width: 0;
      padding: 8px 9px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      color: var(--color-text);
      background: var(--color-panel);
      font: inherit;
      &:disabled {
        opacity: 0.63;
      }
    }
  }
}
</style>

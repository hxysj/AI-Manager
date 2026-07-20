<template>
  <span class="token-count">
    <span>{{ tokenCountParts.compact }}</span>
    <small v-if="tokenCountParts.exact" class="token-count-exact">
      （{{ tokenCountParts.exact }}）
    </small>
  </span>
</template>

<script setup>
import { computed } from "vue"
import { formatTokenCountParts } from "@/utils/formatters"

const props = defineProps({
  value: {
    type: [Number, String],
    default: 0
  }
})

// 保持单位数字与精确数字分离，便于只缩小括号内容。
const tokenCountParts = computed(() => formatTokenCountParts(props.value))
</script>

<style scoped lang="less">
.token-count {
  display: inline-flex;
  align-items: baseline;
  gap: 2px;
  color: inherit;
  font-size: inherit;
  font-weight: inherit;
  line-height: inherit;

  .token-count-exact {
    color: #7890a6;
    font-size: 0.72em;
    font-weight: 500;
    line-height: 1.1;
    white-space: nowrap;
  }
}
</style>

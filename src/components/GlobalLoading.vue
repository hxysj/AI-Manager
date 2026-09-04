<template>
  <Teleport to="body">
    <div v-if="loading" class="global-loading">
      <div class="global-loading__card">
        <span class="global-loading__spinner"></span>
        <span data-emphasis>正在处理中...</span>
      </div>
    </div>
  </Teleport>
</template>

<script setup>
import { useGlobalLoading } from '@/utils/global-loading'

const { loading } = useGlobalLoading()
</script>

<style scoped lang="less">
.global-loading {
  position: fixed;
  inset: 0;
  z-index: 999;
  display: grid;
  place-items: center;
  background: rgba(15, 23, 42, 0.18);
  backdrop-filter: blur(2px);

  &__card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: color-mix(in srgb, var(--color-panel) 96%, transparent);
    box-shadow: 0 18px 42px rgba(34, 56, 83, 0.18);
    color: var(--color-text);
  }

  &__spinner {
    width: 18px;
    height: 18px;
    border: 2px solid var(--color-line);
    border-top-color: var(--color-primary);
    border-radius: 50%;
    animation: global-loading-spin 0.8s linear infinite;
  }
}

@keyframes global-loading-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

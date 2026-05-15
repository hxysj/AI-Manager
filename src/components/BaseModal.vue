<template>
  <div class="base-modal">
    <div class="base-modal__backdrop" @click="$emit('close')"></div>
    <section class="base-modal__panel">
      <header class="base-modal__header">
        <div>
          <h2>{{ title }}</h2>
          <p v-if="description">{{ description }}</p>
        </div>
        <button class="base-modal__close" type="button" @click="$emit('close')">
          ×
        </button>
      </header>
      <div class="base-modal__content">
        <slot></slot>
      </div>
    </section>
  </div>
</template>

<script setup>
defineProps({
  title: {
    type: String,
    required: true
  },
  description: {
    type: String,
    default: ""
  }
})

defineEmits(["close"])
</script>

<style scoped lang="less">
.base-modal {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: grid;
  place-items: center;
  padding: 24px;
}

.base-modal__backdrop {
  position: absolute;
  inset: 0;
  background: rgba(15, 23, 42, 0.24);
}

.base-modal__panel {
  position: relative;
  width: min(560px, 100%);
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  box-shadow: var(--shadow-panel);
}

.base-modal__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding: 22px 24px 10px;
}

.base-modal__header h2 {
  margin: 0;
  font-size: 1.4rem;
}

.base-modal__header p {
  margin: 6px 0 0;
  color: var(--color-text-muted);
  font-size: 0.92rem;
}

.base-modal__close {
  display: grid;
  width: 36px;
  height: 36px;
  place-items: center;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-text-muted);
  cursor: pointer;
  font-size: 1.4rem;
  line-height: 1;
}

.base-modal__content {
  padding: 0 24px 24px;
}
</style>

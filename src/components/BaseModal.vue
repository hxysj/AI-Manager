<template>
  <div class="base-modal">
    <div class="base-modal__backdrop" @click="$emit('close')"></div>
    <section class="base-modal__panel">
      <header class="base-modal__header">
        <div>
          <h2>{{ title }}</h2>
          <p v-if="description">{{ description }}</p>
        </div>
        <button class="base-modal__close" type="button" @click="$emit('close')">×</button>
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
    default: ''
  }
})

defineEmits(['close'])
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
  background: rgba(19, 24, 36, 0.5);
  backdrop-filter: blur(8px);
}

.base-modal__panel {
  position: relative;
  width: min(560px, 100%);
  border: 1px solid rgba(61, 73, 99, 0.12);
  border-radius: 28px;
  background:
    linear-gradient(180deg, rgba(255, 252, 247, 0.98), rgba(249, 245, 238, 0.98)),
    #fff;
  box-shadow: 0 24px 80px rgba(24, 35, 58, 0.24);
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
  font-family: 'Georgia', 'Times New Roman', serif;
  font-size: 1.4rem;
}

.base-modal__header p {
  margin: 6px 0 0;
  color: rgba(43, 57, 84, 0.62);
  font-size: 0.92rem;
}

.base-modal__close {
  display: grid;
  width: 36px;
  height: 36px;
  place-items: center;
  border: 1px solid rgba(61, 73, 99, 0.12);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.82);
  color: #40516e;
  cursor: pointer;
  font-size: 1.4rem;
  line-height: 1;
}

.base-modal__content {
  padding: 0 24px 24px;
}
</style>

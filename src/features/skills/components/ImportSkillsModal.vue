<template>
  <BaseModal
    title="确认导入 Skill"
    description="选择需要从 CLI 真实目录导入到 AI Manager 集中管理的 Skill。"
    @close="$emit('close')"
  >
    <form class="import-skills-modal" @submit.prevent="submit">
      <div class="import-skills-modal__summary">
        发现 {{ candidates.length }} 个可导入 Skill
      </div>

      <div class="import-skills-modal__list">
        <label
          v-for="candidate in candidates"
          :key="candidate.name"
          class="import-skills-modal__item"
        >
          <input
            v-model="selectedNames"
            type="checkbox"
            :value="candidate.name"
          />
          <span class="import-skills-modal__content">
            <strong>{{ candidate.name }}</strong>
            <span>{{ candidate.description || '未提供描述' }}</span>
            <small>{{ candidate.cliNames.join('、') }}</small>
          </span>
        </label>
      </div>

      <div class="import-skills-modal__actions">
        <button class="action-button" type="button" @click="$emit('close')">
          取消
        </button>
        <button
          class="action-button action-button--primary"
          type="submit"
          :disabled="!selectedNames.length"
        >
          导入选中项
        </button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { ref } from 'vue'
import BaseModal from '@/components/BaseModal.vue'

const props = defineProps({
  candidates: {
    type: Array,
    required: true
  }
})

const emit = defineEmits(['close', 'submit'])
const selectedNames = ref(props.candidates.map(item => item.name))

function submit() {
  emit('submit', selectedNames.value)
}
</script>

<style scoped lang="less">
.import-skills-modal {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.import-skills-modal__summary {
  color: var(--color-text-muted);
  font-size: 0.88rem;
  font-weight: 700;
}

.import-skills-modal__list {
  display: flex;
  max-height: 360px;
  flex-direction: column;
  gap: 8px;
  overflow: auto;
}

.import-skills-modal__item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  cursor: pointer;
}

.import-skills-modal__item:hover {
  border-color: #b9ccda;
  background: var(--color-primary-soft);
}

.import-skills-modal__item input {
  margin-top: 4px;
}

.import-skills-modal__content {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 3px;
}

.import-skills-modal__content strong {
  color: var(--color-text);
  font-size: 0.94rem;
}

.import-skills-modal__content span {
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: 0.82rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.import-skills-modal__content small {
  color: var(--color-accent);
  font-size: 0.76rem;
  font-weight: 700;
}

.import-skills-modal__actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding-top: 6px;
}

.action-button {
  height: 40px;
  padding: 0 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #fbfcfd;
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;
}

.action-button:hover {
  border-color: #b9ccda;
  background: var(--color-primary-soft);
}

.action-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.action-button--primary {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #fff;
}

.action-button--primary:hover {
  border-color: #2a4f6f;
  background: #2a4f6f;
}
</style>

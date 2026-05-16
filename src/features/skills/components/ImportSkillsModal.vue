<template>
  <BaseModal
    title="确认导入 Skill"
    description="选择需要从 CLI 真实目录导入到 AI Manager 集中管理的 Skill。"
    @close="$emit('close')"
  >
    <form class="import-skills-modal" @submit.prevent="submit">
      <div class="import-skills-modal__summary">
        <span>可导入 {{ candidateItems.length }} 个</span>
        <span v-if="conflictItems.length">需确认 {{ conflictItems.length }} 个</span>
      </div>

      <div class="import-skills-modal__body">
        <section v-if="candidateItems.length" class="import-skills-modal__section">
          <h3>可导入 Skill</h3>
          <div class="import-skills-modal__list">
            <label
              v-for="candidate in candidateItems"
              :key="candidate.id"
              class="import-skills-modal__item"
            >
              <input
                v-model="selectedSources"
                type="checkbox"
                :value="candidate.id"
              />
              <span class="import-skills-modal__content">
                <strong>{{ candidate.name }}</strong>
                <span>{{ candidate.description || '未提供描述' }}</span>
                <small>{{ candidate.cliNames.join('、') }}</small>
              </span>
            </label>
          </div>
        </section>

        <section v-if="conflictItems.length" class="import-skills-modal__section">
          <h3>同名冲突</h3>
          <article
            v-for="conflict in conflictItems"
            :key="conflict.name"
            class="import-skills-modal__conflict"
          >
            <div class="import-skills-modal__conflict-head">
              <strong>{{ conflict.name }}</strong>
              <span>名称相同但内容不同，请选择保留版本</span>
            </div>

            <label
              v-for="option in conflict.options"
              :key="option.id"
              class="import-skills-modal__item import-skills-modal__item--radio"
            >
              <input
                v-model="selectedConflicts[conflict.name]"
                type="radio"
                :name="`skill-conflict-${conflict.name}`"
                :value="option.id"
              />
              <span class="import-skills-modal__content">
                <strong>{{ option.alreadyManaged ? '保留 Manager 版本' : option.name }}</strong>
                <span>{{ option.description || '未提供描述' }}</span>
                <small>{{ option.cliNames.join('、') }}</small>
              </span>
            </label>
          </article>
        </section>
      </div>

      <div class="import-skills-modal__actions">
        <button class="action-button" type="button" @click="$emit('close')">
          取消
        </button>
        <button
          class="action-button action-button--primary"
          type="submit"
          :disabled="!canSubmit"
        >
          导入选中项
        </button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { computed, reactive, ref, watch } from 'vue'
import BaseModal from '@/components/BaseModal.vue'

const props = defineProps({
  candidates: {
    type: [Array, Object],
    required: true
  }
})

const emit = defineEmits(['close', 'submit'])

const candidateItems = computed(() => {
  return Array.isArray(props.candidates)
    ? props.candidates
    : props.candidates.candidates || []
})
const conflictItems = computed(() => {
  return Array.isArray(props.candidates) ? [] : props.candidates.conflicts || []
})
const selectedSources = ref([])
const selectedConflicts = reactive({})
const canSubmit = computed(() => {
  return (
    selectedSources.value.length ||
    conflictItems.value.every(item => selectedConflicts[item.name])
  )
})

watch(
  () => props.candidates,
  () => {
    selectedSources.value = candidateItems.value.map(item => item.id)

    for (const item of conflictItems.value) {
      selectedConflicts[item.name] =
        item.options.find(option => option.alreadyManaged)?.id ||
        item.options[0]?.id ||
        ''
    }
  },
  { immediate: true }
)

function submit() {
  emit('submit', {
    sourcePaths: candidateItems.value
      .filter(item => selectedSources.value.includes(item.id))
      .flatMap(item => [...(item.sourcePaths || [item.id])]),
    choices: conflictItems.value.map(item => ({
      name: item.name,
      id: selectedConflicts[item.name],
      sourcePaths: [
        ...(
          item.options.find(option => option.id === selectedConflicts[item.name])
            ?.sourcePaths || []
        )
      ]
    }))
  })
}
</script>

<style scoped lang="less">
.import-skills-modal {
  display: flex;
  max-height: 620px;
  min-height: 0;
  flex-direction: column;
  gap: 12px;

  &__summary {
    display: flex;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 0.84rem;
    font-weight: 700;
  }

  &__summary span {
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--color-primary-soft);
  }

  &__body {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 12px;
    overflow: auto;
    padding-right: 4px;
  }

  &__section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &__section h3 {
    margin: 0;
    color: var(--color-text);
    font-size: 0.94rem;
  }

  &__list,
  &__conflict {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &__conflict {
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__conflict-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  &__conflict-head strong {
    color: var(--color-text);
    font-size: 0.92rem;
  }

  &__conflict-head span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
  }

  &__item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    cursor: pointer;
  }

  &__item:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &__item input {
    margin-top: 4px;
  }

  &__item--radio {
    background: #ffffff;
  }

  &__content {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  &__content strong {
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.9rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__content span {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__content small {
    overflow: hidden;
    color: var(--color-accent);
    font-size: 0.74rem;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 6px;
  }
}

.action-button {
  height: 38px;
  padding: 0 14px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #fbfcfd;
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.88rem;
  font-weight: 600;

  &:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }

  &--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #fff;
  }

  &--primary:hover {
    border-color: #2a4f6f;
    background: #2a4f6f;
  }
}
</style>

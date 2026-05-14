<template>
  <BaseModal
    title="新建 Skill"
    description="AI Manager 会在集中式 skills 目录中创建真实 Skill Source，并用 junction 挂载到目标 CLI。"
    @close="$emit('close')"
  >
    <form class="create-skill-modal" @submit.prevent="submit">
      <label>
        <span>Skill 名称</span>
        <input v-model.trim="form.name" required type="text" placeholder="例如：prompt-linter" />
      </label>

      <label>
        <span>描述</span>
        <textarea
          v-model.trim="form.description"
          rows="4"
          placeholder="说明这个 Skill 的用途与触发场景"
        ></textarea>
      </label>

      <div class="create-skill-modal__grid">
        <label>
          <span>作者</span>
          <input v-model.trim="form.author" type="text" placeholder="可选" />
        </label>
        <label>
          <span>标签</span>
          <input v-model.trim="form.tags" type="text" placeholder="design, prompt, lint" />
        </label>
      </div>

      <div class="create-skill-modal__actions">
        <button class="action-button" type="button" @click="$emit('close')">取消</button>
        <button class="action-button action-button--primary" type="submit">创建 Skill</button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { reactive } from 'vue'
import BaseModal from '@/components/BaseModal.vue'

const emit = defineEmits(['close', 'submit'])

const form = reactive({
  name: '',
  description: '',
  author: '',
  tags: ''
})

function submit() {
  emit('submit', {
    name: form.name,
    description: form.description,
    author: form.author,
    tags: form.tags
      .split(',')
      .map(item => item.trim())
      .filter(Boolean)
  })

  form.name = ''
  form.description = ''
  form.author = ''
  form.tags = ''
}
</script>

<style scoped lang="less">
.create-skill-modal {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.create-skill-modal label {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.create-skill-modal span {
  color: rgba(43, 57, 84, 0.7);
  font-size: 0.84rem;
  font-weight: 700;
}

.create-skill-modal input,
.create-skill-modal textarea {
  width: 100%;
  padding: 12px 14px;
  border: 1px solid rgba(58, 69, 94, 0.14);
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.92);
  color: #1f314f;
  font: inherit;
  resize: vertical;
}

.create-skill-modal textarea {
  min-height: 120px;
}

.create-skill-modal__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}

.create-skill-modal__actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding-top: 6px;
}

.action-button {
  height: 40px;
  padding: 0 16px;
  border: 1px solid rgba(58, 69, 94, 0.14);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.84);
  color: #2a4366;
  cursor: pointer;
  font-weight: 600;
}

.action-button--primary {
  border-color: rgba(38, 92, 183, 0.2);
  background: linear-gradient(135deg, #1f5ca2, #d66a2c);
  color: #fff;
}

@media (max-width: 720px) {
  .create-skill-modal__grid {
    grid-template-columns: 1fr;
  }
}
</style>

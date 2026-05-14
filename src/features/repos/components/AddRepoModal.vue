<template>
  <BaseModal
    title="添加 Repo"
    description="支持 GitHub、通用 Git 仓库与本地目录。远程仓库会 clone 到 AI Manager 的 repos 工作区。"
    @close="$emit('close')"
  >
    <form class="add-repo-modal" @submit.prevent="submit">
      <div class="add-repo-modal__grid">
        <label>
          <span>类型</span>
          <select v-model="form.type">
            <option value="github">GitHub</option>
            <option value="git">Git</option>
            <option value="local">Local</option>
          </select>
        </label>
        <label>
          <span>显示名称</span>
          <input v-model.trim="form.name" type="text" placeholder="可选" />
        </label>
      </div>

      <label>
        <span>{{ form.type === 'local' ? '本地目录' : '仓库地址' }}</span>
        <input
          v-model.trim="form.source"
          required
          type="text"
          :placeholder="placeholderText"
        />
      </label>

      <div class="add-repo-modal__actions">
        <button class="action-button" type="button" @click="$emit('close')">取消</button>
        <button class="action-button action-button--primary" type="submit">添加 Repo</button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { computed, reactive } from 'vue'
import BaseModal from '@/components/BaseModal.vue'

const emit = defineEmits(['close', 'submit'])

const form = reactive({
  type: 'github',
  name: '',
  source: ''
})

const placeholderText = computed(() => {
  if (form.type === 'github') {
    return '例如：owner/repo 或 https://github.com/owner/repo.git'
  }

  if (form.type === 'git') {
    return '例如：https://git.example.com/team/repo.git'
  }

  return '例如：D:\\skills-repo'
})

function submit() {
  emit('submit', {
    type: form.type,
    name: form.name,
    source: form.source
  })

  form.type = 'github'
  form.name = ''
  form.source = ''
}
</script>

<style scoped lang="less">
.add-repo-modal {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.add-repo-modal label {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.add-repo-modal span {
  color: rgba(43, 57, 84, 0.7);
  font-size: 0.84rem;
  font-weight: 700;
}

.add-repo-modal input,
.add-repo-modal select {
  width: 100%;
  height: 46px;
  padding: 0 14px;
  border: 1px solid rgba(58, 69, 94, 0.14);
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.92);
  color: #1f314f;
  font: inherit;
}

.add-repo-modal__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}

.add-repo-modal__actions {
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
  .add-repo-modal__grid {
    grid-template-columns: 1fr;
  }
}
</style>

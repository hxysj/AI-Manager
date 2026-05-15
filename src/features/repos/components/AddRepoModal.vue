<template>
  <BaseModal
    title="添加 Repo"
    description="支持 GitHub、通用 Git 仓库与本地目录。远程仓库会 clone 到 AI Manager 的 repos 工作区。"
    @close="$emit('close')"
  >
    <form class="add-repo-modal" @submit.prevent="submit">
      <div class="add-repo-modal__grid">
        <label class="add-repo-modal__field">
          <span>类型</span>
          <select v-model="form.type">
            <option value="github">GitHub</option>
            <option value="git">Git</option>
            <option value="local">Local</option>
          </select>
        </label>
        <label class="add-repo-modal__field">
          <span>显示名称</span>
          <input v-model.trim="form.name" type="text" placeholder="可选" />
        </label>
      </div>

      <label class="add-repo-modal__field">
        <span>{{ form.type === "local" ? "本地目录" : "仓库地址" }}</span>
        <input
          v-model.trim="form.source"
          required
          type="text"
          :placeholder="placeholderText"
        />
      </label>

      <div class="add-repo-modal__actions">
        <button class="action-button" type="button" @click="$emit('close')">
          取消
        </button>
        <button class="action-button action-button--primary" type="submit">
          添加 Repo
        </button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { computed, reactive } from "vue"
import BaseModal from "@/components/BaseModal.vue"

const emit = defineEmits(["close", "submit"])

const form = reactive({
  type: "github",
  name: "",
  source: ""
})

const placeholderText = computed(() => {
  if (form.type === "github") {
    return "例如：owner/repo 或 https://github.com/owner/repo.git"
  }

  if (form.type === "git") {
    return "例如：https://git.example.com/team/repo.git"
  }

  return "例如：D:\\skills-repo"
})

function submit() {
  emit("submit", {
    type: form.type,
    name: form.name,
    source: form.source
  })

  form.type = "github"
  form.name = ""
  form.source = ""
}
</script>

<style scoped lang="less">
.add-repo-modal {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.add-repo-modal__field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.add-repo-modal span {
  color: var(--color-text-muted);
  font-size: 0.84rem;
  font-weight: 700;
}

.add-repo-modal input,
.add-repo-modal select {
  width: 100%;
  height: 46px;
  padding: 0 14px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-text);
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
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;
}

.action-button--primary {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #fff;
}
</style>

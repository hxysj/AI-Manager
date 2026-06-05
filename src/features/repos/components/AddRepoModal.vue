<template>
  <BaseModal
    class="add-repo-modal-shell"
    title="添加项目"
    description="支持 GitHub、通用 Git 仓库与本地目录。远程仓库会 clone 到 Monkey Thief 的项目工作区。"
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
        <div class="add-repo-modal__source-row">
          <input
            v-model.trim="form.source"
            required
            type="text"
            :placeholder="placeholderText"
          />
          <button
            v-if="form.type === 'local'"
            class="add-repo-modal__directory-button"
            type="button"
            @click="selectLocalDirectory"
          >
            选择目录
          </button>
        </div>
      </label>

      <div class="add-repo-modal__actions">
        <button class="action-button" type="button" @click="$emit('close')">
          取消
        </button>
        <button class="action-button action-button--primary" type="submit">
          添加项目
        </button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { computed, reactive } from "vue"
import BaseModal from "@/components/BaseModal.vue"
import { systemApi } from "@/api"

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

async function selectLocalDirectory() {
  const selectedPath = await systemApi.selectDirectory({
    title: "选择本地项目目录",
    defaultPath: form.source
  })

  if (selectedPath) {
    form.source = selectedPath
  }
}
</script>

<style scoped lang="less">
.add-repo-modal-shell {
  :deep(.base-modal__panel) {
    width: 920px;
  }

  :deep(.base-modal__header) {
    padding: 18px 24px 8px;
  }

  :deep(.base-modal__header h2) {
    font-size: 1.12rem;
  }

  :deep(.base-modal__header p) {
    margin-top: 5px;
    font-size: 0.8rem;
  }

  :deep(.base-modal__close) {
    width: 32px;
    height: 32px;
    font-size: 1.1rem;
  }
}

.add-repo-modal {
  display: flex;
  flex-direction: column;
  gap: 12px;

  &__field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  & span {
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
  }

  & input,
  & select {
    width: 100%;
    height: 38px;
    padding: 0 11px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    font-size: 0.84rem;
  }

  &__grid {
    display: flex;
    gap: 12px;
  }

  &__grid > * {
    flex: 1;
  }

  &__source-row {
    display: flex;
    gap: 8px;
  }

  &__source-row input {
    flex: 1;
  }

  &__directory-button {
    height: 38px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
    white-space: nowrap;
  }

  &__directory-button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  &__actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 6px;
    position: sticky;
    bottom: 0;
    background: var(--color-panel);
  }
}

.action-button {
  height: 36px;
  padding: 0 14px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.84rem;
  font-weight: 700;

  &--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #fff;
  }
}
</style>

<template>
  <BaseModal
    title="新建 Skill"
    description="Monkey Thief 会在集中式 skills 目录中创建真实 Skill Source，并用 junction 挂载到目标 CLI。"
    @close="$emit('close')"
  >
    <form class="create-skill-modal" @submit.prevent="submit">
      <label class="create-skill-modal__field">
        <span>Skill 名称</span>
        <input
          v-model.trim="form.name"
          required
          type="text"
          placeholder="例如：prompt-linter"
        />
      </label>

      <label class="create-skill-modal__field">
        <span>描述</span>
        <textarea
          v-model.trim="form.description"
          rows="4"
          placeholder="说明这个 Skill 的用途与触发场景"
        ></textarea>
      </label>

      <div class="create-skill-modal__grid">
        <label class="create-skill-modal__field">
          <span>作者</span>
          <input v-model.trim="form.author" type="text" placeholder="可选" />
        </label>
        <label class="create-skill-modal__field">
          <span>标签</span>
          <input
            v-model.trim="form.tags"
            type="text"
            placeholder="design, prompt, lint"
          />
        </label>
      </div>

      <div class="create-skill-modal__actions">
        <button class="action-button" type="button" @click="$emit('close')">
          取消
        </button>
        <button class="action-button action-button--primary" type="submit">
          创建 Skill
        </button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { reactive } from "vue"
import BaseModal from "@/components/BaseModal.vue"

const emit = defineEmits(["close", "submit"])

const form = reactive({
  name: "",
  description: "",
  author: "",
  tags: ""
})

function submit() {
  emit("submit", {
    name: form.name,
    description: form.description,
    author: form.author,
    tags: form.tags
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean)
  })

  form.name = ""
  form.description = ""
  form.author = ""
  form.tags = ""
}
</script>

<style scoped lang="less">
.create-skill-modal {
  display: flex;
  flex-direction: column;
  gap: 16px;

  &__field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  & span {
    color: var(--color-text-muted);
    font-size: 0.84rem;
    font-weight: 700;
  }

  & input,
  & textarea {
    width: 100%;
    padding: 12px 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    resize: vertical;
  }

  & textarea {
    min-height: 120px;
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
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
  height: 40px;
  padding: 0 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;

  &--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #fff;
  }
}
</style>

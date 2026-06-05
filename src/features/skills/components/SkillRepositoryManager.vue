<template>
  <section class="skill-repository-manager">
    <header class="skill-repository-manager-head">
      <button class="skill-repository-manager-back" type="button" @click="$emit('back')">
        <ArrowLeft :size="15" />
        返回
      </button>
      <div class="skill-repository-manager-title">
        <h1 class="skill-repository-manager-title-text">管理技能仓库</h1>
        <span class="skill-repository-manager-title-desc">
          {{ repositories.length }} 个仓库
        </span>
      </div>
    </header>

    <section class="skill-repository-manager-form-card">
      <div class="skill-repository-manager-form-head">
        <strong class="skill-repository-manager-form-title">
          添加 GitHub 仓库
        </strong>
        <span class="skill-repository-manager-form-desc">
          识别仓库里的 SKILL.md，并将父目录作为 Skill 来源。
        </span>
      </div>
      <form class="skill-repository-manager-form" @submit.prevent="addRepository">
        <label class="skill-repository-manager-field url">
          <span class="skill-repository-manager-field-label">仓库 URL</span>
          <input
            v-model.trim="repositorySource"
            class="skill-repository-manager-field-control"
            type="text"
            placeholder="https://github.com/owner/repo"
          />
        </label>
        <label class="skill-repository-manager-field branch">
          <span class="skill-repository-manager-field-label">分支</span>
          <input
            v-model.trim="repositoryBranch"
            class="skill-repository-manager-field-control"
            type="text"
            placeholder="留空使用默认分支"
          />
        </label>
        <button class="skill-repository-manager-button primary" type="submit">
          <Plus :size="16" />
          添加仓库
        </button>
      </form>
    </section>

    <section class="skill-repository-manager-list-card">
      <div class="skill-repository-manager-list-head">
        <strong class="skill-repository-manager-list-title">已添加仓库</strong>
        <span class="skill-repository-manager-list-count">
          {{ repositorySkillCount }} 个 Skill
        </span>
      </div>

      <div v-if="repositories.length" class="skill-repository-manager-list">
        <article
          v-for="repository in repositories"
          :key="repository.id"
          :class="[
            'skill-repository-manager-item',
            {
              error: repository.status === 'error'
            }
          ]"
        >
          <div class="skill-repository-manager-item-main">
            <div class="skill-repository-manager-item-title-row">
              <strong
                class="skill-repository-manager-item-name"
                :title="repository.name"
              >
                {{ repository.name }}
              </strong>
              <span
                :class="[
                  'skill-repository-manager-status',
                  {
                    error: repository.status === 'error'
                  }
                ]"
              >
                {{ repository.status === "error" ? "访问异常" : "访问正常" }}
              </span>
            </div>
            <span class="skill-repository-manager-source" :title="repository.source">
              {{ repository.source }}
            </span>
            <div class="skill-repository-manager-meta">
              <span class="skill-repository-manager-meta-item">
                分支：{{ repository.branch || "默认分支" }}
              </span>
              <span class="skill-repository-manager-meta-item">
                识别到 {{ repository.skills.length }} 个 Skill
              </span>
            </div>
            <p
              v-if="repository.status === 'error'"
              class="skill-repository-manager-error"
            >
              <AlertTriangle :size="14" />
              {{ repository.error }}
            </p>
          </div>
          <div class="skill-repository-manager-actions">
            <button
              class="skill-repository-manager-icon-button"
              type="button"
              title="刷新仓库"
              @click="$emit('refresh-repository', repository)"
            >
              <RefreshCw :size="15" />
            </button>
            <button
              class="skill-repository-manager-icon-button"
              type="button"
              title="打开 GitHub"
              @click="openRepository(repository)"
            >
              <ExternalLink :size="15" />
            </button>
            <button
              class="skill-repository-manager-icon-button danger"
              type="button"
              title="删除仓库"
              @click="$emit('remove-repository', repository)"
            >
              <Trash2 :size="15" />
            </button>
          </div>
        </article>
      </div>

      <div v-else class="skill-repository-manager-empty">
        <strong class="skill-repository-manager-empty-title">暂无技能仓库</strong>
        <span class="skill-repository-manager-empty-desc">
          添加 GitHub 仓库后会在这里显示扫描结果。
        </span>
      </div>
    </section>
  </section>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  AlertTriangle,
  ArrowLeft,
  ExternalLink,
  Plus,
  RefreshCw,
  Trash2
} from "lucide-vue-next"
import { systemApi } from "@/api"
import { createMessage } from "@/utils/message"

const props = defineProps({
  repositories: {
    type: Array,
    default: () => []
  }
})

const emit = defineEmits([
  "add-repository",
  "back",
  "refresh-repository",
  "remove-repository"
])

const repositorySource = ref("")
const repositoryBranch = ref("")

const repositorySkillCount = computed(() => {
  return props.repositories.reduce(
    (total, repository) => total + repository.skills.length,
    0
  )
})

function addRepository() {
  if (!repositorySource.value) {
    createMessage.warning("请输入 GitHub 仓库地址。")
    return
  }

  emit("add-repository", {
    source: repositorySource.value,
    branch: repositoryBranch.value
  })
  repositorySource.value = ""
  repositoryBranch.value = ""
}

async function openRepository(repository) {
  try {
    await systemApi.openExternal({
      url: repository.htmlUrl
    })
  } catch (error) {
    createMessage.error(error.message)
  }
}
</script>

<style scoped lang="less">
.skill-repository-manager {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;

  .skill-repository-manager-head {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
  }

  .skill-repository-manager-back,
  .skill-repository-manager-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 34px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
    white-space: nowrap;
  }

  .skill-repository-manager-back {
    padding: 0 11px;
  }

  .skill-repository-manager-button {
    padding: 0 12px;
  }

  .skill-repository-manager-back:hover,
  .skill-repository-manager-button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  .skill-repository-manager-button.primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  .skill-repository-manager-button.primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }

  .skill-repository-manager-title {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  .skill-repository-manager-title-text {
    margin: 0;
    color: var(--color-text);
    font-size: 1.26rem;
    line-height: 1.2;
  }

  .skill-repository-manager-title-desc {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .skill-repository-manager-form-card,
  .skill-repository-manager-list-card {
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  .skill-repository-manager-form-card {
    flex: none;
    padding: 14px;
  }

  .skill-repository-manager-form-head {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 12px;
  }

  .skill-repository-manager-form-title {
    color: var(--color-text);
    font-size: 0.96rem;
  }

  .skill-repository-manager-form-desc {
    color: var(--color-text-muted);
    font-size: 0.8rem;
    line-height: 1.45;
  }

  .skill-repository-manager-form {
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }

  .skill-repository-manager-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  .skill-repository-manager-field.url {
    flex: 1;
  }

  .skill-repository-manager-field.branch {
    width: 170px;
    flex: none;
  }

  .skill-repository-manager-field-label {
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
  }

  .skill-repository-manager-field-control {
    height: 36px;
    min-width: 0;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
    color: var(--color-text);
    font: inherit;
    font-size: 0.84rem;
    outline: none;
  }

  .skill-repository-manager-field-control:focus {
    border-color: #8eb6d9;
    box-shadow: 0 0 0 3px rgba(47, 95, 145, 0.1);
  }

  .skill-repository-manager-list-card {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
  }

  .skill-repository-manager-list-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 13px 14px;
    border-bottom: 1px solid var(--color-line);
  }

  .skill-repository-manager-list-title {
    color: var(--color-text);
    font-size: 0.94rem;
  }

  .skill-repository-manager-list-count {
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-weight: 700;
  }

  .skill-repository-manager-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding: 10px;
  }

  .skill-repository-manager-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
  }

  .skill-repository-manager-item.error {
    border-color: #efc6c6;
    background: #fff8f8;
  }

  .skill-repository-manager-item-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 6px;
  }

  .skill-repository-manager-item-title-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
  }

  .skill-repository-manager-item-name {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.92rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-repository-manager-status {
    display: inline-flex;
    flex: none;
    align-items: center;
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    background: var(--color-success-soft);
    color: var(--color-success);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .skill-repository-manager-status.error {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .skill-repository-manager-source {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-repository-manager-meta {
    display: flex;
    gap: 6px;
  }

  .skill-repository-manager-meta-item {
    display: inline-flex;
    align-items: center;
    height: 24px;
    padding: 0 8px;
    border: 1px solid #d8e4ee;
    border-radius: 999px;
    background: #ffffff;
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .skill-repository-manager-error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--color-danger);
    font-size: 0.78rem;
    font-weight: 700;
    line-height: 1.45;
  }

  .skill-repository-manager-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 7px;
  }

  .skill-repository-manager-icon-button {
    display: inline-grid;
    width: 32px;
    height: 32px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .skill-repository-manager-icon-button:hover {
    border-color: #b9ccda;
    color: var(--color-primary);
  }

  .skill-repository-manager-icon-button.danger:hover {
    border-color: #edb9b9;
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .skill-repository-manager-empty {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 10px;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: #ffffff;
    color: var(--color-text-muted);
    text-align: center;
  }

  .skill-repository-manager-empty-title {
    color: var(--color-text);
    font-size: 0.98rem;
  }

  .skill-repository-manager-empty-desc {
    font-size: 0.82rem;
    line-height: 1.45;
  }
}
</style>

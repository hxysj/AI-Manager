<template>
  <section class="repos-view">
    <header class="repos-view__toolbar">
      <div>
        <p class="repos-view__eyebrow">Repo System</p>
        <h1>Skill Repos</h1>
      </div>
      <div class="repos-view__toolbar-actions">
        <button
          class="action-button action-button--primary"
          type="button"
          @click="$emit('add-repo')"
        >
          添加 Repo
        </button>
        <button class="action-button" type="button" @click="$emit('sync-all')">
          同步全部
        </button>
        <button
          class="action-button"
          type="button"
          @click="$emit('open-path', paths.reposDir)"
        >
          打开 Repos 目录
        </button>
      </div>
    </header>

    <div v-if="repos.length" class="repos-view__list">
      <article v-for="repo in repos" :key="repo.id" class="repos-view__card">
        <div class="repos-view__card-main">
          <div class="repos-view__card-head">
            <div>
              <h2>{{ repo.name }}</h2>
              <p>{{ repo.source }}</p>
            </div>
            <span class="repos-view__type">{{ repo.type }}</span>
          </div>

          <div class="repos-view__grid">
            <div>
              <span>本地路径</span>
              <strong>{{ repo.localPath }}</strong>
            </div>
            <div>
              <span>Skill 数量</span>
              <strong>{{ repo.skillCount }}</strong>
            </div>
            <div>
              <span>最近同步</span>
              <strong>{{ formatDateTime(repo.lastSyncedAt) }}</strong>
            </div>
            <div>
              <span>状态</span>
              <strong>{{ repo.status }}</strong>
            </div>
          </div>
        </div>

        <div class="repos-view__card-actions">
          <button
            class="action-button"
            type="button"
            @click="$emit('open-path', repo.localPath)"
          >
            打开目录
          </button>
          <button
            class="action-button"
            type="button"
            @click="$emit('sync-repo', repo.id)"
          >
            同步
          </button>
          <button
            class="action-button action-button--alert"
            type="button"
            @click="$emit('remove-repo', repo.id)"
          >
            删除
          </button>
        </div>
      </article>
    </div>

    <div v-else class="repos-view__empty">
      <h2>还没有 Repo</h2>
      <p>
        添加 GitHub、Git 或本地目录后，系统会自动扫描其中的 `SKILL.md` 并注册到
        Registry。
      </p>
    </div>
  </section>
</template>

<script setup>
import { formatDateTime } from "@/utils/formatters"

defineProps({
  paths: {
    type: Object,
    required: true
  },
  repos: {
    type: Array,
    required: true
  }
})

defineEmits(["add-repo", "sync-all", "open-path", "sync-repo", "remove-repo"])
</script>

<style scoped lang="less">
.repos-view {
  display: flex;
  flex-direction: column;
  gap: 12px;

  &__toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
  }

  &__eyebrow {
    margin: 0 0 5px;
    color: var(--color-text-soft);
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  &__toolbar h1 {
    margin: 0;
    font-size: 1.38rem;
    line-height: 1.2;
  }

  &__toolbar-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  &__list {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  &__card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 18px;
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  &__card-head h2 {
    margin: 0 0 8px;
    font-size: 1.14rem;
  }

  &__card-head p {
    margin: 0;
    color: var(--color-text-muted);
    line-height: 1.6;
    word-break: break-all;
  }

  &__type {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  &__grid div {
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__grid span {
    display: block;
    margin-bottom: 8px;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  &__grid strong {
    line-height: 1.6;
    word-break: break-all;
  }

  &__card-actions {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  &__empty {
    display: flex;
    min-height: 260px;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 8px;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    text-align: center;
  }

  &__empty h2 {
    margin: 0;
    font-size: 1rem;
  }

  &__empty p {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.84rem;
  }
}

.action-button {
  height: 36px;
  padding: 0 13px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.84rem;
  font-weight: 700;

  &--primary {
    border-color: var(--color-primary);
    background: var(--color-primary-solid);
    color: #fff;
  }

  &--alert {
    border-color: var(--color-danger-soft);
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }
}
</style>

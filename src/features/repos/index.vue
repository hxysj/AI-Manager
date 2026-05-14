<template>
  <section class="repos-view">
    <header class="repos-view__toolbar">
      <div>
        <p class="repos-view__eyebrow">Repo System</p>
        <h1>Skill Repos</h1>
      </div>
      <div class="repos-view__toolbar-actions">
        <button class="action-button action-button--primary" type="button" @click="$emit('add-repo')">
          添加 Repo
        </button>
        <button class="action-button" type="button" @click="$emit('sync-all')">
          同步全部
        </button>
        <button class="action-button" type="button" @click="$emit('open-path', paths.reposDir)">
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
          <button class="action-button" type="button" @click="$emit('open-path', repo.localPath)">
            打开目录
          </button>
          <button class="action-button" type="button" @click="$emit('sync-repo', repo.id)">
            同步
          </button>
          <button class="action-button action-button--alert" type="button" @click="$emit('remove-repo', repo.id)">
            删除
          </button>
        </div>
      </article>
    </div>

    <div v-else class="repos-view__empty">
      <h2>还没有 Repo</h2>
      <p>添加 GitHub、Git 或本地目录后，系统会自动扫描其中的 `SKILL.md` 并注册到 Registry。</p>
    </div>
  </section>
</template>

<script setup>
import { formatDateTime } from '@/utils/formatters'

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

defineEmits(['add-repo', 'sync-all', 'open-path', 'sync-repo', 'remove-repo'])
</script>

<style scoped lang="less">
.repos-view {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.repos-view__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
}

.repos-view__eyebrow {
  margin: 0 0 8px;
  color: rgba(43, 57, 84, 0.58);
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.18em;
  text-transform: uppercase;
}

.repos-view__toolbar h1 {
  margin: 0;
  font-family: 'Georgia', 'Times New Roman', serif;
  font-size: 2rem;
}

.repos-view__toolbar-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.repos-view__list {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.repos-view__card {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 18px;
  padding: 20px;
  border: 1px solid rgba(58, 69, 94, 0.1);
  border-radius: 24px;
  background: rgba(255, 255, 255, 0.9);
}

.repos-view__card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
}

.repos-view__card-head h2 {
  margin: 0 0 8px;
  font-size: 1.14rem;
}

.repos-view__card-head p {
  margin: 0;
  color: rgba(43, 57, 84, 0.64);
  line-height: 1.6;
  word-break: break-all;
}

.repos-view__type {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 6px 12px;
  border-radius: 999px;
  background: rgba(36, 94, 161, 0.1);
  color: #245ea1;
  font-size: 0.76rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.repos-view__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.repos-view__grid div {
  padding: 16px;
  border-radius: 18px;
  background: rgba(245, 239, 230, 0.72);
}

.repos-view__grid span {
  display: block;
  margin-bottom: 8px;
  color: rgba(43, 57, 84, 0.58);
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.repos-view__grid strong {
  line-height: 1.6;
  word-break: break-all;
}

.repos-view__card-actions {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.repos-view__empty {
  display: grid;
  min-height: 360px;
  place-items: center;
  border: 1px dashed rgba(58, 69, 94, 0.16);
  border-radius: 28px;
  background: rgba(255, 255, 255, 0.58);
  text-align: center;
}

.repos-view__empty h2 {
  margin: 0 0 10px;
  font-family: 'Georgia', 'Times New Roman', serif;
  font-size: 1.5rem;
}

.repos-view__empty p {
  margin: 0;
  color: rgba(43, 57, 84, 0.6);
}

.action-button {
  height: 42px;
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

.action-button--alert {
  border-color: rgba(220, 38, 38, 0.18);
  background: rgba(220, 38, 38, 0.08);
  color: #b91c1c;
}

@media (max-width: 1080px) {
  .repos-view__toolbar,
  .repos-view__card {
    grid-template-columns: 1fr;
  }

  .repos-view__toolbar-actions {
    justify-content: flex-start;
  }

  .repos-view__grid {
    grid-template-columns: 1fr;
  }

  .repos-view__card-actions {
    flex-direction: row;
    flex-wrap: wrap;
  }
}
</style>

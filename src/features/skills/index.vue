<template>
  <section class="skills-view">
    <header class="skills-view__toolbar">
      <div>
        <p class="skills-view__eyebrow">Skill Registry</p>
        <h1>Skills 管理</h1>
      </div>

      <div class="skills-view__toolbar-actions">
        <button
          class="action-button action-button--primary"
          type="button"
          @click="$emit('create-skill')"
        >
          <Plus class="action-button__icon" :size="16" />
          新建 Skill
        </button>
        <button
          class="action-button"
          type="button"
          @click="$emit('import-skills')"
        >
          <Download class="action-button__icon" :size="16" />
          从 CLI 导入
        </button>
        <button
          class="action-button"
          type="button"
          @click="$emit('open-path', paths.skillsDir)"
        >
          <FolderOpen class="action-button__icon" :size="16" />
          打开 Skills 目录
        </button>
        <button class="action-button" type="button" @click="$emit('refresh')">
          <RefreshCw class="action-button__icon" :size="16" />
          刷新扫描
        </button>
      </div>
    </header>

    <div class="skills-view__filters">
      <label class="skills-view__search">
        <span>搜索</span>
        <input
          v-model.trim="searchQuery"
          type="text"
          placeholder="name / tags / description / repo"
        />
      </label>

      <label class="skills-view__select">
        <span>状态</span>
        <select v-model="statusFilter">
          <option value="all">全部</option>
          <option value="installed">已安装</option>
          <option value="not-installed">未安装</option>
          <option value="broken-link">链接损坏</option>
          <option value="disabled">不可用</option>
        </select>
      </label>
    </div>

    <div class="skills-view__meta">
      <span>{{ filteredSkills.length }} / {{ skills.length }} 个 Skill</span>
      <span>Centralized Skill Source + Junction Mount</span>
    </div>

    <div v-if="filteredSkills.length" class="skills-view__list">
      <SkillCard
        v-for="skill in filteredSkills"
        :key="skill.id"
        :cli-targets="cliTargets"
        :skill="skill"
        @select="$emit('select-skill', skill)"
        @open-source="$emit('open-path', skill.sourcePath)"
        @install="$emit('install-skill', $event)"
        @uninstall="$emit('uninstall-skill', $event)"
      />
    </div>

    <div v-else class="skills-view__empty">
      <h2>没有匹配的 Skill</h2>
      <p>
        可以先在本地 `skills/` 目录创建 Skill，或者添加一个 Repo
        让系统自动扫描。
      </p>
    </div>
  </section>
</template>

<script setup>
import { computed, ref } from 'vue'
import { Download, FolderOpen, Plus, RefreshCw } from 'lucide-vue-next'
import SkillCard from './components/SkillCard.vue'

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  paths: {
    type: Object,
    required: true
  },
  skills: {
    type: Array,
    required: true
  }
})

defineEmits([
  'create-skill',
  'import-skills',
  'install-skill',
  'open-path',
  'refresh',
  'select-skill',
  'uninstall-skill'
])

const searchQuery = ref('')
const statusFilter = ref('all')

const filteredSkills = computed(() => {
  const keyword = searchQuery.value.toLowerCase()

  return props.skills.filter(skill => {
    const matchStatus =
      statusFilter.value === 'all' || skill.status === statusFilter.value
    const searchSource = [
      skill.name,
      skill.description,
      skill.repoName,
      ...(skill.tags || [])
    ]
      .join(' ')
      .toLowerCase()

    const matchKeyword = !keyword || searchSource.includes(keyword)
    return matchStatus && matchKeyword
  })
})
</script>

<style scoped lang="less">
.skills-view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.skills-view__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
}

.skills-view__eyebrow {
  margin: 0 0 8px;
  color: var(--color-text-soft);
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.18em;
  text-transform: uppercase;
}

.skills-view__toolbar h1 {
  margin: 0;
  font-size: 1.58rem;
  line-height: 1.2;
}

.skills-view__toolbar-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.skills-view__filters {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 220px;
  gap: 12px;
  padding: 14px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
}

.skills-view__search,
.skills-view__select {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.skills-view__search span,
.skills-view__select span {
  color: var(--color-text-muted);
  font-size: 0.8rem;
  font-weight: 700;
}

.skills-view__search input,
.skills-view__select select {
  height: 46px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  padding: 0 14px;
  color: var(--color-text);
  font: inherit;
}

.skills-view__meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--color-text-muted);
  font-size: 0.86rem;
}

.skills-view__list {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  overflow: hidden;
  background: var(--color-panel);
  box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
}

.skills-view__empty {
  display: grid;
  min-height: 360px;
  place-items: center;
  border: 1px dashed var(--color-line-strong);
  border-radius: 8px;
  background: var(--color-panel);
  text-align: center;
}

.skills-view__empty h2 {
  margin: 0 0 10px;
  font-size: 1.5rem;
}

.skills-view__empty p {
  margin: 0;
  color: var(--color-text-muted);
}

.action-button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 42px;
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

.action-button__icon {
  flex: 0 0 auto;
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

<template>
  <section class="skills-view">
    <section v-if="viewMode === 'local'" class="skills-view-local-page">
      <header class="skills-view-head">
        <div class="skills-view-title">
          <p class="skills-view-mark">Skill Registry</p>
          <h1 class="skills-view-title-text">Skills 管理</h1>
        </div>
        <div class="skills-view-actions">
          <button
            class="skills-view-button primary"
            type="button"
            @click="viewMode = 'repository-skills'"
          >
            <Library :size="16" />
            Skill 仓库
          </button>
          <button
            class="skills-view-button primary"
            type="button"
            @click="$emit('create-skill')"
          >
            <Plus :size="16" />
            新建 Skill
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('import-skills')"
          >
            <Download :size="16" />
            导入Skill
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('import-zip-skill')"
          >
            <Archive :size="16" />
            导入 zip
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('open-usage')"
          >
            <BarChart3 :size="16" />
            使用统计
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('open-path', paths.skillsDir)"
          >
            <FolderOpen :size="16" />
            打开 Skills 目录
          </button>
          <button class="skills-view-button" type="button" @click="$emit('refresh')">
            <RefreshCw :size="16" />
            刷新扫描
          </button>
        </div>
      </header>

      <section class="skills-view-filter-card">
        <label class="skills-view-field search">
          <span class="skills-view-field-label">搜索</span>
          <input
            v-model.trim="searchQuery"
            class="skills-view-field-control"
            type="text"
            placeholder="name / tags / description / repo"
          />
        </label>
        <label class="skills-view-field status">
          <span class="skills-view-field-label">状态</span>
          <select v-model="statusFilter" class="skills-view-field-control">
            <option value="all">全部</option>
            <option value="installed">已安装</option>
            <option value="not-installed">未安装</option>
            <option value="broken-link">链接损坏</option>
            <option value="disabled">不可用</option>
          </select>
        </label>
      </section>

      <div class="skills-view-result-head">
        <span>{{ filteredSkills.length }} / {{ skills.length }} 个 Skill</span>
        <span>Centralized Skill Source + Junction Mount</span>
      </div>

      <div v-if="filteredSkills.length" class="skills-view-list">
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

      <div v-else class="skills-view-empty">
        <strong class="skills-view-empty-title">没有匹配的 Skill</strong>
        <span class="skills-view-empty-desc">
          可以先在本地 skills 目录创建 Skill，或者调整搜索条件。
        </span>
      </div>
    </section>

    <SkillRepositoryList
      v-else-if="viewMode === 'repository-skills'"
      :repositories="skillRepositories"
      :skills="skills"
      @back="viewMode = 'local'"
      @install-skill="installRepositorySkill"
      @open-manager="viewMode = 'repositories'"
      @refresh="$emit('refresh-skill-repositories')"
    />

    <SkillRepositoryManager
      v-else
      :repositories="skillRepositories"
      @add-repository="$emit('add-skill-repository', $event)"
      @back="viewMode = 'repository-skills'"
      @refresh-repository="refreshRepository"
      @remove-repository="removeRepository"
    />
  </section>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  Archive,
  BarChart3,
  Download,
  FolderOpen,
  Library,
  Plus,
  RefreshCw
} from "lucide-vue-next"
import SkillCard from "./components/SkillCard.vue"
import SkillRepositoryList from "./components/SkillRepositoryList.vue"
import SkillRepositoryManager from "./components/SkillRepositoryManager.vue"

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  paths: {
    type: Object,
    required: true
  },
  skillRepositories: {
    type: Array,
    default: () => []
  },
  skills: {
    type: Array,
    required: true
  }
})

const emit = defineEmits([
  "add-skill-repository",
  "create-skill",
  "import-skills",
  "import-zip-skill",
  "install-repository-skill",
  "install-skill",
  "open-path",
  "open-usage",
  "refresh",
  "refresh-skill-repository",
  "refresh-skill-repositories",
  "remove-skill-repository",
  "select-skill",
  "uninstall-skill"
])

const viewMode = ref("local")
const searchQuery = ref("")
const statusFilter = ref("all")

const filteredSkills = computed(() => {
  const keyword = searchQuery.value.toLowerCase()

  return props.skills.filter(skill => {
    const matchStatus =
      statusFilter.value === "all" || skill.status === statusFilter.value
    const searchSource = [
      skill.name,
      skill.description,
      skill.repoName,
      ...(skill.tags || [])
    ]
      .join(" ")
      .toLowerCase()
    const matchKeyword = !keyword || searchSource.includes(keyword)

    return matchStatus && matchKeyword
  })
})

function refreshRepository(repository) {
  emit("refresh-skill-repository", {
    repositoryId: repository.id
  })
}

function removeRepository(repository) {
  emit("remove-skill-repository", {
    repositoryId: repository.id
  })
}

function installRepositorySkill(skill) {
  emit("install-repository-skill", {
    repositoryId: skill.repositoryId,
    skillId: skill.id
  })
}
</script>

<style scoped lang="less">
.skills-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;

  .skills-view-local-page {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
  }

  .skills-view-head {
    display: flex;
    flex: none;
    align-items: flex-start;
    gap: 12px;
  }

  .skills-view-title {
    display: flex;
    width: 96px;
    flex: none;
    flex-direction: column;
    gap: 3px;
  }

  .skills-view-mark {
    margin: 0;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .skills-view-title-text {
    margin: 0;
    color: var(--color-text);
    font-size: 1.26rem;
    line-height: 1.2;
    white-space: nowrap;
  }

  .skills-view-actions {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    row-gap: 8px;
    flex-wrap: wrap;
  }

  .skills-view-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 34px;
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

  .skills-view-button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  .skills-view-button.primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  .skills-view-button.primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }

  .skills-view-filter-card {
    display: flex;
    flex: none;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  .skills-view-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  .skills-view-field.search {
    flex: 1;
  }

  .skills-view-field.status {
    width: 220px;
    flex: none;
  }

  .skills-view-field-label {
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
  }

  .skills-view-field-control {
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

  .skills-view-field-control:focus {
    border-color: #8eb6d9;
    box-shadow: 0 0 0 3px rgba(47, 95, 145, 0.1);
  }

  .skills-view-result-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-weight: 700;
  }

  .skills-view-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  .skills-view-empty {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: #ffffff;
    color: var(--color-text-muted);
    text-align: center;
  }

  .skills-view-empty-title {
    color: var(--color-text);
    font-size: 0.98rem;
  }

  .skills-view-empty-desc {
    font-size: 0.82rem;
    line-height: 1.45;
  }
}
</style>

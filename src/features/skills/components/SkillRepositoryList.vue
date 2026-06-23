<template>
  <section class="skill-repository-list">
    <header class="skill-repository-list-head">
      <button
        class="skill-repository-list-back"
        type="button"
        @click="$emit('back')"
      >
        <ArrowLeft :size="15" />
        返回
      </button>
      <div class="skill-repository-list-title">
        <h1 class="skill-repository-list-title-text">Skill 仓库</h1>
        <span class="skill-repository-list-title-desc">
          {{ repositorySkillGroups.length }} 个 Skill，{{
            repositorySkillItems.length
          }}
          个来源
        </span>
      </div>
      <div class="skill-repository-list-actions">
        <button
          class="skill-repository-list-button"
          type="button"
          @click="$emit('refresh')"
        >
          <RefreshCw :size="16" />
          刷新
        </button>
        <button
          class="skill-repository-list-button primary"
          type="button"
          @click="$emit('open-manager')"
        >
          <Library :size="16" />
          仓库管理
        </button>
      </div>
    </header>

    <section class="skill-repository-list-filter-card">
      <label class="skill-repository-list-field search">
        <span class="skill-repository-list-field-label">搜索</span>
        <div class="skill-repository-list-search-control">
          <Search :size="15" />
          <input
            v-model.trim="searchQuery"
            class="skill-repository-list-search-input"
            type="text"
            placeholder="搜索名称、描述或路径"
          />
        </div>
      </label>
      <label class="skill-repository-list-field select">
        <span class="skill-repository-list-field-label">仓库</span>
        <select
          v-model="repositoryFilter"
          class="skill-repository-list-field-control"
        >
          <option value="all">全部仓库</option>
          <option
            v-for="repository in repositories"
            :key="repository.id"
            :value="repository.id"
          >
            {{ repository.name }}
          </option>
        </select>
      </label>
      <label class="skill-repository-list-field select">
        <span class="skill-repository-list-field-label">安装状态</span>
        <select
          v-model="installFilter"
          class="skill-repository-list-field-control"
        >
          <option value="all">全部状态</option>
          <option value="not-installed">未安装</option>
          <option value="installed">已安装</option>
        </select>
      </label>
    </section>

    <div class="skill-repository-list-result-head">
      <span>
        {{ filteredRepositorySkillItems.length }} /
        {{ repositorySkillGroups.length }} 个 Skill
      </span>
      <span v-if="repositoryErrorCount">
        {{ repositoryErrorCount }} 个仓库访问异常
      </span>
    </div>

    <div
      v-if="filteredRepositorySkillItems.length"
      class="skill-repository-list-grid"
    >
      <article
        v-for="skill in filteredRepositorySkillItems"
        :key="skill.skillKey"
        class="skill-repository-list-card"
      >
        <div class="skill-repository-list-card-head">
          <strong class="skill-repository-list-card-name" :title="skill.name">
            {{ skill.name }}
          </strong>
          <span
            :class="[
              'skill-repository-list-card-status',
              {
                disabled: isRepositorySkillDisabled(skill),
                installed: isRepositorySkillInstalled(skill)
              }
            ]"
          >
            {{ formatRepositorySkillStatus(skill) }}
          </span>
        </div>
        <p class="skill-repository-list-card-desc" :title="skill.description">
          {{ skill.description || "未提供描述" }}
        </p>
        <div class="skill-repository-list-card-path" :title="skill.displayPath">
          {{ skill.displayPath }}
        </div>
        <div class="skill-repository-list-card-tags">
          <span
            v-for="source in skill.repositorySources"
            :key="source.repositoryId"
            class="skill-repository-list-card-repository"
            :title="source.repositoryName"
          >
            <Library :size="13" />
          </span>
          <span class="skill-repository-list-card-tag">
            {{ skill.repositorySources.length }} 个仓库
          </span>
          <span class="skill-repository-list-card-tag">
            {{ skill.repositoryBranch || "默认分支" }}
          </span>
        </div>
        <div class="skill-repository-list-card-actions">
          <button
            class="skill-repository-list-card-action"
            type="button"
            @click="repositoryDetailSkill = skill"
          >
            <Eye :size="15" />
            查看
          </button>
          <button
            :class="[
              'skill-repository-list-card-action',
              'install',
              {
                installed: isRepositorySkillInstalled(skill)
              }
            ]"
            type="button"
            :disabled="
              isRepositorySkillInstalled(skill) || isRepositorySkillDisabled(skill)
            "
            @click="$emit('install-skill', skill)"
          >
            <Download :size="15" />
            {{ formatRepositorySkillAction(skill) }}
          </button>
        </div>
      </article>
    </div>

    <div v-else class="skill-repository-list-empty">
      <strong class="skill-repository-list-empty-title">{{
        emptyTitle
      }}</strong>
      <span class="skill-repository-list-empty-desc">{{
        emptyDescription
      }}</span>
    </div>

    <section
      v-if="repositoryDetailSkill"
      class="skill-repository-list-detail-layer"
    >
      <div class="skill-repository-list-detail-panel">
        <header class="skill-repository-list-detail-head">
          <button
            class="skill-repository-list-back"
            type="button"
            @click="repositoryDetailSkill = null"
          >
            <ArrowLeft :size="15" />
            返回
          </button>
          <div class="skill-repository-list-detail-title">
            <strong class="skill-repository-list-detail-name">
              {{ repositoryDetailSkill.name }}
            </strong>
            <span class="skill-repository-list-detail-repo">
              {{ repositoryDetailSkill.repositorySources.length }} 个仓库来源
            </span>
          </div>
          <button
            :class="[
              'skill-repository-list-button',
              'primary',
              {
                installed: isRepositorySkillInstalled(repositoryDetailSkill)
              }
            ]"
            type="button"
            :disabled="
              isRepositorySkillInstalled(repositoryDetailSkill) ||
              isRepositorySkillDisabled(repositoryDetailSkill)
            "
            @click="$emit('install-skill', repositoryDetailSkill)"
          >
            <Download :size="16" />
            {{ formatRepositorySkillAction(repositoryDetailSkill) }}
          </button>
        </header>

        <div class="skill-repository-list-detail-body">
          <section class="skill-repository-list-detail-section">
            <span class="skill-repository-list-detail-label">来源仓库</span>
            <div class="skill-repository-list-detail-repositories">
              <span
                v-for="source in repositoryDetailSkill.repositorySources"
                :key="source.repositoryId"
                class="skill-repository-list-detail-repository"
                :title="source.repositorySource"
              >
                <Library :size="13" />
                {{ source.repositoryName }}
              </span>
            </div>
          </section>
          <section class="skill-repository-list-detail-section">
            <span class="skill-repository-list-detail-label">描述</span>
            <p class="skill-repository-list-detail-text">
              {{ repositoryDetailSkill.description || "未提供描述。" }}
            </p>
          </section>
          <section class="skill-repository-list-detail-section">
            <span class="skill-repository-list-detail-label">入口</span>
            <p class="skill-repository-list-detail-text">
              {{ repositoryDetailSkill.entry }}
            </p>
          </section>
          <section class="skill-repository-list-detail-section">
            <span class="skill-repository-list-detail-label">目录</span>
            <p class="skill-repository-list-detail-text">
              {{ repositoryDetailSkill.displayPath }}
            </p>
          </section>
          <section class="skill-repository-list-detail-section">
            <span class="skill-repository-list-detail-label">标签</span>
            <div class="skill-repository-list-detail-tags">
              <strong
                v-for="tag in repositoryDetailSkill.tags"
                :key="tag"
                class="skill-repository-list-detail-tag"
              >
                {{ tag }}
              </strong>
              <strong
                v-if="!repositoryDetailSkill.tags.length"
                class="skill-repository-list-detail-tag"
              >
                暂无标签
              </strong>
            </div>
          </section>
          <section class="skill-repository-list-detail-section content">
            <span class="skill-repository-list-detail-label">SKILL.md</span>
            <pre class="skill-repository-list-detail-content">{{
              repositoryDetailSkill.content || "未提供详细内容。"
            }}</pre>
          </section>
        </div>
      </div>
    </section>
  </section>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  ArrowLeft,
  Download,
  Eye,
  Library,
  RefreshCw,
  Search
} from "lucide-vue-next"

const props = defineProps({
  repositories: {
    type: Array,
    default: () => []
  },
  skills: {
    type: Array,
    required: true
  }
})

defineEmits(["back", "install-skill", "open-manager", "refresh"])

const searchQuery = ref("")
const repositoryFilter = ref("all")
const installFilter = ref("all")
const repositoryDetailSkill = ref(null)

const repositorySkillItems = computed(() => {
  return props.repositories.flatMap((repository) =>
    (Array.isArray(repository.skills) ? repository.skills : []).map((skill) => ({
      ...skill,
      repositoryId: repository.id,
      repositoryName: repository.name,
      repositoryBranch: repository.branch,
      repositorySource: repository.source
    }))
  )
})

const repositorySkillGroups = computed(() => {
  const groups = []
  const groupMap = new Map()

  for (const skill of repositorySkillItems.value) {
    const skillKey = skill.name.toLowerCase()
    const source = {
      id: skill.id,
      repositoryId: skill.repositoryId,
      repositoryName: skill.repositoryName,
      repositoryBranch: skill.repositoryBranch,
      repositorySource: skill.repositorySource,
      displayPath: skill.displayPath,
      skillPath: skill.skillPath
    }
    const group = groupMap.get(skillKey)

    if (!group) {
      const nextGroup = {
        ...skill,
        skillKey,
        sources: [source],
        repositorySources: [source]
      }

      groups.push(nextGroup)
      groupMap.set(skillKey, nextGroup)
      continue
    }

    group.sources.push(source)

    if (
      !group.repositorySources.some(
        (repositorySource) =>
          repositorySource.repositoryId === skill.repositoryId
      )
    ) {
      group.repositorySources.push(source)
    }

    group.tags = [...new Set([...(group.tags || []), ...(skill.tags || [])])]
  }

  return groups
})

const repositoryErrorCount = computed(() => {
  return props.repositories.filter(
    (repository) => repository.status === "error"
  ).length
})

const filteredRepositorySkillItems = computed(() => {
  const keyword = searchQuery.value.toLowerCase()

  return repositorySkillGroups.value
    .map((skill) => {
      const matchedSources =
        repositoryFilter.value === "all"
          ? skill.sources
          : skill.sources.filter(
              (source) => source.repositoryId === repositoryFilter.value
            )

      if (!matchedSources.length) {
        return null
      }

      const selectedSource = matchedSources[0]

      return {
        ...skill,
        id: selectedSource.id,
        repositoryId: selectedSource.repositoryId,
        repositoryName: selectedSource.repositoryName,
        repositoryBranch: selectedSource.repositoryBranch,
        repositorySource: selectedSource.repositorySource,
        skillPath: selectedSource.skillPath,
        displayPath: selectedSource.displayPath
      }
    })
    .filter((skill) => {
      if (!skill) {
        return false
      }

      const installed = isRepositorySkillInstalled(skill)
      const searchSource = [
        skill.name,
        skill.description,
        ...skill.sources.map((source) => source.displayPath),
        ...skill.repositorySources.map((source) => source.repositoryName),
        ...(skill.tags || [])
      ]
        .join(" ")
        .toLowerCase()
      const matchKeyword = !keyword || searchSource.includes(keyword)
      const matchInstall =
        installFilter.value === "all" ||
        (installFilter.value === "installed" && installed) ||
        (installFilter.value === "not-installed" && !installed)

      return matchKeyword && matchInstall
    })
})

const emptyTitle = computed(() => {
  if (!props.repositories.length) {
    return "暂无技能仓库"
  }

  return "没有匹配的 Skill"
})

const emptyDescription = computed(() => {
  if (!props.repositories.length) {
    return "进入仓库管理添加 GitHub 仓库后会自动扫描。"
  }

  return "调整搜索内容、仓库或安装状态后再试。"
})

function isRepositorySkillInstalled(skill) {
  return props.skills.some((item) => item.name === skill.name && !item.disabled)
}

function isRepositorySkillDisabled(skill) {
  return props.skills.some((item) => item.name === skill.name && item.disabled)
}

function formatRepositorySkillStatus(skill) {
  if (isRepositorySkillDisabled(skill)) {
    return "已禁用"
  }

  return isRepositorySkillInstalled(skill) ? "已安装" : "未安装"
}

function formatRepositorySkillAction(skill) {
  if (isRepositorySkillDisabled(skill)) {
    return "已禁用"
  }

  return isRepositorySkillInstalled(skill) ? "已安装" : "安装"
}
</script>

<style scoped lang="less">
.skill-repository-list {
  position: relative;
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;

  .skill-repository-list-head {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
  }

  .skill-repository-list-title {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  .skill-repository-list-title-text {
    margin: 0;
    color: var(--color-text);
    font-size: 1.26rem;
    line-height: 1.2;
  }

  .skill-repository-list-title-desc {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .skill-repository-list-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  .skill-repository-list-back,
  .skill-repository-list-button {
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

  .skill-repository-list-back {
    padding: 0 11px;
  }

  .skill-repository-list-button {
    padding: 0 12px;
  }

  .skill-repository-list-back:hover,
  .skill-repository-list-button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  .skill-repository-list-button:disabled {
    cursor: default;
    opacity: 0.62;
  }

  .skill-repository-list-button.primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  .skill-repository-list-button.primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }

  .skill-repository-list-button.installed {
    border-color: #91c7aa;
    background: #e9f7ef;
    color: #16834f;
  }

  .skill-repository-list-filter-card {
    display: flex;
    flex: none;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  .skill-repository-list-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  .skill-repository-list-field.search {
    flex: 1;
  }

  .skill-repository-list-field.select {
    width: 190px;
    flex: none;
  }

  .skill-repository-list-field-label {
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
  }

  .skill-repository-list-field-control {
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

  .skill-repository-list-field-control:focus {
    border-color: #8eb6d9;
    box-shadow: 0 0 0 3px rgba(47, 95, 145, 0.1);
  }

  .skill-repository-list-search-control {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
    color: var(--color-text-soft);
  }

  .skill-repository-list-search-control:focus-within {
    border-color: #8eb6d9;
    box-shadow: 0 0 0 3px rgba(47, 95, 145, 0.1);
  }

  .skill-repository-list-search-input {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: 0.84rem;
    outline: none;
  }

  .skill-repository-list-result-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-weight: 700;
  }

  .skill-repository-list-grid {
    display: grid;
    min-height: 0;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    overflow: auto;
    padding-right: 2px;
  }

  .skill-repository-list-card {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 10px 26px rgba(34, 56, 83, 0.045);
  }

  .skill-repository-list-card-head {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .skill-repository-list-card-name {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.94rem;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-repository-list-card-status {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .skill-repository-list-card-status.installed {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .skill-repository-list-card-status.disabled {
    background: #edf3f8;
    color: var(--color-text-soft);
  }

  .skill-repository-list-card-desc {
    display: -webkit-box;
    min-height: 38px;
    overflow: hidden;
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    line-height: 1.5;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
  }

  .skill-repository-list-card-path {
    min-height: 18px;
    overflow: hidden;
    color: var(--color-text-soft);
    font-size: 0.76rem;
    font-weight: 700;
    line-height: 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-repository-list-card-tags {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 6px;
  }

  .skill-repository-list-card-repository {
    display: inline-grid;
    width: 24px;
    height: 24px;
    flex: none;
    place-items: center;
    border: 1px solid #c9d9e6;
    border-radius: 999px;
    background: #eef5fb;
    color: var(--color-primary);
  }

  .skill-repository-list-card-tag {
    display: inline-flex;
    max-width: 180px;
    align-items: center;
    height: 24px;
    padding: 0 8px;
    overflow: hidden;
    border: 1px solid #d8e4ee;
    border-radius: 999px;
    background: #f6f9fc;
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-repository-list-card-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: auto;
    padding-top: 4px;
  }

  .skill-repository-list-card-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    padding: 0 11px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.78rem;
    font-weight: 700;
  }

  .skill-repository-list-card-action:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  .skill-repository-list-card-action.install {
    border-color: #14945f;
    background: #14945f;
    color: #ffffff;
  }

  .skill-repository-list-card-action.install.installed {
    border-color: #91c7aa;
    background: #e9f7ef;
    color: #16834f;
  }

  .skill-repository-list-card-action:disabled {
    cursor: default;
  }

  .skill-repository-list-empty {
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

  .skill-repository-list-empty-title {
    color: var(--color-text);
    font-size: 0.98rem;
  }

  .skill-repository-list-empty-desc {
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .skill-repository-list-detail-layer {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(247, 250, 252, 0.86);
    backdrop-filter: blur(2px);
  }

  .skill-repository-list-detail-panel {
    display: flex;
    width: 720px;
    max-height: 100%;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 24px 64px rgba(15, 23, 42, 0.18);
  }

  .skill-repository-list-detail-head {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
    padding: 14px;
    border-bottom: 1px solid var(--color-line);
    background: #fbfcfd;
  }

  .skill-repository-list-detail-title {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  .skill-repository-list-detail-name {
    overflow: hidden;
    color: var(--color-text);
    font-size: 1rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-repository-list-detail-repo {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-repository-list-detail-body {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    padding: 14px;
  }

  .skill-repository-list-detail-section {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 11px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
  }

  .skill-repository-list-detail-section.content {
    flex: none;
  }

  .skill-repository-list-detail-label {
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .skill-repository-list-detail-text {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.84rem;
    line-height: 1.55;
    word-break: break-word;
  }

  .skill-repository-list-detail-content {
    max-height: 260px;
    margin: 0;
    overflow: auto;
    color: var(--color-text-muted);
    font-family: Consolas, "Liberation Mono", monospace;
    font-size: 0.78rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .skill-repository-list-detail-tags {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .skill-repository-list-detail-repositories {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .skill-repository-list-detail-repository {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 26px;
    padding: 0 9px;
    border: 1px solid #d8e4ee;
    border-radius: 999px;
    background: #ffffff;
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
  }

  .skill-repository-list-detail-tag {
    display: inline-flex;
    align-items: center;
    min-height: 24px;
    padding: 0 8px;
    border: 1px solid var(--color-line);
    border-radius: 999px;
    background: #ffffff;
    color: var(--color-text-muted);
    font-size: 0.72rem;
  }
}
</style>

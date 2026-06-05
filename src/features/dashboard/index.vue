<template>
  <section class="dashboard-view">
    <header class="dashboard-view__hero">
      <div>
        <p class="dashboard-view__eyebrow">
          Stage V1 / CLI Detection + Skill System
        </p>
        <h1>统一管理 AI CLI 的 Skills、Configs 与 Repo 挂载状态</h1>
        <p class="dashboard-view__summary">
          当前实现围绕文档定义的集中式 Skill Source、CLI 自动探测、Metadata
          解析、 Registry 同步与 Windows junction 挂载展开。
        </p>
      </div>
      <div class="dashboard-view__hero-actions">
        <button
          class="action-button action-button--primary"
          type="button"
          @click="$emit('refresh')"
        >
          刷新索引
        </button>
        <button
          class="action-button"
          type="button"
          @click="$emit('open-path', paths.workspaceRoot)"
        >
          打开工作区
        </button>
      </div>
    </header>

    <div class="dashboard-view__metrics">
      <article
        v-for="item in metrics"
        :key="item.label"
        class="dashboard-view__metric-card"
      >
        <span>{{ item.label }}</span>
        <strong>{{ item.value }}</strong>
        <small>{{ item.hint }}</small>
      </article>
    </div>

    <div class="dashboard-view__grid">
      <section class="dashboard-view__panel">
        <div class="dashboard-view__panel-header">
          <h2>工作区路径</h2>
          <span>{{ formatDateTime(refreshedAt) }}</span>
        </div>
        <ul class="dashboard-view__path-list">
          <li>
            <span>Skills</span>
            <button type="button" @click="$emit('open-path', paths.skillsDir)">
              {{ paths.skillsDir }}
            </button>
          </li>
          <li>
            <span>Repos</span>
            <button type="button" @click="$emit('open-path', paths.reposDir)">
              {{ paths.reposDir }}
            </button>
          </li>
          <li>
            <span>Storage</span>
            <button type="button" @click="$emit('open-path', paths.storageDir)">
              {{ paths.storageDir }}
            </button>
          </li>
        </ul>
      </section>

      <section class="dashboard-view__panel">
        <div class="dashboard-view__panel-header">
          <h2>CLI 探测结果</h2>
          <span>{{ cliTargets.length }} 个目标</span>
        </div>
        <div class="dashboard-view__cli-list">
          <article
            v-for="item in cliTargets"
            :key="item.id"
            class="dashboard-view__cli-card"
          >
            <div>
              <strong>{{ item.name }}</strong>
              <p>
                {{
                  item.installed
                    ? "配置目录或二进制已找到"
                    : "未发现配置目录与可执行文件"
                }}
              </p>
            </div>
            <div class="dashboard-view__cli-meta">
              <span
                :class="[
                  'state-pill',
                  { 'state-pill--offline': !item.installed }
                ]"
              >
                {{ item.installed ? "在线" : "离线" }}
              </span>
              <small>{{ item.version || item.configPath }}</small>
            </div>
          </article>
        </div>
      </section>

      <section class="dashboard-view__panel">
        <div class="dashboard-view__panel-header">
          <h2>系统诊断</h2>
          <span>{{ diagnostics.length }} 条</span>
        </div>
        <div v-if="diagnostics.length" class="dashboard-view__diagnostics">
          <article
            v-for="item in diagnostics"
            :key="`${item.type}-${item.sourcePath}`"
            class="dashboard-view__diagnostic"
          >
            <strong>{{ item.type }}</strong>
            <p>{{ item.message }}</p>
            <small>{{ item.sourcePath }}</small>
          </article>
        </div>
        <div v-else class="dashboard-view__empty">
          当前没有扫描异常，Registry 状态干净。
        </div>
      </section>
    </div>
  </section>
</template>

<script setup>
import { computed } from "vue"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  diagnostics: {
    type: Array,
    required: true
  },
  paths: {
    type: Object,
    required: true
  },
  refreshedAt: {
    type: Number,
    required: true
  },
  repos: {
    type: Array,
    required: true
  },
  skills: {
    type: Array,
    required: true
  }
})

defineEmits(["refresh", "open-path"])

const metrics = computed(() => {
  const installedCliCount = props.cliTargets.filter(
    (item) => item.installed
  ).length
  const installedSkillCount = props.skills.filter(
    (item) => item.status === "installed"
  ).length

  return [
    {
      label: "CLI Targets",
      value: installedCliCount,
      hint: `${props.cliTargets.length} 个定义目标`
    },
    {
      label: "Managed Skills",
      value: props.skills.length,
      hint: `${installedSkillCount} 个已挂载`
    },
    {
      label: "Repositories",
      value: props.repos.length,
      hint: "支持 github / git / local"
    },
    {
      label: "Diagnostics",
      value: props.diagnostics.length,
      hint: "重复名称与 Metadata 错误"
    }
  ]
})
</script>

<style scoped lang="less">
.dashboard-view {
  display: flex;
  flex-direction: column;
  gap: 16px;

  &__hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 24px;
    padding: 22px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__eyebrow {
    margin: 0 0 10px;
    color: var(--color-text-soft);
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }

  &__hero h1 {
    margin: 0;
    font-size: 2rem;
    line-height: 1.15;
  }

  &__summary {
    max-width: 760px;
    margin: 14px 0 0;
    color: var(--color-text-muted);
    line-height: 1.7;
  }

  &__hero-actions {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  &__metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 16px;
  }

  &__metric-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__metric-card span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  &__metric-card strong {
    font-size: 2.2rem;
    line-height: 1;
  }

  &__metric-card small {
    color: var(--color-text-muted);
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 16px;
  }

  &__panel {
    display: flex;
    min-height: 280px;
    flex-direction: column;
    gap: 16px;
    padding: 22px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__panel-header h2 {
    margin: 0;
    font-size: 1.2rem;
  }

  &__panel-header span {
    color: var(--color-text-muted);
    font-size: 0.82rem;
  }

  &__path-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 0;
    margin: 0;
    list-style: none;
  }

  &__path-list li {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__path-list span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  &__path-list button {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.95rem;
    line-height: 1.5;
    text-align: left;
    word-break: break-all;
  }

  &__cli-list,
  &__diagnostics {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  &__cli-card,
  &__diagnostic {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__cli-card strong,
  &__diagnostic strong {
    display: block;
    margin-bottom: 4px;
    font-size: 0.98rem;
  }

  &__cli-card p,
  &__diagnostic p {
    margin: 0;
    color: var(--color-text-muted);
    line-height: 1.5;
  }

  &__cli-meta {
    display: flex;
    min-width: 160px;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
  }

  &__cli-meta small,
  &__diagnostic small {
    color: var(--color-text-soft);
    font-size: 0.78rem;
    text-align: right;
    word-break: break-all;
  }

  &__empty {
    display: grid;
    min-height: 180px;
    place-items: center;
    border-radius: 8px;
    border: 1px dashed var(--color-line-strong);
    color: var(--color-text-muted);
  }
}

.state-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 7px 12px;
  border-radius: 999px;
  background: var(--color-success-soft);
  color: var(--color-success);
  font-size: 0.76rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;

  &--offline {
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
  }
}

.action-button {
  height: 42px;
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

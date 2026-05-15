<template>
  <div class="app-shell">
    <AppSidebar
      :active-view="activeView"
      :cli-targets="state.cliTargets"
      :collapsed="sidebarCollapsed"
      :nav-items="navItems"
      @toggle="sidebarCollapsed = !sidebarCollapsed"
      @select-view="activeView = $event"
    />

    <main class="app-shell__main">
      <div class="app-shell__status">
        <div class="app-shell__status-left">
          <strong>{{ statusHeadline }}</strong>
          <span>{{ statusSummary }}</span>
        </div>
        <div class="app-shell__status-right">
          <button
            class="status-button"
            type="button"
            @click="openPath(state.paths.workspaceRoot)"
          >
            Workspace
          </button>
          <button
            class="status-button"
            type="button"
            @click="refreshState"
            :disabled="pending"
          >
            {{ pending ? "处理中..." : "立即刷新" }}
          </button>
        </div>
      </div>

      <div v-if="errorMessage" class="app-shell__error">
        <strong>操作失败</strong>
        <p>{{ errorMessage }}</p>
      </div>

      <section class="app-shell__content">
        <DashboardView
          v-if="activeView === 'dashboard'"
          :cli-targets="state.cliTargets"
          :diagnostics="state.diagnostics"
          :paths="state.paths"
          :refreshed-at="state.refreshedAt"
          :repos="state.repos"
          :skills="state.skills"
          @refresh="refreshState"
          @open-path="openPath"
        />

        <SkillsView
          v-else-if="activeView === 'skills'"
          :cli-targets="state.cliTargets"
          :paths="state.paths"
          :skills="state.skills"
          @create-skill="showCreateSkill = true"
          @import-skills="importSkillsFromCli"
          @open-path="openPath"
          @refresh="refreshState"
          @select-skill="selectSkill"
        />

        <ReposView
          v-else-if="activeView === 'repos'"
          :paths="state.paths"
          :repos="state.repos"
          @add-repo="showAddRepo = true"
          @open-path="openPath"
          @remove-repo="removeRepo"
          @sync-all="syncAllRepos"
          @sync-repo="syncRepo"
        />

        <section v-else class="app-shell__placeholder">
          <h1>{{ currentPlaceholder.title }}</h1>
          <p>{{ currentPlaceholder.description }}</p>
          <button
            class="status-button"
            type="button"
            @click="activeView = currentPlaceholder.backTo"
          >
            返回
            {{
              navItems.find((item) => item.id === currentPlaceholder.backTo)
                ?.label
            }}
          </button>
        </section>
      </section>
    </main>

    <SkillDrawer
      :cli-targets="state.cliTargets"
      :skill="selectedSkill"
      @close="selectedSkillName = ''"
      @install="installSkill"
      @uninstall="uninstallSkill"
      @repair="repairSkill"
      @open-path="openPath"
    />

    <CreateSkillModal
      v-if="showCreateSkill"
      @close="showCreateSkill = false"
      @submit="createSkill"
    />

    <AddRepoModal
      v-if="showAddRepo"
      @close="showAddRepo = false"
      @submit="addRepo"
    />
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue"
import AppSidebar from "@/components/AppSidebar.vue"
import DashboardView from "@/features/dashboard/index.vue"
import SkillsView from "@/features/skills/index.vue"
import ReposView from "@/features/repos/index.vue"
import SkillDrawer from "@/features/skills/components/SkillDrawer.vue"
import CreateSkillModal from "@/features/skills/components/CreateSkillModal.vue"
import AddRepoModal from "@/features/repos/components/AddRepoModal.vue"

const navItems = [
  { id: "dashboard", label: "Dashboard", icon: "◈" },
  { id: "skills", label: "Skills", icon: "✦" },
  { id: "repos", label: "Repos", icon: "▣" },
  { id: "sessions", label: "Sessions", icon: "◎" },
  { id: "providers", label: "Providers", icon: "◇" },
  { id: "rules", label: "Rules", icon: "≋" },
  { id: "workspace", label: "Workspace", icon: "⌘" },
  { id: "settings", label: "Settings", icon: "⚙" }
]

const placeholderMap = {
  sessions: {
    title: "Session System 预留",
    description:
      "文档中该模块当前处于预留阶段，前端保留入口，后续可接入 Claude/Codex Session 聚合。",
    backTo: "dashboard"
  },
  providers: {
    title: "Providers 视图待扩展",
    description:
      "当前版本聚焦 CLI Detection 与 Skill System，Provider 层尚未写入后端模型。",
    backTo: "skills"
  },
  rules: {
    title: "Rules 视图待扩展",
    description:
      "规则系统后续可以直接消费 Registry 与 Skill 元数据，这一版只保留导航骨架。",
    backTo: "skills"
  },
  workspace: {
    title: "Workspace 视图待扩展",
    description:
      "当前工作区路径已经由主进程管理，可在 Dashboard 中直接打开相关目录。",
    backTo: "dashboard"
  },
  settings: {
    title: "Settings 视图待扩展",
    description:
      "后续可把扫描深度、忽略目录、刷新策略和默认 Repo 位置放到设置页。",
    backTo: "dashboard"
  }
}

const state = reactive({
  cliTargets: [],
  skills: [],
  repos: [],
  diagnostics: [],
  paths: {
    workspaceRoot: "",
    skillsDir: "",
    reposDir: "",
    storageDir: ""
  },
  refreshedAt: 0
})

const activeView = ref("dashboard")
const pending = ref(false)
const errorMessage = ref("")
const sidebarCollapsed = ref(false)
const selectedSkillName = ref("")
const showCreateSkill = ref(false)
const showAddRepo = ref(false)

let unsubscribe = null

const selectedSkill = computed(() => {
  return (
    state.skills.find((item) => item.name === selectedSkillName.value) || null
  )
})

const currentPlaceholder = computed(() => {
  return placeholderMap[activeView.value] || placeholderMap.sessions
})

const statusHeadline = computed(() => {
  const onlineCount = state.cliTargets.filter((item) => item.installed).length
  return `已检测 ${onlineCount} / ${state.cliTargets.length} 个 CLI，索引 ${state.skills.length} 个 Skill`
})

const statusSummary = computed(() => {
  const brokenCount = state.skills.filter(
    (item) => item.status === "broken-link"
  ).length
  const repoCount = state.repos.length
  return `Broken Links ${brokenCount} · Repos ${repoCount} · Diagnostics ${state.diagnostics.length}`
})

async function bootstrap() {
  pending.value = true
  errorMessage.value = ""

  try {
    updateState(await window.aiManager.bootstrap())
    unsubscribe = window.aiManager.onStateChanged((nextState) => {
      updateState(nextState)
    })
  } catch (error) {
    errorMessage.value = error.message || String(error)
  } finally {
    pending.value = false
  }
}

function updateState(nextState) {
  state.cliTargets = nextState.cliTargets || []
  state.skills = nextState.skills || []
  state.repos = nextState.repos || []
  state.diagnostics = nextState.diagnostics || []
  state.paths = nextState.paths || state.paths
  state.refreshedAt = nextState.refreshedAt || 0

  if (
    selectedSkillName.value &&
    !state.skills.find((item) => item.name === selectedSkillName.value)
  ) {
    selectedSkillName.value = ""
  }
}

async function runAction(action) {
  pending.value = true
  errorMessage.value = ""

  try {
    const nextState = await action()
    if (nextState && typeof nextState === "object" && "skills" in nextState) {
      updateState(nextState)
    }
    return true
  } catch (error) {
    errorMessage.value = error.message || String(error)
    return false
  } finally {
    pending.value = false
  }
}

function selectSkill(skill) {
  selectedSkillName.value = skill.name
}

async function refreshState() {
  await runAction(() => window.aiManager.refresh())
}

async function createSkill(payload) {
  const success = await runAction(() => window.aiManager.createSkill(payload))

  if (success) {
    showCreateSkill.value = false
    activeView.value = "skills"
  }
}

async function importSkillsFromCli() {
  const success = await runAction(() => window.aiManager.importSkillsFromCli())

  if (success) {
    activeView.value = "skills"
  }
}

async function installSkill(payload) {
  await runAction(() => window.aiManager.installSkill(payload))
}

async function uninstallSkill(payload) {
  await runAction(() => window.aiManager.uninstallSkill(payload))
}

async function repairSkill(payload) {
  await runAction(() => window.aiManager.repairSkill(payload))
}

async function addRepo(payload) {
  const success = await runAction(() => window.aiManager.addRepo(payload))

  if (success) {
    showAddRepo.value = false
    activeView.value = "repos"
  }
}

async function syncRepo(repoId) {
  await runAction(() => window.aiManager.syncRepo({ repoId }))
}

async function syncAllRepos() {
  await runAction(() => window.aiManager.syncAllRepos())
}

async function removeRepo(repoId) {
  const shouldContinue = window.confirm(
    "删除 Repo 会先卸载它挂载到 CLI 的 Skill 链接，是否继续？"
  )

  if (!shouldContinue) {
    return
  }

  await runAction(() => window.aiManager.removeRepo({ repoId }))
}

async function openPath(targetPath) {
  if (!targetPath) {
    return
  }

  pending.value = true
  errorMessage.value = ""

  try {
    await window.aiManager.openPath({ targetPath })
  } catch (error) {
    errorMessage.value = error.message || String(error)
  } finally {
    pending.value = false
  }
}

onMounted(() => {
  bootstrap()
})

onBeforeUnmount(() => {
  if (typeof unsubscribe === "function") {
    unsubscribe()
  }
})
</script>

<style scoped lang="less">
.app-shell {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  height: 100vh;
  min-height: 0;
}

.app-shell__main {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  padding: 18px;
  gap: 14px;
  background: var(--color-page);
}

.app-shell__status {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
  padding: 12px 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
}

.app-shell__status-left {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.app-shell__status-left strong {
  font-size: 1rem;
}

.app-shell__status-left span {
  color: var(--color-text-muted);
  font-size: 0.86rem;
}

.app-shell__status-right {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.app-shell__content {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding-right: 6px;
}

.app-shell__error {
  padding: 16px 18px;
  border: 1px solid var(--color-danger-soft);
  border-radius: 8px;
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.app-shell__error strong {
  display: block;
  margin-bottom: 6px;
}

.app-shell__error p {
  margin: 0;
  line-height: 1.6;
}

.app-shell__placeholder {
  display: grid;
  min-height: 520px;
  place-items: center;
  border: 1px dashed var(--color-line-strong);
  border-radius: 8px;
  background: var(--color-panel);
  padding: 32px;
  text-align: center;
}

.app-shell__placeholder h1 {
  margin: 0 0 12px;
  font-size: 2rem;
}

.app-shell__placeholder p {
  max-width: 680px;
  margin: 0 0 18px;
  color: var(--color-text-muted);
  line-height: 1.7;
}

.status-button {
  height: 40px;
  padding: 0 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;
}

.status-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}
</style>

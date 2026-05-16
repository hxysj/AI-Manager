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
      <section class="app-shell__content">
        <SkillsView
          v-if="activeView === 'skills'"
          :cli-targets="state.cliTargets"
          :paths="state.paths"
          :skills="state.skills"
          @create-skill="showCreateSkill = true"
          @import-skills="importSkillsFromCli"
          @import-zip-skill="importSkillFromZip"
          @install-skill="installSkill"
          @open-path="openPath"
          @refresh="refreshState"
          @select-skill="selectSkill"
          @uninstall-skill="uninstallSkill"
        />

        <SessionsView
          v-else-if="activeView === 'sessions'"
          :paths="state.paths"
          :sessions="state.sessions"
          @delete-session="deleteSession"
          @open-path="openPath"
          @refresh="refreshState"
        />

        <ProvidersView
          v-else-if="activeView === 'providers'"
          :cli-targets="state.cliTargets"
          :pending="pending"
          :codex-accounts="state.codexAccounts"
          :codex-login-state="state.codexLoginState"
          :providers="state.providers"
          :runtime-config-schemas="state.runtimeConfigSchemas"
          :runtime-models="state.runtimeModels"
          :runtime-profiles="state.runtimeProfiles"
          @clear-runtime="clearRuntime"
          @codex-official-login="startCodexOfficialLogin"
          @codex-auth-json-import="importCodexAuthJson"
          @codex-account-enable="enableCodexAccount"
          @codex-account-clear="clearCodexAccount"
          @codex-account-refresh="refreshCodexAccount"
          @codex-account-proxy-save="updateCodexAccountProxy"
          @cancel-codex-official-login="cancelCodexOfficialLogin"
          @delete-provider="deleteProvider"
          @save-model="saveRuntimeModel"
          @save-provider="saveProvider"
          @switch-runtime="switchRuntime"
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

        <SettingsView
          v-else-if="activeView === 'settings'"
          :app-settings="state.appSettings"
          :cli-targets="state.cliTargets"
          :pending="pending"
          @open-path="openPath"
          @save="saveSettings"
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

    <ImportSkillsModal
      v-if="showImportSkills"
      :candidates="importCandidates"
      @close="showImportSkills = false"
      @submit="confirmImportSkills"
    />

    <AddRepoModal
      v-if="showAddRepo"
      @close="showAddRepo = false"
      @submit="addRepo"
    />

    <SelectionTranslator />
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue"
import {
  Box,
  Compass,
  Gauge,
  Network,
  Settings,
  ShieldCheck
} from "lucide-vue-next"
import AppSidebar from "@/components/AppSidebar.vue"
import SkillsView from "@/features/skills/index.vue"
import SessionsView from "@/features/sessions/index.vue"
import ProvidersView from "@/features/providers/index.vue"
import ReposView from "@/features/repos/index.vue"
import SettingsView from "@/features/settings/index.vue"
import SkillDrawer from "@/features/skills/components/SkillDrawer.vue"
import CreateSkillModal from "@/features/skills/components/CreateSkillModal.vue"
import ImportSkillsModal from "@/features/skills/components/ImportSkillsModal.vue"
import AddRepoModal from "@/features/repos/components/AddRepoModal.vue"
import SelectionTranslator from "@/components/SelectionTranslator.vue"
import { createMessage } from "@/utils/message"

const navItems = [
  { id: "providers", label: "Providers", icon: Network },
  { id: "skills", label: "Skills", icon: ShieldCheck },
  { id: "sessions", label: "Sessions", icon: Gauge },
  { id: "rules", label: "Rules", icon: Compass },
  { id: "workspace", label: "Workspace", icon: Box },
  { id: "settings", label: "Settings", icon: Settings }
]

const placeholderMap = {
  sessions: {
    title: "Session System",
    description: "当前视图已经接入 Session 聚合，请从侧边栏重新进入。",
    backTo: "providers"
  },
  rules: {
    title: "Rules 视图待扩展",
    description:
      "规则系统后续可以直接消费 Registry 与 Skill 元数据，这一版只保留导航骨架。",
    backTo: "skills"
  },
  workspace: {
    title: "Workspace 视图待扩展",
    description: "当前工作区路径已经由主进程管理，可在设置页中配置相关目录。",
    backTo: "providers"
  },
  settings: {
    title: "Settings",
    description: "设置页已接入。",
    backTo: "providers"
  }
}

const state = reactive({
  cliTargets: [],
  skills: [],
  repos: [],
  sessions: [],
  codexAccounts: [],
  codexLoginState: null,
  providers: [],
  runtimeConfigSchemas: {},
  runtimeModels: [],
  runtimeProfiles: [],
  diagnostics: [],
  paths: {
    workspaceRoot: "",
    skillsDir: "",
    reposDir: "",
    sessionRecycleDir: "",
    storageDir: ""
  },
  appSettings: {
    dataPath: "",
    defaultDataPath: "",
    settingsFilePath: "",
    restartRequired: false,
    cliConfigPaths: {
      claude: "",
      codex: "",
      gemini: ""
    },
    defaultCliConfigPaths: {
      claude: "",
      codex: "",
      gemini: ""
    }
  },
  refreshedAt: 0
})

const activeView = ref("providers")
const pending = ref(false)
const sidebarCollapsed = ref(false)
const selectedSkillName = ref("")
const showCreateSkill = ref(false)
const showImportSkills = ref(false)
const showAddRepo = ref(false)
const importCandidates = ref([])

let unsubscribe = null

const selectedSkill = computed(() => {
  return (
    state.skills.find((item) => item.name === selectedSkillName.value) || null
  )
})

const currentPlaceholder = computed(() => {
  return placeholderMap[activeView.value] || placeholderMap.sessions
})

async function bootstrap() {
  pending.value = true

  try {
    updateState(await window.aiManager.bootstrap())
    unsubscribe = window.aiManager.onStateChanged((nextState) => {
      updateState(nextState)
    })
  } catch (error) {
    showErrorMessage(error)
  } finally {
    pending.value = false
  }
}

function updateState(nextState) {
  state.cliTargets = nextState.cliTargets || []
  state.skills = nextState.skills || []
  state.repos = nextState.repos || []
  state.sessions = nextState.sessions || []
  state.codexAccounts = nextState.codexAccounts || []
  state.codexLoginState = nextState.codexLoginState || null
  state.providers = nextState.providers || []
  state.runtimeConfigSchemas = nextState.runtimeConfigSchemas || {}
  state.runtimeModels = nextState.runtimeModels || []
  state.runtimeProfiles = nextState.runtimeProfiles || []
  state.diagnostics = nextState.diagnostics || []
  state.paths = nextState.paths || state.paths
  state.appSettings = nextState.appSettings || state.appSettings
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

  try {
    const nextState = await action()
    if (nextState && typeof nextState === "object" && "skills" in nextState) {
      updateState(nextState)
    }
    return true
  } catch (error) {
    showErrorMessage(error)
    return false
  } finally {
    pending.value = false
  }
}

function selectSkill(skill) {
  selectedSkillName.value = skill.name
}

function showErrorMessage(error) {
  createMessage.error(error.message || String(error))
}

function showSuccessMessage(message) {
  createMessage.success(message)
}

function showWarningMessage(message) {
  createMessage.warning(message)
}

async function refreshState() {
  await runAction(() => window.aiManager.refresh())
}

async function saveSettings(payload) {
  const success = await runAction(() => window.aiManager.saveSettings(payload))

  if (success) {
    showSuccessMessage(
      state.appSettings.restartRequired
        ? "设置已保存，Data 目录将在重启后生效。"
        : "设置已保存并重新刷新。"
    )
  }
}

async function createSkill(payload) {
  const success = await runAction(() => window.aiManager.createSkill(payload))

  if (success) {
    showCreateSkill.value = false
    activeView.value = "skills"
  }
}

async function importSkillsFromCli() {
  pending.value = true

  try {
    const preview = await window.aiManager.previewSkillsFromCli()
    const candidates = Array.isArray(preview) ? preview : preview.candidates
    const conflicts = Array.isArray(preview) ? [] : preview.conflicts

    importCandidates.value = {
      candidates,
      conflicts
    }

    if (!candidates.length && !conflicts.length) {
      showSuccessMessage("所有 Skill 已经在 AI Manager 集中管理中。")
      return
    }

    showImportSkills.value = true
  } catch (error) {
    showErrorMessage(error)
  } finally {
    pending.value = false
  }
}

async function importSkillFromZip() {
  try {
    const zipPath = await window.aiManager.selectFile({
      title: "选择 Skill zip 压缩包",
      filters: [{ name: "Zip 压缩包", extensions: ["zip"] }]
    })

    if (!zipPath) {
      return
    }

    const success = await runAction(() =>
      window.aiManager.importSkillFromZip({ zipPath })
    )

    if (success) {
      activeView.value = "skills"
      showSuccessMessage("Skill zip 已导入。")
    }
  } catch (error) {
    showErrorMessage(error)
  }
}

async function confirmImportSkills(payload) {
  const success = await runAction(() =>
    window.aiManager.importSkillsFromCli(payload)
  )

  if (success) {
    showImportSkills.value = false
    importCandidates.value = []
    activeView.value = "skills"
    showSuccessMessage("选中的 Skill 已导入并挂载到对应 CLI。")
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

async function deleteSession(sessionId) {
  await runAction(() => window.aiManager.deleteSession({ sessionId }))
}

async function saveProvider(payload) {
  const success = await runAction(() => window.aiManager.saveProvider(payload))

  if (success) {
    showSuccessMessage("Provider 已保存。")
  }
}

async function deleteProvider(providerId) {
  const success = await runAction(() =>
    window.aiManager.deleteProvider({ providerId })
  )

  if (success) {
    showSuccessMessage("Provider 已删除。")
  }
}

async function startCodexOfficialLogin(payload) {
  const success = await runAction(() =>
    window.aiManager.startCodexOfficialLogin(payload)
  )

  if (success) {
    showWarningMessage("已打开浏览器，请完成 Codex 官方登录。")
  }
}

async function cancelCodexOfficialLogin() {
  await runAction(() => window.aiManager.cancelCodexOfficialLogin())
}

async function importCodexAuthJson(payload) {
  const success = await runAction(() =>
    window.aiManager.importCodexAuthJson(payload)
  )

  if (success) {
    showSuccessMessage("Codex 登录 JSON 数据已验证并导入。")
  }
}

async function enableCodexAccount(payload) {
  const success = await runAction(() =>
    window.aiManager.enableCodexAccount(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号已启用。")
  }
}

async function clearCodexAccount() {
  const success = await runAction(() => window.aiManager.clearCodexAccount())

  if (success) {
    showSuccessMessage("Codex 官方账号已取消启用。")
  }
}

async function refreshCodexAccount(payload) {
  const success = await runAction(() =>
    window.aiManager.refreshCodexAccount(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号额度已刷新。")
  }
}

async function updateCodexAccountProxy(payload) {
  const success = await runAction(() =>
    window.aiManager.updateCodexAccountProxy(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号代理已保存。")
  }
}

async function saveRuntimeModel(payload) {
  const success = await runAction(() =>
    window.aiManager.saveRuntimeModel(payload)
  )

  if (success) {
    showSuccessMessage("模型已保存。")
  }
}

async function switchRuntime(payload) {
  const success = await runAction(() => window.aiManager.switchRuntime(payload))

  if (success) {
    showSuccessMessage("Runtime Profile 已切换。")
  }
}

async function clearRuntime(payload) {
  const success = await runAction(() => window.aiManager.clearRuntime(payload))

  if (success) {
    showSuccessMessage("Runtime Profile 已取消使用。")
  }
}

async function openPath(targetPath) {
  if (!targetPath) {
    return
  }

  pending.value = true

  try {
    await window.aiManager.openPath({ targetPath })
  } catch (error) {
    showErrorMessage(error)
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

  &__main {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    padding: 18px;
    gap: 14px;
    background: var(--color-page);
  }

  &__content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-right: 6px;
  }

  &__placeholder {
    display: grid;
    min-height: 520px;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    padding: 32px;
    text-align: center;
  }

  &__placeholder h1 {
    margin: 0 0 12px;
    font-size: 2rem;
  }

  &__placeholder p {
    max-width: 680px;
    margin: 0 0 18px;
    color: var(--color-text-muted);
    line-height: 1.7;
  }
}

.status-button {
  height: 40px;
  padding: 0 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #fbfcfd;
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;

  &:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }
}
</style>

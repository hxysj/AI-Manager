<template>
  <QuickSwitchPanel
    v-if="isQuickSwitchPanel"
    :state="state"
    @state-updated="updateState"
  />

  <div v-else class="app-shell">
    <AppSidebar
      :active-view="activeView"
      :cli-targets="state.cliTargets"
      :collapsed="sidebarCollapsed"
      :nav-items="navItems"
      @toggle="sidebarCollapsed = !sidebarCollapsed"
      @select-view="activeView = $event"
      @title-click="handleSidebarTitleClick"
    />

    <main class="app-shell__main">
      <section
        :class="[
          'app-shell__content',
          { 'app-shell__content--locked': activeView === 'settings' }
        ]"
      >
        <SkillsView
          v-if="activeView === 'skills'"
          :cli-targets="state.cliTargets"
          :paths="state.paths"
          :skill-groups="state.skillGroups"
          :skill-repositories="state.skillRepositories"
          :skill-trash-items="skillTrashItems"
          :skills="state.skills"
          @add-skill-repository="addSkillRepository"
          @batch-skill-action="batchSkillAction"
          @create-skill="showCreateSkill = true"
          @delete-skills="deleteSkills"
          @import-skills="importSkillsFromCli"
          @import-zip-skill="importSkillFromZip"
          @install-repository-skill="installSkillFromRepository"
          @install-skill="installSkill"
          @open-path="openPath"
          @open-usage="activeView = 'skill-usage'"
          @refresh="refreshState"
          @refresh-skill-repository="refreshSkillRepository"
          @remove-skill-repository="removeSkillRepository"
          @restore-skill-trash="restoreSkillTrash"
          @remove-skill-group-items="removeSkillGroupItems"
          @remove-skill-group="removeSkillGroup"
          @save-skill-group="saveSkillGroup"
          @select-skill="selectSkill"
          @set-skill-enabled="setSkillEnabled"
          @show-skill-trash="loadSkillTrash"
          @purge-skill-trash="purgeSkillTrash"
          @uninstall-skill="uninstallSkill"
        />

        <SkillUsageView
          v-else-if="activeView === 'skill-usage'"
          @back="activeView = 'skills'"
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
          :claude-proxy-state="state.claudeProxyState"
          :codex-proxy-state="state.codexProxyState"
          :providers="state.providers"
          :usage="state.usage"
          :runtime-config-schemas="state.runtimeConfigSchemas"
          :runtime-models="state.runtimeModels"
          :runtime-provider-state="state.runtimeProviderState"
          :runtime-profiles="state.runtimeProfiles"
          @clear-runtime="clearRuntime"
          @claude-proxy-enable="enableClaudeProxy"
          @claude-proxy-disable="disableClaudeProxy"
          @claude-proxy-provider-add="addClaudeProxyProvider"
          @claude-proxy-provider-remove="removeClaudeProxyProvider"
          @claude-proxy-provider-activate="activateClaudeProxyProvider"
          @claude-provider-instance-launch="launchClaudeProviderInstance"
          @codex-official-login="startCodexOfficialLogin"
          @codex-auth-json-import="importCodexAuthJson"
          @codex-account-enable="enableCodexAccount"
          @codex-account-clear="clearCodexAccount"
          @codex-account-delete="deleteCodexAccount"
          @codex-account-refresh="refreshCodexAccount"
          @codex-account-disable="disableCodexAccount"
          @codex-account-restore="restoreCodexAccount"
          @codex-accounts-refresh="refreshCodexAccounts"
          @codex-account-proxy-save="updateCodexAccountProxy"
          @codex-proxy-enable="enableCodexProxy"
          @codex-proxy-disable="disableCodexProxy"
          @codex-proxy-provider-add="addCodexProxyProvider"
          @codex-proxy-provider-remove="removeCodexProxyProvider"
          @codex-proxy-provider-activate="activateCodexProxyProvider"
          @codex-proxy-account-model-save="saveCodexProxyAccountModel"
          @codex-provider-instance-launch="launchCodexProviderInstance"
          @cancel-codex-official-login="cancelCodexOfficialLogin"
          @delete-provider="deleteProvider"
          @save-model="saveRuntimeModel"
          @save-provider="saveProvider"
          @resolve-runtime-drift="resolveRuntimeDrift"
          @switch-runtime="switchRuntime"
        />

        <UsageView v-else-if="activeView === 'usage'" :usage="state.usage" />

        <RulesView
          v-else-if="activeView === 'rules'"
          :cli-targets="state.cliTargets"
          :pending="pending"
          :rules="state.rules"
          @delete-rule="deleteRule"
          @enable-rule="enableRule"
          @import-rule="importRule"
          @open-path="openPath"
          @resolve-import-conflict="resolveRuleImportConflict"
          @resolve-drift="resolveRuleDrift"
          @save-rule="saveRule"
          @toggle-rule="toggleRule"
        />

        <ToolsView
          v-else-if="activeView === 'tools'"
          :repos="state.repos"
          @add-repo="showAddRepo = true"
        />

        <SettingsView
          v-else-if="activeView === 'settings'"
          :app-settings="state.appSettings"
          :cli-targets="state.cliTargets"
          :local-backup-directory="localBackupDirectory"
          :local-backups="localBackups"
          :pending="pending"
          @export-data="exportDataBackup"
          @local-backup-now="createLocalBackup"
          @local-backup-restore="previewLocalBackupRestore"
          @local-backups-refresh="refreshLocalBackups"
          @inspect-cloud-data="inspectCloudBackup"
          @pull-cloud-data="pullCloudBackup"
          @push-cloud-data="pushCloudBackup"
          @check-update="checkForAppUpdates"
          @open-path="openPath"
          @quit-app="quitApp"
          @restore-data="restoreDataBackup"
          @save="saveSettings"
          @uninstall-without-trace="uninstallWithoutTrace"
        />

        <LogsView
          v-else-if="activeView === 'logs'"
          :file-path="appLogPath"
          :logs="appLogs"
          @clear="clearAppLogs"
          @refresh="loadAppLogs"
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
      @set-enabled="setSkillEnabled"
    />

    <CreateSkillModal
      v-if="showCreateSkill"
      @close="showCreateSkill = false"
      @submit="createSkill"
    />

    <ImportSkillsModal
      v-if="showImportSkills"
      :candidates="importCandidates"
      :loading="pending"
      @close="showImportSkills = false"
      @submit="confirmImportSkills"
    />

    <AddRepoModal
      v-if="showAddRepo"
      @close="showAddRepo = false"
      @submit="addRepo"
    />

    <DataRestorePreviewModal
      v-if="restorePreview"
      :preview="restorePreview"
      :description="restorePreviewDescription"
      :loading="pending"
      @close="closeRestorePreview"
      @submit="confirmRestore"
    />

    <BaseModal
      v-if="cloudBackupView"
      title="云端备份内容"
      :description="cloudBackupDescription"
      @close="closeCloudBackupView"
    >
      <div class="cloud-backup-modal">
        <div class="cloud-backup-modal__summary">
          <span>文件 {{ cloudBackupView.backup.fileCount }} 个</span>
          <span>目录 {{ cloudBackupView.backup.directoryCount }} 个</span>
          <span>条目 {{ cloudBackupView.backup.entryCount }} 个</span>
        </div>

        <div class="cloud-backup-modal__body">
          <aside class="cloud-backup-modal__list">
            <button
              v-for="entry in cloudBackupView.backup.entries"
              :key="entry.path"
              :class="[
                'cloud-backup-modal__entry',
                {
                  'cloud-backup-modal__entry--active':
                    entry.path === selectedCloudBackupPath
                }
              ]"
              type="button"
              @click="selectedCloudBackupPath = entry.path"
            >
              <strong>{{ entry.typeName }}</strong>
              <span>{{ entry.path }}</span>
            </button>
          </aside>

          <section class="cloud-backup-modal__content">
            <div
              v-if="selectedCloudBackupEntry"
              class="cloud-backup-modal__head"
            >
              <div>
                <strong>{{ selectedCloudBackupEntry.typeName }}</strong>
                <span>{{ selectedCloudBackupEntry.path }}</span>
              </div>
              <small>{{
                formatBackupEntrySize(selectedCloudBackupEntry.size)
              }}</small>
            </div>
            <pre v-if="selectedCloudBackupEntry">{{
              selectedCloudBackupEntry.content || "空内容"
            }}</pre>
          </section>
        </div>
      </div>
    </BaseModal>

    <AppUpdateModal
      :dialog="updateDialog"
      @close="closeUpdateDialog"
      @download="downloadAppUpdate"
      @install="installAppUpdate"
    />

    <div v-if="showCloseConfirm" class="close-confirm">
      <div class="close-confirm__overlay"></div>
      <section class="close-confirm__panel" role="dialog" aria-modal="true">
        <header class="close-confirm__header">
          <div>
            <span>窗口操作</span>
            <h2>关闭应用</h2>
          </div>
          <button
            class="close-confirm__icon-button"
            type="button"
            aria-label="取消关闭"
            @click="submitCloseAction('cancel')"
          >
            <X :size="17" />
          </button>
        </header>

        <div class="close-confirm__body">
          <div class="close-confirm__mark">
            <Info :size="22" />
          </div>
          <div class="close-confirm__copy">
            <strong>关闭按钮要执行什么操作？</strong>
            <span>可以最小化到托盘继续运行，也可以直接关闭软件。</span>
          </div>
        </div>

        <footer class="close-confirm__footer">
          <label class="close-confirm__remember">
            <input v-model="closeRemember" type="checkbox" />
            <span>记住我的选择</span>
          </label>
          <div class="close-confirm__actions">
            <button
              class="close-confirm__button close-confirm__button--primary"
              type="button"
              @click="submitCloseAction('minimize')"
            >
              <Minus :size="15" />
              最小化到托盘
            </button>
            <button
              class="close-confirm__button"
              type="button"
              @click="submitCloseAction('quit')"
            >
              <Power :size="15" />
              直接关闭
            </button>
            <button
              class="close-confirm__button"
              type="button"
              @click="submitCloseAction('cancel')"
            >
              取消
            </button>
          </div>
        </footer>
      </section>
    </div>

    <SelectionTranslator :active-view="activeView" />
    <GlobalLoading />
  </div>
</template>

<script setup>
import {
  computed,
  defineAsyncComponent,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch
} from "vue"
import {
  BarChart3,
  Compass,
  Gauge,
  Info,
  Minus,
  Network,
  Power,
  Settings,
  ShieldCheck,
  Wrench,
  X
} from "lucide-vue-next"
import AppSidebar from "@/components/AppSidebar.vue"
import AppUpdateModal from "@/components/AppUpdateModal.vue"
import BaseModal from "@/components/BaseModal.vue"
import DataRestorePreviewModal from "@/components/DataRestorePreviewModal.vue"
import GlobalLoading from "@/components/GlobalLoading.vue"
import QuickSwitchPanel from "@/components/QuickSwitchPanel.vue"
import SelectionTranslator from "@/components/SelectionTranslator.vue"
import {
  accountApi,
  appApi,
  dataApi,
  providerApi,
  proxyApi,
  repoApi,
  ruleApi,
  runtimeApi,
  sessionApi,
  settingsApi,
  skillApi,
  systemApi
} from "@/api"
import { useGlobalLoading } from "@/utils/global-loading"
import { createMessage } from "@/utils/message"

const ProvidersView = defineAsyncComponent(
  () => import("@/features/providers/index.vue")
)
const ToolsView = defineAsyncComponent(
  () => import("@/features/tools/index.vue")
)
const AddRepoModal = defineAsyncComponent(
  () => import("@/features/repos/components/AddRepoModal.vue")
)
const RulesView = defineAsyncComponent(
  () => import("@/features/rules/index.vue")
)
const SessionsView = defineAsyncComponent(
  () => import("@/features/sessions/index.vue")
)
const SettingsView = defineAsyncComponent(
  () => import("@/features/settings/index.vue")
)
const SkillsView = defineAsyncComponent(
  () => import("@/features/skills/index.vue")
)
const SkillUsageView = defineAsyncComponent(
  () => import("@/features/skills/usage.vue")
)
const UsageView = defineAsyncComponent(
  () => import("@/features/usage/index.vue")
)
const LogsView = defineAsyncComponent(() => import("@/features/logs/index.vue"))
const CreateSkillModal = defineAsyncComponent(
  () => import("@/features/skills/components/CreateSkillModal.vue")
)
const ImportSkillsModal = defineAsyncComponent(
  () => import("@/features/skills/components/ImportSkillsModal.vue")
)
const SkillDrawer = defineAsyncComponent(
  () => import("@/features/skills/components/SkillDrawer.vue")
)

const baseNavItems = [
  { id: "providers", label: "Providers", icon: Network },
  { id: "usage", label: "Usage", icon: BarChart3 },
  { id: "skills", label: "Skills", icon: ShieldCheck },
  { id: "sessions", label: "Sessions", icon: Gauge },
  { id: "rules", label: "Rules", icon: Compass },
  { id: "tools", label: "Tools", icon: Wrench },
  { id: "settings", label: "Settings", icon: Settings }
]

const queryParams = new URLSearchParams(window.location.search)
const queryView = queryParams.get("view")
const isQuickSwitchPanel = queryParams.get("panel") === "quick-switch"

const placeholderMap = {
  sessions: {
    title: "Session System",
    description: "当前视图已经接入 Session 聚合，请从侧边栏重新进入。",
    backTo: "providers"
  },
  workspace: {
    title: "Workspace 视图待扩展",
    description: "当前工作区路径已经由主进程管理，可在设置页配置相关目录。",
    backTo: "providers"
  },
  settings: {
    title: "Settings",
    description: "设置页已经接入。",
    backTo: "providers"
  }
}

const state = reactive({
  cliTargets: [],
  skills: [],
  skillGroups: [],
  skillRepositories: [],
  repos: [],
  sessions: [],
  usage: {},
  codexAccounts: [],
  codexLoginState: null,
  claudeProxyState: {
    enabled: false,
    localBaseUrl: "",
    activeProviderId: "",
    failoverProviderIds: []
  },
  codexProxyState: {
    enabled: false,
    localBaseUrl: "",
    activeProviderId: "",
    failoverProviderIds: [],
    accountModel: ""
  },
  providers: [],
  rules: {
    supportedClis: [],
    prompts: [],
    profiles: {},
    runtimeState: {}
  },
  runtimeConfigSchemas: {},
  runtimeModels: [],
  runtimeProviderState: {},
  runtimeProfiles: [],
  diagnostics: [],
  paths: {
    workspaceRoot: "",
    skillsDir: "",
    promptsDir: "",
    promptProfilesDir: "",
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
      codex: ""
      // 当前版本暂不启用 Gemini。
      // gemini: ""
    },
    defaultCliConfigPaths: {
      claude: "",
      codex: ""
      // 当前版本暂不启用 Gemini。
      // gemini: ""
    },
    cloudSync: {
      provider: "jianguoyun",
      webdavUrl: "",
      username: "",
      password: "",
      fileName: ""
    },
    localBackup: {
      enabled: true,
      intervalMinutes: 60,
      maxCount: 20,
      lastBackupAt: 0
    },
    system: {
      closeAction: "ask",
      quickSwitchVisible: true,
      autoLaunchEnabled: false
    }
  },
  refreshedAt: 0
})

const activeView = ref(
  baseNavItems.some((item) => item.id === queryView) ? queryView : "providers"
)
const showLogsTab = ref(false)
const sidebarTitleClickCount = ref(0)
const appLogs = ref([])
const appLogPath = ref("")
const sidebarCollapsed = ref(false)
const selectedSkillName = ref("")
const skillTrashItems = ref([])
const showCreateSkill = ref(false)
const showImportSkills = ref(false)
const showAddRepo = ref(false)
const showCloseConfirm = ref(false)
const closeRemember = ref(false)
const updateDialog = reactive({
  open: false,
  phase: "idle",
  message: "",
  version: "",
  releaseNotes: "",
  percent: 0,
  transferred: 0,
  total: 0,
  bytesPerSecond: 0,
  installDirectory: "",
  manual: false
})
const importCandidates = ref([])
const localBackups = ref([])
const localBackupDirectory = ref("")
const restorePreview = ref(null)
const restoreSource = ref(null)
const cloudBackupView = ref(null)
const selectedCloudBackupPath = ref("")
const { loading: pending, withGlobalLoading } = useGlobalLoading()

let unsubscribe = null
let unsubscribeClose = null
let unsubscribeUpdate = null

const navItems = computed(() =>
  showLogsTab.value
    ? [...baseNavItems, { id: "logs", label: "日志", icon: Info }]
    : baseNavItems
)

const selectedSkill = computed(() => {
  return (
    state.skills.find((item) => item.name === selectedSkillName.value) || null
  )
})

const currentPlaceholder = computed(() => {
  return placeholderMap[activeView.value] || placeholderMap.sessions
})

const restorePreviewDescription = computed(() => {
  const sourceName =
    restoreSource.value?.type === "cloud"
      ? restoreSource.value.fileName
      : restoreSource.value?.type === "local"
        ? restoreSource.value.fileName || "本地自动备份"
        : restoreSource.value?.filePath || "本地备份"

  return `从 ${sourceName} 兼容合并配置数据。`
})

const cloudBackupDescription = computed(() => {
  if (!cloudBackupView.value) {
    return ""
  }

  return `${cloudBackupView.value.fileName} · 创建于 ${formatCloudBackupTime(
    cloudBackupView.value.backup.createdAt
  )}`
})

const selectedCloudBackupEntry = computed(() => {
  return (
    cloudBackupView.value?.backup.entries.find(
      (entry) => entry.path === selectedCloudBackupPath.value
    ) || null
  )
})

async function bootstrap() {
  await withGlobalLoading(async () => {
    try {
      updateState(await appApi.bootstrap())
      await refreshLocalBackups(false)
      unsubscribe = appApi.onStateChanged((nextState) => {
        const previousLocalBackupAt =
          state.appSettings.localBackup?.lastBackupAt || 0
        updateState(nextState)
        const nextLocalBackupAt =
          state.appSettings.localBackup?.lastBackupAt || 0

        if (nextLocalBackupAt !== previousLocalBackupAt) {
          refreshLocalBackups(false)
        }
      })
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

function updateState(nextState) {
  if ("cliTargets" in nextState) {
    state.cliTargets = nextState.cliTargets || []
  }
  if ("skills" in nextState) {
    state.skills = nextState.skills || []
  }
  if ("skillGroups" in nextState) {
    state.skillGroups = nextState.skillGroups || []
  }
  if ("skillRepositories" in nextState) {
    state.skillRepositories = nextState.skillRepositories || []
  }
  if ("repos" in nextState) {
    state.repos = nextState.repos || []
  }
  if ("sessions" in nextState) {
    state.sessions = nextState.sessions || []
  }
  if ("usage" in nextState) {
    state.usage = nextState.usage || {}
  }
  if ("codexAccounts" in nextState) {
    state.codexAccounts = nextState.codexAccounts || []
  }
  if ("codexLoginState" in nextState) {
    state.codexLoginState = nextState.codexLoginState || null
  }
  if ("claudeProxyState" in nextState) {
    state.claudeProxyState = nextState.claudeProxyState || {
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: []
    }
  }
  if ("codexProxyState" in nextState) {
    state.codexProxyState = nextState.codexProxyState || {
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: [],
      accountModel: ""
    }
  }
  if ("providers" in nextState) {
    state.providers = nextState.providers || []
  }
  if ("rules" in nextState) {
    state.rules = nextState.rules || state.rules
  }
  if ("runtimeConfigSchemas" in nextState) {
    state.runtimeConfigSchemas = nextState.runtimeConfigSchemas || {}
  }
  if ("runtimeModels" in nextState) {
    state.runtimeModels = nextState.runtimeModels || []
  }
  if ("runtimeProviderState" in nextState) {
    state.runtimeProviderState = nextState.runtimeProviderState || {}
  }
  if ("runtimeProfiles" in nextState) {
    state.runtimeProfiles = nextState.runtimeProfiles || []
  }
  if ("diagnostics" in nextState) {
    state.diagnostics = nextState.diagnostics || []
  }
  if ("paths" in nextState) {
    state.paths = nextState.paths || state.paths
  }
  if ("appSettings" in nextState) {
    state.appSettings = nextState.appSettings || state.appSettings
  }
  if ("refreshedAt" in nextState) {
    state.refreshedAt = nextState.refreshedAt || 0
  }

  if (!("claudeProxyState" in nextState) && !state.claudeProxyState) {
    state.claudeProxyState = {
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: []
    }
  }
  if (!("codexProxyState" in nextState) && !state.codexProxyState) {
    state.codexProxyState = {
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: [],
      accountModel: ""
    }
  }
  if (
    selectedSkillName.value &&
    !state.skills.find((item) => item.name === selectedSkillName.value)
  ) {
    selectedSkillName.value = ""
  }
}

async function handleSidebarTitleClick() {
  sidebarTitleClickCount.value += 1

  if (sidebarTitleClickCount.value < 10) {
    return
  }

  showLogsTab.value = true
  activeView.value = "logs"
  sidebarTitleClickCount.value = 0
  await loadAppLogs()
}

async function loadAppLogs() {
  const result = await appApi.getAppLogs()

  appLogs.value = result.logs || []
  appLogPath.value = result.filePath || ""
}

async function clearAppLogs() {
  const result = await appApi.clearAppLogs()

  appLogs.value = result.logs || []
  appLogPath.value = result.filePath || ""
}

function formatCloudBackupTime(value) {
  const timestamp = Number(value || 0)

  if (!timestamp) {
    return "未知时间"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(timestamp))
}

function formatBackupEntrySize(value) {
  const size = Number(value || 0)

  if (size < 1024) {
    return `${size} B`
  }

  if (size < 1024 * 1024) {
    return `${(size / 1024).toFixed(1)} KB`
  }

  return `${(size / 1024 / 1024).toFixed(2)} MB`
}

async function runAction(action) {
  return withGlobalLoading(async () => {
    try {
      const nextState = await action()
      if (nextState && typeof nextState === "object") {
        updateState(nextState)
      }
      return true
    } catch (error) {
      showErrorMessage(error)
      return false
    }
  })
}

function selectSkill(skill) {
  selectedSkillName.value = skill.name
}

function showErrorMessage(error) {
  createMessage.error(error.message || String(error))
}

function isCodexAccountRefreshError(error) {
  return Boolean(error)
}

function getProxyState(cli) {
  if (cli === "claude") {
    return state.claudeProxyState
  }

  if (cli === "codex") {
    return state.codexProxyState
  }

  return null
}

function getProxyApi(cli) {
  if (cli === "claude") {
    return {
      enable: proxyApi.enableClaudeProxy,
      disable: proxyApi.disableClaudeProxy,
      addProvider: proxyApi.addClaudeProxyProvider,
      removeProvider: proxyApi.removeClaudeProxyProvider,
      activateProvider: proxyApi.activateClaudeProxyProvider
    }
  }

  if (cli === "codex") {
    return {
      enable: proxyApi.enableCodexProxy,
      disable: proxyApi.disableCodexProxy,
      addProvider: proxyApi.addCodexProxyProvider,
      removeProvider: proxyApi.removeCodexProxyProvider,
      activateProvider: proxyApi.activateCodexProxyProvider
    }
  }

  return null
}

function showSuccessMessage(message) {
  createMessage.success(message)
}

function showWarningMessage(message) {
  createMessage.warning(message)
}

function applyUpdateStatus(status = {}) {
  updateDialog.phase = status.phase || "idle"
  updateDialog.message = status.message || ""
  updateDialog.version = status.version || ""
  updateDialog.releaseNotes = status.releaseNotes || ""
  updateDialog.percent = Number(status.percent || 0)
  updateDialog.transferred = Number(status.transferred || 0)
  updateDialog.total = Number(status.total || 0)
  updateDialog.bytesPerSecond = Number(status.bytesPerSecond || 0)
  updateDialog.installDirectory =
    status.installDirectory || updateDialog.installDirectory || ""
  updateDialog.manual = Boolean(status.manual)

  if (updateDialog.phase === "error" && updateDialog.message) {
    console.error("[update]", updateDialog.message)
    createMessage.error(updateDialog.message)
  }

  if (updateDialog.phase === "idle" || isQuickSwitchPanel) {
    updateDialog.open = false
    return
  }

  updateDialog.open =
    updateDialog.manual ||
    ["available", "downloading", "error"].includes(updateDialog.phase)
}

async function refreshState() {
  await runAction(() => appApi.refresh())
}

async function saveSettings(payload) {
  const success = await runAction(() => settingsApi.saveSettings(payload))

  if (success) {
    await refreshLocalBackups(false)
    showSuccessMessage(
      state.appSettings.restartRequired
        ? "设置已保存，数据目录将在重启后生效。"
        : "设置已保存并重新刷新。"
    )
  }
}

async function checkForAppUpdates() {
  applyUpdateStatus({
    phase: "checking",
    message: "正在检查更新...",
    manual: true
  })

  try {
    applyUpdateStatus(await appApi.checkForUpdates())
  } catch (error) {
    applyUpdateStatus({
      phase: "error",
      message: error.message || String(error),
      manual: true
    })
  }
}

async function downloadAppUpdate() {
  applyUpdateStatus({
    ...updateDialog,
    phase: "downloading",
    message: `正在下载新版本 ${updateDialog.version || ""}`.trim(),
    manual: true,
    percent: 0,
    transferred: 0,
    total: 0,
    bytesPerSecond: 0
  })

  try {
    applyUpdateStatus(await appApi.downloadUpdate())
  } catch (error) {
    console.error("[update:download]", error)
    applyUpdateStatus({
      phase: "error",
      message: error.message || String(error),
      manual: true
    })
  }
}

async function installAppUpdate() {
  try {
    await appApi.installUpdate({
      installDirectory: updateDialog.installDirectory
    })
  } catch (error) {
    applyUpdateStatus({
      phase: "error",
      message: error.message || String(error),
      manual: true
    })
  }
}

async function closeUpdateDialog() {
  if (["checking", "downloading", "installing"].includes(updateDialog.phase)) {
    return
  }

  updateDialog.open = false

  try {
    await appApi.dismissUpdate()
  } catch (error) {
    showErrorMessage(error)
  }
}

async function exportDataBackup() {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.exportDataBackup()

      if (result?.canceled) {
        return
      }

      showSuccessMessage("配置数据已加密导出。")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function restoreDataBackup() {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.previewDataBackupRestore()

      if (result?.canceled) {
        return
      }

      openRestorePreview(result, "file")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function refreshLocalBackups(showMessage = true) {
  try {
    const result = await dataApi.listLocalBackups()
    localBackups.value = result.backups || []
    localBackupDirectory.value = result.directory || ""

    if (showMessage) {
      showSuccessMessage("本地备份列表已刷新。")
    }
  } catch (error) {
    showErrorMessage(error)
  }
}

async function createLocalBackup() {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.createLocalBackup()
      localBackups.value = result.backups || []
      localBackupDirectory.value =
        result.directory || localBackupDirectory.value

      if (result.state) {
        updateState(result.state)
      }

      showSuccessMessage("本地备份已创建。")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function previewLocalBackupRestore(backup) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.previewLocalBackupRestore({
        backupId: backup.id
      })
      openRestorePreview(result, "local")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function pushCloudBackup(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.pushCloudBackup(payload)

      if (result?.state) {
        updateState(result.state)
      }

      showSuccessMessage("配置数据已推送到坚果云。")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function inspectCloudBackup(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.inspectCloudBackup(payload)

      cloudBackupView.value = result
      selectedCloudBackupPath.value =
        result.backup?.entries.find(
          (entry) => entry.path === "storage/usage-pricing.json"
        )?.path ||
        result.backup?.entries.find((entry) => entry.type === "file")?.path ||
        ""
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function pullCloudBackup(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.previewCloudBackupRestore(payload)
      openRestorePreview(
        {
          ...result,
          cloudSync: payload
        },
        "cloud"
      )
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

function closeCloudBackupView() {
  cloudBackupView.value = null
  selectedCloudBackupPath.value = ""
}

function openRestorePreview(result, type) {
  restorePreview.value = result.preview
  restoreSource.value = {
    type,
    restoreId: result.restoreId,
    filePath: result.filePath || "",
    fileName: result.fileName || "",
    backupId: result.backupId || "",
    cloudSync: result.cloudSync || null
  }
}

function closeRestorePreview(force = false) {
  if (pending.value && !force) {
    return
  }

  restorePreview.value = null
  restoreSource.value = null
}

async function confirmRestore(payload) {
  const source = restoreSource.value

  if (!source) {
    return
  }

  await withGlobalLoading(async () => {
    try {
      const choices = payload?.choices || {}
      const restorePayload = {
        restoreId: source.restoreId,
        choices
      }
      const result =
        source.type === "cloud"
          ? await dataApi.pullCloudBackup({
              restoreId: source.restoreId,
              choices,
              cloudSync: { ...source.cloudSync }
            })
          : source.type === "local"
            ? await dataApi.restoreLocalBackup(restorePayload)
            : await dataApi.restoreDataBackup(restorePayload)

      updateState(result.state)
      if (result.backups) {
        localBackups.value = result.backups
      }
      if (result.directory) {
        localBackupDirectory.value = result.directory
      }
      closeRestorePreview(true)
      showSuccessMessage(
        source.type === "cloud"
          ? "已从坚果云兼容恢复配置数据。"
          : source.type === "local"
            ? "已从本地备份兼容恢复配置数据。"
            : "配置数据已兼容恢复。"
      )
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function createSkill(payload) {
  const success = await runAction(() => skillApi.createSkill(payload))

  if (success) {
    showCreateSkill.value = false
    activeView.value = "skills"
  }
}

async function importSkillsFromCli() {
  await withGlobalLoading(async () => {
    try {
      const preview = await skillApi.previewSkillsFromCli()
      const candidates = Array.isArray(preview) ? preview : preview.candidates
      const conflicts = Array.isArray(preview) ? [] : preview.conflicts

      importCandidates.value = {
        candidates,
        conflicts
      }

      if (!candidates.length && !conflicts.length) {
        showSuccessMessage("当前没有可导入的 Skill。")
        return
      }

      showImportSkills.value = true
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function importSkillFromZip() {
  try {
    const zipPath = await systemApi.selectFile({
      title: "选择 Skill zip 压缩包",
      filters: [{ name: "Zip 压缩包", extensions: ["zip"] }]
    })

    if (!zipPath) {
      return
    }

    let importResult = null
    const success = await runAction(async () => {
      const result = await skillApi.importSkillFromZip({ zipPath })

      importResult = result.zipImport || null
      return result.state || result
    })

    if (success) {
      activeView.value = "skills"
      showSuccessMessage(formatZipImportMessage(importResult))
    }
  } catch (error) {
    showErrorMessage(error)
  }
}

function formatZipImportMessage(result) {
  const imported = result?.imported || []
  const skipped = result?.skipped || []

  if (imported.length) {
    const names = imported.map((item) => item.name).join("、")

    return `Skill zip 已导入 ${imported.length} 个：${names}`
  }

  if (skipped.length) {
    return "zip 中的 Skill 已存在，无需重复导入。"
  }

  return "Skill zip 已导入。"
}

async function confirmImportSkills(payload) {
  const success = await runAction(() => skillApi.importSkillsFromCli(payload))

  if (success) {
    showImportSkills.value = false
    importCandidates.value = []
    activeView.value = "skills"
    showSuccessMessage("选中的 Skill 已导入并挂载到对应 CLI。")
  }
}

async function installSkill(payload) {
  await runAction(() => skillApi.installSkill(payload))
}

async function batchSkillAction(payload) {
  let batchResult = null
  const success = await runAction(async () => {
    const result = await skillApi.batchSkillAction(payload)

    batchResult = result.batch || null
    return result.state || result
  })

  if (success) {
    showSuccessMessage(formatBatchSkillMessage(payload.action, batchResult))
  }
}

async function saveSkillGroup(payload) {
  const success = await runAction(async () => {
    const result = await skillApi.saveSkillGroup(payload)

    return result.state || result
  })

  if (success) {
    showSuccessMessage("Skill 分组已保存。")
  }
}

async function removeSkillGroup(payload) {
  const success = await runAction(async () => {
    const result = await skillApi.removeSkillGroup(payload)

    return result.state || result
  })

  if (success) {
    showSuccessMessage("Skill 分组已删除。")
  }
}

async function removeSkillGroupItems(payload) {
  const success = await runAction(async () => {
    const result = await skillApi.removeSkillGroupItems(payload)

    return result.state || result
  })

  if (success) {
    showSuccessMessage("已移出 Skill 分组。")
  }
}

async function setSkillEnabled(payload) {
  const success = await runAction(() => skillApi.setSkillEnabled(payload))

  if (success) {
    showSuccessMessage(payload.enabled ? "Skill 已恢复。" : "Skill 已禁用。")
  }
}

async function deleteSkills(payload) {
  let deleteResult = null
  const success = await runAction(async () => {
    const result = await skillApi.deleteSkills(payload)

    deleteResult = result.delete || null
    return result.state || result
  })

  if (success) {
    skillTrashItems.value = deleteResult?.trash || []
    showSuccessMessage(formatDeleteSkillMessage(deleteResult))
  }
}

async function loadSkillTrash() {
  try {
    const result = await skillApi.getSkillTrash()

    skillTrashItems.value = result.items || []
  } catch (error) {
    showErrorMessage(error)
  }
}

async function restoreSkillTrash(payload) {
  let trashResult = null
  const success = await runAction(async () => {
    const result = await skillApi.restoreSkillTrash(payload)

    trashResult = result.trash || null
    return result.state || result
  })

  if (success) {
    skillTrashItems.value = trashResult?.trash || []
    showSuccessMessage(
      formatTrashActionMessage("恢复", trashResult?.restored || [])
    )
  }
}

async function purgeSkillTrash(payload) {
  try {
    const result = await skillApi.purgeSkillTrash(payload)
    const trashResult = result.trash || null

    skillTrashItems.value = trashResult?.trash || []
    showSuccessMessage(
      formatTrashActionMessage("永久删除", trashResult?.purged || [])
    )
  } catch (error) {
    showErrorMessage(error)
  }
}

function formatBatchSkillMessage(action, result) {
  const count = result?.successes?.length || 0
  const errorCount = result?.errors?.length || 0
  const actionLabel =
    {
      "install-all": "安装",
      "uninstall-all": "卸载",
      enable: "恢复",
      disable: "禁用"
    }[action] || "处理"
  const suffix = errorCount ? `，${errorCount} 个失败` : ""

  return `已${actionLabel} ${count} 个 Skill${suffix}。`
}

function formatDeleteSkillMessage(result) {
  const count = result?.deleted?.length || 0
  const errorCount = result?.errors?.length || 0
  const suffix = errorCount ? `，${errorCount} 个失败` : ""

  return `已删除 ${count} 个 Skill 到回收站${suffix}。`
}

function formatTrashActionMessage(action, items) {
  return `已${action} ${items.length} 个 Skill。`
}

async function addSkillRepository(payload) {
  const success = await runAction(() => skillApi.addSkillRepository(payload))

  if (success) {
    activeView.value = "skills"
    showSuccessMessage("Skill 仓库已添加。")
  }
}

async function refreshSkillRepository(payload) {
  const success = await runAction(() =>
    skillApi.refreshSkillRepository(payload)
  )

  if (success) {
    showSuccessMessage("Skill 仓库已刷新。")
  }
}

async function removeSkillRepository(payload) {
  const success = await runAction(() => skillApi.removeSkillRepository(payload))

  if (success) {
    showSuccessMessage("Skill 仓库已删除。")
  }
}

async function installSkillFromRepository(payload) {
  const success = await runAction(() =>
    skillApi.installSkillFromRepository(payload)
  )

  if (success) {
    showSuccessMessage("仓库 Skill 已安装到本地。")
  }
}

async function uninstallSkill(payload) {
  await runAction(() => skillApi.uninstallSkill(payload))
}

async function repairSkill(payload) {
  await runAction(() => skillApi.repairSkill(payload))
}

async function addRepo(payload) {
  const success = await runAction(() => repoApi.addRepo(payload))

  if (success) {
    showAddRepo.value = false
    activeView.value = "tools"
  }
}

async function deleteSession(sessionId) {
  await runAction(() => sessionApi.deleteSession({ sessionId }))
}

async function saveProvider(payload) {
  const restoringProvider =
    state.providers.find((item) => item.id === payload.id)?.enabled === false &&
    payload.enabled === true
  const success = await runAction(() => providerApi.saveProvider(payload))

  if (success) {
    showSuccessMessage(
      payload.enabled === false
        ? "Provider 已禁用。"
        : restoringProvider
          ? "Provider 已恢复。"
          : "Provider 已保存。"
    )
  }
}

async function saveRule(payload) {
  const success = await runAction(() => ruleApi.saveRule(payload))

  if (success) {
    showSuccessMessage("Prompt 已保存。")
  }
}

async function deleteRule(ruleId) {
  const success = await runAction(() => ruleApi.deleteRule({ ruleId }))

  if (success) {
    showSuccessMessage("Prompt 已删除。")
  }
}

async function enableRule(payload) {
  const success = await runAction(() => ruleApi.enableRule(payload))

  if (success) {
    showSuccessMessage("Prompt 已启用并同步到全局文件。")
  }
}

async function toggleRule(payload) {
  const success = await runAction(() => ruleApi.toggleRule(payload))

  if (success && payload.enabled === false) {
    showSuccessMessage("Prompt 已取消启用。")
  }
}

async function importRule(payload) {
  const success = await runAction(() => ruleApi.importGlobalRule(payload))

  if (success) {
    showSuccessMessage("已导入当前全局 Prompt。")
  }
}

async function resolveRuleImportConflict(payload) {
  const success = await runAction(() =>
    ruleApi.resolveRuleImportConflict(payload)
  )

  if (success) {
    if (payload.source === "manager") {
      showSuccessMessage("已保留管理器版本。")
    } else {
      showSuccessMessage("已使用全局版本更新相似 Prompt。")
    }
  }
}

async function resolveRuleDrift(payload) {
  const success = await runAction(() => ruleApi.resolveRuleDrift(payload))

  if (success) {
    showSuccessMessage("Prompt Drift 已处理。")
  }
}

async function deleteProvider(providerId) {
  const success = await runAction(() =>
    providerApi.deleteProvider({ providerId })
  )

  if (success) {
    showSuccessMessage("Provider 已删除。")
  }
}

async function startCodexOfficialLogin(payload) {
  const success = await runAction(() =>
    accountApi.startCodexOfficialLogin(payload)
  )

  if (success) {
    showWarningMessage("已打开浏览器，请完成 Codex 官方登录。")
  }
}

async function cancelCodexOfficialLogin() {
  await runAction(() => accountApi.cancelCodexOfficialLogin())
}

async function importCodexAuthJson(payload) {
  const success = await runAction(() => accountApi.importCodexAuthJson(payload))

  if (success) {
    showSuccessMessage("Codex 登录 JSON 已导入。")
  }
}

async function enableCodexAccount(payload) {
  const shouldDisableCodexProxy = state.codexProxyState.enabled
  const success = await runAction(async () => {
    if (shouldDisableCodexProxy) {
      await proxyApi.disableCodexProxy()
    }

    return accountApi.enableCodexAccount(payload)
  })

  if (success) {
    showSuccessMessage(
      shouldDisableCodexProxy
        ? "Codex 代理接管已关闭，官方账号已启用。"
        : "Codex 官方账号已启用。"
    )
  }
}

async function clearCodexAccount() {
  const success = await runAction(() => accountApi.clearCodexAccount())

  if (success) {
    showSuccessMessage("Codex 官方账号已取消启用。")
  }
}

async function deleteCodexAccount(payload) {
  const success = await runAction(() => accountApi.deleteCodexAccount(payload))

  if (success) {
    showSuccessMessage("Codex 官方账号已删除。")
  }
}

async function disableCodexAccount(payload) {
  const success = await runAction(() => accountApi.disableCodexAccount(payload))

  if (success) {
    showSuccessMessage("Codex 官方账号已禁用。")
  }
}

async function restoreCodexAccount(payload) {
  const success = await runAction(() => accountApi.restoreCodexAccount(payload))

  if (success) {
    showSuccessMessage("Codex 官方账号已恢复。")
  }
}

async function refreshCodexAccount(payload) {
  const { onSettled, showSuccess, ...input } = payload

  try {
    updateState(await accountApi.refreshCodexAccount(input))

    if (showSuccess !== false) {
      showSuccessMessage("Codex 官方账号额度已刷新。")
    }
  } catch (error) {
    if (!isCodexAccountRefreshError(error)) {
      showErrorMessage(error)
    }
  } finally {
    if (onSettled) {
      onSettled()
    }
  }
}

async function refreshCodexAccounts() {
  if (!state.codexAccounts.length) {
    return
  }

  await Promise.all(
    state.codexAccounts.map(async (account) => {
      if (account.disabled) {
        return
      }

      try {
        updateState(
          await accountApi.refreshCodexAccount({
            accountId: account.id,
            syncAuth: false
          })
        )
      } catch (error) {
        if (!isCodexAccountRefreshError(error)) {
          showErrorMessage(error)
        }
      }
    })
  )
}

async function updateCodexAccountProxy(payload) {
  const success = await runAction(() =>
    accountApi.updateCodexAccountProxy(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号代理已保存。")
  }
}

async function enableCodexProxy(payload) {
  const success = await runAction(() => proxyApi.enableCodexProxy(payload))

  if (success) {
    showSuccessMessage("Codex 代理接管已开启。")
  }
}

async function enableClaudeProxy(payload) {
  const success = await runAction(() => proxyApi.enableClaudeProxy(payload))

  if (success) {
    showSuccessMessage("Claude 代理接管已开启。")
  }
}

async function disableClaudeProxy() {
  const success = await runAction(() => proxyApi.disableClaudeProxy())

  if (success) {
    showSuccessMessage("Claude 代理接管已关闭。")
  }
}

async function addClaudeProxyProvider(payload) {
  const success = await runAction(() =>
    proxyApi.addClaudeProxyProvider(payload)
  )

  if (success) {
    showSuccessMessage("Provider 已加入代理接管列表。")
  }
}

async function removeClaudeProxyProvider(payload) {
  const success = await runAction(() =>
    proxyApi.removeClaudeProxyProvider(payload)
  )

  if (success) {
    showSuccessMessage("Provider 已移出代理接管列表。")
  }
}

async function activateClaudeProxyProvider(payload) {
  const shouldEnableProxy = !state.claudeProxyState.enabled
  const success = await runAction(async () => {
    const nextState = await proxyApi.activateClaudeProxyProvider(payload)

    if (shouldEnableProxy) {
      return proxyApi.enableClaudeProxy({})
    }

    return nextState
  })

  if (success) {
    showSuccessMessage(
      shouldEnableProxy ? "Claude 代理接管已开启。" : "代理接管目标已切换。"
    )
  }
}

async function disableCodexProxy() {
  const success = await runAction(() => proxyApi.disableCodexProxy())

  if (success) {
    showSuccessMessage("Codex 代理接管已关闭。")
  }
}

async function addCodexProxyProvider(payload) {
  const success = await runAction(() => proxyApi.addCodexProxyProvider(payload))

  if (success) {
    showSuccessMessage("Provider 已加入代理接管列表。")
  }
}

async function removeCodexProxyProvider(payload) {
  const success = await runAction(() =>
    proxyApi.removeCodexProxyProvider(payload)
  )

  if (success) {
    showSuccessMessage("Provider 已移出代理接管列表。")
  }
}

async function activateCodexProxyProvider(payload) {
  const shouldEnableProxy = !state.codexProxyState.enabled
  const success = await runAction(async () => {
    const nextState = await proxyApi.activateCodexProxyProvider(payload)

    if (shouldEnableProxy) {
      return proxyApi.enableCodexProxy({})
    }

    return nextState
  })

  if (success) {
    showSuccessMessage(
      shouldEnableProxy ? "Codex 代理接管已开启。" : "代理接管目标已切换。"
    )
  }
}

async function saveCodexProxyAccountModel(payload) {
  const success = await runAction(() =>
    proxyApi.saveCodexProxyAccountModel(payload)
  )

  if (success) {
    showSuccessMessage("官方账号接管模型已保存。")
  }
}

async function launchCodexProviderInstance(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await runtimeApi.launchCodexProviderInstance(payload)

      showSuccessMessage(`Codex 实例已启动：${result.providerName}`)
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function launchClaudeProviderInstance(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await runtimeApi.launchClaudeProviderInstance(payload)

      showSuccessMessage(`Claude 实例已启动：${result.providerName}`)
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function saveRuntimeModel(payload) {
  const success = await runAction(() => runtimeApi.saveRuntimeModel(payload))

  if (success) {
    showSuccessMessage("模型已保存。")
  }
}

async function switchRuntime(payload) {
  const proxyApi = getProxyApi(payload.cli)
  const proxyState = getProxyState(payload.cli)
  const shouldDisableProxy = Boolean(proxyState?.enabled)
  const success = await runAction(async () => {
    if (shouldDisableProxy) {
      await proxyApi.disable()
    }

    return runtimeApi.switchRuntime(payload)
  })

  if (success) {
    showSuccessMessage(
      shouldDisableProxy
        ? `${payload.cli === "claude" ? "Claude" : "Codex"} 代理接管已关闭，Runtime Profile 已切换。`
        : "Runtime Profile 已切换。"
    )
  }
}

async function clearRuntime(payload) {
  const success = await runAction(() => runtimeApi.clearRuntime(payload))

  if (success) {
    showSuccessMessage("Runtime Profile 已取消使用。")
  }
}

async function resolveRuntimeDrift(payload) {
  const success = await runAction(() => runtimeApi.resolveRuntimeDrift(payload))

  if (success) {
    showSuccessMessage("Runtime 配置差异已处理。")
  }
}

async function openPath(targetPath) {
  if (!targetPath) {
    return
  }

  await withGlobalLoading(async () => {
    try {
      await systemApi.openPath({ targetPath })
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function submitCloseAction(action) {
  showCloseConfirm.value = false

  try {
    await appApi.handleCloseAction({
      action,
      remember: closeRemember.value
    })
  } catch (error) {
    showErrorMessage(error)
  }
}

async function quitApp() {
  try {
    await appApi.handleCloseAction({
      action: "quit",
      remember: false
    })
  } catch (error) {
    showErrorMessage(error)
  }
}

async function uninstallWithoutTrace() {
  try {
    await appApi.uninstallWithoutTrace()
  } catch (error) {
    showErrorMessage(error)
  }
}

onMounted(() => {
  if (isQuickSwitchPanel) {
    document.documentElement.classList.add("quick-switch-html")
    document.body.classList.add("quick-switch-body")
  }

  bootstrap()

  // 视图切换按需加载
  watch(activeView, async (view) => {
    try {
      if (view === "sessions") {
        updateState(await appApi.ensureSessionsReady())
      } else if (view === "tools") {
        updateState(await appApi.ensureToolsReady())
      } else if (view === "skills") {
        updateState(await appApi.ensureSkillsReady())
        await loadSkillTrash()
      }
    } catch (error) {
      showErrorMessage(error)
    }
  })

  unsubscribeUpdate = appApi.onUpdateStatus(applyUpdateStatus)
  appApi
    .getUpdateStatus()
    .then(applyUpdateStatus)
    .catch(() => {})
  unsubscribeClose = appApi.onCloseRequested(() => {
    closeRemember.value = false
    showCloseConfirm.value = true
  })
})

onBeforeUnmount(() => {
  if (typeof unsubscribe === "function") {
    unsubscribe()
  }

  if (typeof unsubscribeClose === "function") {
    unsubscribeClose()
  }

  if (typeof unsubscribeUpdate === "function") {
    unsubscribeUpdate()
  }

  if (isQuickSwitchPanel) {
    document.documentElement.classList.remove("quick-switch-html")
    document.body.classList.remove("quick-switch-body")
  }
})
</script>

<style scoped lang="less">
:global(html.quick-switch-html),
:global(html.quick-switch-html body),
:global(html.quick-switch-html #app) {
  background: transparent;
}

:global(html.quick-switch-html .app-message) {
  top: 34px;
  right: 8px;
  left: 8px;
  width: auto;
  gap: 5px;
  transform: none;
}

:global(html.quick-switch-html .app-message__item) {
  padding: 6px 8px;
  border-radius: 6px;
  box-shadow: 0 6px 14px rgba(34, 56, 83, 0.12);
  font-size: 12px;
  line-height: 1.25;
}

.app-shell {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  width: 125vw;
  height: 125vh;
  min-height: 0;
  transform: scale(0.8);
  transform-origin: left top;

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
    // overflow: auto;
    padding-right: 6px;
  }

  &__content--locked {
    overflow: hidden;
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

.cloud-backup-modal {
  display: flex;
  height: min(680px, calc(100vh - 180px));
  min-height: 0;
  flex-direction: column;
  gap: 12px;

  &__summary {
    display: flex;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 0.84rem;
    font-weight: 700;
  }

  &__summary span {
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--color-primary-soft);
  }

  &__body {
    display: grid;
    flex: 1;
    min-height: 0;
    grid-template-columns: 300px minmax(0, 1fr);
    gap: 12px;
  }

  &__list {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding-right: 4px;
  }

  &__entry {
    display: flex;
    min-height: 58px;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 3px;
    padding: 9px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }

  &__entry--active {
    border-color: #8eb6d9;
    background: #eef6ff;
  }

  &__entry strong {
    font-size: 0.82rem;
  }

  &__entry span {
    width: 100%;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.75rem;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__content {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
  }

  &__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 12px;
    border-bottom: 1px solid var(--color-line);
    background: #f7fafc;
  }

  &__head div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  &__head strong {
    color: var(--color-text);
    font-size: 0.88rem;
  }

  &__head span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.35;
    word-break: break-all;
  }

  &__head small {
    flex: none;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
  }

  &__content pre {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: 12px;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.76rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }
}

.close-confirm {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: grid;
  place-items: center;
  padding: 24px;

  &__overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.28);
    backdrop-filter: blur(2px);
  }

  &__panel {
    position: relative;
    width: 520px;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 18px 48px rgba(15, 23, 42, 0.2);
  }

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 18px 18px 12px;
    border-bottom: 1px solid var(--color-line);
  }

  &__header span {
    display: block;
    margin-bottom: 5px;
    color: var(--color-text-soft);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    line-height: 1;
    text-transform: uppercase;
  }

  &__header h2 {
    margin: 0;
    color: var(--color-text);
    font-size: 1.05rem;
    line-height: 1.25;
  }

  &__icon-button {
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &__icon-button:hover {
    border-color: #c8d2df;
    background: #f7f9fc;
    color: var(--color-text);
  }

  &__body {
    display: flex;
    gap: 14px;
    padding: 18px;
  }

  &__mark {
    display: grid;
    width: 44px;
    height: 44px;
    flex: 0 0 auto;
    place-items: center;
    border: 1px solid #b7d9f6;
    border-radius: 8px;
    background: #e8f4ff;
    color: #0b78d0;
  }

  &__copy {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding-top: 2px;
  }

  &__copy strong {
    color: var(--color-primary);
    font-size: 1rem;
    line-height: 1.35;
  }

  &__copy span {
    color: var(--color-text-muted);
    font-size: 0.84rem;
    line-height: 1.6;
  }

  &__footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 12px 18px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  &__remember {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 600;
  }

  &__remember input {
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: var(--color-primary);
  }

  &__actions {
    display: flex;
    gap: 8px;
  }

  &__button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 700;
  }

  &__button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  &__button--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  &__button--primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }
}
</style>

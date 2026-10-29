<template>
  <section class="settings-view">
    <header class="settings-view__header">
      <div>
        <h1>设置</h1>
      </div>
      <div class="settings-view__header-actions">
        <button
          class="settings-view__check-update"
          type="button"
          @click="$emit('check-update')"
        >
          <RefreshCw :size="16" />
          <div class="text">检查更新</div>
        </button>
      </div>
    </header>

    <nav class="settings-view__tabs">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        :class="[
          'settings-view__tab',
          { 'settings-view__tab--active': activeTab === tab.id }
        ]"
        type="button"
        @click="activeTab = tab.id"
      >
        <component :is="tab.icon" :size="15" />
        <span class="tab-label">{{ tab.label }}</span>
      </button>
    </nav>

    <div class="settings-view__content">
      <div v-if="activeTab === 'directories'" class="settings-view__tab-panel">
        <section class="settings-view__panel">
          <div class="settings-view__panel-header">
            <div>
              <h2>Data 目录</h2>
              <span>当前运行目录：{{ appSettings.dataPath }}</span>
            </div>
            <button type="button" @click="$emit('open-path', draft.dataPath)">
              <FolderOpen :size="16" />
              打开
            </button>
          </div>

          <label class="settings-view__field">
            <span>Data 存放位置</span>
            <div class="settings-view__input-row">
              <input
                v-model.trim="draft.dataPath"
                type="text"
                @change="saveChanges({ dataPath: draft.dataPath })"
                @keydown.enter="$event.target.blur()"
              />
              <button
                type="button"
                @click="selectDirectory('dataPath', draft.dataPath)"
              >
                选择
              </button>
              <button type="button" @click="resetDataPath">
                <RotateCcw :size="15" />
              </button>
            </div>
          </label>

          <p v-if="appSettings.restartRequired" class="settings-view__warning">
            Data 目录已更新，重启应用后生效。
          </p>
          <p class="settings-view__hint">
            启动配置保存于：{{ appSettings.settingsFilePath }}
          </p>
        </section>

        <section class="settings-view__panel">
          <div class="settings-view__panel-header">
            <div>
              <h2>CLI 配置目录</h2>
              <span>保存后会重新检测 CLI、Skill 挂载和 Session 索引。</span>
            </div>
          </div>

          <div class="settings-view__cli-list">
            <article
              v-for="item in cliItems"
              :key="item.id"
              class="settings-view__cli-card"
            >
              <div class="settings-view__cli-title">
                <div>
                  <span data-emphasis>{{ item.name }}</span>
                  <small>{{ item.statusText }}</small>
                </div>
                <span
                  :class="[
                    'settings-view__cli-status',
                    { 'settings-view__cli-status--offline': !item.installed }
                  ]"
                >
                  {{ item.installed ? "已检测" : "未检测" }}
                </span>
              </div>

              <label class="settings-view__field">
                <span>{{ item.name }} 配置目录</span>
                <div class="settings-view__input-row">
                  <input
                    v-model.trim="draft.cliConfigPaths[item.id]"
                    type="text"
                    @change="
                      saveChanges({
                        cliConfigPaths: {
                          [item.id]: draft.cliConfigPaths[item.id]
                        }
                      })
                    "
                    @keydown.enter="$event.target.blur()"
                  />
                  <button
                    type="button"
                    @click="
                      selectDirectory(item.id, draft.cliConfigPaths[item.id])
                    "
                  >
                    选择
                  </button>
                  <button type="button" @click="resetCliPath(item.id)">
                    <RotateCcw :size="15" />
                  </button>
                  <button
                    type="button"
                    @click="$emit('open-path', draft.cliConfigPaths[item.id])"
                  >
                    <FolderOpen :size="15" />
                  </button>
                </div>
              </label>

              <div class="settings-view__cli-meta">
                <span
                  >配置目录：{{ item.detectedPath || item.defaultPath }}</span
                >
                <span>Skill 目录：{{ item.skillsPath || "未检测" }}</span>
                <span>版本：{{ item.version || "未检测" }}</span>
              </div>
            </article>
          </div>
        </section>
      </div>

      <section v-else-if="activeTab === 'data'" class="settings-view__panel">
        <div class="settings-view__data-list">
          <article class="settings-view__data-card">
            <div class="settings-view__data-copy">
              <span data-emphasis>导出到本地文件</span>
              <span>选择保存位置，生成加密的 .aimbackup 本地备份文件。</span>
            </div>
            <button type="button" @click="$emit('export-data')">
              <Download :size="16" />
              导出
            </button>
          </article>

          <article class="settings-view__data-card">
            <div class="settings-view__data-copy">
              <span data-emphasis>从本地备份文件恢复</span>
              <span
                >选择 .aimbackup
                文件，预览冲突后再恢复并保留本机启用状态。</span
              >
            </div>
            <button type="button" @click="$emit('restore-data')">
              <Upload :size="16" />
              恢复
            </button>
          </article>
        </div>

        <section class="settings-view__cloud">
          <div class="settings-view__cloud-header">
            <div>
              <span data-emphasis>本地自动备份</span>
              <span
                >按固定间隔生成本地加密备份，超过保留数量后删除最久的备份。</span
              >
            </div>
            <span class="settings-view__cloud-time">
              上次备份：{{ formatBackupTime(draft.localBackup.lastBackupAt) }}
            </span>
          </div>

          <div class="settings-view__local-toggle">
            <div class="settings-view__choice-group">
              <label
                :class="[
                  'settings-view__choice',
                  { 'settings-view__choice--active': draft.localBackup.enabled }
                ]"
              >
                <input
                  v-model="draft.localBackup.enabled"
                  type="radio"
                  name="local-backup-enabled"
                  :value="true"
                  @change="
                    saveChanges({
                      localBackup: { enabled: draft.localBackup.enabled }
                    })
                  "
                />
                <span>启用</span>
              </label>
              <label
                :class="[
                  'settings-view__choice',
                  {
                    'settings-view__choice--active': !draft.localBackup.enabled
                  }
                ]"
              >
                <input
                  v-model="draft.localBackup.enabled"
                  type="radio"
                  name="local-backup-enabled"
                  :value="false"
                  @change="
                    saveChanges({
                      localBackup: { enabled: draft.localBackup.enabled }
                    })
                  "
                />
                <span>暂停</span>
              </label>
            </div>
          </div>

          <div class="settings-view__cloud-grid">
            <label class="settings-view__field">
              <span>备份间隔（分钟）</span>
              <input
                v-model.number="draft.localBackup.intervalMinutes"
                type="number"
                required
                min="1"
                step="1"
                @change="
                  $event.target.reportValidity() &&
                  saveChanges({
                    localBackup: {
                      intervalMinutes: draft.localBackup.intervalMinutes
                    }
                  })
                "
                @keydown.enter="$event.target.blur()"
              />
            </label>
            <label class="settings-view__field">
              <span>最多保留（份）</span>
              <input
                v-model.number="draft.localBackup.maxCount"
                type="number"
                required
                min="1"
                step="1"
                @change="
                  $event.target.reportValidity() &&
                  saveChanges({
                    localBackup: { maxCount: draft.localBackup.maxCount }
                  })
                "
                @keydown.enter="$event.target.blur()"
              />
            </label>
          </div>

          <div class="settings-view__backup-directory">
            <span>{{ localBackupDirectory || "尚未创建本地备份目录" }}</span>
            <button
              type="button"
              :disabled="!localBackupDirectory"
              @click="$emit('open-path', localBackupDirectory)"
            >
              <FolderOpen :size="16" />
              打开目录
            </button>
          </div>

          <div class="settings-view__cloud-actions">
            <button
              type="button"
              :disabled="pending"
              @click="$emit('local-backup-now')"
            >
              <Download :size="16" />
              立即备份
            </button>
            <button
              type="button"
              :disabled="pending"
              @click="$emit('local-backups-refresh')"
            >
              <RotateCcw :size="16" />
              刷新列表
            </button>
          </div>

          <div class="settings-view__backup-list">
            <article
              v-if="!localBackups.length"
              class="settings-view__backup-empty"
            >
              暂无本地备份。
            </article>
            <article
              v-for="backup in localBackups"
              :key="backup.id"
              class="settings-view__backup-item"
            >
              <div>
                <span data-emphasis>{{ backup.fileName }}</span>
                <span
                  >{{ formatBackupTime(backup.createdAt) }} ·
                  {{ formatBackupSize(backup.size) }}</span
                >
              </div>
              <button
                type="button"
                :disabled="pending"
                @click="$emit('local-backup-restore', backup)"
              >
                <Upload :size="16" />
                恢复
              </button>
            </article>
          </div>
        </section>

        <section
          v-for="syncItem in cloudSyncItems"
          :key="syncItem.settingKey"
          class="settings-view__cloud"
        >
          <div class="settings-view__cloud-header">
            <div>
              <span data-emphasis>{{ syncItem.label }}同步</span>
              <span>{{ syncItem.description }}</span>
            </div>
            <span class="settings-view__cloud-time">
              上次同步：{{
                formatCloudSyncTime(draft[syncItem.settingKey].lastUpdatedAt)
              }}
            </span>
          </div>

          <div class="settings-view__cloud-grid">
            <label class="settings-view__field">
              <span>WebDAV 地址</span>
              <input
                v-model.trim="draft[syncItem.settingKey].webdavUrl"
                type="text"
                @change="
                  saveChanges({
                    [syncItem.settingKey]: {
                      webdavUrl: draft[syncItem.settingKey].webdavUrl
                    }
                  })
                "
                @keydown.enter="$event.target.blur()"
              />
            </label>
            <label class="settings-view__field">
              <span>备份文件名</span>
              <input
                v-model.trim="draft[syncItem.settingKey].fileName"
                type="text"
                @change="
                  saveChanges({
                    [syncItem.settingKey]: {
                      fileName: draft[syncItem.settingKey].fileName
                    }
                  })
                "
                @keydown.enter="$event.target.blur()"
              />
            </label>
            <label class="settings-view__field">
              <span>{{ syncItem.accountLabel }}</span>
              <input
                v-model.trim="draft[syncItem.settingKey].username"
                type="text"
                @change="
                  saveChanges({
                    [syncItem.settingKey]: {
                      username: draft[syncItem.settingKey].username
                    }
                  })
                "
                @keydown.enter="$event.target.blur()"
              />
            </label>
            <label class="settings-view__field">
              <span>应用密码</span>
              <el-input
                v-model="draft[syncItem.settingKey].password"
                type="password"
                show-password
                @change="
                  saveChanges({
                    [syncItem.settingKey]: {
                      password: draft[syncItem.settingKey].password
                    }
                  })
                "
              />
            </label>
          </div>

          <div class="settings-view__cloud-actions">
            <button
              type="button"
              :disabled="pending"
              @click="emitCloudSync('push-cloud-data', syncItem)"
            >
              <UploadCloud :size="16" />
              推送到{{ syncItem.label }}
            </button>
            <button
              type="button"
              :disabled="pending"
              @click="emitCloudSync('inspect-cloud-data', syncItem)"
            >
              <Eye :size="16" />
              查看云端备份
            </button>
            <button
              type="button"
              :disabled="pending"
              @click="emitCloudSync('pull-cloud-data', syncItem)"
            >
              <DownloadCloud :size="16" />
              从{{ syncItem.label }}恢复
            </button>
          </div>
        </section>
      </section>

      <section
        v-else-if="activeTab === 'agents'"
        class="settings-view__panel settings-agent-panel"
      >
        <div class="settings-agent-list">
          <ScreenshotExtension
            :providers="providers"
            :codex-accounts="codexAccounts"
            :runtime-models="runtimeModels"
          />
          <article
            class="settings-agent-card"
            tabindex="0"
            role="button"
            aria-label="进入划词翻译配置"
            @click="showTranslationConfigModal = true"
            @keydown.enter="showTranslationConfigModal = true"
            @keydown.space.prevent="showTranslationConfigModal = true"
          >
            <div class="settings-agent-main">
              <div class="settings-agent-icon"><Languages :size="24" /></div>
              <div class="settings-agent-copy">
                <h3 class="settings-agent-title">划词翻译</h3>
                <p class="settings-agent-description">
                  选中文本后右键翻译，支持查看翻译历史和消耗记录。
                </p>
              </div>
            </div>
            <div class="settings-agent-divider"></div>
            <div class="settings-agent-actions" @click.stop @keydown.stop>
              <div class="settings-agent-buttons">
                <button
                  class="settings-agent-btn"
                  type="button"
                  @click="showTranslationHistoryModal = true"
                >
                  <Clock :size="15" />
                  <span>使用记录</span>
                </button>
                <button
                  class="settings-agent-btn"
                  type="button"
                  @click="showTranslationConfigModal = true"
                >
                  <Settings :size="15" />
                  <span>配置</span>
                </button>
              </div>
              <div class="settings-agent-switch-row">
                <button
                  class="settings-agent-switch"
                  :class="{
                    'settings-agent-switch--active':
                      draft.agents.translation.enabled
                  }"
                  type="button"
                  role="switch"
                  aria-label="启用划词翻译"
                  :aria-checked="draft.agents.translation.enabled"
                  :disabled="pending"
                  @click="toggleTranslationAgent"
                >
                  <span class="settings-agent-switch-thumb"></span>
                </button>
              </div>
            </div>
          </article>
        </div>
      </section>

      <section
        v-else-if="activeTab === 'desktop'"
        class="settings-view__panel settings-desktop-panel"
      >
        <DesktopPetList
          :pets="desktopPets"
          :selected-pet-id="draft.desktop.petId"
          :running-pet-id="runningDesktopPet?.id || ''"
          :enabled="
            Boolean(draft.desktop.enabled && draft.system.quickSwitchVisible)
          "
          :pending="pending"
          @detail="activeDesktopPetId = $event"
          @toggle-master="handleToggleMaster"
        />
      </section>

      <SystemSettings
        v-else-if="activeTab === 'system'"
        :settings="appSettings.system"
        :cli-targets="cliTargets"
        :capabilities="appSettings.capabilities"
        :pending="pending"
        :save-settings="saveSettings"
        @quit-app="$emit('quit-app')"
        @uninstall-without-trace="$emit('uninstall-without-trace')"
      />
    </div>
    <DesktopPetDetail
      v-if="activeTab === 'desktop' && activeDesktopPetId && selectedDesktopPet"
      :pets="desktopPets"
      :pet="selectedDesktopPet"
      :running-pet-id="runningDesktopPet?.id || ''"
      :running="
        draft.desktop.enabled &&
        draft.system.quickSwitchVisible &&
        draft.desktop.petId === selectedDesktopPet.id
      "
      :pending="pending"
      @close="activeDesktopPetId = ''"
      @select="activeDesktopPetId = $event"
      @start="startDesktopPet"
      @stop="stopDesktopPet"
    />
    <TranslationConfigModal
      v-if="showTranslationConfigModal"
      v-model="draft.agents.translation"
      :providers="providers"
      :codex-accounts="codexAccounts"
      :runtime-models="runtimeModels"
      @save="saveTranslationChanges"
      @close="showTranslationConfigModal = false"
    />
    <TranslationHistoryModal
      v-if="showTranslationHistoryModal"
      @close="showTranslationHistoryModal = false"
    />
  </section>
</template>

<script setup>
import { computed, nextTick, reactive, ref, watch } from "vue"
import {
  ChevronRight,
  Clock,
  Download,
  DownloadCloud,
  Eye,
  FolderOpen,
  Languages,
  Monitor,
  Puzzle,
  RefreshCw,
  RotateCcw,
  Settings,
  Upload,
  UploadCloud
} from "lucide-vue-next"
import { systemApi } from "@/api"
import { desktopPets, getDesktopPet } from "@/constants/desktop-pets"
import SystemSettings from "@/features/settings/components/SystemSettings.vue"
import ScreenshotExtension from "@/features/settings/components/ScreenshotExtension.vue"
import DesktopPetList from "@/features/settings/components/DesktopPetList.vue"
import DesktopPetDetail from "@/features/settings/components/DesktopPetDetail.vue"
import TranslationConfigModal from "./components/TranslationConfigModal.vue"
import TranslationHistoryModal from "./components/TranslationHistoryModal.vue"

const props = defineProps({
  appSettings: {
    type: Object,
    required: true
  },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] },
  cliTargets: {
    type: Array,
    required: true
  },
  localBackups: {
    type: Array,
    default: () => []
  },
  localBackupDirectory: {
    type: String,
    default: ""
  },
  pending: {
    type: Boolean,
    required: true
  },
  saveSettings: {
    type: Function,
    required: true
  }
})

const emit = defineEmits([
  "check-update",
  "open-path",
  "export-data",
  "restore-data",
  "local-backup-now",
  "local-backups-refresh",
  "local-backup-restore",
  "quit-app",
  "push-cloud-data",
  "inspect-cloud-data",
  "pull-cloud-data",
  "uninstall-without-trace"
])

const activeTab = ref("directories")
const activeAgent = ref("")
const showTranslationConfigModal = ref(false)
const showTranslationHistoryModal = ref(false)
// 浏览详情不改变待启动的桌宠，启动后再由已保存配置同步选择。
const activeDesktopPetId = ref("")

// 两个 WebDAV 服务保留独立配置，避免切换同步目标时覆盖凭据。
const cloudSyncItems = [
  {
    settingKey: "cloudSync",
    provider: "jianguoyun",
    label: "坚果云",
    accountLabel: "坚果云账号",
    description: "通过坚果云 WebDAV 同步加密备份文件。",
    defaultUrl: "https://dav.jianguoyun.com/dav/AI-Manager"
  },
  {
    settingKey: "koofrSync",
    provider: "koofr",
    label: "Koofr",
    accountLabel: "Koofr 账号",
    description: "通过 Koofr WebDAV 同步加密备份文件。",
    defaultUrl: "https://app.koofr.net/dav/Koofr/AI-Manager"
  }
]

const draft = reactive({
  desktop: { enabled: false, petId: "" },
  agents: {
    translation: {
      enabled: false,
      targetId: "",
      model: "",
      targetLanguage: "简体中文",
      shortcut: "Ctrl+Alt+T",
      clipboardShortcut: "Ctrl+Alt+Shift+T"
    }
  },
  dataPath: "",
  cliConfigPaths: {
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
    fileName: "",
    lastUpdatedAt: 0
  },
  koofrSync: {
    provider: "koofr",
    webdavUrl: "",
    username: "",
    password: "",
    fileName: "",
    lastUpdatedAt: 0
  },
  localBackup: {
    enabled: true,
    intervalMinutes: 60,
    maxCount: 20,
    lastBackupAt: 0
  },
  system: { quickSwitchVisible: true }
})

// 记录已落盘值和正在保存的字段，避免较早的响应覆盖后续操作或其他输入草稿。
const savedFields = new Map()
const pendingFields = new Map()

function settingFields(value, path = []) {
  return Object.entries(value).flatMap(([key, item]) => {
    const fieldPath = [...path, key]
    return item && typeof item === "object"
      ? settingFields(item, fieldPath)
      : [[fieldPath, item]]
  })
}

async function saveChanges(patch) {
  const fields = settingFields(patch)
  if (
    fields.every(
      ([path, value]) =>
        !pendingFields.has(path.join(".")) &&
        savedFields.get(path.join(".")) === value
    )
  )
    return true
  const request = Symbol()
  for (const [path, value] of fields) {
    const target = path.slice(0, -1).reduce((item, key) => item[key], draft)
    target[path.at(-1)] = value
    pendingFields.set(path.join("."), request)
  }
  const success = await props.saveSettings(patch)
  await nextTick()
  for (const [path, value] of fields) {
    const key = path.join(".")
    if (pendingFields.get(key) !== request) continue
    pendingFields.delete(key)
    const target = path.slice(0, -1).reduce((item, part) => item[part], draft)
    // 成功时采用后端规范化后的值，失败时恢复已保存值；保留保存期间继续输入的内容。
    if (target[path.at(-1)] === value)
      target[path.at(-1)] = savedFields.get(key)
  }
  return success
}

const cliNames = {
  claude: "Claude",
  codex: "Codex"
  // 当前版本暂不启用 Gemini。
  // gemini: "Gemini"
}

const tabs = [
  {
    id: "directories",
    label: "目录",
    icon: FolderOpen
  },
  {
    id: "data",
    label: "数据管理",
    icon: Download
  },
  {
    id: "system",
    label: "系统设置",
    icon: Settings
  },
  { id: "desktop", label: "桌面管理", icon: Monitor },
  { id: "agents", label: "功能扩展", icon: Puzzle }
]

// 当前选择直接驱动全部动作预览，不影响已保存的桌宠配置。
const selectedDesktopPet = computed(() =>
  getDesktopPet(activeDesktopPetId.value || draft.desktop.petId)
)
// 运行状态只读取已保存且可见的桌宠，避免把草稿选择显示为正在运行。
const runningDesktopPet = computed(() =>
  props.appSettings.desktop?.enabled &&
  props.appSettings.system?.quickSwitchVisible !== false
    ? getDesktopPet(props.appSettings.desktop.petId)
    : null
)

function startDesktopPet(pet) {
  if (props.pending || !pet) return
  // 立即启动只更新桌宠和悬浮窗，沿用其他已保存配置。
  saveChanges({
    desktop: { enabled: true, petId: pet.id },
    system: { quickSwitchVisible: true }
  })
}

function stopDesktopPet() {
  if (props.pending) return
  // 停止时保留上次启动的桌宠和悬浮窗显示设置。
  saveChanges({
    desktop: { enabled: false }
  })
}

function handleToggleMaster(val) {
  if (props.pending) return
  if (val) {
    // 总开关启动时沿用上次保存的桌宠，没有记录时由 getDesktopPet 回退到第一只。
    const petToStart = getDesktopPet(draft.desktop.petId)
    startDesktopPet(petToStart)
  } else {
    stopDesktopPet()
  }
}

const cliItems = computed(() => {
  return Object.entries(cliNames).map(([id, name]) => {
    const detected = props.cliTargets.find((item) => item.id === id)

    return {
      id,
      name,
      installed: Boolean(detected?.installed),
      statusText: detected?.installed
        ? "配置目录或二进制已找到"
        : "未发现配置目录与可执行文件",
      detectedPath: detected?.configPath || "",
      skillsPath: detected?.skillsPath || "",
      version: detected?.version || "",
      defaultPath: props.appSettings.defaultCliConfigPaths?.[id] || ""
    }
  })
})

function syncDraft() {
  const nextDraft = {
    desktop: {},
    agents: {},
    cliConfigPaths: {},
    cloudSync: {},
    koofrSync: {},
    localBackup: {},
    system: {}
  }
  // 关闭只停用桌宠，不清除上次选择；旧素材不存在时选择第一只。
  nextDraft.desktop.enabled = Boolean(props.appSettings.desktop?.enabled)
  nextDraft.desktop.petId =
    getDesktopPet(props.appSettings.desktop?.petId)?.id || ""
  // 保存后的全局配置驱动划词入口；未保存的编辑仅留在设置草稿中。
  nextDraft.agents.translation = {
    enabled: false,
    targetId: "",
    model: "",
    targetLanguage: "简体中文",
    shortcut: "Ctrl+Alt+T",
    clipboardShortcut: "Ctrl+Alt+Shift+T",
    ...props.appSettings.agents?.translation
  }
  nextDraft.dataPath = props.appSettings.dataPath || ""
  nextDraft.cliConfigPaths.claude =
    props.appSettings.cliConfigPaths?.claude || ""
  nextDraft.cliConfigPaths.codex = props.appSettings.cliConfigPaths?.codex || ""
  // 当前版本暂不启用 Gemini。
  // draft.cliConfigPaths.gemini = props.appSettings.cliConfigPaths?.gemini || ""
  for (const syncItem of cloudSyncItems) {
    const source = props.appSettings[syncItem.settingKey] || {}
    const target = nextDraft[syncItem.settingKey]

    target.provider = syncItem.provider
    target.webdavUrl = source.webdavUrl || syncItem.defaultUrl
    target.username = source.username || ""
    target.password = source.password || ""
    target.fileName = source.fileName || "ai-manager.aimbackup"
    target.lastUpdatedAt = Number(source.lastUpdatedAt || 0)
  }
  nextDraft.localBackup.enabled =
    props.appSettings.localBackup?.enabled !== false
  nextDraft.localBackup.intervalMinutes = Number(
    props.appSettings.localBackup?.intervalMinutes || 60
  )
  nextDraft.localBackup.maxCount = Number(
    props.appSettings.localBackup?.maxCount || 20
  )
  nextDraft.localBackup.lastBackupAt = Number(
    props.appSettings.localBackup?.lastBackupAt || 0
  )
  nextDraft.system.quickSwitchVisible =
    props.appSettings.system?.quickSwitchVisible !== false
  for (const [path, value] of settingFields(nextDraft)) {
    const key = path.join(".")
    const target = path.slice(0, -1).reduce((item, part) => item[part], draft)
    if (
      !pendingFields.has(key) &&
      (!savedFields.has(key) || target[path.at(-1)] === savedFields.get(key))
    ) {
      target[path.at(-1)] = value
    }
    savedFields.set(key, value)
  }
}

async function selectDirectory(key, currentPath) {
  const selectedPath = await systemApi.selectDirectory({
    title: "选择目录",
    defaultPath: currentPath || props.appSettings.dataPath
  })

  if (!selectedPath) {
    return
  }

  if (key === "dataPath") {
    draft.dataPath = selectedPath
    saveChanges({ dataPath: selectedPath })
    return
  }

  draft.cliConfigPaths[key] = selectedPath
  saveChanges({ cliConfigPaths: { [key]: selectedPath } })
}

function resetDataPath() {
  draft.dataPath = props.appSettings.defaultDataPath
  saveChanges({ dataPath: draft.dataPath })
}

function resetCliPath(key) {
  draft.cliConfigPaths[key] =
    props.appSettings.defaultCliConfigPaths?.[key] || ""
  saveChanges({ cliConfigPaths: { [key]: draft.cliConfigPaths[key] } })
}

function toggleTranslationAgent() {
  draft.agents.translation.enabled = !draft.agents.translation.enabled
  saveChanges({
    agents: { translation: { enabled: draft.agents.translation.enabled } }
  })
}

function saveTranslationChanges(changes) {
  saveChanges({ agents: { translation: changes } })
}

function emitCloudSync(eventName, syncItem) {
  const cloudSync = draft[syncItem.settingKey]

  if (
    !cloudSync.webdavUrl ||
    !cloudSync.username ||
    !cloudSync.password ||
    !cloudSync.fileName
  ) {
    window.alert(
      `请先填写${syncItem.label} WebDAV 地址、账号、应用密码和文件名`
    )
    return
  }

  emit(eventName, {
    provider: cloudSync.provider,
    webdavUrl: cloudSync.webdavUrl,
    username: cloudSync.username,
    password: cloudSync.password,
    fileName: cloudSync.fileName,
    lastUpdatedAt: cloudSync.lastUpdatedAt
  })
}

function formatCloudSyncTime(value) {
  const timestamp = Number(value || 0)

  if (!timestamp) {
    return "尚未同步"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(timestamp))
}

function formatBackupTime(value) {
  const timestamp = Number(value || 0)

  if (!timestamp) {
    return "尚未备份"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(timestamp))
}

function formatBackupSize(value) {
  const size = Number(value || 0)

  if (size < 1024) {
    return `${size} B`
  }

  if (size < 1024 * 1024) {
    return `${(size / 1024).toFixed(1)} KB`
  }

  return `${(size / 1024 / 1024).toFixed(1)} MB`
}

watch(
  () => props.appSettings,
  () => {
    syncDraft()
  },
  { deep: true, immediate: true }
)
</script>

<style scoped lang="less">
.settings-view {
  .settings-desktop-panel {
    display: flex;
    flex-direction: column;
  }
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--font-size-base);

  &__header,
  &__panel {
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__header {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 16px 18px;
  }

  &__header p {
    margin: 0 0 5px;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  &__header h1 {
    margin: 0;
    font-size: var(--font-size-xl);
    line-height: 1.2;
  }

  &__header-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  &__tabs {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 4px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    .tab-label {
      font-size: var(--font-size-xs);
      line-height: 12px;
    }
  }

  &__tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-sm);
  }

  &__tab--active {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__check-update,
  &__panel-header button,
  &__input-row button,
  &__data-card button,
  &__backup-directory button,
  &__backup-item button,
  &__cloud-actions button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: var(--font-size-sm);
  }

  &__check-update {
    padding: 0 12px;
    .text {
      font-size: var(--font-size-sm);
      line-height: 14px;
    }
  }

  &__cloud-actions button:disabled,
  &__backup-directory button:disabled,
  &__backup-item button:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }

  &__panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
  }

  &__content {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    gap: 12px;
    overflow-x: hidden;
    overflow-y: auto;
    padding-right: 4px;
  }

  &__tab-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  &__panel-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  &__panel-header h2 {
    margin: 0 0 5px;
    font-size: var(--font-size-lg);
    line-height: 1.35;
  }

  &__panel-header span:not([data-emphasis]),
  &__hint {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    word-break: break-all;
  }

  &__field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  &__field > span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  &__input-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto auto;
    gap: 8px;
  }

  &__input-row input {
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    font-size: var(--font-size-sm);
  }

  &__field input {
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    font-size: var(--font-size-sm);
  }

  &__input-row button {
    min-width: 32px;
    padding: 0 10px;
  }

  &__warning {
    margin: 0;
    padding: 9px 11px;
    border: 1px solid var(--color-warning-line);
    border-radius: 8px;
    background: var(--color-warning-soft);
    color: var(--color-warning);
    font-size: var(--font-size-sm);
  }

  &__hint {
    margin: 0;
  }

  &__cli-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  &__data-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  &__cli-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 13px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__cli-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__cli-title div {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  &__cli-title [data-emphasis] {
    font-size: var(--font-size-base);
    line-height: 1.35;
  }

  &__cli-title small {
    color: var(--color-text-soft);
    font-size: var(--font-size-sm);
    line-height: 1.45;
    text-align: right;
    word-break: break-all;
  }

  &__cli-title div small {
    text-align: left;
  }

  &__cli-status {
    flex: 0 0 auto;
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--color-success-soft);
    color: var(--color-success);
    font-size: var(--font-size-xs);
  }

  &__cli-status--offline {
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
  }

  &__cli-meta {
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: var(--color-text-soft);
    font-size: var(--font-size-sm);
    line-height: 1.45;
    word-break: break-all;
  }

  &__data-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__choice-group {
    display: flex;
    flex: none;
    gap: 6px;
    padding: 4px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__choice {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 28px;
    padding: 0 10px;
    border-radius: 6px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-sm);
  }

  &__choice input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  &__choice--active {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__cloud {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 13px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__cloud-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  &__cloud-header div {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  &__cloud-header [data-emphasis] {
    font-size: var(--font-size-base);
    line-height: 1.35;
  }

  &__cloud-header span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  &__cloud-time {
    flex: none;
    padding-top: 1px;
    color: var(--color-text-soft);
    font-size: var(--font-size-sm);
  }

  &__cloud-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  &__cloud-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  &__local-toggle {
    display: flex;
    justify-content: flex-start;
  }

  &__backup-directory {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__backup-directory span:not([data-emphasis]) {
    min-width: 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    word-break: break-all;
  }

  &__backup-directory button,
  &__backup-item button {
    flex: none;
    padding: 0 10px;
  }

  &__backup-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 260px;
    overflow-y: auto;
    padding-right: 4px;
  }

  &__backup-empty {
    padding: 10px;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }

  &__backup-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__backup-item div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }

  &__backup-item [data-emphasis] {
    overflow: hidden;
    font-size: var(--font-size-sm);
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__backup-item span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    line-height: 1.45;
  }

  &__data-copy {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  &__data-copy [data-emphasis] {
    font-size: var(--font-size-base);
    line-height: 1.35;
  }

  &__data-copy span:not([data-emphasis]) {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  .settings-agent-panel {
    min-height: 0;

    .settings-agent-list {
      display: flex;
      flex-direction: column;
      gap: 24px;

      .settings-agent-card {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 22px 28px;
        border: 1px solid var(--color-line);
        border-radius: 16px;
        background: var(--color-panel);
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
        cursor: pointer;
        transition:
          box-shadow 0.2s ease,
          transform 0.2s ease;

        &:hover {
          box-shadow: 0 6px 20px rgba(0, 0, 0, 0.06);
          transform: translateY(-1px);
        }

        &:focus-visible {
          outline: 2px solid #2563eb;
          outline-offset: 2px;
        }

        .settings-agent-main {
          display: flex;
          align-items: center;
          gap: 18px;
          min-width: 0;
          flex: 1;

          .settings-agent-icon {
            display: flex;
            width: 52px;
            height: 52px;
            flex: none;
            align-items: center;
            justify-content: center;
            border-radius: 14px;
            background: #eff6ff;
            color: #2563eb;
          }

          .settings-agent-copy {
            display: flex;
            min-width: 0;
            flex-direction: column;
            gap: 4px;

            .settings-agent-title {
              margin: 0;
              font-size: 18px;
              color: var(--color-text);
              letter-spacing: -0.01em;
            }

            .settings-agent-description {
              margin: 0;
              color: var(--color-text-muted);
              font-size: 13px;
              line-height: 1.5;
            }
          }
        }

        .settings-agent-divider {
          width: 1px;
          height: 54px;
          flex-shrink: 0;
          margin: 0 32px;
          background: var(--color-line);
        }

        .settings-agent-actions {
          display: flex;
          align-items: center;
          gap: 16px;
          flex: none;

          .settings-agent-switch-row {
            display: flex;
            align-items: center;
            gap: 10px;

            .settings-agent-switch {
              position: relative;
              width: 44px;
              height: 24px;
              flex-shrink: 0;
              border: none;
              border-radius: 12px;
              background: #cbd5e1;
              cursor: pointer;
              padding: 2px;
              transition: background 0.2s ease;

              &:disabled {
                opacity: 0.6;
                cursor: not-allowed;
              }

              &.settings-agent-switch--active {
                background: #2563eb;

                .settings-agent-switch-thumb {
                  transform: translateX(20px);
                }
              }

              .settings-agent-switch-thumb {
                display: block;
                width: 20px;
                height: 20px;
                border-radius: 50%;
                background: #ffffff;
                box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
                transition: transform 0.2s ease;
              }
            }

            .settings-agent-switch-label {
              color: var(--color-text);
              font-size: 15px;
            }
          }

          .settings-agent-buttons {
            display: flex;
            align-items: center;
            gap: 10px;

            .settings-agent-btn {
              display: inline-flex;
              align-items: center;
              gap: 6px;
              padding: 6px 10px;
              border: 1px solid var(--color-line);
              border-radius: 8px;
              background: var(--color-panel);
              color: var(--color-text);
              font-size: 13px;
              cursor: pointer;
              transition:
                background 0.15s ease,
                border-color 0.15s ease;

              &:hover {
                background: var(--color-panel-soft);
                border-color: var(--color-line-strong);
              }
            }
          }
        }
      }
    }
  }
}
</style>

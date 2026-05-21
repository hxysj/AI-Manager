<template>
  <section class="settings-view">
    <header class="settings-view__header">
      <div>
        <p>Application Settings</p>
        <h1>设置</h1>
      </div>
      <div class="settings-view__header-actions">
        <div
          class="settings-view__check-update"
          type="button"
          @click="$emit('check-update')"
        >
          <RefreshCw :size="16" />
          <div class="text">检查更新</div>
        </div>
        <button
          class="settings-view__save"
          type="button"
          :disabled="pending"
          @click="submitSettings"
        >
          <Save :size="16" />
          <div class="text">{{ pending ? "保存中..." : "保存设置" }}</div>
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
              <input v-model.trim="draft.dataPath" type="text" />
              <button
                type="button"
                @click="selectDirectory('dataPath', draft.dataPath)"
              >
                选择
              </button>
              <button
                type="button"
                @click="draft.dataPath = appSettings.defaultDataPath"
              >
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
                  <strong>{{ item.name }}</strong>
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
        <div class="settings-view__panel-header">
          <div>
            <h2>数据管理</h2>
            <span>导出会加密打包当前配置，恢复会兼容合并并刷新状态。</span>
          </div>
        </div>

        <div class="settings-view__data-list">
          <article class="settings-view__data-card">
            <div class="settings-view__data-copy">
              <strong>导出配置</strong>
              <span
                >备份当前 Skills、Prompts、Providers、Runtime 和存储数据。</span
              >
            </div>
            <button type="button" @click="$emit('export-data')">
              <Download :size="16" />
              导出
            </button>
          </article>

          <article class="settings-view__data-card">
            <div class="settings-view__data-copy">
              <strong>恢复配置</strong>
              <span>从加密备份中合并配置，冲突内容恢复前确认。</span>
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
              <strong>本地自动备份</strong>
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
                min="1"
                step="1"
              />
            </label>
            <label class="settings-view__field">
              <span>最多保留（份）</span>
              <input
                v-model.number="draft.localBackup.maxCount"
                type="number"
                min="1"
                step="1"
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
            <button type="button" :disabled="pending" @click="submitSettings">
              <Save :size="16" />
              保存配置
            </button>
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
                <strong>{{ backup.fileName }}</strong>
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

        <section class="settings-view__cloud">
          <div class="settings-view__cloud-header">
            <div>
              <strong>坚果云同步</strong>
              <span>通过坚果云 WebDAV 同步加密备份文件。</span>
            </div>
            <span class="settings-view__cloud-time">
              上次同步：{{ formatCloudSyncTime(draft.cloudSync.lastUpdatedAt) }}
            </span>
          </div>

          <div class="settings-view__cloud-grid">
            <label class="settings-view__field">
              <span>WebDAV 地址</span>
              <input v-model.trim="draft.cloudSync.webdavUrl" type="text" />
            </label>
            <label class="settings-view__field">
              <span>备份文件名</span>
              <input v-model.trim="draft.cloudSync.fileName" type="text" />
            </label>
            <label class="settings-view__field">
              <span>坚果云账号</span>
              <input v-model.trim="draft.cloudSync.username" type="text" />
            </label>
            <label class="settings-view__field">
              <span>应用密码</span>
              <el-input
                v-model="draft.cloudSync.password"
                type="password"
                show-password
              />
            </label>
          </div>

          <div class="settings-view__cloud-actions">
            <button type="button" :disabled="pending" @click="submitSettings">
              <Save :size="16" />
              保存配置
            </button>
            <button type="button" @click="pushCloudData">
              <UploadCloud :size="16" />
              推送到坚果云
            </button>
            <button type="button" @click="pullCloudData">
              <DownloadCloud :size="16" />
              从坚果云恢复
            </button>
          </div>
        </section>
      </section>

      <section v-else class="settings-view__panel">
        <div class="settings-view__panel-header">
          <div>
            <h2>系统设置</h2>
            <span>控制桌面端窗口关闭按钮的默认行为。</span>
          </div>
        </div>

        <div class="settings-view__system-list">
          <article class="settings-view__system-card">
            <div class="settings-view__data-copy">
              <strong>关闭按钮行为</strong>
              <span
                >点击窗口关闭按钮时，选择询问、最小化到托盘或直接关闭软件。</span
              >
            </div>
            <div class="settings-view__choice-group">
              <label
                v-for="item in closeActionItems"
                :key="item.value"
                :class="[
                  'settings-view__choice',
                  {
                    'settings-view__choice--active':
                      draft.system.closeAction === item.value
                  }
                ]"
              >
                <input
                  v-model="draft.system.closeAction"
                  type="radio"
                  name="close-action"
                  :value="item.value"
                />
                <span>{{ item.label }}</span>
              </label>
            </div>
          </article>
          <article class="settings-view__system-card">
            <div class="settings-view__data-copy">
              <strong>悬浮快速切换窗</strong>
              <span>开启后，主界面最小化时显示 Provider 快速切换悬浮窗。</span>
            </div>
            <div class="settings-view__choice-group">
              <label
                :class="[
                  'settings-view__choice',
                  {
                    'settings-view__choice--active':
                      draft.system.quickSwitchVisible
                  }
                ]"
              >
                <input
                  v-model="draft.system.quickSwitchVisible"
                  type="radio"
                  name="quick-switch-visible"
                  :value="true"
                />
                <span>显示</span>
              </label>
              <label
                :class="[
                  'settings-view__choice',
                  {
                    'settings-view__choice--active':
                      !draft.system.quickSwitchVisible
                  }
                ]"
              >
                <input
                  v-model="draft.system.quickSwitchVisible"
                  type="radio"
                  name="quick-switch-visible"
                  :value="false"
                />
                <span>隐藏</span>
              </label>
            </div>
          </article>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup>
import { computed, reactive, ref, watch } from "vue"
import {
  Download,
  DownloadCloud,
  FolderOpen,
  RefreshCw,
  RotateCcw,
  Save,
  Settings,
  Upload,
  UploadCloud
} from "lucide-vue-next"

const props = defineProps({
  appSettings: {
    type: Object,
    required: true
  },
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
  }
})

const emit = defineEmits([
  "save",
  "check-update",
  "open-path",
  "export-data",
  "restore-data",
  "local-backup-now",
  "local-backups-refresh",
  "local-backup-restore",
  "push-cloud-data",
  "pull-cloud-data"
])

const activeTab = ref("directories")

const draft = reactive({
  dataPath: "",
  cliConfigPaths: {
    claude: "",
    codex: "",
    gemini: ""
  },
  cloudSync: {
    provider: "jianguoyun",
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
  system: {
    closeAction: "ask",
    quickSwitchVisible: true
  }
})

const cliNames = {
  claude: "Claude",
  codex: "Codex",
  gemini: "Gemini"
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
  }
]

const closeActionItems = [
  {
    value: "ask",
    label: "每次询问"
  },
  {
    value: "minimize",
    label: "最小化到托盘"
  },
  {
    value: "quit",
    label: "直接关闭"
  }
]

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
  draft.dataPath = props.appSettings.dataPath || ""
  draft.cliConfigPaths.claude = props.appSettings.cliConfigPaths?.claude || ""
  draft.cliConfigPaths.codex = props.appSettings.cliConfigPaths?.codex || ""
  draft.cliConfigPaths.gemini = props.appSettings.cliConfigPaths?.gemini || ""
  draft.cloudSync.provider = "jianguoyun"
  draft.cloudSync.webdavUrl =
    props.appSettings.cloudSync?.webdavUrl ||
    "https://dav.jianguoyun.com/dav/AI-Manager"
  draft.cloudSync.username = props.appSettings.cloudSync?.username || ""
  draft.cloudSync.password = props.appSettings.cloudSync?.password || ""
  draft.cloudSync.fileName =
    props.appSettings.cloudSync?.fileName || "ai-manager.aimbackup"
  draft.cloudSync.lastUpdatedAt = Number(
    props.appSettings.cloudSync?.lastUpdatedAt || 0
  )
  draft.localBackup.enabled = props.appSettings.localBackup?.enabled !== false
  draft.localBackup.intervalMinutes = Number(
    props.appSettings.localBackup?.intervalMinutes || 60
  )
  draft.localBackup.maxCount = Number(
    props.appSettings.localBackup?.maxCount || 20
  )
  draft.localBackup.lastBackupAt = Number(
    props.appSettings.localBackup?.lastBackupAt || 0
  )
  draft.system.closeAction = props.appSettings.system?.closeAction || "ask"
  draft.system.quickSwitchVisible =
    props.appSettings.system?.quickSwitchVisible !== false
}

async function selectDirectory(key, currentPath) {
  const selectedPath = await window.aiManager.selectDirectory({
    title: "选择目录",
    defaultPath: currentPath || props.appSettings.dataPath
  })

  if (!selectedPath) {
    return
  }

  if (key === "dataPath") {
    draft.dataPath = selectedPath
    return
  }

  draft.cliConfigPaths[key] = selectedPath
}

function resetCliPath(key) {
  draft.cliConfigPaths[key] =
    props.appSettings.defaultCliConfigPaths?.[key] || ""
}

function submitSettings() {
  emit("save", {
    dataPath: draft.dataPath,
    cliConfigPaths: {
      claude: draft.cliConfigPaths.claude,
      codex: draft.cliConfigPaths.codex,
      gemini: draft.cliConfigPaths.gemini
    },
    cloudSync: {
      provider: draft.cloudSync.provider,
      webdavUrl: draft.cloudSync.webdavUrl,
      username: draft.cloudSync.username,
      password: draft.cloudSync.password,
      fileName: draft.cloudSync.fileName,
      lastUpdatedAt: draft.cloudSync.lastUpdatedAt
    },
    localBackup: {
      enabled: draft.localBackup.enabled,
      intervalMinutes: Number(draft.localBackup.intervalMinutes || 60),
      maxCount: Number(draft.localBackup.maxCount || 20),
      lastBackupAt: draft.localBackup.lastBackupAt
    },
    system: {
      closeAction: draft.system.closeAction,
      quickSwitchVisible: draft.system.quickSwitchVisible
    }
  })
}

function emitCloudSync(eventName) {
  if (
    !draft.cloudSync.webdavUrl ||
    !draft.cloudSync.username ||
    !draft.cloudSync.password ||
    !draft.cloudSync.fileName
  ) {
    window.alert("请先填写坚果云 WebDAV 地址、账号、应用密码和文件名")
    return
  }

  emit(eventName, {
    provider: draft.cloudSync.provider,
    webdavUrl: draft.cloudSync.webdavUrl,
    username: draft.cloudSync.username,
    password: draft.cloudSync.password,
    fileName: draft.cloudSync.fileName,
    lastUpdatedAt: draft.cloudSync.lastUpdatedAt
  })
}

function pushCloudData() {
  emitCloudSync("push-cloud-data")
}

function pullCloudData() {
  emitCloudSync("pull-cloud-data")
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
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;
  color: var(--color-text);
  font-size: 0.86rem;

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
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  &__header h1 {
    margin: 0;
    font-size: 1.35rem;
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
      font-size: 12px;
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
    font-size: 0.78rem;
    font-weight: 600;
  }

  &__tab--active {
    background: #edf1f7;
    color: var(--color-primary);
  }

  &__save,
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
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 600;
  }

  &__save {
    padding: 0 13px;
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  &__check-update {
    padding: 0 12px;
    .text {
      font-size: 14px;
      line-height: 14px;
    }
  }

  &__save:disabled {
    cursor: not-allowed;
    opacity: 0.56;
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
    font-size: 0.98rem;
    line-height: 1.35;
  }

  &__panel-header span,
  &__hint {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.5;
    word-break: break-all;
  }

  &__field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  &__field > span {
    color: var(--color-text-muted);
    font-size: 0.7rem;
    font-weight: 700;
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
    background: #ffffff;
    color: var(--color-text);
    font-size: 0.8rem;
  }

  &__field input {
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-text);
    font-size: 0.8rem;
  }

  &__input-row button {
    min-width: 32px;
    padding: 0 10px;
  }

  &__warning {
    margin: 0;
    padding: 9px 11px;
    border: 1px solid #eadba8;
    border-radius: 8px;
    background: #fffaf0;
    color: #8a6514;
    font-size: 0.78rem;
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

  &__system-list {
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

  &__cli-title strong {
    font-size: 0.9rem;
    line-height: 1.35;
  }

  &__cli-title small {
    color: var(--color-text-soft);
    font-size: 0.76rem;
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
    background: #e8f7ef;
    color: #138449;
    font-size: 0.7rem;
    font-weight: 700;
  }

  &__cli-status--offline {
    background: #f3f4f6;
    color: #667085;
  }

  &__cli-meta {
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: var(--color-text-soft);
    font-size: 0.76rem;
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

  &__system-card {
    display: flex;
    align-items: flex-start;
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
    background: #ffffff;
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
    font-size: 0.78rem;
    font-weight: 600;
  }

  &__choice input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  &__choice--active {
    background: #edf1f7;
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

  &__cloud-header strong {
    font-size: 0.9rem;
    line-height: 1.35;
  }

  &__cloud-header span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }

  &__cloud-time {
    flex: none;
    padding-top: 1px;
    color: var(--color-text-soft);
    font-size: 0.76rem;
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
    background: #ffffff;
  }

  &__backup-directory span {
    min-width: 0;
    color: var(--color-text-muted);
    font-size: 0.78rem;
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
  }

  &__backup-empty {
    padding: 10px;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 0.78rem;
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
    background: #ffffff;
  }

  &__backup-item div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }

  &__backup-item strong {
    overflow: hidden;
    font-size: 0.82rem;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__backup-item span {
    color: var(--color-text-muted);
    font-size: 0.76rem;
    line-height: 1.45;
  }

  &__data-copy {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  &__data-copy strong {
    font-size: 0.9rem;
    line-height: 1.35;
  }

  &__data-copy span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }
}
</style>

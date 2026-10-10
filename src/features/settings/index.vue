<template>
  <section class="settings-view">
    <header class="settings-view__header">
      <div>
        <p>Application Settings</p>
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
            <span>支持导出至本地备份文件，也可从本地备份文件兼容恢复。</span>
          </div>
        </div>

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
              />
            </label>
            <label class="settings-view__field">
              <span>备份文件名</span>
              <input
                v-model.trim="draft[syncItem.settingKey].fileName"
                type="text"
              />
            </label>
            <label class="settings-view__field">
              <span>{{ syncItem.accountLabel }}</span>
              <input
                v-model.trim="draft[syncItem.settingKey].username"
                type="text"
              />
            </label>
            <label class="settings-view__field">
              <span>应用密码</span>
              <el-input
                v-model="draft[syncItem.settingKey].password"
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

      <section v-else-if="activeTab === 'agents'" class="settings-agent-panel">
        <template v-if="!activeAgent">
          <div class="settings-view__panel-header">
            <div>
              <h2>功能扩展</h2>
              <span>管理应用的扩展功能，点击查看配置和使用记录。</span>
            </div>
          </div>
          <div class="settings-agent-list">
            <article
              class="settings-agent-card"
              tabindex="0"
              @click="activeAgent = 'translation'"
              @keydown.enter="activeAgent = 'translation'"
            >
              <div class="settings-agent-icon"><Languages :size="20" /></div>
              <div class="settings-agent-copy">
                <span data-emphasis>划词翻译</span>
                <span>选中文本后右键翻译，支持查看翻译历史和消耗记录。</span>
              </div>
              <button
                class="settings-agent-toggle"
                :class="{
                  'settings-agent-toggle-active':
                    draft.agents.translation.enabled
                }"
                type="button"
                @click.stop="
                  draft.agents.translation.enabled =
                    !draft.agents.translation.enabled
                "
              >
                {{ draft.agents.translation.enabled ? "已启用" : "已禁用" }}
              </button>
              <ChevronRight :size="18" />
            </article>
          </div>
        </template>
        <TranslationAgentSettings
          v-else
          v-model="draft.agents.translation"
          :providers="providers"
          :codex-accounts="codexAccounts"
          :runtime-models="runtimeModels"
          @back="activeAgent = ''"
        />
      </section>

      <section
        v-else-if="activeTab === 'desktop'"
        class="settings-view__panel settings-desktop-panel"
      >
        <div class="settings-view__panel-header">
          <div>
            <h2>桌面管理</h2>
            <span>用动画桌宠陪伴桌面，拖动移动，悬浮展开工具。</span>
          </div>
        </div>
        <div class="settings-view__system-list">
          <article class="settings-view__system-card">
            <div class="settings-view__data-copy">
              <span data-emphasis>启用桌宠</span>
              <span>开启后替换悬浮窗中心内容；关闭后恢复原来的信息卡片。</span>
            </div>
            <div class="settings-view__choice-group">
              <label
                v-for="item in desktopModeItems"
                :key="String(item.value)"
                class="settings-view__choice"
                :class="{
                  'settings-view__choice--active':
                    draft.desktop.enabled === item.value
                }"
              >
                <input
                  v-model="draft.desktop.enabled"
                  type="radio"
                  name="desktop-pet-enabled"
                  :value="item.value"
                  :disabled="item.value && !desktopPets.length"
                />
                <span>{{ item.label }}</span>
              </label>
            </div>
          </article>
        </div>
        <div v-if="draft.desktop.enabled" class="settings-desktop-selection">
          <div class="settings-desktop-copy">
            <span data-emphasis>选择桌宠</span>
            <span>自动轮换动作，保存后生效，下次启用会记住你的选择。</span>
          </div>
          <div class="settings-desktop-pets">
            <label
              v-for="pet in desktopPets"
              :key="pet.id"
              class="settings-desktop-pet"
              :class="{
                'settings-desktop-pet-active': draft.desktop.petId === pet.id
              }"
            >
              <span class="settings-desktop-preview"
                ><DesktopPet :pet="pet"
              /></span>
              <span class="settings-desktop-pet-choice">
                <input
                  v-model="draft.desktop.petId"
                  type="radio"
                  name="desktop-pet"
                  :value="pet.id"
                />
                <span>{{ pet.name }}</span>
              </span>
            </label>
          </div>
          <section
            v-if="selectedDesktopPet"
            class="settings-desktop-animations"
          >
            <div class="settings-desktop-copy">
              <span data-emphasis>{{ selectedDesktopPet.name }}的全部动画</span>
              <span
                >{{
                  selectedDesktopPet.animations.length
                }}
                个动作，每个动作独立循环播放。</span
              >
            </div>
            <div class="settings-desktop-animation-list">
              <article
                v-for="animation in selectedDesktopPet.animations"
                :key="`${selectedDesktopPet.id}:${animation.id}`"
                class="settings-desktop-animation"
              >
                <div class="settings-desktop-animation-preview">
                  <DesktopPet
                    :pet="selectedDesktopPet"
                    :action="animation.id"
                  />
                </div>
                <span class="settings-desktop-animation-name">{{
                  animation.name
                }}</span>
              </article>
            </div>
          </section>
          <p
            v-if="!draft.system.quickSwitchVisible"
            class="settings-desktop-hint"
          >
            悬浮窗当前已隐藏，可在“系统设置”中开启显示。
          </p>
        </div>
        <p v-if="!desktopPets.length" class="settings-desktop-hint">
          暂无可用桌宠。
        </p>
      </section>

      <section v-else class="settings-view__panel">
        <div class="settings-view__panel-header">
          <div>
            <h2>系统设置</h2>
            <span>控制桌面端启动、窗口关闭和悬浮窗显示行为。</span>
          </div>
        </div>

        <div class="settings-view__system-list">
          <article class="settings-view__system-card">
            <div class="settings-view__data-copy">
              <span data-emphasis>开机自动启动</span>
              <span>开启后登录 Windows 时自动启动软件，可随时关闭。</span>
            </div>
            <div class="settings-view__choice-group">
              <label
                :class="[
                  'settings-view__choice',
                  {
                    'settings-view__choice--active':
                      draft.system.autoLaunchEnabled
                  }
                ]"
              >
                <input
                  v-model="draft.system.autoLaunchEnabled"
                  type="radio"
                  name="auto-launch-enabled"
                  :value="true"
                />
                <span>开启</span>
              </label>
              <label
                :class="[
                  'settings-view__choice',
                  {
                    'settings-view__choice--active':
                      !draft.system.autoLaunchEnabled
                  }
                ]"
              >
                <input
                  v-model="draft.system.autoLaunchEnabled"
                  type="radio"
                  name="auto-launch-enabled"
                  :value="false"
                />
                <span>关闭</span>
              </label>
            </div>
          </article>
          <article class="settings-view__system-card">
            <div class="settings-view__data-copy">
              <span data-emphasis>关闭按钮行为</span>
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
              <span data-emphasis>悬浮快速切换窗</span>
              <span
                >主界面最小化时显示中心节点，鼠标悬浮后展开四周的工具入口。</span
              >
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
          <article
            v-if="draft.system.quickSwitchVisible"
            class="settings-view__system-card settings-floating-tools-card"
          >
            <div class="settings-view__data-copy">
              <span data-emphasis>悬浮窗工具</span>
              <span
                >选择悬浮时显示的工具，默认全部显示。取消全部后只保留中心节点。</span
              >
            </div>
            <div class="settings-floating-tools">
              <label
                v-for="tool in floatingToolItems"
                :key="tool.id"
                class="settings-floating-tool"
              >
                <input
                  v-model="draft.system.quickSwitchTools[tool.id]"
                  type="checkbox"
                />
                <component :is="tool.icon" :size="16" />
                <span class="settings-floating-tool-label">{{
                  tool.label
                }}</span>
              </label>
            </div>
          </article>
          <article class="settings-view__system-card">
            <div class="settings-view__data-copy">
              <span data-emphasis>关闭程序</span>
              <span>立即退出桌面端，不最小化到托盘。</span>
            </div>
            <button
              class="settings-view__danger-button"
              type="button"
              @click="$emit('quit-app')"
            >
              <Power :size="16" />
              关闭程序
            </button>
          </article>
          <article class="settings-view__system-card">
            <div class="settings-view__data-copy">
              <span data-emphasis>无痕卸载</span>
              <span>删除所有本地用户数据，并自动启动静默卸载。</span>
            </div>
            <button
              class="settings-view__danger-button"
              type="button"
              @click="$emit('uninstall-without-trace')"
            >
              <Trash2 :size="16" />
              无痕卸载
            </button>
          </article>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup>
import { computed, reactive, ref, watch } from "vue"
import {
  ChevronRight,
  Download,
  DownloadCloud,
  Eye,
  FolderOpen,
  Languages,
  Monitor,
  Power,
  Puzzle,
  RefreshCw,
  RotateCcw,
  Save,
  Settings,
  Trash2,
  Upload,
  UploadCloud
} from "lucide-vue-next"
import { systemApi } from "@/api"
import { getToolItems } from "@/features/tools/constant"
import { desktopPets, getDesktopPet } from "@/constants/desktop-pets"
import DesktopPet from "@/components/DesktopPet.vue"
import TranslationAgentSettings from "./components/TranslationAgentSettings.vue"

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
  "quit-app",
  "push-cloud-data",
  "inspect-cloud-data",
  "pull-cloud-data",
  "uninstall-without-trace"
])

const activeTab = ref("directories")
const activeAgent = ref("")
const floatingToolItems = computed(() =>
  getToolItems({ cliTargets: props.cliTargets })
)

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
      targetLanguage: "简体中文"
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
  system: {
    closeAction: "ask",
    quickSwitchVisible: true,
    quickSwitchTools: {},
    autoLaunchEnabled: false
  }
})

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

const desktopModeItems = [
  { value: true, label: "启用" },
  { value: false, label: "关闭" }
]

// 当前选择直接驱动全部动作预览，不影响已保存的桌宠配置。
const selectedDesktopPet = computed(() => getDesktopPet(draft.desktop.petId))

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
  // 关闭只停用桌宠，不清除上次选择；旧素材不存在时选择第一只。
  draft.desktop.enabled = Boolean(props.appSettings.desktop?.enabled)
  draft.desktop.petId =
    getDesktopPet(props.appSettings.desktop?.petId)?.id || ""
  // 保存后的全局配置驱动划词入口；未保存的编辑仅留在设置草稿中。
  draft.agents.translation = {
    enabled: false,
    targetId: "",
    model: "",
    targetLanguage: "简体中文",
    ...props.appSettings.agents?.translation
  }
  draft.dataPath = props.appSettings.dataPath || ""
  draft.cliConfigPaths.claude = props.appSettings.cliConfigPaths?.claude || ""
  draft.cliConfigPaths.codex = props.appSettings.cliConfigPaths?.codex || ""
  // 当前版本暂不启用 Gemini。
  // draft.cliConfigPaths.gemini = props.appSettings.cliConfigPaths?.gemini || ""
  for (const syncItem of cloudSyncItems) {
    const source = props.appSettings[syncItem.settingKey] || {}
    const target = draft[syncItem.settingKey]

    target.provider = syncItem.provider
    target.webdavUrl = source.webdavUrl || syncItem.defaultUrl
    target.username = source.username || ""
    target.password = source.password || ""
    target.fileName = source.fileName || "ai-manager.aimbackup"
    target.lastUpdatedAt = Number(source.lastUpdatedAt || 0)
  }
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
  // 缺少配置的工具默认显示，旧配置和以后新增的工具都无需迁移。
  draft.system.quickSwitchTools = {
    ...props.appSettings.system?.quickSwitchTools,
    ...Object.fromEntries(
      floatingToolItems.value.map((tool) => [
        tool.id,
        props.appSettings.system?.quickSwitchTools?.[tool.id] !== false
      ])
    )
  }
  draft.system.autoLaunchEnabled = Boolean(
    props.appSettings.system?.autoLaunchEnabled
  )
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
    desktop: { ...draft.desktop },
    agents: { translation: { ...draft.agents.translation } },
    dataPath: draft.dataPath,
    cliConfigPaths: {
      claude: draft.cliConfigPaths.claude,
      codex: draft.cliConfigPaths.codex
      // 当前版本暂不启用 Gemini。
      // gemini: draft.cliConfigPaths.gemini
    },
    cloudSync: {
      provider: draft.cloudSync.provider,
      webdavUrl: draft.cloudSync.webdavUrl,
      username: draft.cloudSync.username,
      password: draft.cloudSync.password,
      fileName: draft.cloudSync.fileName,
      lastUpdatedAt: draft.cloudSync.lastUpdatedAt
    },
    koofrSync: {
      provider: draft.koofrSync.provider,
      webdavUrl: draft.koofrSync.webdavUrl,
      username: draft.koofrSync.username,
      password: draft.koofrSync.password,
      fileName: draft.koofrSync.fileName,
      lastUpdatedAt: draft.koofrSync.lastUpdatedAt
    },
    localBackup: {
      enabled: draft.localBackup.enabled,
      intervalMinutes: Number(draft.localBackup.intervalMinutes || 60),
      maxCount: Number(draft.localBackup.maxCount || 20),
      lastBackupAt: draft.localBackup.lastBackupAt
    },
    system: {
      closeAction: draft.system.closeAction,
      quickSwitchVisible: draft.system.quickSwitchVisible,
      quickSwitchTools: { ...draft.system.quickSwitchTools },
      autoLaunchEnabled: draft.system.autoLaunchEnabled
    }
  })
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

watch(floatingToolItems, (tools) => {
  for (const tool of tools) {
    if (draft.system.quickSwitchTools[tool.id] === undefined)
      draft.system.quickSwitchTools[tool.id] = true
  }
})
</script>

<style scoped lang="less">
.settings-view {
  .settings-desktop-panel {
    .settings-desktop-selection {
      display: flex;
      flex-direction: column;
      gap: 16px;
      margin-top: 20px;
      .settings-desktop-copy {
        display: flex;
        flex-direction: column;
        gap: 6px;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
      }
      .settings-desktop-pets {
        display: flex;
        flex-wrap: wrap;
        gap: 14px;
        .settings-desktop-pet {
          display: flex;
          width: 168px;
          flex-direction: column;
          align-items: center;
          gap: 12px;
          padding: 14px;
          border: 1px solid var(--color-line);
          border-radius: 14px;
          background: var(--color-panel-soft);
          cursor: pointer;
          transition:
            border-color 160ms,
            background-color 160ms;
          &.settings-desktop-pet-active {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
          }
          &:focus-within {
            outline: 1px solid var(--color-primary);
            outline-offset: 3px;
          }
          .settings-desktop-preview {
            display: flex;
            width: 112px;
            height: 112px;
          }
          .settings-desktop-pet-choice {
            display: flex;
            align-items: center;
            gap: 8px;
            color: var(--color-text);
          }
        }
      }
      .settings-desktop-animations {
        display: flex;
        flex-direction: column;
        gap: 14px;
        .settings-desktop-animation-list {
          display: flex;
          flex-wrap: wrap;
          gap: 12px;
          .settings-desktop-animation {
            display: flex;
            width: 136px;
            flex-direction: column;
            align-items: center;
            gap: 8px;
            padding: 10px;
            border: 1px solid var(--color-line);
            border-radius: 10px;
            background: var(--color-panel-soft);
            .settings-desktop-animation-preview {
              width: 112px;
              height: 112px;
            }
            .settings-desktop-animation-name {
              color: var(--color-text);
              font-size: var(--font-size-sm);
              line-height: 20px;
            }
          }
        }
      }
    }
    .settings-desktop-hint {
      margin: 14px 0 0;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
    }
  }
  .settings-view__panel {
    .settings-view__system-list {
      .settings-floating-tools-card {
        align-items: stretch;
        flex-direction: column;
        .settings-floating-tools {
          display: flex;
          flex-wrap: wrap;
          gap: 10px;
          .settings-floating-tool {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 9px 12px;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            background: var(--color-panel);
            color: var(--color-text-secondary);
            cursor: pointer;
            &:has(:checked) {
              border-color: var(--color-primary);
              background: var(--color-panel-soft);
              color: var(--color-primary);
            }
            .settings-floating-tool-label {
              font-size: var(--font-size-sm);
            }
          }
        }
      }
    }
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
    background: var(--color-panel-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: var(--font-size-sm);
  }

  &__save {
    padding: 0 13px;
    border-color: var(--color-primary);
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  &__check-update {
    padding: 0 12px;
    .text {
      font-size: var(--font-size-sm);
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

  &__danger-button {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--color-danger);
    border-radius: 7px;
    background: var(--color-danger-soft);
    color: var(--color-danger);
    cursor: pointer;
    font-size: var(--font-size-sm);
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
    overflow: auto;
    .settings-agent-list {
      display: flex;
      flex-direction: column;
      gap: 10px;
      .settings-agent-card {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 16px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        cursor: pointer;
        &:hover,
        &:focus-visible {
          border-color: var(--color-line-strong);
          outline: none;
        }
        .settings-agent-icon {
          display: grid;
          width: 38px;
          height: 38px;
          flex: none;
          place-items: center;
          border-radius: 8px;
          background: var(--color-panel-soft);
          color: var(--color-primary);
        }
        .settings-agent-copy {
          display: flex;
          min-width: 0;
          flex: 1;
          flex-direction: column;
          gap: 4px;
          span:not([data-emphasis]) {
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
          }
        }
        .settings-agent-toggle {
          flex: none;
          padding: 6px 10px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          cursor: pointer;
          &.settings-agent-toggle-active {
            border-color: var(--color-primary);
            background: var(--color-panel-soft);
            color: var(--color-primary);
          }
        }
      }
    }
  }
}
</style>

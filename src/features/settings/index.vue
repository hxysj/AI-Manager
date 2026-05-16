<template>
  <section class="settings-view">
    <header class="settings-view__header">
      <div>
        <p>Application Settings</p>
        <h1>设置</h1>
      </div>
      <button
        class="settings-view__save"
        type="button"
        :disabled="pending"
        @click="submitSettings"
      >
        <Save :size="16" />
        {{ pending ? '保存中...' : '保存设置' }}
      </button>
    </header>

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
          <button type="button" @click="draft.dataPath = appSettings.defaultDataPath">
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
              {{ item.installed ? '已检测' : '未检测' }}
            </span>
          </div>

          <label class="settings-view__field">
            <span>{{ item.name }} 配置目录</span>
            <div class="settings-view__input-row">
              <input v-model.trim="draft.cliConfigPaths[item.id]" type="text" />
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
            <span>配置目录：{{ item.detectedPath || item.defaultPath }}</span>
            <span>Skill 目录：{{ item.skillsPath || '未检测' }}</span>
            <span>版本：{{ item.version || '未检测' }}</span>
          </div>
        </article>
      </div>
    </section>
  </section>
</template>

<script setup>
import { computed, reactive, watch } from 'vue'
import { FolderOpen, RotateCcw, Save } from 'lucide-vue-next'

const props = defineProps({
  appSettings: {
    type: Object,
    required: true
  },
  cliTargets: {
    type: Array,
    required: true
  },
  pending: {
    type: Boolean,
    required: true
  }
})

const emit = defineEmits(['save', 'open-path'])

const draft = reactive({
  dataPath: '',
  cliConfigPaths: {
    claude: '',
    codex: '',
    gemini: ''
  }
})

const cliNames = {
  claude: 'Claude',
  codex: 'Codex',
  gemini: 'Gemini'
}

const cliItems = computed(() => {
  return Object.entries(cliNames).map(([id, name]) => {
    const detected = props.cliTargets.find(item => item.id === id)

    return {
      id,
      name,
      installed: Boolean(detected?.installed),
      statusText: detected?.installed
        ? '配置目录或二进制已找到'
        : '未发现配置目录与可执行文件',
      detectedPath: detected?.configPath || '',
      skillsPath: detected?.skillsPath || '',
      version: detected?.version || '',
      defaultPath: props.appSettings.defaultCliConfigPaths?.[id] || ''
    }
  })
})

function syncDraft() {
  draft.dataPath = props.appSettings.dataPath || ''
  draft.cliConfigPaths.claude = props.appSettings.cliConfigPaths?.claude || ''
  draft.cliConfigPaths.codex = props.appSettings.cliConfigPaths?.codex || ''
  draft.cliConfigPaths.gemini = props.appSettings.cliConfigPaths?.gemini || ''
}

async function selectDirectory(key, currentPath) {
  const selectedPath = await window.aiManager.selectDirectory({
    title: '选择目录',
    defaultPath: currentPath || props.appSettings.dataPath
  })

  if (!selectedPath) {
    return
  }

  if (key === 'dataPath') {
    draft.dataPath = selectedPath
    return
  }

  draft.cliConfigPaths[key] = selectedPath
}

function resetCliPath(key) {
  draft.cliConfigPaths[key] =
    props.appSettings.defaultCliConfigPaths?.[key] || ''
}

function submitSettings() {
  emit('save', {
    dataPath: draft.dataPath,
    cliConfigPaths: {
      claude: draft.cliConfigPaths.claude,
      codex: draft.cliConfigPaths.codex,
      gemini: draft.cliConfigPaths.gemini
    }
  })
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
  flex-direction: column;
  gap: 16px;
}

.settings-view__header,
.settings-view__panel {
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
}

.settings-view__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  padding: 22px;
}

.settings-view__header p {
  margin: 0 0 8px;
  color: var(--color-text-soft);
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
}

.settings-view__header h1 {
  margin: 0;
  font-size: 2rem;
}

.settings-view__save,
.settings-view__panel-header button,
.settings-view__input-row button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  height: 38px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #fbfcfd;
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;
}

.settings-view__save {
  padding: 0 16px;
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #ffffff;
}

.settings-view__save:disabled {
  cursor: not-allowed;
  opacity: 0.56;
}

.settings-view__panel {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 22px;
}

.settings-view__panel-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
}

.settings-view__panel-header h2 {
  margin: 0 0 8px;
  font-size: 1.18rem;
}

.settings-view__panel-header span,
.settings-view__hint {
  color: var(--color-text-muted);
  line-height: 1.6;
  word-break: break-all;
}

.settings-view__field {
  display: flex;
  flex-direction: column;
  gap: 9px;
}

.settings-view__field > span {
  color: var(--color-text-muted);
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

.settings-view__input-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto auto;
  gap: 8px;
}

.settings-view__input-row input {
  min-width: 0;
  height: 38px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #ffffff;
  color: var(--color-text);
}

.settings-view__input-row button {
  min-width: 38px;
  padding: 0 12px;
}

.settings-view__warning {
  margin: 0;
  padding: 12px 14px;
  border: 1px solid #eadba8;
  border-radius: 8px;
  background: #fffaf0;
  color: #8a6514;
}

.settings-view__hint {
  margin: 0;
}

.settings-view__cli-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.settings-view__cli-card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);
}

.settings-view__cli-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.settings-view__cli-title div {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.settings-view__cli-title strong {
  font-size: 1rem;
}

.settings-view__cli-title small {
  color: var(--color-text-soft);
  line-height: 1.5;
  text-align: right;
  word-break: break-all;
}

.settings-view__cli-title div small {
  text-align: left;
}

.settings-view__cli-status {
  flex: 0 0 auto;
  padding: 5px 10px;
  border-radius: 999px;
  background: #e8f7ef;
  color: #138449;
  font-size: 0.76rem;
  font-weight: 700;
}

.settings-view__cli-status--offline {
  background: #f3f4f6;
  color: #667085;
}

.settings-view__cli-meta {
  display: flex;
  flex-direction: column;
  gap: 6px;
  color: var(--color-text-soft);
  font-size: 0.8rem;
  line-height: 1.5;
  word-break: break-all;
}
</style>

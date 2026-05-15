<template>
  <section class="providers-view">
    <template v-if="viewMode === 'list'">
      <header class="providers-view__toolbar">
        <div class="providers-view__brand">
          <strong>CC Switch</strong>
          <button class="providers-view__icon-button" type="button">
            <Settings :size="16" />
          </button>
          <label class="providers-view__toggle">
            <Radio :size="14" />
            <input v-model="runtimeEnabled" type="checkbox" />
            <span></span>
          </label>
        </div>

        <div class="providers-view__cli-tabs">
          <button
            v-for="cli in cliTargets"
            :key="cli.id"
            :class="[
              'providers-view__cli-tab',
              { 'providers-view__cli-tab--active': activeCli === cli.id }
            ]"
            type="button"
            @click="selectCli(cli.id)"
          >
            <AiIcon
              v-if="cli.icon"
              class="providers-view__cli-icon"
              :name="cli.icon"
              :alt="`${cli.name} 图标`"
            />
            {{ cli.name }}
          </button>
        </div>

        <div class="providers-view__tools">
          <button class="providers-view__icon-button" type="button">
            <Wrench :size="16" />
          </button>
          <button class="providers-view__icon-button" type="button">
            <PanelRight :size="16" />
          </button>
          <button class="providers-view__icon-button" type="button">
            <History :size="16" />
          </button>
          <button class="providers-view__icon-button" type="button">
            <Paperclip :size="16" />
          </button>
          <button
            class="providers-view__add"
            type="button"
            @click="createProvider"
          >
            <Plus :size="22" />
          </button>
        </div>
      </header>

      <section class="providers-view__list-panel">
        <article
          v-for="provider in scopedProviders"
          :key="provider.id"
          :class="[
            'providers-view__provider-card',
            {
              'providers-view__provider-card--active':
                profileMap[activeCli]?.providerId === provider.id
            }
          ]"
          @click="selectProvider(provider)"
        >
          <GripVertical class="providers-view__drag" :size="16" />
          <span class="providers-view__avatar">{{ provider.name.slice(0, 1) }}</span>
          <div class="providers-view__provider-main">
            <strong>{{ provider.name }}</strong>
            <span>{{ provider.baseUrl || '未配置官网地址' }}</span>
          </div>
          <div class="providers-view__provider-actions">
            <button
              v-if="profileMap[activeCli]?.providerId === provider.id"
              class="providers-view__using"
              type="button"
            >
              使用中
            </button>
            <button
              v-else
              class="providers-view__enable"
              type="button"
              @click.stop="enableProvider(provider)"
            >
              <Play :size="15" />
              启用
            </button>
            <button
              class="providers-view__icon-button"
              type="button"
              @click.stop="editProvider(provider)"
            >
              <SquarePen :size="16" />
            </button>
            <button class="providers-view__icon-button" type="button">
              <Copy :size="16" />
            </button>
            <button class="providers-view__icon-button" type="button">
              <ChartColumn :size="16" />
            </button>
            <button class="providers-view__icon-button" type="button">
              <Terminal :size="16" />
            </button>
            <button
              class="providers-view__icon-button providers-view__icon-button--danger"
              type="button"
              @click.stop="removeProvider(provider)"
            >
              <Trash2 :size="16" />
            </button>
          </div>
        </article>

        <div v-if="!scopedProviders.length" class="providers-view__empty">
          当前 CLI 还没有 Provider。
        </div>
      </section>
    </template>

    <template v-else>
      <header class="providers-view__edit-header">
        <button
          class="providers-view__back"
          type="button"
          @click="viewMode = 'list'"
        >
          <ArrowLeft :size="18" />
        </button>
        <h1>{{ draft.id ? '编辑供应商' : '新增供应商' }}</h1>
      </header>

      <section class="providers-view__edit-panel">
        <div class="providers-view__edit-avatar">
          {{ draft.name.slice(0, 1) || 'AI' }}
        </div>

        <div class="providers-view__form-grid">
          <label class="providers-view__field">
            <span>供应商名称</span>
            <input v-model.trim="draft.name" type="text" />
          </label>
          <label class="providers-view__field">
            <span>备注</span>
            <input v-model.trim="draft.note" type="text" />
          </label>
          <label class="providers-view__field providers-view__field--wide">
            <span>官网链接</span>
            <input v-model.trim="draft.website" type="text" />
          </label>
          <label class="providers-view__field providers-view__field--wide">
            <span>API Key</span>
            <input
              v-model.trim="draft.apiKey"
              type="password"
              :placeholder="selectedProvider?.hasApiKey ? '已保存，留空则保持不变' : ''"
            />
          </label>
          <label class="providers-view__field providers-view__field--wide">
            <span>请求地址</span>
            <input v-model.trim="draft.baseUrl" type="text" />
          </label>
        </div>

        <div class="providers-view__warning">
          填写兼容当前 CLI 的服务端点地址，不要以斜杠结尾
        </div>

        <details class="providers-view__advanced" open>
          <summary>高级选项</summary>
          <label class="providers-view__field">
            <span>API 格式</span>
            <select v-model="draft.type">
              <option v-for="item in providerTypes" :key="item" :value="item">
                {{ providerTypeLabelMap[item] || item }}
              </option>
            </select>
          </label>
          <label class="providers-view__field">
            <span>认证字段</span>
            <select v-model="draft.authField">
              <option value="ANTHROPIC_AUTH_TOKEN">ANTHROPIC_AUTH_TOKEN（默认）</option>
              <option value="OPENAI_API_KEY">OPENAI_API_KEY</option>
              <option value="GOOGLE_API_KEY">GOOGLE_API_KEY</option>
            </select>
          </label>
        </details>

        <section class="providers-view__models">
          <div class="providers-view__section-title">
            <div>
              <h2>模型映射</h2>
              <p>仅在需要将请求映射到不同模型名称时填写。</p>
            </div>
            <div class="providers-view__section-actions">
              <button type="button" @click="fillModelDrafts">一键设置</button>
              <button type="button">获取模型列表</button>
            </div>
          </div>

          <div class="providers-view__form-grid">
            <label class="providers-view__field">
              <span>主模型</span>
              <input v-model.trim="modelDrafts.main" type="text" />
            </label>
            <label class="providers-view__field">
              <span>Haiku 默认模型</span>
              <input v-model.trim="modelDrafts.haiku" type="text" />
            </label>
            <label class="providers-view__field">
              <span>Sonnet 默认模型</span>
              <input v-model.trim="modelDrafts.sonnet" type="text" />
            </label>
            <label class="providers-view__field">
              <span>Opus 默认模型</span>
              <input v-model.trim="modelDrafts.opus" type="text" />
            </label>
          </div>
        </section>

        <section class="providers-view__json">
          <div class="providers-view__json-title">
            <strong>配置 JSON</strong>
            <label>
              <input v-model="draft.writeCommonConfig" type="checkbox" />
              写入通用配置
            </label>
          </div>
          <div class="providers-view__check-row">
            <label><input v-model="draft.hideAiSignature" type="checkbox" />隐藏 AI 署名</label>
            <label><input v-model="draft.teammatesMode" type="checkbox" />Teammates 模式</label>
            <label><input v-model="draft.toolSearch" type="checkbox" />启用 Tool Search</label>
            <label><input v-model="draft.maxThinking" type="checkbox" />最大强度思考</label>
            <label><input v-model="draft.disableUpgrade" type="checkbox" />禁用自动升级</label>
          </div>
          <pre>{{ configPreview }}</pre>
        </section>

        <div class="providers-view__config-card">
          <FlaskConical :size="18" />
          <strong>模型测试配置</strong>
          <span>使用单独配置</span>
          <ChevronRight :size="18" />
        </div>

        <div class="providers-view__config-card">
          <Gauge :size="18" />
          <strong>计费配置</strong>
          <span>使用单独配置</span>
          <ChevronRight :size="18" />
        </div>
      </section>

      <footer class="providers-view__edit-footer">
        <button
          class="providers-view__primary"
          type="button"
          :disabled="pending"
          @click="submitProvider"
        >
          <Save :size="16" />
          保存
        </button>
      </footer>
    </template>
  </section>
</template>

<script setup>
import { computed, reactive, ref, watch } from 'vue'
import {
  ArrowLeft,
  ChartColumn,
  ChevronRight,
  Copy,
  FlaskConical,
  Gauge,
  GripVertical,
  History,
  PanelRight,
  Paperclip,
  Play,
  Plus,
  Radio,
  Save,
  Settings,
  SquarePen,
  Terminal,
  Trash2,
  Wrench
} from 'lucide-vue-next'
import AiIcon from '@/components/AiIcon.vue'

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  pending: {
    type: Boolean,
    required: true
  },
  providers: {
    type: Array,
    required: true
  },
  runtimeModels: {
    type: Array,
    required: true
  },
  runtimeProfiles: {
    type: Array,
    required: true
  }
})

const emit = defineEmits([
  'delete-provider',
  'save-model',
  'save-provider',
  'switch-runtime'
])

const providerTypes = [
  'anthropic',
  'openai',
  'gemini',
  'open' + 'router',
  'deep' + 'seek',
  'custom'
]

const providerTypeLabelMap = {
  anthropic: 'Anthropic Messages（原生）',
  openai: 'OpenAI Chat Completions（需开启路由）',
  gemini: 'Gemini Native generateContent（需开启路由）',
  custom: 'Custom'
}

const draft = reactive({
  id: '',
  cli: '',
  name: '',
  note: '',
  website: '',
  type: 'anthropic',
  baseUrl: '',
  proxy: '',
  apiKey: '',
  authField: 'ANTHROPIC_AUTH_TOKEN',
  enabled: true,
  writeCommonConfig: true,
  hideAiSignature: false,
  teammatesMode: true,
  toolSearch: false,
  maxThinking: true,
  disableUpgrade: false
})

const modelDrafts = reactive({
  main: '',
  haiku: '',
  sonnet: '',
  opus: ''
})

const activeCli = ref('')
const runtimeEnabled = ref(false)
const viewMode = ref('list')

const selectedProvider = computed(() => {
  return props.providers.find(item => item.id === draft.id) || null
})

const scopedProviders = computed(() => {
  return props.providers.filter(item => item.cli === activeCli.value)
})

const profileMap = computed(() => {
  return Object.fromEntries(props.runtimeProfiles.map(item => [item.cli, item]))
})

const configPreview = computed(() => {
  return JSON.stringify(
    {
      env: {
        [draft.authField]: draft.apiKey || '********',
        ANTHROPIC_BASE_URL: draft.baseUrl,
        ANTHROPIC_MODEL: modelDrafts.main,
        ANTHROPIC_DEFAULT_HAIKU_MODEL: modelDrafts.haiku,
        ANTHROPIC_DEFAULT_SONNET_MODEL: modelDrafts.sonnet,
        ANTHROPIC_DEFAULT_OPUS_MODEL: modelDrafts.opus,
        CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS: draft.teammatesMode ? '1' : '0'
      },
      effortLevel: draft.maxThinking ? 'max' : 'default',
      enabledPlugins: {},
      includeCoAuthoredBy: !draft.hideAiSignature,
      pluginConfigs: {},
      teammateMode: draft.teammatesMode ? 'tmux' : 'off'
    },
    null,
    2
  )
})

function ensureActiveCli() {
  if (props.cliTargets.find(item => item.id === activeCli.value)) {
    return
  }

  activeCli.value = props.cliTargets[0]?.id || ''
}

function selectCli(cli) {
  activeCli.value = cli
  clearDraft()
}

function selectProvider(provider) {
  editProvider(provider)
}

function editProvider(provider) {
  draft.id = provider.id
  draft.cli = provider.cli || activeCli.value
  draft.name = provider.name
  draft.note = provider.note || ''
  draft.website = provider.website || ''
  draft.type = provider.type
  draft.baseUrl = provider.baseUrl || ''
  draft.proxy = provider.proxy || ''
  draft.apiKey = ''
  draft.enabled = provider.enabled !== false
  modelDrafts.main = firstModelName(provider.id)
  modelDrafts.haiku = firstModelName(provider.id)
  modelDrafts.sonnet = firstModelName(provider.id)
  modelDrafts.opus = firstModelName(provider.id)
  viewMode.value = 'edit'
}

function createProvider() {
  clearDraft()
  viewMode.value = 'edit'
}

function clearDraft() {
  draft.id = ''
  draft.cli = activeCli.value
  draft.name = ''
  draft.note = ''
  draft.website = ''
  draft.type = 'anthropic'
  draft.baseUrl = ''
  draft.proxy = ''
  draft.apiKey = ''
  draft.authField = 'ANTHROPIC_AUTH_TOKEN'
  draft.enabled = true
  modelDrafts.main = ''
  modelDrafts.haiku = ''
  modelDrafts.sonnet = ''
  modelDrafts.opus = ''
}

function firstModelName(providerId) {
  return props.runtimeModels.find(item => item.providerId === providerId)?.name || ''
}

function fillModelDrafts() {
  const model = modelDrafts.main || modelDrafts.haiku || ''
  modelDrafts.main = model
  modelDrafts.haiku = model
  modelDrafts.sonnet = model
  modelDrafts.opus = model
}

function submitProvider() {
  const payload = {
    id: draft.id || undefined,
    cli: draft.cli,
    name: draft.name,
    note: draft.note,
    website: draft.website,
    type: draft.type,
    baseUrl: draft.baseUrl,
    proxy: draft.proxy,
    enabled: draft.enabled
  }

  if (draft.apiKey) {
    payload.apiKey = draft.apiKey
  }

  emit('save-provider', payload)

  if (draft.id && modelDrafts.main) {
    emit('save-model', {
      id: `${draft.id}:${modelDrafts.main}`,
      providerId: draft.id,
      name: modelDrafts.main
    })
  }

  viewMode.value = 'list'
}

function enableProvider(provider) {
  const model = firstModelName(provider.id)

  if (!model) {
    return
  }

  emit('switch-runtime', {
    cli: activeCli.value,
    providerId: provider.id,
    model
  })
}

function removeProvider(provider) {
  const shouldContinue = window.confirm(
    '删除 Provider 会同时删除关联模型和 Runtime Profile，是否继续？'
  )

  if (shouldContinue) {
    emit('delete-provider', provider.id)
  }
}

watch(
  () => [props.cliTargets, props.providers],
  () => {
    ensureActiveCli()
  },
  { deep: true, immediate: true }
)
</script>

<style scoped lang="less">
.providers-view {
  display: flex;
  min-height: 100%;
  flex-direction: column;
  background: #ffffff;
}

.providers-view__toolbar {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 12px;
  min-height: 58px;
  padding: 0 14px;
  border-bottom: 1px solid #edf0f3;
  background: #ffffff;
}

.providers-view__brand,
.providers-view__tools,
.providers-view__cli-tabs,
.providers-view__provider-actions,
.providers-view__section-actions,
.providers-view__json-title,
.providers-view__check-row,
.providers-view__config-card {
  display: flex;
  align-items: center;
}

.providers-view__brand {
  gap: 14px;
}

.providers-view__brand strong {
  color: #0878ff;
  font-size: 1.08rem;
}

.providers-view__tools {
  justify-content: flex-end;
  gap: 8px;
}

.providers-view__cli-tabs {
  justify-content: center;
  gap: 4px;
  padding: 4px;
  border-radius: 12px;
  background: #f5f6f8;
}

.providers-view__cli-tab {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 38px;
  padding: 0 16px;
  border: 0;
  border-radius: 10px;
  background: transparent;
  color: #667085;
  cursor: pointer;
  font-weight: 600;
}

.providers-view__cli-tab--active {
  background: #ffffff;
  color: #111827;
  box-shadow: 0 1px 5px rgba(15, 23, 42, 0.08);
}

.providers-view__cli-icon {
  width: 18px;
  height: 18px;
}

.providers-view__icon-button,
.providers-view__add,
.providers-view__back {
  display: inline-grid;
  place-items: center;
  border: 0;
  background: transparent;
  color: #667085;
  cursor: pointer;
}

.providers-view__icon-button {
  width: 30px;
  height: 30px;
}

.providers-view__icon-button--danger {
  color: #98a2b3;
}

.providers-view__add {
  width: 38px;
  height: 38px;
  border-radius: 12px;
  background: #ff6a00;
  color: #ffffff;
}

.providers-view__toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 7px;
  border-radius: 999px;
  background: #f0f2f5;
  color: #667085;
}

.providers-view__toggle input {
  display: none;
}

.providers-view__toggle span {
  width: 38px;
  height: 22px;
  border-radius: 999px;
  background: #d7dbe1;
}

.providers-view__toggle span::before {
  content: '';
  display: block;
  width: 20px;
  height: 20px;
  margin: 1px;
  border-radius: 999px;
  background: #ffffff;
}

.providers-view__toggle input:checked + span::before {
  margin-left: 17px;
}

.providers-view__list-panel {
  display: flex;
  overflow: auto;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  border: 1px solid #e5e7eb;
  border-radius: 0;
  background: #ffffff;
}

.providers-view__provider-card {
  display: grid;
  grid-template-columns: 24px 36px minmax(0, 1fr) auto;
  gap: 12px;
  align-items: center;
  min-height: 86px;
  padding: 16px 18px;
  border: 1px solid #dfe3e8;
  border-radius: 14px;
  background: #ffffff;
  cursor: pointer;
}

.providers-view__provider-card--active {
  border-color: #1682ff;
  background: #eef7ff;
}

.providers-view__drag {
  color: #c0c4cc;
}

.providers-view__avatar,
.providers-view__edit-avatar {
  display: grid;
  place-items: center;
  border: 1px solid #e5e7eb;
  border-radius: 12px;
  background: #f8fafc;
  color: #ff6a00;
  font-weight: 700;
}

.providers-view__avatar {
  width: 32px;
  height: 32px;
}

.providers-view__provider-main {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 7px;
}

.providers-view__provider-main strong {
  color: #111827;
}

.providers-view__provider-main span {
  overflow: hidden;
  color: #006eff;
  font-size: 0.9rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.providers-view__provider-actions {
  gap: 8px;
}

.providers-view__enable,
.providers-view__using,
.providers-view__primary,
.providers-view__section-actions button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 36px;
  padding: 0 14px;
  border: 0;
  border-radius: 8px;
  cursor: pointer;
  font-weight: 600;
}

.providers-view__enable,
.providers-view__primary {
  background: #1682ff;
  color: #ffffff;
}

.providers-view__using {
  background: #edf0f4;
  color: #98a2b3;
}

.providers-view__empty {
  display: grid;
  min-height: 220px;
  place-items: center;
  border: 1px dashed #d8dde5;
  border-radius: 14px;
  color: #667085;
}

.providers-view__edit-header {
  display: flex;
  align-items: center;
  gap: 16px;
  height: 64px;
  padding: 0 24px;
  background: #ffffff;
}

.providers-view__back {
  width: 36px;
  height: 36px;
  border: 1px solid #dfe3e8;
  border-radius: 12px;
}

.providers-view__edit-header h1 {
  margin: 0;
  font-size: 1.18rem;
}

.providers-view__edit-panel {
  display: flex;
  overflow: auto;
  flex: 1;
  flex-direction: column;
  gap: 24px;
  margin: 22px 24px 78px;
  padding: 24px;
  border: 1px solid #dfe3e8;
  border-radius: 14px;
  background: #ffffff;
}

.providers-view__edit-avatar {
  width: 78px;
  height: 78px;
  align-self: center;
  font-size: 1.4rem;
}

.providers-view__form-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 22px 16px;
}

.providers-view__field {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 9px;
}

.providers-view__field--wide {
  grid-column: span 2;
}

.providers-view__field span,
.providers-view__section-title p {
  color: #667085;
}

.providers-view__field input,
.providers-view__field select {
  min-width: 0;
  height: 38px;
  padding: 0 12px;
  border: 1px solid #dfe3e8;
  border-radius: 8px;
  background: #ffffff;
  color: #111827;
}

.providers-view__warning {
  padding: 12px 14px;
  border: 1px solid #ffd56a;
  border-radius: 12px;
  background: #fff9e8;
  color: #e07800;
  font-size: 0.86rem;
}

.providers-view__advanced {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.providers-view__advanced summary {
  cursor: pointer;
  font-weight: 700;
}

.providers-view__advanced .providers-view__field {
  margin-top: 14px;
}

.providers-view__section-title {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding-top: 8px;
  border-top: 1px solid #edf0f3;
}

.providers-view__section-title h2 {
  margin: 0 0 8px;
  font-size: 1rem;
}

.providers-view__section-title p {
  margin: 0 0 16px;
  font-size: 0.86rem;
}

.providers-view__section-actions {
  gap: 8px;
}

.providers-view__section-actions button {
  border: 1px solid #dfe3e8;
  background: #ffffff;
  color: #667085;
}

.providers-view__json {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.providers-view__json-title {
  justify-content: space-between;
}

.providers-view__json-title label,
.providers-view__check-row label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: #667085;
}

.providers-view__check-row {
  flex-wrap: wrap;
  gap: 16px;
}

.providers-view__json pre {
  overflow: auto;
  min-height: 190px;
  margin: 0;
  padding: 16px 18px;
  background: #f0f7ff;
  color: #991b1b;
  font-size: 0.85rem;
  line-height: 1.55;
}

.providers-view__config-card {
  gap: 12px;
  min-height: 56px;
  padding: 0 14px;
  border: 1px solid #edf0f3;
  border-radius: 12px;
  color: #667085;
}

.providers-view__config-card strong {
  flex: 1;
  color: #111827;
}

.providers-view__edit-footer {
  position: fixed;
  right: 0;
  bottom: 0;
  left: 0;
  display: flex;
  justify-content: flex-end;
  padding: 16px 24px;
  border-top: 1px solid #edf0f3;
  background: #ffffff;
}

.providers-view__primary:disabled {
  cursor: not-allowed;
  opacity: 0.56;
}
</style>

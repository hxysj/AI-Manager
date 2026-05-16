<template>
  <section class="providers-view">
    <template v-if="viewMode === 'list'">
      <header class="providers-view__toolbar">
        <div class="providers-view__cli-tabs">
          <button
            v-for="cli in visibleCliTargets"
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

        <button
          class="providers-view__add"
          type="button"
          @click="createProvider"
        >
          <Plus :size="22" />
        </button>
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
        >
          <GripVertical class="providers-view__drag" :size="16" />
          <span class="providers-view__avatar">
            <AiIcon
              v-if="provider.icon"
              class="providers-view__avatar-icon"
              :name="provider.icon"
              :alt="`${provider.name} 图标`"
            />
            <template v-else>{{ provider.name.slice(0, 1) }}</template>
          </span>
          <div class="providers-view__provider-main">
            <strong>{{ provider.name }}</strong>
            <span>{{ provider.baseUrl || "未配置官网地址" }}</span>
          </div>
          <div class="providers-view__provider-actions">
            <button
              v-if="profileMap[activeCli]?.providerId === provider.id"
              class="providers-view__using"
              type="button"
              @click.stop="clearRuntime"
            >
              <X :size="15" />
              取消使用
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
        <h1>{{ draft.id ? "编辑供应商" : "新增供应商" }}</h1>
      </header>

      <section class="providers-view__edit-panel">
        <div class="providers-view__avatar-picker">
          <button
            class="providers-view__edit-avatar"
            type="button"
            @click="showIconPicker = !showIconPicker"
          >
            <AiIcon
              v-if="draft.icon"
              class="providers-view__edit-avatar-icon"
              :name="draft.icon"
              :alt="`${draft.name || 'Provider'} 图标`"
            />
            <template v-else>{{ draft.name.slice(0, 1) || "AI" }}</template>
          </button>
          <div v-if="draft.icon" class="providers-view__avatar-name">
            {{ iconLabel(draft.icon) }}
          </div>
          <section v-if="showIconPicker" class="providers-view__icon-panel">
            <label class="providers-view__field providers-view__field--wide">
              <span>搜索图标</span>
              <input
                v-model.trim="iconKeyword"
                type="text"
                placeholder="输入图标名称..."
              />
            </label>
            <div class="providers-view__icon-grid">
              <button
                v-for="icon in filteredIconOptions"
                :key="icon"
                :class="[
                  'providers-view__icon-option',
                  { 'providers-view__icon-option--active': draft.icon === icon }
                ]"
                type="button"
                @click="selectIcon(icon)"
              >
                <AiIcon
                  class="providers-view__icon-option-image"
                  :name="icon"
                  :alt="`${iconLabel(icon)} 图标`"
                />
                <span>{{ iconLabel(icon) }}</span>
              </button>
            </div>
          </section>
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
              :placeholder="
                selectedProvider?.hasApiKey ? '已保存，留空则保持不变' : ''
              "
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

        <details
          v-if="activeRuntimeSchema.advancedFields.length"
          class="providers-view__advanced"
          open
        >
          <summary>高级选项</summary>
          <label
            v-if="activeRuntimeSchema.advancedFields.includes('type')"
            class="providers-view__field"
          >
            <span>API 格式</span>
            <select v-model="draft.type">
              <option v-for="item in providerTypes" :key="item" :value="item">
                {{ providerTypeLabelMap[item] || item }}
              </option>
            </select>
          </label>
          <label
            v-if="activeRuntimeSchema.advancedFields.includes('authField')"
            class="providers-view__field"
          >
            <span>认证字段</span>
            <select v-model="draft.authField">
              <option
                v-for="field in activeRuntimeSchema.authFields"
                :key="field"
                :value="field"
              >
                {{ field }}
              </option>
            </select>
          </label>
        </details>

        <section class="providers-view__models">
          <div class="providers-view__section-title">
            <div>
              <h2>模型映射</h2>
              <p>仅在需要将请求映射到不同模型名称时填写。</p>
            </div>
            <!-- <div class="providers-view__section-actions">
              <button type="button">获取模型列表</button>
            </div> -->
          </div>

          <div class="providers-view__form-grid">
            <label
              v-for="field in activeRuntimeSchema.modelFields"
              :key="field.key"
              class="providers-view__field"
            >
              <span>{{ field.label }}</span>
              <input v-model.trim="modelDrafts[field.key]" type="text" />
              <small v-if="field.description">{{ field.description }}</small>
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
            <label
              v-for="field in activeRuntimeSchema.optionFields"
              :key="field.key"
              class="providers-view__option-field"
            >
              <template v-if="field.type === 'number'">
                <span>{{ field.label }}</span>
                <input
                  v-model.number="draft[field.key]"
                  type="number"
                  :disabled="field.dependsOn && !draft[field.dependsOn]"
                />
              </template>
              <template v-else-if="field.type === 'select'">
                <span>{{ field.label }}</span>
                <select v-model="draft[field.key]">
                  <option
                    v-for="option in field.options"
                    :key="option"
                    :value="option"
                  >
                    {{ option }}
                  </option>
                </select>
              </template>
              <template v-else>
                <input v-model="draft[field.key]" type="checkbox" />
                {{ field.label }}
              </template>
            </label>
          </div>
          <article
            v-for="file in activeRuntimeSchema.configFiles"
            :key="file.name"
            class="providers-view__config-preview"
          >
            <h3>{{ file.name }} ({{ file.format }})</h3>
            <pre>{{ configPreviewMap[file.name] }}</pre>
            <p>{{ file.description }}</p>
          </article>
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
import { computed, reactive, ref, watch } from "vue"
import {
  ArrowLeft,
  ChevronRight,
  FlaskConical,
  Gauge,
  GripVertical,
  Play,
  Plus,
  Save,
  SquarePen,
  Trash2,
  X
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"

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
  runtimeConfigSchemas: {
    type: Object,
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
  "clear-runtime",
  "delete-provider",
  "save-provider",
  "switch-runtime"
])

const providerTypes = [
  "anthropic",
  "openai",
  "gemini",
  "open" + "router",
  "deep" + "seek",
  "custom"
]

const providerTypeLabelMap = {
  anthropic: "Anthropic Messages（原生）",
  openai: "OpenAI Chat Completions（需开启路由）",
  gemini: "Gemini Native generateContent（需开启路由）",
  custom: "Custom"
}

const draft = reactive({
  id: "",
  cli: "",
  icon: "",
  name: "",
  note: "",
  website: "",
  type: "anthropic",
  baseUrl: "",
  proxy: "",
  apiKey: "",
  authField: "ANTHROPIC_AUTH_TOKEN",
  enabled: true,
  writeCommonConfig: true,
  hideAiSignature: false,
  teammatesMode: true,
  toolSearch: false,
  maxThinking: true,
  disableUpgrade: false,
  modelContextWindowEnabled: false,
  serviceTierFast: false,
  modelReasoningEffort: "low",
  modelAutoCompactTokenLimit: 900000
})

const modelDrafts = reactive({
  mainModel: "",
  haikuModel: "",
  sonnetModel: "",
  opusModel: ""
})

const activeCli = ref("")
const viewMode = ref("list")
const showIconPicker = ref(false)
const iconKeyword = ref("")
const iconModules = import.meta.glob("/src/assets/ai-icons/*.svg", {
  query: "?url",
  import: "default"
})
const iconOptions = Object.keys(iconModules)
  .map((item) => item.split("/").pop())
  .sort((left, right) => left.localeCompare(right))

const visibleCliTargets = computed(() => {
  return props.cliTargets.filter((item) => {
    return props.runtimeConfigSchemas[item.id]?.enabled
  })
})

const activeRuntimeSchema = computed(() => {
  return (
    props.runtimeConfigSchemas[activeCli.value] || {
      modelFields: [],
      optionFields: [],
      advancedFields: [],
      configFiles: [],
      authFields: [],
      defaultProviderType: "custom"
    }
  )
})

const selectedProvider = computed(() => {
  return props.providers.find((item) => item.id === draft.id) || null
})

const scopedProviders = computed(() => {
  return props.providers.filter((item) => item.cli === activeCli.value)
})

const profileMap = computed(() => {
  return Object.fromEntries(
    props.runtimeProfiles.map((item) => [item.cli, item])
  )
})

const filteredIconOptions = computed(() => {
  const keyword = iconKeyword.value.toLowerCase()

  return iconOptions.filter((item) =>
    iconLabel(item).toLowerCase().includes(keyword)
  )
})

const configPreviewMap = computed(() => {
  return Object.fromEntries(
    activeRuntimeSchema.value.configFiles.map((file) => [
      file.name,
      applyConfigTemplate(file.template)
    ])
  )
})

function applyConfigTemplate(template) {
  const values = {
    authField: draft.authField,
    apiKey: draft.apiKey || "********",
    baseUrl: draft.baseUrl,
    mainModel: modelDrafts.mainModel,
    haikuModel: modelDrafts.haikuModel,
    sonnetModel: modelDrafts.sonnetModel,
    opusModel: modelDrafts.opusModel,
    toolSearchText: draft.toolSearch ? "true" : "false",
    disableUpgradeText: draft.disableUpgrade ? "1" : "0",
    includeCoAuthoredBy: String(!draft.hideAiSignature),
    teammatesMode: draft.teammatesMode,
    teammateMode: "tmux",
    effortLevel: draft.maxThinking ? "max" : "default",
    writeCommonConfig: draft.writeCommonConfig,
    modelContextWindowEnabled: draft.modelContextWindowEnabled,
    serviceTierFast: draft.serviceTierFast,
    modelReasoningEffort: draft.modelReasoningEffort,
    modelAutoCompactTokenLimit: draft.modelAutoCompactTokenLimit || 900000
  }

  return String(template || "")
    .replace(/\{\{#(\w+)}}([\s\S]*?)\{\{\/\1}}/g, (match, key, content) =>
      values[key] ? content : ""
    )
    .replace(/\{\{(\w+)}}/g, (match, key) => {
      return values[key] ?? match
    })
}

function ensureActiveCli() {
  if (visibleCliTargets.value.find((item) => item.id === activeCli.value)) {
    return
  }

  activeCli.value = visibleCliTargets.value[0]?.id || ""
}

function selectCli(cli) {
  activeCli.value = cli
  clearDraft()
}

function editProvider(provider) {
  draft.id = provider.id
  draft.cli = provider.cli || activeCli.value
  draft.icon = provider.icon || ""
  draft.name = provider.name
  draft.note = provider.note || ""
  draft.website = provider.website || ""
  draft.type = provider.type
  draft.baseUrl = provider.baseUrl || ""
  draft.proxy = provider.proxy || ""
  draft.apiKey = ""
  draft.authField = provider.authField || "ANTHROPIC_AUTH_TOKEN"
  draft.enabled = provider.enabled !== false
  modelDrafts.mainModel =
    provider.runtimeConfig?.mainModel || firstModelName(provider.id)
  modelDrafts.haikuModel =
    provider.runtimeConfig?.haikuModel || firstModelName(provider.id)
  modelDrafts.sonnetModel =
    provider.runtimeConfig?.sonnetModel || firstModelName(provider.id)
  modelDrafts.opusModel =
    provider.runtimeConfig?.opusModel || firstModelName(provider.id)
  draft.hideAiSignature = Boolean(provider.runtimeConfig?.hideAiSignature)
  draft.teammatesMode = provider.runtimeConfig?.teammatesMode !== false
  draft.toolSearch = Boolean(provider.runtimeConfig?.toolSearch)
  draft.maxThinking = provider.runtimeConfig?.maxThinking !== false
  draft.disableUpgrade = Boolean(provider.runtimeConfig?.disableUpgrade)
  draft.writeCommonConfig = provider.runtimeConfig?.writeCommonConfig !== false
  draft.modelContextWindowEnabled = Boolean(
    provider.runtimeConfig?.modelContextWindowEnabled
  )
  draft.serviceTierFast = Boolean(provider.runtimeConfig?.serviceTierFast)
  draft.modelReasoningEffort =
    provider.runtimeConfig?.modelReasoningEffort || "low"
  draft.modelAutoCompactTokenLimit =
    provider.runtimeConfig?.modelAutoCompactTokenLimit || 900000
  showIconPicker.value = false
  iconKeyword.value = ""
  viewMode.value = "edit"
}

function createProvider() {
  clearDraft()
  viewMode.value = "edit"
}

function clearDraft() {
  draft.id = ""
  draft.cli = activeCli.value
  draft.icon = ""
  draft.name = ""
  draft.note = ""
  draft.website = ""
  draft.type = activeRuntimeSchema.value.defaultProviderType
  draft.baseUrl = ""
  draft.proxy = ""
  draft.apiKey = ""
  draft.authField =
    activeRuntimeSchema.value.authFields[0] || "ANTHROPIC_AUTH_TOKEN"
  draft.enabled = true
  draft.hideAiSignature = false
  draft.teammatesMode = true
  draft.toolSearch = false
  draft.maxThinking = true
  draft.disableUpgrade = false
  draft.writeCommonConfig = true
  draft.modelContextWindowEnabled = false
  draft.serviceTierFast = false
  draft.modelReasoningEffort = "low"
  draft.modelAutoCompactTokenLimit = 900000
  modelDrafts.mainModel = ""
  modelDrafts.haikuModel = ""
  modelDrafts.sonnetModel = ""
  modelDrafts.opusModel = ""
  showIconPicker.value = false
  iconKeyword.value = ""
}

function firstModelName(providerId) {
  return (
    props.runtimeModels.find((item) => item.providerId === providerId)?.name ||
    ""
  )
}

function iconLabel(icon) {
  return String(icon || "").replace(/\.svg$/, "")
}

function selectIcon(icon) {
  draft.icon = icon
  showIconPicker.value = false
}

function submitProvider() {
  const payload = {
    id: draft.id || undefined,
    cli: draft.cli,
    icon: draft.icon,
    name: draft.name,
    note: draft.note,
    website: draft.website,
    type: draft.type,
    baseUrl: draft.baseUrl,
    proxy: draft.proxy,
    authField: draft.authField,
    model: modelDrafts.mainModel,
    runtimeConfig: {
      mainModel: modelDrafts.mainModel,
      haikuModel: modelDrafts.haikuModel,
      sonnetModel: modelDrafts.sonnetModel,
      opusModel: modelDrafts.opusModel,
      toolSearch: draft.toolSearch,
      disableUpgrade: draft.disableUpgrade,
      hideAiSignature: draft.hideAiSignature,
      teammatesMode: draft.teammatesMode,
      maxThinking: draft.maxThinking,
      writeCommonConfig: draft.writeCommonConfig,
      modelContextWindowEnabled: draft.modelContextWindowEnabled,
      serviceTierFast: draft.serviceTierFast,
      modelReasoningEffort: draft.modelReasoningEffort,
      modelAutoCompactTokenLimit: draft.modelAutoCompactTokenLimit
    },
    enabled: draft.enabled
  }

  if (draft.apiKey) {
    payload.apiKey = draft.apiKey
  }

  emit("save-provider", payload)

  viewMode.value = "list"
}

function enableProvider(provider) {
  const model = provider.runtimeConfig?.mainModel || firstModelName(provider.id)

  if (!model) {
    return
  }

  emit("switch-runtime", {
    cli: activeCli.value,
    providerId: provider.id,
    model
  })
}

function clearRuntime() {
  emit("clear-runtime", {
    cli: activeCli.value
  })
}

function removeProvider(provider) {
  const shouldContinue = window.confirm(
    "删除 Provider 会同时删除关联模型和 Runtime Profile，是否继续？"
  )

  if (shouldContinue) {
    emit("delete-provider", provider.id)
  }
}

watch(
  () => [visibleCliTargets.value, props.providers],
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
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 58px;
  padding: 0 14px;
  border-bottom: 1px solid #edf0f3;
  background: #ffffff;
}

.providers-view__cli-tabs,
.providers-view__provider-actions,
.providers-view__section-actions,
.providers-view__json-title,
.providers-view__check-row,
.providers-view__config-card {
  display: flex;
  align-items: center;
}

.providers-view__toolbar {
  justify-content: space-between;
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
  display: inline-flex;
  align-items: center;
  justify-content: center;
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
  content: "";
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
  display: flex;
  gap: 12px;
  align-items: center;
  min-height: 86px;
  padding: 16px 18px;
  border: 1px solid #dfe3e8;
  border-radius: 14px;
  background: #ffffff;
}

.providers-view__provider-card--active {
  border-color: #1682ff;
  background: #eef7ff;
}

.providers-view__drag {
  flex: none;
  color: #c0c4cc;
}

.providers-view__avatar,
.providers-view__edit-avatar {
  display: flex;
  align-items: center;
  justify-content: center;
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

.providers-view__avatar-icon {
  width: 22px;
  height: 22px;
}

.providers-view__provider-main {
  display: flex;
  flex: 1;
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
  flex: none;
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
  display: flex;
  min-height: 220px;
  align-items: center;
  justify-content: center;
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
  padding: 0;
  cursor: pointer;
  font-size: 1.4rem;
}

.providers-view__avatar-picker {
  display: flex;
  align-items: center;
  flex-direction: column;
  gap: 10px;
}

.providers-view__edit-avatar-icon {
  width: 48px;
  height: 48px;
}

.providers-view__avatar-name {
  color: #667085;
  font-size: 0.85rem;
}

.providers-view__icon-panel {
  display: flex;
  width: 100%;
  flex-direction: column;
  gap: 14px;
  padding: 16px;
  border: 1px solid #dfe3e8;
  border-radius: 12px;
  background: #fbfcfd;
}

.providers-view__icon-grid {
  display: grid;
  overflow: auto;
  max-height: 360px;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 10px;
  padding-right: 4px;
}

.providers-view__icon-option {
  display: flex;
  min-width: 0;
  height: 86px;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  gap: 8px;
  padding: 8px 6px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: #475467;
  cursor: pointer;
}

.providers-view__icon-option--active {
  border-color: #1682ff;
  background: #eef7ff;
  color: #111827;
}

.providers-view__icon-option-image {
  width: 30px;
  height: 30px;
  flex: none;
}

.providers-view__icon-option span {
  overflow: hidden;
  width: 100%;
  font-size: 0.78rem;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.providers-view__form-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 22px 16px;
}

.providers-view__field {
  display: flex;
  min-width: 0;
  flex: 1 1 calc(50% - 8px);
  flex-direction: column;
  gap: 9px;
}

.providers-view__field--wide {
  flex-basis: 100%;
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
.providers-view__option-field {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: #667085;
}

.providers-view__option-field input[type="number"] {
  width: 112px;
  height: 32px;
  padding: 0 10px;
  border: 1px solid #dfe3e8;
  border-radius: 8px;
  color: #111827;
}

.providers-view__option-field select {
  height: 32px;
  padding: 0 10px;
  border: 1px solid #dfe3e8;
  border-radius: 8px;
  background: #ffffff;
  color: #111827;
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

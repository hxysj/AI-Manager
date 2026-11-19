<template>
  <fieldset
    class="agent-model-fields"
    :data-agent="modelValue?.agentType"
    :disabled="disabled"
    aria-label="Agent 模型配置"
  >
    <section class="agent-config-section" aria-label="基础设置">
      <header class="agent-section-heading">
        <SlidersHorizontal :size="18" />
        <h3 class="agent-section-title">基础设置</h3>
      </header>
      <div class="agent-basic-grid">
        <slot name="source"></slot>
        <label class="model-field">
          {{ modelValue?.agentType === "claude" ? "主模型" : "默认模型" }}
          <span class="agent-select">
            <select
              class="input"
              :value="modelValue?.defaultModelName || ''"
              :disabled="disabled || !modelValue"
              @change="updateDefault($event.target.value)"
            >
              <option value="">
                {{ modelValue ? "应用时选择" : "请先选择使用来源" }}
              </option>
              <option
                v-for="model in models"
                :key="model.upstreamModelId"
                :value="model.upstreamModelId"
              >
                {{ model.displayName || model.upstreamModelId }}
              </option>
            </select>
            <ChevronDown
              class="agent-select-arrow"
              :size="16"
              aria-hidden="true"
            />
          </span>
        </label>
      </div>
      <slot v-if="!modelValue" name="empty"></slot>
      <div
        v-else-if="providerKind !== 'api'"
        class="agent-official-models"
        role="status"
      >
        <ShieldCheck :size="17" />
        <span>使用官方模型列表</span>
      </div>
    </section>

    <template v-if="modelValue?.agentType === 'codex'">
      <ProviderModelListEditor
        v-if="providerKind === 'api'"
        class="agent-catalog"
        :model-value="modelNames"
        :selected-model="modelValue.defaultModelName"
        title="服务商模型列表"
        empty-hint="请在服务商模型目录中配置可用模型。"
        readonly
        selectable
        show-readonly-summary
        :show-sync-control="false"
        :show-reset="false"
        :disabled="disabled"
        max-height="190px"
        @update:selected-model="updateDefault"
      >
        <template #title-icon>
          <Layers3 :size="18" />
        </template>
      </ProviderModelListEditor>
      <section class="agent-config-section" aria-label="模型思考档位">
        <header class="agent-section-heading">
          <SlidersHorizontal :size="18" />
          <h3 class="agent-section-title">模型思考档位</h3>
        </header>
        <div class="agent-basic-grid">
          <label class="model-field">
            配置模型
            <span class="agent-select">
              <select v-model="reasoningModelName" class="input">
                <option value="">当前默认模型</option>
                <option
                  v-for="model in models"
                  :key="model.upstreamModelId"
                  :value="model.upstreamModelId"
                >
                  {{ model.displayName || model.upstreamModelId }}
                </option>
              </select>
              <ChevronDown
                class="agent-select-arrow"
                :size="16"
                aria-hidden="true"
              />
            </span>
          </label>
          <label
            v-if="reasoningModel?.reasoning?.state === 'known'"
            class="model-field"
          >
            目录默认档位
            <span class="agent-select">
              <select
                class="input"
                :value="reasoningOverride.defaultEffort || ''"
                @change="
                  updateReasoningOverride({
                    ...reasoningOverride,
                    defaultEffort: $event.target.value || null
                  })
                "
              >
                <option value="">
                  跟随目录（{{
                    reasoningEffortLabels[effectiveCatalogDefault] ||
                    effectiveCatalogDefault
                  }}）
                </option>
                <option
                  v-for="level in effectiveLevels"
                  :key="level"
                  :value="level"
                >
                  {{ reasoningEffortLabels[level] || level }}
                </option>
              </select>
              <ChevronDown
                class="agent-select-arrow"
                :size="16"
                aria-hidden="true"
              />
            </span>
          </label>
        </div>
        <template v-if="reasoningModel?.reasoning?.state === 'known'">
          <div class="agent-effort-settings">
            <div class="agent-effort-heading">
              <span>可选档位</span>
              <button
                v-if="hasReasoningOverride"
                class="agent-effort-reset"
                type="button"
                @click="updateReasoningOverride(null)"
              >
                <RotateCcw :size="13" aria-hidden="true" />
                <span>恢复跟随</span>
              </button>
            </div>
            <div
              class="agent-effort-options"
              role="group"
              aria-label="可选档位"
            >
              <button
                v-for="level in reasoningModel.reasoning.levels"
                :key="level"
                class="agent-effort-toggle"
                type="button"
                :aria-pressed="effectiveLevels.includes(level)"
                :disabled="
                  effectiveLevels.length === 1 && effectiveLevels[0] === level
                "
                @click="
                  toggleReasoningLevel(level, !effectiveLevels.includes(level))
                "
              >
                <Check
                  class="agent-effort-check"
                  :size="13"
                  aria-hidden="true"
                />
                <span>{{ reasoningEffortLabels[level] || level }}</span>
              </button>
            </div>
            <p class="model-hint">
              {{
                reasoningModel.reasoning.source === "embedded"
                  ? "来源：内置能力目录"
                  : "来源：服务商模型目录"
              }}
              <template
                v-if="
                  providerKind === 'api' &&
                  modelValue.options.syncModelList === false
                "
              >
                · 开启同步模型列表后，Codex 才会读取这些选项。
              </template>
            </p>
          </div>
        </template>
        <p v-else class="model-hint">
          {{
            reasoningModel?.reasoning?.state === "unsupported"
              ? "该模型声明不支持选择思考档位。"
              : "暂无该模型的档位资料，可通过服务商模型目录更新。"
          }}
        </p>
      </section>
      <section class="agent-config-section" aria-label="子 Agent 默认值">
        <header class="agent-section-heading">
          <Workflow :size="18" />
          <h3 class="agent-section-title">子 Agent 默认值</h3>
        </header>
        <div class="agent-basic-grid">
          <label class="model-field">
            默认模型
            <span class="agent-select">
              <select
                class="input"
                :value="modelValue.options.defaultSubagentModel || ''"
                @change="updateSubagentModel($event.target.value)"
              >
                <option value="">由 Codex 决定</option>
                <option
                  v-for="model in models"
                  :key="model.upstreamModelId"
                  :value="model.upstreamModelId"
                >
                  {{ model.displayName || model.upstreamModelId }}
                </option>
              </select>
              <ChevronDown
                class="agent-select-arrow"
                :size="16"
                aria-hidden="true"
              />
            </span>
          </label>
          <label class="model-field">
            默认思考强度
            <span class="agent-select">
              <select
                class="input"
                :value="modelValue.options.defaultSubagentReasoningEffort || ''"
                @change="
                  updateOption(
                    'defaultSubagentReasoningEffort',
                    $event.target.value || null
                  )
                "
              >
                <option value="">由 Codex 决定</option>
                <option
                  v-for="level in subagentLevels"
                  :key="level"
                  :value="level"
                >
                  {{ reasoningEffortLabels[level] || level }}
                </option>
              </select>
              <ChevronDown
                class="agent-select-arrow"
                :size="16"
                aria-hidden="true"
              />
            </span>
          </label>
        </div>
      </section>
      <section
        v-if="providerKind === 'api' || schema.optionFields?.length"
        class="agent-config-section"
        aria-label="高级选项"
      >
        <header class="agent-section-heading">
          <Settings2 :size="18" />
          <h3 class="agent-section-title">高级选项</h3>
        </header>
        <div class="agent-options-grid">
          <div
            v-for="(fields, column) in codexOptionColumns"
            :key="column"
            class="agent-option-column"
          >
            <div
              v-if="column === 0 && providerKind === 'api'"
              class="agent-option-row"
            >
              <div class="agent-switch-line">
                <span>同步模型列表</span>
                <el-switch
                  :model-value="Boolean(modelValue.options.syncModelList)"
                  :disabled="disabled"
                  aria-label="同步模型列表"
                  @update:model-value="updateOption('syncModelList', $event)"
                />
              </div>
              <p class="model-hint">
                保存并应用时，将服务商模型目录同步到 Codex。
              </p>
            </div>
            <div
              v-for="field in fields"
              :key="field.key"
              class="agent-option-row"
            >
              <template v-if="field.type === 'boolean'">
                <div class="agent-switch-line">
                  <span>{{ field.label }}</span>
                  <el-switch
                    :model-value="Boolean(modelValue.options[field.key])"
                    :disabled="disabled"
                    :aria-label="field.label"
                    @update:model-value="updateOption(field.key, $event)"
                  />
                </div>
                <p v-if="optionHints[field.key]" class="model-hint">
                  {{ optionHints[field.key] }}
                </p>
              </template>
              <label v-else-if="field.type === 'select'" class="model-field">
                {{ field.label }}
                <span class="agent-select">
                  <select
                    class="input"
                    :value="modelValue.options[field.key] || ''"
                    @change="
                      updateOption(field.key, $event.target.value || null)
                    "
                  >
                    <option value="">沿用本地配置</option>
                    <option
                      v-for="option in optionValues(field)"
                      :key="option"
                      :value="option"
                    >
                      {{
                        field.key === "modelReasoningEffort"
                          ? reasoningEffortLabels[option] || option
                          : option
                      }}
                    </option>
                  </select>
                  <ChevronDown
                    class="agent-select-arrow"
                    :size="16"
                    aria-hidden="true"
                  />
                </span>
              </label>
              <template v-else-if="field.type === 'number'">
                <label class="model-field">
                  {{ field.label }}
                  <span class="agent-number-control">
                    <input
                      class="input"
                      type="number"
                      min="1"
                      step="1"
                      :value="modelValue.options[field.key]"
                      :disabled="
                        disabled ||
                        Boolean(
                          field.dependsOn &&
                          !modelValue.options[field.dependsOn]
                        )
                      "
                      @input="
                        updateOption(
                          field.key,
                          $event.target.value
                            ? Number($event.target.value)
                            : null
                        )
                      "
                    />
                    <span
                      v-if="field.key === 'modelAutoCompactTokenLimit'"
                      class="agent-number-unit"
                      >tokens</span
                    >
                  </span>
                </label>
                <p
                  v-if="field.dependsOn && !modelValue.options[field.dependsOn]"
                  class="model-hint"
                >
                  开启「{{
                    schema.optionFields.find(
                      (option) => option.key === field.dependsOn
                    )?.label
                  }}」后可设置。
                </p>
              </template>
            </div>
          </div>
        </div>
      </section>
    </template>

    <section
      v-else-if="modelValue?.agentType === 'claude'"
      class="agent-config-section"
      aria-label="模型档位"
    >
      <header class="agent-section-heading">
        <Workflow :size="18" />
        <h3 class="agent-section-title">模型档位</h3>
      </header>
      <p class="model-hint">按任务指定模型，未单独设置的档位跟随主模型。</p>
      <div class="agent-model-grid">
        <label
          v-for="field in claudeFields"
          :key="field.key"
          class="model-field"
        >
          {{ field.label }}
          <span class="agent-select">
            <select
              class="input"
              :value="modelValue.options[field.key] || ''"
              @change="updateOption(field.key, $event.target.value)"
            >
              <option value="">跟随主模型</option>
              <option
                v-for="model in models"
                :key="model.upstreamModelId"
                :value="model.upstreamModelId"
              >
                {{ model.displayName || model.upstreamModelId }}
              </option>
            </select>
            <ChevronDown
              class="agent-select-arrow"
              :size="16"
              aria-hidden="true"
            />
          </span>
        </label>
      </div>
    </section>

    <section
      v-else-if="modelValue?.agentType === 'claude-desktop'"
      class="agent-config-section"
      aria-label="模型映射"
    >
      <header class="agent-section-heading">
        <Workflow :size="18" />
        <h3 class="agent-section-title">模型映射</h3>
      </header>
      <p class="model-hint">
        空档位继承首个已配置模型；全部留空时使用默认模型。1M
        能力随模型继承，也可单独设置。
      </p>
      <div class="agent-desktop-grid">
        <div
          v-for="route in desktopRoutes"
          :key="route.id"
          class="desktop-model"
        >
          <div class="desktop-model-heading">
            <span class="desktop-model-title">{{ route.label }} 模型</span>
            <el-switch
              :model-value="desktopSupports1m(route.id)"
              :disabled="disabled"
              :aria-label="`${route.label} 支持 1M`"
              inline-prompt
              active-text="1M"
              inactive-text="1M"
              @update:model-value="
                updateDesktop(route.id, 'supports1m', $event)
              "
            />
          </div>
          <label class="model-field">
            使用模型
            <span class="agent-select">
              <select
                class="input"
                :value="desktopMapping(route.id).model || ''"
                @change="updateDesktop(route.id, 'model', $event.target.value)"
              >
                <option value="">继承首个模型 / 默认模型</option>
                <option
                  v-for="model in models"
                  :key="model.upstreamModelId"
                  :value="model.upstreamModelId"
                >
                  {{ model.displayName || model.upstreamModelId }}
                </option>
              </select>
              <ChevronDown
                class="agent-select-arrow"
                :size="16"
                aria-hidden="true"
              />
            </span>
          </label>
          <label class="model-field">
            显示名称（可选）
            <input
              class="input"
              :value="desktopMapping(route.id).labelOverride || ''"
              placeholder="使用默认名称"
              @input="
                updateDesktop(route.id, 'labelOverride', $event.target.value)
              "
            />
          </label>
        </div>
      </div>
    </section>
    <p v-else-if="modelValue" class="agent-tool-hint">
      <Info :size="17" />
      多协议工具可指定服务商目录中的模型，未指定模型时使用此默认模型。
    </p>
  </fieldset>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import { ElSwitch } from "element-plus"
import "element-plus/es/components/switch/style/css"
import {
  Check,
  ChevronDown,
  Info,
  Layers3,
  RotateCcw,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Workflow
} from "lucide-vue-next"
import { reasoningEffortLabels } from "@/constants/reasoning-efforts"
import ProviderModelListEditor from "./ProviderModelListEditor.vue"

const props = defineProps({
  modelValue: { type: Object, default: null },
  providerKind: { type: String, default: "api" },
  models: { type: Array, default: () => [] },
  schema: { type: Object, default: () => ({}) },
  desktopRoutes: { type: Array, default: () => [] },
  disabled: { type: Boolean, default: false }
})
const emit = defineEmits(["update:modelValue"])
// Codex 与其他 Agent 共用服务商目录，只单独保存同步开关和模型规则。
const modelNames = computed(() =>
  props.models.map((model) => model.upstreamModelId).join("\n")
)
// 常用开关和请求参数分列，沿用各字段的依赖规则。
const codexOptionColumns = computed(() => {
  const primaryKeys = ["modelContextWindowEnabled", "modelReasoningEffort"]
  const fields = props.schema.optionFields || []
  return [
    primaryKeys.flatMap((key) => fields.filter((field) => field.key === key)),
    fields.filter((field) => !primaryKeys.includes(field.key))
  ]
})
const optionHints = {
  modelContextWindowEnabled: "为当前 Agent 配置 1M 上下文窗口。",
  serviceTierFast: "保存并应用时，启用 Fast 请求模式。"
}
const claudeFields = computed(() => [
  ...(props.schema.modelFields || []).filter(
    (field) => field.key !== "mainModel"
  ),
  { key: "subagentModel", label: "子 Agent 模型" }
])

// 档位编辑与主模型选择独立，避免编辑目录时切换正在配置的主模型。
const reasoningModelName = ref("")
// 服务商目录刷新后，已删除的模型退回当前默认项。
watch(
  () => props.models,
  (models) => {
    if (
      !models.some(
        (model) => model.upstreamModelId === reasoningModelName.value
      )
    )
      reasoningModelName.value = ""
  }
)
const defaultModel = computed(
  () =>
    props.models.find(
      (model) => model.upstreamModelId === props.modelValue?.defaultModelName
    ) || props.models[0]
)
const reasoningModel = computed(
  () =>
    props.models.find(
      (model) => model.upstreamModelId === reasoningModelName.value
    ) || defaultModel.value
)
const reasoningOverride = computed(
  () =>
    props.modelValue?.options.reasoningOverrides?.[
      reasoningModel.value?.upstreamModelId
    ] || {}
)
const hasReasoningOverride = computed(
  () => Object.keys(reasoningOverride.value).length > 0
)
const effectiveLevels = computed(() => modelLevels(reasoningModel.value) || [])
const effectiveCatalogDefault = computed(() =>
  effectiveLevels.value.includes(reasoningModel.value?.reasoning?.defaultEffort)
    ? reasoningModel.value.reasoning.defaultEffort
    : effectiveLevels.value[0]
)
const subagentLevels = computed(
  () =>
    modelLevels(
      props.models.find(
        (model) =>
          model.upstreamModelId ===
          props.modelValue?.options.defaultSubagentModel
      )
    ) || ["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"]
)

// 能力未知时保留客户端原生选择，明确不支持时展示空列表。
function modelLevels(model) {
  const levels = model?.reasoning?.levels
  const allowed =
    props.modelValue?.options.reasoningOverrides?.[model?.upstreamModelId]
      ?.allowedEfforts
  return allowed?.length
    ? levels?.filter((level) => allowed.includes(level))
    : levels
}

function updateReasoningOverride(value) {
  const model = reasoningModel.value
  if (!model || !props.modelValue || props.disabled) return
  const overrides = { ...props.modelValue.options.reasoningOverrides }
  if (value?.allowedEfforts?.length || value?.defaultEffort)
    overrides[model.upstreamModelId] = value
  else delete overrides[model.upstreamModelId]
  const options = { ...props.modelValue.options, reasoningOverrides: overrides }
  const levels = value?.allowedEfforts?.length
    ? value.allowedEfforts
    : model.reasoning.levels
  if (
    model === defaultModel.value &&
    options.modelReasoningEffort &&
    !levels.includes(options.modelReasoningEffort)
  )
    options.modelReasoningEffort = null
  if (
    model.upstreamModelId === options.defaultSubagentModel &&
    options.defaultSubagentReasoningEffort &&
    !levels.includes(options.defaultSubagentReasoningEffort)
  )
    options.defaultSubagentReasoningEffort = null
  emit("update:modelValue", { ...props.modelValue, options })
}

function toggleReasoningLevel(level, checked) {
  const levels = reasoningModel.value.reasoning.levels.filter((item) =>
    item === level ? checked : effectiveLevels.value.includes(item)
  )
  const current = reasoningOverride.value.defaultEffort
  updateReasoningOverride({
    allowedEfforts:
      levels.length === reasoningModel.value.reasoning.levels.length
        ? []
        : levels,
    defaultEffort: levels.includes(current) ? current : null
  })
}

function updateSubagentModel(value) {
  const options = {
    ...props.modelValue.options,
    defaultSubagentModel: value || null
  }
  const levels = modelLevels(
    props.models.find((model) => model.upstreamModelId === value)
  )
  if (
    levels &&
    options.defaultSubagentReasoningEffort &&
    !levels.includes(options.defaultSubagentReasoningEffort)
  )
    options.defaultSubagentReasoningEffort = null
  emit("update:modelValue", { ...props.modelValue, options })
}

function updateDefault(value) {
  if (!props.modelValue || props.disabled) return
  const next = { ...props.modelValue, defaultModelName: value }
  const levels = modelLevels(
    props.models.find((model) => model.upstreamModelId === value)
  )
  if (
    next.agentType === "codex" &&
    next.options.modelReasoningEffort &&
    levels &&
    !levels.includes(next.options.modelReasoningEffort)
  )
    next.options = { ...next.options, modelReasoningEffort: null }
  emit("update:modelValue", next)
}

function optionValues(field) {
  const values = [...(field.options || [])]
  if (field.key !== "modelReasoningEffort") return values
  const levels = modelLevels(defaultModel.value)
  if (levels) return levels
  const current = props.modelValue.options[field.key]
  // 资料缺失时允许保留现有值，界面仍显示能力未知。
  if (current && !values.includes(current)) values.push(current)
  return values
}

function updateOption(key, value) {
  if (!props.modelValue || props.disabled) return
  emit("update:modelValue", {
    ...props.modelValue,
    options: { ...props.modelValue.options, [key]: value }
  })
}

function desktopMapping(id) {
  return props.modelValue.options.modelMappings?.[id] || {}
}

function desktopSupports1m(id) {
  const mapping = desktopMapping(id)
  if (typeof mapping.supports1m === "boolean") return mapping.supports1m
  if (mapping.model) return false
  const first = props.desktopRoutes
    .map((route) => desktopMapping(route.id))
    .find((mapping) => mapping.model)
  return Boolean(first?.supports1m)
}

function updateDesktop(id, key, value) {
  updateOption("modelMappings", {
    ...props.modelValue.options.modelMappings,
    [id]: { ...desktopMapping(id), [key]: value }
  })
}
</script>

<style scoped lang="less">
.agent-model-fields {
  display: flex;
  margin: 0;
  min-width: 0;
  flex-direction: column;
  gap: 18px;
  padding: 0;
  border: 0;
  color: var(--color-text);
  --el-switch-on-color: var(--color-primary-solid);
  --el-switch-off-color: var(--color-line-strong);

  .agent-config-section,
  .agent-catalog {
    min-width: 0;
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);
  }

  .agent-section-heading {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 18px;
    color: var(--color-primary);

    .agent-section-title {
      margin: 0;
      color: var(--color-text);
      font-size: var(--font-size-base);
    }
  }

  .agent-config-section {
    .agent-effort-settings {
      display: flex;
      flex-direction: column;
      gap: 9px;
      margin-top: 16px;
      padding-top: 12px;
      border-top: 1px solid var(--color-line);

      .agent-effort-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        min-height: 24px;
        font-size: var(--font-size-sm);

        .agent-effort-reset {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 3px 6px;
          border: 0;
          border-radius: 5px;
          background: transparent;
          color: var(--color-primary);
          font: inherit;
          cursor: pointer;

          &:hover {
            background: var(--color-primary-soft);
          }

          &:disabled {
            cursor: default;
          }
        }
      }

      .agent-effort-options {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;

        .agent-effort-toggle {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 5px;
          min-width: 63px;
          min-height: 32px;
          padding: 4px 9px;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          font: inherit;
          font-size: var(--font-size-sm);
          line-height: 1.2;
          cursor: pointer;

          .agent-effort-check {
            flex: none;
            visibility: hidden;
          }

          &[aria-pressed="true"] {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            color: var(--color-primary);

            .agent-effort-check {
              visibility: visible;
            }
          }

          &:hover:not(:disabled) {
            border-color: var(--color-primary);
          }

          &:focus-visible {
            outline: 2px solid var(--color-primary);
            outline-offset: 2px;
          }

          &:disabled {
            cursor: default;
          }
        }
      }
    }
  }

  .agent-basic-grid,
  .agent-model-grid,
  .agent-desktop-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 18px 24px;
  }

  .agent-official-models {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    color: var(--color-primary);
    font-size: var(--font-size-sm);
  }

  .model-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 8px;
    font-size: var(--font-size-sm);

    .input {
      box-sizing: border-box;
      width: 100%;
      min-width: 0;
      height: 42px;
      padding: 8px 12px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;

      &:focus-visible {
        border-color: var(--color-primary);
        outline: 2px solid var(--color-info-line);
        outline-offset: 1px;
      }

      &:disabled {
        background: var(--color-panel-soft);
        color: var(--color-text-soft);
        cursor: not-allowed;
      }
    }

    .agent-number-control {
      position: relative;

      .input {
        padding-right: 72px;
      }

      .agent-number-unit {
        position: absolute;
        top: 50%;
        right: 28px;
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        pointer-events: none;
        transform: translateY(-50%);
      }
    }
  }

  .agent-catalog {
    :deep(.provider-model-title) {
      font-size: var(--font-size-base);
    }

    :deep(.provider-model-heading) {
      gap: 9px;
      color: var(--color-primary);
    }

    :deep(.provider-model-count) {
      background: var(--color-panel);
      color: var(--color-text-muted);
    }

    :deep(.provider-model-surface) {
      border: 0;
      background: transparent;
    }

    :deep(.provider-model-list) {
      padding: 2px;
      gap: 10px;
    }

    :deep(.provider-model-item) {
      flex-basis: calc(50% - 5px);
    }
  }

  .agent-options-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 24px;

    .agent-option-column {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 20px;

      & + .agent-option-column {
        padding-left: 24px;
        border-left: 1px solid var(--color-line);
      }

      .agent-option-row {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 6px;

        .agent-switch-line {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 16px;
          min-height: 32px;
          font-size: var(--font-size-sm);
        }
      }
    }
  }

  .agent-model-grid,
  .agent-desktop-grid {
    margin-top: 18px;
  }

  .agent-desktop-grid {
    .desktop-model {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 14px;
      padding: 16px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);

      .desktop-model-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;

        .desktop-model-title {
          font-size: var(--font-size-sm);
        }
      }
    }
  }

  .model-hint {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    line-height: 1.7;
  }

  .agent-tool-hint {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
}
</style>

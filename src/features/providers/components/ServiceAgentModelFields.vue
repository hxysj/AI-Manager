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
                      {{ option }}
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
import { computed, watch } from "vue"
import { ElSwitch } from "element-plus"
import "element-plus/es/components/switch/style/css"
import {
  ChevronDown,
  Info,
  Layers3,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Workflow
} from "lucide-vue-next"
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

// 公共目录变化也可能改变默认模型，继续遵循原有 ultra 限制。
watch(
  () => props.modelValue?.defaultModelName || props.models[0]?.upstreamModelId,
  (model) => {
    if (
      props.modelValue?.agentType === "codex" &&
      props.modelValue.options.modelReasoningEffort === "ultra" &&
      !codexAllowsUltra(model)
    )
      updateOption("modelReasoningEffort", "low")
  }
)

function updateDefault(value) {
  if (!props.modelValue || props.disabled) return
  const next = { ...props.modelValue, defaultModelName: value }
  if (
    next.agentType === "codex" &&
    next.options.modelReasoningEffort === "ultra" &&
    !codexAllowsUltra(value || props.models[0]?.upstreamModelId)
  )
    next.options = { ...next.options, modelReasoningEffort: "low" }
  emit("update:modelValue", next)
}

function codexAllowsUltra(model) {
  // 沿用原 Provider 的规则，仅这两个模型提供 ultra 选项。
  return ["gpt-5.6-sol", "gpt-5.6-terra"].includes(
    String(model || "")
      .trim()
      .toLowerCase()
  )
}

function optionValues(field) {
  const values = [...(field.options || [])]
  if (field.key !== "modelReasoningEffort") return values
  if (
    codexAllowsUltra(
      props.modelValue.defaultModelName || props.models[0]?.upstreamModelId
    )
  )
    values.push("ultra")
  const current = props.modelValue.options[field.key]
  if (current && current !== "ultra" && !values.includes(current))
    values.push(current)
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

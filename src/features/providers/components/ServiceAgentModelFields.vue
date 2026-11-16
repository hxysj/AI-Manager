<template>
  <fieldset
    class="agent-model-fields"
    :data-agent="modelValue.agentType"
    :disabled="disabled"
  >
    <label class="model-field">
      {{ modelValue.agentType === "claude" ? "主模型" : "默认模型" }}
      <select
        class="input"
        :value="modelValue.defaultModelName"
        @change="updateDefault($event.target.value)"
      >
        <option value="">应用时选择</option>
        <option
          v-for="model in models"
          :key="model.upstreamModelId"
          :value="model.upstreamModelId"
        >
          {{ model.displayName || model.upstreamModelId }}
        </option>
      </select>
    </label>

    <template v-if="modelValue.agentType === 'codex'">
      <ProviderModelListEditor
        v-if="providerKind === 'api'"
        :model-value="modelNames"
        :sync-model-list="modelValue.options.syncModelList"
        title="服务商模型列表"
        sync-hint="使用服务商模型目录；开启后，保存并应用到 Codex 时同步。"
        empty-hint="请在服务商模型目录中配置可用模型。"
        readonly
        show-readonly-summary
        :disabled="disabled"
        @update:sync-model-list="updateOption('syncModelList', $event)"
      />
      <div class="option-grid">
        <label
          v-for="field in schema.optionFields || []"
          :key="field.key"
          class="model-field"
        >
          <template v-if="field.type === 'boolean'">
            <span class="model-check">
              <input
                type="checkbox"
                :checked="Boolean(modelValue.options[field.key])"
                @change="updateOption(field.key, $event.target.checked)"
              />{{ field.label }}
            </span>
          </template>
          <template v-else-if="field.type === 'select'">
            {{ field.label }}
            <select
              class="input"
              :value="modelValue.options[field.key] || ''"
              @change="updateOption(field.key, $event.target.value || null)"
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
          </template>
          <template v-else-if="field.type === 'number'">
            {{ field.label }}
            <input
              class="input"
              type="number"
              min="1"
              step="1"
              :value="modelValue.options[field.key]"
              :disabled="
                field.dependsOn && !modelValue.options[field.dependsOn]
              "
              @input="
                updateOption(
                  field.key,
                  $event.target.value ? Number($event.target.value) : null
                )
              "
            />
          </template>
        </label>
      </div>
    </template>

    <div v-else-if="modelValue.agentType === 'claude'" class="option-grid">
      <label v-for="field in claudeFields" :key="field.key" class="model-field">
        {{ field.label }}
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
      </label>
    </div>

    <template v-else-if="modelValue.agentType === 'claude-desktop'">
      <p class="model-hint">
        分别配置 Desktop
        的模型档位，空档位继承首个已配置模型；全部留空时使用默认模型。 1M
        能力默认随模型继承，也可单独设置。
      </p>
      <div v-for="route in desktopRoutes" :key="route.id" class="desktop-model">
        <label class="model-field">
          {{ route.label }} 模型
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
        <label class="model-check">
          <input
            type="checkbox"
            :checked="desktopSupports1m(route.id)"
            @change="
              updateDesktop(route.id, 'supports1m', $event.target.checked)
            "
          />支持 1M
        </label>
      </div>
    </template>
    <p v-else class="model-hint">
      多协议工具可指定服务商目录中的模型，未指定模型时使用此默认模型。
    </p>
  </fieldset>
</template>

<script setup>
import { computed, watch } from "vue"
import ProviderModelListEditor from "./ProviderModelListEditor.vue"

const props = defineProps({
  modelValue: { type: Object, required: true },
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
const claudeFields = computed(() => [
  ...(props.schema.modelFields || []).filter(
    (field) => field.key !== "mainModel"
  ),
  { key: "subagentModel", label: "子 Agent 模型" }
])

// 公共目录变化也可能改变默认模型，继续遵循原有 ultra 限制。
watch(
  () => props.modelValue.defaultModelName || props.models[0]?.upstreamModelId,
  (model) => {
    if (
      props.modelValue.agentType === "codex" &&
      props.modelValue.options.modelReasoningEffort === "ultra" &&
      !codexAllowsUltra(model)
    )
      updateOption("modelReasoningEffort", "low")
  }
)

function updateDefault(value) {
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
  gap: 16px;
  padding: 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);

  .model-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 8px;
    font-size: var(--font-size-sm);

    .input {
      width: 100%;
      min-width: 0;
      padding: 8px 10px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;
    }
  }

  .model-check {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: var(--font-size-sm);
  }

  .option-grid,
  .desktop-model {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 16px;

    .model-field {
      flex: 1 1 220px;
    }
  }

  .desktop-model {
    padding-top: 12px;
    border-top: 1px solid var(--color-line);
  }

  .model-hint {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
}
</style>

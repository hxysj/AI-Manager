<template>
  <div
    class="assistant-model-control"
    :class="{ 'assistant-model-compact': compact }"
    :title="settings.model || '选择模型'"
  >
    <Box :size="16" class="assistant-model-icon" />
    <el-select
      :model-value="selectedModel || undefined"
      class="assistant-model-field"
      aria-label="当前模型"
      placeholder="选择 Provider 和模型"
      filterable
      allow-create
      default-first-option
      :disabled="disabled || modelSaving"
      @update:model-value="selectModel"
    >
      <el-option-group
        v-for="group in modelGroups"
        :key="group.targetId"
        :label="group.label"
      >
        <el-option
          v-for="option in group.models"
          :key="option.value"
          :value="option.value"
          :label="option.model"
          :disabled="group.disabled"
        />
        <el-option
          v-if="!group.models.length"
          :value="group.targetId"
          label="暂无模型，请先在配置中添加"
          disabled
        />
      </el-option-group>
    </el-select>
  </div>
</template>
<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"
import { Box } from "lucide-vue-next"
import { ElOption, ElOptionGroup, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"
import { settingsApi } from "@/api"
const props = defineProps({
  settings: { type: Object, required: true },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] },
  compact: Boolean,
  disabled: Boolean
})
const emit = defineEmits(["change", "saving", "error"])
const settings = computed(() => props.settings)
const modelSaving = ref(false)
let disposed = false
let selectionVersion = 0
watch(
  [
    () => settings.value.targetId,
    () => settings.value.model,
    () => settings.value.protocol
  ],
  () => {
    selectionVersion += 1
  },
  { flush: "sync" }
)
// 选择值同时包含来源和模型，同名模型在不同 Provider 下仍可准确切换。
const modelGroups = computed(() =>
  [
    ...props.providers.map((provider) => ({
      targetId: provider.id,
      label: `${provider.name} · ${provider.cli === "claude" ? "Claude" : "Codex"} Provider`,
      disabled: provider.enabled === false,
      names: [
        provider.runtimeConfig?.mainModel,
        ...props.runtimeModels
          .filter((model) => model.providerId === provider.id)
          .map((model) => model.name || model.id)
      ]
    })),
    ...props.codexAccounts.map((account) => ({
      targetId: `account:${account.id}`,
      label: `Codex 官方 · ${account.email || account.accountId || account.id}`,
      disabled: account.disabled === true,
      names: props.runtimeModels
        .filter((model) =>
          props.providers.some(
            (provider) =>
              provider.id === model.providerId && provider.cli === "codex"
          )
        )
        .map((model) => model.name || model.id)
    }))
  ].map((group) => ({
    ...group,
    models: [
      ...new Set(
        [
          ...group.names,
          group.targetId === settings.value.targetId ? settings.value.model : ""
        ].filter(Boolean)
      )
    ].map((model) => ({
      model,
      value: JSON.stringify([group.targetId, model])
    }))
  }))
)
const selectedModel = computed(() =>
  settings.value.model
    ? JSON.stringify([settings.value.targetId, settings.value.model])
    : ""
)

// 两种输入界面共用来源与模型切换，停用来源不能通过自定义模型绕过。
async function selectModel(value) {
  if (!value?.trim() || modelSaving.value || props.disabled) return
  const selected = modelGroups.value.find((group) =>
    group.models.some((option) => option.value === value)
  )
  if (selected?.disabled) return
  const targetId = selected?.targetId || settings.value.targetId
  const model =
    selected?.models.find((option) => option.value === value)?.model ||
    value.trim()
  if (
    !selected &&
    !modelGroups.value.some(
      (group) => group.targetId === targetId && !group.disabled
    )
  ) {
    emit("error", "请先选择 Provider 分组下的模型。")
    return
  }
  if (model === settings.value.model && targetId === settings.value.targetId)
    return
  const patch =
    targetId === settings.value.targetId
      ? { model }
      : { targetId, model, protocol: "auto" }
  const version = selectionVersion
  modelSaving.value = true
  emit("saving", true)
  emit("error", "")
  try {
    await settingsApi.saveSettings({
      agents: { assistant: patch }
    })
    // 保存期间其他窗口可能更新配置，只合并本次模型选择并保留较新的选择。
    if (!disposed && version === selectionVersion)
      emit("change", { ...settings.value, ...patch })
  } catch (error) {
    if (!disposed) emit("error", error.message || String(error))
  } finally {
    modelSaving.value = false
    if (!disposed) emit("saving", false)
  }
}
onBeforeUnmount(() => {
  disposed = true
})
</script>
<style scoped lang="less">
.assistant-model-control {
  display: flex;
  align-items: center;
  min-width: 0;
  height: 36px;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: var(--color-panel);
  .assistant-model-icon {
    flex: none;
    margin-left: 10px;
    color: var(--color-text-muted);
  }
  .assistant-model-field {
    flex: 1;
    min-width: 0;
    :deep(.el-select__wrapper) {
      min-height: 34px;
      padding-left: 8px;
      border-radius: 7px;
      background: transparent;
      box-shadow: none;
      font-size: var(--font-size-sm);
    }
    :deep(.el-select__selected-item) {
      color: var(--color-text);
    }
  }
  &.assistant-model-compact {
    height: 28px;
    .assistant-model-field {
      :deep(.el-select__wrapper) {
        min-height: 26px;
        padding-right: 6px;
        font-size: var(--font-size-xs);
      }
    }
    .assistant-model-icon {
      margin-left: 7px;
    }
  }
  &:focus-within {
    border-color: var(--color-primary);
  }
}
</style>

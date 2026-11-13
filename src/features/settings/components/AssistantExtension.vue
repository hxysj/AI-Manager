<template>
  <section class="assistant-extension">
    <header class="assistant-extension-header">
      <div class="assistant-extension-icon"><Bot :size="24" /></div>
      <div class="assistant-extension-heading">
        <h3 class="assistant-extension-title">应用助手</h3>
        <p class="assistant-extension-description">
          启用后随应用启动，在桌宠旁对话、操作工具和控制桌宠。
        </p>
        <span class="assistant-extension-current"
          >当前模型：{{ modelValue.model || "未设置" }}</span
        >
      </div>
      <button
        class="assistant-extension-button"
        type="button"
        @click="expanded = !expanded"
      >
        <Settings :size="15" />配置
      </button>
      <button
        class="assistant-extension-switch"
        :class="{
          'assistant-extension-switch-active': modelValue.enabled !== false
        }"
        type="button"
        role="switch"
        aria-label="启用应用助手"
        :aria-checked="modelValue.enabled !== false"
        :disabled="pending"
        @click="$emit('save', { enabled: modelValue.enabled === false })"
      >
        <span class="assistant-extension-switch-thumb"></span>
      </button>
    </header>
    <div v-if="expanded" class="assistant-extension-config">
      <div class="assistant-extension-fields">
        <label class="assistant-extension-field">
          <span>模型来源</span>
          <el-select
            :model-value="modelValue.targetId"
            placeholder="选择 Provider 或本地模型"
            filterable
            :disabled="pending"
            @update:model-value="selectTarget"
          >
            <el-option-group
              v-for="group in targetGroups"
              :key="group.label"
              :label="group.label"
            >
              <el-option
                v-for="item in group.items"
                :key="item.id"
                :value="item.id"
                :label="item.name"
                :disabled="item.disabled"
              />
            </el-option-group>
          </el-select>
        </label>
        <label class="assistant-extension-field">
          <span>助手模型</span>
          <el-select
            :model-value="modelValue.model"
            placeholder="选择或输入模型名"
            filterable
            :allow-create="!local"
            default-first-option
            :disabled="pending || !modelValue.targetId || local"
            @update:model-value="$emit('save', { model: $event })"
          >
            <el-option
              v-for="model in modelOptions"
              :key="model"
              :value="model"
              :label="model"
            />
          </el-select>
        </label>
        <label v-if="!local" class="assistant-extension-field">
          <span>API 协议</span>
          <el-select
            :model-value="modelValue.protocol || 'auto'"
            :disabled="pending"
            @update:model-value="$emit('save', { protocol: $event })"
          >
            <el-option
              v-for="option in protocolOptions"
              :key="option.id"
              :value="option.id"
              :label="option.name"
            />
          </el-select>
        </label>
      </div>
      <p class="assistant-extension-hint">
        {{
          local
            ? "本地助手模型待添加；当前 OCR 和专用翻译模型不用于通用对话。"
            : "选择支持工具调用的模型，修改后自动保存。只在发送消息时调用模型服务。"
        }}
      </p>
      <p v-if="invalidTarget" class="assistant-extension-error">
        模型来源已删除或停用，请重新选择。
      </p>
    </div>
  </section>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import { Bot, Settings } from "lucide-vue-next"
import { ElOption, ElOptionGroup, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"

const props = defineProps({
  modelValue: { type: Object, required: true },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] },
  pending: { type: Boolean, default: false },
  openRequest: { type: Number, default: 0 }
})
const emit = defineEmits(["save"])
const expanded = ref(false)
const local = computed(() => props.modelValue.targetId === "local")
const provider = computed(() =>
  props.providers.find((item) => item.id === props.modelValue.targetId)
)
// 模型配置统一由设置保存，对话窗口和桌宠读取同一份已保存配置。
const targetGroups = computed(() => [
  ...["codex", "claude"].map((cli) => ({
    label: `${cli === "codex" ? "Codex" : "Claude"} Provider`,
    items: props.providers
      .filter((item) => item.cli === cli)
      .map((item) => ({
        id: item.id,
        name: item.name,
        disabled: item.enabled === false
      }))
  })),
  {
    label: "Codex 官方账号",
    items: props.codexAccounts.map((item) => ({
      id: `account:${item.id}`,
      name: item.email || item.accountId || item.id,
      disabled: item.disabled === true
    }))
  },
  {
    label: "本地运行",
    items: [{ id: "local", name: "本地模型（助手模型待添加）" }]
  }
])
const protocolOptions = computed(() => [
  { id: "auto", name: "自动" },
  ...(provider.value?.cli === "claude"
    ? [{ id: "messages", name: "Anthropic Messages" }]
    : [
        { id: "responses", name: "OpenAI Responses" },
        ...(!props.modelValue.targetId?.startsWith("account:")
          ? [{ id: "chat", name: "OpenAI Chat" }]
          : [])
      ])
])
const modelOptions = computed(() =>
  local.value
    ? []
    : [
        ...new Set(
          [
            provider.value?.runtimeConfig?.mainModel,
            ...props.runtimeModels
              .filter(
                (item) =>
                  item.providerId === props.modelValue.targetId ||
                  (props.modelValue.targetId?.startsWith("account:") &&
                    props.providers.some(
                      (provider) =>
                        provider.id === item.providerId &&
                        provider.cli === "codex"
                    ))
              )
              .map((item) => item.name || item.id),
            props.modelValue.model
          ].filter(Boolean)
        )
      ]
)
const invalidTarget = computed(
  () =>
    props.modelValue.targetId &&
    !targetGroups.value.some((group) =>
      group.items.some(
        (item) => item.id === props.modelValue.targetId && !item.disabled
      )
    )
)
function selectTarget(targetId) {
  emit("save", {
    targetId,
    model:
      props.providers.find((item) => item.id === targetId)?.runtimeConfig
        ?.mainModel || "",
    protocol: "auto"
  })
}
watch(
  () => props.openRequest,
  (value) => {
    if (value) expanded.value = true
  },
  { immediate: true }
)
</script>

<style scoped lang="less">
.assistant-extension {
  border: 1px solid var(--color-line);
  border-radius: 16px;
  background: var(--color-panel);
  .assistant-extension-header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 22px 28px;
    .assistant-extension-icon {
      display: flex;
      color: var(--color-primary);
    }
    .assistant-extension-heading {
      flex: 1;
      min-width: 0;
      .assistant-extension-title {
        margin: 0 0 6px;
        font-size: 16px;
      }
      .assistant-extension-description {
        margin: 0 0 8px;
        color: var(--color-text-muted);
        font-size: 12px;
        line-height: 1.6;
      }
      .assistant-extension-current {
        color: var(--color-primary);
        font-size: 12px;
        overflow-wrap: anywhere;
      }
    }
    .assistant-extension-button {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 7px 10px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
    }
    .assistant-extension-switch {
      display: flex;
      align-items: center;
      flex: none;
      width: 38px;
      height: 22px;
      padding: 3px;
      border: 0;
      border-radius: 12px;
      background: var(--color-line);
      cursor: pointer;
      .assistant-extension-switch-thumb {
        width: 16px;
        height: 16px;
        border-radius: 50%;
        background: #fff;
        transition: transform 0.2s;
      }
      &.assistant-extension-switch-active {
        background: var(--color-primary);
        .assistant-extension-switch-thumb {
          transform: translateX(16px);
        }
      }
      &:disabled {
        opacity: 0.5;
        cursor: default;
      }
    }
  }
  .assistant-extension-config {
    padding: 20px 28px;
    border-top: 1px solid var(--color-line);
    .assistant-extension-fields {
      display: flex;
      gap: 14px;
      .assistant-extension-field {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        gap: 8px;
        font-size: 12px;
        color: var(--color-text-muted);
        :deep(.el-select) {
          width: 100%;
        }
      }
    }
    .assistant-extension-hint {
      margin: 14px 0 0;
      color: var(--color-text-muted);
      font-size: 12px;
      line-height: 1.6;
    }
    .assistant-extension-error {
      color: var(--color-danger);
      font-size: 12px;
    }
  }
}
</style>

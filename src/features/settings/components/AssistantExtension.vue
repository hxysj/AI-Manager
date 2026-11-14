<template>
  <section class="assistant-extension">
    <article class="assistant-extension-card">
      <div class="assistant-extension-main">
        <div class="assistant-extension-icon"><Bot :size="24" /></div>
        <div class="assistant-extension-copy">
          <div class="assistant-extension-heading">
            <h3 class="assistant-extension-title">应用助手</h3>
            <span class="assistant-extension-status">
              {{ modelValue.enabled !== false ? "已启动" : "已关闭" }}
            </span>
          </div>
          <p class="assistant-extension-description">
            启用后随应用启动，按快捷键在鼠标旁输入，由桌宠回复。
          </p>
          <span class="assistant-extension-current"
            >当前模型：{{ modelValue.model || "未设置" }}</span
          >
        </div>
      </div>
      <div class="assistant-extension-actions" @click.stop @keydown.stop>
        <button
          class="assistant-extension-button"
          type="button"
          @click="showConfig = true"
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
      </div>
    </article>
    <p
      v-if="desktopStatus.error"
      class="assistant-extension-error"
      role="alert"
    >
      {{ desktopStatus.error }}
    </p>
    <BaseModal
      v-if="showConfig"
      class="assistant-extension-modal"
      title="应用助手配置"
      description="配置模型来源、快捷键、上下文压缩和公共记忆。"
      @close="showConfig = false"
    >
      <div class="assistant-extension-config">
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
        <label class="assistant-extension-shortcut">
          <span>唤出输入框快捷键</span>
          <input
            class="assistant-extension-shortcut-input"
            aria-label="唤出输入框快捷键"
            :value="modelValue.shortcut || 'Ctrl+Alt+Space'"
            readonly
            :disabled="pending"
            @keydown="recordShortcut"
            @keyup="$event.key === 'PrintScreen' && recordShortcut($event)"
          />
          <span class="assistant-extension-shortcut-hint">
            点击后按下新的组合键
          </span>
        </label>
        <p
          v-if="shortcutError || desktopStatus.error"
          class="assistant-extension-error"
          role="alert"
        >
          {{ shortcutError || desktopStatus.error }}
        </p>
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
        <div class="assistant-extension-context">
          <label class="assistant-context-toggle">
            <input
              type="checkbox"
              :checked="modelValue.autoCompress !== false"
              :disabled="pending"
              @change="$emit('save', { autoCompress: $event.target.checked })"
            />
            自动压缩上下文
          </label>
          <label class="assistant-context-toggle">
            <input
              type="checkbox"
              :checked="modelValue.sharedMemoryEnabled !== false"
              :disabled="pending"
              @change="
                $emit('save', { sharedMemoryEnabled: $event.target.checked })
              "
            />
            启用跨会话公共记忆
          </label>
          <div class="assistant-context-fields">
            <label class="assistant-context-field">
              <span>模型上下文容量（Token）</span>
              <input
                aria-label="模型上下文容量"
                type="number"
                min="4096"
                max="2000000"
                step="1"
                :value="modelValue.contextWindow || 128000"
                :disabled="pending"
                @change="saveContextNumber($event, 'contextWindow')"
              />
            </label>
            <label class="assistant-context-field">
              <span>自动压缩阈值（%）</span>
              <input
                aria-label="自动压缩阈值"
                type="number"
                min="30"
                max="95"
                step="1"
                :value="modelValue.compressionThreshold || 75"
                :disabled="pending"
                @change="saveContextNumber($event, 'compressionThreshold')"
              />
            </label>
          </div>
          <label class="assistant-context-prompt">
            <span>摘要补充要求（可选）</span>
            <textarea
              aria-label="摘要补充要求"
              rows="3"
              maxlength="16000"
              placeholder="例如：优先保留项目路径、用户约束和未完成事项"
              :value="modelValue.compressionPrompt || ''"
              :disabled="pending"
              @change="
                $emit('save', { compressionPrompt: $event.target.value.trim() })
              "
            ></textarea>
          </label>
          <p class="assistant-extension-hint">
            请按实际模型容量设置。压缩复用当前 Provider
            和模型，保留本地原始历史；公共记忆可在应用助手中编辑，关闭后保留已有内容。
          </p>
        </div>
      </div>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"
import { Bot, Settings } from "lucide-vue-next"
import { ElOption, ElOptionGroup, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"
import BaseModal from "@/components/BaseModal.vue"
import { assistantApi } from "@/api"
import { shortcutFromEvent } from "@/utils/shortcuts"

const props = defineProps({
  modelValue: { type: Object, required: true },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] },
  pending: { type: Boolean, default: false },
  openRequest: { type: Number, default: 0 }
})
const emit = defineEmits(["save"])
// 数字设置通过浏览器校验后按字段保存，避免覆盖正在编辑的其他设置。
function saveContextNumber(event, key) {
  if (!event.target.reportValidity()) return
  emit("save", { [key]: Number(event.target.value) })
}
const showConfig = ref(false)
const shortcutError = ref("")
const desktopStatus = ref({})
let unsubscribeStatus
let disposed = false
async function loadDesktopStatus() {
  try {
    const status = await assistantApi.desktopStatus()
    if (!disposed) desktopStatus.value = status
  } catch {
    // 原生状态不影响模型配置的编辑和保存。
  }
}
function recordShortcut(event) {
  try {
    const shortcut = shortcutFromEvent(event)
    if (!shortcut) return
    shortcutError.value = ""
    emit("save", { shortcut })
  } catch (error) {
    shortcutError.value = error.message
  }
}
onMounted(async () => {
  try {
    const unsubscribe = await assistantApi.onDesktopStatus(loadDesktopStatus)
    if (disposed) unsubscribe()
    else {
      unsubscribeStatus = unsubscribe
      await loadDesktopStatus()
    }
  } catch {
    // 无原生桥接时仍可查看设置。
  }
})
onBeforeUnmount(() => {
  disposed = true
  unsubscribeStatus?.()
})
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
    if (value) showConfig.value = true
  },
  { immediate: true }
)
</script>

<style scoped lang="less">
.assistant-extension {
  .assistant-extension-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 22px 28px;
    border: 1px solid var(--color-line);
    border-radius: 16px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);

    .assistant-extension-main {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      gap: 18px;

      .assistant-extension-icon {
        display: flex;
        width: 52px;
        height: 52px;
        flex: none;
        align-items: center;
        justify-content: center;
        border-radius: 14px;
        background: #eff6ff;
        color: #2563eb;
      }

      .assistant-extension-copy {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 4px;

        .assistant-extension-heading {
          display: flex;
          align-items: center;
          gap: 10px;

          .assistant-extension-title {
            margin: 0;
            color: var(--color-text);
            font-size: var(--font-size-base);
          }

          .assistant-extension-status {
            color: var(--color-text-muted);
            font-size: 12px;
          }
        }

        .assistant-extension-description {
          margin: 0;
          color: var(--color-text-muted);
          font-size: 13px;
          line-height: 1.5;
        }

        .assistant-extension-current {
          color: var(--color-primary);
          font-size: 12px;
          overflow-wrap: anywhere;
        }
      }
    }

    .assistant-extension-actions {
      display: flex;
      min-height: 52px;
      flex: none;
      align-items: center;
      gap: 10px;
      padding-left: 24px;
      border-left: 1px solid var(--color-line);

      .assistant-extension-button {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 6px 10px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 13px;
        cursor: pointer;

        &:hover {
          background: var(--color-panel-soft);
        }

        &:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }
      }

      .assistant-extension-switch {
        display: flex;
        width: 44px;
        height: 24px;
        flex: none;
        align-items: center;
        padding: 2px;
        border: none;
        border-radius: 12px;
        background: #cbd5e1;
        cursor: pointer;
        transition: background 0.2s ease;

        .assistant-extension-switch-thumb {
          display: block;
          width: 20px;
          height: 20px;
          border-radius: 50%;
          background: #fff;
          box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
          transition: transform 0.2s ease;
        }

        &.assistant-extension-switch-active {
          background: #2563eb;

          .assistant-extension-switch-thumb {
            transform: translateX(20px);
          }
        }

        &:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }
      }
    }
  }

  .assistant-extension-error {
    margin: 8px 0 0;
    color: var(--color-danger);
    font-size: 13px;
    line-height: 1.5;
  }

  .assistant-extension-modal {
    :deep(.base-modal__panel) {
      width: 640px;
      max-width: calc(100vw - 48px);
      border-radius: 12px;
    }

    :deep(.base-modal__content) {
      padding: 0;
      overflow: visible;
    }

    .assistant-extension-config {
      display: flex;
      max-height: calc(88vh - 96px);
      flex-direction: column;
      gap: 14px;
      overflow-y: auto;
      padding: 16px 24px 20px;

      .assistant-extension-fields {
        display: flex;
        gap: 12px;

        .assistant-extension-field {
          display: flex;
          min-width: 0;
          flex: 1;
          flex-direction: column;
          gap: 6px;
          color: var(--color-text-muted);
          font-size: 12px;

          :deep(.el-select) {
            width: 100%;
          }
        }
      }

      .assistant-extension-shortcut {
        display: flex;
        align-items: center;
        gap: 12px;
        color: var(--color-text-muted);
        font-size: 12px;

        .assistant-extension-shortcut-input {
          width: 180px;
          box-sizing: border-box;
          padding: 8px 10px;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          outline: none;
          background: var(--color-panel-soft);
          color: var(--color-text);
          font: inherit;
          cursor: pointer;

          &:focus {
            border-color: var(--color-primary);
          }
        }

        .assistant-extension-shortcut-hint {
          color: var(--color-text-muted);
          font-size: 11px;
        }
      }

      .assistant-extension-hint {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 12px;
        line-height: 1.6;
      }

      .assistant-extension-context {
        display: flex;
        flex-direction: column;
        gap: 12px;
        padding-top: 14px;
        border-top: 1px solid var(--color-line);
        color: var(--color-text-muted);
        font-size: 12px;

        .assistant-context-toggle {
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .assistant-context-fields {
          display: flex;
          gap: 12px;

          .assistant-context-field {
            display: flex;
            min-width: 0;
            flex: 1;
            flex-direction: column;
            gap: 6px;

            :deep(input) {
              width: 100%;
              box-sizing: border-box;
              padding: 8px 10px;
              border: 1px solid var(--color-line);
              border-radius: 7px;
              background: var(--color-panel-soft);
              color: var(--color-text);
              font: inherit;
            }
          }
        }

        .assistant-context-prompt {
          display: flex;
          flex-direction: column;
          gap: 6px;

          :deep(textarea) {
            min-height: 70px;
            resize: vertical;
            padding: 8px 10px;
            border: 1px solid var(--color-line);
            border-radius: 7px;
            background: var(--color-panel-soft);
            color: var(--color-text);
            font: inherit;
          }
        }
      }
    }
  }
}
</style>

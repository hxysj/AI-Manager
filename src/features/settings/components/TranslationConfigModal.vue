<template>
  <BaseModal
    class="translation-config-modal"
    role="dialog"
    aria-modal="true"
    @close="$emit('close')"
  >
    <template #header>
      <div class="translation-modal-header">
        <div class="translation-modal-title">
          <div class="translation-modal-icon">
            <Settings :size="18" />
          </div>
          <div>
            <h3>划词翻译配置</h3>
            <p>配置用于划词翻译的模型、账号及目标语言</p>
          </div>
        </div>
        <button
          class="translation-modal-close"
          type="button"
          aria-label="关闭"
          @click="$emit('close')"
        >
          <X :size="18" />
        </button>
      </div>
    </template>

    <div class="translation-config-body">
      <div class="translation-fields">
        <label class="translation-field">
          <span>Provider / 官方账号</span>
          <el-select
            :model-value="modelValue.targetId"
            placeholder="请选择 Codex Provider 或账号"
            filterable
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
                :label="item.name"
                :value="item.id"
                :disabled="item.disabled"
              />
            </el-option-group>
          </el-select>
        </label>
        <label class="translation-field">
          <span>翻译模型</span>
          <el-select
            :model-value="modelValue.model"
            placeholder="选择或输入模型名称"
            filterable
            allow-create
            default-first-option
            :disabled="!modelValue.targetId"
            @update:model-value="update('model', $event)"
          >
            <el-option
              v-for="model in modelOptions"
              :key="model"
              :label="model"
              :value="model"
            />
          </el-select>
        </label>
        <label class="translation-field">
          <span>目标语言</span>
          <el-select
            :model-value="modelValue.targetLanguage"
            @update:model-value="update('targetLanguage', $event)"
          >
            <el-option
              v-for="language in languages"
              :key="language"
              :label="language"
              :value="language"
            />
          </el-select>
        </label>
      </div>

      <div class="translation-notices">
        <p class="translation-hint">
          目前仅支持 Codex 模式的 Provider 和官方账号。{{
            official
              ? "官方账号翻译需要已安装 Codex CLI。"
              : "Provider 翻译需要已安装 Node.js 20 或更高版本。"
          }}
        </p>
        <p v-if="invalidTarget" class="translation-error">
          所选 Provider / 账号已删除或禁用，请重新选择。
        </p>
        <p class="translation-hint">
          翻译固定使用低推理强度（low）。Windows
          桌面取词仅在按快捷键时读取选区； 不支持取词的应用可先复制，再按
          Ctrl+Alt+Shift+T 翻译剪贴板。
          关闭翻译会注销全局快捷键，关闭结果窗口会释放窗口资源。
        </p>
        <p v-if="desktopStatus.error" class="translation-error">
          {{ desktopStatus.error }}
        </p>
      </div>

      <footer class="translation-config-footer">
        <button
          class="translation-primary-btn"
          type="button"
          @click="$emit('close')"
        >
          确定
        </button>
      </footer>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import { Settings, X } from "lucide-vue-next"
import { translationApi } from "@/api"
import { ElOption, ElOptionGroup, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({
  modelValue: { type: Object, required: true },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] }
})

const emit = defineEmits(["update:modelValue", "close"])

const languages = [
  "简体中文",
  "繁體中文",
  "English",
  "日本語",
  "한국어",
  "Français",
  "Deutsch",
  "Español"
]

const desktopStatus = ref({ error: "" })
let unsubscribeDesktopStatus = null

async function loadDesktopStatus() {
  try {
    desktopStatus.value = await translationApi.desktopStatus()
  } catch (error) {
    desktopStatus.value = { error: error.message || String(error) }
  }
}

const targetGroups = computed(() => [
  {
    label: "Codex Provider",
    items: props.providers
      .filter((item) => item.cli === "codex")
      .map((item) => ({
        id: item.id,
        name: item.name,
        disabled: item.enabled === false
      }))
  },
  {
    label: "Codex 官方账号",
    items: props.codexAccounts.map((item) => ({
      id: `account:${item.id}`,
      name: item.email || item.accountId || item.id,
      disabled: item.disabled === true
    }))
  }
])

const official = computed(() =>
  Boolean(props.modelValue.targetId?.startsWith("account:"))
)

const selectedProvider = computed(() =>
  props.providers.find(
    (item) => item.id === props.modelValue.targetId && item.cli === "codex"
  )
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

const modelOptions = computed(() => [
  ...new Set(
    [
      selectedProvider.value?.runtimeConfig?.mainModel,
      ...props.runtimeModels
        .filter(
          (item) =>
            item.providerId === props.modelValue.targetId ||
            (official.value &&
              props.providers.some(
                (provider) =>
                  provider.id === item.providerId && provider.cli === "codex"
              ))
        )
        .map((item) => item.name || item.id),
      props.modelValue.model
    ].filter(Boolean)
  )
])

function update(key, value) {
  emit("update:modelValue", { ...props.modelValue, [key]: value })
}

function selectTarget(targetId) {
  const provider = props.providers.find((item) => item.id === targetId)
  const model = provider?.runtimeConfig?.mainModel || ""
  emit("update:modelValue", { ...props.modelValue, targetId, model })
}

onMounted(() => {
  loadDesktopStatus()
  unsubscribeDesktopStatus = translationApi.onDesktopStatus(loadDesktopStatus)
})

onBeforeUnmount(() => {
  unsubscribeDesktopStatus?.()
})
</script>

<style scoped lang="less">
.translation-config-modal {
  :deep(.base-modal__panel) {
    width: 620px;
    max-width: calc(100vw - 32px);
    border-radius: 12px;
  }

  .translation-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 18px 24px 14px;
    border-bottom: 1px solid var(--color-line);

    .translation-modal-title {
      display: flex;
      align-items: center;
      gap: 12px;

      .translation-modal-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
        border-radius: 8px;
        background: #eff6ff;
        color: #2563eb;
      }

      h3 {
        margin: 0;
        font-size: 17px;
        font-weight: 600;
        color: var(--color-text);
      }

      p {
        margin: 2px 0 0;
        font-size: 13px;
        color: var(--color-text-muted);
      }
    }

    .translation-modal-close {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 32px;
      height: 32px;
      border: none;
      border-radius: 6px;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      transition:
        background 0.15s ease,
        color 0.15s ease;

      &:hover {
        background: var(--color-panel-soft);
        color: var(--color-text);
      }
    }
  }

  .translation-config-body {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 20px 24px 24px;
    max-height: calc(85vh - 100px);
    overflow-y: auto;

    .translation-fields {
      display: flex;
      flex-direction: column;
      gap: 16px;

      .translation-field {
        display: flex;
        flex-direction: column;
        gap: 8px;

        span {
          font-size: 14px;
          font-weight: 500;
          color: var(--color-text);
        }

        :deep(.el-select) {
          width: 100%;
        }
      }
    }

    .translation-notices {
      display: flex;
      flex-direction: column;
      gap: 10px;
      padding: 12px 14px;
      border-radius: 8px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);

      .translation-hint {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 12.5px;
        line-height: 1.6;
      }

      .translation-error {
        margin: 0;
        color: var(--color-danger);
        font-size: 12.5px;
        line-height: 1.6;
      }
    }

    .translation-config-footer {
      display: flex;
      justify-content: flex-end;
      padding-top: 8px;

      .translation-primary-btn {
        padding: 8px 24px;
        border: none;
        border-radius: 8px;
        background: #2563eb;
        color: #ffffff;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        transition: background 0.15s ease;

        &:hover {
          background: #1d4ed8;
        }
      }
    }
  }
}
</style>

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
            <p>配置用于划词翻译的模型、账号、快捷键及目标语言</p>
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
      <div class="translation-section">
        <div class="translation-fields">
          <label class="translation-field">
            <span class="translation-field-label">Provider / 官方账号</span>
            <el-select
              :model-value="modelValue.targetId"
              placeholder="请选择 Codex Provider 或账号"
              filterable
              popper-class="translation-select-popper"
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
                />
              </el-option-group>
            </el-select>
          </label>

          <div class="translation-field-row">
            <label class="translation-field">
              <span class="translation-field-label">翻译模型</span>
              <el-select
                :model-value="modelValue.model"
                placeholder="选择或输入模型名称"
                filterable
                allow-create
                default-first-option
                popper-class="translation-select-popper"
                :disabled="!modelValue.targetId"
                @update:model-value="update({ model: $event })"
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
              <span class="translation-field-label">目标语言</span>
              <el-select
                :model-value="modelValue.targetLanguage"
                popper-class="translation-select-popper"
                @update:model-value="update({ targetLanguage: $event })"
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
        </div>
      </div>

      <div class="translation-section">
        <div class="translation-section-header">
          <span class="translation-section-title">快捷键配置</span>
          <button
            class="translation-reset-btn"
            type="button"
            title="恢复默认快捷键"
            @click="resetShortcuts"
          >
            <RotateCcw :size="12" />
            <span>恢复默认</span>
          </button>
        </div>
        <div class="translation-field-row">
          <label class="translation-field">
            <span class="translation-field-label-wrap">
              <span>划词翻译快捷键</span>
              <span class="translation-field-sub">选中文本触发</span>
            </span>
            <div class="translation-shortcut-wrap">
              <input
                class="translation-shortcut-input"
                :value="modelValue.shortcut || 'Ctrl+Alt+T'"
                placeholder="点击后按下快捷键"
                readonly
                @keydown="recordShortcut($event, 'shortcut')"
                @keyup="
                  $event.key === 'PrintScreen' &&
                  recordShortcut($event, 'shortcut')
                "
              />
              <kbd class="translation-shortcut-kbd">{{
                modelValue.shortcut || "Ctrl+Alt+T"
              }}</kbd>
            </div>
          </label>
          <label class="translation-field">
            <span class="translation-field-label-wrap">
              <span>剪贴板翻译快捷键</span>
              <span class="translation-field-sub">复制后直接翻译</span>
            </span>
            <div class="translation-shortcut-wrap">
              <input
                class="translation-shortcut-input"
                :value="modelValue.clipboardShortcut || 'Ctrl+Alt+Shift+T'"
                placeholder="点击后按下快捷键"
                readonly
                @keydown="recordShortcut($event, 'clipboardShortcut')"
                @keyup="
                  $event.key === 'PrintScreen' &&
                  recordShortcut($event, 'clipboardShortcut')
                "
              />
              <kbd class="translation-shortcut-kbd">{{
                modelValue.clipboardShortcut || "Ctrl+Alt+Shift+T"
              }}</kbd>
            </div>
          </label>
        </div>
        <p v-if="shortcutError" class="translation-field-error">
          {{ shortcutError }}
        </p>
      </div>

      <div class="translation-notices">
        <p v-if="invalidTarget" class="translation-error">
          所选 Provider / 账号已删除或禁用，请重新选择。
        </p>
        <p v-if="desktopStatus.error" class="translation-error">
          {{ desktopStatus.error }}
        </p>
        <p class="translation-hint">
          目前仅支持 Codex 模式。 按快捷键划词取词，或先复制再按
          <strong>{{
            modelValue.clipboardShortcut || "Ctrl+Alt+Shift+T"
          }}</strong>
          翻译剪贴板。
        </p>
      </div>

      <footer class="translation-config-footer">
        <button
          class="translation-cancel-btn"
          type="button"
          @click="$emit('close')"
        >
          关闭
        </button>
      </footer>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import { RotateCcw, Settings, X } from "lucide-vue-next"
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

const emit = defineEmits(["update:modelValue", "save", "close"])

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
const shortcutError = ref("")
let unsubscribeDesktopStatus = null

async function loadDesktopStatus() {
  try {
    desktopStatus.value = await translationApi.desktopStatus()
  } catch (error) {
    desktopStatus.value = { error: error.message || String(error) }
  }
}

const targetGroups = computed(() =>
  [
    {
      label: "Codex Provider",
      items: props.providers
        .filter((item) => item.cli === "codex" && item.enabled !== false)
        .map((item) => ({
          id: item.id,
          name: item.name
        }))
    },
    {
      label: "Codex 官方账号",
      items: props.codexAccounts
        .filter((item) => item.disabled !== true)
        .map((item) => ({
          id: `account:${item.id}`,
          name: item.email || item.accountId || item.id
        }))
    }
  ].filter((group) => group.items.length > 0)
)

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
      group.items.some((item) => item.id === props.modelValue.targetId)
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

function update(changes) {
  const next = { ...props.modelValue, ...changes }
  // 快捷键先校验再单独保存，普通配置修改不提交其他功能的草稿。
  if (
    ("shortcut" in changes || "clipboardShortcut" in changes) &&
    next.shortcut === next.clipboardShortcut
  ) {
    shortcutError.value = "划词翻译与剪贴板翻译快捷键不能相同"
    return
  }
  shortcutError.value = ""
  emit("update:modelValue", next)
  emit("save", changes)
}

function selectTarget(targetId) {
  const provider = props.providers.find((item) => item.id === targetId)
  const model = provider?.runtimeConfig?.mainModel || props.modelValue.model
  update({ targetId, model })
}

function recordShortcut(event, field) {
  if (event.key === "Tab") return
  event.preventDefault()
  if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return

  const key =
    event.code === "Space"
      ? "Space"
      : event.key.length === 1
        ? event.key.toUpperCase()
        : event.key

  if (!/^[A-Z0-9]$|^F([1-9]|1[0-9]|2[0-4])$|^(PrintScreen|Space)$/.test(key)) {
    shortcutError.value = "请使用字母、数字、F1–F24、PrintScreen 或空格"
    return
  }

  if (
    !event.ctrlKey &&
    !event.altKey &&
    !event.shiftKey &&
    !event.metaKey &&
    !/^F\d+$|^PrintScreen$/.test(key)
  ) {
    shortcutError.value = "请同时按下 Ctrl、Alt、Shift 或 Win 修饰键"
    return
  }

  const combination = [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Win",
    key
  ]
    .filter(Boolean)
    .join("+")

  shortcutError.value = ""
  update({ [field]: combination })
}

function resetShortcuts() {
  shortcutError.value = ""
  update({
    shortcut: "Ctrl+Alt+T",
    clipboardShortcut: "Ctrl+Alt+Shift+T"
  })
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
    width: 600px;
    max-width: calc(100vw - 32px);
    border-radius: 12px;
    overflow: hidden;
  }

  :deep(.base-modal__header) {
    padding: 0;
    border-bottom: none;
  }

  :deep(.base-modal__content) {
    padding: 0;
    overflow: visible;
  }

  .translation-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 16px 22px 14px;
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
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text);
      }

      p {
        margin: 2px 0 0;
        font-size: 12.5px;
        color: var(--color-text-muted);
      }
    }

    .translation-modal-close {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 30px;
      height: 30px;
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
    gap: 14px;
    padding: 16px 22px 18px;
    max-height: calc(88vh - 70px);
    overflow-y: auto;
    overflow-x: hidden;

    &::-webkit-scrollbar {
      width: 6px;
    }
    &::-webkit-scrollbar-track {
      background: transparent;
    }
    &::-webkit-scrollbar-thumb {
      background: var(--color-line);
      border-radius: 3px;
      &:hover {
        background: var(--color-line-strong);
      }
    }
    scrollbar-width: thin;
    scrollbar-color: var(--color-line) transparent;

    .translation-section {
      display: flex;
      flex-direction: column;
      gap: 8px;

      .translation-section-header {
        display: flex;
        align-items: center;
        justify-content: space-between;

        .translation-section-title {
          font-size: 13px;
          font-weight: 600;
          color: var(--color-text);
        }

        .translation-reset-btn {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 3px 8px;
          border: 1px solid var(--color-line);
          border-radius: 5px;
          background: transparent;
          color: var(--color-text-muted);
          font-size: 11.5px;
          cursor: pointer;
          transition: all 0.15s ease;

          &:hover {
            border-color: var(--color-line-strong);
            color: var(--color-text);
            background: var(--color-panel-soft);
          }
        }
      }
    }

    .translation-fields {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }

    .translation-field-row {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 12px;

      @media (max-width: 520px) {
        grid-template-columns: 1fr;
      }
    }

    .translation-field {
      display: flex;
      flex-direction: column;
      gap: 5px;

      .translation-field-label,
      span {
        font-size: 13px;
        font-weight: 500;
        color: var(--color-text);
      }

      .translation-field-label-wrap {
        display: flex;
        align-items: baseline;
        justify-content: space-between;

        .translation-field-sub {
          font-size: 11px;
          font-weight: 400;
          color: var(--color-text-muted);
        }
      }

      :deep(.el-select) {
        width: 100%;
        --el-select-input-focus-border-color: #2563eb;
        --el-input-focus-border-color: #2563eb;

        .el-select__wrapper {
          min-height: 36px;
          border-radius: 7px;
          background: var(--color-panel);
          box-shadow: 0 0 0 1px var(--color-line) inset;
          transition:
            box-shadow 0.15s ease,
            background 0.15s ease;

          &:hover {
            box-shadow: 0 0 0 1px var(--color-line-strong) inset;
          }

          &.is-focused {
            box-shadow: 0 0 0 1.5px #2563eb inset !important;
          }

          .el-select__placeholder {
            color: var(--color-text-muted);
            font-size: 13px;
          }

          .el-select__selected-item {
            color: var(--color-text);
            font-size: 13px;
          }

          input,
          .el-select__input,
          .el-select__input-wrapper {
            border: none !important;
            outline: none !important;
            box-shadow: none !important;
            background: transparent !important;
            height: auto !important;
            padding: 0 !important;
            margin: 0 !important;
            color: inherit !important;
          }
        }
      }

      .translation-shortcut-wrap {
        position: relative;
        display: flex;
        align-items: center;

        .translation-shortcut-input {
          width: 100%;
          height: 36px;
          padding: 0 10px;
          padding-right: 85px;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          background: var(--color-panel);
          color: var(--color-text);
          font-size: 13px;
          cursor: pointer;
          transition:
            border-color 0.15s ease,
            box-shadow 0.15s ease;

          &:hover {
            border-color: var(--color-line-strong);
          }

          &:focus {
            outline: none;
            border-color: #2563eb;
            box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.15);
          }
        }

        .translation-shortcut-kbd {
          position: absolute;
          right: 7px;
          display: inline-flex;
          align-items: center;
          padding: 2px 6px;
          border: 1px solid var(--color-line);
          border-radius: 4px;
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11px;
          font-weight: 500;
          pointer-events: none;
        }
      }
    }

    .translation-field-error {
      margin: 0;
      color: var(--color-danger);
      font-size: 12px;
    }

    .translation-notices {
      display: flex;
      flex-direction: column;
      gap: 6px;
      padding: 10px 12px;
      border-radius: 7px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);

      .translation-hint {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 12px;
        line-height: 1.5;

        strong {
          color: var(--color-text);
        }
      }

      .translation-error {
        margin: 0;
        color: var(--color-danger);
        font-size: 12px;
        line-height: 1.5;
      }
    }

    .translation-config-footer {
      display: flex;
      justify-content: flex-end;
      align-items: center;
      gap: 10px;
      padding-top: 4px;

      .translation-cancel-btn,
      .translation-secondary-btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        height: 34px;
        padding: 0 18px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 13px;
        cursor: pointer;
        transition: all 0.15s ease;

        &:hover {
          background: var(--color-panel-soft);
          border-color: var(--color-line-strong);
        }

        &:active {
          transform: translateY(1px);
        }
      }
    }
  }
}
</style>

<!-- 下拉弹出层挂载到 body，用专属类名限定美化样式 -->
<style lang="less">
.translation-select-popper.el-popper {
  border-radius: 10px !important;
  border: 1px solid var(--color-line) !important;
  background: var(--color-panel) !important;
  box-shadow:
    0 10px 25px -5px rgba(0, 0, 0, 0.15),
    0 8px 10px -6px rgba(0, 0, 0, 0.1) !important;
  --el-bg-color-overlay: var(--color-panel);
  --el-border-color-light: var(--color-line);
  --el-text-color-regular: var(--color-text);
  --el-text-color-primary: var(--color-text);
  --el-text-color-placeholder: var(--color-text-muted);

  .el-popper__arrow {
    display: none;
  }

  .el-select-dropdown {
    padding: 4px 0;
  }

  .el-select-group__wrap:not(:last-child) {
    border-bottom: 1px solid var(--color-line);
    padding-bottom: 4px;
    margin-bottom: 4px;
  }

  .el-select-group__title {
    padding: 8px 14px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.05em;
  }

  .el-select-dropdown__item {
    height: 34px;
    line-height: 34px;
    margin: 2px 6px;
    padding: 0 12px;
    border-radius: 6px;
    font-size: 13px;
    color: var(--color-text);
    transition:
      background-color 0.12s ease,
      color 0.12s ease;

    &.is-hovering:not(.is-disabled):not(.is-selected) {
      background: var(--color-panel-soft);
      color: var(--color-text);
    }

    &.is-selected {
      background: #eff6ff;
      color: #2563eb;
      font-weight: 600;
    }

    &.is-disabled {
      opacity: 0.45;
      cursor: not-allowed;
    }
  }

  .el-select-dropdown__empty {
    padding: 16px;
    color: var(--color-text-muted);
    font-size: 13px;
  }
}

html[data-theme="dark"] .translation-select-popper.el-popper {
  .el-select-dropdown__item.is-selected {
    background: rgba(37, 99, 235, 0.2);
    color: #60a5fa;
  }
}
</style>

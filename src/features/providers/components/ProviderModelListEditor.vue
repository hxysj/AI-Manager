<template>
  <section class="provider-model-editor">
    <header v-if="!readonly" class="provider-model-toolbar">
      <div class="provider-model-heading">
        <span class="provider-model-title">模型列表</span>
        <span class="provider-model-count">{{ models.length }} 个</span>
      </div>
      <div class="provider-model-actions">
        <button
          class="provider-model-button"
          type="button"
          :disabled="disabled || loading"
          :aria-pressed="bulkEditing"
          @click="bulkEditing = !bulkEditing"
        >
          <Check v-if="bulkEditing" :size="14" />
          <SquarePen v-else :size="14" />
          {{ bulkEditing ? "完成编辑" : "批量编辑" }}
        </button>
        <button
          class="provider-model-button provider-model-fetch"
          type="button"
          :disabled="disabled || loading"
          :aria-busy="loading"
          @click="emit('fetch')"
        >
          <RefreshCw
            class="provider-model-fetch-icon"
            :class="{ 'provider-model-spinning': loading }"
            :size="14"
          />
          {{ loading ? "获取中…" : "获取上游模型" }}
        </button>
        <button
          class="provider-model-button"
          type="button"
          :disabled="disabled || loading"
          @click="resetModels"
        >
          <RotateCcw :size="14" />
          恢复默认模型
        </button>
      </div>
    </header>

    <div class="provider-model-surface">
      <textarea
        v-if="!readonly && bulkEditing"
        class="provider-model-bulk"
        :value="modelValue"
        :disabled="disabled || loading"
        aria-label="批量编辑模型列表"
        placeholder="每行一个模型，也支持逗号或分号分隔"
        spellcheck="false"
        @input="emit('update:modelValue', $event.target.value)"
      ></textarea>
      <el-scrollbar
        v-else
        class="provider-model-scroll"
        :max-height="maxHeight"
        :aria-label="readonly ? '上游模型列表' : '已配置的模型'"
        tabindex="0"
      >
        <ul v-if="models.length" class="provider-model-list">
          <li v-for="model in models" :key="model" class="provider-model-item">
            <Box class="provider-model-icon" :size="14" />
            <span class="provider-model-name" :title="model">{{ model }}</span>
            <button
              v-if="!readonly"
              class="provider-model-remove"
              type="button"
              :disabled="disabled || loading"
              :aria-label="`移除模型 ${model}`"
              :title="`移除 ${model}`"
              @click="removeModel(model)"
            >
              <X :size="13" />
            </button>
          </li>
        </ul>
        <div v-else class="provider-model-empty" role="status">
          <Box :size="22" />
          <span class="provider-model-empty-title">{{
            readonly ? "暂无上游模型" : "暂无自定义模型"
          }}</span>
          <span>{{
            readonly
              ? "请先点击“获取上游模型”。"
              : "获取上游模型，或在下方手动添加"
          }}</span>
        </div>
      </el-scrollbar>

      <div v-if="!readonly && !bulkEditing" class="provider-model-add">
        <input
          v-model="modelInput"
          class="provider-model-input"
          type="text"
          :disabled="disabled || loading"
          aria-label="添加模型"
          placeholder="输入模型 ID，回车添加，支持粘贴多个"
          autocomplete="off"
          spellcheck="false"
          @keydown.enter.prevent="addModels"
        />
        <button
          class="provider-model-add-button"
          type="button"
          :disabled="disabled || loading || !parseModels(modelInput).length"
          @click="addModels"
        >
          <Plus :size="14" />
          添加
        </button>
      </div>
      <div v-else-if="!readonly" class="provider-model-bulk-hint">
        每行一个模型，重复项会自动合并。
      </div>
    </div>

    <footer v-if="!readonly" class="provider-model-footer">
      <label class="provider-model-sync">
        <el-switch
          :model-value="syncModelList"
          :disabled="disabled || loading"
          size="small"
          aria-label="同步模型列表"
          @update:model-value="emit('update:syncModelList', $event)"
        />
        <span>同步模型列表</span>
      </label>
      <p class="provider-model-hint">
        保存并启用此供应商后生效；关闭或列表留空时使用 Codex 官方默认模型。
      </p>
    </footer>
  </section>
</template>

<script setup>
import { computed, ref } from "vue"
import { ElScrollbar, ElSwitch } from "element-plus"
import "element-plus/es/components/scrollbar/style/css"
import "element-plus/es/components/switch/style/css"
import {
  Box,
  Check,
  Plus,
  RefreshCw,
  RotateCcw,
  SquarePen,
  X
} from "lucide-vue-next"

const props = defineProps({
  modelValue: { type: String, default: "" },
  syncModelList: { type: Boolean, default: false },
  readonly: { type: Boolean, default: false },
  maxHeight: { type: [String, Number], default: "176px" },
  disabled: { type: Boolean, default: false },
  loading: { type: Boolean, default: false }
})
const emit = defineEmits([
  "update:modelValue",
  "update:syncModelList",
  "fetch",
  "reset"
])
const bulkEditing = ref(false)
const modelInput = ref("")
const models = computed(() => parseModels(props.modelValue))

// 与 Codex 保存规则一致，保留首次出现的模型名和顺序。
function parseModels(value) {
  const seen = new Set()
  return String(value)
    .split(/[\r\n,;]+/)
    .map((model) => model.trim())
    .filter((model) => {
      const key = model.toLowerCase()
      if (!model || seen.has(key)) return false
      seen.add(key)
      return true
    })
}

function addModels(event) {
  if (props.readonly || props.disabled || props.loading || event?.isComposing)
    return
  emit(
    "update:modelValue",
    parseModels(`${props.modelValue}\n${modelInput.value}`).join("\n")
  )
  modelInput.value = ""
}

function removeModel(model) {
  if (props.readonly || props.disabled || props.loading) return
  emit(
    "update:modelValue",
    models.value.filter((item) => item !== model).join("\n")
  )
}

function resetModels() {
  if (props.readonly || props.disabled || props.loading) return
  modelInput.value = ""
  bulkEditing.value = false
  emit("reset")
}
</script>

<style scoped lang="less">
.provider-model-editor {
  display: flex;
  min-width: 0;
  flex: none;
  flex-direction: column;
  gap: 10px;

  .provider-model-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;

    .provider-model-heading {
      display: flex;
      flex: none;
      align-items: center;
      gap: 8px;

      .provider-model-title {
        color: var(--color-text);
        font-size: var(--font-size-base);
      }

      .provider-model-count {
        padding: 2px 7px;
        border-radius: 5px;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        font-variant-numeric: tabular-nums;
      }
    }

    .provider-model-actions {
      display: flex;
      align-items: center;
      gap: 6px;

      .provider-model-button {
        display: inline-flex;
        height: 28px;
        align-items: center;
        justify-content: center;
        gap: 5px;
        padding: 0 9px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        white-space: nowrap;
        cursor: pointer;

        &:hover:not(:disabled),
        &[aria-pressed="true"] {
          border-color: var(--color-line-strong);
          background: var(--color-panel-soft);
          color: var(--color-text);
        }

        &:focus-visible {
          outline: 2px solid var(--color-primary);
          outline-offset: 2px;
        }

        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        &.provider-model-fetch {
          border-color: var(--color-info-line);
          background: var(--color-primary-soft);
          color: var(--color-primary);

          .provider-model-fetch-icon {
            &.provider-model-spinning {
              animation: provider-model-spin 1s linear infinite;
            }
          }
        }
      }
    }
  }

  .provider-model-surface {
    flex: none;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);

    &:focus-within {
      border-color: var(--color-primary);
    }

    .provider-model-scroll {
      height: auto;
      --el-scrollbar-bg-color: var(--color-text-muted);
      --el-scrollbar-hover-bg-color: var(--color-text-muted);

      :deep(.el-scrollbar__wrap) {
        height: auto;
        overflow-x: hidden;
      }

      .provider-model-list {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        margin: 0;
        padding: 10px;
        list-style: none;

        .provider-model-item {
          display: flex;
          box-sizing: border-box;
          min-width: 0;
          height: 32px;
          flex: 0 0 calc(50% - 4px);
          align-items: center;
          gap: 8px;
          padding: 0 6px 0 10px;
          border-radius: 6px;
          background: var(--color-panel-soft);

          .provider-model-icon {
            flex: none;
            color: var(--color-text-muted);
          }

          .provider-model-name {
            overflow: hidden;
            min-width: 0;
            flex: 1;
            color: var(--color-text);
            font-size: var(--font-size-sm);
            font-family: Consolas, "Cascadia Code", monospace;
            text-overflow: ellipsis;
            white-space: nowrap;
          }

          .provider-model-remove {
            display: inline-flex;
            width: 22px;
            height: 22px;
            flex: none;
            align-items: center;
            justify-content: center;
            padding: 0;
            border: 0;
            border-radius: 4px;
            background: transparent;
            color: var(--color-text-muted);
            cursor: pointer;

            &:hover:not(:disabled) {
              background: var(--color-danger-soft);
              color: var(--color-danger);
            }

            &:focus-visible {
              outline: 2px solid var(--color-primary);
            }

            &:disabled {
              opacity: 0.5;
              cursor: not-allowed;
            }
          }
        }
      }

      .provider-model-empty {
        display: flex;
        box-sizing: border-box;
        min-height: 88px;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 6px;
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);

        .provider-model-empty-title {
          color: var(--color-text);
          font-size: var(--font-size-base);
        }
      }
    }

    .provider-model-bulk {
      display: block;
      box-sizing: border-box;
      width: 100%;
      height: 176px;
      margin: 0;
      padding: 10px 12px;
      border: 0;
      outline: none;
      background: transparent;
      color: var(--color-text);
      font-size: var(--font-size-sm);
      font-family: Consolas, "Cascadia Code", monospace;
      line-height: 1.8;
      resize: none;
    }

    .provider-model-add,
    .provider-model-bulk-hint {
      display: flex;
      box-sizing: border-box;
      height: 40px;
      align-items: center;
      gap: 8px;
      padding: 0 10px;
      border-top: 1px solid var(--color-line);

      .provider-model-input {
        width: 0;
        height: 100%;
        min-width: 0;
        flex: 1;
        padding: 0;
        border: 0;
        outline: none;
        background: transparent;
        color: var(--color-text);
        font-size: var(--font-size-sm);

        &::placeholder {
          color: var(--color-text-muted);
        }
      }

      .provider-model-add-button {
        display: inline-flex;
        height: 26px;
        flex: none;
        align-items: center;
        gap: 4px;
        padding: 0 8px;
        border: 0;
        border-radius: 5px;
        background: var(--color-primary-soft);
        color: var(--color-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;

        &:focus-visible {
          outline: 2px solid var(--color-primary);
        }

        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
      }
    }

    .provider-model-bulk-hint {
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
    }
  }

  .provider-model-footer {
    display: flex;
    align-items: center;
    gap: 12px;

    .provider-model-sync {
      display: inline-flex;
      flex: none;
      align-items: center;
      gap: 6px;
      color: var(--color-text);
      font-size: var(--font-size-sm);
      cursor: pointer;
      --el-switch-on-color: var(--color-primary-solid);
    }

    .provider-model-hint {
      margin: 0;
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
      line-height: 1.6;
    }
  }
}

@keyframes provider-model-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

<template>
  <BaseModal
    class="image-settings-modal"
    title="图像设置"
    :description="description"
    @close="$emit('close')"
  >
    <div class="image-settings-modal-body">
      <section class="modal-settings-group">
        <div class="group-head">
          <span class="group-title">尺寸与画质</span>
        </div>
        <div class="settings-grid-3">
          <label class="form-field">
            <span class="field-label">画质等级</span>
            <select v-model="form.quality" class="field-input">
              <option
                v-for="option in qualityOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </label>
          <label class="form-field">
            <span class="field-label">宽度 (W, px)</span>
            <input
              v-model.number="width"
              class="field-input"
              type="number"
              min="1"
              step="1"
              required
            />
          </label>
          <label class="form-field">
            <span class="field-label">高度 (H, px)</span>
            <input
              v-model.number="height"
              class="field-input"
              type="number"
              min="1"
              step="1"
              required
            />
          </label>
        </div>

        <div class="preset-block">
          <span class="preset-label">常用尺寸与比例预设</span>
          <div class="preset-list">
            <button
              v-for="preset in sizePresets"
              :key="preset.label"
              class="preset-button"
              :class="{
                active:
                  width === preset.width &&
                  height === preset.height &&
                  ratio === preset.ratio &&
                  tier === preset.tier
              }"
              type="button"
              :disabled="
                preset.tier !== '1k' &&
                preset.tier !== 'auto' &&
                !form.model.includes('codex')
              "
              :title="
                preset.tier !== '1k' && preset.tier !== 'auto'
                  ? '仅名称包含 codex 的模型可用'
                  : `${preset.width} × ${preset.height}`
              "
              @click="$emit('apply-size', preset)"
            >
              {{ preset.label }}
            </button>
          </div>
        </div>
        <p v-if="width * height > 40000000" class="field-error">
          尺寸总像素不能超过 4000 万（当前约为
          {{ Math.round((width * height) / 10000) }}
          万像素），请调整宽度和高度。
        </p>
      </section>

      <section class="modal-settings-group">
        <div class="group-head">
          <span class="group-title">生成数量</span>
          <span class="group-hint"
            >单次提交每张图片将作为一条独立的生图任务</span
          >
        </div>
        <div class="settings-grid-1">
          <label class="form-field">
            <span class="field-label">任务张数 (1 - 100)</span>
            <input
              v-model.number="form.n"
              class="field-input"
              type="number"
              min="1"
              max="100"
              step="1"
              required
            />
          </label>
        </div>
        <div class="preset-list">
          <button
            v-for="count in 10"
            :key="count"
            class="preset-button"
            :class="{ active: form.n === count }"
            type="button"
            @click="form.n = count"
          >
            {{ count }} 张
          </button>
        </div>
      </section>

      <section class="modal-settings-group">
        <div class="group-head">
          <span class="group-title">输出选项</span>
        </div>
        <div class="settings-grid-3">
          <label v-for="field in fields" :key="field.key" class="form-field">
            <span class="field-label">{{ field.label }}</span>
            <select v-model="form[field.key]" class="field-input">
              <option
                v-for="option in field.options"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </label>
        </div>
        <p
          v-if="
            form.background === 'transparent' && form.outputFormat === 'jpeg'
          "
          class="field-error"
        >
          透明背景需要选择 PNG 或 WebP 格式。
        </p>
      </section>
    </div>

    <footer class="image-settings-modal-footer">
      <div class="footer-left">
        <button
          class="action-button modal-footer-btn"
          type="button"
          title="恢复为默认画质、1024×1024 尺寸与 1 张生成"
          @click="$emit('reset')"
        >
          恢复默认
        </button>
      </div>
      <div class="footer-right">
        <button
          class="action-button modal-footer-btn primary-btn"
          type="button"
          @click="$emit('close')"
        >
          完成
        </button>
      </div>
    </footer>
  </BaseModal>
</template>

<script setup>
import { toRefs } from "vue"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({
  description: {
    type: String,
    default: ""
  },
  form: {
    type: Object,
    required: true
  },
  sizePresets: {
    type: Array,
    default: () => []
  },
  qualityOptions: {
    type: Array,
    default: () => []
  },
  fields: {
    type: Array,
    default: () => []
  }
})

defineEmits(["close", "reset", "apply-size"])
const { form, sizePresets, qualityOptions, fields } = toRefs(props)
const width = defineModel("width", { type: Number, required: true })
const height = defineModel("height", { type: Number, required: true })
const ratio = defineModel("ratio", { type: String, required: true })
const tier = defineModel("tier", { type: String, required: true })
</script>

<style scoped lang="less">
.image-settings-modal {
  .action-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 31px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    color: var(--color-text);
    background: var(--color-panel-soft);
    cursor: pointer;
    font-size: var(--font-size-sm);

    &:disabled {
      opacity: 0.45;
      cursor: not-allowed;
    }
  }

  :deep(.base-modal__panel) {
    width: min(640px, calc(100vw - 32px));
    max-height: min(86vh, 760px);
  }

  :deep(.base-modal__content) {
    padding: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .image-settings-modal-body {
    flex: 1;
    overflow-y: auto;
    padding: 16px 24px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;

    .modal-settings-group {
      display: flex;
      flex-direction: column;
      gap: 12px;
      padding: 14px 16px;
      border: 1px solid var(--color-line);
      border-radius: 9px;
      background: var(--color-panel-soft);

      .group-head {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: 10px;

        .group-title {
          font-size: 13px;
          font-weight: 600;
          color: var(--color-text);
        }

        .group-hint {
          font-size: 11px;
          color: var(--color-text-soft);
        }
      }

      .settings-grid-3 {
        display: grid;
        grid-template-columns: repeat(3, 1fr);
        gap: 10px;

        @media (max-width: 580px) {
          grid-template-columns: 1fr;
        }
      }

      .settings-grid-1 {
        display: flex;
        flex-direction: column;
        gap: 10px;
      }

      .form-field {
        display: flex;
        flex-direction: column;
        gap: 6px;

        .field-label {
          font-size: 12px;
          color: var(--color-text-muted);
        }

        .field-input {
          width: 100%;
          height: 36px;
          padding: 6px 10px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          outline: none;
          color: var(--color-text);
          background: var(--color-panel);
          font-size: 13px;

          &:focus {
            border-color: var(--color-primary);
          }
        }
      }

      .preset-block {
        display: flex;
        flex-direction: column;
        gap: 8px;

        .preset-label {
          font-size: 12px;
          color: var(--color-text-muted);
        }
      }

      .preset-list {
        display: flex;
        flex-wrap: wrap;
        gap: 6px;

        .preset-button {
          padding: 5px 9px;
          border: 1px solid var(--color-line);
          border-radius: 5px;
          background: var(--color-panel);
          color: var(--color-text);
          font-size: 12px;
          cursor: pointer;
          transition: all 0.15s;

          &:hover:not(:disabled) {
            border-color: var(--color-line-strong);
          }

          &.active {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            color: var(--color-primary);
            font-weight: 500;
          }

          &:disabled {
            opacity: 0.35;
            cursor: not-allowed;
          }
        }
      }

      .field-error {
        margin: 0;
        color: var(--color-danger);
        font-size: 12px;
        line-height: 1.5;
      }
    }
  }

  .image-settings-modal-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 24px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
    flex-shrink: 0;

    .footer-left,
    .footer-right {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .modal-footer-btn {
      height: 32px;
      font-size: 13px;

      &.primary-btn {
        background: var(--color-primary-solid, var(--color-primary));
        border-color: var(--color-primary-solid, var(--color-primary));
        color: #fff;

        &:hover:not(:disabled) {
          opacity: 0.9;
        }
      }
    }
  }
}
</style>

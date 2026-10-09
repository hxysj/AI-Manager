<template>
  <div v-if="visible" class="git-confirm-modal" @click="handleBackdropClick">
    <div
      class="git-confirm-modal-panel"
      role="dialog"
      aria-modal="true"
      @click.stop
    >
      <!-- 头部：图标 + 标题/描述 + 关闭按钮 -->
      <header class="git-confirm-modal-head">
        <div class="git-confirm-modal-head-left">
          <div
            :class="[
              'git-confirm-modal-icon-badge',
              `git-confirm-modal-icon-badge-${actionType}`
            ]"
          >
            <component :is="actionIcon" :size="20" />
          </div>
          <div class="git-confirm-modal-title-group">
            <h3 class="git-confirm-modal-title">{{ title || "操作确认" }}</h3>
            <p v-if="description" class="git-confirm-modal-desc">
              {{ description }}
            </p>
          </div>
        </div>

        <button
          class="git-confirm-modal-close-btn"
          type="button"
          title="关闭 (Esc)"
          @click="handleCancel"
        >
          <X :size="16" />
        </button>
      </header>

      <!-- 内容区：目标信息卡片与表单输入 -->
      <div class="git-confirm-modal-body">
        <!-- 详情/提示卡片 -->
        <div
          v-if="message || detail"
          :class="[
            'git-confirm-modal-info-card',
            `git-confirm-modal-info-card-${actionType}`
          ]"
        >
          <div v-if="message" class="git-confirm-modal-message">
            <template v-for="(part, idx) in parsedMessageParts" :key="idx">
              <code v-if="part.isHighlight" class="git-confirm-modal-badge">
                {{ part.text }}
              </code>
              <span v-else>{{ part.text }}</span>
            </template>
          </div>
          <div v-if="detail" class="git-confirm-modal-detail">
            <span class="git-confirm-modal-detail-label">相关详情：</span>
            <span class="git-confirm-modal-detail-text" :title="detail">
              {{ detail }}
            </span>
          </div>
        </div>

        <!-- 输入框（如分支重命名或输入新分支名） -->
        <div v-if="input" class="git-confirm-modal-field">
          <label class="git-confirm-modal-field-label">
            {{ inputLabel || "请输入" }}
          </label>
          <div class="git-confirm-modal-input-wrapper">
            <input
              ref="inputRef"
              v-model="localInputValue"
              type="text"
              class="git-confirm-modal-input"
              :placeholder="`请输入${inputLabel || '名称'}`"
              @keydown.enter="handleConfirm"
              @keydown.esc="handleCancel"
            />
          </div>
        </div>
      </div>

      <!-- 底部操作按钮 -->
      <footer class="git-confirm-modal-footer">
        <button
          class="git-confirm-modal-btn git-confirm-modal-btn-cancel"
          type="button"
          @click="handleCancel"
        >
          取消
        </button>
        <button
          :class="[
            'git-confirm-modal-btn',
            `git-confirm-modal-btn-${actionType}`
          ]"
          type="button"
          :disabled="input && !localInputValue.trim()"
          @click="handleConfirm"
        >
          {{ confirmText || "确认" }}
        </button>
      </footer>
    </div>
  </div>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import { Archive, GitBranch, RotateCcw, Trash2, X } from "lucide-vue-next"

const props = defineProps({
  visible: {
    type: Boolean,
    default: false
  },
  title: {
    type: String,
    default: "操作确认"
  },
  description: {
    type: String,
    default: ""
  },
  message: {
    type: String,
    default: ""
  },
  detail: {
    type: String,
    default: ""
  },
  input: {
    type: Boolean,
    default: false
  },
  inputLabel: {
    type: String,
    default: ""
  },
  inputValue: {
    type: String,
    default: ""
  },
  confirmText: {
    type: String,
    default: "确认"
  }
})

const emit = defineEmits(["confirm", "cancel"])

const localInputValue = ref("")
const inputRef = ref(null)

watch(
  () => props.visible,
  (val) => {
    if (val) {
      localInputValue.value = props.inputValue || ""
      if (props.input) {
        nextTick(() => {
          inputRef.value?.focus()
          inputRef.value?.select()
        })
      }
    }
  },
  { immediate: true }
)

// 判断操作类型：'delete' | 'restore' | 'archive' | 'default'
const actionType = computed(() => {
  const text =
    `${props.title} ${props.confirmText} ${props.description}`.toLowerCase()
  if (
    text.includes("删除") ||
    text.includes("delete") ||
    text.includes("不可撤销")
  ) {
    return "danger"
  }
  if (text.includes("恢复") || text.includes("restore")) {
    return "success"
  }
  if (text.includes("归档") || text.includes("archive")) {
    return "primary"
  }
  return "primary"
})

// 根据操作类型匹配图标
const actionIcon = computed(() => {
  if (actionType.value === "danger") return Trash2
  if (actionType.value === "success") return RotateCcw
  if (props.title?.includes("分支")) return GitBranch
  return Archive
})

// 解析消息中的「」高亮标识符，便于生成精致的 Badge
const parsedMessageParts = computed(() => {
  const msg = props.message || ""
  if (!msg) return []

  const parts = []
  const regex = /「([^」]+)」/g
  let lastIndex = 0
  let match

  while ((match = regex.exec(msg)) !== null) {
    if (match.index > lastIndex) {
      parts.push({
        text: msg.slice(lastIndex, match.index),
        isHighlight: false
      })
    }
    parts.push({
      text: match[1],
      isHighlight: true
    })
    lastIndex = regex.lastIndex
  }

  if (lastIndex < msg.length) {
    parts.push({
      text: msg.slice(lastIndex),
      isHighlight: false
    })
  }

  return parts.length ? parts : [{ text: msg, isHighlight: false }]
})

function handleConfirm() {
  emit("confirm", props.input ? localInputValue.value : true)
}

function handleCancel() {
  emit("cancel")
}

function handleBackdropClick() {
  emit("cancel")
}
</script>

<style scoped lang="less">
.git-confirm-modal {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(15, 23, 42, 0.45);
  backdrop-filter: blur(4px);
  animation: modalFadeIn 0.15s ease-out;

  .git-confirm-modal-panel {
    display: flex;
    flex-direction: column;
    width: 460px;
    max-width: calc(100vw - 32px);
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 20px 40px -15px rgba(0, 0, 0, 0.25);
    overflow: hidden;
    animation: modalPopIn 0.18s cubic-bezier(0.16, 1, 0.3, 1);

    // 头部
    .git-confirm-modal-head {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 12px;
      padding: 20px 20px 14px;

      .git-confirm-modal-head-left {
        display: flex;
        align-items: flex-start;
        gap: 12px;
        min-width: 0;

        .git-confirm-modal-icon-badge {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 40px;
          height: 40px;
          border-radius: 10px;
          flex-shrink: 0;
          transition: all 0.2s ease;

          &:where(.git-confirm-modal-icon-badge-primary) {
            background: #eff6ff;
            border: 1px solid #bfdbfe;
            color: #2563eb;
          }

          &:where(.git-confirm-modal-icon-badge-success) {
            background: #ecfdf5;
            border: 1px solid #a7f3d0;
            color: #059669;
          }

          &:where(.git-confirm-modal-icon-badge-danger) {
            background: #fef2f2;
            border: 1px solid #fecaca;
            color: #dc2626;
          }
        }

        .git-confirm-modal-title-group {
          display: flex;
          flex-direction: column;
          gap: 3px;
          min-width: 0;

          .git-confirm-modal-title {
            font: inherit;
            margin: 0;
            font-size: 16px;
            color: var(--color-text);
            line-height: 1.4;
          }

          .git-confirm-modal-desc {
            margin: 0;
            font-size: 13px;
            color: var(--color-text-muted);
            line-height: 1.45;
          }
        }
      }

      .git-confirm-modal-close-btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--color-text-muted);
        cursor: pointer;
        flex-shrink: 0;
        transition: all 0.15s ease;

        &:hover {
          background: var(--color-panel-soft);
          color: var(--color-text);
        }
      }
    }

    // 主体
    .git-confirm-modal-body {
      display: flex;
      flex-direction: column;
      gap: 14px;
      padding: 0 20px 20px;

      .git-confirm-modal-info-card {
        display: flex;
        flex-direction: column;
        gap: 8px;
        padding: 12px 14px;
        border-radius: 8px;
        background: var(--color-panel-soft);
        border: 1px solid var(--color-line);
        font-size: 13.5px;
        line-height: 1.5;
        color: var(--color-text);

        &:where(.git-confirm-modal-info-card-danger) {
          background: #fff8f8;
          border-color: #fee2e2;
        }

        &.git-confirm-modal-info-card-danger {
          .git-confirm-modal-message {
            .git-confirm-modal-badge {
              background: #fee2e2;
              color: #991b1b;
              border-color: #fca5a5;
            }
          }
        }

        .git-confirm-modal-message {
          word-break: break-all;

          .git-confirm-modal-badge {
            display: inline-flex;
            align-items: center;
            padding: 1px 6px;
            margin: 0 2px;
            border-radius: 4px;
            background: #e0f2fe;
            color: #0369a1;
            border: 1px solid #bae6fd;
            font-family: "JetBrains Mono", Consolas, monospace;
            font-size: 12.5px;
          }
        }

        .git-confirm-modal-detail {
          display: flex;
          align-items: center;
          gap: 4px;
          font-size: 12.5px;
          color: var(--color-text-muted);
          padding-top: 4px;
          border-top: 1px dashed var(--color-line);

          .git-confirm-modal-detail-label {
            flex-shrink: 0;
          }

          .git-confirm-modal-detail-text {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            color: var(--color-text);
          }
        }
      }

      // 输入表单
      .git-confirm-modal-field {
        display: flex;
        flex-direction: column;
        gap: 6px;

        .git-confirm-modal-field-label {
          font-size: 13px;
          color: var(--color-text);
        }

        .git-confirm-modal-input-wrapper {
          position: relative;

          .git-confirm-modal-input {
            width: 100%;
            height: 36px;
            padding: 0 12px;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            background: var(--color-panel);
            color: var(--color-text);
            font-size: 13.5px;
            transition: all 0.15s ease;
            box-sizing: border-box;

            &:focus {
              outline: none;
              border-color: #3b82f6;
              box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.15);
            }

            &::placeholder {
              color: var(--color-text-soft);
            }
          }
        }
      }
    }

    // 底部
    .git-confirm-modal-footer {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 10px;
      padding: 12px 20px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel-soft);

      .git-confirm-modal-btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        height: 34px;
        padding: 0 16px;
        border-radius: 7px;
        font-size: 13px;
        cursor: pointer;
        transition: all 0.15s ease;
        user-select: none;

        &:where(.git-confirm-modal-btn-cancel) {
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          color: var(--color-text);
        }

        &:where(.git-confirm-modal-btn-cancel):hover {
          background: var(--color-panel-soft);
          border-color: var(--color-line-strong);
        }

        &:where(.git-confirm-modal-btn-primary) {
          border: 1px solid #2563eb;
          background: #2563eb;
          color: #ffffff;
          box-shadow: 0 1px 2px rgba(37, 99, 235, 0.2);
        }

        &:where(.git-confirm-modal-btn-primary):hover:not(:disabled) {
          background: #1d4ed8;
          border-color: #1d4ed8;
        }

        &:where(.git-confirm-modal-btn-success) {
          border: 1px solid #059669;
          background: #059669;
          color: #ffffff;
          box-shadow: 0 1px 2px rgba(5, 150, 105, 0.2);
        }

        &:where(.git-confirm-modal-btn-success):hover:not(:disabled) {
          background: #047857;
          border-color: #047857;
        }

        &:where(.git-confirm-modal-btn-danger) {
          border: 1px solid #dc2626;
          background: #dc2626;
          color: #ffffff;
          box-shadow: 0 1px 2px rgba(220, 38, 38, 0.2);
        }

        &:where(.git-confirm-modal-btn-danger):hover:not(:disabled) {
          background: #b91c1c;
          border-color: #b91c1c;
        }

        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
      }
    }
  }
}

@keyframes modalFadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes modalPopIn {
  from {
    opacity: 0;
    transform: scale(0.96) translateY(-4px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

// 深色模式适配
:root[data-theme="dark"] {
  .git-confirm-modal {
    .git-confirm-modal-panel {
      .git-confirm-modal-head {
        .git-confirm-modal-head-left {
          .git-confirm-modal-icon-badge {
            &:where(.git-confirm-modal-icon-badge-primary) {
              background: rgba(37, 99, 235, 0.18);
              border-color: rgba(37, 99, 235, 0.4);
              color: #60a5fa;
            }

            &:where(.git-confirm-modal-icon-badge-success) {
              background: rgba(5, 150, 105, 0.18);
              border-color: rgba(5, 150, 105, 0.4);
              color: #34d399;
            }

            &:where(.git-confirm-modal-icon-badge-danger) {
              background: rgba(220, 38, 38, 0.18);
              border-color: rgba(220, 38, 38, 0.4);
              color: #f87171;
            }
          }
        }
      }

      .git-confirm-modal-body {
        .git-confirm-modal-info-card {
          &:where(.git-confirm-modal-info-card-danger) {
            background: rgba(220, 38, 38, 0.08);
            border-color: rgba(220, 38, 38, 0.25);
          }

          &.git-confirm-modal-info-card-danger {
            .git-confirm-modal-message {
              .git-confirm-modal-badge {
                background: rgba(220, 38, 38, 0.25);
                color: #fca5a5;
                border-color: rgba(220, 38, 38, 0.4);
              }
            }
          }

          .git-confirm-modal-message {
            .git-confirm-modal-badge {
              background: rgba(59, 130, 246, 0.2);
              border-color: rgba(59, 130, 246, 0.35);
              color: #93c5fd;
            }
          }
        }
      }
    }
  }
}
</style>

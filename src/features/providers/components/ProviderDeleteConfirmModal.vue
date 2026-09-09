<template>
  <BaseModal
    class="provider-delete-modal"
    :title="title"
    role="alertdialog"
    aria-modal="true"
    :aria-label="title"
    :aria-busy="pending"
    @close="close"
    @keydown="handleKeydown"
  >
    <div class="delete-content">
      <div class="delete-target">
        <span class="delete-icon"><Trash2 :size="22" /></span>
        <div class="delete-target-copy">
          <span class="delete-target-label">即将删除</span>
          <span class="delete-target-name">{{ name }}</span>
        </div>
      </div>
      <p class="delete-description">{{ description }}</p>
      <p class="delete-warning">
        <AlertTriangle :size="15" />此操作不可撤销，请确认后继续。
      </p>
      <p v-if="error" class="delete-error" role="alert">{{ error }}</p>
    </div>
    <footer class="delete-actions">
      <button
        ref="cancelButton"
        class="delete-button"
        type="button"
        :disabled="pending"
        @click="close"
      >
        取消
      </button>
      <button
        class="delete-button delete-button-danger"
        type="button"
        :disabled="pending"
        @click="!pending && $emit('confirm')"
      >
        <LoaderCircle v-if="pending" class="delete-spinner" :size="16" />
        <Trash2 v-else :size="16" />
        {{ pending ? "删除中…" : "确认删除" }}
      </button>
    </footer>
  </BaseModal>
</template>

<script setup>
import { onBeforeUnmount, onMounted, ref } from "vue"
import { AlertTriangle, LoaderCircle, Trash2 } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({
  title: { type: String, default: "删除供应商" },
  name: { type: String, required: true },
  description: { type: String, required: true },
  pending: { type: Boolean, default: false },
  error: { type: String, default: "" }
})
const emit = defineEmits(["close", "confirm"])
const cancelButton = ref(null)
let previousFocus = null

function close() {
  if (!props.pending) emit("close")
}

function handleKeydown(event) {
  if (event.key === "Escape") {
    event.preventDefault()
    event.stopPropagation()
    close()
  }
  if (event.key !== "Tab") return
  const buttons = event.currentTarget.querySelectorAll("button:not(:disabled)")
  const first = buttons[0]
  const last = buttons[buttons.length - 1]
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last?.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first?.focus()
  }
}

onMounted(() => {
  previousFocus = document.activeElement
  cancelButton.value?.focus()
})

onBeforeUnmount(() => {
  if (previousFocus?.isConnected) previousFocus.focus()
})
</script>

<style scoped lang="less">
.provider-delete-modal {
  :deep(.base-modal__panel) {
    width: 480px;
    border-radius: 14px;
  }
  :deep(.base-modal__header) {
    align-items: center;
    padding: 22px 24px 18px;
    h2 {
      font-size: var(--font-size-lg);
    }
  }
  :deep(.base-modal__content) {
    padding: 0;
  }
  .delete-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 0 24px 24px;
    overflow-y: auto;
    .delete-target {
      display: flex;
      align-items: center;
      gap: 13px;
      padding: 16px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel-soft);
      .delete-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 44px;
        height: 44px;
        flex: none;
        border-radius: 11px;
        color: var(--color-danger);
        background: var(--color-danger-soft);
      }
      .delete-target-copy {
        display: flex;
        flex-direction: column;
        gap: 4px;
        min-width: 0;
        .delete-target-label {
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);
        }
        .delete-target-name {
          color: var(--color-text);
          font-size: var(--font-size-base);
          overflow-wrap: anywhere;
        }
      }
    }
    .delete-description {
      margin: 0;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
      line-height: 1.75;
    }
    .delete-warning {
      display: flex;
      align-items: center;
      gap: 7px;
      margin: 0;
      color: var(--color-danger);
      font-size: var(--font-size-xs);
    }
    .delete-error {
      margin: 0;
      color: var(--color-danger);
      font-size: var(--font-size-sm);
    }
  }
  .delete-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 16px 24px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    .delete-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 7px;
      min-width: 88px;
      min-height: 38px;
      padding: 8px 15px;
      border: 1px solid var(--color-line-strong);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
      font-size: var(--font-size-sm);
      &:focus-visible {
        outline: 2px solid var(--color-primary);
        outline-offset: 3px;
      }
      &:hover:not(:disabled) {
        filter: brightness(0.95);
      }
      &:disabled {
        opacity: 0.6;
        cursor: not-allowed;
      }
      &.delete-button-danger {
        border-color: var(--color-danger);
        background: var(--color-danger);
        color: var(--color-page);
      }
      .delete-spinner {
        animation: delete-spin 1s linear infinite;
      }
    }
  }
}
@keyframes delete-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

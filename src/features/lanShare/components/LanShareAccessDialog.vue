<template>
  <section class="lan-share-access-dialog">
    <div class="lan-share-access-overlay" @click="emit('close')"></div>
    <div class="lan-share-access-panel">
      <header class="lan-share-access-head">
        <span data-emphasis class="lan-share-access-heading">设备快传</span>
        <button
          class="lan-share-access-close"
          type="button"
          @click="emit('close')"
        >
          <X :size="16" />
        </button>
      </header>
      <div class="lan-share-access-body">
        <div class="lan-share-access-qr" v-html="qrSvg"></div>
        <p class="lan-share-access-hint">使用移动设备扫描二维码访问</p>
        <div class="lan-share-access-url-wrapper">
          <span class="lan-share-access-label">访问地址</span>
          <p class="lan-share-access-url">{{ accessUrl }}</p>
        </div>
      </div>
      <footer class="lan-share-access-actions">
        <button
          class="lan-share-access-button"
          type="button"
          @click="emit('copy-url')"
        >
          <Copy :size="15" />
          复制地址
        </button>
        <button
          class="lan-share-access-button lan-share-access-button-danger"
          type="button"
          @click="emit('stop-service')"
        >
          <Square :size="15" />
          关闭服务
        </button>
      </footer>
    </div>
  </section>
</template>

<script setup>
import { Copy, Square, X } from "lucide-vue-next"

defineProps({
  qrSvg: {
    type: String,
    default: ""
  },
  accessUrl: {
    type: String,
    default: ""
  }
})

const emit = defineEmits(["close", "copy-url", "stop-service"])
</script>

<style scoped lang="less">
.lan-share-access-dialog {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: grid;
  place-items: center;
  padding: 16px;

  .lan-share-access-overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.32);
    backdrop-filter: blur(3px);
  }

  .lan-share-access-panel {
    position: relative;
    display: flex;
    width: min(460px, 100%);
    max-height: calc(100vh - 32px);
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 20px 60px rgba(15, 23, 42, 0.25);

    .lan-share-access-head {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      padding: 14px 16px;
      border-bottom: 1px solid var(--color-line);
      background: linear-gradient(180deg, var(--color-panel) 0%, var(--color-panel-soft) 100%);

      .lan-share-access-heading {
        color: var(--color-text);
        font-size: clamp(1rem, 2.5vw, 1.1rem);
      }

      .lan-share-access-close {
        display: inline-flex;
        width: 30px;
        height: 30px;
        align-items: center;
        justify-content: center;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s;

        &:hover {
          border-color: var(--color-primary);
          color: var(--color-primary);
        }
      }
    }

    .lan-share-access-body {
      display: flex;
      flex: 1;
      flex-direction: column;
      gap: 14px;
      padding: 20px 16px;
      overflow-y: auto;

      .lan-share-access-qr {
        display: grid;
        place-items: center;
        padding: 12px;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background: var(--color-panel-soft);

        :deep(svg) {
          max-width: 100%;
          height: auto;
        }
      }

      .lan-share-access-hint {
        margin: 0;
        color: var(--color-text-muted);
        font-size: clamp(0.8rem, 2vw, 0.88rem);
        text-align: center;
      }

      .lan-share-access-url-wrapper {
        display: flex;
        flex-direction: column;
        gap: 6px;

        .lan-share-access-label {
          color: var(--color-text-soft);
          font-size: clamp(0.75rem, 1.8vw, 0.8rem);
        }

        .lan-share-access-url {
          margin: 0;
          padding: 10px 12px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel-soft);
          color: var(--color-primary);
          font-family: "JetBrains Mono", "Consolas", monospace;
          font-size: clamp(0.75rem, 1.8vw, 0.82rem);
          line-height: 1.5;
          word-break: break-all;
        }
      }
    }

    .lan-share-access-actions {
      display: flex;
      flex: none;
      gap: 8px;
      padding: 14px 16px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel-soft);

      .lan-share-access-button {
        display: inline-flex;
        height: 36px;
        flex: 1;
        align-items: center;
        justify-content: center;
        gap: 6px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-primary);
        cursor: pointer;
        font-size: clamp(0.82rem, 2vw, 0.9rem);
        transition: all 0.2s;

        &:hover {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
        }
      }

      .lan-share-access-button-danger {
        border-color: var(--color-danger);
        color: var(--color-danger);

        &:hover {
          background: var(--color-danger-soft);
        }
      }
    }
  }

  @media (max-width: 480px) {
    padding: 12px;

    .lan-share-access-panel {
      .lan-share-access-head {
        padding: 12px;
      }

      .lan-share-access-body {
        padding: 16px 12px;
        gap: 12px;
      }

      .lan-share-access-actions {
        padding: 12px;
        flex-direction: column;

        .lan-share-access-button {
          height: 40px;
        }
      }
    }
  }
}
</style>

<template>
  <section class="lan-share-access-dialog">
    <div class="lan-share-access-overlay" @click="emit('close')"></div>
    <div class="lan-share-access-panel">
      <header class="lan-share-access-head">
        <div class="lan-share-access-title">
          <span class="lan-share-access-mark">设备访问</span>
          <strong class="lan-share-access-heading">
            扫描二维码或输入地址
          </strong>
        </div>
        <button
          class="lan-share-access-close"
          type="button"
          @click="emit('close')"
        >
          <X :size="15" />
        </button>
      </header>
      <div class="lan-share-access-qr" v-html="qrSvg"></div>
      <p class="lan-share-access-url">{{ accessUrl }}</p>
      <footer class="lan-share-access-actions">
        <button
          class="lan-share-access-button"
          type="button"
          @click="emit('copy-url')"
        >
          <Copy :size="14" />
          复制地址
        </button>
        <button
          class="lan-share-access-button"
          type="button"
          @click="emit('stop-service')"
        >
          <Square :size="14" />
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

  .lan-share-access-overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.28);
    backdrop-filter: blur(2px);
  }

  .lan-share-access-panel {
    position: relative;
    display: flex;
    width: 420px;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 18px 44px rgba(15, 23, 42, 0.2);

    .lan-share-access-head {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 12px;

      .lan-share-access-title {
        display: flex;
        flex-direction: column;
        gap: 4px;

        .lan-share-access-mark {
          color: var(--color-text-soft);
          font-size: 0.72rem;
          font-weight: 700;
        }

        .lan-share-access-heading {
          color: var(--color-primary);
          font-size: 1rem;
        }
      }

      .lan-share-access-close {
        display: inline-flex;
        width: 30px;
        height: 30px;
        align-items: center;
        justify-content: center;
        gap: 6px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: #ffffff;
        color: var(--color-primary);
        cursor: pointer;
        font-weight: 700;
      }
    }

    .lan-share-access-qr {
      display: grid;
      place-items: center;
      padding: 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: #f8fafc;
    }

    .lan-share-access-url {
      margin: 0;
      padding: 9px 10px;
      border-radius: 7px;
      background: #f4f7fa;
      color: var(--color-primary);
      font-family: "JetBrains Mono", "Consolas", monospace;
      font-size: 0.78rem;
      line-height: 1.45;
      word-break: break-all;
    }

    .lan-share-access-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 8px;

      .lan-share-access-button {
        display: inline-flex;
        height: 34px;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 0 12px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: #ffffff;
        color: var(--color-primary);
        cursor: pointer;
        font-weight: 700;
      }
    }
  }
}
</style>

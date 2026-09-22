<template>
  <section class="lan-share-access-dialog">
    <div class="lan-share-access-overlay" @click="emit('close')"></div>
    <div class="lan-share-access-panel">
      <!-- 科技感角标与扫描线 -->
      <div class="cyber-corner cyber-corner--tl"></div>
      <div class="cyber-corner cyber-corner--tr"></div>
      <div class="cyber-corner cyber-corner--bl"></div>
      <div class="cyber-corner cyber-corner--br"></div>
      <div class="cyber-scan-line"></div>

      <header class="lan-share-access-head">
        <div class="cyber-head-badge">
          <Radio :size="14" class="cyber-pulse-icon" />
          <span>GUEST_UPLINK // 访客访问信道</span>
        </div>
        <button
          class="lan-share-access-close"
          type="button"
          aria-label="关闭"
          @click="emit('close')"
        >
          <X :size="15" />
        </button>
      </header>

      <div class="lan-share-access-body">
        <div class="cyber-qr-wrapper">
          <div class="cyber-qr-frame">
            <div class="lan-share-access-qr" v-html="qrSvg"></div>
          </div>
          <div class="cyber-qr-glow"></div>
        </div>

        <p class="lan-share-access-hint">
          未安装客户端的设备（手机/平板/电脑）接入同一局域网后，直接扫码或在浏览器中打开链接，即可建立免装传输信道。
        </p>

        <div class="lan-share-access-url-wrapper">
          <div class="cyber-url-meta">
            <span class="lan-share-access-label"
              >HTTP WEB TERMINAL ADDRESS</span
            >
            <span class="cyber-url-protocol">TCP/IP PORT {{ accessPort }}</span>
          </div>
          <p class="lan-share-access-url">{{ accessUrl }}</p>
        </div>
      </div>

      <footer class="lan-share-access-actions">
        <button
          class="lan-share-access-button cyber-btn-primary"
          type="button"
          @click="emit('copy-url')"
        >
          <Copy :size="14" />
          <span>复制访问链接</span>
        </button>
        <button
          class="lan-share-access-button cyber-btn-cancel"
          type="button"
          @click="emit('close')"
        >
          <span>关闭</span>
        </button>
      </footer>
    </div>
  </section>
</template>

<script setup>
import { computed } from "vue"
import { Copy, Radio, X } from "lucide-vue-next"

const props = defineProps({
  qrSvg: {
    type: String,
    default: ""
  },
  accessUrl: {
    type: String,
    default: ""
  }
})

const emit = defineEmits(["close", "copy-url"])

const accessPort = computed(() => {
  try {
    const url = new URL(props.accessUrl)
    return url.port || "80"
  } catch {
    return "17631"
  }
})
</script>

<style scoped lang="less">
.lan-share-access-dialog {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: grid;
  place-items: center;
  padding: 16px;

  .lan-share-access-overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.45);
    backdrop-filter: blur(4px);
    animation: cyberFadeIn 0.2s ease-out;
  }

  .lan-share-access-panel {
    position: relative;
    display: flex;
    width: min(450px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    flex-direction: column;
    overflow: hidden;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    border-radius: 12px;
    box-shadow: 0 18px 48px rgba(15, 23, 42, 0.2);
    box-sizing: border-box;
    animation: cyberZoomIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);

    .cyber-corner {
      position: absolute;
      width: 10px;
      height: 10px;
      pointer-events: none;
      z-index: 2;

      &--tl {
        top: 6px;
        left: 6px;
        border-top: 2px solid var(--color-primary);
        border-left: 2px solid var(--color-primary);
      }
      &--tr {
        top: 6px;
        right: 6px;
        border-top: 2px solid var(--color-primary);
        border-right: 2px solid var(--color-primary);
      }
      &--bl {
        bottom: 6px;
        left: 6px;
        border-bottom: 2px solid var(--color-primary);
        border-left: 2px solid var(--color-primary);
      }
      &--br {
        bottom: 6px;
        right: 6px;
        border-bottom: 2px solid var(--color-primary);
        border-right: 2px solid var(--color-primary);
      }
    }

    .cyber-scan-line {
      position: absolute;
      top: 0;
      left: 0;
      right: 0;
      height: 2px;
      background: linear-gradient(
        90deg,
        transparent,
        var(--color-primary),
        transparent
      );
      opacity: 0.6;
      animation: cyberScan 3.5s linear infinite;
      pointer-events: none;
      z-index: 2;
    }

    .lan-share-access-head {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      padding: 14px 18px;
      border-bottom: 1px solid var(--color-line);
      background: var(--color-panel-soft);

      .cyber-head-badge {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        font-family:
          ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        font-size: 11px;
        font-weight: 600;
        letter-spacing: 1px;
        color: var(--color-primary);
        padding: 4px 8px;
        background: var(--color-primary-soft);
        border: 1px solid var(--color-info-line);
        border-radius: 4px;
      }

      .cyber-pulse-icon {
        color: var(--color-primary);
        animation: cyberPulse 1.8s ease-in-out infinite;
      }

      .lan-share-access-close {
        display: inline-flex;
        width: 28px;
        height: 28px;
        align-items: center;
        justify-content: center;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s;

        &:hover {
          border-color: var(--color-line-strong);
          color: var(--color-text);
          background: var(--color-panel-soft);
        }
      }
    }

    .lan-share-access-body {
      display: flex;
      flex: 1;
      flex-direction: column;
      align-items: center;
      gap: 16px;
      padding: 22px 20px;
      overflow-y: auto;

      .cyber-qr-wrapper {
        position: relative;
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 10px;

        .cyber-qr-frame {
          position: relative;
          z-index: 1;
          padding: 12px;
          background: #ffffff;
          border-radius: 10px;
          border: 1px solid var(--color-line);
          box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
        }

        .lan-share-access-qr {
          display: grid;
          place-items: center;
          width: 170px;
          height: 170px;

          :deep(svg) {
            width: 100%;
            height: 100%;
          }
        }

        .cyber-qr-glow {
          position: absolute;
          inset: 0;
          border-radius: 14px;
          background: radial-gradient(
            circle,
            var(--color-primary-soft),
            transparent 70%
          );
          filter: blur(8px);
          pointer-events: none;
        }
      }

      .lan-share-access-hint {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 12.5px;
        text-align: center;
        line-height: 1.6;
        padding: 0 8px;
      }

      .lan-share-access-url-wrapper {
        width: 100%;
        display: flex;
        flex-direction: column;
        gap: 6px;
        box-sizing: border-box;

        .cyber-url-meta {
          display: flex;
          align-items: center;
          justify-content: space-between;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 10px;
          letter-spacing: 0.8px;

          .lan-share-access-label {
            color: var(--color-primary);
            font-weight: 600;
          }

          .cyber-url-protocol {
            color: var(--color-success);
          }
        }

        .lan-share-access-url {
          margin: 0;
          padding: 10px 14px;
          border: 1px dashed var(--color-primary);
          border-radius: 6px;
          background: var(--color-primary-soft);
          color: var(--color-primary);
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 12.5px;
          font-weight: 500;
          line-height: 1.5;
          word-break: break-all;
          user-select: all;
        }
      }
    }

    .lan-share-access-actions {
      display: flex;
      flex: none;
      gap: 12px;
      padding: 14px 18px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel-soft);

      .lan-share-access-button {
        display: inline-flex;
        height: 38px;
        flex: 1;
        align-items: center;
        justify-content: center;
        gap: 8px;
        border-radius: 7px;
        font-size: 13.5px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s;

        &.cyber-btn-primary {
          background: var(--color-primary-solid);
          border: 1px solid var(--color-primary);
          color: #ffffff;

          &:hover {
            background: var(--color-primary);
            box-shadow: 0 2px 10px rgba(0, 0, 0, 0.15);
          }
        }

        &.cyber-btn-cancel {
          background: var(--color-panel);
          border: 1px solid var(--color-line);
          color: var(--color-text);

          &:hover {
            background: var(--color-panel-soft);
            border-color: var(--color-line-strong);
          }
        }
      }
    }
  }
}

@keyframes cyberFadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes cyberZoomIn {
  from {
    opacity: 0;
    transform: scale(0.95);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

@keyframes cyberScan {
  0% {
    top: 0%;
  }
  100% {
    top: 100%;
  }
}

@keyframes cyberPulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.6;
    transform: scale(0.92);
  }
}
</style>

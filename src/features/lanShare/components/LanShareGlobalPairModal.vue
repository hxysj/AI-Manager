<template>
  <div class="cyber-pair-modal-backdrop" @click.self="$emit('reject')">
    <div class="cyber-pair-panel">
      <!-- 科技感角标与扫描线 -->
      <div class="cyber-corner cyber-corner--tl"></div>
      <div class="cyber-corner cyber-corner--tr"></div>
      <div class="cyber-corner cyber-corner--bl"></div>
      <div class="cyber-corner cyber-corner--br"></div>
      <div class="cyber-scan-line"></div>

      <!-- 头部 -->
      <header class="cyber-pair-header">
        <div class="cyber-pair-badge">
          <ShieldAlert :size="18" class="cyber-pulse-icon" />
          <span>SECURITY // INCOMING_LINK_REQ</span>
        </div>
        <button
          class="cyber-pair-close"
          type="button"
          aria-label="关闭"
          :disabled="loading"
          @click="$emit('reject')"
        >
          <X :size="16" />
        </button>
      </header>

      <!-- 主体内容 -->
      <div class="cyber-pair-body">
        <div class="cyber-radar-avatar">
          <div class="cyber-radar-ring ring-1"></div>
          <div class="cyber-radar-ring ring-2"></div>
          <div class="cyber-radar-ring ring-3"></div>
          <Laptop :size="32" class="cyber-device-icon" />
        </div>

        <div class="cyber-device-info">
          <h3 class="cyber-device-name">{{ request?.name || "未知设备" }}</h3>
          <div class="cyber-telemetry-row">
            <span class="cyber-telemetry-tag"
              >IP: {{ request?.ip || "局域网未知节点" }}</span
            >
            <span class="cyber-telemetry-tag cyber-tag-live">P2P DIRECT</span>
          </div>
        </div>

        <p class="cyber-prompt-text">
          该设备请求与本机建立设备快传配对。请核对双方屏幕上显示的 6
          位安全校验码：
        </p>

        <!-- 校验码展示盒 -->
        <div class="cyber-code-hud">
          <div class="cyber-code-label">VERIFICATION KEY</div>
          <div class="cyber-code-chars">
            <span
              v-for="(chunk, index) in formattedCode"
              :key="index"
              class="cyber-code-chunk"
            >
              {{ chunk }}
            </span>
          </div>
        </div>

        <div class="cyber-security-note">
          <span class="cyber-dot-green"></span>
          <span>授权后将开启点对点文件传输与离线消息同步信道。</span>
        </div>
      </div>

      <!-- 底部操作按钮 -->
      <footer class="cyber-pair-footer">
        <button
          class="cyber-btn cyber-btn-reject"
          type="button"
          :disabled="loading"
          @click="$emit('reject')"
        >
          <XCircle :size="15" />
          <span>拒绝接入</span>
        </button>
        <button
          class="cyber-btn cyber-btn-accept"
          type="button"
          :disabled="loading"
          @click="$emit('accept')"
        >
          <CheckCircle2 :size="15" />
          <span>{{ loading ? "正在建立信道..." : "授信并连接" }}</span>
        </button>
      </footer>
    </div>
  </div>
</template>

<script setup>
import { computed } from "vue"
import { ShieldAlert, Laptop, CheckCircle2, XCircle, X } from "lucide-vue-next"

const props = defineProps({
  request: {
    type: Object,
    default: null
  },
  loading: {
    type: Boolean,
    default: false
  }
})

defineEmits(["accept", "reject"])

// 将 6 位校验码格式化为 2-2-2 形式便于辨识
const formattedCode = computed(() => {
  const raw = String(props.request?.code || "000000").replace(/\s+/g, "")
  if (raw.length <= 3) return [raw]
  const chunks = []
  for (let i = 0; i < raw.length; i += 2) {
    chunks.push(raw.slice(i, i + 2))
  }
  return chunks
})
</script>

<style scoped lang="less">
.cyber-pair-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(15, 23, 42, 0.45);
  backdrop-filter: blur(4px);
  animation: cyberFadeIn 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.cyber-pair-panel {
  position: relative;
  width: 460px;
  max-width: calc(100vw - 32px);
  background: var(--color-panel);
  border: 1px solid var(--color-line);
  border-radius: 12px;
  box-shadow: 0 18px 48px rgba(15, 23, 42, 0.2);
  padding: 24px;
  color: var(--color-text);
  overflow: hidden;
  box-sizing: border-box;
}

/* 科技角标 */
.cyber-corner {
  position: absolute;
  width: 10px;
  height: 10px;
  pointer-events: none;

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

/* 扫描流光线 */
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
}

.cyber-pair-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 18px;
}

.cyber-pair-badge {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1px;
  color: var(--color-warning);
  padding: 4px 10px;
  background: var(--color-warning-soft);
  border: 1px solid var(--color-warning-line);
  border-radius: 4px;
}

.cyber-pulse-icon {
  color: var(--color-warning);
  animation: cyberPulse 1.5s ease-in-out infinite;
}

.cyber-pair-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  background: var(--color-panel-soft);
  border: 1px solid var(--color-line);
  border-radius: 6px;
  color: var(--color-text-muted);
  cursor: pointer;
  transition: all 0.2s;

  &:hover:not(:disabled) {
    color: var(--color-text);
    border-color: var(--color-line-strong);
    background: var(--color-panel);
  }
}

.cyber-pair-body {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 16px;
}

/* 科技雷达环 */
.cyber-radar-avatar {
  position: relative;
  width: 72px;
  height: 72px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 4px;

  .cyber-device-icon {
    position: relative;
    z-index: 2;
    color: var(--color-primary);
  }

  .cyber-radar-ring {
    position: absolute;
    border-radius: 50%;
    border: 1px solid var(--color-line);

    &.ring-1 {
      width: 50px;
      height: 50px;
      background: radial-gradient(
        circle,
        var(--color-primary-soft),
        transparent 70%
      );
    }
    &.ring-2 {
      width: 66px;
      height: 66px;
      border-style: dashed;
      border-color: var(--color-primary);
      opacity: 0.55;
      animation: cyberRotate 10s linear infinite;
    }
    &.ring-3 {
      width: 82px;
      height: 82px;
      border-color: var(--color-line-strong);
    }
  }
}

.cyber-device-info {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}

.cyber-device-name {
  margin: 0;
  font-size: 19px;
  font-weight: 600;
  color: var(--color-text);
  letter-spacing: 0.5px;
}

.cyber-telemetry-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.cyber-telemetry-tag {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 11px;
  padding: 2px 8px;
  background: var(--color-panel-soft);
  border: 1px solid var(--color-line);
  border-radius: 4px;
  color: var(--color-text-muted);

  &.cyber-tag-live {
    color: var(--color-success);
    border-color: var(--color-success-line);
    background: var(--color-success-soft);
  }
}

.cyber-prompt-text {
  margin: 0;
  font-size: 13px;
  color: var(--color-text-muted);
  line-height: 1.55;
  padding: 0 10px;
}

/* 验证码 HUD */
.cyber-code-hud {
  width: 100%;
  padding: 14px 18px;
  background: var(--color-panel-soft);
  border: 1px dashed var(--color-line-strong);
  border-radius: 8px;
  box-sizing: border-box;
}

.cyber-code-label {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 10px;
  color: var(--color-text-soft);
  letter-spacing: 2px;
  margin-bottom: 8px;
  font-weight: 600;
}

.cyber-code-chars {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
}

.cyber-code-chunk {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: 3px;
  color: var(--color-primary);
  background: var(--color-panel);
  border: 1px solid var(--color-line);
  border-radius: 6px;
  padding: 6px 14px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
}

.cyber-security-note {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11.5px;
  color: var(--color-text-soft);
  text-align: left;
  width: 100%;

  .cyber-dot-green {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-success);
    box-shadow: 0 0 6px var(--color-success);
    flex-shrink: 0;
  }
}

.cyber-pair-footer {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 22px;
  padding-top: 18px;
  border-top: 1px solid var(--color-line);
}

.cyber-btn {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 40px;
  padding: 0 16px;
  border-radius: 7px;
  font-size: 13.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  border: 1px solid transparent;

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  &.cyber-btn-reject {
    background: var(--color-danger-soft);
    border-color: var(--color-danger-line);
    color: var(--color-danger);

    &:hover:not(:disabled) {
      background: var(--color-danger);
      border-color: var(--color-danger);
      color: #ffffff;
    }
  }

  &.cyber-btn-accept {
    background: var(--color-primary-solid);
    border-color: var(--color-primary);
    color: #ffffff;

    &:hover:not(:disabled) {
      background: var(--color-primary);
      box-shadow: 0 2px 10px rgba(0, 0, 0, 0.15);
    }
  }
}

@keyframes cyberFadeIn {
  from {
    opacity: 0;
    transform: scale(0.96);
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

@keyframes cyberRotate {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
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

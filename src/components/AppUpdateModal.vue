<template>
  <div v-if="dialog.open" class="app-update-modal">
    <div class="app-update-overlay"></div>
    <section class="app-update-panel" role="dialog" aria-modal="true">
      <header class="app-update-header">
        <div class="app-update-title">
          <span class="app-update-eyebrow">应用更新</span>
          <h2 class="app-update-heading">{{ updateTitle }}</h2>
        </div>
        <button
          class="app-update-icon-button"
          type="button"
          aria-label="关闭更新面板"
          :disabled="isRunning"
          @click="emit('close')"
        >
          <X :size="17" />
        </button>
      </header>

      <div class="app-update-body">
        <div class="app-update-mark">
          <Info :size="22" />
        </div>
        <div class="app-update-copy">
          <span class="app-update-message">{{ updateMessage }}</span>
        </div>
      </div>

      <div v-if="dialog.phase === 'downloading'" class="app-update-progress">
        <div class="app-update-progress-head">
          <span class="app-update-transfer">{{ updateTransferText }}</span>
          <strong class="app-update-percent">{{ updateProgressText }}</strong>
        </div>
        <div class="app-update-progress-track">
          <div
            class="app-update-progress-bar"
            :style="{ width: updateProgressWidth }"
          ></div>
        </div>
      </div>

      <pre
        v-if="dialog.releaseNotes && dialog.phase !== 'downloading'"
        class="app-update-notes"
        >{{ dialog.releaseNotes }}</pre
      >

      <footer class="app-update-footer">
        <button
          v-if="dialog.phase === 'available'"
          class="app-update-button"
          type="button"
          @click="emit('close')"
        >
          稍后
        </button>
        <button
          v-if="dialog.phase === 'available'"
          class="app-update-button app-update-primary-button"
          type="button"
          @click="emit('download')"
        >
          <RefreshCw :size="15" />
          立即下载
        </button>
        <button
          v-else-if="dialog.phase === 'downloaded'"
          class="app-update-button"
          type="button"
          @click="emit('close')"
        >
          稍后
        </button>
        <button
          v-if="dialog.phase === 'downloaded'"
          class="app-update-button app-update-primary-button"
          type="button"
          @click="emit('install')"
        >
          打开安装向导
        </button>
        <button
          v-else-if="dialog.phase === 'installer-opened'"
          class="app-update-button app-update-primary-button"
          type="button"
          @click="emit('close')"
        >
          关闭
        </button>
        <button
          v-else-if="dialog.phase === 'downloading'"
          class="app-update-button"
          type="button"
          disabled
        >
          下载中...
        </button>
        <button
          v-else-if="!settledPhases.includes(dialog.phase)"
          class="app-update-button app-update-primary-button"
          type="button"
          @click="emit('close')"
        >
          确定
        </button>
      </footer>
    </section>
  </div>
</template>

<script setup>
import { computed } from "vue"
import { Info, RefreshCw, X } from "lucide-vue-next"

const props = defineProps({
  dialog: {
    type: Object,
    required: true
  }
})

const emit = defineEmits(["close", "download", "install"])

const runningPhases = ["checking", "downloading", "installing"]
const settledPhases = [
  "available",
  "downloaded",
  "downloading",
  "installer-opened"
]

const isRunning = computed(() => runningPhases.includes(props.dialog.phase))

const updateTitle = computed(() => {
  const titleMap = {
    checking: "正在检查更新",
    available: "发现新版本",
    downloading: "正在下载更新",
    downloaded: "更新已下载",
    installing: "正在安装更新",
    "installer-opened": "安装程序已打开",
    "not-available": "当前已是最新版本",
    unconfigured: "缺少更新配置",
    "dev-disabled": "开发模式无法完整检查更新",
    error: "检查更新失败"
  }

  return titleMap[props.dialog.phase] || "检查更新"
})

const updateMessage = computed(() => {
  return props.dialog.message || "正在准备更新状态。"
})

const updateProgressWidth = computed(() => {
  const percent = Math.min(100, Math.max(0, Number(props.dialog.percent || 0)))

  return `${percent}%`
})

const updateProgressText = computed(() => {
  const percent = Math.min(100, Math.max(0, Number(props.dialog.percent || 0)))

  return `${percent.toFixed(1)}%`
})

const updateTransferText = computed(() => {
  if (!props.dialog.total) {
    return "正在获取下载进度"
  }

  return `${formatUpdateBytes(props.dialog.transferred)} / ${formatUpdateBytes(
    props.dialog.total
  )}`
})

function formatUpdateBytes(value) {
  const size = Number(value || 0)

  if (size >= 1024 * 1024) {
    return `${(size / 1024 / 1024).toFixed(1)} MB`
  }

  if (size >= 1024) {
    return `${(size / 1024).toFixed(1)} KB`
  }

  return `${size} B`
}
</script>

<style scoped lang="less">
.app-update-modal {
  position: fixed;
  inset: 0;
  z-index: 82;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;

  .app-update-overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.28);
    backdrop-filter: blur(2px);
  }

  .app-update-panel {
    position: relative;
    width: 560px;
    max-height: calc(100vh - 48px);
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 18px 48px rgba(15, 23, 42, 0.2);
  }

  .app-update-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 18px 18px 12px;
    border-bottom: 1px solid var(--color-line);

    .app-update-title {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 5px;

      .app-update-eyebrow {
        color: var(--color-text-soft);
        font-size: 0.68rem;
        font-weight: 700;
        letter-spacing: 0.12em;
        line-height: 1;
        text-transform: uppercase;
      }

      .app-update-heading {
        margin: 0;
        color: var(--color-text);
        font-size: 1.05rem;
        line-height: 1.25;
      }
    }

    .app-update-icon-button {
      display: flex;
      width: 30px;
      height: 30px;
      align-items: center;
      justify-content: center;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: #ffffff;
      color: var(--color-text-muted);
      cursor: pointer;

      &:hover {
        border-color: #c8d2df;
        background: #f7f9fc;
        color: var(--color-text);
      }

      &:disabled {
        cursor: default;
        opacity: 0.55;
      }
    }
  }

  .app-update-body {
    display: flex;
    gap: 14px;
    padding: 18px;

    .app-update-mark {
      display: flex;
      width: 44px;
      height: 44px;
      flex: 0 0 auto;
      align-items: center;
      justify-content: center;
      border: 1px solid #b7d9f6;
      border-radius: 8px;
      background: #e8f4ff;
      color: #0b78d0;
    }

    .app-update-copy {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 8px;
      padding-top: 2px;

      .app-update-message {
        color: var(--color-text-muted);
        font-size: 0.86rem;
        line-height: 1.6;
      }
    }
  }

  .app-update-progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 18px 16px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #f8fbff;

    .app-update-progress-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      color: var(--color-text-muted);
      font-size: 0.8rem;
      font-weight: 700;

      .app-update-percent {
        color: var(--color-primary);
      }
    }

    .app-update-progress-track {
      height: 8px;
      overflow: hidden;
      border-radius: 999px;
      background: #e6edf5;

      .app-update-progress-bar {
        height: 100%;
        border-radius: inherit;
        background: var(--color-primary);
      }
    }
  }

  .app-update-notes {
    max-height: 160px;
    margin: 0 18px 16px;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #f8fafc;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.76rem;
    line-height: 1.55;
    padding: 12px;
    white-space: pre-wrap;
  }

  .app-update-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 0 18px 18px;

    .app-update-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 7px;
      min-width: 88px;
      height: 34px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: #ffffff;
      color: var(--color-primary);
      font-size: 0.86rem;
      font-weight: 700;
      cursor: pointer;

      &:hover {
        border-color: var(--color-primary);
        background: #f7f9fc;
      }

      &:disabled {
        cursor: default;
        opacity: 0.68;
      }
    }

    .app-update-primary-button {
      border-color: var(--color-primary);
      background: var(--color-primary);
      color: #ffffff;

      &:hover {
        border-color: var(--color-primary);
        background: var(--color-primary);
      }
    }
  }
}
</style>

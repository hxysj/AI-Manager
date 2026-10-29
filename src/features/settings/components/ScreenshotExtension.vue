<template>
  <section class="screenshot-extension">
    <article
      class="screenshot-extension-card"
      tabindex="0"
      role="button"
      aria-label="进入截图贴图详情"
      @click="openManager()"
      @keydown.enter="openManager()"
      @keydown.space.prevent="openManager()"
    >
      <div class="screenshot-extension-main">
        <div class="screenshot-extension-icon"><Scissors :size="24" /></div>
        <div class="screenshot-extension-copy">
          <div class="screenshot-extension-heading">
            <h3 class="screenshot-extension-title">截图贴图</h3>
            <span class="screenshot-extension-status" aria-live="polite">
              {{ ready ? (!supported ? "当前平台暂不支持" : enabled ? "已启动" : "已关闭") : "读取中…" }}
            </span>
          </div>
          <p class="screenshot-extension-description">
            {{ supported ? "支持区域截图、长截图、文字识别和桌面贴图，关闭后保留历史与配置。" : "当前平台可查看和导出已有截图记录；原生截图、快捷键和桌面贴图暂不可用。" }}
          </p>
        </div>
      </div>
      <div class="screenshot-extension-actions" @click.stop @keydown.stop>
        <button
          class="screenshot-extension-button"
          type="button"
          :disabled="!ready || pending"
          @click="openManager()"
        >
          <Clock :size="15" />截图记录
        </button>
        <button
          class="screenshot-extension-button"
          type="button"
          :disabled="!ready || pending || !supported"
          @click="openManager(true)"
        >
          <Settings :size="15" />配置
        </button>
        <button
          class="screenshot-extension-switch"
          :class="{ 'screenshot-extension-switch--active': enabled }"
          type="button"
          role="switch"
          aria-label="启用截图贴图"
          :aria-checked="enabled"
          :aria-busy="pending"
          :disabled="!ready || pending || !supported"
          @click="toggleEnabled"
        >
          <span class="screenshot-extension-switch-thumb"></span>
        </button>
      </div>
    </article>
    <p v-if="error" class="screenshot-extension-error" role="alert">
      {{ error }}
    </p>
    <BaseModal
      v-if="showManager"
      class="screenshot-extension-modal"
      title="截图贴图"
      description="管理截图记录、桌面贴图和快捷键配置。"
      @close="showManager = false"
    >
      <ScreenshotView />
    </BaseModal>
    <ScreenshotView
      v-if="showSettings"
      settings-only
      :providers="providers"
      :codex-accounts="codexAccounts"
      :runtime-models="runtimeModels"
      @close="showSettings = false"
    />
  </section>
</template>

<script setup>
import { defineAsyncComponent, onBeforeUnmount, onMounted, ref } from "vue"
import { Clock, Scissors, Settings } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { screenshotApi } from "@/api/modules/screenshot"
import { subscribeReady } from "@/api/request"

defineProps({
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] }
})

// 管理页与原生截图窗口共用独立模块，打开详情后才加载管理界面。
const ScreenshotView = defineAsyncComponent(
  () => import("@/features/screenshot/index.vue")
)
const enabled = ref(false)
const supported = ref(true)
const ready = ref(false)
const pending = ref(false)
const error = ref("")
const showManager = ref(false)
const showSettings = ref(false)
let unlisten
let disposed = false
let sequence = 0

async function loadStatus() {
  if (disposed || pending.value) return
  const request = ++sequence
  try {
    const settings = await screenshotApi.call("status")
    if (disposed || pending.value || request !== sequence) return
    supported.value = settings.supported !== false
    enabled.value = supported.value && settings.enabled
    ready.value = true
  } catch (cause) {
    if (!disposed && request === sequence) error.value = String(cause)
  }
}

async function toggleEnabled() {
  if (!ready.value || pending.value || !supported.value) return
  pending.value = true
  sequence++
  error.value = ""
  try {
    const settings = await screenshotApi.call("set-enabled", {
      enabled: !enabled.value
    })
    enabled.value = settings.enabled
  } catch (cause) {
    error.value = String(cause)
  } finally {
    pending.value = false
    // 以已保存状态为准，失败和其他窗口的修改都不会留下虚假的启用状态。
    await loadStatus()
  }
}

function openManager(settings = false) {
  if (!ready.value || pending.value) return
  if (settings) showSettings.value = true
  else showManager.value = true
}

onMounted(async () => {
  try {
    unlisten = await subscribeReady("screenshot:changed", loadStatus)
    if (disposed) {
      unlisten()
      return
    }
    await loadStatus()
  } catch (cause) {
    if (!disposed) error.value = String(cause)
  }
})

onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
})
</script>

<style scoped lang="less">
.screenshot-extension {
  .screenshot-extension-card {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 22px 28px;
    border: 1px solid var(--color-line);
    border-radius: 16px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
    cursor: pointer;

    &:focus-visible {
      outline: 2px solid #2563eb;
      outline-offset: 2px;
    }

    .screenshot-extension-main {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      gap: 18px;

      .screenshot-extension-icon {
        display: flex;
        width: 52px;
        height: 52px;
        flex: none;
        align-items: center;
        justify-content: center;
        border-radius: 14px;
        background: #eff6ff;
        color: #2563eb;
      }

      .screenshot-extension-copy {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 4px;

        .screenshot-extension-heading {
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          gap: 10px;

          .screenshot-extension-title {
            margin: 0;
            color: var(--color-text);
            font-size: var(--font-size-base);
          }

          .screenshot-extension-status {
            color: var(--color-text-muted);
            font-size: 12px;
          }
        }

        .screenshot-extension-description {
          margin: 0;
          color: var(--color-text-muted);
          font-size: 13px;
          line-height: 1.5;
        }
      }
    }

    .screenshot-extension-actions {
      display: flex;
      min-height: 52px;
      flex: none;
      align-items: center;
      gap: 10px;
      padding-left: 24px;
      border-left: 1px solid var(--color-line);

      .screenshot-extension-button {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 6px 10px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 13px;
        cursor: pointer;

        &:hover {
          background: var(--color-panel-soft);
        }
      }

      .screenshot-extension-switch {
        width: 44px;
        height: 24px;
        flex: none;
        margin-left: 6px;
        padding: 2px;
        border: none;
        border-radius: 12px;
        background: #cbd5e1;
        cursor: pointer;
        transition: background 0.2s ease;

        .screenshot-extension-switch-thumb {
          display: block;
          width: 20px;
          height: 20px;
          border-radius: 50%;
          background: #ffffff;
          box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
          transition: transform 0.2s ease;
        }

        &.screenshot-extension-switch--active {
          background: #2563eb;

          .screenshot-extension-switch-thumb {
            transform: translateX(20px);
          }
        }
      }

      :disabled {
        opacity: 0.6;
        cursor: not-allowed;
      }
    }
  }

  .screenshot-extension-error {
    margin: 8px 0 0;
    color: var(--color-danger);
    font-size: 13px;
  }
}

.screenshot-extension-modal {
  :deep(.base-modal__panel) {
    width: 86vw;
    max-width: calc(100vw - 48px);
    height: 80vh;
    border-radius: 16px;
  }

  :deep(.base-modal__header h2) {
    font-size: var(--font-size-lg);
  }

  :deep(.base-modal__content) {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
  }
}
</style>

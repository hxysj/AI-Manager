<template>
  <section class="lan-share-preview-dialog">
    <div class="lan-share-preview-overlay" @click="emit('close')"></div>
    <div class="lan-share-preview-panel">
      <header class="lan-share-preview-head">
        <div class="lan-share-preview-title">
          <span class="lan-share-preview-mark">文件预览</span>
          <span data-emphasis class="lan-share-preview-heading" :title="file?.name || ''">
            {{ file?.name || "文件预览" }}
          </span>
          <small class="lan-share-preview-meta">
            {{ formatSize(file?.size) }} · {{ file?.mimeType || "文件" }}
          </small>
        </div>
        <button
          class="lan-share-preview-close"
          type="button"
          @click="emit('close')"
        >
          <X :size="15" />
        </button>
      </header>
      <div class="lan-share-preview-body">
        <img
          v-if="previewKind === 'image'"
          class="lan-share-preview-media"
          :src="previewUrl"
          :alt="file?.name || '图片预览'"
        />
        <video
          v-else-if="previewKind === 'video'"
          class="lan-share-preview-media"
          :src="previewUrl"
          controls
        ></video>
        <audio
          v-else-if="previewKind === 'audio'"
          class="lan-share-preview-audio"
          :src="previewUrl"
          controls
        ></audio>
        <iframe
          v-else-if="previewKind === 'pdf'"
          class="lan-share-preview-frame"
          :src="previewUrl"
          :title="file?.name || 'PDF 预览'"
        ></iframe>
        <pre
          v-else-if="previewKind === 'text' && textContent"
          class="lan-share-preview-text"
          >{{ textContent }}</pre
        >
        <iframe
          v-else-if="previewKind === 'text'"
          class="lan-share-preview-frame"
          :src="previewUrl"
          :title="file?.name || '文本预览'"
        ></iframe>
        <div v-else class="lan-share-preview-empty">
          当前文件类型暂不支持在应用内预览，可下载后使用本地程序打开。
        </div>
      </div>
      <footer class="lan-share-preview-actions">
        <button
          class="lan-share-preview-button"
          type="button"
          @click="emit('close')"
        >
          关闭
        </button>
        <button
          class="lan-share-preview-button lan-share-preview-button-primary"
          type="button"
          @click="emit('download', file)"
        >
          <Download :size="14" />
          下载文件
        </button>
      </footer>
    </div>
  </section>
</template>

<script setup>
import { Download, X } from "lucide-vue-next"

defineProps({
  file: {
    type: Object,
    default: null
  },
  previewUrl: {
    type: String,
    default: ""
  },
  previewKind: {
    type: String,
    default: "unsupported"
  },
  textContent: {
    type: String,
    default: ""
  }
})

const emit = defineEmits(["close", "download"])

function formatSize(value) {
  const size = Number(value || 0)

  if (size >= 1024 * 1024 * 1024) {
    return `${(size / 1024 / 1024 / 1024).toFixed(2)} GB`
  }
  if (size >= 1024 * 1024) {
    return `${(size / 1024 / 1024).toFixed(2)} MB`
  }
  if (size >= 1024) {
    return `${(size / 1024).toFixed(1)} KB`
  }

  return `${size} B`
}
</script>

<style scoped lang="less">
.lan-share-preview-dialog {
  position: fixed;
  inset: 0;
  z-index: 82;
  display: grid;
  place-items: center;
  padding: 24px;

  .lan-share-preview-overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.3);
    backdrop-filter: blur(2px);
  }

  .lan-share-preview-panel {
    position: relative;
    display: flex;
    width: 860px;
    max-height: calc(100vh - 48px);
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 20px 54px rgba(15, 23, 42, 0.22);

    .lan-share-preview-head {
      display: flex;
      flex: none;
      align-items: flex-start;
      justify-content: space-between;
      gap: 12px;
      padding: 14px 16px;
      border-bottom: 1px solid var(--color-line);

      .lan-share-preview-title {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 4px;

        .lan-share-preview-mark {
          color: var(--color-text-soft);
          font-size: 0.72rem;
        }

        .lan-share-preview-heading {
          overflow: hidden;
          color: var(--color-primary);
          font-size: 1rem;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .lan-share-preview-meta {
          color: var(--color-text-muted);
          font-size: 0.76rem;
        }
      }

      .lan-share-preview-close {
        display: inline-flex;
        width: 30px;
        height: 30px;
        flex: none;
        align-items: center;
        justify-content: center;
        gap: 6px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-primary);
        cursor: pointer;
      }
    }

    .lan-share-preview-body {
      display: flex;
      min-height: 420px;
      flex: 1;
      align-items: center;
      justify-content: center;
      overflow: auto;
      background: var(--color-panel-soft);

      .lan-share-preview-media {
        display: block;
        max-width: 100%;
        max-height: 68vh;
      }

      .lan-share-preview-audio {
        width: calc(100% - 48px);
      }

      .lan-share-preview-frame {
        width: 100%;
        height: 68vh;
        border: 0;
        background: var(--color-panel);
      }

      .lan-share-preview-text {
        width: 100%;
        min-height: 420px;
        margin: 0;
        padding: 16px 18px;
        color: var(--color-text);
        font-family: "JetBrains Mono", "Consolas", monospace;
        font-size: 0.78rem;
        line-height: 1.6;
        white-space: pre-wrap;
        word-break: break-word;
      }

      .lan-share-preview-empty {
        display: flex;
        min-height: 220px;
        align-items: center;
        justify-content: center;
        padding: 24px;
        color: var(--color-text-muted);
        font-size: 0.86rem;
      }
    }

    .lan-share-preview-actions {
      display: flex;
      flex: none;
      justify-content: flex-end;
      gap: 8px;
      padding: 12px 16px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel);

      .lan-share-preview-button {
        display: inline-flex;
        height: 34px;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 0 12px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-primary);
        cursor: pointer;
      }

      .lan-share-preview-button-primary {
        border-color: var(--color-primary);
        background: var(--color-primary-solid);
        color: #ffffff;
      }
    }
  }
}
</style>

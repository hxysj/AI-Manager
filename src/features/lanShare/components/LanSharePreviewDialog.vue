<template>
  <Teleport to="body">
    <section class="lan-share-preview-dialog" :style="{ zIndex }">
      <div class="lan-share-preview-overlay" @click="emit('close')"></div>
      <div class="lan-share-preview-panel">
        <!-- 头部栏 -->
        <header class="lan-share-preview-head">
          <div class="lan-share-preview-head-left">
            <div
              class="lan-share-preview-avatar"
              :class="`is-${previewKind}`"
              aria-hidden="true"
            >
              <component :is="getKindIcon(previewKind)" :size="20" />
            </div>
            <div class="lan-share-preview-title-box">
              <div class="lan-share-preview-title-row">
                <h2
                  class="lan-share-preview-filename"
                  :title="file?.name || ''"
                >
                  {{ file?.name || "文件预览" }}
                </h2>
              </div>
              <div class="lan-share-preview-meta-row">
                <span class="lan-share-preview-badge">
                  {{ formatSize(file?.size) }}
                </span>
                <span class="lan-share-preview-badge">
                  {{ file?.mimeType || "未知格式" }}
                </span>
                <span
                  class="lan-share-preview-badge lan-share-preview-badge--kind"
                >
                  {{ getKindLabel(previewKind) }}
                </span>
              </div>
            </div>
          </div>
          <button
            class="lan-share-preview-close"
            type="button"
            aria-label="关闭预览"
            @click="emit('close')"
          >
            <X :size="16" />
          </button>
        </header>

        <!-- 预览主体区 -->
        <div class="lan-share-preview-body">
          <!-- 图片预览 -->
          <div
            v-if="previewKind === 'image'"
            class="lan-share-preview-image-wrap"
          >
            <img
              class="lan-share-preview-image"
              :src="previewUrl"
              :alt="file?.name || '图片预览'"
            />
          </div>

          <!-- 视频预览 -->
          <div
            v-else-if="previewKind === 'video'"
            class="lan-share-preview-video-wrap"
          >
            <video
              class="lan-share-preview-video"
              :src="previewUrl"
              controls
            ></video>
          </div>

          <!-- 音频预览 -->
          <div
            v-else-if="previewKind === 'audio'"
            class="lan-share-preview-audio-wrap"
          >
            <div class="lan-share-preview-audio-card">
              <div class="lan-share-preview-audio-disc">
                <Music :size="36" class="lan-share-preview-disc-icon" />
                <div class="lan-share-preview-sound-waves" aria-hidden="true">
                  <span class="wave-bar"></span>
                  <span class="wave-bar"></span>
                  <span class="wave-bar"></span>
                  <span class="wave-bar"></span>
                  <span class="wave-bar"></span>
                </div>
              </div>
              <div class="lan-share-preview-audio-info">
                <span class="audio-name">{{ file?.name || "音频播放" }}</span>
                <span class="audio-meta">
                  {{ formatSize(file?.size) }} · {{ file?.mimeType || "audio" }}
                </span>
              </div>
              <audio
                class="lan-share-preview-audio-player"
                :src="previewUrl"
                controls
              ></audio>
            </div>
          </div>

          <!-- PDF 文档预览 -->
          <div
            v-else-if="previewKind === 'pdf'"
            class="lan-share-preview-pdf-wrap"
          >
            <iframe
              class="lan-share-preview-frame"
              :src="previewUrl"
              :title="file?.name || 'PDF 预览'"
            ></iframe>
          </div>

          <!-- 文本与代码预览（支持行号与全文快速复制） -->
          <div
            v-else-if="previewKind === 'text' && textContent"
            class="lan-share-preview-code-wrap"
          >
            <div class="lan-share-preview-code-toolbar">
              <div class="lan-share-preview-code-meta">
                <span class="meta-tag">文本 · UTF-8</span>
                <span>{{ textLines.length }} 行</span>
                <span>{{ (textContent || "").length }} 字符</span>
              </div>
              <button
                class="lan-share-preview-copy-btn"
                :class="{ 'is-copied': textCopied }"
                type="button"
                @click="copyTextContent"
              >
                <Check v-if="textCopied" :size="13" :stroke-width="2.5" />
                <Copy v-else :size="13" />
                <span>{{ textCopied ? "已复制全文" : "复制全文" }}</span>
              </button>
            </div>
            <div class="lan-share-preview-code-scroll">
              <div class="lan-share-preview-gutter" aria-hidden="true">
                <span
                  v-for="n in textLines.length"
                  :key="n"
                  class="gutter-num"
                  >{{ n }}</span
                >
              </div>
              <pre class="lan-share-preview-code-content">{{
                textContent
              }}</pre>
            </div>
          </div>

          <!-- 文本预览备用 iframe -->
          <iframe
            v-else-if="previewKind === 'text'"
            class="lan-share-preview-frame"
            sandbox
            :src="previewUrl"
            :title="file?.name || '文本预览'"
          ></iframe>

          <!-- 暂不支持在线预览的文件格式 -->
          <div v-else class="lan-share-preview-unsupported">
            <div class="lan-share-preview-unsupported-icon">
              <File :size="42" />
            </div>
            <h3 class="lan-share-preview-unsupported-title">
              暂不支持在应用内直接预览
            </h3>
            <p class="lan-share-preview-unsupported-desc">
              该文件格式（{{
                file?.mimeType || "未知格式"
              }}）暂无法在线解析。<br />
              您可以下载到本地后，使用相应的专业程序打开。
            </p>
            <button
              class="lan-share-preview-unsupported-btn"
              type="button"
              :disabled="saving"
              @click="emit('download', file)"
            >
              <Download :size="15" />
              <span>{{ saving ? "正在保存…" : "立即下载" }}</span>
            </button>
          </div>
        </div>

        <!-- 底部栏 -->
        <footer class="lan-share-preview-actions">
          <div class="lan-share-preview-hint">
            <span>文件已就绪 · 支持保存到本地任意目录</span>
          </div>
          <div class="lan-share-preview-buttons">
            <button
              class="lan-share-preview-btn lan-share-preview-btn--cancel"
              type="button"
              @click="emit('close')"
            >
              关闭
            </button>
            <button
              class="lan-share-preview-btn lan-share-preview-btn--primary"
              type="button"
              :disabled="saving"
              @click="emit('download', file)"
            >
              <Download :size="15" />
              <span>{{ saving ? "保存中…" : "下载文件" }}</span>
            </button>
          </div>
        </footer>
      </div>
    </section>
  </Teleport>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  Check,
  Copy,
  Download,
  File,
  FileCode,
  FileText,
  Film,
  Image,
  Music,
  X
} from "lucide-vue-next"
import { useZIndex } from "element-plus"

const { nextZIndex } = useZIndex()
const zIndex = nextZIndex()

const props = defineProps({
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
  },
  saving: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(["close", "download"])

const textCopied = ref(false)
let copyTimer = null

const textLines = computed(() => {
  if (!props.textContent) return []
  return props.textContent.split(/\r?\n/)
})

async function copyTextContent() {
  const content = props.textContent
  if (!content) return
  try {
    if (navigator?.clipboard?.writeText) {
      await navigator.clipboard.writeText(content)
    } else {
      const textarea = document.createElement("textarea")
      textarea.value = content
      textarea.style.position = "fixed"
      textarea.style.opacity = "0"
      document.body.appendChild(textarea)
      textarea.select()
      document.execCommand("copy")
      document.body.removeChild(textarea)
    }
    textCopied.value = true
    clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      textCopied.value = false
    }, 2000)
  } catch (error) {
    console.error("复制失败", error)
  }
}

function getKindIcon(kind) {
  switch (kind) {
    case "image":
      return Image
    case "video":
      return Film
    case "audio":
      return Music
    case "text":
      return FileCode
    case "pdf":
      return FileText
    default:
      return File
  }
}

function getKindLabel(kind) {
  switch (kind) {
    case "image":
      return "图片预览"
    case "video":
      return "视频文件"
    case "audio":
      return "音频文件"
    case "text":
      return "代码与文本"
    case "pdf":
      return "PDF 文档"
    default:
      return "二进制文件"
  }
}

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
  display: grid;
  place-items: center;
  padding: 24px;

  .lan-share-preview-overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.45);
    backdrop-filter: blur(8px);
  }

  .lan-share-preview-panel {
    position: relative;
    display: flex;
    width: min(940px, calc(100vw - 48px));
    max-height: min(840px, calc(100vh - 48px));
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 18px;
    background: var(--color-panel);
    box-shadow:
      0 24px 64px -12px rgba(15, 23, 42, 0.24),
      0 0 0 1px rgba(0, 0, 0, 0.04);

    .lan-share-preview-head {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 18px 24px;
      border-bottom: 1px solid var(--color-line);

      &-left {
        display: flex;
        align-items: center;
        gap: 14px;
        min-width: 0;
      }

      .lan-share-preview-avatar {
        display: grid;
        place-items: center;
        width: 42px;
        height: 42px;
        border-radius: 10px;
        flex-shrink: 0;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);

        &.is-image {
          background: #eff6ff;
          color: #2563eb;
        }

        &.is-video {
          background: #f5f3ff;
          color: #7c3aed;
        }

        &.is-audio {
          background: #fef3c7;
          color: #d97706;
        }

        &.is-text {
          background: #ecfdf5;
          color: #059669;
        }

        &.is-pdf {
          background: #fff1f2;
          color: #e11d48;
        }
      }

      .lan-share-preview-title-box {
        display: flex;
        flex-direction: column;
        gap: 4px;
        min-width: 0;

        .lan-share-preview-filename {
          margin: 0;
          font-size: 16.5px;
          font-weight: 700;
          color: var(--color-text);
          letter-spacing: -0.01em;
          line-height: 1.3;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }

        .lan-share-preview-meta-row {
          display: flex;
          align-items: center;
          gap: 6px;
          flex-wrap: wrap;

          .lan-share-preview-badge {
            display: inline-flex;
            align-items: center;
            padding: 2px 8px;
            border-radius: 5px;
            font-size: 11.5px;
            font-weight: 500;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            border: 1px solid var(--color-line);
            font-variant-numeric: tabular-nums;

            &--kind {
              background: #eff6ff;
              color: #2563eb;
              border-color: #bfdbfe;
            }
          }
        }
      }

      .lan-share-preview-close {
        display: grid;
        place-items: center;
        width: 32px;
        height: 32px;
        flex-shrink: 0;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.15s ease;

        &:hover {
          background: var(--color-panel-soft);
          border-color: var(--color-line-strong);
          color: var(--color-text);
        }

        &:focus-visible {
          outline: 2px solid var(--color-primary);
          outline-offset: 2px;
        }
      }
    }

    .lan-share-preview-body {
      display: flex;
      min-height: 360px;
      max-height: calc(84vh - 160px);
      flex: 1;
      overflow: hidden;
      position: relative;
      background: var(--color-panel-soft);

      /* 图片 */
      .lan-share-preview-image-wrap {
        display: flex;
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        padding: 24px;
        overflow: auto;
        background-color: var(--color-panel-soft);
        background-image:
          linear-gradient(
            45deg,
            rgba(148, 163, 184, 0.08) 25%,
            transparent 25%
          ),
          linear-gradient(
            -45deg,
            rgba(148, 163, 184, 0.08) 25%,
            transparent 25%
          ),
          linear-gradient(
            45deg,
            transparent 75%,
            rgba(148, 163, 184, 0.08) 75%
          ),
          linear-gradient(
            -45deg,
            transparent 75%,
            rgba(148, 163, 184, 0.08) 75%
          );
        background-size: 16px 16px;
        background-position:
          0 0,
          0 8px,
          8px -8px,
          -8px 0;

        .lan-share-preview-image {
          display: block;
          max-width: 100%;
          max-height: 64vh;
          object-fit: contain;
          border-radius: 8px;
          box-shadow: 0 10px 30px -8px rgba(15, 23, 42, 0.18);
        }
      }

      /* 视频 */
      .lan-share-preview-video-wrap {
        display: flex;
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        background: #090d16;
        padding: 20px;

        .lan-share-preview-video {
          max-width: 100%;
          max-height: 64vh;
          border-radius: 10px;
          outline: none;
        }
      }

      /* 音频 */
      .lan-share-preview-audio-wrap {
        display: flex;
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        padding: 32px 24px;

        .lan-share-preview-audio-card {
          display: flex;
          flex-direction: column;
          align-items: center;
          width: 100%;
          max-width: 440px;
          padding: 28px 24px;
          border-radius: 16px;
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          box-shadow: 0 12px 32px -8px rgba(15, 23, 42, 0.08);

          .lan-share-preview-audio-disc {
            position: relative;
            display: grid;
            place-items: center;
            width: 80px;
            height: 80px;
            border-radius: 50%;
            background: linear-gradient(135deg, #fef3c7 0%, #fde68a 100%);
            color: #d97706;
            box-shadow: 0 8px 24px rgba(217, 119, 6, 0.2);

            .lan-share-preview-sound-waves {
              position: absolute;
              bottom: -10px;
              display: flex;
              align-items: flex-end;
              gap: 3px;
              height: 18px;

              .wave-bar {
                width: 3.5px;
                background: #f59e0b;
                border-radius: 9999px;
                animation: soundWave 1.2s ease-in-out infinite;

                &:nth-child(1) {
                  height: 6px;
                  animation-delay: 0.1s;
                }
                &:nth-child(2) {
                  height: 14px;
                  animation-delay: 0.3s;
                }
                &:nth-child(3) {
                  height: 18px;
                  animation-delay: 0.2s;
                }
                &:nth-child(4) {
                  height: 11px;
                  animation-delay: 0.4s;
                }
                &:nth-child(5) {
                  height: 7px;
                  animation-delay: 0.15s;
                }
              }
            }
          }

          .lan-share-preview-audio-info {
            display: flex;
            flex-direction: column;
            align-items: center;
            gap: 4px;
            margin: 22px 0 18px;
            text-align: center;

            .audio-name {
              font-size: 15px;
              font-weight: 600;
              color: var(--color-text);
              max-width: 360px;
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .audio-meta {
              font-size: 12.5px;
              color: var(--color-text-muted);
            }
          }

          .lan-share-preview-audio-player {
            width: 100%;
            height: 38px;
            outline: none;
          }
        }
      }

      /* PDF */
      .lan-share-preview-pdf-wrap {
        width: 100%;
        height: 68vh;

        .lan-share-preview-frame {
          width: 100%;
          height: 100%;
          border: 0;
          background: var(--color-panel);
        }
      }

      /* 文本与代码 */
      .lan-share-preview-code-wrap {
        display: flex;
        flex-direction: column;
        width: 100%;
        height: 100%;
        background: var(--color-panel);

        .lan-share-preview-code-toolbar {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 8px 16px;
          border-bottom: 1px solid var(--color-line);
          background: var(--color-panel-soft);
          font-size: 12px;

          .lan-share-preview-code-meta {
            display: flex;
            align-items: center;
            gap: 12px;
            color: var(--color-text-muted);

            .meta-tag {
              display: inline-flex;
              align-items: center;
              padding: 1px 7px;
              border-radius: 4px;
              background: #ecfdf5;
              color: #059669;
              font-weight: 600;
              font-size: 11px;
            }
          }

          .lan-share-preview-copy-btn {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            padding: 4px 10px;
            border-radius: 6px;
            border: 1px solid var(--color-line);
            background: var(--color-panel);
            color: var(--color-text);
            font-size: 12px;
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover {
              background: var(--color-panel-soft);
              border-color: var(--color-line-strong);
            }

            &.is-copied {
              border-color: #10b981;
              background: #ecfdf5;
              color: #059669;
            }
          }
        }

        .lan-share-preview-code-scroll {
          display: flex;
          flex: 1;
          overflow: auto;
          font-family:
            ui-monospace, "SF Mono", Menlo, Consolas, "JetBrains Mono",
            monospace;
          font-size: 12.5px;
          line-height: 1.65;

          &::-webkit-scrollbar {
            width: 6px;
            height: 6px;
          }
          &::-webkit-scrollbar-track {
            background: transparent;
          }
          &::-webkit-scrollbar-thumb {
            background: rgba(148, 163, 184, 0.4);
            border-radius: 9999px;

            &:hover {
              background: rgba(100, 116, 139, 0.6);
            }
          }

          .lan-share-preview-gutter {
            display: flex;
            flex-direction: column;
            padding: 14px 12px 14px 16px;
            border-right: 1px solid var(--color-line);
            background: var(--color-panel-soft);
            user-select: none;
            text-align: right;
            color: var(--color-text-soft);

            .gutter-num {
              min-width: 24px;
            }
          }

          .lan-share-preview-code-content {
            margin: 0;
            padding: 14px 18px;
            color: var(--color-text);
            white-space: pre;
            word-break: normal;
            flex: 1;
          }
        }
      }

      /* 备用 frame */
      .lan-share-preview-frame {
        width: 100%;
        height: 64vh;
        border: 0;
        background: var(--color-panel);
      }

      /* 不支持格式空状态 */
      .lan-share-preview-unsupported {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        width: 100%;
        padding: 48px 24px;
        text-align: center;

        &-icon {
          display: grid;
          place-items: center;
          width: 72px;
          height: 72px;
          border-radius: 16px;
          background: var(--color-panel);
          border: 1px solid var(--color-line);
          color: var(--color-text-muted);
          box-shadow: 0 8px 24px rgba(15, 23, 42, 0.06);
          margin-bottom: 18px;
        }

        &-title {
          margin: 0;
          font-size: 16px;
          font-weight: 700;
          color: var(--color-text);
        }

        &-desc {
          margin: 10px 0 22px;
          font-size: 13.5px;
          color: var(--color-text-muted);
          line-height: 1.6;
          max-width: 440px;
        }

        &-btn {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          height: 38px;
          padding: 0 22px;
          border-radius: 8px;
          border: none;
          background: #1677ff;
          color: #ffffff;
          font-size: 13.5px;
          font-weight: 500;
          cursor: pointer;
          box-shadow: 0 2px 8px rgba(22, 119, 255, 0.28);
          transition: all 0.15s ease;

          &:hover:not(:disabled) {
            filter: brightness(1.08);
            box-shadow: 0 4px 12px rgba(22, 119, 255, 0.36);
          }
        }
      }
    }

    .lan-share-preview-actions {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 14px 24px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel);

      .lan-share-preview-hint {
        font-size: 12.5px;
        color: var(--color-text-muted);
      }

      .lan-share-preview-buttons {
        display: flex;
        align-items: center;
        gap: 10px;

        .lan-share-preview-btn {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 6px;
          height: 36px;
          padding: 0 18px;
          border-radius: 8px;
          font-size: 13.5px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s ease;

          &--cancel {
            border: 1px solid var(--color-line);
            background: var(--color-panel);
            color: var(--color-text);

            &:hover {
              background: var(--color-panel-soft);
              border-color: var(--color-line-strong);
            }
          }

          &--primary {
            border: none;
            background: #1677ff;
            color: #ffffff;
            box-shadow: 0 2px 6px rgba(22, 119, 255, 0.25);

            &:hover:not(:disabled) {
              filter: brightness(1.08);
              box-shadow: 0 4px 12px rgba(22, 119, 255, 0.35);
            }

            &:disabled {
              opacity: 0.6;
              cursor: not-allowed;
              box-shadow: none;
            }
          }
        }
      }
    }
  }
}

@keyframes soundWave {
  0%,
  100% {
    height: 4px;
  }
  50% {
    height: 18px;
  }
}

:root[data-theme="dark"] {
  .lan-share-preview-dialog {
    .lan-share-preview-panel {
      .lan-share-preview-head {
        .lan-share-preview-avatar {
          &.is-image {
            background: rgba(37, 99, 235, 0.2);
            color: #60a5fa;
          }
          &.is-video {
            background: rgba(124, 58, 237, 0.2);
            color: #a78bfa;
          }
          &.is-audio {
            background: rgba(217, 119, 6, 0.2);
            color: #fcd34d;
          }
          &.is-text {
            background: rgba(5, 150, 105, 0.2);
            color: #34d399;
          }
          &.is-pdf {
            background: rgba(225, 29, 72, 0.2);
            color: #fb7185;
          }
        }

        .lan-share-preview-close {
          background: rgba(255, 255, 255, 0.05);
          border-color: var(--color-line);

          &:hover {
            background: rgba(255, 255, 255, 0.1);
            border-color: var(--color-line-strong);
          }
        }
      }

      .lan-share-preview-body {
        .lan-share-preview-image-wrap {
          background-image:
            linear-gradient(
              45deg,
              rgba(255, 255, 255, 0.03) 25%,
              transparent 25%
            ),
            linear-gradient(
              -45deg,
              rgba(255, 255, 255, 0.03) 25%,
              transparent 25%
            ),
            linear-gradient(
              45deg,
              transparent 75%,
              rgba(255, 255, 255, 0.03) 75%
            ),
            linear-gradient(
              -45deg,
              transparent 75%,
              rgba(255, 255, 255, 0.03) 75%
            );
        }

        .lan-share-preview-audio-wrap .lan-share-preview-audio-card {
          background: rgba(30, 41, 59, 0.6);
          border-color: rgba(255, 255, 255, 0.08);

          .lan-share-preview-audio-disc {
            background: linear-gradient(
              135deg,
              rgba(217, 119, 6, 0.4) 0%,
              rgba(180, 83, 9, 0.5) 100%
            );
            color: #fbbf24;
          }
        }

        .lan-share-preview-code-wrap {
          background: rgba(15, 23, 42, 0.7);

          .lan-share-preview-code-toolbar {
            background: rgba(15, 23, 42, 0.9);

            .meta-tag {
              background: rgba(5, 150, 105, 0.2);
              color: #34d399;
            }

            .lan-share-preview-copy-btn {
              background: rgba(255, 255, 255, 0.06);
              border-color: rgba(255, 255, 255, 0.12);
              color: #e2e8f0;

              &:hover {
                background: rgba(255, 255, 255, 0.1);
              }

              &.is-copied {
                background: rgba(16, 185, 129, 0.2);
                border-color: rgba(16, 185, 129, 0.4);
                color: #34d399;
              }
            }
          }

          .lan-share-preview-code-scroll {
            .lan-share-preview-gutter {
              background: rgba(15, 23, 42, 0.85);
              border-color: rgba(255, 255, 255, 0.08);
            }
          }
        }

        .lan-share-preview-unsupported {
          &-icon {
            background: rgba(30, 41, 59, 0.6);
            border-color: rgba(255, 255, 255, 0.08);
          }
        }
      }

      .lan-share-preview-actions {
        .lan-share-preview-btn--cancel {
          background: rgba(255, 255, 255, 0.06);
          border-color: rgba(255, 255, 255, 0.12);
          color: #e2e8f0;

          &:hover {
            background: rgba(255, 255, 255, 0.1);
          }
        }
      }
    }
  }
}
</style>

<template>
  <BaseModal
    v-if="detail"
    :class="{
      'image-detail-modal': detailView === 'images',
      'params-detail-modal': detailView === 'parameters'
    }"
    :title="detailView === 'parameters' ? '任务参数' : '生成图片'"
    :description="
      detail.accountName +
      ' · ' +
      formatDateTime(detail.createdAt) +
      ' · ' +
      (statusLabels[detail.status] || detail.status)
    "
    @close="$emit('close')"
  >
    <section v-if="detailView === 'images'" class="detail-content">
      <p v-if="detail.error?.message" class="detail-error">
        {{ detail.error.message }}
      </p>
      <div class="detail-images">
        <figure
          v-for="(item, index) in detail.data"
          :key="index"
          class="detail-image-card"
        >
          <el-image
            class="detail-image"
            :src="imageUrl(item)"
            :preview-src-list="detail.data.map(imageUrl)"
            :initial-index="index"
            fit="contain"
            preview-teleported
          />
          <figcaption class="detail-caption">
            <div class="detail-image-info">
              <p class="detail-info-title">图片信息</p>
              <dl class="detail-info-list">
                <div class="detail-info-field">
                  <dt class="detail-info-label">尺寸</dt>
                  <dd class="detail-info-value">
                    {{ detail.images[index]?.size || "未知" }}
                  </dd>
                </div>
                <div class="detail-info-field">
                  <dt class="detail-info-label">格式</dt>
                  <dd class="detail-info-value">
                    {{ item.output_format || "未知" }}
                  </dd>
                </div>
                <div class="detail-info-field">
                  <dt class="detail-info-label">调用模式</dt>
                  <dd class="detail-info-value">
                    {{
                      detail.request.generationMode === "web" ? "Web" : "Codex"
                    }}
                  </dd>
                </div>
                <div class="detail-info-field">
                  <dt class="detail-info-label">模型</dt>
                  <dd class="detail-info-value">
                    {{ detail.request.model }}
                  </dd>
                </div>
              </dl>
            </div>
            <div class="detail-image-actions">
              <button
                class="action-button"
                type="button"
                @click="$emit('edit-result', item)"
              >
                <Paintbrush :size="14" />编辑
              </button>
              <button
                class="action-button"
                type="button"
                @click="$emit('reference-result', item)"
              >
                <ImagePlus :size="14" />引用
              </button>
              <button
                class="action-button"
                type="button"
                :disabled="exporting"
                @click="$emit('download-image', index)"
              >
                <Download :size="14" />{{ exporting ? "下载中…" : "下载" }}
              </button>
            </div>
          </figcaption>
        </figure>
      </div>
    </section>

    <div v-else class="params-modal-container">
      <div class="params-modal-body">
        <div
          v-if="detail.error?.message"
          class="params-error-alert"
          role="alert"
        >
          <AlertCircle :size="16" class="alert-icon" />
          <div class="alert-content">
            <span class="alert-title">任务遇到异常</span>
            <p class="alert-message">{{ detail.error.message }}</p>
          </div>
        </div>

        <section class="params-card prompt-card">
          <header class="card-header">
            <div class="card-title">
              <FileText :size="15" class="title-icon" />
              <span>生成提示词</span>
              <span v-if="detail.request.prompt" class="char-count-pill"
                >{{ detail.request.prompt.length }} 字符</span
              >
            </div>
            <button
              class="card-action-btn"
              type="button"
              :title="copiedPromptInModal ? '已复制' : '复制提示词'"
              @click="
                copyText(
                  detail.request.prompt,
                  copiedPromptInModal,
                  '提示词已复制到剪切板'
                )
              "
            >
              <Check
                v-if="copiedPromptInModal"
                :size="13"
                class="copy-success-icon"
              />
              <Copy v-else :size="13" />
              <span>{{ copiedPromptInModal ? "已复制" : "复制提示词" }}</span>
            </button>
          </header>
          <div class="prompt-display-box">
            <p class="prompt-text">
              {{ detail.request.prompt || "（无提示词）" }}
            </p>
          </div>
        </section>

        <section class="params-card specs-card">
          <header class="card-header">
            <div class="card-title">
              <Cpu :size="15" class="title-icon" />
              <span>配置与规格</span>
            </div>
          </header>
          <div class="specs-grid">
            <div class="spec-tile">
              <span class="spec-label">模型</span>
              <span class="spec-value mono-val" :title="detail.request.model">{{
                detail.request.model
              }}</span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">调用模式</span>
              <span class="spec-value">
                <span
                  class="mode-tag"
                  :class="
                    detail.request.generationMode === 'web' ? 'web' : 'codex'
                  "
                >
                  {{
                    detail.request.generationMode === "web" ? "Web" : "Codex"
                  }}
                </span>
              </span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">任务类型</span>
              <span class="spec-value">{{
                detail.request.mode === "edit" ? "图片编辑" : "文生图"
              }}</span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">尺寸与比例</span>
              <span class="spec-value mono-val">
                {{ detail.request.size }}
                <span v-if="detail.request.ratio" class="sub-pill">{{
                  detail.request.ratio
                }}</span>
              </span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">画面质量</span>
              <span class="spec-value">{{
                formatQuality(detail.request.quality)
              }}</span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">任务状态</span>
              <span class="spec-value status-val" :class="detail.status">{{
                statusLabels[detail.status] || detail.status
              }}</span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">生成数量</span>
              <span class="spec-value">
                请求 {{ detail.request.n }} 张 · 实际 {{ detail.imageCount }} 张
              </span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">格式 / 响应</span>
              <span class="spec-value uppercase-val">
                {{ detail.request.outputFormat }}
                <span class="sub-pill">{{
                  detail.request.responseFormat
                }}</span>
              </span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">背景模式</span>
              <span class="spec-value">{{
                detail.request.background || "默认"
              }}</span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">参考图</span>
              <span class="spec-value">{{
                detail.inputCount ? detail.inputCount + " 张参考图" : "无"
              }}</span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">重绘蒙版</span>
              <span class="spec-value">
                <span class="mask-tag" :class="{ 'has-mask': detail.hasMask }">
                  {{ detail.hasMask ? "已应用蒙版" : "无" }}
                </span>
              </span>
            </div>
            <div class="spec-tile">
              <span class="spec-label">执行账号</span>
              <span
                class="spec-value text-ellipsis"
                :title="detail.accountName"
                >{{ detail.accountName || "默认账号" }}</span
              >
            </div>
          </div>
        </section>

        <section class="params-card technical-card">
          <details class="tech-accordion">
            <summary class="tech-summary">
              <div class="tech-summary-left">
                <Terminal :size="15" class="title-icon" />
                <span>请求与用量记录</span>
                <ChevronDown :size="13" class="tech-chevron" />
                <span class="tech-summary-hint">点击展开底层追踪数据</span>
              </div>
              <button
                class="card-action-btn"
                type="button"
                :title="copiedTechnicalInModal ? '已复制' : '复制 JSON'"
                @click.stop="
                  copyText(
                    technicalJson,
                    copiedTechnicalInModal,
                    '用量记录已复制为 JSON'
                  )
                "
              >
                <Check
                  v-if="copiedTechnicalInModal"
                  :size="13"
                  class="copy-success-icon"
                />
                <Copy v-else :size="13" />
                <span>{{
                  copiedTechnicalInModal ? "已复制" : "复制 JSON"
                }}</span>
              </button>
            </summary>
            <div class="tech-body">
              <div class="tech-quick-ids">
                <div v-if="detail.id" class="quick-id-item">
                  <span class="id-label">Task ID:</span>
                  <span class="id-val mono-val">{{ detail.id }}</span>
                </div>
                <div v-if="detail.requestId" class="quick-id-item">
                  <span class="id-label">Request ID:</span>
                  <span class="id-val mono-val">{{ detail.requestId }}</span>
                </div>
                <div v-if="detail.endpoint" class="quick-id-item">
                  <span class="id-label">Endpoint:</span>
                  <span class="id-val mono-val">{{ detail.endpoint }}</span>
                </div>
              </div>
              <pre class="json-code-box"><code>{{ technicalJson }}</code></pre>
            </div>
          </details>
        </section>

        <div
          v-if="detail.request.responseFormat === 'url' && detail.imageCount"
          class="params-hint-banner"
        >
          <Info :size="14" class="hint-icon" />
          <span
            >图片保存在本地；URL 为预览用 data URL，不是可分享的公网链接。</span
          >
        </div>
      </div>

      <footer class="params-modal-footer">
        <div class="footer-left">
          <button
            class="action-button modal-footer-btn"
            type="button"
            :title="copiedAllParamsInModal ? '已复制' : '复制全部参数为 JSON'"
            @click="
              copyText(
                allParamsJson,
                copiedAllParamsInModal,
                '全部任务参数已复制为 JSON'
              )
            "
          >
            <Check
              v-if="copiedAllParamsInModal"
              :size="14"
              class="copy-success-icon"
            />
            <Copy v-else :size="14" />
            <span>{{
              copiedAllParamsInModal ? "已复制全部" : "复制全部参数"
            }}</span>
          </button>
        </div>
        <div class="footer-right">
          <button
            class="action-button modal-footer-btn"
            type="button"
            @click="$emit('close')"
          >
            关闭
          </button>
          <button
            class="action-button modal-footer-btn"
            type="button"
            title="将本任务提示词及规格参数填入左侧表单"
            @click="$emit('reuse')"
          >
            <SlidersHorizontal :size="14" />
            <span>复用参数到表单</span>
          </button>
          <button
            class="action-button modal-footer-btn primary-btn"
            type="button"
            :disabled="
              submitting || ['queued', 'processing'].includes(detail.status)
            "
            title="以此任务参数重新发起生成"
            @click="$emit('regenerate')"
          >
            <RefreshCw :size="14" :class="{ spinning: submitting }" />
            <span>重新生成</span>
          </button>
        </div>
      </footer>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, toRefs } from "vue"
import {
  AlertCircle,
  Check,
  ChevronDown,
  Copy,
  Cpu,
  Download,
  FileText,
  ImagePlus,
  Info,
  Paintbrush,
  RefreshCw,
  SlidersHorizontal,
  Terminal
} from "lucide-vue-next"
import { ElImage } from "element-plus"
import "element-plus/es/components/image/style/css"
import BaseModal from "@/components/BaseModal.vue"
import { createMessage } from "@/utils/message"

const props = defineProps({
  detail: {
    type: Object,
    default: null
  },
  detailView: {
    type: String,
    default: "images"
  },
  statusLabels: {
    type: Object,
    default: () => ({})
  },
  formatDateTime: {
    type: Function,
    required: true
  },
  formatQuality: {
    type: Function,
    required: true
  },
  imageUrl: {
    type: Function,
    required: true
  },
  exporting: {
    type: Boolean,
    default: false
  },
  submitting: {
    type: Boolean,
    default: false
  }
})

defineEmits([
  "close",
  "edit-result",
  "reference-result",
  "download-image",
  "reuse",
  "regenerate"
])

const {
  detail,
  detailView,
  statusLabels,
  formatDateTime,
  formatQuality,
  imageUrl,
  exporting,
  submitting
} = toRefs(props)

const copiedPromptInModal = ref(false)
const copiedTechnicalInModal = ref(false)
const copiedAllParamsInModal = ref(false)
const copyTimers = new Map()

const technicalJson = computed(() =>
  JSON.stringify(
    {
      taskId: detail.value?.id,
      requestId: detail.value?.requestId,
      endpoint: detail.value?.endpoint,
      usage: detail.value?.usage
    },
    null,
    2
  )
)
const allParamsJson = computed(() =>
  JSON.stringify(
    {
      id: detail.value?.id,
      status: detail.value?.status,
      createdAt: detail.value?.createdAt,
      accountName: detail.value?.accountName,
      request: detail.value?.request,
      imageCount: detail.value?.imageCount,
      inputCount: detail.value?.inputCount,
      hasMask: detail.value?.hasMask,
      error: detail.value?.error,
      usage: detail.value?.usage
    },
    null,
    2
  )
)

async function copyText(text, state, successMessage) {
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    state.value = true
    const previousTimer = copyTimers.get(state)
    if (previousTimer) clearTimeout(previousTimer)
    copyTimers.set(
      state,
      setTimeout(() => {
        state.value = false
        copyTimers.delete(state)
      }, 1500)
    )
    createMessage.success(successMessage)
  } catch (error) {
    createMessage.error("复制失败: " + (error?.message || String(error)))
  }
}

onBeforeUnmount(() => {
  copyTimers.forEach((timer) => clearTimeout(timer))
  copyTimers.clear()
})
</script>

<style scoped lang="less">
.action-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  height: 31px;
  padding: 0 10px;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  color: var(--color-text);
  background: var(--color-panel-soft);
  cursor: pointer;
  font-size: var(--font-size-sm);

  &:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
}

.spinning {
  animation: image-workbench-spin 1.5s linear infinite;
}

.image-detail-modal {
  :deep(.base-modal__panel) {
    width: min(1000px, calc(100vw - 48px));
  }
}

.params-detail-modal {
  :deep(.base-modal__panel) {
    width: min(820px, calc(100vw - 36px));
    max-height: min(88vh, 880px);
  }

  :deep(.base-modal__content) {
    padding: 0;
  }
}

.detail-content {
  overflow: auto;

  .detail-error {
    color: var(--color-danger);
  }

  .detail-images {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;

    .detail-image-card {
      display: grid;
      grid-template-columns: minmax(0, 3fr) minmax(0, 1fr);
      margin: 0;
      width: 100%;
      height: min(60vh, 640px);

      .detail-image {
        display: block;
        box-sizing: border-box;
        min-height: 0;
        width: 100%;
        height: 100%;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel-soft);
      }

      .detail-caption {
        display: flex;
        flex-direction: column;
        gap: 16px;
        min-width: 0;
        min-height: 0;
        padding-left: 16px;

        .detail-image-info {
          flex: 1;
          min-height: 0;
          overflow: auto;

          .detail-info-title {
            margin: 0 0 12px;
            color: var(--color-text);
          }

          .detail-info-list {
            display: grid;
            gap: 12px;
            margin: 0;
            font-size: var(--font-size-sm);

            .detail-info-field {
              min-width: 0;

              .detail-info-label {
                margin-bottom: 4px;
                color: var(--color-text-muted);
              }

              .detail-info-value {
                margin: 0;
                overflow-wrap: anywhere;
                color: var(--color-text);
              }
            }
          }
        }

        .detail-image-actions {
          display: flex;
          flex: none;
          flex-direction: column;
          gap: 8px;
          padding-top: 16px;
          border-top: 1px solid var(--color-line);
        }
      }
    }
  }
}

.params-modal-container {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow: hidden;

  .params-modal-body {
    flex: 1;
    overflow-y: auto;
    padding: 6px 24px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;

    .params-error-alert {
      display: flex;
      align-items: flex-start;
      gap: 10px;
      padding: 12px 14px;
      border-radius: 8px;
      background: rgba(239, 68, 68, 0.08);
      border: 1px solid rgba(239, 68, 68, 0.25);
      color: var(--color-danger);

      .alert-icon {
        flex-shrink: 0;
        margin-top: 2px;
      }

      .alert-content {
        flex: 1;
        min-width: 0;

        .alert-title {
          display: block;
          font-weight: 600;
          font-size: var(--font-size-sm);
          margin-bottom: 2px;
        }

        .alert-message {
          margin: 0;
          font-size: 13px;
          line-height: 1.5;
          overflow-wrap: anywhere;
          color: var(--color-text);
        }
      }
    }

    .params-card {
      border: 1px solid var(--color-line);
      border-radius: 9px;
      background: var(--color-panel-soft);
      padding: 14px 16px;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);

      .card-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        margin-bottom: 12px;

        .card-title {
          display: flex;
          align-items: center;
          gap: 7px;
          font-size: 13px;
          font-weight: 600;
          color: var(--color-text);

          .title-icon {
            color: var(--color-primary);
          }

          .char-count-pill {
            padding: 1px 7px;
            border-radius: 10px;
            background: var(--color-panel);
            border: 1px solid var(--color-line);
            font-size: 11px;
            font-weight: 400;
            color: var(--color-text-muted);
          }
        }

        .card-action-btn {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          height: 26px;
          padding: 0 9px;
          border: 1px solid var(--color-line);
          border-radius: 5px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          font-size: 12px;
          cursor: pointer;
          transition: all 0.15s;

          &:hover {
            color: var(--color-text);
            border-color: var(--color-line-strong);
          }

          .copy-success-icon {
            color: var(--color-success, #10b981);
          }
        }
      }

      &.prompt-card {
        .prompt-display-box {
          padding: 12px 14px;
          background: var(--color-panel);
          border: 1px solid var(--color-line);
          border-radius: 7px;
          max-height: 200px;
          overflow-y: auto;

          .prompt-text {
            margin: 0;
            font-size: 13px;
            line-height: 1.7;
            color: var(--color-text);
            white-space: pre-wrap;
            word-break: break-word;
            user-select: text;
          }
        }
      }

      &.specs-card {
        .specs-grid {
          display: grid;
          grid-template-columns: repeat(3, 1fr);
          gap: 10px;

          @media (max-width: 680px) {
            grid-template-columns: repeat(2, 1fr);
          }

          .spec-tile {
            display: flex;
            flex-direction: column;
            gap: 5px;
            padding: 10px 12px;
            background: var(--color-panel);
            border: 1px solid var(--color-line);
            border-radius: 6px;
            min-width: 0;

            .spec-label {
              font-size: 11px;
              color: var(--color-text-muted);
            }

            .spec-value {
              font-size: 13px;
              font-weight: 500;
              color: var(--color-text);
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;

              &.mono-val {
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;
                font-size: 12px;
              }

              &.uppercase-val {
                text-transform: uppercase;
                font-size: 12px;
              }

              &.text-ellipsis {
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
              }

              .sub-pill {
                display: inline-block;
                margin-left: 4px;
                padding: 1px 5px;
                border-radius: 3px;
                background: var(--color-panel-soft);
                border: 1px solid var(--color-line);
                font-size: 10px;
                font-family: inherit;
                color: var(--color-text-muted);
                vertical-align: baseline;
              }

              .mode-tag {
                display: inline-block;
                padding: 1px 7px;
                border-radius: 4px;
                font-size: 11px;
                font-weight: 600;

                &.web {
                  background: rgba(16, 185, 129, 0.1);
                  color: var(--color-success, #10b981);
                  border: 1px solid rgba(16, 185, 129, 0.25);
                }

                &.codex {
                  background: rgba(59, 130, 246, 0.1);
                  color: var(--color-primary, #3b82f6);
                  border: 1px solid rgba(59, 130, 246, 0.25);
                }
              }

              .mask-tag {
                display: inline-block;
                padding: 1px 6px;
                border-radius: 4px;
                font-size: 11px;
                color: var(--color-text-muted);

                &.has-mask {
                  background: rgba(139, 92, 246, 0.1);
                  color: #8b5cf6;
                  border: 1px solid rgba(139, 92, 246, 0.25);
                  font-weight: 500;
                }
              }

              &.status-val {
                &.completed {
                  color: var(--color-success, #10b981);
                }

                &.processing {
                  color: var(--color-primary, #3b82f6);
                }

                &.failed {
                  color: var(--color-danger, #ef4444);
                }

                &.partial,
                &.interrupted {
                  color: var(--color-warning, #f59e0b);
                }
              }
            }
          }
        }
      }

      &.technical-card {
        padding: 0;
        overflow: hidden;

        .tech-accordion {
          .tech-summary {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 12px 16px;
            cursor: pointer;
            user-select: none;
            font-size: 13px;
            font-weight: 600;
            color: var(--color-text);
            outline: none;
            list-style: none;

            &::-webkit-details-marker {
              display: none;
            }

            .tech-summary-left {
              display: flex;
              align-items: center;
              gap: 8px;

              .title-icon {
                color: var(--color-primary);
              }

              .tech-chevron {
                color: var(--color-text-soft);
                transition: transform 0.2s ease;
              }

              .tech-summary-hint {
                font-size: 11px;
                font-weight: 400;
                color: var(--color-text-soft);
                margin-left: 4px;
              }
            }

            .card-action-btn {
              display: inline-flex;
              align-items: center;
              gap: 4px;
              height: 26px;
              padding: 0 9px;
              border: 1px solid var(--color-line);
              border-radius: 5px;
              background: var(--color-panel);
              color: var(--color-text-muted);
              font-size: 12px;
              cursor: pointer;
              transition: all 0.15s;

              &:hover {
                color: var(--color-text);
                border-color: var(--color-line-strong);
              }

              .copy-success-icon {
                color: var(--color-success, #10b981);
              }
            }
          }

          &[open] {
            .tech-chevron {
              transform: rotate(180deg);
            }
          }

          .tech-body {
            padding: 0 16px 14px;
            border-top: 1px solid var(--color-line);
            display: flex;
            flex-direction: column;
            gap: 10px;

            .tech-quick-ids {
              display: flex;
              flex-wrap: wrap;
              gap: 12px;
              padding-top: 10px;

              .quick-id-item {
                display: inline-flex;
                align-items: center;
                gap: 5px;
                font-size: 11px;

                .id-label {
                  color: var(--color-text-muted);
                }

                .id-val {
                  color: var(--color-text);
                  font-family:
                    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                    monospace;
                }
              }
            }

            .json-code-box {
              margin: 0;
              padding: 12px;
              border-radius: 6px;
              background: var(--color-panel);
              border: 1px solid var(--color-line);
              font-family:
                ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
              font-size: 12px;
              line-height: 1.6;
              color: var(--color-text);
              white-space: pre-wrap;
              word-break: break-all;
              max-height: 220px;
              overflow-y: auto;
            }
          }
        }
      }
    }

    .params-hint-banner {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      border-radius: 6px;
      background: var(--color-panel-soft);
      border: 1px dashed var(--color-line-strong);
      color: var(--color-text-soft);
      font-size: 12px;

      .hint-icon {
        flex-shrink: 0;
        color: var(--color-primary);
      }
    }
  }

  .params-modal-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 24px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
    flex-shrink: 0;

    .footer-left,
    .footer-right {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .modal-footer-btn {
      height: 32px;
      font-size: 13px;

      &.primary-btn {
        background: var(--color-primary-solid, var(--color-primary));
        border-color: var(--color-primary-solid, var(--color-primary));
        color: #fff;

        &:hover:not(:disabled) {
          opacity: 0.9;
        }

        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
      }

      .copy-success-icon {
        color: var(--color-success, #10b981);
      }
    }
  }
}

@keyframes image-workbench-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

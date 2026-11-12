<template>
  <section class="local-model-extension">
    <div class="extension-card">
      <div class="extension-icon"><BrainCircuit :size="24" /></div>
      <div class="extension-copy">
        <h3 class="extension-title">本地大模型</h3>
        <p class="extension-description">
          按需下载模型，管理本地文件与磁盘空间，模型不随应用安装。
        </p>
      </div>
      <button class="model-button" type="button" @click="openManager">
        <HardDrive :size="15" />模型列表
      </button>
    </div>

    <BaseModal v-if="showManager" class="model-modal" @close="closeManager">
      <template #header>
        <div class="manager-heading">
          <h2 class="manager-title">本地大模型</h2>
          <div class="storage-summary" aria-live="polite">
            <HardDrive :size="15" />
            <span
              >文件占用
              {{ ready ? formatBytes(snapshot.usedBytes) : "—" }}</span
            >
            <span
              >已下载 {{ snapshot.installedCount }} /
              {{ snapshot.models.length }} 个模型</span
            >
          </div>
        </div>
        <div class="manager-header-actions">
          <button
            class="model-button"
            type="button"
            :disabled="refreshing"
            @click="load(true)"
          >
            <RefreshCw :size="14" />{{ refreshing ? "刷新中…" : "刷新" }}
          </button>
          <button
            class="manager-close"
            type="button"
            aria-label="关闭模型列表"
            @click="closeManager"
          >
            <X :size="16" />
          </button>
        </div>
      </template>
      <div class="model-manager">
        <div v-if="!ready" class="manager-empty" role="status">
          {{
            error ? "模型列表读取失败，请点击刷新重试。" : "正在读取本地模型…"
          }}
        </div>
        <div v-else class="model-list">
          <article
            v-for="model in snapshot.models"
            :key="model.id"
            class="model-card"
          >
            <div class="model-row">
              <button
                class="model-toggle"
                type="button"
                :aria-expanded="Boolean(expandedModels[model.id])"
                :aria-controls="`model-details-${model.id}`"
                :title="`${expandedModels[model.id] ? '收起' : '展开'} ${model.name} 详情`"
                @click="expandedModels[model.id] = !expandedModels[model.id]"
              >
                <ChevronRight
                  :size="16"
                  class="model-chevron"
                  :class="{ 'is-expanded': expandedModels[model.id] }"
                />
                <span class="model-name">{{ model.name }}</span>
              </button>
              <span class="model-size" title="完整下载大小">{{
                formatBytes(model.downloadBytes)
              }}</span>
              <label class="source-field">
                <span>下载源</span>
                <select
                  :value="sources[model.id] || model.sources[0]?.id"
                  @change="sources[model.id] = $event.target.value"
                  class="source-select"
                  :aria-label="`${model.name} 下载源`"
                  :disabled="isActive(model) || Boolean(pending)"
                >
                  <option
                    v-for="source in model.sources"
                    :key="source.id"
                    :value="source.id"
                  >
                    {{ source.name }}
                  </option>
                </select>
              </label>
              <div class="model-row-action">
                <button
                  v-if="isActive(model)"
                  class="model-button"
                  type="button"
                  :disabled="Boolean(pending) || model.phase === 'cancelling'"
                  @click="perform('cancel', model)"
                >
                  <Pause :size="14" />{{
                    model.phase === "cancelling" ? "暂停中…" : "暂停下载"
                  }}
                </button>
                <button
                  v-else-if="model.phase !== 'downloaded'"
                  class="model-button primary-button"
                  type="button"
                  :disabled="Boolean(pending) || !model.compatibility?.allowed"
                  :title="
                    model.compatibility?.allowed
                      ? ''
                      : '请展开查看下载条件，再刷新检测'
                  "
                  @click="perform('download', model)"
                >
                  <Download :size="14" />{{
                    pending === "download" && pendingId === model.id
                      ? "准备中…"
                      : model.usedBytes
                        ? "继续下载"
                        : "下载模型"
                  }}
                </button>
                <span v-else class="model-status status-complete">已下载</span>
              </div>
            </div>
            <div
              v-if="expandedModels[model.id]"
              :id="`model-details-${model.id}`"
              class="model-details"
            >
              <div class="model-heading">
                <span class="model-publisher">{{ model.publisher }}</span>
                <span
                  class="model-status"
                  :class="{
                    'status-complete': model.phase === 'downloaded',
                    'status-error': model.phase === 'error'
                  }"
                >
                  {{ phaseLabels[model.phase] || model.phase }}
                </span>
              </div>
              <p class="model-description">{{ model.description }}</p>
              <div class="model-meta">
                <span class="model-tag">{{ model.format }}</span>
                <span class="model-tag">{{ model.license }}</span>
                <span>完整下载 {{ formatBytes(model.downloadBytes) }}</span>
                <span>本地占用 {{ formatBytes(model.usedBytes) }}</span>
              </div>
              <div
                class="model-compatibility"
                :class="{
                  'compatibility-blocked': !model.compatibility?.allowed
                }"
                aria-live="polite"
              >
                <div class="compatibility-title">
                  {{
                    model.compatibility?.allowed
                      ? "下载条件已满足，内存仅供运行参考"
                      : model.compatibility
                        ? "暂不满足下载条件"
                        : "尚未完成本机检测，请刷新后重试"
                  }}
                </div>
                <div
                  v-for="check in model.compatibility?.checks"
                  :key="check.label"
                  class="compatibility-check"
                  :class="{
                    'check-failed': check.passed === false && !check.advisory,
                    'check-unknown': check.passed == null
                  }"
                >
                  <span class="check-label">{{ check.label }}</span>
                  <span class="check-detail">{{ check.detail }}</span>
                  <span class="check-result">{{
                    check.advisory
                      ? "仅供参考"
                      : check.passed == null
                        ? "缺少资源配置"
                        : check.passed
                          ? "通过"
                          : "未通过"
                  }}</span>
                </div>
                <div
                  v-if="model.phase === 'downloaded'"
                  class="model-validation"
                >
                  <div
                    class="validation-detail"
                    :class="{
                      'validation-failed':
                        model.compatibility?.validation?.passed === false
                    }"
                  >
                    {{
                      model.compatibility?.validation?.detail ||
                      "下载后可进行本机运行校验"
                    }}
                  </div>
                  <div
                    v-if="model.compatibility?.validation?.checkedAt"
                    class="validation-time"
                  >
                    上次校验
                    {{
                      formatDateTime(model.compatibility.validation.checkedAt)
                    }}
                  </div>
                </div>
              </div>
              <div class="model-files">
                <div
                  v-for="file in model.files"
                  :key="file.name"
                  class="model-file"
                >
                  <FileCheck
                    v-if="file.verified"
                    :size="15"
                    class="file-verified"
                  />
                  <FileDown v-else :size="15" />
                  <span class="file-name">{{ file.name }}</span>
                  <span class="file-state">
                    {{
                      file.verified
                        ? "已校验"
                        : file.localBytes === file.bytes
                          ? "待校验"
                          : file.cachedBytes
                            ? `已缓存 ${formatBytes(file.cachedBytes)}`
                            : "未下载"
                    }}
                  </span>
                  <span class="file-size">{{ formatBytes(file.bytes) }}</span>
                </div>
              </div>
              <div
                v-if="
                  isActive(model) ||
                  model.phase === 'paused' ||
                  model.phase === 'error'
                "
                class="download-progress"
              >
                <div class="progress-copy" aria-live="polite">
                  <span>{{
                    model.phase === "verifying"
                      ? `正在校验 ${model.currentFile}`
                      : `${formatBytes(model.downloadedBytes)} / ${formatBytes(model.downloadBytes)}`
                  }}</span>
                  <span>{{ model.percent.toFixed(1) }}%</span>
                </div>
                <progress
                  class="progress-bar"
                  :value="model.percent"
                  max="100"
                  aria-label="模型下载进度"
                ></progress>
              </div>
              <div class="model-actions">
                <button
                  v-if="model.phase === 'downloaded'"
                  class="model-button"
                  type="button"
                  :disabled="Boolean(pending)"
                  @click="perform('validate', model)"
                >
                  <FileCheck :size="14" />{{
                    pending === "validate" && pendingId === model.id
                      ? "校验中…"
                      : "运行校验"
                  }}
                </button>
                <button
                  v-if="model.usedBytes > 0"
                  class="model-button danger-button"
                  type="button"
                  :disabled="isActive(model) || Boolean(pending)"
                  @click="deleteId = model.id"
                >
                  <Trash2 :size="14" />删除
                </button>
              </div>
              <div
                v-if="deleteId === model.id"
                class="delete-confirm"
                role="alert"
              >
                <span
                  >删除 {{ model.name }} 的全部模型与缓存文件，释放
                  {{ formatBytes(model.usedBytes) }}？</span
                >
                <div class="confirm-actions">
                  <button
                    class="model-button"
                    type="button"
                    :disabled="Boolean(pending)"
                    @click="deleteId = ''"
                  >
                    取消
                  </button>
                  <button
                    class="model-button danger-button"
                    type="button"
                    :disabled="Boolean(pending) || isActive(model)"
                    @click="perform('delete', model)"
                  >
                    {{
                      pending === "delete" && pendingId === model.id
                        ? "删除中…"
                        : "确认删除"
                    }}
                  </button>
                </div>
              </div>
            </div>
          </article>
        </div>
      </div>
    </BaseModal>
  </section>
</template>

<script setup>
import { onBeforeUnmount, ref } from "vue"
import {
  BrainCircuit,
  ChevronRight,
  Download,
  FileCheck,
  FileDown,
  HardDrive,
  Pause,
  RefreshCw,
  Trash2,
  X
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { localModelsApi } from "@/api/modules/local-models"
import { subscribeReady } from "@/api/request"
import { createMessage } from "@/utils/message"
import { formatDateTime } from "@/utils/formatters"

const showManager = ref(false)
const ready = ref(false)
const refreshing = ref(false)
const pending = ref("")
const error = ref("")
const deleteId = ref("")
// 展开状态按模型保存，后台进度刷新不打断正在查看的详情。
const expandedModels = ref({})
// 每个模型维护自己的下载源，避免将 OCR 官方仓库用于翻译权重。
const sources = ref({})
const pendingId = ref("")
const snapshot = ref({
  models: [],
  usedBytes: 0,
  installedCount: 0,
  storagePath: ""
})
const phaseLabels = {
  "not-downloaded": "未下载",
  downloading: "下载中",
  verifying: "校验中",
  cancelling: "暂停中",
  paused: "未完成",
  downloaded: "已下载",
  error: "下载失败"
}
let unlisten
let disposed = false
let sequence = 0
let lastDownloadError = ""

function formatBytes(value) {
  const bytes = Number(value || 0)
  if (bytes < 1024) return `${bytes} B`
  const units = ["KiB", "MiB", "GiB", "TiB"]
  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)) - 1,
    units.length - 1
  )
  return `${(bytes / 1024 ** (index + 1)).toFixed(2)} ${units[index]}`
}

function isActive(model) {
  return ["downloading", "verifying", "cancelling"].includes(model.phase)
}

async function load(manual = false) {
  const current = ++sequence
  if (manual) refreshing.value = true
  try {
    const result = await localModelsApi.list()
    // 事件刷新和手动刷新可能交错，只接受最新一次读取。
    if (disposed || current !== sequence || !showManager.value) return
    snapshot.value = result
    ready.value = true
    const downloadError = result.models
      .filter((model) => model.phase === "error")
      .map((model) => model.error)
      .filter(Boolean)
      .join("\n")
    // 下载在后台执行，失败通过统一消息提示展示，同一次失败不随进度刷新重复弹出。
    if (downloadError && downloadError !== lastDownloadError) {
      createMessage.error(downloadError)
    }
    lastDownloadError = downloadError
    if (manual) error.value = ""
  } catch (cause) {
    if (!disposed && current === sequence) {
      const message = String(cause)
      if (manual || error.value !== message) createMessage.error(message)
      error.value = message
    }
  } finally {
    if (manual) refreshing.value = false
  }
}

async function openManager() {
  if (showManager.value) return
  showManager.value = true
  error.value = ""
  try {
    // 先注册监听，再开始读取和下载，避免遗漏第一个进度通知。
    if (!unlisten) {
      const stop = await subscribeReady("local-models:changed", () => {
        if (showManager.value) load()
      })
      if (disposed) return stop()
      unlisten = stop
    }
    await load(true)
  } catch (cause) {
    if (!disposed) {
      error.value = String(cause)
      createMessage.error(error.value)
    }
  }
}

function closeManager() {
  showManager.value = false
  deleteId.value = ""
  expandedModels.value = {}
  sequence++
}

async function perform(action, model) {
  if (pending.value) return
  // 检测未通过时保留暂停和删除操作，下载入口仍由后端重新校验。
  if (action === "download" && !model.compatibility?.allowed) return
  pending.value = action
  pendingId.value = model.id
  error.value = ""
  try {
    if (action === "download") {
      lastDownloadError = ""
      await localModelsApi.download({
        id: model.id,
        source: sources.value[model.id] || model.sources[0]?.id
      })
    } else {
      const result = await localModelsApi[action](model.id)
      if (action === "validate" && result.passed === false) {
        createMessage.error(result.detail)
      }
    }
    deleteId.value = ""
  } catch (cause) {
    if (!disposed) {
      error.value = String(cause)
      createMessage.error(error.value)
    }
  } finally {
    pending.value = ""
    pendingId.value = ""
    if (!disposed && showManager.value) await load()
  }
}

onBeforeUnmount(() => {
  disposed = true
  sequence++
  unlisten?.()
})
</script>

<style scoped lang="less">
.local-model-extension {
  .model-button {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 7px 11px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
    font-size: 13px;
    cursor: pointer;

    &:hover {
      background: var(--color-panel-soft);
    }
    &:disabled {
      opacity: 0.55;
      cursor: not-allowed;
    }
    &.primary-button {
      border-color: #2563eb;
      background: #2563eb;
      color: #fff;
    }
    &.danger-button {
      color: var(--color-danger);
    }
  }

  .extension-card {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 22px 28px;
    border: 1px solid var(--color-line);
    border-radius: 16px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);

    .extension-icon {
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

    .extension-copy {
      min-width: 0;
      flex: 1;

      .extension-title {
        margin: 0 0 4px;
        font-size: var(--font-size-base);
      }
      .extension-description {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 13px;
        line-height: 1.5;
      }
    }
  }

  .model-modal {
    :deep(.base-modal__panel) {
      width: 900px;
      max-width: calc(100vw - 48px);
    }
    :deep(.base-modal__header) {
      flex-shrink: 0;
      align-items: center;
      gap: 12px;
      padding: 10px 24px;
      border-bottom: 1px solid var(--color-line);
    }

    .manager-heading {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      gap: 16px;

      .manager-title {
        flex: none;
        margin: 0;
        font-size: var(--font-size-xl);
      }

      .storage-summary {
        display: flex;
        align-items: center;
        gap: 8px;
        color: var(--color-text-muted);
        font-size: 12px;
        white-space: nowrap;
      }
    }

    .manager-header-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 8px;

      .manager-close {
        display: flex;
        width: 32px;
        height: 32px;
        align-items: center;
        justify-content: center;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;

        &:hover {
          background: var(--color-panel-soft);
          color: var(--color-text);
        }
      }
    }

    .model-manager {
      display: flex;
      min-height: 0;
      flex-direction: column;
      padding-top: 12px;
      font-size: 13px;

      .manager-empty {
        padding: 32px 0;
        text-align: center;
        color: var(--color-text-muted);
      }

      .model-list {
        min-height: 0;
        overflow-y: auto;
        border: 1px solid var(--color-line);
        border-radius: 8px;

        .model-card {
          background: var(--color-panel);

          & + .model-card {
            border-top: 1px solid var(--color-line);
          }

          .model-row {
            display: flex;
            min-height: 60px;
            align-items: center;
            gap: 12px;
            padding: 12px 14px;

            .model-toggle {
              display: flex;
              min-width: 0;
              flex: 1;
              align-items: center;
              gap: 8px;
              padding: 4px 0;
              border: 0;
              background: transparent;
              color: var(--color-text);
              font: inherit;
              text-align: left;
              cursor: pointer;

              &:hover {
                color: #2563eb;
              }

              .model-chevron {
                flex: none;

                &.is-expanded {
                  transform: rotate(90deg);
                }
              }
              .model-name {
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
              }
            }

            .model-size {
              width: 80px;
              flex: none;
              color: var(--color-text-muted);
              font-variant-numeric: tabular-nums;
              text-align: right;
              white-space: nowrap;
            }

            .source-field {
              display: flex;
              flex: none;
              align-items: center;
              gap: 8px;
              color: var(--color-text-muted);

              .source-select {
                width: 180px;
                padding: 7px 9px;
                border: 1px solid var(--color-line);
                border-radius: 7px;
                background: var(--color-panel);
                color: var(--color-text);
                font: inherit;

                &:disabled {
                  opacity: 0.55;
                  cursor: not-allowed;
                }
              }
            }

            .model-row-action {
              display: flex;
              width: 100px;
              flex: none;
              justify-content: flex-end;
            }
          }

          .model-status {
            flex: none;
            padding: 4px 9px;
            border-radius: 6px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);

            &.status-complete {
              background: #ecfdf5;
              color: #047857;
            }
            &.status-error {
              color: var(--color-danger);
            }
          }

          .model-details {
            padding: 14px;
            border-top: 1px solid var(--color-line);

            .model-heading {
              display: flex;
              align-items: center;
              justify-content: space-between;
              gap: 12px;

              .model-publisher {
                color: var(--color-text-muted);
                font-size: 12px;
              }
            }

            .model-description {
              margin: 12px 0;
              color: var(--color-text-muted);
              line-height: 1.6;
            }

            .model-meta {
              display: flex;
              flex-wrap: wrap;
              align-items: center;
              gap: 10px 16px;
              color: var(--color-text-muted);
              font-size: 12px;

              .model-tag {
                padding: 3px 7px;
                border: 1px solid var(--color-line);
                border-radius: 5px;
              }
            }

            .model-compatibility {
              display: flex;
              flex-direction: column;
              gap: 10px;
              margin-top: 16px;
              padding: 14px;
              border: 1px solid var(--color-line);
              border-radius: 8px;
              background: var(--color-panel-soft);

              .compatibility-title {
                color: #047857;
              }
              .model-validation {
                .validation-detail {
                  color: var(--color-text-muted);

                  &.validation-failed {
                    color: var(--color-danger);
                  }
                }
                .validation-time {
                  color: var(--color-text-muted);
                  font-size: 12px;
                }
              }

              &.compatibility-blocked {
                .compatibility-title {
                  color: var(--color-danger);
                }
              }

              .compatibility-check {
                display: flex;
                align-items: baseline;
                gap: 12px;
                color: var(--color-text-muted);
                font-size: 12px;
                line-height: 1.6;

                .check-label {
                  width: 72px;
                  flex: none;
                }
                .check-detail {
                  min-width: 0;
                  flex: 1;
                  overflow-wrap: anywhere;
                }
                .check-result {
                  flex: none;
                  color: #047857;
                }

                &.check-failed {
                  .check-detail,
                  .check-result {
                    color: var(--color-danger);
                  }
                }
                &.check-unknown {
                  .check-result {
                    color: var(--color-text-muted);
                  }
                }
              }
            }

            .model-files {
              display: flex;
              flex-direction: column;
              gap: 12px;
              margin: 18px 0;
              padding: 14px;
              border-radius: 8px;
              background: var(--color-panel-soft);

              .model-file {
                display: flex;
                align-items: center;
                gap: 8px;
                color: var(--color-text-muted);
                font-size: 12px;

                .file-verified {
                  color: #047857;
                }
                .file-name {
                  min-width: 0;
                  flex: 1;
                  overflow-wrap: anywhere;
                }
                .file-state {
                  flex: none;
                }
                .file-size {
                  width: 85px;
                  flex: none;
                  text-align: right;
                }
              }
            }

            .download-progress {
              margin: 16px 0;

              .progress-copy {
                display: flex;
                justify-content: space-between;
                gap: 12px;
                margin-bottom: 8px;
                color: var(--color-text-muted);
                overflow-wrap: anywhere;
              }

              .progress-bar {
                display: block;
                width: 100%;
                height: 7px;
                accent-color: #2563eb;
              }
            }

            .model-actions {
              display: flex;
              flex-wrap: wrap;
              align-items: center;
              gap: 10px;
            }

            .delete-confirm {
              display: flex;
              flex-direction: column;
              gap: 12px;
              margin-top: 16px;
              padding-top: 16px;
              border-top: 1px solid var(--color-line);
              color: var(--color-danger);
              line-height: 1.6;

              .confirm-actions {
                display: flex;
                justify-content: flex-end;
                gap: 8px;
              }
            }
          }
        }
      }
    }
  }
}
</style>

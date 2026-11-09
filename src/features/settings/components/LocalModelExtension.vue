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

    <BaseModal
      v-if="showManager"
      class="model-modal"
      title="本地大模型"
      description="选择需要的模型下载到本机，也可以清理模型文件释放空间。"
      @close="closeManager"
    >
      <div class="model-manager">
        <div class="manager-toolbar">
          <div class="storage-summary" aria-live="polite">
            <HardDrive :size="18" />
            <span
              >文件占用
              {{ ready ? formatBytes(snapshot.usedBytes) : "—" }}</span
            >
            <span class="summary-muted"
              >已下载 {{ snapshot.installedCount }} /
              {{ snapshot.models.length }} 个模型</span
            >
          </div>
          <button
            class="model-button"
            type="button"
            :disabled="refreshing"
            @click="load(true)"
          >
            <RefreshCw :size="14" />{{ refreshing ? "刷新中…" : "刷新" }}
          </button>
        </div>
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
            <div class="model-heading">
              <div class="model-heading-copy">
                <h3 class="model-title">{{ model.name }}</h3>
                <span class="model-publisher">{{ model.publisher }}</span>
              </div>
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
              <label class="source-field">
                <span>下载源</span>
                <select
                  v-model="source"
                  class="source-select"
                  :disabled="isActive(model) || Boolean(pending)"
                >
                  <option value="modelscope">ModelScope（官方）</option>
                  <option value="huggingface">Hugging Face（官方）</option>
                </select>
              </label>
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
                :disabled="Boolean(pending)"
                @click="perform('download', model)"
              >
                <Download :size="14" />{{
                  pending === "download"
                    ? "准备中…"
                    : model.usedBytes
                      ? "继续下载"
                      : "下载模型"
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
                  {{ pending === "delete" ? "删除中…" : "确认删除" }}
                </button>
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
  Download,
  FileCheck,
  FileDown,
  HardDrive,
  Pause,
  RefreshCw,
  Trash2
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { localModelsApi } from "@/api/modules/local-models"
import { subscribeReady } from "@/api/request"
import { createMessage } from "@/utils/message"

const showManager = ref(false)
const ready = ref(false)
const refreshing = ref(false)
const pending = ref("")
const error = ref("")
const deleteId = ref("")
const source = ref("modelscope")
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
  sequence++
}

async function perform(action, model) {
  if (pending.value) return
  pending.value = action
  error.value = ""
  try {
    if (action === "download") {
      lastDownloadError = ""
      await localModelsApi.download({ id: model.id, source: source.value })
    } else {
      await localModelsApi[action](model.id)
    }
    deleteId.value = ""
  } catch (cause) {
    if (!disposed) {
      error.value = String(cause)
      createMessage.error(error.value)
    }
  } finally {
    pending.value = ""
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
    }

    .model-manager {
      display: flex;
      min-height: 0;
      flex-direction: column;
      gap: 16px;
      font-size: 13px;

      .manager-toolbar {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 14px 0;
        border-bottom: 1px solid var(--color-line);

        .storage-summary {
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          gap: 10px;

          .summary-muted {
            color: var(--color-text-muted);
          }
        }
      }

      .manager-empty {
        padding: 32px 0;
        text-align: center;
        color: var(--color-text-muted);
      }

      .model-list {
        min-height: 0;
        overflow-y: auto;

        .model-card {
          padding: 20px;
          border: 1px solid var(--color-line);
          border-radius: 12px;
          background: var(--color-panel);

          .model-heading {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 12px;

            .model-heading-copy {
              display: flex;
              flex-wrap: wrap;
              align-items: baseline;
              gap: 10px;

              .model-title {
                margin: 0;
                font-size: 17px;
              }
              .model-publisher {
                color: var(--color-text-muted);
                font-size: 12px;
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

            .source-field {
              display: flex;
              flex: 1;
              align-items: center;
              gap: 8px;
              color: var(--color-text-muted);

              .source-select {
                padding: 7px 9px;
                border: 1px solid var(--color-line);
                border-radius: 7px;
                background: var(--color-panel);
                color: var(--color-text);
                font: inherit;
              }
            }
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
</style>

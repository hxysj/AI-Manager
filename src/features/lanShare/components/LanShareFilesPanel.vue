<template>
  <section class="lan-share-files-panel">
    <header class="lan-share-files-head">
      <div class="lan-share-files-title">
        <span data-emphasis class="lan-share-files-name">共享文件</span>
        <span class="lan-share-files-subtitle">{{ files.length }} 个文件</span>
      </div>
      <div class="lan-share-files-actions">
        <button
          class="lan-share-files-mini-button"
          type="button"
          :disabled="!canManageFiles || loading"
          @click="addFiles"
        >
          <Plus :size="13" />
          添加文件
        </button>
        <button
          class="lan-share-files-mini-button"
          type="button"
          :disabled="!canManageFiles || loading"
          @click="refreshFiles"
        >
          <RefreshCw :size="13" />
          刷新
        </button>
        <button
          class="lan-share-files-mini-button"
          type="button"
          :disabled="!files.length"
          @click="toggleSelectAllFiles"
        >
          {{ allFilesSelected ? "取消全选" : "全选" }}
        </button>
        <button
          class="lan-share-files-mini-button"
          type="button"
          :disabled="!selectedFileIds.length || loading"
          @click="removeSelectedFiles"
        >
          <Trash2 :size="13" />
          删除所选
        </button>
        <button
          class="lan-share-files-mini-button"
          type="button"
          :disabled="!selectedFileIds.length || loading"
          @click="exportSelectedFiles"
        >
          <Archive :size="13" />
          打包保存
        </button>
      </div>
    </header>
    <div class="lan-share-files-list">
      <article
        v-for="file in files"
        :key="file.id"
        :class="[
          'lan-share-files-item',
          { 'lan-share-files-item-disabled': !file.enabled }
        ]"
      >
        <label class="lan-share-files-check">
          <input
            v-model="selectedFileIds"
            class="lan-share-files-check-input"
            type="checkbox"
            :value="file.id"
          />
          <span class="lan-share-files-check-mark"></span>
        </label>
        <span class="lan-share-files-icon">
          <FileText :size="16" />
        </span>
        <div class="lan-share-files-main">
          <span data-emphasis class="lan-share-files-file-name" :title="file.name">
            {{ file.name }}
          </span>
          <span class="lan-share-files-meta" :title="file.path">
            {{ formatSize(file.size) }} · {{ file.mimeType || "文件" }} ·
            {{ formatDateTime(file.updatedAt) }}
          </span>
        </div>
        <div class="lan-share-files-row-actions">
          <button
            class="lan-share-files-icon-button"
            type="button"
            title="预览文件"
            :disabled="!serviceRunning || !file.enabled"
            @click="emit('preview-file', file)"
          >
            <Eye :size="14" />
          </button>
          <button
            class="lan-share-files-icon-button"
            type="button"
            title="移除共享"
            :disabled="loading"
            @click="removeFile(file)"
          >
            <Trash2 :size="14" />
          </button>
        </div>
      </article>
      <div v-if="!files.length" class="lan-share-files-empty">
        {{ emptyText }}
      </div>
    </div>
  </section>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import { Archive, Eye, FileText, Plus, RefreshCw, Trash2 } from "lucide-vue-next"
import { lanShareApi, systemApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  currentSessionId: {
    type: String,
    default: ""
  },
  canManageFiles: {
    type: Boolean,
    default: false
  },
  serviceRunning: {
    type: Boolean,
    default: false
  },
  stateVersion: {
    type: Number,
    default: 0
  }
})

const emit = defineEmits(["refresh-state", "preview-file"])

const files = ref([])
const selectedFileIds = ref([])
const loading = ref(false)

const emptyText = computed(() => {
  if (!props.canManageFiles) {
    return "请先选择一个会话，再添加要共享给该会话的文件。"
  }

  return "当前会话还没有共享文件。"
})

const allFilesSelected = computed(() => {
  return (
    Boolean(files.value.length) &&
    files.value.every((file) => selectedFileIds.value.includes(file.id))
  )
})

watch(
  () => [props.currentSessionId, props.stateVersion],
  () => loadFiles(),
  { immediate: true }
)

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

async function loadFiles() {
  if (!props.currentSessionId) {
    files.value = []
    selectedFileIds.value = []
    return
  }

  try {
    const result = unwrapData(await lanShareApi.getState())
    const stateFiles = Array.isArray(result?.files) ? result.files : []

    files.value = stateFiles.filter((file) => {
      return file.sessionId === props.currentSessionId
    })
    selectedFileIds.value = selectedFileIds.value.filter((fileId) => {
      return files.value.some((file) => file.id === fileId)
    })
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

function unwrapData(result) {
  return result?.status && "data" in result ? result.data : result
}

async function runFileAction(action, successMessage) {
  loading.value = true

  try {
    const result = await action()

    if (successMessage) {
      createMessage.success(successMessage)
    }

    emit("refresh-state")
    await loadFiles()
    return result
  } catch (error) {
    createMessage.error(error?.message || String(error))
    return null
  } finally {
    loading.value = false
  }
}

async function addFiles() {
  if (!props.currentSessionId) {
    createMessage.warning("请先选择会话后再添加文件。")
    return
  }

  const selectedPaths = await systemApi.selectFiles({
    title: "选择要共享的文件"
  })

  if (!selectedPaths?.length) {
    return
  }

  await runFileAction(
    async () =>
      lanShareApi.addFiles({
        sessionId: props.currentSessionId,
        paths: selectedPaths
      }),
    "共享文件已更新。"
  )
}

async function refreshFiles() {
  await runFileAction(
    async () => lanShareApi.refreshFiles(),
    "文件状态已刷新。"
  )
}

async function removeFile(file) {
  await runFileAction(
    async () => lanShareApi.removeFile({ fileId: file.id }),
    "共享文件已移除。"
  )
}

async function removeSelectedFiles() {
  if (!selectedFileIds.value.length) {
    return
  }

  const fileIds = [...selectedFileIds.value]
  const result = await runFileAction(
    async () => lanShareApi.removeFiles({ fileIds }),
    "所选共享文件已移除。"
  )

  if (result) {
    selectedFileIds.value = []
  }
}

async function exportSelectedFiles() {
  if (!selectedFileIds.value.length) {
    return
  }

  const targetPath = await systemApi.saveFile({
    title: "保存共享文件压缩包",
    defaultPath: "lan-share-files.zip",
    filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }]
  })

  if (!targetPath) {
    return
  }

  await runFileAction(
    async () =>
      lanShareApi.exportFilesZip({
        fileIds: [...selectedFileIds.value],
        targetPath
      }),
    "所选文件已打包保存。"
  )
}

function toggleSelectAllFiles() {
  if (allFilesSelected.value) {
    selectedFileIds.value = []
    return
  }

  selectedFileIds.value = files.value.map((file) => file.id)
}
</script>

<style scoped lang="less">
.lan-share-files-panel {
  display: flex;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);

  .lan-share-files-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 48px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);

    .lan-share-files-title {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 2px;

      .lan-share-files-name {
        color: var(--color-text);
        font-size: 0.9rem;
      }

      .lan-share-files-subtitle {
        color: var(--color-text-muted);
        font-size: 0.76rem;
      }
    }

    .lan-share-files-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 8px;

      .lan-share-files-mini-button {
        display: inline-flex;
        height: 30px;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 0 9px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-primary);
        cursor: pointer;
        font-size: 0.76rem;
      }

      .lan-share-files-mini-button:disabled {
        cursor: not-allowed;
        opacity: 0.45;
      }
    }
  }

  .lan-share-files-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding: 10px;

    .lan-share-files-item {
      display: flex;
      align-items: center;
      gap: 9px;
      min-height: 58px;
      padding: 9px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);

      .lan-share-files-check {
        position: relative;
        display: inline-flex;
        width: 18px;
        height: 18px;
        flex: none;
        align-items: center;
        justify-content: center;
        cursor: pointer;

        .lan-share-files-check-input {
          position: absolute;
          inset: 0;
          margin: 0;
          cursor: pointer;
          opacity: 0;
        }

        .lan-share-files-check-mark {
          display: inline-flex;
          width: 16px;
          height: 16px;
          border: 1px solid var(--color-line);
          border-radius: 4px;
          background: var(--color-panel);
        }

        .lan-share-files-check-input:checked + .lan-share-files-check-mark {
          border-color: var(--color-primary);
          background: var(--color-primary-solid);
        }
      }

      .lan-share-files-icon {
        display: inline-flex;
        width: 32px;
        height: 32px;
        flex: 0 0 32px;
        align-items: center;
        justify-content: center;
        border-radius: 7px;
        background: var(--color-primary-soft);
        color: var(--color-primary);
      }

      .lan-share-files-main {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 3px;

        .lan-share-files-file-name,
        .lan-share-files-meta {
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .lan-share-files-file-name {
          color: var(--color-text);
          font-size: 0.84rem;
        }

        .lan-share-files-meta {
          color: var(--color-text-muted);
          font-size: 0.72rem;
        }
      }

      .lan-share-files-row-actions {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;

        .lan-share-files-icon-button {
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

        .lan-share-files-icon-button:disabled {
          cursor: not-allowed;
          opacity: 0.45;
        }
      }
    }

    .lan-share-files-item-disabled {
      opacity: 0.58;
    }

    .lan-share-files-empty {
      display: flex;
      min-height: 120px;
      align-items: center;
      justify-content: center;
      border: 1px dashed var(--color-line);
      border-radius: 8px;
      color: var(--color-text-muted);
      font-size: 0.82rem;
    }
  }
}
</style>

<template>
  <BaseModal
    v-if="visible"
    class="image-batch-modal"
    title="批量生成"
    :description="`${selectedRows.length}/${rows.length} 条任务已选中 · 按队列执行 · 批量任务会创建到新会话`"
    @close="handleClose"
  >
    <div class="image-batch-dialog">
      <div class="batch-dialog-scroll">
        <section class="batch-toolbar">
          <div class="batch-file-picker">
            <Upload :size="16" />
            <span>{{ fileName || "导入 XLSX" }}</span>
            <input
              type="file"
              accept=".xlsx,.xls"
              :disabled="running"
              @change="importWorkbook"
            />
          </div>
          <button
            class="batch-action-button"
            type="button"
            :disabled="running"
            @click="addRow"
          >
            <Plus :size="15" />
            <span>添加任务</span>
          </button>
          <button
            class="batch-action-button batch-clear-button"
            type="button"
            :disabled="running || !rows.length"
            @click="clearRows"
          >
            <Trash2 :size="15" />
            <span>清空</span>
          </button>
        </section>

        <div class="batch-hint">
          <FileSpreadsheet :size="15" />
          <span
            >表头支持“名称 / name”和“提示词 /
            prompt”，导入后可在右侧选择对应列。</span
          >
        </div>

        <div class="batch-workspace">
          <section class="batch-queue-area">
            <header class="batch-queue-area-head">
              <span>任务队列</span>
              <span>{{ selectedRows.length }} / {{ rows.length }} 条已选</span>
            </header>
            <section v-if="rows.length" class="batch-table-wrap">
              <div class="batch-table-head">
                <span class="batch-column-select">
                  <input
                    v-model="allRowsSelected"
                    type="checkbox"
                    aria-label="全选任务"
                    :indeterminate="
                      selectedRows.length > 0 && !allRowsSelected
                    "
                    :disabled="running"
                  />
                </span>
                <span class="batch-column-index">序号</span>
                <span class="batch-column-name">名称</span>
                <span class="batch-column-prompt">提示词</span>
                <span class="batch-column-status">状态</span>
                <span class="batch-column-action">操作</span>
              </div>
              <div class="batch-table-body">
                <div
                  v-for="(row, index) in rows"
                  :key="row.id"
                  class="batch-row"
                >
                  <span class="batch-column-select">
                    <input
                      v-model="row.included"
                      type="checkbox"
                      aria-label="选择任务"
                      :disabled="running"
                    />
                  </span>
                  <span class="batch-column-index">{{ index + 1 }}</span>
                  <input
                    v-model.trim="row.name"
                    class="batch-field batch-column-name"
                    placeholder="图片名称"
                    :disabled="running"
                  />
                  <textarea
                    v-model.trim="row.prompt"
                    class="batch-field batch-column-prompt"
                    rows="2"
                    placeholder="输入提示词"
                    :disabled="running"
                  ></textarea>
                  <span
                    class="batch-column-status"
                    :class="{ 'is-skipped': row.status === 'skipped' }"
                  >
                    <span class="batch-status" :class="row.status">{{
                      statusLabel(row)
                    }}</span>
                    <small
                      v-if="row.error"
                      class="batch-row-error"
                      :title="row.error"
                      >{{ row.error }}</small
                    >
                  </span>
                  <button
                    class="batch-remove-button"
                    type="button"
                    title="移除任务"
                    :disabled="running"
                    @click="removeRow(row.id)"
                  >
                    <X :size="14" />
                  </button>
                </div>
              </div>
            </section>
            <div v-else class="batch-empty">
              <FileSpreadsheet :size="28" />
              <p>导入 Excel，或点击“添加任务”开始配置。</p>
            </div>
          </section>

          <aside class="batch-settings-area">
            <section class="batch-settings-panel">
              <header class="batch-settings-head">
                <span>生成参数</span>
                <span>批量任务独立设置</span>
              </header>
              <label class="batch-settings-field">
                <span>生成模型</span>
                <select v-model="selectedModel" :disabled="running">
                  <option
                    v-for="model in models"
                    :key="model.id"
                    :value="model.id"
                  >
                    {{ model.id }}
                  </option>
                </select>
              </label>
              <label class="batch-settings-field">
                <span>图片尺寸</span>
                <select v-model="selectedSize" :disabled="running">
                  <option
                    v-for="preset in sizePresets"
                    :key="preset.label"
                    :value="`${preset.width}x${preset.height}`"
                  >
                    {{ preset.label }} · {{ preset.width }}×{{ preset.height }}
                  </option>
                </select>
              </label>
            </section>

            <section v-if="columnOptions.length" class="batch-settings-panel">
              <header class="batch-settings-head">
                <span>导入列映射</span>
                <span>生成前可调整</span>
              </header>
              <label class="batch-settings-field">
                <span>名称列</span>
                <select
                  v-model="nameColumn"
                  :disabled="running"
                  @change="applyImportedRows"
                >
                  <option
                    v-for="column in columnOptions"
                    :key="`name-${column.index}`"
                    :value="String(column.index)"
                  >
                    {{ column.label }}
                  </option>
                </select>
              </label>
              <label class="batch-settings-field">
                <span>提示词列</span>
                <select
                  v-model="promptColumn"
                  :disabled="running"
                  @change="applyImportedRows"
                >
                  <option
                    v-for="column in columnOptions"
                    :key="`prompt-${column.index}`"
                    :value="String(column.index)"
                  >
                    {{ column.label }}
                  </option>
                </select>
              </label>
              <p v-if="columnSelectionError" class="batch-column-error">
                {{ columnSelectionError }}
              </p>
            </section>

            <section v-if="batchQueue" class="batch-progress-panel">
              <header class="batch-progress-head">
                <div>
                  <span class="batch-progress-title">执行进度</span>
                  <span class="batch-progress-meta">全部任务</span>
                </div>
                <span class="batch-progress-count"
                  >{{ completedCount }} / {{ rows.length }}</span
                >
              </header>
              <div class="batch-progress-track">
                <span :style="{ width: `${progressPercent}%` }"></span>
              </div>
              <div class="batch-group-list">
                <span class="batch-group-pill" :class="batchQueue.status">
                  全部任务 · {{ batchQueue.rows.length }} 条
                </span>
              </div>
            </section>
          </aside>
        </div>
      </div>

      <footer class="batch-footer">
        <p v-if="errorMessage" class="batch-error">{{ errorMessage }}</p>
        <div class="batch-footer-actions">
          <button
            class="batch-secondary-button"
            type="button"
            :disabled="running"
            @click="handleClose"
          >
            关闭
          </button>
          <button
            class="batch-primary-button"
            type="button"
            :disabled="running || !canStart"
            @click="startBatch"
          >
            <LoaderCircle v-if="running" :size="15" class="batch-spinner" />
            <Play v-else :size="15" />
            <span>{{ running ? "生成中…" : "开始批量生成" }}</span>
          </button>
        </div>
      </footer>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import {
  FileSpreadsheet,
  LoaderCircle,
  Play,
  Plus,
  Trash2,
  Upload,
  X
} from "lucide-vue-next"
import * as XLSX from "xlsx"
import BaseModal from "@/components/BaseModal.vue"
import { systemApi, toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

const props = defineProps({
  visible: { type: Boolean, default: false },
  conversationId: { type: String, default: "" },
  settings: { type: Object, default: () => ({}) },
  models: { type: Array, default: () => [] },
  sizePresets: { type: Array, default: () => [] }
})
const emit = defineEmits(["close", "finished", "session-created"])

const rows = ref([])
const fileName = ref("")
const running = ref(false)
const errorMessage = ref("")
const batchQueue = ref(null)
const runningConversationId = ref("")
const selectedModel = ref("")
const selectedSize = ref("")
const columnOptions = ref([])
const nameColumn = ref("")
const promptColumn = ref("")
const importedRows = ref([])

const selectedRows = computed(() => rows.value.filter((row) => row.included))
const allRowsSelected = computed({
  get: () => rows.value.length > 0 && rows.value.every((row) => row.included),
  set: (value) => rows.value.forEach((row) => (row.included = value))
})
const columnSelectionError = computed(() => {
  if (!columnOptions.value.length) return ""
  if (nameColumn.value === promptColumn.value)
    return "名称列和提示词列需要选择不同字段"
  if (nameColumn.value === "" || promptColumn.value === "")
    return "请选择名称列和提示词列"
  return ""
})
const canStart = computed(
  () =>
    selectedRows.value.length > 0 &&
    selectedRows.value.every((row) => row.prompt.trim()) &&
    !columnSelectionError.value &&
    Boolean(
      props.settings.accountId && selectedModel.value && selectedSize.value
    )
)
const completedCount = computed(
  () =>
    rows.value.filter((row) =>
      ["completed", "failed", "partial", "interrupted", "skipped"].includes(
        row.status
      )
    ).length
)
const progressPercent = computed(() =>
  rows.value.length
    ? Math.round((completedCount.value / rows.value.length) * 100)
    : 0
)

function createRow(name = "", prompt = "") {
  return {
    id: crypto.randomUUID(),
    name,
    prompt,
    included: true,
    status: "pending",
    taskId: "",
    error: ""
  }
}

function addRow() {
  rows.value.push(createRow())
}

function removeRow(id) {
  rows.value = rows.value.filter((row) => row.id !== id)
}

function clearRows() {
  rows.value = []
  batchQueue.value = null
  fileName.value = ""
  errorMessage.value = ""
  columnOptions.value = []
  nameColumn.value = ""
  promptColumn.value = ""
  importedRows.value = []
}

function normalizeHeader(value) {
  return String(value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[\s_-]/g, "")
}

function statusLabel(row) {
  return (
    {
      pending: "待生成",
      queued: "排队中",
      processing: "生成中",
      completed: "已完成",
      partial: "部分完成",
      failed: "失败",
      interrupted: "已中断",
      skipped: "已跳过"
    }[row.status] || "待生成"
  )
}

async function importWorkbook(event) {
  const file = event.target.files?.[0]
  event.target.value = ""
  if (!file) return
  errorMessage.value = ""
  try {
    const workbook = XLSX.read(await file.arrayBuffer(), { type: "array" })
    const sheet = workbook.Sheets[workbook.SheetNames[0]]
    const matrix = XLSX.utils.sheet_to_json(sheet, { header: 1, defval: "" })
    if (!matrix.length) throw new Error("Excel 文件没有可读取的数据")
    const headerRow = matrix[0].map((value) => String(value ?? "").trim())
    const headers = headerRow.map(normalizeHeader)
    const availableColumns = headerRow
      .map((label, index) => ({ label, index }))
      .filter((column) => column.label)
    if (availableColumns.length < 2)
      throw new Error("Excel 至少需要包含名称列和提示词列")
    const detectedNameIndex = headers.findIndex((value) =>
      ["名称", "name", "标题", "title"].includes(value)
    )
    const detectedPromptIndex = headers.findIndex((value) =>
      ["提示词", "prompt", "提示"].includes(value)
    )
    const nameIndex =
      detectedNameIndex >= 0 ? detectedNameIndex : availableColumns[0].index
    const promptIndex =
      detectedPromptIndex >= 0 && detectedPromptIndex !== nameIndex
        ? detectedPromptIndex
        : availableColumns.find((column) => column.index !== nameIndex)?.index
    if (promptIndex == null) throw new Error("无法找到可用的提示词列")
    columnOptions.value = availableColumns
    nameColumn.value = String(nameIndex)
    promptColumn.value = String(promptIndex)
    importedRows.value = matrix.slice(1)
    batchQueue.value = null
    applyImportedRows()
    if (!rows.value.length) throw new Error("没有找到名称和提示词数据")
    fileName.value = file.name
    createMessage.success(`已导入 ${rows.value.length} 条批量任务`)
  } catch (error) {
    errorMessage.value = error.message || String(error)
  }
}

function applyImportedRows() {
  if (!running.value) batchQueue.value = null
  const nameIndex = Number(nameColumn.value)
  const promptIndex = Number(promptColumn.value)
  if (
    !Number.isInteger(nameIndex) ||
    !Number.isInteger(promptIndex) ||
    nameIndex < 0 ||
    promptIndex < 0 ||
    nameIndex === promptIndex
  ) {
    rows.value = []
    return
  }
  rows.value = importedRows.value
    .map((item) =>
      createRow(
        String(item[nameIndex] ?? "").trim(),
        String(item[promptIndex] ?? "").trim()
      )
    )
    .filter((item) => item.name || item.prompt)
}

function buildQueue() {
  // 所有任务进入同一个会话，由后端并发限制负责排队执行。
  batchQueue.value = {
    id: crypto.randomUUID(),
    rows: selectedRows.value.slice(),
    status: "pending"
  }
}

async function submitQueue(queue) {
  queue.status = "processing"
  const roundId = crypto.randomUUID()
  for (const row of queue.rows) {
    row.status = "queued"
    try {
      const result = await toolboxApi.submitImageTask({
        ...props.settings,
        mode: "generate",
        prompt: row.prompt,
        model: selectedModel.value,
        size: selectedSize.value,
        n: 1,
        images: [],
        mask: "",
        conversationId: runningConversationId.value,
        roundId,
        batchName: row.name
      })
      row.taskId = result.items?.[0]?.id || ""
    } catch (error) {
      row.status = "failed"
      row.error = String(error)
    }
  }
  const pending = queue.rows.filter((row) => row.taskId)
  if (pending.length) await waitForTasks(pending)
  const successIds = queue.rows
    .filter(
      (row) =>
        row.taskId &&
        ["completed", "partial"].includes(row.status) &&
        row.imageCount !== 0
    )
    .map((row) => row.taskId)
  if (successIds.length) await downloadBatch(successIds)
  queue.status = queue.rows.some((row) =>
    ["failed", "interrupted"].includes(row.status)
  )
    ? "partial"
    : "completed"
}

async function waitForTasks(taskRows) {
  // 只轮询当前批量会话，避免其他会话的任务影响进度。
  const ids = new Set(taskRows.map((row) => row.taskId))
  while (ids.size) {
    const seen = new Set()
    let page = 1
    let pageCount = 1
    do {
      const result = await toolboxApi.listImageTasks({
        page,
        pageSize: 100,
        conversationId: runningConversationId.value,
        groupByRound: false
      })
      for (const task of result.items || []) {
        if (!ids.has(task.id)) continue
        seen.add(task.id)
        const row = taskRows.find((item) => item.taskId === task.id)
        if (!row) continue
        row.status = task.status
        row.imageCount = task.imageCount || 0
        row.error = task.error?.message || ""
        if (
          ["completed", "partial", "failed", "interrupted"].includes(
            task.status
          )
        )
          ids.delete(task.id)
      }
      pageCount = Math.max(1, Math.ceil((result.total || 0) / 100))
      page += 1
    } while (ids.size && page <= pageCount)

    // 任务可能被用户从队列删除，避免轮询永久等待已不存在的任务。
    for (const row of taskRows) {
      if (ids.has(row.taskId) && !seen.has(row.taskId)) {
        row.status = "interrupted"
        row.error = "任务已从队列移除"
        ids.delete(row.taskId)
      }
    }
    if (ids.size)
      await new Promise((resolve) => window.setTimeout(resolve, 1200))
  }
}

async function downloadBatch(ids) {
  const targetPath = await systemApi.saveFile({
    title: "保存批量图片",
    defaultPath: "image-batch.zip",
    filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }]
  })
  if (targetPath) {
    await toolboxApi.exportImageTasks({ ids, targetPath })
    createMessage.success("批量图片已导出")
  }
}

async function startBatch() {
  // 入队前重置本地状态，数据库中的任务记录负责跨重启恢复。
  if (!canStart.value || running.value) return
  running.value = true
  errorMessage.value = ""
  runningConversationId.value = props.conversationId || crypto.randomUUID()
  if (!props.conversationId)
    emit("session-created", runningConversationId.value)
  rows.value.forEach((row) => {
    row.status = row.included ? "pending" : "skipped"
    row.taskId = ""
    row.error = ""
  })
  buildQueue()
  // 创建会话后立即退出配置弹框，后续提交和进度交给工作台队列展示。
  emit("close")
  try {
    await submitQueue(batchQueue.value)
    const failed = rows.value.filter((row) =>
      ["failed", "interrupted"].includes(row.status)
    ).length
    emit("finished", {
      total: rows.value.length,
      failed
    })
    if (!failed) emit("close")
  } catch (error) {
    errorMessage.value = String(error)
  } finally {
    running.value = false
  }
}

function handleClose() {
  emit("close")
}

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      selectedModel.value = props.settings.model || props.models[0]?.id || ""
      selectedSize.value =
        props.settings.size ||
        (props.sizePresets[0]
          ? `${props.sizePresets[0].width}x${props.sizePresets[0].height}`
          : "1024x1024")
      if (!rows.value.length) addRow()
    }
  },
  { immediate: true }
)
</script>

<style scoped lang="less">
.image-batch-modal {
  :deep(.base-modal__panel) {
    width: min(1000px, calc(100vw - 48px));
  }

  :deep(.base-modal__content) {
    min-height: 0;
    overflow: hidden;
  }
}

.image-batch-dialog {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  gap: 14px;
  overflow: hidden;
  color: var(--color-text);

  .batch-dialog-scroll {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 14px;
    overflow-x: hidden;
    overflow-y: auto;
    padding-right: 4px;
    scrollbar-gutter: stable;
  }

  .batch-column-error {
    margin: -2px 0 0;
    color: var(--color-danger);
    font-size: 11px;
    line-height: 1.4;
  }

  .batch-column-select {
    width: 28px;
    flex: 0 0 28px;
    text-align: center;

    input {
      width: 14px;
      height: 14px;
      margin: 0;
      accent-color: var(--color-primary);
      cursor: pointer;
    }

    input:disabled {
      cursor: not-allowed;
    }
  }

  .batch-workspace {
    display: flex;
    min-height: 320px;
    gap: 14px;
  }

  .batch-queue-area {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 8px;
  }

  .batch-queue-area-head,
  .batch-settings-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--color-text);
    font-size: 13px;
  }

  .batch-queue-area-head span:last-child,
  .batch-settings-head span:last-child {
    color: var(--color-text-muted);
    font-size: 11px;
  }

  .batch-settings-area {
    display: flex;
    width: 230px;
    flex: 0 0 230px;
    flex-direction: column;
    gap: 12px;
  }

  .batch-settings-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  .batch-settings-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--color-text-muted);
    font-size: 12px;

    select {
      width: 100%;
      height: 32px;
      padding: 0 8px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text);
    }
  }

  .batch-toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;

    .batch-file-picker,
    .batch-action-button {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      height: 34px;
      padding: 0 12px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
      font-size: 13px;
    }

    .batch-file-picker {
      position: relative;
      overflow: hidden;
      color: var(--color-primary);
      border-color: var(--color-primary);

      input {
        position: absolute;
        inset: 0;
        opacity: 0;
        cursor: pointer;
      }
    }

    .batch-action-button:hover:not(:disabled) {
      border-color: var(--color-primary);
      color: var(--color-primary);
      background: var(--color-primary-soft);
    }

    .batch-clear-button {
      color: var(--color-danger);
    }
  }

  .batch-hint {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--color-text-muted);
    font-size: 12px;
  }

  .batch-table-wrap {
    min-height: 220px;
    max-height: 370px;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
  }

  .batch-table-head,
  .batch-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .batch-table-head {
    min-height: 34px;
    padding: 0 12px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 12px;
  }

  .batch-table-body {
    max-height: 334px;
    overflow-y: auto;
  }

  .batch-row {
    min-height: 62px;
    padding: 7px 12px;
    border-top: 1px solid var(--color-line);
  }

  .batch-column-index {
    width: 40px;
    flex: 0 0 40px;
    color: var(--color-text-muted);
    text-align: center;
  }

  .batch-column-name {
    width: 150px;
    flex: 0 0 150px;
  }

  .batch-column-prompt {
    min-width: 0;
    flex: 1;
  }

  .batch-column-status {
    width: 94px;
    flex: 0 0 94px;
  }

  .batch-column-action {
    width: 34px;
    flex: 0 0 34px;
  }

  .batch-field {
    min-height: 34px;
    padding: 7px 9px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    outline: none;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    resize: vertical;
  }

  .batch-field:focus {
    border-color: var(--color-primary);
  }

  .batch-status {
    display: inline-flex;
    align-items: center;
    min-height: 24px;
    padding: 0 7px;
    border-radius: 9999px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 11px;
    white-space: nowrap;
  }

  .batch-status.processing,
  .batch-status.queued {
    color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  .batch-status.completed {
    color: var(--color-success);
    background: rgba(16, 185, 129, 0.1);
  }

  .batch-status.failed,
  .batch-status.partial {
    color: var(--color-danger);
    background: rgba(239, 68, 68, 0.1);
  }

  .batch-status.skipped {
    color: var(--color-text-muted);
    background: var(--color-line);
  }

  .batch-column-status.is-skipped {
    opacity: 0.7;
  }

  .batch-row-error {
    display: block;
    overflow: hidden;
    color: var(--color-danger);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .batch-remove-button {
    display: inline-grid;
    width: 28px;
    height: 28px;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .batch-remove-button:hover:not(:disabled) {
    color: var(--color-danger);
    background: rgba(239, 68, 68, 0.1);
  }

  .batch-empty {
    display: flex;
    min-height: 220px;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 8px;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    color: var(--color-text-muted);

    p {
      margin: 0;
      font-size: 13px;
    }
  }

  .batch-progress-panel {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  .batch-progress-head {
    display: flex;
    align-items: center;
    justify-content: space-between;

    div {
      display: flex;
      align-items: center;
      gap: 10px;
    }
  }

  .batch-progress-title {
    font-size: 13px;
  }

  .batch-progress-meta,
  .batch-progress-count {
    color: var(--color-text-muted);
    font-size: 12px;
  }

  .batch-progress-track {
    height: 6px;
    overflow: hidden;
    border-radius: 9999px;
    background: var(--color-line);

    span {
      display: block;
      height: 100%;
      border-radius: inherit;
      background: var(--color-primary);
      transition: width 0.2s ease;
    }
  }

  .batch-group-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .batch-group-pill {
    padding: 4px 7px;
    border: 1px solid var(--color-line);
    border-radius: 9999px;
    color: var(--color-text-muted);
    font-size: 11px;
  }

  .batch-group-pill.processing {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }

  .batch-group-pill.completed {
    border-color: var(--color-success);
    color: var(--color-success);
  }

  .batch-group-pill.partial {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }

  .batch-footer {
    display: flex;
    flex: none;
    min-height: 36px;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .batch-error {
    margin: 0;
    color: var(--color-danger);
    font-size: 12px;
  }

  .batch-footer-actions {
    display: flex;
    margin-left: auto;
    gap: 8px;
  }

  .batch-secondary-button,
  .batch-primary-button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 34px;
    padding: 0 14px;
    border-radius: 7px;
    cursor: pointer;
    font-size: 13px;
  }

  .batch-secondary-button {
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    color: var(--color-text);
  }

  .batch-primary-button {
    border: 1px solid #2563eb;
    background: #2563eb;
    color: #ffffff;
  }

  .batch-primary-button:hover:not(:disabled) {
    background: #1d4ed8;
  }

  .batch-primary-button:disabled,
  .batch-secondary-button:disabled,
  .batch-action-button:disabled,
  .batch-remove-button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .batch-spinner {
    animation: image-batch-spin 0.9s linear infinite;
  }
}

@media (max-width: 760px) {
  .image-batch-modal {
    :deep(.base-modal__panel) {
      width: calc(100vw - 32px);
    }

    :deep(.base-modal__header) {
      padding: 14px 16px 8px;
    }

    :deep(.base-modal__content) {
      padding: 0 16px 16px;
    }
  }

  .image-batch-dialog {
    .batch-workspace {
      flex-direction: column;
    }

    .batch-settings-area {
      width: auto;
      flex-basis: auto;
    }

    .batch-table-wrap {
      overflow-x: auto;
    }

    .batch-table-head,
    .batch-row {
      min-width: 560px;
    }

    .batch-footer {
      flex-wrap: wrap;
    }

    .batch-error {
      flex: 1 0 100%;
    }

    .batch-footer-actions {
      width: 100%;
      justify-content: flex-end;
    }
  }
}

@keyframes image-batch-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

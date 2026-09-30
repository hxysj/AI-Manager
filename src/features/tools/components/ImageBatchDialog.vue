<template>
  <BaseModal v-if="visible" class="image-batch-modal" @close="handleClose">
    <!-- 自定义弹窗顶部 Header -->
    <template #header>
      <div class="batch-modal-header">
        <div class="batch-modal-badge">
          <ImageIcon :size="20" class="batch-modal-badge-icon" />
        </div>
        <div class="batch-modal-header-text">
          <h2 class="batch-modal-title">批量生成</h2>
          <p class="batch-modal-subtitle">
            按设定并发数执行，可批量创建图片生成任务。
          </p>
        </div>
      </div>
      <button
        class="batch-modal-close-btn"
        type="button"
        title="关闭"
        @click="handleClose"
      >
        <X :size="16" />
      </button>
    </template>

    <div class="image-batch-dialog">
      <!-- 顶部操作工具栏 -->
      <section class="batch-toolbar">
        <div class="batch-btn is-primary is-file-picker">
          <Upload :size="14" />
          <span>{{ fileName || "导入 XLSX" }}</span>
          <input
            type="file"
            accept=".xlsx,.xls"
            :disabled="running"
            class="batch-file-input"
            @change="importWorkbook"
          />
        </div>

        <button
          class="batch-btn is-outline"
          type="button"
          :disabled="running"
          @click="addRow"
        >
          <Plus :size="14" />
          <span>添加任务</span>
        </button>

        <button
          class="batch-btn is-danger-outline"
          type="button"
          :disabled="running || !rows.length"
          @click="clearRows"
        >
          <Trash2 :size="14" />
          <span>清空</span>
        </button>
      </section>

      <!-- 双列工作区 -->
      <div class="batch-workspace">
        <!-- 左侧列：任务列表卡片 -->
        <section class="batch-queue-area">
          <div class="batch-queue-card">
            <!-- 列表卡片顶栏 -->
            <header class="batch-queue-card-head">
              <div class="batch-head-left">
                <!-- 全选复选框 -->
                <label class="batch-check-label">
                  <input
                    v-model="allRowsSelected"
                    type="checkbox"
                    :indeterminate="selectedRows.length > 0 && !allRowsSelected"
                    :disabled="running || !rows.length"
                  />
                  <span>全选</span>
                </label>

                <!-- 批量选择下拉菜单 -->
                <div class="batch-select-dropdown-wrap">
                  <button
                    class="batch-select-menu-btn"
                    type="button"
                    :disabled="running || !rows.length"
                    @click="batchSelectMenuOpen = !batchSelectMenuOpen"
                  >
                    <span>批量选择</span>
                    <ChevronDown :size="12" />
                  </button>

                  <div
                    v-if="batchSelectMenuOpen"
                    class="batch-menu-backdrop"
                    @click="batchSelectMenuOpen = false"
                  />

                  <div
                    v-if="batchSelectMenuOpen"
                    class="batch-dropdown-popover"
                  >
                    <button
                      type="button"
                      class="batch-dropdown-item"
                      @click="selectAllRows"
                    >
                      全部选中
                    </button>
                    <button
                      type="button"
                      class="batch-dropdown-item"
                      @click="deselectAllRows"
                    >
                      取消全选
                    </button>
                    <button
                      type="button"
                      class="batch-dropdown-item"
                      @click="invertSelection"
                    >
                      反向选择
                    </button>
                    <button
                      type="button"
                      class="batch-dropdown-item"
                      @click="selectPendingOnly"
                    >
                      仅选待生成
                    </button>
                  </div>
                </div>

                <!-- 已选计数 -->
                <span class="batch-selected-badge">
                  已选 {{ selectedRows.length }} / {{ rows.length }} 项
                </span>
              </div>
            </header>

            <!-- 任务表格容器 -->
            <div v-if="rows.length" class="batch-table-shell">
              <div class="batch-table-header">
                <span class="col-check"></span>
                <span class="col-index">序号</span>
                <span class="col-name">名称</span>
                <span class="col-prompt">提示词</span>
                <span class="col-status">状态</span>
                <span class="col-action">操作</span>
              </div>

              <div class="batch-table-body">
                <div
                  v-for="(row, index) in rows"
                  :key="row.id"
                  class="batch-table-row"
                  :class="{ 'is-excluded': !row.included }"
                >
                  <!-- 行勾选 -->
                  <div class="col-check">
                    <input
                      v-model="row.included"
                      type="checkbox"
                      :disabled="running"
                    />
                  </div>

                  <!-- 序号 -->
                  <div class="col-index">
                    <span class="batch-row-index">{{ index + 1 }}</span>
                  </div>

                  <!-- 图片名称 -->
                  <div class="col-name">
                    <input
                      v-model.trim="row.name"
                      class="batch-input"
                      placeholder="图片名称"
                      :disabled="running"
                    />
                  </div>

                  <!-- 提示词输入 -->
                  <div class="col-prompt">
                    <textarea
                      v-model.trim="row.prompt"
                      class="batch-textarea"
                      rows="2"
                      placeholder="输入提示词..."
                      :disabled="running"
                    />
                  </div>

                  <!-- 状态徽章 -->
                  <div class="col-status">
                    <div class="batch-status-pill" :class="`is-${row.status}`">
                      <span class="status-dot" />
                      <span>{{ statusLabel(row) }}</span>
                    </div>
                    <small
                      v-if="row.error"
                      class="batch-row-error"
                      :title="row.error"
                    >
                      {{ row.error }}
                    </small>
                  </div>

                  <!-- 操作按钮 -->
                  <div class="col-action">
                    <button
                      class="batch-row-del-btn"
                      type="button"
                      title="删除任务"
                      :disabled="running"
                      @click="removeRow(row.id)"
                    >
                      <Trash2 :size="14" />
                    </button>
                  </div>
                </div>
              </div>
            </div>

            <!-- 空列表占位 -->
            <div v-else class="batch-empty-placeholder">
              <div class="empty-icon-circle">
                <FileSpreadsheet :size="28" />
              </div>
              <h4 class="empty-title">暂无批量任务</h4>
              <p class="empty-desc">
                导入 XLSX 表格或点击上方“添加任务”开始配置
              </p>
              <button
                class="batch-btn is-outline is-sm"
                type="button"
                :disabled="running"
                @click="addRow"
              >
                <Plus :size="13" />
                <span>添加第一条任务</span>
              </button>
            </div>
          </div>
        </section>

        <!-- 右侧列：生成参数卡片 -->
        <aside class="batch-settings-area">
          <div class="batch-settings-card">
            <!-- 参数卡片头部 -->
            <header class="batch-settings-head">
              <div class="settings-head-left">
                <Settings :size="16" class="settings-head-icon" />
                <span class="settings-head-title">生成参数</span>
              </div>
              <span
                class="settings-head-link"
                title="批量任务独立设置模型、尺寸与输出数量"
              >
                批量任务独立设置 &gt;
              </span>
            </header>

            <!-- 生成模型 -->
            <div class="batch-form-field">
              <label class="batch-field-label">生成模型</label>
              <div class="batch-select-wrap">
                <select
                  v-model="selectedModel"
                  class="batch-select"
                  :disabled="running"
                >
                  <option
                    v-for="model in models"
                    :key="model.id"
                    :value="model.id"
                  >
                    {{ model.id }}
                  </option>
                </select>
                <ChevronDown :size="14" class="select-chevron" />
              </div>
            </div>

            <!-- 图片尺寸 -->
            <div class="batch-form-field">
              <label class="batch-field-label">图片尺寸</label>
              <div class="batch-select-wrap">
                <select
                  v-model="selectedSize"
                  class="batch-select"
                  :disabled="running"
                >
                  <option
                    v-for="preset in sizePresets"
                    :key="preset.label"
                    :value="`${preset.width}x${preset.height}`"
                  >
                    {{ preset.label }} · {{ preset.width }}×{{ preset.height }}
                  </option>
                </select>
                <ChevronDown :size="14" class="select-chevron" />
              </div>
            </div>

            <!-- 输出数量 -->
            <div class="batch-form-field">
              <div class="batch-field-label-row">
                <span class="batch-field-label">输出数量</span>
                <span
                  class="batch-help-icon"
                  title="每次任务生成的图片数量 (1–4)"
                >
                  <HelpCircle :size="13" />
                </span>
              </div>
              <div class="batch-stepper">
                <button
                  class="stepper-btn"
                  type="button"
                  :disabled="running || outputCount <= 1"
                  @click="outputCount = Math.max(1, outputCount - 1)"
                >
                  <Minus :size="14" />
                </button>
                <span class="stepper-val">{{ outputCount }}</span>
                <button
                  class="stepper-btn"
                  type="button"
                  :disabled="running || outputCount >= 4"
                  @click="outputCount = Math.min(4, outputCount + 1)"
                >
                  <Plus :size="14" />
                </button>
              </div>
            </div>

            <!-- 并发数量 -->
            <div class="batch-form-field">
              <div class="batch-field-label-row">
                <span class="batch-field-label">并发任务数</span>
                <span
                  class="batch-help-icon"
                  title="同时执行的批量任务数量 (1–10)，默认 2"
                >
                  <HelpCircle :size="13" />
                </span>
              </div>
              <div class="batch-stepper">
                <button
                  class="stepper-btn"
                  type="button"
                  :disabled="running || concurrency <= 1"
                  @click="concurrency = Math.max(1, concurrency - 1)"
                >
                  <Minus :size="14" />
                </button>
                <span class="stepper-val">{{ concurrency }}</span>
                <button
                  class="stepper-btn"
                  type="button"
                  :disabled="running || concurrency >= 10"
                  @click="concurrency = Math.min(10, concurrency + 1)"
                >
                  <Plus :size="14" />
                </button>
              </div>
            </div>

            <!-- 导入列映射 (导入 Excel 后展示) -->
            <div v-if="columnOptions.length" class="batch-mapping-section">
              <header class="batch-mapping-head">
                <span class="mapping-head-title">导入列映射</span>
                <span class="mapping-head-sub">生成前可调整</span>
              </header>

              <div class="batch-form-field">
                <label class="batch-field-label">名称列</label>
                <div class="batch-select-wrap">
                  <select
                    v-model="nameColumn"
                    class="batch-select"
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
                  <ChevronDown :size="14" class="select-chevron" />
                </div>
              </div>

              <div class="batch-form-field">
                <label class="batch-field-label">提示词列</label>
                <div class="batch-select-wrap">
                  <select
                    v-model="promptColumn"
                    class="batch-select"
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
                  <ChevronDown :size="14" class="select-chevron" />
                </div>
              </div>

              <p v-if="columnSelectionError" class="batch-column-error">
                {{ columnSelectionError }}
              </p>
            </div>

            <!-- 执行进度 (批处理进行中展示) -->
            <div v-if="batchQueue" class="batch-progress-section">
              <header class="batch-progress-head">
                <span class="progress-head-title">执行进度</span>
                <span class="progress-head-count">
                  {{ completedCount }} / {{ rows.length }}
                </span>
              </header>
              <div class="batch-progress-track">
                <div
                  class="batch-progress-bar"
                  :style="{ width: `${progressPercent}%` }"
                />
              </div>
              <div class="batch-group-list">
                <span class="batch-group-pill" :class="batchQueue.status">
                  全部任务 · {{ batchQueue.rows.length }} 条
                </span>
              </div>
            </div>
          </div>
        </aside>
      </div>

      <!-- 弹窗底部操作与提示栏 -->
      <footer class="batch-footer">
        <div class="batch-footer-info">
          <div class="batch-footer-hint">
            <Info :size="14" class="hint-icon" />
            <span>
              导入表头需包含 name 和 prompt，也可在右侧生成参数中指定列。
            </span>
          </div>
          <p v-if="errorMessage" class="batch-footer-error">
            {{ errorMessage }}
          </p>
        </div>

        <div class="batch-footer-actions">
          <button
            class="batch-footer-btn is-secondary"
            type="button"
            :disabled="running"
            @click="handleClose"
          >
            取消
          </button>
          <button
            class="batch-footer-btn is-primary"
            type="button"
            :disabled="running || !canStart"
            @click="startBatch"
          >
            <LoaderCircle v-if="running" :size="14" class="spinning" />
            <Play v-else :size="14" fill="currentColor" />
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
  ChevronDown,
  FileSpreadsheet,
  HelpCircle,
  Image as ImageIcon,
  Info,
  LoaderCircle,
  Minus,
  Play,
  Plus,
  Settings,
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
const emit = defineEmits([
  "close",
  "finished",
  "session-created",
  "tasks-created"
])

const rows = ref([])
const fileName = ref("")
const running = ref(false)
const errorMessage = ref("")
const batchQueue = ref(null)
const runningConversationId = ref("")
const selectedModel = ref("")
const selectedSize = ref("")
const outputCount = ref(1)
// 默认限制为 2 个并发，避免批量任务瞬间占满账号额度。
const concurrency = ref(2)
const columnOptions = ref([])
const nameColumn = ref("")
const promptColumn = ref("")
const importedRows = ref([])
const batchSelectMenuOpen = ref(false)

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

function selectAllRows() {
  rows.value.forEach((row) => (row.included = true))
  batchSelectMenuOpen.value = false
}

function deselectAllRows() {
  rows.value.forEach((row) => (row.included = false))
  batchSelectMenuOpen.value = false
}

function invertSelection() {
  rows.value.forEach((row) => (row.included = !row.included))
  batchSelectMenuOpen.value = false
}

function selectPendingOnly() {
  rows.value.forEach((row) => (row.included = row.status === "pending"))
  batchSelectMenuOpen.value = false
}

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
  batchQueue.value = {
    id: crypto.randomUUID(),
    rows: selectedRows.value.slice(),
    status: "pending"
  }
}

async function submitQueue(queue) {
  queue.status = "processing"
  const roundId = crypto.randomUUID()
  queue.rows.forEach((row) => {
    row.status = "queued"
    row.taskId = ""
    row.error = ""
  })
  try {
    const result = await toolboxApi.submitImageBatch({
      ...props.settings,
      model: selectedModel.value,
      size: selectedSize.value,
      n: outputCount.value,
      concurrency: concurrency.value,
      conversationId: runningConversationId.value,
      roundId,
      items: queue.rows.map((row) => ({
        batchName: row.name,
        prompt: row.prompt
      }))
    })
    queue.rows.forEach((row, index) => {
      row.taskId = result.items?.[index]?.id || ""
      if (!row.taskId) {
        row.status = "failed"
        row.error = "批量任务记录创建失败"
      }
    })
    // 任务记录已经落库，通知工作台立即展示队列。
    if (queue.rows.some((row) => row.taskId)) emit("tasks-created")
  } catch (error) {
    queue.rows.forEach((row) => {
      row.status = "failed"
      row.error = String(error)
    })
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
  if (!canStart.value || running.value) return
  running.value = true
  errorMessage.value = ""
  runningConversationId.value = props.conversationId || crypto.randomUUID()
  rows.value.forEach((row) => {
    row.status = row.included ? "pending" : "skipped"
    row.taskId = ""
    row.error = ""
  })
  buildQueue()
  // 记录落库前先传递预期数量，避免队列头短暂显示 0/0。
  if (!props.conversationId)
    emit(
      "session-created",
      runningConversationId.value,
      batchQueue.value.rows.length,
      concurrency.value
    )
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
      outputCount.value = Number(props.settings.n) || 1
      if (!rows.value.length) addRow()
    }
  },
  { immediate: true }
)
</script>

<style scoped lang="less">
/* =========================================================================
   BaseModal 弹窗样式定制
   ========================================================================= */
.image-batch-modal {
  :deep(.base-modal__panel) {
    width: min(1040px, calc(100vw - 40px));
    max-height: calc(100vh - 48px);
    border-radius: 14px;
    box-shadow: 0 20px 48px -8px rgba(15, 23, 42, 0.18);
    border: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  :deep(.base-modal__header) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-line);
  }

  :deep(.base-modal__content) {
    padding: 16px 20px 20px;
    min-height: 0;
    overflow: hidden;
  }
}

/* 弹窗 Header */
.batch-modal-header {
  display: flex;
  align-items: center;
  gap: 12px;
}

.batch-modal-badge {
  display: grid;
  width: 38px;
  height: 38px;
  flex-shrink: 0;
  place-items: center;
  border-radius: 10px;
  background: #2563eb;
  color: #ffffff;
  box-shadow: 0 2px 6px rgba(37, 99, 235, 0.25);

  .batch-modal-badge-icon {
    stroke-width: 2.2;
  }
}

.batch-modal-header-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.batch-modal-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: var(--color-text);
  line-height: 1.3;
}

.batch-modal-subtitle {
  margin: 0;
  font-size: 12.5px;
  color: var(--color-text-muted);
}

.batch-modal-close-btn {
  display: grid;
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  place-items: center;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: var(--color-panel);
  color: var(--color-text-muted);
  cursor: pointer;
  transition: all 0.16s ease;

  &:hover {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    color: var(--color-text);
  }
}

/* =========================================================================
   弹窗主体容器
   ========================================================================= */
.image-batch-dialog {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  gap: 14px;
  overflow: hidden;
  color: var(--color-text);
}

/* =========================================================================
   顶部操作工具栏 (Toolbar)
   ========================================================================= */
.batch-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.batch-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  border-radius: 6px;
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.16s ease;

  &:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  &.is-primary {
    border: 0;
    background: #2563eb;
    color: #ffffff;
    box-shadow: 0 2px 4px rgba(37, 99, 235, 0.2);

    &:hover:not(:disabled) {
      background: #1d4ed8;
    }
  }

  &.is-outline {
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    color: var(--color-text);

    &:hover:not(:disabled) {
      border-color: #2563eb;
      color: #2563eb;
      background: var(--color-primary-soft);
    }
  }

  &.is-danger-outline {
    border: 1px solid var(--color-danger-line, #fecaca);
    background: var(--color-danger-soft, #fff5f5);
    color: var(--color-danger, #ef4444);

    &:hover:not(:disabled) {
      border-color: var(--color-danger, #ef4444);
      background: var(--color-danger, #ef4444);
      color: #ffffff;
    }
  }

  &.is-sm {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
}

.batch-btn.is-file-picker {
  position: relative;
  overflow: hidden;

  .batch-file-input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
}

/* =========================================================================
   双列工作区布局 (Asymmetric 2-Column Layout)
   ========================================================================= */
.batch-workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 280px;
  gap: 14px;
  min-height: 0;
  flex: 1;
}

/* =========================================================================
   左侧列：任务队列卡片
   ========================================================================= */
.batch-queue-area {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}

.batch-queue-card {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
  border: 1px solid var(--color-line);
  border-radius: 10px;
  background: var(--color-panel);
  overflow: hidden;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.03);
}

.batch-queue-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: var(--color-panel-soft);
  border-bottom: 1px solid var(--color-line);
  flex-shrink: 0;

  .batch-head-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }
}

.batch-check-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--color-text);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  user-select: none;

  input[type="checkbox"] {
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: #2563eb;
    cursor: pointer;
  }
}

/* 批量选择下拉菜单 */
.batch-select-dropdown-wrap {
  position: relative;
  display: inline-block;
}

.batch-select-menu-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--color-line);
  border-radius: 5px;
  background: var(--color-panel);
  color: var(--color-text-muted);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.16s ease;

  &:hover:not(:disabled) {
    border-color: var(--color-line-strong);
    color: var(--color-text);
  }
}

.batch-menu-backdrop {
  position: fixed;
  inset: 0;
  z-index: 20;
}

.batch-dropdown-popover {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 25;
  min-width: 110px;
  padding: 4px;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel);
  box-shadow: 0 4px 14px rgba(15, 23, 42, 0.12);

  .batch-dropdown-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--color-text);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: all 0.14s ease;

    &:hover {
      background: var(--color-panel-soft);
      color: #2563eb;
    }
  }
}

.batch-selected-badge {
  color: var(--color-text-muted);
  font-size: 12px;
}

/* =========================================================================
   任务表格布局 (Grid Table)
   ========================================================================= */
.batch-table-shell {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
}

.batch-table-header {
  display: grid;
  grid-template-columns: 36px 42px 140px minmax(0, 1fr) 96px 44px;
  align-items: center;
  padding: 8px 12px;
  background: var(--color-panel-soft);
  border-bottom: 1px solid var(--color-line);
  color: var(--color-text-soft);
  font-size: 11.5px;
  font-weight: 600;
  flex-shrink: 0;

  .col-check,
  .col-index,
  .col-action {
    text-align: center;
  }
}

.batch-table-body {
  flex: 1;
  min-height: 0;
  max-height: 360px;
  overflow-y: auto;
  scrollbar-gutter: stable;
}

.batch-table-row {
  display: grid;
  grid-template-columns: 36px 42px 140px minmax(0, 1fr) 96px 44px;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--color-line);
  transition: background-color 0.14s ease;

  &:hover {
    background: color-mix(in srgb, var(--color-primary-soft) 25%, transparent);
  }

  &.is-excluded {
    opacity: 0.65;
  }

  .col-check {
    display: flex;
    justify-content: center;

    input[type="checkbox"] {
      width: 14px;
      height: 14px;
      margin: 0;
      accent-color: #2563eb;
      cursor: pointer;
    }
  }

  .col-index {
    display: flex;
    justify-content: center;
  }

  .col-action {
    display: flex;
    justify-content: center;
  }
}

.batch-row-index {
  color: var(--color-text-muted);
  font-size: 12px;
  font-family: "Bahnschrift", monospace;
  font-weight: 600;
}

.batch-input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel);
  color: var(--color-text);
  font-size: 12.5px;
  outline: none;
  transition: border-color 0.16s ease;

  &:focus {
    border-color: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.12);
  }
}

.batch-textarea {
  width: 100%;
  min-height: 44px;
  padding: 6px 10px;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel);
  color: var(--color-text);
  font-size: 12px;
  line-height: 1.45;
  outline: none;
  resize: vertical;
  font-family: inherit;
  transition: border-color 0.16s ease;

  &:focus {
    border-color: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.12);
  }
}

/* 状态徽章 (Status Badge with Dot) */
.batch-status-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 8px;
  border-radius: 9999px;
  font-size: 11px;
  font-weight: 600;
  white-space: nowrap;

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  &.is-pending {
    background: #f1f5f9;
    color: #64748b;
    .status-dot {
      background: #94a3b8;
    }
  }

  &.is-queued {
    background: #fef9c3;
    color: #854d0e;
    .status-dot {
      background: #eab308;
    }
  }

  &.is-processing {
    background: #eff6ff;
    color: #2563eb;
    .status-dot {
      background: #3b82f6;
      animation: batch-dot-pulse 1.4s infinite;
    }
  }

  &.is-completed {
    background: #f0fdf4;
    color: #16a34a;
    .status-dot {
      background: #22c55e;
    }
  }

  &.is-failed,
  &.is-partial {
    background: #fef2f2;
    color: #dc2626;
    .status-dot {
      background: #ef4444;
    }
  }

  &.is-skipped,
  &.is-interrupted {
    background: #f3f4f6;
    color: #9ca3af;
    .status-dot {
      background: #cbd5e1;
    }
  }
}

.batch-row-error {
  display: block;
  max-width: 90px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--color-danger, #ef4444);
  font-size: 10.5px;
  margin-top: 2px;
}

.batch-row-del-btn {
  display: grid;
  width: 28px;
  height: 28px;
  place-items: center;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: #94a3b8;
  cursor: pointer;
  transition: all 0.16s ease;

  &:hover:not(:disabled) {
    background: #fee2e2;
    color: #ef4444;
  }
}

/* 空状态占位 */
.batch-empty-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 24px;
  text-align: center;
  min-height: 240px;

  .empty-icon-circle {
    display: grid;
    width: 52px;
    height: 52px;
    place-items: center;
    border-radius: 50%;
    background: #eff6ff;
    color: #2563eb;
    margin-bottom: 12px;
  }

  .empty-title {
    margin: 0 0 6px;
    color: var(--color-text);
    font-size: 15px;
    font-weight: 600;
  }

  .empty-desc {
    margin: 0 0 16px;
    color: var(--color-text-muted);
    font-size: 12.5px;
  }
}

/* =========================================================================
   右侧列：生成参数卡片 (Parameters Panel)
   ========================================================================= */
.batch-settings-area {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.batch-settings-card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px;
  border: 1px solid var(--color-line);
  border-radius: 10px;
  background: var(--color-panel);
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.03);
  overflow-y: auto;
}

.batch-settings-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--color-line);

  .settings-head-left {
    display: flex;
    align-items: center;
    gap: 6px;

    .settings-head-icon {
      color: #2563eb;
    }

    .settings-head-title {
      font-size: 13.5px;
      font-weight: 700;
      color: var(--color-text);
    }
  }

  .settings-head-link {
    font-size: 11.5px;
    color: #2563eb;
    text-decoration: none;
    cursor: default;
  }
}

.batch-form-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.batch-field-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text-muted);
}

.batch-field-label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;

  .batch-help-icon {
    display: inline-flex;
    color: var(--color-text-soft);
    cursor: help;
  }
}

/* 自定义下拉选择框 */
.batch-select-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.batch-select {
  width: 100%;
  height: 32px;
  padding: 0 26px 0 10px;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel);
  color: var(--color-text);
  font-size: 12.5px;
  outline: none;
  cursor: pointer;
  appearance: none;
  transition: border-color 0.16s ease;

  &:focus {
    border-color: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.12);
  }
}

.select-chevron {
  position: absolute;
  right: 8px;
  color: var(--color-text-muted);
  pointer-events: none;
}

/* 步进器组件 (Stepper) */
.batch-stepper {
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--color-line);
  border-radius: 6px;
  background: var(--color-panel);
  overflow: hidden;
  width: fit-content;

  .stepper-btn {
    display: grid;
    width: 32px;
    height: 30px;
    place-items: center;
    border: 0;
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition: background-color 0.14s ease;

    &:hover:not(:disabled) {
      background: var(--color-panel-soft);
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.4;
    }
  }

  .stepper-val {
    width: 36px;
    height: 30px;
    line-height: 30px;
    text-align: center;
    font-size: 13px;
    font-weight: 700;
    font-family: "Bahnschrift", monospace;
    color: var(--color-text);
    border-left: 1px solid var(--color-line);
    border-right: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    user-select: none;
  }
}

/* 导入列映射 */
.batch-mapping-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--color-line);
}

.batch-mapping-head {
  display: flex;
  align-items: center;
  justify-content: space-between;

  .mapping-head-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text);
  }

  .mapping-head-sub {
    font-size: 11px;
    color: var(--color-text-muted);
  }
}

.batch-column-error {
  margin: 0;
  font-size: 11px;
  color: var(--color-danger, #ef4444);
  line-height: 1.4;
}

/* 执行进度面板 */
.batch-progress-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--color-line);
}

.batch-progress-head {
  display: flex;
  align-items: center;
  justify-content: space-between;

  .progress-head-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text);
  }

  .progress-head-count {
    font-size: 11.5px;
    color: var(--color-text-muted);
    font-family: monospace;
  }
}

.batch-progress-track {
  height: 6px;
  border-radius: 9999px;
  background: var(--color-line);
  overflow: hidden;

  .batch-progress-bar {
    height: 100%;
    border-radius: inherit;
    background: #2563eb;
    transition: width 0.24s ease;
  }
}

.batch-group-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.batch-group-pill {
  padding: 2px 8px;
  border: 1px solid var(--color-line);
  border-radius: 9999px;
  font-size: 11px;
  color: var(--color-text-muted);

  &.processing {
    border-color: #2563eb;
    color: #2563eb;
    background: rgba(37, 99, 235, 0.08);
  }

  &.completed {
    border-color: #16a34a;
    color: #16a34a;
    background: rgba(22, 163, 74, 0.08);
  }

  &.partial {
    border-color: #dc2626;
    color: #dc2626;
    background: rgba(220, 38, 38, 0.08);
  }
}

/* =========================================================================
   弹窗底部 (Footer)
   ========================================================================= */
.batch-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-top: 12px;
  border-top: 1px solid var(--color-line);
  flex-shrink: 0;

  .batch-footer-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .batch-footer-hint {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-text-muted);

    .hint-icon {
      color: #2563eb;
      flex-shrink: 0;
    }
  }

  .batch-footer-error {
    margin: 0;
    color: var(--color-danger, #ef4444);
    font-size: 12px;
  }

  .batch-footer-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
    margin-left: auto;
  }

  .batch-footer-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 34px;
    padding: 0 16px;
    border-radius: 6px;
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.16s ease;

    &:disabled {
      cursor: not-allowed;
      opacity: 0.45;
    }

    &.is-secondary {
      border: 1px solid var(--color-line);
      background: var(--color-panel);
      color: var(--color-text);

      &:hover:not(:disabled) {
        border-color: var(--color-line-strong);
        background: var(--color-panel-soft);
      }
    }

    &.is-primary {
      border: 0;
      background: #2563eb;
      color: #ffffff;
      font-weight: 600;
      padding: 0 18px;
      box-shadow: 0 2px 5px rgba(37, 99, 235, 0.25);

      &:hover:not(:disabled) {
        background: #1d4ed8;
      }
    }
  }
}

.spinning {
  animation: batch-spin 0.8s linear infinite;
}

@keyframes batch-spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes batch-dot-pulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.4;
    transform: scale(1.3);
  }
}

/* =========================================================================
   暗色主题适配 (Dark Mode)
   ========================================================================= */
:global(:root[data-theme="dark"]),
.tools-view--dark & {
  .batch-modal-badge {
    background: #2563eb;
    box-shadow: 0 2px 8px rgba(37, 99, 235, 0.4);
  }

  .batch-queue-card,
  .batch-settings-card {
    background: var(--color-panel);
    border-color: var(--color-line);
  }

  .batch-table-header {
    background: var(--color-panel-soft);
  }

  .batch-empty-placeholder {
    .empty-icon-circle {
      background: rgba(37, 99, 235, 0.15);
      color: #60a5fa;
    }
  }

  .batch-status-pill {
    &.is-pending {
      background: #1e293b;
      color: #94a3b8;
      .status-dot {
        background: #64748b;
      }
    }

    &.is-queued {
      background: rgba(234, 179, 8, 0.16);
      color: #facc15;
      .status-dot {
        background: #eab308;
      }
    }

    &.is-processing {
      background: rgba(37, 99, 235, 0.16);
      color: #60a5fa;
      .status-dot {
        background: #3b82f6;
      }
    }

    &.is-completed {
      background: rgba(34, 197, 94, 0.16);
      color: #4ade80;
      .status-dot {
        background: #22c55e;
      }
    }

    &.is-failed,
    &.is-partial {
      background: rgba(239, 68, 68, 0.16);
      color: #f87171;
      .status-dot {
        background: #ef4444;
      }
    }

    &.is-skipped,
    &.is-interrupted {
      background: rgba(148, 163, 184, 0.16);
      color: #94a3b8;
      .status-dot {
        background: #64748b;
      }
    }
  }

  .batch-btn.is-danger-outline {
    border-color: rgba(239, 68, 68, 0.3);
    background: rgba(239, 68, 68, 0.1);
    color: #f87171;

    &:hover:not(:disabled) {
      background: #ef4444;
      color: #ffffff;
    }
  }

  .batch-row-del-btn:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.18);
    color: #f87171;
  }
}

/* =========================================================================
   响应式折叠
   ========================================================================= */
@media (max-width: 820px) {
  .batch-workspace {
    grid-template-columns: 1fr;
  }

  .batch-settings-area {
    width: 100%;
  }

  .batch-footer {
    flex-direction: column;
    align-items: stretch;

    .batch-footer-actions {
      justify-content: flex-end;
    }
  }
}
</style>

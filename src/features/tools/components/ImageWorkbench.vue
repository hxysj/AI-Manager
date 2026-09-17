<template>
  <section class="image-workbench">
    <form class="create-panel" @submit.prevent="submitTask">
      <header class="panel-head">
        <span data-emphasis>创建任务</span>
        <span class="official-badge"
          ><ShieldCheck :size="12" /> Codex 官方</span
        >
      </header>
      <div class="mode-tabs" role="tablist" aria-label="图片任务类型">
        <button
          v-for="mode in modes"
          :key="mode.value"
          class="mode-tab"
          :class="{ active: form.mode === mode.value }"
          type="button"
          role="tab"
          :aria-selected="form.mode === mode.value"
          @click="form.mode = mode.value"
        >
          <component :is="mode.icon" :size="15" />{{ mode.label }}
        </button>
      </div>
      <label class="form-field">
        <span class="field-label">官方账号</span>
        <select
          v-model="form.accountId"
          class="field-input"
          :disabled="loadingAccounts || submitting"
        >
          <option value="">
            {{ loadingAccounts ? "正在读取账号…" : "请选择官方登录账号" }}
          </option>
          <option
            v-for="account in accounts"
            :key="account.id"
            :value="account.id"
            :disabled="account.disabled || account.requiresReauth"
          >
            {{ account.email || account.id }} ·
            {{
              account.disabled
                ? "已停用"
                : account.requiresReauth
                  ? "需要重新登录"
                  : account.plan || "官方账号"
            }}
          </option>
        </select>
      </label>
      <p
        v-if="!loadingAccounts && !availableAccounts.length"
        class="account-hint"
      >
        请先在 Provider 页面登录或恢复 Codex 官方账号。其他 Provider
        暂不支持图片生成。
      </p>
      <label class="form-field">
        <span class="field-label"
          >提示词 <span class="field-key">prompt</span></span
        >
        <textarea
          v-model="form.prompt"
          class="field-input prompt-input"
          :placeholder="
            form.mode === 'edit'
              ? '描述你希望如何修改参考图，例如：给人物加一顶红色帽子'
              : '描述你想生成的画面，例如：木桌上的红苹果，柔和的摄影棚光线'
          "
          maxlength="16000"
          required
        ></textarea>
      </label>
      <div class="parameter-fields">
        <label v-for="field in fields" :key="field.key" class="form-field">
          <span class="field-label"
            >{{ field.label }}
            <span class="field-key">{{ field.apiName }}</span></span
          >
          <template v-if="field.key === 'model'">
            <input
              v-model.trim="form.model"
              class="field-input"
              list="image-workbench-models"
              placeholder="选择或输入图片模型"
              maxlength="128"
              required
            />
            <datalist id="image-workbench-models">
              <option
                v-for="option in field.options"
                :key="option.value"
                :value="option.value"
              />
            </datalist>
          </template>
          <select v-else v-model="form[field.key]" class="field-input">
            <option
              v-for="option in field.options"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
        </label>
        <label class="form-field">
          <span class="field-label">数量 <span class="field-key">n</span></span>
          <input
            v-model.number="form.n"
            class="field-input"
            type="number"
            min="1"
            max="10"
            step="1"
            required
          />
        </label>
      </div>
      <p
        v-if="form.background === 'transparent' && form.outputFormat === 'jpeg'"
        class="field-error"
      >
        透明背景需要选择 PNG 或 WebP。
      </p>
      <section v-if="form.mode === 'edit'" class="reference-section">
        <div class="reference-head">
          <span>参考图</span
          ><span class="field-key">{{ references.length }} / 10</span>
        </div>
        <div class="reference-list">
          <div
            v-for="(reference, index) in references"
            :key="reference.id"
            class="reference-item"
          >
            <img
              class="reference-image"
              :src="reference.url"
              :alt="reference.name"
            />
            <button
              class="remove-reference"
              type="button"
              :aria-label="`移除 ${reference.name}`"
              @click="references.splice(index, 1)"
            >
              <X :size="12" />
            </button>
            <span class="reference-name" :title="reference.name">{{
              reference.name
            }}</span>
          </div>
          <label v-if="references.length < 10" class="upload-control">
            <Plus :size="21" /><span>添加图片</span>
            <input
              class="upload-input"
              type="file"
              accept="image/png,image/jpeg,image/webp"
              multiple
              :disabled="uploading"
              @change="addImages($event, false)"
            />
          </label>
        </div>
        <p class="reference-hint">PNG / JPEG / WebP，单张最多 20 MB。</p>
        <div class="mask-row">
          <label class="mask-upload">
            <Plus :size="13" />{{ mask ? "更换蒙版" : "添加蒙版（可选）" }}
            <input
              class="upload-input"
              type="file"
              accept="image/png"
              :disabled="uploading"
              @change="addImages($event, true)"
            />
          </label>
          <button
            v-if="mask"
            class="text-button"
            type="button"
            @click="mask = null"
          >
            移除
          </button>
        </div>
        <p v-if="mask" class="reference-hint">
          {{ mask.name }} · 透明区域为编辑范围；尺寸须与首张参考图一致。
        </p>
      </section>
      <button class="submit-button" type="submit" :disabled="!canSubmit">
        <LoaderCircle
          v-if="submitting || uploading"
          class="spinning"
          :size="16"
        /><Sparkles v-else :size="16" />
        {{
          submitting
            ? "正在提交…"
            : form.mode === "edit"
              ? "提交图片编辑任务"
              : "提交文生图任务"
        }}
      </button>
      <p class="submit-hint">
        生成使用所选账号的图片额度，任务在后台继续执行。
      </p>
      <p v-if="submitError" class="field-error" role="alert">
        {{ submitError }}
      </p>
    </form>

    <section class="tasks-panel">
      <header class="tasks-head">
        <div class="tasks-title">
          <span data-emphasis>我的任务</span
          ><span class="task-count">{{ total }}</span>
        </div>
        <div class="tasks-controls">
          <select
            v-model="status"
            class="status-select"
            aria-label="筛选任务状态"
          >
            <option value="">全部状态</option>
            <option
              v-for="(label, key) in statusLabels"
              :key="key"
              :value="key"
            >
              {{ label }}
            </option>
          </select>
          <label class="auto-refresh"
            ><input v-model="autoRefresh" type="checkbox" />自动刷新</label
          >
          <button
            class="icon-button"
            type="button"
            aria-label="刷新账号与任务"
            title="刷新账号与任务"
            :disabled="refreshing"
            @click="refreshAll"
          >
            <RefreshCw :size="15" :class="{ spinning: refreshing }" />
          </button>
          <button class="action-button" type="button" @click="toggleSelection">
            {{ selecting ? "取消" : "选择" }}
          </button>
        </div>
      </header>
      <div v-if="selecting" class="selection-bar">
        <label class="auto-refresh"
          ><input
            type="checkbox"
            :checked="allSelected"
            :disabled="!selectableTasks.length"
            @change="toggleAll"
          />本页全选</label
        >
        <span class="selection-count">已选 {{ selected.length }} 项</span>
        <button
          class="action-button"
          type="button"
          :disabled="!selected.length || exporting"
          @click="exportTasks(selected)"
        >
          <Download :size="14" />导出
        </button>
        <button
          class="action-button danger-button"
          type="button"
          :disabled="!selected.length || deleting"
          @click="deleteTasks"
        >
          <Trash2 :size="14" />删除
        </button>
      </div>
      <p v-if="listError" class="list-error" role="alert">{{ listError }}</p>
      <div v-if="!tasks.length" class="empty-state">
        <div class="empty-icon">
          <ImagePlus :size="30" :stroke-width="1.3" />
        </div>
        <span class="empty-title">{{
          refreshing
            ? "正在读取任务…"
            : status
              ? "暂无此状态的任务"
              : "第一张作品，从这里开始"
        }}</span>
        <span class="empty-description">{{
          status ? "切换状态查看其他任务" : "填写提示词，生成或编辑你的图片"
        }}</span>
      </div>
      <div v-else class="task-list">
        <article
          v-for="task in tasks"
          :key="task.id"
          class="task-card"
          :class="{ 'task-selected': selected.includes(task.id) }"
        >
          <label v-if="selecting" class="task-check"
            ><input
              v-model="selected"
              type="checkbox"
              :value="task.id"
              :disabled="task.status === 'processing'"
              :aria-label="`选择任务 ${task.request.prompt}`"
          /></label>
          <button
            class="task-preview"
            type="button"
            :disabled="!task.imageCount"
            aria-label="预览任务图片"
            @click="showDetail(task)"
          >
            <img
              v-if="task.thumbnail"
              class="task-thumbnail"
              :src="task.thumbnail"
              alt="生成图片缩略图"
            />
            <LoaderCircle
              v-else-if="task.status === 'processing'"
              class="spinning"
              :size="25"
            />
            <ImageOff v-else :size="25" :stroke-width="1.3" />
          </button>
          <div class="task-content">
            <div class="task-meta">
              <span class="task-status" :class="task.status">{{
                statusLabels[task.status]
              }}</span
              ><span class="task-time">{{
                formatDateTime(task.createdAt)
              }}</span>
            </div>
            <p class="task-prompt" :title="task.request.prompt">
              {{ task.request.prompt }}
            </p>
            <p class="task-spec">
              {{ task.request.model }} ·
              {{ task.request.mode === "edit" ? "图片编辑" : "文生图" }} ·
              {{ task.imageCount }} / {{ task.request.n }} 张
            </p>
            <p v-if="task.error?.message" class="task-error">
              {{ task.error.message }}
            </p>
            <div class="task-actions">
              <button
                class="text-button"
                type="button"
                :disabled="detailLoading"
                @click="showDetail(task)"
              >
                查看详情
              </button>
              <button
                class="text-button"
                type="button"
                @click="reuseTask(task)"
              >
                复用参数
              </button>
              <button
                v-if="task.imageCount"
                class="text-button"
                type="button"
                :disabled="exporting"
                @click="exportTasks([task.id])"
              >
                导出图片
              </button>
            </div>
          </div>
        </article>
      </div>
      <footer v-if="total > pageSize" class="pagination">
        <button
          class="action-button"
          type="button"
          :disabled="page <= 1"
          @click="page--"
        >
          上一页
        </button>
        <span>{{ page }} / {{ Math.max(1, Math.ceil(total / pageSize)) }}</span>
        <button
          class="action-button"
          type="button"
          :disabled="page * pageSize >= total"
          @click="page++"
        >
          下一页
        </button>
      </footer>
    </section>

    <BaseModal
      v-if="detail"
      title="图片任务详情"
      :description="`${detail.accountName} · ${formatDateTime(detail.createdAt)} · ${statusLabels[detail.status]}`"
      @close="detail = null"
    >
      <section class="detail-content">
        <p class="detail-prompt">{{ detail.request.prompt }}</p>
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
              {{ detail.images[index]?.size }} · {{ item.output_format
              }}<button
                class="text-button"
                type="button"
                @click="editResult(item)"
              >
                用此图编辑
              </button>
            </figcaption>
          </figure>
        </div>
        <div class="detail-meta">
          <span>模型：{{ detail.request.model }}</span
          ><span>质量：{{ detail.request.quality }}</span
          ><span>请求尺寸：{{ detail.request.size }}</span>
          <span>背景：{{ detail.request.background || "默认" }}</span
          ><span>实际图片：{{ detail.imageCount }} 张</span
          ><span>参考图：{{ detail.inputCount }} 张</span>
        </div>
        <details class="technical-detail">
          <summary>请求与用量记录</summary>
          <pre class="usage-content">{{
            JSON.stringify(
              {
                taskId: detail.id,
                requestId: detail.requestId,
                endpoint: detail.endpoint,
                usage: detail.usage
              },
              null,
              2
            )
          }}</pre>
        </details>
        <p
          v-if="detail.request.responseFormat === 'url' && detail.imageCount"
          class="detail-hint"
        >
          图片保存在本地；URL 为预览用 data URL，不是可分享的公网链接。
        </p>
      </section>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue"
import {
  Download,
  ImageOff,
  ImagePlus,
  LoaderCircle,
  Paintbrush,
  Plus,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Trash2,
  X
} from "lucide-vue-next"
import { ElMessageBox } from "element-plus"
import BaseModal from "@/components/BaseModal.vue"
import { systemApi, toolboxApi } from "@/api"
import { subscribe } from "@/api/request"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const modes = [
  { value: "generate", label: "文生图", icon: Sparkles },
  { value: "edit", label: "图片编辑", icon: Paintbrush }
]
const statusLabels = {
  processing: "生成中",
  completed: "已完成",
  partial: "部分完成",
  failed: "失败",
  interrupted: "已中断"
}
const fields = [
  {
    key: "model",
    label: "模型",
    apiName: "model",
    options: [
      "gpt-image-2",
      "gpt-image-1.5",
      "gpt-image-1",
      "gpt-image-2.5-flare",
      "gpt-image-2.5-sunburst",
      "gpt-image-2.5-flare-2026-09-08",
      "gpt-image-2.5-sunburst-2026-09-08"
    ].map((value) => ({ value, label: value }))
  },
  {
    key: "size",
    label: "尺寸",
    apiName: "size",
    options: [
      { value: "1024x1024", label: "1K · 1024 × 1024" },
      { value: "1024x1536", label: "竖图 · 1024 × 1536" },
      { value: "1536x1024", label: "横图 · 1536 × 1024" },
      { value: "2048x2048", label: "2K · 2048 × 2048" },
      { value: "auto", label: "自动" }
    ]
  },
  {
    key: "quality",
    label: "质量",
    apiName: "quality",
    options: ["high", "medium", "low", "auto"].map((value) => ({
      value,
      label: value
    }))
  },
  {
    key: "outputFormat",
    label: "图片格式",
    apiName: "output_format",
    options: ["png", "jpeg", "webp"].map((value) => ({ value, label: value }))
  },
  {
    key: "background",
    label: "背景",
    apiName: "background",
    options: [
      { value: "", label: "默认（不传）" },
      { value: "auto", label: "自动" },
      { value: "opaque", label: "不透明" },
      { value: "transparent", label: "透明" }
    ]
  },
  {
    key: "responseFormat",
    label: "响应格式",
    apiName: "response_format",
    options: [
      { value: "url", label: "url（本地图片）" },
      { value: "b64_json", label: "b64_json" }
    ]
  }
]
const form = reactive({
  accountId: "",
  mode: "generate",
  prompt: "",
  model: "gpt-image-2",
  size: "1024x1024",
  quality: "high",
  n: 1,
  outputFormat: "png",
  background: "",
  responseFormat: "url"
})
const accounts = ref([])
const references = ref([])
const mask = ref(null)
const tasks = ref([])
const total = ref(0)
const page = ref(1)
const pageSize = 12
const status = ref("")
const autoRefresh = ref(true)
const selecting = ref(false)
const selected = ref([])
const detail = ref(null)
const detailLoading = ref(false)
const loadingAccounts = ref(true)
const refreshing = ref(false)
const submitting = ref(false)
const uploading = ref(false)
const exporting = ref(false)
const deleting = ref(false)
const submitError = ref("")
const listError = ref("")
let timer
let requestVersion = 0
let disposed = false
const availableAccounts = computed(() =>
  accounts.value.filter(
    (account) => !account.disabled && !account.requiresReauth
  )
)
const canSubmit = computed(
  () =>
    !submitting.value &&
    !uploading.value &&
    availableAccounts.value.some((account) => account.id === form.accountId) &&
    form.prompt.trim() &&
    form.model.trim() &&
    Number.isInteger(form.n) &&
    form.n >= 1 &&
    form.n <= 10 &&
    (form.mode !== "edit" || references.value.length) &&
    !(form.background === "transparent" && form.outputFormat === "jpeg")
)
const selectableTasks = computed(() =>
  tasks.value.filter((task) => task.status !== "processing")
)
const allSelected = computed(
  () =>
    selectableTasks.value.length > 0 &&
    selectableTasks.value.every((task) => selected.value.includes(task.id))
)

async function loadAccounts() {
  loadingAccounts.value = true
  try {
    accounts.value = await toolboxApi.imageAccounts()
    if (
      !availableAccounts.value.some((account) => account.id === form.accountId)
    ) {
      form.accountId =
        (
          availableAccounts.value.find((account) => account.active) ||
          availableAccounts.value[0]
        )?.id || ""
    }
  } finally {
    loadingAccounts.value = false
  }
}

async function loadTasks() {
  // 状态筛选和轮询可能重叠，只应用最后一次请求，避免旧列表覆盖新筛选。
  const version = ++requestVersion
  refreshing.value = true
  try {
    const result = await toolboxApi.listImageTasks({
      page: page.value,
      status: status.value
    })
    if (version !== requestVersion || disposed) return
    tasks.value = result.items
    total.value = result.total
    selected.value = selected.value.filter((id) =>
      result.items.some(
        (task) => task.id === id && task.status !== "processing"
      )
    )
    listError.value = ""
    const lastPage = Math.max(1, Math.ceil(total.value / pageSize))
    if (page.value > lastPage) page.value = lastPage
  } catch (error) {
    if (version === requestVersion) listError.value = String(error)
  } finally {
    if (version === requestVersion) refreshing.value = false
  }
}

async function refreshAll() {
  const results = await Promise.allSettled([loadAccounts(), loadTasks()])
  if (results[0].status === "rejected")
    listError.value = `账号读取失败：${results[0].reason}`
}

async function addImages(event, isMask) {
  const files = Array.from(event.target.files || [])
  event.target.value = ""
  uploading.value = true
  submitError.value = ""
  try {
    if (!isMask && references.value.length + files.length > 10)
      throw new Error("最多添加 10 张参考图")
    const added = []
    for (const file of files) {
      if (
        !(
          isMask ? ["image/png"] : ["image/png", "image/jpeg", "image/webp"]
        ).includes(file.type)
      )
        throw new Error("请选择支持的图片格式")
      if (file.size > 20 * 1024 * 1024)
        throw new Error("单张图片不能超过 20 MB")
      const url = await new Promise((resolve, reject) => {
        const reader = new FileReader()
        reader.onload = () => resolve(reader.result)
        reader.onerror = () => reject(new Error("读取图片失败"))
        reader.readAsDataURL(file)
      })
      added.push({ id: crypto.randomUUID(), name: file.name, url })
    }
    const nextReferences = isMask
      ? references.value
      : [...references.value, ...added]
    const nextMask = isMask ? added[0] || mask.value : mask.value
    if (
      nextReferences.reduce((size, item) => size + item.url.length, 0) +
        (nextMask?.url.length || 0) >
      32 * 1024 * 1024
    )
      throw new Error("图片编码后总大小超过 32 MB，请减少图片或压缩后上传")
    references.value = nextReferences
    mask.value = nextMask
  } catch (error) {
    submitError.value = error.message || String(error)
  } finally {
    uploading.value = false
  }
}

async function submitTask() {
  if (!canSubmit.value) return
  submitting.value = true
  submitError.value = ""
  try {
    // 草稿在提交后仍然保留；上游失败不会清空用户的提示词和参考图。
    await toolboxApi.submitImageTask({
      ...form,
      images:
        form.mode === "edit" ? references.value.map((item) => item.url) : [],
      mask: form.mode === "edit" ? mask.value?.url || "" : ""
    })
    status.value = ""
    page.value = 1
    await loadTasks()
    createMessage.success("任务已提交，正在后台生成")
  } catch (error) {
    submitError.value = String(error)
  } finally {
    submitting.value = false
  }
}

function toggleSelection() {
  selecting.value = !selecting.value
  selected.value = []
}

function toggleAll() {
  selected.value = allSelected.value
    ? []
    : selectableTasks.value.map((task) => task.id)
}

async function reuseTask(task) {
  if (uploading.value) return
  uploading.value = true
  try {
    // 编辑历史同时恢复参考图和蒙版，用户确认后再手动提交。
    const inputs = await toolboxApi.imageTaskInputs({ id: task.id })
    Object.assign(form, task.request)
    references.value = inputs.images.map((url, index) => ({ id: crypto.randomUUID(), name: `参考图 ${index + 1}`, url }))
    mask.value = inputs.mask ? { name: '历史蒙版.png', url: inputs.mask } : null
    submitError.value = ''
    createMessage.success('已恢复历史任务参数')
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    uploading.value = false
  }
}

async function showDetail(task) {
  detailLoading.value = true
  try {
    detail.value = await toolboxApi.imageTaskDetail({ id: task.id })
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    detailLoading.value = false
  }
}

function imageUrl(item) {
  return item.url || `data:image/${item.output_format};base64,${item.b64_json}`
}

function editResult(item) {
  Object.assign(form, detail.value.request)
  mask.value = null
  form.mode = "edit"
  references.value = [
    { id: crypto.randomUUID(), name: "生成结果", url: imageUrl(item) }
  ]
  submitError.value = ""
  detail.value = null
}

async function exportTasks(ids) {
  exporting.value = true
  try {
    const targetPath = await systemApi.saveFile({
      title: "导出图片",
      defaultPath: "image-workbench.zip",
      filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }]
    })
    if (!targetPath) return
    const result = await toolboxApi.exportImageTasks({
      ids: [...ids],
      targetPath
    })
    createMessage.success(`已导出 ${result.imageCount} 张图片`)
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    exporting.value = false
  }
}

async function deleteTasks() {
  const ids = [...selected.value]
  try {
    await ElMessageBox.confirm(
      `删除所选 ${ids.length} 个任务及其本地图片？此操作无法撤销。`,
      "删除图片任务",
      { confirmButtonText: "删除", cancelButtonText: "取消", type: "warning" }
    )
  } catch {
    return
  }
  deleting.value = true
  try {
    await toolboxApi.deleteImageTasks({ ids })
    selected.value = []
    await loadTasks()
    createMessage.success("任务已删除")
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    deleting.value = false
  }
}

watch(status, () => {
  selected.value = []
  if (page.value !== 1) page.value = 1
  else loadTasks()
})
watch(page, () => {
  selected.value = []
  loadTasks()
})
const unsubscribe = subscribe("images:changed", () => {
  if (autoRefresh.value) loadTasks()
})
const unsubscribeError = subscribe("images:storage-error", (event) => {
  listError.value = `任务结果保存失败：${event.message}`
})
onMounted(() => {
  refreshAll()
  timer = window.setInterval(() => {
    if (autoRefresh.value && !refreshing.value && !document.hidden) loadTasks()
  }, 3000)
})
onBeforeUnmount(() => {
  disposed = true
  requestVersion++
  window.clearInterval(timer)
  unsubscribe()
  unsubscribeError()
})
</script>

<style scoped lang="less">
.image-workbench {
  display: flex;
  flex: 1;
  align-items: flex-start;
  gap: 18px;
  min-height: 0;
  overflow: auto;
  padding: 2px 2px 16px;
  color: var(--color-text);
  font-size: 12px;

  .create-panel {
    flex: 0 0 326px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 9px;
    background: var(--color-panel);

    .panel-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      font-size: 14px;
      .official-badge {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        color: var(--color-text-muted);
        font-size: 10px;
      }
    }
    .mode-tabs {
      display: flex;
      padding: 4px;
      gap: 4px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      .mode-tab {
        display: flex;
        flex: 1;
        align-items: center;
        justify-content: center;
        gap: 8px;
        height: 34px;
        border: 0;
        border-radius: 6px;
        color: var(--color-text-muted);
        background: transparent;
        cursor: pointer;
        &.active {
          background: var(--color-primary-solid);
          color: #fff;
          box-shadow: 0 3px 10px #00000012;
        }
      }
    }
    .parameter-fields {
      display: flex;
      flex-wrap: wrap;
      gap: 14px 12px;
    }
    .form-field {
      display: flex;
      flex-direction: column;
      gap: 7px;
      min-width: 0;
      width: 100%;
      .field-label {
        display: flex;
        align-items: baseline;
        gap: 5px;
        color: var(--color-text-muted);
      }
      .field-input {
        width: 100%;
        min-width: 0;
        height: 39px;
        padding: 8px 9px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        outline: none;
        color: var(--color-text);
        background: var(--color-panel-soft);
        font-size: 12px;
        &:focus {
          border-color: var(--color-primary);
        }
      }
      .prompt-input {
        height: 112px;
        min-height: 85px;
        resize: vertical;
        line-height: 1.7;
      }
    }
    .parameter-fields {
      .form-field {
        width: calc(50% - 6px);
      }
    }
    .field-key {
      color: var(--color-text-soft);
      font-size: 10px;
    }
    .account-hint,
    .field-error {
      margin: 0;
      color: var(--color-warning);
      line-height: 1.7;
    }
    .field-error {
      color: var(--color-danger);
      overflow-wrap: anywhere;
    }
    .reference-section {
      display: flex;
      flex-direction: column;
      gap: 9px;
      .reference-head {
        display: flex;
        justify-content: space-between;
        color: var(--color-text-muted);
      }
      .reference-list {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        .reference-item {
          position: relative;
          width: 84px;
          .reference-image {
            width: 84px;
            height: 84px;
            object-fit: cover;
            border: 1px solid var(--color-line);
            border-radius: 6px;
          }
          .remove-reference {
            position: absolute;
            top: 4px;
            right: 4px;
            display: flex;
            padding: 3px;
            border: 0;
            border-radius: 50%;
            color: #fff;
            background: #0009;
            cursor: pointer;
          }
          .reference-name {
            display: block;
            overflow: hidden;
            white-space: nowrap;
            text-overflow: ellipsis;
            color: var(--color-text-muted);
            font-size: 10px;
          }
        }
        .upload-control {
          position: relative;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 7px;
          width: 84px;
          height: 84px;
          border: 1px dashed var(--color-line-strong);
          border-radius: 6px;
          color: var(--color-primary);
          cursor: pointer;
        }
      }
      .upload-input {
        position: absolute;
        inset: 0;
        width: 100%;
        height: 100%;
        opacity: 0;
        cursor: pointer;
      }
      .reference-hint {
        margin: 0;
        color: var(--color-text-soft);
        font-size: 10px;
        overflow-wrap: anywhere;
      }
      .mask-row {
        display: flex;
        justify-content: space-between;
        .mask-upload {
          position: relative;
          display: inline-flex;
          align-items: center;
          gap: 5px;
          color: var(--color-text-muted);
          cursor: pointer;
        }
      }
    }
    .submit-button {
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      height: 40px;
      border: 1px solid var(--color-primary-solid);
      border-radius: 6px;
      color: #fff;
      background: var(--color-primary-solid);
      cursor: pointer;
      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
    }
    .submit-hint {
      margin: -6px 0 0;
      color: var(--color-text-soft);
      font-size: 10px;
      line-height: 1.6;
    }
  }

  .tasks-panel {
    flex: 1;
    min-width: 315px;
    min-height: 260px;
    border: 1px solid var(--color-line);
    border-radius: 9px;
    background: var(--color-panel);
    .tasks-head {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: space-between;
      gap: 14px;
      padding: 18px;
      .tasks-title {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 14px;
        .task-count {
          color: var(--color-text-soft);
          font-size: 12px;
        }
      }
      .tasks-controls {
        display: flex;
        align-items: center;
        gap: 8px;
        .status-select {
          width: 96px;
          height: 31px;
          padding: 4px 6px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          color: var(--color-text);
          background: var(--color-panel-soft);
          font-size: 11px;
        }
        .icon-button {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 31px;
          height: 31px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          cursor: pointer;
        }
      }
    }
    .auto-refresh {
      display: flex;
      align-items: center;
      gap: 3px;
      white-space: nowrap;
      font-size: 10px;
      color: var(--color-text-muted);
    }
    .selection-bar {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 10px 18px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel-soft);
      .selection-count {
        flex: 1;
        color: var(--color-text-muted);
        font-size: 11px;
      }
    }
    .list-error {
      margin: 0;
      padding: 0 18px 12px;
      color: var(--color-danger);
      overflow-wrap: anywhere;
    }
    .empty-state {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 9px;
      min-height: 270px;
      padding: 25px;
      .empty-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 62px;
        height: 62px;
        margin-bottom: 9px;
        border: 1px dashed var(--color-line-strong);
        border-radius: 16px;
        color: var(--color-text-soft);
        background: var(--color-panel-soft);
      }
      .empty-title {
        color: var(--color-text-muted);
        font-size: 13px;
      }
      .empty-description {
        color: var(--color-text-soft);
        font-size: 11px;
      }
    }
    .task-list {
      display: flex;
      flex-direction: column;
      padding: 0 18px 18px;
      gap: 12px;
      .task-card {
        display: flex;
        align-items: flex-start;
        gap: 12px;
        padding: 12px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        &.task-selected {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
        }
        .task-check {
          padding-top: 4px;
        }
        .task-preview {
          display: flex;
          flex: 0 0 90px;
          height: 90px;
          align-items: center;
          justify-content: center;
          padding: 0;
          overflow: hidden;
          border: 0;
          border-radius: 5px;
          background: var(--color-panel-soft);
          color: var(--color-text-soft);
          cursor: pointer;
          .task-thumbnail {
            width: 100%;
            height: 100%;
            object-fit: cover;
          }
        }
        .task-content {
          flex: 1;
          min-width: 0;
          .task-meta {
            display: flex;
            align-items: center;
            flex-wrap: wrap;
            gap: 6px 12px;
            .task-status {
              color: var(--color-text-muted);
              font-size: 10px;
              &.completed {
                color: var(--color-success);
              }
              &.failed {
                color: var(--color-danger);
              }
              &.processing {
                color: var(--color-primary);
              }
              &.partial,
              &.interrupted {
                color: var(--color-warning);
              }
            }
            .task-time {
              color: var(--color-text-soft);
              font-size: 10px;
            }
          }
          .task-prompt {
            margin: 7px 0 5px;
            display: -webkit-box;
            -webkit-line-clamp: 2;
            -webkit-box-orient: vertical;
            overflow: hidden;
            overflow-wrap: anywhere;
            line-height: 1.6;
          }
          .task-spec {
            margin: 0;
            color: var(--color-text-soft);
            font-size: 10px;
            overflow-wrap: anywhere;
          }
          .task-error {
            margin: 6px 0;
            color: var(--color-danger);
            font-size: 10px;
            overflow-wrap: anywhere;
          }
          .task-actions {
            display: flex;
            flex-wrap: wrap;
            gap: 12px;
            margin-top: 8px;
          }
        }
      }
    }
    .pagination {
      display: flex;
      justify-content: flex-end;
      align-items: center;
      gap: 12px;
      padding: 0 18px 18px;
      color: var(--color-text-muted);
    }
  }
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
    font-size: 11px;
    &.danger-button {
      color: var(--color-danger);
    }
    &:disabled {
      opacity: 0.45;
      cursor: not-allowed;
    }
  }
  .text-button {
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--color-primary);
    font-size: 11px;
    cursor: pointer;
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }
  .spinning {
    animation: image-workbench-spin 1.5s linear infinite;
  }
  .detail-content {
    overflow: auto;
    .detail-prompt {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      line-height: 1.8;
    }
    .detail-error {
      color: var(--color-danger);
    }
    .detail-images {
      display: flex;
      flex-wrap: wrap;
      gap: 16px;
      .detail-image-card {
        margin: 0;
        width: calc(50% - 8px);
        .detail-image {
          width: 100%;
          height: 260px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
        }
        .detail-caption {
          display: flex;
          justify-content: space-between;
          padding: 8px 0;
          color: var(--color-text-muted);
        }
      }
    }
    .detail-meta {
      display: flex;
      flex-wrap: wrap;
      gap: 12px 22px;
      padding: 16px 0;
      color: var(--color-text-muted);
    }
    .technical-detail {
      color: var(--color-text-muted);
      .usage-content {
        white-space: pre-wrap;
        overflow-wrap: anywhere;
        padding: 12px;
        border-radius: 6px;
        background: var(--color-panel-soft);
        font-size: 11px;
      }
    }
    .detail-hint {
      color: var(--color-text-soft);
    }
  }
}
@keyframes image-workbench-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

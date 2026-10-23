<template>
  <section class="screenshot-tool">
    <div class="screenshot-actions">
      <button
        class="screenshot-primary"
        type="button"
        :disabled="busy || !ready"
        @click="run('capture')"
      >
        <ScanLine :size="18" />区域截图
        <kbd v-if="settings.hotkeys" class="action-shortcut">{{
          captureShortcut
        }}</kbd>
      </button>
      <button
        class="screenshot-button"
        type="button"
        :disabled="busy || !ready"
        title="先框选区域，再自动滚动拼接成长图"
        @click="startLongCapture"
      >
        <ScanLine :size="17" />长截图
      </button>
      <button
        class="screenshot-button"
        type="button"
        :disabled="busy || !ready"
        @click="run('clipboard')"
      >
        <ClipboardPaste :size="17" />剪贴板贴图
      </button>
      <button
        class="screenshot-button"
        type="button"
        :disabled="busy || !ready"
        @click="importImage"
      >
        <ImagePlus :size="17" />打开图片
      </button>
      <span class="actions-spacer" />
      <button
        class="screenshot-button"
        type="button"
        :disabled="busy || !ready"
        @click="run('toggle')"
      >
        <Eye :size="17" />显示 / 隐藏
      </button>
      <button
        class="screenshot-icon"
        type="button"
        title="刷新历史"
        aria-label="刷新历史"
        :disabled="busy"
        @click="load"
      >
        <RefreshCw :size="17" />
      </button>
      <button
        class="screenshot-icon"
        type="button"
        title="截图设置"
        aria-label="截图设置"
        :disabled="!ready || busy"
        @click="openSettings"
      >
        <Settings2 :size="17" />
      </button>
    </div>
    <div v-if="error" class="screenshot-error" role="alert">{{ error }}</div>
    <div
      v-for="message in warnings"
      :key="message"
      class="screenshot-warning"
      role="status"
    >
      {{ message }}
    </div>
    <BaseModal
      v-if="showSettings"
      class="screenshot-settings-modal"
      title="截图贴图设置"
      description="设置快捷键、历史数量和贴图恢复方式。"
      @close="!busy && (showSettings = false)"
    >
      <form class="screenshot-settings" @submit.prevent="saveSettings">
        <label class="setting-row"
          ><input
            v-model="settingsDraft.hotkeys"
            type="checkbox"
            :disabled="busy"
          />启用全局快捷键</label
        >
        <label
          v-for="shortcut in shortcutFields"
          :key="shortcut.id"
          class="setting-row"
          >{{ shortcut.label }}
          <input
            class="setting-input"
            :value="settingsDraft.shortcuts[shortcut.id]"
            :aria-label="shortcut.label"
            placeholder="点击后按下快捷键"
            readonly
            :disabled="busy || !settingsDraft.hotkeys"
            @keydown="recordShortcut($event, shortcut.id)"
            @keyup="
              $event.key === 'PrintScreen' &&
              recordShortcut($event, shortcut.id)
            "
          />
        </label>
        <label class="setting-row"
          >历史记录上限
          <input
            v-model.number="settingsDraft.historyLimit"
            class="setting-input"
            aria-label="历史记录上限"
            type="number"
            min="1"
            max="500"
            step="1"
            required
            :disabled="busy"
          /><span>张（1–500）</span>
        </label>
        <span class="setting-hint"
          >包含截图和贴图；超过上限自动删除最旧记录及文件，并关闭对应贴图。降低上限会立即清理，无法撤销。</span
        >
        <label class="setting-row"
          ><input
            v-model="settingsDraft.restorePins"
            type="checkbox"
            :disabled="busy"
          />启动时恢复打开的贴图</label
        >
        <button
          class="screenshot-button"
          type="button"
          :disabled="busy"
          @click="run('reset-input')"
        >
          取消全部鼠标穿透
        </button>
        <span class="setting-path">数据目录：{{ dataPath }}</span>
        <div v-if="error" class="settings-error" role="alert">{{ error }}</div>
        <div class="settings-actions">
          <button
            class="screenshot-button"
            type="button"
            :disabled="busy"
            @click="settingsDraft = defaultSettings()"
          >
            恢复默认
          </button>
          <span class="actions-spacer" />
          <button
            class="screenshot-button"
            type="button"
            :disabled="busy"
            @click="showSettings = false"
          >
            取消
          </button>
          <button class="screenshot-primary" type="submit" :disabled="busy">
            保存设置
          </button>
        </div>
      </form>
    </BaseModal>
    <div class="screenshot-history-head">
      <span class="history-title">截图与贴图</span>
      <span class="history-count"
        >{{ entries.length }} 张图片 · {{ openCount }} 张打开</span
      >
      <span class="actions-spacer" />
      <span class="history-hint"
        >最多 {{ settings.historyLimit }} 张，超出自动删除最旧图片及贴图</span
      >
    </div>
    <div v-if="!entries.length" class="screenshot-empty">
      <div class="empty-icon"><ScanLine :size="36" :stroke-width="1.3" /></div>
      <span class="empty-title">截取灵感，贴在眼前</span>
      <span class="empty-description"
        >选择桌面区域，添加标注，再复制、保存或贴到桌面。</span
      >
      <span class="empty-shortcut"
        >{{
          settings.hotkeys
            ? `${captureShortcut} 开始截图`
            : "点击“区域截图”开始"
        }}
        · Esc 取消</span
      >
    </div>
    <div v-else class="screenshot-history">
      <article v-for="entry in entries" :key="entry.id" class="screenshot-card">
        <button
          class="card-preview"
          type="button"
          title="贴到桌面"
          :disabled="busy"
          @click="run('pin', { id: entry.id })"
        >
          <img
            v-if="entry.thumbnail"
            :src="entry.thumbnail"
            class="card-image"
            alt="截图缩略图"
          /><span v-else class="missing-image">图片缺失</span
          ><span v-if="entry.pin" class="card-state">{{
            pinLabel(entry.pin)
          }}</span>
        </button>
        <div class="card-info">
          <span class="card-size">{{ entry.width }} × {{ entry.height }}</span
          ><span class="card-time">{{ formatTime(entry.createdAt) }}</span>
        </div>
        <div class="card-actions">
          <button
            class="card-action"
            type="button"
            title="贴到桌面 / 恢复关闭的贴图"
            aria-label="贴到桌面"
            :disabled="busy"
            @click="run('pin', { id: entry.id })"
          >
            <Pin :size="15" />贴图
          </button>
          <button
            class="card-action"
            type="button"
            title="复制图片"
            aria-label="复制图片"
            :disabled="busy"
            @click="run('copy', { id: entry.id })"
          >
            <Copy :size="15" />
          </button>
          <button
            class="card-action"
            type="button"
            title="保存 PNG"
            aria-label="保存 PNG"
            :disabled="busy"
            @click="run('export', { id: entry.id })"
          >
            <Download :size="15" />
          </button>
          <button
            class="card-action card-delete"
            type="button"
            title="删除截图及其贴图"
            aria-label="删除截图及其贴图"
            :disabled="busy"
            @click="deleteEntry(entry)"
          >
            <Trash2 :size="15" />
          </button>
        </div>
      </article>
    </div>
    <div class="screenshot-footer">
      <MousePointer2 :size="14" /><span
        >贴图：拖动移动 · 滚轮缩放 · 右键编辑、透明度和关闭</span
      >
    </div>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  ClipboardPaste,
  Copy,
  Download,
  Eye,
  ImagePlus,
  MousePointer2,
  Pin,
  RefreshCw,
  ScanLine,
  Settings2,
  Trash2
} from "lucide-vue-next"
import { screenshotApi } from "@/api/modules/screenshot"
import { systemApi } from "@/api/modules/system"
import { subscribeReady } from "@/api/request"
import { createMessage } from "@/utils/message"
import BaseModal from "@/components/BaseModal.vue"

const entries = ref([])
function defaultSettings() {
  // 默认组合与后端一致，已保存的用户组合在读取设置时覆盖默认值。
  return {
    hotkeys: true,
    shortcutStyle: "alt",
    restorePins: true,
    historyLimit: 50,
    shortcuts: {
      captureAutoCopy: "Ctrl+F1",
      capture: "Shift+F1",
      clipboard: "F3",
      toggle: "Shift+F3",
      switchGroup: "Ctrl+F3",
      resetInput: "Ctrl+Alt+R",
      captureLong: "Ctrl+Alt+F1"
    }
  }
}
const settings = ref(defaultSettings())
const settingsDraft = ref(defaultSettings())
const shortcutFields = [
  { id: "captureAutoCopy", label: "截屏并自动复制" },
  { id: "capture", label: "自定义截屏" },
  { id: "clipboard", label: "贴图" },
  { id: "toggle", label: "隐藏 / 显示所有贴图" },
  { id: "switchGroup", label: "切换到另一贴图组" },
  { id: "resetInput", label: "取消鼠标穿透" },
  { id: "captureLong", label: "长截图" }
]
const warnings = ref([])
const dataPath = ref("")
const ready = ref(false)
const busy = ref(false)
const error = ref("")
const showSettings = ref(false)
watch(showSettings, (paused) => {
  screenshotApi.call("pause-hotkeys", { paused }).catch((cause) => {
    error.value = String(cause)
  })
})
const captureShortcut = computed(() => settings.value.shortcuts.capture)
const openCount = computed(
  () => entries.value.filter((entry) => entry.pin?.lifecycle === "open").length
)
let unlisten
let unlistenError
let disposed = false
let loading = false

function pinLabel(pin) {
  if (pin.lifecycle === "closed") return "已关闭 · 可恢复"
  if (pin.lifecycle === "destroyed") return "贴图已销毁"
  return pin.visible ? "桌面显示中" : "已隐藏"
}

function formatTime(value) {
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  })
}

async function load() {
  if (loading || disposed) return
  loading = true
  try {
    const result = await screenshotApi.call("list")
    if (disposed) return
    entries.value = result.entries
    settings.value = {
      ...defaultSettings(),
      ...result.settings,
      shortcuts: {
        ...defaultSettings().shortcuts,
        ...result.settings.shortcuts
      }
    }
    warnings.value = [...result.errors, ...result.hotkeyErrors]
    dataPath.value = result.dataPath
    ready.value = true
  } catch (cause) {
    error.value = `读取截图数据失败：${String(cause)}`
  } finally {
    loading = false
  }
}

async function run(action, payload) {
  if (busy.value) return
  busy.value = true
  error.value = ""
  try {
    const result = await screenshotApi.call(action, payload)
    if (action === "copy") createMessage.success("图片已复制")
    if (action === "export" && result.saved) createMessage.success("图片已保存")
    if (action === "reset-input")
      createMessage.success("已取消全部贴图鼠标穿透")
    await load()
  } catch (cause) {
    error.value = String(cause)
  } finally {
    busy.value = false
  }
}

async function startLongCapture() {
  // 通过后端传递模式，截图窗口和主窗口各自独立，不能依赖 sessionStorage。
  await run("capture-long")
}

async function importImage() {
  const path = await systemApi
    .selectFile({
      title: "打开图片并贴到桌面",
      filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp"] }]
    })
    .catch((cause) => {
      error.value = String(cause)
      return ""
    })
  if (path) await run("import", { path })
}

function openSettings() {
  settingsDraft.value = JSON.parse(JSON.stringify(settings.value))
  error.value = ""
  showSettings.value = true
}

function recordShortcut(event, id) {
  if (event.key === "Tab") return
  event.preventDefault()
  if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return
  const key =
    event.code === "Space"
      ? "Space"
      : event.key.length === 1
        ? event.key.toUpperCase()
        : event.key
  if (!/^[A-Z0-9]$|^F([1-9]|1[0-9]|2[0-4])$|^(PrintScreen|Space)$/.test(key)) {
    error.value = "请使用字母、数字、F1–F24、PrintScreen 或空格"
    return
  }
  if (
    !event.ctrlKey &&
    !event.altKey &&
    !event.shiftKey &&
    !event.metaKey &&
    !/^F\d+$|^PrintScreen$/.test(key)
  ) {
    error.value = "请同时按下 Ctrl、Alt、Shift 或 Win 修饰键"
    return
  }
  settingsDraft.value.shortcuts[id] = [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Win",
    key
  ]
    .filter(Boolean)
    .join("+")
  error.value = ""
}

async function saveSettings() {
  if (busy.value) return
  const draft = settingsDraft.value
  if (
    !Number.isInteger(draft.historyLimit) ||
    draft.historyLimit < 1 ||
    draft.historyLimit > 500
  ) {
    error.value = "历史记录上限必须为 1 到 500 的整数"
    return
  }
  if (new Set(Object.values(draft.shortcuts)).size !== shortcutFields.length) {
    error.value = "截图快捷键不能重复"
    return
  }
  if (
    entries.value.length > draft.historyLimit &&
    !window.confirm(
      `保存后将删除最旧的 ${entries.value.length - draft.historyLimit} 张图片及其贴图，无法撤销。继续？`
    )
  )
    return
  busy.value = true
  error.value = ""
  try {
    await screenshotApi.call("settings", JSON.parse(JSON.stringify(draft)))
    await load()
    showSettings.value = false
    createMessage.success("截图设置已保存")
  } catch (cause) {
    error.value = String(cause)
  } finally {
    busy.value = false
  }
}

async function deleteEntry(entry) {
  if (
    !window.confirm(
      "删除这张截图及其贴图？此操作会删除本地图片和标注，无法撤销。"
    )
  )
    return
  await run("delete", { id: entry.id })
}

onMounted(async () => {
  try {
    // 先订阅再读取，避免初始化期间新截图被遗漏。
    unlisten = await subscribeReady("screenshot:changed", load)
    unlistenError = await subscribeReady("screenshot:error", (value) => {
      error.value = value
    })
    if (disposed) {
      unlisten()
      unlistenError()
      return
    }
    await load()
  } catch (cause) {
    error.value = String(cause)
  }
})
onBeforeUnmount(() => {
  if (showSettings.value)
    screenshotApi.call("pause-hotkeys", { paused: false }).catch(() => {})
  disposed = true
  unlisten?.()
  unlistenError?.()
})
</script>

<style scoped lang="less">
.screenshot-tool {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  gap: 14px;
  overflow: hidden;
  color: var(--color-text);
  .screenshot-button,
  .screenshot-primary,
  .screenshot-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    padding: 8px 11px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: inherit;
    cursor: pointer;
    font-size: var(--font-size-sm);
    white-space: nowrap;
    &:hover {
      border-color: var(--color-primary);
    }
    &:disabled {
      opacity: 0.5;
      cursor: default;
    }
  }
  .actions-spacer {
    flex: 1;
  }
  .screenshot-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    .screenshot-primary {
      background: var(--color-primary-solid);
      color: #fff;
      border-color: transparent;
      .action-shortcut {
        margin-left: 4px;
        opacity: 0.75;
        font: inherit;
        font-size: var(--font-size-xs);
      }
    }
    .screenshot-icon {
      padding: 8px;
    }
  }
  .screenshot-error,
  .screenshot-warning {
    padding: 9px 12px;
    border-radius: 6px;
    font-size: var(--font-size-sm);
  }
  .screenshot-error {
    color: var(--color-danger);
    background: var(--color-danger-soft);
  }
  .screenshot-warning {
    color: var(--color-warning);
    background: var(--color-warning-soft);
  }
  .screenshot-settings {
    display: flex;
    align-items: stretch;
    flex-direction: column;
    overflow-y: auto;
    gap: 12px 22px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
    .setting-row {
      display: flex;
      align-items: center;
      gap: 7px;
      font-size: var(--font-size-sm);
      .setting-input {
        margin-left: auto;
        width: 230px;
        padding: 5px;
        border: 1px solid var(--color-line);
        border-radius: 5px;
        background: var(--color-panel);
        color: inherit;
        font-size: inherit;
      }
    }
    .setting-hint,
    .setting-path {
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
    .setting-path {
      overflow-wrap: anywhere;
    }
    .settings-error {
      color: var(--color-danger);
      font-size: var(--font-size-sm);
    }
    .settings-actions {
      display: flex;
      gap: 8px;
      align-items: center;
    }
  }
  .screenshot-settings-modal {
    :deep(.base-modal__panel) {
      width: 600px;
      max-width: 100%;
    }
  }
  .screenshot-history-head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-top: 4px;
    .history-title {
      font-size: var(--font-size-base);
    }
    .history-count,
    .history-hint {
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
  }
  .screenshot-empty {
    display: flex;
    flex: 1;
    min-height: 180px;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 13px;
    border: 1px dashed var(--color-line-strong);
    border-radius: 12px;
    background: var(--color-panel-soft);
    .empty-icon {
      display: flex;
      padding: 17px;
      border: 1px solid var(--color-line);
      border-radius: 18px;
      background: var(--color-panel);
      color: var(--color-primary);
    }
    .empty-title {
      margin-top: 4px;
      font-size: var(--font-size-lg);
    }
    .empty-description {
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
    }
    .empty-shortcut {
      color: var(--color-text-soft);
      font-size: var(--font-size-xs);
    }
  }
  .screenshot-history {
    display: flex;
    flex-wrap: wrap;
    align-content: flex-start;
    gap: 14px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
    padding: 2px 3px;
    .screenshot-card {
      display: flex;
      flex-direction: column;
      width: 240px;
      height: 225px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      overflow: hidden;
      background: var(--color-panel);
      .card-preview {
        position: relative;
        display: flex;
        align-items: center;
        justify-content: center;
        height: 148px;
        padding: 10px;
        border: 0;
        background: var(--color-panel-soft);
        cursor: pointer;
        .card-image {
          width: 100%;
          height: 100%;
          object-fit: contain;
        }
        .missing-image {
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }
        .card-state {
          position: absolute;
          right: 6px;
          bottom: 6px;
          padding: 2px 7px;
          border-radius: 4px;
          background: #17212bda;
          color: #d7efff;
          font-size: var(--font-size-xs);
        }
      }
      .card-info {
        display: flex;
        justify-content: space-between;
        gap: 8px;
        padding: 8px 10px;
        .card-size,
        .card-time {
          font-size: var(--font-size-xs);
          color: var(--color-text-muted);
        }
      }
      .card-actions {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 0 6px 6px;
        .card-action {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          height: 29px;
          padding: 4px 8px;
          border: 0;
          border-radius: 5px;
          background: transparent;
          color: var(--color-text-muted);
          cursor: pointer;
          font-size: var(--font-size-xs);
          &:hover {
            background: var(--color-primary-soft);
            color: var(--color-primary);
          }
          &:disabled {
            opacity: 0.5;
          }
          &.card-delete {
            margin-left: auto;
            &:hover {
              background: var(--color-danger-soft);
              color: var(--color-danger);
            }
          }
        }
      }
    }
  }
  .screenshot-footer {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: none;
    padding-top: 4px;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);
  }
}
</style>

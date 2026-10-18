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
        <kbd class="action-shortcut">{{ shortcutPrefix }} A</kbd>
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
        @click="showSettings = !showSettings"
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
    <div v-if="showSettings && ready" class="screenshot-settings">
      <label class="setting-row"
        ><input
          v-model="settings.hotkeys"
          type="checkbox"
          :disabled="busy"
          @change="saveSettings"
        />启用全局快捷键</label
      >
      <label class="setting-row"
        >快捷键组合
        <select
          v-model="settings.shortcutStyle"
          class="setting-select"
          :disabled="busy"
          @change="saveSettings"
        >
          <option value="alt">Ctrl + Alt</option>
          <option value="shift">Ctrl + Shift</option>
        </select></label
      >
      <label class="setting-row"
        ><input
          v-model="settings.restorePins"
          type="checkbox"
          :disabled="busy"
          @change="saveSettings"
        />启动时恢复打开的贴图</label
      >
      <span class="setting-hint"
        >A 截图 · P 剪贴板贴图 · H 显隐贴图 · R 取消鼠标穿透</span
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
    </div>
    <div class="screenshot-history-head">
      <span class="history-title">截图与贴图</span>
      <span class="history-count"
        >{{ entries.length }} 张图片 · {{ openCount }} 张打开</span
      >
      <span class="actions-spacer" />
      <span class="history-hint"
        >普通历史上限 50 张 / 512 MiB / 30 天，贴图单独保留</span
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
            ? `${shortcutPrefix} A 开始截图`
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
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
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

const entries = ref([])
const settings = ref({ hotkeys: true, shortcutStyle: "alt", restorePins: true })
const warnings = ref([])
const dataPath = ref("")
const ready = ref(false)
const busy = ref(false)
const error = ref("")
const showSettings = ref(false)
const shortcutPrefix = computed(() =>
  settings.value.shortcutStyle === "shift" ? "Ctrl + Shift +" : "Ctrl + Alt +"
)
const openCount = computed(
  () => entries.value.filter(entry => entry.pin?.lifecycle === "open").length
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
    settings.value = result.settings
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

async function importImage() {
  const path = await systemApi
    .selectFile({
      title: "打开图片并贴到桌面",
      filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp"] }]
    })
    .catch(cause => {
      error.value = String(cause)
      return ""
    })
  if (path) await run("import", { path })
}

async function saveSettings() {
  await run("settings", { ...settings.value })
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
    unlistenError = await subscribeReady("screenshot:error", value => {
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
    align-items: center;
    flex-wrap: wrap;
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
      .setting-select {
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
      flex-basis: 100%;
      overflow-wrap: anywhere;
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

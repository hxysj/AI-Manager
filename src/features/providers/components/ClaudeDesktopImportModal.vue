<template>
  <BaseModal
    class="desktop-import-modal"
    title="导入到 Claude Desktop"
    description="从 Claude Code 供应商中选择需要导入的配置。"
    role="dialog"
    aria-modal="true"
    aria-label="导入到 Claude Desktop"
    @close="close"
    @keydown.esc.stop="close"
  >
    <form
      class="import-form"
      :aria-busy="loading || pending"
      @submit.prevent="submit"
    >
      <div class="import-notice">
        <Info :size="17" /><span
          >仅复制勾选项的配置与全部 API
          Key，不影响原供应商，不自动启用，也不覆盖已导入项。</span
        >
      </div>
      <div class="import-toolbar">
        <label class="import-search">
          <Search :size="16" />
          <input
            v-model="query"
            class="import-search-input"
            type="search"
            placeholder="搜索供应商名称或地址"
            aria-label="搜索供应商"
            :disabled="pending"
          />
        </label>
        <span class="import-count">{{ availableCount }} 项可导入</span>
      </div>
      <div class="import-select-bar">
        <label class="import-select-all">
          <input
            type="checkbox"
            :checked="allSelected"
            :indeterminate="partlySelected"
            :disabled="loading || pending || !selectableItems.length"
            @change="selectVisible($event.target.checked)"
          />
          {{ query.trim() ? "选择搜索结果" : "全选可导入项" }}
        </label>
        <button
          v-if="selectedIds.length"
          class="import-clear"
          type="button"
          :disabled="pending"
          @click="selectedIds = []"
        >
          清空选择
        </button>
      </div>
      <div v-if="error" class="import-error" role="alert">
        <span>{{ error }}</span>
        <button
          class="import-retry"
          type="button"
          :disabled="loading || pending"
          @click="$emit('refresh')"
        >
          重新加载
        </button>
      </div>
      <div class="import-list">
        <div v-if="loading" class="import-empty" role="status">
          <LoaderCircle
            class="import-spinner"
            :size="24"
          />正在读取可导入的供应商…
        </div>
        <div v-else-if="!filteredItems.length" class="import-empty">
          <Download :size="26" />
          <span>{{
            query.trim() ? "没有匹配的供应商" : "暂无 Claude Code 供应商"
          }}</span>
          <span v-if="!query.trim()" class="import-empty-hint"
            >可先在 Claude 中添加供应商，再回来导入。</span
          >
        </div>
        <label
          v-for="item in loading ? [] : filteredItems"
          :key="item.id"
          class="import-item"
          :class="{
            'import-item-selected': selectedIds.includes(item.id),
            'import-item-unavailable': item.status !== 'available'
          }"
        >
          <input
            v-model="selectedIds"
            class="import-checkbox"
            type="checkbox"
            :value="item.id"
            :disabled="pending || item.status !== 'available'"
            :aria-label="`导入 ${item.name}`"
          />
          <div class="import-item-content">
            <div class="import-item-heading">
              <span class="import-item-name">{{ item.name }}</span>
              <span
                v-if="item.status !== 'available'"
                class="import-status"
                :class="{
                  'import-status-existing': item.status === 'existing'
                }"
                >{{ item.status === "existing" ? "已导入" : "不可导入" }}</span
              >
              <span v-else-if="item.enabled === false" class="import-status"
                >原配置已禁用</span
              >
            </div>
            <span class="import-item-url">{{
              item.baseUrl || "未配置地址"
            }}</span>
            <div v-if="item.status === 'available'" class="import-item-meta">
              <span>{{ item.mode === "direct" ? "直连" : "本地路由" }}</span>
              <span>{{ formatLabels[item.apiFormat] || item.apiFormat }}</span>
              <span>{{ item.apiKeyCount }} 个 Key</span>
            </div>
            <span v-else class="import-item-reason">{{ item.reason }}</span>
          </div>
        </label>
      </div>
      <footer class="import-actions">
        <span class="import-selected-count"
          >已选 {{ selectedIds.length }} 项</span
        >
        <button
          class="import-button"
          type="button"
          :disabled="pending"
          @click="close"
        >
          取消
        </button>
        <button
          class="import-button import-button-primary"
          type="submit"
          :disabled="loading || pending || !selectedIds.length"
        >
          <LoaderCircle v-if="pending" class="import-spinner" :size="16" />
          <Download v-else :size="16" />
          {{ pending ? "正在导入…" : `导入选中项（${selectedIds.length}）` }}
        </button>
      </footer>
    </form>
  </BaseModal>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import { Download, Info, LoaderCircle, Search } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({
  items: { type: Array, default: () => [] },
  loading: { type: Boolean, default: false },
  pending: { type: Boolean, default: false },
  error: { type: String, default: "" }
})
const emit = defineEmits(["close", "submit", "refresh"])
const query = ref("")
const selectedIds = ref([])
const formatLabels = {
  anthropic: "Anthropic",
  openai_chat: "Chat Completions",
  openai_responses: "Responses"
}
const availableCount = computed(
  () => props.items.filter((item) => item.status === "available").length
)
const filteredItems = computed(() => {
  const search = query.value.trim().toLowerCase()
  return props.items.filter(
    (item) =>
      !search ||
      `${item.name} ${item.baseUrl} ${item.id}`.toLowerCase().includes(search)
  )
})
const selectableItems = computed(() =>
  filteredItems.value.filter((item) => item.status === "available")
)
const allSelected = computed(
  () =>
    selectableItems.value.length > 0 &&
    selectableItems.value.every((item) => selectedIds.value.includes(item.id))
)
const partlySelected = computed(
  () =>
    !allSelected.value &&
    selectableItems.value.some((item) => selectedIds.value.includes(item.id))
)

function selectVisible(checked) {
  const visibleIds = new Set(selectableItems.value.map((item) => item.id))
  selectedIds.value = checked
    ? [...new Set([...selectedIds.value, ...visibleIds])]
    : selectedIds.value.filter((id) => !visibleIds.has(id))
}

function submit() {
  if (props.loading || props.pending || !selectedIds.value.length) return
  emit("submit", [...selectedIds.value])
}

function close() {
  if (!props.pending) emit("close")
}

watch(
  () => props.items,
  (items) => {
    const availableIds = new Set(
      items.filter((item) => item.status === "available").map((item) => item.id)
    )
    selectedIds.value = selectedIds.value.filter((id) => availableIds.has(id))
  }
)
</script>

<style scoped lang="less">
.desktop-import-modal {
  :deep(.base-modal__panel) {
    width: 720px;
    border-radius: 14px;
  }
  :deep(.base-modal__header) {
    padding-bottom: 18px;
    h2 {
      font-size: var(--font-size-lg);
    }
    p {
      font-size: var(--font-size-sm);
    }
  }
  .import-form {
    display: flex;
    flex-direction: column;
    min-height: 0;
    gap: 14px;
    font-size: var(--font-size-sm);
    .import-notice {
      display: flex;
      align-items: flex-start;
      gap: 9px;
      padding: 12px 14px;
      border-radius: 8px;
      background: var(--color-primary-soft);
      color: var(--color-primary);
      line-height: 1.65;
      :deep(svg) {
        flex: none;
        margin-top: 3px;
      }
    }
    .import-toolbar {
      display: flex;
      align-items: center;
      gap: 14px;
      .import-search {
        display: flex;
        align-items: center;
        gap: 8px;
        flex: 1;
        padding: 9px 12px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        color: var(--color-text-muted);
        &:focus-within {
          border-color: var(--color-primary);
        }
        .import-search-input {
          width: 100%;
          border: 0;
          outline: 0;
          color: var(--color-text);
          background: transparent;
          font-size: var(--font-size-sm);
        }
      }
      .import-count {
        color: var(--color-text-muted);
        white-space: nowrap;
      }
    }
    .import-select-bar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      .import-select-all {
        display: flex;
        align-items: center;
        gap: 8px;
        cursor: pointer;
      }
      .import-clear {
        border: 0;
        background: transparent;
        color: var(--color-primary);
        cursor: pointer;
        font-size: var(--font-size-sm);
      }
    }
    .import-error {
      display: flex;
      justify-content: space-between;
      gap: 12px;
      color: var(--color-danger);
      .import-retry {
        flex: none;
        border: 0;
        background: transparent;
        color: var(--color-primary);
        cursor: pointer;
        font-size: var(--font-size-sm);
      }
    }
    .import-list {
      display: flex;
      flex-direction: column;
      gap: 9px;
      min-height: 0;
      max-height: 380px;
      overflow-y: auto;
      .import-empty {
        display: flex;
        align-items: center;
        flex-direction: column;
        gap: 12px;
        padding: 38px 16px;
        color: var(--color-text-muted);
        .import-empty-hint {
          font-size: var(--font-size-xs);
        }
      }
      .import-item {
        display: flex;
        align-items: flex-start;
        gap: 12px;
        padding: 14px;
        border: 1px solid var(--color-line);
        border-radius: 9px;
        cursor: pointer;
        &.import-item-selected {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
        }
        &.import-item-unavailable {
          background: var(--color-panel-soft);
          cursor: default;
        }
        .import-checkbox {
          margin-top: 5px;
          accent-color: var(--color-primary-solid);
        }
        .import-item-content {
          display: flex;
          flex-direction: column;
          gap: 5px;
          min-width: 0;
          flex: 1;
          .import-item-heading {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 10px;
            .import-item-name {
              font-size: var(--font-size-base);
              overflow-wrap: anywhere;
            }
            .import-status {
              flex: none;
              padding: 2px 7px;
              border: 1px solid var(--color-line);
              border-radius: 5px;
              color: var(--color-text-muted);
              font-size: var(--font-size-xs);
              &.import-status-existing {
                background: var(--color-success-soft);
                border-color: var(--color-success-line);
                color: var(--color-success);
              }
            }
          }
          .import-item-url {
            color: var(--color-text-muted);
            overflow-wrap: anywhere;
            font-size: var(--font-size-xs);
          }
          .import-item-meta {
            display: flex;
            gap: 12px;
            color: var(--color-text-muted);
            font-size: var(--font-size-xs);
          }
          .import-item-reason {
            color: var(--color-text-muted);
            font-size: var(--font-size-xs);
            overflow-wrap: anywhere;
          }
        }
      }
    }
    .import-actions {
      display: flex;
      align-items: center;
      flex: none;
      gap: 10px;
      padding-top: 16px;
      border-top: 1px solid var(--color-line);
      .import-selected-count {
        margin-right: auto;
        color: var(--color-text-muted);
      }
      .import-button {
        display: inline-flex;
        justify-content: center;
        align-items: center;
        gap: 7px;
        min-height: 38px;
        padding: 8px 14px;
        border: 1px solid var(--color-line-strong);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: var(--font-size-sm);
        cursor: pointer;
        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
        &.import-button-primary {
          background: var(--color-primary-solid);
          border-color: var(--color-primary-solid);
          color: #fff;
        }
      }
    }
    .import-spinner {
      animation: import-spin 1s linear infinite;
    }
  }
}
@keyframes import-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

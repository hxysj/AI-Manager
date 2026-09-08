<template>
  <BaseModal class="key-manager-modal" title="API Key 管理" @close="close">
    <section class="key-manager" :aria-busy="busy">
      <header class="key-manager-overview">
        <div class="key-manager-provider">
          <span class="key-manager-provider-icon"><KeyRound :size="18" /></span>
          <div class="key-manager-provider-copy">
            <span class="key-manager-provider-name">{{
              provider.name || "Provider"
            }}</span>
            <span class="key-manager-description"
              >管理密钥配置，查看每个 Key 的请求表现</span
            >
          </div>
        </div>
        <span class="key-manager-count">{{ draft.apiKeys.length }} 个 Key</span>
      </header>

      <div class="key-manager-workspace">
        <aside class="key-manager-sidebar">
          <div class="key-manager-list-heading">
            <span>密钥列表</span>
            <button
              class="key-manager-add"
              type="button"
              title="添加 API Key"
              aria-label="添加 API Key"
              :disabled="busy"
              @click="addKey"
            >
              <Plus :size="14" /><span>添加</span>
            </button>
          </div>
          <nav class="key-manager-list" aria-label="API Key 列表">
            <button
              v-for="(item, index) in draft.apiKeys"
              :key="item.id"
              class="key-manager-list-item"
              :class="{ 'key-manager-item-selected': selectedId === item.id }"
              type="button"
              :aria-pressed="selectedId === item.id"
              @click="selectedId = item.id"
            >
              <span class="key-manager-item-heading">
                <span class="key-manager-item-name">{{
                  item.name || `Key ${index + 1}`
                }}</span>
                <span
                  v-if="draft.activeApiKeyId === item.id"
                  class="key-manager-active-tag"
                >
                  {{
                    provider.activeApiKeyId === item.id && !isCredentialChanged(item)
                      ? "生效中"
                      : "待应用"
                  }}
                </span>
              </span>
              <span class="key-manager-item-mask">{{
                isCredentialChanged(item)
                  ? "密钥待保存"
                  : item.masked || "已保存密钥"
              }}</span>
              <span class="key-manager-item-usage">
                <template
                  v-if="
                    !isCredentialChanged(item) && usage[item.id]?.requestCount
                  "
                >
                  <span
                    >{{ formatCount(usage[item.id].requestCount) }} 次请求</span
                  >
                  <span
                    :class="{
                      'key-manager-has-failures':
                        usage[item.id].failureCount > 0
                    }"
                    >{{ formatCount(usage[item.id].failureCount) }} 次失败</span
                  >
                </template>
                <span v-else>{{
                  isCredentialChanged(item) ? "保存后开始统计" : "暂无请求记录"
                }}</span>
              </span>
            </button>
            <span v-if="!draft.apiKeys.length" class="key-manager-list-empty"
              >尚未添加密钥</span
            >
          </nav>
        </aside>

        <section v-if="selectedKey" class="key-manager-detail">
          <header class="key-manager-detail-header">
            <div class="key-manager-detail-title">
              <span class="key-manager-selected-name">{{
                selectedKey.name || "未命名 Key"
              }}</span>
              <span class="key-manager-selected-state">{{
                selectedState
              }}</span>
            </div>
            <div class="key-manager-key-actions">
              <button
                v-if="draft.activeApiKeyId !== selectedKey.id"
                class="key-manager-activate"
                type="button"
                :disabled="busy"
                @click="$emit('activate', selectedKey.id)"
              >
                <Check :size="14" /><span>设为生效</span>
              </button>
              <button
                class="key-manager-delete"
                type="button"
                :disabled="busy"
                title="删除当前 API Key"
                aria-label="删除当前 API Key"
                @click="removeKey"
              >
                <Trash2 :size="15" />
              </button>
            </div>
          </header>

          <div class="key-manager-tabs" role="tablist" aria-label="密钥详情">
            <button
              id="key-manager-config-tab"
              class="key-manager-tab"
              type="button"
              role="tab"
              :aria-selected="activeTab === 'config'"
              aria-controls="key-manager-config"
              :tabindex="activeTab === 'config' ? 0 : -1"
              @click="activeTab = 'config'"
              @keydown="switchTab"
            >
              <SlidersHorizontal :size="14" />配置
            </button>
            <button
              id="key-manager-usage-tab"
              class="key-manager-tab"
              type="button"
              role="tab"
              :aria-selected="activeTab === 'usage'"
              aria-controls="key-manager-usage"
              :tabindex="activeTab === 'usage' ? 0 : -1"
              @click="activeTab = 'usage'"
              @keydown="switchTab"
            >
              <Activity :size="14" />用量统计
            </button>
          </div>

          <div class="key-manager-detail-scroll">
            <section
              v-show="activeTab === 'config'"
              id="key-manager-config"
              class="key-manager-config"
              role="tabpanel"
              aria-labelledby="key-manager-config-tab"
            >
              <label class="key-manager-field">
                <span class="key-manager-field-label">Key 名称</span>
                <el-input
                  :model-value="selectedKey.name"
                  :disabled="busy"
                  placeholder="例如：主用 Key"
                  @update:model-value="(value) => updateKey({ name: value })"
                />
              </label>
              <label class="key-manager-field">
                <span class="key-manager-field-label">API Key</span>
                <el-input
                  :key="selectedKey.id"
                  :model-value="selectedKey.apiKey"
                  :disabled="busy"
                  type="password"
                  show-password
                  autocomplete="new-password"
                  spellcheck="false"
                  :placeholder="selectedKey.masked || '输入 API Key'"
                  @update:model-value="(value) => updateKey({ apiKey: value })"
                />
                <span class="key-manager-field-hint">{{
                  selectedKey.masked
                    ? "已保存的密钥留空则保持不变，输入新值将替换密钥。"
                    : "密钥加密保存在本机，请勿在备注中填写完整密钥。"
                }}</span>
              </label>
              <label class="key-manager-field">
                <span class="key-manager-field-label"
                  >备注<span class="key-manager-optional">选填</span></span
                >
                <el-input
                  :model-value="selectedKey.note"
                  :disabled="busy"
                  placeholder="例如：生产环境 / 备用额度"
                  @update:model-value="(value) => updateKey({ note: value })"
                />
              </label>
              <div class="key-manager-config-note">
                <Info :size="14" /><span
                  >只有生效的 Key
                  会被运行时使用。所有修改在“保存并应用”后生效。</span
                >
              </div>
            </section>

            <section
              v-show="activeTab === 'usage'"
              id="key-manager-usage"
              class="key-manager-usage"
              role="tabpanel"
              aria-labelledby="key-manager-usage-tab"
            >
              <div class="key-manager-usage-toolbar">
                <div class="key-manager-usage-description">
                  <span>每 5 秒更新</span>
                  <el-tooltip
                    content="仅统计经本项目发送的请求；CLI/Desktop 直连与官方账号不计入，历史请求不推测 Key 归属。"
                    placement="top"
                    :show-after="200"
                  >
                    <button
                      class="key-manager-info"
                      type="button"
                      aria-label="查看统计范围说明"
                    >
                      <Info :size="14" />
                    </button>
                  </el-tooltip>
                </div>
                <button
                  class="key-manager-refresh"
                  type="button"
                  :disabled="usageLoading"
                  @click="$emit('refresh')"
                >
                  <RefreshCw
                    :size="14"
                    :class="{ 'key-manager-refreshing': usageLoading }"
                  />{{ usageLoading ? "更新中" : "刷新" }}
                </button>
              </div>
              <p v-if="usageError" class="key-manager-error" role="alert">
                {{ usageError }}
              </p>
              <ApiKeyUsageStats
                :usage="usage[selectedKey.id]"
                :pending="isCredentialChanged(selectedKey)"
              />
            </section>
          </div>
        </section>

        <div v-else class="key-manager-empty">
          <KeyRound :size="28" />
          <span>添加你的第一个 API Key</span>
          <span class="key-manager-empty-description"
            >支持多个密钥，分别配置名称、备注和使用状态。</span
          >
          <button
            class="key-manager-empty-add"
            type="button"
            :disabled="busy"
            @click="addKey"
          >
            <Plus :size="15" />添加 API Key
          </button>
        </div>
      </div>

      <footer class="key-manager-footer">
        <span
          class="key-manager-save-hint"
          :class="{ 'key-manager-unsaved': hasChanges }"
          >{{ hasChanges ? "有未应用的修改" : "配置已保存" }}</span
        >
        <div class="key-manager-footer-actions">
          <button
            class="key-manager-cancel"
            type="button"
            :disabled="busy"
            @click="close"
          >
            取消
          </button>
          <button
            class="key-manager-save"
            type="button"
            :disabled="busy"
            @click="$emit('save')"
          >
            <Save :size="15" />{{ busy ? "保存中…" : "保存并应用" }}
          </button>
        </div>
      </footer>
    </section>
  </BaseModal>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import {
  Activity,
  Check,
  Info,
  KeyRound,
  Plus,
  RefreshCw,
  Save,
  SlidersHorizontal,
  Trash2
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import ApiKeyUsageStats from "./ApiKeyUsageStats.vue"

const props = defineProps({
  provider: { type: Object, required: true },
  draft: { type: Object, required: true },
  usage: { type: Object, default: () => ({}) },
  usageLoading: { type: Boolean, default: false },
  usageError: { type: String, default: "" },
  busy: { type: Boolean, default: false }
})
const emit = defineEmits([
  "close",
  "save",
  "add",
  "remove",
  "activate",
  "update-key",
  "refresh"
])
const selectedId = ref("")
const activeTab = ref("config")
const selectedKey = computed(() =>
  props.draft.apiKeys.find((item) => item.id === selectedId.value)
)
const selectedState = computed(() => {
  if (props.draft.activeApiKeyId === selectedId.value) {
    return props.provider.activeApiKeyId === selectedId.value &&
      !isCredentialChanged(selectedKey.value)
      ? "当前生效"
      : "保存后生效"
  }
  return props.provider.activeApiKeyId === selectedId.value
    ? "保存后切换为备用"
    : "备用密钥"
})
const hasChanges = computed(() => {
  const savedKeys = props.provider.apiKeys || []
  return (
    props.draft.activeApiKeyId !== (props.provider.activeApiKeyId || "") ||
    props.draft.apiKeys.length !== savedKeys.length ||
    props.draft.apiKeys.some((item) => {
      const saved = savedKeys.find((key) => key.id === item.id)
      return (
        !saved ||
        item.name !== saved.name ||
        item.note !== (saved.note || "") ||
        isCredentialChanged(item)
      )
    })
  )
})

watch(
  () => props.draft.apiKeys.map((item) => item.id),
  (ids) => {
    if (!ids.includes(selectedId.value)) {
      selectedId.value = ids.includes(props.draft.activeApiKeyId)
        ? props.draft.activeApiKeyId
        : ids[0] || ""
    }
  },
  { immediate: true }
)

function isCredentialChanged(item) {
  const saved = props.provider.apiKeys?.find((key) => key.id === item.id)
  if (!saved) return true
  const savedValue =
    saved.apiKey ||
    (props.provider.activeApiKeyId === item.id ? props.provider.apiKey : "")
  return Boolean(item.apiKey && item.apiKey !== savedValue)
}

function formatCount(value) {
  return Number(value || 0).toLocaleString("zh-CN")
}

function updateKey(patch) {
  if (selectedKey.value && !props.busy)
    emit("update-key", selectedKey.value.id, patch)
}

async function addKey() {
  if (props.busy) return
  emit("add")
  await nextTick()
  selectedId.value = props.draft.apiKeys.at(-1)?.id || ""
  activeTab.value = "config"
}

function removeKey() {
  if (props.busy) return
  const index = props.draft.apiKeys.findIndex(
    (item) => item.id === selectedId.value
  )
  if (index >= 0) emit("remove", index)
}

function close() {
  if (!props.busy) emit("close")
}

async function switchTab(event) {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return
  event.preventDefault()
  const tabList = event.currentTarget.parentElement
  activeTab.value =
    event.key === "Home"
      ? "config"
      : event.key === "End"
        ? "usage"
        : activeTab.value === "config"
          ? "usage"
          : "config"
  await nextTick()
  tabList?.querySelector('[aria-selected="true"]')?.focus()
}
</script>

<style scoped lang="less">
.key-manager-modal.base-modal {
  padding: 24px;

  :deep(.base-modal__panel) {
    width: min(940px, calc(100vw - 48px));
    height: min(680px, calc(100dvh - 48px));
  }

  :deep(.base-modal__header) {
    align-items: center;
    flex: none;
    padding: 18px 22px 14px;
  }

  :deep(.base-modal__content) {
    padding: 0;
  }

  .key-manager {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    font-size: var(--font-size-sm);

    .key-manager-overview {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 4px 22px 18px;

      .key-manager-provider {
        display: flex;
        min-width: 0;
        align-items: center;
        gap: 10px;

        .key-manager-provider-icon {
          display: grid;
          width: 36px;
          height: 36px;
          flex: none;
          place-items: center;
          border-radius: 10px;
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }

        .key-manager-provider-copy {
          display: flex;
          min-width: 0;
          flex-direction: column;
          gap: 3px;

          .key-manager-provider-name {
            overflow: hidden;
            color: var(--color-text);
            text-overflow: ellipsis;
            white-space: nowrap;
          }

          .key-manager-description {
            color: var(--color-text-muted);
            font-size: var(--font-size-xs);
          }
        }
      }

      .key-manager-count {
        flex: none;
        padding: 3px 8px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        font-variant-numeric: tabular-nums;
      }
    }

    .key-manager-workspace {
      display: grid;
      min-width: 0;
      min-height: 0;
      flex: 1;
      grid-template-columns: 220px minmax(0, 1fr);
      border-top: 1px solid var(--color-line);

      .key-manager-sidebar {
        display: flex;
        min-width: 0;
        min-height: 0;
        flex-direction: column;
        border-right: 1px solid var(--color-line);
        background: var(--color-panel-soft);

        .key-manager-list-heading {
          display: flex;
          flex: none;
          align-items: center;
          justify-content: space-between;
          padding: 14px 14px 8px;
          color: var(--color-text-muted);
          font-size: var(--font-size-xs);

          .key-manager-add {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            padding: 4px 6px;
            border: 0;
            border-radius: 5px;
            background: transparent;
            color: var(--color-primary);
          }
        }

        .key-manager-list {
          display: flex;
          min-height: 0;
          flex-direction: column;
          gap: 6px;
          overflow-y: auto;
          padding: 0 10px 12px;
          scrollbar-width: thin;

          .key-manager-list-item {
            display: flex;
            min-width: 0;
            flex: none;
            flex-direction: column;
            gap: 7px;
            padding: 12px 10px;
            border: 1px solid transparent;
            border-radius: 8px;
            background: transparent;
            color: var(--color-text);
            text-align: left;

            &:hover {
              background: var(--color-panel);
            }

            &.key-manager-item-selected {
              border-color: var(--color-info-line);
              background: var(--color-panel);
              box-shadow: inset 3px 0 0 var(--color-primary);
            }

            .key-manager-item-heading {
              display: flex;
              min-width: 0;
              align-items: center;
              justify-content: space-between;
              gap: 6px;

              .key-manager-item-name {
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
              }

              .key-manager-active-tag {
                flex: none;
                padding: 1px 5px;
                border-radius: 4px;
                background: var(--color-primary-soft);
                color: var(--color-primary);
                font-size: var(--font-size-xs);
              }
            }

            .key-manager-item-mask {
              overflow: hidden;
              color: var(--color-text-muted);
              font-family: ui-monospace, Consolas, monospace;
              font-size: var(--font-size-xs);
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .key-manager-item-usage {
              display: flex;
              flex-wrap: wrap;
              gap: 4px 10px;
              color: var(--color-text-muted);
              font-size: var(--font-size-xs);
              font-variant-numeric: tabular-nums;

              .key-manager-has-failures {
                color: var(--color-danger);
              }
            }
          }

          .key-manager-list-empty {
            padding: 12px 4px;
            color: var(--color-text-soft);
            font-size: var(--font-size-xs);
          }
        }
      }

      .key-manager-detail {
        display: flex;
        min-width: 0;
        min-height: 0;
        flex-direction: column;

        .key-manager-detail-header {
          display: flex;
          flex: none;
          align-items: center;
          justify-content: space-between;
          gap: 12px;
          padding: 18px 22px 12px;

          .key-manager-detail-title {
            display: flex;
            min-width: 0;
            flex-direction: column;
            gap: 3px;

            .key-manager-selected-name {
              overflow: hidden;
              font-size: var(--font-size-base);
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .key-manager-selected-state {
              color: var(--color-text-muted);
              font-size: var(--font-size-xs);
            }
          }

          .key-manager-key-actions {
            display: flex;
            flex: none;
            align-items: center;
            gap: 8px;

            .key-manager-activate,
            .key-manager-delete {
              display: inline-flex;
              height: 30px;
              align-items: center;
              justify-content: center;
              gap: 5px;
              padding: 0 8px;
              border: 1px solid var(--color-line);
              border-radius: 6px;
              background: var(--color-panel);
              color: var(--color-primary);
              font-size: var(--font-size-xs);
            }

            .key-manager-delete {
              width: 30px;
              padding: 0;
              color: var(--color-text-muted);

              &:hover {
                border-color: var(--color-danger-line);
                background: var(--color-danger-soft);
                color: var(--color-danger);
              }
            }
          }
        }

        .key-manager-tabs {
          display: flex;
          flex: none;
          gap: 18px;
          margin: 0 22px;
          border-bottom: 1px solid var(--color-line);

          .key-manager-tab {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            padding: 9px 1px;
            border: 0;
            border-bottom: 2px solid transparent;
            background: transparent;
            color: var(--color-text-muted);

            &[aria-selected="true"] {
              border-bottom-color: var(--color-primary);
              color: var(--color-primary);
            }
          }
        }

        .key-manager-detail-scroll {
          min-height: 0;
          flex: 1;
          overflow-y: auto;
          padding: 20px 22px;
          scrollbar-width: thin;

          .key-manager-config {
            display: flex;
            flex-direction: column;
            gap: 18px;

            .key-manager-field {
              display: flex;
              min-width: 0;
              flex-direction: column;
              gap: 7px;

              .key-manager-field-label {
                display: flex;
                align-items: center;
                gap: 7px;
                color: var(--color-text);

                .key-manager-optional {
                  color: var(--color-text-soft);
                  font-size: var(--font-size-xs);
                }
              }

              :deep(.el-input__wrapper) {
                min-height: 36px;
                border-radius: 7px;
                background: var(--color-panel);
              }

              :deep(.el-input__inner) {
                font-size: var(--font-size-sm);
              }

              .key-manager-field-hint {
                color: var(--color-text-muted);
                font-size: var(--font-size-xs);
                line-height: 1.6;
              }
            }

            .key-manager-config-note {
              display: flex;
              align-items: flex-start;
              gap: 7px;
              padding: 10px 12px;
              border-radius: 7px;
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
              font-size: var(--font-size-xs);
              line-height: 1.7;
            }
          }

          .key-manager-usage {
            display: flex;
            flex-direction: column;
            gap: 14px;

            .key-manager-usage-toolbar {
              display: flex;
              align-items: center;
              justify-content: space-between;
              gap: 10px;

              .key-manager-usage-description {
                display: flex;
                align-items: center;
                gap: 6px;
                color: var(--color-text-muted);
                font-size: var(--font-size-xs);

                .key-manager-info {
                  display: grid;
                  padding: 2px;
                  place-items: center;
                  border: 0;
                  background: transparent;
                  color: var(--color-text-muted);
                }
              }

              .key-manager-refresh {
                display: inline-flex;
                align-items: center;
                gap: 6px;
                padding: 5px 8px;
                border: 1px solid var(--color-line);
                border-radius: 6px;
                background: var(--color-panel);
                color: var(--color-text);
                font-size: var(--font-size-xs);

                .key-manager-refreshing {
                  animation: key-manager-spin 1s linear infinite;
                }
              }
            }

            .key-manager-error {
              margin: 0;
              color: var(--color-danger);
              font-size: var(--font-size-xs);
              overflow-wrap: anywhere;
            }
          }
        }
      }

      .key-manager-empty {
        display: flex;
        min-width: 0;
        align-items: center;
        justify-content: center;
        flex-direction: column;
        gap: 12px;
        padding: 22px;
        color: var(--color-text-muted);
        text-align: center;

        .key-manager-empty-description {
          font-size: var(--font-size-xs);
        }

        .key-manager-empty-add {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          padding: 8px 12px;
          border: 1px solid var(--color-info-line);
          border-radius: 7px;
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
      }
    }

    .key-manager-footer {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 14px 22px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel);

      .key-manager-save-hint {
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);

        &.key-manager-unsaved {
          color: var(--color-warning);
        }
      }

      .key-manager-footer-actions {
        display: flex;
        flex: none;
        gap: 8px;

        .key-manager-cancel,
        .key-manager-save {
          display: inline-flex;
          min-height: 36px;
          align-items: center;
          justify-content: center;
          gap: 7px;
          padding: 0 14px;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }

        .key-manager-save {
          border-color: var(--color-primary-solid);
          background: var(--color-primary-solid);
          color: #fff;
        }
      }
    }
  }

  @media (max-width: 740px) {
    padding: 12px;

    :deep(.base-modal__panel) {
      width: calc(100vw - 24px);
      height: calc(100dvh - 24px);
      max-height: calc(100dvh - 24px);
    }

    :deep(.base-modal__header) {
      padding: 12px 16px 8px;
    }

    .key-manager {
      .key-manager-overview {
        padding: 2px 16px 12px;

        .key-manager-provider
          .key-manager-provider-copy
          .key-manager-description {
          display: none;
        }
      }

      .key-manager-workspace {
        grid-template-columns: minmax(0, 1fr);
        grid-template-rows: auto minmax(0, 1fr);

        .key-manager-sidebar {
          flex-direction: row;
          align-items: center;
          border-right: 0;
          border-bottom: 1px solid var(--color-line);

          .key-manager-list-heading {
            order: 1;
            padding: 8px;
            font-size: 0;

            .key-manager-add {
              gap: 0;
              padding: 7px;
              font-size: 0;
            }
          }

          .key-manager-list {
            min-width: 0;
            flex: 1;
            flex-direction: row;
            overflow-x: auto;
            padding: 8px 0 8px 12px;

            .key-manager-list-item {
              max-width: 210px;
              padding: 8px 10px;

              &.key-manager-item-selected {
                box-shadow: none;
              }

              .key-manager-item-mask,
              .key-manager-item-usage {
                display: none;
              }
            }
          }
        }

        .key-manager-detail {
          .key-manager-detail-header {
            padding: 12px 16px 6px;
          }

          .key-manager-tabs {
            margin: 0 16px;
          }

          .key-manager-detail-scroll {
            padding: 16px;
          }
        }
      }

      .key-manager-footer {
        padding: 10px 16px;
      }
    }
  }

  @media (max-width: 380px) {
    .key-manager .key-manager-footer .key-manager-save-hint {
      max-width: 70px;
      font-size: var(--font-size-xs);
    }
  }

  @media (max-height: 560px) {
    :deep(.base-modal__header) {
      padding-top: 8px;
      padding-bottom: 4px;
    }

    .key-manager {
      .key-manager-overview {
        padding-top: 0;
        padding-bottom: 8px;

        .key-manager-provider {
          .key-manager-provider-icon,
          .key-manager-provider-copy .key-manager-description {
            display: none;
          }
        }
      }

      .key-manager-workspace .key-manager-detail {
        .key-manager-detail-header {
          padding-top: 6px;
          padding-bottom: 4px;

          .key-manager-detail-title .key-manager-selected-state {
            display: none;
          }
        }

        .key-manager-tabs .key-manager-tab {
          padding-top: 6px;
          padding-bottom: 6px;
        }

        .key-manager-detail-scroll {
          padding-top: 10px;
          padding-bottom: 10px;

          .key-manager-config {
            gap: 12px;

            .key-manager-config-note {
              display: none;
            }
          }
        }
      }
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .key-manager
      .key-manager-workspace
      .key-manager-detail
      .key-manager-detail-scroll
      .key-manager-usage
      .key-manager-usage-toolbar
      .key-manager-refresh
      .key-manager-refreshing {
      animation: none;
    }
  }
}

@keyframes key-manager-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

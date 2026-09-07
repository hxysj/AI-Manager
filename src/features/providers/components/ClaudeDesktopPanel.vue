<template>
  <section class="desktop-panel" :aria-busy="busy">
    <Teleport v-if="mounted" to="#claude-desktop-toolbar">
      <div class="desktop-tools">
        <label
          class="desktop-proxy-toggle"
          :class="{
            'desktop-tool-disabled': busy || snapshot.status.mode !== 'proxy'
          }"
          title="本地路由供应商需要保持 Proxy 开启；直连不需要"
        >
          <span>Proxy</span>
          <input
            class="desktop-proxy-input"
            type="checkbox"
            :checked="snapshot.status.proxyRunning"
            :disabled="
              busy ||
              !snapshot.status.supported ||
              snapshot.status.mode !== 'proxy'
            "
            @change="setGateway($event.target.checked)"
          />
          <span class="desktop-toggle-track"></span>
        </label>
        <button
          class="desktop-tool-button"
          :disabled="busy"
          title="从 Claude Code 导入兼容供应商"
          @click="importProviders"
        >
          <Download :size="15" /><span>导入</span>
        </button>
        <button
          class="desktop-tool-button desktop-disabled-filter"
          :class="{ 'desktop-filter-active': showDisabledItems }"
          :title="showDisabledItems ? '隐藏禁用项' : '显示禁用项'"
          :aria-label="`${showDisabledItems ? '隐藏禁用项' : '显示禁用项'}，共 ${disabledItemCount} 项`"
          :aria-pressed="showDisabledItems"
          @click="showDisabledItems = !showDisabledItems"
        >
          <Eye v-if="showDisabledItems" :size="16" />
          <EyeOff v-else :size="16" />
          <span class="desktop-filter-label">{{
            showDisabledItems ? "已显示禁用项" : "显示禁用项"
          }}</span>
          <span
            v-if="disabledItemCount"
            class="providers-toolbar-count"
            :class="{ 'providers-toolbar-count-active': showDisabledItems }"
            aria-hidden="true"
            >{{ disabledItemCount }}</span
          >
        </button>
        <button
          class="desktop-tool-button"
          :disabled="busy || refreshing"
          title="刷新 Desktop 状态"
          aria-label="刷新 Desktop 状态"
          @click="refreshState"
        >
          <RefreshCw :size="16" />
        </button>
        <button
          class="desktop-tool-button"
          :disabled="busy"
          title="查看当前 Desktop 系统配置"
          aria-label="查看当前 Desktop 系统配置"
          @click="$emit('view-config', snapshot.status)"
        >
          <Server :size="17" />
        </button>
        <button
          class="desktop-tool-button desktop-tool-add"
          :disabled="busy || !snapshot.defaultRoutes.length"
          title="新增 Desktop 供应商"
          aria-label="新增 Desktop 供应商"
          @click="openEditor()"
        >
          <Plus :size="22" />
        </button>
      </div>
    </Teleport>
    <p v-if="errorText" class="desktop-error" role="alert">{{ errorText }}</p>
    <section
      v-if="snapshot.status.warnings?.length"
      class="desktop-warning"
      role="status"
    >
      <p
        v-for="warning in snapshot.status.warnings"
        :key="warning"
        class="desktop-warning-item"
      >
        {{ warning }}
      </p>
    </section>
    <slot
      name="providers"
      :providers="visibleProviders"
      :current="snapshot.currentProviderId"
      :ready="
        snapshot.status.supported && snapshot.status.needsRepair === false
      "
      :busy="busy"
      :supported="snapshot.status.supported"
      :empty="snapshot.providers.length === 1"
      :modes="modeLabels"
      :formats="formatLabels"
      :actions="{
        apply: applyProvider,
        clear: clearProvider,
        edit: openEditor,
        toggle: setProviderEnabled,
        remove: (provider) => (deleteTarget = provider)
      }"
    ></slot>

    <BaseModal
      v-if="deleteTarget"
      title="删除 Desktop 供应商"
      @close="!busy && (deleteTarget = null)"
    >
      <div class="desktop-confirm">
        <p class="desktop-confirm-text">
          确定删除「{{
            deleteTarget.name
          }}」？仅删除管理器内的供应商及密钥，不修改 Desktop 配置。
        </p>
        <button
          class="desktop-button desktop-danger"
          :disabled="busy"
          @click="removeProvider"
        >
          确认删除
        </button>
      </div>
    </BaseModal>
    <BaseModal
      v-if="importResult"
      title="Claude Code 导入结果"
      @close="importResult = null"
    >
      <div class="desktop-confirm">
        <p class="desktop-confirm-text">
          已导入
          {{ importResult.importedCount }}
          个供应商；已存在的记录保持不变。导入不修改 Desktop 配置，需点击启用。
        </p>
        <p
          v-for="reason in importResult.skipped"
          :key="reason"
          class="desktop-error"
        >
          {{ reason }}
        </p>
      </div>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import { Download, Eye, EyeOff, Plus, RefreshCw, Server } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { claudeDesktopApi } from "@/api/modules/providers"
import { createMessage } from "@/utils/message"

const modeLabels = { official: "官方模式", direct: "直连", proxy: "本地路由" }
const formatLabels = {
  anthropic: "Anthropic",
  openai_chat: "Chat Completions",
  openai_responses: "Responses"
}
const mounted = ref(false)
const emit = defineEmits(["edit-provider", "view-config", "busy-change"])
const showDisabledItems = ref(false)
const disabledItemCount = computed(
  () =>
    snapshot.value.providers.filter((provider) => provider.enabled === false)
      .length
)
const visibleProviders = computed(() =>
  snapshot.value.providers.filter(
    (provider) => showDisabledItems.value || provider.enabled !== false
  )
)
const snapshot = ref({
  providers: [],
  currentProviderId: "",
  status: {},
  defaultRoutes: []
})
const busy = ref(false)
const refreshing = ref(false)
const errorText = ref("")
const deleteTarget = ref(null)
const importResult = ref(null)
const gatewayPort = ref(15723)
let pollTimer = null
let requestVersion = 0

async function refreshState() {
  if (busy.value || refreshing.value) return
  refreshing.value = true
  const version = ++requestVersion
  try {
    const next = await claudeDesktopApi.getState()
    if (version !== requestVersion) return
    if (
      gatewayPort.value === snapshot.value.status.port ||
      !snapshot.value.status.port
    )
      gatewayPort.value = next.status.port
    snapshot.value = next
    errorText.value = ""
  } catch (cause) {
    if (version === requestVersion) errorText.value = String(cause)
  } finally {
    refreshing.value = false
  }
}

async function runAction(action, successMessage) {
  if (busy.value) return null
  busy.value = true
  emit("busy-change", true)
  const version = ++requestVersion
  try {
    const next = await action()
    if (version !== requestVersion) return null
    snapshot.value = next
    gatewayPort.value = next.status.port
    errorText.value = ""
    createMessage.success(
      next.configurationWritten
        ? "配置已写入，请完全退出并重启 Claude Desktop"
        : successMessage
    )
    return next
  } catch (cause) {
    if (version === requestVersion) {
      errorText.value = String(cause)
      createMessage.error(String(cause))
    }
    return null
  } finally {
    busy.value = false
    emit("busy-change", false)
  }
}

async function importProviders() {
  const result = await runAction(
    () => claudeDesktopApi.importProviders(),
    "导入完成，尚未修改 Desktop 配置"
  )
  if (result) importResult.value = result
}

function openEditor(provider = {}) {
  emit("edit-provider", provider, snapshot.value.defaultRoutes)
}

async function saveProvider(payload) {
  return runAction(() => claudeDesktopApi.saveProvider(payload), "供应商已保存")
}

async function setProviderEnabled(provider, enabled) {
  await runAction(
    () => claudeDesktopApi.setProviderEnabled(provider.id, enabled),
    enabled ? "已取消禁用，点击启用后生效" : "供应商已禁用"
  )
}

async function clearProvider() {
  await runAction(() => claudeDesktopApi.clearProvider(), "已取消使用")
}

async function applyProvider(provider) {
  await runAction(
    () => claudeDesktopApi.switchProvider(provider.id),
    "供应商已启用"
  )
}

async function removeProvider() {
  const result = await runAction(
    () => claudeDesktopApi.deleteProvider(deleteTarget.value.id),
    "供应商已删除"
  )
  if (result) deleteTarget.value = null
}

async function setGateway(enabled, port = gatewayPort.value) {
  return runAction(
    () => claudeDesktopApi.setGateway({ enabled, port }),
    "本地网关已停止"
  )
}

defineExpose({ saveProvider, setGateway })

onMounted(() => {
  mounted.value = true
  refreshState()
  pollTimer = window.setInterval(refreshState, 5000)
})

onBeforeUnmount(() => {
  window.clearInterval(pollTimer)
  requestVersion += 1
})
</script>

<style scoped lang="less">
.desktop-tools {
  display: flex;
  align-items: center;
  gap: 8px;
  .desktop-disabled-filter {
    position: relative;
    flex-shrink: 0;
    .desktop-filter-label {
      white-space: nowrap;
    }
  }
  .desktop-disabled-filter.desktop-filter-active {
    border-color: var(--color-warning-line);
    color: var(--color-warning);
    background: var(--color-warning-soft);
  }
  @media (width < 1100px) {
    .desktop-tool-button.desktop-disabled-filter {
      width: 38px;
      padding: 0;
      .desktop-filter-label {
        display: none;
      }
    }
  }
  .desktop-tool-button,
  .desktop-proxy-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 38px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 12px;
  }
  .desktop-tool-button {
    &:hover:not(:disabled) {
      color: var(--color-primary);
      border-color: var(--color-line-strong);
    }
    &:disabled {
      opacity: 0.45;
      cursor: not-allowed;
    }
  }
  .desktop-tool-add {
    width: 38px;
    padding: 0;
    background: var(--color-primary-solid);
    border-color: var(--color-primary-solid);
    color: #fff;
    &:hover:not(:disabled) {
      color: #fff;
    }
  }
  .desktop-tool-disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .desktop-proxy-toggle {
    position: relative;
    .desktop-proxy-input {
      position: absolute;
      width: 30px;
      height: 18px;
      right: 10px;
      opacity: 0;
      cursor: inherit;
    }
    .desktop-toggle-track {
      width: 30px;
      height: 17px;
      border-radius: 12px;
      background: var(--color-line-strong);
      pointer-events: none;
      &::after {
        content: "";
        display: block;
        width: 11px;
        height: 11px;
        margin: 3px;
        border-radius: 50%;
        background: #fff;
        transition: transform 0.15s;
      }
    }
    .desktop-proxy-input:checked + .desktop-toggle-track {
      background: var(--color-primary-solid);
      &::after {
        transform: translateX(13px);
      }
    }
    .desktop-proxy-input:focus-visible + .desktop-toggle-track {
      outline: 2px solid var(--color-primary);
      outline-offset: 3px;
    }
  }
}
.desktop-panel {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
  flex-direction: column;
  gap: 12px;
  color: var(--color-text);
  .desktop-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 34px;
    padding: 7px 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    white-space: nowrap;
    &:disabled {
      opacity: 0.45;
      cursor: not-allowed;
    }
  }
  .desktop-error {
    margin: 0;
    color: var(--color-danger);
    line-height: 1.6;
    overflow-wrap: anywhere;
  }
  .desktop-warning {
    padding: 10px 14px;
    border: 1px solid var(--color-warning-line);
    border-radius: 8px;
    background: var(--color-warning-soft);
    .desktop-warning-item {
      margin: 3px 0;
      font-size: 12px;
      line-height: 1.5;
      color: var(--color-text-muted);
      overflow-wrap: anywhere;
    }
  }
  .desktop-confirm {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 16px;
    .desktop-confirm-text {
      margin: 0;
      font-size: 14px;
      line-height: 1.7;
    }
  }
}
</style>

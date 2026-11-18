<template>
  <BaseModal
    class="codex-login-modal"
    :title="account?.id ? '更新认证信息' : '添加 Codex 账号'"
    @close="emit('close')"
  >
    <section class="codex-login-panel">
      <nav class="codex-login-tabs">
        <button
          :class="[
            'codex-login-tab',
            { 'codex-login-tab-active': tab === 'oauth' }
          ]"
          type="button"
          :disabled="pending || loginState?.status === 'pending'"
          @click="tab = 'oauth'"
        >
          <Globe2 :size="13" />
          OAuth 授权
        </button>
        <button
          :class="[
            'codex-login-tab',
            { 'codex-login-tab-active': tab === 'auth' }
          ]"
          type="button"
          :disabled="pending || loginState?.status === 'pending'"
          @click="tab = 'auth'"
        >
          <ShieldCheck :size="13" />
          JSON 数据
        </button>
      </nav>

      <label class="codex-login-field">
        <span>代理地址</span>
        <input
          class="codex-login-input"
          v-model.trim="proxy"
          type="text"
          placeholder="可选，例如：http://127.0.0.1:7890"
          :disabled="pending || loginState?.status === 'pending'"
        />
      </label>

      <template v-if="tab === 'oauth'">
        <p class="codex-login-intro">
          点击浏览器登录后才会启动本地回调服务并生成授权链接。
        </p>
        <div
          v-if="loginState?.message"
          :class="[
            'codex-login-status',
            `codex-login-status-${loginState.status}`
          ]"
        >
          {{ loginState.message }}
        </div>
        <label v-if="loginState?.authUrl" class="codex-login-field">
          <span>授权链接</span>
          <div class="codex-login-copy-row">
            <input
              class="codex-login-input"
              :value="loginState.authUrl"
              readonly
              type="text"
            />
            <button
              class="codex-login-copy"
              type="button"
              title="复制链接"
              aria-label="复制链接"
              @click="copyAuthUrl(loginState.authUrl)"
            >
              <Copy :size="16" />
            </button>
          </div>
        </label>
        <label v-if="loginState?.authUrl" class="codex-login-field">
          <span>手动输入回调地址</span>
          <div class="codex-login-callback-row">
            <input
              class="codex-login-input"
              v-model.trim="manualCallbackUrl"
              type="text"
              placeholder="粘贴完整回调地址，例如：http://localhost..."
            />
            <button
              class="codex-login-continue"
              type="button"
              :disabled="!manualCallbackUrl"
              @click="openExternalLink(manualCallbackUrl)"
            >
              <Check :size="15" />
              我已授权，继续
            </button>
          </div>
        </label>
        <div v-if="loginState?.authUrl" class="codex-login-tip">
          完成授权后，此窗口将自动更新
        </div>
      </template>

      <template v-else>
        <p class="codex-login-intro">
          粘贴已有 Codex 登录 JSON 数据，Monkey Thief 会使用 access_token
          解析并验证账号。
        </p>
        <div class="codex-auth-session">
          <span
            >在浏览器登录 ChatGPT 后，打开下方地址并复制页面 JSON 到这里。</span
          >
          <div class="codex-auth-session-row">
            <a
              class="codex-auth-session-link"
              :href="sessionUrl"
              @click.prevent="openExternalLink(sessionUrl)"
              >{{ sessionUrl }}</a
            >
            <button
              class="codex-auth-session-copy"
              type="button"
              @click="copyAuthUrl(sessionUrl)"
            >
              <Copy :size="15" />复制地址
            </button>
          </div>
          <small>JSON 包含登录凭据，请勿分享给他人。</small>
        </div>
        <label class="codex-login-field">
          <span>JSON 数据</span>
          <textarea
            class="codex-login-json"
            v-model.trim="content"
            placeholder='{"access_token":"","account_id":"","id_token":"","refresh_token":""}'
          />
        </label>
      </template>
    </section>
    <footer class="codex-login-footer">
      <button
        v-if="tab === 'auth'"
        class="codex-login-action"
        type="button"
        :disabled="pending || loginState?.status === 'pending' || !content"
        @click="importAuth"
      >
        <Check :size="15" />解析并验证
      </button>
      <template v-else>
        <button
          v-if="loginState?.status === 'pending'"
          class="codex-login-action"
          type="button"
          @click="emit('cancel')"
        >
          取消登录
        </button>
        <button
          class="codex-login-primary"
          type="button"
          :disabled="pending || loginState?.status === 'pending'"
          @click="startLogin"
        >
          <Globe2 :size="16" />浏览器登录
        </button>
      </template>
    </footer>
  </BaseModal>
</template>

<script setup>
import { ref, watch } from "vue"
import { Check, Copy, Globe2, ShieldCheck } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { systemApi } from "@/api/modules/system"
import { createMessage } from "@/utils/message"

const props = defineProps({
  account: { type: Object, default: null },
  pending: { type: Boolean, default: false },
  loginState: { type: Object, default: null },
  initialTab: { type: String, default: "oauth" }
})
const emit = defineEmits(["close", "login", "cancel", "import-auth"])
const tab = ref(props.initialTab)
const proxy = ref(props.account?.proxy || "")
const content = ref("")
const manualCallbackUrl = ref("")
const sessionUrl = "https://chatgpt.com/api/auth/session"
watch(
  () => props.loginState,
  () => {
    manualCallbackUrl.value = ""
  }
)
function startLogin() {
  emit("login", { accountId: props.account?.id || "", proxy: proxy.value })
}
function importAuth() {
  emit("import-auth", {
    accountId: props.account?.id || "",
    proxy: proxy.value,
    content: content.value
  })
}
async function copyAuthUrl(url) {
  try {
    await navigator.clipboard.writeText(url || "")
    createMessage.success("链接已复制。")
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}
async function openExternalLink(url) {
  try {
    await systemApi.openExternal({ url })
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}
</script>

<style scoped lang="less">
.codex-login-modal {
  :deep(.base-modal__panel) {
    width: 560px;
    max-width: 100%;
    border-radius: 16px;
  }
  :deep(.base-modal__header) {
    flex: none;
    align-items: center;
    padding: 22px 26px 20px;
    border-bottom: 1px solid var(--color-line);
  }
  :deep(.base-modal__content) {
    padding: 18px 26px 24px;
  }
  .codex-login-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: 14px;
    overflow: auto;
    // 仅正文区域滚动，标题和底部操作始终留在可见区域。
    > * {
      flex-shrink: 0;
    }
    .codex-login-tabs {
      display: flex;
      gap: 4px;
      padding: 4px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-primary-soft);
      .codex-login-tab {
        display: inline-flex;
        flex: 1;
        align-items: center;
        justify-content: center;
        gap: 5px;
        height: 30px;
        padding: 0 6px;
        border: 0;
        border-radius: 7px;
        background: transparent;
        color: var(--color-text-muted);
        cursor: pointer;
        font-size: var(--font-size-xs);
        &.codex-login-tab-active {
          background: var(--color-primary-solid);
          color: #fff;
        }
        &:disabled {
          cursor: not-allowed;
          opacity: 0.7;
        }
      }
    }
    .codex-login-intro {
      margin: 10px 0 0;
      color: var(--color-text-muted);
      font-size: var(--font-size-base);
      line-height: 1.6;
    }
    .codex-login-field {
      display: flex;
      flex-direction: column;
      gap: 8px;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
      .codex-login-copy-row,
      .codex-login-callback-row {
        display: flex;
        gap: 8px;
      }
      .codex-login-copy-row,
      .codex-login-callback-row {
        .codex-login-input {
          flex: 1;
        }
        .codex-login-copy,
        .codex-login-continue {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          flex: none;
          gap: 6px;
          height: 36px;
          padding: 0 13px;
          border: 1px solid var(--color-line);
          border-radius: 10px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          cursor: pointer;
          &:disabled {
            cursor: not-allowed;
            opacity: 0.52;
          }
        }
        .codex-login-copy {
          width: 44px;
          padding: 0;
        }
      }
      .codex-login-input,
      .codex-login-json {
        min-width: 0;
        padding: 0 12px;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }
      .codex-login-input {
        height: 36px;
      }
      .codex-login-json {
        height: 160px;
        padding: 12px;
        line-height: 1.55;
        resize: none;
      }
    }
    .codex-login-tip {
      display: flex;
      align-items: center;
      min-height: 36px;
      padding: 0 12px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
    }
    .codex-login-status {
      padding: 9px 11px;
      border-radius: 8px;
      font-size: var(--font-size-sm);
      color: var(--color-text-muted);
      background: var(--color-panel-soft);
      &.codex-login-status-success {
        color: var(--color-success);
        background: var(--color-success-soft);
      }
      &.codex-login-status-failed {
        color: var(--color-danger);
        background: var(--color-danger-soft);
      }
      &.codex-login-status-cancelled {
        color: var(--color-warning);
        background: var(--color-warning-soft);
      }
    }
    .codex-auth-session {
      display: flex;
      flex-direction: column;
      gap: 7px;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
      .codex-auth-session-row {
        display: flex;
        align-items: center;
        gap: 8px;
        .codex-auth-session-link {
          flex: 1;
          min-width: 0;
          color: var(--color-primary);
          overflow-wrap: anywhere;
          text-decoration: underline;
        }
        .codex-auth-session-copy {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 6px 9px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel);
          color: inherit;
          white-space: nowrap;
          cursor: pointer;
        }
      }
    }
  }
  .codex-login-footer {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 16px;
    margin-top: 16px;
    border-top: 1px solid var(--color-line);
    .codex-login-action,
    .codex-login-primary {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 7px;
      height: 38px;
      padding: 0 16px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;
      font-size: var(--font-size-sm);
      cursor: pointer;
      &:disabled {
        opacity: 0.52;
        cursor: default;
      }
    }
    .codex-login-primary {
      background: var(--color-primary-solid);
      border-color: var(--color-primary-solid);
      color: white;
    }
  }
}
</style>

<template>
  <BaseModal
    title="Google 账号"
    description="登录后查看模型额度，选择 Codex 使用的模型。"
    @close="close"
  >
    <section class="google-account">
      <div class="google-identity">
        <AiIcon class="google-icon" name="google" alt="Google" />
        <div class="google-identity-text">
          <span data-emphasis>{{
            account?.google?.name || "连接 Google 账号"
          }}</span>
          <span class="google-muted">{{
            account?.google?.email || "通过浏览器授权登录，无需填写 API Key"
          }}</span>
        </div>
        <span v-if="account?.google?.plan" class="google-plan" :title="account.google.plan">{{
          planLabel
        }}</span>
      </div>

      <label class="google-field">
        <span>网络代理（可选）</span>
        <input
          v-model="proxy"
          class="google-input"
          placeholder="例如 http://127.0.0.1:7890"
          :disabled="busy || waiting"
        />
        <span class="google-muted"
          >用于登录、刷新额度和调用 Google 模型；留空使用系统网络设置。</span
        >
      </label>

      <div class="google-actions">
        <button
          class="google-button google-primary"
          :disabled="busy || waiting"
          @click="login"
        >
          <AiIcon class="google-button-icon" name="google" alt="" />
          {{ account ? "重新登录" : "登录 Google 账号" }}
        </button>
        <button
          v-if="waiting"
          class="google-button"
          :disabled="busy"
          @click="cancel"
        >
          取消授权
        </button>
        <button
          v-if="account"
          class="google-button"
          :disabled="busy || waiting || account.enabled === false"
          @click="refresh"
        >
          <RefreshCw
            :size="15"
            :class="{ 'google-spinning': busy }"
          />刷新模型与额度
        </button>
        <span v-if="account?.google?.updatedAt" class="google-muted"
          >{{ formatDateTime(account.google.updatedAt) }} 更新</span
        >
      </div>
      <p
        v-if="message"
        class="google-status"
        :class="{ 'google-error': failed }"
        role="status"
      >
        {{ message }}
      </p>
      <div v-if="waiting && loginState.authUrl" class="google-auth-link">
        <input
          class="google-input"
          :value="loginState.authUrl"
          readonly
          aria-label="Google 授权链接"
        />
        <button class="google-button" @click="copyLink">复制链接</button>
      </div>
      <p v-if="account?.google?.message" class="google-status google-error">
        {{ account.google.message }}
      </p>
      <p v-if="account?.google?.quotaWarning" class="google-status">
        分组额度暂未获取，当前显示模型接口额度。{{
          account.google.quotaWarning
        }}
      </p>

      <template v-if="account">
        <div v-if="account.google?.quotaGroups?.length" class="google-groups">
          <div class="google-model-heading">
            <span data-emphasis>共享额度</span>
            <span class="google-muted">同组模型共用，两个周期分别计算</span>
          </div>
          <div
            v-for="(group, index) in account.google.quotaGroups"
            :key="index"
            class="google-group"
          >
            <span data-emphasis>{{ group.displayName }}</span>
            <div
              v-for="(bucket, bucketIndex) in group.buckets"
              :key="bucketIndex"
              class="google-bucket"
            >
              <div class="google-bucket-heading">
                <span>{{ quotaWindowLabel(bucket) }}</span>
                <span>{{ bucket.remainingFraction == null ? "额度未知" : `剩余 ${Math.round(bucket.remainingFraction * 1000) / 10}%` }}</span>
              </div>
              <div class="google-quota-track">
                <span
                  class="google-quota-fill"
                  :class="{ 'google-quota-low': bucket.remainingFraction != null && bucket.remainingFraction <= 0.1 }"
                  :style="{ width: `${(bucket.remainingFraction ?? 0) * 100}%` }"
                ></span>
              </div>
              <span class="google-muted" :title="bucket.resetTime || ''">{{ resetLabel(bucket.resetTime) }}</span>
            </div>
          </div>
        </div>
        <div class="google-model-heading">
          <span data-emphasis>可用模型</span>
          <span class="google-muted"
            >{{ models.length }} 个 · 选择一个作为默认模型</span
          >
        </div>
        <p v-if="account.google?.quotaGroups?.length" class="google-model-note google-muted">
          以下为单模型额度，使用时还受上方共享额度限制。
        </p>
        <div
          v-if="models.length"
          class="google-models"
          role="radiogroup"
          aria-label="默认模型"
        >
          <label
            v-for="item in models"
            :key="item.id"
            class="google-model"
            :class="{ 'google-model-selected': model === item.id }"
          >
            <input
              v-model="model"
              type="radio"
              :value="item.id"
              :disabled="busy || waiting || account.enabled === false"
            />
            <div class="google-model-info">
              <span data-emphasis>{{ item.displayName }}</span>
              <span class="google-muted">{{ item.id }}</span>
            </div>
            <div class="google-model-quota">
              <span>{{
                item.remainingPercent == null
                  ? "额度未知"
                  : `模型额度剩余 ${item.remainingPercent}%`
              }}</span>
              <div class="google-quota-track">
                <span
                  class="google-quota-fill"
                  :class="{
                    'google-quota-low':
                      item.remainingPercent != null &&
                      item.remainingPercent <= 10
                  }"
                  :style="{ width: `${item.remainingPercent ?? 0}%` }"
                ></span>
              </div>
              <span class="google-muted" :title="item.resetTime || ''">{{
                resetLabel(item.resetTime)
              }}</span>
            </div>
          </label>
        </div>
        <p v-else class="google-empty">
          暂未获取到可用模型，请检查账号状态及网络代理后刷新。
        </p>
        <div class="google-footer">
          <span class="google-muted"
            >启用后自动开启本地转发，使用期间请保持本应用运行。支持本地和 MCP
            工具；联网搜索请使用 MCP。</span
          >
          <button
            class="google-button google-primary"
            :disabled="busy || waiting || account.enabled === false"
            @click="save"
          >
            保存设置
          </button>
        </div>
      </template>
    </section>
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"
import { RefreshCw } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import AiIcon from "@/components/AiIcon.vue"
import { googleAccountApi } from "@/api/modules/accounts"
import { createMessage } from "@/utils/message"

const props = defineProps({ provider: { type: Object, default: null } })
const emit = defineEmits(["close", "created"])
const account = ref(props.provider)
const proxy = ref(props.provider?.google?.proxy || "")
const model = ref(props.provider?.runtimeConfig?.mainModel || "")
const busy = ref(false)
const loginState = ref({})
const message = ref("")
const failed = ref(false)
const models = computed(() => account.value?.google?.models || [])
const waiting = computed(() => loginState.value.status === "pending")
// 保留原始等级作为提示，将 Google 已知的订阅标识转换成可读名称。
const planLabel = computed(() => ({
  "g1-pro-tier": "Google AI Pro",
  "g1-ultra-tier": "Google AI Ultra",
  "free-tier": "免费版",
  "FREE": "免费版"
})[account.value?.google?.plan] || account.value?.google?.plan)
let pollTimer = null
let stopped = false

watch(
  () => props.provider,
  (provider) => {
    if (provider) account.value = provider
  }
)

function applyAccount(provider) {
  account.value = provider
  model.value = provider.runtimeConfig?.mainModel || ""
  proxy.value = provider.google?.proxy || ""
}

async function run(action) {
  busy.value = true
  failed.value = false
  message.value = ""
  try {
    await action()
  } catch (error) {
    failed.value = true
    message.value = error.message || String(error)
  } finally {
    busy.value = false
  }
}

// 串行轮询授权状态，关闭弹窗后停止监听，避免重复轮询或遗留授权服务。
async function poll() {
  if (stopped) return
  try {
    const result = await googleAccountApi.state()
    if (stopped) return
    loginState.value = result || {}
    message.value = result?.message || ""
    failed.value = result?.status === "failed"
    if (result?.status === "success" && result.provider) {
      applyAccount(result.provider)
      emit("created", result.providerId)
    }
  } catch (error) {
    message.value = error.message || String(error)
    failed.value = true
  }
  if (!stopped && waiting.value) pollTimer = setTimeout(poll, 1000)
}

function login() {
  return run(async () => {
    loginState.value = await googleAccountApi.login({
      providerId: account.value?.id,
      proxy: proxy.value
    })
    message.value = loginState.value.message
    pollTimer = setTimeout(poll, 1000)
  })
}

function cancel() {
  return run(async () => {
    clearTimeout(pollTimer)
    loginState.value = await googleAccountApi.cancel()
    message.value = loginState.value.message
  })
}

function refresh() {
  return run(async () => {
    // 先保存网络设置，额度接口失败时仍可修改代理后重试。
    await googleAccountApi.save({
      providerId: account.value.id,
      model: model.value,
      proxy: proxy.value
    })
    applyAccount(await googleAccountApi.refresh(account.value.id))
    message.value = "模型与额度已更新"
  })
}

function save() {
  return run(async () => {
    applyAccount(
      await googleAccountApi.save({
        providerId: account.value.id,
        model: model.value,
        proxy: proxy.value
      })
    )
    message.value = "设置已保存"
    createMessage.success("Google 账号设置已保存。")
  })
}

async function copyLink() {
  try {
    await navigator.clipboard.writeText(loginState.value.authUrl)
    createMessage.success("授权链接已复制。")
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

// 兼容当前接口与参考文档中的两种窗口命名，未知窗口保留接口名称。
function quotaWindowLabel(bucket) {
  const window = String(bucket.window || "").replace(/^WINDOW_/i, "").toLowerCase()
  return ({ "5h": "5 小时额度", "weekly": "周额度" })[window] || bucket.displayName || bucket.window || "额度"
}

function formatDateTime(value) {
  return new Date(value).toLocaleString("zh-CN", { hour12: false })
}

function resetLabel(value) {
  if (!value) return "恢复时间未知"
  const time = new Date(value).getTime()
  if (!Number.isFinite(time)) return "恢复时间未知"
  if (time <= Date.now()) return "周期已结束，请刷新额度"
  return `${formatDateTime(time)} 重置`
}

async function close() {
  if (waiting.value) await cancel()
  emit("close")
}

onBeforeUnmount(() => {
  stopped = true
  clearTimeout(pollTimer)
  if (waiting.value) googleAccountApi.cancel().catch(() => {})
})
</script>

<style scoped lang="less">
.google-account {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  gap: 18px;
  color: var(--color-text);

  .google-muted {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
  .google-identity {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    .google-icon {
      width: 32px;
      height: 32px;
    }
    .google-identity-text {
      display: flex;
      flex: 1;
      flex-direction: column;
      gap: 5px;
    }
    .google-plan {
      padding: 5px 10px;
      border-radius: 6px;
      background: rgba(52, 168, 83, 0.1);
      color: #258343;
    }
  }
  .google-field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .google-input {
    min-width: 0;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    color: inherit;
    background: var(--color-panel);
  }
  .google-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 9px 14px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    color: inherit;
    background: var(--color-panel);
    cursor: pointer;
    &:disabled {
      opacity: 0.5;
      cursor: default;
    }
    &.google-primary {
      background: rgba(66, 133, 244, 0.09);
      color: #3977d9;
      border-color: rgba(66, 133, 244, 0.25);
    }
    .google-button-icon {
      width: 17px;
      height: 17px;
    }
    .google-spinning {
      animation: google-spin 1s linear infinite;
    }
  }
  .google-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .google-status {
    margin: 0;
    padding: 10px 12px;
    border-radius: 6px;
    background: rgba(66, 133, 244, 0.07);
    line-height: 1.6;
    overflow-wrap: anywhere;
    &.google-error {
      color: #c5553d;
      background: rgba(234, 67, 53, 0.07);
    }
  }
  .google-auth-link {
    display: flex;
    gap: 8px;
    .google-input {
      flex: 1;
    }
  }
  .google-model-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .google-models {
    display: flex;
    flex-direction: column;
    gap: 8px;
    .google-model {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 13px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      cursor: pointer;
      &.google-model-selected {
        border-color: #4285f4;
        background: rgba(66, 133, 244, 0.04);
      }
      .google-model-info {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        gap: 5px;
        overflow-wrap: anywhere;
      }
      .google-model-quota {
        display: flex;
        flex-direction: column;
        gap: 6px;
        width: 255px;
        flex-shrink: 0;

      }
    }
  }
  .google-empty {
    padding: 28px 14px;
    text-align: center;
    color: var(--color-text-muted);
    border: 1px dashed var(--color-line);
    border-radius: 7px;
  }
  .google-model-note {
    margin: -8px 0 0;
  }
  .google-quota-track {
    height: 4px;
    border-radius: 4px;
    background: rgba(128, 128, 128, 0.12);
    overflow: hidden;
    .google-quota-fill {
      display: block;
      height: 100%;
      background: #34a853;
      &.google-quota-low {
        background: #ea783d;
      }
    }
  }
  .google-groups {
    display: flex;
    flex-direction: column;
    gap: 10px;
    .google-group {
      display: flex;
      flex-direction: column;
      gap: 14px;
      padding: 14px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      .google-bucket {
        display: flex;
        flex-direction: column;
        gap: 6px;
        .google-bucket-heading {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 12px;
        }
      }
    }
  }
  .google-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding-top: 15px;
    border-top: 1px solid var(--color-line);
    .google-button {
      flex-shrink: 0;
    }
  }
}
@keyframes google-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

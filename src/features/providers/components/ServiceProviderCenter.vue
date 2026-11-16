<template>
  <section class="service-center">
    <header class="center-header">
      <div class="header-title">
        <h2 class="page-title">服务商配置中心</h2>
        <p class="page-description">共享网关模式 · {{ data.baseUrl }}</p>
      </div>
      <div class="header-actions">
        <button class="btn" :disabled="busy" @click="refresh">刷新</button>
      </div>
    </header>

    <p class="notice">
      上游统一使用 OpenAI Responses 或 Chat，Agent
      的协议由本地网关转换。迁移只复制配置；启用新模式后，为各 Agent
      单独应用配置。修改已接管的配置后，需要解除并重新应用。
    </p>
    <div class="provider-toolbar">
      <h3 class="section-title">服务商配置</h3>
      <button class="btn btn-primary" :disabled="busy" @click="editProvider()">
        新增服务商
      </button>
    </div>
    <div class="provider-list">
      <article
        v-for="provider in data.providers"
        :key="provider.id"
        class="provider-card"
      >
        <div class="card-title">
          <span>{{ provider.name }}</span>
          <span class="muted"
            >{{
              provider.kind === "codex_account"
                ? "Codex 官方账号"
                : provider.upstreamProtocol
            }}
            · {{ provider.enabled ? "启用" : "禁用" }} ·
            {{ provider.hasCredential ? "已配置凭据" : "缺少凭据" }}</span
          >
        </div>
        <p class="provider-url">
          {{
            provider.kind === "codex_account"
              ? "官方账号凭据由网关管理"
              : provider.baseUrl
          }}
        </p>
        <p class="muted">
          授权：{{ enabledAgents(provider.id) || "尚未授权" }} · 模型
          {{ providerModels(provider.id).length }} 个
        </p>
        <div class="card-actions">
          <button class="btn" :disabled="busy" @click="editProvider(provider)">
            编辑
          </button>
          <button
            class="btn"
            :disabled="busy"
            @click="
              perform('fetch-models', {
                providerId: provider.id,
                agentVersion: agentVersion('codex')
              })
            "
          >
            获取模型
          </button>
          <button
            class="btn"
            :disabled="busy || data.backend !== 'service'"
            @click="openApply(provider)"
          >
            应用 / 测试 Agent
          </button>
          <template v-if="provider.kind === 'codex_account'">
            <button
              class="btn"
              :disabled="busy || data.backend !== 'service'"
              @click="
                perform('login', {
                  accountId: provider.id,
                  proxy: provider.proxy
                })
              "
            >
              官方登录
            </button>
            <button
              class="btn"
              :disabled="busy"
              @click="perform('refresh-quota', { providerId: provider.id })"
            >
              刷新官方额度
            </button>
            <button
              class="btn"
              :disabled="busy"
              @click="((accountProvider = provider.id), (accountContent = ''))"
            >
              导入 auth.json
            </button>
          </template>
          <button
            class="btn"
            :disabled="busy"
            @click="deleteProvider = provider"
          >
            删除
          </button>
        </div>
        <details v-if="provider.kind === 'codex_account'" class="quota-report">
          <summary>官方额度（独立于网关用量）</summary>
          <pre class="quota-json">{{
            JSON.stringify(
              data.accounts?.find((item) => item.providerId === provider.id)
                ?.quota || {},
              null,
              2
            )
          }}</pre>
        </details>
        <div v-if="provider.kind === 'api'" class="key-list">
          <div
            v-for="key in data.keys.filter(
              (item) => item.providerId === provider.id
            )"
            :key="key.id"
            class="key-row"
          >
            <span
              >{{ key.name || key.logicalId }} · {{ key.fingerprint }} ·
              {{ key.status }} {{ key.isActive ? "（使用中）" : "" }}</span
            >
            <button
              class="btn"
              :disabled="busy || key.isActive || key.status !== 'active'"
              @click="
                perform('key-select', { id: key.id, providerId: provider.id })
              "
            >
              使用此 Key
            </button>
            <button
              class="btn"
              :disabled="busy || key.status === 'revoked'"
              @click="
                perform('key-revoke', { id: key.id, providerId: provider.id })
              "
            >
              撤销
            </button>
          </div>
        </div>
      </article>
      <p v-if="!data.providers?.length" class="muted">
        暂无服务商，请新建服务商。
      </p>
    </div>

    <h3 class="section-title">Agent 实例与文件接管</h3>
    <div class="instance-list">
      <article
        v-for="instance in visibleInstances"
        :key="instance.id"
        class="instance-card"
      >
        <div class="instance-description">
          <span
            >{{ instance.name }} · {{ agentLabel(instance.agentType) }} ·
            {{ instance.status }} / {{ instance.syncStatus }}</span
          >
          <code>{{ instance.configPath }}</code>
          <span v-if="instance.lastError" class="error-text">{{
            instance.lastError
          }}</span>
        </div>
        <button
          class="btn"
          :disabled="busy || instance.status === 'detached'"
          @click="perform('detach', { id: instance.id })"
        >
          解除接管 / 重试恢复
        </button>
        <div
          v-for="file in data.managedFiles.filter(
            (item) =>
              item.instanceId === instance.id &&
              item.restoreStatus === 'conflict'
          )"
          :key="file.id"
          class="file-conflict"
        >
          <span>{{ file.filePath }} · {{ file.errorMessage }}</span>
          <button class="btn" :disabled="busy" @click="showBackup(file.id)">
            查看加密备份
          </button>
        </div>
      </article>
    </div>
    <p v-if="loginState?.status" class="notice">
      官方登录：{{ loginState.status }} {{ loginState.message }}
      <button
        v-if="loginState.status === 'pending'"
        class="btn"
        @click="perform('cancel-login')"
      >
        取消登录
      </button>
    </p>
    <details class="migration-report">
      <summary>
        最近迁移报告（{{ data.migrationItems?.length || 0 }} 项）
      </summary>
      <p
        v-for="item in data.migrationItems"
        :key="`${item.sourceTable}:${item.sourceId}`"
        class="report-row"
      >
        {{ item.sourceId }} · {{ item.status }} · {{ item.reason }}
      </p>
    </details>
    <details class="migration-report">
      <summary>兼容性测试记录（工具调用 → 工具结果 → 流式文本）</summary>
      <p v-for="item in data.compatibility" :key="item.id" class="report-row">
        {{ providerName(item.providerId) }} · {{ agentLabel(item.agentType) }} ·
        修订 {{ item.providerRevision }} · {{ item.status }} ·
        {{ formatTime(item.testedAt) }}
      </p>
    </details>

    <section class="usage-panel">
      <h3 class="section-title">网关统计</h3>
      <div class="usage-counters">
        <div v-for="item in counters" :key="item.label" class="counter">
          <span>{{ item.label }}</span
          ><span>{{ item.value ?? "未知" }}</span>
        </div>
      </div>
      <p class="muted">
        只累计共享网关收到的请求。上游未返回用量时保留未知；缓存输入包含在输入
        Token 中。官方账号额度与此处调用量分别记录。
      </p>
      <label class="usage-dimension"
        >汇总维度<select v-model="usageDimension" class="input">
          <option
            v-for="item in usageDimensions"
            :key="item.id"
            :value="item.id"
          >
            {{ item.label }}
          </option>
        </select></label
      >
      <div class="usage-table-wrap">
        <table class="usage-table">
          <thead>
            <tr>
              <th class="usage-cell">分组</th>
              <th class="usage-cell">上游尝试 / 成功</th>
              <th class="usage-cell">输入 / 输出 / 缓存 Token（已知值）</th>
              <th class="usage-cell">未知用量</th>
              <th class="usage-cell">估算费用 / 已计价次数</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in data.usage?.[usageDimension]"
              :key="`${item.groupKey}:${item.currency}:${item.groupName}`"
            >
              <td class="usage-cell">
                {{
                  usageDimension === "byAgent"
                    ? agentLabel(item.groupName)
                    : item.groupName || item.groupKey
                }}
              </td>
              <td class="usage-cell">
                {{ item.attemptCount }} / {{ item.successCount }}
              </td>
              <td class="usage-cell">
                {{ item.inputTokens ?? "未知" }} /
                {{ item.outputTokens ?? "未知" }} /
                {{ item.cachedInputTokens ?? "未知" }}
              </td>
              <td class="usage-cell">{{ item.unknownUsageCount }}</td>
              <td class="usage-cell">
                {{
                  item.estimatedCostMicros == null
                    ? "未知"
                    : `${item.estimatedCostMicros / 1000000} ${item.currency}`
                }}
                / {{ item.pricedCount }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p class="muted">
        以下展示最近 100 条请求；汇总使用全部账本记录。不同币种分别展示，部分
        Token 或价格缺失时费用保持未知。
      </p>
      <div class="usage-table-wrap">
        <table class="usage-table">
          <thead>
            <tr>
              <th class="usage-cell">时间 / Agent</th>
              <th class="usage-cell">服务商 / Key</th>
              <th class="usage-cell">模型</th>
              <th class="usage-cell">结果</th>
              <th class="usage-cell">输入 / 输出 Token</th>
              <th class="usage-cell">估算费用</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="log in data.usage?.logs" :key="log.id">
              <td class="usage-cell">
                {{ formatTime(log.createdAt) }}<br />{{
                  agentLabel(log.agentType)
                }}
              </td>
              <td class="usage-cell">
                {{ log.providerName || "未路由" }}<br />{{ log.credentialName }}
              </td>
              <td class="usage-cell">
                {{ log.upstreamModel || log.requestedModel }}
              </td>
              <td class="usage-cell" :title="log.errorMessage">
                {{ log.state }} / {{ log.statusCode || "—" }}
              </td>
              <td class="usage-cell">
                {{ log.inputTokens ?? "未知" }} /
                {{ log.outputTokens ?? "未知" }}
              </td>
              <td class="usage-cell">
                {{
                  log.estimatedCostMicros == null
                    ? "未知"
                    : `${log.estimatedCostMicros / 1000000} ${log.currency}`
                }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <BaseModal
      v-if="form"
      title="服务商配置"
      description="上游地址填写 API 基础地址；密钥留空会保留现有值。"
      @close="form = null"
    >
      <form class="provider-form" @submit.prevent="saveProvider">
        <label class="form-field"
          >名称<input v-model="form.name" class="input" required
        /></label>
        <label class="form-field"
          >类型<select
            v-model="form.kind"
            class="input"
            :disabled="Boolean(form.id)"
          >
            <option value="api">OpenAI 兼容服务商</option>
            <option value="codex_account">Codex 官方账号</option>
          </select></label
        >
        <template v-if="form.kind === 'api'">
          <label class="form-field"
            >上游协议<select v-model="form.upstreamProtocol" class="input">
              <option value="responses">Responses</option>
              <option value="chat">Chat Completions</option>
            </select></label
          >
          <label class="form-field"
            >基础地址<input
              v-model="form.baseUrl"
              class="input"
              placeholder="https://example.com/v1"
              required
          /></label>
          <label class="form-field"
            >API Key<ProviderApiKeyInput v-model="form.apiKey"
          /></label>
          <label class="form-field"
            >Key 标识 / 名称<input
              v-model="form.keyLogicalId"
              class="input"
              placeholder="default" /><input
              v-model="form.keyName"
              class="input"
              placeholder="默认 Key"
          /></label>
        </template>
        <label class="form-field"
          >代理地址<input
            v-model="form.proxy"
            class="input"
            placeholder="可选，例如 http://127.0.0.1:7890"
        /></label>
        <label class="form-field"
          >备注<input v-model="form.note" class="input"
        /></label>
        <label class="check-field"
          ><input v-model="form.enabled" type="checkbox" />启用此服务商</label
        >
        <fieldset class="agent-fields">
          <legend>允许使用的 Agent</legend>
          <div
            v-for="binding in form.bindings"
            :key="binding.agentType"
            class="agent-binding"
          >
            <label class="check-field"
              ><input v-model="binding.enabled" type="checkbox" />{{
                agentLabel(binding.agentType)
              }}</label
            >
            <label v-if="binding.enabled" class="form-field"
              >默认模型<select v-model="binding.defaultModelId" class="input">
                <option :value="null">应用时选择</option>
                <option
                  v-for="model in providerModels(form.id).filter(
                    (item) => item.enabled
                  )"
                  :key="model.id"
                  :value="model.id"
                >
                  {{ model.displayName || model.upstreamModelId }}
                </option>
              </select></label
            >
          </div>
          <p v-if="!form.id" class="muted">
            新服务商保存模型后，可以再次编辑并选择默认模型。
          </p>
        </fieldset>
        <label class="form-field"
          >模型 JSON（可填模型名数组，或编辑模型详情）<textarea
            v-model="form.modelsJson"
            class="input code-input"
            rows="8"
            placeholder='["model-name"]'
          ></textarea>
        </label>
        <p class="muted">
          详情字段：upstreamModelId、displayName、enabled、contextWindow、catalog、inputPriceMicros、cachedInputPriceMicros、outputPriceMicros、currency。价格单位为每百万
          Token 的货币微单位；未知填 null。已有模型设 enabled=false 可禁用。
        </p>
        <details>
          <summary>高级配置：请求头、Agent 参数和模型别名</summary>
          <label class="form-field"
            >请求头 JSON<textarea
              v-model="form.headersJson"
              class="input code-input"
              rows="3"
            ></textarea>
          </label>
          <label
            v-for="binding in form.bindings"
            :key="binding.agentType"
            class="form-field"
            >{{ agentLabel(binding.agentType) }} 参数 JSON<textarea
              v-model="binding.optionsJson"
              class="input code-input"
              rows="3"
            ></textarea>
          </label>
          <label class="form-field"
            >模型别名 JSON<textarea
              v-model="form.routesJson"
              class="input code-input"
              rows="4"
              placeholder='[{"agentType":"claude","clientModelName":"sonnet","modelId":"已有模型ID"}]'
            ></textarea>
          </label>
        </details>
        <button class="btn btn-primary" :disabled="busy">保存配置</button>
      </form>
    </BaseModal>

    <BaseModal
      v-if="applyForm"
      title="应用服务商到 Agent"
      description="接管前保存原始文件。默认配置目录来自应用设置；可填写独立目录来配置其他实例。"
      @close="applyForm = null"
    >
      <div class="provider-form">
        <label class="form-field"
          >Agent<select
            v-model="applyForm.agentType"
            class="input"
            @change="updateAgent"
          >
            <option
              v-for="agent in allowedApplyAgents"
              :key="agent.id"
              :value="agent.id"
            >
              {{ agent.label }}
            </option>
          </select></label
        >
        <label class="form-field"
          >模型<select v-model="applyForm.modelId" class="input">
            <option
              v-for="model in providerModels(applyForm.providerId).filter(
                (item) => item.enabled
              )"
              :key="model.id"
              :value="model.id"
            >
              {{ model.displayName }} · {{ model.upstreamModelId }}
            </option>
          </select></label
        >
        <label
          v-if="!['claude-desktop', 'gateway'].includes(applyForm.agentType)"
          class="form-field"
          >配置目录<input v-model="applyForm.configPath" class="input"
        /></label>
        <label class="form-field"
          >实例名称<input v-model="applyForm.name" class="input"
        /></label>
        <p v-if="applyForm.agentType === 'claude-desktop'" class="muted">
          Desktop 会接管当前用户的普通 / 3P 配置和 Profile
          索引。应用后重启客户端。
        </p>
        <div class="card-actions">
          <button
            class="btn"
            :disabled="busy || !applyForm.modelId"
            @click="
              perform('test', {
                ...applyForm,
                agentVersion: agentVersion(applyForm.agentType)
              })
            "
          >
            测试工具与流式兼容
          </button>
          <button
            v-if="applyForm.agentType === 'gateway'"
            class="btn btn-primary"
            :disabled="busy || !applyForm.modelId"
            @click="perform('issue-token', applyForm)"
          >
            生成调用 Token
          </button>
          <button
            v-else
            class="btn btn-primary"
            :disabled="busy || !applyForm.modelId"
            @click="applyProvider"
          >
            备份并应用
          </button>
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-if="createdToken"
      title="多协议调用 Token"
      description="Token 仅在本次显示，请自行保存。它只允许使用当前服务商已启用的模型；可在实例列表撤销。"
      @close="createdToken = null"
    >
      <div class="provider-form">
        <p>{{ createdToken.baseUrl }}</p>
        <textarea
          class="input code-input"
          :value="createdToken.token"
          rows="3"
          readonly
        ></textarea>
        <p class="muted">
          Chat：/v1/chat/completions · Responses：/v1/responses ·
          Claude：/v1/messages · Gemini：/v1beta/models/模型名:generateContent
        </p>
      </div>
    </BaseModal>
    <BaseModal
      v-if="accountProvider"
      title="导入官方账号"
      description="粘贴官方 auth.json，凭据会加密写入新表。"
      @close="accountProvider = ''"
    >
      <div class="provider-form">
        <textarea
          v-model="accountContent"
          class="input code-input"
          rows="12"
          autocomplete="off"
          spellcheck="false"
        ></textarea
        ><button
          class="btn btn-primary"
          :disabled="busy || !accountContent.trim()"
          @click="importAccount"
        >
          导入账号
        </button>
      </div>
    </BaseModal>
    <BaseModal
      v-if="deleteProvider"
      title="删除服务商"
      :description="`将软删除 ${deleteProvider.name}。使用中的实例需要先解除接管，统计和旧表记录会保留。`"
      @close="deleteProvider = null"
    >
      <div class="provider-form">
        <button class="btn" :disabled="busy" @click="removeProvider">
          确认删除
        </button>
      </div>
    </BaseModal>
    <BaseModal
      v-if="backup"
      title="文件恢复对照"
      description="内容可能包含本地网关 Token，请妥善保管。手动恢复原始内容后可重试解除接管。"
      @close="backup = null"
    >
      <div class="provider-form">
        <p>{{ backup.path }}</p>
        <label class="form-field"
          >接管前（{{
            backup.beforeExists ? "原文件存在" : "原文件不存在，应删除接管文件"
          }}）<textarea
            class="input code-input"
            :value="backup.before"
            rows="10"
            readonly
          ></textarea></label
        ><label class="form-field"
          >本程序写入内容<textarea
            class="input code-input"
            :value="backup.after"
            rows="10"
            readonly
          ></textarea>
        </label>
      </div>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onMounted, ref, watch } from "vue"
import BaseModal from "@/components/BaseModal.vue"
import ProviderApiKeyInput from "./ProviderApiKeyInput.vue"
import { serviceProviderApi } from "@/api/modules/service-providers"
import { createMessage } from "@/utils/message"

const props = defineProps({
  initialState: { type: Object, default: () => ({}) },
  cliTargets: { type: Array, default: () => [] },
  loginState: { type: Object, default: null }
})
const data = ref({
  providers: [],
  models: [],
  bindings: [],
  keys: [],
  instances: [],
  managedFiles: [],
  ...props.initialState
})
const busy = ref(false)
const form = ref(null)
const applyForm = ref(null)
const accountProvider = ref("")
const accountContent = ref("")
const deleteProvider = ref(null)
const backup = ref(null)
const createdToken = ref(null)
const usageDimension = ref("byProvider")
const usageDimensions = [
  { id: "byProvider", label: "服务商" },
  { id: "byAgent", label: "Agent" },
  { id: "byModel", label: "模型" },
  { id: "byKey", label: "上游 Key / 官方凭据版本" },
  { id: "byToken", label: "调用 Token" },
  { id: "byDay", label: "日期（本地时区）" }
]
const agents = [
  { id: "codex", label: "Codex" },
  { id: "claude", label: "Claude Code" },
  { id: "claude-desktop", label: "Claude Desktop" },
  { id: "gateway", label: "多协议工具" }
]
const visibleInstances = computed(() =>
  data.value.instances.filter(
    (item) =>
      !item.configPath.startsWith("test:") &&
      !item.configPath.startsWith("internal:")
  )
)
const allowedApplyAgents = computed(() =>
  agents.filter((agent) =>
    data.value.bindings.some(
      (binding) =>
        binding.providerId === applyForm.value?.providerId &&
        binding.agentType === agent.id &&
        binding.enabled
    )
  )
)
const counters = computed(() => [
  { label: "入站请求", value: data.value.usage?.requests?.requestCount },
  { label: "成功", value: data.value.usage?.requests?.successCount ?? 0 },
  {
    label: "失败 / 拒绝",
    value: data.value.usage?.requests?.failureCount ?? 0
  },
  { label: "未确认结果", value: data.value.usage?.requests?.unknownCount ?? 0 },
  {
    label: "客户端取消",
    value: data.value.usage?.requests?.cancelledCount ?? 0
  },
  { label: "上游尝试", value: data.value.usage?.attempts?.attemptCount },
  { label: "已确认用量", value: data.value.usage?.attempts?.measuredCount }
])

// 后端事件与页面刷新都使用同一公开状态，凭据不会进入列表。
watch(
  () => props.initialState,
  (value) => {
    if (value?.providers) data.value = value
  },
  { deep: true }
)
onMounted(refresh)
const agentLabel = (id) =>
  agents.find((item) => item.id === id)?.label || id || "未鉴权"
const agentVersion = (id) =>
  props.cliTargets.find((item) => item.id === id)?.version || ""
const providerName = (id) =>
  data.value.providers.find((item) => item.id === id)?.name || id
const providerModels = (id) =>
  data.value.models.filter((item) => item.providerId === id)
const enabledAgents = (id) =>
  data.value.bindings
    .filter((item) => item.providerId === id && item.enabled)
    .map((item) => agentLabel(item.agentType))
    .join("、")
const formatTime = (value) =>
  value ? new Date(value).toLocaleString("zh-CN", { hour12: false }) : "—"

async function refresh() {
  return perform("state", undefined, false)
}

async function perform(action, payload, notify = true) {
  if (busy.value) return false
  busy.value = true
  try {
    const result = await serviceProviderApi.invoke(action, payload)
    if (action === "issue-token") createdToken.value = result
    data.value = await serviceProviderApi.invoke("state")
    if (notify) createMessage.success("操作已完成")
    return true
  } catch (error) {
    createMessage.error(error?.message || String(error))
    // 回滚失败也会更新冲突状态，重新读取后给出可处理的文件。
    if (action !== "state") {
      const result = await serviceProviderApi.invoke("state").catch(() => null)
      if (result) data.value = result
    }
    return false
  } finally {
    busy.value = false
  }
}

async function editProvider(provider = {}) {
  try {
    const detail = provider.id
      ? await serviceProviderApi.invoke("detail", { id: provider.id })
      : {}
    form.value = {
      name: "",
      kind: "api",
      upstreamProtocol: "responses",
      baseUrl: "",
      proxy: "",
      note: "",
      enabled: true,
      ...provider,
      ...detail,
      enabled: provider.enabled !== 0,
      apiKey: "",
      keyLogicalId: "default",
      keyName: "默认 Key",
      modelsJson: JSON.stringify(
        providerModels(provider.id).map((item) => ({
          ...item,
          enabled: Boolean(item.enabled)
        })),
        null,
        2
      ),
      headersJson: JSON.stringify(detail.headers || {}, null, 2),
      routesJson: JSON.stringify(
        data.value.routes
          ?.filter((item) => item.providerId === provider.id)
          .map((item) => ({ ...item, enabled: Boolean(item.enabled) })) || [],
        null,
        2
      ),
      bindings: agents.map((agent) => {
        const binding = data.value.bindings.find(
          (item) =>
            item.providerId === provider.id && item.agentType === agent.id
        )
        return {
          agentType: agent.id,
          enabled: Boolean(binding?.enabled),
          defaultModelId: binding?.defaultModelId,
          optionsJson: JSON.stringify(binding?.options || {}, null, 2)
        }
      })
    }
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

async function saveProvider() {
  try {
    const value = {
      ...form.value,
      upstreamProtocol:
        form.value.kind === "codex_account"
          ? "responses"
          : form.value.upstreamProtocol,
      models: JSON.parse(form.value.modelsJson),
      headers: JSON.parse(form.value.headersJson),
      routes: JSON.parse(form.value.routesJson),
      bindings: form.value.bindings.map((item) => ({
        agentType: item.agentType,
        enabled: item.enabled,
        defaultModelId: item.defaultModelId || null,
        options: JSON.parse(item.optionsJson)
      }))
    }
    if (!Array.isArray(value.models) || !Array.isArray(value.routes))
      throw new Error("模型和别名必须为 JSON 数组")
    if (await perform("save", value)) form.value = null
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

function openApply(provider) {
  const agent = agents.find((item) =>
    data.value.bindings.some(
      (binding) =>
        binding.providerId === provider.id &&
        binding.agentType === item.id &&
        binding.enabled
    )
  )
  if (!agent) return createMessage.error("请先编辑服务商并授权 Agent")
  applyForm.value = {
    providerId: provider.id,
    agentType: agent.id,
    modelId: "",
    configPath: "",
    name: ""
  }
  updateAgent()
}

function updateAgent() {
  const binding = data.value.bindings.find(
    (item) =>
      item.providerId === applyForm.value.providerId &&
      item.agentType === applyForm.value.agentType
  )
  applyForm.value.modelId =
    binding?.defaultModelId ||
    providerModels(applyForm.value.providerId).find((item) => item.enabled)
      ?.id ||
    ""
  applyForm.value.configPath =
    props.cliTargets.find((item) => item.id === applyForm.value.agentType)
      ?.configPath || ""
  applyForm.value.name = agentLabel(applyForm.value.agentType)
}

async function applyProvider() {
  if (await perform("activate", applyForm.value)) applyForm.value = null
}

async function importAccount() {
  if (
    await perform("import-account", {
      id: accountProvider.value,
      content: accountContent.value
    })
  ) {
    accountProvider.value = ""
    accountContent.value = ""
  }
}

async function removeProvider() {
  if (
    await perform("delete", {
      id: deleteProvider.value.id,
      revision: deleteProvider.value.revision
    })
  )
    deleteProvider.value = null
}

async function showBackup(id) {
  try {
    backup.value = await serviceProviderApi.invoke("file-backup", { id })
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}
</script>

<style scoped lang="less">
.service-center {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 24px;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 7px 12px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    &:disabled {
      opacity: 0.5;
      cursor: default;
    }
    &.btn-primary {
      background: var(--color-primary-solid);
      color: white;
      border-color: var(--color-primary-solid);
    }
  }
  .input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: 13px;
  }
  .center-header {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    .header-title {
      .page-title {
        margin: 0;
      }
      .page-description {
        color: var(--color-text-muted);
        margin-bottom: 0;
      }
    }
    .header-actions {
      display: flex;
      gap: 8px;
      align-items: center;
      flex-wrap: wrap;
    }
  }
  .notice {
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    margin: 0;
  }
  .provider-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .section-title {
    margin: 0;
    font-size: 16px;
  }
  .muted {
    color: var(--color-text-muted);
    font-size: 13px;
  }
  .provider-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    .provider-card {
      padding: 16px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      .card-title {
        display: flex;
        align-items: center;
        gap: 16px;
      }
      .provider-url {
        overflow-wrap: anywhere;
      }
      .quota-report {
        margin-top: 12px;
        .quota-json {
          white-space: pre-wrap;
          overflow-wrap: anywhere;
          font-size: 12px;
        }
      }
      .card-actions {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
      }
      .key-list {
        margin-top: 12px;
        .key-row {
          display: flex;
          align-items: center;
          gap: 10px;
          padding-top: 8px;
          font-size: 13px;
        }
      }
    }
  }
  .instance-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    .instance-card {
      display: flex;
      align-items: flex-start;
      flex-wrap: wrap;
      gap: 12px;
      padding: 14px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      .instance-description {
        display: flex;
        flex: 1;
        flex-direction: column;
        gap: 8px;
        min-width: 200px;
        overflow-wrap: anywhere;
      }
      .file-conflict {
        display: flex;
        width: 100%;
        justify-content: space-between;
        align-items: center;
        gap: 10px;
      }
      .error-text {
        color: var(--color-danger);
        font-size: 13px;
      }
    }
  }
  .migration-report {
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    .report-row {
      font-size: 13px;
      overflow-wrap: anywhere;
    }
  }
  .usage-panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    .usage-counters {
      display: flex;
      gap: 12px;
      flex-wrap: wrap;
      .counter {
        display: flex;
        flex-direction: column;
        gap: 8px;
        min-width: 110px;
        padding: 14px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
      }
    }
    .usage-dimension {
      display: flex;
      align-items: center;
      gap: 12px;
      .input {
        width: 240px;
      }
    }
    .usage-table-wrap {
      overflow: auto;
      .usage-table {
        width: 100%;
        border-collapse: collapse;
        white-space: nowrap;
        .usage-cell {
          padding: 12px;
          text-align: left;
          border-bottom: 1px solid var(--color-line);
          font-size: 13px;
        }
      }
    }
  }
  .provider-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px 0 0;
    min-height: 0;
    overflow: auto;
    .form-field {
      display: flex;
      flex-direction: column;
      gap: 8px;
      font-size: 13px;
    }
    .check-field {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      margin-right: 16px;
    }
    .agent-fields {
      border: 1px solid var(--color-line);
      border-radius: 8px;
      padding: 12px;
    }
    .code-input {
      font-family: monospace;
      resize: vertical;
    }
    .muted {
      margin: 0;
    }
    .card-actions {
      display: flex;
      gap: 10px;
    }
  }
}
</style>

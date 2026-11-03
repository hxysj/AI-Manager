<template>
  <section class="gateway-tool">
    <header class="gateway-service">
      <div class="service-title">
        <ServerCog :size="25" />
        <div class="service-description">
          <h2 class="service-name">统一模型网关</h2>
          <p class="service-address">{{ gateway.baseUrl || 'http://127.0.0.1:8046' }}</p>
        </div>
        <span class="service-state" :class="{ active: gateway.enabled }">{{ gateway.enabled ? '已开启' : '已关闭' }}</span>
      </div>
      <div class="service-actions">
        <button class="tool-button" type="button" :disabled="busy" @click="load"><RefreshCw :size="14" />刷新</button>
        <button class="tool-button primary" type="button" :disabled="busy || loading" @click="toggleGateway"><Power :size="14" />{{ gateway.enabled ? '关闭网关' : '开启网关' }}</button>
      </div>
    </header>
    <p v-if="loadError" class="gateway-error" role="alert">{{ loadError }}</p>
    <section class="gateway-metrics">
      <article v-for="metric in metrics" :key="metric.label" class="metric-item">
        <span class="metric-label">{{ metric.label }}</span><span class="metric-value">{{ metric.value }}</span>
      </article>
    </section>
    <nav class="gateway-tabs" aria-label="反代管理">
      <button v-for="tab in tabs" :key="tab.id" class="tab-button" :class="{ active: activeTab === tab.id }" type="button" @click="activeTab = tab.id">{{ tab.label }}</button>
    </nav>
    <section v-if="activeTab === 'config'" class="gateway-content">
      <form class="gateway-panel" @submit.prevent="saveConfig">
        <div class="panel-heading"><h3 class="panel-title">监听配置</h3><button class="tool-button primary" type="submit" :disabled="busy || loading"><Save :size="14" />保存配置</button></div>
        <div class="field-row">
          <label class="form-field"><span class="field-label">监听 IP</span><input v-model.trim="draft.host" class="field-input" required placeholder="127.0.0.1" /><span class="field-help">本机使用 127.0.0.1；局域网使用 0.0.0.0。</span></label>
          <label class="form-field"><span class="field-label">端口</span><input v-model.number="draft.port" class="field-input" required type="number" min="1" max="65535" /></label>
        </div>
        <div class="panel-heading"><h3 class="panel-title">底层池</h3><span class="panel-detail">只转发给选中的目标；空池不接收生成请求。</span></div>
        <div class="target-list">
          <label v-for="target in targets" :key="target.id" class="target-item" :class="{ selected: draft.providerIds.includes(target.id) }">
            <input v-model="draft.providerIds" class="target-checkbox" type="checkbox" :value="target.id" />
            <span class="target-info"><span class="target-name">{{ target.name }}</span><span class="target-type">{{ target.type }}</span></span>
          </label>
          <p v-if="!targets.length" class="empty-state">请先在 Providers 中添加 Provider、Codex 或 Gemini 官方账号。</p>
        </div>
      </form>
      <section class="gateway-panel">
        <div class="panel-heading"><h3 class="panel-title">模型路由</h3><span class="panel-detail">为外部模型名指定目标与真实模型，添加后保存生效。</span></div>
        <form class="mapping-form" @submit.prevent="addMapping">
          <input v-model.trim="mapping.alias" class="field-input" required placeholder="外部模型名，例如 my-model" />
          <select v-model="mapping.targetId" class="field-input" required><option value="">选择池中目标</option><option v-for="target in selectedTargets" :key="target.id" :value="target.id">{{ target.name }}</option></select>
          <input v-model.trim="mapping.model" class="field-input" required placeholder="真实模型名" />
          <button class="tool-button" type="submit"><Plus :size="14" />添加</button>
        </form>
        <div class="mapping-list">
          <div v-for="(rule, alias) in draft.modelMappings" :key="alias" class="mapping-item"><code class="model-code">{{ alias }}</code><ArrowRight :size="14" /><span class="mapping-target">{{ targetName(rule.targetId) }}</span><code class="model-code">{{ rule.model }}</code><button class="tool-button danger" type="button" :aria-label="`移除 ${alias} 路由`" @click="delete draft.modelMappings[alias]"><Trash2 :size="14" /></button></div>
          <p v-if="!Object.keys(draft.modelMappings).length" class="empty-state">没有别名规则时，按池中已配置的模型名匹配目标。</p>
        </div>
      </section>
      <section class="gateway-panel">
        <div class="panel-heading"><h3 class="panel-title">接入地址</h3><span class="panel-detail">所有模型接口均需调用 Token。</span></div>
        <div class="endpoint-list"><div v-for="route in routes" :key="route.path" class="endpoint-item"><span class="endpoint-method">{{ route.method }}</span><code class="endpoint-path">{{ route.path }}</code><span class="endpoint-label">{{ route.name }}</span><button class="tool-button" type="button" :aria-label="`复制 ${route.name} 地址`" @click="copyText(`${gateway.baseUrl}${route.path}`)"><Copy :size="13" /></button></div></div>
        <p class="panel-note">认证：Authorization: Bearer &lt;调用 Token&gt;。Claude 可用 x-api-key，Gemini 可用 x-goog-api-key。</p>
        <p class="panel-note">跨协议支持文本和函数工具调用；图片、思考内容及其他专属字段请使用同协议目标。请求需携带完整历史。</p>
        <div class="model-list"><code v-for="model in gateway.models || []" :key="model.id" class="model-pill">{{ model.id }}</code></div>
      </section>
    </section>
    <section v-else-if="activeTab === 'tokens'" class="gateway-content">
      <section class="gateway-panel">
        <div class="panel-heading"><h3 class="panel-title">调用 Token</h3><span class="panel-detail">独立于底层 Provider 密钥，可单独停用。</span></div>
        <form class="token-form" @submit.prevent="createToken">
          <input v-model.trim="tokenDraft.name" class="field-input" required maxlength="80" placeholder="调用方名称" />
          <input v-model.trim="tokenDraft.description" class="field-input" maxlength="200" placeholder="用途说明" />
          <input v-model="tokenDraft.expiresAt" class="field-input" type="datetime-local" aria-label="到期时间，留空永久有效" />
          <button class="tool-button primary" type="submit" :disabled="busy"><Plus :size="14" />创建 Token</button>
        </form>
        <div v-if="createdToken" class="created-token" role="status"><span class="created-note">明文仅在创建后显示，请复制保存。</span><code class="created-value">{{ createdToken }}</code><button class="tool-button" type="button" @click="copyText(createdToken)"><Copy :size="14" />复制</button><button class="tool-button" type="button" @click="createdToken = ''">收起</button></div>
        <div class="token-list">
          <div v-for="token in gateway.tokens || []" :key="token.id" class="token-item">
            <div class="token-info"><span class="token-name">{{ token.name }}</span><span class="token-detail">{{ token.tokenHint }} · {{ token.description || '无说明' }} · {{ token.expiresAt ? formatDateTime(token.expiresAt) + ' 到期' : '永久有效' }}</span></div>
            <div class="token-stats"><span class="token-detail">{{ token.requestCount || 0 }} 请求 / {{ token.failureCount || 0 }} 失败</span><span class="token-detail">输入 {{ formatTokenCount(token.inputTokens) }} / 输出 {{ formatTokenCount(token.outputTokens) }}</span></div>
            <button class="tool-button" type="button" :disabled="busy" @click="setTokenEnabled(token)">{{ token.enabled ? '停用' : '启用' }}</button>
            <button class="tool-button danger" type="button" :disabled="busy" :aria-label="`删除 ${token.name}`" @click="deletingToken = token"><Trash2 :size="14" /></button>
          </div>
          <p v-if="!gateway.tokens?.length" class="empty-state">创建第一个 Token 后即可从外部调用。</p>
        </div>
      </section>
    </section>
    <section v-else class="gateway-content">
      <section class="gateway-panel">
        <div class="panel-heading"><h3 class="panel-title">用量归属</h3><span class="panel-detail">明细保留最近 5000 条，调用 Token 总计持续累加。</span></div>
        <div class="usage-groups"><div v-for="group in usageGroups" :key="group.label" class="usage-group"><h4 class="group-label">{{ group.label }}</h4><div v-for="item in group.items" :key="item.key" class="usage-item"><span class="usage-name">{{ item.name || item.key }}</span><span class="usage-amount">{{ item.requestCount }} 次 · {{ formatTokenCount(item.inputTokens + item.outputTokens) }} Token</span></div></div></div>
      </section>
      <section class="gateway-panel">
        <div class="panel-heading"><h3 class="panel-title">最近请求</h3><span class="panel-detail">记录请求模型、路由模型与上游报告模型。</span></div>
        <div class="request-table-wrap"><table class="request-table"><thead class="request-head"><tr><th>时间 / 调用方</th><th>协议</th><th>请求 → 路由 → 上游报告</th><th>最终目标</th><th>输入 / 输出</th><th>状态 / 耗时</th></tr></thead><tbody class="request-body"><tr v-for="log in gateway.logs || []" :key="log.id" class="request-row"><td>{{ formatDateTime(log.createdAt) }}<span class="cell-detail">{{ log.tokenName }}</span></td><td>{{ log.protocol }}</td><td><code class="cell-model">{{ log.requestedModel || '—' }}</code><span class="cell-detail">→ {{ log.physicalModel || '—' }}</span><span class="cell-detail">上游：{{ log.upstreamModel || '未报告' }}</span></td><td>{{ log.providerName || '未路由' }}<span class="cell-detail">{{ log.providerId }}</span></td><td>{{ log.usageKnown ? `${log.inputTokens} / ${log.outputTokens}` : '未报告' }}</td><td><span class="request-status" :class="{ failed: !log.ok }">{{ log.ok ? '成功' : '失败' }} · {{ log.statusCode }}</span><span class="cell-detail">{{ log.latencyMs }} ms</span><span v-if="log.errorMessage" class="cell-error">{{ log.errorMessage }}</span></td></tr></tbody></table><p v-if="!gateway.logs?.length" class="empty-state">暂无请求记录。</p></div>
      </section>
    </section>
    <BaseModal v-if="deletingToken" title="删除调用 Token" :description="`删除“${deletingToken.name}”后，使用它的客户端会立即失去访问权限。`" @close="deletingToken = null"><div class="delete-actions"><button class="tool-button" type="button" @click="deletingToken = null">取消</button><button class="tool-button danger" type="button" :disabled="busy" @click="deleteToken">确认删除</button></div></BaseModal>
  </section>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { ArrowRight, Copy, Plus, Power, RefreshCw, Save, ServerCog, Trash2 } from 'lucide-vue-next'
import BaseModal from '@/components/BaseModal.vue'
import { multiProtocolGatewayApi } from '@/api'
import { formatDateTime, formatTokenCount } from '@/utils/formatters'
import { createMessage } from '@/utils/message'

const props = defineProps({
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] }
})
const tabs = [{ id: 'config', label: '配置与模型' }, { id: 'tokens', label: '调用 Token' }, { id: 'logs', label: '用量与追踪' }]
const routes = [
  { method: 'POST', path: '/v1/chat/completions', name: 'OpenAI Chat' },
  { method: 'POST', path: '/v1/responses', name: 'OpenAI Responses' },
  { method: 'POST', path: '/v1/messages', name: 'Anthropic Messages' },
  { method: 'POST', path: '/v1beta/models/{model}:generateContent', name: 'Gemini' },
  { method: 'POST', path: '/v1beta/models/{model}:streamGenerateContent?alt=sse', name: 'Gemini 流式' },
  { method: 'GET', path: '/v1/models', name: '模型列表' }
]
const gateway = ref({})
const activeTab = ref('config')
const loading = ref(true)
const busy = ref(false)
const loadError = ref('')
const createdToken = ref('')
const deletingToken = ref(null)
const draft = reactive({ host: '127.0.0.1', port: 8046, providerIds: [], modelMappings: {} })
const mapping = reactive({ alias: '', targetId: '', model: '' })
const tokenDraft = reactive({ name: '', description: '', expiresAt: '' })
const targets = computed(() => [
  ...props.providers.filter(item => item.enabled !== false).map(item => ({ id: item.id, name: item.name || item.id, type: item.type === 'google-account' ? 'Gemini 官方账号' : `${item.cli || item.type} Provider` })),
  ...props.codexAccounts.filter(item => !item.disabled).map(item => ({ id: `account:${item.id}`, name: item.email || item.id, type: 'Codex 官方账号' }))
])
const selectedTargets = computed(() => targets.value.filter(item => draft.providerIds.includes(item.id)))
const metrics = computed(() => [
  { label: '累计请求', value: gateway.value.usage?.requestCount || 0 },
  { label: '失败请求', value: gateway.value.usage?.failureCount || 0 },
  { label: '输入 Token', value: formatTokenCount(gateway.value.usage?.inputTokens) },
  { label: '输出 Token', value: formatTokenCount(gateway.value.usage?.outputTokens) }
])
const usageGroups = computed(() => [
  { label: '调用方', items: gateway.value.byToken || [] },
  { label: '底层目标', items: gateway.value.byProvider || [] },
  { label: '真实模型', items: gateway.value.byModel || [] }
])

// 配置草稿只在读取和保存配置时回填，Token 操作不会覆盖尚未保存的路由。
function applyState(result, resetDraft = false) {
  gateway.value = result
  if (resetDraft) {
    draft.host = result.host
    draft.port = result.port
    draft.providerIds = [...result.providerIds]
    draft.modelMappings = structuredClone(result.modelMappings)
  }
}

async function load() {
  loading.value = true
  loadError.value = ''
  try {
    applyState(await multiProtocolGatewayApi.getState(), true)
  } catch (error) {
    loadError.value = error.message || String(error)
  } finally {
    loading.value = false
  }
}

async function perform(action, message, resetDraft = false) {
  if (busy.value) return null
  busy.value = true
  try {
    const result = await action()
    // 明文 Token 只保留在此工具窗口，不进入全局状态、广播或持久化草稿。
    const { createdToken: token, ...state } = result
    applyState(state, resetDraft)
    if (token) createdToken.value = token.token
    if (message) createMessage.success(message)
    return result
  } catch (error) {
    createMessage.error(error.message || String(error))
    return null
  } finally {
    busy.value = false
  }
}

function configPayload() {
  return { host: draft.host, port: draft.port, providerIds: [...draft.providerIds], modelMappings: JSON.parse(JSON.stringify(draft.modelMappings)) }
}

function saveConfig() {
  return perform(() => multiProtocolGatewayApi.saveConfig(configPayload()), '网关配置已保存', true)
}

function toggleGateway() {
  const wasEnabled = gateway.value.enabled
  return perform(() => wasEnabled ? multiProtocolGatewayApi.disable() : multiProtocolGatewayApi.enable(configPayload()), wasEnabled ? '网关已关闭' : '网关已启动', true)
}

function addMapping() {
  draft.modelMappings[mapping.alias] = { targetId: mapping.targetId, model: mapping.model }
  mapping.alias = ''
  mapping.model = ''
}

function targetName(id) {
  return targets.value.find(item => item.id === id)?.name || id
}

async function createToken() {
  const result = await perform(() => multiProtocolGatewayApi.createToken({ name: tokenDraft.name, description: tokenDraft.description, expiresAt: tokenDraft.expiresAt ? new Date(tokenDraft.expiresAt).getTime() : 0 }), '调用 Token 已创建')
  if (result) {
    tokenDraft.name = ''
    tokenDraft.description = ''
    tokenDraft.expiresAt = ''
  }
}

function setTokenEnabled(token) {
  return perform(() => multiProtocolGatewayApi.setTokenEnabled({ tokenId: token.id, enabled: !token.enabled }))
}

async function deleteToken() {
  const result = await perform(() => multiProtocolGatewayApi.deleteToken({ tokenId: deletingToken.value.id }), '调用 Token 已删除')
  if (result) deletingToken.value = null
}

async function copyText(value) {
  try {
    await navigator.clipboard.writeText(value)
    createMessage.success('已复制')
  } catch (error) {
    createMessage.error(error.message || '复制失败')
  }
}

onMounted(load)
</script>

<style scoped lang="less">
.gateway-tool {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  gap: 16px;
  overflow: auto;
  padding: 4px 4px 24px;
  color: var(--color-text);

  .tool-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    flex-shrink: 0;
    padding: 8px 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    white-space: nowrap;
    font-size: 12px;

    &.primary { color: #fff; border-color: #7c3aed; background: #7c3aed; }
    &.danger { color: #dc6471; }
    &:disabled { opacity: 0.5; cursor: wait; }
    &:hover:not(:disabled) { filter: brightness(0.95); }
  }

  .gateway-service {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 20px;
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);

    .service-title {
      display: flex;
      align-items: center;
      gap: 14px;
      color: #9b79eb;

      .service-description {
        .service-name { margin: 0 0 6px; color: var(--color-text); font-size: 18px; }
        .service-address { margin: 0; color: var(--color-text-muted); font-size: 12px; }
      }
      .service-state {
        padding: 5px 9px;
        border: 1px solid var(--color-line);
        border-radius: 20px;
        color: var(--color-text-muted);
        font-size: 11px;
        &.active { color: #43a883; background: rgba(67, 168, 131, 0.1); }
      }
    }
    .service-actions { display: flex; gap: 8px; }
  }
  .gateway-error { margin: 0; color: #dc6471; }
  .gateway-metrics {
    display: flex;
    gap: 12px;
    .metric-item {
      display: flex;
      flex: 1;
      min-width: 0;
      flex-direction: column;
      gap: 8px;
      padding: 16px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      .metric-label { font-size: 12px; color: var(--color-text-muted); }
      .metric-value { font-size: 20px; overflow-wrap: anywhere; }
    }
  }
  .gateway-tabs {
    display: flex;
    gap: 8px;
    border-bottom: 1px solid var(--color-line);
    .tab-button {
      padding: 10px 16px;
      border: 0;
      border-bottom: 2px solid transparent;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      &.active { color: #9b79eb; border-bottom-color: #9b79eb; }
    }
  }
  .gateway-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    .gateway-panel {
      padding: 20px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      .panel-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        margin-bottom: 16px;
        .panel-title { margin: 0; font-size: 15px; }
        .panel-detail { color: var(--color-text-muted); font-size: 12px; }
      }
      .field-input {
        width: 100%;
        min-width: 0;
        padding: 9px 11px;
        box-sizing: border-box;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        color: var(--color-text);
        background: var(--color-panel);
        outline: none;
        &:focus { border-color: #9b79eb; }
      }
      .field-row {
        display: flex;
        gap: 16px;
        margin-bottom: 24px;
        .form-field {
          display: flex;
          flex: 1;
          flex-direction: column;
          gap: 8px;
          .field-label { font-size: 12px; }
          .field-help { font-size: 11px; color: var(--color-text-muted); }
        }
      }
      .target-list {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        .target-item {
          display: flex;
          align-items: center;
          gap: 10px;
          width: 260px;
          padding: 12px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          cursor: pointer;
          &.selected { border-color: #9b79eb; background: rgba(155, 121, 235, 0.06); }
          .target-checkbox { accent-color: #7c3aed; }
          .target-info {
            display: flex;
            min-width: 0;
            flex-direction: column;
            gap: 4px;
            .target-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
            .target-type { color: var(--color-text-muted); font-size: 11px; }
          }
        }
      }
      .mapping-form, .token-form { display: flex; gap: 8px; margin-bottom: 16px; }
      .mapping-list {
        .mapping-item {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 10px 0;
          border-bottom: 1px solid var(--color-line);
          .model-code { color: #9b79eb; overflow-wrap: anywhere; }
          .mapping-target { flex: 1; font-size: 12px; }
        }
      }
      .endpoint-list {
        .endpoint-item {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 8px 0;
          .endpoint-method { width: 38px; color: #9b79eb; font-size: 11px; }
          .endpoint-path { flex: 1; font-size: 12px; }
          .endpoint-label { font-size: 11px; color: var(--color-text-muted); }
        }
      }
      .panel-note { margin: 14px 0; color: var(--color-text-muted); font-size: 12px; }
      .model-list {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        .model-pill { padding: 5px 8px; border: 1px solid var(--color-line); border-radius: 5px; font-size: 11px; color: #9b79eb; }
      }
      .created-token {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 10px;
        padding: 12px;
        margin-bottom: 16px;
        border: 1px solid #43a883;
        border-radius: 8px;
        .created-note { width: 100%; color: #43a883; font-size: 12px; }
        .created-value { flex: 1; overflow-wrap: anywhere; }
      }
      .token-list {
        .token-item {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 14px 0;
          border-bottom: 1px solid var(--color-line);
          .token-info {
            display: flex;
            flex: 1;
            min-width: 0;
            flex-direction: column;
            gap: 5px;
            .token-name { font-size: 13px; }
            .token-detail { font-size: 11px; color: var(--color-text-muted); overflow-wrap: anywhere; }
          }
          .token-stats {
            display: flex;
            flex-direction: column;
            gap: 5px;
            text-align: right;
            .token-detail { font-size: 11px; color: var(--color-text-muted); }
          }
        }
      }
      .usage-groups {
        display: flex;
        gap: 24px;
        .usage-group {
          flex: 1;
          min-width: 0;
          .group-label { margin: 0 0 10px; color: var(--color-text-muted); font-size: 12px; }
          .usage-item {
            display: flex;
            flex-direction: column;
            gap: 5px;
            margin-top: 10px;
            .usage-name { font-size: 12px; overflow-wrap: anywhere; }
            .usage-amount { color: var(--color-text-muted); font-size: 11px; }
          }
        }
      }
      .request-table-wrap {
        overflow-x: auto;
        .request-table {
          width: 100%;
          border-collapse: collapse;
          font-size: 11px;
          .request-head {
            :deep(th) { padding: 10px; border-bottom: 1px solid var(--color-line); text-align: left; }
          }
          .request-body {
            .request-row {
              :deep(td) { padding: 12px 10px; border-bottom: 1px solid var(--color-line); vertical-align: top; }
              .cell-detail { display: block; margin-top: 5px; color: var(--color-text-muted); overflow-wrap: anywhere; }
              .cell-model { color: #9b79eb; overflow-wrap: anywhere; }
              .cell-error { display: block; max-width: 240px; margin-top: 5px; color: #dc6471; overflow-wrap: anywhere; }
              .request-status { color: #43a883; &.failed { color: #dc6471; } }
            }
          }
        }
      }
      .empty-state { padding: 16px; margin: 0; color: var(--color-text-muted); font-size: 12px; }
    }
  }
  .delete-actions { display: flex; justify-content: flex-end; gap: 8px; }
}
</style>

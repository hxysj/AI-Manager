<template>
  <section class="service-center">
    <header class="provider-toolbar">
      <div class="provider-tabs" aria-label="服务商类型">
        <button
          v-for="item in providerTabs"
          :key="item.id"
          class="provider-tab"
          :class="{ 'provider-tab-active': providerTab === item.id }"
          type="button"
          :aria-pressed="providerTab === item.id"
          @click="providerTab = item.id"
        >
          <component :is="item.icon" :size="17" />
          {{ item.label }}
        </button>
      </div>
      <div class="toolbar-actions">
        <button
          class="toolbar-button"
          :class="{ 'toolbar-button-selected': showDisabledItems }"
          type="button"
          :title="showDisabledItems ? '隐藏禁用项' : '显示禁用项'"
          :aria-label="`${showDisabledItems ? '隐藏禁用项' : '显示禁用项'}，共 ${disabledProviderCount} 项`"
          :aria-pressed="showDisabledItems"
          @click="showDisabledItems = !showDisabledItems"
        >
          <Eye v-if="showDisabledItems" :size="17" />
          <EyeOff v-else :size="17" />
          <span v-if="disabledProviderCount" class="toolbar-count">
            {{ disabledProviderCount }}
          </span>
        </button>
        <button
          class="toolbar-button"
          type="button"
          title="刷新列表"
          aria-label="刷新列表"
          :disabled="busy"
          @click="refresh"
        >
          <RefreshCw :size="17" />
        </button>
        <button
          class="toolbar-button"
          type="button"
          title="网关与实例管理"
          aria-label="网关与实例管理"
          @click="showGatewayManager = true"
        >
          <Server :size="18" />
        </button>
        <button
          class="toolbar-button toolbar-add"
          type="button"
          title="新增服务商"
          aria-label="新增服务商"
          :disabled="busy"
          @click="editProvider()"
        >
          <Plus :size="22" />
        </button>
      </div>
    </header>

    <div class="provider-list">
      <p v-if="loginState?.status === 'pending'" class="notice">
        官方登录进行中：{{ loginState.message }}
        <button class="btn" :disabled="busy" @click="perform('cancel-login')">
          取消登录
        </button>
      </p>
      <article
        v-for="provider in visibleProviders"
        :key="provider.id"
        class="provider-card"
        :class="{
          'provider-card-active': providerInstances(provider.id).length,
          'provider-card-disabled': !provider.enabled
        }"
      >
        <span class="provider-avatar">
          <AiIcon
            v-if="provider.icon"
            :name="provider.icon"
            class="provider-avatar-icon"
            :alt="`${providerDisplayName(provider)} 图标`"
          />
          <ShieldCheck v-else-if="provider.kind === 'codex_account'" :size="18" />
          <template v-else>{{ providerDisplayName(provider).slice(0, 1) || "AI" }}</template>
        </span>
        <div class="provider-main">
          <div class="provider-title">
            <span
              class="provider-name"
              data-emphasis
              :title="providerDisplayName(provider)"
            >
              {{ providerDisplayName(provider) }}
            </span>
            <span class="provider-tag">
              {{
                provider.kind === "codex_account"
                  ? "官方账号"
                  : provider.upstreamProtocol === "chat"
                    ? "Chat"
                    : "Responses"
              }}
            </span>
            <span v-if="!provider.enabled" class="provider-tag disabled-tag"
              >已禁用</span
            >
            <span
              v-if="!provider.hasCredential"
              class="provider-tag missing-tag"
              >待配置凭据</span
            >
          </div>
          <span
            v-if="provider.note"
            class="provider-note"
            :title="provider.note"
            >{{ provider.note }}</span
          >
          <span class="provider-url" :title="provider.baseUrl">
            {{
              provider.kind === "codex_account"
                ? "Codex 官方账号"
                : provider.baseUrl
            }}
          </span>
          <div
            v-if="
              provider.kind === 'codex_account' && quotaWindows(provider).length
            "
            class="quota-list"
          >
            <div
              v-for="quota in quotaWindows(provider)"
              :key="quota.name"
              class="quota-item"
              :title="
                quota.resetAt
                  ? `重置时间：${formatTime(quota.resetAt * 1000)}`
                  : ''
              "
            >
              <div class="quota-heading">
                <span>{{ quota.name }}</span>
                <span>{{
                  quota.remaining == null ? "未知" : `${quota.remaining}%`
                }}</span>
              </div>
              <div class="quota-track">
                <span
                  class="quota-fill"
                  :style="{ width: `${quota.remaining ?? 0}%` }"
                ></span>
              </div>
            </div>
          </div>
          <span
            class="provider-meta"
            :title="
              enabledAgents(provider.id) || '请在编辑中选择允许使用的 Agent'
            "
          >
            {{ enabledAgents(provider.id) || "尚未授权 Agent" }} ·
            {{ providerModels(provider.id).length }} 个模型
          </span>
        </div>
        <div class="provider-actions">
          <div class="action-main">
            <span
              class="state-pill"
              :class="{
                'state-pill-muted':
                  !provider.enabled || !providerInstances(provider.id).length
              }"
            >
              <span
                v-if="provider.enabled && providerInstances(provider.id).length"
                class="state-dot"
              ></span>
              {{
                !provider.enabled
                  ? "已禁用"
                  : providerInstances(provider.id).length
                    ? "已启用"
                    : "未启用"
              }}
            </span>
            <button
              v-if="!provider.enabled"
              class="provider-enable"
              type="button"
              :disabled="busy"
              @click="toggleProviderEnabled(provider)"
            >
              <RefreshCw :size="15" />恢复
            </button>
            <button
              v-else-if="providerInstances(provider.id).length"
              class="provider-using"
              type="button"
              :disabled="busy"
              @click="stopUsingProvider(provider)"
            >
              <X :size="15" />取消使用
            </button>
            <button
              v-else
              class="provider-enable"
              type="button"
              :disabled="busy || !provider.hasCredential"
              @click="openApply(provider)"
            >
              <Play :size="15" />启用
            </button>
          </div>
          <div class="icon-actions">
            <button
              v-if="provider.kind === 'codex_account'"
              class="icon-button"
              type="button"
              title="刷新额度"
              aria-label="刷新额度"
              :disabled="busy"
              @click="perform('refresh-quota', { providerId: provider.id })"
            >
              <RefreshCw :size="16" />
            </button>
            <button
              class="icon-button"
              type="button"
              title="查看详情"
              aria-label="查看详情"
              @click="openProviderDetail(provider)"
            >
              <Eye :size="16" />
            </button>
            <button
              v-if="provider.kind === 'api'"
              class="icon-button"
              type="button"
              title="管理 API Key"
              aria-label="管理 API Key"
              @click="openProviderDetail(provider, 'keys')"
            >
              <KeyRound :size="16" />
            </button>
            <button
              class="icon-button"
              type="button"
              title="编辑服务商"
              aria-label="编辑服务商"
              :disabled="busy"
              @click="editProvider(provider)"
            >
              <SquarePen :size="16" />
            </button>
            <button
              v-if="provider.enabled"
              class="icon-button icon-warning"
              type="button"
              title="禁用服务商"
              aria-label="禁用服务商"
              :disabled="busy"
              @click="toggleProviderEnabled(provider)"
            >
              <Ban :size="16" />
            </button>
            <button
              class="icon-button icon-danger"
              type="button"
              title="删除服务商"
              aria-label="删除服务商"
              :disabled="busy"
              @click="deleteProvider = provider"
            >
              <Trash2 :size="16" />
            </button>
          </div>
        </div>
      </article>
      <div v-if="!visibleProviders.length" class="provider-empty">
        <span>{{
          disabledProviderCount && !showDisabledItems
            ? "当前服务商均已禁用，可点击右上角显示禁用项。"
            : "暂无服务商，点击右上角新增。"
        }}</span>
      </div>
    </div>

    <BaseModal
      v-if="detailProvider"
      :title="providerDisplayName(detailProvider)"
      description=""
      @close="detailProviderId = ''"
    >
      <div class="provider-detail">
        <div class="detail-tabs">
          <button
            v-for="item in detailTabs"
            :key="item.id"
            class="detail-tab"
            :class="{ 'detail-tab-active': detailTab === item.id }"
            type="button"
            @click="detailTab = item.id"
          >
            {{ item.label }}
          </button>
        </div>
        <template v-if="detailTab === 'overview'">
          <dl class="detail-fields">
            <div class="detail-field">
              <dt class="detail-label">服务商类型</dt>
              <dd class="detail-value">
                {{
                  detailProvider.kind === "codex_account"
                    ? "Codex 官方账号"
                    : "OpenAI 兼容服务商"
                }}
              </dd>
            </div>
            <div v-if="detailProvider.kind === 'api'" class="detail-field">
              <dt class="detail-label">基础地址</dt>
              <dd class="detail-value">{{ detailProvider.baseUrl }}</dd>
            </div>
            <div class="detail-field">
              <dt class="detail-label">允许使用</dt>
              <dd class="detail-value">
                {{ enabledAgents(detailProvider.id) || "尚未授权 Agent" }}
              </dd>
            </div>
            <div class="detail-field">
              <dt class="detail-label">凭据状态</dt>
              <dd class="detail-value">
                {{ detailProvider.hasCredential ? "已配置" : "待配置" }}
              </dd>
            </div>
            <div v-if="detailProvider.proxy" class="detail-field">
              <dt class="detail-label">代理地址</dt>
              <dd class="detail-value">{{ detailProvider.proxy }}</dd>
            </div>
          </dl>
          <div class="card-actions">
            <button
              class="btn"
              :disabled="busy"
              @click="
                perform('fetch-models', {
                  providerId: detailProvider.id,
                  agentVersion: agentVersion('codex')
                })
              "
            >
              获取模型
            </button>
            <button
              class="btn"
              :disabled="busy || !detailProvider.enabled"
              @click="openApply(detailProvider)"
            >
              应用 / 测试 Agent
            </button>
            <button
              class="btn"
              :disabled="busy"
              @click="editProvider(detailProvider)"
            >
              编辑配置与 Agent 授权
            </button>
          </div>
          <div class="detail-models">
            <h3 class="section-title">模型</h3>
            <span
              v-for="model in providerModels(detailProvider.id)"
              :key="model.id"
              class="model-tag"
              :class="{ 'model-tag-disabled': !model.enabled }"
              >{{ model.displayName || model.upstreamModelId }}</span
            >
            <p v-if="!providerModels(detailProvider.id).length" class="muted">
              尚未配置模型，可以获取模型或在编辑中添加。
            </p>
          </div>
          <template v-if="detailProvider.kind === 'codex_account'">
            <h3 class="section-title">官方账号</h3>
            <div class="card-actions">
              <button
                class="btn"
                :disabled="busy || data.backend !== 'service'"
                @click="
                  perform('login', {
                    accountId: detailProvider.id,
                    proxy: detailProvider.proxy
                  })
                "
              >
                官方登录
              </button>
              <button
                class="btn"
                :disabled="busy"
                @click="
                  perform('refresh-quota', { providerId: detailProvider.id })
                "
              >
                刷新官方额度
              </button>
              <button
                class="btn"
                :disabled="busy"
                @click="
                  ((accountProvider = detailProvider.id), (accountContent = ''))
                "
              >
                导入 auth.json
              </button>
            </div>
            <details class="quota-report">
              <summary>官方额度（独立于网关用量）</summary>
              <pre class="quota-json">{{
                JSON.stringify(
                  providerAccount(detailProvider.id)?.quota || {},
                  null,
                  2
                )
              }}</pre>
            </details>
          </template>
        </template>
        <div v-else-if="detailTab === 'keys'" class="key-list">
          <div class="card-actions">
            <button
              class="btn btn-primary"
              :disabled="busy"
              @click="editProvider(detailProvider)"
            >
              添加 / 更新 Key
            </button>
          </div>
          <div
            v-for="key in data.keys.filter(
              (item) => item.providerId === detailProvider.id
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
                perform('key-select', {
                  id: key.id,
                  providerId: detailProvider.id
                })
              "
            >
              使用此 Key
            </button>
            <button
              class="btn"
              :disabled="busy || key.status === 'revoked'"
              @click="
                perform('key-revoke', {
                  id: key.id,
                  providerId: detailProvider.id
                })
              "
            >
              撤销
            </button>
          </div>
          <p
            v-if="
              !data.keys.some((item) => item.providerId === detailProvider.id)
            "
            class="muted"
          >
            尚未添加 API Key。
          </p>
        </div>
        <div v-else class="instance-list">
          <article
            v-for="instance in providerInstances(detailProvider.id)"
            :key="instance.id"
            class="instance-card"
          >
            <div class="instance-description">
              <span
                >{{ instance.name }} ·
                {{ agentLabel(instance.agentType) }}</span
              ><code>{{ instance.configPath }}</code
              ><span v-if="instance.lastError" class="error-text">{{
                instance.lastError
              }}</span>
            </div>
            <button
              class="btn"
              :disabled="busy"
              @click="perform('detach', { id: instance.id })"
            >
              取消使用 / 恢复配置
            </button>
          </article>
          <p v-if="!providerInstances(detailProvider.id).length" class="muted">
            此服务商尚未应用到 Agent。
          </p>
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-if="showGatewayManager"
      title="网关与实例管理"
      :description="data.baseUrl"
      @close="showGatewayManager = false"
    >
      <div class="gateway-manager">
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
          <p
            v-for="item in data.compatibility"
            :key="item.id"
            class="report-row"
          >
            {{ providerName(item.providerId) }} ·
            {{ agentLabel(item.agentType) }} · 修订
            {{ item.providerRevision }} · {{ item.status }} ·
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
            以下展示最近 100
            条请求；汇总使用全部账本记录。不同币种分别展示，部分 Token
            或价格缺失时费用保持未知。
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
                    {{ log.providerName || "未路由" }}<br />{{
                      log.credentialName
                    }}
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
      </div>
    </BaseModal>

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
import {
  Ban,
  Eye,
  EyeOff,
  KeyRound,
  Layers,
  Play,
  Plus,
  RefreshCw,
  Server,
  ShieldCheck,
  SquarePen,
  Trash2,
  X
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
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
const providerTab = ref("all")
const showDisabledItems = ref(false)
const showGatewayManager = ref(false)
const detailProviderId = ref("")
const detailTab = ref("overview")
const providerTabs = [
  { id: "all", label: "全部", icon: Layers },
  { id: "api", label: "服务商", icon: Server },
  { id: "codex_account", label: "官方账号", icon: ShieldCheck }
]
// 列表按配置类型展示，Agent 授权保留在编辑中，避免重新按 Agent 拆分配置。
const scopedProviders = computed(() =>
  data.value.providers.filter(
    (item) => providerTab.value === "all" || item.kind === providerTab.value
  )
)
const disabledProviderCount = computed(
  () => scopedProviders.value.filter((item) => !item.enabled).length
)
const visibleProviders = computed(() =>
  scopedProviders.value.filter(
    (item) => showDisabledItems.value || item.enabled
  )
)
const detailProvider = computed(() =>
  data.value.providers.find((item) => item.id === detailProviderId.value)
)
const detailTabs = computed(() => [
  { id: "overview", label: "基本信息" },
  ...(detailProvider.value?.kind === "api"
    ? [{ id: "keys", label: "API Key" }]
    : []),
  { id: "instances", label: "已启用实例" }
])
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
const providerAccount = (id) =>
  data.value.accounts?.find((item) => item.providerId === id)
const providerDisplayName = (provider) =>
  provider.kind === "codex_account" &&
  (!provider.name || provider.name === provider.id)
    ? providerAccount(provider.id)?.email || "Codex 官方账号"
    : provider.name
const providerModels = (id) =>
  data.value.models.filter((item) => item.providerId === id)
const providerInstances = (id) =>
  visibleInstances.value.filter(
    (item) => item.providerId === id && item.status === "active"
  )
const enabledAgents = (id) =>
  data.value.bindings
    .filter((item) => item.providerId === id && item.enabled)
    .map((item) => agentLabel(item.agentType))
    .join("、")
const formatTime = (value) =>
  value ? new Date(value).toLocaleString("zh-CN", { hour12: false }) : "—"

function quotaWindows(provider) {
  const limits = providerAccount(provider.id)?.quota?.rate_limit
  return [limits?.primary_window, limits?.secondary_window]
    .filter(Boolean)
    .map((window, index) => ({
      name:
        window.limit_window_seconds >= 86400
          ? `${window.limit_window_seconds / 86400} 天额度`
          : window.limit_window_seconds
            ? `${window.limit_window_seconds / 3600} 小时额度`
            : `额度 ${index + 1}`,
      remaining:
        window.used_percent == null
          ? null
          : Math.max(0, Math.min(100, 100 - window.used_percent)),
      resetAt: window.reset_at
    }))
}

function openProviderDetail(provider, tab = "overview") {
  detailProviderId.value = provider.id
  detailTab.value = tab
}

async function toggleProviderEnabled(provider) {
  if (busy.value) return false
  busy.value = true
  let detail
  try {
    detail = await serviceProviderApi.invoke("detail", { id: provider.id })
  } catch (error) {
    createMessage.error(error?.message || String(error))
    return false
  } finally {
    busy.value = false
  }
  // 只更新配置启用状态；模型、授权和 Key 由后端保留，使用详情中的最新修订号。
  return perform("save", { ...provider, ...detail, enabled: !provider.enabled })
}

async function stopUsingProvider(provider) {
  const instances = providerInstances(provider.id)
  if (instances.length === 1) return perform("detach", { id: instances[0].id })
  openProviderDetail(provider, "instances")
}

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
      enabled: Boolean(detail.enabled ?? provider.enabled ?? true),
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
    detailProviderId.value = ""
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
  detailProviderId.value = ""
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
  height: 100%;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--color-panel);
  :deep(.base-modal__panel) {
    max-width: calc(100vw - 48px);
  }
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
  .provider-toolbar {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 58px;
    padding: 0 14px;
    border-bottom: 1px solid var(--color-line);
    .provider-tabs {
      display: flex;
      gap: 4px;
      padding: 4px;
      border-radius: 12px;
      background: var(--color-panel-soft);
      .provider-tab {
        display: inline-flex;
        align-items: center;
        gap: 7px;
        height: 38px;
        padding: 0 16px;
        border: 0;
        border-radius: 10px;
        background: transparent;
        color: var(--color-text-muted);
        font: inherit;
        cursor: pointer;
        white-space: nowrap;
        &.provider-tab-active {
          background: var(--color-panel);
          color: var(--color-text);
          box-shadow: 0 1px 5px rgba(15, 23, 42, 0.08);
        }
      }
    }
    .toolbar-actions {
      display: flex;
      flex: none;
      gap: 8px;
      align-items: center;
      .toolbar-button {
        position: relative;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 38px;
        height: 38px;
        padding: 0;
        border: 1px solid var(--color-line);
        border-radius: 12px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        &:disabled {
          opacity: 0.55;
          cursor: default;
        }
        &:hover {
          background: var(--color-panel-soft);
          color: var(--color-text);
        }
        &.toolbar-button-selected {
          border-color: var(--color-warning-line);
          background: var(--color-warning-soft);
          color: var(--color-warning);
        }
        &.toolbar-add {
          border-color: var(--color-primary-solid);
          background: var(--color-primary-solid);
          color: #fff;
        }
        .toolbar-count {
          position: absolute;
          top: -6px;
          right: -6px;
          min-width: 18px;
          height: 18px;
          padding: 0 4px;
          border: 2px solid var(--color-panel);
          border-radius: 999px;
          background: var(--color-primary-solid);
          color: #fff;
          font-size: var(--font-size-xs);
          line-height: 14px;
        }
      }
    }
  }
  .notice {
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    margin: 0;
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
    flex: 1;
    min-height: 0;
    padding: 14px;
    flex-direction: column;
    gap: 10px;
    overflow-x: hidden;
    overflow-y: auto;
    .provider-card {
      display: flex;
      flex: none;
      align-items: center;
      gap: 14px;
      padding: 18px 18px 16px 12px;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel);
      box-shadow: 0 3px 12px rgba(15, 23, 42, 0.06);
      transition:
        border-color 0.18s ease,
        background 0.18s ease,
        box-shadow 0.18s ease;
      &:hover {
        border-color: var(--color-info-line);
        box-shadow: 0 10px 24px rgba(15, 23, 42, 0.1);
      }
      &.provider-card-active {
        border-color: var(--color-info-line);
        background: var(--color-primary-soft);
        box-shadow:
          0 8px 22px rgba(22, 130, 255, 0.12),
          inset 4px 0 0 var(--color-primary-solid);
      }
      &.provider-card-disabled {
        border-color: var(--color-line-strong);
        background: var(--color-panel-soft);
        box-shadow: none;
        opacity: 0.72;
      }
      .provider-avatar {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 32px;
        border: 1px solid var(--color-line);
        border-radius: 12px;
        background: var(--color-panel-soft);
        color: var(--color-text);
        .provider-avatar-icon {
          width: 22px;
          height: 22px;
        }
      }
      .provider-main {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        gap: 8px;
        .provider-title {
          display: flex;
          min-width: 0;
          align-items: center;
          gap: 8px;
          .provider-name {
            overflow: hidden;
            min-width: 0;
            font-size: var(--font-size-lg);
            text-overflow: ellipsis;
            white-space: nowrap;
          }
          .provider-tag {
            flex: none;
            padding: 1px 8px;
            border: 1px solid var(--color-success-line);
            border-radius: 999px;
            background: var(--color-success-soft);
            color: var(--color-success);
            font-size: var(--font-size-xs);
            line-height: 1.4;
            &.disabled-tag {
              border-color: var(--color-line);
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
            }
            &.missing-tag {
              border-color: var(--color-warning-line);
              background: var(--color-warning-soft);
              color: var(--color-warning);
            }
          }
        }
        .provider-note,
        .provider-url,
        .provider-meta {
          overflow: hidden;
          color: var(--color-text-muted);
          font-size: var(--font-size-base);
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .provider-url {
          color: var(--color-primary);
        }
        .provider-meta {
          font-size: var(--font-size-xs);
        }
        .quota-list {
          display: flex;
          gap: 12px;
          .quota-item {
            display: flex;
            flex: 1;
            min-width: 0;
            flex-direction: column;
            gap: 5px;
            .quota-heading {
              display: flex;
              justify-content: space-between;
              gap: 8px;
              color: var(--color-text-muted);
              font-size: var(--font-size-xs);
            }
            .quota-track {
              height: 4px;
              overflow: hidden;
              border-radius: 999px;
              background: var(--color-line);
              .quota-fill {
                display: block;
                height: 100%;
                border-radius: inherit;
                background: var(--color-success);
              }
            }
          }
        }
      }
      .provider-actions {
        display: flex;
        flex: none;
        width: 272px;
        flex-direction: column;
        align-items: flex-end;
        gap: 12px;
        .action-main {
          display: flex;
          align-items: center;
          justify-content: flex-end;
          gap: 8px;
          min-height: 36px;
          .state-pill {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            height: 24px;
            padding: 0 10px;
            border-radius: 999px;
            background: var(--color-success-soft);
            color: var(--color-success);
            font-size: var(--font-size-sm);
            &.state-pill-muted {
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
            }
            .state-dot {
              width: 6px;
              height: 6px;
              border-radius: 999px;
              background: var(--color-success);
            }
          }
          .provider-enable,
          .provider-using {
            display: inline-flex;
            min-width: 134px;
            height: 36px;
            align-items: center;
            justify-content: center;
            gap: 6px;
            padding: 0 14px;
            border: 0;
            border-radius: 6px;
            background: var(--color-primary-solid);
            color: #fff;
            font: inherit;
            font-size: var(--font-size-sm);
            cursor: pointer;
            &:disabled {
              opacity: 0.55;
              cursor: default;
            }
          }
          .provider-using {
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
          }
        }
        .icon-actions {
          display: flex;
          align-items: center;
          gap: 18px;
          padding-right: 3px;
          .icon-button {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            padding: 2px;
            border: 0;
            border-radius: 4px;
            background: transparent;
            color: var(--color-text-muted);
            cursor: pointer;
            &:hover {
              color: var(--color-primary);
              background: var(--color-primary-soft);
            }
            &.icon-warning:hover {
              color: var(--color-warning);
              background: var(--color-warning-soft);
            }
            &.icon-danger:hover {
              color: var(--color-danger);
              background: var(--color-danger-soft);
            }
            &:disabled {
              opacity: 0.55;
              cursor: default;
            }
          }
        }
      }
    }
    .provider-empty {
      display: flex;
      min-height: 220px;
      align-items: center;
      justify-content: center;
      padding: 20px;
      border: 1px dashed var(--color-line);
      border-radius: 14px;
      color: var(--color-text-muted);
      text-align: center;
    }
  }
  .gateway-manager,
  .provider-detail {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 18px;
    padding-top: 12px;
    overflow: auto;
  }
  .card-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .provider-detail {
    .detail-tabs {
      display: flex;
      gap: 8px;
      border-bottom: 1px solid var(--color-line);
      .detail-tab {
        padding: 10px 14px;
        border: 0;
        border-bottom: 2px solid transparent;
        background: transparent;
        color: var(--color-text-muted);
        font: inherit;
        cursor: pointer;
        &.detail-tab-active {
          border-bottom-color: var(--color-primary);
          color: var(--color-primary);
        }
      }
    }
    .detail-fields {
      display: flex;
      flex-direction: column;
      gap: 14px;
      margin: 0;
      .detail-field {
        display: flex;
        gap: 16px;
        font-size: var(--font-size-base);
        .detail-label {
          flex: none;
          width: 80px;
          color: var(--color-text-muted);
        }
        .detail-value {
          min-width: 0;
          margin: 0;
          overflow-wrap: anywhere;
        }
      }
    }
    .detail-models {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      .section-title {
        width: 100%;
        margin-bottom: 4px;
      }
      .model-tag {
        padding: 5px 9px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        font-size: var(--font-size-sm);
        &.model-tag-disabled {
          color: var(--color-text-muted);
          opacity: 0.6;
        }
      }
    }
    .key-list {
      display: flex;
      flex-direction: column;
      gap: 12px;
      .key-row {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 12px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        font-size: var(--font-size-sm);
      }
    }
    .quota-report {
      .quota-json {
        white-space: pre-wrap;
        overflow-wrap: anywhere;
        font-size: var(--font-size-xs);
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

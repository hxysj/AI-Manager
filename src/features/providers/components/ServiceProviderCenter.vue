<template>
  <section class="service-center">
    <header class="provider-toolbar">
      <div class="provider-tabs" aria-label="Agent 与服务商">
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
          v-if="providerTab !== 'agents'"
          class="toolbar-button"
          :class="{ 'toolbar-button-selected': showDisabledItems }"
          type="button"
          :title="showDisabledItems ? '隐藏停用项' : '显示停用项'"
          :aria-label="`${showDisabledItems ? '隐藏停用项' : '显示停用项'}，共 ${disabledProviderCount} 项`"
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
          title="网关与 Agent 管理"
          aria-label="网关与 Agent 管理"
          @click="openGatewayManager()"
        >
          <Server :size="18" />
        </button>
        <button
          v-if="providerTab !== 'agents'"
          class="toolbar-button toolbar-add"
          type="button"
          title="新增服务商"
          aria-label="新增服务商"
          :disabled="busy"
          @click="showCreateOptions = true"
        >
          <Plus :size="22" />
        </button>
      </div>
    </header>

    <div v-if="providerTab === 'agents'" class="agent-list">
      <article
        v-for="agent in detectedAgents"
        :key="agent.id"
        class="agent-card"
      >
        <span class="agent-avatar">
          <AiIcon
            v-if="agent.icon"
            class="agent-icon"
            :name="agent.icon"
            :alt="agentLabel(agent.id)"
          />
          <Bot v-else :size="24" />
        </span>
        <div class="agent-main">
          <div class="agent-heading">
            <span class="agent-name" data-emphasis>{{
              agentLabel(agent.id)
            }}</span>
            <span class="agent-version">{{ agent.version || "已检测" }}</span>
          </div>
          <span class="agent-source">
            <span
              v-if="currentAgentInstance(agent.id)"
              class="agent-source-dot"
            ></span>
            {{ currentAgentSource(agent.id) }}
          </span>
          <span
            v-if="agent.configPath"
            class="agent-path"
            :title="agent.configPath"
          >
            {{ agent.configPath }}
          </span>
        </div>
        <button
          class="agent-config-button"
          type="button"
          :aria-label="`配置 ${agentLabel(agent.id)}`"
          :disabled="busy"
          @click="openAgentConfig(agent)"
        >
          <Settings2 :size="16" />
          配置
          <ChevronRight :size="15" />
        </button>
      </article>
      <div v-if="!detectedAgents.length" class="provider-empty">
        尚未检测到 Codex、Claude Code 或 Claude Desktop，请安装后刷新。
      </div>
    </div>

    <div v-else class="provider-list">
      <p v-if="loginState?.status === 'pending'" class="notice">
        官方登录进行中：{{ loginState.message }}
        <button class="btn" :disabled="busy" @click="perform('cancel-login')">
          取消登录
        </button>
      </p>
      <p v-if="googleLoginState?.status === 'pending'" class="notice">
        Google 登录进行中：{{ googleLoginState.message }}
        <button class="btn" :disabled="busy" @click="cancelGoogleLogin">
          取消登录
        </button>
      </p>
      <article
        v-for="provider in visibleProviders"
        :key="provider.id"
        class="provider-card"
        :class="{
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
          <ShieldCheck
            v-else-if="provider.kind === 'codex_account'"
            :size="18"
          />
          <template v-else>{{
            providerDisplayName(provider).slice(0, 1) || "AI"
          }}</template>
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
            <span
              v-for="agent in providerRunningAgents(provider.id)"
              :key="agent"
              class="running-agent-tag"
              :title="`${agentLabel(agent)} 正在使用此服务商`"
            >
              <span class="running-agent-dot"></span>
              {{ agentLabel(agent) }}
            </span>
            <span class="provider-tag">
              {{
                provider.kind === "codex_account"
                  ? "GPT 官方账号"
                  : provider.kind === "google_account"
                    ? "Google 账号"
                    : provider.upstreamProtocol === "chat"
                      ? "Chat"
                      : "Responses"
              }}
            </span>
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
              provider.kind !== "api"
                ? provider.kind === "google_account"
                  ? "Google 官方账号"
                  : "GPT / Codex 官方账号"
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
          <span class="provider-meta">
            {{
              provider.kind !== "api"
                ? "使用官方模型列表"
                : `${providerModels(provider.id).length} 个模型`
            }}
          </span>
        </div>
        <div class="provider-actions">
          <div class="icon-actions">
            <button
              v-if="provider.kind !== 'api'"
              class="icon-button"
              type="button"
              title="刷新额度"
              aria-label="刷新额度"
              :disabled="
                busy ||
                !provider.enabled ||
                !provider.hasCredential ||
                refreshingQuotaIds.has(provider.id)
              "
              @click="refreshQuota(provider)"
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
              class="icon-button icon-warning"
              type="button"
              :title="provider.enabled ? '停用服务商' : '恢复可用'"
              :aria-label="provider.enabled ? '停用服务商' : '恢复可用'"
              :disabled="busy"
              @click="toggleProviderEnabled(provider)"
            >
              <Ban v-if="provider.enabled" :size="16" />
              <RefreshCw v-else :size="16" />
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
                  detailProvider.kind !== "api"
                    ? detailProvider.kind === "google_account"
                      ? "Google 官方账号"
                      : "GPT / Codex 官方账号"
                    : "OpenAI 兼容服务商"
                }}
              </dd>
            </div>
            <div v-if="detailProvider.kind === 'api'" class="detail-field">
              <dt class="detail-label">基础地址</dt>
              <dd class="detail-value">{{ detailProvider.baseUrl }}</dd>
            </div>
            <div class="detail-field">
              <dt class="detail-label">正在使用的 Agent</dt>
              <dd class="detail-value">
                {{
                  providerRunningAgents(detailProvider.id)
                    .map(agentLabel)
                    .join("、") || "暂无 Agent 使用"
                }}
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
              v-if="detailProvider.kind === 'api'"
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
              :disabled="busy"
              @click="openGatewayManager(detailProvider)"
            >
              管理 Agent
            </button>
            <button
              class="btn"
              :disabled="busy"
              @click="editProvider(detailProvider)"
            >
              编辑配置
            </button>
          </div>
          <p v-if="detailProvider.kind !== 'api'" class="muted">
            使用官方模型列表
          </p>
          <div v-else class="detail-models">
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
                :disabled="
                  busy ||
                  !detailProvider.enabled ||
                  !detailProvider.hasCredential ||
                  refreshingQuotaIds.has(detailProvider.id)
                "
                @click="refreshQuota(detailProvider)"
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
          <template v-if="detailProvider.kind === 'google_account'">
            <h3 class="section-title">Google 官方账号</h3>
            <div class="card-actions">
              <button
                class="btn"
                :disabled="busy"
                @click="startGoogleLogin(detailProvider)"
              >
                Google 登录
              </button>
              <button
                class="btn"
                :disabled="
                  busy ||
                  !detailProvider.enabled ||
                  !detailProvider.hasCredential ||
                  refreshingQuotaIds.has(detailProvider.id)
                "
                @click="refreshQuota(detailProvider)"
              >
                刷新官方额度
              </button>
            </div>
            <p class="muted">
              {{
                googleAccount(detailProvider.id)?.metadata?.message ||
                "使用官方模型列表"
              }}
            </p>
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
              @click="openGatewayManager(detailProvider)"
            >
              管理接管
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
      title="网关与 Agent 管理"
      :description="data.baseUrl"
      @close="showGatewayManager = false"
    >
      <div class="gateway-manager">
        <div class="agent-apply">
          <label class="form-field"
            >服务商<select v-model="managerProviderId" class="input">
              <option value="" disabled>选择服务商</option>
              <option
                v-for="provider in data.providers"
                :key="provider.id"
                :value="provider.id"
              >
                {{ providerDisplayName(provider)
                }}{{ provider.enabled ? "" : "（已停用）" }}
              </option>
            </select></label
          >
          <button
            class="btn btn-primary"
            :disabled="
              busy ||
              !managerProvider?.enabled ||
              !managerProvider?.hasCredential
            "
            @click="openApply(managerProvider)"
          >
            应用 / 测试 Agent
          </button>
          <button
            class="btn"
            :disabled="
              busy ||
              !managerProvider?.enabled ||
              !managerProvider?.hasCredential
            "
            @click="openToolConfig(managerProvider)"
          >
            配置多协议工具
          </button>
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
              <span>{{ providerName(instance.providerId) }}</span>
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
      v-if="showCreateOptions"
      title="添加服务商或官方账号"
      description="选择来源类型，账号授权成功后可在 Agent 列表中使用。"
      @close="showCreateOptions = false"
    >
      <div class="create-options">
        <button
          v-for="source in createSources"
          :key="source.kind"
          class="create-option"
          type="button"
          :disabled="busy"
          @click="createSource(source)"
        >
          <AiIcon :name="source.icon" :alt="source.label" class="create-icon" />
          <span class="create-description">
            <span class="create-label">{{ source.label }}</span>
            <span class="create-hint">{{ source.hint }}</span>
          </span>
          <ChevronRight :size="18" />
        </button>
      </div>
    </BaseModal>

    <BaseModal
      v-if="form && editorSection === 'provider'"
      :title="form.id ? '编辑服务商' : '新增服务商'"
      description="配置上游服务与公共模型目录。"
      @close="form = null"
    >
      <form class="provider-form" @submit.prevent="saveProvider">
        <ProviderIconPicker v-model="form.icon" :name="form.name" />
        <div class="form-grid">
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
              <option value="google_account">Google 官方账号</option>
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
              >API Key<ProviderApiKeyInput
                v-model="form.apiKey"
                :placeholder="
                  form.hasCredential
                    ? '留空保留当前 Key，填写则更新当前 Key'
                    : '请输入 API Key'
                "
              /><span v-if="form.hasCredential" class="muted"
                >当前 Key：{{ form.keyName }}，留空保留。</span
              ></label
            >
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
        </div>
        <label class="check-field"
          ><input v-model="form.enabled" type="checkbox" />服务商可用</label
        >
        <template v-if="form.kind === 'api'">
          <ProviderModelListEditor
            :model-value="draftModelNames"
            title="服务商模型目录"
            :show-sync-control="false"
            :show-reset="false"
            :disabled="busy"
            @update:model-value="updateDraftModelNames"
            @fetch="fetchEditorModels"
          />
          <p class="muted">
            移除模型会将其停用，原有价格、模型详情与历史用量继续保留。
          </p>
        </template>
        <p v-else class="muted">使用官方模型列表</p>
        <p v-if="!form.id" class="muted">
          保存后，到“Agent 列表”中选择此服务商并配置模型。
        </p>
        <details class="advanced-fields">
          <summary>
            {{
              form.kind === "api"
                ? "高级配置：模型详情、请求头与模型别名"
                : "高级配置：请求头与模型别名"
            }}
          </summary>
          <div v-if="form.kind === 'api'" class="form-grid">
            <label class="form-field"
              >Key 标识<input
                v-model="form.keyLogicalId"
                class="input"
                placeholder="default"
            /></label>
            <label class="form-field"
              >Key 名称<input
                v-model="form.keyName"
                class="input"
                placeholder="默认 Key"
            /></label>
          </div>
          <template v-if="form.kind === 'api'">
            <label class="form-field"
              >模型详情 JSON<textarea
                v-model="form.modelsJson"
                class="input code-input"
                rows="8"
                placeholder='["model-name"]'
              ></textarea>
            </label>
            <p class="muted">
              可配置
              displayName、contextWindow、catalog、enabled、currency，以及
              inputPriceMicros、cachedInputPriceMicros、outputPriceMicros。
              价格单位为每百万 Token 的货币微单位，未知填 null。
            </p>
          </template>
          <label class="form-field"
            >请求头 JSON<textarea
              v-model="form.headersJson"
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
        <div class="form-footer">
          <button
            class="btn"
            type="button"
            :disabled="busy"
            @click="form = null"
          >
            取消
          </button>
          <button class="btn btn-primary" type="submit" :disabled="busy">
            {{ !form.id && form.kind !== "api" ? "保存并登录" : "保存配置" }}
          </button>
        </div>
      </form>
    </BaseModal>

    <BaseModal
      v-if="agentConfig"
      :title="`配置 ${agentLabel(agentConfig.id)}`"
      :description="
        agentConfig.id === 'gateway'
          ? '保存此服务商的工具授权后，可在网关管理中生成独立调用 Token。'
          : '选择当前使用的服务商或官方账号，保存后应用到此 Agent。'
      "
      @close="closeAgentConfig"
    >
      <form class="provider-form" @submit.prevent="saveAgentConfig">
        <label class="form-field">
          使用来源
          <select
            v-model="agentProviderId"
            class="input"
            :disabled="busy"
            @change="selectAgentProvider"
          >
            <option value="" :disabled="agentConfig.id === 'gateway'">
              {{
                agentConfig.id === "gateway"
                  ? "选择服务商"
                  : "停止使用，恢复原有配置"
              }}
            </option>
            <optgroup
              v-for="group in sourceGroups"
              :key="group.kind"
              :label="group.label"
            >
              <option
                v-for="provider in group.providers"
                :key="provider.id"
                :value="provider.id"
                :disabled="!provider.hasCredential"
              >
                {{ providerDisplayName(provider)
                }}{{ !provider.hasCredential ? "（待登录或配置凭据）" : "" }}
              </option>
            </optgroup>
          </select>
        </label>
        <p v-if="!agentProviderId" class="muted">
          选择服务商或官方账号后，可配置该 Agent 的模型规则。
        </p>
        <p v-else-if="form && form.kind !== 'api'" class="muted">
          使用官方模型列表
        </p>
        <ServiceAgentModelFields
          v-if="form && agentBindingIndex >= 0"
          v-model="form.bindings[agentBindingIndex]"
          :provider-kind="form.kind"
          :models="draftModels.filter((item) => item.enabled)"
          :schema="runtimeConfigSchemas[agentConfig.id] || {}"
          :desktop-routes="data.desktopModelRoutes || []"
          :disabled="busy"
        />
        <div class="form-footer">
          <button
            class="btn"
            type="button"
            :disabled="busy"
            @click="closeAgentConfig"
          >
            取消
          </button>
          <button
            class="btn btn-primary"
            type="submit"
            :disabled="
              busy ||
              Boolean(agentProviderId && !form) ||
              (agentConfig.id === 'gateway' && !agentProviderId)
            "
          >
            {{ agentConfig.id === "gateway" ? "保存授权" : "保存并应用" }}
          </button>
        </div>
      </form>
    </BaseModal>

    <BaseModal
      v-if="applyForm"
      title="应用服务商到 Agent"
      :description="`${providerName(applyForm.providerId)} · 接管前保存原始文件，默认配置目录来自应用设置。`"
      @close="closeApply"
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
          <button class="btn" :disabled="busy" @click="closeApply">
            返回管理
          </button>
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
import { computed, onMounted, onUnmounted, ref, watch } from "vue"
import {
  Ban,
  Bot,
  ChevronRight,
  Eye,
  EyeOff,
  KeyRound,
  Plus,
  RefreshCw,
  Server,
  Settings2,
  ShieldCheck,
  SquarePen,
  Trash2
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import BaseModal from "@/components/BaseModal.vue"
import ProviderApiKeyInput from "./ProviderApiKeyInput.vue"
import ProviderIconPicker from "./ProviderIconPicker.vue"
import ProviderModelListEditor from "./ProviderModelListEditor.vue"
import ServiceAgentModelFields from "./ServiceAgentModelFields.vue"
import { serviceProviderApi } from "@/api/modules/service-providers"
import { createMessage } from "@/utils/message"

const props = defineProps({
  initialState: { type: Object, default: () => ({}) },
  cliTargets: { type: Array, default: () => [] },
  runtimeConfigSchemas: { type: Object, default: () => ({}) },
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
const providerTab = ref("agents")
const showCreateOptions = ref(false)
const googleLoginState = ref(null)
const refreshingQuotaIds = ref(new Set())
let googleLoginTimer
let googleLoginRequest = 0
let officialQuotaTask
let disposed = false
let savedProviderId = ""
const createSources = [
  {
    kind: "api",
    label: "API 服务商",
    icon: "codex.svg",
    hint: "配置 OpenAI 兼容地址与 API Key"
  },
  {
    kind: "codex_account",
    label: "GPT / Codex 官方账号",
    icon: "codex.svg",
    hint: "官方登录，也支持导入 auth.json"
  },
  {
    kind: "google_account",
    label: "Google 账号",
    icon: "google.svg",
    hint: "Google 授权登录，使用官方模型与额度"
  }
]
const showDisabledItems = ref(false)
const showGatewayManager = ref(false)
const managerProviderId = ref("")
const managerProvider = computed(() =>
  data.value.providers.find((item) => item.id === managerProviderId.value)
)
const detailProviderId = ref("")
const detailTab = ref("overview")
const providerTabs = [
  { id: "agents", label: "Agent 列表", icon: Bot },
  { id: "api", label: "服务商", icon: Server },
  { id: "codex_account", label: "官方账号", icon: ShieldCheck }
]
// Agent 管理使用来源，服务商与官方账号各自维护连接和凭据。
const scopedProviders = computed(() =>
  data.value.providers.filter((item) =>
    providerTab.value === "api" ? item.kind === "api" : item.kind !== "api"
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
  { id: "instances", label: "关联 Agent" }
])
const form = ref(null)
const editorSection = ref("provider")
const agentConfig = ref(null)
const agentProviderId = ref("")
const sourceGroups = computed(() =>
  [
    { kind: "api", label: "服务商" },
    { kind: "codex_account", label: "GPT / Codex 官方账号" },
    { kind: "google_account", label: "Google 账号" }
  ]
    .map((group) => ({
      ...group,
      providers: data.value.providers.filter(
        (item) => item.enabled && item.kind === group.kind
      )
    }))
    .filter((group) => group.providers.length)
)
const detectedAgents = computed(() =>
  props.cliTargets.filter(
    (item) =>
      item.installed && ["codex", "claude", "claude-desktop"].includes(item.id)
  )
)
const agentBindingIndex = computed(
  () =>
    form.value?.bindings.findIndex(
      (item) => item.agentType === agentConfig.value?.id
    ) ?? -1
)
// 简单模型列表与高级详情共用一份草稿，避免编辑名称时丢失价格和目录扩展。
const draftModels = computed(() => {
  try {
    return parseDraftModels()
  } catch {
    return []
  }
})
const draftModelNames = computed(() =>
  draftModels.value
    .filter((item) => item.enabled)
    .map((item) => item.upstreamModelId)
    .join("\n")
)
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
// 进入官方账号列表才查询额度，普通状态刷新和 Agent 配置只读已有缓存。
watch(providerTab, (tab) => {
  if (tab === "codex_account") refreshOfficialQuotas()
})
onMounted(async () => {
  await refresh()
  if (disposed) return
  try {
    const request = googleLoginRequest
    const result = await serviceProviderApi.invoke("google-login-state")
    if (disposed || request !== googleLoginRequest) return
    googleLoginState.value = result
    if (googleLoginState.value?.status === "pending")
      googleLoginTimer = setTimeout(() => pollGoogleLogin(request), 1500)
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
})
const agentLabel = (id) =>
  agents.find((item) => item.id === id)?.label || id || "未鉴权"
const agentVersion = (id) =>
  props.cliTargets.find((item) => item.id === id)?.version || ""
const providerName = (id) =>
  data.value.providers.find((item) => item.id === id)?.name || id
const providerAccount = (id) =>
  data.value.accounts?.find((item) => item.providerId === id)
const googleAccount = (id) =>
  data.value.googleAccounts?.find((item) => item.providerId === id)
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
// 启动标识只取客户端真实接管结果，授权和内置会话不代表客户端已启用。
const currentAgentInstance = (agent) =>
  visibleInstances.value.find(
    (item) =>
      item.agentType === agent &&
      item.status === "active" &&
      data.value.providers.some(
        (provider) => provider.id === item.providerId && provider.enabled
      )
  )
const currentAgentSource = (agent) => {
  const instance = currentAgentInstance(agent)
  if (!instance) return "尚未选择使用来源"
  const provider = data.value.providers.find(
    (item) => item.id === instance.providerId
  )
  return `${providerDisplayName(provider)}${instance.syncStatus === "synced" ? "" : " · 待同步"}`
}
const providerRunningAgents = (id) => [
  ...new Set(
    visibleInstances.value
      .filter(
        (item) =>
          item.providerId === id &&
          ["codex", "claude", "claude-desktop"].includes(item.agentType) &&
          item.status === "active" &&
          item.syncStatus === "synced" &&
          data.value.providers.some(
            (provider) => provider.id === id && provider.enabled
          )
      )
      .map((item) => item.agentType)
  )
]
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

function openGatewayManager(provider) {
  managerProviderId.value =
    provider?.id ||
    managerProviderId.value ||
    data.value.providers.find((item) => item.enabled)?.id ||
    ""
  detailProviderId.value = ""
  showGatewayManager.value = true
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
  // 后端停用时撤销访问并恢复客户端文件，模型规则和凭据继续保留。
  return perform("save", {
    ...provider,
    ...detail,
    enabled: !Boolean(detail.enabled ?? provider.enabled)
  })
}

async function refresh() {
  const success = await perform(
    providerTab.value === "agents" ? "refresh-agents" : "state",
    undefined,
    false
  )
  if (success) await refreshOfficialQuotas()
  return success
}

async function refreshOfficialQuotas() {
  if (disposed || providerTab.value !== "codex_account") return
  if (officialQuotaTask) return officialQuotaTask
  officialQuotaTask = (async () => {
    for (const source of data.value.providers.filter(
      (item) => item.kind !== "api" && item.enabled && item.hasCredential
    )) {
      if (disposed || providerTab.value !== "codex_account") break
      const provider = data.value.providers.find(
        (item) => item.id === source.id
      )
      if (provider) await refreshQuota(provider, false)
    }
  })()
  try {
    await officialQuotaTask
  } finally {
    officialQuotaTask = null
  }
}

async function refreshQuota(provider, notify = true) {
  if (
    disposed ||
    !provider.enabled ||
    !provider.hasCredential ||
    refreshingQuotaIds.value.has(provider.id)
  )
    return false
  refreshingQuotaIds.value.add(provider.id)
  try {
    await serviceProviderApi.invoke("refresh-quota", {
      providerId: provider.id
    })
    if (!disposed) data.value = await serviceProviderApi.invoke("state")
    if (!disposed && notify) createMessage.success("官方额度已刷新")
    return true
  } catch (error) {
    if (!disposed)
      createMessage.error(
        `${providerDisplayName(provider)}：${error?.message || String(error)}`
      )
    return false
  } finally {
    refreshingQuotaIds.value.delete(provider.id)
  }
}

async function perform(action, payload, notify = true) {
  if (busy.value) return false
  busy.value = true
  try {
    const result = await serviceProviderApi.invoke(action, payload)
    if (action === "save") savedProviderId = result.id || ""
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

async function createSource(source) {
  showCreateOptions.value = false
  await editProvider({
    kind: source.kind,
    name: source.kind === "api" ? "" : source.label,
    icon: source.icon
  })
}

// 登录流程沿用已有 OAuth 实现，新模式通过独立入口存取新表。
async function startGoogleLogin(provider) {
  if (busy.value) return
  busy.value = true
  const request = ++googleLoginRequest
  clearTimeout(googleLoginTimer)
  try {
    const result = await serviceProviderApi.invoke("google-login", {
      providerId: provider.id,
      proxy: provider.proxy
    })
    if (disposed || request !== googleLoginRequest) return
    googleLoginState.value = result
    if (result?.status === "pending")
      googleLoginTimer = setTimeout(() => pollGoogleLogin(request), 1500)
  } catch (error) {
    createMessage.error(error?.message || String(error))
  } finally {
    busy.value = false
  }
}

async function pollGoogleLogin(request = googleLoginRequest) {
  if (disposed || request !== googleLoginRequest) return
  try {
    const result = await serviceProviderApi.invoke("google-login-state")
    if (disposed || request !== googleLoginRequest) return
    googleLoginState.value = result
    if (result.status === "pending") {
      googleLoginTimer = setTimeout(() => pollGoogleLogin(request), 1500)
      return
    }
    clearTimeout(googleLoginTimer)
    if (result.status === "success") {
      await refresh()
      createMessage.success(result.message || "Google 登录完成")
    } else if (result.status === "failed") {
      createMessage.error(result.message || "Google 登录失败")
    }
  } catch (error) {
    clearTimeout(googleLoginTimer)
    createMessage.error(error?.message || String(error))
  }
}

async function cancelGoogleLogin() {
  if (await perform("google-cancel-login")) {
    googleLoginRequest += 1
    clearTimeout(googleLoginTimer)
    googleLoginState.value = null
  }
}

onUnmounted(() => {
  disposed = true
  googleLoginRequest += 1
  clearTimeout(googleLoginTimer)
})

async function editProvider(provider = {}, section = "provider") {
  if (busy.value) return
  if (section === "provider") agentConfig.value = null
  if (section !== "provider" && !provider.enabled) return
  busy.value = true
  try {
    const detail = provider.id
      ? await serviceProviderApi.invoke("detail", { id: provider.id })
      : {}
    const activeKey = data.value.keys.find(
      (item) =>
        item.providerId === provider.id &&
        item.isActive &&
        item.status === "active"
    )
    form.value = {
      name: "",
      icon: "",
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
      keyLogicalId: activeKey?.logicalId || "default",
      keyName: activeKey?.name || "默认 Key",
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
        const options = JSON.parse(JSON.stringify(binding?.options || {}))
        // 保留各 Agent 独立参数，已有新模式配置沿用此前默认同步目录的行为。
        if (agent.id === "codex") {
          options.syncModelList = Boolean(
            options.syncModelList ?? Boolean(provider.id)
          )
          // 旧子列表不再生效，编辑保存统一使用服务商目录。
          delete options.modelList
          options.serviceTierFast = Boolean(options.serviceTierFast)
          options.modelContextWindowEnabled = Boolean(
            options.modelContextWindowEnabled
          )
          options.modelAutoCompactTokenLimit ??= 900000
        }
        if (agent.id === "claude-desktop") options.modelMappings ??= {}
        return {
          agentType: agent.id,
          enabled: Boolean(binding?.enabled),
          defaultModelName:
            providerModels(provider.id).find(
              (item) => item.id === binding?.defaultModelId
            )?.upstreamModelId || "",
          options
        }
      })
    }
    // 两个弹框复用草稿加载，各自展示和保存对应配置。
    editorSection.value = section
    detailProviderId.value = ""
    showGatewayManager.value = false
  } catch (error) {
    createMessage.error(error?.message || String(error))
  } finally {
    busy.value = false
  }
}

async function openAgentConfig(agent) {
  if (busy.value || !agent.installed) return
  agentConfig.value = agent
  agentProviderId.value = currentAgentInstance(agent.id)?.providerId || ""
  await selectAgentProvider()
}

async function openToolConfig(provider) {
  if (busy.value || !provider?.enabled || !provider.hasCredential) return
  agentConfig.value = { id: "gateway" }
  agentProviderId.value = provider.id
  await selectAgentProvider()
}

async function selectAgentProvider() {
  form.value = null
  if (!agentProviderId.value) return
  const provider = data.value.providers.find(
    (item) => item.id === agentProviderId.value
  )
  if (!provider?.enabled || !provider.hasCredential) return
  await editProvider(provider, "agent")
  if (form.value) {
    const binding = form.value.bindings[agentBindingIndex.value]
    binding.enabled = true
    binding.defaultModelName ||=
      draftModels.value.find((item) => item.enabled)?.upstreamModelId || ""
  }
}

function closeAgentConfig() {
  if (busy.value) return
  agentConfig.value = null
  agentProviderId.value = ""
  form.value = null
}

async function saveAgentConfig() {
  const binding = form.value?.bindings[agentBindingIndex.value]
  if (agentProviderId.value && (!binding || !binding.defaultModelName))
    return createMessage.error("请先选择可用模型")
  const tool = agentConfig.value.id === "gateway"
  if (
    await perform("configure-agent", {
      agentType: agentConfig.value.id,
      providerId: agentProviderId.value,
      revision: form.value?.revision,
      defaultModelName: binding?.defaultModelName,
      options: binding ? JSON.parse(JSON.stringify(binding.options)) : {}
    })
  ) {
    closeAgentConfig()
    if (tool) showGatewayManager.value = true
  }
}

function parseDraftModels() {
  const models = JSON.parse(form.value?.modelsJson || "[]")
  if (!Array.isArray(models)) throw new Error("模型详情必须为 JSON 数组")
  const names = new Set()
  return models.map((item) => {
    const model =
      typeof item === "string"
        ? {
            ...providerModels(form.value?.id).find(
              (model) => model.upstreamModelId === item
            ),
            upstreamModelId: item
          }
        : item
    if (
      !model ||
      typeof model.upstreamModelId !== "string" ||
      !model.upstreamModelId.trim()
    )
      throw new Error("请为每个模型填写 upstreamModelId")
    const name = model.upstreamModelId.trim()
    if (names.has(name)) throw new Error(`模型 ${name} 重复`)
    names.add(name)
    return {
      ...model,
      upstreamModelId: name,
      enabled: Boolean(model.enabled ?? true)
    }
  })
}

function updateDraftModelNames(value) {
  try {
    const names = [
      ...new Set(
        value
          .split(/[\r\n,;]+/)
          .map((item) => item.trim())
          .filter(Boolean)
      )
    ]
    const models = parseDraftModels()
    const next = models.map((item) => ({
      ...item,
      enabled: names.includes(item.upstreamModelId)
    }))
    for (const name of names) {
      if (!models.some((item) => item.upstreamModelId === name))
        next.push({ upstreamModelId: name, displayName: name, enabled: true })
    }
    form.value.modelsJson = JSON.stringify(next, null, 2)
    for (const binding of form.value.bindings) {
      if (!names.includes(binding.defaultModelName))
        binding.defaultModelName = ""
      if (binding.agentType === "claude") {
        for (const key of [
          "haikuModel",
          "sonnetModel",
          "opusModel",
          "subagentModel"
        ])
          if (!names.includes(binding.options[key])) binding.options[key] = ""
      } else if (binding.agentType === "claude-desktop") {
        for (const mapping of Object.values(
          binding.options.modelMappings || {}
        ))
          if (!names.includes(mapping.model)) mapping.model = ""
      }
    }
  } catch (error) {
    createMessage.error(`请先修正模型详情：${error?.message || error}`)
  }
}

async function fetchEditorModels() {
  if (busy.value || !form.value) return
  if (!form.value.id)
    return createMessage.error("请先保存服务商，再获取上游模型")
  const editor = form.value
  busy.value = true
  try {
    const models = parseDraftModels()
    const detail = await serviceProviderApi.invoke("detail", { id: editor.id })
    if (detail.revision !== editor.revision)
      throw new Error("配置已被修改，请重新打开编辑后获取模型")
    if (
      editor.apiKey ||
      ["baseUrl", "proxy", "upstreamProtocol"].some(
        (key) => editor[key] !== detail[key]
      ) ||
      JSON.stringify(JSON.parse(editor.headersJson)) !==
        JSON.stringify(detail.headers || {})
    )
      throw new Error("连接信息尚未保存，请先保存配置再获取模型")
    await serviceProviderApi.invoke("fetch-models", {
      providerId: editor.id,
      agentVersion: agentVersion("codex")
    })
    data.value = await serviceProviderApi.invoke("state")
    const updated = data.value.providers.find((item) => item.id === editor.id)
    if (updated?.revision !== editor.revision + 1)
      throw new Error(
        "获取模型期间配置已变化，请重新打开编辑，避免覆盖其他修改"
      )
    // 获取目录只追加新模型，保留用户尚未保存的模型、价格、默认模型与授权。
    const added = providerModels(editor.id).filter(
      (item) =>
        !models.some((model) => model.upstreamModelId === item.upstreamModelId)
    )
    editor.modelsJson = JSON.stringify([...models, ...added], null, 2)
    editor.revision = updated.revision
    createMessage.success("已获取模型，保存后应用当前编辑")
  } catch (error) {
    createMessage.error(error?.message || String(error))
  } finally {
    busy.value = false
  }
}

async function saveProvider() {
  try {
    const startLogin = !form.value.id && form.value.kind !== "api"
    const kind = form.value.kind
    const models = parseDraftModels()
    for (const binding of form.value.bindings) {
      if (
        binding.enabled &&
        binding.defaultModelName &&
        !models.some(
          (item) =>
            item.enabled && item.upstreamModelId === binding.defaultModelName
        )
      )
        throw new Error(
          `${agentLabel(binding.agentType)} 的默认模型已停用，请重新选择`
        )
    }
    const value = {
      id: form.value.id,
      revision: form.value.revision,
      name: form.value.name,
      icon: form.value.icon,
      kind: form.value.kind,
      baseUrl: form.value.baseUrl,
      proxy: form.value.proxy,
      note: form.value.note,
      enabled: form.value.enabled,
      upstreamProtocol:
        form.value.kind !== "api" ? "responses" : form.value.upstreamProtocol,
      headers: JSON.parse(form.value.headersJson),
      bindings: form.value.bindings.map((item) => ({
        agentType: item.agentType,
        enabled: item.enabled,
        defaultModelName: item.defaultModelName || "",
        options: JSON.parse(JSON.stringify(item.options))
      }))
    }
    // Agent 配置省略目录、别名与密钥字段，由后端保留原值。
    if (editorSection.value === "provider") {
      Object.assign(value, {
        apiKey: form.value.kind === "api" ? form.value.apiKey : "",
        keyLogicalId: form.value.keyLogicalId,
        keyName: form.value.keyName,
        models,
        routes: JSON.parse(form.value.routesJson)
      })
      if (!Array.isArray(value.routes))
        throw new Error("模型和别名必须为 JSON 数组")
    }
    if (await perform("save", value)) {
      form.value = null
      providerTab.value = kind === "api" ? "api" : "codex_account"
      if (startLogin && savedProviderId) {
        detailProviderId.value = savedProviderId
        detailTab.value = "overview"
        if (kind === "google_account")
          await startGoogleLogin({ id: savedProviderId, proxy: value.proxy })
        else
          await perform(
            "login",
            { accountId: savedProviderId, proxy: value.proxy },
            false
          )
      }
    }
  } catch (error) {
    createMessage.error(error?.message || String(error))
  }
}

function openApply(provider) {
  if (!provider?.enabled || !provider.hasCredential)
    return createMessage.error("请先恢复服务商可用状态并配置凭据")
  const agent = agents.find((item) =>
    data.value.bindings.some(
      (binding) =>
        binding.providerId === provider.id &&
        binding.agentType === item.id &&
        binding.enabled
    )
  )
  if (!agent) return createMessage.error("请先点击 Agent 配置并授权 Agent")
  detailProviderId.value = ""
  managerProviderId.value = provider.id
  showGatewayManager.value = false
  applyForm.value = {
    providerId: provider.id,
    agentType: agent.id,
    modelId: "",
    configPath: "",
    name: ""
  }
  updateAgent()
}

function closeApply() {
  applyForm.value = null
  showGatewayManager.value = true
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
  if (await perform("activate", applyForm.value)) closeApply()
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
  .agent-list {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    overflow-y: auto;
    .agent-card {
      display: flex;
      flex: none;
      align-items: center;
      gap: 16px;
      padding: 20px;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel);
      box-shadow: 0 3px 12px rgba(15, 23, 42, 0.04);
      .agent-avatar {
        display: grid;
        flex: none;
        width: 44px;
        height: 44px;
        border: 1px solid var(--color-line);
        border-radius: 12px;
        background: var(--color-panel-soft);
        color: var(--color-text);
        place-items: center;
        .agent-icon {
          width: 28px;
          height: 28px;
        }
      }
      .agent-main {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        gap: 7px;
        .agent-heading {
          display: flex;
          align-items: center;
          gap: 10px;
          .agent-name {
            color: var(--color-text);
            font-size: var(--font-size-lg);
          }
          .agent-version {
            color: var(--color-text-soft);
            font-size: var(--font-size-xs);
          }
        }
        .agent-source {
          display: flex;
          align-items: center;
          gap: 6px;
          color: var(--color-text-muted);
          font-size: var(--font-size-base);
          .agent-source-dot {
            width: 6px;
            height: 6px;
            border-radius: 50%;
            background: var(--color-success);
          }
        }
        .agent-path {
          overflow: hidden;
          color: var(--color-text-soft);
          font-size: var(--font-size-xs);
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }
      .agent-config-button {
        display: inline-flex;
        flex: none;
        align-items: center;
        gap: 8px;
        min-height: 36px;
        padding: 0 12px;
        border: 1px solid var(--color-line);
        border-radius: 9px;
        background: var(--color-panel-soft);
        color: var(--color-text);
        font: inherit;
        cursor: pointer;
        transition:
          border-color 0.18s ease,
          background 0.18s ease;
        &:hover {
          border-color: var(--color-info-line);
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
        &:disabled {
          opacity: 0.5;
          cursor: default;
        }
      }
    }
  }
  .create-options {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 4px 0;
    .create-option {
      display: flex;
      align-items: center;
      gap: 14px;
      padding: 16px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      font: inherit;
      text-align: left;
      cursor: pointer;
      &:hover {
        border-color: var(--color-info-line);
        background: var(--color-panel-soft);
      }
      .create-icon {
        width: 28px;
        height: 28px;
      }
      .create-description {
        display: flex;
        flex: 1;
        flex-direction: column;
        gap: 5px;
        .create-label {
          color: var(--color-text);
        }
        .create-hint {
          color: var(--color-text-soft);
          font-size: var(--font-size-xs);
        }
      }
    }
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
            &.missing-tag {
              border-color: var(--color-warning-line);
              background: var(--color-warning-soft);
              color: var(--color-warning);
            }
          }
          .running-agent-tag {
            display: inline-flex;
            flex: none;
            align-items: center;
            gap: 5px;
            padding: 2px 7px;
            border-radius: 5px;
            background: var(--color-success-soft);
            color: var(--color-success);
            font-size: var(--font-size-xs);
            .running-agent-dot {
              width: 5px;
              height: 5px;
              border-radius: 50%;
              background: currentColor;
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
        flex-direction: column;
        align-items: flex-end;
        gap: 12px;
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
  .gateway-manager {
    .agent-apply {
      display: flex;
      flex-wrap: wrap;
      align-items: flex-end;
      gap: 10px;

      .form-field {
        display: flex;
        flex: 1;
        min-width: 200px;
        flex-direction: column;
        gap: 8px;
        font-size: var(--font-size-sm);
      }
    }
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
    .form-grid {
      display: flex;
      flex-wrap: wrap;
      gap: 16px;

      .form-field {
        flex: 1 1 280px;
        min-width: 0;
      }
    }
    .check-field {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      margin-right: 16px;
    }
    .advanced-fields {
      padding: 14px;
      border: 1px solid var(--color-line);
      border-radius: 8px;

      .form-field,
      .form-grid,
      .muted {
        margin-top: 16px;
      }
    }
    .form-footer {
      position: sticky;
      bottom: 0;
      z-index: 1;
      display: flex;
      flex: none;
      justify-content: flex-end;
      gap: 10px;
      padding: 12px 0;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel);
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

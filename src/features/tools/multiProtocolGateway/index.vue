<template>
  <section class="gateway-tool">
    <!-- 顶部标题与全局刷新 -->
    <header class="gateway-top-bar">
      <div class="top-title-group">
        <div class="top-icon-box">
          <Waypoints :size="20" />
        </div>
        <div class="top-text-box">
          <h2 class="top-title">多协议反代</h2>
          <p class="top-subtitle">
            统一转发 Provider、Codex 与 Gemini 官方账号，管理调用 Token
            和真实模型记录。
          </p>
        </div>
      </div>
      <button
        class="icon-action-btn"
        type="button"
        title="刷新网关状态"
        :disabled="busy || loading"
        @click="load"
      >
        <RotateCw :size="16" :class="{ spin: loading }" />
      </button>
    </header>

    <!-- 统一模型网关状态卡片 -->
    <section class="gateway-service-card">
      <div class="service-left">
        <div class="service-badge-icon">
          <ServerCog :size="24" />
        </div>
        <div class="service-meta">
          <div class="service-title-line">
            <h3 class="service-name">统一模型网关</h3>
            <span
              class="service-status-pill"
              :class="{ active: gateway.enabled }"
            >
              <span class="status-dot"></span>
              {{ gateway.enabled ? "运行中" : "未运行" }}
            </span>
          </div>
          <p class="service-url">
            {{ gateway.baseUrl || defaultBaseUrl }}
          </p>
        </div>
      </div>

      <div class="service-right">
        <!-- 局域网模式 switch -->
        <div class="switch-item">
          <span
            class="switch-label"
            title="开启后监听 0.0.0.0，局域网内其他设备可通过本机 IP 访问"
          >
            <Info :size="13" class="info-icon" /> 局域网模式
          </span>
          <button
            type="button"
            class="switch-control"
            :class="{ checked: isLanMode }"
            :disabled="busy || loading"
            :aria-checked="isLanMode"
            role="switch"
            @click="toggleLanMode"
          >
            <span class="switch-thumb"></span>
          </button>
        </div>

        <!-- 监听配置按钮：刷新按钮旁边单独一个按钮点击后弹框出现 -->
        <button
          class="tool-button"
          type="button"
          title="配置网关监听 IP 与端口"
          @click="openListenerModal"
        >
          <SlidersHorizontal :size="14" />
          监听配置
        </button>

        <!-- 刷新按钮 -->
        <button
          class="tool-button"
          type="button"
          :disabled="busy || loading"
          @click="load"
        >
          <RefreshCw :size="14" :class="{ spin: loading }" />
          刷新
        </button>

        <!-- 启动 / 停止网关按钮 -->
        <button
          class="tool-button"
          :class="gateway.enabled ? 'danger' : 'primary'"
          type="button"
          :disabled="busy || loading"
          @click="toggleGateway"
        >
          <component :is="gateway.enabled ? Square : Play" :size="13" />
          {{ gateway.enabled ? "停止网关" : "启动网关" }}
        </button>
      </div>
    </section>

    <!-- 局域网模式提醒横幅 -->
    <div
      v-if="isLanMode && gateway.enabled && !lanBannerDismissed"
      class="lan-alert-banner"
    >
      <div class="banner-content">
        <Info :size="15" class="banner-icon" />
        <span
          >局域网访问已开启，局域网内设备可通过本机IP:{{
            draft.port
          }}
          访问该服务。</span
        >
      </div>
      <button
        class="banner-close-btn"
        type="button"
        title="关闭提醒"
        @click="lanBannerDismissed = true"
      >
        <X :size="14" />
      </button>
    </div>

    <!-- 加载错误提示 -->
    <p v-if="loadError" class="gateway-error" role="alert">{{ loadError }}</p>

    <!-- 4 个统计 KPI 卡片 -->
    <section class="gateway-metrics">
      <article
        v-for="metric in metrics"
        :key="metric.label"
        class="metric-item"
        :class="metric.type"
      >
        <div class="metric-top">
          <span class="metric-icon">
            <component :is="metric.icon" :size="16" />
          </span>
          <span class="metric-label">{{ metric.label }}</span>
        </div>
        <span class="metric-value">{{ metric.value }}</span>
      </article>
    </section>

    <!-- 4 个管理页签导航 -->
    <nav class="gateway-tabs" aria-label="反代管理">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="tab-button"
        :class="{ active: activeTab === tab.id }"
        type="button"
        @click="activeTab = tab.id"
      >
        <component :is="tab.icon" :size="15" class="tab-icon" />
        <span>{{ tab.label }}</span>
      </button>
    </nav>

    <!-- 页签 1: 配置与模型 (底层池 + 支持模型) -->
    <section v-if="activeTab === 'config'" class="gateway-content">
      <!-- 底层池 (Provider) -->
      <section class="gateway-panel">
        <div class="panel-heading">
          <div class="heading-title-group">
            <Layers :size="16" class="heading-icon" />
            <h3 class="panel-title">底层池 (Provider)</h3>
          </div>
          <span class="panel-detail"
            >选择可用的底层 Provider，作为模型调用的后端池。</span
          >
        </div>

        <div class="target-list">
          <label
            v-for="target in targets"
            :key="target.id"
            class="target-item"
            :class="{ selected: draft.providerIds.includes(target.id) }"
          >
            <input
              v-model="draft.providerIds"
              class="target-checkbox"
              type="checkbox"
              :value="target.id"
              @change="onProviderToggle"
            />
            <span class="target-info">
              <span class="target-name">{{ target.name }}</span>
              <span class="target-type">{{ target.type }}</span>
            </span>
            <span
              v-if="draft.providerIds.includes(target.id)"
              class="target-status-badge"
            >
              <span class="status-dot"></span>
              {{ gateway.enabled ? "运行中" : "可用" }}
            </span>
          </label>
          <p v-if="!targets.length" class="empty-state">
            请先在 Providers 中添加 Provider、Codex 或 Gemini 官方账号。
          </p>
        </div>
      </section>

      <!-- 支持模型 -->
      <section class="gateway-panel">
        <div class="panel-heading">
          <div class="heading-title-group">
            <Boxes :size="16" class="heading-icon" />
            <h3 class="panel-title">支持模型</h3>
          </div>
          <div class="heading-actions">
            <button class="tool-button" type="button" @click="openImportModal">
              <Upload :size="13" /> 导入模型
            </button>
            <button
              class="tool-button primary"
              type="button"
              @click="openAddModelModal"
            >
              <Plus :size="13" /> 添加模型
            </button>
          </div>
        </div>

        <!-- 支持模型筛选栏 -->
        <div v-if="modelEntries.length > 0" class="model-filter-bar">
          <div class="search-input-wrap">
            <Search :size="13" class="filter-icon" />
            <input
              v-model.trim="modelFilter.search"
              class="model-search-input"
              placeholder="搜索模型名称或 ID..."
              @input="modelPage = 1"
            />
          </div>
          <div class="target-select-wrap">
            <select
              v-model="modelFilter.targetId"
              class="target-select"
              @change="modelPage = 1"
            >
              <option value="all">
                全部服务商 ({{ modelEntries.length }})
              </option>
              <option
                v-for="target in modelTargetOptions"
                :key="target.id"
                :value="target.id"
              >
                {{ target.name }} ({{ target.count }})
              </option>
            </select>
          </div>
          <button
            v-if="modelFilter.search || modelFilter.targetId !== 'all'"
            class="tool-button"
            type="button"
            @click="resetModelFilter"
          >
            <RotateCcw :size="12" /> 重置
          </button>
        </div>

        <div v-if="paginatedConfigModels.length > 0" class="model-table-wrap">
          <table class="model-table">
            <thead>
              <tr>
                <th>模型名称</th>
                <th>模型 ID</th>
                <th>路由目标</th>
                <th>创建时间</th>
                <th class="action-th">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in paginatedConfigModels" :key="item.alias">
                <td>
                  <div class="model-name-cell">
                    <span
                      class="brand-avatar"
                      :class="getModelBrand(item.alias).class"
                    >
                      {{ getModelBrand(item.alias).initial }}
                    </span>
                    <span class="model-name-text">{{ item.alias }}</span>
                  </div>
                </td>
                <td>
                  <code class="model-id-code">{{ item.alias }}</code>
                </td>
                <td>
                  <div class="target-cell">
                    <span class="target-badge">{{ item.targetName }}</span>
                    <span
                      v-if="item.realModel && item.realModel !== item.alias"
                      class="real-model-hint"
                    >
                      → {{ item.realModel }}
                    </span>
                  </div>
                </td>
                <td>
                  <span class="time-text">{{
                    item.createdAt ? formatDateTime(item.createdAt) : "—"
                  }}</span>
                </td>
                <td class="action-td">
                  <div class="table-actions">
                    <button
                      class="action-btn"
                      type="button"
                      title="编辑模型路由"
                      @click="openEditModelModal(item)"
                    >
                      <Pencil :size="13" /> 编辑
                    </button>
                    <button
                      class="action-btn danger"
                      type="button"
                      title="删除模型路由"
                      @click="removeModelMapping(item.alias)"
                    >
                      <Trash2 :size="13" /> 删除
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <div v-else-if="modelEntries.length > 0" class="empty-models-box">
          <Boxes :size="32" class="empty-icon" />
          <p class="empty-title">未找到匹配的模型</p>
          <p class="empty-sub">请尝试调整关键字或筛选其他服务商</p>
          <button class="tool-button" type="button" @click="resetModelFilter">
            重置筛选
          </button>
        </div>

        <div v-else class="empty-models-box">
          <Boxes :size="36" class="empty-icon" />
          <p class="empty-title">暂无模型</p>
          <p class="empty-sub">还没有添加任何模型，点击「添加模型」开始配置</p>
          <button
            class="tool-button primary"
            type="button"
            @click="openAddModelModal"
          >
            <Plus :size="13" /> 添加模型
          </button>
        </div>

        <!-- 支持模型分页设置栏 -->
        <div v-if="filteredConfigModels.length > 0" class="pagination-footer">
          <span class="total-text"
            >共 {{ filteredConfigModels.length }} 个模型</span
          >
          <div class="page-controls">
            <select
              v-model.number="modelPageSize"
              class="page-size-select"
              @change="modelPage = 1"
            >
              <option :value="5">5 条/页</option>
              <option :value="10">10 条/页</option>
              <option :value="20">20 条/页</option>
            </select>
            <button
              class="page-btn"
              :disabled="modelPage <= 1"
              @click="modelPage--"
            >
              &lt;
            </button>
            <span class="page-num active"
              >{{ modelPage }} / {{ totalConfigModelPages }}</span
            >
            <button
              class="page-btn"
              :disabled="modelPage >= totalConfigModelPages"
              @click="modelPage++"
            >
              &gt;
            </button>
          </div>
        </div>
      </section>
    </section>

    <!-- 页签 2: 接入地址 -->
    <section v-else-if="activeTab === 'connections'" class="gateway-content">
      <GatewayConnectionPanel
        :base-url="gateway.baseUrl || ''"
        :enabled="gateway.enabled === true"
        :models="gateway.models || []"
        :model-mappings="gateway.modelMappings || {}"
        :targets="targets"
        @manage-tokens="activeTab = 'tokens'"
      />
    </section>

    <!-- 页签 3: 调用 Token -->
    <section v-else-if="activeTab === 'tokens'" class="gateway-content">
      <section class="gateway-panel">
        <div class="panel-heading">
          <div class="heading-title-group">
            <KeyRound :size="16" class="heading-icon" />
            <h3 class="panel-title">调用 Token</h3>
          </div>
          <button
            class="tool-button primary"
            type="button"
            @click="showCreateTokenModal = true"
          >
            <Plus :size="13" /> 创建 Token
          </button>
        </div>

        <!-- 筛选栏 -->
        <div class="token-filter-bar">
          <div class="filter-field">
            <Search :size="13" class="filter-icon" />
            <input
              v-model.trim="tokenFilter.name"
              class="filter-input"
              placeholder="输入名称关键字"
            />
          </div>
          <div class="filter-field">
            <input
              v-model.trim="tokenFilter.description"
              class="filter-input"
              placeholder="输入用途说明关键字"
            />
          </div>
          <div class="filter-field">
            <select v-model="tokenFilter.expiry" class="filter-select">
              <option value="all">全部有效状态</option>
              <option value="permanent">永久有效</option>
              <option value="active">未过期</option>
              <option value="expired">已过期</option>
            </select>
          </div>
          <div class="filter-actions">
            <button class="tool-button" type="button" @click="resetTokenFilter">
              <RotateCcw :size="13" /> 重置
            </button>
            <button
              class="tool-button primary"
              type="button"
              @click="tokenPage = 1"
            >
              <Search :size="13" /> 查询
            </button>
          </div>
        </div>

        <!-- Token 明文展示提示框 (仅在 tokens 页签渲染) -->
        <div v-if="createdToken" class="created-token-card" role="status">
          <div class="created-card-top">
            <span class="created-note"
              >Token
              创建成功！明文仅在创建后显示一次，请立即复制并妥善保存。</span
            >
            <button
              class="tool-button"
              type="button"
              @click="createdToken = ''"
            >
              已保存并收起
            </button>
          </div>
          <div class="created-card-body">
            <code class="created-value">{{ createdToken }}</code>
            <button
              class="tool-button primary"
              type="button"
              @click="copyText(createdToken)"
            >
              <Copy :size="13" /> 复制密钥
            </button>
          </div>
        </div>

        <!-- Token 列表表格 -->
        <div v-if="filteredTokens.length > 0" class="token-table-wrap">
          <table class="token-table">
            <thead>
              <tr>
                <th>Token 名称</th>
                <th>Token 前缀</th>
                <th>创建时间</th>
                <th>请求统计</th>
                <th>输入 Token</th>
                <th>输出 Token</th>
                <th>状态</th>
                <th class="action-th">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="token in paginatedTokens" :key="token.id">
                <td>
                  <div class="token-name-col">
                    <span class="token-name">{{ token.name }}</span>
                    <span class="token-desc">{{
                      token.description || "无用途说明"
                    }}</span>
                  </div>
                </td>
                <td>
                  <div class="token-hint-col">
                    <div class="hint-row">
                      <span class="hint-text">{{ token.tokenHint }}</span>
                      <button
                        class="hint-copy-btn"
                        type="button"
                        title="复制前缀"
                        @click="copyText(token.tokenHint)"
                      >
                        <Copy :size="11" />
                      </button>
                    </div>
                  </div>
                </td>
                <td>
                  <span class="time-text">{{
                    token.createdAt ? formatDateTime(token.createdAt) : "—"
                  }}</span>
                </td>
                <td>
                  <div class="token-stats-col">
                    <span class="stats-total"
                      ><BarChart3 :size="12" /> 请求
                      {{ formatNumber(token.requestCount || 0) }} 次</span
                    >
                    <span class="stats-detail"
                      >成功 {{ formatNumber(token.successCount || 0) }} | 失败
                      {{ formatNumber(token.failureCount || 0) }}</span
                    >
                  </div>
                </td>
                <td>
                  <span class="number-text">{{
                    formatNumber(token.inputTokens || 0)
                  }}</span>
                </td>
                <td>
                  <span class="number-text">{{
                    formatNumber(token.outputTokens || 0)
                  }}</span>
                </td>
                <td>
                  <span
                    class="token-status-pill"
                    :class="{ enabled: token.enabled }"
                  >
                    <span class="status-dot"></span>
                    {{ token.enabled ? "有效" : "停用" }}
                  </span>
                </td>
                <td class="action-td">
                  <div class="table-actions">
                    <button
                      class="action-btn"
                      type="button"
                      title="复制 Token 提示"
                      @click="copyText(token.tokenHint)"
                    >
                      <Copy :size="12" /> 复制
                    </button>
                    <button
                      class="action-btn"
                      type="button"
                      title="查看 Token 详情"
                      @click="viewingToken = token"
                    >
                      <Eye :size="12" /> 查看
                    </button>
                    <button
                      class="action-btn"
                      type="button"
                      :title="token.enabled ? '停用 Token' : '启用 Token'"
                      :disabled="busy"
                      @click="setTokenEnabled(token)"
                    >
                      <component
                        :is="token.enabled ? Square : Play"
                        :size="12"
                      />
                      {{ token.enabled ? "停用" : "启用" }}
                    </button>
                    <button
                      class="action-btn danger"
                      type="button"
                      title="删除 Token"
                      :disabled="busy"
                      :aria-label="`删除 ${token.name}`"
                      @click="deletingToken = token"
                    >
                      <Trash2 :size="12" /> 删除
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <div v-else class="empty-state-box">
          <KeyRound :size="36" class="empty-icon" />
          <p class="empty-title">暂无调用 Token</p>
          <p class="empty-sub">
            创建第一个 Token 后即可从外部客户端调用统一模型网关。
          </p>
          <button
            class="tool-button primary"
            type="button"
            @click="showCreateTokenModal = true"
          >
            <Plus :size="13" /> 创建 Token
          </button>
        </div>

        <!-- Token 分页 -->
        <div v-if="filteredTokens.length > 0" class="pagination-footer">
          <span class="total-text">共 {{ filteredTokens.length }} 条记录</span>
          <div class="page-controls">
            <span class="page-size">10 条/页</span>
            <button
              class="page-btn"
              :disabled="tokenPage <= 1"
              @click="tokenPage--"
            >
              &lt;
            </button>
            <span class="page-num active">{{ tokenPage }}</span>
            <button
              class="page-btn"
              :disabled="tokenPage * pageSize >= filteredTokens.length"
              @click="tokenPage++"
            >
              &gt;
            </button>
          </div>
        </div>
      </section>
    </section>

    <!-- 页签 4: 用量与追踪 -->
    <section v-else class="gateway-content">
      <div class="usage-top-row">
        <!-- 用量概览 -->
        <section class="gateway-panel usage-overview-panel">
          <div class="panel-heading">
            <div class="heading-title-group">
              <BarChart3 :size="16" class="heading-icon" />
              <h3 class="panel-title">用量概览</h3>
            </div>
            <div class="overview-controls">
              <select v-model="logTimeFilter" class="overview-period-select">
                <option value="24h">近 24 小时</option>
                <option value="7d">近 7 天</option>
                <option value="30d">近 30 天</option>
                <option value="all">全部时间</option>
              </select>
              <span class="date-range-badge">{{ formattedDateRange }}</span>
            </div>
          </div>

          <div class="overview-stats-grid">
            <div class="overview-tile">
              <span class="tile-label">总请求</span>
              <span class="tile-value">{{
                formatNumber(overviewStats.totalRequests)
              }}</span>
            </div>
            <div class="overview-tile success">
              <span class="tile-label">成功</span>
              <span class="tile-value">{{
                formatNumber(overviewStats.successRequests)
              }}</span>
            </div>
            <div class="overview-tile failure">
              <span class="tile-label">失败</span>
              <span class="tile-value">{{
                formatNumber(overviewStats.failureRequests)
              }}</span>
            </div>
            <div class="overview-tile token">
              <span class="tile-label">总 Token</span>
              <span class="tile-value">{{
                formatNumber(overviewStats.totalTokens)
              }}</span>
            </div>
          </div>
        </section>

        <!-- 用量归属 -->
        <section class="gateway-panel usage-attribution-panel">
          <div class="panel-heading">
            <div class="heading-title-group">
              <Layers :size="16" class="heading-icon" />
              <h3 class="panel-title">用量归属</h3>
            </div>
            <span class="panel-detail"
              >明细保留最近 5000 条，调用 Token 总计持续累加。</span
            >
          </div>

          <div
            v-if="usageAttributionRows.length > 0"
            class="attribution-table-wrap"
          >
            <table class="attribution-table">
              <thead>
                <tr>
                  <th>调用方</th>
                  <th>底层目标</th>
                  <th>真实模型</th>
                  <th>请求数</th>
                  <th>输入 Token</th>
                  <th>输出 Token</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in usageAttributionRows" :key="item.key">
                  <td>
                    <span class="cell-caller">{{ item.caller }}</span>
                  </td>
                  <td>
                    <span class="target-badge">{{ item.target }}</span>
                  </td>
                  <td>
                    <code class="model-id-code">{{ item.model }}</code>
                  </td>
                  <td>{{ formatNumber(item.requestCount) }}</td>
                  <td>{{ formatNumber(item.inputTokens) }}</td>
                  <td>{{ formatNumber(item.outputTokens) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <p v-else class="empty-state">暂无归属数据。</p>
        </section>
      </div>

      <!-- 最近请求 -->
      <section class="gateway-panel">
        <div class="panel-heading">
          <div class="heading-title-group">
            <ListFilter :size="16" class="heading-icon" />
            <h3 class="panel-title">最近请求</h3>
          </div>
          <div class="logs-toolbar">
            <div class="log-search-wrap">
              <Search :size="13" class="filter-icon" />
              <input
                v-model.trim="logSearch"
                class="log-search-input"
                placeholder="搜索请求 (请求 ID、路由、模型等)"
              />
            </div>
            <button
              class="tool-button"
              type="button"
              title="导出当前请求日志为 JSON 文件"
              @click="exportLogs"
            >
              <Download :size="13" /> 导出
            </button>
          </div>
        </div>

        <div v-if="filteredLogs.length > 0" class="request-table-wrap">
          <table class="request-table">
            <thead class="request-head">
              <tr>
                <th>时间 / 调用方</th>
                <th>协议</th>
                <th>请求 → 路由 → 上游报告</th>
                <th>最终目标</th>
                <th>输入 / 输出</th>
                <th>状态 / 耗时</th>
              </tr>
            </thead>
            <tbody class="request-body">
              <tr
                v-for="log in paginatedLogs"
                :key="log.id"
                class="request-row"
              >
                <td>
                  <span class="log-time">{{
                    formatDateTime(log.createdAt)
                  }}</span>
                  <span class="cell-detail">{{
                    log.tokenName || "直接调用"
                  }}</span>
                </td>
                <td>
                  <span class="protocol-tag" :class="log.protocol">{{
                    log.protocol || "chat"
                  }}</span>
                </td>
                <td>
                  <div class="routing-chain">
                    <code class="cell-model">{{
                      log.requestedModel || "—"
                    }}</code>
                    <span class="cell-arrow">→</span>
                    <span class="cell-target">{{
                      log.physicalModel || "—"
                    }}</span>
                    <span
                      class="upstream-report"
                      :class="{ ok: log.ok, error: !log.ok }"
                    >
                      {{
                        log.ok
                          ? `${log.statusCode || 200} OK`
                          : `${log.statusCode || 500} Error`
                      }}
                    </span>
                  </div>
                </td>
                <td>
                  <span class="log-provider-name">{{
                    log.providerName || "未路由"
                  }}</span>
                  <span class="cell-detail">{{ log.providerId || "—" }}</span>
                </td>
                <td>
                  <span v-if="log.usageKnown" class="token-in-out">
                    {{ formatNumber(log.inputTokens) }} /
                    {{ formatNumber(log.outputTokens) }}
                  </span>
                  <span v-else class="cell-muted">未报告</span>
                </td>
                <td>
                  <span
                    class="request-status-badge"
                    :class="{ failed: !log.ok }"
                  >
                    {{ log.ok ? "成功" : "失败" }} · {{ log.statusCode }}
                  </span>
                  <span class="cell-detail">{{ log.latencyMs || 0 }} ms</span>
                  <span v-if="log.errorMessage" class="cell-error">
                    {{ log.errorMessage }}
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-else class="empty-state">暂无请求记录。</p>

        <!-- 日志分页 -->
        <div v-if="filteredLogs.length > 0" class="pagination-footer">
          <span class="total-text"
            >共 {{ filteredLogs.length }} 条记录，显示第
            {{ (logPage - 1) * pageSize + 1 }} -
            {{ Math.min(logPage * pageSize, filteredLogs.length) }} 条</span
          >
          <div class="page-controls">
            <span class="page-size">10 条/页</span>
            <button
              class="page-btn"
              :disabled="logPage <= 1"
              @click="logPage--"
            >
              &lt;
            </button>
            <span class="page-num active">{{ logPage }}</span>
            <button
              class="page-btn"
              :disabled="logPage * pageSize >= filteredLogs.length"
              @click="logPage++"
            >
              &gt;
            </button>
          </div>
        </div>
      </section>
    </section>

    <!-- 弹框 1: 监听配置弹框 (刷新按钮旁边单独按钮触发) -->
    <BaseModal
      v-if="showListenerModal"
      title="监听配置"
      description="本地访问使用 127.0.0.1；局域网访问需开启「局域网模式」或设置为 0.0.0.0。"
      @close="showListenerModal = false"
    >
      <form class="modal-form" @submit.prevent="saveListenerConfig">
        <div class="field-row">
          <label class="form-field">
            <span class="field-label">监听 IP</span>
            <input
              v-model.trim="listenerDraft.host"
              class="field-input"
              required
              placeholder="127.0.0.1"
            />
            <span class="field-help"
              >本机使用 127.0.0.1；局域网访问使用 0.0.0.0 (所有网卡)。</span
            >
          </label>
          <label class="form-field">
            <span class="field-label">监听端口</span>
            <input
              v-model.number="listenerDraft.port"
              class="field-input"
              required
              type="number"
              min="1"
              max="65535"
              placeholder="8046"
            />
            <span class="field-help"
              >默认端口 8046，端口冲突时将无法启动服务。</span
            >
          </label>
        </div>

        <div class="modal-footer-actions">
          <button
            class="tool-button"
            type="button"
            @click="resetListenerDefaults"
          >
            <RotateCcw :size="13" /> 重置
          </button>
          <div class="right-buttons">
            <button
              class="tool-button"
              type="button"
              @click="showListenerModal = false"
            >
              取消
            </button>
            <button
              class="tool-button primary"
              type="submit"
              :disabled="busy || loading"
            >
              <Save :size="13" /> 保存配置
            </button>
          </div>
        </div>
      </form>
    </BaseModal>

    <!-- 弹框 2: 添加/编辑模型路由弹框 -->
    <BaseModal
      v-if="showAddModelModal"
      :title="editingModelAlias ? '编辑支持模型' : '添加支持模型'"
      description="为外部客户端调用的大模型名称指定路由目标和真实底层模型。"
      @close="showAddModelModal = false"
    >
      <form class="modal-form" @submit.prevent="saveModelMapping">
        <div class="field-column">
          <label class="form-field">
            <span class="field-label">外部模型名 (ID)</span>
            <input
              v-model.trim="modelForm.alias"
              class="field-input"
              required
              :disabled="!!editingModelAlias"
              placeholder="例如：gpt-4o 或 claude-3-7-sonnet"
            />
            <span class="field-help"
              >外部客户端在请求参数 model 中指定的模型标识。</span
            >
          </label>
          <label class="form-field">
            <span class="field-label">路由目标 (底层池 Provider)</span>
            <select v-model="modelForm.targetId" class="field-input" required>
              <option value="">请选择池中目标</option>
              <option
                v-for="target in selectedTargets"
                :key="target.id"
                :value="target.id"
              >
                {{ target.name }} ({{ target.type }})
              </option>
            </select>
            <span class="field-help"
              >此模型的请求将转发至该底层目标并鉴权。</span
            >
          </label>
          <label class="form-field">
            <span class="field-label">真实底层模型名</span>
            <input
              v-model.trim="modelForm.model"
              class="field-input"
              required
              placeholder="例如：claude-3-7-sonnet-20250219 或 gpt-4o-2024-11-20"
            />
            <span class="field-help"
              >向上游 Provider 发起真实请求时填写的模型名。</span
            >
          </label>
        </div>

        <div class="modal-footer-actions">
          <div></div>
          <div class="right-buttons">
            <button
              class="tool-button"
              type="button"
              @click="showAddModelModal = false"
            >
              取消
            </button>
            <button class="tool-button primary" type="submit" :disabled="busy">
              <Save :size="13" />
              {{ editingModelAlias ? "保存修改" : "确认添加" }}
            </button>
          </div>
        </div>
      </form>
    </BaseModal>

    <!-- 弹框 3: 导入底层池模型弹框 -->
    <BaseModal
      v-if="showImportModal"
      title="导入底层池模型"
      description="自动识别所选底层池 Provider 支持的模型，勾选后可一键加入网关支持模型列表中。"
      @close="showImportModal = false"
    >
      <div class="import-modal-content">
        <div v-if="importableModels.length > 0" class="import-models-list">
          <label
            v-for="model in importableModels"
            :key="model.id"
            class="import-model-item"
          >
            <input
              v-model="selectedImportModelIds"
              type="checkbox"
              :value="model.id"
            />
            <div class="import-model-meta">
              <span class="import-name">{{ model.name || model.id }}</span>
              <span class="import-target">{{ model.targetName }}</span>
            </div>
          </label>
        </div>
        <p v-else class="empty-state">
          当前底层池中暂无可导入的未添加模型，请先在底层池中勾选 Provider。
        </p>

        <div class="modal-footer-actions">
          <div></div>
          <div class="right-buttons">
            <button
              class="tool-button"
              type="button"
              @click="showImportModal = false"
            >
              取消
            </button>
            <button
              class="tool-button primary"
              type="button"
              :disabled="!selectedImportModelIds.length || busy"
              @click="executeImportModels"
            >
              <Upload :size="13" /> 导入选中 ({{
                selectedImportModelIds.length
              }})
            </button>
          </div>
        </div>
      </div>
    </BaseModal>

    <!-- 弹框 4: 创建 Token 弹框 -->
    <BaseModal
      v-if="showCreateTokenModal"
      title="创建调用 Token"
      description="用于外部客户端或 SDK 访问统一模型网关的鉴权 Token。"
      @close="showCreateTokenModal = false"
    >
      <form class="modal-form" @submit.prevent="submitCreateToken">
        <div class="field-column">
          <label class="form-field">
            <span class="field-label">调用方名称</span>
            <input
              v-model.trim="tokenDraft.name"
              class="field-input"
              required
              maxlength="80"
              placeholder="例如：开发环境调试、Python 自动化、OpenWebUI"
            />
          </label>
          <label class="form-field">
            <span class="field-label">用途说明 (可选)</span>
            <input
              v-model.trim="tokenDraft.description"
              class="field-input"
              maxlength="200"
              placeholder="简要说明此 Token 的用途或使用者"
            />
          </label>
          <label class="form-field">
            <span class="field-label">到期时间 (留空为永久有效)</span>
            <input
              v-model="tokenDraft.expiresAt"
              class="field-input"
              type="datetime-local"
            />
          </label>
        </div>

        <div class="modal-footer-actions">
          <div></div>
          <div class="right-buttons">
            <button
              class="tool-button"
              type="button"
              @click="showCreateTokenModal = false"
            >
              取消
            </button>
            <button
              class="tool-button primary"
              type="submit"
              :disabled="busy || !tokenDraft.name"
            >
              <Plus :size="13" /> 立即创建
            </button>
          </div>
        </div>
      </form>
    </BaseModal>

    <!-- 弹框 5: 查看 Token 详情弹框 -->
    <BaseModal
      v-if="viewingToken"
      title="调用 Token 详情"
      :description="`查看 ${viewingToken.name} 的配置与用量统计`"
      @close="viewingToken = null"
    >
      <div class="token-detail-view">
        <div class="detail-row">
          <span class="detail-label">Token 名称：</span>
          <span class="detail-value">{{ viewingToken.name }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">密钥前缀：</span>
          <span class="detail-value">{{ viewingToken.tokenHint }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">用途说明：</span>
          <span class="detail-value">{{
            viewingToken.description || "无说明"
          }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">当前状态：</span>
          <span
            class="token-status-pill"
            :class="{ enabled: viewingToken.enabled }"
          >
            <span class="status-dot"></span>
            {{ viewingToken.enabled ? "有效" : "已停用" }}
          </span>
        </div>
        <div class="detail-row">
          <span class="detail-label">创建时间：</span>
          <span class="detail-value">{{
            viewingToken.createdAt
              ? formatDateTime(viewingToken.createdAt)
              : "—"
          }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">过期时间：</span>
          <span class="detail-value">{{
            viewingToken.expiresAt
              ? formatDateTime(viewingToken.expiresAt)
              : "永久有效"
          }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">请求统计：</span>
          <span class="detail-value"
            >累计 {{ formatNumber(viewingToken.requestCount || 0) }} 次 (成功
            {{ formatNumber(viewingToken.successCount || 0) }} / 失败
            {{ formatNumber(viewingToken.failureCount || 0) }})</span
          >
        </div>
        <div class="detail-row">
          <span class="detail-label">输入 / 输出：</span>
          <span class="detail-value"
            >{{ formatNumber(viewingToken.inputTokens || 0) }} /
            {{ formatNumber(viewingToken.outputTokens || 0) }} Token</span
          >
        </div>
        <div class="modal-footer-actions">
          <div></div>
          <button
            class="tool-button"
            type="button"
            @click="viewingToken = null"
          >
            关闭
          </button>
        </div>
      </div>
    </BaseModal>

    <!-- 弹框 6: 删除 Token 确认弹框 -->
    <BaseModal
      v-if="deletingToken"
      title="删除调用 Token"
      :description="`删除“${deletingToken.name}”后，使用它的客户端会立即失去访问权限。`"
      @close="deletingToken = null"
    >
      <div class="delete-actions">
        <button class="tool-button" type="button" @click="deletingToken = null">
          取消
        </button>
        <button
          class="tool-button danger"
          type="button"
          :disabled="busy"
          @click="deleteToken"
        >
          确认删除
        </button>
      </div>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from "vue"
import {
  AlertTriangle,
  ArrowDownToLine,
  ArrowRight,
  ArrowUpFromLine,
  BarChart3,
  Boxes,
  Code2,
  Copy,
  Download,
  Eye,
  Info,
  KeyRound,
  Layers,
  ListFilter,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  RotateCw,
  Save,
  Search,
  ServerCog,
  SlidersHorizontal,
  Square,
  Trash2,
  Upload,
  Waypoints,
  X
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import GatewayConnectionPanel from "@/features/tools/multiProtocolGateway/components/GatewayConnectionPanel.vue"
import { multiProtocolGatewayApi } from "@/api"
import { formatDateTime, formatTokenCount } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] }
})

const tabs = [
  { id: "config", label: "配置与模型", icon: Layers },
  { id: "connections", label: "接入地址", icon: Code2 },
  { id: "tokens", label: "调用 Token", icon: KeyRound },
  { id: "logs", label: "用量与追踪", icon: BarChart3 }
]

const gateway = ref({})
const activeTab = ref("config")
const loading = ref(true)
const busy = ref(false)
const loadError = ref("")
const createdToken = ref("")
const deletingToken = ref(null)
const viewingToken = ref(null)
const lanBannerDismissed = ref(false)

const draft = reactive({
  host: "127.0.0.1",
  port: 8046,
  providerIds: [],
  modelMappings: {}
})

// 兼容单元测试中 setup.mapping 与 setup.addMapping()
const mapping = reactive({ alias: "", targetId: "", model: "" })
const tokenDraft = reactive({ name: "", description: "", expiresAt: "" })

// 弹框控制状态
const showListenerModal = ref(false)
const listenerDraft = reactive({ host: "127.0.0.1", port: 8046 })

const showAddModelModal = ref(false)
const editingModelAlias = ref("")
const modelForm = reactive({ alias: "", targetId: "", model: "" })

const showImportModal = ref(false)
const selectedImportModelIds = ref([])

const showCreateTokenModal = ref(false)

// 筛选与分页状态
const modelFilter = reactive({
  search: "",
  targetId: "all"
})
const modelPage = ref(1)
const modelPageSize = ref(10)

const tokenFilter = reactive({
  name: "",
  description: "",
  expiry: "all"
})
const tokenPage = ref(1)
const pageSize = 10

const logTimeFilter = ref("7d")
const logSearch = ref("")
const logPage = ref(1)

const defaultBaseUrl = computed(() => {
  const host = draft.host === "0.0.0.0" ? "0.0.0.0" : draft.host || "127.0.0.1"
  return `http://${host}:${draft.port || 8046}`
})

const isLanMode = computed(() => draft.host === "0.0.0.0")

const targets = computed(() => [
  ...props.providers
    .filter((item) => item.enabled !== false)
    .map((item) => ({
      id: item.id,
      name: item.name || item.id,
      cli: item.cli || item.type,
      type:
        item.type === "google-account"
          ? "Gemini 官方账号"
          : `${item.cli || item.type} Provider`
    })),
  ...props.codexAccounts
    .filter((item) => !item.disabled)
    .map((item) => ({
      id: `account:${item.id}`,
      name: item.email || item.id,
      cli: "codex",
      type: "Codex 官方账号"
    }))
])

const selectedTargets = computed(() =>
  targets.value.filter((item) => draft.providerIds.includes(item.id))
)

const metrics = computed(() => [
  {
    label: "累计请求",
    value: formatNumber(gateway.value.usage?.requestCount || 0),
    icon: Waypoints,
    type: "request"
  },
  {
    label: "失败请求",
    value: formatNumber(gateway.value.usage?.failureCount || 0),
    icon: AlertTriangle,
    type: "failure"
  },
  {
    label: "输入 Token",
    value: formatTokenCount(gateway.value.usage?.inputTokens || 0),
    icon: ArrowDownToLine,
    type: "input"
  },
  {
    label: "输出 Token",
    value: formatTokenCount(gateway.value.usage?.outputTokens || 0),
    icon: ArrowUpFromLine,
    type: "output"
  }
])

const usageGroups = computed(() => [
  { label: "调用方", items: gateway.value.byToken || [] },
  { label: "底层目标", items: gateway.value.byProvider || [] },
  { label: "真实模型", items: gateway.value.byModel || [] }
])

const modelEntries = computed(() => {
  const list = []
  for (const [alias, rule] of Object.entries(draft.modelMappings || {})) {
    const target = targets.value.find((t) => t.id === rule.targetId)
    list.push({
      alias,
      id: alias,
      targetId: rule.targetId,
      targetName: target
        ? `${target.name} (${target.cli || target.type})`
        : rule.targetId,
      realModel: rule.model,
      createdAt: rule.createdAt || null
    })
  }
  return list
})

const filteredConfigModels = computed(() => {
  let list = modelEntries.value
  if (modelFilter.targetId !== "all") {
    list = list.filter((m) => m.targetId === modelFilter.targetId)
  }
  if (modelFilter.search) {
    const q = modelFilter.search.toLowerCase()
    list = list.filter(
      (m) =>
        m.alias.toLowerCase().includes(q) ||
        (m.realModel && m.realModel.toLowerCase().includes(q)) ||
        (m.targetName && m.targetName.toLowerCase().includes(q))
    )
  }
  return list
})

const totalConfigModelPages = computed(() =>
  Math.max(
    1,
    Math.ceil(filteredConfigModels.value.length / modelPageSize.value)
  )
)

const paginatedConfigModels = computed(() => {
  const start = (modelPage.value - 1) * modelPageSize.value
  return filteredConfigModels.value.slice(start, start + modelPageSize.value)
})

const modelTargetOptions = computed(() => {
  const map = new Map()
  for (const item of modelEntries.value) {
    const key = item.targetId
    if (!map.has(key)) {
      map.set(key, { id: key, name: item.targetName, count: 0 })
    }
    map.get(key).count++
  }
  return Array.from(map.values())
})

function resetModelFilter() {
  modelFilter.search = ""
  modelFilter.targetId = "all"
  modelPage.value = 1
}

const importableModels = computed(() => {
  const existingAliases = new Set(Object.keys(draft.modelMappings || {}))
  const poolIds = new Set(draft.providerIds || [])
  const list = []
  for (const m of gateway.value.models || []) {
    if (!existingAliases.has(m.id)) {
      const target = targets.value.find(
        (t) => t.id === m.owned_by || t.name === m.owned_by
      )
      list.push({
        id: m.id,
        name: m.name || m.displayName || m.id,
        targetId:
          target?.id || (poolIds.size ? poolIds.values().next().value : ""),
        targetName: target?.name || m.owned_by || "底层池"
      })
    }
  }
  return list
})

const filteredTokens = computed(() => {
  let list = gateway.value.tokens || []
  if (tokenFilter.name) {
    const q = tokenFilter.name.toLowerCase()
    list = list.filter((t) => t.name?.toLowerCase().includes(q))
  }
  if (tokenFilter.description) {
    const q = tokenFilter.description.toLowerCase()
    list = list.filter((t) => t.description?.toLowerCase().includes(q))
  }
  if (tokenFilter.expiry === "permanent") {
    list = list.filter((t) => !t.expiresAt || t.expiresAt === 0)
  } else if (tokenFilter.expiry === "active") {
    const now = Date.now()
    list = list.filter((t) => !t.expiresAt || t.expiresAt > now)
  } else if (tokenFilter.expiry === "expired") {
    const now = Date.now()
    list = list.filter((t) => t.expiresAt && t.expiresAt <= now)
  }
  return list
})

const paginatedTokens = computed(() => {
  const start = (tokenPage.value - 1) * pageSize
  return filteredTokens.value.slice(start, start + pageSize)
})

const timeFilteredLogs = computed(() => {
  const logs = gateway.value.logs || []
  const now = Date.now()
  if (logTimeFilter.value === "24h") {
    return logs.filter((l) => (l.createdAt || 0) >= now - 24 * 3600 * 1000)
  }
  if (logTimeFilter.value === "7d") {
    return logs.filter((l) => (l.createdAt || 0) >= now - 7 * 24 * 3600 * 1000)
  }
  if (logTimeFilter.value === "30d") {
    return logs.filter((l) => (l.createdAt || 0) >= now - 30 * 24 * 3600 * 1000)
  }
  return logs
})

const overviewStats = computed(() => {
  if (logTimeFilter.value === "all" || !gateway.value.logs?.length) {
    const u = gateway.value.usage || {}
    return {
      totalRequests: u.requestCount || 0,
      successRequests:
        u.successCount ||
        Math.max(0, (u.requestCount || 0) - (u.failureCount || 0)),
      failureRequests: u.failureCount || 0,
      totalTokens: (u.inputTokens || 0) + (u.outputTokens || 0),
      inputTokens: u.inputTokens || 0,
      outputTokens: u.outputTokens || 0
    }
  }
  let total = 0
  let success = 0
  let failure = 0
  let inTokens = 0
  let outTokens = 0
  for (const log of timeFilteredLogs.value) {
    total++
    if (log.ok) success++
    else failure++
    inTokens += log.inputTokens || 0
    outTokens += log.outputTokens || 0
  }
  return {
    totalRequests: total,
    successRequests: success,
    failureRequests: failure,
    totalTokens: inTokens + outTokens,
    inputTokens: inTokens,
    outputTokens: outTokens
  }
})

const formattedDateRange = computed(() => {
  const now = new Date()
  const endStr = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`
  const days =
    logTimeFilter.value === "24h"
      ? 1
      : logTimeFilter.value === "30d"
        ? 30
        : logTimeFilter.value === "all"
          ? 365
          : 7
  const start = new Date(now.getTime() - days * 24 * 3600 * 1000)
  const startStr = `${start.getFullYear()}-${String(start.getMonth() + 1).padStart(2, "0")}-${String(start.getDate()).padStart(2, "0")}`
  return `${startStr} ~ ${endStr}`
})

const usageAttributionRows = computed(() => {
  const logs = gateway.value.logs || []
  if (!logs.length) {
    const byProv = gateway.value.byProvider || []
    return byProv.map((p) => ({
      key: p.key,
      caller: p.name || p.key,
      target: p.name || p.key,
      model: "—",
      requestCount: p.requestCount || 0,
      inputTokens: p.inputTokens || 0,
      outputTokens: p.outputTokens || 0
    }))
  }
  const map = new Map()
  for (const log of logs) {
    const caller = log.tokenName || "直接调用"
    const target = log.providerName || "未路由"
    const model = log.physicalModel || log.requestedModel || "—"
    const key = `${caller}__${target}__${model}`
    if (!map.has(key)) {
      map.set(key, {
        key,
        caller,
        target,
        model,
        requestCount: 0,
        inputTokens: 0,
        outputTokens: 0
      })
    }
    const item = map.get(key)
    item.requestCount++
    item.inputTokens += log.inputTokens || 0
    item.outputTokens += log.outputTokens || 0
  }
  return Array.from(map.values())
})

const filteredLogs = computed(() => {
  let list = gateway.value.logs || []
  if (logSearch.value) {
    const q = logSearch.value.toLowerCase()
    list = list.filter(
      (l) =>
        l.requestedModel?.toLowerCase().includes(q) ||
        l.physicalModel?.toLowerCase().includes(q) ||
        l.tokenName?.toLowerCase().includes(q) ||
        l.providerName?.toLowerCase().includes(q) ||
        l.errorMessage?.toLowerCase().includes(q)
    )
  }
  return list
})

const paginatedLogs = computed(() => {
  const start = (logPage.value - 1) * pageSize
  return filteredLogs.value.slice(start, start + pageSize)
})

function formatNumber(num) {
  if (num === null || num === undefined) return "0"
  return new Intl.NumberFormat("zh-CN").format(Number(num) || 0)
}

function getModelBrand(alias = "") {
  const lower = String(alias).toLowerCase()
  if (lower.includes("claude") || lower.includes("anthropic")) {
    return { name: "Anthropic", initial: "A", class: "anthropic" }
  }
  if (lower.includes("gemini") || lower.includes("google")) {
    return { name: "Google", initial: "G", class: "gemini" }
  }
  if (lower.includes("codex")) {
    return { name: "Codex", initial: "</>", class: "codex" }
  }
  if (
    lower.includes("gpt") ||
    lower.includes("openai") ||
    lower.includes("o1") ||
    lower.includes("o3")
  ) {
    return { name: "OpenAI", initial: "O", class: "openai" }
  }
  if (lower.includes("doubao")) {
    return { name: "Doubao", initial: "D", class: "doubao" }
  }
  return { name: "Model", initial: "M", class: "generic" }
}

function applyState(result, resetDraft = false) {
  gateway.value = result
  if (resetDraft) {
    draft.host = result.host || "127.0.0.1"
    draft.port = result.port || 8046
    draft.providerIds = Array.isArray(result.providerIds)
      ? [...result.providerIds]
      : []
    draft.modelMappings = result.modelMappings
      ? structuredClone(result.modelMappings)
      : {}
    listenerDraft.host = draft.host
    listenerDraft.port = draft.port
  }
}

async function load() {
  loading.value = true
  loadError.value = ""
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
  return {
    host: draft.host,
    port: draft.port,
    providerIds: [...draft.providerIds],
    modelMappings: JSON.parse(JSON.stringify(draft.modelMappings))
  }
}

function saveConfig() {
  return perform(
    () => multiProtocolGatewayApi.saveConfig(configPayload()),
    "网关配置已保存",
    true
  )
}

function toggleGateway() {
  const wasEnabled = gateway.value.enabled
  return perform(
    () =>
      wasEnabled
        ? multiProtocolGatewayApi.disable()
        : multiProtocolGatewayApi.enable(configPayload()),
    wasEnabled ? "网关已关闭" : "网关已启动",
    true
  )
}

async function toggleLanMode() {
  const nextHost = isLanMode.value ? "127.0.0.1" : "0.0.0.0"
  draft.host = nextHost
  listenerDraft.host = nextHost
  await saveConfig()
}

function openListenerModal() {
  listenerDraft.host = draft.host || gateway.value.host || "127.0.0.1"
  listenerDraft.port = draft.port || gateway.value.port || 8046
  showListenerModal.value = true
}

async function saveListenerConfig() {
  draft.host = listenerDraft.host
  draft.port = listenerDraft.port
  const res = await saveConfig()
  if (res) {
    showListenerModal.value = false
  }
}

function resetListenerDefaults() {
  listenerDraft.host = "127.0.0.1"
  listenerDraft.port = 8046
}

function onProviderToggle() {
  saveConfig()
}

// 供单元测试调用
function addMapping() {
  draft.modelMappings[mapping.alias] = {
    targetId: mapping.targetId,
    model: mapping.model
  }
  mapping.alias = ""
  mapping.model = ""
}

function openAddModelModal() {
  editingModelAlias.value = ""
  modelForm.alias = ""
  modelForm.targetId =
    selectedTargets.value[0]?.id || targets.value[0]?.id || ""
  modelForm.model = ""
  showAddModelModal.value = true
}

function openEditModelModal(item) {
  editingModelAlias.value = item.alias
  modelForm.alias = item.alias
  modelForm.targetId = item.targetId
  modelForm.model = item.realModel
  showAddModelModal.value = true
}

async function saveModelMapping() {
  if (!modelForm.alias || !modelForm.targetId || !modelForm.model) {
    createMessage.error("请完整填写模型信息")
    return
  }
  draft.modelMappings[modelForm.alias] = {
    targetId: modelForm.targetId,
    model: modelForm.model,
    createdAt: draft.modelMappings[modelForm.alias]?.createdAt || Date.now()
  }
  showAddModelModal.value = false
  await saveConfig()
}

async function removeModelMapping(alias) {
  delete draft.modelMappings[alias]
  await saveConfig()
}

function openImportModal() {
  selectedImportModelIds.value = []
  showImportModal.value = true
}

async function executeImportModels() {
  for (const id of selectedImportModelIds.value) {
    const item = importableModels.value.find((m) => m.id === id)
    if (item) {
      draft.modelMappings[item.id] = {
        targetId: item.targetId || draft.providerIds[0] || "",
        model: item.id,
        createdAt: Date.now()
      }
    }
  }
  selectedImportModelIds.value = []
  showImportModal.value = false
  await saveConfig()
}

async function submitCreateToken() {
  await createToken()
  showCreateTokenModal.value = false
}

async function createToken() {
  const result = await perform(
    () =>
      multiProtocolGatewayApi.createToken({
        name: tokenDraft.name,
        description: tokenDraft.description,
        expiresAt: tokenDraft.expiresAt
          ? new Date(tokenDraft.expiresAt).getTime()
          : 0
      }),
    "调用 Token 已创建"
  )
  if (result) {
    tokenDraft.name = ""
    tokenDraft.description = ""
    tokenDraft.expiresAt = ""
  }
}

function resetTokenFilter() {
  tokenFilter.name = ""
  tokenFilter.description = ""
  tokenFilter.expiry = "all"
  tokenPage.value = 1
}

function setTokenEnabled(token) {
  return perform(() =>
    multiProtocolGatewayApi.setTokenEnabled({
      tokenId: token.id,
      enabled: !token.enabled
    })
  )
}

async function deleteToken() {
  const result = await perform(
    () =>
      multiProtocolGatewayApi.deleteToken({ tokenId: deletingToken.value.id }),
    "调用 Token 已删除"
  )
  if (result) deletingToken.value = null
}

function exportLogs() {
  const logs = gateway.value.logs || []
  if (!logs.length) {
    createMessage.error("暂无请求记录可导出")
    return
  }
  const dataStr =
    "data:text/json;charset=utf-8," +
    encodeURIComponent(JSON.stringify(logs, null, 2))
  const downloadAnchor = document.createElement("a")
  downloadAnchor.setAttribute("href", dataStr)
  downloadAnchor.setAttribute("download", `gateway-logs-${Date.now()}.json`)
  document.body.appendChild(downloadAnchor)
  downloadAnchor.click()
  downloadAnchor.remove()
  createMessage.success("已导出请求记录")
}

async function copyText(value) {
  try {
    await navigator.clipboard.writeText(value)
    createMessage.success("已复制")
  } catch (error) {
    createMessage.error(error.message || "复制失败")
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

  .spin {
    animation: gw-spin 1s linear infinite;
  }

  @keyframes gw-spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .tool-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    flex-shrink: 0;
    padding: 7px 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    white-space: nowrap;
    font-size: 12px;
    transition:
      background 0.15s,
      border-color 0.15s,
      color 0.15s;

    &.primary {
      color: #fff;
      border-color: #7c3aed;
      background: #7c3aed;

      &:hover:not(:disabled) {
        background: #6d28d9;
        border-color: #6d28d9;
      }
    }
    &.danger {
      color: #ef4444;
      border-color: rgba(239, 68, 68, 0.3);
      background: rgba(239, 68, 68, 0.06);

      &:hover:not(:disabled) {
        background: rgba(239, 68, 68, 0.12);
      }
    }
    &:disabled {
      opacity: 0.5;
      cursor: wait;
    }
    &:hover:not(:disabled):not(.primary):not(.danger) {
      border-color: #7c3aed;
      color: #7c3aed;
    }
  }

  .icon-action-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;

    &:hover:not(:disabled) {
      color: #7c3aed;
      border-color: #7c3aed;
    }
  }

  /* 顶部标题栏 */
  .gateway-top-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;

    .top-title-group {
      display: flex;
      align-items: center;
      gap: 12px;

      .top-icon-box {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 38px;
        height: 38px;
        border-radius: 10px;
        color: #7c3aed;
        background: rgba(124, 58, 237, 0.1);
      }
      .top-text-box {
        .top-title {
          margin: 0 0 3px;
          font-size: 17px;
          color: var(--color-text);
        }
        .top-subtitle {
          margin: 0;
          color: var(--color-text-muted);
          font-size: 12px;
        }
      }
    }
  }

  /* 统一模型网关卡片 */
  .gateway-service-card {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 20px;
    padding: 18px 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);

    .service-left {
      display: flex;
      align-items: center;
      gap: 14px;

      .service-badge-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 44px;
        height: 44px;
        border-radius: 10px;
        color: #7c3aed;
        background: rgba(124, 58, 237, 0.1);
      }
      .service-meta {
        .service-title-line {
          display: flex;
          align-items: center;
          gap: 10px;
          margin-bottom: 4px;

          .service-name {
            margin: 0;
            color: var(--color-text);
            font-size: 16px;
          }
          .service-status-pill {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            padding: 2px 8px;
            border: 1px solid var(--color-line);
            border-radius: 20px;
            color: var(--color-text-muted);
            font-size: 11px;

            &.active {
              color: #10b981;
              border-color: rgba(16, 185, 129, 0.25);
              background: rgba(16, 185, 129, 0.08);
            }
            .status-dot {
              width: 6px;
              height: 6px;
              border-radius: 50%;
              background: currentColor;
            }
          }
        }
        .service-url {
          margin: 0;
          color: var(--color-text-muted);
          font-size: 12px;
        }
      }
    }

    .service-right {
      display: flex;
      align-items: center;
      gap: 14px;

      .switch-item {
        display: flex;
        align-items: center;
        gap: 8px;
        cursor: pointer;

        .switch-label {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          color: var(--color-text-muted);
          font-size: 12px;
          white-space: nowrap;

          .info-icon {
            color: var(--color-text-muted);
          }
        }

        .switch-control {
          position: relative;
          display: inline-block;
          width: 36px;
          height: 20px;
          padding: 0;
          border: 0;
          border-radius: 10px;
          background: var(--color-line);
          cursor: pointer;
          transition: background 0.2s;

          &.checked {
            background: #7c3aed;

            .switch-thumb {
              transform: translateX(16px);
            }
          }
          &:disabled {
            opacity: 0.5;
            cursor: wait;
          }

          .switch-thumb {
            position: absolute;
            top: 2px;
            left: 2px;
            width: 16px;
            height: 16px;
            border-radius: 50%;
            background: #fff;
            transition: transform 0.2s;
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
          }
        }
      }
    }
  }

  /* 局域网提醒条 */
  .lan-alert-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 16px;
    border: 1px solid rgba(124, 58, 237, 0.25);
    border-radius: 8px;
    background: rgba(124, 58, 237, 0.06);
    color: #7c3aed;
    font-size: 12px;

    .banner-content {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .banner-close-btn {
      display: flex;
      padding: 2px;
      border: 0;
      border-radius: 4px;
      color: #7c3aed;
      background: transparent;
      cursor: pointer;
      &:hover {
        background: rgba(124, 58, 237, 0.15);
      }
    }
  }

  .gateway-error {
    margin: 0;
    color: #ef4444;
    font-size: 12px;
  }

  /* 4 个指标统计卡片 */
  .gateway-metrics {
    display: flex;
    gap: 12px;

    .metric-item {
      display: flex;
      flex: 1;
      min-width: 0;
      flex-direction: column;
      gap: 10px;
      padding: 14px 16px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);

      .metric-top {
        display: flex;
        align-items: center;
        gap: 8px;

        .metric-icon {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          padding: 6px;
          border-radius: 6px;
        }
        .metric-label {
          font-size: 12px;
          color: var(--color-text-muted);
        }
      }
      .metric-value {
        font-size: 22px;
        overflow-wrap: anywhere;
        color: var(--color-text);
      }

      &.request .metric-icon {
        color: #7c3aed;
        background: rgba(124, 58, 237, 0.08);
      }
      &.failure .metric-icon {
        color: #ef4444;
        background: rgba(239, 68, 68, 0.08);
      }
      &.input .metric-icon {
        color: #2563eb;
        background: rgba(37, 99, 235, 0.08);
      }
      &.output .metric-icon {
        color: #059669;
        background: rgba(5, 150, 105, 0.08);
      }
    }
  }

  /* 页签导航 */
  .gateway-tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--color-line);

    .tab-button {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      padding: 10px 16px;
      border: 0;
      border-bottom: 2px solid transparent;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      font-size: 13px;
      transition:
        color 0.15s,
        border-color 0.15s;

      &:hover {
        color: var(--color-text);
      }
      &.active {
        color: #7c3aed;
        border-bottom-color: #7c3aed;
      }
    }
  }

  /* 页签主体内容区 */
  .gateway-content {
    display: flex;
    flex-direction: column;
    gap: 16px;

    .gateway-panel {
      padding: 18px 20px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);

      .panel-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        margin-bottom: 16px;

        .heading-title-group {
          display: flex;
          align-items: center;
          gap: 8px;

          .heading-icon {
            color: #7c3aed;
          }
          .panel-title {
            margin: 0;
            font-size: 15px;
            color: var(--color-text);
          }
        }
        .panel-detail {
          color: var(--color-text-muted);
          font-size: 12px;
        }
        .heading-actions {
          display: flex;
          align-items: center;
          gap: 8px;
        }
      }

      /* 底层池列表 (兼容测试选择器链) */
      .target-list {
        display: flex;
        flex-wrap: wrap;
        gap: 10px;

        .target-item {
          display: flex;
          align-items: center;
          gap: 10px;
          width: 250px;
          padding: 12px 14px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel-soft);
          cursor: pointer;
          transition:
            border-color 0.15s,
            background 0.15s;

          &.selected {
            border-color: #7c3aed;
            background: rgba(124, 58, 237, 0.05);
          }
          .target-checkbox {
            accent-color: #7c3aed;
          }
          .target-info {
            display: flex;
            flex: 1;
            min-width: 0;
            flex-direction: column;
            gap: 3px;

            .target-name {
              font-size: 13px;
              color: var(--color-text);
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
            .target-type {
              color: var(--color-text-muted);
              font-size: 11px;
            }
          }
          .target-status-badge {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            color: #10b981;
            font-size: 11px;
            white-space: nowrap;

            .status-dot {
              width: 5px;
              height: 5px;
              border-radius: 50%;
              background: #10b981;
            }
          }
        }
      }

      /* 支持模型筛选栏 */
      .model-filter-bar {
        display: flex;
        align-items: center;
        gap: 10px;
        margin-bottom: 14px;

        .search-input-wrap {
          position: relative;
          display: flex;
          align-items: center;
          flex: 1;

          .filter-icon {
            position: absolute;
            left: 10px;
            color: var(--color-text-muted);
          }
          .model-search-input {
            width: 100%;
            padding: 7px 10px 7px 30px;
            box-sizing: border-box;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: 12px;
            outline: none;

            &:focus {
              border-color: #7c3aed;
            }
          }
        }

        .target-select-wrap {
          .target-select {
            padding: 7px 10px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: 12px;
            outline: none;
            cursor: pointer;

            &:focus {
              border-color: #7c3aed;
            }
          }
        }
      }

      /* 支持模型表格 */
      .model-table-wrap {
        overflow-x: auto;

        .model-table {
          width: 100%;
          border-collapse: collapse;

          th {
            padding: 9px 12px;
            border-bottom: 1px solid var(--color-line);
            text-align: left;
            color: var(--color-text-muted);
            font-size: 11px;

            &.action-th {
              text-align: right;
            }
          }
          td {
            padding: 12px;
            border-bottom: 1px solid var(--color-line);
            color: var(--color-text);
            font-size: 12px;

            &.action-td {
              text-align: right;
            }
          }

          .model-name-cell {
            display: flex;
            align-items: center;
            gap: 8px;

            .brand-avatar {
              display: inline-flex;
              align-items: center;
              justify-content: center;
              width: 24px;
              height: 24px;
              border-radius: 6px;
              font-size: 11px;

              &.anthropic {
                color: #d97706;
                background: rgba(217, 119, 6, 0.1);
              }
              &.gemini {
                color: #2563eb;
                background: rgba(37, 99, 235, 0.1);
              }
              &.codex {
                color: #059669;
                background: rgba(5, 150, 105, 0.1);
              }
              &.openai {
                color: #10b981;
                background: rgba(16, 185, 129, 0.1);
              }
              &.doubao {
                color: #3b82f6;
                background: rgba(59, 130, 246, 0.1);
              }
              &.generic {
                color: #7c3aed;
                background: rgba(124, 58, 237, 0.1);
              }
            }
            .model-name-text {
              font-size: 13px;
            }
          }

          .model-id-code {
            color: #7c3aed;
            font-size: 11px;
          }
          .target-cell {
            display: flex;
            flex-direction: column;
            gap: 2px;

            .target-badge {
              font-size: 12px;
            }
            .real-model-hint {
              color: var(--color-text-muted);
              font-size: 10px;
            }
          }
          .time-text {
            color: var(--color-text-muted);
            font-size: 11px;
          }
        }
      }

      .empty-models-box {
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 32px 16px;
        text-align: center;

        .empty-icon {
          color: var(--color-text-muted);
          opacity: 0.5;
          margin-bottom: 10px;
        }
        .empty-title {
          margin: 0 0 6px;
          color: var(--color-text);
          font-size: 14px;
        }
        .empty-sub {
          margin: 0 0 16px;
          color: var(--color-text-muted);
          font-size: 12px;
        }
      }

      /* Token 筛选与列表 */
      .token-filter-bar {
        display: flex;
        align-items: center;
        gap: 10px;
        margin-bottom: 16px;

        .filter-field {
          position: relative;
          display: flex;
          align-items: center;
          flex: 1;

          .filter-icon {
            position: absolute;
            left: 10px;
            color: var(--color-text-muted);
          }
          .filter-input {
            width: 100%;
            padding: 7px 10px 7px 30px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: 12px;
            outline: none;

            &:focus {
              border-color: #7c3aed;
            }
          }
          .filter-select {
            width: 100%;
            padding: 7px 10px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: 12px;
            outline: none;
            cursor: pointer;

            &:focus {
              border-color: #7c3aed;
            }
          }
        }
        .filter-actions {
          display: flex;
          align-items: center;
          gap: 8px;
        }
      }

      .created-token-card {
        padding: 14px;
        margin-bottom: 16px;
        border: 1px solid #10b981;
        border-radius: 8px;
        background: rgba(16, 185, 129, 0.05);

        .created-card-top {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-bottom: 10px;

          .created-note {
            color: #10b981;
            font-size: 12px;
          }
        }
        .created-card-body {
          display: flex;
          align-items: center;
          gap: 12px;

          .created-value {
            flex: 1;
            padding: 8px 12px;
            border-radius: 6px;
            color: #10b981;
            background: var(--color-panel);
            overflow-wrap: anywhere;
            font-family: Consolas, monospace;
            font-size: 12px;
          }
        }
      }

      .token-table-wrap {
        overflow-x: auto;

        .token-table {
          width: 100%;
          border-collapse: collapse;

          th {
            padding: 9px 10px;
            border-bottom: 1px solid var(--color-line);
            text-align: left;
            color: var(--color-text-muted);
            font-size: 11px;

            &.action-th {
              text-align: right;
            }
          }
          td {
            padding: 12px 10px;
            border-bottom: 1px solid var(--color-line);
            color: var(--color-text);
            font-size: 12px;

            &.action-td {
              text-align: right;
            }
          }

          .token-name-col {
            display: flex;
            flex-direction: column;
            gap: 3px;

            .token-name {
              font-size: 13px;
            }
            .token-desc {
              color: var(--color-text-muted);
              font-size: 11px;
            }
          }

          .token-hint-col {
            display: flex;
            flex-direction: column;
            gap: 2px;

            .hint-row {
              display: flex;
              align-items: center;
              gap: 5px;

              .hint-text {
                font-family: Consolas, monospace;
                color: #7c3aed;
                font-size: 11px;
              }
              .hint-copy-btn {
                display: flex;
                padding: 2px;
                border: 0;
                background: transparent;
                color: var(--color-text-muted);
                cursor: pointer;
                &:hover {
                  color: #7c3aed;
                }
              }
            }
          }

          .token-stats-col {
            display: flex;
            flex-direction: column;
            gap: 3px;

            .stats-total {
              display: inline-flex;
              align-items: center;
              gap: 4px;
              color: var(--color-text);
              font-size: 11px;
            }
            .stats-detail {
              color: var(--color-text-muted);
              font-size: 10px;
            }
          }

          .number-text {
            color: var(--color-text);
            font-size: 12px;
          }
          .time-text {
            color: var(--color-text-muted);
            font-size: 11px;
          }

          .token-status-pill {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            padding: 2px 7px;
            border-radius: 12px;
            color: var(--color-text-muted);
            background: var(--color-panel-soft);
            font-size: 11px;

            &.enabled {
              color: #10b981;
              background: rgba(16, 185, 129, 0.08);

              .status-dot {
                background: #10b981;
              }
            }
            .status-dot {
              width: 5px;
              height: 5px;
              border-radius: 50%;
              background: currentColor;
            }
          }
        }
      }

      .empty-state-box {
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 36px 16px;
        text-align: center;

        .empty-icon {
          color: var(--color-text-muted);
          opacity: 0.5;
          margin-bottom: 10px;
        }
        .empty-title {
          margin: 0 0 6px;
          color: var(--color-text);
          font-size: 14px;
        }
        .empty-sub {
          margin: 0 0 16px;
          color: var(--color-text-muted);
          font-size: 12px;
        }
      }

      /* 最近请求工具栏与表格 */
      .logs-toolbar {
        display: flex;
        align-items: center;
        gap: 10px;

        .log-search-wrap {
          position: relative;
          display: flex;
          align-items: center;

          .filter-icon {
            position: absolute;
            left: 10px;
            color: var(--color-text-muted);
          }
          .log-search-input {
            width: 260px;
            padding: 6px 10px 6px 30px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: 11px;
            outline: none;

            &:focus {
              border-color: #7c3aed;
            }
          }
        }
      }

      .request-table-wrap {
        overflow-x: auto;

        .request-table {
          width: 100%;
          border-collapse: collapse;

          .request-head {
            th {
              padding: 9px 10px;
              border-bottom: 1px solid var(--color-line);
              text-align: left;
              color: var(--color-text-muted);
              font-size: 11px;
            }
          }

          .request-body {
            .request-row {
              td {
                padding: 10px;
                border-bottom: 1px solid var(--color-line);
                vertical-align: top;
                font-size: 11px;
              }

              .log-time {
                color: var(--color-text);
              }
              .cell-detail {
                display: block;
                margin-top: 3px;
                color: var(--color-text-muted);
                font-size: 10px;
              }
              .protocol-tag {
                display: inline-block;
                padding: 2px 6px;
                border-radius: 4px;
                font-size: 10px;
                color: #7c3aed;
                background: rgba(124, 58, 237, 0.08);

                &.openai {
                  color: #10b981;
                  background: rgba(16, 185, 129, 0.08);
                }
                &.claude {
                  color: #d97706;
                  background: rgba(217, 119, 6, 0.08);
                }
                &.gemini {
                  color: #2563eb;
                  background: rgba(37, 99, 235, 0.08);
                }
              }

              .routing-chain {
                display: flex;
                align-items: center;
                gap: 5px;

                .cell-model {
                  color: #7c3aed;
                }
                .cell-arrow {
                  color: var(--color-text-muted);
                }
                .cell-target {
                  color: var(--color-text);
                }
                .upstream-report {
                  display: inline-block;
                  padding: 1px 5px;
                  border-radius: 3px;
                  font-size: 9px;

                  &.ok {
                    color: #10b981;
                    background: rgba(16, 185, 129, 0.1);
                  }
                  &.error {
                    color: #ef4444;
                    background: rgba(239, 68, 68, 0.1);
                  }
                }
              }

              .log-provider-name {
                color: var(--color-text);
              }
              .token-in-out {
                color: var(--color-text);
              }
              .cell-muted {
                color: var(--color-text-muted);
              }

              .request-status-badge {
                display: inline-block;
                color: #10b981;

                &.failed {
                  color: #ef4444;
                }
              }
              .cell-error {
                display: block;
                margin-top: 4px;
                color: #ef4444;
                font-size: 10px;
                overflow-wrap: anywhere;
              }
            }
          }
        }
      }

      .empty-state {
        padding: 20px;
        margin: 0;
        color: var(--color-text-muted);
        font-size: 12px;
        text-align: center;
      }

      .pagination-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-top: 14px;
        padding-top: 12px;
        border-top: 1px solid var(--color-line);

        .total-text {
          color: var(--color-text-muted);
          font-size: 11px;
        }
        .page-controls {
          display: flex;
          align-items: center;
          gap: 6px;

          .page-size {
            color: var(--color-text-muted);
            font-size: 11px;
            margin-right: 6px;
          }
          .page-btn {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            width: 26px;
            height: 26px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            color: var(--color-text);
            background: var(--color-panel);
            cursor: pointer;
            font-size: 12px;

            &:disabled {
              opacity: 0.4;
              cursor: default;
            }
            &:hover:not(:disabled) {
              border-color: #7c3aed;
              color: #7c3aed;
            }
          }
          .page-num {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            width: 26px;
            height: 26px;
            border-radius: 5px;
            font-size: 12px;

            &.active {
              color: #fff;
              background: #7c3aed;
            }
          }
        }
      }
    }

    /* 用量与追踪页签: 顶部概览与归属两栏布局 */
    .usage-top-row {
      display: flex;
      gap: 16px;

      .usage-overview-panel {
        flex: 0 0 46%;

        .overview-controls {
          display: flex;
          align-items: center;
          gap: 8px;

          .overview-period-select {
            padding: 5px 10px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: 12px;
            outline: none;
            cursor: pointer;

            &:focus {
              border-color: #7c3aed;
            }
          }
          .date-range-badge {
            display: inline-flex;
            align-items: center;
            padding: 5px 10px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text-muted);
            background: var(--color-panel-soft);
            font-size: 11px;
            white-space: nowrap;
          }
        }

        .overview-stats-grid {
          display: grid;
          grid-template-columns: repeat(2, 1fr);
          gap: 12px;
          margin-top: 14px;

          .overview-tile {
            display: flex;
            flex-direction: column;
            gap: 6px;
            padding: 14px 16px;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            background: var(--color-panel-soft);
            box-sizing: border-box;

            .tile-label {
              color: var(--color-text-muted);
              font-size: 12px;
            }
            .tile-value {
              color: var(--color-text);
              font-size: 20px;
            }

            &.success .tile-value {
              color: #10b981;
            }
            &.failure .tile-value {
              color: #ef4444;
            }
            &.token .tile-value {
              color: #7c3aed;
            }
          }
        }
      }

      .usage-attribution-panel {
        flex: 1;
        min-width: 0;

        .attribution-table-wrap {
          max-height: 220px;
          overflow-y: auto;

          .attribution-table {
            width: 100%;
            border-collapse: collapse;

            th {
              padding: 8px 10px;
              border-bottom: 1px solid var(--color-line);
              text-align: left;
              color: var(--color-text-muted);
              font-size: 11px;
            }
            td {
              padding: 10px;
              border-bottom: 1px solid var(--color-line);
              color: var(--color-text);
              font-size: 12px;
            }

            .cell-caller {
              color: var(--color-text);
            }
            .target-badge {
              display: inline-block;
              padding: 2px 6px;
              border-radius: 4px;
              color: #7c3aed;
              background: rgba(124, 58, 237, 0.08);
              font-size: 11px;
            }
            .model-id-code {
              color: #2563eb;
              font-size: 11px;
            }
          }
        }
      }
    }
  }

  /* 表格内按钮组件 */
  .table-actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;

    .action-btn {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      padding: 4px 8px;
      border: 1px solid var(--color-line);
      border-radius: 5px;
      color: var(--color-text-muted);
      background: var(--color-panel-soft);
      cursor: pointer;
      font-size: 11px;
      transition:
        color 0.15s,
        border-color 0.15s;

      &:hover {
        color: #7c3aed;
        border-color: #7c3aed;
      }
      &.danger {
        color: #ef4444;
        &:hover {
          border-color: #ef4444;
          background: rgba(239, 68, 68, 0.08);
        }
      }
    }
  }

  /* 弹框表单 */
  .modal-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 6px;

    .field-row {
      display: flex;
      gap: 14px;
    }
    .field-column {
      display: flex;
      flex-direction: column;
      gap: 14px;
    }

    .form-field {
      display: flex;
      flex: 1;
      flex-direction: column;
      gap: 7px;

      .field-label {
        font-size: 12px;
        color: var(--color-text);
      }
      .field-input {
        width: 100%;
        padding: 8px 10px;
        box-sizing: border-box;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        color: var(--color-text);
        background: var(--color-panel);
        font-size: 12px;
        outline: none;

        &:focus {
          border-color: #7c3aed;
        }
      }
      .field-help {
        color: var(--color-text-muted);
        font-size: 11px;
        line-height: 1.4;
      }
    }
  }

  .modal-footer-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 14px;
    padding-top: 14px;
    border-top: 1px solid var(--color-line);

    .right-buttons {
      display: flex;
      align-items: center;
      gap: 8px;
    }
  }

  .import-modal-content {
    display: flex;
    flex-direction: column;
    gap: 14px;

    .import-models-list {
      max-height: 280px;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 8px;

      .import-model-item {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 10px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel-soft);
        cursor: pointer;

        &:hover {
          border-color: #7c3aed;
        }
        .import-model-meta {
          display: flex;
          flex-direction: column;
          gap: 2px;

          .import-name {
            font-size: 12px;
            color: var(--color-text);
          }
          .import-target {
            font-size: 10px;
            color: var(--color-text-muted);
          }
        }
      }
    }
  }

  .token-detail-view {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 6px;

    .detail-row {
      display: flex;
      align-items: center;
      gap: 10px;
      font-size: 12px;

      .detail-label {
        width: 90px;
        color: var(--color-text-muted);
      }
      .detail-value {
        color: var(--color-text);
      }
    }
  }

  .delete-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }

  :deep(.base-modal__panel) {
    max-width: 560px;
    width: 100%;
  }
}
</style>

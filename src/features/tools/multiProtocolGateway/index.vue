<template>
  <section class="gateway-tool">
    <!-- 管理页签导航 -->
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

    <!-- 加载错误提示 -->
    <p v-if="loadError" class="gateway-error" role="alert">{{ loadError }}</p>

    <!-- 接入地址 -->
    <section
      v-if="activeTab === 'connections'"
      class="gateway-content connections-view"
    >
      <!-- 统一模型网关控制卡片 -->
      <section class="gateway-service-card">
        <div class="service-left">
          <div class="service-badge-icon">
            <ServerCog :size="22" />
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

          <!-- 网关配置入口 -->
          <button
            class="tool-button"
            type="button"
            title="配置网关监听 IP 与端口"
            :disabled="busy || loading"
            @click="openListenerModal"
          >
            <SlidersHorizontal :size="13" />
            监听配置
          </button>

          <button
            class="tool-button"
            type="button"
            title="选择参与反代的 Provider 与官方账号"
            :disabled="busy || loading"
            @click="openPoolModal"
          >
            <Layers :size="13" />
            底层池
          </button>

          <!-- 刷新按钮 -->
          <button
            class="tool-button"
            type="button"
            :disabled="busy || loading"
            @click="load"
          >
            <RefreshCw :size="13" :class="{ spin: loading }" />
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

      <!-- 协议与模型集成面板 -->
      <GatewayConnectionPanel
        :base-url="gateway.baseUrl || ''"
        :enabled="gateway.enabled === true"
        :models="gateway.models || []"
        :model-mappings="gateway.modelMappings || {}"
        :targets="targets"
        @manage-tokens="activeTab = 'tokens'"
      />
    </section>

    <!-- 调用 Token -->
    <section
      v-else-if="activeTab === 'tokens'"
      class="gateway-content tokens-view"
    >
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

        <!-- Token 列表表格 -->
        <div v-if="filteredTokens.length > 0" class="token-table-wrap">
          <table class="token-table">
            <thead>
              <tr>
                <th>Token 名称</th>
                <th>API Key</th>
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
                  <div class="api-key-display">
                    <code class="api-key-value">{{
                      tokenValues[token.id] || token.maskedKey
                    }}</code>
                    <button
                      class="api-key-button"
                      type="button"
                      :title="
                        token.canReveal
                          ? tokenValues[token.id]
                            ? '隐藏 API Key'
                            : '预览 API Key'
                          : '旧 Token 仅保存校验值，请重新创建后复制或预览'
                      "
                      :aria-label="`${tokenValues[token.id] ? '隐藏' : '预览'} ${token.name} 的 API Key`"
                      :aria-pressed="!!tokenValues[token.id]"
                      :disabled="
                        !token.canReveal || pendingTokenIds.has(token.id)
                      "
                      @click="toggleTokenPreview(token)"
                    >
                      <component
                        :is="tokenValues[token.id] ? EyeOff : Eye"
                        :size="13"
                      />
                    </button>
                    <button
                      class="api-key-button"
                      type="button"
                      :title="
                        token.canReveal
                          ? '复制完整 API Key'
                          : '旧 Token 仅保存校验值，请重新创建后复制或预览'
                      "
                      :aria-label="`复制 ${token.name} 的完整 API Key`"
                      :disabled="
                        !token.canReveal || pendingTokenIds.has(token.id)
                      "
                      @click="copyToken(token)"
                    >
                      <Copy :size="13" />
                    </button>
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
                      :title="
                        token.canReveal
                          ? '复制完整 API Key'
                          : '旧 Token 仅保存校验值，请重新创建后复制或预览'
                      "
                      :disabled="
                        !token.canReveal || pendingTokenIds.has(token.id)
                      "
                      @click="copyToken(token)"
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

    <!-- 用量与追踪 -->
    <section v-else class="gateway-content usage-logs-view">
      <!-- 第一行：用量调度全景 + 账号/模型/用户 排行卡片 (Image 8) -->
      <div class="usage-dashboard-layout">
        <!-- 用量调度全景 -->
        <section class="usage-card usage-summary-card">
          <div class="usage-card-heading">
            <h3 class="usage-card-title">用量调度全景</h3>
            <div class="usage-period-control">
              <span>周期：</span>
              <select v-model="logTimeFilter" class="overview-period-select">
                <option value="24h">24 小时</option>
                <option value="7d">7 天</option>
                <option value="30d">30 天</option>
                <option value="all">全部</option>
              </select>
            </div>
          </div>
          <div class="usage-summary-body">
            <div class="usage-summary-copy">
              <span class="usage-metric-label">总消耗量</span>
              <div class="usage-total-line">
                <strong>{{ formatCompact(overviewStats.totalTokens) }}</strong>
                <span>Tokens</span>
              </div>
              <div class="usage-breakdown-line">
                <span
                  >入: {{ formatCompact(overviewStats.inputTokens) }} ({{
                    formatPercent(overviewStats.inputShare)
                  }})</span
                >
                <i>/</i>
                <span
                  >出: {{ formatCompact(overviewStats.outputTokens) }} ({{
                    formatPercent(overviewStats.outputShare)
                  }})</span
                >
                <span
                  class="usage-thinking-pill"
                  :class="{ empty: !overviewStats.reasoningTokens }"
                >
                  思: {{ formatCompact(overviewStats.reasoningTokens) }} ({{
                    formatPercent(overviewStats.reasoningShare)
                  }})
                </span>
              </div>
              <div v-if="topModelRows.length" class="usage-model-list">
                <div
                  v-for="(item, index) in topModelRows"
                  :key="item.key"
                  class="usage-model-row"
                >
                  <span
                    class="usage-model-dot"
                    :class="`tone-${index + 1}`"
                  ></span>
                  <code>{{ item.name }}</code>
                  <span class="model-tokens">{{
                    formatCompact(item.totalTokens)
                  }}</span>
                  <strong class="model-share">{{
                    formatPercent(item.share)
                  }}</strong>
                </div>
              </div>
              <p v-else class="usage-empty-inline">暂无已报告 Token 用量</p>
            </div>

            <div class="usage-token-orbit" aria-label="Token 消耗总览">
              <svg
                class="sunburst-svg"
                viewBox="0 0 180 180"
                aria-hidden="true"
              >
                <defs>
                  <linearGradient
                    id="sunburst-glow"
                    x1="0%"
                    y1="0%"
                    x2="100%"
                    y2="100%"
                  >
                    <stop offset="0%" stop-color="#10b981" />
                    <stop offset="50%" stop-color="#06b6d4" />
                    <stop offset="100%" stop-color="#3b82f6" />
                  </linearGradient>
                </defs>
                <g class="sunburst-rays">
                  <line
                    v-for="(ray, idx) in sunburstRays"
                    :key="idx"
                    :x1="ray.x1"
                    :y1="ray.y1"
                    :x2="ray.x2"
                    :y2="ray.y2"
                    stroke="url(#sunburst-glow)"
                    stroke-width="2"
                    stroke-linecap="round"
                    opacity="0.85"
                  />
                </g>
              </svg>
              <div class="usage-token-orbit-core">
                <strong>{{ formatCompact(overviewStats.totalTokens) }}</strong>
                <span>SUMMARY</span>
              </div>
            </div>

            <div class="usage-summary-side">
              <div class="summary-metric-block">
                <div class="metric-top-row">
                  <span class="usage-metric-label">预估支出</span>
                  <button
                    type="button"
                    class="pricing-matrix-link"
                    @click="showPricingModal = true"
                  >
                    定价矩阵 ↗
                  </button>
                </div>
                <div class="metric-value-row">
                  <strong class="cost-accent"
                    >${{ overviewStats.estimatedCostFormatted }}</strong
                  >
                </div>
                <small class="cost-subtext"
                  >均次 ~${{ overviewStats.costPerReqFormatted }} / req</small
                >
              </div>

              <div class="summary-metric-block">
                <div class="metric-top-row">
                  <span class="usage-metric-label">缓存节省</span>
                </div>
                <div class="metric-value-row">
                  <strong class="savings-accent"
                    >+${{ overviewStats.estimatedSavingsFormatted }}</strong
                  >
                  <span class="cache-hit-pill"
                    >{{ formatPercent(overviewStats.cacheRate) }} 命中</span
                  >
                </div>
              </div>

              <div
                class="gateway-live-indicator"
                :class="{ active: gateway.enabled }"
              >
                <span class="live-dot"></span>
                <span>{{
                  gateway.enabled ? "网关调用正常" : "网关服务未开启"
                }}</span>
              </div>
            </div>
          </div>
        </section>

        <!-- 排行卡片 (账号 / 模型 / 用户 - Image 8) -->
        <section class="usage-card usage-ranking-card">
          <div class="usage-ranking-tabs" role="tablist">
            <button
              v-for="tab in usageRankingTabs"
              :key="tab.id"
              type="button"
              class="usage-ranking-tab"
              :class="{ active: usageRankingTab === tab.id }"
              @click="usageRankingTab = tab.id"
            >
              {{ tab.label }}
            </button>
          </div>

          <div v-if="usageRankingRows.length" class="usage-ranking-list">
            <div
              v-for="(item, index) in usageRankingRows"
              :key="item.key"
              class="usage-ranking-row"
            >
              <div class="usage-ranking-main">
                <div class="usage-ranking-line">
                  <div class="ranking-name-col">
                    <span
                      class="usage-ranking-number"
                      :class="{ 'rank-1': index === 0 }"
                    >
                      {{ index + 1 }}
                    </span>
                    <span class="ranking-entity-name" :title="item.name">{{
                      item.name
                    }}</span>
                    <span v-if="index === 0" class="ranking-pill-primary"
                      >主力</span
                    >
                  </div>
                  <strong class="ranking-tokens">{{
                    formatCompact(item.totalTokens)
                  }}</strong>
                </div>

                <div class="usage-ranking-progress">
                  <span
                    :style="{ width: `${Math.max(item.share, 4)}%` }"
                  ></span>
                </div>

                <div class="usage-ranking-meta">
                  <span
                    >占比: {{ formatPercent(item.share) }} ({{
                      item.requestCount
                    }}
                    次)</span
                  >
                  <span>~${{ item.costFormatted }}</span>
                </div>
              </div>
            </div>
          </div>
          <p v-else class="usage-empty-inline">暂无调用记录</p>

          <div class="usage-ranking-footer">
            <span>{{ rankingFooterNote }}</span>
            <span class="ranking-live-status"> <i></i> 已激活 </span>
          </div>
        </section>
      </div>

      <!-- 第二行：活跃度 91 天全景 (Image 7) + 24小时活跃律动 + 效能指数 · J23 -->
      <div class="usage-insight-grid activity-efficiency-grid">
        <!-- 活跃度 91 天全景 (Image 7) -->
        <section class="usage-card usage-heatmap-card">
          <div class="usage-card-heading compact">
            <div class="usage-heading-inline">
              <Calendar :size="15" class="heatmap-icon" />
              <h3 class="usage-card-title">活跃度</h3>
              <span class="usage-card-note">91 天全景</span>
            </div>
            <span class="heatmap-streak-pill">
              <Flame :size="12" /> 连续 {{ heatmapSummary.streak }} 天
            </span>
          </div>

          <div class="usage-heatmap-body">
            <div class="heatmap-left-col">
              <div class="heatmap-grid" aria-label="91 天活跃全景">
                <span
                  v-for="day in heatmapDays"
                  :key="day.date"
                  class="heatmap-cell"
                  :class="`level-${day.level}`"
                  :title="`${day.date} · ${formatCompact(day.tokens)} Token · ${day.requests} 次`"
                ></span>
              </div>
              <div class="heatmap-legend-line">
                <div class="heatmap-legend">
                  <span>少</span>
                  <span class="legend-cell level-0"></span>
                  <span class="legend-cell level-1"></span>
                  <span class="legend-cell level-2"></span>
                  <span class="legend-cell level-3"></span>
                  <span class="legend-cell level-4"></span>
                  <span>多</span>
                </div>
                <span class="heatmap-legend-note">连续周期沉淀</span>
              </div>
            </div>

            <div class="heatmap-right-col">
              <div class="heatmap-stat-item">
                <span class="stat-label">13周累计活跃</span>
                <div class="stat-value-line">
                  <strong>{{
                    formatCompact(heatmapSummary.totalTokens)
                  }}</strong>
                  <span>Tokens</span>
                </div>
              </div>

              <div class="heatmap-stat-row">
                <span class="stat-sub-label">活跃天数</span>
                <strong class="stat-sub-val"
                  >{{ heatmapSummary.activeDays }} ({{
                    heatmapSummary.activeDaysRate
                  }}%)</strong
                >
              </div>

              <div class="heatmap-stat-row">
                <span class="stat-sub-label"
                  >峰值日 {{ heatmapSummary.peakDayLabel }}</span
                >
                <strong class="stat-sub-val peak">
                  {{ formatCompact(heatmapSummary.peakDayTokens) }}
                </strong>
              </div>
            </div>
          </div>
        </section>

        <!-- 24小时活跃律动 -->
        <section class="usage-card usage-rhythm-card">
          <div class="usage-card-heading compact">
            <div class="usage-heading-inline">
              <Clock :size="15" class="rhythm-icon" />
              <h3 class="usage-card-title">24小时活跃律动</h3>
            </div>
            <span class="usage-card-note">
              均次负载: {{ formatCompact(overviewStats.averageTokens) }} | 峰值:
              {{ hourlyPeak.label }}
            </span>
          </div>
          <div class="usage-rhythm-body">
            <div class="rhythm-chart" aria-label="24 小时节奏活动">
              <div
                v-for="item in hourlyActivity"
                :key="item.hour"
                class="rhythm-bar-col"
                :title="`${item.label} · ${item.requests} 次 · ${formatCompact(item.tokens)} Token`"
              >
                <span
                  class="rhythm-bar"
                  :class="{ active: item.requests > 0, peak: item.peak }"
                  :style="{
                    height:
                      item.requests > 0
                        ? `${Math.max(item.height, 22)}%`
                        : '4px'
                  }"
                ></span>
              </div>
            </div>
          </div>
          <div class="usage-card-footline">
            <span>⚖ 入出倍率: {{ inputOutputRatio }}</span>
            <span class="trough-info">低谷: {{ hourlyLow.label }}</span>
          </div>
        </section>

        <!-- 效能指数 · J23 -->
        <section class="usage-card usage-efficiency-card">
          <div class="usage-card-heading compact">
            <div class="usage-heading-inline">
              <Flame :size="15" class="efficiency-icon" />
              <h3 class="usage-card-title">效能指数 · J23</h3>
              <span class="usage-card-note">缓存节省效能</span>
            </div>
            <div class="usage-efficiency-actions">
              <div class="efficiency-badges">
                <span class="efficiency-pill thinking"
                  >~{{ formatCompact(overviewStats.averageThinking) }} 思 /
                  req</span
                >
                <span class="efficiency-pill throughput">⚡ 0 延时吞吐</span>
              </div>
            </div>
          </div>

          <div class="usage-gauge-body">
            <div class="semicircle-gauge-container">
              <svg
                class="semicircle-gauge-svg"
                viewBox="0 0 260 145"
                aria-hidden="true"
              >
                <g class="gauge-ticks">
                  <line
                    v-for="(tick, idx) in gaugeTicks"
                    :key="idx"
                    :x1="tick.x1"
                    :y1="tick.y1"
                    :x2="tick.x2"
                    :y2="tick.y2"
                    :stroke="tick.active ? tick.color : 'var(--color-line)'"
                    :stroke-width="tick.active ? 3.5 : 2"
                    stroke-linecap="round"
                    :opacity="tick.active ? 1 : 0.28"
                  />
                </g>
              </svg>
              <div class="gauge-overlay-content">
                <div class="gauge-hit-badge">
                  命中率: {{ formatPercent(overviewStats.cacheRate) }}
                </div>
                <div class="gauge-main-number">
                  <strong>{{
                    formatCompact(overviewStats.cacheReadTokens)
                  }}</strong>
                </div>
                <span class="gauge-sub-caption">复用 TOKEN 规模</span>
              </div>
            </div>
          </div>
          <div class="usage-card-footline">
            <span
              >未缓存输入:
              {{ formatCompact(overviewStats.uncachedInput) }}</span
            >
            <span class="savings-highlight"
              >+${{ overviewStats.estimatedSavingsFormatted }} 节省</span
            >
          </div>
        </section>
      </div>

      <!-- 最近请求 -->
      <section class="gateway-panel usage-card recent-requests-panel">
        <div class="panel-heading">
          <div class="heading-title-group">
            <ListFilter :size="16" class="heading-icon" />
            <h3 class="panel-title">最近请求</h3>
            <span class="panel-detail">用户按调用 Token 统计</span>
          </div>
          <div class="logs-toolbar">
            <div class="log-view-tabs" role="tablist">
              <button
                type="button"
                class="log-view-tab"
                :class="{ active: logViewTab === 'requests' }"
                @click="logViewTab = 'requests'"
              >
                调用记录 ({{ (gateway.logs || []).length }})
              </button>
              <button
                type="button"
                class="log-view-tab"
                :class="{ active: logViewTab === 'ip' }"
                @click="logViewTab = 'ip'"
              >
                IP 访问统计 ({{ ipStatsRows.length }})
              </button>
            </div>
            <div class="log-search-wrap">
              <Search :size="13" class="filter-icon" />
              <input
                v-model.trim="logSearch"
                class="log-search-input"
                placeholder="搜索请求 ID、IP、模型或调用 Token..."
              />
              <button
                v-if="logSearch"
                type="button"
                class="clear-input-btn"
                title="清除搜索"
                @click="logSearch = ''"
              >
                <X :size="12" />
              </button>
            </div>
            <button
              class="tool-button filter-toggle-btn"
              :class="{ active: showLogFilters || activeLogFilterCount > 0 }"
              type="button"
              title="切换筛选面板"
              @click="showLogFilters = !showLogFilters"
            >
              <Filter :size="13" />
              筛选
              <span v-if="activeLogFilterCount > 0" class="filter-badge">
                {{ activeLogFilterCount }}
              </span>
            </button>
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

        <div v-if="showLogFilters" class="logs-filter-drawer">
          <div class="filter-drawer-row">
            <div class="filter-drawer-group">
              <span class="drawer-label">请求状态：</span>
              <div class="drawer-options">
                <button
                  type="button"
                  class="drawer-option-btn"
                  :class="{ active: logFilterStatus === 'all' }"
                  @click="logFilterStatus = 'all'"
                >
                  全部
                </button>
                <button
                  type="button"
                  class="drawer-option-btn success"
                  :class="{ active: logFilterStatus === 'success' }"
                  @click="logFilterStatus = 'success'"
                >
                  成功
                </button>
                <button
                  type="button"
                  class="drawer-option-btn failure"
                  :class="{ active: logFilterStatus === 'failed' }"
                  @click="logFilterStatus = 'failed'"
                >
                  失败
                </button>
              </div>
            </div>
            <div class="filter-drawer-group">
              <span class="drawer-label">协议类型：</span>
              <div class="drawer-options">
                <button
                  type="button"
                  class="drawer-option-btn"
                  :class="{ active: logFilterProtocol === 'all' }"
                  @click="logFilterProtocol = 'all'"
                >
                  全部
                </button>
                <button
                  type="button"
                  class="drawer-option-btn"
                  :class="{ active: logFilterProtocol === 'chat' }"
                  @click="logFilterProtocol = 'chat'"
                >
                  chat
                </button>
                <button
                  type="button"
                  class="drawer-option-btn"
                  :class="{ active: logFilterProtocol === 'openai' }"
                  @click="logFilterProtocol = 'openai'"
                >
                  openai
                </button>
                <button
                  type="button"
                  class="drawer-option-btn"
                  :class="{ active: logFilterProtocol === 'anthropic' }"
                  @click="logFilterProtocol = 'anthropic'"
                >
                  anthropic
                </button>
                <button
                  type="button"
                  class="drawer-option-btn"
                  :class="{ active: logFilterProtocol === 'gemini' }"
                  @click="logFilterProtocol = 'gemini'"
                >
                  gemini
                </button>
              </div>
            </div>
            <button
              v-if="activeLogFilterCount > 0"
              type="button"
              class="drawer-reset-btn"
              @click="resetLogFilters"
            >
              <RotateCcw :size="12" /> 重置筛选
            </button>
          </div>
        </div>

        <template v-if="logViewTab === 'requests'">
          <div v-if="filteredLogs.length > 0" class="request-table-wrap">
            <table class="request-table">
              <thead class="request-head">
                <tr>
                  <th>时间 / 调用 Token</th>
                  <th>来源 IP</th>
                  <th>协议</th>
                  <th>请求 → 路由 → 上游报告</th>
                  <th>最终目标</th>
                  <th>输入 / 输出 / 缓存</th>
                  <th>状态 / 耗时</th>
                  <th>操作</th>
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
                    <span class="cell-detail request-token-name">{{
                      log.tokenName || "直接调用"
                    }}</span>
                    <span
                      v-if="log.tokenId"
                      class="cell-detail request-token-id"
                      >{{ log.tokenId }}</span
                    >
                  </td>
                  <td>
                    <span class="log-ip" :class="{ muted: !log.clientIp }">{{
                      log.clientIp || "未记录"
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
                        >{{ log.statusCode || (log.ok ? 200 : 500) }}</span
                      >
                    </div>
                  </td>
                  <td>
                    <span class="log-provider-name">{{
                      log.providerName || "未路由"
                    }}</span>
                    <span class="cell-detail">{{ log.providerId || "—" }}</span>
                  </td>
                  <td>
                    <template v-if="log.usageKnown">
                      <span class="token-in-out"
                        >{{ formatNumber(log.inputTokens) }} /
                        {{ formatNumber(log.outputTokens) }}</span
                      >
                      <span class="cell-detail cache-request-value"
                        >缓存 {{ formatNumber(log.cacheReadTokens || 0) }}</span
                      >
                    </template>
                    <span v-else class="cell-muted">未报告</span>
                  </td>
                  <td>
                    <div class="status-cell-wrap" :class="{ failed: !log.ok }">
                      <span class="status-text"
                        ><component
                          :is="log.ok ? CheckCircle2 : XCircle"
                          :size="13"
                          class="status-icon"
                        />{{ log.ok ? "成功" : "失败" }}
                        {{ log.statusCode }}</span
                      >
                      <span class="cell-detail"
                        >{{ log.latencyMs || 0 }} ms</span
                      >
                      <span v-if="log.errorMessage" class="cell-error">{{
                        log.errorMessage
                      }}</span>
                    </div>
                  </td>
                  <td>
                    <div class="table-actions">
                      <button
                        class="action-btn"
                        type="button"
                        :aria-label="`查看请求 ${log.id} 的详情`"
                        @click="openRequestDetail(log)"
                      >
                        <Eye :size="13" /> 详情
                      </button>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <p v-else class="empty-state">暂无请求记录。</p>

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
        </template>
        <template v-else-if="logViewTab === 'ip'">
          <div v-if="ipStatsRows.length > 0" class="request-table-wrap">
            <table class="request-table ip-stats-table">
              <thead class="request-head">
                <tr>
                  <th style="width: 50px">排名</th>
                  <th>IP 来源地址</th>
                  <th>总 Token 消耗</th>
                  <th>请求次数</th>
                  <th>缓存复用</th>
                  <th>用量占比</th>
                </tr>
              </thead>
              <tbody class="request-body">
                <tr
                  v-for="(item, index) in ipStatsRows"
                  :key="item.key"
                  class="request-row"
                >
                  <td>
                    <span class="usage-ranking-number">{{ index + 1 }}</span>
                  </td>
                  <td>
                    <code class="ip-addr-code">{{ item.name }}</code>
                  </td>
                  <td>
                    <strong>{{ formatCompact(item.totalTokens) }}</strong>
                  </td>
                  <td>
                    <span>{{ formatNumber(item.requestCount) }} 次</span>
                  </td>
                  <td>
                    <span>{{ formatCompact(item.cacheReadTokens) }} 缓存</span>
                  </td>
                  <td>
                    <div class="usage-ip-bar-wrap">
                      <span class="usage-ip-bar"
                        ><i
                          :style="{ width: `${Math.max(item.share, 4)}%` }"
                        ></i
                      ></span>
                      <small>{{ formatPercent(item.share) }}</small>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <p v-else class="empty-state">暂无 IP 访问记录。</p>
        </template>
      </section>
    </section>
    <!-- 请求正文按需读取，避免刷新列表时传输全部对话。 -->
    <BaseModal
      v-if="viewingRequest"
      class="request-detail-modal"
      title="请求详情"
      :description="viewingRequest.id"
      @close="closeRequestDetail"
    >
      <template #header>
        <div class="modal-header-left">
          <div class="modal-icon-badge">
            <Layers :size="20" />
          </div>
          <div class="modal-header-titles">
            <h2 class="modal-main-title">请求详情</h2>
            <p class="modal-id-subtitle">{{ viewingRequest.id }}</p>
          </div>
        </div>
        <button
          class="modal-close-icon-btn"
          type="button"
          title="关闭"
          @click="closeRequestDetail"
        >
          <X :size="16" />
        </button>
      </template>

      <div class="request-detail-view">
        <!-- 顶部两列元数据卡片 -->
        <div class="request-detail-meta-card">
          <div class="meta-column">
            <div class="meta-row">
              <span class="meta-label-group">
                <MapPin :size="13" class="meta-icon" />
                <span class="meta-label">请求 IP</span>
              </span>
              <span class="meta-value">{{
                viewingRequest.clientIp || "未记录"
              }}</span>
            </div>
            <div class="meta-row">
              <span class="meta-label-group">
                <LogIn :size="13" class="meta-icon" />
                <span class="meta-label">入口</span>
              </span>
              <span class="meta-value">
                <strong class="method-tag">{{
                  viewingRequest.requestMethod || "POST"
                }}</strong>
                {{ viewingRequest.requestPath || "未记录" }}
              </span>
            </div>
            <div class="meta-row">
              <span class="meta-label-group">
                <GitFork :size="13" class="meta-icon" />
                <span class="meta-label">协议</span>
              </span>
              <span class="meta-value">
                {{ viewingRequest.protocol || "—" }}
                <span class="meta-arrow">→</span>
                {{ viewingRequest.upstreamProtocol || "未记录" }}
              </span>
            </div>
            <div class="meta-row">
              <span class="meta-label-group">
                <Layers :size="13" class="meta-icon" />
                <span class="meta-label">Provider</span>
              </span>
              <span class="meta-value">
                <span class="meta-provider-pill">
                  {{ viewingRequest.providerName || "未路由" }}
                </span>
              </span>
            </div>
          </div>

          <div class="meta-column">
            <div class="meta-row">
              <span class="meta-label-group">
                <Box :size="13" class="meta-icon" />
                <span class="meta-label">模型</span>
              </span>
              <span class="meta-value">
                {{ viewingRequest.requestedModel || "—" }}
                <span class="meta-arrow">→</span>
                {{ viewingRequest.physicalModel || "—" }}
              </span>
            </div>
            <div class="meta-row">
              <span class="meta-label-group">
                <component
                  :is="viewingRequest.ok ? CheckCircle2 : AlertCircle"
                  :size="13"
                  class="meta-icon"
                  :class="viewingRequest.ok ? 'ok-color' : 'err-color'"
                />
                <span class="meta-label">状态</span>
              </span>
              <span class="meta-value">
                <span
                  class="meta-status-pill"
                  :class="viewingRequest.ok ? 'ok' : 'err'"
                >
                  {{ viewingRequest.statusCode || "—" }}
                </span>
              </span>
            </div>
            <div class="meta-row">
              <span class="meta-label-group">
                <Clock :size="13" class="meta-icon" />
                <span class="meta-label">耗时</span>
              </span>
              <span class="meta-value"
                >{{ viewingRequest.latencyMs || 0 }} ms</span
              >
            </div>
            <div class="meta-row">
              <span class="meta-label-group">
                <ArrowUpRight :size="13" class="meta-icon" />
                <span class="meta-label">上游状态</span>
              </span>
              <span class="meta-value">
                <span class="meta-status-pill" :class="upstreamStatusClass">
                  {{ viewingRequest.upstreamStatusCode || "未记录" }}
                </span>
              </span>
            </div>
            <div class="meta-row">
              <span class="meta-label-group">
                <Link2 :size="13" class="meta-icon" />
                <span class="meta-label">路径</span>
              </span>
              <span class="meta-value">{{
                viewingRequest.upstreamPath || "—"
              }}</span>
            </div>
          </div>
        </div>

        <p v-if="viewingRequest.errorMessage" class="request-detail-error">
          <AlertCircle :size="14" class="error-icon" />
          {{ viewingRequest.errorMessage }}
        </p>

        <nav class="request-detail-tabs" aria-label="请求内容">
          <button
            v-for="section in requestDetailSections"
            :key="section.id"
            class="request-detail-tab"
            :class="{ active: requestDetailTab === section.id }"
            type="button"
            @click="requestDetailTab = section.id"
          >
            {{ section.label }}
          </button>
        </nav>

        <p v-if="requestDetailLoading" class="request-detail-hint">
          <RefreshCw :size="14" class="spin" /> 正在读取请求内容…
        </p>
        <p v-else-if="requestDetailError" class="request-detail-error">
          {{ requestDetailError }}
        </p>
        <template v-else>
          <pre v-if="requestDetailBody !== null" class="request-detail-body">{{
            requestDetailBody
          }}</pre>
          <p v-else class="request-detail-hint">
            此记录未保存该项内容。旧记录无法补回，尚未请求上游时也没有上游内容。
          </p>
        </template>

        <div class="request-detail-actions">
          <button
            class="tool-button"
            type="button"
            :disabled="
              requestDetailLoading ||
              !!requestDetailError ||
              requestDetailBody === null
            "
            @click="copyText(requestDetailBody)"
          >
            <Copy :size="13" /> 复制当前内容
          </button>
          <button
            class="tool-button primary"
            type="button"
            @click="closeRequestDetail"
          >
            关闭
          </button>
        </div>
      </div>
    </BaseModal>

    <!-- 监听配置弹框 -->
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

    <!-- 底层池弹框 -->
    <BaseModal
      v-if="showPoolModal"
      class="pool-modal"
      title="底层池"
      description="选择参与反代的 Provider 或官方账号，模型将从所选来源的已有配置中读取。"
      @close="showPoolModal = false"
    >
      <form class="modal-form" @submit.prevent="savePoolConfig">
        <div class="pool-content">
          <div class="pool-header-bar">
            <p class="pool-summary">
              已选择 {{ poolProviderIds.length }} / {{ targets.length }} 个来源
            </p>
            <div v-if="targets.length" class="pool-batch-actions">
              <button
                type="button"
                class="pool-action-link"
                @click="selectAllPool"
              >
                全选所有
              </button>
              <span class="pool-action-divider">|</span>
              <button
                type="button"
                class="pool-action-link"
                @click="clearAllPool"
              >
                清空选择
              </button>
            </div>
          </div>
          <div class="target-list">
            <div
              v-for="group in groupedTargets"
              :key="group.name"
              class="target-group"
            >
              <div class="target-group-header">
                <div class="target-group-title">
                  <span class="group-title-text">{{ group.name }}</span>
                  <span class="group-count-badge">
                    {{ getGroupSelectedCount(group) }} /
                    {{ group.items.length }}
                  </span>
                </div>
                <button
                  type="button"
                  class="group-action-btn"
                  @click="toggleGroupSelection(group)"
                >
                  {{ isGroupAllSelected(group) ? "取消全选" : "全选本组" }}
                </button>
              </div>
              <div class="target-group-items">
                <label
                  v-for="target in group.items"
                  :key="target.id"
                  class="target-item"
                  :class="{ selected: poolProviderIds.includes(target.id) }"
                >
                  <input
                    v-model="poolProviderIds"
                    class="target-checkbox"
                    type="checkbox"
                    :value="target.id"
                    :disabled="busy || loading"
                  />
                  <span class="target-info">
                    <span class="target-name">{{ target.name }}</span>
                    <span class="target-type">{{ target.type }}</span>
                  </span>
                  <span
                    v-if="poolProviderIds.includes(target.id)"
                    class="target-status-badge"
                  >
                    已选择
                  </span>
                </label>
              </div>
            </div>
            <p v-if="!targets.length" class="empty-state">
              请先在 Providers 中添加 Provider、Codex 或 Gemini 官方账号。
            </p>
          </div>
          <p class="pool-hint">移出底层池后，该来源的模型将不再对外提供。</p>
        </div>
        <div class="modal-footer-actions">
          <span></span>
          <div class="right-buttons">
            <button
              class="tool-button"
              type="button"
              @click="showPoolModal = false"
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

    <!-- 创建 Token 弹框 -->
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

    <!-- Token 详情弹框 -->
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
          <span class="detail-label">API Key：</span>
          <div class="api-key-display">
            <code class="api-key-value">{{
              tokenValues[viewingToken.id] || viewingToken.maskedKey
            }}</code>
            <button
              class="api-key-button"
              type="button"
              :title="
                viewingToken.canReveal
                  ? tokenValues[viewingToken.id]
                    ? '隐藏 API Key'
                    : '预览 API Key'
                  : '旧 Token 仅保存校验值，请重新创建后复制或预览'
              "
              :aria-label="`${tokenValues[viewingToken.id] ? '隐藏' : '预览'} API Key`"
              :aria-pressed="!!tokenValues[viewingToken.id]"
              :disabled="
                !viewingToken.canReveal || pendingTokenIds.has(viewingToken.id)
              "
              @click="toggleTokenPreview(viewingToken)"
            >
              <component
                :is="tokenValues[viewingToken.id] ? EyeOff : Eye"
                :size="13"
              />
            </button>
            <button
              class="api-key-button"
              type="button"
              title="复制完整 API Key"
              :disabled="
                !viewingToken.canReveal || pendingTokenIds.has(viewingToken.id)
              "
              @click="copyToken(viewingToken)"
            >
              <Copy :size="13" />
            </button>
          </div>
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

    <!-- 删除 Token 确认弹框 -->
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

    <!-- 定价矩阵弹框 -->
    <BaseModal
      v-if="showPricingModal"
      title="模型计费与定价矩阵"
      description="从系统用量计费配置中动态读取的真实费率规则，用于网关调用预估支出与上下文缓存节省额度"
      @close="showPricingModal = false"
    >
      <div class="pricing-matrix-modal">
        <div class="pricing-table-wrap">
          <table v-if="pricingMatrixRows.length" class="pricing-table">
            <thead>
              <tr>
                <th>模型体系</th>
                <th>输入定价</th>
                <th>输出定价</th>
                <th>缓存读取</th>
                <th>缓存节省率</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in pricingMatrixRows" :key="item.model">
                <td>
                  <strong>{{ item.model }}</strong>
                  <small>{{ item.provider }}</small>
                </td>
                <td>{{ item.currency }}{{ item.input }} / M</td>
                <td>{{ item.currency }}{{ item.output }} / M</td>
                <td>{{ item.currency }}{{ item.cacheRead }} / M</td>
                <td class="saving-cell">-{{ item.discount }}%</td>
              </tr>
            </tbody>
          </table>
          <div v-else class="pricing-empty-state">
            <p>系统尚未配置模型定价规则</p>
            <span>可在「用量统计」管理面板中配置模型计费价格与汇率</span>
          </div>
        </div>
        <div class="pricing-notes">
          <p>
            •
            <strong>预估支出</strong
            >：按每次请求实际模型匹配系统定价规则与输入/输出 Token 计算。
          </p>
          <p>
            • <strong>缓存节省</strong>：按命中上下文缓存的 Token
            规模与标准输入单价差额进行冲抵计算。
          </p>
        </div>
        <div class="modal-footer-actions">
          <div></div>
          <button
            class="tool-button primary"
            type="button"
            @click="showPricingModal = false"
          >
            完成
          </button>
        </div>
      </div>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onMounted, reactive, ref, watch } from "vue"
import {
  Activity,
  AlertCircle,
  ArrowUpRight,
  BarChart3,
  Box,
  Calendar,
  CheckCircle2,
  Clock,
  Code2,
  Copy,
  Database,
  Download,
  Eye,
  EyeOff,
  Filter,
  Flame,
  GitFork,
  Info,
  KeyRound,
  Layers,
  Link2,
  ListFilter,
  LogIn,
  MapPin,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  RotateCw,
  Save,
  Search,
  ServerCog,
  SlidersHorizontal,
  Sparkles,
  Square,
  Trash2,
  X,
  XCircle,
  Zap
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import GatewayConnectionPanel from "@/features/tools/multiProtocolGateway/components/GatewayConnectionPanel.vue"
import { multiProtocolGatewayApi, usageApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] }
})

const emit = defineEmits(["status-change"])

const tabs = [
  { id: "connections", label: "接入地址", icon: Code2 },
  { id: "tokens", label: "调用 Token", icon: KeyRound },
  { id: "logs", label: "用量与追踪", icon: BarChart3 }
]

const gateway = ref({})
const activeTab = ref("connections")
const loading = ref(true)
const busy = ref(false)
const loadError = ref("")
const tokenValues = reactive({})
const pendingTokenIds = reactive(new Set())
let tokenPreviewVersion = 0
const deletingToken = ref(null)
const viewingToken = ref(null)
const lanBannerDismissed = ref(false)

watch(
  [() => gateway.value.enabled, loading, busy],
  () => {
    if (typeof emit === "function") {
      emit("status-change", {
        enabled: Boolean(gateway.value.enabled),
        loading: loading.value,
        busy: busy.value
      })
    }
  },
  { immediate: true }
)

defineExpose({
  load,
  loading,
  busy,
  gateway
})

const draft = reactive({
  host: "127.0.0.1",
  port: 8046,
  providerIds: [],
  modelMappings: {}
})

const tokenDraft = reactive({ name: "", description: "", expiresAt: "" })

// 弹框控制状态
const showListenerModal = ref(false)
const listenerDraft = reactive({ host: "127.0.0.1", port: 8046 })

const showPoolModal = ref(false)
const poolProviderIds = ref([])

const showCreateTokenModal = ref(false)

// 筛选与分页状态
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
const showLogFilters = ref(false)
const logFilterStatus = ref("all")
const logFilterProtocol = ref("all")
const usageRankingTab = ref("provider")
const usageRankingTabs = [
  { id: "provider", label: "账号" },
  { id: "model", label: "模型" },
  { id: "user", label: "用户" }
]
const efficiencyTab = ref("gauge")
const logViewTab = ref("requests")
const showPricingModal = ref(false)

const systemPricing = ref({ exchangeRate: 7.2, items: [] })

async function loadPricingConfig() {
  try {
    if (typeof usageApi?.getUsagePricing !== "function") return
    const res = await usageApi.getUsagePricing()
    if (res?.data) {
      systemPricing.value = {
        exchangeRate: Number(res.data.exchangeRate || 7.2),
        items: Array.isArray(res.data.items) ? res.data.items : []
      }
    }
  } catch (err) {
    console.warn("加载系统定价配置失败:", err)
  }
}

const pricingMatrixRows = computed(() => {
  const items = systemPricing.value?.items || []
  return items.map((item) => {
    const input = Number(item.inputCostPerMillion) || 0
    const output = Number(item.outputCostPerMillion) || 0
    const cacheRead = Number(item.cacheReadCostPerMillion) || 0
    const discount =
      input > 0 && cacheRead > 0
        ? Math.round(((input - cacheRead) / input) * 100)
        : input > 0 && cacheRead === 0
          ? 100
          : 0
    return {
      model: item.modelId || "未命名模型",
      provider: item.modelCategory || "通用",
      currency: item.currency === "CNY" ? "¥" : "$",
      input: input.toFixed(2),
      output: output.toFixed(2),
      cacheRead: cacheRead.toFixed(4).replace(/\.?0+$/, "") || "0",
      discount: String(discount)
    }
  })
})

const viewingRequest = ref(null)
const requestDetailLoading = ref(false)
const requestDetailError = ref("")
const requestDetailTab = ref("responseBody")
const requestDetailSections = [
  { id: "requestBody", label: "请求内容" },
  { id: "upstreamRequestBody", label: "转发请求" },
  { id: "upstreamResponseBody", label: "上游响应" },
  { id: "responseBody", label: "响应内容" }
]
let requestDetailVersion = 0

const requestDetailBody = computed(() => {
  const value = viewingRequest.value?.[requestDetailTab.value]
  if (typeof value !== "string") return null
  try {
    return JSON.stringify(JSON.parse(value), null, 2)
  } catch {
    return value
  }
})

const upstreamStatusClass = computed(() => {
  const code = Number(viewingRequest.value?.upstreamStatusCode)
  if (!code) return "muted"
  if (code >= 200 && code < 300) return "ok"
  return "err"
})

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

function formatGroupTitle(target) {
  if (target.type === "Gemini 官方账号" || target.cli === "google-account") {
    return "Gemini 官方账号"
  }
  if (target.type === "Codex 官方账号") {
    return "Codex 官方账号"
  }
  const cli = (target.cli || "").toLowerCase()
  if (cli === "claude") {
    return "Claude Provider"
  }
  if (cli === "codex") {
    return "Codex Provider"
  }
  if (cli === "openai") {
    return "OpenAI Provider"
  }
  if (cli === "gemini") {
    return "Gemini Provider"
  }
  if (target.cli) {
    return `${target.cli.charAt(0).toUpperCase() + target.cli.slice(1)} Provider`
  }
  return target.type || "其他来源"
}

const groupedTargets = computed(() => {
  const map = new Map()

  for (const target of targets.value) {
    const groupName = formatGroupTitle(target)
    if (!map.has(groupName)) {
      map.set(groupName, {
        name: groupName,
        items: []
      })
    }
    map.get(groupName).items.push(target)
  }

  const list = Array.from(map.values())
  const orderMap = {
    "Claude Provider": 1,
    "Codex Provider": 2,
    "Gemini 官方账号": 3,
    "Codex 官方账号": 4
  }

  list.sort((a, b) => {
    const orderA = orderMap[a.name] || 99
    const orderB = orderMap[b.name] || 99
    if (orderA !== orderB) return orderA - orderB
    return a.name.localeCompare(b.name)
  })

  return list
})

function isGroupAllSelected(group) {
  if (!group.items.length) return false
  return group.items.every((item) => poolProviderIds.value.includes(item.id))
}

function getGroupSelectedCount(group) {
  return group.items.filter((item) => poolProviderIds.value.includes(item.id))
    .length
}

function toggleGroupSelection(group) {
  const allSelected = isGroupAllSelected(group)
  const itemIds = group.items.map((item) => item.id)
  if (allSelected) {
    poolProviderIds.value = poolProviderIds.value.filter(
      (id) => !itemIds.includes(id)
    )
  } else {
    const toAdd = itemIds.filter((id) => !poolProviderIds.value.includes(id))
    poolProviderIds.value = [...poolProviderIds.value, ...toAdd]
  }
}

function selectAllPool() {
  poolProviderIds.value = targets.value.map((t) => t.id)
}

function clearAllPool() {
  poolProviderIds.value = []
}

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

function calculateLogCost(log, pricingItems) {
  const inTok = Number(log.inputTokens) || 0
  const outTok = Number(log.outputTokens) || 0
  const cachedTok = Number(log.cacheReadTokens) || 0

  const logModel = (log.upstreamModel || log.physicalModel || log.model || "")
    .trim()
    .toLowerCase()

  const matchedPricing = (pricingItems || []).find((p) => {
    const pModel = (p.modelId || "").trim().toLowerCase()
    return (
      pModel &&
      (logModel === pModel ||
        logModel.includes(pModel) ||
        pModel.includes(logModel))
    )
  })

  let cost = 0
  let savings = 0
  if (matchedPricing) {
    const inPrice = Number(matchedPricing.inputCostPerMillion) || 0
    const outPrice = Number(matchedPricing.outputCostPerMillion) || 0
    const cacheReadPrice = Number(matchedPricing.cacheReadCostPerMillion) || 0
    cost = (inTok * inPrice + outTok * outPrice) / 1000000
    if (cachedTok > 0) {
      savings = (cachedTok * Math.max(0, inPrice - cacheReadPrice)) / 1000000
    }
  } else if (log.totalCostUsd) {
    cost = Number(log.totalCostUsd) || 0
  }
  return { cost, savings }
}

function usageFromLogs(logs, fallback = {}) {
  let totalRequests = 0
  let successRequests = 0
  let failureRequests = 0
  let inputTokens = 0
  let outputTokens = 0
  let cacheReadTokens = 0
  let cacheInputTokens = 0
  let cacheHitRequestCount = 0
  let cacheMeasuredRequestCount = 0
  let reasoningTokens = 0
  let estimatedCost = 0
  let estimatedSavings = 0

  const pricingItems = systemPricing.value?.items || []

  for (const log of logs) {
    totalRequests++
    if (log.ok) successRequests++
    else failureRequests++
    const inTok = Number(log.inputTokens) || 0
    const outTok = Number(log.outputTokens) || 0
    const cachedTok = Number(log.cacheReadTokens) || 0
    inputTokens += inTok
    outputTokens += outTok

    const rTokens = Number(log.reasoningTokens) || 0
    if (rTokens > 0) {
      reasoningTokens += rTokens
    } else if (log.reasoning && typeof log.reasoning === "string") {
      reasoningTokens += Math.ceil(log.reasoning.length / 4)
    }

    if (log.usageKnown) {
      cacheReadTokens += cachedTok
      cacheInputTokens += Number(log.cacheInputTokens) || inTok || 0
      cacheMeasuredRequestCount++
      if (cachedTok > 0) cacheHitRequestCount++
    }

    // 根据真实模型匹配定价项计算真实支出与节省
    const { cost: logCost, savings: logSavings } = calculateLogCost(
      log,
      pricingItems
    )
    estimatedCost += logCost
    estimatedSavings += logSavings
  }

  if (!logs.length && fallback) {
    totalRequests = Number(fallback.requestCount) || 0
    successRequests = Number(fallback.successCount) || 0
    failureRequests = Number(fallback.failureCount) || 0
    inputTokens = Number(fallback.inputTokens) || 0
    outputTokens = Number(fallback.outputTokens) || 0
    cacheReadTokens = Number(fallback.cacheReadTokens) || 0
    cacheInputTokens = Number(fallback.cacheInputTokens) || 0
    cacheHitRequestCount = Number(fallback.cacheHitRequestCount) || 0
    cacheMeasuredRequestCount = Number(fallback.cacheMeasuredRequestCount) || 0
    reasoningTokens = Number(fallback.reasoningTokens) || 0
    if (fallback.estimatedCost) {
      estimatedCost = Number(fallback.estimatedCost) || 0
    }
    if (fallback.estimatedSavings) {
      estimatedSavings = Number(fallback.estimatedSavings) || 0
    }
  }

  const totalTokens = inputTokens + outputTokens
  const cacheRate =
    cacheInputTokens > 0 ? (cacheReadTokens / cacheInputTokens) * 100 : 0
  const reasoningShare =
    outputTokens > 0 ? (reasoningTokens / outputTokens) * 100 : 0
  const averageThinking =
    totalRequests > 0 ? reasoningTokens / totalRequests : 0
  const costPerReq = totalRequests > 0 ? estimatedCost / totalRequests : 0
  const uncachedInput = Math.max(
    0,
    (cacheInputTokens || inputTokens) - cacheReadTokens
  )

  const estimatedCostFormatted =
    estimatedCost >= 100 ? estimatedCost.toFixed(1) : estimatedCost.toFixed(2)
  const costPerReqFormatted =
    costPerReq >= 0.1 ? costPerReq.toFixed(2) : costPerReq.toFixed(3)
  const estimatedSavingsFormatted =
    estimatedSavings >= 100
      ? estimatedSavings.toFixed(1)
      : estimatedSavings.toFixed(2)

  return {
    totalRequests,
    successRequests,
    failureRequests,
    totalTokens,
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheInputTokens,
    cacheHitRequestCount,
    cacheMeasuredRequestCount,
    cacheRate,
    reasoningTokens,
    reasoningShare,
    averageThinking,
    estimatedCost,
    estimatedCostFormatted,
    costPerReq,
    costPerReqFormatted,
    estimatedSavings,
    estimatedSavingsFormatted,
    uncachedInput,
    inputShare: totalTokens ? (inputTokens / totalTokens) * 100 : 0,
    outputShare: totalTokens ? (outputTokens / totalTokens) * 100 : 0,
    averageTokens: totalRequests ? totalTokens / totalRequests : 0
  }
}

function groupUsageRows(logs, field) {
  const map = new Map()
  const pricingItems = systemPricing.value?.items || []

  for (const log of logs) {
    let key, name
    if (field === "provider" || field === "providerId") {
      key = log.providerId || log.providerName || "unknown-provider"
      name = log.providerName || log.providerId || "未路由"
    } else if (field === "model" || field === "physicalModel") {
      key =
        log.physicalModel || log.requestedModel || log.model || "unknown-model"
      name = log.physicalModel || log.requestedModel || log.model || "未报告"
    } else if (field === "user" || field === "tokenId") {
      if (log.tokenName) {
        key = `token_${log.tokenId || log.tokenName}`
        name = log.tokenName
      } else if (log.tokenId) {
        key = `token_${log.tokenId}`
        name = log.tokenId
      } else if (log.clientIp) {
        key = `ip_${log.clientIp}`
        name = log.clientIp
      } else {
        key = "direct"
        name = "直接调用"
      }
    } else if (field === "clientIp") {
      key = log.clientIp || "unknown"
      name = log.clientIp || "未知 IP"
    } else {
      key = log[field] || "unknown"
      name = log[field] || "未知"
    }

    if (!map.has(key)) {
      map.set(key, {
        key,
        name,
        requestCount: 0,
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        cacheReadTokens: 0,
        cacheInputTokens: 0,
        cost: 0
      })
    }
    const item = map.get(key)
    const inTok = Number(log.inputTokens) || 0
    const outTok = Number(log.outputTokens) || 0
    const cachedTok = Number(log.cacheReadTokens) || 0

    item.requestCount++
    item.inputTokens += inTok
    item.outputTokens += outTok
    item.totalTokens += inTok + outTok

    if (log.usageKnown) {
      item.cacheReadTokens += cachedTok
      item.cacheInputTokens += Number(log.cacheInputTokens) || inTok || 0
    }

    const { cost } = calculateLogCost(log, pricingItems)
    item.cost += cost
  }

  const total = Array.from(map.values()).reduce(
    (sum, item) => sum + item.totalTokens,
    0
  )
  return Array.from(map.values())
    .map((item) => ({
      ...item,
      share: total ? (item.totalTokens / total) * 100 : 0,
      costFormatted:
        item.cost >= 100 ? item.cost.toFixed(1) : item.cost.toFixed(2)
    }))
    .sort((a, b) => b.totalTokens - a.totalTokens)
}

function rowsFromAggregate(items) {
  const total = (items || []).reduce(
    (sum, item) =>
      sum + (Number(item.inputTokens) || 0) + (Number(item.outputTokens) || 0),
    0
  )
  return (items || [])
    .map((item) => {
      const inputTokens = Number(item.inputTokens) || 0
      const outputTokens = Number(item.outputTokens) || 0
      const totalTokens = inputTokens + outputTokens
      return {
        key: item.key,
        name: item.name || item.key || "未记录",
        requestCount: Number(item.requestCount) || 0,
        inputTokens,
        outputTokens,
        totalTokens: inputTokens + outputTokens,
        cacheReadTokens: Number(item.cacheReadTokens) || 0,
        cacheInputTokens: Number(item.cacheInputTokens) || 0,
        share: total ? ((inputTokens + outputTokens) / total) * 100 : 0,
        cost: 0,
        costFormatted: "0.00"
      }
    })
    .sort((a, b) => b.totalTokens - a.totalTokens)
}

const overviewStats = computed(() => {
  const hasLogWindow =
    logTimeFilter.value !== "all" && gateway.value.logs?.length
  const summary = hasLogWindow
    ? usageFromLogs(timeFilteredLogs.value)
    : usageFromLogs([], gateway.value.usage || {})
  return summary
})

const usageModelRows = computed(() => {
  if (logTimeFilter.value === "all")
    return rowsFromAggregate(gateway.value.byModel)
  return groupUsageRows(timeFilteredLogs.value, "physicalModel")
})

const topModelRows = computed(() => usageModelRows.value.slice(0, 3))

const usageRankingRows = computed(() => {
  const aggregateMap = {
    provider: "byProvider",
    model: "byModel",
    user: "byToken"
  }
  if (logTimeFilter.value === "all") {
    if (timeFilteredLogs.value.length > 0) {
      return groupUsageRows(
        timeFilteredLogs.value,
        usageRankingTab.value
      ).slice(0, 6)
    }
    return rowsFromAggregate(
      gateway.value[aggregateMap[usageRankingTab.value]]
    ).slice(0, 6)
  }
  return groupUsageRows(timeFilteredLogs.value, usageRankingTab.value).slice(
    0,
    6
  )
})

const rankingFooterNote = computed(() => {
  if (usageRankingTab.value === "provider") return "网关防刷隔离"
  if (usageRankingTab.value === "model") return "按模型调度用量排行"
  return "按 Token 记录调用用户"
})

const ipStatsRows = computed(() => {
  if (logTimeFilter.value === "all") {
    return rowsFromAggregate(gateway.value.byIp).slice(0, 6)
  }
  return groupUsageRows(timeFilteredLogs.value, "clientIp").slice(0, 6)
})

const sunburstRays = computed(() => {
  const count = 52
  const center = 90
  const rInner = 58
  const rOuter = 78
  const rays = []
  for (let i = 0; i < count; i++) {
    const angle = (i * 2 * Math.PI) / count
    const cos = Math.cos(angle)
    const sin = Math.sin(angle)
    rays.push({
      x1: +(center + rInner * cos).toFixed(2),
      y1: +(center + rInner * sin).toFixed(2),
      x2: +(center + rOuter * cos).toFixed(2),
      y2: +(center + rOuter * sin).toFixed(2)
    })
  }
  return rays
})

const gaugeTicks = computed(() => {
  const count = 33
  const centerX = 130
  const centerY = 125
  const rInner = 86
  const rOuter = 104
  const ticks = []
  const cacheRate = overviewStats.value.cacheRate || 0

  for (let i = 0; i < count; i++) {
    const fraction = i / (count - 1)
    const angle = Math.PI * (1 - fraction)
    const cos = Math.cos(angle)
    const sin = Math.sin(angle)
    const pct = fraction * 100

    let color = "#10b981"
    if (pct > 65 && pct <= 82) {
      color = "#f59e0b"
    } else if (pct > 82) {
      color = "#f97316"
    }

    const active = cacheRate > 0 ? pct <= cacheRate : false

    ticks.push({
      x1: +(centerX + rInner * cos).toFixed(2),
      y1: +(centerY - rInner * sin).toFixed(2),
      x2: +(centerX + rOuter * cos).toFixed(2),
      y2: +(centerY - rOuter * sin).toFixed(2),
      color,
      active
    })
  }
  return ticks
})

const hourlyActivity = computed(() => {
  const now = Date.now()
  const hour = 3600 * 1000
  const currentHour = Math.floor(now / hour) * hour
  const logs = (gateway.value.logs || []).filter(
    (log) => (log.createdAt || 0) >= currentHour - 23 * hour
  )
  const items = Array.from({ length: 24 }, (_, index) => {
    const start = currentHour - (23 - index) * hour
    const end = start + hour
    const bucket = logs.filter(
      (log) => log.createdAt >= start && log.createdAt < end
    )
    return {
      hour: start,
      label: `${new Date(start).getHours().toString().padStart(2, "0")}:00`,
      requests: bucket.length,
      tokens: bucket.reduce(
        (sum, log) =>
          sum +
          (Number(log.inputTokens) || 0) +
          (Number(log.outputTokens) || 0),
        0
      ),
      height: 5
    }
  })
  const max = Math.max(...items.map((item) => item.requests), 1)
  const peak = Math.max(...items.map((item) => item.requests))
  return items.map((item) => ({
    ...item,
    height: item.requests ? 12 + (item.requests / max) * 88 : 5,
    peak: item.requests === peak && peak > 0
  }))
})

const hourlyPeak = computed(() =>
  hourlyActivity.value.reduce(
    (peak, item) => (item.requests > peak.requests ? item : peak),
    { label: "—", requests: 0 }
  )
)

const hourlyLow = computed(() =>
  hourlyActivity.value.reduce(
    (low, item) => (item.requests < low.requests ? item : low),
    hourlyActivity.value[0] || { label: "—", requests: 0 }
  )
)

const inputOutputRatio = computed(() => {
  const output = overviewStats.value.outputTokens
  if (!output) return "—"
  return `${(overviewStats.value.inputTokens / output).toFixed(1)} : 1`
})

const hourly24TotalTokens = computed(() => {
  return hourlyActivity.value.reduce((sum, h) => sum + h.tokens, 0)
})

const unitCostPerMillion = computed(() => {
  const total = overviewStats.value.totalTokens
  if (!total) return "0.00"
  const cost = overviewStats.value.estimatedCost
  return ((cost / total) * 1000000).toFixed(2)
})

const heatmapDays = computed(() => {
  const oneDay = 24 * 3600 * 1000
  const todayEnd = new Date().setHours(23, 59, 59, 999)
  const logs = gateway.value.logs || []

  const days = []
  for (let i = 90; i >= 0; i--) {
    const dayStart = todayEnd - (i + 1) * oneDay + 1
    const dayEnd = todayEnd - i * oneDay
    const dateObj = new Date(dayStart)
    const y = dateObj.getFullYear()
    const m = (dateObj.getMonth() + 1).toString().padStart(2, "0")
    const d = dateObj.getDate().toString().padStart(2, "0")
    const dateStr = `${y}-${m}-${d}`

    const dayLogs = logs.filter(
      (l) => (l.createdAt || 0) >= dayStart && (l.createdAt || 0) <= dayEnd
    )
    const tokens = dayLogs.reduce(
      (sum, l) =>
        sum + (Number(l.inputTokens) || 0) + (Number(l.outputTokens) || 0),
      0
    )
    const requests = dayLogs.length

    days.push({
      date: dateStr,
      tokens,
      requests,
      level: 0
    })
  }

  const maxTokens = Math.max(...days.map((d) => d.tokens), 0)
  return days.map((day) => {
    let level = 0
    if (day.tokens > 0) {
      if (!maxTokens || day.tokens <= maxTokens * 0.25) level = 1
      else if (day.tokens <= maxTokens * 0.5) level = 2
      else if (day.tokens <= maxTokens * 0.75) level = 3
      else level = 4
    }
    return { ...day, level }
  })
})

const heatmapSummary = computed(() => {
  const days = heatmapDays.value
  const totalTokens = days.reduce((sum, d) => sum + d.tokens, 0)
  const activeDays = days.filter((d) => d.requests > 0 || d.tokens > 0).length
  const activeDaysRate = Math.round((activeDays / 91) * 100)
  const peakDayTokens = Math.max(...days.map((d) => d.tokens), 0)

  let peakDayLabel = "--"
  if (peakDayTokens > 0) {
    const peakDay = days.find((d) => d.tokens === peakDayTokens)
    if (peakDay) {
      peakDayLabel = peakDay.date.slice(5)
    }
  }

  let streak = 0
  for (let i = days.length - 1; i >= 0; i--) {
    if (days[i].requests > 0 || days[i].tokens > 0) {
      streak++
    } else {
      if (i === days.length - 1) continue
      break
    }
  }

  return {
    totalTokens,
    activeDays,
    activeDaysRate,
    peakDayTokens,
    peakDayLabel,
    streak
  }
})

const activeLogFilterCount = computed(() => {
  let count = 0
  if (logFilterStatus.value !== "all") count++
  if (logFilterProtocol.value !== "all") count++
  return count
})

function resetLogFilters() {
  logFilterStatus.value = "all"
  logFilterProtocol.value = "all"
  logSearch.value = ""
}

const filteredLogs = computed(() => {
  let list = gateway.value.logs || []
  if (logFilterStatus.value === "success") {
    list = list.filter((l) => l.ok)
  } else if (logFilterStatus.value === "failed") {
    list = list.filter((l) => !l.ok)
  }
  if (logFilterProtocol.value !== "all") {
    list = list.filter(
      (l) => (l.protocol || "chat") === logFilterProtocol.value
    )
  }
  if (logSearch.value) {
    const q = logSearch.value.toLowerCase()
    list = list.filter(
      (l) =>
        l.id?.toLowerCase().includes(q) ||
        l.clientIp?.toLowerCase().includes(q) ||
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

function formatCompact(num) {
  const value = Number(num) || 0
  if (Math.abs(value) >= 1_000_000_000)
    return `${(value / 1_000_000_000).toFixed(1).replace(/\.0$/, "")}B`
  if (Math.abs(value) >= 1_000_000)
    return `${(value / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`
  if (Math.abs(value) >= 1_000)
    return `${(value / 1_000).toFixed(1).replace(/\.0$/, "")}K`
  return formatNumber(value)
}

function formatPercent(value) {
  const number = Number(value) || 0
  if (number === 0) return "0%"
  if (number >= 99.95) return "100%"
  return `${number.toFixed(number >= 10 ? 0 : 1)}%`
}

function applyState(result, resetDraft = false) {
  hideTokenPreviews()
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
    const [state] = await Promise.all([
      multiProtocolGatewayApi.getState(),
      loadPricingConfig()
    ])
    applyState(state, true)
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
    // 创建接口的明文不进入列表状态；需要时再通过专用接口读取。
    const { createdToken, ...state } = result
    applyState(state, resetDraft)
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

function saveConfig(overrides = {}) {
  return perform(
    () =>
      multiProtocolGatewayApi.saveConfig({ ...configPayload(), ...overrides }),
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

function openPoolModal() {
  poolProviderIds.value = targets.value
    .filter((target) => draft.providerIds.includes(target.id))
    .map((target) => target.id)
  showPoolModal.value = true
}

async function savePoolConfig() {
  const providerIds = poolProviderIds.value.filter((id) =>
    targets.value.some((target) => target.id === id)
  )
  // 保留仍在池中的既有路由，移除已退出来源的路由，避免保存时指向无效目标。
  const modelMappings = Object.fromEntries(
    Object.entries(configPayload().modelMappings).filter(([, rule]) =>
      providerIds.includes(rule.targetId)
    )
  )
  const result = await saveConfig({ providerIds, modelMappings })
  if (result) showPoolModal.value = false
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

// 复制不会展开密钥；预览明文只留在当前页面，隐藏、刷新或切换页签时清除。
function hideTokenPreviews() {
  tokenPreviewVersion += 1
  for (const id of Object.keys(tokenValues)) delete tokenValues[id]
}

async function readTokenKey(token) {
  if (tokenValues[token.id]) return tokenValues[token.id]
  if (pendingTokenIds.has(token.id)) return ""
  pendingTokenIds.add(token.id)
  try {
    const result = await multiProtocolGatewayApi.getTokenValue({
      tokenId: token.id
    })
    return result.apiKey
  } catch (error) {
    createMessage.error(error.message || "读取 API Key 失败")
    return ""
  } finally {
    pendingTokenIds.delete(token.id)
  }
}

async function copyToken(token) {
  const apiKey = await readTokenKey(token)
  if (apiKey) await copyText(apiKey)
}

async function toggleTokenPreview(token) {
  if (tokenValues[token.id]) {
    delete tokenValues[token.id]
    return
  }
  const version = tokenPreviewVersion
  const apiKey = await readTokenKey(token)
  if (apiKey && version === tokenPreviewVersion) tokenValues[token.id] = apiKey
}

async function openRequestDetail(log) {
  const version = ++requestDetailVersion
  viewingRequest.value = { ...log }
  requestDetailTab.value = "responseBody"
  requestDetailError.value = ""
  requestDetailLoading.value = true
  try {
    const detail = await multiProtocolGatewayApi.getLogDetail({ logId: log.id })
    if (version === requestDetailVersion) viewingRequest.value = detail
  } catch (error) {
    if (version === requestDetailVersion) {
      requestDetailError.value = error.message || "读取请求详情失败"
    }
  } finally {
    if (version === requestDetailVersion) requestDetailLoading.value = false
  }
}

function closeRequestDetail() {
  // 关闭或切换记录后，忽略尚未返回的旧详情请求。
  requestDetailVersion++
  viewingRequest.value = null
  requestDetailLoading.value = false
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

watch([activeTab, viewingToken], hideTokenPreviews, { flush: "sync" })
onMounted(load)
</script>

<style scoped lang="less">
.gateway-tool {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  gap: 14px;
  overflow: auto;
  padding: 4px 6px 24px;
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

  /* 管理页签导航 */
  .gateway-tabs {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-bottom: 2px;

    .tab-button {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      padding: 7px 14px;
      border: 1px solid transparent;
      border-radius: 8px;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      font-size: 13px;
      transition: all 0.15s;

      .tab-icon {
        flex-shrink: 0;
      }

      &:hover:not(.active) {
        color: var(--color-text);
        background: var(--color-panel-soft);
      }

      &.active {
        color: #7c3aed;
        background: rgba(124, 58, 237, 0.09);
        border-color: rgba(124, 58, 237, 0.2);
      }
    }
  }

  .gateway-error {
    margin: 0;
    padding: 10px 14px;
    border-radius: 8px;
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #ef4444;
    background: rgba(239, 68, 68, 0.06);
    font-size: 12px;
  }

  .gateway-content {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;

    &.connections-view {
      gap: 16px;
    }
  }

  /* 统一模型网关卡片 */
  .gateway-service-card {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 20px;
    padding: 16px 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);

    .service-left {
      display: flex;
      align-items: center;
      gap: 14px;
      min-width: 0;

      .service-badge-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 44px;
        height: 44px;
        border-radius: 10px;
        color: #7c3aed;
        background: rgba(124, 58, 237, 0.08);
        border: 1px solid rgba(124, 58, 237, 0.15);
        flex-shrink: 0;
      }

      .service-meta {
        display: flex;
        flex-direction: column;
        gap: 3px;
        min-width: 0;

        .service-title-line {
          display: flex;
          align-items: center;
          gap: 10px;

          .service-name {
            margin: 0;
            font-size: 15px;
            color: var(--color-text);
          }

          .service-status-pill {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            padding: 2px 8px;
            border-radius: 12px;
            color: var(--color-text-muted);
            background: var(--color-panel-soft);
            font-size: 11px;

            &.active {
              color: #10b981;
              background: rgba(16, 185, 129, 0.08);

              .status-dot {
                background: #10b981;
              }
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
          font-family: Consolas, monospace;
        }
      }
    }

    .service-right {
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;

      .switch-item {
        display: flex;
        align-items: center;
        gap: 8px;
        margin-right: 4px;

        .switch-label {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          color: var(--color-text-muted);
          font-size: 12px;
          cursor: help;

          .info-icon {
            opacity: 0.7;
          }
        }

        .switch-control {
          position: relative;
          width: 38px;
          height: 20px;
          padding: 0;
          border: 1px solid var(--color-line);
          border-radius: 10px;
          background: var(--color-panel-soft);
          cursor: pointer;
          transition:
            background 0.2s,
            border-color 0.2s;

          .switch-thumb {
            position: absolute;
            top: 2px;
            left: 2px;
            width: 14px;
            height: 14px;
            border-radius: 50%;
            background: #fff;
            box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
            transition: transform 0.2s;
          }

          &.checked {
            background: #7c3aed;
            border-color: #7c3aed;

            .switch-thumb {
              transform: translateX(18px);
            }
          }

          &:disabled {
            opacity: 0.5;
            cursor: wait;
          }
        }
      }
    }
  }

  .lan-alert-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border: 1px solid rgba(124, 58, 237, 0.2);
    border-radius: 8px;
    background: rgba(124, 58, 237, 0.05);
    color: var(--color-text);
    font-size: 12px;

    .banner-content {
      display: flex;
      align-items: center;
      gap: 8px;

      .banner-icon {
        color: #7c3aed;
        flex-shrink: 0;
      }
    }

    .banner-close-btn {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 20px;
      height: 20px;
      border: 0;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      border-radius: 4px;

      &:hover {
        color: var(--color-text);
      }
    }
  }

  /* 通用面板容器 */
  .gateway-panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);

    .panel-heading {
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 12px;

      .heading-title-group {
        display: flex;
        align-items: center;
        gap: 8px;

        .heading-icon {
          color: #7c3aed;
          flex-shrink: 0;
        }
        .panel-title {
          margin: 0;
          font-size: 14px;
          color: var(--color-text);
        }
      }

      .panel-detail {
        color: var(--color-text-muted);
        font-size: 11px;
      }
    }
  }

  .gateway-card,
  .usage-card {
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-sizing: border-box;
  }

  .usage-dashboard-layout {
    display: grid;
    grid-template-columns: minmax(0, 1.65fr) minmax(320px, 1.1fr);
    gap: 14px;
    align-items: stretch;
  }

  .usage-summary-card {
    min-height: 250px;
    padding: 18px 22px;

    .usage-summary-body {
      display: grid;
      grid-template-columns: minmax(0, 1.35fr) 180px minmax(180px, 0.9fr);
      align-items: center;
      gap: 20px;
      min-height: 200px;
    }
  }

  .usage-ranking-card {
    display: flex;
    flex-direction: column;
    padding: 16px 18px;
    min-height: 250px;

    .usage-ranking-tabs {
      display: flex;
      align-items: center;
      padding: 3px;
      border-radius: 8px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);
      margin-bottom: 12px;
      gap: 2px;

      .usage-ranking-tab {
        flex: 1;
        padding: 5px 0;
        border: none;
        border-radius: 6px;
        background: transparent;
        color: var(--color-text-muted);
        font-size: 12px;
        cursor: pointer;
        text-align: center;
        transition: all 0.15s ease;

        &:hover {
          color: var(--color-text);
        }

        &.active {
          color: var(--color-text);
          background: var(--color-panel);
          box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
        }
      }
    }

    .usage-ranking-list {
      display: flex;
      flex-direction: column;
      gap: 11px;
      flex: 1;
      min-width: 0;

      .usage-ranking-row {
        display: flex;
        flex-direction: column;
        min-width: 0;

        .usage-ranking-main {
          display: flex;
          flex-direction: column;
          gap: 4px;
          min-width: 0;
        }

        .usage-ranking-line {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
          min-width: 0;

          .ranking-name-col {
            display: flex;
            align-items: center;
            gap: 7px;
            min-width: 0;
            flex: 1;
          }

          .usage-ranking-number {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            width: 18px;
            height: 18px;
            border-radius: 4px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            font-size: 11px;
            flex-shrink: 0;

            &.rank-1 {
              background: #f59e0b;
              color: #ffffff;
              box-shadow: 0 1px 4px rgba(245, 158, 11, 0.35);
            }
          }

          .ranking-entity-name {
            color: var(--color-text);
            font-size: 12px;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
          }

          .ranking-pill-primary {
            display: inline-flex;
            align-items: center;
            padding: 1px 5px;
            border-radius: 4px;
            font-size: 10px;
            color: #d97706;
            background: rgba(245, 158, 11, 0.12);
            border: 1px solid rgba(245, 158, 11, 0.25);
            flex-shrink: 0;
          }

          .ranking-tokens {
            color: #2563eb;
            font-size: 12px;
            white-space: nowrap;
            flex-shrink: 0;
          }
        }

        .usage-ranking-progress {
          width: 100%;
          height: 4px;
          border-radius: 2px;
          background: var(--color-panel-soft);
          overflow: hidden;

          span {
            display: block;
            height: 100%;
            border-radius: 2px;
            background: #2563eb;
            transition: width 0.3s ease;
          }
        }

        .usage-ranking-meta {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
          color: var(--color-text-muted);
          font-size: 10px;
        }
      }
    }

    .usage-ranking-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      margin-top: 12px;
      padding-top: 8px;
      border-top: 1px dashed var(--color-line);
      color: var(--color-text-muted);
      font-size: 11px;

      .ranking-live-status {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        color: #2563eb;

        i {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: #2563eb;
          box-shadow: 0 0 6px rgba(37, 99, 235, 0.5);
        }
      }
    }
  }

  .usage-card-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--color-line);

    &.compact {
      align-items: center;
      padding-bottom: 10px;
    }

    .usage-eyebrow {
      display: block;
      margin-bottom: 4px;
      color: #0ea5e9;
      font-size: 10px;
      letter-spacing: 0.16em;
    }

    .usage-card-title {
      margin: 0;
      color: var(--color-text);
      font-size: 15px;
    }

    .usage-period-control,
    .usage-card-note,
    .usage-cache-tag {
      color: var(--color-text-muted);
      font-size: 11px;
      white-space: nowrap;
    }

    .usage-period-control {
      display: flex;
      align-items: center;
      gap: 5px;

      .overview-period-select {
        padding: 3px 6px;
        border: 1px solid var(--color-line);
        border-radius: 5px;
        color: var(--color-text);
        background: var(--color-panel-soft);
        font-size: 11px;
        outline: none;
      }
    }

    .usage-heading-inline {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      color: #2563eb;
    }
  }

  .usage-summary-copy {
    min-width: 0;

    .usage-metric-label {
      display: block;
      color: var(--color-text-muted);
      font-size: 12px;
    }

    .usage-total-line {
      display: flex;
      align-items: baseline;
      gap: 8px;
      margin: 6px 0 8px;

      strong {
        color: var(--color-text);
        font-size: 34px;
        line-height: 1;
        letter-spacing: -0.04em;
      }

      span {
        color: var(--color-text-muted);
        font-size: 12px;
      }
    }

    .usage-breakdown-line {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 4px;
      color: var(--color-text-muted);
      font-size: 11px;

      span {
        color: #0ea5e9;
      }

      i {
        color: var(--color-text-muted);
        font-style: normal;
        opacity: 0.6;
      }

      em {
        padding: 3px 6px;
        border-radius: 5px;
        color: #8b5cf6;
        background: rgba(139, 92, 246, 0.1);
        font-style: normal;
      }
    }

    .usage-model-list {
      display: flex;
      flex-direction: column;
      gap: 9px;
      margin-top: 22px;

      .usage-model-row {
        display: grid;
        grid-template-columns: 9px minmax(0, 1fr) auto auto;
        align-items: center;
        gap: 7px;
        color: var(--color-text-muted);
        font-size: 11px;

        .usage-model-dot {
          width: 7px;
          height: 7px;
          border-radius: 50%;
          background: #10b981;

          &.tone-2 {
            background: #3b82f6;
          }

          &.tone-3 {
            background: #8b5cf6;
          }
        }

        code {
          min-width: 0;
          overflow: hidden;
          color: var(--color-text);
          font-family: Consolas, monospace;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        strong {
          color: var(--color-text);
          font-size: 10px;
        }
      }
    }
  }

  .usage-token-orbit {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 180px;
    height: 180px;
    margin: 0 auto;
    flex-shrink: 0;

    .sunburst-svg {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      pointer-events: none;
    }

    .usage-token-orbit-core {
      position: relative;
      z-index: 2;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      width: 96px;
      height: 96px;
      border: 1px solid var(--color-line);
      border-radius: 50%;
      color: var(--color-text-muted);
      background: var(--color-panel);
      box-shadow: 0 2px 12px rgba(0, 0, 0, 0.08);

      strong {
        color: var(--color-text);
        font-size: 19px;
        line-height: 1.1;
      }

      span {
        margin-top: 4px;
        color: var(--color-text-muted);
        font-size: 8px;
        letter-spacing: 0.16em;
      }
    }
  }

  .usage-summary-side {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 14px;
    padding-left: 20px;
    border-left: 1px solid var(--color-line);
    min-height: 180px;

    .summary-metric-block {
      display: flex;
      flex-direction: column;
      gap: 3px;

      .metric-top-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;

        .usage-metric-label {
          color: var(--color-text-muted);
          font-size: 11px;
        }

        .pricing-matrix-link {
          padding: 0;
          border: none;
          background: transparent;
          color: #7c3aed;
          font-size: 11px;
          cursor: pointer;
          text-decoration: none;
          white-space: nowrap;
          line-height: 1;

          &:hover {
            color: #6d28d9;
            text-decoration: underline;
          }
        }
      }

      .metric-value-row {
        display: flex;
        align-items: baseline;
        gap: 8px;
        margin-top: 2px;

        .cost-accent {
          color: var(--color-text);
          font-size: 24px;
          line-height: 1.1;
        }

        .savings-accent {
          color: #10b981;
          font-size: 24px;
          line-height: 1.1;
        }

        .cache-hit-pill {
          display: inline-flex;
          align-items: center;
          padding: 2px 7px;
          border-radius: 999px;
          background: rgba(16, 185, 129, 0.1);
          border: 1px solid rgba(16, 185, 129, 0.25);
          color: #10b981;
          font-size: 10px;
        }
      }

      .cost-subtext {
        color: var(--color-text-muted);
        font-size: 11px;
      }
    }

    .gateway-live-indicator {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding-top: 8px;
      border-top: 1px dashed var(--color-line);
      color: var(--color-text-muted);
      font-size: 11px;

      &.active {
        color: #10b981;

        .live-dot {
          background: #10b981;
          box-shadow: 0 0 6px rgba(16, 185, 129, 0.5);
        }
      }

      .live-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: var(--color-line);
      }
    }
  }

  .usage-card-footline {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 10px;
  }

  .usage-insight-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    margin-top: 14px;

    &.activity-efficiency-grid {
      grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr) minmax(0, 1fr);
    }

    .usage-card {
      min-width: 0;
      min-height: 220px;
      padding: 14px 16px;
    }
  }

  .usage-heatmap-card {
    .heatmap-icon {
      color: #2563eb;
    }

    .heatmap-streak-pill {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      padding: 2px 7px;
      border-radius: 999px;
      font-size: 10px;
      color: #ea580c;
      background: rgba(234, 88, 12, 0.1);
      border: 1px solid rgba(234, 88, 12, 0.22);
    }

    .usage-heatmap-body {
      display: grid;
      grid-template-columns: minmax(0, 1.35fr) minmax(115px, 0.85fr);
      gap: 12px;
      align-items: center;
      padding-top: 8px;
      min-height: 140px;
    }

    .heatmap-left-col {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }

    .heatmap-grid {
      display: grid;
      grid-template-columns: repeat(13, 10px);
      grid-template-rows: repeat(7, 10px);
      grid-auto-flow: column;
      gap: 4px;

      .heatmap-cell {
        width: 10px;
        height: 10px;
        border-radius: 2px;
        background: var(--color-panel-soft);
        transition:
          transform 0.1s,
          opacity 0.1s;

        &:hover {
          transform: scale(1.25);
          z-index: 1;
        }

        &.level-0 {
          background: var(--color-panel-soft);
        }
        &.level-1 {
          background: #bfdbfe;
        }
        &.level-2 {
          background: #60a5fa;
        }
        &.level-3 {
          background: #2563eb;
        }
        &.level-4 {
          background: #1e40af;
        }
      }
    }

    .heatmap-legend-line {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      color: var(--color-text-muted);
      font-size: 10px;

      .heatmap-legend {
        display: flex;
        align-items: center;
        gap: 3px;

        .legend-cell {
          width: 8px;
          height: 8px;
          border-radius: 2px;

          &.level-0 {
            background: var(--color-panel-soft);
          }
          &.level-1 {
            background: #bfdbfe;
          }
          &.level-2 {
            background: #60a5fa;
          }
          &.level-3 {
            background: #2563eb;
          }
          &.level-4 {
            background: #1e40af;
          }
        }
      }

      .heatmap-legend-note {
        color: var(--color-text-muted);
        font-size: 10px;
      }
    }

    .heatmap-right-col {
      display: flex;
      flex-direction: column;
      justify-content: center;
      gap: 8px;
      padding-left: 12px;
      border-left: 1px solid var(--color-line);

      .heatmap-stat-item {
        display: flex;
        flex-direction: column;
        gap: 2px;

        .stat-label {
          color: var(--color-text-muted);
          font-size: 11px;
        }

        .stat-value-line {
          display: flex;
          align-items: baseline;
          gap: 4px;

          strong {
            color: #2563eb;
            font-size: 18px;
            line-height: 1.2;
          }

          span {
            color: var(--color-text-muted);
            font-size: 11px;
          }
        }
      }

      .heatmap-stat-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 6px;
        font-size: 11px;

        .stat-sub-label {
          color: var(--color-text-muted);
        }

        .stat-sub-val {
          color: var(--color-text);

          &.peak {
            color: #f59e0b;
          }
        }
      }
    }
  }

  .usage-rhythm-card {
    .rhythm-icon {
      color: #f59e0b;
    }

    .usage-rhythm-body {
      display: flex;
      flex-direction: column;
      gap: 10px;
      padding: 6px 0 8px;
    }

    .rhythm-chart {
      display: flex;
      align-items: flex-end;
      gap: 4px;
      height: 64px;
      padding: 4px 0 10px;
      border-bottom: 1px solid var(--color-line);

      .rhythm-bar-col {
        flex: 1;
        height: 100%;
        display: flex;
        align-items: flex-end;
        justify-content: center;

        .rhythm-bar {
          width: 5px;
          border-radius: 2px;
          background: var(--color-line);
          transition:
            height 0.2s,
            background 0.2s;

          &.active {
            background: linear-gradient(180deg, #818cf8, #6366f1);
          }

          &.peak {
            background: linear-gradient(180deg, #f59e0b, #fbbf24);
            box-shadow: 0 0 6px rgba(245, 158, 11, 0.4);
          }
        }
      }
    }

    .rhythm-subcards {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 10px;

      .rhythm-subcard {
        padding: 8px 12px;
        border-radius: 7px;
        background: var(--color-panel-soft);
        border: 1px solid var(--color-line);

        .subcard-label {
          font-size: 11px;
          color: var(--color-text-muted);
        }

        .subcard-value-line {
          display: flex;
          align-items: baseline;
          gap: 4px;
          margin-top: 3px;

          strong {
            font-size: 16px;
            color: var(--color-text);

            &.cost-green {
              color: #10b981;
            }
          }

          span {
            font-size: 10px;
            color: var(--color-text-muted);
          }
        }
      }
    }

    .ready-badge {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      color: #10b981;

      &::before {
        content: "";
        width: 5px;
        height: 5px;
        border-radius: 50%;
        background: #10b981;
      }
    }
  }

  .usage-efficiency-card {
    .efficiency-icon {
      color: #f97316;
    }

    .efficiency-badges {
      display: flex;
      align-items: center;
      gap: 6px;

      .efficiency-pill {
        display: inline-flex;
        align-items: center;
        padding: 2px 7px;
        border-radius: 5px;
        font-size: 10px;

        &.thinking {
          color: #a855f7;
          border: 1px solid rgba(168, 85, 247, 0.25);
          background: rgba(168, 85, 247, 0.08);
        }

        &.throughput {
          color: #10b981;
          border: 1px solid rgba(16, 185, 129, 0.25);
          background: rgba(16, 185, 129, 0.08);
        }
      }
    }

    .usage-gauge-body {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: space-between;
      min-height: 150px;
      padding: 10px 0 0;

      .semicircle-gauge-container {
        position: relative;
        display: flex;
        align-items: flex-end;
        justify-content: center;
        width: 240px;
        height: 125px;

        .semicircle-gauge-svg {
          width: 100%;
          height: 100%;
          overflow: visible;
        }

        .gauge-overlay-content {
          position: absolute;
          bottom: 2px;
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 2px;

          .gauge-hit-badge {
            padding: 2px 8px;
            border-radius: 999px;
            border: 1px solid rgba(14, 165, 233, 0.3);
            background: rgba(14, 165, 233, 0.1);
            color: #0ea5e9;
            font-size: 10px;
          }

          .gauge-main-number {
            strong {
              font-size: 26px;
              color: var(--color-text);
              letter-spacing: -0.5px;
            }
          }

          .gauge-sub-caption {
            font-size: 10px;
            color: var(--color-text-muted);
            letter-spacing: 0.5px;
          }
        }
      }
    }

    .savings-highlight {
      color: #10b981;
    }
  }

  .usage-empty-inline {
    margin: auto 0;
    color: var(--color-text-muted);
    font-size: 11px;
  }

  /* 最近请求面板 */
  .recent-requests-panel {
    margin-top: 14px;

    .logs-toolbar {
      display: flex;
      align-items: center;
      gap: 10px;

      .log-view-tabs {
        display: inline-flex;
        align-items: center;
        gap: 3px;
        padding: 3px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel-soft);
        flex-shrink: 0;

        .log-view-tab {
          padding: 4px 10px;
          border: 0;
          border-radius: 4px;
          color: var(--color-text-muted);
          background: transparent;
          cursor: pointer;
          font-size: 11px;
          transition: all 0.15s ease;

          &.active {
            color: var(--color-text);
            background: var(--color-panel);
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
          }

          &:hover:not(.active) {
            color: var(--color-text);
          }
        }
      }

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
          width: 270px;
          padding: 6px 28px 6px 30px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          color: var(--color-text);
          background: var(--color-panel-soft);
          font-size: 11px;
          outline: none;
          transition: border-color 0.15s;

          &:focus {
            border-color: #7c3aed;
          }
        }
        .clear-input-btn {
          position: absolute;
          right: 7px;
          display: flex;
          align-items: center;
          justify-content: center;
          width: 16px;
          height: 16px;
          padding: 0;
          border: 0;
          border-radius: 50%;
          background: transparent;
          color: var(--color-text-muted);
          cursor: pointer;

          &:hover {
            color: var(--color-text);
          }
        }
      }

      .filter-toggle-btn {
        position: relative;

        &.active {
          border-color: #7c3aed;
          color: #7c3aed;
          background: rgba(124, 58, 237, 0.08);
        }

        .filter-badge {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          padding: 0 5px;
          height: 15px;
          border-radius: 8px;
          background: #7c3aed;
          color: #fff;
          font-size: 10px;
          margin-left: 2px;
        }
      }
    }

    /* 筛选抽屉 */
    .logs-filter-drawer {
      padding: 10px 14px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);

      .filter-drawer-row {
        display: flex;
        align-items: center;
        gap: 24px;
        flex-wrap: wrap;

        .filter-drawer-group {
          display: flex;
          align-items: center;
          gap: 8px;

          .drawer-label {
            font-size: 11px;
            color: var(--color-text-muted);
          }

          .drawer-options {
            display: flex;
            align-items: center;
            gap: 4px;

            .drawer-option-btn {
              padding: 3px 9px;
              border: 1px solid var(--color-line);
              border-radius: 5px;
              background: var(--color-panel);
              color: var(--color-text-muted);
              font-size: 11px;
              cursor: pointer;
              transition: all 0.15s;

              &:hover {
                color: var(--color-text);
              }

              &.active {
                border-color: #7c3aed;
                color: #7c3aed;
                background: rgba(124, 58, 237, 0.08);

                &.success {
                  border-color: #10b981;
                  color: #10b981;
                  background: rgba(16, 185, 129, 0.08);
                }
                &.failure {
                  border-color: #ef4444;
                  color: #ef4444;
                  background: rgba(239, 68, 68, 0.08);
                }
              }
            }
          }
        }

        .drawer-reset-btn {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 3px 8px;
          border: 0;
          background: transparent;
          color: var(--color-text-muted);
          font-size: 11px;
          cursor: pointer;

          &:hover {
            color: #7c3aed;
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
            .log-ip {
              color: var(--color-text);

              &.muted {
                color: var(--color-text-muted);
              }
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
                font-size: 10px;

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

            .status-cell-wrap {
              .status-text {
                display: inline-flex;
                align-items: center;
                gap: 4px;
                color: #10b981;

                .status-icon {
                  flex-shrink: 0;
                }
              }

              &.failed {
                .status-text {
                  color: #ef4444;
                }
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

    .ip-stats-table {
      .usage-ranking-number {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        border-radius: 4px;
        background: rgba(124, 58, 237, 0.1);
        color: #7c3aed;
        font-size: 11px;
      }

      .ip-addr-code {
        font-family: Consolas, monospace;
        color: var(--color-text);
        font-size: 11px;
      }

      .usage-ip-bar-wrap {
        display: flex;
        align-items: center;
        gap: 8px;
        min-width: 120px;

        .usage-ip-bar {
          flex: 1;
          height: 6px;
          border-radius: 3px;
          background: var(--color-line);
          overflow: hidden;

          i {
            display: block;
            height: 100%;
            border-radius: inherit;
            background: linear-gradient(90deg, #0ea5e9, #38bdf8);
          }
        }

        small {
          color: var(--color-text-muted);
          font-size: 10px;
          min-width: 32px;
        }
      }
    }
  }

  /* 调用 Token 专属表格 */
  .token-filter-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 2px;

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

  /* 表格内按钮组件 */
  .api-key-display {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 170px;

    .api-key-value {
      min-width: 0;
      max-width: 260px;
      color: #7c3aed;
      font-family: Consolas, monospace;
      font-size: 11px;
      overflow-wrap: anywhere;
    }
    .api-key-button {
      display: inline-flex;
      flex-shrink: 0;
      padding: 3px;
      border: 0;
      border-radius: 4px;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;

      &:hover:not(:disabled) {
        color: #7c3aed;
        background: var(--color-primary-soft);
      }
      &:disabled {
        opacity: 0.4;
        cursor: default;
      }
    }
  }

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

  /* 请求详情弹框样式 */
  .request-detail-modal {
    :deep(.base-modal__panel) {
      width: min(860px, calc(100vw - 48px));
      max-width: 860px;
      border-radius: 14px;
    }

    :deep(.base-modal__header) {
      padding: 18px 22px 10px;
      align-items: center;
    }

    .modal-header-left {
      display: flex;
      align-items: center;
      gap: 12px;

      .modal-icon-badge {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 38px;
        height: 38px;
        border-radius: 10px;
        color: #7c3aed;
        background: rgba(124, 58, 237, 0.1);
        flex-shrink: 0;
      }

      .modal-header-titles {
        display: flex;
        flex-direction: column;
        gap: 2px;

        .modal-main-title {
          margin: 0;
          font-size: 16px;
          color: var(--color-text);
          line-height: 1.3;
        }

        .modal-id-subtitle {
          margin: 0;
          font-size: 11px;
          color: var(--color-text-muted);
          font-family: Consolas, monospace;
          line-height: 1.3;
        }
      }
    }

    .modal-close-icon-btn {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 30px;
      height: 30px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      cursor: pointer;
      transition: all 0.15s;

      &:hover {
        color: var(--color-text);
        border-color: var(--color-text-muted);
      }
    }

    .request-detail-view {
      display: flex;
      flex-direction: column;
      gap: 12px;
      min-height: 0;

      .request-detail-meta-card {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 8px 24px;
        padding: 12px 16px;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background: var(--color-panel-soft);

        .meta-column {
          display: flex;
          flex-direction: column;
          gap: 8px;

          .meta-row {
            display: flex;
            align-items: center;
            gap: 10px;
            font-size: 11px;

            .meta-label-group {
              display: inline-flex;
              align-items: center;
              gap: 5px;
              width: 72px;
              flex-shrink: 0;
              color: var(--color-text-muted);

              .meta-icon {
                color: var(--color-text-muted);
                flex-shrink: 0;

                &.ok-color {
                  color: #10b981;
                }
                &.err-color {
                  color: #ef4444;
                }
              }

              .meta-label {
                font-size: 11px;
                color: var(--color-text-muted);
              }
            }

            .meta-value {
              display: inline-flex;
              align-items: center;
              gap: 6px;
              color: var(--color-text);
              font-size: 11px;
              min-width: 0;
              overflow-wrap: anywhere;

              .method-tag {
                color: var(--color-text);
              }

              .meta-arrow {
                color: var(--color-text-muted);
              }

              .meta-provider-pill {
                display: inline-flex;
                align-items: center;
                padding: 1px 7px;
                border-radius: 4px;
                color: #7c3aed;
                background: rgba(124, 58, 237, 0.1);
                font-size: 11px;
              }

              .meta-status-pill {
                display: inline-flex;
                align-items: center;
                padding: 1px 7px;
                border-radius: 4px;
                font-size: 11px;

                &.ok {
                  color: #10b981;
                  background: rgba(16, 185, 129, 0.12);
                }
                &.err {
                  color: #ef4444;
                  background: rgba(239, 68, 68, 0.12);
                }
                &.muted {
                  color: var(--color-text-muted);
                  background: var(--color-panel);
                }
              }
            }
          }
        }
      }

      .request-detail-tabs {
        display: flex;
        gap: 6px;
        border-bottom: 1px solid var(--color-line);
        margin-top: 4px;

        .request-detail-tab {
          padding: 8px 14px;
          border: 0;
          border-bottom: 2px solid transparent;
          color: var(--color-text-muted);
          background: transparent;
          cursor: pointer;
          font-size: 12px;
          transition: all 0.15s;

          &.active {
            border-bottom-color: #7c3aed;
            color: #7c3aed;
          }

          &:hover:not(.active) {
            color: var(--color-text);
          }
        }
      }

      .request-detail-body {
        margin: 0;
        min-height: 160px;
        max-height: 48vh;
        padding: 14px;
        overflow: auto;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        color: var(--color-text);
        background: var(--color-panel-soft);
        font-family: Consolas, "Fira Code", monospace;
        font-size: 12px;
        line-height: 1.6;
        white-space: pre-wrap;
        overflow-wrap: anywhere;
      }

      .request-detail-hint {
        margin: 0;
        padding: 24px;
        color: var(--color-text-muted);
        font-size: 12px;
        line-height: 1.6;
        text-align: center;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 6px;
      }

      .request-detail-error {
        margin: 0;
        padding: 8px 12px;
        border-radius: 6px;
        border: 1px solid rgba(239, 68, 68, 0.3);
        color: #ef4444;
        background: rgba(239, 68, 68, 0.06);
        font-size: 11px;
        overflow-wrap: anywhere;
        display: flex;
        align-items: center;
        gap: 6px;

        .error-icon {
          flex-shrink: 0;
        }
      }

      .request-detail-actions {
        display: flex;
        justify-content: flex-end;
        gap: 10px;
        padding-top: 8px;
      }
    }
  }

  .pool-modal {
    :deep(.base-modal__header) {
      flex-shrink: 0;
    }

    .modal-form {
      min-height: 0;

      .modal-footer-actions {
        flex-shrink: 0;
      }
    }
  }

  /* 弹框表单 */
  .modal-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 6px;

    .pool-content {
      display: flex;
      flex-direction: column;
      gap: 12px;
      min-height: 0;

      .pool-header-bar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;

        .pool-summary {
          margin: 0;
          color: var(--color-text);
          font-size: 12px;
        }

        .pool-batch-actions {
          display: flex;
          align-items: center;
          gap: 6px;

          .pool-action-link {
            background: transparent;
            border: none;
            color: #7c3aed;
            font-size: 11px;
            cursor: pointer;
            padding: 2px 4px;
            border-radius: 4px;
            transition: background-color 0.15s;

            &:hover {
              background: rgba(124, 58, 237, 0.08);
            }
          }

          .pool-action-divider {
            color: var(--color-line);
          }
        }
      }

      .target-list {
        display: flex;
        flex-direction: column;
        gap: 14px;
        max-height: 380px;
        overflow-y: auto;
        padding-right: 4px;

        .target-group {
          display: flex;
          flex-direction: column;
          gap: 8px;

          .target-group-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 2px 2px 0;

            .target-group-title {
              display: flex;
              align-items: center;
              gap: 6px;

              .group-title-text {
                color: var(--color-text);
                font-size: 12px;
                opacity: 0.85;
              }

              .group-count-badge {
                display: inline-flex;
                align-items: center;
                padding: 1px 6px;
                border-radius: 999px;
                background: rgba(124, 58, 237, 0.08);
                color: #7c3aed;
                font-size: 11px;
              }
            }

            .group-action-btn {
              background: transparent;
              border: none;
              color: #7c3aed;
              font-size: 11px;
              cursor: pointer;
              padding: 2px 6px;
              border-radius: 4px;
              transition: background-color 0.15s;

              &:hover {
                background: rgba(124, 58, 237, 0.08);
              }
            }
          }

          .target-group-items {
            display: flex;
            flex-direction: column;
            gap: 8px;
          }
        }

        .target-item {
          display: flex;
          align-items: center;
          gap: 10px;
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
              color: var(--color-text);
              font-size: 13px;
              overflow-wrap: anywhere;
            }

            .target-type {
              color: var(--color-text-muted);
              font-size: 11px;
            }
          }

          .target-status-badge {
            color: #10b981;
            font-size: 11px;
            white-space: nowrap;
          }
        }
      }

      .pool-hint {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 11px;
        line-height: 1.5;
      }
    }

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

  .pricing-matrix-modal {
    display: flex;
    flex-direction: column;
    gap: 14px;

    .pricing-table-wrap {
      overflow-x: auto;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);

      .pricing-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 12px;

        th,
        td {
          padding: 8px 12px;
          text-align: left;
          border-bottom: 1px solid var(--color-line);
        }

        th {
          color: var(--color-text-muted);
          background: var(--color-panel-soft);
          font-size: 11px;
        }

        td {
          color: var(--color-text);

          strong {
            display: block;
            color: var(--color-text);
          }

          small {
            display: block;
            color: var(--color-text-muted);
            font-size: 10px;
          }

          &.saving-cell {
            color: #10b981;
          }
        }

        tbody tr:last-child td {
          border-bottom: none;
        }
      }

      .pricing-empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 36px 16px;
        color: var(--color-text-muted);
        text-align: center;

        p {
          margin: 0;
          font-size: 13px;
          color: var(--color-text);
        }

        span {
          font-size: 11px;
        }
      }
    }

    .pricing-notes {
      display: flex;
      flex-direction: column;
      gap: 6px;
      font-size: 11px;
      color: var(--color-text-muted);
      line-height: 1.5;

      p {
        margin: 0;
      }

      strong {
        color: var(--color-text);
      }
    }
  }

  :deep(.base-modal__panel) {
    max-width: 560px;
    width: 100%;
  }
}
</style>

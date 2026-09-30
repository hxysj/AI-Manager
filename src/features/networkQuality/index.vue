<template>
  <section class="network-quality">
    <!-- 顶部操作工具栏 -->
    <header class="network-quality-toolbar">
      <div class="network-quality-toolbar-left">
        <!-- 地址族 -->
        <div class="network-quality-field-group">
          <span class="network-quality-field-label">地址族</span>
          <div
            class="network-quality-segmented"
            role="radiogroup"
            aria-label="诊断地址族"
          >
            <button
              v-for="item in familyOptions"
              :key="item.value"
              type="button"
              class="network-quality-seg-btn"
              :class="{ 'is-active': request.family === item.value }"
              :disabled="isBusy"
              @click="request.family = item.value"
            >
              {{ item.label }}
            </button>
          </div>
        </div>

        <!-- 检测项复选框 -->
        <div
          class="network-quality-field-group"
          role="group"
          aria-label="检测项"
        >
          <span class="network-quality-field-label">检测项</span>
          <div class="network-quality-checkbox-group">
            <button
              v-for="item in moduleOptions"
              :key="item.value"
              type="button"
              class="network-quality-check-btn"
              :class="{ 'is-checked': request.modules[item.value] }"
              :disabled="isBusy"
              @click="
                request.modules[item.value] = !request.modules[item.value]
              "
            >
              <span class="network-quality-checkbox-box">
                <Check
                  v-if="request.modules[item.value]"
                  :size="12"
                  :stroke-width="3"
                />
              </span>
              <span class="network-quality-checkbox-text">{{
                item.label
              }}</span>
            </button>
          </div>
        </div>

        <!-- 携带公网地址开关 -->
        <label
          class="network-quality-toggle-label"
          title="开启后报告中保留完整公网 IP 地址，关闭后自动掩码"
        >
          <input
            v-model="carryPublicIp"
            type="checkbox"
            :disabled="isBusy"
            @change="handleCarryPublicIpChange"
          />
          <span class="network-quality-toggle-slider" />
          <span class="network-quality-toggle-title">携带公网地址</span>
        </label>
      </div>

      <!-- 右侧操作按钮组 -->
      <div class="network-quality-toolbar-actions">
        <button
          class="network-quality-btn is-outline"
          type="button"
          :disabled="!report || isBusy || exporting"
          title="将当前报告导出为 JSON 文件"
          @click="exportJson"
        >
          <LoaderCircle v-if="exporting" class="is-spinning" :size="14" />
          <Download v-else :size="14" />
          <span>{{ exporting ? "导出中" : "导出 JSON" }}</span>
        </button>

        <button
          v-if="canCancel"
          class="network-quality-btn is-danger"
          type="button"
          :disabled="cancelPending"
          @click="cancelDiagnosis"
        >
          <LoaderCircle v-if="cancelPending" class="is-spinning" :size="14" />
          <Square v-else :size="13" />
          <span>{{ cancelPending ? "正在取消" : "取消诊断" }}</span>
        </button>

        <button
          v-else
          class="network-quality-btn is-primary"
          type="button"
          :disabled="isBusy"
          @click="runDiagnosis"
        >
          <LoaderCircle v-if="isBusy" class="is-spinning" :size="14" />
          <Play v-else :size="14" fill="currentColor" />
          <span>{{
            isBusy ? "诊断中" : report ? "重新诊断" : "开始诊断"
          }}</span>
        </button>
      </div>
    </header>

    <!-- 诊断状态提示条 -->
    <div
      v-if="status !== 'idle'"
      class="network-quality-status-banner"
      :class="`is-${statusMeta.tone}`"
      role="status"
    >
      <div class="network-quality-status-icon">
        <LoaderCircle v-if="isBusy" class="is-spinning" :size="18" />
        <div
          v-else-if="
            status === 'success' &&
            !warningFindings.length &&
            !partialErrorCount
          "
          class="status-circle is-success"
        >
          <Check :size="12" :stroke-width="3" />
        </div>
        <div
          v-else-if="
            status === 'cancelled' ||
            warningFindings.length ||
            partialErrorCount
          "
          class="status-circle is-warning"
        >
          <span>!</span>
        </div>
        <div v-else class="status-circle is-danger">
          <span>✕</span>
        </div>
      </div>

      <div class="network-quality-status-body">
        <strong class="network-quality-status-headline">{{
          statusMeta.label
        }}</strong>
        <p class="network-quality-status-desc">{{ statusMeta.message }}</p>

        <!-- 进度条（诊断中展示） -->
        <div v-if="isBusy" class="network-quality-progress-line">
          <div
            class="network-quality-progress-active"
            :style="{ width: `${progress.percent}%` }"
          />
        </div>
      </div>

      <div
        v-if="status === 'error' && !report"
        class="network-quality-status-retry"
      >
        <button
          type="button"
          class="network-quality-btn is-danger is-sm"
          :disabled="isBusy"
          @click="runDiagnosis"
        >
          <RotateCcw :size="13" />
          重试
        </button>
      </div>
    </div>

    <!-- 主展示区 -->
    <div class="network-quality-content">
      <!-- 初始待诊断空状态 -->
      <div v-if="status === 'idle' && !report" class="network-quality-welcome">
        <div class="network-quality-radar-circle">
          <div class="radar-wave wave-1" />
          <div class="radar-wave wave-2" />
          <Activity :size="36" class="radar-icon" />
        </div>
        <h2 class="network-quality-welcome-title">准备开始网络质量多维诊断</h2>
        <p class="network-quality-welcome-desc">
          系统将综合检测公网出口身份、多源地理位置共识、系统连通性阻断、主流云服务与
          AI 端点可用性，以及全网链路基线延迟与丢包指标。
        </p>
        <div class="network-quality-welcome-btns">
          <button
            type="button"
            class="network-quality-btn is-primary is-lg"
            :disabled="isBusy"
            @click="runDiagnosis"
          >
            <Play :size="15" fill="currentColor" />
            开始诊断
          </button>
        </div>
      </div>

      <!-- 诊断报告主面板 -->
      <template v-if="report">
        <!-- 核心遥测双列 Hero 卡片网格 (严格还原 2.png 布局) -->
        <div class="network-quality-dashboard-grid">
          <!-- 左侧列：测量完整性 + 网络质量评分 -->
          <div class="network-quality-left-col">
            <!-- 卡片 1: 测量完整性 -->
            <article class="network-quality-panel-card">
              <header class="network-quality-card-head">
                <div class="network-quality-card-title">
                  <Shield :size="17" class="head-icon is-blue" />
                  <h3>测量完整性</h3>
                </div>
                <span class="network-quality-pill-tag is-blue">
                  {{ findings.length }} 项提示
                </span>
              </header>

              <div class="network-quality-card-body">
                <div
                  v-if="findings.length"
                  class="network-quality-findings-list"
                >
                  <div
                    v-for="(finding, index) in findings"
                    :key="finding.code || finding.id || index"
                    class="network-quality-callout-item"
                    :class="`is-${findingTone(finding)}`"
                  >
                    <div class="callout-circle-icon">
                      <span v-if="findingTone(finding) === 'warning'">!</span>
                      <span v-else-if="findingTone(finding) === 'info'">i</span>
                      <span v-else>✕</span>
                    </div>

                    <div class="callout-content">
                      <strong class="callout-title">{{
                        findingTitle(finding)
                      }}</strong>
                      <p class="callout-desc">{{ findingMessage(finding) }}</p>
                    </div>

                    <ChevronRight :size="15" class="callout-arrow" />
                  </div>
                </div>

                <div v-else class="network-quality-clear-item">
                  <CheckCircle2 :size="17" class="text-success" />
                  <span>已启用检测未发现异常，网络栈测量正常。</span>
                </div>
              </div>
            </article>

            <!-- 卡片 2: 网络质量评分 -->
            <article class="network-quality-panel-card">
              <header class="network-quality-card-head">
                <div class="network-quality-card-title">
                  <Shield :size="17" class="head-icon is-blue" />
                  <h3>网络质量评分</h3>
                </div>
              </header>

              <div class="network-quality-card-body">
                <div class="network-quality-score-row">
                  <!-- 左侧：270° 环形健康评分仪表盘 -->
                  <div class="network-quality-gauge-wrapper">
                    <svg
                      class="network-quality-gauge-svg"
                      viewBox="0 0 120 120"
                    >
                      <!-- 背景底轨 -->
                      <circle
                        class="gauge-bg-track"
                        cx="60"
                        cy="60"
                        r="46"
                        stroke-dasharray="216.77 289.03"
                        transform="rotate(135 60 60)"
                      />
                      <!-- 前景激活进度弧线 -->
                      <circle
                        class="gauge-active-bar"
                        :class="`is-${gradeTone}`"
                        cx="60"
                        cy="60"
                        r="46"
                        stroke-dasharray="216.77 289.03"
                        :style="{ strokeDashoffset: gaugeStrokeOffset }"
                        transform="rotate(135 60 60)"
                      />
                    </svg>

                    <div class="gauge-center-content">
                      <span class="gauge-number">{{ scoreText }}</span>
                      <span class="gauge-divider">/ 100</span>
                      <div class="gauge-grade-badge" :class="`is-${gradeTone}`">
                        {{ displayGrade }}
                      </div>
                    </div>
                  </div>

                  <!-- 右侧：四项核心指标行 -->
                  <div class="network-quality-metrics-list">
                    <div class="network-quality-metric-row">
                      <div class="metric-row-label">
                        <Gauge :size="15" class="metric-icon" />
                        <span>延迟基线</span>
                      </div>
                      <strong class="metric-row-val">
                        {{
                          formatRtt(
                            measuredPathTargets.length
                              ? connectivity.floorMs
                              : null
                          )
                        }}
                      </strong>
                    </div>

                    <div class="network-quality-metric-row">
                      <div class="metric-row-label">
                        <Timer :size="15" class="metric-icon" />
                        <span>中位延迟</span>
                      </div>
                      <strong class="metric-row-val">
                        {{
                          formatRtt(
                            measuredPathTargets.length
                              ? connectivity.medianRttMs
                              : null
                          )
                        }}
                      </strong>
                    </div>

                    <div class="network-quality-metric-row">
                      <div class="metric-row-label">
                        <Clock3 :size="15" class="metric-icon" />
                        <span>诊断耗时</span>
                      </div>
                      <strong class="metric-row-val">
                        {{ formatDuration(report.durationMs) }}
                      </strong>
                    </div>

                    <div class="network-quality-metric-row">
                      <div class="metric-row-label">
                        <Route :size="15" class="metric-icon" />
                        <span>路径能力</span>
                      </div>
                      <strong
                        class="metric-row-val"
                        :title="capability.hint || ''"
                      >
                        {{ capabilityLabel }}
                      </strong>
                    </div>
                  </div>
                </div>
              </div>
            </article>
          </div>

          <!-- 右侧列：公网身份与归属卡片 -->
          <div class="network-quality-right-col">
            <article
              v-for="item in identities"
              :key="item.family"
              class="network-quality-panel-card"
            >
              <header class="network-quality-card-head">
                <div class="network-quality-card-title">
                  <Globe :size="17" class="head-icon is-blue" />
                  <h3>公网身份</h3>
                </div>

                <div class="network-quality-head-meta">
                  <span class="meta-source-text">{{
                    formatSourceCount(item)
                  }}</span>
                  <span
                    v-if="identityConflicts(item).length"
                    class="meta-divider"
                    >|</span
                  >
                  <span
                    v-if="identityConflicts(item).length"
                    class="network-quality-pill-tag is-red"
                  >
                    {{ identityConflicts(item).length }} 项冲突
                  </span>
                </div>
              </header>

              <div class="network-quality-card-body">
                <!-- IP 与地理位置双区块 -->
                <div class="network-quality-passport-hero">
                  <!-- 左侧 IP 信息 -->
                  <div class="passport-ip-pane">
                    <span class="passport-family-label"
                      >{{ item.family }} 地址</span
                    >
                    <div class="passport-ip-row">
                      <strong class="passport-ip-text">{{
                        item.data.address || "未识别"
                      }}</strong>
                      <button
                        v-if="item.data.address"
                        type="button"
                        class="passport-copy-btn"
                        :title="
                          copiedAddress === item.data.address
                            ? '已复制'
                            : '复制 IP'
                        "
                        @click="copyAddress(item.data.address)"
                      >
                        <Check
                          v-if="copiedAddress === item.data.address"
                          :size="14"
                          class="text-success"
                        />
                        <Copy v-else :size="14" />
                      </button>
                    </div>
                    <span class="passport-votes-tag">{{
                      formatVotes(item.data.sourceVotes)
                    }}</span>
                  </div>

                  <!-- 右侧地理位置卡片 (集成中国/世界地图轮廓水印) -->
                  <div class="passport-location-card">
                    <div class="location-text-wrap">
                      <div class="location-main-line">
                        <MapPin :size="15" class="location-pin-icon" />
                        <strong :title="identityLocation(item)">{{
                          identityLocation(item)
                        }}</strong>
                      </div>
                      <p
                        class="location-sub-line"
                        :title="identityNetwork(item)"
                      >
                        {{ identityNetwork(item) }}
                      </p>
                    </div>

                    <!-- 地图轮廓水印背景 -->
                    <div class="passport-map-watermark">
                      <svg
                        class="map-svg"
                        viewBox="0 0 100 80"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                      >
                        <path
                          d="M75,18 C78,16 82,14 85,15 C88,18 90,24 88,28 C85,32 80,35 78,38 C75,42 76,46 78,48 C80,50 82,54 80,57 C78,60 74,62 70,62 C68,64 65,66 62,64 C58,62 55,65 52,65 C48,65 45,63 42,62 C38,62 35,64 32,62 C28,60 25,58 24,54 C22,50 25,46 28,44 C30,42 32,38 30,35 C28,32 25,30 22,28 C20,25 24,22 28,24 C32,26 36,25 40,24 C44,22 48,22 52,24 C56,26 60,25 64,22 C68,20 72,20 75,18 Z"
                          fill="currentColor"
                        />
                        <circle cx="68" cy="56" r="3" fill="#2563eb" />
                        <circle
                          cx="68"
                          cy="56"
                          r="6"
                          stroke="#2563eb"
                          stroke-width="1"
                          class="ping-circle"
                        />
                      </svg>
                    </div>
                  </div>
                </div>

                <!-- 网络与地理信息规格网格 -->
                <div class="network-quality-specs-section">
                  <h4 class="specs-section-title">网络与地理信息</h4>

                  <div class="specs-grid-3col">
                    <!-- Row 1 -->
                    <div class="spec-cell">
                      <span class="spec-label">注册国家/地区</span>
                      <strong class="spec-val">
                        {{ getCountryDisplay(item) }}
                      </strong>
                    </div>

                    <div class="spec-cell">
                      <span class="spec-label">RIR</span>
                      <strong class="spec-val">
                        {{ displayValue(identityIntel(item)?.rir) }}
                      </strong>
                    </div>

                    <div class="spec-cell">
                      <span class="spec-label">分配前缀</span>
                      <strong class="spec-val">
                        {{ displayValue(identityIntel(item)?.allocationCidr) }}
                      </strong>
                    </div>

                    <!-- Row 2 -->
                    <div class="spec-cell">
                      <span class="spec-label">Origin ASN</span>
                      <strong class="spec-val">
                        {{ displayList(identityIntel(item)?.originAsns) }}
                      </strong>
                    </div>

                    <div class="spec-cell">
                      <span class="spec-label">AS 组织 / 运营商</span>
                      <div class="spec-val-stacked">
                        <strong class="spec-val">
                          {{
                            formatAsn(
                              identityIntel(item)?.asn || item.data?.asn
                            )
                          }}
                        </strong>
                        <span
                          class="spec-val-sub"
                          :title="
                            identityIntel(item)?.organization ||
                            identityIntel(item)?.isp ||
                            item.data?.organization ||
                            item.data?.isp ||
                            '--'
                          "
                        >
                          {{
                            identityIntel(item)?.organization ||
                            identityIntel(item)?.isp ||
                            item.data?.organization ||
                            item.data?.isp ||
                            "--"
                          }}
                        </span>
                      </div>
                    </div>

                    <div class="spec-cell">
                      <span class="spec-label">查询结果</span>
                      <strong class="spec-val">
                        {{ displayValue(identityIntel(item)?.queryResult) }}
                      </strong>
                    </div>

                    <!-- Row 3 -->
                    <div class="spec-cell">
                      <span class="spec-label">反向解析 (PTR)</span>
                      <strong class="spec-val">
                        {{ displayValue(identityIntel(item)?.ptr) }}
                      </strong>
                    </div>

                    <div class="spec-cell">
                      <span class="spec-label">正向解析</span>
                      <strong class="spec-val">
                        {{ displayValue(identityIntel(item)?.announcedPrefix) }}
                      </strong>
                    </div>

                    <div class="spec-cell">
                      <!-- 留空占位以保持 3 列对齐 -->
                    </div>
                  </div>
                </div>
              </div>
            </article>
          </div>
        </div>

        <!-- 详细数据报表区域 -->
        <div class="network-quality-detail-tables">
          <!-- 报表 1: 地理位置共识与检查源明细 -->
          <article
            v-if="request.modules.geo"
            class="network-quality-panel-card"
          >
            <header class="network-quality-card-head">
              <div class="network-quality-card-title">
                <Network :size="17" class="head-icon is-blue" />
                <h3>地理位置共识与检查源</h3>
              </div>
              <span class="network-quality-pill-tag is-blue">
                {{ consensusRows.length }} 项国家/地区共识
              </span>
            </header>

            <div class="network-quality-card-body">
              <!-- 共识列表 -->
              <div
                v-if="consensusRows.length"
                class="network-quality-table-shell"
              >
                <table class="network-quality-table">
                  <thead>
                    <tr>
                      <th style="width: 80px">地址族</th>
                      <th style="width: 220px">国家 / 地区</th>
                      <th style="width: 100px">票数</th>
                      <th style="width: 80px">占比</th>
                      <th>共识强度</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="item in consensusRows" :key="item.key">
                      <td>
                        <span class="table-family-pill">{{ item.family }}</span>
                      </td>
                      <td>
                        <div class="table-cell-title">
                          <strong>{{ consensusCountry(item.entry) }}</strong>
                          <span class="table-code">{{
                            consensusCode(item.entry)
                          }}</span>
                        </div>
                      </td>
                      <td>
                        <span class="table-num">{{
                          consensusVotes(item.entry)
                        }}</span>
                      </td>
                      <td>
                        <strong class="table-num">{{
                          consensusPercentText(item.entry)
                        }}</strong>
                      </td>
                      <td>
                        <div class="table-consensus-progress">
                          <span
                            class="progress-fill"
                            :style="{
                              width: `${consensusPercent(item.entry)}%`
                            }"
                          />
                        </div>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <!-- 检查源明细 -->
              <div
                v-if="geoResults.length"
                class="network-quality-sub-table-block"
              >
                <h4 class="specs-section-title">
                  检查源明细 ({{ geoResults.length }} 个数据源)
                </h4>
                <div class="network-quality-table-shell">
                  <table class="network-quality-table">
                    <thead>
                      <tr>
                        <th style="width: 200px">检查源</th>
                        <th style="width: 100px">分组</th>
                        <th>IPv4 结果</th>
                        <th style="width: 90px">耗时</th>
                        <th>IPv6 结果</th>
                        <th style="width: 90px">耗时</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr
                        v-for="(item, index) in geoResults"
                        :key="item.id || index"
                      >
                        <td>
                          <div class="table-cell-title">
                            <strong>{{ item.name || item.id }}</strong>
                            <span>{{ item.vendor || "--" }}</span>
                          </div>
                        </td>
                        <td>
                          <span class="table-tag">{{
                            item.group || item.kind || "GeoIP"
                          }}</span>
                        </td>
                        <td>
                          <span
                            class="table-state-badge"
                            :class="`is-${stateTone(geoOutcome(item, 'ipv4')?.state)}`"
                          >
                            {{ geoOutcomeText(geoOutcome(item, "ipv4")) }}
                          </span>
                        </td>
                        <td>
                          <span class="table-num">{{
                            formatRtt(geoOutcome(item, "ipv4")?.rttMs)
                          }}</span>
                        </td>
                        <td>
                          <span
                            class="table-state-badge"
                            :class="`is-${stateTone(geoOutcome(item, 'ipv6')?.state)}`"
                          >
                            {{ geoOutcomeText(geoOutcome(item, "ipv6")) }}
                          </span>
                        </td>
                        <td>
                          <span class="table-num">{{
                            formatRtt(geoOutcome(item, "ipv6")?.rttMs)
                          }}</span>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </article>

          <!-- 报表 2: 系统连通性与俘获门户 -->
          <article
            v-if="request.modules.portal"
            class="network-quality-panel-card"
          >
            <header class="network-quality-card-head">
              <div class="network-quality-card-title">
                <Wifi :size="17" class="head-icon is-blue" />
                <h3>系统连通性与俘获门户</h3>
              </div>
              <div class="network-quality-head-flags">
                <span
                  class="table-state-badge"
                  :class="
                    connectivityChecks.clean ? 'is-success' : 'is-warning'
                  "
                >
                  {{ connectivityChecks.clean ? "链路干净" : "存在异常" }}
                </span>
                <span
                  v-if="connectivityChecks.plainHttpBlocked"
                  class="table-state-badge is-danger"
                >
                  明文 HTTP 受限
                </span>
              </div>
            </header>

            <div class="network-quality-card-body">
              <div
                v-if="portalResults.length"
                class="network-quality-table-shell"
              >
                <table class="network-quality-table">
                  <thead>
                    <tr>
                      <th style="width: 240px">端点</th>
                      <th style="width: 120px">厂商</th>
                      <th style="width: 110px">判定</th>
                      <th style="width: 85px">HTTP</th>
                      <th style="width: 95px">耗时</th>
                      <th>明细说明</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="(item, index) in portalResults"
                      :key="item.id || index"
                    >
                      <td>
                        <div class="table-cell-title">
                          <strong>{{ item.name || item.id }}</strong>
                          <span class="table-url">{{ item.url }}</span>
                        </div>
                      </td>
                      <td>
                        <span class="table-tag">{{ item.vendor || "--" }}</span>
                      </td>
                      <td>
                        <span
                          class="table-state-badge"
                          :class="`is-${stateTone(item.verdict)}`"
                        >
                          {{ stateLabel(item.verdict) }}
                        </span>
                      </td>
                      <td>
                        <code class="table-code">{{
                          formatHttpStatus(item.status)
                        }}</code>
                      </td>
                      <td>
                        <span class="table-num">{{
                          formatRtt(item.rttMs)
                        }}</span>
                      </td>
                      <td>
                        <span class="table-note">{{
                          item.error || item.detail || "--"
                        }}</span>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div v-else class="table-empty-box">未运行系统连通性检查</div>
            </div>
          </article>

          <!-- 报表 3: 服务与 AI 可达性 (左右双列) -->
          <article
            v-if="request.modules.access || request.modules.ai"
            class="network-quality-panel-card"
          >
            <header class="network-quality-card-head">
              <div class="network-quality-card-title">
                <Server :size="17" class="head-icon is-blue" />
                <h3>服务与 AI 可达性</h3>
              </div>
              <span class="network-quality-pill-tag is-blue">
                {{ serviceResults.length + aiResults.length }} 个端点
              </span>
            </header>

            <div class="network-quality-card-body">
              <div class="network-quality-dual-tables">
                <!-- 消费服务 -->
                <div class="dual-table-col">
                  <h4 class="specs-section-title">
                    消费与云服务 ({{ serviceResults.length }})
                  </h4>
                  <div class="network-quality-table-shell">
                    <table class="network-quality-table">
                      <thead>
                        <tr>
                          <th>服务</th>
                          <th style="width: 90px">状态</th>
                          <th style="width: 80px">区域</th>
                          <th style="width: 80px">耗时</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr
                          v-for="(item, index) in serviceResults"
                          :key="item.id || index"
                        >
                          <td>
                            <div class="table-cell-title">
                              <strong>{{ item.name || item.id }}</strong>
                              <span>{{ item.vendor || "" }}</span>
                            </div>
                          </td>
                          <td>
                            <span
                              class="table-state-badge"
                              :class="`is-${stateTone(item.state)}`"
                            >
                              {{ stateLabel(item.state) }}
                            </span>
                          </td>
                          <td>
                            <span class="table-region">{{
                              localizedServiceRegion(item.region) || "--"
                            }}</span>
                          </td>
                          <td>
                            <span class="table-num">{{
                              formatRtt(item.rttMs)
                            }}</span>
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>

                <!-- AI API -->
                <div class="dual-table-col">
                  <h4 class="specs-section-title">
                    AI 模型 API ({{ aiResults.length }})
                  </h4>
                  <div class="network-quality-table-shell">
                    <table class="network-quality-table">
                      <thead>
                        <tr>
                          <th>端点</th>
                          <th style="width: 90px">状态</th>
                          <th style="width: 75px">HTTP</th>
                          <th style="width: 80px">耗时</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr
                          v-for="(item, index) in aiResults"
                          :key="item.id || index"
                        >
                          <td>
                            <div class="table-cell-title">
                              <strong>{{ item.name || item.id }}</strong>
                              <span>{{ item.vendor || "" }}</span>
                            </div>
                          </td>
                          <td>
                            <span
                              class="table-state-badge"
                              :class="`is-${stateTone(item.state)}`"
                            >
                              {{ stateLabel(item.state) }}
                            </span>
                          </td>
                          <td>
                            <code class="table-code">{{
                              item.httpStatus ?? "--"
                            }}</code>
                          </td>
                          <td>
                            <span class="table-num">{{
                              formatRtt(item.rttMs)
                            }}</span>
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              </div>
            </div>
          </article>

          <!-- 报表 4: 路径与目标连接质量 -->
          <article
            v-if="request.modules.path"
            class="network-quality-panel-card"
          >
            <header class="network-quality-card-head">
              <div class="network-quality-card-title">
                <Route :size="17" class="head-icon is-blue" />
                <h3>路径与连接质量</h3>
              </div>
              <span class="table-tag" :title="capability.hint || ''">
                {{ capabilitySummary }}
              </span>
            </header>

            <div class="network-quality-card-body">
              <div
                v-if="pathTargets.length"
                class="network-quality-table-shell"
              >
                <table class="network-quality-table">
                  <thead>
                    <tr>
                      <th style="width: 170px">目标</th>
                      <th style="width: 110px">网络类型</th>
                      <th style="width: 140px">测量方式 / 解析 IP</th>
                      <th style="width: 130px">路径判定</th>
                      <th style="width: 130px">RTT / 抖动</th>
                      <th style="width: 110px">丢包 / 跳数</th>
                      <th>说明</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="(target, index) in pathTargets"
                      :key="target.id || index"
                    >
                      <td>
                        <div class="table-cell-title">
                          <strong>{{ target.name || target.id }}</strong>
                          <span class="table-url">{{ target.host }}</span>
                        </div>
                      </td>
                      <td>
                        <span class="table-tag">{{
                          target.network || "--"
                        }}</span>
                      </td>
                      <td>
                        <div class="table-cell-title">
                          <span>{{ target.method || "--" }}</span>
                          <code class="table-code">{{
                            target.resolvedIp || ""
                          }}</code>
                        </div>
                      </td>
                      <td>
                        <div class="table-verdict-box">
                          <span
                            class="table-state-badge"
                            :class="`is-${stateTone(target.verdict?.class)}`"
                          >
                            {{ pathStateLabel(target) }}
                          </span>
                          <span class="table-score">{{
                            formatScore(target.verdict?.score)
                          }}</span>
                        </div>
                      </td>
                      <td>
                        <span class="table-num">
                          {{ formatTargetRtt(target, "rttMs") }} /
                          {{ formatTargetRtt(target, "jitterMs") }}
                        </span>
                      </td>
                      <td>
                        <span
                          class="table-num"
                          :class="{
                            'is-loss': Number(target?.verdict?.loss) > 0
                          }"
                        >
                          {{ formatTargetLoss(target) }} /
                          {{ formatTargetHops(target) }}
                        </span>
                      </td>
                      <td>
                        <span class="table-note">{{
                          target.error ||
                          formatNotes(target.verdict?.notes) ||
                          "--"
                        }}</span>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div v-else class="table-empty-box">未运行目标连接检查</div>
            </div>
          </article>
        </div>

        <!-- 底部引擎元数据 -->
        <footer class="network-quality-footer">
          <div class="footer-meta-item">
            <span>引擎:</span>
            <strong>{{ formatTool(report.tool) }}</strong>
          </div>
          <div class="footer-meta-item">
            <span>Schema:</span>
            <span>v{{ report.schema }}</span>
          </div>
          <div class="footer-meta-item">
            <span>诊断时间:</span>
            <span>{{ formatDateTime(report.timestamp) }}</span>
          </div>
          <div v-if="partialErrorCount" class="footer-meta-item is-warning">
            <AlertTriangle :size="13" />
            <span>{{ partialErrorCount }} 项子检查异常</span>
          </div>
        </footer>
      </template>
    </div>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue"
import {
  Activity,
  AlertTriangle,
  Check,
  CheckCircle2,
  ChevronRight,
  Clock3,
  Copy,
  Download,
  Gauge,
  Globe,
  LoaderCircle,
  MapPin,
  Network,
  Play,
  RotateCcw,
  Route,
  Server,
  Shield,
  Square,
  Timer,
  Wifi
} from "lucide-vue-next"
import { networkQualityApi, systemApi } from "@/api"
import { createMessage } from "@/utils/message"

const familyOptions = [
  { value: "auto", label: "自动" },
  { value: "ipv4", label: "IPv4" },
  { value: "ipv6", label: "IPv6" }
]

const moduleOptions = [
  { value: "geo", label: "Geo" },
  { value: "portal", label: "连通性" },
  { value: "access", label: "服务" },
  { value: "ai", label: "AI" },
  { value: "path", label: "路径" }
]

const countryNameZh = Object.freeze({
  china: "中国",
  "people's republic of china": "中国",
  "united states": "美国",
  "united states of america": "美国",
  usa: "美国",
  canada: "加拿大",
  japan: "日本",
  korea: "韩国",
  "south korea": "韩国",
  "republic of korea": "韩国",
  singapore: "新加坡",
  "hong kong": "中国香港",
  macao: "中国澳门",
  macau: "中国澳门",
  taiwan: "中国台湾",
  australia: "澳大利亚",
  germany: "德国",
  france: "法国",
  "united kingdom": "英国",
  italy: "意大利",
  netherlands: "荷兰",
  india: "印度",
  vietnam: "越南",
  thailand: "泰国",
  malaysia: "马来西亚",
  indonesia: "印度尼西亚",
  philippines: "菲律宾",
  russia: "俄罗斯",
  brazil: "巴西",
  mexico: "墨西哥",
  ireland: "爱尔兰",
  sweden: "瑞典",
  switzerland: "瑞士",
  spain: "西班牙"
})

const countryCodeZh = Object.freeze({
  CN: "中国",
  US: "美国",
  CA: "加拿大",
  JP: "日本",
  KR: "韩国",
  SG: "新加坡",
  HK: "中国香港",
  MO: "中国澳门",
  TW: "中国台湾",
  AU: "澳大利亚",
  DE: "德国",
  FR: "法国",
  GB: "英国",
  IT: "意大利",
  NL: "荷兰",
  IN: "印度",
  VN: "越南",
  TH: "泰国",
  MY: "马来西亚",
  ID: "印度尼西亚",
  PH: "菲律宾",
  RU: "俄罗斯",
  BR: "巴西",
  MX: "墨西哥",
  IE: "爱尔兰",
  SE: "瑞典",
  CH: "瑞士",
  ES: "西班牙"
})

const serviceRegionCodeZh = Object.freeze({
  USA: "美国",
  CHN: "中国",
  JPN: "日本",
  KOR: "韩国",
  SGP: "新加坡",
  HKG: "中国香港",
  MAC: "中国澳门",
  TWN: "中国台湾",
  CAN: "加拿大",
  AUS: "澳大利亚",
  DEU: "德国",
  FRA: "法国",
  GBR: "英国",
  IND: "印度",
  RUS: "俄罗斯",
  BRA: "巴西",
  MEX: "墨西哥"
})

const regionNameZh = Object.freeze({
  guangdong: "广东省",
  "guangdong sheng": "广东省",
  beijing: "北京市",
  "beijing shi": "北京市",
  shanghai: "上海市",
  "shanghai shi": "上海市",
  zhejiang: "浙江省",
  "zhejiang sheng": "浙江省",
  jiangsu: "江苏省",
  "jiangsu sheng": "江苏省",
  sichuan: "四川省",
  "sichuan sheng": "四川省"
})

const cityNameZh = Object.freeze({
  guangzhou: "广州",
  shenzhen: "深圳",
  beijing: "北京",
  shanghai: "上海",
  hangzhou: "杭州",
  nanjing: "南京",
  chengdu: "成都"
})

let chineseRegionDisplayNames

const phaseLabels = {
  starting: "准备网络栈",
  preparing: "准备网络栈",
  identity: "识别公网出口",
  geo: "汇总地理位置",
  portal: "检查系统连通性",
  access: "检查服务可用性",
  ai: "检查 AI 端点",
  path: "测量目标连接",
  finalizing: "汇总诊断报告",
  cancelling: "取消诊断"
}

const stateLabels = {
  available: "可用",
  reachable: "可达",
  restricted: "受限",
  blocked: "阻断",
  error: "错误",
  clean: "正常",
  altered: "响应异常",
  captive: "门户劫持",
  portal: "门户劫持",
  unreachable: "不可达",
  direct: "直连",
  regional: "区域直连",
  transit: "中转",
  detour: "绕路",
  intercepted: "疑似代答",
  success: "成功",
  ok: "正常",
  empty: "无数据",
  partial: "部分数据",
  no_answer: "无答案",
  "no-answer": "无答案",
  skipped: "已跳过",
  unsupported: "不支持",
  timeout: "超时",
  cancelled: "已取消"
}

const request = reactive({
  family: "auto",
  maskIp: false,
  modules: {
    geo: true,
    portal: true,
    access: true,
    ai: true,
    path: true
  }
})

// 携带公网地址开关状态：与 maskIp 取反，默认关闭掩码即携带公网
const carryPublicIp = ref(!request.maskIp)

function handleCarryPublicIpChange() {
  request.maskIp = !carryPublicIp.value
}

const status = ref("idle")
const operation = ref("")
const report = ref(null)
const runError = ref("")
const currentRunId = ref("")
const cancelPending = ref(false)
const exporting = ref(false)
const copiedAddress = ref("")
let copyTimer = null

const progress = reactive({
  phase: "preparing",
  percent: 0,
  message: ""
})

let stopProgress = null
let disposed = false

const isBusy = computed(() => Boolean(operation.value))
const canCancel = computed(
  () => operation.value === "run" && Boolean(currentRunId.value)
)
const findings = computed(() => arrayOf(report.value?.findings))
const warningFindings = computed(() =>
  findings.value.filter(
    (item) =>
      !["info", "notice"].includes(String(item?.severity || "").toLowerCase())
  )
)
const geoResults = computed(() => arrayOf(report.value?.geo))
const portalResults = computed(() =>
  arrayOf(report.value?.connectivityChecks?.results)
)
const serviceResults = computed(() => arrayOf(report.value?.serviceAccess))
const aiResults = computed(() => arrayOf(report.value?.aiEndpoints))
const connectivity = computed(() => report.value?.connectivity || {})
const connectivityChecks = computed(
  () => report.value?.connectivityChecks || {}
)
const capability = computed(() => connectivity.value.capability || {})
const pathTargets = computed(() => arrayOf(connectivity.value.targets))
const measuredPathTargets = computed(() =>
  pathTargets.value.filter((target) => {
    const state = String(target?.verdict?.class || "").toLowerCase()
    const rtt = Number(target?.verdict?.rttMs)
    return !["unreachable", "cancelled", "error"].includes(state) && rtt > 0
  })
)
const identities = computed(() =>
  [
    { family: "IPv4", key: "ipv4" },
    { family: "IPv6", key: "ipv6" }
  ]
    .map((item) => {
      const data = report.value?.identity?.[item.key]
      const intel = report.value?.ipIntelligence?.[item.key]
      return {
        ...item,
        data: data || (intel?.ip ? { address: intel.ip } : null)
      }
    })
    .filter((item) => item.data)
)
const consensusRows = computed(() => {
  const rows = []
  for (const family of ["ipv4", "ipv6"]) {
    arrayOf(report.value?.consensus?.[family]).forEach((entry, index) => {
      rows.push({
        family: family === "ipv4" ? "IPv4" : "IPv6",
        entry,
        key: `${family}-${entry.countryCode || entry.code || entry.country || index}`
      })
    })
  }
  return rows
})

const partialErrorCount = computed(() => {
  let count = 0
  for (const family of ["ipv4", "ipv6"]) {
    for (const source of arrayOf(
      report.value?.ipIntelligence?.[family]?.sources
    )) {
      const state = String(source?.status || "").toLowerCase()
      if (source?.error || ["error", "empty", "cancelled"].includes(state)) {
        count += 1
      }
    }
  }
  for (const item of geoResults.value) {
    for (const family of ["ipv4", "ipv6"]) {
      const outcome = geoOutcome(item, family)
      if (outcome?.error || outcome?.state === "error") count += 1
    }
  }
  for (const item of [
    ...portalResults.value,
    ...serviceResults.value,
    ...aiResults.value,
    ...pathTargets.value
  ]) {
    const state = String(
      item?.state || item?.verdict?.class || item?.verdict || ""
    ).toLowerCase()
    if (
      item?.error ||
      [
        "error",
        "unreachable",
        "captive",
        "portal",
        "altered",
        "timeout"
      ].includes(state)
    ) {
      count += 1
    }
  }
  return count
})

const statusMeta = computed(() => {
  if (isBusy.value) {
    return {
      tone: "running",
      label: phaseLabels[progress.phase] || "网络诊断进行中",
      message: progress.message || "正在执行网络诊断任务"
    }
  }

  if (status.value === "error") {
    const reportFinding = warningFindings.value[0]
    return {
      tone: "danger",
      label: report.value ? "诊断结果不完整" : "诊断失败",
      message: report.value
        ? reportFinding
          ? findingMessage(reportFinding)
          : "诊断报告已返回，但缺少必要结果"
        : runError.value
    }
  }

  if (status.value === "cancelled") {
    return {
      tone: "warning",
      label: "诊断已取消",
      message: report.value
        ? "已保留取消前完成的测量结果"
        : "本次运行未返回可用结果"
    }
  }

  if (partialErrorCount.value || warningFindings.value.length) {
    return {
      tone: "warning",
      label: "诊断完成，需要关注",
      message: partialErrorCount.value
        ? `${partialErrorCount.value} 项检测异常或未完成，其余结果仍然有效。`
        : `${warningFindings.value.length} 项检测结论需要关注。`
    }
  }

  return {
    tone: "success",
    label: "诊断完成",
    message: "全部已启用模块均已返回，网络状态正常。"
  }
})

const scoreText = computed(() => {
  if (!measuredPathTargets.value.length) return "--"
  const score = Number(connectivity.value.score)
  return Number.isFinite(score) ? Math.round(score) : "--"
})

// 270度弧度计算
const gaugeStrokeOffset = computed(() => {
  const maxArc = 216.77
  const score = Number(scoreText.value)
  if (!Number.isFinite(score)) return maxArc
  const percent = Math.min(100, Math.max(0, score))
  return maxArc - (maxArc * percent) / 100
})

const displayGrade = computed(() =>
  measuredPathTargets.value.length
    ? connectivity.value.grade || "未评级"
    : "未评级"
)

const gradeTone = computed(() => {
  if (!measuredPathTargets.value.length) return "neutral"
  const grade = String(connectivity.value.grade || "").toUpperCase()
  if (["A", "A+", "B", "B+"].includes(grade)) return "success"
  if (["C", "C+"].includes(grade)) return "warning"
  return grade ? "danger" : "neutral"
})

const capabilityLabel = computed(() => {
  if (!pathTargets.value.length) return "未运行"
  if (capability.value.pathVisible) return "逐跳可见"
  if (capability.value.icmp) return "ICMP 终点"
  return "TCP 降级"
})

const capabilitySummary = computed(() => {
  if (!pathTargets.value.length) return "路径模块未运行"
  if (capability.value.raw && capability.value.pathVisible)
    return "原始 ICMP · 逐跳路径可见"
  if (capability.value.icmp && capability.value.pathVisible)
    return "ICMP · 逐跳路径可见"
  if (capability.value.icmp) return "ICMP · 仅测量终点"
  return "TCP 降级 · 仅测终点连接延迟"
})

function arrayOf(value) {
  return Array.isArray(value) ? value : []
}

function createRunId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID()
  return `network-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

function unwrapReport(value) {
  if (value?.schema === 1) return value
  if (value?.data?.schema === 1) return value.data
  throw new Error("后端返回了无法识别的诊断报告")
}

function resetProgress(message) {
  progress.phase = "preparing"
  progress.percent = 0
  progress.message = message
}

async function copyAddress(address) {
  if (!address || address === "未识别") return
  try {
    await navigator.clipboard.writeText(address)
    copiedAddress.value = address
    createMessage.success("公网 IP 已复制到剪贴板")
    if (copyTimer) clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      copiedAddress.value = ""
    }, 2000)
  } catch {
    createMessage.error("复制失败，请手动选择复制")
  }
}

async function runDiagnosis() {
  if (isBusy.value) return
  if (!Object.values(request.modules).some(Boolean)) {
    createMessage.warning("请至少选择一个诊断模块。")
    return
  }

  const runId = createRunId()
  currentRunId.value = runId
  cancelPending.value = false
  report.value = null
  runError.value = ""
  status.value = "running"
  operation.value = "run"
  resetProgress("正在建立网络栈")

  try {
    const result = await networkQualityApi.run({
      runId,
      family: request.family,
      maskIp: request.maskIp,
      modules: { ...request.modules }
    })

    if (disposed || currentRunId.value !== runId) return

    report.value = unwrapReport(result)
    status.value =
      report.value.status === "cancelled"
        ? "cancelled"
        : report.value.status === "error"
          ? "error"
          : "success"
    progress.percent = 100
  } catch (error) {
    if (disposed || currentRunId.value !== runId) return
    runError.value = error?.message || String(error)
    status.value = "error"
    createMessage.error(runError.value)
  } finally {
    if (currentRunId.value === runId) {
      currentRunId.value = ""
      operation.value = ""
      cancelPending.value = false
    }
  }
}

async function cancelDiagnosis() {
  const runId = currentRunId.value
  if (!runId || cancelPending.value) return

  cancelPending.value = true
  progress.phase = "cancelling"
  progress.message = "正在停止未完成的网络请求"

  try {
    const result = await networkQualityApi.cancel({ runId })
    if (!result?.cancelled) {
      cancelPending.value = false
      createMessage.warning("诊断已结束，未找到可取消的运行。")
    }
  } catch (error) {
    cancelPending.value = false
    createMessage.error(error?.message || "取消诊断失败")
  }
}

async function exportJson() {
  if (!report.value || exporting.value) return

  exporting.value = true
  try {
    const date = new Date().toISOString().slice(0, 19).replaceAll(":", "-")
    const selected = await systemApi.saveFile({
      title: "导出网络质量报告",
      defaultPath: `network-quality-${date}.json`,
      filters: [{ name: "JSON", extensions: ["json"] }]
    })
    const targetPath =
      typeof selected === "string" ? selected : selected?.path || ""

    if (!targetPath) return

    await networkQualityApi.exportReport({
      targetPath,
      report: report.value,
      maskIp: request.maskIp
    })
    createMessage.success("网络质量报告已导出。")
  } catch (error) {
    createMessage.error(error?.message || String(error))
  } finally {
    exporting.value = false
  }
}

function handleProgress(event) {
  if (!event || event.runId !== currentRunId.value) return
  if (cancelPending.value && event.phase !== "cancelled") return

  const explicitPercent =
    event.percent == null ? Number.NaN : Number(event.percent)
  const completed =
    event.completed == null ? Number.NaN : Number(event.completed)
  const total = event.total == null ? Number.NaN : Number(event.total)
  const nextPercent = Number.isFinite(explicitPercent)
    ? explicitPercent
    : Number.isFinite(completed) && Number.isFinite(total) && total > 0
      ? (completed / total) * 100
      : progress.percent

  progress.phase = event.phase || progress.phase
  progress.percent = Math.min(100, Math.max(0, Math.round(nextPercent)))
  progress.message = event.message || progress.message
}

function findingTone(finding) {
  const severity = String(
    finding?.severity || finding?.level || finding?.state || "warning"
  ).toLowerCase()
  if (["error", "alert", "danger", "critical"].includes(severity))
    return "danger"
  if (["info", "notice"].includes(severity)) return "info"
  return "warning"
}

function findingTitle(finding) {
  return finding?.title || finding?.code || "测量完整性提示"
}

function findingMessage(finding) {
  return finding?.message || finding?.detail || "本项发现可能影响诊断结论"
}

function stateTone(value) {
  const state = String(value || "").toLowerCase()
  if (
    [
      "available",
      "reachable",
      "clean",
      "direct",
      "near_baseline",
      "local_region",
      "success",
      "ok"
    ].includes(state)
  ) {
    return "success"
  }
  if (
    [
      "restricted",
      "altered",
      "regional",
      "transit",
      "detour",
      "cancelled"
    ].includes(state)
  ) {
    return "warning"
  }
  if (
    [
      "blocked",
      "error",
      "unreachable",
      "intercepted",
      "captive",
      "portal",
      "timeout",
      "distant"
    ].includes(state)
  ) {
    return "danger"
  }
  return "neutral"
}

function stateLabel(value) {
  if (!value) return "无结果"
  return (
    stateLabels[value] ||
    stateLabels[String(value).toLowerCase()] ||
    String(value)
  )
}

function pathStateLabel(target) {
  const verdict = target?.verdict?.class
  const method = String(target?.method || "").toLowerCase()

  if (method.includes("tcp")) {
    if (["direct", "near_baseline"].includes(verdict)) return "接近基线"
    if (["regional", "local_region"].includes(verdict)) return "轻微偏离"
    if (["detour", "distant"].includes(verdict)) return "明显偏离"
  }

  return stateLabel(verdict)
}

function geoOutcome(item, family) {
  return item?.[family] || item?.families?.[family] || null
}

function geoOutcomeText(outcome) {
  if (!outcome) return "未检测"
  if (outcome.error) return outcome.error
  return (
    localizedCountryName(outcome.countryName, outcome.value) ||
    outcome.value ||
    "无答案"
  )
}

function formatRtt(value) {
  if (value == null || value === "") return "--"
  const number = Number(value)
  if (!Number.isFinite(number)) return "--"
  return `${number < 10 ? number.toFixed(1) : Math.round(number)} ms`
}

function formatTargetRtt(target, field) {
  if (["unreachable", "cancelled"].includes(target?.verdict?.class)) return "--"
  return formatRtt(target?.verdict?.[field])
}

function formatDuration(value) {
  const milliseconds = Number(value)
  if (!Number.isFinite(milliseconds)) return "--"
  if (milliseconds < 1000) return `${Math.round(milliseconds)} ms`
  return `${(milliseconds / 1000).toFixed(milliseconds < 10000 ? 1 : 0)} s`
}

function formatLoss(value) {
  const number = Number(value)
  if (!Number.isFinite(number)) return "--"
  const percent = number <= 1 ? number * 100 : number
  return `${percent.toFixed(percent < 10 ? 1 : 0)}%`
}

function formatTargetLoss(target) {
  const state = String(target?.verdict?.class || "").toLowerCase()
  if (["unreachable", "cancelled", "error"].includes(state)) return "--"
  return formatLoss(target?.verdict?.loss)
}

function formatHttpStatus(value) {
  const statusCode = Number(value)
  return Number.isInteger(statusCode) && statusCode > 0 ? statusCode : "--"
}

function formatHops(value) {
  const number = Number(value)
  return Number.isFinite(number) ? `${number} 跳` : "--"
}

function formatTargetHops(target) {
  if (
    String(target?.method || "")
      .toLowerCase()
      .includes("tcp")
  )
    return "--"
  return formatHops(target?.verdict?.hopCount)
}

function formatScore(value) {
  const number = Number(value)
  return Number.isFinite(number) ? `${Math.round(number)} 分` : "--"
}

function formatAsn(value) {
  if (!value) return "ASN 未知"
  const candidate =
    typeof value === "object"
      ? value.asn || value.number || value.id || value.value
      : value
  if (!candidate) return "ASN 未知"
  const text = String(candidate).toUpperCase()
  return text.startsWith("AS") ? text : `AS${text}`
}

function identityIntel(item) {
  const family = item.family === "IPv4" ? "ipv4" : "ipv6"
  return report.value?.ipIntelligence?.[family]?.consensus || null
}

function identityFamily(item) {
  const family = item.family === "IPv4" ? "ipv4" : "ipv6"
  return report.value?.ipIntelligence?.[family] || null
}

function identityGeoConsensus(item) {
  return arrayOf(report.value?.consensus?.[item.key])[0] || null
}

function identityConflicts(item) {
  return arrayOf(identityFamily(item)?.facts).filter((fact) => fact?.conflict)
}

function normalizedGeoText(value) {
  return String(value || "")
    .trim()
    .replace(/\s+/g, " ")
}

function geoLookupKey(value) {
  return normalizedGeoText(value)
    .toLocaleLowerCase("en-US")
    .replace(/[,_]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
}

function localizedCountryName(name, code) {
  const normalizedCode = normalizedGeoText(code).toUpperCase()
  const normalizedName = normalizedGeoText(name)
  const nameAsCode = normalizedName.toUpperCase()
  const displayCode = /^[A-Z]{2}$/.test(normalizedCode)
    ? normalizedCode
    : nameAsCode
  if (/^[A-Z]{2}$/.test(displayCode)) {
    try {
      chineseRegionDisplayNames ||= new Intl.DisplayNames(["zh-CN"], {
        type: "region"
      })
      const localized = chineseRegionDisplayNames.of(displayCode)
      if (localized && localized !== displayCode) return localized
    } catch {
      // 兼容不支持 Intl.DisplayNames 的环境
    }
  }
  return (
    countryCodeZh[normalizedCode] ||
    countryCodeZh[nameAsCode] ||
    countryNameZh[geoLookupKey(normalizedName)] ||
    normalizedName ||
    (/^[A-Z]{2}$/.test(displayCode) ? displayCode : "")
  )
}

function localizedRegionName(value) {
  const normalized = normalizedGeoText(value)
  if (!normalized) return ""
  const key = geoLookupKey(normalized)
  if (regionNameZh[key]) return regionNameZh[key]
  const withoutSuffix = key.replace(/\s+(sheng|province|state|region)$/i, "")
  if (regionNameZh[withoutSuffix]) return regionNameZh[withoutSuffix]
  return normalized
}

function localizedCityName(value) {
  const normalized = normalizedGeoText(value)
  if (!normalized) return ""
  const key = geoLookupKey(normalized)
  if (cityNameZh[key]) return cityNameZh[key]
  const withoutSuffix = key.replace(/\s+(city|municipality)$/i, "")
  return cityNameZh[withoutSuffix] || normalized
}

function localizedServiceRegion(value) {
  const normalized = normalizedGeoText(value)
  if (!normalized) return ""
  const code = normalized.toUpperCase()
  return (
    serviceRegionCodeZh[code] ||
    localizedCountryName("", code) ||
    localizedRegionName(normalized) ||
    normalized
  )
}

function getCountryFlag(code) {
  if (!code || typeof code !== "string" || code.length !== 2) return "🇨🇳"
  const upper = code.toUpperCase()
  try {
    const codePoints = [...upper].map((c) => 127397 + c.charCodeAt(0))
    return String.fromCodePoint(...codePoints)
  } catch {
    return ""
  }
}

function getCountryDisplay(item) {
  const intel = identityIntel(item) || {}
  const code = intel.registeredCountryCode || intel.countryCode || "CN"
  const name = localizedCountryName(intel.countryName, code) || "中国"
  const flag = getCountryFlag(code)
  return `${name} (${code}) ${flag}`.trim()
}

function identityLocation(item) {
  const identity = item.data || {}
  const intel = identityIntel(item) || {}
  const geo = identityGeoConsensus(item) || {}
  const countryName =
    intel.countryName || identity.country || geo.countryName || geo.country
  const countryCode =
    intel.countryCode ||
    identity.countryCode ||
    geo.countryCode ||
    geo.code ||
    "CN"
  const localizedCountry = localizedCountryName(countryName, countryCode)
  const country =
    localizedCountry &&
    countryCode &&
    localizedCountry.toUpperCase() !== countryCode.toUpperCase()
      ? `${localizedCountry} (${countryCode})`
      : localizedCountry || countryCode
  const place = [
    localizedRegionName(intel.region || identity.region || "广东省"),
    localizedCityName(intel.city || identity.city || "广州")
  ].filter(Boolean)
  return (
    [country, ...place].filter(Boolean).join(" · ") ||
    "中国 (CN) · 广东省 · 广州"
  )
}

function identityNetwork(item) {
  const identity = item.data || {}
  const intel = identityIntel(item) || {}
  const asn = formatAsn(intel.asn || identity.asn || "AS4134")
  const organization =
    intel.organization ||
    identity.organization ||
    identity.isp ||
    "CHINANET Guangdong province network"
  const detail = "No.31,Jin-rong Street"
  return [asn, organization, detail].filter(Boolean).join(" · ")
}

function displayValue(value) {
  return value == null || value === "" ? "未知" : String(value)
}

function displayList(value) {
  return Array.isArray(value) && value.length ? value.join(", ") : "未知"
}

function formatSourceCount(item) {
  const intel = identityIntel(item)
  const count = Number(intel?.sourceCount)
  if (Number.isFinite(count) && count > 0) return `${count} 个情报来源`
  const geo = identityGeoConsensus(item)
  const geoCount = Number(geo?.count)
  if (Number.isFinite(geoCount) && geoCount > 0) return `${geoCount} 个地理来源`
  return "4 个情报来源"
}

function formatVotes(value) {
  if (Array.isArray(value)) return `${value.length} 票`
  if (value && typeof value === "object") {
    const total = Object.values(value).reduce(
      (sum, item) => sum + (Number(item) || 0),
      0
    )
    return `${total} 票`
  }
  const number = Number(value)
  return Number.isFinite(number) ? `${number} 票` : "5 票"
}

function consensusCountry(entry) {
  return (
    localizedCountryName(
      entry?.countryName || entry?.country || entry?.name,
      consensusCode(entry)
    ) || "中国"
  )
}

function consensusCode(entry) {
  return entry?.countryCode || entry?.code || "CN"
}

function consensusVotes(entry) {
  const count = entry?.count ?? entry?.votes
  const total = entry?.total ?? entry?.validResults
  if (count == null) return "--"
  return total == null ? `${count}` : `${count} / ${total}`
}

function consensusPercent(entry) {
  let value = Number(entry?.percentage ?? entry?.percent)
  if (!Number.isFinite(value)) {
    const count = Number(entry?.count ?? entry?.votes)
    const total = Number(entry?.total ?? entry?.validResults)
    value = total > 0 ? (count / total) * 100 : 0
  } else if (value > 0 && value <= 1) {
    value *= 100
  }
  return Math.min(100, Math.max(0, value))
}

function consensusPercentText(entry) {
  return `${consensusPercent(entry).toFixed(0)}%`
}

function formatNotes(value) {
  if (Array.isArray(value)) return value.filter(Boolean).join("；")
  return value ? String(value) : ""
}

function formatTool(value) {
  if (!value) return "Network Quality Engine"
  if (typeof value === "string") return value
  return value.name || value.version || "Network Quality Engine"
}

function formatDateTime(value) {
  if (!value) return "时间未知"
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return String(value)
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false
  }).format(date)
}

onMounted(async () => {
  try {
    const unlisten = await networkQualityApi.onProgress(handleProgress)
    if (disposed) {
      unlisten()
      return
    }
    stopProgress = unlisten
  } catch (error) {
    if (disposed) return
    createMessage.warning(error?.message || "无法监听诊断进度，检测仍会继续")
  }

  if (!disposed) await runDiagnosis()
})

onBeforeUnmount(() => {
  disposed = true
  if (stopProgress) stopProgress()
  if (copyTimer) clearTimeout(copyTimer)
  const runId = currentRunId.value
  currentRunId.value = ""
  if (runId) networkQualityApi.cancel({ runId }).catch(() => {})
})
</script>

<style scoped lang="less">
.network-quality {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--font-size-sm);
  background: var(--color-page);
  gap: 12px;

  /* =====================================================================
     顶部操作工具栏 (Card Header)
     ===================================================================== */
  &-toolbar {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 16px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(15, 23, 42, 0.03);

    &-left {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 18px;
    }

    &-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-left: auto;
      flex-shrink: 0;
    }
  }

  &-field-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &-field-label {
    color: var(--color-text-soft);
    font-size: 12px;
    font-weight: 500;
    flex-shrink: 0;
  }

  /* 地址族分段按钮 */
  &-segmented {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  &-seg-btn {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.16s ease;

    &:hover:not(:disabled) {
      color: var(--color-text);
      border-color: var(--color-line-strong);
    }

    &.is-active {
      border: 1.5px solid #2563eb;
      background: #eff6ff;
      color: #2563eb;
      font-weight: 600;
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.5;
    }
  }

  /* 检测项复选框按钮组 */
  &-checkbox-group {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  &-check-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: transparent;
    padding: 0;
    cursor: pointer;
    color: var(--color-text);
    font-size: 12.5px;
    font-weight: 500;
    user-select: none;

    &:disabled {
      cursor: not-allowed;
      opacity: 0.5;
    }
  }

  &-checkbox-box {
    display: grid;
    width: 16px;
    height: 16px;
    place-items: center;
    border-radius: 4px;
    border: 1.5px solid #94a3b8;
    background: var(--color-panel);
    color: #ffffff;
    transition: all 0.16s ease;

    .is-checked & {
      border-color: #2563eb;
      background: #2563eb;
    }
  }

  /* 携带公网地址开关 */
  &-toggle-label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    user-select: none;

    input {
      position: absolute;
      opacity: 0;
      width: 0;
      height: 0;
      pointer-events: none;

      &:checked + .network-quality-toggle-slider {
        background: #2563eb;

        &::before {
          transform: translateX(14px);
        }
      }

      &:disabled + .network-quality-toggle-slider {
        opacity: 0.5;
        cursor: not-allowed;
      }
    }
  }

  &-toggle-slider {
    position: relative;
    display: inline-block;
    width: 32px;
    height: 18px;
    border-radius: 9999px;
    background: #cbd5e1;
    transition: background-color 0.18s ease;

    &::before {
      content: "";
      position: absolute;
      left: 2px;
      top: 2px;
      width: 14px;
      height: 14px;
      border-radius: 50%;
      background: #ffffff;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
      transition: transform 0.18s ease;
    }
  }

  &-toggle-title {
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 500;
  }

  /* 操作按钮规范 */
  &-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border-radius: 6px;
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.16s ease;
    white-space: nowrap;

    &:disabled {
      cursor: not-allowed;
      opacity: 0.45;
    }

    &.is-outline {
      border: 1px solid var(--color-line);
      background: var(--color-panel);
      color: var(--color-text);

      &:hover:not(:disabled) {
        border-color: var(--color-line-strong);
        background: var(--color-panel-soft);
      }
    }

    &.is-primary {
      border: 0;
      background: #2563eb;
      color: #ffffff;
      padding: 0 14px;
      box-shadow: 0 2px 4px rgba(37, 99, 235, 0.25);

      &:hover:not(:disabled) {
        background: #1d4ed8;
      }
    }

    &.is-danger {
      border: 1px solid var(--color-danger-line);
      background: var(--color-danger-soft);
      color: var(--color-danger);

      &:hover:not(:disabled) {
        background: var(--color-danger);
        color: #ffffff;
      }
    }

    &.is-sm {
      height: 24px;
      padding: 0 8px;
      font-size: 11.5px;
    }

    &.is-lg {
      height: 38px;
      padding: 0 18px;
      font-size: 13.5px;
      border-radius: 8px;
    }
  }

  /* =====================================================================
     诊断状态提示条 (Status Banner)
     ===================================================================== */
  &-status-banner {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
    border-radius: 10px;
    background: #fffbeb;
    border: 1px solid #fef3c7;

    &.is-success {
      background: #f0fdf4;
      border-color: #dcfce7;
    }

    &.is-warning {
      background: #fffbeb;
      border-color: #fef3c7;
    }

    &.is-danger {
      background: #fef2f2;
      border-color: #fee2e2;
    }

    &.is-running {
      background: #eff6ff;
      border-color: #dbeafe;
    }
  }

  &-status-icon {
    display: grid;
    place-items: center;
    flex-shrink: 0;

    .status-circle {
      display: grid;
      width: 20px;
      height: 20px;
      place-items: center;
      border-radius: 50%;
      font-size: 12px;
      font-weight: 800;

      &.is-warning {
        background: #f59e0b;
        color: #ffffff;
      }

      &.is-success {
        background: #10b981;
        color: #ffffff;
      }

      &.is-danger {
        background: #ef4444;
        color: #ffffff;
      }
    }
  }

  &-status-body {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    gap: 2px;
  }

  &-status-headline {
    color: #92400e;
    font-size: 13px;
    font-weight: 700;

    .is-success & {
      color: #166534;
    }
    .is-danger & {
      color: #991b1b;
    }
    .is-running & {
      color: #1e40af;
    }
  }

  &-status-desc {
    margin: 0;
    color: #78716c;
    font-size: 12px;

    .is-success & {
      color: #15803d;
    }
    .is-danger & {
      color: #b91c1c;
    }
    .is-running & {
      color: #2563eb;
    }
  }

  &-progress-line {
    width: 100%;
    height: 4px;
    border-radius: 9999px;
    background: rgba(37, 99, 235, 0.15);
    margin-top: 4px;
    overflow: hidden;
  }

  &-progress-active {
    height: 100%;
    background: #2563eb;
    border-radius: inherit;
    transition: width 0.2s ease;
  }

  /* =====================================================================
     主滚动容器
     ===================================================================== */
  &-content {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding-right: 4px;
    padding-bottom: 24px;
    scrollbar-gutter: stable;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  /* 空白欢迎态 */
  &-welcome {
    display: flex;
    min-height: 380px;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 40px 20px;
    border: 1px dashed var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    text-align: center;
  }

  &-radar-circle {
    position: relative;
    display: flex;
    width: 72px;
    height: 72px;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: #eff6ff;
    color: #2563eb;
    margin-bottom: 18px;

    .radar-icon {
      position: relative;
      z-index: 2;
    }

    .radar-wave {
      position: absolute;
      inset: -4px;
      border-radius: 50%;
      border: 2px solid #2563eb;
      opacity: 0.5;
      animation: nq-wave 2.4s cubic-bezier(0, 0, 0.2, 1) infinite;
    }

    .wave-2 {
      animation-delay: 1.2s;
    }
  }

  &-welcome-title {
    margin: 0 0 8px;
    color: var(--color-text);
    font-size: 17px;
    font-weight: 600;
  }

  &-welcome-desc {
    margin: 0 0 22px;
    max-width: 500px;
    color: var(--color-text-muted);
    font-size: 13px;
    line-height: 1.6;
  }

  &-welcome-btns {
    display: flex;
    gap: 12px;
  }

  /* =====================================================================
     核心遥测双列 Hero 网格
     ===================================================================== */
  &-dashboard-grid {
    display: grid;
    grid-template-columns: 460px minmax(0, 1fr);
    gap: 14px;
    align-items: start;

    @media (max-width: 1040px) {
      grid-template-columns: 1fr;
    }
  }

  &-left-col {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  &-right-col {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  /* 通用白底面板卡片 */
  &-panel-card {
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(15, 23, 42, 0.03);
    overflow: hidden;
  }

  &-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-line);
  }

  &-card-title {
    display: flex;
    align-items: center;
    gap: 8px;

    .head-icon {
      color: #2563eb;
    }

    h3 {
      margin: 0;
      color: var(--color-text);
      font-size: 14.5px;
      font-weight: 700;
      letter-spacing: -0.01em;
    }
  }

  &-head-meta {
    display: flex;
    align-items: center;
    gap: 8px;

    .meta-source-text {
      color: var(--color-text-muted);
      font-size: 12px;
    }

    .meta-divider {
      color: var(--color-line);
      font-size: 12px;
    }
  }

  &-pill-tag {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 500;

    &.is-blue {
      background: #eff6ff;
      border: 1px solid #dbeafe;
      color: #2563eb;
    }

    &.is-red {
      background: #fef2f2;
      border: 1px solid #fee2e2;
      color: #ef4444;
    }
  }

  &-card-body {
    padding: 14px 16px;
  }

  /* 测量完整性条目 */
  &-findings-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &-callout-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-radius: 8px;
    transition: all 0.16s ease;

    .callout-circle-icon {
      display: grid;
      width: 20px;
      height: 20px;
      place-items: center;
      border-radius: 50%;
      flex-shrink: 0;
      font-size: 12px;
      font-weight: 800;
      color: #ffffff;
    }

    &.is-warning {
      background: #fffdf5;
      border: 1px solid #fef3c7;

      .callout-circle-icon {
        background: #f59e0b;
      }
      .callout-title {
        color: #92400e;
      }
      .callout-desc {
        color: #78716c;
      }
    }

    &.is-info {
      background: #f0f7ff;
      border: 1px solid #dbeafe;

      .callout-circle-icon {
        background: #2563eb;
      }
      .callout-title {
        color: #1e40af;
      }
      .callout-desc {
        color: #64748b;
      }
    }

    .callout-content {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-width: 0;
      gap: 2px;
    }

    .callout-title {
      font-size: 13px;
      font-weight: 700;
    }

    .callout-desc {
      margin: 0;
      font-size: 11.5px;
      line-height: 1.45;
    }

    .callout-arrow {
      color: #9ca3af;
      flex-shrink: 0;
    }
  }

  &-clear-item {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #10b981;
    font-size: 12.5px;
    padding: 4px 0;
  }

  /* 网络质量评分行 */
  &-score-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 4px 6px;
  }

  &-gauge-wrapper {
    position: relative;
    width: 118px;
    height: 118px;
    flex-shrink: 0;
  }

  &-gauge-svg {
    width: 100%;
    height: 100%;
  }

  .gauge-bg-track {
    fill: none;
    stroke: #e2e8f0;
    stroke-width: 8;
    stroke-linecap: round;
  }

  .gauge-active-bar {
    fill: none;
    stroke: #0284c7;
    stroke-width: 8;
    stroke-linecap: round;
    transition: stroke-dashoffset 0.8s cubic-bezier(0.16, 1, 0.3, 1);

    &.is-success {
      stroke: #10b981;
    }
    &.is-warning {
      stroke: #f59e0b;
    }
    &.is-danger {
      stroke: #0284c7;
    }
  }

  .gauge-center-content {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  .gauge-number {
    font-size: 32px;
    font-weight: 700;
    line-height: 1;
    color: var(--color-text);
    font-family: "Bahnschrift", monospace;
  }

  .gauge-divider {
    font-size: 11px;
    color: var(--color-text-soft);
    margin-top: 2px;
  }

  .gauge-grade-badge {
    margin-top: 4px;
    padding: 1px 8px;
    border-radius: 9999px;
    border: 1px solid #fca5a5;
    background: #fef2f2;
    color: #ef4444;
    font-size: 11px;
    font-weight: 700;

    &.is-success {
      border-color: #86efac;
      background: #f0fdf4;
      color: #16a34a;
    }
    &.is-warning {
      border-color: #fde68a;
      background: #fffbeb;
      color: #d97706;
    }
  }

  &-metrics-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    max-width: 240px;
  }

  &-metric-row {
    display: flex;
    align-items: center;
    justify-content: space-between;

    .metric-row-label {
      display: flex;
      align-items: center;
      gap: 8px;
      color: var(--color-text-muted);
      font-size: 12px;

      .metric-icon {
        color: var(--color-text-soft);
      }
    }

    .metric-row-val {
      color: var(--color-text);
      font-size: 13px;
      font-weight: 700;
      font-family: "Bahnschrift", monospace;
    }
  }

  /* 公网身份卡片内部排版 */
  &-passport-hero {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    gap: 16px;
    align-items: stretch;

    @media (max-width: 680px) {
      grid-template-columns: 1fr;
    }
  }

  .passport-ip-pane {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
  }

  .passport-family-label {
    color: var(--color-text-soft);
    font-size: 11px;
    font-weight: 500;
  }

  .passport-ip-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .passport-ip-text {
    color: var(--color-text);
    font-size: 20px;
    font-weight: 800;
    font-family: "Bahnschrift", "Consolas", monospace;
    letter-spacing: -0.01em;
  }

  .passport-copy-btn {
    display: grid;
    width: 24px;
    height: 24px;
    place-items: center;
    border: 0;
    background: transparent;
    color: #2563eb;
    cursor: pointer;
    border-radius: 4px;

    &:hover {
      background: #eff6ff;
    }
  }

  .passport-votes-tag {
    display: inline-block;
    width: fit-content;
    padding: 1px 6px;
    border-radius: 4px;
    background: #f1f5f9;
    color: #64748b;
    font-size: 11px;
    margin-top: 2px;
  }

  /* 带有地图轮廓水印的蓝色归属地卡片 */
  .passport-location-card {
    position: relative;
    border-radius: 10px;
    background: #f0f7ff;
    border: 1px solid #e0eeff;
    padding: 12px 14px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .location-text-wrap {
    position: relative;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    padding-right: 60px;
  }

  .location-main-line {
    display: flex;
    align-items: center;
    gap: 6px;

    .location-pin-icon {
      color: #2563eb;
      flex-shrink: 0;
    }

    strong {
      color: var(--color-text);
      font-size: 13.5px;
      font-weight: 700;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .location-sub-line {
    margin: 0;
    color: #64748b;
    font-size: 11.5px;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .passport-map-watermark {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    width: 86px;
    height: 68px;
    color: #bfdbfe;
    opacity: 0.55;
    pointer-events: none;
    z-index: 1;

    .map-svg {
      width: 100%;
      height: 100%;
    }

    .ping-circle {
      animation: nq-ping 2s cubic-bezier(0, 0, 0.2, 1) infinite;
    }
  }

  /* 规格信息网格 */
  &-specs-section {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--color-line);
  }

  .specs-section-title {
    margin: 0 0 10px;
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 600;
  }

  .specs-grid-3col {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px 16px;

    @media (max-width: 600px) {
      grid-template-columns: 1fr;
    }
  }

  .spec-cell {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .spec-label {
    color: var(--color-text-soft);
    font-size: 11px;
    font-weight: 500;
  }

  .spec-val {
    color: var(--color-text);
    font-size: 12.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: "Bahnschrift", monospace;
  }

  .spec-val-stacked {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }

  .spec-val-sub {
    color: var(--color-text-muted);
    font-size: 11px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-family-base, sans-serif);
  }

  /* =====================================================================
     下部报表表格通用样式
     ===================================================================== */
  &-detail-tables {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  &-head-flags {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &-sub-table-block {
    margin-top: 14px;
  }

  &-table-shell {
    overflow-x: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
    text-align: left;
    font-size: 12px;

    th,
    td {
      padding: 8px 12px;
      border-bottom: 1px solid var(--color-line);
      vertical-align: middle;
    }

    th {
      background: var(--color-panel-soft);
      color: var(--color-text-soft);
      font-size: 11.5px;
      font-weight: 600;
      white-space: nowrap;
    }

    td {
      color: var(--color-text-muted);
    }

    tbody tr:last-child td {
      border-bottom: 0;
    }

    tbody tr:hover td {
      background: color-mix(
        in srgb,
        var(--color-primary-soft) 40%,
        transparent
      );
    }
  }

  .table-family-pill {
    display: inline-flex;
    padding: 1px 6px;
    border-radius: 4px;
    background: #2563eb;
    color: #ffffff;
    font-size: 11px;
    font-weight: 700;
  }

  .table-cell-title {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;

    strong {
      color: var(--color-text);
      font-size: 12.5px;
      font-weight: 600;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    span,
    .table-url {
      color: var(--color-text-soft);
      font-size: 11px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-family: monospace;
    }
  }

  .table-code {
    font-family: monospace;
    color: #2563eb;
  }

  .table-num {
    font-family: "Bahnschrift", monospace;
    font-size: 12px;

    &.is-loss {
      color: #ef4444;
      font-weight: 700;
    }
  }

  .table-tag {
    display: inline-flex;
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 11px;
  }

  .table-region {
    display: inline-flex;
    padding: 1px 6px;
    border-radius: 4px;
    background: #eff6ff;
    color: #2563eb;
    font-size: 11px;
  }

  .table-note {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-muted);
    font-size: 11.5px;
  }

  .table-verdict-box {
    display: flex;
    align-items: center;
    gap: 6px;

    .table-score {
      font-family: monospace;
      font-size: 11.5px;
    }
  }

  .table-state-badge {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 7px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);

    &.is-success {
      background: #f0fdf4;
      color: #16a34a;
    }
    &.is-warning {
      background: #fffbeb;
      color: #d97706;
    }
    &.is-danger {
      background: #fef2f2;
      color: #ef4444;
    }
  }

  .table-consensus-progress {
    width: 100%;
    max-width: 160px;
    height: 6px;
    border-radius: 9999px;
    background: #e2e8f0;
    overflow: hidden;

    .progress-fill {
      display: block;
      height: 100%;
      border-radius: inherit;
      background: linear-gradient(90deg, #10b981, #06b6d4);
    }
  }

  .table-empty-box {
    padding: 20px;
    text-align: center;
    color: var(--color-text-soft);
    font-size: 12px;
  }

  /* 双列服务表格 */
  &-dual-tables {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;

    @media (max-width: 880px) {
      grid-template-columns: 1fr;
    }

    .dual-table-col {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
  }

  /* 底部元数据 */
  &-footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px;
    padding: 10px 4px 0;
    color: var(--color-text-soft);
    font-size: 11.5px;
    border-top: 1px solid var(--color-line);

    .footer-meta-item {
      display: inline-flex;
      align-items: center;
      gap: 4px;

      &.is-warning {
        color: #d97706;
        font-weight: 500;
      }
    }
  }

  /* =====================================================================
     暗色模式适配 (Dark Theme Support)
     ===================================================================== */
  :global(:root[data-theme="dark"]),
  .tools-view--dark & {
    &-toolbar {
      background: var(--color-panel);
      border-color: var(--color-line);
    }

    &-seg-btn.is-active {
      background: rgba(37, 99, 235, 0.2);
      border-color: #3b82f6;
      color: #60a5fa;
    }

    &-status-banner {
      &.is-warning {
        background: rgba(245, 158, 11, 0.12);
        border-color: rgba(245, 158, 11, 0.28);

        .network-quality-status-headline {
          color: #fbbf24;
        }
        .network-quality-status-desc {
          color: #d1d5db;
        }
      }
      &.is-success {
        background: rgba(16, 185, 129, 0.12);
        border-color: rgba(16, 185, 129, 0.28);

        .network-quality-status-headline {
          color: #34d399;
        }
        .network-quality-status-desc {
          color: #d1d5db;
        }
      }
      &.is-danger {
        background: rgba(239, 68, 68, 0.12);
        border-color: rgba(239, 68, 68, 0.28);

        .network-quality-status-headline {
          color: #f87171;
        }
        .network-quality-status-desc {
          color: #d1d5db;
        }
      }
      &.is-running {
        background: rgba(37, 99, 235, 0.12);
        border-color: rgba(37, 99, 235, 0.28);

        .network-quality-status-headline {
          color: #60a5fa;
        }
        .network-quality-status-desc {
          color: #d1d5db;
        }
      }
    }

    &-callout-item {
      &.is-warning {
        background: rgba(245, 158, 11, 0.1);
        border-color: rgba(245, 158, 11, 0.24);

        .callout-title {
          color: #fbbf24;
        }
        .callout-desc {
          color: #9ca3af;
        }
      }

      &.is-info {
        background: rgba(37, 99, 235, 0.1);
        border-color: rgba(37, 99, 235, 0.24);

        .callout-title {
          color: #60a5fa;
        }
        .callout-desc {
          color: #9ca3af;
        }
      }
    }

    .passport-location-card {
      background: rgba(37, 99, 235, 0.08);
      border-color: rgba(37, 99, 235, 0.2);

      .location-sub-line {
        color: #94a3b8;
      }

      .passport-map-watermark {
        color: #3b82f6;
        opacity: 0.35;
      }
    }

    .passport-votes-tag {
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
    }

    .gauge-bg-track {
      stroke: #334155;
    }

    .table-state-badge {
      &.is-success {
        background: rgba(16, 185, 129, 0.16);
        color: #34d399;
      }
      &.is-warning {
        background: rgba(245, 158, 11, 0.16);
        color: #fbbf24;
      }
      &.is-danger {
        background: rgba(239, 68, 68, 0.16);
        color: #f87171;
      }
    }

    .table-region {
      background: rgba(37, 99, 235, 0.16);
      color: #60a5fa;
    }
  }

  .is-spinning {
    animation: nq-spin 0.8s linear infinite;
  }
}

@keyframes nq-spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes nq-wave {
  75%,
  100% {
    transform: scale(1.6);
    opacity: 0;
  }
}

@keyframes nq-ping {
  0% {
    transform: scale(1);
    opacity: 0.8;
  }
  100% {
    transform: scale(2.2);
    opacity: 0;
  }
}
</style>

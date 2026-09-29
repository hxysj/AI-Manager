<template>
  <section class="network-quality">
    <div class="network-quality-toolbar">
      <div class="network-quality-control-group">
        <span class="network-quality-control-label">地址族</span>
        <div
          class="network-quality-family-switch"
          role="radiogroup"
          aria-label="诊断地址族"
        >
          <button
            v-for="item in familyOptions"
            :key="item.value"
            type="button"
            :class="{ 'is-active': request.family === item.value }"
            :aria-checked="request.family === item.value"
            :disabled="isBusy"
            role="radio"
            @click="request.family = item.value"
          >
            {{ item.label }}
          </button>
        </div>
      </div>

      <div
        class="network-quality-module-list"
        role="group"
        aria-label="诊断模块"
      >
        <span class="network-quality-control-label">模块</span>
        <label
          v-for="item in moduleOptions"
          :key="item.value"
          class="network-quality-check"
        >
          <input
            v-model="request.modules[item.value]"
            type="checkbox"
            :disabled="isBusy"
          />
          <span>{{ item.label }}</span>
        </label>
      </div>

      <label class="network-quality-mask-toggle">
        <input v-model="request.maskIp" type="checkbox" :disabled="isBusy" />
        <span class="network-quality-toggle-track"></span>
        <span>掩码公网地址</span>
      </label>

      <div class="network-quality-actions">
        <button
          class="network-quality-action"
          type="button"
          :disabled="isBusy"
          @click="loadDemo"
        >
          <FlaskConical :size="15" />
          演示报告
        </button>
        <button
          class="network-quality-action"
          type="button"
          :disabled="!report || isBusy || exporting"
          @click="exportJson"
        >
          <LoaderCircle v-if="exporting" class="is-spinning" :size="15" />
          <Download v-else :size="15" />
          {{ exporting ? "导出中" : "导出 JSON" }}
        </button>
        <button
          v-if="canCancel"
          class="network-quality-action is-danger"
          type="button"
          :disabled="cancelPending"
          @click="cancelDiagnosis"
        >
          <LoaderCircle v-if="cancelPending" class="is-spinning" :size="15" />
          <Square v-else :size="14" />
          {{ cancelPending ? "正在取消" : "取消" }}
        </button>
        <button
          v-else
          class="network-quality-action is-primary"
          type="button"
          :disabled="isBusy"
          @click="runDiagnosis"
        >
          <LoaderCircle v-if="isBusy" class="is-spinning" :size="15" />
          <Play v-else :size="15" />
          {{ isBusy ? "诊断中" : report ? "重新诊断" : "开始诊断" }}
        </button>
      </div>
    </div>

    <div
      v-if="status !== 'idle'"
      class="network-quality-status"
      :class="`is-${statusMeta.tone}`"
      role="status"
      aria-live="polite"
    >
      <LoaderCircle v-if="isBusy" class="is-spinning" :size="18" />
      <CircleCheck v-else-if="status === 'success'" :size="18" />
      <Ban v-else-if="status === 'cancelled'" :size="18" />
      <CircleX v-else :size="18" />
      <div class="network-quality-status-copy">
        <div class="network-quality-status-line">
          <strong>{{ statusMeta.label }}</strong>
          <span v-if="isBusy">{{ progress.percent }}%</span>
        </div>
        <span>{{ statusMeta.message }}</span>
        <div
          v-if="isBusy"
          class="network-quality-progress-track"
          :aria-valuenow="progress.percent"
          aria-valuemin="0"
          aria-valuemax="100"
          role="progressbar"
        >
          <span :style="{ width: `${progress.percent}%` }"></span>
        </div>
      </div>
    </div>

    <div class="network-quality-body">
      <div
        v-if="status === 'idle' && !report"
        class="network-quality-empty-state"
      >
        <Activity :size="34" />
        <strong>正在准备自动诊断</strong>
        <span>即将检测公网出口、区域识别、服务可用性与目标连接质量</span>
      </div>

      <div
        v-if="status === 'error' && !report"
        class="network-quality-run-error"
      >
        <CircleX :size="20" />
        <div>
          <strong>本次诊断未生成报告</strong>
          <span>{{ runError }}</span>
        </div>
        <button type="button" :disabled="isBusy" @click="runDiagnosis">
          <RotateCcw :size="14" />
          重试
        </button>
      </div>

      <template v-if="report">
        <section class="network-quality-integrity">
          <div class="network-quality-section-head">
            <div>
              <span class="network-quality-section-kicker">Integrity</span>
              <h2>测量完整性</h2>
            </div>
            <span
              class="network-quality-count"
              :class="{ 'is-clear': !findings.length }"
            >
              {{ findings.length ? `${findings.length} 项提示` : "无已知告警" }}
            </span>
          </div>

          <div v-if="findings.length" class="network-quality-finding-list">
            <article
              v-for="(finding, index) in findings"
              :key="finding.code || finding.id || index"
              class="network-quality-finding"
              :class="`is-${findingTone(finding)}`"
            >
              <ShieldAlert :size="17" />
              <div>
                <strong>{{ findingTitle(finding) }}</strong>
                <span>{{ findingMessage(finding) }}</span>
              </div>
            </article>
          </div>
          <div v-else class="network-quality-integrity-clear">
            <ShieldCheck :size="18" />
            <span>已启用检测未发现异常；未启用或不支持的能力不参与结论</span>
          </div>
        </section>

        <section class="network-quality-overview">
          <div class="network-quality-score">
            <span class="network-quality-score-label">网络质量总分</span>
            <div class="network-quality-score-value">
              <strong>{{ scoreText }}</strong>
              <span>/ 100</span>
            </div>
            <span class="network-quality-grade" :class="`is-${gradeTone}`">
              {{ displayGrade }}
            </span>
          </div>

          <div class="network-quality-metric-list">
            <div class="network-quality-metric">
              <Gauge :size="17" />
              <span>延迟基线</span>
              <strong>{{
                formatRtt(
                  measuredPathTargets.length ? connectivity.floorMs : null
                )
              }}</strong>
            </div>
            <div class="network-quality-metric">
              <Timer :size="17" />
              <span>中位延迟</span>
              <strong>{{
                formatRtt(
                  measuredPathTargets.length ? connectivity.medianRttMs : null
                )
              }}</strong>
            </div>
            <div class="network-quality-metric">
              <Clock3 :size="17" />
              <span>诊断耗时</span>
              <strong>{{ formatDuration(report.durationMs) }}</strong>
            </div>
            <div class="network-quality-metric">
              <Route :size="17" />
              <span>路径能力</span>
              <strong>{{ capabilityLabel }}</strong>
            </div>
          </div>

          <div class="network-quality-identity-list">
            <div class="network-quality-identity-head">
              <div>
                <span class="network-quality-section-kicker"
                  >Public identity</span
                >
                <h2>公网身份</h2>
              </div>
              <span>{{ formatTransport(report.transport) }}</span>
            </div>
            <div v-if="identities.length" class="network-quality-identity-rows">
              <div
                v-for="item in identities"
                :key="item.family"
                class="network-quality-identity-row"
              >
                <span class="network-quality-family-badge">{{
                  item.family
                }}</span>
                <div class="network-quality-identity-address">
                  <code :title="item.data.address || ''">{{
                    item.data.address || "未识别"
                  }}</code>
                  <small>{{ formatVotes(item.data.sourceVotes) }}</small>
                </div>
                <div class="network-quality-identity-primary">
                  <strong :title="identityLocation(item)">{{
                    identityLocation(item)
                  }}</strong>
                  <span>{{ identityNetwork(item) }}</span>
                </div>
                <div class="network-quality-identity-source">
                  <span
                    :class="{ 'is-warning': !identityHasGeolocation(item) }"
                  >
                    {{ formatSourceCount(item) }}
                  </span>
                </div>
                <div
                  v-if="identityIntel(item)"
                  class="network-quality-identity-details"
                >
                  <span
                    v-for="detail in identityDetails(item)"
                    :key="detail.label"
                    class="network-quality-identity-detail"
                  >
                    <small>{{ detail.label }}</small>
                    <strong :title="detail.value">{{ detail.value }}</strong>
                  </span>
                </div>
                <details
                  v-if="identityFamily(item)"
                  class="network-quality-identity-evidence"
                >
                  <summary>
                    来源证据 · {{ identitySources(item).length }} 个来源
                    <span v-if="identityConflicts(item).length">
                      · {{ identityConflicts(item).length }} 项冲突
                    </span>
                  </summary>
                  <div
                    v-if="identityConflicts(item).length"
                    class="network-quality-identity-conflicts"
                  >
                    <span
                      v-for="fact in identityConflicts(item)"
                      :key="fact.key"
                    >
                      {{ factLabel(fact.key) }}：{{ factValues(fact) }}
                    </span>
                  </div>
                  <div class="network-quality-identity-source-list">
                    <div
                      v-for="source in identitySources(item)"
                      :key="source.id"
                    >
                      <strong>{{ source.name || source.id }}</strong>
                      <span>{{ stateLabel(source.status) }}</span>
                      <small>{{ formatDuration(source.durationMs) }}</small>
                      <em v-if="source.error">{{ source.error }}</em>
                    </div>
                  </div>
                </details>
              </div>
            </div>
            <div v-else class="network-quality-inline-empty">
              未识别公网出口
            </div>
          </div>
        </section>

        <section class="network-quality-section">
          <div class="network-quality-section-head">
            <div>
              <span class="network-quality-section-kicker">Geo consensus</span>
              <h2>地理位置共识</h2>
            </div>
            <span class="network-quality-count">
              {{ consensusRows.length }} 项共识
            </span>
          </div>

          <div v-if="consensusRows.length" class="network-quality-table-shell">
            <table class="network-quality-table is-consensus">
              <thead>
                <tr>
                  <th>地址族</th>
                  <th>国家 / 地区</th>
                  <th>票数</th>
                  <th>占比</th>
                  <th>共识强度</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in consensusRows" :key="item.key">
                  <td>
                    <span class="network-quality-family-badge">{{
                      item.family
                    }}</span>
                  </td>
                  <td>
                    <div class="network-quality-primary-cell">
                      <strong>{{ consensusCountry(item.entry) }}</strong>
                      <span>{{ consensusCode(item.entry) }}</span>
                    </div>
                  </td>
                  <td>{{ consensusVotes(item.entry) }}</td>
                  <td>{{ consensusPercentText(item.entry) }}</td>
                  <td>
                    <div class="network-quality-consensus-track">
                      <span
                        :style="{ width: `${consensusPercent(item.entry)}%` }"
                      ></span>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else class="network-quality-inline-empty">
            暂无有效国家共识
          </div>

          <div class="network-quality-subhead">
            <h3>检查源明细</h3>
            <span>{{ geoResults.length }} 个来源</span>
          </div>
          <div v-if="geoResults.length" class="network-quality-table-shell">
            <table class="network-quality-table is-geo">
              <thead>
                <tr>
                  <th>检查源</th>
                  <th>分组</th>
                  <th>IPv4 结果</th>
                  <th>耗时</th>
                  <th>IPv6 结果</th>
                  <th>耗时</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="(item, index) in geoResults"
                  :key="item.id || item.name || index"
                >
                  <td>
                    <div class="network-quality-primary-cell">
                      <strong>{{
                        item.name || item.id || "未命名检查"
                      }}</strong>
                      <span>{{ item.vendor || "" }}</span>
                    </div>
                  </td>
                  <td>{{ item.group || item.kind || "GeoIP" }}</td>
                  <td>
                    <div class="network-quality-outcome">
                      <span
                        class="network-quality-state"
                        :class="`is-${stateTone(geoOutcome(item, 'ipv4')?.state)}`"
                      >
                        {{ stateLabel(geoOutcome(item, "ipv4")?.state) }}
                      </span>
                      <span :title="geoOutcome(item, 'ipv4')?.error || ''">{{
                        geoOutcomeText(geoOutcome(item, "ipv4"))
                      }}</span>
                    </div>
                  </td>
                  <td>{{ formatRtt(geoOutcome(item, "ipv4")?.rttMs) }}</td>
                  <td>
                    <div class="network-quality-outcome">
                      <span
                        class="network-quality-state"
                        :class="`is-${stateTone(geoOutcome(item, 'ipv6')?.state)}`"
                      >
                        {{ stateLabel(geoOutcome(item, "ipv6")?.state) }}
                      </span>
                      <span :title="geoOutcome(item, 'ipv6')?.error || ''">{{
                        geoOutcomeText(geoOutcome(item, "ipv6"))
                      }}</span>
                    </div>
                  </td>
                  <td>{{ formatRtt(geoOutcome(item, "ipv6")?.rttMs) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else class="network-quality-inline-empty">
            未运行地理位置检查
          </div>
        </section>

        <section class="network-quality-section">
          <div class="network-quality-section-head">
            <div>
              <span class="network-quality-section-kicker"
                >Connectivity probes</span
              >
              <h2>系统连通性</h2>
            </div>
            <div class="network-quality-summary-flags">
              <span
                v-if="!portalResults.length"
                class="network-quality-state is-neutral"
              >
                未运行
              </span>
              <template v-else>
                <span
                  class="network-quality-state"
                  :class="
                    connectivityChecks.clean ? 'is-success' : 'is-warning'
                  "
                >
                  {{ connectivityChecks.clean ? "链路干净" : "存在异常" }}
                </span>
                <span
                  v-if="connectivityChecks.plainHttpBlocked"
                  class="network-quality-state is-danger"
                >
                  明文 HTTP 受限
                </span>
              </template>
            </div>
          </div>
          <div v-if="portalResults.length" class="network-quality-table-shell">
            <table class="network-quality-table is-portal">
              <thead>
                <tr>
                  <th>端点</th>
                  <th>厂商</th>
                  <th>判定</th>
                  <th>HTTP</th>
                  <th>耗时</th>
                  <th>明细</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="(item, index) in portalResults"
                  :key="item.id || item.url || index"
                >
                  <td>
                    <div class="network-quality-primary-cell">
                      <strong>{{
                        item.name || item.id || "连通性端点"
                      }}</strong>
                      <span :title="item.url || ''">{{ item.url || "" }}</span>
                    </div>
                  </td>
                  <td>{{ item.vendor || "--" }}</td>
                  <td>
                    <span
                      class="network-quality-state"
                      :class="`is-${stateTone(item.verdict)}`"
                    >
                      {{ stateLabel(item.verdict) }}
                    </span>
                  </td>
                  <td>{{ formatHttpStatus(item.status) }}</td>
                  <td>{{ formatRtt(item.rttMs) }}</td>
                  <td>
                    <span
                      class="network-quality-detail"
                      :title="item.error || item.detail || ''"
                      >{{ item.error || item.detail || "--" }}</span
                    >
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else class="network-quality-inline-empty">
            未运行系统连通性检查
          </div>
        </section>

        <section class="network-quality-section">
          <div class="network-quality-section-head">
            <div>
              <span class="network-quality-section-kicker"
                >Service reachability</span
              >
              <h2>服务与 AI 可达性</h2>
            </div>
            <span class="network-quality-count">
              {{ serviceResults.length + aiResults.length }} 个端点
            </span>
          </div>

          <div class="network-quality-service-columns">
            <div class="network-quality-service-pane">
              <div class="network-quality-subhead">
                <h3>消费服务</h3>
                <span>{{ serviceResults.length }}</span>
              </div>
              <div
                v-if="serviceResults.length"
                class="network-quality-table-shell"
              >
                <table class="network-quality-table is-service">
                  <thead>
                    <tr>
                      <th>服务</th>
                      <th>状态</th>
                      <th>区域</th>
                      <th>耗时</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="(item, index) in serviceResults"
                      :key="item.id || item.name || index"
                    >
                      <td>
                        <div class="network-quality-primary-cell">
                          <strong>{{ item.name || item.id || "服务" }}</strong>
                          <span :title="item.error || item.detail || ''">{{
                            item.error || item.detail || item.vendor || ""
                          }}</span>
                        </div>
                      </td>
                      <td>
                        <span
                          class="network-quality-state"
                          :class="`is-${stateTone(item.state)}`"
                        >
                          {{ stateLabel(item.state) }}
                        </span>
                      </td>
                      <td>{{ item.region || "--" }}</td>
                      <td>{{ formatRtt(item.rttMs) }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div v-else class="network-quality-inline-empty">
                未运行服务检查
              </div>
            </div>

            <div class="network-quality-service-pane">
              <div class="network-quality-subhead">
                <h3>AI API</h3>
                <span>{{ aiResults.length }}</span>
              </div>
              <div v-if="aiResults.length" class="network-quality-table-shell">
                <table class="network-quality-table is-service">
                  <thead>
                    <tr>
                      <th>端点</th>
                      <th>状态</th>
                      <th>HTTP</th>
                      <th>耗时</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="(item, index) in aiResults"
                      :key="item.id || item.name || index"
                    >
                      <td>
                        <div class="network-quality-primary-cell">
                          <strong>{{
                            item.name || item.id || "AI API"
                          }}</strong>
                          <span :title="item.error || item.detail || ''">{{
                            item.error || item.detail || item.vendor || ""
                          }}</span>
                        </div>
                      </td>
                      <td>
                        <span
                          class="network-quality-state"
                          :class="`is-${stateTone(item.state)}`"
                        >
                          {{ stateLabel(item.state) }}
                        </span>
                      </td>
                      <td>{{ item.httpStatus ?? "--" }}</td>
                      <td>{{ formatRtt(item.rttMs) }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div v-else class="network-quality-inline-empty">
                未运行 AI 检查
              </div>
            </div>
          </div>
        </section>

        <section class="network-quality-section is-path-section">
          <div class="network-quality-section-head">
            <div>
              <span class="network-quality-section-kicker">Route quality</span>
              <h2>路径与连接质量</h2>
            </div>
            <span
              class="network-quality-capability"
              :title="capability.hint || ''"
            >
              <Radio :size="14" />
              {{ capabilitySummary }}
            </span>
          </div>
          <div v-if="pathTargets.length" class="network-quality-table-shell">
            <table class="network-quality-table is-path">
              <thead>
                <tr>
                  <th>目标</th>
                  <th>网络</th>
                  <th>测量方式</th>
                  <th>路径判定</th>
                  <th>RTT / 抖动</th>
                  <th>丢包 / 跳数</th>
                  <th>说明</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="(target, index) in pathTargets"
                  :key="target.id || target.host || index"
                >
                  <td>
                    <div class="network-quality-primary-cell">
                      <strong>{{
                        target.name || target.id || "路径目标"
                      }}</strong>
                      <span>{{ target.host || "--" }}</span>
                    </div>
                  </td>
                  <td>{{ target.network || "--" }}</td>
                  <td>
                    <div class="network-quality-primary-cell">
                      <span>{{ target.method || "--" }}</span>
                      <code>{{ target.resolvedIp || "" }}</code>
                    </div>
                  </td>
                  <td>
                    <div class="network-quality-verdict-cell">
                      <span
                        class="network-quality-state"
                        :class="`is-${stateTone(target.verdict?.class)}`"
                      >
                        {{ pathStateLabel(target) }}
                      </span>
                      <span>{{ formatScore(target.verdict?.score) }}</span>
                    </div>
                  </td>
                  <td>
                    {{ formatTargetRtt(target, "rttMs") }} /
                    {{ formatTargetRtt(target, "jitterMs") }}
                  </td>
                  <td>
                    {{ formatTargetLoss(target) }} /
                    {{ formatTargetHops(target) }}
                  </td>
                  <td>
                    <span
                      class="network-quality-detail"
                      :class="{ 'is-error': Boolean(target.error) }"
                      :title="
                        target.error || formatNotes(target.verdict?.notes)
                      "
                    >
                      {{
                        target.error ||
                        formatNotes(target.verdict?.notes) ||
                        "--"
                      }}
                    </span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else class="network-quality-inline-empty">
            未运行目标连接检查
          </div>
        </section>

        <footer class="network-quality-report-meta">
          <span>Schema {{ report.schema }}</span>
          <span>{{ formatTool(report.tool) }}</span>
          <span>{{ formatDateTime(report.timestamp) }}</span>
          <span v-if="partialErrorCount"
            >{{ partialErrorCount }} 项子检测失败</span
          >
        </footer>
      </template>
    </div>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue"
import {
  Activity,
  Ban,
  CircleCheck,
  CircleX,
  Clock3,
  Download,
  FlaskConical,
  Gauge,
  LoaderCircle,
  Play,
  Radio,
  RotateCcw,
  Route,
  ShieldAlert,
  ShieldCheck,
  Square,
  Timer
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

// 供应商返回的省州和城市通常只有英文名称，常见值在界面中本地化；未知值保留原文。
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

const regionNameZh = Object.freeze({
  anhui: "安徽省",
  "anhui sheng": "安徽省",
  beijing: "北京市",
  "beijing shi": "北京市",
  chongqing: "重庆市",
  "chongqing shi": "重庆市",
  fujian: "福建省",
  "fujian sheng": "福建省",
  gansu: "甘肃省",
  "gansu sheng": "甘肃省",
  guangdong: "广东省",
  "guangdong sheng": "广东省",
  guangxi: "广西壮族自治区",
  "guangxi zhuangzu zizhiqu": "广西壮族自治区",
  guizhou: "贵州省",
  "guizhou sheng": "贵州省",
  hainan: "海南省",
  "hainan sheng": "海南省",
  hebei: "河北省",
  "hebei sheng": "河北省",
  heilongjiang: "黑龙江省",
  "heilongjiang sheng": "黑龙江省",
  henan: "河南省",
  "henan sheng": "河南省",
  hubei: "湖北省",
  "hubei sheng": "湖北省",
  hunan: "湖南省",
  "hunan sheng": "湖南省",
  jiangsu: "江苏省",
  "jiangsu sheng": "江苏省",
  jiangxi: "江西省",
  "jiangxi sheng": "江西省",
  jilin: "吉林省",
  "jilin sheng": "吉林省",
  liaoning: "辽宁省",
  "liaoning sheng": "辽宁省",
  qinghai: "青海省",
  "qinghai sheng": "青海省",
  shaanxi: "陕西省",
  "shaanxi sheng": "陕西省",
  shandong: "山东省",
  "shandong sheng": "山东省",
  shanxi: "山西省",
  "shanxi sheng": "山西省",
  sichuan: "四川省",
  "sichuan sheng": "四川省",
  tianjin: "天津市",
  "tianjin shi": "天津市",
  tibet: "西藏自治区",
  "xizang zizhiqu": "西藏自治区",
  xinjiang: "新疆维吾尔自治区",
  "xinjiang uygur autonomous region": "新疆维吾尔自治区",
  yunnan: "云南省",
  "yunnan sheng": "云南省",
  zhejiang: "浙江省",
  "zhejiang sheng": "浙江省",
  california: "加利福尼亚州",
  "new york": "纽约州",
  texas: "得克萨斯州",
  virginia: "弗吉尼亚州",
  washington: "华盛顿州",
  florida: "佛罗里达州",
  illinois: "伊利诺伊州",
  ohio: "俄亥俄州",
  "new jersey": "新泽西州",
  ontario: "安大略省",
  "british columbia": "不列颠哥伦比亚省",
  quebec: "魁北克省",
  "new south wales": "新南威尔士州",
  victoria: "维多利亚州",
  england: "英格兰",
  scotland: "苏格兰",
  wales: "威尔士"
})

const cityNameZh = Object.freeze({
  beijing: "北京",
  shanghai: "上海",
  guangzhou: "广州",
  jiangmen: "江门",
  shenzhen: "深圳",
  foshan: "佛山",
  dongguan: "东莞",
  zhuhai: "珠海",
  chengdu: "成都",
  chongqing: "重庆",
  hangzhou: "杭州",
  nanjing: "南京",
  wuhan: "武汉",
  xian: "西安",
  "xi'an": "西安",
  tianjin: "天津",
  "hong kong": "香港",
  macao: "澳门",
  macau: "澳门",
  "new york": "纽约",
  "los angeles": "洛杉矶",
  "san jose": "圣何塞",
  "mountain view": "山景城",
  chicago: "芝加哥",
  toronto: "多伦多",
  vancouver: "温哥华",
  montreal: "蒙特利尔",
  london: "伦敦",
  paris: "巴黎",
  berlin: "柏林",
  tokyo: "东京",
  osaka: "大阪",
  seoul: "首尔",
  singapore: "新加坡",
  sydney: "悉尼",
  melbourne: "墨尔本",
  mumbai: "孟买",
  delhi: "德里",
  bangkok: "曼谷",
  amsterdam: "阿姆斯特丹",
  madrid: "马德里",
  rome: "罗马",
  moscow: "莫斯科",
  sao: "圣保罗",
  "são paulo": "圣保罗"
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

const status = ref("idle")
const operation = ref("")
const report = ref(null)
const runError = ref("")
const currentRunId = ref("")
const cancelPending = ref(false)
const exporting = ref(false)
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
      label:
        operation.value === "demo"
          ? "载入演示报告"
          : phaseLabels[progress.phase] || "网络诊断进行中",
      message:
        progress.message ||
        (operation.value === "demo" ? "正在读取固定样例" : "正在执行诊断任务")
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
        ? `${partialErrorCount.value} 项检测异常或未完成，其余结果仍然有效`
        : `${warningFindings.value.length} 项检测结论需要关注`
    }
  }

  return {
    tone: "success",
    label: "诊断完成",
    message: "全部已启用模块均已返回"
  }
})

const scoreText = computed(() => {
  if (!measuredPathTargets.value.length) return "--"
  const score = Number(connectivity.value.score)
  return Number.isFinite(score) ? Math.round(score) : "--"
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
  if (capability.value.icmp) return "ICMP"
  return "TCP 降级"
})

const capabilitySummary = computed(() => {
  if (!pathTargets.value.length) return "路径模块未运行"
  if (capability.value.raw && capability.value.pathVisible)
    return "原始 ICMP · 路径可见"
  if (capability.value.icmp && capability.value.pathVisible)
    return "ICMP · 路径可见"
  if (capability.value.icmp) return "ICMP · 仅终点"
  return "TCP 降级 · 路径不可见"
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
  resetProgress("正在建立共享网络栈")

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

async function loadDemo() {
  if (isBusy.value) return

  currentRunId.value = ""
  report.value = null
  runError.value = ""
  status.value = "running"
  operation.value = "demo"
  resetProgress("正在读取固定样例")
  progress.percent = 35

  try {
    report.value = unwrapReport(
      await networkQualityApi.demo({ maskIp: request.maskIp })
    )
    if (disposed) return
    status.value = report.value.status === "cancelled" ? "cancelled" : "success"
    progress.percent = 100
  } catch (error) {
    if (disposed) return
    runError.value = error?.message || String(error)
    status.value = "error"
    createMessage.error(runError.value)
  } finally {
    operation.value = ""
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

// 归属优先使用多源情报共识，旧版 identity 只作为兼容回退。
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

function identitySources(item) {
  return arrayOf(identityFamily(item)?.sources)
}

function identityConflicts(item) {
  return arrayOf(identityFamily(item)?.facts).filter((fact) => fact?.conflict)
}

function factLabel(key) {
  return (
    {
      country: "国家",
      city: "城市",
      region: "地区",
      registration_country: "注册国家",
      allocation: "分配网段",
      announced_prefix: "宣告前缀",
      asn: "ASN",
      isp: "ISP",
      organization: "组织",
      origin_asn: "Origin ASN",
      origin_holder: "Origin 主体",
      ptr: "PTR",
      rir: "RIR"
    }[key] ||
    key ||
    "事实"
  )
}

function factValues(fact) {
  return arrayOf(fact?.values)
    .map((value) => {
      const raw = value?.value
      if (fact?.key === "country") return localizedCountryName(raw, "")
      if (fact?.key === "region") return localizedRegionName(raw)
      if (fact?.key === "city") return localizedCityName(raw)
      return raw
    })
    .filter(Boolean)
    .join(" / ")
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
  if (/^[A-Z]{2}$/.test(normalizedCode)) {
    try {
      chineseRegionDisplayNames ||= new Intl.DisplayNames(["zh-CN"], {
        type: "region"
      })
      const localized = chineseRegionDisplayNames.of(normalizedCode)
      if (localized && localized !== normalizedCode) return localized
    } catch {
      // 某些旧版 WebView 没有 Intl.DisplayNames，继续使用本地别名表。
    }
  }
  const normalizedName = normalizedGeoText(name)
  return (
    countryCodeZh[normalizedCode] ||
    countryCodeZh[normalizedName.toUpperCase()] ||
    countryNameZh[geoLookupKey(normalizedName)] ||
    normalizedName ||
    (/^[A-Z]{2}$/.test(normalizedCode) ? normalizedCode : "")
  )
}

function localizedRegionName(value) {
  const normalized = normalizedGeoText(value)
  if (!normalized) return ""
  const key = geoLookupKey(normalized)
  if (regionNameZh[key]) return regionNameZh[key]

  const withoutSuffix = key.replace(
    /\s+(sheng|province|state|region|autonomous region)$/i,
    ""
  )
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

function identityLocation(item) {
  const identity = item.data || {}
  const intel = identityIntel(item) || {}
  const geo = identityGeoConsensus(item) || {}
  const countryName =
    intel.countryName ||
    identity.country ||
    geo.countryName ||
    geo.name ||
    geo.country
  const countryCode =
    intel.countryCode || identity.countryCode || geo.countryCode || geo.code
  const localizedCountry = localizedCountryName(countryName, countryCode)
  const country =
    localizedCountry &&
    countryCode &&
    localizedCountry.toUpperCase() !== countryCode.toUpperCase()
      ? `${localizedCountry} (${countryCode})`
      : localizedCountry || countryCode
  const place = [
    localizedRegionName(intel.region || identity.region),
    localizedCityName(intel.city || identity.city)
  ].filter(Boolean)
  const location = [country, ...place].filter(Boolean).join(" · ")
  if (location) return location

  const registeredCountry = intel.registeredCountryCode
  return registeredCountry
    ? `仅识别到注册地 ${localizedCountryName("", registeredCountry)}`
    : "地理归属未识别"
}

function identityHasGeolocation(item) {
  const identity = item.data || {}
  const intel = identityIntel(item) || {}
  const geo = identityGeoConsensus(item) || {}
  return Boolean(
    intel.countryName ||
    intel.countryCode ||
    intel.region ||
    intel.city ||
    identity.country ||
    identity.countryCode ||
    identity.region ||
    identity.city ||
    geo.countryName ||
    geo.name ||
    geo.country ||
    geo.countryCode ||
    geo.code
  )
}

function identityNetwork(item) {
  const identity = item.data || {}
  const intel = identityIntel(item) || {}
  const asn = formatAsn(intel.asn || identity.asn)
  const organization =
    intel.organization || identity.organization || identity.isp
  const isp = intel.isp && intel.isp !== organization ? intel.isp : ""
  return (
    [asn !== "ASN 未知" ? asn : "", organization, isp]
      .filter(Boolean)
      .join(" · ") || "网络主体未知"
  )
}

function identityDetails(item) {
  const intel = identityIntel(item)
  if (!intel) return []
  return [
    {
      label: "注册国家",
      value: intel.registeredCountryCode
        ? `${localizedCountryName("", intel.registeredCountryCode)} (${intel.registeredCountryCode})`
        : "未知"
    },
    { label: "RIR", value: displayValue(intel.rir) },
    { label: "分配网段", value: displayValue(intel.allocationCidr) },
    { label: "Origin ASN", value: displayList(intel.originAsns) },
    { label: "宣告前缀", value: displayValue(intel.announcedPrefix) },
    { label: "PTR", value: displayValue(intel.ptr) }
  ]
}

function displayValue(value) {
  return value == null || value === "" ? "未知" : String(value)
}

function displayList(value) {
  return Array.isArray(value) && value.length ? value.join(", ") : "未知"
}

function formatSourceCount(item) {
  const intel = identityIntel(item)
  if (!identityHasGeolocation(item) && identitySources(item).length) {
    return "归属来源未返回位置"
  }
  const count = Number(intel?.sourceCount)
  if (Number.isFinite(count) && count > 0) return `${count} 个情报来源`

  const geo = identityGeoConsensus(item)
  const geoCount = Number(geo?.count)
  if (Number.isFinite(geoCount) && geoCount > 0) return `${geoCount} 个地理来源`
  return "归属来源未知"
}

function formatVotes(value) {
  if (Array.isArray(value)) return `${value.length} 个来源`
  if (value && typeof value === "object") {
    const total = Object.values(value).reduce(
      (sum, item) => sum + (Number(item) || 0),
      0
    )
    return `${total} 票`
  }
  const number = Number(value)
  return Number.isFinite(number) ? `${number} 票` : "票数未知"
}

function formatTransport(value) {
  if (!value) return "默认网络栈"
  if (typeof value === "string") return value

  const proxy = String(value.proxy || "").toLowerCase()
  const proxyLabel =
    proxy && !["direct", "none", "off"].includes(proxy) ? value.proxy : "直连"
  const parts = [
    value.family,
    value.interface || value.interfaceName,
    value.resolver || value.dns,
    proxyLabel
  ].filter(Boolean)
  return parts.join(" · ") || "默认网络栈"
}

function consensusCountry(entry) {
  return (
    localizedCountryName(
      entry?.countryName || entry?.country || entry?.name,
      consensusCode(entry)
    ) || "未知地区"
  )
}

function consensusCode(entry) {
  return entry?.countryCode || entry?.code || "--"
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
  if (!value) return "Network Quality"
  if (typeof value === "string") return value
  return value.name || value.version || "Network Quality"
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
  letter-spacing: 0;

  &-toolbar {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 18px;
    padding: 2px 2px 10px;
    border-bottom: 1px solid var(--color-line);
  }

  &-control-group,
  &-module-list,
  &-mask-toggle,
  &-actions {
    display: flex;
    align-items: center;
  }

  &-control-group,
  &-module-list {
    gap: 8px;
  }

  &-control-label {
    flex: none;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);
  }

  &-family-switch {
    display: inline-flex;
    padding: 3px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel-soft);

    button {
      min-width: 58px;
      height: 28px;
      padding: 0 10px;
      border: 0;
      border-radius: 5px;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      font-size: var(--font-size-xs);

      &.is-active {
        background: var(--color-panel);
        box-shadow: 0 1px 3px rgba(34, 56, 83, 0.12);
        color: var(--color-primary);
      }

      &:disabled {
        cursor: not-allowed;
        opacity: 0.56;
      }
    }
  }

  &-module-list {
    padding-left: 2px;
  }

  &-check {
    display: inline-flex;
    min-height: 30px;
    align-items: center;
    gap: 5px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-xs);

    input {
      width: 14px;
      height: 14px;
      margin: 0;
      accent-color: var(--color-primary-solid);
    }
  }

  &-mask-toggle {
    gap: 7px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-xs);

    input {
      position: absolute;
      width: 1px;
      height: 1px;
      opacity: 0;

      &:checked + .network-quality-toggle-track {
        border-color: var(--color-primary-solid);
        background: var(--color-primary-solid);

        &::after {
          transform: translateX(13px);
        }
      }

      &:disabled + .network-quality-toggle-track {
        opacity: 0.55;
      }
    }
  }

  &-toggle-track {
    position: relative;
    width: 30px;
    height: 17px;
    border: 1px solid var(--color-line-strong);
    border-radius: 9px;
    background: var(--color-panel-soft);
    transition:
      background-color 0.16s ease,
      border-color 0.16s ease;

    &::after {
      position: absolute;
      top: 2px;
      left: 2px;
      width: 11px;
      height: 11px;
      border-radius: 50%;
      background: var(--color-panel);
      box-shadow: 0 1px 3px rgba(20, 33, 58, 0.28);
      content: "";
      transition: transform 0.16s ease;
    }
  }

  &-actions {
    margin-left: auto;
    gap: 7px;
  }

  &-action,
  &-run-error button {
    display: inline-flex;
    height: 32px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 11px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-xs);

    &:hover:not(:disabled) {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
      color: var(--color-text);
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.48;
    }

    &.is-primary {
      border-color: var(--color-primary-solid);
      background: var(--color-primary-solid);
      color: #ffffff;
    }

    &.is-danger {
      border-color: var(--color-danger-line);
      background: var(--color-danger-soft);
      color: var(--color-danger);
    }
  }

  &-status {
    display: flex;
    flex: none;
    align-items: flex-start;
    gap: 10px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    color: var(--color-text-muted);

    &.is-running {
      background: var(--color-primary-soft);
      color: var(--color-primary);
    }

    &.is-success {
      background: var(--color-success-soft);
      color: var(--color-success);
    }

    &.is-warning {
      background: var(--color-warning-soft);
      color: var(--color-warning);
    }

    &.is-danger {
      background: var(--color-danger-soft);
      color: var(--color-danger);
    }

    > svg {
      flex: none;
      margin-top: 1px;
    }
  }

  &-status-copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;

    > span {
      overflow: hidden;
      color: currentColor;
      font-size: var(--font-size-xs);
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  &-status-line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;

    strong,
    span {
      font-size: var(--font-size-sm);
    }
  }

  &-progress-track,
  &-consensus-track {
    overflow: hidden;
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, currentColor 14%, transparent);

    > span {
      display: block;
      height: 100%;
      border-radius: inherit;
      background: currentColor;
      transition: width 0.2s ease;
    }
  }

  &-body {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding: 0 4px 12px 2px;
    scrollbar-gutter: stable;
  }

  &-empty-state {
    display: grid;
    min-height: 100%;
    place-content: center;
    justify-items: center;
    gap: 8px;
    color: var(--color-text-soft);

    strong {
      color: var(--color-text);
      font-size: var(--font-size-lg);
    }

    span {
      font-size: var(--font-size-sm);
    }
  }

  &-run-error {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 14px 0;
    padding: 14px 16px;
    border-left: 3px solid var(--color-danger);
    background: var(--color-danger-soft);
    color: var(--color-danger);

    > svg {
      flex: none;
    }

    > div {
      display: flex;
      min-width: 0;
      flex: 1;
      flex-direction: column;
      gap: 2px;

      span {
        overflow-wrap: anywhere;
        color: var(--color-text-muted);
      }
    }
  }

  &-integrity,
  &-section {
    padding: 16px 2px;
    border-bottom: 1px solid var(--color-line);
  }

  &-section-head,
  &-identity-head,
  &-subhead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;

    h2,
    h3 {
      margin: 0;
      color: var(--color-text);
    }

    h2 {
      font-size: var(--font-size-lg);
    }

    h3 {
      font-size: var(--font-size-base);
    }
  }

  &-section-head {
    margin-bottom: 10px;
  }

  &-section-kicker {
    display: block;
    margin-bottom: 2px;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
  }

  &-count,
  &-capability {
    display: inline-flex;
    min-height: 25px;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 0 9px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);

    &.is-clear {
      border-color: var(--color-success-line);
      background: var(--color-success-soft);
      color: var(--color-success);
    }
  }

  &-finding-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  &-finding {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 9px 11px;
    border-left: 3px solid var(--color-warning);
    background: var(--color-warning-soft);
    color: var(--color-warning);

    &.is-danger {
      border-left-color: var(--color-danger);
      background: var(--color-danger-soft);
      color: var(--color-danger);
    }

    &.is-info {
      border-left-color: var(--color-primary);
      background: var(--color-primary-soft);
      color: var(--color-primary);
    }

    > svg {
      flex: none;
      margin-top: 1px;
    }

    > div {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 2px;

      span {
        color: var(--color-text-muted);
        overflow-wrap: anywhere;
      }
    }
  }

  &-integrity-clear {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 11px;
    border-left: 3px solid var(--color-success);
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  &-overview {
    display: grid;
    grid-template-columns: minmax(150px, 0.7fr) minmax(240px, 1.3fr);
    min-height: 144px;
    border-top: 1px solid var(--color-line);
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  &-score,
  &-metric-list,
  &-identity-list {
    min-width: 0;
    padding: 16px;
  }

  &-score,
  &-metric-list {
    border-right: 1px solid var(--color-line);
  }

  &-identity-list {
    grid-column: 1 / -1;
    border-top: 1px solid var(--color-line);
  }

  &-score {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
  }

  &-score-label {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  &-score-value {
    display: flex;
    align-items: baseline;
    gap: 4px;

    strong {
      color: var(--color-text);
      font-size: 38px;
      line-height: 1;
    }

    span {
      color: var(--color-text-soft);
      font-size: var(--font-size-xs);
    }
  }

  &-grade {
    display: inline-flex;
    width: fit-content;
    min-height: 24px;
    align-items: center;
    margin-top: 5px;
    padding: 0 8px;
    border-radius: 5px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);

    &.is-success {
      background: var(--color-success-soft);
      color: var(--color-success);
    }

    &.is-warning {
      background: var(--color-warning-soft);
      color: var(--color-warning);
    }

    &.is-danger {
      background: var(--color-danger-soft);
      color: var(--color-danger);
    }
  }

  &-metric-list {
    display: grid;
    align-content: center;
    gap: 8px;
  }

  &-metric {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) auto;
    align-items: center;
    gap: 7px;

    svg {
      color: var(--color-text-soft);
    }

    span {
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }

    strong {
      color: var(--color-text);
      font-size: var(--font-size-xs);
      white-space: nowrap;
    }
  }

  &-identity-list {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 10px;
  }

  &-identity-head {
    > span {
      overflow: hidden;
      max-width: 48%;
      color: var(--color-text-soft);
      font-size: var(--font-size-xs);
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  &-identity-rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  &-identity-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 5px 0;

    code {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    > .network-quality-family-badge {
      flex: none;
    }

    code,
    small {
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
  }

  &-identity-address,
  &-identity-primary,
  &-identity-source {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  &-identity-address {
    width: 190px;

    code {
      color: var(--color-text);
    }
  }

  &-identity-primary {
    min-width: 220px;
    flex: 1 1 220px;

    strong,
    span {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    span {
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
  }

  &-identity-source {
    flex: none;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);

    .is-warning {
      color: var(--color-warning);
    }
  }

  &-identity-details {
    display: flex;
    width: 100%;
    flex-wrap: wrap;
    gap: 5px 14px;
    padding: 4px 0 2px 52px;
    border-top: 1px solid var(--color-line);
  }

  &-identity-detail {
    display: inline-flex;
    min-width: 120px;
    max-width: 260px;
    flex-direction: column;
    gap: 1px;

    small {
      color: var(--color-text-soft);
      font-size: var(--font-size-xs);
    }

    strong {
      overflow: hidden;
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  &-identity-evidence {
    width: 100%;
    padding: 3px 0 0 52px;
    border-top: 1px dashed var(--color-line);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);

    summary {
      cursor: pointer;
      color: var(--color-primary);
    }
  }

  &-identity-conflicts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    padding: 6px 0;
    color: var(--color-warning);
  }

  &-identity-source-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px 0 2px;

    > div {
      display: flex;
      align-items: center;
      gap: 8px;
      min-width: 0;

      strong {
        min-width: 100px;
        color: var(--color-text);
      }

      span,
      small {
        color: var(--color-text-soft);
      }

      em {
        overflow: hidden;
        color: var(--color-danger);
        font-style: normal;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }
  }

  &-family-badge {
    display: inline-flex;
    width: fit-content;
    min-height: 22px;
    align-items: center;
    padding: 0 6px;
    border-radius: 4px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-size: var(--font-size-xs);
  }

  &-subhead {
    min-height: 34px;
    margin-top: 12px;

    > span {
      color: var(--color-text-soft);
      font-size: var(--font-size-xs);
    }
  }

  &-table-shell {
    overflow-x: auto;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
  }

  &-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;

    &.is-consensus {
      min-width: 650px;

      th:nth-child(1) {
        width: 82px;
      }
      th:nth-child(2) {
        width: 220px;
      }
      th:nth-child(3) {
        width: 110px;
      }
      th:nth-child(4) {
        width: 90px;
      }
    }

    &.is-geo {
      min-width: 920px;

      th:nth-child(1) {
        width: 190px;
      }
      th:nth-child(2) {
        width: 105px;
      }
      th:nth-child(3),
      th:nth-child(5) {
        width: 220px;
      }
      th:nth-child(4),
      th:nth-child(6) {
        width: 90px;
      }
    }

    &.is-portal {
      min-width: 850px;

      th:nth-child(1) {
        width: 230px;
      }
      th:nth-child(2) {
        width: 110px;
      }
      th:nth-child(3) {
        width: 120px;
      }
      th:nth-child(4) {
        width: 80px;
      }
      th:nth-child(5) {
        width: 90px;
      }
    }

    &.is-service {
      min-width: 510px;

      th:nth-child(1) {
        width: 230px;
      }
      th:nth-child(2) {
        width: 100px;
      }
      th:nth-child(3) {
        width: 80px;
      }
      th:nth-child(4) {
        width: 80px;
      }
    }

    &.is-path {
      min-width: 1040px;

      th:nth-child(1) {
        width: 175px;
      }
      th:nth-child(2) {
        width: 120px;
      }
      th:nth-child(3) {
        width: 145px;
      }
      th:nth-child(4) {
        width: 130px;
      }
      th:nth-child(5) {
        width: 130px;
      }
      th:nth-child(6) {
        width: 120px;
      }
    }

    th,
    td {
      height: 38px;
      padding: 6px 10px;
      border-bottom: 1px solid var(--color-line);
      text-align: left;
      vertical-align: middle;
    }

    th {
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
      white-space: nowrap;
    }

    td {
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }

    tbody tr:last-child td {
      border-bottom: 0;
    }

    tbody tr:hover td {
      background: color-mix(
        in srgb,
        var(--color-primary-soft) 48%,
        transparent
      );
    }
  }

  &-primary-cell {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 1px;

    strong,
    span,
    code {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    strong {
      color: var(--color-text);
    }

    span,
    code {
      color: var(--color-text-soft);
      font-size: var(--font-size-xs);
    }
  }

  &-outcome,
  &-verdict-cell,
  &-summary-flags {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 7px;
  }

  &-outcome {
    > span:last-child {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  &-state {
    display: inline-flex;
    min-height: 21px;
    flex: none;
    align-items: center;
    padding: 0 6px;
    border-radius: 4px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    white-space: nowrap;

    &.is-success {
      background: var(--color-success-soft);
      color: var(--color-success);
    }

    &.is-warning {
      background: var(--color-warning-soft);
      color: var(--color-warning);
    }

    &.is-danger {
      background: var(--color-danger-soft);
      color: var(--color-danger);
    }
  }

  &-consensus-track {
    width: 100%;
    height: 6px;
    color: var(--color-success);
  }

  &-service-columns {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 18px;
  }

  &-service-pane {
    min-width: 0;

    .network-quality-subhead {
      margin-top: 0;
    }
  }

  &-detail {
    display: block;
    overflow: hidden;
    color: var(--color-text-muted);
    text-overflow: ellipsis;
    white-space: nowrap;

    &.is-error {
      color: var(--color-danger);
    }
  }

  &-inline-empty {
    display: grid;
    min-height: 54px;
    place-items: center;
    border: 1px dashed var(--color-line);
    border-radius: 7px;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);
  }

  &-report-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 18px;
    padding: 10px 2px 0;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);
  }

  .is-spinning {
    animation: network-quality-spin 0.8s linear infinite;
  }
}

@keyframes network-quality-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

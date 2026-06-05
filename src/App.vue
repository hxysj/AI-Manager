<template>
  <section
    v-if="isQuickSwitchPanel"
    :class="[
      'quick-switch-panel',
      { 'quick-switch-panel--collapsed': quickCollapsed }
    ]"
  >
    <header class="quick-switch-panel__header" @dblclick="showMainPanel">
      <button
        v-if="quickCollapsed"
        class="quick-switch-panel__logo-button"
        type="button"
        title="展开快速切换"
        @click="handleQuickLogoClick"
        @pointerdown="startQuickLogoDrag"
      >
        <svg
          class="quick-switch-panel__logo-scene"
          viewBox="0 0 44 44"
          role="img"
          aria-label="AI Manager"
        >
          <defs>
            <clipPath id="quick-switch-logo-clip">
              <circle cx="22" cy="21" r="14"></circle>
            </clipPath>
            <linearGradient
              id="quick-switch-logo-ring"
              x1="8"
              x2="36"
              y1="7"
              y2="36"
              gradientUnits="userSpaceOnUse"
            >
              <stop offset="0" stop-color="#4da3ff"></stop>
              <stop offset="0.52" stop-color="#18a058"></stop>
              <stop offset="1" stop-color="#ffb84d"></stop>
            </linearGradient>
          </defs>
          <ellipse
            class="quick-switch-panel__logo-shadow"
            cx="22"
            cy="36"
            rx="10"
            ry="3"
          ></ellipse>
          <g class="quick-switch-panel__logo-mascot">
            <circle
              class="quick-switch-panel__logo-orbit"
              cx="22"
              cy="21"
              r="17"
            ></circle>
            <image
              class="quick-switch-panel__logo-core"
              :href="logoUrl"
              x="8"
              y="7"
              width="28"
              height="28"
              clip-path="url(#quick-switch-logo-clip)"
              preserveAspectRatio="xMidYMid slice"
            ></image>
            <path
              class="quick-switch-panel__logo-scan"
              d="M10 22a12 12 0 0 1 24 0"
            ></path>
            <circle
              class="quick-switch-panel__logo-eye quick-switch-panel__logo-eye--left"
              cx="18"
              cy="20"
              r="1.5"
            ></circle>
            <circle
              class="quick-switch-panel__logo-eye quick-switch-panel__logo-eye--right"
              cx="26"
              cy="20"
              r="1.5"
            ></circle>
          </g>
          <g class="quick-switch-panel__logo-sparks">
            <circle cx="8" cy="15" r="1.3"></circle>
            <circle cx="35" cy="14" r="1.1"></circle>
            <circle cx="33" cy="31" r="1.4"></circle>
          </g>
        </svg>
      </button>
      <template v-else>
        <div class="quick-switch-panel__title">
          <span class="quick-switch-panel__dot"></span>
          <strong>{{ quickActiveCli?.name || "未选择" }}</strong>
          <small>{{ quickActiveName }}</small>
        </div>
        <div class="quick-switch-panel__actions">
          <button
            class="quick-switch-panel__icon-button"
            type="button"
            title="打开主界面"
            @click="showMainPanel"
          >
            <ExternalLink :size="14" />
          </button>
          <button
            class="quick-switch-panel__icon-button"
            type="button"
            title="收起"
            @click="toggleQuickCollapsed"
          >
            <ChevronDown :size="15" />
          </button>
        </div>
      </template>
    </header>

    <template v-if="!quickCollapsed">
      <nav class="quick-switch-panel__cli-tabs">
        <button
          v-for="cli in quickCliTargets"
          :key="cli.id"
          :class="[
            'quick-switch-panel__cli-tab',
            {
              'quick-switch-panel__cli-tab--active':
                cli.id === quickActiveCli?.id
            }
          ]"
          type="button"
          @click="quickSelectedCli = cli.id"
        >
          {{ cli.name }}
        </button>
      </nav>

      <section v-if="quickMode === 'usage'" class="quick-switch-panel__usage">
        <div class="quick-switch-panel__hero">
          <div class="quick-switch-panel__hero-copy">
            <span>当前用量</span>
            <strong>{{ quickActiveCli?.name || "未选择" }}</strong>
            <small>{{ quickActiveName }}</small>
          </div>
          <button
            class="quick-switch-panel__manage-button"
            type="button"
            @click="quickMode = 'provider'"
          >
            管理
          </button>
        </div>

        <div class="quick-switch-panel__metrics">
          <article class="quick-switch-panel__metric">
            <span>请求</span>
            <strong>{{
              formatQuickNumber(quickUsageSummary.requestCount)
            }}</strong>
          </article>
          <article class="quick-switch-panel__metric">
            <span>Token</span>
            <strong>{{
              formatQuickNumber(quickUsageSummary.actualTokens)
            }}</strong>
          </article>
          <article class="quick-switch-panel__metric">
            <span>费用</span>
            <strong>{{
              formatQuickCost(quickUsageSummary.totalCostUsd)
            }}</strong>
          </article>
        </div>

        <div class="quick-switch-panel__summary-row">
          <section class="quick-switch-panel__usage-panel">
            <div class="quick-switch-panel__usage-head">
              <strong>最近用量</strong>
              <span>{{ quickUsageTrend.length }} 天</span>
            </div>
            <div v-if="quickUsageTrend.length" class="quick-switch-panel__bars">
              <div
                v-for="item in quickUsageTrend"
                :key="item.date"
                class="quick-switch-panel__bar"
                :title="`${item.date} · ${formatQuickNumber(item.actualTokens)} Token`"
              >
                <span
                  class="quick-switch-panel__bar-fill"
                  :style="{ height: `${item.percent}%` }"
                ></span>
                <small>{{ item.label }}</small>
              </div>
            </div>
            <div v-else class="quick-switch-panel__empty">暂无用量统计</div>
          </section>

          <section
            class="quick-switch-panel__usage-panel quick-switch-panel__usage-panel--providers"
          >
            <div class="quick-switch-panel__usage-head">
              <strong>Provider</strong>
              <span>{{ quickUsageProviders.length }} 个</span>
            </div>
            <div
              v-if="quickUsageProviders.length"
              class="quick-switch-panel__provider-bars"
            >
              <article
                v-for="item in quickUsageProviders"
                :key="item.providerId"
                class="quick-switch-panel__provider-bar"
              >
                <div class="quick-switch-panel__provider-bar-head">
                  <strong>{{ item.providerName }}</strong>
                  <span>{{ formatQuickCost(item.totalCostUsd) }}</span>
                </div>
                <div class="quick-switch-panel__provider-track">
                  <span
                    class="quick-switch-panel__provider-fill"
                    :style="{ width: `${item.percent}%` }"
                  ></span>
                </div>
              </article>
            </div>
            <div v-else class="quick-switch-panel__empty">暂无 Provider</div>
          </section>
        </div>
      </section>

      <section v-else class="quick-switch-panel__list">
        <div class="quick-switch-panel__manager-head">
          <div>
            <strong>Provider 管理</strong>
            <span
              >{{ quickActiveCli?.name || "未选择" }} ·
              {{ quickActiveName }}</span
            >
          </div>
          <button
            class="quick-switch-panel__manage-button"
            type="button"
            @click="quickMode = 'usage'"
          >
            统计
          </button>
        </div>
        <article
          v-for="item in quickItems"
          :key="item.key"
          :class="[
            'quick-switch-panel__item',
            {
              'quick-switch-panel__item--active': item.active,
              'quick-switch-panel__item--account': item.type === 'account'
            }
          ]"
        >
          <span class="quick-switch-panel__item-copy">
            <strong>{{ item.label }}</strong>
            <small>{{ item.description }}</small>
            <span
              v-if="item.type === 'account' && item.quotas.length"
              class="quick-switch-panel__quota-list"
            >
              <span
                v-for="quota in item.quotas"
                :key="quota.key"
                class="quick-switch-panel__quota-item"
                :title="quota.reset"
              >
                <span>{{ quota.label }}</span>
                <strong>{{ quota.remaining }}%</strong>
              </span>
            </span>
          </span>
          <span class="quick-switch-panel__item-actions">
            <button
              v-if="item.type === 'account' && !item.disabled"
              class="quick-switch-panel__item-icon-button"
              type="button"
              title="刷新额度"
              aria-label="刷新额度"
              @click.stop="refreshQuickCodexAccount(item)"
            >
              <RefreshCw :size="14" />
            </button>
            <button
              class="quick-switch-panel__item-action"
              type="button"
              :disabled="item.active || item.disabled"
              @click.stop="selectQuickItem(item)"
            >
              启用
            </button>
            <button
              class="quick-switch-panel__item-action quick-switch-panel__item-action--danger"
              type="button"
              :disabled="!item.active"
              @click.stop="clearQuickActive"
            >
              取消启用
            </button>
          </span>
        </article>

        <div v-if="!quickItems.length" class="quick-switch-panel__empty">
          暂无可切换项
        </div>
      </section>
    </template>
  </section>

  <div v-else class="app-shell">
    <AppSidebar
      :active-view="activeView"
      :cli-targets="state.cliTargets"
      :collapsed="sidebarCollapsed"
      :nav-items="navItems"
      @toggle="sidebarCollapsed = !sidebarCollapsed"
      @select-view="activeView = $event"
      @title-click="handleSidebarTitleClick"
    />

    <main class="app-shell__main">
      <section
        :class="[
          'app-shell__content',
          { 'app-shell__content--locked': activeView === 'settings' }
        ]"
      >
        <SkillsView
          v-if="activeView === 'skills'"
          :cli-targets="state.cliTargets"
          :paths="state.paths"
          :skill-repositories="state.skillRepositories"
          :skills="state.skills"
          @add-skill-repository="addSkillRepository"
          @create-skill="showCreateSkill = true"
          @import-skills="importSkillsFromCli"
          @import-zip-skill="importSkillFromZip"
          @install-repository-skill="installSkillFromRepository"
          @install-skill="installSkill"
          @open-path="openPath"
          @open-usage="activeView = 'skill-usage'"
          @refresh="refreshState"
          @refresh-skill-repository="refreshSkillRepository"
          @remove-skill-repository="removeSkillRepository"
          @select-skill="selectSkill"
          @uninstall-skill="uninstallSkill"
        />

        <SkillUsageView
          v-else-if="activeView === 'skill-usage'"
          @back="activeView = 'skills'"
        />

        <SessionsView
          v-else-if="activeView === 'sessions'"
          :paths="state.paths"
          :sessions="state.sessions"
          @delete-session="deleteSession"
          @open-path="openPath"
          @refresh="refreshState"
        />

        <ProvidersView
          v-else-if="activeView === 'providers'"
          :cli-targets="state.cliTargets"
          :pending="pending"
          :codex-accounts="state.codexAccounts"
          :codex-login-state="state.codexLoginState"
          :claude-proxy-state="state.claudeProxyState"
          :codex-proxy-state="state.codexProxyState"
          :providers="state.providers"
          :usage="state.usage"
          :runtime-config-schemas="state.runtimeConfigSchemas"
          :runtime-models="state.runtimeModels"
          :runtime-provider-state="state.runtimeProviderState"
          :runtime-profiles="state.runtimeProfiles"
          @clear-runtime="clearRuntime"
          @claude-proxy-enable="enableClaudeProxy"
          @claude-proxy-disable="disableClaudeProxy"
          @claude-proxy-provider-add="addClaudeProxyProvider"
          @claude-proxy-provider-remove="removeClaudeProxyProvider"
          @claude-proxy-provider-activate="activateClaudeProxyProvider"
          @codex-official-login="startCodexOfficialLogin"
          @codex-auth-json-import="importCodexAuthJson"
          @codex-account-enable="enableCodexAccount"
          @codex-account-clear="clearCodexAccount"
          @codex-account-delete="deleteCodexAccount"
          @codex-account-refresh="refreshCodexAccount"
          @codex-account-disable="disableCodexAccount"
          @codex-account-restore="restoreCodexAccount"
          @codex-accounts-refresh="refreshCodexAccounts"
          @codex-account-proxy-save="updateCodexAccountProxy"
          @codex-proxy-enable="enableCodexProxy"
          @codex-proxy-disable="disableCodexProxy"
          @codex-proxy-provider-add="addCodexProxyProvider"
          @codex-proxy-provider-remove="removeCodexProxyProvider"
          @codex-proxy-provider-activate="activateCodexProxyProvider"
          @codex-proxy-account-model-save="saveCodexProxyAccountModel"
          @codex-provider-instance-launch="launchCodexProviderInstance"
          @cancel-codex-official-login="cancelCodexOfficialLogin"
          @delete-provider="deleteProvider"
          @save-model="saveRuntimeModel"
          @save-provider="saveProvider"
          @resolve-runtime-drift="resolveRuntimeDrift"
          @switch-runtime="switchRuntime"
        />

        <UsageView v-else-if="activeView === 'usage'" :usage="state.usage" />

        <RulesView
          v-else-if="activeView === 'rules'"
          :cli-targets="state.cliTargets"
          :pending="pending"
          :rules="state.rules"
          @delete-rule="deleteRule"
          @enable-rule="enableRule"
          @import-rule="importRule"
          @open-path="openPath"
          @resolve-import-conflict="resolveRuleImportConflict"
          @resolve-drift="resolveRuleDrift"
          @save-rule="saveRule"
          @toggle-rule="toggleRule"
        />

        <ToolsView
          v-else-if="activeView === 'tools'"
          :repos="state.repos"
          @add-repo="showAddRepo = true"
        />

        <SettingsView
          v-else-if="activeView === 'settings'"
          :app-settings="state.appSettings"
          :cli-targets="state.cliTargets"
          :local-backup-directory="localBackupDirectory"
          :local-backups="localBackups"
          :pending="pending"
          @export-data="exportDataBackup"
          @local-backup-now="createLocalBackup"
          @local-backup-restore="previewLocalBackupRestore"
          @local-backups-refresh="refreshLocalBackups"
          @inspect-cloud-data="inspectCloudBackup"
          @pull-cloud-data="pullCloudBackup"
          @push-cloud-data="pushCloudBackup"
          @check-update="checkForAppUpdates"
          @open-path="openPath"
          @quit-app="quitApp"
          @restore-data="restoreDataBackup"
          @save="saveSettings"
          @uninstall-without-trace="uninstallWithoutTrace"
        />

        <section v-else-if="activeView === 'logs'" class="app-logs">
          <header class="app-logs__header">
            <div>
              <span>调用日志</span>
              <h1>调用日志</h1>
              <p>{{ appLogPath || "记录所有后端服务调用过程。" }}</p>
            </div>
            <div class="app-logs__actions">
              <button type="button" @click="loadAppLogs">
                <RefreshCw :size="15" />
                刷新
              </button>
              <button type="button" @click="clearAppLogs">清空</button>
            </div>
          </header>

          <div class="app-logs__filters">
            <label>
              <span>分类</span>
              <select v-model="appLogScopeFilter">
                <option value="all">全部</option>
                <option
                  v-for="scope in appLogScopeOptions"
                  :key="scope"
                  :value="scope"
                >
                  {{ formatLogScope(scope) }}
                </option>
              </select>
            </label>
            <label>
              <span>服务</span>
              <select v-model="appLogServiceFilter">
                <option value="all">全部</option>
                <option
                  v-for="service in appLogServiceOptions"
                  :key="service"
                  :value="service"
                >
                  {{ service }}
                </option>
              </select>
            </label>
            <label>
              <span>状态</span>
              <select v-model="appLogStatusFilter">
                <option value="all">全部</option>
                <option value="success">成功</option>
                <option value="error">失败</option>
                <option value="pending">进行中</option>
              </select>
            </label>
            <strong>{{ filteredAppLogs.length }} 条</strong>
          </div>

          <div v-if="filteredAppLogs.length" class="app-logs__list">
            <article
              v-for="item in pagedAppLogs"
              :key="item.id"
              :class="[
                'app-logs__item',
                { 'app-logs__item--error': item.status === 'error' }
              ]"
            >
              <div class="app-logs__item-head">
                <strong>{{ formatLogTitle(item) }}</strong>
                <span>{{ formatLogStatus(item.status) }}</span>
              </div>
              <p v-if="item.message">{{ item.message }}</p>
              <div class="app-logs__meta">
                <span>{{ formatLogTime(item.createdAt) }}</span>
                <span>{{ formatLogScope(item.scope) }}</span>
                <span>{{ item.service || "未知服务" }}</span>
                <span>{{ item.method || item.channel }}</span>
                <span>{{ item.action }}</span>
                <span>{{ item.durationMs || 0 }}ms</span>
                <span>{{ item.traceId }}</span>
              </div>
              <pre v-if="item.payload">{{
                formatLogPayload(item.payload)
              }}</pre>
              <pre v-if="item.result">{{ formatLogPayload(item.result) }}</pre>
            </article>
          </div>
          <div v-if="filteredAppLogs.length" class="app-logs__pagination">
            <span>
              {{ appLogPageStart }}-{{ appLogPageEnd }} /
              {{ filteredAppLogs.length }}
            </span>
            <select v-model.number="appLogPageSize">
              <option :value="20">20 条/页</option>
              <option :value="50">50 条/页</option>
              <option :value="100">100 条/页</option>
            </select>
            <button
              type="button"
              :disabled="currentAppLogPage <= 1"
              @click="goAppLogPage(currentAppLogPage - 1)"
            >
              上一页
            </button>
            <strong>{{ currentAppLogPage }} / {{ appLogPageCount }}</strong>
            <button
              type="button"
              :disabled="currentAppLogPage >= appLogPageCount"
              @click="goAppLogPage(currentAppLogPage + 1)"
            >
              下一页
            </button>
          </div>
          <div v-else class="app-logs__empty">暂无调用日志。</div>
        </section>

        <section v-else class="app-shell__placeholder">
          <h1>{{ currentPlaceholder.title }}</h1>
          <p>{{ currentPlaceholder.description }}</p>
          <button
            class="status-button"
            type="button"
            @click="activeView = currentPlaceholder.backTo"
          >
            返回
            {{
              navItems.find((item) => item.id === currentPlaceholder.backTo)
                ?.label
            }}
          </button>
        </section>
      </section>
    </main>

    <SkillDrawer
      :cli-targets="state.cliTargets"
      :skill="selectedSkill"
      @close="selectedSkillName = ''"
      @install="installSkill"
      @uninstall="uninstallSkill"
      @repair="repairSkill"
      @open-path="openPath"
    />

    <CreateSkillModal
      v-if="showCreateSkill"
      @close="showCreateSkill = false"
      @submit="createSkill"
    />

    <ImportSkillsModal
      v-if="showImportSkills"
      :candidates="importCandidates"
      :loading="pending"
      @close="showImportSkills = false"
      @submit="confirmImportSkills"
    />

    <AddRepoModal
      v-if="showAddRepo"
      @close="showAddRepo = false"
      @submit="addRepo"
    />

    <BaseModal
      v-if="restorePreview"
      title="确认恢复配置"
      :description="restorePreviewDescription"
      @close="closeRestorePreview"
    >
      <form class="restore-preview-modal" @submit.prevent="confirmRestore">
        <div class="restore-preview-modal__summary">
          <span class="restore-preview-modal__summary-pill"
            >新增 {{ restoreAddedItems.length }} 项</span
          >
          <span class="restore-preview-modal__summary-pill"
            >冲突 {{ restoreConflictItems.length }} 项</span
          >
        </div>

        <p class="restore-preview-modal__notice">
          新增项会合并到当前数据；Provider 和 Runtime Profile 恢复后保持未启用。
        </p>

        <div class="restore-preview-modal__body">
          <section
            v-if="restoreAddedItems.length"
            class="restore-preview-modal__section"
          >
            <h3 class="restore-preview-modal__section-title">将新增</h3>
            <section
              v-for="group in restoreAddedGroups"
              :key="group.path"
              class="restore-preview-modal__group"
            >
              <div class="restore-preview-modal__group-head">
                <strong>{{ group.path }}</strong>
                <span>{{ group.items.length }} 项</span>
              </div>
              <div class="restore-preview-modal__tree">
                <template v-for="row in group.rows" :key="row.key">
                  <div
                    v-if="row.kind === 'dir'"
                    class="restore-preview-modal__tree-folder"
                    :style="{ paddingLeft: `${row.depth * 18 + 10}px` }"
                  >
                    <strong>{{ row.name }}</strong>
                    <span>{{ row.itemCount }} 项</span>
                  </div>
                  <article
                    v-else
                    class="restore-preview-modal__item restore-preview-modal__tree-item"
                    :style="{ marginLeft: `${row.depth * 18}px` }"
                  >
                    <strong class="restore-preview-modal__item-name"
                      >{{ row.item.type }}：{{ row.item.name }}</strong
                    >
                    <span class="restore-preview-modal__item-path">{{
                      row.relativePath
                    }}</span>
                  </article>
                </template>
              </div>
            </section>
          </section>

          <section
            v-if="restoreConflictItems.length"
            class="restore-preview-modal__section"
          >
            <h3 class="restore-preview-modal__section-title">需要选择</h3>
            <section
              v-for="group in restoreConflictGroups"
              :key="group.path"
              class="restore-preview-modal__group"
            >
              <div class="restore-preview-modal__group-head">
                <strong>{{ group.path }}</strong>
                <div class="restore-preview-modal__group-actions">
                  <span>{{ group.items.length }} 项</span>
                  <button
                    class="restore-preview-modal__bulk-button"
                    type="button"
                    :disabled="pending"
                    @click="chooseRestoreItems(group.items, 'current')"
                  >
                    保留当前
                  </button>
                  <button
                    class="restore-preview-modal__bulk-button"
                    type="button"
                    :disabled="pending"
                    @click="chooseRestoreItems(group.items, 'backup')"
                  >
                    使用备份
                  </button>
                </div>
              </div>
              <div class="restore-preview-modal__tree">
                <template v-for="row in group.rows" :key="row.key">
                  <div
                    v-if="row.kind === 'dir'"
                    class="restore-preview-modal__tree-folder"
                    :style="{ paddingLeft: `${row.depth * 18 + 10}px` }"
                  >
                    <strong>{{ row.name }}</strong>
                    <div class="restore-preview-modal__directory-actions">
                      <span>{{ row.itemCount }} 项</span>
                      <button
                        class="restore-preview-modal__bulk-button"
                        type="button"
                        :disabled="pending"
                        @click="chooseRestoreItems(row.items, 'current')"
                      >
                        保留当前
                      </button>
                      <button
                        class="restore-preview-modal__bulk-button"
                        type="button"
                        :disabled="pending"
                        @click="chooseRestoreItems(row.items, 'backup')"
                      >
                        使用备份
                      </button>
                    </div>
                  </div>
                  <article
                    v-else
                    class="restore-preview-modal__conflict restore-preview-modal__tree-item"
                    :style="{ marginLeft: `${row.depth * 18}px` }"
                  >
                    <div class="restore-preview-modal__conflict-head">
                      <div>
                        <strong class="restore-preview-modal__item-name"
                          >{{ row.item.type }}：{{ row.item.name }}</strong
                        >
                        <span class="restore-preview-modal__item-path">{{
                          row.relativePath
                        }}</span>
                      </div>
                      <button
                        class="restore-preview-modal__compare-button"
                        type="button"
                        :disabled="pending"
                        @click="toggleRestoreCompare(row.item)"
                      >
                        对比
                      </button>
                    </div>
                    <label class="restore-preview-modal__choice">
                      <input
                        v-model="restoreChoices[row.item.key]"
                        type="radio"
                        :name="`restore-${row.item.key}`"
                        value="current"
                        :disabled="pending"
                      />
                      <span class="restore-preview-modal__choice-text"
                        >保留当前版本</span
                      >
                    </label>
                    <label class="restore-preview-modal__choice">
                      <input
                        v-model="restoreChoices[row.item.key]"
                        type="radio"
                        :name="`restore-${row.item.key}`"
                        value="backup"
                        :disabled="pending"
                      />
                      <span class="restore-preview-modal__choice-text"
                        >使用备份版本</span
                      >
                    </label>
                  </article>
                </template>
              </div>
            </section>
          </section>

          <div
            v-if="!restoreAddedItems.length && !restoreConflictItems.length"
            class="restore-preview-modal__empty"
          >
            当前数据和备份没有差异。
          </div>
        </div>

        <div class="restore-preview-modal__actions">
          <button
            class="status-button"
            type="button"
            :disabled="pending"
            @click="closeRestorePreview"
          >
            取消
          </button>
          <button
            class="status-button restore-preview-modal__primary"
            type="submit"
            :disabled="pending || !restoreCanSubmit"
          >
            {{ pending ? "恢复中..." : "确认恢复" }}
          </button>
        </div>
      </form>
    </BaseModal>

    <BaseModal
      v-if="restoreCompareItem"
      title="检查恢复差异"
      :description="restoreCompareDescription"
      @close="closeRestoreCompare"
    >
      <div class="restore-preview-modal restore-preview-modal--compare">
        <div class="restore-preview-modal__compare-summary">
          已标记 {{ restoreCompareChangedCount }} 处不同
        </div>
        <div
          class="restore-preview-modal__compare restore-preview-modal__compare--dialog"
        >
          <section class="restore-preview-modal__compare-panel">
            <strong>当前内容</strong>
            <div
              ref="restoreCurrentCompareCodeRef"
              class="restore-preview-modal__compare-code"
              @scroll="syncRestoreCompareScroll('current')"
            >
              <div
                v-for="row in restoreCompareRows"
                :key="`current-${row.index}`"
                :class="[
                  'restore-preview-modal__compare-line',
                  `restore-preview-modal__compare-line--${row.currentStatus}`
                ]"
              >
                <span class="restore-preview-modal__compare-number">{{
                  row.currentLineNumber
                }}</span>
                <span class="restore-preview-modal__compare-marker">{{
                  row.currentMarker
                }}</span>
                <span class="restore-preview-modal__compare-text">{{
                  row.currentText
                }}</span>
              </div>
            </div>
          </section>
          <section class="restore-preview-modal__compare-panel">
            <strong>备份内容</strong>
            <div
              ref="restoreBackupCompareCodeRef"
              class="restore-preview-modal__compare-code"
              @scroll="syncRestoreCompareScroll('backup')"
            >
              <div
                v-for="row in restoreCompareRows"
                :key="`backup-${row.index}`"
                :class="[
                  'restore-preview-modal__compare-line',
                  `restore-preview-modal__compare-line--${row.backupStatus}`
                ]"
              >
                <span class="restore-preview-modal__compare-number">{{
                  row.backupLineNumber
                }}</span>
                <span class="restore-preview-modal__compare-marker">{{
                  row.backupMarker
                }}</span>
                <span class="restore-preview-modal__compare-text">{{
                  row.backupText
                }}</span>
              </div>
            </div>
          </section>
        </div>
        <div class="restore-preview-modal__actions">
          <button
            class="status-button restore-preview-modal__primary"
            type="button"
            @click="closeRestoreCompare"
          >
            确定
          </button>
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-if="cloudBackupView"
      title="云端备份内容"
      :description="cloudBackupDescription"
      @close="closeCloudBackupView"
    >
      <div class="cloud-backup-modal">
        <div class="cloud-backup-modal__summary">
          <span>文件 {{ cloudBackupView.backup.fileCount }} 个</span>
          <span>目录 {{ cloudBackupView.backup.directoryCount }} 个</span>
          <span>条目 {{ cloudBackupView.backup.entryCount }} 个</span>
        </div>

        <div class="cloud-backup-modal__body">
          <aside class="cloud-backup-modal__list">
            <button
              v-for="entry in cloudBackupView.backup.entries"
              :key="entry.path"
              :class="[
                'cloud-backup-modal__entry',
                {
                  'cloud-backup-modal__entry--active':
                    entry.path === selectedCloudBackupPath
                }
              ]"
              type="button"
              @click="selectedCloudBackupPath = entry.path"
            >
              <strong>{{ entry.typeName }}</strong>
              <span>{{ entry.path }}</span>
            </button>
          </aside>

          <section class="cloud-backup-modal__content">
            <div
              v-if="selectedCloudBackupEntry"
              class="cloud-backup-modal__head"
            >
              <div>
                <strong>{{ selectedCloudBackupEntry.typeName }}</strong>
                <span>{{ selectedCloudBackupEntry.path }}</span>
              </div>
              <small>{{
                formatBackupEntrySize(selectedCloudBackupEntry.size)
              }}</small>
            </div>
            <pre v-if="selectedCloudBackupEntry">{{
              selectedCloudBackupEntry.content || "空内容"
            }}</pre>
          </section>
        </div>
      </div>
    </BaseModal>

    <div v-if="updateDialog.open" class="update-modal">
      <div class="update-modal__overlay"></div>
      <section class="update-modal__panel" role="dialog" aria-modal="true">
        <header class="update-modal__header">
          <div>
            <span>应用更新</span>
            <h2>{{ updateDialogTitle }}</h2>
          </div>
          <button
            class="update-modal__icon-button"
            type="button"
            aria-label="关闭更新面板"
            :disabled="
              ['checking', 'downloading', 'installing'].includes(
                updateDialog.phase
              )
            "
            @click="closeUpdateDialog"
          >
            <X :size="17" />
          </button>
        </header>

        <div class="update-modal__body">
          <div class="update-modal__mark">
            <Info :size="22" />
          </div>
          <div class="update-modal__copy">
            <span>{{ updateDialogMessage }}</span>
          </div>
        </div>

        <div
          v-if="updateDialog.phase === 'downloading'"
          class="update-modal__progress"
        >
          <div class="update-modal__progress-head">
            <span>{{ updateTransferText }}</span>
            <strong>{{ updateProgressText }}</strong>
          </div>
          <div class="update-modal__progress-track">
            <div
              class="update-modal__progress-bar"
              :style="{ width: updateProgressWidth }"
            ></div>
          </div>
        </div>

        <pre
          v-if="
            updateDialog.releaseNotes && updateDialog.phase !== 'downloading'
          "
          class="update-modal__notes"
          >{{ updateDialog.releaseNotes }}</pre
        >

        <footer class="update-modal__footer">
          <button
            v-if="updateDialog.phase === 'available'"
            class="update-modal__button"
            type="button"
            @click="closeUpdateDialog"
          >
            稍后
          </button>
          <button
            v-if="updateDialog.phase === 'available'"
            class="update-modal__button update-modal__button--primary"
            type="button"
            @click="downloadAppUpdate"
          >
            <RefreshCw :size="15" />
            立即下载
          </button>
          <button
            v-else-if="updateDialog.phase === 'downloaded'"
            class="update-modal__button"
            type="button"
            @click="closeUpdateDialog"
          >
            稍后
          </button>
          <button
            v-if="updateDialog.phase === 'downloaded'"
            class="update-modal__button update-modal__button--primary"
            type="button"
            @click="installAppUpdate"
          >
            打开安装向导
          </button>
          <button
            v-else-if="updateDialog.phase === 'downloading'"
            class="update-modal__button"
            type="button"
            disabled
          >
            下载中...
          </button>
          <button
            v-else-if="
              !['available', 'downloaded', 'downloading'].includes(
                updateDialog.phase
              )
            "
            class="update-modal__button update-modal__button--primary"
            type="button"
            @click="closeUpdateDialog"
          >
            确定
          </button>
        </footer>
      </section>
    </div>

    <div v-if="showCloseConfirm" class="close-confirm">
      <div class="close-confirm__overlay"></div>
      <section class="close-confirm__panel" role="dialog" aria-modal="true">
        <header class="close-confirm__header">
          <div>
            <span>窗口操作</span>
            <h2>关闭应用</h2>
          </div>
          <button
            class="close-confirm__icon-button"
            type="button"
            aria-label="取消关闭"
            @click="submitCloseAction('cancel')"
          >
            <X :size="17" />
          </button>
        </header>

        <div class="close-confirm__body">
          <div class="close-confirm__mark">
            <Info :size="22" />
          </div>
          <div class="close-confirm__copy">
            <strong>关闭按钮要执行什么操作？</strong>
            <span>可以最小化到托盘继续运行，也可以直接关闭软件。</span>
          </div>
        </div>

        <footer class="close-confirm__footer">
          <label class="close-confirm__remember">
            <input v-model="closeRemember" type="checkbox" />
            <span>记住我的选择</span>
          </label>
          <div class="close-confirm__actions">
            <button
              class="close-confirm__button close-confirm__button--primary"
              type="button"
              @click="submitCloseAction('minimize')"
            >
              <Minus :size="15" />
              最小化到托盘
            </button>
            <button
              class="close-confirm__button"
              type="button"
              @click="submitCloseAction('quit')"
            >
              <Power :size="15" />
              直接关闭
            </button>
            <button
              class="close-confirm__button"
              type="button"
              @click="submitCloseAction('cancel')"
            >
              取消
            </button>
          </div>
        </footer>
      </section>
    </div>

    <SelectionTranslator :active-view="activeView" />
    <GlobalLoading />
  </div>
</template>

<script setup>
import {
  computed,
  defineAsyncComponent,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch
} from "vue"
import {
  BarChart3,
  ChevronDown,
  Compass,
  ExternalLink,
  Gauge,
  Info,
  Minus,
  Network,
  Power,
  RefreshCw,
  Settings,
  ShieldCheck,
  Wrench,
  X
} from "lucide-vue-next"
import AppSidebar from "@/components/AppSidebar.vue"
import BaseModal from "@/components/BaseModal.vue"
import GlobalLoading from "@/components/GlobalLoading.vue"
import SelectionTranslator from "@/components/SelectionTranslator.vue"
import logoUrl from "@/assets/ai-manager-logo.svg?url"
import {
  accountApi,
  appApi,
  dataApi,
  providerApi,
  proxyApi,
  repoApi,
  ruleApi,
  runtimeApi,
  sessionApi,
  settingsApi,
  skillApi,
  systemApi
} from "@/api"
import { useGlobalLoading } from "@/utils/global-loading"
import { createMessage } from "@/utils/message"

const ProvidersView = defineAsyncComponent(
  () => import("@/features/providers/index.vue")
)
const ToolsView = defineAsyncComponent(
  () => import("@/features/tools/index.vue")
)
const AddRepoModal = defineAsyncComponent(
  () => import("@/features/repos/components/AddRepoModal.vue")
)
const RulesView = defineAsyncComponent(
  () => import("@/features/rules/index.vue")
)
const SessionsView = defineAsyncComponent(
  () => import("@/features/sessions/index.vue")
)
const SettingsView = defineAsyncComponent(
  () => import("@/features/settings/index.vue")
)
const SkillsView = defineAsyncComponent(
  () => import("@/features/skills/index.vue")
)
const SkillUsageView = defineAsyncComponent(
  () => import("@/features/skills/usage.vue")
)
const UsageView = defineAsyncComponent(
  () => import("@/features/usage/index.vue")
)
const CreateSkillModal = defineAsyncComponent(
  () => import("@/features/skills/components/CreateSkillModal.vue")
)
const ImportSkillsModal = defineAsyncComponent(
  () => import("@/features/skills/components/ImportSkillsModal.vue")
)
const SkillDrawer = defineAsyncComponent(
  () => import("@/features/skills/components/SkillDrawer.vue")
)

const baseNavItems = [
  { id: "providers", label: "Providers", icon: Network },
  { id: "usage", label: "Usage", icon: BarChart3 },
  { id: "skills", label: "Skills", icon: ShieldCheck },
  { id: "sessions", label: "Sessions", icon: Gauge },
  { id: "rules", label: "Rules", icon: Compass },
  { id: "tools", label: "Tools", icon: Wrench },
  { id: "settings", label: "Settings", icon: Settings }
]

const queryParams = new URLSearchParams(window.location.search)
const queryView = queryParams.get("view")
const isQuickSwitchPanel = queryParams.get("panel") === "quick-switch"

const placeholderMap = {
  sessions: {
    title: "Session System",
    description: "当前视图已经接入 Session 聚合，请从侧边栏重新进入。",
    backTo: "providers"
  },
  workspace: {
    title: "Workspace 视图待扩展",
    description: "当前工作区路径已经由主进程管理，可在设置页配置相关目录。",
    backTo: "providers"
  },
  settings: {
    title: "Settings",
    description: "设置页已经接入。",
    backTo: "providers"
  }
}

const state = reactive({
  cliTargets: [],
  skills: [],
  skillRepositories: [],
  repos: [],
  sessions: [],
  usage: {},
  codexAccounts: [],
  codexLoginState: null,
  claudeProxyState: {
    enabled: false,
    localBaseUrl: "",
    activeProviderId: "",
    failoverProviderIds: []
  },
  codexProxyState: {
    enabled: false,
    localBaseUrl: "",
    activeProviderId: "",
    failoverProviderIds: [],
    accountModel: ""
  },
  providers: [],
  rules: {
    supportedClis: [],
    prompts: [],
    profiles: {},
    runtimeState: {}
  },
  runtimeConfigSchemas: {},
  runtimeModels: [],
  runtimeProviderState: {},
  runtimeProfiles: [],
  diagnostics: [],
  paths: {
    workspaceRoot: "",
    skillsDir: "",
    promptsDir: "",
    promptProfilesDir: "",
    reposDir: "",
    sessionRecycleDir: "",
    storageDir: ""
  },
  appSettings: {
    dataPath: "",
    defaultDataPath: "",
    settingsFilePath: "",
    restartRequired: false,
    cliConfigPaths: {
      claude: "",
      codex: ""
      // 当前版本暂不启用 Gemini。
      // gemini: ""
    },
    defaultCliConfigPaths: {
      claude: "",
      codex: ""
      // 当前版本暂不启用 Gemini。
      // gemini: ""
    },
    cloudSync: {
      provider: "jianguoyun",
      webdavUrl: "",
      username: "",
      password: "",
      fileName: ""
    },
    localBackup: {
      enabled: true,
      intervalMinutes: 60,
      maxCount: 20,
      lastBackupAt: 0
    },
    system: {
      closeAction: "ask",
      quickSwitchVisible: true,
      autoLaunchEnabled: false
    }
  },
  refreshedAt: 0
})

const activeView = ref(
  baseNavItems.some((item) => item.id === queryView) ? queryView : "providers"
)
const showLogsTab = ref(false)
const sidebarTitleClickCount = ref(0)
const appLogs = ref([])
const appLogPath = ref("")
const appLogScopeFilter = ref("all")
const appLogServiceFilter = ref("all")
const appLogStatusFilter = ref("all")
const appLogPage = ref(1)
const appLogPageSize = ref(20)
const quickSelectedCli = ref("")
const quickMode = ref("usage")
const quickCollapsed = ref(false)
const quickLogoDrag = {
  active: false,
  moved: false,
  lastX: 0,
  lastY: 0,
  totalX: 0,
  totalY: 0
}
const sidebarCollapsed = ref(false)
const selectedSkillName = ref("")
const showCreateSkill = ref(false)
const showImportSkills = ref(false)
const showAddRepo = ref(false)
const showCloseConfirm = ref(false)
const closeRemember = ref(false)
const updateDialog = reactive({
  open: false,
  phase: "idle",
  message: "",
  version: "",
  releaseNotes: "",
  percent: 0,
  transferred: 0,
  total: 0,
  bytesPerSecond: 0,
  installDirectory: "",
  manual: false
})
const importCandidates = ref([])
const localBackups = ref([])
const localBackupDirectory = ref("")
const restorePreview = ref(null)
const restoreSource = ref(null)
const restoreCompareKey = ref("")
const restoreCurrentCompareCodeRef = ref(null)
const restoreBackupCompareCodeRef = ref(null)
const restoreChoices = reactive({})
const cloudBackupView = ref(null)
const selectedCloudBackupPath = ref("")
const { loading: pending, withGlobalLoading } = useGlobalLoading()

let unsubscribe = null
let unsubscribeClose = null
let unsubscribeUpdate = null
let syncingRestoreCompareScroll = false

const navItems = computed(() =>
  showLogsTab.value
    ? [...baseNavItems, { id: "logs", label: "日志", icon: Info }]
    : baseNavItems
)

const appLogScopeOptions = computed(() =>
  [...new Set(appLogs.value.map((item) => item.scope || "backend"))].sort()
)

const appLogServiceOptions = computed(() =>
  [
    ...new Set(
      appLogs.value.map((item) => item.service || "未知服务").filter(Boolean)
    )
  ].sort()
)

const filteredAppLogs = computed(() =>
  appLogs.value.filter((item) => {
    const scope = item.scope || "backend"
    const service = item.service || "未知服务"

    return (
      (appLogScopeFilter.value === "all" ||
        appLogScopeFilter.value === scope) &&
      (appLogServiceFilter.value === "all" ||
        appLogServiceFilter.value === service) &&
      (appLogStatusFilter.value === "all" ||
        appLogStatusFilter.value === item.status)
    )
  })
)

const appLogPageCount = computed(() =>
  Math.max(1, Math.ceil(filteredAppLogs.value.length / appLogPageSize.value))
)

const currentAppLogPage = computed(() =>
  Math.min(appLogPage.value, appLogPageCount.value)
)

const appLogPageStart = computed(() => {
  if (!filteredAppLogs.value.length) {
    return 0
  }

  return (currentAppLogPage.value - 1) * appLogPageSize.value + 1
})

const appLogPageEnd = computed(() =>
  Math.min(
    currentAppLogPage.value * appLogPageSize.value,
    filteredAppLogs.value.length
  )
)

const pagedAppLogs = computed(() =>
  filteredAppLogs.value.slice(appLogPageStart.value - 1, appLogPageEnd.value)
)

const updateDialogTitle = computed(() => {
  const titleMap = {
    checking: "正在检查更新",
    available: "发现新版本",
    downloading: "正在下载更新",
    downloaded: "更新已下载",
    installing: "正在安装更新",
    "not-available": "当前已是最新版本",
    unconfigured: "缺少更新配置",
    "dev-disabled": "开发模式无法完整检查更新",
    error: "检查更新失败"
  }

  return titleMap[updateDialog.phase] || "检查更新"
})

const updateDialogMessage = computed(() => {
  return updateDialog.message || "正在准备更新状态。"
})

const updateProgressWidth = computed(() => {
  const percent = Math.min(100, Math.max(0, Number(updateDialog.percent || 0)))

  return `${percent}%`
})

const updateProgressText = computed(() => {
  const percent = Math.min(100, Math.max(0, Number(updateDialog.percent || 0)))

  return `${percent.toFixed(1)}%`
})

const updateTransferText = computed(() => {
  if (!updateDialog.total) {
    return "正在获取下载进度"
  }

  return `${formatUpdateBytes(updateDialog.transferred)} / ${formatUpdateBytes(
    updateDialog.total
  )}`
})

watch(
  [appLogScopeFilter, appLogServiceFilter, appLogStatusFilter, appLogPageSize],
  () => {
    appLogPage.value = 1
  }
)

const selectedSkill = computed(() => {
  return (
    state.skills.find((item) => item.name === selectedSkillName.value) || null
  )
})

const currentPlaceholder = computed(() => {
  return placeholderMap[activeView.value] || placeholderMap.sessions
})

const restoreAddedItems = computed(() => {
  return restorePreview.value?.added || []
})

const restoreConflictItems = computed(() => {
  return restorePreview.value?.conflicts || []
})

const restoreAddedGroups = computed(() => {
  return groupRestoreItems(restoreAddedItems.value)
})

const restoreConflictGroups = computed(() => {
  return groupRestoreItems(restoreConflictItems.value)
})

const restoreCompareItem = computed(() => {
  return (
    restoreConflictItems.value.find(
      (item) => item.key === restoreCompareKey.value
    ) || null
  )
})

const restoreCompareRows = computed(() => {
  if (!restoreCompareItem.value) {
    return []
  }

  return createRestoreCompareRows(
    restoreCompareItem.value.currentContent,
    restoreCompareItem.value.backupContent
  )
})

const restoreCompareChangedCount = computed(() => {
  return restoreCompareRows.value.filter((item) => item.status !== "same")
    .length
})

const restoreCompareDescription = computed(() => {
  if (!restoreCompareItem.value) {
    return ""
  }

  return `${restoreCompareItem.value.type}：${restoreCompareItem.value.name} · ${restoreCompareItem.value.path}`
})

const restorePreviewDescription = computed(() => {
  const sourceName =
    restoreSource.value?.type === "cloud"
      ? restoreSource.value.fileName
      : restoreSource.value?.type === "local"
        ? restoreSource.value.fileName || "本地自动备份"
        : restoreSource.value?.filePath || "本地备份"

  return `从 ${sourceName} 兼容合并配置数据。`
})

const cloudBackupDescription = computed(() => {
  if (!cloudBackupView.value) {
    return ""
  }

  return `${cloudBackupView.value.fileName} · 创建于 ${formatCloudBackupTime(
    cloudBackupView.value.backup.createdAt
  )}`
})

const selectedCloudBackupEntry = computed(() => {
  return (
    cloudBackupView.value?.backup.entries.find(
      (entry) => entry.path === selectedCloudBackupPath.value
    ) || null
  )
})

const restoreCanSubmit = computed(() => {
  return Boolean(
    restoreAddedItems.value.length || restoreConflictItems.value.length
  )
})

const quickCliTargets = computed(() => {
  return state.cliTargets.filter((item) => {
    return state.runtimeConfigSchemas[item.id]?.enabled
  })
})

const quickActiveCli = computed(() => {
  return (
    quickCliTargets.value.find((item) => item.id === quickSelectedCli.value) ||
    quickCliTargets.value[0] ||
    null
  )
})

const quickActiveProfile = computed(() => {
  return (
    state.runtimeProfiles.find(
      (item) => item.cli === quickActiveCli.value?.id
    ) || null
  )
})

const quickActiveProxyState = computed(() => {
  if (quickActiveCli.value?.id === "claude") {
    return state.claudeProxyState
  }

  if (quickActiveCli.value?.id === "codex") {
    return state.codexProxyState
  }

  return null
})

const quickProxyActiveTargetId = computed(() => {
  if (!quickActiveProxyState.value?.enabled) {
    return ""
  }

  return quickActiveProxyState.value.activeProviderId || ""
})

const quickProxyActiveProvider = computed(() => {
  if (!quickProxyActiveTargetId.value.startsWith("account:")) {
    return (
      state.providers.find(
        (item) => item.id === quickProxyActiveTargetId.value
      ) || null
    )
  }

  return null
})

const quickProxyActiveAccount = computed(() => {
  if (!quickProxyActiveTargetId.value.startsWith("account:")) {
    return null
  }

  const accountId = quickProxyActiveTargetId.value.slice("account:".length)

  return state.codexAccounts.find((item) => item.id === accountId) || null
})

const quickActiveProvider = computed(() => {
  if (quickProxyActiveTargetId.value) {
    return quickProxyActiveProvider.value
  }

  return (
    state.providers.find(
      (item) => item.id === quickActiveProfile.value?.providerId
    ) || null
  )
})

const quickActiveAccount = computed(() => {
  if (quickActiveCli.value?.id !== "codex") {
    return null
  }

  if (quickProxyActiveTargetId.value) {
    return quickProxyActiveAccount.value
  }

  return state.codexAccounts.find((item) => item.active) || null
})

const quickActiveName = computed(() => {
  if (quickProxyActiveTargetId.value) {
    return `Proxy 接管中：${
      quickActiveProvider.value?.name ||
      quickActiveAccount.value?.email ||
      quickActiveAccount.value?.accountId ||
      "未激活"
    }`
  }

  if (quickActiveAccount.value) {
    return (
      quickActiveAccount.value.email ||
      quickActiveAccount.value.accountId ||
      "Codex 官方账号"
    )
  }

  return quickActiveProvider.value?.name || "未启用"
})

const quickItems = computed(() => {
  if (!quickActiveCli.value) {
    return []
  }

  const providerItems = state.providers
    .filter((item) => {
      return item.cli === quickActiveCli.value.id && item.enabled !== false
    })
    .map((provider) => {
      const model = firstQuickModelName(provider)
      const active = quickProxyActiveTargetId.value
        ? quickProxyActiveTargetId.value === provider.id
        : !quickActiveAccount.value &&
          quickActiveProvider.value?.id === provider.id

      return {
        key: `provider:${provider.id}`,
        type: "provider",
        provider,
        model,
        label: provider.name,
        description: model || "缺少模型",
        active,
        disabled: !model || provider.enabled === false
      }
    })

  if (quickActiveCli.value.id !== "codex") {
    return providerItems
  }

  return [
    ...providerItems,
    ...state.codexAccounts.map((account) => ({
      key: `account:${account.id}`,
      type: "account",
      account,
      label: account.email || account.accountId || "Codex 官方账号",
      description: formatQuickAccountDescription(account),
      quotas: formatQuickAccountQuotas(account),
      active: quickProxyActiveTargetId.value
        ? quickProxyActiveTargetId.value === `account:${account.id}`
        : account.active,
      disabled: Boolean(account.disabled)
    }))
  ]
})

const quickUsageLogs = computed(() => {
  return (state.usage.logs || []).filter((item) => {
    return item.appType === quickActiveCli.value?.id
  })
})

const quickUsageSummary = computed(() => {
  return quickUsageLogs.value.reduce(
    (result, item) => {
      result.requestCount += 1
      result.actualTokens += Number(item.actualTokens || 0)
      result.totalCostUsd += Number(item.totalCostUsd || 0)
      return result
    },
    {
      requestCount: 0,
      actualTokens: 0,
      totalCostUsd: 0
    }
  )
})

const quickUsageTrend = computed(() => {
  const groups = new Map()

  for (const item of quickUsageLogs.value) {
    const date = new Date(Number(item.createdAt || 0))
    const key = date.toLocaleDateString("zh-CN")

    groups.set(key, {
      date: key,
      label: `${date.getMonth() + 1}/${date.getDate()}`,
      timestamp: new Date(
        date.getFullYear(),
        date.getMonth(),
        date.getDate()
      ).getTime(),
      actualTokens:
        (groups.get(key)?.actualTokens || 0) + Number(item.actualTokens || 0)
    })
  }

  const rows = Array.from(groups.values())
    .sort((left, right) => left.timestamp - right.timestamp)
    .slice(-7)
  const maxTokens = Math.max(...rows.map((item) => item.actualTokens), 1)

  return rows.map((item) => ({
    ...item,
    percent: Math.max(8, Math.round((item.actualTokens / maxTokens) * 100))
  }))
})

const quickUsageProviders = computed(() => {
  const groups = new Map()

  for (const item of quickUsageLogs.value) {
    const key = item.providerId || item.providerName || "unknown"
    const previous = groups.get(key) || {
      providerId: key,
      providerName: item.providerName || "未知 Provider",
      actualTokens: 0,
      totalCostUsd: 0
    }

    previous.actualTokens += Number(item.actualTokens || 0)
    previous.totalCostUsd += Number(item.totalCostUsd || 0)
    groups.set(key, previous)
  }

  const rows = Array.from(groups.values())
    .sort((left, right) => right.actualTokens - left.actualTokens)
    .slice(0, 2)
  const maxTokens = Math.max(...rows.map((item) => item.actualTokens), 1)

  return rows.map((item) => ({
    ...item,
    percent: Math.max(4, Math.round((item.actualTokens / maxTokens) * 100))
  }))
})

async function bootstrap() {
  await withGlobalLoading(async () => {
    try {
      updateState(await appApi.bootstrap())
      await refreshLocalBackups(false)
      unsubscribe = appApi.onStateChanged((nextState) => {
        const previousLocalBackupAt =
          state.appSettings.localBackup?.lastBackupAt || 0
        updateState(nextState)
        const nextLocalBackupAt =
          state.appSettings.localBackup?.lastBackupAt || 0

        if (nextLocalBackupAt !== previousLocalBackupAt) {
          refreshLocalBackups(false)
        }
      })
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

function updateState(nextState) {
  if ("cliTargets" in nextState) {
    state.cliTargets = nextState.cliTargets || []
  }
  if ("skills" in nextState) {
    state.skills = nextState.skills || []
  }
  if ("skillRepositories" in nextState) {
    state.skillRepositories = nextState.skillRepositories || []
  }
  if ("repos" in nextState) {
    state.repos = nextState.repos || []
  }
  if ("sessions" in nextState) {
    state.sessions = nextState.sessions || []
  }
  if ("usage" in nextState) {
    state.usage = nextState.usage || {}
  }
  if ("codexAccounts" in nextState) {
    state.codexAccounts = nextState.codexAccounts || []
  }
  if ("codexLoginState" in nextState) {
    state.codexLoginState = nextState.codexLoginState || null
  }
  if ("claudeProxyState" in nextState) {
    state.claudeProxyState = nextState.claudeProxyState || {
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: []
    }
  }
  if ("codexProxyState" in nextState) {
    state.codexProxyState = nextState.codexProxyState || {
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: [],
      accountModel: ""
    }
  }
  if ("providers" in nextState) {
    state.providers = nextState.providers || []
  }
  if ("rules" in nextState) {
    state.rules = nextState.rules || state.rules
  }
  if ("runtimeConfigSchemas" in nextState) {
    state.runtimeConfigSchemas = nextState.runtimeConfigSchemas || {}
  }
  if ("runtimeModels" in nextState) {
    state.runtimeModels = nextState.runtimeModels || []
  }
  if ("runtimeProviderState" in nextState) {
    state.runtimeProviderState = nextState.runtimeProviderState || {}
  }
  if ("runtimeProfiles" in nextState) {
    state.runtimeProfiles = nextState.runtimeProfiles || []
  }
  if ("diagnostics" in nextState) {
    state.diagnostics = nextState.diagnostics || []
  }
  if ("paths" in nextState) {
    state.paths = nextState.paths || state.paths
  }
  if ("appSettings" in nextState) {
    state.appSettings = nextState.appSettings || state.appSettings
  }
  if ("refreshedAt" in nextState) {
    state.refreshedAt = nextState.refreshedAt || 0
  }

  if (!("claudeProxyState" in nextState) && !state.claudeProxyState) {
    state.claudeProxyState = {
    enabled: false,
    localBaseUrl: "",
    activeProviderId: "",
    failoverProviderIds: []
  }
  }
  if (!("codexProxyState" in nextState) && !state.codexProxyState) {
    state.codexProxyState = {
    enabled: false,
    localBaseUrl: "",
    activeProviderId: "",
    failoverProviderIds: [],
    accountModel: ""
  }
  }
  ensureQuickSelectedCli()

  if (
    selectedSkillName.value &&
    !state.skills.find((item) => item.name === selectedSkillName.value)
  ) {
    selectedSkillName.value = ""
  }
}

function ensureQuickSelectedCli() {
  if (
    quickSelectedCli.value &&
    quickCliTargets.value.find((item) => item.id === quickSelectedCli.value)
  ) {
    return
  }

  quickSelectedCli.value = quickCliTargets.value[0]?.id || ""
}

async function handleSidebarTitleClick() {
  sidebarTitleClickCount.value += 1

  if (sidebarTitleClickCount.value < 10) {
    return
  }

  showLogsTab.value = true
  activeView.value = "logs"
  sidebarTitleClickCount.value = 0
  await loadAppLogs()
}

async function loadAppLogs() {
  const result = await appApi.getAppLogs()

  appLogs.value = result.logs || []
  appLogPath.value = result.filePath || ""
  appLogPage.value = 1
}

async function clearAppLogs() {
  const result = await appApi.clearAppLogs()

  appLogs.value = result.logs || []
  appLogPath.value = result.filePath || ""
  appLogPage.value = 1
}

function formatLogTime(value) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(Number(value || 0)))
}

function formatCloudBackupTime(value) {
  const timestamp = Number(value || 0)

  if (!timestamp) {
    return "未知时间"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(timestamp))
}

function formatBackupEntrySize(value) {
  const size = Number(value || 0)

  if (size < 1024) {
    return `${size} B`
  }

  if (size < 1024 * 1024) {
    return `${(size / 1024).toFixed(1)} KB`
  }

  return `${(size / 1024 / 1024).toFixed(2)} MB`
}

function formatLogPayload(value) {
  return JSON.stringify(value, null, 2)
}

function formatLogScope(value) {
  if (value === "backend") {
    return "后端"
  }

  if (value === "renderer") {
    return "渲染进程"
  }

  return value || "未知"
}

function formatLogStatus(value) {
  if (value === "success") {
    return "成功"
  }

  if (value === "error") {
    return "失败"
  }

  if (value === "pending") {
    return "进行中"
  }

  return value || "未知"
}

function formatLogTitle(item) {
  return [item.service, item.method || item.channel].filter(Boolean).join(".")
}

function goAppLogPage(page) {
  appLogPage.value = Math.min(Math.max(page, 1), appLogPageCount.value)
}

async function runAction(action) {
  return withGlobalLoading(async () => {
    try {
      const nextState = await action()
      if (nextState && typeof nextState === "object") {
        updateState(nextState)
      }
      return true
    } catch (error) {
      showErrorMessage(error)
      return false
    }
  })
}

function selectSkill(skill) {
  selectedSkillName.value = skill.name
}

function showErrorMessage(error) {
  createMessage.error(error.message || String(error))
}

function isCodexAccountRefreshError(error) {
  return Boolean(error)
}

function getProxyState(cli) {
  if (cli === "claude") {
    return state.claudeProxyState
  }

  if (cli === "codex") {
    return state.codexProxyState
  }

  return null
}

function getProxyApi(cli) {
  if (cli === "claude") {
    return {
      enable: proxyApi.enableClaudeProxy,
      disable: proxyApi.disableClaudeProxy,
      addProvider: proxyApi.addClaudeProxyProvider,
      removeProvider: proxyApi.removeClaudeProxyProvider,
      activateProvider: proxyApi.activateClaudeProxyProvider
    }
  }

  if (cli === "codex") {
    return {
      enable: proxyApi.enableCodexProxy,
      disable: proxyApi.disableCodexProxy,
      addProvider: proxyApi.addCodexProxyProvider,
      removeProvider: proxyApi.removeCodexProxyProvider,
      activateProvider: proxyApi.activateCodexProxyProvider
    }
  }

  return null
}

function showSuccessMessage(message) {
  createMessage.success(message)
}

function showWarningMessage(message) {
  createMessage.warning(message)
}

function applyUpdateStatus(status = {}) {
  updateDialog.phase = status.phase || "idle"
  updateDialog.message = status.message || ""
  updateDialog.version = status.version || ""
  updateDialog.releaseNotes = status.releaseNotes || ""
  updateDialog.percent = Number(status.percent || 0)
  updateDialog.transferred = Number(status.transferred || 0)
  updateDialog.total = Number(status.total || 0)
  updateDialog.bytesPerSecond = Number(status.bytesPerSecond || 0)
  updateDialog.installDirectory =
    status.installDirectory || updateDialog.installDirectory || ""
  updateDialog.manual = Boolean(status.manual)

  if (updateDialog.phase === "error" && updateDialog.message) {
    console.error("[update]", updateDialog.message)
    createMessage.error(updateDialog.message)
  }

  if (updateDialog.phase === "idle" || isQuickSwitchPanel) {
    updateDialog.open = false
    return
  }

  updateDialog.open =
    updateDialog.manual ||
    ["available", "downloading", "downloaded", "error"].includes(
      updateDialog.phase
    )
}

function formatUpdateBytes(value) {
  const size = Number(value || 0)

  if (size >= 1024 * 1024) {
    return `${(size / 1024 / 1024).toFixed(1)} MB`
  }

  if (size >= 1024) {
    return `${(size / 1024).toFixed(1)} KB`
  }

  return `${size} B`
}

function firstQuickModelName(provider) {
  return (
    provider.runtimeConfig?.mainModel ||
    state.runtimeModels.find((item) => item.providerId === provider.id)?.name ||
    ""
  )
}

function formatQuickNumber(value) {
  return new Intl.NumberFormat("zh-CN", {
    maximumFractionDigits: 0
  }).format(Number(value || 0))
}

function formatQuickCost(value) {
  const cost = Number(value || 0)

  if (!cost) {
    return "$0"
  }

  return `$${cost >= 1 ? cost.toFixed(2) : cost.toFixed(6)}`
}

function formatQuickAccountDescription(account) {
  const rateLimit = account.usage?.rate_limit
  const primaryWindow = rateLimit?.primary_window
  const remaining = primaryWindow
    ? `${Math.max(0, 100 - Number(primaryWindow.used_percent || 0))}%`
    : "额度未知"

  return `${account.plan || "free"} · ${remaining}`
}

function formatQuickAccountQuotas(account) {
  const rateLimit = account.usage?.rate_limit

  if (!rateLimit) {
    return []
  }

  return [
    { key: "primary", window: rateLimit.primary_window },
    { key: "secondary", window: rateLimit.secondary_window }
  ]
    .filter((item) => item.window)
    .map((item) => {
      return {
        key: item.key,
        label: formatQuickRateWindowName(item.key, item.window),
        remaining: Math.max(0, 100 - Number(item.window.used_percent || 0)),
        reset: formatQuickResetText(item.window.reset_at)
      }
    })
}

function formatQuickRateWindowName(key, window) {
  const seconds = Number(window.limit_window_seconds || 0)

  if (seconds === 604800) {
    return key === "secondary" ? "7天额度" : "周额度"
  }

  if (seconds % 86400 === 0) {
    return `${seconds / 86400}天额度`
  }

  if (seconds % 3600 === 0) {
    return `${seconds / 3600}小时额度`
  }

  return `${seconds}秒额度`
}

function formatQuickResetText(value) {
  const timestamp = Number(value || 0)

  if (!timestamp) {
    return "重置时间未知"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(timestamp > 1e12 ? timestamp : timestamp * 1000))
}

async function showMainPanel() {
  await appApi.showMainPanel()
}

async function toggleQuickCollapsed() {
  quickCollapsed.value = !quickCollapsed.value
  await appApi.setQuickSwitchCollapsed({
    collapsed: quickCollapsed.value
  })
}

function startQuickLogoDrag(event) {
  if (event.button !== 0) {
    return
  }

  event.preventDefault()
  quickLogoDrag.active = true
  quickLogoDrag.moved = false
  quickLogoDrag.lastX = event.screenX
  quickLogoDrag.lastY = event.screenY
  quickLogoDrag.totalX = 0
  quickLogoDrag.totalY = 0
  window.addEventListener("pointermove", moveQuickLogoDrag)
  window.addEventListener("pointerup", stopQuickLogoDrag)
  window.addEventListener("pointercancel", stopQuickLogoDrag)
}

function moveQuickLogoDrag(event) {
  if (!quickLogoDrag.active) {
    return
  }

  const x = event.screenX - quickLogoDrag.lastX
  const y = event.screenY - quickLogoDrag.lastY
  quickLogoDrag.lastX = event.screenX
  quickLogoDrag.lastY = event.screenY
  quickLogoDrag.totalX += Math.abs(x)
  quickLogoDrag.totalY += Math.abs(y)

  if (quickLogoDrag.totalX + quickLogoDrag.totalY > 3) {
    quickLogoDrag.moved = true
  }

  if (x || y) {
    appApi.moveQuickSwitchBy({ x, y })
  }
}

function stopQuickLogoDrag() {
  quickLogoDrag.active = false
  window.removeEventListener("pointermove", moveQuickLogoDrag)
  window.removeEventListener("pointerup", stopQuickLogoDrag)
  window.removeEventListener("pointercancel", stopQuickLogoDrag)
}

async function handleQuickLogoClick() {
  if (quickLogoDrag.moved) {
    return
  }

  await toggleQuickCollapsed()
}

async function refreshQuickCodexAccount(item) {
  await withGlobalLoading(async () => {
    try {
      updateState(
        await accountApi.refreshCodexAccount({
          accountId: item.account.id,
          syncAuth: false
        })
      )
    } catch (error) {
      if (!isCodexAccountRefreshError(error)) {
        showErrorMessage(error)
      }
    }
  })
}

async function selectQuickItem(item) {
  if (item.type === "provider") {
    await runAction(async () => {
      const proxyApi = getProxyApi(quickActiveCli.value?.id)
      const proxyState = getProxyState(quickActiveCli.value?.id)

      if (proxyState?.enabled) {
        await proxyApi.disable()
      }

      if (quickActiveCli.value?.id === "codex") {
        await accountApi.clearCodexAccount()
      }

      return runtimeApi.switchRuntime({
        cli: quickActiveCli.value.id,
        providerId: item.provider.id,
        model: item.model
      })
    })
    return
  }

  await runAction(async () => {
    const proxyApi = getProxyApi(quickActiveCli.value?.id)
    const proxyState = getProxyState(quickActiveCli.value?.id)

    if (proxyState?.enabled) {
      await proxyApi.disable()
    }

    await runtimeApi.clearRuntime({
      cli: quickActiveCli.value.id
    })
    return accountApi.enableCodexAccount({
      accountId: item.account.id
    })
  })
}

async function clearQuickActive() {
  await runAction(async () => {
    const proxyApi = getProxyApi(quickActiveCli.value?.id)
    const proxyState = getProxyState(quickActiveCli.value?.id)

    if (proxyState?.enabled) {
      return proxyApi.disable()
    }

    if (quickActiveCli.value?.id === "codex") {
      await accountApi.clearCodexAccount()
    }

    return runtimeApi.clearRuntime({
      cli: quickActiveCli.value.id
    })
  })
}

async function refreshState() {
  await runAction(() => appApi.refresh())
}

async function saveSettings(payload) {
  const success = await runAction(() => settingsApi.saveSettings(payload))

  if (success) {
    await refreshLocalBackups(false)
    showSuccessMessage(
      state.appSettings.restartRequired
        ? "设置已保存，数据目录将在重启后生效。"
        : "设置已保存并重新刷新。"
    )
  }
}

async function checkForAppUpdates() {
  applyUpdateStatus({
    phase: "checking",
    message: "正在检查更新...",
    manual: true
  })

  try {
    applyUpdateStatus(await appApi.checkForUpdates())
  } catch (error) {
    applyUpdateStatus({
      phase: "error",
      message: error.message || String(error),
      manual: true
    })
  }
}

async function downloadAppUpdate() {
  applyUpdateStatus({
    ...updateDialog,
    phase: "downloading",
    message: `正在下载新版本 ${updateDialog.version || ""}`.trim(),
    manual: true,
    percent: 0,
    transferred: 0,
    total: 0,
    bytesPerSecond: 0
  })

  try {
    applyUpdateStatus(await appApi.downloadUpdate())
  } catch (error) {
    console.error("[update:download]", error)
    applyUpdateStatus({
      phase: "error",
      message: error.message || String(error),
      manual: true
    })
  }
}

async function installAppUpdate() {
  try {
    await appApi.installUpdate({
      installDirectory: updateDialog.installDirectory
    })
  } catch (error) {
    applyUpdateStatus({
      phase: "error",
      message: error.message || String(error),
      manual: true
    })
  }
}

async function closeUpdateDialog() {
  if (["checking", "downloading", "installing"].includes(updateDialog.phase)) {
    return
  }

  updateDialog.open = false

  try {
    await appApi.dismissUpdate()
  } catch (error) {
    showErrorMessage(error)
  }
}

async function exportDataBackup() {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.exportDataBackup()

      if (result?.canceled) {
        return
      }

      showSuccessMessage("配置数据已加密导出。")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function restoreDataBackup() {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.previewDataBackupRestore()

      if (result?.canceled) {
        return
      }

      openRestorePreview(result, "file")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function refreshLocalBackups(showMessage = true) {
  try {
    const result = await dataApi.listLocalBackups()
    localBackups.value = result.backups || []
    localBackupDirectory.value = result.directory || ""

    if (showMessage) {
      showSuccessMessage("本地备份列表已刷新。")
    }
  } catch (error) {
    showErrorMessage(error)
  }
}

async function createLocalBackup() {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.createLocalBackup()
      localBackups.value = result.backups || []
      localBackupDirectory.value =
        result.directory || localBackupDirectory.value

      if (result.state) {
        updateState(result.state)
      }

      showSuccessMessage("本地备份已创建。")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function previewLocalBackupRestore(backup) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.previewLocalBackupRestore({
        backupId: backup.id
      })
      openRestorePreview(result, "local")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function pushCloudBackup(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.pushCloudBackup(payload)

      if (result?.state) {
        updateState(result.state)
      }

      showSuccessMessage("配置数据已推送到坚果云。")
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function inspectCloudBackup(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.inspectCloudBackup(payload)

      cloudBackupView.value = result
      selectedCloudBackupPath.value =
        result.backup?.entries.find(
          (entry) => entry.path === "storage/usage-pricing.json"
        )?.path ||
        result.backup?.entries.find((entry) => entry.type === "file")?.path ||
        ""
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function pullCloudBackup(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await dataApi.previewCloudBackupRestore(payload)
      openRestorePreview(
        {
          ...result,
          cloudSync: payload
        },
        "cloud"
      )
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

function closeCloudBackupView() {
  cloudBackupView.value = null
  selectedCloudBackupPath.value = ""
}

function openRestorePreview(result, type) {
  restorePreview.value = result.preview
  restoreSource.value = {
    type,
    restoreId: result.restoreId,
    filePath: result.filePath || "",
    fileName: result.fileName || "",
    backupId: result.backupId || "",
    cloudSync: result.cloudSync || null
  }

  for (const key of Object.keys(restoreChoices)) {
    delete restoreChoices[key]
  }

  restoreCompareKey.value = ""

  for (const item of result.preview?.conflicts || []) {
    restoreChoices[item.key] =
      item.path === "storage/usage-pricing.json" ? "backup" : "current"
  }
}

function toggleRestoreCompare(item) {
  restoreCompareKey.value = item.key
}

function closeRestoreCompare() {
  restoreCompareKey.value = ""
}

function syncRestoreCompareScroll(source) {
  if (syncingRestoreCompareScroll) {
    return
  }

  const currentElement = restoreCurrentCompareCodeRef.value
  const backupElement = restoreBackupCompareCodeRef.value
  const sourceElement = source === "current" ? currentElement : backupElement
  const targetElement = source === "current" ? backupElement : currentElement

  if (!sourceElement || !targetElement) {
    return
  }

  const sourceScrollHeight =
    sourceElement.scrollHeight - sourceElement.clientHeight
  const targetScrollHeight =
    targetElement.scrollHeight - targetElement.clientHeight
  const sourceScrollWidth =
    sourceElement.scrollWidth - sourceElement.clientWidth
  const targetScrollWidth =
    targetElement.scrollWidth - targetElement.clientWidth

  syncingRestoreCompareScroll = true
  targetElement.scrollTop = sourceScrollHeight
    ? (sourceElement.scrollTop / sourceScrollHeight) * targetScrollHeight
    : sourceElement.scrollTop
  targetElement.scrollLeft = sourceScrollWidth
    ? (sourceElement.scrollLeft / sourceScrollWidth) * targetScrollWidth
    : sourceElement.scrollLeft
  requestAnimationFrame(() => {
    syncingRestoreCompareScroll = false
  })
}

function formatRestoreCompareContent(value) {
  if (value === undefined || value === null || value === "") {
    return "空内容"
  }

  return String(value)
}

function groupRestoreItems(items) {
  const groups = new Map()

  for (const item of items) {
    const groupPath = item.groupPath || item.path || "根目录"

    if (!groups.has(groupPath)) {
      groups.set(groupPath, {
        path: groupPath,
        items: []
      })
    }

    groups.get(groupPath).items.push(item)
  }

  return Array.from(groups.values()).map((group) => ({
    ...group,
    rows: createRestoreTreeRows(group.path, group.items)
  }))
}

function createRestoreTreeRows(groupPath, items) {
  const rows = []
  const dirKeys = new Set()
  const normalizedGroupPath = groupPath === "根目录" ? "" : groupPath
  const itemInfos = items.map((item) => {
    const normalizedPath = String(item.path || "").replace(/\\/g, "/")
    const relativePath =
      normalizedGroupPath &&
      normalizedPath.startsWith(`${normalizedGroupPath}/`)
        ? normalizedPath.slice(normalizedGroupPath.length + 1)
        : normalizedPath

    return {
      item,
      relativePath,
      parts: relativePath.split("/").filter(Boolean)
    }
  })
  const dirCounts = new Map()

  for (const itemInfo of itemInfos) {
    itemInfo.parts.slice(0, -1).forEach((part, index) => {
      const key = itemInfo.parts.slice(0, index + 1).join("/")

      dirCounts.set(key, (dirCounts.get(key) || 0) + 1)
    })
  }

  for (const itemInfo of itemInfos) {
    itemInfo.parts.slice(0, -1).forEach((part, index) => {
      const key = itemInfo.parts.slice(0, index + 1).join("/")

      if (dirKeys.has(key)) {
        return
      }

      dirKeys.add(key)
      rows.push({
        key: `dir:${groupPath}:${key}`,
        kind: "dir",
        name: part,
        depth: index,
        itemCount: dirCounts.get(key) || 0,
        items: itemInfos
          .filter(
            (targetInfo) =>
              targetInfo.parts.slice(0, index + 1).join("/") === key
          )
          .map((targetInfo) => targetInfo.item)
      })
    })

    rows.push({
      key: itemInfo.item.key,
      kind: "item",
      item: itemInfo.item,
      relativePath: itemInfo.relativePath,
      depth: Math.max(itemInfo.parts.length - 1, 0)
    })
  }

  return rows
}

function chooseRestoreItems(items, choice) {
  for (const item of items) {
    restoreChoices[item.key] = choice
  }
}

function createRestoreCompareRows(currentContent, backupContent) {
  const currentLines =
    formatRestoreCompareContent(currentContent).split(/\r?\n/)
  const backupLines = formatRestoreCompareContent(backupContent).split(/\r?\n/)
  const maxLength = Math.max(currentLines.length, backupLines.length)
  const rows = []

  for (let index = 0; index < maxLength; index += 1) {
    const currentText = currentLines[index]
    const backupText = backupLines[index]
    const hasCurrent = index < currentLines.length
    const hasBackup = index < backupLines.length

    if (hasCurrent && hasBackup && currentText === backupText) {
      rows.push({
        index: rows.length,
        status: "same",
        currentStatus: "same",
        backupStatus: "same",
        currentLineNumber: index + 1,
        backupLineNumber: index + 1,
        currentMarker: "",
        backupMarker: "",
        currentText,
        backupText
      })
      continue
    }

    rows.push({
      index: rows.length,
      status: "changed",
      currentStatus: hasCurrent ? "current-only" : "empty",
      backupStatus: hasBackup ? "backup-only" : "empty",
      currentLineNumber: hasCurrent ? index + 1 : "",
      backupLineNumber: hasBackup ? index + 1 : "",
      currentMarker: hasCurrent ? "当前" : "缺少",
      backupMarker: hasBackup ? "备份" : "缺少",
      currentText: hasCurrent ? currentText : "",
      backupText: hasBackup ? backupText : ""
    })
  }

  return rows
}

function closeRestorePreview(force = false) {
  if (pending.value && !force) {
    return
  }

  restorePreview.value = null
  restoreSource.value = null
  restoreCompareKey.value = ""

  for (const key of Object.keys(restoreChoices)) {
    delete restoreChoices[key]
  }
}

async function confirmRestore() {
  const source = restoreSource.value

  if (!source) {
    return
  }

  await withGlobalLoading(async () => {
    try {
      const payload = {
        restoreId: source.restoreId,
        choices: { ...restoreChoices }
      }
      const result =
        source.type === "cloud"
          ? await dataApi.pullCloudBackup({
              restoreId: source.restoreId,
              choices: { ...restoreChoices },
              cloudSync: { ...source.cloudSync }
            })
          : source.type === "local"
            ? await dataApi.restoreLocalBackup(payload)
            : await dataApi.restoreDataBackup(payload)

      updateState(result.state)
      if (result.backups) {
        localBackups.value = result.backups
      }
      if (result.directory) {
        localBackupDirectory.value = result.directory
      }
      closeRestorePreview(true)
      showSuccessMessage(
        source.type === "cloud"
          ? "已从坚果云兼容恢复配置数据。"
          : source.type === "local"
            ? "已从本地备份兼容恢复配置数据。"
            : "配置数据已兼容恢复。"
      )
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function createSkill(payload) {
  const success = await runAction(() => skillApi.createSkill(payload))

  if (success) {
    showCreateSkill.value = false
    activeView.value = "skills"
  }
}

async function importSkillsFromCli() {
  await withGlobalLoading(async () => {
    try {
      const preview = await skillApi.previewSkillsFromCli()
      const candidates = Array.isArray(preview) ? preview : preview.candidates
      const conflicts = Array.isArray(preview) ? [] : preview.conflicts

      importCandidates.value = {
        candidates,
        conflicts
      }

      if (!candidates.length && !conflicts.length) {
        showSuccessMessage("当前没有可导入的 Skill。")
        return
      }

      showImportSkills.value = true
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function importSkillFromZip() {
  try {
    const zipPath = await systemApi.selectFile({
      title: "选择 Skill zip 压缩包",
      filters: [{ name: "Zip 压缩包", extensions: ["zip"] }]
    })

    if (!zipPath) {
      return
    }

    const success = await runAction(() =>
      skillApi.importSkillFromZip({ zipPath })
    )

    if (success) {
      activeView.value = "skills"
      showSuccessMessage("Skill zip 已导入。")
    }
  } catch (error) {
    showErrorMessage(error)
  }
}

async function confirmImportSkills(payload) {
  const success = await runAction(() =>
    skillApi.importSkillsFromCli(payload)
  )

  if (success) {
    showImportSkills.value = false
    importCandidates.value = []
    activeView.value = "skills"
    showSuccessMessage("选中的 Skill 已导入并挂载到对应 CLI。")
  }
}

async function installSkill(payload) {
  await runAction(() => skillApi.installSkill(payload))
}

async function addSkillRepository(payload) {
  const success = await runAction(() =>
    skillApi.addSkillRepository(payload)
  )

  if (success) {
    activeView.value = "skills"
    showSuccessMessage("Skill 仓库已添加。")
  }
}

async function refreshSkillRepository(payload) {
  const success = await runAction(() =>
    skillApi.refreshSkillRepository(payload)
  )

  if (success) {
    showSuccessMessage("Skill 仓库已刷新。")
  }
}

async function removeSkillRepository(payload) {
  const success = await runAction(() =>
    skillApi.removeSkillRepository(payload)
  )

  if (success) {
    showSuccessMessage("Skill 仓库已删除。")
  }
}

async function installSkillFromRepository(payload) {
  const success = await runAction(() =>
    skillApi.installSkillFromRepository(payload)
  )

  if (success) {
    showSuccessMessage("仓库 Skill 已安装到本地。")
  }
}

async function uninstallSkill(payload) {
  await runAction(() => skillApi.uninstallSkill(payload))
}

async function repairSkill(payload) {
  await runAction(() => skillApi.repairSkill(payload))
}

async function addRepo(payload) {
  const success = await runAction(() => repoApi.addRepo(payload))

  if (success) {
    showAddRepo.value = false
    activeView.value = "tools"
  }
}

async function deleteSession(sessionId) {
  await runAction(() => sessionApi.deleteSession({ sessionId }))
}

async function saveProvider(payload) {
  const restoringProvider =
    state.providers.find((item) => item.id === payload.id)?.enabled === false &&
    payload.enabled === true
  const success = await runAction(() => providerApi.saveProvider(payload))

  if (success) {
    showSuccessMessage(
      payload.enabled === false
        ? "Provider 已禁用。"
        : restoringProvider
          ? "Provider 已恢复。"
          : "Provider 已保存。"
    )
  }
}

async function saveRule(payload) {
  const success = await runAction(() => ruleApi.saveRule(payload))

  if (success) {
    showSuccessMessage("Prompt 已保存。")
  }
}

async function deleteRule(ruleId) {
  const success = await runAction(() => ruleApi.deleteRule({ ruleId }))

  if (success) {
    showSuccessMessage("Prompt 已删除。")
  }
}

async function enableRule(payload) {
  const success = await runAction(() => ruleApi.enableRule(payload))

  if (success) {
    showSuccessMessage("Prompt 已启用并同步到全局文件。")
  }
}

async function toggleRule(payload) {
  const success = await runAction(() => ruleApi.toggleRule(payload))

  if (success && payload.enabled === false) {
    showSuccessMessage("Prompt 已取消启用。")
  }
}

async function importRule(payload) {
  const success = await runAction(() =>
    ruleApi.importGlobalRule(payload)
  )

  if (success) {
    showSuccessMessage("已导入当前全局 Prompt。")
  }
}

async function resolveRuleImportConflict(payload) {
  const success = await runAction(() =>
    ruleApi.resolveRuleImportConflict(payload)
  )

  if (success) {
    if (payload.source === "manager") {
      showSuccessMessage("已保留管理器版本。")
    } else {
      showSuccessMessage("已使用全局版本更新相似 Prompt。")
    }
  }
}

async function resolveRuleDrift(payload) {
  const success = await runAction(() =>
    ruleApi.resolveRuleDrift(payload)
  )

  if (success) {
    showSuccessMessage("Prompt Drift 已处理。")
  }
}

async function deleteProvider(providerId) {
  const success = await runAction(() =>
    providerApi.deleteProvider({ providerId })
  )

  if (success) {
    showSuccessMessage("Provider 已删除。")
  }
}

async function startCodexOfficialLogin(payload) {
  const success = await runAction(() =>
    accountApi.startCodexOfficialLogin(payload)
  )

  if (success) {
    showWarningMessage("已打开浏览器，请完成 Codex 官方登录。")
  }
}

async function cancelCodexOfficialLogin() {
  await runAction(() => accountApi.cancelCodexOfficialLogin())
}

async function importCodexAuthJson(payload) {
  const success = await runAction(() =>
    accountApi.importCodexAuthJson(payload)
  )

  if (success) {
    showSuccessMessage("Codex 登录 JSON 已导入。")
  }
}

async function enableCodexAccount(payload) {
  const shouldDisableCodexProxy = state.codexProxyState.enabled
  const success = await runAction(async () => {
    if (shouldDisableCodexProxy) {
      await proxyApi.disableCodexProxy()
    }

    return accountApi.enableCodexAccount(payload)
  })

  if (success) {
    showSuccessMessage(
      shouldDisableCodexProxy
        ? "Codex 代理接管已关闭，官方账号已启用。"
        : "Codex 官方账号已启用。"
    )
  }
}

async function clearCodexAccount() {
  const success = await runAction(() => accountApi.clearCodexAccount())

  if (success) {
    showSuccessMessage("Codex 官方账号已取消启用。")
  }
}

async function deleteCodexAccount(payload) {
  const success = await runAction(() =>
    accountApi.deleteCodexAccount(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号已删除。")
  }
}

async function disableCodexAccount(payload) {
  const success = await runAction(() =>
    accountApi.disableCodexAccount(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号已禁用。")
  }
}

async function restoreCodexAccount(payload) {
  const success = await runAction(() =>
    accountApi.restoreCodexAccount(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号已恢复。")
  }
}

async function refreshCodexAccount(payload) {
  const { onSettled, showSuccess, ...input } = payload

  try {
    updateState(await accountApi.refreshCodexAccount(input))

    if (showSuccess !== false) {
      showSuccessMessage("Codex 官方账号额度已刷新。")
    }
  } catch (error) {
    if (!isCodexAccountRefreshError(error)) {
      showErrorMessage(error)
    }
  } finally {
    if (onSettled) {
      onSettled()
    }
  }
}

async function refreshCodexAccounts() {
  if (!state.codexAccounts.length) {
    return
  }

  await Promise.all(
    state.codexAccounts.map(async (account) => {
      if (account.disabled) {
        return
      }

      try {
        updateState(
          await accountApi.refreshCodexAccount({
            accountId: account.id,
            syncAuth: false
          })
        )
      } catch (error) {
        if (!isCodexAccountRefreshError(error)) {
          showErrorMessage(error)
        }
      }
    })
  )
}

async function updateCodexAccountProxy(payload) {
  const success = await runAction(() =>
    accountApi.updateCodexAccountProxy(payload)
  )

  if (success) {
    showSuccessMessage("Codex 官方账号代理已保存。")
  }
}

async function enableCodexProxy(payload) {
  const success = await runAction(() =>
    proxyApi.enableCodexProxy(payload)
  )

  if (success) {
    showSuccessMessage("Codex 代理接管已开启。")
  }
}

async function enableClaudeProxy(payload) {
  const success = await runAction(() =>
    proxyApi.enableClaudeProxy(payload)
  )

  if (success) {
    showSuccessMessage("Claude 代理接管已开启。")
  }
}

async function disableClaudeProxy() {
  const success = await runAction(() => proxyApi.disableClaudeProxy())

  if (success) {
    showSuccessMessage("Claude 代理接管已关闭。")
  }
}

async function addClaudeProxyProvider(payload) {
  const success = await runAction(() =>
    proxyApi.addClaudeProxyProvider(payload)
  )

  if (success) {
    showSuccessMessage("Provider 已加入代理接管列表。")
  }
}

async function removeClaudeProxyProvider(payload) {
  const success = await runAction(() =>
    proxyApi.removeClaudeProxyProvider(payload)
  )

  if (success) {
    showSuccessMessage("Provider 已移出代理接管列表。")
  }
}

async function activateClaudeProxyProvider(payload) {
  const shouldEnableProxy = !state.claudeProxyState.enabled
  const success = await runAction(async () => {
    const nextState =
      await proxyApi.activateClaudeProxyProvider(payload)

    if (shouldEnableProxy) {
      return proxyApi.enableClaudeProxy({})
    }

    return nextState
  })

  if (success) {
    showSuccessMessage(
      shouldEnableProxy ? "Claude 代理接管已开启。" : "代理接管目标已切换。"
    )
  }
}

async function disableCodexProxy() {
  const success = await runAction(() => proxyApi.disableCodexProxy())

  if (success) {
    showSuccessMessage("Codex 代理接管已关闭。")
  }
}

async function addCodexProxyProvider(payload) {
  const success = await runAction(() =>
    proxyApi.addCodexProxyProvider(payload)
  )

  if (success) {
    showSuccessMessage("Provider 已加入代理接管列表。")
  }
}

async function removeCodexProxyProvider(payload) {
  const success = await runAction(() =>
    proxyApi.removeCodexProxyProvider(payload)
  )

  if (success) {
    showSuccessMessage("Provider 已移出代理接管列表。")
  }
}

async function activateCodexProxyProvider(payload) {
  const shouldEnableProxy = !state.codexProxyState.enabled
  const success = await runAction(async () => {
    const nextState = await proxyApi.activateCodexProxyProvider(payload)

    if (shouldEnableProxy) {
      return proxyApi.enableCodexProxy({})
    }

    return nextState
  })

  if (success) {
    showSuccessMessage(
      shouldEnableProxy ? "Codex 代理接管已开启。" : "代理接管目标已切换。"
    )
  }
}

async function saveCodexProxyAccountModel(payload) {
  const success = await runAction(() =>
    proxyApi.saveCodexProxyAccountModel(payload)
  )

  if (success) {
    showSuccessMessage("官方账号接管模型已保存。")
  }
}

async function launchCodexProviderInstance(payload) {
  await withGlobalLoading(async () => {
    try {
      const result = await runtimeApi.launchCodexProviderInstance(payload)

      showSuccessMessage(`Codex 实例已启动：${result.providerName}`)
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function saveRuntimeModel(payload) {
  const success = await runAction(() =>
    runtimeApi.saveRuntimeModel(payload)
  )

  if (success) {
    showSuccessMessage("模型已保存。")
  }
}

async function switchRuntime(payload) {
  const proxyApi = getProxyApi(payload.cli)
  const proxyState = getProxyState(payload.cli)
  const shouldDisableProxy = Boolean(proxyState?.enabled)
  const success = await runAction(async () => {
    if (shouldDisableProxy) {
      await proxyApi.disable()
    }

    return runtimeApi.switchRuntime(payload)
  })

  if (success) {
    showSuccessMessage(
      shouldDisableProxy
        ? `${payload.cli === "claude" ? "Claude" : "Codex"} 代理接管已关闭，Runtime Profile 已切换。`
        : "Runtime Profile 已切换。"
    )
  }
}

async function clearRuntime(payload) {
  const success = await runAction(() => runtimeApi.clearRuntime(payload))

  if (success) {
    showSuccessMessage("Runtime Profile 已取消使用。")
  }
}

async function resolveRuntimeDrift(payload) {
  const success = await runAction(() =>
    runtimeApi.resolveRuntimeDrift(payload)
  )

  if (success) {
    showSuccessMessage("Runtime 配置差异已处理。")
  }
}

async function openPath(targetPath) {
  if (!targetPath) {
    return
  }

  await withGlobalLoading(async () => {
    try {
      await systemApi.openPath({ targetPath })
    } catch (error) {
      showErrorMessage(error)
    }
  })
}

async function submitCloseAction(action) {
  showCloseConfirm.value = false

  try {
    await appApi.handleCloseAction({
      action,
      remember: closeRemember.value
    })
  } catch (error) {
    showErrorMessage(error)
  }
}

async function quitApp() {
  try {
    await appApi.handleCloseAction({
      action: "quit",
      remember: false
    })
  } catch (error) {
    showErrorMessage(error)
  }
}

async function uninstallWithoutTrace() {
  try {
    await appApi.uninstallWithoutTrace()
  } catch (error) {
    showErrorMessage(error)
  }
}

onMounted(() => {
  if (isQuickSwitchPanel) {
    document.documentElement.classList.add("quick-switch-html")
    document.body.classList.add("quick-switch-body")
  }

  bootstrap()

  // 视图切换按需加载
  watch(activeView, async (view) => {
    try {
      if (view === 'sessions') {
        updateState(await appApi.ensureSessionsReady())
      } else if (view === 'tools') {
        updateState(await appApi.ensureToolsReady())
      } else if (view === 'skills') {
        updateState(await appApi.ensureSkillsReady())
      }
    } catch (error) {
      showErrorMessage(error)
    }
  })

  unsubscribeUpdate = appApi.onUpdateStatus(applyUpdateStatus)
  appApi
    .getUpdateStatus()
    .then(applyUpdateStatus)
    .catch(() => {})
  unsubscribeClose = appApi.onCloseRequested(() => {
    closeRemember.value = false
    showCloseConfirm.value = true
  })
})

onBeforeUnmount(() => {
  stopQuickLogoDrag()

  if (typeof unsubscribe === "function") {
    unsubscribe()
  }

  if (typeof unsubscribeClose === "function") {
    unsubscribeClose()
  }

  if (typeof unsubscribeUpdate === "function") {
    unsubscribeUpdate()
  }

  if (isQuickSwitchPanel) {
    document.documentElement.classList.remove("quick-switch-html")
    document.body.classList.remove("quick-switch-body")
  }
})
</script>

<style scoped lang="less">
:global(html.quick-switch-html),
:global(html.quick-switch-html body),
:global(html.quick-switch-html #app) {
  background: transparent;
}

:global(html.quick-switch-html .app-message) {
  top: 34px;
  right: 8px;
  left: 8px;
  width: auto;
  gap: 5px;
  transform: none;
}

:global(html.quick-switch-html .app-message__item) {
  padding: 6px 8px;
  border-radius: 6px;
  box-shadow: 0 6px 14px rgba(34, 56, 83, 0.12);
  font-size: 12px;
  line-height: 1.25;
}

.quick-switch-panel {
  display: flex;
  height: 100vh;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid #b8cce5;
  background: #eef4fb;
  color: #101828;

  &__header {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 34px;
    padding: 0 8px 0 10px;
    border-bottom: 1px solid #d7e3f1;
    background: #fbfdff;
    -webkit-app-region: drag;
  }

  &__title {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 7px;
  }

  &__dot {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 999px;
    background: #18a058;
    box-shadow: 0 0 0 3px #e3f5ec;
  }

  &__title strong {
    flex: none;
    font-size: 13px;
    line-height: 1;
  }

  &__title small {
    overflow: hidden;
    min-width: 0;
    color: #667085;
    font-size: 12px;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 4px;
    -webkit-app-region: no-drag;
  }

  &__icon-button {
    display: inline-flex;
    width: 26px;
    height: 26px;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: #2d6cdf;
    cursor: pointer;
  }

  &__icon-button:hover {
    border-color: #bdd6f7;
    background: #eef6ff;
  }

  &__logo-button {
    display: inline-flex;
    width: 44px;
    height: 44px;
    align-items: center;
    justify-content: center;
    border: 0;
    background: transparent;
    cursor: grab;
    touch-action: none;
    -webkit-app-region: no-drag;
  }

  &__logo-button:active {
    cursor: grabbing;
  }

  &__logo-scene {
    width: 42px;
    height: 42px;
    overflow: visible;
  }

  &__logo-shadow {
    animation: quick-switch-logo-shadow 2.4s ease-in-out infinite;
    fill: rgba(16, 24, 40, 0.2);
    transform-origin: 22px 36px;
  }

  &__logo-mascot {
    animation: quick-switch-logo-float 2.4s ease-in-out infinite;
    transform-origin: 22px 26px;
  }

  &__logo-orbit {
    animation: quick-switch-logo-pulse 2.4s ease-in-out infinite;
    fill: rgba(255, 255, 255, 0.84);
    stroke: url("#quick-switch-logo-ring");
    stroke-width: 1.8;
    transform-origin: 22px 21px;
  }

  &__logo-core {
    animation: quick-switch-logo-breathe 2.4s ease-in-out infinite;
    transform-origin: 22px 21px;
  }

  &__logo-scan {
    animation: quick-switch-logo-scan 1.8s linear infinite;
    fill: none;
    stroke: #ffffff;
    stroke-linecap: round;
    stroke-width: 2.2;
    transform-origin: 22px 22px;
  }

  &__logo-eye {
    animation: quick-switch-logo-blink 3.6s ease-in-out infinite;
    fill: #18a058;
    transform-origin: center;
  }

  &__logo-sparks {
    animation: quick-switch-logo-sparkle 2.2s ease-in-out infinite;
    fill: #ffb84d;
    transform-origin: 22px 22px;
  }

  &__cli-tabs {
    display: flex;
    flex: none;
    gap: 4px;
    padding: 6px 7px;
    background: #fbfdff;
  }

  &__cli-tab {
    height: 24px;
    flex: 1;
    border: 1px solid #dce6f2;
    border-radius: 6px;
    background: #f0f4f9;
    color: #516070;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
  }

  &__cli-tab--active {
    border-color: #1677ff;
    background: #1677ff;
    color: #ffffff;
  }

  &__usage {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
    padding: 0 7px 6px;
  }

  &__hero {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 34px;
    padding: 0 8px;
    border: 1px solid #d6e4f3;
    border-radius: 7px;
    background: #ffffff;
  }

  &__hero-copy {
    display: grid;
    min-width: 0;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 2px 8px;
  }

  &__hero-copy span {
    grid-row: 1 / 3;
    align-self: center;
    padding: 2px 6px;
    border-radius: 5px;
    background: #eef6ff;
    color: #1677ff;
    font-size: 10px;
    font-weight: 800;
  }

  &__hero-copy strong,
  &__hero-copy small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__hero-copy strong {
    font-size: 12px;
    line-height: 1.15;
  }

  &__hero-copy small {
    color: #667085;
    font-size: 11px;
    line-height: 1.15;
  }

  &__manage-button {
    display: inline-flex;
    height: 23px;
    flex: none;
    align-items: center;
    justify-content: center;
    padding: 0 10px;
    border: 1px solid #b9d4f4;
    border-radius: 6px;
    background: #f7fbff;
    color: #1769c2;
    cursor: pointer;
    font-size: 11px;
    font-weight: 800;
  }

  &__manage-button:hover {
    border-color: #7fb7f5;
    background: #eaf5ff;
  }

  &__metrics {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 5px;
  }

  &__metric {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    height: 28px;
    padding: 0 7px;
    border: 1px solid #d8e6f4;
    border-radius: 7px;
    background: #ffffff;
  }

  &__metric span {
    color: #667085;
    font-size: 10px;
    font-weight: 700;
  }

  &__metric strong {
    overflow: hidden;
    color: #101828;
    font-size: 12px;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__summary-row {
    display: grid;
    min-height: 0;
    flex: 1;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.9fr);
    gap: 5px;
  }

  &__usage-panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
    padding: 6px 7px;
    border: 1px solid #d8e6f4;
    border-radius: 7px;
    background: #ffffff;
  }

  &__usage-panel--providers {
    min-width: 0;
  }

  &__usage-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  &__usage-head strong {
    color: #101828;
    font-size: 11px;
  }

  &__usage-head span {
    color: #667085;
    font-size: 10px;
    font-weight: 700;
  }

  &__bars {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 4px;
    height: 48px;
    align-items: end;
  }

  &__bar {
    display: flex;
    min-width: 0;
    height: 100%;
    flex-direction: column;
    justify-content: flex-end;
    gap: 4px;
  }

  &__bar-fill {
    display: block;
    min-height: 6px;
    border-radius: 4px 4px 2px 2px;
    background: #1677ff;
  }

  &__bar small {
    overflow: hidden;
    color: #667085;
    font-size: 9px;
    line-height: 1;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__provider-bars {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 6px;
    overflow: hidden;
  }

  &__provider-bar {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  &__provider-bar-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  &__provider-bar-head strong,
  &__provider-bar-head span {
    overflow: hidden;
    font-size: 10px;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__provider-bar-head strong {
    color: #101828;
  }

  &__provider-bar-head span {
    flex: none;
    color: #667085;
    font-weight: 700;
  }

  &__provider-track {
    height: 6px;
    overflow: hidden;
    border-radius: 999px;
    background: #edf2f7;
  }

  &__provider-fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: #18a058;
  }

  &__list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    overflow-x: hidden;
    overflow-y: auto;
    padding: 0 7px 7px;
  }

  &__manager-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 9px;
    border: 1px solid #d8e6f4;
    border-radius: 7px;
    background: #ffffff;
  }

  &__manager-head div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  &__manager-head strong,
  &__manager-head span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__manager-head strong {
    color: #101828;
    font-size: 12px;
    line-height: 1.2;
  }

  &__manager-head span {
    color: #667085;
    font-size: 11px;
    line-height: 1.2;
  }

  &__item {
    display: flex;
    min-height: 50px;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 9px;
    border: 1px solid #dfe8f3;
    border-radius: 7px;
    background: #ffffff;
    color: #101828;
    text-align: left;
    transition:
      border-color 0.18s ease,
      background 0.18s ease,
      box-shadow 0.18s ease,
      transform 0.18s ease;
  }

  &__item:hover {
    border-color: #9dc9ff;
    background: #fbfdff;
    box-shadow: 0 7px 18px rgba(22, 119, 255, 0.12);
    transform: translateY(-1px);
  }

  &__item--active {
    border-color: #56a7ff;
    background: #eef7ff;
    box-shadow: inset 3px 0 0 #1677ff;
  }

  &__item--account {
    min-height: 68px;
  }

  &__item-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 5px;
  }

  &__item-icon-button {
    display: inline-flex;
    width: 26px;
    height: 26px;
    flex: none;
    align-items: center;
    justify-content: center;
    border: 1px solid #d8e7f7;
    border-radius: 6px;
    background: #ffffff;
    color: #2d6cdf;
    cursor: pointer;
  }

  &__item-icon-button:hover {
    border-color: #9dc9ff;
    background: #eef6ff;
  }

  &__item-action {
    display: inline-flex;
    height: 26px;
    flex: none;
    align-items: center;
    justify-content: center;
    padding: 0 10px;
    border: 1px solid #9dc9ff;
    border-radius: 6px;
    background: #f0f7ff;
    color: #1677ff;
    cursor: pointer;
    font-size: 12px;
    font-weight: 700;
  }

  &__item-action:hover {
    border-color: #56a7ff;
    background: #e4f1ff;
  }

  &__item-action:disabled {
    border-color: #d0d5dd;
    background: #f3f4f6;
    color: #98a2b3;
    cursor: not-allowed;
  }

  &__item-action--danger {
    border-color: #ffc7be;
    background: #fff6f4;
    color: #b42318;
  }

  &__item-action--danger:hover {
    border-color: #ffafa3;
    background: #fff0ee;
  }

  &__item-copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  &__item-copy strong,
  &__item-copy small {
    overflow: hidden;
    max-width: 230px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__item-copy strong {
    font-size: 13px;
    line-height: 1.25;
  }

  &__item-copy small {
    color: #667085;
    font-size: 12px;
  }

  &__quota-list {
    display: flex;
    min-width: 0;
    gap: 5px;
    margin-top: 1px;
  }

  &__quota-item {
    display: inline-flex;
    height: 18px;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border: 1px solid #d8e7f7;
    border-radius: 5px;
    background: #f7fbff;
    color: #49627d;
    font-size: 11px;
    line-height: 18px;
    white-space: nowrap;
  }

  &__quota-item strong {
    color: #1677ff;
    font-size: 11px;
    line-height: 18px;
  }

  &__empty {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    color: #667085;
  }

  &--collapsed {
    border: 0;
    background: transparent;
  }

  &--collapsed &__header {
    height: 100vh;
    justify-content: center;
    padding: 0;
    border-bottom: 0;
    background: transparent;
  }
}

@keyframes quick-switch-logo-float {
  0% {
    transform: translateY(0);
  }

  25% {
    transform: translateY(-3px);
  }

  50% {
    transform: translateY(1px);
  }

  75% {
    transform: translateY(-2px);
  }

  100% {
    transform: translateY(0);
  }
}

@keyframes quick-switch-logo-shadow {
  0%,
  100% {
    opacity: 0.42;
    transform: scaleX(0.86);
  }

  50% {
    opacity: 0.22;
    transform: scaleX(1.08);
  }
}

@keyframes quick-switch-logo-pulse {
  0%,
  100% {
    opacity: 0.88;
    transform: scale(0.95);
  }

  50% {
    opacity: 1;
    transform: scale(1.04);
  }
}

@keyframes quick-switch-logo-breathe {
  0%,
  100% {
    transform: scale(0.94);
  }

  50% {
    transform: scale(1.03);
  }
}

@keyframes quick-switch-logo-scan {
  0% {
    opacity: 0.2;
    transform: rotate(0deg);
  }

  45% {
    opacity: 0.78;
  }

  100% {
    opacity: 0.2;
    transform: rotate(360deg);
  }
}

@keyframes quick-switch-logo-blink {
  0%,
  88%,
  100% {
    transform: scaleY(1);
  }

  92%,
  96% {
    transform: scaleY(0.18);
  }
}

@keyframes quick-switch-logo-sparkle {
  0%,
  100% {
    opacity: 0.28;
    transform: rotate(0deg) scale(0.9);
  }

  50% {
    opacity: 0.95;
    transform: rotate(18deg) scale(1.08);
  }
}

.app-shell {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  height: 100vh;
  min-height: 0;

  &__main {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    padding: 18px;
    gap: 14px;
    background: var(--color-page);
  }

  &__content {
    flex: 1;
    min-height: 0;
    // overflow: auto;
    padding-right: 6px;
  }

  &__content--locked {
    overflow: hidden;
  }

  &__placeholder {
    display: grid;
    min-height: 520px;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    padding: 32px;
    text-align: center;
  }

  &__placeholder h1 {
    margin: 0 0 12px;
    font-size: 2rem;
  }

  &__placeholder p {
    max-width: 680px;
    margin: 0 0 18px;
    color: var(--color-text-muted);
    line-height: 1.7;
  }
}

.app-logs {
  display: flex;
  min-height: 0;
  flex-direction: column;
  gap: 14px;

  &__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 18px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__header span {
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
  }

  &__header h1 {
    margin: 4px 0;
    color: var(--color-primary);
    font-size: 1.35rem;
  }

  &__header p {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.86rem;
  }

  &__actions {
    display: flex;
    gap: 8px;
  }

  &__actions button {
    display: inline-flex;
    height: 36px;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
    font-weight: 700;
  }

  &__filters {
    display: flex;
    align-items: end;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__filters label {
    display: grid;
    gap: 6px;
  }

  &__filters label span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__filters select {
    width: 180px;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-primary);
    font-weight: 700;
  }

  &__filters strong {
    margin-left: auto;
    color: var(--color-text-muted);
    font-size: 0.84rem;
  }

  &__list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  &__item {
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__item--error {
    border-color: #f3b7b7;
    background: #fff7f7;
  }

  &__item-head,
  &__meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__item-head strong {
    color: var(--color-primary);
    font-size: 0.95rem;
  }

  &__item-head span {
    color: var(--color-text-muted);
    font-size: 0.82rem;
    font-weight: 700;
  }

  &__item p {
    margin: 8px 0 0;
    color: #b42318;
    font-size: 0.86rem;
  }

  &__meta {
    justify-content: flex-start;
    flex-wrap: wrap;
    margin-top: 8px;
    color: var(--color-text-soft);
    font-size: 0.78rem;
  }

  &__item pre {
    overflow: auto;
    max-height: 220px;
    margin: 10px 0 0;
    padding: 10px;
    border-radius: 8px;
    background: #f5f7fa;
    color: #2c3b4f;
    font-size: 0.78rem;
    line-height: 1.55;
  }

  &__pagination {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    font-size: 0.84rem;
    font-weight: 700;
  }

  &__pagination select,
  &__pagination button {
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
    font-weight: 700;
  }

  &__pagination button:disabled {
    cursor: not-allowed;
    opacity: 0.48;
  }

  &__empty {
    display: grid;
    min-height: 300px;
    place-items: center;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
  }
}

.status-button {
  height: 40px;
  padding: 0 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #fbfcfd;
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;

  &:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }
}

.cloud-backup-modal {
  display: flex;
  height: min(680px, calc(100vh - 180px));
  min-height: 0;
  flex-direction: column;
  gap: 12px;

  &__summary {
    display: flex;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 0.84rem;
    font-weight: 700;
  }

  &__summary span {
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--color-primary-soft);
  }

  &__body {
    display: grid;
    flex: 1;
    min-height: 0;
    grid-template-columns: 300px minmax(0, 1fr);
    gap: 12px;
  }

  &__list {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding-right: 4px;
  }

  &__entry {
    display: flex;
    min-height: 58px;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 3px;
    padding: 9px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }

  &__entry--active {
    border-color: #8eb6d9;
    background: #eef6ff;
  }

  &__entry strong {
    font-size: 0.82rem;
  }

  &__entry span {
    width: 100%;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.75rem;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__content {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
  }

  &__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 12px;
    border-bottom: 1px solid var(--color-line);
    background: #f7fafc;
  }

  &__head div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  &__head strong {
    color: var(--color-text);
    font-size: 0.88rem;
  }

  &__head span {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.35;
    word-break: break-all;
  }

  &__head small {
    flex: none;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
  }

  &__content pre {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: 12px;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.76rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }
}

.restore-preview-modal {
  display: flex;
  height: min(640px, calc(100vh - 180px));
  min-height: 0;
  flex-direction: column;
  gap: 12px;

  &--compare {
    height: min(680px, calc(100vh - 180px));
  }

  &__summary {
    display: flex;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 0.84rem;
    font-weight: 700;
  }

  &__summary-pill {
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--color-primary-soft);
  }

  &__notice {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid #d8e2ec;
    border-radius: 8px;
    background: #f6f9fc;
    color: var(--color-text-muted);
    font-size: 0.84rem;
    line-height: 1.6;
  }

  &__body {
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    gap: 12px;
    overflow: auto;
    padding-right: 4px;
  }

  &__section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &__section-title {
    margin: 0;
    color: var(--color-text);
    font-size: 0.94rem;
  }

  &__list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &__group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  &__group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 10px;
    border: 1px solid #d8e2ec;
    border-radius: 8px;
    background: #f6f9fc;
  }

  &__group-head strong,
  &__group-head span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__group-head strong {
    min-width: 0;
    color: var(--color-text);
    font-size: 0.82rem;
  }

  &__group-head span {
    flex: none;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
  }

  &__group-actions,
  &__directory-actions {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 6px;
  }

  &__bulk-button {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.72rem;
    font-weight: 700;
  }

  &__bulk-button:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }

  &__tree {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  &__tree-folder {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 26px;
    border-left: 2px solid #b7c7d9;
    color: var(--color-text);
    font-size: 0.8rem;
  }

  &__tree-folder strong {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__tree-folder span {
    flex: none;
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
  }

  &__tree-item {
    position: relative;
  }

  &__tree-item::before {
    position: absolute;
    top: -7px;
    bottom: 12px;
    left: -10px;
    width: 8px;
    border-bottom: 1px solid #b7c7d9;
    border-left: 1px solid #b7c7d9;
    content: "";
  }

  &__item,
  &__conflict {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__item-name {
    color: var(--color-text);
    font-size: 0.88rem;
  }

  &__item-path {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.45;
    word-break: break-all;
  }

  &__conflict-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 4px;
  }

  &__conflict-head div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  &__compare-button {
    flex: none;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__compare-button:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }

  &__choice {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &__choice input {
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: var(--color-primary);
  }

  &__choice-text {
    line-height: 1.35;
  }

  &__compare {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    margin-top: 4px;
  }

  &__compare--dialog {
    flex: 1;
    min-height: 0;
    margin-top: 0;
  }

  &__compare-panel {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    gap: 6px;
  }

  &__compare-panel strong {
    color: var(--color-text);
    font-size: 0.78rem;
  }

  &__compare-summary {
    grid-column: 1 / -1;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__compare-code {
    flex: 1;
    max-height: 260px;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.74rem;
    line-height: 1.55;
  }

  &__compare--dialog &__compare-code {
    max-height: none;
  }

  &__compare-line {
    display: grid;
    grid-template-columns: 38px 54px minmax(0, 1fr);
    gap: 6px;
    min-height: 22px;
    padding: 2px 8px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  &__compare-line--current-only {
    background: #fff2f0;
  }

  &__compare-line--backup-only {
    background: #eff8ff;
  }

  &__compare-line--empty {
    background: #f8fafc;
    color: var(--color-text-soft);
  }

  &__compare-number {
    color: var(--color-text-soft);
    text-align: right;
    user-select: none;
  }

  &__compare-marker {
    color: var(--color-text-muted);
    font-weight: 700;
    user-select: none;
  }

  &__compare-text {
    min-width: 0;
  }

  &__empty {
    display: grid;
    min-height: 120px;
    place-items: center;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 0.9rem;
    font-weight: 700;
  }

  &__actions {
    display: flex;
    flex: none;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 4px;
  }

  &__primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  &__primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }
}

.update-modal {
  position: fixed;
  inset: 0;
  z-index: 82;
  display: grid;
  place-items: center;
  padding: 24px;

  &__overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.28);
    backdrop-filter: blur(2px);
  }

  &__panel {
    position: relative;
    width: 560px;
    max-height: calc(100vh - 48px);
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 18px 48px rgba(15, 23, 42, 0.2);
  }

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 18px 18px 12px;
    border-bottom: 1px solid var(--color-line);
  }

  &__header span {
    display: block;
    margin-bottom: 5px;
    color: var(--color-text-soft);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    line-height: 1;
    text-transform: uppercase;
  }

  &__header h2 {
    margin: 0;
    color: var(--color-text);
    font-size: 1.05rem;
    line-height: 1.25;
  }

  &__icon-button {
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &__icon-button:hover {
    border-color: #c8d2df;
    background: #f7f9fc;
    color: var(--color-text);
  }

  &__icon-button:disabled {
    cursor: default;
    opacity: 0.55;
  }

  &__body {
    display: flex;
    gap: 14px;
    padding: 18px;
  }

  &__mark {
    display: grid;
    width: 44px;
    height: 44px;
    flex: 0 0 auto;
    place-items: center;
    border: 1px solid #b7d9f6;
    border-radius: 8px;
    background: #e8f4ff;
    color: #0b78d0;
  }

  &__copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 8px;
    padding-top: 2px;
  }

  &__copy strong {
    color: var(--color-primary);
    font-size: 1rem;
    line-height: 1.35;
  }

  &__copy span {
    color: var(--color-text-muted);
    font-size: 0.86rem;
    line-height: 1.6;
  }

  &__progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 18px 16px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #f8fbff;
  }

  &__progress-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-weight: 700;
  }

  &__progress-head strong {
    color: var(--color-primary);
  }

  &__progress-track {
    height: 8px;
    overflow: hidden;
    border-radius: 999px;
    background: #e6edf5;
  }

  &__progress-bar {
    height: 100%;
    border-radius: inherit;
    background: var(--color-primary);
  }

  &__notes {
    max-height: 160px;
    margin: 0 18px 16px;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #f8fafc;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.76rem;
    line-height: 1.55;
    padding: 12px;
    white-space: pre-wrap;
  }

  &__footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 0 18px 18px;
  }

  &__button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    min-width: 88px;
    height: 34px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    font-size: 0.86rem;
    font-weight: 700;
    cursor: pointer;
  }

  &__button:hover {
    border-color: var(--color-primary);
    background: #f7f9fc;
  }

  &__button:disabled {
    cursor: default;
    opacity: 0.68;
  }

  &__button--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  &__button--primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }
}

.close-confirm {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: grid;
  place-items: center;
  padding: 24px;

  &__overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.28);
    backdrop-filter: blur(2px);
  }

  &__panel {
    position: relative;
    width: 520px;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 18px 48px rgba(15, 23, 42, 0.2);
  }

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 18px 18px 12px;
    border-bottom: 1px solid var(--color-line);
  }

  &__header span {
    display: block;
    margin-bottom: 5px;
    color: var(--color-text-soft);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    line-height: 1;
    text-transform: uppercase;
  }

  &__header h2 {
    margin: 0;
    color: var(--color-text);
    font-size: 1.05rem;
    line-height: 1.25;
  }

  &__icon-button {
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &__icon-button:hover {
    border-color: #c8d2df;
    background: #f7f9fc;
    color: var(--color-text);
  }

  &__body {
    display: flex;
    gap: 14px;
    padding: 18px;
  }

  &__mark {
    display: grid;
    width: 44px;
    height: 44px;
    flex: 0 0 auto;
    place-items: center;
    border: 1px solid #b7d9f6;
    border-radius: 8px;
    background: #e8f4ff;
    color: #0b78d0;
  }

  &__copy {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding-top: 2px;
  }

  &__copy strong {
    color: var(--color-primary);
    font-size: 1rem;
    line-height: 1.35;
  }

  &__copy span {
    color: var(--color-text-muted);
    font-size: 0.84rem;
    line-height: 1.6;
  }

  &__footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 12px 18px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  &__remember {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 600;
  }

  &__remember input {
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: var(--color-primary);
  }

  &__actions {
    display: flex;
    gap: 8px;
  }

  &__button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 700;
  }

  &__button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  &__button--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  &__button--primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }
}
</style>

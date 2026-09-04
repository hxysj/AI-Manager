<template>
  <section class="providers-view">
    <div v-if="viewMode === 'list'" class="providers-view__list-shell">
      <header class="providers-view__toolbar">
        <div class="providers-view__cli-tabs">
          <button
            v-for="cli in visibleCliTargets"
            :key="cli.id"
            :class="[
              'providers-view__cli-tab',
              { 'providers-view__cli-tab--active': activeCli === cli.id }
            ]"
            type="button"
            @click="selectCli(cli.id)"
          >
            <AiIcon
              v-if="cli.icon"
              class="providers-view__cli-icon"
              :name="cli.icon"
              :alt="`${cli.name} 图标`"
            />
            {{ cli.name }}
          </button>
        </div>

        <section v-if="showRuntimeWarning" class="providers-view__runtime">
          <strong>{{ activeCliName }} Runtime 配置不一致</strong>
        </section>

        <div class="providers-view__toolbar-actions">
          <div v-if="activeProxyEnabled" class="providers-view-proxy-tabs">
            <button
              :class="[
                'providers-view-proxy-tab',
                { 'providers-view-proxy-tab-active': proxyTab === 'proxy' }
              ]"
              type="button"
              @click="proxyTab = 'proxy'"
            >
              接管池
            </button>
            <button
              :class="[
                'providers-view-proxy-tab',
                {
                  'providers-view-proxy-tab-active': proxyTab === 'providers'
                }
              ]"
              type="button"
              @click="proxyTab = 'providers'"
            >
              Provider 列表
            </button>
          </div>
          <label
            v-if="activeProxyState"
            :class="[
              'providers-view-proxy-switch',
              { 'providers-view-proxy-switch-disabled': pending }
            ]"
            :title="`${activeCliName} 代理接管`"
          >
            <span>Proxy</span>
            <input
              type="checkbox"
              :checked="proxySwitchEnabled"
              :disabled="pending"
              @change="toggleProxySwitch"
            />
            <i></i>
          </label>
          <button
            v-if="activeProxyState && !activeProxyEnabled"
            class="providers-view-proxy-manage"
            type="button"
            :disabled="pending"
            @click="showProxyManager = true"
          >
            <SlidersHorizontal :size="16" />
            接管池
            <span>{{ activeProxyProviderIds.length }}</span>
          </button>
          <button
            v-if="
              activeProxyState &&
              showProxyAddAction &&
              !activeProxyEnabled &&
              !activeProxyProviderIds.length
            "
            class="providers-view-proxy-add"
            type="button"
            :disabled="pending"
            @click="openProxyProviderPicker"
          >
            <Plus :size="16" />
            加入接管池
          </button>
          <button
            :class="[
              'providers-disabled-filter',
              {
                'providers-disabled-filter-active': showDisabledItems
              }
            ]"
            type="button"
            :title="
              showDisabledItems
                ? '隐藏已禁用的 Provider 与官方账号'
                : '显示已禁用的 Provider 与官方账号'
            "
            :aria-pressed="showDisabledItems"
            @click="showDisabledItems = !showDisabledItems"
          >
            <EyeOff v-if="!showDisabledItems" :size="16" />
            <Eye v-else :size="16" />
            <span class="providers-disabled-filter-label">
              {{ showDisabledItems ? "已显示禁用项" : "显示禁用项" }}
            </span>
            <span
              v-if="disabledItemCount"
              class="providers-disabled-filter-count"
            >
              {{ disabledItemCount }}
            </span>
          </button>
          <button
            class="providers-view__system-config"
            type="button"
            title="查看当前系统配置"
            aria-label="查看当前系统配置"
            :disabled="pending"
            @click="openRuntimeConfigDialog"
          >
            <Server :size="18" />
          </button>
          <button
            class="providers-view__add"
            type="button"
            @click="createProvider"
          >
            <Plus :size="22" />
          </button>
        </div>
      </header>

      <CodexProxyPanel
        v-if="activeProxyState && (!activeProxyEnabled || proxyTab === 'proxy')"
        ref="proxyPanelRef"
        :accounts="codexAccounts"
        :cli-name="activeCliName"
        :include-accounts="activeCli === 'codex'"
        :pending="pending"
        :providers="scopedProviders"
        :proxy-state="activeProxyState"
        @account-model-save="emit('codex-proxy-account-model-save', $event)"
        @add-provider="
          (payload) => {
            showProxyAddAction = false
            emitProxyEvent('provider-add', payload)
          }
        "
        @remove-provider="emitProxyEvent('provider-remove', $event)"
        @activate-provider="emitProxyEvent('provider-activate', $event)"
        @restore-account="emit('codex-account-restore', $event)"
        @restore-provider="emit('save-provider', $event)"
      />

      <BaseModal
        v-if="showProxyManager"
        class="providers-view-proxy-modal"
        title="接管池管理"
        description=""
        @close="showProxyManager = false"
      >
        <CodexProxyPanel
          mode="manage"
          :accounts="codexAccounts"
          :cli-name="activeCliName"
          :include-accounts="activeCli === 'codex'"
          :pending="pending"
          :providers="scopedProviders"
          :proxy-state="activeProxyState"
          @account-model-save="emit('codex-proxy-account-model-save', $event)"
          @add-provider="
            (payload) => {
              showProxyAddAction = false
              emitProxyEvent('provider-add', payload)
            }
          "
          @remove-provider="emitProxyEvent('provider-remove', $event)"
          @activate-provider="emitProxyEvent('provider-activate', $event)"
          @restore-account="emit('codex-account-restore', $event)"
          @restore-provider="emit('save-provider', $event)"
        />
      </BaseModal>

      <section
        v-if="!activeProxyEnabled || proxyTab === 'providers'"
        class="providers-view__list-panel"
      >
        <article
          v-for="item in mixedItems"
          :key="item.key"
          :class="item.className"
        >
          <template v-if="item.type === 'account'">
            <span class="providers-view__shield">
              <ShieldCheck :size="18" />
            </span>
            <div class="providers-view__account-main">
              <div class="providers-view__account-title">
                <strong :title="item.account.email">
                  {{ item.account.email }}
                </strong>
                <span
                  :class="[
                    'providers-view__account-tag',
                    {
                      'providers-view__account-tag--pro':
                        item.account.plan === 'pro',
                      'providers-view__account-tag--plus':
                        item.account.plan === 'plus'
                    }
                  ]"
                  :title="formatPlanName(item.account.plan)"
                >
                  {{ formatPlanName(item.account.plan) }}
                </span>
                <span
                  v-if="item.account.disabled"
                  class="providers-view__account-tag providers-view__account-tag--disabled"
                >
                  已禁用
                </span>
                <span
                  v-if="item.account.refresh_status === 'failed'"
                  class="providers-view__account-error"
                  :aria-label="`刷新额度失败：${item.account.refresh_message}`"
                >
                  <span
                    class="providers-view__account-tag providers-view__account-tag--error"
                  >
                    {{ item.account.refresh_status_code || "错误" }}
                  </span>
                  <span class="providers-view__account-error-tip">
                    <span class="providers-view__account-error-title">
                      刷新额度失败
                    </span>
                    <span class="providers-view__account-error-message">
                      {{ item.account.refresh_message }}
                    </span>
                  </span>
                </span>
                <button
                  v-if="
                    !item.account.disabled &&
                    item.account.refresh_status === 'failed'
                  "
                  class="providers-view__reauth-button"
                  type="button"
                  @click.stop="openCodexAuthUpdateModal(item.account)"
                >
                  更新认证信息
                </button>
              </div>
              <div
                v-if="
                  item.account.usage?.rate_limit ||
                  isCodexAccountRefreshing(item.account)
                "
                class="providers-view__quota-list"
              >
                <template v-if="item.account.usage?.rate_limit">
                  <div
                    v-for="quota in rateLimitWindows(
                      item.account.usage.rate_limit
                    )"
                    :key="quota.key"
                    :class="[
                      'providers-view__account-quota',
                      quotaLevelClass(quota.window),
                      {
                        'providers-view__account-quota--loading':
                          isCodexAccountRefreshing(item.account)
                      }
                    ]"
                  >
                    <div
                      class="providers-view__quota-bar"
                      :title="formatUnixTime(quota.window.reset_at)"
                    >
                      <div class="providers-view__quota-title">
                        <span class="providers-view__quota-icon"></span>
                        <span class="providers-view__quota-name">{{
                          formatRateWindowName(
                            quota.window.limit_window_seconds
                          )
                        }}</span>
                      </div>
                      <span
                        class="providers-view__quota-fill"
                        :style="{
                          width: formatRateWidth(
                            100 - (quota.window.used_percent || 0)
                          )
                        }"
                      ></span>
                      <div class="providers-view__quota-meta">
                        <strong class="providers-view__quota-value">
                          {{ 100 - (quota.window.used_percent || 0) }}% ·
                        </strong>
                        <span class="providers-view__quota-reset">
                          {{
                            formatResetCountdown(quota.window.reset_at)
                          }}后重置
                        </span>
                      </div>
                    </div>
                  </div>
                </template>
                <div
                  v-else
                  class="providers-view__account-quota providers-view__account-quota--loading"
                >
                  <div class="providers-view__quota-bar" title="额度刷新中">
                    <div class="providers-view__quota-title">
                      <span class="providers-view__quota-icon"></span>
                      <span class="providers-view__quota-name">额度刷新中</span>
                    </div>
                    <span
                      class="providers-view__quota-fill"
                      style="width: 56%"
                    ></span>
                    <div class="providers-view__quota-meta">
                      <strong class="providers-view__quota-value"> ... </strong>
                      <span class="providers-view__quota-reset"> 请稍候 </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
            <div class="providers-view__account-actions">
              <div class="providers-view__action-main">
                <span
                  v-if="item.account.disabled"
                  class="providers-view__state-pill providers-view__state-pill--disabled"
                >
                  已禁用
                </span>
                <button
                  v-if="item.account.disabled"
                  class="providers-view__enable"
                  type="button"
                  @click="restoreCodexAccount(item.account)"
                >
                  <RefreshCw :size="15" />
                  恢复
                </button>
                <span
                  v-else-if="item.account.active"
                  class="providers-view__state-pill"
                >
                  <span class="providers-view__state-dot"></span>
                  已启用
                </span>
                <button
                  v-if="!item.account.disabled && item.account.active"
                  class="providers-view__using"
                  type="button"
                  @click="clearCodexAccount"
                >
                  <X :size="15" />
                  取消启用
                </button>
                <button
                  v-else-if="!item.account.disabled"
                  class="providers-view__enable"
                  type="button"
                  @click="enableCodexAccount(item.account)"
                >
                  <Play :size="15" />
                  启用
                </button>
              </div>
              <div class="providers-view__icon-actions">
                <button
                  :class="[
                    'providers-view__icon-button',
                    {
                      'providers-view__icon-button--loading':
                        isCodexAccountRefreshing(item.account)
                    }
                  ]"
                  type="button"
                  title="刷新额度"
                  aria-label="刷新额度"
                  :disabled="isCodexAccountRefreshing(item.account)"
                  v-if="!item.account.disabled"
                  @click="refreshCodexAccount(item.account)"
                >
                  <RefreshCw :size="15" />
                </button>
                <button
                  class="providers-view__icon-button"
                  type="button"
                  title="查看详情"
                  aria-label="查看详情"
                  @click="openCodexAccountDetail(item.account)"
                >
                  <Eye :size="15" />
                </button>
                <button
                  v-if="!item.account.disabled"
                  class="providers-view__icon-button"
                  type="button"
                  title="启动官方账号实例"
                  aria-label="启动官方账号实例"
                  :disabled="pending"
                  @click="launchCodexProviderInstance(item.account)"
                >
                  <SquareTerminal :size="15" />
                </button>

                <button
                  v-if="!item.account.disabled"
                  class="providers-view__icon-button"
                  type="button"
                  title="编辑代理"
                  aria-label="编辑代理"
                  @click="openCodexAccountProxy(item.account)"
                >
                  <SquarePen :size="15" />
                </button>
                <button
                  v-if="!item.account.disabled"
                  class="providers-view__icon-button providers-view__icon-button--warning"
                  type="button"
                  title="禁用账号"
                  aria-label="禁用账号"
                  @click="disableCodexAccount(item.account)"
                >
                  <Ban :size="15" />
                </button>
                <button
                  v-if="!item.account.disabled"
                  class="providers-view__icon-button providers-view__icon-button--danger"
                  type="button"
                  title="删除账号"
                  aria-label="删除账号"
                  @click="deleteCodexAccount(item.account)"
                >
                  <Trash2 :size="15" />
                </button>
              </div>
            </div>
          </template>
          <template v-else>
            <!-- <GripVertical class="providers-view__drag" :size="16" /> -->
            <span class="providers-view__avatar">
              <AiIcon
                v-if="item.provider.icon"
                class="providers-view__avatar-icon"
                :name="item.provider.icon"
                :alt="`${item.provider.name} 图标`"
              />
              <template v-else>{{ item.provider.name.slice(0, 1) }}</template>
            </span>
            <div class="providers-view__provider-main">
              <div class="providers-view__provider-title">
                <strong>{{ item.provider.name }}</strong>
                <span
                  v-if="item.provider.enabled === false"
                  class="providers-view__account-tag providers-view__account-tag--disabled"
                >
                  已禁用
                </span>
              </div>
              <span
                v-if="item.provider.note"
                class="providers-view__provider-note"
                :title="item.provider.note"
              >
                {{ item.provider.note }}
              </span>
              <span>{{ item.provider.baseUrl || "未配置官网地址" }}</span>
            </div>
            <div class="providers-view__provider-actions">
              <div class="providers-view__action-main">
                <span
                  v-if="item.provider.enabled === false"
                  class="providers-view__state-pill providers-view__state-pill--disabled"
                >
                  已禁用
                </span>
                <button
                  v-if="item.provider.enabled === false"
                  class="providers-view__enable"
                  type="button"
                  @click.stop="restoreProvider(item.provider)"
                >
                  <RefreshCw :size="15" />
                  恢复
                </button>
                <span
                  v-else-if="
                    profileMap[activeCli]?.providerId === item.provider.id
                  "
                  class="providers-view__state-pill"
                >
                  <span class="providers-view__state-dot"></span>
                  已启用
                </span>
                <button
                  v-if="
                    item.provider.enabled !== false &&
                    showRuntimeWarning &&
                    profileMap[activeCli]?.providerId === item.provider.id
                  "
                  class="providers-view__compare-button"
                  type="button"
                  :disabled="pending"
                  @click.stop="openRuntimeCompareDialog"
                >
                  对比
                </button>
                <button
                  v-if="
                    item.provider.enabled !== false &&
                    profileMap[activeCli]?.providerId === item.provider.id
                  "
                  class="providers-view__using"
                  type="button"
                  @click.stop="clearRuntime"
                >
                  <X :size="15" />
                  取消使用
                </button>
                <button
                  v-else-if="item.provider.enabled !== false"
                  class="providers-view__enable"
                  type="button"
                  @click.stop="enableProvider(item.provider)"
                >
                  <Play :size="15" />
                  启用
                </button>
              </div>
              <div class="providers-view__icon-actions">
                <button
                  class="providers-view__icon-button"
                  type="button"
                  title="查看详情"
                  aria-label="查看详情"
                  @click.stop="openProviderDetail(item.provider)"
                >
                  <Eye :size="16" />
                </button>
                <button
                  v-if="
                    activeCli === 'codex' && item.provider.enabled !== false
                  "
                  class="providers-view__icon-button"
                  type="button"
                  title="启动 Codex 实例"
                  aria-label="启动 Codex 实例"
                  :disabled="pending"
                  @click.stop="launchCodexProviderInstance(item.provider)"
                >
                  <SquareTerminal :size="16" />
                </button>
                <button
                  v-if="
                    activeCli === 'claude' && item.provider.enabled !== false
                  "
                  class="providers-view__icon-button"
                  type="button"
                  title="启动 Claude 实例"
                  aria-label="启动 Claude 实例"
                  :disabled="pending"
                  @click.stop="launchClaudeProviderInstance(item.provider)"
                >
                  <SquareTerminal :size="16" />
                </button>
                <button
                  v-if="item.provider.enabled !== false"
                  class="providers-view__icon-button"
                  type="button"
                  title="管理 API Key"
                  aria-label="管理 API Key"
                  @click.stop="openApiKeyManager(item.provider)"
                >
                  <KeyRound :size="16" />
                </button>
                <button
                  v-if="item.provider.enabled !== false"
                  class="providers-view__icon-button"
                  type="button"
                  title="编辑 Provider"
                  aria-label="编辑 Provider"
                  @click.stop="editProvider(item.provider)"
                >
                  <SquarePen :size="16" />
                </button>
                <button
                  v-if="item.provider.enabled !== false"
                  class="providers-view__icon-button providers-view__icon-button--warning"
                  type="button"
                  title="禁用 Provider"
                  aria-label="禁用 Provider"
                  @click.stop="disableProvider(item.provider)"
                >
                  <Ban :size="16" />
                </button>
                <button
                  v-if="item.provider.enabled !== false"
                  class="providers-view__icon-button providers-view__icon-button--danger"
                  type="button"
                  @click.stop="removeProvider(item.provider)"
                >
                  <Trash2 :size="16" />
                </button>
              </div>
            </div>
          </template>
        </article>

        <div v-if="!mixedItems.length" class="providers-view__empty">
          {{ showDisabledItems ? "当前 CLI 还没有 Provider。" : "当前 CLI 暂无启用的 Provider。" }}
        </div>
      </section>
    </div>

    <template v-else>
      <header class="providers-view__edit-header">
        <button
          class="providers-view__back"
          type="button"
          @click="viewMode = 'list'"
        >
          <ArrowLeft :size="18" />
        </button>
        <h1>{{ draft.id ? "编辑供应商" : "新增供应商" }}</h1>
      </header>

      <section class="providers-view__edit-panel">
        <div class="providers-view__avatar-picker">
          <button
            class="providers-view__edit-avatar"
            type="button"
            @click="showIconPicker = !showIconPicker"
          >
            <AiIcon
              v-if="draft.icon"
              class="providers-view__edit-avatar-icon"
              :name="draft.icon"
              :alt="`${draft.name || 'Provider'} 图标`"
            />
            <template v-else>{{ draft.name.slice(0, 1) || "AI" }}</template>
          </button>
          <div v-if="draft.icon" class="providers-view__avatar-name">
            {{ iconLabel(draft.icon) }}
          </div>
          <section v-if="showIconPicker" class="providers-view__icon-panel">
            <label class="providers-view-icon-upload">
              <Upload :size="16" />
              上传图标
              <input
                type="file"
                accept="image/svg+xml,image/png,image/jpeg,image/webp"
                @change="uploadCustomIcon"
              />
            </label>
            <label class="providers-view__field providers-view__field--wide">
              <span>搜索图标</span>
              <input
                v-model.trim="iconKeyword"
                type="text"
                placeholder="输入图标名称..."
              />
            </label>
            <div class="providers-view__icon-grid">
              <button
                v-for="icon in filteredIconOptions"
                :key="icon"
                :class="[
                  'providers-view__icon-option',
                  { 'providers-view__icon-option--active': draft.icon === icon }
                ]"
                type="button"
                @click="selectIcon(icon)"
              >
                <AiIcon
                  class="providers-view__icon-option-image"
                  :name="icon"
                  :alt="`${iconLabel(icon)} 图标`"
                />
                <span>{{ iconLabel(icon) }}</span>
              </button>
            </div>
          </section>
        </div>

        <div class="providers-view__form-grid">
          <label class="providers-view__field">
            <span>供应商名称</span>
            <input v-model.trim="draft.name" type="text" />
          </label>
          <label class="providers-view__field">
            <span>备注</span>
            <input v-model.trim="draft.note" type="text" />
          </label>
          <label class="providers-view__field providers-view__field--wide">
            <span>官网链接</span>
            <input v-model.trim="draft.website" type="text" />
          </label>
          <section class="providers-view__api-keys providers-view__field--wide">
            <div class="providers-view__api-keys-header">
              <span>API Key</span>
              <button type="button" @click="addApiKey()">
                <Plus :size="14" />
                添加 Key
              </button>
            </div>
            <div class="providers-view__api-key-list">
              <div
                v-for="(item, index) in draft.apiKeys"
                :key="item.id"
                class="providers-view__api-key-item"
              >
                <div class="providers-view__api-key-meta">
                  <input
                    v-model.trim="item.name"
                    class="providers-view__api-key-name"
                    type="text"
                    placeholder="Key 名称"
                  />
                  <button
                    type="button"
                    :class="{
                      'providers-view__api-key-active':
                        draft.activeApiKeyId === item.id
                    }"
                    @click="activateApiKey(item.id)"
                  >
                    {{
                      draft.activeApiKeyId === item.id ? "当前生效" : "设为生效"
                    }}
                  </button>
                  <button
                    type="button"
                    title="删除 API Key"
                    aria-label="删除 API Key"
                    @click="removeApiKey(index)"
                  >
                    <Trash2 :size="14" />
                  </button>
                </div>
                <input
                  v-model.trim="item.note"
                  class="providers-view__api-key-note"
                  type="text"
                  placeholder="备注信息，例如：生产环境 / 备用额度"
                />
                <el-input
                  v-model="item.apiKey"
                  type="password"
                  show-password
                  :placeholder="
                    item.masked ? `${item.masked}，留空则保持不变` : '输入 API Key'
                  "
                />
              </div>
            </div>
            <small>可保存多个 Key，但同时只会使用当前生效的一个。</small>
          </section>
          <label class="providers-view__field providers-view__field--wide">
            <span>请求地址</span>
            <input v-model.trim="draft.baseUrl" type="text" />
          </label>
        </div>

        <div class="providers-view__warning">
          填写兼容当前 CLI 的服务端点地址，不要以斜杠结尾
        </div>

        <details
          v-if="activeRuntimeSchema.advancedFields.length"
          class="providers-view__advanced"
        >
          <summary>高级选项</summary>
          <label
            v-if="activeRuntimeSchema.advancedFields.includes('type')"
            class="providers-view__field"
          >
            <span>API 格式</span>
            <select v-model="draft.type">
              <option v-for="item in providerTypes" :key="item" :value="item">
                {{ providerTypeLabelMap[item] || item }}
              </option>
            </select>
          </label>
          <label
            v-if="activeRuntimeSchema.advancedFields.includes('authField')"
            class="providers-view__field"
          >
            <span>认证字段</span>
            <select v-model="draft.authField">
              <option
                v-for="field in activeRuntimeSchema.authFields"
                :key="field"
                :value="field"
              >
                {{ field }}
              </option>
            </select>
          </label>
        </details>

        <section class="providers-view__models">
          <div class="providers-view__section-title">
            <div>
              <h2>模型映射</h2>
              <p>仅在需要将请求映射到不同模型名称时填写。</p>
            </div>
            <!-- <div class="providers-view__section-actions">
              <button type="button">获取模型列表</button>
            </div> -->
          </div>

          <div class="providers-view__form-grid">
            <label
              v-for="field in activeRuntimeSchema.modelFields"
              :key="field.key"
              class="providers-view__field"
            >
              <span>{{ field.label }}</span>
              <input v-model.trim="modelDrafts[field.key]" type="text" />
              <small v-if="field.description">{{ field.description }}</small>
            </label>
          </div>
        </section>

        <section class="providers-view__json">
          <div class="providers-view__json-title">
            <strong>配置 JSON</strong>
          </div>
          <div class="providers-view__check-row">
            <label
              v-for="field in activeRuntimeSchema.optionFields"
              :key="field.key"
              class="providers-view__option-field"
            >
              <template v-if="field.type === 'number'">
                <span>{{ field.label }}</span>
                <input
                  v-model.number="draft[field.key]"
                  type="number"
                  :disabled="field.dependsOn && !draft[field.dependsOn]"
                />
              </template>
              <template v-else-if="field.type === 'select'">
                <span>{{ field.label }}</span>
                <select v-model="draft[field.key]">
                  <option
                    v-for="option in runtimeFieldOptions(field)"
                    :key="option"
                    :value="option"
                  >
                    {{ option }}
                  </option>
                </select>
              </template>
              <template v-else>
                <input v-model="draft[field.key]" type="checkbox" />
                {{ field.label }}
              </template>
            </label>
          </div>
          <details
            v-for="file in activeRuntimeSchema.configFiles"
            :key="file.name"
            class="providers-view__config-preview"
            open
          >
            <summary>
              <span>{{ file.name }} ({{ file.format }})</span>
            </summary>
            <pre>{{ configPreviewMap[file.name] }}</pre>
            <p>{{ file.description }}</p>
          </details>
        </section>
      </section>

      <footer class="providers-view__edit-footer">
        <button
          class="providers-view__primary"
          type="button"
          :disabled="pending"
          @click="submitProvider"
        >
          <Save :size="16" />
          保存
        </button>
      </footer>
    </template>

    <BaseModal
      v-if="showProviderCreateModal"
      class="providers-view__provider-create-modal"
      :title="draft.id ? '编辑供应商' : '新增供应商'"
      @close="closeProviderCreateModal"
    >
      <section class="providers-view__create-form">
        <div class="providers-view__avatar-picker">
          <button
            class="providers-view__edit-avatar"
            type="button"
            @click="showIconPicker = !showIconPicker"
          >
            <AiIcon
              v-if="draft.icon"
              class="providers-view__edit-avatar-icon"
              :name="draft.icon"
              :alt="`${draft.name || 'Provider'} 图标`"
            />
            <template v-else>{{ draft.name.slice(0, 1) || "AI" }}</template>
          </button>
          <div v-if="draft.icon" class="providers-view__avatar-name">
            {{ iconLabel(draft.icon) }}
          </div>
          <section v-if="showIconPicker" class="providers-view__icon-panel">
            <label class="providers-view-icon-upload">
              <Upload :size="16" />
              上传图标
              <input
                type="file"
                accept="image/svg+xml,image/png,image/jpeg,image/webp"
                @change="uploadCustomIcon"
              />
            </label>
            <label class="providers-view__field providers-view__field--wide">
              <span>搜索图标</span>
              <input
                v-model.trim="iconKeyword"
                type="text"
                placeholder="输入图标名称..."
              />
            </label>
            <div class="providers-view__icon-grid">
              <button
                v-for="icon in filteredIconOptions"
                :key="icon"
                :class="[
                  'providers-view__icon-option',
                  { 'providers-view__icon-option--active': draft.icon === icon }
                ]"
                type="button"
                @click="selectIcon(icon)"
              >
                <AiIcon
                  class="providers-view__icon-option-image"
                  :name="icon"
                  :alt="`${iconLabel(icon)} 图标`"
                />
                <span>{{ iconLabel(icon) }}</span>
              </button>
            </div>
          </section>
        </div>

        <div class="providers-view__form-grid">
          <label class="providers-view__field">
            <span>供应商名称</span>
            <input v-model.trim="draft.name" type="text" />
          </label>
          <label class="providers-view__field">
            <span>备注</span>
            <input v-model.trim="draft.note" type="text" />
          </label>
          <label class="providers-view__field providers-view__field--wide">
            <span>官网链接</span>
            <input v-model.trim="draft.website" type="text" />
          </label>
          <section class="providers-view__api-keys providers-view__field--wide">
            <div class="providers-view__api-keys-header">
              <span>API Key</span>
              <button type="button" @click="addApiKey()">
                <Plus :size="14" />
                添加 Key
              </button>
            </div>
            <div class="providers-view__api-key-list">
              <div
                v-for="(item, index) in draft.apiKeys"
                :key="item.id"
                class="providers-view__api-key-item"
              >
                <div class="providers-view__api-key-meta">
                  <input
                    v-model.trim="item.name"
                    class="providers-view__api-key-name"
                    type="text"
                    placeholder="Key 名称"
                  />
                  <button
                    type="button"
                    :class="{
                      'providers-view__api-key-active':
                        draft.activeApiKeyId === item.id
                    }"
                    @click="activateApiKey(item.id)"
                  >
                    {{
                      draft.activeApiKeyId === item.id ? "当前生效" : "设为生效"
                    }}
                  </button>
                  <button
                    type="button"
                    title="删除 API Key"
                    aria-label="删除 API Key"
                    @click="removeApiKey(index)"
                  >
                    <Trash2 :size="14" />
                  </button>
                </div>
                <input
                  v-model.trim="item.note"
                  class="providers-view__api-key-note"
                  type="text"
                  placeholder="备注信息，例如：生产环境 / 备用额度"
                />
                <el-input
                  v-model="item.apiKey"
                  type="password"
                  show-password
                  :placeholder="
                    item.masked ? `${item.masked}，留空则保持不变` : '输入 API Key'
                  "
                />
              </div>
            </div>
            <small>可保存多个 Key，但同时只会使用当前生效的一个。</small>
          </section>
          <label class="providers-view__field providers-view__field--wide">
            <span>请求地址</span>
            <input v-model.trim="draft.baseUrl" type="text" />
          </label>
        </div>

        <div class="providers-view__warning">
          填写兼容当前 CLI 的服务端点地址，不要以斜杠结尾
        </div>

        <details
          v-if="activeRuntimeSchema.advancedFields.length"
          class="providers-view__advanced"
        >
          <summary>高级选项</summary>
          <label
            v-if="activeRuntimeSchema.advancedFields.includes('type')"
            class="providers-view__field"
          >
            <span>API 格式</span>
            <select v-model="draft.type">
              <option v-for="item in providerTypes" :key="item" :value="item">
                {{ providerTypeLabelMap[item] || item }}
              </option>
            </select>
          </label>
          <label
            v-if="activeRuntimeSchema.advancedFields.includes('authField')"
            class="providers-view__field"
          >
            <span>认证字段</span>
            <select v-model="draft.authField">
              <option
                v-for="field in activeRuntimeSchema.authFields"
                :key="field"
                :value="field"
              >
                {{ field }}
              </option>
            </select>
          </label>
        </details>

        <section class="providers-view__models">
          <div class="providers-view__section-title">
            <div>
              <h2>模型映射</h2>
              <p>仅在需要将请求映射到不同模型名称时填写。</p>
            </div>
            <!-- <div class="providers-view__section-actions">
              <button type="button">获取模型列表</button>
            </div> -->
          </div>

          <div class="providers-view__form-grid">
            <label
              v-for="field in activeRuntimeSchema.modelFields"
              :key="field.key"
              class="providers-view__field"
            >
              <span>{{ field.label }}</span>
              <input v-model.trim="modelDrafts[field.key]" type="text" />
              <small v-if="field.description">{{ field.description }}</small>
            </label>
          </div>
        </section>

        <section class="providers-view__json">
          <div class="providers-view__json-title">
            <strong>配置 JSON</strong>
          </div>
          <div class="providers-view__check-row">
            <label
              v-for="field in activeRuntimeSchema.optionFields"
              :key="field.key"
              class="providers-view__option-field"
            >
              <template v-if="field.type === 'number'">
                <span>{{ field.label }}</span>
                <input
                  v-model.number="draft[field.key]"
                  type="number"
                  :disabled="field.dependsOn && !draft[field.dependsOn]"
                />
              </template>
              <template v-else-if="field.type === 'select'">
                <span>{{ field.label }}</span>
                <select v-model="draft[field.key]">
                  <option
                    v-for="option in runtimeFieldOptions(field)"
                    :key="option"
                    :value="option"
                  >
                    {{ option }}
                  </option>
                </select>
              </template>
              <template v-else>
                <input v-model="draft[field.key]" type="checkbox" />
                {{ field.label }}
              </template>
            </label>
          </div>
          <details
            v-for="file in activeRuntimeSchema.configFiles"
            :key="file.name"
            class="providers-view__config-preview"
            open
          >
            <summary>
              <span>{{ file.name }} ({{ file.format }})</span>
            </summary>
            <pre>{{ configPreviewMap[file.name] }}</pre>
            <p>{{ file.description }}</p>
          </details>
        </section>
      </section>

      <footer class="providers-view__create-footer">
        <button
          class="providers-view__primary"
          type="button"
          :disabled="pending"
          @click="submitProvider"
        >
          <Save :size="16" />
          保存
        </button>
      </footer>
    </BaseModal>

    <BaseModal
      v-if="showApiKeyManager"
      class="providers-view__api-key-modal"
      title="API Key 管理"
      @close="closeApiKeyManager"
    >
      <section class="providers-view__api-key-manager">
        <header class="providers-view__api-key-manager-header">
          <strong>{{ apiKeyManagerProvider?.name || "Provider" }}</strong>
          <span>仅当前生效的 Key 会被运行时使用。</span>
        </header>
        <div class="providers-view__api-key-list">
          <div
            v-for="(item, index) in apiKeyManagerDraft.apiKeys"
            :key="item.id"
            class="providers-view__api-key-item"
          >
            <div class="providers-view__api-key-meta">
              <input
                v-model.trim="item.name"
                class="providers-view__api-key-name"
                type="text"
                placeholder="Key 名称"
              />
              <button
                type="button"
                :class="{
                  'providers-view__api-key-active':
                    apiKeyManagerDraft.activeApiKeyId === item.id
                }"
                @click="activateApiKey(item.id, apiKeyManagerDraft)"
              >
                {{
                  apiKeyManagerDraft.activeApiKeyId === item.id
                    ? "当前生效"
                    : "设为生效"
                }}
              </button>
              <button
                type="button"
                title="删除 API Key"
                aria-label="删除 API Key"
                @click="removeApiKey(index, apiKeyManagerDraft)"
              >
                <Trash2 :size="14" />
              </button>
            </div>
            <input
              v-model.trim="item.note"
              class="providers-view__api-key-note"
              type="text"
              placeholder="备注信息，例如：生产环境 / 备用额度"
            />
            <el-input
              v-model="item.apiKey"
              type="password"
              show-password
              :placeholder="
                item.masked ? `${item.masked}，留空则保持不变` : '输入 API Key'
              "
            />
          </div>
        </div>
        <button
          class="providers-view__api-key-add"
          type="button"
          @click="addApiKey(apiKeyManagerDraft)"
        >
          <Plus :size="15" />
          添加 API Key
        </button>
        <footer class="providers-view__api-key-manager-footer">
          <button type="button" @click="closeApiKeyManager">取消</button>
          <button
            class="providers-view__primary"
            type="button"
            :disabled="pending"
            @click="saveApiKeyManager"
          >
            <Save :size="16" />
            保存并应用
          </button>
        </footer>
      </section>
    </BaseModal>

    <BaseModal
      v-if="showCodexCreateOptions"
      title="新增供应商"
      description="选择官方账号登录，或者继续使用兼容供应商配置。"
      @close="showCodexCreateOptions = false"
    >
      <section class="providers-view__create-options">
        <button
          class="providers-view__create-option"
          type="button"
          @click="openCodexLoginModal"
        >
          <div class="option-logo">
            <ShieldCheck :size="28" />
            <strong>官方登录</strong>
          </div>
          <span
            >通过 OAuth 管理 Codex 账号，后续使用独立
            Runtime，不写入系统配置。</span
          >
        </button>
        <button
          class="providers-view__create-option"
          type="button"
          @click="startProviderCreate"
        >
          <div class="option-logo">
            <Server :size="28" />
            <strong>供应商</strong>
          </div>
          <span>使用当前 API Key、Base URL 和模型映射方案。</span>
        </button>
      </section>
    </BaseModal>

    <BaseModal
      v-if="showCodexLoginModal"
      class="providers-view__codex-login-modal"
      :title="codexLoginTitle"
      @close="closeCodexLoginModal"
    >
      <section class="providers-view__login-panel">
        <nav class="providers-view__login-tabs">
          <button
            :class="[
              'providers-view__login-tab',
              { 'providers-view__login-tab--active': codexLoginTab === 'oauth' }
            ]"
            type="button"
            @click="codexLoginTab = 'oauth'"
          >
            <Globe2 :size="13" />
            OAuth 授权
          </button>
          <button
            :class="[
              'providers-view__login-tab',
              { 'providers-view__login-tab--active': codexLoginTab === 'auth' }
            ]"
            type="button"
            @click="codexLoginTab = 'auth'"
          >
            <ShieldCheck :size="13" />
            JSON 数据
          </button>
        </nav>

        <label class="providers-view__login-field">
          <span>代理地址</span>
          <input
            v-model.trim="codexProxyDraft"
            type="text"
            placeholder="可选，例如：http://127.0.0.1:7890"
          />
        </label>

        <template v-if="codexLoginTab === 'oauth'">
          <p class="providers-view__login-intro">
            点击浏览器登录后才会启动本地回调服务并生成授权链接。
          </p>
          <div
            v-if="codexLoginState?.message"
            :class="[
              'providers-view__login-status',
              `providers-view__login-status--${codexLoginState.status}`
            ]"
          >
            {{ codexLoginState.message }}
          </div>
          <label
            v-if="codexLoginState?.authUrl"
            class="providers-view__login-field"
          >
            <span>授权链接</span>
            <div class="providers-view__login-copy-row">
              <input :value="codexLoginState.authUrl" readonly type="text" />
              <button
                type="button"
                title="复制链接"
                aria-label="复制链接"
                @click="copyAuthUrl"
              >
                <Copy :size="16" />
              </button>
            </div>
          </label>
          <button
            class="providers-view__login-primary"
            type="button"
            :disabled="pending || codexLoginState?.status === 'pending'"
            @click="startCodexOfficialLogin"
          >
            <Globe2 :size="16" />
            浏览器登录
          </button>
          <label
            v-if="codexLoginState?.authUrl"
            class="providers-view__login-field"
          >
            <span>手动输入回调地址</span>
            <div class="providers-view__login-callback-row">
              <input
                v-model.trim="manualCallbackUrl"
                type="text"
                placeholder="粘贴完整回调地址，例如：http://localhost..."
              />
              <button
                type="button"
                :disabled="!manualCallbackUrl"
                @click="openManualCallbackUrl"
              >
                <Check :size="15" />
                我已授权，继续
              </button>
            </div>
          </label>
          <div
            v-if="codexLoginState?.authUrl"
            class="providers-view__login-tip"
          >
            完成授权后，此窗口将自动更新
          </div>
          <div
            v-if="codexLoginState?.status === 'pending'"
            class="providers-view__login-actions"
          >
            <button type="button" @click="emit('cancel-codex-official-login')">
              取消登录
            </button>
          </div>
        </template>

        <template v-else>
          <p class="providers-view__login-intro">
            粘贴已有 Codex 登录 JSON 数据，Monkey Thief 会使用 access_token
            解析并验证账号。
          </p>
          <label class="providers-view__login-field">
            <span>JSON 数据</span>
            <textarea
              v-model.trim="codexAuthDataDraft"
              placeholder='{"access_token":"","account_id":"","id_token":"","refresh_token":""}'
            />
          </label>
          <div class="providers-view__login-actions">
            <button
              type="button"
              :disabled="pending || !codexAuthDataDraft"
              @click="importCodexAuthData"
            >
              解析并验证
            </button>
          </div>
        </template>
      </section>
    </BaseModal>

    <BaseModal
      v-if="showCodexProxyModal"
      class="providers-view__codex-proxy-modal"
      title="编辑代理"
      @close="closeCodexAccountProxy"
    >
      <section class="providers-view__login-panel">
        <p class="providers-view__login-intro">
          这个代理只绑定当前 Codex 官方账号，后续刷新额度和 token
          时会继续使用它。
        </p>
        <label class="providers-view__login-field">
          <span>代理地址</span>
          <input
            v-model.trim="editingCodexProxy"
            type="text"
            placeholder="可选，例如：http://127.0.0.1:7890"
          />
        </label>
        <div class="providers-view__login-actions">
          <button type="button" @click="closeCodexAccountProxy">取消</button>
          <button
            type="button"
            :disabled="pending"
            @click="saveCodexAccountProxy"
          >
            保存
          </button>
        </div>
      </section>
    </BaseModal>

    <div v-if="showCodexAccountDrawer" class="providers-view__drawer">
      <div
        class="providers-view__drawer-backdrop"
        @click="closeCodexAccountDetail"
      ></div>
      <aside class="providers-view__drawer-panel">
        <header class="providers-view__drawer-header">
          <div>
            <h2>账户详情</h2>
            <p>{{ codexAccountDetail?.email || "未加载" }}</p>
          </div>
          <button
            class="providers-view__drawer-close"
            type="button"
            @click="closeCodexAccountDetail"
          >
            ×
          </button>
        </header>
        <nav class="providers-view-drawer-tabs">
          <button
            :class="[
              'providers-view-drawer-tab',
              {
                'providers-view-drawer-tab-active':
                  codexAccountDetailTab === 'config'
              }
            ]"
            type="button"
            @click="codexAccountDetailTab = 'config'"
          >
            配置
          </button>
          <button
            :class="[
              'providers-view-drawer-tab',
              {
                'providers-view-drawer-tab-active':
                  codexAccountDetailTab === 'usage'
              }
            ]"
            type="button"
            @click="codexAccountDetailTab = 'usage'"
          >
            用量
          </button>
        </nav>
        <div class="providers-view__drawer-content">
          <div
            v-if="
              codexAccountDetailLoading && codexAccountDetailTab === 'config'
            "
            class="providers-view__drawer-empty"
          >
            加载中...
          </div>
          <template
            v-else-if="codexAccountDetail && codexAccountDetailTab === 'config'"
          >
            <section class="providers-view__drawer-section">
              <h3>auth</h3>
              <pre class="providers-view__drawer-json">{{
                formatJson(codexAccountDetail.auth)
              }}</pre>
            </section>
            <section class="providers-view__drawer-section">
              <h3>基础信息</h3>
              <pre class="providers-view__drawer-json">{{
                formatJson({
                  accountId: codexAccountDetail.accountId,
                  email: codexAccountDetail.email,
                  plan: codexAccountDetail.plan,
                  proxy: codexAccountDetail.proxy,
                  disabled: codexAccountDetail.disabled,
                  expired: codexAccountDetail.expired,
                  last_refresh: codexAccountDetail.last_refresh
                })
              }}</pre>
            </section>
          </template>
          <template v-else-if="codexAccountDetailTarget">
            <div class="providers-view-usage-panel">
              <section class="providers-view__drawer-section">
                <h3>额度阶段</h3>
                <div
                  v-if="codexCurrentQuotaStages.length"
                  class="providers-view-quota-stage-list"
                >
                  <article
                    v-for="stage in codexCurrentQuotaStages"
                    :key="stage.id"
                    class="providers-view-quota-stage-card"
                  >
                    <div class="providers-view-quota-stage-head">
                      <div class="providers-view-quota-stage-title">
                        <strong class="providers-view-quota-stage-name">{{
                          formatRateWindowName(stage.limitWindowSeconds)
                        }}</strong>
                        <span class="providers-view-quota-stage-status">
                          当前阶段
                        </span>
                      </div>
                      <strong class="providers-view-quota-stage-percent">
                        剩余 {{ formatQuotaPercent(stage.remainingPercent) }}%
                      </strong>
                    </div>
                    <div class="providers-view-quota-stage-track">
                      <span
                        class="providers-view-quota-stage-fill"
                        :style="{
                          width: formatRateWidth(stage.usedPercent)
                        }"
                      ></span>
                    </div>
                    <div class="providers-view-quota-stage-metrics">
                      <span class="providers-view-quota-stage-metric">
                        已使用
                        <strong class="providers-view-quota-stage-metric-value">
                          {{ formatQuotaPercent(stage.usedPercent) }}%
                        </strong>
                      </span>
                      <span class="providers-view-quota-stage-metric">
                        Token
                        <strong class="providers-view-quota-stage-metric-value">
                          <TokenCount :value="stage.summary?.actualTokens" />
                        </strong>
                      </span>
                      <span class="providers-view-quota-stage-metric">
                        请求
                        <strong class="providers-view-quota-stage-metric-value">
                          {{
                            formatProviderNumber(stage.summary?.requestCount)
                          }}
                        </strong>
                      </span>
                      <span class="providers-view-quota-stage-metric">
                        已计费
                        <strong class="providers-view-quota-stage-metric-value">
                          {{ formatProviderCost(stage.summary?.totalCostUsd) }}
                        </strong>
                      </span>
                    </div>
                    <span class="providers-view-quota-stage-range">
                      {{ formatQuotaStageRange(stage) }}
                    </span>
                  </article>
                </div>
                <div v-else class="providers-view__drawer-empty">
                  刷新账号额度后开始记录额度阶段。
                </div>
                <div
                  v-if="codexQuotaStageHistory.length"
                  class="providers-view-quota-history"
                >
                  <div class="providers-view-quota-history-head">
                    <strong class="providers-view-quota-history-title">
                      历史阶段
                    </strong>
                    <span class="providers-view-quota-history-count">
                      {{ codexQuotaStageHistory.length }} 个
                    </span>
                  </div>
                  <article
                    v-for="stage in codexQuotaStageHistory"
                    :key="stage.id"
                    class="providers-view-quota-history-row"
                  >
                    <div class="providers-view-quota-history-main">
                      <strong class="providers-view-quota-history-name">{{
                        formatRateWindowName(stage.limitWindowSeconds)
                      }}</strong>
                      <span class="providers-view-quota-history-range">
                        {{ formatQuotaStageRange(stage) }}
                      </span>
                    </div>
                    <div class="providers-view-quota-history-metrics">
                      <span class="providers-view-quota-history-usage">
                        使用 {{ formatQuotaPercent(stage.usedPercent) }}% /
                        {{
                          formatProviderNumber(stage.summary?.requestCount)
                        }}
                        次
                      </span>
                      <strong class="providers-view-quota-history-token">
                        <TokenCount :value="stage.summary?.actualTokens" />
                      </strong>
                      <span class="providers-view-quota-history-cost">{{
                        formatProviderCost(stage.summary?.totalCostUsd)
                      }}</span>
                    </div>
                  </article>
                </div>
              </section>
              <section class="providers-view__drawer-section">
                <h3>用量概览</h3>
                <div class="providers-view-usage-hero">
                  <div class="providers-view-usage-hero-item">
                    <span class="providers-view-usage-label">全部 Token</span>
                    <strong class="providers-view-usage-total">
                      <TokenCount
                        :value="codexAccountUsageSummary.actualTokens"
                      />
                    </strong>
                    <span class="providers-view-usage-subtext">
                      {{
                        formatProviderNumber(
                          codexAccountUsageSummary.requestCount
                        )
                      }}
                      次请求
                    </span>
                  </div>
                  <div class="providers-view-usage-hero-item">
                    <span class="providers-view-usage-label">今日 Token</span>
                    <strong class="providers-view-usage-total">
                      <TokenCount
                        :value="codexAccountUsageTodaySummary.actualTokens"
                      />
                    </strong>
                    <span class="providers-view-usage-subtext">
                      {{
                        formatProviderNumber(
                          codexAccountUsageTodaySummary.requestCount
                        )
                      }}
                      次请求
                    </span>
                  </div>
                  <div class="providers-view-usage-hero-side">
                    <span class="providers-view-usage-label">费用</span>
                    <strong class="providers-view-usage-cost">
                      {{
                        formatProviderCost(
                          codexAccountUsageSummary.totalCostUsd
                        )
                      }}
                    </strong>
                    <span class="providers-view-usage-subtext">
                      今日
                      {{
                        formatProviderCost(
                          codexAccountUsageTodaySummary.totalCostUsd
                        )
                      }}
                    </span>
                  </div>
                </div>
              </section>
              <section class="providers-view__drawer-section">
                <h3>Token 明细</h3>
                <div class="providers-view-usage-token-grid">
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">输入</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount
                        :value="codexAccountUsageSummary.inputTokens"
                      />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="codexAccountUsageTodaySummary.inputTokens"
                      />
                    </small>
                  </article>
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">输出</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount
                        :value="codexAccountUsageSummary.outputTokens"
                      />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="codexAccountUsageTodaySummary.outputTokens"
                      />
                    </small>
                  </article>
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">缓存读取</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount
                        :value="codexAccountUsageSummary.cacheReadTokens"
                      />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="codexAccountUsageTodaySummary.cacheReadTokens"
                      />
                    </small>
                  </article>
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">缓存写入</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount
                        :value="codexAccountUsageSummary.cacheCreationTokens"
                      />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="codexAccountUsageTodaySummary.cacheCreationTokens"
                      />
                    </small>
                  </article>
                </div>
              </section>
              <section class="providers-view__drawer-section">
                <h3>模型费用</h3>
                <div
                  v-if="codexAccountUsageModelStats.length"
                  class="providers-view-usage-list"
                >
                  <article
                    v-for="item in codexAccountUsageModelStats"
                    :key="item.model"
                    class="providers-view-usage-row"
                  >
                    <div class="providers-view-usage-row-main">
                      <strong
                        class="providers-view-usage-value"
                        :title="item.model"
                      >
                        {{ item.model }}
                      </strong>
                      <span class="providers-view-usage-label">
                        {{ formatProviderNumber(item.requestCount) }} 次请求
                      </span>
                    </div>
                    <div class="providers-view-usage-row-side">
                      <strong class="providers-view-usage-value">
                        <TokenCount :value="item.actualTokens" />
                      </strong>
                      <span class="providers-view-usage-label">
                        {{ formatProviderCost(item.totalCostUsd) }}
                      </span>
                      <span class="providers-view-usage-label">
                        今日 <TokenCount :value="item.todayActualTokens" />
                      </span>
                    </div>
                  </article>
                </div>
                <div v-else class="providers-view__drawer-empty">
                  暂无用量记录。
                </div>
              </section>
              <div v-if="usageStatsLoading" class="providers-view-usage-mask">
                <span class="providers-view-usage-mask-text">统计中...</span>
              </div>
            </div>
          </template>
        </div>
      </aside>
    </div>

    <div v-if="showProviderDrawer" class="providers-view__drawer">
      <div
        class="providers-view__drawer-backdrop"
        @click="closeProviderDetail"
      ></div>
      <aside class="providers-view__drawer-panel">
        <header class="providers-view__drawer-header">
          <div>
            <h2>Provider 详情</h2>
            <p>{{ providerDetail?.name || "未加载" }}</p>
          </div>
          <button
            class="providers-view__drawer-close"
            type="button"
            @click="closeProviderDetail"
          >
            ×
          </button>
        </header>
        <nav class="providers-view-drawer-tabs">
          <button
            :class="[
              'providers-view-drawer-tab',
              {
                'providers-view-drawer-tab-active':
                  providerDetailTab === 'config'
              }
            ]"
            type="button"
            @click="providerDetailTab = 'config'"
          >
            配置
          </button>
          <button
            :class="[
              'providers-view-drawer-tab',
              {
                'providers-view-drawer-tab-active':
                  providerDetailTab === 'usage'
              }
            ]"
            type="button"
            @click="providerDetailTab = 'usage'"
          >
            用量
          </button>
        </nav>
        <div class="providers-view__drawer-content">
          <template v-if="providerDetail && providerDetailTab === 'config'">
            <section class="providers-view__drawer-section">
              <h3>基础信息</h3>
              <pre class="providers-view__drawer-json">{{
                formatJson({
                  id: providerDetail.id,
                  cli: providerDetail.cli,
                  name: providerDetail.name,
                  type: providerDetail.type,
                  baseUrl: providerDetail.baseUrl,
                  proxy: providerDetail.proxy,
                  apiKey: providerDetail.apiKey,
                  authField: providerDetail.authField,
                  enabled: providerDetail.enabled,
                  website: providerDetail.website,
                  note: providerDetail.note
                })
              }}</pre>
            </section>
            <section class="providers-view__drawer-section">
              <h3>Runtime 配置</h3>
              <pre class="providers-view__drawer-json">{{
                formatJson(providerDetail.runtimeConfig)
              }}</pre>
            </section>
          </template>
          <template v-else-if="providerDetail">
            <div class="providers-view-usage-panel">
              <section class="providers-view__drawer-section">
                <h3>用量概览</h3>
                <div class="providers-view-usage-hero">
                  <div class="providers-view-usage-hero-item">
                    <span class="providers-view-usage-label">全部 Token</span>
                    <strong class="providers-view-usage-total">
                      <TokenCount :value="providerUsageSummary.actualTokens" />
                    </strong>
                    <span class="providers-view-usage-subtext">
                      {{
                        formatProviderNumber(providerUsageSummary.requestCount)
                      }}
                      次请求
                    </span>
                  </div>
                  <div class="providers-view-usage-hero-item">
                    <span class="providers-view-usage-label">今日 Token</span>
                    <strong class="providers-view-usage-total">
                      <TokenCount
                        :value="providerUsageTodaySummary.actualTokens"
                      />
                    </strong>
                    <span class="providers-view-usage-subtext">
                      {{
                        formatProviderNumber(
                          providerUsageTodaySummary.requestCount
                        )
                      }}
                      次请求
                    </span>
                  </div>
                  <div class="providers-view-usage-hero-side">
                    <span class="providers-view-usage-label">费用</span>
                    <strong class="providers-view-usage-cost">
                      {{
                        formatProviderCost(providerUsageSummary.totalCostUsd)
                      }}
                    </strong>
                    <span class="providers-view-usage-subtext">
                      今日
                      {{
                        formatProviderCost(
                          providerUsageTodaySummary.totalCostUsd
                        )
                      }}
                    </span>
                  </div>
                </div>
              </section>
              <section class="providers-view__drawer-section">
                <h3>Token 明细</h3>
                <div class="providers-view-usage-token-grid">
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">输入</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount :value="providerUsageSummary.inputTokens" />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="providerUsageTodaySummary.inputTokens"
                      />
                    </small>
                  </article>
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">输出</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount :value="providerUsageSummary.outputTokens" />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="providerUsageTodaySummary.outputTokens"
                      />
                    </small>
                  </article>
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">缓存读取</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount
                        :value="providerUsageSummary.cacheReadTokens"
                      />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="providerUsageTodaySummary.cacheReadTokens"
                      />
                    </small>
                  </article>
                  <article class="providers-view-usage-token-item">
                    <span class="providers-view-usage-label">缓存写入</span>
                    <strong class="providers-view-usage-value">
                      <TokenCount
                        :value="providerUsageSummary.cacheCreationTokens"
                      />
                    </strong>
                    <small>
                      今日
                      <TokenCount
                        :value="providerUsageTodaySummary.cacheCreationTokens"
                      />
                    </small>
                  </article>
                </div>
              </section>
              <section class="providers-view__drawer-section">
                <h3>模型费用</h3>
                <div
                  v-if="providerUsageModelStats.length"
                  class="providers-view-usage-list"
                >
                  <article
                    v-for="item in providerUsageModelStats"
                    :key="item.model"
                    class="providers-view-usage-row"
                  >
                    <div class="providers-view-usage-row-main">
                      <strong
                        class="providers-view-usage-value"
                        :title="item.model"
                      >
                        {{ item.model }}
                      </strong>
                      <span class="providers-view-usage-label">
                        {{ formatProviderNumber(item.requestCount) }} 次请求
                      </span>
                    </div>
                    <div class="providers-view-usage-row-side">
                      <strong class="providers-view-usage-value">
                        <TokenCount :value="item.actualTokens" />
                      </strong>
                      <span class="providers-view-usage-label">
                        {{ formatProviderCost(item.totalCostUsd) }}
                      </span>
                      <span class="providers-view-usage-label">
                        今日 <TokenCount :value="item.todayActualTokens" />
                      </span>
                    </div>
                  </article>
                </div>
                <div v-else class="providers-view__drawer-empty">
                  暂无用量记录。
                </div>
              </section>
              <div v-if="usageStatsLoading" class="providers-view-usage-mask">
                <span class="providers-view-usage-mask-text">统计中...</span>
              </div>
            </div>
          </template>
        </div>
      </aside>
    </div>

    <BaseModal
      v-if="showRuntimeConfig"
      class="providers-view__runtime-config-modal"
      title="当前系统配置"
      :description="runtimeConfigDescription"
      @close="closeRuntimeConfigDialog"
    >
      <section class="providers-view__runtime-config-panel">
        <pre
          v-if="runtimeConfigContent"
          class="providers-view__drawer-json providers-view__runtime-config-json"
          >{{ runtimeConfigContent }}</pre
        >
        <p v-else class="providers-view__drawer-empty">当前系统配置为空</p>
      </section>
    </BaseModal>

    <BaseModal
      v-if="showRuntimeDiff"
      class="providers-view__diff-modal"
      title="Runtime Diff"
      :description="runtimeDiffDescription"
      @close="closeRuntimeDiffDialog"
    >
      <div ref="runtimeDiffEditorRef" class="providers-view__diff-editor"></div>
      <footer class="providers-view__diff-footer">
        <button
          class="providers-view__diff-button"
          type="button"
          :disabled="pending"
          @click="resolveRuntimeDiff('cancel')"
        >
          取消启用
        </button>
        <button
          class="providers-view__diff-button"
          type="button"
          :disabled="pending"
          @click="resolveRuntimeDiff('runtime')"
        >
          用 CLI 配置更新管理器
        </button>
        <button
          class="providers-view__diff-button providers-view__diff-button--primary"
          type="button"
          :disabled="pending"
          @click="resolveRuntimeDiff('manager')"
        >
          用管理器配置覆盖 CLI
        </button>
      </footer>
    </BaseModal>
  </section>
</template>

<script setup>
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch
} from "vue"
import * as monaco from "monaco-editor/esm/vs/editor/editor.api"
import {
  ArrowLeft,
  Ban,
  Check,
  Copy,
  Globe2,
  GripVertical,
  Eye,
  EyeOff,
  KeyRound,
  Play,
  Plus,
  RefreshCw,
  Save,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  SquareTerminal,
  SquarePen,
  Trash2,
  Upload,
  X
} from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import BaseModal from "@/components/BaseModal.vue"
import TokenCount from "@/components/TokenCount.vue"
import CodexProxyPanel from "@/features/providers/components/CodexProxyPanel.vue"
import { accountApi, runtimeApi, systemApi, usageApi } from "@/api"
import { formatTokenCount } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  codexAccounts: {
    type: Array,
    required: true
  },
  codexLoginState: {
    type: Object,
    default: null
  },
  claudeProxyState: {
    type: Object,
    default: () => ({
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: []
    })
  },
  codexProxyState: {
    type: Object,
    default: () => ({
      enabled: false,
      localBaseUrl: "",
      activeProviderId: "",
      failoverProviderIds: [],
      accountModel: ""
    })
  },
  cliTargets: {
    type: Array,
    required: true
  },
  pending: {
    type: Boolean,
    required: true
  },
  providers: {
    type: Array,
    required: true
  },
  usage: {
    type: Object,
    default: () => ({})
  },
  runtimeConfigSchemas: {
    type: Object,
    required: true
  },
  runtimeModels: {
    type: Array,
    required: true
  },
  runtimeProviderState: {
    type: Object,
    required: true
  },
  runtimeProfiles: {
    type: Array,
    required: true
  }
})

const emit = defineEmits([
  "claude-proxy-disable",
  "claude-proxy-enable",
  "claude-proxy-provider-activate",
  "claude-proxy-provider-add",
  "claude-proxy-provider-remove",
  "claude-provider-instance-launch",
  "codex-auth-json-import",
  "codex-account-clear",
  "codex-account-enable",
  "codex-account-delete",
  "codex-account-disable",
  "codex-account-proxy-save",
  "codex-account-refresh",
  "codex-account-restore",
  "codex-accounts-refresh",
  "codex-official-login",
  "codex-proxy-enable",
  "codex-proxy-disable",
  "codex-proxy-provider-add",
  "codex-proxy-provider-remove",
  "codex-proxy-provider-activate",
  "codex-proxy-account-model-save",
  "codex-provider-instance-launch",
  "clear-runtime",
  "cancel-codex-official-login",
  "delete-provider",
  "resolve-runtime-drift",
  "save-provider",
  "switch-runtime"
])

const providerTypes = [
  "anthropic",
  "openai",
  "gemini",
  "open" + "router",
  "deep" + "seek",
  "custom"
]

const providerTypeLabelMap = {
  anthropic: "Anthropic Messages（原生）",
  openai: "OpenAI Chat Completions（需开启路由）",
  gemini: "Gemini Native generateContent（需开启路由）",
  custom: "Custom"
}

const draft = reactive({
  id: "",
  cli: "",
  icon: "",
  name: "",
  note: "",
  website: "",
  type: "anthropic",
  baseUrl: "",
  proxy: "",
  apiKey: "",
  apiKeys: [],
  activeApiKeyId: "",
  authField: "ANTHROPIC_AUTH_TOKEN",
  enabled: true,
  hideAiSignature: false,
  teammatesMode: true,
  toolSearch: false,
  maxThinking: true,
  disableUpgrade: false,
  modelContextWindowEnabled: false,
  serviceTierFast: false,
  modelReasoningEffort: "low",
  modelAutoCompactTokenLimit: 900000
})

const modelDrafts = reactive({
  mainModel: "",
  haikuModel: "",
  sonnetModel: "",
  opusModel: ""
})

const activeCli = ref("")
const viewMode = ref("list")
const showIconPicker = ref(false)
const showCodexCreateOptions = ref(false)
const showProviderCreateModal = ref(false)
const showApiKeyManager = ref(false)
const showCodexLoginModal = ref(false)
const showCodexProxyModal = ref(false)
const showCodexAccountDrawer = ref(false)
const showProviderDrawer = ref(false)
const showRuntimeConfig = ref(false)
const showRuntimeDiff = ref(false)
const runtimeDiffEditorRef = ref(null)
const runtimeConfigContent = ref("")
const runtimeConfigPath = ref("")
const runtimeDiffPath = ref("")
const codexLoginTab = ref("oauth")
const codexAuthDataDraft = ref("")
const codexProxyDraft = ref("")
const codexAuthUpdateAccountId = ref("")
const codexAccountProxyDrafts = reactive({})
const codexAccountRefreshingMap = reactive({})
const editingCodexAccountId = ref("")
const editingCodexProxy = ref("")
const codexAccountDetail = ref(null)
const codexAccountDetailTarget = ref(null)
const codexAccountDetailLoading = ref(false)
const codexAccountDetailTab = ref("config")
const usageStats = ref(props.usage || {})
const usageStatsLoading = ref(false)
const usageStatsTarget = ref("")
const proxyPanelRef = ref(null)
const showProxyAddAction = ref(false)
const showProxyManager = ref(false)
const proxyTab = ref("proxy")
const proxySwitchEnabled = ref(false)
// 控制列表是否包含已禁用的 Provider 与官方账号。
const showDisabledItems = ref(false)
const providerDetail = ref(null)
const providerDetailTab = ref("config")
const apiKeyManagerProvider = ref(null)
const apiKeyManagerDraft = reactive({
  providerId: "",
  apiKey: "",
  apiKeys: [],
  activeApiKeyId: ""
})
const manualCallbackUrl = ref("")
const countdownNow = ref(Date.now())
let countdownTimer = null
const iconKeyword = ref("")
const iconModules = import.meta.glob("/src/assets/ai-icons/*.svg", {
  query: "?url",
  import: "default"
})
const iconOptions = Object.keys(iconModules)
  .map((item) => item.split("/").pop())
  .sort((left, right) => left.localeCompare(right))
let runtimeDiffEditor = null

const visibleCliTargets = computed(() => {
  return props.cliTargets.filter((item) => {
    return props.runtimeConfigSchemas[item.id]?.enabled
  })
})

const activeRuntimeSchema = computed(() => {
  return (
    props.runtimeConfigSchemas[activeCli.value] || {
      modelFields: [],
      optionFields: [],
      advancedFields: [],
      configFiles: [],
      authFields: [],
      defaultProviderType: "custom"
    }
  )
})

function runtimeFieldOptions(field) {
  // 仅 Codex 5.6 Sol 和 Terra 模型支持 ultra 思考强度。
  if (
    activeCli.value === "codex" &&
    field.key === "modelReasoningEffort" &&
    isCodexFiveSixSolOrTerraModel(modelDrafts.mainModel)
  ) {
    return ["low", "medium", "high", "xhigh", "ultra"]
  }

  return field.options || []
}

function isCodexFiveSixSolOrTerraModel(model) {
  return ["gpt-5.6-sol", "gpt-5.6-terra"].includes(
    String(model || "")
      .trim()
      .toLowerCase()
  )
}

function normalizeModelReasoningEffort(model, effort) {
  // 模型切换后避免保存当前模型不支持的思考强度。
  const options = isCodexFiveSixSolOrTerraModel(model)
    ? ["low", "medium", "high", "xhigh", "ultra"]
    : ["low", "medium", "high", "xhigh"]

  return options.includes(effort) ? effort : "low"
}

const activeCliName = computed(() => {
  return (
    visibleCliTargets.value.find((item) => item.id === activeCli.value)?.name ||
    activeCli.value ||
    "Runtime"
  )
})

const activeDraftApiKey = computed(() => {
  return draft.apiKeys.find((item) => item.id === draft.activeApiKeyId) || null
})

const scopedProviders = computed(() => {
  return props.providers.filter((item) => item.cli === activeCli.value)
})

// 统计当前 CLI 下可按需展示的禁用项。
const disabledItemCount = computed(() => {
  const disabledProviderCount = scopedProviders.value.filter(
    (provider) => provider.enabled === false
  ).length
  const disabledAccountCount =
    activeCli.value === "codex"
      ? props.codexAccounts.filter((account) => account.disabled).length
      : 0

  return disabledProviderCount + disabledAccountCount
})

const mixedItems = computed(() => {
  const providerItems = scopedProviders.value
    .filter(
      (provider) => showDisabledItems.value || provider.enabled !== false
    )
    .map((provider) => ({
      type: "provider",
      provider,
      key: `provider:${provider.id}`,
      className: [
        "providers-view__provider-card",
        {
          "providers-view__provider-card--active":
            profileMap.value[activeCli.value]?.providerId === provider.id,
          "providers-view__provider-card--runtime-warning":
            showRuntimeWarning.value &&
            profileMap.value[activeCli.value]?.providerId === provider.id,
          "providers-view__provider-card--disabled": provider.enabled === false
        }
      ],
      createdAt: provider.createdAt || 0
    }))
  const accountItems =
    activeCli.value === "codex"
      ? props.codexAccounts
          .filter((account) => showDisabledItems.value || !account.disabled)
          .map((account) => ({
            type: "account",
            account,
            key: `account:${account.id}`,
            className: [
              "providers-view__account-card",
              {
                "providers-view__account-card--active": account.active,
                "providers-view__account-card--refreshing":
                  codexAccountRefreshingMap[account.id],
                "providers-view__account-card--error":
                  account.refresh_status === "failed",
                "providers-view__account-card--disabled": account.disabled
              }
            ],
            createdAt: account.createdAt || account.updatedAt || 0
          }))
      : []

  return [...providerItems, ...accountItems].sort(
    (left, right) => right.createdAt - left.createdAt
  )
})

const profileMap = computed(() => {
  return Object.fromEntries(
    props.runtimeProfiles.map((item) => [item.cli, item])
  )
})

const runtimeState = computed(() => {
  return props.runtimeProviderState[activeCli.value] || {}
})

const activeProxyState = computed(() => {
  if (activeCli.value === "claude") {
    return props.claudeProxyState
  }

  if (activeCli.value === "codex") {
    return props.codexProxyState
  }

  return null
})

const activeProxyEnabled = computed(() => {
  return Boolean(activeProxyState.value?.enabled)
})

const activeProxyProviderIds = computed(() => {
  return activeProxyState.value?.failoverProviderIds || []
})

const runtimeStatus = computed(() => {
  return runtimeState.value.status || "NO_ACTIVE"
})

const showRuntimeWarning = computed(() => {
  return ["MODIFIED_EXTERNALLY", "DIRTY_MANAGER", "CONFLICT"].includes(
    runtimeStatus.value
  )
})

const runtimeDiffDescription = computed(() => {
  return runtimeDiffPath.value || "当前没有 CLI 配置文件路径"
})

const runtimeConfigDescription = computed(() => {
  return runtimeConfigPath.value || "当前没有 CLI 配置文件路径"
})

const filteredIconOptions = computed(() => {
  const keyword = iconKeyword.value.toLowerCase()

  return iconOptions.filter((item) =>
    iconLabel(item).toLowerCase().includes(keyword)
  )
})

const codexLoginTitle = computed(() => {
  return codexAuthUpdateAccountId.value ? "更新认证信息" : "添加 Codex 账号"
})

const configPreviewMap = computed(() => {
  return Object.fromEntries(
    activeRuntimeSchema.value.configFiles.map((file) => [
      file.name,
      formatConfigPreview(file, applyConfigTemplate(file.template))
    ])
  )
})

const emptyUsageSummary = {
  requestCount: 0,
  inputTokens: 0,
  outputTokens: 0,
  cacheReadTokens: 0,
  cacheCreationTokens: 0,
  actualTokens: 0,
  totalCostUsd: 0
}

// 额度阶段按重置时间持久化，当前阶段和历史阶段分别展示。
const codexCurrentQuotaStages = computed(() => {
  return (codexAccountDetail.value?.quotaStages || []).filter(
    stage => stage.active
  )
})

const codexQuotaStageHistory = computed(() => {
  return (codexAccountDetail.value?.quotaStages || [])
    .filter(stage => !stage.active)
    .slice(0, 12)
})

const codexAccountUsageProviderIds = computed(() => {
  if (!codexAccountDetailTarget.value) {
    return []
  }

  return [
    `codex-account:${codexAccountDetailTarget.value.id}`,
    `account:${codexAccountDetailTarget.value.id}`
  ]
})

const codexAccountUsageTarget = computed(() => {
  if (!codexAccountDetailTarget.value) {
    return ""
  }

  return `codex-account:${codexAccountDetailTarget.value.id}`
})

const codexAccountUsageLogs = computed(() => {
  if (usageStatsTarget.value === codexAccountUsageTarget.value) {
    return []
  }

  return (usageStats.value.logs || []).filter((item) =>
    codexAccountUsageProviderIds.value.includes(item.providerId)
  )
})

const codexAccountUsageSummary = computed(() => {
  if (usageStatsTarget.value === codexAccountUsageTarget.value) {
    return usageStats.value.summary || emptyUsageSummary
  }

  return mergeUsageSummaries(
    (usageStats.value.providerStats || []).filter((item) =>
      codexAccountUsageProviderIds.value.includes(item.providerId)
    )
  )
})

const codexAccountUsageTodaySummary = computed(() => {
  if (usageStatsTarget.value === codexAccountUsageTarget.value) {
    return usageStats.value.todaySummary || emptyUsageSummary
  }

  return mergeUsageLogSummaries(
    codexAccountUsageLogs.value.filter((item) => isTodayUsageLog(item))
  )
})

const codexAccountUsageTodayModelStats = computed(() => {
  if (usageStatsTarget.value === codexAccountUsageTarget.value) {
    return mergeUsageModelStats(usageStats.value.todayModelStats || [])
  }

  return mergeUsageLogModelStats(
    codexAccountUsageLogs.value.filter((item) => isTodayUsageLog(item))
  )
})

const codexAccountUsageModelStats = computed(() => {
  return mergeUsageModelStats(
    (usageStats.value.modelStats || []).filter((item) => {
      return (
        usageStatsTarget.value === codexAccountUsageTarget.value ||
        codexAccountUsageProviderIds.value.includes(item.providerId)
      )
    }),
    codexAccountUsageTodayModelStats.value
  )
})

const providerUsageProviderId = computed(() => {
  if (!providerDetail.value) {
    return ""
  }

  return providerDetail.value.id
})

const providerUsageTarget = computed(() => {
  if (!providerUsageProviderId.value) {
    return ""
  }

  return `provider:${providerUsageProviderId.value}`
})

const providerUsageLogs = computed(() => {
  if (usageStatsTarget.value === providerUsageTarget.value) {
    return []
  }

  return (usageStats.value.logs || []).filter((item) => {
    return item.providerId === providerUsageProviderId.value
  })
})

const providerUsageSummary = computed(() => {
  if (usageStatsTarget.value === providerUsageTarget.value) {
    return usageStats.value.summary || emptyUsageSummary
  }

  return (
    (usageStats.value.providerStats || []).find((item) => {
      return item.providerId === providerUsageProviderId.value
    }) || emptyUsageSummary
  )
})

const providerUsageTodaySummary = computed(() => {
  if (usageStatsTarget.value === providerUsageTarget.value) {
    return usageStats.value.todaySummary || emptyUsageSummary
  }

  return mergeUsageLogSummaries(
    providerUsageLogs.value.filter((item) => isTodayUsageLog(item))
  )
})

const providerUsageTodayModelStats = computed(() => {
  if (usageStatsTarget.value === providerUsageTarget.value) {
    return mergeUsageModelStats(usageStats.value.todayModelStats || [])
  }

  return mergeUsageLogModelStats(
    providerUsageLogs.value.filter((item) => isTodayUsageLog(item))
  )
})

const providerUsageModelStats = computed(() => {
  return mergeUsageModelStats(
    (usageStats.value.modelStats || []).filter((item) => {
      return (
        usageStatsTarget.value === providerUsageTarget.value ||
        item.providerId === providerUsageProviderId.value
      )
    }),
    providerUsageTodayModelStats.value
  )
})

onMounted(() => {
  countdownTimer = window.setInterval(() => {
    countdownNow.value = Date.now()
  }, 1000)
})

onBeforeUnmount(() => {
  window.clearInterval(countdownTimer)

  if (runtimeDiffEditor) {
    runtimeDiffEditor.dispose()
  }
})

function formatConfigPreview(file, content) {
  if (file.format !== "JSON") {
    return content
  }

  return JSON.stringify(JSON.parse(content), null, 2)
}

function applyConfigTemplate(template) {
  const activeApiKey = activeDraftApiKey.value?.apiKey || draft.apiKey
  const values = {
    authField: draft.authField,
    apiKey: activeApiKey,
    hasApiKey: Boolean(activeApiKey || activeDraftApiKey.value?.masked),
    baseUrl: draft.baseUrl,
    hasBaseUrl: Boolean(draft.baseUrl),
    mainModel: modelDrafts.mainModel,
    hasMainModel: Boolean(modelDrafts.mainModel),
    haikuModel: modelDrafts.haikuModel,
    hasHaikuModel: Boolean(modelDrafts.haikuModel),
    sonnetModel: modelDrafts.sonnetModel,
    hasSonnetModel: Boolean(modelDrafts.sonnetModel),
    opusModel: modelDrafts.opusModel,
    hasOpusModel: Boolean(modelDrafts.opusModel),
    toolSearch: draft.toolSearch,
    toolSearchText: draft.toolSearch ? "true" : "false",
    disableUpgrade: draft.disableUpgrade,
    disableUpgradeText: draft.disableUpgrade ? "1" : "0",
    includeCoAuthoredBy: String(!draft.hideAiSignature),
    hideAiSignature: draft.hideAiSignature,
    teammatesMode: draft.teammatesMode,
    teammateMode: "tmux",
    effortLevel: draft.maxThinking ? "max" : "default",
    modelContextWindowEnabled: draft.modelContextWindowEnabled,
    serviceTierFast: draft.serviceTierFast,
    modelReasoningEffort: draft.modelReasoningEffort,
    modelAutoCompactTokenLimit: draft.modelAutoCompactTokenLimit || 900000
  }

  return String(template || "")
    .replace(/\{\{#(\w+)}}([\s\S]*?)\{\{\/\1}}/g, (match, key, content) =>
      values[key] ? content : ""
    )
    .replace(/\{\{(\w+)}}/g, (match, key) => {
      return values[key] ?? match
    })
    .replace(/,(\s*[}\]])/g, "$1")
    .replace(/^[\t ]*\r?\n/gm, "")
}

function ensureActiveCli() {
  if (visibleCliTargets.value.find((item) => item.id === activeCli.value)) {
    return
  }

  activeCli.value = visibleCliTargets.value[0]?.id || ""
}

function selectCli(cli) {
  const previousCli = activeCli.value
  activeCli.value = cli
  closeCodexAccountDetail()
  closeProviderDetail()
  closeApiKeyManager()
  closeProviderCreateModal()
  clearDraft()

  if (cli === "codex" && previousCli !== "codex") {
    refreshCodexAccounts()
  }
}

function editProvider(provider) {
  closeCodexAccountDetail()
  closeProviderDetail()
  draft.id = provider.id
  draft.cli = provider.cli || activeCli.value
  draft.icon = provider.icon || ""
  draft.name = provider.name
  draft.note = provider.note || ""
  draft.website = provider.website || ""
  draft.type = provider.type
  draft.baseUrl = provider.baseUrl || ""
  draft.proxy = provider.proxy || ""
  draft.apiKey = provider.apiKey || ""
  draft.apiKeys = normalizeApiKeyDrafts(provider)
  draft.activeApiKeyId =
    provider.activeApiKeyId || draft.apiKeys[0]?.id || ""
  const activeApiKey = draft.apiKeys.find(
    (item) => item.id === draft.activeApiKeyId
  )
  if (activeApiKey && provider.apiKey) {
    activeApiKey.apiKey = provider.apiKey
  }
  draft.authField = provider.authField || "ANTHROPIC_AUTH_TOKEN"
  draft.enabled = provider.enabled !== false
  modelDrafts.mainModel = provider.runtimeConfig?.mainModel || ""
  modelDrafts.haikuModel = provider.runtimeConfig?.haikuModel || ""
  modelDrafts.sonnetModel = provider.runtimeConfig?.sonnetModel || ""
  modelDrafts.opusModel = provider.runtimeConfig?.opusModel || ""
  draft.hideAiSignature = Boolean(provider.runtimeConfig?.hideAiSignature)
  draft.teammatesMode = provider.runtimeConfig?.teammatesMode !== false
  draft.toolSearch = Boolean(provider.runtimeConfig?.toolSearch)
  draft.maxThinking = provider.runtimeConfig?.maxThinking !== false
  draft.disableUpgrade = Boolean(provider.runtimeConfig?.disableUpgrade)
  draft.modelContextWindowEnabled = Boolean(
    provider.runtimeConfig?.modelContextWindowEnabled
  )
  draft.serviceTierFast = Boolean(provider.runtimeConfig?.serviceTierFast)
  draft.modelReasoningEffort =
    provider.runtimeConfig?.modelReasoningEffort || "low"
  draft.modelAutoCompactTokenLimit =
    provider.runtimeConfig?.modelAutoCompactTokenLimit || 900000
  showIconPicker.value = false
  iconKeyword.value = ""
  showProviderCreateModal.value = true
}

function createProvider() {
  if (activeCli.value === "codex") {
    showCodexCreateOptions.value = true
    return
  }

  startProviderCreate()
}

function startProviderCreate() {
  showCodexCreateOptions.value = false
  closeCodexAccountDetail()
  closeProviderDetail()
  clearDraft()
  showProviderCreateModal.value = true
}

function closeProviderCreateModal() {
  showProviderCreateModal.value = false
  showIconPicker.value = false
  iconKeyword.value = ""
}

function openCodexLoginModal(account = null) {
  showCodexCreateOptions.value = false
  showCodexLoginModal.value = true
  closeCodexAccountDetail()
  closeProviderDetail()
  codexLoginTab.value = "oauth"
  manualCallbackUrl.value = ""
  codexAuthDataDraft.value = ""
  codexAuthUpdateAccountId.value = account?.id || ""
  codexProxyDraft.value = account?.proxy || ""
}

function openCodexAuthUpdateModal(account) {
  openCodexLoginModal(account)
}

function closeCodexLoginModal() {
  showCodexLoginModal.value = false
  codexAuthUpdateAccountId.value = ""
  closeCodexAccountDetail()
  closeProviderDetail()
  emit("cancel-codex-official-login")
}

function startCodexOfficialLogin() {
  emit("codex-official-login", {
    accountId: codexAuthUpdateAccountId.value,
    proxy: codexProxyDraft.value
  })
}

async function copyAuthUrl() {
  try {
    await navigator.clipboard.writeText(props.codexLoginState.authUrl || "")
    createMessage.success("授权链接已复制。")
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

async function openManualCallbackUrl() {
  try {
    await systemApi.openExternal({ url: manualCallbackUrl.value })
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

function importCodexAuthData() {
  emit("codex-auth-json-import", {
    accountId: codexAuthUpdateAccountId.value,
    content: codexAuthDataDraft.value,
    proxy: codexProxyDraft.value
  })
}

function enableCodexAccount(account) {
  emit("codex-account-enable", {
    accountId: account.id
  })
}

function clearCodexAccount() {
  emit("codex-account-clear")
}

function deleteCodexAccount(account) {
  const shouldContinue = window.confirm(
    "删除官方账号后会清除本地 auth.json，是否继续？"
  )

  if (!shouldContinue) {
    return
  }

  emit("codex-account-delete", {
    accountId: account.id
  })
}

function openCodexAccountProxy(account) {
  editingCodexAccountId.value = account.id
  editingCodexProxy.value =
    codexAccountProxyDrafts[account.id] ?? account.proxy ?? ""
  showCodexProxyModal.value = true
}

function closeCodexAccountProxy() {
  showCodexProxyModal.value = false
  editingCodexAccountId.value = ""
  editingCodexProxy.value = ""
}

function saveCodexAccountProxy() {
  codexAccountProxyDrafts[editingCodexAccountId.value] = editingCodexProxy.value
  emit("codex-account-proxy-save", {
    accountId: editingCodexAccountId.value,
    proxy: editingCodexProxy.value
  })
  closeCodexAccountProxy()
}

async function openCodexAccountDetail(account) {
  closeProviderDetail()
  codexAccountDetailTarget.value = account
  codexAccountDetailTab.value = "config"
  showCodexAccountDrawer.value = true
  codexAccountDetailLoading.value = true
  codexAccountDetail.value = null

  try {
    const result = await accountApi.getCodexAccountDetail({
      accountId: account.id
    })

    codexAccountDetail.value = result?.data || null
  } catch (error) {
    createMessage.error(error.message || String(error))
    closeCodexAccountDetail()
  } finally {
    codexAccountDetailLoading.value = false
  }
}

function closeCodexAccountDetail() {
  showCodexAccountDrawer.value = false
  codexAccountDetail.value = null
  codexAccountDetailTarget.value = null
  codexAccountDetailLoading.value = false
  codexAccountDetailTab.value = "config"
}

function openProviderDetail(provider) {
  closeCodexAccountDetail()
  providerDetail.value = provider
  providerDetailTab.value = "config"
  showProviderDrawer.value = true
}

function closeProviderDetail() {
  showProviderDrawer.value = false
  providerDetail.value = null
  providerDetailTab.value = "config"
}

function openApiKeyManager(provider) {
  closeCodexAccountDetail()
  closeProviderDetail()
  apiKeyManagerProvider.value = provider
  apiKeyManagerDraft.providerId = provider.id
  apiKeyManagerDraft.apiKey = provider.apiKey || ""
  apiKeyManagerDraft.apiKeys = normalizeApiKeyDrafts(provider)
  apiKeyManagerDraft.activeApiKeyId =
    provider.activeApiKeyId || apiKeyManagerDraft.apiKeys[0]?.id || ""

  const activeApiKey = apiKeyManagerDraft.apiKeys.find(
    (item) => item.id === apiKeyManagerDraft.activeApiKeyId
  )
  if (activeApiKey && provider.apiKey) {
    activeApiKey.apiKey = provider.apiKey
  }

  showApiKeyManager.value = true
}

function closeApiKeyManager() {
  showApiKeyManager.value = false
  apiKeyManagerProvider.value = null
  apiKeyManagerDraft.providerId = ""
  apiKeyManagerDraft.apiKey = ""
  apiKeyManagerDraft.apiKeys = []
  apiKeyManagerDraft.activeApiKeyId = ""
}

function saveApiKeyManager() {
  if (
    apiKeyManagerDraft.apiKeys.some((item) => !item.apiKey && !item.masked)
  ) {
    createMessage.error("请填写新增的 API Key，或删除空白项。")
    return
  }

  emit(
    "save-provider",
    {
      ...apiKeyManagerProvider.value,
      id: apiKeyManagerDraft.providerId,
      apiKeys: apiKeyManagerDraft.apiKeys.map((item) => ({
        id: item.id,
        name: item.name,
        note: item.note,
        ...(item.apiKey ? { apiKey: item.apiKey } : {})
      })),
      activeApiKeyId: apiKeyManagerDraft.activeApiKeyId
    },
    closeApiKeyManager
  )
}

async function ensureUsageStatsReady() {
  let target = ""
  let payload = {}

  if (
    codexAccountDetailTab.value === "usage" &&
    codexAccountUsageTarget.value
  ) {
    target = codexAccountUsageTarget.value
    payload = {
      statsScope: "provider",
      providerIds: codexAccountUsageProviderIds.value
    }
  } else if (providerDetailTab.value === "usage" && providerUsageTarget.value) {
    target = providerUsageTarget.value
    payload = {
      statsScope: "provider",
      providerId: providerUsageProviderId.value
    }
  }

  if (!target || usageStatsLoading.value || usageStatsTarget.value === target) {
    return
  }

  usageStatsLoading.value = true

  try {
    const result = await usageApi.getUsageStats(payload)

    usageStats.value = result?.data || usageStats.value
    usageStatsTarget.value = target
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    usageStatsLoading.value = false

    if (
      (codexAccountDetailTab.value === "usage" &&
        target !== codexAccountUsageTarget.value) ||
      (providerDetailTab.value === "usage" &&
        target !== providerUsageTarget.value)
    ) {
      ensureUsageStatsReady()
    }
  }
}

function rateLimitWindows(rateLimit) {
  return [
    { key: "primary", window: rateLimit.primary_window },
    { key: "secondary", window: rateLimit.secondary_window }
  ].filter((item) => item.window)
}

function formatPlanName(value) {
  if (value === "pro") {
    return "Pro"
  }

  if (value === "plus") {
    return "Plus"
  }

  return value || "未识别套餐"
}

function formatRateWindowName(value) {
  const seconds = Number(value || 0)

  if (seconds === 18000) {
    return "五小时额度"
  }

  if (seconds === 604800) {
    return "周额度"
  }

  if (seconds % 86400 === 0) {
    return `${seconds / 86400}天额度`
  }

  if (seconds % 3600 === 0) {
    return `${seconds / 3600}小时额度`
  }

  return `${seconds}秒额度`
}

function formatRateWidth(value) {
  const percent = Number(value || 0)

  if (percent < 0) {
    return "0%"
  }

  if (percent > 100) {
    return "100%"
  }

  return `${percent}%`
}

function quotaLevelClass(window) {
  const remaining = 100 - (window.used_percent || 0)

  if (remaining < 10) {
    return "providers-view__account-quota--danger"
  }

  if (remaining < 30) {
    return "providers-view__account-quota--warning"
  }

  return ""
}

function formatResetCountdown(value) {
  const timestamp = Number(value || 0)
  const seconds = Math.max(
    0,
    Math.floor(
      ((timestamp > 1e12 ? timestamp : timestamp * 1000) - countdownNow.value) /
        1000
    )
  )
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)

  return days ? `${days}d ${hours}h ${minutes}m` : `${hours}h ${minutes}m`
}

function formatUnixTime(value) {
  const timestamp = Number(value || 0)
  const normalizedTimestamp = timestamp > 1e12 ? timestamp : timestamp * 1000

  if (!timestamp) {
    return "重置时间未知"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(normalizedTimestamp))
}

function formatQuotaPercent(value) {
  const percent = Math.min(100, Math.max(0, Number(value || 0)))

  return Number.isInteger(percent) ? String(percent) : percent.toFixed(1)
}

function formatQuotaStageRange(stage) {
  return `${formatUnixTime(stage.startsAt)} - ${formatUnixTime(stage.resetAt)}`
}

function formatJson(value) {
  return JSON.stringify(value || {}, null, 2)
}

function mergeUsageSummaries(items) {
  return items.reduce(
    (result, item) => {
      result.requestCount += Number(item.requestCount || 0)
      result.inputTokens += Number(item.inputTokens || 0)
      result.outputTokens += Number(item.outputTokens || 0)
      result.cacheReadTokens += Number(item.cacheReadTokens || 0)
      result.cacheCreationTokens += Number(item.cacheCreationTokens || 0)
      result.actualTokens += Number(item.actualTokens || 0)
      result.totalCostUsd += Number(item.totalCostUsd || 0)
      return result
    },
    { ...emptyUsageSummary }
  )
}

function mergeUsageLogSummaries(items) {
  return items.reduce(
    (result, item) => {
      result.requestCount += 1
      result.inputTokens += usageInputTokens(item)
      result.outputTokens += Number(item.outputTokens || 0)
      result.cacheReadTokens += Number(item.cacheReadTokens || 0)
      result.cacheCreationTokens += Number(item.cacheCreationTokens || 0)
      result.actualTokens += Number(item.actualTokens || 0)
      result.totalCostUsd += Number(item.totalCostUsd || 0)
      return result
    },
    { ...emptyUsageSummary }
  )
}

function mergeUsageLogModelStats(items) {
  return mergeUsageModelStats(
    items.map((item) => ({
      appType: item.appType,
      model: item.model || "未识别模型",
      requestCount: 1,
      actualTokens: Number(item.actualTokens || 0),
      totalCostUsd: Number(item.totalCostUsd || 0)
    }))
  )
}

function mergeUsageModelStats(items, todayItems = []) {
  const groups = new Map()
  const todayGroups = new Map()

  for (const item of todayItems) {
    const key = `${item.appType || ""}:${item.model || "未识别模型"}`

    todayGroups.set(key, {
      requestCount:
        (todayGroups.get(key)?.requestCount || 0) +
        Number(item.requestCount || 0),
      actualTokens:
        (todayGroups.get(key)?.actualTokens || 0) +
        Number(item.actualTokens || 0),
      totalCostUsd:
        (todayGroups.get(key)?.totalCostUsd || 0) +
        Number(item.totalCostUsd || 0)
    })
  }

  for (const item of items) {
    const key = `${item.appType || ""}:${item.model || "未识别模型"}`
    const current = groups.get(key) || {
      ...item,
      model: item.model || "未识别模型",
      requestCount: 0,
      actualTokens: 0,
      totalCostUsd: 0
    }

    current.requestCount += Number(item.requestCount || 0)
    current.actualTokens += Number(item.actualTokens || 0)
    current.totalCostUsd += Number(item.totalCostUsd || 0)
    groups.set(key, current)
  }

  return Array.from(groups.entries())
    .map(([key, item]) => ({
      ...item,
      todayRequestCount: todayGroups.get(key)?.requestCount || 0,
      todayActualTokens: todayGroups.get(key)?.actualTokens || 0,
      todayTotalCostUsd: todayGroups.get(key)?.totalCostUsd || 0
    }))
    .sort((left, right) => right.actualTokens - left.actualTokens)
}

function isTodayUsageLog(item) {
  const date = new Date(Number(item.createdAt || 0))
  const now = new Date()

  return (
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate()
  )
}

function usageInputTokens(item) {
  if (item.appType === "codex" || item.appType === "gemini") {
    return Math.max(
      0,
      Number(item.inputTokens || 0) - Number(item.cacheReadTokens || 0)
    )
  }

  return Number(item.inputTokens || 0)
}

function formatProviderNumber(value) {
  return new Intl.NumberFormat("zh-CN").format(Number(value || 0))
}

function formatProviderCost(value) {
  const cost = Number(value || 0)

  if (!cost) {
    return "$0"
  }

  return `$${cost >= 1 ? cost.toFixed(2) : cost.toFixed(6)}`
}

function isCodexAccountRefreshing(account) {
  return Boolean(codexAccountRefreshingMap[account.id])
}

function refreshCodexAccounts() {
  props.codexAccounts.forEach((account) => {
    if (account.disabled) {
      return
    }

    refreshCodexAccount(account, {
      showSuccess: false,
      syncAuth: false
    })
  })
}

function refreshCodexAccount(account, options = {}) {
  if (account.disabled) {
    return
  }

  if (codexAccountRefreshingMap[account.id]) {
    return
  }

  codexAccountRefreshingMap[account.id] = true
  emit("codex-account-refresh", {
    accountId: account.id,
    showSuccess: options.showSuccess !== false,
    syncAuth: options.syncAuth,
    onSettled: () => {
      codexAccountRefreshingMap[account.id] = false
    }
  })
}

// 已保存的 Key 只保留标识和掩码，留空时由后端复用原密文。
function createApiKeyDraft(index, item = {}) {
  return {
    id:
      item.id ||
      `key-${Date.now()}-${index}-${Math.random().toString(36).slice(2, 8)}`,
    name: item.name || `Key ${index + 1}`,
    note: item.note || "",
    apiKey: item.apiKey || "",
    masked: item.masked || ""
  }
}

function normalizeApiKeyDrafts(provider) {
  const items = Array.isArray(provider.apiKeys) ? provider.apiKeys : []

  if (items.length) {
    return items.map((item, index) => createApiKeyDraft(index, item))
  }

  if (provider.apiKey || provider.hasApiKey) {
    return [
      createApiKeyDraft(0, {
        id: provider.activeApiKeyId || "default",
        name: "默认 Key",
        apiKey: provider.apiKey || "",
        masked: provider.apiKey ? maskApiKey(provider.apiKey) : "已保存"
      })
    ]
  }

  return [createApiKeyDraft(0)]
}

function maskApiKey(value) {
  const text = String(value || "")

  if (text.length <= 8) {
    return "••••••••"
  }

  return `${text.slice(0, 4)}••••${text.slice(-4)}`
}

function addApiKey(target = draft) {
  const item = createApiKeyDraft(target.apiKeys.length)
  target.apiKeys.push(item)

  if (!target.activeApiKeyId) {
    activateApiKey(item.id, target)
  }
}

function activateApiKey(id, target = draft) {
  target.activeApiKeyId = id
  target.apiKey = target.apiKeys.find((item) => item.id === id)?.apiKey || ""
}

function removeApiKey(index, target = draft) {
  const removed = target.apiKeys.splice(index, 1)[0]

  if (removed?.id !== target.activeApiKeyId) {
    return
  }

  if (target.apiKeys.length) {
    activateApiKey(target.apiKeys[0].id, target)
  } else {
    target.activeApiKeyId = ""
    target.apiKey = ""
  }
}

function clearDraft() {
  draft.id = ""
  draft.cli = activeCli.value
  draft.icon = ""
  draft.name = ""
  draft.note = ""
  draft.website = ""
  draft.type = activeRuntimeSchema.value.defaultProviderType
  draft.baseUrl = ""
  draft.proxy = ""
  draft.apiKey = ""
  draft.apiKeys = [createApiKeyDraft(0)]
  draft.activeApiKeyId = draft.apiKeys[0].id
  draft.authField =
    activeRuntimeSchema.value.authFields[0] || "ANTHROPIC_AUTH_TOKEN"
  draft.enabled = true
  draft.hideAiSignature = false
  draft.teammatesMode = true
  draft.toolSearch = false
  draft.maxThinking = true
  draft.disableUpgrade = false
  draft.modelContextWindowEnabled = false
  draft.serviceTierFast = false
  draft.modelReasoningEffort = "low"
  draft.modelAutoCompactTokenLimit = 900000
  modelDrafts.mainModel = ""
  modelDrafts.haikuModel = ""
  modelDrafts.sonnetModel = ""
  modelDrafts.opusModel = ""
  showIconPicker.value = false
  iconKeyword.value = ""
}

function firstModelName(providerId) {
  return (
    props.runtimeModels.find((item) => item.providerId === providerId)?.name ||
    ""
  )
}

function iconLabel(icon) {
  if (/^(data:|https?:\/\/|file:|blob:)/i.test(String(icon || ""))) {
    return "自定义图标"
  }

  return String(icon || "").replace(/\.svg$/, "")
}

function selectIcon(icon) {
  draft.icon = icon
  showIconPicker.value = false
}

function uploadCustomIcon(event) {
  const input = event.target
  const file = input.files?.[0]

  if (!file) {
    return
  }

  if (
    !["image/svg+xml", "image/png", "image/jpeg", "image/webp"].includes(
      file.type
    )
  ) {
    createMessage.error("请选择 SVG、PNG、JPG 或 WebP 图标。")
    input.value = ""
    return
  }

  if (file.size > 1024 * 1024) {
    createMessage.error("图标文件不能超过 1MB。")
    input.value = ""
    return
  }

  const reader = new FileReader()

  reader.onload = () => {
    draft.icon = String(reader.result || "")
    showIconPicker.value = false
    input.value = ""
  }
  reader.onerror = () => {
    createMessage.error("图标读取失败。")
    input.value = ""
  }
  reader.readAsDataURL(file)
}

function submitProvider() {
  const payload = {
    id: draft.id || undefined,
    cli: draft.cli,
    icon: draft.icon,
    name: draft.name,
    note: draft.note,
    website: draft.website,
    type: draft.type,
    baseUrl: draft.baseUrl,
    proxy: draft.proxy,
    authField: draft.authField,
    apiKeys: draft.apiKeys.map((item) => ({
      id: item.id,
      name: item.name,
      note: item.note,
      ...(item.apiKey ? { apiKey: item.apiKey } : {})
    })),
    activeApiKeyId: draft.activeApiKeyId,
    model: modelDrafts.mainModel,
    runtimeConfig: {
      mainModel: modelDrafts.mainModel,
      haikuModel: modelDrafts.haikuModel,
      sonnetModel: modelDrafts.sonnetModel,
      opusModel: modelDrafts.opusModel,
      toolSearch: draft.toolSearch,
      disableUpgrade: draft.disableUpgrade,
      hideAiSignature: draft.hideAiSignature,
      teammatesMode: draft.teammatesMode,
      maxThinking: draft.maxThinking,
      modelContextWindowEnabled: draft.modelContextWindowEnabled,
      serviceTierFast: draft.serviceTierFast,
      modelReasoningEffort: normalizeModelReasoningEffort(
        modelDrafts.mainModel,
        draft.modelReasoningEffort
      ),
      modelAutoCompactTokenLimit: draft.modelAutoCompactTokenLimit
    },
    enabled: draft.enabled
  }

  emit("save-provider", payload)

  viewMode.value = "list"
  closeProviderCreateModal()
}

function enableProvider(provider) {
  if (provider.enabled === false) {
    return
  }

  const model = provider.runtimeConfig?.mainModel || firstModelName(provider.id)

  if (!model) {
    return
  }

  emit("switch-runtime", {
    cli: activeCli.value,
    providerId: provider.id,
    model
  })
}

function launchCodexProviderInstance(target) {
  if (activeCli.value !== "codex") {
    return
  }

  if (target.enabled === false || target.disabled === true) {
    return
  }

  emit("codex-provider-instance-launch", {
    ...(target.accountId
      ? {
          accountId: target.id
        }
      : {
          providerId: target.id
        })
  })
}

function launchClaudeProviderInstance(provider) {
  if (activeCli.value !== "claude" || provider.enabled === false) {
    return
  }

  emit("claude-provider-instance-launch", {
    providerId: provider.id
  })
}

function disableProvider(provider) {
  emit("save-provider", {
    ...provider,
    enabled: false
  })
}

function restoreProvider(provider) {
  emit("save-provider", {
    ...provider,
    enabled: true
  })
}

function disableCodexAccount(account) {
  emit("codex-account-disable", {
    accountId: account.id
  })
}

function restoreCodexAccount(account) {
  emit("codex-account-restore", {
    accountId: account.id
  })
}

function emitProxyEvent(action, payload) {
  emit(`${activeCli.value}-proxy-${action}`, payload)
}

function toggleProxySwitch(event) {
  proxySwitchEnabled.value = event.target.checked

  if (event.target.checked) {
    if (!activeProxyProviderIds.value.length) {
      proxySwitchEnabled.value = false
      event.target.checked = false
      showProxyAddAction.value = true
      createMessage.error("请先添加代理接管池")
      return
    }

    emitProxyEvent("enable", {})
    return
  }

  emitProxyEvent("disable")
}

function openProxyProviderPicker() {
  proxyPanelRef.value?.openProviderPicker()
}

function clearRuntime() {
  emit("clear-runtime", {
    cli: activeCli.value
  })
}

async function openRuntimeCompareDialog() {
  try {
    const result = await runtimeApi.compareRuntime({
      cli: activeCli.value
    })

    showRuntimeDiff.value = true
    runtimeDiffPath.value = result.runtimePath || ""
    await nextTick()
    renderRuntimeDiff(result.managerContent || "", result.runtimeContent || "")
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

async function openRuntimeConfigDialog() {
  try {
    const result = await runtimeApi.getRuntimeConfig({
      cli: activeCli.value
    })

    runtimeConfigPath.value = result.runtimePath || ""
    runtimeConfigContent.value = result.runtimeContent || ""
    showRuntimeConfig.value = true
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

function closeRuntimeConfigDialog() {
  showRuntimeConfig.value = false
  runtimeConfigContent.value = ""
  runtimeConfigPath.value = ""
}

function renderRuntimeDiff(managerContent, runtimeContent) {
  if (!runtimeDiffEditorRef.value) {
    return
  }

  if (runtimeDiffEditor) {
    runtimeDiffEditor.dispose()
  }

  runtimeDiffEditor = monaco.editor.createDiffEditor(
    runtimeDiffEditorRef.value,
    {
      automaticLayout: true,
      minimap: { enabled: false },
      readOnly: true,
      renderSideBySide: true,
      scrollBeyondLastLine: false
    }
  )
  runtimeDiffEditor.setModel({
    original: monaco.editor.createModel(managerContent, "plaintext"),
    modified: monaco.editor.createModel(runtimeContent, "plaintext")
  })
}

function closeRuntimeDiffDialog() {
  showRuntimeDiff.value = false
  runtimeDiffPath.value = ""

  if (runtimeDiffEditor) {
    runtimeDiffEditor.dispose()
    runtimeDiffEditor = null
  }
}

function resolveRuntimeDiff(source) {
  if (source === "cancel") {
    clearRuntime()
    closeRuntimeDiffDialog()
    return
  }

  emit("resolve-runtime-drift", {
    cli: activeCli.value,
    source
  })
  closeRuntimeDiffDialog()
}

function removeProvider(provider) {
  const shouldContinue = window.confirm(
    "删除 Provider 会同时删除关联模型和 Runtime Profile，是否继续？"
  )

  if (shouldContinue) {
    emit("delete-provider", provider.id)
  }
}

watch(
  () => [visibleCliTargets.value, props.providers],
  () => {
    ensureActiveCli()
  },
  { deep: true, immediate: true }
)

watch(
  () => props.usage,
  (usage) => {
    if (!usageStatsLoading.value) {
      usageStats.value = usage || {}
      usageStatsTarget.value = ""
      ensureUsageStatsReady()
    }
  }
)

watch(
  () => modelDrafts.mainModel,
  (model) => {
    if (activeCli.value !== "codex") {
      return
    }

    draft.modelReasoningEffort = normalizeModelReasoningEffort(
      model,
      draft.modelReasoningEffort
    )
  }
)

watch(codexAccountDetailTab, (tab) => {
  if (tab === "usage") {
    ensureUsageStatsReady()
  }
})

watch(providerDetailTab, (tab) => {
  if (tab === "usage") {
    ensureUsageStatsReady()
  }
})

watch(
  () => props.codexAccounts,
  (accounts) => {
    accounts.forEach((account) => {
      codexAccountProxyDrafts[account.id] = account.proxy || ""
    })
  },
  { deep: true, immediate: true }
)

watch(
  () => props.codexLoginState,
  () => {
    manualCallbackUrl.value = ""
  }
)

watch(
  () => activeProxyState.value?.enabled,
  (enabled) => {
    proxySwitchEnabled.value = Boolean(enabled)
    proxyTab.value = enabled ? "proxy" : "providers"
  },
  { immediate: true }
)

watch(
  () => props.pending,
  (pending) => {
    if (!pending) {
      proxySwitchEnabled.value = activeProxyEnabled.value
    }
  }
)

watch(
  () => props.codexAccounts,
  (accounts) => {
    if (codexAuthUpdateAccountId.value) {
      const account = accounts.find(
        (item) => item.id === codexAuthUpdateAccountId.value
      )

      if (account && account.refresh_status !== "failed") {
        closeCodexLoginModal()
        return
      }
    }

    if (
      codexAccountDetail.value &&
      !accounts.find((item) => item.id === codexAccountDetail.value.id)
    ) {
      closeCodexAccountDetail()
    }
  },
  { deep: true }
)

watch(
  () => props.providers,
  (providers) => {
    if (
      providerDetail.value &&
      !providers.find((item) => item.id === providerDetail.value.id)
    ) {
      closeProviderDetail()
    }
  },
  { deep: true }
)
</script>

<style scoped lang="less">
@keyframes providers-quota-loading {
  from {
    opacity: 0.36;
  }

  to {
    opacity: 1;
  }
}

@keyframes providers-quota-sweep {
  from {
    transform: translateX(-110%);
  }

  to {
    transform: translateX(110%);
  }
}

@keyframes providers-item-sweep {
  from {
    background-position: 160% 0;
  }

  to {
    background-position: -160% 0;
  }
}

@keyframes providers-refresh-spin {
  to {
    transform: rotate(360deg);
  }
}

.providers-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  background: var(--color-panel);

  :deep(.token-count) {
    font-size: inherit;
  }

  :deep(.token-count-exact) {
    font-size: 0.74em;
  }

  &__toolbar {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
    min-height: 58px;
    padding: 0 14px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  &__list-shell {
    display: flex;
    height: 100%;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
  }

  &__cli-tabs,
  &__provider-actions,
  &__section-actions,
  &__json-title,
  &__check-row,
  &__runtime,
  &__diff-footer {
    display: flex;
    align-items: center;
  }

  &__toolbar {
    justify-content: space-between;
  }

  &__toolbar &__runtime {
    flex: 1;
    min-width: 0;
    justify-content: flex-start;
    margin-left: 12px;
  }

  &__cli-tabs {
    justify-content: center;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--color-panel-soft);
  }

  &__cli-tab {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 38px;
    padding: 0 16px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-weight: 600;
  }

  &__cli-tab--active {
    background: var(--color-panel);
    color: var(--color-text);
    box-shadow: 0 1px 5px rgba(15, 23, 42, 0.08);
  }

  &__toolbar-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  &__cli-icon {
    width: 18px;
    height: 18px;
  }

  &__icon-button,
  &__system-config,
  &__add,
  &__back {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition:
      background 0.18s ease,
      color 0.18s ease,
      transform 0.18s ease;
  }

  &__system-config {
    width: 38px;
    height: 38px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
  }

  &__system-config:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  &-proxy-switch {
    display: inline-flex;
    height: 38px;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &-proxy-switch input {
    display: none;
  }

  &-proxy-switch i {
    position: relative;
    width: 34px;
    height: 18px;
    border-radius: 999px;
    background: var(--color-line-strong);
  }

  &-proxy-switch i::before {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: var(--color-panel);
    transition: left 0.18s ease;
  }

  &-proxy-switch input:checked + i {
    background: var(--color-warning);
  }

  &-proxy-switch input:checked + i::before {
    left: 19px;
  }

  &-proxy-switch-disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  &-proxy-add,
  &-proxy-manage {
    display: inline-flex;
    height: 38px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 12px;
    border: 1px solid var(--color-warning-line);
    border-radius: 12px;
    background: var(--color-warning-soft);
    color: var(--color-warning);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &-proxy-manage {
    border-color: var(--color-line);
    background: var(--color-panel);
    color: var(--color-text-muted);
  }

  &-proxy-manage span {
    display: grid;
    min-width: 18px;
    height: 18px;
    place-items: center;
    border-radius: 999px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 12px;
  }

  &-proxy-manage:hover {
    border-color: var(--color-warning-line);
    color: var(--color-warning);
  }

  &-proxy-add:disabled,
  &-proxy-manage:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  &-proxy-tabs {
    display: inline-flex;
    flex: none;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);
  }

  &-proxy-tab {
    height: 32px;
    padding: 0 14px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &-proxy-tab-active {
    background: var(--color-panel);
    color: var(--color-text);
    box-shadow: 0 1px 5px rgba(15, 23, 42, 0.08);
  }

  &__icon-button {
    width: 30px;
    height: 30px;
    border-radius: 8px;
  }

  &__icon-button:hover {
    background: var(--color-panel-soft);
    color: var(--color-primary);
    transform: translateY(-1px);
  }

  &__icon-button:disabled,
  &__icon-button:disabled:hover {
    background: transparent;
    color: var(--color-text-soft);
    cursor: not-allowed;
    opacity: 0.5;
    transform: none;
  }

  &__icon-button--loading,
  &__icon-button--loading:disabled,
  &__icon-button--loading:disabled:hover {
    background: var(--color-primary-soft);
    color: var(--color-primary);
    opacity: 1;
  }

  &__icon-button--loading :deep(svg) {
    animation: providers-refresh-spin 0.8s linear infinite;
  }

  &__icon-button--danger {
    color: var(--color-text-soft);
  }

  &__icon-button--danger:hover {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &__icon-button--warning {
    color: var(--color-text-soft);
  }

  &__icon-button--warning:hover {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  &__add {
    width: 38px;
    height: 38px;
    border-radius: 12px;
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  .providers-disabled-filter {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 38px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 700;
    transition:
      border-color 0.18s ease,
      background 0.18s ease,
      color 0.18s ease,
      box-shadow 0.18s ease;

    &:hover {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
      color: var(--color-text);
      box-shadow: 0 2px 7px rgba(15, 23, 42, 0.06);
    }

    .providers-disabled-filter-label {
      white-space: nowrap;
    }

    .providers-disabled-filter-count {
      display: inline-flex;
      min-width: 18px;
      height: 18px;
      align-items: center;
      justify-content: center;
      padding: 0 4px;
      border-radius: 999px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      font-size: 0.7rem;
      font-variant-numeric: tabular-nums;
    }
  }

  .providers-disabled-filter-active,
  .providers-disabled-filter-active:hover {
    border-color: var(--color-warning-line);
    background: var(--color-warning-soft);
    color: var(--color-warning);
    box-shadow: none;

    .providers-disabled-filter-count {
      background: var(--color-warning-soft);
      color: var(--color-warning);
    }
  }

  &__list-panel {
    display: flex;
    height: auto;
    overflow-x: hidden;
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
    flex-direction: column;
    gap: 10px;
    padding: 14px 14px;
    border: 0;
    border-radius: 0;
  }

  :deep(.codex-proxy-panel) + &__list-panel {
    height: auto;
  }

  &__runtime {
    flex: none;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--color-warning-line);
    border-radius: 8px;
    background: var(--color-warning-soft);
  }

  &__runtime strong {
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.86rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__provider-card,
  &__account-card {
    display: flex;
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
      box-shadow 0.18s ease,
      transform 0.18s ease;
  }

  &__account-card {
    position: relative;
  }

  &__provider-card--active,
  &__account-card--active {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
    box-shadow:
      0 8px 22px rgba(22, 130, 255, 0.12),
      inset 4px 0 0 var(--color-primary-solid);
  }

  &__account-card--error {
    border-color: var(--color-line);
    background: var(--color-danger-soft);
    box-shadow: 0 8px 22px rgba(180, 35, 24, 0.1);
  }

  &__provider-card--disabled,
  &__account-card--disabled {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    box-shadow: none;
    opacity: 0.72;
  }

  &__provider-card:hover,
  &__account-card:hover {
    border-color: var(--color-info-line);
    background: var(--color-panel-soft);
    box-shadow: 0 10px 24px rgba(15, 23, 42, 0.1);
    transform: translateY(-2px);
  }

  &__provider-card--active:hover,
  &__account-card--active:hover {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
    box-shadow:
      0 12px 26px rgba(22, 130, 255, 0.16),
      inset 4px 0 0 var(--color-primary-solid);
  }

  &__provider-card--runtime-warning,
  &__provider-card--runtime-warning:hover {
    border-color: var(--color-warning-line);
  }

  &__account-card--error:hover {
    border-color: var(--color-line);
    background: var(--color-danger-soft);
    box-shadow: 0 12px 26px rgba(180, 35, 24, 0.14);
  }

  &__provider-card--disabled:hover,
  &__account-card--disabled:hover {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    box-shadow: 0 8px 20px rgba(15, 23, 42, 0.08);
    transform: translateY(-1px);
  }

  &__account-card--refreshing,
  &__account-card--refreshing:hover {
    transform: none;
  }

  &__account-card--refreshing:not(.providers-view__account-card--active):not(
      .providers-view__account-card--error
    ):hover {
    border-color: var(--color-line);
    background: var(--color-panel);
    box-shadow: 0 3px 12px rgba(15, 23, 42, 0.06);
  }

  &__account-card--refreshing::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: linear-gradient(
      100deg,
      transparent 0%,
      rgba(22, 130, 255, 0.08) 34%,
      rgba(22, 130, 255, 0.22) 50%,
      rgba(22, 130, 255, 0.08) 66%,
      transparent 100%
    );
    background-size: 220% 100%;
    animation: providers-item-sweep 1.2s ease-in-out infinite;
    pointer-events: none;
  }

  &__account-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--color-line);
  }

  &__account-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 10px;
  }

  &__account-title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  &__account-title strong {
    overflow: hidden;
    min-width: 0;
    color: var(--color-text);
    font-size: 0.98rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__account-tag {
    flex: none;
    padding: 1px 8px;
    border: 1px solid var(--color-success-line);
    border-radius: 999px;
    background: var(--color-success-soft);
    color: var(--color-success);
    font-size: 0.72rem;
    line-height: 1.4;
  }

  &__account-error {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
  }

  &__account-error-tip {
    position: absolute;
    top: calc(100% + 9px);
    left: 50%;
    z-index: 8;
    display: flex;
    width: max-content;
    max-width: 360px;
    min-width: 220px;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow:
      0 14px 34px rgba(52, 64, 84, 0.18),
      0 0 0 1px rgba(253, 162, 155, 0.28);
    color: var(--color-text);
    font-size: 0.76rem;
    line-height: 1.5;
    opacity: 0;
    pointer-events: none;
    transform: translate(-50%, -4px);
    transition:
      opacity 0.16s ease,
      transform 0.16s ease,
      visibility 0.16s ease;
    visibility: hidden;
    white-space: normal;
  }

  &__account-error-tip::before {
    content: "";
    position: absolute;
    top: -5px;
    left: 50%;
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: var(--color-panel);
    box-shadow: -1px -1px 0 rgba(253, 162, 155, 0.2);
    transform: translateX(-50%) rotate(45deg);
  }

  &__account-error:hover &__account-error-tip {
    opacity: 1;
    transform: translate(-50%, 0);
    visibility: visible;
  }

  &__account-error-title {
    color: var(--color-danger);
    font-size: 0.72rem;
    font-weight: 800;
  }

  &__account-error-message {
    overflow-wrap: anywhere;
  }

  &__account-tag--pro {
    border-color: #8b5cf6;
    background: var(--color-accent-soft);
    color: #6d28d9;
    font-weight: 800;
  }

  &__account-tag--plus {
    border-color: var(--color-primary-solid);
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-weight: 800;
  }

  &__account-tag--error {
    border-color: var(--color-danger-line);
    background: var(--color-danger-soft);
    color: var(--color-danger);
    font-weight: 800;
  }

  &__account-tag--disabled {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-weight: 800;
  }

  &__quota-list {
    display: flex;
    flex-direction: row;
    gap: 12px;
    min-width: 0;
  }

  &__account-quota {
    --quota-color: #12b981;
    --quota-bg: #f6fffb;
    --quota-icon-bg: #ecfdf3;

    display: flex;
    position: relative;
    overflow: hidden;
    width: 212px;
    min-width: 0;
    flex-direction: column;
    padding: 8px 10px 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--quota-bg);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.72);
  }

  &__account-quota--warning {
    --quota-color: #f59e0b;
    --quota-bg: #fffbeb;
    --quota-icon-bg: #fff7d6;
  }

  &__account-quota--danger {
    --quota-color: var(--color-danger);
    --quota-bg: #fff5f5;
    --quota-icon-bg: #ffe4e6;
  }

  &__account-quota--loading {
    --quota-color: var(--color-primary-solid);
    --quota-bg: #eef7ff;
    --quota-icon-bg: #eaf5ff;

    border-color: var(--color-info-line);
    box-shadow:
      0 8px 18px rgba(22, 130, 255, 0.14),
      inset 0 0 0 1px rgba(255, 255, 255, 0.86);
  }

  &__account-quota--loading::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(
      100deg,
      transparent 0%,
      rgba(22, 130, 255, 0.18) 42%,
      transparent 72%
    );
    animation: providers-quota-sweep 1.1s ease-in-out infinite;
    pointer-events: none;
  }

  &__quota-bar {
    position: relative;
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 7px;
    overflow: hidden;
  }

  &__quota-title,
  &__quota-meta {
    position: relative;
    z-index: 1;
    display: flex;
    min-width: 0;
    align-items: center;
  }

  &__quota-title {
    gap: 7px;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 700;
  }

  &__quota-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__quota-icon {
    width: 18px;
    height: 18px;
    flex: none;
    border: 4px solid var(--quota-color);
    border-radius: 999px;
    background: var(--quota-icon-bg);
  }

  &__account-quota--loading &__quota-icon,
  &__account-quota--loading &__quota-fill {
    animation: providers-quota-loading 0.86s ease-in-out infinite alternate;
  }

  &__account-quota--loading &__quota-name,
  &__account-quota--loading &__quota-value {
    color: var(--color-primary);
  }

  &__quota-bar::before {
    content: "";
    display: block;
    width: 100%;
    height: 7px;
    border-radius: 999px;
    background: var(--color-success-soft);
    order: 2;
  }

  &__account-quota--loading &__quota-bar::before {
    background: var(--color-primary-soft);
  }

  &__quota-fill {
    position: absolute;
    top: 25px;
    left: 0;
    display: block;
    height: 7px;
    max-width: 100%;
    border-radius: 999px;
    background: var(--quota-color);
  }

  &__account-quota--loading &__quota-fill {
    box-shadow: 0 0 12px rgba(22, 130, 255, 0.48);
  }

  &__quota-meta {
    order: 3;
    gap: 8px;
    justify-content: flex-end;
    color: var(--color-text-muted);
    font-size: 0.74rem;
  }

  &__quota-value {
    color: var(--quota-color);
    font-size: 1rem;
    line-height: 1;
  }

  &__quota-reset {
    overflow: hidden;
    min-width: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__account-actions,
  &__provider-actions {
    display: flex;
    flex: none;
    width: 272px;
    flex-direction: column;
    align-items: flex-end;
    gap: 12px;
  }

  &__drag {
    flex: none;
    color: var(--color-text-muted);
  }

  &__shield,
  &__avatar,
  &__edit-avatar {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);
    color: #ff6a00;
    font-weight: 700;
    transition:
      border-color 0.18s ease,
      background 0.18s ease,
      transform 0.18s ease;
  }

  &__shield,
  &__avatar {
    width: 32px;
    height: 32px;
    flex: none;
  }

  &__shield {
    color: var(--color-text);
  }

  &__avatar-icon {
    width: 22px;
    height: 22px;
  }

  &__provider-card:hover &__shield,
  &__provider-card:hover &__avatar,
  &__account-card:hover &__shield,
  &__account-card:hover &__avatar {
    border-color: #cfe6ff;
    background: var(--color-primary-soft);
    transform: scale(1.06);
  }

  &__provider-card--active &__shield,
  &__provider-card--active &__avatar,
  &__account-card--active &__shield,
  &__account-card--active &__avatar {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__provider-main {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 8px;
  }

  &__provider-title {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
  }

  &__provider-main strong {
    color: var(--color-text);
    font-size: 1rem;
  }

  &__provider-main span {
    overflow: hidden;
    color: var(--color-primary);
    font-size: 0.9rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__provider-title &__account-tag {
    color: var(--color-text-muted);
    font-size: 0.72rem;
  }

  &__provider-main &__provider-note {
    color: var(--color-text-muted);
    font-size: 0.84rem;
  }

  &__action-main,
  &__icon-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }

  &__action-main {
    width: 100%;
    gap: 8px;
    min-height: 36px;
  }

  &__icon-actions {
    gap: 18px;
    padding-right: 3px;
  }

  &__state-pill {
    display: inline-flex;
    flex: none;
    height: 24px;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    border-radius: 999px;
    background: var(--color-success-soft);
    color: var(--color-success);
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__state-dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--color-success);
  }

  &__state-pill--disabled {
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
  }

  &__enable,
  &__using,
  &__primary,
  &__compare-button,
  &__reauth-button,
  &__section-actions button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
  }

  &__enable,
  &__primary {
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  &__enable,
  &__using {
    align-self: center;
    min-width: 134px;
    border-radius: 6px;
    font-size: 14px;
    line-height: 36px;
  }

  &__using {
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
  }

  &__compare-button {
    flex: none;
    border: 1px solid var(--color-danger);
    background: var(--color-panel);
    color: var(--color-danger);
    font-size: 12px;
    height: 25px;
  }

  &__reauth-button {
    flex: none;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--color-danger-line);
    background: var(--color-panel);
    color: var(--color-danger);
    font-size: 12px;
  }

  &__empty {
    display: flex;
    min-height: 220px;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--color-line);
    border-radius: 14px;
    color: var(--color-text-muted);
  }

  &__edit-header {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 64px;
    padding: 0 24px;
    background: var(--color-panel);
  }

  &__back {
    width: 36px;
    height: 36px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
  }

  &__edit-header h1 {
    margin: 0;
    font-size: 1.18rem;
  }

  &__edit-panel {
    display: flex;
    overflow: auto;
    flex: 1;
    flex-direction: column;
    gap: 24px;
    margin: 22px 24px 78px;
    padding: 24px;
    border: 1px solid var(--color-line);
    border-radius: 14px;
    background: var(--color-panel);
  }

  &__edit-avatar {
    width: 78px;
    height: 78px;
    align-self: center;
    padding: 0;
    cursor: pointer;
    font-size: 1.4rem;
  }

  &__avatar-picker {
    display: flex;
    align-items: center;
    flex-direction: column;
    gap: 10px;
  }

  &__edit-avatar-icon {
    width: 48px;
    height: 48px;
  }

  &__avatar-name {
    color: var(--color-text-muted);
    font-size: 0.85rem;
  }

  &__icon-panel {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);
  }

  &-icon-upload {
    display: inline-flex;
    width: fit-content;
    align-items: center;
    gap: 7px;
    padding: 8px 12px;
    border: 1px solid var(--color-info-line);
    border-radius: 8px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.84rem;
    font-weight: 700;
  }

  &-icon-upload input {
    display: none;
  }

  &__icon-grid {
    display: flex;
    overflow: auto;
    max-height: 360px;
    flex-wrap: wrap;
    gap: 10px;
    padding-right: 4px;
  }

  &__icon-option {
    display: flex;
    flex: 0 0 132px;
    min-width: 0;
    height: 86px;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 8px;
    padding: 8px 6px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &__icon-option--active {
    border-color: var(--color-primary-solid);
    background: var(--color-primary-soft);
    color: var(--color-text);
  }

  &__icon-option-image {
    width: 30px;
    height: 30px;
    flex: none;
  }

  &__icon-option span {
    overflow: hidden;
    width: 100%;
    font-size: 0.78rem;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__form-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 22px 16px;
  }

  &__field {
    display: flex;
    min-width: 0;
    flex: 1 1 calc(50% - 8px);
    flex-direction: column;
    gap: 9px;
  }

  &__field--wide {
    flex-basis: 100%;
  }

  &__field span,
  &__section-title p {
    color: var(--color-text-muted);
  }

  &__field input,
  &__field select {
    min-width: 0;
    height: 38px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
  }

  &__api-keys {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  &__api-keys-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__api-keys-header button,
  &__api-key-meta button {
    display: inline-flex;
    height: 30px;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 0 9px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__api-keys-header button:hover,
  &__api-key-meta button:hover {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__api-key-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  &__api-key-item {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 9px;
    background: var(--color-panel-soft);
  }

  &__api-key-meta {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__api-key-name {
    flex: 1;
  }

  &__api-key-note {
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
  }

  &__api-key-manager {
    display: flex;
    width: 100%;
    min-width: 0;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 14px;
  }

  &__api-key-modal {
    :deep(.base-modal__panel) {
      width: min(680px, calc(100vw - 48px));
    }

    :deep(.base-modal__content) {
      overflow: hidden;
    }

    .providers-view__api-key-list {
      min-height: 0;
      flex: 1;
      overflow-y: auto;
      padding-right: 4px;
    }
  }

  &__api-key-manager-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.82rem;
  }

  &__api-key-manager-header strong {
    color: var(--color-text);
    font-size: 1rem;
  }

  &__api-key-add {
    display: inline-flex;
    width: fit-content;
    height: 34px;
    align-items: center;
    gap: 6px;
    padding: 0 11px;
    border: 1px dashed var(--color-info-line);
    border-radius: 8px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &__api-key-manager-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 4px;
  }

  &__api-key-manager-footer > button {
    min-height: 36px;
    padding: 0 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-weight: 700;
  }

  &__api-key-meta button.providers-view__api-key-active {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__api-keys small {
    color: var(--color-text-muted);
    font-size: 0.78rem;
  }

  &__warning {
    padding: 12px 14px;
    border: 1px solid var(--color-warning-line);
    border-radius: 12px;
    background: var(--color-warning-soft);
    color: var(--color-warning);
    font-size: 0.86rem;
  }

  &__advanced {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  &__advanced summary {
    cursor: pointer;
    font-weight: 700;
  }

  &__advanced &__field {
    margin-top: 14px;
  }

  &__section-title {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding-top: 8px;
    border-top: 1px solid var(--color-line);
  }

  &__section-title h2 {
    margin: 0 0 8px;
    font-size: 1rem;
  }

  &__section-title p {
    margin: 0 0 16px;
    font-size: 0.86rem;
  }

  &__section-actions {
    gap: 8px;
  }

  &__section-actions button {
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    color: var(--color-text-muted);
  }

  &__json {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  &__json-title {
    justify-content: space-between;
  }

  &__json-title label,
  &__option-field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--color-text-muted);
  }

  &__option-field input[type="number"] {
    width: 112px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    color: var(--color-text);
  }

  &__option-field select {
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
  }

  &__check-row {
    flex-wrap: wrap;
    gap: 16px;
  }

  &__config-preview {
    border: 1px solid var(--color-line);
    border-radius: 8px;
    overflow: hidden;
    background: var(--color-panel);
  }

  &__config-preview summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    cursor: pointer;
    color: var(--color-text);
    font-weight: 700;
    list-style-position: inside;
  }

  &__config-preview summary span {
    font-size: 0.95rem;
  }

  &__json pre {
    overflow: auto;
    max-height: 260px;
    margin: 0;
    padding: 16px 18px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    color: var(--color-text);
    font-size: 0.85rem;
    line-height: 1.55;
  }

  &__config-preview p {
    margin: 0;
    padding: 10px 14px;
    border-top: 1px solid var(--color-line);
    color: var(--color-text-muted);
    font-size: 0.82rem;
  }

  &__edit-footer {
    position: fixed;
    right: 0;
    bottom: 0;
    left: 0;
    display: flex;
    justify-content: flex-end;
    padding: 16px 24px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  &__primary:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }

  &__create-options {
    display: flex;
    gap: 16px;
  }

  &__create-option {
    display: flex;
    flex: 1;
    min-height: 180px;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    padding: 22px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }
  .option-logo {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  &__create-option:hover {
    border-color: var(--color-primary-solid);
    background: var(--color-primary-soft);
  }

  &__create-option svg {
    color: var(--color-primary);
  }

  &__create-option strong {
    font-size: 1.05rem;
  }

  &__create-option span {
    color: var(--color-text-muted);
    font-size: 0.9rem;
    line-height: 1.6;
  }

  &__provider-create-modal {
    :deep(.base-modal__panel) {
      width: 920px;
      border: 1px solid var(--color-line);
      border-radius: 16px;
      box-shadow: 0 24px 70px rgba(15, 23, 42, 0.26);
    }

    :deep(.base-modal__header) {
      align-items: center;
      padding: 22px 26px 20px;
      border-bottom: 1px solid var(--color-line);
    }

    :deep(.base-modal__header h2) {
      color: var(--color-text);
      font-size: 1.18rem;
    }

    :deep(.base-modal__close) {
      width: 28px;
      height: 28px;
      border: 0;
      background: transparent;
      color: var(--color-text-muted);
      font-size: 1.3rem;
    }

    :deep(.base-modal__content) {
      display: flex;
      min-height: 0;
      flex: 1;
      flex-direction: column;
      padding: 18px 26px 24px;
    }
  }

  &__create-form {
    display: flex;
    overflow: auto;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 24px;
    padding: 6px 2px 20px;
  }

  &__create-footer {
    display: flex;
    flex: none;
    justify-content: flex-end;
    padding: 16px 0 0;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
    position: sticky;
    bottom: 0;
  }

  &__codex-login-modal {
    :deep(.base-modal__panel) {
      width: 560px;
      border: 1px solid var(--color-line);
      border-radius: 16px;
      box-shadow: 0 24px 70px rgba(15, 23, 42, 0.26);
    }

    :deep(.base-modal__header) {
      align-items: center;
      padding: 22px 26px 20px;
      border-bottom: 1px solid var(--color-line);
    }

    :deep(.base-modal__header h2) {
      color: var(--color-text);
      font-size: 1.18rem;
    }

    :deep(.base-modal__close) {
      width: 28px;
      height: 28px;
      border: 0;
      background: transparent;
      color: var(--color-text-muted);
      font-size: 1.3rem;
    }

    :deep(.base-modal__content) {
      padding: 18px 26px 24px;
    }
  }

  &__codex-proxy-modal {
    :deep(.base-modal__panel) {
      width: 440px;
      border: 1px solid var(--color-line);
      border-radius: 16px;
      box-shadow: 0 24px 70px rgba(15, 23, 42, 0.26);
    }

    :deep(.base-modal__header) {
      align-items: center;
      padding: 22px 26px 20px;
      border-bottom: 1px solid var(--color-line);
    }

    :deep(.base-modal__header h2) {
      color: var(--color-text);
      font-size: 1.18rem;
    }

    :deep(.base-modal__close) {
      width: 28px;
      height: 28px;
      border: 0;
      background: transparent;
      color: var(--color-text-muted);
      font-size: 1.3rem;
    }

    :deep(.base-modal__content) {
      padding: 18px 26px 24px;
    }
  }

  &__diff-modal {
    :deep(.base-modal__panel) {
      width: 1120px;
    }

    :deep(.base-modal__header) {
      padding: 18px 22px 10px;
    }

    :deep(.base-modal__header h2) {
      color: var(--color-text);
      font-size: 1.05rem;
      line-height: 1.35;
    }

    :deep(.base-modal__header p) {
      font-size: 0.86rem;
      line-height: 1.5;
      white-space: pre-line;
    }
  }

  &__runtime-config-modal {
    :deep(.base-modal__panel) {
      width: 880px;
    }

    :deep(.base-modal__header) {
      padding: 18px 22px 10px;
    }

    :deep(.base-modal__header h2) {
      color: var(--color-text);
      font-size: 1.05rem;
      line-height: 1.35;
    }

    :deep(.base-modal__header p) {
      font-size: 0.86rem;
      line-height: 1.5;
      white-space: pre-line;
    }
  }

  &__runtime-config-panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
  }

  &__runtime-config-json {
    max-height: 560px;
  }

  &__diff-editor {
    height: 560px;
    border: 1px solid var(--color-line);
  }

  &__diff-footer {
    flex: none;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 12px;
    background: var(--color-panel);
    position: sticky;
    bottom: 0;
  }

  &__diff-button {
    min-width: 168px;
    height: 34px;
    padding: 0 14px;
    border: 1px solid var(--color-line-strong);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.86rem;
    font-weight: 600;
  }

  &__diff-button--primary {
    border-color: var(--color-primary-solid);
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  &__diff-button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  &__drawer {
    position: fixed;
    inset: 0;
    z-index: 44;
    display: flex;
    justify-content: flex-end;
  }

  &__drawer-backdrop {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.24);
  }

  &__drawer-panel {
    position: relative;
    display: flex;
    width: 520px;
    flex-direction: column;
    border-left: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow: -24px 0 70px rgba(15, 23, 42, 0.18);
  }

  &__drawer-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 22px 24px 18px;
    border-bottom: 1px solid var(--color-line);

    h2 {
      margin: 0;
      color: var(--color-text);
      font-size: 1.18rem;
    }

    p {
      margin: 6px 0 0;
      color: var(--color-text-muted);
      font-size: 0.86rem;
    }
  }

  .providers-view-drawer-tabs {
    display: flex;
    gap: 6px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .providers-view-drawer-tab {
    height: 30px;
    padding: 0 14px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  .providers-view-drawer-tab-active {
    border-color: var(--color-primary-solid);
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  &__drawer-close {
    width: 28px;
    height: 28px;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 1.4rem;
    line-height: 1;
  }

  &__drawer-content {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 14px;
    overflow: auto;
    padding: 18px 24px 24px;
  }

  &__drawer-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  &__drawer-section h3 {
    margin: 0;
    color: var(--color-text);
    font-size: 0.92rem;
  }

  &__drawer-json {
    overflow: auto;
    margin: 0;
    padding: 14px 16px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel-soft);
    color: var(--color-text);
    font-size: 0.8rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__drawer-empty {
    color: var(--color-text-muted);
    font-size: 0.88rem;
  }

  .providers-view-usage-panel {
    position: relative;
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 14px;

    .providers-view-usage-mask {
      position: absolute;
      inset: 0;
      z-index: 2;
      display: flex;
      align-items: center;
      justify-content: center;
      border: 1px solid rgba(207, 219, 234, 0.72);
      border-radius: 8px;
      background: rgba(248, 251, 255, 0.78);
      backdrop-filter: blur(4px);

      .providers-view-usage-mask-text {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-width: 88px;
        height: 32px;
        padding: 0 14px;
        border: 1px solid var(--color-line);
        border-radius: 999px;
        background: var(--color-panel);
        color: var(--color-primary);
        font-size: 0.84rem;
        font-weight: 700;
        box-shadow: 0 10px 24px rgba(31, 52, 78, 0.1);
      }
    }

    .providers-view-quota-stage-list {
      display: flex;
      flex-direction: column;
      gap: 10px;

      .providers-view-quota-stage-card {
        display: flex;
        flex-direction: column;
        gap: 10px;
        padding: 14px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);

        .providers-view-quota-stage-head {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 12px;

          .providers-view-quota-stage-title {
            display: flex;
            min-width: 0;
            align-items: center;
            gap: 8px;

            .providers-view-quota-stage-name {
              overflow: hidden;
              color: var(--color-text);
              font-size: 0.9rem;
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .providers-view-quota-stage-status {
              flex: none;
              color: var(--color-primary);
              font-size: 0.72rem;
              font-weight: 700;
            }
          }

          .providers-view-quota-stage-percent {
            flex: none;
            color: var(--color-success);
            font-size: 0.86rem;
          }
        }

        .providers-view-quota-stage-track {
          height: 7px;
          overflow: hidden;
          border-radius: 4px;
          background: var(--color-panel-soft);

          .providers-view-quota-stage-fill {
            display: block;
            height: 100%;
            border-radius: inherit;
            background: var(--color-primary-solid);
          }
        }

        .providers-view-quota-stage-metrics {
          display: flex;
          align-items: center;
          flex-wrap: wrap;
          gap: 12px;

          .providers-view-quota-stage-metric {
            display: flex;
            min-width: 0;
            flex: 1 1 100px;
            align-items: baseline;
            gap: 4px;
            color: var(--color-text-muted);
            font-size: 0.74rem;

            .providers-view-quota-stage-metric-value {
              overflow: hidden;
              color: var(--color-text);
              font-size: 0.82rem;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
          }
        }

        .providers-view-quota-stage-range {
          color: var(--color-text-muted);
          font-size: 0.72rem;
        }
      }
    }

    .providers-view-quota-history {
      display: flex;
      flex-direction: column;
      gap: 7px;
      padding-top: 4px;

      .providers-view-quota-history-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        color: var(--color-text-muted);
        font-size: 0.76rem;

        .providers-view-quota-history-title {
          color: var(--color-text-muted);
          font-size: 0.78rem;
        }

        .providers-view-quota-history-count {
          flex: none;
        }
      }

      .providers-view-quota-history-row {
        display: flex;
        min-width: 0;
        align-items: center;
        justify-content: space-between;
        gap: 14px;
        padding: 10px 12px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);

        .providers-view-quota-history-main,
        .providers-view-quota-history-metrics {
          display: flex;
          min-width: 0;
          flex-direction: column;
          gap: 3px;
        }

        .providers-view-quota-history-main {
          .providers-view-quota-history-name {
            color: var(--color-text);
            font-size: 0.8rem;
          }

          .providers-view-quota-history-range {
            overflow: hidden;
            color: var(--color-text-muted);
            font-size: 0.7rem;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
        }

        .providers-view-quota-history-metrics {
          flex: none;
          align-items: flex-end;

          .providers-view-quota-history-token {
            color: var(--color-text);
            font-size: 0.82rem;
          }

          .providers-view-quota-history-usage,
          .providers-view-quota-history-cost {
            color: var(--color-text-muted);
            font-size: 0.7rem;
          }
        }
      }
    }
  }

  .providers-view-usage-hero {
    display: flex;
    min-width: 0;
    align-items: stretch;
    justify-content: space-between;
    gap: 12px;
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: linear-gradient(135deg, var(--color-panel-soft) 0%, var(--color-primary-soft) 100%);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.72);
  }

  .providers-view-usage-hero-item,
  .providers-view-usage-hero-side {
    display: flex;
    min-width: 0;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
  }

  .providers-view-usage-hero-item {
    flex: 1;
  }

  .providers-view-usage-hero-side {
    width: 132px;
    align-items: flex-end;
    padding-left: 14px;
    border-left: 1px solid var(--color-line);
  }

  .providers-view-usage-label {
    color: var(--color-text-muted);
    font-size: 0.78rem;
  }

  .providers-view-usage-value {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: 1rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .providers-view-usage-total {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: 1.46rem;
    font-weight: 800;
    line-height: 1.12;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .providers-view-usage-cost {
    color: var(--color-primary);
    font-size: 1.22rem;
    font-weight: 800;
    line-height: 1.1;
  }

  .providers-view-usage-subtext {
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  .providers-view-usage-token-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .providers-view-usage-token-item {
    display: flex;
    min-width: 0;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);

    small {
      color: var(--color-text-muted);
      font-size: 0.72rem;
    }
  }

  .providers-view-usage-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .providers-view-usage-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 14px 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 8px 22px rgba(31, 52, 78, 0.05);
  }

  .providers-view-usage-row-main,
  .providers-view-usage-row-side {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }

  .providers-view-usage-row-side {
    align-items: flex-end;
    flex-shrink: 0;
  }

  &__login-panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  &__login-tabs {
    display: flex;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-primary-soft);
  }

  &__login-tab {
    display: inline-flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 30px;
    padding: 0 6px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.72rem;
    line-height: 0.72rem;
    font-weight: 700;
    white-space: nowrap;
  }

  &__login-tab--active {
    background: var(--color-primary-solid);
    color: #ffffff;
  }

  &__login-intro {
    margin: 10px 0 0;
    color: var(--color-text-muted);
    font-size: 0.9rem;
    line-height: 1.6;
  }

  &__login-field {
    display: flex;
    flex-direction: column;
    gap: 8px;

    span {
      color: var(--color-text-muted);
      font-size: 0.76rem;
      font-weight: 700;
    }
  }

  &__login-copy-row,
  &__login-callback-row {
    display: flex;
    gap: 8px;
  }

  &__login-copy-row input,
  &__login-callback-row input,
  &__login-field input,
  &__login-field textarea,
  &__login-tip {
    min-width: 0;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__login-copy-row input,
  &__login-callback-row input,
  &__login-field input,
  &__login-tip {
    height: 36px;
  }

  &__login-field textarea {
    height: 160px;
    padding: 12px;
    line-height: 1.55;
    resize: none;
  }

  &__login-copy-row input,
  &__login-callback-row input {
    flex: 1;
  }

  &__login-copy-row button,
  &__login-callback-row button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 36px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-weight: 700;
  }

  &__login-copy-row button {
    width: 44px;
    padding: 0;
  }

  &__login-primary {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 38px;
    border: 0;
    border-radius: 10px;
    background: linear-gradient(90deg, #1f66f2, #10a5aa);
    color: #ffffff;
    cursor: pointer;
    font-weight: 800;
  }

  &__login-primary:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }

  &__login-callback-row button {
    flex: none;
    gap: 6px;
    padding: 0 13px;
    color: var(--color-text-muted);

    &:disabled {
      cursor: not-allowed;
      opacity: 0.52;
    }
  }

  &__login-tip {
    display: flex;
    align-items: center;
    color: var(--color-text-muted);
  }

  &__login-actions {
    display: flex;
    justify-content: flex-end;

    button {
      height: 32px;
      padding: 0 12px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
      font-weight: 700;
    }

    button:disabled {
      cursor: not-allowed;
      opacity: 0.52;
    }
  }

  &__login-status {
    padding: 9px 11px;
    border-radius: 8px;
    font-size: 0.82rem;

    &--success {
      border: 1px solid var(--color-success-line);
      background: var(--color-success-soft);
      color: var(--color-success);
    }

    &--failed {
      border: 1px solid var(--color-danger-line);
      background: var(--color-danger-soft);
      color: var(--color-danger);
    }

    &--cancelled {
      border: 1px solid var(--color-warning-line);
      background: var(--color-warning-soft);
      color: var(--color-warning);
    }
  }
}
</style>

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
                  'providers-view-proxy-tab-active':
                    proxyTab === 'providers'
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
              :checked="activeProxyState.enabled"
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
        v-if="
          activeProxyState &&
          (!activeProxyEnabled || proxyTab === 'proxy')
        "
        ref="proxyPanelRef"
        :accounts="codexAccounts"
        :cli-name="activeCliName"
        :include-accounts="activeCli === 'codex'"
        :pending="pending"
        :providers="scopedProviders"
        :proxy-state="activeProxyState"
        @account-model-save="
          emit('codex-proxy-account-model-save', $event)
        "
        @add-provider="
          payload => {
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
          @account-model-save="
            emit('codex-proxy-account-model-save', $event)
          "
          @add-provider="
            payload => {
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
                  v-if="item.provider.enabled !== false"
                  class="providers-view__icon-button"
                  type="button"
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
          当前 CLI 还没有 Provider。
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
          <label class="providers-view__field providers-view__field--wide">
            <span>API Key</span>
            <el-input
              v-model="draft.apiKey"
              type="password"
              show-password
              :placeholder="
                selectedProvider?.hasApiKey ? '已保存，留空则保持不变' : ''
              "
            />
          </label>
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
                    v-for="option in field.options"
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
          <label class="providers-view__field providers-view__field--wide">
            <span>API Key</span>
            <el-input
              v-model="draft.apiKey"
              type="password"
              show-password
              :placeholder="
                selectedProvider?.hasApiKey ? '已保存，留空则保持不变' : ''
              "
            />
          </label>
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
                    v-for="option in field.options"
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
            <section class="providers-view__drawer-section">
              <h3>用量概览</h3>
              <div class="providers-view-usage-hero">
                <div class="providers-view-usage-hero-main">
                  <span class="providers-view-usage-label">Token 总量</span>
                  <strong class="providers-view-usage-total">
                    {{
                      formatProviderNumber(codexAccountUsageSummary.actualTokens)
                    }}
                  </strong>
                  <span class="providers-view-usage-subtext">
                    {{
                      formatProviderNumber(codexAccountUsageSummary.requestCount)
                    }}
                    次请求
                  </span>
                </div>
                <div class="providers-view-usage-hero-side">
                  <span class="providers-view-usage-label">费用</span>
                  <strong class="providers-view-usage-cost">
                    {{ formatProviderCost(codexAccountUsageSummary.totalCostUsd) }}
                  </strong>
                  <span class="providers-view-usage-subtext">估算金额</span>
                </div>
              </div>
            </section>
            <section class="providers-view__drawer-section">
              <h3>Token 明细</h3>
              <div class="providers-view-usage-token-grid">
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">输入</span>
                  <strong class="providers-view-usage-value">
                    {{
                      formatProviderNumber(codexAccountUsageSummary.inputTokens)
                    }}
                  </strong>
                </article>
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">输出</span>
                  <strong class="providers-view-usage-value">
                    {{
                      formatProviderNumber(codexAccountUsageSummary.outputTokens)
                    }}
                  </strong>
                </article>
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">缓存读取</span>
                  <strong class="providers-view-usage-value">
                    {{
                      formatProviderNumber(
                        codexAccountUsageSummary.cacheReadTokens
                      )
                    }}
                  </strong>
                </article>
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">缓存写入</span>
                  <strong class="providers-view-usage-value">
                    {{
                      formatProviderNumber(
                        codexAccountUsageSummary.cacheCreationTokens
                      )
                    }}
                  </strong>
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
                      {{ formatProviderNumber(item.actualTokens) }}
                    </strong>
                    <span class="providers-view-usage-label">
                      {{ formatProviderCost(item.totalCostUsd) }}
                    </span>
                  </div>
                </article>
              </div>
              <div v-else class="providers-view__drawer-empty">
                暂无用量记录。
              </div>
            </section>
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
            <section class="providers-view__drawer-section">
              <h3>用量概览</h3>
              <div class="providers-view-usage-hero">
                <div class="providers-view-usage-hero-main">
                  <span class="providers-view-usage-label">Token 总量</span>
                  <strong class="providers-view-usage-total">
                    {{ formatProviderNumber(providerUsageSummary.actualTokens) }}
                  </strong>
                  <span class="providers-view-usage-subtext">
                    {{ formatProviderNumber(providerUsageSummary.requestCount) }}
                    次请求
                  </span>
                </div>
                <div class="providers-view-usage-hero-side">
                  <span class="providers-view-usage-label">费用</span>
                  <strong class="providers-view-usage-cost">
                    {{ formatProviderCost(providerUsageSummary.totalCostUsd) }}
                  </strong>
                  <span class="providers-view-usage-subtext">估算金额</span>
                </div>
              </div>
            </section>
            <section class="providers-view__drawer-section">
              <h3>Token 明细</h3>
              <div class="providers-view-usage-token-grid">
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">输入</span>
                  <strong class="providers-view-usage-value">
                    {{ formatProviderNumber(providerUsageSummary.inputTokens) }}
                  </strong>
                </article>
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">输出</span>
                  <strong class="providers-view-usage-value">
                    {{ formatProviderNumber(providerUsageSummary.outputTokens) }}
                  </strong>
                </article>
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">缓存读取</span>
                  <strong class="providers-view-usage-value">
                    {{
                      formatProviderNumber(providerUsageSummary.cacheReadTokens)
                    }}
                  </strong>
                </article>
                <article class="providers-view-usage-token-item">
                  <span class="providers-view-usage-label">缓存写入</span>
                  <strong class="providers-view-usage-value">
                    {{
                      formatProviderNumber(
                        providerUsageSummary.cacheCreationTokens
                      )
                    }}
                  </strong>
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
                      {{ formatProviderNumber(item.actualTokens) }}
                    </strong>
                    <span class="providers-view-usage-label">
                      {{ formatProviderCost(item.totalCostUsd) }}
                    </span>
                  </div>
                </article>
              </div>
              <div v-else class="providers-view__drawer-empty">
                暂无用量记录。
              </div>
            </section>
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
import CodexProxyPanel from "@/features/providers/components/CodexProxyPanel.vue"
import { accountApi, runtimeApi, systemApi } from "@/api"
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
const proxyPanelRef = ref(null)
const showProxyAddAction = ref(false)
const showProxyManager = ref(false)
const proxyTab = ref("proxy")
const providerDetail = ref(null)
const providerDetailTab = ref("config")
const manualCallbackUrl = ref("")
const countdownNow = ref(Date.now())
let countdownTimer = null
const iconKeyword = ref("")
const iconModules = import.meta.glob("/src/assets/ai-icons/*.svg", {
  query: "?url",
  import: "default"
})
const iconOptions = Object.keys(iconModules)
  .map(item => item.split("/").pop())
  .sort((left, right) => left.localeCompare(right))
let runtimeDiffEditor = null

const visibleCliTargets = computed(() => {
  return props.cliTargets.filter(item => {
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

const activeCliName = computed(() => {
  return (
    visibleCliTargets.value.find(item => item.id === activeCli.value)?.name ||
    activeCli.value ||
    "Runtime"
  )
})

const selectedProvider = computed(() => {
  return props.providers.find(item => item.id === draft.id) || null
})

const scopedProviders = computed(() => {
  return props.providers.filter(item => item.cli === activeCli.value)
})

const mixedItems = computed(() => {
  const providerItems = scopedProviders.value.map(provider => ({
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
      ? props.codexAccounts.map(account => ({
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
  return Object.fromEntries(props.runtimeProfiles.map(item => [item.cli, item]))
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

  return iconOptions.filter(item =>
    iconLabel(item).toLowerCase().includes(keyword)
  )
})

const codexLoginTitle = computed(() => {
  return codexAuthUpdateAccountId.value ? "更新认证信息" : "添加 Codex 账号"
})

const configPreviewMap = computed(() => {
  return Object.fromEntries(
    activeRuntimeSchema.value.configFiles.map(file => [
      file.name,
      formatConfigPreview(file, applyConfigTemplate(file.template))
    ])
  )
})

const codexAccountUsageLogs = computed(() => {
  if (!codexAccountDetailTarget.value) {
    return []
  }

  const accountIds = [
    `codex-account:${codexAccountDetailTarget.value.id}`,
    `account:${codexAccountDetailTarget.value.id}`
  ]

  return (props.usage.logs || []).filter(item => {
    return accountIds.includes(item.providerId)
  })
})

const codexAccountUsageSummary = computed(() => {
  return codexAccountUsageLogs.value.reduce(
    (result, item) => {
      result.requestCount += 1
      result.inputTokens += providerUsageInputTokens(item)
      result.outputTokens += Number(item.outputTokens || 0)
      result.cacheReadTokens += Number(item.cacheReadTokens || 0)
      result.cacheCreationTokens += Number(item.cacheCreationTokens || 0)
      result.actualTokens += Number(item.actualTokens || 0)
      result.totalCostUsd += Number(item.totalCostUsd || 0)
      return result
    },
    {
      requestCount: 0,
      inputTokens: 0,
      outputTokens: 0,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      actualTokens: 0,
      totalCostUsd: 0
    }
  )
})

const codexAccountUsageModelStats = computed(() => {
  const groups = new Map()

  for (const item of codexAccountUsageLogs.value) {
    const model = item.model || "未识别模型"
    const current = groups.get(model) || {
      model,
      requestCount: 0,
      actualTokens: 0,
      totalCostUsd: 0
    }

    current.requestCount += 1
    current.actualTokens += Number(item.actualTokens || 0)
    current.totalCostUsd += Number(item.totalCostUsd || 0)
    groups.set(model, current)
  }

  return Array.from(groups.values()).sort(
    (left, right) => right.actualTokens - left.actualTokens
  )
})

const providerUsageLogs = computed(() => {
  if (!providerDetail.value) {
    return []
  }

  return (props.usage.logs || []).filter(item => {
    return item.providerId === providerDetail.value.id
  })
})

const providerUsageSummary = computed(() => {
  return providerUsageLogs.value.reduce(
    (result, item) => {
      result.requestCount += 1
      result.inputTokens += providerUsageInputTokens(item)
      result.outputTokens += Number(item.outputTokens || 0)
      result.cacheReadTokens += Number(item.cacheReadTokens || 0)
      result.cacheCreationTokens += Number(item.cacheCreationTokens || 0)
      result.actualTokens += Number(item.actualTokens || 0)
      result.totalCostUsd += Number(item.totalCostUsd || 0)
      return result
    },
    {
      requestCount: 0,
      inputTokens: 0,
      outputTokens: 0,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      actualTokens: 0,
      totalCostUsd: 0
    }
  )
})

const providerUsageModelStats = computed(() => {
  const groups = new Map()

  for (const item of providerUsageLogs.value) {
    const model = item.model || "未识别模型"
    const current = groups.get(model) || {
      model,
      requestCount: 0,
      actualTokens: 0,
      totalCostUsd: 0
    }

    current.requestCount += 1
    current.actualTokens += Number(item.actualTokens || 0)
    current.totalCostUsd += Number(item.totalCostUsd || 0)
    groups.set(model, current)
  }

  return Array.from(groups.values()).sort(
    (left, right) => right.actualTokens - left.actualTokens
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
  const values = {
    authField: draft.authField,
    apiKey: draft.apiKey,
    hasApiKey: Boolean(draft.apiKey),
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
  if (visibleCliTargets.value.find(item => item.id === activeCli.value)) {
    return
  }

  activeCli.value = visibleCliTargets.value[0]?.id || ""
}

function selectCli(cli) {
  const previousCli = activeCli.value
  activeCli.value = cli
  closeCodexAccountDetail()
  closeProviderDetail()
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

function rateLimitWindows(rateLimit) {
  return [
    { key: "primary", window: rateLimit.primary_window },
    { key: "secondary", window: rateLimit.secondary_window }
  ].filter(item => item.window)
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

function formatJson(value) {
  return JSON.stringify(value || {}, null, 2)
}

function providerUsageInputTokens(item) {
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
  props.codexAccounts.forEach(account => {
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
    props.runtimeModels.find(item => item.providerId === providerId)?.name || ""
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
      modelReasoningEffort: draft.modelReasoningEffort,
      modelAutoCompactTokenLimit: draft.modelAutoCompactTokenLimit
    },
    enabled: draft.enabled
  }

  if (draft.apiKey) {
    payload.apiKey = draft.apiKey
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
  if (event.target.checked) {
    if (!activeProxyProviderIds.value.length) {
      event.target.checked = false
      showProxyAddAction.value = true
      createMessage.error("请先添加代理接管池")
      return
    }

    emitProxyEvent("enable", {})
    event.target.checked = activeProxyState.value.enabled
    return
  }

  emitProxyEvent("disable")
  event.target.checked = activeProxyState.value.enabled
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
  () => props.codexAccounts,
  accounts => {
    accounts.forEach(account => {
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
  enabled => {
    proxyTab.value = enabled ? "proxy" : "providers"
  },
  { immediate: true }
)

watch(
  () => props.codexAccounts,
  accounts => {
    if (codexAuthUpdateAccountId.value) {
      const account = accounts.find(
        item => item.id === codexAuthUpdateAccountId.value
      )

      if (account && account.refresh_status !== "failed") {
        closeCodexLoginModal()
        return
      }
    }

    if (
      codexAccountDetail.value &&
      !accounts.find(item => item.id === codexAccountDetail.value.id)
    ) {
      closeCodexAccountDetail()
    }
  },
  { deep: true }
)

watch(
  () => props.providers,
  providers => {
    if (
      providerDetail.value &&
      !providers.find(item => item.id === providerDetail.value.id)
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
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  background: #ffffff;

  &__toolbar {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
    min-height: 58px;
    padding: 0 14px;
    border-bottom: 1px solid #edf0f3;
    background: #ffffff;
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
    background: #f5f6f8;
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
    color: #667085;
    cursor: pointer;
    font-weight: 600;
  }

  &__cli-tab--active {
    background: #ffffff;
    color: #111827;
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
    color: #667085;
    cursor: pointer;
    transition:
      background 0.18s ease,
      color 0.18s ease,
      transform 0.18s ease;
  }

  &__system-config {
    width: 38px;
    height: 38px;
    border: 1px solid #d8e0eb;
    border-radius: 12px;
    background: #ffffff;
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
    border: 1px solid #d8e0eb;
    border-radius: 12px;
    background: #ffffff;
    color: #475467;
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
    background: #d0d5dd;
  }

  &-proxy-switch i::before {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: #ffffff;
    transition: left 0.18s ease;
  }

  &-proxy-switch input:checked + i {
    background: #d7a533;
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
    border: 1px solid #f2b94b;
    border-radius: 12px;
    background: #fffaf0;
    color: #9a6700;
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &-proxy-manage {
    border-color: #d8e0eb;
    background: #ffffff;
    color: #475467;
  }

  &-proxy-manage span {
    display: grid;
    min-width: 18px;
    height: 18px;
    place-items: center;
    border-radius: 999px;
    background: #f2f4f7;
    color: #667085;
    font-size: 12px;
  }

  &-proxy-manage:hover {
    border-color: #f2b94b;
    color: #9a6700;
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
    border: 1px solid #edf0f3;
    border-radius: 12px;
    background: #f7f8fa;
  }

  &-proxy-tab {
    height: 32px;
    padding: 0 14px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: #667085;
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  &-proxy-tab-active {
    background: #ffffff;
    color: #111827;
    box-shadow: 0 1px 5px rgba(15, 23, 42, 0.08);
  }

  &__icon-button {
    width: 30px;
    height: 30px;
    border-radius: 8px;
  }

  &__icon-button:hover {
    background: #f3f7fb;
    color: #1682ff;
    transform: translateY(-1px);
  }

  &__icon-button:disabled,
  &__icon-button:disabled:hover {
    background: transparent;
    color: #98a2b3;
    cursor: not-allowed;
    opacity: 0.5;
    transform: none;
  }

  &__icon-button--loading,
  &__icon-button--loading:disabled,
  &__icon-button--loading:disabled:hover {
    background: #eaf5ff;
    color: #1682ff;
    opacity: 1;
  }

  &__icon-button--loading :deep(svg) {
    animation: providers-refresh-spin 0.8s linear infinite;
  }

  &__icon-button--danger {
    color: #98a2b3;
  }

  &__icon-button--danger:hover {
    background: #fff1f0;
    color: #d92d20;
  }

  &__icon-button--warning {
    color: #98a2b3;
  }

  &__icon-button--warning:hover {
    background: #fff7ed;
    color: #c2410c;
  }

  &__add {
    width: 38px;
    height: 38px;
    border-radius: 12px;
    background: var(--color-primary);
    color: #ffffff;
  }

  &__toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 7px;
    border-radius: 999px;
    background: #f0f2f5;
    color: #667085;
  }

  &__toggle input {
    display: none;
  }

  &__toggle span {
    width: 38px;
    height: 22px;
    border-radius: 999px;
    background: #d7dbe1;
  }

  &__toggle span::before {
    content: "";
    display: block;
    width: 20px;
    height: 20px;
    margin: 1px;
    border-radius: 999px;
    background: #ffffff;
  }

  &__toggle input:checked + span::before {
    margin-left: 17px;
  }

  &__list-panel {
    display: flex;
    height: calc(100vh - 76px);
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
    height: calc(100vh - 260px);
  }

  &__runtime {
    flex: none;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid #ffd56a;
    border-radius: 8px;
    background: #fff9e8;
  }

  &__runtime strong {
    overflow: hidden;
    color: #111827;
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
    border: 1px solid #edf1f6;
    border-radius: 12px;
    background: #ffffff;
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
    border-color: #9fd1ff;
    background: #f7fbff;
    box-shadow:
      0 8px 22px rgba(22, 130, 255, 0.12),
      inset 4px 0 0 #1682ff;
  }

  &__account-card--error {
    border-color: #edf1f6;
    background: #fff7f7;
    box-shadow: 0 8px 22px rgba(180, 35, 24, 0.1);
  }

  &__provider-card--disabled,
  &__account-card--disabled {
    border-color: #cbd5e1;
    background: #f8fafc;
    box-shadow: none;
    opacity: 0.72;
  }

  &__provider-card:hover,
  &__account-card:hover {
    border-color: #d8ecff;
    background: #fbfdff;
    box-shadow: 0 10px 24px rgba(15, 23, 42, 0.1);
    transform: translateY(-2px);
  }

  &__provider-card--active:hover,
  &__account-card--active:hover {
    border-color: #9fd1ff;
    background: #f7fbff;
    box-shadow:
      0 12px 26px rgba(22, 130, 255, 0.16),
      inset 4px 0 0 #1682ff;
  }

  &__provider-card--runtime-warning,
  &__provider-card--runtime-warning:hover {
    border-color: #f2b94b;
  }

  &__account-card--error:hover {
    border-color: #edf1f6;
    background: #fff7f7;
    box-shadow: 0 12px 26px rgba(180, 35, 24, 0.14);
  }

  &__provider-card--disabled:hover,
  &__account-card--disabled:hover {
    border-color: #94a3b8;
    background: #f1f5f9;
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
    border-color: #edf1f6;
    background: #ffffff;
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
    border-bottom: 1px solid #edf0f3;
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
    color: #111827;
    font-size: 0.98rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__account-tag {
    flex: none;
    padding: 1px 8px;
    border: 1px solid #bdebd8;
    border-radius: 999px;
    background: #f0fffa;
    color: #0d9f72;
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
    background: #ffffff;
    box-shadow:
      0 14px 34px rgba(52, 64, 84, 0.18),
      0 0 0 1px rgba(253, 162, 155, 0.28);
    color: #344054;
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
    background: #ffffff;
    box-shadow: -1px -1px 0 rgba(253, 162, 155, 0.2);
    transform: translateX(-50%) rotate(45deg);
  }

  &__account-error:hover &__account-error-tip {
    opacity: 1;
    transform: translate(-50%, 0);
    visibility: visible;
  }

  &__account-error-title {
    color: #b42318;
    font-size: 0.72rem;
    font-weight: 800;
  }

  &__account-error-message {
    overflow-wrap: anywhere;
  }

  &__account-tag--pro {
    border-color: #8b5cf6;
    background: #f4f0ff;
    color: #6d28d9;
    font-weight: 800;
  }

  &__account-tag--plus {
    border-color: #1570ef;
    background: #eef7ff;
    color: #175cd3;
    font-weight: 800;
  }

  &__account-tag--error {
    border-color: #fda29b;
    background: #fff1f1;
    color: #b42318;
    font-weight: 800;
  }

  &__account-tag--disabled {
    border-color: #d0d5dd;
    background: #f2f4f7;
    color: #667085;
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
    border: 1px solid #edf1f6;
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
    --quota-color: #ef4444;
    --quota-bg: #fff5f5;
    --quota-icon-bg: #ffe4e6;
  }

  &__account-quota--loading {
    --quota-color: #1682ff;
    --quota-bg: #eef7ff;
    --quota-icon-bg: #eaf5ff;

    border-color: #9fd1ff;
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
    color: #475467;
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
    color: #1682ff;
  }

  &__quota-bar::before {
    content: "";
    display: block;
    width: 100%;
    height: 7px;
    border-radius: 999px;
    background: #dbeee2;
    order: 2;
  }

  &__account-quota--loading &__quota-bar::before {
    background: #cfe6ff;
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
    color: #667085;
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
    color: #c0c4cc;
  }

  &__shield,
  &__avatar,
  &__edit-avatar {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid #e5e7eb;
    border-radius: 12px;
    background: #f8fafc;
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
    color: #344054;
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
    background: #eef7ff;
    transform: scale(1.06);
  }

  &__provider-card--active &__shield,
  &__provider-card--active &__avatar,
  &__account-card--active &__shield,
  &__account-card--active &__avatar {
    border-color: #b7dcff;
    background: #eaf5ff;
    color: #1682ff;
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
    color: #111827;
    font-size: 1rem;
  }

  &__provider-main span {
    overflow: hidden;
    color: #006eff;
    font-size: 0.9rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__provider-title &__account-tag {
    color: #667085;
    font-size: 0.72rem;
  }

  &__provider-main &__provider-note {
    color: #667085;
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
    background: #dff8ee;
    color: #07966f;
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__state-dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: #00b981;
  }

  &__state-pill--disabled {
    background: #f2f4f7;
    color: #667085;
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
    background: #1682ff;
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
    background: #edf2f8;
    color: #7f8da3;
  }

  &__compare-button {
    flex: none;
    border: 1px solid #d92d20;
    background: #ffffff;
    color: #b42318;
    font-size: 12px;
    height: 25px;
  }

  &__reauth-button {
    flex: none;
    height: 28px;
    padding: 0 10px;
    border: 1px solid #fda29b;
    background: #ffffff;
    color: #b42318;
    font-size: 12px;
  }

  &__empty {
    display: flex;
    min-height: 220px;
    align-items: center;
    justify-content: center;
    border: 1px dashed #d8dde5;
    border-radius: 14px;
    color: #667085;
  }

  &__edit-header {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 64px;
    padding: 0 24px;
    background: #ffffff;
  }

  &__back {
    width: 36px;
    height: 36px;
    border: 1px solid #dfe3e8;
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
    border: 1px solid #dfe3e8;
    border-radius: 14px;
    background: #ffffff;
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
    color: #667085;
    font-size: 0.85rem;
  }

  &__icon-panel {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    border: 1px solid #dfe3e8;
    border-radius: 12px;
    background: #fbfcfd;
  }

  &-icon-upload {
    display: inline-flex;
    width: fit-content;
    align-items: center;
    gap: 7px;
    padding: 8px 12px;
    border: 1px solid #c7d7fe;
    border-radius: 8px;
    background: #eff6ff;
    color: #1d4ed8;
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
    color: #475467;
    cursor: pointer;
  }

  &__icon-option--active {
    border-color: #1682ff;
    background: #eef7ff;
    color: #111827;
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
    color: #667085;
  }

  &__field input,
  &__field select {
    min-width: 0;
    height: 38px;
    padding: 0 12px;
    border: 1px solid #dfe3e8;
    border-radius: 8px;
    background: #ffffff;
    color: #111827;
  }

  &__warning {
    padding: 12px 14px;
    border: 1px solid #ffd56a;
    border-radius: 12px;
    background: #fff9e8;
    color: #e07800;
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
    border-top: 1px solid #edf0f3;
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
    border: 1px solid #dfe3e8;
    background: #ffffff;
    color: #667085;
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
    color: #667085;
  }

  &__option-field input[type="number"] {
    width: 112px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid #dfe3e8;
    border-radius: 8px;
    color: #111827;
  }

  &__option-field select {
    height: 32px;
    padding: 0 10px;
    border: 1px solid #dfe3e8;
    border-radius: 8px;
    background: #ffffff;
    color: #111827;
  }

  &__check-row {
    flex-wrap: wrap;
    gap: 16px;
  }

  &__config-preview {
    border: 1px solid #edf0f3;
    border-radius: 8px;
    overflow: hidden;
    background: #ffffff;
  }

  &__config-preview summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    cursor: pointer;
    color: #111827;
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
    border-top: 1px solid #edf0f3;
    background: #f6f9fc;
    color: #243447;
    font-size: 0.85rem;
    line-height: 1.55;
  }

  &__config-preview p {
    margin: 0;
    padding: 10px 14px;
    border-top: 1px solid #edf0f3;
    color: #667085;
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
    border-top: 1px solid #edf0f3;
    background: #ffffff;
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
    border: 1px solid #dfe3e8;
    border-radius: 12px;
    background: #ffffff;
    color: #111827;
    cursor: pointer;
    text-align: left;
  }
  .option-logo {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  &__create-option:hover {
    border-color: #1682ff;
    background: #eef7ff;
  }

  &__create-option svg {
    color: #1682ff;
  }

  &__create-option strong {
    font-size: 1.05rem;
  }

  &__create-option span {
    color: #667085;
    font-size: 0.9rem;
    line-height: 1.6;
  }

  &__provider-create-modal {
    :deep(.base-modal__panel) {
      width: 920px;
      border: 1px solid #d8e0eb;
      border-radius: 16px;
      box-shadow: 0 24px 70px rgba(15, 23, 42, 0.26);
    }

    :deep(.base-modal__header) {
      align-items: center;
      padding: 22px 26px 20px;
      border-bottom: 1px solid #edf1f6;
    }

    :deep(.base-modal__header h2) {
      color: #172033;
      font-size: 1.18rem;
    }

    :deep(.base-modal__close) {
      width: 28px;
      height: 28px;
      border: 0;
      background: transparent;
      color: #667085;
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
    border-top: 1px solid #edf0f3;
    background: #ffffff;
    position: sticky;
    bottom: 0;
  }

  &__codex-login-modal {
    :deep(.base-modal__panel) {
      width: 560px;
      border: 1px solid #d8e0eb;
      border-radius: 16px;
      box-shadow: 0 24px 70px rgba(15, 23, 42, 0.26);
    }

    :deep(.base-modal__header) {
      align-items: center;
      padding: 22px 26px 20px;
      border-bottom: 1px solid #edf1f6;
    }

    :deep(.base-modal__header h2) {
      color: #172033;
      font-size: 1.18rem;
    }

    :deep(.base-modal__close) {
      width: 28px;
      height: 28px;
      border: 0;
      background: transparent;
      color: #667085;
      font-size: 1.3rem;
    }

    :deep(.base-modal__content) {
      padding: 18px 26px 24px;
    }
  }

  &__codex-proxy-modal {
    :deep(.base-modal__panel) {
      width: 440px;
      border: 1px solid #d8e0eb;
      border-radius: 16px;
      box-shadow: 0 24px 70px rgba(15, 23, 42, 0.26);
    }

    :deep(.base-modal__header) {
      align-items: center;
      padding: 22px 26px 20px;
      border-bottom: 1px solid #edf1f6;
    }

    :deep(.base-modal__header h2) {
      color: #172033;
      font-size: 1.18rem;
    }

    :deep(.base-modal__close) {
      width: 28px;
      height: 28px;
      border: 0;
      background: transparent;
      color: #667085;
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
      color: #1f2937;
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
      color: #1f2937;
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
    border: 1px solid #dfe3e8;
  }

  &__diff-footer {
    flex: none;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 12px;
    background: #ffffff;
    position: sticky;
    bottom: 0;
  }

  &__diff-button {
    min-width: 168px;
    height: 34px;
    padding: 0 14px;
    border: 1px solid #d0d5dd;
    border-radius: 7px;
    background: #ffffff;
    color: #475467;
    cursor: pointer;
    font-size: 0.86rem;
    font-weight: 600;
  }

  &__diff-button--primary {
    border-color: #1570ef;
    background: #1570ef;
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
    border-left: 1px solid #d8e0eb;
    background: #ffffff;
    box-shadow: -24px 0 70px rgba(15, 23, 42, 0.18);
  }

  &__drawer-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 22px 24px 18px;
    border-bottom: 1px solid #edf1f6;

    h2 {
      margin: 0;
      color: #172033;
      font-size: 1.18rem;
    }

    p {
      margin: 6px 0 0;
      color: #697789;
      font-size: 0.86rem;
    }
  }

  .providers-view-drawer-tabs {
    display: flex;
    gap: 6px;
    padding: 10px 24px;
    border-bottom: 1px solid #edf1f6;
    background: #f8fbfe;
  }

  .providers-view-drawer-tab {
    height: 30px;
    padding: 0 14px;
    border: 1px solid #d8e0eb;
    border-radius: 7px;
    background: #ffffff;
    color: #526176;
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  .providers-view-drawer-tab-active {
    border-color: #1682ff;
    background: #1682ff;
    color: #ffffff;
  }

  &__drawer-close {
    width: 28px;
    height: 28px;
    border: 0;
    background: transparent;
    color: #667085;
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
    color: #172033;
    font-size: 0.92rem;
  }

  &__drawer-json {
    overflow: auto;
    margin: 0;
    padding: 14px 16px;
    border: 1px solid #d8e0eb;
    border-radius: 10px;
    background: #f7fafc;
    color: #243447;
    font-size: 0.8rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__drawer-empty {
    color: #697789;
    font-size: 0.88rem;
  }

  .providers-view-usage-hero {
    display: flex;
    min-width: 0;
    align-items: stretch;
    justify-content: space-between;
    gap: 12px;
    padding: 16px;
    border: 1px solid #cfdbea;
    border-radius: 8px;
    background: linear-gradient(135deg, #f8fbff 0%, #eef6ff 100%);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.72);
  }

  .providers-view-usage-hero-main,
  .providers-view-usage-hero-side {
    display: flex;
    min-width: 0;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
  }

  .providers-view-usage-hero-main {
    flex: 1;
  }

  .providers-view-usage-hero-side {
    width: 128px;
    align-items: flex-end;
    padding-left: 14px;
    border-left: 1px solid #d7e4f2;
  }

  .providers-view-usage-label {
    color: #697789;
    font-size: 0.78rem;
  }

  .providers-view-usage-value {
    min-width: 0;
    overflow: hidden;
    color: #172033;
    font-size: 1rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .providers-view-usage-total {
    min-width: 0;
    overflow: hidden;
    color: #101828;
    font-size: 1.46rem;
    font-weight: 800;
    line-height: 1.12;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .providers-view-usage-cost {
    color: #114a8b;
    font-size: 1.22rem;
    font-weight: 800;
    line-height: 1.1;
  }

  .providers-view-usage-subtext {
    color: #7a899c;
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
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid #d8e0eb;
    border-radius: 8px;
    background: #f8fbfe;
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
    border: 1px solid #d5dfeb;
    border-radius: 8px;
    background: #ffffff;
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
    border: 1px solid #d8e0eb;
    border-radius: 10px;
    background: #edf3f9;
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
    color: #526176;
    cursor: pointer;
    font-size: 0.72rem;
    line-height: 0.72rem;
    font-weight: 700;
    white-space: nowrap;
  }

  &__login-tab--active {
    background: #1682ff;
    color: #ffffff;
  }

  &__login-intro {
    margin: 10px 0 0;
    color: #697789;
    font-size: 0.9rem;
    line-height: 1.6;
  }

  &__login-field {
    display: flex;
    flex-direction: column;
    gap: 8px;

    span {
      color: #8a97aa;
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
    border: 1px solid #d8e0eb;
    border-radius: 10px;
    background: #f7fafc;
    color: #526176;
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
    border: 1px solid #d8e0eb;
    border-radius: 10px;
    background: #ffffff;
    color: #526176;
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
    color: #526176;

    &:disabled {
      cursor: not-allowed;
      opacity: 0.52;
    }
  }

  &__login-tip {
    display: flex;
    align-items: center;
    color: #9aa6b7;
  }

  &__login-actions {
    display: flex;
    justify-content: flex-end;

    button {
      height: 32px;
      padding: 0 12px;
      border: 1px solid #d8e0eb;
      border-radius: 8px;
      background: #ffffff;
      color: #172033;
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
      border: 1px solid #bfe5ce;
      background: #f0fbf4;
      color: #17803d;
    }

    &--failed {
      border: 1px solid #ffd0d0;
      background: #fff1f1;
      color: #c12626;
    }

    &--cancelled {
      border: 1px solid #ffd56a;
      background: #fff9e8;
      color: #e07800;
    }
  }
}
</style>

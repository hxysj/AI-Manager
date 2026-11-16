import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { compileScript, compileStyleAsync, compileTemplate, parse } from '@vue/compiler-sfc'
import { transformSync } from 'esbuild'
import { createSSRApp, h, reactive } from 'vue'
import { renderToString } from '@vue/server-renderer'

const require = createRequire(import.meta.url)
const files = [
  'src/App.vue',
  'src/components/QuickSwitchPanel.vue',
  'src/features/providers/components/ServiceProviderCenter.vue',
  'src/features/tools/index.vue',
  'src/features/assistant/components/AssistantModelSelect.vue',
  'src/features/settings/components/TranslationAgentSettings.vue',
  'src/features/settings/components/TranslationConfigModal.vue',
  'src/features/settings/index.vue',
  'src/features/settings/components/SystemSettings.vue',
  'src/features/usage/index.vue',
  'src/components/TokenCount.vue'
]
let centerCode
let settingsCode
const selectorComponents = []
for (const filename of files) {
  const { descriptor, errors } = parse(readFileSync(filename, 'utf8'), { filename })
  assert.deepEqual(errors, [], filename)
  const script = compileScript(descriptor, { id: 'service-provider-test', genDefaultAs: '__component' })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename,
    id: 'service-provider-test',
    compilerOptions: { bindingMetadata: script.bindings }
  })
  assert.deepEqual(template.errors, [], filename)
  transformSync(script.content, { format: 'esm' })
  for (const style of descriptor.styles) {
    const result = await compileStyleAsync({
      source: style.content,
      filename,
      id: 'service-provider-test',
      scoped: style.scoped,
      preprocessLang: style.lang
    })
    assert.deepEqual(result.errors, [], filename)
  }
  if (filename.endsWith('ServiceProviderCenter.vue')) {
    centerCode = transformSync(`${script.content}\n${template.code}\n__component.render = render\nexport default __component`, { format: 'cjs' }).code
  } else if (filename.endsWith('SystemSettings.vue')) {
    settingsCode = transformSync(`${script.content}\n${template.code}\n__component.render = render\nexport default __component`, { format: 'cjs' }).code
  } else if (/AssistantModelSelect|TranslationAgentSettings|TranslationConfigModal/.test(filename)) {
    selectorComponents.push({
      filename,
      code: transformSync(`${script.content}\nexport default __component`, { format: 'cjs' }).code
    })
  }
}

// 使用真实配置中心的 setup、模板和保存逻辑，隔离桌面 IPC 与文件写入。
let state = {
  backend: 'service',
  baseUrl: 'http://127.0.0.1:15724',
  providers: [{ id: 'p', name: '统一服务商', icon: 'alibabacloud.svg', kind: 'api', enabled: 1, hasCredential: 1, revision: 7, upstreamProtocol: 'responses' }],
  models: [{ id: 'm', providerId: 'p', upstreamModelId: 'upstream-model', displayName: '测试模型', enabled: 1, inputPriceMicros: 2000000, catalog: { custom: true } }],
  bindings: [{ providerId: 'p', agentType: 'codex', enabled: 1, defaultModelId: 'm', options: { serviceTierFast: true } }],
  keys: [],
  instances: [
    { id: 'external', agentType: 'gateway', name: '外部工具', configPath: 'external:one', status: 'active' },
    { id: 'internal', agentType: 'codex', name: '内置会话', configPath: 'internal:one', status: 'active' }
  ],
  managedFiles: [],
  routes: [{ agentType: 'codex', providerId: 'p', clientModelName: 'alias', modelId: 'm', enabled: 1 }],
  usage: {
    requests: { requestCount: 2, successCount: 1 },
    attempts: { attemptCount: 1, measuredCount: 0 },
    byProvider: [{ groupKey: 'p', groupName: '统一服务商', currency: 'CNY', attemptCount: 1, successCount: 1, unknownUsageCount: 1, pricedCount: 0, estimatedCostMicros: null }],
    logs: [{ id: 'log', state: 'unknown', inputTokens: null, outputTokens: null, estimatedCostMicros: null }]
  }
}
const calls = []
const messages = []
let failState = false
const invoke = async (action, payload) => {
  calls.push({ action, payload })
  if (action === 'state') {
    if (failState) throw new Error('状态读取失败')
    return state
  }
  if (action === 'detail') return { headers: { 'x-test': 'yes' }, revision: 7 }
  if (action === 'issue-token') return { token: 'shown-once-token', baseUrl: state.baseUrl }
  if (action === 'switch-backend' && payload.backend === 'legacy') {
    state = { ...state, managedFiles: [{ id: 'file', instanceId: 'external', restoreStatus: 'conflict', errorMessage: '文件被外部修改' }] }
    throw new Error('恢复文件冲突')
  }
  return {}
}
const localRequire = name => {
  if (name === '@/api/modules/service-providers') return { serviceProviderApi: { invoke } }
  if (name === '@/utils/message') return { createMessage: { success: message => messages.push(message), error: message => messages.push(message) } }
  // 记录传入已有 AiIcon 的图标配置，验证列表没有再写死 Agent 图标。
  if (name === '@/components/AiIcon.vue') return {
    __esModule: true,
    default: { props: ['name', 'alt'], render() { return h('span', { 'data-provider-icon': this.name, 'aria-label': this.alt }) } }
  }
  if (name.endsWith('.vue')) {
    return {
      __esModule: true,
      default: { inheritAttrs: false, render() { return this.$slots.default?.() } }
    }
  }
  return require(name)
}
const module = { exports: {} }
new Function('require', 'module', 'exports', centerCode)(localRequire, module, module.exports)
const Center = module.exports.default
let bindings
const app = createSSRApp({
  ...Center,
  setup(props, context) {
    bindings = Center.setup(props, context)
    return bindings
  }
}, { initialState: state, cliTargets: [{ id: 'codex', configPath: 'D:/test-codex', version: 'test' }] })
const failures = []
app.config.errorHandler = error => failures.push(error)
const html = await renderToString(app)
assert.deepEqual(failures, [])
assert.ok(html.includes('统一服务商') && html.includes('provider-avatar') && html.includes('icon-actions'))
assert.ok(html.includes('管理 API Key') && html.includes('查看详情'))
assert.ok(html.includes('data-provider-icon="alibabacloud.svg"'))
assert.ok(!html.includes('网关统计') && !html.includes('外部工具') && !html.includes('服务商配置中心'))
assert.ok(!html.includes('内置会话'))
assert.ok(!/<button\b[^>]*>\s*(?:复制旧 Codex 配置|启用新模式|回滚旧模式)\s*<\/button>/u.test(html))
const renderCenter = async (initialState, configure) => {
  const app = createSSRApp({
    ...Center,
    setup(props, context) {
      const value = Center.setup(props, context)
      configure?.(value)
      return value
    }
  }, { initialState })
  return renderToString(app)
}
const managerHtml = await renderCenter(state, value => { value.showGatewayManager.value = true })
assert.ok(managerHtml.includes('网关统计') && managerHtml.includes('未知') && managerHtml.includes('外部工具'))
assert.ok(!managerHtml.includes('内置会话'))
// 官方账号显示邮箱，类型筛选和禁用项筛选只影响列表，不改变后端授权。
const official = { id: 'account:internal-id', name: 'account:internal-id', kind: 'codex_account', enabled: 1, hasCredential: 1 }
const disabled = { id: 'disabled', name: '已禁用服务商', kind: 'api', enabled: 0 }
const listState = {
  ...state,
  providers: [...state.providers, official, disabled],
  accounts: [{ providerId: official.id, email: 'official@example.test', quota: { rate_limit: { primary_window: { used_percent: 25, limit_window_seconds: 18000 } } } }]
}
const listHtml = await renderCenter(listState)
assert.ok(listHtml.includes('official@example.test') && listHtml.includes('75%'))
assert.ok(!listHtml.includes('account:internal-id') && !listHtml.includes('已禁用服务商'))
const accountHtml = await renderCenter(listState, value => { value.providerTab.value = 'codex_account' })
assert.ok(accountHtml.includes('official@example.test') && !accountHtml.includes('统一服务商'))
const customIcon = 'data:image/png;base64,aWNvbg=='
const customIconHtml = await renderCenter({ ...listState, providers: [{ ...official, icon: customIcon }] })
assert.ok(customIconHtml.includes(`data-provider-icon="${customIcon}"`))
const noIconHtml = await renderCenter({ ...state, providers: [{ ...state.providers[0], icon: '' }] })
assert.ok(!noIconHtml.includes('data-provider-icon'))
const disabledHtml = await renderCenter(listState, value => { value.showDisabledItems.value = true })
assert.ok(disabledHtml.includes('已禁用服务商'))
const keysHtml = await renderCenter(state, value => { value.openProviderDetail(state.providers[0], 'keys') })
assert.ok(keysHtml.includes('添加 / 更新 Key') && keysHtml.includes('尚未添加 API Key'))
await bindings.toggleProviderEnabled(state.providers[0])
const disabledPayload = calls.find(item => item.action === 'save').payload
assert.equal(disabledPayload.enabled, false)
assert.equal(disabledPayload.headers['x-test'], 'yes')
assert.equal(disabledPayload.revision, 7)
assert.equal(disabledPayload.models, undefined)
assert.equal(disabledPayload.icon, 'alibabacloud.svg')
calls.length = 0
const instance = { id: 'one', providerId: 'p', agentType: 'codex', configPath: 'D:/test-codex', status: 'active' }
bindings.data.value = { ...state, instances: [instance] }
await bindings.stopUsingProvider(state.providers[0])
assert.deepEqual(calls.find(item => item.action === 'detach').payload, { id: 'one' })
calls.length = 0
bindings.data.value = { ...state, instances: [instance, { ...instance, id: 'two' }] }
await bindings.stopUsingProvider(state.providers[0])
assert.equal(calls.length, 0)
assert.equal(bindings.detailProviderId.value, 'p')
assert.equal(bindings.detailTab.value, 'instances')
bindings.data.value = state
await bindings.editProvider({ ...state.providers[0], enabled: false })
assert.equal(bindings.form.value.enabled, false)
await bindings.editProvider(state.providers[0])
assert.equal(bindings.form.value.revision, 7)
assert.equal(bindings.form.value.bindings[0].defaultModelId, 'm')
bindings.form.value.bindings.find(item => item.agentType === 'claude').enabled = true
await bindings.saveProvider()
const saved = calls.find(item => item.action === 'save').payload
assert.equal(saved.models[0].catalog.custom, true)
assert.equal(saved.models[0].inputPriceMicros, 2000000)
assert.equal(saved.bindings.find(item => item.agentType === 'claude').enabled, true)
assert.equal(saved.routes[0].modelId, 'm')
assert.equal(saved.revision, 7)
assert.equal(saved.icon, 'alibabacloud.svg')
assert.equal(bindings.form.value, null)
bindings.openApply(state.providers[0])
assert.equal(bindings.applyForm.value.modelId, 'm')
assert.equal(bindings.applyForm.value.configPath, 'D:/test-codex')
await bindings.perform('issue-token', { providerId: 'p', modelId: 'm' })
assert.equal(bindings.createdToken.value.token, 'shown-once-token')
assert.equal(await bindings.perform('switch-backend', { backend: 'legacy' }), false)
assert.equal(bindings.data.value.managedFiles[0].restoreStatus, 'conflict')
assert.ok(messages.includes('恢复文件冲突'))
failState = true
assert.equal(await bindings.perform('switch-backend', { backend: 'legacy' }), false)
assert.equal(bindings.busy.value, false)

// 模式入口位于系统设置；等待同步时阻止重复点击，失败保持旧模式，成功后显示后端新状态。
const settingsModule = { exports: {} }
new Function('require', 'module', 'exports', settingsCode)(name => {
  if (name === '@/api') return { appApi: {} }
  if (name === '@/features/tools/constant') return { getToolItems: () => [] }
  return localRequire(name)
}, settingsModule, settingsModule.exports)
const SystemSettings = settingsModule.exports.default
const modeState = reactive({ backend: 'legacy' })
const modeCalls = []
let finishMode
let modeBindings
const settingsProps = {
  settings: {}, cliTargets: [], serviceProviders: modeState, pending: false,
  saveSettings: async () => true,
  switchProviderBackend: backend => {
    modeCalls.push(backend)
    return new Promise(resolve => { finishMode = resolve })
  }
}
const settingsApp = createSSRApp({
  ...SystemSettings,
  setup(props, context) {
    modeBindings = SystemSettings.setup(props, context)
    return modeBindings
  }
}, settingsProps)
const settingsHtml = await renderToString(settingsApp)
assert.ok(settingsHtml.includes('共享服务商模式') && settingsHtml.includes('自动同步旧 Codex'))
const enabling = modeBindings.toggleProviderMode()
assert.equal(modeBindings.providerModePending.value, true)
await modeBindings.toggleProviderMode()
assert.deepEqual(modeCalls, ['service'])
finishMode(false)
assert.equal(await enabling, false)
assert.equal(modeBindings.serviceMode.value, false)
assert.equal(modeBindings.providerModePending.value, false)
const retry = modeBindings.toggleProviderMode()
modeState.backend = 'service'
finishMode(true)
assert.equal(await retry, true)
assert.equal(modeBindings.serviceMode.value, true)
const rollback = modeBindings.toggleProviderMode()
assert.equal(modeCalls.at(-1), 'legacy')
finishMode(false)
assert.equal(await rollback, false)
assert.equal(modeBindings.serviceMode.value, true)

// 原 Usage 组件在两种模式共用，未知值不在显示层被转换成零。
const appSource = readFileSync('src/App.vue', 'utf8')
assert.ok(!appSource.includes('usage-only') && !appSource.includes('serviceCenterOpen'))
assert.ok(appSource.includes('<UsageView v-else-if="activeView === \'usage\'" :usage="state.usage" />'))
const formatterSource = transformSync(readFileSync('src/utils/formatters.js', 'utf8'), { format: 'cjs' }).code
const formatter = { exports: {} }
new Function('module', 'exports', formatterSource)(formatter, formatter.exports)
assert.equal(formatter.exports.formatTokenCountParts(null).compact, '未知')
assert.equal(formatter.exports.formatTokenCountParts(0).compact, '0')

// 官方账号只能展示自身新表模型，历史公开 ID 不叠加 account: 前缀。
for (const { filename, code } of selectorComponents) {
  const compiled = { exports: {} }
  new Function('require', 'module', 'exports', code)(name => {
    if (name.startsWith('@/')) return name.endsWith('.vue')
      ? localRequire(name)
      : {}
    if (name.endsWith('.css') || name.endsWith('/style/css')) return {}
    return require(name)
  }, compiled, compiled.exports)
  const Component = compiled.exports.default
  let selector
  const selection = { targetId: 'account:official', model: 'own-model' }
  const selectorApp = createSSRApp({
    ...Component,
    setup(props, context) {
      selector = Component.setup(props, context)
      return selector
    },
    render: () => null
  }, {
    settings: selection,
    modelValue: selection,
    providers: [{ id: 'other', name: '其他服务商', cli: 'codex', enabled: true }],
    codexAccounts: [{ id: 'official', serviceProviderId: 'account:official', serviceManaged: true, modelNames: ['own-model'] }],
    runtimeModels: [
      { id: 'own', providerId: 'account:official', name: 'own-model' },
      { id: 'other-model', providerId: 'other', name: 'other-model' }
    ]
  })
  await renderToString(selectorApp)
  if (filename.endsWith('AssistantModelSelect.vue')) {
    const official = selector.modelGroups.value.find(item => item.targetId === 'account:official')
    assert.deepEqual(official.models.map(item => item.model), ['own-model'])
    assert.ok(!selector.modelGroups.value.some(item => item.targetId === 'account:account:official'))
  } else {
    assert.deepEqual(
      selector.modelOptions.value.map(item => typeof item === 'string' ? item : item.id),
      ['own-model']
    )
    assert.equal(selector.invalidTarget.value, false)
  }
}
console.log(`${files.length} 个 SFC 编译通过；原列表布局、图标继承、类型/禁用筛选、账号展示、Key 入口、实例取消、设置模式切换、Usage 保留、配置保存、授权、Token 及回滚交互通过`)

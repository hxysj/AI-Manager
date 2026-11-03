import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { compileScript, compileStyleAsync, compileTemplate, parse } from '@vue/compiler-sfc'
import { transformSync } from 'esbuild'
import * as vue from 'vue'
import { renderToString } from '@vue/server-renderer'

const require = createRequire(import.meta.url)

async function compileComponent(filename, id, localRequire) {
  const source = readFileSync(filename, 'utf8')
  const { descriptor, errors } = parse(source, { filename })
  assert.deepEqual(errors, [])
  assert.equal(descriptor.styles[0].lang, 'less')
  assert.equal(descriptor.styles[0].scoped, true)
  assert.ok(!/font-weight|@media/.test(descriptor.styles[0].content))
  const script = compileScript(descriptor, { id, genDefaultAs: '__component' })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename,
    id,
    compilerOptions: { bindingMetadata: script.bindings }
  })
  assert.deepEqual(template.errors, [])
  const style = await compileStyleAsync({
    source: descriptor.styles[0].content,
    filename,
    id: `data-v-${id}`,
    scoped: true,
    preprocessLang: 'less'
  })
  assert.deepEqual(style.errors, [])
  const module = { exports: {} }
  const code = transformSync(`${script.content}\n${template.code}\n__component.render = render\nexport default __component`, { format: 'cjs' }).code
  new Function('require', 'module', 'exports', code)(localRequire, module, module.exports)
  return { component: module.exports.default, css: style.code }
}

// 直接执行真实 setup，模拟管理接口，验证草稿与一次性密钥不会互相污染。
let mounted
let persisted = {
  enabled: false,
  host: '127.0.0.1',
  port: 8046,
  baseUrl: 'http://127.0.0.1:8046',
  providerIds: ['provider'],
  modelMappings: {},
  tokens: [],
  usage: {}
}
const calls = []
const api = {
  getState: async () => structuredClone(persisted),
  createToken: async payload => {
    calls.push(['create', payload])
    return { ...structuredClone(persisted), createdToken: { token: 'mpg_once' } }
  },
  saveConfig: async payload => {
    calls.push(['save', payload])
    persisted = { ...persisted, ...payload }
    return structuredClone(persisted)
  },
  enable: async payload => {
    calls.push(['enable', payload])
    persisted = { ...persisted, ...payload, enabled: true }
    return structuredClone(persisted)
  },
  disable: async () => {
    persisted.enabled = false
    return structuredClone(persisted)
  },
  setTokenEnabled: async payload => {
    calls.push(['set-enabled', payload])
    return structuredClone(persisted)
  },
  deleteToken: async payload => {
    calls.push(['delete', payload])
    return structuredClone(persisted)
  }
}
const messages = []
let connectionComponent
const localRequire = name => {
  if (name === 'vue') return { ...vue, onMounted: callback => { mounted = callback } }
  if (name === '@/api') return { multiProtocolGatewayApi: api }
  if (name === '@/utils/message') return { createMessage: { success() {}, error: message => messages.push(message) } }
  if (name === '@/utils/formatters') return { formatDateTime: String, formatTokenCount: value => String(value || 0) }
  if (name.endsWith('/GatewayConnectionPanel.vue')) return { __esModule: true, default: connectionComponent }
  if (name.endsWith('.vue')) return { __esModule: true, default: {} }
  return require(name)
}
const connection = await compileComponent('src/features/tools/multiProtocolGateway/components/GatewayConnectionPanel.vue', 'connection-check', localRequire)
connectionComponent = connection.component
assert.ok(connection.css.includes('.gateway-connection .protocol-panel .protocol-cards .protocol-card .endpoint-list .endpoint-item .endpoint-picker .endpoint-path'))
assert.ok(connection.css.includes('.gateway-connection .integration-panel .integration-body .model-browser .model-table-wrap .model-table .model-body .model-row .model-cell .model-picker .model-name'))
const gateway = await compileComponent('src/features/tools/multiProtocolGateway/index.vue', 'gateway-check', localRequire)
assert.ok(gateway.css.includes('.gateway-tool .gateway-content .gateway-panel .target-list .target-item .target-info .target-name'))
const setup = gateway.component.setup({
  providers: [
    { id: 'provider', name: '普通 Provider', enabled: true, cli: 'claude' },
    { id: 'google', type: 'google-account', enabled: true },
    { id: 'disabled', enabled: false }
  ],
  codexAccounts: [{ id: 'official', email: 'test@example.com' }, { id: 'off', disabled: true }]
}, { expose() {} })
await mounted()
assert.deepEqual(setup.targets.value.map(target => target.id), ['provider', 'google', 'account:official'])
setup.draft.port = 8047
Object.assign(setup.mapping, { alias: 'alias', targetId: 'provider', model: 'real-model' })
setup.addMapping()
assert.equal(setup.draft.modelMappings.alias.model, 'real-model')
setup.tokenDraft.name = '调用方'
await setup.createToken()
assert.equal(setup.createdToken.value, 'mpg_once')
assert.ok(!Object.hasOwn(setup.gateway.value, 'createdToken'))
assert.equal(setup.draft.port, 8047)
assert.equal(setup.draft.modelMappings.alias.model, 'real-model')
await setup.saveConfig()
assert.equal(calls.find(call => call[0] === 'save')[1].modelMappings.alias.model, 'real-model')
await setup.toggleGateway()
assert.equal(setup.gateway.value.enabled, true)
await setup.setTokenEnabled({ id: 'token-one', enabled: true })
assert.deepEqual(calls.at(-1), ['set-enabled', { tokenId: 'token-one', enabled: false }])
setup.deletingToken.value = { id: 'token-one' }
await setup.deleteToken()
assert.equal(setup.deletingToken.value, null)
assert.deepEqual(calls.at(-1), ['delete', { tokenId: 'token-one' }])
assert.deepEqual(messages, [])
for (const [tab, expected] of [['config', '底层池'], ['connections', '支持模型与集成'], ['tokens', '创建 Token'], ['logs', '最近请求']]) {
  setup.activeTab.value = tab
  const app = vue.createSSRApp({ ...gateway.component, setup: () => setup })
  const failures = []
  app.config.errorHandler = error => failures.push(error)
  const html = await renderToString(app)
  assert.deepEqual(failures, [])
  assert.ok(html.includes(expected))
  assert.equal(html.includes('Multi-Protocol Support'), tab === 'connections')
  if (tab !== 'tokens') assert.ok(!html.includes('mpg_once'))
}

// 使用真实模板的事件处理器，验证选择、复制与示例联动，而非复制一套页面逻辑。
const panelProps = vue.reactive({
  baseUrl: 'http://127.0.0.1:9000///',
  enabled: true,
  models: [
    { id: 'public-model', owned_by: 'provider' },
    { id: 'second-model', owned_by: '普通 Provider' }
  ],
  modelMappings: { 'public-model': { targetId: 'provider', model: 'real-model' } },
  targets: [{ id: 'provider', name: '普通 Provider' }]
})
const emitted = []
const panelSetup = connectionComponent.setup(panelProps, { expose() {}, emit: name => emitted.push(name) })
const panelProxy = vue.proxyRefs(panelSetup)
// 保留 Vue 渲染上下文，在读取模板事件时也能正确处理 v-model 指令。
let renderPanelTree
await renderToString(vue.createSSRApp({
  render() {
    renderPanelTree = vue.withCtx(() => connectionComponent.render({}, [], panelProps, panelProxy))
    return renderPanelTree()
  }
}))
function panelNodes(node = renderPanelTree()) {
  if (!node || typeof node !== 'object') return []
  return [node, ...(Array.isArray(node.children) ? node.children.flatMap(panelNodes) : [])]
}
function panelNode(className) {
  const node = panelNodes().find(node => typeof node.props?.class === 'string' && node.props.class.split(' ').includes(className))
  assert.ok(node, `找不到 ${className}`)
  return node
}
function clickLabel(label) {
  const node = panelNodes().find(node => node.props?.['aria-label'] === label)
  assert.ok(node, `找不到按钮 ${label}`)
  return node.props.onClick({ stopPropagation() {} })
}
async function renderPanel() {
  return renderToString(vue.createSSRApp({ ...connectionComponent, setup: () => panelSetup }, panelProps))
}
assert.equal(panelSetup.modelId.value, 'public-model')
assert.equal(panelSetup.modelEntries.value[0].targetName, '普通 Provider')
assert.equal(panelSetup.modelEntries.value[0].realModel, 'real-model')
const copied = []
const navigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator')
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { clipboard: { writeText: async text => copied.push(text) } } })
try {
  for (const protocol of panelSetup.protocols) {
    await clickLabel(`复制 ${protocol.name} BASE`)
    assert.equal(copied.at(-1), `http://127.0.0.1:9000${protocol.basePath}`)
    for (const endpoint of protocol.endpoints) {
      const node = panelNodes().find(node => node.props?.class?.includes('endpoint-picker') && node.children?.some(child => child.props?.class === 'endpoint-path' && child.children === endpoint.path))
      assert.ok(node)
      node.props.onClick()
      assert.equal(panelSetup.selectedProtocol.value, protocol.id)
      assert.equal(panelSetup.activeEndpoint.value.id, endpoint.id)
      await clickLabel(`复制 ${endpoint.name} 地址`)
      assert.equal(copied.at(-1), `http://127.0.0.1:9000${endpoint.path.replace('{model}', 'public-model')}`)
      assert.equal(panelSetup.modelsPath.value, protocol.id === 'gemini' ? '/v1beta/models' : '/v1/models')
      for (const language of ['python', 'curl', 'powershell']) {
        panelNode('language-select').props['onUpdate:modelValue'](language)
        const snippet = panelSetup.snippet.value
        const header = protocol.id === 'openai' ? 'Authorization' : protocol.id === 'anthropic' ? 'x-api-key' : 'x-goog-api-key'
        assert.ok(snippet.includes(header))
        assert.ok(snippet.includes('AI_MANAGER_TOKEN'))
        assert.ok(!snippet.includes('mpg_once'))
        assert.ok(!snippet.includes('YOUR_MODEL'))
        const body = language === 'python'
          ? JSON.parse(snippet.match(/payload = ([\s\S]+?)\n\nrequest =/)[1])
          : language === 'powershell'
            ? JSON.parse(snippet.match(/\$body = @'\n([\s\S]+?)\n'@/)[1])
            : JSON.parse(snippet.match(/--data-raw '([\s\S]+)'$/)[1])
        if (protocol.id === 'gemini') assert.equal(body.contents[0].parts[0].text, '你好')
        else {
          assert.equal(body.model, 'public-model')
          if (endpoint.id === 'responses') assert.equal(body.input, '你好')
          else assert.equal(body.messages[0].content, '你好')
          if (protocol.id === 'anthropic') assert.equal(body.max_tokens, 1024)
        }
        await panelNode('code-copy').props.onClick()
        assert.equal(copied.at(-1), snippet)
        const html = await renderPanel()
        assert.ok(html.includes('服务运行中'))
        assert.ok(html.includes('快速集成'))
      }
    }
  }
  await panelNode('models-copy').props.onClick()
  assert.equal(copied.at(-1), 'http://127.0.0.1:9000/v1beta/models')
  await clickLabel('复制模型 second-model')
  assert.equal(copied.at(-1), 'second-model')
  const secondModel = panelNodes().find(node => node.props?.class === 'model-picker' && node.children?.some(child => child.props?.class === 'model-name' && child.children === 'second-model'))
  secondModel.props.onClick()
  assert.equal(panelSetup.modelId.value, 'second-model')
  assert.ok(panelSetup.endpointUrl(panelSetup.activeEndpoint.value).includes('second-model'))
  panelNode('token-link').props.onClick()
  assert.deepEqual(emitted, ['manage-tokens'])

  // 特殊别名保持转义，且模型删除、空池、停服后不会继续展示过期选择或状态。
  const specialId = 'model/中文\'"$`<img src=x onerror=alert(1)>\n'
  panelProps.models.push({ id: specialId })
  panelSetup.selectedModelId.value = specialId
  assert.ok(panelSetup.endpointUrl(panelSetup.activeEndpoint.value).includes(encodeURIComponent(specialId)))
  const specialHtml = await renderPanel()
  assert.ok(specialHtml.includes('&lt;img'))
  assert.ok(!specialHtml.includes('<img src=x'))
  panelSetup.language.value = 'curl'
  assert.ok(panelSetup.snippet.value.includes("'\\''"))
  panelSetup.language.value = 'powershell'
  assert.ok(panelSetup.snippet.value.includes("''"))
  panelProps.models = [{ id: 'replacement' }]
  assert.equal(panelSetup.modelId.value, 'replacement')
  panelProps.models = []
  panelProps.enabled = false
  assert.equal(panelSetup.modelId.value, '')
  assert.equal(panelNode('code-copy').props.disabled, true)
  const emptyHtml = await renderPanel()
  assert.ok(emptyHtml.includes('暂时没有可用模型'))
  assert.ok(emptyHtml.includes('服务未开启'))
  assert.ok(panelSetup.snippet.value.includes('YOUR_MODEL'))
  assert.ok(!emptyHtml.includes('服务运行中'))
  panelProps.baseUrl = ''
  assert.equal(panelSetup.base.value, 'http://127.0.0.1:8046')
  assert.deepEqual(messages, [])
} finally {
  if (navigatorDescriptor) Object.defineProperty(globalThis, 'navigator', navigatorDescriptor)
  else delete globalThis.navigator
}
console.log('多协议反代：Vue、嵌套 scoped Less、四个页签、配置草稿、Token、协议模型联动、15 组示例与复制交互验证通过')

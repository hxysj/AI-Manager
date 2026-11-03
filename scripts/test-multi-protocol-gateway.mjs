import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { compileScript, compileStyleAsync, compileTemplate, parse } from '@vue/compiler-sfc'
import { transformSync } from 'esbuild'
import * as vue from 'vue'
import { renderToString } from '@vue/server-renderer'

const require = createRequire(import.meta.url)
const filename = 'src/features/tools/multiProtocolGateway/index.vue'
const source = readFileSync(filename, 'utf8')
const { descriptor, errors } = parse(source, { filename })
assert.deepEqual(errors, [])
assert.equal(descriptor.styles[0].lang, 'less')
assert.equal(descriptor.styles[0].scoped, true)
assert.ok(!/font-weight|@media/.test(descriptor.styles[0].content))
const script = compileScript(descriptor, { id: 'gateway-check', genDefaultAs: '__component' })
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: 'gateway-check',
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
const style = await compileStyleAsync({
  source: descriptor.styles[0].content,
  filename,
  id: 'data-v-gateway-check',
  scoped: true,
  preprocessLang: 'less'
})
assert.deepEqual(style.errors, [])
assert.ok(style.code.includes('.gateway-tool .gateway-content .gateway-panel .target-list .target-item .target-info .target-name'))

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
const localRequire = name => {
  if (name === 'vue') return { ...vue, onMounted: callback => { mounted = callback } }
  if (name === '@/api') return { multiProtocolGatewayApi: api }
  if (name === '@/utils/message') return { createMessage: { success() {}, error: message => messages.push(message) } }
  if (name === '@/utils/formatters') return { formatDateTime: String, formatTokenCount: value => String(value || 0) }
  if (name.endsWith('.vue')) return { __esModule: true, default: {} }
  return require(name)
}
const module = { exports: {} }
const code = transformSync(`${script.content}\n${template.code}\n__component.render = render\nexport default __component`, { format: 'cjs' }).code
new Function('require', 'module', 'exports', code)(localRequire, module, module.exports)
const setup = module.exports.default.setup({
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
for (const [tab, expected] of [['config', '底层池'], ['tokens', '创建 Token'], ['logs', '最近请求']]) {
  setup.activeTab.value = tab
  const app = vue.createSSRApp({ ...module.exports.default, setup: () => setup })
  const failures = []
  app.config.errorHandler = error => failures.push(error)
  const html = await renderToString(app)
  assert.deepEqual(failures, [])
  assert.ok(html.includes(expected))
}
console.log('多协议反代：Vue、嵌套 scoped Less、三个页签渲染、配置草稿、账号池和 Token 交互验证通过')

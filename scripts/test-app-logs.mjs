import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { webcrypto } from 'node:crypto'
import { parse, compileScript, compileTemplate, compileStyleAsync } from '@vue/compiler-sfc'
import { transformSync } from 'esbuild'

const require = createRequire(import.meta.url)
const vue = require('vue')
const calls = []
const handlers = new Map()
let failInvoke = false
const native = {
  isTauri: () => true,
  invoke: async (command, args) => {
    calls.push({ command, args })
    if (failInvoke && args?.channel !== 'app-log:record') throw new Error('模拟接口失败')
    return { ok: true }
  }
}
Object.defineProperty(globalThis, 'crypto', { configurable: true, value: webcrypto })
globalThis.location = { search: '?panel=screenshot' }
globalThis.document = { querySelector: () => ({ dataset: { activeView: 'providers' } }) }
globalThis.window = { addEventListener: (name, callback) => handlers.set(name, callback) }
function loadScript(path, modules) {
  const module = { exports: {} }
  const source = readFileSync(path, 'utf8')
  const code = transformSync(source, { format: 'cjs' }).code
  new Function('require', 'module', 'exports', code)(
    name => modules[name] || require(name), module, module.exports
  )
  return module.exports
}
const logger = loadScript('src/utils/app-logger.js', { '@tauri-apps/api/core': native })
const request = loadScript('src/api/request.js', {
  '@tauri-apps/api/core': native,
  '@tauri-apps/api/event': { listen: async () => () => {} },
  '@/utils/app-logger': logger
})

// 使用真实请求封装验证 traceId 贯通、失败原样抛出以及日志故障不递归。
await request.request('screenshot:ocr', { model: 'vision' })
const first = calls.at(-1).args
assert.equal(first.payload.model, 'vision')
assert.equal(first.context.panel, 'screenshot')
assert.equal(first.context.view, 'providers')
assert.ok(first.context.callSite.includes('createLogContext'))
failInvoke = true
await assert.rejects(request.request('provider:save', { id: 'provider-a' }), /模拟接口失败/)
const ipc = calls.at(-1).args.payload
const failed = calls.at(-2).args
assert.equal(ipc.context.traceId, failed.context.traceId)
assert.equal(ipc.channel, 'provider:save')
assert.equal(ipc.kind, 'ipc-error')
assert.ok(ipc.error.stack.includes('模拟接口失败'))
assert.ok(ipc.durationMs >= 0)
const beforeLogFailure = calls.length
await assert.rejects(request.request('app-log:list'), /模拟接口失败/)
assert.equal(calls.length, beforeLogFailure + 1)
await assert.rejects(request.invokeWithDiagnostics('screenshot_frame'), /模拟接口失败/)
assert.equal(calls.at(-1).args.payload.channel, 'screenshot_frame')
failInvoke = false
logger.installRuntimeLogging()
logger.installRuntimeLogging()
assert.equal(handlers.size, 2)
handlers.get('error')({ error: new Error('页面错误'), filename: 'screen.js', lineno: 12, colno: 5 })
assert.equal(calls.at(-1).args.payload.context.line, 12)
assert.equal(calls.at(-1).args.payload.context.column, 5)
assert.equal(calls.at(-1).args.payload.context.filename, 'screen.js')
assert.match(calls.at(-1).args.payload.error.stack, /页面错误/)
// 空图片地址不能被解析成当前页面；有效资源保留原始地址与浏览器实际选择的地址。
function resourceError(tagName, attributes, properties) {
  handlers.get('error')({
    target: {
      tagName,
      getAttribute: (name) => attributes[name] ?? null,
      ...properties
    }
  })
  return calls.at(-1).args.payload
}
const emptyImage = resourceError(
  'IMG',
  { src: '' },
  {
    src: 'http://127.0.0.1:13134/index.html?panel=tool&tool=image-workbench',
    currentSrc: ''
  }
)
assert.match(emptyImage.message, /img，地址为空/)
assert.equal(emptyImage.context.filename, '')
assert.equal(emptyImage.context.resourceSource, '')
assert.equal(emptyImage.context.resourceUrl, '')
assert.equal(
  JSON.stringify(emptyImage).includes('index.html?panel=tool'),
  false
)
const failedImage = resourceError(
  'IMG',
  { src: '/preview.png' },
  {
    src: 'http://127.0.0.1:13134/preview.png',
    currentSrc: 'http://127.0.0.1:13134/preview.png'
  }
)
assert.equal(failedImage.context.resourceTag, 'img')
assert.equal(failedImage.context.resourceSource, '/preview.png')
assert.equal(failedImage.context.filename, failedImage.context.resourceUrl)
assert.equal(
  failedImage.context.resourceUrl,
  'http://127.0.0.1:13134/preview.png'
)
const responsiveImage = resourceError(
  'IMG',
  { src: '', srcset: '/preview@2x.png 2x' },
  {
    currentSrc: 'http://127.0.0.1:13134/preview@2x.png'
  }
)
assert.equal(responsiveImage.context.resourceSourceSet, '/preview@2x.png 2x')
assert.equal(
  responsiveImage.context.resourceUrl,
  'http://127.0.0.1:13134/preview@2x.png'
)
for (const [tag, attribute, property] of [
  ['SCRIPT', 'src', 'script.js'],
  ['LINK', 'href', 'style.css']
]) {
  const url = `http://127.0.0.1:13134/${property}`
  const resource = resourceError(
    tag,
    { [attribute]: `/${property}` },
    { [attribute]: url }
  )
  assert.equal(resource.context.resourceTag, tag.toLowerCase())
  assert.equal(resource.context.resourceSource, `/${property}`)
  assert.equal(resource.context.resourceUrl, url)
}
handlers.get('unhandledrejection')({ reason: new Error('异步错误') })
assert.equal(calls.at(-1).args.payload.kind, 'unhandled-rejection')
let previousCalled = false
const app = { config: { errorHandler: () => { previousCalled = true } } }
logger.installAppLogging(app)
app.config.errorHandler(new Error('渲染错误'), { $options: { __name: 'LogsView' } }, 'render')
assert.equal(calls.at(-1).args.payload.context.component, 'LogsView')
assert.equal(previousCalled, true)

// 编译真实日志组件，验证调用合并、全文搜索、分页、复制与导出。
const filename = 'src/features/logs/index.vue'
const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename })
const script = compileScript(descriptor, { id: 'logs-check', genDefaultAs: '__component' })
const template = compileTemplate({
  source: descriptor.template.content, filename, id: 'logs-check',
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
for (const style of descriptor.styles) {
  const result = await compileStyleAsync({
    source: style.content, filename, id: 'logs-check',
    scoped: style.scoped, preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}
const messages = []
const module = { exports: {} }
const code = transformSync(`${script.content}\n${template.code}\n__component.render = render\nexport default __component`, { format: 'cjs' }).code
new Function('require', 'module', 'exports', code)(
  name => {
    if (name === 'lucide-vue-next') return { Copy: {}, Download: {}, RefreshCw: {} }
    if (name === '@/utils/message') return {
      createMessage: { success: text => messages.push(text), error: text => messages.push(text) }
    }
    return require(name)
  }, module, module.exports
)
const component = module.exports.default
const props = vue.reactive({
  logs: [
    { id: 'finish', traceId: 'same-call', scope: 'backend', action: 'finish', status: 'success', service: 'provider', result: { model: 'vision-model', enabled: false } },
    { id: 'start', traceId: 'same-call', scope: 'backend', action: 'start', status: 'pending' },
    { id: 'failed', traceId: 'failed-call', scope: 'backend', action: 'finish', status: 'error', service: 'screenshot', payload: { model: 'ocr' }, error: { httpStatus: 401 } },
    { id: 'pending', traceId: 'pending-call', scope: 'backend', action: 'start', status: 'pending' }
  ],
  dates: ['2026-10-26'], selectedDate: '', hasMore: false, filePath: 'D:/data/workspace/logs'
})
const emitted = []
const scope = vue.effectScope()
const state = scope.run(() => component.setup(props, { expose() {}, emit: (...args) => emitted.push(args) }))
assert.equal(state.filteredLogs.value.length, 3)
state.keyword.value = 'vision-model'
assert.equal(state.filteredLogs.value[0].id, 'finish')
state.keyword.value = '401'
assert.equal(state.filteredLogs.value[0].id, 'failed')
state.statusFilter.value = 'success'
assert.equal(state.filteredLogs.value.length, 0)
state.keyword.value = ''
assert.equal(state.filteredLogs.value[0].id, 'finish')
state.statusFilter.value = 'all'
state.pageSize.value = 1
state.goPage(2)
assert.equal(state.pagedLogs.value[0].id, 'failed')
let clipboard = ''
Object.defineProperty(globalThis, 'navigator', {
  configurable: true,
  value: { clipboard: { writeText: async text => { clipboard = text } } }
})
await state.copyLog(props.logs[2])
assert.equal(JSON.parse(clipboard).error.httpStatus, 401)
assert.equal(messages.at(-1), '日志详情已复制')
let downloaded = null
let exportedBlob = null
globalThis.document.createElement = () => ({
  click() { downloaded = this.download }
})
URL.createObjectURL = blob => { exportedBlob = blob; return 'blob:log-export' }
URL.revokeObjectURL = () => {}
state.exportLogs()
assert.ok(downloaded.startsWith('app-debug-'))
assert.equal(JSON.parse(await exportedBlob.text()).logs.length, 3)
let vnode
const preview = vue.createRenderer({
  createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
  insert() {}, remove() {}, patchProp() {}
}).createApp({
  render() {
    vnode = component.render({}, [], props, vue.proxyRefs(state))
    return vue.h('div')
  }
})
preview.mount({})
function find(node, predicate) {
  if (!node || typeof node !== 'object') return []
  return [
    ...(predicate(node) ? [node] : []),
    ...(Array.isArray(node.children) ? node.children.flatMap(child => find(child, predicate)) : [])
  ]
}
const dateSelect = find(vnode, item => item.type === 'select')[0]
dateSelect.props.onChange({ target: { value: '2026-10-26' } })
assert.deepEqual(emitted.at(-1), ['refresh', '2026-10-26'])
scope.stop()
preview.unmount()
console.log('日志前端验证通过：请求关联、二进制接口失败、异常捕获、防递归、调用合并、日期筛选、搜索、复制、导出与样式编译')

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { parse, compileScript, compileTemplate, compileStyleAsync } from '@vue/compiler-sfc'
import { transformSync } from 'esbuild'

// 使用真实 Vue 生命周期和渲染，仅模拟原生 IPC、选区和剪贴板。
const require = createRequire(import.meta.url)
const vue = require('vue')
const filename = 'src/components/SelectionTranslator.vue'
const { descriptor, errors } = parse(readFileSync(filename, 'utf8'), { filename })
assert.deepEqual(errors, [])
const script = compileScript(descriptor, { id: 'translation', genDefaultAs: '__component' })
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: 'translation',
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
for (const style of descriptor.styles) {
  const result = await compileStyleAsync({
    source: style.content, filename, id: 'translation', scoped: true, preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}

const calls = []
const pending = []
let selected = null
let copied = ''
let monacoLoads = 0
const listeners = new Map()
globalThis.document = {
  body: { style: {} },
  addEventListener: (name, handler) => listeners.set(name, handler),
  removeEventListener: name => listeners.delete(name)
}
globalThis.window = {
  innerWidth: 1000, innerHeight: 800,
  addEventListener() {}, removeEventListener() {},
  getSelection: () => ({ toString: () => '应用内选中文字' })
}
globalThis.Element = class { closest() { return null } }
globalThis.HTMLInputElement = class extends Element {}
globalThis.HTMLTextAreaElement = class extends Element {}
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {
  clipboard: { writeText: async text => { copied = text } }
} })
const api = {
  onDesktopSelection: async callback => {
    selected = callback
    return () => { selected = null }
  },
  desktopReady: async () => {
    assert.equal(typeof selected, 'function', '先监听，再获取首条选区')
    calls.push('ready')
    return { text: 'Hello', error: '', targetLanguage: '简体中文' }
  },
  translateText: payload => {
    calls.push(payload.text)
    return new Promise(resolve => pending.push(resolve))
  },
  desktopClose: async () => calls.push('close'),
  desktopDrag: async () => calls.push('drag')
}
const module = { exports: {} }
new Function('require', 'module', 'exports', 'loadMonaco', transformSync(
  `${script.content}\n${template.code}\n__component.render = render\nexport default __component`
    .replace('import("monaco-editor/esm/vs/editor/editor.api")', 'loadMonaco()'),
  { format: 'cjs' }
).code)(name => {
  if (name === '@/api') return { translationApi: api }
  if (name === 'lucide-vue-next') {
    const icon = { render: () => vue.h('span') }
    return { Copy: icon, Languages: icon, X: icon }
  }
  return require(name)
}, module, module.exports, async () => {
  monacoLoads += 1
  return { editor: { getEditors: () => [] } }
})

function node(type, text = '') {
  return { type, text, children: [], parent: null, props: {}, getBoundingClientRect: () => ({ left: 0, top: 0, width: 440, height: 460 }) }
}
const body = node('body')
const renderer = vue.createRenderer({
  createElement: node,
  createText: text => node('#text', text),
  createComment: text => node('#comment', text),
  setText: (item, text) => { item.text = text },
  setElementText: (item, text) => { item.children = []; item.text = text },
  parentNode: item => item.parent,
  nextSibling: item => item.parent?.children[item.parent.children.indexOf(item) + 1] || null,
  querySelector: () => body,
  patchProp: (item, key, previous, value) => { item.props[key] = value },
  insert(item, parent, anchor) {
    if (item.parent) item.parent.children.splice(item.parent.children.indexOf(item), 1)
    item.parent = parent
    const index = anchor ? parent.children.indexOf(anchor) : -1
    if (index < 0) parent.children.push(item)
    else parent.children.splice(index, 0, item)
  },
  remove(item) {
    if (item.parent) item.parent.children.splice(item.parent.children.indexOf(item), 1)
  }
})
function mount(props) {
  const root = node('root')
  const vnode = vue.h(module.exports.default, props)
  renderer.render(vnode, root)
  return { state: vnode.component.setupState, unmount: () => renderer.render(null, root) }
}
async function tick() {
  for (let index = 0; index < 8; index += 1) { await Promise.resolve(); await vue.nextTick() }
}

const desktop = mount({ desktop: true })
await tick()
assert.deepEqual(calls, ['ready', 'Hello'])
assert.equal(monacoLoads, 0, '桌面翻译不能加载编辑器')
assert.equal(desktop.state.visible, true)
assert.equal(listeners.has('contextmenu'), false, '桌面窗口不安装主界面菜单监听')
desktop.state.startDrag({ button: 0 })
await tick()
assert.equal(calls.at(-1), 'drag')
selected({ text: 'Second selection', error: '', targetLanguage: 'English' })
await tick()
pending[1]({ translatedText: '第二条译文' })
await tick()
pending[0]({ translatedText: '旧译文' })
await tick()
assert.equal(desktop.state.result.translatedText, '第二条译文', '过期响应不能覆盖新选区')
assert.equal(desktop.state.settings.targetLanguage, 'English')
await desktop.state.copyResult()
assert.equal(copied, '第二条译文')
const beforeError = pending.length
selected({ text: '', error: '密码输入框不支持划词翻译' })
await tick()
assert.equal(pending.length, beforeError, '取词失败不能向模型发送空文本')
assert.match(desktop.state.errorMessage, /密码/)
listeners.get('keydown')({ key: 'Escape' })
await tick()
assert.equal(calls.at(-1), 'close')
desktop.unmount()
assert.equal(selected, null)
assert.equal(listeners.has('keydown'), false)

const local = mount({ settings: { enabled: true, targetLanguage: '简体中文' } })
await tick()
let prevented = false
listeners.get('contextmenu')({ target: new Element(), clientX: 999, clientY: 799,
  preventDefault() { prevented = true }, stopPropagation() {} })
assert.equal(prevented, true)
assert.equal(local.state.menu.text, '应用内选中文字')
assert.equal(local.state.menu.visible, true)
local.unmount()
assert.equal(listeners.has('contextmenu'), false)
console.log('翻译组件验证通过：首条取词、独立窗口、原生拖动、竞态、错误、复制、销毁和应用内菜单')

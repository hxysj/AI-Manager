// 离线挂载实际工作台状态逻辑，所有账号、文件和生图接口都由内存替身提供。
import assert from 'node:assert/strict'
import { readFile, writeFile, unlink } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { parse, compileScript, compileTemplate, compileStyleAsync } from '@vue/compiler-sfc'
import { build } from 'esbuild'
import { createRenderer, nextTick } from 'vue'

const storage = new Map()
globalThis.localStorage = { getItem: key => storage.get(key), setItem: (key, value) => storage.set(key, value) }
globalThis.window = { setInterval: () => 1, clearInterval: () => {} }
globalThis.document = { hidden: false }
const calls = []
let tasks = []
let history = []
globalThis.__imageTest = {
  systemApi: { saveFile: async () => '' },
  toolboxApi: {
    imageAccounts: async () => [{ id: 'account', active: true }],
    imageModels: async () => ({ data: [{ id: 'gpt-image-2' }, { id: 'gpt-5-5' }] }),
    imageQuota: async () => ({ accountId: 'account', remaining: 3 }),
    listImageTasks: async payload => { calls.push(['list', payload]); return { items: tasks, total: tasks.length ? 1 : 0 } },
    imageHistory: async () => history,
    imageTaskInputs: async () => ({ images: ['data:image/png;base64,reference'], mask: 'data:image/png;base64,mask' }),
    submitImageTask: async payload => { calls.push(['submit', structuredClone(payload)]); return { roundId: payload.roundId, items: [] } },
    resumeImageTask: async payload => { calls.push(['resume', payload]); return {} },
    clearImageResults: async payload => { calls.push(['clear', payload]); return {} },
    deleteImageTasks: async payload => { calls.push(['delete', payload]); tasks = []; history = []; return {} }
  },
  createMessage: { success: () => {}, warning: () => {}, error: message => { throw new Error(message) } }
}

const temporary = resolve(`scripts/.image-workbench-test-${process.pid}.mjs`)
const sourcePath = resolve('src/features/tools/components/ImageWorkbench.vue')
try {
  for (const path of [sourcePath, resolve('src/features/tools/components/ImageDrawingDialog.vue')]) {
    const { descriptor, errors } = parse(await readFile(path, 'utf8'))
    assert.deepEqual(errors, [])
    const script = compileScript(descriptor, { id: path })
    const template = compileTemplate({ source: descriptor.template.content, filename: path, id: path, compilerOptions: { bindingMetadata: script.bindings } })
    const style = await compileStyleAsync({ source: descriptor.styles[0].content, filename: path, id: path, preprocessLang: 'less' })
    assert.deepEqual(template.errors, [])
    assert.deepEqual(style.errors, [])
  }
  const bundle = await build({
    entryPoints: [sourcePath], bundle: true, write: false, format: 'esm', platform: 'node', external: ['vue'],
    plugins: [{ name: 'offline-image-workbench', setup(builder) {
      builder.onResolve({ filter: /^@\/api$|^@\/utils\/message$/ }, args => ({ path: args.path, namespace: 'mock' }))
      builder.onLoad({ filter: /.*/, namespace: 'mock' }, () => ({ contents: 'export const {systemApi, toolboxApi, createMessage} = globalThis.__imageTest' }))
      builder.onResolve({ filter: /^@\/api\/request$/ }, () => ({ path: 'events', namespace: 'events' }))
      builder.onLoad({ filter: /.*/, namespace: 'events' }, () => ({ contents: 'export const subscribe = () => () => {}' }))
      builder.onResolve({ filter: /^element-plus$|^lucide-vue-next$|\.css$|\.vue$/ }, args => {
        if (args.kind === 'entry-point') return
        return { path: args.path, namespace: 'ui' }
      })
      builder.onLoad({ filter: /.*/, namespace: 'ui' }, () => ({ contents: 'export default {}; export const ElImage = {}; export const ElMessageBox = {confirm: async () => {}, prompt: async () => ({value: "新名称"})}; export const Download={}, BookOpen={}, ImageOff={}, ImagePlus={}, LoaderCircle={}, Paintbrush={}, Plus={}, RefreshCw={}, Sparkles={}, Trash2={}, X={}' }))
      builder.onResolve({ filter: /^@\// }, args => ({ path: resolve('src', args.path.slice(2) + '.js') }))
      builder.onLoad({ filter: /ImageWorkbench\.vue$/ }, async args => ({ contents: compileScript(parse(await readFile(args.path, 'utf8')).descriptor, { id: 'workbench-test' }).content, resolveDir: resolve('src/features/tools/components') }))
    } }]
  })
  await writeFile(temporary, bundle.outputFiles[0].contents)
  const component = (await import(pathToFileURL(temporary))).default
  component.render = () => null
  const renderer = createRenderer({ createComment: () => ({}), insert: () => {}, remove: () => {}, parentNode: () => null, nextSibling: () => null })
  const app = renderer.createApp(component)
  const instance = app.mount({})
  const state = instance.$.setupState
  const settle = async () => { for (let i = 0; i < 12; i++) await nextTick() }
  await settle()
  assert.equal(state.form.generationMode, 'web')
  assert.equal(state.form.quality, 'auto')
  assert.equal(state.form.model, 'gpt-image-2')
  assert.equal(state.form.accountId, 'account')
  state.form.model = 'gpt-image-2.5-flare'
  await settle()
  assert.equal(state.qualityOptions.length, 6)
  state.form.quality = 'max'
  state.form.model = 'gpt-image-2'
  await settle()
  assert.equal(state.form.quality, 'auto')
  state.applySizePreset(state.sizePresets.find(item => item.label === '16:9'))
  state.width = 1900
  await settle()
  assert.equal(state.ratio, '16:9')
  assert.equal(state.height, 1088)
  state.form.prompt = '测试画面'
  state.form.n = 100
  assert.ok(state.canSubmit)
  state.form.n = 101
  assert.equal(state.canSubmit, false)
  state.form.n = 100
  const before = calls.filter(([name]) => name === 'submit').length
  state.promptKeydown({ key: 'Enter', isComposing: true })
  state.promptKeydown({ key: 'Enter', shiftKey: true })
  assert.equal(calls.filter(([name]) => name === 'submit').length, before)
  await state.submitTask()
  const submitted = calls.find(([name]) => name === 'submit')[1]
  assert.equal(submitted.n, 100)
  assert.equal(submitted.size, '1900x1088')
  assert.equal(submitted.generationMode, 'web')
  assert.ok(submitted.conversationId && submitted.roundId)
  assert.equal(JSON.parse(storage.get('image-workbench-preferences')).n, 100)
  state.applyDrawing({ name: 'sketch.png', url: 'data:image/png;base64,sketch', source: '' })
  assert.equal(state.references.length, 1)
  assert.equal(state.form.mode, 'edit')
  state.applyDrawing({ name: 'edit-mask.png', url: 'data:image/png;base64,mask', source: 'data:image/png;base64,result' })
  assert.equal(state.references[0].url, 'data:image/png;base64,result')
  assert.equal(state.mask.name, 'edit-mask.png')
  state.removeReference(0)
  assert.equal(state.mask, null)
  assert.equal(state.form.mode, 'generate')
  const task = { id: 'original', batchCount: 4, status: 'failed', createdAt: Date.now(), request: { ...submitted, n: 1, generationMode: 'codex', conversationId: 'original-conversation', roundId: 'original-round', size: '1024x1536', mode: 'edit' } }
  await state.reuseTask(task, true)
  await settle()
  assert.equal(state.form.generationMode, 'codex')
  assert.equal(state.form.n, 4)
  assert.equal(state.height, 1536)
  assert.equal(state.references.length, 1)
  assert.ok(state.mask)
  assert.ok(!('conversationId' in state.form))
  state.form.generationMode = 'web'
  await state.regenerateTask(task)
  const retried = calls.filter(([name]) => name === 'submit').at(-1)[1]
  assert.equal(retried.generationMode, 'codex')
  assert.equal(retried.n, 1)
  assert.equal(retried.conversationId, 'original-conversation')
  assert.deepEqual([...state.roundState['original-round'].replaced], ['original'])
  await state.regenerateRound({ id: 'original-round', tasks: [task] })
  const regenerated = calls.filter(([name]) => name === 'submit').at(-1)[1]
  assert.notEqual(regenerated.roundId, 'original-round')
  assert.equal(regenerated.n, 4)
  const submitCount = calls.filter(([name]) => name === 'submit').length
  await state.resumeTask(task)
  assert.equal(calls.filter(([name]) => name === 'submit').length, submitCount)
  assert.deepEqual(calls.filter(([name]) => name === 'resume').at(-1)[1], { id: 'original' })
  const previousId = state.activeConversationId
  state.newConversation()
  await settle()
  assert.notEqual(state.activeConversationId, previousId)
  assert.equal(state.form.prompt, '')
  assert.equal(state.references.length, 0)
  await state.renameConversation(state.currentConversation)
  assert.equal(state.currentConversation.title, '新名称')
  assert.ok(!storage.get('image-workbench-conversations').includes('data:image'))
  assert.ok(calls.some(([name, payload]) => name === 'list' && payload.groupByRound))
  tasks = [task]
  history = [{ id: task.id, status: 'failed', createdAt: task.createdAt, conversationId: task.request.conversationId, roundId: task.request.roundId, prompt: task.request.prompt }]
  await state.loadHistory()
  state.ignoreTaskError({ id: 'original-round' }, task)
  assert.deepEqual([...state.roundState['original-round'].ignored], ['original'])
  await state.removeRoundPrompt({ id: 'original-round' })
  assert.equal(state.roundState['original-round'].hidePrompt, true)
  await state.removeRoundResults({ id: 'original-round' })
  assert.deepEqual(calls.filter(([name]) => name === 'clear').at(-1)[1].ids, ['original'])
  await state.deleteConversation('original-conversation')
  assert.deepEqual(calls.filter(([name]) => name === 'delete').at(-1)[1].ids, ['original'])
  assert.ok(!state.conversations.some(item => item.id === 'original-conversation'))
  const lastConversation = state.activeConversationId
  app.unmount()
  const restoredApp = renderer.createApp(component)
  const restored = restoredApp.mount({}).$.setupState
  await settle()
  assert.equal(restored.activeConversationId, lastConversation)
  assert.equal(restored.height, 1536)
  assert.equal(restored.form.n, 4)
  restoredApp.unmount()
  console.log('生图工作台离线检查通过：设置、100 张提交、输入法、草图/蒙版回填、复用、重试、继续等待与会话持久化。')
} finally {
  await unlink(temporary).catch(() => {})
}

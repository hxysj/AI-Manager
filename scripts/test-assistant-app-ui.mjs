import assert from "node:assert/strict"
import { execFile } from "node:child_process"
import { existsSync } from "node:fs"
import { mkdtemp, realpath, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { pathToFileURL } from "node:url"
import { promisify } from "node:util"
import { build } from "esbuild"

// 使用独立无头浏览器验证真实 DOM 和 Vue v-model，不连接用户现有浏览器。
const executable = [
  process.env.ASSISTANT_TEST_BROWSER,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe"
].find((path) => path && existsSync(path))
assert.ok(
  executable,
  "请通过 ASSISTANT_TEST_BROWSER 指定用于界面验证的 Chromium 浏览器"
)
const root = await mkdtemp(join(tmpdir(), "assistant-app-ui-"))
const fixture = `
import { createApp, h, reactive, nextTick } from 'vue'
import { createAppUiController, installAssistantUiControl } from './src/features/assistant/ui-control.js'

const claims = new Set()
const replies = []
let callback
let unsubscribeCount = 0
let pending = []
globalThis.__assistantUiTest = {
  onUiRequest: async handle => { callback = handle; return () => { unsubscribeCount++ } },
  uiPending: async () => pending,
  uiClaim: async request => {
    if (request.runId === 'cancelled' || claims.has(request.id)) return false
    claims.add(request.id)
    return true
  },
  uiResult: async result => { replies.push(result) }
}
const assert = (condition, message) => { if (!condition) throw new Error(message) }
const rejects = async (run, pattern) => {
  try { await run() } catch (error) { assert(pattern.test(error.message), error.message); return }
  throw new Error('预期操作失败却成功了')
}
const flush = async () => { for (let i = 0; i < 12; i++) { await Promise.resolve(); await nextTick() } }
const model = reactive({ prompt: '', generated: false, mode: 'web', menu: false, choice: '' })
const app = createApp({
  setup: () => () => h('section', [
    h('h1', '图片工作台'),
    h('label', ['提示词', h('textarea', {
      'aria-label': '图片提示词', value: model.prompt, maxlength: 2000,
      onInput: event => { model.prompt = event.target.value }
    })]),
    h('button', { disabled: !model.prompt, onClick: () => { model.generated = true } }, '开始生成'),
    h('p', { role: 'status' }, model.generated ? '任务已提交' : '等待提示词'),
    h('label', ['生成模式', h('select', {
      value: model.mode, onChange: event => { model.mode = event.target.value }
    }, [h('option', { value: 'web' }, 'Web'), h('option', { value: 'codex' }, 'Codex')])]),
    h('button', { onClick: () => { model.menu = !model.menu } }, '选择模型'),
    ...(model.menu ? [h('div', {
      role: 'option', tabindex: 0,
      onClick: () => { model.choice = 'image-model'; model.menu = false }
    }, '图片模型')] : []),
    h('input', { 'aria-label': '只读说明', value: '原值', readonly: true }),
    h('input', { 'aria-label': '受限输入', maxlength: 3 }),
    h('input', { 'aria-label': '导入图片', type: 'file' }),
    h('input', { 'aria-label': '登录密码', type: 'password', value: 'private-password' }),
    h('input', { 'aria-label': 'API Key', value: 'private-api-key' }),
    h('button', { style: { display: 'none' } }, '隐藏按钮')
  ])
})
app.mount('#app')
const controller = createAppUiController()
const find = (snapshot, name) => snapshot.nodes.find(node => node.name === name)
const action = (snapshot, name, kind, extra = {}) => {
  const node = find(snapshot, name)
  assert(node, '未找到控件：' + name + '，当前控件：' + snapshot.nodes.map(item => item.name).join('、'))
  return controller.interact({
    snapshotId: snapshot.snapshotId, nodeId: node.nodeId, action: kind, ...extra
  })
}

async function run() {
  let snapshot = controller.inspect()
  assert(snapshot.text.includes('图片工作台'), '应读取工作台可见文字')
  assert(!JSON.stringify(snapshot).includes('private-'), '密码和密钥不能进入模型上下文')
  assert(!find(snapshot, '隐藏按钮'), '隐藏控件不能被操作')
  assert(find(snapshot, '开始生成').disabled, '未填写提示词时生成按钮应停用')
  await rejects(() => action(snapshot, '开始生成', 'click'), /不可用/)
  const stale = snapshot
  snapshot = await action(snapshot, '图片提示词', 'focus')
  assert(document.activeElement.tagName === 'TEXTAREA', '应真实聚焦文本框')
  await rejects(() => action(stale, '图片提示词', 'fill', { text: '旧快照' }), /已变化/)
  snapshot = await action(snapshot, '图片提示词', 'fill', { text: '一张游乐园照片' })
  assert(model.prompt === '一张游乐园照片', '输入事件必须更新真实 Vue 状态')
  assert(!find(snapshot, '开始生成').disabled, '填写提示词后应允许生成')
  snapshot = await action(snapshot, '开始生成', 'click')
  assert(model.generated && snapshot.text.includes('任务已提交'), '点击必须触发工作台原有提交处理')
  snapshot = await action(snapshot, '生成模式', 'select', { text: 'codex' })
  assert(model.mode === 'codex', '选择必须更新真实 Vue 状态')
  await rejects(() => action(snapshot, '生成模式', 'select', { text: 'missing' }), /选项不存在/)
  snapshot = await action(snapshot, '选择模型', 'click')
  snapshot = await action(snapshot, '图片模型', 'click')
  assert(model.choice === 'image-model' && !model.menu, '应支持展开后读取并点击自定义选项')
  await rejects(() => action(snapshot, '只读说明', 'fill', { text: '改写' }), /只读/)
  await rejects(() => action(snapshot, '受限输入', 'fill', { text: '过长的内容' }), /字符限制/)
  await rejects(() => action(snapshot, '导入图片', 'click'), /系统对话框/)
  document.querySelector('textarea').value = '用户刚修改'
  await rejects(() => action(snapshot, '图片提示词', 'fill', { text: '覆盖' }), /用户修改/)
  snapshot = controller.inspect({ limit: 2 })
  assert(snapshot.nodes.length === 2 && snapshot.hasMore, '控件列表应支持分页')
  const removed = controller.inspect()
  const removedNode = find(removed, '开始生成')
  document.querySelector('button').remove()
  await rejects(() => controller.interact({
    snapshotId: removed.snapshotId, nodeId: removedNode.nodeId, action: 'click'
  }), /已变化/)
  model.generated = false
  await nextTick()

  pending = [{ id: 'pending', runId: 'run', windowLabel: 'main', operation: 'read', args: {} }]
  installAssistantUiControl(app)
  await flush()
  assert(replies.length === 1 && replies[0].result.snapshotId, '应补领页面加载期间的读取请求')
  await callback(pending[0])
  assert(replies.length === 1, '事件和待处理快照不能重复执行')
  await callback({ ...pending[0], id: 'other', windowLabel: 'tool-window-image-workbench' })
  assert(replies.length === 1, '不能响应其他窗口的请求')
  const state = replies[0].result
  await callback({
    id: 'cancelled', runId: 'cancelled', windowLabel: 'main', operation: 'interact',
    args: { snapshotId: state.snapshotId, nodeId: find(state, '图片提示词').nodeId, action: 'fill', text: '不应执行' }
  })
  assert(model.prompt === '一张游乐园照片', '已经取消的任务不能操作界面')
  app.unmount()
  await callback({ ...pending[0], id: 'unmounted' })
  assert(replies.length === 1 && unsubscribeCount === 1, '卸载后应清理监听并停止响应')
}
run().then(() => {
  document.getElementById('assistant-ui-result').textContent = JSON.stringify({ ok: true })
}).catch(error => {
  document.getElementById('assistant-ui-result').textContent = JSON.stringify({ error: error.message, stack: error.stack })
})
`

try {
  await build({
    stdin: { contents: fixture, resolveDir: process.cwd(), loader: "js" },
    bundle: true,
    format: "iife",
    outfile: join(root, "fixture.js"),
    plugins: [
      {
        name: "assistant-ui-host",
        setup(builder) {
          builder.onResolve({ filter: /^@\/|^@tauri-apps\/api\// }, (args) => {
            if (
              [
                "@/api/modules/assistant",
                "@/utils/app-logger",
                "@tauri-apps/api/core",
                "@tauri-apps/api/window"
              ].includes(args.path)
            ) {
              return { path: args.path, namespace: "assistant-ui-host" }
            }
            return { path: resolve(args.path.replace("@/", "src/") + ".js") }
          })
          builder.onLoad(
            { filter: /.*/, namespace: "assistant-ui-host" },
            (args) => ({
              contents:
                args.path === "@/api/modules/assistant"
                  ? "export const assistantApi = new Proxy({}, { get: (_, name) => (...args) => globalThis.__assistantUiTest[name](...args) })"
                  : args.path === "@/utils/app-logger"
                    ? "export const reportAppError = (...args) => { throw new Error(JSON.stringify(args)) }"
                    : args.path === "@tauri-apps/api/core"
                      ? "export const isTauri = () => true"
                      : 'export const getCurrentWindow = () => ({ label: "main" })',
              loader: "js"
            })
          )
        }
      }
    ]
  })
  const html = join(root, "index.html")
  await writeFile(
    html,
    '<!doctype html><html><head><meta charset="utf-8"><title>助手界面测试</title></head><body><div id="app"></div><output id="assistant-ui-result"></output><script src="fixture.js"></script></body></html>'
  )
  const { stdout } = await promisify(execFile)(
    executable,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      `--user-data-dir=${join(root, "profile")}`,
      "--dump-dom",
      "--virtual-time-budget=3000",
      pathToFileURL(html).href
    ],
    { windowsHide: true, timeout: 60000, maxBuffer: 2 * 1024 * 1024 }
  )
  const result = stdout.match(
    /<output id="assistant-ui-result">([^<]+)<\/output>/
  )?.[1]
  assert.ok(result, "浏览器未返回界面验证结果")
  assert.deepEqual(
    JSON.parse(result.replaceAll("&quot;", '"').replaceAll("&amp;", "&")),
    { ok: true }
  )
  console.log(
    "助手应用界面验证通过：真实 Vue 输入、聚焦、提交、下拉选项、敏感字段过滤、快照失效、取消和监听清理"
  )
} finally {
  const resolved = await realpath(root)
  assert.equal(
    dirname(resolved).toLowerCase(),
    (await realpath(tmpdir())).toLowerCase()
  )
  await rm(resolved, {
    recursive: true,
    force: true,
    maxRetries: 10,
    retryDelay: 200
  })
}

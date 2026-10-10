import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { parse, compileScript, compileTemplate, compileStyleAsync } from '@vue/compiler-sfc'
import { transformSync } from 'esbuild'

// 使用 Vue 原生渲染器运行交互，只替换 IPC 和宿主节点，不操作真实仓库。
const require = createRequire(import.meta.url)
const vue = require('vue')
const calls = []
const messages = []
let handlers = {}
const api = new Proxy({}, {
  get: (_, name) => async payload => {
    calls.push({ name, payload })
    assert.ok(handlers[name], `缺少接口模拟：${name}`)
    return handlers[name](payload)
  }
})
const modules = new Map()
const styles = []

function loadComponent(path) {
  path = resolve(path)
  if (modules.has(path)) return modules.get(path)
  const { descriptor, errors } = parse(readFileSync(path, 'utf8'), { filename: path })
  assert.deepEqual(errors, [])
  const script = compileScript(descriptor, { id: path, genDefaultAs: '__component' })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename: path,
    id: path,
    compilerOptions: { bindingMetadata: script.bindings, hoistStatic: false }
  })
  assert.deepEqual(template.errors, [])
  for (const style of descriptor.styles) styles.push({ path, style })
  const code = transformSync(`${script.content}\n${template.code}\n__component.render = render\nexport default __component`, { format: 'cjs' }).code
  const module = { exports: {} }
  const localRequire = name => {
    if (name === '@/api') return { gitToolApi: api, systemApi: api }
    if (name === '@/utils/message') return { createMessage: {
      success: text => messages.push({ success: true, text }),
      error: text => messages.push({ success: false, text })
    } }
    if (name === '@/utils/formatters') return { formatDateTime: value => value }
    if (name.endsWith('.vue')) return { default: loadComponent(name.replace('@/', 'src/')), __esModule: true }
    if (name === '@/features/gitTool/constant') {
      const constant = { exports: {} }
      const code = transformSync(readFileSync(resolve('src/features/gitTool/constant.js'), 'utf8'), { format: 'cjs' }).code
      new Function('module', 'exports', code)(constant, constant.exports)
      return constant.exports
    }
    return require(name)
  }
  new Function('require', 'module', 'exports', code)(localRequire, module, module.exports)
  modules.set(path, module.exports.default)
  return module.exports.default
}

globalThis.window = { setTimeout, clearTimeout, addEventListener() {}, removeEventListener() {} }
globalThis.Document = class { activeElement = null }
globalThis.document = new Document()
// 模拟滚动容器尺寸变化，验证大 diff 在窗口缩放后仍按可见区域渲染。
globalThis.ResizeObserver = class {
  static instances = []
  constructor(callback) { this.callback = callback; ResizeObserver.instances.push(this) }
  observe(element) { this.element = element; this.callback() }
  disconnect() { this.disconnected = true }
}

function hostNode(type, text = '') {
  return {
    type, tagName: type.toUpperCase(), text, children: [], parent: null, props: {}, listeners: {}, value: '', clientHeight: 400,
    addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler) },
    removeEventListener() {},
    setAttribute(name, value) { this.props[name] = value },
    removeAttribute(name) { delete this.props[name] },
    getRootNode() { return document },
    focus() {},
    get options() { return this.children.filter(node => node.type === 'option') }
  }
}

const renderer = vue.createRenderer({
  createElement: type => hostNode(type),
  createText: text => hostNode('#text', text),
  createComment: text => hostNode('#comment', text),
  setText: (node, text) => { node.text = text },
  setElementText: (node, text) => { node.children = []; node.text = text },
  parentNode: node => node.parent,
  nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] || null,
  patchProp: (node, name, previous, value) => { node.props[name] = value; if (name === 'value') node.value = value },
  insert(node, parent, anchor) {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = parent
    const index = anchor ? parent.children.indexOf(anchor) : -1
    if (index < 0) parent.children.push(node)
    else parent.children.splice(index, 0, node)
  },
  remove(node) {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = null
  }
})

function mount(component, props, slots) {
  const root = hostNode('root')
  const vnode = vue.h(component, props, slots)
  const warnings = []
  const errors = []
  vnode.appContext = { ...vue.createApp({})._context }
  vnode.appContext.config.warnHandler = message => warnings.push(message)
  vnode.appContext.config.errorHandler = error => { errors.push(error) }
  renderer.render(vnode, root)
  return { root, component: vnode.component, state: vnode.component.setupState, warnings, errors, unmount: () => renderer.render(null, root) }
}

function findNodes(root, className) {
  const result = []
  function visit(node) {
    if (String(node.props.class || '').split(' ').includes(className)) result.push(node)
    node.children.forEach(visit)
  }
  visit(root)
  return result
}

function findComponent(vnode, component) {
  if (vnode.component?.type === component) return vnode.component
  const children = vnode.component ? [vnode.component.subTree] : Array.isArray(vnode.children) ? vnode.children : []
  for (const child of children) {
    const found = findComponent(child, component)
    if (found) return found
  }
  return null
}

const tick = async () => { await Promise.resolve(); await vue.nextTick(); await Promise.resolve(); await vue.nextTick() }
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
const clone = value => structuredClone(value)
const status = {
  branch: 'main', headHash: 'a'.repeat(40), hasHead: true, detached: false,
  upstream: 'origin/main', ahead: 1, behind: 2, projectPath: 'D:/test', operation: '',
  remotes: [{ name: 'origin', fetchUrl: 'local.git', pushUrl: 'local.git' }],
  files: [{ path: 'both.txt', oldPath: '', indexStatus: 'M', worktreeStatus: 'M', staged: true, unstaged: true, conflicted: false, untracked: false }]
}
const branches = [{ name: 'main', isCurrent: true }, { name: 'feature', isCurrent: false }]
const Workspace = loadComponent('src/features/gitTool/components/GitWorkspacePanel.vue')
const Modal = loadComponent('src/features/gitTool/components/GitOperationModal.vue')
const Page = loadComponent('src/features/gitTool/index.vue')

handlers.getGitToolWorktreeDiff = payload => ({ selectedFilePath: payload.filePath, patch: payload.staged ? '+index' : '+worktree' })
let success = false
const workspace = mount(Workspace, { repoId: 'repo', status: clone(status), busy: false, execute: async () => ({ success }) })
await tick()
assert.equal(findNodes(workspace.root, 'git-workspace-file').length, 2)
assert.equal(workspace.state.detail.patch, '+worktree')
await workspace.state.selectFile('both.txt', 'staged')
assert.equal(workspace.state.detail.patch, '+index')
workspace.state.message = 'feat: 保留提交草稿'
await workspace.state.submitCommit()
assert.equal(workspace.state.message, 'feat: 保留提交草稿')
success = true
await workspace.state.submitCommit()
assert.equal(workspace.state.message, '')
assert.deepEqual(workspace.warnings, [])
assert.deepEqual(workspace.errors, [])
workspace.unmount()

let resolveOldDiff
handlers.getGitToolWorktreeDiff = payload => payload.staged
  ? { selectedFilePath: payload.filePath, patch: '+latest' }
  : new Promise(resolve => { resolveOldDiff = resolve })
const racingWorkspace = mount(Workspace, { repoId: 'repo', status: clone(status), execute: async () => null })
await racingWorkspace.state.selectFile('both.txt', 'staged')
resolveOldDiff({ selectedFilePath: 'both.txt', patch: '+stale' })
await tick()
assert.equal(racingWorkspace.state.detail.patch, '+latest')
racingWorkspace.unmount()

const preview = { hash: 'b'.repeat(40), subject: '目标提交', author: '测试', date: '2026-10-09', parents: ['parent1', 'parent2'], changedFiles: ['both.txt'] }
handlers.previewGitToolRevision = () => clone(preview)
let submitted
const reset = mount(Modal, { action: 'reset', repoId: 'repo', status: clone(status), branches, execute: async payload => { submitted = payload; return { success: true } } })
await delay(280)
await tick()
assert.equal(reset.state.canSubmit, true)
reset.state.form.resetMode = 'hard'
await tick()
assert.equal(reset.state.canSubmit, false)
reset.state.form.confirmedDiscard = true
await reset.state.submit()
assert.equal(submitted.revision, preview.hash)
assert.equal(submitted.expectedBranch, 'main')
assert.equal(submitted.expectedHead, status.headHash)
assert.equal(submitted.confirmedDiscard, true)
reset.state.form.revision = 'new-target'
await tick()
assert.equal(reset.state.form.confirmedDiscard, false)
assert.equal(reset.state.canSubmit, false)
assert.deepEqual(reset.warnings, [])
assert.deepEqual(reset.errors, [])
reset.unmount()

let resolveOldPreview
handlers.previewGitToolRevision = payload => payload.revision === 'old'
  ? new Promise(resolve => { resolveOldPreview = resolve })
  : clone(preview)
const pick = mount(Modal, { action: 'cherry-pick', repoId: 'repo', status: clone(status), branches, initial: { revision: 'old' }, execute: async payload => { submitted = payload; return { success: true } } })
await delay(280)
pick.state.form.revision = 'new'
await tick()
await delay(280)
await tick()
resolveOldPreview({ ...preview, hash: 'stale' })
await tick()
assert.equal(pick.state.preview.hash, preview.hash)
assert.equal(findNodes(pick.root, 'git-operation-field').length, 2)
pick.state.form.mainline = 2
await pick.state.submit()
assert.equal(submitted.mainline, 2)
pick.unmount()

const ComparePanel = loadComponent('src/features/gitTool/components/GitBranchComparePanel.vue')
handlers.getGitToolBranchDiff = payload => ({
  baseBranch: payload.baseBranch,
  targetBranch: payload.targetBranch,
  command: `git diff --stat ${payload.baseBranch}..${payload.targetBranch}`,
  files: [{ path: 'src/app.js', status: 'M', additions: 2, deletions: 1, binary: false }],
  fileCount: 1,
  additions: 2,
  deletions: 1,
  changedLines: 3,
  patch: 'diff --git a/src/app.js b/src/app.js\n+added\n-removed'
})
const compare = mount(ComparePanel, {
  repoId: 'repo',
  branches,
  currentBranch: 'main',
  selectedBranch: 'feature'
})
await tick()
assert.equal(findNodes(compare.root, 'git-branch-compare-stat-total').length, 1)
assert.equal(findNodes(compare.root, 'git-branch-compare-line-add').length, 1)
assert.equal(findNodes(compare.root, 'git-branch-compare-file').length, 1)
const smallDiff = handlers.getGitToolBranchDiff
handlers.getGitToolBranchDiff = payload => ({
  ...smallDiff(payload),
  patch: Array.from({ length: 610659 }, (_, index) => `+line ${index}`).join('\n')
})
await compare.state.compareBranches()
await tick()
assert.ok(findNodes(compare.root, 'git-branch-compare-line').length < 100)
const diffViewport = findNodes(compare.root, 'git-branch-compare-diff')[0]
const diffObserver = ResizeObserver.instances.findLast(observer => vue.toRaw(observer.element) === diffViewport)
diffViewport.props.onScroll({ target: { scrollTop: 4000000 } })
await tick()
assert.equal(findNodes(compare.root, 'git-branch-compare-line')[0].text, '+line 199980')
const previousRows = findNodes(compare.root, 'git-branch-compare-line').length
diffViewport.clientHeight = 800
diffObserver.callback()
await tick()
assert.ok(findNodes(compare.root, 'git-branch-compare-line').length > previousRows)
assert.ok(findNodes(compare.root, 'git-branch-compare-line').length < 100)
diffViewport.props.onScroll({ target: { scrollTop: 610659 * 20 - 800 } })
await tick()
assert.equal(findNodes(compare.root, 'git-branch-compare-line').at(-1).text, '+line 610658')
handlers.getGitToolBranchDiff = smallDiff
await compare.state.compareBranches()
await tick()
assert.equal(findNodes(compare.root, 'git-branch-compare-line-add').length, 1)
assert.equal(compare.state.diffScrollTop, 0)
assert.deepEqual(compare.warnings, [])
assert.deepEqual(compare.errors, [])
compare.unmount()
assert.equal(diffObserver.disconnected, true)

handlers.getGitToolBranchDiff = payload => ({
  baseBranch: payload.baseBranch,
  targetBranch: payload.targetBranch,
  command: `git diff --stat ${payload.baseBranch}..${payload.targetBranch}`,
  files: [],
  fileCount: 0,
  additions: 0,
  deletions: 0,
  changedLines: 0,
  patch: ''
})
const selfCompare = mount(ComparePanel, {
  repoId: 'repo',
  branches,
  currentBranch: 'main',
  selectedBranch: 'main'
})
await tick()
assert.equal(calls.findLast(call => call.name === 'getGitToolBranchDiff').payload.baseBranch, 'main')
assert.equal(calls.findLast(call => call.name === 'getGitToolBranchDiff').payload.targetBranch, 'main')
assert.equal(findNodes(selfCompare.root, 'git-branch-compare-empty-view').length, 1)
selfCompare.unmount()

handlers = {
  getGitToolStatus: () => clone(status),
  scanGitToolBranches: () => ({ project: {}, branches, archives: [], stashArchives: [] }),
  listGitToolCommits: () => [],
  getGitToolWorktreeDiff: payload => ({ selectedFilePath: payload.filePath, patch: '+preview' }),
  getGitToolBranchDiff: smallDiff,
  executeGitToolOperation: payload => ({ success: true, output: '完成', status: clone(status) })
}
const page = mount(Page, { repos: [{ id: 'repo', name: '测试仓库', localPath: 'D:/test' }] })
await delay(60)
await tick()
assert.equal(findNodes(page.root, 'git-tool-operation-button').length, 7)
page.state.branchCompareVisible = true
await tick()
assert.equal(findNodes(page.root, 'git-tool-branch-panel').length, 0)
assert.equal(findNodes(page.root, 'git-tool-operation-bar').length, 0)
assert.equal(findNodes(page.root, 'git-branch-compare').length, 1)
page.state.branchCompareVisible = false
await tick()
assert.equal(findNodes(page.root, 'git-tool-branch-panel').length, 1)
assert.equal(findNodes(page.root, 'git-tool-operation-button').length, 7)
await page.state.openGitOperation('merge', { revision: 'feature' })
assert.equal(page.state.operationDialog.status.branch, 'main')
page.state.operationDialog = null
await tick()
const before = calls.filter(call => call.name === 'executeGitToolOperation').length
const discard = page.state.executeGitOperation({ action: 'discard', filePaths: ['both.txt'] })
await tick()
assert.equal(page.state.confirmDialog.visible, true)
assert.equal(await page.state.executeGitOperation({ action: 'commit', message: '重复操作' }), null)
assert.equal(calls.filter(call => call.name === 'executeGitToolOperation').length, before)
page.state.cancelConfirmDialog()
assert.equal(await discard, null)
assert.equal(calls.filter(call => call.name === 'executeGitToolOperation').length, before)
const confirmed = page.state.executeGitOperation({ action: 'discard', filePaths: ['both.txt'] })
await tick()
page.state.resolveConfirmDialog(true)
await confirmed
assert.equal(calls.findLast(call => call.name === 'executeGitToolOperation').payload.expectedBranch, 'main')

// 模拟冲突后转入工作区，继续操作完成后保留组件和提交草稿。
const conflictStatus = { ...clone(status), operation: 'merge', files: [{ ...status.files[0], staged: false, unstaged: false, conflicted: true }] }
handlers.executeGitToolOperation = () => ({ success: false, output: 'CONFLICT', status: clone(conflictStatus) })
await page.state.executeGitOperation({ action: 'merge', revision: preview.hash })
await tick()
assert.equal(page.state.gitWorkspace, 'worktree')
assert.equal(page.state.conflictCount, 1)
assert.equal(findNodes(page.root, 'git-workspace-conflict-actions').length, 1)
assert.equal(findNodes(page.root, 'git-tool-operation-banner-button')[1].props.disabled, true)
const workspaceNode = page.root.children[0].children.find(node => node.props.class === 'git-workspace')
const workspaceInstance = findComponent(page.component.subTree, Workspace)
assert.ok(workspaceInstance)
workspaceInstance.setupState.message = 'feat: 刷新时保留草稿'
await page.state.refreshGitProject()
await tick()
assert.equal(findComponent(page.component.subTree, Workspace), workspaceInstance)
assert.equal(workspaceInstance.setupState.message, 'feat: 刷新时保留草稿')
page.state.gitStatus = clone(conflictStatus)
await tick()
handlers.executeGitToolOperation = () => ({ success: true, output: 'resolved', status: { ...clone(status), operation: 'merge' } })
const resolving = page.state.executeGitOperation({ action: 'stage', filePaths: ['both.txt'] })
await tick()
page.state.resolveConfirmDialog(true)
await resolving
await tick()
assert.equal(page.state.conflictCount, 0)
assert.equal(findNodes(page.root, 'git-tool-operation-banner-button')[1].props.disabled, false)
handlers.executeGitToolOperation = () => ({ success: true, output: 'continued', status: { ...clone(status), files: [] } })
await page.state.executeGitOperation({ action: 'continue' })
await tick()
assert.equal(page.state.gitStatus.operation, '')
assert.equal(findNodes(page.root, 'git-tool-operation-banner').length, 0)
assert.equal(page.root.children[0].children.find(node => node.props.class === 'git-workspace'), workspaceNode)
assert.equal(workspaceInstance.setupState.message, 'feat: 刷新时保留草稿')
let finishOperation
handlers.executeGitToolOperation = () => new Promise(resolve => { finishOperation = resolve })
const pending = page.state.executeGitOperation({ action: 'fetch', remote: 'origin' })
assert.equal(await page.state.executeGitOperation({ action: 'fetch', remote: 'origin' }), null)
finishOperation({ success: true, output: 'fetch', status: clone(status) })
await pending
assert.deepEqual(page.warnings, [])
assert.deepEqual(page.errors, [])
page.unmount()

for (const { path, style } of styles) {
  const result = await compileStyleAsync({ source: style.content, filename: path, id: path, scoped: style.scoped, preprocessLang: style.lang })
  assert.deepEqual(result.errors, [], path)
}
console.log('Git 工具界面交互、请求竞争、确认流程及 SFC 编译检查通过')

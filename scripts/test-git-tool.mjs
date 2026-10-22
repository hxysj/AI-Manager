import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { resolve } from "node:path"
import {
  parse,
  compileScript,
  compileTemplate,
  compileStyleAsync
} from "@vue/compiler-sfc"
import { transformSync } from "esbuild"

// 使用 Vue 原生渲染器运行交互，只替换 IPC 和宿主节点，不操作真实仓库。
const require = createRequire(import.meta.url)
const vue = require("vue")
// 交互回归使用轻量选择器宿主，真实下拉层另在浏览器中核对。
const selectComponents = {
  ElSelect: vue.defineComponent({
    props: ["modelValue", "disabled"],
    emits: ["update:modelValue", "change"],
    setup:
      (props, { emit, slots }) =>
      () =>
        vue.h(
          "select",
          {
            value: props.modelValue,
            disabled: props.disabled,
            onChange: (event) => {
              emit("update:modelValue", event.target.value)
              emit("change", event.target.value)
            }
          },
          slots.default?.()
        )
  }),
  ElOption: vue.defineComponent({
    props: ["value", "label", "disabled"],
    setup:
      (props, { slots }) =>
      () =>
        vue.h(
          "option",
          {
            value: props.value,
            disabled: props.disabled
          },
          slots.default?.() || props.label
        )
  })
}
const calls = []
const messages = []
let handlers = {}
const api = new Proxy(
  {},
  {
    get: (_, name) => async (payload) => {
      calls.push({ name, payload })
      assert.ok(handlers[name], `缺少接口模拟：${name}`)
      return handlers[name](payload)
    }
  }
)
const modules = new Map()
const styles = []

function loadComponent(path) {
  path = resolve(path)
  if (modules.has(path)) return modules.get(path)
  const { descriptor, errors } = parse(readFileSync(path, "utf8"), {
    filename: path
  })
  assert.deepEqual(errors, [])
  const script = compileScript(descriptor, {
    id: path,
    genDefaultAs: "__component"
  })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename: path,
    id: path,
    compilerOptions: { bindingMetadata: script.bindings, hoistStatic: false }
  })
  assert.deepEqual(template.errors, [])
  for (const style of descriptor.styles) styles.push({ path, style })
  const code = transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
    { format: "cjs" }
  ).code
  const module = { exports: {} }
  const localRequire = (name) => {
    if (name === "element-plus") return selectComponents
    if (name === "element-plus/es/components/select/style/css") return {}
    if (name === "@/api") return { gitToolApi: api, systemApi: api }
    if (name === "@/utils/message")
      return {
        createMessage: {
          success: (text) => messages.push({ success: true, text }),
          error: (text) => messages.push({ success: false, text })
        }
      }
    if (name === "@/utils/formatters")
      return { formatDateTime: (value) => value }
    if (name.endsWith(".vue"))
      return {
        default: loadComponent(name.replace("@/", "src/")),
        __esModule: true
      }
    if (name === "@/features/gitTool/constant") {
      const constant = { exports: {} }
      const code = transformSync(
        readFileSync(resolve("src/features/gitTool/constant.js"), "utf8"),
        { format: "cjs" }
      ).code
      new Function("module", "exports", code)(constant, constant.exports)
      return constant.exports
    }
    return require(name)
  }
  new Function("require", "module", "exports", code)(
    localRequire,
    module,
    module.exports
  )
  modules.set(path, module.exports.default)
  return module.exports.default
}

globalThis.window = {
  setTimeout,
  clearTimeout,
  addEventListener() {},
  removeEventListener() {}
}
globalThis.Document = class {
  activeElement = null
}
globalThis.document = new Document()
// 模拟滚动容器尺寸变化，验证大 diff 在窗口缩放后仍按可见区域渲染。
globalThis.ResizeObserver = class {
  static instances = []
  constructor(callback) {
    this.callback = callback
    ResizeObserver.instances.push(this)
  }
  observe(element) {
    this.element = element
    this.callback()
  }
  disconnect() {
    this.disconnected = true
  }
}

function hostNode(type, text = "") {
  return {
    type,
    tagName: type.toUpperCase(),
    text,
    children: [],
    parent: null,
    props: {},
    listeners: {},
    value: "",
    clientHeight: 400,
    addEventListener(name, handler) {
      ;(this.listeners[name] ||= []).push(handler)
    },
    removeEventListener() {},
    setAttribute(name, value) {
      this.props[name] = value
    },
    removeAttribute(name) {
      delete this.props[name]
    },
    getRootNode() {
      return document
    },
    focus() {},
    get options() {
      return this.children.filter((node) => node.type === "option")
    }
  }
}

const renderer = vue.createRenderer({
  createElement: (type) => hostNode(type),
  createText: (text) => hostNode("#text", text),
  createComment: (text) => hostNode("#comment", text),
  setText: (node, text) => {
    node.text = text
  },
  setElementText: (node, text) => {
    node.children = []
    node.text = text
  },
  parentNode: (node) => node.parent,
  nextSibling: (node) =>
    node.parent?.children[node.parent.children.indexOf(node) + 1] || null,
  patchProp: (node, name, previous, value) => {
    node.props[name] = value
    if (name === "value") node.value = value
  },
  insert(node, parent, anchor) {
    if (node.parent)
      node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = parent
    const index = anchor ? parent.children.indexOf(anchor) : -1
    if (index < 0) parent.children.push(node)
    else parent.children.splice(index, 0, node)
  },
  remove(node) {
    if (node.parent)
      node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = null
  }
})

function mount(component, props, slots) {
  const root = hostNode("root")
  const vnode = vue.h(component, props, slots)
  const warnings = []
  const errors = []
  vnode.appContext = { ...vue.createApp({})._context }
  vnode.appContext.config.warnHandler = (message) => warnings.push(message)
  vnode.appContext.config.errorHandler = (error) => {
    errors.push(error)
  }
  renderer.render(vnode, root)
  return {
    root,
    component: vnode.component,
    state: vnode.component.setupState,
    warnings,
    errors,
    unmount: () => renderer.render(null, root)
  }
}

function findNodes(root, className) {
  const result = []
  function visit(node) {
    if (
      String(node.props.class || "")
        .split(" ")
        .includes(className)
    )
      result.push(node)
    node.children.forEach(visit)
  }
  visit(root)
  return result
}

function findComponent(vnode, component) {
  if (vnode.component?.type === component) return vnode.component
  const children = vnode.component
    ? [vnode.component.subTree]
    : Array.isArray(vnode.children)
      ? vnode.children
      : []
  for (const child of children) {
    const found = findComponent(child, component)
    if (found) return found
  }
  return null
}

const tick = async () => {
  await Promise.resolve()
  await vue.nextTick()
  await Promise.resolve()
  await vue.nextTick()
}
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const clone = (value) => structuredClone(value)
const status = {
  branch: "main",
  headHash: "a".repeat(40),
  hasHead: true,
  detached: false,
  upstream: "origin/main",
  ahead: 1,
  behind: 2,
  projectPath: "D:/test",
  operation: "",
  remotes: [{ name: "origin", fetchUrl: "local.git", pushUrl: "local.git" }],
  files: [
    {
      path: "both.txt",
      oldPath: "",
      indexStatus: "M",
      worktreeStatus: "M",
      staged: true,
      unstaged: true,
      conflicted: false,
      untracked: false
    }
  ]
}
const branches = [
  { name: "main", isCurrent: true },
  { name: "feature", isCurrent: false }
]
const Workspace = loadComponent(
  "src/features/gitTool/components/GitWorkspacePanel.vue"
)
const Modal = loadComponent(
  "src/features/gitTool/components/GitOperationModal.vue"
)
const Page = loadComponent("src/features/gitTool/index.vue")

handlers.getGitToolWorktreeDiff = (payload) => ({
  selectedFilePath: payload.filePath,
  patch: payload.staged ? "+index" : "+worktree"
})
let success = false
const workspace = mount(Workspace, {
  repoId: "repo",
  status: clone(status),
  busy: false,
  execute: async () => ({ success })
})
await tick()
assert.equal(findNodes(workspace.root, "git-workspace-file").length, 2)
assert.equal(workspace.state.detail.patch, "+worktree")
assert.equal(
  calls.findLast((call) => call.name === "getGitToolWorktreeDiff").payload
    .worktreePath,
  status.projectPath
)
await workspace.state.selectFile("both.txt", "staged")
assert.equal(workspace.state.detail.patch, "+index")
workspace.state.message = "feat: 保留提交草稿"
await workspace.state.submitCommit()
assert.equal(workspace.state.message, "feat: 保留提交草稿")
success = true
await workspace.state.submitCommit()
assert.equal(workspace.state.message, "")
assert.deepEqual(workspace.warnings, [])
assert.deepEqual(workspace.errors, [])
workspace.unmount()

let resolveOldDiff
handlers.getGitToolWorktreeDiff = (payload) =>
  payload.staged
    ? { selectedFilePath: payload.filePath, patch: "+latest" }
    : new Promise((resolve) => {
        resolveOldDiff = resolve
      })
const racingWorkspace = mount(Workspace, {
  repoId: "repo",
  status: clone(status),
  execute: async () => null
})
await racingWorkspace.state.selectFile("both.txt", "staged")
resolveOldDiff({ selectedFilePath: "both.txt", patch: "+stale" })
await tick()
assert.equal(racingWorkspace.state.detail.patch, "+latest")
racingWorkspace.unmount()

const preview = {
  hash: "b".repeat(40),
  subject: "目标提交",
  author: "测试",
  date: "2026-10-09",
  parents: ["parent1", "parent2"],
  changedFiles: ["both.txt"]
}
handlers.previewGitToolRevision = () => clone(preview)
let submitted
const reset = mount(Modal, {
  action: "reset",
  repoId: "repo",
  status: clone(status),
  branches,
  execute: async (payload) => {
    submitted = payload
    return { success: true }
  }
})
await delay(280)
await tick()
assert.equal(reset.state.canSubmit, true)
reset.state.form.resetMode = "hard"
await tick()
assert.equal(reset.state.canSubmit, false)
reset.state.form.confirmedDiscard = true
await reset.state.submit()
assert.equal(submitted.revision, preview.hash)
assert.equal(submitted.expectedBranch, "main")
assert.equal(submitted.expectedHead, status.headHash)
assert.equal(submitted.worktreePath, status.projectPath)
assert.equal(submitted.expectedWorktreePath, status.projectPath)
assert.equal(submitted.confirmedDiscard, true)
reset.state.form.revision = "new-target"
await tick()
assert.equal(reset.state.form.confirmedDiscard, false)
assert.equal(reset.state.canSubmit, false)
assert.deepEqual(reset.warnings, [])
assert.deepEqual(reset.errors, [])
reset.unmount()

let resolveOldPreview
handlers.previewGitToolRevision = (payload) =>
  payload.revision === "old"
    ? new Promise((resolve) => {
        resolveOldPreview = resolve
      })
    : clone(preview)
const pick = mount(Modal, {
  action: "cherry-pick",
  repoId: "repo",
  status: clone(status),
  branches,
  initial: { revision: "old" },
  execute: async (payload) => {
    submitted = payload
    return { success: true }
  }
})
await delay(280)
pick.state.form.revision = "new"
await tick()
await delay(280)
await tick()
resolveOldPreview({ ...preview, hash: "stale" })
await tick()
assert.equal(pick.state.preview.hash, preview.hash)
assert.equal(findNodes(pick.root, "git-operation-field").length, 3)
pick.state.form.mainline = 2
await pick.state.submit()
assert.equal(submitted.mainline, 2)
pick.unmount()

// Checkout 明确标明实际工作树，被其他工作树占用的分支不能选中。
const linkedPath = "D:/linked 中文 worktree"
const linkedStatus = {
  ...clone(status),
  projectPath: linkedPath,
  branch: "linked"
}
const occupiedBranches = [
  { name: "main", isCurrent: false, worktreePath: status.projectPath },
  { name: "linked", isCurrent: true, worktreePath: linkedPath },
  { name: "feature", isCurrent: false, worktreePath: "" }
]
const checkoutBranch = mount(Modal, {
  action: "switch-branch",
  repoId: "repo",
  status: linkedStatus,
  branches: occupiedBranches,
  execute: async (payload) => {
    submitted = payload
    return { success: true }
  }
})
const branchSelect = findNodes(
  checkoutBranch.root,
  "git-operation-control"
).find((node) => node.type === "select")
assert.equal(
  branchSelect.options.find((option) => option.props.value === "main").props
    .disabled,
  true
)
assert.equal(
  findNodes(checkoutBranch.root, "git-operation-control")[0].text,
  linkedPath
)
checkoutBranch.state.form.branchName = "main"
assert.equal(checkoutBranch.state.canSubmit, false)
checkoutBranch.state.form.branchName = "feature"
assert.equal(checkoutBranch.state.canSubmit, true)
await checkoutBranch.state.submit()
assert.equal(submitted.branchName, "feature")
assert.equal(submitted.expectedWorktreePath, linkedPath)
assert.deepEqual(checkoutBranch.errors, [])
checkoutBranch.unmount()

handlers.previewGitToolRevision = () => clone(preview)
const checkoutCommit = mount(Modal, {
  action: "checkout-commit",
  repoId: "repo",
  status: linkedStatus,
  branches: occupiedBranches,
  initial: { revision: "v1.0" },
  execute: async (payload) => {
    submitted = payload
    return { success: true }
  }
})
await delay(280)
await tick()
assert.equal(
  calls.findLast((call) => call.name === "previewGitToolRevision").payload
    .worktreePath,
  linkedPath
)
await checkoutCommit.state.submit()
assert.equal(submitted.action, "checkout-commit")
assert.equal(submitted.revision, preview.hash)
assert.equal(submitted.expectedWorktreePath, linkedPath)
assert.deepEqual(checkoutCommit.errors, [])
checkoutCommit.unmount()

const ComparePanel = loadComponent(
  "src/features/gitTool/components/GitBranchComparePanel.vue"
)
handlers.getGitToolBranchDiff = (payload) => ({
  baseBranch: payload.baseBranch,
  targetBranch: payload.targetBranch,
  command: `git diff --stat ${payload.baseBranch}..${payload.targetBranch}`,
  files: [
    {
      path: "src/app.js",
      status: "M",
      additions: 2,
      deletions: 1,
      binary: false
    }
  ],
  fileCount: 1,
  additions: 2,
  deletions: 1,
  changedLines: 3,
  patch: "diff --git a/src/app.js b/src/app.js\n+added\n-removed"
})
const compare = mount(ComparePanel, {
  repoId: "repo",
  branches,
  currentBranch: "main",
  selectedBranch: "feature"
})
await tick()
assert.equal(findNodes(compare.root, "git-branch-compare-stat-total").length, 1)
assert.equal(findNodes(compare.root, "git-branch-compare-line-add").length, 1)
assert.equal(findNodes(compare.root, "git-branch-compare-file").length, 1)
const smallDiff = handlers.getGitToolBranchDiff
handlers.getGitToolBranchDiff = (payload) => ({
  ...smallDiff(payload),
  patch: Array.from({ length: 610659 }, (_, index) => `+line ${index}`).join(
    "\n"
  )
})
await compare.state.compareBranches()
await tick()
assert.ok(findNodes(compare.root, "git-branch-compare-line").length < 100)
const diffViewport = findNodes(compare.root, "git-branch-compare-diff")[0]
const diffObserver = ResizeObserver.instances.findLast(
  (observer) => vue.toRaw(observer.element) === diffViewport
)
diffViewport.props.onScroll({ target: { scrollTop: 4000000 } })
await tick()
assert.equal(
  findNodes(compare.root, "git-branch-compare-line")[0].text,
  "+line 199980"
)
const previousRows = findNodes(compare.root, "git-branch-compare-line").length
diffViewport.clientHeight = 800
diffObserver.callback()
await tick()
assert.ok(
  findNodes(compare.root, "git-branch-compare-line").length > previousRows
)
assert.ok(findNodes(compare.root, "git-branch-compare-line").length < 100)
diffViewport.props.onScroll({ target: { scrollTop: 610659 * 20 - 800 } })
await tick()
assert.equal(
  findNodes(compare.root, "git-branch-compare-line").at(-1).text,
  "+line 610658"
)
handlers.getGitToolBranchDiff = smallDiff
await compare.state.compareBranches()
await tick()
assert.equal(findNodes(compare.root, "git-branch-compare-line-add").length, 1)
assert.equal(compare.state.diffScrollTop, 0)
assert.deepEqual(compare.warnings, [])
assert.deepEqual(compare.errors, [])
compare.unmount()
assert.equal(diffObserver.disconnected, true)

handlers.getGitToolBranchDiff = (payload) => ({
  baseBranch: payload.baseBranch,
  targetBranch: payload.targetBranch,
  command: `git diff --stat ${payload.baseBranch}..${payload.targetBranch}`,
  files: [],
  fileCount: 0,
  additions: 0,
  deletions: 0,
  changedLines: 0,
  patch: ""
})
const selfCompare = mount(ComparePanel, {
  repoId: "repo",
  branches,
  currentBranch: "main",
  selectedBranch: "main"
})
await tick()
assert.equal(
  calls.findLast((call) => call.name === "getGitToolBranchDiff").payload
    .baseBranch,
  "main"
)
assert.equal(
  calls.findLast((call) => call.name === "getGitToolBranchDiff").payload
    .targetBranch,
  "main"
)
assert.equal(
  findNodes(selfCompare.root, "git-branch-compare-empty-view").length,
  1
)
selfCompare.unmount()

// 新项目挂载时还没有分支，列表加载完成后应自动比较当前分支自身。
const beforeReady = calls.filter(
  (call) => call.name === "getGitToolBranchDiff"
).length
const waitingCompare = mount(ComparePanel, {
  repoId: "new-repo",
  branches: [],
  currentBranch: ""
})
await tick()
assert.equal(
  calls.filter((call) => call.name === "getGitToolBranchDiff").length,
  beforeReady
)
waitingCompare.component.props.branches = [{ name: "feature", isCurrent: true }]
waitingCompare.component.props.currentBranch = "feature"
await tick()
assert.deepEqual(
  calls.findLast((call) => call.name === "getGitToolBranchDiff").payload,
  {
    repoId: "new-repo",
    worktreePath: "",
    baseBranch: "feature",
    targetBranch: "feature"
  }
)
waitingCompare.unmount()

// 搜索只定位和展开祖先目录，其他文件仍留在完整树中。
const FileHistoryTree = loadComponent(
  "src/features/gitTool/components/GitFileHistoryTree.vue"
)
const FileHistory = loadComponent(
  "src/features/gitTool/components/GitFileHistoryPanel.vue"
)
let pickedFile
const fileTree = mount(FileHistoryTree, {
  files: [
    "README.md",
    "src/alpha.js",
    "src/nested/target.js",
    "src/nested/other.js",
    "tests/target.test.js"
  ],
  onSelectFile: (path) => {
    pickedFile = path
  }
})
await tick()
fileTree.state.query = "target"
await tick()
assert.equal(fileTree.state.matches.length, 2)
assert.equal(fileTree.state.locatedPath, "src/nested/target.js")
assert.ok(fileTree.state.visibleNodes.some((node) => node.path === "README.md"))
assert.ok(
  fileTree.state.visibleNodes.some((node) => node.path === "src/alpha.js")
)
assert.ok(
  fileTree.state.visibleNodes.some(
    (node) => node.path === "src/nested/other.js"
  )
)
await fileTree.state.locate(1)
assert.equal(fileTree.state.locatedPath, "tests/target.test.js")
fileTree.state.selectNode(
  fileTree.state.visibleNodes.find(
    (node) => node.path === "tests/target.test.js"
  )
)
assert.equal(pickedFile, "tests/target.test.js")
const completeCount = fileTree.state.visibleNodes.length
fileTree.state.query = "不存在的文件"
await tick()
assert.equal(fileTree.state.visibleNodes.length, completeCount)
assert.equal(fileTree.state.matches.length, 0)
assert.deepEqual(fileTree.errors, [])
fileTree.unmount()

const largeTree = mount(FileHistoryTree, {
  files: Array.from({ length: 3000 }, (_, index) => `src/file-${index}.js`)
})
largeTree.state.query = "file-2999.js"
await tick()
assert.equal(largeTree.state.visibleNodes.length, 3001)
assert.ok(findNodes(largeTree.root, "git-history-tree-row").length < 40)
assert.equal(
  findNodes(largeTree.root, "git-history-tree-row-located").length,
  1
)
// 深处滚动后目录收起，旧滚动位置也应显示剩下的目录行。
largeTree.state.selectNode(largeTree.state.tree[0])
await tick()
assert.equal(findNodes(largeTree.root, "git-history-tree-row").length, 1)
largeTree.unmount()

const historyCommit = {
  hash: "c".repeat(40),
  shortHash: "ccccccc",
  subject: "fix: 修复文件历史",
  author: "测试作者",
  date: "2026-04-29T09:30:00+08:00",
  filePath: "src/current.js",
  oldPath: "",
  checkStatus: "exists-hash",
  checkTargetBranch: "release"
}
const historyResponse = (payload) => ({
  commits: [{ ...historyCommit, filePath: payload.filePath }],
  offset: payload.offset,
  hasMore: payload.offset === 0,
  filePath: payload.filePath,
  revision: "d".repeat(40),
  branchName: payload.allBranches ? "所有分支" : payload.branchName
})
handlers.listGitToolWorktreeFiles = () => ["README.md", "src/current.js"]
handlers.listGitToolFileHistory = historyResponse
let openedHistoryCommit
const history = mount(FileHistory, {
  repoId: "repo",
  projectPath: linkedPath,
  worktreePath: linkedPath,
  branchName: "feature",
  checkBranchName: "release",
  branches,
  onSelectCommit: (commit) => {
    openedHistoryCommit = commit
  }
})
await tick()
assert.equal(
  findNodes(history.root, "git-file-history-check-branch").length,
  1,
  "文件历史应提供独立的合入对比分支选择器"
)
assert.deepEqual(Array.from(history.state.files), [
  "README.md",
  "src/current.js"
])
assert.equal(
  calls.findLast((call) => call.name === "listGitToolWorktreeFiles").payload
    .worktreePath,
  linkedPath
)
await history.state.selectFile("src/current.js")
await tick()
assert.equal(findNodes(history.root, "git-file-history-record").length, 1)
assert.equal(
  findNodes(history.root, "git-file-history-check")[0].text,
  "已合入"
)
assert.match(
  findNodes(history.root, "git-file-history-check")[0].props.title,
  /「release」.*哈希一致/
)
assert.match(
  findNodes(history.root, "git-file-history-date").at(-1).text,
  /29 Apr 2026/
)
assert.equal(
  calls.findLast((call) => call.name === "listGitToolFileHistory").payload
    .followRenames,
  true
)
history.state.filePath = "尚未提交的输入.txt"
await history.state.loadPage(60)
assert.equal(
  calls.findLast((call) => call.name === "listGitToolFileHistory").payload
    .filePath,
  "src/current.js"
)
assert.equal(
  calls.findLast((call) => call.name === "listGitToolFileHistory").payload
    .revision,
  "d".repeat(40)
)
assert.equal(history.state.result.commits.length, 1)
history.state.openCommit(history.state.result.commits[0])
assert.equal(openedHistoryCommit.historyBranchName, "feature")
// 切换查询分支自动重查，分页快照不能沿用上一分支的提交。
history.state.filePath = "src/current.js"
findNodes(history.root, "git-file-history-branch")[0].props.onChange({
  target: { value: "main" }
})
await tick()
const historyRequest = calls.findLast(
  (call) => call.name === "listGitToolFileHistory"
).payload
assert.equal(historyRequest.branchName, "main")
assert.equal(historyRequest.offset, 0)
assert.equal(historyRequest.revision, undefined)
assert.equal(history.state.result.branchName, "main")
findNodes(history.root, "git-file-history-branch")[0].props.onChange({
  target: { value: ":all" }
})
await tick()
assert.equal(
  calls.findLast((call) => call.name === "listGitToolFileHistory").payload
    .followRenames,
  false
)
assert.equal(
  calls.findLast((call) => call.name === "listGitToolFileHistory").payload
    .allBranches,
  true
)
let finishOldHistory
handlers.listGitToolFileHistory = (payload) =>
  payload.filePath === "old.js"
    ? new Promise((resolve) => {
        finishOldHistory = () => resolve(historyResponse(payload))
      })
    : historyResponse(payload)
history.state.filePath = "old.js"
const oldHistory = history.state.search()
history.state.filePath = "latest.js"
await history.state.search()
finishOldHistory()
await oldHistory
assert.equal(history.state.result.filePath, "latest.js")
handlers.listGitToolFileHistory = () => {
  throw new Error("读取失败")
}
await history.state.search()
assert.equal(history.state.result, null)
assert.match(history.state.error, /读取失败/)
assert.deepEqual(history.errors, [])
history.unmount()

handlers = {
  getGitToolStatus: () => clone(status),
  scanGitToolBranches: () => ({
    project: {},
    branches,
    archives: [],
    stashArchives: []
  }),
  listGitToolCommits: () => [],
  getGitToolWorktreeDiff: (payload) => ({
    selectedFilePath: payload.filePath,
    patch: "+preview"
  }),
  getGitToolBranchDiff: smallDiff,
  executeGitToolOperation: (payload) => ({
    success: true,
    output: "完成",
    status: clone(status)
  })
}
const page = mount(Page, {
  repos: [{ id: "repo", name: "测试仓库", localPath: "D:/test" }]
})
await delay(60)
await tick()
assert.equal(findNodes(page.root, "git-tool-operation-button").length, 8)
const operationButtons = findNodes(page.root, "git-tool-operation-button")
assert.deepEqual(
  operationButtons.map((button) => button.text),
  [
    "创建分支",
    "切换分支",
    "切换到提交",
    "合并",
    "保存改动",
    "获取更新",
    "拉取",
    "推送"
  ]
)
for (const button of operationButtons)
  assert.match(button.props.title, /命令：git /)
assert.match(operationButtons[2].props.title, /git switch --detach/)
page.state.branchCompareVisible = true
await tick()
assert.equal(findNodes(page.root, "git-tool-branch-panel").length, 0)
assert.equal(findNodes(page.root, "git-tool-operation-bar").length, 0)
assert.equal(findNodes(page.root, "git-branch-compare").length, 1)
page.state.branchCompareVisible = false
await tick()
assert.equal(findNodes(page.root, "git-tool-branch-panel").length, 1)
assert.equal(findNodes(page.root, "git-tool-operation-button").length, 8)
await page.state.openGitOperation("merge", { revision: "feature" })
assert.equal(page.state.operationDialog.status.branch, "main")
page.state.operationDialog = null
await tick()
// 右键菜单的每项操作都有图标，并在悬停提示中说明对应命令。
page.state.openCommitContextMenu(
  { clientX: 40, clientY: 50 },
  { hash: status.headHash }
)
await tick()
const commitMenuButtons = findNodes(page.root, "git-tool-context-menu-button")
assert.equal(commitMenuButtons.length, 7)
for (const button of commitMenuButtons) {
  assert.ok(button.children.some((child) => child.type === "svg"))
  assert.match(button.props.title, /命令：git /)
}
page.state.closeContextMenus()
page.state.openBranchContextMenu({ clientX: 40, clientY: 50 }, branches[1])
await tick()
const branchMenuButtons = findNodes(page.root, "git-tool-context-menu-button")
assert.equal(branchMenuButtons.length, 4)
for (const button of branchMenuButtons.slice(1)) {
  assert.ok(button.children.some((child) => child.type === "svg"))
  assert.match(button.props.title, /命令：git /)
}
page.state.closeContextMenus()
// 单条检查和列表回显都保留“改动一致”状态，避免降级成疑似匹配。
page.state.project = { ...page.state.project, checkBranchName: "release" }
const checkedCommit = {
  hash: status.headHash,
  rowId: status.headHash,
  subject: "fix: 修复图片工作台的运行内存占用",
  author: "测试作者",
  date: "2026-10-20T08:00:00+08:00",
  checkStatus: "none"
}
handlers.checkGitToolCommitOnBranch = () => ({ matchedBy: "similar-content" })
page.state.commits = [checkedCommit]
page.state.openCommitContextMenu({ clientX: 40, clientY: 50 }, checkedCommit)
await page.state.checkContextCommitOnBranch()
await tick()
assert.equal(page.state.commits[0].checkStatus, "exists-content")
assert.equal(page.state.commits[0].checkTargetBranch, "release")
assert.equal(page.state.formatCheckStatus("exists-content"), "改动一致")
assert.equal(page.state.formatCheckStatus("exists-hash"), "已合入")
assert.equal(page.state.formatCheckStatus("exists-subject"), "疑似已合入")
assert.equal(messages.at(-1).text, "改动一致")
// 各种检查状态都在实际徽标上显示目标分支与判断依据。
for (const [checkStatus, label, reason] of [
  ["exists-hash", "已合入", /提交历史.*哈希一致/],
  ["exists-subject", "疑似已合入", /标题和作者时间完全相同.*哈希不同/],
  ["exists-content", "改动一致", /85%.*改动指纹一致/],
  ["missing", "未合入", /未找到相同提交哈希/]
]) {
  page.state.commits = [
    { ...checkedCommit, checkStatus, checkTargetBranch: "release" }
  ]
  await tick()
  const badges = findNodes(page.root, "git-tool-check")
  assert.equal(badges.length, 1)
  assert.equal(badges[0].text, label)
  assert.match(badges[0].props.title, /「release」/)
  assert.match(badges[0].props.title, reason)
}
assert.equal(
  calls.findLast((call) => call.name === "checkGitToolCommitOnBranch").payload
    .targetBranchName,
  "release"
)
page.state.commits = []
const before = calls.filter(
  (call) => call.name === "executeGitToolOperation"
).length
const discard = page.state.executeGitOperation({
  action: "discard",
  filePaths: ["both.txt"]
})
await tick()
assert.equal(page.state.confirmDialog.visible, true)
assert.equal(
  await page.state.executeGitOperation({
    action: "commit",
    message: "重复操作"
  }),
  null
)
assert.equal(
  calls.filter((call) => call.name === "executeGitToolOperation").length,
  before
)
page.state.cancelConfirmDialog()
assert.equal(await discard, null)
assert.equal(
  calls.filter((call) => call.name === "executeGitToolOperation").length,
  before
)
const confirmed = page.state.executeGitOperation({
  action: "discard",
  filePaths: ["both.txt"]
})
await tick()
page.state.resolveConfirmDialog(true)
await confirmed
assert.equal(
  calls.findLast((call) => call.name === "executeGitToolOperation").payload
    .expectedBranch,
  "main"
)

// 模拟冲突后转入工作区，继续操作完成后保留组件和提交草稿。
const conflictStatus = {
  ...clone(status),
  operation: "merge",
  files: [
    { ...status.files[0], staged: false, unstaged: false, conflicted: true }
  ]
}
handlers.executeGitToolOperation = () => ({
  success: false,
  output: "CONFLICT",
  status: clone(conflictStatus)
})
await page.state.executeGitOperation({
  action: "merge",
  revision: preview.hash
})
await tick()
assert.equal(page.state.gitWorkspace, "worktree")
assert.equal(page.state.conflictCount, 1)
assert.equal(findNodes(page.root, "git-workspace-conflict-actions").length, 1)
assert.equal(
  findNodes(page.root, "git-tool-operation-banner-button")[1].props.disabled,
  true
)
const workspaceNode = page.root.children[0].children.find(
  (node) => node.props.class === "git-workspace"
)
const workspaceInstance = findComponent(page.component.subTree, Workspace)
assert.ok(workspaceInstance)
workspaceInstance.setupState.message = "feat: 刷新时保留草稿"
await page.state.refreshGitProject()
await tick()
assert.equal(
  findComponent(page.component.subTree, Workspace),
  workspaceInstance
)
assert.equal(workspaceInstance.setupState.message, "feat: 刷新时保留草稿")
page.state.gitStatus = clone(conflictStatus)
await tick()
handlers.executeGitToolOperation = () => ({
  success: true,
  output: "resolved",
  status: { ...clone(status), operation: "merge" }
})
const resolving = page.state.executeGitOperation({
  action: "stage",
  filePaths: ["both.txt"]
})
await tick()
page.state.resolveConfirmDialog(true)
await resolving
await tick()
assert.equal(page.state.conflictCount, 0)
assert.equal(
  findNodes(page.root, "git-tool-operation-banner-button")[1].props.disabled,
  false
)
handlers.executeGitToolOperation = () => ({
  success: true,
  output: "continued",
  status: { ...clone(status), files: [] }
})
await page.state.executeGitOperation({ action: "continue" })
await tick()
assert.equal(page.state.gitStatus.operation, "")
assert.equal(findNodes(page.root, "git-tool-operation-banner").length, 0)
assert.equal(
  page.root.children[0].children.find(
    (node) => node.props.class === "git-workspace"
  ),
  workspaceNode
)
assert.equal(workspaceInstance.setupState.message, "feat: 刷新时保留草稿")
let finishOperation
handlers.executeGitToolOperation = () =>
  new Promise((resolve) => {
    finishOperation = resolve
  })
const pending = page.state.executeGitOperation({
  action: "fetch",
  remote: "origin"
})
assert.equal(
  await page.state.executeGitOperation({ action: "fetch", remote: "origin" }),
  null
)
finishOperation({ success: true, output: "fetch", status: clone(status) })
await pending
assert.deepEqual(page.warnings, [])
assert.deepEqual(page.errors, [])
page.unmount()

// 同一仓库切换工作树时，路径、请求目标和草稿必须一同切换。
const worktreeList = [
  { path: status.projectPath, branch: "main", isMain: true },
  { path: linkedPath, branch: "linked", isMain: false },
  { path: "D:/gone", branch: "gone", prunable: true }
]
let detached = false
function worktreeStatus(path = status.projectPath) {
  return {
    ...clone(status),
    projectPath: path,
    branch: path === linkedPath ? (detached ? "" : "linked") : "main",
    detached: path === linkedPath && detached,
    headHash: path === linkedPath && detached ? preview.hash : status.headHash,
    worktrees: clone(worktreeList)
  }
}
handlers = {
  getGitToolStatus: (payload) =>
    worktreeStatus(payload.worktreePath || status.projectPath),
  scanGitToolBranches: (payload) => ({
    project: {
      projectPath: payload.worktreePath,
      worktrees: clone(worktreeList)
    },
    currentBranch: worktreeStatus(payload.worktreePath).branch,
    branches: occupiedBranches.map((branch) => ({
      ...branch,
      isCurrent: branch.name === worktreeStatus(payload.worktreePath).branch
    }))
  }),
  listGitToolCommits: () => [],
  getGitToolCommitDetail: () =>
    new Promise((resolve) => {
      finishOldDetail = resolve
    }),
  getGitToolWorktreeDiff: (payload) => ({
    selectedFilePath: payload.filePath,
    patch: "+selected worktree"
  }),
  executeGitToolOperation: (payload) => {
    if (payload.action === "checkout-commit") detached = true
    return {
      success: true,
      output: "完成",
      status: worktreeStatus(payload.worktreePath)
    }
  }
}
let finishOldDetail
const worktreePage = mount(Page, {
  repos: [{ id: "repo", name: "测试仓库", localPath: status.projectPath }]
})
await delay(60)
await tick()
const worktreeSelector = () =>
  findNodes(worktreePage.root, "git-tool-worktree-picker")[0]
assert.equal(worktreeSelector().options.length, 3)
assert.equal(worktreeSelector().options.at(-1).props.disabled, true)
const oldDetail = worktreePage.state.loadCommitDetail(status.headHash, "")
worktreePage.state.gitWorkspace = "worktree"
await tick()
const mainWorkspace = findComponent(worktreePage.component.subTree, Workspace)
mainWorkspace.setupState.message = "main 草稿"
// 后台刷新返回较晚，也不能覆盖已经选中的工作树。
let finishOldStatus
const freshStatus = handlers.getGitToolStatus
handlers.getGitToolStatus = () =>
  new Promise((resolve) => {
    finishOldStatus = resolve
  })
const oldRefresh = worktreePage.state.refreshGitProject({ silent: true })
handlers.getGitToolStatus = freshStatus
worktreeSelector().props.onChange({ target: { value: linkedPath } })
await delay(60)
await tick()
assert.equal(worktreePage.state.activeProjectPath, linkedPath)
assert.equal(worktreePage.state.gitStatus.branch, "linked")
const linkedWorkspace = findComponent(worktreePage.component.subTree, Workspace)
assert.notEqual(linkedWorkspace, mainWorkspace)
assert.equal(linkedWorkspace.setupState.message, "")
assert.equal(
  calls.findLast((call) => call.name === "getGitToolWorktreeDiff").payload
    .worktreePath,
  linkedPath
)
finishOldStatus(worktreeStatus())
finishOldDetail({ hash: status.headHash, files: [] })
await Promise.all([oldRefresh, oldDetail])
await tick()
assert.equal(worktreePage.state.gitStatus.branch, "linked")
assert.equal(worktreePage.state.selectedCommitDetail, null)
assert.equal(worktreePage.state.commitDetailLoading, false)
await worktreePage.state.executeGitOperation({
  action: "stage",
  filePaths: ["both.txt"]
})
const stageRequest = calls.findLast(
  (call) => call.name === "executeGitToolOperation"
).payload
assert.equal(stageRequest.worktreePath, linkedPath)
assert.equal(stageRequest.expectedWorktreePath, linkedPath)
await worktreePage.state.openGitOperation("switch-branch", {
  branchName: "feature"
})
assert.equal(worktreePage.state.operationDialog.status.projectPath, linkedPath)
assert.equal(worktreeSelector().props.disabled, true)
worktreePage.state.handleWorktreeChange(status.projectPath)
assert.equal(worktreePage.state.selectedWorktreePath, linkedPath)
worktreePage.state.operationDialog = null
await worktreePage.state.executeGitOperation({
  action: "checkout-commit",
  revision: preview.hash
})
await tick()
assert.equal(worktreePage.state.gitStatus.detached, true)
assert.equal(worktreePage.state.selectedBranch, preview.hash)
worktreePage.state.selectGitWorkspace("branch")
await tick()
assert.equal(
  calls.findLast((call) => call.name === "listGitToolCommits").payload
    .branchName,
  preview.hash
)
// 从页签进入文件历史，再打开旧路径的提交详情，保持所选工作树和查询分支。
handlers.listGitToolWorktreeFiles = () => ["src/current.js"]
handlers.listGitToolFileHistory = (payload) => ({
  ...historyResponse(payload),
  commits: [{ ...historyCommit, filePath: "src/旧名称 [1].js" }]
})
handlers.getGitToolCommitDetail = (payload) => ({
  hash: payload.commitHash,
  selectedFilePath: payload.filePath,
  files: [],
  patch: "+历史文件改动"
})
findNodes(worktreePage.root, "git-tool-tab")
  .find((button) => button.children.some((child) => child.text === "文件历史"))
  .props.onClick()
await tick()
const pageHistory = findComponent(worktreePage.component.subTree, FileHistory)
assert.ok(pageHistory)
assert.equal(pageHistory.props.worktreePath, linkedPath)
pageHistory.setupState.scope = "feature"
await pageHistory.setupState.selectFile("src/current.js")
await tick()
// 文件历史沿用项目对比分支，切换只更新当前面板，不额外加载分支提交列表。
handlers.updateGitToolCheckBranch = (payload) => ({
  ...worktreePage.state.project,
  checkBranchName: payload.branchName
})
handlers.listGitToolFileHistory = (payload) => ({
  ...historyResponse(payload),
  commits: [
    {
      ...historyCommit,
      filePath: "src/旧名称 [1].js",
      checkStatus: worktreePage.state.project.checkBranchName
        ? "exists-hash"
        : "",
      checkTargetBranch: worktreePage.state.project.checkBranchName
    }
  ]
})
const compareSelector = () =>
  findNodes(worktreePage.root, "git-file-history-check-branch")[0]
const commitCallsBeforeCheck = calls.filter(
  (call) => call.name === "listGitToolCommits"
).length
compareSelector().props.onChange({ target: { value: "main" } })
await tick()
assert.equal(
  calls.findLast((call) => call.name === "updateGitToolCheckBranch").payload
    .branchName,
  "main"
)
assert.equal(pageHistory.props.checkBranchName, "main")
assert.equal(pageHistory.setupState.result.commits[0].checkTargetBranch, "main")
assert.equal(pageHistory.setupState.result.branchName, "feature")
assert.equal(
  calls.filter((call) => call.name === "listGitToolCommits").length,
  commitCallsBeforeCheck
)
assert.equal(
  calls.findLast((call) => call.name === "listGitToolFileHistory").payload
    .revision,
  undefined
)
assert.match(
  findNodes(worktreePage.root, "git-file-history-check")[0].props.title,
  /「main」/
)
// 保存失败时保持原配置和查询结果，显示错误并恢复选择器。
handlers.updateGitToolCheckBranch = () => {
  throw new Error("保存对比分支失败")
}
compareSelector().props.onChange({ target: { value: "feature" } })
await tick()
assert.equal(pageHistory.props.checkBranchName, "main")
assert.equal(pageHistory.setupState.result.commits[0].checkTargetBranch, "main")
assert.equal(compareSelector().props.disabled, false)
assert.match(messages.at(-1).text, /保存对比分支失败/)
handlers.updateGitToolCheckBranch = (payload) => ({
  ...worktreePage.state.project,
  checkBranchName: payload.branchName
})
// 上一次合入检查返回较晚时，不能覆盖已关闭检查的新结果。
let finishPreviousCheck
const checkedHistoryResponse = handlers.listGitToolFileHistory
handlers.listGitToolFileHistory = (payload) => {
  const response = checkedHistoryResponse(payload)
  if (worktreePage.state.project.checkBranchName === "feature") {
    return new Promise((resolve) => {
      finishPreviousCheck = () => resolve(response)
    })
  }
  return response
}
compareSelector().props.onChange({ target: { value: "feature" } })
await tick()
assert.equal(typeof finishPreviousCheck, "function")
compareSelector().props.onChange({ target: { value: "" } })
await tick()
finishPreviousCheck()
await tick()
assert.equal(pageHistory.setupState.result.commits[0].checkStatus, "")
assert.equal(findNodes(worktreePage.root, "git-file-history-check").length, 0)
findNodes(worktreePage.root, "git-file-history-record")[0].props.onClick()
await tick()
assert.deepEqual(
  calls.findLast((call) => call.name === "getGitToolCommitDetail").payload,
  {
    repoId: "repo",
    worktreePath: linkedPath,
    commitHash: historyCommit.hash,
    filePath: "src/旧名称 [1].js"
  }
)
assert.equal(worktreePage.state.commitModalBranchName, "feature")
assert.equal(worktreePage.state.commitModalProjectPath, linkedPath)
assert.equal(worktreePage.state.commitModalCommits.length, 0)
assert.equal(worktreePage.state.selectedCommitDetail.patch, "+历史文件改动")
assert.deepEqual(worktreePage.warnings, [])
assert.deepEqual(worktreePage.errors, [])
worktreePage.unmount()

for (const { path, style } of styles) {
  const result = await compileStyleAsync({
    source: style.content,
    filename: path,
    id: path,
    scoped: style.scoped,
    preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [], path)
}
console.log("Git 工具界面交互、请求竞争、确认流程及 SFC 编译检查通过")

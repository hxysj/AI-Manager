// 离线挂载实际工作台状态逻辑，所有账号、文件和生图接口都由内存替身提供。
import assert from "node:assert/strict"
import { readFile, writeFile, unlink } from "node:fs/promises"
import { resolve } from "node:path"
import { pathToFileURL } from "node:url"
import {
  parse,
  compileScript,
  compileTemplate,
  compileStyleAsync
} from "@vue/compiler-sfc"
import { build } from "esbuild"
import { createRenderer, nextTick } from "vue"

const storage = new Map()
globalThis.localStorage = {
  getItem: (key) => storage.get(key),
  setItem: (key, value) => storage.set(key, value)
}
globalThis.window = { setTimeout, clearTimeout }
globalThis.document = { hidden: false }
const calls = []
const listeners = new Map()
let registrationGate = Promise.resolve()
let registrationFailure = null
const subscribe = (name, callback) => {
  listeners.set(name, callback)
  return () => listeners.delete(name)
}
globalThis.__imageEvents = {
  subscribe,
  subscribeReady: async (...args) => {
    await registrationGate
    if (registrationFailure) throw registrationFailure
    return subscribe(...args)
  }
}
let tasks = []
let history = []
let taskTotal = null
let nextListReply = null
let nextHistoryReply = null
let batchSubmit = async () => ({ items: [] })
let batchWait = async () => ({ items: [] })
globalThis.__imageTest = {
  promptName: "新名称",
  systemApi: { saveFile: async () => "" },
  toolboxApi: {
    imageAccounts: async () => [{ id: "account", active: true }],
    imageModels: async () => ({
      data: [{ id: "gpt-image-2" }, { id: "gpt-5-5" }]
    }),
    imageQuota: async () => ({ accountId: "account", remaining: 3 }),
    listImageTasks: async (payload) => {
      calls.push(["list", payload])
      if (nextListReply) {
        const reply = nextListReply
        nextListReply = null
        return reply
      }
      const items = payload.status
        ? tasks.filter((task) => task.status === payload.status)
        : tasks
      return {
        items: structuredClone(items),
        total: taskTotal ?? (items.length ? 1 : 0)
      }
    },
    imageHistory: async () => {
      if (nextHistoryReply) {
        const reply = nextHistoryReply
        nextHistoryReply = null
        return reply
      }
      return structuredClone(history)
    },
    submitImageBatch: (payload) => batchSubmit(payload),
    waitImageTasks: (payload) => batchWait(payload),
    imageTaskPreviews: async ({ ids }) => {
      calls.push(["previews", ids])
      return ids.map((id) => ({ id, thumbnail: `preview-${id}` }))
    },
    imageTaskInputs: async () => ({
      images: ["data:image/png;base64,reference"],
      mask: "data:image/png;base64,mask"
    }),
    submitImageTask: async (payload) => {
      calls.push(["submit", structuredClone(payload)])
      return { roundId: payload.roundId, items: [] }
    },
    resumeImageTask: async (payload) => {
      calls.push(["resume", payload])
      return {}
    },
    clearImageResults: async (payload) => {
      calls.push(["clear", payload])
      return {}
    },
    deleteImageTasks: async (payload) => {
      calls.push(["delete", payload])
      tasks = []
      history = []
      return {}
    }
  },
  createMessage: {
    success: () => {},
    warning: () => {},
    error: (message) => {
      throw new Error(message)
    }
  }
}

const temporary = resolve(`scripts/.image-workbench-test-${process.pid}.mjs`)
const batchTemporary = resolve(`scripts/.image-batch-test-${process.pid}.mjs`)
const panelTemporary = resolve(`scripts/.image-panel-test-${process.pid}.mjs`)
const sourcePath = resolve("src/features/tools/imageWorkbench/index.vue")
const batchSourcePath = resolve(
  "src/features/tools/imageWorkbench/components/ImageBatchDialog.vue"
)
const panelSourcePath = resolve(
  "src/features/tools/imageWorkbench/components/BatchResultsPanel.vue"
)
try {
  for (const path of [
    sourcePath,
    batchSourcePath,
    resolve(
      "src/features/tools/imageWorkbench/components/ImageDrawingDialog.vue"
    ),
    resolve(
      "src/features/tools/imageWorkbench/components/BatchResultsPanel.vue"
    )
  ]) {
    const { descriptor, errors } = parse(await readFile(path, "utf8"))
    assert.deepEqual(errors, [])
    const script = compileScript(descriptor, { id: path })
    const template = compileTemplate({
      source: descriptor.template.content,
      filename: path,
      id: path,
      compilerOptions: { bindingMetadata: script.bindings }
    })
    const style = await compileStyleAsync({
      source: descriptor.styles[0].content,
      filename: path,
      id: path,
      preprocessLang: "less"
    })
    assert.deepEqual(template.errors, [])
    assert.deepEqual(style.errors, [])
  }
  const bundle = await build({
    entryPoints: [sourcePath, batchSourcePath, panelSourcePath],
    outdir: "scripts/.image-workbench-check",
    bundle: true,
    write: false,
    format: "esm",
    platform: "node",
    external: ["vue", "xlsx"],
    plugins: [
      {
        name: "offline-image-workbench",
        setup(builder) {
          builder.onResolve(
            { filter: /^@\/api$|^@\/utils\/message$/ },
            (args) => ({ path: args.path, namespace: "mock" })
          )
          builder.onLoad({ filter: /.*/, namespace: "mock" }, () => ({
            contents:
              "export const {systemApi, toolboxApi, createMessage} = globalThis.__imageTest"
          }))
          builder.onResolve({ filter: /^@\/api\/request$/ }, () => ({
            path: "events",
            namespace: "events"
          }))
          builder.onLoad({ filter: /.*/, namespace: "events" }, () => ({
            contents:
              "export const { subscribe, subscribeReady } = globalThis.__imageEvents"
          }))
          builder.onResolve(
            { filter: /^element-plus$|^lucide-vue-next$|\.css$|\.vue$/ },
            (args) => {
              if (args.kind === "entry-point") return
              return { path: args.path, namespace: "ui" }
            }
          )
          builder.onLoad({ filter: /.*/, namespace: "ui" }, () => ({
            contents:
              "export default {}; export const ElImage = {}; export const ElDropdown = {}; export const ElDropdownItem = {}; export const ElDropdownMenu = {}; export const ElMessageBox = {confirm: async () => {}, prompt: async () => ({value: globalThis.__imageTest.promptName})}; export const Download={}, DownloadCloud={}, Upload={}, UploadCloud={}, BookOpen={}, ImageOff={}, ImagePlus={}, LoaderCircle={}, Paintbrush={}, Plus={}, RefreshCw={}, Sparkles={}, Trash2={}, X={}, Copy={}, Check={}, CheckCircle2={}, CheckSquare={}, ChevronDown={}, ChevronUp={}, ChevronLeft={}, ChevronRight={}, SlidersHorizontal={}, Maximize2={}, Info={}, Clock={}, AlertCircle={}, Cpu={}, Database={}, FileText={}, Grid={}, Image={}, List={}, MessageSquare={}, Minus={}, MoreHorizontal={}, MoreVertical={}, Pause={}, Pencil={}, Play={}, Search={}, Square={}, Terminal={}, ZoomIn={}, RotateCcw={}, FileSpreadsheet={}, HelpCircle={}, Settings={}"
          }))
          builder.onResolve({ filter: /^@\// }, (args) => ({
            path: resolve("src", args.path.slice(2) + ".js")
          }))
          builder.onLoad(
            {
              filter:
                /index\.vue$|ImageBatchDialog\.vue$|BatchResultsPanel\.vue$/
            },
            async (args) => ({
              contents: compileScript(
                parse(await readFile(args.path, "utf8")).descriptor,
                { id: "workbench-test" }
              ).content,
              resolveDir: resolve("src/features/tools/imageWorkbench")
            })
          )
        }
      }
    ]
  })
  for (const [name, target] of [
    ["index.js", temporary],
    ["ImageBatchDialog.js", batchTemporary],
    ["BatchResultsPanel.js", panelTemporary]
  ]) {
    await writeFile(
      target,
      bundle.outputFiles.find((file) => file.path.endsWith(name)).contents
    )
  }
  const component = (await import(pathToFileURL(temporary))).default
  component.render = () => null
  const renderer = createRenderer({
    createComment: () => ({}),
    insert: () => {},
    remove: () => {},
    parentNode: () => null,
    nextSibling: () => null
  })
  const app = renderer.createApp(component)
  const instance = app.mount({})
  const state = instance.$.setupState
  const settle = async () => {
    for (let i = 0; i < 12; i++) await nextTick()
  }
  await settle()
  assert.equal(state.form.generationMode, "web")
  assert.equal(state.form.quality, "auto")
  assert.equal(state.form.model, "gpt-image-2")
  assert.equal(state.form.accountId, "account")
  assert.equal(state.pageSize, 50)
  assert.equal(calls.filter(([name]) => name === "list").at(-1)[1].pageSize, 50)
  // 切换条数应重置页码和勾选，并把数值传给后端，而不是 Vue 引用。
  taskTotal = 123
  state.page = 3
  await settle()
  assert.equal(state.page, 3)
  state.selected = ["stale-task"]
  state.pageSize = 20
  await settle()
  assert.equal(state.page, 1)
  assert.deepEqual([...state.selected], [])
  assert.equal(calls.filter(([name]) => name === "list").at(-1)[1].pageSize, 20)
  state.pageSize = 100
  await settle()
  assert.equal(
    calls.filter(([name]) => name === "list").at(-1)[1].pageSize,
    100
  )
  assert.equal(
    JSON.parse(storage.get("image-workbench-preferences")).pageSize,
    100
  )
  taskTotal = null
  state.form.model = "gpt-image-2.5-flare"
  await settle()
  assert.equal(state.qualityOptions.length, 6)
  state.form.quality = "max"
  state.form.model = "gpt-image-2"
  await settle()
  assert.equal(state.form.quality, "auto")
  state.applySizePreset(state.sizePresets.find((item) => item.label === "16:9"))
  state.width = 1900
  await settle()
  assert.equal(state.ratio, "16:9")
  assert.equal(state.height, 1088)
  state.form.prompt = "测试画面"
  state.form.n = 100
  assert.ok(state.canSubmit)
  state.form.n = 101
  assert.equal(state.canSubmit, false)
  state.form.n = 100
  const before = calls.filter(([name]) => name === "submit").length
  state.promptKeydown({ key: "Enter", isComposing: true })
  state.promptKeydown({ key: "Enter", shiftKey: true })
  assert.equal(calls.filter(([name]) => name === "submit").length, before)
  await state.submitTask()
  const submitted = calls.find(([name]) => name === "submit")[1]
  assert.equal(submitted.n, 100)
  assert.equal(submitted.size, "1900x1088")
  assert.equal(submitted.generationMode, "web")
  assert.ok(submitted.conversationId && submitted.roundId)
  assert.equal(JSON.parse(storage.get("image-workbench-preferences")).n, 100)
  state.applyDrawing({
    name: "sketch.png",
    url: "data:image/png;base64,sketch",
    source: ""
  })
  assert.equal(state.references.length, 1)
  assert.equal(state.form.mode, "edit")
  state.applyDrawing({
    name: "edit-mask.png",
    url: "data:image/png;base64,mask",
    source: "data:image/png;base64,result"
  })
  assert.equal(state.references[0].url, "data:image/png;base64,result")
  assert.equal(state.mask.name, "edit-mask.png")
  state.removeReference(0)
  assert.equal(state.mask, null)
  assert.equal(state.form.mode, "generate")
  const task = {
    id: "original",
    batchCount: 4,
    status: "failed",
    createdAt: Date.now(),
    request: {
      ...submitted,
      n: 1,
      generationMode: "codex",
      conversationId: "original-conversation",
      roundId: "original-round",
      size: "1024x1536",
      mode: "edit"
    }
  }
  await state.reuseTask(task, true)
  await settle()
  assert.equal(state.form.generationMode, "codex")
  assert.equal(state.form.n, 4)
  assert.equal(state.height, 1536)
  assert.equal(state.references.length, 1)
  assert.ok(state.mask)
  assert.ok(!("conversationId" in state.form))
  state.form.generationMode = "web"
  await state.regenerateTask(task)
  const retried = calls.filter(([name]) => name === "submit").at(-1)[1]
  assert.equal(retried.generationMode, "codex")
  assert.equal(retried.n, 1)
  assert.equal(retried.conversationId, "original-conversation")
  assert.deepEqual(
    [...state.roundState["original-round"].replaced],
    ["original"]
  )
  await state.regenerateRound({ id: "original-round", tasks: [task] })
  const regenerated = calls.filter(([name]) => name === "submit").at(-1)[1]
  assert.notEqual(regenerated.roundId, "original-round")
  assert.equal(regenerated.n, 4)
  const submitCount = calls.filter(([name]) => name === "submit").length
  await state.resumeTask(task)
  assert.equal(calls.filter(([name]) => name === "submit").length, submitCount)
  assert.deepEqual(calls.filter(([name]) => name === "resume").at(-1)[1], {
    id: "original"
  })
  const previousId = state.activeConversationId
  state.newConversation()
  await settle()
  assert.notEqual(state.activeConversationId, previousId)
  assert.equal(state.form.prompt, "")
  assert.equal(state.references.length, 0)
  await state.renameConversation(state.currentConversation)
  assert.equal(state.currentConversation.title, "新名称")
  globalThis.__imageTest.promptName = "新对话"
  await state.renameConversation(state.currentConversation)
  state.form.prompt = "保留用户主动填写的名称"
  await state.submitTask()
  assert.equal(state.currentConversation.title, "新对话")
  globalThis.__imageTest.promptName = "新名称"
  await state.renameConversation(state.currentConversation)
  assert.ok(
    !storage.get("image-workbench-conversations").includes("data:image")
  )
  assert.ok(
    calls.some(([name, payload]) => name === "list" && payload.groupByRound)
  )
  tasks = [task]
  history = [
    {
      id: task.id,
      status: "failed",
      createdAt: task.createdAt,
      conversationId: task.request.conversationId,
      roundId: task.request.roundId,
      prompt: task.request.prompt
    }
  ]
  await state.loadHistory()
  state.ignoreTaskError({ id: "original-round" }, task)
  assert.deepEqual(
    [...state.roundState["original-round"].ignored],
    ["original"]
  )
  await state.removeRoundPrompt({ id: "original-round" })
  assert.equal(state.roundState["original-round"].hidePrompt, true)
  await state.removeRoundResults({ id: "original-round" })
  assert.deepEqual(calls.filter(([name]) => name === "clear").at(-1)[1].ids, [
    "original"
  ])
  await state.deleteConversation("original-conversation")
  assert.deepEqual(calls.filter(([name]) => name === "delete").at(-1)[1].ids, [
    "original"
  ])
  assert.ok(
    !state.conversations.some((item) => item.id === "original-conversation")
  )
  const lastConversation = state.activeConversationId
  app.unmount()
  const restoredApp = renderer.createApp(component)
  const restored = restoredApp.mount({}).$.setupState
  await settle()
  assert.equal(restored.activeConversationId, lastConversation)
  assert.equal(restored.height, 1536)
  assert.equal(restored.form.n, 4)
  assert.equal(restored.pageSize, 100)
  assert.equal(restored.currentConversation.title, "新名称")
  restored.form.prompt = "测试提示词"
  // 校验图像设置弹框相关计算属性、重置与超限检测
  assert.equal(restored.hasSettingsError, false)
  restored.width = 9000
  restored.height = 9000
  await settle()
  assert.equal(restored.hasSettingsError, true)
  assert.equal(restored.canSubmit, false)
  restored.resetToDefaultSettings()
  await settle()
  assert.equal(restored.width, 1024)
  assert.equal(restored.height, 1024)
  assert.equal(restored.form.quality, "auto")
  assert.equal(restored.form.n, 1)
  assert.equal(restored.hasSettingsError, false)
  // 透明背景与 JPEG 冲突检测
  restored.form.background = "transparent"
  restored.form.outputFormat = "jpeg"
  await settle()
  assert.equal(restored.hasSettingsError, true)
  restored.form.outputFormat = "png"
  await settle()
  assert.equal(restored.hasSettingsError, false)
  // 校验模型直选与自定义输入
  restored.form.model = "gpt-5-5"
  await settle()
  assert.equal(restored.form.model, "gpt-5-5")
  restored.form.model = "custom-model-x"
  await settle()
  assert.equal(restored.form.model, "custom-model-x")
  assert.equal(restored.canSubmit, true)

  // 任务和历史快照加载期间的旧响应不能覆盖完成通知。
  const liveTask = {
    id: "live-task",
    status: "processing",
    revision: 1,
    imageCount: 0,
    request: {
      conversationId: restored.activeConversationId,
      roundId: "live-round"
    }
  }
  tasks = [liveTask]
  history = [
    {
      id: liveTask.id,
      status: "processing",
      revision: 1,
      conversationId: restored.activeConversationId,
      createdAt: 1
    }
  ]
  await restored.loadTasks()
  const staleTasks = structuredClone(tasks)
  let releaseList
  nextListReply = new Promise((resolve) => {
    releaseList = resolve
  })
  const loading = restored.loadTasks()
  await settle()
  const completion = {
    taskId: liveTask.id,
    conversationId: restored.activeConversationId,
    task: { ...liveTask, status: "completed", revision: 2, imageCount: 1 }
  }
  tasks = [completion.task]
  history = [{ ...history[0], status: "completed", revision: 2 }]
  listeners.get("images:changed")(completion)
  listeners.get("images:changed")({ ...completion, task: liveTask })
  releaseList({ items: staleTasks, total: 1 })
  await loading
  assert.equal(restored.tasks[0].status, "completed")
  assert.equal(restored.tasks[0].revision, 2)

  restored.autoRefresh = false
  await settle()
  tasks = [{ ...completion.task, status: "failed", revision: 3 }]
  history = [{ ...history[0], status: "failed", revision: 3 }]
  listeners.get("images:changed")({ ...completion, task: tasks[0] })
  assert.equal(restored.tasks[0].status, "completed")
  restored.autoRefresh = true
  await settle()
  assert.equal(restored.tasks[0].status, "failed")

  let releaseHistory
  const staleHistory = structuredClone(history)
  nextHistoryReply = new Promise((resolve) => {
    releaseHistory = resolve
  })
  const historyLoading = restored.loadHistory()
  await settle()
  tasks = [{ ...completion.task, revision: 4 }]
  history = [{ ...history[0], status: "completed", revision: 4 }]
  listeners.get("images:changed")({ ...completion, task: tasks[0] })
  releaseHistory(staleHistory)
  await historyLoading
  assert.equal(restored.historyIndex[0].status, "completed")

  history.push({
    id: "other-task",
    conversationId: "other-conversation",
    status: "queued",
    revision: 0,
    createdAt: 2
  })
  await restored.loadHistory()
  const listCount = calls.filter(([name]) => name === "list").length
  listeners.get("images:changed")({
    conversationId: "other-conversation",
    task: { id: "other-task", status: "processing", revision: 1 }
  })
  assert.equal(
    restored.historyIndex.find((item) => item.id === "other-task").status,
    "processing"
  )
  assert.equal(calls.filter(([name]) => name === "list").length, listCount)

  tasks = [{ ...liveTask, revision: 5 }]
  history[0] = { ...history[0], status: "processing", revision: 5 }
  restored.status = "processing"
  await settle()
  tasks = [{ ...completion.task, revision: 6 }]
  history[0] = { ...history[0], status: "completed", revision: 6 }
  listeners.get("images:changed")({ ...completion, task: tasks[0] })
  await new Promise((resolve) => setTimeout(resolve, 250))
  await settle()
  assert.equal(restored.tasks.length, 0)
  assert.equal(restored.total, 0)
  restored.status = ""
  await settle()
  listeners.get("images:changed")({
    ...completion,
    task: { ...tasks[0], status: "interrupted", revision: 7 }
  })
  listeners.get("images:changed")({ ...completion, task: tasks[0] })
  assert.equal(restored.tasks[0].status, "interrupted")
  assert.equal(restored.tasks[0].revision, 7)
  restoredApp.unmount()

  // 批量进度先等监听注册，完成判断由后端快照返回。
  const batchComponent = (await import(pathToFileURL(batchTemporary))).default
  batchComponent.render = () => null
  let releaseRegistration
  registrationGate = new Promise((resolve) => {
    releaseRegistration = resolve
  })
  let saveCount = 0
  let submitCountForBatch = 0
  let finishedCount = 0
  globalThis.__imageTest.systemApi.saveFile = async () => {
    saveCount++
    return ""
  }
  const batchProps = {
    visible: true,
    settings: { ...submitted, accountId: "account" },
    onFinished: () => {
      finishedCount++
    }
  }
  const batchApp = renderer.createApp(batchComponent, batchProps)
  const batchState = batchApp.mount({}).$.setupState
  batchState.rows[0].prompt = "批量测试"
  const completedBatchTask = {
    id: "batch-task",
    status: "completed",
    revision: 2,
    imageCount: 1
  }
  let releaseWait
  batchSubmit = async (payload) => {
    submitCountForBatch++
    assert.ok(listeners.has("images:changed"))
    listeners.get("images:changed")({
      conversationId: payload.conversationId,
      task: completedBatchTask
    })
    return { items: [{ id: "batch-task", status: "queued", revision: 0 }] }
  }
  batchWait = async ({ ids }) => {
    assert.deepEqual(ids, ["batch-task"])
    return new Promise((resolve) => {
      releaseWait = resolve
    })
  }
  const batchStart = batchState.startBatch()
  await settle()
  assert.equal(submitCountForBatch, 0)
  releaseRegistration()
  await settle()
  assert.equal(batchState.rows[0].status, "completed")
  releaseWait({ items: [completedBatchTask] })
  await batchStart
  assert.equal(saveCount, 1)
  assert.equal(finishedCount, 1)
  batchApp.unmount()

  // 没收到完成事件也能结束；离开页面后不能弹保存窗口或发送 finished。
  registrationGate = Promise.resolve()
  batchSubmit = async () => ({
    items: [{ id: "batch-task", status: "queued", revision: 0 }]
  })
  batchWait = async () => ({ items: [completedBatchTask] })
  const silentApp = renderer.createApp(batchComponent, batchProps)
  const silentState = silentApp.mount({}).$.setupState
  silentState.rows[0].prompt = "无事件测试"
  const beforeBatchLists = calls.filter(([name]) => name === "list").length
  await silentState.startBatch()
  assert.equal(silentState.rows[0].status, "completed")
  assert.equal(
    calls.filter(([name]) => name === "list").length,
    beforeBatchLists
  )
  silentApp.unmount()

  batchWait = async () =>
    new Promise((resolve) => {
      releaseWait = resolve
    })
  const leavingApp = renderer.createApp(batchComponent, batchProps)
  const leavingState = leavingApp.mount({}).$.setupState
  leavingState.rows[0].prompt = "卸载测试"
  const leavingStart = leavingState.startBatch()
  await settle()
  const beforeLeaving = { saveCount, finishedCount }
  leavingApp.unmount()
  await leavingStart
  releaseWait({ items: [completedBatchTask] })
  await settle()
  assert.deepEqual({ saveCount, finishedCount }, beforeLeaving)
  assert.equal(listeners.size, 0)

  registrationFailure = new Error("监听注册失败")
  const failedApp = renderer.createApp(batchComponent, batchProps)
  const failedState = failedApp.mount({}).$.setupState
  failedState.rows[0].prompt = "注册失败测试"
  batchSubmit = async () => {
    throw new Error("注册失败时不应提交")
  }
  await failedState.startBatch()
  assert.match(failedState.rows[0].error, /监听注册失败/)
  failedApp.unmount()
  registrationFailure = null

  // 200 条批量元数据只读取当前页的 20 张缩略图，切页释放上一页。
  const panelComponent = (await import(pathToFileURL(panelTemporary))).default
  panelComponent.render = () => null
  const panelApp = renderer.createApp(panelComponent, {
    tasks: Array.from({ length: 200 }, (_, index) => ({
      id: `preview-task-${index}`,
      status: "completed",
      imageCount: 1,
      revision: 2
    }))
  })
  const panelState = panelApp.mount({}).$.setupState
  await settle()
  assert.equal(Object.keys(panelState.previews).length, 20)
  assert.equal(
    calls.filter(([name]) => name === "previews").at(-1)[1].length,
    20
  )
  panelState.page = 2
  await settle()
  assert.equal(Object.keys(panelState.previews).length, 20)
  assert.ok(panelState.previews["preview-task-20"])
  assert.equal(panelState.previews["preview-task-0"], undefined)
  let releasePreviews
  const previewsApi = globalThis.__imageTest.toolboxApi.imageTaskPreviews
  globalThis.__imageTest.toolboxApi.imageTaskPreviews = async () =>
    new Promise((resolve) => {
      releasePreviews = resolve
    })
  panelState.page = 3
  await settle()
  globalThis.__imageTest.toolboxApi.imageTaskPreviews = previewsApi
  panelState.page = 4
  await settle()
  releasePreviews([{ id: "preview-task-40", thumbnail: "stale" }])
  await settle()
  assert.ok(panelState.previews["preview-task-60"])
  assert.equal(panelState.previews["preview-task-40"], undefined)
  panelApp.unmount()
  console.log(
    "生图工作台离线检查通过：原有功能、监听注册竞态、无事件完成、取消版本、快照重放、跨会话状态、自动更新开关、筛选、卸载与缩略图按页加载。"
  )
} finally {
  await unlink(temporary).catch(() => {})
  await unlink(batchTemporary).catch(() => {})
  await unlink(panelTemporary).catch(() => {})
}

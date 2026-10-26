import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { parse, compileScript, compileTemplate } from "@vue/compiler-sfc"
import { transformSync } from "esbuild"
import { createSSRApp } from "vue"
import { renderToString } from "@vue/server-renderer"

const require = createRequire(import.meta.url)
const filename = "src/features/providers/index.vue"
const { descriptor, errors } = parse(readFileSync(filename, "utf8"), {
  filename
})
assert.deepEqual(errors, [])
const script = compileScript(descriptor, {
  id: "providers-startup",
  genDefaultAs: "__component"
})
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: "providers-startup",
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
const code = transformSync(
  `${script.content}\n${template.code}\n__component.render = render\nexport default __component`.replaceAll(
    "import.meta.glob",
    "globAssets"
  ),
  { format: "cjs" }
).code
const module = { exports: {} }
const api = new Proxy(
  {},
  { get: () => () => assert.fail("首屏测试不应发起外部请求") }
)
const modelFetchCalls = []
const modelFetchErrors = []
let fetchClaudeModelsResult = () => assert.fail("未配置模型获取结果")
const providerApi = {
  fetchClaudeModels: payload => {
    modelFetchCalls.push(payload)
    return fetchClaudeModelsResult(payload)
  }
}
const openedUrls = []
const copiedUrls = []
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { clipboard: { writeText: async value => copiedUrls.push(value) } }
})
const localRequire = name => {
  if (name.endsWith("/style/css")) return {}
  if (name === "@/components/BaseModal.vue") {
    return {
      default: {
        inheritAttrs: false,
        emits: ["close"],
        render() {
          return this.$slots.default?.()
        }
      },
      __esModule: true
    }
  }
  if (name.endsWith(".vue"))
    return { default: { render: () => null }, __esModule: true }
  if (name.startsWith("monaco-editor")) return {}
  if (name === "@/api")
    return {
      accountApi: api,
      runtimeApi: api,
      systemApi: {
        openExternal: async payload => openedUrls.push(payload.url)
      },
      usageApi: api
    }
  if (name === "@/api/modules/accounts") return { googleAccountApi: api }
  if (name === "@/api/modules/providers")
    return { claudeDesktopApi: api, providerApi }
  if (name === "@/utils/message")
    return {
      createMessage: { success() {}, error: error => modelFetchErrors.push(error) }
    }
  if (name === "@/utils/typography") return { getFontSize: () => 14 }
  if (name === "@/utils/formatters")
    return { formatTokenCount: value => String(value) }
  return require(name)
}
new Function("require", "module", "exports", "globAssets", code)(
  localRequire,
  module,
  module.exports,
  () => ({})
)
const Providers = module.exports.default
const props = {
  cliTargets: [],
  pending: false,
  codexAccounts: [],
  providers: [],
  runtimeConfigSchemas: {},
  runtimeModels: [],
  runtimeProviderState: {},
  runtimeProfiles: []
}
const schema = {
  enabled: true,
  modelFields: [{ key: "modelList", label: "模型列表", type: "textarea" }],
  optionFields: [],
  advancedFields: [],
  configFiles: [],
  authFields: ["OPENAI_API_KEY"],
  defaultProviderType: "openai"
}
for (const cli of [null, "claude", "codex", "claude-desktop"]) {
  const app = createSSRApp(Providers, {
    ...props,
    cliTargets: cli ? [{ id: cli, name: cli, installed: true }] : [],
    runtimeConfigSchemas:
      cli && cli !== "claude-desktop" ? { [cli]: schema } : {}
  })
  const failures = []
  app.component("ElInput", { render: () => null })
  app.config.errorHandler = error => failures.push(error)
  const html = await renderToString(app)
  assert.deepEqual(
    failures,
    [],
    `${cli || "空状态"} 首屏初始化失败：${failures.map(error => error.stack).join("\n")}`
  )
  assert.ok(html.includes("providers-view"))
  if (cli) assert.ok(html.includes(cli))
  assert.equal(html.includes("providers-view__toolbar"), Boolean(cli))
  assert.equal(html.includes("尚未检测到已安装的应用"), !cli)
  assert.ok(!html.includes("同步模型列表"))
  assert.ok(!html.includes("恢复默认模型"))
}

// 即使保留了历史配置，未安装或缺少安装状态的应用也不能显示设置入口。
for (const installed of [false, undefined]) {
  const app = createSSRApp(Providers, {
    ...props,
    cliTargets: [
      { id: "claude", name: "Claude", installed },
      { id: "codex", name: "Codex", installed },
      { id: "claude-desktop", name: "Claude Desktop", installed }
    ],
    providers: [{ id: "saved-provider", cli: "claude", name: "历史供应商" }],
    runtimeConfigSchemas: { claude: schema, codex: schema }
  })
  app.component("ElInput", { render: () => null })
  const html = await renderToString(app)
  assert.ok(html.includes("尚未检测到已安装的应用"))
  assert.ok(!html.includes("providers-view__toolbar"))
  assert.ok(!html.includes("历史供应商"))
}

const mixedApp = createSSRApp(Providers, {
  ...props,
  cliTargets: [
    { id: "claude", name: "未安装的 Claude", installed: false },
    { id: "codex", name: "已安装的 Codex", installed: true },
    { id: "claude-desktop", name: "未安装的 Desktop", installed: false },
    { id: "unsupported", name: "不支持配置的应用", installed: true }
  ],
  runtimeConfigSchemas: { claude: schema, codex: schema }
})
mixedApp.component("ElInput", { render: () => null })
const mixedHtml = await renderToString(mixedApp)
assert.ok(mixedHtml.includes("已安装的 Codex"))
assert.ok(!mixedHtml.includes("未安装的 Claude"))
assert.ok(!mixedHtml.includes("未安装的 Desktop"))
assert.ok(!mixedHtml.includes("不支持配置的应用"))
assert.ok(mixedHtml.includes("providers-view__cli-tab--active"))

const activeProvider = {
  id: "active-provider",
  cli: "codex",
  name: "Active Provider",
  type: "openai",
  runtimeConfig: { modelList: ["active-model"], mainModel: "active-model" }
}
const editedProvider = {
  id: "edited-provider",
  cli: "codex",
  name: "Edited Provider",
  type: "openai",
  runtimeConfig: { modelList: ["edited-model"], mainModel: "edited-model" }
}
let bindings
const providerSaves = []
const Editor = {
  ...Providers,
  setup(props, context) {
    bindings = Providers.setup(props, context)
    bindings.editProvider(editedProvider)
    return bindings
  }
}
const editor = createSSRApp(Editor, {
  ...props,
  cliTargets: [{ id: "codex", name: "Codex", installed: true }],
  runtimeConfigSchemas: { codex: schema },
  onSaveProvider: payload => providerSaves.push(payload),
  providers: [activeProvider, editedProvider],
  runtimeProfiles: [
    { cli: "codex", providerId: activeProvider.id, model: "active-model" }
  ]
})
const failures = []
editor.component("ElInput", { render: () => null })
editor.config.errorHandler = error => failures.push(error)
const editorHtml = await renderToString(editor)
assert.deepEqual(failures, [])
assert.ok(editorHtml.includes("同步模型列表"))
assert.ok(editorHtml.includes("恢复默认模型"))
assert.ok(editorHtml.includes('class="providers-model-sync"'))
assert.equal(bindings.draft.syncModelList, false)
bindings.modelDrafts.modelList = "edited-new-model"
bindings.modelDrafts.mainModel = "edited-new-model"
await bindings.submitProvider()
assert.equal(providerSaves[0].id, editedProvider.id)
assert.equal(providerSaves[0].runtimeConfig.modelList, "edited-new-model")
assert.equal(providerSaves[0].runtimeConfig.syncModelList, false)
bindings.editProvider({
  ...editedProvider,
  runtimeConfig: { ...editedProvider.runtimeConfig, syncModelList: true }
})
assert.equal(bindings.draft.syncModelList, true)
await bindings.submitProvider()
assert.equal(providerSaves[1].runtimeConfig.syncModelList, true)
bindings.resetCodexDraftModels()
assert.equal(bindings.modelDrafts.modelList, "")
assert.equal(providerSaves.length, 2)
assert.deepEqual(activeProvider.runtimeConfig.modelList, ["active-model"])
bindings.clearDraft()
assert.equal(bindings.draft.syncModelList, false)

let authBindings
const Auth = {
  ...Providers,
  setup(props, context) {
    authBindings = Providers.setup(props, context)
    authBindings.showCodexLoginModal.value = true
    authBindings.codexLoginTab.value = "json"
    return authBindings
  }
}
const authApp = createSSRApp(Auth, {
  ...props,
  cliTargets: [{ id: "codex", name: "Codex", installed: true }],
  runtimeConfigSchemas: { codex: schema }
})
authApp.component("ElInput", { render: () => null })
authApp.config.errorHandler = error => failures.push(error)
const authHtml = await renderToString(authApp)
assert.ok(authHtml.includes('href="https://chatgpt.com/api/auth/session"'))
assert.ok(authHtml.includes("复制地址"))
assert.equal(openedUrls.length, 0)
await authBindings.openExternalLink(authBindings.codexSessionUrl)
await authBindings.copyAuthUrl(authBindings.codexSessionUrl)
assert.deepEqual(openedUrls, ["https://chatgpt.com/api/auth/session"])
assert.deepEqual(copiedUrls, ["https://chatgpt.com/api/auth/session"])
assert.deepEqual(failures, [])
assert.deepEqual(modelFetchErrors, [])

// 使用真实组件进行离线渲染，验证 Claude 列表、手输模型和供应商隔离。
const claudeProvider = {
  id: "claude-provider",
  cli: "claude",
  name: "Claude Provider",
  type: "anthropic",
  baseUrl: "https://example.com",
  authField: "ANTHROPIC_API_KEY",
  activeApiKeyId: "saved-key",
  apiKeys: [{ id: "saved-key", name: "已保存的 Key", masked: "sk-***" }],
  runtimeConfig: {
    modelList: ["saved-model"],
    mainModel: "custom-main",
    haikuModel: "custom-haiku",
    sonnetModel: "custom-sonnet",
    opusModel: "custom-opus"
  }
}
const otherClaudeProvider = {
  ...claudeProvider,
  id: "other-claude-provider",
  name: "Other Claude Provider",
  runtimeConfig: { modelList: ["other-model"], mainModel: "other-model" }
}
let claudeBindings
const ClaudeEditor = {
  ...Providers,
  setup(props, context) {
    claudeBindings = Providers.setup(props, context)
    claudeBindings.editProvider(claudeProvider)
    return claudeBindings
  }
}
const claudeSaves = []
const claudeApp = createSSRApp(ClaudeEditor, {
  ...props,
  cliTargets: [{ id: "claude", name: "Claude", installed: true }],
  runtimeConfigSchemas: {
    claude: {
      ...schema,
      defaultProviderType: "anthropic",
      authFields: ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"],
      modelFields: [
        { key: "mainModel", label: "主模型" },
        { key: "haikuModel", label: "Haiku 默认模型" },
        { key: "sonnetModel", label: "Sonnet 默认模型" },
        { key: "opusModel", label: "Opus 默认模型" }
      ]
    }
  },
  providers: [claudeProvider, otherClaudeProvider],
  onSaveProvider: payload => claudeSaves.push(payload)
})
claudeApp.component("ElInput", { render: () => null })
const { ID_INJECTION_KEY, ZINDEX_INJECTION_KEY } = require("element-plus")
claudeApp.provide(ID_INJECTION_KEY, { prefix: 1, current: 0 })
claudeApp.provide(ZINDEX_INJECTION_KEY, { current: 0 })
claudeApp.config.errorHandler = error => failures.push(error)
const claudeHtml = await renderToString(claudeApp)
assert.deepEqual(failures, [])
assert.ok(claudeHtml.includes("获取上游模型"))
assert.equal((claudeHtml.match(/class="[^"]*providers-model-select/g) || []).length, 4)
assert.ok(!claudeHtml.includes("同步到 Codex"))
assert.ok(!claudeHtml.includes("同步模型列表"))
assert.deepEqual(claudeBindings.claudeModelOptions.value, [
  "saved-model", "custom-main", "custom-haiku", "custom-sonnet", "custom-opus"
])
fetchClaudeModelsResult = async () => ({ models: ["claude-a", "claude-b"] })
await claudeBindings.fetchClaudeModels()
assert.equal(modelFetchCalls.at(-1).providerId, claudeProvider.id)
assert.equal(modelFetchCalls.at(-1).keyId, "saved-key")
assert.equal(modelFetchCalls.at(-1).authField, "ANTHROPIC_API_KEY")
assert.equal(modelFetchCalls.at(-1).apiKey, "")
assert.equal(claudeBindings.modelDrafts.modelList, "claude-a\nclaude-b")
assert.equal(claudeBindings.modelDrafts.mainModel, "custom-main")
assert.equal(claudeBindings.modelDrafts.haikuModel, "custom-haiku")
assert.equal(claudeBindings.modelDrafts.sonnetModel, "custom-sonnet")
assert.equal(claudeBindings.modelDrafts.opusModel, "custom-opus")
claudeBindings.modelDrafts.mainModel = "manually-entered-model"
await claudeBindings.submitProvider()
assert.equal(claudeSaves.at(-1).runtimeConfig.mainModel, "manually-entered-model")
assert.equal(claudeSaves.at(-1).runtimeConfig.modelList, "claude-a\nclaude-b")
claudeBindings.editProvider({
  ...claudeProvider,
  runtimeConfig: { ...claudeSaves.at(-1).runtimeConfig, modelList: ["claude-a", "claude-b"] }
})
assert.ok(claudeBindings.claudeModelOptions.value.includes("claude-b"))
assert.equal(claudeBindings.modelDrafts.mainModel, "manually-entered-model")
claudeBindings.editProvider(otherClaudeProvider)
assert.deepEqual(claudeBindings.claudeModelOptions.value, ["other-model"])

let finishModelFetch
fetchClaudeModelsResult = () => new Promise(resolve => { finishModelFetch = resolve })
claudeBindings.editProvider(claudeProvider)
const pendingModelFetch = claudeBindings.fetchClaudeModels()
assert.equal(claudeBindings.claudeModelsPending.value, true)
const savesBeforePending = claudeSaves.length
await claudeBindings.submitProvider()
assert.equal(claudeSaves.length, savesBeforePending)
claudeBindings.editProvider(otherClaudeProvider)
finishModelFetch({ models: ["late-model"] })
await pendingModelFetch
assert.equal(claudeBindings.claudeModelsPending.value, false)
assert.deepEqual(claudeBindings.claudeModelOptions.value, ["other-model"])

fetchClaudeModelsResult = async () => { throw new Error("模拟上游获取失败") }
await claudeBindings.fetchClaudeModels()
assert.equal(claudeBindings.modelDrafts.modelList, "other-model")
assert.deepEqual(modelFetchErrors, ["Error: 模拟上游获取失败"])
claudeBindings.clearDraft()
assert.deepEqual(claudeBindings.claudeModelOptions.value, [])
claudeBindings.modelDrafts.mainModel = "manual-model-without-list"
await claudeBindings.submitProvider()
assert.equal(claudeSaves.at(-1).runtimeConfig.mainModel, "manual-model-without-list")
assert.equal(claudeSaves.at(-1).runtimeConfig.modelList, "")
claudeBindings.clearDraft()
fetchClaudeModelsResult = async () => ({ models: ["new-provider-model"] })
await claudeBindings.fetchClaudeModels()
assert.equal(modelFetchCalls.at(-1).providerId, undefined)
assert.equal(claudeBindings.modelDrafts.mainModel, "")
claudeBindings.modelDrafts.mainModel = "custom-without-upstream"
claudeBindings.modelDrafts.sonnetModel = "new-provider-model"
await claudeBindings.submitProvider()
assert.equal(claudeSaves.at(-1).id, undefined)
assert.equal(claudeSaves.at(-1).runtimeConfig.modelList, "new-provider-model")
assert.equal(claudeSaves.at(-1).runtimeConfig.mainModel, "custom-without-upstream")
assert.equal(claudeSaves.at(-1).runtimeConfig.sonnetModel, "new-provider-model")
assert.deepEqual(claudeProvider.runtimeConfig.modelList, ["saved-model"])
assert.deepEqual(failures, [])
console.log(
  "Providers 安装状态、Codex 同步、Claude 模型列表与手动输入、保存隔离、请求竞态及获取失败回归通过"
)

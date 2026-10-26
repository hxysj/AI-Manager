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
const providerApi = api
const openedUrls = []
const copiedUrls = []
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { clipboard: { writeText: async value => copiedUrls.push(value) } }
})
const localRequire = name => {
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
      createMessage: { success() {}, error: error => assert.fail(error) }
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
console.log(
  "Providers 安装状态筛选、空状态、首屏、同步选项默认关闭与保存隔离、认证链接打开及复制回归通过"
)

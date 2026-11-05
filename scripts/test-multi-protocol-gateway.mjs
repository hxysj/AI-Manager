import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import {
  compileScript,
  compileStyleAsync,
  compileTemplate,
  parse
} from "@vue/compiler-sfc"
import { transformSync } from "esbuild"
import * as vue from "vue"
import { renderToString } from "@vue/server-renderer"

const require = createRequire(import.meta.url)

async function compileComponent(filename, id, localRequire) {
  const source = readFileSync(filename, "utf8")
  const { descriptor, errors } = parse(source, { filename })
  assert.deepEqual(errors, [])
  assert.equal(descriptor.styles[0].lang, "less")
  assert.equal(descriptor.styles[0].scoped, true)
  assert.ok(!/font-weight|@media/.test(descriptor.styles[0].content))
  const script = compileScript(descriptor, { id, genDefaultAs: "__component" })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename,
    id,
    compilerOptions: { bindingMetadata: script.bindings }
  })
  assert.deepEqual(template.errors, [])
  const style = await compileStyleAsync({
    source: descriptor.styles[0].content,
    filename,
    id: `data-v-${id}`,
    scoped: true,
    preprocessLang: "less"
  })
  assert.deepEqual(style.errors, [])
  const module = { exports: {} }
  const code = transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
    { format: "cjs" }
  ).code
  new Function("require", "module", "exports", code)(
    localRequire,
    module,
    module.exports
  )
  return { component: module.exports.default, css: style.code }
}

// 直接执行真实 setup，模拟管理接口，验证草稿与按需读取的密钥不会互相污染。
let mounted
const tokenSecret = "sk-test-0123456789abcdef0123456789ab"
let tokenReadGate
let tokenReadError
let saveConfigError
let logDetailGate
let logDetailError
const detailFixture = {
  id: "request-one",
  createdAt: Date.now(),
  clientIp: "192.168.1.28",
  requestPath: "/v1/chat/completions",
  protocol: "chat",
  upstreamProtocol: "responses",
  providerName: "Antigravity",
  requestBody: JSON.stringify({
    model: "alias",
    messages: [{ role: "user", content: "你好 <script>原样文本</script>" }]
  }),
  upstreamRequestBody: JSON.stringify({
    model: "real-model",
    input: [{ role: "user", content: "你好" }]
  }),
  upstreamResponseBody:
    'event: response.output_text.delta\ndata: {"delta":"响应中文🙂"}\n\n',
  responseBody: JSON.stringify({
    choices: [{ message: { content: "响应中文🙂" } }]
  })
}
let persisted = {
  enabled: false,
  host: "127.0.0.1",
  port: 8046,
  baseUrl: "http://127.0.0.1:8046",
  providerIds: ["provider", "google"],
  modelMappings: {
    alias: { targetId: "provider", model: "real-model" },
    "google-alias": { targetId: "google", model: "google-real-model" }
  },
  tokens: [],
  usage: {}
}
const calls = []
const api = {
  getState: async () => structuredClone(persisted),
  getLogDetail: async (payload) => {
    calls.push(["log-detail", payload])
    if (logDetailGate) await logDetailGate
    if (logDetailError) throw logDetailError
    return { ...structuredClone(detailFixture), id: payload.logId }
  },
  createToken: async (payload) => {
    calls.push(["create", payload])
    persisted.tokens.push({
      id: "token-one",
      name: payload.name,
      tokenHint: "sk-test-…",
      maskedKey: "sk-test-••••••••••••",
      canReveal: true,
      enabled: true
    })
    return {
      ...structuredClone(persisted),
      createdToken: { id: "token-one", token: tokenSecret }
    }
  },
  getTokenValue: async (payload) => {
    calls.push(["token-value", payload])
    if (tokenReadGate) await tokenReadGate
    if (tokenReadError) throw tokenReadError
    const token = persisted.tokens.find((token) => token.id === payload.tokenId)
    if (!token?.canReveal)
      throw new Error("旧 Token 仅保存了校验值，无法还原 API Key，请重新创建")
    return { apiKey: tokenSecret }
  },
  saveConfig: async (payload) => {
    calls.push(["save", payload])
    if (saveConfigError) throw saveConfigError
    persisted = { ...persisted, ...payload }
    return structuredClone(persisted)
  },
  enable: async (payload) => {
    calls.push(["enable", payload])
    persisted = { ...persisted, ...payload, enabled: true }
    return structuredClone(persisted)
  },
  disable: async () => {
    persisted.enabled = false
    return structuredClone(persisted)
  },
  setTokenEnabled: async (payload) => {
    calls.push(["set-enabled", payload])
    return structuredClone(persisted)
  },
  deleteToken: async (payload) => {
    calls.push(["delete", payload])
    persisted.tokens = persisted.tokens.filter(
      (token) => token.id !== payload.tokenId
    )
    return structuredClone(persisted)
  }
}
const messages = []
let connectionComponent
let modalComponent
const localRequire = (name) => {
  if (name === "vue")
    return {
      ...vue,
      onMounted: (callback) => {
        mounted = callback
      }
    }
  if (name === "@/api")
    return {
      multiProtocolGatewayApi: api,
      usageApi: {
        getUsagePricing: async () => ({
          status: "ok",
          data: { exchangeRate: 7.2, items: [] }
        })
      }
    }
  if (name === "@/utils/message")
    return {
      createMessage: {
        success() {},
        error: (message) => messages.push(message)
      }
    }
  if (name === "@/utils/formatters")
    return {
      formatDateTime: String,
      formatTokenCount: (value) => String(value || 0)
    }
  if (name.endsWith("/GatewayConnectionPanel.vue"))
    return { __esModule: true, default: connectionComponent }
  if (name.endsWith("/BaseModal.vue"))
    return { __esModule: true, default: modalComponent }
  if (name.endsWith(".vue")) return { __esModule: true, default: {} }
  return require(name)
}
const connection = await compileComponent(
  "src/features/tools/multiProtocolGateway/components/GatewayConnectionPanel.vue",
  "connection-check",
  localRequire
)
connectionComponent = connection.component
const modal = await compileComponent(
  "src/components/BaseModal.vue",
  "modal-check",
  localRequire
)
modalComponent = modal.component
assert.ok(
  connection.css.includes(
    ".gateway-connection .protocol-panel .protocol-cards .protocol-card .endpoint-list .endpoint-item .endpoint-picker .endpoint-path"
  )
)
assert.ok(
  connection.css.includes(
    ".gateway-connection .integration-panel .integration-body .model-browser .model-table-wrap .model-table .model-body .model-row .model-cell .model-picker .model-name"
  )
)
const gateway = await compileComponent(
  "src/features/tools/multiProtocolGateway/index.vue",
  "gateway-check",
  localRequire
)
assert.ok(
  gateway.css.includes(
    ".gateway-tool .modal-form .pool-content .target-list .target-item .target-info .target-name"
  )
)
const setup = gateway.component.setup(
  {
    providers: [
      { id: "provider", name: "普通 Provider", enabled: true, cli: "claude" },
      { id: "google", type: "google-account", enabled: true },
      { id: "disabled", enabled: false }
    ],
    codexAccounts: [
      { id: "official", email: "test@example.com" },
      { id: "off", disabled: true }
    ]
  },
  { expose() {} }
)
await mounted()
const setupProxy = vue.proxyRefs(setup)
let renderGatewayTree
await renderToString(
  vue.createSSRApp({
    render() {
      renderGatewayTree = vue.withCtx(() =>
        gateway.component.render({}, [], {}, setupProxy)
      )
      return renderGatewayTree()
    }
  })
)
function gatewayNodes(node = renderGatewayTree()) {
  if (!node || typeof node !== "object") return []
  const children = Array.isArray(node.children)
    ? node.children
    : typeof node.children?.default === "function"
      ? node.children.default()
      : []
  return [node, ...children.flatMap(gatewayNodes)]
}
async function renderGateway() {
  return renderToString(
    vue.createSSRApp({ ...gateway.component, setup: () => setup })
  )
}
function poolModal() {
  const node = gatewayNodes().find(
    (node) => node.type === modalComponent && node.props?.title === "底层池"
  )
  assert.ok(node, "找不到底层池弹框")
  return node
}
function poolNode(predicate) {
  const node = gatewayNodes(poolModal()).find(predicate)
  assert.ok(node, "找不到底层池弹框中的控件")
  return node
}
function openPool() {
  const node = gatewayNodes().find(
    (node) => node.props?.title === "选择参与反代的 Provider 与官方账号"
  )
  assert.ok(node, "找不到底层池入口")
  return node.props.onClick()
}
function selectPool(providerIds) {
  poolNode((node) => node.props?.class === "target-checkbox").props[
    "onUpdate:modelValue"
  ](providerIds)
}
function savePool() {
  return poolNode((node) => node.props?.class === "modal-form").props.onSubmit({
    preventDefault() {}
  })
}
assert.equal(setup.activeTab.value, "connections")
assert.deepEqual(
  setup.tabs.map((tab) => tab.id),
  ["connections", "tokens", "logs"]
)
const initialHtml = await renderGateway()
assert.ok(initialHtml.includes("支持模型与集成"))
assert.ok(!initialHtml.includes("gateway-metrics"))
assert.ok(!initialHtml.includes("target-list"))
assert.ok(!initialHtml.includes("配置与模型"))
for (const entry of ["添加模型", "编辑支持模型", "导入模型", "模型配置"]) {
  assert.ok(!initialHtml.includes(entry))
}
assert.deepEqual(
  setup.targets.value.map((target) => target.id),
  ["provider", "google", "account:official"]
)
setup.draft.port = 8047
assert.equal(setup.draft.modelMappings.alias.model, "real-model")
setup.tokenDraft.name = "调用方"
await setup.createToken()
assert.deepEqual(setup.tokenValues, {})
assert.ok(!Object.hasOwn(setup.gateway.value, "createdToken"))
assert.equal(setup.draft.port, 8047)
assert.equal(setup.draft.modelMappings.alias.model, "real-model")
await setup.saveConfig()
assert.equal(
  calls.find((call) => call[0] === "save")[1].modelMappings.alias.model,
  "real-model"
)
await setup.toggleGateway()
assert.equal(setup.gateway.value.enabled, true)
await setup.setTokenEnabled({ id: "token-one", enabled: true })
assert.deepEqual(calls.at(-1), [
  "set-enabled",
  { tokenId: "token-one", enabled: false }
])
setup.deletingToken.value = { id: "token-one" }
await setup.deleteToken()
assert.equal(setup.deletingToken.value, null)
assert.deepEqual(calls.at(-1), ["delete", { tokenId: "token-one" }])
assert.deepEqual(messages, [])

// 底层池先编辑草稿，取消不提交；保存失败不污染配置，也不会清掉仍选中来源的路由。
const savesBeforePool = calls.filter(
  ([operation]) => operation === "save"
).length
openPool()
assert.equal(setup.showPoolModal.value, true)
assert.deepEqual(setup.poolProviderIds.value, ["provider", "google"])
assert.equal(
  gatewayNodes(poolModal()).filter(
    (node) => node.props?.class === "target-checkbox"
  ).length,
  3
)
assert.ok((await renderGateway()).includes("test@example.com"))
selectPool(["google"])
assert.deepEqual(setup.draft.providerIds, ["provider", "google"])
poolNode(
  (node) =>
    node.type === "button" &&
    (typeof node.children === "string"
      ? node.children.trim() === "取消"
      : node.children?.some?.((child) => child.children?.trim?.() === "取消"))
).props.onClick()
assert.equal(setup.showPoolModal.value, false)
assert.equal(
  calls.filter(([operation]) => operation === "save").length,
  savesBeforePool
)
openPool()
assert.deepEqual(setup.poolProviderIds.value, ["provider", "google"])
selectPool(["google"])
saveConfigError = new Error("底层池保存失败")
await savePool()
assert.equal(messages.pop(), "底层池保存失败")
assert.equal(setup.busy.value, false)
assert.equal(setup.showPoolModal.value, true)
assert.deepEqual(setup.poolProviderIds.value, ["google"])
assert.deepEqual(setup.draft.providerIds, ["provider", "google"])
assert.equal(setup.draft.modelMappings.alias.model, "real-model")
saveConfigError = null
await savePool()
assert.equal(setup.showPoolModal.value, false)
assert.deepEqual(setup.draft.providerIds, ["google"])
assert.deepEqual(setup.draft.modelMappings, {
  "google-alias": { targetId: "google", model: "google-real-model" }
})
assert.deepEqual(calls.at(-1)[1].modelMappings, setup.draft.modelMappings)
openPool()
selectPool([])
await savePool()
assert.deepEqual(setup.draft.providerIds, [])
assert.deepEqual(setup.draft.modelMappings, {})
assert.equal(setup.showPoolModal.value, false)
assert.deepEqual(messages, [])
for (const [tab, expected] of [
  ["connections", "支持模型与集成"],
  ["tokens", "创建 Token"],
  ["logs", "最近请求"]
]) {
  setup.activeTab.value = tab
  const app = vue.createSSRApp({ ...gateway.component, setup: () => setup })
  const failures = []
  app.config.errorHandler = (error) => failures.push(error)
  const html = await renderToString(app)
  assert.deepEqual(failures, [])
  assert.ok(html.includes(expected))
  if (tab === "logs") {
    assert.ok(html.includes("用量调度全景"))
    assert.ok(html.includes("缓存节省效能"))
    assert.ok(html.includes("IP 访问统计"))
    assert.ok(html.includes("用户按调用 Token 统计"))
    assert.ok(html.includes("缓存"))
  }
  assert.equal(html.includes("Multi-Protocol Support"), tab === "connections")
  assert.ok(!html.includes(tokenSecret))
}

// 使用真实模板的事件处理器，验证选择、复制与示例联动，而非复制一套页面逻辑。
const panelProps = vue.reactive({
  baseUrl: "http://127.0.0.1:9000///",
  enabled: true,
  models: [
    { id: "public-model", owned_by: "provider" },
    { id: "second-model", owned_by: "普通 Provider" }
  ],
  modelMappings: {
    "public-model": { targetId: "provider", model: "real-model" }
  },
  targets: [{ id: "provider", name: "普通 Provider" }]
})
const emitted = []
const panelSetup = connectionComponent.setup(panelProps, {
  expose() {},
  emit: (name) => emitted.push(name)
})
const panelProxy = vue.proxyRefs(panelSetup)
// 保留 Vue 渲染上下文，在读取模板事件时也能正确处理 v-model 指令。
let renderPanelTree
await renderToString(
  vue.createSSRApp({
    render() {
      renderPanelTree = vue.withCtx(() =>
        connectionComponent.render({}, [], panelProps, panelProxy)
      )
      return renderPanelTree()
    }
  })
)
function panelNodes(node = renderPanelTree()) {
  if (!node || typeof node !== "object") return []
  return [
    node,
    ...(Array.isArray(node.children) ? node.children.flatMap(panelNodes) : [])
  ]
}
function panelNode(className) {
  const node = panelNodes().find(
    (node) =>
      typeof node.props?.class === "string" &&
      node.props.class.split(" ").includes(className)
  )
  assert.ok(node, `找不到 ${className}`)
  return node
}
function clickLabel(label) {
  const node = panelNodes().find((node) => node.props?.["aria-label"] === label)
  assert.ok(node, `找不到按钮 ${label}`)
  return node.props.onClick({ stopPropagation() {} })
}
async function renderPanel() {
  return renderToString(
    vue.createSSRApp(
      { ...connectionComponent, setup: () => panelSetup },
      panelProps
    )
  )
}
assert.equal(panelSetup.modelId.value, "public-model")
assert.equal(panelSetup.modelEntries.value[0].targetName, "普通 Provider")
assert.equal(panelSetup.modelEntries.value[0].realModel, "real-model")
const copied = []
const navigatorDescriptor = Object.getOwnPropertyDescriptor(
  globalThis,
  "navigator"
)
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { clipboard: { writeText: async (text) => copied.push(text) } }
})
try {
  // 请求正文由详情接口读取，支持 IP 搜索、四段内容、原文复制和异步关闭。
  persisted.logs = [
    {
      id: detailFixture.id,
      createdAt: detailFixture.createdAt,
      clientIp: detailFixture.clientIp,
      protocol: "chat",
      ok: true,
      statusCode: 200
    }
  ]
  await setup.load()
  setup.activeTab.value = "logs"
  assert.ok((await renderGateway()).includes("192.168.1.28"))
  setup.logSearch.value = "192.168.1.28"
  assert.equal(setup.filteredLogs.value.length, 1)
  setup.logSearch.value = "203.0.113.1"
  assert.equal(setup.filteredLogs.value.length, 0)
  setup.logSearch.value = ""
  const detailButton = gatewayNodes().find(
    (node) => node.props?.["aria-label"] === "查看请求 request-one 的详情"
  )
  assert.ok(detailButton)
  await detailButton.props.onClick()
  assert.equal(setup.viewingRequest.value.id, "request-one")
  assert.equal(setup.requestDetailLoading.value, false)
  assert.deepEqual(calls.at(-1), ["log-detail", { logId: "request-one" }])
  assert.equal(
    setup.requestDetailBody.value,
    JSON.stringify(JSON.parse(detailFixture.responseBody), null, 2)
  )
  const detailModal = () =>
    gatewayNodes().find(
      (node) => node.type === modalComponent && node.props?.title === "请求详情"
    )
  for (const section of setup.requestDetailSections) {
    const button = gatewayNodes(detailModal()).find(
      (node) =>
        node.props?.class?.includes?.("request-detail-tab") &&
        node.children?.trim?.() === section.label
    )
    assert.ok(button)
    button.props.onClick()
    assert.equal(setup.requestDetailTab.value, section.id)
    assert.ok(setup.requestDetailBody.value.length > 0)
  }
  setup.requestDetailTab.value = "requestBody"
  const detailHtml = await renderGateway()
  assert.ok(detailHtml.includes("&lt;script&gt;"))
  assert.ok(!detailHtml.includes("<script>原样文本"))
  const copyDetail = gatewayNodes(detailModal()).find(
    (node) =>
      node.type === "button" &&
      Array.isArray(node.children) &&
      node.children.some(
        (child) =>
          typeof child.children === "string" &&
          child.children.includes("复制当前内容")
      )
  )
  assert.ok(copyDetail)
  await copyDetail.props.onClick()
  assert.equal(copied.at(-1), setup.requestDetailBody.value)
  setup.closeRequestDetail()
  assert.equal(setup.viewingRequest.value, null)
  // 独立保存延迟，模拟关闭弹框后接口才返回。
  let releaseDetail
  logDetailGate = new Promise((resolve) => {
    releaseDetail = resolve
  })
  const pendingDetail = setup.openRequestDetail({ id: "request-old" })
  assert.equal(setup.requestDetailLoading.value, true)
  setup.closeRequestDetail()
  releaseDetail()
  await pendingDetail
  logDetailGate = null
  assert.equal(setup.viewingRequest.value, null)
  logDetailError = new Error("请求详情读取失败")
  await setup.openRequestDetail({ id: "missing" })
  assert.equal(setup.requestDetailError.value, "请求详情读取失败")
  assert.equal(setup.requestDetailLoading.value, false)
  logDetailError = null
  setup.closeRequestDetail()

  // 列表默认遮挡完整 Key；复制直接取得原文，预览可收起且不进入网关公共状态。
  setup.tokenDraft.name = "预览调用方"
  await setup.createToken()
  setup.activeTab.value = "tokens"
  const token = setup.gateway.value.tokens[0]
  function keyButton(label) {
    const node = gatewayNodes().find(
      (node) => node.props?.["aria-label"] === label
    )
    assert.ok(node, `找不到密钥按钮 ${label}`)
    return node
  }
  assert.ok((await renderGateway()).includes(token.maskedKey))
  assert.ok(!(await renderGateway()).includes(tokenSecret))
  await keyButton("复制 预览调用方 的完整 API Key").props.onClick()
  assert.equal(copied.at(-1), tokenSecret)
  assert.deepEqual(setup.tokenValues, {})
  await keyButton("预览 预览调用方 的 API Key").props.onClick()
  assert.equal(setup.tokenValues[token.id], tokenSecret)
  assert.ok((await renderGateway()).includes(tokenSecret))
  assert.ok(!JSON.stringify(setup.gateway.value).includes(tokenSecret))
  await keyButton("隐藏 预览调用方 的 API Key").props.onClick()
  assert.deepEqual(setup.tokenValues, {})
  assert.ok(!(await renderGateway()).includes(tokenSecret))
  await setup.toggleTokenPreview(token)
  await setup.load()
  assert.deepEqual(setup.tokenValues, {})
  await setup.copyToken(token)
  assert.equal(copied.at(-1), tokenSecret)

  let finishRead
  tokenReadGate = new Promise((resolve) => {
    finishRead = resolve
  })
  const pending = setup.toggleTokenPreview(token)
  assert.equal(keyButton("预览 预览调用方 的 API Key").props.disabled, true)
  setup.activeTab.value = "logs"
  finishRead()
  await pending
  tokenReadGate = null
  assert.deepEqual(setup.tokenValues, {})
  setup.activeTab.value = "tokens"
  tokenReadError = new Error("读取失败")
  await setup.copyToken(token)
  assert.equal(messages.pop(), "读取失败")
  assert.deepEqual(setup.tokenValues, {})
  tokenReadError = null
  persisted.tokens.push({
    id: "legacy",
    name: "旧密钥",
    maskedKey: "mpg_old••••••••",
    canReveal: false
  })
  await setup.load()
  assert.equal(keyButton("复制 旧密钥 的完整 API Key").props.disabled, true)
  assert.equal(keyButton("预览 旧密钥 的 API Key").props.disabled, true)
  assert.ok((await renderGateway()).includes("旧 Token 仅保存校验值"))
  assert.deepEqual(messages, [])

  for (const protocol of panelSetup.protocols) {
    await clickLabel(`复制 ${protocol.name} BASE`)
    assert.equal(copied.at(-1), `http://127.0.0.1:9000${protocol.basePath}`)
    for (const endpoint of protocol.endpoints) {
      const node = panelNodes().find(
        (node) =>
          node.props?.class?.includes("endpoint-picker") &&
          node.children?.some(
            (child) =>
              child.props?.class === "endpoint-path" &&
              child.children === endpoint.path
          )
      )
      assert.ok(node)
      node.props.onClick()
      assert.equal(panelSetup.selectedProtocol.value, protocol.id)
      assert.equal(panelSetup.activeEndpoint.value.id, endpoint.id)
      await clickLabel(`复制 ${endpoint.name} 地址`)
      assert.equal(
        copied.at(-1),
        `http://127.0.0.1:9000${endpoint.path.replace("{model}", "public-model")}`
      )
      assert.equal(
        panelSetup.modelsPath.value,
        protocol.id === "gemini" ? "/v1beta/models" : "/v1/models"
      )
      for (const language of ["python", "curl", "powershell"]) {
        panelNode("language-select").props["onUpdate:modelValue"](language)
        const snippet = panelSetup.snippet.value
        const header =
          protocol.id === "openai"
            ? "Authorization"
            : protocol.id === "anthropic"
              ? "x-api-key"
              : "x-goog-api-key"
        assert.ok(snippet.includes(header))
        assert.ok(snippet.includes("AI_MANAGER_TOKEN"))
        assert.ok(!snippet.includes(tokenSecret))
        assert.ok(!snippet.includes("YOUR_MODEL"))
        const body =
          language === "python"
            ? JSON.parse(snippet.match(/payload = ([\s\S]+?)\n\nrequest =/)[1])
            : language === "powershell"
              ? JSON.parse(snippet.match(/\$body = @'\n([\s\S]+?)\n'@/)[1])
              : JSON.parse(snippet.match(/--data-raw '([\s\S]+)'$/)[1])
        if (protocol.id === "gemini")
          assert.equal(body.contents[0].parts[0].text, "你好")
        else {
          assert.equal(body.model, "public-model")
          if (endpoint.id === "responses") assert.equal(body.input, "你好")
          else assert.equal(body.messages[0].content, "你好")
          if (protocol.id === "anthropic") assert.equal(body.max_tokens, 1024)
        }
        await panelNode("code-copy").props.onClick()
        assert.equal(copied.at(-1), snippet)
        const html = await renderPanel()
        assert.ok(html.includes("服务运行中"))
        assert.ok(html.includes("快速集成"))
      }
    }
  }
  await panelNode("models-copy").props.onClick()
  assert.equal(copied.at(-1), "http://127.0.0.1:9000/v1beta/models")
  await clickLabel("复制模型 second-model")
  assert.equal(copied.at(-1), "second-model")
  const secondModel = panelNodes().find(
    (node) =>
      node.props?.class === "model-picker" &&
      node.children?.some(
        (child) =>
          child.props?.class === "model-name" &&
          child.children === "second-model"
      )
  )
  secondModel.props.onClick()
  assert.equal(panelSetup.modelId.value, "second-model")
  assert.ok(
    panelSetup
      .endpointUrl(panelSetup.activeEndpoint.value)
      .includes("second-model")
  )
  panelNode("token-link").props.onClick()
  assert.deepEqual(emitted, ["manage-tokens"])

  // 特殊别名保持转义，且模型删除、空池、停服后不会继续展示过期选择或状态。
  const specialId = "model/中文'\"$`<img src=x onerror=alert(1)>\n"
  panelProps.models.push({ id: specialId })
  panelSetup.selectedModelId.value = specialId
  assert.ok(
    panelSetup
      .endpointUrl(panelSetup.activeEndpoint.value)
      .includes(encodeURIComponent(specialId))
  )
  const specialHtml = await renderPanel()
  assert.ok(specialHtml.includes("&lt;img"))
  assert.ok(!specialHtml.includes("<img src=x"))
  panelSetup.language.value = "curl"
  assert.ok(panelSetup.snippet.value.includes("'\\''"))
  panelSetup.language.value = "powershell"
  assert.ok(panelSetup.snippet.value.includes("''"))
  panelProps.models = [{ id: "replacement" }]
  assert.equal(panelSetup.modelId.value, "replacement")
  panelProps.models = []
  panelProps.enabled = false
  assert.equal(panelSetup.modelId.value, "")
  assert.equal(panelNode("code-copy").props.disabled, true)
  const emptyHtml = await renderPanel()
  assert.ok(emptyHtml.includes("暂时没有可用模型"))
  assert.ok(emptyHtml.includes("点击顶部「底层池」"))
  assert.ok(!emptyHtml.includes("配置与模型"))
  assert.ok(emptyHtml.includes("服务未开启"))
  assert.ok(panelSetup.snippet.value.includes("YOUR_MODEL"))
  assert.ok(!emptyHtml.includes("服务运行中"))
  panelProps.baseUrl = ""
  assert.equal(panelSetup.base.value, "http://127.0.0.1:8046")
  assert.deepEqual(messages, [])
} finally {
  if (navigatorDescriptor)
    Object.defineProperty(globalThis, "navigator", navigatorDescriptor)
  else delete globalThis.navigator
}
console.log(
  "多协议反代：Vue、嵌套 scoped Less、三个页签、底层池弹框与保存、配置草稿、Token、协议模型联动、15 组示例与复制交互验证通过"
)

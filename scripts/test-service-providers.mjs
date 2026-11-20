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
import { createRenderer, createSSRApp, h, nextTick, reactive, ref } from "vue"
import { renderToString } from "@vue/server-renderer"

const require = createRequire(import.meta.url)
const formatterModule = { exports: {} }
new Function(
  "module",
  "exports",
  transformSync(readFileSync("src/utils/formatters.js", "utf8"), {
    format: "cjs"
  }).code
)(formatterModule, formatterModule.exports)
const files = [
  "src/App.vue",
  "src/components/AppSidebar.vue",
  "src/components/QuickSwitchPanel.vue",
  "src/features/providers/components/ServiceProviderCenter.vue",
  "src/features/providers/components/ServiceGatewayManager.vue",
  "src/features/providers/index.vue",
  "src/features/providers/components/CodexAccountLoginModal.vue",
  "src/features/providers/components/GoogleAccountModal.vue",
  "src/features/providers/components/ProviderIconPicker.vue",
  "src/features/providers/components/ProviderModelListEditor.vue",
  "src/features/providers/components/ServiceAgentModelFields.vue",
  "src/features/providers/components/ServiceOfficialAccountOverview.vue",
  "src/features/providers/components/ServiceProviderOverview.vue",
  "src/features/tools/index.vue",
  "src/features/assistant/components/AssistantModelSelect.vue",
  "src/features/settings/components/TranslationAgentSettings.vue",
  "src/features/settings/components/TranslationConfigModal.vue",
  "src/features/settings/index.vue",
  "src/features/settings/components/SystemSettings.vue",
  "src/features/usage/index.vue",
  "src/components/TokenCount.vue"
]
let centerCode
let settingsCode
const selectorComponents = []
const editorComponents = new Map()
for (const filename of files) {
  const { descriptor, errors } = parse(readFileSync(filename, "utf8"), {
    filename
  })
  assert.deepEqual(errors, [], filename)
  const script = compileScript(descriptor, {
    id: "service-provider-test",
    genDefaultAs: "__component"
  })
  const template = compileTemplate({
    source: descriptor.template.content,
    filename,
    id: "service-provider-test",
    compilerOptions: { bindingMetadata: script.bindings }
  })
  assert.deepEqual(template.errors, [], filename)
  transformSync(script.content, { format: "esm" })
  for (const style of descriptor.styles) {
    const result = await compileStyleAsync({
      source: style.content,
      filename,
      id: "service-provider-test",
      scoped: style.scoped,
      preprocessLang: style.lang
    })
    assert.deepEqual(result.errors, [], filename)
  }
  if (filename.endsWith("ServiceProviderCenter.vue")) {
    centerCode = transformSync(
      `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
      { format: "cjs" }
    ).code
  } else if (filename.endsWith("SystemSettings.vue")) {
    settingsCode = transformSync(
      `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
      { format: "cjs" }
    ).code
  } else if (
    /ProviderIconPicker|ProviderModelListEditor|ServiceAgentModelFields|ServiceGatewayManager|ServiceOfficialAccountOverview|ServiceProviderOverview|CodexAccountLoginModal|GoogleAccountModal/.test(
      filename
    )
  ) {
    editorComponents.set(
      filename.split("/").pop(),
      transformSync(
        `${script.content}\n${template.code}\n__component.render = render\nexport default __component`.replaceAll(
          "import.meta.glob",
          "globAssets"
        ),
        { format: "cjs" }
      ).code
    )
  } else if (
    /AssistantModelSelect|TranslationAgentSettings|TranslationConfigModal/.test(
      filename
    )
  ) {
    selectorComponents.push({
      filename,
      code: transformSync(`${script.content}\nexport default __component`, {
        format: "cjs"
      }).code
    })
  }
}

// 使用真实配置中心的 setup、模板和保存逻辑，隔离桌面 IPC 与文件写入。
let state = {
  backend: "service",
  baseUrl: "http://127.0.0.1:15724",
  providers: [
    {
      id: "p",
      name: "统一服务商",
      icon: "alibabacloud.svg",
      kind: "api",
      enabled: 1,
      hasCredential: 1,
      revision: 7,
      upstreamProtocol: "responses",
      baseUrl: "https://example.test/v1",
      proxy: ""
    }
  ],
  models: [
    {
      id: "m",
      providerId: "p",
      upstreamModelId: "upstream-model",
      displayName: "测试模型",
      enabled: 1,
      inputPriceMicros: 2000000,
      catalog: { custom: true }
    }
  ],
  bindings: [
    {
      providerId: "p",
      agentType: "codex",
      enabled: 1,
      defaultModelId: "m",
      options: { serviceTierFast: true, modelList: [] }
    }
  ],
  keys: [],
  instances: [
    {
      id: "external",
      agentType: "gateway",
      name: "外部工具",
      configPath: "external:one",
      status: "active"
    },
    {
      id: "internal",
      agentType: "codex",
      name: "内置会话",
      configPath: "internal:one",
      status: "active"
    }
  ],
  managedFiles: [],
  routes: [
    {
      agentType: "codex",
      providerId: "p",
      clientModelName: "alias",
      modelId: "m",
      enabled: 1
    }
  ],
  usage: {
    requests: { requestCount: 2, successCount: 1 },
    attempts: { attemptCount: 1, measuredCount: 0 },
    byProvider: [
      {
        groupKey: "p",
        groupName: "统一服务商",
        currency: "CNY",
        attemptCount: 1,
        successCount: 1,
        unknownUsageCount: 1,
        pricedCount: 0,
        estimatedCostMicros: null
      }
    ],
    logs: [
      {
        id: "log",
        state: "unknown",
        inputTokens: null,
        outputTokens: null,
        estimatedCostMicros: null
      }
    ]
  }
}
const calls = []
const messages = []
let failState = false
let failSave = false
let detailOverride = {}
let quotaHandler
let googleStateHandler
let loginHandler
const invoke = async (action, payload) => {
  calls.push({ action, payload })
  if (action === "login" && loginHandler) return loginHandler(payload)
  if (action === "refresh-quota" && quotaHandler) return quotaHandler(payload)
  if (action === "google-login-state" && googleStateHandler)
    return googleStateHandler()
  if (["save", "configure-agent"].includes(action) && failSave)
    throw new Error("配置已被修改，请刷新后重试")
  if (action === "state") {
    if (failState) throw new Error("状态读取失败")
    return state
  }
  if (action === "detail")
    return {
      baseUrl: state.providers[0].baseUrl,
      proxy: "",
      upstreamProtocol: "responses",
      headers: { "x-test": "yes" },
      revision: state.providers[0].revision,
      ...detailOverride
    }
  if (action === "fetch-models") {
    state = {
      ...state,
      providers: state.providers.map((item) =>
        item.id === payload.providerId
          ? { ...item, revision: item.revision + 1 }
          : item
      ),
      models: [
        ...state.models,
        {
          id: "fetched",
          providerId: payload.providerId,
          upstreamModelId: "fetched-model",
          enabled: 1
        }
      ]
    }
  }
  if (action === "switch-backend" && payload.backend === "legacy") {
    state = {
      ...state,
      managedFiles: [
        {
          id: "file",
          instanceId: "external",
          restoreStatus: "conflict",
          errorMessage: "文件被外部修改"
        }
      ]
    }
    throw new Error("恢复文件冲突")
  }
  if (action === "save") return { id: payload.id || `new-${payload.kind}` }
  if (action === "google-login")
    return { status: "pending", message: "等待授权" }
  if (action === "login")
    return { status: "pending", authUrl: "https://auth.example.test/authorize" }
  if (action === "import-account") return { id: payload.id || "new-official" }
  return {}
}
const localRequire = (name) => {
  if (name.endsWith("/style/css")) return {}
  if (editorModules.has(name.split("/").pop()))
    return editorModules.get(name.split("/").pop())
  if (name === "@/api/modules/service-providers")
    return { serviceProviderApi: { invoke } }
  if (name === "@/api/modules/accounts")
    return {
      googleAccountApi: {
        login: (payload) => invoke("legacy-google-login", payload),
        state: () => invoke("legacy-google-state"),
        cancel: () => invoke("legacy-google-cancel")
      }
    }
  if (name === "@/api/modules/system")
    return {
      systemApi: { openExternal: (payload) => invoke("open-external", payload) }
    }
  if (name === "@/utils/formatters") return formatterModule.exports
  if (name === "@/constants/reasoning-efforts")
    return require("../src/constants/reasoning-efforts.js")
  if (name === "@/utils/message")
    return {
      createMessage: {
        success: (message) => messages.push(message),
        error: (message) => messages.push(message)
      }
    }
  // 记录传入已有 AiIcon 的图标配置，验证列表没有再写死 Agent 图标。
  if (name === "@/components/AiIcon.vue")
    return {
      __esModule: true,
      default: {
        props: ["name", "alt"],
        render() {
          return h("span", {
            "data-provider-icon": this.name,
            "aria-label": this.alt
          })
        }
      }
    }
  if (name.endsWith(".vue")) {
    return {
      __esModule: true,
      default: {
        inheritAttrs: false,
        render() {
          return this.$slots.default?.()
        }
      }
    }
  }
  return require(name)
}
const editorModules = new Map()
for (const [filename, code] of editorComponents) {
  const module = { exports: {} }
  new Function("require", "module", "exports", "globAssets", code)(
    localRequire,
    module,
    module.exports,
    () => ({
      "/src/assets/ai-icons/alibabacloud.svg": "",
      "/src/assets/ai-icons/openai.svg": ""
    })
  )
  editorModules.set(filename, module.exports)
}
const module = { exports: {} }
new Function("require", "module", "exports", centerCode)(
  localRequire,
  module,
  module.exports
)
const Center = module.exports.default
const runtimeConfigSchemas = {
  codex: {
    optionFields: [
      {
        key: "modelContextWindowEnabled",
        label: "1M 上下文窗口",
        type: "boolean"
      },
      {
        key: "modelAutoCompactTokenLimit",
        label: "压缩阈值",
        type: "number",
        dependsOn: "modelContextWindowEnabled"
      },
      { key: "serviceTierFast", label: "开启 Fast 模式", type: "boolean" },
      {
        key: "modelReasoningEffort",
        label: "思考强度",
        type: "select",
        options: ["low", "medium", "high", "xhigh"]
      }
    ]
  },
  claude: {
    modelFields: [
      { key: "mainModel", label: "主模型" },
      { key: "haikuModel", label: "Haiku 默认模型" },
      { key: "sonnetModel", label: "Sonnet 默认模型" },
      { key: "opusModel", label: "Opus 默认模型" }
    ]
  }
}
const cliTargets = [
  {
    id: "codex",
    installed: true,
    icon: "codex.svg",
    configPath: "D:/test-codex",
    version: "test"
  },
  { id: "claude", installed: true, configPath: "D:/test-claude" },
  { id: "claude-desktop", installed: false }
]
let bindings
const app = createSSRApp(
  {
    ...Center,
    setup(props, context) {
      bindings = Center.setup(props, context)
      return bindings
    }
  },
  {
    initialState: state,
    runtimeConfigSchemas,
    cliTargets
  }
)
const failures = []
app.config.errorHandler = (error) => failures.push(error)
const html = await renderToString(app)
assert.deepEqual(failures, [])
assert.ok(html.includes("Agent 列表") && html.includes("agent-card"))
assert.ok(html.includes("Codex") && html.includes("Claude Code"))
assert.ok(!html.includes('aria-label="配置 Claude Desktop"'))
assert.ok(html.includes('aria-label="配置 Codex"'))
assert.ok(
  !html.includes('aria-label="新增服务商"') && !html.includes("显示停用项")
)
assert.ok(!html.includes("provider-avatar") && !html.includes("内置会话"))
assert.ok(
  !readFileSync("src/components/AppSidebar.vue", "utf8").includes(
    "Detected CLI"
  )
)
const renderCenter = async (initialState, configure) => {
  const app = createSSRApp(
    {
      ...Center,
      setup(props, context) {
        const value = Center.setup(props, context)
        value.providerTab.value = "api"
        configure?.(value)
        return value
      }
    },
    { initialState, runtimeConfigSchemas, cliTargets }
  )
  return renderToString(app)
}
const providerHtml = await renderCenter(state)
assert.ok(
  providerHtml.includes("统一服务商") &&
    providerHtml.includes("provider-avatar")
)
assert.ok(
  providerHtml.includes("管理 API Key") && providerHtml.includes("查看详情")
)
assert.ok(providerHtml.includes('data-provider-icon="alibabacloud.svg"'))
assert.ok(
  providerHtml.includes('aria-label="新增服务商"') &&
    providerHtml.includes("显示停用项")
)
assert.ok(
  !providerHtml.includes("state-pill") && !providerHtml.includes("Agent 配置")
)
const managerHtml = await renderCenter(state, (value) => {
  value.showGatewayManager.value = true
})
assert.ok(
  managerHtml.includes("Agent 接管") &&
    managerHtml.includes("调用统计") &&
    managerHtml.includes("尚无 Agent 接管记录")
)
assert.ok(
  !managerHtml.includes("内置会话") && !managerHtml.includes("外部工具")
)
assert.ok(
  managerHtml.includes("配置 Codex") && !managerHtml.includes("测试来源")
)
assert.ok(
  !managerHtml.includes("外部调用") && !managerHtml.includes("生成调用 Token")
)
assert.ok(
  !managerHtml.includes("本地共享网关") && !managerHtml.includes("文件冲突")
)
const usageManagerHtml = await renderCenter(state, (value) => {
  value.openGatewayManager(state.providers[0])
  value.gatewayTab.value = "usage"
})
assert.ok(
  usageManagerHtml.includes("未知") &&
    usageManagerHtml.includes("已确认用量") &&
    usageManagerHtml.includes("汇总维度") &&
    usageManagerHtml.includes("最近请求")
)
const diagnosticsManagerHtml = await renderCenter(state, (value) => {
  value.openGatewayManager(state.providers[0])
  value.gatewayTab.value = "diagnostics"
})
assert.ok(
  diagnosticsManagerHtml.includes("暂无迁移记录") &&
    diagnosticsManagerHtml.includes("暂无兼容性测试记录") &&
    diagnosticsManagerHtml.includes("测试来源") &&
    diagnosticsManagerHtml.includes("兼容性测试")
)
const testOnlyHtml = await renderCenter(state, (value) => {
  value.gatewayTab.value = "diagnostics"
  value.openCompatibilityTest(state.providers[0])
})
assert.ok(testOnlyHtml.includes("测试工具与流式兼容"))
assert.ok(
  !testOnlyHtml.includes("备份并应用") &&
    !testOnlyHtml.includes("生成调用 Token") &&
    !testOnlyHtml.includes("配置目录")
)
// 官方账号显示邮箱，类型筛选和禁用项筛选只影响列表，不改变后端授权。
const official = {
  id: "account:internal-id",
  name: "account:internal-id",
  kind: "codex_account",
  enabled: 1,
  hasCredential: 1
}
const disabled = {
  id: "disabled",
  name: "已禁用服务商",
  kind: "api",
  enabled: 0
}
const listState = {
  ...state,
  providers: [...state.providers, official, disabled],
  accounts: [
    {
      providerId: official.id,
      email: "official@example.test",
      quota: {
        rate_limit: {
          primary_window: { used_percent: 25, limit_window_seconds: 18000 }
        }
      }
    }
  ]
}
const listHtml = await renderCenter(listState, (value) => {
  value.providerTab.value = "codex_account"
})
assert.ok(
  listHtml.includes("official@example.test") && listHtml.includes("75%")
)
assert.ok(
  !listHtml.includes("account:internal-id") &&
    !listHtml.includes("已禁用服务商")
)
// 官方授权失败显示在对应账号，接口 JSON 不进入页面，重新授权仍指向原账号。
const expiredFailure =
  'OpenAI 请求失败：401 {"error":{"message":"Your authentication token has expired. Please try refreshing it.","code":"token_expired"},"status":401}'
const expiredState = {
  ...listState,
  accounts: listState.accounts.map((account) => ({
    ...account,
    refreshStatus: "failed",
    quota: {
      ...account.quota,
      refreshError: { message: expiredFailure, statusCode: 401 }
    }
  }))
}
const expiredHtml = await renderCenter(expiredState, (value) => {
  value.providerTab.value = "codex_account"
})
assert.ok(expiredHtml.includes("登录凭据已过期，请重新授权。"))
assert.ok(expiredHtml.includes("HTTP 401"))
assert.ok(expiredHtml.includes('class="btn provider-reauth-button"'))
assert.ok(!expiredHtml.includes("token_expired"))
const expiredDetailHtml = await renderCenter(expiredState, (value) => {
  value.openProviderDetail(official)
})
assert.ok(expiredDetailHtml.includes("授权已失效"))
assert.ok(expiredDetailHtml.includes("重新授权"))
assert.ok(expiredDetailHtml.includes("登录凭据已过期，请重新授权。"))
const namedOfficial = { ...official, name: "GPT 工作账号" }
const namedOfficialHtml = await renderCenter(
  { ...listState, providers: [namedOfficial] },
  (value) => {
    value.providerTab.value = "codex_account"
    assert.equal(value.providerName(official.id), "GPT 工作账号")
  }
)
assert.ok(namedOfficialHtml.includes('title="GPT 工作账号"'))
assert.ok(!namedOfficialHtml.includes("account:internal-id"))
const googleNamedState = {
  ...listState,
  providers: [
    {
      id: "google-storage-id",
      name: "google-storage-id",
      kind: "google_account",
      enabled: 1,
      hasCredential: 1
    }
  ],
  googleAccounts: [
    {
      providerId: "google-storage-id",
      subject: "google-subject",
      email: "google@example.test",
      metadata: { name: "Google 工作账号" }
    }
  ]
}
const googleNamedHtml = await renderCenter(googleNamedState, (value) => {
  value.providerTab.value = "google_account"
  assert.equal(value.providerName("google-storage-id"), "Google 工作账号")
})
assert.ok(googleNamedHtml.includes('title="Google 工作账号"'))
assert.ok(!googleNamedHtml.includes("google-storage-id"))
const accountHtml = await renderCenter(listState, (value) => {
  value.providerTab.value = "codex_account"
})
assert.ok(
  accountHtml.includes("official@example.test") &&
    !accountHtml.includes("统一服务商")
)
const officialDetailHtml = await renderCenter(listState, (value) => {
  value.openProviderDetail(official)
})
assert.ok(officialDetailHtml.includes("使用官方模型列表"))
assert.ok(!officialDetailHtml.includes("获取模型"))
assert.ok(!officialDetailHtml.includes("尚未配置模型"))
// 官方详情使用公开账号资料，零额度和未知额度必须明确区分。
const accountDetails = {
  ...listState,
  providers: [
    {
      ...official,
      createdAt: 1794873600000,
      updatedAt: 1794960000000,
      note: "团队工作账号"
    }
  ],
  accounts: [
    {
      ...listState.accounts[0],
      accountId: "official-account-id",
      expiresAt: 1795046400000,
      quota: {
        plan_type: "plus",
        rate_limit: {
          primary_window: {
            used_percent: 25,
            limit_window_seconds: 18000,
            reset_at: 4102444800
          },
          secondary_window: { used_percent: 100, limit_window_seconds: 604800 }
        }
      }
    }
  ],
  instances: [
    {
      id: "official-codex",
      providerId: official.id,
      agentType: "codex",
      configPath: "D:/official-codex",
      status: "active",
      syncStatus: "synced"
    }
  ]
}
const richOfficialHtml = await renderCenter(accountDetails, (value) => {
  value.openProviderDetail(official)
})
for (const text of [
  "official-account-id",
  "Plus",
  "75%",
  "0%",
  "5 小时额度",
  "周额度",
  "2100",
  "访问令牌到期",
  "创建时间",
  "团队工作账号",
  "官方未提供重置时间",
  "官方额度与网关请求用量独立统计"
])
  assert.ok(richOfficialHtml.includes(text), text)
assert.ok(richOfficialHtml.includes('class="official-agent-tag"'))
assert.ok(!richOfficialHtml.includes("quota-json"))
const waitingOfficialHtml = await renderCenter(
  {
    ...listState,
    providers: [{ ...official, hasCredential: 0 }],
    accounts: []
  },
  (value) => value.openProviderDetail(official)
)
assert.ok(waitingOfficialHtml.includes("登录后查看官方额度"))
assert.ok(waitingOfficialHtml.includes("套餐待获取"))
assert.ok(!waitingOfficialHtml.includes("official-quota-number"))
const unknownQuotaHtml = await renderCenter(
  {
    ...listState,
    accounts: [
      {
        providerId: official.id,
        quota: {
          rate_limit: { primary_window: { limit_window_seconds: 18000 } }
        }
      }
    ]
  },
  (value) => value.openProviderDetail(official)
)
assert.ok(unknownQuotaHtml.includes('aria-valuetext="额度未知"'))
assert.ok(!unknownQuotaHtml.includes('aria-valuenow="0"'))
const googleOfficial = {
  ...official,
  id: "google-official",
  name: "Google 工作账号",
  kind: "google_account"
}
const googleQuotaState = {
  ...listState,
  providers: [googleOfficial],
  googleAccounts: [
    {
      providerId: googleOfficial.id,
      email: "google@example.test",
      subject: "google-subject-id",
      metadata: {
        plan: "g1-pro-tier",
        project: "cloud-project-id",
        status: "reauth_required",
        message: "Google 授权已失效，请重新登录",
        updatedAt: 1794960000000,
        quotaGroups: [
          {
            displayName: "共享分组",
            buckets: [
              {
                window: "WINDOW_5H",
                remainingFraction: 0,
                resetTime: "2100-01-01T00:00:00Z"
              },
              { window: "WINDOW_WEEKLY", remainingFraction: null }
            ]
          }
        ]
      }
    }
  ]
}
const googleDetailHtml = await renderCenter(googleQuotaState, (value) => {
  value.openProviderDetail(googleOfficial)
  value.refreshingQuotaIds.value.add(googleOfficial.id)
})
for (const text of [
  "google@example.test",
  "google-subject-id",
  ">Pro<",
  "cloud-project-id",
  "需重新登录",
  "登录授权已失效",
  "共享分组 · 5 小时额度",
  "共享分组 · 周额度",
  "0%",
  "官方未返回数值",
  "刷新中"
])
  assert.ok(googleDetailHtml.includes(text), text)
assert.ok(googleDetailHtml.includes('aria-valuenow="0"'))
assert.ok(googleDetailHtml.includes('aria-valuetext="额度未知"'))
assert.ok(!googleDetailHtml.includes("导入 auth.json"))
assert.ok(!googleDetailHtml.includes("获取模型"))
const googleListHtml = await renderCenter(googleQuotaState, (value) => {
  value.providerTab.value = "codex_account"
})
assert.ok(googleListHtml.includes("共享分组"))
assert.ok(googleListHtml.includes("5 小时额度"))
assert.ok(googleListHtml.includes('aria-valuenow="0"'))
assert.ok(googleListHtml.includes('aria-valuetext="额度未知"'))
assert.ok(!googleListHtml.includes("provider-card-refreshing"))
const googleLoadingHtml = await renderCenter(
  { ...googleQuotaState, googleAccounts: [] },
  (value) => {
    value.providerTab.value = "codex_account"
    value.refreshingQuotaIds.value.add(googleOfficial.id)
  }
)
assert.ok(googleLoadingHtml.includes("quota-placeholder"))
assert.ok(!googleLoadingHtml.includes("provider-card-refreshing"))
assert.ok(!googleLoadingHtml.includes("icon-button-refreshing"))
assert.ok(googleLoadingHtml.includes("正在获取分组额度…"))
const apiDetailHtml = await renderCenter(state, (value) => {
  value.openProviderDetail(state.providers[0])
})
assert.ok(apiDetailHtml.includes("OpenAI 兼容服务商"))
assert.ok(apiDetailHtml.includes("https://example.test/v1"))
assert.ok(
  apiDetailHtml.includes("获取模型") && apiDetailHtml.includes("测试模型")
)
assert.ok(!apiDetailHtml.includes("official-overview"))
// 服务商概览按当前来源展示目录与 Key，未知价格不能显示成免费。
const richApiState = {
  ...state,
  providers: [
    {
      ...state.providers[0],
      upstreamProtocol: "chat",
      createdAt: 1794873600000,
      updatedAt: 1794960000000,
      note: "研发团队共享服务商"
    }
  ],
  models: [
    {
      ...state.models[0],
      contextWindow: 131072,
      inputPriceMicros: 0,
      cachedInputPriceMicros: null,
      outputPriceMicros: 2000000,
      currency: "USD"
    },
    {
      id: "disabled-model",
      providerId: "p",
      upstreamModelId: "disabled-model-id",
      displayName: "停用模型",
      enabled: 0,
      inputPriceMicros: 3500000,
      currency: "CNY"
    },
    {
      id: "other-model",
      providerId: "other",
      upstreamModelId: "other-provider-model",
      enabled: 1
    }
  ],
  keys: [
    {
      id: "revoked",
      providerId: "p",
      name: "已撤销 Key",
      status: "revoked",
      isActive: 1,
      fingerprint: "revoked-fingerprint"
    },
    {
      id: "primary",
      providerId: "p",
      name: "生产 Key",
      status: "active",
      isActive: 1,
      fingerprint: "primary-fingerprint"
    },
    {
      id: "backup",
      providerId: "p",
      name: "备用 Key",
      status: "active",
      isActive: 0
    },
    {
      id: "other",
      providerId: "other",
      name: "其他来源 Key",
      status: "active",
      isActive: 1
    }
  ],
  instances: [
    {
      id: "api-codex",
      providerId: "p",
      agentType: "codex",
      configPath: "D:/api-codex",
      status: "active",
      syncStatus: "synced"
    }
  ]
}
const richApiHtml = await renderCenter(richApiState, (value) => {
  value.openProviderDetail(richApiState.providers[0])
})
for (const text of [
  "Chat Completions",
  "连接配置",
  "凭据与使用",
  "生产 Key",
  "primary-fingerprint",
  "2 / 3 个",
  "131072 Tokens",
  "0 USD",
  "2 USD",
  "3.5 CNY",
  "未配置",
  "停用模型",
  "disabled-model-id",
  "每 100 万 Tokens",
  "研发团队共享服务商"
])
  assert.ok(richApiHtml.includes(text), text)
assert.ok(richApiHtml.includes('class="service-agent-tag"'))
assert.ok(!richApiHtml.includes("revoked-fingerprint"))
assert.ok(!richApiHtml.includes("其他来源 Key"))
assert.ok(!richApiHtml.includes("other-provider-model"))
// 全局费用只作为展示投影，服务商编辑仍保留空报价。
const inheritedApiState = {
  ...state,
  models: [
    {
      ...state.models[0],
      inputPriceMicros: null,
      cachedInputPriceMicros: null,
      outputPriceMicros: null,
      currency: "USD",
      effectivePricing: {
        source: "global",
        inputPriceMicros: 7500000,
        cachedInputPriceMicros: 750000,
        outputPriceMicros: 15000000,
        currency: "CNY"
      }
    }
  ]
}
const inheritedApiHtml = await renderCenter(inheritedApiState, (value) =>
  value.openProviderDetail(inheritedApiState.providers[0])
)
for (const text of ["继承全局费用", "7.5 CNY", "0.75 CNY", "15 CNY"]) {
  assert.ok(inheritedApiHtml.includes(text), text)
}
assert.ok(!inheritedApiHtml.includes("7.5 USD"))
assert.equal(inheritedApiState.models[0].inputPriceMicros, null)
const emptyApiHtml = await renderCenter(
  {
    ...state,
    providers: [{ ...state.providers[0], hasCredential: 0, enabled: 0 }],
    models: [],
    keys: [],
    instances: []
  },
  (value) => value.openProviderDetail(state.providers[0])
)
for (const text of [
  "待配置 API Key",
  "尚未配置模型",
  "暂无 Agent 使用",
  "0 / 0 个",
  "已停用"
])
  assert.ok(emptyApiHtml.includes(text), text)
assert.ok(!emptyApiHtml.includes('class="service-model-table"'))
const customIcon = "data:image/png;base64,aWNvbg=="
const customIconHtml = await renderCenter(
  {
    ...listState,
    providers: [{ ...official, icon: customIcon }]
  },
  (value) => {
    value.providerTab.value = "codex_account"
  }
)
assert.ok(customIconHtml.includes(`data-provider-icon="${customIcon}"`))
const noIconHtml = await renderCenter({
  ...state,
  providers: [{ ...state.providers[0], icon: "" }]
})
assert.ok(!noIconHtml.includes("data-provider-icon"))
const disabledHtml = await renderCenter(listState, (value) => {
  value.showDisabledItems.value = true
})
assert.ok(disabledHtml.includes("已禁用服务商"))
assert.ok(
  !disabledHtml.includes("state-pill") && disabledHtml.includes("恢复可用")
)
const keysHtml = await renderCenter(state, (value) => {
  value.openProviderDetail(state.providers[0], "keys")
})
assert.ok(
  keysHtml.includes("添加 / 更新 Key") && keysHtml.includes("尚未添加 API Key")
)
// Key 列表只展示当前来源，撤销和不可读取的凭据不能被误标为使用中。
const detailKeysHtml = await renderCenter(
  {
    ...richApiState,
    keys: [
      ...richApiState.keys,
      {
        id: "unreadable",
        providerId: "p",
        name: "待重新录入 Key",
        status: "unreadable",
        isActive: 0
      }
    ]
  },
  (value) => value.openProviderDetail(state.providers[0], "keys")
)
const keyRows = [
  ...detailKeysHtml.matchAll(
    /<tr[^>]*class="detail-key-row[^"]*"[^>]*>[\s\S]*?<\/tr>/g
  )
].map((match) => match[0])
for (const name of ["生产 Key", "已撤销 Key", "待重新录入 Key"]) {
  const row = keyRows.find((item) => item.includes(name))
  assert.ok(
    /<button[^>]*disabled[^>]*>[\s\S]*?使用此 Key/.test(row),
    `${name} 不可重复启用或选择无效凭据`
  )
}
assert.ok(
  !/<button[^>]*disabled[^>]*>[\s\S]*?使用此 Key/.test(
    keyRows.find((item) => item.includes("备用 Key"))
  )
)
const revokedRow = keyRows.find((item) => item.includes("已撤销 Key"))
assert.ok(revokedRow.includes("已撤销") && !revokedRow.includes("使用中"))
assert.ok(detailKeysHtml.includes("无法读取"))
assert.ok(!detailKeysHtml.includes("其他来源 Key"))
assert.equal(
  keyRows.filter((item) => item.includes("detail-key-current")).length,
  1
)
// 关联页继续只读真实活动实例，补充协议、同步时间与冲突信息。
const detailAgentsHtml = await renderCenter(
  {
    ...state,
    instances: [
      {
        id: "detail-active",
        name: "本地开发实例",
        providerId: "p",
        agentType: "codex",
        configPath: "D:/detail-codex",
        clientProtocol: "responses",
        status: "active",
        syncStatus: "synced",
        lastSyncedAt: 1794873600000
      },
      {
        id: "detail-pending",
        name: "配置待同步实例",
        providerId: "p",
        agentType: "claude",
        configPath: "D:/detail-claude",
        clientProtocol: "messages",
        status: "active",
        syncStatus: "conflict",
        lastError: "配置已被外部修改"
      },
      ...[
        ["hidden-detached", "p", "D:/detached", "detached"],
        ["hidden-other", "other", "D:/other", "active"],
        ["hidden-internal", "p", "internal:session", "active"],
        ["hidden-test", "p", "test:probe", "active"]
      ].map(([name, providerId, configPath, status]) => ({
        id: name,
        name,
        providerId,
        configPath,
        status,
        agentType: "codex",
        syncStatus: "synced"
      }))
    ]
  },
  (value) => value.openProviderDetail(state.providers[0], "instances")
)
for (const label of [
  "本地开发实例",
  "D:/detail-codex",
  "Responses",
  "Messages",
  "最近同步",
  "已同步",
  "配置冲突",
  "配置已被外部修改",
  "未记录"
]) {
  assert.ok(detailAgentsHtml.includes(label), label)
}
assert.equal(
  (detailAgentsHtml.match(/class="detail-agent-card"/g) || []).length,
  2
)
assert.ok(!detailAgentsHtml.includes("hidden-"))
const emptyAgentsHtml = await renderCenter(state, (value) =>
  value.openProviderDetail(state.providers[0], "instances")
)
assert.ok(
  emptyAgentsHtml.includes("尚未关联 Agent") &&
    emptyAgentsHtml.includes("管理 Agent")
)
await bindings.toggleProviderEnabled(state.providers[0])
const disabledPayload = calls.find((item) => item.action === "save").payload
assert.equal(disabledPayload.enabled, false)
assert.equal(disabledPayload.headers["x-test"], "yes")
assert.equal(disabledPayload.revision, 7)
assert.equal(disabledPayload.models, undefined)
assert.equal(disabledPayload.icon, "alibabacloud.svg")
calls.length = 0
const instance = {
  id: "one",
  providerId: "p",
  agentType: "codex",
  configPath: "D:/test-codex",
  status: "active",
  syncStatus: "synced"
}
const activeHtml = await renderCenter({ ...state, instances: [instance] })
assert.ok(
  activeHtml.includes("running-agent-tag") &&
    !activeHtml.includes("取消使用") &&
    !activeHtml.includes("provider-card-active")
)
const conflictManagerHtml = await renderCenter(
  {
    ...state,
    instances: [{ ...instance, syncStatus: "conflict", status: "error" }],
    managedFiles: [
      {
        id: "conflict-file",
        instanceId: instance.id,
        restoreStatus: "conflict",
        filePath: "D:/test-codex/config.toml",
        errorMessage: "配置已被外部修改"
      }
    ]
  },
  (value) => value.openGatewayManager(state.providers[0])
)
assert.ok(
  conflictManagerHtml.includes("配置冲突") &&
    conflictManagerHtml.includes("重试恢复") &&
    conflictManagerHtml.includes("查看加密备份") &&
    conflictManagerHtml.includes("D:/test-codex/config.toml")
)
bindings.openProviderDetail(state.providers[0], "instances")
bindings.openGatewayManager(state.providers[0])
assert.equal(calls.length, 0)
assert.equal(bindings.detailProviderId.value, "")
assert.equal(bindings.showGatewayManager.value, true)
assert.equal(bindings.managerProviderId.value, "p")
await bindings.perform("detach", { id: instance.id })
assert.deepEqual(calls.find((item) => item.action === "detach").payload, {
  id: "one"
})
calls.length = 0
state = {
  ...state,
  keys: [
    {
      id: "active-key",
      providerId: "p",
      logicalId: "production",
      name: "生产 Key",
      isActive: 1,
      status: "active"
    }
  ]
}
bindings.data.value = state
await bindings.editProvider({ ...state.providers[0], enabled: false })
assert.equal(bindings.form.value.enabled, false)
await bindings.editProvider(state.providers[0])
assert.equal(bindings.form.value.revision, 7)
assert.equal(bindings.form.value.bindings[0].defaultModelName, "upstream-model")
assert.equal(bindings.form.value.keyLogicalId, "production")
assert.equal(bindings.form.value.keyName, "生产 Key")
assert.equal(bindings.form.value.apiKey, "")
assert.equal(bindings.showGatewayManager.value, false)
const editHtml = await renderCenter(state, (value) => {
  value.form.value = bindings.form.value
})
assert.ok(
  editHtml.includes("选择服务商图标") && editHtml.includes("服务商模型目录")
)
assert.ok(
  !editHtml.includes("同步模型列表") && !editHtml.includes("agent-fields")
)
assert.ok(!editHtml.includes("1M 上下文窗口") && !editHtml.includes("思考强度"))
assert.ok(
  !editHtml.includes("Haiku 默认模型") && !editHtml.includes("子 Agent 模型")
)
assert.ok(editHtml.includes("获取上游模型") && editHtml.includes("批量编辑"))
bindings.agentConfig.value = cliTargets[0]
bindings.agentProviderId.value = "p"
await bindings.selectAgentProvider()
assert.equal(bindings.editorSection.value, "agent")
const codexAgentHtml = await renderCenter(state, (value) => {
  value.form.value = bindings.form.value
  value.editorSection.value = "agent"
  value.agentConfig.value = cliTargets[0]
  value.agentProviderId.value = "p"
})
assert.ok(
  codexAgentHtml.includes("保存并应用") &&
    codexAgentHtml.includes("agent-model-fields")
)
assert.ok(
  codexAgentHtml.includes("1M 上下文窗口") &&
    codexAgentHtml.includes("思考强度")
)
for (const forbidden of [
  "选择服务商图标",
  "基础地址",
  "请求头 JSON",
  "模型别名 JSON",
  "获取上游模型",
  "批量编辑"
])
  assert.ok(!codexAgentHtml.includes(forbidden), forbidden)
assert.equal(bindings.form.value.bindings[0].options.syncModelList, true)
assert.equal(bindings.form.value.bindings[0].options.modelList, undefined)
const codexSection = codexAgentHtml
  .slice(codexAgentHtml.indexOf('data-agent="codex"'))
  .split("</fieldset>")[0]
assert.ok(codexSection.includes('title="upstream-model"'))
assert.ok(!codexSection.includes("暂无自定义模型"))
// Codex 只能引用目录，目录维护与获取入口仍在服务商区域。
for (const forbidden of [
  "provider-model-actions",
  "provider-model-remove",
  "provider-model-add",
  "批量编辑",
  "获取上游模型"
])
  assert.ok(!codexSection.includes(forbidden), forbidden)
assert.ok(
  codexSection.includes("服务商模型列表") && codexSection.includes("1 个")
)
assert.ok(codexSection.includes("同步模型列表"))
const officialEditHtml = await renderCenter(state, (value) => {
  value.form.value = { ...bindings.form.value, kind: "codex_account" }
})
assert.ok(officialEditHtml.includes("使用官方模型列表"))
assert.ok(!officialEditHtml.includes("provider-model-editor"))
assert.ok(!officialEditHtml.includes("同步模型列表"))
assert.ok(!officialEditHtml.includes("获取上游模型"))
assert.ok(!officialEditHtml.includes("模型详情 JSON"))
assert.ok(!officialEditHtml.includes("思考强度"))
const officialAgentHtml = await renderCenter(
  {
    ...state,
    providers: [official]
  },
  (value) => {
    value.form.value = { ...bindings.form.value, kind: "codex_account" }
    value.editorSection.value = "agent"
    value.agentConfig.value = cliTargets[0]
    value.agentProviderId.value = "p"
  }
)
assert.ok(
  officialAgentHtml.includes("使用来源") &&
    officialAgentHtml.includes("思考强度")
)
assert.ok(officialAgentHtml.includes("使用官方模型列表"))
for (const forbidden of [
  "provider-model-editor",
  "同步模型列表",
  "获取上游模型",
  "模型详情 JSON"
])
  assert.ok(!officialAgentHtml.includes(forbidden), forbidden)
// 各 Agent 展开自己的规则，保留独立参数并共用服务商模型目录。
bindings.form.value.bindings[0].options.syncModelList = false
bindings.form.value.bindings[1].enabled = true
bindings.form.value.bindings[1].options = {
  haikuModel: "upstream-model",
  sonnetModel: "",
  opusModel: "upstream-model",
  subagentModel: ""
}
bindings.form.value.bindings[2].enabled = true
bindings.form.value.bindings[2].options = {
  modelMappings: {
    "claude-fable-5": {
      model: "upstream-model",
      labelOverride: "自定义 Fable",
      supports1m: true
    }
  }
}
const agentHtml = await renderCenter(
  {
    ...state,
    desktopModelRoutes: [
      { id: "claude-sonnet-4-6", label: "Sonnet" },
      { id: "claude-opus-4-8", label: "Opus" },
      { id: "claude-fable-5", label: "Fable" },
      { id: "claude-haiku-4-5", label: "Haiku" }
    ]
  },
  (value) => {
    value.form.value = bindings.form.value
    value.editorSection.value = "agent"
    value.agentConfig.value = cliTargets[0]
    value.agentProviderId.value = "p"
  }
)
const claudeHtml = await renderCenter(state, (value) => {
  value.form.value = bindings.form.value
  value.editorSection.value = "agent"
  value.agentConfig.value = cliTargets[1]
  value.agentProviderId.value = "p"
})
assert.ok(
  claudeHtml.includes("Haiku 默认模型") && claudeHtml.includes("子 Agent 模型")
)
assert.ok(
  !claudeHtml.includes("同步模型列表") && !claudeHtml.includes("思考强度")
)
const desktopHtml = await renderCenter(
  { ...state, desktopModelRoutes: [{ id: "claude-fable-5", label: "Fable" }] },
  (value) => {
    value.form.value = bindings.form.value
    value.editorSection.value = "agent"
    value.agentConfig.value = { id: "claude-desktop" }
    value.agentProviderId.value = "p"
  }
)
assert.ok(
  desktopHtml.includes("Fable 模型") && desktopHtml.includes("自定义 Fable")
)
bindings.form.value.bindings[2].enabled = false
const AgentFields = editorModules.get("ServiceAgentModelFields.vue").default
const agentProps = reactive({
  modelValue: bindings.form.value.bindings[0],
  models: bindings.draftModels.value.filter((item) => item.enabled),
  schema: runtimeConfigSchemas.codex,
  "onUpdate:modelValue": (value) => {
    agentProps.modelValue = value
    bindings.form.value.bindings[0] = value
  }
})
const agentFields = AgentFields.setup(agentProps, {
  expose() {},
  emit(event, value) {
    if (event === "update:modelValue") agentProps["onUpdate:modelValue"](value)
  }
})
assert.equal(agentFields.modelNames.value, "upstream-model")
bindings.editorSection.value = "provider"
bindings.updateDraftModelNames("upstream-model, shared-added;shared-added")
agentProps.models = bindings.draftModels.value.filter((item) => item.enabled)
assert.equal(agentFields.modelNames.value, "upstream-model\nshared-added")
assert.equal(agentFields.modelNames.value, bindings.draftModelNames.value)
assert.equal(agentProps.modelValue.options.modelList, undefined)
agentFields.updateOption("syncModelList", true)
assert.equal(bindings.form.value.bindings[0].options.syncModelList, true)
agentFields.updateOption("syncModelList", false)
assert.equal(bindings.form.value.bindings[0].options.syncModelList, false)
const ModelListEditor = editorModules.get("ProviderModelListEditor.vue").default
const readonlyEvents = []
const readonlyEditor = ModelListEditor.setup(
  { modelValue: "upstream-model", readonly: true },
  { expose() {}, emit: (...event) => readonlyEvents.push(event) }
)
readonlyEditor.modelInput.value = "should-not-add"
readonlyEditor.addModels()
readonlyEditor.removeModel("upstream-model")
readonlyEditor.resetModels()
assert.deepEqual(readonlyEvents, [])
// 目录条目只发出默认模型选择，禁用或非选择模式均不允许操作。
const selectionProps = reactive({
  modelValue: "upstream-model\nanother-model",
  readonly: true,
  selectable: true,
  disabled: false,
  loading: false
})
const selectionEvents = []
const selectionEditor = ModelListEditor.setup(selectionProps, {
  expose() {},
  emit: (...event) => selectionEvents.push(event)
})
selectionEditor.selectModel("another-model")
assert.deepEqual(selectionEvents, [["update:selectedModel", "another-model"]])
selectionEditor.selectModel("unknown-model")
selectionProps.disabled = true
selectionEditor.selectModel("upstream-model")
selectionProps.disabled = false
selectionProps.selectable = false
selectionEditor.selectModel("upstream-model")
assert.equal(selectionEvents.length, 1)
assert.equal(selectionProps.modelValue, "upstream-model\nanother-model")
const emptyCodexHtml = await renderToString(
  createSSRApp(AgentFields, {
    modelValue: agentProps.modelValue,
    models: []
  })
)
assert.ok(emptyCodexHtml.includes("请在服务商模型目录中配置可用模型。"))
assert.ok(!emptyCodexHtml.includes("获取上游模型"))
const oldReadonlyHtml = await renderToString(
  createSSRApp(ModelListEditor, {
    modelValue: "upstream-model",
    readonly: true
  })
)
assert.ok(!oldReadonlyHtml.includes("provider-model-toolbar"))
assert.ok(!oldReadonlyHtml.includes("同步模型列表"))
const ruleProps = reactive({
  modelValue: {
    agentType: "codex",
    defaultModelName: "gpt-5.6-sol",
    options: { modelReasoningEffort: "ultra" }
  },
  models: [{ upstreamModelId: "gpt-5.6-sol" }],
  schema: runtimeConfigSchemas.codex,
  desktopRoutes: [{ id: "claude-sonnet-4-6" }, { id: "claude-fable-5" }]
})
const ruleFields = AgentFields.setup(ruleProps, {
  expose() {},
  emit(event, value) {
    if (event === "update:modelValue") ruleProps.modelValue = value
  }
})
const effortField = runtimeConfigSchemas.codex.optionFields[3]
assert.ok(ruleFields.optionValues(effortField).includes("ultra"))
ruleFields.updateDefault("ordinary-model")
assert.equal(ruleProps.modelValue.options.modelReasoningEffort, "low")
assert.ok(!ruleFields.optionValues(effortField).includes("ultra"))
ruleFields.updateOption("modelReasoningEffort", "ultra")
ruleFields.updateDefault("")
assert.ok(ruleFields.optionValues(effortField).includes("ultra"))
await nextTick()
ruleProps.models = [{ upstreamModelId: "ordinary-model" }]
await nextTick()
assert.equal(ruleProps.modelValue.options.modelReasoningEffort, "low")
assert.ok(!ruleFields.optionValues(effortField).includes("ultra"))
ruleProps.modelValue = {
  agentType: "claude-desktop",
  options: {
    modelMappings: {
      "claude-fable-5": { model: "upstream-model", supports1m: true }
    }
  }
}
assert.equal(ruleFields.desktopSupports1m("claude-sonnet-4-6"), true)
ruleFields.updateDesktop("claude-sonnet-4-6", "supports1m", false)
assert.equal(ruleFields.desktopSupports1m("claude-sonnet-4-6"), false)
bindings.updateDraftModelNames("new-model")
assert.equal(
  bindings.draftModels.value.find((item) => item.id === "m").enabled,
  false
)
assert.equal(bindings.form.value.bindings[0].defaultModelName, "")
bindings.updateDraftModelNames("upstream-model\nnew-model")
assert.equal(
  bindings.draftModels.value.find((item) => item.id === "m").catalog.custom,
  true
)
bindings.form.value.bindings[0].defaultModelName = "new-model"
bindings.form.value.icon = customIcon
bindings.form.value.apiKey = "replacement-key"
bindings.form.value.bindings.find(
  (item) => item.agentType === "claude"
).enabled = true
await bindings.saveProvider()
const saved = calls.find((item) => item.action === "save").payload
assert.equal(saved.models[0].catalog.custom, true)
assert.equal(saved.models[0].inputPriceMicros, 2000000)
assert.equal(saved.bindings[0].defaultModelName, "new-model")
assert.equal(saved.bindings[0].options.syncModelList, false)
assert.equal(saved.bindings[0].options.modelList, undefined)
assert.equal(saved.bindings[2].enabled, false)
assert.equal(
  saved.bindings[2].options.modelMappings["claude-fable-5"].labelOverride,
  "自定义 Fable"
)
assert.equal(saved.keyLogicalId, "production")
assert.equal(saved.apiKey, "replacement-key")
assert.equal(
  saved.bindings.find((item) => item.agentType === "claude").enabled,
  true
)
assert.equal(saved.routes[0].modelId, "m")
assert.equal(saved.revision, 7)
assert.equal(saved.icon, customIcon)
assert.equal(bindings.form.value, null)
// 单个 Agent 独立选择来源，取消不污染状态，失败保留草稿。
const originalBindings = JSON.stringify(state.bindings)
await bindings.openAgentConfig(cliTargets[0])
assert.equal(bindings.agentProviderId.value, "")
bindings.agentProviderId.value = "p"
await bindings.selectAgentProvider()
bindings.form.value.bindings[0].options.syncModelList = false
bindings.closeAgentConfig()
assert.equal(JSON.stringify(state.bindings), originalBindings)
await bindings.openAgentConfig(cliTargets[0])
bindings.agentProviderId.value = "p"
await bindings.selectAgentProvider()
assert.equal(bindings.form.value.bindings[0].enabled, true)
bindings.form.value.bindings[0].options.syncModelList = false
const agentDraft = bindings.form.value
failSave = true
calls.length = 0
await bindings.saveAgentConfig()
assert.equal(bindings.form.value, agentDraft)
assert.equal(bindings.editorSection.value, "agent")
assert.equal(bindings.busy.value, false)
failSave = false
await bindings.saveAgentConfig()
const savedAgent = calls
  .filter((item) => item.action === "configure-agent")
  .at(-1).payload
assert.deepEqual(savedAgent, {
  agentType: "codex",
  providerId: "p",
  revision: 7,
  defaultModelName: "upstream-model",
  options: agentDraft.bindings[0].options
})
assert.equal(JSON.stringify(state.bindings), originalBindings)
assert.equal(bindings.form.value, null)
assert.equal(bindings.agentConfig.value, null)
await bindings.openAgentConfig(cliTargets[0])
await bindings.saveAgentConfig()
assert.equal(
  calls.filter((item) => item.action === "configure-agent").at(-1).payload
    .providerId,
  ""
)
const sourceHtml = await renderCenter(listState, (value) => {
  value.editorSection.value = "agent"
  value.agentConfig.value = cliTargets[0]
})
assert.ok(!sourceHtml.includes('value="disabled"'))
assert.ok(!sourceHtml.includes("（已禁用）"))
assert.ok(!sourceHtml.includes('label="Google 账号"'))
const uncredentialedSourceHtml = await renderCenter(
  {
    ...listState,
    providers: [
      ...listState.providers,
      {
        id: "uncredentialed",
        name: "待配置",
        kind: "google_account",
        enabled: 1,
        hasCredential: 0
      }
    ]
  },
  (value) => {
    value.agentConfig.value = cliTargets[0]
  }
)
assert.match(uncredentialedSourceHtml, /value="uncredentialed" disabled/)
const optionsHtml = await renderCenter(state, (value) => {
  value.showCreateOptions.value = true
})
for (const label of ["API 服务商", "GPT / Codex 官方账号", "Google 账号"])
  assert.ok(optionsHtml.includes(label))
calls.length = 0
// 模型获取合并新目录，保留尚未保存的模型和价格；连接信息未保存时不调用旧上游。
await bindings.editProvider(state.providers[0])
bindings.updateDraftModelNames("upstream-model\nlocal-draft")
bindings.form.value.name = "尚未保存的名称"
await bindings.fetchEditorModels()
assert.equal(bindings.form.value.name, "尚未保存的名称")
assert.equal(bindings.form.value.revision, 8)
assert.equal(bindings.form.value.bindings[0].options.modelList, undefined)
assert.ok(
  bindings.draftModelNames.value.includes("fetched-model") &&
    bindings.draftModelNames.value.includes("local-draft")
)
assert.equal(
  bindings.draftModels.value.find((item) => item.id === "m").inputPriceMicros,
  2000000
)
const fetchedCodexHtml = await renderToString(
  createSSRApp(AgentFields, {
    modelValue: bindings.form.value.bindings[0],
    models: bindings.draftModels.value.filter((item) => item.enabled),
    schema: runtimeConfigSchemas.codex
  })
)
assert.ok(fetchedCodexHtml.includes('title="fetched-model"'))
assert.ok(fetchedCodexHtml.includes('title="local-draft"'))
for (const agentType of ["claude", "claude-desktop"]) {
  const sharedHtml = await renderToString(
    createSSRApp(AgentFields, {
      modelValue: bindings.form.value.bindings.find(
        (item) => item.agentType === agentType
      ),
      models: bindings.draftModels.value.filter((item) => item.enabled),
      schema: runtimeConfigSchemas[agentType] || {},
      desktopRoutes: [{ id: "claude-fable-5", label: "Fable" }]
    })
  )
  assert.ok(sharedHtml.includes('value="fetched-model"'))
}
bindings.form.value.bindings[0].options.syncModelList = true
calls.length = 0
await bindings.saveProvider()
assert.equal(
  calls.find((item) => item.action === "save").payload.bindings[0].options
    .syncModelList,
  true
)
await bindings.editProvider(state.providers[0])
calls.length = 0
bindings.form.value.baseUrl = "https://unsaved.test/v1"
await bindings.fetchEditorModels()
assert.ok(!calls.some((item) => item.action === "fetch-models"))
bindings.form.value = null
// 新建先保存公共模型目录，再从 Agent 列表选择来源。
await bindings.editProvider()
assert.equal(bindings.editorSection.value, "provider")
assert.equal(bindings.form.value.bindings[0].options.syncModelList, false)
bindings.updateDraftModelNames("brand-new-model")
bindings.form.value.name = "新服务商"
bindings.form.value.baseUrl = "https://new.test/v1"
calls.length = 0
await bindings.saveProvider()
const created = calls.find((item) => item.action === "save").payload
assert.equal(created.models[0].upstreamModelId, "brand-new-model")
assert.ok(created.bindings.every((item) => !item.enabled))
// 官方账号直接进入独立登录面板，开始授权前不创建记录。
calls.length = 0
await bindings.createSource(
  bindings.createSources.find((item) => item.kind === "codex_account")
)
assert.equal(bindings.form.value, null)
assert.equal(bindings.officialLogin.value.kind, "codex_account")
assert.equal(bindings.officialLogin.value.provider, null)
assert.ok(
  !calls.some((item) =>
    ["save", "login", "import-account"].includes(item.action)
  )
)
const codexLoginHtml = await renderCenter(state, (value) =>
  value.openOfficialLogin({ kind: "codex_account" })
)
assert.ok(
  codexLoginHtml.includes("OAuth 授权") && codexLoginHtml.includes("浏览器登录")
)
assert.ok(codexLoginHtml.includes("JSON 数据"))
assert.ok(!codexLoginHtml.includes("provider-form"))
await bindings.startOfficialLogin({
  accountId: "",
  proxy: "http://127.0.0.1:7890"
})
assert.deepEqual(calls.find((item) => item.action === "login").payload, {
  accountId: "",
  proxy: "http://127.0.0.1:7890"
})
assert.equal(bindings.officialLoginState.value.status, "pending")
await bindings.closeOfficialLogin()
assert.ok(calls.some((item) => item.action === "cancel-login"))
assert.equal(bindings.officialLogin.value, null)
assert.ok(!calls.some((item) => item.action === "save"))
bindings.openOfficialLogin(official, "auth")
assert.equal(bindings.officialLogin.value.provider.id, official.id)
assert.equal(bindings.officialLogin.value.tab, "auth")
await bindings.importOfficialAccount({
  accountId: official.id,
  content: '{"tokens":{}}',
  proxy: ""
})
assert.deepEqual(
  calls.find((item) => item.action === "import-account").payload,
  {
    id: official.id,
    content: '{"tokens":{}}',
    proxy: ""
  }
)
assert.equal(bindings.officialLogin.value, null)
await bindings.createSource(
  bindings.createSources.find((item) => item.kind === "google_account")
)
assert.equal(bindings.form.value, null)
assert.equal(bindings.officialLogin.value.kind, "google_account")
const googleLoginHtml = await renderCenter(state, (value) =>
  value.openOfficialLogin({ kind: "google_account" })
)
assert.ok(googleLoginHtml.includes("登录 Google 账号"))
assert.ok(!googleLoginHtml.includes("provider-form"))
calls.length = 0
const GoogleLogin = editorModules.get("GoogleAccountModal.vue").default
let googleBindings
const googleEvents = []
await renderToString(
  createSSRApp({
    setup(props, context) {
      googleBindings = GoogleLogin.setup(
        { serviceMode: true, provider: null },
        {
          ...context,
          emit: (...args) => googleEvents.push(args)
        }
      )
      return () => null
    }
  })
)
await googleBindings.login()
assert.deepEqual(calls.find((item) => item.action === "google-login").payload, {
  providerId: undefined,
  proxy: ""
})
assert.equal(googleBindings.loginState.value.status, "pending")
assert.ok(
  !calls.some(
    (item) => item.action.startsWith("legacy-google-") || item.action === "save"
  )
)
let releaseGoogleState
googleStateHandler = () =>
  new Promise((resolve) => {
    releaseGoogleState = resolve
  })
const pendingGooglePoll = googleBindings.poll()
await googleBindings.cancel()
releaseGoogleState({ status: "pending" })
await pendingGooglePoll
googleStateHandler = undefined
assert.notEqual(googleBindings.loginState.value.status, "pending")
googleStateHandler = async () => ({
  status: "success",
  providerId: "auto-google"
})
await googleBindings.poll()
assert.deepEqual(googleEvents.at(-1), ["created", "auto-google"])
googleStateHandler = undefined
await bindings.closeOfficialLogin()
// Agent 管理不再配置 gateway，即使旧授权存在也不会出现在 Agent 选项中。
assert.ok(!bindings.agents.some((item) => item.id === "gateway"))
bindings.openCompatibilityTest(state.providers[0])
assert.equal(bindings.compatibilityForm.value.modelId, "m")
assert.equal(bindings.showGatewayManager.value, false)
assert.equal(bindings.compatibilityForm.value.agentType, "codex")
assert.equal(bindings.compatibilityForm.value.configPath, undefined)
calls.length = 0
await bindings.perform("test", {
  ...bindings.compatibilityForm.value,
  agentVersion: bindings.agentVersion("codex")
})
assert.ok(calls.some((item) => item.action === "test"))
assert.ok(!calls.some((item) => item.action === "activate"))
bindings.gatewayTab.value = "diagnostics"
bindings.closeCompatibilityTest()
assert.equal(bindings.compatibilityForm.value, null)
assert.equal(bindings.showGatewayManager.value, true)
assert.equal(bindings.gatewayTab.value, "diagnostics")
assert.equal(
  await bindings.perform("switch-backend", { backend: "legacy" }),
  false
)
assert.equal(bindings.data.value.managedFiles[0].restoreStatus, "conflict")
assert.ok(messages.includes("恢复文件冲突"))
failState = true
assert.equal(
  await bindings.perform("switch-backend", { backend: "legacy" }),
  false
)
assert.equal(bindings.busy.value, false)

// 实际挂载验证：官方页并行查询各账号，Agent/服务商页和来源弹框不查询额度。
failState = false
const previousState = state
state = {
  ...previousState,
  providers: [
    ...previousState.providers,
    official,
    {
      id: "google",
      name: "Google",
      kind: "google_account",
      enabled: 1,
      hasCredential: 1
    },
    { ...official, id: "codex-secondary" },
    {
      id: "google-secondary",
      name: "Google 工作账号",
      kind: "google_account",
      enabled: 1,
      hasCredential: 1
    },
    { ...official, id: "disabled-account", enabled: 0 },
    { ...official, id: "uncredentialed-account", hasCredential: 0 }
  ]
}
const testRenderer = createRenderer({
  createElement: () => ({}),
  createComment: () => ({}),
  createText: () => ({}),
  insert() {},
  remove() {},
  parentNode: () => null,
  nextSibling: () => null,
  setText() {},
  setElementText() {},
  patchProp() {}
})
let liveBindings
const liveApp = testRenderer.createApp({
  setup(_, context) {
    liveBindings = Center.setup(
      {
        initialState: state,
        cliTargets,
        runtimeConfigSchemas,
        loginState: null
      },
      context
    )
    return () => null
  }
})
const settle = async () => {
  await nextTick()
  await new Promise(setImmediate)
}
calls.length = 0
liveApp.mount({})
await settle()
assert.equal(calls.length, 0, "进入 Provider 使用已有状态，不自动触发刷新")
liveBindings.providerTab.value = "api"
await settle()
await liveBindings.refresh()
await liveBindings.openAgentConfig(cliTargets[0])
liveBindings.agentProviderId.value = official.id
await liveBindings.selectAgentProvider()
assert.ok(
  calls.some(
    (item) =>
      item.action === "fetch-models" && item.payload.providerId === official.id
  )
)
assert.equal(liveBindings.agentBinding.value.defaultModelName, "fetched-model")
liveBindings.closeAgentConfig()
assert.ok(!calls.some((item) => item.action === "refresh-quota"))
const pendingQuotas = new Map()
quotaHandler = ({ providerId }) =>
  new Promise((resolve) => {
    pendingQuotas.set(providerId, resolve)
  })
liveBindings.providerTab.value = "codex_account"
await settle()
assert.deepEqual(
  new Set(pendingQuotas.keys()),
  new Set([official.id, "google", "codex-secondary", "google-secondary"])
)
const repeatedQuotaBatch = liveBindings.refreshOfficialQuotas()
assert.equal(
  await liveBindings.refreshQuota(
    state.providers.find((item) => item.id === "google")
  ),
  false
)
pendingQuotas.get("google")({})
pendingQuotas.delete("google")
await settle()
assert.equal(liveBindings.refreshingQuotaIds.value.has("google"), false)
assert.equal(liveBindings.refreshingQuotaIds.value.has(official.id), true)
liveBindings.providerTab.value = "agents"
for (const resolve of pendingQuotas.values()) resolve({})
await repeatedQuotaBatch
await settle()
assert.equal(calls.filter((item) => item.action === "refresh-quota").length, 4)
pendingQuotas.clear()
state.providers = state.providers.filter((item) => !item.id.endsWith("-secondary"))
await liveBindings.refresh()
quotaHandler = async ({ providerId }) => {
  if (providerId === official.id) throw new Error(expiredFailure)
  return {}
}
calls.length = 0
liveBindings.providerTab.value = "codex_account"
await settle()
assert.deepEqual(
  calls
    .filter((item) => item.action === "refresh-quota")
    .map((item) => item.payload.providerId),
  [official.id, "google"]
)
assert.equal(liveBindings.refreshingQuotaIds.value.size, 0)
assert.equal(
  liveBindings.officialAccountIssues.value[official.id].requiresReauth,
  true
)
assert.ok(
  liveBindings.officialAccountIssues.value[official.id].message.includes(
    "登录凭据已过期"
  )
)
assert.equal(liveBindings.officialAccountIssues.value.google, null)
assert.ok(!messages.some((message) => message.includes("token_expired")))
await liveBindings.refresh()
await settle()
assert.equal(calls.filter((item) => item.action === "refresh-quota").length, 2)
await liveBindings.refresh(true)
assert.equal(calls.filter((item) => item.action === "refresh-quota").length, 3)
assert.equal(await liveBindings.refreshQuota(official), false)
// 返回官方列表时只刷新仍有授权的账号，已出现 401 的账号不再发请求。
liveBindings.providerTab.value = "api"
await settle()
liveBindings.providerTab.value = "codex_account"
await settle()
assert.deepEqual(
  calls
    .filter((item) => item.action === "refresh-quota")
    .map((item) => item.payload.providerId),
  [official.id, "google", "google", "google"]
)
liveBindings.openOfficialLogin(official)
assert.equal(liveBindings.officialLogin.value.provider.id, official.id)
await liveBindings.completeOfficialLogin()
assert.equal(liveBindings.officialAccountIssues.value[official.id], null)
quotaHandler = async () => ({})
assert.equal(await liveBindings.refreshQuota(official, false), true)
quotaHandler = async () => {
  throw new Error("连接失败，请检查网络代理")
}
await liveBindings.refreshQuota(official, false)
assert.equal(
  liveBindings.officialAccountIssues.value[official.id].requiresReauth,
  false
)
assert.match(
  liveBindings.officialAccountIssues.value[official.id].message,
  /连接失败/
)
quotaHandler = async () => ({})
await liveBindings.refreshQuota(official, false)
assert.equal(liveBindings.officialAccountIssues.value[official.id], null)
calls.length = 0
liveBindings.openOfficialLogin({ kind: "google_account" })
await liveBindings.completeOfficialLogin("google")
await settle()
assert.deepEqual(
  calls
    .filter((item) => item.action === "refresh-quota")
    .map((item) => item.payload.providerId),
  ["google"]
)
liveBindings.providerTab.value = "api"
await settle()
quotaHandler = ({ providerId }) =>
  new Promise((resolve) => {
    pendingQuotas.set(providerId, resolve)
  })
calls.length = 0
liveBindings.providerTab.value = "codex_account"
await settle()
const pendingQuotaBatch = liveBindings.refreshOfficialQuotas()
liveApp.unmount()
for (const resolve of pendingQuotas.values()) resolve({})
await pendingQuotaBatch
assert.deepEqual(
  calls.map((item) => item.action),
  ["refresh-quota", "refresh-quota"]
)
quotaHandler = undefined
state = previousState

// 重新进入页面时读取持久化的授权失败状态，同样跳过请求。
let expiredBindings
const expiredApp = testRenderer.createApp({
  setup(_, context) {
    expiredBindings = Center.setup(
      {
        initialState: expiredState,
        cliTargets,
        runtimeConfigSchemas,
        loginState: null
      },
      context
    )
    return () => null
  }
})
calls.length = 0
expiredApp.mount({})
expiredBindings.providerTab.value = "codex_account"
await settle()
assert.equal(
  expiredBindings.officialAccountIssues.value[official.id].requiresReauth,
  true
)
assert.equal(calls.length, 0, "重新进入后，授权已过期的账号无需再次请求")
expiredApp.unmount()

// 后端成功事件可先于 IPC 返回；完成后关闭面板并更新官方列表。
const loginProps = reactive({
  initialState: state,
  cliTargets,
  runtimeConfigSchemas,
  loginState: null
})
let eventLoginBindings
const eventLoginApp = testRenderer.createApp({
  setup(_, context) {
    eventLoginBindings = Center.setup(loginProps, context)
    return () => null
  }
})
eventLoginApp.mount({})
await settle()
eventLoginBindings.openOfficialLogin({ kind: "codex_account" })
let finishLogin
loginHandler = () =>
  new Promise((resolve) => {
    finishLogin = resolve
  })
const startingLogin = eventLoginBindings.startOfficialLogin({
  accountId: "",
  proxy: ""
})
loginProps.loginState = { status: "success", providerId: "auto-codex" }
await settle()
assert.equal(eventLoginBindings.officialLoginState.value.status, "success")
finishLogin({ status: "pending" })
await startingLogin
await settle()
assert.equal(eventLoginBindings.officialLogin.value, null)
assert.equal(eventLoginBindings.providerTab.value, "codex_account")
assert.equal(eventLoginBindings.busy.value, false)
loginHandler = undefined
eventLoginApp.unmount()

// 模式入口位于系统设置；等待同步时阻止重复点击，失败保持旧模式，成功后显示后端新状态。
const settingsModule = { exports: {} }
new Function("require", "module", "exports", settingsCode)(
  (name) => {
    if (name === "@/api") return { appApi: {} }
    if (name === "@/features/tools/constant") return { getToolItems: () => [] }
    return localRequire(name)
  },
  settingsModule,
  settingsModule.exports
)
const SystemSettings = settingsModule.exports.default
const modeState = reactive({ backend: "legacy" })
const modeCalls = []
let finishMode
let modeBindings
const settingsProps = {
  settings: {},
  cliTargets: [],
  serviceProviders: modeState,
  pending: false,
  saveSettings: async () => true,
  switchProviderBackend: (backend) => {
    modeCalls.push(backend)
    return new Promise((resolve) => {
      finishMode = resolve
    })
  }
}
const settingsApp = createSSRApp(
  {
    ...SystemSettings,
    setup(props, context) {
      modeBindings = SystemSettings.setup(props, context)
      return modeBindings
    }
  },
  settingsProps
)
const settingsHtml = await renderToString(settingsApp)
assert.ok(
  settingsHtml.includes("共享服务商模式") &&
    settingsHtml.includes("自动同步旧 Codex")
)
const enabling = modeBindings.toggleProviderMode()
assert.equal(modeBindings.providerModePending.value, true)
await modeBindings.toggleProviderMode()
assert.deepEqual(modeCalls, ["service"])
finishMode(false)
assert.equal(await enabling, false)
assert.equal(modeBindings.serviceMode.value, false)
assert.equal(modeBindings.providerModePending.value, false)
const retry = modeBindings.toggleProviderMode()
modeState.backend = "service"
finishMode(true)
assert.equal(await retry, true)
assert.equal(modeBindings.serviceMode.value, true)
const rollback = modeBindings.toggleProviderMode()
assert.equal(modeCalls.at(-1), "legacy")
finishMode(false)
assert.equal(await rollback, false)
assert.equal(modeBindings.serviceMode.value, true)

// 原 Usage 组件在两种模式共用，未知值不在显示层被转换成零。
const appSource = readFileSync("src/App.vue", "utf8")
assert.ok(
  !appSource.includes("usage-only") && !appSource.includes("serviceCenterOpen")
)
const usageViewSource = appSource.match(/<UsageView\b[\s\S]*?\/>/)?.[0] || ""
assert.ok(usageViewSource.includes("v-else-if=\"activeView === 'usage'\""))
assert.ok(usageViewSource.includes(':usage="state.usage"'))
assert.ok(
  usageViewSource.includes(
    ':provider-backend="state.serviceProviders?.backend"'
  )
)
const formatterSource = transformSync(
  readFileSync("src/utils/formatters.js", "utf8"),
  { format: "cjs" }
).code
const formatter = { exports: {} }
new Function("module", "exports", formatterSource)(formatter, formatter.exports)
assert.equal(formatter.exports.formatTokenCountParts(null).compact, "未知")
assert.equal(formatter.exports.formatTokenCountParts(0).compact, "0")

// 官方账号只能展示自身新表模型，历史公开 ID 不叠加 account: 前缀。
for (const { filename, code } of selectorComponents) {
  const compiled = { exports: {} }
  new Function("require", "module", "exports", code)(
    (name) => {
      // 此处验证官方模型归属，本地模型列表使用空缓存，避免调用桌面接口。
      if (name === "@/features/assistant/local-models")
        return {
          useAssistantLocalModels: () => ({
            localModels: ref([]),
            localModelsError: ref("")
          })
        }
      if (name.startsWith("@/"))
        return name.endsWith(".vue") ? localRequire(name) : {}
      if (name.endsWith(".css") || name.endsWith("/style/css")) return {}
      return require(name)
    },
    compiled,
    compiled.exports
  )
  const Component = compiled.exports.default
  let selector
  const selection = { targetId: "account:official", model: "own-model" }
  const selectorApp = createSSRApp(
    {
      ...Component,
      setup(props, context) {
        selector = Component.setup(props, context)
        return selector
      },
      render: () => null
    },
    {
      settings: selection,
      modelValue: selection,
      providers: [
        { id: "other", name: "其他服务商", cli: "codex", enabled: true }
      ],
      codexAccounts: [
        {
          id: "official",
          serviceProviderId: "account:official",
          serviceManaged: true,
          modelNames: ["own-model"]
        }
      ],
      runtimeModels: [
        { id: "own", providerId: "account:official", name: "own-model" },
        { id: "other-model", providerId: "other", name: "other-model" }
      ]
    }
  )
  await renderToString(selectorApp)
  if (filename.endsWith("AssistantModelSelect.vue")) {
    const official = selector.modelGroups.value.find(
      (item) => item.targetId === "account:official"
    )
    assert.deepEqual(
      official.models.map((item) => item.model),
      ["own-model"]
    )
    assert.ok(
      !selector.modelGroups.value.some(
        (item) => item.targetId === "account:account:official"
      )
    )
  } else {
    assert.deepEqual(
      selector.modelOptions.value.map((item) =>
        typeof item === "string" ? item : item.id
      ),
      ["own-model"]
    )
    assert.equal(selector.invalidTarget.value, false)
  }
}
console.log(
  `${files.length} 个 SFC 编译通过；原列表布局、图标继承、类型/禁用筛选、官方详情与额度状态、服务商概览与价格状态、Key 入口、Agent 管理、模型目录共用、Codex 同步开关、设置模式切换、Usage 保留、配置保存、诊断测试及回滚交互通过`
)

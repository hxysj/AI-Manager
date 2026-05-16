const fs = require("node:fs/promises")
const path = require("node:path")
const crypto = require("node:crypto")

const providerTypes = new Set([
  "openai",
  "anthropic",
  "gemini",
  "open" + "router",
  "deep" + "seek",
  "custom"
])

const cliTypes = new Set(["claude", "codex", "gemini", "open" + "code"])

const defaultModels = {
  openai: ["gpt-5.2", "gpt-5.1"],
  anthropic: ["claude-sonnet-4-5", "claude-opus-4-1"],
  gemini: ["gemini-2.5-pro", "gemini-2.5-flash"],
  ["open" + "router"]: ["openai/gpt-5.2", "anthropic/claude-sonnet-4.5"],
  ["deep" + "seek"]: ["deep" + "seek-chat", "deep" + "seek-reasoner"],
  custom: []
}

const runtimeConfigSchemas = {
  claude: {
    cli: "claude",
    enabled: true,
    defaultProviderType: "anthropic",
    advancedFields: ["type", "authField"],
    authFields: ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"],
    modelFields: [
      {
        key: "mainModel",
        label: "主模型",
        configKey: "ANTHROPIC_MODEL"
      },
      {
        key: "haikuModel",
        label: "Haiku 默认模型",
        configKey: "ANTHROPIC_DEFAULT_HAIKU_MODEL"
      },
      {
        key: "sonnetModel",
        label: "Sonnet 默认模型",
        configKey: "ANTHROPIC_DEFAULT_SONNET_MODEL"
      },
      {
        key: "opusModel",
        label: "Opus 默认模型",
        configKey: "ANTHROPIC_DEFAULT_OPUS_MODEL"
      }
    ],
    optionFields: [
      { key: "hideAiSignature", label: "隐藏 AI 署名", type: "boolean" },
      { key: "teammatesMode", label: "Teammates 模式", type: "boolean" },
      { key: "toolSearch", label: "启用 Tool Search", type: "boolean" },
      { key: "maxThinking", label: "最大强度思考", type: "boolean" },
      { key: "disableUpgrade", label: "禁用自动升级", type: "boolean" }
    ],
    configFiles: [
      {
        name: "settings.json",
        format: "JSON",
        description: "Claude settings.json 配置内容",
        template: `{
  "env": {
{{#hasApiKey}}
    "{{authField}}": "{{apiKey}}",
{{/hasApiKey}}
{{#hasBaseUrl}}
    "ANTHROPIC_BASE_URL": "{{baseUrl}}",
{{/hasBaseUrl}}
{{#hasMainModel}}
    "ANTHROPIC_MODEL": "{{mainModel}}",
{{/hasMainModel}}
{{#hasHaikuModel}}
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "{{haikuModel}}",
{{/hasHaikuModel}}
{{#hasOpusModel}}
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "{{opusModel}}",
{{/hasOpusModel}}
{{#hasSonnetModel}}
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "{{sonnetModel}}",
{{/hasSonnetModel}}
{{#toolSearch}}
    "ENABLE_TOOL_SEARCH": "{{toolSearchText}}",
{{/toolSearch}}
{{#disableUpgrade}}
    "DISABLE_${"AUTO"}UPDATER": "{{disableUpgradeText}}",
{{/disableUpgrade}}
  },
  "enabledPlugins": {},
  "includeCoAuthoredBy": {{includeCoAuthoredBy}},
  "pluginConfigs": {},
{{#teammatesMode}}
  "teammateMode": "{{teammateMode}}",
{{/teammatesMode}}
  "effortLevel": "{{effortLevel}}"
{{#hideAiSignature}}
  ,
  "attribution": {
    "commit": "",
    "pr": ""
  }
{{/hideAiSignature}}
}`
      }
    ]
  },
  codex: {
    cli: "codex",
    enabled: true,
    defaultProviderType: "openai",
    advancedFields: [],
    authFields: ["OPENAI_API_KEY"],
    modelFields: [
      {
        key: "mainModel",
        label: "模型名称",
        configKey: "model",
        description: "指定使用的模型，将自动更新到 config.toml 中"
      }
    ],
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
      {
        key: "serviceTierFast",
        label: "开启 Fast 模式",
        type: "boolean"
      },
      {
        key: "modelReasoningEffort",
        label: "思考强度",
        type: "select",
        options: ["low", "medium", "high", "xhigh"]
      }
    ],
    configFiles: [
      {
        name: "auth.json",
        format: "JSON",
        description: "Codex auth.json 配置内容",
        template: `{
  "OPENAI_API_KEY": "{{apiKey}}"
}`
      },
      {
        name: "config.toml",
        format: "TOML",
        description: "Codex config.toml 配置内容",
        template: `model_provider = "custom"
model = "{{mainModel}}"
model_reasoning_effort = "{{modelReasoningEffort}}"
disable_response_storage = true
{{#serviceTierFast}}
service_tier = "fast"
{{/serviceTierFast}}
{{#modelContextWindowEnabled}}
model_context_window = 1000000
model_auto_compact_token_limit = {{modelAutoCompactTokenLimit}}
{{/modelContextWindowEnabled}}

[model_providers]
[model_providers.custom]
name = "custom"
wire_api = "responses"
requires_openai_auth = true
base_url = "{{baseUrl}}"`
      }
    ]
  },
  gemini: {
    cli: "gemini",
    enabled: false,
    defaultProviderType: "gemini",
    advancedFields: [],
    authFields: ["GOOGLE_API_KEY"],
    modelFields: [],
    optionFields: [],
    configFiles: []
  },
  ["open" + "code"]: {
    cli: "open" + "code",
    enabled: false,
    defaultProviderType: "openai",
    advancedFields: [],
    authFields: ["OPENAI_API_KEY"],
    modelFields: [],
    optionFields: [],
    configFiles: []
  }
}

function createId(prefix) {
  return `${prefix}-${crypto.randomUUID()}`
}

function now() {
  return Date.now()
}

function normalizeHeaders(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {}
  }

  return Object.fromEntries(
    Object.entries(value)
      .map(([key, item]) => [String(key).trim(), String(item).trim()])
      .filter(([key, item]) => key && item)
  )
}

function normalizeRuntimeConfig(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {}
  }

  return {
    mainModel: String(value.mainModel || "").trim() || undefined,
    haikuModel: String(value.haikuModel || "").trim() || undefined,
    sonnetModel: String(value.sonnetModel || "").trim() || undefined,
    opusModel: String(value.opusModel || "").trim() || undefined,
    toolSearch: Boolean(value.toolSearch),
    disableUpgrade: Boolean(value.disableUpgrade),
    hideAiSignature: Boolean(value.hideAiSignature),
    teammatesMode:
      value.teammatesMode === undefined ? true : Boolean(value.teammatesMode),
    maxThinking:
      value.maxThinking === undefined ? true : Boolean(value.maxThinking),
    modelContextWindowEnabled: Boolean(value.modelContextWindowEnabled),
    serviceTierFast: Boolean(value.serviceTierFast),
    modelReasoningEffort:
      String(value.modelReasoningEffort || "low").trim() || "low",
    modelAutoCompactTokenLimit:
      Number(value.modelAutoCompactTokenLimit) || 900000
  }
}

function toTomlString(value) {
  return JSON.stringify(String(value || ""))
}

function applyTemplate(template, values) {
  return String(template || "")
    .replace(/\{\{#(\w+)}}([\s\S]*?)\{\{\/\1}}/g, (match, key, content) =>
      values[key] ? content : ""
    )
    .replace(/\{\{(\w+)}}/g, (match, key) => values[key] ?? match)
    .replace(/,(\s*[}\]])/g, "$1")
    .replace(/^[\t ]*\r?\n/gm, "")
}

function createTemplateValues(provider, profile, apiKey) {
  const runtimeConfig = provider.runtimeConfig || {}
  const mainModel =
    runtimeConfig.mainModel || (provider.cli === "claude" ? "" : profile.model)
  const haikuModel = runtimeConfig.haikuModel || ""
  const sonnetModel = runtimeConfig.sonnetModel || ""
  const opusModel = runtimeConfig.opusModel || ""

  return {
    authField: provider.authField || "ANTHROPIC_AUTH_TOKEN",
    apiKey,
    hasApiKey: Boolean(apiKey),
    baseUrl: profile.baseUrl || provider.baseUrl || "",
    hasBaseUrl: Boolean(profile.baseUrl || provider.baseUrl),
    mainModel,
    hasMainModel: Boolean(mainModel),
    haikuModel,
    hasHaikuModel: Boolean(haikuModel),
    sonnetModel,
    hasSonnetModel: Boolean(sonnetModel),
    opusModel,
    hasOpusModel: Boolean(opusModel),
    toolSearch: runtimeConfig.toolSearch,
    toolSearchText: runtimeConfig.toolSearch ? "true" : "false",
    disableUpgrade: runtimeConfig.disableUpgrade,
    disableUpgradeText: runtimeConfig.disableUpgrade ? "1" : "0",
    includeCoAuthoredBy: String(!runtimeConfig.hideAiSignature),
    hideAiSignature: runtimeConfig.hideAiSignature,
    teammatesMode: runtimeConfig.teammatesMode,
    teammateMode: "tmux",
    effortLevel: runtimeConfig.maxThinking ? "max" : "default",
    modelContextWindowEnabled: runtimeConfig.modelContextWindowEnabled,
    serviceTierFast: runtimeConfig.serviceTierFast,
    modelReasoningEffort: runtimeConfig.modelReasoningEffort || "low",
    modelAutoCompactTokenLimit:
      runtimeConfig.modelAutoCompactTokenLimit || 900000
  }
}

function normalizeProvider(input, previous) {
  const cli = String(input.cli || previous?.cli || "").trim()
  const name = String(input.name || previous?.name || "").trim()
  const type = String(input.type || previous?.type || "custom").trim()

  if (!cliTypes.has(cli)) {
    throw new Error(`不支持的 CLI Runtime：${cli}`)
  }

  if (!name) {
    throw new Error("Provider 名称不能为空")
  }

  if (!providerTypes.has(type)) {
    throw new Error(`不支持的 Provider 类型：${type}`)
  }

  return {
    id: previous?.id || input.id || createId("provider"),
    cli,
    icon:
      "icon" in input
        ? String(input.icon || "").trim() || undefined
        : String(previous?.icon || "").trim() || undefined,
    name,
    type,
    note: String(input.note || previous?.note || "").trim() || undefined,
    website:
      String(input.website || previous?.website || "").trim() || undefined,
    baseUrl: String(input.baseUrl || "").trim() || undefined,
    proxy: String(input.proxy || "").trim() || undefined,
    authField:
      String(input.authField || previous?.authField || "").trim() || undefined,
    runtimeConfig: normalizeRuntimeConfig(
      input.runtimeConfig || previous?.runtimeConfig
    ),
    headers: normalizeHeaders(input.headers),
    enabled:
      input.enabled === undefined
        ? previous?.enabled !== false
        : Boolean(input.enabled),
    createdAt: previous?.createdAt || now(),
    updatedAt: now()
  }
}

function normalizeModel(input, previous) {
  const providerId = String(
    input.providerId || previous?.providerId || ""
  ).trim()
  const name = String(input.name || previous?.name || input.id || "").trim()

  if (!providerId) {
    throw new Error("模型必须关联 Provider")
  }

  if (!name) {
    throw new Error("模型名称不能为空")
  }

  return {
    id: previous?.id || input.id || name,
    providerId,
    name,
    contextWindow:
      Number(input.contextWindow || previous?.contextWindow) || undefined,
    maxOutput: Number(input.maxOutput || previous?.maxOutput) || undefined,
    supportsTools: Boolean(input.supportsTools || previous?.supportsTools),
    supportsVision: Boolean(input.supportsVision || previous?.supportsVision),
    supportsReasoning: Boolean(
      input.supportsReasoning || previous?.supportsReasoning
    )
  }
}

function normalizeProfile(input, previous) {
  const cli = String(input.cli || previous?.cli || "").trim()
  const providerId = String(
    input.providerId || previous?.providerId || ""
  ).trim()
  const model = String(input.model || previous?.model || "").trim()

  if (!cliTypes.has(cli)) {
    throw new Error(`不支持的 CLI Runtime：${cli}`)
  }

  if (!providerId) {
    throw new Error("Runtime Profile 必须选择 Provider")
  }

  if (!model) {
    throw new Error("Runtime Profile 必须选择模型")
  }

  return {
    id: previous?.id || input.id || cli,
    cli,
    providerId,
    model,
    baseUrl: String(input.baseUrl || "").trim() || undefined,
    proxy: String(input.proxy || "").trim() || undefined,
    env: normalizeHeaders(input.env),
    updatedAt: now()
  }
}

class RuntimeKeyManager {
  constructor(storage) {
    this.storage = storage
    this.keys = {}
    this.secret = crypto
      .createHash("sha256")
      .update(`${process.env.USERPROFILE || ""}|ai-manager-runtime-provider`)
      .digest()
  }

  async init() {
    this.keys = await this.storage.read("runtimeProviderKeys", {})
  }

  encrypt(value) {
    const iv = crypto.randomBytes(12)
    const cipher = crypto.createCipheriv("aes-256-gcm", this.secret, iv)
    const encrypted = Buffer.concat([
      cipher.update(String(value), "utf8"),
      cipher.final()
    ])

    return [
      iv.toString("base64"),
      cipher.getAuthTag().toString("base64"),
      encrypted.toString("base64")
    ].join(".")
  }

  decrypt(value) {
    const [ivText, tagText, encryptedText] = String(value).split(".")
    const decipher = crypto.createDecipheriv(
      "aes-256-gcm",
      this.secret,
      Buffer.from(ivText, "base64")
    )
    decipher.setAuthTag(Buffer.from(tagText, "base64"))

    return Buffer.concat([
      decipher.update(Buffer.from(encryptedText, "base64")),
      decipher.final()
    ]).toString("utf8")
  }

  setProviderKey(providerId, apiKey) {
    const key = String(apiKey || "").trim()

    if (!key) {
      delete this.keys[providerId]
    } else {
      this.keys[providerId] = this.encrypt(key)
    }

    this.storage.scheduleWrite("runtimeProviderKeys", this.keys)
  }

  getProviderKey(providerId) {
    if (!this.keys[providerId]) {
      return ""
    }

    return this.decrypt(this.keys[providerId])
  }

  deleteProviderKey(providerId) {
    delete this.keys[providerId]
    this.storage.scheduleWrite("runtimeProviderKeys", this.keys)
  }

  hasProviderKey(providerId) {
    return Boolean(this.keys[providerId])
  }
}

class RuntimeProviderService {
  constructor(storage) {
    this.storage = storage
    this.keyManager = new RuntimeKeyManager(storage)
    this.providers = []
    this.models = []
    this.profiles = []
  }

  async init() {
    await this.keyManager.init()
    this.providers = await this.storage.read("providers", [])
    this.models = await this.storage.read("runtimeModels", [])
    this.profiles = await this.storage.read("runtimeProfiles", [])
  }

  getState() {
    return {
      runtimeConfigSchemas,
      providers: this.providers.map((item) => ({
        ...item,
        hasApiKey: this.keyManager.hasProviderKey(item.id)
      })),
      runtimeModels: this.models,
      runtimeProfiles: this.profiles.map((item) => this.toPublicProfile(item))
    }
  }

  toPublicProfile(profile) {
    const provider = this.providers.find(
      (item) => item.id === profile.providerId
    )

    return {
      ...profile,
      providerName: provider?.name,
      providerType: provider?.type,
      hasApiKey: provider ? this.keyManager.hasProviderKey(provider.id) : false
    }
  }

  persistMetadata() {
    this.storage.scheduleWrite("providers", this.providers)
    this.storage.scheduleWrite("runtimeModels", this.models)
    this.storage.scheduleWrite("runtimeProfiles", this.profiles)
  }

  saveProvider(input) {
    const previous = this.providers.find((item) => item.id === input.id)
    const provider = normalizeProvider(input, previous)

    if (previous) {
      this.providers = this.providers.map((item) =>
        item.id === provider.id ? provider : item
      )
    } else {
      this.providers = [...this.providers, provider]
      this.addDefaultModels(provider)
    }

    if ("apiKey" in input) {
      this.keyManager.setProviderKey(provider.id, input.apiKey)
    }

    if (input.model) {
      this.saveModel({
        id: `${provider.id}:${input.model}`,
        providerId: provider.id,
        name: input.model
      })
    }

    this.persistMetadata()
    return provider
  }

  deleteProvider(providerId) {
    this.providers = this.providers.filter((item) => item.id !== providerId)
    this.models = this.models.filter((item) => item.providerId !== providerId)
    this.profiles = this.profiles.filter(
      (item) => item.providerId !== providerId
    )
    this.keyManager.deleteProviderKey(providerId)
    this.persistMetadata()
  }

  addDefaultModels(provider) {
    const models = defaultModels[provider.type] || []
    this.models = [
      ...this.models,
      ...models.map((model) =>
        normalizeModel({
          id: `${provider.id}:${model}`,
          providerId: provider.id,
          name: model
        })
      )
    ]
  }

  saveModel(input) {
    const previous = this.models.find((item) => item.id === input.id)
    const model = normalizeModel(input, previous)

    if (!this.providers.find((item) => item.id === model.providerId)) {
      throw new Error("模型关联的 Provider 不存在")
    }

    if (previous) {
      this.models = this.models.map((item) =>
        item.id === model.id ? model : item
      )
    } else {
      this.models = [...this.models, model]
    }

    this.storage.scheduleWrite("runtimeModels", this.models)
    return model
  }

  switchRuntime(input) {
    const provider = this.providers.find((item) => item.id === input.providerId)

    if (!provider) {
      throw new Error("Provider 不存在")
    }

    if (provider.cli !== input.cli) {
      throw new Error("Runtime Profile 不能使用其他 CLI 的 Provider")
    }

    const previous = this.profiles.find((item) => item.cli === input.cli)
    const profile = normalizeProfile(input, previous)

    if (previous) {
      this.profiles = this.profiles.map((item) =>
        item.cli === profile.cli ? profile : item
      )
    } else {
      this.profiles = [...this.profiles, profile]
    }

    this.storage.scheduleWrite("runtimeProfiles", this.profiles)
    return this.toPublicProfile(profile)
  }

  clearRuntime(cli) {
    this.profiles = this.profiles.filter((item) => item.cli !== cli)
    this.storage.scheduleWrite("runtimeProfiles", this.profiles)
  }

  async writeCliConfig(cli, cliTarget) {
    const profile = this.profiles.find((item) => item.cli === cli)

    if (!profile) {
      throw new Error("Runtime Profile 不存在")
    }

    const provider = this.providers.find(
      (item) => item.id === profile.providerId
    )

    if (!provider) {
      throw new Error("Provider 不存在")
    }

    if (!cliTarget?.configPath) {
      throw new Error("CLI 配置目录不存在")
    }

    await fs.mkdir(cliTarget.configPath, { recursive: true })

    if (cli === "claude") {
      await this.writeClaudeConfig(cliTarget.configPath, provider, profile)
    }

    if (cli === "codex") {
      await this.writeCodexConfig(cliTarget.configPath, provider, profile)
    }
  }

  async writeClaudeConfig(configPath, provider, profile) {
    const apiKey = this.keyManager.getProviderKey(provider.id)
    const schema = runtimeConfigSchemas.claude.configFiles[0]

    await fs.writeFile(
      path.join(configPath, "settings.json"),
      `${applyTemplate(schema.template, createTemplateValues(provider, profile, apiKey))}\n`,
      "utf8"
    )
  }

  async writeCodexConfig(configPath, provider, profile) {
    const apiKey = this.keyManager.getProviderKey(provider.id)
    const values = createTemplateValues(provider, profile, apiKey)
    const authSchema = runtimeConfigSchemas.codex.configFiles[0]
    const configSchema = runtimeConfigSchemas.codex.configFiles[1]

    await fs.writeFile(
      path.join(configPath, "auth.json"),
      `${applyTemplate(authSchema.template, values)}\n`,
      "utf8"
    )

    await fs.writeFile(
      path.join(configPath, "config.toml"),
      `${applyTemplate(configSchema.template, values)}\n`,
      "utf8"
    )
  }

  buildRuntimeEnv(cli) {
    const profile = this.profiles.find((item) => item.cli === cli)

    if (!profile) {
      throw new Error("Runtime Profile 不存在")
    }

    const provider = this.providers.find(
      (item) => item.id === profile.providerId
    )

    if (!provider) {
      throw new Error("Provider 不存在")
    }

    const apiKey = this.keyManager.getProviderKey(provider.id)
    const baseUrl = profile.baseUrl || provider.baseUrl
    const proxy = profile.proxy || provider.proxy
    const env = {
      ...profile.env
    }

    if (cli === "claude") {
      env[provider.authField || "ANTHROPIC_AUTH_TOKEN"] = apiKey
      env.ANTHROPIC_MODEL = profile.model
      if (baseUrl) {
        env.ANTHROPIC_BASE_URL = baseUrl
      }
    }

    if (cli === "codex" || cli === "open" + "code") {
      env.OPENAI_API_KEY = apiKey
      env.OPENAI_MODEL = profile.model
      if (baseUrl) {
        env.OPENAI_BASE_URL = baseUrl
      }
    }

    if (cli === "gemini") {
      env.GOOGLE_API_KEY = apiKey
      env.GEMINI_MODEL = profile.model
    }

    if (proxy) {
      env.HTTP_PROXY = proxy
      env.HTTPS_PROXY = proxy
    }

    return env
  }
}

module.exports = {
  RuntimeProviderService
}

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
const runtimeProviderSecret = crypto
  .createHash("sha256")
  .update("ai-manager-runtime-provider|v2|fixed")
  .digest()
const legacyRuntimeProviderSecret = crypto
  .createHash("sha256")
  .update(`${process.env.USERPROFILE || ""}|ai-manager-runtime-provider`)
  .digest()

const defaultModels = {
  openai: ["gpt-5.2", "gpt-5.1"],
  anthropic: ["claude-sonnet-4-5", "claude-opus-4-1"],
  gemini: ["gemini-2.5-pro", "gemini-2.5-flash"],
  ["open" + "router"]: ["openai/gpt-5.2", "anthropic/claude-sonnet-4.5"],
  ["deep" + "seek"]: ["deep" + "seek-chat", "deep" + "seek-reasoner"],
  custom: []
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

function sha256(content) {
  return crypto.createHash("sha256").update(content).digest("hex")
}

function parseBooleanText(value) {
  return String(value || "").trim() === "true"
}

function parseDisableUpgradeText(value) {
  return String(value || "").trim() === "1"
}

function parseTomlValue(value) {
  const text = String(value || "").trim()

  if (/^".*"$/.test(text)) {
    return JSON.parse(text)
  }

  if (/^\d+$/.test(text)) {
    return Number(text)
  }

  if (text === "true" || text === "false") {
    return text === "true"
  }

  return text
}

function parseSimpleToml(content) {
  const root = {}
  const sections = {}
  let current = root

  for (const line of String(content || "").split(/\r?\n/)) {
    const text = line.trim()

    if (!text || text.startsWith("#")) {
      continue
    }

    const sectionMatch = text.match(/^\[(.+)]$/)

    if (sectionMatch) {
      current = sections[sectionMatch[1]] || {}
      sections[sectionMatch[1]] = current
      continue
    }

    const equalIndex = text.indexOf("=")

    if (equalIndex <= 0) {
      continue
    }

    current[text.slice(0, equalIndex).trim()] = parseTomlValue(
      text.slice(equalIndex + 1)
    )
  }

  return {
    root,
    sections
  }
}

function combineConfigContents(files) {
  return files
    .map(file => `### ${file.name}\n${file.content || ""}`)
    .join("\n\n")
}

function normalizeClaudeSettingsContent(content) {
  const settings = String(content || "").trim() ? JSON.parse(content) : {}

  if ("effortLevel" in settings) {
    delete settings.effortLevel
  }

  if ("model" in settings) {
    delete settings.model
  }

  return `${JSON.stringify(settings, null, 2)}\n`
}

function combineManagedConfigContents(cli, files) {
  if (cli === "claude") {
    return combineConfigContents(
      files.map(file => {
        if (file.name !== "settings.json") {
          return file
        }

        return {
          name: file.name,
          content: normalizeClaudeSettingsContent(file.content)
        }
      })
    )
  }

  if (cli !== "codex") {
    return combineConfigContents(files)
  }

  return combineConfigContents(
    files.map(file => {
      if (file.name === "auth.json") {
        const auth = file.content.trim() ? JSON.parse(file.content) : {}

        return {
          name: file.name,
          content: `${JSON.stringify(
            {
              OPENAI_API_KEY: String(auth.OPENAI_API_KEY || "")
            },
            null,
            2
          )}\n`
        }
      }

      if (file.name !== "config.toml") {
        return file
      }

      const config = parseSimpleToml(file.content)
      const customProvider = config.sections["model_providers.custom"] || {}
      const content = [
        `model_provider = ${toTomlString(config.root.model_provider || "custom")}`,
        `model = ${toTomlString(config.root.model || "")}`,
        `model_reasoning_effort = ${toTomlString(
          config.root.model_reasoning_effort || "low"
        )}`,
        `disable_response_storage = ${
          config.root.disable_response_storage === false ? "false" : "true"
        }`
      ]

      if (config.root.service_tier === "fast") {
        content.push('service_tier = "fast"')
      }

      if (config.root.model_context_window) {
        content.push(
          `model_context_window = ${Number(config.root.model_context_window)}`,
          `model_auto_compact_token_limit = ${
            Number(config.root.model_auto_compact_token_limit) || 900000
          }`
        )
      }

      content.push(
        "",
        "[model_providers]",
        "[model_providers.custom]",
        `name = ${toTomlString(customProvider.name || "custom")}`,
        `wire_api = ${toTomlString(customProvider.wire_api || "responses")}`,
        `requires_openai_auth = ${
          customProvider.requires_openai_auth === false ? "false" : "true"
        }`,
        `base_url = ${toTomlString(customProvider.base_url || "")}`
      )

      return {
        name: file.name,
        content: `${content.join("\n")}\n`
      }
    })
  )
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
    this.secret = runtimeProviderSecret
    this.legacySecret = legacyRuntimeProviderSecret
  }

  async init() {
    const storedKeys = await this.storage.read("runtimeProviderKeys", {})
    this.keys = {}

    for (const [providerId, value] of Object.entries(storedKeys || {})) {
      if (!value) {
        continue
      }

      try {
        this.keys[providerId] = this.encrypt(this.decrypt(value))
      } catch {
      }
    }

    if (Object.keys(this.keys).length !== Object.keys(storedKeys || {}).length) {
      await this.storage.writeNow("runtimeProviderKeys", this.keys)
    }
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
    try {
      return this.decryptWithSecret(value, this.secret)
    } catch {
      return this.decryptWithSecret(value, this.legacySecret)
    }
  }

  decryptWithSecret(value, secret) {
    const [ivText, tagText, encryptedText] = String(value).split(".")
    const decipher = crypto.createDecipheriv(
      "aes-256-gcm",
      secret,
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
    this.runtimeState = {}
  }

  async init() {
    await this.keyManager.init()
    this.providers = await this.storage.read("providers", [])
    this.models = await this.storage.read("runtimeModels", [])
    this.profiles = await this.storage.read("runtimeProfiles", [])
    this.runtimeState = await this.storage.read("runtimeProviderState", {})
  }

  exportProviderKeys() {
    return Object.fromEntries(
      this.providers
        .map((item) => [item.id, this.keyManager.getProviderKey(item.id)])
        .filter(([, apiKey]) => Boolean(apiKey))
    )
  }

  async importProviderKeys(apiKeys) {
    const nextKeys = {}

    for (const [providerId, apiKey] of Object.entries(apiKeys || {})) {
      const key = String(apiKey || "").trim()

      if (key) {
        nextKeys[providerId] = this.keyManager.encrypt(key)
      }
    }

    this.keyManager.keys = nextKeys
    await this.storage.writeNow("runtimeProviderKeys", nextKeys)
  }

  async mergeProviderKeys(apiKeys, choices = {}) {
    const nextKeys = { ...this.keyManager.keys }

    for (const [providerId, apiKey] of Object.entries(apiKeys || {})) {
      const key = String(apiKey || "").trim()

      if (!key) {
        continue
      }

      if (
        nextKeys[providerId] &&
        choices[`json:storage/providers.json:${providerId}`] !== "backup"
      ) {
        continue
      }

      nextKeys[providerId] = this.keyManager.encrypt(key)
    }

    this.keyManager.keys = nextKeys
    await this.storage.writeNow("runtimeProviderKeys", nextKeys)
  }

  getState() {
    return {
      runtimeConfigSchemas,
      providers: this.providers.map((item) => ({
        ...item,
        apiKey: this.keyManager.getProviderKey(item.id),
        hasApiKey: this.keyManager.hasProviderKey(item.id)
      })),
      runtimeModels: this.models,
      runtimeProfiles: this.profiles.map((item) => this.toPublicProfile(item)),
      runtimeProviderState: this.runtimeState
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

  saveRuntimeState() {
    this.storage.scheduleWrite("runtimeProviderState", this.runtimeState)
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
    const files = this.buildCliConfigFiles(cli, provider, profile)

    await Promise.all(
      files.map(file =>
        fs.writeFile(
          path.join(cliTarget.configPath, file.name),
          file.content,
          "utf8"
        )
      )
    )
    this.runtimeState[cli] = {
      activeProviderId: provider.id,
      runtimeHash: sha256(combineManagedConfigContents(cli, files)),
      lastSyncAt: now(),
      runtimePath: this.formatRuntimePath(cliTarget.configPath, files),
      status: "SYNCED"
    }
    this.saveRuntimeState()
  }

  buildCliConfigFiles(cli, provider, profile) {
    if (cli === "claude") {
      return this.buildClaudeConfigFiles(provider, profile)
    }

    if (cli === "codex") {
      return this.buildCodexConfigFiles(provider, profile)
    }

    return []
  }

  buildClaudeConfigFiles(provider, profile) {
    const apiKey = this.keyManager.getProviderKey(provider.id)
    const schema = runtimeConfigSchemas.claude.configFiles[0]

    return [
      {
        name: "settings.json",
        content: `${applyTemplate(schema.template, createTemplateValues(provider, profile, apiKey))}\n`
      }
    ]
  }

  buildCodexConfigFiles(provider, profile) {
    const apiKey = this.keyManager.getProviderKey(provider.id)
    const values = createTemplateValues(provider, profile, apiKey)
    const authSchema = runtimeConfigSchemas.codex.configFiles[0]
    const configSchema = runtimeConfigSchemas.codex.configFiles[1]

    return [
      {
        name: "auth.json",
        content: `${applyTemplate(authSchema.template, values)}\n`
      },
      {
        name: "config.toml",
        content: `${applyTemplate(configSchema.template, values)}\n`
      }
    ]
  }

  formatRuntimePath(configPath, files) {
    return files.map(file => path.join(configPath, file.name)).join("\n")
  }

  async readRuntimeConfigFiles(cli, cliTarget) {
    const schema = runtimeConfigSchemas[cli]

    if (!schema?.configFiles?.length || !cliTarget?.configPath) {
      return []
    }

    const files = []

    for (const file of schema.configFiles) {
      const filePath = path.join(cliTarget.configPath, file.name)
      files.push({
        name: file.name,
        content: (await pathExists(filePath))
          ? await fs.readFile(filePath, "utf8")
          : ""
      })
    }

    return files
  }

  async compareRuntime(cli, cliTarget) {
    const profile = this.profiles.find(item => item.cli === cli)

    if (!profile) {
      throw new Error("Runtime Profile 不存在")
    }

    const provider = this.providers.find(item => item.id === profile.providerId)

    if (!provider) {
      throw new Error("Provider 不存在")
    }

    if (!cliTarget?.configPath) {
      throw new Error("CLI 配置目录不存在")
    }

    const managerFiles = this.buildCliConfigFiles(cli, provider, profile)
    const runtimeFiles = await this.readRuntimeConfigFiles(cli, cliTarget)

    return {
      provider,
      profile: this.toPublicProfile(profile),
      managerContent: combineManagedConfigContents(cli, managerFiles),
      runtimeContent: combineManagedConfigContents(cli, runtimeFiles),
      runtimePath: this.formatRuntimePath(cliTarget.configPath, managerFiles)
    }
  }

  async getRuntimeConfig(cli, cliTarget) {
    if (!cliTarget?.configPath) {
      throw new Error("CLI 配置目录不存在")
    }

    const runtimeFiles = await this.readRuntimeConfigFiles(cli, cliTarget)

    return {
      runtimeContent: combineConfigContents(runtimeFiles),
      runtimePath: this.formatRuntimePath(cliTarget.configPath, runtimeFiles)
    }
  }

  async refreshDrift(cliTargets) {
    for (const cli of Object.keys(runtimeConfigSchemas)) {
      const schema = runtimeConfigSchemas[cli]

      if (!schema.enabled || !schema.configFiles.length) {
        continue
      }

      const cliTarget = cliTargets.find(item => item.id === cli)
      const profile = this.profiles.find(item => item.cli === cli)
      const previousState = this.runtimeState[cli] || {}

      if (!profile) {
        this.runtimeState[cli] = {
          ...previousState,
          activeProviderId: "",
          runtimePath: cliTarget?.configPath || "",
          status: "NO_ACTIVE"
        }
        continue
      }

      const provider = this.providers.find(item => item.id === profile.providerId)

      if (!provider) {
        this.runtimeState[cli] = {
          ...previousState,
          activeProviderId: profile.providerId,
          runtimePath: cliTarget?.configPath || "",
          status: "NO_ACTIVE"
        }
        continue
      }

      const managerFiles = this.buildCliConfigFiles(cli, provider, profile)
      const runtimePath = cliTarget?.configPath
        ? this.formatRuntimePath(cliTarget.configPath, managerFiles)
        : ""

      if (!cliTarget?.configPath) {
        this.runtimeState[cli] = {
          ...previousState,
          activeProviderId: provider.id,
          runtimePath,
          status: "DIRTY_MANAGER"
        }
        continue
      }

      const runtimeFiles = await this.readRuntimeConfigFiles(cli, cliTarget)
      const managerHash = sha256(combineManagedConfigContents(cli, managerFiles))
      const runtimeHash = sha256(combineManagedConfigContents(cli, runtimeFiles))
      let status = "SYNCED"

      if (runtimeHash !== managerHash) {
        if (!previousState.runtimeHash) {
          status = "MODIFIED_EXTERNALLY"
        } else if (
          runtimeHash !== previousState.runtimeHash &&
          managerHash !== previousState.runtimeHash
        ) {
          status = "CONFLICT"
        } else if (managerHash !== previousState.runtimeHash) {
          status = "DIRTY_MANAGER"
        } else {
          status = "MODIFIED_EXTERNALLY"
        }
      }

      this.runtimeState[cli] = {
        ...previousState,
        activeProviderId: provider.id,
        runtimeHash:
          status === "SYNCED" ? runtimeHash : previousState.runtimeHash,
        runtimePath,
        status
      }
    }

    this.saveRuntimeState()
  }

  getRuntimeWatchPaths(cliTargets) {
    return Object.values(runtimeConfigSchemas)
      .filter(schema => schema.enabled)
      .flatMap(schema => {
        const cliTarget = cliTargets.find(item => item.id === schema.cli)

        if (!cliTarget?.configPath) {
          return []
        }

        return schema.configFiles.map(file =>
          path.join(cliTarget.configPath, file.name)
        )
      })
  }

  async syncRuntimeConfigToManager(cli, cliTarget) {
    const profile = this.profiles.find(item => item.cli === cli)

    if (!profile) {
      throw new Error("Runtime Profile 不存在")
    }

    const provider = this.providers.find(item => item.id === profile.providerId)

    if (!provider) {
      throw new Error("Provider 不存在")
    }

    if (!cliTarget?.configPath) {
      throw new Error("CLI 配置目录不存在")
    }

    if (cli === "claude") {
      await this.syncClaudeRuntimeToManager(cliTarget, provider, profile)
    }

    if (cli === "codex") {
      await this.syncCodexRuntimeToManager(cliTarget, provider, profile)
    }
  }

  async syncClaudeRuntimeToManager(cliTarget, provider, profile) {
    const settingsPath = path.join(cliTarget.configPath, "settings.json")
    const settings = JSON.parse(await fs.readFile(settingsPath, "utf8"))
    const env = settings.env || {}
    const authField = env.ANTHROPIC_API_KEY
      ? "ANTHROPIC_API_KEY"
      : provider.authField || "ANTHROPIC_AUTH_TOKEN"
    const mainModel = String(env.ANTHROPIC_MODEL || profile.model || "").trim()

    this.saveProvider({
      ...provider,
      baseUrl: String(env.ANTHROPIC_BASE_URL || "").trim(),
      authField,
      apiKey: String(env[authField] || "").trim(),
      runtimeConfig: {
        ...provider.runtimeConfig,
        mainModel,
        haikuModel: String(env.ANTHROPIC_DEFAULT_HAIKU_MODEL || "").trim(),
        sonnetModel: String(env.ANTHROPIC_DEFAULT_SONNET_MODEL || "").trim(),
        opusModel: String(env.ANTHROPIC_DEFAULT_OPUS_MODEL || "").trim(),
        toolSearch: parseBooleanText(env.ENABLE_TOOL_SEARCH),
        disableUpgrade: parseDisableUpgradeText(env["DISABLE_AUTOUPDATER"]),
        hideAiSignature: settings.includeCoAuthoredBy === false,
        teammatesMode: settings.teammateMode === "tmux",
        maxThinking: settings.effortLevel === "max"
      }
    })

    if (mainModel) {
      this.saveModel({
        id: `${provider.id}:${mainModel}`,
        providerId: provider.id,
        name: mainModel
      })
      this.switchRuntime({
        ...profile,
        model: mainModel,
        baseUrl: String(env.ANTHROPIC_BASE_URL || "").trim()
      })
    }
  }

  async syncCodexRuntimeToManager(cliTarget, provider, profile) {
    const authPath = path.join(cliTarget.configPath, "auth.json")
    const configPath = path.join(cliTarget.configPath, "config.toml")
    const auth = JSON.parse(await fs.readFile(authPath, "utf8"))
    const config = parseSimpleToml(await fs.readFile(configPath, "utf8"))
    const customProvider = config.sections["model_providers.custom"] || {}
    const model = String(config.root.model || profile.model || "").trim()
    const apiKey = String(
      auth.OPENAI_API_KEY ||
        auth.tokens?.access_token ||
        provider.apiKey ||
        ""
    ).trim()

    this.saveProvider({
      ...provider,
      baseUrl: String(customProvider.base_url || "").trim(),
      apiKey,
      runtimeConfig: {
        ...provider.runtimeConfig,
        mainModel: model,
        modelReasoningEffort:
          String(config.root.model_reasoning_effort || "low").trim() || "low",
        serviceTierFast: config.root.service_tier === "fast",
        modelContextWindowEnabled: Boolean(config.root.model_context_window),
        modelAutoCompactTokenLimit:
          Number(config.root.model_auto_compact_token_limit) || 900000
      }
    })

    if (model) {
      this.saveModel({
        id: `${provider.id}:${model}`,
        providerId: provider.id,
        name: model
      })
      this.switchRuntime({
        ...profile,
        model,
        baseUrl: String(customProvider.base_url || "").trim()
      })
    }
  }

  async resolveDrift(input, cliTarget) {
    const cli = String(input.cli || "").trim()

    if (input.source === "runtime") {
      await this.syncRuntimeConfigToManager(cli, cliTarget)
      return
    }

    if (input.source !== "manager") {
      throw new Error("请选择 Runtime 配置同步方向")
    }

    await this.writeCliConfig(cli, cliTarget)
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

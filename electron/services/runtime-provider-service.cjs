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
    name,
    type,
    baseUrl: String(input.baseUrl || "").trim() || undefined,
    proxy: String(input.proxy || "").trim() || undefined,
    headers: normalizeHeaders(input.headers),
    enabled: input.enabled === undefined ? previous?.enabled !== false : Boolean(input.enabled),
    createdAt: previous?.createdAt || now(),
    updatedAt: now()
  }
}

function normalizeModel(input, previous) {
  const providerId = String(input.providerId || previous?.providerId || "").trim()
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
    contextWindow: Number(input.contextWindow || previous?.contextWindow) || undefined,
    maxOutput: Number(input.maxOutput || previous?.maxOutput) || undefined,
    supportsTools: Boolean(input.supportsTools || previous?.supportsTools),
    supportsVision: Boolean(input.supportsVision || previous?.supportsVision),
    supportsReasoning: Boolean(input.supportsReasoning || previous?.supportsReasoning)
  }
}

function normalizeProfile(input, previous) {
  const cli = String(input.cli || previous?.cli || "").trim()
  const providerId = String(input.providerId || previous?.providerId || "").trim()
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
      providers: this.providers.map(item => ({
        ...item,
        hasApiKey: this.keyManager.hasProviderKey(item.id)
      })),
      runtimeModels: this.models,
      runtimeProfiles: this.profiles.map(item => this.toPublicProfile(item))
    }
  }

  toPublicProfile(profile) {
    const provider = this.providers.find(item => item.id === profile.providerId)

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
    const previous = this.providers.find(item => item.id === input.id)
    const provider = normalizeProvider(input, previous)

    if (previous) {
      this.providers = this.providers.map(item =>
        item.id === provider.id ? provider : item
      )
    } else {
      this.providers = [...this.providers, provider]
      this.addDefaultModels(provider)
    }

    if ("apiKey" in input) {
      this.keyManager.setProviderKey(provider.id, input.apiKey)
    }

    this.persistMetadata()
    return provider
  }

  deleteProvider(providerId) {
    this.providers = this.providers.filter(item => item.id !== providerId)
    this.models = this.models.filter(item => item.providerId !== providerId)
    this.profiles = this.profiles.filter(item => item.providerId !== providerId)
    this.keyManager.deleteProviderKey(providerId)
    this.persistMetadata()
  }

  addDefaultModels(provider) {
    const models = defaultModels[provider.type] || []
    this.models = [
      ...this.models,
      ...models.map(model => normalizeModel({
        id: `${provider.id}:${model}`,
        providerId: provider.id,
        name: model
      }))
    ]
  }

  saveModel(input) {
    const previous = this.models.find(item => item.id === input.id)
    const model = normalizeModel(input, previous)

    if (!this.providers.find(item => item.id === model.providerId)) {
      throw new Error("模型关联的 Provider 不存在")
    }

    if (previous) {
      this.models = this.models.map(item => item.id === model.id ? model : item)
    } else {
      this.models = [...this.models, model]
    }

    this.storage.scheduleWrite("runtimeModels", this.models)
    return model
  }

  switchRuntime(input) {
    const provider = this.providers.find(item => item.id === input.providerId)

    if (!provider) {
      throw new Error("Provider 不存在")
    }

    if (provider.cli !== input.cli) {
      throw new Error("Runtime Profile 不能使用其他 CLI 的 Provider")
    }

    const previous = this.profiles.find(item => item.cli === input.cli)
    const profile = normalizeProfile(input, previous)

    if (previous) {
      this.profiles = this.profiles.map(item =>
        item.cli === profile.cli ? profile : item
      )
    } else {
      this.profiles = [...this.profiles, profile]
    }

    this.storage.scheduleWrite("runtimeProfiles", this.profiles)
    return this.toPublicProfile(profile)
  }

  buildRuntimeEnv(cli) {
    const profile = this.profiles.find(item => item.cli === cli)

    if (!profile) {
      throw new Error("Runtime Profile 不存在")
    }

    const provider = this.providers.find(item => item.id === profile.providerId)

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
      env.ANTHROPIC_API_KEY = apiKey
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

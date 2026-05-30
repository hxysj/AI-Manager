const fs = require("node:fs/promises")
const http = require("node:http")
const crypto = require("node:crypto")
const os = require("node:os")
const path = require("node:path")
const { EventEmitter } = require("node:events")
const { fetchWithProxy } = require("./codex-account-service.cjs")

const proxyManagedApiKey = "PROXY_MANAGED"
const codexAccountPrefix = "account:"
const codexOfficialBaseUrl = "https://chatgpt.com/backend-api/codex"
const proxyCliDefaults = {
  claude: {
    name: "Claude",
    port: 15722,
    api: {
      port: 15723,
      protocol: "openai-chat-to-anthropic"
    },
    storageKeys: {
      config: "claudeProxyConfig",
      liveBackup: "claudeProxyLiveBackup",
      logs: "claudeProxyRequestLogs",
      apiConfig: "claudeApiConfig",
      apiRecords: "claudeApiRequestRecords"
    }
  },
  codex: {
    name: "Codex",
    port: 15721,
    api: {
      port: 15724,
      protocol: "openai-chat"
    },
    storageKeys: {
      config: "codexProxyConfig",
      liveBackup: "codexProxyLiveBackup",
      logs: "codexProxyRequestLogs",
      apiConfig: "codexApiConfig",
      apiRecords: "codexApiRequestRecords"
    }
  }
}

const baseProxyConfig = {
  enabled: false,
  host: "127.0.0.1",
  activeProviderId: "",
  failoverProviderIds: [],
  accountModel: "",
  retryCount: 1,
  streamTimeoutMs: 120000,
  requestTimeoutMs: 120000,
  updatedAt: 0
}

const baseApiConfig = {
  enabled: false,
  host: "127.0.0.1",
  port: 15724,
  apiKey: "",
  apiKeyId: "",
  apiKeys: [],
  protocol: "openai-chat",
  updatedAt: 0
}

function getDefaultProxyConfig(cli) {
  return {
    ...baseProxyConfig,
    port: proxyCliDefaults[cli]?.port || proxyCliDefaults.codex.port
  }
}

function getDefaultApiConfig(cli) {
  const apiDefaults = proxyCliDefaults[cli]?.api || proxyCliDefaults.codex.api

  return {
    ...baseApiConfig,
    port: apiDefaults.port,
    protocol: apiDefaults.protocol
  }
}

function createLocalApiKey(cli) {
  return `mt-${cli}-${crypto.randomBytes(24).toString("hex")}`
}

function createApiKeyRecord(cli, input = {}) {
  const now = Date.now()

  return {
    id: String(input.id || `api-key-${crypto.randomUUID()}`).trim(),
    cli,
    key: String(input.key || createLocalApiKey(cli)).trim(),
    enabled: input.enabled === undefined ? true : Boolean(input.enabled),
    createdAt: Number(input.createdAt || now),
    updatedAt: Number(input.updatedAt || now)
  }
}

function createApiKeyFingerprint(apiKey) {
  return crypto
    .createHash("sha256")
    .update(String(apiKey || ""))
    .digest("hex")
    .slice(0, 16)
}

function normalizeProxyConfig(input = {}, defaults = getDefaultProxyConfig("codex")) {
  const host = String(input.host || defaults.host).trim()
  const port = Number(input.port || defaults.port)

  return {
    enabled: Boolean(input.enabled),
    host,
    port: Number.isFinite(port) ? port : defaults.port,
    activeProviderId: String(input.activeProviderId || "").trim(),
    failoverProviderIds: Array.isArray(input.failoverProviderIds)
      ? input.failoverProviderIds.map(item => String(item || "").trim()).filter(Boolean)
      : [],
    accountModel: String(input.accountModel || "").trim(),
    retryCount: Number(input.retryCount || defaults.retryCount),
    streamTimeoutMs: Number(
      input.streamTimeoutMs || defaults.streamTimeoutMs
    ),
    requestTimeoutMs: Number(
      input.requestTimeoutMs || defaults.requestTimeoutMs
    ),
    updatedAt: Number(input.updatedAt || 0)
  }
}

function normalizeApiConfig(
  input = {},
  defaults = getDefaultApiConfig("codex"),
  cli = "codex"
) {
  const host = String(input.host || defaults.host).trim()
  const port = Number(input.port || defaults.port)
  const legacyApiKey = String(input.apiKey || "").trim()
  const apiKeys = Array.isArray(input.apiKeys)
    ? input.apiKeys
        .map(item => createApiKeyRecord(cli, item))
        .filter(item => item.key && item.cli === cli)
    : []

  if (legacyApiKey && !apiKeys.find(item => item.key === legacyApiKey)) {
    apiKeys.unshift(
      createApiKeyRecord(cli, {
        id: input.apiKeyId || undefined,
        key: legacyApiKey,
        createdAt: input.updatedAt || 0,
        updatedAt: input.updatedAt || 0
      })
    )
  }

  if (!apiKeys.length) {
    apiKeys.push(createApiKeyRecord(cli))
  }

  const activeApiKey =
    apiKeys.find(item => item.id === input.apiKeyId) ||
    apiKeys.find(item => item.enabled) ||
    apiKeys[0]

  return {
    enabled: Boolean(input.enabled),
    host,
    port: Number.isFinite(port) ? port : defaults.port,
    apiKey: activeApiKey.key,
    apiKeyId: activeApiKey.id,
    apiKeys,
    protocol: String(input.protocol || defaults.protocol).trim(),
    updatedAt: Number(input.updatedAt || 0)
  }
}

function normalizeHostForClient(host) {
  if (host === "0.0.0.0") {
    return "127.0.0.1"
  }

  if (host === "::") {
    return "::1"
  }

  return host
}

function formatHostForUrl(host) {
  return host.includes(":") && !host.startsWith("[") ? `[${host}]` : host
}

function buildLocalBaseUrl(config) {
  const host = formatHostForUrl(normalizeHostForClient(config.host))

  return `http://${host}:${config.port}/v1`
}

function buildAnthropicLocalBaseUrl(config) {
  const host = formatHostForUrl(normalizeHostForClient(config.host))

  return `http://${host}:${config.port}`
}

function buildApiLocalBaseUrl(config) {
  const host = formatHostForUrl(normalizeHostForClient(config.host))

  return `http://${host}:${config.port}/v1`
}

function buildApiLanBaseUrls(config) {
  if (!["0.0.0.0", "::"].includes(config.host)) {
    return []
  }

  return Object.values(os.networkInterfaces())
    .flat()
    .filter(item => item && item.family === "IPv4" && !item.internal)
    .map(item => `http://${item.address}:${config.port}/v1`)
}

function toAccountTargetId(accountId) {
  return `${codexAccountPrefix}${String(accountId || "").trim()}`
}

function isAccountTarget(targetId) {
  return String(targetId || "").startsWith(codexAccountPrefix)
}

function getAccountIdFromTarget(targetId) {
  return String(targetId || "").slice(codexAccountPrefix.length)
}

function parseTomlString(value) {
  const text = String(value || "").trim()

  if (/^".*"$/.test(text)) {
    return JSON.parse(text)
  }

  return text
}

function readTomlRootValue(content, key) {
  for (const line of String(content || "").split(/\r?\n/)) {
    const text = line.trim()

    if (!text || text.startsWith("#") || text.startsWith("[")) {
      continue
    }

    const equalIndex = text.indexOf("=")

    if (equalIndex <= 0) {
      continue
    }

    if (text.slice(0, equalIndex).trim() === key) {
      return parseTomlString(text.slice(equalIndex + 1))
    }
  }

  return ""
}

function toTomlString(value) {
  return JSON.stringify(String(value || ""))
}

function setTomlRootValue(content, key, value) {
  const lines = String(content || "").split(/\r?\n/)
  const nextLine = `${key} = ${toTomlString(value)}`
  const existingIndex = lines.findIndex(line => {
    const text = line.trim()

    if (!text || text.startsWith("#") || text.startsWith("[")) {
      return false
    }

    return text.split("=")[0]?.trim() === key
  })

  if (existingIndex >= 0) {
    lines[existingIndex] = nextLine
    return lines.join("\n").replace(/\n*$/, "\n")
  }

  const firstSectionIndex = lines.findIndex(line => line.trim().startsWith("["))
  const insertIndex = firstSectionIndex >= 0 ? firstSectionIndex : lines.length

  lines.splice(insertIndex, 0, nextLine)
  return lines.join("\n").replace(/\n*$/, "\n")
}

function removeTomlRootValue(content, key) {
  const lines = String(content || "").split(/\r?\n/)
  const nextLines = []
  let inSection = false

  for (const line of lines) {
    const text = line.trim()

    if (text.startsWith("[")) {
      inSection = true
      nextLines.push(line)
      continue
    }

    const equalIndex = text.indexOf("=")

    if (
      !inSection &&
      equalIndex > 0 &&
      text.slice(0, equalIndex).trim() === key
    ) {
      continue
    }

    nextLines.push(line)
  }

  return nextLines.join("\n").replace(/\n*$/, "\n")
}

function setTomlSectionValue(content, sectionName, key, value) {
  const lines = String(content || "").split(/\r?\n/)
  const sectionHeader = `[${sectionName}]`
  const nextLine = `${key} = ${toTomlString(value)}`
  let sectionIndex = lines.findIndex(line => line.trim() === sectionHeader)

  if (sectionIndex < 0) {
    const trimmed = lines.join("\n").replace(/\n*$/, "")
    const prefix = trimmed ? `${trimmed}\n\n` : ""

    return `${prefix}${sectionHeader}\n${nextLine}\n`
  }

  let insertIndex = sectionIndex + 1

  while (
    insertIndex < lines.length &&
    !lines[insertIndex].trim().startsWith("[")
  ) {
    const text = lines[insertIndex].trim()
    const equalIndex = text.indexOf("=")

    if (equalIndex > 0 && text.slice(0, equalIndex).trim() === key) {
      lines[insertIndex] = nextLine
      return lines.join("\n").replace(/\n*$/, "\n")
    }

    insertIndex += 1
  }

  lines.splice(insertIndex, 0, nextLine)
  return lines.join("\n").replace(/\n*$/, "\n")
}

function setCodexProxyConfigToml(content, localBaseUrl, model = "") {
  const modelProvider = readTomlRootValue(content, "model_provider")
  let nextContent = model
    ? setTomlRootValue(content, "model", model)
    : removeTomlRootValue(content, "model")

  if (!modelProvider) {
    return setTomlRootValue(
      setTomlRootValue(nextContent, "base_url", localBaseUrl),
      "wire_api",
      "responses"
    )
  }

  const sectionName = `model_providers.${modelProvider}`

  return setTomlSectionValue(
    setTomlSectionValue(nextContent, sectionName, "base_url", localBaseUrl),
    sectionName,
    "wire_api",
    "responses"
  )
}

function buildClaudeProxySettings(content, localBaseUrl, provider) {
  const settings = String(content || "").trim() ? JSON.parse(content) : {}
  const runtimeConfig = provider.runtimeConfig || {}
  const env = {
    ...(settings.env || {}),
    ANTHROPIC_AUTH_TOKEN: proxyManagedApiKey,
    ANTHROPIC_BASE_URL: localBaseUrl
  }
  const modelEnvKeys = [
    ["mainModel", "ANTHROPIC_MODEL"],
    ["haikuModel", "ANTHROPIC_DEFAULT_HAIKU_MODEL"],
    ["sonnetModel", "ANTHROPIC_DEFAULT_SONNET_MODEL"],
    ["opusModel", "ANTHROPIC_DEFAULT_OPUS_MODEL"]
  ]

  for (const [configKey, envKey] of modelEnvKeys) {
    const value = String(runtimeConfig[configKey] || "").trim()

    if (value) {
      env[envKey] = value
    } else {
      delete env[envKey]
    }
  }

  return `${JSON.stringify(
    {
      ...settings,
      env
    },
    null,
    2
  )}\n`
}

function createJsonResponse(response, statusCode, payload) {
  response.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8"
  })
  response.end(`${JSON.stringify(payload)}\n`)
}

function getAuthorizationToken(request) {
  const authorization = String(request.headers.authorization || "").trim()

  if (authorization.toLowerCase().startsWith("bearer ")) {
    return authorization.slice("bearer ".length).trim()
  }

  return String(request.headers["x-api-key"] || "").trim()
}

function getRequestClientIp(request) {
  const forwardedFor = String(request.headers["x-forwarded-for"] || "")
    .split(",")[0]
    .trim()

  return (
    forwardedFor ||
    request.socket?.remoteAddress ||
    request.connection?.remoteAddress ||
    ""
  )
}

function normalizeModelName(value) {
  return String(value || "").trim().toLowerCase()
}

function getOpenAiUsage(payload = {}) {
  const usage = payload.usage || {}
  const inputTokens = Number(
    usage.prompt_tokens || usage.input_tokens || 0
  )
  const outputTokens = Number(
    usage.completion_tokens || usage.output_tokens || 0
  )
  const cacheReadTokens = Number(
    usage.prompt_tokens_details?.cached_tokens ||
      usage.cache_read_input_tokens ||
      usage.cached_input_tokens ||
      0
  )
  const cacheCreationTokens = Number(
    usage.cache_creation_input_tokens || 0
  )

  return {
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheCreationTokens,
    totalTokens: Number(usage.total_tokens || inputTokens + outputTokens)
  }
}

function normalizeOpenAiPayloadModel(body, model) {
  const payload = parseJsonBody(body)

  if (model) {
    payload.model = model
  }

  return Buffer.from(`${JSON.stringify(payload)}\n`)
}

function getAnthropicUsage(payload = {}) {
  const usage = payload.usage || {}
  const inputTokens = Number(usage.input_tokens || 0)
  const outputTokens = Number(usage.output_tokens || 0)
  const cacheReadTokens = Number(usage.cache_read_input_tokens || 0)
  const cacheCreationTokens = Number(
    usage.cache_creation_input_tokens ||
      usage.cache_creation?.ephemeral_1h_input_tokens ||
      usage.cache_creation?.ephemeral_5m_input_tokens ||
      0
  )

  return {
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheCreationTokens,
    totalTokens:
      inputTokens + outputTokens + cacheReadTokens + cacheCreationTokens
  }
}

function createEmptyUsage() {
  return {
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    totalTokens: 0
  }
}

function pickRequestMessages(input = {}) {
  if (Array.isArray(input.messages)) {
    return input.messages.map(message => ({
      role: String(message.role || "").trim(),
      content: message.content
    }))
  }

  if (input.input !== undefined) {
    return [
      {
        role: "user",
        content: input.input
      }
    ]
  }

  return []
}

function parseJsonBody(body) {
  return JSON.parse(body.toString("utf8"))
}

function createOpenAiChatResponse(payload, model) {
  const content = Array.isArray(payload.content)
    ? payload.content
        .filter(item => item.type === "text")
        .map(item => item.text || "")
        .join("")
    : String(payload.content || "")

  return {
    id: `chatcmpl-${crypto.randomUUID()}`,
    object: "chat.completion",
    created: Math.floor(Date.now() / 1000),
    model,
    choices: [
      {
        index: 0,
        message: {
          role: "assistant",
          content
        },
        finish_reason: payload.stop_reason || "stop"
      }
    ],
    usage: {
      prompt_tokens: payload.usage?.input_tokens || 0,
      completion_tokens: payload.usage?.output_tokens || 0,
      total_tokens:
        (payload.usage?.input_tokens || 0) + (payload.usage?.output_tokens || 0)
    }
  }
}

function normalizeOpenAiContent(content) {
  if (Array.isArray(content)) {
    return content
      .filter(item => item.type === "text")
      .map(item => ({
        type: "text",
        text: String(item.text || "")
      }))
  }

  return String(content || "")
}

function convertOpenAiChatToAnthropic(input, model) {
  if (input.stream) {
    throw new Error("Claude API 服务暂不支持流式请求")
  }

  if (!model) {
    throw new Error("请求缺少模型名称")
  }

  const messages = []
  const system = []

  for (const message of input.messages || []) {
    const role = String(message.role || "").trim()
    const content = normalizeOpenAiContent(message.content)

    if (role === "system") {
      if (Array.isArray(content)) {
        system.push(...content.map(item => item.text).filter(Boolean))
      } else if (content) {
        system.push(content)
      }
      continue
    }

    if (role === "assistant" || role === "user") {
      messages.push({
        role,
        content
      })
    }
  }

  return {
    model,
    messages,
    max_tokens: Number(input.max_tokens || 4096),
    ...(system.length ? { system: system.join("\n") } : {}),
    ...(input.temperature === undefined
      ? {}
      : { temperature: Number(input.temperature) }),
    ...(input.top_p === undefined ? {} : { top_p: Number(input.top_p) }),
    ...(input.stop === undefined ? {} : { stop_sequences: input.stop })
  }
}

function normalizeEndpoint(requestUrl) {
  const url = new URL(requestUrl, "http://127.0.0.1")
  const pathname = url.pathname.replace(/\/+/g, "/")
  const knownEndpoints = [
    "/chat/completions",
    "/responses",
    "/responses/compact"
  ]

  for (const endpoint of knownEndpoints) {
    if (
      pathname === endpoint ||
      pathname === `/v1${endpoint}` ||
      pathname === `/v1/v1${endpoint}` ||
      pathname === `/codex/v1${endpoint}`
    ) {
      return {
        endpoint,
        search: url.search
      }
    }
  }

  return null
}

function normalizeAnthropicEndpoint(requestUrl) {
  const url = new URL(requestUrl, "http://127.0.0.1")
  const pathname = url.pathname.replace(/\/+/g, "/")
  const knownEndpoints = ["/messages", "/messages/count_tokens"]

  for (const endpoint of knownEndpoints) {
    if (pathname === endpoint || pathname === `/v1${endpoint}`) {
      return {
        endpoint,
        search: url.search
      }
    }
  }

  return null
}

function buildUpstreamUrl(baseUrl, endpoint, search) {
  const cleanBase = String(baseUrl || "").trim().replace(/\/+$/, "")

  if (!cleanBase) {
    throw new Error("当前 Codex Provider 缺少请求地址")
  }

  if (/\/chat\/completions$/i.test(cleanBase)) {
    return `${cleanBase}${search || ""}`
  }

  if (/\/responses(?:\/compact)?$/i.test(cleanBase)) {
    return `${cleanBase}${search || ""}`
  }

  const basePath = new URL(cleanBase).pathname.replace(/\/+$/, "")
  const pathPrefix = basePath && basePath !== "/" ? "" : "/v1"

  return `${cleanBase}${pathPrefix}${endpoint}${search || ""}`.replace(
    /\/v1\/v1\//g,
    "/v1/"
  )
}

function buildAnthropicUpstreamUrl(baseUrl, endpoint, search) {
  const cleanBase = String(baseUrl || "").trim().replace(/\/+$/, "")

  if (!cleanBase) {
    throw new Error("当前 Claude Provider 缺少请求地址")
  }

  if (/\/messages(?:\/count_tokens)?$/i.test(cleanBase)) {
    return `${cleanBase}${search || ""}`
  }

  const basePath = new URL(cleanBase).pathname.replace(/\/+$/, "")
  const pathPrefix = /\/v1$/i.test(basePath) ? "" : "/v1"

  return `${cleanBase}${pathPrefix}${endpoint}${search || ""}`.replace(
    /\/v1\/v1\//g,
    "/v1/"
  )
}

class CodexProxyService extends EventEmitter {
  constructor(
    storage,
    runtimeProviderService,
    codexAccountService,
    getCodexCliTarget,
    options = {}
  ) {
    super()
    this.cli = options.cli || "codex"
    this.cliName = proxyCliDefaults[this.cli]?.name || this.cli
    this.storageKeys =
      options.storageKeys || proxyCliDefaults[this.cli]?.storageKeys || proxyCliDefaults.codex.storageKeys
    this.defaultConfig = getDefaultProxyConfig(this.cli)
    this.defaultApiConfig = getDefaultApiConfig(this.cli)
    this.storage = storage
    this.runtimeProviderService = runtimeProviderService
    this.codexAccountService = codexAccountService
    this.getCodexCliTarget = getCodexCliTarget
    this.config = normalizeProxyConfig({}, this.defaultConfig)
    this.liveBackup = null
    this.logs = []
    this.server = null
    this.apiConfig = normalizeApiConfig({}, this.defaultApiConfig, this.cli)
    this.apiRecords = []
    this.apiServer = null
  }

  async init() {
    this.config = normalizeProxyConfig(
      await this.storage.read(this.storageKeys.config, this.defaultConfig),
      this.defaultConfig
    )
    this.liveBackup = await this.storage.read(this.storageKeys.liveBackup, null)
    this.logs = await this.storage.read(this.storageKeys.logs, [])
    this.apiConfig = normalizeApiConfig(
      await this.storage.read(this.storageKeys.apiConfig, this.defaultApiConfig),
      this.defaultApiConfig,
      this.cli
    )
    this.apiRecords = await this.storage.read(this.storageKeys.apiRecords, [])

    if (this.config.enabled) {
      await this.startServer()
    }
    if (this.apiConfig.enabled) {
      await this.startApiServer()
    }
  }

  getState() {
    return {
      ...this.config,
      localBaseUrl:
        this.cli === "claude"
          ? buildAnthropicLocalBaseUrl(this.config)
          : buildLocalBaseUrl(this.config),
      api: {
        ...this.apiConfig,
        apiKeys: this.apiConfig.apiKeys.map(item => ({
          ...item,
          usage: this.getApiKeyUsage(item.id)
        })),
        apiKeyCount: this.apiConfig.apiKeys.length,
        usage: this.getApiUsageSummary(),
        currentKeyUsage: this.getCurrentApiKeyUsage(),
        localBaseUrl: buildApiLocalBaseUrl(this.apiConfig),
        lanBaseUrls: buildApiLanBaseUrls(this.apiConfig)
      },
      hasLiveBackup: Boolean(this.liveBackup),
      logs: this.logs
    }
  }

  isEnabled() {
    return this.config.enabled
  }

  async persistConfig() {
    await this.storage.writeNow(this.storageKeys.config, this.config)
  }

  emitChanged() {
    this.emit("changed", this.getState())
  }

  async persistLogs() {
    await this.storage.writeNow(
      this.storageKeys.logs,
      this.logs.slice(0, 500)
    )
  }

  async startServer() {
    if (this.server) {
      return
    }

    this.server = http.createServer((request, response) => {
      this.handleRequest(request, response).catch(error => {
        createJsonResponse(response, 502, {
          error: {
            message: error.message
          }
        })
      })
    })

    await new Promise((resolve, reject) => {
      this.server.once("error", reject)
      this.server.listen(this.config.port, this.config.host, () => {
        this.server.off("error", reject)
        resolve()
      })
    })
  }

  async stopServer() {
    if (!this.server) {
      return
    }

    await new Promise((resolve, reject) => {
      this.server.close(error => (error ? reject(error) : resolve()))
    })
    this.server = null
  }

  async startApiServer() {
    if (this.apiServer) {
      return
    }

    this.apiServer = http.createServer((request, response) => {
      this.handleApiRequest(request, response).catch(error => {
        createJsonResponse(response, 502, {
          error: {
            message: error.message
          }
        })
      })
    })

    await new Promise((resolve, reject) => {
      this.apiServer.once("error", reject)
      this.apiServer.listen(this.apiConfig.port, this.apiConfig.host, () => {
        this.apiServer.off("error", reject)
        resolve()
      })
    })
  }

  async stopApiServer() {
    if (!this.apiServer) {
      return
    }

    await new Promise((resolve, reject) => {
      this.apiServer.close(error => (error ? reject(error) : resolve()))
    })
    this.apiServer = null
  }

  async persistApiConfig() {
    await this.storage.writeNow(this.storageKeys.apiConfig, this.apiConfig)
  }

  async persistApiRecords() {
    await this.storage.writeNow(
      this.storageKeys.apiRecords,
      this.apiRecords.slice(0, 2000)
    )
  }

  getApiUsageSummary() {
    return this.apiRecords.reduce(
      (summary, record) => {
        if (!record.ok) {
          return summary
        }

        summary.requestCount += 1
        summary.inputTokens += Number(record.inputTokens || 0)
        summary.outputTokens += Number(record.outputTokens || 0)
        summary.cacheReadTokens += Number(record.cacheReadTokens || 0)
        summary.cacheCreationTokens += Number(record.cacheCreationTokens || 0)
        summary.totalTokens += Number(record.totalTokens || 0)
        return summary
      },
      {
        requestCount: 0,
        ...createEmptyUsage()
      }
    )
  }

  getApiKeyUsage(apiKeyId) {
    return this.apiRecords.reduce(
      (summary, record) => {
        if (!record.ok || record.apiKeyId !== apiKeyId) {
          return summary
        }

        summary.requestCount += 1
        summary.inputTokens += Number(record.inputTokens || 0)
        summary.outputTokens += Number(record.outputTokens || 0)
        summary.cacheReadTokens += Number(record.cacheReadTokens || 0)
        summary.cacheCreationTokens += Number(record.cacheCreationTokens || 0)
        summary.totalTokens += Number(record.totalTokens || 0)
        return summary
      },
      {
        requestCount: 0,
        ...createEmptyUsage()
      }
    )
  }

  getCurrentApiKeyUsage() {
    return this.getApiKeyUsage(this.apiConfig.apiKeyId)
  }

  getCodexPaths(cliTarget) {
    if (!cliTarget?.configPath) {
      throw new Error(`${this.cliName} 配置目录不存在`)
    }

    if (this.cli === "claude") {
      return {
        settingsPath: path.join(cliTarget.configPath, "settings.json")
      }
    }

    return {
      authPath: path.join(cliTarget.configPath, "auth.json"),
      configPath: path.join(cliTarget.configPath, "config.toml")
    }
  }

  async readLiveConfig(cliTarget) {
    const paths = this.getCodexPaths(cliTarget)
    if (this.cli === "claude") {
      let settings = "{}\n"

      try {
        settings = await fs.readFile(paths.settingsPath, "utf8")
      } catch (error) {
        if (error.code !== "ENOENT") {
          throw error
        }
      }

      return {
        settings
      }
    }

    const authContent = await fs.readFile(paths.authPath, "utf8")
    const auth = JSON.parse(authContent)
    let config = ""

    try {
      config = await fs.readFile(paths.configPath, "utf8")
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw error
      }
    }

    return {
      auth,
      config
    }
  }

  async writeLiveConfigAtomic(cliTarget, liveConfig) {
    const paths = this.getCodexPaths(cliTarget)
    if (this.cli === "claude") {
      await fs.mkdir(path.dirname(paths.settingsPath), { recursive: true })
      await fs.writeFile(paths.settingsPath, liveConfig.settings, "utf8")
      return
    }

    let previousAuth = null

    try {
      previousAuth = await fs.readFile(paths.authPath, "utf8")
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw error
      }
    }

    await fs.mkdir(path.dirname(paths.authPath), { recursive: true })
    await fs.writeFile(
      paths.authPath,
      `${JSON.stringify(liveConfig.auth, null, 2)}\n`,
      "utf8"
    )

    try {
      await fs.writeFile(paths.configPath, liveConfig.config, "utf8")
    } catch (error) {
      if (previousAuth === null) {
        await fs.rm(paths.authPath, { force: true })
      } else {
        await fs.writeFile(paths.authPath, previousAuth, "utf8")
      }

      throw error
    }
  }

  getProvider(providerId) {
    const provider = this.runtimeProviderService.providers.find(
      item => item.id === providerId
    )

    if (!provider || provider.cli !== this.cli) {
      throw new Error(`${this.cliName} Provider 不存在`)
    }

    if (provider.enabled === false) {
      throw new Error(`${this.cliName} Provider 已禁用`)
    }

    return provider
  }

  getProviderApiKey(providerId) {
    const apiKey = this.runtimeProviderService.keyManager.getProviderKey(
      providerId
    )

    if (!apiKey) {
      throw new Error(`当前 ${this.cliName} Provider 缺少 API Key`)
    }

    return apiKey
  }

  getAccount(targetId) {
    if (this.cli !== "codex") {
      throw new Error(`${this.cliName} 代理不支持官方账号`)
    }

    const accountId = getAccountIdFromTarget(targetId)
    const account = this.codexAccountService.accounts.find(
      item => item.id === accountId
    )

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    if (account.disabled) {
      throw new Error("Codex 官方账号已禁用")
    }

    return account
  }

  getTarget(targetId) {
    if (isAccountTarget(targetId)) {
      const account = this.getAccount(targetId)

      return {
        id: targetId,
        type: "account",
        name: account.email || account.accountId || account.id,
        baseUrl: codexOfficialBaseUrl,
        accountId: account.id,
        proxy: account.proxy || "",
        model:
          account.model ||
          account.defaultModel ||
          this.config.accountModel ||
          "",
        models: this.getAccountModels(account)
      }
    }

    const provider = this.getProvider(targetId)

    return {
      id: targetId,
      type: "provider",
      name: provider.name,
      baseUrl: provider.baseUrl,
      proxy: provider.proxy || "",
      provider,
      model: provider.runtimeConfig?.mainModel || "",
      models: this.getProviderModels(provider)
    }
  }

  getProviderModels(provider) {
    const runtimeConfig = provider.runtimeConfig || {}

    return [
      runtimeConfig.mainModel,
      runtimeConfig.haikuModel,
      runtimeConfig.sonnetModel,
      runtimeConfig.opusModel,
      provider.model
    ]
      .map(item => String(item || "").trim())
      .filter((item, index, models) => item && models.indexOf(item) === index)
  }

  getAccountModels(account) {
    return [
      account.model,
      account.defaultModel,
      this.config.accountModel
    ]
      .map(item => String(item || "").trim())
      .filter((item, index, models) => item && models.indexOf(item) === index)
  }

  targetSupportsModel(targetId, model) {
    const normalizedModel = normalizeModelName(model)

    if (!normalizedModel) {
      return false
    }

    return this.getTarget(targetId).models.some(
      item => normalizeModelName(item) === normalizedModel
    )
  }

  async getTargetAuth(targetId) {
    if (isAccountTarget(targetId)) {
      const auth = await this.codexAccountService.getProxyAuth(
        getAccountIdFromTarget(targetId),
        this.getCodexCliTarget?.()
      )

      return {
        token: auth.accessToken,
        accountId: auth.accountId
      }
    }

    return {
      token: this.getProviderApiKey(targetId),
      provider: this.getProvider(targetId),
      accountId: ""
    }
  }

  async assertTargetReady(targetId) {
    this.getTarget(targetId)

    if (isAccountTarget(targetId)) {
      await this.codexAccountService.getProxyAuth(
        getAccountIdFromTarget(targetId),
        this.getCodexCliTarget?.()
      )
      return
    }

    this.getProviderApiKey(targetId)
  }

  assertTargetJoined(targetId) {
    if (!this.config.failoverProviderIds.includes(targetId)) {
      throw new Error("请先把该目标加入代理接管池")
    }
  }

  isTargetEnabled(targetId) {
    if (isAccountTarget(targetId)) {
      const account = this.codexAccountService.accounts.find(
        item => item.id === getAccountIdFromTarget(targetId)
      )

      return Boolean(account && !account.disabled)
    }

    const provider = this.runtimeProviderService.providers.find(
      item => item.id === targetId
    )

    return Boolean(
      provider && provider.cli === this.cli && provider.enabled !== false
    )
  }

  async enable(input = {}, cliTarget) {
    const activeProviderId = this.getForwardProviderIds()[0] || ""

    if (!activeProviderId) {
      throw new Error("请先把 Provider 加入代理接管列表")
    }

    await this.assertTargetReady(activeProviderId)
    this.assertTargetJoined(activeProviderId)
    await this.startServer()

    const liveConfig = await this.readLiveConfig(cliTarget)
    const localBaseUrl =
      this.cli === "claude"
        ? buildAnthropicLocalBaseUrl(this.config)
        : buildLocalBaseUrl(this.config)
    const activeTarget = this.getTarget(activeProviderId)
    const configModel =
      activeTarget.model || readTomlRootValue(liveConfig.config, "model")
    const nextLiveConfig =
      this.cli === "claude"
        ? {
            settings: buildClaudeProxySettings(
              liveConfig.settings,
              localBaseUrl,
              activeTarget.provider
            )
          }
        : {
            auth: {
              ...liveConfig.auth,
              OPENAI_API_KEY: proxyManagedApiKey
            },
            config: setCodexProxyConfigToml(
              liveConfig.config,
              localBaseUrl,
              configModel
            )
          }

    this.liveBackup = {
      ...liveConfig,
      activeProviderId,
      previousAccountId: input.previousAccountId || "",
      previousProfile: input.previousProfile || null,
      createdAt: Date.now()
    }
    await this.storage.writeNow(this.storageKeys.liveBackup, this.liveBackup)
    await this.writeLiveConfigAtomic(cliTarget, nextLiveConfig)
    this.config = normalizeProxyConfig({
      ...this.config,
      enabled: true,
      activeProviderId,
      updatedAt: Date.now()
    }, this.defaultConfig)
    await this.persistConfig()
    this.emitChanged()
    return this.getState()
  }

  async disable(cliTarget) {
    if (!this.liveBackup) {
      throw new Error(`${this.cliName} 代理 Live 备份不存在，无法恢复`)
    }

    const previousAccountId = this.liveBackup.previousAccountId || ""
    const previousProfile = this.liveBackup.previousProfile || null

    await this.writeLiveConfigAtomic(cliTarget, {
      auth: this.liveBackup.auth,
      config: this.liveBackup.config,
      settings: this.liveBackup.settings
    })
    this.liveBackup = null
    await this.storage.writeNow(this.storageKeys.liveBackup, null)
    this.config = normalizeProxyConfig({
      ...this.config,
      enabled: false,
      activeProviderId: "",
      updatedAt: Date.now()
    }, this.defaultConfig)
    await this.persistConfig()
    await this.stopServer()
    this.emitChanged()
    return {
      state: this.getState(),
      previousAccountId,
      previousProfile
    }
  }

  async addProvider(input) {
    const targetId = input.accountId
      ? toAccountTargetId(input.accountId)
      : String(input.providerId || "").trim()

    await this.assertTargetReady(targetId)

    if (!this.config.failoverProviderIds.includes(targetId)) {
      this.config = normalizeProxyConfig({
        ...this.config,
        failoverProviderIds: [...this.config.failoverProviderIds, targetId],
        updatedAt: Date.now()
      }, this.defaultConfig)
      await this.persistConfig()
      this.emitChanged()
    }

    return this.getState()
  }

  async removeProvider(input) {
    const targetId = input.accountId
      ? toAccountTargetId(input.accountId)
      : String(input.providerId || "").trim()

    if (this.config.enabled && this.config.activeProviderId === targetId) {
      throw new Error("当前被接管的目标不能移出接管池")
    }

    const nextProviderIds = this.config.failoverProviderIds.filter(
      item => item !== targetId
    )

    this.config = normalizeProxyConfig({
      ...this.config,
      activeProviderId:
        this.config.activeProviderId === targetId
          ? nextProviderIds[0] || ""
          : this.config.activeProviderId,
      failoverProviderIds: nextProviderIds,
      updatedAt: Date.now()
    }, this.defaultConfig)
    await this.persistConfig()
    this.emitChanged()
    return this.getState()
  }

  async updateAccountModel(input, cliTarget) {
    this.config = normalizeProxyConfig({
      ...this.config,
      accountModel: input.accountModel,
      updatedAt: Date.now()
    }, this.defaultConfig)
    await this.persistConfig()

    if (
      this.cli === "codex" &&
      this.config.enabled &&
      isAccountTarget(this.config.activeProviderId)
    ) {
      const liveConfig = await this.readLiveConfig(cliTarget)

      await this.writeLiveConfigAtomic(cliTarget, {
        auth: liveConfig.auth,
        config: setCodexProxyConfigToml(
          liveConfig.config,
          buildLocalBaseUrl(this.config),
          this.config.accountModel || readTomlRootValue(
            this.liveBackup?.config,
            "model"
          )
        )
      })
    }

    this.emitChanged()
    return this.getState()
  }

  async updateActiveProvider(providerId, cliTarget) {
    await this.assertTargetReady(providerId)
    this.assertTargetJoined(providerId)
    const target = this.getTarget(providerId)
    if (this.cli === "claude" && this.config.enabled) {
      const liveConfig = await this.readLiveConfig(cliTarget)

      await this.writeLiveConfigAtomic(cliTarget, {
        settings: buildClaudeProxySettings(
          liveConfig.settings,
          buildAnthropicLocalBaseUrl(this.config),
          target.provider
        )
      })
    }
    if (this.cli === "codex" && this.config.enabled) {
      const liveConfig = await this.readLiveConfig(cliTarget)

      await this.writeLiveConfigAtomic(cliTarget, {
        auth: liveConfig.auth,
        config: setCodexProxyConfigToml(
          liveConfig.config,
          buildLocalBaseUrl(this.config),
          target.model || readTomlRootValue(this.liveBackup?.config, "model")
        )
      })
    }
    this.config = normalizeProxyConfig({
      ...this.config,
      activeProviderId: providerId,
      updatedAt: Date.now()
    }, this.defaultConfig)
    await this.persistConfig()
    this.emitChanged()
    return this.getState()
  }

  getForwardProviderIds() {
    return [
      this.config.activeProviderId,
      ...this.config.failoverProviderIds
    ].filter((providerId, index, providerIds) => {
      return (
        providerId &&
        providerIds.indexOf(providerId) === index &&
        this.isTargetEnabled(providerId)
      )
    })
  }

  getApiProviderIds(requestModel) {
    const providerIds = this.getForwardProviderIds()
    const matchedProviderIds = providerIds.filter(providerId =>
      this.targetSupportsModel(providerId, requestModel)
    )

    if (matchedProviderIds.length) {
      return matchedProviderIds
    }

    return providerIds
      .map(providerId => ({
        providerId,
        sort: crypto.randomInt(0, 1000000)
      }))
      .sort((left, right) => left.sort - right.sort)
      .map(item => item.providerId)
  }

  async readBody(request) {
    const chunks = []

    for await (const chunk of request) {
      chunks.push(chunk)
    }

    return Buffer.concat(chunks)
  }

  async buildForwardHeaders(request, targetId) {
    const headers = { ...request.headers }
    const auth = await this.getTargetAuth(targetId)

    delete headers.authorization
    delete headers["x-api-key"]
    delete headers.host
    delete headers["content-length"]
    delete headers["accept-encoding"]
    headers["accept-encoding"] = "identity"

    if (auth.accountId) {
      headers["chatgpt-account-id"] = auth.accountId
      headers.authorization = `Bearer ${auth.token}`
      return headers
    }

    if (this.cli === "claude") {
      headers["x-api-key"] = auth.token
      return headers
    }

    headers.authorization = `Bearer ${auth.token}`
    return headers
  }

  async forwardRequest(request, route, body, targetId, modelOverride = "") {
    const target = this.getTarget(targetId)
    const upstreamUrl =
      this.cli === "claude"
        ? buildAnthropicUpstreamUrl(target.baseUrl, route.endpoint, route.search)
        : buildUpstreamUrl(target.baseUrl, route.endpoint, route.search)
    const method = String(request.method || "GET").toUpperCase()
    const model =
      modelOverride ||
      target.model ||
      readTomlRootValue(this.liveBackup?.config, "model")
    const options = {
      method,
      headers: await this.buildForwardHeaders(request, targetId)
    }
    let requestBody = body

    if (method !== "GET" && method !== "HEAD" && model) {
      requestBody = normalizeOpenAiPayloadModel(body, model)
    }

    if (method !== "GET" && method !== "HEAD") {
      options.body = requestBody
    }

    const startedAt = Date.now()
    const response = await fetchWithProxy(upstreamUrl, options, target.proxy)

    return {
      response,
      target,
      upstreamUrl,
      model,
      latencyMs: Date.now() - startedAt
    }
  }

  appendLog(input) {
    this.logs.unshift({
      id: `proxy-log-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      appType: this.cli,
      dataSource: "proxy",
      createdAt: Date.now(),
      ...input
    })
    this.logs = this.logs.slice(0, 500)
    this.persistLogs().catch(() => {})
    this.emitChanged()
  }

  appendApiRecord(input) {
    this.apiRecords.unshift({
      id: `api-record-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      cli: this.cli,
      createdAt: Date.now(),
      ...input
    })
    this.apiRecords = this.apiRecords.slice(0, 2000)
    this.persistApiRecords().catch(() => {})
  }

  getApiKeyRecord(token) {
    const apiKey = String(token || "").trim()

    return this.apiConfig.apiKeys.find(item => {
      return item.cli === this.cli && item.enabled && item.key === apiKey
    })
  }

  async forwardClaudeApiRequest(request, input, targetId, modelOverride = "") {
    const target = this.getTarget(targetId)
    const model =
      modelOverride || target.model || String(input.model || "").trim()
    const payload = convertOpenAiChatToAnthropic(input, model)
    const upstreamUrl = buildAnthropicUpstreamUrl(
      target.baseUrl,
      "/messages",
      ""
    )
    const options = {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "anthropic-version": request.headers["anthropic-version"] || "2023-06-01",
        "x-api-key": this.getProviderApiKey(targetId)
      },
      body: Buffer.from(`${JSON.stringify(payload)}\n`)
    }
    const startedAt = Date.now()
    const response = await fetchWithProxy(upstreamUrl, options, target.proxy)

    return {
      response,
      target,
      upstreamUrl,
      model,
      latencyMs: Date.now() - startedAt
    }
  }

  async forwardCodexApiRequest(request, body, targetId, modelOverride = "") {
    const route = {
      endpoint: "/chat/completions",
      search: ""
    }
    const input = parseJsonBody(body)

    if (input.stream) {
      throw new Error("Codex API 服务暂不支持流式请求")
    }

    return this.forwardRequest(request, route, body, targetId, modelOverride)
  }

  async forwardApiRequest(request, body, input, targetId, modelOverride = "") {
    if (this.cli === "claude") {
      return this.forwardClaudeApiRequest(request, input, targetId, modelOverride)
    }

    if (this.cli === "codex") {
      return this.forwardCodexApiRequest(request, body, targetId, modelOverride)
    }

    throw new Error(`${this.cliName} 暂不支持 API 服务`)
  }

  async writeApiResponse(response, result) {
    if (this.cli === "claude") {
      const text = await result.response.text()
      if (!result.response.ok) {
        const error = new Error(
          `上游返回非 2xx 状态：${result.response.status} ${text}`
        )

        error.status = result.response.status
        error.upstreamUrl = result.upstreamUrl
        error.upstreamResponseText = text
        throw error
      }

      const upstreamPayload = JSON.parse(text)
      const responsePayload = createOpenAiChatResponse(
        upstreamPayload,
        result.model
      )

      createJsonResponse(response, result.response.status, responsePayload)
      return {
        responseSize: Buffer.byteLength(JSON.stringify(responsePayload)),
        usage: getAnthropicUsage(upstreamPayload)
      }
    }

    if (!result.response.ok) {
      const errorText = await result.response.text()
      const error = new Error(
        `上游返回非 2xx 状态：${result.response.status} ${errorText}`
      )

      error.status = result.response.status
      error.upstreamUrl = result.upstreamUrl
      error.upstreamResponseText = errorText
      throw error
    }

    const text = await result.response.text()
    const responsePayload = JSON.parse(text)
    const headers = Object.fromEntries(result.response.headers.entries())

    response.writeHead(result.response.status, headers)
    response.end(`${text}\n`)
    return {
      responseSize: Buffer.byteLength(text),
      usage: getOpenAiUsage(responsePayload)
    }
  }

  async handleRequest(request, response) {
    const route =
      this.cli === "claude"
        ? normalizeAnthropicEndpoint(request.url)
        : normalizeEndpoint(request.url)

    if (!route) {
      createJsonResponse(response, 404, {
        error: {
          message: `${this.cliName} 代理不支持该请求路径`
        }
      })
      return
    }

    if (!this.config.enabled) {
      createJsonResponse(response, 503, {
        error: {
          message: `${this.cliName} 代理未开启接管`
        }
      })
      return
    }

    const body = await this.readBody(request)
    const providerIds = this.getForwardProviderIds()
    let lastError = null

    for (const providerId of providerIds) {
      const target = this.getTarget(providerId)

      try {
        if (this.config.activeProviderId !== providerId) {
          this.config = normalizeProxyConfig({
            ...this.config,
            activeProviderId: providerId,
            updatedAt: Date.now()
          }, this.defaultConfig)
          await this.persistConfig()
        }
        this.emitChanged()

        const result = await this.forwardRequest(request, route, body, providerId)
        if (!result.response.ok) {
          const errorText = await result.response.text()
          const error = new Error(
            `上游返回非 2xx 状态：${result.response.status} ${errorText}`
          )

          error.status = result.response.status
          error.upstreamUrl = result.upstreamUrl
          error.upstreamResponseText = errorText
          throw error
        }

        const headers = Object.fromEntries(result.response.headers.entries())
        let responseSize = 0

        response.writeHead(result.response.status, headers)
        if (result.response.body) {
          for await (const chunk of result.response.body) {
            const buffer = Buffer.from(chunk)

            responseSize += buffer.length
            response.write(buffer)
          }
        }
        response.end()
        if (this.config.activeProviderId !== providerId) {
          this.config = normalizeProxyConfig({
            ...this.config,
            activeProviderId: providerId,
            updatedAt: Date.now()
          }, this.defaultConfig)
          await this.persistConfig()
          this.emitChanged()
        }
        this.appendLog({
          providerId,
          providerName: result.target.name,
          targetType: result.target.type,
          method: request.method,
          requestUrl: request.url,
          upstreamUrl: result.upstreamUrl,
          endpoint: route.endpoint,
          statusCode: result.response.status,
          ok: result.response.ok,
          latencyMs: result.latencyMs,
          responseSize,
          errorMessage: result.response.ok ? "" : "上游返回非 2xx 状态"
        })
        return
      } catch (error) {
        lastError = error
        this.appendLog({
          providerId,
          providerName: target.name,
          targetType: target.type,
          method: request.method,
          requestUrl: request.url,
          upstreamUrl: error.upstreamUrl || "",
          endpoint: route.endpoint,
          statusCode: error.status || 0,
          ok: false,
          latencyMs: 0,
          errorMessage: error.message,
          upstreamResponseText: error.upstreamResponseText || ""
        })
      }
    }

    throw lastError || new Error(`没有可用的 ${this.cliName} 代理 Provider`)
  }

  async handleApiRequest(request, response) {
    const url = new URL(request.url, "http://127.0.0.1")
    const pathname = url.pathname.replace(/\/+/g, "/")
    const requestUrl = url.pathname + url.search
    const requestHost = String(request.headers.host || "").trim()
    const baseUrl = requestHost
      ? `http://${requestHost}/v1`
      : buildApiLocalBaseUrl(this.apiConfig)
    const clientIp = getRequestClientIp(request)
    const token = getAuthorizationToken(request)
    const apiKeyRecord = this.getApiKeyRecord(token)

    const endpoint = "/v1/chat/completions"

    if (pathname !== endpoint && pathname !== "/chat/completions") {
      createJsonResponse(response, 404, {
        error: {
          message: `${this.cliName} API 服务不支持该请求路径`
        }
      })
      return
    }

    if (String(request.method || "").toUpperCase() !== "POST") {
      createJsonResponse(response, 405, {
        error: {
          message: `${this.cliName} API 服务仅支持 POST 请求`
        }
      })
      return
    }

    if (!apiKeyRecord) {
      createJsonResponse(response, 401, {
        error: {
          message: "API Key 无效"
        }
      })
      return
    }

    if (!this.config.enabled) {
      createJsonResponse(response, 503, {
        error: {
          message: `${this.cliName} 接管池未开启`
        }
      })
      return
    }

    const body = await this.readBody(request)
    const input = parseJsonBody(body)
    const requestModel = String(input.model || "").trim()
    const requestMessages = pickRequestMessages(input)
    const providerIds = this.getApiProviderIds(requestModel)
    let lastError = null

    for (const providerId of providerIds) {
      const target = this.getTarget(providerId)
      const matchedModel = requestModel && this.targetSupportsModel(
        providerId,
        requestModel
      )
      const modelOverride = matchedModel ? requestModel : ""

      try {
        const result = await this.forwardApiRequest(
          request,
          body,
          input,
          providerId,
          modelOverride
        )
        const output = await this.writeApiResponse(response, result)
        if (this.config.activeProviderId !== providerId) {
          this.config = normalizeProxyConfig({
            ...this.config,
            activeProviderId: providerId,
            updatedAt: Date.now()
          }, this.defaultConfig)
          await this.persistConfig()
          this.emitChanged()
        }
        this.appendLog({
          providerId,
          providerName: result.target.name,
          targetType: "api",
          method: request.method,
          requestUrl: request.url,
          upstreamUrl: result.upstreamUrl,
          endpoint,
          statusCode: result.response.status,
          ok: true,
          latencyMs: result.latencyMs,
          responseSize: output.responseSize,
          errorMessage: ""
        })
        this.appendApiRecord({
          apiKeyId: apiKeyRecord.id,
          apiKeyFingerprint: createApiKeyFingerprint(apiKeyRecord.key),
          clientIp,
          method: request.method,
          requestUrl,
          requestHost,
          baseUrl,
          requestModel,
          finalModel: result.model || "",
          matchedModel: Boolean(matchedModel),
          providerId,
          providerName: result.target.name,
          targetType: result.target.type,
          upstreamUrl: result.upstreamUrl,
          statusCode: result.response.status,
          ok: true,
          latencyMs: result.latencyMs,
          responseSize: output.responseSize,
          requestMessages,
          usage: output.usage,
          inputTokens: output.usage.inputTokens,
          outputTokens: output.usage.outputTokens,
          cacheReadTokens: output.usage.cacheReadTokens,
          cacheCreationTokens: output.usage.cacheCreationTokens,
          totalTokens: output.usage.totalTokens,
          errorMessage: ""
        })
        return
      } catch (error) {
        lastError = error
        this.appendLog({
          providerId,
          providerName: target.name,
          targetType: "api",
          method: request.method,
          requestUrl: request.url,
          upstreamUrl: error.upstreamUrl || "",
          endpoint,
          statusCode: error.status || 0,
          ok: false,
          latencyMs: 0,
          errorMessage: error.message,
          upstreamResponseText: error.upstreamResponseText || ""
        })
        this.appendApiRecord({
          apiKeyId: apiKeyRecord.id,
          apiKeyFingerprint: createApiKeyFingerprint(apiKeyRecord.key),
          clientIp,
          method: request.method,
          requestUrl,
          requestHost,
          baseUrl,
          requestModel,
          finalModel: modelOverride || target.model || "",
          matchedModel: Boolean(matchedModel),
          providerId,
          providerName: target.name,
          targetType: target.type,
          upstreamUrl: error.upstreamUrl || "",
          statusCode: error.status || 0,
          ok: false,
          latencyMs: 0,
          responseSize: 0,
          requestMessages,
          usage: createEmptyUsage(),
          inputTokens: 0,
          outputTokens: 0,
          cacheReadTokens: 0,
          cacheCreationTokens: 0,
          totalTokens: 0,
          errorMessage: error.message,
          upstreamResponseText: error.upstreamResponseText || ""
        })
      }
    }

    throw lastError || new Error(`没有可用的 ${this.cliName} API Provider`)
  }

  async enableApi(input = {}, cliTarget) {
    if (!this.config.enabled) {
      throw new Error(`${this.cliName} 接管池未开启`)
    }

    this.apiConfig = normalizeApiConfig({
      ...this.apiConfig,
      ...input,
      enabled: true,
      updatedAt: Date.now()
    }, this.defaultApiConfig, this.cli)
    await this.startApiServer()
    await this.persistApiConfig()
    this.emitChanged()
    return this.getState()
  }

  async disableApi() {
    this.apiConfig = normalizeApiConfig({
      ...this.apiConfig,
      enabled: false,
      updatedAt: Date.now()
    }, this.defaultApiConfig, this.cli)
    await this.stopApiServer()
    await this.persistApiConfig()
    this.emitChanged()
    return this.getState()
  }

  async regenerateApiKey() {
    const nextApiKey = createApiKeyRecord(this.cli)

    this.apiConfig = normalizeApiConfig({
      ...this.apiConfig,
      apiKey: nextApiKey.key,
      apiKeyId: nextApiKey.id,
      apiKeys: [
        nextApiKey,
        ...this.apiConfig.apiKeys
      ],
      updatedAt: Date.now()
    }, this.defaultApiConfig, this.cli)
    await this.persistApiConfig()
    this.emitChanged()
    return this.getState()
  }

  async dispose() {
    await this.stopServer()
    await this.stopApiServer()
  }
}

module.exports = {
  CodexProxyService
}

const fs = require("node:fs/promises")
const http = require("node:http")
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
    storageKeys: {
      config: "claudeProxyConfig",
      liveBackup: "claudeProxyLiveBackup",
      logs: "claudeProxyRequestLogs"
    }
  },
  codex: {
    name: "Codex",
    port: 15721,
    storageKeys: {
      config: "codexProxyConfig",
      liveBackup: "codexProxyLiveBackup",
      logs: "codexProxyRequestLogs"
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

function getDefaultProxyConfig(cli) {
  return {
    ...baseProxyConfig,
    port: proxyCliDefaults[cli]?.port || proxyCliDefaults.codex.port
  }
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
    this.storage = storage
    this.runtimeProviderService = runtimeProviderService
    this.codexAccountService = codexAccountService
    this.getCodexCliTarget = getCodexCliTarget
    this.config = normalizeProxyConfig({}, this.defaultConfig)
    this.liveBackup = null
    this.logs = []
    this.server = null
  }

  async init() {
    this.config = normalizeProxyConfig(
      await this.storage.read(this.storageKeys.config, this.defaultConfig),
      this.defaultConfig
    )
    this.liveBackup = await this.storage.read(this.storageKeys.liveBackup, null)
    this.logs = await this.storage.read(this.storageKeys.logs, [])

    if (this.config.enabled) {
      await this.startServer()
    }
  }

  getState() {
    return {
      ...this.config,
      localBaseUrl:
        this.cli === "claude"
          ? buildAnthropicLocalBaseUrl(this.config)
          : buildLocalBaseUrl(this.config),
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
        model: this.config.accountModel
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
      model: provider.runtimeConfig?.mainModel || ""
    }
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

  async forwardRequest(request, route, body, targetId) {
    const target = this.getTarget(targetId)
    const upstreamUrl =
      this.cli === "claude"
        ? buildAnthropicUpstreamUrl(target.baseUrl, route.endpoint, route.search)
        : buildUpstreamUrl(target.baseUrl, route.endpoint, route.search)
    const method = String(request.method || "GET").toUpperCase()
    const model =
      target.model || readTomlRootValue(this.liveBackup?.config, "model")
    const options = {
      method,
      headers: await this.buildForwardHeaders(request, targetId)
    }
    let requestBody = body

    if (method !== "GET" && method !== "HEAD" && model) {
      const payload = JSON.parse(body.toString("utf8"))

      payload.model = model
      requestBody = Buffer.from(`${JSON.stringify(payload)}\n`)
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

  async dispose() {
    await this.stopServer()
  }
}

module.exports = {
  CodexProxyService
}

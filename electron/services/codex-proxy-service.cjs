const fs = require("node:fs/promises")
const http = require("node:http")
const path = require("node:path")
const { EventEmitter } = require("node:events")
const { fetchWithProxy } = require("./codex-account-service.cjs")

const proxyManagedApiKey = "PROXY_MANAGED"
const codexAccountPrefix = "account:"
const codexOfficialBaseUrl = "https://chatgpt.com/backend-api/codex"
const defaultProxyConfig = {
  enabled: false,
  host: "127.0.0.1",
  port: 15721,
  activeProviderId: "",
  failoverProviderIds: [],
  retryCount: 1,
  streamTimeoutMs: 120000,
  requestTimeoutMs: 120000,
  updatedAt: 0
}

function normalizeProxyConfig(input = {}) {
  const host = String(input.host || defaultProxyConfig.host).trim()
  const port = Number(input.port || defaultProxyConfig.port)

  return {
    enabled: Boolean(input.enabled),
    host,
    port: Number.isFinite(port) ? port : defaultProxyConfig.port,
    activeProviderId: String(input.activeProviderId || "").trim(),
    failoverProviderIds: Array.isArray(input.failoverProviderIds)
      ? input.failoverProviderIds.map(item => String(item || "").trim()).filter(Boolean)
      : [],
    retryCount: Number(input.retryCount || defaultProxyConfig.retryCount),
    streamTimeoutMs: Number(
      input.streamTimeoutMs || defaultProxyConfig.streamTimeoutMs
    ),
    requestTimeoutMs: Number(
      input.requestTimeoutMs || defaultProxyConfig.requestTimeoutMs
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

function setCodexProxyConfigToml(content, localBaseUrl) {
  const modelProvider = readTomlRootValue(content, "model_provider")

  if (!modelProvider) {
    return setTomlRootValue(
      setTomlRootValue(content, "base_url", localBaseUrl),
      "wire_api",
      "responses"
    )
  }

  const sectionName = `model_providers.${modelProvider}`

  return setTomlSectionValue(
    setTomlSectionValue(content, sectionName, "base_url", localBaseUrl),
    sectionName,
    "wire_api",
    "responses"
  )
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

class CodexProxyService extends EventEmitter {
  constructor(storage, runtimeProviderService, codexAccountService, getCodexCliTarget) {
    super()
    this.storage = storage
    this.runtimeProviderService = runtimeProviderService
    this.codexAccountService = codexAccountService
    this.getCodexCliTarget = getCodexCliTarget
    this.config = normalizeProxyConfig()
    this.liveBackup = null
    this.logs = []
    this.server = null
  }

  async init() {
    this.config = normalizeProxyConfig(
      await this.storage.read("codexProxyConfig", defaultProxyConfig)
    )
    this.liveBackup = await this.storage.read("codexProxyLiveBackup", null)
    this.logs = await this.storage.read("codexProxyRequestLogs", [])

    if (this.config.enabled) {
      await this.startServer()
    }
  }

  getState() {
    return {
      ...this.config,
      localBaseUrl: buildLocalBaseUrl(this.config),
      hasLiveBackup: Boolean(this.liveBackup),
      logs: this.logs
    }
  }

  isEnabled() {
    return this.config.enabled
  }

  async persistConfig() {
    await this.storage.writeNow("codexProxyConfig", this.config)
  }

  emitChanged() {
    this.emit("changed", this.getState())
  }

  async persistLogs() {
    await this.storage.writeNow(
      "codexProxyRequestLogs",
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
      throw new Error("Codex 配置目录不存在")
    }

    return {
      authPath: path.join(cliTarget.configPath, "auth.json"),
      configPath: path.join(cliTarget.configPath, "config.toml")
    }
  }

  async readLiveConfig(cliTarget) {
    const paths = this.getCodexPaths(cliTarget)
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

    if (!provider || provider.cli !== "codex") {
      throw new Error("Codex Provider 不存在")
    }

    if (provider.enabled === false) {
      throw new Error("Codex Provider 已禁用")
    }

    return provider
  }

  getProviderApiKey(providerId) {
    const apiKey = this.runtimeProviderService.keyManager.getProviderKey(
      providerId
    )

    if (!apiKey) {
      throw new Error("当前 Codex Provider 缺少 API Key")
    }

    return apiKey
  }

  getAccount(targetId) {
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
        proxy: account.proxy || ""
      }
    }

    const provider = this.getProvider(targetId)

    return {
      id: targetId,
      type: "provider",
      name: provider.name,
      baseUrl: provider.baseUrl,
      proxy: provider.proxy || "",
      provider
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
      provider && provider.cli === "codex" && provider.enabled !== false
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
    const localBaseUrl = buildLocalBaseUrl(this.config)
    const nextLiveConfig = {
      auth: {
        ...liveConfig.auth,
        OPENAI_API_KEY: proxyManagedApiKey
      },
      config: setCodexProxyConfigToml(liveConfig.config, localBaseUrl)
    }

    this.liveBackup = {
      ...liveConfig,
      activeProviderId,
      previousAccountId: input.previousAccountId || "",
      previousProfile: input.previousProfile || null,
      createdAt: Date.now()
    }
    await this.storage.writeNow("codexProxyLiveBackup", this.liveBackup)
    await this.writeLiveConfigAtomic(cliTarget, nextLiveConfig)
    this.config = normalizeProxyConfig({
      ...this.config,
      enabled: true,
      activeProviderId,
      updatedAt: Date.now()
    })
    await this.persistConfig()
    this.emitChanged()
    return this.getState()
  }

  async disable(cliTarget) {
    if (!this.liveBackup) {
      throw new Error("Codex 代理 Live 备份不存在，无法恢复")
    }

    const previousAccountId = this.liveBackup.previousAccountId || ""
    const previousProfile = this.liveBackup.previousProfile || null

    await this.writeLiveConfigAtomic(cliTarget, {
      auth: this.liveBackup.auth,
      config: this.liveBackup.config
    })
    this.liveBackup = null
    await this.storage.writeNow("codexProxyLiveBackup", null)
    this.config = normalizeProxyConfig({
      ...this.config,
      enabled: false,
      activeProviderId: "",
      updatedAt: Date.now()
    })
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
      })
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
    })
    await this.persistConfig()
    this.emitChanged()
    return this.getState()
  }

  async updateActiveProvider(providerId) {
    await this.assertTargetReady(providerId)
    this.assertTargetJoined(providerId)
    this.config = normalizeProxyConfig({
      ...this.config,
      activeProviderId: providerId,
      updatedAt: Date.now()
    })
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
    delete headers.host
    delete headers["content-length"]
    delete headers["accept-encoding"]
    headers.authorization = `Bearer ${auth.token}`
    headers["accept-encoding"] = "identity"

    if (auth.accountId) {
      headers["chatgpt-account-id"] = auth.accountId
    }

    return headers
  }

  async forwardRequest(request, route, body, targetId) {
    const target = this.getTarget(targetId)
    const upstreamUrl = buildUpstreamUrl(
      target.baseUrl,
      route.endpoint,
      route.search
    )
    const method = String(request.method || "GET").toUpperCase()
    const options = {
      method,
      headers: await this.buildForwardHeaders(request, targetId)
    }

    if (method !== "GET" && method !== "HEAD") {
      options.body = body
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
      appType: "codex",
      dataSource: "proxy",
      createdAt: Date.now(),
      ...input
    })
    this.logs = this.logs.slice(0, 500)
    this.persistLogs().catch(() => {})
    this.emitChanged()
  }

  async handleRequest(request, response) {
    const route = normalizeEndpoint(request.url)

    if (!route) {
      createJsonResponse(response, 404, {
        error: {
          message: "Codex 代理不支持该请求路径"
        }
      })
      return
    }

    if (!this.config.enabled) {
      createJsonResponse(response, 503, {
        error: {
          message: "Codex 代理未开启接管"
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
          })
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
          })
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

    throw lastError || new Error("没有可用的 Codex 代理 Provider")
  }

  async dispose() {
    await this.stopServer()
  }
}

module.exports = {
  CodexProxyService
}

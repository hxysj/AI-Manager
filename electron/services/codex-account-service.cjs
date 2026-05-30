const crypto = require("node:crypto")
const fs = require("node:fs/promises")
const http = require("node:http")
const path = require("node:path")
const { EventEmitter } = require("node:events")
const { session, shell } = require("electron")

const OAUTH_BASE_URL = "https://auth.openai.com"
const TOKEN_URL = `${OAUTH_BASE_URL}/oauth/token`
const CODEX_USAGE_URL = "https://chatgpt.com/backend-api/wham/usage"
const CODEX_CLIENT_ID = "app_EMoamEEZ73f0CkXaXp7hrann"
const OAUTH_REDIRECT_URI = "http://localhost:1455/auth/callback"
const OAUTH_SCOPE = "openid profile email offline_access"
const AUTO_REFRESH_INTERVAL = 30 * 60 * 1000
const missingRefreshTokenReason = "missing_refresh_token"
const reauthRefreshErrors = new Set([
  "refresh_token_reused",
  "refresh_token_expired",
  "refresh_token_invalidated",
  "invalid_grant"
])

function createPkce() {
  const verifier = crypto.randomBytes(32).toString("base64url")
  const challenge = crypto
    .createHash("sha256")
    .update(verifier)
    .digest("base64url")

  return {
    verifier,
    challenge
  }
}

function createId(prefix) {
  return `${prefix}-${crypto.randomUUID()}`
}

function formatRfc3339(timestamp) {
  return new Date(timestamp).toISOString().replace(".000Z", "Z")
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

function normalizeProxyRules(proxy) {
  const value = String(proxy || "").trim()

  if (!value || value.includes("=") || value.startsWith("socks")) {
    return value
  }

  const target = value.replace(/^https?:\/\//, "")
  return `http=${target};https=${target}`
}

async function readJson(response) {
  const text = await response.text()

  if (!response.ok) {
    const error = new Error(`OpenAI 请求失败：${response.status} ${text}`)
    error.status = response.status
    throw error
  }

  return JSON.parse(text)
}

async function fetchWithProxy(url, options, proxy) {
  if (!proxy) {
    await session.defaultSession.setProxy({ mode: "system" })
    return session.defaultSession.fetch(url, options)
  }

  const partition = crypto
    .createHash("sha1")
    .update(String(proxy))
    .digest("hex")
    .slice(0, 12)
  const networkSession = session.fromPartition(`codex-account-${partition}`)

  await networkSession.setProxy({
    mode: "fixed_servers",
    proxyRules: normalizeProxyRules(proxy)
  })

  return networkSession.fetch(url, options)
}

function decodeJwtPayload(token) {
  const payload = String(token || "").split(".")[1]

  if (!payload) {
    return {}
  }

  return JSON.parse(
    Buffer.from(
      payload.replace(/-/g, "+").replace(/_/g, "/"),
      "base64"
    ).toString("utf8")
  )
}

function extractAccountId(claims) {
  const authClaims = claims["https://api.openai.com/auth"] || {}

  return String(
    authClaims.chatgpt_account_id || claims.account_id || claims.sub || ""
  ).trim()
}

function extractEmail(claims) {
  const profileClaims = claims["https://api.openai.com/profile"] || {}

  return profileClaims.email || claims.email || ""
}

function parseTimestamp(value) {
  const timestamp = Date.parse(String(value || ""))

  return Number.isNaN(timestamp) ? 0 : timestamp
}

function createTokensFromAuthData(authData) {
  const tokenSource = authData.tokens || authData
  const accessToken = tokenSource.access_token || tokenSource.accessToken || ""
  const idToken =
    tokenSource.id_token || tokenSource.idToken || tokenSource.id_otkne || ""
  const claims = decodeJwtPayload(idToken || accessToken)
  const expiresAt =
    Number(tokenSource.expiresAt || authData.expiresAt || 0) ||
    parseTimestamp(tokenSource.expired || authData.expired) ||
    Number(claims.exp || 0) * 1000

  claims.sub = tokenSource.account_id || claims.sub
  claims.account_id = tokenSource.account_id || claims.account_id

  return {
    claims,
    tokens: {
      accessToken,
      refreshToken: tokenSource.refresh_token || tokenSource.refreshToken || "",
      idToken,
      expiresAt,
      access_token: accessToken,
      refresh_token:
        tokenSource.refresh_token || tokenSource.refreshToken || "",
      id_token: idToken,
      last_refresh: tokenSource.last_refresh || authData.last_refresh || "",
      expired:
        tokenSource.expired ||
        authData.expired ||
        (expiresAt ? formatRfc3339(expiresAt) : ""),
      token_updated_at:
        Number(
          tokenSource.token_updated_at || authData.token_updated_at || 0
        ) || parseTimestamp(tokenSource.last_refresh || authData.last_refresh)
    }
  }
}

function accountExpiresAt(account) {
  return account.auth?.expiresAt || parseTimestamp(account.expired)
}

function accountAccessToken(account) {
  return account.auth?.accessToken || account.access_token || ""
}

function accountRefreshToken(account) {
  return account.auth?.refreshToken || account.refresh_token || ""
}

function accountAccessTokenExpired(account) {
  const expiresAt = accountExpiresAt(account)

  return !accountAccessToken(account) || !expiresAt || expiresAt <= Date.now()
}

function tokensAccessTokenExpired(tokens) {
  return (
    !tokens.accessToken || !tokens.expiresAt || tokens.expiresAt <= Date.now()
  )
}

function removeTomlSections(content, sectionNames) {
  const lines = String(content || "").split(/\r?\n/)
  const nextLines = []
  let skipping = false

  for (const line of lines) {
    const section = line.trim().match(/^\[(.+)]$/)

    if (section) {
      skipping = sectionNames.has(section[1])

      if (skipping) {
        continue
      }
    }

    if (!skipping) {
      nextLines.push(line)
    }
  }

  return nextLines.join("\n")
}

function removeTomlRootKeys(content, keys) {
  const lines = String(content || "").split(/\r?\n/)
  const nextLines = []
  let inSection = false

  for (const line of lines) {
    if (/^\s*\[.+]\s*$/.test(line)) {
      inSection = true
    }

    const key = line.match(/^\s*([A-Za-z0-9_-]+)\s*=/)?.[1]

    if (!inSection && key && keys.has(key)) {
      continue
    }

    nextLines.push(line)
  }

  return nextLines
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim()
}

function shouldMarkReauth(error) {
  const text = [
    error.oauthError,
    error.oauthErrorDescription,
    error.message
  ].join(" ")

  return [...reauthRefreshErrors].some(item => text.includes(item))
}

class CodexAccountService extends EventEmitter {
  constructor(storage) {
    super()
    this.storage = storage
    this.accounts = []
    this.loginState = null
    this.loginServer = null
    this.autoRefreshTimer = null
    this.autoRefreshTimers = new Map()
    this.getCodexCliTarget = null
    this.activeAccountId = ""
  }

  async init() {
    this.accounts = await this.storage.read("codexAccounts", [])
    this.activeAccountId = await this.storage.read("codexActiveAccountId", "")
  }

  startAutoRefresh(getCodexCliTarget) {
    if (this.autoRefreshTimer) {
      return
    }

    this.getCodexCliTarget = getCodexCliTarget

    this.refreshExpiringAccounts().catch(error => {
      this.emit("login-state", {
        status: "failed",
        message: `Codex 自动刷新失败：${error.message || String(error)}`
      })
    })

    this.autoRefreshTimer = setInterval(() => {
      this.refreshExpiringAccounts().catch(error => {
        this.emit("login-state", {
          status: "failed",
          message: `Codex 自动刷新失败：${error.message || String(error)}`
        })
      })
    }, AUTO_REFRESH_INTERVAL)
  }

  stopAutoRefresh() {
    if (this.autoRefreshTimer) {
      clearInterval(this.autoRefreshTimer)
      this.autoRefreshTimer = null
    }

    for (const schedule of this.autoRefreshTimers.values()) {
      clearTimeout(schedule.timer)
    }
    this.autoRefreshTimers.clear()
  }

  getState() {
    return this.accounts.map(account => ({
      id: account.id,
      provider: account.provider,
      accountId: account.accountId,
      account_id: account.account_id,
      email: account.email,
      plan: account.plan,
      usage: account.usage,
      proxy: account.proxy,
      model: account.model || "",
      defaultModel: account.defaultModel || account.model || "",
      autoRefresh: account.autoRefresh,
      createdAt: account.createdAt,
      updatedAt: account.updatedAt,
      last_refresh: account.last_refresh,
      expired: account.expired,
      type: account.type,
      token_generation: account.token_generation || 0,
      token_updated_at: account.token_updated_at || 0,
      refresh_status: account.refresh_status || "",
      refresh_status_code: account.refresh_status_code || 0,
      refresh_message: account.refresh_message || "",
      requires_reauth: Boolean(account.requires_reauth),
      reauth_reason: account.reauth_reason || "",
      reauth_message: account.reauth_message || "",
      disabled: Boolean(account.disabled),
      active: account.id === this.activeAccountId,
      auth: {
        expiresAt: account.auth?.expiresAt || 0
      }
    }))
  }

  async startLogin(input = {}) {
    if (this.loginServer || this.loginState?.status === "pending") {
      throw new Error("Codex 官方登录正在进行中")
    }

    const targetAccountId = String(input.accountId || "").trim()
    if (
      targetAccountId &&
      !this.accounts.find(account => account.id === targetAccountId)
    ) {
      throw new Error("Codex 官方账号不存在")
    }

    const pkce = createPkce()
    const state = crypto.randomBytes(16).toString("hex")
    const callback = await this.startCallbackServer()
    const params = new URLSearchParams({
      client_id: CODEX_CLIENT_ID,
      code_challenge: pkce.challenge,
      code_challenge_method: "S256",
      codex_cli_simplified_flow: "true",
      id_token_add_organizations: "true",
      prompt: "login",
      redirect_uri: callback.redirectUri,
      response_type: "code",
      scope: OAUTH_SCOPE,
      state
    })
    const authUrl = `${OAUTH_BASE_URL}/oauth/authorize?${params.toString()}`

    this.loginState = {
      status: "pending",
      verifier: pkce.verifier,
      state,
      redirectUri: callback.redirectUri,
      proxy: String(input.proxy || "").trim(),
      targetAccountId,
      authUrl
    }
    this.emit("login-state", this.getLoginState())

    try {
      await shell.openExternal(authUrl)
    } catch (error) {
      this.loginState = {
        ...this.loginState,
        status: "failed",
        message: error.message || String(error)
      }
      this.emit("login-state", this.getLoginState())
      this.stopCallbackServer()
      throw error
    }

    return {
      authUrl,
      redirectUri: callback.redirectUri,
      status: "pending"
    }
  }

  async importAuthJson(input) {
    const authData = JSON.parse(String(input?.content || ""))
    const { tokens, claims } = createTokensFromAuthData(authData)

    if (!tokens.accessToken) {
      throw new Error("Codex 登录 JSON 数据缺少 access_token")
    }

    const proxy = String(input.proxy || "").trim()
    const targetAccountId = String(input.accountId || "").trim()
    const targetAccount = targetAccountId
      ? this.accounts.find(account => account.id === targetAccountId)
      : null
    const accountId = extractAccountId(claims)

    if (targetAccountId && !targetAccount) {
      throw new Error("Codex 官方账号不存在")
    }

    if (targetAccount?.disabled) {
      throw new Error("Codex 官方账号已禁用，不能编辑")
    }

    if (
      targetAccount &&
      ![
        targetAccount.id,
        targetAccount.accountId,
        targetAccount.account_id
      ].includes(accountId)
    ) {
      throw new Error("登录数据与当前账号不一致")
    }

    if (
      accountId &&
      this.accounts.find(
        account =>
          account.id !== targetAccountId &&
          (account.id === accountId ||
            account.accountId === accountId ||
            account.account_id === accountId)
      )
    ) {
      throw new Error("此账户已导入")
    }

    const usage = await this.fetchUsageInfo(tokens.accessToken, claims, proxy)
    const profile = {
      email: extractEmail(claims),
      sub: claims.sub
    }
    const account = this.saveAccount(tokens, profile, claims, usage, proxy)

    if (
      account.id === this.activeAccountId &&
      typeof this.getCodexCliTarget === "function"
    ) {
      await this.writeAccountBundle(account, this.getCodexCliTarget())
    }

    return {
      id: account.id,
      email: account.email,
      plan: account.plan
    }
  }

  startCallbackServer() {
    return new Promise((resolve, reject) => {
      const server = http.createServer(async (request, response) => {
        try {
          const requestUrl = new URL(request.url, this.loginState.redirectUri)

          if (requestUrl.pathname !== "/auth/callback") {
            response.writeHead(404)
            response.end("Not found")
            return
          }

          await this.completeLogin(requestUrl)
          response.writeHead(200, {
            "content-type": "text/html; charset=utf-8"
          })
          response.end("Codex 登录已完成，可以返回 Monkey Thief。")
        } catch (error) {
          this.failLogin(error)
          response.writeHead(500, {
            "content-type": "text/plain; charset=utf-8"
          })
          response.end(error.message)
        }
      })

      server.on("error", reject)
      server.listen(1455, "127.0.0.1", () => {
        this.loginServer = server
        resolve({
          redirectUri: OAUTH_REDIRECT_URI
        })
      })
    })
  }

  stopCallbackServer() {
    if (this.loginServer) {
      this.loginServer.close()
      this.loginServer = null
    }
  }

  getLoginState() {
    if (!this.loginState) {
      return null
    }

    return {
      status: this.loginState.status,
      authUrl: this.loginState.authUrl,
      redirectUri: this.loginState.redirectUri,
      message: this.loginState.message || "",
      account: this.loginState.account || null
    }
  }

  failLogin(error) {
    if (!this.loginState) {
      return
    }

    this.loginState = {
      ...this.loginState,
      status: "failed",
      message: error.message || String(error)
    }
    this.emit("login-state", this.getLoginState())
    this.stopCallbackServer()
  }

  cancelLogin() {
    if (!this.loginState) {
      return null
    }

    if (this.loginState.status !== "pending") {
      this.loginState = null
      this.emit("login-state", null)
      return null
    }

    const nextLoginState = {
      ...this.loginState,
      status: "cancelled",
      message: "Codex 官方登录已取消"
    }
    this.loginState = nextLoginState
    this.emit("login-state", nextLoginState)
    this.stopCallbackServer()
    this.loginState = null
    return this.getLoginState()
  }

  saveAccount(tokens, profile, claims, usage, proxy = "") {
    const accountId =
      extractAccountId(claims) ||
      profile.account_id ||
      profile.user_id ||
      profile.sub ||
      createId("codex-account")
    const email = profile.email || extractEmail(claims) || "未识别账号"
    const currentAccount =
      this.accounts.find(account => account.id === accountId) || null
    const tokenUpdatedAt =
      Number(tokens.token_updated_at || tokens.tokenUpdatedAt || 0) ||
      parseTimestamp(tokens.last_refresh) ||
      Date.now()
    const nextAccount = {
      id: accountId,
      provider: "codex",
      type: "codex",
      accountId,
      account_id: accountId,
      email,
      plan:
        usage.plan_type ||
        profile.chatgpt_plan_type ||
        claims.chatgpt_plan_type ||
        "",
      usage,
      proxy: proxy || currentAccount?.proxy || "",
      model: currentAccount?.model || currentAccount?.defaultModel || "",
      defaultModel: currentAccount?.defaultModel || currentAccount?.model || "",
      id_token: tokens.id_token,
      access_token: tokens.access_token,
      refresh_token: tokens.refresh_token,
      last_refresh: tokens.last_refresh,
      expired: tokens.expired,
      auth: tokens,
      token_generation:
        Number(tokens.token_generation || tokens.tokenGeneration || 0) ||
        Number(currentAccount?.token_generation || 0),
      token_updated_at: tokenUpdatedAt,
      refresh_status: "",
      refresh_status_code: 0,
      refresh_message: "",
      requires_reauth: false,
      reauth_reason: "",
      reauth_message: "",
      autoRefresh: currentAccount?.autoRefresh !== false,
      disabled: Boolean(currentAccount?.disabled),
      createdAt: currentAccount?.createdAt || Date.now(),
      updatedAt: Date.now()
    }

    this.accounts = currentAccount
      ? this.accounts.map(account =>
          account.id === accountId ? nextAccount : account
        )
      : [...this.accounts, nextAccount]
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
    return nextAccount
  }

  async enableAccount(accountId, cliTarget) {
    const account = await this.prepareAccountForSwitch(accountId, cliTarget)

    await this.writeAccountBundle(account, cliTarget)

    this.activeAccountId = account.id
    this.storage.scheduleWrite("codexActiveAccountId", this.activeAccountId)
    this.emit("changed", this.getState())
  }

  async prepareAccountForSwitch(accountId, cliTarget) {
    let account = this.accounts.find(item => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    if (account.disabled) {
      throw new Error("Codex 官方账号已禁用，不能启用")
    }

    if (account.type === "apikey") {
      return account
    }

    account = await this.syncAccountFromAuthoritySources(account, cliTarget)
    account = this.clearMissingRefreshTokenReauth(account)

    if (account.requires_reauth) {
      throw new Error(
        account.reauth_message ||
          account.reauth_reason ||
          "Codex 登录授权需要重新登录"
      )
    }

    if (!accountAccessTokenExpired(account)) {
      return account
    }

    return this.performManagedTokenRefresh(account, cliTarget)
  }

  async getProxyAuth(accountId, cliTarget) {
    const account = await this.prepareAccountForSwitch(accountId, cliTarget)
    const accessToken = accountAccessToken(account)

    if (!accessToken) {
      throw new Error("Codex 官方账号缺少 access_token")
    }

    return {
      accessToken,
      accountId: account.account_id || account.accountId || account.id,
      name: account.email || account.accountId || account.id
    }
  }

  async syncAccountFromAuthoritySources(account, cliTarget) {
    if (!cliTarget?.configPath) {
      return account
    }

    const authPath = path.join(cliTarget.configPath, "auth.json")

    if (!(await pathExists(authPath))) {
      return account
    }

    const authData = JSON.parse(await fs.readFile(authPath, "utf8"))
    const { tokens, claims } = createTokensFromAuthData(authData)
    const sourceAccountId = extractAccountId(claims)

    const accountId = account.account_id || account.accountId || account.id

    if (!tokens.accessToken || sourceAccountId !== accountId) {
      return account
    }

    const sourceUpdatedAt =
      tokens.token_updated_at || parseTimestamp(authData.last_refresh)
    const accountUpdatedAt =
      Number(account.token_updated_at || 0) ||
      parseTimestamp(account.last_refresh)
    const shouldUseSource =
      sourceUpdatedAt >= accountUpdatedAt ||
      (accountAccessTokenExpired(account) && !tokensAccessTokenExpired(tokens))

    if (!shouldUseSource) {
      return account
    }

    return this.saveAccount(
      {
        ...tokens,
        token_generation: account.token_generation || 0,
        token_updated_at: sourceUpdatedAt || Date.now()
      },
      {
        email: extractEmail(claims) || account.email,
        sub: claims.sub || account.accountId
      },
      claims,
      account.usage || {},
      account.proxy
    )
  }

  clearMissingRefreshTokenReauth(account) {
    if (
      !account.requires_reauth ||
      account.reauth_reason !== missingRefreshTokenReason ||
      !accountRefreshToken(account)
    ) {
      return account
    }

    const nextAccount = {
      ...account,
      requires_reauth: false,
      reauth_reason: "",
      reauth_message: "",
      updatedAt: Date.now()
    }

    this.accounts = this.accounts.map(item =>
      item.id === account.id ? nextAccount : item
    )
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    return nextAccount
  }

  async performManagedTokenRefresh(account, cliTarget) {
    if (!accountRefreshToken(account)) {
      const message =
        "Codex 登录授权缺少 refresh_token，无法自动续期；当前 access_token 已不可用。"
      this.markAccountReauth(account.id, missingRefreshTokenReason, message)
      throw new Error(message)
    }

    try {
      const tokens = await this.refreshToken(
        accountRefreshToken(account),
        account.proxy
      )
      const claims = decodeJwtPayload(tokens.idToken || tokens.accessToken)
      claims.account_id =
        account.account_id ||
        account.accountId ||
        account.id ||
        claims.account_id
      const usage = await this.fetchUsageInfo(
        tokens.accessToken,
        claims,
        account.proxy
      )
      const nextAccount = this.saveAccount(
        {
          ...tokens,
          token_generation: Number(account.token_generation || 0) + 1,
          token_updated_at: Date.now()
        },
        {
          email: extractEmail(claims) || account.email,
          sub: claims.sub || account.accountId
        },
        claims,
        usage,
        account.proxy
      )

      if (nextAccount.id === this.activeAccountId) {
        await this.writeAccountBundle(nextAccount, cliTarget)
      }

      return nextAccount
    } catch (error) {
      if (shouldMarkReauth(error)) {
        this.markAccountReauth(
          account.id,
          error.oauthError || "invalid_grant",
          "Codex 登录授权已失效，请重新登录。"
        )
      }

      this.markAccountRefreshError(account.id, error.status || 0, error.message)

      throw error
    }
  }

  markAccountReauth(accountId, reason, message) {
    this.accounts = this.accounts.map(account =>
      account.id === accountId
        ? {
            ...account,
            requires_reauth: true,
            reauth_reason: reason,
            reauth_message: message,
            updatedAt: Date.now()
          }
        : account
    )
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
  }

  markAccountRefreshError(accountId, statusCode, message) {
    this.accounts = this.accounts.map(account =>
      account.id === accountId
        ? {
            ...account,
            refresh_status: "failed",
            refresh_status_code: statusCode,
            refresh_message: message,
            updatedAt: Date.now()
          }
        : account
    )
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
  }

  async refreshAccountUsage(accountId, cliTarget, options = {}) {
    const account = this.accounts.find(item => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    if (account.disabled) {
      throw new Error("Codex 官方账号已禁用，不能刷新额度")
    }

    const expiresAt = accountExpiresAt(account)
    let tokens = {
      accessToken: account.auth?.accessToken || account.access_token,
      refreshToken: account.auth?.refreshToken || account.refresh_token,
      idToken: account.auth?.idToken || account.id_token,
      expiresAt,
      access_token: account.access_token || account.auth?.accessToken,
      refresh_token: account.refresh_token || account.auth?.refreshToken,
      id_token: account.id_token || account.auth?.idToken,
      last_refresh: account.last_refresh,
      expired: account.expired,
      token_generation: account.token_generation || 0,
      token_updated_at:
        Number(account.token_updated_at || 0) ||
        parseTimestamp(account.last_refresh)
    }

    if (
      !tokens.accessToken ||
      !tokens.expiresAt ||
      tokens.expiresAt <= Date.now()
    ) {
      if (!tokens.refreshToken) {
        const message =
          "Codex 登录授权缺少 refresh_token，无法自动续期；当前 access_token 已不可用。"
        this.markAccountReauth(account.id, missingRefreshTokenReason, message)
        this.markAccountRefreshError(account.id, 0, message)
        throw new Error(message)
      }

      try {
        tokens = {
          ...(await this.refreshToken(tokens.refreshToken, account.proxy)),
          token_generation: Number(account.token_generation || 0) + 1,
          token_updated_at: Date.now()
        }
      } catch (error) {
        if (shouldMarkReauth(error)) {
          this.markAccountReauth(
            account.id,
            error.oauthError || "invalid_grant",
            "Codex 登录授权已失效，请重新登录。"
          )
        }

        this.markAccountRefreshError(
          account.id,
          error.status || 0,
          error.message
        )

        throw error
      }
    }

    const claims = decodeJwtPayload(tokens.idToken || tokens.accessToken)
    claims.account_id =
      account.account_id || account.accountId || account.id || claims.account_id
    let usage

    try {
      usage = await this.fetchUsageInfo(
        tokens.accessToken,
        claims,
        account.proxy
      )
    } catch (error) {
      this.markAccountRefreshError(account.id, error.status || 0, error.message)

      throw error
    }
    const nextAccount = this.saveAccount(
      tokens,
      {
        email: extractEmail(claims) || account.email,
        sub: claims.sub || account.accountId
      },
      claims,
      usage,
      account.proxy
    )

    if (options.syncAuth !== false && nextAccount.id === this.activeAccountId) {
      await this.writeAccountBundle(nextAccount, cliTarget)
    }

    return nextAccount
  }

  disableAccount(accountId) {
    const account = this.accounts.find(item => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    this.clearScheduledRefresh(account.id)
    this.accounts = this.accounts.map(item =>
      item.id === account.id
        ? {
            ...item,
            autoRefresh: false,
            disabled: true,
            updatedAt: Date.now()
          }
        : item
    )

    if (this.activeAccountId === account.id) {
      this.activeAccountId = ""
      this.storage.scheduleWrite("codexActiveAccountId", this.activeAccountId)
    }

    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
  }

  restoreAccount(accountId) {
    const account = this.accounts.find(item => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    this.accounts = this.accounts.map(item =>
      item.id === account.id
        ? {
            ...item,
            disabled: false,
            updatedAt: Date.now()
          }
        : item
    )
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
  }

  async writeAccountBundle(account, cliTarget) {
    await this.writeAccountAuth(account, cliTarget)
    await this.writeCodexBuiltinConfig(cliTarget)
  }

  async writeAccountAuth(account, cliTarget) {
    if (!cliTarget?.configPath) {
      throw new Error("Codex CLI 配置目录不存在")
    }

    const accessToken = accountAccessToken(account)

    if (!accessToken) {
      throw new Error("OAuth 账号缺少 access_token，无法写入 auth.json")
    }

    await fs.mkdir(cliTarget.configPath, { recursive: true })
    await fs.writeFile(
      path.join(cliTarget.configPath, "auth.json"),
      `${JSON.stringify(
        {
          OPENAI_API_KEY: null,
          last_refresh: formatRfc3339(Date.now()),
          tokens: {
            access_token: accessToken,
            account_id: account.account_id || account.accountId || account.id,
            id_token: account.auth?.idToken || account.id_token,
            refresh_token: accountRefreshToken(account)
          }
        },
        null,
        2
      )}\n`,
      "utf8"
    )
  }

  async writeCodexBuiltinConfig(cliTarget) {
    const configPath = path.join(cliTarget.configPath, "config.toml")

    if (!(await pathExists(configPath))) {
      return
    }

    const content = await fs.readFile(configPath, "utf8")
    const withoutManagedProviders = removeTomlSections(
      content,
      new Set(["model_providers.custom", "model_providers.codex_local_access"])
    )
    const nextContent = removeTomlRootKeys(
      withoutManagedProviders,
      new Set(["model_provider", "openai_base_url"])
    )

    if (!nextContent) {
      await fs.rm(configPath, { force: true })
      return
    }

    await fs.writeFile(configPath, `${nextContent}\n`, "utf8")
  }

  clearActiveAccount() {
    this.activeAccountId = ""
    this.storage.scheduleWrite("codexActiveAccountId", this.activeAccountId)
    this.emit("changed", this.getState())
  }

  async deleteAccount(accountId, cliTarget) {
    const account = this.accounts.find(item => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    if (account.disabled) {
      throw new Error("Codex 官方账号已禁用，不能删除")
    }

    this.accounts = this.accounts.filter(item => item.id !== accountId)

    if (account.id === this.activeAccountId) {
      this.activeAccountId = ""
      this.storage.scheduleWrite("codexActiveAccountId", this.activeAccountId)
      await fs.rm(path.join(cliTarget.configPath, "auth.json"), {
        force: true
      })
    }

    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
  }

  updateAccountProxy(accountId, proxy, model) {
    const account = this.accounts.find(item => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    if (account.disabled) {
      throw new Error("Codex 官方账号已禁用，不能编辑")
    }

    const nextModel =
      model === undefined
        ? account.model || account.defaultModel || ""
        : String(model || "").trim()

    account.proxy = String(proxy || "").trim()
    account.model = nextModel
    account.defaultModel = account.model
    account.updatedAt = Date.now()
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
  }

  async completeLogin(requestUrl) {
    const code = requestUrl.searchParams.get("code")
    const state = requestUrl.searchParams.get("state")
    const error = requestUrl.searchParams.get("error")

    if (error) {
      throw new Error(`Codex 登录失败：${error}`)
    }

    if (!code) {
      throw new Error("Codex 登录回调缺少 authorization code")
    }

    if (!this.loginState || state !== this.loginState.state) {
      throw new Error("Codex 登录 state 校验失败")
    }

    const tokens = await this.exchangeCode(code)
    const claims = decodeJwtPayload(tokens.idToken || tokens.accessToken)
    const accountId = extractAccountId(claims)
    const targetAccountId = this.loginState.targetAccountId || ""
    const targetAccount = targetAccountId
      ? this.accounts.find(account => account.id === targetAccountId)
      : null

    if (targetAccountId && !targetAccount) {
      throw new Error("Codex 官方账号不存在")
    }

    if (targetAccount?.disabled) {
      throw new Error("Codex 官方账号已禁用，不能编辑")
    }

    if (
      targetAccount &&
      ![
        targetAccount.id,
        targetAccount.accountId,
        targetAccount.account_id
      ].includes(accountId)
    ) {
      throw new Error("登录账号与当前账号不一致")
    }

    if (
      accountId &&
      this.accounts.find(
        account =>
          account.id !== targetAccountId &&
          (account.id === accountId ||
            account.accountId === accountId ||
            account.account_id === accountId)
      )
    ) {
      throw new Error("此账户已导入")
    }

    const usage = await this.fetchUsageInfo(
      tokens.accessToken,
      claims,
      this.loginState.proxy
    )
    const profile = {
      email: extractEmail(claims),
      sub: claims.sub
    }
    const nextAccount = this.saveAccount(
      tokens,
      profile,
      claims,
      usage,
      this.loginState.proxy
    )

    if (
      nextAccount.id === this.activeAccountId &&
      typeof this.getCodexCliTarget === "function"
    ) {
      await this.writeAccountBundle(nextAccount, this.getCodexCliTarget())
    }

    this.loginState = {
      ...this.loginState,
      status: "success",
      message: "Codex 官方登录已完成",
      account: {
        id: nextAccount.id,
        email: nextAccount.email,
        plan: nextAccount.plan
      }
    }
    this.emit("login-state", this.getLoginState())
    this.stopCallbackServer()
  }

  async exchangeCode(code) {
    const response = await fetchWithProxy(
      TOKEN_URL,
      {
        method: "POST",
        headers: {
          accept: "application/json",
          "content-type": "application/x-www-form-urlencoded"
        },
        body: new URLSearchParams({
          grant_type: "authorization_code",
          code,
          client_id: CODEX_CLIENT_ID,
          redirect_uri: this.loginState.redirectUri,
          code_verifier: this.loginState.verifier
        })
      },
      this.loginState.proxy
    )
    const payload = await readJson(response)
    const now = Date.now()
    const expiresAt = now + Number(payload.expires_in || 86400) * 1000

    return {
      accessToken: payload.access_token,
      refreshToken: payload.refresh_token,
      idToken: payload.id_token,
      expiresAt,
      access_token: payload.access_token,
      refresh_token: payload.refresh_token,
      id_token: payload.id_token,
      last_refresh: formatRfc3339(now),
      expired: formatRfc3339(expiresAt)
    }
  }

  async refreshToken(refreshToken, proxy = "") {
    const response = await fetchWithProxy(
      TOKEN_URL,
      {
        method: "POST",
        headers: {
          accept: "application/json",
          "content-type": "application/x-www-form-urlencoded"
        },
        body: new URLSearchParams({
          grant_type: "refresh_token",
          refresh_token: refreshToken,
          client_id: CODEX_CLIENT_ID
        })
      },
      proxy
    )
    const text = await response.text()

    if (!response.ok) {
      const payload = JSON.parse(text)
      const error = new Error(
        `OpenAI 请求失败：${response.status} ${
          payload.error_description ||
          payload.error?.message ||
          payload.error?.code ||
          payload.error ||
          text
        }`
      )
      error.status = response.status
      error.oauthError = payload.error?.code || payload.error || ""
      error.oauthErrorDescription =
        payload.error_description || payload.error?.message || ""
      throw error
    }

    const payload = JSON.parse(text)
    const now = Date.now()
    const expiresAt = now + Number(payload.expires_in || 86400) * 1000
    const nextRefreshToken = payload.refresh_token || refreshToken

    return {
      accessToken: payload.access_token,
      refreshToken: nextRefreshToken,
      idToken: payload.id_token,
      expiresAt,
      access_token: payload.access_token,
      refresh_token: nextRefreshToken,
      id_token: payload.id_token,
      last_refresh: formatRfc3339(now),
      expired: formatRfc3339(expiresAt)
    }
  }

  async refreshExpiringAccounts() {
    const now = Date.now()
    const nextCheckAt = now + AUTO_REFRESH_INTERVAL
    const cliTarget =
      typeof this.getCodexCliTarget === "function"
        ? this.getCodexCliTarget()
        : null
    const accounts = []

    this.accounts.forEach(account => {
      if (
        !account.disabled &&
        account.autoRefresh !== false &&
        account.requires_reauth !== true &&
        account.auth?.refreshToken
      ) {
        const expiresAt = accountExpiresAt(account)

        if (!expiresAt || expiresAt <= now) {
          this.clearScheduledRefresh(account.id)
          accounts.push(account)
          return
        }

        if (expiresAt <= nextCheckAt) {
          this.scheduleAccountRefresh(account, expiresAt)
        }

        return
      }

      this.clearScheduledRefresh(account.id)
    })

    for (let index = 0; index < accounts.length; index += 3) {
      await Promise.all(
        accounts
          .slice(index, index + 3)
          .map(account => this.refreshAccount(account, cliTarget))
      )
    }
  }

  scheduleAccountRefresh(account, expiresAt) {
    const currentSchedule = this.autoRefreshTimers.get(account.id)

    if (currentSchedule?.expiresAt === expiresAt) {
      return
    }

    this.clearScheduledRefresh(account.id)

    const timer = setTimeout(
      () => {
        this.autoRefreshTimers.delete(account.id)
        const currentAccount = this.accounts.find(
          item => item.id === account.id
        )

        if (!currentAccount || accountExpiresAt(currentAccount) > Date.now()) {
          return
        }

        const cliTarget =
          typeof this.getCodexCliTarget === "function"
            ? this.getCodexCliTarget()
            : null

        this.refreshAccount(currentAccount, cliTarget).catch(error => {
          this.emit("login-state", {
            status: "failed",
            message: `Codex 自动刷新失败：${error.message || String(error)}`
          })
        })
      },
      Math.max(0, expiresAt - Date.now() + 1000)
    )

    this.autoRefreshTimers.set(account.id, {
      expiresAt,
      timer
    })
  }

  clearScheduledRefresh(accountId) {
    const schedule = this.autoRefreshTimers.get(accountId)

    if (!schedule) {
      return
    }

    clearTimeout(schedule.timer)
    this.autoRefreshTimers.delete(accountId)
  }

  async refreshAccount(account, cliTarget) {
    await this.performManagedTokenRefresh(account, cliTarget)
  }

  async getAccountDetail(accountId, cliTarget) {
    const account = this.accounts.find(item => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    const nextAccount = this.accounts.find(item => item.id === accountId)

    return {
      id: nextAccount.id,
      provider: nextAccount.provider,
      accountId: nextAccount.accountId,
      account_id: nextAccount.account_id,
      email: nextAccount.email,
      plan: nextAccount.plan,
      usage: nextAccount.usage,
      proxy: nextAccount.proxy,
      model: nextAccount.model || "",
      defaultModel: nextAccount.defaultModel || nextAccount.model || "",
      autoRefresh: nextAccount.autoRefresh,
      createdAt: nextAccount.createdAt,
      updatedAt: nextAccount.updatedAt,
      last_refresh: nextAccount.last_refresh,
      expired: nextAccount.expired,
      type: nextAccount.type,
      token_generation: nextAccount.token_generation || 0,
      token_updated_at: nextAccount.token_updated_at || 0,
      refresh_status: nextAccount.refresh_status || "",
      refresh_status_code: nextAccount.refresh_status_code || 0,
      refresh_message: nextAccount.refresh_message || "",
      requires_reauth: Boolean(nextAccount.requires_reauth),
      reauth_reason: nextAccount.reauth_reason || "",
      reauth_message: nextAccount.reauth_message || "",
      disabled: Boolean(nextAccount.disabled),
      active: nextAccount.id === this.activeAccountId,
      auth: nextAccount.auth
    }
  }

  async fetchUsageInfo(accessToken, claims, proxy = "") {
    const accountId = extractAccountId(claims)
    const headers = {
      "content-type": "application/json",
      "cache-control": "no-cache",
      authorization: `Bearer ${accessToken}`,
      "user-agent":
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
    }

    if (accountId) {
      headers["chatgpt-account-id"] = accountId
    }

    const response = await fetchWithProxy(
      CODEX_USAGE_URL,
      {
        headers
      },
      proxy
    )

    const usage = await readJson(response)
    console.log("[Codex 刷新额度接口返回]", JSON.stringify(usage, null, 2))
    return usage
  }
}

module.exports = {
  CodexAccountService,
  fetchWithProxy
}

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
const OAUTH_SCOPE =
  "openid profile email offline_access model.request model.read organization.write"
const AUTO_REFRESH_INTERVAL = 30 * 60 * 1000
const REFRESH_THRESHOLD = 10 * 60 * 1000
const DETAIL_REFRESH_THRESHOLD = 24 * 60 * 60 * 1000

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
    throw new Error(`OpenAI 请求失败：${response.status} ${text}`)
  }

  return JSON.parse(text)
}

async function fetchWithProxy(url, options, proxy) {
  if (!proxy) {
    return fetch(url, options)
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

class CodexAccountService extends EventEmitter {
  constructor(storage) {
    super()
    this.storage = storage
    this.accounts = []
    this.loginState = null
    this.loginServer = null
    this.autoRefreshTimer = null
    this.activeAccountId = ""
  }

  async init() {
    this.accounts = await this.storage.read("codexAccounts", [])
    this.activeAccountId = await this.storage.read("codexActiveAccountId", "")
  }

  startAutoRefresh() {
    if (this.autoRefreshTimer) {
      return
    }

    this.refreshExpiringAccounts().catch((error) => {
      this.emit("login-state", {
        status: "failed",
        message: `Codex 自动刷新失败：${error.message || String(error)}`
      })
    })

    this.autoRefreshTimer = setInterval(() => {
      this.refreshExpiringAccounts().catch((error) => {
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
  }

  getState() {
    return this.accounts.map((account) => ({
      id: account.id,
      provider: account.provider,
      accountId: account.accountId,
      account_id: account.account_id,
      email: account.email,
      plan: account.plan,
      usage: account.usage,
      proxy: account.proxy,
      autoRefresh: account.autoRefresh,
      createdAt: account.createdAt,
      updatedAt: account.updatedAt,
      last_refresh: account.last_refresh,
      expired: account.expired,
      type: account.type,
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
    const tokenSource = authData.tokens || authData
    const refreshToken =
      tokenSource.refresh_token || tokenSource.refreshToken || ""

    if (!refreshToken) {
      throw new Error("Codex 登录 JSON 数据缺少 refresh_token")
    }

    const proxy = String(input.proxy || "").trim()
    const tokens = await this.refreshToken(refreshToken, proxy)
    const claims = decodeJwtPayload(
      tokenSource.id_token ||
        tokenSource.idToken ||
        tokenSource.id_otkne ||
        tokens.idToken ||
        tokens.accessToken
    )
    claims.sub = tokenSource.account_id || claims.sub
    claims.account_id = tokenSource.account_id || claims.account_id
    const accountId = extractAccountId(claims)

    if (
      accountId &&
      this.accounts.find(
        (account) =>
          account.id === accountId ||
          account.accountId === accountId ||
          account.account_id === accountId
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
          response.end("Codex 登录已完成，可以返回 AI Manager。")
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
      this.accounts.find((account) => account.id === accountId) || null
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
      id_token: tokens.id_token,
      access_token: tokens.access_token,
      refresh_token: tokens.refresh_token,
      last_refresh: tokens.last_refresh,
      expired: tokens.expired,
      auth: tokens,
      autoRefresh: currentAccount?.autoRefresh !== false,
      createdAt: currentAccount?.createdAt || Date.now(),
      updatedAt: Date.now()
    }

    this.accounts = [
      ...this.accounts.filter((account) => account.id !== accountId),
      nextAccount
    ]
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
    return nextAccount
  }

  async enableAccount(accountId, cliTarget) {
    const account = this.accounts.find((item) => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    await this.writeAccountAuth(account, cliTarget)

    this.activeAccountId = account.id
    this.storage.scheduleWrite("codexActiveAccountId", this.activeAccountId)
    this.emit("changed", this.getState())
  }

  async refreshAccountUsage(accountId, cliTarget) {
    const account = this.accounts.find((item) => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    const expiresAt =
      account.auth?.expiresAt || Date.parse(account.expired) || 0
    let tokens = {
      accessToken: account.auth?.accessToken || account.access_token,
      refreshToken: account.auth?.refreshToken || account.refresh_token,
      idToken: account.auth?.idToken || account.id_token,
      expiresAt,
      access_token: account.access_token || account.auth?.accessToken,
      refresh_token: account.refresh_token || account.auth?.refreshToken,
      id_token: account.id_token || account.auth?.idToken,
      last_refresh: account.last_refresh,
      expired: account.expired
    }

    if (!tokens.refreshToken) {
      throw new Error("Codex 官方账号缺少 refresh_token")
    }

    if (
      !tokens.accessToken ||
      !tokens.expiresAt ||
      tokens.expiresAt <= Date.now()
    ) {
      tokens = await this.refreshToken(tokens.refreshToken, account.proxy)
    }

    const claims = decodeJwtPayload(tokens.idToken || tokens.accessToken)
    claims.account_id = account.account_id || claims.account_id
    const usage = await this.fetchUsageInfo(
      tokens.accessToken,
      claims,
      account.proxy
    )
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

    if (nextAccount.id === this.activeAccountId) {
      await this.writeAccountAuth(nextAccount, cliTarget)
    }

    return nextAccount
  }

  async writeAccountAuth(account, cliTarget) {
    if (!cliTarget?.configPath) {
      throw new Error("Codex CLI 配置目录不存在")
    }

    await fs.mkdir(cliTarget.configPath, { recursive: true })
    await fs.writeFile(
      path.join(cliTarget.configPath, "auth.json"),
      `${JSON.stringify(
        {
          id_token: account.id_token,
          access_token: account.access_token,
          refresh_token: account.refresh_token,
          account_id: account.account_id,
          last_refresh: account.last_refresh,
          email: account.email,
          type: "codex",
          expired: account.expired
        },
        null,
        2
      )}\n`,
      "utf8"
    )
  }

  clearActiveAccount() {
    this.activeAccountId = ""
    this.storage.scheduleWrite("codexActiveAccountId", this.activeAccountId)
    this.emit("changed", this.getState())
  }

  updateAccountProxy(accountId, proxy) {
    const account = this.accounts.find((item) => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    account.proxy = String(proxy || "").trim()
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

    if (
      accountId &&
      this.accounts.find(
        (account) =>
          account.id === accountId ||
          account.accountId === accountId ||
          account.account_id === accountId
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
    const payload = await readJson(response)
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
    const threshold = Date.now() + REFRESH_THRESHOLD
    const accounts = this.accounts.filter((account) => {
      return (
        account.autoRefresh !== false &&
        account.auth?.refreshToken &&
        (!account.auth.expiresAt || account.auth.expiresAt <= threshold)
      )
    })

    for (let index = 0; index < accounts.length; index += 3) {
      await Promise.all(
        accounts
          .slice(index, index + 3)
          .map((account) => this.refreshAccount(account))
      )
    }
  }

  async refreshAccount(account) {
    const tokens = await this.refreshToken(
      account.auth.refreshToken,
      account.proxy
    )
    const claims = decodeJwtPayload(tokens.idToken || tokens.accessToken)
    const usage = await this.fetchUsageInfo(
      tokens.accessToken,
      claims,
      account.proxy
    )
    const profile = {
      email: extractEmail(claims),
      sub: claims.sub
    }

    this.saveAccount(tokens, profile, claims, usage, account.proxy)
  }

  async getAccountDetail(accountId, cliTarget) {
    const account = this.accounts.find((item) => item.id === accountId)

    if (!account) {
      throw new Error("Codex 官方账号不存在")
    }

    const expiresAt =
      account.auth?.expiresAt || Date.parse(account.expired) || 0
    const shouldRefresh =
      expiresAt && expiresAt - Date.now() <= DETAIL_REFRESH_THRESHOLD

    if (shouldRefresh) {
      const tokens = await this.refreshToken(
        account.auth.refreshToken,
        account.proxy
      )
      const claims = decodeJwtPayload(tokens.idToken || tokens.accessToken)
      claims.account_id = account.account_id || claims.account_id
      const usage = await this.fetchUsageInfo(
        tokens.accessToken,
        claims,
        account.proxy
      )
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

      if (nextAccount.id === this.activeAccountId) {
        await this.writeAccountAuth(nextAccount, cliTarget)
      }
    }

    const nextAccount = this.accounts.find((item) => item.id === accountId)

    return {
      id: nextAccount.id,
      provider: nextAccount.provider,
      accountId: nextAccount.accountId,
      account_id: nextAccount.account_id,
      email: nextAccount.email,
      plan: nextAccount.plan,
      usage: nextAccount.usage,
      proxy: nextAccount.proxy,
      autoRefresh: nextAccount.autoRefresh,
      createdAt: nextAccount.createdAt,
      updatedAt: nextAccount.updatedAt,
      last_refresh: nextAccount.last_refresh,
      expired: nextAccount.expired,
      type: nextAccount.type,
      active: nextAccount.id === this.activeAccountId,
      auth: nextAccount.auth
    }
  }

  async fetchUsageInfo(accessToken, claims, proxy = "") {
    const accountId = extractAccountId(claims)
    const headers = {
      "content-type": "application/json",
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

    return readJson(response)
  }
}

module.exports = {
  CodexAccountService
}

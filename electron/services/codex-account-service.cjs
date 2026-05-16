const crypto = require("node:crypto")
const http = require("node:http")
const { EventEmitter } = require("node:events")
const { shell } = require("electron")

const OAUTH_BASE_URL = "https://auth.openai.com"
const TOKEN_URL = `${OAUTH_BASE_URL}/oauth/token`
const PROFILE_URL = "https://api.openai.com/profile"
const CODEX_CLIENT_ID = "app_EMoamEEZ73f0CkXaXp7hrann"
const OAUTH_REDIRECT_URI = "http://localhost:1455/auth/callback"
const OAUTH_SCOPE = "openid email profile offline_access"

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

async function readJson(response) {
  const text = await response.text()

  if (!response.ok) {
    throw new Error(`OpenAI 请求失败：${response.status} ${text}`)
  }

  return JSON.parse(text)
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

class CodexAccountService extends EventEmitter {
  constructor(storage) {
    super()
    this.storage = storage
    this.accounts = []
    this.loginState = null
    this.loginServer = null
  }

  async init() {
    this.accounts = await this.storage.read("codexAccounts", [])
  }

  getState() {
    return this.accounts.map((account) => ({
      ...account,
      auth: {
        expiresAt: account.auth?.expiresAt || 0
      }
    }))
  }

  async startLogin() {
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
    const profile = await this.fetchProfile(tokens.accessToken)
    const claims = decodeJwtPayload(tokens.accessToken)
    const accountId =
      profile.user_id || profile.sub || claims.sub || createId("codex-account")
    const nextAccount = {
      id: accountId,
      provider: "codex",
      email: profile.email || claims.email || "未识别账号",
      plan: profile.chatgpt_plan_type || claims.chatgpt_plan_type || "",
      auth: tokens,
      createdAt:
        this.accounts.find((account) => account.id === accountId)?.createdAt ||
        Date.now(),
      updatedAt: Date.now()
    }

    this.accounts = [
      ...this.accounts.filter((account) => account.id !== accountId),
      nextAccount
    ]
    this.storage.scheduleWrite("codexAccounts", this.accounts)
    this.emit("changed", this.getState())
    this.loginState = {
      ...this.loginState,
      status: "success",
      message: "Codex 官方登录已完成",
      account: {
        id: accountId,
        email: nextAccount.email,
        plan: nextAccount.plan
      }
    }
    this.emit("login-state", this.getLoginState())
    this.stopCallbackServer()
  }

  async exchangeCode(code) {
    const response = await fetch(TOKEN_URL, {
      method: "POST",
      headers: {
        "content-type": "application/x-www-form-urlencoded"
      },
      body: new URLSearchParams({
        grant_type: "authorization_code",
        code,
        client_id: CODEX_CLIENT_ID,
        redirect_uri: this.loginState.redirectUri,
        code_verifier: this.loginState.verifier
      })
    })
    const payload = await readJson(response)

    return {
      accessToken: payload.access_token,
      refreshToken: payload.refresh_token,
      expiresAt: Date.now() + Number(payload.expires_in || 0) * 1000
    }
  }

  async fetchProfile(accessToken) {
    const response = await fetch(PROFILE_URL, {
      headers: {
        authorization: `Bearer ${accessToken}`
      }
    })

    return readJson(response)
  }
}

module.exports = {
  CodexAccountService
}

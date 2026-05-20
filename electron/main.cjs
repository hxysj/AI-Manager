const path = require("node:path")
const fs = require("node:fs")
const os = require("node:os")
const fsp = require("node:fs/promises")
const {
  app,
  BrowserWindow,
  Menu,
  Tray,
  nativeImage,
  screen,
  ipcMain,
  shell,
  dialog
} = require("electron")
const { ManagerService } = require("./services/manager-service.cjs")
const {
  resolvePortablePath,
  serializeAppSettingsPaths
} = require("./services/path-utils.cjs")
const { TranslationService } = require("./services/translation-service.cjs")

let mainWindow = null
let quickSwitchWindow = null
let tray = null
let managerService = null
let translationService = null
let managerReadyPromise = null
let isQuitting = false
let closeDialogOpen = false
let quickSwitchCollapsed = false
const defaultUserDataPath = "D:\\ai-manager-data"
const settingsFilePath = path.join(defaultUserDataPath, "app-settings.json")
const appIconPath = app.isPackaged
  ? path.join(process.resourcesPath, "assets", "icon.png")
  : path.join(__dirname, "..", "build", "icon.png")
app.setAppUserModelId("com.monkeythief.desktop")
const quickSwitchExpandedSize = { width: 360, height: 238 }
const quickSwitchCollapsedSize = { width: 44, height: 44 }
const portableHomePrefix = path.join(path.dirname(os.homedir()), "%USERNAME%")
const defaultCliConfigPaths = {
  claude: path.join(portableHomePrefix, ".claude"),
  codex: path.join(portableHomePrefix, ".codex"),
  gemini: path.join(portableHomePrefix, ".gemini")
}
const defaultCloudSyncSettings = {
  provider: "jianguoyun",
  webdavUrl: "https://dav.jianguoyun.com/dav/AI-Manager",
  username: "",
  password: "",
  fileName: "ai-manager.aimbackup",
  lastUpdatedAt: 0
}
const defaultSystemSettings = {
  closeAction: "ask",
  quickSwitchVisible: true
}

function normalizeSystemSettings(input = {}) {
  const closeAction = String(
    input.closeAction || defaultSystemSettings.closeAction
  )

  return {
    closeAction: ["ask", "minimize", "quit"].includes(closeAction)
      ? closeAction
      : defaultSystemSettings.closeAction,
    quickSwitchVisible:
      input.quickSwitchVisible === undefined
        ? defaultSystemSettings.quickSwitchVisible
        : Boolean(input.quickSwitchVisible)
  }
}

function normalizeCloudSyncSettings(input = {}) {
  return {
    provider: "jianguoyun",
    webdavUrl: String(
      input.webdavUrl || defaultCloudSyncSettings.webdavUrl
    ).trim(),
    username: String(input.username || "").trim(),
    password: String(input.password || ""),
    fileName: String(
      input.fileName || defaultCloudSyncSettings.fileName
    ).trim(),
    lastUpdatedAt: Number(input.lastUpdatedAt || 0)
  }
}

function normalizeAppSettings(input = {}) {
  const cliConfigPaths = input.cliConfigPaths || {}
  return {
    dataPath: resolvePortablePath(
      String(input.dataPath || defaultUserDataPath).trim()
    ),
    defaultDataPath: defaultUserDataPath,
    settingsFilePath,
    cliConfigPaths: {
      claude: resolvePortablePath(
        String(cliConfigPaths.claude || defaultCliConfigPaths.claude).trim()
      ),
      codex: resolvePortablePath(
        String(cliConfigPaths.codex || defaultCliConfigPaths.codex).trim()
      ),
      gemini: resolvePortablePath(
        String(cliConfigPaths.gemini || defaultCliConfigPaths.gemini).trim()
      )
    },
    defaultCliConfigPaths,
    cloudSync: normalizeCloudSyncSettings(input.cloudSync),
    system: normalizeSystemSettings(input.system)
  }
}

function encodeWebDavPath(value) {
  return String(value || "")
    .split("/")
    .filter(Boolean)
    .map(item => encodeURIComponent(item))
    .join("/")
}

function buildWebDavFileUrl(config) {
  const rootUrl = config.webdavUrl.endsWith("/")
    ? config.webdavUrl
    : `${config.webdavUrl}/`

  return new URL(encodeWebDavPath(config.fileName), rootUrl).toString()
}

function buildWebDavAuthHeader(config) {
  return `Basic ${Buffer.from(
    `${config.username}:${config.password}`,
    "utf8"
  ).toString("base64")}`
}

async function ensureWebDavDirectory(config) {
  const response = await fetch(config.webdavUrl, {
    method: "MKCOL",
    headers: {
      Authorization: buildWebDavAuthHeader(config)
    }
  })

  if (![201, 405].includes(response.status)) {
    throw new Error(`坚果云目录创建失败：${response.status}`)
  }
}

async function uploadWebDavBackup(config, content) {
  await ensureWebDavDirectory(config)

  const response = await fetch(buildWebDavFileUrl(config), {
    method: "PUT",
    headers: {
      Authorization: buildWebDavAuthHeader(config),
      "Content-Type": "application/octet-stream"
    },
    body: content
  })

  if (![200, 201, 204].includes(response.status)) {
    throw new Error(`坚果云上传失败：${response.status}`)
  }
}

async function downloadWebDavBackup(config) {
  const response = await fetch(buildWebDavFileUrl(config), {
    method: "GET",
    headers: {
      Authorization: buildWebDavAuthHeader(config)
    }
  })

  if (response.status === 404) {
    throw new Error("坚果云上未找到配置备份")
  }

  if (response.status !== 200) {
    throw new Error(`坚果云下载失败：${response.status}`)
  }

  return response.text()
}

function loadAppSettings() {
  try {
    return normalizeAppSettings(
      JSON.parse(fs.readFileSync(settingsFilePath, "utf8"))
    )
  } catch {
    return normalizeAppSettings()
  }
}

function saveAppSettings(nextSettings) {
  fs.mkdirSync(path.dirname(settingsFilePath), { recursive: true })
  fs.writeFileSync(
    settingsFilePath,
    `${JSON.stringify(serializeAppSettingsPaths(nextSettings), null, 2)}\n`,
    "utf8"
  )
}

function sendStateChanged(state) {
  for (const targetWindow of [mainWindow, quickSwitchWindow]) {
    if (targetWindow && !targetWindow.isDestroyed()) {
      targetWindow.webContents.send("state:changed", state)
    }
  }
}

async function restartManagerService(nextSettings = appSettings) {
  if (managerService) {
    managerService.removeAllListeners()
    await managerService.dispose()
  }

  managerService = new ManagerService(app.getPath("userData"), nextSettings)
  managerReadyPromise = managerService.init()
  await managerReadyPromise
  managerService.on("state-changed", state => {
    sendStateChanged(state)
    updateTrayMenu(state)
  })

  return managerService.getState()
}

async function showMainPanel() {
  if (!mainWindow || mainWindow.isDestroyed()) {
    await createWindow()
  }

  if (mainWindow.isMinimized()) {
    mainWindow.restore()
  }

  mainWindow.show()
  mainWindow.focus()
}

function positionQuickSwitchWindow() {
  if (!quickSwitchWindow || quickSwitchWindow.isDestroyed()) {
    return
  }

  const display = screen.getPrimaryDisplay()
  const { workArea } = display
  const [width, height] = quickSwitchWindow.getSize()
  const x = Math.round(workArea.x + workArea.width - width - 16)
  const y = Math.round(workArea.y + workArea.height - height - 12)

  quickSwitchWindow.setPosition(x, y, false)
}

function setQuickSwitchCollapsed(collapsed) {
  quickSwitchCollapsed = Boolean(collapsed)

  if (!quickSwitchWindow || quickSwitchWindow.isDestroyed()) {
    return
  }

  const size = quickSwitchCollapsed
    ? quickSwitchCollapsedSize
    : quickSwitchExpandedSize
  const bounds = quickSwitchWindow.getBounds()
  const display = screen.getDisplayMatching(bounds)
  const { workArea } = display
  const nextX = bounds.x + bounds.width - size.width
  const nextY = bounds.y + bounds.height - size.height
  const x = Math.min(
    Math.max(nextX, workArea.x),
    workArea.x + workArea.width - size.width
  )
  const y = Math.min(
    Math.max(nextY, workArea.y),
    workArea.y + workArea.height - size.height
  )
  quickSwitchWindow.setResizable(true)
  quickSwitchWindow.setMinimumSize(1, 1)
  quickSwitchWindow.setMaximumSize(workArea.width, workArea.height)
  quickSwitchWindow.setBounds({ x, y, width: size.width, height: size.height }, false)
  quickSwitchWindow.setMinimumSize(size.width, size.height)
  quickSwitchWindow.setMaximumSize(size.width, size.height)
  quickSwitchWindow.setResizable(false)
}

function moveQuickSwitchWindowBy(payload) {
  if (!quickSwitchWindow || quickSwitchWindow.isDestroyed()) {
    return
  }

  const bounds = quickSwitchWindow.getBounds()
  quickSwitchWindow.setPosition(
    bounds.x + Math.round(Number(payload?.x || 0)),
    bounds.y + Math.round(Number(payload?.y || 0)),
    false
  )
}

async function syncQuickSwitchWindow() {
  const shouldShowQuickSwitchWindow =
    appSettings.system.quickSwitchVisible &&
    mainWindow &&
    !mainWindow.isDestroyed() &&
    (!mainWindow.isVisible() || mainWindow.isMinimized())

  if (shouldShowQuickSwitchWindow) {
    await createQuickSwitchWindow()
    return
  }

  if (quickSwitchWindow && !quickSwitchWindow.isDestroyed()) {
    quickSwitchWindow.destroy()
    quickSwitchWindow = null
  }
}

function requestCloseAction() {
  if (closeDialogOpen) {
    return
  }

  closeDialogOpen = true
  mainWindow.webContents.send("app:close-requested")
}

function handleCloseAction(payload = {}) {
  closeDialogOpen = false
  const action = String(payload.action || "cancel")

  if (payload.remember && action !== "cancel") {
    appSettings = normalizeAppSettings({
      ...appSettings,
      system: {
        closeAction: action
      }
    })
    saveAppSettings(appSettings)
    managerService.setAppSettings(appSettings)
  }

  if (action === "minimize") {
    mainWindow.hide()
    return
  }

  if (action === "quit") {
    isQuitting = true
    app.quit()
  }
}

function formatTrayPlanName(value) {
  if (value === "pro") {
    return "Pro"
  }

  if (value === "plus") {
    return "Plus"
  }

  return value || "未知套餐"
}

function formatTrayQuotaWindowName(value) {
  const seconds = Number(value || 0)

  if (seconds === 18000) {
    return "5小时"
  }

  if (seconds === 604800) {
    return "周"
  }

  if (seconds % 86400 === 0) {
    return `${seconds / 86400}天`
  }

  if (seconds % 3600 === 0) {
    return `${seconds / 3600}小时`
  }

  return `${seconds}秒`
}

function formatTrayQuota(account) {
  const rateLimit = account.usage?.rate_limit
  const windows = [
    rateLimit?.primary_window,
    rateLimit?.secondary_window
  ].filter(Boolean)

  if (!windows.length) {
    return "额度未知"
  }

  return windows
    .map(window => {
      const remaining = Math.max(0, 100 - Number(window.used_percent || 0))
      return `${formatTrayQuotaWindowName(
        window.limit_window_seconds
      )}剩余 ${remaining}%`
    })
    .join(" / ")
}

function formatTrayAccountLabel(account) {
  return `${account.email || account.accountId || "Codex 账号"} · ${formatTrayPlanName(
    account.plan
  )} · ${formatTrayQuota(account)}`
}

function findRuntimeModel(state, provider, profile) {
  return (
    provider.runtimeConfig?.mainModel ||
    profile?.model ||
    state.runtimeModels.find(item => item.providerId === provider.id)?.name ||
    ""
  )
}

function getVisibleCliTargets(state) {
  return state.cliTargets.filter(item => {
    return state.runtimeConfigSchemas[item.id]?.enabled
  })
}

function showTrayError(error) {
  const message = error?.message || String(error)

  if (tray && process.platform === "win32") {
    tray.displayBalloon({
      title: "操作失败",
      content: message
    })
    return
  }

  dialog.showErrorBox("操作失败", message)
}

async function runTrayAction(action) {
  try {
    await managerReadyPromise
    const state = await action()
    updateTrayMenu(state || managerService.getState())
  } catch (error) {
    showTrayError(error)
  }
}

function buildProviderTrayItems(state, cli) {
  const providers = state.providers.filter(item => {
    return item.cli === cli.id && item.enabled !== false
  })
  const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
  const activeProvider = providers.find(item => item.id === profile?.providerId)
  const items = [
    {
      label: `当前 Provider：${activeProvider?.name || "未启用"}`,
      enabled: false
    }
  ]

  if (activeProvider) {
    items.push({
      label: "取消启用 Provider",
      click: () =>
        runTrayAction(() => {
          return managerService.clearRuntime(cli.id)
        })
    })
  }

  items.push({ type: "separator" })

  if (!providers.length) {
    items.push({
      label: "暂无可用 Provider",
      enabled: false
    })
    return items
  }

  for (const provider of providers) {
    const model = findRuntimeModel(state, provider, profile)
    const active = provider.id === profile?.providerId

    items.push({
      label: active
        ? `${provider.name}（已启用）`
        : model
          ? provider.name
          : `${provider.name}（缺少模型）`,
      enabled: Boolean(model),
      submenu: active
        ? [
            {
              label: "取消启用",
              click: () =>
                runTrayAction(() => {
                  return managerService.clearRuntime(cli.id)
                })
            }
          ]
        : [
            {
              label: "启用",
              click: () =>
                runTrayAction(() => {
                  return managerService.switchRuntime({
                    cli: cli.id,
                    providerId: provider.id,
                    model
                  })
                })
            }
          ]
    })
  }

  return items
}

function buildCodexAccountTrayItems(state) {
  const accounts = state.codexAccounts || []
  const activeAccount = accounts.find(item => item.active)
  const items = [
    {
      label: `当前官方账号：${
        activeAccount?.email || activeAccount?.accountId || "未启用"
      }`,
      enabled: false
    }
  ]

  if (activeAccount) {
    items.push(
      {
        label: "刷新当前账号额度",
        click: () =>
          runTrayAction(() => {
            return managerService.refreshCodexAccount({
              accountId: activeAccount.id,
              syncAuth: false
            })
          })
      },
      {
        label: "取消启用官方账号",
        click: () =>
          runTrayAction(() => {
            return managerService.clearCodexAccount()
          })
      }
    )
  }

  items.push({ type: "separator" })

  if (!accounts.length) {
    items.push({
      label: "暂无 Codex 官方账号",
      enabled: false
    })
    return items
  }

  for (const account of accounts) {
    items.push({
      label: account.active
        ? `${formatTrayAccountLabel(account)}（已启用）`
        : formatTrayAccountLabel(account),
      submenu: account.active
        ? [
            {
              label: "取消启用",
              click: () =>
                runTrayAction(() => {
                  return managerService.clearCodexAccount()
                })
            }
          ]
        : [
            {
              label: "启用",
              click: () =>
                runTrayAction(() => {
                  return managerService.enableCodexAccount({
                    accountId: account.id
                  })
                })
            }
          ]
    })
  }

  return items
}

function buildCliTrayItems(state) {
  const cliTargets = getVisibleCliTargets(state)

  if (!cliTargets.length) {
    return [
      {
        label: "暂无可用 CLI",
        enabled: false
      }
    ]
  }

  return cliTargets.map(cli => {
    const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
    const provider = state.providers.find(
      item => item.id === profile?.providerId
    )
    const submenu = [
      {
        label: `当前应用：${cli.name}`,
        enabled: false
      },
      { type: "separator" },
      ...buildProviderTrayItems(state, cli)
    ]

    if (cli.id === "codex") {
      submenu.push(
        { type: "separator" },
        {
          label: "Codex 官方账号",
          submenu: buildCodexAccountTrayItems(state)
        }
      )
    }

    return {
      label: `${cli.name}：${provider?.name || "未启用"}`,
      submenu
    }
  })
}

function buildQuickSwitchTrayItems(state) {
  const cliTargets = getVisibleCliTargets(state)

  if (!cliTargets.length) {
    return [
      {
        label: "暂无可用 CLI",
        enabled: false
      }
    ]
  }

  return cliTargets.map(cli => {
    const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
    const provider = state.providers.find(
      item => item.id === profile?.providerId
    )
    const submenu = [
      {
        label: `当前应用：${cli.name}`,
        enabled: false
      },
      { type: "separator" },
      ...buildProviderTrayItems(state, cli)
    ]

    if (cli.id === "codex") {
      submenu.push({ type: "separator" }, ...buildCodexAccountTrayItems(state))
    }

    return {
      label: `${cli.name}：${provider?.name || "未启用"}`,
      submenu
    }
  })
}

function getCliTrayActiveName(state, cli) {
  if (cli.id === "codex") {
    const activeAccount = (state.codexAccounts || []).find(item => item.active)

    if (activeAccount) {
      return activeAccount.email || activeAccount.accountId || "Codex 官方账号"
    }
  }

  const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
  const provider = state.providers.find(item => item.id === profile?.providerId)

  return provider?.name || "未启用"
}

function buildUnifiedCodexTrayItems(state, cli) {
  const providers = state.providers.filter(item => {
    return item.cli === cli.id && item.enabled !== false
  })
  const accounts = state.codexAccounts || []
  const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
  const activeProvider = providers.find(item => item.id === profile?.providerId)
  const activeAccount = accounts.find(item => item.active)
  const items = [
    {
      label: `当前启用：${getCliTrayActiveName(state, cli)}`,
      enabled: false
    }
  ]

  if (activeAccount) {
    items.push({
      label: "刷新当前账号额度",
      click: () =>
        runTrayAction(() => {
          return managerService.refreshCodexAccount({
            accountId: activeAccount.id,
            syncAuth: false
          })
        })
    })
  }

  if (activeProvider || activeAccount) {
    items.push({
      label: "取消启用",
      click: () =>
        runTrayAction(async () => {
          await managerService.clearRuntime(cli.id)
          return managerService.clearCodexAccount()
        })
    })
  }

  items.push({ type: "separator" })

  if (!providers.length && !accounts.length) {
    items.push({
      label: "暂无可用 Provider 或官方账号",
      enabled: false
    })
    return items
  }

  for (const provider of providers) {
    const model = findRuntimeModel(state, provider, profile)
    const active = !activeAccount && provider.id === activeProvider?.id

    items.push({
      label: active
        ? `${provider.name}（已启用）`
        : model
          ? provider.name
          : `${provider.name}（缺少模型）`,
      enabled: Boolean(model),
      submenu: active
        ? [
            {
              label: "取消启用",
              click: () =>
                runTrayAction(async () => {
                  await managerService.clearCodexAccount()
                  return managerService.clearRuntime(cli.id)
                })
            }
          ]
        : [
            {
              label: "启用",
              click: () =>
                runTrayAction(async () => {
                  await managerService.clearCodexAccount()
                  return managerService.switchRuntime({
                    cli: cli.id,
                    providerId: provider.id,
                    model
                  })
                })
            }
          ]
    })
  }

  for (const account of accounts) {
    items.push({
      label: account.active
        ? `${formatTrayAccountLabel(account)}（已启用）`
        : formatTrayAccountLabel(account),
      submenu: account.active
        ? [
            {
              label: "刷新额度",
              click: () =>
                runTrayAction(() => {
                  return managerService.refreshCodexAccount({
                    accountId: account.id,
                    syncAuth: false
                  })
                })
            },
            {
              label: "取消启用",
              click: () =>
                runTrayAction(async () => {
                  await managerService.clearRuntime(cli.id)
                  return managerService.clearCodexAccount()
                })
            }
          ]
        : [
            {
              label: "启用",
              click: () =>
                runTrayAction(async () => {
                  await managerService.clearRuntime(cli.id)
                  return managerService.enableCodexAccount({
                    accountId: account.id
                  })
                })
            }
          ]
    })
  }

  return items
}

function buildUnifiedQuickSwitchTrayItems(state) {
  const cliTargets = getVisibleCliTargets(state)

  if (!cliTargets.length) {
    return [
      {
        label: "暂无可用 CLI",
        enabled: false
      }
    ]
  }

  return cliTargets.map(cli => {
    const submenu =
      cli.id === "codex"
        ? [
            {
              label: `当前应用：${cli.name}`,
              enabled: false
            },
            { type: "separator" },
            ...buildUnifiedCodexTrayItems(state, cli)
          ]
        : [
            {
              label: `当前应用：${cli.name}`,
              enabled: false
            },
            { type: "separator" },
            ...buildProviderTrayItems(state, cli)
          ]

    return {
      label: `${cli.name}：${getCliTrayActiveName(state, cli)}`,
      submenu
    }
  })
}

function buildTrayTooltip(state) {
  const lines = ["Monkey Thief"]

  for (const cli of getVisibleCliTargets(state)) {
    const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
    const provider = state.providers.find(
      item => item.id === profile?.providerId
    )

    lines.push(`${cli.name}: ${provider?.name || "未启用"}`)
  }

  return lines.join("\n")
}

function buildUnifiedTrayTooltip(state) {
  const lines = ["Monkey Thief"]

  for (const cli of getVisibleCliTargets(state)) {
    lines.push(`${cli.name}: ${getCliTrayActiveName(state, cli)}`)
  }

  return lines.join("\n")
}

function getTrayStatusTarget(state) {
  const cliTargets = getVisibleCliTargets(state)

  for (const cli of cliTargets) {
    const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
    const provider = state.providers.find(
      item => item.id === profile?.providerId
    )

    if (provider) {
      return {
        cli,
        provider
      }
    }
  }

  return {
    cli: cliTargets[0],
    provider: null
  }
}

function getUnifiedTrayStatusTarget(state) {
  const cliTargets = getVisibleCliTargets(state)

  for (const cli of cliTargets) {
    if (cli.id === "codex") {
      const account = (state.codexAccounts || []).find(item => item.active)

      if (account) {
        return {
          cli,
          provider: null,
          account
        }
      }
    }

    const profile = state.runtimeProfiles.find(item => item.cli === cli.id)
    const provider = state.providers.find(
      item => item.id === profile?.providerId
    )

    if (provider) {
      return {
        cli,
        provider,
        account: null
      }
    }
  }

  return {
    cli: cliTargets[0],
    provider: null,
    account: null
  }
}

function getTrayStatusText(value) {
  const text = String(value || "")
    .replace(/[^a-zA-Z0-9\u4e00-\u9fa5]/g, "")
    .trim()

  if (!text) {
    return "AI"
  }

  if (/^[a-zA-Z0-9]+$/.test(text)) {
    return text.slice(0, 2).toUpperCase()
  }

  return text.slice(0, 2)
}

function createTrayStatusImage(state) {
  if (!state) {
    return nativeImage.createFromPath(appIconPath)
  }

  const target = getUnifiedTrayStatusTarget(state)
  const label = getTrayStatusText(
    target.account?.email || target.provider?.name || target.cli?.name
  )
  const cliLabel = getTrayStatusText(target.cli?.name).slice(0, 1)
  const background = target.cli?.id === "codex" ? "#111827" : "#1682ff"
  const svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">
  <rect width="32" height="32" rx="7" fill="${background}"/>
  <text x="16" y="20" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="13" font-weight="700" fill="#ffffff">${label}</text>
  <circle cx="25" cy="25" r="6" fill="#ffffff"/>
  <text x="25" y="29" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="8" font-weight="700" fill="${background}">${cliLabel}</text>
</svg>`

  return nativeImage.createFromDataURL(
    `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`
  )
}

function updateTrayMenu(state = managerService?.getState()) {
  if (!tray || !state) {
    return
  }

  tray.setImage(createTrayStatusImage(state))
  tray.setToolTip(buildUnifiedTrayTooltip(state))
  tray.setContextMenu(
    Menu.buildFromTemplate([
      {
        label: "打开主面板",
        click: async () => {
          await showMainPanel()
        }
      },
      { type: "separator" },
      {
        label: "Provider 快速切换",
        submenu: buildUnifiedQuickSwitchTrayItems(state)
      },
      { type: "separator" },
      {
        label: "退出",
        click: () => {
          isQuitting = true
          app.quit()
        }
      }
    ])
  )
}

function createTray() {
  if (tray) {
    return
  }

  tray = new Tray(appIconPath)
  tray.setToolTip("Monkey Thief")
  tray.setContextMenu(
    Menu.buildFromTemplate([
      {
        label: "打开主面板",
        click: async () => {
          await showMainPanel()
        }
      },
      {
        label: "退出",
        click: () => {
          isQuitting = true
          app.quit()
        }
      }
    ])
  )
  updateTrayMenu()
  tray.on("double-click", async () => {
    await showMainPanel()
  })
}

let appSettings = loadAppSettings()
const userDataPath = appSettings.dataPath

fs.mkdirSync(userDataPath, { recursive: true })
app.setPath("userData", userDataPath)

async function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 760,
    minWidth: 1200,
    minHeight: 760,
    icon: appIconPath,
    backgroundColor: "#ffffff",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "default",
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      devTools: Boolean(process.env.VITE_DEV_SERVER_URL)
    }
  })

  const devServerUrl = process.env.VITE_DEV_SERVER_URL

  if (devServerUrl) {
    await mainWindow.loadURL(devServerUrl)
  } else {
    await mainWindow.loadFile(path.join(__dirname, "..", "dist", "index.html"))
  }

  mainWindow.webContents.on("before-input-event", (event, input) => {
    if (devServerUrl && input.type === "keyDown" && input.key === "F12") {
      mainWindow.webContents.toggleDevTools()
      event.preventDefault()
    }
  })

  mainWindow.webContents.on("context-menu", (_, params) => {
    const selectedText = params.selectionText.trim()

    if (!selectedText) {
      return
    }

    Menu.buildFromTemplate([
      {
        label: "翻译选中文本",
        click: () => {
          mainWindow.webContents.send("translation:selection-requested", {
            text: selectedText,
            x: params.x,
            y: params.y
          })
        }
      }
    ]).popup({ window: mainWindow })
  })

  mainWindow.on("close", event => {
    if (isQuitting) {
      return
    }

    event.preventDefault()

    if (appSettings.system.closeAction === "minimize") {
      mainWindow.hide()
      return
    }

    if (appSettings.system.closeAction === "quit") {
      isQuitting = true
      app.quit()
      return
    }

    requestCloseAction()
  })

  mainWindow.on("minimize", () => {
    syncQuickSwitchWindow()
  })

  mainWindow.on("hide", () => {
    syncQuickSwitchWindow()
  })

  mainWindow.on("restore", () => {
    syncQuickSwitchWindow()
  })

  mainWindow.on("show", () => {
    syncQuickSwitchWindow()
  })

  mainWindow.on("closed", () => {
    mainWindow = null
  })
}

async function createQuickSwitchWindow() {
  if (quickSwitchWindow && !quickSwitchWindow.isDestroyed()) {
    return
  }

  quickSwitchWindow = new BrowserWindow({
    width: quickSwitchExpandedSize.width,
    height: quickSwitchExpandedSize.height,
    frame: false,
    resizable: false,
    minimizable: false,
    maximizable: false,
    skipTaskbar: true,
    alwaysOnTop: true,
    icon: appIconPath,
    transparent: true,
    backgroundColor: "#00000000",
    hasShadow: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      devTools: Boolean(process.env.VITE_DEV_SERVER_URL)
    }
  })

  quickSwitchWindow.setAlwaysOnTop(true, "screen-saver")
  setQuickSwitchCollapsed(quickSwitchCollapsed)
  positionQuickSwitchWindow()

  const devServerUrl = process.env.VITE_DEV_SERVER_URL

  if (devServerUrl) {
    await quickSwitchWindow.loadURL(`${devServerUrl}?panel=quick-switch`)
  } else {
    await quickSwitchWindow.loadFile(path.join(__dirname, "..", "dist", "index.html"), {
      query: {
        panel: "quick-switch"
      }
    })
  }

  quickSwitchWindow.on("closed", () => {
    quickSwitchWindow = null
  })
}

function registerIpc() {
  ipcMain.handle("app:bootstrap", async () => {
    await managerReadyPromise
    return managerService.getState()
  })
  ipcMain.handle("app:refresh", async () => managerService.refreshAll())
  ipcMain.handle("app:close-action", async (_, payload) => {
    handleCloseAction(payload)
    return true
  })
  ipcMain.handle("quick-switch:show-main", async () => {
    await showMainPanel()
    return true
  })
  ipcMain.handle("quick-switch:set-collapsed", async (_, payload) => {
    setQuickSwitchCollapsed(payload?.collapsed)
    return true
  })
  ipcMain.handle("quick-switch:move-by", async (_, payload) => {
    moveQuickSwitchWindowBy(payload)
    return true
  })

  ipcMain.handle("settings:save", async (_, payload) => {
    const nextSettings = normalizeAppSettings(payload)
    fs.mkdirSync(nextSettings.dataPath, { recursive: true })
    saveAppSettings(nextSettings)
    appSettings = nextSettings
    await syncQuickSwitchWindow()

    if (
      path.resolve(nextSettings.dataPath) !==
      path.resolve(app.getPath("userData"))
    ) {
      await managerService.updateAppSettings(appSettings)
      managerService.setAppSettings(appSettings, true)
      return managerService.getState()
    }

    await managerService.updateAppSettings(appSettings)
    return managerService.getState()
  })

  ipcMain.handle("data:export", async () => {
    const result = await dialog.showSaveDialog(mainWindow, {
      title: "导出配置数据",
      defaultPath: path.join(
        app.getPath("desktop"),
        `monkey-thief-${new Date().toISOString().slice(0, 10)}.aimbackup`
      ),
      filters: [{ name: "Monkey Thief 备份", extensions: ["aimbackup"] }]
    })

    if (result.canceled || !result.filePath) {
      return {
        canceled: true
      }
    }

    await fsp.writeFile(
      result.filePath,
      await managerService.createDataBackup(),
      "utf8"
    )

    return {
      canceled: false,
      filePath: result.filePath
    }
  })

  ipcMain.handle("data:restore", async () => {
    const result = await dialog.showOpenDialog(mainWindow, {
      title: "恢复配置数据",
      defaultPath: app.getPath("desktop"),
      filters: [{ name: "Monkey Thief 备份", extensions: ["aimbackup"] }],
      properties: ["openFile"]
    })

    if (result.canceled || !result.filePaths[0]) {
      return {
        canceled: true
      }
    }

    const restoreResult = await managerService.restoreDataBackup(
      await fsp.readFile(result.filePaths[0], "utf8")
    )
    appSettings = normalizeAppSettings(restoreResult.appSettings)
    saveAppSettings(appSettings)

    return {
      canceled: false,
      state: await restartManagerService(appSettings)
    }
  })

  ipcMain.handle("data:cloud-push", async (_, payload) => {
    const cloudSync = normalizeCloudSyncSettings(payload)
    const lastUpdatedAt = Date.now()
    await uploadWebDavBackup(cloudSync, await managerService.createDataBackup())
    appSettings = normalizeAppSettings({
      ...appSettings,
      cloudSync: {
        ...cloudSync,
        lastUpdatedAt
      }
    })
    saveAppSettings(appSettings)
    managerService.setAppSettings(appSettings)

    return {
      uploadedAt: lastUpdatedAt,
      fileName: cloudSync.fileName,
      state: managerService.getState()
    }
  })

  ipcMain.handle("data:cloud-pull", async (_, payload) => {
    const cloudSync = normalizeCloudSyncSettings(payload)
    const lastUpdatedAt = Date.now()
    const restoreResult = await managerService.restoreDataBackup(
      await downloadWebDavBackup(cloudSync)
    )
    appSettings = normalizeAppSettings({
      ...restoreResult.appSettings,
      cloudSync: {
        ...cloudSync,
        lastUpdatedAt
      }
    })
    saveAppSettings(appSettings)

    return {
      downloadedAt: lastUpdatedAt,
      fileName: cloudSync.fileName,
      state: await restartManagerService(appSettings)
    }
  })

  ipcMain.handle("system:select-directory", async (_, payload) => {
    const result = await dialog.showOpenDialog(mainWindow, {
      title: payload?.title || "选择目录",
      defaultPath: payload?.defaultPath || app.getPath("home"),
      properties: ["openDirectory", "createDirectory"]
    })

    return result.canceled ? "" : result.filePaths[0]
  })

  ipcMain.handle("system:select-file", async (_, payload) => {
    const result = await dialog.showOpenDialog(mainWindow, {
      title: payload?.title || "选择文件",
      defaultPath: payload?.defaultPath || app.getPath("home"),
      filters: payload?.filters || [],
      properties: ["openFile"]
    })

    return result.canceled ? "" : result.filePaths[0]
  })

  ipcMain.handle("skill:create", async (_, payload) => {
    await managerService.createSkill(payload)
    return managerService.getState()
  })

  ipcMain.handle("skill:preview-import-from-cli", async (_, payload) => {
    return managerService.previewSkillsFromCli(payload?.targetId)
  })

  ipcMain.handle("skill:import-from-cli", async (_, payload) => {
    await managerService.importSkillsFromCli(payload?.targetId, payload)
    return managerService.getState()
  })

  ipcMain.handle("skill:import-from-zip", async (_, payload) => {
    await managerService.importSkillFromZip(payload?.zipPath)
    return managerService.getState()
  })

  ipcMain.handle("skill:install", async (_, payload) => {
    await managerService.installSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  ipcMain.handle("skill:uninstall", async (_, payload) => {
    await managerService.uninstallSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  ipcMain.handle("skill:repair", async (_, payload) => {
    await managerService.repairSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  ipcMain.handle("repo:add", async (_, payload) => {
    await managerService.addRepo(payload)
    return managerService.getState()
  })

  ipcMain.handle("repo:sync", async (_, payload) => {
    await managerService.syncRepo(payload.repoId)
    return managerService.getState()
  })

  ipcMain.handle("repo:sync-all", async () => {
    await managerService.syncAllRepos()
    return managerService.getState()
  })

  ipcMain.handle("repo:remove", async (_, payload) => {
    await managerService.removeRepo(payload.repoId)
    return managerService.getState()
  })

  ipcMain.handle("session:search", async (_, payload) => {
    return managerService.searchSessions(payload?.query)
  })

  ipcMain.handle("session:messages", async (_, payload) => {
    return managerService.loadSessionMessages(payload?.sessionId)
  })

  ipcMain.handle("session:delete", async (_, payload) => {
    await managerService.deleteSession(payload.sessionId)
    return managerService.getState()
  })

  ipcMain.handle("session:recycle-list", async () => {
    return managerService.listRecycledSessions()
  })

  ipcMain.handle("session:restore", async (_, payload) => {
    await managerService.restoreSession(payload.sessionId)
    return managerService.getState()
  })

  ipcMain.handle("session:purge", async (_, payload) => {
    await managerService.purgeSession(payload.sessionId)
    return true
  })

  ipcMain.handle("provider:save", async (_, payload) => {
    return managerService.saveProvider(payload)
  })

  ipcMain.handle("provider:delete", async (_, payload) => {
    return managerService.deleteProvider(payload.providerId)
  })

  ipcMain.handle("rule:save", async (_, payload) => {
    return managerService.saveRule(payload)
  })

  ipcMain.handle("rule:delete", async (_, payload) => {
    return managerService.deleteRule(payload.ruleId)
  })

  ipcMain.handle("rule:toggle", async (_, payload) => {
    return managerService.toggleRule(payload)
  })

  ipcMain.handle("rule:enable", async (_, payload) => {
    return managerService.enableRule(payload.ruleId)
  })

  ipcMain.handle("rule:move", async (_, payload) => {
    return managerService.moveRule(payload)
  })

  ipcMain.handle("rule:import-global", async (_, payload) => {
    return managerService.importRule(payload)
  })

  ipcMain.handle("rule:preview-import-global", async (_, payload) => {
    return managerService.previewImportRule(payload)
  })

  ipcMain.handle("rule:resolve-import-conflict", async (_, payload) => {
    return managerService.resolveRuleImportConflict(payload)
  })

  ipcMain.handle("rule:compare", async (_, payload) => {
    return managerService.compareRule(payload)
  })

  ipcMain.handle("rule:resolve-drift", async (_, payload) => {
    return managerService.resolveRuleDrift(payload)
  })

  ipcMain.handle("codex-account:login", async (_, payload) => {
    return managerService.startCodexOfficialLogin(payload)
  })

  ipcMain.handle("codex-account:cancel", async () => {
    return managerService.cancelCodexOfficialLogin()
  })

  ipcMain.handle("codex-account:import-auth-json", async (_, payload) => {
    return managerService.importCodexAuthJson(payload)
  })

  ipcMain.handle("codex-account:enable", async (_, payload) => {
    return managerService.enableCodexAccount(payload)
  })

  ipcMain.handle("codex-account:clear", async () => {
    return managerService.clearCodexAccount()
  })

  ipcMain.handle("codex-account:refresh", async (_, payload) => {
    return managerService.refreshCodexAccount(payload)
  })

  ipcMain.handle("codex-account:update-proxy", async (_, payload) => {
    return managerService.updateCodexAccountProxy(payload)
  })

  ipcMain.handle("codex-account:detail", async (_, payload) => {
    return managerService.getCodexAccountDetail(payload)
  })

  ipcMain.handle("codex-account:delete", async (_, payload) => {
    return managerService.deleteCodexAccount(payload)
  })

  ipcMain.handle("runtime-model:save", async (_, payload) => {
    return managerService.saveRuntimeModel(payload)
  })

  ipcMain.handle("runtime:switch", async (_, payload) => {
    return managerService.switchRuntime(payload)
  })

  ipcMain.handle("runtime:clear", async (_, payload) => {
    return managerService.clearRuntime(payload.cli)
  })

  ipcMain.handle("runtime:compare", async (_, payload) => {
    return managerService.compareRuntime(payload)
  })

  ipcMain.handle("runtime:config", async (_, payload) => {
    return managerService.getRuntimeConfig(payload)
  })

  ipcMain.handle("runtime:resolve-drift", async (_, payload) => {
    return managerService.resolveRuntimeDrift(payload)
  })

  ipcMain.handle("runtime:env", async (_, payload) => {
    return managerService.buildRuntimeEnv(payload.cli)
  })

  ipcMain.handle("system:open-path", async (_, payload) => {
    if (!payload?.targetPath) {
      return false
    }

    const result = await shell.openPath(payload.targetPath)

    if (result) {
      throw new Error(result)
    }

    return true
  })

  ipcMain.handle("system:open-external", async (_, payload) => {
    if (!payload?.url) {
      return false
    }

    await shell.openExternal(payload.url)
    return true
  })

  ipcMain.handle("translation:translate", async (_, payload) => {
    return translationService.translate(payload?.text)
  })
}

app.whenReady().then(async () => {
  managerService = new ManagerService(app.getPath("userData"), appSettings)
  translationService = new TranslationService(app.getPath("userData"))
  managerReadyPromise = managerService.init()
  managerService.on("state-changed", state => {
    sendStateChanged(state)
    updateTrayMenu(state)
  })

  registerIpc()
  createTray()
  managerReadyPromise
    .then(() => {
      updateTrayMenu()
    })
    .catch(showTrayError)
  await createWindow()
  await syncQuickSwitchWindow()

  screen.on("display-metrics-changed", positionQuickSwitchWindow)
  screen.on("display-added", positionQuickSwitchWindow)
  screen.on("display-removed", positionQuickSwitchWindow)

  app.on("activate", async () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      await createWindow()
      return
    }

    await showMainPanel()
  })
})

app.on("window-all-closed", () => {
  if (!isQuitting) {
    return
  }

  if (process.platform !== "darwin") {
    app.quit()
  }
})

app.on("before-quit", async () => {
  isQuitting = true

  if (managerService) {
    await managerService.dispose()
  }
})

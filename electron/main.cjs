const path = require("node:path")
const fs = require("node:fs")
const os = require("node:os")
const fsp = require("node:fs/promises")
const {
  app,
  BrowserWindow,
  Menu,
  Tray,
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
let tray = null
let managerService = null
let translationService = null
let managerReadyPromise = null
let isQuitting = false
const defaultUserDataPath = "D:\\ai-manager-data"
const settingsFilePath = path.join(defaultUserDataPath, "app-settings.json")
const appIconPath = app.isPackaged
  ? path.join(process.resourcesPath, "assets", "icon.png")
  : path.join(__dirname, "..", "build", "icon.png")
app.setAppUserModelId("com.monkeythief.desktop")
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

function normalizeCloudSyncSettings(input = {}) {
  return {
    provider: "jianguoyun",
    webdavUrl: String(
      input.webdavUrl || defaultCloudSyncSettings.webdavUrl
    ).trim(),
    username: String(input.username || "").trim(),
    password: String(input.password || ""),
    fileName: String(input.fileName || defaultCloudSyncSettings.fileName).trim(),
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
    cloudSync: normalizeCloudSyncSettings(input.cloudSync)
  }
}

function encodeWebDavPath(value) {
  return String(value || "")
    .split("/")
    .filter(Boolean)
    .map((item) => encodeURIComponent(item))
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

async function restartManagerService(nextSettings = appSettings) {
  if (managerService) {
    managerService.removeAllListeners()
    await managerService.dispose()
  }

  managerService = new ManagerService(app.getPath("userData"), nextSettings)
  managerReadyPromise = managerService.init()
  await managerReadyPromise
  managerService.on("state-changed", (state) => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("state:changed", state)
    }
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

  mainWindow.on("close", (event) => {
    if (isQuitting) {
      return
    }

    event.preventDefault()
    mainWindow.hide()
  })

  mainWindow.on("closed", () => {
    mainWindow = null
  })
}

function registerIpc() {
  ipcMain.handle("app:bootstrap", async () => {
    await managerReadyPromise
    return managerService.getState()
  })
  ipcMain.handle("app:refresh", async () => managerService.refreshAll())

  ipcMain.handle("settings:save", async (_, payload) => {
    const nextSettings = normalizeAppSettings(payload)
    fs.mkdirSync(nextSettings.dataPath, { recursive: true })
    saveAppSettings(nextSettings)
    appSettings = nextSettings

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
    await uploadWebDavBackup(
      cloudSync,
      await managerService.createDataBackup()
    )
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
  managerService.on("state-changed", (state) => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("state:changed", state)
    }
  })

  registerIpc()
  createTray()
  await createWindow()

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

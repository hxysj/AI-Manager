const path = require("node:path")
const fs = require("node:fs")
const os = require("node:os")
const { app, BrowserWindow, Menu, ipcMain, shell, dialog } = require("electron")
const { ManagerService } = require("./services/manager-service.cjs")
const { TranslationService } = require("./services/translation-service.cjs")

let mainWindow = null
let managerService = null
let translationService = null
const defaultUserDataPath = "D:\\ai-manager-data"
const settingsFilePath = path.join(defaultUserDataPath, "app-settings.json")
const defaultCliConfigPaths = {
  claude: path.join(os.homedir(), ".claude"),
  codex: path.join(os.homedir(), ".codex"),
  gemini: path.join(os.homedir(), ".gemini")
}

function normalizeAppSettings(input = {}) {
  return {
    dataPath: String(input.dataPath || defaultUserDataPath).trim(),
    defaultDataPath: defaultUserDataPath,
    settingsFilePath,
    cliConfigPaths: {
      claude: String(
        input.cliConfigPaths?.claude || defaultCliConfigPaths.claude
      ).trim(),
      codex: String(
        input.cliConfigPaths?.codex || defaultCliConfigPaths.codex
      ).trim(),
      gemini: String(
        input.cliConfigPaths?.gemini || defaultCliConfigPaths.gemini
      ).trim()
    },
    defaultCliConfigPaths
  }
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
    `${JSON.stringify(nextSettings, null, 2)}\n`,
    "utf8"
  )
}

let appSettings = loadAppSettings()
const userDataPath = appSettings.dataPath

fs.mkdirSync(userDataPath, { recursive: true })
app.setPath("userData", userDataPath)

async function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1480,
    height: 980,
    minWidth: 1200,
    minHeight: 760,
    backgroundColor: "#ffffff",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "default",
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false
    }
  })

  const devServerUrl = process.env.VITE_DEV_SERVER_URL

  if (devServerUrl) {
    await mainWindow.loadURL(devServerUrl)
  } else {
    await mainWindow.loadFile(path.join(__dirname, "..", "dist", "index.html"))
  }

  mainWindow.webContents.on("before-input-event", (event, input) => {
    if (input.type === "keyDown" && input.key === "F12") {
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

  mainWindow.on("closed", () => {
    mainWindow = null
  })
}

function registerIpc() {
  ipcMain.handle("app:bootstrap", async () => managerService.getState())
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

  ipcMain.handle("runtime-model:save", async (_, payload) => {
    return managerService.saveRuntimeModel(payload)
  })

  ipcMain.handle("runtime:switch", async (_, payload) => {
    return managerService.switchRuntime(payload)
  })

  ipcMain.handle("runtime:clear", async (_, payload) => {
    return managerService.clearRuntime(payload.cli)
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
  await managerService.init()
  managerService.on("state-changed", (state) => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("state:changed", state)
    }
  })

  registerIpc()
  await createWindow()

  app.on("activate", async () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      await createWindow()
    }
  })
})

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit()
  }
})

app.on("before-quit", async () => {
  if (managerService) {
    await managerService.dispose()
  }
})

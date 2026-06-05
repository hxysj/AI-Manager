const path = require("node:path")
const fs = require("node:fs")
const os = require("node:os")
const crypto = require("node:crypto")
const fsp = require("node:fs/promises")
const { spawn } = require("node:child_process")
const { AsyncLocalStorage } = require("node:async_hooks")
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
const { autoUpdater } = require("electron-updater")

let mainWindow = null
let quickSwitchWindow = null
let tray = null
let managerService = null
let translationService = null
let managerReadyPromise = null
let isQuitting = false
let closeDialogOpen = false
let quickSwitchCollapsed = false
let localBackupTimer = null
let localBackupRunning = false
let updateConfigured = false
let updateChecking = false
let updateDownloading = false
let updatePromptOpen = false
let updateManualCheck = false
let updateStatus = {
  phase: "idle",
  manual: false,
  message: "",
  version: "",
  releaseNotes: "",
  percent: 0,
  transferred: 0,
  total: 0,
  bytesPerSecond: 0,
  installDirectory: "",
  configured: false,
  isDev: !app.isPackaged,
  updatedAt: 0
}
let appCallLogs = []
let appCallLogWriteTimer = null
const appCallTraceStorage = new AsyncLocalStorage()
const instrumentedServices = new WeakSet()
const restoreBackupDrafts = new Map()
const defaultUserDataPath = "D:\\ai-manager-data"
const settingsFilePath = path.join(defaultUserDataPath, "app-settings.json")
const updateConfigPath = path.join(__dirname, "update-config.generated.cjs")
const appIconPath = app.isPackaged
  ? path.join(process.resourcesPath, "assets", "icon.png")
  : path.join(__dirname, "..", "build", "icon.png")
app.setAppUserModelId("com.monkeythief.desktop")
const singleInstanceLock = app.requestSingleInstanceLock()

if (!singleInstanceLock) {
  app.quit()
}

const quickSwitchExpandedSize = { width: 360, height: 238 }
const quickSwitchCollapsedSize = { width: 44, height: 44 }
const portableHomePrefix = path.join(path.dirname(os.homedir()), "%USERNAME%")
const defaultCliConfigPaths = {
  claude: path.join(portableHomePrefix, ".claude"),
  codex: path.join(portableHomePrefix, ".codex")
  // 当前版本暂不启用 Gemini。
  // gemini: path.join(portableHomePrefix, ".gemini")
}
const defaultCloudSyncSettings = {
  provider: "jianguoyun",
  webdavUrl: "https://dav.jianguoyun.com/dav/AI-Manager",
  username: "",
  password: "",
  fileName: "ai-manager.aimbackup",
  lastUpdatedAt: 0
}
const defaultLocalBackupSettings = {
  enabled: true,
  intervalMinutes: 60,
  maxCount: 20,
  lastBackupAt: 0
}
const defaultSystemSettings = {
  closeAction: "ask",
  quickSwitchVisible: true,
  autoLaunchEnabled: false
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
        : Boolean(input.quickSwitchVisible),
    autoLaunchEnabled:
      input.autoLaunchEnabled === undefined
        ? defaultSystemSettings.autoLaunchEnabled
        : Boolean(input.autoLaunchEnabled)
  }
}

function applyAutoLaunchSetting(settings = appSettings) {
  app.setLoginItemSettings({
    openAtLogin: settings.system.autoLaunchEnabled
  })
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

function normalizeLocalBackupSettings(input = {}) {
  const intervalMinutes = Math.floor(
    Number(input.intervalMinutes || defaultLocalBackupSettings.intervalMinutes)
  )
  const maxCount = Math.floor(
    Number(input.maxCount || defaultLocalBackupSettings.maxCount)
  )

  return {
    enabled:
      input.enabled === undefined
        ? defaultLocalBackupSettings.enabled
        : Boolean(input.enabled),
    intervalMinutes: Number.isFinite(intervalMinutes)
      ? Math.max(1, intervalMinutes)
      : defaultLocalBackupSettings.intervalMinutes,
    maxCount: Number.isFinite(maxCount)
      ? Math.max(1, maxCount)
      : defaultLocalBackupSettings.maxCount,
    lastBackupAt: Number(input.lastBackupAt || 0)
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
      )
      // 当前版本暂不启用 Gemini。
      // gemini: resolvePortablePath(
      //   String(cliConfigPaths.gemini || defaultCliConfigPaths.gemini).trim()
      // )
    },
    defaultCliConfigPaths,
    cloudSync: normalizeCloudSyncSettings(input.cloudSync),
    localBackup: normalizeLocalBackupSettings(input.localBackup),
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

function cacheRestoreBackup(content, source = {}) {
  const restoreId = crypto.randomUUID()

  restoreBackupDrafts.set(restoreId, {
    content,
    source,
    createdAt: Date.now()
  })

  return restoreId
}

function getRestoreBackupDraft(restoreId) {
  const draft = restoreBackupDrafts.get(String(restoreId || ""))

  if (!draft) {
    throw new Error("恢复预览已失效，请重新选择备份")
  }

  return draft
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

function getAppCallLogPath() {
  return path.join(
    app.getPath("userData"),
    "workspace",
    "logs",
    "app-call-logs.json"
  )
}

function getLegacyAppCallLogPath() {
  return path.join(
    app.getPath("userData"),
    "workspace",
    "storage",
    "app-call-logs.json"
  )
}

function sanitizeLogValue(value) {
  if (value === undefined) {
    return undefined
  }

  try {
    return JSON.parse(
      JSON.stringify(value, (key, item) =>
        /password|token|key|secret/i.test(key) ? item ? "***" : item : item
      )
    )
  } catch (error) {
    return {
      uncloneable: true,
      message: error.message
    }
  }
}

function summarizeLogValue(value) {
  if (value === undefined || value === null) {
    return value
  }

  if (["number", "boolean"].includes(typeof value)) {
    return value
  }

  if (typeof value === "string") {
    return value.length > 500 ? `${value.slice(0, 500)}...` : value
  }

  if (Array.isArray(value)) {
    return {
      type: "array",
      length: value.length
    }
  }

  if (value && typeof value === "object") {
    return {
      type: "object",
      keys: Object.keys(value).slice(0, 30)
    }
  }

  return String(value)
}

function splitIpcTraceArgs(args) {
  const lastArg = args[args.length - 1]

  if (lastArg?.__traceMeta === true) {
    return {
      traceMeta: lastArg,
      businessArgs: args.slice(0, -1)
    }
  }

  return {
    traceMeta: {},
    businessArgs: args
  }
}

async function initAppCallLogs() {
  await migrateAppCallLogs()

  try {
    appCallLogs = JSON.parse(await fsp.readFile(getAppCallLogPath(), "utf8"))
  } catch {
    appCallLogs = []
  }
}

async function migrateAppCallLogs() {
  const sourcePath = getLegacyAppCallLogPath()
  const targetPath = getAppCallLogPath()

  if (!fs.existsSync(sourcePath)) {
    return
  }

  await fsp.mkdir(path.dirname(targetPath), { recursive: true })

  if (!fs.existsSync(targetPath)) {
    await fsp.rename(sourcePath, targetPath)
    return
  }

  const sourceItems = JSON.parse(await fsp.readFile(sourcePath, "utf8"))
  const targetItems = JSON.parse(await fsp.readFile(targetPath, "utf8"))

  if (!Array.isArray(sourceItems) || !Array.isArray(targetItems)) {
    await fsp.rm(sourcePath)
    return
  }

  const itemMap = new Map()

  for (const item of [...targetItems, ...sourceItems]) {
    itemMap.set(item.id, item)
  }

  await fsp.writeFile(
    targetPath,
    `${JSON.stringify(Array.from(itemMap.values()), null, 2)}\n`,
    "utf8"
  )
  await fsp.rm(sourcePath)
}

function scheduleAppCallLogWrite() {
  if (appCallLogWriteTimer) {
    clearTimeout(appCallLogWriteTimer)
  }

  appCallLogWriteTimer = setTimeout(async () => {
    appCallLogWriteTimer = null
    await fsp.mkdir(path.dirname(getAppCallLogPath()), { recursive: true })
    await fsp.writeFile(
      getAppCallLogPath(),
      `${JSON.stringify(appCallLogs, null, 2)}\n`,
      "utf8"
    )
  }, 200)
}

function appendAppCallLog(input = {}) {
  const trace = appCallTraceStorage.getStore() || {}

  appCallLogs.unshift({
    id: crypto.randomUUID(),
    traceId: String(input.traceId || trace.traceId || crypto.randomUUID()),
    scope: String(input.scope || "backend"),
    service: String(input.service || trace.service || ""),
    method: String(input.method || trace.method || input.channel || ""),
    channel: String(input.channel || ""),
    action: String(input.action || ""),
    status: String(input.status || ""),
    durationMs: Number(input.durationMs || 0),
    message: String(input.message || ""),
    payload: sanitizeLogValue(input.payload),
    result: summarizeLogValue(input.result),
    createdAt: Date.now()
  })
  appCallLogs = appCallLogs.slice(0, 1000)
  scheduleAppCallLogWrite()
}

function assertIpcResultCloneable(channel, traceId, result) {
  try {
    structuredClone(result)
  } catch (error) {
    appendAppCallLog({
      traceId,
      scope: "backend",
      service: "IpcMain",
      method: channel,
      channel,
      action: "clone",
      status: "error",
      message: error.message
    })
    throw new Error(`IPC 返回值无法克隆：${channel}，${error.message}`)
  }
}

function registerLoggedIpc(channel, handler) {
  if (channel.startsWith("app-log:")) {
    ipcMain.handle(channel, handler)
    return
  }

  ipcMain.handle(channel, async (event, ...args) => {
    const { traceMeta, businessArgs } = splitIpcTraceArgs(args)
    const trace = {
      traceId: traceMeta.traceId || crypto.randomUUID(),
      channel,
      service: "IpcMain",
      method: channel
    }
    const startedAt = Date.now()

    appendAppCallLog({
      traceId: trace.traceId,
      scope: "backend",
      service: "IpcMain",
      method: channel,
      channel,
      action: "start",
      status: "pending",
      payload: businessArgs[0]
    })

    try {
      const result = await appCallTraceStorage.run(trace, () =>
        handler(event, ...businessArgs)
      )

      assertIpcResultCloneable(channel, trace.traceId, result)
      appendAppCallLog({
        traceId: trace.traceId,
        scope: "backend",
        service: "IpcMain",
        method: channel,
        channel,
        action: "finish",
        status: "success",
        durationMs: Date.now() - startedAt,
        result
      })

      return result
    } catch (error) {
      appendAppCallLog({
        traceId: trace.traceId,
        scope: "backend",
        service: "IpcMain",
        method: channel,
        channel,
        action: "finish",
        status: "error",
        durationMs: Date.now() - startedAt,
        message: error.message
      })
      throw error
    }
  })
}

function instrumentBackendService(serviceName, service) {
  if (!service || instrumentedServices.has(service)) {
    return
  }

  instrumentedServices.add(service)

  for (const methodName of Object.getOwnPropertyNames(
    Object.getPrototypeOf(service)
  )) {
    if (methodName === "constructor") {
      continue
    }

    const original = service[methodName]

    if (typeof original !== "function") {
      continue
    }

    service[methodName] = function (...args) {
      const parentTrace = appCallTraceStorage.getStore()
      const trace = parentTrace || {
        traceId: crypto.randomUUID(),
        service: serviceName,
        method: methodName
      }
      const startedAt = Date.now()

      appendAppCallLog({
        traceId: trace.traceId,
        scope: "backend",
        service: serviceName,
        method: methodName,
        channel: trace.channel || "",
        action: "start",
        status: "pending",
        payload: args[0]
      })

      return appCallTraceStorage.run(trace, () => {
        try {
          const result = original.apply(this, args)

          if (result && typeof result.then === "function") {
            return result
              .then((value) => {
                appendAppCallLog({
                  traceId: trace.traceId,
                  scope: "backend",
                  service: serviceName,
                  method: methodName,
                  channel: trace.channel || "",
                  action: "finish",
                  status: "success",
                  durationMs: Date.now() - startedAt,
                  result: value
                })
                return value
              })
              .catch((error) => {
                appendAppCallLog({
                  traceId: trace.traceId,
                  scope: "backend",
                  service: serviceName,
                  method: methodName,
                  channel: trace.channel || "",
                  action: "finish",
                  status: "error",
                  durationMs: Date.now() - startedAt,
                  message: error.message
                })
                throw error
              })
          }

          appendAppCallLog({
            traceId: trace.traceId,
            scope: "backend",
            service: serviceName,
            method: methodName,
            channel: trace.channel || "",
            action: "finish",
            status: "success",
            durationMs: Date.now() - startedAt,
            result
          })
          return result
        } catch (error) {
          appendAppCallLog({
            traceId: trace.traceId,
            scope: "backend",
            service: serviceName,
            method: methodName,
            channel: trace.channel || "",
            action: "finish",
            status: "error",
            durationMs: Date.now() - startedAt,
            message: error.message
          })
          throw error
        }
      })
    }
  }
}

function instrumentBackendServices(service) {
  instrumentBackendService("ManagerService", service)

  for (const childService of Object.values(service)) {
    if (/Service$/.test(childService?.constructor?.name || "")) {
      instrumentBackendService(childService.constructor.name, childService)
    }
  }
}

function sendStateChanged(state) {
  const payload = JSON.parse(JSON.stringify(state))

  for (const targetWindow of [mainWindow, quickSwitchWindow]) {
    if (targetWindow && !targetWindow.isDestroyed()) {
      targetWindow.webContents.send("state:changed", payload)
    }
  }
}

function sendUpdateStatus(patch = {}) {
  updateStatus = {
    ...updateStatus,
    ...patch,
    configured: updateConfigured,
    isDev: !app.isPackaged,
    updatedAt: Date.now()
  }
  const payload = JSON.parse(JSON.stringify(updateStatus))

  for (const targetWindow of [mainWindow, quickSwitchWindow]) {
    if (targetWindow && !targetWindow.isDestroyed()) {
      targetWindow.webContents.send("app:update-status", payload)
    }
  }

  return payload
}

function getUpdateStatus() {
  return JSON.parse(
    JSON.stringify({
      ...updateStatus,
      configured: updateConfigured,
      isDev: !app.isPackaged
    })
  )
}

async function restartManagerService(nextSettings = appSettings) {
  if (managerService) {
    managerService.removeAllListeners()
    await managerService.dispose()
  }

  managerService = new ManagerService(app.getPath("userData"), nextSettings)
  instrumentBackendServices(managerService)
  managerReadyPromise = managerService.init()
  await managerReadyPromise
  managerService.on("state-changed", state => {
    sendStateChanged(state)
    updateTrayMenu(state)
  })

  return JSON.parse(JSON.stringify(managerService.getState()))
}

function getLocalBackupDirectory() {
  return path.join(app.getPath("userData"), "local-backups")
}

function getLocalBackupPath(backupId) {
  const fileName = path.basename(String(backupId || ""))

  if (!fileName.endsWith(".aimbackup")) {
    throw new Error("本地备份文件无效")
  }

  return path.join(getLocalBackupDirectory(), fileName)
}

async function listLocalBackupFiles() {
  const backupDir = getLocalBackupDirectory()
  let children = []

  try {
    children = await fsp.readdir(backupDir, { withFileTypes: true })
  } catch (error) {
    if (error.code === "ENOENT") {
      return []
    }

    throw error
  }

  const backups = []

  for (const child of children) {
    if (!child.isFile() || !child.name.endsWith(".aimbackup")) {
      continue
    }

    const filePath = path.join(backupDir, child.name)
    const stat = await fsp.stat(filePath)
    backups.push({
      id: child.name,
      fileName: child.name,
      filePath,
      createdAt: stat.mtimeMs,
      size: stat.size
    })
  }

  return backups.sort((left, right) => right.createdAt - left.createdAt)
}

async function getLocalBackupsPayload() {
  return {
    directory: getLocalBackupDirectory(),
    backups: await listLocalBackupFiles()
  }
}

async function pruneLocalBackups() {
  const backups = await listLocalBackupFiles()
  const expiredBackups = backups.slice(appSettings.localBackup.maxCount)

  for (const backup of expiredBackups) {
    await fsp.rm(backup.filePath, { force: true })
  }
}

async function createLocalBackup() {
  if (localBackupRunning) {
    return null
  }

  localBackupRunning = true

  try {
    await managerReadyPromise

    const backupDir = getLocalBackupDirectory()
    const createdAt = Date.now()
    const fileName = `monkey-thief-auto-${new Date(createdAt)
      .toISOString()
      .replace(/[:.]/g, "-")}.aimbackup`
    const filePath = path.join(backupDir, fileName)

    await fsp.mkdir(backupDir, { recursive: true })
    await fsp.writeFile(
      filePath,
      await managerService.createDataBackup({ includeGitToolData: true }),
      "utf8"
    )
    await pruneLocalBackups()

    appSettings = normalizeAppSettings({
      ...appSettings,
      localBackup: {
        ...appSettings.localBackup,
        lastBackupAt: createdAt
      }
    })
    saveAppSettings(appSettings)
    managerService.setAppSettings(appSettings)

    const stat = await fsp.stat(filePath)

    return {
      id: fileName,
      fileName,
      filePath,
      createdAt: stat.mtimeMs,
      size: stat.size
    }
  } finally {
    localBackupRunning = false
  }
}

async function createLocalBackupIfDue() {
  if (!appSettings.localBackup.enabled) {
    return
  }

  const intervalMs = appSettings.localBackup.intervalMinutes * 60 * 1000

  if (Date.now() - appSettings.localBackup.lastBackupAt < intervalMs) {
    return
  }

  await createLocalBackup()
}

function restartLocalBackupTimer() {
  if (localBackupTimer) {
    clearInterval(localBackupTimer)
    localBackupTimer = null
  }

  if (!appSettings.localBackup.enabled) {
    return
  }

  const intervalMs = appSettings.localBackup.intervalMinutes * 60 * 1000

  localBackupTimer = setInterval(() => {
    createLocalBackupIfDue().catch(showTrayError)
  }, intervalMs)

  createLocalBackupIfDue().catch(showTrayError)
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
  const centerX = bounds.x + bounds.width / 2
  const centerY = bounds.y + bounds.height / 2
  const nextX = Math.round(centerX - size.width / 2)
  const nextY = Math.round(centerY - size.height / 2)
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

function loadUpdateConfig() {
  if (!app.isPackaged || !fs.existsSync(updateConfigPath)) {
    return {
      githubToken: ""
    }
  }

  return require(updateConfigPath)
}

function formatUpdateReleaseNotes(value) {
  if (Array.isArray(value)) {
    return value.map(item => item.note || "").filter(Boolean).join("\n\n")
  }

  return String(value || "").trim()
}

function getDefaultUpdateInstallDirectory() {
  return path.dirname(app.getPath("exe"))
}

function setupAutoUpdater() {
  const updateConfig = loadUpdateConfig()
  const githubToken = String(updateConfig.githubToken || "").trim()

  if (!githubToken) {
    return
  }

  process.env.GH_TOKEN = githubToken
  updateConfigured = true
  autoUpdater.autoDownload = false
  autoUpdater.disableDifferentialDownload = true
  autoUpdater.autoInstallOnAppQuit = false
  autoUpdater.addAuthHeader(`Bearer ${githubToken}`)

  autoUpdater.on("checking-for-update", () => {
    updateChecking = true
    sendUpdateStatus({
      phase: "checking",
      message: "正在检查更新...",
      manual: updateManualCheck,
      percent: 0,
      transferred: 0,
      total: 0,
      bytesPerSecond: 0,
      installDirectory: getDefaultUpdateInstallDirectory()
    })
  })

  autoUpdater.on("update-not-available", () => {
    updateChecking = false
    if (updateManualCheck) {
      sendUpdateStatus({
        phase: "not-available",
        message: "当前已是最新版本。",
        manual: true
      })
      updateManualCheck = false
      return
    }

    sendUpdateStatus({
      phase: "idle",
      message: "",
      manual: false
    })
  })

  autoUpdater.on("error", error => {
    updateChecking = false
    updateDownloading = false
    updatePromptOpen = false
    console.error("[update]", error)
    sendUpdateStatus({
      phase: "error",
      message: error?.message || String(error),
      manual: updateManualCheck
    })
    updateManualCheck = false
  })

  autoUpdater.on("update-available", info => {
    updateChecking = false
    updateManualCheck = false

    if (updatePromptOpen || updateDownloading) {
      return
    }

    updatePromptOpen = true
    const releaseNotes = formatUpdateReleaseNotes(info.releaseNotes)
    sendUpdateStatus({
      phase: "available",
      message: `发现新版本 ${info.version}`,
      version: info.version,
      releaseNotes,
      manual: true,
      percent: 0,
      transferred: 0,
      total: 0,
      bytesPerSecond: 0,
      installDirectory: getDefaultUpdateInstallDirectory()
    })
  })

  autoUpdater.on("download-progress", progress => {
    sendUpdateStatus({
      phase: "downloading",
      message: `正在下载新版本 ${updateStatus.version || ""}`.trim(),
      manual: true,
      percent: Number(progress.percent || 0),
      transferred: Number(progress.transferred || 0),
      total: Number(progress.total || 0),
      bytesPerSecond: Number(progress.bytesPerSecond || 0)
    })
  })

  autoUpdater.on("update-downloaded", info => {
    updateDownloading = false
    updatePromptOpen = true
    sendUpdateStatus({
      phase: "downloaded",
      message: `新版本 ${info.version} 已下载完成，请在安装向导中确认安装目录。`,
      version: info.version,
      manual: true,
      percent: 100,
      installDirectory: getDefaultUpdateInstallDirectory()
    })
  })
}

async function downloadAppUpdate() {
  if (!updateConfigured) {
    return sendUpdateStatus({
      phase: "unconfigured",
      message: "当前安装包未包含更新配置。",
      manual: true
    })
  }

  updatePromptOpen = false
  updateDownloading = true
  sendUpdateStatus({
    phase: "downloading",
    message: `正在下载新版本 ${updateStatus.version || ""}`.trim(),
    manual: true,
    percent: 0,
    transferred: 0,
    total: 0,
    bytesPerSecond: 0,
    installDirectory: getDefaultUpdateInstallDirectory()
  })

  try {
    await autoUpdater.downloadUpdate()
  } catch (error) {
    updateDownloading = false
    console.error("[update:download]", error)
    return sendUpdateStatus({
      phase: "error",
      message: error?.message || String(error),
      manual: true
    })
  }

  return getUpdateStatus()
}

function installAppUpdate(payload = {}) {
  const installDirectory = String(
    payload.installDirectory || getDefaultUpdateInstallDirectory()
  ).trim()
  const installerPath = autoUpdater.installerPath

  if (!installDirectory) {
    throw new Error("安装目录不能为空")
  }

  if (!installerPath) {
    throw new Error("更新安装包未下载完成")
  }

  sendUpdateStatus({
    phase: "installing",
    message: "正在打开安装向导，安装目录可在向导中继续确认。",
    manual: true,
    installDirectory
  })
  spawn(installerPath, [`/D=${installDirectory}`], {
    detached: true,
    stdio: "ignore",
    windowsHide: false
  }).unref()
  isQuitting = true
  require("electron").autoUpdater.emit("before-quit-for-update")
  app.quit()
  return getUpdateStatus()
}

async function findAppUninstaller() {
  const installDirectory = path.dirname(app.getPath("exe"))
  const entries = await fsp.readdir(installDirectory, { withFileTypes: true })
  const uninstaller = entries.find(item => {
    return item.isFile() && /^uninstall.*\.exe$/i.test(item.name)
  })

  if (!uninstaller) {
    throw new Error("未找到应用卸载程序")
  }

  return path.join(installDirectory, uninstaller.name)
}

function toPowerShellLiteral(value) {
  return `'${String(value || "").replace(/'/g, "''")}'`
}

async function uninstallWithoutTrace() {
  if (!app.isPackaged) {
    throw new Error("开发环境不执行无痕卸载")
  }

  const uninstallerPath = await findAppUninstaller()
  const dataPath = path.resolve(app.getPath("userData"))
  const configuredDataPath = path.resolve(appSettings.dataPath)

  if (dataPath !== configuredDataPath) {
    throw new Error("当前运行数据目录与配置不一致，已拒绝无痕卸载")
  }

  if (dataPath === path.parse(dataPath).root) {
    throw new Error("数据目录不能是磁盘根目录")
  }

  if (!path.relative(dataPath, uninstallerPath).startsWith("..")) {
    throw new Error("数据目录不能包含应用卸载程序")
  }

  const cleanupScript = [
    "$ErrorActionPreference = 'SilentlyContinue'",
    `$processId = ${process.pid}`,
    `$dataPath = ${toPowerShellLiteral(dataPath)}`,
    `$settingsPath = ${toPowerShellLiteral(settingsFilePath)}`,
    `$uninstallerPath = ${toPowerShellLiteral(uninstallerPath)}`,
    "Wait-Process -Id $processId",
    "Remove-Item -LiteralPath $dataPath -Recurse -Force",
    "Remove-Item -LiteralPath $settingsPath -Force",
    "Start-Process -FilePath $uninstallerPath -ArgumentList '/S'"
  ].join("; ")

  if (localBackupTimer) {
    clearInterval(localBackupTimer)
    localBackupTimer = null
  }

  spawn(
    "powershell.exe",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-WindowStyle",
      "Hidden",
      "-Command",
      cleanupScript
    ],
    {
      detached: true,
      stdio: "ignore",
      windowsHide: true
    }
  ).unref()

  isQuitting = true
  app.quit()
  return true
}

function dismissAppUpdate() {
  updatePromptOpen = false

  if (updateChecking || updateDownloading) {
    return getUpdateStatus()
  }

  return sendUpdateStatus({
    phase: "idle",
    message: "",
    manual: false,
    percent: 0,
    transferred: 0,
    total: 0,
    bytesPerSecond: 0
  })
}

async function checkForAppUpdates(manual = false) {
  if (manual) {
    await showMainPanel()
  }

  if (!app.isPackaged) {
    return sendUpdateStatus({
      phase: "dev-disabled",
      message:
        "开发模式没有打包后的更新元数据和安装器上下文，无法使用 electron-updater 完整检查并安装更新。请使用打包安装版验证更新流程。",
      manual: Boolean(manual)
    })
  }

  if (!updateConfigured) {
    return sendUpdateStatus({
      phase: "unconfigured",
      message: "当前安装包未包含更新配置。",
      manual: Boolean(manual)
    })
  }

  if (updateChecking || updateDownloading || updatePromptOpen) {
    return sendUpdateStatus({
      phase: updateDownloading
        ? "downloading"
        : updatePromptOpen
          ? updateStatus.phase
          : "checking",
      message: updateDownloading
        ? "更新正在下载中。"
        : updatePromptOpen
          ? updateStatus.message
          : "正在检查更新。",
      manual: Boolean(manual)
    })
  }

  updateManualCheck = Boolean(manual)
  updateChecking = true
  autoUpdater.checkForUpdates().catch(error => {
    updateChecking = false
    updateManualCheck = false
    sendUpdateStatus({
      phase: "error",
      message: error?.message || String(error),
      manual: Boolean(manual)
    })
  })
  return sendUpdateStatus({
    phase: "checking",
    message: "正在检查更新...",
    manual: Boolean(manual),
    percent: 0,
    transferred: 0,
    total: 0,
    bytesPerSecond: 0
  })
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
  const items = []

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
      click: () =>
        runTrayAction(() => {
          if (active) {
            return managerService.clearRuntime(cli.id)
          }

          return managerService.switchRuntime({
            cli: cli.id,
            providerId: provider.id,
            model
          })
        })
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
      enabled: !account.active,
      click: () =>
        runTrayAction(() => {
          return managerService.enableCodexAccount({
            accountId: account.id
          })
        })
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

function getCodexProxyActiveTarget(state) {
  if (!state.codexProxyState?.enabled) {
    return null
  }

  const targetId = String(state.codexProxyState.activeProviderId || "")

  if (targetId.startsWith("account:")) {
    const accountId = targetId.slice("account:".length)
    const account = (state.codexAccounts || []).find(item => item.id === accountId)

    return {
      type: "account",
      provider: null,
      account,
      name: account?.email || account?.accountId || targetId || "未激活"
    }
  }

  const provider = state.providers.find(item => item.id === targetId)

  return {
    type: "provider",
    provider,
    account: null,
    name: provider?.name || targetId || "未激活"
  }
}

function getCliTrayActiveName(state, cli) {
  if (cli.id === "codex") {
    const proxyTarget = getCodexProxyActiveTarget(state)

    if (proxyTarget) {
      return `Proxy 接管中：${proxyTarget.name}`
    }

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
  const proxyTarget = getCodexProxyActiveTarget(state)
  const activeProvider =
    proxyTarget?.provider ||
    providers.find(item => item.id === profile?.providerId)
  const activeAccount = proxyTarget?.account || accounts.find(item => item.active)
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

  if (proxyTarget) {
    items.push({
      label: "关闭 Proxy 接管",
      click: () => runTrayAction(() => managerService.disableCodexProxy())
    })
  } else if (activeProvider || activeAccount) {
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
    const active = proxyTarget
      ? provider.id === proxyTarget.provider?.id
      : !activeAccount && provider.id === activeProvider?.id

    items.push({
      label: active
        ? `${provider.name}（${proxyTarget ? "Proxy 当前" : "已启用"}）`
        : model
          ? provider.name
          : `${provider.name}（缺少模型）`,
      enabled: Boolean(model) && !active,
      click: () =>
        runTrayAction(async () => {
          if (proxyTarget) {
            await managerService.disableCodexProxy()
          }

          await managerService.clearCodexAccount()
          return managerService.switchRuntime({
            cli: cli.id,
            providerId: provider.id,
            model
          })
        })
    })
  }

  for (const account of accounts) {
    const active = proxyTarget
      ? proxyTarget.account?.id === account.id
      : account.active

    items.push({
      label: active
        ? `${formatTrayAccountLabel(account)}（${
            proxyTarget ? "Proxy 当前" : "已启用"
          }）`
        : formatTrayAccountLabel(account),
      enabled: !active && !account.disabled,
      click: () =>
        runTrayAction(async () => {
          if (proxyTarget) {
            await managerService.disableCodexProxy()
          }

          await managerService.clearRuntime(cli.id)
          return managerService.enableCodexAccount({
            accountId: account.id
          })
        })
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
      const proxyTarget = getCodexProxyActiveTarget(state)

      if (proxyTarget) {
        return {
          cli,
          provider: proxyTarget.provider,
          account: proxyTarget.account,
          proxy: true
        }
      }

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
  const cliLabel = target.proxy
    ? "P"
    : getTrayStatusText(target.cli?.name).slice(0, 1)
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

function createTrayIconImage() {
  return nativeImage.createFromPath(appIconPath)
}

function updateTrayMenu(state = managerService?.getState()) {
  if (!tray || !state) {
    return
  }

  tray.setImage(createTrayIconImage())
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
        label: "检查更新",
        click: () => {
          checkForAppUpdates(true)
        }
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

  tray = new Tray(createTrayIconImage())
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
        label: "检查更新",
        click: () => {
          checkForAppUpdates(true)
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
  mainWindow.webContents.once("did-finish-load", () =>
    setTimeout(checkForAppUpdates, 8000)
  )

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
  registerLoggedIpc("app-log:append", async (_, payload) => {
    appendAppCallLog(payload)
    return true
  })
  registerLoggedIpc("app-log:list", async () => ({
    logs: appCallLogs,
    filePath: getAppCallLogPath()
  }))
  registerLoggedIpc("app-log:clear", async () => {
    appCallLogs = []
    scheduleAppCallLogWrite()
    return {
      logs: appCallLogs,
      filePath: getAppCallLogPath()
    }
  })

  registerLoggedIpc("app:bootstrap", async () => {
    await managerReadyPromise
    return managerService.getState()
  })
  registerLoggedIpc("app:refresh", async () => managerService.refreshAll())
  registerLoggedIpc("app:check-updates", async () => checkForAppUpdates(true))
  registerLoggedIpc("app:update-status", async () => getUpdateStatus())
  registerLoggedIpc("app:update-download", async () => downloadAppUpdate())
  registerLoggedIpc("app:update-install", async (_, payload = {}) =>
    installAppUpdate(payload)
  )
  registerLoggedIpc("app:update-dismiss", async () => dismissAppUpdate())
  registerLoggedIpc("app:uninstall-without-trace", async () =>
    uninstallWithoutTrace()
  )
  registerLoggedIpc("app:close-action", async (_, payload) => {
    handleCloseAction(payload)
    return true
  })
  registerLoggedIpc("quick-switch:show-main", async () => {
    await showMainPanel()
    return true
  })
  registerLoggedIpc("quick-switch:set-collapsed", async (_, payload) => {
    setQuickSwitchCollapsed(payload?.collapsed)
    return true
  })
  registerLoggedIpc("quick-switch:move-by", async (_, payload) => {
    moveQuickSwitchWindowBy(payload)
    return true
  })

  registerLoggedIpc("settings:save", async (_, payload) => {
    const nextSettings = normalizeAppSettings(payload)
    fs.mkdirSync(nextSettings.dataPath, { recursive: true })
    applyAutoLaunchSetting(nextSettings)
    saveAppSettings(nextSettings)
    appSettings = nextSettings
    await syncQuickSwitchWindow()

    if (
      path.resolve(nextSettings.dataPath) !==
      path.resolve(app.getPath("userData"))
    ) {
      await managerService.updateAppSettings(appSettings)
      managerService.setAppSettings(appSettings, true)
      await pruneLocalBackups()
      restartLocalBackupTimer()
      return managerService.getState()
    }

    await managerService.updateAppSettings(appSettings)
    await pruneLocalBackups()
    restartLocalBackupTimer()
    return managerService.getState()
  })

  registerLoggedIpc("data:export", async () => {
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

  registerLoggedIpc("data:preview-restore", async () => {
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

    const content = await fsp.readFile(result.filePaths[0], "utf8")
    const restoreId = cacheRestoreBackup(content, {
      type: "file",
      filePath: result.filePaths[0]
    })

    return {
      canceled: false,
      restoreId,
      filePath: result.filePaths[0],
      preview: await managerService.previewDataBackupRestore(content)
    }
  })

  registerLoggedIpc("data:restore", async (_, payload = {}) => {
    const draft = getRestoreBackupDraft(payload.restoreId)

    await managerService.restoreDataBackup(draft.content, {
      choices: payload.choices || {}
    })
    restoreBackupDrafts.delete(payload.restoreId)

    return {
      canceled: false,
      state: await restartManagerService(appSettings)
    }
  })

  registerLoggedIpc("data:local-backups", async () => getLocalBackupsPayload())

  registerLoggedIpc("data:local-backup-now", async () => {
    const backup = await createLocalBackup()

    if (!backup) {
      throw new Error("本地自动备份正在进行，请稍后再试")
    }

    return {
      backup,
      ...(await getLocalBackupsPayload()),
      state: managerService.getState()
    }
  })

  registerLoggedIpc("data:local-backup-preview", async (_, payload = {}) => {
    const filePath = getLocalBackupPath(payload.backupId)
    const content = await fsp.readFile(filePath, "utf8")
    const restoreId = cacheRestoreBackup(content, {
      type: "local",
      backupId: payload.backupId,
      filePath
    })

    return {
      restoreId,
      fileName: path.basename(filePath),
      filePath,
      preview: await managerService.previewDataBackupRestore(content)
    }
  })

  registerLoggedIpc("data:local-backup-restore", async (_, payload = {}) => {
    const draft = getRestoreBackupDraft(payload.restoreId)

    await managerService.restoreDataBackup(draft.content, {
      choices: payload.choices || {}
    })
    restoreBackupDrafts.delete(payload.restoreId)

    return {
      canceled: false,
      ...(await getLocalBackupsPayload()),
      state: await restartManagerService(appSettings)
    }
  })

  registerLoggedIpc("data:cloud-push", async (_, payload) => {
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

  registerLoggedIpc("data:cloud-preview", async (_, payload) => {
    const cloudSync = normalizeCloudSyncSettings(payload)
    const content = await downloadWebDavBackup(cloudSync)
    const restoreId = cacheRestoreBackup(content, {
      type: "cloud",
      cloudSync
    })

    return JSON.parse(JSON.stringify({
      restoreId,
      fileName: cloudSync.fileName,
      preview: await managerService.previewDataBackupRestore(content)
    }))
  })

  registerLoggedIpc("data:cloud-inspect", async (_, payload) => {
    const cloudSync = normalizeCloudSyncSettings(payload)
    const content = await downloadWebDavBackup(cloudSync)

    return JSON.parse(JSON.stringify({
      fileName: cloudSync.fileName,
      backup: managerService.inspectDataBackup(content)
    }))
  })

  registerLoggedIpc("data:cloud-pull", async (_, payload) => {
    const draft = getRestoreBackupDraft(payload.restoreId)
    const cloudSync = normalizeCloudSyncSettings(
      payload.cloudSync || draft.source.cloudSync
    )
    const lastUpdatedAt = Date.now()

    await managerService.restoreDataBackup(draft.content, {
      choices: payload.choices || {}
    })
    restoreBackupDrafts.delete(payload.restoreId)
    appSettings = normalizeAppSettings({
      ...appSettings,
      cloudSync: {
        ...cloudSync,
        lastUpdatedAt
      }
    })
    saveAppSettings(appSettings)

    return JSON.parse(JSON.stringify({
      downloadedAt: lastUpdatedAt,
      fileName: cloudSync.fileName,
      state: await restartManagerService(appSettings)
    }))
  })

  registerLoggedIpc("system:select-directory", async (_, payload) => {
    const result = await dialog.showOpenDialog(mainWindow, {
      title: payload?.title || "选择目录",
      defaultPath: payload?.defaultPath || app.getPath("home"),
      properties: ["openDirectory", "createDirectory"]
    })

    return result.canceled ? "" : result.filePaths[0]
  })

  registerLoggedIpc("system:select-file", async (_, payload) => {
    const result = await dialog.showOpenDialog(mainWindow, {
      title: payload?.title || "选择文件",
      defaultPath: payload?.defaultPath || app.getPath("home"),
      filters: payload?.filters || [],
      properties: ["openFile"]
    })

    return result.canceled ? "" : result.filePaths[0]
  })

  registerLoggedIpc("skill:create", async (_, payload) => {
    await managerService.createSkill(payload)
    return managerService.getState()
  })

  registerLoggedIpc("skill:preview-import-from-cli", async (_, payload) => {
    return managerService.previewSkillsFromCli(payload?.targetId)
  })

  registerLoggedIpc("skill:import-from-cli", async (_, payload) => {
    await managerService.importSkillsFromCli(payload?.targetId, payload)
    return managerService.getState()
  })

  registerLoggedIpc("skill:import-from-zip", async (_, payload) => {
    await managerService.importSkillFromZip(payload?.zipPath)
    return managerService.getState()
  })

  registerLoggedIpc("skill:install", async (_, payload) => {
    await managerService.installSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  registerLoggedIpc("skill:uninstall", async (_, payload) => {
    await managerService.uninstallSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  registerLoggedIpc("skill:repair", async (_, payload) => {
    await managerService.repairSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  registerLoggedIpc("skill:files", async (_, payload) => {
    return managerService.getSkillFiles(payload.skillName)
  })

  registerLoggedIpc("skill-repository:add", async (_, payload) => {
    return managerService.addSkillRepository(payload)
  })

  registerLoggedIpc("skill-repository:refresh", async (_, payload) => {
    return managerService.refreshSkillRepository(payload.repositoryId)
  })

  registerLoggedIpc("skill-repository:remove", async (_, payload) => {
    return managerService.removeSkillRepository(payload.repositoryId)
  })

  registerLoggedIpc("skill-repository:install-skill", async (_, payload) => {
    await managerService.installSkillFromRepository(
      payload.repositoryId,
      payload.skillId
    )
    return managerService.getState()
  })

  registerLoggedIpc("repo:add", async (_, payload) => {
    await managerService.addRepo(payload)
    return managerService.getState()
  })

  registerLoggedIpc("repo:sync", async (_, payload) => {
    await managerService.syncRepo(payload.repoId)
    return managerService.getState()
  })

  registerLoggedIpc("repo:sync-all", async () => {
    await managerService.syncAllRepos()
    return managerService.getState()
  })

  registerLoggedIpc("repo:remove", async (_, payload) => {
    await managerService.removeRepo(payload.repoId)
    return managerService.getState()
  })

  registerLoggedIpc("git-tool:branches", async (_, payload) => {
    return managerService.scanGitToolBranches(payload.repoId)
  })

  registerLoggedIpc("git-tool:commits", async (_, payload) => {
    return managerService.listGitToolCommits(payload)
  })

  registerLoggedIpc("git-tool:commit-detail", async (_, payload) => {
    return managerService.getGitToolCommitDetail(payload)
  })

  registerLoggedIpc("git-tool:update-check-branch", async (_, payload) => {
    return managerService.updateGitToolCheckBranch(payload)
  })

  registerLoggedIpc("git-tool:clear-check-cache", async (_, payload) => {
    return managerService.clearGitToolCommitCheckCache(payload)
  })

  registerLoggedIpc("git-tool:check-commit", async (_, payload) => {
    return managerService.checkGitToolCommitOnBranch(payload)
  })

  registerLoggedIpc("git-tool:archive-branch", async (_, payload) => {
    return managerService.archiveGitToolBranch(payload)
  })

  registerLoggedIpc("git-tool:archives", async (_, payload) => {
    return managerService.listGitToolArchives(payload.repoId)
  })

  registerLoggedIpc("git-tool:archive-commits", async (_, payload) => {
    return managerService.listGitToolArchiveCommits(payload.archiveId)
  })

  registerLoggedIpc("git-tool:archive-commit-detail", async (_, payload) => {
    return managerService.getGitToolArchiveCommitDetail(payload)
  })

  registerLoggedIpc("git-tool:restore-archive", async (_, payload) => {
    return managerService.restoreGitToolArchive(payload)
  })

  registerLoggedIpc("git-tool:delete-archive", async (_, payload) => {
    return managerService.deleteGitToolArchive(payload.archiveId)
  })

  registerLoggedIpc("git-tool:stashes", async (_, payload) => {
    return managerService.listGitToolStashes(payload.repoId)
  })

  registerLoggedIpc("git-tool:stash-archives", async (_, payload) => {
    return managerService.listGitToolStashArchives(payload.repoId)
  })

  registerLoggedIpc("git-tool:stash-detail", async (_, payload) => {
    return managerService.getGitToolStashDetail(payload)
  })

  registerLoggedIpc("git-tool:stash-archive-detail", async (_, payload) => {
    return managerService.getGitToolStashArchiveDetail(payload)
  })

  registerLoggedIpc("git-tool:archive-stash", async (_, payload) => {
    return managerService.archiveGitToolStash(payload)
  })

  registerLoggedIpc("git-tool:restore-stash-archive", async (_, payload) => {
    return managerService.restoreGitToolStashArchive(payload.stashArchiveId)
  })

  registerLoggedIpc("git-tool:delete-stash-archive", async (_, payload) => {
    return managerService.deleteGitToolStashArchive(payload.stashArchiveId)
  })

  registerLoggedIpc("session:search", async (_, payload) => {
    return managerService.searchSessions(payload?.query)
  })

  registerLoggedIpc("session:messages", async (_, payload) => {
    return managerService.loadSessionMessages(payload?.sessionId)
  })

  registerLoggedIpc("usage:stats", async (_, payload) => {
    return managerService.getUsageStats(payload || {})
  })

  registerLoggedIpc("skill-usage:stats", async (_, payload) => {
    return managerService.getSkillUsageStats(payload || {})
  })

  registerLoggedIpc("usage:pricing", async () => {
    return managerService.getUsagePricing()
  })

  registerLoggedIpc("usage:save-pricing", async (_, payload) => {
    return managerService.saveUsagePricing(payload || {})
  })

  registerLoggedIpc("usage:sync", async (_, payload) => {
    return managerService.syncUsage(payload || {})
  })

  registerLoggedIpc("usage:export-image", async (_, payload = {}) => {
    const now = new Date()
    const timestamp = [
      now.getFullYear(),
      String(now.getMonth() + 1).padStart(2, "0"),
      String(now.getDate()).padStart(2, "0")
    ].join("") + `-${[
      String(now.getHours()).padStart(2, "0"),
      String(now.getMinutes()).padStart(2, "0"),
      String(now.getSeconds()).padStart(2, "0")
    ].join("")}`
    const fileName = [
      "export",
      ...(payload.filters || []),
      timestamp
    ]
      .map((item) =>
        String(item || "")
          .replace(/[<>:"/\\|?*\x00-\x1F]+/g, "-")
          .replace(/\s+/g, "")
          .replace(/^-+|-+$/g, "")
      )
      .filter(Boolean)
      .join("-")

    const result = await dialog.showSaveDialog(mainWindow, {
      title: "导出用量报告长图",
      defaultPath: path.join(
        app.getPath("desktop"),
        `${fileName}.png`
      ),
      filters: [{ name: "PNG 长图", extensions: ["png"] }]
    })

    if (result.canceled || !result.filePath) {
      return {
        canceled: true
      }
    }

    const reportQuery = new URLSearchParams({
      view: "usage",
      export: "usage-report",
      rangeType: String(payload.rangeType || "today"),
      startAt: String(payload.startAt || ""),
      endAt: String(payload.endAt || ""),
      appType: String(payload.appType || "all"),
      providerId: String(payload.providerId || "all"),
      requestSource: String(payload.requestSource || "all"),
      model: String(payload.model || "all"),
      displayCurrency: String(payload.displayCurrency || "USD")
    })
    const reportWindow = new BrowserWindow({
      width: 1200,
      height: 760,
      show: false,
      frame: false,
      skipTaskbar: true,
      paintWhenInitiallyHidden: true,
      autoHideMenuBar: true,
      icon: appIconPath,
      backgroundColor: "#ffffff",
      webPreferences: {
        preload: path.join(__dirname, "preload.cjs"),
        contextIsolation: true,
        nodeIntegration: false,
        devTools: false
      }
    })
    let wasAttached = false
    let screenshot = null

    try {
      const devServerUrl = process.env.VITE_DEV_SERVER_URL

      if (devServerUrl) {
        const reportUrl = new URL(devServerUrl)

        for (const [key, value] of reportQuery) {
          reportUrl.searchParams.set(key, value)
        }

        await reportWindow.loadURL(reportUrl.toString())
      } else {
        await reportWindow.loadFile(
          path.join(__dirname, "..", "dist", "index.html"),
          {
            query: Object.fromEntries(reportQuery)
          }
        )
      }

      await reportWindow.webContents.executeJavaScript(`
        new Promise((resolve) => {
          const waitReady = () => {
            if (window.__usageReportReady) {
              resolve(true)
              return
            }

            requestAnimationFrame(waitReady)
          }

          waitReady()
        })
      `)

      wasAttached = reportWindow.webContents.debugger.isAttached()
      if (!wasAttached) {
        reportWindow.webContents.debugger.attach("1.3")
      }

      const pageSize = await reportWindow.webContents.executeJavaScript(`({
        width: Math.ceil(document.documentElement.scrollWidth),
        height: Math.ceil(document.documentElement.scrollHeight)
      })`)
      await reportWindow.webContents.debugger.sendCommand("Page.enable")
      screenshot = await reportWindow.webContents.debugger.sendCommand(
        "Page.captureScreenshot",
        {
          format: "png",
          fromSurface: true,
          captureBeyondViewport: true,
          clip: {
            x: 0,
            y: 0,
            width: pageSize.width,
            height: pageSize.height,
            scale: 1
          }
        }
      )
    } finally {
      if (!wasAttached && reportWindow.webContents.debugger.isAttached()) {
        reportWindow.webContents.debugger.detach()
      }
      if (!reportWindow.isDestroyed()) {
        reportWindow.destroy()
      }
    }

    await fsp.writeFile(result.filePath, Buffer.from(screenshot.data, "base64"))

    return {
      canceled: false,
      filePath: result.filePath
    }
  })

  registerLoggedIpc("session:delete", async (_, payload) => {
    await managerService.deleteSession(payload.sessionId)
    return managerService.getState()
  })

  registerLoggedIpc("session:recycle-list", async () => {
    return managerService.listRecycledSessions()
  })

  registerLoggedIpc("session:restore", async (_, payload) => {
    await managerService.restoreSession(payload.sessionId)
    return managerService.getState()
  })

  registerLoggedIpc("session:purge", async (_, payload) => {
    await managerService.purgeSession(payload.sessionId)
    return true
  })

  registerLoggedIpc("provider:save", async (_, payload) => {
    return managerService.saveProvider(payload)
  })

  registerLoggedIpc("provider:delete", async (_, payload) => {
    return managerService.deleteProvider(payload.providerId)
  })

  registerLoggedIpc("rule:save", async (_, payload) => {
    return managerService.saveRule(payload)
  })

  registerLoggedIpc("rule:delete", async (_, payload) => {
    return managerService.deleteRule(payload.ruleId)
  })

  registerLoggedIpc("rule:toggle", async (_, payload) => {
    return managerService.toggleRule(payload)
  })

  registerLoggedIpc("rule:enable", async (_, payload) => {
    return managerService.enableRule(payload.ruleId)
  })

  registerLoggedIpc("rule:move", async (_, payload) => {
    return managerService.moveRule(payload)
  })

  registerLoggedIpc("rule:import-global", async (_, payload) => {
    return managerService.importRule(payload)
  })

  registerLoggedIpc("rule:preview-import-global", async (_, payload) => {
    return managerService.previewImportRule(payload)
  })

  registerLoggedIpc("rule:resolve-import-conflict", async (_, payload) => {
    return managerService.resolveRuleImportConflict(payload)
  })

  registerLoggedIpc("rule:compare", async (_, payload) => {
    return managerService.compareRule(payload)
  })

  registerLoggedIpc("rule:resolve-drift", async (_, payload) => {
    return managerService.resolveRuleDrift(payload)
  })

  registerLoggedIpc("codex-account:login", async (_, payload) => {
    return managerService.startCodexOfficialLogin(payload)
  })

  registerLoggedIpc("codex-account:cancel", async () => {
    return managerService.cancelCodexOfficialLogin()
  })

  registerLoggedIpc("codex-account:import-auth-json", async (_, payload) => {
    return managerService.importCodexAuthJson(payload)
  })

  registerLoggedIpc("codex-account:enable", async (_, payload) => {
    return managerService.enableCodexAccount(payload)
  })

  registerLoggedIpc("codex-account:clear", async () => {
    return managerService.clearCodexAccount()
  })

  registerLoggedIpc("codex-account:refresh", async (_, payload) => {
    return managerService.refreshCodexAccount(payload)
  })

  registerLoggedIpc("codex-account:disable", async (_, payload) => {
    return managerService.disableCodexAccount(payload)
  })

  registerLoggedIpc("codex-account:restore", async (_, payload) => {
    return managerService.restoreCodexAccount(payload)
  })

  registerLoggedIpc("codex-account:update-proxy", async (_, payload) => {
    return managerService.updateCodexAccountProxy(payload)
  })

  registerLoggedIpc("codex-account:detail", async (_, payload) => {
    return managerService.getCodexAccountDetail(payload)
  })

  registerLoggedIpc("codex-account:delete", async (_, payload) => {
    return managerService.deleteCodexAccount(payload)
  })

  registerLoggedIpc("claude-proxy:enable", async (_, payload) => {
    return managerService.enableClaudeProxy(payload)
  })

  registerLoggedIpc("claude-proxy:disable", async () => {
    return managerService.disableClaudeProxy()
  })

  registerLoggedIpc("claude-proxy:add-provider", async (_, payload) => {
    return managerService.addClaudeProxyProvider(payload)
  })

  registerLoggedIpc("claude-proxy:remove-provider", async (_, payload) => {
    return managerService.removeClaudeProxyProvider(payload)
  })

  registerLoggedIpc("claude-proxy:activate-provider", async (_, payload) => {
    return managerService.activateClaudeProxyProvider(payload)
  })

  registerLoggedIpc("codex-proxy:enable", async (_, payload) => {
    return managerService.enableCodexProxy(payload)
  })

  registerLoggedIpc("codex-proxy:disable", async () => {
    return managerService.disableCodexProxy()
  })

  registerLoggedIpc("codex-proxy:add-provider", async (_, payload) => {
    return managerService.addCodexProxyProvider(payload)
  })

  registerLoggedIpc("codex-proxy:remove-provider", async (_, payload) => {
    return managerService.removeCodexProxyProvider(payload)
  })

  registerLoggedIpc("codex-proxy:activate-provider", async (_, payload) => {
    return managerService.activateCodexProxyProvider(payload)
  })

  registerLoggedIpc("codex-proxy:save-account-model", async (_, payload) => {
    return managerService.saveCodexProxyAccountModel(payload)
  })

  registerLoggedIpc("codex:launch-provider-instance", async (_, payload) => {
    return managerService.launchCodexProviderInstance(payload)
  })

  registerLoggedIpc("runtime-model:save", async (_, payload) => {
    return managerService.saveRuntimeModel(payload)
  })

  registerLoggedIpc("runtime:switch", async (_, payload) => {
    return managerService.switchRuntime(payload)
  })

  registerLoggedIpc("runtime:clear", async (_, payload) => {
    return managerService.clearRuntime(payload.cli)
  })

  registerLoggedIpc("runtime:compare", async (_, payload) => {
    return managerService.compareRuntime(payload)
  })

  registerLoggedIpc("runtime:config", async (_, payload) => {
    return managerService.getRuntimeConfig(payload)
  })

  registerLoggedIpc("runtime:resolve-drift", async (_, payload) => {
    return managerService.resolveRuntimeDrift(payload)
  })

  registerLoggedIpc("runtime:env", async (_, payload) => {
    return managerService.buildRuntimeEnv(payload.cli)
  })

  registerLoggedIpc("system:open-path", async (_, payload) => {
    if (!payload?.targetPath) {
      return false
    }

    const result = await shell.openPath(payload.targetPath)

    if (result) {
      throw new Error(result)
    }

    return true
  })

  registerLoggedIpc("system:open-external", async (_, payload) => {
    if (!payload?.url) {
      return false
    }

    await shell.openExternal(payload.url)
    return true
  })

  registerLoggedIpc("translation:translate", async (_, payload) => {
    return translationService.translate(payload?.text)
  })
}

if (singleInstanceLock) {
  app.on("second-instance", () => {
    showMainPanel().catch(showTrayError)
  })

  app.whenReady().then(async () => {
    await initAppCallLogs()
    applyAutoLaunchSetting(appSettings)
    managerService = new ManagerService(app.getPath("userData"), appSettings)
    instrumentBackendServices(managerService)
    translationService = new TranslationService(app.getPath("userData"))
    instrumentBackendService("TranslationService", translationService)
    managerReadyPromise = managerService.init()
    managerService.on("state-changed", state => {
      sendStateChanged(state)
      updateTrayMenu(state)
    })

    registerIpc()
    createTray()
    restartLocalBackupTimer()
    managerReadyPromise
      .then(() => {
        updateTrayMenu()
      })
      .catch(showTrayError)
    await createWindow()
    await syncQuickSwitchWindow()
    setupAutoUpdater()

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

    if (localBackupTimer) {
      clearInterval(localBackupTimer)
      localBackupTimer = null
    }

    if (managerService) {
      await managerService.dispose()
    }
  })
}


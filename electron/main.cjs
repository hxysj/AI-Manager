const path = require('node:path')
const fs = require('node:fs')
const { app, BrowserWindow, Menu, ipcMain, shell } = require('electron')
const { ManagerService } = require('./services/manager-service.cjs')
const { TranslationService } = require('./services/translation-service.cjs')

let mainWindow = null
let managerService = null
let translationService = null
const userDataPath = 'D:\\ai-manager-data'

fs.mkdirSync(userDataPath, { recursive: true })
app.setPath('userData', userDataPath)

async function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1480,
    height: 980,
    minWidth: 1200,
    minHeight: 760,
    backgroundColor: '#ffffff',
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : 'default',
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false
    }
  })

  const devServerUrl = process.env.VITE_DEV_SERVER_URL

  if (devServerUrl) {
    await mainWindow.loadURL(devServerUrl)
  } else {
    await mainWindow.loadFile(path.join(__dirname, '..', 'dist', 'index.html'))
  }

  mainWindow.webContents.on('before-input-event', (event, input) => {
    if (input.type === 'keyDown' && input.key === 'F12') {
      mainWindow.webContents.toggleDevTools()
      event.preventDefault()
    }
  })

  mainWindow.webContents.on('context-menu', (_, params) => {
    const selectedText = params.selectionText.trim()

    if (!selectedText) {
      return
    }

    Menu.buildFromTemplate([
      {
        label: '翻译选中文本',
        click: () => {
          mainWindow.webContents.send('translation:selection-requested', {
            text: selectedText,
            x: params.x,
            y: params.y
          })
        }
      }
    ]).popup({ window: mainWindow })
  })

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

function registerIpc() {
  ipcMain.handle('app:bootstrap', async () => managerService.getState())
  ipcMain.handle('app:refresh', async () => managerService.refreshAll())

  ipcMain.handle('skill:create', async (_, payload) => {
    await managerService.createSkill(payload)
    return managerService.getState()
  })

  ipcMain.handle('skill:preview-import-from-cli', async (_, payload) => {
    return managerService.previewSkillsFromCli(payload?.targetId)
  })

  ipcMain.handle('skill:import-from-cli', async (_, payload) => {
    await managerService.importSkillsFromCli(payload?.targetId, payload?.skillNames)
    return managerService.getState()
  })

  ipcMain.handle('skill:install', async (_, payload) => {
    await managerService.installSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  ipcMain.handle('skill:uninstall', async (_, payload) => {
    await managerService.uninstallSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  ipcMain.handle('skill:repair', async (_, payload) => {
    await managerService.repairSkill(payload.skillName, payload.targetId)
    return managerService.getState()
  })

  ipcMain.handle('repo:add', async (_, payload) => {
    await managerService.addRepo(payload)
    return managerService.getState()
  })

  ipcMain.handle('repo:sync', async (_, payload) => {
    await managerService.syncRepo(payload.repoId)
    return managerService.getState()
  })

  ipcMain.handle('repo:sync-all', async () => {
    await managerService.syncAllRepos()
    return managerService.getState()
  })

  ipcMain.handle('repo:remove', async (_, payload) => {
    await managerService.removeRepo(payload.repoId)
    return managerService.getState()
  })

  ipcMain.handle('session:search', async (_, payload) => {
    return managerService.searchSessions(payload?.query)
  })

  ipcMain.handle('session:messages', async (_, payload) => {
    return managerService.loadSessionMessages(payload?.sessionId)
  })

  ipcMain.handle('session:delete', async (_, payload) => {
    await managerService.deleteSession(payload.sessionId)
    return managerService.getState()
  })

  ipcMain.handle('session:recycle-list', async () => {
    return managerService.listRecycledSessions()
  })

  ipcMain.handle('session:restore', async (_, payload) => {
    await managerService.restoreSession(payload.sessionId)
    return managerService.getState()
  })

  ipcMain.handle('session:purge', async (_, payload) => {
    await managerService.purgeSession(payload.sessionId)
    return true
  })

  ipcMain.handle('system:open-path', async (_, payload) => {
    if (!payload?.targetPath) {
      return false
    }

    const result = await shell.openPath(payload.targetPath)

    if (result) {
      throw new Error(result)
    }

    return true
  })

  ipcMain.handle('translation:translate', async (_, payload) => {
    return translationService.translate(payload?.text)
  })
}

app.whenReady().then(async () => {
  managerService = new ManagerService(app.getPath('userData'))
  translationService = new TranslationService(app.getPath('userData'))
  await managerService.init()
  managerService.on('state-changed', state => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send('state:changed', state)
    }
  })

  registerIpc()
  await createWindow()

  app.on('activate', async () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      await createWindow()
    }
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

app.on('before-quit', async () => {
  if (managerService) {
    await managerService.dispose()
  }
})

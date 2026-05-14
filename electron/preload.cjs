const { contextBridge, ipcRenderer } = require('electron')

contextBridge.exposeInMainWorld('aiManager', {
  bootstrap: () => ipcRenderer.invoke('app:bootstrap'),
  refresh: () => ipcRenderer.invoke('app:refresh'),
  createSkill: payload => ipcRenderer.invoke('skill:create', payload),
  installSkill: payload => ipcRenderer.invoke('skill:install', payload),
  uninstallSkill: payload => ipcRenderer.invoke('skill:uninstall', payload),
  repairSkill: payload => ipcRenderer.invoke('skill:repair', payload),
  addRepo: payload => ipcRenderer.invoke('repo:add', payload),
  syncRepo: payload => ipcRenderer.invoke('repo:sync', payload),
  syncAllRepos: () => ipcRenderer.invoke('repo:sync-all'),
  removeRepo: payload => ipcRenderer.invoke('repo:remove', payload),
  openPath: payload => ipcRenderer.invoke('system:open-path', payload),
  onStateChanged: callback => {
    const handler = (_, state) => callback(state)
    ipcRenderer.on('state:changed', handler)
    return () => ipcRenderer.removeListener('state:changed', handler)
  }
})

const { contextBridge, ipcRenderer } = require("electron")

contextBridge.exposeInMainWorld("aiManager", {
  bootstrap: () => ipcRenderer.invoke("app:bootstrap"),
  refresh: () => ipcRenderer.invoke("app:refresh"),
  saveSettings: (payload) => ipcRenderer.invoke("settings:save", payload),
  selectDirectory: (payload) =>
    ipcRenderer.invoke("system:select-directory", payload),
  createSkill: (payload) => ipcRenderer.invoke("skill:create", payload),
  previewSkillsFromCli: (payload) =>
    ipcRenderer.invoke("skill:preview-import-from-cli", payload),
  importSkillsFromCli: (payload) =>
    ipcRenderer.invoke("skill:import-from-cli", payload),
  installSkill: (payload) => ipcRenderer.invoke("skill:install", payload),
  uninstallSkill: (payload) => ipcRenderer.invoke("skill:uninstall", payload),
  repairSkill: (payload) => ipcRenderer.invoke("skill:repair", payload),
  addRepo: (payload) => ipcRenderer.invoke("repo:add", payload),
  syncRepo: (payload) => ipcRenderer.invoke("repo:sync", payload),
  syncAllRepos: () => ipcRenderer.invoke("repo:sync-all"),
  removeRepo: (payload) => ipcRenderer.invoke("repo:remove", payload),
  searchSessions: (payload) => ipcRenderer.invoke("session:search", payload),
  loadSessionMessages: (payload) =>
    ipcRenderer.invoke("session:messages", payload),
  deleteSession: (payload) => ipcRenderer.invoke("session:delete", payload),
  listRecycledSessions: () => ipcRenderer.invoke("session:recycle-list"),
  restoreSession: (payload) => ipcRenderer.invoke("session:restore", payload),
  purgeSession: (payload) => ipcRenderer.invoke("session:purge", payload),
  openPath: (payload) => ipcRenderer.invoke("system:open-path", payload),
  translateText: (payload) => ipcRenderer.invoke("translation:translate", payload),
  onTranslateSelection: (callback) => {
    const handler = (_, payload) => callback(payload)
    ipcRenderer.on("translation:selection-requested", handler)
    return () =>
      ipcRenderer.removeListener("translation:selection-requested", handler)
  },
  onStateChanged: (callback) => {
    const handler = (_, state) => callback(state)
    ipcRenderer.on("state:changed", handler)
    return () => ipcRenderer.removeListener("state:changed", handler)
  }
})

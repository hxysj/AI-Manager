const { contextBridge, ipcRenderer } = require("electron")

function toPlainPayload(payload) {
  return payload === undefined ? undefined : JSON.parse(JSON.stringify(payload))
}

contextBridge.exposeInMainWorld("aiManager", {
  bootstrap: () => ipcRenderer.invoke("app:bootstrap"),
  refresh: () => ipcRenderer.invoke("app:refresh"),
  saveSettings: (payload) =>
    ipcRenderer.invoke("settings:save", toPlainPayload(payload)),
  selectDirectory: (payload) =>
    ipcRenderer.invoke("system:select-directory", toPlainPayload(payload)),
  selectFile: (payload) =>
    ipcRenderer.invoke("system:select-file", toPlainPayload(payload)),
  createSkill: (payload) =>
    ipcRenderer.invoke("skill:create", toPlainPayload(payload)),
  previewSkillsFromCli: (payload) =>
    ipcRenderer.invoke(
      "skill:preview-import-from-cli",
      toPlainPayload(payload)
    ),
  importSkillsFromCli: (payload) =>
    ipcRenderer.invoke("skill:import-from-cli", toPlainPayload(payload)),
  importSkillFromZip: (payload) =>
    ipcRenderer.invoke("skill:import-from-zip", toPlainPayload(payload)),
  installSkill: (payload) =>
    ipcRenderer.invoke("skill:install", toPlainPayload(payload)),
  uninstallSkill: (payload) =>
    ipcRenderer.invoke("skill:uninstall", toPlainPayload(payload)),
  repairSkill: (payload) =>
    ipcRenderer.invoke("skill:repair", toPlainPayload(payload)),
  addRepo: (payload) => ipcRenderer.invoke("repo:add", toPlainPayload(payload)),
  syncRepo: (payload) =>
    ipcRenderer.invoke("repo:sync", toPlainPayload(payload)),
  syncAllRepos: () => ipcRenderer.invoke("repo:sync-all"),
  removeRepo: (payload) =>
    ipcRenderer.invoke("repo:remove", toPlainPayload(payload)),
  searchSessions: (payload) =>
    ipcRenderer.invoke("session:search", toPlainPayload(payload)),
  loadSessionMessages: (payload) =>
    ipcRenderer.invoke("session:messages", toPlainPayload(payload)),
  deleteSession: (payload) =>
    ipcRenderer.invoke("session:delete", toPlainPayload(payload)),
  listRecycledSessions: () => ipcRenderer.invoke("session:recycle-list"),
  restoreSession: (payload) =>
    ipcRenderer.invoke("session:restore", toPlainPayload(payload)),
  purgeSession: (payload) =>
    ipcRenderer.invoke("session:purge", toPlainPayload(payload)),
  saveProvider: (payload) =>
    ipcRenderer.invoke("provider:save", toPlainPayload(payload)),
  deleteProvider: (payload) =>
    ipcRenderer.invoke("provider:delete", toPlainPayload(payload)),
  startCodexOfficialLogin: () => ipcRenderer.invoke("codex-account:login"),
  cancelCodexOfficialLogin: () => ipcRenderer.invoke("codex-account:cancel"),
  saveRuntimeModel: (payload) =>
    ipcRenderer.invoke("runtime-model:save", toPlainPayload(payload)),
  switchRuntime: (payload) =>
    ipcRenderer.invoke("runtime:switch", toPlainPayload(payload)),
  clearRuntime: (payload) =>
    ipcRenderer.invoke("runtime:clear", toPlainPayload(payload)),
  getRuntimeEnv: (payload) =>
    ipcRenderer.invoke("runtime:env", toPlainPayload(payload)),
  openPath: (payload) =>
    ipcRenderer.invoke("system:open-path", toPlainPayload(payload)),
  openExternal: (payload) =>
    ipcRenderer.invoke("system:open-external", toPlainPayload(payload)),
  translateText: (payload) =>
    ipcRenderer.invoke("translation:translate", toPlainPayload(payload)),
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

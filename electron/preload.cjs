const { contextBridge, ipcRenderer } = require("electron")

function toPlainPayload(payload) {
  return payload === undefined ? undefined : JSON.parse(JSON.stringify(payload))
}

contextBridge.exposeInMainWorld("aiManager", {
  bootstrap: () => ipcRenderer.invoke("app:bootstrap"),
  refresh: () => ipcRenderer.invoke("app:refresh"),
  saveSettings: payload =>
    ipcRenderer.invoke("settings:save", toPlainPayload(payload)),
  selectDirectory: payload =>
    ipcRenderer.invoke("system:select-directory", toPlainPayload(payload)),
  selectFile: payload =>
    ipcRenderer.invoke("system:select-file", toPlainPayload(payload)),
  createSkill: payload =>
    ipcRenderer.invoke("skill:create", toPlainPayload(payload)),
  previewSkillsFromCli: payload =>
    ipcRenderer.invoke(
      "skill:preview-import-from-cli",
      toPlainPayload(payload)
    ),
  importSkillsFromCli: payload =>
    ipcRenderer.invoke("skill:import-from-cli", toPlainPayload(payload)),
  importSkillFromZip: payload =>
    ipcRenderer.invoke("skill:import-from-zip", toPlainPayload(payload)),
  installSkill: payload =>
    ipcRenderer.invoke("skill:install", toPlainPayload(payload)),
  uninstallSkill: payload =>
    ipcRenderer.invoke("skill:uninstall", toPlainPayload(payload)),
  repairSkill: payload =>
    ipcRenderer.invoke("skill:repair", toPlainPayload(payload)),
  addRepo: payload => ipcRenderer.invoke("repo:add", toPlainPayload(payload)),
  syncRepo: payload => ipcRenderer.invoke("repo:sync", toPlainPayload(payload)),
  syncAllRepos: () => ipcRenderer.invoke("repo:sync-all"),
  removeRepo: payload =>
    ipcRenderer.invoke("repo:remove", toPlainPayload(payload)),
  searchSessions: payload =>
    ipcRenderer.invoke("session:search", toPlainPayload(payload)),
  loadSessionMessages: payload =>
    ipcRenderer.invoke("session:messages", toPlainPayload(payload)),
  deleteSession: payload =>
    ipcRenderer.invoke("session:delete", toPlainPayload(payload)),
  listRecycledSessions: () => ipcRenderer.invoke("session:recycle-list"),
  restoreSession: payload =>
    ipcRenderer.invoke("session:restore", toPlainPayload(payload)),
  purgeSession: payload =>
    ipcRenderer.invoke("session:purge", toPlainPayload(payload)),
  saveProvider: payload =>
    ipcRenderer.invoke("provider:save", toPlainPayload(payload)),
  deleteProvider: payload =>
    ipcRenderer.invoke("provider:delete", toPlainPayload(payload)),
  saveRule: payload => ipcRenderer.invoke("rule:save", toPlainPayload(payload)),
  deleteRule: payload =>
    ipcRenderer.invoke("rule:delete", toPlainPayload(payload)),
  toggleRule: payload =>
    ipcRenderer.invoke("rule:toggle", toPlainPayload(payload)),
  enableRule: payload =>
    ipcRenderer.invoke("rule:enable", toPlainPayload(payload)),
  moveRule: payload => ipcRenderer.invoke("rule:move", toPlainPayload(payload)),
  importGlobalRule: payload =>
    ipcRenderer.invoke("rule:import-global", toPlainPayload(payload)),
  previewImportGlobalRule: payload =>
    ipcRenderer.invoke("rule:preview-import-global", toPlainPayload(payload)),
  resolveRuleImportConflict: payload =>
    ipcRenderer.invoke("rule:resolve-import-conflict", toPlainPayload(payload)),
  compareRule: payload =>
    ipcRenderer.invoke("rule:compare", toPlainPayload(payload)),
  resolveRuleDrift: payload =>
    ipcRenderer.invoke("rule:resolve-drift", toPlainPayload(payload)),
  startCodexOfficialLogin: payload =>
    ipcRenderer.invoke("codex-account:login", toPlainPayload(payload)),
  cancelCodexOfficialLogin: () => ipcRenderer.invoke("codex-account:cancel"),
  importCodexAuthJson: payload =>
    ipcRenderer.invoke(
      "codex-account:import-auth-json",
      toPlainPayload(payload)
    ),
  enableCodexAccount: payload =>
    ipcRenderer.invoke("codex-account:enable", toPlainPayload(payload)),
  clearCodexAccount: () => ipcRenderer.invoke("codex-account:clear"),
  refreshCodexAccount: payload =>
    ipcRenderer.invoke("codex-account:refresh", toPlainPayload(payload)),
  updateCodexAccountProxy: payload =>
    ipcRenderer.invoke("codex-account:update-proxy", toPlainPayload(payload)),
  getCodexAccountDetail: payload =>
    ipcRenderer.invoke("codex-account:detail", toPlainPayload(payload)),
  saveRuntimeModel: payload =>
    ipcRenderer.invoke("runtime-model:save", toPlainPayload(payload)),
  switchRuntime: payload =>
    ipcRenderer.invoke("runtime:switch", toPlainPayload(payload)),
  clearRuntime: payload =>
    ipcRenderer.invoke("runtime:clear", toPlainPayload(payload)),
  getRuntimeEnv: payload =>
    ipcRenderer.invoke("runtime:env", toPlainPayload(payload)),
  openPath: payload =>
    ipcRenderer.invoke("system:open-path", toPlainPayload(payload)),
  openExternal: payload =>
    ipcRenderer.invoke("system:open-external", toPlainPayload(payload)),
  translateText: payload =>
    ipcRenderer.invoke("translation:translate", toPlainPayload(payload)),
  onTranslateSelection: callback => {
    const handler = (_, payload) => callback(payload)
    ipcRenderer.on("translation:selection-requested", handler)
    return () =>
      ipcRenderer.removeListener("translation:selection-requested", handler)
  },
  onStateChanged: callback => {
    const handler = (_, state) => callback(state)
    ipcRenderer.on("state:changed", handler)
    return () => ipcRenderer.removeListener("state:changed", handler)
  }
})

const { contextBridge, ipcRenderer } = require("electron")

const rawInvoke = ipcRenderer.invoke.bind(ipcRenderer)

function toPlainPayload(payload) {
  try {
    return payload === undefined ? undefined : JSON.parse(JSON.stringify(payload))
  } catch (error) {
    rawInvoke("app-log:append", {
      channel: "preload:toPlainPayload",
      action: "error",
      status: "error",
      message: error.message
    }).catch(() => {})
    throw error
  }
}

function appendCallLog(input) {
  rawInvoke("app-log:append", toPlainPayload(input)).catch(() => {})
}

ipcRenderer.invoke = async (channel, ...args) => {
  if (String(channel).startsWith("app-log:")) {
    return rawInvoke(channel, ...args)
  }

  const traceId =
    `trace-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`
  const startedAt = Date.now()

  appendCallLog({
    traceId,
    scope: "renderer",
    service: "Preload",
    method: String(channel),
    channel,
    action: "start",
    status: "pending",
    payload: args[0]
  })

  try {
    const result = await rawInvoke(channel, ...args, {
      __traceMeta: true,
      traceId
    })

    appendCallLog({
      traceId,
      scope: "renderer",
      service: "Preload",
      method: String(channel),
      channel,
      action: "finish",
      status: "success",
      durationMs: Date.now() - startedAt
    })

    return result
  } catch (error) {
    appendCallLog({
      traceId,
      scope: "renderer",
      service: "Preload",
      method: String(channel),
      channel,
      action: "finish",
      status: "error",
      durationMs: Date.now() - startedAt,
      message: error.message
    })
    throw error
  }
}

contextBridge.exposeInMainWorld("aiManager", {
  bootstrap: () => ipcRenderer.invoke("app:bootstrap"),
  refresh: () => ipcRenderer.invoke("app:refresh"),
  checkForUpdates: () => ipcRenderer.invoke("app:check-updates"),
  getUpdateStatus: () => ipcRenderer.invoke("app:update-status"),
  downloadUpdate: () => ipcRenderer.invoke("app:update-download"),
  installUpdate: () => ipcRenderer.invoke("app:update-install"),
  dismissUpdate: () => ipcRenderer.invoke("app:update-dismiss"),
  showMainPanel: () => ipcRenderer.invoke("quick-switch:show-main"),
  setQuickSwitchCollapsed: payload =>
    ipcRenderer.invoke("quick-switch:set-collapsed", toPlainPayload(payload)),
  moveQuickSwitchBy: payload =>
    ipcRenderer.invoke("quick-switch:move-by", toPlainPayload(payload)),
  handleCloseAction: payload =>
    ipcRenderer.invoke("app:close-action", toPlainPayload(payload)),
  saveSettings: payload =>
    ipcRenderer.invoke("settings:save", toPlainPayload(payload)),
  exportDataBackup: () => ipcRenderer.invoke("data:export"),
  previewDataBackupRestore: () => ipcRenderer.invoke("data:preview-restore"),
  restoreDataBackup: payload =>
    ipcRenderer.invoke("data:restore", toPlainPayload(payload)),
  listLocalBackups: () => ipcRenderer.invoke("data:local-backups"),
  createLocalBackup: () => ipcRenderer.invoke("data:local-backup-now"),
  previewLocalBackupRestore: payload =>
    ipcRenderer.invoke("data:local-backup-preview", toPlainPayload(payload)),
  restoreLocalBackup: payload =>
    ipcRenderer.invoke("data:local-backup-restore", toPlainPayload(payload)),
  pushCloudBackup: payload =>
    ipcRenderer.invoke("data:cloud-push", toPlainPayload(payload)),
  previewCloudBackupRestore: payload =>
    ipcRenderer.invoke("data:cloud-preview", toPlainPayload(payload)),
  pullCloudBackup: payload =>
    ipcRenderer.invoke("data:cloud-pull", toPlainPayload(payload)),
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
  getUsageStats: payload =>
    ipcRenderer.invoke("usage:stats", toPlainPayload(payload)),
  getUsagePricing: () => ipcRenderer.invoke("usage:pricing"),
  saveUsagePricing: payload =>
    ipcRenderer.invoke("usage:save-pricing", toPlainPayload(payload)),
  syncUsage: () => ipcRenderer.invoke("usage:sync"),
  exportUsageReportImage: payload =>
    ipcRenderer.invoke("usage:export-image", toPlainPayload(payload)),
  getAppLogs: () => ipcRenderer.invoke("app-log:list"),
  clearAppLogs: () => ipcRenderer.invoke("app-log:clear"),
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
  disableCodexAccount: payload =>
    ipcRenderer.invoke("codex-account:disable", toPlainPayload(payload)),
  updateCodexAccountProxy: payload =>
    ipcRenderer.invoke("codex-account:update-proxy", toPlainPayload(payload)),
  getCodexAccountDetail: payload =>
    ipcRenderer.invoke("codex-account:detail", toPlainPayload(payload)),
  deleteCodexAccount: payload =>
    ipcRenderer.invoke("codex-account:delete", toPlainPayload(payload)),
  enableCodexProxy: payload =>
    ipcRenderer.invoke("codex-proxy:enable", toPlainPayload(payload)),
  disableCodexProxy: () => ipcRenderer.invoke("codex-proxy:disable"),
  addCodexProxyProvider: payload =>
    ipcRenderer.invoke("codex-proxy:add-provider", toPlainPayload(payload)),
  removeCodexProxyProvider: payload =>
    ipcRenderer.invoke("codex-proxy:remove-provider", toPlainPayload(payload)),
  activateCodexProxyProvider: payload =>
    ipcRenderer.invoke("codex-proxy:activate-provider", toPlainPayload(payload)),
  saveRuntimeModel: payload =>
    ipcRenderer.invoke("runtime-model:save", toPlainPayload(payload)),
  switchRuntime: payload =>
    ipcRenderer.invoke("runtime:switch", toPlainPayload(payload)),
  clearRuntime: payload =>
    ipcRenderer.invoke("runtime:clear", toPlainPayload(payload)),
  compareRuntime: payload =>
    ipcRenderer.invoke("runtime:compare", toPlainPayload(payload)),
  getRuntimeConfig: payload =>
    ipcRenderer.invoke("runtime:config", toPlainPayload(payload)),
  resolveRuntimeDrift: payload =>
    ipcRenderer.invoke("runtime:resolve-drift", toPlainPayload(payload)),
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
  },
  onUpdateStatus: callback => {
    const handler = (_, payload) => callback(payload)
    ipcRenderer.on("app:update-status", handler)
    return () => ipcRenderer.removeListener("app:update-status", handler)
  },
  onCloseRequested: callback => {
    const handler = () => callback()
    ipcRenderer.on("app:close-requested", handler)
    return () => ipcRenderer.removeListener("app:close-requested", handler)
  }
})

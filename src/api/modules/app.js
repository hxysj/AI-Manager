import { request, subscribe, subscribeReady } from "../request"

export const appApi = {
  bootstrap: () => request("app:bootstrap"),
  refresh: () => request("app:refresh"),
  ensureSessionsReady: () => request("app:ensure-sessions-ready"),
  ensureToolsReady: () => request("app:ensure-tools-ready"),
  ensureSkillsReady: () => request("app:ensure-skills-ready"),
  checkForUpdates: () => request("app:check-updates"),
  getUpdateStatus: () => request("app:update-status"),
  getReleaseAnnouncement: () => request("app:release-announcement"),
  dismissReleaseAnnouncement: () => request("app:release-announcement-dismiss"),
  downloadUpdate: () => request("app:update-download"),
  installUpdate: (payload) => request("app:update-install", payload),
  dismissUpdate: () => request("app:update-dismiss"),
  uninstallWithoutTrace: () => request("app:uninstall-without-trace"),
  showMainPanel: (payload) => request("quick-switch:show-main", payload),
  // 工具窗口独立打开并复用，首次页面就绪后再显示原生窗口。
  openToolWindow: (payload) => request("tool-window:open", payload),
  bootstrapToolWindow: () => request("tool-window:bootstrap"),
  markToolWindowReady: (payload) => request("tool-window:ready", payload),
  // 悬浮窗只读取已有状态，监听注册完成后再通知原生窗口显示。
  bootstrapQuickSwitch: () => request("quick-switch:bootstrap"),
  markQuickSwitchReady: () => request("quick-switch:ready"),
  onQuickSwitchVisibilityChanged: (callback) =>
    subscribeReady("quick-switch:visibility-changed", callback),
  setQuickSwitchCollapsed: (payload) =>
    request("quick-switch:set-collapsed", payload),
  moveQuickSwitchBy: (payload) => request("quick-switch:move-by", payload),
  handleCloseAction: (payload) => request("app:close-action", payload),
  getAppLogs: () => request("app-log:list"),
  clearAppLogs: () => request("app-log:clear"),
  onStateChanged: (callback) => subscribe("state:changed", callback),
  onStateChangedReady: (callback) => subscribeReady("state:changed", callback),
  onToolRequested: (callback) => subscribe("app:open-tool", callback),
  onScreenshotError: (callback) => subscribe("screenshot:error", callback),
  onUpdateStatus: (callback) => subscribe("app:update-status", callback),
  onCloseRequested: (callback) => subscribe("app:close-requested", callback)
}

import { request, subscribe, subscribeReady } from '../request'

export const appApi = {
  bootstrap: () => request('app:bootstrap'),
  refresh: () => request('app:refresh'),
  ensureSessionsReady: () => request('app:ensure-sessions-ready'),
  ensureToolsReady: () => request('app:ensure-tools-ready'),
  ensureSkillsReady: () => request('app:ensure-skills-ready'),
  checkForUpdates: () => request('app:check-updates'),
  getUpdateStatus: () => request('app:update-status'),
  downloadUpdate: () => request('app:update-download'),
  installUpdate: payload => request('app:update-install', payload),
  dismissUpdate: () => request('app:update-dismiss'),
  uninstallWithoutTrace: () => request('app:uninstall-without-trace'),
  showMainPanel: payload => request('quick-switch:show-main', payload),
  // 悬浮窗只读取已有状态，监听注册完成后再通知原生窗口显示。
  bootstrapQuickSwitch: () => request('quick-switch:bootstrap'),
  markQuickSwitchReady: () => request('quick-switch:ready'),
  onQuickSwitchVisibilityChanged: callback =>
    subscribeReady('quick-switch:visibility-changed', callback),
  setQuickSwitchCollapsed: payload =>
    request('quick-switch:set-collapsed', payload),
  moveQuickSwitchBy: payload => request('quick-switch:move-by', payload),
  handleCloseAction: payload => request('app:close-action', payload),
  getAppLogs: () => request('app-log:list'),
  clearAppLogs: () => request('app-log:clear'),
  onStateChanged: callback => subscribe('state:changed', callback),
  onStateChangedReady: callback => subscribeReady('state:changed', callback),
  onToolRequested: callback => subscribe('app:open-tool', callback),
  onScreenshotError: callback => subscribe('screenshot:error', callback),
  onUpdateStatus: callback => subscribe('app:update-status', callback),
  onCloseRequested: callback => subscribe('app:close-requested', callback)
}

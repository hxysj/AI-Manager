import { request, subscribe } from '../request'

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
  showMainPanel: () => request('quick-switch:show-main'),
  setQuickSwitchCollapsed: payload =>
    request('quick-switch:set-collapsed', payload),
  moveQuickSwitchBy: payload => request('quick-switch:move-by', payload),
  handleCloseAction: payload => request('app:close-action', payload),
  getAppLogs: () => request('app-log:list'),
  clearAppLogs: () => request('app-log:clear'),
  onStateChanged: callback => subscribe('state:changed', callback),
  onUpdateStatus: callback => subscribe('app:update-status', callback),
  onCloseRequested: callback => subscribe('app:close-requested', callback)
}

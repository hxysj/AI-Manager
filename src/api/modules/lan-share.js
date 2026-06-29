import { request, subscribe } from '../request'

export const lanShareApi = {
  getState: () => request('lan-share:state'),
  startService: payload => request('lan-share:start', payload),
  stopService: () => request('lan-share:stop'),
  addFiles: payload => request('lan-share:add-files', payload),
  removeFile: payload => request('lan-share:remove-file', payload),
  refreshFiles: () => request('lan-share:refresh-files'),
  listMessages: payload => request('lan-share:list-messages', payload),
  sendMessage: payload => request('lan-share:send-message', payload),
  createSession: payload => request('lan-share:create-session', payload),
  activateSession: payload => request('lan-share:activate-session', payload),
  deleteMessage: payload => request('lan-share:delete-message', payload),
  clearSession: payload => request('lan-share:clear-session', payload),
  deleteDeviceHistory: payload =>
    request('lan-share:delete-device-history', payload),
  onStateChanged: callback => subscribe('lan-share:state-changed', callback),
  onMessageCreated: callback =>
    subscribe('lan-share:message-created', callback),
  onDevicesChanged: callback =>
    subscribe('lan-share:devices-changed', callback)
}

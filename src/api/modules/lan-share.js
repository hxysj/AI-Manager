import { request, subscribe } from '../request'

export const lanShareApi = {
  getState: () => request('lan-share:state'),
  setAccessIp: payload => request('lan-share:set-access-ip', payload),
  getClipboardFiles: () => request('lan-share:clipboard-files'),
  connectPeer: payload => request('lan-share:connect-peer', payload),
  respondPairing: payload => request('lan-share:respond-pairing', payload),
  discardUploads: payload => request('lan-share:discard-uploads', payload),
  startService: payload => request('lan-share:start', payload),
  stopService: () => request('lan-share:stop'),
  addFiles: payload => request('lan-share:add-files', payload),
  removeFile: payload => request('lan-share:remove-file', payload),
  removeFiles: payload => request('lan-share:remove-files', payload),
  refreshFiles: () => request('lan-share:refresh-files'),
  exportFilesZip: payload => request('lan-share:export-files-zip', payload),
  listMessages: payload => request('lan-share:list-messages', payload),
  sendMessage: payload => request('lan-share:send-message', payload),
  createSession: payload => request('lan-share:create-session', payload),
  activateSession: payload => request('lan-share:activate-session', payload),
  deleteMessage: payload => request('lan-share:delete-message', payload),
  deleteMessages: payload => request('lan-share:delete-messages', payload),
  clearSession: payload => request('lan-share:clear-session', payload),
  deleteSession: payload => request('lan-share:delete-session', payload),
  deleteDeviceHistory: payload =>
    request('lan-share:delete-device-history', payload),
  deleteDevice: payload => request('lan-share:delete-device', payload),
  createGroup: payload => request('lan-share:create-group', payload),
  updateGroup: payload => request('lan-share:update-group', payload),
  removeGroupMember: payload =>
    request('lan-share:remove-group-member', payload),
  clearGroupMessages: payload =>
    request('lan-share:clear-group-messages', payload),
  deleteGroup: payload => request('lan-share:delete-group', payload),
  onStateChanged: callback => subscribe('lan-share:state-changed', callback),
  onMessageCreated: callback =>
    subscribe('lan-share:message-created', callback),
  onDevicesChanged: callback =>
    subscribe('lan-share:devices-changed', callback)
}

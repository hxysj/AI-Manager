import { request } from '../request'

export const providerApi = {
  getKeyUsage: payload => request('provider:key-usage', payload),
  saveProvider: payload => request('provider:save', payload),
  deleteProvider: payload => request('provider:delete', payload)
}

export const claudeDesktopApi = {
  getState: () => request('claude-desktop:state'),
  getConfig: () => request('claude-desktop:config'),
  saveProvider: payload => request('claude-desktop:save', payload),
  setProviderEnabled: (providerId, enabled) => request('claude-desktop:set-enabled', { providerId, enabled }),
  switchProvider: providerId => request('claude-desktop:switch', { providerId }),
  clearProvider: () => request('claude-desktop:clear'),
  deleteProvider: providerId => request('claude-desktop:delete', { providerId }),
  importProviders: () => request('claude-desktop:import'),
  setGateway: payload => request('claude-desktop:gateway', payload)
}

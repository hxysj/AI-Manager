import { request } from '../request'

export const providerApi = {
  // 获取成功后，已有 Claude 供应商的模型列表由后端单独缓存。
  fetchClaudeModels: payload => request('claude-models:fetch', payload),
  fetchCodexModels: payload => request('codex-models:fetch', payload),
  syncCodexModels: payload => request('codex-models:sync', payload),
  getKeyValue: payload => request('provider:key-value', payload),
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
  previewImport: () => request('claude-desktop:import-preview'),
  importProviders: providerIds => request('claude-desktop:import', { providerIds }),
  setGateway: payload => request('claude-desktop:gateway', payload)
}

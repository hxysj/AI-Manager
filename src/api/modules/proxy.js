import { request } from '../request'

export const proxyApi = {
  enableClaudeProxy: payload => request('claude-proxy:enable', payload),
  disableClaudeProxy: () => request('claude-proxy:disable'),
  addClaudeProxyProvider: payload =>
    request('claude-proxy:add-provider', payload),
  removeClaudeProxyProvider: payload =>
    request('claude-proxy:remove-provider', payload),
  activateClaudeProxyProvider: payload =>
    request('claude-proxy:activate-provider', payload),
  enableCodexProxy: payload => request('codex-proxy:enable', payload),
  disableCodexProxy: () => request('codex-proxy:disable'),
  addCodexProxyProvider: payload =>
    request('codex-proxy:add-provider', payload),
  removeCodexProxyProvider: payload =>
    request('codex-proxy:remove-provider', payload),
  activateCodexProxyProvider: payload =>
    request('codex-proxy:activate-provider', payload),
  saveCodexProxyAccountModel: payload =>
    request('codex-proxy:save-account-model', payload)
}

import { request } from '../request'

// Google 登录与额度只通过后端操作，前端不接触 OAuth Token。
export const googleAccountApi = {
  login: payload => request('google-account:login', payload),
  cancel: () => request('google-account:cancel'),
  state: () => request('google-account:state'),
  refresh: providerId => request('google-account:refresh', { providerId }),
  save: payload => request('google-account:save', payload)
}

export const accountApi = {
  startCodexOfficialLogin: payload => request('codex-account:login', payload),
  cancelCodexOfficialLogin: () => request('codex-account:cancel'),
  importCodexAuthJson: payload =>
    request('codex-account:import-auth-json', payload),
  enableCodexAccount: payload => request('codex-account:enable', payload),
  clearCodexAccount: () => request('codex-account:clear'),
  refreshCodexAccount: payload => request('codex-account:refresh', payload),
  disableCodexAccount: payload => request('codex-account:disable', payload),
  restoreCodexAccount: payload => request('codex-account:restore', payload),
  updateCodexAccountProxy: payload =>
    request('codex-account:update-proxy', payload),
  getCodexAccountDetail: payload => request('codex-account:detail', payload),
  deleteCodexAccount: payload => request('codex-account:delete', payload)
}

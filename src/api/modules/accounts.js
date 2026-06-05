import { request } from '../request'

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

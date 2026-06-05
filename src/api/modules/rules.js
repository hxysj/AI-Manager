import { request } from '../request'

export const ruleApi = {
  saveRule: payload => request('rule:save', payload),
  deleteRule: payload => request('rule:delete', payload),
  toggleRule: payload => request('rule:toggle', payload),
  enableRule: payload => request('rule:enable', payload),
  moveRule: payload => request('rule:move', payload),
  importGlobalRule: payload => request('rule:import-global', payload),
  previewImportGlobalRule: payload =>
    request('rule:preview-import-global', payload),
  resolveRuleImportConflict: payload =>
    request('rule:resolve-import-conflict', payload),
  compareRule: payload => request('rule:compare', payload),
  resolveRuleDrift: payload => request('rule:resolve-drift', payload)
}

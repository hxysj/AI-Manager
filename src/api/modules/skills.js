import { request } from '../request'

export const skillApi = {
  createSkill: payload => request('skill:create', payload),
  previewSkillsFromCli: payload =>
    request('skill:preview-import-from-cli', payload),
  importSkillsFromCli: payload => request('skill:import-from-cli', payload),
  importSkillFromZip: payload => request('skill:import-from-zip', payload),
  installSkill: payload => request('skill:install', payload),
  uninstallSkill: payload => request('skill:uninstall', payload),
  batchSkillAction: payload => request('skill:batch-action', payload),
  deleteSkills: payload => request('skill:delete', payload),
  repairSkill: payload => request('skill:repair', payload),
  setSkillEnabled: payload => request('skill:set-enabled', payload),
  getSkillFiles: payload => request('skill:files', payload),
  getSkillGroups: payload => request('skill-groups:list', payload),
  saveSkillGroup: payload => request('skill-groups:save', payload),
  removeSkillGroup: payload => request('skill-groups:remove', payload),
  removeSkillGroupItems: payload => request('skill-groups:remove-items', payload),
  getSkillTrash: payload => request('skill-trash:list', payload),
  restoreSkillTrash: payload => request('skill-trash:restore', payload),
  purgeSkillTrash: payload => request('skill-trash:purge', payload),
  addSkillRepository: payload => request('skill-repository:add', payload),
  refreshSkillRepository: payload =>
    request('skill-repository:refresh', payload),
  removeSkillRepository: payload => request('skill-repository:remove', payload),
  installSkillFromRepository: payload =>
    request('skill-repository:install-skill', payload)
}

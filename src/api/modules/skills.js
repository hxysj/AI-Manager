import { request } from '../request'

export const skillApi = {
  createSkill: payload => request('skill:create', payload),
  previewSkillsFromCli: payload =>
    request('skill:preview-import-from-cli', payload),
  importSkillsFromCli: payload => request('skill:import-from-cli', payload),
  importSkillFromZip: payload => request('skill:import-from-zip', payload),
  installSkill: payload => request('skill:install', payload),
  uninstallSkill: payload => request('skill:uninstall', payload),
  repairSkill: payload => request('skill:repair', payload),
  getSkillFiles: payload => request('skill:files', payload),
  addSkillRepository: payload => request('skill-repository:add', payload),
  refreshSkillRepository: payload =>
    request('skill-repository:refresh', payload),
  removeSkillRepository: payload => request('skill-repository:remove', payload),
  installSkillFromRepository: payload =>
    request('skill-repository:install-skill', payload)
}

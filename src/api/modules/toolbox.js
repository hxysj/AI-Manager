import { request } from '../request'

export const toolboxApi = {
  openToolbox: () => request('tools:open-toolbox'),
  listCodexPets: () => request('tools:codex-pets'),
  renameCodexPet: payload => request('tools:rename-codex-pet', payload),
  toggleCodexPet: payload => request('tools:toggle-codex-pet', payload),
  deleteCodexPet: payload => request('tools:delete-codex-pet', payload)
}

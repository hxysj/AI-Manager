import { request } from '../request'

export const toolboxApi = {
  exportImages: payload => request('tools:export-images', payload),
  // 端口写操作始终携带启动时间，由后端再次校验进程身份。
  listPorts: () => request('tools:list-ports'),
  terminatePortProcess: payload =>
    request('tools:terminate-port-process', payload),
  listCodexPets: () => request('tools:codex-pets'),
  renameCodexPet: payload => request('tools:rename-codex-pet', payload),
  toggleCodexPet: payload => request('tools:toggle-codex-pet', payload),
  deleteCodexPet: payload => request('tools:delete-codex-pet', payload)
}

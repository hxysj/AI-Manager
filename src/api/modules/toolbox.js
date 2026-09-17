import { request } from "../request"

export const toolboxApi = {
  // 提示词库分页读取，完整提示词只在打开详情时加载。
  imagePromptCatalog: () => request('tools:image-prompts-catalog'),
  listImagePrompts: payload => request('tools:image-prompts-list', payload),
  imagePromptDetail: payload => request('tools:image-prompts-detail', payload),
  importImagePrompts: payload => request('tools:image-prompts-import', payload),
  // 图片生成只由后端读取官方账号凭据，前端只传账号 ID。
  imageAccounts: () => request("tools:image-accounts"),
  submitImageTask: (payload) => request("tools:image-submit", payload),
  listImageTasks: (payload) => request("tools:image-list", payload),
  imageTaskDetail: (payload) => request("tools:image-detail", payload),
  imageTaskInputs: payload => request('tools:image-inputs', payload),
  deleteImageTasks: (payload) => request("tools:image-delete", payload),
  exportImageTasks: (payload) => request("tools:image-export", payload),
  exportImages: (payload) => request("tools:export-images", payload),
  // JSON Agent 请求由后端注入当前 Codex Provider 的密钥和地址。
  requestJsonAgent: (payload) => request("tools:json-agent-request", payload),
  // 端口写操作始终携带启动时间，由后端再次校验进程身份。
  listPorts: () => request("tools:list-ports"),
  terminatePortProcess: (payload) =>
    request("tools:terminate-port-process", payload),
  listCodexPets: () => request("tools:codex-pets"),
  renameCodexPet: (payload) => request("tools:rename-codex-pet", payload),
  toggleCodexPet: (payload) => request("tools:toggle-codex-pet", payload),
  deleteCodexPet: (payload) => request("tools:delete-codex-pet", payload)
}

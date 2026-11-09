import { request } from "@/api/request"

// 模型目录与下载清单由后端管理，前端只传内置模型标识。
export const localModelsApi = {
  list: () => request("local-models:list"),
  // 调用方按能力读取已下载模型，推理时由后端按需加载。
  installed: (capability) => request("local-models:installed", { capability }),
  generate: (payload) => request("local-models:generate", payload),
  download: (payload) => request("local-models:download", payload),
  cancel: (id) => request("local-models:cancel", { id }),
  delete: (id) => request("local-models:delete", { id })
}

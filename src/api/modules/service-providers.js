import { request } from "@/api/request"

// 新配置中心独立命名空间，避免调用旧 Provider 的迁移和保存路径。
export const serviceProviderApi = {
  invoke: (action, payload) => request(`service-provider:${action}`, payload)
}

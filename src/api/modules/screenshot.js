import { request } from "@/api/request"

// 图像仅通过专用通道传输，避免进入通用操作日志。
export const screenshotApi = {
  call: (action, payload) => request(`screenshot:${action}`, payload)
}

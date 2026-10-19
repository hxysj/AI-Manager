import { request } from "@/api/request"
import { invoke } from "@tauri-apps/api/core"

// 图像仅通过专用通道传输，避免进入通用操作日志。
export const screenshotApi = {
  // 框选首帧直接接收二进制像素，避免图片压缩与解码等待。
  frame: () => invoke("screenshot_frame"),
  call: (action, payload) => request(`screenshot:${action}`, payload)
}

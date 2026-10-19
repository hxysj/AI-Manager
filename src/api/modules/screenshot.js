import { request } from "@/api/request"
import { invoke } from "@tauri-apps/api/core"

function binaryBuffer(data) {
  if (data instanceof ArrayBuffer) return data
  if (ArrayBuffer.isView(data))
    return data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength)
  // IPC 降级为 postMessage 时，原生字节会作为普通数组返回。
  // 必须先还原为二进制，直接传给 Blob 会变成逗号分隔的文本。
  if (Array.isArray(data)) return new Uint8Array(data).buffer
  throw new Error("截图返回的图片数据格式无效")
}

// 图像仅通过专用通道传输，避免进入通用操作日志。
export const screenshotApi = {
  // 框选首帧直接接收二进制像素，避免图片压缩与解码等待。
  frame: async () => binaryBuffer(await invoke("screenshot_frame")),
  // 贴图只加载需要的 PNG，避免两张大图经 Base64 和 JSON 重复传输。
  image: async (id, kind) =>
    binaryBuffer(await invoke("screenshot_image", { id, kind })),
  call: (action, payload) => request(`screenshot:${action}`, payload)
}

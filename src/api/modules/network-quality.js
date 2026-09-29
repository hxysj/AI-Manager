import { request, subscribeReady } from "../request"

export const networkQualityApi = {
  run: (payload) => request("network-quality:run", payload),
  cancel: (payload) => request("network-quality:cancel", payload),
  demo: (payload) => request("network-quality:demo", payload),
  exportReport: (payload) => request("network-quality:export", payload),
  // 长任务进度通过事件推送，调用方负责按 runId 丢弃过期事件。
  onProgress: (callback) => subscribeReady("network-quality:progress", callback)
}

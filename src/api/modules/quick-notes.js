import { request, subscribeReady } from "../request"

// 工具页、主助手和桌面助手共享速记数据，数据变更只刷新本模块。
export const quickNotesApi = {
  invoke: (action, payload = {}) => request(`quick-notes:${action}`, payload),
  onChanged: (callback) => subscribeReady("quick-notes:changed", callback),
  onAssistant: (callback) => subscribeReady("quick-notes:assistant", callback),
  onOpen: (callback) => subscribeReady("quick-notes:open", callback)
}

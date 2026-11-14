import { request, subscribeReady } from "../request"

// 助手任务由后台持有，窗口关闭后仍可通过快照恢复进度。
export const assistantApi = {
  bootstrap: () => request("assistant:bootstrap"),
  catalog: () => request("assistant:catalog"),
  getState: () => request("assistant:state"),
  listThreads: (page) => request("assistant:threads", { page }),
  createThread: () => request("assistant:create-thread"),
  messages: (threadId, page = 0) =>
    request("assistant:messages", { threadId, page }),
  deleteThread: (threadId) => request("assistant:delete-thread", { threadId }),
  send: (payload) => request("assistant:send", payload),
  context: (threadId) => request("assistant:context", { threadId }),
  compress: (threadId) => request("assistant:compress", { threadId }),
  publicMemory: () => request("assistant:public-memory"),
  savePublicMemory: (text, previous) =>
    request("assistant:save-public-memory", { text, previous }),
  desktopSend: (message, images = []) =>
    request("assistant:desktop-send", { message, images }),
  desktopOpen: () => request("assistant:desktop-open"),
  desktopReady: () => request("assistant:desktop-ready"),
  desktopClose: () => request("assistant:desktop-close"),
  desktopStatus: () => request("assistant:desktop-status"),
  onDesktopOpen: (callback) =>
    subscribeReady("assistant:desktop-open", callback),
  onDesktopStatus: (callback) =>
    subscribeReady("assistant:desktop-status", callback),
  cancel: (runId) => request("assistant:cancel", { runId }),
  onEvent: (callback) => subscribeReady("assistant:event", callback),
  editorPending: () => request("assistant:editor-pending"),
  editorClaim: (payload) => request("assistant:editor-claim", payload),
  editorResult: (payload) => request("assistant:editor-result", payload),
  onEditorRequest: (callback) =>
    subscribeReady("assistant:editor-request", callback)
}

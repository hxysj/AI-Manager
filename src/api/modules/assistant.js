import { request, subscribeReady } from "../request"

// 助手任务由后台持有，窗口关闭后仍可通过快照恢复进度。
export const assistantApi = {
  bootstrap: () => request("assistant:bootstrap"),
  getState: () => request("assistant:state"),
  listThreads: (page) => request("assistant:threads", { page }),
  createThread: () => request("assistant:create-thread"),
  messages: (threadId, page = 0) =>
    request("assistant:messages", { threadId, page }),
  deleteThread: (threadId) => request("assistant:delete-thread", { threadId }),
  send: (payload) => request("assistant:send", payload),
  cancel: (runId) => request("assistant:cancel", { runId }),
  onEvent: (callback) => subscribeReady("assistant:event", callback),
  editorPending: () => request("assistant:editor-pending"),
  editorClaim: (payload) => request("assistant:editor-claim", payload),
  editorResult: (payload) => request("assistant:editor-result", payload),
  onEditorRequest: (callback) =>
    subscribeReady("assistant:editor-request", callback)
}

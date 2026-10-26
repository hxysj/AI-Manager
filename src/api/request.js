import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { createLogContext, reportAppError } from "@/utils/app-logger"

export function request(channel, payload) {
  if (payload === undefined) {
    return invokeWithDiagnostics("dispatch_api", { channel })
  }

  return invokeWithDiagnostics("dispatch_api", { channel, payload })
}

// 二进制专用接口也捕获失败；普通调用的上下文会一同传给后端落盘。
export async function invokeWithDiagnostics(command, args = {}) {
  const context = createLogContext()
  const startedAt = performance.now()
  const channel = args.channel || command
  try {
    return await invoke(
      command,
      command === "dispatch_api" ? { ...args, context } : args
    )
  } catch (error) {
    if (!channel.startsWith("app-log:")) {
      reportAppError("ipc-error", error, {
        channel,
        context,
        durationMs: Math.round(performance.now() - startedAt)
      })
    }
    throw error
  }
}

export function subscribe(eventName, callback) {
  let stopped = false
  let unlisten = null

  listen(eventName, (event) => {
    if (!stopped) {
      callback(event.payload)
    }
  }).then((handler) => {
    if (stopped) {
      handler()
      return
    }

    unlisten = handler
  })

  return () => {
    stopped = true

    if (unlisten) {
      unlisten()
    }
  }
}

// 需要立即触发后端任务时，先等待事件监听真正注册完成。
export async function subscribeReady(eventName, callback) {
  return listen(eventName, (event) => callback(event.payload))
}

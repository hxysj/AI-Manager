import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

export function request(channel, payload) {
  if (payload === undefined) {
    return invoke("dispatch_api", { channel })
  }

  return invoke("dispatch_api", { channel, payload })
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

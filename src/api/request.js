import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export function request(channel, payload) {
  if (payload === undefined) {
    return invoke('dispatch_api', { channel })
  }

  return invoke('dispatch_api', { channel, payload })
}

export function subscribe(eventName, callback) {
  let stopped = false
  let unlisten = null

  listen(eventName, event => {
    if (!stopped) {
      callback(event.payload)
    }
  }).then(handler => {
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

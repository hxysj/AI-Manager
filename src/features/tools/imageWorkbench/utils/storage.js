import { createMessage } from "@/utils/message"

export function readLocal(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(key) || "null") ?? fallback
  } catch {
    return fallback
  }
}

export function writeLocal(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    createMessage.error("本地存储空间不足，当前更改未持久化")
  }
}

// 翻译和助手共用录制规则，保存的键名与原生快捷键解析保持一致。
export function shortcutFromEvent(event) {
  if (event.key === "Tab" || event.isComposing) return null
  event.preventDefault()
  if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return null
  const key =
    event.code === "Space"
      ? "Space"
      : event.key.length === 1
        ? event.key.toUpperCase()
        : event.key
  if (!/^[A-Z0-9]$|^F([1-9]|1[0-9]|2[0-4])$|^(PrintScreen|Space)$/.test(key))
    throw new Error("请使用字母、数字、F1–F24、PrintScreen 或空格")
  if (
    !event.ctrlKey &&
    !event.altKey &&
    !event.shiftKey &&
    !event.metaKey &&
    !/^F\d+$|^PrintScreen$/.test(key)
  )
    throw new Error("请同时按下 Ctrl、Alt、Shift 或 Win 修饰键")
  return [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Win",
    key
  ]
    .filter(Boolean)
    .join("+")
}

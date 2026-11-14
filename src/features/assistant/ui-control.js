import { nextTick } from "vue"
import { isTauri } from "@tauri-apps/api/core"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { assistantApi } from "@/api/modules/assistant"
import { reportAppError } from "@/utils/app-logger"

function visible(element) {
  if (!element.isConnected || !element.getClientRects().length) return false
  if (element.closest('[hidden], [inert], [aria-hidden="true"]')) return false
  return !["hidden", "collapse"].includes(
    window.getComputedStyle(element).visibility
  )
}

function controlName(element) {
  const labelledBy = (element.getAttribute("aria-labelledby") || "")
    .split(/\s+/)
    .map((id) => document.getElementById(id)?.textContent || "")
    .join(" ")
  const label = element.labels?.[0] || element.closest("label")
  let labelText = ""
  if (label) {
    const copy = label.cloneNode(true)
    copy
      .querySelectorAll("input, textarea, select, button")
      .forEach((control) => control.remove())
    labelText = copy.textContent
  }
  return (
    element.getAttribute("aria-label") ||
    labelledBy.trim() ||
    labelText.trim() ||
    element.getAttribute("placeholder") ||
    element.getAttribute("title") ||
    element.innerText ||
    element.textContent ||
    ""
  )
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 300)
}

function privateControl(element) {
  return (
    element.type === "password" ||
    Boolean(element.closest("[data-assistant-private], .monaco-editor")) ||
    /api[\s_-]*key|token|password|secret|authorization|cookie|密钥|密码|令牌|凭据/i.test(
      `${element.id || ""} ${element.name || ""} ${controlName(element)}`
    )
  )
}

// 控件引用只在最近一次读取后有效，页面重建或用户改写内容后要求重新读取。
export function createAppUiController() {
  let snapshotId = ""
  const controls = new Map()

  function inspect(args = {}) {
    snapshotId = crypto.randomUUID()
    controls.clear()
    const elements = Array.from(
      document.body.querySelectorAll(
        'button, input, textarea, select, a[href], [role="button"], [role="option"], [role="tab"], [role="checkbox"], [role="switch"], [role="combobox"], [contenteditable="true"], [tabindex]'
      )
    ).filter((element) => visible(element) && !privateControl(element))
    const nodes = elements.slice(0, 2000).map((element, index) => {
      const node = {
        nodeId: String(index + 1),
        tag: element.tagName.toLowerCase(),
        role: element.getAttribute("role") || "",
        name: controlName(element),
        type: element.type || "",
        disabled: Boolean(
          element.matches(":disabled") ||
          element.getAttribute("aria-disabled") === "true"
        ),
        readOnly: Boolean(element.readOnly),
        focused: document.activeElement === element
      }
      if (element.matches("input, textarea, select"))
        node.value = element.value.slice(0, 30000)
      if (element.isContentEditable)
        node.value = element.textContent.slice(0, 30000)
      if (element.matches('input[type="checkbox"], input[type="radio"]'))
        node.checked = element.checked
      if (element.maxLength >= 0) node.maxLength = element.maxLength
      if (element.tagName === "SELECT") {
        node.options = Array.from(element.options)
          .map((option) => ({
            value: option.value,
            label: option.label,
            disabled: option.disabled
          }))
          .slice(0, 200)
      }
      controls.set(node.nodeId, { element, node })
      return node
    })
    const walker = document.createTreeWalker(
      document.body,
      NodeFilter.SHOW_TEXT
    )
    let text = ""
    let current
    let examined = 0
    while (
      (current = walker.nextNode()) &&
      text.length < 8000 &&
      examined++ < 10000
    ) {
      const parent = current.parentElement
      if (
        !parent ||
        parent.closest(
          "script, style, textarea, [contenteditable], .monaco-editor"
        ) ||
        !visible(parent) ||
        privateControl(parent)
      )
        continue
      const value = current.textContent.replace(/\s+/g, " ").trim()
      if (value) text += `${value}\n`
    }
    const offset = Math.max(0, Math.trunc(args.offset || 0))
    const limit = Math.min(200, Math.max(1, Math.trunc(args.limit || 80)))
    return {
      snapshotId,
      title: document.title,
      text: text.slice(0, 8000),
      textTruncated: Boolean(current),
      nodes: nodes.slice(offset, offset + limit),
      offset,
      total: elements.length,
      hasMore: offset + limit < nodes.length,
      controlsTruncated: elements.length > nodes.length
    }
  }

  async function interact(args) {
    const entry = controls.get(args.nodeId)
    const element = entry?.element
    if (
      args.snapshotId !== snapshotId ||
      !element ||
      !visible(element) ||
      privateControl(element) ||
      controlName(element) !== entry.node.name
    ) {
      throw new Error("界面控件已变化，请重新读取界面后操作")
    }
    if (
      element.matches(":disabled") ||
      element.getAttribute("aria-disabled") === "true"
    ) {
      throw new Error("目标控件当前不可用")
    }
    if (!["focus", "fill", "click", "select", "scroll"].includes(args.action)) {
      throw new Error("不支持的界面操作")
    }
    element.scrollIntoView({ block: "nearest", inline: "nearest" })
    if (args.action === "click") {
      if (element.matches('input[type="file"]'))
        throw new Error("文件选择需要用户在系统对话框中完成")
      element.click()
    } else if (args.action === "focus") {
      element.focus()
      if (
        document.activeElement !== element &&
        !element.contains(document.activeElement)
      ) {
        throw new Error("目标控件无法获得焦点")
      }
    } else if (args.action === "fill" || args.action === "select") {
      if (typeof args.text !== "string" || args.text.length > 30000)
        throw new Error("请输入不超过 30000 个字符的文本")
      if (element.readOnly) throw new Error("目标输入框为只读")
      if (
        (element.isContentEditable ? element.textContent : element.value) !==
        entry.node.value
      ) {
        throw new Error("输入内容已被用户修改，请重新读取后操作")
      }
      if (args.action === "select") {
        if (element.tagName !== "SELECT")
          throw new Error("自定义下拉框请先点击展开，再读取并点击选项")
        if (
          !Array.from(element.options).some(
            (option) => option.value === args.text && !option.disabled
          )
        ) {
          throw new Error("目标选项不存在或已停用")
        }
      } else if (
        !element.isContentEditable &&
        !element.matches(
          'textarea, input:not([type="checkbox"]):not([type="radio"]):not([type="file"]):not([type="hidden"]):not([type="button"]):not([type="submit"])'
        )
      ) {
        throw new Error("目标控件不支持文本输入")
      }
      if (element.maxLength >= 0 && args.text.length > element.maxLength) {
        throw new Error(`输入内容超过控件的 ${element.maxLength} 字符限制`)
      }
      element.focus()
      if (element.isContentEditable) element.textContent = args.text
      else element.value = args.text
      element.dispatchEvent(new Event("input", { bubbles: true }))
      element.dispatchEvent(new Event("change", { bubbles: true }))
    } else {
      let container = element
      while (container && container.scrollHeight <= container.clientHeight)
        container = container.parentElement
      if (!container) throw new Error("目标区域不能继续滚动")
      container.scrollBy({
        top: args.direction === "up" ? -480 : 480,
        behavior: "instant"
      })
    }
    await nextTick()
    return { actionDispatched: true, ...inspect() }
  }

  return { inspect, interact }
}

// 主面板和工具窗口共用回执桥接；待处理快照补齐首次加载期间遗漏的事件。
export function installAssistantUiControl(app) {
  if (!isTauri()) return
  const windowLabel = getCurrentWindow().label
  if (windowLabel !== "main" && !windowLabel.startsWith("tool-window-")) return
  const controller = createAppUiController()
  let disposed = false
  let unsubscribe
  const processing = new Set()
  const handle = async (request) => {
    if (
      disposed ||
      request.windowLabel !== windowLabel ||
      processing.has(request.id)
    )
      return
    processing.add(request.id)
    const identity = { id: request.id, runId: request.runId, windowLabel }
    try {
      if (!(await assistantApi.uiClaim(identity)) || disposed) return
      let response
      try {
        const result =
          request.operation === "read"
            ? controller.inspect(request.args)
            : await controller.interact(request.args)
        response = { ...identity, result }
      } catch (error) {
        response = { ...identity, error: error.message || String(error) }
      }
      await assistantApi.uiResult(response)
    } catch (error) {
      if (!disposed) reportAppError("assistant-ui-error", error)
    } finally {
      processing.delete(request.id)
    }
  }
  app.onUnmount(() => {
    disposed = true
    unsubscribe?.()
  })
  assistantApi
    .onUiRequest(handle)
    .then(async (unlisten) => {
      if (disposed) return unlisten()
      unsubscribe = unlisten
      for (const request of await assistantApi.uiPending(windowLabel))
        await handle(request)
    })
    .catch((error) => {
      if (!disposed) reportAppError("assistant-ui-startup", error)
    })
}

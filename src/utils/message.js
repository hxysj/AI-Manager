let messageContainer = null

function ensureContainer() {
  if (messageContainer) {
    return messageContainer
  }

  messageContainer = document.createElement("div")
  messageContainer.className = "app-message"
  document.body.appendChild(messageContainer)

  return messageContainer
}

function showMessage(type, message) {
  const container = ensureContainer()
  const item = document.createElement("div")

  item.className = `app-message__item app-message__item--${type}`
  item.textContent = message
  container.appendChild(item)

  window.setTimeout(() => {
    item.classList.add("app-message__item--leaving")
    window.setTimeout(() => item.remove(), 180)
  }, 3200)
}

export const createMessage = {
  success(message) {
    showMessage("success", message)
  },
  error(message) {
    showMessage("error", message)
  },
  warning(message) {
    showMessage("warning", message)
  }
}

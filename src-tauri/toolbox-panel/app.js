import { tools } from './tools/registry.js'

const toolNav = document.querySelector('#toolNav')
const toolTitle = document.querySelector('#toolTitle')
const toolSummary = document.querySelector('#toolSummary')
const toolContent = document.querySelector('#toolContent')
const homeButton = document.querySelector('#homeButton')

let activeToolId = ''

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;')
}

function renderNav() {
  toolNav.innerHTML = tools
    .map(
      tool => `
        <button
          class="toolbox-nav__item ${tool.id === activeToolId ? 'toolbox-nav__item--active' : ''}"
          type="button"
          data-tool-id="${escapeHtml(tool.id)}"
        >
          <span class="toolbox-nav__icon">${escapeHtml(tool.shortName)}</span>
          <span class="toolbox-nav__main">
            <strong>${escapeHtml(tool.name)}</strong>
            <small>${escapeHtml(tool.summary)}</small>
          </span>
        </button>
      `
    )
    .join('')
}

function renderHome() {
  activeToolId = ''
  toolTitle.textContent = '工具导航'
  toolSummary.textContent = '选择一个工具开始处理内容。'
  renderNav()
  toolContent.innerHTML = `
    <div class="toolbox-home">
      ${tools
        .map(
          tool => `
            <button class="toolbox-card" type="button" data-tool-id="${escapeHtml(tool.id)}">
              <span class="toolbox-card__icon">${escapeHtml(tool.shortName)}</span>
              <span class="toolbox-card__main">
                <strong>${escapeHtml(tool.name)}</strong>
                <small>${escapeHtml(tool.summary)}</small>
              </span>
              <span class="toolbox-card__meta">${escapeHtml(tool.meta)}</span>
            </button>
          `
        )
        .join('')}
    </div>
  `
}

function openTool(toolId) {
  const tool = tools.find(item => item.id === toolId)

  if (!tool) {
    renderHome()
    return
  }

  activeToolId = tool.id
  toolTitle.textContent = tool.name
  toolSummary.textContent = tool.summary
  renderNav()
  tool.render(toolContent)
}

toolNav.addEventListener('click', event => {
  const button = event.target.closest('[data-tool-id]')

  if (button) {
    openTool(button.dataset.toolId)
  }
})

toolContent.addEventListener('click', event => {
  const button = event.target.closest('[data-tool-id]')

  if (button) {
    openTool(button.dataset.toolId)
  }
})

homeButton.addEventListener('click', renderHome)

renderHome()

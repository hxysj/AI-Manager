export const imageLinkExtractorTool = {
  id: 'image-link-extractor',
  name: '图片链接提取',
  shortName: 'IMG',
  summary: '从字符串内容中用正则提取图片链接并预览图片。',
  meta: '正则 / 图片',
  render: renderImageLinkExtractorTool
}

const imageUrlPattern =
  /https?:\/\/[^\s"'<>()[\]{}]+?\.(?:png|jpe?g|gif|webp|bmp|svg|avif)(?:\?[^\s"'<>()[\]{}]*)?(?:#[^\s"'<>()[\]{}]*)?/giu

const sampleText = `文章封面：https://images.pexels.com/photos/1181671/pexels-photo-1181671.jpeg?auto=compress&cs=tinysrgb&w=900
Markdown 图片：![工作台](https://dummyimage.com/960x540/16685f/ffffff.png&text=Utility+Toolbox)
HTML 图片：<img src="https://www.gstatic.com/webp/gallery/1.webp">
普通链接：https://example.com/page 不会被识别为图片`

function renderImageLinkExtractorTool(container) {
  container.innerHTML = `
    <section class="image-tool">
      <section class="image-tool__input-panel">
        <div class="image-tool__panel-head">
          <div>
            <strong>字符串内容</strong>
            <span>粘贴包含图片地址的文本、Markdown、HTML 或 JSON。</span>
          </div>
          <div class="image-tool__actions">
            <button class="toolbox-button image-tool__sample" type="button">示例</button>
            <button class="toolbox-button image-tool__clear" type="button">清空</button>
          </div>
        </div>
        <textarea class="image-tool__textarea" spellcheck="false" placeholder="在这里输入字符串内容"></textarea>
      </section>

      <section class="image-tool__result-panel">
        <div class="image-tool__panel-head">
          <div>
            <strong>图片内容</strong>
            <span class="image-tool__summary">已提取 0 个图片链接。</span>
          </div>
          <div class="image-tool__export-actions">
            <button class="toolbox-button image-tool__select-all" type="button" disabled>全选</button>
            <select class="image-tool__export-format" aria-label="导出格式">
              <option value="pdf">PDF</option>
              <option value="zip">ZIP</option>
            </select>
            <button class="toolbox-button toolbox-button--primary image-tool__export" type="button" disabled>
              导出所选
            </button>
          </div>
        </div>
        <div class="image-tool__notice" role="status" hidden></div>
        <div class="image-tool__result"></div>
      </section>

      <div class="image-preview" hidden>
        <button class="image-preview__backdrop" type="button" aria-label="关闭预览"></button>
        <section class="image-preview__dialog" role="dialog" aria-modal="true">
          <header class="image-preview__head">
            <strong>图片预览</strong>
            <button class="image-preview__close" type="button">关闭</button>
          </header>
          <div class="image-preview__body">
            <img class="image-preview__image" alt="图片预览">
          </div>
          <a class="image-preview__link" target="_blank" rel="noreferrer"></a>
        </section>
      </div>
    </section>
  `

  const textarea = container.querySelector('.image-tool__textarea')
  const result = container.querySelector('.image-tool__result')
  const summary = container.querySelector('.image-tool__summary')
  const selectAllButton = container.querySelector('.image-tool__select-all')
  const exportFormat = container.querySelector('.image-tool__export-format')
  const exportButton = container.querySelector('.image-tool__export')
  const notice = container.querySelector('.image-tool__notice')
  const preview = container.querySelector('.image-preview')
  const previewImage = container.querySelector('.image-preview__image')
  const previewLink = container.querySelector('.image-preview__link')
  const selectedLinks = new Set()
  let links = []
  let exporting = false

  function updateSelectionState() {
    const selectedCount = selectedLinks.size
    const allSelected = Boolean(links.length) && selectedCount === links.length

    summary.textContent = `已提取 ${links.length} 个图片链接，已选择 ${selectedCount} 个。`
    selectAllButton.disabled = !links.length || exporting
    selectAllButton.textContent = allSelected ? '取消全选' : '全选'
    exportFormat.disabled = !links.length || exporting
    exportButton.disabled = !selectedCount || exporting
    exportButton.textContent = exporting ? '导出中...' : '导出所选'
  }

  function showNotice(message, type = '') {
    notice.textContent = message
    notice.className = `image-tool__notice${type ? ` image-tool__notice--${type}` : ''}`
    notice.hidden = !message
  }

  function renderImages() {
    links = extractImageLinks(textarea.value)

    // 输入变化时仅保留仍然存在的图片选择。
    for (const selectedLink of selectedLinks) {
      if (!links.includes(selectedLink)) {
        selectedLinks.delete(selectedLink)
      }
    }
    showNotice('')
    updateSelectionState()

    if (!textarea.value.trim()) {
      result.innerHTML = '<div class="image-tool__empty">输入字符串后会自动提取图片链接。</div>'
      return
    }

    if (!links.length) {
      result.innerHTML = '<div class="image-tool__empty">没有匹配到图片链接。</div>'
      return
    }

    result.innerHTML = links
      .map(
        (link, index) => `
          <article class="image-card${selectedLinks.has(link) ? ' image-card--selected' : ''}">
            <label class="image-card__select" title="选择图片">
              <input type="checkbox" data-image-index="${index}" ${selectedLinks.has(link) ? 'checked' : ''}>
              <span>选择图片 ${index + 1}</span>
            </label>
            <button class="image-card__preview" type="button" data-image-url="${escapeHtml(link)}">
              <img src="${escapeHtml(link)}" alt="提取到的图片 ${index + 1}" loading="lazy">
            </button>
            <div class="image-card__meta">
              <strong>#${index + 1}</strong>
              <a href="${escapeHtml(link)}" target="_blank" rel="noreferrer">${escapeHtml(link)}</a>
            </div>
          </article>
        `
      )
      .join('')
  }

  function openPreview(url) {
    previewImage.src = url
    previewLink.href = url
    previewLink.textContent = url
    preview.hidden = false
  }

  function closePreview() {
    preview.hidden = true
    previewImage.removeAttribute('src')
    previewLink.removeAttribute('href')
    previewLink.textContent = ''
  }

  async function exportSelectedImages() {
    const selectedUrls = links.filter(link => selectedLinks.has(link))

    if (!selectedUrls.length || exporting) {
      return
    }

    exporting = true
    showNotice(`正在生成 ${exportFormat.value.toUpperCase()} 文件...`)
    updateSelectionState()

    try {
      // 由本地服务下载远程图片，避免浏览器跨域限制。
      const response = await fetch('/api/images/export', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          format: exportFormat.value,
          urls: selectedUrls
        })
      })

      if (!response.ok) {
        const error = await response.json().catch(() => null)
        throw new Error(error?.message || `导出失败（${response.status}）`)
      }

      const blob = await response.blob()
      const downloadUrl = URL.createObjectURL(blob)
      const link = document.createElement('a')
      const timestamp = new Date().toISOString().replace(/[-:T]/g, '').slice(0, 14)

      link.href = downloadUrl
      link.download = `images-${timestamp}.${exportFormat.value}`
      document.body.appendChild(link)
      link.click()
      link.remove()
      window.setTimeout(() => URL.revokeObjectURL(downloadUrl), 1000)
      showNotice(`已导出 ${selectedUrls.length} 张图片。`, 'success')
    } catch (error) {
      showNotice(error?.message || String(error), 'error')
    } finally {
      exporting = false
      updateSelectionState()
    }
  }

  textarea.addEventListener('input', renderImages)
  container.querySelector('.image-tool__sample').addEventListener('click', () => {
    textarea.value = sampleText
    renderImages()
  })
  container.querySelector('.image-tool__clear').addEventListener('click', () => {
    textarea.value = ''
    selectedLinks.clear()
    renderImages()
    textarea.focus()
  })
  selectAllButton.addEventListener('click', () => {
    if (selectedLinks.size === links.length) {
      selectedLinks.clear()
    } else {
      links.forEach(link => selectedLinks.add(link))
    }

    renderImages()
  })
  exportButton.addEventListener('click', exportSelectedImages)
  result.addEventListener('change', event => {
    const input = event.target.closest('[data-image-index]')

    if (!input) {
      return
    }

    const link = links[Number(input.dataset.imageIndex)]

    if (input.checked) {
      selectedLinks.add(link)
    } else {
      selectedLinks.delete(link)
    }

    input.closest('.image-card')?.classList.toggle('image-card--selected', input.checked)
    showNotice('')
    updateSelectionState()
  })
  result.addEventListener('click', event => {
    const button = event.target.closest('[data-image-url]')

    if (button) {
      openPreview(button.dataset.imageUrl)
    }
  })
  container.querySelector('.image-preview__backdrop').addEventListener('click', closePreview)
  container.querySelector('.image-preview__close').addEventListener('click', closePreview)

  renderImages()
}

function extractImageLinks(text) {
  const links = []
  const seen = new Set()

  for (const match of text.matchAll(imageUrlPattern)) {
    const link = match[0]

    if (!seen.has(link)) {
      seen.add(link)
      links.push(link)
    }
  }

  return links
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;')
}

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
        </div>
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
  const preview = container.querySelector('.image-preview')
  const previewImage = container.querySelector('.image-preview__image')
  const previewLink = container.querySelector('.image-preview__link')

  function renderImages() {
    const links = extractImageLinks(textarea.value)
    summary.textContent = `已提取 ${links.length} 个图片链接。`

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
          <article class="image-card">
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

  textarea.addEventListener('input', renderImages)
  container.querySelector('.image-tool__sample').addEventListener('click', () => {
    textarea.value = sampleText
    renderImages()
  })
  container.querySelector('.image-tool__clear').addEventListener('click', () => {
    textarea.value = ''
    renderImages()
    textarea.focus()
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

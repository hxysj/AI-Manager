// 在页面和后台工具中使用相同的 URL 匹配与去重逻辑。
const imageUrlPattern =
  /https?:\/\/[^\s"'<>()[\]{}]+?\.(?:png|jpe?g|gif|webp|bmp|svg|avif)(?:\?[^\s"'<>()[\]{}]*)?(?:#[^\s"'<>()[\]{}]*)?/giu

export function extractImageLinks(text) {
  const found = []
  const seen = new Set()

  for (const match of text.matchAll(imageUrlPattern)) {
    if (!seen.has(match[0])) {
      seen.add(match[0])
      found.push(match[0])
    }
  }
  return found
}

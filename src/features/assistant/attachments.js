// 图片直接作为多模态内容发送，保持剪贴板文本的原有粘贴行为。
export async function readAssistantImages(files, existing = []) {
  const images = Array.from(files).filter((file) =>
    file.type.startsWith("image/")
  )
  if (!images.length) return existing
  if (existing.length + images.length > 4)
    throw new Error("每条消息最多添加 4 张图片")
  if (
    images.some(
      (file) =>
        !["image/png", "image/jpeg", "image/webp", "image/gif"].includes(
          file.type
        )
    )
  )
    throw new Error("支持 PNG、JPEG、WebP 和 GIF 图片")
  if (
    images.some((file) => file.size > 5 * 1024 * 1024) ||
    images.reduce(
      (size, file) => size + file.size,
      existing.reduce((size, image) => size + image.size, 0)
    ) >
      12 * 1024 * 1024
  )
    throw new Error("单张图片最多 5 MB，每条消息图片合计最多 12 MB")
  const loaded = await Promise.all(
    images.map(
      (file) =>
        new Promise((resolve, reject) => {
          const reader = new FileReader()
          reader.onload = () =>
            resolve({
              id: crypto.randomUUID(),
              name: file.name || "粘贴的图片",
              size: file.size,
              mimeType: file.type,
              dataUrl: reader.result
            })
          reader.onerror = () => reject(new Error("图片读取失败，请重新粘贴"))
          reader.readAsDataURL(file)
        })
    )
  )
  return [...existing, ...loaded]
}

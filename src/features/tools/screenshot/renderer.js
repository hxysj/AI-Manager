// 所有坐标使用原图像素；预览、复制、保存和贴图共用同一渲染器。
export function selectionRect(start, end, width, height) {
  const left = Math.max(
    0,
    Math.min(width - 1, Math.floor(Math.min(start.x, end.x)))
  )
  const top = Math.max(
    0,
    Math.min(height - 1, Math.floor(Math.min(start.y, end.y)))
  )
  const right = Math.max(
    left + 1,
    Math.min(width, Math.ceil(Math.max(start.x, end.x)))
  )
  const bottom = Math.max(
    top + 1,
    Math.min(height, Math.ceil(Math.max(start.y, end.y)))
  )
  return { x: left, y: top, width: right - left, height: bottom - top }
}

export function loadImage(source) {
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = () => reject(new Error("图片加载失败"))
    image.src = source
  })
}

export function renderDocument(canvas, image, annotations) {
  if (canvas.width !== image.naturalWidth) canvas.width = image.naturalWidth
  if (canvas.height !== image.naturalHeight) canvas.height = image.naturalHeight
  const ctx = canvas.getContext("2d")
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.drawImage(image, 0, 0)
  for (const item of annotations) {
    const [start, end] = [item.points[0], item.points.at(-1)]
    ctx.save()
    ctx.strokeStyle = item.color
    ctx.fillStyle = item.color
    ctx.lineWidth = item.width
    ctx.lineCap = "round"
    ctx.lineJoin = "round"
    const x = Math.min(start.x, end.x)
    const y = Math.min(start.y, end.y)
    const w = Math.abs(end.x - start.x)
    const h = Math.abs(end.y - start.y)
    if (item.type === "mosaic" && w >= 1 && h >= 1) {
      const rect = selectionRect(start, end, canvas.width, canvas.height)
      const tile = document.createElement("canvas")
      const block = Math.max(4, item.width * 3)
      tile.width = Math.max(1, Math.ceil(rect.width / block))
      tile.height = Math.max(1, Math.ceil(rect.height / block))
      tile
        .getContext("2d")
        .drawImage(
          canvas,
          rect.x,
          rect.y,
          rect.width,
          rect.height,
          0,
          0,
          tile.width,
          tile.height
        )
      ctx.imageSmoothingEnabled = false
      ctx.drawImage(
        tile,
        0,
        0,
        tile.width,
        tile.height,
        rect.x,
        rect.y,
        rect.width,
        rect.height
      )
    } else if (item.type === "text") {
      const size = Math.max(14, item.width * 7)
      ctx.font = `${size}px "Microsoft YaHei", sans-serif`
      ctx.textBaseline = "top"
      item.text
        .split("\n")
        .forEach((line, index) =>
          ctx.fillText(line, start.x, start.y + index * size * 1.35)
        )
    } else {
      ctx.beginPath()
      if (item.type === "rectangle") ctx.rect(x, y, w, h)
      if (item.type === "ellipse")
        ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2)
      if (item.type === "pen" || item.type === "arrow") {
        ctx.moveTo(start.x, start.y)
        for (const point of item.points.slice(1)) ctx.lineTo(point.x, point.y)
      }
      ctx.stroke()
      if (
        item.type === "arrow" &&
        Math.hypot(end.x - start.x, end.y - start.y) > 1
      ) {
        const angle = Math.atan2(end.y - start.y, end.x - start.x)
        const size = Math.max(10, item.width * 4)
        ctx.beginPath()
        ctx.moveTo(end.x, end.y)
        ctx.lineTo(
          end.x - size * Math.cos(angle - Math.PI / 6),
          end.y - size * Math.sin(angle - Math.PI / 6)
        )
        ctx.lineTo(
          end.x - size * Math.cos(angle + Math.PI / 6),
          end.y - size * Math.sin(angle + Math.PI / 6)
        )
        ctx.closePath()
        ctx.fill()
      }
    }
    ctx.restore()
  }
}

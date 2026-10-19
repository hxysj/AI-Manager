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

// 浏览器异步编码，避免大选区同步生成 PNG 时卡住整个截图页面。
export async function canvasSource(canvas) {
  const blob = await new Promise((resolve, reject) => {
    canvas.toBlob(
      (value) => (value ? resolve(value) : reject(new Error("截图编码失败"))),
      "image/png"
    )
  })
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result)
    reader.onerror = () => reject(reader.error || new Error("截图读取失败"))
    reader.readAsDataURL(blob)
  })
}

export async function thumbnailSource(image) {
  const scale = Math.min(1, 320 / image.width, 200 / image.height)
  const canvas = document.createElement("canvas")
  canvas.width = Math.max(1, Math.round(image.width * scale))
  canvas.height = Math.max(1, Math.round(image.height * scale))
  const context = canvas.getContext("2d")
  context.imageSmoothingQuality = "high"
  context.drawImage(image, 0, 0, canvas.width, canvas.height)
  try {
    return await canvasSource(canvas)
  } finally {
    canvas.width = 0
    canvas.height = 0
  }
}

export function renderDocument(canvas, image, annotations) {
  if (canvas.width !== image.naturalWidth) canvas.width = image.naturalWidth
  if (canvas.height !== image.naturalHeight) canvas.height = image.naturalHeight
  const ctx = canvas.getContext("2d")
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.drawImage(image, 0, 0)
  if (!annotations.length) {
    // 普通截图不分配整图大小的透明层，撤销完标注也释放该缓冲区。
    if (canvas.annotationLayer) {
      canvas.annotationLayer.width = 0
      canvas.annotationLayer.height = 0
      canvas.annotationLayer = null
    }
    return
  }
  // 复用透明层，避免鼠标移动预览时反复分配整张大画布。
  const layer =
    canvas.annotationLayer ||
    (canvas.annotationLayer = document.createElement("canvas"))
  if (layer.width !== canvas.width) layer.width = canvas.width
  if (layer.height !== canvas.height) layer.height = canvas.height
  const layerCtx = layer.getContext("2d")
  layerCtx.clearRect(0, 0, layer.width, layer.height)
  for (const item of annotations) {
    const [start, end] = [item.points[0], item.points.at(-1)]
    if (!start) continue
    if (item.type === "eraser") {
      // 橡皮擦只作用于透明标注层，擦除后会露出原图。
      layerCtx.save()
      layerCtx.globalCompositeOperation = "destination-out"
      layerCtx.strokeStyle = "#000"
      layerCtx.lineWidth = Math.max(8, item.width * 3)
      layerCtx.lineCap = "round"
      layerCtx.lineJoin = "round"
      layerCtx.beginPath()
      layerCtx.moveTo(start.x, start.y)
      for (const point of item.points.slice(1))
        layerCtx.lineTo(point.x, point.y)
      layerCtx.stroke()
      layerCtx.restore()
      continue
    }
    if (item.type === "mosaic") {
      const w = Math.abs(end.x - start.x)
      const h = Math.abs(end.y - start.y)
      if (w < 1 || h < 1) continue
      const rect = selectionRect(start, end, canvas.width, canvas.height)
      const sample = document.createElement("canvas")
      sample.width = rect.width
      sample.height = rect.height
      const sampleCtx = sample.getContext("2d")
      sampleCtx.drawImage(
        image,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        0,
        0,
        rect.width,
        rect.height
      )
      sampleCtx.drawImage(
        layer,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        0,
        0,
        rect.width,
        rect.height
      )
      const tile = document.createElement("canvas")
      const block = Math.max(4, item.width * 3)
      tile.width = Math.max(1, Math.ceil(rect.width / block))
      tile.height = Math.max(1, Math.ceil(rect.height / block))
      tile
        .getContext("2d")
        .drawImage(
          sample,
          0,
          0,
          rect.width,
          rect.height,
          0,
          0,
          tile.width,
          tile.height
        )
      layerCtx.save()
      layerCtx.imageSmoothingEnabled = false
      layerCtx.drawImage(
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
      layerCtx.restore()
      continue
    }
    layerCtx.save()
    layerCtx.strokeStyle = item.color
    layerCtx.fillStyle = item.color
    layerCtx.lineWidth = item.width
    layerCtx.lineCap = "round"
    layerCtx.lineJoin = "round"
    layerCtx.setLineDash(
      item.style === "dashed" ? [item.width * 3, item.width * 2] : []
    )
    const x = Math.min(start.x, end.x)
    const y = Math.min(start.y, end.y)
    const w = Math.abs(end.x - start.x)
    const h = Math.abs(end.y - start.y)
    if (item.type === "text") {
      const size = Math.max(14, item.width * 7)
      layerCtx.font = `${size}px "Microsoft YaHei", sans-serif`
      layerCtx.textBaseline = "top"
      item.text
        .split("\n")
        .forEach((line, index) =>
          layerCtx.fillText(line, start.x, start.y + index * size * 1.35)
        )
    } else {
      layerCtx.beginPath()
      if (item.type === "rectangle") layerCtx.rect(x, y, w, h)
      if (item.type === "ellipse")
        layerCtx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2)
      if (item.type === "highlighter") {
        layerCtx.globalAlpha = 0.3
        layerCtx.lineWidth = item.width * 4
      }
      if (["pen", "arrow", "line", "highlighter"].includes(item.type)) {
        layerCtx.moveTo(start.x, start.y)
        for (const point of item.points.slice(1))
          layerCtx.lineTo(point.x, point.y)
      }
      layerCtx.stroke()
      if (
        item.type === "arrow" &&
        Math.hypot(end.x - start.x, end.y - start.y) > 1
      ) {
        const angle = Math.atan2(end.y - start.y, end.x - start.x)
        const size = Math.max(10, item.width * 4)
        layerCtx.beginPath()
        layerCtx.moveTo(end.x, end.y)
        layerCtx.lineTo(
          end.x - size * Math.cos(angle - Math.PI / 6),
          end.y - size * Math.sin(angle - Math.PI / 6)
        )
        layerCtx.lineTo(
          end.x - size * Math.cos(angle + Math.PI / 6),
          end.y - size * Math.sin(angle + Math.PI / 6)
        )
        layerCtx.closePath()
        layerCtx.fill()
      }
    }
    layerCtx.restore()
  }
  ctx.drawImage(layer, 0, 0)
}

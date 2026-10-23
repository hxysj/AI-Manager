import * as ortModule from "onnxruntime-web"
import dictionary from "./ocr-dictionary.json"

const ort = ortModule.default || ortModule
// 资源放在 public/models 下。使用当前 WebView 的根地址拼接，避免在
// Tauri 的 index.html?panel=screenshot 路由下被解析到错误目录。
const MODEL_BASE =
  window.location.origin && window.location.origin !== "null"
    ? `${window.location.origin}/`
    : document.baseURI
const MODEL_ROOT = new URL("/models/ocr/", MODEL_BASE).toString()
const DET_MODEL = new URL("PP-OCRv6_det_small.onnx", MODEL_ROOT).toString()
const REC_MODEL = new URL("PP-OCRv6_rec_small.onnx", MODEL_ROOT).toString()
const DET_SHORT_SIDE = 736
const DET_MAX_SIDE = 1536
const REC_HEIGHT = 48
const REC_MAX_WIDTH = 2000
const sessions = { det: null, rec: null }
let loading = null

function errorMessage(error) {
  if (!error) return "未知错误"
  const cause = error.cause ? `；原因：${errorMessage(error.cause)}` : ""
  return `${error.message || String(error)}${cause}`
}

async function loadModel(url, label) {
  const response = await fetch(url, { credentials: "same-origin" })
  if (!response.ok)
    throw new Error(`${label}文件读取失败（HTTP ${response.status}）：${url}`)
  const bytes = new Uint8Array(await response.arrayBuffer())
  if (bytes.byteLength < 1024)
    throw new Error(`${label}文件内容为空：${url}`)
  return bytes
}

function align32(value) {
  return Math.max(32, Math.round(value / 32) * 32)
}

function imageFromSource(source) {
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = () => reject(new Error("OCR 无法读取截图原图"))
    image.src = source
  })
}

function canvasPixels(image, width, height) {
  const canvas = document.createElement("canvas")
  canvas.width = width
  canvas.height = height
  const context = canvas.getContext("2d", { willReadFrequently: true })
  context.imageSmoothingEnabled = true
  context.drawImage(image, 0, 0, width, height)
  return { canvas, pixels: context.getImageData(0, 0, width, height).data }
}

function rgbTensor(pixels, width, height) {
  const result = new Float32Array(width * height * 3)
  const plane = width * height
  for (let index = 0; index < plane; index += 1) {
    const offset = index * 4
    // PP-OCR 的输入是 BGR、按通道连续排列，范围为 [-1, 1]。
    result[index] = pixels[offset + 2] / 127.5 - 1
    result[plane + index] = pixels[offset + 1] / 127.5 - 1
    result[plane * 2 + index] = pixels[offset] / 127.5 - 1
  }
  return result
}

async function getSessions() {
  if (sessions.det && sessions.rec) return sessions
  if (!loading) {
    // WebView 使用 wasm 后端，不依赖旧项目的原生 OCR 进程。
    ort.env.wasm.numThreads = 1
    ort.env.wasm.proxy = false
    // 1.30 使用独立的 Emscripten 模块；指定模块文件后，Tauri 的自定义
    // 协议不会再尝试从 onnxruntime-web 的 node_modules 路径取 WASM。
    ort.env.wasm.wasmPaths = {
      mjs: new URL(
        "ort-wasm-simd-threaded.jsep.mjs",
        MODEL_ROOT
      ).toString()
    }
    loading = Promise.all([
      loadModel(DET_MODEL, "检测模型"),
      loadModel(REC_MODEL, "识别模型")
    ])
      .then(([detModel, recModel]) =>
        Promise.all([
          ort.InferenceSession.create(detModel, { executionProviders: ["wasm"] }),
          ort.InferenceSession.create(recModel, { executionProviders: ["wasm"] })
        ])
      )
      .then(([det, rec]) => {
      sessions.det = det
      sessions.rec = rec
      return sessions
      })
      .catch((error) => {
      loading = null
      sessions.det = null
      sessions.rec = null
      throw new Error(`OCR 模型加载失败：${errorMessage(error)}`)
      })
  }
  return loading
}

function outputData(result) {
  const names = Object.keys(result)
  if (!names.length) throw new Error("OCR 模型没有返回推理结果")
  return result[names[0]]
}

function outputShape(tensor) {
  return tensor.dims.map((value) => Number(value))
}

function connectedBoxes(
  tensor,
  resizedWidth,
  resizedHeight,
  sourceWidth,
  sourceHeight
) {
  const dims = outputShape(tensor)
  const outHeight = dims.length === 4 ? dims[2] : dims.at(-2)
  const outWidth = dims.length === 4 ? dims[3] : dims.at(-1)
  const data = tensor.data
  const mask = new Uint8Array(outWidth * outHeight)
  for (let y = 0; y < outHeight; y += 1) {
    for (let x = 0; x < outWidth; x += 1) {
      const value = Number(data[y * outWidth + x])
      mask[y * outWidth + x] = value > 0.3 ? 1 : 0
    }
  }
  const visited = new Uint8Array(mask.length)
  const boxes = []
  const queue = new Int32Array(mask.length)
  for (let start = 0; start < mask.length; start += 1) {
    if (!mask[start] || visited[start]) continue
    let head = 0
    let tail = 0
    queue[tail++] = start
    visited[start] = 1
    let minX = start % outWidth
    let maxX = minX
    let minY = Math.floor(start / outWidth)
    let maxY = minY
    let count = 0
    while (head < tail) {
      const current = queue[head++]
      const x = current % outWidth
      const y = Math.floor(current / outWidth)
      count += 1
      minX = Math.min(minX, x)
      maxX = Math.max(maxX, x)
      minY = Math.min(minY, y)
      maxY = Math.max(maxY, y)
      for (const [dx, dy] of [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1]
      ]) {
        const nx = x + dx
        const ny = y + dy
        if (nx < 0 || ny < 0 || nx >= outWidth || ny >= outHeight) continue
        const next = ny * outWidth + nx
        if (mask[next] && !visited[next]) {
          visited[next] = 1
          queue[tail++] = next
        }
      }
    }
    if (count < 4) continue
    let scoreTotal = 0
    let scoreCount = 0
    for (let y = minY; y <= maxY; y += 1) {
      for (let x = minX; x <= maxX; x += 1) {
        scoreTotal += Number(data[y * outWidth + x])
        scoreCount += 1
      }
    }
    if (scoreTotal / Math.max(1, scoreCount) < 0.5) continue
    const sx = sourceWidth / resizedWidth
    const sy = sourceHeight / resizedHeight
    const mappedX = (minX / outWidth) * resizedWidth * sx
    const mappedY = (minY / outHeight) * resizedHeight * sy
    const mappedRight = ((maxX + 1) / outWidth) * resizedWidth * sx
    const mappedBottom = ((maxY + 1) / outHeight) * resizedHeight * sy
    // DBNet 输出的是收缩后的文字区域，按 unclip 比例扩回笔画边缘。
    const paddingX = Math.max(2, (mappedRight - mappedX) * 0.3)
    const paddingY = Math.max(2, (mappedBottom - mappedY) * 0.3)
    const x = Math.max(0, Math.floor(mappedX - paddingX))
    const y = Math.max(0, Math.floor(mappedY - paddingY))
    const right = Math.min(sourceWidth, Math.ceil(mappedRight + paddingX))
    const bottom = Math.min(sourceHeight, Math.ceil(mappedBottom + paddingY))
    if (right - x >= 6 && bottom - y >= 6)
      boxes.push({ x, y, width: right - x, height: bottom - y })
  }
  // 检测模型可能把同一行拆成相邻连通块，把垂直重叠且间距很小的框合并。
  boxes.sort((a, b) => a.y - b.y || a.x - b.x)
  const merged = []
  for (const box of boxes) {
    const previous = merged.at(-1)
    if (
      previous &&
      Math.abs(previous.y - box.y) <
        Math.max(previous.height, box.height) * 0.45 &&
      box.x <= previous.x + previous.width + 8
    ) {
      const right = Math.max(previous.x + previous.width, box.x + box.width)
      const bottom = Math.max(previous.y + previous.height, box.y + box.height)
      previous.x = Math.min(previous.x, box.x)
      previous.y = Math.min(previous.y, box.y)
      previous.width = right - previous.x
      previous.height = bottom - previous.y
    } else merged.push({ ...box })
  }
  return merged.slice(0, 256)
}

function cropCanvas(image, box, width) {
  const source = document.createElement("canvas")
  source.width = width
  source.height = REC_HEIGHT
  const context = source.getContext("2d", { willReadFrequently: true })
  context.fillStyle = "#ffffff"
  context.fillRect(0, 0, width, REC_HEIGHT)
  const scale = REC_HEIGHT / box.height
  const targetWidth = Math.min(
    width,
    Math.max(1, Math.round(box.width * scale))
  )
  context.drawImage(
    image,
    box.x,
    box.y,
    box.width,
    box.height,
    0,
    0,
    targetWidth,
    REC_HEIGHT
  )
  return context.getImageData(0, 0, width, REC_HEIGHT).data
}

function recTensor(crops, width) {
  const plane = width * REC_HEIGHT
  const data = new Float32Array(crops.length * plane * 3)
  crops.forEach((pixels, cropIndex) => {
    const offset = cropIndex * plane * 3
    for (let index = 0; index < plane; index += 1) {
      const pixel = index * 4
      data[offset + index] = pixels[pixel + 2] / 127.5 - 1
      data[offset + plane + index] = pixels[pixel + 1] / 127.5 - 1
      data[offset + plane * 2 + index] = pixels[pixel] / 127.5 - 1
    }
  })
  return data
}

function decodeRecognition(tensor, lineIndex, batchSize) {
  const dims = outputShape(tensor)
  const time =
    dims.length === 3
      ? dims[1]
      : Math.floor(tensor.data.length / batchSize / 18710)
  const classes = dims.at(-1)
  const stride = time * classes
  let text = ""
  let score = 0
  let previous = -1
  for (let step = 0; step < time; step += 1) {
    let best = 0
    let value = -Infinity
    for (let index = 0; index < classes; index += 1) {
      const candidate = Number(
        tensor.data[lineIndex * stride + step * classes + index]
      )
      if (candidate > value) {
        value = candidate
        best = index
      }
    }
    if (best > 0 && best !== previous) {
      if (best - 1 < dictionary.length) text += dictionary[best - 1]
      else if (best - 1 === dictionary.length) text += " "
    }
    if (best > 0) score += Math.max(0, Math.min(1, value))
    previous = best
  }
  return { text: text.trim(), score: score / Math.max(1, time) }
}

export async function recognizePpocr(source) {
  const image = await imageFromSource(source)
  const sourceWidth = image.naturalWidth || image.width
  const sourceHeight = image.naturalHeight || image.height
  if (!sourceWidth || !sourceHeight) throw new Error("OCR 输入图片为空")
  // 与 PP-OCR 的检测预处理一致：短边补到 736，同时限制最长边，避免大图推理过慢。
  let ratio = sourceWidth > 0 && sourceHeight > 0
    ? Math.min(1, DET_SHORT_SIDE / Math.min(sourceWidth, sourceHeight))
    : 1
  if (Math.max(sourceWidth, sourceHeight) * ratio > DET_MAX_SIDE)
    ratio = DET_MAX_SIDE / Math.max(sourceWidth, sourceHeight)
  const resizedWidth = align32(Math.max(32, Math.round(sourceWidth * ratio)))
  const resizedHeight = align32(Math.max(32, Math.round(sourceHeight * ratio)))
  const resized = canvasPixels(image, resizedWidth, resizedHeight)
  const { det, rec } = await getSessions()
  const detResult = await det.run({
    x: new ort.Tensor(
      "float32",
      rgbTensor(resized.pixels, resizedWidth, resizedHeight),
      [1, 3, resizedHeight, resizedWidth]
    )
  })
  const boxes = connectedBoxes(
    outputData(detResult),
    resizedWidth,
    resizedHeight,
    sourceWidth,
    sourceHeight
  )
  if (!boxes.length) return { text: "", lines: [], engine: "本地文字识别" }
  const lines = []
  for (const box of boxes) {
    const width = Math.min(
      REC_MAX_WIDTH,
      Math.max(16, Math.round((REC_HEIGHT * box.width) / box.height))
    )
    const crop = cropCanvas(image, box, width)
    const recResult = await rec.run({
      x: new ort.Tensor("float32", recTensor([crop], width), [
        1,
        3,
        REC_HEIGHT,
        width
      ])
    })
    const line = { ...box, ...decodeRecognition(outputData(recResult), 0, 1) }
    if (line.text) lines.push(line)
  }
  return {
    text: lines.map((line) => line.text).join("\n"),
    lines,
    engine: "本地文字识别"
  }
}

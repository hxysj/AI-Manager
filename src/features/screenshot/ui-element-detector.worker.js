import * as ortModule from "onnxruntime-web"

const ort = ortModule.default || ortModule
const INPUT_SIZE = 640
const CONFIDENCE_THRESHOLD = 0.3
const NMS_THRESHOLD = 0.45
const MAX_REGIONS = 96
const MODEL_BASE =
  self.location.origin && self.location.origin !== "null"
    ? `${self.location.origin}/`
    : self.location.href
const MODEL_URL = new URL(
  "/models/screenshot/carwin-element-detection.onnx",
  MODEL_BASE
).toString()
const RUNTIME_ROOT = new URL("/models/ocr/", MODEL_BASE).toString()

let session = null
let loading = null
let queue = Promise.resolve()

function errorMessage(error) {
  if (!error) return "未知错误"
  const cause = error.cause ? `；原因：${errorMessage(error.cause)}` : ""
  return `${error.message || String(error)}${cause}`
}

async function loadModel() {
  const response = await fetch(MODEL_URL, { credentials: "same-origin" })
  if (!response.ok)
    throw new Error(`界面元素检测模型读取失败（HTTP ${response.status}）`)
  const bytes = new Uint8Array(await response.arrayBuffer())
  if (bytes.byteLength < 1024) throw new Error("界面元素检测模型内容为空")
  return bytes
}

async function getSession() {
  if (session) return session
  if (!loading) {
    // 推理在线程中运行，WASM 保持单线程，避免截图窗口和模型争用 CPU。
    ort.env.wasm.numThreads = 1
    ort.env.wasm.proxy = false
    ort.env.wasm.wasmPaths = {
      mjs: new URL("ort-wasm-simd-threaded.jsep.mjs", RUNTIME_ROOT).toString()
    }
    loading = loadModel()
      .then((model) =>
        ort.InferenceSession.create(model, { executionProviders: ["wasm"] })
      )
      .then((value) => {
        session = value
        return value
      })
      .catch((error) => {
        loading = null
        session = null
        throw error
      })
  }
  return loading
}

function rgbTensor(pixels) {
  const plane = INPUT_SIZE * INPUT_SIZE
  const data = new Float32Array(plane * 3)
  for (let index = 0; index < plane; index += 1) {
    const pixel = index * 4
    data[index] = pixels[pixel] / 255
    data[plane + index] = pixels[pixel + 1] / 255
    data[plane * 2 + index] = pixels[pixel + 2] / 255
  }
  return new ort.Tensor("float32", data, [1, 3, INPUT_SIZE, INPUT_SIZE])
}

function intersectionOverUnion(left, right) {
  const width = Math.max(
    0,
    Math.min(left.right, right.right) - Math.max(left.left, right.left)
  )
  const height = Math.max(
    0,
    Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top)
  )
  const intersection = width * height
  const union =
    (left.right - left.left) * (left.bottom - left.top) +
    (right.right - right.left) * (right.bottom - right.top) -
    intersection
  return intersection / Math.max(1, union)
}

function pixelDifference(pixels, firstX, firstY, secondX, secondY) {
  const left =
    (Math.max(0, Math.min(INPUT_SIZE - 1, firstY)) * INPUT_SIZE +
      Math.max(0, Math.min(INPUT_SIZE - 1, firstX))) *
    4
  const right =
    (Math.max(0, Math.min(INPUT_SIZE - 1, secondY)) * INPUT_SIZE +
      Math.max(0, Math.min(INPUT_SIZE - 1, secondX))) *
    4
  return Math.max(
    Math.abs(pixels[left] - pixels[right]),
    Math.abs(pixels[left + 1] - pixels[right + 1]),
    Math.abs(pixels[left + 2] - pixels[right + 2])
  )
}

function edgeContrast(pixels, x, y, horizontal) {
  let strongest = 0
  for (let offset = -2; offset <= 1; offset += 1) {
    strongest = Math.max(
      strongest,
      horizontal
        ? pixelDifference(pixels, x, y + offset, x, y + offset + 1)
        : pixelDifference(pixels, x + offset, y, x + offset + 1, y)
    )
  }
  return strongest
}

function boundaryStrength(pixels, left, top, right, bottom) {
  const horizontalSamples = Math.max(
    8,
    Math.min(32, Math.round((right - left) / 12))
  )
  const verticalSamples = Math.max(
    8,
    Math.min(32, Math.round((bottom - top) / 12))
  )
  const coverage = (count, point) => {
    let matched = 0
    for (let index = 0; index < count; index += 1) {
      const ratio = count === 1 ? 0.5 : index / (count - 1)
      if (point(ratio) >= 18) matched += 1
    }
    return matched / count
  }
  const topCoverage = coverage(horizontalSamples, (ratio) =>
    edgeContrast(
      pixels,
      Math.round(left + (right - left) * (0.08 + ratio * 0.84)),
      Math.round(top),
      true
    )
  )
  const bottomCoverage = coverage(horizontalSamples, (ratio) =>
    edgeContrast(
      pixels,
      Math.round(left + (right - left) * (0.08 + ratio * 0.84)),
      Math.round(bottom),
      true
    )
  )
  const leftCoverage = coverage(verticalSamples, (ratio) =>
    edgeContrast(
      pixels,
      Math.round(left),
      Math.round(top + (bottom - top) * (0.08 + ratio * 0.84)),
      false
    )
  )
  const rightCoverage = coverage(verticalSamples, (ratio) =>
    edgeContrast(
      pixels,
      Math.round(right),
      Math.round(top + (bottom - top) * (0.08 + ratio * 0.84)),
      false
    )
  )
  return {
    average: (topCoverage + bottomCoverage + leftCoverage + rightCoverage) / 4,
    opposite: Math.max(
      Math.min(topCoverage, bottomCoverage),
      Math.min(leftCoverage, rightCoverage)
    )
  }
}

function suppressOverlaps(candidates) {
  const result = []
  for (const candidate of candidates.sort(
    (left, right) => right.score - left.score
  )) {
    if (
      result.some(
        (selected) => intersectionOverUnion(candidate, selected) > NMS_THRESHOLD
      )
    )
      continue
    result.push(candidate)
    if (result.length >= MAX_REGIONS) break
  }
  return result
}

function decodeOutputs(outputs, request) {
  const candidates = []
  const pixels = new Uint8ClampedArray(request.pixels)
  for (const name of ["output0", "343", "378"]) {
    const output = outputs[name]
    if (!output) continue
    const stride = output.dims.at(-1)
    const rows = output.data.length / stride
    for (let row = 0; row < rows; row += 1) {
      const offset = row * stride
      const score =
        Number(output.data[offset + 4]) * Number(output.data[offset + 5])
      if (score < CONFIDENCE_THRESHOLD) continue
      const centerX = Number(output.data[offset])
      const centerY = Number(output.data[offset + 1])
      const width = Number(output.data[offset + 2])
      const height = Number(output.data[offset + 3])
      const inputLeft = centerX - width / 2
      const inputTop = centerY - height / 2
      const inputRight = centerX + width / 2
      const inputBottom = centerY + height / 2
      const left = Math.max(
        0,
        Math.min(
          request.sourceWidth,
          (inputLeft - request.paddingX) / request.scaleX
        )
      )
      const top = Math.max(
        0,
        Math.min(
          request.sourceHeight,
          (inputTop - request.paddingY) / request.scaleY
        )
      )
      const right = Math.max(
        0,
        Math.min(
          request.sourceWidth,
          (inputRight - request.paddingX) / request.scaleX
        )
      )
      const bottom = Math.max(
        0,
        Math.min(
          request.sourceHeight,
          (inputBottom - request.paddingY) / request.scaleY
        )
      )
      const sourceWidth = right - left
      const sourceHeight = bottom - top
      if (sourceWidth < 8 || sourceHeight < 8) continue
      if (
        sourceWidth > request.sourceWidth * 0.98 &&
        sourceHeight > request.sourceHeight * 0.98
      )
        continue
      const textBandHeight = Math.min(
        52,
        Math.max(28, request.sourceHeight * 0.045)
      )
      if (sourceHeight <= textBandHeight && sourceWidth / sourceHeight >= 1.7) {
        // 横向矮框只有具备连续矩形边界时才保留，过滤菜单文字和代码文本。
        const boundary = boundaryStrength(
          pixels,
          inputLeft,
          inputTop,
          inputRight,
          inputBottom
        )
        const compactHeight = Math.min(
          32,
          Math.max(24, request.sourceHeight * 0.03)
        )
        const minimumAverage = sourceHeight <= compactHeight ? 0.5 : 0.3
        const minimumOpposite = sourceHeight <= compactHeight ? 0.32 : 0.18
        if (
          boundary.average < minimumAverage ||
          boundary.opposite < minimumOpposite
        )
          continue
      }
      candidates.push({ left, top, right, bottom, score })
    }
  }
  return suppressOverlaps(candidates).map((region, index) => ({
    id: `model-${index}`,
    kind: "model",
    label: "界面元素",
    x: Math.round(region.left),
    y: Math.round(region.top),
    width: Math.max(1, Math.round(region.right - region.left)),
    height: Math.max(1, Math.round(region.bottom - region.top)),
    score: region.score
  }))
}

async function detect(request) {
  const currentSession = await getSession()
  const outputs = await currentSession.run({
    images: rgbTensor(new Uint8ClampedArray(request.pixels))
  })
  return decodeOutputs(outputs, request)
}

self.addEventListener("message", ({ data }) => {
  queue = queue
    .then(async () => {
      const regions = await detect(data)
      self.postMessage({ id: data.id, regions })
    })
    .catch((error) => {
      self.postMessage({
        id: data.id,
        error: `界面元素检测失败：${errorMessage(error)}`
      })
    })
})

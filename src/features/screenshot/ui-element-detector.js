const INPUT_SIZE = 640

let worker = null
let requestSequence = 0
const requests = new Map()

function detectorWorker() {
  if (worker) return worker
  worker = new Worker(
    new URL("./ui-element-detector.worker.js", import.meta.url),
    { type: "module" }
  )
  worker.addEventListener("message", ({ data }) => {
    const request = requests.get(data.id)
    if (!request) return
    requests.delete(data.id)
    if (data.error) request.reject(new Error(data.error))
    else request.resolve(data.regions || [])
  })
  worker.addEventListener("error", (event) => {
    const error = new Error(event.message || "界面元素检测线程异常")
    for (const request of requests.values()) request.reject(error)
    requests.clear()
    worker?.terminate()
    worker = null
  })
  return worker
}

function detectorPixels(source, sourceWidth, sourceHeight) {
  const scale = Math.min(INPUT_SIZE / sourceWidth, INPUT_SIZE / sourceHeight)
  const width = Math.max(1, Math.round(sourceWidth * scale))
  const height = Math.max(1, Math.round(sourceHeight * scale))
  const paddingX = Math.floor((INPUT_SIZE - width) / 2)
  const paddingY = Math.floor((INPUT_SIZE - height) / 2)
  const canvas = document.createElement("canvas")
  canvas.width = INPUT_SIZE
  canvas.height = INPUT_SIZE
  const context = canvas.getContext("2d", { willReadFrequently: true })
  context.fillStyle = "rgb(114, 114, 114)"
  context.fillRect(0, 0, INPUT_SIZE, INPUT_SIZE)
  context.imageSmoothingEnabled = true
  context.imageSmoothingQuality = "high"
  context.drawImage(
    source,
    0,
    0,
    sourceWidth,
    sourceHeight,
    paddingX,
    paddingY,
    width,
    height
  )
  const pixels = context.getImageData(0, 0, INPUT_SIZE, INPUT_SIZE).data
  canvas.width = 0
  canvas.height = 0
  return {
    pixels,
    paddingX,
    paddingY,
    scaleX: width / sourceWidth,
    scaleY: height / sourceHeight
  }
}

function intersection(left, right) {
  const width = Math.max(
    0,
    Math.min(left.x + left.width, right.x + right.width) -
      Math.max(left.x, right.x)
  )
  const height = Math.max(
    0,
    Math.min(left.y + left.height, right.y + right.height) -
      Math.max(left.y, right.y)
  )
  return width * height
}

function duplicateRegion(left, right) {
  const leftArea = left.width * left.height
  const rightArea = right.width * right.height
  const smaller = Math.min(leftArea, rightArea)
  const larger = Math.max(leftArea, rightArea)
  return (
    smaller > 0 &&
    intersection(left, right) / smaller > 0.88 &&
    smaller / larger > 0.72
  )
}

export function mergeUiElementRegions(current, detected) {
  const result = [...current]
  for (const region of detected) {
    if (result.some((item) => duplicateRegion(item, region))) continue
    result.push(region)
  }
  return result
}

export function detectUiElementRegions(source, sourceWidth, sourceHeight) {
  if (!source || !sourceWidth || !sourceHeight) return Promise.resolve([])
  const prepared = detectorPixels(source, sourceWidth, sourceHeight)
  const id = ++requestSequence
  return new Promise((resolve, reject) => {
    requests.set(id, { resolve, reject })
    detectorWorker().postMessage(
      {
        id,
        pixels: prepared.pixels.buffer,
        sourceWidth,
        sourceHeight,
        paddingX: prepared.paddingX,
        paddingY: prepared.paddingY,
        scaleX: prepared.scaleX,
        scaleY: prepared.scaleY
      },
      [prepared.pixels.buffer]
    )
  })
}

export function disposeUiElementDetector() {
  const error = new Error("界面元素检测已结束")
  for (const request of requests.values()) request.reject(error)
  requests.clear()
  worker?.terminate()
  worker = null
}

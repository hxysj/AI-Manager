let worker = null
let requestSequence = 0
const requests = new Map()

function detectorWorker() {
  if (worker) return worker
  const current = new Worker(
    new URL('./rectangle-detector.worker.js', import.meta.url),
    { type: 'module' }
  )
  worker = current
  current.addEventListener('message', ({ data }) => {
    if (worker !== current) return
    const request = requests.get(data.id)
    if (!request) return
    requests.delete(data.id)
    if (data.error) request.reject(new Error(data.error))
    else request.resolve(data.regions)
  })
  current.addEventListener('error', event => {
    if (worker !== current) return
    const error = new Error(event.message || '矩形检测线程异常')
    for (const request of requests.values()) request.reject(error)
    requests.clear()
    current.terminate()
    worker = null
  })
  return worker
}

export function detectRectangleRegions(pixels, width, height) {
  const id = ++requestSequence
  return new Promise((resolve, reject) => {
    const current = detectorWorker()
    requests.set(id, { resolve, reject })
    try {
      // 画布已绘制好截图，直接转移原始 RGBA 缓冲区，避免再缩放或复制整屏。
      current.postMessage(
        {
          id,
          pixels: pixels.buffer,
          byteOffset: pixels.byteOffset,
          byteLength: pixels.byteLength,
          width,
          height
        },
        [pixels.buffer]
      )
    } catch (error) {
      requests.delete(id)
      reject(error)
    }
  })
}

export function disposeRectangleDetector() {
  const error = new Error('矩形检测已结束')
  for (const request of requests.values()) request.reject(error)
  requests.clear()
  worker?.terminate()
  worker = null
}

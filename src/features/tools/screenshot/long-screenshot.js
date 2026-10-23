import { canvasSource, thumbnailSource } from "./renderer"
import { screenshotApi } from "@/api/modules/screenshot"

function frameFromBytes(bytes) {
  if (bytes.byteLength < 20) throw new Error("长截图帧数据不完整")
  const header = new DataView(bytes, 0, 20)
  const width = header.getUint32(8, true)
  const height = header.getUint32(12, true)
  if (!width || !height || bytes.byteLength !== 20 + width * height * 4)
    throw new Error("长截图帧尺寸无效")
  return {
    left: header.getInt32(0, true),
    top: header.getInt32(4, true),
    width,
    height,
    pixels: new Uint8ClampedArray(bytes, 20)
  }
}

function frameDifference(previous, current) {
  if (previous.width !== current.width || previous.height !== current.height)
    return 1
  let total = 0
  let count = 0
  for (let y = 0; y < previous.height; y += 6) {
    for (let x = 0; x < previous.width; x += 8) {
      const index = (y * previous.width + x) * 4
      total += Math.abs(previous.pixels[index] - current.pixels[index])
      total += Math.abs(previous.pixels[index + 1] - current.pixels[index + 1])
      total += Math.abs(previous.pixels[index + 2] - current.pixels[index + 2])
      count += 3
    }
  }
  return total / Math.max(1, count * 255)
}

function overlapScore(previous, current, previousStart, currentStart, overlap) {
  const width = Math.min(previous.width, current.width)
  let total = 0
  let count = 0
  for (let y = 0; y < overlap; y += 4) {
    const previousY = previousStart + y
    const currentY = currentStart + y
    for (let x = 0; x < width; x += 8) {
      const a = (previousY * previous.width + x) * 4
      const b = (currentY * current.width + x) * 4
      total += Math.abs(previous.pixels[a] - current.pixels[b])
      total += Math.abs(previous.pixels[a + 1] - current.pixels[b + 1])
      total += Math.abs(previous.pixels[a + 2] - current.pixels[b + 2])
      count += 3
    }
  }
  return total / Math.max(1, count * 255)
}

function rowSignatures(frame) {
  const limit = Math.max(
    1,
    frame.width - Math.max(8, Math.round(frame.width * 0.03))
  )
  const signatures = new Uint32Array(frame.height)
  for (let y = 0; y < frame.height; y += 1) {
    let hash = 2166136261
    for (let x = 0; x < limit; x += 4) {
      const offset = (y * frame.width + x) * 4
      // 量化颜色后计算行哈希，容忍滚动动画造成的少量抗锯齿差异。
      hash ^= (frame.pixels[offset] >> 3) << 16
      hash ^= (frame.pixels[offset + 1] >> 3) << 8
      hash ^= frame.pixels[offset + 2] >> 3
      hash = Math.imul(hash, 16777619)
    }
    signatures[y] = hash >>> 0
  }
  return signatures
}

function findOverlap(previous, current) {
  if (previous.width !== current.width)
    return {
      previousStart: previous.height,
      currentStart: current.height,
      overlap: 0,
      score: 1
    }
  const previousRows = rowSignatures(previous)
  const currentRows = rowSignatures(current)
  const currentStart = Math.min(
    current.height - 1,
    Math.floor(current.height * 0.08)
  )
  const minimum = Math.max(
    32,
    Math.min(96, Math.floor(Math.min(previous.height, current.height) * 0.12))
  )
  const searchStart = Math.max(0, previous.height - current.height * 2)
  const positions = new Map()
  for (let index = currentStart; index < current.height - minimum; index += 1) {
    const list = positions.get(currentRows[index]) || []
    if (list.length < 8) list.push(index)
    positions.set(currentRows[index], list)
  }
  let best = null
  for (let start = searchStart; start < previous.height - minimum; start += 1) {
    const matches = positions.get(previousRows[start]) || []
    for (const candidate of matches) {
      let length = 0
      while (
        start + length < previous.height &&
        candidate + length < current.height &&
        previousRows[start + length] === currentRows[candidate + length]
      )
        length += 1
      if (length < minimum) continue
      const score = overlapScore(previous, current, start, candidate, length)
      const resultHeight = start + length + current.height - candidate - length
      if (resultHeight < previous.height) continue
      if (
        !best ||
        length > best.overlap ||
        (length === best.overlap && score < best.score)
      )
        best = {
          previousStart: start,
          currentStart: candidate,
          overlap: length,
          score
        }
    }
  }
  if (best && best.score < 0.3) return best
  // 没有稳定哈希匹配时，用像素差找一个安全的短重叠，避免整屏突然变化时重复大段内容。
  const fallbackOverlap = Math.max(
    48,
    Math.min(96, Math.floor(current.height * 0.16))
  )
  let fallback = {
    previousStart: Math.max(0, previous.height - fallbackOverlap),
    currentStart: 0,
    overlap: fallbackOverlap,
    score: Infinity
  }
  for (
    let overlap = fallbackOverlap;
    overlap <= Math.floor(current.height * 0.55);
    overlap += 8
  ) {
    const score = overlapScore(
      previous,
      current,
      previous.height - overlap,
      0,
      overlap
    )
    if (score < fallback.score)
      fallback = {
        previousStart: previous.height - overlap,
        currentStart: 0,
        overlap,
        score
      }
  }
  return fallback
}

function appendFrame(canvas, previous, frame, match) {
  const previousKeep = Math.max(
    1,
    Math.min(
      canvas.height,
      canvas.height - previous.height + match.previousStart + match.overlap
    )
  )
  const currentSkip = Math.min(
    frame.height,
    Math.max(0, match.currentStart + match.overlap)
  )
  if (currentSkip >= frame.height) return canvas
  const output = document.createElement("canvas")
  output.width = frame.width
  output.height = previousKeep + Math.max(1, frame.height - currentSkip)
  const context = output.getContext("2d")
  context.drawImage(
    canvas,
    0,
    0,
    canvas.width,
    previousKeep,
    0,
    0,
    output.width,
    previousKeep
  )
  const source = document.createElement("canvas")
  source.width = frame.width
  source.height = frame.height
  source
    .getContext("2d")
    .putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0)
  context.drawImage(
    source,
    0,
    currentSkip,
    frame.width,
    Math.max(1, frame.height - currentSkip),
    0,
    previousKeep,
    frame.width,
    Math.max(1, frame.height - currentSkip)
  )
  source.width = 0
  source.height = 0
  canvas.width = 0
  canvas.height = 0
  return output
}

function frameCanvas(frame) {
  const canvas = document.createElement("canvas")
  canvas.width = frame.width
  canvas.height = frame.height
  canvas
    .getContext("2d")
    .putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0)
  return canvas
}

const wait = (milliseconds) =>
  new Promise((resolve) => window.setTimeout(resolve, milliseconds))

async function settledFrame(rect, signal) {
  let frame = frameFromBytes(
    await screenshotApi.regionFrame(rect.x, rect.y, rect.width, rect.height)
  )
  for (let attempt = 0; attempt < 6; attempt += 1) {
    if (signal.aborted) break
    await wait(80)
    const next = frameFromBytes(
      await screenshotApi.regionFrame(rect.x, rect.y, rect.width, rect.height)
    )
    if (frameDifference(frame, next) < 0.006) return next
    frame = next
  }
  return frame
}

export async function captureLongScreenshot({
  rect,
  signal,
  shouldAutoScroll,
  onProgress
}) {
  if (!rect?.width || !rect?.height) throw new Error("请先选择长截图区域")
  let canvas
  try {
    await screenshotApi.call("long-start")
    // 覆盖层此时已切换为透明选区边框，等待一次重绘后再抓取首帧。
    await wait(120)
    let previous = await settledFrame(rect, signal)
    canvas = frameCanvas(previous)
    let count = 1
    await onProgress?.(canvas, count)
    // 默认手动滚动，用户开启自动滚动后每次只滚一档，保留足够的拼接重叠。
    // 页面不动或已经到底时继续等待完成指令。
    while (!signal.aborted) {
      await wait(160)
      if (signal.aborted && signal.reason !== "finish") break
      if (!signal.aborted && shouldAutoScroll?.())
        await screenshotApi.call("long-scroll", {
          x: rect.x + Math.floor(rect.width / 2),
          y: rect.y + Math.floor(rect.height / 2),
          delta: -120
        })
      const current = await settledFrame(rect, signal)
      if (signal.aborted && signal.reason !== "finish") break
      const difference = frameDifference(previous, current)
      if (difference < 0.012) continue
      const match = findOverlap(previous, current)
      // 后端 PNG 解码同样限制单边 32768 像素，提前停止避免生成无法保存的长图。
      // 与截图服务的 4000 万像素上限保持一致，避免拼完后无法保存。
      const height =
        canvas.height -
        previous.height +
        match.previousStart +
        current.height -
        match.currentStart
      if (height > 32768 || canvas.width * height > 38_000_000) break
      const next = appendFrame(canvas, previous, current, match)
      if (next === canvas) continue
      canvas = next
      previous = current
      await onProgress?.(canvas, ++count)
    }
    if (signal.aborted && signal.reason !== "finish")
      throw new DOMException("长截图已取消", "AbortError")
    return {
      source: await canvasSource(canvas),
      thumbnail: await thumbnailSource(canvas),
      width: canvas.width,
      height: canvas.height
    }
  } finally {
    if (canvas) {
      canvas.width = 0
      canvas.height = 0
    }
    await screenshotApi.call("long-end")
  }
}

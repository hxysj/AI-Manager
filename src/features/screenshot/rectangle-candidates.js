// 根据 screenshot-win 的候选矩形算法说明实现，使用原图物理像素。
// 算法参考：uclogic/screenshot-win，8cef278c4042827a82ecd088b456eae791c69af3。
// 对应 selector/candidates.go 和 selector/candidate_lines.go 的边缘、线段与四边组合流程。
function candidateEdges(pixels, width, height) {
  const size = width * height
  const gray = new Uint8Array(size)
  for (let index = 0, pixel = 0; index < size; index += 1, pixel += 4) {
    gray[index] =
      (19595 * pixels[pixel] +
        38470 * pixels[pixel + 1] +
        7471 * pixels[pixel + 2] +
        32768) >> 16
  }

  const blur = new Uint8Array(size)
  for (let y = 1; y < height - 1; y += 1) {
    for (let x = 1; x < width - 1; x += 1) {
      const index = y * width + x
      blur[index] =
        (gray[index - width - 1] +
          2 * gray[index - width] +
          gray[index - width + 1] +
          2 * gray[index - 1] +
          4 * gray[index] +
          2 * gray[index + 1] +
          gray[index + width - 1] +
          2 * gray[index + width] +
          gray[index + width + 1]) >> 4
    }
  }

  const magnitude = new Uint16Array(size)
  const direction = new Uint8Array(size)
  for (let y = 2; y < height - 2; y += 1) {
    for (let x = 2; x < width - 2; x += 1) {
      const index = y * width + x
      const gx =
        -blur[index - width - 1] + blur[index - width + 1] -
        2 * blur[index - 1] + 2 * blur[index + 1] -
        blur[index + width - 1] + blur[index + width + 1]
      const gy =
        -blur[index - width - 1] - 2 * blur[index - width] -
        blur[index - width + 1] + blur[index + width - 1] +
        2 * blur[index + width] + blur[index + width + 1]
      const ax = Math.abs(gx)
      const ay = Math.abs(gy)
      magnitude[index] = ax + ay
      if (ay * 1000 <= ax * 414) direction[index] = 0
      else if (ax * 1000 <= ay * 414) direction[index] = 2
      else direction[index] = gx * gy > 0 ? 1 : 3
    }
  }

  const edges = new Uint8Array(size)
  for (let y = 2; y < height - 2; y += 1) {
    for (let x = 2; x < width - 2; x += 1) {
      const index = y * width + x
      if (direction[index] !== 0 && direction[index] !== 2) continue
      const step = direction[index] === 2 ? width : 1
      const bit = direction[index] === 2 ? 2 : 1
      if (
        magnitude[index] >= 20 &&
        magnitude[index] >= magnitude[index - step] &&
        magnitude[index] >= magnitude[index + step]
      ) {
        edges[index] = bit
        if (magnitude[index] >= 100) edges[index] |= bit << 2
      }
    }
  }
  return edges
}

function candidateLines(edges, width, height, vertical) {
  const across = vertical ? width : height
  const along = vertical ? height : width
  const bit = vertical ? 1 : 2
  const minimum = vertical ? 56 : 76
  const stride = vertical ? 1 : width
  const segments = []

  for (let position = 2; position < across - 2; position += 1) {
    let start = -1
    let last = -1
    const flush = () => {
      if (start < 0 || last - start + 1 < minimum) return
      for (let index = segments.length - 1; index >= 0; index -= 1) {
        const segment = segments[index]
        if (position - segment.lo > 12) break
        if (
          position - segment.hi <= 4 &&
          Math.abs(start - segment.start) <= 8 &&
          Math.abs(last - segment.end) <= 8
        ) {
          segment.hi = position
          segment.start = Math.min(segment.start, start)
          segment.end = Math.max(segment.end, last)
          return
        }
      }
      segments.push({ lo: position, hi: position, start, end: last })
    }
    const lo = Math.max(2, position - 2)
    const hi = Math.min(across - 3, position + 2)
    for (let offset = 2; offset < along - 2; offset += 1) {
      let value = 0
      let index = vertical ? offset * width + lo : lo * width + offset
      for (let scan = lo; scan <= hi; scan += 1, index += stride) {
        value |= edges[index]
      }
      if (value & bit) {
        if (start < 0) start = offset
        last = offset
      } else if (start >= 0 && offset - last > 8) {
        flush()
        start = -1
      }
    }
    flush()
  }

  // 合并厚边和相邻轨迹后再统计覆盖率，前缀和用于后续四边组合。
  const lines = segments.map(segment => {
    const hits = new Uint32Array(segment.end - segment.start + 2)
    const strong = new Uint32Array(hits.length)
    const breaks = []
    let gap = 0
    for (let offset = segment.start; offset <= segment.end; offset += 1) {
      let value = 0
      let index = vertical
        ? offset * width + segment.lo
        : segment.lo * width + offset
      for (let scan = segment.lo; scan <= segment.hi; scan += 1, index += stride) {
        value |= edges[index]
      }
      const current = offset - segment.start
      hits[current + 1] = hits[current]
      strong[current + 1] = strong[current]
      if (value & bit) {
        hits[current + 1] += 1
        gap = 0
      } else {
        gap += 1
        if (gap > 8) breaks.push(offset)
      }
      if (value & (bit << 2)) strong[current + 1] += 1
    }
    return {
      position: Math.floor((segment.lo + segment.hi) / 2),
      start: segment.start,
      end: segment.end,
      hits,
      strong,
      breaks
    }
  })
  return lines.sort((left, right) =>
    left.position - right.position || left.start - right.start
  )
}

function supports(line, start, end) {
  if (start < line.start - 12 || end > line.end + 12) return false
  const first = start + 12
  const last = end - 12
  if (first > last || first < line.start || last > line.end) return false
  const lo = first - line.start
  const hi = last - line.start + 1
  const length = hi - lo
  const coverage =
    (line.strong[hi] - line.strong[lo]) * 100 < length * 25 ? 85 : 75
  if ((line.hits[hi] - line.hits[lo]) * 100 < length * coverage) return false
  let left = 0
  let right = line.breaks.length
  while (left < right) {
    const middle = (left + right) >>> 1
    if (line.breaks[middle] < first + 8) left = middle + 1
    else right = middle
  }
  return left === line.breaks.length || line.breaks[left] > last
}

function uniqueRectangles(rectangles) {
  rectangles.sort((left, right) =>
    left.width * left.height - right.width * right.height ||
    left.y - right.y || left.x - right.x ||
    left.y + left.height - right.y - right.height ||
    left.x + left.width - right.x - right.width
  )
  const result = []
  const buckets = new Map()
  for (const rectangle of rectangles) {
    const right = rectangle.x + rectangle.width
    const bottom = rectangle.y + rectangle.height
    let duplicate = false
    // 仅合并四条边都接近的框，保留相互包裹的不同矩形。
    search:
    for (
      let x0 = Math.floor((rectangle.x - 3) / 4);
      x0 <= Math.floor((rectangle.x + 3) / 4);
      x0 += 1
    ) {
      for (
        let y0 = Math.floor((rectangle.y - 3) / 4);
        y0 <= Math.floor((rectangle.y + 3) / 4);
        y0 += 1
      ) {
        for (
          let x1 = Math.floor((right - 3) / 4);
          x1 <= Math.floor((right + 3) / 4);
          x1 += 1
        ) {
          for (
            let y1 = Math.floor((bottom - 3) / 4);
            y1 <= Math.floor((bottom + 3) / 4);
            y1 += 1
          ) {
            const nearby = buckets.get(`${x0},${y0},${x1},${y1}`)
            if (!nearby) continue
            for (const old of nearby) {
              if (
                Math.abs(rectangle.x - old.x) <= 3 &&
                Math.abs(rectangle.y - old.y) <= 3 &&
                Math.abs(right - old.x - old.width) <= 3 &&
                Math.abs(bottom - old.y - old.height) <= 3
              ) {
                duplicate = true
                break search
              }
            }
          }
        }
      }
    }
    if (duplicate) continue
    result.push(rectangle)
    const key = `${Math.floor(rectangle.x / 4)},${Math.floor(rectangle.y / 4)},${Math.floor(right / 4)},${Math.floor(bottom / 4)}`
    const bucket = buckets.get(key)
    if (bucket) bucket.push(rectangle)
    else buckets.set(key, [rectangle])
  }
  return result
}

export function detectRectangles(pixels, width, height) {
  if (
    !Number.isInteger(width) || !Number.isInteger(height) ||
    width <= 0 || height <= 0 || pixels.length !== width * height * 4
  ) throw new Error('矩形检测像素数据不完整')
  if (width < 100 || height < 80) return []
  const edges = candidateEdges(pixels, width, height)
  const horizontal = candidateLines(edges, width, height, false)
  const vertical = candidateLines(edges, width, height, true)
  const rectangles = []
  for (let topIndex = 0; topIndex < horizontal.length; topIndex += 1) {
    const top = horizontal[topIndex]
    for (
      let bottomIndex = topIndex + 1;
      bottomIndex < horizontal.length;
      bottomIndex += 1
    ) {
      const bottom = horizontal[bottomIndex]
      if (bottom.position - top.position + 1 < 80) continue
      const lo = Math.max(top.start, bottom.start) - 12
      const hi = Math.min(top.end, bottom.end) + 12
      if (hi - lo + 1 < 100) continue
      let first = 0
      let last = vertical.length
      while (first < last) {
        const middle = (first + last) >>> 1
        if (vertical[middle].position < lo) first = middle + 1
        else last = middle
      }
      const sides = []
      for (
        let index = first;
        index < vertical.length && vertical[index].position <= hi;
        index += 1
      ) {
        if (supports(vertical[index], top.position, bottom.position))
          sides.push(vertical[index])
      }
      for (let leftIndex = 0; leftIndex < sides.length; leftIndex += 1) {
        const left = sides[leftIndex].position
        for (
          let rightIndex = leftIndex + 1;
          rightIndex < sides.length;
          rightIndex += 1
        ) {
          const right = sides[rightIndex].position
          if (
            right - left + 1 < 100 ||
            !supports(top, left, right) ||
            !supports(bottom, left, right)
          ) continue
          rectangles.push({
            x: left,
            y: top.position,
            width: right - left + 1,
            height: bottom.position - top.position + 1
          })
        }
      }
    }
  }
  return uniqueRectangles(rectangles)
}

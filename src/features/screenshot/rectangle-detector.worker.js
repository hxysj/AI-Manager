import { detectRectangles } from './rectangle-candidates.js'

self.addEventListener('message', ({ data }) => {
  try {
    const pixels = new Uint8ClampedArray(data.pixels, data.byteOffset, data.byteLength)
    const rectangles = detectRectangles(pixels, data.width, data.height)
    const regions = rectangles.map((rectangle, index) => ({
      ...rectangle,
      id: `rectangle-${index}`,
      kind: 'rectangle',
      label: '矩形区域'
    }))
    self.postMessage({ id: data.id, regions })
  } catch (error) {
    self.postMessage({ id: data.id, error: error.message || String(error) })
  }
})

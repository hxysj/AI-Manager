import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { Worker as ThreadWorker } from 'node:worker_threads'
import { parse, compileScript, compileTemplate, compileStyleAsync } from '@vue/compiler-sfc'
import { transformSync } from 'esbuild'
import * as vue from 'vue'
import { detectRectangles } from '../src/features/screenshot/rectangle-candidates.js'
import {
  detectRectangleRegions,
  disposeRectangleDetector
} from '../src/features/screenshot/rectangle-detector.js'

function frame(width, height, shade = 245) {
  const pixels = new Uint8ClampedArray(width * height * 4)
  for (let index = 0; index < pixels.length; index += 4) {
    pixels[index] = shade
    pixels[index + 1] = shade
    pixels[index + 2] = shade
    pixels[index + 3] = 255
  }
  return { pixels, width, height }
}

function paint(image, x, y, width, height, shade = 25) {
  for (let row = y; row < y + height; row += 1) {
    for (let column = x; column < x + width; column += 1) {
      const index = (row * image.width + column) * 4
      image.pixels[index] = shade
      image.pixels[index + 1] = shade
      image.pixels[index + 2] = shade
    }
  }
}

function border(image, rectangle, shade = 25, corner = 0) {
  const { x, y, width, height } = rectangle
  paint(image, x + corner, y, width - corner * 2, 1, shade)
  paint(image, x + corner, y + height - 1, width - corner * 2, 1, shade)
  paint(image, x, y + corner, 1, height - corner * 2, shade)
  paint(image, x + width - 1, y + corner, 1, height - corner * 2, shade)
}

const detect = image => detectRectangles(image.pixels, image.width, image.height)
const outer = { x: 50, y: 40, width: 500, height: 360 }
const middle = { x: 85, y: 70, width: 430, height: 300 }
const inner = { x: 150, y: 120, width: 220, height: 150 }
const nested = frame(640, 480)
for (const rectangle of [outer, middle, inner]) border(nested, rectangle)
assert.deepEqual(detect(nested), [inner, middle, outer], '嵌套框必须按面积排序保留')
assert.deepEqual(detect(frame(640, 480)), [], '空白画面不能产生矩形')
assert.deepEqual(detect(frame(99, 80)), [])
assert.throws(() => detectRectangles(new Uint8Array(4), 640, 480), /数据不完整/)

const small = { x: 30, y: 30, width: 100, height: 80 }
const minimum = frame(200, 150)
border(minimum, small)
assert.deepEqual(detect(minimum), [small], '尺寸门槛使用原图的 100×80 像素')
for (const rectangle of [{ ...small, width: 99 }, { ...small, height: 79 }]) {
  const image = frame(200, 150)
  border(image, rectangle)
  assert.deepEqual(detect(image), [])
}

const panel = { x: 50, y: 40, width: 250, height: 180 }
const weak = frame(400, 300, 180)
border(weak, panel, 160)
assert.deepEqual(detect(weak), [panel], '连续的弱边缘也应通过覆盖率检查')
for (const shade of [25, 160]) {
  const dashed = frame(400, 300, 180)
  border(dashed, panel, shade)
  for (let x = 75; x < 275; x += 20) paint(dashed, x, 35, 3, 11, 180)
  assert.deepEqual(
    detect(dashed),
    shade === 25 ? [panel] : [],
    '同样的断续边框应根据强边缘比例采用 75% 或 85% 覆盖率'
  )
}
const rounded = frame(400, 300)
border(rounded, panel, 25, 8)
assert.deepEqual(detect(rounded), [panel], '四角存在缺口时仍可组合四条边')
const shortGap = frame(400, 300)
border(shortGap, panel)
paint(shortGap, 150, 35, 5, 11, 245)
assert.deepEqual(detect(shortGap), [panel], '短缺口不应打断边框')
const longGap = frame(400, 300)
border(longGap, panel)
paint(longGap, 150, 35, 24, 11, 245)
assert.deepEqual(detect(longGap), [], '长断裂不能拼成有效矩形')
const open = frame(400, 300)
border(open, panel)
paint(open, 295, 35, 11, 190, 245)
assert.deepEqual(detect(open), [], '缺少一整条边时应拒绝')
const thick = frame(400, 300)
paint(thick, 50, 40, 250, 3)
paint(thick, 50, 217, 250, 3)
paint(thick, 50, 40, 3, 180)
paint(thick, 297, 40, 3, 180)
assert.equal(detect(thick).length, 1, '厚边的近邻轨迹不能产生重复候选')
const diagonal = frame(400, 300)
for (let offset = 0; offset < 200; offset += 1) paint(diagonal, 50 + offset, 40 + offset, 1, 1)
assert.deepEqual(detect(diagonal), [], '斜向梯度不能误判为轴向矩形')

const closeNested = frame(600, 400)
const closeOuter = { x: 50, y: 40, width: 450, height: 300 }
const closeInner = { x: 66, y: 56, width: 418, height: 268 }
border(closeNested, closeOuter)
border(closeNested, closeInner)
assert.deepEqual(detect(closeNested), [closeInner, closeOuter], '高重叠率的嵌套框不能被 NMS 删掉')

const grid = frame(1720, 1020)
const gridRectangles = []
for (let row = 0; row < 9; row += 1) {
  for (let column = 0; column < 12; column += 1) {
    const rectangle = { x: 20 + column * 140, y: 20 + row * 110, width: 120, height: 90 }
    gridRectangles.push(rectangle)
    border(grid, rectangle)
  }
}
assert.deepEqual(detect(grid), gridRectangles, '保留超过旧上限 96 个的全部矩形')
const large = frame(3840, 2160)
const largeRectangle = { x: 2300, y: 1500, width: 1100, height: 480 }
border(large, largeRectangle)
border(large, { x: 30, y: 40, width: 1800, height: 1200 })
const started = performance.now()
const largeRegions = detect(large)
const largeDuration = performance.now() - started
assert.ok(largeRegions.some(rectangle => JSON.stringify(rectangle) === JSON.stringify(largeRectangle)))
assert.deepEqual(detect(large), largeRegions, '同一图像的结果和坐标应稳定')

// 在真正的线程中执行浏览器 Worker，覆盖 transferable、帧头偏移和错误恢复。
const threads = []
const terminations = []
class BrowserWorker {
  constructor(url, options) {
    assert.equal(options.type, 'module')
    this.listeners = new Map()
    this.thread = new ThreadWorker(`
      const { parentPort } = require('node:worker_threads')
      globalThis.self = {
        addEventListener: (name, listener) => parentPort.on(name, data => listener({ data })),
        postMessage: data => parentPort.postMessage(data)
      }
      import(${JSON.stringify(url.href)})
    `, { eval: true })
    threads.push(this)
  }
  addEventListener(name, listener) {
    this.listeners.set(name, listener)
    this.thread.on(name, value => listener(name === 'message' ? { data: value } : value))
  }
  postMessage(data, transfer) {
    this.thread.postMessage(data, transfer)
  }
  terminate() {
    terminations.push(this.thread.terminate())
  }
}
globalThis.Worker = BrowserWorker
try {
  const padded = new Uint8Array(nested.pixels.length + 28)
  padded.set(nested.pixels, 20)
  const regions = await detectRectangleRegions(
    new Uint8ClampedArray(padded.buffer, 20, nested.pixels.length),
    nested.width,
    nested.height
  )
  assert.equal(padded.buffer.byteLength, 0, '整屏像素应直接转移给 Worker')
  assert.deepEqual(regions.map(({ x, y, width, height }) => ({ x, y, width, height })), [inner, middle, outer])
  assert.equal(regions[0].kind, 'rectangle')
  assert.equal(regions[0].label, '矩形区域')
  await assert.rejects(detectRectangleRegions(new Uint8ClampedArray(4), 200, 100), /数据不完整/)

  const inFlight = detectRectangleRegions(new Uint8ClampedArray(nested.pixels), nested.width, nested.height)
  const rejection = assert.rejects(inFlight, /检测已结束/)
  const previous = threads.at(-1)
  disposeRectangleDetector()
  await rejection
  const next = detectRectangleRegions(new Uint8ClampedArray(nested.pixels), nested.width, nested.height)
  previous.listeners.get('error')({ message: '旧线程的延迟错误' })
  assert.equal((await next).length, 3, '旧线程错误不能影响新一轮检测')
  assert.equal(threads.length, 2)
  threads.at(-1).postMessage = () => { throw new Error('模拟像素转移失败') }
  await assert.rejects(
    detectRectangleRegions(new Uint8ClampedArray(nested.pixels), nested.width, nested.height),
    /模拟像素转移失败/
  )
  disposeRectangleDetector()
} finally {
  disposeRectangleDetector()
  await Promise.all(terminations)
}

// 直接运行截图主组件的 setup，聚焦检测生命周期及鼠标、Alt 快捷键的真实处理函数。
const filename = 'src/features/screenshot/components/ScreenshotWindow/index.vue'
const { descriptor, errors } = parse(readFileSync(filename, 'utf8'), { filename })
assert.deepEqual(errors, [])
const script = compileScript(descriptor, { id: 'rectangle-test', genDefaultAs: '__component' })
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: 'rectangle-test',
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
for (const style of descriptor.styles) {
  const result = await compileStyleAsync({
    source: style.content,
    filename,
    id: 'rectangle-test',
    scoped: true,
    preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}
const requests = []
const detections = []
const strokes = []
const contexts = []
let unmount
let captureState
let detectorDisposals = 0
const flush = async () => {
  for (let index = 0; index < 5; index += 1) await Promise.resolve()
}
const makeCanvas = () => {
  const context = {
    clearRect() {},
    drawImage() {},
    putImageData(image) { this.image = image },
    getImageData: () => ({ data: new Uint8ClampedArray([25, 25, 25, 255]) }),
    strokeRect: (...args) => strokes.push(args)
  }
  contexts.push(context)
  return { width: 1, height: 1, getContext: () => context }
}
globalThis.location = { search: '?mode=capture' }
globalThis.innerWidth = 320
globalThis.innerHeight = 240
globalThis.devicePixelRatio = 2
globalThis.ImageData = class {
  constructor(data, width, height) { Object.assign(this, { data, width, height }) }
}
globalThis.window = {
  document: { createElement: makeCanvas },
  removeEventListener() {},
  confirm: () => true
}
const imports = {
  vue: {
    ...vue,
    onMounted() {},
    onBeforeUnmount(callback) { unmount = callback }
  },
  '@/api/modules/screenshot': {
    screenshotApi: {
      async frame() {
        const bytes = new ArrayBuffer(20 + nested.pixels.length)
        const header = new DataView(bytes)
        header.setInt32(0, -1600, true)
        header.setInt32(4, -200, true)
        header.setUint32(8, nested.width, true)
        header.setUint32(12, nested.height, true)
        new Uint8ClampedArray(bytes, 20).set(nested.pixels)
        return bytes
      },
      async call(action, payload) {
        requests.push({ action, payload })
        if (action === 'save-document') return 'detected-frame'
        if (action === 'export') return { saved: true }
        if (action === 'cancel') captureState.resetCapture()
        if (action === 'capture-mode') return { long: false }
      }
    }
  },
  '@/api/modules/system': { systemApi: {} },
  '@/api/request': { subscribeReady() {} },
  '../../renderer': {
    canvasSource: async () => 'data:image/png;base64,test',
    thumbnailSource: async () => 'thumbnail',
    selectionRect(start, end) {
      return {
        x: Math.min(start.x, end.x),
        y: Math.min(start.y, end.y),
        width: Math.max(1, Math.abs(end.x - start.x)),
        height: Math.max(1, Math.abs(end.y - start.y))
      }
    }
  },
  '../../long-screenshot': { captureLongScreenshot() {} },
  '../../rectangle-detector': {
    detectRectangleRegions(pixels, width, height) {
      assert.equal(pixels.byteOffset, 20)
      assert.equal(pixels.length, width * height * 4)
      assert.deepEqual(Array.from(pixels.slice(0, 4)), Array.from(nested.pixels.slice(0, 4)))
      return new Promise((resolve, reject) => detections.push({ resolve, reject }))
    },
    disposeRectangleDetector() { detectorDisposals += 1 }
  }
}
const module = { exports: {} }
new Function('require', 'module', 'exports', transformSync(
  `${script.content}\nexport default __component`,
  { format: 'cjs' }
).code)(name => {
  if (imports[name]) return imports[name]
  if (name.endsWith('.vue')) return { __esModule: true, default: {} }
  throw new Error(`测试中存在未处理的依赖：${name}`)
}, module, module.exports)
captureState = vue.proxyRefs(module.exports.default.setup({}, { expose() {} }))
captureState.captureBackground = makeCanvas()
captureState.desktop = {
  getBoundingClientRect: () => ({ left: 0, top: 0, width: 320, height: 240 }),
  setPointerCapture() {}
}
captureState.magnifier = { draw() {} }
const candidates = [inner, middle, outer].map((rectangle, index) => ({
  ...rectangle, id: `rectangle-${index}`, kind: 'rectangle', label: '矩形区域'
}))
await captureState.loadFrame()
assert.equal(requests.at(-1).action, 'ready', '检测未完成时截图也应立即就绪')
assert.equal(detections.length, 1)
assert.deepEqual(captureState.pageRegions, [])
await captureState.movePointer({ clientX: 100, clientY: 80 })
detections[0].resolve(candidates)
await flush()
assert.deepEqual(captureState.activeRegion, candidates[0], '检测完成后应立即命中当前鼠标的小框')
assert.deepEqual(captureState.smartLevels, candidates)
captureState.adjustSmartLevel({ deltaY: 1, preventDefault() {} })
assert.deepEqual(captureState.activeRegion, candidates[1])
await captureState.movePointer({ clientX: 101, clientY: 81 })
assert.deepEqual(captureState.activeRegion, candidates[1], '同一小框内应保留滚轮选择的层级')
await captureState.movePointer({ clientX: 2, clientY: 2 })
assert.equal(captureState.activeRegion, null, '移出所有框立即清空选框')
assert.equal(captureState.containsRegion(inner, { x: 370, y: 130 }), false, '命中范围左闭右开')
assert.equal(detections.length, 1, '移动和滚轮不能重复检测')
assert.equal(requests.some(request => request.action === 'smart-selection'), false)
await captureState.movePointer({ clientX: 100, clientY: 80 })
captureState.startSelection({ button: 0, pointerId: 1, clientX: 100, clientY: 80 })
assert.deepEqual(captureState.selection, candidates[0], '点击采用最小包裹框')
await captureState.movePointer({ clientX: 120, clientY: 100 })
assert.equal(captureState.drag.kind, 'select', '拖动仍可自定义截图范围')
captureState.cancelDrag()

// 连续截图和取消后，即使旧 Promise 迟到也不能把上一帧的框带回来。
await captureState.loadFrame()
const stale = detections.at(-1)
await captureState.loadFrame()
detections.at(-1).resolve(candidates)
await flush()
stale.resolve([{ x: 0, y: 0, width: 640, height: 480 }])
await flush()
assert.deepEqual(captureState.pageRegions, candidates)
await captureState.loadFrame()
const cancelled = detections.at(-1)
captureState.resetCapture()
cancelled.resolve(candidates)
await flush()
assert.equal(captureState.frame, null)
assert.deepEqual(captureState.pageRegions, [])

const keyEvent = (key, extra = {}) => ({
  key, target: { tagName: 'DIV' }, preventDefault() {}, ...extra
})
await captureState.loadFrame()
await captureState.keydown(keyEvent('Alt'))
assert.equal(captureState.showDetectedRegions, true)
captureState.keyup(keyEvent('Alt'))
assert.equal(captureState.showDetectedRegions, false)
const savesBefore = requests.filter(request => request.action === 'save-document').length
const copied = captureState.keydown(keyEvent('c', { altKey: true }))
await flush()
assert.equal(requests.filter(request => request.action === 'save-document').length, savesBefore)
detections.at(-1).resolve(candidates)
await copied
assert.equal(strokes.length, 3, 'Alt+C 必须等待检测完成并画出全部矩形')
assert.deepEqual(requests.findLast(request => request.action === 'save-document').payload.origin, { x: -1600, y: -200 })
assert.ok(requests.some(request => request.action === 'copy'))
assert.equal(captureState.frame, null)
assert.equal(contexts.find(context => context.image)?.image.width, 640)

await captureState.loadFrame()
detections.at(-1).resolve(candidates)
await flush()
assert.equal(captureState.detectedRegions.length, 3, 'Alt 总览不能删掉嵌套层级')
await captureState.keydown(keyEvent('s', { altKey: true }))
assert.equal(strokes.length, 6, 'Alt+S 导出相同的完整矩形集')
assert.ok(requests.some(request => request.action === 'export'))

await captureState.loadFrame()
const lateExport = captureState.keydown(keyEvent('s', { altKey: true }))
captureState.resetCapture()
detections.at(-1).resolve(candidates)
await lateExport
assert.equal(strokes.length, 6, '取消截图后不能继续导出旧框')
await captureState.loadFrame()
detections.at(-1).reject(new Error('测试检测失败'))
await flush()
assert.match(captureState.error, /矩形检测失败.*测试检测失败/)
captureState.startSelection({ button: 0, pointerId: 2, clientX: 2, clientY: 2 })
assert.equal(captureState.drag.kind, 'select', '检测失败后仍然允许手动截图')
captureState.cancelDrag()
await captureState.loadFrame()
const abandoned = detections.at(-1)
unmount()
abandoned.resolve(candidates)
await flush()
assert.deepEqual(captureState.pageRegions, [], '卸载后的检测结果不能回填')
assert.ok(detectorDisposals >= detections.length)

// 使用现有真实组件处理函数验证 Ctrl 和 Command，避免只校验源码字符串。
globalThis.location.search = '?mode=pin&id=keyboard-test'
const pinState = vue.proxyRefs(module.exports.default.setup({}, { expose() {} }))
for (const modifier of ['ctrlKey', 'metaKey']) {
  await pinState.keydown(keyEvent('c', { [modifier]: true }))
  assert.deepEqual(requests.at(-1), {
    action: 'copy', payload: { id: 'keyboard-test' }
  })
}
const beforeInput = requests.length
await pinState.keydown(keyEvent('c', { metaKey: true, target: { tagName: 'TEXTAREA' } }))
assert.equal(requests.length, beforeInput, '文字输入框保留系统复制行为')

const editorFile = 'src/features/screenshot/components/ImageEditor/index.vue'
const editorDescriptor = parse(readFileSync(editorFile, 'utf8'), { filename: editorFile }).descriptor
const editorScript = compileScript(editorDescriptor, { id: 'keyboard-editor', genDefaultAs: '__component' })
const editorModule = { exports: {} }
new Function('require', 'module', 'exports', transformSync(
  `${editorScript.content}\nexport default __component`, { format: 'cjs' }
).code)(name => {
  if (name === 'vue') return { ...imports.vue, watch() {} }
  if (name === '../../renderer') return imports[name]
  if (name.endsWith('.vue')) return { __esModule: true, default: {} }
  throw new Error(`测试中存在未处理的依赖：${name}`)
}, editorModule, editorModule.exports)
const editorState = vue.proxyRefs(editorModule.exports.default.setup(
  { source: '', initialAnnotations: [], busy: false }, { expose() {}, emit() {} }
))
for (const modifier of ['ctrlKey', 'metaKey']) {
  editorState.annotations = [{ type: 'pen' }]
  editorState.undoStack = [[]]
  editorState.redoStack = []
  editorState.keydown(keyEvent('z', { [modifier]: true }))
  assert.deepEqual(editorState.annotations, [])
  editorState.keydown(keyEvent('z', { [modifier]: true, shiftKey: true }))
  assert.deepEqual(editorState.annotations, [{ type: 'pen' }])
  editorState.textPosition = { x: 1, y: 2 }
  editorState.text = '跨平台输入'
  editorState.textKeydown(keyEvent('Enter', { [modifier]: true }))
  assert.equal(editorState.annotations.at(-1).text, '跨平台输入')
}
console.log(`截图矩形检测验证通过；4K 合成图 ${largeRegions.length} 个矩形，耗时 ${largeDuration.toFixed(1)} ms`)
console.log('截图 Ctrl/Command 复制、撤销、重做与文字确认验证通过')

<template>
  <section class="coordinate-tool">
    <section class="coordinate-tool-input-panel">
      <header class="coordinate-tool-panel-head">
        <div>
          <h2>坐标数据</h2>
          <p>粘贴题目接口 JSON，自动识别题框和作答框。</p>
        </div>
        <div class="coordinate-tool-head-actions">
          <button
            type="button"
            title="载入示例"
            aria-label="载入示例"
            @click="loadSample"
          >
            <ClipboardPaste :size="15" />
          </button>
          <button
            type="button"
            title="清空数据"
            aria-label="清空数据"
            @click="clearInput"
          >
            <Trash2 :size="15" />
          </button>
        </div>
      </header>

      <textarea
        v-model="jsonText"
        class="coordinate-tool-json-input"
        spellcheck="false"
        placeholder="在这里粘贴单题对象、题目数组或带 0/1 数字键的题目 JSON..."
      />

      <div
        class="coordinate-tool-parse-status"
        :class="{ 'is-error': parseError }"
        role="status"
      >
        <span class="coordinate-tool-status-dot" />
        <span>{{ parseError || parseStatus }}</span>
      </div>

      <div class="coordinate-tool-input-note">
        <Crosshair :size="15" />
        <span
          >支持多题输入；每道题有 sub_question 时只显示 sub_answer_area*，否则显示
          answer_area*。</span
        >
      </div>
    </section>

    <section class="coordinate-tool-preview-panel">
      <header class="coordinate-tool-panel-head coordinate-tool-preview-head">
        <div>
          <h2>坐标预览</h2>
          <p>
            {{
              questionViews.length
                ? "每道题按自己的题目图片进行坐标映射"
                : "等待题目图片"
            }}
          </p>
        </div>
        <div class="coordinate-tool-summary">
          <span>{{ questionViews.length }} 道题</span>
          <span>{{ questionCount }} 题框</span>
          <span>{{ answerCount }} 作答框</span>
          <span v-if="loadedImageCount">{{ loadedImageCount }} 张题图</span>
        </div>
      </header>

      <div
        v-if="!questionViews.length"
        class="coordinate-tool-preview-empty"
      >
        <ScanLine :size="28" />
        <span>解析后将在这里显示每道题的题目图片和坐标框</span>
      </div>
      <div v-else class="coordinate-tool-question-list">
        <article
          v-for="question in questionViews"
          :key="question.id"
          class="coordinate-tool-question"
        >
          <header class="coordinate-tool-question-head">
            <div>
              <h3>题目 {{ question.index + 1 }}</h3>
              <span v-if="question.bankId">bank_id {{ question.bankId }}</span>
            </div>
            <p>{{ question.mappingSummary }}</p>
          </header>

          <div class="coordinate-tool-workspace">
            <div class="coordinate-tool-stage-wrap">
              <div class="coordinate-tool-stage" :style="question.stageStyle">
                <img
                  v-if="question.sourceUrl && !question.imageError"
                  class="coordinate-tool-image"
                  :src="question.sourceUrl"
                  :alt="'题目 ' + (question.index + 1) + ' 坐标预览'"
                  @load="handleImageLoad(question.id, $event)"
                  @error="handleImageError(question.id)"
                />
                <div v-else class="coordinate-tool-empty-stage">
                  <ImageOff :size="28" />
                  <span>{{
                    question.sourceUrl ? "图片加载失败" : "缺少题目图片地址"
                  }}</span>
                </div>

                <div class="coordinate-tool-overlay" aria-label="坐标框预览">
                  <div
                    v-for="box in question.visibleBoxes"
                    :key="box.id"
                    class="coordinate-tool-box"
                    :class="'is-' + box.type"
                    :style="boxStyle(box, question.renderSize)"
                  >
                    <span class="coordinate-tool-box-label">{{
                      box.overlayLabel || box.label
                    }}</span>
                  </div>
                </div>
              </div>
            </div>

            <aside class="coordinate-tool-controls">
              <div class="coordinate-tool-legend">
                <div class="coordinate-tool-legend-item">
                  <span class="coordinate-tool-legend-swatch is-question" />
                  <span>题框</span>
                  <button
                    type="button"
                    :title="
                      allVisible(question, 'question')
                        ? '隐藏全部题框'
                        : '显示全部题框'
                    "
                    :aria-label="
                      allVisible(question, 'question')
                        ? '隐藏全部题框'
                        : '显示全部题框'
                    "
                    @click="
                      setGroupVisible(
                        question,
                        'question',
                        !allVisible(question, 'question')
                      )
                    "
                  >
                    <Eye
                      v-if="allVisible(question, 'question')"
                      :size="14"
                    />
                    <EyeOff v-else :size="14" />
                  </button>
                </div>
                <div class="coordinate-tool-legend-item">
                  <span class="coordinate-tool-legend-swatch is-answer" />
                  <span>作答框</span>
                  <button
                    type="button"
                    :title="
                      allVisible(question, 'answer')
                        ? '隐藏全部作答框'
                        : '显示全部作答框'
                    "
                    :aria-label="
                      allVisible(question, 'answer')
                        ? '隐藏全部作答框'
                        : '显示全部作答框'
                    "
                    @click="
                      setGroupVisible(
                        question,
                        'answer',
                        !allVisible(question, 'answer')
                      )
                    "
                  >
                    <Eye
                      v-if="allVisible(question, 'answer')"
                      :size="14"
                    />
                    <EyeOff v-else :size="14" />
                  </button>
                </div>
              </div>

              <div
                v-if="!question.allBoxes.length"
                class="coordinate-tool-empty-list"
              >
                <ScanLine :size="20" />
                <span>这道题没有可显示的坐标框</span>
              </div>
              <div v-else class="coordinate-tool-box-list">
                <div
                  v-for="box in question.allBoxes"
                  :key="box.id"
                  class="coordinate-tool-box-row"
                  :class="{
                    'is-hidden': !isVisible(question.id, box.id)
                  }"
                >
                  <span
                    class="coordinate-tool-row-marker"
                    :class="'is-' + box.type"
                  />
                  <div class="coordinate-tool-row-main">
                    <span class="coordinate-tool-row-label">{{
                      box.label
                    }}</span>
                    <span class="coordinate-tool-row-coordinates">
                      映射 x {{ formatCoordinate(box.mappedX) }} · y
                      {{ formatCoordinate(box.mappedY) }} ·
                      {{ formatCoordinate(box.mappedWidth) }} ×
                      {{ formatCoordinate(box.mappedHeight) }}
                    </span>
                  </div>
                  <button
                    type="button"
                    :title="
                      isVisible(question.id, box.id)
                        ? '隐藏此框'
                        : '显示此框'
                    "
                    :aria-label="
                      isVisible(question.id, box.id)
                        ? '隐藏' + box.label
                        : '显示' + box.label
                    "
                    @click="toggleVisible(question.id, box.id)"
                  >
                    <Eye
                      v-if="isVisible(question.id, box.id)"
                      :size="15"
                    />
                    <EyeOff v-else :size="15" />
                  </button>
                </div>
              </div>
            </aside>
          </div>
        </article>
      </div>
    </section>
  </section>
</template>

<script setup>
import { computed, reactive, ref, watch } from "vue"
import {
  ClipboardPaste,
  Crosshair,
  Eye,
  EyeOff,
  ImageOff,
  ScanLine,
  Trash2
} from "lucide-vue-next"

const SAMPLE_DATA = {
  img_url:
    "https://dream-preprocess-center.oss-cn-shenzhen.aliyuncs.com/paper_cut_img%2F1790044045155284916.jpg",
  question_area: [{ x: 1229, y: 89, page: 2, width: 1090, height: 605 }],
  sub_question: [
    {
      sub_answer_area: [
        { x: 1229, y: 89, page: 2, width: 1090, height: 605 },
        { x: 1241, y: 482, page: 2, width: 1090, height: 122 }
      ],
      sub_answer_area_multi: [null, null]
    }
  ],
  answer_area: [],
  answer_area_multi: []
}

const jsonText = ref("")
const parsedItems = ref([])
const parseError = ref("")
const imageSizes = reactive({})
const imageErrors = reactive({})
const visibility = reactive({})

// 统一兼容单个对象、数组和多连框的嵌套数组。
function flattenBoxes(value) {
  if (Array.isArray(value)) return value.flatMap((item) => flattenBoxes(item))
  return value && typeof value === "object" ? [value] : []
}

function numberValue(value) {
  const number = Number(value)
  return Number.isFinite(number) ? number : 0
}

function createBox(raw, id, type, label, source) {
  const box = {
    id,
    type,
    label,
    source,
    x: numberValue(raw.x),
    y: numberValue(raw.y),
    width: numberValue(raw.width),
    height: numberValue(raw.height),
    page: raw.page ?? raw.page_num ?? ""
  }

  return box.width > 0 && box.height > 0 ? box : null
}

function buildBoxes(data, itemIndex = 0) {
  if (!data || typeof data !== "object") return []

  const boxes = []
  flattenBoxes(data.question_area).forEach((item, index) => {
    const box = createBox(
      item,
      `question-${itemIndex}-${index}`,
      "question",
      `题框 ${index + 1}`,
      "question_area"
    )
    if (box) boxes.push(box)
  })

  const subQuestions = Array.isArray(data.sub_question) ? data.sub_question : []
  if (subQuestions.length) {
    subQuestions.forEach((subQuestion, subIndex) => {
      flattenBoxes(subQuestion.sub_answer_area).forEach((item, index) => {
        const box = createBox(
          item,
          `answer-${itemIndex}-sub-${subIndex}-${index}`,
          "answer",
          `小题 ${subIndex + 1} · 作答框 ${index + 1}`,
          "sub_answer_area"
        )
        if (box) boxes.push(box)
      })
      flattenBoxes(subQuestion.sub_answer_area_multi).forEach((item, index) => {
        const box = createBox(
          item,
          `answer-${itemIndex}-sub-multi-${subIndex}-${index}`,
          "answer",
          `小题 ${subIndex + 1} · 多连框 ${index + 1}`,
          "sub_answer_area_multi"
        )
        if (box) boxes.push(box)
      })
    })
  } else {
    flattenBoxes(data.answer_area).forEach((item, index) => {
      const box = createBox(
        item,
        `answer-${itemIndex}-${index}`,
        "answer",
        `作答框 ${index + 1}`,
        "answer_area"
      )
      if (box) boxes.push(box)
    })
    flattenBoxes(data.answer_area_multi).forEach((item, index) => {
      const box = createBox(
        item,
        `answer-${itemIndex}-multi-${index}`,
        "answer",
        `多连框 ${index + 1}`,
        "answer_area_multi"
      )
      if (box) boxes.push(box)
    })
  }

  let answerSequence = 0
  boxes.forEach((box) => {
    box.overlayLabel =
      box.type === "answer" ? String(++answerSequence) : box.label
  })

  return boxes
}

function mergeBounds(boxes) {
  if (!boxes.length) {
    return { x: 0, y: 0, width: 1, height: 1 }
  }

  const minX = Math.min(...boxes.map((box) => box.x))
  const minY = Math.min(...boxes.map((box) => box.y))
  const maxX = Math.max(...boxes.map((box) => box.x + box.width))
  const maxY = Math.max(...boxes.map((box) => box.y + box.height))

  return {
    x: minX,
    y: minY,
    width: Math.max(maxX - minX, 1),
    height: Math.max(maxY - minY, 1)
  }
}

function buildQuestionFrame(questionBoxes, imageSize = {}) {
  const bounds = mergeBounds(questionBoxes)
  if (questionBoxes.length <= 1) {
    return {
      ...bounds,
      mode: "bounds",
      segments: questionBoxes.map((box) => ({
        ...box,
        offsetX: box.x - bounds.x,
        offsetY: box.y - bounds.y
      }))
    }
  }

  const sortedBoxes = [...questionBoxes].sort((a, b) => {
    if (a.page === "" || b.page === "") return 0
    return numberValue(a.page) - numberValue(b.page)
  })
  const vertical = {
    x: sortedBoxes[0].x,
    y: sortedBoxes[0].y,
    width: Math.max(...sortedBoxes.map((box) => box.width), 1),
    height: Math.max(
      sortedBoxes.reduce((sum, box) => sum + box.height, 0),
      1
    )
  }
  const verticalSegments = []
  let heightOffset = 0
  sortedBoxes.forEach((box) => {
    verticalSegments.push({
      ...box,
      offsetX: box.x - vertical.x,
      offsetY: heightOffset
    })
    heightOffset += box.height
  })

  // 有明确分页时使用纵向拼接；同页多框按最小外接矩形合并。
  const pageKeys = new Set(
    questionBoxes
      .map((box) => box.page)
      .filter((page) => page !== "")
      .map((page) => String(page))
  )
  let mode = pageKeys.size > 1 ? "vertical" : "bounds"
  if (imageSize.width && imageSize.height && pageKeys.size <= 1) {
    const imageAspect = imageSize.width / imageSize.height
    const boundsDistance = Math.abs(imageAspect - bounds.width / bounds.height)
    const verticalDistance = Math.abs(
      imageAspect - vertical.width / vertical.height
    )
    if (verticalDistance < boundsDistance) mode = "vertical"
  }

  return mode === "vertical"
    ? { ...vertical, mode, segments: verticalSegments }
    : {
        ...bounds,
        mode,
        segments: questionBoxes.map((box) => ({
          ...box,
          offsetX: box.x - bounds.x,
          offsetY: box.y - bounds.y
        }))
      }
}

function overlapArea(first, second) {
  const width = Math.max(
    0,
    Math.min(first.x + first.width, second.x + second.width) -
      Math.max(first.x, second.x)
  )
  const height = Math.max(
    0,
    Math.min(first.y + first.height, second.y + second.height) -
      Math.max(first.y, second.y)
  )
  return width * height
}

function findQuestionSegment(box, frame) {
  if (!frame.segments.length) return null
  if (box.type === "question") {
    return frame.segments.find((segment) => segment.id === box.id) || null
  }

  const pageMatches = frame.segments.filter(
    (segment) =>
      box.page === "" || segment.page === "" || String(box.page) === String(segment.page)
  )
  const candidates = pageMatches.length ? pageMatches : frame.segments
  const centerX = box.x + box.width / 2
  const centerY = box.y + box.height / 2
  const centered = candidates.find(
    (segment) =>
      centerX >= segment.x &&
      centerX <= segment.x + segment.width &&
      centerY >= segment.y &&
      centerY <= segment.y + segment.height
  )
  if (centered) return centered

  return candidates.reduce((best, segment) => {
    if (!best || overlapArea(box, segment) > overlapArea(box, best)) {
      return segment
    }
    return best
  }, null)
}

function formatCoordinate(value) {
  if (!Number.isFinite(value)) return "0"
  const rounded = Math.round(value * 100) / 100
  return String(rounded)
}

function normalizeQuestionItems(data) {
  if (Array.isArray(data)) {
    return data.filter((item) => item && typeof item === "object")
  }
  if (!data || typeof data !== "object") return []

  const numericKeys = Object.keys(data)
    .filter((key) => /^\d+$/.test(key))
    .sort((first, second) => Number(first) - Number(second))
  if (
    numericKeys.length &&
    numericKeys.every(
      (key) => data[key] && typeof data[key] === "object" && !Array.isArray(data[key])
    )
  ) {
    return numericKeys.map((key) => data[key])
  }
  return [data]
}

function clearViewState() {
  Object.keys(imageSizes).forEach((id) => delete imageSizes[id])
  Object.keys(imageErrors).forEach((id) => delete imageErrors[id])
  Object.keys(visibility).forEach((id) => delete visibility[id])
}

function parseInput() {
  clearViewState()
  if (!jsonText.value.trim()) {
    parsedItems.value = []
    parseError.value = ""
    return
  }

  try {
    const data = JSON.parse(jsonText.value)
    const items = normalizeQuestionItems(data)
    if (!items.length) throw new Error("请输入题目对象或题目数组")
    parsedItems.value = items
    parseError.value = ""
  } catch (error) {
    parsedItems.value = []
    parseError.value = error.message || "JSON 格式无效"
  }
}

watch(jsonText, parseInput, { immediate: true })

function getSourceUrl(data) {
  return data.img_url || data.img_original_url || data.img_template_url || ""
}

const questionViews = computed(() =>
  parsedItems.value.map((data, index) => {
    const id = "question-" + index
    const rawBoxes = buildBoxes(data, index)
    const rawQuestionBoxes = rawBoxes.filter((box) => box.type === "question")
    const imageSize = imageSizes[id] || {}
    const questionFrame = buildQuestionFrame(
      rawQuestionBoxes.length ? rawQuestionBoxes : rawBoxes,
      imageSize
    )
    const renderSize = {
      width: imageSize.width || questionFrame.width,
      height: imageSize.height || questionFrame.height
    }
    const mappingScale = {
      x: renderSize.width / questionFrame.width,
      y: renderSize.height / questionFrame.height
    }
    const allBoxes = rawBoxes.map((box) => {
      const segment = findQuestionSegment(box, questionFrame)
      const localX = segment
        ? segment.offsetX + box.x - segment.x
        : box.x - questionFrame.x
      const localY = segment
        ? segment.offsetY + box.y - segment.y
        : box.y - questionFrame.y
      return {
        ...box,
        mappedX: localX * mappingScale.x,
        mappedY: localY * mappingScale.y,
        mappedWidth: box.width * mappingScale.x,
        mappedHeight: box.height * mappingScale.y
      }
    })
    const mappingSummary =
      "合并题框 " +
      formatCoordinate(questionFrame.width) +
      " × " +
      formatCoordinate(questionFrame.height) +
      " → 题目图片 " +
      formatCoordinate(renderSize.width) +
      " × " +
      formatCoordinate(renderSize.height) +
      "，缩放 " +
      formatCoordinate(mappingScale.x) +
      " × " +
      formatCoordinate(mappingScale.y)

    return {
      id,
      index,
      bankId: data.bank_id,
      sourceUrl: getSourceUrl(data),
      imageError: imageErrors[id] === true,
      imageSize,
      questionFrame,
      renderSize,
      mappingSummary,
      stageStyle: {
        aspectRatio: renderSize.width + " / " + renderSize.height
      },
      allBoxes,
      questionBoxes: allBoxes.filter((box) => box.type === "question"),
      answerBoxes: allBoxes.filter((box) => box.type === "answer"),
      visibleBoxes: allBoxes.filter((box) => isVisible(id, box.id))
    }
  })
)
const questionCount = computed(() =>
  questionViews.value.reduce((count, question) => count + question.questionBoxes.length, 0)
)
const answerCount = computed(() =>
  questionViews.value.reduce((count, question) => count + question.answerBoxes.length, 0)
)
const loadedImageCount = computed(
  () => questionViews.value.filter((question) => question.imageSize.width).length
)
const parseStatus = computed(() => {
  if (!parsedItems.value.length) return "等待输入 JSON"
  const boxCount = questionCount.value + answerCount.value
  return parsedItems.value.length + " 道题，" + boxCount + " 个坐标框已解析"
})

function visibilityKey(viewId, boxId) {
  return viewId + ":" + boxId
}

function isVisible(viewId, boxId) {
  return visibility[visibilityKey(viewId, boxId)] !== false
}

function toggleVisible(viewId, boxId) {
  const key = visibilityKey(viewId, boxId)
  visibility[key] = !isVisible(viewId, boxId)
}

function allVisible(question, type) {
  const boxes = question.allBoxes.filter((box) => box.type === type)
  return boxes.length > 0 && boxes.every((box) => isVisible(question.id, box.id))
}

function setGroupVisible(question, type, visible) {
  question.allBoxes
    .filter((box) => box.type === type)
    .forEach((box) => {
      visibility[visibilityKey(question.id, box.id)] = visible
    })
}

function boxStyle(box, renderSize) {
  return {
    left: (box.mappedX / renderSize.width) * 100 + "%",
    top: (box.mappedY / renderSize.height) * 100 + "%",
    width: (box.mappedWidth / renderSize.width) * 100 + "%",
    height: (box.mappedHeight / renderSize.height) * 100 + "%"
  }
}

function handleImageLoad(viewId, event) {
  imageErrors[viewId] = false
  imageSizes[viewId] = {
    width: event.target.naturalWidth || 0,
    height: event.target.naturalHeight || 0
  }
}

function handleImageError(viewId) {
  imageErrors[viewId] = true
  imageSizes[viewId] = { width: 0, height: 0 }
}

function loadSample() {
  jsonText.value = JSON.stringify(SAMPLE_DATA, null, 2)
}

function clearInput() {
  jsonText.value = ""
}
</script>

<style scoped lang="less">
.coordinate-tool {
  display: flex;
  min-height: 0;
  flex: 1;
  gap: 14px;
  overflow: hidden;
  color: var(--color-text);

  .coordinate-tool-input-panel,
  .coordinate-tool-preview-panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
  }

  .coordinate-tool-input-panel {
    width: 340px;
    flex: 0 0 340px;
    padding: 16px;
  }

  .coordinate-tool-preview-panel {
    min-width: 0;
    flex: 1;
    overflow: hidden;
  }

  .coordinate-tool-preview-empty {
    display: flex;
    min-height: 0;
    flex: 1;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 12px;
  }

  .coordinate-tool-question-list {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding: 14px;
  }

  .coordinate-tool-question {
    min-height: 0;
  }

  .coordinate-tool-question + .coordinate-tool-question {
    margin-top: 16px;
    border-top: 1px solid var(--color-line);
    padding-top: 16px;
  }

  .coordinate-tool-question-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 10px;

    h3 {
      margin: 0;
      font-size: 14px;
      line-height: 1.35;
    }

    span,
    p {
      color: var(--color-text-muted);
      font-size: 11px;
      line-height: 1.4;
    }

    span {
      display: block;
      margin-top: 3px;
    }

    p {
      margin: 0;
      text-align: right;
    }
  }

  .coordinate-tool-panel-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;

    h2 {
      margin: 0;
      font-size: 16px;
      line-height: 1.3;
    }

    p {
      margin: 5px 0 0;
      color: var(--color-text-muted);
      font-size: 12px;
      line-height: 1.45;
    }
  }

  .coordinate-tool-preview-head {
    flex: none;
    padding: 16px 18px 12px;
    border-bottom: 1px solid var(--color-line);
  }

  .coordinate-tool-head-actions,
  .coordinate-tool-summary,
  .coordinate-tool-legend,
  .coordinate-tool-legend-item {
    display: flex;
    align-items: center;
  }

  .coordinate-tool-head-actions {
    gap: 6px;

    button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 28px;
      height: 28px;
      padding: 0;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      cursor: pointer;
    }

    button:hover {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }
  }

  .coordinate-tool-json-input {
    min-height: 300px;
    flex: 1;
    margin-top: 14px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    outline: none;
    background: var(--color-panel-soft);
    color: var(--color-text);
    font-family: "Cascadia Code", "SFMono-Regular", Consolas, monospace;
    font-size: 12px;
    line-height: 1.55;
    resize: none;

    &:focus {
      border-color: var(--color-primary);
      box-shadow: 0 0 0 3px var(--color-primary-soft);
    }
  }

  .coordinate-tool-parse-status,
  .coordinate-tool-input-note {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--color-text-muted);
    font-size: 12px;
    line-height: 1.45;
  }

  .coordinate-tool-parse-status {
    margin-top: 10px;

    &.is-error {
      color: #dc2626;
    }
  }

  .coordinate-tool-status-dot {
    width: 7px;
    height: 7px;
    flex: 0 0 7px;
    border-radius: 50%;
    background: #22c55e;
  }

  .coordinate-tool-parse-status.is-error .coordinate-tool-status-dot {
    background: #ef4444;
  }

  .coordinate-tool-input-note {
    align-items: flex-start;
    margin-top: 12px;
    padding: 10px;
    border-radius: 8px;
    background: var(--color-primary-soft);
  }

  .coordinate-tool-summary {
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;

    span {
      padding: 5px 8px;
      border-radius: 6px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      font-size: 11px;
    }
  }

  .coordinate-tool-workspace {
    display: flex;
    min-height: 360px;
    gap: 14px;
  }

  .coordinate-tool-stage-wrap {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    align-items: center;
    justify-content: center;
    overflow: auto;
    border-radius: 9px;
    background: #e8edf3;
  }

  .coordinate-tool-stage {
    position: relative;
    width: 100%;
    max-width: 100%;
    overflow: hidden;
    background: #f8fafc;
    box-shadow: 0 12px 28px rgba(30, 41, 59, 0.14);
  }

  .coordinate-tool-image,
  .coordinate-tool-empty-stage {
    display: block;
    width: 100%;
    height: 100%;
  }

  .coordinate-tool-image {
    object-fit: contain;
  }

  .coordinate-tool-empty-stage {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 8px;
    color: #64748b;
    font-size: 12px;
  }

  .coordinate-tool-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .coordinate-tool-box {
    position: absolute;
    min-width: 3px;
    min-height: 3px;
    box-sizing: border-box;
    pointer-events: none;

    &.is-question {
      border: 2px solid #f59e0b;
      background: rgba(245, 158, 11, 0.1);
    }

    &.is-answer {
      border: 2px solid #0284c7;
      background: rgba(2, 132, 199, 0.1);
    }
  }

  .coordinate-tool-box-label {
    position: absolute;
    top: 2px;
    left: 2px;
    max-width: 160px;
    overflow: hidden;
    padding: 2px 5px;
    border-radius: 0 0 5px 0;
    background: rgba(15, 23, 42, 0.82);
    color: #fff;
    font-size: 10px;
    line-height: 1.3;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .coordinate-tool-controls {
    display: flex;
    width: 286px;
    min-width: 240px;
    min-height: 0;
    flex-direction: column;
    border-left: 1px solid var(--color-line);
    padding-left: 14px;
  }

  .coordinate-tool-legend {
    flex: none;
    gap: 12px;
    padding: 0 0 10px;
    border-bottom: 1px solid var(--color-line);
  }

  .coordinate-tool-legend-item {
    gap: 6px;
    min-width: 0;
    color: var(--color-text-muted);
    font-size: 12px;

    button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 28px;
      height: 28px;
      margin-left: 2px;
      padding: 0;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      cursor: pointer;
    }

    button:hover {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }
  }

  .coordinate-tool-legend-swatch,
  .coordinate-tool-row-marker {
    display: inline-block;
    width: 9px;
    height: 9px;
    flex: 0 0 9px;
    border-radius: 2px;

    &.is-question {
      background: #f59e0b;
    }

    &.is-answer {
      background: #0284c7;
    }
  }

  .coordinate-tool-empty-list {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 160px;
    flex-direction: column;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 12px;
    text-align: center;
  }

  .coordinate-tool-box-list {
    min-height: 0;
    overflow-y: auto;
    padding-top: 8px;
  }

  .coordinate-tool-box-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 8px 0;
    border-bottom: 1px solid var(--color-line);

    &.is-hidden {
      opacity: 0.48;
    }

    button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 26px;
      height: 26px;
      flex: 0 0 26px;
      padding: 0;
      border: 0;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
    }

    button:hover {
      color: var(--color-primary);
    }
  }

  .coordinate-tool-row-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  .coordinate-tool-row-label,
  .coordinate-tool-row-coordinates {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .coordinate-tool-row-label {
    color: var(--color-text);
    font-size: 12px;
  }

  .coordinate-tool-row-coordinates {
    color: var(--color-text-muted);
    font-family: "Cascadia Code", "SFMono-Regular", Consolas, monospace;
    font-size: 10px;
  }
}

:global(:root[data-theme="dark"]) .coordinate-tool,
.tools-view--dark .coordinate-tool {
  .coordinate-tool-stage-wrap {
    background: #111827;
  }

  .coordinate-tool-stage {
    background: #0f172a;
  }
}
</style>

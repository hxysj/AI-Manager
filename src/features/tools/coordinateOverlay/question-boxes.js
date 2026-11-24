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

export function buildBoxes(data, itemIndex = 0) {
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

export function normalizeQuestionItems(data) {
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
      (key) =>
        data[key] && typeof data[key] === "object" && !Array.isArray(data[key])
    )
  ) {
    return numericKeys.map((key) => data[key])
  }
  return [data]
}

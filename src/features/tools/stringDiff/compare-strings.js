// 页面与应用助手共用同一个文本和 JSON 对比算法。
export function compareStrings(left, right) {
  if (!left.trim() || !right.trim()) {
    return {
      rows: [],
      mode: "待输入",
      info: "请在左右两侧都输入内容后开始对比。",
      warn: false
    }
  }

  const leftJson = tryParseJson(left)
  const rightJson = tryParseJson(right)

  if (leftJson.ok && rightJson.ok) {
    return {
      rows: buildJsonRows(leftJson.value, rightJson.value),
      mode: "JSON",
      info: "已按 JSON 路径和值进行对比。",
      warn: false
    }
  }

  const warn = leftJson.ok !== rightJson.ok
  return {
    rows: buildTextRows(left, right),
    mode: "字符串",
    info: warn
      ? "一侧 JSON 解析失败，已按字符串对比。"
      : "已按字符串内容进行对比。",
    warn
  }
}

export function tryParseJson(text) {
  const trimmed = text.trim()

  if (!trimmed) {
    return { ok: false }
  }

  try {
    return { ok: true, value: JSON.parse(trimmed) }
  } catch {
    return { ok: false }
  }
}

function stableStringify(value) {
  if (value === undefined) {
    return ""
  }
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value)
  }
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(",")}]`
  }

  const body = Object.keys(value)
    .sort((a, b) => a.localeCompare(b, "zh-Hans-CN"))
    .map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`)
    .join(",")
  return `{${body}}`
}

function prettyValue(value) {
  if (value === undefined) {
    return ""
  }
  if (typeof value === "string") {
    return value
  }
  return JSON.stringify(value, null, 2)
}

function formatPath(parent, key, isArray) {
  if (isArray) {
    return `${parent}[${key}]`
  }

  return /^[A-Za-z_$][\w$]*$/.test(key)
    ? `${parent}.${key}`
    : `${parent}[${JSON.stringify(key)}]`
}

// JSON 先展开为叶子路径，避免对象键顺序影响比较结果。
function flattenJson(value, path = "$", map = new Map()) {
  if (value === null || typeof value !== "object") {
    map.set(path, {
      normalized: stableStringify(value),
      display: prettyValue(value)
    })
    return map
  }

  if (Array.isArray(value)) {
    if (!value.length) {
      map.set(path, { normalized: "[]", display: "[]" })
      return map
    }
    value.forEach((item, index) =>
      flattenJson(item, formatPath(path, index, true), map)
    )
    return map
  }

  const keys = Object.keys(value).sort((a, b) =>
    a.localeCompare(b, "zh-Hans-CN")
  )
  if (!keys.length) {
    map.set(path, { normalized: "{}", display: "{}" })
    return map
  }
  keys.forEach((key) =>
    flattenJson(value[key], formatPath(path, key, false), map)
  )
  return map
}

function lcsPairs(left, right) {
  const rows = left.length + 1
  const columns = right.length + 1
  const matrix = Array.from({ length: rows }, () => Array(columns).fill(0))

  for (let leftIndex = left.length - 1; leftIndex >= 0; leftIndex -= 1) {
    for (let rightIndex = right.length - 1; rightIndex >= 0; rightIndex -= 1) {
      matrix[leftIndex][rightIndex] =
        left[leftIndex] === right[rightIndex]
          ? matrix[leftIndex + 1][rightIndex + 1] + 1
          : Math.max(
              matrix[leftIndex + 1][rightIndex],
              matrix[leftIndex][rightIndex + 1]
            )
    }
  }

  const pairs = []
  let leftIndex = 0
  let rightIndex = 0
  while (leftIndex < left.length && rightIndex < right.length) {
    if (left[leftIndex] === right[rightIndex]) {
      pairs.push([leftIndex, rightIndex])
      leftIndex += 1
      rightIndex += 1
    } else if (
      matrix[leftIndex + 1][rightIndex] >= matrix[leftIndex][rightIndex + 1]
    ) {
      leftIndex += 1
    } else {
      rightIndex += 1
    }
  }
  return pairs
}

function tokenizeText(text) {
  return Array.from(
    text.matchAll(/[\u4e00-\u9fa5]|[A-Za-z0-9_]+|\s+|./gu),
    (match) => match[0]
  )
}

function createDiffParts(tokens, keepIndexes) {
  const parts = []
  tokens.forEach((token, index) => {
    const changed = !keepIndexes.has(index)
    const previous = parts.at(-1)

    if (previous?.changed === changed) {
      previous.text += token
    } else {
      parts.push({ text: token, changed })
    }
  })
  return parts
}

function inlineDiff(left, right) {
  if (left === right) {
    return {
      leftParts: left ? [{ text: left, changed: false }] : [],
      rightParts: right ? [{ text: right, changed: false }] : []
    }
  }

  const leftTokens = tokenizeText(left)
  const rightTokens = tokenizeText(right)
  const leftKeep = new Set()
  const rightKeep = new Set()
  lcsPairs(leftTokens, rightTokens).forEach(([leftIndex, rightIndex]) => {
    leftKeep.add(leftIndex)
    rightKeep.add(rightIndex)
  })

  return {
    leftParts: createDiffParts(leftTokens, leftKeep),
    rightParts: createDiffParts(rightTokens, rightKeep)
  }
}

function buildJsonRows(leftValue, rightValue) {
  const leftMap = flattenJson(leftValue)
  const rightMap = flattenJson(rightValue)
  const paths = Array.from(
    new Set([...leftMap.keys(), ...rightMap.keys()])
  ).sort((a, b) => a.localeCompare(b, "zh-Hans-CN", { numeric: true }))

  return paths.map((path) => {
    const leftItem = leftMap.get(path)
    const rightItem = rightMap.get(path)
    const hasLeft = Boolean(leftItem)
    const hasRight = Boolean(rightItem)
    const same =
      hasLeft && hasRight && leftItem.normalized === rightItem.normalized
    let status = "不同"

    if (same) {
      status = "相同"
    } else if (!hasLeft) {
      status = "右侧新增"
    } else if (!hasRight) {
      status = "左侧独有"
    }

    const leftDisplay = hasLeft ? leftItem.display : "缺失"
    const rightDisplay = hasRight ? rightItem.display : "缺失"
    return {
      key: path,
      status,
      same,
      leftText: hasLeft ? leftItem.display : "",
      rightText: hasRight ? rightItem.display : "",
      leftParts: leftDisplay ? [{ text: leftDisplay, changed: !same }] : [],
      rightParts: rightDisplay ? [{ text: rightDisplay, changed: !same }] : []
    }
  })
}

function buildTextRows(leftValue, rightValue) {
  const leftLines = leftValue.split(/\r?\n/)
  const rightLines = rightValue.split(/\r?\n/)
  const pairs = lcsPairs(leftLines, rightLines)
  const rows = []
  let leftIndex = 0
  let rightIndex = 0

  function pushChanged(currentLeft, currentRight, leftNo, rightNo, status) {
    const leftLine = currentLeft ?? ""
    const rightLine = currentRight ?? ""
    const same = leftLine === rightLine
    const parts = inlineDiff(leftLine, rightLine)

    rows.push({
      key: `${leftNo || "-"} / ${rightNo || "-"}`,
      leftLineNo: leftNo || null,
      rightLineNo: rightNo || null,
      status: same ? "相同" : status,
      same,
      leftText: leftLine,
      rightText: rightLine,
      leftParts:
        parts.leftParts.length || currentLeft !== undefined
          ? parts.leftParts
          : [{ text: "缺失", changed: true }],
      rightParts:
        parts.rightParts.length || currentRight !== undefined
          ? parts.rightParts
          : [{ text: "缺失", changed: true }]
    })
  }

  pairs.forEach(([nextLeft, nextRight]) => {
    while (leftIndex < nextLeft || rightIndex < nextRight) {
      if (leftIndex < nextLeft && rightIndex < nextRight) {
        pushChanged(
          leftLines[leftIndex],
          rightLines[rightIndex],
          leftIndex + 1,
          rightIndex + 1,
          "不同"
        )
        leftIndex += 1
        rightIndex += 1
      } else if (leftIndex < nextLeft) {
        pushChanged(
          leftLines[leftIndex],
          undefined,
          leftIndex + 1,
          "",
          "左侧独有"
        )
        leftIndex += 1
      } else {
        pushChanged(
          undefined,
          rightLines[rightIndex],
          "",
          rightIndex + 1,
          "右侧新增"
        )
        rightIndex += 1
      }
    }

    pushChanged(
      leftLines[nextLeft],
      rightLines[nextRight],
      nextLeft + 1,
      nextRight + 1,
      "相同"
    )
    leftIndex = nextLeft + 1
    rightIndex = nextRight + 1
  })

  while (leftIndex < leftLines.length || rightIndex < rightLines.length) {
    if (leftIndex < leftLines.length && rightIndex < rightLines.length) {
      pushChanged(
        leftLines[leftIndex],
        rightLines[rightIndex],
        leftIndex + 1,
        rightIndex + 1,
        "不同"
      )
      leftIndex += 1
      rightIndex += 1
    } else if (leftIndex < leftLines.length) {
      pushChanged(
        leftLines[leftIndex],
        undefined,
        leftIndex + 1,
        "",
        "左侧独有"
      )
      leftIndex += 1
    } else {
      pushChanged(
        undefined,
        rightLines[rightIndex],
        "",
        rightIndex + 1,
        "右侧新增"
      )
      rightIndex += 1
    }
  }

  return rows
}

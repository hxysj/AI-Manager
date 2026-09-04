<template>
  <section class="string-diff">
    <div class="string-diff-toolbar">
      <label class="string-diff-toggle" title="仅显示不一致的内容">
        <input v-model="diffOnly" type="checkbox" />
        <span class="string-diff-toggle-label">仅差异</span>
      </label>

      <div class="string-diff-actions">
        <button
          class="string-diff-button string-diff-button-primary"
          type="button"
          @click="clearContent"
        >
          <Trash2 :size="15" />
          清空
        </button>
        <button
          class="string-diff-button string-diff-detail-button"
          type="button"
          :class="{ 'string-diff-detail-button-active': resultVisible }"
          :disabled="!comparison.rows.length"
          @click="resultVisible = !resultVisible"
        >
          <PanelRight :size="15" />
          差异详情
          <span v-if="comparison.rows.length" class="string-diff-detail-count">
            {{ comparison.rows.filter((row) => !row.same).length }}
          </span>
        </button>
      </div>
    </div>

    <section class="string-diff-inputs" aria-label="输入区">
      <article class="string-diff-panel">
        <header class="string-diff-panel-head">
          <span data-emphasis class="string-diff-panel-title">左侧内容</span>
          <span class="string-diff-panel-meta">
            <span v-if="leftParseState" class="string-diff-parse-state">{{
              leftParseState
            }}</span>
            <span class="string-diff-counter">{{ leftText.length }} 字符</span>
          </span>
        </header>
        <div class="string-diff-editor">
          <pre
            ref="leftHighlight"
            class="string-diff-highlight"
            aria-hidden="true"
            v-html="leftHighlightHtml"
          ></pre>
          <textarea
            ref="leftInput"
            v-model="leftText"
            class="string-diff-textarea"
            wrap="off"
            spellcheck="false"
            placeholder="在这里输入 JSON 或字符串"
            @scroll="syncEditorScroll('left', $event)"
          ></textarea>
        </div>
      </article>

      <div class="string-diff-navigation" aria-label="差异导航">
        <button
          v-for="marker in diffMarkers"
          :key="marker.key"
          class="string-diff-navigation-dot"
          :class="{
            'string-diff-navigation-dot-active': activeDiffKey === marker.key
          }"
          :style="{ top: `${marker.position}%` }"
          type="button"
          :aria-label="marker.label"
          :title="marker.label"
          @click.stop="jumpToDiff(marker)"
        ></button>
      </div>

      <article class="string-diff-panel">
        <header class="string-diff-panel-head">
          <span data-emphasis class="string-diff-panel-title">右侧内容</span>
          <span class="string-diff-panel-meta">
            <span v-if="rightParseState" class="string-diff-parse-state">{{
              rightParseState
            }}</span>
            <span class="string-diff-counter">{{ rightText.length }} 字符</span>
          </span>
        </header>
        <div class="string-diff-editor">
          <pre
            ref="rightHighlight"
            class="string-diff-highlight"
            aria-hidden="true"
            v-html="rightHighlightHtml"
          ></pre>
          <textarea
            ref="rightInput"
            v-model="rightText"
            class="string-diff-textarea"
            wrap="off"
            spellcheck="false"
            placeholder="在这里输入 JSON 或字符串"
            @scroll="syncEditorScroll('right', $event)"
          ></textarea>
        </div>
      </article>
    </section>

    <aside
      v-if="resultVisible"
      class="string-diff-result-drawer"
      aria-label="对比结果"
    >
      <header class="string-diff-result-head">
        <div class="string-diff-result-mode">
          <span
            :class="[
              'string-diff-badge',
              { 'string-diff-badge-warning': comparison.warn }
            ]"
          >
            {{ comparison.mode === "待输入" ? "未开始" : comparison.mode }}
          </span>
          <span class="string-diff-parse-info">{{ comparison.info }}</span>
        </div>
        <div class="string-diff-result-actions">
          <button
            class="string-diff-button"
            type="button"
            :disabled="!comparison.rows.length"
            @click="copyResult"
          >
            <Copy :size="15" />
            {{ copyLabel }}
          </button>
          <button
            class="string-diff-icon-button"
            type="button"
            title="关闭差异详情"
            @click="resultVisible = false"
          >
            <X :size="15" />
          </button>
        </div>
      </header>

      <div class="string-diff-drawer-summary" aria-label="统计">
        <article
          v-for="metric in summaryMetrics"
          :key="metric.label"
          class="string-diff-metric"
        >
          <span data-emphasis class="string-diff-metric-value">{{ metric.value }}</span>
          <span class="string-diff-metric-label">{{ metric.label }}</span>
        </article>
      </div>

      <div class="string-diff-result-body">
        <div v-if="!visibleRows.length" class="string-diff-empty">
          {{ comparison.rows.length ? "当前筛选下没有差异" : "暂无结果" }}
        </div>

        <table v-else class="string-diff-table">
          <thead class="string-diff-table-head">
            <tr class="string-diff-table-row">
              <th class="string-diff-key-cell">
                {{ comparison.mode === "JSON" ? "路径" : "行号" }}
              </th>
              <th class="string-diff-status-cell">状态</th>
              <th class="string-diff-value-cell">左侧</th>
              <th class="string-diff-value-cell">右侧</th>
            </tr>
          </thead>
          <tbody class="string-diff-table-body">
            <tr
              v-for="row in visibleRows"
              :key="row.key"
              :class="[
                'string-diff-table-row',
                row.same ? 'string-diff-row-same' : 'string-diff-row-changed'
              ]"
            >
              <td class="string-diff-key-cell">{{ row.key }}</td>
              <td class="string-diff-status-cell">
                <span
                  :class="[
                    'string-diff-status',
                    { 'string-diff-status-same': row.same }
                  ]"
                >
                  {{ row.status }}
                </span>
              </td>
              <td class="string-diff-value-cell">
                <span
                  v-for="(part, index) in row.leftParts"
                  :key="`${row.key}-left-${index}`"
                  :class="{ 'string-diff-mark-left': part.changed }"
                  >{{ part.text }}</span
                >
              </td>
              <td class="string-diff-value-cell">
                <span
                  v-for="(part, index) in row.rightParts"
                  :key="`${row.key}-right-${index}`"
                  :class="{ 'string-diff-mark-right': part.changed }"
                  >{{ part.text }}</span
                >
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </aside>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, ref } from "vue"
import { Copy, PanelRight, Trash2, X } from "lucide-vue-next"
import { createMessage } from "@/utils/message"

const leftInput = ref(null)
const rightInput = ref(null)
const leftHighlight = ref(null)
const rightHighlight = ref(null)
const leftText = ref("")
const rightText = ref("")
const diffOnly = ref(true)
const copyLabel = ref("复制结果")
const resultVisible = ref(false)
const activeDiffKey = ref("")
let copyTimer = null
let syncingScroll = false

const comparison = computed(() => {
  if (!leftText.value.trim() || !rightText.value.trim()) {
    return {
      rows: [],
      mode: "待输入",
      info: "请在左右两侧都输入内容后开始对比。",
      warn: false
    }
  }

  const leftJson = tryParseJson(leftText.value)
  const rightJson = tryParseJson(rightText.value)

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
    rows: buildTextRows(leftText.value, rightText.value),
    mode: "字符串",
    info: warn
      ? "一侧 JSON 解析失败，已按字符串对比。"
      : "已按字符串内容进行对比。",
    warn
  }
})

const visibleRows = computed(() =>
  diffOnly.value
    ? comparison.value.rows.filter((row) => !row.same)
    : comparison.value.rows
)

const editorRows = computed(() => {
  if (!leftText.value.trim() || !rightText.value.trim()) {
    return []
  }

  return buildTextRows(leftText.value, rightText.value)
})

const leftHighlightHtml = computed(() =>
  renderEditorHighlight(leftText.value, editorRows.value, "left")
)

const rightHighlightHtml = computed(() =>
  renderEditorHighlight(rightText.value, editorRows.value, "right")
)

const diffMarkers = computed(() =>
  buildDiffMarkers(editorRows.value, leftText.value, rightText.value)
)

const leftParseState = computed(() => getParseState(leftText.value))
const rightParseState = computed(() => getParseState(rightText.value))

const summaryMetrics = computed(() => {
  const differentCount = comparison.value.rows.filter((row) => !row.same).length
  return [
    { label: "对比项", value: comparison.value.rows.length },
    { label: "差异项", value: differentCount },
    {
      label: "相同项",
      value: comparison.value.rows.length - differentCount
    },
    { label: "当前模式", value: comparison.value.mode }
  ]
})

function clearContent() {
  leftText.value = ""
  rightText.value = ""
  resultVisible.value = false
  activeDiffKey.value = ""
  leftInput.value?.focus()
}

function getParseState(text) {
  if (!text.trim()) {
    return ""
  }

  return tryParseJson(text).ok ? "JSON" : "文本"
}

function escapeHtml(text) {
  return String(text)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;")
}

function renderEditorHighlight(text, rows, side) {
  if (!text) {
    return ""
  }

  const lineKey = side === "left" ? "leftLineNo" : "rightLineNo"
  const changedClass =
    side === "left"
      ? "string-diff-highlight-mark-left"
      : "string-diff-highlight-mark-right"
  const lines = text.split(/\r?\n/)
  const html = lines.map((line, index) => {
    const row = rows.find((item) => item[lineKey] === index + 1)

    if (!row) {
      return escapeHtml(line)
    }

    const parts = side === "left" ? row.leftParts : row.rightParts
    if (!parts.length) {
      return row.same ? "" : `<span class="${changedClass}">&nbsp;</span>`
    }

    return parts
      .map((part) =>
        part.changed
          ? `<span class="${changedClass}">${escapeHtml(part.text)}</span>`
          : `<span class="string-diff-highlight-neutral">${escapeHtml(part.text)}</span>`
      )
      .join("")
  })

  return html.join("\n")
}

function buildDiffMarkers(rows, leftValue, rightValue) {
  if (!leftValue.trim() || !rightValue.trim()) {
    return []
  }

  const totalLines = Math.max(
    leftValue.split(/\r?\n/).length,
    rightValue.split(/\r?\n/).length
  )
  const lastPosition = Math.max(totalLines - 1, 1)

  return rows
    .filter((row) => !row.same && (row.leftLineNo || row.rightLineNo))
    .map((row, index) => {
      const lineNo = Math.max(row.leftLineNo || 0, row.rightLineNo || 0)
      const position = 8 + ((lineNo - 1) / lastPosition) * 84
      return {
        key: `diff-${row.key}-${index}`,
        leftLineNo: row.leftLineNo,
        rightLineNo: row.rightLineNo,
        position,
        label: `差异：${row.key}，${row.status}（左：${compactMarkerValue(
          row.leftText
        )}；右：${compactMarkerValue(row.rightText)}）`
      }
    })
}

function compactMarkerValue(value) {
  const text = String(value || "缺失").replace(/\s+/g, " ")
  return text.length > 54 ? `${text.slice(0, 54)}...` : text
}

function syncEditorScroll(side, event) {
  if (syncingScroll) {
    return
  }

  const source = event.currentTarget
  const target = side === "left" ? rightInput.value : leftInput.value
  const sourceHighlight =
    side === "left" ? leftHighlight.value : rightHighlight.value
  const targetHighlight =
    side === "left" ? rightHighlight.value : leftHighlight.value

  if (!source || !target || !sourceHighlight || !targetHighlight) {
    return
  }

  // 用滚动比例同步，避免左右文本行数不同导致定位偏移。
  syncingScroll = true
  const sourceMax = Math.max(source.scrollHeight - source.clientHeight, 0)
  const targetMax = Math.max(target.scrollHeight - target.clientHeight, 0)
  const progress = sourceMax ? source.scrollTop / sourceMax : 0
  const targetTop = targetMax * progress

  sourceHighlight.scrollTop = source.scrollTop
  sourceHighlight.scrollLeft = source.scrollLeft
  target.scrollTop = targetTop
  target.scrollLeft = source.scrollLeft
  targetHighlight.scrollTop = targetTop
  targetHighlight.scrollLeft = source.scrollLeft

  window.requestAnimationFrame(() => {
    syncingScroll = false
  })
}

function jumpToDiff(marker) {
  if (!leftInput.value || !rightInput.value) {
    return
  }

  activeDiffKey.value = marker.key
  const leftLineNo = marker.leftLineNo || marker.rightLineNo || 1
  const rightLineNo = marker.rightLineNo || marker.leftLineNo || 1
  syncingScroll = true
  scrollInputToLine(leftInput.value, leftHighlight.value, leftLineNo)
  scrollInputToLine(rightInput.value, rightHighlight.value, rightLineNo)
  const focusInput = marker.rightLineNo ? rightInput.value : leftInput.value
  focusInput.focus({ preventScroll: true })
  window.requestAnimationFrame(() => {
    syncingScroll = false
  })
}

function scrollInputToLine(input, highlight, lineNo) {
  if (!input || !highlight || !lineNo) {
    return
  }

  // 将目标行放在编辑区中上方，点击导航后能立即看见上下文。
  const lineHeight =
    Number.parseFloat(window.getComputedStyle(input).lineHeight) || 18
  const targetTop = Math.max(
    0,
    (lineNo - 1) * lineHeight - input.clientHeight * 0.35
  )
  const maxTop = Math.max(0, input.scrollHeight - input.clientHeight)
  const scrollTop = Math.min(targetTop, maxTop)
  input.scrollTop = scrollTop
  highlight.scrollTop = scrollTop
}

async function copyResult() {
  const rows = visibleRows.value
  const lines = [
    `模式: ${comparison.value.mode}`,
    `对比项: ${comparison.value.rows.length}`,
    `差异项: ${comparison.value.rows.filter((row) => !row.same).length}`,
    "",
    ...rows.map((row) =>
      [
        `[${row.status}] ${row.key}`,
        `左: ${row.leftText}`,
        `右: ${row.rightText}`
      ].join("\n")
    )
  ]

  try {
    await navigator.clipboard.writeText(lines.join("\n\n"))
    copyLabel.value = "已复制"
    window.clearTimeout(copyTimer)
    copyTimer = window.setTimeout(() => {
      copyLabel.value = "复制结果"
    }, 900)
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

function tryParseJson(text) {
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

onBeforeUnmount(() => window.clearTimeout(copyTimer))
</script>

<style scoped lang="less">
.string-diff {
  position: relative;
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 10px;
  overflow: hidden;

  .string-diff-toolbar {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 42px;
    padding: 7px 9px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);

    .string-diff-toggle {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      color: var(--color-text-muted);
      cursor: pointer;
      font-size: 0.78rem;

      .string-diff-toggle-label {
        line-height: 1;
      }
    }

    .string-diff-actions {
      display: flex;
      align-items: center;
      gap: 7px;
    }
  }

  .string-diff-button {
    display: inline-flex;
    height: 32px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.76rem;

    &:hover:not(:disabled) {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.48;
    }
  }

  .string-diff-button-primary {
    border-color: var(--color-primary);
    background: var(--color-primary-solid);
    color: #ffffff;

    &:hover:not(:disabled) {
      border-color: var(--color-primary-solid);
      background: var(--color-primary-solid);
    }
  }

  .string-diff-detail-button {
    position: relative;
  }

  .string-diff-detail-button-active {
    border-color: var(--color-success-line);
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .string-diff-detail-count {
    display: inline-flex;
    min-width: 17px;
    height: 17px;
    align-items: center;
    justify-content: center;
    border-radius: 9px;
    background: var(--color-warning-soft);
    color: var(--color-warning);
    font-size: 0.62rem;
    line-height: 1;
  }

  .string-diff-inputs {
    display: grid;
    min-height: 0;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 16px minmax(0, 1fr);
    gap: 10px;

    .string-diff-panel {
      display: flex;
      min-height: 0;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);

      .string-diff-panel-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        min-height: 40px;
        padding: 8px 11px;
        border-bottom: 1px solid var(--color-line);
        background: var(--color-panel-soft);

        .string-diff-panel-title {
          color: var(--color-text);
          font-size: 0.8rem;
        }

        .string-diff-panel-meta {
          display: inline-flex;
          align-items: center;
          gap: 8px;
        }

        .string-diff-parse-state {
          color: var(--color-success);
          font-size: 0.66rem;
        }

        .string-diff-counter {
          flex: none;
          color: var(--color-text-soft);
          font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
          font-size: 0.72rem;
        }
      }

      .string-diff-editor {
        position: relative;
        min-height: 0;
        flex: 1;
        overflow: hidden;

        .string-diff-highlight,
        .string-diff-textarea {
          position: absolute;
          inset: 0;
          box-sizing: border-box;
          width: 100%;
          height: 100%;
          margin: 0;
          padding: 11px;
          border: 0;
          font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
          font-size: 0.76rem;
          line-height: 1.55;
          tab-size: 2;
          white-space: pre;
          overflow-wrap: normal;
          word-break: normal;
        }

        .string-diff-highlight {
          z-index: 0;
          overflow: auto;
          color: transparent;
          pointer-events: none;
          scrollbar-width: none;
        }

        .string-diff-highlight::-webkit-scrollbar {
          display: none;
        }

        .string-diff-highlight {
          :deep(.string-diff-highlight-mark-left),
          :deep(.string-diff-highlight-mark-right) {
            border-radius: 3px;
            color: transparent;
          }

          :deep(.string-diff-highlight-mark-left) {
            background: var(--color-danger-soft);
            box-shadow: inset 0 -2px 0 rgba(180, 35, 24, 0.5);
          }

          :deep(.string-diff-highlight-mark-right) {
            background: var(--color-success-soft);
            box-shadow: inset 0 -2px 0 rgba(23, 128, 61, 0.5);
          }
        }

        .string-diff-textarea {
          z-index: 1;
          display: block;
          resize: none;
          outline: 0;
          background: transparent;
          color: var(--color-text);
          caret-color: var(--color-text);
        }

        .string-diff-textarea::placeholder {
          color: var(--color-text-soft);
        }

        .string-diff-textarea:focus {
          box-shadow: inset 0 0 0 2px
            color-mix(in srgb, var(--color-primary) 24%, transparent);
        }
      }
    }

    .string-diff-navigation {
      position: relative;
      z-index: 3;
      min-height: 0;
      margin: 7px 0;
      border: 1px solid var(--color-line-strong);
      border-radius: 7px;
      background: color-mix(
        in srgb,
        var(--color-panel-soft) 88%,
        transparent
      );

      .string-diff-navigation-dot {
        position: absolute;
        left: 3px;
        width: 8px;
        height: 8px;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: var(--color-danger);
        box-shadow: 0 0 0 1px var(--color-panel);
        cursor: pointer;
        transform: translateY(-50%);
      }

      .string-diff-navigation-dot:hover,
      .string-diff-navigation-dot-active {
        left: 2px;
        width: 10px;
        height: 10px;
        background: var(--color-danger);
        box-shadow: 0 0 0 2px
          color-mix(in srgb, var(--color-danger) 24%, transparent);
      }
    }
  }

  .string-diff-result-drawer {
    position: absolute;
    z-index: 10;
    top: 0;
    right: 0;
    bottom: 0;
    display: flex;
    width: min(680px, calc(100% - 24px));
    min-width: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line-strong);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: -16px 0 42px rgba(31, 52, 69, 0.16);

    .string-diff-result-head {
      display: flex;
      min-height: 48px;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 7px 9px;
      border-bottom: 1px solid var(--color-line);
      background: var(--color-panel-soft);

      .string-diff-result-mode {
        display: flex;
        min-width: 0;
        align-items: center;
        gap: 8px;

        .string-diff-badge {
          display: inline-flex;
          min-height: 22px;
          flex: none;
          align-items: center;
          padding: 0 7px;
          border: 1px solid var(--color-line);
          border-radius: 999px;
          background: var(--color-panel);
          color: var(--color-primary);
          font-size: 0.7rem;
        }

        .string-diff-badge-warning {
          border-color: var(--color-warning-line);
          background: var(--color-warning-soft);
          color: var(--color-warning);
        }

        .string-diff-parse-info {
          overflow: hidden;
          color: var(--color-text-muted);
          font-size: 0.74rem;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }

      .string-diff-result-actions {
        display: flex;
        flex: none;
        align-items: center;
        gap: 6px;
      }

      .string-diff-icon-button {
        display: inline-flex;
        width: 30px;
        height: 30px;
        align-items: center;
        justify-content: center;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
      }

      .string-diff-icon-button:hover {
        border-color: var(--color-line-strong);
        background: var(--color-panel-soft);
        color: var(--color-primary);
      }
    }

    .string-diff-drawer-summary {
      display: grid;
      flex: none;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 8px;
      padding: 9px;
      border-bottom: 1px solid var(--color-line);
      background: var(--color-panel-soft);
    }

    .string-diff-metric {
      min-width: 0;
      min-height: 52px;
      padding: 8px 10px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);

      .string-diff-metric-value {
        display: block;
        overflow: hidden;
        margin-bottom: 4px;
        color: var(--color-primary);
        font-size: 1rem;
        line-height: 1;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .string-diff-metric-label {
        color: var(--color-text-muted);
        font-size: 0.68rem;
      }
    }

    .string-diff-result-body {
      min-height: 0;
      flex: 1;
      overflow: auto;

      .string-diff-empty {
        display: grid;
        min-height: 176px;
        place-items: center;
        color: var(--color-text-muted);
        font-size: 0.8rem;
      }

      .string-diff-table {
        width: 100%;
        min-width: 820px;
        border-collapse: collapse;
        table-layout: fixed;

        .string-diff-table-head {
          .string-diff-table-row {
            .string-diff-key-cell,
            .string-diff-status-cell,
            .string-diff-value-cell {
              position: sticky;
              z-index: 2;
              top: 0;
              padding: 8px 10px;
              border-bottom: 1px solid var(--color-line);
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
              font-size: 0.7rem;
              text-align: left;
            }
          }
        }

        .string-diff-table-body {
          .string-diff-table-row {
            .string-diff-key-cell,
            .string-diff-status-cell,
            .string-diff-value-cell {
              padding: 8px 10px;
              border-bottom: 1px solid var(--color-line);
              vertical-align: top;
              color: var(--color-text);
              font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
              font-size: 0.74rem;
              line-height: 1.5;
              text-align: left;
              white-space: pre-wrap;
              word-break: break-word;
            }

            .string-diff-key-cell {
              color: var(--color-primary);
            }

            .string-diff-status {
              display: inline-flex;
              min-height: 22px;
              align-items: center;
              padding: 0 7px;
              border-radius: 999px;
              background: var(--color-warning-soft);
              color: var(--color-warning);
              font-family: inherit;
              font-size: 0.68rem;
              white-space: nowrap;
            }

            .string-diff-status-same {
              background: var(--color-success-soft);
              color: var(--color-success);
            }

            .string-diff-mark-left,
            .string-diff-mark-right {
              border-radius: 4px;
              padding: 1px 2px;
            }

            .string-diff-mark-left {
              background: var(--color-danger-soft);
              box-shadow: inset 0 -2px 0 rgba(180, 35, 24, 0.58);
            }

            .string-diff-mark-right {
              background: var(--color-success-soft);
              box-shadow: inset 0 -2px 0 rgba(23, 128, 61, 0.58);
            }
          }

          .string-diff-row-changed {
            background: color-mix(
              in srgb,
              var(--color-warning-soft) 46%,
              transparent
            );
          }

          .string-diff-row-same {
            background: color-mix(
              in srgb,
              var(--color-success-soft) 42%,
              transparent
            );
          }
        }

        .string-diff-key-cell {
          width: 180px;
        }

        .string-diff-status-cell {
          width: 96px;
        }
      }
    }
  }
}
</style>

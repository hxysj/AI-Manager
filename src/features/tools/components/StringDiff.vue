<template>
  <section class="string-diff">
    <div class="string-diff-toolbar">
      <label class="string-diff-toggle" title="仅显示不一致的内容">
        <input v-model="diffOnly" type="checkbox" />
        <span class="string-diff-toggle-label">仅差异</span>
      </label>

      <div class="string-diff-actions">
        <button
          class="string-diff-button"
          type="button"
          @click="loadJsonSample"
        >
          <Braces :size="15" />
          JSON 示例
        </button>
        <button
          class="string-diff-button"
          type="button"
          @click="loadTextSample"
        >
          <Text :size="15" />
          文本示例
        </button>
        <button
          class="string-diff-button string-diff-button-primary"
          type="button"
          @click="clearContent"
        >
          <Trash2 :size="15" />
          清空
        </button>
      </div>
    </div>

    <section class="string-diff-inputs" aria-label="输入区">
      <article class="string-diff-panel">
        <header class="string-diff-panel-head">
          <strong class="string-diff-panel-title">左侧内容</strong>
          <span class="string-diff-counter">{{ leftText.length }} 字符</span>
        </header>
        <textarea
          ref="leftInput"
          v-model="leftText"
          class="string-diff-textarea"
          spellcheck="false"
          placeholder="在这里输入 JSON 或字符串"
        ></textarea>
      </article>

      <article class="string-diff-panel">
        <header class="string-diff-panel-head">
          <strong class="string-diff-panel-title">右侧内容</strong>
          <span class="string-diff-counter">{{ rightText.length }} 字符</span>
        </header>
        <textarea
          v-model="rightText"
          class="string-diff-textarea"
          spellcheck="false"
          placeholder="在这里输入 JSON 或字符串"
        ></textarea>
      </article>
    </section>

    <section class="string-diff-summary" aria-label="统计">
      <article
        v-for="metric in summaryMetrics"
        :key="metric.label"
        class="string-diff-metric"
      >
        <strong class="string-diff-metric-value">{{ metric.value }}</strong>
        <span class="string-diff-metric-label">{{ metric.label }}</span>
      </article>
    </section>

    <section class="string-diff-result" aria-label="对比结果">
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
        <button
          class="string-diff-button"
          type="button"
          :disabled="!comparison.rows.length"
          @click="copyResult"
        >
          <Copy :size="15" />
          {{ copyLabel }}
        </button>
      </header>

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
    </section>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, ref } from "vue"
import { Braces, Copy, Text, Trash2 } from "lucide-vue-next"
import { createMessage } from "@/utils/message"

const samples = {
  jsonLeft: {
    name: "订单 A1024",
    status: "paid",
    buyer: { name: "张三", level: 3 },
    items: [
      { sku: "P-100", count: 2, price: 39.9 },
      { sku: "P-208", count: 1, price: 88 }
    ],
    tags: ["new", "vip"]
  },
  jsonRight: {
    name: "订单 A1024",
    status: "refunded",
    buyer: { name: "张三", level: 4 },
    items: [
      { sku: "P-100", count: 2, price: 39.9 },
      { sku: "P-209", count: 1, price: 92 }
    ],
    tags: ["vip", "urgent"],
    remark: "客服已跟进"
  },
  textLeft: "用户ID: 10086\n状态: 已支付\n金额: 128.50\n备注: 周五下午配送",
  textRight:
    "用户ID: 10086\n状态: 已退款\n金额: 126.50\n备注: 周六上午配送\n客服: 李明"
}

const leftInput = ref(null)
const leftText = ref("")
const rightText = ref("")
const diffOnly = ref(true)
const copyLabel = ref("复制结果")
let copyTimer = null

const comparison = computed(() => {
  if (!leftText.value && !rightText.value) {
    return {
      rows: [],
      mode: "待输入",
      info: "左右两侧输入后自动对比。",
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

function loadJsonSample() {
  leftText.value = JSON.stringify(samples.jsonLeft, null, 2)
  rightText.value = JSON.stringify(samples.jsonRight, null, 2)
}

function loadTextSample() {
  leftText.value = samples.textLeft
  rightText.value = samples.textRight
}

function clearContent() {
  leftText.value = ""
  rightText.value = ""
  leftInput.value?.focus()
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
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 10px;
  overflow: auto;

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
    background: #f8fafc;

    .string-diff-toggle {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      color: var(--color-text-muted);
      cursor: pointer;
      font-size: 0.78rem;
      font-weight: 700;

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
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.76rem;
    font-weight: 700;

    &:hover:not(:disabled) {
      border-color: #b9ccda;
      background: #f3f7fb;
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.48;
    }
  }

  .string-diff-button-primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;

    &:hover:not(:disabled) {
      border-color: #263f63;
      background: #263f63;
    }
  }

  .string-diff-inputs {
    display: grid;
    flex: none;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;

    .string-diff-panel {
      overflow: hidden;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: #ffffff;

      .string-diff-panel-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        min-height: 40px;
        padding: 8px 11px;
        border-bottom: 1px solid var(--color-line);
        background: #f8fafc;

        .string-diff-panel-title {
          color: var(--color-text);
          font-size: 0.8rem;
        }

        .string-diff-counter {
          flex: none;
          color: var(--color-text-soft);
          font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
          font-size: 0.72rem;
        }
      }

      .string-diff-textarea {
        display: block;
        width: 100%;
        min-height: 180px;
        resize: vertical;
        border: 0;
        outline: 0;
        padding: 11px;
        background: #ffffff;
        color: var(--color-text);
        font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
        font-size: 0.76rem;
        line-height: 1.55;
        tab-size: 2;

        &:focus {
          box-shadow: inset 0 0 0 2px rgba(47, 95, 145, 0.18);
        }
      }
    }
  }

  .string-diff-summary {
    display: grid;
    flex: none;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;

    .string-diff-metric {
      min-width: 0;
      min-height: 58px;
      padding: 9px 11px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: #ffffff;

      .string-diff-metric-value {
        display: block;
        overflow: hidden;
        margin-bottom: 4px;
        color: var(--color-primary);
        font-size: 1.12rem;
        line-height: 1;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .string-diff-metric-label {
        color: var(--color-text-muted);
        font-size: 0.7rem;
        font-weight: 700;
      }
    }
  }

  .string-diff-result {
    min-height: 220px;
    flex: 1;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;

    .string-diff-result-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      min-height: 42px;
      padding: 7px 9px;
      border-bottom: 1px solid var(--color-line);
      background: #f8fafc;

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
          border: 1px solid #d6e2ec;
          border-radius: 999px;
          background: #ffffff;
          color: var(--color-primary);
          font-size: 0.7rem;
          font-weight: 800;
        }

        .string-diff-badge-warning {
          border-color: #ffe0a3;
          background: #fff6df;
          color: #9a6700;
        }

        .string-diff-parse-info {
          overflow: hidden;
          color: var(--color-text-muted);
          font-size: 0.74rem;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }
    }

    .string-diff-result-body {
      max-height: 390px;
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
              background: #f0f4f8;
              color: #4f6076;
              font-size: 0.7rem;
              font-weight: 800;
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
              font-weight: 700;
            }

            .string-diff-status {
              display: inline-flex;
              min-height: 22px;
              align-items: center;
              padding: 0 7px;
              border-radius: 999px;
              background: #fff6df;
              color: #9a6700;
              font-family: inherit;
              font-size: 0.68rem;
              font-weight: 800;
              white-space: nowrap;
            }

            .string-diff-status-same {
              background: #eaf8ef;
              color: #177a3b;
            }

            .string-diff-mark-left,
            .string-diff-mark-right {
              border-radius: 4px;
              padding: 1px 2px;
            }

            .string-diff-mark-left {
              background: #fdebea;
              box-shadow: inset 0 -2px 0 rgba(180, 35, 24, 0.58);
            }

            .string-diff-mark-right {
              background: #eaf8ef;
              box-shadow: inset 0 -2px 0 rgba(23, 128, 61, 0.58);
            }
          }

          .string-diff-row-changed {
            background: rgba(255, 246, 223, 0.46);
          }

          .string-diff-row-same {
            background: rgba(234, 248, 239, 0.42);
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

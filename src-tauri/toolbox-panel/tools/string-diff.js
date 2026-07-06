export const stringDiffTool = {
  id: 'string-diff',
  name: '差异对比工具',
  shortName: 'DIFF',
  summary: '对比两个字符串或 JSON 内容哪里不同。',
  meta: '文本 / JSON',
  render: renderStringDiffTool
}

const samples = {
  jsonLeft: {
    name: '订单 A1024',
    status: 'paid',
    buyer: { name: '张三', level: 3 },
    items: [
      { sku: 'P-100', count: 2, price: 39.9 },
      { sku: 'P-208', count: 1, price: 88 }
    ],
    tags: ['new', 'vip']
  },
  jsonRight: {
    name: '订单 A1024',
    status: 'refunded',
    buyer: { name: '张三', level: 4 },
    items: [
      { sku: 'P-100', count: 2, price: 39.9 },
      { sku: 'P-209', count: 1, price: 92 }
    ],
    tags: ['vip', 'urgent'],
    remark: '客服已跟进'
  },
  textLeft: '用户ID: 10086\n状态: 已支付\n金额: 128.50\n备注: 周五下午配送',
  textRight: '用户ID: 10086\n状态: 已退款\n金额: 126.50\n备注: 周六上午配送\n客服: 李明'
}

function renderStringDiffTool(container) {
  container.innerHTML = `
    <section class="diff-tool">
      <div class="diff-toolbar">
        <label class="diff-toggle" title="只在结果区显示不一致的内容">
          <input class="diff-only" type="checkbox" checked>
          <span>仅差异</span>
        </label>
        <div class="diff-actions">
          <button class="diff-button diff-sample-json" type="button">JSON 示例</button>
          <button class="diff-button diff-sample-text" type="button">文本示例</button>
          <button class="diff-button diff-button--primary diff-clear" type="button">清空</button>
        </div>
      </div>

      <section class="diff-input-grid" aria-label="输入区">
        <article class="diff-panel">
          <div class="diff-panel__head">
            <div class="diff-panel__title">左侧内容</div>
            <div class="diff-counter diff-left-count">0 字符</div>
          </div>
          <textarea class="diff-textarea diff-left-input" spellcheck="false" placeholder="在这里输入 JSON 或字符串"></textarea>
        </article>

        <article class="diff-panel">
          <div class="diff-panel__head">
            <div class="diff-panel__title">右侧内容</div>
            <div class="diff-counter diff-right-count">0 字符</div>
          </div>
          <textarea class="diff-textarea diff-right-input" spellcheck="false" placeholder="在这里输入 JSON 或字符串"></textarea>
        </article>
      </section>

      <section class="diff-summary" aria-label="统计">
        <div class="diff-metric">
          <strong class="diff-total">0</strong>
          <span>对比项</span>
        </div>
        <div class="diff-metric">
          <strong class="diff-diff-count">0</strong>
          <span>差异项</span>
        </div>
        <div class="diff-metric">
          <strong class="diff-same-count">0</strong>
          <span>相同项</span>
        </div>
        <div class="diff-metric">
          <strong class="diff-mode-text">待输入</strong>
          <span>当前模式</span>
        </div>
      </section>

      <section class="diff-result" aria-label="对比结果">
        <div class="diff-result__head">
          <div class="diff-result__mode">
            <span class="diff-badge diff-mode-badge">未开始</span>
            <span class="diff-parse-info">左右两侧输入后自动对比。</span>
          </div>
          <button class="diff-button diff-copy" type="button">复制结果</button>
        </div>
        <div class="diff-result__body">
          <div class="diff-empty">暂无结果</div>
        </div>
      </section>
    </section>
  `

  const leftInput = container.querySelector('.diff-left-input')
  const rightInput = container.querySelector('.diff-right-input')
  const leftCount = container.querySelector('.diff-left-count')
  const rightCount = container.querySelector('.diff-right-count')
  const totalCount = container.querySelector('.diff-total')
  const diffCount = container.querySelector('.diff-diff-count')
  const sameCount = container.querySelector('.diff-same-count')
  const modeText = container.querySelector('.diff-mode-text')
  const modeBadge = container.querySelector('.diff-mode-badge')
  const parseInfo = container.querySelector('.diff-parse-info')
  const resultBody = container.querySelector('.diff-result__body')
  const diffOnly = container.querySelector('.diff-only')
  const copyResult = container.querySelector('.diff-copy')

  let lastRows = []
  let lastMode = '待输入'

  function renderRows(rows, mode) {
    const visibleRows = diffOnly.checked ? rows.filter(row => !row.same) : rows

    if (!visibleRows.length) {
      resultBody.innerHTML = `<div class="diff-empty">${rows.length ? '当前筛选下没有差异' : '暂无结果'}</div>`
      return
    }

    const firstHead = mode === 'JSON' ? '路径' : '行号'
    const html = `
      <table class="diff-table">
        <thead>
          <tr>
            <th class="diff-cell--key">${firstHead}</th>
            <th class="diff-cell--status">状态</th>
            <th>左侧</th>
            <th>右侧</th>
          </tr>
        </thead>
        <tbody>
          ${visibleRows
            .map(
              row => `
                <tr class="${row.same ? 'diff-row--same' : 'diff-row--changed'}">
                  <td class="diff-cell--key">${escapeHtml(row.key)}</td>
                  <td class="diff-cell--status">
                    <span class="diff-status ${row.same ? 'diff-status--same' : ''}">${escapeHtml(row.status)}</span>
                  </td>
                  <td>${row.leftHtml}</td>
                  <td>${row.rightHtml}</td>
                </tr>
              `
            )
            .join('')}
        </tbody>
      </table>`

    resultBody.innerHTML = html
  }

  function updateSummary(rows, mode, info, isWarn = false) {
    const diffRows = rows.filter(row => !row.same)
    const sameRows = rows.length - diffRows.length
    totalCount.textContent = rows.length
    diffCount.textContent = diffRows.length
    sameCount.textContent = sameRows
    modeText.textContent = mode
    modeBadge.textContent = mode
    modeBadge.classList.toggle('diff-badge--warn', isWarn)
    parseInfo.textContent = info
    lastRows = rows
    lastMode = mode
  }

  // 输入变化后即时重新计算，不需要额外点击确认。
  function compare() {
    const leftText = leftInput.value
    const rightText = rightInput.value
    leftCount.textContent = `${leftText.length} 字符`
    rightCount.textContent = `${rightText.length} 字符`

    if (!leftText && !rightText) {
      updateSummary([], '待输入', '左右两侧输入后自动对比。')
      resultBody.innerHTML = '<div class="diff-empty">暂无结果</div>'
      return
    }

    const leftJson = tryParseJson(leftText)
    const rightJson = tryParseJson(rightText)

    if (leftJson.ok && rightJson.ok) {
      const rows = buildJsonRows(leftJson.value, rightJson.value)
      updateSummary(rows, 'JSON', '已按 JSON 路径和值进行对比。')
      renderRows(rows, 'JSON')
      return
    }

    const rows = buildTextRows(leftText, rightText)
    const hasOneValidJson = leftJson.ok !== rightJson.ok
    const info = hasOneValidJson
      ? '一侧 JSON 解析失败，已按字符串对比。'
      : '已按字符串内容进行对比。'
    updateSummary(rows, '字符串', info, hasOneValidJson)
    renderRows(rows, '字符串')
  }

  async function copyPlainResult() {
    if (!lastRows.length) {
      return
    }

    const visibleRows = diffOnly.checked
      ? lastRows.filter(row => !row.same)
      : lastRows
    const lines = [
      `模式: ${lastMode}`,
      `对比项: ${lastRows.length}`,
      `差异项: ${lastRows.filter(row => !row.same).length}`,
      '',
      ...visibleRows.map(row =>
        [
          `[${row.status}] ${row.key}`,
          `左: ${row.leftText}`,
          `右: ${row.rightText}`
        ].join('\n')
      )
    ]
    await navigator.clipboard.writeText(lines.join('\n\n'))
    copyResult.textContent = '已复制'
    setTimeout(() => {
      copyResult.textContent = '复制结果'
    }, 900)
  }

  leftInput.addEventListener('input', compare)
  rightInput.addEventListener('input', compare)
  diffOnly.addEventListener('change', compare)
  copyResult.addEventListener('click', copyPlainResult)

  container.querySelector('.diff-sample-json').addEventListener('click', () => {
    leftInput.value = JSON.stringify(samples.jsonLeft, null, 2)
    rightInput.value = JSON.stringify(samples.jsonRight, null, 2)
    compare()
  })

  container.querySelector('.diff-sample-text').addEventListener('click', () => {
    leftInput.value = samples.textLeft
    rightInput.value = samples.textRight
    compare()
  })

  container.querySelector('.diff-clear').addEventListener('click', () => {
    leftInput.value = ''
    rightInput.value = ''
    compare()
    leftInput.focus()
  })

  compare()
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;')
}

function tryParseJson(text) {
  const trimmed = text.trim()

  if (!trimmed) {
    return { ok: false, empty: true }
  }

  try {
    return { ok: true, value: JSON.parse(trimmed) }
  } catch (error) {
    return { ok: false, empty: false, message: error.message }
  }
}

function stableStringify(value) {
  if (value === undefined) {
    return ''
  }

  if (value === null || typeof value !== 'object') {
    return JSON.stringify(value)
  }

  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(',')}]`
  }

  const body = Object.keys(value)
    .sort((a, b) => a.localeCompare(b, 'zh-Hans-CN'))
    .map(key => `${JSON.stringify(key)}:${stableStringify(value[key])}`)
    .join(',')
  return `{${body}}`
}

function prettyValue(value) {
  if (value === undefined) {
    return ''
  }

  if (typeof value === 'string') {
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

function flattenJson(value, path = '$', map = new Map()) {
  if (value === null || typeof value !== 'object') {
    map.set(path, {
      value,
      normalized: stableStringify(value),
      display: prettyValue(value)
    })
    return map
  }

  if (Array.isArray(value)) {
    if (value.length === 0) {
      map.set(path, { value, normalized: '[]', display: '[]' })
      return map
    }

    value.forEach((item, index) => flattenJson(item, formatPath(path, index, true), map))
    return map
  }

  const keys = Object.keys(value).sort((a, b) => a.localeCompare(b, 'zh-Hans-CN'))

  if (keys.length === 0) {
    map.set(path, { value, normalized: '{}', display: '{}' })
    return map
  }

  keys.forEach(key => flattenJson(value[key], formatPath(path, key, false), map))
  return map
}

function lcsPairs(left, right) {
  const rows = left.length + 1
  const cols = right.length + 1
  const dp = Array.from({ length: rows }, () => Array(cols).fill(0))

  for (let i = left.length - 1; i >= 0; i -= 1) {
    for (let j = right.length - 1; j >= 0; j -= 1) {
      dp[i][j] =
        left[i] === right[j]
          ? dp[i + 1][j + 1] + 1
          : Math.max(dp[i + 1][j], dp[i][j + 1])
    }
  }

  const pairs = []
  let i = 0
  let j = 0

  while (i < left.length && j < right.length) {
    if (left[i] === right[j]) {
      pairs.push([i, j])
      i += 1
      j += 1
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      i += 1
    } else {
      j += 1
    }
  }

  return pairs
}

function tokenizeText(text) {
  const tokens = []
  const pattern = /[\u4e00-\u9fa5]|[A-Za-z0-9_]+|\s+|./gu

  for (const match of text.matchAll(pattern)) {
    tokens.push(match[0])
  }

  return tokens
}

function inlineDiff(left, right) {
  if (left === right) {
    const safe = escapeHtml(left)
    return { leftHtml: safe, rightHtml: safe }
  }

  const leftTokens = tokenizeText(left)
  const rightTokens = tokenizeText(right)
  const leftKeep = new Set()
  const rightKeep = new Set()

  lcsPairs(leftTokens, rightTokens).forEach(([leftIndex, rightIndex]) => {
    leftKeep.add(leftIndex)
    rightKeep.add(rightIndex)
  })

  const leftHtml = leftTokens
    .map((token, index) =>
      leftKeep.has(index)
        ? escapeHtml(token)
        : `<span class="diff-mark-left">${escapeHtml(token)}</span>`
    )
    .join('')
  const rightHtml = rightTokens
    .map((token, index) =>
      rightKeep.has(index)
        ? escapeHtml(token)
        : `<span class="diff-mark-right">${escapeHtml(token)}</span>`
    )
    .join('')

  return { leftHtml, rightHtml }
}

function buildJsonRows(leftValue, rightValue) {
  const leftMap = flattenJson(leftValue)
  const rightMap = flattenJson(rightValue)
  const paths = Array.from(new Set([...leftMap.keys(), ...rightMap.keys()])).sort((a, b) =>
    a.localeCompare(b, 'zh-Hans-CN', { numeric: true })
  )

  return paths.map(path => {
    const leftItem = leftMap.get(path)
    const rightItem = rightMap.get(path)
    const hasLeft = Boolean(leftItem)
    const hasRight = Boolean(rightItem)
    const same = hasLeft && hasRight && leftItem.normalized === rightItem.normalized
    let status = '不同'

    if (same) {
      status = '相同'
    } else if (!hasLeft) {
      status = '右侧新增'
    } else if (!hasRight) {
      status = '左侧独有'
    }

    return {
      key: path,
      status,
      same,
      leftText: hasLeft ? leftItem.display : '',
      rightText: hasRight ? rightItem.display : '',
      leftHtml: same
        ? escapeHtml(leftItem.display)
        : `<span class="diff-mark-left">${escapeHtml(hasLeft ? leftItem.display : '缺失')}</span>`,
      rightHtml: same
        ? escapeHtml(rightItem.display)
        : `<span class="diff-mark-right">${escapeHtml(hasRight ? rightItem.display : '缺失')}</span>`
    }
  })
}

function buildTextRows(leftText, rightText) {
  const leftLines = leftText.split(/\r?\n/)
  const rightLines = rightText.split(/\r?\n/)
  const pairs = lcsPairs(leftLines, rightLines)
  const rows = []
  let leftIndex = 0
  let rightIndex = 0

  function pushChanged(currentLeft, currentRight, leftNo, rightNo, status) {
    const leftValue = currentLeft ?? ''
    const rightValue = currentRight ?? ''
    const same = leftValue === rightValue
    const diff = inlineDiff(leftValue, rightValue)
    rows.push({
      key: `${leftNo || '-'} / ${rightNo || '-'}`,
      status: same ? '相同' : status,
      same,
      leftText: leftValue,
      rightText: rightValue,
      leftHtml:
        diff.leftHtml ||
        (currentLeft === undefined ? '<span class="diff-mark-left">缺失</span>' : ''),
      rightHtml:
        diff.rightHtml ||
        (currentRight === undefined ? '<span class="diff-mark-right">缺失</span>' : '')
    })
  }

  pairs.forEach(([nextLeft, nextRight]) => {
    while (leftIndex < nextLeft || rightIndex < nextRight) {
      if (leftIndex < nextLeft && rightIndex < nextRight) {
        pushChanged(leftLines[leftIndex], rightLines[rightIndex], leftIndex + 1, rightIndex + 1, '不同')
        leftIndex += 1
        rightIndex += 1
      } else if (leftIndex < nextLeft) {
        pushChanged(leftLines[leftIndex], undefined, leftIndex + 1, '', '左侧独有')
        leftIndex += 1
      } else {
        pushChanged(undefined, rightLines[rightIndex], '', rightIndex + 1, '右侧新增')
        rightIndex += 1
      }
    }

    pushChanged(leftLines[nextLeft], rightLines[nextRight], nextLeft + 1, nextRight + 1, '相同')
    leftIndex = nextLeft + 1
    rightIndex = nextRight + 1
  })

  while (leftIndex < leftLines.length || rightIndex < rightLines.length) {
    if (leftIndex < leftLines.length && rightIndex < rightLines.length) {
      pushChanged(leftLines[leftIndex], rightLines[rightIndex], leftIndex + 1, rightIndex + 1, '不同')
      leftIndex += 1
      rightIndex += 1
    } else if (leftIndex < leftLines.length) {
      pushChanged(leftLines[leftIndex], undefined, leftIndex + 1, '', '左侧独有')
      leftIndex += 1
    } else {
      pushChanged(undefined, rightLines[rightIndex], '', rightIndex + 1, '右侧新增')
      rightIndex += 1
    }
  }

  return rows
}

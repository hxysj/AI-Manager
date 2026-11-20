const statusLabels = {
  installed: "已安装",
  "not-installed": "未安装",
  "broken-link": "链接损坏",
  disabled: "不可用"
}

export function formatDateTime(value) {
  if (!value) {
    return "未记录"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(value))
}

export function formatStatusLabel(value) {
  return statusLabels[value] || value || "未知"
}

export function formatServiceProviderName(provider, state = {}) {
  const name = provider.name?.trim()
  if (provider.kind === "api") return name || provider.id

  const isGoogle = provider.kind === "google_account"
  const accounts = isGoogle ? state.googleAccounts : state.accounts
  const account = accounts?.find((item) => item.providerId === provider.id)
  const identities = [provider.id, account?.accountId, account?.subject]
  if (name && !identities.includes(name)) return name

  return (
    account?.metadata?.name ||
    account?.name ||
    account?.email ||
    (isGoogle ? "Google 官方账号" : "Codex 官方账号")
  )
}

// Google 的额度按共享模型分组，同一组内按周期从短到长展示。
export function formatGoogleQuotaGroups(groups) {
  return (groups || [])
    .map((group, groupIndex) => ({
      id: groupIndex,
      name: group.displayName || `共享额度 ${groupIndex + 1}`,
      windows: (group.buckets || [])
        .map((bucket, index) => {
          const window = String(bucket.window || "")
            .replace(/^WINDOW_/i, "")
            .toLowerCase()
          const period = { "5h": 18000, weekly: 604800 }[window]
          return {
            id: index,
            name:
              { "5h": "5 小时额度", weekly: "周额度" }[window] ||
              bucket.displayName ||
              bucket.window ||
              "额度",
            order: period || Number.MAX_SAFE_INTEGER,
            remaining: Number.isFinite(bucket.remainingFraction)
              ? Math.round(
                  Math.max(0, Math.min(1, bucket.remainingFraction)) * 1000
                ) / 10
              : null,
            resetAt: Date.parse(bucket.resetTime) || null
          }
        })
        .sort((left, right) => left.order - right.order)
    }))
    .filter((group) => group.windows.length)
}

export function formatCount(value, singularLabel, pluralLabel = singularLabel) {
  const count = Number(value || 0)
  const label = count === 1 ? singularLabel : pluralLabel
  return `${count} ${label}`
}

export function iconLetters(value) {
  return (
    String(value || "")
      .replace(/[^a-zA-Z0-9]/g, "")
      .slice(0, 2)
      .toUpperCase() || "AI"
  )
}

export function hashColor(value) {
  const source = String(value || "ai-manager")
  let seed = 0

  for (let index = 0; index < source.length; index += 1) {
    seed = source.charCodeAt(index) + ((seed << 5) - seed)
  }

  const hue = Math.abs(seed % 360)

  return `hsl(${hue} 24% 86%)`
}

// Token 数量超过千和百万时使用紧凑单位，避免占满统计卡片和表格列。
export function formatTokenCount(value) {
  const { compact, exact } = formatTokenCountParts(value)

  return exact ? `${compact}（${exact}）` : compact
}

export function formatTokenCountParts(value) {
  if (value === null) return { compact: "未知", exact: "" }
  const tokenCount = Number(value || 0)

  if (!Number.isFinite(tokenCount)) {
    return {
      compact: "0",
      exact: ""
    }
  }

  const absoluteCount = Math.abs(tokenCount)
  const exactCount = new Intl.NumberFormat("zh-CN", {
    maximumFractionDigits: 20
  }).format(tokenCount)

  if (absoluteCount >= 1000000) {
    return {
      compact: `${(tokenCount / 1000000).toFixed(2)}M`,
      exact: exactCount
    }
  }

  if (absoluteCount >= 1000) {
    if (absoluteCount >= 999950) {
      return {
        compact: `${(tokenCount / 1000000).toFixed(2)}M`,
        exact: exactCount
      }
    }

    return {
      compact: `${(tokenCount / 1000).toFixed(2)}k`,
      exact: exactCount
    }
  }

  return {
    compact: exactCount,
    exact: ""
  }
}

const fs = require("node:fs/promises")
const path = require("node:path")
const crypto = require("node:crypto")

const DEFAULT_EXCHANGE_RATE = 7.2

function toNumber(value) {
  const number = Number(value || 0)
  return Number.isFinite(number) ? Math.max(0, Math.floor(number)) : 0
}

function toPriceNumber(value) {
  const number = Number(value || 0)
  return Number.isFinite(number) ? Math.max(0, number) : 0
}

function toTimestamp(value, fallback) {
  const timestamp = value ? new Date(value).getTime() : 0
  return Number.isFinite(timestamp) && timestamp > 0 ? timestamp : fallback
}

function createHashId(parts) {
  return crypto.createHash("sha1").update(parts.join("|")).digest("hex")
}

function normalizeAppType(value) {
  return String(value || "")
    .trim()
    .toLowerCase()
}

function normalizeCurrency(value) {
  return String(value || "").toUpperCase() === "CNY" ? "CNY" : "USD"
}

function normalizeModelCategory(value) {
  return String(value || "").trim()
}

function formatAppProviderName(cli) {
  const names = {
    claude: "Claude",
    codex: "Codex",
    gemini: "Gemini"
  }

  return names[cli] || cli || "未知 CLI"
}

function normalizePricingItem(input) {
  const modelId = String(input.modelId || "").trim()

  if (!modelId) {
    throw new Error("模型名称不能为空")
  }

  return {
    id: input.id || `pricing-${crypto.randomUUID()}`,
    modelId,
    modelCategory: normalizeModelCategory(
      input.modelCategory ?? input.category
    ),
    currency: normalizeCurrency(input.currency),
    inputCostPerMillion: toPriceNumber(input.inputCostPerMillion),
    outputCostPerMillion: toPriceNumber(input.outputCostPerMillion),
    cacheReadCostPerMillion: toPriceNumber(input.cacheReadCostPerMillion),
    cacheCreationCostPerMillion: toPriceNumber(
      input.cacheCreationCostPerMillion
    )
  }
}

function normalizePricingConfig(input = {}) {
  const exchangeRate = toPriceNumber(
    input.exchangeRate || DEFAULT_EXCHANGE_RATE
  )

  if (exchangeRate <= 0) {
    throw new Error("汇率必须大于 0")
  }

  return {
    exchangeRate,
    items: (Array.isArray(input.items) ? input.items : []).map((item) =>
      normalizePricingItem(item)
    )
  }
}

function getUsageValue(usage, keys) {
  for (const key of keys) {
    if (usage?.[key] !== undefined) {
      return toNumber(usage[key])
    }
  }

  return 0
}

function normalizeBillableInput(log) {
  if (log.appType === "codex" || log.appType === "gemini") {
    return Math.max(0, log.inputTokens - log.cacheReadTokens)
  }

  return log.inputTokens
}

function toActualTokens(log) {
  return (
    normalizeBillableInput(log) +
    log.outputTokens +
    log.cacheReadTokens +
    log.cacheCreationTokens
  )
}

function findModelPricing(log, pricingConfig) {
  const modelKeys = [log.model, log.requestModel]
    .map((item) =>
      String(item || "")
        .trim()
        .toLowerCase()
    )
    .filter(Boolean)

  return pricingConfig.items.find((item) =>
    modelKeys.includes(item.modelId.toLowerCase())
  )
}

function priceToUsd(value, currency, exchangeRate) {
  return currency === "CNY" ? value / exchangeRate : value
}

function calculateCostUsd(log, pricingConfig) {
  const pricing = findModelPricing(log, pricingConfig)

  if (!pricing) {
    return {
      inputCostUsd: 0,
      outputCostUsd: 0,
      cacheReadCostUsd: 0,
      cacheCreationCostUsd: 0,
      totalCostUsd: 0
    }
  }

  const inputCostUsd =
    (normalizeBillableInput(log) *
      priceToUsd(
        pricing.inputCostPerMillion,
        pricing.currency,
        pricingConfig.exchangeRate
      )) /
    1000000
  const outputCostUsd =
    (log.outputTokens *
      priceToUsd(
        pricing.outputCostPerMillion,
        pricing.currency,
        pricingConfig.exchangeRate
      )) /
    1000000
  const cacheReadCostUsd =
    (log.cacheReadTokens *
      priceToUsd(
        pricing.cacheReadCostPerMillion,
        pricing.currency,
        pricingConfig.exchangeRate
      )) /
    1000000
  const cacheCreationCostUsd =
    (log.cacheCreationTokens *
      priceToUsd(
        pricing.cacheCreationCostPerMillion,
        pricing.currency,
        pricingConfig.exchangeRate
      )) /
    1000000

  return {
    inputCostUsd,
    outputCostUsd,
    cacheReadCostUsd,
    cacheCreationCostUsd,
    totalCostUsd:
      inputCostUsd + outputCostUsd + cacheReadCostUsd + cacheCreationCostUsd
  }
}

function enrichUsageLog(log, pricingConfig) {
  const sourceLog = {
    ...log,
    providerId: log.providerId || log.appType,
    providerName: log.providerName || formatAppProviderName(log.appType),
    providerType: log.providerType || ""
  }

  return {
    ...sourceLog,
    actualTokens: toActualTokens(sourceLog),
    ...calculateCostUsd(sourceLog, pricingConfig)
  }
}

function createEmptySummary() {
  return {
    requestCount: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    actualTokens: 0,
    cacheHitRate: 0,
    totalCostUsd: 0,
    lastUsedAt: 0
  }
}

function appendSummary(summary, log) {
  summary.requestCount += 1
  summary.inputTokens += normalizeBillableInput(log)
  summary.outputTokens += log.outputTokens
  summary.cacheReadTokens += log.cacheReadTokens
  summary.cacheCreationTokens += log.cacheCreationTokens
  summary.actualTokens += toActualTokens(log)
  summary.totalCostUsd += log.totalCostUsd
  summary.lastUsedAt = Math.max(summary.lastUsedAt, log.createdAt)
}

function finalizeSummary(summary) {
  const cacheBase =
    summary.inputTokens + summary.cacheReadTokens + summary.cacheCreationTokens
  summary.cacheHitRate = cacheBase
    ? Number((summary.cacheReadTokens / cacheBase).toFixed(4))
    : 0
  summary.totalCostUsd = Number(summary.totalCostUsd.toFixed(8))
  return summary
}

function resolveProvider(cli, input = {}) {
  const proxyState =
    input.proxyStates?.[`${cli}ProxyState`] ||
    (cli === "codex" ? input.codexProxyState : null)
  const proxyTargetId = proxyState?.enabled ? proxyState.activeProviderId : ""
  const proxyAccountId = String(proxyTargetId || "").startsWith("account:")
    ? String(proxyTargetId).slice("account:".length)
    : ""
  const proxyAccount = (input.codexAccounts || []).find(
    (item) => item.id === proxyAccountId
  )
  const proxyProvider = (input.providers || []).find(
    (item) => item.id === proxyTargetId
  )

  if (cli === "codex" && proxyAccount) {
    return {
      providerId: proxyTargetId,
      providerName:
        proxyAccount.email || proxyAccount.accountId || "Codex 官方账号",
      providerType: "codex"
    }
  }

  if (proxyProvider && proxyProvider.cli === cli) {
    return {
      providerId: proxyProvider.id,
      providerName: proxyProvider.name,
      providerType: proxyProvider.type
    }
  }

  const activeCodexAccount = (input.codexAccounts || []).find(
    (item) => item.active
  )

  if (cli === "codex" && activeCodexAccount) {
    return {
      providerId: `codex-account:${activeCodexAccount.id}`,
      providerName:
        activeCodexAccount.email ||
        activeCodexAccount.accountId ||
        "Codex 官方账号",
      providerType: "codex"
    }
  }

  const profile = (input.runtimeProfiles || []).find((item) => item.cli === cli)
  const providerId =
    profile?.providerId || input.runtimeProviderState?.[cli]?.activeProviderId
  const provider = (input.providers || []).find(
    (item) => item.id === providerId
  )

  if (provider && provider.cli === cli && provider.enabled !== false) {
    return {
      providerId: provider.id,
      providerName: provider.name,
      providerType: provider.type
    }
  }

  return {
    providerId: cli,
    providerName: formatAppProviderName(cli),
    providerType: ""
  }
}

function createUsageLog(session, providerInfo, input) {
  const inputTokens = toNumber(input.inputTokens)
  const outputTokens = toNumber(input.outputTokens)
  const cacheReadTokens = toNumber(input.cacheReadTokens)
  const cacheCreationTokens = toNumber(input.cacheCreationTokens)

  return {
    requestId: input.requestId,
    providerId: providerInfo.providerId,
    providerName: providerInfo.providerName,
    providerType: providerInfo.providerType,
    appType: session.cli,
    model: input.model || session.model || "",
    requestModel: input.requestModel || input.model || session.model || "",
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheCreationTokens,
    inputCostUsd: 0,
    outputCostUsd: 0,
    cacheReadCostUsd: 0,
    cacheCreationCostUsd: 0,
    totalCostUsd: 0,
    statusCode: 200,
    errorMessage: "",
    sessionId: session.id,
    sessionTitle: session.title,
    projectName: session.projectName || "",
    rawPath: session.rawPath,
    dataSource: input.dataSource,
    requestSource: input.requestSource || session.requestSource || "",
    instanceProviderId:
      input.instanceProviderId || session.instanceProviderId || "",
    instanceProviderName:
      input.instanceProviderName || session.instanceProviderName || "",
    instanceProviderType:
      input.instanceProviderType || session.instanceProviderType || "",
    createdAt: input.createdAt
  }
}

function createLogProviderInfo(log) {
  return {
    providerId: log.providerId || log.appType,
    providerName: log.providerName || formatAppProviderName(log.appType),
    providerType: log.providerType || ""
  }
}

function createSessionProviderInfo(session, fallback) {
  if (session.requestSource === "provider-instance") {
    return {
      providerId: session.instanceProviderId,
      providerName: session.instanceProviderName,
      providerType: session.instanceProviderType || fallback.providerType || ""
    }
  }

  return fallback
}

function createRequestRecord(log, providerInfo) {
  return {
    requestId: log.requestId,
    providerId: providerInfo.providerId,
    providerName: providerInfo.providerName,
    providerType: providerInfo.providerType || "",
    appType: log.appType,
    model: log.model || "",
    requestModel: log.requestModel || log.model || "",
    inputTokens: log.inputTokens,
    outputTokens: log.outputTokens,
    cacheReadTokens: log.cacheReadTokens,
    cacheCreationTokens: log.cacheCreationTokens,
    actualTokens: toActualTokens(log),
    dataSource: log.dataSource,
    sessionId: log.sessionId,
    sessionTitle: log.sessionTitle || "",
    projectName: log.projectName || "",
    rawPath: log.rawPath || "",
    requestSource: log.requestSource || "",
    instanceProviderId: log.instanceProviderId || "",
    instanceProviderName: log.instanceProviderName || "",
    instanceProviderType: log.instanceProviderType || "",
    requestTime: log.createdAt,
    createdAt: log.createdAt
  }
}

function applyRequestRecord(log, record) {
  return {
    ...log,
    providerId: record.providerId,
    providerName: record.providerName,
    providerType: record.providerType || "",
    requestSource: record.requestSource || "",
    instanceProviderId: record.instanceProviderId || "",
    instanceProviderName: record.instanceProviderName || "",
    instanceProviderType: record.instanceProviderType || ""
  }
}

function extractClaudeLogs(session, content, providerInfo) {
  const logs = []

  for (const line of content.split(/\r?\n/)) {
    const text = line.trim()

    if (!text) {
      continue
    }

    const record = JSON.parse(text)
    const message = record.message || record.payload?.message
    const usage = message?.usage

    if (
      record.type !== "assistant" ||
      !usage ||
      !message.stop_reason ||
      toNumber(usage.output_tokens) <= 0
    ) {
      continue
    }

    const messageId = message.id || record.uuid
    logs.push(
      createUsageLog(session, providerInfo, {
        requestId: messageId
          ? `session:${messageId}`
          : `session:${createHashId([session.id, record.timestamp, logs.length])}`,
        model: message.model,
        inputTokens: usage.input_tokens,
        outputTokens: usage.output_tokens,
        cacheReadTokens: usage.cache_read_input_tokens,
        cacheCreationTokens:
          usage.cache_creation_input_tokens ||
          usage.cache_creation?.ephemeral_1h_input_tokens ||
          usage.cache_creation?.ephemeral_5m_input_tokens,
        dataSource: "session_log",
        createdAt: toTimestamp(record.timestamp, session.updatedAt)
      })
    )
  }

  return logs
}

function subtractTokenUsage(current, previous) {
  if (!previous) {
    return current
  }

  return {
    input_tokens:
      toNumber(current.input_tokens) - toNumber(previous.input_tokens),
    cached_input_tokens:
      toNumber(current.cached_input_tokens) -
      toNumber(previous.cached_input_tokens),
    output_tokens:
      toNumber(current.output_tokens) - toNumber(previous.output_tokens)
  }
}

function normalizeCodexTokenUsage(usage) {
  return {
    input_tokens: toNumber(usage?.input_tokens),
    cached_input_tokens: toNumber(usage?.cached_input_tokens),
    output_tokens: toNumber(usage?.output_tokens)
  }
}

function isValidCodexDelta(delta) {
  return (
    delta.input_tokens >= 0 &&
    delta.cached_input_tokens >= 0 &&
    delta.output_tokens >= 0 &&
    delta.input_tokens + delta.output_tokens > 0
  )
}

function extractCodexLogs(session, content, providerInfo) {
  const logs = []
  let model = session.model || ""
  let previousTotalUsage = null

  for (const line of content.split(/\r?\n/)) {
    const text = line.trim()

    if (!text) {
      continue
    }

    const record = JSON.parse(text)
    const payload = record.payload || record

    if (payload.model) {
      model = payload.model
    }

    if (payload.type === "session_meta") {
      model = payload.model || payload.metadata?.model || model
      continue
    }

    if (payload.type !== "token_count") {
      continue
    }

    const info = payload.info || {}
    const totalUsage = normalizeCodexTokenUsage(info.total_token_usage)
    const lastUsage = normalizeCodexTokenUsage(info.last_token_usage)
    const delta = subtractTokenUsage(totalUsage, previousTotalUsage)
    const usage = isValidCodexDelta(delta) ? delta : lastUsage

    previousTotalUsage = totalUsage

    if (!isValidCodexDelta(usage)) {
      continue
    }

    logs.push(
      createUsageLog(session, providerInfo, {
        requestId: `codex:${createHashId([
          session.id,
          record.timestamp,
          logs.length,
          usage.input_tokens,
          usage.output_tokens
        ])}`,
        model,
        inputTokens: usage.input_tokens,
        outputTokens: usage.output_tokens,
        cacheReadTokens: usage.cached_input_tokens,
        cacheCreationTokens: 0,
        dataSource: "codex_session",
        createdAt: toTimestamp(record.timestamp, session.updatedAt)
      })
    )
  }

  return logs
}

function collectGeminiUsageItems(source, output) {
  if (!source || typeof source !== "object") {
    return
  }

  if (source.usageMetadata) {
    output.push(source)
  }

  if (Array.isArray(source)) {
    for (const item of source) {
      collectGeminiUsageItems(item, output)
    }
    return
  }

  for (const value of Object.values(source)) {
    collectGeminiUsageItems(value, output)
  }
}

function extractGeminiLogs(session, content, providerInfo) {
  const payload = JSON.parse(content)
  const items = []
  const logs = []

  collectGeminiUsageItems(payload, items)

  for (const item of items) {
    const usage = item.usageMetadata
    const inputTokens = getUsageValue(usage, ["promptTokenCount"])
    const totalTokens = getUsageValue(usage, ["totalTokenCount"])
    const outputTokens = Math.max(0, totalTokens - inputTokens)

    if (inputTokens + outputTokens <= 0) {
      continue
    }

    logs.push(
      createUsageLog(session, providerInfo, {
        requestId: `gemini:${createHashId([
          session.id,
          logs.length,
          inputTokens,
          outputTokens,
          item.createTime || item.timestamp || ""
        ])}`,
        model: item.model || payload.model || session.model || "",
        inputTokens,
        outputTokens,
        cacheReadTokens: getUsageValue(usage, ["cachedContentTokenCount"]),
        cacheCreationTokens: 0,
        dataSource: "gemini_session",
        createdAt: toTimestamp(
          item.createTime || item.timestamp || payload.updatedAt,
          session.updatedAt
        )
      })
    )
  }

  return logs
}

function inRange(log, filters) {
  if (
    filters.appType &&
    filters.appType !== "all" &&
    log.appType !== filters.appType
  ) {
    return false
  }

  if (
    filters.providerId &&
    filters.providerId !== "all" &&
    log.providerId !== filters.providerId
  ) {
    return false
  }

  if (filters.model && filters.model !== "all" && log.model !== filters.model) {
    return false
  }

  if (
    filters.requestSource &&
    filters.requestSource !== "all" &&
    (log.requestSource || "session") !== filters.requestSource
  ) {
    return false
  }

  if (filters.startAt && log.createdAt < filters.startAt) {
    return false
  }

  if (filters.endAt && log.createdAt > filters.endAt) {
    return false
  }

  return true
}

function createGroupStats(logs, keySelector, baseSelector) {
  const groups = new Map()

  for (const log of logs) {
    const key = keySelector(log) || "unknown"

    if (!groups.has(key)) {
      groups.set(key, {
        ...baseSelector(log),
        ...createEmptySummary()
      })
    }

    appendSummary(groups.get(key), log)
  }

  return Array.from(groups.values())
    .map((item) => finalizeSummary(item))
    .sort((left, right) => right.actualTokens - left.actualTokens)
}

function createTrendStats(logs, trendMode = "day", filters = {}) {
  const groups = new Map()
  const isSingleDay =
    filters.startAt &&
    filters.endAt &&
    new Date(filters.startAt).toDateString() ===
      new Date(filters.endAt).toDateString()

  if (trendMode === "hour") {
    if (isSingleDay) {
      for (let hour = 0; hour < 24; hour += 1) {
        const label = `${String(hour).padStart(2, "0")}:00`

        groups.set(label, {
          date: label,
          sortAt: hour,
          ...createEmptySummary()
        })
      }
    }

    for (const log of logs) {
      const date = new Date(log.createdAt)
      const hour = String(date.getHours()).padStart(2, "0")
      const day = date.toLocaleDateString("zh-CN")
      const label = isSingleDay ? `${hour}:00` : `${day} ${hour}:00`

      if (!groups.has(label)) {
        groups.set(label, {
          date: label,
          sortAt: new Date(
            date.getFullYear(),
            date.getMonth(),
            date.getDate(),
            date.getHours()
          ).getTime(),
          ...createEmptySummary()
        })
      }

      appendSummary(groups.get(label), log)
    }

    return Array.from(groups.values())
      .sort((left, right) => left.sortAt - right.sortAt)
      .map(({ sortAt, ...item }) => finalizeSummary(item))
  }

  if (trendMode === "minute") {
    if (isSingleDay) {
      for (let hour = 0; hour < 24; hour += 1) {
        for (let minute = 0; minute < 60; minute += 1) {
          const label = `${String(hour).padStart(2, "0")}:${String(
            minute
          ).padStart(2, "0")}`

          groups.set(label, {
            date: label,
            sortAt: hour * 60 + minute,
            ...createEmptySummary()
          })
        }
      }
    }

    for (const log of logs) {
      const date = new Date(log.createdAt)
      const hour = String(date.getHours()).padStart(2, "0")
      const minute = String(date.getMinutes()).padStart(2, "0")
      const day = date.toLocaleDateString("zh-CN")
      const label = isSingleDay ? `${hour}:${minute}` : `${day} ${hour}:${minute}`

      if (!groups.has(label)) {
        groups.set(label, {
          date: label,
          sortAt: new Date(
            date.getFullYear(),
            date.getMonth(),
            date.getDate(),
            date.getHours(),
            date.getMinutes()
          ).getTime(),
          ...createEmptySummary()
        })
      }

      appendSummary(groups.get(label), log)
    }

    return Array.from(groups.values())
      .sort((left, right) => left.sortAt - right.sortAt)
      .map(({ sortAt, ...item }) => finalizeSummary(item))
  }

  for (const log of logs) {
    const date = new Date(log.createdAt).toLocaleDateString("zh-CN")

    if (!groups.has(date)) {
      groups.set(date, {
        date,
        ...createEmptySummary()
      })
    }

    appendSummary(groups.get(date), log)
  }

  return Array.from(groups.values())
    .map((item) => finalizeSummary(item))
    .sort((left, right) => new Date(left.date) - new Date(right.date))
}

class UsageService {
  constructor() {
    this.storage = null
    this.logs = []
    this.requestRecords = []
    this.pricingConfig = normalizePricingConfig()
  }

  bindStorage(storage) {
    this.storage = storage
  }

  async init() {
    this.logs = await this.storage.read("usageLogs", [])
    this.requestRecords = await this.storage.read("usageRequestRecords", [])
    this.pricingConfig = normalizePricingConfig(
      await this.storage.read("usagePricing", normalizePricingConfig())
    )

    const recordMap = new Map(
      this.requestRecords.map((item) => [item.requestId, item])
    )

    for (const log of this.logs) {
      if (!recordMap.has(log.requestId)) {
        recordMap.set(
          log.requestId,
          createRequestRecord(log, createLogProviderInfo(log))
        )
      }
    }

    this.logs = this.logs.map((log) =>
      applyRequestRecord(log, recordMap.get(log.requestId))
    )
    this.requestRecords = Array.from(recordMap.values()).sort(
      (left, right) => right.createdAt - left.createdAt
    )
    this.storage.scheduleWrite("usageLogs", this.logs)
    this.storage.scheduleWrite("usageRequestRecords", this.requestRecords)
  }

  async refresh(input) {
    const logs = []
    const diagnostics = []
    const recordMap = new Map(
      this.requestRecords.map((item) => [item.requestId, item])
    )

    for (const log of this.logs) {
      if (!recordMap.has(log.requestId)) {
        recordMap.set(
          log.requestId,
          createRequestRecord(log, createLogProviderInfo(log))
        )
      }
    }

    for (const session of input.sessions || []) {
      const appType = normalizeAppType(session.cli)

      if (
        !["claude", "codex", "gemini"].includes(appType) ||
        !session.rawPath
      ) {
        continue
      }

      try {
        const requestSource =
          session.requestSource ||
          (input.proxyStates?.[`${appType}ProxyState`]?.enabled
            ? "proxy-managed"
            : "")
        const usageSession = {
          ...session,
          requestSource
        }
        const providerInfo = createSessionProviderInfo(
          usageSession,
          resolveProvider(appType, input)
        )
        const content = await fs.readFile(usageSession.rawPath, "utf8")
        const extension = path.extname(usageSession.rawPath).toLowerCase()

        if (appType === "claude") {
          logs.push(...extractClaudeLogs(usageSession, content, providerInfo))
        } else if (appType === "codex" && extension !== ".json") {
          logs.push(...extractCodexLogs(usageSession, content, providerInfo))
        } else if (appType === "gemini") {
          logs.push(...extractGeminiLogs(usageSession, content, providerInfo))
        }
      } catch (error) {
        diagnostics.push({
          type: "usage-parse-error",
          message: error.message,
          sourcePath: session.rawPath
        })
      }
    }

    const mergedLogs = []

    for (const log of logs) {
      const record = recordMap.get(log.requestId)
      const shouldRefreshProxyRecord =
        input.proxyStates?.[`${log.appType}ProxyState`]?.enabled &&
        record?.providerId === log.appType
      const shouldRefreshInstanceRecord =
        log.requestSource === "provider-instance" &&
        (record?.providerId !== log.providerId ||
          record?.requestSource !== log.requestSource)
      const providerInfo =
        record && !shouldRefreshProxyRecord && !shouldRefreshInstanceRecord
          ? {
              providerId: record.providerId,
              providerName:
                input.providers?.find((item) => item.id === record.providerId)
                  ?.name || record.providerName,
              providerType:
                input.providers?.find((item) => item.id === record.providerId)
                  ?.type || record.providerType
            }
          : {
              providerId: log.providerId,
              providerName: log.providerName,
              providerType: log.providerType
            }
      const requestRecord = createRequestRecord(
        record && !shouldRefreshProxyRecord && !shouldRefreshInstanceRecord
          ? {
              ...log,
              requestSource: record.requestSource || "",
              instanceProviderId: record.instanceProviderId || "",
              instanceProviderName: record.instanceProviderName || "",
              instanceProviderType: record.instanceProviderType || ""
            }
          : log,
        providerInfo
      )

      recordMap.set(log.requestId, requestRecord)
      mergedLogs.push(applyRequestRecord(log, requestRecord))
    }

    this.logs = Array.from(
      new Map(mergedLogs.map((item) => [item.requestId, item])).values()
    ).sort((left, right) => right.createdAt - left.createdAt)
    this.requestRecords = Array.from(recordMap.values()).sort(
      (left, right) => right.createdAt - left.createdAt
    )
    this.storage.scheduleWrite("usageLogs", this.logs)
    this.storage.scheduleWrite("usageRequestRecords", this.requestRecords)

    return {
      logs: this.logs,
      diagnostics
    }
  }

  getStats(input = {}) {
    const filters = {
      appType: normalizeAppType(input.appType || "all"),
      providerId: String(input.providerId || "all"),
      model: String(input.model || "all"),
      requestSource: String(input.requestSource || "all"),
      startAt: Number(input.startAt || 0),
      endAt: Number(input.endAt || 0),
      trendMode:
        input.trendMode === "hour" || input.trendMode === "minute"
          ? input.trendMode
          : "day"
    }
    const logs = this.logs
      .filter((item) => inRange(item, filters))
      .map((item) => enrichUsageLog(item, this.pricingConfig))
    const optionLogs = this.logs
      .filter((item) =>
        inRange(item, {
          ...filters,
          providerId: "all",
          requestSource: "all",
          model: "all"
        })
      )
      .map((item) => enrichUsageLog(item, this.pricingConfig))
    const summary = logs.reduce((result, log) => {
      appendSummary(result, log)
      return result
    }, createEmptySummary())

    return {
      status: "ok",
      data: {
        summary: finalizeSummary(summary),
        providerStats: createGroupStats(
          logs,
          (log) => log.providerId,
          (log) => ({
            providerId: log.providerId,
            providerName: log.providerName,
            providerType: log.providerType
          })
        ),
        modelStats: createGroupStats(
          logs,
          (log) => `${log.appType}:${log.model || "unknown"}`,
          (log) => ({
            appType: log.appType,
            model: log.model || "未识别模型",
            providerName: log.providerName
          })
        ),
        trends: createTrendStats(logs, filters.trendMode, filters),
        logs,
        filters: {
          appTypes: Array.from(new Set(this.logs.map((item) => item.appType))),
          providers: createGroupStats(
            optionLogs,
            (log) => log.providerId,
            (log) => ({
              providerId: log.providerId,
              providerName: log.providerName,
              providerType: log.providerType
            })
          ),
          models: Array.from(
            new Set(optionLogs.map((item) => item.model).filter(Boolean))
          ),
          requestSources: Array.from(
            new Set(optionLogs.map((item) => item.requestSource || "session"))
          )
        },
        pricingConfig: this.getPricingConfig()
      },
      message: ""
    }
  }

  getPricingConfig() {
    return {
      exchangeRate: this.pricingConfig.exchangeRate,
      items: this.pricingConfig.items.map((item) => ({ ...item }))
    }
  }

  savePricingConfig(input) {
    this.pricingConfig = normalizePricingConfig(input)
    this.storage.scheduleWrite("usagePricing", this.pricingConfig)
    return this.getPricingConfig()
  }
}

module.exports = {
  UsageService
}

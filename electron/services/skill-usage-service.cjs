const fs = require("node:fs/promises")
const path = require("node:path")
const matter = require("gray-matter")

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

function getCliType(cliTarget) {
  return cliTarget.type || cliTarget.cli || cliTarget.id
}

function firstDefined(...values) {
  return values.find((item) => item !== undefined && item !== null && item !== "")
}

function toTimestampMs(value, fallback = 0) {
  if (typeof value === "number") {
    return value > 1000000000000 ? value : value * 1000
  }

  const timestamp = value ? new Date(value).getTime() : 0
  return Number.isFinite(timestamp) && timestamp > 0 ? timestamp : fallback
}

function toNumber(value) {
  const number = Number(value || 0)
  return Number.isFinite(number) ? Math.max(0, Math.floor(number)) : 0
}

function normalizeBillableInput(log) {
  if (log.appType === "codex" || log.appType === "gemini") {
    return Math.max(0, toNumber(log.inputTokens) - toNumber(log.cacheReadTokens))
  }

  return toNumber(log.inputTokens)
}

function toActualTokens(log) {
  return (
    normalizeBillableInput(log) +
    toNumber(log.outputTokens) +
    toNumber(log.cacheReadTokens) +
    toNumber(log.cacheCreationTokens)
  )
}

function createEmptySummary() {
  return {
    usageCount: 0,
    requestCount: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    actualTokens: 0,
    totalCostUsd: 0,
    lastUsedAt: 0
  }
}

function appendLogSummary(summary, log) {
  summary.requestCount += 1
  summary.inputTokens += normalizeBillableInput(log)
  summary.outputTokens += toNumber(log.outputTokens)
  summary.cacheReadTokens += toNumber(log.cacheReadTokens)
  summary.cacheCreationTokens += toNumber(log.cacheCreationTokens)
  summary.actualTokens += toNumber(log.actualTokens || toActualTokens(log))
  summary.totalCostUsd += Number(log.totalCostUsd || 0)
  summary.lastUsedAt = Math.max(summary.lastUsedAt, Number(log.createdAt || 0))
}

function collectDisplayRecords(value, records) {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectDisplayRecords(item, records)
    }
    return
  }

  if (!value || typeof value !== "object") {
    return
  }

  if (typeof value.display === "string") {
    records.push(value)
  }

  if (typeof value.payload?.arguments === "string") {
    records.push({ ...value, display: value.payload.arguments })
  }

  for (const item of Object.values(value)) {
    collectDisplayRecords(item, records)
  }
}

function extractSkillNames(display, aliasMap) {
  const matches = []
  const patterns = [
    /(?:^|[\s`"'([{<])\/([A-Za-z0-9][A-Za-z0-9._-]*)/g,
    /(?:^|[\\/])skills[\\/](?:\.system[\\/])?([^\\/]+)[\\/]SKILL\.md/gi
  ]

  for (const pattern of patterns) {
    let match = pattern.exec(display)

    while (match) {
      const skillName = aliasMap.get(match[1].toLowerCase())

      if (skillName && !matches.includes(skillName)) {
        matches.push(skillName)
      }

      match = pattern.exec(display)
    }
  }

  return matches
}

async function readSkillName(skillRoot) {
  const skillFile = path.join(skillRoot, "SKILL.md")
  const content = await fs.readFile(skillFile, "utf8")
  const metadata = matter(content).data || {}

  return String(metadata.name || path.basename(skillRoot)).trim()
}

async function scanSkillRoots(skillsPath) {
  if (!(await pathExists(skillsPath))) {
    return []
  }

  const entries = await fs.readdir(skillsPath, { withFileTypes: true })
  const roots = []

  for (const entry of entries) {
    const skillRoot = path.join(skillsPath, entry.name)

    if (!(await pathExists(skillRoot))) {
      continue
    }

    if (!entry.isDirectory() && !entry.isSymbolicLink()) {
      continue
    }

    const stat = await fs.stat(skillRoot).catch(() => null)

    if (stat?.isDirectory() && (await pathExists(path.join(skillRoot, "SKILL.md")))) {
      roots.push(skillRoot)
    }
  }

  return roots
}

async function readHistoryRecords(filePath) {
  const content = await fs.readFile(filePath, "utf8")
  const records = []

  if (path.extname(filePath).toLowerCase() === ".json") {
    collectDisplayRecords(JSON.parse(content), records)
    return records
  }

  for (const line of content.split(/\r?\n/)) {
    const text = line.trim()

    if (!text) {
      continue
    }

    collectDisplayRecords(JSON.parse(text), records)
  }

  return records
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

    appendLogSummary(groups.get(key), log)
  }

  return Array.from(groups.values()).sort(
    (left, right) => right.actualTokens - left.actualTokens
  )
}

class SkillUsageService {
  constructor(sessionService) {
    this.sessionService = sessionService
  }

  async getStats(input = {}) {
    const cliTargets = input.cliTargets || []
    const skills = await this.collectSkills(cliTargets, input.managedSkills || [])
    const aliasMap = this.createAliasMap(skills)
    const files = await this.collectHistoryFiles(cliTargets)
    const diagnostics = []
    const invocations = []
    const filters = {
      cli: String(input.cli || "all"),
      startAt: Number(input.startAt || 0),
      endAt: Number(input.endAt || 0)
    }

    for (const item of files) {
      try {
        const records = await readHistoryRecords(item.filePath)

        for (const record of records) {
          const display = String(record.display || "").trim()
          const skillNames = extractSkillNames(display, aliasMap)

          if (!skillNames.length) {
            continue
          }

          const createdAt = toTimestampMs(
            firstDefined(
              record.timestamp,
              record.createdAt,
              record.created_at,
              record.payload?.timestamp,
              record.message?.timestamp
            ),
            0
          )

          for (const skillName of skillNames) {
            invocations.push({
              skillName,
              cli: item.cli,
              cliName: item.cliName,
              display,
              rawPath: item.filePath,
              sourceType: item.sourceType,
              createdAt,
              timestamp: createdAt ? Math.floor(createdAt / 1000) : 0
            })
          }
        }
      } catch (error) {
        diagnostics.push({
          type: "skill-usage-parse-error",
          message: error.message,
          sourcePath: item.filePath
        })
      }
    }

    const usageLogs = (input.usageLogs || []).filter((log) =>
      this.matchLogFilters(log, filters)
    )
    const matchedLogs = this.matchInvocationLogs(
      invocations.filter((item) => this.matchInvocationFilters(item, filters)),
      usageLogs
    )
    const rows = this.createRows(skills, invocations, matchedLogs, filters)
    const summary = rows.reduce(
      (result, item) => {
        result.skillCount += 1
        result.usageCount += item.usageCount
        result.requestCount += item.requestCount
        result.actualTokens += item.actualTokens
        result.totalCostUsd += item.totalCostUsd
        result.lastUsedAt = Math.max(result.lastUsedAt, item.lastUsedAt)

        if (item.usageCount > 0) {
          result.usedSkillCount += 1
        }

        return result
      },
      {
        skillCount: 0,
        usedSkillCount: 0,
        usageCount: 0,
        requestCount: 0,
        actualTokens: 0,
        totalCostUsd: 0,
        lastUsedAt: 0
      }
    )

    return {
      status: "ok",
      data: {
        summary,
        skills: rows,
        filters: {
          clis: Array.from(
            new Map(
              cliTargets.map((item) => [
                getCliType(item),
                {
                  id: getCliType(item),
                  name: item.name || getCliType(item)
                }
              ])
            ).values()
          )
        },
        diagnostics
      },
      message: ""
    }
  }

  async collectSkills(cliTargets, managedSkills) {
    const skillMap = new Map()

    for (const skill of managedSkills) {
      this.appendSkill(skillMap, {
        name: skill.name,
        description: skill.description || "",
        sourcePath: skill.sourcePath,
        cli: "",
        cliName: ""
      })
    }

    for (const cliTarget of cliTargets) {
      if (!cliTarget.skillsPath) {
        continue
      }

      const cli = getCliType(cliTarget)
      const skillRoots = await scanSkillRoots(cliTarget.skillsPath)

      for (const skillRoot of skillRoots) {
        const name = await readSkillName(skillRoot)

        this.appendSkill(skillMap, {
          name,
          description: "",
          sourcePath: skillRoot,
          cli,
          cliName: cliTarget.name || cli
        })
      }
    }

    return Array.from(skillMap.values()).sort((left, right) =>
      left.name.localeCompare(right.name, "zh-Hans-CN")
    )
  }

  appendSkill(skillMap, skill) {
    const name = String(skill.name || "").trim()

    if (!name) {
      return
    }

    if (!skillMap.has(name)) {
      skillMap.set(name, {
        name,
        description: skill.description || "",
        sourcePaths: [],
        cliTypes: [],
        aliases: [name]
      })
    }

    const item = skillMap.get(name)

    if (skill.description && !item.description) {
      item.description = skill.description
    }

    if (skill.sourcePath && !item.sourcePaths.includes(skill.sourcePath)) {
      item.sourcePaths.push(skill.sourcePath)
      item.aliases.push(path.basename(skill.sourcePath))
    }

    if (skill.cli && !item.cliTypes.find((cli) => cli.id === skill.cli)) {
      item.cliTypes.push({
        id: skill.cli,
        name: skill.cliName || skill.cli
      })
    }
  }

  createAliasMap(skills) {
    const aliasMap = new Map()

    for (const skill of skills) {
      for (const alias of skill.aliases) {
        const value = String(alias || "").trim()

        if (value) {
          aliasMap.set(value.toLowerCase(), skill.name)
        }
      }
    }

    return aliasMap
  }

  async collectHistoryFiles(cliTargets) {
    const files = []

    for (const cliTarget of cliTargets) {
      const cli = getCliType(cliTarget)
      const cliName = cliTarget.name || cli
      const configPath = cliTarget.configPath

      if (cli === "claude" && configPath) {
        const historyPath = path.join(configPath, "history.jsonl")

        if (await pathExists(historyPath)) {
          files.push({
            cli,
            cliName,
            filePath: historyPath,
            sourceType: "history"
          })
        }
      }

      for (const sessionPath of this.sessionService.getSessionPaths(cliTarget)) {
        if (!(await pathExists(sessionPath))) {
          continue
        }

        const sessionFiles = await this.sessionService.scanSessionFiles(
          sessionPath,
          cliTarget
        )

        for (const filePath of sessionFiles) {
          files.push({
            cli,
            cliName,
            filePath,
            sourceType: "session"
          })
        }
      }
    }

    return files
  }

  matchInvocationFilters(invocation, filters) {
    if (filters.cli !== "all" && invocation.cli !== filters.cli) {
      return false
    }

    if (filters.startAt && !invocation.createdAt) {
      return false
    }

    if (filters.startAt && invocation.createdAt < filters.startAt) {
      return false
    }

    if (filters.endAt && !invocation.createdAt) {
      return false
    }

    if (filters.endAt && invocation.createdAt > filters.endAt) {
      return false
    }

    return true
  }

  matchLogFilters(log, filters) {
    if (filters.cli !== "all" && log.appType !== filters.cli) {
      return false
    }

    return true
  }

  matchInvocationLogs(invocations, usageLogs) {
    const logsByPath = new Map()
    const result = new Map()

    for (const log of usageLogs) {
      const rawPath = log.rawPath || ""

      if (!rawPath) {
        continue
      }

      if (!logsByPath.has(rawPath)) {
        logsByPath.set(rawPath, [])
      }

      logsByPath.get(rawPath).push(log)
    }

    for (const logs of logsByPath.values()) {
      logs.sort((left, right) => left.createdAt - right.createdAt)
    }

    const invocationsByPath = new Map()

    for (const invocation of invocations) {
      if (!invocation.rawPath || invocation.sourceType !== "session") {
        continue
      }

      if (!invocationsByPath.has(invocation.rawPath)) {
        invocationsByPath.set(invocation.rawPath, [])
      }

      invocationsByPath.get(invocation.rawPath).push(invocation)
    }

    for (const [rawPath, items] of invocationsByPath) {
      const logs = logsByPath.get(rawPath) || []
      const sortedItems = items.sort((left, right) => left.createdAt - right.createdAt)

      for (let index = 0; index < sortedItems.length; index += 1) {
        const invocation = sortedItems[index]
        const nextInvocation = sortedItems[index + 1]
        const matched = logs.filter((log) => {
          if (!invocation.createdAt) {
            return false
          }

          if (log.createdAt < invocation.createdAt) {
            return false
          }

          return !nextInvocation?.createdAt || log.createdAt < nextInvocation.createdAt
        })

        result.set(invocation, matched)
      }
    }

    return result
  }

  createRows(skills, invocations, matchedLogs, filters) {
    const rows = skills.map((skill) => {
      const skillInvocations = invocations.filter((item) => {
        return item.skillName === skill.name && this.matchInvocationFilters(item, filters)
      })
      const logs = skillInvocations.flatMap((item) => matchedLogs.get(item) || [])
      const summary = logs.reduce((result, log) => {
        appendLogSummary(result, log)
        return result
      }, createEmptySummary())
      summary.usageCount = skillInvocations.length
      summary.lastUsedAt = Math.max(
        summary.lastUsedAt,
        ...skillInvocations.map((item) => item.createdAt || 0)
      )

      return {
        ...skill,
        ...summary,
        totalCostUsd: Number(summary.totalCostUsd.toFixed(8)),
        providers: createGroupStats(
          logs,
          (log) => log.providerId,
          (log) => ({
            providerId: log.providerId,
            providerName: log.providerName || log.providerId || "未知 Provider"
          })
        ),
        models: createGroupStats(
          logs,
          (log) => `${log.appType}:${log.model || "unknown"}`,
          (log) => ({
            appType: log.appType,
            model: log.model || "未识别模型",
            providerName: log.providerName || ""
          })
        )
      }
    })

    return rows.sort((left, right) => {
      if (right.usageCount !== left.usageCount) {
        return right.usageCount - left.usageCount
      }

      return left.name.localeCompare(right.name, "zh-Hans-CN")
    })
  }
}

module.exports = {
  SkillUsageService
}

const crypto = require("node:crypto")
const nodeFs = require("node:fs")
const fs = require("node:fs/promises")
const path = require("node:path")
const readline = require("node:readline")
const chokidar = require("chokidar")

const ignoredDirectories = new Set(["node_modules", ".git", "dist", "build"])

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

async function moveFile(sourcePath, targetPath) {
  await fs.copyFile(sourcePath, targetPath)
  await fs.rm(sourcePath)
}

function createSessionId(rawPath) {
  return crypto.createHash("sha1").update(path.resolve(rawPath)).digest("hex")
}

function truncateText(value, length) {
  const text = String(value || "").replace(/\s+/g, " ").trim()
  return text.length > length ? `${text.slice(0, length)}...` : text
}

function readNestedValue(source, keys) {
  for (const key of keys) {
    if (source?.[key]) {
      return source[key]
    }
  }

  return undefined
}

function getCliType(cliTarget) {
  return cliTarget.type || cliTarget.cli || cliTarget.id
}

function normalizeContent(content) {
  if (typeof content === "string") {
    return content
  }

  if (Array.isArray(content)) {
    return content
      .map((item) => {
        if (typeof item === "string") {
          return item
        }

        return item.text || item.content || item.input || item.result || ""
      })
      .filter(Boolean)
      .join("\n")
  }

  if (content && typeof content === "object") {
    return content.text || content.content || JSON.stringify(content)
  }

  return ""
}

function normalizeRole(value) {
  if (["user", "assistant", "tool", "system"].includes(value)) {
    return value
  }

  if (value === "tool_use" || value === "tool_result") {
    return "tool"
  }

  return "system"
}

function extractToolCalls(record) {
  const payload = record.payload || record
  const content = payload.message?.content || payload.content

  if (!Array.isArray(content)) {
    if (payload.type === "function_call") {
      return [
        {
          name: payload.name || payload.call_id || "function_call",
          arguments: payload.arguments,
          result: undefined
        }
      ]
    }

    return []
  }

  return content
    .filter((item) => item?.type === "tool_use" || item?.type === "tool_result")
    .map((item) => ({
      name: item.name || item.toolName || item.type,
      arguments: item.input ? JSON.stringify(item.input) : undefined,
      result: normalizeContent(item.content)
    }))
}

function normalizeMessage(record) {
  const payload = record.payload || record
  const message = payload.message || payload
  const role = normalizeRole(message.role || payload.role || payload.type)
  const content = normalizeContent(message.content || payload.content)
  const timestampSource =
    record.timestamp || payload.timestamp || message.timestamp || payload.createdAt
  const timestamp = timestampSource ? new Date(timestampSource).getTime() : undefined
  const toolCalls = extractToolCalls(record)

  return {
    role,
    content,
    timestamp,
    toolCalls,
    files: Array.isArray(record.files) ? record.files : []
  }
}

function collectMetadataItem(metadata, item) {
  const payload = item.payload || item
  metadata.title = metadata.title || payload.title || payload.metadata?.title
  metadata.cwd = metadata.cwd || payload.cwd || payload.metadata?.cwd
  metadata.workspace =
    metadata.workspace || payload.workspace || payload.metadata?.workspace
  metadata.projectPath =
    metadata.projectPath ||
    payload.projectPath ||
    payload.metadata?.projectPath
  metadata.model = metadata.model || payload.model || payload.message?.model
  metadata.tokenCount =
    metadata.tokenCount || payload.tokenCount || payload.usage?.total_tokens
  return metadata
}

class BaseSessionParser {
  parse(content, extension) {
    if (extension === ".json") {
      return this.parseJsonContent(content)
    }

    return this.parseLineContent(content, extension)
  }

  parseJsonContent(content) {
    const payload = JSON.parse(content)
    const messages = Array.isArray(payload)
      ? payload.map((item) => normalizeMessage(item))
      : (payload.messages || []).map((item) => normalizeMessage(item))
    const metadata = Array.isArray(payload) ? this.collectMetadata(payload) : payload

    return { messages: messages.filter((item) => item.content), metadata }
  }

  parseLineContent(content, extension) {
    if (extension === ".md" && !content.toLowerCase().includes("messages")) {
      return { messages: [], metadata: {} }
    }

    const records = content
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => JSON.parse(line))
    const messages = records.map((item) => normalizeMessage(item))

    return {
      messages: messages.filter((item) => item.content),
      metadata: this.collectMetadata(records)
    }
  }

  collectMetadata(records) {
    return records.reduce((metadata, item) => {
      return collectMetadataItem(metadata, item)
    }, {})
  }

  createMetadataState() {
    return {
      metadata: {},
      firstUserContent: "",
      firstAssistantContent: "",
      messageCount: 0,
      hasMessageRole: false
    }
  }

  collectMetadataRecord(state, record) {
    collectMetadataItem(state.metadata, record)
    const message = normalizeMessage(record)

    if (!this.includeMetadataMessage(record, message)) {
      return
    }

    state.messageCount += 1

    if (!state.firstUserContent && message.role === "user") {
      state.firstUserContent = message.content
    }

    if (!state.firstAssistantContent && message.role === "assistant") {
      state.firstAssistantContent = message.content
    }

    if (["user", "assistant", "tool"].includes(message.role)) {
      state.hasMessageRole = true
    }
  }

  includeMetadataMessage(_, message) {
    return Boolean(message.content)
  }

  isValidMetadata(state) {
    return state.messageCount > 0
  }

  isValidSession(parsed) {
    return parsed.messages.length > 0
  }
}

class ClaudeSessionParser extends BaseSessionParser {
  collectMetadataRecord(state, record, line) {
    state.hasClaudeSignal =
      state.hasClaudeSignal ||
      line.includes("messages") ||
      line.includes("role") ||
      line.includes("assistant")
    super.collectMetadataRecord(state, record, line)
  }

  isValidSession(parsed, content) {
    const hasMessageRole = parsed.messages.some((item) =>
      ["user", "assistant", "tool"].includes(item.role)
    )
    const hasClaudeSignal =
      content.includes("messages") ||
      content.includes("role") ||
      content.includes("assistant")

    return parsed.messages.length > 0 && hasMessageRole && hasClaudeSignal
  }

  isValidMetadata(state) {
    return state.messageCount > 0 && state.hasMessageRole && state.hasClaudeSignal
  }
}

class CodexSessionParser extends BaseSessionParser {
  parseLineContent(content) {
    const records = content
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => JSON.parse(line))
    const messages = records
      .filter((item) => {
        const payload = item.payload || item
        return payload.type === "message" || payload.type === "function_call"
      })
      .map((item) => normalizeMessage(item))

    return {
      messages: messages.filter((item) => item.content || item.toolCalls.length),
      metadata: this.collectMetadata(records)
    }
  }

  isValidSession(parsed) {
    return parsed.messages.some((item) =>
      ["user", "assistant", "tool"].includes(item.role)
    )
  }

  includeMetadataMessage(record, message) {
    const payload = record.payload || record

    if (payload.type !== "message" && payload.type !== "function_call") {
      return false
    }

    return Boolean(message.content || message.toolCalls.length)
  }

  isValidMetadata(state) {
    return state.hasMessageRole
  }
}

class GeminiSessionParser extends BaseSessionParser {
  isValidSession(parsed) {
    return parsed.messages.length > 0
  }
}

class OpenCodeSessionParser extends BaseSessionParser {
  isValidSession(parsed) {
    return parsed.messages.some((item) =>
      ["user", "assistant", "tool"].includes(item.role)
    )
  }

  isValidMetadata(state) {
    return state.hasMessageRole
  }
}

class BaseSessionScanner {
  constructor(defaultRules) {
    this.defaultRules = defaultRules
  }

  getRules(cliTarget) {
    return cliTarget.sessionScanRules || this.defaultRules
  }

  async scan(rootPath, cliTarget, depth = 0) {
    if (depth > 5) {
      return []
    }

    const entries = await fs.readdir(rootPath, { withFileTypes: true })
    const files = []

    for (const entry of entries) {
      const entryPath = path.join(rootPath, entry.name)

      if (entry.isDirectory()) {
        if (!ignoredDirectories.has(entry.name)) {
          files.push(...(await this.scan(entryPath, cliTarget, depth + 1)))
        }

        continue
      }

      if (entry.isFile() && this.matchFile(entry.name, cliTarget)) {
        files.push(entryPath)
      }
    }

    return files
  }

  matchFile(fileName, cliTarget) {
    const rules = this.getRules(cliTarget)
    const extensionMatched = new Set(rules.extensions || []).has(
      path.extname(fileName)
    )
    const names = rules.names || []
    const nameMatched =
      !names.length || names.some((item) => fileName.startsWith(item))

    return extensionMatched && nameMatched
  }
}

class ClaudeScanner extends BaseSessionScanner {
  constructor() {
    super({
      extensions: [".jsonl"],
      names: []
    })
  }
}

class CodexScanner extends BaseSessionScanner {
  constructor() {
    super({
      extensions: [".json", ".jsonl", ".transcript"],
      names: []
    })
  }
}

class GeminiScanner extends BaseSessionScanner {
  constructor() {
    super({
      extensions: [".json", ".jsonl"],
      names: ["session", "checkpoint"]
    })
  }
}

class OpenCodeScanner extends BaseSessionScanner {
  constructor() {
    super({
      extensions: [".json", ".jsonl", ".transcript"],
      names: []
    })
  }
}

class SessionService {
  constructor(paths) {
    this.paths = paths
    this.storage = null
    this.watcher = null
    this.watchTimer = null
    this.sessions = []
    this.parsers = {
      claude: new ClaudeSessionParser(),
      codex: new CodexSessionParser(),
      gemini: new GeminiSessionParser(),
      opencode: new OpenCodeSessionParser(),
      default: new BaseSessionParser()
    }
    this.scanners = {
      claude: new ClaudeScanner(),
      codex: new CodexScanner(),
      gemini: new GeminiScanner(),
      opencode: new OpenCodeScanner(),
      default: new BaseSessionScanner({
        extensions: [".json", ".jsonl", ".transcript"],
        names: []
      })
    }
  }

  bindStorage(storage) {
    this.storage = storage
  }

  async init() {
    this.sessions = await this.storage.read("sessions", [])
  }

  async refresh(cliTargets) {
    const sessions = []
    const diagnostics = []

    for (const cliTarget of cliTargets.filter((item) => item.installed)) {
      for (const sessionPath of this.getSessionPaths(cliTarget)) {
        if (!(await pathExists(sessionPath))) {
          continue
        }

        const files = await this.scanSessionFiles(sessionPath, cliTarget)

        for (const filePath of files) {
          try {
            const parsed = await this.parseSessionMetadata(cliTarget, filePath)

            if (!parsed) {
              continue
            }

            sessions.push(this.toMetadata(parsed))
          } catch (error) {
            diagnostics.push({
              type: "session-parse-error",
              message: error.message,
              sourcePath: filePath
            })
          }
        }
      }
    }

    this.sessions = sessions.sort(
      (left, right) => right.updatedAt - left.updatedAt
    )
    this.storage.scheduleWrite(
      "sessions",
      this.sessions.map((item) => this.toMetadata(item))
    )

    return { sessions: this.sessions, diagnostics }
  }

  getSessionPaths(cliTarget) {
    if (Array.isArray(cliTarget.sessionPaths) && cliTarget.sessionPaths.length) {
      return cliTarget.sessionPaths
    }

    return [cliTarget.sessionsPath].filter(Boolean)
  }

  async scanSessionFiles(rootPath, cliTarget) {
    const scanner = this.scanners[getCliType(cliTarget)] || this.scanners.default
    return scanner.scan(rootPath, cliTarget)
  }

  async parseSession(cliTarget, rawPath) {
    const extension = path.extname(rawPath)
    const content = await fs.readFile(rawPath, "utf8")
    const stat = await fs.stat(rawPath)
    const parser = this.parsers[getCliType(cliTarget)] || this.parsers.default
    const parsed = parser.parse(content, extension)

    if (!parser.isValidSession(parsed, content)) {
      return null
    }

    const id = createSessionId(rawPath)
    const projectPath = await this.resolveProjectPath(
      cliTarget,
      rawPath,
      parsed.metadata
    )
    const firstUserMessage = parsed.messages.find((item) => item.role === "user")
    const firstAssistantMessage = parsed.messages.find(
      (item) => item.role === "assistant"
    )
    const title =
      parsed.metadata.title || truncateText(firstUserMessage?.content, 50)

    return {
      id,
      cli: getCliType(cliTarget),
      cliName: cliTarget.name || cliTarget.cliName,
      title: title || path.basename(rawPath),
      summary: truncateText(firstAssistantMessage?.content, 120),
      projectPath,
      projectName: projectPath ? path.basename(projectPath) : undefined,
      model: parsed.metadata.model,
      rawPath,
      createdAt: stat.birthtimeMs,
      updatedAt: stat.mtimeMs,
      messageCount: parsed.messages.length,
      tokenCount: parsed.metadata.tokenCount,
      pinned: false,
      archived: false,
      deleted: false,
      messages: parsed.messages
    }
  }

  async parseSessionMetadata(cliTarget, rawPath) {
    const extension = path.extname(rawPath)
    const stat = await fs.stat(rawPath)
    const parser = this.parsers[getCliType(cliTarget)] || this.parsers.default
    let parsed = null

    if (extension === ".json") {
      const content = await fs.readFile(rawPath, "utf8")
      const fullParsed = parser.parse(content, extension)

      if (!parser.isValidSession(fullParsed, content)) {
        return null
      }

      parsed = {
        metadata: fullParsed.metadata,
        firstUserContent:
          fullParsed.messages.find((item) => item.role === "user")?.content || "",
        firstAssistantContent:
          fullParsed.messages.find((item) => item.role === "assistant")?.content || "",
        messageCount: fullParsed.messages.length
      }
    } else {
      const state = parser.createMetadataState()
      const lines = readline.createInterface({
        input: nodeFs.createReadStream(rawPath, { encoding: "utf8" }),
        crlfDelay: Infinity
      })

      for await (const line of lines) {
        const text = line.trim()

        if (!text) {
          continue
        }

        parser.collectMetadataRecord(state, JSON.parse(text), text)
      }

      if (!parser.isValidMetadata(state)) {
        return null
      }

      parsed = state
    }

    const id = createSessionId(rawPath)
    const projectPath = await this.resolveProjectPath(
      cliTarget,
      rawPath,
      parsed.metadata
    )
    const title =
      parsed.metadata.title || truncateText(parsed.firstUserContent, 50)

    return {
      id,
      cli: getCliType(cliTarget),
      cliName: cliTarget.name || cliTarget.cliName,
      title: title || path.basename(rawPath),
      summary: truncateText(parsed.firstAssistantContent, 120),
      projectPath,
      projectName: projectPath ? path.basename(projectPath) : undefined,
      model: parsed.metadata.model,
      rawPath,
      createdAt: stat.birthtimeMs,
      updatedAt: stat.mtimeMs,
      messageCount: parsed.messageCount,
      tokenCount: parsed.metadata.tokenCount,
      pinned: false,
      archived: false,
      deleted: false
    }
  }

  async resolveProjectPath(cliTarget, rawPath, metadata) {
    const projectPath = readNestedValue(metadata, [
      "cwd",
      "workspace",
      "projectPath"
    ])

    if (projectPath || getCliType(cliTarget) !== "gemini") {
      return projectPath
    }

    const candidatePaths = [
      path.join(path.dirname(rawPath), ".project_root"),
      path.join(path.dirname(path.dirname(rawPath)), ".project_root")
    ]

    for (const candidatePath of candidatePaths) {
      if (await pathExists(candidatePath)) {
        return (await fs.readFile(candidatePath, "utf8")).trim()
      }
    }

    return undefined
  }

  toMetadata(session) {
    const { messages, ...metadata } = session
    return metadata
  }

  getRecycleSessionPath(session) {
    const extension = path.extname(session.rawPath) || ".session"
    return path.join(this.paths.sessionRecycleSessionsDir, `${session.id}${extension}`)
  }

  getRecycleMetadataPath(sessionId) {
    return path.join(this.paths.sessionRecycleMetadataDir, `${sessionId}.json`)
  }

  async loadMessages(sessionId) {
    const session = this.sessions.find((item) => item.id === sessionId)

    if (!session) {
      throw new Error("Session 不存在")
    }

    const parsed = await this.parseSession(session, session.rawPath)
    return parsed?.messages || []
  }

  async search(query) {
    const keyword = String(query || "").trim().toLowerCase()

    if (!keyword) {
      return this.sessions
    }

    const results = []

    for (const session of this.sessions) {
      const metadataText = [
        session.title,
        session.summary,
        session.projectName,
        session.projectPath,
        session.model,
        session.cliName
      ]
        .join(" ")
        .toLowerCase()

      if (metadataText.includes(keyword)) {
        results.push(session)
        continue
      }

      const messages = await this.loadMessages(session.id)
      const messageText = messages
        .map((item) => [
          item.role,
          item.content,
          ...(item.toolCalls || []).map((tool) =>
            [tool.name, tool.arguments, tool.result].join(" ")
          ),
          ...(item.files || [])
        ].join(" "))
        .join(" ")
        .toLowerCase()

      if (messageText.includes(keyword)) {
        results.push(session)
      }
    }

    return results
  }

  async moveToRecycle(sessionId) {
    const session = this.sessions.find((item) => item.id === sessionId)

    if (!session) {
      throw new Error("Session 不存在")
    }

    const recycledPath = this.getRecycleSessionPath(session)
    const metadata = {
      ...this.toMetadata(session),
      originalPath: session.rawPath,
      recycledPath,
      recycledAt: Date.now()
    }
    await moveFile(session.rawPath, recycledPath)
    await fs.writeFile(
      this.getRecycleMetadataPath(sessionId),
      `${JSON.stringify(metadata, null, 2)}\n`,
      "utf8"
    )
    this.sessions = this.sessions.filter((item) => item.id !== sessionId)
    this.storage.scheduleWrite("sessions", this.sessions.map((item) => this.toMetadata(item)))
  }

  async listRecycle() {
    const entries = await fs.readdir(this.paths.sessionRecycleMetadataDir, {
      withFileTypes: true
    })
    const sessions = []

    for (const entry of entries) {
      if (!entry.isFile() || path.extname(entry.name) !== ".json") {
        continue
      }

      const content = await fs.readFile(
        path.join(this.paths.sessionRecycleMetadataDir, entry.name),
        "utf8"
      )
      sessions.push(JSON.parse(content))
    }

    return sessions.sort((left, right) => right.recycledAt - left.recycledAt)
  }

  async restoreFromRecycle(sessionId) {
    const metadataPath = this.getRecycleMetadataPath(sessionId)
    const metadata = JSON.parse(await fs.readFile(metadataPath, "utf8"))
    await fs.mkdir(path.dirname(metadata.originalPath), { recursive: true })
    await moveFile(metadata.recycledPath, metadata.originalPath)
    await fs.rm(metadataPath)
  }

  async purgeFromRecycle(sessionId) {
    const metadataPath = this.getRecycleMetadataPath(sessionId)
    const metadata = JSON.parse(await fs.readFile(metadataPath, "utf8"))
    await fs.rm(metadata.recycledPath)
    await fs.rm(metadataPath)
  }

  startWatcher(cliTargets, onRefresh) {
    this.stopWatcher()

    const watchPaths = Array.from(
      new Set(cliTargets.flatMap((item) => this.getSessionPaths(item)).filter(Boolean))
    )

    if (!watchPaths.length) {
      return
    }

    this.watcher = chokidar.watch(watchPaths, {
      ignoreInitial: true,
      depth: 5,
      followSymlinks: false
    })

    const trigger = () => {
      clearTimeout(this.watchTimer)
      this.watchTimer = setTimeout(() => {
        onRefresh()
      }, 500)
    }

    this.watcher.on("add", trigger)
    this.watcher.on("change", trigger)
    this.watcher.on("unlink", trigger)
    this.watcher.on("addDir", trigger)
    this.watcher.on("unlinkDir", trigger)
  }

  stopWatcher() {
    clearTimeout(this.watchTimer)
    this.watchTimer = null

    if (this.watcher) {
      this.watcher.close()
      this.watcher = null
    }
  }

  async dispose() {
    this.stopWatcher()
  }
}

module.exports = {
  SessionService
}

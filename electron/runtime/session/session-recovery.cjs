const fs = require("node:fs/promises")
const path = require("node:path")
const crypto = require("node:crypto")

function createSessionId(rawPath) {
  return crypto.createHash("sha1").update(path.resolve(rawPath)).digest("hex")
}

function normalizeContent(value) {
  if (typeof value === "string") {
    return value
  }

  if (Array.isArray(value)) {
    return value
      .map(item => normalizeContent(item?.text || item?.content || item?.input || item))
      .filter(Boolean)
      .join("\n")
  }

  if (value && typeof value === "object") {
    return value.text || value.content || JSON.stringify(value)
  }

  return ""
}

function readPayload(record) {
  return record.payload || record
}

async function readFirstLine(filePath) {
  const handle = await fs.open(filePath, "r")
  const chunks = []
  let position = 0

  try {
    while (true) {
      const buffer = Buffer.alloc(65536)
      const { bytesRead } = await handle.read(
        buffer,
        0,
        buffer.length,
        position
      )

      if (!bytesRead) {
        break
      }

      const chunk = buffer.subarray(0, bytesRead)
      const lineEnd = chunk.indexOf(10)

      if (lineEnd >= 0) {
        chunks.push(chunk.subarray(0, lineEnd))
        break
      }

      chunks.push(chunk)
      position += bytesRead
    }
  } finally {
    await handle.close()
  }

  return Buffer.concat(chunks).toString("utf8").trim()
}

async function readRecentLines(filePath, size) {
  const handle = await fs.open(filePath, "r")
  const length = Math.min(Number(size || 0), 512 * 1024)
  const offset = Math.max(0, Number(size || 0) - length)
  const buffer = Buffer.alloc(length)

  try {
    const { bytesRead } = await handle.read(buffer, 0, buffer.length, offset)
    const content = buffer.subarray(0, bytesRead).toString("utf8")
    const lines = content.split(/\r?\n/)

    if (offset > 0) {
      lines.shift()
    }

    return lines.map(line => line.trim()).filter(Boolean)
  } finally {
    await handle.close()
  }
}

function createChatMessageFromRecord(filePath, record, index) {
  const payload = readPayload(record)
  const type = payload.type || record.type

  if (type !== "message" && !payload.message) {
    return null
  }

  const message = payload.message || payload

  if (message.role !== "user" && message.role !== "assistant") {
    return null
  }

  const text = normalizeContent(message.content || payload.content)

  if (!text.trim()) {
    return null
  }

  const timestamp = record.timestamp || payload.timestamp
    ? new Date(record.timestamp || payload.timestamp).getTime()
    : Date.now()

  return {
    id: createSessionId(`${filePath}:${timestamp}:${message.role}:${index}`),
    role: message.role,
    source: "session",
    text,
    createdAt: timestamp,
    updatedAt: timestamp
  }
}

async function createRecentChatMessages(filePath, size) {
  const lines = await readRecentLines(filePath, size)
  const messages = []

  for (let index = 0; index < lines.length; index += 1) {
    if (!lines[index].startsWith("{") || !lines[index].endsWith("}")) {
      continue
    }

    const message = createChatMessageFromRecord(
      filePath,
      JSON.parse(lines[index]),
      index
    )

    if (message) {
      messages.push(message)
    }
  }

  return messages.slice(-80)
}

function inferEventFromRecord(filePath, record) {
  const payload = readPayload(record)
  const type = payload.type || record.type
  const sessionId = createSessionId(filePath)
  const timestamp = record.timestamp || payload.timestamp
    ? new Date(record.timestamp || payload.timestamp).getTime()
    : Date.now()

  if (type === "session_meta") {
    return {
      sessionId,
      type: "SESSION_RESUMED",
      timestamp,
      title: payload.title || path.basename(filePath),
      cwd: payload.cwd,
      model: payload.model,
      payload: {
        source: "session"
      }
    }
  }

  if (type === "function_call") {
    return {
      sessionId,
      type: "TOOL_STARTED",
      timestamp,
      payload: {
        id: payload.call_id || payload.id,
        name: payload.name || "function_call",
        source: "session"
      }
    }
  }

  if (type === "function_call_output") {
    return {
      sessionId,
      type: "TOOL_COMPLETED",
      timestamp,
      payload: {
        id: payload.call_id || payload.id,
        name: payload.name || "function_call",
        source: "session"
      }
    }
  }

  if (type === "fork" || type === "agent") {
    return {
      sessionId,
      type: "AGENT_SPAWNED",
      timestamp,
      payload: {
        id: payload.id,
        title: payload.title || payload.name || "Agent",
        source: "fork",
        confidence: 80
      }
    }
  }

  if (payload.usage) {
    return {
      sessionId,
      type: "TOKEN_USAGE",
      timestamp,
      payload: {
        input: payload.usage.input_tokens || payload.usage.prompt_tokens || 0,
        output: payload.usage.output_tokens || payload.usage.completion_tokens || 0
      }
    }
  }

  if (type === "message" || payload.message) {
    const message = payload.message || payload
    const text = normalizeContent(message.content || payload.content)

    if (message.role === "user") {
      return {
        sessionId,
        type: "WAITING_USER",
        timestamp,
        payload: {
          role: "user",
          source: "session",
          text
        }
      }
    }

    if (message.role === "assistant") {
      return {
        sessionId,
        type: "STREAM_DELTA",
        timestamp,
        payload: {
          role: "assistant",
          text,
          source: "session"
        }
      }
    }
  }

  return null
}

async function createSessionEventFromFile(filePath) {
  const stat = await fs.stat(filePath)
  const firstLine =
    path.extname(filePath) === ".jsonl" ? await readFirstLine(filePath) : ""
  const record = firstLine ? JSON.parse(firstLine) : {}
  const payload = record.payload || {}
  const chatMessages =
    path.extname(filePath) === ".jsonl"
      ? await createRecentChatMessages(filePath, stat.size)
      : []
  const startedAt = payload.timestamp
    ? new Date(payload.timestamp).getTime()
    : stat.birthtimeMs

  return {
    sessionId: createSessionId(filePath),
    mode: "external",
    type: "SESSION_RESUMED",
    timestamp: stat.mtimeMs,
    title: path.basename(filePath),
    cwd: record.type === "session_meta" ? payload.cwd || "" : "",
    model: record.type === "session_meta" ? payload.model || "" : "",
    startedAt,
    payload: {
      rawPath: filePath,
      chatMessages,
      size: stat.size,
      source: "session"
    }
  }
}

module.exports = {
  createSessionEventFromFile,
  createSessionId,
  inferEventFromRecord
}

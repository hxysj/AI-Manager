const { EventEmitter } = require("node:events")

function stripAnsi(value) {
  return String(value || "").replace(/\x1B\[[0-?]*[ -/]*[@-~]/g, "")
}

function matchToolName(text, patterns) {
  for (const pattern of patterns) {
    const match = pattern.exec(text)

    if (match?.[1]) {
      return match[1].trim()
    }
  }

  return ""
}

class StreamParser extends EventEmitter {
  constructor(sessionId) {
    super()
    this.sessionId = sessionId
    this.started = false
  }

  feed(chunk) {
    const text = stripAnsi(chunk)
    const timestamp = Date.now()

    if (!this.started) {
      this.started = true
      this.emitEvent("STREAM_STARTED", timestamp, {})
    }

    const startedTool = matchToolName(text, [
      /Running tool\s+([^\r\n]+)/i,
      /Tool started[:：]\s*([^\r\n]+)/i,
      /call:\s*([a-zA-Z0-9_-]+)/i
    ])

    if (startedTool) {
      this.emitEvent("TOOL_STARTED", timestamp, {
        name: startedTool,
        source: "stdout"
      })
    }

    const completedTool = matchToolName(text, [
      /Completed tool\s+([^\r\n]+)/i,
      /Tool completed[:：]\s*([^\r\n]+)/i
    ])

    if (completedTool) {
      this.emitEvent("TOOL_COMPLETED", timestamp, {
        name: completedTool,
        source: "stdout"
      })
    }

    if (/Waiting for approval|approval required|需要批准/i.test(text)) {
      this.emitEvent("APPROVAL_REQUEST", timestamp, {
        text,
        source: "stdout"
      })
    }

    if (/Spawning agent|spawned agent|启动 Agent/i.test(text)) {
      this.emitEvent("AGENT_SPAWNED", timestamp, {
        title: "Agent",
        source: "stdout",
        confidence: 70
      })
    }

    if (text) {
      this.emitEvent("STREAM_DELTA", timestamp, {
        text,
        source: "stdout"
      })
    }
  }

  emitEvent(type, timestamp, payload) {
    this.emit("event", {
      sessionId: this.sessionId,
      type,
      timestamp,
      payload
    })
  }
}

module.exports = {
  StreamParser
}

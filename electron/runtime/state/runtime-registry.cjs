const crypto = require("node:crypto")

const maxTimelineLength = 2000
const maxChatMessageLength = 400
const externalTransientStates = new Set([
  "streaming",
  "running_tools",
  "waiting_approval",
  "background_agents"
])

function createId(parts) {
  return crypto.createHash("sha1").update(parts.join("|")).digest("hex")
}

function toArray(value) {
  return Array.isArray(value) ? value : []
}

function createSession(input) {
  const now = Date.now()

  return {
    id: input.sessionId,
    mode: input.mode || "external",
    cwd: input.cwd || "",
    title: input.title || input.sessionId,
    model: input.model || "",
    state: "idle",
    startedAt: input.startedAt || now,
    lastActivityAt: input.lastActivityAt || now,
    activeTools: [],
    agents: [],
    tokenUsage: {
      input: 0,
      output: 0
    },
    chatMessages: [],
    timeline: []
  }
}

class RuntimeRegistry {
  constructor() {
    this.sequence = 0
    this.sessions = new Map()
  }

  restore(snapshot = {}) {
    this.sequence = Number(snapshot.sequence || 0)
    this.sessions = new Map()

    for (const session of toArray(snapshot.sessions)) {
      const restoredSession = {
        ...session,
        activeTools: toArray(session.activeTools),
        agents: toArray(session.agents),
        chatMessages: toArray(session.chatMessages).slice(-maxChatMessageLength),
        timeline: toArray(session.timeline).slice(-maxTimelineLength),
        tokenUsage: session.tokenUsage || { input: 0, output: 0 }
      }

      if (
        restoredSession.mode === "external" &&
        externalTransientStates.has(restoredSession.state)
      ) {
        restoredSession.state = "idle"
        restoredSession.activeTools = restoredSession.activeTools.map(tool =>
          tool.state === "running"
            ? {
                ...tool,
                state: "completed",
                completedAt:
                  tool.completedAt || restoredSession.lastActivityAt || Date.now()
              }
            : tool
        )
      }

      this.sessions.set(session.id, restoredSession)
    }
  }

  applyEvent(event) {
    const sessionId = event.sessionId

    if (!sessionId) {
      return null
    }

    let session = this.sessions.get(sessionId)

    if (!session) {
      session = createSession(event)
      this.sessions.set(sessionId, session)
    }

    const patch = this.reduceSession(session, event)
    const timelineItem = this.createTimelineItem(sessionId, event)

    session.timeline.push(timelineItem)
    if (session.timeline.length > maxTimelineLength) {
      session.timeline.shift()
    }

    this.sequence += 1

    return {
      sequence: this.sequence,
      sessionId,
      event: timelineItem,
      patch
    }
  }

  reduceSession(session, event) {
    const now = event.timestamp || Date.now()
    const patch = {
      lastActivityAt: now
    }

    if (event.cwd !== undefined) {
      session.cwd = event.cwd
      patch.cwd = event.cwd
    }

    if (event.title) {
      session.title = event.title
      patch.title = event.title
    }

    if (event.model) {
      session.model = event.model
      patch.model = event.model
    }

    if (event.mode) {
      session.mode = event.mode
      patch.mode = event.mode
    }

    if (event.startedAt) {
      session.startedAt = event.startedAt
      patch.startedAt = event.startedAt
    }

    this.applyTypedEvent(session, event, patch)
    session.lastActivityAt = patch.lastActivityAt

    return patch
  }

  applyTypedEvent(session, event, patch) {
    if (event.type === "SESSION_RESUMED") {
      if (session.mode === "external") {
        if (Array.isArray(event.payload?.chatMessages)) {
          session.chatMessages = event.payload.chatMessages.slice(
            -maxChatMessageLength
          )
          patch.chatMessages = session.chatMessages
        }
        session.activeTools = session.activeTools.map(tool =>
          tool.state === "running"
            ? {
                ...tool,
                state: "completed",
                completedAt: tool.completedAt || event.timestamp || Date.now()
              }
            : tool
        )
        session.state = "idle"
        patch.activeTools = session.activeTools
        patch.state = session.state
      }
      return
    }

    if (event.type === "STREAM_STARTED") {
      session.state = "streaming"
      patch.state = session.state
      return
    }

    if (event.type === "STREAM_DELTA") {
      session.state = session.state === "idle" ? "streaming" : session.state
      patch.state = session.state
      patch.streamDelta = event.payload?.text || ""
      this.appendChatMessage(session, patch, {
        role: event.payload?.role || "assistant",
        source: event.payload?.source || "stream",
        text: event.payload?.text || "",
        timestamp: event.timestamp || Date.now()
      })
      return
    }

    if (event.type === "STREAM_COMPLETED") {
      session.state = "completed"
      patch.state = session.state
      return
    }

    if (event.type === "WAITING_USER") {
      session.state = "waiting_user"
      patch.state = session.state
      this.appendChatMessage(session, patch, {
        role: event.payload?.role || "user",
        source: event.payload?.source || "session",
        text: event.payload?.text || "",
        timestamp: event.timestamp || Date.now()
      })
      return
    }

    if (event.type === "APPROVAL_REQUEST") {
      session.state = "waiting_approval"
      patch.state = session.state
      return
    }

    if (event.type === "TOKEN_USAGE") {
      session.tokenUsage = {
        input: Number(event.payload?.input || session.tokenUsage.input || 0),
        output: Number(event.payload?.output || session.tokenUsage.output || 0)
      }
      patch.tokenUsage = session.tokenUsage
      return
    }

    if (event.type === "AGENT_SPAWNED") {
      const agent = {
        id: event.payload?.id || createId([session.id, "agent", Date.now()]),
        inferred: true,
        confidence: Number(event.payload?.confidence || 70),
        source: event.payload?.source || "stdout",
        title: event.payload?.title || "Agent",
        startedAt: event.timestamp || Date.now()
      }
      session.agents = [...session.agents, agent]
      session.state = "background_agents"
      patch.agents = session.agents
      patch.state = session.state
      return
    }

    if (event.type === "TOOL_STARTED") {
      const tool = {
        id: event.payload?.id || createId([session.id, event.payload?.name, Date.now()]),
        name: event.payload?.name || "tool",
        state: "running",
        startedAt: event.timestamp || Date.now()
      }
      session.activeTools = [...session.activeTools, tool]
      session.state = "running_tools"
      patch.activeTools = session.activeTools
      patch.state = session.state
      return
    }

    if (event.type === "TOOL_COMPLETED" || event.type === "TOOL_FAILED") {
      const name = event.payload?.name || ""
      session.activeTools = session.activeTools.map(tool => {
        if (event.payload?.id && tool.id !== event.payload.id) {
          return tool
        }

        if (!event.payload?.id && name && tool.name !== name) {
          return tool
        }

        return {
          ...tool,
          state: event.type === "TOOL_FAILED" ? "failed" : "completed",
          completedAt: event.timestamp || Date.now()
        }
      })
      session.state = session.activeTools.some(tool => tool.state === "running")
        ? "running_tools"
        : "streaming"
      patch.activeTools = session.activeTools
      patch.state = session.state
      return
    }

    if (event.type === "RUNTIME_ERROR") {
      session.state = "error"
      patch.state = session.state
      patch.error = event.payload?.message || ""
    }
  }

  appendChatMessage(session, patch, input) {
    const text = String(input.text || "")

    if (!text.trim()) {
      return
    }

    const lastMessage = session.chatMessages[session.chatMessages.length - 1]

    if (
      lastMessage &&
      lastMessage.role === input.role &&
      lastMessage.source === input.source &&
      input.source === "stdout"
    ) {
      session.chatMessages = [
        ...session.chatMessages.slice(0, -1),
        {
          ...lastMessage,
          text: `${lastMessage.text}${text}`.slice(-12000),
          updatedAt: input.timestamp
        }
      ].slice(-maxChatMessageLength)
    } else {
      session.chatMessages = [
        ...session.chatMessages,
        {
          id: createId([
            session.id,
            input.role,
            input.source,
            input.timestamp,
            session.chatMessages.length
          ]),
          role: input.role,
          source: input.source,
          text,
          createdAt: input.timestamp,
          updatedAt: input.timestamp
        }
      ].slice(-maxChatMessageLength)
    }

    patch.chatMessages = session.chatMessages
  }

  createTimelineItem(sessionId, event) {
    return {
      id: event.id || createId([sessionId, event.type, event.timestamp || Date.now(), Math.random()]),
      sessionId,
      type: event.type,
      timestamp: event.timestamp || Date.now(),
      payload: event.payload || {}
    }
  }

  getSnapshot() {
    return {
      sequence: this.sequence,
      sessions: Array.from(this.sessions.values()).sort(
        (left, right) => right.lastActivityAt - left.lastActivityAt
      )
    }
  }

  getSession(sessionId) {
    return this.sessions.get(sessionId) || null
  }

  getActiveSessions() {
    const now = Date.now()

    return Array.from(this.sessions.values()).filter(session => {
      return now - Number(session.lastActivityAt || 0) < 5 * 60 * 1000
    })
  }

  removeSession(sessionId) {
    this.sessions.delete(sessionId)
  }
}

module.exports = {
  RuntimeRegistry
}

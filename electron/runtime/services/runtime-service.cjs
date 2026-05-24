const { EventEmitter } = require("node:events")
const path = require("node:path")
const chokidar = require("chokidar")
const { PtyManager } = require("../pty/pty-manager.cjs")
const { TailReader } = require("../session/tail-reader.cjs")
const { JsonlParser } = require("../session/jsonl-parser.cjs")
const {
  createSessionEventFromFile,
  createSessionId,
  inferEventFromRecord
} = require("../session/session-recovery.cjs")
const { RuntimeRegistry } = require("../state/runtime-registry.cjs")

class RuntimeService extends EventEmitter {
  constructor() {
    super()
    this.storage = null
    this.watcher = null
    this.watchTimer = null
    this.registry = new RuntimeRegistry()
    this.ptyManager = new PtyManager()
    this.tailReader = new TailReader()
    this.jsonlParser = new JsonlParser()
    this.pendingDeltas = new Map()
    this.deltaTimer = null
  }

  bindStorage(storage) {
    this.storage = storage
  }

  async init() {
    const snapshot = await this.storage.read("runtimeSnapshots", {
      sequence: 0,
      sessions: []
    })
    const offsets = await this.storage.read("runtimeOffsets", {})

    this.registry.restore(snapshot)
    this.tailReader = new TailReader(offsets)
  }

  startWatcher(cliTargets) {
    this.stopWatcher()

    const watchPaths = Array.from(
      new Set(
        cliTargets
          .filter(item => item.installed && (item.id === "codex" || item.cli === "codex"))
          .flatMap(item => {
            if (Array.isArray(item.sessionPaths) && item.sessionPaths.length) {
              return item.sessionPaths
            }

            return [item.sessionsPath].filter(Boolean)
          })
      )
    )

    if (!watchPaths.length) {
      return
    }

    this.watcher = chokidar.watch(watchPaths, {
      ignoreInitial: false,
      depth: 5,
      followSymlinks: false
    })

    this.watcher.on("add", filePath => {
      this.handleSessionFile(filePath, true)
    })
    this.watcher.on("change", filePath => {
      clearTimeout(this.watchTimer)
      this.watchTimer = setTimeout(() => {
        this.handleSessionFile(filePath, false)
      }, 80)
    })
  }

  async handleSessionFile(filePath, firstRead) {
    try {
      const isJsonl = path.extname(filePath) === ".jsonl"

      if (firstRead) {
        const event = await createSessionEventFromFile(filePath)

        this.applyEvent(event)
        if (isJsonl) {
          this.tailReader.setOffset(filePath, event.payload.size)
          this.persistOffsets()
        }
        return
      }

      if (!isJsonl) {
        return
      }

      const chunk = await this.tailReader.readIncrement(filePath)

      if (!chunk) {
        this.persistOffsets()
        return
      }

      const records = this.jsonlParser.feed(filePath, chunk)

      for (const record of records) {
        const event = inferEventFromRecord(filePath, record)

        if (event) {
          this.applyEvent(event)
        }
      }

      this.persistOffsets()
    } catch (error) {
      this.applyEvent({
        sessionId: createSessionId(filePath),
        type: "RUNTIME_ERROR",
        timestamp: Date.now(),
        payload: {
          message: error.message
        }
      })
    }
  }

  startManagedRuntime(input = {}) {
    const runtime = this.ptyManager.createRuntime(input)
    const startedAt = Date.now()

    runtime.on("event", event => this.applyEvent(event))
    this.applyEvent({
      sessionId: runtime.sessionId,
      mode: "managed",
      type: "STREAM_STARTED",
      timestamp: startedAt,
      cwd: input.cwd || "",
      title: input.title || "Managed Codex",
      model: input.model || "",
      payload: {
        source: "pty"
      }
    })

    return this.registry.getSession(runtime.sessionId)
  }

  writeRuntime(sessionId, data) {
    this.ptyManager.write(sessionId, data)
    return this.registry.getSession(sessionId)
  }

  stopRuntime(sessionId) {
    const stopped = this.ptyManager.stop(sessionId)

    if (stopped) {
      this.applyEvent({
        sessionId,
        type: "STREAM_COMPLETED",
        timestamp: Date.now(),
        payload: {
          source: "pty"
        }
      })
    }

    return stopped
  }

  applyEvent(event) {
    const delta = this.registry.applyEvent(event)

    if (!delta) {
      return
    }

    this.persistSnapshot()
    this.queueDelta(delta)
  }

  queueDelta(delta) {
    const previous = this.pendingDeltas.get(delta.sessionId)

    this.pendingDeltas.set(delta.sessionId, {
      ...delta,
      events: [...(previous?.events || []), delta.event].filter(Boolean),
      patch: {
        ...(previous?.patch || {}),
        ...(delta.patch || {})
      }
    })

    if (!this.deltaTimer) {
      this.deltaTimer = setTimeout(() => {
        this.flushDeltas()
      }, 16)
    }
  }

  flushDeltas() {
    const deltas = Array.from(this.pendingDeltas.values())

    this.pendingDeltas.clear()
    this.deltaTimer = null

    for (const delta of deltas) {
      this.emit("delta", delta)
    }
  }

  persistSnapshot() {
    this.storage.scheduleWrite("runtimeSnapshots", this.registry.getSnapshot())
  }

  persistOffsets() {
    this.storage.scheduleWrite("runtimeOffsets", this.tailReader.getOffsets())
  }

  getSnapshot() {
    return this.registry.getSnapshot()
  }

  getActiveSessions() {
    return this.registry.getActiveSessions()
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
    clearTimeout(this.deltaTimer)
    this.deltaTimer = null
    this.pendingDeltas.clear()
    this.ptyManager.dispose()
    await this.storage.flush()
  }
}

module.exports = {
  RuntimeService
}

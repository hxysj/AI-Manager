const crypto = require("node:crypto")
const { CodexProcess } = require("./codex-process.cjs")

function loadPty() {
  try {
    return require("node-pty")
  } catch (error) {
    throw new Error(`node-pty 不可用：${error.message}`)
  }
}

function createRuntimeId(parts) {
  return crypto
    .createHash("sha1")
    .update(parts.join("|"))
    .digest("hex")
    .slice(0, 16)
}

class PtyManager {
  constructor() {
    this.processes = new Map()
  }

  createRuntime(input = {}) {
    const pty = loadPty()
    const cwd = input.cwd || process.cwd()
    const sessionId =
      input.sessionId || createRuntimeId([cwd, Date.now(), Math.random()])
    const proc = pty.spawn(input.command || "codex", input.args || [], {
      cwd,
      cols: Number(input.cols || 120),
      rows: Number(input.rows || 40),
      env: {
        ...process.env,
        ...(input.env || {})
      }
    })
    const runtime = new CodexProcess({
      sessionId,
      cwd,
      proc
    })

    this.processes.set(sessionId, runtime)
    runtime.on("exit", () => {
      this.processes.delete(sessionId)
    })

    return runtime
  }

  write(sessionId, data) {
    const runtime = this.processes.get(sessionId)

    if (!runtime) {
      throw new Error("Runtime 不存在")
    }

    runtime.write(data)
  }

  stop(sessionId) {
    const runtime = this.processes.get(sessionId)

    if (!runtime) {
      return false
    }

    runtime.kill()
    this.processes.delete(sessionId)
    return true
  }

  dispose() {
    for (const runtime of this.processes.values()) {
      runtime.kill()
    }

    this.processes.clear()
  }
}

module.exports = {
  PtyManager
}

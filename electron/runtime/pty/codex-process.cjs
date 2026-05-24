const { EventEmitter } = require("node:events")
const { StreamParser } = require("./stream-parser.cjs")
const { TerminalBuffer } = require("./terminal-buffer.cjs")

class CodexProcess extends EventEmitter {
  constructor(input) {
    super()
    this.sessionId = input.sessionId
    this.cwd = input.cwd
    this.proc = input.proc
    this.parser = new StreamParser(this.sessionId)
    this.buffer = new TerminalBuffer()

    this.parser.on("event", event => this.emit("event", event))
    this.bind()
  }

  bind() {
    this.proc.onData(chunk => {
      this.buffer.push(chunk)
      this.parser.feed(chunk)
    })

    this.proc.onExit(event => {
      this.emit("event", {
        sessionId: this.sessionId,
        type: event.exitCode === 0 ? "STREAM_COMPLETED" : "RUNTIME_ERROR",
        timestamp: Date.now(),
        payload: {
          exitCode: event.exitCode,
          signal: event.signal,
          message: event.exitCode === 0 ? "" : `Codex 已退出：${event.exitCode}`
        }
      })
      this.emit("exit", event)
    })
  }

  write(data) {
    this.proc.write(String(data || ""))
  }

  resize(cols, rows) {
    this.proc.resize(Number(cols || 120), Number(rows || 40))
  }

  kill() {
    this.proc.kill()
  }

  getOutput() {
    return this.buffer.getContent()
  }
}

module.exports = {
  CodexProcess
}

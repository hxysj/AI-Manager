class TerminalBuffer {
  constructor(limit = 20000) {
    this.limit = limit
    this.content = ""
  }

  push(chunk) {
    this.content = `${this.content}${chunk}`

    if (this.content.length > this.limit) {
      this.content = this.content.slice(this.content.length - this.limit)
    }
  }

  getContent() {
    return this.content
  }
}

module.exports = {
  TerminalBuffer
}

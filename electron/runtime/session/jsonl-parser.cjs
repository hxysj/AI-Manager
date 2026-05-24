class JsonlParser {
  constructor() {
    this.pendingLines = new Map()
  }

  feed(filePath, chunk) {
    const content = `${this.pendingLines.get(filePath) || ""}${chunk}`
    const lines = content.split(/\r?\n/)

    this.pendingLines.set(filePath, lines.pop() || "")

    return lines
      .map(line => line.trim())
      .filter(Boolean)
      .map(line => JSON.parse(line))
  }
}

module.exports = {
  JsonlParser
}

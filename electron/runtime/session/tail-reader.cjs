const fs = require("node:fs/promises")

class TailReader {
  constructor(offsets = {}) {
    this.offsets = new Map(Object.entries(offsets))
  }

  async readIncrement(filePath) {
    const stat = await fs.stat(filePath)
    const previousOffset = Number(this.offsets.get(filePath) || 0)
    const offset = previousOffset > stat.size ? 0 : previousOffset

    if (stat.size === offset) {
      return ""
    }

    const handle = await fs.open(filePath, "r")
    const buffer = Buffer.alloc(stat.size - offset)

    try {
      await handle.read(buffer, 0, buffer.length, offset)
      this.offsets.set(filePath, stat.size)
      return buffer.toString("utf8")
    } finally {
      await handle.close()
    }
  }

  setOffset(filePath, offset) {
    this.offsets.set(filePath, Number(offset || 0))
  }

  getOffsets() {
    return Object.fromEntries(this.offsets.entries())
  }
}

module.exports = {
  TailReader
}

const fs = require("node:fs/promises")
const crypto = require("node:crypto")

class JsonStorage {
  constructor(storageFiles, delay = 300) {
    this.storageFiles = storageFiles
    this.delay = delay
    this.pendingPayloads = new Map()
    this.timers = new Map()
  }

  async read(key, fallback) {
    const filePath = this.storageFiles[key]

    if (!filePath) {
      throw new Error(`Unknown storage key: ${key}`)
    }

    try {
      const content = await fs.readFile(filePath, "utf8")
      return JSON.parse(content)
    } catch (error) {
      if (error.code === "ENOENT") {
        return fallback
      }

      throw error
    }
  }

  async writeNow(key, payload) {
    const filePath = this.storageFiles[key]

    if (!filePath) {
      throw new Error(`Unknown storage key: ${key}`)
    }

    const content = JSON.stringify(payload, null, 2)
    const tempPath = `${filePath}.${crypto.randomUUID()}.tmp`

    try {
      await fs.writeFile(tempPath, `${content}\n`, "utf8")
      await fs.rename(tempPath, filePath)
    } catch (error) {
      await fs.rm(tempPath, { force: true })
      throw error
    }
  }

  scheduleWrite(key, payload) {
    this.pendingPayloads.set(key, payload)

    if (this.timers.has(key)) {
      clearTimeout(this.timers.get(key))
    }

    this.timers.set(
      key,
      setTimeout(async () => {
        const nextPayload = this.pendingPayloads.get(key)
        this.pendingPayloads.delete(key)
        this.timers.delete(key)
        await this.writeNow(key, nextPayload)
      }, this.delay)
    )
  }

  async flush() {
    const entries = Array.from(this.pendingPayloads.entries())

    for (const [key, payload] of entries) {
      if (this.timers.has(key)) {
        clearTimeout(this.timers.get(key))
        this.timers.delete(key)
      }

      this.pendingPayloads.delete(key)
      await this.writeNow(key, payload)
    }
  }
}

module.exports = {
  JsonStorage
}

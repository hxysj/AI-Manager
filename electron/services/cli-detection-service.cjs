const { createCliAdapters } = require("./cli-adapters.cjs")

class CliDetectionService {
  constructor() {
    this.adapters = createCliAdapters()
  }

  async detectAll() {
    return Promise.all(this.adapters.map((adapter) => adapter.detect()))
  }

  getAdapter(targetId) {
    const adapter = this.adapters.find((item) => item.id === targetId)

    if (!adapter) {
      throw new Error(`Unsupported CLI target: ${targetId}`)
    }

    return adapter
  }
}

module.exports = {
  CliDetectionService
}

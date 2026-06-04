const path = require("node:path")
const crypto = require("node:crypto")
const { Worker } = require("node:worker_threads")

class TranslationService {
  constructor(userDataPath) {
    this.cacheDir = path.join(userDataPath, "models", "transformers")
    this.worker = null
    this.pendingRequests = new Map()
  }

  async translate(text) {
    const sourceText = String(text || "").trim()

    if (!sourceText) {
      throw new Error("没有可翻译的文本")
    }

    return {
      sourceText,
      translatedText: await this.requestTranslate(sourceText)
    }
  }

  requestTranslate(text) {
    const worker = this.getWorker()
    const id = crypto.randomUUID()

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, {
        resolve,
        reject
      })
      worker.postMessage({
        id,
        text
      })
    })
  }

  getWorker() {
    if (this.worker) {
      return this.worker
    }

    this.worker = new Worker(path.join(__dirname, "translation-worker.cjs"), {
      workerData: {
        cacheDir: this.cacheDir
      }
    })
    this.worker.on("message", payload => {
      const request = this.pendingRequests.get(payload.id)

      if (!request) {
        return
      }

      this.pendingRequests.delete(payload.id)

      if (payload.status === "error") {
        request.reject(new Error(payload.message || "翻译失败"))
        return
      }

      request.resolve(payload.translatedText || "")
    })
    this.worker.on("error", error => {
      this.rejectPendingRequests(error)
      this.worker = null
    })
    this.worker.on("exit", code => {
      if (code !== 0) {
        this.rejectPendingRequests(new Error(`翻译 Worker 已退出：${code}`))
      }

      this.worker = null
    })

    return this.worker
  }

  rejectPendingRequests(error) {
    for (const request of this.pendingRequests.values()) {
      request.reject(error)
    }

    this.pendingRequests.clear()
  }

  async dispose() {
    this.rejectPendingRequests(new Error("翻译服务已关闭"))

    if (this.worker) {
      await this.worker.terminate()
      this.worker = null
    }
  }
}

module.exports = {
  TranslationService
}

const path = require('node:path')

class TranslationService {
  constructor(userDataPath) {
    this.cacheDir = path.join(userDataPath, 'models', 'transformers')
    this.translatorPromise = null
  }

  async translate(text) {
    const sourceText = String(text || '').trim()

    if (!sourceText) {
      throw new Error('没有可翻译的文本')
    }

    const translator = await this.getTranslator()
    const result = await translator(sourceText.slice(0, 1200))

    return {
      sourceText,
      translatedText: result[0]?.translation_text || ''
    }
  }

  async getTranslator() {
    if (!this.translatorPromise) {
      this.translatorPromise = this.createTranslator()
    }

    return this.translatorPromise
  }

  async createTranslator() {
    const { env, pipeline } = await import('@xenova/transformers')

    env.cacheDir = this.cacheDir
    env.remoteHost =
      process.env.AI_MANAGER_HF_ENDPOINT || 'https://hf-mirror.com/'
    env.allowLocalModels = true
    env.allowRemoteModels = true

    return pipeline('translation_en_to_zh', 'Xenova/opus-mt-en-zh')
  }
}

module.exports = {
  TranslationService
}

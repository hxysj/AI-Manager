import path from 'node:path'
import { env, pipeline } from '@xenova/transformers'

const payload = JSON.parse(process.argv[2] || '{}')
const sourceText = String(payload.text || '').trim()

if (!sourceText) {
  throw new Error('没有可翻译的文本')
}

env.cacheDir = path.join(String(payload.userDataPath || ''), 'models', 'transformers')
env.remoteHost = process.env.AI_MANAGER_HF_ENDPOINT || 'https://hf-mirror.com/'
env.allowLocalModels = true
env.allowRemoteModels = true

const translator = await pipeline('translation_en_to_zh', 'Xenova/opus-mt-en-zh')
const result = await translator(sourceText.slice(0, 1200))

process.stdout.write(
  JSON.stringify({
    sourceText,
    translatedText: result[0]?.translation_text || ''
  })
)

const { parentPort, workerData } = require("node:worker_threads")

let translatorPromise = null

async function getTranslator() {
  if (!translatorPromise) {
    translatorPromise = createTranslator()
  }

  return translatorPromise
}

async function createTranslator() {
  const { env, pipeline } = await import("@xenova/transformers")

  env.cacheDir = workerData.cacheDir
  env.remoteHost =
    process.env.AI_MANAGER_HF_ENDPOINT || "https://hf-mirror.com/"
  env.allowLocalModels = true
  env.allowRemoteModels = true
  env.backends.onnx.logLevel = "error"

  return pipeline("translation_en_to_zh", "Xenova/opus-mt-en-zh")
}

parentPort.on("message", async payload => {
  try {
    const translator = await getTranslator()
    const result = await translator(String(payload.text || "").slice(0, 1200))

    parentPort.postMessage({
      id: payload.id,
      status: "success",
      translatedText: result[0]?.translation_text || ""
    })
  } catch (error) {
    parentPort.postMessage({
      id: payload.id,
      status: "error",
      message: error.message || String(error)
    })
  }
})

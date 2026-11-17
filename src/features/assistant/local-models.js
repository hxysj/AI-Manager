import { onBeforeUnmount, onMounted, ref } from "vue"
import { localModelsApi } from "@/api/modules/local-models"
import { subscribeReady } from "@/api/request"

// 配置页与两个对话入口共用已安装目录，下载或删除后自动更新，卸载后不再接收回包。
export function useAssistantLocalModels() {
  const localModels = ref([])
  const localModelsError = ref("")
  let disposed = false
  let sequence = 0
  let unsubscribe
  async function load() {
    const current = ++sequence
    try {
      const result = await localModelsApi.installed("assistant")
      if (disposed || current !== sequence) return
      localModels.value = result.models
      localModelsError.value = ""
    } catch (error) {
      if (disposed || current !== sequence) return
      localModels.value = []
      localModelsError.value = `本地助手模型读取失败：${error.message || error}`
    }
  }
  onMounted(async () => {
    try {
      const stop = await subscribeReady("local-models:changed", load)
      if (disposed) return stop()
      unsubscribe = stop
      await load()
    } catch (error) {
      if (!disposed) localModelsError.value = String(error)
    }
  })
  onBeforeUnmount(() => {
    disposed = true
    unsubscribe?.()
  })
  return { localModels, localModelsError }
}

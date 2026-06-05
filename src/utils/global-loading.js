import { readonly, ref } from 'vue'

const loading = ref(false)

let loadingCount = 0

function showGlobalLoading() {
  loadingCount += 1
  loading.value = true
}

function hideGlobalLoading() {
  loadingCount = Math.max(loadingCount - 1, 0)

  if (!loadingCount) {
    loading.value = false
  }
}

async function withGlobalLoading(action) {
  showGlobalLoading()

  try {
    return await action()
  } finally {
    hideGlobalLoading()
  }
}

export function useGlobalLoading() {
  return {
    loading: readonly(loading),
    showGlobalLoading,
    hideGlobalLoading,
    withGlobalLoading
  }
}

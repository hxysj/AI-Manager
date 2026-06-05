<template>
  <aside
    v-if="visible"
    class="selection-translator"
    :style="{ left: `${position.x}px`, top: `${position.y}px` }"
  >
    <header class="selection-translator__header">
      <strong>划词翻译</strong>
      <button type="button" @click="closeTranslator">×</button>
    </header>

    <div class="selection-translator__body">
      <p class="selection-translator__source">{{ sourceText }}</p>

      <div v-if="loading" class="selection-translator__loading">
        正在调用本地翻译模型...
      </div>

      <p v-else-if="errorMessage" class="selection-translator__error">
        {{ errorMessage }}
      </p>

      <p v-else class="selection-translator__result">
        {{ translatedText }}
      </p>
    </div>
  </aside>
</template>

<script setup>
import { onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'

const props = defineProps({
  activeView: {
    type: String,
    default: ''
  }
})

const visible = ref(false)
const loading = ref(false)
const sourceText = ref('')
const translatedText = ref('')
const errorMessage = ref('')
const position = reactive({
  x: 420,
  y: 120
})
const translatorWidth = 360
const translatorMaxHeight = 420
const viewportPadding = 18

let unsubscribe = null
let requestId = 0

onMounted(() => {
  unsubscribe = window.aiManager.onTranslateSelection(payload => {
    showTranslator(payload)
  })
})

onBeforeUnmount(() => {
  if (typeof unsubscribe === 'function') {
    unsubscribe()
  }
})

watch(
  () => props.activeView,
  () => {
    closeTranslator()
  }
)

async function showTranslator(payload) {
  const currentRequestId = requestId + 1

  requestId = currentRequestId
  sourceText.value = payload.text
  translatedText.value = ''
  errorMessage.value = ''
  loading.value = true
  visible.value = true
  updatePosition(payload)

  try {
    const result = await window.aiManager.translateText({ text: payload.text })
    if (currentRequestId !== requestId || !visible.value) {
      return
    }

    translatedText.value = result.translatedText
  } catch (error) {
    if (currentRequestId !== requestId || !visible.value) {
      return
    }

    errorMessage.value = error.message || String(error)
  } finally {
    if (currentRequestId === requestId) {
      loading.value = false
    }
  }
}

function updatePosition(payload) {
  const maxX = window.innerWidth - translatorWidth - viewportPadding
  const maxY = window.innerHeight - translatorMaxHeight - viewportPadding

  position.x = Math.min(Math.max(viewportPadding, payload.x), maxX)
  position.y = Math.min(Math.max(viewportPadding, payload.y), maxY)
}

function closeTranslator() {
  requestId += 1
  visible.value = false
  loading.value = false
}
</script>

<style scoped lang="less">
.selection-translator {
  position: fixed;
  z-index: 80;
  width: 360px;
  max-height: 420px;
  overflow: hidden;
  border: 1px solid #c7d3e2;
  border-radius: 8px;
  background: #ffffff;
  box-shadow: 0 18px 42px rgba(34, 56, 83, 0.18);

  &__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-line);
    background: #f7f9fc;
  }

  &__header strong {
    color: var(--color-text);
    font-size: 0.92rem;
  }

  &__header button {
    display: grid;
    width: 26px;
    height: 26px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: #ffffff;
    color: var(--color-text-muted);
    cursor: pointer;
    line-height: 1;
  }

  &__body {
    display: flex;
    max-height: 360px;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    padding: 14px;
  }

  &__source {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.82rem;
    line-height: 1.55;
  }

  &__loading,
  &__error,
  &__result {
    margin: 0;
    padding-top: 10px;
    border-top: 1px solid var(--color-line);
    line-height: 1.7;
  }

  &__loading {
    color: var(--color-text-soft);
  }

  &__error {
    color: var(--color-danger);
  }

  &__result {
    color: var(--color-text);
  }
}
</style>

<template>
  <el-input
    class="provider-api-key-input"
    :model-value="modelValue || savedValue"
    :type="visible ? 'text' : 'password'"
    :disabled="disabled"
    autocomplete="new-password"
    spellcheck="false"
    aria-label="API Key"
    @update:model-value="updateValue"
  >
    <template v-if="$slots.prefix" #prefix><slot name="prefix" /></template>
    <template #suffix>
      <button
        class="provider-key-visibility"
        type="button"
        :disabled="disabled || loading || !canReveal"
        :aria-label="loading ? '读取 API Key 中' : visible ? '隐藏 API Key' : '查看 API Key'"
        :title="loading ? '读取 API Key 中' : visible ? '隐藏 API Key' : '查看 API Key'"
        :aria-pressed="visible"
        :aria-busy="loading"
        @mousedown.prevent
        @click="toggleVisibility"
      >
        <LoaderCircle v-if="loading" class="provider-key-loading" :size="16" />
        <EyeOff v-else-if="visible" :size="16" />
        <Eye v-else :size="16" />
      </button>
    </template>
  </el-input>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { Eye, EyeOff, LoaderCircle } from 'lucide-vue-next'
import { providerApi } from '@/api'
import { createMessage } from '@/utils/message'

const props = defineProps({
  modelValue: { type: String, default: '' },
  providerId: { type: String, default: '' },
  keyId: { type: String, default: '' },
  cli: { type: String, default: '' },
  masked: { type: String, default: '' },
  disabled: { type: Boolean, default: false }
})
const emit = defineEmits(['update:modelValue'])
const visible = ref(false)
const loading = ref(false)
const savedValue = ref('')
let requestVersion = 0
const canReveal = computed(() => Boolean(
  props.modelValue || (props.providerId && props.keyId && props.masked)
))

watch(() => [props.providerId, props.keyId, props.cli, props.masked, props.disabled], () => {
  visible.value = false
  clearRead()
}, { flush: 'sync' })
watch(() => props.modelValue, clearRead, { flush: 'sync' })
onBeforeUnmount(clearRead)

function clearRead() {
  requestVersion += 1
  savedValue.value = ''
  loading.value = false
}

function updateValue(value) {
  clearRead()
  emit('update:modelValue', value)
}

async function toggleVisibility() {
  if (props.disabled || loading.value || !canReveal.value) return
  if (visible.value) {
    visible.value = false
    clearRead()
    return
  }
  if (props.modelValue) {
    visible.value = true
    return
  }
  const version = ++requestVersion
  loading.value = true
  try {
    const result = await providerApi.getKeyValue({
      providerId: props.providerId,
      keyId: props.keyId,
      cli: props.cli
    })
    if (version !== requestVersion) return
    if (!result?.apiKey) throw new Error('API Key 为空')
    savedValue.value = result.apiKey
    visible.value = true
  } catch {
    if (version === requestVersion) createMessage.error('读取 API Key 失败，请重试。')
  } finally {
    if (version === requestVersion) loading.value = false
  }
}
</script>

<style scoped lang="less">
.provider-api-key-input {
  .provider-key-visibility {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    &:hover:not(:disabled) {
      background: var(--color-primary-soft);
      color: var(--color-primary);
    }
    &:focus-visible {
      outline: 2px solid var(--color-primary);
      outline-offset: 2px;
    }
    &:disabled {
      cursor: default;
      opacity: 0.5;
    }
    .provider-key-loading {
      animation: provider-key-spin 1s linear infinite;
    }
  }
}
@keyframes provider-key-spin {
  to { transform: rotate(360deg); }
}
</style>

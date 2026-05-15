<template>
  <img v-if="iconUrl" :class="className" :src="iconUrl" :alt="alt" />
</template>

<script setup>
import { ref, watch } from 'vue'

const props = defineProps({
  alt: {
    type: String,
    default: ''
  },
  className: {
    type: String,
    default: ''
  },
  name: {
    type: String,
    default: ''
  }
})

const iconUrl = ref('')
const iconModules = import.meta.glob('/src/assets/ai-icons/*.svg', {
  query: '?url',
  import: 'default'
})

watch(
  () => props.name,
  async name => {
    iconUrl.value = ''

    if (!name) {
      return
    }

    const iconName = name.endsWith('.svg') ? name : `${name}.svg`
    const loader = iconModules[`/src/assets/ai-icons/${iconName}`]

    if (loader) {
      iconUrl.value = await loader()
    }
  },
  { immediate: true }
)
</script>

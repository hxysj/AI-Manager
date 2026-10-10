<template>
  <div class="desktop-pet">
    <img
      v-if="animation"
      :key="`${pet.id}:${animation.id}`"
      class="desktop-pet-animation"
      :src="animation.url"
      :alt="pet.name"
      draggable="false"
      @load="scheduleAnimation"
    />
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"

const props = defineProps({
  pet: { type: Object, required: true },
  dragging: { type: Boolean, default: false },
  action: { type: String, default: "" }
})
const animationIndex = ref(0)
const animation = computed(() => {
  // 管理页固定播放指定动作，悬浮窗继续自动轮换。
  if (props.action)
    return props.pet.animations.find((item) => item.id === props.action)
  if (props.dragging) {
    const walk = props.pet.animations.find((item) => item.id === "walk")
    if (walk) return walk
  }
  return props.pet.animations[animationIndex.value]
})
let animationTimer = null
let disposed = false

// WebP 原生循环播放；按完整动作周期轮换，避免中途截断动画。
function scheduleAnimation() {
  clearTimeout(animationTimer)
  if (
    disposed ||
    props.action ||
    props.dragging ||
    props.pet.animations.length < 2
  )
    return
  const duration = animation.value.duration
  animationTimer = setTimeout(
    rotateAnimation,
    duration * Math.max(2, Math.ceil(4000 / duration))
  )
}

function rotateAnimation() {
  if (disposed || props.action || props.dragging) return
  animationIndex.value =
    (animationIndex.value + 1) % props.pet.animations.length
}

watch(
  () => props.pet.id,
  () => {
    clearTimeout(animationTimer)
    animationIndex.value = 0
  }
)
watch(animation, () => clearTimeout(animationTimer))
// 正好轮换到行走动作时，拖动前后图片不变，也要恢复轮换计时。
watch(
  () => [props.dragging, props.action],
  () => {
    clearTimeout(animationTimer)
    scheduleAnimation()
  }
)
onBeforeUnmount(() => {
  disposed = true
  clearTimeout(animationTimer)
})
</script>

<style scoped lang="less">
.desktop-pet {
  display: flex;
  width: 100%;
  height: 100%;
  align-items: center;
  justify-content: center;
  .desktop-pet-animation {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
    user-select: none;
  }
}
</style>

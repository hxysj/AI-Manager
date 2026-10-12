<template>
  <div class="desktop-pet">
    <svg
      v-if="animation"
      :key="`${pet.id}:${animation.id}`"
      class="desktop-pet-animation"
      :viewBox="`0 0 ${animation.frameWidth} ${animation.frameHeight}`"
      role="img"
      :aria-label="pet.name"
    >
      <!-- 墙壁使用固定图层，逐帧只更新墙右侧的角色。 -->
      <svg
        v-if="animation.staticFrame"
        class="desktop-pet-static-frame"
        x="0"
        :y="animation.frameHeight - animation.staticFrame.height"
        :width="animation.staticFrame.width"
        :height="animation.staticFrame.height"
        :viewBox="`${animation.staticFrame.x} ${animation.staticFrame.y} ${animation.staticFrame.width} ${animation.staticFrame.height}`"
        overflow="hidden"
      >
        <image
          :href="animation.url"
          :width="animation.sheetWidth"
          :height="animation.sheetHeight"
        />
      </svg>
      <svg
        class="desktop-pet-frame"
        :x="
          animation.staticFrame
            ? animation.staticFrame.width
            : (animation.frameWidth - frame.width) / 2
        "
        :y="animation.frameHeight - frame.height"
        :width="frame.width"
        :height="frame.height"
        :viewBox="`${frame.x} ${frame.y} ${frame.width} ${frame.height}`"
        overflow="hidden"
      >
        <image
          class="desktop-pet-sheet"
          :href="animation.url"
          :width="animation.sheetWidth"
          :height="animation.sheetHeight"
          @load="startAnimation"
        />
      </svg>
    </svg>
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from "vue"

const props = defineProps({
  pet: { type: Object, required: true },
  dragging: { type: Boolean, default: false },
  action: { type: String, default: "" },
  paused: { type: Boolean, default: false }
})
const animationIndex = ref(0)
const frameIndex = ref(0)
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
const frame = computed(() => animation.value.frames[frameIndex.value])
let animationTimer = null
let frameTimer = null
let disposed = false
let imageLoaded = false

// 精灵图只显示当前帧；固定画布和底部对齐避免不同帧缩放跳动。
function startAnimation() {
  imageLoaded = true
  clearInterval(frameTimer)
  if (disposed || props.paused) return
  frameTimer = setInterval(() => {
    frameIndex.value = (frameIndex.value + 1) % animation.value.frames.length
  }, animation.value.frameDuration)
  scheduleAnimation()
}

// 按完整动作周期轮换，避免中途截断动画。
function scheduleAnimation() {
  clearTimeout(animationTimer)
  if (
    disposed ||
    props.paused ||
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
  if (disposed || props.paused || props.action || props.dragging) return
  animationIndex.value =
    (animationIndex.value + 1) % props.pet.animations.length
}

watch(
  () => props.pet.id,
  () => {
    clearTimeout(animationTimer)
    clearInterval(frameTimer)
    animationIndex.value = 0
    frameIndex.value = 0
    imageLoaded = false
  }
)
watch(animation, () => {
  clearTimeout(animationTimer)
  clearInterval(frameTimer)
  frameIndex.value = 0
  imageLoaded = false
})
// 隐藏的预热窗口保留图片，暂停计时；重新显示直接继续播放。
watch(
  () => props.paused,
  (paused) => {
    clearTimeout(animationTimer)
    clearInterval(frameTimer)
    if (!paused && imageLoaded) startAnimation()
  }
)
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
  clearInterval(frameTimer)
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
    pointer-events: none;
    user-select: none;
  }
}
</style>

<template>
  <section
    v-if="currentFile"
    class="attachment-gallery"
    :class="{ 'attachment-gallery-stack': isImageAlbum && !expanded }"
    aria-label="消息附件"
  >
    <div
      v-if="isImageAlbum"
      class="attachment-album"
      :class="{ 'attachment-album-expanded': expanded }"
    >
      <div
        :key="expanded ? 'expanded' : 'stacked'"
        class="attachment-image-list"
        :tabindex="expanded ? undefined : 0"
        :aria-label="
          expanded ? '全部图片' : '图片叠放，可用滚轮或左右方向键切换'
        "
        @wheel="scrollImages"
        @keydown.left="switchImage($event, -1)"
        @keydown.right="switchImage($event, 1)"
      >
        <Transition
          v-for="(entry, index) in visibleImages"
          :key="index"
          name="attachment-stack-card"
          :css="!expanded"
          @before-enter="
            (element) => {
              element.inert = false
            }
          "
          @before-leave="prepareLeavingCard"
        >
          <div
            :key="entry.file.id"
            class="attachment-image-card"
            :style="{
              '--stack-index': index,
              '--stack-direction': switchDirection
            }"
          >
            <button
              class="attachment-image-button"
              type="button"
              :disabled="!entry.previewUrl"
              :aria-label="`查看第 ${entry.position} 张图片`"
              @click="$emit('preview', entry.file)"
            >
              <img
                v-if="entry.previewUrl && !failedImages[entry.file.id]"
                class="attachment-image"
                :src="entry.previewUrl"
                :alt="`第 ${entry.position} 张图片`"
                loading="lazy"
                @error="failedImages[entry.file.id] = true"
              />
              <span v-else class="attachment-image-placeholder">
                <ImageOff :size="24" />
                <span>{{
                  entry.previewUrl ? "图片加载失败" : "暂不可预览"
                }}</span>
              </span>
            </button>
            <a
              v-if="entry.downloadUrl && (expanded || index === 0)"
              class="attachment-download"
              :href="entry.downloadUrl"
              :download="entry.file.name"
              :aria-label="`下载第 ${entry.position} 张图片`"
              title="下载图片"
              ><Download :size="15"
            /></a>
          </div>
        </Transition>
      </div>
      <div class="attachment-album-controls">
        <button
          class="attachment-album-toggle"
          type="button"
          :aria-expanded="expanded"
          @click="expanded = !expanded"
        >
          <ChevronUp v-if="expanded" :size="14" />
          <Layers v-else :size="14" />
          <span>{{ expanded ? "收起" : "展开" }} {{ files.length }} 张</span>
        </button>
        <span
          v-if="!expanded"
          class="attachment-album-position"
          aria-live="polite"
        >
          {{ activeIndex + 1 }} / {{ files.length }} · 滚轮切换
        </span>
      </div>
    </div>
    <template v-else>
      <div
        class="attachment-stage"
        :class="{
          'attachment-stage-media': kind === 'video' || kind === 'audio'
        }"
      >
        <button
          v-if="kind === 'image' && previewUrl && !failed"
          class="attachment-image-button"
          type="button"
          aria-label="放大查看图片"
          @click="$emit('preview', currentFile)"
        >
          <img
            :key="currentFile.id"
            class="attachment-image"
            :src="previewUrl"
            alt="消息图片"
            loading="lazy"
            @error="failed = true"
          />
        </button>
        <video
          v-else-if="kind === 'video' && previewUrl"
          :key="`video-${currentFile.id}`"
          class="attachment-video"
          :src="previewUrl"
          controls
          preload="metadata"
        ></video>
        <audio
          v-else-if="kind === 'audio' && previewUrl"
          :key="`audio-${currentFile.id}`"
          class="attachment-audio"
          :src="previewUrl"
          controls
          preload="metadata"
        ></audio>
        <button
          v-else
          class="attachment-document"
          type="button"
          :disabled="!previewUrl"
          aria-label="打开附件"
          @click="$emit('preview', currentFile)"
        >
          <ImageOff v-if="kind === 'image'" :size="30" :stroke-width="1.4" />
          <FileText v-else :size="30" :stroke-width="1.4" />
          <span class="attachment-document-copy">
            <span class="attachment-document-kind">{{
              kind === "image"
                ? "图片加载失败"
                : kind === "pdf"
                  ? "PDF 文档"
                  : "文件附件"
            }}</span>
            <span class="attachment-document-size">{{
              formatFileSize(currentFile.size)
            }}</span>
          </span>
        </button>
        <a
          v-if="downloadUrl"
          class="attachment-download"
          :href="downloadUrl"
          :download="currentFile.name"
          title="下载附件"
          aria-label="下载附件"
          ><Download :size="15"
        /></a>
      </div>
      <div v-if="files.length > 1" class="attachment-navigation">
        <button
          class="attachment-nav-button"
          type="button"
          aria-label="上一个附件"
          :disabled="activeIndex === 0"
          @click="activeIndex--"
        >
          <ChevronLeft :size="16" />
        </button>
        <span class="attachment-position" aria-live="polite"
          >{{ activeIndex + 1 }} / {{ files.length }}</span
        >
        <button
          class="attachment-nav-button"
          type="button"
          aria-label="下一个附件"
          :disabled="activeIndex === files.length - 1"
          @click="activeIndex++"
        >
          <ChevronRight :size="16" />
        </button>
      </div>
    </template>
  </section>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import {
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Download,
  FileText,
  ImageOff,
  Layers
} from "lucide-vue-next"
import { fileKind, fileUrl, formatFileSize } from "@/features/lanShare/utils"

const props = defineProps({
  files: { type: Array, default: () => [] },
  service: { type: Object, default: () => ({}) },
  sessionId: { type: String, default: "" }
})
defineEmits(["preview"])
const activeIndex = ref(0)
const failed = ref(false)
const expanded = ref(false)
const failedImages = ref({})
const switchDirection = ref(1)
let wheelDistance = 0
let lastWheelAt = 0
let lastSwitchAt = -Infinity
const currentFile = computed(() => props.files[activeIndex.value])
const kind = computed(() => fileKind(currentFile.value))
const isImageAlbum = computed(
  () =>
    props.files.length > 1 &&
    props.files.every((file) => fileKind(file) === "image")
)
const visibleImages = computed(() =>
  Array.from(
    {
      length: expanded.value
        ? props.files.length
        : Math.min(3, props.files.length)
    },
    (unused, offset) => {
      const index = expanded.value
        ? offset
        : (activeIndex.value + offset) % props.files.length
      const file = props.files[index]
      return {
        file,
        position: index + 1,
        previewUrl: fileUrl(props.service, file, props.sessionId),
        downloadUrl: fileUrl(props.service, file, props.sessionId, "download")
      }
    }
  )
)
const previewUrl = computed(() =>
  fileUrl(props.service, currentFile.value, props.sessionId)
)
const downloadUrl = computed(() =>
  fileUrl(props.service, currentFile.value, props.sessionId, "download")
)
watch(
  [
    () => props.files.map((file) => file.id).join(","),
    () => props.sessionId,
    () => props.service.accessUrl,
    () => props.service.running
  ],
  () => {
    activeIndex.value = 0
    failedImages.value = {}
    failed.value = false
    wheelDistance = 0
    lastWheelAt = 0
    lastSwitchAt = -Infinity
  }
)
watch(currentFile, () => {
  failed.value = false
})
watch(expanded, () => {
  wheelDistance = 0
  lastWheelAt = 0
  lastSwitchAt = -Infinity
})

function switchImage(event, direction) {
  if (
    expanded.value ||
    !isImageAlbum.value ||
    event.ctrlKey ||
    event.metaKey ||
    event.altKey
  )
    return
  event.preventDefault()
  if (event.type === "keydown")
    event.currentTarget.focus({ preventScroll: true })
  switchDirection.value = direction
  activeIndex.value =
    (activeIndex.value + direction + props.files.length) % props.files.length
  wheelDistance = 0
}

function prepareLeavingCard(element) {
  element.inert = true
  element.style.setProperty("--stack-direction", switchDirection.value)
}

function scrollImages(event) {
  if (
    expanded.value ||
    !isImageAlbum.value ||
    event.ctrlKey ||
    event.metaKey ||
    event.altKey
  )
    return
  const delta =
    Math.abs(event.deltaX) > Math.abs(event.deltaY)
      ? event.deltaX
      : event.deltaY
  if (!delta) return
  event.preventDefault()
  if (
    event.timeStamp - lastWheelAt > 180 ||
    Math.sign(wheelDistance) !== Math.sign(delta)
  )
    wheelDistance = 0
  lastWheelAt = event.timeStamp
  if (event.timeStamp - lastSwitchAt < 180) return
  wheelDistance +=
    delta * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 240 : 1)
  if (Math.abs(wheelDistance) < 32) return
  switchImage(event, Math.sign(wheelDistance))
  lastSwitchAt = event.timeStamp
}
</script>

<style scoped lang="less">
.attachment-gallery {
  width: 360px;
  max-width: 100%;
  min-width: 0;
  &.attachment-gallery-stack {
    width: 260px;
  }

  .attachment-image-button {
    display: flex;
    width: 100%;
    padding: 0;
    justify-content: center;
    border: 0;
    background: transparent;
    cursor: zoom-in;
    &:disabled {
      cursor: default;
    }
    .attachment-image {
      display: block;
      width: 100%;
      max-height: 240px;
      object-fit: contain;
    }
    .attachment-image-placeholder {
      display: flex;
      width: 100%;
      height: 100%;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 10px;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
      background: var(--color-panel-soft);
    }
  }
  .attachment-download {
    position: absolute;
    right: 8px;
    bottom: 8px;
    display: flex;
    width: 28px;
    height: 28px;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    color: var(--color-text-muted);
    background: var(--color-panel);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.15s ease;
  }
  .attachment-image-button:focus-visible,
  .attachment-document:focus-visible,
  .attachment-download:focus-visible,
  .attachment-album-toggle:focus-visible,
  .attachment-nav-button:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: -2px;
  }

  .attachment-stage {
    position: relative;
    overflow: hidden;
    border-radius: 10px;
    &:hover .attachment-download,
    &:focus-within .attachment-download {
      opacity: 1;
      pointer-events: auto;
    }
    .attachment-video {
      display: block;
      width: 100%;
      max-height: 240px;
    }
    .attachment-audio {
      display: block;
      width: 100%;
    }
    &.attachment-stage-media {
      padding-bottom: 36px;
      .attachment-download {
        bottom: 2px;
      }
    }
    .attachment-document {
      display: flex;
      width: 100%;
      min-width: 0;
      align-items: center;
      gap: 12px;
      padding: 16px 48px 16px 16px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      color: var(--color-primary);
      background: var(--color-panel);
      text-align: left;
      cursor: pointer;
      &:disabled {
        cursor: default;
      }
      .attachment-document-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 5px;
        .attachment-document-kind {
          color: var(--color-text);
          font-size: var(--font-size-base);
        }
        .attachment-document-size {
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }
      }
    }
  }

  .attachment-album {
    position: relative;
    width: 260px;
    max-width: 100%;
    &:not(.attachment-album-expanded) {
      .attachment-image-list {
        overflow: clip;
        overflow-clip-margin: 16px;
        overflow-anchor: none;
      }
    }
    .attachment-image-list {
      position: relative;
      aspect-ratio: 6 / 5;
      isolation: isolate;
      &:focus-visible {
        outline: 2px solid var(--color-primary);
        outline-offset: 2px;
      }
      .attachment-image-card {
        --stack-transform: translate(
            calc(var(--stack-index) * -10px),
            calc(var(--stack-index) * 4px)
          )
          rotate(calc(2deg - var(--stack-index) * 4deg));
        position: absolute;
        inset: 10px 8px 24px 30px;
        z-index: calc(3 - var(--stack-index));
        overflow: hidden;
        border: 2px solid var(--color-panel);
        border-radius: 9px;
        background: var(--color-panel-soft);
        box-shadow: 0 3px 10px rgb(0 0 0 / 12%);
        transform-origin: center bottom;
        transform: var(--stack-transform);
        &.attachment-stack-card-enter-active {
          transition:
            transform 220ms cubic-bezier(0.22, 1, 0.36, 1),
            opacity 220ms ease;
        }
        &.attachment-stack-card-leave-active {
          z-index: calc(6 - var(--stack-index));
          pointer-events: none;
          transition:
            transform 160ms ease-in,
            opacity 160ms ease-in;
        }
        &.attachment-stack-card-enter-from {
          opacity: 0;
          transform: translateX(calc(var(--stack-direction) * 36px)) scale(0.94)
            var(--stack-transform);
        }
        &.attachment-stack-card-leave-to {
          opacity: 0;
          transform: translateX(calc(var(--stack-direction) * -48px))
            scale(0.96) var(--stack-transform);
        }
        .attachment-image-button {
          height: 100%;
          .attachment-image {
            height: 100%;
            max-height: none;
            object-fit: cover;
          }
        }
        &:hover .attachment-download,
        &:focus-within .attachment-download {
          opacity: 1;
          pointer-events: auto;
        }
      }
    }
    .attachment-album-controls {
      position: relative;
      display: flex;
      align-items: center;
      gap: 8px;
      margin-top: -12px;
      .attachment-album-toggle {
        display: flex;
        flex: none;
        align-items: center;
        gap: 5px;
        padding: 4px 8px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        color: var(--color-text-muted);
        background: var(--color-panel);
        font-size: var(--font-size-xs);
        white-space: nowrap;
        cursor: pointer;
        &:hover {
          color: var(--color-primary);
        }
      }
      .attachment-album-position {
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
      }
    }
    &.attachment-album-expanded {
      width: 100%;
      .attachment-image-list {
        display: flex;
        flex-direction: column;
        gap: 10px;
        aspect-ratio: auto;
        .attachment-image-card {
          position: relative;
          inset: auto;
          z-index: auto;
          min-width: 0;
          flex: none;
          border: 0;
          box-shadow: none;
          transform: none;
          .attachment-image-button {
            height: auto;
            .attachment-image {
              height: auto;
              object-fit: contain;
            }
            .attachment-image-placeholder {
              min-height: 160px;
            }
          }
        }
      }
      .attachment-album-controls {
        margin-top: 8px;
      }
    }
  }

  .attachment-navigation {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 12px;
    padding: 6px 0;
    .attachment-nav-button {
      display: flex;
      width: 28px;
      height: 26px;
      align-items: center;
      justify-content: center;
      padding: 0;
      border: 1px solid var(--color-line);
      border-radius: 5px;
      background: var(--color-panel);
      color: var(--color-text);
    }
    .attachment-position {
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
      font-variant-numeric: tabular-nums;
    }
  }
}
</style>

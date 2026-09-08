<template>
  <section v-if="currentFile" class="attachment-gallery" aria-label="消息附件">
    <div class="attachment-stage">
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
          :alt="currentFile.name"
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
      <div v-else class="attachment-document">
        <FileText :size="32" :stroke-width="1.4" />
        <div class="attachment-document-copy">
          <span class="attachment-document-name">{{ currentFile.name }}</span
          ><span class="attachment-document-hint">{{
            failed
              ? "预览暂不可用，可尝试下载文件"
              : kind === "pdf"
                ? "PDF 文档 · 可在应用内预览"
                : "文件附件 · 可预览或下载"
          }}</span>
        </div>
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
    </div>
    <footer class="attachment-footer">
      <div class="attachment-caption">
        <span class="attachment-name" :title="currentFile.name">{{
          currentFile.name
        }}</span>
        <span class="attachment-size">{{
          formatFileSize(currentFile.size)
        }}</span>
      </div>
      <button
        class="attachment-action"
        type="button"
        title="预览附件"
        aria-label="预览附件"
        :disabled="!previewUrl"
        @click="$emit('preview', currentFile)"
      >
        <Expand :size="15" />
      </button>
      <a
        v-if="downloadUrl"
        class="attachment-action"
        :href="downloadUrl"
        :download="currentFile.name"
        title="下载附件"
        aria-label="下载附件"
        ><Download :size="15"
      /></a>
    </footer>
  </section>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import {
  ChevronLeft,
  ChevronRight,
  Download,
  Expand,
  FileText
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
const currentFile = computed(() => props.files[activeIndex.value])
const kind = computed(() => fileKind(currentFile.value))
const previewUrl = computed(() =>
  fileUrl(props.service, currentFile.value, props.sessionId)
)
const downloadUrl = computed(() =>
  fileUrl(props.service, currentFile.value, props.sessionId, "download")
)
watch(
  () => props.files.map((file) => file.id).join(","),
  () => {
    activeIndex.value = 0
  }
)
watch(currentFile, () => {
  failed.value = false
})
</script>

<style scoped lang="less">
.attachment-gallery {
  width: min(360px, 100%);
  min-width: 0;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 10px;
  background: var(--color-panel);

  .attachment-stage {
    position: relative;
    display: flex;
    min-height: 0;
    flex-direction: column;
    justify-content: center;
    background: var(--color-panel-soft);

    .attachment-image-button {
      display: flex;
      width: 100%;
      padding: 0;
      justify-content: center;
      border: 0;
      background: transparent;
      cursor: zoom-in;

      .attachment-image {
        display: block;
        width: 100%;
        max-height: 240px;
        object-fit: contain;
      }
    }

    .attachment-video {
      display: block;
      width: 100%;
      max-height: 240px;
    }
    .attachment-audio {
      width: 100%;
      padding: 16px 8px;
    }
    .attachment-document {
      display: flex;
      min-width: 0;
      flex-direction: row;
      align-items: center;
      gap: 10px;
      padding: 16px 14px;
      color: var(--color-primary);

      .attachment-document-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 7px;
        .attachment-document-name {
          color: var(--color-text);
          overflow-wrap: anywhere;
          font-size: var(--font-size-base);
        }
        .attachment-document-hint {
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }
      }
    }

    .attachment-navigation {
      display: flex;
      justify-content: center;
      align-items: center;
      gap: 12px;
      padding: 6px 10px;
      background: var(--color-panel);
      border-top: 1px solid var(--color-line);

      .attachment-nav-button {
        display: grid;
        width: 28px;
        height: 26px;
        place-items: center;
        padding: 0;
        border: 1px solid var(--color-line);
        border-radius: 5px;
        background: transparent;
        color: var(--color-text);
      }
      .attachment-position {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        font-variant-numeric: tabular-nums;
      }
    }
  }

  .attachment-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 11px;
    border-top: 1px solid var(--color-line);

    .attachment-caption {
      display: flex;
      min-width: 0;
      flex: 1;
      flex-direction: column;
      gap: 3px;

      .attachment-name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: var(--font-size-base);
      }
      .attachment-size {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }
    }
    .attachment-action {
      display: grid;
      width: 28px;
      height: 28px;
      place-items: center;
      padding: 0;
      border: 0;
      border-radius: 5px;
      color: var(--color-text-muted);
      background: transparent;
    }
  }
}
</style>

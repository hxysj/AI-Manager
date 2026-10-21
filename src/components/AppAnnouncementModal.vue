<template>
  <Teleport to="body">
    <BaseModal
      class="app-announcement-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="app-announcement-title"
      :aria-busy="saving"
      @close="requestClose"
      @keydown="handleKeydown"
    >
      <template #header>
        <div class="announcement-heading">
          <span class="announcement-label">
            <BellRing :size="15" /> 更新公告
          </span>
          <div class="announcement-title-row">
            <h2 id="app-announcement-title" class="announcement-title">
              本次更新内容
            </h2>
            <span class="announcement-version">{{ announcement.version }}</span>
          </div>
          <p class="announcement-description">
            看看这个版本带来了哪些新功能与改进。
          </p>
        </div>
        <button
          class="announcement-close"
          type="button"
          aria-label="关闭更新公告"
          :disabled="saving"
          @click="requestClose"
        >
          <X :size="18" />
        </button>
      </template>

      <div class="announcement-notes" tabindex="0" aria-label="本次更新说明">
        <section
          v-for="(section, index) in sections"
          :key="index"
          class="announcement-section"
        >
          <h3 class="announcement-section-title">{{ section.title }}</h3>
          <ul class="announcement-items">
            <li
              v-for="(item, itemIndex) in section.items"
              :key="itemIndex"
              class="announcement-item"
            >
              {{ item }}
            </li>
          </ul>
        </section>
        <p v-if="!sections.length" class="announcement-empty">
          本次更新包含功能改进与问题修复。
        </p>
      </div>

      <footer class="announcement-footer">
        <span class="announcement-hint">关闭后，本版本不再提示</span>
        <button
          ref="confirmButton"
          class="announcement-confirm"
          type="button"
          :disabled="saving"
          @click="requestClose"
        >
          {{ saving ? "正在保存…" : "确认" }}
        </button>
      </footer>
    </BaseModal>
  </Teleport>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import { BellRing, X } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({
  announcement: { type: Object, required: true },
  saving: { type: Boolean, default: false }
})
const emit = defineEmits(["close"])
const confirmButton = ref(null)
let previousFocus = null

// 发布说明仅按标题和条目排版，使用文本插值避免执行其中的 HTML。
const sections = computed(() => {
  const groups = []
  for (const rawLine of String(props.announcement.releaseNotes || "").split(
    /\r?\n/
  )) {
    const line = rawLine.trim()
    if (
      !line ||
      (/^#{1,2}\s/.test(line) && line.includes(props.announcement.version))
    ) {
      continue
    }
    const heading = line.match(/^#{1,6}\s+(.+)/)
    if (heading) {
      groups.push({ title: heading[1], items: [] })
      continue
    }
    if (!groups.length) groups.push({ title: "更新内容", items: [] })
    groups[groups.length - 1].items.push(
      line.replace(/^[-*]\s+/, "").replace(/`([^`]+)`/g, "$1")
    )
  }
  return groups
})

function requestClose() {
  if (!props.saving) emit("close")
}

function handleKeydown(event) {
  if (event.key === "Escape") {
    event.preventDefault()
    event.stopPropagation()
    requestClose()
  } else if (event.key === "Tab") {
    const targets = event.currentTarget.querySelectorAll(
      'button:not(:disabled), [tabindex="0"]'
    )
    const first = targets[0]
    const last = targets[targets.length - 1]
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault()
      last?.focus()
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault()
      first?.focus()
    }
  }
}

onMounted(() => {
  previousFocus = document.activeElement
  confirmButton.value?.focus()
})

onBeforeUnmount(() => {
  if (previousFocus?.isConnected) previousFocus.focus()
})
</script>

<style scoped lang="less">
.app-announcement-modal {
  color: var(--color-text);

  :deep(.base-modal__panel) {
    width: min(660px, calc(100vw - 48px));
    max-height: min(760px, calc(100vh - 48px));
    border-radius: 12px;
  }

  :deep(.base-modal__header) {
    flex-shrink: 0;
    padding: 24px 28px 20px;
    border-bottom: 1px solid var(--color-line);
  }

  :deep(.base-modal__content) {
    padding: 0;
  }

  .announcement-heading {
    min-width: 0;

    .announcement-label {
      display: flex;
      align-items: center;
      gap: 6px;
      color: var(--color-primary);
      font-size: var(--font-size-sm);
    }

    .announcement-title-row {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
      margin-top: 10px;

      .announcement-title {
        margin: 0;
        font-size: var(--font-size-xl);
      }

      .announcement-version {
        padding: 3px 9px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }
    }

    .announcement-description {
      margin: 10px 0 0;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
      line-height: 1.6;
    }
  }

  .announcement-close {
    display: grid;
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;

    &:hover {
      background: var(--color-panel-soft);
      color: var(--color-text);
    }

    &:disabled {
      opacity: 0.6;
      cursor: wait;
    }
  }

  .announcement-notes {
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding: 24px 28px;

    .announcement-section {
      & + .announcement-section {
        margin-top: 24px;
      }

      .announcement-section-title {
        margin: 0 0 10px;
        padding-left: 10px;
        border-left: 3px solid var(--color-primary);
        font-size: var(--font-size-base);
        line-height: 1.3;
      }

      .announcement-items {
        display: grid;
        gap: 8px;
        margin: 0;
        padding-left: 20px;

        .announcement-item {
          padding-left: 2px;
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
          line-height: 1.8;
          overflow-wrap: anywhere;

          &::marker {
            color: var(--color-text-soft);
          }
        }
      }
    }

    .announcement-empty {
      margin: 0;
      color: var(--color-text-muted);
    }
  }

  .announcement-footer {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    padding: 16px 28px;
    border-top: 1px solid var(--color-line);

    .announcement-hint {
      color: var(--color-text-soft);
      font-size: var(--font-size-xs);
    }

    .announcement-confirm {
      min-width: 96px;
      padding: 10px 20px;
      border: 1px solid var(--color-primary-solid);
      border-radius: 8px;
      background: var(--color-primary-solid);
      color: #fff;
      cursor: pointer;
      font-size: var(--font-size-sm);

      &:hover {
        filter: brightness(1.06);
      }

      &:disabled {
        opacity: 0.6;
        cursor: wait;
      }
    }
  }
}
</style>

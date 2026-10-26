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
        <div class="announcement-header-decor">
          <svg
            class="announcement-illustration"
            viewBox="0 0 180 140"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            aria-hidden="true"
          >
            <defs>
              <filter
                id="announcementGlow"
                x="-30%"
                y="-30%"
                width="160%"
                height="160%"
              >
                <feGaussianBlur stdDeviation="16" />
              </filter>
              <linearGradient
                id="announcementDocGrad"
                x1="20"
                y1="10"
                x2="130"
                y2="120"
                gradientUnits="userSpaceOnUse"
              >
                <stop offset="0%" stop-color="#bfdbfe" stop-opacity="0.95" />
                <stop offset="45%" stop-color="#93c5fd" stop-opacity="0.88" />
                <stop offset="100%" stop-color="#60a5fa" stop-opacity="0.82" />
              </linearGradient>
              <linearGradient
                id="announcementDocStroke"
                x1="20"
                y1="10"
                x2="130"
                y2="120"
                gradientUnits="userSpaceOnUse"
              >
                <stop offset="0%" stop-color="#ffffff" stop-opacity="0.9" />
                <stop offset="50%" stop-color="#ffffff" stop-opacity="0.4" />
                <stop offset="100%" stop-color="#93c5fd" stop-opacity="0.2" />
              </linearGradient>
              <linearGradient
                id="announcementDocSheen"
                x1="0"
                y1="0"
                x2="1"
                y2="1"
              >
                <stop offset="0%" stop-color="#ffffff" stop-opacity="0.5" />
                <stop offset="45%" stop-color="#ffffff" stop-opacity="0.1" />
                <stop offset="50%" stop-color="#ffffff" stop-opacity="0" />
              </linearGradient>
              <radialGradient
                id="announcementBadgeGrad"
                cx="38%"
                cy="32%"
                r="68%"
              >
                <stop offset="0%" stop-color="#93c5fd" />
                <stop offset="25%" stop-color="#60a5fa" />
                <stop offset="70%" stop-color="#2563eb" />
                <stop offset="100%" stop-color="#1d4ed8" />
              </radialGradient>
              <linearGradient
                id="announcementBadgeRim"
                x1="0"
                y1="0"
                x2="0"
                y2="1"
              >
                <stop offset="0%" stop-color="#ffffff" stop-opacity="0.95" />
                <stop offset="100%" stop-color="#93c5fd" stop-opacity="0.4" />
              </linearGradient>
              <filter
                id="announcementDocShadow"
                x="-20%"
                y="-20%"
                width="150%"
                height="150%"
              >
                <feDropShadow
                  dx="0"
                  dy="10"
                  stdDeviation="12"
                  flood-color="#1e40af"
                  flood-opacity="0.18"
                />
              </filter>
              <filter
                id="announcementBadgeShadow"
                x="-30%"
                y="-30%"
                width="170%"
                height="170%"
              >
                <feDropShadow
                  dx="0"
                  dy="8"
                  stdDeviation="10"
                  flood-color="#1e3a8a"
                  flood-opacity="0.32"
                />
              </filter>
            </defs>

            <!-- Ambient Glow -->
            <circle
              cx="100"
              cy="65"
              r="52"
              fill="#60a5fa"
              opacity="0.32"
              filter="url(#announcementGlow)"
            />
            <circle
              cx="135"
              cy="40"
              r="36"
              fill="#38bdf8"
              opacity="0.28"
              filter="url(#announcementGlow)"
            />

            <!-- Tilted Document -->
            <g
              transform="rotate(-6 82 60)"
              filter="url(#announcementDocShadow)"
            >
              <rect
                x="38"
                y="12"
                width="88"
                height="104"
                rx="16"
                fill="url(#announcementDocGrad)"
                stroke="url(#announcementDocStroke)"
                stroke-width="1.5"
              />
              <path
                d="M 39 13 L 125 13 C 125 13 125 50 92 72 C 55 92 39 62 39 62 Z"
                fill="url(#announcementDocSheen)"
              />
              <rect
                x="50"
                y="32"
                width="46"
                height="6.5"
                rx="3.25"
                fill="#ffffff"
                opacity="0.92"
              />
              <rect
                x="50"
                y="46"
                width="64"
                height="6.5"
                rx="3.25"
                fill="#ffffff"
                opacity="0.92"
              />
              <rect
                x="50"
                y="60"
                width="36"
                height="6.5"
                rx="3.25"
                fill="#ffffff"
                opacity="0.92"
              />
            </g>

            <!-- Floating Upward Arrow Circle Badge -->
            <g
              transform="translate(108, 70)"
              filter="url(#announcementBadgeShadow)"
            >
              <circle
                cx="25"
                cy="25"
                r="24"
                fill="url(#announcementBadgeGrad)"
                stroke="url(#announcementBadgeRim)"
                stroke-width="2"
              />
              <path
                d="M 10 18 A 20 20 0 0 1 40 18"
                stroke="#ffffff"
                stroke-width="2"
                stroke-linecap="round"
                opacity="0.55"
                fill="none"
              />
              <g transform="translate(25, 25)">
                <path
                  d="M 0 -9 L -7 -2 C -7.5 -1.5 -7.2 -0.8 -6.5 -0.8 L -3 -0.8 L -3 8 C -3 8.6 -2.6 9 -2 9 L 2 9 C 2.6 9 3 8.6 3 8 L 3 -0.8 L 6.5 -0.8 C 7.2 -0.8 7.5 -1.5 7 -2 Z"
                  fill="#ffffff"
                />
              </g>
            </g>
          </svg>
        </div>

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
          <X :size="16" />
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
        <label class="announcement-remember">
          <input
            v-model="dontShowAgain"
            type="checkbox"
            class="announcement-checkbox-native"
            :disabled="saving"
          />
          <span
            class="announcement-checkbox-custom"
            :class="{ 'is-checked': dontShowAgain }"
            aria-hidden="true"
          >
            <Check v-if="dontShowAgain" :size="12" :stroke-width="3" />
          </span>
          <span class="announcement-hint">关闭后，本版本不再提示</span>
        </label>
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
import { BellRing, Check, X } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"

const props = defineProps({
  announcement: { type: Object, required: true },
  saving: { type: Boolean, default: false }
})
const emit = defineEmits(["close"])
const dontShowAgain = ref(true)
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
  if (!props.saving) {
    emit("close", { remember: dontShowAgain.value })
  }
}

function handleKeydown(event) {
  if (event.key === "Escape") {
    event.preventDefault()
    event.stopPropagation()
    requestClose()
  } else if (event.key === "Tab") {
    const targets = event.currentTarget.querySelectorAll(
      'button:not(:disabled), input:not(:disabled), [tabindex="0"]'
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
    width: min(670px, calc(100vw - 48px));
    max-height: min(780px, calc(100vh - 48px));
    border-radius: 16px;
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow:
      0 24px 64px -12px rgba(15, 23, 42, 0.18),
      0 0 0 1px rgba(0, 0, 0, 0.04);
    overflow: hidden;
  }

  :deep(.base-modal__header) {
    position: relative;
    flex-shrink: 0;
    padding: 16px 28px 14px;
    border-bottom: 1px solid var(--color-line);
    overflow: hidden;
    background: radial-gradient(
      ellipse 540px 240px at 85% -10%,
      rgba(219, 234, 254, 0.75) 0%,
      rgba(224, 242, 254, 0.4) 45%,
      rgba(255, 255, 255, 0) 80%
    );
  }

  :deep(.base-modal__content) {
    padding: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .announcement-header-decor {
    position: absolute;
    right: 56px;
    top: -2px;
    width: 120px;
    height: 96px;
    pointer-events: none;
    z-index: 1;

    .announcement-illustration {
      width: 100%;
      height: 100%;
      overflow: visible;
    }
  }

  .announcement-heading {
    position: relative;
    z-index: 2;
    min-width: 0;
    max-width: 420px;

    .announcement-label {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      color: #2563eb;
      font-size: 12px;
      font-weight: 600;
      line-height: 1.25;
    }

    .announcement-title-row {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
      margin-top: 6px;

      .announcement-title {
        margin: 0;
        font-size: 20px;
        font-weight: 800;
        color: var(--color-text);
        letter-spacing: -0.02em;
        line-height: 1.25;
      }

      .announcement-version {
        display: inline-flex;
        align-items: center;
        padding: 3px 11px;
        border: 1px solid rgba(186, 230, 253, 0.8);
        border-radius: 9999px;
        background: #e0f2fe;
        color: #0284c7;
        font-size: 12.5px;
        font-weight: 600;
        line-height: 1.3;
      }
    }

    .announcement-description {
      margin: 6px 0 0;
      color: var(--color-text-muted);
      font-size: 13px;
      line-height: 1.4;
    }
  }

  .announcement-close {
    position: absolute;
    right: 22px;
    top: 16px;
    z-index: 5;
    display: grid;
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.85);
    backdrop-filter: blur(8px);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;

    &:hover {
      background: var(--color-panel-soft);
      border-color: var(--color-line-strong);
      color: var(--color-text);
    }

    &:focus-visible {
      outline: 2px solid #2563eb;
      outline-offset: 2px;
    }

    &:disabled {
      opacity: 0.6;
      cursor: wait;
    }
  }

  .announcement-notes {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 22px 28px 24px;

    &::-webkit-scrollbar {
      width: 6px;
    }
    &::-webkit-scrollbar-track {
      background: transparent;
    }
    &::-webkit-scrollbar-thumb {
      background: rgba(148, 163, 184, 0.45);
      border-radius: 9999px;

      &:hover {
        background: rgba(100, 116, 139, 0.7);
      }
    }

    .announcement-section {
      background: var(--color-panel);
      border: 1px solid var(--color-line);
      border-radius: 12px;
      padding: 16px 20px;
      box-shadow: 0 1px 2px rgba(15, 23, 42, 0.02);
      transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);

      & + .announcement-section {
        margin-top: 14px;
      }

      &:hover {
        border-color: var(--color-line-strong);
        box-shadow: 0 4px 12px rgba(15, 23, 42, 0.04);
      }

      .announcement-section-title {
        margin: 0 0 12px;
        font-size: 15px;
        font-weight: 700;
        color: var(--color-text);
        line-height: 1.4;
        letter-spacing: -0.01em;
      }

      .announcement-items {
        display: flex;
        flex-direction: column;
        gap: 10px;
        margin: 0;
        padding: 0;
        list-style: none;

        .announcement-item {
          position: relative;
          padding-left: 16px;
          color: var(--color-text-muted);
          font-size: 13.5px;
          line-height: 1.65;
          overflow-wrap: anywhere;

          &::before {
            content: "";
            position: absolute;
            left: 2px;
            top: 8.5px;
            width: 5px;
            height: 5px;
            border-radius: 50%;
            background: #2563eb;
          }
        }
      }
    }

    .announcement-empty {
      margin: 0;
      color: var(--color-text-muted);
      font-size: 14px;
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
    background: var(--color-panel);

    .announcement-remember {
      position: relative;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      min-height: 38px;
      gap: 8px;
      cursor: pointer;
      user-select: none;

      .announcement-checkbox-native {
        position: absolute;
        opacity: 0;
        width: 0;
        height: 0;
        pointer-events: none;

        &:focus-visible + .announcement-checkbox-custom {
          outline: 2px solid #2563eb;
          outline-offset: 2px;
        }
      }

      .announcement-checkbox-custom {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
        border: 1.5px solid var(--color-line-strong);
        border-radius: 4px;
        background: var(--color-panel);
        color: #ffffff;
        transition: all 0.15s ease;

        &.is-checked {
          border-color: #2563eb;
          background: #2563eb;
        }
      }

      .announcement-hint {
        display: inline-flex;
        align-items: center;
        color: var(--color-text-muted);
        font-size: 13.5px;
        line-height: 20px;
        transition: color 0.15s ease;
      }

      &:hover .announcement-hint {
        color: var(--color-text);
      }

      &:hover .announcement-checkbox-custom:not(.is-checked) {
        border-color: #2563eb;
      }
    }

    .announcement-confirm {
      min-width: 96px;
      height: 38px;
      padding: 0 24px;
      border: none;
      border-radius: 8px;
      background: #1677ff;
      color: #ffffff;
      cursor: pointer;
      font-size: 14px;
      font-weight: 500;
      box-shadow: 0 2px 8px rgba(22, 119, 255, 0.28);
      transition: all 0.15s cubic-bezier(0.16, 1, 0.3, 1);

      &:hover {
        filter: brightness(1.08);
        box-shadow: 0 4px 14px rgba(22, 119, 255, 0.36);
        transform: translateY(-1px);
      }

      &:active {
        transform: translateY(0);
        filter: brightness(0.96);
      }

      &:focus-visible {
        outline: 2px solid #1677ff;
        outline-offset: 2px;
      }

      &:disabled {
        opacity: 0.6;
        cursor: wait;
        transform: none;
        box-shadow: none;
      }
    }
  }
}

:root[data-theme="dark"] .app-announcement-modal {
  :deep(.base-modal__header) {
    background: radial-gradient(
      ellipse 540px 240px at 85% -10%,
      rgba(30, 58, 138, 0.45) 0%,
      rgba(14, 116, 144, 0.2) 45%,
      transparent 80%
    );
  }

  .announcement-heading {
    .announcement-label {
      color: #60a5fa;
    }

    .announcement-title-row .announcement-version {
      background: rgba(14, 165, 233, 0.16);
      color: #38bdf8;
      border-color: rgba(56, 189, 248, 0.28);
    }
  }

  .announcement-close {
    background: rgba(30, 41, 59, 0.7);
    border-color: var(--color-line);

    &:hover {
      background: var(--color-panel-soft);
      border-color: var(--color-line-strong);
    }
  }

  .announcement-notes .announcement-section {
    background: var(--color-panel-soft);

    .announcement-items .announcement-item::before {
      background: #3b82f6;
    }
  }

  .announcement-footer {
    .announcement-remember {
      .announcement-checkbox-custom {
        background: var(--color-panel-soft);
        border-color: var(--color-line-strong);

        &.is-checked {
          border-color: #2563eb;
          background: #2563eb;
        }
      }
    }
  }
}
</style>

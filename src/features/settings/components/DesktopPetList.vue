<template>
  <div class="settings-desktop-selection">
    <!-- 顶部卡片：桌宠功能卡片 -->
    <section
      class="desktop-hero-card"
      tabindex="0"
      role="button"
      aria-label="进入桌宠功能详情"
      @click="openDetail"
      @keydown.enter="openDetail"
      @keydown.space.prevent="openDetail"
    >
      <div class="desktop-hero-left">
        <div class="desktop-hero-icon">
          <Monitor :size="24" />
        </div>
        <div class="desktop-hero-info">
          <h3 class="desktop-hero-title">桌宠功能</h3>
          <p class="desktop-hero-desc">
            在桌面上显示和运行可爱的桌面宠物，让它们陪伴你的工作与学习。
          </p>
        </div>
      </div>

      <div class="desktop-hero-divider"></div>

      <div class="desktop-hero-right">
        <div class="desktop-hero-switch-row">
          <button
            class="desktop-switch"
            :class="{ 'desktop-switch--active': enabled }"
            type="button"
            role="switch"
            :aria-checked="enabled"
            :disabled="pending"
            @click.stop="$emit('toggle-master', !enabled)"
            @keydown.stop
          >
            <span class="desktop-switch-thumb"></span>
          </button>
          <span class="desktop-hero-switch-label">启用桌宠功能</span>
        </div>
        <div class="desktop-hero-status-row">
          <span
            class="desktop-status-dot"
            :class="{ 'desktop-status-dot--active': enabled }"
          ></span>
          <span
            class="desktop-status-label"
            :class="{ 'desktop-status-label--active': enabled }"
          >
            {{ enabled ? "当前已开启" : "当前已关闭" }}
          </span>
        </div>
        <span class="desktop-hero-status-desc">
          {{ enabled ? "桌面宠物功能正常运行中" : "桌面宠物功能已暂停" }}
        </span>
      </div>
    </section>
  </div>
</template>

<script setup>
import { Monitor } from "lucide-vue-next"

const props = defineProps({
  pets: { type: Array, required: true },
  selectedPetId: { type: String, default: "" },
  runningPetId: { type: String, default: "" },
  enabled: { type: Boolean, default: false },
  pending: { type: Boolean, default: false }
})

const emit = defineEmits(["detail", "toggle-master"])

function openDetail() {
  if (!props.pets.length) return
  // 进入详情时优先展示当前运行桌宠，其次使用草稿选择，最后回退到第一只桌宠。
  emit("detail", props.runningPetId || props.selectedPetId || props.pets[0].id)
}
</script>

<style scoped lang="less">
.settings-desktop-selection {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

// 顶部 Hero 卡片
.desktop-hero-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 22px 28px;
  background: var(--color-panel);
  border: 1px solid var(--color-line);
  border-radius: 16px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
  cursor: pointer;
  transition: box-shadow 0.2s ease, transform 0.2s ease;

  &:hover {
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.06);
    transform: translateY(-1px);
  }

  &:focus-visible {
    outline: 2px solid #2563eb;
    outline-offset: 2px;
  }

  .desktop-hero-left {
    display: flex;
    align-items: center;
    gap: 18px;
    flex: 1;

    .desktop-hero-icon {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 52px;
      height: 52px;
      border-radius: 14px;
      background: #eff6ff;
      color: #2563eb;
      flex-shrink: 0;
    }

    .desktop-hero-info {
      display: flex;
      flex-direction: column;
      gap: 4px;

      .desktop-hero-title {
        margin: 0;
        font-size: 18px;
        font-weight: 700;
        color: var(--color-text);
        letter-spacing: -0.01em;
      }

      .desktop-hero-desc {
        margin: 0;
        font-size: 13px;
        color: var(--color-text-muted);
        line-height: 1.5;
      }
    }
  }

  .desktop-hero-divider {
    width: 1px;
    height: 54px;
    background: var(--color-line);
    margin: 0 32px;
    flex-shrink: 0;
  }

  .desktop-hero-right {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 200px;
    flex-shrink: 0;

    .desktop-hero-switch-row {
      display: flex;
      align-items: center;
      gap: 10px;

      .desktop-hero-switch-label {
        font-size: 15px;
        font-weight: 600;
        color: var(--color-text);
      }
    }

    .desktop-hero-status-row {
      display: flex;
      align-items: center;
      gap: 6px;
      margin-top: 2px;

      .desktop-status-dot {
        width: 7px;
        height: 7px;
        border-radius: 50%;
        background: #94a3b8;

        &--active {
          background: #16a34a;
        }
      }

      .desktop-status-label {
        font-size: 13px;
        font-weight: 500;
        color: var(--color-text-muted);

        &--active {
          color: #16a34a;
        }
      }
    }

    .desktop-hero-status-desc {
      font-size: 12px;
      color: var(--color-text-soft);
      line-height: 1.4;
    }
  }
}

// 统一的平滑开关样式
.desktop-switch {
  position: relative;
  width: 44px;
  height: 24px;
  border: none;
  border-radius: 12px;
  background: #cbd5e1;
  cursor: pointer;
  padding: 2px;
  transition: background 0.2s ease;
  flex-shrink: 0;

  &:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  &--active {
    background: #2563eb;

    .desktop-switch-thumb {
      transform: translateX(20px);
    }
  }

  .desktop-switch-thumb {
    display: block;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: #ffffff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    transition: transform 0.2s ease;
  }
}
</style>

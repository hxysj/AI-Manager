<template>
  <BaseModal
    class="settings-desktop-detail"
    title="桌宠功能 · 详情"
    description="选择桌宠、预览全部动作，打开开关即可启动桌面陪伴。"
    role="dialog"
    aria-modal="true"
    :aria-label="`${pet.name}的详情`"
    @close="$emit('close')"
    @keydown.esc.stop="$emit('close')"
  >
    <div class="settings-desktop-detail-body">
      <!-- 桌宠目录放在详情内，避免占用桌面管理外层空间。 -->
      <section class="desktop-detail-pets-section">
        <div class="desktop-detail-pets-header">
          <div>
            <h3 class="desktop-detail-pets-title">可用桌宠</h3>
            <p class="desktop-detail-pets-desc">选择桌宠查看详情和动作预览。</p>
          </div>
          <span class="desktop-detail-pets-count">{{ pets.length }} 只桌宠</span>
        </div>

        <div v-if="pets.length" class="desktop-detail-pets-list">
          <button
            v-for="item in pets"
            :key="item.id"
            class="desktop-detail-pet-option"
            :class="{ 'desktop-detail-pet-option--selected': item.id === pet.id }"
            type="button"
            :aria-pressed="item.id === pet.id"
            @click="$emit('select', item.id)"
          >
            <span class="desktop-detail-pet-preview">
              <DesktopPet :pet="item" />
            </span>
            <span class="desktop-detail-pet-name">{{ item.name }}</span>
            <span class="desktop-detail-pet-actions">
              {{ item.animations?.length || item.actionCount || 0 }} 个动作
            </span>
            <span v-if="item.id === pet.id" class="desktop-detail-pet-current">
              当前查看
            </span>
          </button>
        </div>
        <p v-else class="desktop-detail-empty-hint">暂无可用桌宠。</p>
      </section>

      <!-- 顶部桌宠概览卡片 -->
      <section class="desktop-detail-hero">
        <div class="desktop-detail-hero-left">
          <div class="desktop-detail-avatar">
            <DesktopPet :pet="pet" />
          </div>
          <div class="desktop-detail-info">
            <h3 class="desktop-detail-name">{{ pet.name }}</h3>
            <span class="desktop-detail-sub">
              {{ pet.animations?.length || pet.actionCount || 0 }} 个动作 · 自动轮换播放
            </span>
            <p class="desktop-detail-desc">
              {{
                pet.id === "monkey_animations"
                  ? "启动后会在桌面上显示小猴子，自动播放动作，陪伴你的日常使用。"
                  : pet.description ||
                    `启动后会在桌面上显示${pet.name}，自动播放动作，陪伴你的日常使用。`
              }}
            </p>
          </div>
        </div>

        <div class="desktop-detail-hero-right">
          <button
            class="desktop-switch"
            :class="{ 'desktop-switch--active': running }"
            type="button"
            role="switch"
            :aria-checked="running"
            :disabled="pending"
            @click="toggleRunning"
          >
            <span class="desktop-switch-thumb"></span>
          </button>
          <span class="desktop-detail-switch-label">
            {{ running ? "已启用" : "已禁用" }}
          </span>
        </div>
      </section>

      <!-- 动作预览部分 -->
      <section class="desktop-detail-actions-section">
        <div class="desktop-detail-actions-header">
          <h4 class="desktop-detail-actions-title">动作预览</h4>
          <span class="desktop-detail-actions-desc">
            包含以下 {{ pet.animations?.length || pet.actionCount || 0 }} 个动作，会自动随机播放。
          </span>
        </div>

        <!-- 5列网格排版，对齐 Image 10 设计 -->
        <div v-if="pet.animations?.length" class="desktop-detail-actions-grid">
          <article
            v-for="animation in pet.animations"
            :key="`${pet.id}:${animation.id}`"
            class="desktop-action-card"
          >
            <div class="desktop-action-preview">
              <DesktopPet :pet="pet" :action="animation.id" />
            </div>
            <span class="desktop-action-name">{{ animation.name }}</span>
          </article>
        </div>
        <p v-else class="desktop-detail-empty-hint">暂无可用动作数据。</p>
      </section>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from "@/components/BaseModal.vue"
import DesktopPet from "@/components/DesktopPet.vue"

const props = defineProps({
  pets: { type: Array, default: () => [] },
  pet: { type: Object, required: true },
  running: { type: Boolean, default: false },
  pending: { type: Boolean, default: false }
})

const emit = defineEmits(["close", "select", "start", "stop"])

function toggleRunning() {
  if (props.pending) return
  if (props.running) {
    emit("stop")
  } else {
    emit("start", props.pet)
  }
}
</script>

<style scoped lang="less">
.settings-desktop-detail {
  :deep(.base-modal__panel) {
    width: 920px;
    max-width: 95vw;
    border-radius: 16px;
  }

  .settings-desktop-detail-body {
    display: flex;
    flex-direction: column;
    gap: 20px;
    max-height: calc(85vh - 120px);
    overflow-y: auto;
    padding-right: 4px;

    &::-webkit-scrollbar {
      width: 6px;
    }
    &::-webkit-scrollbar-thumb {
      background: var(--color-line-strong);
      border-radius: 3px;
    }
  }

  .desktop-detail-pets-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    background: var(--color-panel-soft);
    border: 1px solid var(--color-line);
    border-radius: 14px;

    .desktop-detail-pets-header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 16px;

      .desktop-detail-pets-title {
        margin: 0;
        font-size: 16px;
        color: var(--color-text);
      }

      .desktop-detail-pets-desc {
        margin: 4px 0 0;
        font-size: 13px;
        color: var(--color-text-muted);
      }

      .desktop-detail-pets-count {
        flex-shrink: 0;
        padding-top: 2px;
        font-size: 13px;
        color: var(--color-text-muted);
      }
    }

    .desktop-detail-pets-list {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
    }

    .desktop-detail-pet-option {
      display: flex;
      flex: 1 1 140px;
      flex-direction: column;
      align-items: center;
      min-width: 140px;
      max-width: 180px;
      gap: 4px;
      padding: 10px 12px;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
      text-align: center;
      transition: border-color 0.2s ease, box-shadow 0.2s ease;

      &:hover {
        border-color: var(--color-line-strong);
        box-shadow: 0 3px 10px rgba(0, 0, 0, 0.05);
      }

      &:focus-visible {
        outline: 2px solid #2563eb;
        outline-offset: 2px;
      }

      &--selected {
        border-color: #3b82f6;
        box-shadow: 0 0 0 1px #3b82f6;
      }

      .desktop-detail-pet-preview {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 72px;
        height: 72px;
      }

      .desktop-detail-pet-name {
        font-size: 14px;
      }

      .desktop-detail-pet-actions,
      .desktop-detail-pet-current {
        font-size: 12px;
        color: var(--color-text-muted);
      }

      .desktop-detail-pet-current {
        color: #2563eb;
      }
    }
  }

  // 顶部卡片
  .desktop-detail-hero {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 24px;
    background: var(--color-panel-soft);
    border: 1px solid var(--color-line);
    border-radius: 14px;

    .desktop-detail-hero-left {
      display: flex;
      align-items: center;
      gap: 20px;
      flex: 1;

      .desktop-detail-avatar {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 88px;
        height: 88px;
        flex-shrink: 0;
      }

      .desktop-detail-info {
        display: flex;
        flex-direction: column;
        gap: 4px;

        .desktop-detail-name {
          margin: 0;
          font-size: 19px;
          font-weight: 700;
          color: var(--color-text);
        }

        .desktop-detail-sub {
          font-size: 13px;
          color: var(--color-text-muted);
        }

        .desktop-detail-desc {
          margin: 4px 0 0;
          font-size: 13px;
          color: var(--color-text-muted);
          line-height: 1.5;
        }
      }
    }

    .desktop-detail-hero-right {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 6px;
      margin-left: 24px;
      flex-shrink: 0;

      .desktop-detail-switch-label {
        font-size: 12px;
        font-weight: 500;
        color: var(--color-text-muted);
      }
    }
  }

  // 开关样式
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

  // 动作预览区域
  .desktop-detail-actions-section {
    display: flex;
    flex-direction: column;
    gap: 14px;

    .desktop-detail-actions-header {
      display: flex;
      flex-direction: column;
      gap: 4px;

      .desktop-detail-actions-title {
        margin: 0;
        font-size: 16px;
        font-weight: 700;
        color: var(--color-text);
      }

      .desktop-detail-actions-desc {
        font-size: 13px;
        color: var(--color-text-muted);
      }
    }

    .desktop-detail-actions-grid {
      display: grid;
      grid-template-columns: repeat(5, minmax(0, 1fr));
      gap: 14px;

      @media (max-width: 768px) {
        grid-template-columns: repeat(3, minmax(0, 1fr));
      }
    }

    .desktop-action-card {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 8px;
      padding: 14px 8px 10px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);
      border-radius: 12px;
      transition: all 0.2s ease;

      &:hover {
        background: var(--color-panel);
        border-color: var(--color-line-strong);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
        transform: translateY(-1px);
      }

      .desktop-action-preview {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 80px;
        height: 80px;
      }

      .desktop-action-name {
        font-size: 13px;
        font-weight: 500;
        color: var(--color-text);
      }
    }

    .desktop-detail-empty-hint {
      margin: 0;
      padding: 32px;
      text-align: center;
      color: var(--color-text-muted);
      border: 1px dashed var(--color-line);
      border-radius: 12px;
      font-size: 13px;
    }
  }
}
</style>

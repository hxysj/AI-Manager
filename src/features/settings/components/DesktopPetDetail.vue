<template>
  <BaseModal
    class="settings-desktop-detail"
    title="桌宠功能 · 详情"
    role="dialog"
    aria-modal="true"
    :aria-label="`${pet.name}的详情`"
    @close="$emit('close')"
    @keydown.esc.stop="$emit('close')"
  >
    <template #header>
      <div class="desktop-detail-heading">
        <h2 class="desktop-detail-title">桌宠功能 · 详情</h2>
      </div>
      <button
        class="desktop-detail-close"
        type="button"
        aria-label="关闭桌宠详情"
        @click="$emit('close')"
      >
        <X :size="20" />
      </button>
    </template>
    <div class="settings-desktop-detail-body">
      <!-- 桌宠目录放在详情内，避免占用桌面管理外层空间。 -->
      <aside class="desktop-detail-pets-section" aria-label="宠物列表">
        <h3 class="desktop-detail-pets-title">宠物列表</h3>
        <div v-if="pets.length" class="desktop-detail-pets-list">
          <button
            v-for="item in pets"
            :key="item.id"
            class="desktop-detail-pet-option"
            :class="{
              'desktop-detail-pet-option--selected': item.id === pet.id
            }"
            type="button"
            :aria-pressed="item.id === pet.id"
            @click="$emit('select', item.id)"
          >
            <span class="desktop-detail-pet-preview">
              <DesktopPet
                :pet="item"
                :action="item.animations?.[0]?.id || ''"
              />
            </span>
            <span class="desktop-detail-pet-copy">
              <span class="desktop-detail-pet-name">{{ item.name }}</span>
              <span class="desktop-detail-pet-actions">
                {{ item.animations?.length || item.actionCount || 0 }} 个动作
              </span>
              <span
                v-if="item.id === runningPetId"
                class="desktop-detail-pet-current"
              >
                当前激活
              </span>
            </span>
          </button>
        </div>
        <p v-else class="desktop-detail-empty-hint">暂无可用桌宠。</p>
      </aside>

      <div class="desktop-detail-content">
        <!-- 顶部桌宠概览卡片 -->
        <section class="desktop-detail-hero">
          <div class="desktop-detail-hero-left">
            <div class="desktop-detail-avatar">
              <DesktopPet :pet="pet" />
            </div>
            <div class="desktop-detail-info">
              <h3 class="desktop-detail-name">{{ pet.name }}</h3>
              <p class="desktop-detail-desc">
                {{
                  pet.description ||
                  `可爱的${pet.name}，会在桌面上自动播放各种有趣的动作，陪伴你的日常使用。`
                }}
              </p>
              <div
                class="desktop-detail-status"
                :class="{ 'desktop-detail-status--active': running }"
                role="status"
              >
                <span class="desktop-detail-status-dot"></span>
                <span>{{
                  running ? "状态：已启用 · 自动轮播动作" : "状态：未启用"
                }}</span>
              </div>
            </div>
          </div>

          <div class="desktop-detail-hero-right">
            <button
              class="desktop-switch"
              :class="{ 'desktop-switch--active': running }"
              type="button"
              role="switch"
              aria-label="启用桌宠"
              :aria-checked="running"
              :disabled="pending"
              @click="toggleRunning"
            >
              <span class="desktop-switch-thumb"></span>
            </button>
            <span class="desktop-detail-switch-label">启用桌宠</span>
          </div>
        </section>

        <!-- 动作预览部分 -->
        <section class="desktop-detail-actions-section">
          <div class="desktop-detail-actions-header">
            <h4 class="desktop-detail-actions-title">动作列表</h4>
            <span class="desktop-detail-actions-desc">
              以下为{{ pet.name }}可播放的动作预览。
            </span>
          </div>

          <!-- 动作卡片按可用宽度换行，内容较多时在右侧独立滚动。 -->
          <div
            v-if="pet.animations?.length"
            class="desktop-detail-actions-grid"
          >
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
        <div class="desktop-detail-hint">
          <Info :size="17" />
          <span>动作会根据桌宠状态自动播放。</span>
        </div>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import { Info, X } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import DesktopPet from "@/components/DesktopPet.vue"

const props = defineProps({
  pets: { type: Array, default: () => [] },
  pet: { type: Object, required: true },
  runningPetId: { type: String, default: "" },
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
    width: 86vw;
    max-width: calc(100vw - 48px);
    height: 80vh;
    border-radius: 18px;
  }

  :deep(.base-modal__header) {
    padding: 24px 24px 18px;
  }

  .desktop-detail-heading {
    .desktop-detail-title {
      margin: 0;
      color: var(--color-text);
      font-size: var(--font-size-lg);
      line-height: 1.35;
    }
  }

  .desktop-detail-close {
    display: flex;
    width: 38px;
    height: 38px;
    flex: none;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;

    &:hover {
      background: var(--color-panel-soft);
      color: var(--color-text);
    }
  }

  .settings-desktop-detail-body {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: 16px;
    overflow: hidden;

    .desktop-detail-empty-hint {
      margin: 0;
      padding: 24px 12px;
      color: var(--color-text-muted);
      font-size: 13px;
      text-align: center;
      line-height: 1.5;
    }

    .desktop-detail-pets-section {
      display: flex;
      width: 26%;
      min-width: 200px;
      min-height: 0;
      flex: none;
      flex-direction: column;
      gap: 16px;
      padding: 16px 12px;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel-soft);

      .desktop-detail-pets-title {
        margin: 0 4px;
        color: var(--color-text);
        font-size: var(--font-size-base);
        line-height: 1.4;
      }

      .desktop-detail-pets-list {
        display: flex;
        min-height: 0;
        flex-direction: column;
        gap: 12px;
        overflow-y: auto;
        padding: 1px;

        .desktop-detail-pet-option {
          display: flex;
          min-height: 112px;
          flex: none;
          align-items: center;
          gap: 12px;
          padding: 12px;
          border: 1px solid var(--color-line);
          border-radius: 12px;
          background: var(--color-panel);
          color: var(--color-text);
          cursor: pointer;
          text-align: left;
          transition:
            border-color 0.2s ease,
            background 0.2s ease,
            box-shadow 0.2s ease;

          &:hover {
            border-color: var(--color-primary);
          }

          &:focus-visible {
            outline: 2px solid var(--color-primary);
            outline-offset: -2px;
          }

          &.desktop-detail-pet-option--selected {
            border-color: #3b82f6;
            background: rgba(59, 130, 246, 0.06);
            box-shadow: 0 0 0 1px #3b82f6;
          }

          .desktop-detail-pet-preview {
            display: flex;
            width: 78px;
            height: 88px;
            flex: none;
            align-items: center;
            justify-content: center;
          }

          .desktop-detail-pet-copy {
            display: flex;
            min-width: 0;
            align-items: flex-start;
            flex-direction: column;
            gap: 6px;

            .desktop-detail-pet-name {
              font-size: 16px;
              line-height: 1.4;
            }

            .desktop-detail-pet-actions {
              color: var(--color-text-muted);
              font-size: 13px;
              line-height: 1.4;
            }

            .desktop-detail-pet-current {
              padding: 4px 10px;
              border-radius: 8px;
              background: rgba(34, 197, 94, 0.15);
              color: #16a34a;
              font-size: 12px;
              line-height: 1.35;
            }
          }
        }
      }
    }

    .desktop-detail-content {
      display: flex;
      min-width: 0;
      min-height: 0;
      flex: 1;
      flex-direction: column;
      gap: 18px;
      overflow-y: auto;
      padding-right: 4px;

      &::-webkit-scrollbar {
        width: 6px;
      }

      &::-webkit-scrollbar-thumb {
        border-radius: 6px;
        background: var(--color-line-strong);
      }

      .desktop-detail-hero {
        display: flex;
        flex: none;
        align-items: center;
        gap: 20px;
        padding: 20px;
        border: 1px solid var(--color-line);
        border-radius: 14px;
        background: var(--color-panel);

        .desktop-detail-hero-left {
          display: flex;
          min-width: 0;
          flex: 1;
          align-items: center;
          gap: 20px;

          .desktop-detail-avatar {
            display: flex;
            width: 128px;
            height: 128px;
            flex: none;
            align-items: center;
            justify-content: center;
            padding: 10px;
            border-radius: 14px;
            background: var(--color-panel-soft);
          }

          .desktop-detail-info {
            display: flex;
            min-width: 0;
            flex: 1;
            flex-direction: column;
            gap: 10px;

            .desktop-detail-name {
              margin: 0;
              color: var(--color-text);
              font-size: var(--font-size-lg);
              line-height: 1.35;
            }

            .desktop-detail-desc {
              margin: 0;
              color: var(--color-text-muted);
              font-size: 14px;
              line-height: 1.6;
            }

            .desktop-detail-status {
              display: flex;
              align-items: center;
              gap: 8px;
              color: var(--color-text-muted);
              font-size: 13px;
              line-height: 1.5;

              .desktop-detail-status-dot {
                width: 9px;
                height: 9px;
                flex: none;
                border-radius: 50%;
                background: var(--color-text-muted);
              }

              &.desktop-detail-status--active {
                .desktop-detail-status-dot {
                  background: #22b56d;
                }
              }
            }
          }
        }

        .desktop-detail-hero-right {
          display: flex;
          min-width: 100px;
          flex: none;
          align-self: stretch;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 10px;
          padding-left: 20px;
          border-left: 1px solid var(--color-line);

          .desktop-switch {
            position: relative;
            width: 56px;
            height: 32px;
            flex: none;
            padding: 3px;
            border: none;
            border-radius: 18px;
            background: #cbd5e1;
            cursor: pointer;
            transition: background 0.2s ease;

            &:disabled {
              opacity: 0.6;
              cursor: not-allowed;
            }

            &.desktop-switch--active {
              background: #2563eb;

              .desktop-switch-thumb {
                transform: translateX(24px);
              }
            }

            .desktop-switch-thumb {
              display: block;
              width: 26px;
              height: 26px;
              border-radius: 50%;
              background: #ffffff;
              box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
              transition: transform 0.2s ease;
            }
          }

          .desktop-detail-switch-label {
            color: var(--color-text);
            font-size: 15px;
            line-height: 1.4;
          }
        }
      }

      .desktop-detail-actions-section {
        display: flex;
        flex-direction: column;
        gap: 14px;

        .desktop-detail-actions-header {
          display: flex;
          flex-direction: column;
          gap: 6px;

          .desktop-detail-actions-title {
            margin: 0;
            color: var(--color-text);
            font-size: var(--font-size-base);
            line-height: 1.4;
          }

          .desktop-detail-actions-desc {
            color: var(--color-text-muted);
            font-size: 14px;
            line-height: 1.5;
          }
        }

        .desktop-detail-actions-grid {
          display: flex;
          flex-wrap: wrap;
          gap: 12px;

          .desktop-action-card {
            display: flex;
            min-width: 0;
            max-width: 180px;
            flex: 1 1 140px;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 8px;
            padding: 14px 8px;
            border: 1px solid var(--color-line);
            border-radius: 12px;
            background: var(--color-panel-soft);

            .desktop-action-preview {
              display: flex;
              width: 100%;
              max-width: 116px;
              height: 100px;
              align-items: center;
              justify-content: center;
            }

            .desktop-action-name {
              color: var(--color-text);
              font-size: 14px;
              line-height: 1.5;
            }
          }
        }
      }

      .desktop-detail-hint {
        display: flex;
        flex: none;
        align-items: center;
        gap: 10px;
        padding: 14px;
        border-radius: 9px;
        background: rgba(59, 130, 246, 0.07);
        color: var(--color-text-muted);
        font-size: 13px;
        line-height: 1.5;
      }
    }
  }
}
</style>

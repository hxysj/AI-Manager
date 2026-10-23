<template>
  <div class="settings-desktop-selection">
    <!-- 顶部卡片：桌宠功能卡片 -->
    <section class="desktop-hero-card">
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
            @click="$emit('toggle-master', !enabled)"
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

    <!-- 可用桌宠区域（已去除桌宠数量统计行） -->
    <section class="desktop-pets-section">
      <div class="desktop-section-header">
        <PawPrint :size="20" class="desktop-section-icon" />
        <div class="desktop-section-info">
          <h3 class="desktop-section-title">可用桌宠</h3>
          <p class="desktop-section-desc">
            选择要启用的桌面宠物，启用后它们将可以在桌面上显示和运行。
          </p>
        </div>
      </div>

      <div v-if="pets.length" class="desktop-cards-grid">
        <article
          v-for="pet in pets"
          :key="pet.id"
          class="desktop-pet-card"
          :class="{
            'desktop-pet-card--active': isPetRunning(pet)
          }"
          tabindex="0"
          role="button"
          :aria-label="`${pet.name}，点击查看详情`"
          @click="$emit('detail', pet.id)"
          @keydown.enter="$emit('detail', pet.id)"
          @keydown.space.prevent="$emit('detail', pet.id)"
        >
          <!-- 运行状态角标 -->
          <div
            class="desktop-pet-badge"
            :class="{ 'desktop-pet-badge--running': isPetRunning(pet) }"
          >
            <span class="desktop-badge-dot"></span>
            <span>{{ isPetRunning(pet) ? "运行中" : "未运行" }}</span>
          </div>

          <!-- 桌宠动作预览 -->
          <div class="desktop-pet-preview-wrap">
            <DesktopPet :pet="pet" />
          </div>

          <!-- 桌宠信息 -->
          <div class="desktop-pet-meta">
            <h4 class="desktop-pet-name">{{ pet.name }}</h4>
            <span class="desktop-pet-action-count">
              {{ pet.actionCount || pet.animations?.length || 0 }} 个动作
            </span>
            <p class="desktop-pet-desc">{{ pet.description }}</p>
          </div>

          <!-- 卡片底部操作栏 -->
          <div class="desktop-pet-footer" @click.stop>
            <div class="desktop-pet-switch-wrap">
              <button
                class="desktop-switch"
                :class="{ 'desktop-switch--active': isPetRunning(pet) }"
                type="button"
                role="switch"
                :aria-checked="isPetRunning(pet)"
                :disabled="pending"
                @click.stop="togglePet(pet)"
              >
                <span class="desktop-switch-thumb"></span>
              </button>
              <span class="desktop-pet-switch-text">
                {{ isPetRunning(pet) ? "已启用" : "已禁用" }}
              </span>
            </div>

            <!-- 当前运行标记 -->
            <div v-if="isPetRunning(pet)" class="desktop-pet-current-badge">
              <Star :size="13" class="desktop-star-icon" />
              <span>当前运行</span>
            </div>
          </div>
        </article>
      </div>

      <p v-else class="desktop-empty-hint">暂无可用桌宠。</p>
    </section>
  </div>
</template>

<script setup>
import { Monitor, PawPrint, Star } from "lucide-vue-next"
import DesktopPet from "@/components/DesktopPet.vue"

const props = defineProps({
  pets: { type: Array, required: true },
  modelValue: { type: String, default: "" },
  runningPetId: { type: String, default: "" },
  enabled: { type: Boolean, default: false },
  pending: { type: Boolean, default: false }
})

const emit = defineEmits([
  "update:modelValue",
  "detail",
  "start",
  "stop",
  "toggle-master"
])

function isPetRunning(pet) {
  return props.enabled && props.runningPetId === pet.id
}

function togglePet(pet) {
  if (props.pending) return
  if (isPetRunning(pet)) {
    emit("stop")
  } else {
    emit("update:modelValue", pet.id)
    emit("start", pet)
  }
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

// 可用桌宠区域
.desktop-pets-section {
  display: flex;
  flex-direction: column;
  gap: 16px;

  .desktop-section-header {
    display: flex;
    align-items: flex-start;
    gap: 10px;

    .desktop-section-icon {
      color: #2563eb;
      margin-top: 3px;
      flex-shrink: 0;
    }

    .desktop-section-info {
      display: flex;
      flex-direction: column;
      gap: 4px;

      .desktop-section-title {
        margin: 0;
        font-size: 17px;
        font-weight: 700;
        color: var(--color-text);
      }

      .desktop-section-desc {
        margin: 0;
        font-size: 13px;
        color: var(--color-text-muted);
      }
    }
  }

  .desktop-cards-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 20px;
  }

  .desktop-pet-card {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 300px;
    max-width: 100%;
    min-height: 380px;
    padding: 20px;
    background: var(--color-panel);
    border: 1.5px solid var(--color-line);
    border-radius: 16px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
    cursor: pointer;
    transition: all 0.2s ease;
    user-select: none;
    box-sizing: border-box;

    &:hover {
      box-shadow: 0 6px 20px rgba(0, 0, 0, 0.06);
      transform: translateY(-2px);
    }

    &--active {
      border-color: #3b82f6;
      box-shadow:
        0 0 0 1px #3b82f6,
        0 4px 16px rgba(59, 130, 246, 0.1);
    }

    &:focus-visible {
      outline: 2px solid #2563eb;
      outline-offset: 2px;
    }

    .desktop-pet-badge {
      position: absolute;
      top: 16px;
      right: 16px;
      display: inline-flex;
      align-items: center;
      gap: 5px;
      padding: 3px 9px;
      border-radius: 9999px;
      background: #f1f5f9;
      border: 1px solid #e2e8f0;
      color: #64748b;
      font-size: 12px;
      font-weight: 500;

      .desktop-badge-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: #94a3b8;
      }

      &--running {
        background: #ecfdf5;
        border-color: #d1fae5;
        color: #16a34a;

        .desktop-badge-dot {
          background: #16a34a;
        }
      }
    }

    .desktop-pet-preview-wrap {
      display: flex;
      align-items: center;
      justify-content: center;
      height: 140px;
      margin: 8px 0 14px;
    }

    .desktop-pet-meta {
      display: flex;
      flex-direction: column;
      flex: 1;

      .desktop-pet-name {
        margin: 0;
        font-size: 18px;
        font-weight: 700;
        color: var(--color-text);
      }

      .desktop-pet-action-count {
        margin-top: 4px;
        font-size: 13px;
        color: var(--color-text-muted);
      }

      .desktop-pet-desc {
        margin: 8px 0 0;
        font-size: 13px;
        color: var(--color-text-muted);
        line-height: 1.5;
        min-height: 40px;
      }
    }

    .desktop-pet-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-top: auto;
      padding-top: 14px;
      border-top: 1px solid var(--color-line);

      .desktop-pet-switch-wrap {
        display: flex;
        align-items: center;
        gap: 8px;

        .desktop-pet-switch-text {
          font-size: 13px;
          color: var(--color-text);
          font-weight: 500;
        }
      }

      .desktop-pet-current-badge {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        padding: 4px 10px;
        border-radius: 6px;
        background: #eff6ff;
        color: #2563eb;
        font-size: 12px;
        font-weight: 600;

        .desktop-star-icon {
          fill: #2563eb;
          color: #2563eb;
        }
      }
    }
  }

  .desktop-empty-hint {
    margin: 0;
    padding: 32px;
    text-align: center;
    color: var(--color-text-muted);
    border: 1px dashed var(--color-line);
    border-radius: 12px;
  }
}
</style>

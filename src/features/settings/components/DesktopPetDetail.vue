<template>
  <BaseModal
    class="settings-desktop-detail"
    :title="`${pet.name} · 桌宠详情`"
    description="预览桌宠的全部动作，也可以直接启动这只桌宠。"
    role="dialog"
    aria-modal="true"
    :aria-label="`${pet.name}的详情`"
    @close="$emit('close')"
    @keydown.esc.stop="$emit('close')"
  >
    <div class="settings-desktop-detail-body">
      <div class="settings-desktop-detail-summary">
        <div class="settings-desktop-detail-preview">
          <DesktopPet :pet="pet" />
        </div>
        <div class="settings-desktop-copy">
          <h3 class="settings-desktop-detail-name">{{ pet.name }}</h3>
          <span class="settings-desktop-description">
            {{ pet.animations.length }} 个动作 · 自动轮换播放
          </span>
          <span class="settings-desktop-description">
            启动后陪伴桌面，下面可以预览每个动作。
          </span>
          <button
            class="settings-desktop-button settings-desktop-start"
            type="button"
            :disabled="pending || running"
            @click="$emit('start', pet)"
          >
            <Play :size="14" />
            {{ running ? "运行中" : "启动这只桌宠" }}
          </button>
        </div>
      </div>
      <section class="settings-desktop-animations">
        <div class="settings-desktop-copy">
          <span data-emphasis>全部动作</span>
          <span class="settings-desktop-description"
            >每个动作独立循环播放。</span
          >
        </div>
        <div class="settings-desktop-animation-list">
          <article
            v-for="animation in pet.animations"
            :key="`${pet.id}:${animation.id}`"
            class="settings-desktop-animation"
          >
            <div class="settings-desktop-animation-preview">
              <DesktopPet :pet="pet" :action="animation.id" />
            </div>
            <span class="settings-desktop-animation-name">{{
              animation.name
            }}</span>
          </article>
        </div>
      </section>
    </div>
  </BaseModal>
</template>

<script setup>
import { Play } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import DesktopPet from "@/components/DesktopPet.vue"

// 详情只展示传入桌宠，关闭与启动均通过事件交给设置页。
defineProps({
  pet: { type: Object, required: true },
  running: { type: Boolean, default: false },
  pending: { type: Boolean, default: false }
})

defineEmits(["close", "start"])
</script>

<style scoped lang="less">
.settings-desktop-detail {
  :deep(.base-modal__panel) {
    width: 820px;
  }
  .settings-desktop-detail-body {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 20px;
    overflow-y: auto;
    padding-right: 4px;
    .settings-desktop-copy {
      display: flex;
      flex-direction: column;
      gap: 6px;
      .settings-desktop-description {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        line-height: 1.5;
      }
    }
    .settings-desktop-detail-summary {
      display: flex;
      flex: none;
      align-items: center;
      gap: 24px;
      padding: 18px;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel-soft);
      .settings-desktop-detail-preview {
        width: 144px;
        height: 144px;
        flex: none;
      }
      .settings-desktop-copy {
        align-items: flex-start;
        .settings-desktop-detail-name {
          margin: 0 0 6px;
          font-size: var(--font-size-xl);
        }
        .settings-desktop-start {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 6px;
          min-height: 32px;
          margin-top: 8px;
          padding: 0 12px;
          border: 1px solid var(--color-primary-solid);
          border-radius: 7px;
          background: var(--color-primary-solid);
          color: #ffffff;
          cursor: pointer;
          font-size: var(--font-size-sm);
          &:disabled {
            opacity: 0.56;
            cursor: not-allowed;
          }
        }
      }
    }
    .settings-desktop-animations {
      display: flex;
      flex-direction: column;
      gap: 14px;
      .settings-desktop-animation-list {
        display: flex;
        flex-wrap: wrap;
        gap: 12px;
        .settings-desktop-animation {
          display: flex;
          width: 136px;
          flex-direction: column;
          flex: none;
          align-items: center;
          gap: 8px;
          padding: 10px;
          border: 1px solid var(--color-line);
          border-radius: 10px;
          background: var(--color-panel-soft);
          .settings-desktop-animation-preview {
            width: 112px;
            height: 112px;
          }
          .settings-desktop-animation-name {
            color: var(--color-text);
            font-size: var(--font-size-sm);
            line-height: 20px;
          }
        }
      }
    }
  }
}
</style>

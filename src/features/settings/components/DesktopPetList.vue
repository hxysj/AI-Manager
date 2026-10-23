<template>
  <div class="settings-desktop-selection">
    <div class="settings-desktop-toolbar">
      <div class="settings-desktop-copy">
        <span data-emphasis>已有桌宠 · {{ pets.length }}</span>
        <span class="settings-desktop-description">选中后可直接启动，也可保存选择，留待下次使用。</span>
      </div>
      <button
        class="settings-desktop-button settings-desktop-start"
        type="button"
        :disabled="pending || !selectedPet || runningPetId === modelValue"
        @click="$emit('start', selectedPet)"
      >
        <Play :size="14" />
        {{ runningPetId && runningPetId === modelValue ? '运行中' : '启动所选桌宠' }}
      </button>
    </div>
    <div class="settings-desktop-pets">
      <article
        v-for="pet in pets"
        :key="pet.id"
        class="settings-desktop-pet"
        :class="{
          'settings-desktop-pet-active': modelValue === pet.id
        }"
      >
        <button
          class="settings-desktop-pet-detail"
          type="button"
          :aria-label="`查看${pet.name}的详情`"
          @click="$emit('detail', pet.id)"
        >
          <span v-if="runningPetId === pet.id" class="settings-desktop-pet-running">运行中</span>
          <span class="settings-desktop-preview"><DesktopPet :pet="pet" /></span>
          <span class="settings-desktop-pet-name">{{ pet.name }}</span>
          <span class="settings-desktop-description">{{ pet.animations.length }} 个动作</span>
          <span class="settings-desktop-detail-link">查看详情 <ChevronRight :size="13" /></span>
        </button>
        <label class="settings-desktop-pet-choice">
          <input
            :checked="modelValue === pet.id"
            class="settings-desktop-pet-radio"
            type="radio"
            name="desktop-pet"
            :value="pet.id"
            @change="$emit('update:modelValue', pet.id)"
          />
          <span>{{ modelValue === pet.id ? '已选中' : '选择启动' }}</span>
        </label>
      </article>
    </div>
    <p v-if="!pets.length" class="settings-desktop-hint">暂无可用桌宠。</p>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { ChevronRight, Play } from 'lucide-vue-next'
import DesktopPet from '@/components/DesktopPet.vue'

const props = defineProps({
  pets: { type: Array, required: true },
  modelValue: { type: String, default: '' },
  runningPetId: { type: String, default: '' },
  pending: { type: Boolean, default: false }
})

defineEmits(['update:modelValue', 'detail', 'start'])

// 列表只管理选中项，浏览详情与启动由父组件分别处理。
const selectedPet = computed(() => props.pets.find(pet => pet.id === props.modelValue))
</script>

<style scoped lang="less">
.settings-desktop-selection {
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
  .settings-desktop-button {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-sm);
    &:hover:not(:disabled) {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }
    &:disabled {
      opacity: 0.56;
      cursor: not-allowed;
    }
    &.settings-desktop-start {
      border-color: var(--color-primary-solid);
      background: var(--color-primary-solid);
      color: #ffffff;
    }
  }
  .settings-desktop-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-top: 6px;
  .settings-desktop-pets {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    .settings-desktop-pet {
      display: flex;
      width: 210px;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      transition:
        border-color 160ms,
        background-color 160ms;
      &.settings-desktop-pet-active {
        border-color: var(--color-primary);
      }
      &:focus-within {
        outline: 1px solid var(--color-primary);
        outline-offset: 3px;
      }
      .settings-desktop-pet-detail {
        position: relative;
        display: flex;
        width: 100%;
        flex-direction: column;
        align-items: center;
        gap: 6px;
        padding: 18px 14px 14px;
        border: 0;
        background: var(--color-panel-soft);
        color: var(--color-text);
        cursor: pointer;
        &:hover {
          background: var(--color-primary-soft);
        }
        .settings-desktop-preview {
          display: flex;
          width: 120px;
          height: 120px;
          margin-bottom: 8px;
        }
        .settings-desktop-pet-name {
          font-size: var(--font-size-base);
        }
        .settings-desktop-description {
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }
        .settings-desktop-detail-link {
          display: inline-flex;
          align-items: center;
          gap: 3px;
          margin-top: 5px;
          color: var(--color-primary);
          font-size: var(--font-size-sm);
        }
        .settings-desktop-pet-running {
          position: absolute;
          top: 10px;
          right: 10px;
          padding: 3px 7px;
          border-radius: 5px;
          background: var(--color-success-soft);
          color: var(--color-success);
          font-size: var(--font-size-xs);
        }
      }
      .settings-desktop-pet-choice {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
        padding: 11px;
        border-top: 1px solid var(--color-line);
        color: var(--color-text);
        cursor: pointer;
        font-size: var(--font-size-sm);
        &:has(:checked) {
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
        .settings-desktop-pet-radio {
          margin: 0;
          accent-color: var(--color-primary);
        }
      }
    }
  }
  .settings-desktop-hint {
    margin: 0;
    padding: 32px 16px;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }
}
</style>

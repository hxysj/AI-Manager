<template>
  <aside :class="['app-sidebar', { 'app-sidebar--collapsed': collapsed }]">
    <div class="app-sidebar__header">
      <button
        class="app-sidebar__logo-button"
        type="button"
        :aria-label="collapsed ? '展开侧边栏' : '收起侧边栏'"
        @click="$emit('toggle')"
      >
        <img class="app-sidebar__logo" :src="logoUrl" alt="Monkey Thief 图标" />
      </button>
      <span
        v-if="!collapsed"
        class="app-sidebar__title"
        @click="$emit('title-click')"
      >
        Monkey Thief
      </span>
      <button
        v-if="!collapsed"
        class="app-sidebar-assistant"
        type="button"
        title="打开应用助手"
        aria-label="打开应用助手"
        @click="$emit('open-assistant')"
      >
        <Bot :size="17" :stroke-width="1.9" />
      </button>
      <button
        v-if="!collapsed"
        class="app-sidebar__theme-toggle"
        type="button"
        :title="themeMode === 'dark' ? '切换为亮色模式' : '切换为暗色模式'"
        :aria-label="themeMode === 'dark' ? '切换为亮色模式' : '切换为暗色模式'"
        @click="$emit('toggle-theme')"
      >
        <Sun v-if="themeMode === 'dark'" :size="17" :stroke-width="1.9" />
        <Moon v-else :size="17" :stroke-width="1.9" />
      </button>
    </div>

    <el-scrollbar
      class="app-sidebar-nav-scroll"
      view-class="app-sidebar__nav"
      tag="nav"
      aria-label="主导航"
      tabindex="0"
      :style="{ '--sidebar-nav-count': navItems.length }"
    >
      <button
        v-for="item in navItems"
        :key="item.id"
        :class="[
          'app-sidebar__nav-item',
          { 'app-sidebar__nav-item--active': activeView === item.id }
        ]"
        type="button"
        @click="$emit('select-view', item.id)"
      >
        <span class="app-sidebar__nav-icon">
          <component :is="item.icon" :size="17" :stroke-width="1.9" />
        </span>
        <span v-if="!collapsed" class="app-sidebar__nav-label">{{
          item.label
        }}</span>
      </button>
    </el-scrollbar>
  </aside>
</template>

<script setup>
import { Bot, Moon, Sun } from "lucide-vue-next"
import { ElScrollbar } from "element-plus"
import "element-plus/es/components/scrollbar/style/css"
import logoUrl from "@/assets/ai-manager-logo.png?url"

defineProps({
  activeView: {
    type: String,
    required: true
  },
  collapsed: {
    type: Boolean,
    required: true
  },
  navItems: {
    type: Array,
    required: true
  },
  themeMode: {
    type: String,
    default: "light"
  }
})

defineEmits([
  "toggle",
  "toggle-theme",
  "select-view",
  "title-click",
  "open-assistant"
])
</script>

<style scoped lang="less">
.app-sidebar {
  --sidebar-nav-item-min-height: 36px;
  --sidebar-nav-item-max-height: 48px;
  --sidebar-nav-gap: 5px;
  --sidebar-nav-padding: 16px;
  display: flex;
  width: 260px;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border-right: 1px solid var(--color-line);
  background: var(--color-panel);
  transition: width 0.24s ease;

  &--collapsed {
    width: 86px;
  }

  &--collapsed &__header {
    justify-content: center;
    padding-right: 0;
    padding-left: 0;
  }

  &.app-sidebar--collapsed {
    .app-sidebar-nav-scroll {
      :deep(.app-sidebar__nav) {
        align-items: center;
        padding-right: 8px;
        padding-left: 8px;
      }
    }
  }

  &--collapsed &__nav-item {
    width: 48px;
    height: 48px;
    justify-content: center;
    padding: 0;
  }

  &--collapsed &__nav-item:hover {
    transform: none;
  }

  &--collapsed &__nav-item--active::before {
    left: -8px;
  }

  &__header {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    padding: 18px 18px 14px;
    border-bottom: 1px solid var(--color-line);
    .app-sidebar-assistant {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: center;
      width: 26px;
      height: 32px;
      margin-left: auto;
      padding: 0;
      border: 0;
      border-radius: 7px;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      &:hover {
        color: var(--color-primary);
        background: var(--color-primary-soft);
      }
    }
  }

  &__logo-button {
    display: grid;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: pointer;
  }

  &__logo {
    width: 34px;
    height: 34px;
    object-fit: contain;
  }

  &__title {
    color: var(--color-primary);
    cursor: pointer;
    font-size: var(--font-size-lg);
  }

  &__theme-toggle {
    display: grid;
    width: 32px;
    height: 32px;
    flex: none;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &:not(.app-sidebar--collapsed) &__theme-toggle {
    margin-left: -8px;
  }

  &__theme-toggle:hover {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__theme-toggle :deep(svg) {
    display: block;
  }

  .app-sidebar-nav-scroll {
    flex: 1;
    min-height: 0;
    height: auto;
    --el-scrollbar-bg-color: var(--color-text-muted);
    --el-scrollbar-hover-bg-color: var(--color-text-muted);

    :deep(.el-scrollbar__wrap) {
      overflow-x: hidden;
    }

    :deep(.app-sidebar__nav) {
      display: flex;
      height: 100%;
      min-height: calc(
        var(--sidebar-nav-count) * var(--sidebar-nav-item-min-height) +
          (var(--sidebar-nav-count) - 1) * var(--sidebar-nav-gap) +
          var(--sidebar-nav-padding) * 2
      );
      flex-direction: column;
      gap: var(--sidebar-nav-gap);
      padding: var(--sidebar-nav-padding) 12px;
    }
  }

  &__nav-item {
    position: relative;
    display: flex;
    flex: 0 1 var(--sidebar-nav-item-max-height);
    min-height: var(--sidebar-nav-item-min-height);
    max-height: var(--sidebar-nav-item-max-height);
    align-items: center;
    gap: 12px;
    padding: 0 14px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    text-align: left;
    transition:
      transform 0.18s ease,
      background-color 0.18s ease,
      color 0.18s ease;
  }

  &__nav-item:hover {
    background: var(--color-panel-soft);
    transform: translateX(2px);
  }

  &__nav-item--active {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__nav-item--active::before {
    content: "";
    position: absolute;
    left: -4px;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 999px;
    background: var(--color-primary-solid);
  }

  &__nav-icon {
    display: grid;
    width: 20px;
    height: 20px;
    flex: 0 0 20px;
    color: var(--color-text-muted);
    place-items: center;
  }

  &__nav-label {
    font-size: var(--font-size-base);
  }
}
</style>

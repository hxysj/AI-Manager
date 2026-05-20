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
      <span v-if="!collapsed" class="app-sidebar__title">Monkey Thief</span>
    </div>

    <nav class="app-sidebar__nav">
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
    </nav>

    <section class="app-sidebar__section">
      <div v-if="!collapsed" class="app-sidebar__section-header">
        Detected CLI
      </div>
      <div class="app-sidebar__cli-list">
        <article
          v-for="cli in cliTargets"
          :key="cli.id"
          :class="[
            'app-sidebar__cli-card',
            { 'app-sidebar__cli-card--offline': !cli.installed }
          ]"
        >
          <span class="app-sidebar__cli-icon">
            <AiIcon
              v-if="cli.icon"
              class="app-sidebar__cli-icon-image"
              :name="cli.icon"
              :alt="`${cli.name} 图标`"
            />
            <span
              v-else
              class="app-sidebar__cli-swatch"
              :style="{ background: colorMap[cli.id] || colorMap.default }"
            ></span>
          </span>
          <div v-if="!collapsed" class="app-sidebar__cli-info">
            <strong>{{ cli.name }}</strong>
            <small>{{ cli.installed ? "已检测" : "未安装" }}</small>
          </div>
          <span
            v-if="!collapsed"
            :class="[
              'app-sidebar__cli-dot',
              { 'app-sidebar__cli-dot--offline': !cli.installed }
            ]"
          ></span>
        </article>
      </div>
    </section>
  </aside>
</template>

<script setup>
import AiIcon from '@/components/AiIcon.vue'
import logoUrl from '@/assets/ai-manager-logo.svg?url'

defineProps({
  activeView: {
    type: String,
    required: true
  },
  cliTargets: {
    type: Array,
    required: true
  },
  collapsed: {
    type: Boolean,
    required: true
  },
  navItems: {
    type: Array,
    required: true
  }
})

defineEmits(['toggle', 'select-view'])

const colorMap = {
  claude: '#c58f72',
  codex: '#7d8aa3',
  gemini: '#9fb5d6',
  trae: '#4d8dff',
  'trae-cn': '#2fbea2',
  ['open' + 'code']: '#aaa3c7',
  default: '#a8b0bd'
}
</script>

<style scoped lang="less">
.app-sidebar {
  display: flex;
  width: 260px;
  min-height: 0;
  flex-direction: column;
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

  &--collapsed &__nav {
    align-items: center;
    padding-right: 8px;
    padding-left: 8px;
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

  &--collapsed &__section {
    padding: 12px 0 14px;
  }

  &--collapsed &__cli-list {
    align-items: center;
    gap: 10px;
  }

  &--collapsed &__cli-card {
    width: 38px;
    height: 38px;
    padding: 0;
    border-color: transparent;
    border-radius: 10px;
    background: transparent;
  }

  &--collapsed &__cli-card:hover {
    background: #f4f6fa;
  }

  &__header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 18px 18px 14px;
    border-bottom: 1px solid var(--color-line);
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
    font-size: 1.02rem;
    font-weight: 700;
  }

  &__nav {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 5px;
    padding: 16px 12px;
  }

  &__nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
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
    background: #f4f6fa;
    transform: translateX(2px);
  }

  &__nav-item--active {
    background: #edf1f7;
    color: #263f63;
  }

  &__nav-item--active::before {
    content: '';
    position: absolute;
    left: -4px;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 999px;
    background: #506b91;
  }

  &__nav-icon {
    display: grid;
    width: 20px;
    height: 20px;
    flex: 0 0 20px;
    color: #6a7890;
    place-items: center;
  }

  &__nav-label {
    font-size: 0.92rem;
    font-weight: 600;
  }

  &__section {
    display: flex;
    max-height: 42%;
    min-height: 0;
    flex: 0 1 auto;
    flex-direction: column;
    padding: 0 12px 14px;
    border-top: 1px solid var(--color-line);
  }

  &__section-header {
    padding: 14px 8px 10px;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  &__cli-list {
    display: flex;
    overflow-y: auto;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
    padding-right: 2px;
  }

  &__cli-card {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 8px 8px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__cli-card--offline {
    opacity: 0.62;
  }

  &__cli-icon {
    display: grid;
    width: 22px;
    height: 22px;
    flex: 0 0 22px;
    place-items: center;
  }

  &__cli-icon-image {
    width: 22px;
    height: 22px;
    object-fit: contain;
  }

  &__cli-swatch {
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    border-radius: 50%;
  }

  &__cli-info {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  &__cli-info strong {
    font-size: 0.88rem;
  }

  &__cli-info small {
    color: var(--color-text-soft);
    font-size: 0.76rem;
  }

  &__cli-dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--color-success);
  }

  &__cli-dot--offline {
    background: #9ca3af;
  }
}
</style>

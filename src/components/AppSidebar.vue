<template>
  <aside :class="['app-sidebar', { 'app-sidebar--collapsed': collapsed }]">
    <div class="app-sidebar__header">
      <button class="app-sidebar__brand" type="button" @click="$emit('select-view', 'dashboard')">
        <span class="app-sidebar__logo"></span>
        <span v-if="!collapsed" class="app-sidebar__title">AI Manager</span>
      </button>
      <button class="app-sidebar__toggle" type="button" @click="$emit('toggle')">
        <span>{{ collapsed ? '›' : '‹' }}</span>
      </button>
    </div>

    <nav class="app-sidebar__nav">
      <button
        v-for="item in navItems"
        :key="item.id"
        :class="['app-sidebar__nav-item', { 'app-sidebar__nav-item--active': activeView === item.id }]"
        type="button"
        @click="$emit('select-view', item.id)"
      >
        <span class="app-sidebar__nav-icon">{{ item.icon }}</span>
        <span v-if="!collapsed" class="app-sidebar__nav-label">{{ item.label }}</span>
      </button>
    </nav>

    <section class="app-sidebar__section">
      <div v-if="!collapsed" class="app-sidebar__section-header">Detected CLI</div>
      <div class="app-sidebar__cli-list">
        <article
          v-for="cli in cliTargets"
          :key="cli.id"
          :class="['app-sidebar__cli-card', { 'app-sidebar__cli-card--offline': !cli.installed }]"
        >
          <span class="app-sidebar__cli-swatch" :style="{ background: colorMap[cli.id] || colorMap.default }"></span>
          <div v-if="!collapsed" class="app-sidebar__cli-info">
            <strong>{{ cli.name }}</strong>
            <small>{{ cli.installed ? '已检测' : '未安装' }}</small>
          </div>
          <span :class="['app-sidebar__cli-dot', { 'app-sidebar__cli-dot--offline': !cli.installed }]"></span>
        </article>
      </div>
    </section>
  </aside>
</template>

<script setup>
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
  claude: 'linear-gradient(135deg, #ce7647, #9f4927)',
  codex: 'linear-gradient(135deg, #0d8d76, #125b80)',
  gemini: 'linear-gradient(135deg, #4a7cf7, #4f46e5)',
  opencode: 'linear-gradient(135deg, #7747db, #4325b8)',
  default: 'linear-gradient(135deg, #5f6b7a, #334155)'
}
</script>

<style scoped lang="less">
.app-sidebar {
  display: flex;
  width: 260px;
  flex-direction: column;
  border-right: 1px solid rgba(57, 71, 103, 0.12);
  background:
    linear-gradient(180deg, rgba(255, 248, 240, 0.94), rgba(246, 240, 229, 0.9)),
    radial-gradient(circle at top left, rgba(193, 89, 31, 0.16), transparent 40%);
  backdrop-filter: blur(18px);
  transition: width 0.24s ease;
}

.app-sidebar--collapsed {
  width: 86px;
}

.app-sidebar__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 18px 18px 14px;
  border-bottom: 1px solid rgba(57, 71, 103, 0.1);
}

.app-sidebar__brand {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  border: 0;
  background: transparent;
  color: inherit;
  cursor: pointer;
}

.app-sidebar__logo {
  width: 34px;
  height: 34px;
  border-radius: 12px;
  background:
    linear-gradient(150deg, rgba(255, 255, 255, 0.72), transparent 40%),
    linear-gradient(135deg, #d46a2a, #2b6cb0);
  box-shadow: 0 10px 24px rgba(27, 55, 101, 0.18);
}

.app-sidebar__title {
  font-family: 'Georgia', 'Times New Roman', serif;
  font-size: 1.08rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.app-sidebar__toggle {
  display: grid;
  width: 32px;
  height: 32px;
  place-items: center;
  border: 1px solid rgba(57, 71, 103, 0.12);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.72);
  color: #40516e;
  cursor: pointer;
}

.app-sidebar__nav {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 6px;
  padding: 18px 12px;
}

.app-sidebar__nav-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 13px 14px;
  border: 0;
  border-radius: 16px;
  background: transparent;
  color: #3b4c68;
  cursor: pointer;
  text-align: left;
  transition:
    transform 0.18s ease,
    background-color 0.18s ease,
    color 0.18s ease;
}

.app-sidebar__nav-item:hover {
  background: rgba(255, 255, 255, 0.74);
  transform: translateX(2px);
}

.app-sidebar__nav-item--active {
  background:
    linear-gradient(135deg, rgba(38, 92, 183, 0.14), rgba(213, 110, 41, 0.12)),
    rgba(255, 255, 255, 0.88);
  color: #1f314f;
  box-shadow: inset 0 0 0 1px rgba(58, 93, 156, 0.08);
}

.app-sidebar__nav-item--active::before {
  content: '';
  position: absolute;
  left: -4px;
  top: 9px;
  bottom: 9px;
  width: 4px;
  border-radius: 999px;
  background: linear-gradient(180deg, #c65d20, #245ea1);
}

.app-sidebar__nav-icon {
  width: 22px;
  flex: 0 0 22px;
  font-size: 1rem;
  text-align: center;
}

.app-sidebar__nav-label {
  font-size: 0.96rem;
  font-weight: 600;
}

.app-sidebar__section {
  padding: 0 12px 14px;
  border-top: 1px solid rgba(57, 71, 103, 0.08);
}

.app-sidebar__section-header {
  padding: 14px 8px 10px;
  color: rgba(43, 57, 84, 0.58);
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

.app-sidebar__cli-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.app-sidebar__cli-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.68);
}

.app-sidebar__cli-card--offline {
  opacity: 0.62;
}

.app-sidebar__cli-swatch {
  width: 18px;
  height: 18px;
  flex: 0 0 18px;
  border-radius: 6px;
}

.app-sidebar__cli-info {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
}

.app-sidebar__cli-info strong {
  font-size: 0.88rem;
}

.app-sidebar__cli-info small {
  color: rgba(43, 57, 84, 0.58);
  font-size: 0.76rem;
}

.app-sidebar__cli-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #0d9d6b;
  box-shadow: 0 0 0 4px rgba(13, 157, 107, 0.14);
}

.app-sidebar__cli-dot--offline {
  background: #9ca3af;
  box-shadow: 0 0 0 4px rgba(156, 163, 175, 0.14);
}

@media (max-width: 1100px) {
  .app-sidebar {
    width: 88px;
  }

  .app-sidebar__title,
  .app-sidebar__nav-label,
  .app-sidebar__section-header,
  .app-sidebar__cli-info {
    display: none;
  }
}
</style>

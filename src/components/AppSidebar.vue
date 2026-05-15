<template>
  <aside :class="['app-sidebar', { 'app-sidebar--collapsed': collapsed }]">
    <div class="app-sidebar__header">
      <button
        class="app-sidebar__logo-button"
        type="button"
        :aria-label="collapsed ? '展开侧边栏' : '收起侧边栏'"
        @click="$emit('toggle')"
      >
        <span class="app-sidebar__logo"></span>
      </button>
      <span v-if="!collapsed" class="app-sidebar__title">AI Manager</span>
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
        <span class="app-sidebar__nav-icon">{{ item.icon }}</span>
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
            <img
              v-if="resolveCliIcon(cli.icon)"
              class="app-sidebar__cli-icon-image"
              :src="resolveCliIcon(cli.icon)"
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
import claudeCodeIcon from "@/assets/ai-icons/claudecode.svg"
import codexIcon from "@/assets/ai-icons/codex.svg"
import geminiCliIcon from "@/assets/ai-icons/geminicli.svg"

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

defineEmits(["toggle", "select-view"])

const iconAssetMap = {
  "claudecode.svg": claudeCodeIcon,
  "codex.svg": codexIcon,
  "geminicli.svg": geminiCliIcon
}

const colorMap = {
  claude: "#c58f72",
  codex: "#86b8aa",
  gemini: "#9fb5d6",
  ["open" + "code"]: "#aaa3c7",
  default: "#a8b0bd"
}

function resolveCliIcon(icon) {
  return iconAssetMap[icon]
}
</script>

<style scoped lang="less">
.app-sidebar {
  display: flex;
  width: 260px;
  flex-direction: column;
  border-right: 1px solid var(--color-line);
  background: var(--color-panel);
  transition: width 0.24s ease;
}

.app-sidebar--collapsed {
  width: 86px;
}

.app-sidebar__header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 18px 18px 14px;
  border-bottom: 1px solid var(--color-line);
}

.app-sidebar__logo-button {
  display: grid;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: pointer;
}

.app-sidebar__logo {
  width: 34px;
  height: 34px;
  background: url("@/assets/ai-manager-logo.svg") center / contain no-repeat;
}

.app-sidebar__title {
  font-size: 1.08rem;
  font-weight: 700;
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

.app-sidebar__nav-item:hover {
  background: var(--color-panel-soft);
  transform: translateX(2px);
}

.app-sidebar__nav-item--active {
  background: var(--color-primary-soft);
  color: var(--color-text);
}

.app-sidebar__nav-item--active::before {
  content: "";
  position: absolute;
  left: -4px;
  top: 9px;
  bottom: 9px;
  width: 4px;
  border-radius: 999px;
  background: #8b95a6;
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
  border-top: 1px solid var(--color-line);
}

.app-sidebar__section-header {
  padding: 14px 8px 10px;
  color: var(--color-text-soft);
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
  justify-content: center;
  gap: 10px;
  padding: 8px 8px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
}

.app-sidebar__cli-card--offline {
  opacity: 0.62;
}

.app-sidebar__cli-icon {
  display: grid;
  width: 22px;
  height: 22px;
  flex: 0 0 22px;
  place-items: center;
}

.app-sidebar__cli-icon-image {
  width: 22px;
  height: 22px;
  object-fit: contain;
}

.app-sidebar__cli-swatch {
  width: 18px;
  height: 18px;
  flex: 0 0 18px;
  border-radius: 50%;
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
  color: var(--color-text-soft);
  font-size: 0.76rem;
}

.app-sidebar__cli-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: var(--color-success);
}

.app-sidebar__cli-dot--offline {
  background: #9ca3af;
}
</style>

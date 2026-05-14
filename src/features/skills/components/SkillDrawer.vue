<template>
  <div v-if="skill" class="skill-drawer">
    <div class="skill-drawer__overlay" @click="$emit('close')"></div>
    <aside class="skill-drawer__panel">
      <header class="skill-drawer__header">
        <div class="skill-drawer__hero">
          <div class="skill-drawer__icon" :style="{ background: hashColor(skill.name) }">
            {{ iconLetters(skill.name) }}
          </div>
          <div class="skill-drawer__title-wrap">
            <p>{{ skill.repoName }}</p>
            <h2>{{ skill.name }}</h2>
            <span :class="['skill-drawer__headline-status', `skill-drawer__headline-status--${skill.status}`]">
              {{ formatStatusLabel(skill.status) }}
            </span>
          </div>
        </div>
        <button class="skill-drawer__close" type="button" @click="$emit('close')">×</button>
      </header>

      <div class="skill-drawer__tabs">
        <button
          v-for="item in tabs"
          :key="item.id"
          :class="['skill-drawer__tab', { 'skill-drawer__tab--active': activeTab === item.id }]"
          type="button"
          @click="activeTab = item.id"
        >
          {{ item.label }}
        </button>
      </div>

      <div class="skill-drawer__content">
        <section v-if="activeTab === 'overview'" class="skill-drawer__section">
          <div class="skill-drawer__block">
            <span>描述</span>
            <p>{{ skill.description || '未提供描述。' }}</p>
          </div>

          <div class="skill-drawer__block">
            <span>标签</span>
            <div class="skill-drawer__tag-list">
              <strong v-for="tag in skill.tags" :key="tag">{{ tag }}</strong>
              <strong v-if="!skill.tags.length" class="skill-drawer__muted-tag">暂无标签</strong>
            </div>
          </div>

          <div class="skill-drawer__grid">
            <article>
              <span>Entry</span>
              <strong>{{ skill.entry }}</strong>
            </article>
            <article>
              <span>创建时间</span>
              <strong>{{ formatDateTime(skill.createdAt) }}</strong>
            </article>
            <article>
              <span>更新时间</span>
              <strong>{{ formatDateTime(skill.updatedAt) }}</strong>
            </article>
            <article>
              <span>作者</span>
              <strong>{{ skill.author || '未声明' }}</strong>
            </article>
          </div>
        </section>

        <section v-if="activeTab === 'targets'" class="skill-drawer__section">
          <article v-for="cli in cliTargets" :key="cli.id" class="skill-drawer__target-card">
            <div class="skill-drawer__target-head">
              <div>
                <h3>{{ cli.name }}</h3>
                <p>{{ cli.skillsPath || '该 CLI 不支持 Skill 目录' }}</p>
              </div>
              <span :class="['skill-drawer__state-pill', `skill-drawer__state-pill--${skill.installStates[cli.id]?.state}`]">
                {{ formatStatusLabel(skill.installStates[cli.id]?.state) }}
              </span>
            </div>
            <div class="skill-drawer__target-actions">
              <button
                v-if="skill.installStates[cli.id]?.state === 'installed'"
                class="action-button"
                type="button"
                @click="$emit('uninstall', { skillName: skill.name, targetId: cli.id })"
              >
                卸载
              </button>
              <button
                v-else-if="skill.installStates[cli.id]?.state === 'broken-link'"
                class="action-button action-button--alert"
                type="button"
                @click="$emit('repair', { skillName: skill.name, targetId: cli.id })"
              >
                修复链接
              </button>
              <button
                v-else
                class="action-button action-button--primary"
                type="button"
                :disabled="!cli.installed"
                @click="$emit('install', { skillName: skill.name, targetId: cli.id })"
              >
                安装
              </button>
              <button class="action-button" type="button" @click="$emit('open-path', cli.skillsPath)">
                打开目录
              </button>
            </div>
          </article>
        </section>

        <section v-if="activeTab === 'files'" class="skill-drawer__section">
          <div class="skill-drawer__block">
            <span>源目录</span>
            <button class="skill-drawer__path-button" type="button" @click="$emit('open-path', skill.sourcePath)">
              {{ skill.sourcePath }}
            </button>
          </div>
          <div class="skill-drawer__block">
            <span>入口文件</span>
            <button class="skill-drawer__path-button" type="button" @click="$emit('open-path', skill.entryPath)">
              {{ skill.entryPath }}
            </button>
          </div>
          <div class="skill-drawer__block">
            <span>图标文件</span>
            <p>{{ skill.icon || '未提供 icon，将使用默认图标。' }}</p>
          </div>
        </section>
      </div>
    </aside>
  </div>
</template>

<script setup>
import { ref, watch } from 'vue'
import { formatDateTime, formatStatusLabel, hashColor, iconLetters } from '@/utils/formatters'

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  skill: {
    type: Object,
    default: null
  }
})

defineEmits(['close', 'install', 'uninstall', 'repair', 'open-path'])

const tabs = [
  { id: 'overview', label: 'Overview' },
  { id: 'targets', label: 'Targets' },
  { id: 'files', label: 'Files' }
]

const activeTab = ref('overview')

watch(
  () => props.skill?.name,
  () => {
    activeTab.value = 'overview'
  }
)
</script>

<style scoped lang="less">
.skill-drawer {
  position: fixed;
  inset: 0;
  z-index: 30;
}

.skill-drawer__overlay {
  position: absolute;
  inset: 0;
  background: rgba(19, 24, 36, 0.38);
}

.skill-drawer__panel {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  display: flex;
  width: min(560px, 100%);
  flex-direction: column;
  border-left: 1px solid rgba(58, 69, 94, 0.12);
  background:
    linear-gradient(180deg, rgba(255, 252, 248, 0.98), rgba(245, 239, 230, 0.98)),
    #fff;
  box-shadow: -20px 0 48px rgba(24, 35, 58, 0.16);
}

.skill-drawer__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  padding: 24px 24px 18px;
  border-bottom: 1px solid rgba(58, 69, 94, 0.1);
}

.skill-drawer__hero {
  display: flex;
  gap: 16px;
}

.skill-drawer__icon {
  display: grid;
  width: 64px;
  height: 64px;
  place-items: center;
  border-radius: 20px;
  color: #fff;
  font-size: 1.1rem;
  font-weight: 700;
  letter-spacing: 0.08em;
}

.skill-drawer__title-wrap p {
  margin: 0 0 8px;
  color: rgba(43, 57, 84, 0.58);
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
}

.skill-drawer__title-wrap h2 {
  margin: 0 0 10px;
  font-family: 'Georgia', 'Times New Roman', serif;
  font-size: 1.8rem;
  line-height: 1.1;
}

.skill-drawer__headline-status,
.skill-drawer__state-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 6px 12px;
  border-radius: 999px;
  font-size: 0.74rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.skill-drawer__headline-status--installed,
.skill-drawer__state-pill--installed {
  background: rgba(14, 148, 104, 0.12);
  color: #0e7b58;
}

.skill-drawer__headline-status--not-installed,
.skill-drawer__state-pill--not-installed {
  background: rgba(36, 94, 161, 0.1);
  color: #245ea1;
}

.skill-drawer__headline-status--broken-link,
.skill-drawer__state-pill--broken-link {
  background: rgba(220, 38, 38, 0.12);
  color: #b91c1c;
}

.skill-drawer__headline-status--disabled,
.skill-drawer__state-pill--disabled {
  background: rgba(148, 163, 184, 0.18);
  color: #607084;
}

.skill-drawer__close {
  display: grid;
  width: 38px;
  height: 38px;
  place-items: center;
  border: 1px solid rgba(58, 69, 94, 0.12);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.84);
  color: #40516e;
  cursor: pointer;
  font-size: 1.4rem;
  line-height: 1;
}

.skill-drawer__tabs {
  display: flex;
  gap: 8px;
  padding: 14px 24px 0;
  border-bottom: 1px solid rgba(58, 69, 94, 0.1);
}

.skill-drawer__tab {
  position: relative;
  padding: 12px 10px;
  border: 0;
  background: transparent;
  color: rgba(43, 57, 84, 0.58);
  cursor: pointer;
  font-weight: 700;
}

.skill-drawer__tab--active {
  color: #1f5ca2;
}

.skill-drawer__tab--active::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 2px;
  border-radius: 999px;
  background: linear-gradient(90deg, #c65d20, #245ea1);
}

.skill-drawer__content {
  flex: 1;
  overflow: auto;
  padding: 24px;
}

.skill-drawer__section {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.skill-drawer__block {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px;
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.84);
}

.skill-drawer__block span {
  color: rgba(43, 57, 84, 0.58);
  font-size: 0.76rem;
  font-weight: 700;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.skill-drawer__block p {
  margin: 0;
  line-height: 1.7;
}

.skill-drawer__tag-list {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.skill-drawer__tag-list strong {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 6px 12px;
  border-radius: 999px;
  background: rgba(245, 239, 230, 0.82);
  color: #755934;
  font-size: 0.8rem;
}

.skill-drawer__muted-tag {
  color: #728195;
}

.skill-drawer__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.skill-drawer__grid article,
.skill-drawer__target-card {
  padding: 18px;
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.84);
}

.skill-drawer__grid article span {
  display: block;
  margin-bottom: 8px;
  color: rgba(43, 57, 84, 0.58);
  font-size: 0.76rem;
  font-weight: 700;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.skill-drawer__grid article strong {
  font-size: 0.96rem;
  line-height: 1.6;
  word-break: break-word;
}

.skill-drawer__target-card {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.skill-drawer__target-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.skill-drawer__target-head h3 {
  margin: 0 0 8px;
}

.skill-drawer__target-head p {
  margin: 0;
  color: rgba(43, 57, 84, 0.62);
  line-height: 1.6;
  word-break: break-all;
}

.skill-drawer__target-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.skill-drawer__path-button {
  padding: 0;
  border: 0;
  background: transparent;
  color: #214d86;
  cursor: pointer;
  line-height: 1.6;
  text-align: left;
  word-break: break-all;
}

.action-button {
  height: 38px;
  padding: 0 14px;
  border: 1px solid rgba(58, 69, 94, 0.12);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.9);
  color: #2a4366;
  cursor: pointer;
  font-weight: 600;
}

.action-button--primary {
  border-color: rgba(38, 92, 183, 0.2);
  background: linear-gradient(135deg, #1f5ca2, #d66a2c);
  color: #fff;
}

.action-button--alert {
  border-color: rgba(220, 38, 38, 0.2);
  background: rgba(220, 38, 38, 0.08);
  color: #b91c1c;
}

.action-button:disabled {
  cursor: not-allowed;
  opacity: 0.46;
}

@media (max-width: 700px) {
  .skill-drawer__grid {
    grid-template-columns: 1fr;
  }
}
</style>

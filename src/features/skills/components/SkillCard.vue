<template>
  <article class="skill-card" @click="$emit('select', skill)">
    <div class="skill-card__main">
      <div class="skill-card__title-row">
        <h3 class="skill-card__title">{{ skill.name }}</h3>
        <span class="skill-card__repo">{{ skill.repoName }}</span>
        <span
          :class="['skill-card__status', `skill-card__status--${skill.status}`]"
        >
          {{ formatStatusLabel(skill.status) }}
        </span>
      </div>

      <p class="skill-card__description">
        {{ skill.description || "未提供描述，点击查看详情。" }}
      </p>

      <div class="skill-card__meta">
        <span>{{ skill.entry }}</span>
        <span>{{ formatDateTime(skill.updatedAt) }}</span>
        <span v-for="tag in skill.tags.slice(0, 2)" :key="tag">{{ tag }}</span>
      </div>
    </div>

    <div class="skill-card__indicators">
      <!-- <span
        class="skill-card__icon"
        :style="{ background: skill.icon ? '#ffffff' : hashColor(skill.name) }"
      >
        <img
          v-if="skill.icon"
          class="skill-card__icon-image"
          :src="toFileUrl(skill.icon)"
          :alt="skill.name"
        />
        <span v-else>{{ iconLetters(skill.name) }}</span>
      </span> -->
      <button
        v-for="cli in cliTargets"
        :key="cli.id"
        :class="[
          'skill-card__target-pill',
          `skill-card__target-pill--${skill.installStates[cli.id]?.state || 'not-installed'}`
        ]"
        type="button"
        :title="`${cli.name}：${formatStatusLabel(skill.installStates[cli.id]?.state)}`"
        :disabled="skill.installStates[cli.id]?.state === 'disabled'"
        @click.stop="toggleCliSkill(cli)"
      >
        <AiIcon
          v-if="cli.icon"
          class="skill-card__target-icon"
          :name="cli.icon"
          :alt="`${cli.name} 图标`"
        />
        <span v-else>{{ cli.name.slice(0, 1) }}</span>
      </button>
      <button
        class="skill-card__action"
        type="button"
        title="打开源目录"
        @click.stop="$emit('open-source', skill)"
      >
        <FolderOpen class="skill-card__action-icon" :size="15" />
      </button>
    </div>
  </article>
</template>

<script setup>
import { FolderOpen } from 'lucide-vue-next'
import AiIcon from '@/components/AiIcon.vue'
import { formatDateTime, formatStatusLabel } from '@/utils/formatters'

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  skill: {
    type: Object,
    required: true
  }
})

const emit = defineEmits(['select', 'open-source', 'install', 'uninstall'])

function toggleCliSkill(cli) {
  const state = props.skill.installStates[cli.id]?.state
  const payload = {
    skillName: props.skill.name,
    targetId: cli.id
  }

  if (state === 'installed') {
    emit('uninstall', payload)
    return
  }

  emit('install', payload)
}

function toFileUrl(value) {
  return encodeURI(`file:///${String(value).replace(/\\/g, '/')}`)
}
</script>

<style scoped lang="less">
.skill-card {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 18px;
  align-items: center;
  min-height: 56px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--color-line);
  background: var(--color-panel);
  cursor: pointer;
  transition: background-color 0.18s ease;
}

.skill-card:hover {
  background: var(--color-panel-soft);
}

.skill-card__main {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.skill-card__title-row {
  display: flex;
  align-items: center;
  gap: 7px;
  flex-wrap: wrap;
}

.skill-card__title {
  margin: 0;
  color: var(--color-text);
  font-size: 0.95rem;
  line-height: 1.2;
}

.skill-card__repo,
.skill-card__status {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 0.76rem;
  line-height: 1.2;
}

.skill-card__repo {
  color: var(--color-text-soft);
}

.skill-card__status {
  padding: 2px 7px;
  border-radius: 999px;
  font-weight: 600;
}

.skill-card__status--installed {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.skill-card__status--not-installed {
  background: var(--color-primary-soft);
  color: var(--color-text-muted);
}

.skill-card__status--broken-link {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.skill-card__status--disabled {
  background: var(--color-primary-soft);
  color: var(--color-text-soft);
}

.skill-card__description {
  overflow: hidden;
  margin: 0;
  color: var(--color-text-muted);
  font-size: 0.84rem;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.skill-card__meta {
  display: flex;
  gap: 10px;
  overflow: hidden;
  color: var(--color-text-soft);
  font-size: 0.76rem;
  white-space: nowrap;
}

.skill-card__indicators {
  display: flex;
  align-items: center;
  gap: 10px;
}

.skill-card__icon,
.skill-card__target-pill,
.skill-card__action {
  display: inline-grid;
  width: 30px;
  height: 30px;
  place-items: center;
  border: 1px solid var(--color-line);
  border-radius: 50%;
  color: var(--color-text-muted);
  font-size: 0.78rem;
  font-weight: 700;
  overflow: hidden;
}

.skill-card__icon-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.skill-card__target-pill {
  background: var(--color-panel);
  cursor: pointer;
}

.skill-card__target-icon {
  width: 18px;
  height: 18px;
  object-fit: contain;
}

.skill-card__target-pill--installed {
  border-color: #cbd6e4;
  background: var(--color-success-soft);
  color: var(--color-success);
}

.skill-card__target-pill--broken-link {
  border-color: #ead1d1;
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.skill-card__target-pill--disabled {
  background: var(--color-primary-soft);
  color: var(--color-text-soft);
}

.skill-card__target-pill--not-installed {
  border-color: transparent;
  background: var(--color-panel);
  color: var(--color-text-soft);
}

.skill-card__target-pill:disabled {
  cursor: not-allowed;
  opacity: 0.48;
}

.skill-card__action {
  background: var(--color-panel);
  cursor: pointer;
}

.skill-card__action-icon {
  flex: 0 0 auto;
}
</style>

<template>
  <article class="skill-card" @click="$emit('select', skill)">
    <div class="skill-card__icon" :style="{ background: hashColor(skill.name) }">
      <span>{{ iconLetters(skill.name) }}</span>
    </div>

    <div class="skill-card__main">
      <div class="skill-card__header">
        <div class="skill-card__title-group">
          <h3>{{ skill.name }}</h3>
          <span class="skill-card__repo">{{ skill.repoName }}</span>
          <span :class="['skill-card__status', `skill-card__status--${skill.status}`]">
            {{ formatStatusLabel(skill.status) }}
          </span>
        </div>
        <div class="skill-card__targets">
          <span
            v-for="cli in cliTargets"
            :key="cli.id"
            :class="[
              'skill-card__target-pill',
              `skill-card__target-pill--${skill.installStates[cli.id]?.state || 'not-installed'}`
            ]"
          >
            {{ cli.name.slice(0, 1) }}
          </span>
        </div>
      </div>

      <p class="skill-card__description">
        {{ skill.description || '未提供描述，点击查看详情或编辑提示词入口。' }}
      </p>

      <div class="skill-card__footer">
        <div class="skill-card__tags">
          <span v-for="tag in skill.tags" :key="tag" class="skill-card__tag">{{ tag }}</span>
          <span v-if="!skill.tags.length" class="skill-card__tag skill-card__tag--muted">无标签</span>
        </div>
        <div class="skill-card__meta">
          <span>{{ skill.entry }}</span>
          <span>{{ formatDateTime(skill.updatedAt) }}</span>
        </div>
      </div>
    </div>

    <div class="skill-card__actions">
      <button class="skill-card__action" type="button" @click.stop="$emit('open-source', skill)">
        打开源目录
      </button>
    </div>
  </article>
</template>

<script setup>
import { formatDateTime, formatStatusLabel, hashColor, iconLetters } from '@/utils/formatters'

defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  skill: {
    type: Object,
    required: true
  }
})

defineEmits(['select', 'open-source'])
</script>

<style scoped lang="less">
.skill-card {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 18px;
  align-items: center;
  padding: 18px;
  border: 1px solid rgba(58, 69, 94, 0.1);
  border-radius: 24px;
  background: rgba(255, 255, 255, 0.9);
  cursor: pointer;
  transition:
    transform 0.18s ease,
    box-shadow 0.18s ease,
    border-color 0.18s ease;
}

.skill-card:hover {
  transform: translateY(-2px);
  border-color: rgba(35, 74, 133, 0.16);
  box-shadow: 0 16px 40px rgba(31, 48, 77, 0.08);
}

.skill-card__icon {
  display: grid;
  width: 54px;
  height: 54px;
  place-items: center;
  border-radius: 18px;
  color: #fff;
  font-weight: 700;
  letter-spacing: 0.08em;
  box-shadow: 0 12px 30px rgba(31, 48, 77, 0.14);
}

.skill-card__main {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 10px;
}

.skill-card__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 18px;
}

.skill-card__title-group {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.skill-card__title-group h3 {
  margin: 0;
  font-size: 1.08rem;
}

.skill-card__repo,
.skill-card__status,
.skill-card__tag,
.skill-card__target-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.skill-card__repo {
  background: rgba(36, 94, 161, 0.1);
  color: #245ea1;
}

.skill-card__status--installed {
  background: rgba(14, 148, 104, 0.12);
  color: #0e7b58;
}

.skill-card__status--not-installed {
  background: rgba(59, 130, 246, 0.12);
  color: #215a9d;
}

.skill-card__status--broken-link {
  background: rgba(220, 38, 38, 0.12);
  color: #b91c1c;
}

.skill-card__status--disabled {
  background: rgba(148, 163, 184, 0.18);
  color: #607084;
}

.skill-card__targets {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.skill-card__target-pill {
  min-width: 30px;
  padding: 5px 0;
}

.skill-card__target-pill--installed {
  background: rgba(14, 148, 104, 0.12);
  color: #0e7b58;
}

.skill-card__target-pill--broken-link {
  background: rgba(220, 38, 38, 0.12);
  color: #b91c1c;
}

.skill-card__target-pill--disabled {
  background: rgba(148, 163, 184, 0.18);
  color: #607084;
}

.skill-card__target-pill--not-installed {
  background: rgba(36, 94, 161, 0.1);
  color: #245ea1;
}

.skill-card__description {
  margin: 0;
  color: rgba(43, 57, 84, 0.72);
  line-height: 1.65;
}

.skill-card__footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
}

.skill-card__tags {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.skill-card__tag {
  background: rgba(245, 239, 230, 0.82);
  color: #7b5d33;
}

.skill-card__tag--muted {
  color: #7b889b;
}

.skill-card__meta {
  display: flex;
  gap: 12px;
  color: rgba(43, 57, 84, 0.54);
  font-size: 0.82rem;
}

.skill-card__actions {
  display: flex;
  align-items: center;
}

.skill-card__action {
  height: 36px;
  padding: 0 14px;
  border: 1px solid rgba(58, 69, 94, 0.12);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.9);
  color: #2a4366;
  cursor: pointer;
  font-weight: 600;
}

@media (max-width: 1080px) {
  .skill-card {
    grid-template-columns: 1fr;
  }

  .skill-card__footer,
  .skill-card__header {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>

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
          `skill-card__target-pill--${skill.installStates?.[cli.id]?.state || 'not-installed'}`
        ]"
        type="button"
        :title="`${cli.name}：${formatStatusLabel(skill.installStates?.[cli.id]?.state)}`"
        :disabled="skill.installStates?.[cli.id]?.state === 'disabled'"
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
import { FolderOpen } from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import { formatDateTime, formatStatusLabel } from "@/utils/formatters"

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

const emit = defineEmits(["select", "open-source", "install", "uninstall"])

function toggleCliSkill(cli) {
  const state = props.skill.installStates?.[cli.id]?.state
  const payload = {
    skillName: props.skill.name,
    targetId: cli.id
  }

  if (state === "installed") {
    emit("uninstall", payload)
    return
  }

  emit("install", payload)
}

function toFileUrl(value) {
  return encodeURI(`file:///${String(value).replace(/\\/g, "/")}`)
}
</script>

<style scoped lang="less">
.skill-card {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 14px;
  align-items: center;
  padding: 8px 14px;
  border-bottom: 1px solid var(--color-line);
  background: var(--color-panel);
  cursor: pointer;
  transition: background-color 0.18s ease;

  &:hover {
    background: var(--color-panel-soft);
  }

  &__main {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  &__title-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  &__title {
    margin: 0;
    color: var(--color-text);
    font-size: 0.9rem;
    line-height: 1.2;
  }

  &__repo,
  &__status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 0.72rem;
    line-height: 1.2;
  }

  &__repo {
    color: var(--color-text-soft);
  }

  &__status {
    padding: 2px 7px;
    border-radius: 999px;
    font-weight: 600;
  }

  &__status--installed {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  &__status--not-installed {
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
  }

  &__status--broken-link {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &__status--disabled {
    background: var(--color-primary-soft);
    color: var(--color-text-soft);
  }

  &__description {
    overflow: hidden;
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__meta {
    display: flex;
    gap: 8px;
    overflow: hidden;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    white-space: nowrap;
  }

  &__indicators {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__icon,
  &__target-pill,
  &__action {
    display: inline-grid;
    width: 28px;
    height: 28px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 50%;
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
    overflow: hidden;
  }

  &__icon-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  &__target-pill {
    background: var(--color-panel);
    cursor: pointer;
  }

  &__target-icon {
    width: 16px;
    height: 16px;
    object-fit: contain;
  }

  &__target-pill--installed {
    border-color: #cbd6e4;
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  &__target-pill--broken-link {
    border-color: #ead1d1;
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &__target-pill--disabled {
    background: var(--color-primary-soft);
    color: var(--color-text-soft);
  }

  &__target-pill--not-installed {
    border-color: transparent;
    background: var(--color-panel);
    color: var(--color-text-soft);
  }

  &__target-pill:disabled {
    cursor: not-allowed;
    opacity: 0.48;
  }

  &__action {
    background: var(--color-panel);
    cursor: pointer;
  }

  &__action-icon {
    flex: 0 0 auto;
  }
}
</style>

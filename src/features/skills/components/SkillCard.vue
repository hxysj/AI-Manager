<template>
  <article class="skill-card" @click="$emit('select', skill)">
    <label class="skill-card-check" @click.stop>
      <input
        class="skill-card-check-input"
        type="checkbox"
        :checked="selected"
        @change="$emit('toggle-select', skill)"
      />
    </label>
    <div class="skill-card-main">
      <div class="skill-card-title-row">
        <h3 class="skill-card-title">{{ skill.name }}</h3>
        <span v-if="groupName" class="skill-card-group">{{ groupName }}</span>
        <span class="skill-card-repo">{{ skill.repoName }}</span>
        <span
          :class="['skill-card-status', `skill-card-status-${skill.status}`]"
        >
          {{ formatStatusLabel(skill.status) }}
        </span>
      </div>

      <p class="skill-card-description">
        {{ skill.description || "未提供描述，点击查看详情。" }}
      </p>

      <div class="skill-card-meta">
        <span>{{ skill.entry }}</span>
        <span>{{ formatDateTime(skill.updatedAt) }}</span>
        <span v-for="tag in skill.tags.slice(0, 2)" :key="tag">{{ tag }}</span>
      </div>
    </div>

    <div class="skill-card-indicators">
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
        :class="[
          'skill-card-state-action',
          { 'skill-card-state-action-disabled': skill.disabled }
        ]"
        type="button"
        :title="skill.disabled ? '恢复 Skill' : '禁用 Skill'"
        @click.stop="
          $emit('set-enabled', {
            skillName: skill.name,
            enabled: skill.disabled
          })
        "
      >
        <Power v-if="skill.disabled" class="skill-card-action-icon" :size="15" />
        <PowerOff v-else class="skill-card-action-icon" :size="15" />
      </button>
      <button
        v-for="cli in cliTargets"
        :key="cli.id"
        :class="[
          'skill-card-target-pill',
          `skill-card-target-pill-${skill.installStates?.[cli.id]?.state || 'not-installed'}`
        ]"
        type="button"
        :title="`${cli.name}：${formatStatusLabel(skill.installStates?.[cli.id]?.state)}`"
        :disabled="skill.disabled || skill.installStates?.[cli.id]?.state === 'disabled'"
        @click.stop="toggleCliSkill(cli)"
      >
        <AiIcon
          v-if="cli.icon"
          class="skill-card-target-icon"
          :name="cli.icon"
          :alt="`${cli.name} 图标`"
        />
        <span v-else>{{ cli.name.slice(0, 1) }}</span>
      </button>
      <button
        class="skill-card-action"
        type="button"
        title="打开源目录"
        @click.stop="$emit('open-source', skill)"
      >
        <FolderOpen class="skill-card-action-icon" :size="15" />
      </button>
    </div>
  </article>
</template>

<script setup>
import { FolderOpen, Power, PowerOff } from "lucide-vue-next"
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
  },
  selected: {
    type: Boolean,
    default: false
  },
  groupName: {
    type: String,
    default: ""
  }
})

const emit = defineEmits([
  "toggle-select",
  "select",
  "open-source",
  "install",
  "set-enabled",
  "uninstall"
])

function toggleCliSkill(cli) {
  if (props.skill.disabled) {
    return
  }

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
  display: flex;
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

  .skill-card-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .skill-card-check-input {
    width: 16px;
    height: 16px;
    accent-color: var(--color-primary);
  }

  .skill-card-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  .skill-card-title-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .skill-card-title {
    margin: 0;
    color: var(--color-text);
    font-size: 0.9rem;
    line-height: 1.2;
  }

  .skill-card-repo,
  .skill-card-status,
  .skill-card-group {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 0.72rem;
    line-height: 1.2;
  }

  .skill-card-repo {
    color: var(--color-text-soft);
  }

  .skill-card-group,
  .skill-card-status {
    padding: 2px 7px;
    border-radius: 999px;
    font-weight: 600;
  }

  .skill-card-group {
    background: #edf3f8;
    color: var(--color-primary);
  }

  .skill-card-status-installed {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .skill-card-status-not-installed {
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
  }

  .skill-card-status-broken-link {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .skill-card-status-disabled {
    background: var(--color-primary-soft);
    color: var(--color-text-soft);
  }

  .skill-card-description {
    overflow: hidden;
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skill-card-meta {
    display: flex;
    gap: 8px;
    overflow: hidden;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    white-space: nowrap;
  }

  .skill-card-indicators {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .skill-card-icon,
  .skill-card-target-pill,
  .skill-card-state-action,
  .skill-card-action {
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

  .skill-card-icon-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .skill-card-target-pill {
    background: var(--color-panel);
    cursor: pointer;
  }

  .skill-card-state-action {
    border-color: #ead1d1;
    background: var(--color-danger-soft);
    color: var(--color-danger);
    cursor: pointer;
  }

  .skill-card-state-action-disabled {
    border-color: #d8e4ee;
    background: #edf3f8;
    color: var(--color-primary);
  }

  .skill-card-target-icon {
    width: 16px;
    height: 16px;
    object-fit: contain;
  }

  .skill-card-target-pill-installed {
    border-color: #cbd6e4;
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .skill-card-target-pill-broken-link {
    border-color: #ead1d1;
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .skill-card-target-pill-disabled {
    background: var(--color-primary-soft);
    color: var(--color-text-soft);
  }

  .skill-card-target-pill-not-installed {
    border-color: transparent;
    background: var(--color-panel);
    color: var(--color-text-soft);
  }

  .skill-card-target-pill:disabled {
    cursor: not-allowed;
    opacity: 0.48;
  }

  .skill-card-action {
    background: var(--color-panel);
    cursor: pointer;
  }

  .skill-card-action-icon {
    flex: 0 0 auto;
  }
}
</style>

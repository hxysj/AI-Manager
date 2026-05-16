<template>
  <div v-if="skill" class="skill-drawer">
    <div class="skill-drawer__overlay" @click="$emit('close')"></div>
    <aside class="skill-drawer__panel">
      <header class="skill-drawer__header">
        <div class="skill-drawer__hero">
          <div
            class="skill-drawer__icon"
            :style="{ background: skill.icon ? '#ffffff' : hashColor(skill.name) }"
          >
            <img
              v-if="skill.icon"
              class="skill-drawer__icon-image"
              :src="toFileUrl(skill.icon)"
              :alt="skill.name"
            />
            <span v-else>{{ iconLetters(skill.name) }}</span>
          </div>
          <div class="skill-drawer__title-wrap">
            <p>{{ skill.repoName }}</p>
            <h2>{{ skill.name }}</h2>
            <span
              :class="[
                'skill-drawer__headline-status',
                `skill-drawer__headline-status--${skill.status}`
              ]"
            >
              {{ formatStatusLabel(skill.status) }}
            </span>
          </div>
        </div>
        <button
          class="skill-drawer__close"
          type="button"
          title="关闭"
          @click="$emit('close')"
        >
          <X :size="18" />
        </button>
      </header>

      <div class="skill-drawer__tabs">
        <button
          v-for="item in tabs"
          :key="item.id"
          :class="[
            'skill-drawer__tab',
            { 'skill-drawer__tab--active': activeTab === item.id }
          ]"
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
            <span>详细内容</span>
            <pre class="skill-drawer__content-text">{{
              skill.content || '未提供详细内容。'
            }}</pre>
          </div>

          <div class="skill-drawer__block">
            <span>标签</span>
            <div class="skill-drawer__tag-list">
              <strong v-for="tag in skill.tags" :key="tag">{{ tag }}</strong>
              <strong v-if="!skill.tags.length" class="skill-drawer__muted-tag">
                暂无标签
              </strong>
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
          <article
            v-for="cli in cliTargets"
            :key="cli.id"
            class="skill-drawer__target-card"
          >
            <div class="skill-drawer__target-head">
              <div>
                <h3>{{ cli.name }}</h3>
                <p>{{ cli.skillsPath || '该 CLI 不支持 Skill 目录' }}</p>
              </div>
              <span
                :class="[
                  'skill-drawer__state-pill',
                  `skill-drawer__state-pill--${skill.installStates[cli.id]?.state}`
                ]"
              >
                {{ formatStatusLabel(skill.installStates[cli.id]?.state) }}
              </span>
            </div>
            <div class="skill-drawer__target-actions">
              <button
                v-if="skill.installStates[cli.id]?.state === 'installed'"
                class="action-button"
                type="button"
                @click="
                  $emit('uninstall', {
                    skillName: skill.name,
                    targetId: cli.id
                  })
                "
              >
                卸载
              </button>
              <button
                v-else-if="skill.installStates[cli.id]?.state === 'broken-link'"
                class="action-button action-button--alert"
                type="button"
                @click="
                  $emit('repair', {
                    skillName: skill.name,
                    targetId: cli.id
                  })
                "
              >
                修复链接
              </button>
              <button
                v-else
                class="action-button action-button--primary"
                type="button"
                :disabled="!cli.installed"
                @click="
                  $emit('install', {
                    skillName: skill.name,
                    targetId: cli.id
                  })
                "
              >
                安装
              </button>
              <button
                class="action-button"
                type="button"
                @click="$emit('open-path', cli.skillsPath)"
              >
                打开目录
              </button>
            </div>
          </article>
        </section>

        <section v-if="activeTab === 'files'" class="skill-drawer__section">
          <div class="skill-drawer__block">
            <span>源目录</span>
            <button
              class="skill-drawer__path-button"
              type="button"
              @click="$emit('open-path', skill.sourcePath)"
            >
              {{ skill.sourcePath }}
            </button>
          </div>
          <div class="skill-drawer__block">
            <span>入口文件</span>
            <button
              class="skill-drawer__path-button"
              type="button"
              @click="$emit('open-path', skill.entryPath)"
            >
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
import { X } from 'lucide-vue-next'
import {
  formatDateTime,
  formatStatusLabel,
  hashColor,
  iconLetters
} from '@/utils/formatters'

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

function toFileUrl(value) {
  return encodeURI(`file:///${String(value).replace(/\\/g, '/')}`)
}
</script>

<style scoped lang="less">
.skill-drawer {
  position: fixed;
  inset: 0;
  z-index: 30;

  &__overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.24);
  }

  &__panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    display: flex;
    width: 620px;
    flex-direction: column;
    border-left: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow: var(--shadow-panel);
  }

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 18px;
    padding: 20px 22px 16px;
    border-bottom: 1px solid var(--color-line);
    background: #fbfcfd;
  }

  &__hero {
    display: flex;
    min-width: 0;
    gap: 14px;
  }

  &__icon {
    display: grid;
    width: 56px;
    height: 56px;
    flex: 0 0 56px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 1.1rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    overflow: hidden;
  }

  &__icon-image {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  &__title-wrap {
    min-width: 0;
  }

  &__title-wrap p {
    overflow: hidden;
    margin: 0 0 5px;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }

  &__title-wrap h2 {
    overflow: hidden;
    margin: 0 0 8px;
    color: var(--color-text);
    font-size: 1.42rem;
    line-height: 1.18;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__headline-status,
  &__state-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 24px;
    padding: 4px 9px;
    border-radius: 999px;
    font-size: 0.7rem;
    font-weight: 700;
    line-height: 1.2;
  }

  &__headline-status--installed,
  &__state-pill--installed {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  &__headline-status--not-installed,
  &__state-pill--not-installed {
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
  }

  &__headline-status--broken-link,
  &__state-pill--broken-link {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &__headline-status--disabled,
  &__state-pill--disabled {
    background: var(--color-primary-soft);
    color: var(--color-text-soft);
  }

  &__close {
    display: grid;
    width: 34px;
    height: 34px;
    flex: 0 0 34px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &__close:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__tabs {
    display: flex;
    gap: 6px;
    padding: 10px 22px 0;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  &__tab {
    position: relative;
    padding: 9px 10px;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.86rem;
    font-weight: 700;
  }

  &__tab--active {
    color: var(--color-text);
  }

  &__tab--active::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    border-radius: 999px;
    background: var(--color-primary);
  }

  &__content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px 22px 22px;
    background: var(--color-page);
  }

  &__section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  &__block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 8px 22px rgba(34, 56, 83, 0.04);
  }

  &__block span {
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  &__block p {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.88rem;
    line-height: 1.55;
  }

  &__content-text {
    max-height: 360px;
    margin: 0;
    overflow: auto;
    color: var(--color-text-muted);
    font-family: inherit;
    font-size: 0.84rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__tag-list {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  &__tag-list strong {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 24px;
    padding: 4px 9px;
    border-radius: 999px;
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  &__muted-tag {
    color: var(--color-text-soft);
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  &__grid article,
  &__target-card {
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 8px 22px rgba(34, 56, 83, 0.04);
  }

  &__grid article span {
    display: block;
    margin-bottom: 6px;
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  &__grid article strong {
    color: var(--color-text);
    font-size: 0.88rem;
    line-height: 1.45;
    word-break: break-word;
  }

  &__target-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  &__target-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  &__target-head h3 {
    margin: 0 0 6px;
    color: var(--color-text);
    font-size: 0.96rem;
    line-height: 1.2;
  }

  &__target-head p {
    overflow: hidden;
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.45;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__target-head > div {
    min-width: 0;
  }

  &__target-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  &__path-button {
    overflow: hidden;
    padding: 8px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    line-height: 1.45;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__path-button:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }
}

.action-button {
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.84rem;
  font-weight: 600;

  &:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #fff;
  }

  &--primary:hover {
    border-color: #2a4f6f;
    background: #2a4f6f;
  }

  &--alert {
    border-color: var(--color-danger-soft);
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &--alert:hover {
    border-color: #ead1d1;
    background: var(--color-danger-soft);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.46;
  }
}
</style>

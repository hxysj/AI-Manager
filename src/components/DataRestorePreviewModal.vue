<template>
  <BaseModal
    title="确认恢复配置"
    :description="description"
    @close="handleClose"
  >
    <form class="restore-preview-modal" @submit.prevent="submit">
      <div class="restore-toolbar">
        <div class="restore-stats">
          <span class="restore-stat restore-stat-added">
            <span class="restore-stat-value">{{ restoreAddedItems.length }}</span>
            <span class="restore-stat-label">新增</span>
          </span>
          <span class="restore-stat restore-stat-conflict">
            <span class="restore-stat-value">{{
              restoreConflictItems.length
            }}</span>
            <span class="restore-stat-label">冲突</span>
          </span>
          <span class="restore-stat restore-stat-current">
            <span class="restore-stat-value">{{ restoreCurrentChoiceCount }}</span>
            <span class="restore-stat-label">保留当前</span>
          </span>
          <span class="restore-stat restore-stat-backup">
            <span class="restore-stat-value">{{ restoreBackupChoiceCount }}</span>
            <span class="restore-stat-label">使用备份</span>
          </span>
        </div>

        <div v-if="restoreConflictItems.length" class="restore-toolbar-actions">
          <button
            class="restore-mini-button restore-mini-button-current"
            type="button"
            :disabled="loading"
            @click="chooseAllRestoreConflicts('current')"
          >
            全部保留当前
          </button>
          <button
            class="restore-mini-button restore-mini-button-backup"
            type="button"
            :disabled="loading"
            @click="chooseAllRestoreConflicts('backup')"
          >
            全部使用备份
          </button>
        </div>
      </div>

      <p class="restore-notice">
        已有 Provider、Codex 官方账号、Skill 和宠物保留本机启用状态；备份新增的 Provider、官方账号和 Skill 默认禁用。
      </p>

      <div class="restore-body">
        <aside class="restore-nav">
          <button
            v-for="tab in restoreNavigationItems"
            :key="tab.id"
            :class="[
              'restore-nav-button',
              {
                'restore-nav-button-active': restoreSelectedRoot === tab.id
              }
            ]"
            type="button"
            @click="restoreSelectedRoot = tab.id"
          >
            <span class="restore-nav-main">
              <span class="restore-nav-name">{{ tab.label }}</span>
              <span class="restore-nav-total">{{ tab.totalCount }} 项</span>
            </span>
            <span v-if="tab.path" class="restore-nav-path">{{ tab.path }}</span>
            <span class="restore-nav-counts">
              <span v-if="tab.addedCount" class="restore-nav-count">
                新增 {{ tab.addedCount }}
              </span>
              <span v-if="tab.conflictCount" class="restore-nav-count">
                冲突 {{ tab.conflictCount }}
              </span>
            </span>
          </button>
        </aside>

        <section class="restore-panel">
          <div class="restore-list">
            <section
              v-if="restoreFilteredAddedItems.length"
              class="restore-section"
            >
              <div class="restore-section-head">
                <strong class="restore-section-title">将新增</strong>
                <span class="restore-section-count">
                  {{ restoreFilteredAddedItems.length }} 项
                </span>
              </div>
              <section
                v-for="group in restoreFilteredAddedGroups"
                :key="group.path"
                class="restore-group"
              >
                <div class="restore-group-head">
                  <strong class="restore-group-title">{{ group.path }}</strong>
                  <span class="restore-group-count">
                    {{ group.items.length }} 项
                  </span>
                </div>
                <div class="restore-tree">
                  <template v-for="row in group.rows" :key="row.key">
                    <div
                      v-if="row.kind === 'dir'"
                      class="restore-tree-folder"
                      :style="{ paddingLeft: `${row.depth * 18 + 10}px` }"
                    >
                      <strong class="restore-tree-folder-name">{{
                        row.name
                      }}</strong>
                      <span class="restore-tree-folder-count">
                        {{ row.itemCount }} 项
                      </span>
                    </div>
                    <article
                      v-else
                      class="restore-item restore-tree-item"
                      :style="{ marginLeft: `${row.depth * 18}px` }"
                    >
                      <strong class="restore-item-name"
                        >{{ row.item.type }}：{{ row.item.name }}</strong
                      >
                      <span class="restore-item-path">{{
                        row.relativePath
                      }}</span>
                    </article>
                  </template>
                </div>
              </section>
            </section>

            <section
              v-if="restoreFilteredConflictItems.length"
              class="restore-section"
            >
              <div class="restore-section-head">
                <strong class="restore-section-title">需要决策</strong>
                <span class="restore-section-count">
                  {{ restoreFilteredConflictItems.length }} 项
                </span>
              </div>
              <section
                v-for="group in restoreFilteredConflictGroups"
                :key="group.path"
                class="restore-group"
              >
                <div class="restore-group-head">
                  <strong class="restore-group-title">{{ group.path }}</strong>
                  <div class="restore-group-actions">
                    <span class="restore-group-count">
                      {{ group.items.length }} 项
                    </span>
                    <button
                      class="restore-mini-button restore-mini-button-current"
                      type="button"
                      :disabled="loading"
                      @click="chooseRestoreItems(group.items, 'current')"
                    >
                      保留当前
                    </button>
                    <button
                      class="restore-mini-button restore-mini-button-backup"
                      type="button"
                      :disabled="loading"
                      @click="chooseRestoreItems(group.items, 'backup')"
                    >
                      使用备份
                    </button>
                  </div>
                </div>
                <div class="restore-tree">
                  <template v-for="row in group.rows" :key="row.key">
                    <div
                      v-if="row.kind === 'dir'"
                      class="restore-tree-folder"
                      :style="{ paddingLeft: `${row.depth * 18 + 10}px` }"
                    >
                      <strong class="restore-tree-folder-name">{{
                        row.name
                      }}</strong>
                      <div class="restore-directory-actions">
                        <span class="restore-tree-folder-count">
                          {{ row.itemCount }} 项
                        </span>
                        <button
                          class="restore-mini-button restore-mini-button-current"
                          type="button"
                          :disabled="loading"
                          @click="chooseRestoreItems(row.items, 'current')"
                        >
                          保留当前
                        </button>
                        <button
                          class="restore-mini-button restore-mini-button-backup"
                          type="button"
                          :disabled="loading"
                          @click="chooseRestoreItems(row.items, 'backup')"
                        >
                          使用备份
                        </button>
                      </div>
                    </div>
                    <article
                      v-else
                      class="restore-conflict restore-tree-item"
                      :style="{ marginLeft: `${row.depth * 18}px` }"
                    >
                      <div class="restore-conflict-head">
                        <div class="restore-conflict-info">
                          <strong class="restore-item-name"
                            >{{ row.item.type }}：{{ row.item.name }}</strong
                          >
                          <span class="restore-item-path">{{
                            row.relativePath
                          }}</span>
                        </div>
                        <button
                          class="restore-compare-button"
                          type="button"
                          :disabled="loading"
                          @click="toggleRestoreCompare(row.item)"
                        >
                          对比
                        </button>
                      </div>

                      <div class="restore-choice-row">
                        <label
                          :class="[
                            'restore-choice',
                            {
                              'restore-choice-active':
                                restoreChoices[row.item.key] === 'current'
                            }
                          ]"
                        >
                          <input
                            v-model="restoreChoices[row.item.key]"
                            class="restore-choice-input"
                            type="radio"
                            :name="`restore-${row.item.key}`"
                            value="current"
                            :disabled="loading"
                          />
                          <span class="restore-choice-title">保留当前</span>
                          <span class="restore-choice-desc">不覆盖本机数据</span>
                        </label>
                        <label
                          :class="[
                            'restore-choice',
                            'restore-choice-backup',
                            {
                              'restore-choice-active':
                                restoreChoices[row.item.key] === 'backup'
                            }
                          ]"
                        >
                          <input
                            v-model="restoreChoices[row.item.key]"
                            class="restore-choice-input"
                            type="radio"
                            :name="`restore-${row.item.key}`"
                            value="backup"
                            :disabled="loading"
                          />
                          <span class="restore-choice-title">使用备份</span>
                          <span class="restore-choice-desc">恢复备份版本</span>
                        </label>
                      </div>
                    </article>
                  </template>
                </div>
              </section>
            </section>

            <div
              v-if="
                !restoreFilteredAddedItems.length &&
                !restoreFilteredConflictItems.length
              "
              class="restore-empty"
            >
              当前导航项没有差异。
            </div>
          </div>
        </section>
      </div>

      <div class="restore-actions">
        <button
          class="restore-action-button"
          type="button"
          :disabled="loading"
          @click="handleClose"
        >
          取消
        </button>
        <button
          class="restore-action-button restore-action-button-primary"
          type="submit"
          :disabled="loading || !restoreCanSubmit"
        >
          {{ loading ? '恢复中...' : '确认恢复' }}
        </button>
      </div>
    </form>
  </BaseModal>

  <BaseModal
    v-if="restoreCompareItem"
    title="检查恢复差异"
    :description="restoreCompareDescription"
    @close="closeRestoreCompare"
  >
    <div class="restore-preview-modal restore-preview-modal-compare">
      <div class="restore-compare-summary">
        已标记 {{ restoreCompareChangedCount }} 处不同
      </div>
      <div class="restore-compare restore-compare-dialog">
        <section class="restore-compare-panel">
          <strong class="restore-compare-title">当前内容</strong>
          <div
            ref="restoreCurrentCompareCodeRef"
            class="restore-compare-code"
            @scroll="syncRestoreCompareScroll('current')"
          >
            <div
              v-for="row in restoreCompareRows"
              :key="`current-${row.index}`"
              :class="[
                'restore-compare-line',
                `restore-compare-line-${row.currentStatus}`
              ]"
            >
              <span class="restore-compare-number">{{
                row.currentLineNumber
              }}</span>
              <span class="restore-compare-marker">{{
                row.currentMarker
              }}</span>
              <span class="restore-compare-text">{{ row.currentText }}</span>
            </div>
          </div>
        </section>
        <section class="restore-compare-panel">
          <strong class="restore-compare-title">备份内容</strong>
          <div
            ref="restoreBackupCompareCodeRef"
            class="restore-compare-code"
            @scroll="syncRestoreCompareScroll('backup')"
          >
            <div
              v-for="row in restoreCompareRows"
              :key="`backup-${row.index}`"
              :class="[
                'restore-compare-line',
                `restore-compare-line-${row.backupStatus}`
              ]"
            >
              <span class="restore-compare-number">{{
                row.backupLineNumber
              }}</span>
              <span class="restore-compare-marker">{{ row.backupMarker }}</span>
              <span class="restore-compare-text">{{ row.backupText }}</span>
            </div>
          </div>
        </section>
      </div>
      <div class="restore-actions">
        <button
          class="restore-action-button restore-action-button-primary"
          type="button"
          @click="closeRestoreCompare"
        >
          确定
        </button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, reactive, ref, watch } from 'vue'
import BaseModal from '@/components/BaseModal.vue'

const props = defineProps({
  preview: {
    type: Object,
    required: true
  },
  description: {
    type: String,
    default: ''
  },
  loading: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['close', 'submit'])

const restoreSelectedRoot = ref('all')
const restoreCompareKey = ref('')
const restoreCurrentCompareCodeRef = ref(null)
const restoreBackupCompareCodeRef = ref(null)
const restoreChoices = reactive({})

let syncingRestoreCompareScroll = false

const restoreAddedItems = computed(() => {
  return props.preview?.added || []
})

const restoreConflictItems = computed(() => {
  return props.preview?.conflicts || []
})

const restoreFilteredAddedItems = computed(() => {
  return filterRestoreItemsByRoot(restoreAddedItems.value)
})

const restoreFilteredConflictItems = computed(() => {
  return filterRestoreItemsByRoot(restoreConflictItems.value)
})

const restoreFilteredAddedGroups = computed(() => {
  return groupRestoreItems(restoreFilteredAddedItems.value)
})

const restoreFilteredConflictGroups = computed(() => {
  return groupRestoreItems(restoreFilteredConflictItems.value)
})

const restoreCurrentChoiceCount = computed(() => {
  return restoreConflictItems.value.filter(
    item => restoreChoices[item.key] === 'current'
  ).length
})

const restoreBackupChoiceCount = computed(() => {
  return restoreConflictItems.value.filter(
    item => restoreChoices[item.key] === 'backup'
  ).length
})

const restoreNavigationItems = computed(() => {
  const roots = new Map()

  // 导航按实际同步数据的最外层路径聚合，避免固定分类掩盖大量子项。
  for (const [items, countKey] of [
    [restoreAddedItems.value, 'addedCount'],
    [restoreConflictItems.value, 'conflictCount']
  ]) {
    for (const item of items) {
      const rootPath = getRestoreItemRoot(item)

      if (!roots.has(rootPath)) {
        roots.set(rootPath, {
          id: rootPath,
          label: getRestoreRootLabel(rootPath),
          path: rootPath,
          addedCount: 0,
          conflictCount: 0
        })
      }
      roots.get(rootPath)[countKey] += 1
    }
  }

  return [
    {
      id: 'all',
      label: '全部同步数据',
      path: '',
      addedCount: restoreAddedItems.value.length,
      conflictCount: restoreConflictItems.value.length,
      totalCount:
        restoreAddedItems.value.length + restoreConflictItems.value.length
    },
    ...Array.from(roots.values())
      .sort((left, right) => left.path.localeCompare(right.path))
      .map(item => ({
        ...item,
        totalCount: item.addedCount + item.conflictCount
      }))
  ]
})

const restoreCompareItem = computed(() => {
  return (
    restoreConflictItems.value.find(item => item.key === restoreCompareKey.value) ||
    null
  )
})

const restoreCompareRows = computed(() => {
  if (!restoreCompareItem.value) {
    return []
  }

  return createRestoreCompareRows(
    restoreCompareItem.value.currentContent,
    restoreCompareItem.value.backupContent
  )
})

const restoreCompareChangedCount = computed(() => {
  return restoreCompareRows.value.filter(item => item.status !== 'same').length
})

const restoreCompareDescription = computed(() => {
  if (!restoreCompareItem.value) {
    return ''
  }

  return `${restoreCompareItem.value.type}：${
    restoreCompareItem.value.name
  } · ${restoreCompareItem.value.path}`
})

const restoreCanSubmit = computed(() => {
  return Boolean(
    restoreAddedItems.value.length || restoreConflictItems.value.length
  )
})

watch(
  () => props.preview,
  () => {
    resetRestoreState()
  },
  { immediate: true }
)

function resetRestoreState() {
  restoreSelectedRoot.value = 'all'
  restoreCompareKey.value = ''

  for (const key of Object.keys(restoreChoices)) {
    delete restoreChoices[key]
  }

  for (const item of props.preview?.conflicts || []) {
    restoreChoices[item.key] = 'current'
  }
}

function handleClose() {
  if (props.loading) {
    return
  }

  emit('close')
}

function submit() {
  if (props.loading || !restoreCanSubmit.value) {
    return
  }

  emit('submit', {
    choices: { ...restoreChoices }
  })
}

function toggleRestoreCompare(item) {
  restoreCompareKey.value = item.key
}

function closeRestoreCompare() {
  restoreCompareKey.value = ''
}

function syncRestoreCompareScroll(source) {
  if (syncingRestoreCompareScroll) {
    return
  }

  const currentElement = restoreCurrentCompareCodeRef.value
  const backupElement = restoreBackupCompareCodeRef.value
  const sourceElement = source === 'current' ? currentElement : backupElement
  const targetElement = source === 'current' ? backupElement : currentElement

  if (!sourceElement || !targetElement) {
    return
  }

  const sourceScrollHeight =
    sourceElement.scrollHeight - sourceElement.clientHeight
  const targetScrollHeight =
    targetElement.scrollHeight - targetElement.clientHeight
  const sourceScrollWidth =
    sourceElement.scrollWidth - sourceElement.clientWidth
  const targetScrollWidth =
    targetElement.scrollWidth - targetElement.clientWidth

  syncingRestoreCompareScroll = true
  targetElement.scrollTop = sourceScrollHeight
    ? (sourceElement.scrollTop / sourceScrollHeight) * targetScrollHeight
    : sourceElement.scrollTop
  targetElement.scrollLeft = sourceScrollWidth
    ? (sourceElement.scrollLeft / sourceScrollWidth) * targetScrollWidth
    : sourceElement.scrollLeft
  requestAnimationFrame(() => {
    syncingRestoreCompareScroll = false
  })
}

function formatRestoreCompareContent(value) {
  if (value === undefined || value === null || value === '') {
    return '空内容'
  }

  return String(value)
}

function getRestoreItemPath(item) {
  return String(item.groupPath || item.path || '根目录').replace(/\\/g, '/')
}

function getRestoreItemRoot(item) {
  const itemPath = String(item.path || item.groupPath || '根目录').replace(
    /\\/g,
    '/'
  )
  const groupPath = String(item.groupPath || '').replace(/\\/g, '/')

  if (groupPath === 'storage/ai-manager.db') {
    return groupPath
  }

  const pathParts = itemPath.split('/').filter(Boolean)

  if (
    pathParts.length > 1 &&
    ['storage', 'skills', 'prompts', 'pets-disabled', 'codex-pets'].includes(
      pathParts[0]
    )
  ) {
    return pathParts.slice(0, 2).join('/')
  }

  return pathParts[0] || '根目录'
}

function getRestoreRootLabel(rootPath) {
  const labels = {
    'app-settings.json': '云同步设置',
    'storage/ai-manager.db': '主数据库',
    'storage/providers.json': 'Provider',
    'storage/codex-accounts.json': 'Codex 官方账号',
    'storage/skills.json': 'Skill 索引',
    'storage/rules.json': 'Prompt 索引'
  }

  if (labels[rootPath]) {
    return labels[rootPath]
  }
  if (rootPath.startsWith('skills/')) {
    return `Skill · ${rootPath.slice('skills/'.length)}`
  }
  if (rootPath.startsWith('prompts/')) {
    return `Prompt · ${rootPath.slice('prompts/'.length)}`
  }
  if (rootPath.startsWith('codex-pets/')) {
    return `启用宠物 · ${rootPath.slice('codex-pets/'.length)}`
  }
  if (rootPath.startsWith('pets-disabled/')) {
    return `禁用宠物 · ${rootPath.slice('pets-disabled/'.length)}`
  }

  return rootPath
}

function filterRestoreItemsByRoot(items) {
  if (restoreSelectedRoot.value === 'all') {
    return items
  }

  return items.filter(item => getRestoreItemRoot(item) === restoreSelectedRoot.value)
}

function groupRestoreItems(items) {
  const groups = new Map()

  for (const item of items) {
    const groupPath = item.groupPath || item.path || '根目录'

    if (!groups.has(groupPath)) {
      groups.set(groupPath, {
        path: groupPath,
        items: []
      })
    }

    groups.get(groupPath).items.push(item)
  }

  return Array.from(groups.values()).map(group => ({
    ...group,
    rows: createRestoreTreeRows(group.path, group.items)
  }))
}

function createRestoreTreeRows(groupPath, items) {
  const rows = []
  const dirKeys = new Set()
  const normalizedGroupPath = groupPath === '根目录' ? '' : groupPath
  const itemInfos = items.map(item => {
    const normalizedPath = String(item.path || '').replace(/\\/g, '/')
    const relativePath =
      normalizedGroupPath &&
      normalizedPath.startsWith(`${normalizedGroupPath}/`)
        ? normalizedPath.slice(normalizedGroupPath.length + 1)
        : normalizedPath

    return {
      item,
      relativePath,
      parts: relativePath.split('/').filter(Boolean)
    }
  })
  const dirCounts = new Map()

  for (const itemInfo of itemInfos) {
    itemInfo.parts.slice(0, -1).forEach((part, index) => {
      const key = itemInfo.parts.slice(0, index + 1).join('/')

      dirCounts.set(key, (dirCounts.get(key) || 0) + 1)
    })
  }

  for (const itemInfo of itemInfos) {
    itemInfo.parts.slice(0, -1).forEach((part, index) => {
      const key = itemInfo.parts.slice(0, index + 1).join('/')

      if (dirKeys.has(key)) {
        return
      }

      dirKeys.add(key)
      rows.push({
        key: `dir:${groupPath}:${key}`,
        kind: 'dir',
        name: part,
        depth: index,
        itemCount: dirCounts.get(key) || 0,
        items: itemInfos
          .filter(
            targetInfo =>
              targetInfo.parts.slice(0, index + 1).join('/') === key
          )
          .map(targetInfo => targetInfo.item)
      })
    })

    rows.push({
      key: itemInfo.item.key,
      kind: 'item',
      item: itemInfo.item,
      relativePath: itemInfo.relativePath,
      depth: Math.max(itemInfo.parts.length - 1, 0)
    })
  }

  return rows
}

function chooseRestoreItems(items, choice) {
  for (const item of items) {
    restoreChoices[item.key] = choice
  }
}

function chooseAllRestoreConflicts(choice) {
  chooseRestoreItems(restoreConflictItems.value, choice)
}

function createRestoreCompareRows(currentContent, backupContent) {
  const currentLines =
    formatRestoreCompareContent(currentContent).split(/\r?\n/)
  const backupLines = formatRestoreCompareContent(backupContent).split(/\r?\n/)
  const maxLength = Math.max(currentLines.length, backupLines.length)
  const rows = []

  for (let index = 0; index < maxLength; index += 1) {
    const currentText = currentLines[index]
    const backupText = backupLines[index]
    const hasCurrent = index < currentLines.length
    const hasBackup = index < backupLines.length

    if (hasCurrent && hasBackup && currentText === backupText) {
      rows.push({
        index: rows.length,
        status: 'same',
        currentStatus: 'same',
        backupStatus: 'same',
        currentLineNumber: index + 1,
        backupLineNumber: index + 1,
        currentMarker: '',
        backupMarker: '',
        currentText,
        backupText
      })
      continue
    }

    rows.push({
      index: rows.length,
      status: 'changed',
      currentStatus: hasCurrent ? 'current-only' : 'empty',
      backupStatus: hasBackup ? 'backup-only' : 'empty',
      currentLineNumber: hasCurrent ? index + 1 : '',
      backupLineNumber: hasBackup ? index + 1 : '',
      currentMarker: hasCurrent ? '当前' : '缺少',
      backupMarker: hasBackup ? '备份' : '缺少',
      currentText: hasCurrent ? currentText : '',
      backupText: hasBackup ? backupText : ''
    })
  }

  return rows
}
</script>

<style scoped lang="less">
.restore-preview-modal {
  display: flex;
  height: min(640px, calc(100vh - 180px));
  min-height: 0;
  flex-direction: column;
  gap: 12px;
}

.restore-preview-modal-compare {
  height: min(680px, calc(100vh - 180px));
}

.restore-toolbar {
  display: flex;
  flex: none;
  align-items: stretch;
  justify-content: space-between;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);

  .restore-stats {
    display: flex;
    min-width: 0;
    align-items: stretch;
    gap: 8px;

    .restore-stat {
      display: flex;
      width: 96px;
      flex-direction: column;
      justify-content: center;
      gap: 2px;
      padding: 8px 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);

      .restore-stat-value {
        color: var(--color-text);
        font-size: 1.04rem;
        font-weight: 800;
        line-height: 1.1;
      }

      .restore-stat-label {
        color: var(--color-text-muted);
        font-size: 0.72rem;
        font-weight: 700;
        line-height: 1.2;
      }
    }

    .restore-stat-added {
      border-color: var(--color-success-line);
      background: var(--color-success-soft);
    }

    .restore-stat-conflict {
      border-color: var(--color-warning-line);
      background: var(--color-warning-soft);
    }

    .restore-stat-current {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
    }

    .restore-stat-backup {
      border-color: var(--color-info-line);
      background: var(--color-primary-soft);
    }
  }

  .restore-toolbar-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }
}

.restore-notice {
  flex: none;
  margin: 0;
  padding: 10px 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-text-muted);
  font-size: 0.84rem;
  line-height: 1.6;
}

.restore-body {
  display: flex;
  flex: 1;
  min-height: 0;
  gap: 12px;

  .restore-nav {
    display: flex;
    width: 240px;
    flex: none;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding-right: 4px;

    .restore-nav-button {
      display: flex;
      min-height: 76px;
      flex-direction: column;
      align-items: stretch;
      justify-content: center;
      gap: 6px;
      padding: 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      color: var(--color-text);
      cursor: pointer;
      text-align: left;

      .restore-nav-main {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;

        .restore-nav-name {
          min-width: 0;
          overflow: hidden;
          font-size: 0.84rem;
          font-weight: 800;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .restore-nav-total {
          flex: none;
          color: var(--color-text-muted);
          font-size: 0.72rem;
          font-weight: 700;
        }
      }

      .restore-nav-counts {
        display: flex;
        flex-wrap: wrap;
        gap: 5px;

        .restore-nav-count {
          padding: 2px 6px;
          border-radius: 999px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          font-size: 0.68rem;
          font-weight: 700;
          line-height: 1.4;
        }
      }

      .restore-nav-path {
        overflow: hidden;
        color: var(--color-text-soft);
        font-family: Consolas, "Courier New", monospace;
        font-size: 0.68rem;
        line-height: 1.35;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }

    .restore-nav-button:hover {
      border-color: var(--color-line-strong);
      background: var(--color-panel);
    }

    .restore-nav-button-active {
      border-color: var(--color-info-line);
      background: var(--color-primary-soft);
      box-shadow: inset 3px 0 0 var(--color-primary);
    }
  }

  .restore-panel {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);

    .restore-list {
      display: flex;
      flex: 1;
      min-height: 0;
      flex-direction: column;
      gap: 12px;
      overflow: auto;
      padding: 12px;

      .restore-section {
        display: flex;
        flex-direction: column;
        gap: 10px;

        .restore-section-head {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 10px;

          .restore-section-title {
            color: var(--color-text);
            font-size: 0.9rem;
          }

          .restore-section-count {
            color: var(--color-text-muted);
            font-size: 0.74rem;
            font-weight: 700;
          }
        }
      }
    }
  }
}

.restore-group {
  display: flex;
  flex-direction: column;
  gap: 8px;

  .restore-group-head {
    display: flex;
    min-height: 36px;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);

    .restore-group-title {
      min-width: 0;
      overflow: hidden;
      color: var(--color-text);
      font-size: 0.82rem;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .restore-group-count {
      flex: none;
      color: var(--color-text-muted);
      font-size: 0.74rem;
      font-weight: 700;
    }

    .restore-group-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 6px;
    }
  }
}

.restore-tree {
  display: flex;
  flex-direction: column;
  gap: 7px;

  .restore-tree-folder {
    display: flex;
    min-height: 28px;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-left: 2px solid var(--color-line-strong);
    color: var(--color-text);
    font-size: 0.8rem;

    .restore-tree-folder-name {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .restore-tree-folder-count {
      flex: none;
      color: var(--color-text-muted);
      font-size: 0.72rem;
      font-weight: 700;
    }

    .restore-directory-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 6px;
    }
  }

  .restore-tree-item {
    position: relative;
  }

  .restore-tree-item::before {
    position: absolute;
    top: -8px;
    bottom: 13px;
    left: -10px;
    width: 8px;
    border-bottom: 1px solid var(--color-line-strong);
    border-left: 1px solid var(--color-line-strong);
    content: "";
  }
}

.restore-item,
.restore-conflict {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);

  .restore-item-name {
    color: var(--color-text);
    font-size: 0.86rem;
    line-height: 1.35;
  }

  .restore-item-path {
    color: var(--color-text-muted);
    font-size: 0.76rem;
    line-height: 1.45;
    word-break: break-all;
  }
}

.restore-conflict {
  background: var(--color-panel);

  .restore-conflict-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;

    .restore-conflict-info {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 2px;
    }
  }

  .restore-choice-row {
    display: flex;
    gap: 8px;

    .restore-choice {
      position: relative;
      display: flex;
      min-width: 0;
      flex: 1;
      flex-direction: column;
      gap: 2px;
      padding: 8px 10px 8px 30px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      cursor: pointer;

      .restore-choice-input {
        position: absolute;
        top: 10px;
        left: 10px;
        width: 14px;
        height: 14px;
        margin: 0;
        accent-color: var(--color-primary);
      }

      .restore-choice-title {
        color: var(--color-text);
        font-size: 0.78rem;
        font-weight: 800;
        line-height: 1.25;
      }

      .restore-choice-desc {
        overflow: hidden;
        font-size: 0.7rem;
        font-weight: 700;
        line-height: 1.25;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }

    .restore-choice-backup.restore-choice-active {
      border-color: var(--color-info-line);
      background: var(--color-primary-soft);
    }

    .restore-choice-active {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
      box-shadow: inset 0 0 0 1px rgba(63, 114, 175, 0.14);
    }
  }
}

.restore-mini-button,
.restore-compare-button {
  flex: none;
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.74rem;
  font-weight: 800;
}

.restore-mini-button:disabled,
.restore-compare-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.restore-mini-button-current {
  color: var(--color-text-muted);
}

.restore-mini-button-backup {
  border-color: var(--color-info-line);
  color: var(--color-primary);
}

.restore-empty {
  display: flex;
  min-height: 140px;
  align-items: center;
  justify-content: center;
  border: 1px dashed var(--color-line);
  border-radius: 8px;
  color: var(--color-text-muted);
  font-size: 0.9rem;
  font-weight: 700;
}

.restore-actions {
  display: flex;
  flex: none;
  justify-content: flex-end;
  gap: 10px;
  padding-top: 4px;
}

.restore-action-button {
  height: 40px;
  padding: 0 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);
  color: var(--color-primary);
  cursor: pointer;
  font-weight: 600;
}

.restore-action-button:hover {
  border-color: var(--color-line-strong);
  background: var(--color-primary-soft);
}

.restore-action-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.restore-action-button-primary {
  border-color: var(--color-primary);
  background: var(--color-primary-solid);
  color: #ffffff;
}

.restore-action-button-primary:hover {
  border-color: var(--color-primary);
  background: var(--color-primary-solid);
}

.restore-compare {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-top: 4px;
}

.restore-compare-dialog {
  flex: 1;
  min-height: 0;
  margin-top: 0;
}

.restore-compare-panel {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  gap: 6px;

  .restore-compare-title {
    color: var(--color-text);
    font-size: 0.78rem;
  }
}

.restore-compare-summary {
  grid-column: 1 / -1;
  color: var(--color-text-muted);
  font-size: 0.78rem;
  font-weight: 700;
}

.restore-compare-code {
  flex: 1;
  max-height: 260px;
  min-height: 0;
  overflow: auto;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-text);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: 0.74rem;
  line-height: 1.55;
}

.restore-compare-dialog {
  .restore-compare-code {
    max-height: none;
  }
}

.restore-compare-line {
  display: grid;
  grid-template-columns: 38px 54px minmax(0, 1fr);
  gap: 6px;
  min-height: 22px;
  padding: 2px 8px;
  white-space: pre-wrap;
  word-break: break-all;
}

.restore-compare-line-current-only {
  background: var(--color-danger-soft);
}

.restore-compare-line-backup-only {
  background: var(--color-primary-soft);
}

.restore-compare-line-empty {
  background: var(--color-panel-soft);
  color: var(--color-text-soft);
}

.restore-compare-number {
  color: var(--color-text-soft);
  text-align: right;
  user-select: none;
}

.restore-compare-marker {
  color: var(--color-text-muted);
  font-weight: 700;
  user-select: none;
}

.restore-compare-text {
  min-width: 0;
}
</style>

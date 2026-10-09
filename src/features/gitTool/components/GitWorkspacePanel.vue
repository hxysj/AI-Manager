<template>
  <section class="git-workspace">
    <aside class="git-workspace-sidebar">
      <header class="git-workspace-header">
        <span class="git-workspace-heading">未提交改动</span>
        <span class="git-workspace-count"
          >{{ status.files.length }} 个文件</span
        >
      </header>
      <div class="git-workspace-files">
        <section
          v-for="group in groups"
          :key="group.key"
          class="git-workspace-group"
        >
          <div class="git-workspace-group-head">
            <span class="git-workspace-group-title"
              >{{ group.label }} · {{ group.files.length }}</span
            >
            <button
              v-if="group.files.length"
              class="git-workspace-group-action"
              type="button"
              :disabled="busy"
              @click="
                changeFiles(
                  group.key === 'staged' ? 'unstage' : 'stage',
                  selected[group.key].length
                    ? selected[group.key]
                    : group.files.map(file => file.path)
                )
              "
            >
              {{
                group.key === 'staged'
                  ? '取消暂存'
                  : group.key === 'conflict'
                    ? '暂存已解决文件'
                    : '暂存'
              }}{{ selected[group.key].length ? '所选' : '全部' }}
            </button>
          </div>
          <div v-if="!group.files.length" class="git-workspace-group-empty">
            暂无文件
          </div>
          <div
            v-for="file in group.files"
            :key="file.path"
            class="git-workspace-file"
          >
            <div class="git-workspace-file-row">
              <input
                class="git-workspace-file-check"
                type="checkbox"
                :checked="selected[group.key].includes(file.path)"
                :disabled="busy"
                :aria-label="`选择 ${file.path}`"
                @change="toggleSelected(group.key, file.path)"
              />
              <button
                :class="[
                  'git-workspace-file-path',
                  {
                    'git-workspace-file-active':
                      selectedPath === file.path && selectedGroup === group.key
                  }
                ]"
                type="button"
                :title="
                  file.oldPath ? `${file.oldPath} → ${file.path}` : file.path
                "
                @click="selectFile(file.path, group.key)"
              >
                <span class="git-workspace-file-status">{{
                  group.key === 'staged'
                    ? file.indexStatus
                    : file.worktreeStatus
                }}</span>
                <span class="git-workspace-file-name">{{ file.path }}</span>
              </button>
              <button
                class="git-workspace-file-button"
                type="button"
                title="打开文件"
                :disabled="busy"
                @click="openFile(file.path)"
              >
                <ExternalLink :size="13" />
              </button>
              <button
                v-if="group.key !== 'staged' && group.key !== 'conflict'"
                class="git-workspace-file-button git-workspace-file-danger"
                type="button"
                :title="file.untracked ? '删除未跟踪文件' : '丢弃未暂存改动'"
                :disabled="busy"
                @click="
                  changeFiles(file.untracked ? 'delete-untracked' : 'discard', [
                    file.path
                  ])
                "
              >
                <RotateCcw :size="13" />
              </button>
            </div>
            <div
              v-if="group.key === 'conflict'"
              class="git-workspace-conflict-actions"
            >
              <button
                class="git-workspace-conflict-button"
                type="button"
                :disabled="busy"
                @click="changeFiles('resolve-ours', [file.path])"
              >
                {{
                  status.operation === 'rebase'
                    ? '保留上游版本'
                    : '采用当前分支版本'
                }}
              </button>
              <button
                class="git-workspace-conflict-button"
                type="button"
                :disabled="busy"
                @click="changeFiles('resolve-theirs', [file.path])"
              >
                {{
                  status.operation === 'rebase'
                    ? '保留重放提交版本'
                    : '采用传入版本'
                }}
              </button>
              <button
                class="git-workspace-conflict-button"
                type="button"
                :disabled="busy"
                @click="changeFiles('resolve-delete', [file.path])"
              >
                按删除解决
              </button>
            </div>
          </div>
        </section>
      </div>
      <form class="git-workspace-commit" @submit.prevent="submitCommit">
        <label class="git-workspace-commit-label" for="git-commit-message"
          >提交信息</label
        >
        <textarea
          id="git-commit-message"
          v-model="message"
          class="git-workspace-commit-input"
          placeholder="feat: 简述本次修改&#10;&#10;可补充详细说明"
          :disabled="busy"
        />
        <div class="git-workspace-commit-footer">
          <span class="git-workspace-commit-hint"
            >仅提交暂存区的 {{ stagedCount }} 个文件</span
          >
          <button
            class="git-workspace-commit-button"
            type="submit"
            :disabled="
              busy ||
              !message.trim() ||
              !stagedCount ||
              !!status.operation ||
              groups[0].files.length > 0
            "
          >
            提交
          </button>
        </div>
      </form>
    </aside>
    <section class="git-workspace-preview">
      <div v-if="diffLoading" class="git-workspace-preview-empty">
        正在读取差异…
      </div>
      <div v-else-if="diffError" class="git-workspace-preview-error">
        {{ diffError }}
      </div>
      <slot
        v-else-if="detail"
        :detail="detail"
        :title="`${selectedGroup === 'staged' ? '暂存区' : selectedGroup === 'conflict' ? '冲突文件' : '工作区'} · ${selectedPath}`"
        :select="selectCurrentGroupFile"
      />
      <div v-else class="git-workspace-preview-empty">
        {{
          status.files.length
            ? '选择文件查看差异'
            : '工作区干净，没有未提交改动'
        }}
      </div>
    </section>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { ExternalLink, RotateCcw } from 'lucide-vue-next'
import { gitToolApi, systemApi } from '@/api'
import { createMessage } from '@/utils/message'

const props = defineProps({
  repoId: { type: String, required: true },
  status: { type: Object, required: true },
  busy: { type: Boolean, default: false },
  execute: { type: Function, required: true }
})

const message = ref('')
const selectedPath = ref('')
const selectedGroup = ref('unstaged')
const selected = ref({ conflict: [], unstaged: [], untracked: [], staged: [] })
const detail = ref(null)
const diffLoading = ref(false)
const diffError = ref('')
let diffSequence = 0

// 同一个文件可以同时存在已暂存和未暂存修改，两侧分别读取差异。
const groups = computed(() => [
  {
    key: 'conflict',
    label: '冲突',
    files: props.status.files.filter(file => file.conflicted)
  },
  {
    key: 'unstaged',
    label: '未暂存',
    files: props.status.files.filter(file => file.unstaged && !file.untracked)
  },
  {
    key: 'untracked',
    label: '未跟踪',
    files: props.status.files.filter(file => file.untracked)
  },
  {
    key: 'staged',
    label: '已暂存',
    files: props.status.files.filter(file => file.staged)
  }
])
const stagedCount = computed(() => groups.value[3].files.length)

watch(
  () => [props.repoId, props.status],
  async ([repoId], previous) => {
    if (previous && repoId !== previous[0]) {
      message.value = ''
      selectedPath.value = ''
      selected.value = { conflict: [], unstaged: [], untracked: [], staged: [] }
    }
    for (const group of groups.value)
      selected.value[group.key] = selected.value[group.key].filter(path =>
        group.files.some(file => file.path === path)
      )
    const current = groups.value.find(
      group => group.key === selectedGroup.value
    )
    if (!current.files.some(file => file.path === selectedPath.value)) {
      const next = groups.value.find(group => group.files.length)
      selectedPath.value = next?.files[0]?.path || ''
      selectedGroup.value = next?.key || 'unstaged'
    }
    await loadDiff()
  },
  { immediate: true }
)

function toggleSelected(group, path) {
  const current = selected.value[group]
  selected.value[group] = current.includes(path)
    ? current.filter(value => value !== path)
    : [...current, path]
}

async function selectFile(path, group) {
  selectedPath.value = path
  selectedGroup.value = group
  await loadDiff()
}

function selectCurrentGroupFile(path) {
  return selectFile(path, selectedGroup.value)
}

async function loadDiff() {
  const sequence = ++diffSequence
  detail.value = null
  diffError.value = ''
  if (!selectedPath.value) {
    diffLoading.value = false
    return
  }
  diffLoading.value = true
  try {
    const result = await gitToolApi.getGitToolWorktreeDiff({
      repoId: props.repoId,
      filePath: selectedPath.value,
      staged: selectedGroup.value === 'staged'
    })
    if (sequence !== diffSequence) return
    detail.value = {
      ...result,
      files: groups.value
        .find(group => group.key === selectedGroup.value)
        .files.map(file => ({
          ...file,
          status:
            selectedGroup.value === 'staged'
              ? file.indexStatus
              : file.worktreeStatus
        }))
    }
  } catch (error) {
    if (sequence === diffSequence)
      diffError.value = String(error?.message || error)
  } finally {
    if (sequence === diffSequence) diffLoading.value = false
  }
}

async function changeFiles(action, filePaths) {
  await props.execute({ action, filePaths })
}

async function submitCommit() {
  if (props.busy || !message.value.trim() || !stagedCount.value) return
  const result = await props.execute({
    action: 'commit',
    message: message.value.trim()
  })
  if (result?.success) message.value = ''
}

async function openFile(path) {
  try {
    await systemApi.openPath({
      path: `${props.status.projectPath.replace(/[\\/]+$/, '')}/${path}`
    })
  } catch (error) {
    createMessage.error(String(error?.message || error))
  }
}

onBeforeUnmount(() => {
  diffSequence++
})
</script>

<style scoped lang="less">
.git-workspace {
  display: flex;
  flex: 1;
  min-height: 0;
  gap: 12px;

  .git-workspace-sidebar {
    display: flex;
    flex-direction: column;
    flex: 0 0 350px;
    min-height: 0;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    overflow: hidden;

    .git-workspace-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 13px 14px;
      border-bottom: 1px solid var(--color-line);

      .git-workspace-heading {
        color: var(--color-text);
      }
      .git-workspace-count {
        font-size: 12px;
        color: var(--color-text-muted);
      }
    }

    .git-workspace-files {
      flex: 1;
      min-height: 0;
      overflow: auto;

      .git-workspace-group {
        padding: 8px 8px 10px;
        border-bottom: 1px solid var(--color-line);

        .git-workspace-group-head {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
          padding: 2px 4px 7px;

          .git-workspace-group-title {
            font-size: 12px;
            color: var(--color-text-muted);
          }
          .git-workspace-group-action {
            border: 0;
            background: transparent;
            color: var(--color-primary);
            font-size: 12px;
            cursor: pointer;
            &:disabled {
              opacity: 0.5;
              cursor: default;
            }
          }
        }

        .git-workspace-group-empty {
          padding: 5px 6px;
          color: var(--color-text-soft);
          font-size: 12px;
        }

        .git-workspace-file {
          .git-workspace-file-row {
            display: flex;
            align-items: center;
            gap: 4px;
            min-height: 32px;

            .git-workspace-file-check {
              flex: none;
              accent-color: var(--color-primary-solid);
            }
            .git-workspace-file-path {
              display: flex;
              align-items: center;
              flex: 1;
              min-width: 0;
              gap: 6px;
              padding: 6px;
              border: 0;
              border-radius: 4px;
              background: transparent;
              color: var(--color-text);
              text-align: left;
              cursor: pointer;
              &:hover,
              &.git-workspace-file-active {
                background: var(--color-primary-soft);
              }

              .git-workspace-file-status {
                color: var(--color-primary);
                font-family: Consolas, monospace;
                font-size: 12px;
              }
              .git-workspace-file-name {
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
                font-size: 12px;
              }
            }
            .git-workspace-file-button {
              display: inline-flex;
              align-items: center;
              justify-content: center;
              flex: none;
              width: 24px;
              height: 26px;
              border: 0;
              border-radius: 4px;
              background: transparent;
              color: var(--color-text-muted);
              cursor: pointer;
              &:hover {
                background: var(--color-panel-soft);
              }
              &.git-workspace-file-danger {
                color: var(--color-danger);
              }
              &:disabled {
                opacity: 0.5;
                cursor: default;
              }
            }
          }
          .git-workspace-conflict-actions {
            display: flex;
            gap: 4px;
            padding: 0 0 7px 22px;

            .git-workspace-conflict-button {
              padding: 4px 5px;
              border: 1px solid var(--color-line);
              border-radius: 4px;
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
              font-size: 11px;
              cursor: pointer;
              &:disabled {
                opacity: 0.5;
                cursor: default;
              }
            }
          }
        }
      }
    }

    .git-workspace-commit {
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding: 12px;
      background: var(--color-panel-soft);

      .git-workspace-commit-label {
        font-size: 12px;
        color: var(--color-text-muted);
      }
      .git-workspace-commit-input {
        min-height: 90px;
        padding: 10px;
        resize: vertical;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text);
        font: inherit;
        font-size: 13px;
        &:focus {
          outline: 1px solid var(--color-primary);
        }
      }
      .git-workspace-commit-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;

        .git-workspace-commit-hint {
          font-size: 11px;
          color: var(--color-text-muted);
        }
        .git-workspace-commit-button {
          padding: 7px 16px;
          border: 0;
          border-radius: 6px;
          background: var(--color-primary-solid);
          color: #fff;
          cursor: pointer;
          &:disabled {
            opacity: 0.45;
            cursor: default;
          }
        }
      }
    }
  }

  .git-workspace-preview {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;

    .git-workspace-preview-empty,
    .git-workspace-preview-error {
      display: flex;
      align-items: center;
      justify-content: center;
      flex: 1;
      padding: 24px;
      border: 1px dashed var(--color-line);
      border-radius: 8px;
      color: var(--color-text-muted);
    }
    .git-workspace-preview-error {
      color: var(--color-danger);
    }
  }
}
</style>

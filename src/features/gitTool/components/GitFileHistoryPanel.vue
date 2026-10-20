<template>
  <section class="git-file-history">
    <GitFileHistoryTree
      :files="files"
      :selected-path="result?.filePath || filePath"
      :loading="treeLoading"
      :error="treeError"
      :busy="busy"
      @select-file="selectFile"
      @refresh="loadFiles"
    />
    <main class="git-file-history-main">
      <form class="git-file-history-form" @submit.prevent="search">
        <div class="git-file-history-path-field">
          <FileText :size="15" />
          <input
            v-model="filePath"
            class="git-file-history-path-input"
            aria-label="文件路径"
            placeholder="输入仓库相对路径，支持已删除文件，例如 src/main.js"
            :disabled="busy"
          />
        </div>
        <ElSelect
          v-model="scope"
          class="git-file-history-branch"
          aria-label="历史查询分支"
          filterable
          :fit-input-width="true"
          :persistent="false"
          :show-arrow="false"
          :suffix-icon="ChevronDown"
          :offset="6"
          :fallback-placements="['bottom-start', 'top-start']"
          popper-class="git-file-history-branch-popup"
          no-match-text="没有匹配的分支"
          no-data-text="暂无分支"
          :disabled="busy"
        >
          <ElOption value="HEAD" label="当前工作树" />
          <ElOption value=":all" label="所有分支" />
          <ElOption
            v-for="branch in branches"
            :key="branch.name"
            :value="branch.name"
            :label="branch.name"
            :title="branch.name"
          />
        </ElSelect>
        <button
          class="git-file-history-button git-file-history-search"
          type="submit"
          :disabled="busy || !filePath.trim()"
          :title="
            scope === ':all'
              ? '查找所有本地及已获取的远程分支中的文件记录\n命令：git log --all --full-history -- <文件>'
              : '查询文件历史并按需追踪重命名\n命令：git log --follow <分支> -- <文件>'
          "
        >
          <Search :size="14" /><span>{{
            loading ? "重新查询" : "查询历史"
          }}</span>
        </button>
      </form>

      <div class="git-file-history-options">
        <label class="git-file-history-follow">
          <input
            v-model="followRenames"
            class="git-file-history-checkbox"
            type="checkbox"
            :disabled="busy || scope === ':all'"
          />
          <span>追踪重命名前的历史</span>
        </label>
        <span class="git-file-history-note">{{
          scope === ":all"
            ? "所有分支按输入路径查询；选择具体分支可追踪重命名。"
            : "包括文件的新增、修改、重命名和删除记录。"
        }}</span>
      </div>

      <div v-if="error" class="git-file-history-error" role="alert">
        {{ error }}
      </div>
      <div v-if="loading" class="git-file-history-empty" role="status">
        <RefreshCw :size="22" /><span>正在读取文件提交记录…</span>
      </div>
      <template v-else-if="result">
        <div class="git-file-history-summary">
          <span class="git-file-history-result-path" :title="result.filePath">{{
            result.filePath
          }}</span>
          <span>{{ result.branchName }}</span>
        </div>
        <div v-if="result.commits.length" class="git-file-history-list">
          <div class="git-file-history-columns">
            <span class="git-file-history-message">提交信息</span>
            <span class="git-file-history-date">日期</span>
            <span class="git-file-history-author">作者</span>
            <span class="git-file-history-hash">提交</span>
          </div>
          <div class="git-file-history-rows">
            <button
              v-for="commit in result.commits"
              :key="commit.hash"
              class="git-file-history-record"
              type="button"
              :title="`查看本次提交的文件改动\n命令：git show ${commit.hash} -- ${commit.filePath}`"
              @click="openCommit(commit)"
            >
              <span class="git-file-history-message">
                <span
                  class="git-file-history-subject"
                  :title="`${commit.subject}\n当时的文件：${commit.filePath}`"
                  >{{ commit.subject }}</span
                >
                <span
                  v-if="formatCheckStatus(commit.checkStatus)"
                  :class="[
                    'git-file-history-check',
                    `git-file-history-check-${commit.checkStatus}`
                  ]"
                  :title="formatCheckReason(commit)"
                  >{{ formatCheckStatus(commit.checkStatus) }}</span
                >
              </span>
              <span class="git-file-history-date">{{
                formatGitGraphDate(commit.date)
              }}</span>
              <span class="git-file-history-author" :title="commit.author">{{
                commit.author
              }}</span>
              <span class="git-file-history-hash">{{ commit.shortHash }}</span>
            </button>
          </div>
        </div>
        <div v-else class="git-file-history-empty">
          <FileText :size="28" /><span>没有找到该文件的提交记录</span>
          <span class="git-file-history-note"
            >请核对路径，或切换分支后重新查询。</span
          >
        </div>
        <div
          v-if="result.commits.length || offset"
          class="git-file-history-pagination"
        >
          <span
            >{{
              result.commits.length
                ? `第 ${offset + 1}–${offset + result.commits.length} 条`
                : "本页没有记录"
            }}
            · 每页 {{ pageSize }} 条</span
          >
          <div class="git-file-history-page-buttons">
            <button
              class="git-file-history-button"
              type="button"
              :disabled="busy || !offset"
              @click="loadPage(offset - pageSize)"
            >
              <ChevronLeft :size="14" />上一页
            </button>
            <button
              class="git-file-history-button"
              type="button"
              :disabled="busy || !result.hasMore"
              @click="loadPage(offset + pageSize)"
            >
              下一页<ChevronRight :size="14" />
            </button>
          </div>
        </div>
      </template>
      <div v-else-if="!error" class="git-file-history-empty">
        <History :size="32" /><span>选择一个文件，查看它的修改历史</span>
        <span class="git-file-history-note"
          >历史直接从 Git 读取，按页加载。</span
        >
      </div>
    </main>
  </section>
</template>

<script setup>
import { onBeforeUnmount, onMounted, ref } from "vue"
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  FileText,
  History,
  RefreshCw,
  Search
} from "lucide-vue-next"
import { ElOption, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"
import { gitToolApi } from "@/api"
import {
  formatGitGraphDate,
  formatCheckStatus,
  formatCheckReason
} from "@/features/gitTool/constant"
import GitFileHistoryTree from "@/features/gitTool/components/GitFileHistoryTree.vue"

const props = defineProps({
  repoId: { type: String, required: true },
  projectPath: { type: String, required: true },
  worktreePath: { type: String, default: "" },
  branchName: { type: String, default: "" },
  branches: { type: Array, default: () => [] },
  busy: { type: Boolean, default: false }
})
const emit = defineEmits(["select-commit"])
const filePath = ref("")
const scope = ref(props.branchName || "HEAD")
const followRenames = ref(true)
const loading = ref(false)
const error = ref("")
const result = ref(null)
const offset = ref(0)
const files = ref([])
const treeLoading = ref(false)
const treeError = ref("")
const pageSize = 60
let query = null
let sequence = 0
let treeSequence = 0

// 翻页沿用已提交的查询和分支快照，编辑输入框不会改变已有结果的上下文。
async function search() {
  if (props.busy || !filePath.value.trim()) return
  result.value = null
  offset.value = 0
  query = {
    repoId: props.repoId,
    worktreePath: props.worktreePath,
    filePath: filePath.value,
    branchName: scope.value === ":all" ? "" : scope.value,
    allBranches: scope.value === ":all",
    followRenames: scope.value !== ":all" && followRenames.value
  }
  await loadPage(0)
}

async function loadPage(nextOffset) {
  if (!query || props.busy) return
  const current = ++sequence
  const request = { ...query, offset: Math.max(0, nextOffset), limit: pageSize }
  loading.value = true
  error.value = ""
  try {
    const response = await gitToolApi.listGitToolFileHistory(request)
    if (current !== sequence) return
    result.value = response
    offset.value = response.offset
    query = {
      ...query,
      filePath: response.filePath,
      revision: response.revision
    }
  } catch (failure) {
    if (current === sequence) error.value = failure?.message || String(failure)
  } finally {
    if (current === sequence) loading.value = false
  }
}

async function loadFiles() {
  const current = ++treeSequence
  treeLoading.value = true
  treeError.value = ""
  try {
    const response = await gitToolApi.listGitToolWorktreeFiles({
      repoId: props.repoId,
      worktreePath: props.worktreePath
    })
    if (current === treeSequence) files.value = response
  } catch (failure) {
    if (current === treeSequence)
      treeError.value = failure?.message || String(failure)
  } finally {
    if (current === treeSequence) treeLoading.value = false
  }
}

async function selectFile(path) {
  filePath.value = path
  await search()
}

function openCommit(commit) {
  emit("select-commit", {
    ...commit,
    historyBranchName: result.value.branchName
  })
}

// 切换项目或工作树后，旧请求不能回写到已卸载的面板。
onBeforeUnmount(() => {
  treeSequence++
  sequence++
})
onMounted(loadFiles)
</script>

<style scoped lang="less">
.git-file-history {
  display: flex;
  min-height: 0;
  flex: 1;
  gap: 12px;

  .git-file-history-main {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel);

    .git-file-history-form {
      display: flex;
      align-items: center;
      gap: 8px;

      .git-file-history-path-field {
        display: flex;
        min-width: 180px;
        flex: 1;
        align-items: center;
        gap: 8px;
        padding: 0 10px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        color: var(--color-text-muted);

        &:focus-within {
          border-color: var(--color-primary);
        }

        .git-file-history-path-input {
          width: 100%;
          min-width: 0;
          height: 34px;
          border: 0;
          outline: none;
          background: transparent;
          color: var(--color-text);
          font: inherit;
          box-shadow: none;
        }
      }

      .git-file-history-branch {
        width: 180px;
        flex: none;
        --el-fill-color-blank: var(--color-panel);
        --el-fill-color-light: var(--color-panel-soft);
        --el-border-color: var(--color-line);
        --el-border-color-hover: var(--color-line-strong);
        --el-color-primary: var(--color-primary);
        --el-text-color-regular: var(--color-text);
        --el-text-color-placeholder: var(--color-text-soft);

        :deep(.el-select__wrapper) {
          min-height: 34px;
          padding: 5px 10px;
          border-radius: 6px;
        }

        // 聚焦边框交给外层选择框，内部搜索输入不重复绘制边框。
        :deep(.el-select__input) {
          border: 0;
          outline: none;
          box-shadow: none;
          background: transparent;
        }
      }
    }

    .git-file-history-button {
      display: inline-flex;
      height: 34px;
      flex: none;
      align-items: center;
      justify-content: center;
      gap: 6px;
      padding: 0 12px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: transparent;
      color: var(--color-text);
      font: inherit;
      cursor: pointer;

      &:hover {
        background: var(--color-primary-soft);
      }
      &:disabled {
        opacity: 0.45;
        cursor: default;
      }
    }

    .git-file-history-search {
      background: var(--color-primary-soft);
      color: var(--color-primary);
    }

    .git-file-history-options {
      display: flex;
      align-items: center;
      gap: 16px;
      margin: 12px 0 18px;

      .git-file-history-follow {
        display: flex;
        align-items: center;
        gap: 6px;
      }
    }

    .git-file-history-note {
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
    }
    .git-file-history-error {
      margin-bottom: 12px;
      color: var(--color-danger);
    }

    .git-file-history-empty {
      display: flex;
      min-height: 180px;
      flex: 1;
      align-items: center;
      justify-content: center;
      flex-direction: column;
      gap: 12px;
      color: var(--color-text-muted);
    }

    .git-file-history-summary {
      display: flex;
      align-items: center;
      gap: 14px;
      padding: 12px 0;
      border-top: 1px solid var(--color-line);
      color: var(--color-text-muted);

      .git-file-history-result-path {
        overflow: hidden;
        color: var(--color-text);
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }

    .git-file-history-list {
      display: flex;
      min-height: 0;
      flex: 1;
      flex-direction: column;

      .git-file-history-columns,
      .git-file-history-record {
        display: flex;
        align-items: center;
        gap: 14px;
        padding: 7px 10px;
        text-align: left;
      }

      .git-file-history-columns {
        flex: none;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
      }
      .git-file-history-message {
        display: flex;
        min-width: 0;
        flex: 1;
        align-items: center;
        gap: 5px;
      }
      .git-file-history-date {
        width: 150px;
        flex: none;
        font-size: var(--font-size-sm);
      }
      .git-file-history-author {
        overflow: hidden;
        width: 100px;
        flex: none;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .git-file-history-hash {
        width: 75px;
        flex: none;
        font-family: var(--font-family-mono, monospace);
        font-size: var(--font-size-sm);
      }

      .git-file-history-rows {
        min-height: 0;
        overflow-y: auto;

        .git-file-history-record {
          width: 100%;
          border: 0;
          border-bottom: 1px solid var(--color-line);
          background: transparent;
          color: var(--color-text);
          font: inherit;
          cursor: pointer;

          &:hover {
            background: var(--color-primary-soft);
          }
          .git-file-history-subject {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
          .git-file-history-check {
            flex: none;
            padding: 2px 6px;
            border-radius: 999px;
            font-size: var(--font-size-sm);
            &.git-file-history-check-exists-hash,
            &.git-file-history-check-exists-content {
              background: var(--color-success-soft);
              color: var(--color-success);
            }
            &.git-file-history-check-exists-subject {
              background: var(--color-warning-soft);
              color: var(--color-warning);
            }
            &.git-file-history-check-missing {
              background: var(--color-danger-soft);
              color: var(--color-danger);
            }
          }
        }
      }
    }

    .git-file-history-pagination {
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      padding-top: 12px;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);

      .git-file-history-page-buttons {
        display: flex;
        gap: 8px;
      }
    }
  }
}
</style>

<!-- 弹层挂载到页面根部，使用独立类名避免继承项目、工作树的多行菜单样式。 -->
<style lang="less">
.git-file-history-branch-popup.el-popper {
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  box-shadow: var(--shadow-panel);
  --el-color-primary: var(--color-primary);
  --el-text-color-regular: var(--color-text);
  --el-text-color-placeholder: var(--color-text-soft);
  --el-fill-color-light: var(--color-panel-soft);

  .el-select-dropdown {
    .el-select-dropdown__wrap {
      max-height: 256px;

      .el-select-dropdown__list {
        padding: 4px;

        .el-select-dropdown__item {
          height: 32px;
          padding: 0 10px;
          border-radius: 4px;
          font: inherit;
          line-height: 32px;

          &.is-selected {
            background: var(--color-primary-soft);
          }
        }
      }
    }

    .el-select-dropdown__empty {
      padding: 16px 10px;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
    }
  }
}
</style>

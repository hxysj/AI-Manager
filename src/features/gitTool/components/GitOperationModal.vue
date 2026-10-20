<template>
  <BaseModal class="git-operation-modal" @close="close">
    <template #header>
      <div class="git-operation-header">
        <div class="git-operation-title">{{ definition.label }}</div>
        <p class="git-operation-description">{{ definition.description }}</p>
      </div>
      <button
        class="git-operation-close"
        type="button"
        aria-label="关闭"
        :disabled="busy"
        @click="close"
      >
        <X :size="17" />
      </button>
    </template>
    <form class="git-operation-form" @submit.prevent="submit">
      <div class="git-operation-field">
        <span class="git-operation-field-label">实际操作工作树</span>
        <code class="git-operation-control">{{ status.projectPath }}</code>
      </div>
      <div class="git-operation-current">
        <span class="git-operation-current-label">实际操作分支</span>
        <span class="git-operation-current-branch">{{
          status.branch || "未关联分支"
        }}</span>
        <code class="git-operation-current-hash">{{
          status.headHash.slice(0, 8) || "尚无提交"
        }}</code>
        <span class="git-operation-current-changes"
          >{{ status.files.length }} 个未提交文件</span
        >
      </div>

      <label
        v-if="action === 'switch-branch' || action === 'rename-branch'"
        class="git-operation-field"
      >
        <span class="git-operation-field-label">{{
          action === "switch-branch" ? "切换到本地分支" : "本地分支"
        }}</span>
        <select
          v-model="form.branchName"
          class="git-operation-control"
          :disabled="busy"
          required
        >
          <option
            v-for="branch in branches"
            :key="branch.name"
            :value="branch.name"
            :disabled="
              action === 'switch-branch' &&
              !!branch.worktreePath &&
              !branch.isCurrent
            "
          >
            {{ branch.name
            }}{{
              branch.isCurrent
                ? "（当前）"
                : branch.worktreePath
                  ? `（已在 ${branch.worktreePath} 检出）`
                  : ""
            }}
          </option>
        </select>
      </label>
      <label
        v-if="action === 'create-branch' || action === 'rename-branch'"
        class="git-operation-field"
      >
        <span class="git-operation-field-label">{{
          action === "create-branch" ? "新分支名称" : "修改为"
        }}</span>
        <input
          v-model.trim="form.newBranchName"
          class="git-operation-control"
          :disabled="busy"
          placeholder="feature/my-change"
          required
        />
      </label>
      <label v-if="revisionRequired" class="git-operation-field">
        <span class="git-operation-field-label">{{
          action === "create-branch"
            ? "起始分支或提交"
            : action === "checkout-commit"
              ? "目标提交或标签"
              : "目标分支或提交"
        }}</span>
        <input
          v-model.trim="form.revision"
          class="git-operation-control"
          list="git-operation-revisions"
          :disabled="busy"
          placeholder="分支名称或提交哈希"
          required
        />
        <datalist id="git-operation-revisions">
          <option
            v-for="branch in branches"
            :key="branch.name"
            :value="branch.name"
          />
        </datalist>
      </label>
      <label v-if="action === 'create-branch'" class="git-operation-check"
        ><input
          v-model="form.switchAfterCreate"
          type="checkbox"
          :disabled="busy"
        />创建后切换到新分支</label
      >

      <div v-if="revisionRequired" class="git-operation-preview">
        <span v-if="previewLoading" class="git-operation-preview-hint"
          >正在核对目标提交…</span
        >
        <span v-else-if="previewError" class="git-operation-preview-error">{{
          previewError
        }}</span>
        <template v-else-if="preview">
          <span class="git-operation-preview-subject">{{
            preview.subject
          }}</span>
          <code class="git-operation-preview-hash">{{ preview.hash }}</code>
          <span class="git-operation-preview-hint"
            >{{ preview.author }} · {{ formatDateTime(preview.date) }}</span
          >
          <template v-if="action === 'reset'">
            <span class="git-operation-preview-hint"
              >目标与当前提交之间有
              {{ preview.changedFiles.length }} 个不同文件</span
            >
            <div
              v-if="preview.changedFiles.length"
              class="git-operation-preview-files"
            >
              <span
                v-for="path in preview.changedFiles"
                :key="path"
                class="git-operation-preview-file"
                >{{ path }}</span
              >
            </div>
          </template>
        </template>
      </div>
      <label
        v-if="
          preview?.parents.length > 1 &&
          ['cherry-pick', 'revert'].includes(action)
        "
        class="git-operation-field"
      >
        <span class="git-operation-field-label">合并提交的主线父提交</span>
        <select
          v-model.number="form.mainline"
          class="git-operation-control"
          :disabled="busy"
        >
          <option
            v-for="(parent, index) in preview.parents"
            :key="parent"
            :value="index + 1"
          >
            父提交 {{ index + 1 }} · {{ parent.slice(0, 8) }}
          </option>
        </select>
      </label>

      <label v-if="action === 'stash-save'" class="git-operation-field">
        <span class="git-operation-field-label">贮藏说明（可选）</span>
        <input
          v-model="form.message"
          class="git-operation-control"
          :disabled="busy"
          placeholder="描述当前保存的改动"
        />
      </label>
      <label v-if="action === 'stash-save'" class="git-operation-check"
        ><input
          v-model="form.includeUntracked"
          type="checkbox"
          :disabled="busy"
        />同时保存未跟踪文件</label
      >
      <template v-if="action === 'stash-apply' || action === 'stash-pop'">
        <div class="git-operation-stash">
          <span class="git-operation-stash-subject">{{ initial.subject }}</span
          ><code class="git-operation-stash-hash">{{ initial.stashHash }}</code>
        </div>
        <label class="git-operation-check"
          ><input
            v-model="form.restoreIndex"
            type="checkbox"
            :disabled="busy"
          />恢复保存时的暂存状态</label
        >
      </template>

      <template v-if="['fetch', 'pull', 'push'].includes(action)">
        <label class="git-operation-field">
          <span class="git-operation-field-label">远程仓库</span>
          <select
            v-model="form.remote"
            class="git-operation-control"
            :disabled="busy"
            required
          >
            <option
              v-for="remote in status.remotes"
              :key="remote.name"
              :value="remote.name"
            >
              {{ remote.name }} ·
              {{ action === "push" ? remote.pushUrl : remote.fetchUrl }}
            </option>
          </select>
        </label>
        <label v-if="action !== 'fetch'" class="git-operation-field">
          <span class="git-operation-field-label">远程分支</span>
          <input
            v-model.trim="form.remoteBranch"
            class="git-operation-control"
            :disabled="busy"
            placeholder="main"
            required
          />
        </label>
        <label v-if="action === 'pull'" class="git-operation-field">
          <span class="git-operation-field-label">整合策略</span>
          <select
            v-model="form.pullMode"
            class="git-operation-control"
            :disabled="busy"
          >
            <option
              v-for="mode in [
                { value: 'ff-only', label: '仅快进：发生分叉时停止' },
                { value: 'merge', label: '合并：保留分支历史' },
                { value: 'rebase', label: '变基：重放本地提交' }
              ]"
              :key="mode.value"
              :value="mode.value"
            >
              {{ mode.label }}
            </option>
          </select>
        </label>
        <span v-if="!status.remotes.length" class="git-operation-error"
          >当前仓库尚未配置远程地址</span
        >
      </template>

      <template v-if="action === 'reset'">
        <label class="git-operation-field">
          <span class="git-operation-field-label">重置模式</span>
          <select
            v-model="form.resetMode"
            class="git-operation-control"
            :disabled="busy"
          >
            <option
              v-for="mode in [
                { value: 'soft', label: '软重置：保留工作区和暂存区' },
                { value: 'mixed', label: '混合重置：保留工作区，重置暂存区' },
                { value: 'hard', label: '硬重置：重置工作区和暂存区' }
              ]"
              :key="mode.value"
              :value="mode.value"
            >
              {{ mode.label }}
            </option>
          </select>
        </label>
        <div v-if="form.resetMode === 'hard'" class="git-operation-danger">
          <span class="git-operation-danger-text"
            >将丢弃已跟踪文件的未提交修改，并可能删除阻碍恢复目标文件的未跟踪文件。当前有
            {{ status.files.filter((file) => !file.untracked).length }}
            个已跟踪改动文件。</span
          >
          <label class="git-operation-danger-check"
            ><input
              v-model="form.confirmedDiscard"
              type="checkbox"
              :disabled="busy"
            />我已核对目标提交和影响范围，确认执行 硬重置</label
          >
        </div>
      </template>
      <div class="git-operation-footer">
        <span class="git-operation-footer-note">{{
          busy ? "正在执行 Git 操作…" : "操作会在上方标明的工作树中执行"
        }}</span>
        <button
          class="git-operation-cancel"
          type="button"
          :disabled="busy"
          @click="close"
        >
          取消
        </button>
        <button
          :class="[
            'git-operation-submit',
            {
              'git-operation-submit-danger':
                action === 'reset' && form.resetMode === 'hard'
            }
          ]"
          type="submit"
          :title="`命令：${definition.command}`"
          :disabled="!canSubmit"
        >
          {{ busy ? "执行中…" : `确认${definition.label}` }}
        </button>
      </div>
    </form>
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, reactive, ref, watch } from "vue"
import { X } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { gitToolApi } from "@/api"
import { formatDateTime } from "@/utils/formatters"
import { gitOperationDefinitions } from "@/features/gitTool/constant"

const props = defineProps({
  action: { type: String, required: true },
  repoId: { type: String, required: true },
  status: { type: Object, required: true },
  branches: { type: Array, required: true },
  initial: { type: Object, default: () => ({}) },
  busy: { type: Boolean, default: false },
  execute: { type: Function, required: true }
})
const emit = defineEmits(["close"])
const definition = computed(() => gitOperationDefinitions[props.action])
const revisionRequired = computed(() =>
  [
    "create-branch",
    "checkout-commit",
    "merge",
    "cherry-pick",
    "revert",
    "reset"
  ].includes(props.action)
)
const remote =
  props.status.remotes.find((item) =>
    props.status.upstream.startsWith(`${item.name}/`)
  )?.name ||
  props.status.remotes.find((item) => item.name === "origin")?.name ||
  props.status.remotes[0]?.name ||
  ""
const form = reactive({
  branchName: props.initial.branchName || props.status.branch,
  newBranchName: "",
  revision:
    props.initial.revision ||
    (props.action === "merge"
      ? props.branches.find((branch) => !branch.isCurrent)?.name || ""
      : "HEAD"),
  switchAfterCreate: true,
  message: "",
  includeUntracked: true,
  restoreIndex: true,
  remote,
  remoteBranch: props.status.upstream.startsWith(`${remote}/`)
    ? props.status.upstream.slice(remote.length + 1)
    : props.status.branch,
  pullMode: "ff-only",
  resetMode: "soft",
  confirmedDiscard: false,
  mainline: 1
})
const preview = ref(null)
const previewError = ref("")
const previewLoading = ref(false)
let previewTimer = 0
let previewSequence = 0

const canSubmit = computed(() => {
  if (
    props.busy ||
    (revisionRequired.value &&
      (!preview.value || previewLoading.value || previewError.value))
  )
    return false
  if (
    ["create-branch", "rename-branch"].includes(props.action) &&
    !form.newBranchName
  )
    return false
  if (
    ["switch-branch", "rename-branch"].includes(props.action) &&
    !form.branchName
  )
    return false
  if (
    props.action === "switch-branch" &&
    props.branches.some(
      (branch) =>
        branch.name === form.branchName &&
        branch.worktreePath &&
        !branch.isCurrent
    )
  )
    return false
  if (["fetch", "pull", "push"].includes(props.action) && !form.remote)
    return false
  if (["pull", "push"].includes(props.action) && !form.remoteBranch)
    return false
  return (
    props.action !== "reset" ||
    form.resetMode !== "hard" ||
    form.confirmedDiscard
  )
})

// 输入稍作停顿后核对目标，旧请求结果不能覆盖当前输入的预览。
watch(
  () => form.revision,
  (revision) => {
    form.confirmedDiscard = false
    window.clearTimeout(previewTimer)
    const sequence = ++previewSequence
    preview.value = null
    previewError.value = ""
    if (!revisionRequired.value || !revision) {
      previewLoading.value = false
      return
    }
    previewLoading.value = true
    previewTimer = window.setTimeout(async () => {
      try {
        const result = await gitToolApi.previewGitToolRevision({
          repoId: props.repoId,
          worktreePath: props.status.projectPath,
          revision
        })
        if (sequence === previewSequence) preview.value = result
      } catch (error) {
        if (sequence === previewSequence)
          previewError.value = String(error?.message || error)
      } finally {
        if (sequence === previewSequence) previewLoading.value = false
      }
    }, 250)
  },
  { immediate: true }
)

// 目标或模式改变后重新核对，避免沿用上一轮 硬重置 确认。
watch(
  () => form.resetMode,
  () => {
    form.confirmedDiscard = false
  }
)

async function submit() {
  if (!canSubmit.value) return
  const result = await props.execute({
    ...form,
    action: props.action,
    branchName:
      props.action === "create-branch" ? form.newBranchName : form.branchName,
    revision: preview.value?.hash || form.revision,
    stashHash: props.initial.stashHash || "",
    expectedHead: props.status.headHash,
    worktreePath: props.status.projectPath,
    expectedWorktreePath: props.status.projectPath,
    expectedBranch: props.status.branch
  })
  if (
    result?.success ||
    result?.status.operation ||
    result?.status.files.some((file) => file.conflicted)
  )
    emit("close")
}

function close() {
  if (!props.busy) emit("close")
}

onBeforeUnmount(() => {
  window.clearTimeout(previewTimer)
  previewSequence++
})
</script>

<style scoped lang="less">
.git-operation-modal {
  :deep(.base-modal__panel) {
    width: 650px;
  }
  :deep(.base-modal__content) {
    overflow: auto;
  }

  .git-operation-header {
    .git-operation-title {
      font-size: 18px;
      color: var(--color-text);
    }
    .git-operation-description {
      margin: 6px 0 0;
      color: var(--color-text-muted);
      font-size: 13px;
    }
  }
  .git-operation-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .git-operation-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-top: 12px;

    .git-operation-current {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 8px;
      padding: 12px;
      border: 1px solid var(--color-info-line);
      border-radius: 6px;
      background: var(--color-primary-soft);

      .git-operation-current-label,
      .git-operation-current-changes {
        color: var(--color-text-muted);
        font-size: 12px;
      }
      .git-operation-current-branch {
        color: var(--color-primary);
        font-size: 13px;
        word-break: break-all;
      }
      .git-operation-current-hash {
        color: var(--color-text-muted);
        font-size: 12px;
      }
    }
    .git-operation-field {
      display: flex;
      flex-direction: column;
      gap: 7px;

      .git-operation-field-label {
        font-size: 13px;
        color: var(--color-text-muted);
      }
      .git-operation-control {
        width: 100%;
        min-height: 36px;
        padding: 8px 10px;
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
    }
    .git-operation-check {
      display: flex;
      align-items: center;
      gap: 8px;
      font-size: 13px;
      color: var(--color-text-muted);
    }
    .git-operation-preview,
    .git-operation-stash {
      display: flex;
      flex-direction: column;
      gap: 7px;
      padding: 12px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel-soft);
    }

    .git-operation-preview {
      .git-operation-preview-subject {
        font-size: 13px;
        color: var(--color-text);
        word-break: break-all;
      }
      .git-operation-preview-hash {
        font-size: 12px;
        color: var(--color-primary);
        word-break: break-all;
      }
      .git-operation-preview-hint {
        font-size: 12px;
        color: var(--color-text-muted);
      }
      .git-operation-preview-error {
        color: var(--color-danger);
        font-size: 13px;
      }
      .git-operation-preview-files {
        display: flex;
        flex-direction: column;
        gap: 4px;
        max-height: 110px;
        overflow: auto;

        .git-operation-preview-file {
          font-size: 12px;
          color: var(--color-text-muted);
          word-break: break-all;
        }
      }
    }
    .git-operation-stash {
      .git-operation-stash-subject {
        font-size: 13px;
        color: var(--color-text);
        word-break: break-all;
      }
      .git-operation-stash-hash {
        font-size: 12px;
        color: var(--color-primary);
        word-break: break-all;
      }
    }
    .git-operation-error {
      color: var(--color-danger);
      font-size: 13px;
    }
    .git-operation-danger {
      display: flex;
      flex-direction: column;
      gap: 12px;
      padding: 12px;
      border: 1px solid var(--color-danger-line);
      border-radius: 6px;
      background: var(--color-danger-soft);

      .git-operation-danger-text {
        color: var(--color-danger);
        font-size: 13px;
        line-height: 1.6;
      }
      .git-operation-danger-check {
        display: flex;
        align-items: center;
        gap: 8px;
        color: var(--color-danger);
        font-size: 12px;
      }
    }
    .git-operation-footer {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 8px;
      padding-top: 12px;
      border-top: 1px solid var(--color-line);

      .git-operation-footer-note {
        flex: 1;
        color: var(--color-text-muted);
        font-size: 12px;
      }
      .git-operation-cancel,
      .git-operation-submit {
        padding: 8px 14px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text);
        cursor: pointer;
        &:disabled {
          opacity: 0.45;
          cursor: default;
        }
      }
      .git-operation-submit {
        border-color: var(--color-primary-solid);
        background: var(--color-primary-solid);
        color: #fff;
        &.git-operation-submit-danger {
          border-color: var(--color-danger);
          background: var(--color-danger);
        }
      }
    }
  }
}
</style>

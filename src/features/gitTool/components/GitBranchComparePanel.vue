<template>
  <section class="git-branch-compare">
    <header class="git-branch-compare-head">
      <div class="git-branch-compare-title-wrap">
        <span data-emphasis class="git-branch-compare-title">分支对比</span>
        <span class="git-branch-compare-subtitle"
          >查看两个分支之间的文件和行级差异</span
        >
      </div>
      <button
        class="git-branch-compare-close"
        type="button"
        :disabled="loading"
        @click="emit('close')"
      >
        <X :size="14" />
        <span>返回提交记录</span>
      </button>
    </header>

    <div class="git-branch-compare-controls">
      <label class="git-branch-compare-select-wrap">
        <span>基准分支</span>
        <select v-model="baseBranch" :disabled="busy || loading">
          <option value="" disabled>请选择分支</option>
          <option
            v-for="branch in branches"
            :key="`base:${branch.name}`"
            :value="branch.name"
          >
            {{ branch.name }}
          </option>
        </select>
      </label>
      <ArrowRight class="git-branch-compare-arrow" :size="16" />
      <label class="git-branch-compare-select-wrap">
        <span>对比分支</span>
        <select v-model="targetBranch" :disabled="busy || loading">
          <option value="" disabled>请选择分支</option>
          <option
            v-for="branch in branches"
            :key="`target:${branch.name}`"
            :value="branch.name"
          >
            {{ branch.name }}
          </option>
        </select>
      </label>
      <button
        class="git-branch-compare-submit"
        type="button"
        :disabled="busy || loading || !canCompare"
        @click="compareBranches"
      >
        <RefreshCw
          :size="14"
          :class="{ 'git-branch-compare-spinner': loading }"
        />
        <span>{{ loading ? "正在对比" : "开始对比" }}</span>
      </button>
    </div>

    <div v-if="error" class="git-branch-compare-error">{{ error }}</div>
    <div v-else-if="loading" class="git-branch-compare-empty">
      正在读取两个分支的差异…
    </div>
    <template v-else-if="result">
      <div class="git-branch-compare-result-head">
        <div class="git-branch-compare-result-route">
          <span class="git-branch-compare-route-branch">{{
            result.baseBranch
          }}</span>
          <ArrowRight :size="14" />
          <span
            class="git-branch-compare-route-branch git-branch-compare-route-target"
            >{{ result.targetBranch }}</span
          >
        </div>
        <code class="git-branch-compare-command">{{ result.command }}</code>
      </div>
      <div class="git-branch-compare-stats">
        <div class="git-branch-compare-stat">
          <span>变更文件</span>
          <span data-emphasis>{{ result.fileCount }}</span>
        </div>
        <div class="git-branch-compare-stat git-branch-compare-stat-add">
          <span>新增行</span>
          <span data-emphasis>+{{ result.additions }}</span>
        </div>
        <div class="git-branch-compare-stat git-branch-compare-stat-delete">
          <span>删除行</span>
          <span data-emphasis>-{{ result.deletions }}</span>
        </div>
        <div class="git-branch-compare-stat git-branch-compare-stat-total">
          <span>不同总行数</span>
          <span data-emphasis>{{ result.changedLines }}</span>
        </div>
      </div>
      <div v-if="result.files.length" class="git-branch-compare-body">
        <aside class="git-branch-compare-files">
          <div class="git-branch-compare-files-head">
            文件差异 · {{ result.files.length }}
          </div>
          <div
            v-for="file in result.files"
            :key="file.path"
            class="git-branch-compare-file"
            :title="file.path"
          >
            <span
              :class="[
                'git-branch-compare-file-status',
                `git-branch-compare-file-status-${statusClass(file.status)}`
              ]"
            >
              {{ statusLabel(file.status) }}
            </span>
            <span class="git-branch-compare-file-path">{{ file.path }}</span>
            <span v-if="file.binary" class="git-branch-compare-file-binary"
              >二进制</span
            >
            <span v-else class="git-branch-compare-file-lines">
              <span class="git-branch-compare-file-add"
                >+{{ file.additions }}</span
              >
              <span class="git-branch-compare-file-delete"
                >-{{ file.deletions }}</span
              >
            </span>
          </div>
        </aside>
        <pre
          ref="diffViewport"
          class="git-branch-compare-diff"
          @scroll="diffScrollTop = $event.target.scrollTop"
        ><code :style="{
          paddingTop: `${visibleDiffRange.start * 20}px`,
          paddingBottom: `${(diffLines.length - visibleDiffRange.end) * 20}px`
        }"><span v-for="(line, index) in diffLines.slice(visibleDiffRange.start, visibleDiffRange.end)" :key="visibleDiffRange.start + index" :class="['git-branch-compare-line', `git-branch-compare-line-${lineClass(line)}`]">{{ line || ' ' }}</span></code></pre>
      </div>
      <div v-else class="git-branch-compare-empty">两个分支没有文件差异</div>
    </template>
    <div v-else class="git-branch-compare-empty">选择两个分支后开始对比</div>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ArrowRight, RefreshCw, X } from 'lucide-vue-next'
import { gitToolApi } from '@/api'

const props = defineProps({
  repoId: { type: String, required: true },
  branches: { type: Array, default: () => [] },
  currentBranch: { type: String, default: '' },
  selectedBranch: { type: String, default: '' },
  busy: { type: Boolean, default: false }
})

const emit = defineEmits(['close'])
const baseBranch = ref('')
const targetBranch = ref('')
const loading = ref(false)
const error = ref('')
const result = ref(null)
const diffViewport = ref(null)
const diffScrollTop = ref(0)
const diffViewportHeight = ref(0)
let diffResizeObserver = null
let requestSequence = 0

const canCompare = computed(() => {
  return Boolean(
    baseBranch.value &&
    targetBranch.value &&
    baseBranch.value !== targetBranch.value
  )
})

const diffLines = computed(() => {
  if (!result.value?.patch) return []
  return result.value.patch.split('\n')
})

// 大型分支对比只渲染可见行及前后缓冲，避免数十万节点阻塞窗口缩放。
const visibleDiffRange = computed(() => ({
  start: Math.max(0, Math.floor(diffScrollTop.value / 20) - 20),
  end: Math.min(
    diffLines.value.length,
    Math.ceil((diffScrollTop.value + diffViewportHeight.value) / 20) + 20
  )
}))

watch(diffViewport, element => {
  diffResizeObserver?.disconnect()
  if (!element) return
  diffViewportHeight.value = element.clientHeight
  diffResizeObserver = new ResizeObserver(() => {
    diffViewportHeight.value = element.clientHeight
  })
  diffResizeObserver.observe(element)
}, { flush: 'post' })

watch(
  () => [props.branches, props.currentBranch, props.selectedBranch],
  () => {
    const names = props.branches.map((branch) => branch.name)
    if (!names.includes(baseBranch.value)) {
      baseBranch.value =
        names.find((name) => name === props.currentBranch) || names[0] || ''
    }
    if (
      !names.includes(targetBranch.value) ||
      targetBranch.value === baseBranch.value
    ) {
      targetBranch.value =
        names.find(
          (name) => name === props.selectedBranch && name !== baseBranch.value
        ) ||
        names.find((name) => name !== baseBranch.value) ||
        ''
    }
  },
  { immediate: true }
)

async function compareBranches() {
  if (!canCompare.value) return
  // 快速切换分支时只接受最后一次请求的结果。
  const sequence = ++requestSequence
  loading.value = true
  error.value = ''
  try {
    const nextResult = await gitToolApi.getGitToolBranchDiff({
      repoId: props.repoId,
      baseBranch: baseBranch.value,
      targetBranch: targetBranch.value
    })
    if (sequence === requestSequence) {
      result.value = nextResult
      diffScrollTop.value = 0
    }
  } catch (requestError) {
    if (sequence === requestSequence)
      error.value = String(requestError?.message || requestError)
  } finally {
    if (sequence === requestSequence) loading.value = false
  }
}

function statusLabel(status) {
  return (
    { A: '增', M: '改', D: '删', R: '移', C: '拷' }[status] || status || '-'
  )
}

function statusClass(status) {
  return (
    { A: 'add', M: 'modify', D: 'delete', R: 'move', C: 'copy' }[status] ||
    'modify'
  )
}

function lineClass(line) {
  if (line.startsWith('+') && !line.startsWith('+++')) return 'add'
  if (line.startsWith('-') && !line.startsWith('---')) return 'delete'
  if (line.startsWith('@@')) return 'chunk'
  if (line.startsWith('diff --git') || line.startsWith('index ')) return 'meta'
  return 'normal'
}

onMounted(() => {
  if (canCompare.value) compareBranches()
})

onBeforeUnmount(() => diffResizeObserver?.disconnect())
</script>

<style scoped lang="less">
.git-branch-compare {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;

  .git-branch-compare-head,
  .git-branch-compare-result-head,
  .git-branch-compare-controls,
  .git-branch-compare-stats {
    display: flex;
    align-items: center;
  }

  .git-branch-compare-head {
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-line);
  }

  .git-branch-compare-title-wrap,
  .git-branch-compare-select-wrap {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .git-branch-compare-title {
    color: var(--color-text);
    font-size: 14px;
  }

  .git-branch-compare-subtitle,
  .git-branch-compare-select-wrap > span,
  .git-branch-compare-files-head {
    color: var(--color-text-muted);
    font-size: 11px;
  }

  .git-branch-compare-close,
  .git-branch-compare-submit {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 12px;

    &:disabled {
      cursor: default;
      opacity: 0.5;
    }
  }

  .git-branch-compare-close {
    padding: 6px 9px;
  }

  .git-branch-compare-controls {
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .git-branch-compare-select-wrap {
    min-width: 0;
    flex: 1;

    select {
      min-width: 0;
      height: 30px;
      padding: 0 8px;
      border: 1px solid var(--color-line);
      border-radius: 5px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;
      font-size: 12px;
    }
  }

  .git-branch-compare-arrow {
    flex: none;
    margin-top: 15px;
    color: var(--color-text-soft);
  }

  .git-branch-compare-submit {
    flex: none;
    align-self: flex-end;
    height: 30px;
    padding: 0 10px;
    border-color: var(--color-primary);
    background: var(--color-primary-solid);
    color: #fff;
  }

  .git-branch-compare-spinner {
    animation: git-branch-compare-spin 0.9s linear infinite;
  }

  .git-branch-compare-error,
  .git-branch-compare-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 160px;
    padding: 24px;
    color: var(--color-text-muted);
  }

  .git-branch-compare-error {
    color: var(--color-danger);
  }

  .git-branch-compare-result-head {
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-line);
  }

  .git-branch-compare-result-route {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    color: var(--color-text-soft);
    font-size: 12px;
  }

  .git-branch-compare-route-branch {
    max-width: 230px;
    overflow: hidden;
    padding: 3px 7px;
    border-radius: 4px;
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-branch-compare-route-target {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .git-branch-compare-command {
    max-width: 48%;
    overflow: hidden;
    color: var(--color-text-soft);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-branch-compare-stats {
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-line);
  }

  .git-branch-compare-stat {
    display: flex;
    min-width: 100px;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    color: var(--color-text-muted);
    font-size: 11px;

    [data-emphasis] {
      color: var(--color-text);
      font-family: "JetBrains Mono", "Consolas", monospace;
      font-size: 15px;
    }
  }

  .git-branch-compare-stat-add [data-emphasis] {
    color: var(--color-success);
  }

  .git-branch-compare-stat-delete [data-emphasis] {
    color: var(--color-danger);
  }

  .git-branch-compare-stat-total {
    border-color: var(--color-primary);
  }

  .git-branch-compare-body {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
  }

  .git-branch-compare-files {
    width: 300px;
    flex: 0 0 300px;
    min-height: 0;
    overflow: auto;
    border-right: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .git-branch-compare-files-head {
    padding: 9px 10px;
    border-bottom: 1px solid var(--color-line);
  }

  .git-branch-compare-file {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 5px 9px;
    border-bottom: 1px solid var(--color-line);
  }

  .git-branch-compare-file-status {
    display: inline-flex;
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    font-size: 11px;
  }

  .git-branch-compare-file-status-add {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .git-branch-compare-file-status-delete {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .git-branch-compare-file-status-modify,
  .git-branch-compare-file-status-move,
  .git-branch-compare-file-status-copy {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  .git-branch-compare-file-path {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    color: var(--color-text);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-branch-compare-file-binary,
  .git-branch-compare-file-lines {
    flex: none;
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 10px;
  }

  .git-branch-compare-file-binary {
    color: var(--color-text-soft);
  }

  .git-branch-compare-file-lines {
    display: inline-flex;
    gap: 4px;
  }

  .git-branch-compare-file-add {
    color: var(--color-success);
  }

  .git-branch-compare-file-delete {
    color: var(--color-danger);
  }

  .git-branch-compare-diff {
    min-width: 0;
    min-height: 0;
    flex: 1;
    margin: 0;
    overflow: auto;
    background: var(--color-panel);
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 12px;
    line-height: 1.45;

    code {
      display: block;
      min-width: 100%;
      width: max-content;
    }
  }

  .git-branch-compare-line {
    display: block;
    height: 20px;
    padding: 1px 10px;
    line-height: 18px;
    white-space: pre;
  }

  .git-branch-compare-line-add {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .git-branch-compare-line-delete {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .git-branch-compare-line-chunk {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .git-branch-compare-line-meta {
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
  }
}

@keyframes git-branch-compare-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

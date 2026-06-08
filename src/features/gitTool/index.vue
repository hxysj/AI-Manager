<template>
  <section class="git-tool" @click="closeContextMenus">
    <section class="git-tool-top">
      <div class="git-tool-command-row">
        <label class="git-tool-picker">
          <span class="git-tool-label">项目</span>
          <select
            class="git-tool-select"
            :value="selectedRepoId"
            :disabled="repos.length === 0"
            @change="handleRepoChange"
          >
            <option value="" disabled>请选择项目</option>
            <option v-for="repo in repos" :key="repo.id" :value="repo.id">
              {{ repo.name }}
            </option>
          </select>
        </label>

        <span class="git-tool-repo-path">{{
          selectedRepo?.localPath || "未选择项目"
        }}</span>

        <div class="git-tool-tabs">
          <button
            :class="[
              'git-tool-tab',
              { 'git-tool-tab-active': gitWorkspace === 'branch' }
            ]"
            type="button"
            @click="selectGitWorkspace('branch')"
          >
            分支
          </button>
          <button
            :class="[
              'git-tool-tab',
              { 'git-tool-tab-active': gitWorkspace === 'stash' }
            ]"
            type="button"
            @click="selectGitWorkspace('stash')"
          >
            Stash
          </button>
        </div>
        <button
          class="git-tool-action git-tool-action-primary"
          type="button"
          :disabled="gitLoading || !selectedRepo"
          @click="refreshGitProject"
        >
          <RefreshCw :size="14" />
          刷新
        </button>
        <button
          class="git-tool-action"
          type="button"
          @click="$emit('add-repo')"
        >
          <Plus :size="14" />
          添加项目
        </button>
      </div>
    </section>

    <section v-if="!repos.length" class="git-tool-empty">
      <strong class="git-tool-empty-title">当前没有项目</strong>
      <button
        class="git-tool-action git-tool-action-primary"
        type="button"
        @click="$emit('add-repo')"
      >
        添加项目
      </button>
    </section>

    <section v-else-if="gitLoading" class="git-tool-loading">
      <RefreshCw class="git-tool-loading-icon" :size="22" />
      <strong class="git-tool-loading-title">正在加载 Git 数据</strong>
      <span class="git-tool-loading-desc">{{
        selectedRepo?.name || "当前项目"
      }}</span>
    </section>

    <section v-else-if="gitWorkspace === 'branch'" class="git-tool-workbench">
      <aside class="git-tool-branch-panel">
        <div class="git-tool-branch-head">
          <div class="git-tool-branch-title-row">
            <strong>本地分支</strong>
            <span>{{ currentBranch || "-" }}</span>
          </div>
          <span class="git-tool-branch-path">{{
            selectedRepo?.localPath || ""
          }}</span>
          <div class="git-tool-branch-summary-row">
            <span>
              已选 {{ selectedBranchNames.length }}/{{
                archivableBranches.length
              }}
            </span>
            <div class="git-tool-branch-toolbar">
              <button
                class="git-tool-branch-toolbar-button"
                type="button"
                :disabled="!archivableBranches.length"
                @click="selectAllBranches"
              >
                全选
              </button>
              <button
                class="git-tool-branch-toolbar-button"
                type="button"
                :disabled="!selectedBranchNames.length"
                @click="archiveSelectedBranches"
              >
                归档选中
              </button>
            </div>
          </div>
        </div>

        <div class="git-tool-branch-list">
          <section
            v-for="group in branchGroups"
            :key="group.id"
            class="git-tool-branch-group"
          >
            <button
              class="git-tool-branch-group-head"
              type="button"
              @click="toggleBranchGroup(group.id)"
            >
              <component
                :is="isBranchGroupClosed(group.id) ? ChevronRight : ChevronDown"
                :size="13"
              />
              <strong>{{ group.label }}</strong>
              <span>{{ group.branches.length }}</span>
            </button>
            <div
              v-if="!isBranchGroupClosed(group.id)"
              class="git-tool-branch-group-body"
            >
              <article
                v-for="branch in group.branches"
                :key="branch.name"
                :class="[
                  'git-tool-branch',
                  {
                    'git-tool-branch-active': branch.name === selectedBranch,
                    'git-tool-branch-current': branch.isCurrent
                  }
                ]"
                @contextmenu.prevent="openBranchContextMenu($event, branch)"
              >
                <input
                  class="git-tool-branch-check"
                  type="checkbox"
                  :checked="isBranchChecked(branch.name)"
                  :disabled="branch.isCurrent"
                  @click.stop
                  @change="toggleBranchChecked(branch, $event)"
                />
                <button
                  class="git-tool-branch-main"
                  type="button"
                  @click="selectBranch(branch.name)"
                >
                  <GitBranchIcon :size="14" />
                  <span class="git-tool-branch-name" :title="branch.name">{{
                    branch.name
                  }}</span>
                  <span v-if="branch.isCurrent" class="git-tool-branch-badge">
                    当前
                  </span>
                </button>
              </article>
            </div>
          </section>
        </div>
      </aside>

      <section class="git-tool-commit-panel">
        <div class="git-tool-panel-head">
          <div>
            <strong class="git-tool-panel-title">{{
              selectedBranch || "提交记录"
            }}</strong>
            <span class="git-tool-panel-subtitle"
              >{{ commits.length }} 条提交</span
            >
          </div>
          <div class="git-tool-panel-actions">
            <select
              class="git-tool-mini-select"
              :value="project?.checkBranchName || ''"
              :disabled="!selectedBranch"
              title="检查分支"
              @change="handleCheckBranchChange"
            >
              <option value="">不检查</option>
              <option
                v-for="branch in branches"
                :key="branch.name"
                :value="branch.name"
                :disabled="branch.name === selectedBranch"
              >
                {{ branch.name }}
              </option>
            </select>
            <button
              class="git-tool-icon-button"
              type="button"
              title="清理检查缓存"
              :disabled="!project?.checkBranchName || !selectedBranch"
              @click="clearCheckCache"
            >
              <RotateCcw :size="14" />
            </button>
            <button
              class="git-tool-action"
              type="button"
              :disabled="!selectedRepo"
              @click="openArchiveListDrawer"
            >
              <Archive :size="14" />
              归档列表
            </button>
          </div>
        </div>

        <div class="git-tool-commit-layout">
          <div class="git-tool-commit-list git-tool-commit-table">
            <div class="git-tool-commit-table-head">
              <span class="git-tool-commit-table-head-cell">图谱</span>
              <span class="git-tool-commit-table-head-cell">描述</span>
              <span class="git-tool-commit-table-head-cell">日期</span>
              <span class="git-tool-commit-table-head-cell">作者</span>
              <span class="git-tool-commit-table-head-cell">提交</span>
            </div>
            <button
              v-for="commit in commits"
              :key="commit.rowId"
              :class="[
                'git-tool-commit',
                {
                  'git-tool-commit-active':
                    commit.hash && commit.hash === selectedCommit?.hash,
                  'git-tool-commit-graph': commit.isGraphOnly
                }
              ]"
              type="button"
              :disabled="commit.isGraphOnly"
              @click="selectCommit(commit)"
              @contextmenu.prevent="openCommitContextMenu($event, commit)"
            >
              <span class="git-tool-commit-graph-cell">
                <svg
                  class="git-tool-commit-graph-svg"
                  viewBox="0 0 48 32"
                  preserveAspectRatio="none"
                >
                  <path
                    v-for="line in getCommitGraphLines(commit)"
                    :key="`${commit.rowId}:${line.key}`"
                    class="git-tool-commit-graph-line"
                    :d="line.path"
                    :stroke="line.color"
                  />
                  <circle
                    v-if="getCommitGraphNode(commit)"
                    class="git-tool-commit-graph-node"
                    :cx="getCommitGraphNode(commit)?.x"
                    cy="16"
                    r="4"
                    :fill="getCommitGraphNode(commit)?.color"
                  />
                </svg>
              </span>
              <span class="git-tool-commit-description">
                <strong class="git-tool-commit-title" :title="commit.subject">{{
                  commit.subject
                }}</strong>
                <span
                  v-if="commit.checkStatus !== 'none'"
                  :class="[
                    'git-tool-check',
                    `git-tool-check-${commit.checkStatus}`
                  ]"
                >
                  {{ formatCheckStatus(commit.checkStatus) }}
                </span>
              </span>
              <span class="git-tool-commit-date">
                {{ commit.isGraphOnly ? "" : formatFullDate(commit.date) }}
              </span>
              <span class="git-tool-commit-author">
                {{ commit.isGraphOnly ? "" : commit.author }}
              </span>
              <span class="git-tool-commit-hash">
                {{ commit.isGraphOnly ? "" : commit.shortHash }}
              </span>
            </button>
            <div v-if="commitsLoading" class="git-tool-list-empty">
              正在读取提交
            </div>
            <div v-else-if="!commits.length" class="git-tool-list-empty">
              暂无提交记录
            </div>
          </div>
        </div>
      </section>
    </section>

    <section v-else class="git-tool-stash-workbench">
      <section class="git-tool-stash-panel">
        <div class="git-tool-panel-head">
          <div>
            <strong class="git-tool-panel-title">当前 Stash</strong>
            <span class="git-tool-panel-subtitle"
              >{{ stashes.length }} 条记录</span
            >
          </div>
          <div class="git-tool-panel-actions">
            <button
              class="git-tool-action"
              type="button"
              :disabled="stashLoading || !selectedRepo"
              @click="loadStashes"
            >
              <RefreshCw :size="14" />
              刷新
            </button>
          </div>
        </div>

        <div class="git-tool-stash-summary-row">
          <span>已选 {{ selectedStashHashes.length }}/{{ stashes.length }}</span>
          <div class="git-tool-stash-toolbar">
            <button
              class="git-tool-branch-toolbar-button"
              type="button"
              :disabled="!stashes.length"
              @click="selectAllStashes"
            >
              全选
            </button>
            <button
              class="git-tool-branch-toolbar-button"
              type="button"
              :disabled="!selectedStashHashes.length"
              @click="archiveSelectedStashes"
            >
              归档选中
            </button>
          </div>
        </div>

        <div class="git-tool-stash-list">
          <div v-if="stashLoading" class="git-tool-list-empty">
            正在读取 Stash
          </div>
          <template v-else>
            <article
              v-for="stash in stashes"
              :key="stash.hash"
              class="git-tool-stash"
            >
              <input
                class="git-tool-stash-check"
                type="checkbox"
                :checked="isStashChecked(stash.hash)"
                @click.stop
                @change="toggleStashChecked(stash, $event)"
              />
              <button
                class="git-tool-stash-main"
                type="button"
                @click="openStashDetail(stash)"
              >
                <strong
                  class="git-tool-stash-name"
                  :title="`${stash.stashRef} ${stash.subject}`"
                >
                  {{ stash.stashRef }} {{ stash.subject }}
                </strong>
                <span class="git-tool-stash-meta">
                  {{ stash.shortHash }} · {{ stash.author }} ·
                  {{ formatDate(stash.date) }}
                </span>
              </button>
              <button
                class="git-tool-action"
                type="button"
                @click="archiveStash(stash)"
              >
                <ArchiveRestore :size="14" />
                归档
              </button>
            </article>
            <div v-if="!stashes.length" class="git-tool-list-empty">
              暂无 stash
            </div>
          </template>
        </div>
      </section>

      <section class="git-tool-stash-panel">
        <div class="git-tool-panel-head">
          <div>
            <strong class="git-tool-panel-title">Stash 归档</strong>
            <span class="git-tool-panel-subtitle"
              >{{ stashArchives.length }} 条记录</span
            >
          </div>
        </div>

        <div class="git-tool-stash-list">
          <div v-if="stashLoading" class="git-tool-list-empty">
            正在读取 Stash 归档
          </div>
          <template v-else>
            <article
              v-for="archive in stashArchives"
              :key="archive.stashArchiveId"
              class="git-tool-stash"
            >
              <button
                class="git-tool-stash-main"
                type="button"
                @click="openStashArchiveDetail(archive)"
              >
                <strong
                  class="git-tool-stash-name"
                  :title="`${archive.stashRef} ${archive.message}`"
                >
                  {{ archive.stashRef }} {{ archive.message }}
                </strong>
                <span class="git-tool-stash-meta">
                  {{ formatHash(archive.commitHash) }} ·
                  {{ formatDate(archive.archivedAt) }}
                </span>
              </button>
              <div class="git-tool-stash-actions">
                <button
                  class="git-tool-icon-button"
                  type="button"
                  title="恢复 stash"
                  @click="restoreStashArchive(archive)"
                >
                  <RotateCcw :size="14" />
                </button>
                <button
                  class="git-tool-icon-button git-tool-icon-danger"
                  type="button"
                  title="删除 stash 归档"
                  @click="deleteStashArchive(archive)"
                >
                  <Trash2 :size="14" />
                </button>
              </div>
            </article>
            <div v-if="!stashArchives.length" class="git-tool-list-empty">
              暂无 stash 归档
            </div>
          </template>
        </div>
      </section>
    </section>

    <section
      v-if="detailDrawerVisible"
      class="git-tool-drawer"
      @click="closeDetailDrawer"
    >
      <div
        :class="[
          'git-tool-drawer-panel',
          {
            'git-tool-drawer-panel-archives': detailDrawerType === 'archives'
          }
        ]"
        @click.stop
      >
        <header class="git-tool-drawer-head">
          <div>
            <button
              v-if="detailDrawerType === 'archive'"
              class="git-tool-drawer-back"
              type="button"
              @click="backToArchiveList"
            >
              <ChevronRight class="git-tool-drawer-back-icon" :size="14" />
              <span class="git-tool-drawer-back-text">返回归档列表</span>
            </button>
            <span v-else class="git-tool-label">{{ detailDrawerEyebrow }}</span>
            <strong class="git-tool-drawer-title">{{
              detailDrawerTitle
            }}</strong>
          </div>
          <div class="git-tool-drawer-actions">
            <span
              v-if="detailDrawerType === 'archive'"
              class="git-tool-drawer-hash"
            >
              {{ formatHash(selectedArchive?.commitHash) }}
            </span>
            <button
              class="git-tool-icon-button"
              type="button"
              title="关闭"
              @click="closeDetailDrawer"
            >
              <X :size="14" />
            </button>
          </div>
        </header>
        <div
          v-if="detailDrawerType === 'archives'"
          class="git-tool-drawer-body"
        >
          <div class="git-tool-drawer-archives">
            <div v-if="archives.length" class="git-tool-archive-tools">
              <span>
                已选 {{ selectedArchiveIds.length }}/{{ archives.length }}
              </span>
              <div class="git-tool-archive-toolbar">
                <button
                  class="git-tool-branch-toolbar-button"
                  type="button"
                  :disabled="!archives.length"
                  @click="selectAllArchives"
                >
                  全选
                </button>
                <button
                  class="git-tool-branch-toolbar-button"
                  type="button"
                  :disabled="!selectedArchives.length"
                  @click="restoreArchives(selectedArchives)"
                >
                  恢复选中
                </button>
                <button
                  class="git-tool-branch-toolbar-button"
                  type="button"
                  @click="restoreArchives(archives)"
                >
                  全部恢复
                </button>
              </div>
            </div>
            <section
              v-for="group in archiveGroups"
              :key="group.id"
              class="git-tool-archive-group"
            >
              <div class="git-tool-archive-group-head">
                <button
                  class="git-tool-archive-group-toggle"
                  type="button"
                  @click="toggleArchiveGroup(group.id)"
                >
                  <component
                    :is="
                      isArchiveGroupClosed(group.id)
                        ? ChevronRight
                        : ChevronDown
                    "
                    :size="13"
                  />
                  <strong>{{ group.label }}</strong>
                  <span>{{ group.archives.length }}</span>
                </button>
                <button
                  class="git-tool-branch-toolbar-button"
                  type="button"
                  @click="restoreArchives(group.archives)"
                >
                  恢复本组
                </button>
              </div>
              <div
                v-if="!isArchiveGroupClosed(group.id)"
                class="git-tool-archive-group-body"
              >
                <article
                  v-for="archive in group.archives"
                  :key="archive.archiveId"
                  class="git-tool-archive"
                >
                  <input
                    class="git-tool-archive-check"
                    type="checkbox"
                    :checked="isArchiveChecked(archive.archiveId)"
                    @click.stop
                    @change="toggleArchiveChecked(archive, $event)"
                  />
                  <button
                    class="git-tool-archive-main"
                    type="button"
                    @click="openArchiveDetail(archive)"
                  >
                    <strong
                      class="git-tool-archive-name"
                      :title="archive.branchName"
                      >{{ archive.branchName }}</strong
                    >
                    <span class="git-tool-archive-path">{{
                      archive.projectPath
                    }}</span>
                    <span class="git-tool-archive-meta">
                      <code>{{ formatHash(archive.commitHash) }}</code>
                      <span>{{ formatDate(archive.archivedAt) }}</span>
                    </span>
                  </button>
                  <div class="git-tool-archive-actions">
                    <button
                      class="git-tool-icon-button"
                      type="button"
                      title="恢复归档"
                      @click="restoreArchive(archive)"
                    >
                      <RotateCcw :size="14" />
                    </button>
                    <button
                      class="git-tool-icon-button git-tool-icon-danger"
                      type="button"
                      title="删除归档"
                      @click="deleteArchive(archive)"
                    >
                      <Trash2 :size="14" />
                    </button>
                  </div>
                </article>
              </div>
            </section>
            <div v-if="!archives.length" class="git-tool-list-empty">
              暂无分支归档
            </div>
          </div>
        </div>
        <div
          v-else-if="detailDrawerType === 'archive'"
          class="git-tool-drawer-body git-tool-archive-detail"
        >
          <div class="git-tool-archive-detail-meta">
            <span>{{ selectedArchive?.projectPath || "" }}</span>
            <strong>{{ formatFullDate(selectedArchive?.archivedAt) }}</strong>
          </div>

          <div class="git-tool-archive-commit-table">
            <div class="git-tool-archive-commit-head">
              <span>描述</span>
              <span>日期</span>
              <span>作者</span>
              <span>提交</span>
            </div>
            <div class="git-tool-archive-commit-list">
              <button
                v-for="commit in visibleArchiveCommits"
                :key="commit.rowId"
                :class="[
                  'git-tool-archive-commit',
                  {
                    'git-tool-archive-commit-active':
                      commit.hash && commit.hash === selectedArchiveCommit?.hash
                  }
                ]"
                type="button"
                :disabled="commit.isGraphOnly"
                @click="selectArchiveCommit(commit)"
              >
                <span class="git-tool-archive-commit-title">{{
                  commit.subject
                }}</span>
                <span>{{
                  commit.isGraphOnly ? "" : formatFullDate(commit.date)
                }}</span>
                <span>{{ commit.isGraphOnly ? "" : commit.author }}</span>
                <span class="git-tool-archive-commit-hash">{{
                  commit.isGraphOnly ? "" : commit.shortHash
                }}</span>
              </button>
              <div
                v-if="!visibleArchiveCommits.length"
                class="git-tool-list-empty"
              >
                暂无归档提交
              </div>
            </div>
          </div>

          <div class="git-tool-archive-detail-content">
            <div
              v-if="archiveCommitDetailLoading"
              class="git-tool-detail-empty"
            >
              正在读取详情
            </div>
            <GitChangeDetail
              v-else
              :detail="selectedArchiveCommitDetail"
              :title="selectedArchiveCommit?.subject || '归档提交详情'"
              @select-file="selectArchiveCommitFile"
            />
          </div>
        </div>
        <div v-else class="git-tool-drawer-body git-tool-drawer-body-detail">
          <div v-if="activeDetailLoading" class="git-tool-detail-empty">
            正在读取详情
          </div>
          <GitChangeDetail
            v-else
            :detail="activeDetail"
            :title="activeDetailTitle"
            @select-file="selectActiveDetailFile"
          />
        </div>
      </div>
    </section>

    <div
      v-if="commitContextMenu.visible"
      class="git-tool-context-menu"
      :style="{
        left: `${commitContextMenu.x}px`,
        top: `${commitContextMenu.y}px`
      }"
      @click.stop
    >
      <button
        class="git-tool-context-menu-button"
        type="button"
        @click="checkContextCommitOnBranch"
      >
        <RefreshCw :size="14" />
        校验分支是否存在
      </button>
      <button
        class="git-tool-context-menu-button"
        type="button"
        @click="copyContextCommitSubject"
      >
        <Copy :size="14" />
        复制提交消息
      </button>
      <button
        class="git-tool-context-menu-button"
        type="button"
        @click="copyContextCommitHash"
      >
        <Hash :size="14" />
        复制完整 Hash
      </button>
    </div>

    <div
      v-if="branchContextMenu.visible"
      class="git-tool-context-menu"
      :style="{
        left: `${branchContextMenu.x}px`,
        top: `${branchContextMenu.y}px`
      }"
      @click.stop
    >
      <button
        class="git-tool-context-menu-button"
        type="button"
        @click="copyContextBranchName"
      >
        <Copy :size="14" />
        复制分支名
      </button>
    </div>

    <BaseModal
      v-if="confirmDialog.visible"
      :title="confirmDialog.title"
      :description="confirmDialog.description"
      @close="cancelConfirmDialog"
    >
      <section class="git-tool-confirm">
        <div class="git-tool-confirm-icon">
          <Archive :size="19" />
        </div>
        <div class="git-tool-confirm-content">
          <strong>{{ confirmDialog.message }}</strong>
          <span v-if="confirmDialog.detail">{{ confirmDialog.detail }}</span>
          <label v-if="confirmDialog.input" class="git-tool-confirm-field">
            <span>{{ confirmDialog.inputLabel }}</span>
            <input
              v-model.trim="confirmDialog.inputValue"
              type="text"
              @keydown.enter="resolveConfirmDialog"
            />
          </label>
        </div>
      </section>
      <div class="git-tool-confirm-actions">
        <button
          class="git-tool-confirm-button"
          type="button"
          @click="cancelConfirmDialog"
        >
          取消
        </button>
        <button
          class="git-tool-confirm-button git-tool-confirm-button-primary"
          type="button"
          @click="resolveConfirmDialog"
        >
          {{ confirmDialog.confirmText }}
        </button>
      </div>
    </BaseModal>
  </section>
</template>

<script setup>
import {
  computed,
  defineComponent,
  h,
  nextTick,
  onBeforeUnmount,
  ref,
  watch
} from "vue"
import {
  Archive,
  ArchiveRestore,
  ChevronDown,
  ChevronRight,
  Copy,
  FileText,
  Folder,
  GitBranch as GitBranchIcon,
  Hash,
  Plus,
  RefreshCw,
  RotateCcw,
  Trash2,
  X
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { gitToolApi } from "@/api"
import { createMessage } from "@/utils/message"

const props = defineProps({
  repos: {
    type: Array,
    required: true
  }
})

const emit = defineEmits(["add-repo", "status-change"])

const GitChangeDetail = defineComponent({
  props: {
    detail: {
      type: Object,
      default: null
    },
    title: {
      type: String,
      default: ""
    }
  },
  emits: ["select-file"],
  setup(detailProps, { emit }) {
    const selectedPath = computed(
      () => detailProps.detail?.selectedFilePath || ""
    )
    const diffLines = computed(() =>
      getDiffLines(detailProps.detail?.patch || "")
    )
    const fileTree = computed(() =>
      buildFileTree(detailProps.detail?.files || [])
    )
    const closedDirectoryPaths = ref(new Set())

    watch(
      () => detailProps.detail?.hash,
      () => {
        closedDirectoryPaths.value = new Set()
      }
    )

    function toggleDirectoryClosed(path) {
      const nextPaths = new Set(closedDirectoryPaths.value)

      if (nextPaths.has(path)) {
        nextPaths.delete(path)
      } else {
        nextPaths.add(path)
      }

      closedDirectoryPaths.value = nextPaths
    }

    return () =>
      h(
        "section",
        {
          class: "git-change-view"
        },
        detailProps.detail
          ? [
              h("header", { class: "git-change-view-head" }, [
                h("div", { class: "git-change-view-summary" }, [
                  h("span", { class: "git-tool-label" }, "提交详情"),
                  h(
                    "strong",
                    {
                      class: "git-change-view-title",
                      title: detailProps.title
                    },
                    detailProps.title || detailProps.detail.subject
                  ),
                  h("div", { class: "git-change-view-meta-row" }, [
                    h("span", {}, [
                      h("small", {}, "完整 hash"),
                      h("code", {}, detailProps.detail.hash || "-")
                    ]),
                    h("span", {}, [
                      h("small", {}, "作者"),
                      h("strong", {}, detailProps.detail.author || "-")
                    ]),
                    h("span", {}, [
                      h("small", {}, "提交时间"),
                      h("strong", {}, formatDate(detailProps.detail.date))
                    ])
                  ])
                ])
              ]),
              h("div", { class: "git-change-view-body" }, [
                h("aside", { class: "git-change-view-tree" }, [
                  h("div", { class: "git-change-view-tree-head" }, [
                    h("span", {}, "变更文件"),
                    h("strong", {}, `${detailProps.detail.files.length} 个文件`)
                  ]),
                  h(
                    "div",
                    { class: "git-change-view-tree-body" },
                    fileTree.value.children.length
                      ? renderFileTreeNodes(
                          fileTree.value.children,
                          selectedPath.value,
                          emit,
                          closedDirectoryPaths.value,
                          toggleDirectoryClosed
                        )
                      : h(
                          "div",
                          { class: "git-tool-list-empty" },
                          "暂无变更文件"
                        )
                  )
                ]),
                h("section", { class: "git-change-view-diff-panel" }, [
                  h("header", { class: "git-change-view-file-head" }, [
                    h(
                      "strong",
                      { title: selectedPath.value },
                      selectedPath.value || "请选择文件"
                    )
                  ]),
                  h(
                    "pre",
                    { class: "git-change-view-diff" },
                    diffLines.value.length
                      ? diffLines.value.map((line, index) =>
                          h(
                            "code",
                            {
                              key: `${index}:${line}`,
                              class: [
                                "git-change-view-line",
                                `git-change-view-line-${getDiffLineClass(line)}`
                              ]
                            },
                            line || " "
                          )
                        )
                      : h(
                          "code",
                          { class: "git-change-view-line" },
                          "暂无 diff"
                        )
                  )
                ])
              ])
            ]
          : h("div", { class: "git-tool-detail-empty" }, "请选择记录")
      )
  }
})

function buildFileTree(files) {
  const root = {
    name: "",
    path: "",
    type: "directory",
    count: 0,
    children: []
  }

  for (const file of files) {
    const parts = String(file.path || "")
      .split("/")
      .filter(Boolean)
    let current = root

    current.count += 1

    parts.forEach((part, index) => {
      const isFile = index === parts.length - 1
      const pathText = parts.slice(0, index + 1).join("/")
      let child = current.children.find(
        (item) =>
          item.name === part && item.type === (isFile ? "file" : "directory")
      )

      if (!child) {
        child = {
          name: part,
          path: pathText,
          type: isFile ? "file" : "directory",
          count: 0,
          file: isFile ? file : null,
          children: []
        }
        current.children.push(child)
      }

      child.count += 1
      current = child
    })
  }

  sortFileTree(root)
  return root
}

function sortFileTree(node) {
  node.children.sort((left, right) => {
    if (left.type !== right.type) {
      return left.type === "directory" ? -1 : 1
    }

    return left.name.localeCompare(right.name)
  })

  node.children.forEach((child) => sortFileTree(child))
}

function renderFileTreeNodes(
  nodes,
  selectedPath,
  emit,
  closedDirectoryPaths,
  toggleDirectoryClosed,
  level = 0
) {
  return nodes.map((node) => {
    if (node.type === "directory") {
      const isClosed = closedDirectoryPaths.has(node.path)
      const children = [
        h(
          "button",
          {
            class: [
              "git-change-view-tree-directory",
              { "git-change-view-tree-directory-closed": isClosed }
            ],
            style: { paddingLeft: `${level * 14 + 8}px` },
            type: "button",
            title: node.path,
            onClick: () => toggleDirectoryClosed(node.path)
          },
          [
            h(isClosed ? ChevronRight : ChevronDown, {
              class: "git-change-view-tree-caret",
              size: 13
            }),
            h(Folder, {
              class: "git-change-view-tree-folder",
              size: 13
            }),
            h("strong", { title: node.path }, node.name),
            h("small", {}, node.count)
          ]
        )
      ]

      if (!isClosed) {
        children.push(
          h(
            "div",
            { class: "git-change-view-tree-children" },
            renderFileTreeNodes(
              node.children,
              selectedPath,
              emit,
              closedDirectoryPaths,
              toggleDirectoryClosed,
              level + 1
            )
          )
        )
      }

      return h(
        "div",
        {
          key: `dir:${node.path}`,
          class: "git-change-view-tree-node"
        },
        children
      )
    }

    return h(
      "button",
      {
        key: `file:${node.path}`,
        class: [
          "git-change-view-tree-file",
          { "git-change-view-tree-file-active": node.path === selectedPath }
        ],
        style: { paddingLeft: `${level * 14 + 8}px` },
        type: "button",
        title: node.file.oldPath
          ? `${node.file.oldPath} -> ${node.path}`
          : node.path,
        onClick: () => emit("select-file", node.path)
      },
      [
        h(
          "span",
          {
            class: [
              "git-change-view-file-status",
              `git-change-view-file-status-${getFileStatusClass(node.file.status)}`
            ]
          },
          getFileStatusLabel(node.file.status)
        ),
        h(FileText, {
          class: "git-change-view-file-icon",
          size: 13
        }),
        h("span", { class: "git-change-view-file-path" }, node.name)
      ]
    )
  })
}

const selectedRepoId = ref("")
const gitWorkspace = ref("branch")
const gitLoading = ref(false)
const commitsLoading = ref(false)
const commitDetailLoading = ref(false)
const detailDrawerType = ref("")
const archiveCommitDetailLoading = ref(false)
const stashDetailLoading = ref(false)
const stashLoading = ref(false)
const project = ref(null)
const branches = ref([])
const commits = ref([])
const archives = ref([])
const stashes = ref([])
const stashArchives = ref([])
const currentBranch = ref("")
const selectedBranch = ref("")
const selectedBranchNames = ref([])
const selectedArchiveIds = ref([])
const selectedStashHashes = ref([])
const selectedCommit = ref(null)
const selectedCommitDetail = ref(null)
const selectedArchive = ref(null)
const archiveCommits = ref([])
const selectedArchiveCommit = ref(null)
const selectedArchiveCommitDetail = ref(null)
const selectedStash = ref(null)
const selectedStashArchive = ref(null)
const stashDetail = ref(null)
const closedBranchGroups = ref(new Set())
const closedArchiveGroups = ref(new Set())
const confirmDialog = ref({
  visible: false,
  title: "操作确认",
  description: "",
  message: "",
  detail: "",
  input: false,
  inputLabel: "",
  inputValue: "",
  confirmText: "确定",
  resolve: null
})
let commitLoadSeq = 0
let stashLoaded = false
const graphColors = ["#d95c6a", "#d08a2f", "#3f8b62", "#4679b2", "#8a64b7"]
const graphTrackOffset = 14
const graphColumnWidth = 4
const graphNodeY = 16
const graphNodeRadius = 4
const graphLineStart = -1
const graphLineEnd = 33
let refreshTimer = 0
const commitContextMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  commit: null
})
const branchContextMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  branch: null
})

const selectedRepo = computed(() => {
  return props.repos.find((item) => item.id === selectedRepoId.value) || null
})

const gitToolStatus = computed(() => [
  {
    label: "当前分支",
    value: currentBranch.value || "-"
  },
  {
    label: "本地分支",
    value: branches.value.length
  },
  {
    label: "Stash",
    value: stashes.value.length
  },
  {
    label: "归档",
    value: archives.value.length + stashArchives.value.length
  }
])

const archivableBranches = computed(() => {
  return branches.value.filter((item) => !item.isCurrent)
})

const commitGraphMap = computed(() => {
  const graphMap = new Map()

  commits.value.forEach((commit) => {
    graphMap.set(commit.rowId, createCommitGraph(commit.graph || ""))
  })

  return graphMap
})

const branchGroups = computed(() => {
  const featureBranches = []
  const baseBranches = []

  branches.value.forEach((branch) => {
    if (["master", "main", "release", "develop", "dev"].includes(branch.name)) {
      baseBranches.push(branch)
      return
    }

    featureBranches.push(branch)
  })

  return [
    {
      id: "feature",
      label: "feat",
      branches: featureBranches
    },
    {
      id: "base",
      label: "基础分支",
      branches: baseBranches
    }
  ].filter((item) => item.branches.length)
})

const archiveGroups = computed(() => {
  const groupMap = new Map()
  const baseArchives = []

  archives.value.forEach((archive) => {
    if (
      ["master", "main", "release", "develop", "dev"].includes(
        archive.branchName
      )
    ) {
      baseArchives.push(archive)
      return
    }

    const groupName = archive.branchName.includes("/")
      ? archive.branchName.split("/")[0]
      : "其他分支"

    if (!groupMap.has(groupName)) {
      groupMap.set(groupName, {
        id: groupName,
        label: groupName,
        archives: []
      })
    }

    groupMap.get(groupName).archives.push(archive)
  })

  const groups = Array.from(groupMap.values())

  if (baseArchives.length) {
    groups.push({
      id: "base",
      label: "基础分支",
      archives: baseArchives
    })
  }

  return groups
})

const selectedArchives = computed(() => {
  return archives.value.filter((item) =>
    selectedArchiveIds.value.includes(item.archiveId)
  )
})

function isBranchGroupClosed(groupId) {
  return closedBranchGroups.value.has(groupId)
}

function toggleBranchGroup(groupId) {
  const nextGroups = new Set(closedBranchGroups.value)

  if (nextGroups.has(groupId)) {
    nextGroups.delete(groupId)
  } else {
    nextGroups.add(groupId)
  }

  closedBranchGroups.value = nextGroups
}

function isArchiveGroupClosed(groupId) {
  return closedArchiveGroups.value.has(groupId)
}

function toggleArchiveGroup(groupId) {
  const nextGroups = new Set(closedArchiveGroups.value)

  if (nextGroups.has(groupId)) {
    nextGroups.delete(groupId)
  } else {
    nextGroups.add(groupId)
  }

  closedArchiveGroups.value = nextGroups
}

const stashDetailTitle = computed(() => {
  if (selectedStash.value) {
    return `${selectedStash.value.stashRef} ${selectedStash.value.subject}`
  }

  if (selectedStashArchive.value) {
    return `${selectedStashArchive.value.stashRef} ${selectedStashArchive.value.message}`
  }

  return "Stash 详情"
})

const detailDrawerVisible = computed(() => Boolean(detailDrawerType.value))

const detailDrawerEyebrow = computed(() => {
  if (detailDrawerType.value === "archives") {
    return "分支归档"
  }

  if (detailDrawerType.value === "archive") {
    return "归档提交记录"
  }

  if (detailDrawerType.value === "stash") {
    return "Stash 详情"
  }

  return "提交详情"
})

const detailDrawerTitle = computed(() => {
  if (detailDrawerType.value === "archives") {
    return `${archives.value.length} 条记录`
  }

  if (detailDrawerType.value === "archive") {
    return selectedArchive.value?.branchName || "归档"
  }

  if (detailDrawerType.value === "stash") {
    return stashDetailTitle.value
  }

  return selectedCommit.value?.subject || "提交详情"
})

const activeDetail = computed(() => {
  if (detailDrawerType.value === "stash") {
    return stashDetail.value
  }

  return selectedCommitDetail.value
})

const activeDetailTitle = computed(() => {
  if (detailDrawerType.value === "stash") {
    return stashDetailTitle.value
  }

  return selectedCommit.value?.subject || "提交详情"
})

const activeDetailLoading = computed(() => {
  return detailDrawerType.value === "stash"
    ? stashDetailLoading.value
    : commitDetailLoading.value
})

const visibleArchiveCommits = computed(() => {
  return archiveCommits.value.filter((item) => !item.isGraphOnly)
})

watch(
  gitToolStatus,
  (value) => {
    emit("status-change", value)
  },
  { immediate: true }
)

watch(
  () => props.repos,
  async () => {
    const previousRepoId = selectedRepoId.value

    if (
      selectedRepoId.value &&
      !props.repos.find((item) => item.id === selectedRepoId.value)
    ) {
      selectedRepoId.value = ""
    }

    if (!selectedRepoId.value && props.repos[0]) {
      selectedRepoId.value = props.repos[0].id
    }

    if (selectedRepoId.value && selectedRepoId.value !== previousRepoId) {
      await nextTick()
      scheduleRefreshGitProject()
    }
  },
  { immediate: true }
)

function handleRepoChange(event) {
  selectedRepoId.value = event.target.value
  stashLoaded = false
  scheduleRefreshGitProject()
}

function selectGitWorkspace(workspace) {
  gitWorkspace.value = workspace

  if (workspace === "stash" && !stashLoaded) {
    loadStashes()
  }
}

function scheduleRefreshGitProject() {
  window.clearTimeout(refreshTimer)
  refreshTimer = window.setTimeout(() => {
    refreshGitProject()
  }, 30)
}

onBeforeUnmount(() => {
  window.clearTimeout(refreshTimer)
})

async function refreshGitProject() {
  if (!selectedRepoId.value) {
    return
  }

  gitLoading.value = true

  try {
    const result = await gitToolApi.scanGitToolBranches({
      repoId: selectedRepoId.value
    })

    project.value = result.project || null
    branches.value = result.branches || []
    currentBranch.value = result.currentBranch || ""
    archives.value = result.archives || []
    stashes.value = result.stashes || []
    stashArchives.value = result.stashArchives || []
    stashLoaded = false
    selectedBranchNames.value = selectedBranchNames.value.filter((branchName) =>
      branches.value.find((item) => item.name === branchName && !item.isCurrent)
    )
    selectedArchiveIds.value = selectedArchiveIds.value.filter((archiveId) =>
      archives.value.find((item) => item.archiveId === archiveId)
    )
    selectedStashHashes.value = selectedStashHashes.value.filter((stashHash) =>
      stashes.value.find((item) => item.hash === stashHash)
    )

    if (!branches.value.find((item) => item.name === selectedBranch.value)) {
      selectedBranch.value =
        branches.value.find((item) => item.name === currentBranch.value)
          ?.name ||
        branches.value[0]?.name ||
        ""
    }

    if (gitWorkspace.value === "branch" && selectedBranch.value) {
      loadCommits({ checkCommits: false })
    }

    if (gitWorkspace.value === "stash") {
      loadStashes()
    }
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function selectBranch(branchName) {
  closeCommitContextMenu()
  selectedBranch.value = branchName
  selectedCommit.value = null
  selectedCommitDetail.value = null
  await loadCommits()
}

async function loadCommits(options = {}) {
  closeCommitContextMenu()

  if (!selectedRepoId.value || !selectedBranch.value) {
    commits.value = []
    return
  }

  commitsLoading.value = true
  const loadSeq = commitLoadSeq + 1
  commitLoadSeq = loadSeq
  const repoId = selectedRepoId.value
  const branchName = selectedBranch.value
  commits.value = []
  selectedCommit.value = null
  selectedCommitDetail.value = null

  try {
    const quickCommits = await gitToolApi.listGitToolCommits({
      repoId,
      branchName,
      skipCheck: true
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      branchName !== selectedBranch.value
    ) {
      return
    }

    commits.value = quickCommits

    if (
      options.checkCommits === false ||
      !project.value?.checkBranchName ||
      project.value.checkBranchName === branchName
    ) {
      return
    }

    const checkedCommits = await gitToolApi.listGitToolCommits({
      repoId,
      branchName,
      skipCheck: false
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      branchName !== selectedBranch.value
    ) {
      return
    }

    commits.value = checkedCommits
  } catch (error) {
    showErrorMessage(error)
  } finally {
    if (loadSeq === commitLoadSeq) {
      commitsLoading.value = false
    }
  }
}

async function selectCommit(commit) {
  if (commit.isGraphOnly) {
    return
  }

  closeCommitContextMenu()
  selectedCommit.value = commit
  detailDrawerType.value = "commit"
  await loadCommitDetail(commit.hash, "")
}

function openCommitContextMenu(event, commit) {
  if (commit.isGraphOnly) {
    return
  }

  closeBranchContextMenu()
  commitContextMenu.value = {
    visible: true,
    x: event.clientX,
    y: event.clientY,
    commit
  }
}

function openBranchContextMenu(event, branch) {
  closeCommitContextMenu()
  branchContextMenu.value = {
    visible: true,
    x: event.clientX,
    y: event.clientY,
    branch
  }
}

function closeCommitContextMenu() {
  if (!commitContextMenu.value.visible) {
    return
  }

  commitContextMenu.value = {
    visible: false,
    x: 0,
    y: 0,
    commit: null
  }
}

function closeBranchContextMenu() {
  if (!branchContextMenu.value.visible) {
    return
  }

  branchContextMenu.value = {
    visible: false,
    x: 0,
    y: 0,
    branch: null
  }
}

function closeContextMenus() {
  closeCommitContextMenu()
  closeBranchContextMenu()
}

async function checkContextCommitOnBranch() {
  const commit = commitContextMenu.value.commit

  if (!commit || !selectedRepoId.value || !selectedBranch.value) {
    closeCommitContextMenu()
    return
  }

  if (!project.value?.checkBranchName) {
    createMessage.error("请先选择检查分支。")
    closeCommitContextMenu()
    return
  }

  try {
    const result = await gitToolApi.checkGitToolCommitOnBranch({
      repoId: selectedRepoId.value,
      sourceBranchName: selectedBranch.value,
      targetBranchName: project.value.checkBranchName,
      commitHash: commit.hash,
      subject: commit.subject,
      date: commit.date
    })
    const checkStatus = result?.matchedBy
      ? result.matchedBy === "hash"
        ? "exists-hash"
        : "exists-subject"
      : "missing"

    commits.value = commits.value.map((item) =>
      item.hash === commit.hash
        ? {
            ...item,
            checkStatus,
            checkTargetBranch: project.value.checkBranchName
          }
        : item
    )
    createMessage.success(formatCheckStatus(checkStatus))
  } catch (error) {
    showErrorMessage(error)
  } finally {
    closeCommitContextMenu()
  }
}

async function copyContextCommitSubject() {
  const commit = commitContextMenu.value.commit

  if (!commit) {
    closeCommitContextMenu()
    return
  }

  try {
    await navigator.clipboard.writeText(commit.subject || "")
    createMessage.success("提交消息已复制。")
  } catch (error) {
    showErrorMessage(error)
  } finally {
    closeCommitContextMenu()
  }
}

async function copyContextCommitHash() {
  const commit = commitContextMenu.value.commit

  if (!commit) {
    closeCommitContextMenu()
    return
  }

  try {
    await navigator.clipboard.writeText(commit.hash || "")
    createMessage.success("完整 Hash 已复制。")
  } catch (error) {
    showErrorMessage(error)
  } finally {
    closeCommitContextMenu()
  }
}

async function copyContextBranchName() {
  const branch = branchContextMenu.value.branch

  if (!branch) {
    closeBranchContextMenu()
    return
  }

  try {
    await navigator.clipboard.writeText(branch.name || "")
    createMessage.success("分支名已复制。")
  } catch (error) {
    showErrorMessage(error)
  } finally {
    closeBranchContextMenu()
  }
}

async function loadCommitDetail(commitHash, filePath) {
  if (!selectedRepoId.value || !commitHash) {
    return
  }

  commitDetailLoading.value = true

  try {
    selectedCommitDetail.value = await gitToolApi.getGitToolCommitDetail({
      repoId: selectedRepoId.value,
      commitHash,
      filePath
    })
  } catch (error) {
    showErrorMessage(error)
  } finally {
    commitDetailLoading.value = false
  }
}

async function selectCommitFile(filePath) {
  if (selectedCommit.value) {
    await loadCommitDetail(selectedCommit.value.hash, filePath)
  }
}

async function selectActiveDetailFile(filePath) {
  if (detailDrawerType.value === "stash") {
    await selectStashDetailFile(filePath)
    return
  }

  await selectCommitFile(filePath)
}

function openArchiveListDrawer() {
  closeCommitContextMenu()
  detailDrawerType.value = "archives"
}

function isBranchChecked(branchName) {
  return selectedBranchNames.value.includes(branchName)
}

function toggleBranchChecked(branch, event) {
  if (branch.isCurrent) {
    return
  }

  if (event.target.checked) {
    selectedBranchNames.value = [...selectedBranchNames.value, branch.name]
    return
  }

  selectedBranchNames.value = selectedBranchNames.value.filter(
    (item) => item !== branch.name
  )
}

function selectAllBranches() {
  selectedBranchNames.value = archivableBranches.value.map((item) => item.name)
}

function isArchiveChecked(archiveId) {
  return selectedArchiveIds.value.includes(archiveId)
}

function toggleArchiveChecked(archive, event) {
  if (event.target.checked) {
    selectedArchiveIds.value = [...selectedArchiveIds.value, archive.archiveId]
    return
  }

  selectedArchiveIds.value = selectedArchiveIds.value.filter(
    (item) => item !== archive.archiveId
  )
}

function selectAllArchives() {
  selectedArchiveIds.value = archives.value.map((item) => item.archiveId)
}

function isStashChecked(stashHash) {
  return selectedStashHashes.value.includes(stashHash)
}

function toggleStashChecked(stash, event) {
  if (event.target.checked) {
    selectedStashHashes.value = [...selectedStashHashes.value, stash.hash]
    return
  }

  selectedStashHashes.value = selectedStashHashes.value.filter(
    (item) => item !== stash.hash
  )
}

function selectAllStashes() {
  selectedStashHashes.value = stashes.value.map((item) => item.hash)
}

function confirmGitAction(options) {
  return new Promise((resolve) => {
    confirmDialog.value = {
      visible: true,
      title: options.title,
      description: options.description || "",
      message: options.message,
      detail: options.detail || "",
      input: Boolean(options.input),
      inputLabel: options.inputLabel || "",
      inputValue: options.inputValue || "",
      confirmText: options.confirmText || "确定",
      resolve
    }
  })
}

function closeConfirmDialog(result) {
  const resolve = confirmDialog.value.resolve

  confirmDialog.value = {
    visible: false,
    title: "操作确认",
    description: "",
    message: "",
    detail: "",
    input: false,
    inputLabel: "",
    inputValue: "",
    confirmText: "确定",
    resolve: null
  }

  if (resolve) {
    resolve(result)
  }
}

function cancelConfirmDialog() {
  closeConfirmDialog(false)
}

function resolveConfirmDialog() {
  closeConfirmDialog(
    confirmDialog.value.input ? confirmDialog.value.inputValue : true
  )
}

async function archiveSelectedBranches() {
  if (!selectedRepoId.value || !selectedBranchNames.value.length) {
    return
  }

  const branchNames = [...selectedBranchNames.value]

  if (
    !(await confirmGitAction({
      title: "归档本地分支",
      description: "归档后会清理本地分支，请确认后继续。",
      message: `归档成功后，${branchNames.length} 个本地分支会被删除。`,
      detail: `已选 ${branchNames.length}/${archivableBranches.value.length}`,
      confirmText: "确认归档"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    for (const branchName of branchNames) {
      await gitToolApi.archiveGitToolBranch({
        repoId: selectedRepoId.value,
        branchName
      })
    }

    createMessage.success("选中分支已归档。")
    selectedBranchNames.value = []
    selectedBranch.value = ""
    await refreshGitProject()
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function handleCheckBranchChange(event) {
  if (!selectedRepoId.value) {
    return
  }

  try {
    project.value = await gitToolApi.updateGitToolCheckBranch({
      repoId: selectedRepoId.value,
      branchName: event.target.value
    })
    await loadCommits()
  } catch (error) {
    showErrorMessage(error)
  }
}

async function clearCheckCache() {
  if (
    !selectedRepoId.value ||
    !selectedBranch.value ||
    !project.value?.checkBranchName
  ) {
    return
  }

  try {
    await gitToolApi.clearGitToolCommitCheckCache({
      repoId: selectedRepoId.value,
      sourceBranchName: selectedBranch.value,
      targetBranchName: project.value.checkBranchName
    })
    await loadCommits()
    createMessage.success("检查缓存已清理。")
  } catch (error) {
    showErrorMessage(error)
  }
}

async function openArchiveDetail(archive) {
  selectedArchive.value = archive
  detailDrawerType.value = "archive"
  archiveCommitDetailLoading.value = true

  try {
    archiveCommits.value = await gitToolApi.listGitToolArchiveCommits({
      archiveId: archive.archiveId
    })
    selectedArchiveCommit.value =
      archiveCommits.value.find((item) => !item.isGraphOnly) || null
    selectedArchiveCommitDetail.value = null

    if (selectedArchiveCommit.value) {
      await loadArchiveCommitDetail(selectedArchiveCommit.value.hash, "")
    }
  } catch (error) {
    showErrorMessage(error)
  } finally {
    archiveCommitDetailLoading.value = false
  }
}

function closeDetailDrawer() {
  detailDrawerType.value = ""
  selectedArchive.value = null
  archiveCommits.value = []
  selectedArchiveCommit.value = null
  selectedArchiveCommitDetail.value = null
  selectedStash.value = null
  selectedStashArchive.value = null
  stashDetail.value = null
  selectedCommit.value = null
  selectedCommitDetail.value = null
}

function backToArchiveList() {
  detailDrawerType.value = "archives"
  selectedArchive.value = null
  archiveCommits.value = []
  selectedArchiveCommit.value = null
  selectedArchiveCommitDetail.value = null
}

async function selectArchiveCommit(commit) {
  if (commit.isGraphOnly) {
    return
  }

  selectedArchiveCommit.value = commit
  await loadArchiveCommitDetail(commit.hash, "")
}

async function loadArchiveCommitDetail(commitHash, filePath) {
  if (!selectedArchive.value || !commitHash) {
    return
  }

  archiveCommitDetailLoading.value = true

  try {
    selectedArchiveCommitDetail.value =
      await gitToolApi.getGitToolArchiveCommitDetail({
        archiveId: selectedArchive.value.archiveId,
        commitHash,
        filePath
      })
  } catch (error) {
    showErrorMessage(error)
  } finally {
    archiveCommitDetailLoading.value = false
  }
}

async function selectArchiveCommitFile(filePath) {
  if (selectedArchiveCommit.value) {
    await loadArchiveCommitDetail(selectedArchiveCommit.value.hash, filePath)
  }
}

async function restoreArchive(archive) {
  const targetBranchName = await confirmGitAction({
    title: "恢复分支归档",
    description: "输入恢复后的本地分支名。",
    message: `恢复归档「${archive.branchName}」到本地分支。`,
    detail: formatHash(archive.commitHash),
    input: true,
    inputLabel: "分支名",
    inputValue: archive.branchName,
    confirmText: "恢复"
  })

  if (!targetBranchName) {
    return
  }

  gitLoading.value = true

  try {
    await gitToolApi.restoreGitToolArchive({
      archiveId: archive.archiveId,
      targetBranchName: targetBranchName.trim()
    })
    createMessage.success("分支已恢复。")
    selectedArchiveIds.value = selectedArchiveIds.value.filter(
      (item) => item !== archive.archiveId
    )
    closeDetailDrawer()
    selectedBranch.value = targetBranchName.trim()
    await refreshGitProject()
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function restoreArchives(targetArchives) {
  if (!targetArchives.length) {
    return
  }

  if (
    !(await confirmGitAction({
      title: "批量恢复分支归档",
      description: "会使用归档前的分支名恢复到本地分支。",
      message: `确认恢复 ${targetArchives.length} 个分支归档吗？`,
      detail: targetArchives.map((archive) => archive.branchName).join("、"),
      confirmText: "确认恢复"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    for (const archive of targetArchives) {
      await gitToolApi.restoreGitToolArchive({
        archiveId: archive.archiveId,
        targetBranchName: archive.branchName
      })
    }

    createMessage.success("分支归档已恢复。")
    selectedArchiveIds.value = selectedArchiveIds.value.filter(
      (archiveId) =>
        !targetArchives.find((archive) => archive.archiveId === archiveId)
    )
    closeDetailDrawer()
    selectedBranch.value = targetArchives[0].branchName
    await refreshGitProject()
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function deleteArchive(archive) {
  if (
    !(await confirmGitAction({
      title: "删除分支归档",
      description: "删除后无法在归档列表中恢复。",
      message: `确认删除归档「${archive.branchName}」吗？`,
      confirmText: "删除"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    archives.value = await gitToolApi.deleteGitToolArchive({
      archiveId: archive.archiveId
    })
    selectedArchiveIds.value = selectedArchiveIds.value.filter(
      (item) => item !== archive.archiveId
    )
    createMessage.success("归档已删除。")
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function loadStashes() {
  if (!selectedRepoId.value) {
    return
  }

  stashLoading.value = true

  try {
    stashes.value = await gitToolApi.listGitToolStashes({
      repoId: selectedRepoId.value
    })
    stashArchives.value = await gitToolApi.listGitToolStashArchives({
      repoId: selectedRepoId.value
    })
    selectedStashHashes.value = selectedStashHashes.value.filter((stashHash) =>
      stashes.value.find((item) => item.hash === stashHash)
    )
    stashLoaded = true
  } catch (error) {
    showErrorMessage(error)
  } finally {
    stashLoading.value = false
  }
}

async function openStashDetail(stash) {
  detailDrawerType.value = "stash"
  selectedStash.value = stash
  selectedStashArchive.value = null
  stashDetail.value = null
  stashDetailLoading.value = true

  try {
    stashDetail.value = await gitToolApi.getGitToolStashDetail({
      repoId: selectedRepoId.value,
      stashHash: stash.hash,
      filePath: ""
    })
  } catch (error) {
    showErrorMessage(error)
  } finally {
    stashDetailLoading.value = false
  }
}

async function openStashArchiveDetail(archive) {
  detailDrawerType.value = "stash"
  selectedStash.value = null
  selectedStashArchive.value = archive
  stashDetail.value = null
  stashDetailLoading.value = true

  try {
    stashDetail.value = await gitToolApi.getGitToolStashArchiveDetail({
      stashArchiveId: archive.stashArchiveId,
      filePath: ""
    })
  } catch (error) {
    showErrorMessage(error)
  } finally {
    stashDetailLoading.value = false
  }
}

async function selectStashDetailFile(filePath) {
  if (selectedStash.value) {
    await openStashDetailFile(filePath)
    return
  }

  if (selectedStashArchive.value) {
    await openStashArchiveDetailFile(filePath)
  }
}

async function openStashDetailFile(filePath) {
  stashDetailLoading.value = true

  try {
    stashDetail.value = await gitToolApi.getGitToolStashDetail({
      repoId: selectedRepoId.value,
      stashHash: selectedStash.value.hash,
      filePath
    })
  } catch (error) {
    showErrorMessage(error)
  } finally {
    stashDetailLoading.value = false
  }
}

async function openStashArchiveDetailFile(filePath) {
  stashDetailLoading.value = true

  try {
    stashDetail.value = await gitToolApi.getGitToolStashArchiveDetail({
      stashArchiveId: selectedStashArchive.value.stashArchiveId,
      filePath
    })
  } catch (error) {
    showErrorMessage(error)
  } finally {
    stashDetailLoading.value = false
  }
}

async function archiveStash(stash) {
  if (
    !(await confirmGitAction({
      title: "归档 Stash",
      description: "归档后会从当前 stash list 中移除。",
      message: `归档成功后，项目中的「${stash.stashRef}」会从 stash list 中删除。`,
      detail: stash.subject,
      confirmText: "确认归档"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    await gitToolApi.archiveGitToolStash({
      repoId: selectedRepoId.value,
      stashRef: stash.stashRef,
      stashHash: stash.hash
    })
    createMessage.success("stash 已归档。")
    stashDetail.value = null
    selectedStash.value = null
    await loadStashes()
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function archiveSelectedStashes() {
  if (!selectedRepoId.value || !selectedStashHashes.value.length) {
    return
  }

  const selectedStashes = stashes.value
    .filter((stash) => selectedStashHashes.value.includes(stash.hash))
    .sort((left, right) => right.index - left.index)

  if (
    !(await confirmGitAction({
      title: "归档 Stash",
      description: "归档后会从当前 stash list 中移除。",
      message: `归档成功后，${selectedStashes.length} 条 Stash 会从 stash list 中删除。`,
      detail: `已选 ${selectedStashes.length}/${stashes.value.length}`,
      confirmText: "确认归档"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    for (const stash of selectedStashes) {
      await gitToolApi.archiveGitToolStash({
        repoId: selectedRepoId.value,
        stashRef: stash.stashRef,
        stashHash: stash.hash
      })
    }

    createMessage.success("选中 stash 已归档。")
    selectedStashHashes.value = []
    stashDetail.value = null
    selectedStash.value = null
    await loadStashes()
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function restoreStashArchive(archive) {
  if (
    !(await confirmGitAction({
      title: "恢复 Stash",
      description: "恢复后会重新写入当前项目的 stash list。",
      message: `确认恢复「${archive.stashRef}」到 stash list 吗？`,
      detail: archive.message,
      confirmText: "恢复"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    await gitToolApi.restoreGitToolStashArchive({
      stashArchiveId: archive.stashArchiveId
    })
    createMessage.success("stash 已恢复。")
    stashDetail.value = null
    selectedStashArchive.value = null
    await loadStashes()
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

async function deleteStashArchive(archive) {
  if (
    !(await confirmGitAction({
      title: "删除 Stash 归档",
      description: "删除后无法在归档列表中恢复。",
      message: `确认删除 stash 归档「${archive.stashRef}」吗？`,
      detail: archive.message,
      confirmText: "删除"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    stashArchives.value = await gitToolApi.deleteGitToolStashArchive({
      stashArchiveId: archive.stashArchiveId
    })
    createMessage.success("stash 归档已删除。")
  } catch (error) {
    showErrorMessage(error)
  } finally {
    gitLoading.value = false
  }
}

function formatHash(value) {
  return String(value || "").slice(0, 8) || "-"
}

function formatDate(value) {
  if (!value) {
    return "-"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(new Date(value))
}

function formatFullDate(value) {
  if (!value) {
    return "-"
  }

  const date = new Date(value)
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  const hour = String(date.getHours()).padStart(2, "0")
  const minute = String(date.getMinutes()).padStart(2, "0")

  return `${year}/${month}/${day} ${hour}:${minute}`
}

function formatCheckStatus(status) {
  const statusMap = {
    "exists-hash": "已合入",
    "exists-subject": "疑似已合入",
    missing: "未合入"
  }

  return statusMap[status] || ""
}

function getGraphColumnX(index) {
  return graphTrackOffset + index * graphColumnWidth
}

function getGraphColor(index) {
  return graphColors[Math.abs(index) % graphColors.length]
}

function createCommitGraph(graph) {
  return {
    lines: createGraphLines(graph),
    node: createGraphNode(graph)
  }
}

function getCommitGraphLines(commit) {
  return commitGraphMap.value.get(commit.rowId)?.lines || []
}

function getCommitGraphNode(commit) {
  return commitGraphMap.value.get(commit.rowId)?.node || null
}

// 按 git --graph 的字符轨道绘制，同一轨道颜色固定绑定。
function createGraphLines(graph) {
  const lines = []

  graph.split("").forEach((char, index) => {
    const x = getGraphColumnX(index)

    if (char === "*") {
      lines.push({
        key: `node-line-top-${index}`,
        path: `M ${x} ${graphLineStart} L ${x} ${graphNodeY - graphNodeRadius}`,
        color: getGraphColor(index)
      })
      lines.push({
        key: `node-line-bottom-${index}`,
        path: `M ${x} ${graphNodeY + graphNodeRadius} L ${x} ${graphLineEnd}`,
        color: getGraphColor(index)
      })
      return
    }

    if (char === "/") {
      lines.push({
        key: `diagonal-left-${index}`,
        path: `M ${x + graphColumnWidth} ${graphLineStart} L ${x - graphColumnWidth} ${graphLineEnd}`,
        color: getGraphColor(index)
      })
      return
    }

    if (char === "\\") {
      lines.push({
        key: `diagonal-right-${index}`,
        path: `M ${x - graphColumnWidth} ${graphLineStart} L ${x + graphColumnWidth} ${graphLineEnd}`,
        color: getGraphColor(index)
      })
      return
    }

    if (char === "|") {
      lines.push({
        key: `line-${index}`,
        path: `M ${x} ${graphLineStart} L ${x} ${graphLineEnd}`,
        color: getGraphColor(index)
      })
    }
  })

  return lines
}

function createGraphNode(graph) {
  const nodeIndex = graph.indexOf("*")

  if (nodeIndex === -1) {
    return null
  }

  return {
    x: getGraphColumnX(nodeIndex),
    color: getGraphColor(nodeIndex)
  }
}

function getFileStatusLabel(status) {
  const statusMap = {
    A: "增",
    M: "改",
    D: "删",
    R: "移",
    C: "拷"
  }

  return statusMap[status] || status || "-"
}

function getFileStatusClass(status) {
  const statusMap = {
    A: "add",
    M: "modify",
    D: "delete",
    R: "move",
    C: "copy"
  }

  return statusMap[status] || "modify"
}

function getDiffLines(patch) {
  return String(patch || "").split("\n")
}

function getDiffLineClass(line) {
  if (line.startsWith("+") && !line.startsWith("+++")) {
    return "add"
  }

  if (line.startsWith("-") && !line.startsWith("---")) {
    return "delete"
  }

  if (line.startsWith("@@")) {
    return "chunk"
  }

  if (line.startsWith("diff --git") || line.startsWith("index ")) {
    return "meta"
  }

  return "normal"
}

function showErrorMessage(error) {
  createMessage.error(error?.message || "操作失败")
}
</script>

<style scoped lang="less">
.git-tool :deep(.base-modal) {
  z-index: 96;
}

.git-tool :deep(.base-modal__panel) {
  width: 420px;
  border-color: #cbddec;
  box-shadow: 0 20px 52px rgba(15, 23, 42, 0.2);
}

.git-tool :deep(.base-modal__header) {
  align-items: center;
  padding: 15px 16px 8px;
}

.git-tool :deep(.base-modal__header h2) {
  color: var(--color-text);
  font-size: 0.98rem;
}

.git-tool :deep(.base-modal__header p) {
  margin-top: 4px;
  color: var(--color-text-muted);
  font-size: 0.78rem;
}

.git-tool :deep(.base-modal__close) {
  width: 30px;
  height: 30px;
  border-radius: 7px;
  font-size: 1.1rem;
}

.git-tool :deep(.base-modal__content) {
  padding: 0 16px 16px;
}

.git-tool-confirm {
  display: flex;
  gap: 12px;
  padding: 12px;
  border: 1px solid #d8e4ee;
  border-radius: 8px;
  background: #f8fbff;
}

.git-tool-confirm-icon {
  display: grid;
  width: 34px;
  height: 34px;
  flex: none;
  place-items: center;
  border: 1px solid #c4d8ea;
  border-radius: 8px;
  background: #eef6ff;
  color: var(--color-primary);
}

.git-tool-confirm-content {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 5px;
}

.git-tool-confirm-content strong {
  color: var(--color-text);
  font-size: 0.86rem;
  line-height: 1.5;
}

.git-tool-confirm-content span {
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: 0.76rem;
  line-height: 1.45;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-confirm-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding-top: 5px;
}

.git-tool-confirm-field span {
  color: var(--color-text-muted);
  font-size: 0.72rem;
  font-weight: 700;
}

.git-tool-confirm-field input {
  height: 32px;
  padding: 0 9px;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: #ffffff;
  color: var(--color-text);
  font-size: 0.8rem;
  font-weight: 700;
}

.git-tool-confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding-top: 12px;
}

.git-tool-confirm-button {
  display: inline-flex;
  min-width: 74px;
  height: 32px;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: #ffffff;
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.8rem;
  font-weight: 700;
}

.git-tool-confirm-button:hover {
  border-color: #b9ccda;
  background: #f7f9fc;
}

.git-tool-confirm-button-primary {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #ffffff;
}

.git-tool-confirm-button-primary:hover {
  border-color: var(--color-primary);
  background: #284f79;
}

.git-tool-action,
.git-tool-icon-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: #ffffff;
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.82rem;
  font-weight: 700;
}

.git-tool-action:hover,
.git-tool-icon-button:hover {
  border-color: #b9ccda;
  background: #f7f9fc;
}

.git-tool-action:disabled,
.git-tool-icon-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.git-tool-action-primary {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #ffffff;
}

.git-tool-action-primary:hover {
  border-color: var(--color-primary);
  background: var(--color-primary);
}

.git-tool {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 10px;
  overflow: hidden;

  .git-tool-loading {
    display: flex;
    min-height: 180px;
    flex: 1;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 8px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    color: var(--color-text-muted);

    .git-tool-loading-icon {
      color: var(--color-primary);
      animation: git-tool-loading-spin 0.9s linear infinite;
    }

    .git-tool-loading-title {
      color: var(--color-text);
      font-size: 0.94rem;
    }

    .git-tool-loading-desc {
      max-width: 420px;
      overflow: hidden;
      font-size: 0.76rem;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }
}

@keyframes git-tool-loading-spin {
  from {
    transform: rotate(0deg);
  }

  to {
    transform: rotate(360deg);
  }
}

.git-tool-top {
  display: flex;
  flex: none;
  flex-direction: column;
  gap: 8px;
  padding: 0 0 10px;
  border-bottom: 1px solid var(--color-line);
}

.git-tool-command-row {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  min-height: 42px;
}

.git-tool-picker {
  display: flex;
  width: 280px;
  flex: 0 0 280px;
  flex-direction: column;
  gap: 5px;
}

.git-tool-label {
  color: var(--color-text-muted);
  font-size: 0.72rem;
  font-weight: 700;
}

.git-tool-repo-path {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  padding: 0 8px 7px;
  color: var(--color-text-muted);
  font-size: 0.76rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-select,
.git-tool-mini-select {
  min-width: 0;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: #ffffff;
  color: var(--color-text);
  font-size: 0.82rem;
}

.git-tool-tabs {
  display: flex;
  flex: none;
  align-items: center;
  gap: 3px;
  height: 34px;
  padding: 3px;
  border: 1px solid #e4edf5;
  border-radius: 17px;
  background: #f5f8fb;

  .git-tool-tab {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 68px;
    height: 28px;
    padding: 0 13px;
    border: 0;
    border-radius: 14px;
    background: transparent;
    color: #2c4667;
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 800;
  }

  .git-tool-tab-active {
    background: #ffffff;
    color: #11395f;
    box-shadow: 0 1px 4px rgba(47, 95, 145, 0.12);
  }
}

.git-tool-empty {
  display: flex;
  min-height: 180px;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  gap: 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #ffffff;
  color: var(--color-text-muted);
}

.git-tool-empty-title {
  color: var(--color-text);
  font-size: 1rem;
}

.git-tool-workbench {
  display: flex;
  min-height: 0;
  flex: 1;
  gap: 12px;
}

.git-tool-branch-panel,
.git-tool-commit-panel,
.git-tool-stash-panel {
  display: flex;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: var(--color-panel);
}

.git-tool-branch-panel {
  width: 280px;
  flex: 0 0 280px;
  background: #ffffff;
}

.git-tool-commit-panel {
  flex: 1;
  min-width: 0;
}

.git-tool-panel-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
  min-height: 56px;
  padding: 11px 12px;
  border-bottom: 1px solid var(--color-line);
  background: #ffffff;
}

.git-tool-panel-title {
  display: block;
  color: var(--color-text);
  font-size: 0.9rem;
}

.git-tool-panel-subtitle {
  display: block;
  max-width: 260px;
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: 0.74rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-panel-actions {
  display: flex;
  align-items: center;
  gap: 7px;
}

.git-tool-mini-select {
  width: 150px;
}

.git-tool-icon-button {
  width: 32px;
  padding: 0;
}

.git-tool-icon-danger {
  color: var(--color-danger);
}

.git-tool-archive-list,
.git-tool-stash-list,
.git-tool-commit-list,
.git-tool-drawer-commits,
.git-tool-drawer-archives {
  display: flex;
  min-height: 0;
  flex-direction: column;
  gap: 7px;
  overflow: auto;
  padding: 8px;
}

.git-tool-branch-head {
  display: flex;
  flex: none;
  flex-direction: column;
  gap: 8px;
  padding: 14px 12px 10px;
  background: #ffffff;
}

.git-tool-branch-title-row,
.git-tool-branch-summary-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.git-tool-branch-title-row strong {
  color: var(--color-text);
  font-size: 0.84rem;
}

.git-tool-branch-title-row span {
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-branch-path {
  display: block;
  overflow: hidden;
  color: #2f5f91;
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-branch-summary-row {
  color: #2f5f91;
  font-size: 0.76rem;
}

.git-tool-branch-toolbar {
  display: flex;
  flex: none;
  gap: 7px;
}

.git-tool-branch-toolbar-button {
  height: 30px;
  padding: 0 12px;
  border: 1px solid #c9dff2;
  border-radius: 7px;
  background: #eef6ff;
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.76rem;
  font-weight: 700;
}

.git-tool-branch-toolbar-button:disabled {
  cursor: not-allowed;
  opacity: 0.54;
}

.git-tool-branch-list {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 8px;
  overflow: auto;
  padding: 0 8px 10px;
}

.git-tool-branch-group {
  display: flex;
  flex-direction: column;
}

.git-tool-branch-group-head {
  display: flex;
  align-items: center;
  width: 100%;
  gap: 7px;
  height: 30px;
  padding: 0 9px;
  border: 1px solid #cfe0ef;
  border-radius: 7px;
  background: #eef5fb;
  color: #2f5f91;
  cursor: pointer;
  text-align: left;
}

.git-tool-branch-group-head:hover {
  border-color: #bad4ea;
  background: #e6f1fa;
}

.git-tool-branch-group-head strong {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  color: var(--color-text);
  font-size: 0.78rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-branch-group-head span {
  flex: none;
  color: var(--color-text-muted);
  font-size: 0.72rem;
}

.git-tool-branch-group-body {
  display: flex;
  flex-direction: column;
  border-left: 1px solid #cfe0ef;
  margin-left: 8px;
  padding-top: 4px;
}

.git-tool-branch {
  display: flex;
  min-height: 34px;
  align-items: center;
  gap: 7px;
  padding: 2px 0 2px 8px;
  color: var(--color-text);
}

.git-tool-branch-check {
  width: 14px;
  height: 14px;
  flex: none;
  margin: 0;
  accent-color: var(--color-primary);
}

.git-tool-branch-main {
  display: flex;
  min-width: 0;
  flex: 1;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 9px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--color-text);
  cursor: pointer;
  text-align: left;
}

.git-tool-branch-main:hover {
  background: #f2f7fb;
}

.git-tool-branch-active .git-tool-branch-main {
  background: #e9f4ff;
  color: var(--color-primary);
}

.git-tool-branch-current .git-tool-branch-main {
  background: #4d7fa8;
  color: #ffffff;
}

.git-tool-branch-current .git-tool-branch-main:hover {
  background: #4d7fa8;
}

.git-tool-branch-name,
.git-tool-archive-name,
.git-tool-stash-name,
.git-tool-commit-title,
.git-change-view-title {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-branch-name {
  min-width: 0;
  flex: 1;
  color: var(--color-primary);
  font-size: 0.78rem;
  font-weight: 700;
}

.git-tool-branch-current .git-tool-branch-name {
  color: #ffffff;
}

.git-tool-branch-badge {
  flex: none;
  padding: 2px 6px;
  border-radius: 999px;
  background: #ffe3a6;
  color: #7a4a00;
  font-size: 0.68rem;
  font-weight: 700;
}

.git-tool-commit-layout {
  display: flex;
  min-height: 0;
  flex: 1;
}

.git-tool-commit-list {
  width: 100%;
  flex: 1;
}

.git-tool-commit-table {
  gap: 0;
  padding: 0;
  background: #ffffff;
}

.git-tool-commit-table-head {
  position: sticky;
  top: 0;
  z-index: 2;
  display: grid;
  grid-template-columns: 64px minmax(320px, 1fr) 160px 110px 100px;
  flex: none;
  align-items: center;
  min-width: 850px;
  height: 30px;
  border-bottom: 1px solid var(--color-line);
  background: #edf3f8;
  color: #3d5874;
  font-size: 0.72rem;
  font-weight: 700;
}

.git-tool-commit-table-head-cell {
  min-width: 0;
  overflow: hidden;
  padding: 0 10px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-commit-table .git-tool-commit {
  display: grid;
  grid-template-columns: 64px minmax(320px, 1fr) 160px 110px 100px;
  min-width: 850px;
  min-height: 32px;
  gap: 0;
  padding: 0;
  border: 0;
  border-bottom: 1px solid var(--color-line);
  border-radius: 0;
  background: #ffffff;
}

.git-tool-commit-table .git-tool-commit:hover {
  background: #f7fbff;
}

.git-tool-commit-table .git-tool-commit-active {
  background: #e8f2fb;
}

.git-tool-commit-table .git-tool-commit-graph {
  color: var(--color-text-soft);
}

.git-tool-commit-graph-cell {
  position: relative;
  display: block;
  min-width: 0;
  height: 100%;
  overflow: visible;
  padding: 0;
}

.git-tool-commit-graph-svg {
  display: block;
  width: 48px;
  height: calc(100% + 2px);
  margin-top: -1px;
  overflow: visible;
}

.git-tool-commit-graph-line {
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 2;
  vector-effect: non-scaling-stroke;
}

.git-tool-commit-graph-node {
  stroke: #ffffff;
  stroke-width: 2;
  vector-effect: non-scaling-stroke;
}

.git-tool-commit-description,
.git-tool-commit-date,
.git-tool-commit-author,
.git-tool-commit-hash {
  display: flex;
  min-width: 0;
  align-items: center;
  height: 100%;
  overflow: hidden;
  padding: 0 10px;
  color: #11395f;
  font-size: 0.78rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-commit-description {
  gap: 7px;
  color: var(--color-text);
}

.git-tool-commit-hash {
  color: var(--color-primary);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: 0.74rem;
}

.git-tool-commit {
  display: flex;
  min-height: 52px;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: #f8fafc;
  color: var(--color-text);
  cursor: pointer;
  text-align: left;
}

.git-tool-commit:disabled {
  cursor: default;
}

.git-tool-commit:hover {
  border-color: #cbddec;
  background: #ffffff;
}

.git-tool-commit-active {
  border-color: #8eb6d9;
  background: #eef6ff;
}

.git-tool-commit-graph {
  min-height: 28px;
  color: var(--color-text-soft);
}

.git-tool-commit-main {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 3px;
}

.git-tool-commit-title {
  display: block;
  color: var(--color-text);
  font-size: 0.82rem;
}

.git-tool-commit-meta,
.git-tool-archive-meta,
.git-tool-stash-meta,
.git-change-view-meta {
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-check {
  flex: none;
  padding: 2px 6px;
  border-radius: 999px;
  font-size: 0.68rem;
  font-weight: 700;
}

.git-tool-check-exists-hash {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.git-tool-check-exists-subject {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.git-tool-check-missing {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.git-tool-context-menu {
  position: fixed;
  z-index: 90;
  display: flex;
  min-width: 166px;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
  border: 1px solid #cbddec;
  border-radius: 7px;
  background: #ffffff;
  box-shadow: 0 10px 28px rgba(15, 23, 42, 0.16);
}

.git-tool-context-menu-button {
  display: flex;
  align-items: center;
  gap: 9px;
  height: 32px;
  padding: 0 8px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: #355b7e;
  cursor: pointer;
  font-size: 0.8rem;
  text-align: left;
}

.git-tool-context-menu-button:hover {
  background: #eef6ff;
  color: var(--color-primary);
}

.git-tool-drawer-detail {
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.git-tool-detail-empty,
.git-tool-list-empty {
  display: flex;
  min-height: 110px;
  align-items: center;
  justify-content: center;
  border: 1px dashed var(--color-line);
  border-radius: 8px;
  color: var(--color-text-muted);
  font-size: 0.84rem;
  font-weight: 700;
}

.git-tool-archive,
.git-tool-stash {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: #f8fafc;
}

.git-tool-archive-main,
.git-tool-stash-main {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--color-text);
  cursor: pointer;
  text-align: left;
}

.git-tool-archive-path {
  width: 100%;
  overflow: hidden;
  color: var(--color-text-soft);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-archive-actions,
.git-tool-stash-actions {
  display: flex;
  flex: none;
  gap: 6px;
}

.git-tool-stash-workbench {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  gap: 12px;

  .git-tool-stash-panel {
    flex: 1;
    min-width: 0;
  }

  .git-tool-stash-summary-row {
    display: flex;
    min-height: 42px;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-line);
    background: #f8fafc;
    color: #2f5f91;
    font-size: 0.76rem;
  }

  .git-tool-stash-toolbar {
    display: flex;
    flex: none;
    gap: 7px;
  }

  .git-tool-stash-list {
    flex: 1;
  }

  .git-tool-stash-check {
    width: 14px;
    height: 14px;
    flex: none;
    margin: 0;
    accent-color: var(--color-primary);
  }
}

.git-tool-drawer {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: flex;
  justify-content: flex-end;
  background: rgba(15, 23, 42, 0.18);
}

.git-tool-drawer-panel {
  display: flex;
  height: 100%;
  flex-direction: column;
  border-left: 1px solid var(--color-line);
  background: var(--color-panel);
  box-shadow: -16px 0 38px rgba(15, 23, 42, 0.16);
}

.git-tool-drawer-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 14px;
  padding: 14px;
  border-bottom: 1px solid var(--color-line);
  background: #f8fafc;
}

.git-tool-drawer-title {
  display: block;
  margin-top: 3px;
  color: var(--color-text);
  font-size: 0.96rem;
}

.git-tool-drawer-back {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 24px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.76rem;
  font-weight: 700;
  line-height: 1;

  .git-tool-drawer-back-icon {
    display: block;
    flex: none;
    transform: rotate(180deg);
  }

  .git-tool-drawer-back-text {
    display: inline-flex;
    align-items: center;
    height: 24px;
  }
}

.git-tool-drawer-back:hover {
  color: #1f4f7e;
}

.git-tool-drawer-actions {
  display: flex;
  flex: none;
  align-items: center;
  gap: 8px;
}

.git-tool-drawer-hash {
  height: 28px;
  padding: 0 10px;
  border: 1px solid #c9dff2;
  border-radius: 7px;
  background: #eef6ff;
  color: var(--color-primary);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: 0.72rem;
  line-height: 28px;
}

.git-tool-drawer-body {
  display: flex;
  min-height: 0;
  flex: 1;
}

.git-tool-archive-detail {
  flex-direction: column;
  background: #ffffff;
}

.git-tool-drawer-body-detail {
  display: block;
}

.git-tool-drawer-commits {
  width: 300px;
  flex: 0 0 300px;
  border-right: 1px solid var(--color-line);
}

.git-tool-drawer-archives {
  width: 520px;
  min-height: 0;
  flex: 0 0 520px;
  gap: 8px;
  overflow: auto;
  padding: 10px;
  background: #f8fafc;

  .git-tool-archive-tools {
    display: flex;
    min-height: 38px;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 0 2px 2px;
    color: #2f5f91;
    font-size: 0.76rem;
  }

  .git-tool-archive-toolbar {
    display: flex;
    flex: none;
    gap: 7px;
  }

  .git-tool-archive-check {
    width: 14px;
    height: 14px;
    flex: none;
    margin: 0;
    accent-color: var(--color-primary);
  }

  .git-tool-archive-group {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .git-tool-archive-group-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .git-tool-archive-group-toggle {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 9px;
    border: 1px solid #cfe0ef;
    border-radius: 7px;
    background: #eef5fb;
    color: #2f5f91;
    cursor: pointer;
    text-align: left;
  }

  .git-tool-archive-group-toggle:hover {
    border-color: #bad4ea;
    background: #e6f1fa;
  }

  .git-tool-archive-group-toggle strong {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.78rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-tool-archive-group-toggle span {
    flex: none;
    color: var(--color-text-muted);
    font-size: 0.72rem;
  }

  .git-tool-archive-group-body {
    display: flex;
    flex-direction: column;
    gap: 7px;
    border-left: 1px solid #cfe0ef;
    margin-left: 8px;
    padding-left: 8px;
  }

  .git-tool-archive {
    min-height: 66px;
    padding: 10px 11px;
    border-color: #d9e5ee;
    background: #ffffff;
  }

  .git-tool-archive:hover {
    border-color: #bfd5e8;
    background: #f7fbff;
  }

  .git-tool-archive-main {
    gap: 5px;
  }

  .git-tool-archive-name {
    font-size: 0.9rem;
  }

  .git-tool-archive-meta {
    display: flex;
    align-items: center;
    gap: 8px;

    code {
      padding: 2px 6px;
      border-radius: 5px;
      background: #eef6ff;
      color: var(--color-primary);
      font-family: "JetBrains Mono", "Consolas", monospace;
      font-size: 0.72rem;
      font-weight: 700;
    }
  }
}

.git-tool-archive-detail-meta {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 38px;
  padding: 0 16px;
  border-bottom: 1px solid var(--color-line);
  background: #eef5fb;
  color: #2f5f91;
  font-size: 0.76rem;
}

.git-tool-archive-detail-meta span,
.git-tool-archive-detail-meta strong {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-archive-detail-meta strong {
  flex: none;
  font-weight: 600;
}

.git-tool-archive-commit-table {
  display: flex;
  height: 260px;
  flex: 0 0 260px;
  flex-direction: column;
  min-height: 0;
  border-bottom: 1px solid var(--color-line);
}

.git-tool-archive-commit-head,
.git-tool-archive-commit {
  display: grid;
  grid-template-columns: minmax(360px, 1fr) 160px 110px 100px;
  min-width: 760px;
}

.git-tool-archive-commit-head {
  flex: none;
  align-items: center;
  height: 31px;
  border-bottom: 1px solid var(--color-line);
  background: #edf3f8;
  color: #3d5874;
  font-size: 0.72rem;
  font-weight: 700;
}

.git-tool-archive-commit-head span,
.git-tool-archive-commit span {
  min-width: 0;
  overflow: hidden;
  padding: 0 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-archive-commit-list {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: auto;
}

.git-tool-archive-commit {
  flex: none;
  align-items: center;
  min-height: 33px;
  border: 0;
  border-bottom: 1px solid var(--color-line);
  background: #ffffff;
  color: #11395f;
  cursor: pointer;
  font-size: 0.78rem;
  text-align: left;
}

.git-tool-archive-commit:hover {
  background: #f7fbff;
}

.git-tool-archive-commit-active {
  background: #e8f2fb;
}

.git-tool-archive-commit-title {
  color: var(--color-text);
  font-weight: 700;
}

.git-tool-archive-commit-hash {
  color: var(--color-primary);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: 0.74rem;
}

.git-tool-archive-detail-content {
  flex: 1;
  min-height: 0;
  padding: 12px;
  background: #f8fafc;
}

.git-tool-archive-detail-content .git-change-view {
  overflow: hidden;
  border: 1px solid #cbddec;
  border-radius: 7px;
  background: #ffffff;
}

.git-change-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
}

.git-change-view-head {
  flex: none;
  padding: 10px 12px;
  border-bottom: 1px solid var(--color-line);
  background: #ffffff;
}

.git-change-view-summary {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.git-change-view-title {
  display: block;
  color: var(--color-text);
  font-size: 0.86rem;
}

.git-change-view-meta-row {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 8px;
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: 0.72rem;
}

.git-change-view-meta-row span {
  display: inline-flex;
  min-width: 0;
  align-items: center;
  gap: 5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-change-view-meta-row small {
  flex: none;
  color: var(--color-text-soft);
  font-size: 0.68rem;
  font-weight: 700;
}

.git-change-view-meta-row code,
.git-change-view-meta-row strong {
  min-width: 0;
  overflow: hidden;
  color: var(--color-text-muted);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: 0.72rem;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-change-view-body {
  display: flex;
  min-height: 0;
  flex: 1;
}

.git-change-view-tree {
  display: flex;
  width: 300px;
  flex: 0 0 300px;
  min-height: 0;
  flex-direction: column;
  border-right: 1px solid var(--color-line);
  background: #f8fafc;
}

.git-change-view-tree-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  height: 32px;
  padding: 0 9px;
  border-bottom: 1px solid var(--color-line);
  color: var(--color-text-muted);
  font-size: 0.74rem;
}

.git-change-view-tree-head strong {
  color: var(--color-primary);
  font-size: 0.72rem;
}

.git-change-view-tree-body {
  min-height: 0;
  flex: 1;
  overflow: auto;
  padding: 4px 0;
}

.git-change-view-tree-node {
  display: flex;
  flex-direction: column;
}

.git-change-view-tree-directory,
.git-change-view-tree-file {
  display: flex;
  align-items: center;
  gap: 5px;
  min-height: 26px;
  padding: 0 8px;
  border: 0;
  border-left: 2px solid transparent;
  background: transparent;
  color: var(--color-text);
  text-align: left;
}

.git-change-view-tree-directory {
  width: 100%;
  cursor: pointer;
  color: var(--color-primary);
  font-size: 0.76rem;
  font-weight: 700;
}

.git-change-view-tree-directory:hover {
  background: #edf3f8;
}

.git-change-view-tree-file {
  width: 100%;
  cursor: pointer;
}

.git-change-view-tree-file:hover {
  background: #edf3f8;
}

.git-change-view-tree-file-active {
  border-left-color: var(--color-primary);
  background: #e9f4ff;
}

.git-change-view-tree-caret,
.git-change-view-tree-folder {
  flex: none;
  color: #6d85a5;
}

.git-change-view-tree-directory strong {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-change-view-tree-directory small {
  display: inline-flex;
  min-width: 18px;
  height: 18px;
  align-items: center;
  justify-content: center;
  border-radius: 999px;
  background: #e8eef5;
  color: #506b91;
  font-size: 0.68rem;
}

.git-change-view-file-status {
  display: inline-flex;
  width: 18px;
  height: 18px;
  flex: 0 0 18px;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  font-size: 0.64rem;
  font-weight: 700;
}

.git-change-view-file-status-add {
  background: var(--color-success-soft);
  color: var(--color-success);
}

.git-change-view-file-status-modify,
.git-change-view-file-status-move,
.git-change-view-file-status-copy {
  background: var(--color-warning-soft);
  color: var(--color-warning);
}

.git-change-view-file-status-delete {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.git-change-view-file-icon {
  flex: none;
  color: var(--color-text-muted);
}

.git-change-view-file-path {
  min-width: 0;
  overflow: hidden;
  color: var(--color-text);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-change-view-diff-panel {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
}

.git-change-view-file-head {
  flex: none;
  height: 32px;
  padding: 0 10px;
  border-bottom: 1px solid var(--color-line);
  background: #f8fafc;
}

.git-change-view-file-head strong {
  display: block;
  overflow: hidden;
  color: var(--color-primary);
  font-size: 0.76rem;
  line-height: 32px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-change-view-diff {
  display: block;
  flex: 1;
  min-width: 0;
  min-height: 0;
  margin: 0;
  overflow: auto;
  background: #ffffff;
  color: var(--color-text);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: 0.7rem;
  line-height: 1.45;
}

.git-change-view-line {
  display: block;
  min-height: 19px;
  padding: 1px 10px;
  white-space: pre-wrap;
  word-break: break-all;
}

.git-change-view-line-add {
  background: #effaf2;
  color: #176238;
}

.git-change-view-line-delete {
  background: #fff0ee;
  color: #9f241b;
}

.git-change-view-line-chunk {
  background: #eef5fb;
  color: #2f5f91;
  font-weight: 700;
}

.git-change-view-line-meta {
  background: #f4f6f8;
  color: #637386;
}

.git-tool {
  :deep(.git-change-view) {
    display: flex;
    height: 100%;
    min-height: 0;
    flex-direction: column;
  }

  :deep(.git-change-view-head) {
    flex: none;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-line);
    background: #ffffff;
  }

  :deep(.git-change-view-summary) {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  :deep(.git-change-view .git-tool-label) {
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  :deep(.git-change-view-title) {
    display: block;
    max-width: 100%;
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.86rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :deep(.git-change-view-meta-row) {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.72rem;
  }

  :deep(.git-change-view-meta-row span) {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    gap: 5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :deep(.git-change-view-meta-row small) {
    flex: none;
    color: var(--color-text-soft);
    font-size: 0.68rem;
    font-weight: 700;
  }

  :deep(.git-change-view-meta-row code),
  :deep(.git-change-view-meta-row strong) {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-muted);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.72rem;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :deep(.git-change-view-body) {
    display: flex;
    min-height: 0;
    flex: 1;
  }

  :deep(.git-change-view-tree) {
    display: flex;
    width: 300px;
    flex: 0 0 300px;
    min-height: 0;
    flex-direction: column;
    border-right: 1px solid var(--color-line);
    background: #f8fafc;
  }

  :deep(.git-change-view-tree-head) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 32px;
    padding: 0 9px;
    border-bottom: 1px solid var(--color-line);
    color: var(--color-text-muted);
    font-size: 0.74rem;
  }

  :deep(.git-change-view-tree-head strong) {
    color: var(--color-primary);
    font-size: 0.72rem;
  }

  :deep(.git-change-view-tree-body) {
    min-height: 0;
    flex: 1;
    overflow: auto;
    padding: 4px 0;
  }

  :deep(.git-change-view-tree-node) {
    display: flex;
    flex-direction: column;
  }

  :deep(.git-change-view-tree-directory),
  :deep(.git-change-view-tree-file) {
    display: flex;
    align-items: center;
    gap: 5px;
    min-height: 26px;
    padding: 0 8px;
    border: 0;
    border-left: 2px solid transparent;
    background: transparent;
    color: var(--color-text);
    text-align: left;
  }

  :deep(.git-change-view-tree-directory) {
    width: 100%;
    cursor: pointer;
    color: var(--color-primary);
    font-size: 0.76rem;
    font-weight: 700;
  }

  :deep(.git-change-view-tree-directory:hover) {
    background: #edf3f8;
  }

  :deep(.git-change-view-tree-file) {
    width: 100%;
    cursor: pointer;
  }

  :deep(.git-change-view-tree-file:hover) {
    background: #edf3f8;
  }

  :deep(.git-change-view-tree-file-active) {
    border-left-color: var(--color-primary);
    background: #e9f4ff;
  }

  :deep(.git-change-view-tree-caret),
  :deep(.git-change-view-tree-folder) {
    flex: none;
    color: #6d85a5;
  }

  :deep(.git-change-view-tree-directory strong) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :deep(.git-change-view-tree-directory small) {
    display: inline-flex;
    min-width: 18px;
    height: 18px;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: #e8eef5;
    color: #506b91;
    font-size: 0.68rem;
  }

  :deep(.git-change-view-file-status) {
    display: inline-flex;
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    font-size: 0.64rem;
    font-weight: 700;
  }

  :deep(.git-change-view-file-status-add) {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  :deep(.git-change-view-file-status-modify),
  :deep(.git-change-view-file-status-move),
  :deep(.git-change-view-file-status-copy) {
    background: var(--color-warning-soft);
    color: var(--color-warning);
  }

  :deep(.git-change-view-file-status-delete) {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  :deep(.git-change-view-file-icon) {
    flex: none;
    color: var(--color-text-muted);
  }

  :deep(.git-change-view-file-path) {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :deep(.git-change-view-diff-panel) {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    flex-direction: column;
  }

  :deep(.git-change-view-file-head) {
    flex: none;
    height: 32px;
    padding: 0 10px;
    border-bottom: 1px solid var(--color-line);
    background: #f8fafc;
  }

  :deep(.git-change-view-file-head strong) {
    display: block;
    overflow: hidden;
    color: var(--color-primary);
    font-size: 0.76rem;
    line-height: 32px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :deep(.git-change-view-diff) {
    display: block;
    flex: 1;
    min-width: 0;
    min-height: 0;
    margin: 0;
    overflow: auto;
    background: #ffffff;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.7rem;
    line-height: 1.45;
  }

  :deep(.git-change-view-line) {
    display: block;
    min-height: 19px;
    padding: 1px 10px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  :deep(.git-change-view-line-add) {
    background: #effaf2;
    color: #176238;
  }

  :deep(.git-change-view-line-delete) {
    background: #fff0ee;
    color: #9f241b;
  }

  :deep(.git-change-view-line-chunk) {
    background: #eef5fb;
    color: #2f5f91;
    font-weight: 700;
  }

  :deep(.git-change-view-line-meta) {
    background: #f4f6f8;
    color: #637386;
  }

  :deep(.git-change-view .git-tool-detail-empty),
  :deep(.git-change-view .git-tool-list-empty) {
    display: flex;
    min-height: 110px;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 0.84rem;
    font-weight: 700;
  }
}
</style>

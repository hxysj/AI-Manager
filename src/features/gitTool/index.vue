<template>
  <section class="git-tool" @click="closeCommitContextMenu">
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
            @click="gitWorkspace = 'branch'"
          >
            <GitBranchIcon :size="15" />
            分支
          </button>
          <button
            :class="[
              'git-tool-tab',
              { 'git-tool-tab-active': gitWorkspace === 'stash' }
            ]"
            type="button"
            @click="gitWorkspace = 'stash'"
          >
            <Archive :size="15" />
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

      <div class="git-tool-status-list">
        <div class="git-tool-status">
          <span class="git-tool-label">当前分支</span>
          <strong class="git-tool-status-value">{{
            currentBranch || "-"
          }}</strong>
        </div>
        <div class="git-tool-status">
          <span class="git-tool-label">本地分支</span>
          <strong class="git-tool-status-value">{{ branches.length }}</strong>
        </div>
        <div class="git-tool-status">
          <span class="git-tool-label">Stash</span>
          <strong class="git-tool-status-value">{{ stashes.length }}</strong>
        </div>
        <div class="git-tool-status">
          <span class="git-tool-label">归档</span>
          <strong class="git-tool-status-value">{{
            archives.length + stashArchives.length
          }}</strong>
        </div>
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
              已选 {{ selectedBranchNames.length }}/{{ archivableBranches.length }}
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
            <div class="git-tool-branch-group-head">
              <ChevronDown :size="13" />
              <strong>{{ group.label }}</strong>
              <span>{{ group.branches.length }}</span>
            </div>
            <div class="git-tool-branch-group-body">
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
                  <span class="git-tool-branch-name">{{ branch.name }}</span>
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
                <span
                  v-if="!commit.isGraphOnly"
                  class="git-tool-commit-graph-line"
                ></span>
                <span
                  v-if="!commit.isGraphOnly"
                  class="git-tool-commit-graph-dot"
                ></span>
                <span class="git-tool-commit-graph-text">{{
                  commit.isGraphOnly ? commit.graph : ""
                }}</span>
              </span>
              <span class="git-tool-commit-description">
                <strong class="git-tool-commit-title">{{
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
            <div v-if="!commits.length" class="git-tool-list-empty">
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
          <button
            class="git-tool-action"
            type="button"
            :disabled="!selectedRepo"
            @click="loadStashes"
          >
            <RefreshCw :size="14" />
            刷新
          </button>
        </div>

        <div class="git-tool-stash-list">
          <article
            v-for="stash in stashes"
            :key="stash.hash"
            class="git-tool-stash"
          >
            <button
              class="git-tool-stash-main"
              type="button"
              @click="openStashDetail(stash)"
            >
              <strong class="git-tool-stash-name">
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
              <strong class="git-tool-stash-name">
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
        </div>
      </section>
    </section>

    <section
      v-if="detailDrawerVisible"
      class="git-tool-drawer"
      @click="closeDetailDrawer"
    >
      <div class="git-tool-drawer-panel" @click.stop>
        <header class="git-tool-drawer-head">
          <div>
            <span class="git-tool-label">{{ detailDrawerEyebrow }}</span>
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
            <article
              v-for="archive in archives"
              :key="archive.archiveId"
              class="git-tool-archive"
            >
              <button
                class="git-tool-archive-main"
                type="button"
                @click="openArchiveDetail(archive)"
              >
                <strong class="git-tool-archive-name">{{
                  archive.branchName
                }}</strong>
                <span class="git-tool-archive-meta">
                  {{ formatHash(archive.commitHash) }} ·
                  {{ formatDate(archive.archivedAt) }}
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
                <span>{{ commit.isGraphOnly ? "" : formatFullDate(commit.date) }}</span>
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
  </section>
</template>

<script setup>
import { computed, defineComponent, h, ref, watch } from "vue"
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
import { createMessage } from "@/utils/message"

const props = defineProps({
  repos: {
    type: Array,
    required: true
  }
})

defineEmits(["add-repo"])

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
const project = ref(null)
const branches = ref([])
const commits = ref([])
const archives = ref([])
const stashes = ref([])
const stashArchives = ref([])
const currentBranch = ref("")
const selectedBranch = ref("")
const selectedBranchNames = ref([])
const selectedCommit = ref(null)
const selectedCommitDetail = ref(null)
const selectedArchive = ref(null)
const archiveCommits = ref([])
const selectedArchiveCommit = ref(null)
const selectedArchiveCommitDetail = ref(null)
const selectedStash = ref(null)
const selectedStashArchive = ref(null)
const stashDetail = ref(null)
const commitContextMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  commit: null
})

const selectedRepo = computed(() => {
  return props.repos.find((item) => item.id === selectedRepoId.value) || null
})

const archivableBranches = computed(() => {
  return branches.value.filter((item) => !item.isCurrent)
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
  () => props.repos,
  () => {
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
      refreshGitProject()
    }
  },
  { immediate: true }
)

function handleRepoChange(event) {
  selectedRepoId.value = event.target.value
  refreshGitProject()
}

async function refreshGitProject() {
  if (!selectedRepoId.value) {
    return
  }

  gitLoading.value = true

  try {
    const result = await window.aiManager.scanGitToolBranches({
      repoId: selectedRepoId.value
    })

    project.value = result.project || null
    branches.value = result.branches || []
    currentBranch.value = result.currentBranch || ""
    archives.value = result.archives || []
    stashes.value = result.stashes || []
    stashArchives.value = result.stashArchives || []
    selectedBranchNames.value = selectedBranchNames.value.filter((branchName) =>
      branches.value.find((item) => item.name === branchName && !item.isCurrent)
    )

    if (!branches.value.find((item) => item.name === selectedBranch.value)) {
      selectedBranch.value =
        branches.value.find((item) => item.name === currentBranch.value)
          ?.name ||
        branches.value[0]?.name ||
        ""
    }

    if (selectedBranch.value) {
      await loadCommits()
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

async function loadCommits() {
  closeCommitContextMenu()

  if (!selectedRepoId.value || !selectedBranch.value) {
    commits.value = []
    return
  }

  commitsLoading.value = true

  try {
    commits.value = await window.aiManager.listGitToolCommits({
      repoId: selectedRepoId.value,
      branchName: selectedBranch.value
    })

    selectedCommit.value = null
    selectedCommitDetail.value = null
  } catch (error) {
    showErrorMessage(error)
  } finally {
    commitsLoading.value = false
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

  commitContextMenu.value = {
    visible: true,
    x: event.clientX,
    y: event.clientY,
    commit
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
    const result = await window.aiManager.checkGitToolCommitOnBranch({
      repoId: selectedRepoId.value,
      sourceBranchName: selectedBranch.value,
      targetBranchName: project.value.checkBranchName,
      commitHash: commit.hash,
      subject: commit.subject
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

async function loadCommitDetail(commitHash, filePath) {
  if (!selectedRepoId.value || !commitHash) {
    return
  }

  commitDetailLoading.value = true

  try {
    selectedCommitDetail.value = await window.aiManager.getGitToolCommitDetail({
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

async function archiveSelectedBranches() {
  if (!selectedRepoId.value || !selectedBranchNames.value.length) {
    return
  }

  const branchNames = [...selectedBranchNames.value]

  if (
    !window.confirm(
      `归档成功后，${branchNames.length} 个本地分支会被删除，是否继续？`
    )
  ) {
    return
  }

  try {
    for (const branchName of branchNames) {
      await window.aiManager.archiveGitToolBranch({
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
  }
}

async function handleCheckBranchChange(event) {
  if (!selectedRepoId.value) {
    return
  }

  try {
    project.value = await window.aiManager.updateGitToolCheckBranch({
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
    await window.aiManager.clearGitToolCommitCheckCache({
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
    archiveCommits.value = await window.aiManager.listGitToolArchiveCommits({
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
      await window.aiManager.getGitToolArchiveCommitDetail({
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
  const targetBranchName = window.prompt(
    "请输入恢复后的分支名",
    archive.branchName
  )

  if (!targetBranchName) {
    return
  }

  try {
    await window.aiManager.restoreGitToolArchive({
      archiveId: archive.archiveId,
      targetBranchName: targetBranchName.trim()
    })
    createMessage.success("分支已恢复。")
    closeDetailDrawer()
    selectedBranch.value = targetBranchName.trim()
    await refreshGitProject()
  } catch (error) {
    showErrorMessage(error)
  }
}

async function deleteArchive(archive) {
  if (!window.confirm(`确认删除归档「${archive.branchName}」吗？`)) {
    return
  }

  try {
    archives.value = await window.aiManager.deleteGitToolArchive({
      archiveId: archive.archiveId
    })
    createMessage.success("归档已删除。")
  } catch (error) {
    showErrorMessage(error)
  }
}

async function loadStashes() {
  if (!selectedRepoId.value) {
    return
  }

  try {
    stashes.value = await window.aiManager.listGitToolStashes({
      repoId: selectedRepoId.value
    })
    stashArchives.value = await window.aiManager.listGitToolStashArchives({
      repoId: selectedRepoId.value
    })
  } catch (error) {
    showErrorMessage(error)
  }
}

async function openStashDetail(stash) {
  detailDrawerType.value = "stash"
  selectedStash.value = stash
  selectedStashArchive.value = null
  stashDetail.value = null
  stashDetailLoading.value = true

  try {
    stashDetail.value = await window.aiManager.getGitToolStashDetail({
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
    stashDetail.value = await window.aiManager.getGitToolStashArchiveDetail({
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
    stashDetail.value = await window.aiManager.getGitToolStashDetail({
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
    stashDetail.value = await window.aiManager.getGitToolStashArchiveDetail({
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
    !window.confirm(
      `归档成功后，项目中的「${stash.stashRef}」会从 stash list 中删除，是否继续？`
    )
  ) {
    return
  }

  try {
    await window.aiManager.archiveGitToolStash({
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
  }
}

async function restoreStashArchive(archive) {
  if (!window.confirm(`确认恢复「${archive.stashRef}」到 stash list 吗？`)) {
    return
  }

  try {
    await window.aiManager.restoreGitToolStashArchive({
      stashArchiveId: archive.stashArchiveId
    })
    createMessage.success("stash 已恢复。")
    stashDetail.value = null
    selectedStashArchive.value = null
    await loadStashes()
  } catch (error) {
    showErrorMessage(error)
  }
}

async function deleteStashArchive(archive) {
  if (!window.confirm(`确认删除 stash 归档「${archive.stashRef}」吗？`)) {
    return
  }

  try {
    stashArchives.value = await window.aiManager.deleteGitToolStashArchive({
      stashArchiveId: archive.stashArchiveId
    })
    createMessage.success("stash 归档已删除。")
  } catch (error) {
    showErrorMessage(error)
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

.git-tool-status-list {
  display: flex;
  min-width: 0;
  gap: 0;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  overflow: hidden;
}

.git-tool-status {
  display: flex;
  min-width: 0;
  flex: 1;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 34px;
  padding: 0 11px;
  border-right: 1px solid var(--color-line);
  background: #ffffff;
}

.git-tool-status:last-child {
  border-right: 0;
}

.git-tool-status-value {
  overflow: hidden;
  color: var(--color-primary);
  font-size: 0.88rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-tabs {
  display: flex;
  flex: none;
  gap: 6px;
}

.git-tool-tab {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: #ffffff;
  color: var(--color-text-muted);
  cursor: pointer;
  font-size: 0.82rem;
  font-weight: 700;
}

.git-tool-tab-active {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #ffffff;
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
  gap: 7px;
  height: 30px;
  padding: 0 9px;
  border: 1px solid #cfe0ef;
  border-radius: 7px;
  background: #eef5fb;
  color: #2f5f91;
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
  display: flex;
  min-width: 0;
  height: 100%;
  align-items: center;
  padding: 0 8px;
}

.git-tool-commit-graph-line {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 14px;
  width: 1px;
  background: #e15675;
}

.git-tool-commit-graph-dot {
  position: relative;
  z-index: 1;
  width: 7px;
  height: 7px;
  flex: 0 0 7px;
  border-radius: 999px;
  background: #e15675;
  box-shadow: 0 0 0 2px #ffffff;
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

.git-tool-commit-graph-text {
  flex: none;
  color: var(--color-text-soft);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: 0.75rem;
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

.git-tool-archive-actions,
.git-tool-stash-actions {
  display: flex;
  flex: none;
  gap: 6px;
}

.git-tool-stash-workbench {
  display: flex;
  min-height: 0;
  flex: 1;
  gap: 12px;
}

.git-tool-stash-panel {
  width: 330px;
  flex: 0 0 330px;
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
  width: 1040px;
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
  flex: 1;
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

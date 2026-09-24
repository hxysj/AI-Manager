<template>
  <section class="git-tool" @click="closeContextMenus">
    <section class="git-tool-top">
      <div class="git-tool-top-card">
        <span class="git-tool-project-label">项目</span>
        <div class="git-tool-command-row">
          <div class="git-tool-picker-box">
            <FolderGit2 :size="16" class="git-tool-picker-icon" />
            <span class="git-tool-picker-name">{{
              selectedRepo?.name || "请选择项目"
            }}</span>
            <ChevronDown :size="15" class="git-tool-picker-arrow" />
            <select
              class="git-tool-select-native"
              :value="selectedRepoId"
              :disabled="repos.length === 0"
              @change="handleRepoChange"
            >
              <option value="" disabled>请选择项目</option>
              <option v-for="repo in repos" :key="repo.id" :value="repo.id">
                {{ repo.name }}
              </option>
            </select>
          </div>

          <div class="git-tool-repo-path-box">
            <Folder :size="15" class="git-tool-path-icon" />
            <span
              class="git-tool-repo-path-text"
              :title="selectedRepo?.localPath || ''"
            >
              {{ selectedRepo?.localPath || "未选择项目" }}
            </span>
            <button
              v-if="selectedRepo?.localPath"
              class="git-tool-path-copy"
              type="button"
              title="复制路径"
              @click.stop="copyRepoPath"
            >
              <Copy :size="14" />
            </button>
          </div>

          <div class="git-tool-tabs">
            <button
              :class="[
                'git-tool-tab',
                { 'git-tool-tab-active': gitWorkspace === 'branch' }
              ]"
              type="button"
              @click="selectGitWorkspace('branch')"
            >
              <GitBranchIcon :size="14" />
              <span>分支</span>
            </button>
            <button
              :class="[
                'git-tool-tab',
                { 'git-tool-tab-active': gitWorkspace === 'stash' }
              ]"
              type="button"
              @click="selectGitWorkspace('stash')"
            >
              <Database :size="14" />
              <span>Stash</span>
            </button>
          </div>

          <button
            class="git-tool-action git-tool-action-primary"
            type="button"
            :disabled="gitLoading || !selectedRepo"
            @click="refreshGitProject"
          >
            <RefreshCw :size="14" />
            <span>刷新</span>
          </button>
          <button
            class="git-tool-action git-tool-action-outline"
            type="button"
            @click="$emit('add-repo')"
          >
            <Plus :size="14" />
            <span>添加项目</span>
          </button>
        </div>
      </div>
    </section>

    <section v-if="!repos.length" class="git-tool-empty">
      <span data-emphasis class="git-tool-empty-title">当前没有项目</span>
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
      <span data-emphasis class="git-tool-loading-title"
        >正在加载 Git 数据</span
      >
      <span class="git-tool-loading-desc">{{
        selectedRepo?.name || "当前项目"
      }}</span>
    </section>

    <section v-else-if="gitWorkspace === 'branch'" class="git-tool-workbench">
      <aside class="git-tool-branch-panel">
        <div class="git-tool-branch-head">
          <div class="git-tool-branch-title-row">
            <div class="git-tool-branch-title-wrap">
              <span data-emphasis class="git-tool-branch-title">本地分支</span>
              <span class="git-tool-branch-count-badge">{{
                branches.length
              }}</span>
            </div>
          </div>
          <div class="git-tool-branch-search">
            <Search :size="13" class="git-tool-branch-search-icon" />
            <input
              v-model="branchSearchKeyword"
              type="text"
              class="git-tool-branch-search-input"
              placeholder="搜索分支..."
            />
            <button
              v-if="branchSearchKeyword"
              class="git-tool-branch-search-clear"
              type="button"
              title="清空"
              @click="branchSearchKeyword = ''"
            >
              <X :size="12" />
            </button>
          </div>
          <div class="git-tool-branch-action-row">
            <label class="git-tool-branch-select-all">
              <input
                type="checkbox"
                class="git-tool-branch-check"
                :checked="isAllArchivableBranchesSelected"
                :disabled="!archivableBranches.length"
                @change="toggleSelectAllBranches"
              />
              <span>全选</span>
            </label>
            <button
              class="git-tool-branch-archive-btn"
              type="button"
              :disabled="!selectedBranchNames.length"
              @click="archiveSelectedBranches"
            >
              <Archive :size="13" />
              <span>归档选中</span>
            </button>
          </div>
        </div>

        <div class="git-tool-branch-list">
          <section
            v-for="group in filteredBranchGroups"
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
              <span data-emphasis class="git-tool-branch-group-label">{{
                group.label
              }}</span>
              <span class="git-tool-branch-group-badge">{{
                group.branches.length
              }}</span>
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
                  <GitBranchIcon :size="14" class="git-tool-branch-icon" />
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
          <div v-if="!filteredBranchGroups.length" class="git-tool-list-empty">
            {{ branchSearchKeyword ? "未找到匹配的分支" : "暂无分支" }}
          </div>
        </div>
      </aside>

      <section class="git-tool-commit-panel">
        <div class="git-tool-panel-head">
          <div class="git-tool-panel-title-group">
            <GitBranchIcon :size="20" class="git-tool-panel-branch-icon" />
            <div class="git-tool-panel-title-text">
              <span data-emphasis class="git-tool-panel-title">{{
                selectedBranch || "提交记录"
              }}</span>
              <span class="git-tool-panel-subtitle"
                >{{ commits.length }} 条提交</span
              >
            </div>
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
              class="git-tool-action git-tool-action-outline"
              type="button"
              :disabled="!selectedRepo"
              @click="openArchiveListDrawer"
            >
              <Archive :size="14" />
              <span>归档</span>
            </button>
          </div>
        </div>

        <div class="git-tool-commit-layout">
          <div
            class="git-tool-commit-list git-tool-commit-table"
            @scroll="handleCommitListScroll"
          >
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
                  :viewBox="`0 0 ${graphSvgWidth} 32`"
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
                    :cy="graphNodeY"
                    :r="graphNodeRadius"
                    :fill="getCommitGraphNode(commit)?.color"
                  />
                </svg>
              </span>
              <span class="git-tool-commit-description">
                <span
                  v-for="badge in getVisibleCommitRefBadges(commit)"
                  :key="`${commit.rowId}:${badge.type}:${badge.name}`"
                  :class="[
                    'git-tool-commit-ref',
                    `git-tool-commit-ref-${badge.type}`
                  ]"
                  :title="badge.title"
                >
                  <component
                    :is="badge.type === 'tag' ? Tag : GitBranchIcon"
                    class="git-tool-commit-ref-icon"
                    :size="11"
                  />
                  <span class="git-tool-commit-ref-name">
                    {{ badge.name }}
                  </span>
                </span>
                <span
                  v-if="getHiddenCommitRefBadgeCount(commit)"
                  class="git-tool-commit-ref git-tool-commit-ref-more"
                  :title="commit.refs"
                >
                  +{{ getHiddenCommitRefBadgeCount(commit) }}
                </span>
                <span
                  data-emphasis
                  class="git-tool-commit-title"
                  :title="commit.subject"
                  >{{ commit.subject }}</span
                >
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
                {{ commit.isGraphOnly ? "" : formatGitGraphDate(commit.date) }}
              </span>
              <span class="git-tool-commit-author">
                {{ commit.isGraphOnly ? "" : commit.author }}
              </span>
              <span class="git-tool-commit-hash">
                <span
                  v-if="!commit.isGraphOnly"
                  class="git-tool-commit-hash-text"
                >
                  {{ commit.shortHash }}
                </span>
                <button
                  v-if="!commit.isGraphOnly && commit.shortHash"
                  class="git-tool-commit-hash-copy"
                  type="button"
                  title="复制提交哈希"
                  @click.stop="copyCommitHash(commit)"
                >
                  <Copy :size="13" />
                </button>
              </span>
            </button>
            <div v-if="commitsLoading" class="git-tool-list-empty">
              正在读取提交
            </div>
            <div v-else-if="commitsLoadingMore" class="git-tool-list-empty">
              正在加载更多提交
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
          <div class="git-tool-stash-panel-header-left">
            <div
              class="git-tool-stash-panel-badge git-tool-stash-panel-badge--blue"
            >
              <Database :size="18" />
            </div>
            <div class="git-tool-stash-panel-header-info">
              <span data-emphasis class="git-tool-panel-title">当前 Stash</span>
              <span class="git-tool-panel-subtitle"
                >{{ stashes.length }} 条记录</span
              >
            </div>
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
          <span class="git-tool-stash-selected-text">
            已选 {{ selectedStashHashes.length }}/{{ stashes.length }}
          </span>
          <div class="git-tool-stash-toolbar">
            <button
              class="git-tool-stash-btn git-tool-stash-btn-white"
              type="button"
              :disabled="!stashes.length"
              @click="toggleSelectAllStashes"
            >
              {{ isAllStashesSelected ? "取消全选" : "全选" }}
            </button>
            <button
              class="git-tool-stash-btn git-tool-stash-btn-white"
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
              class="git-tool-stash-card"
            >
              <input
                class="git-tool-stash-check"
                type="checkbox"
                :checked="isStashChecked(stash.hash)"
                @click.stop
                @change="toggleStashChecked(stash, $event)"
              />
              <div
                class="git-tool-stash-item-icon git-tool-stash-item-icon--blue"
              >
                <GitBranchIcon :size="15" />
              </div>
              <button
                class="git-tool-stash-main"
                type="button"
                @click="openStashDetail(stash)"
              >
                <span
                  data-emphasis
                  class="git-tool-stash-name"
                  :title="`${stash.stashRef} ${stash.subject}`"
                >
                  {{ stash.stashRef }} {{ stash.subject }}
                </span>
                <span class="git-tool-stash-meta">
                  {{ stash.shortHash }} · {{ stash.author }} ·
                  {{ formatDate(stash.date) }}
                </span>
              </button>
              <button
                class="git-tool-stash-btn-archive"
                type="button"
                @click="archiveStash(stash)"
              >
                <ArchiveRestore :size="13" />
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
          <div class="git-tool-stash-panel-header-left">
            <div
              class="git-tool-stash-panel-badge git-tool-stash-panel-badge--green"
            >
              <Archive :size="18" />
            </div>
            <div class="git-tool-stash-panel-header-info">
              <span data-emphasis class="git-tool-panel-title">Stash 归档</span>
              <span class="git-tool-panel-subtitle"
                >{{ stashArchives.length }} 条记录</span
              >
            </div>
          </div>
        </div>

        <div class="git-tool-stash-list git-tool-stash-list--archives">
          <div v-if="stashLoading" class="git-tool-list-empty">
            正在读取 Stash 归档
          </div>
          <template v-else>
            <article
              v-for="archive in stashArchives"
              :key="archive.stashArchiveId"
              class="git-tool-stash-card"
            >
              <div
                class="git-tool-stash-item-icon git-tool-stash-item-icon--green"
              >
                <Archive :size="15" />
              </div>
              <button
                class="git-tool-stash-main"
                type="button"
                @click="openStashArchiveDetail(archive)"
              >
                <span
                  data-emphasis
                  class="git-tool-stash-name"
                  :title="`${archive.stashRef} ${archive.message}`"
                >
                  {{ archive.stashRef }} {{ archive.message }}
                </span>
                <span class="git-tool-stash-meta">
                  {{ formatHash(archive.commitHash) }} ·
                  {{ formatDate(archive.archivedAt) }}
                </span>
              </button>
              <div class="git-tool-stash-actions">
                <button
                  class="git-tool-icon-button git-tool-stash-action-restore"
                  type="button"
                  title="恢复 stash"
                  @click="restoreStashArchive(archive)"
                >
                  <RotateCcw :size="14" />
                </button>
                <button
                  class="git-tool-icon-button git-tool-stash-action-delete"
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
          <div
            v-if="detailDrawerType === 'archives'"
            class="git-tool-drawer-title-box"
          >
            <span data-emphasis class="git-tool-drawer-main-title">分支归档</span>
            <span class="git-tool-drawer-main-subtitle"
              >{{ archives.length }} 条记录</span
            >
          </div>
          <div
            v-else-if="detailDrawerType === 'archive'"
            class="git-tool-drawer-title-box"
          >
            <button
              class="git-tool-drawer-back"
              type="button"
              @click="backToArchiveList"
            >
              <ChevronRight class="git-tool-drawer-back-icon" :size="14" />
              <span class="git-tool-drawer-back-text">返回归档列表</span>
            </button>
            <span data-emphasis class="git-tool-drawer-main-title">{{
              selectedArchive?.branchName || "归档"
            }}</span>
          </div>
          <div v-else class="git-tool-drawer-title-box">
            <span class="git-tool-label">{{ detailDrawerEyebrow }}</span>
            <span data-emphasis class="git-tool-drawer-title">{{
              detailDrawerTitle
            }}</span>
          </div>
          <div class="git-tool-drawer-actions">
            <span
              v-if="detailDrawerType === 'archive'"
              class="git-tool-drawer-hash"
            >
              {{ formatHash(selectedArchive?.commitHash) }}
            </span>
            <button
              class="git-tool-icon-button git-tool-drawer-close-btn"
              type="button"
              title="关闭"
              @click="closeDetailDrawer"
            >
              <X :size="16" />
            </button>
          </div>
        </header>
        <div
          v-if="detailDrawerType === 'archives'"
          class="git-tool-drawer-body"
        >
          <div class="git-tool-drawer-archives">
            <div v-if="archives.length" class="git-tool-archive-tools">
              <span class="git-tool-archive-selected-text">
                已选 {{ selectedArchiveIds.length }}/{{ archives.length }}
              </span>
              <div class="git-tool-archive-toolbar">
                <button
                  class="git-tool-archive-btn git-tool-archive-btn-white"
                  type="button"
                  :disabled="!archives.length"
                  @click="toggleSelectAllArchives"
                >
                  {{ isAllArchivesSelected ? "取消全选" : "全选" }}
                </button>
                <button
                  class="git-tool-archive-btn git-tool-archive-btn-white"
                  type="button"
                  :disabled="!selectedArchives.length"
                  @click="restoreArchives(selectedArchives)"
                >
                  恢复选中
                </button>
                <button
                  class="git-tool-archive-btn git-tool-archive-btn-primary"
                  type="button"
                  :disabled="!archives.length"
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
                    :size="15"
                    class="git-tool-archive-group-chevron"
                  />
                  <span class="git-tool-archive-group-branch-icon">
                    <GitBranchIcon :size="14" />
                  </span>
                  <span data-emphasis class="git-tool-archive-group-name">{{
                    group.label
                  }}</span>
                  <span class="git-tool-archive-group-badge">{{
                    group.archives.length
                  }}</span>
                </button>
                <button
                  class="git-tool-archive-btn-group-restore"
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
                  class="git-tool-archive-card"
                >
                  <input
                    class="git-tool-archive-check"
                    type="checkbox"
                    :checked="isArchiveChecked(archive.archiveId)"
                    @click.stop
                    @change="toggleArchiveChecked(archive, $event)"
                  />
                  <button
                    class="git-tool-archive-card-main"
                    type="button"
                    @click="openArchiveDetail(archive)"
                  >
                    <span
                      data-emphasis
                      class="git-tool-archive-card-name"
                      :title="archive.branchName"
                      >{{ archive.branchName }}</span
                    >
                    <span
                      class="git-tool-archive-card-path"
                      :title="archive.projectPath"
                    >
                      <Folder
                        :size="13"
                        class="git-tool-archive-card-folder"
                      />
                      <span>{{ archive.projectPath }}</span>
                    </span>
                    <span class="git-tool-archive-card-meta">
                      <code class="git-tool-archive-hash-badge">{{
                        formatHash(archive.commitHash)
                      }}</code>
                      <span class="git-tool-archive-meta-divider">|</span>
                      <span class="git-tool-archive-date">{{
                        formatDate(archive.archivedAt)
                      }}</span>
                    </span>
                  </button>
                  <div class="git-tool-archive-card-actions">
                    <button
                      class="git-tool-archive-action-btn git-tool-archive-action-restore"
                      type="button"
                      title="恢复归档"
                      @click.stop="restoreArchive(archive)"
                    >
                      <RotateCcw :size="12" />
                      <span>恢复</span>
                    </button>
                    <button
                      class="git-tool-archive-action-btn git-tool-archive-action-delete"
                      type="button"
                      title="删除归档"
                      @click.stop="deleteArchive(archive)"
                    >
                      <Trash2 :size="12" />
                      <span>删除</span>
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
            <span data-emphasis>{{
              formatFullDate(selectedArchive?.archivedAt)
            }}</span>
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
          <span data-emphasis>{{ confirmDialog.message }}</span>
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
  Database,
  FileText,
  Folder,
  FolderGit2,
  GitBranch as GitBranchIcon,
  Hash,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Tag,
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
                    "span",
                    {
                      "data-emphasis": true,
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
                      h(
                        "span",
                        { "data-emphasis": true },
                        detailProps.detail.author || "-"
                      )
                    ]),
                    h("span", {}, [
                      h("small", {}, "提交时间"),
                      h(
                        "span",
                        { "data-emphasis": true },
                        formatDate(detailProps.detail.date)
                      )
                    ])
                  ])
                ])
              ]),
              h("div", { class: "git-change-view-body" }, [
                h("aside", { class: "git-change-view-tree" }, [
                  h("div", { class: "git-change-view-tree-head" }, [
                    h("span", {}, "变更文件"),
                    h(
                      "span",
                      { "data-emphasis": true },
                      `${detailProps.detail.files.length} 个文件`
                    )
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
                      "span",
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
            h("span", { "data-emphasis": true, title: node.path }, node.name),
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
const commitsLoadingMore = ref(false)
const commitsHasMore = ref(false)
const commitsCheckEnabled = ref(false)
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
const commitPageSize = 80
const graphColors = [
  "#19a5ff",
  "#22c55e",
  "#d946ef",
  "#f59e0b",
  "#ef4444",
  "#14b8a6"
]
const graphColumnWidth = 8
const graphSvgWidth = 144
const graphTrackOffset = 12
const graphNodeY = 16
const graphNodeRadius = 3.6
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

const branchSearchKeyword = ref("")

const filteredBranchGroups = computed(() => {
  const keyword = branchSearchKeyword.value.trim().toLowerCase()
  if (!keyword) return branchGroups.value
  return branchGroups.value
    .map((group) => ({
      ...group,
      branches: group.branches.filter((branch) =>
        branch.name.toLowerCase().includes(keyword)
      )
    }))
    .filter((group) => group.branches.length > 0)
})

const isAllArchivableBranchesSelected = computed(() => {
  return (
    archivableBranches.value.length > 0 &&
    selectedBranchNames.value.length === archivableBranches.value.length
  )
})

function toggleSelectAllBranches() {
  if (isAllArchivableBranchesSelected.value) {
    selectedBranchNames.value = []
  } else {
    selectedBranchNames.value = archivableBranches.value.map(
      (item) => item.name
    )
  }
}

async function copyRepoPath() {
  if (!selectedRepo.value?.localPath) return
  try {
    await navigator.clipboard.writeText(selectedRepo.value.localPath)
    createMessage.success("已复制到剪贴板")
  } catch {
    createMessage.error("复制路径失败")
  }
}

async function copyCommitHash(commit) {
  const hash = commit.hash || commit.shortHash
  if (!hash) return
  try {
    await navigator.clipboard.writeText(hash)
    createMessage.success("已复制提交哈希")
  } catch {
    createMessage.error("复制提交哈希失败")
  }
}

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
      loadCommits()
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
    commitsHasMore.value = false
    commitsCheckEnabled.value = false
    return
  }

  commitsLoading.value = true
  commitsLoadingMore.value = false
  const loadSeq = commitLoadSeq + 1
  commitLoadSeq = loadSeq
  const repoId = selectedRepoId.value
  const branchName = selectedBranch.value
  commits.value = []
  commitsHasMore.value = false
  commitsCheckEnabled.value = false
  selectedCommit.value = null
  selectedCommitDetail.value = null

  try {
    const quickCommits = await gitToolApi.listGitToolCommits({
      repoId,
      branchName,
      skipCheck: true,
      offset: 0,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      branchName !== selectedBranch.value
    ) {
      return
    }

    commits.value = quickCommits
    commitsHasMore.value =
      quickCommits.filter((item) => !item.isGraphOnly).length === commitPageSize

    if (
      options.checkCommits === false ||
      !project.value?.checkBranchName ||
      project.value.checkBranchName === branchName
    ) {
      return
    }

    commitsCheckEnabled.value = true

    const checkedCommits = await gitToolApi.listGitToolCommits({
      repoId,
      branchName,
      skipCheck: false,
      offset: 0,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      branchName !== selectedBranch.value
    ) {
      return
    }

    commits.value = checkedCommits
    commitsHasMore.value =
      checkedCommits.filter((item) => !item.isGraphOnly).length ===
      commitPageSize
  } catch (error) {
    showErrorMessage(error)
  } finally {
    if (loadSeq === commitLoadSeq) {
      commitsLoading.value = false
    }
  }
}

function handleCommitListScroll(event) {
  const target = event.currentTarget

  if (
    commitsLoading.value ||
    commitsLoadingMore.value ||
    !commitsHasMore.value ||
    !selectedRepoId.value ||
    !selectedBranch.value
  ) {
    return
  }

  if (target.scrollTop + target.clientHeight >= target.scrollHeight - 24) {
    loadMoreCommits()
  }
}

async function loadMoreCommits() {
  if (
    commitsLoading.value ||
    commitsLoadingMore.value ||
    !commitsHasMore.value ||
    !selectedRepoId.value ||
    !selectedBranch.value
  ) {
    return
  }

  commitsLoadingMore.value = true
  const loadSeq = commitLoadSeq
  const repoId = selectedRepoId.value
  const branchName = selectedBranch.value
  const offset = commits.value.filter((item) => !item.isGraphOnly).length
  const currentLength = commits.value.length

  try {
    const moreCommits = await gitToolApi.listGitToolCommits({
      repoId,
      branchName,
      skipCheck: true,
      offset,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      branchName !== selectedBranch.value
    ) {
      return
    }

    commits.value = commits.value.concat(moreCommits)
    commitsHasMore.value =
      moreCommits.filter((item) => !item.isGraphOnly).length === commitPageSize

    if (!moreCommits.length || !commitsCheckEnabled.value) {
      return
    }

    const checkedMoreCommits = await gitToolApi.listGitToolCommits({
      repoId,
      branchName,
      skipCheck: false,
      offset,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      branchName !== selectedBranch.value
    ) {
      return
    }

    commits.value = commits.value
      .slice(0, currentLength)
      .concat(checkedMoreCommits)
    commitsHasMore.value =
      checkedMoreCommits.filter((item) => !item.isGraphOnly).length ===
      commitPageSize
  } catch (error) {
    showErrorMessage(error)
  } finally {
    if (loadSeq === commitLoadSeq) {
      commitsLoadingMore.value = false
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

const isAllArchivesSelected = computed(
  () =>
    archives.value.length > 0 &&
    selectedArchiveIds.value.length === archives.value.length
)

function toggleSelectAllArchives() {
  if (isAllArchivesSelected.value) {
    selectedArchiveIds.value = []
  } else {
    selectedArchiveIds.value = archives.value.map((item) => item.archiveId)
  }
}

function selectAllArchives() {
  toggleSelectAllArchives()
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

const isAllStashesSelected = computed(
  () =>
    stashes.value.length > 0 &&
    selectedStashHashes.value.length === stashes.value.length
)

function toggleSelectAllStashes() {
  if (isAllStashesSelected.value) {
    selectedStashHashes.value = []
  } else {
    selectedStashHashes.value = stashes.value.map((item) => item.hash)
  }
}

function selectAllStashes() {
  toggleSelectAllStashes()
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

function formatGitGraphDate(value) {
  if (!value) {
    return "-"
  }

  const date = new Date(value)
  const monthNames = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec"
  ]
  const day = date.getDate()
  const month = monthNames[date.getMonth()]
  const year = date.getFullYear()
  const hour = String(date.getHours()).padStart(2, "0")
  const minute = String(date.getMinutes()).padStart(2, "0")

  return `${day} ${month} ${year} ${hour}:${minute}`
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

function getCommitRefBadges(commit) {
  if (commit.isGraphOnly) {
    return []
  }

  return String(commit.refs || "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
    .map((refText) => {
      if (refText.startsWith("tag: ")) {
        return {
          type: "tag",
          name: refText.replace(/^tag:\s*/, ""),
          title: refText
        }
      }

      const isCurrent = refText.startsWith("HEAD -> ")
      const name = refText.replace(/^HEAD ->\s*/, "")

      return {
        type: isCurrent ? "current" : name.includes("/") ? "remote" : "branch",
        name,
        title: refText
      }
    })
}

function getVisibleCommitRefBadges(commit) {
  return getCommitRefBadges(commit).slice(0, 2)
}

function getHiddenCommitRefBadgeCount(commit) {
  return Math.max(getCommitRefBadges(commit).length - 2, 0)
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
  console.error("[GitTool] 操作失败:", error)
  const message =
    error?.data?.message || error?.message || error?.toString() || "操作失败"
  createMessage.error(message)
}
</script>

<style scoped lang="less">
.git-tool :deep(.base-modal) {
  z-index: 96;
}

.git-tool :deep(.base-modal__panel) {
  width: 420px;
  border-color: var(--color-line-strong);
  box-shadow: 0 20px 52px rgba(15, 23, 42, 0.2);
}

.git-tool :deep(.base-modal__header) {
  align-items: center;
  padding: 15px 16px 8px;
}

.git-tool :deep(.base-modal__header h2) {
  color: var(--color-text);
  font-size: var(--font-size-lg);
}

.git-tool :deep(.base-modal__header p) {
  margin-top: 4px;
  color: var(--color-text-muted);
  font-size: var(--font-size-base);
}

.git-tool :deep(.base-modal__close) {
  width: 30px;
  height: 30px;
  border-radius: 7px;
  font-size: var(--font-size-lg);
}

.git-tool :deep(.base-modal__content) {
  padding: 0 16px 16px;
}

.git-tool-confirm {
  display: flex;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);
}

.git-tool-confirm-icon {
  display: grid;
  width: 34px;
  height: 34px;
  flex: none;
  place-items: center;
  border: 1px solid var(--color-info-line);
  border-radius: 8px;
  background: var(--color-primary-soft);
  color: var(--color-primary);
}

.git-tool-confirm-content {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 5px;
}

.git-tool-confirm-content [data-emphasis] {
  color: var(--color-text);
  font-size: var(--font-size-base);
  line-height: 1.5;
}

.git-tool-confirm-content span:not([data-emphasis]) {
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: var(--font-size-base);
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

.git-tool-confirm-field span:not([data-emphasis]) {
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
}

.git-tool-confirm-field input {
  height: 32px;
  padding: 0 9px;
  border: 1px solid var(--color-line);
  border-radius: 7px;
  background: var(--color-panel);
  color: var(--color-text);
  font-size: var(--font-size-base);
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
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: var(--font-size-base);
}

.git-tool-confirm-button:hover {
  border-color: var(--color-line-strong);
  background: var(--color-panel-soft);
}

.git-tool-confirm-button-primary {
  border-color: var(--color-primary);
  background: var(--color-primary-solid);
  color: #ffffff;
}

.git-tool-confirm-button-primary:hover {
  border-color: var(--color-primary);
  background: var(--color-primary-solid);
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
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: var(--font-size-base);
}

.git-tool-action:hover,
.git-tool-icon-button:hover {
  border-color: var(--color-line-strong);
  background: var(--color-panel-soft);
}

.git-tool-action:disabled,
.git-tool-icon-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.git-tool-action-primary {
  border-color: var(--color-primary);
  background: var(--color-primary-solid);
  color: #ffffff;
}

.git-tool-action-primary:hover {
  border-color: var(--color-primary);
  background: var(--color-primary-solid);
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
    background: var(--color-panel);
    color: var(--color-text-muted);

    .git-tool-loading-icon {
      color: var(--color-primary);
      animation: git-tool-loading-spin 0.9s linear infinite;
    }

    .git-tool-loading-title {
      color: var(--color-text);
      font-size: var(--font-size-lg);
    }

    .git-tool-loading-desc {
      max-width: 420px;
      overflow: hidden;
      font-size: var(--font-size-base);
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
  flex: none;
}

.git-tool-top-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 14px 12px;
  border: 1px solid var(--color-line);
  border-radius: 10px;
  background: var(--color-panel);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);
}

.git-tool-project-label {
  color: var(--color-text-muted);
  font-size: 12px;
  font-weight: 500;
  line-height: 1;
}

.git-tool-command-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 36px;
}

.git-tool-picker-box {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  cursor: pointer;
  min-width: 170px;
  transition: border-color 0.15s ease;

  &:hover {
    border-color: var(--color-line-strong);
  }

  .git-tool-picker-icon {
    color: #2563eb;
    flex-shrink: 0;
  }

  .git-tool-picker-name {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    color: var(--color-text);
    font-size: 13.5px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-tool-picker-arrow {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .git-tool-select-native {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
}

.git-tool-repo-path-box {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 8px 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);
  flex: 1;
  min-width: 0;

  .git-tool-path-icon {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .git-tool-repo-path-text {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-tool-path-copy {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: all 0.15s ease;

    &:hover {
      background: var(--color-line);
      color: var(--color-text);
    }
  }
}

.git-tool-tabs {
  display: flex;
  align-items: center;
  gap: 3px;
  height: 36px;
  padding: 3px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);
  flex-shrink: 0;

  .git-tool-tab {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 28px;
    padding: 0 14px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    transition: all 0.15s ease;

    &:hover {
      color: var(--color-text);
    }

    &.git-tool-tab-active {
      background: #2563eb;
      color: #ffffff;
      box-shadow: 0 1px 3px rgba(37, 99, 235, 0.25);
    }
  }
}

.git-tool-action {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 36px;
  padding: 0 15px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-text);
  cursor: pointer;
  font-size: 13.5px;
  font-weight: 500;
  flex-shrink: 0;
  transition: all 0.15s ease;

  &:hover {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }

  &.git-tool-action-primary {
    border-color: #2563eb;
    background: #2563eb;
    color: #ffffff;

    &:hover:not(:disabled) {
      background: #1d4ed8;
      border-color: #1d4ed8;
    }
  }

  &.git-tool-action-outline {
    border-color: var(--color-line-strong);
    background: var(--color-panel);
    color: var(--color-text);

    &:hover:not(:disabled) {
      border-color: #2563eb;
      color: #2563eb;
      background: var(--color-panel-soft);
    }
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
  background: var(--color-panel);
  color: var(--color-text-muted);
}

.git-tool-empty-title {
  color: var(--color-text);
  font-size: var(--font-size-lg);
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
  border-radius: 10px;
  background: var(--color-panel);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);
}

.git-tool-branch-panel {
  width: 250px;
  flex: 0 0 250px;
  background: var(--color-panel);
}

.git-tool-commit-panel {
  flex: 1;
  min-width: 0;
}

.git-tool-panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 52px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--color-line);
  background: var(--color-panel);

  .git-tool-panel-title-group {
    display: flex;
    align-items: center;
    gap: 10px;

    .git-tool-panel-branch-icon {
      color: #2563eb;
      flex-shrink: 0;
    }

    .git-tool-panel-title-text {
      display: flex;
      flex-direction: column;
      gap: 1px;

      .git-tool-panel-title {
        color: var(--color-text);
        font-size: 16px;
        font-weight: 700;
        line-height: 1.2;
      }

      .git-tool-panel-subtitle {
        color: var(--color-text-muted);
        font-size: 12px;
        line-height: 1.2;
      }
    }
  }

  .git-tool-panel-actions {
    display: flex;
    align-items: center;
    gap: 8px;

    .git-tool-mini-select {
      width: 120px;
      height: 32px;
      padding: 0 8px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-text);
      font-size: 12.5px;
    }

    .git-tool-icon-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 32px;
      height: 32px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      cursor: pointer;
      transition: all 0.15s ease;

      &:hover:not(:disabled) {
        border-color: var(--color-line-strong);
        background: var(--color-panel-soft);
        color: var(--color-text);
      }

      &:disabled {
        opacity: 0.48;
        cursor: not-allowed;
      }
    }
  }
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
  padding: 12px 12px 10px;
  border-bottom: 1px solid var(--color-line);
  background: var(--color-panel);

  .git-tool-branch-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;

    .git-tool-branch-title-wrap {
      display: flex;
      align-items: center;
      gap: 7px;

      .git-tool-branch-title {
        color: var(--color-text);
        font-size: 15px;
        font-weight: 700;
        line-height: 1.2;
      }

      .git-tool-branch-count-badge {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 1px 7px;
        border-radius: 10px;
        background: var(--color-panel-soft);
        border: 1px solid var(--color-line);
        color: var(--color-text-muted);
        font-size: 11.5px;
        font-weight: 600;
      }
    }
  }

  .git-tool-branch-search {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;

    .git-tool-branch-search-icon {
      position: absolute;
      left: 9px;
      color: var(--color-text-muted);
      pointer-events: none;
    }

    .git-tool-branch-search-input {
      width: 100%;
      height: 32px;
      padding: 0 24px 0 28px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel-soft);
      color: var(--color-text);
      font-size: 12.5px;
      transition: all 0.15s ease;

      &::placeholder {
        color: var(--color-text-muted);
      }

      &:focus {
        border-color: #2563eb;
        background: var(--color-panel);
        outline: none;
      }
    }

    .git-tool-branch-search-clear {
      position: absolute;
      right: 6px;
      display: flex;
      align-items: center;
      justify-content: center;
      width: 18px;
      height: 18px;
      border: 0;
      border-radius: 50%;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;

      &:hover {
        background: var(--color-line);
        color: var(--color-text);
      }
    }
  }

  .git-tool-branch-action-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 2px;

    .git-tool-branch-select-all {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      color: var(--color-text-muted);
      font-size: 12.5px;
      cursor: pointer;
      user-select: none;
    }

    .git-tool-branch-archive-btn {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      height: 26px;
      padding: 0 9px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      font-size: 12px;
      cursor: pointer;
      transition: all 0.15s ease;

      &:hover:not(:disabled) {
        border-color: #2563eb;
        color: #2563eb;
        background: #eff6ff;
      }

      &:disabled {
        opacity: 0.48;
        cursor: not-allowed;
      }
    }
  }
}

.git-tool-branch-list {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  overflow: auto;
  padding: 8px;
}

.git-tool-branch-group {
  display: flex;
  flex-direction: column;
  gap: 2px;

  .git-tool-branch-group-head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    font-size: 12.5px;
    transition: background 0.15s ease;

    &:hover {
      background: var(--color-panel-soft);
    }

    .git-tool-branch-group-label {
      font-weight: 600;
      color: var(--color-text);
    }

    .git-tool-branch-group-badge {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      padding: 1px 6px;
      margin-left: auto;
      border-radius: 10px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      font-size: 11px;
    }
  }

  .git-tool-branch-group-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-left: 6px;
  }
}

.git-tool-branch {
  display: flex;
  min-height: 32px;
  align-items: center;
  gap: 6px;
  padding: 2px 6px;
  border-radius: 6px;
  color: var(--color-text);
  transition: background 0.15s ease;

  &:hover {
    background: var(--color-panel-soft);
  }

  &.git-tool-branch-active {
    background: #eff6ff;

    .git-tool-branch-main .git-tool-branch-name {
      color: #2563eb;
      font-weight: 600;
    }
  }

  &.git-tool-branch-current {
    .git-tool-branch-main {
      .git-tool-branch-icon {
        color: #2563eb;
      }

      .git-tool-branch-name {
        color: #2563eb;
        font-weight: 600;
      }
    }
  }

  .git-tool-branch-check {
    width: 14px;
    height: 14px;
    flex: none;
    margin: 0;
    accent-color: #2563eb;
    cursor: pointer;
  }

  .git-tool-branch-main {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;

    .git-tool-branch-icon {
      color: var(--color-text-muted);
      flex-shrink: 0;
    }

    .git-tool-branch-name {
      min-width: 0;
      flex: 1;
      overflow: hidden;
      color: var(--color-text);
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .git-tool-branch-badge {
      flex: none;
      padding: 1px 7px;
      border-radius: 10px;
      background: #e0effe;
      color: #1d4ed8;
      font-size: 11px;
      font-weight: 600;
    }
  }
}

.git-tool-commit-layout {
  display: flex;
  min-height: 0;
  flex: 1;
}

.git-tool-commit-table {
  width: 100%;
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 0;
  background: var(--color-panel);

  .git-tool-commit-table-head {
    position: sticky;
    top: 0;
    z-index: 2;
    display: grid;
    grid-template-columns: 76px minmax(360px, 1fr) 150px 90px 105px;
    flex: none;
    align-items: center;
    min-width: 780px;
    height: 32px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 600;

    .git-tool-commit-table-head-cell {
      min-width: 0;
      overflow: hidden;
      padding: 0 10px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .git-tool-commit {
    display: grid;
    grid-template-columns: 76px minmax(360px, 1fr) 150px 90px 105px;
    min-width: 780px;
    min-height: 36px;
    gap: 0;
    padding: 0;
    border: 0;
    border-bottom: 1px solid var(--color-line);
    border-radius: 0;
    background: var(--color-panel);
    text-align: left;
    cursor: pointer;
    transition: background 0.12s ease;

    &:hover {
      background: var(--color-panel-soft);
    }

    &.git-tool-commit-active {
      background: #eff6ff;
    }

    .git-tool-commit-graph-cell {
      position: relative;
      display: flex;
      align-items: center;
      justify-content: center;
      min-width: 0;
      height: 100%;
      overflow: visible;
      padding: 0;

      .git-tool-commit-graph-svg {
        display: block;
        width: 72px;
        height: calc(100% + 2px);
        margin-top: -1px;
        overflow: visible;

        .git-tool-commit-graph-line {
          fill: none;
          stroke-linecap: round;
          stroke-linejoin: round;
          stroke-width: 1.8;
          vector-effect: non-scaling-stroke;
        }

        .git-tool-commit-graph-node {
          stroke: #ffffff;
          stroke-width: 1.8;
          vector-effect: non-scaling-stroke;
        }
      }
    }

    .git-tool-commit-description {
      display: flex;
      align-items: center;
      gap: 6px;
      min-width: 0;
      height: 100%;
      overflow: hidden;
      padding: 0 10px;
      color: var(--color-text);

      .git-tool-commit-ref {
        display: inline-flex;
        align-items: center;
        gap: 3px;
        height: 20px;
        flex: none;
        padding: 0 6px;
        border-radius: 4px;
        font-size: 11px;
        line-height: 20px;
        white-space: nowrap;

        .git-tool-commit-ref-icon {
          flex: none;
        }

        .git-tool-commit-ref-name {
          min-width: 0;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        &.git-tool-commit-ref-current {
          background: #2563eb;
          color: #ffffff;
          font-weight: 600;
        }

        &.git-tool-commit-ref-branch {
          background: #eff6ff;
          border: 1px solid #bfdbfe;
          color: #1d4ed8;
          font-weight: 500;
        }

        &.git-tool-commit-ref-remote {
          background: var(--color-panel-soft);
          border: 1px solid var(--color-line);
          color: var(--color-text-muted);
        }

        &.git-tool-commit-ref-tag {
          background: #fef3c7;
          border: 1px solid #fde68a;
          color: #b45309;
          font-weight: 500;
        }

        &.git-tool-commit-ref-more {
          background: var(--color-panel-soft);
          border: 1px solid var(--color-line);
          color: var(--color-text-muted);
        }
      }

      .git-tool-commit-title {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text);
        font-size: 13px;
      }
    }

    .git-tool-commit-date,
    .git-tool-commit-author {
      display: flex;
      min-width: 0;
      align-items: center;
      height: 100%;
      overflow: hidden;
      padding: 0 10px;
      color: var(--color-text-muted);
      font-size: 12.5px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .git-tool-commit-hash {
      display: flex;
      min-width: 0;
      align-items: center;
      justify-content: space-between;
      height: 100%;
      padding: 0 10px;

      .git-tool-commit-hash-text {
        color: #2563eb;
        font-family: "JetBrains Mono", "Consolas", monospace;
        font-size: 12px;
        font-weight: 600;
      }

      .git-tool-commit-hash-copy {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 22px;
        height: 22px;
        border: 0;
        border-radius: 4px;
        background: transparent;
        color: var(--color-text-muted);
        cursor: pointer;
        opacity: 0.65;
        transition: all 0.15s ease;

        &:hover {
          opacity: 1;
          background: var(--color-line);
          color: #2563eb;
        }
      }
    }

    &.git-tool-commit-graph {
      min-height: 20px;
      color: var(--color-text-soft);
    }
  }
}

:global(:root[data-theme="dark"]) {
  .git-tool {
    .git-tool-branch.git-tool-branch-active {
      background: rgba(37, 99, 235, 0.16);

      .git-tool-branch-main .git-tool-branch-name {
        color: #60a5fa;
      }
    }

    .git-tool-branch.git-tool-branch-current {
      .git-tool-branch-main {
        .git-tool-branch-icon,
        .git-tool-branch-name {
          color: #60a5fa;
        }
      }
    }

    .git-tool-branch .git-tool-branch-main .git-tool-branch-badge {
      background: rgba(37, 99, 235, 0.25);
      color: #93c5fd;
    }

    .git-tool-branch-archive-btn:hover:not(:disabled) {
      background: rgba(37, 99, 235, 0.16);
      border-color: #60a5fa;
      color: #60a5fa;
    }

    .git-tool-commit-table .git-tool-commit {
      &.git-tool-commit-active {
        background: rgba(37, 99, 235, 0.16);
      }

      .git-tool-commit-graph-cell .git-tool-commit-graph-svg .git-tool-commit-graph-node {
        stroke: var(--color-panel);
      }

      .git-tool-commit-description .git-tool-commit-ref {
        &.git-tool-commit-ref-branch {
          background: rgba(37, 99, 235, 0.2);
          border-color: rgba(96, 165, 250, 0.4);
          color: #93c5fd;
        }

        &.git-tool-commit-ref-tag {
          background: rgba(245, 158, 11, 0.16);
          border-color: rgba(245, 158, 11, 0.3);
          color: #fcd34d;
        }
      }

      .git-tool-commit-hash {
        .git-tool-commit-hash-text {
          color: #60a5fa;
        }

        .git-tool-commit-hash-copy:hover {
          color: #60a5fa;
        }
      }
    }

    .git-tool-stash-workbench {
      .git-tool-stash-panel-badge--blue {
        background: rgba(37, 99, 235, 0.16);
        color: #60a5fa;
      }

      .git-tool-stash-panel-badge--green {
        background: rgba(13, 148, 104, 0.16);
        color: #34d399;
      }

      .git-tool-stash-selected-text {
        color: #60a5fa;
      }

      .git-tool-stash-btn-white {
        background: var(--color-panel);
        border-color: var(--color-line-strong);
        color: var(--color-text);

        &:hover:not(:disabled) {
          background: var(--color-panel-soft);
        }
      }

      .git-tool-stash-item-icon--blue {
        background: rgba(37, 99, 235, 0.16);
        color: #60a5fa;
      }

      .git-tool-stash-item-icon--green {
        background: rgba(13, 148, 104, 0.16);
        color: #34d399;
      }

      .git-tool-stash-btn-archive {
        background: var(--color-panel);
        border-color: rgba(37, 99, 235, 0.35);
        color: #60a5fa;

        &:hover {
          background: rgba(37, 99, 235, 0.16);
          border-color: #60a5fa;
        }
      }

      .git-tool-stash-action-restore {
        color: #34d399;

        &:hover {
          background: rgba(13, 148, 104, 0.16);
          border-color: rgba(52, 211, 153, 0.4);
        }
      }

      .git-tool-stash-action-delete {
        color: #f87171;

        &:hover {
          background: rgba(239, 68, 68, 0.16);
          border-color: rgba(248, 113, 113, 0.4);
        }
      }
    }

    .git-tool-drawer-panel-archives {
      .git-tool-drawer-archives {
        background: var(--color-page);
      }

      .git-tool-archive-tools {
        background: rgba(37, 99, 235, 0.12);
        border-color: rgba(37, 99, 235, 0.28);
        color: #60a5fa;
      }

      .git-tool-archive-selected-text {
        color: #60a5fa;
      }

      .git-tool-archive-btn-white {
        background: var(--color-panel);
        border-color: var(--color-line-strong);
        color: var(--color-text);

        &:hover:not(:disabled) {
          background: var(--color-panel-soft);
        }
      }

      .git-tool-archive-group {
        background: var(--color-panel);
        border-color: var(--color-line);
      }

      .git-tool-archive-group-branch-icon {
        background: rgba(37, 99, 235, 0.16);
        color: #60a5fa;
      }

      .git-tool-archive-group-badge {
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
      }

      .git-tool-archive-btn-group-restore {
        background: var(--color-panel);
        border-color: rgba(37, 99, 235, 0.35);
        color: #60a5fa;

        &:hover {
          background: rgba(37, 99, 235, 0.16);
          border-color: #60a5fa;
        }
      }

      .git-tool-archive-group-body {
        border-top-color: var(--color-line);
      }

      .git-tool-archive-card {
        background: var(--color-panel-soft);
        border-color: var(--color-line);

        &:hover {
          background: var(--color-panel);
          border-color: rgba(96, 165, 250, 0.4);
        }
      }

      .git-tool-archive-hash-badge {
        background: rgba(37, 99, 235, 0.16);
        border-color: rgba(37, 99, 235, 0.3);
        color: #60a5fa;
      }

      .git-tool-archive-action-btn {
        background: var(--color-panel);
        border-color: var(--color-line);

        &:hover {
          background: var(--color-panel-soft);
        }
      }

      .git-tool-archive-action-restore {
        color: #60a5fa;

        &:hover {
          background: rgba(37, 99, 235, 0.16);
          border-color: rgba(96, 165, 250, 0.4);
        }
      }

      .git-tool-archive-action-delete {
        color: #f87171;

        &:hover {
          background: rgba(239, 68, 68, 0.16);
          border-color: rgba(248, 113, 113, 0.4);
        }
      }
    }
  }
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
  min-width: 0;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--font-size-base);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-commit-meta,
.git-tool-archive-meta,
.git-tool-stash-meta,
.git-change-view-meta {
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-check {
  flex: none;
  padding: 2px 6px;
  border-radius: 999px;
  font-size: var(--font-size-sm);
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
  border: 1px solid var(--color-line-strong);
  border-radius: 7px;
  background: var(--color-panel);
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
  color: var(--color-primary);
  cursor: pointer;
  font-size: var(--font-size-base);
  text-align: left;
}

.git-tool-context-menu-button:hover {
  background: var(--color-primary-soft);
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
  font-size: var(--font-size-base);
}

.git-tool-archive,
.git-tool-stash {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: var(--color-panel-soft);
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
  font-size: var(--font-size-sm);
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
  gap: 16px;

  .git-tool-stash-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    overflow: hidden;
  }

  .git-tool-stash-panel-header-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .git-tool-stash-panel-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    flex: none;

    &--blue {
      background: #eff6ff;
      color: #2563eb;
    }

    &--green {
      background: #ecfdf5;
      color: #0d9468;
    }
  }

  .git-tool-stash-panel-header-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .git-tool-stash-summary-row {
    display: flex;
    min-height: 42px;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    color: var(--color-text);
    font-size: var(--font-size-base);
  }

  .git-tool-stash-selected-text {
    font-size: 13px;
    font-weight: 600;
    color: #2563eb;
  }

  .git-tool-stash-toolbar {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  .git-tool-stash-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 26px;
    padding: 0 10px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .git-tool-stash-btn-white {
    border: 1px solid var(--color-line-strong);
    background: var(--color-panel);
    color: var(--color-text);

    &:hover:not(:disabled) {
      border-color: #94a3b8;
      background: var(--color-panel-soft);
    }
  }

  .git-tool-stash-list {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px;
    gap: 8px;
  }

  .git-tool-stash-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 9px;
    background: var(--color-panel);
    transition: border-color 0.2s ease, background-color 0.2s ease, box-shadow 0.2s ease;

    &:hover {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
    }
  }

  .git-tool-stash-check {
    width: 15px;
    height: 15px;
    flex: none;
    margin: 0;
    accent-color: #2563eb;
    cursor: pointer;
  }

  .git-tool-stash-item-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    flex: none;

    &--blue {
      background: #eff6ff;
      color: #2563eb;
    }

    &--green {
      background: #ecfdf5;
      color: #0d9468;
    }
  }

  .git-tool-stash-main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    gap: 3px;
    border: 0;
    background: transparent;
    padding: 0;
    text-align: left;
    cursor: pointer;
  }

  .git-tool-stash-name {
    width: 100%;
    overflow: hidden;
    color: var(--color-text);
    font-size: 13.5px;
    font-weight: 600;
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-tool-stash-meta {
    width: 100%;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 11.5px;
    line-height: 1.3;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-tool-stash-btn-archive {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid #bfdbfe;
    border-radius: 6px;
    background: var(--color-panel);
    color: #2563eb;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;

    &:hover {
      border-color: #2563eb;
      background: #eff6ff;
    }
  }

  .git-tool-stash-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .git-tool-stash-action-restore,
  .git-tool-stash-action-delete {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    cursor: pointer;
    transition: all 0.15s ease;

    svg {
      flex: none;
    }
  }

  .git-tool-stash-action-restore {
    color: #0d9468;

    &:hover {
      border-color: #a7f3d0;
      background: #ecfdf5;
      color: #059669;
    }
  }

  .git-tool-stash-action-delete {
    color: #ef4444;

    &:hover {
      border-color: #fecaca;
      background: #fef2f2;
      color: #dc2626;
    }
  }
}

.git-tool-drawer {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: flex;
  justify-content: flex-end;
  background: rgba(15, 23, 42, 0.28);
  backdrop-filter: blur(2px);
}

.git-tool-drawer-panel {
  display: flex;
  height: 100%;
  flex-direction: column;
  border-left: 1px solid var(--color-line);
  background: var(--color-panel);
  box-shadow: -16px 0 38px rgba(15, 23, 42, 0.16);

  &.git-tool-drawer-panel-archives {
    width: 560px;
    max-width: 92vw;

    .git-tool-drawer-head {
      padding: 16px 20px;
      border-bottom: 1px solid var(--color-line);
      background: var(--color-panel);
    }

    .git-tool-drawer-title-box {
      display: flex;
      flex-direction: column;
      gap: 3px;
    }

    .git-tool-drawer-main-title {
      color: var(--color-text);
      font-size: 20px;
      font-weight: 700;
      line-height: 1.25;
    }

    .git-tool-drawer-main-subtitle {
      color: var(--color-text-muted);
      font-size: 13px;
      line-height: 1.4;
    }

    .git-tool-drawer-close-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 32px;
      height: 32px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      cursor: pointer;
      transition: all 0.15s ease;

      &:hover {
        border-color: var(--color-line-strong);
        background: var(--color-panel-soft);
        color: var(--color-text);
      }
    }

    .git-tool-drawer-body {
      flex: 1;
      min-height: 0;
      overflow: hidden;
    }

    .git-tool-drawer-archives {
      width: 100%;
      flex: 1;
      min-height: 0;
      display: flex;
      flex-direction: column;
      gap: 12px;
      overflow-y: auto;
      padding: 16px;
      background: #f8fafc;
    }

    .git-tool-archive-tools {
      display: flex;
      min-height: 46px;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 9px 14px;
      border: 1px solid #bfdbfe;
      border-radius: 10px;
      background: #f1f7fe;
      color: #2563eb;
    }

    .git-tool-archive-selected-text {
      font-size: 13.5px;
      font-weight: 600;
      color: #2563eb;
    }

    .git-tool-archive-toolbar {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .git-tool-archive-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      height: 28px;
      padding: 0 12px;
      border-radius: 6px;
      font-size: 12.5px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.15s ease;

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
    }

    .git-tool-archive-btn-white {
      border: 1px solid var(--color-line-strong, #cbd5e1);
      background: #ffffff;
      color: var(--color-text, #334155);

      &:hover:not(:disabled) {
        border-color: #94a3b8;
        background: #f8fafc;
      }
    }

    .git-tool-archive-btn-primary {
      border: 1px solid #2563eb;
      background: #2563eb;
      color: #ffffff;

      &:hover:not(:disabled) {
        background: #1d4ed8;
        border-color: #1d4ed8;
      }
    }

    .git-tool-archive-group {
      display: flex;
      flex-direction: column;
      border: 1px solid #e2e8f0;
      border-radius: 12px;
      background: #ffffff;
      padding: 14px 16px;
      margin-bottom: 4px;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);
      transition: border-color 0.2s ease, box-shadow 0.2s ease;
    }

    .git-tool-archive-group-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
    }

    .git-tool-archive-group-toggle {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      gap: 8px;
      height: 32px;
      padding: 0;
      border: 0;
      background: transparent;
      color: var(--color-text);
      cursor: pointer;
      text-align: left;
    }

    .git-tool-archive-group-chevron {
      color: var(--color-text-muted);
      flex: none;
      transition: transform 0.2s ease;
    }

    .git-tool-archive-group-branch-icon {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 24px;
      height: 24px;
      flex: none;
      border-radius: 6px;
      background: #eff6ff;
      color: #2563eb;
    }

    .git-tool-archive-group-name {
      min-width: 0;
      overflow: hidden;
      color: var(--color-text);
      font-size: 15px;
      font-weight: 700;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .git-tool-archive-group-badge {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      min-width: 20px;
      height: 20px;
      padding: 0 6px;
      border-radius: 10px;
      background: var(--color-panel-soft, #f1f5f9);
      color: var(--color-text-muted);
      font-size: 12px;
      font-weight: 600;
    }

    .git-tool-archive-btn-group-restore {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      height: 26px;
      padding: 0 10px;
      border: 1px solid #bfdbfe;
      border-radius: 6px;
      background: #ffffff;
      color: #2563eb;
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.15s ease;

      &:hover {
        border-color: #2563eb;
        background: #eff6ff;
      }
    }

    .git-tool-archive-group-body {
      display: flex;
      flex-direction: column;
      gap: 9px;
      margin-top: 12px;
      padding-top: 10px;
      border-top: 1px solid #f1f5f9;
    }

    .git-tool-archive-card {
      position: relative;
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 12px 14px;
      border: 1px solid #e2e8f0;
      border-radius: 10px;
      background: #ffffff;
      transition: border-color 0.2s ease, background-color 0.2s ease, box-shadow 0.2s ease;

      &:hover {
        border-color: #93c5fd;
        box-shadow: 0 2px 8px rgba(37, 99, 235, 0.08);
      }
    }

    .git-tool-archive-check {
      width: 15px;
      height: 15px;
      flex: none;
      margin: 0;
      accent-color: #2563eb;
      cursor: pointer;
    }

    .git-tool-archive-card-main {
      display: flex;
      min-width: 0;
      flex: 1;
      flex-direction: column;
      align-items: flex-start;
      gap: 4px;
      border: 0;
      background: transparent;
      padding: 0;
      cursor: pointer;
      text-align: left;
    }

    .git-tool-archive-card-name {
      width: 100%;
      overflow: hidden;
      color: var(--color-text);
      font-size: 14.5px;
      font-weight: 700;
      line-height: 1.35;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .git-tool-archive-card-path {
      display: inline-flex;
      max-width: 100%;
      align-items: center;
      gap: 5px;
      color: var(--color-text-muted);
      font-size: 12px;
      line-height: 1.3;

      span {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }

    .git-tool-archive-card-folder {
      flex: none;
      color: var(--color-text-muted);
    }

    .git-tool-archive-card-meta {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      color: var(--color-text-muted);
      font-size: 11.5px;
      margin-top: 1px;
    }

    .git-tool-archive-hash-badge {
      display: inline-block;
      padding: 1px 6px;
      border: 1px solid #dbeafe;
      border-radius: 4px;
      background: #eff6ff;
      color: #2563eb;
      font-family: "JetBrains Mono", "Consolas", monospace;
      font-size: 11px;
      font-weight: 600;
      line-height: 1.4;
    }

    .git-tool-archive-meta-divider {
      color: var(--color-line-strong, #cbd5e1);
      font-size: 11px;
    }

    .git-tool-archive-date {
      color: var(--color-text-muted);
      font-size: 11.5px;
    }

    .git-tool-archive-card-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 6px;
    }

    .git-tool-archive-action-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 4px;
      height: 26px;
      padding: 0 9px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.15s ease;
      white-space: nowrap;

      svg {
        flex: none;
      }
    }

    .git-tool-archive-action-restore {
      color: #2563eb;

      &:hover {
        border-color: #bfdbfe;
        background: #eff6ff;
        color: #1d4ed8;
      }
    }

    .git-tool-archive-action-delete {
      color: #ef4444;

      &:hover {
        border-color: #fecaca;
        background: #fef2f2;
        color: #dc2626;
      }
    }
  }
}

.git-tool-drawer-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 14px;
  padding: 14px;
  border-bottom: 1px solid var(--color-line);
  background: var(--color-panel-soft);
}

.git-tool-drawer-title {
  display: block;
  margin-top: 3px;
  color: var(--color-text);
  font-size: var(--font-size-lg);
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
  font-size: var(--font-size-base);
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
  color: var(--color-primary);
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
  border: 1px solid var(--color-info-line);
  border-radius: 7px;
  background: var(--color-primary-soft);
  color: var(--color-primary);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: var(--font-size-sm);
  line-height: 28px;
}

.git-tool-drawer-body {
  display: flex;
  min-height: 0;
  flex: 1;
}

.git-tool-archive-detail {
  flex-direction: column;
  background: var(--color-panel);
}

.git-tool-drawer-body-detail {
  display: block;
}

.git-tool-drawer-commits {
  width: 300px;
  flex: 0 0 300px;
  border-right: 1px solid var(--color-line);
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
  background: var(--color-primary-soft);
  color: var(--color-primary);
  font-size: var(--font-size-base);
}

.git-tool-archive-detail-meta span:not([data-emphasis]),
.git-tool-archive-detail-meta [data-emphasis] {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.git-tool-archive-detail-meta [data-emphasis] {
  flex: none;
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
  background: var(--color-primary-soft);
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
}

.git-tool-archive-commit-head span:not([data-emphasis]),
.git-tool-archive-commit span:not([data-emphasis]) {
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
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: var(--font-size-base);
  text-align: left;
}

.git-tool-archive-commit:hover {
  background: var(--color-primary-soft);
}

.git-tool-archive-commit-active {
  background: var(--color-primary-soft);
}

.git-tool-archive-commit-title {
  color: var(--color-text);
}

.git-tool-archive-commit-hash {
  color: var(--color-primary);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: var(--font-size-sm);
}

.git-tool-archive-detail-content {
  flex: 1;
  min-height: 0;
  padding: 12px;
  background: var(--color-panel-soft);
}

.git-tool-archive-detail-content .git-change-view {
  overflow: hidden;
  border: 1px solid var(--color-line-strong);
  border-radius: 7px;
  background: var(--color-panel);
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
  background: var(--color-panel);
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
  font-size: var(--font-size-base);
}

.git-change-view-meta-row {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 8px;
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
}

.git-change-view-meta-row span:not([data-emphasis]) {
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
  font-size: var(--font-size-sm);
}

.git-change-view-meta-row code,
.git-change-view-meta-row [data-emphasis] {
  min-width: 0;
  overflow: hidden;
  color: var(--color-text-muted);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: var(--font-size-sm);
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
  background: var(--color-panel-soft);
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
  font-size: var(--font-size-base);
}

.git-change-view-tree-head [data-emphasis] {
  color: var(--color-primary);
  font-size: var(--font-size-sm);
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
  font-size: var(--font-size-base);
}

.git-change-view-tree-directory:hover {
  background: var(--color-primary-soft);
}

.git-change-view-tree-file {
  width: 100%;
  cursor: pointer;
}

.git-change-view-tree-file:hover {
  background: var(--color-primary-soft);
}

.git-change-view-tree-file-active {
  border-left-color: var(--color-primary);
  background: var(--color-primary-soft);
}

.git-change-view-tree-caret,
.git-change-view-tree-folder {
  flex: none;
  color: var(--color-text-muted);
}

.git-change-view-tree-directory [data-emphasis] {
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
  background: var(--color-panel-soft);
  color: var(--color-primary);
  font-size: var(--font-size-sm);
}

.git-change-view-file-status {
  display: inline-flex;
  width: 18px;
  height: 18px;
  flex: 0 0 18px;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  font-size: var(--font-size-sm);
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
  font-size: var(--font-size-base);
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
  background: var(--color-panel-soft);
}

.git-change-view-file-head [data-emphasis] {
  display: block;
  overflow: hidden;
  color: var(--color-primary);
  font-size: var(--font-size-base);
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
  background: var(--color-panel);
  color: var(--color-text);
  font-family: "JetBrains Mono", "Consolas", monospace;
  font-size: var(--font-size-base);
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
  background: var(--color-success-soft);
  color: var(--color-success);
}

.git-change-view-line-delete {
  background: var(--color-danger-soft);
  color: var(--color-danger);
}

.git-change-view-line-chunk {
  background: var(--color-primary-soft);
  color: var(--color-primary);
}

.git-change-view-line-meta {
  background: var(--color-panel-soft);
  color: var(--color-text-muted);
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
    background: var(--color-panel);
  }

  :deep(.git-change-view-summary) {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  :deep(.git-change-view .git-tool-label) {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  :deep(.git-change-view-title) {
    display: block;
    max-width: 100%;
    overflow: hidden;
    color: var(--color-text);
    font-size: var(--font-size-base);
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
    font-size: var(--font-size-sm);
  }

  :deep(.git-change-view-meta-row span:not([data-emphasis])) {
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
    font-size: var(--font-size-sm);
  }

  :deep(.git-change-view-meta-row code),
  :deep(.git-change-view-meta-row [data-emphasis]) {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-muted);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: var(--font-size-sm);
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
    background: var(--color-panel-soft);
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
    font-size: var(--font-size-base);
  }

  :deep(.git-change-view-tree-head [data-emphasis]) {
    color: var(--color-primary);
    font-size: var(--font-size-sm);
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
    font-size: var(--font-size-base);
  }

  :deep(.git-change-view-tree-directory:hover) {
    background: var(--color-primary-soft);
  }

  :deep(.git-change-view-tree-file) {
    width: 100%;
    cursor: pointer;
  }

  :deep(.git-change-view-tree-file:hover) {
    background: var(--color-primary-soft);
  }

  :deep(.git-change-view-tree-file-active) {
    border-left-color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  :deep(.git-change-view-tree-caret),
  :deep(.git-change-view-tree-folder) {
    flex: none;
    color: var(--color-text-muted);
  }

  :deep(.git-change-view-tree-directory [data-emphasis]) {
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
    background: var(--color-panel-soft);
    color: var(--color-primary);
    font-size: var(--font-size-sm);
  }

  :deep(.git-change-view-file-status) {
    display: inline-flex;
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    font-size: var(--font-size-sm);
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
    font-size: var(--font-size-base);
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
    background: var(--color-panel-soft);
  }

  :deep(.git-change-view-file-head [data-emphasis]) {
    display: block;
    overflow: hidden;
    color: var(--color-primary);
    font-size: var(--font-size-base);
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
    background: var(--color-panel);
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: var(--font-size-base);
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
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  :deep(.git-change-view-line-delete) {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  :deep(.git-change-view-line-chunk) {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  :deep(.git-change-view-line-meta) {
    background: var(--color-panel-soft);
    color: var(--color-text-muted);
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
    font-size: var(--font-size-base);
  }
}
</style>

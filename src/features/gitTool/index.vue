<template>
  <section class="git-tool" @click="closeContextMenus">
    <section class="git-tool-top">
      <div class="git-tool-top-card">
        <span class="git-tool-project-label">项目</span>
        <div class="git-tool-command-row">
          <ElSelect
            class="git-tool-picker git-tool-repo-picker"
            :model-value="selectedRepoId"
            aria-label="项目"
            placeholder="选择或搜索项目"
            filterable
            :persistent="false"
            :show-arrow="false"
            :suffix-icon="ChevronDown"
            :offset="8"
            popper-class="git-tool-picker-popup git-tool-repo-popup"
            no-match-text="没有匹配的项目"
            no-data-text="尚未添加项目"
            :disabled="
              repos.length === 0 ||
              operationBusy ||
              !!operationDialog ||
              confirmDialog.visible
            "
            @change="handleRepoChange"
          >
            <template #prefix>
              <FolderGit2 :size="16" class="git-tool-picker-icon" />
            </template>
            <template #label>{{ selectedRepo?.name }}</template>
            <template #header>
              <div class="git-tool-picker-heading">
                <span>选择项目</span><span>{{ repos.length }} 个项目</span>
              </div>
            </template>
            <ElOption
              v-for="repo in repos"
              :key="repo.id"
              :value="repo.id"
              :label="`${repo.name} ${repo.localPath || ''}`"
            >
              <div class="git-tool-picker-option">
                <div class="git-tool-picker-option-heading">
                  <FolderGit2 :size="15" class="git-tool-picker-option-icon" />
                  <span class="git-tool-picker-option-name">{{
                    repo.name
                  }}</span>
                  <Check
                    v-if="repo.id === selectedRepoId"
                    :size="15"
                    class="git-tool-picker-check"
                  />
                </div>
                <span
                  class="git-tool-picker-option-path"
                  :title="repo.localPath"
                  >{{ repo.localPath || "尚未设置目录" }}</span
                >
              </div>
            </ElOption>
          </ElSelect>

          <ElSelect
            v-if="worktrees.length"
            class="git-tool-picker git-tool-worktree-picker"
            :model-value="selectedWorktreePath"
            aria-label="工作树"
            placeholder="搜索分支或工作树路径"
            filterable
            :persistent="false"
            :show-arrow="false"
            :suffix-icon="ChevronDown"
            :offset="8"
            popper-class="git-tool-picker-popup git-tool-worktree-popup"
            no-match-text="没有匹配的工作树"
            no-data-text="暂无工作树"
            :disabled="
              gitLoading ||
              operationBusy ||
              !!operationDialog ||
              confirmDialog.visible
            "
            @change="handleWorktreeChange"
          >
            <template #prefix>
              <GitBranchIcon :size="16" class="git-tool-picker-icon" />
            </template>
            <template #label
              >工作树 · {{ selectedWorktree?.branch || "未关联分支" }}</template
            >
            <template #header>
              <div class="git-tool-picker-heading">
                <span>选择工作树</span
                ><span>{{ worktrees.length }} 个目录</span>
              </div>
            </template>
            <ElOption
              v-for="tree in worktrees"
              :key="tree.path"
              :value="tree.path"
              :label="`${tree.branch || '未关联分支'} ${tree.path} ${tree.isMain ? '主工作树' : '附加工作树'}`"
              :disabled="tree.bare || tree.prunable"
            >
              <div class="git-tool-picker-option">
                <div class="git-tool-picker-option-heading">
                  <GitBranchIcon
                    :size="15"
                    class="git-tool-picker-option-icon"
                  />
                  <span class="git-tool-picker-option-name">{{
                    tree.branch || "未关联分支"
                  }}</span>
                  <span class="git-tool-picker-tag">{{
                    tree.isMain ? "主工作树" : "附加工作树"
                  }}</span>
                  <span
                    v-if="tree.prunable || tree.bare"
                    class="git-tool-picker-tag git-tool-picker-tag-warning"
                    >{{ tree.prunable ? "目录失效" : "裸仓库" }}</span
                  >
                  <span
                    v-else-if="tree.locked"
                    class="git-tool-picker-tag"
                    :title="tree.lockReason"
                    >已锁定</span
                  >
                  <Check
                    v-if="tree.path === selectedWorktreePath"
                    :size="15"
                    class="git-tool-picker-check"
                  />
                </div>
                <span class="git-tool-picker-option-path" :title="tree.path">{{
                  tree.path
                }}</span>
              </div>
            </ElOption>
          </ElSelect>

          <div class="git-tool-repo-path-box">
            <Folder :size="15" class="git-tool-path-icon" />
            <span class="git-tool-repo-path-text" :title="activeProjectPath">
              {{ activeProjectPath || "未选择项目" }}
            </span>
            <button
              v-if="activeProjectPath"
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
              v-for="tab in [
                { key: 'worktree', label: '工作区', icon: FileText },
                { key: 'branch', label: '分支', icon: GitBranchIcon },
                { key: 'file-history', label: '文件历史', icon: History },
                { key: 'stash', label: '贮藏', icon: Database }
              ]"
              :key="tab.key"
              :class="[
                'git-tool-tab',
                { 'git-tool-tab-active': gitWorkspace === tab.key }
              ]"
              type="button"
              @click="selectGitWorkspace(tab.key)"
            >
              <component :is="tab.icon" :size="14" />
              <span>{{ tab.label }}</span>
            </button>
          </div>

          <button
            class="git-tool-action git-tool-action-primary"
            type="button"
            :disabled="gitLoading || operationBusy || !selectedRepo"
            @click="refreshGitProject()"
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
        <div
          v-if="gitStatus && !branchCompareVisible"
          class="git-tool-operation-bar"
        >
          <div class="git-tool-operation-status">
            <GitBranchIcon :size="13" />
            <span class="git-tool-operation-branch">{{
              gitStatus.branch ||
              `未关联分支 · ${gitStatus.headHash.slice(0, 8)}`
            }}</span>
            <span class="git-tool-operation-upstream">{{
              gitStatus.upstream || "未设置上游"
            }}</span>
            <span v-if="gitStatus.upstream" class="git-tool-operation-count"
              >↑ {{ gitStatus.ahead }} · ↓ {{ gitStatus.behind }}</span
            >
            <button
              class="git-tool-operation-dirty"
              type="button"
              @click="selectGitWorkspace('worktree')"
            >
              {{
                gitStatus.files.length
                  ? `${gitStatus.files.length} 个未提交文件`
                  : "工作区干净"
              }}
            </button>
          </div>
          <div class="git-tool-operation-actions">
            <button
              v-for="action in [
                'create-branch',
                'switch-branch',
                'checkout-commit',
                'merge',
                'stash-save',
                'fetch',
                'pull',
                'push'
              ]"
              :key="action"
              class="git-tool-operation-button"
              type="button"
              :title="`${gitOperationDefinitions[action].description}\n命令：${gitOperationDefinitions[action].command}`"
              :disabled="
                operationBusy ||
                gitLoading ||
                !!gitStatus.operation ||
                conflictCount > 0 ||
                (['fetch', 'pull', 'push'].includes(action) &&
                  !gitStatus.remotes.length)
              "
              @click="openGitOperation(action)"
            >
              {{ gitOperationDefinitions[action].label }}
            </button>
          </div>
        </div>
      </div>
    </section>

    <div
      v-if="gitStatus?.operation || conflictCount"
      class="git-tool-operation-banner"
    >
      <div class="git-tool-operation-banner-info">
        <span class="git-tool-operation-banner-title">{{
          gitStatus.operation
            ? `${activeOperationLabel}操作尚未完成`
            : "工作区存在冲突"
        }}</span>
        <span class="git-tool-operation-banner-description">{{
          conflictCount
            ? `${conflictCount} 个冲突文件：打开编辑器解决后暂存，或选择保留的一侧版本。`
            : "冲突已暂存，可以继续完成操作。"
        }}</span>
      </div>
      <div class="git-tool-operation-banner-actions">
        <button
          class="git-tool-operation-banner-button"
          type="button"
          @click="selectGitWorkspace('worktree')"
        >
          查看工作区
        </button>
        <button
          v-if="gitStatus.operation"
          class="git-tool-operation-banner-button"
          type="button"
          :disabled="operationBusy || conflictCount > 0"
          @click="executeGitOperation({ action: 'continue' })"
        >
          继续
        </button>
        <button
          v-if="
            ['cherry-pick', 'revert', 'rebase'].includes(gitStatus.operation)
          "
          class="git-tool-operation-banner-button"
          type="button"
          :disabled="operationBusy"
          @click="executeGitOperation({ action: 'skip' })"
        >
          跳过当前提交
        </button>
        <button
          v-if="gitStatus.operation"
          class="git-tool-operation-banner-button"
          type="button"
          :disabled="operationBusy"
          @click="executeGitOperation({ action: 'abort' })"
        >
          中止
        </button>
      </div>
    </div>
    <div
      v-if="operationOutput.text"
      :class="[
        'git-tool-operation-output',
        { 'git-tool-operation-output-error': !operationOutput.success }
      ]"
    >
      <span class="git-tool-operation-output-label">{{
        operationOutput.label
      }}</span>
      <pre class="git-tool-operation-output-text">{{
        operationOutput.text
      }}</pre>
      <button
        class="git-tool-operation-output-close"
        type="button"
        aria-label="关闭操作输出"
        @click="operationOutput.text = ''"
      >
        <X :size="14" />
      </button>
    </div>

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

    <section v-else-if="gitLoading && !gitStatus" class="git-tool-loading">
      <RefreshCw class="git-tool-loading-icon" :size="22" />
      <span data-emphasis class="git-tool-loading-title"
        >正在加载 Git 数据</span
      >
      <span class="git-tool-loading-desc">{{
        selectedRepo?.name || "当前项目"
      }}</span>
    </section>

    <GitWorkspacePanel
      v-else-if="gitWorkspace === 'worktree' && gitStatus"
      :key="`${selectedRepoId}:${selectedWorktreePath}`"
      :repo-id="selectedRepoId"
      :status="gitStatus"
      :busy="
        operationBusy ||
        gitLoading ||
        !!operationDialog ||
        confirmDialog.visible
      "
      :execute="executeGitOperation"
    >
      <template #default="{ detail, title, select }">
        <GitChangeDetail
          :detail="detail"
          :title="title"
          :hide-tree="true"
          @select-file="select"
        />
      </template>
    </GitWorkspacePanel>
    <section
      v-else-if="gitWorkspace === 'worktree'"
      class="git-tool-list-empty"
    >
      {{ gitStatusError || "正在读取工作区…" }}
    </section>
    <GitFileHistoryPanel
      v-else-if="gitWorkspace === 'file-history'"
      :key="`file-history:${selectedRepoId}:${selectedWorktreePath}`"
      :repo-id="selectedRepoId"
      :project-path="activeProjectPath"
      :worktree-path="selectedWorktreePath"
      :branch-name="selectedBranch || currentBranch"
      :check-branch-name="project?.checkBranchName || ''"
      :check-branch-loading="checkBranchLoading"
      :branches="branches"
      :busy="
        operationBusy ||
        gitLoading ||
        !!operationDialog ||
        confirmDialog.visible
      "
      @select-commit="selectCommit"
      @change-check-branch="handleCheckBranchChange"
    />
    <section v-else-if="gitWorkspace === 'branch'" class="git-tool-workbench">
      <aside v-if="!branchCompareVisible" class="git-tool-branch-panel">
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
                  :disabled="branch.isCurrent || !!branch.worktreePath"
                  @click.stop
                  @change="toggleBranchChecked(branch, $event)"
                />
                <button
                  class="git-tool-branch-main"
                  type="button"
                  :disabled="operationBusy"
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
            <div
              v-if="branchCompareVisible"
              class="git-tool-panel-badge git-tool-panel-badge--blue"
            >
              <GitCompare :size="18" />
            </div>
            <GitBranchIcon
              v-else
              :size="20"
              class="git-tool-panel-branch-icon"
            />
            <div class="git-tool-panel-title-text">
              <span data-emphasis class="git-tool-panel-title">{{
                branchCompareVisible ? "对比分支" : selectedBranch || "提交记录"
              }}</span>
              <span class="git-tool-panel-subtitle">{{
                branchCompareVisible
                  ? "选择两个分支进行对比，查看文件差异和代码变更。"
                  : `${commits.length} 条提交`
              }}</span>
            </div>
          </div>
          <div class="git-tool-panel-actions">
            <select
              v-if="!branchCompareVisible"
              class="git-tool-mini-select"
              :value="project?.checkBranchName || ''"
              :disabled="!selectedBranch || checkBranchLoading"
              title="检查分支"
              @change="handleCheckBranchChange($event.target.value)"
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
              v-if="!branchCompareVisible"
              class="git-tool-icon-button"
              type="button"
              title="清理检查缓存"
              :disabled="!project?.checkBranchName || !selectedBranch"
              @click="clearCheckCache"
            >
              <RotateCcw :size="14" />
            </button>
            <button
              v-if="!branchCompareVisible"
              class="git-tool-action git-tool-action-outline"
              type="button"
              :disabled="!selectedRepo"
              @click="openArchiveListDrawer"
            >
              <Archive :size="14" />
              <span>归档</span>
            </button>
            <button
              class="git-tool-action git-tool-action-outline"
              type="button"
              :disabled="branches.length < 2 || !selectedRepo"
              @click="branchCompareVisible = !branchCompareVisible"
            >
              <component
                :is="branchCompareVisible ? History : GitCompare"
                :size="14"
              />
              <span>{{ branchCompareVisible ? "提交记录" : "对比分支" }}</span>
            </button>
          </div>
        </div>

        <GitBranchComparePanel
          :key="`${selectedRepoId}:${selectedWorktreePath}`"
          v-if="branchCompareVisible"
          :repo-id="selectedRepoId"
          :project-path="activeProjectPath"
          :branches="branches"
          :current-branch="currentBranch"
          :selected-branch="selectedBranch"
          :busy="operationBusy || gitLoading"
          @close="branchCompareVisible = false"
        />
        <div v-else class="git-tool-commit-layout">
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
              :disabled="commit.isGraphOnly || operationBusy"
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
                  :title="formatCheckReason(commit)"
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
              <span data-emphasis class="git-tool-panel-title">当前贮藏</span>
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
            正在读取贮藏
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
              <div class="git-tool-stash-operation-actions">
                <button
                  v-for="action in ['stash-apply', 'stash-pop']"
                  :key="action"
                  class="git-tool-stash-operation-button"
                  type="button"
                  :title="`${gitOperationDefinitions[action].description}\n命令：${gitOperationDefinitions[action].command}`"
                  :disabled="
                    operationBusy || !!gitStatus?.operation || conflictCount > 0
                  "
                  @click="
                    openGitOperation(action, {
                      stashHash: stash.hash,
                      subject: stash.subject
                    })
                  "
                >
                  {{ action === "stash-apply" ? "应用" : "应用并移除" }}
                </button>
              </div>
            </article>
            <div v-if="!stashes.length" class="git-tool-list-empty">
              暂无贮藏
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
              <span data-emphasis class="git-tool-panel-title">贮藏归档</span>
              <span class="git-tool-panel-subtitle"
                >{{ stashArchives.length }} 条记录</span
              >
            </div>
          </div>
        </div>

        <div class="git-tool-stash-list git-tool-stash-list--archives">
          <div v-if="stashLoading" class="git-tool-list-empty">
            正在读取贮藏 归档
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
                  title="恢复贮藏"
                  @click="restoreStashArchive(archive)"
                >
                  <RotateCcw :size="14" />
                </button>
                <button
                  class="git-tool-icon-button git-tool-stash-action-delete"
                  type="button"
                  title="删除贮藏 归档"
                  @click="deleteStashArchive(archive)"
                >
                  <Trash2 :size="14" />
                </button>
              </div>
            </article>
            <div v-if="!stashArchives.length" class="git-tool-list-empty">
              暂无贮藏 归档
            </div>
          </template>
        </div>
      </section>
    </section>

    <section
      v-if="detailDrawerVisible && detailDrawerType === 'archives'"
      class="git-tool-drawer"
      @click="closeDetailDrawer"
    >
      <div
        class="git-tool-drawer-panel git-tool-drawer-panel-archives"
        @click.stop
      >
        <header class="git-tool-drawer-head">
          <div class="git-tool-drawer-title-box">
            <span data-emphasis class="git-tool-drawer-main-title"
              >分支归档</span
            >
            <span class="git-tool-drawer-main-subtitle"
              >{{ archives.length }} 条记录</span
            >
          </div>
          <div class="git-tool-drawer-actions">
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
        <div class="git-tool-drawer-body">
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
                      <Folder :size="13" class="git-tool-archive-card-folder" />
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
      </div>
    </section>

    <!-- 提交详情/归档详情重构现代化弹窗 -->
    <GitCommitDetailModal
      :visible="commitModalVisible"
      :type="commitModalType"
      :project-path="commitModalProjectPath"
      :branch-name="commitModalBranchName"
      :archived-at="commitModalArchivedAt"
      :commit-hash="commitModalHash"
      :author="commitModalAuthor"
      :commits="commitModalCommits"
      :selected-commit-hash="commitModalSelectedHash"
      :commit-detail="commitModalDetail"
      :loading="commitModalLoading"
      @close="closeDetailDrawer"
      @back="handleCommitModalBack"
      @select-commit="handleCommitModalSelectCommit"
      @select-file="handleCommitModalSelectFile"
    />

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
        title="校验提交是否已合入检查分支，或标题和作者高度相似且改动一致&#10;命令：git merge-base --is-ancestor <提交> <检查分支>；git diff-tree -p --binary <提交> | git patch-id --verbatim"
      >
        <RefreshCw :size="14" />
        校验分支是否存在
      </button>
      <button
        class="git-tool-context-menu-button"
        type="button"
        @click="copyContextCommitSubject"
        title="复制提交消息&#10;查询命令：git show -s --format=%s <提交>"
      >
        <Copy :size="14" />
        复制提交消息
      </button>
      <button
        class="git-tool-context-menu-button"
        type="button"
        @click="copyContextCommitHash"
        title="复制完整提交哈希&#10;查询命令：git rev-parse <提交>"
      >
        <Hash :size="14" />
        复制完整哈希
      </button>
      <button
        v-for="action in ['checkout-commit', 'revert', 'reset', 'cherry-pick']"
        :key="action"
        class="git-tool-context-menu-button"
        type="button"
        :title="`${gitOperationDefinitions[action].description}\n命令：${gitOperationDefinitions[action].command}`"
        :disabled="operationBusy || !!gitStatus?.operation || conflictCount > 0"
        @click="
          openGitOperation(action, { revision: commitContextMenu.commit?.hash })
        "
      >
        <component :is="gitOperationIcons[action]" :size="14" />
        {{ gitOperationDefinitions[action].label }}
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
      <button
        v-for="action in ['switch-branch', 'rename-branch', 'merge']"
        :key="action"
        class="git-tool-context-menu-button"
        type="button"
        :title="`${gitOperationDefinitions[action].description}\n命令：${gitOperationDefinitions[action].command}`"
        :disabled="operationBusy || !!gitStatus?.operation || conflictCount > 0"
        @click="
          openGitOperation(action, {
            branchName: branchContextMenu.branch?.name,
            revision: branchContextMenu.branch?.name
          })
        "
      >
        <component :is="gitOperationIcons[action]" :size="14" />
        {{ gitOperationDefinitions[action].label }}
      </button>
    </div>

    <GitOperationModal
      v-if="operationDialog"
      :key="operationDialog.action"
      :action="operationDialog.action"
      :initial="operationDialog.initial"
      :repo-id="selectedRepoId"
      :status="operationDialog.status"
      :branches="branches"
      :busy="operationBusy"
      :execute="executeGitOperation"
      @close="operationDialog = null"
    />

    <!-- 操作确认弹窗组件 -->
    <GitConfirmModal
      :visible="confirmDialog.visible"
      :title="confirmDialog.title"
      :description="confirmDialog.description"
      :message="confirmDialog.message"
      :detail="confirmDialog.detail"
      :input="confirmDialog.input"
      :input-label="confirmDialog.inputLabel"
      :input-value="confirmDialog.inputValue"
      :confirm-text="confirmDialog.confirmText"
      @confirm="resolveConfirmDialog"
      @cancel="cancelConfirmDialog"
    />
  </section>
</template>

<script setup>
import {
  computed,
  defineComponent,
  h,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch
} from "vue"
import {
  Archive,
  ArchiveRestore,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Database,
  FileText,
  Folder,
  FolderGit2,
  GitBranch as GitBranchIcon,
  GitCommitHorizontal,
  GitCompare,
  GitFork,
  GitMerge,
  Hash,
  History,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Tag,
  Trash2,
  Undo2,
  X
} from "lucide-vue-next"
import GitCommitDetailModal from "@/features/gitTool/components/GitCommitDetailModal.vue"
import GitConfirmModal from "@/features/gitTool/components/GitConfirmModal.vue"
import GitWorkspacePanel from "@/features/gitTool/components/GitWorkspacePanel.vue"
import GitOperationModal from "@/features/gitTool/components/GitOperationModal.vue"
import GitBranchComparePanel from "@/features/gitTool/components/GitBranchComparePanel.vue"
import GitFileHistoryPanel from "@/features/gitTool/components/GitFileHistoryPanel.vue"
import {
  gitOperationDefinitions,
  formatGitGraphDate,
  formatCheckStatus,
  formatCheckReason
} from "@/features/gitTool/constant"
import { ElOption, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"
import { gitToolApi } from "@/api"
import { createMessage } from "@/utils/message"

const props = defineProps({
  repos: {
    type: Array,
    required: true
  }
})

const emit = defineEmits(["add-repo", "status-change"])

// 右键操作沿用同一套图标，和已有菜单项保持一致的对齐方式。
const gitOperationIcons = {
  "switch-branch": GitBranchIcon,
  "rename-branch": Pencil,
  "checkout-commit": GitCommitHorizontal,
  merge: GitMerge,
  "cherry-pick": GitFork,
  revert: Undo2,
  reset: RotateCcw
}

const GitChangeDetail = defineComponent({
  props: {
    detail: {
      type: Object,
      default: null
    },
    title: {
      type: String,
      default: ""
    },
    // 工作区已有文件分组，仅复用差异区域，隐藏重复的提交元信息。
    hideTree: {
      type: Boolean,
      default: false
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
                  h(
                    "span",
                    { class: "git-tool-label" },
                    detailProps.hideTree ? "文件差异" : "提交详情"
                  ),
                  h(
                    "span",
                    {
                      "data-emphasis": true,
                      class: "git-change-view-title",
                      title: detailProps.title
                    },
                    detailProps.title || detailProps.detail.subject
                  ),
                  detailProps.hideTree
                    ? null
                    : h("div", { class: "git-change-view-meta-row" }, [
                        h("span", {}, [
                          h("small", {}, "完整哈希"),
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
                detailProps.hideTree
                  ? null
                  : h("aside", { class: "git-change-view-tree" }, [
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
// 项目共享分支历史，各工作树的 HEAD、暂存区和文件改动独立管理。
const selectedWorktreePath = ref("")
const worktrees = ref([])
const selectedWorktree = computed(() =>
  worktrees.value.find((tree) => tree.path === selectedWorktreePath.value)
)
const gitWorkspace = ref("branch")
// 工作区状态与执行状态独立于历史浏览；写操作完成后统一刷新。
const gitStatus = ref(null)
const activeOperationLabel = computed(() =>
  gitStatus.value?.operation === "rebase"
    ? "变基"
    : gitOperationDefinitions[gitStatus.value?.operation]?.label || ""
)
const gitStatusError = ref("")
const operationBusy = ref(false)
const operationDialog = ref(null)
const operationOutput = ref({ text: "", success: true, label: "" })
const conflictCount = computed(
  () => gitStatus.value?.files.filter((file) => file.conflicted).length || 0
)
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
const checkBranchLoading = ref(false)
const branches = ref([])
const commits = ref([])
const archives = ref([])
const stashes = ref([])
const stashArchives = ref([])
const currentBranch = ref("")
const selectedBranch = ref("")
// 分支对比视图独立于提交列表，切换时保留当前分支选择。
const branchCompareVisible = ref(false)
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
let projectLoadSeq = 0
let detailLoadSeq = 0
let stashLoadSeq = 0
let stashDetailLoadSeq = 0
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
const activeProjectPath = computed(
  () => selectedWorktreePath.value || selectedRepo.value?.localPath || ""
)

const gitToolStatus = computed(() => [
  { label: "未提交", value: gitStatus.value?.files.length || 0 },
  {
    label: "当前分支",
    value: currentBranch.value || "-"
  },
  {
    label: "本地分支",
    value: branches.value.length
  },
  {
    label: "贮藏",
    value: stashes.value.length
  },
  {
    label: "归档",
    value: archives.value.length + stashArchives.value.length
  }
])

const archivableBranches = computed(() => {
  return branches.value.filter((item) => !item.isCurrent && !item.worktreePath)
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
  if (!activeProjectPath.value) return
  try {
    await navigator.clipboard.writeText(activeProjectPath.value)
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

const commitModalVisible = computed(() => {
  return (
    detailDrawerVisible.value &&
    ["archive", "commit", "stash"].includes(detailDrawerType.value)
  )
})

const commitModalType = computed(() => {
  return detailDrawerType.value || "commit"
})

const commitModalProjectPath = computed(() => {
  if (detailDrawerType.value === "archive") {
    return (
      selectedArchive.value?.projectPath || selectedRepo.value?.localPath || ""
    )
  }
  return activeProjectPath.value
})

const commitModalBranchName = computed(() => {
  if (detailDrawerType.value === "archive") {
    return selectedArchive.value?.branchName || ""
  }
  if (detailDrawerType.value === "stash") {
    return selectedStash.value?.stashRef || "stash"
  }
  return selectedCommit.value?.historyBranchName || currentBranch.value || ""
})

const commitModalArchivedAt = computed(() => {
  return selectedArchive.value?.archivedAt || null
})

const commitModalHash = computed(() => {
  if (detailDrawerType.value === "archive") {
    return selectedArchive.value?.commitHash || ""
  }
  if (detailDrawerType.value === "stash") {
    return selectedStash.value?.stashRef || ""
  }
  return selectedCommit.value?.hash || ""
})

const commitModalAuthor = computed(() => {
  if (detailDrawerType.value === "archive") {
    return selectedArchiveCommit.value?.author || ""
  }
  if (detailDrawerType.value === "commit") {
    return selectedCommit.value?.author || ""
  }
  return ""
})

const commitModalCommits = computed(() => {
  if (detailDrawerType.value === "archive") {
    return visibleArchiveCommits.value
  }
  if (detailDrawerType.value === "commit") {
    if (selectedCommit.value?.historyBranchName) return []
    return commits.value.filter((item) => !item.isGraphOnly)
  }
  return []
})

const commitModalSelectedHash = computed(() => {
  if (detailDrawerType.value === "archive") {
    return selectedArchiveCommit.value?.hash || ""
  }
  if (detailDrawerType.value === "commit") {
    return selectedCommit.value?.hash || ""
  }
  if (detailDrawerType.value === "stash") {
    return selectedStash.value?.hash || selectedStash.value?.stashRef || ""
  }
  return ""
})

const commitModalDetail = computed(() => {
  if (detailDrawerType.value === "archive") {
    return selectedArchiveCommitDetail.value
  }
  if (detailDrawerType.value === "commit") {
    return selectedCommitDetail.value
  }
  if (detailDrawerType.value === "stash") {
    return stashDetail.value
  }
  return null
})

const commitModalLoading = computed(() => {
  if (detailDrawerType.value === "archive") {
    return archiveCommitDetailLoading.value
  }
  if (detailDrawerType.value === "commit") {
    return commitDetailLoading.value
  }
  if (detailDrawerType.value === "stash") {
    return stashDetailLoading.value
  }
  return false
})

function handleCommitModalBack() {
  if (detailDrawerType.value === "archive") {
    backToArchiveList()
  } else {
    closeDetailDrawer()
  }
}

async function handleCommitModalSelectCommit(commit) {
  if (detailDrawerType.value === "archive") {
    await selectArchiveCommit(commit)
  } else if (detailDrawerType.value === "commit") {
    await selectCommit(commit)
  }
}

async function handleCommitModalSelectFile(filePath) {
  if (detailDrawerType.value === "archive") {
    await selectArchiveCommitFile(filePath)
  } else if (detailDrawerType.value === "commit") {
    await selectCommitFile(filePath)
  } else if (detailDrawerType.value === "stash") {
    await selectStashDetailFile(filePath)
  }
}

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
      resetGitProject(true)
      await nextTick()
      scheduleRefreshGitProject()
    }
  },
  { immediate: true }
)

function handleRepoChange(repoId) {
  selectedRepoId.value = repoId
  resetGitProject(true)
  scheduleRefreshGitProject()
}

function handleWorktreeChange(worktreePath) {
  if (
    operationBusy.value ||
    operationDialog.value ||
    confirmDialog.value.visible
  )
    return
  selectedWorktreePath.value = worktreePath
  resetGitProject(false)
  scheduleRefreshGitProject()
}

// 切换目录后让旧请求失效，避免旧工作树的结果或草稿覆盖新目录。
function resetGitProject(clearWorktrees) {
  projectLoadSeq++
  commitLoadSeq++
  detailLoadSeq++
  stashLoadSeq++
  stashDetailLoadSeq++
  if (clearWorktrees) {
    selectedWorktreePath.value = ""
    worktrees.value = []
  }
  gitLoading.value = false
  commitsLoading.value = false
  commitsLoadingMore.value = false
  commitDetailLoading.value = false
  stashLoading.value = false
  stashDetailLoading.value = false
  gitStatus.value = null
  branches.value = []
  currentBranch.value = ""
  selectedBranch.value = ""
  project.value = null
  commits.value = []
  operationOutput.value = { text: "", success: true, label: "" }
  closeDetailDrawer()
  closeContextMenus()
  stashLoaded = false
}

function selectGitWorkspace(workspace) {
  gitWorkspace.value = workspace
  if (workspace !== "branch") branchCompareVisible.value = false

  if (workspace === "stash" && !stashLoaded) {
    loadStashes()
  }
  if (workspace === "branch" && selectedBranch.value && !commits.value.length)
    loadCommits()
}

function scheduleRefreshGitProject() {
  window.clearTimeout(refreshTimer)
  refreshTimer = window.setTimeout(() => {
    refreshGitProject()
  }, 30)
}

onBeforeUnmount(() => {
  window.clearTimeout(refreshTimer)
  window.removeEventListener("keydown", handleGitKeydown)
  closeConfirmDialog(false)
})

onMounted(() => {
  window.addEventListener("keydown", handleGitKeydown)
})

function handleGitKeydown(event) {
  if (event.key !== "Escape" || operationBusy.value) return
  if (confirmDialog.value.visible) cancelConfirmDialog()
  else if (operationDialog.value) operationDialog.value = null
  else {
    closeContextMenus()
    closeDetailDrawer()
  }
}

async function refreshGitProject(options = {}) {
  if (!selectedRepoId.value) {
    return
  }

  const repoId = selectedRepoId.value
  const worktreePath = selectedWorktreePath.value
  const sequence = ++projectLoadSeq
  if (!options.silent) gitLoading.value = true
  gitStatusError.value = ""

  try {
    const status =
      options.status ||
      (await gitToolApi.getGitToolStatus({ repoId, worktreePath }))
    if (sequence !== projectLoadSeq) return
    gitStatus.value = status
    selectedWorktreePath.value = status.projectPath
    worktrees.value = status.worktrees || worktrees.value
    const result = await gitToolApi.scanGitToolBranches({
      repoId,
      worktreePath: status.projectPath
    })
    if (sequence !== projectLoadSeq) return

    project.value = result.project || null
    worktrees.value = result.project?.worktrees || worktrees.value
    branches.value = result.branches || []
    currentBranch.value = status.branch || result.currentBranch || ""
    archives.value = result.archives || []
    stashes.value = result.stashes || []
    stashArchives.value = result.stashArchives || []
    stashLoaded = false
    selectedBranchNames.value = selectedBranchNames.value.filter((branchName) =>
      branches.value.find(
        (item) =>
          item.name === branchName && !item.isCurrent && !item.worktreePath
      )
    )
    selectedArchiveIds.value = selectedArchiveIds.value.filter((archiveId) =>
      archives.value.find((item) => item.archiveId === archiveId)
    )
    selectedStashHashes.value = selectedStashHashes.value.filter((stashHash) =>
      stashes.value.find((item) => item.hash === stashHash)
    )

    if (
      status.detached &&
      !branches.value.find((item) => item.name === selectedBranch.value)
    ) {
      selectedBranch.value = status.headHash
    } else if (
      !branches.value.find((item) => item.name === selectedBranch.value)
    ) {
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
    if (sequence === projectLoadSeq)
      gitStatusError.value = String(error?.message || error)
    if (sequence === projectLoadSeq) showErrorMessage(error)
  } finally {
    if (sequence === projectLoadSeq) gitLoading.value = false
  }
}

// 操作弹窗记录打开时的仓库快照，执行时由后端再次验证。
async function openGitOperation(action, initial = {}) {
  if (
    operationBusy.value ||
    gitLoading.value ||
    confirmDialog.value.visible ||
    operationDialog.value ||
    !selectedRepoId.value
  )
    return
  closeContextMenus()
  operationBusy.value = true
  const repoId = selectedRepoId.value
  const worktreePath = selectedWorktreePath.value
  try {
    const status = await gitToolApi.getGitToolStatus({
      repoId,
      worktreePath
    })
    if (
      repoId !== selectedRepoId.value ||
      worktreePath !== selectedWorktreePath.value
    )
      return
    gitStatus.value = status
    operationDialog.value = { action, initial, status }
  } catch (error) {
    showErrorMessage(error)
  } finally {
    operationBusy.value = false
  }
}

async function executeGitOperation(payload) {
  if (
    operationBusy.value ||
    gitLoading.value ||
    confirmDialog.value.visible ||
    !selectedRepoId.value ||
    !gitStatus.value
  )
    return null
  const repoId = selectedRepoId.value
  const worktreePath = selectedWorktreePath.value
  const status = gitStatus.value
  const label =
    gitOperationDefinitions[payload.action]?.label ||
    {
      stage: "暂存文件",
      unstage: "取消暂存",
      discard: "丢弃改动",
      "delete-untracked": "删除未跟踪文件",
      commit: "提交",
      "resolve-ours": "采用当前一侧版本",
      "resolve-theirs": "采用传入一侧版本",
      "resolve-delete": "按删除解决冲突",
      continue: "继续操作",
      abort: "中止操作",
      skip: "跳过当前提交"
    }[payload.action]
  const request = {
    expectedHead: status.headHash,
    expectedBranch: status.branch,
    ...payload,
    repoId,
    worktreePath,
    expectedWorktreePath: status.projectPath
  }
  const replacingFiles = [
    "discard",
    "delete-untracked",
    "resolve-ours",
    "resolve-theirs",
    "resolve-delete"
  ].includes(payload.action)
  const resolvingConflicts =
    payload.action === "stage" &&
    payload.filePaths?.some((path) =>
      status.files.some((file) => file.path === path && file.conflicted)
    )
  if (
    replacingFiles ||
    resolvingConflicts ||
    ["abort", "skip"].includes(payload.action)
  ) {
    const confirmed = await confirmGitAction({
      title: label,
      description: replacingFiles
        ? "所选文件内容将被替换或删除，当前未保存内容不可撤销。"
        : resolvingConflicts
          ? "请确认已解决所选文件的冲突，暂存后 Git 会将它们标记为已解决。"
          : "将结束或跳过当前步骤，正在处理的冲突修改可能被丢弃。",
      message: `确认在「${status.branch || "未关联分支"}」执行${label}吗？`,
      detail:
        payload.filePaths?.join("、") ||
        `${activeOperationLabel.value} · ${status.headHash.slice(0, 8)}`,
      confirmText: `确认${label}`
    })
    if (!confirmed) return null
  }
  if (
    operationBusy.value ||
    repoId !== selectedRepoId.value ||
    worktreePath !== selectedWorktreePath.value
  )
    return null
  operationBusy.value = true
  try {
    const result = await gitToolApi.executeGitToolOperation(request)
    if (
      repoId !== selectedRepoId.value ||
      worktreePath !== selectedWorktreePath.value
    )
      return result
    gitStatus.value = result.status
    operationOutput.value = {
      text: result.output || `${label}${result.success ? "已完成" : "失败"}`,
      success: result.success,
      label
    }
    if (
      result.status.operation ||
      result.status.files.some((file) => file.conflicted)
    ) {
      gitWorkspace.value = "worktree"
      operationDialog.value = null
      closeDetailDrawer()
    }
    if (
      ["switch-branch", "checkout-commit", "rename-branch", "reset"].includes(
        payload.action
      ) ||
      (payload.action === "create-branch" && payload.switchAfterCreate)
    ) {
      selectedBranch.value = result.status.branch
      selectedCommit.value = null
      selectedCommitDetail.value = null
      closeDetailDrawer()
    }
    await refreshGitProject({ silent: true, status: result.status })
    if (result.success) createMessage.success(`${label}已完成`)
    else
      createMessage.error(
        result.status.operation ||
          result.status.files.some((file) => file.conflicted)
          ? "操作遇到冲突，请在工作区处理"
          : `${label}失败，请查看操作输出`
      )
    return result
  } catch (error) {
    operationOutput.value = {
      text: String(error?.message || error),
      success: false,
      label
    }
    showErrorMessage(error)
    await refreshGitProject({ silent: true })
    if (
      operationDialog.value &&
      (gitStatus.value?.headHash !== request.expectedHead ||
        gitStatus.value?.branch !== request.expectedBranch)
    ) {
      operationDialog.value = null
    }
    return null
  } finally {
    operationBusy.value = false
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
  const worktreePath = selectedWorktreePath.value
  const branchName = selectedBranch.value
  commits.value = []
  commitsHasMore.value = false
  commitsCheckEnabled.value = false
  selectedCommit.value = null
  selectedCommitDetail.value = null

  try {
    const quickCommits = await gitToolApi.listGitToolCommits({
      repoId,
      worktreePath,
      branchName,
      skipCheck: true,
      offset: 0,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      worktreePath !== selectedWorktreePath.value ||
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
      worktreePath,
      branchName,
      skipCheck: false,
      offset: 0,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      worktreePath !== selectedWorktreePath.value ||
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
  const worktreePath = selectedWorktreePath.value
  const branchName = selectedBranch.value
  const offset = commits.value.filter((item) => !item.isGraphOnly).length
  const currentLength = commits.value.length

  try {
    const moreCommits = await gitToolApi.listGitToolCommits({
      repoId,
      worktreePath,
      branchName,
      skipCheck: true,
      offset,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      worktreePath !== selectedWorktreePath.value ||
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
      worktreePath,
      branchName,
      skipCheck: false,
      offset,
      limit: commitPageSize
    })

    if (
      loadSeq !== commitLoadSeq ||
      repoId !== selectedRepoId.value ||
      worktreePath !== selectedWorktreePath.value ||
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
  await loadCommitDetail(
    commit.hash,
    commit.historyBranchName ? commit.filePath : ""
  )
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
      worktreePath: selectedWorktreePath.value,
      sourceBranchName: selectedBranch.value,
      targetBranchName: project.value.checkBranchName,
      commitHash: commit.hash,
      subject: commit.subject,
      date: commit.date
    })
    // 改动匹配使用独立状态，区分提交历史确认与标题时间的疑似匹配。
    const checkStatus =
      {
        hash: "exists-hash",
        "subject-date": "exists-subject",
        "similar-content": "exists-content"
      }[result?.matchedBy] || "missing"

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
    createMessage.success("完整哈希 已复制。")
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

  const sequence = ++detailLoadSeq
  commitDetailLoading.value = true

  try {
    const detail = await gitToolApi.getGitToolCommitDetail({
      repoId: selectedRepoId.value,
      worktreePath: selectedWorktreePath.value,
      commitHash,
      filePath
    })
    if (sequence === detailLoadSeq) selectedCommitDetail.value = detail
  } catch (error) {
    if (sequence === detailLoadSeq) showErrorMessage(error)
  } finally {
    if (sequence === detailLoadSeq) commitDetailLoading.value = false
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
  if (branch.isCurrent || branch.worktreePath) {
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

function resolveConfirmDialog(val) {
  closeConfirmDialog(
    val !== undefined
      ? val
      : confirmDialog.value.input
        ? confirmDialog.value.inputValue
        : true
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
  const successBranches = []
  const failedBranches = []

  try {
    for (const branchName of branchNames) {
      try {
        await gitToolApi.archiveGitToolBranch({
          repoId: selectedRepoId.value,
          worktreePath: selectedWorktreePath.value,
          branchName
        })
        successBranches.push(branchName)
      } catch (error) {
        failedBranches.push({
          branchName,
          error
        })
      }
    }

    selectedBranchNames.value = failedBranches.map((item) => item.branchName)
    await refreshGitProject()

    if (successBranches.length) {
      createMessage.success(
        failedBranches.length
          ? `成功归档 ${successBranches.length} 个分支，${failedBranches.length} 个分支归档失败。`
          : `成功归档 ${successBranches.length} 个分支。`
      )
    }

    if (failedBranches.length) {
      console.error("[GitTool] 批量归档失败:", failedBranches)
      const firstError = failedBranches[0].error
      const firstMessage =
        firstError?.data?.message ||
        firstError?.message ||
        firstError?.toString() ||
        "操作失败"

      createMessage.error(
        `归档失败：${failedBranches
          .map((item) => item.branchName)
          .join("、")}；${firstMessage}`
      )
    }
  } finally {
    gitLoading.value = false
  }
}

async function handleCheckBranchChange(branchName) {
  if (!selectedRepoId.value || checkBranchLoading.value) {
    return
  }

  const repoId = selectedRepoId.value
  checkBranchLoading.value = true
  try {
    const updatedProject = await gitToolApi.updateGitToolCheckBranch({
      repoId,
      branchName
    })
    // 切换仓库后不再回写旧项目；文件历史由面板监听配置后刷新。
    if (repoId !== selectedRepoId.value) return
    project.value = updatedProject
    if (gitWorkspace.value === "branch") await loadCommits()
  } catch (error) {
    showErrorMessage(error)
  } finally {
    checkBranchLoading.value = false
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
      worktreePath: selectedWorktreePath.value,
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
        worktreePath: selectedWorktreePath.value,
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

  const sequence = ++stashLoadSeq
  const repoId = selectedRepoId.value
  const worktreePath = selectedWorktreePath.value
  stashLoading.value = true

  try {
    const [nextStashes, nextArchives] = await Promise.all([
      gitToolApi.listGitToolStashes({ repoId, worktreePath }),
      gitToolApi.listGitToolStashArchives({ repoId })
    ])
    if (sequence !== stashLoadSeq) return
    stashes.value = nextStashes
    stashArchives.value = nextArchives
    selectedStashHashes.value = selectedStashHashes.value.filter((stashHash) =>
      stashes.value.find((item) => item.hash === stashHash)
    )
    stashLoaded = true
  } catch (error) {
    if (sequence === stashLoadSeq) showErrorMessage(error)
  } finally {
    if (sequence === stashLoadSeq) stashLoading.value = false
  }
}

async function openStashDetail(stash) {
  detailDrawerType.value = "stash"
  selectedStash.value = stash
  selectedStashArchive.value = null
  stashDetail.value = null
  const sequence = ++stashDetailLoadSeq
  stashDetailLoading.value = true

  try {
    const detail = await gitToolApi.getGitToolStashDetail({
      repoId: selectedRepoId.value,
      worktreePath: selectedWorktreePath.value,
      stashHash: stash.hash,
      filePath: ""
    })
    if (sequence === stashDetailLoadSeq) stashDetail.value = detail
  } catch (error) {
    if (sequence === stashDetailLoadSeq) showErrorMessage(error)
  } finally {
    if (sequence === stashDetailLoadSeq) stashDetailLoading.value = false
  }
}

async function openStashArchiveDetail(archive) {
  detailDrawerType.value = "stash"
  selectedStash.value = null
  selectedStashArchive.value = archive
  stashDetail.value = null
  const sequence = ++stashDetailLoadSeq
  stashDetailLoading.value = true

  try {
    const detail = await gitToolApi.getGitToolStashArchiveDetail({
      stashArchiveId: archive.stashArchiveId,
      filePath: ""
    })
    if (sequence === stashDetailLoadSeq) stashDetail.value = detail
  } catch (error) {
    if (sequence === stashDetailLoadSeq) showErrorMessage(error)
  } finally {
    if (sequence === stashDetailLoadSeq) stashDetailLoading.value = false
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
  const sequence = ++stashDetailLoadSeq
  stashDetailLoading.value = true

  try {
    const detail = await gitToolApi.getGitToolStashDetail({
      repoId: selectedRepoId.value,
      worktreePath: selectedWorktreePath.value,
      stashHash: selectedStash.value.hash,
      filePath
    })
    if (sequence === stashDetailLoadSeq) stashDetail.value = detail
  } catch (error) {
    if (sequence === stashDetailLoadSeq) showErrorMessage(error)
  } finally {
    if (sequence === stashDetailLoadSeq) stashDetailLoading.value = false
  }
}

async function openStashArchiveDetailFile(filePath) {
  const sequence = ++stashDetailLoadSeq
  stashDetailLoading.value = true

  try {
    const detail = await gitToolApi.getGitToolStashArchiveDetail({
      stashArchiveId: selectedStashArchive.value.stashArchiveId,
      filePath
    })
    if (sequence === stashDetailLoadSeq) stashDetail.value = detail
  } catch (error) {
    if (sequence === stashDetailLoadSeq) showErrorMessage(error)
  } finally {
    if (sequence === stashDetailLoadSeq) stashDetailLoading.value = false
  }
}

async function archiveStash(stash) {
  if (
    !(await confirmGitAction({
      title: "归档贮藏",
      description: "归档后会从当前 贮藏列表 中移除。",
      message: `归档成功后，项目中的「${stash.stashRef}」会从 贮藏列表 中删除。`,
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
      worktreePath: selectedWorktreePath.value,
      stashRef: stash.stashRef,
      stashHash: stash.hash
    })
    createMessage.success("贮藏已归档。")
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
      title: "归档贮藏",
      description: "归档后会从当前 贮藏列表 中移除。",
      message: `归档成功后，${selectedStashes.length} 条贮藏记录 会从 贮藏列表 中删除。`,
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
        worktreePath: selectedWorktreePath.value,
        stashRef: stash.stashRef,
        stashHash: stash.hash
      })
    }

    createMessage.success("选中 贮藏已归档。")
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
      title: "恢复贮藏",
      description: "恢复后会重新写入当前项目的 贮藏列表。",
      message: `确认恢复「${archive.stashRef}」到 贮藏列表 吗？`,
      detail: archive.message,
      confirmText: "恢复"
    }))
  ) {
    return
  }

  gitLoading.value = true

  try {
    await gitToolApi.restoreGitToolStashArchive({
      stashArchiveId: archive.stashArchiveId,
      worktreePath: selectedWorktreePath.value
    })
    createMessage.success("贮藏已恢复。")
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
      title: "删除 贮藏归档",
      description: "删除后无法在归档列表中恢复。",
      message: `确认删除贮藏 归档「${archive.stashRef}」吗？`,
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
    createMessage.success("贮藏归档已删除。")
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

  .git-tool-picker {
    flex-shrink: 0;
    --el-fill-color-blank: var(--color-panel);
    --el-fill-color-light: var(--color-panel-soft);
    --el-border-color: var(--color-line);
    --el-border-color-hover: var(--color-line-strong);
    --el-color-primary: var(--color-primary);
    --el-text-color-regular: var(--color-text);
    --el-text-color-placeholder: var(--color-text-soft);

    &.git-tool-repo-picker {
      width: clamp(150px, 13vw, 200px);
    }

    &.git-tool-worktree-picker {
      width: clamp(185px, 16vw, 250px);
    }

    :deep(.el-select__wrapper) {
      min-height: 36px;
      padding: 6px 10px;
      border-radius: 8px;
      font-size: 13px;
    }

    // 聚焦提示由外层选择框显示，内部搜索框不重复绘制边框。
    :deep(.el-select__input) {
      border: 0;
      outline: none;
      box-shadow: none;
      background: transparent;
    }

    .git-tool-picker-icon {
      flex-shrink: 0;
      color: var(--color-primary);
    }
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

      .git-tool-commit-graph-cell
        .git-tool-commit-graph-svg
        .git-tool-commit-graph-node {
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

.git-tool-check-exists-hash,
.git-tool-check-exists-content {
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
    transition:
      border-color 0.2s ease,
      background-color 0.2s ease,
      box-shadow 0.2s ease;

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
      transition:
        border-color 0.2s ease,
        box-shadow 0.2s ease;
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
      transition:
        border-color 0.2s ease,
        background-color 0.2s ease,
        box-shadow 0.2s ease;

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
.git-tool {
  .git-tool-top {
    .git-tool-top-card {
      .git-tool-operation-bar {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding-top: 12px;
        margin-top: 12px;
        border-top: 1px solid var(--color-line);

        .git-tool-operation-status {
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;
          color: var(--color-primary);

          .git-tool-operation-branch {
            font-size: 12px;
            white-space: nowrap;
          }
          .git-tool-operation-upstream {
            font-size: 11px;
            color: var(--color-text-muted);
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
          .git-tool-operation-count {
            font-size: 11px;
            color: var(--color-text-muted);
            white-space: nowrap;
          }
          .git-tool-operation-dirty {
            padding: 3px 7px;
            border: 1px solid var(--color-line);
            border-radius: 4px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            font-size: 11px;
            white-space: nowrap;
            cursor: pointer;
          }
        }
        .git-tool-operation-actions {
          display: flex;
          flex-wrap: wrap;
          max-width: 100%;
          flex: none;
          align-items: center;
          gap: 5px;

          .git-tool-operation-button {
            padding: 6px 9px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            background: var(--color-panel);
            color: var(--color-text);
            font-size: 12px;
            cursor: pointer;
            &:hover:not(:disabled) {
              border-color: var(--color-primary);
              color: var(--color-primary);
            }
            &:disabled {
              opacity: 0.45;
              cursor: default;
            }
          }
        }
      }
    }
  }

  .git-tool-operation-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--color-warning-line);
    border-radius: 8px;
    background: var(--color-warning-soft);
    flex: none;

    .git-tool-operation-banner-info {
      display: flex;
      flex-direction: column;
      gap: 5px;

      .git-tool-operation-banner-title {
        font-size: 13px;
        color: var(--color-warning);
      }
      .git-tool-operation-banner-description {
        font-size: 12px;
        color: var(--color-text-muted);
      }
    }
    .git-tool-operation-banner-actions {
      display: flex;
      gap: 6px;
      flex: none;

      .git-tool-operation-banner-button {
        padding: 6px 9px;
        border: 1px solid var(--color-warning-line);
        border-radius: 5px;
        background: var(--color-panel);
        color: var(--color-warning);
        cursor: pointer;
        font-size: 12px;
        &:disabled {
          opacity: 0.45;
          cursor: default;
        }
      }
    }
  }

  .git-tool-operation-output {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel-soft);
    flex: none;

    &.git-tool-operation-output-error {
      border-color: var(--color-danger-line);
    }
    .git-tool-operation-output-label {
      flex: none;
      font-size: 12px;
      color: var(--color-primary);
    }
    .git-tool-operation-output-text {
      flex: 1;
      min-width: 0;
      max-height: 100px;
      margin: 0;
      overflow: auto;
      white-space: pre-wrap;
      word-break: break-all;
      font-size: 12px;
      color: var(--color-text-muted);
    }
    .git-tool-operation-output-close {
      display: inline-flex;
      border: 0;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
    }
  }

  .git-tool-stash-workbench {
    .git-tool-stash-panel {
      .git-tool-stash-list {
        .git-tool-stash-card {
          .git-tool-stash-operation-actions {
            display: flex;
            flex: none;
            gap: 4px;

            .git-tool-stash-operation-button {
              padding: 5px 8px;
              border: 1px solid var(--color-line);
              border-radius: 4px;
              background: var(--color-panel);
              color: var(--color-primary);
              cursor: pointer;
              font-size: 12px;
              &:disabled {
                opacity: 0.45;
                cursor: default;
              }
            }
          }
        }
      }
    }
  }

  .git-operation-modal {
    :deep(.base-modal__panel) {
      width: 650px;
    }
    :deep(.base-modal__content) {
      overflow: auto;
    }
  }
}
</style>

<!-- 下拉层挂载到页面根部，用专属类名限定样式范围。 -->
<style lang="less">
.git-tool-picker-popup.el-popper {
  width: min(440px, calc(100vw - 32px));
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 12px;
  background: var(--color-panel);
  box-shadow: var(--shadow-panel);
  --el-bg-color-overlay: var(--color-panel);
  --el-text-color-secondary: var(--color-text-muted);

  &.git-tool-worktree-popup {
    width: min(560px, calc(100vw - 32px));
  }

  .el-select-dropdown {
    width: 100%;
    max-width: 100%;

    .el-select-dropdown__header {
      padding: 11px 14px;
      border-bottom: 1px solid var(--color-line);

      .git-tool-picker-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        color: var(--color-text-muted);
        font-size: 12px;
      }
    }

    .el-select-dropdown__wrap {
      max-height: 320px;

      .el-select-dropdown__list {
        padding: 6px;

        .el-select-dropdown__item {
          height: auto;
          min-height: 62px;
          margin: 2px 0;
          padding: 10px;
          border: 1px solid transparent;
          border-radius: 8px;
          color: var(--color-text);
          font: inherit;
          font-size: 13px;
          line-height: 1.4;
          white-space: normal;
          transition: background-color 0.15s ease;

          &.is-hovering:not(.is-disabled):not(.is-selected) {
            background: var(--color-panel-soft);
          }

          &.is-selected {
            border-color: var(--color-info-line);
            background: var(--color-primary-soft);
            color: var(--color-primary);
          }

          &.is-disabled {
            opacity: 0.5;
            cursor: not-allowed;
          }

          .git-tool-picker-option {
            min-width: 0;

            .git-tool-picker-option-heading {
              display: flex;
              align-items: center;
              gap: 7px;
              min-width: 0;

              .git-tool-picker-option-icon,
              .git-tool-picker-check {
                flex-shrink: 0;
                color: var(--color-primary);
              }

              .git-tool-picker-option-name {
                min-width: 0;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
              }

              .git-tool-picker-check {
                margin-left: auto;
              }

              .git-tool-picker-tag {
                flex-shrink: 0;
                padding: 1px 5px;
                border: 1px solid var(--color-line);
                border-radius: 4px;
                background: var(--color-panel);
                color: var(--color-text-muted);
                font-size: 10px;
                white-space: nowrap;

                &.git-tool-picker-tag-warning {
                  border-color: var(--color-warning-line);
                  background: var(--color-warning-soft);
                  color: var(--color-warning);
                }
              }
            }

            .git-tool-picker-option-path {
              display: block;
              margin: 5px 0 0 22px;
              overflow: hidden;
              color: var(--color-text-muted);
              font-size: 12px;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
          }
        }
      }
    }

    .el-select-dropdown__empty {
      padding: 24px 14px;
      color: var(--color-text-muted);
      font-size: 13px;
    }
  }
}
</style>

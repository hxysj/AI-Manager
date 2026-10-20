import { request } from "@/api/request"

export const gitToolApi = {
  // 工作区读取与写操作沿用统一 IPC，请求结果包含最新仓库状态。
  getGitToolStatus: (payload) => request("git-tool:status", payload),
  getGitToolWorktreeDiff: (payload) =>
    request("git-tool:worktree-diff", payload),
  getGitToolBranchDiff: (payload) => request("git-tool:branch-diff", payload),
  previewGitToolRevision: (payload) =>
    request("git-tool:revision-preview", payload),
  executeGitToolOperation: (payload) => request("git-tool:operation", payload),
  scanGitToolBranches: (payload) => request("git-tool:branches", payload),
  listGitToolCommits: (payload) => request("git-tool:commits", payload),
  // 文件历史按页读取，沿用仓库和当前工作树的上下文。
  listGitToolFileHistory: (payload) =>
    request("git-tool:file-history", payload),
  listGitToolWorktreeFiles: (payload) =>
    request("git-tool:worktree-files", payload),
  getGitToolCommitDetail: (payload) =>
    request("git-tool:commit-detail", payload),
  updateGitToolCheckBranch: (payload) =>
    request("git-tool:update-check-branch", payload),
  clearGitToolCommitCheckCache: (payload) =>
    request("git-tool:clear-check-cache", payload),
  checkGitToolCommitOnBranch: (payload) =>
    request("git-tool:check-commit", payload),
  archiveGitToolBranch: (payload) =>
    request("git-tool:archive-branch", payload),
  listGitToolArchives: (payload) => request("git-tool:archives", payload),
  listGitToolArchiveCommits: (payload) =>
    request("git-tool:archive-commits", payload),
  getGitToolArchiveCommitDetail: (payload) =>
    request("git-tool:archive-commit-detail", payload),
  restoreGitToolArchive: (payload) =>
    request("git-tool:restore-archive", payload),
  deleteGitToolArchive: (payload) =>
    request("git-tool:delete-archive", payload),
  listGitToolStashes: (payload) => request("git-tool:stashes", payload),
  listGitToolStashArchives: (payload) =>
    request("git-tool:stash-archives", payload),
  getGitToolStashDetail: (payload) => request("git-tool:stash-detail", payload),
  getGitToolStashArchiveDetail: (payload) =>
    request("git-tool:stash-archive-detail", payload),
  archiveGitToolStash: (payload) => request("git-tool:archive-stash", payload),
  restoreGitToolStashArchive: (payload) =>
    request("git-tool:restore-stash-archive", payload),
  deleteGitToolStashArchive: (payload) =>
    request("git-tool:delete-stash-archive", payload)
}

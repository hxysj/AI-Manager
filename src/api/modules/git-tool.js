import { request } from '../request'

export const gitToolApi = {
  scanGitToolBranches: payload => request('git-tool:branches', payload),
  listGitToolCommits: payload => request('git-tool:commits', payload),
  getGitToolCommitDetail: payload =>
    request('git-tool:commit-detail', payload),
  updateGitToolCheckBranch: payload =>
    request('git-tool:update-check-branch', payload),
  clearGitToolCommitCheckCache: payload =>
    request('git-tool:clear-check-cache', payload),
  checkGitToolCommitOnBranch: payload =>
    request('git-tool:check-commit', payload),
  archiveGitToolBranch: payload => request('git-tool:archive-branch', payload),
  listGitToolArchives: payload => request('git-tool:archives', payload),
  listGitToolArchiveCommits: payload =>
    request('git-tool:archive-commits', payload),
  getGitToolArchiveCommitDetail: payload =>
    request('git-tool:archive-commit-detail', payload),
  restoreGitToolArchive: payload =>
    request('git-tool:restore-archive', payload),
  deleteGitToolArchive: payload => request('git-tool:delete-archive', payload),
  listGitToolStashes: payload => request('git-tool:stashes', payload),
  listGitToolStashArchives: payload =>
    request('git-tool:stash-archives', payload),
  getGitToolStashDetail: payload => request('git-tool:stash-detail', payload),
  getGitToolStashArchiveDetail: payload =>
    request('git-tool:stash-archive-detail', payload),
  archiveGitToolStash: payload => request('git-tool:archive-stash', payload),
  restoreGitToolStashArchive: payload =>
    request('git-tool:restore-stash-archive', payload),
  deleteGitToolStashArchive: payload =>
    request('git-tool:delete-stash-archive', payload)
}

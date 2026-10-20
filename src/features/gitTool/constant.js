// 操作名称和影响说明供工具栏、右键菜单与操作表单共用。
export const gitOperationDefinitions = {
  "create-branch": {
    label: "创建分支",
    description: "从指定提交创建本地分支，可以同时切换到新分支。",
    command:
      "git branch <新分支> <提交>；创建后切换：git switch -c <新分支> <提交>"
  },
  "switch-branch": {
    label: "切换分支",
    description:
      "在所选工作树切换本地分支，遇到可能覆盖未提交改动的情况会停止。",
    command: "git switch <分支>"
  },
  "checkout-commit": {
    label: "切换到提交",
    description:
      "切换到指定提交或标签，暂时脱离分支；继续开发前可从该提交创建分支。",
    command: "git switch --detach <提交或标签>"
  },
  "rename-branch": {
    label: "重命名分支",
    description: "修改本地分支名称，远程分支名称保持原样。",
    command: "git branch -m <原分支> <新分支>"
  },
  "stash-save": {
    label: "保存改动",
    description: "把当前未提交的改动保存为贮藏记录，并清理已保存的工作区改动。",
    command:
      "git stash push；包含未跟踪文件：git stash push --include-untracked"
  },
  "stash-apply": {
    label: "应用贮藏",
    description: "把保存的改动应用到所选工作树，并保留贮藏记录。",
    command:
      "git stash apply <贮藏记录>；恢复暂存状态：git stash apply --index <贮藏记录>"
  },
  "stash-pop": {
    label: "应用并移除",
    description:
      "把改动应用到所选工作树，成功后移除贮藏记录；发生冲突时保留记录。",
    command:
      "git stash pop <贮藏记录>；恢复暂存状态：git stash pop --index <贮藏记录>"
  },
  fetch: {
    label: "获取更新",
    description:
      "获取远程提交并清理失效的远程跟踪引用，不修改当前工作树的文件。",
    command: "git fetch --prune <远程仓库>"
  },
  pull: {
    label: "拉取",
    description: "拉取远程分支并按照选定策略整合到当前分支。",
    command:
      "git pull --ff-only <远程仓库> <远程分支>；其他策略：--no-rebase / --rebase"
  },
  push: {
    label: "推送",
    description: "推送当前分支，并为它设置所选远程分支为上游。",
    command: "git push --set-upstream <远程仓库> <当前分支>:<远程分支>"
  },
  merge: {
    label: "合并",
    description: "把目标分支或提交合并到当前分支，可能需要解决冲突。",
    command: "git merge <分支或提交>"
  },
  "cherry-pick": {
    label: "拣选提交",
    description: "把所选提交的修改应用到当前分支，生成新的提交。",
    command: "git cherry-pick <提交>"
  },
  revert: {
    label: "撤销提交",
    description: "新增一条提交来撤销目标提交的修改，保留原有历史。",
    command: "git revert --no-edit <提交>"
  },
  reset: {
    label: "重置",
    description: "把当前分支移动到目标提交，工作区和暂存区的处理由模式决定。",
    command: "git reset --soft <提交>；其他模式：--mixed / --hard"
  }
}

// 分支列表与文件历史共用日期、状态和判断理由。
export function formatGitGraphDate(value) {
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

export function formatCheckStatus(status) {
  const statusMap = {
    "exists-hash": "已合入",
    "exists-subject": "疑似已合入",
    "exists-content": "改动一致",
    missing: "未合入"
  }

  return statusMap[status] || ""
}

// 同时展示目标分支与判断依据，避免将疑似匹配理解为已验证合入。
export function formatCheckReason(commit) {
  const branch = commit.checkTargetBranch
    ? `「${commit.checkTargetBranch}」`
    : "目标分支"
  const reasonMap = {
    "exists-hash": `该提交位于${branch}的提交历史中，提交哈希一致。`,
    "exists-subject": `${branch}中找到提交标题和作者时间完全相同的提交，但哈希不同；尚未通过该规则验证代码改动一致。`,
    "exists-content": `${branch}中找到标题和作者名相似度均不低于 85%，且代码改动指纹一致的提交；保留空白差异，忽略行号偏移。`,
    missing: `${branch}中未找到相同提交哈希、标题和作者时间完全相同的提交，也未找到标题和作者名高度相似且代码改动一致的提交。`
  }

  return reasonMap[commit.checkStatus] || ""
}

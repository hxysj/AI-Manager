// 操作名称和影响说明供工具栏、右键菜单与操作表单共用。
export const gitOperationDefinitions = {
  'create-branch': {
    label: '创建分支',
    description: '从指定提交创建本地分支，可以同时切换到新分支。'
  },
  'switch-branch': {
    label: '切换分支',
    description: '切换实际工作区分支。Git 会拒绝覆盖尚未保存的改动。'
  },
  'rename-branch': {
    label: '重命名分支',
    description: '修改本地分支名称，远程分支名称保持原样。'
  },
  'stash-save': {
    label: '保存到 Stash',
    description: '保存当前未提交改动，恢复干净的已跟踪文件工作区。'
  },
  'stash-apply': {
    label: '应用 Stash',
    description: '把保存的改动应用到当前工作区，保留这条 Stash。'
  },
  'stash-pop': {
    label: '弹出 Stash',
    description: '应用成功后移除这条 Stash；发生冲突时保留记录。'
  },
  fetch: {
    label: 'Fetch',
    description: '获取远程提交并清理失效的远程跟踪引用。'
  },
  pull: {
    label: 'Pull',
    description: '拉取远程分支并按照选定策略整合到当前分支。'
  },
  push: {
    label: 'Push',
    description: '推送当前分支，并为它设置所选远程分支为上游。'
  },
  merge: {
    label: 'Merge',
    description: '把目标分支或提交合并到当前分支，可能需要解决冲突。'
  },
  'cherry-pick': {
    label: 'Cherry-pick',
    description: '把所选提交的修改应用到当前分支，生成新的提交。'
  },
  revert: {
    label: 'Revert',
    description: '新增一条提交来撤销目标提交的修改，保留原有历史。'
  },
  reset: {
    label: 'Reset',
    description: '把当前分支移动到目标提交，工作区和暂存区的处理由模式决定。'
  }
}

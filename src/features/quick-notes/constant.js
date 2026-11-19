// 便签、列表和助手速记共用名称，颜色只用于辅助辨识。
export const noteTypes = [
  { id: "note", label: "普通便签" },
  { id: "todo", label: "待办事项" },
  { id: "worklog", label: "工作留痕" }
]
export const taskStates = [
  { id: "pending", label: "待完成" },
  { id: "doing", label: "进行中" },
  { id: "completed", label: "已完成" },
  { id: "cancelled", label: "已取消" }
]
// 工作事项的整体状态独立于待办状态和归档状态。
export const workStates = [
  { id: "planned", label: "计划中" },
  { id: "in_progress", label: "进行中" },
  { id: "blocked", label: "已阻塞" },
  { id: "paused", label: "已暂停" },
  { id: "completed", label: "已完成" }
]
export function workStateLabel(tracking) {
  return (
    workStates.find((state) => state.id === tracking?.status)?.label || "未跟踪"
  )
}
export function workProgressError(tracking) {
  if (!tracking || tracking.status === "completed") return ""
  const { progress } = tracking
  if (progress == null) return ""
  if (Number.isInteger(progress) && progress >= 0 && progress < 100) return ""
  return "工作进度必须留空或填写 0 至 99 的整数"
}
export const repeatOptions = [
  { id: "once", label: "仅一次" },
  { id: "daily", label: "每天" },
  { id: "weekdays", label: "工作日" },
  { id: "weekly", label: "每周" },
  { id: "monthly", label: "每月（不存在的日期取月底）" }
]
export function localInput(value, withSeconds = false) {
  if (!value) return ""
  const date = new Date(value)
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, withSeconds ? 19 : 16)
}
// 提醒未被编辑时保留原始触发点，避免预览及重新保存丢失秒数和毫秒。
export function reminderAt(plan) {
  return plan.localAt === localInput(plan.nextFireAt, true)
    ? plan.nextFireAt
    : new Date(plan.localAt).getTime()
}
export function emptyNote() {
  return {
    type: "note",
    title: "",
    content: "",
    groupId: "inbox",
    categoryId: "uncategorized",
    tags: [],
    color: "sand",
    pinned: false,
    tasks: [],
    worklog: {},
    workTracking: null,
    progressNote: "",
    occurredOn: "",
    reminders: []
  }
}

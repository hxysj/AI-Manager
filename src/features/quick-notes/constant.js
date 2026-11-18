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
export const repeatOptions = [
  { id: "once", label: "仅一次" },
  { id: "daily", label: "每天" },
  { id: "weekdays", label: "工作日" },
  { id: "weekly", label: "每周" },
  { id: "monthly", label: "每月（不存在的日期取月底）" }
]
export function localInput(value) {
  if (!value) return ""
  const date = new Date(value)
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16)
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
    occurredOn: "",
    reminders: []
  }
}

// 指定日期覆盖周期规则；同优先级使用最近修改的规则。
export function calendarDay(date, rules) {
  const key = `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`
  const weekday = (date.getDay()+6)%7
  const matched = rules.filter(rule => {
    if (!rule.enabled || rule.calendar === 'lunar') return false
    if (rule.startDate && key < rule.startDate || rule.endDate && key > rule.endDate) return false
    if (rule.dateParity === 'odd' && date.getDate()%2 !== 1 || rule.dateParity === 'even' && date.getDate()%2 !== 0) return false
    if (rule.repeat === 'once') {
      // 跨天到午夜结束时，末日没有实际请假时长。
      if (rule.kind === 'leave' && key === rule.endDate && rule.endTime === '00:00') return false
      return key >= rule.startDate && key <= (rule.endDate || rule.startDate)
    }
    if (rule.repeat === 'monthly') return rule.monthDays.includes(date.getDate())
    if (rule.repeat === 'yearly') return rule.month === date.getMonth()+1 && rule.day === date.getDate()
    if (!rule.weekdays.includes(weekday)) return false
    if (rule.repeat === 'alternating') {
      const anchor = new Date(`${rule.anchorDate}T12:00:00`)
      const serial = value => Date.UTC(value.getFullYear(),value.getMonth(),value.getDate())/86400000
      const monday = value => serial(value)-(value.getDay()+6)%7
      return ((Math.floor((monday(date)-monday(anchor))/7)%2)+2)%2 === rule.cycleWeek
    }
    return rule.repeat === 'weekly'
  }).sort((a,b) => (b.repeat === 'once')-(a.repeat === 'once') || b.priority-a.priority || b.updatedAt-a.updatedAt)
  // 请假叠加到原排班，保留同一天的休息、补班和其他标记。
  return {
    schedule: matched.find(rule => rule.kind !== 'leave') || null,
    leaves: matched.filter(rule => rule.kind === 'leave')
  }
}

// 跨日时段只在首末日标明时刻，区间内其余日期为全天。
export function leaveDayLabel(rule, key) {
  if (!rule.startTime) return '全天'
  const endDate = rule.endDate || rule.startDate
  if (rule.startDate === endDate) return `${rule.startTime}–${rule.endTime}`
  if (key === rule.startDate) return `${rule.startTime} 起`
  if (key === endDate) return `至 ${rule.endTime}`
  return '全天'
}

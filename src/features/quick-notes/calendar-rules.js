// 指定日期覆盖周期规则；同优先级使用最近修改的规则。
export function calendarDay(date, rules) {
  const key = `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`
  const weekday = (date.getDay()+6)%7
  const matched = rules.filter(rule => {
    if (!rule.enabled || rule.calendar === 'lunar') return false
    if (rule.startDate && key < rule.startDate || rule.endDate && key > rule.endDate) return false
    if (rule.dateParity === 'odd' && date.getDate()%2 !== 1 || rule.dateParity === 'even' && date.getDate()%2 !== 0) return false
    if (rule.repeat === 'once') return key >= rule.startDate && key <= (rule.endDate || rule.startDate)
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
  return matched[0] || null
}

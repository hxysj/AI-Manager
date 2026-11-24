import { LunarMonth } from "lunar-javascript"

// 同一档内先比较指定日期和优先级，最后比较修改时间。
export function calendarDay(date, rules, lunar, publicSchedule) {
  const key = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`
  const weekday = (date.getDay() + 6) % 7
  const matched = rules
    .filter((rule) => {
      if (!rule.enabled) return false
      if (
        (rule.startDate && key < rule.startDate) ||
        (rule.endDate && key > rule.endDate)
      )
        return false
      if (
        (rule.dateParity === "odd" && date.getDate() % 2 !== 1) ||
        (rule.dateParity === "even" && date.getDate() % 2 !== 0)
      )
        return false
      if (rule.calendar === "lunar") return matchesLunarRule(rule, lunar)
      if (rule.repeat === "once") {
        // 跨天到午夜结束时，末日没有实际请假时长。
        if (
          rule.kind === "leave" &&
          key === rule.endDate &&
          rule.endTime === "00:00"
        )
          return false
        return key >= rule.startDate && key <= (rule.endDate || rule.startDate)
      }
      if (rule.repeat === "monthly")
        return rule.monthDays.includes(date.getDate())
      if (rule.repeat === "yearly")
        return rule.month === date.getMonth() + 1 && rule.day === date.getDate()
      if (!rule.weekdays.includes(weekday)) return false
      if (rule.repeat === "alternating") {
        const anchor = new Date(`${rule.anchorDate}T12:00:00`)
        const serial = (value) =>
          Date.UTC(value.getFullYear(), value.getMonth(), value.getDate()) /
          86400000
        const monday = (value) => serial(value) - ((value.getDay() + 6) % 7)
        return (
          ((Math.floor((monday(date) - monday(anchor)) / 7) % 2) + 2) % 2 ===
          rule.cycleWeek
        )
      }
      return rule.repeat === "weekly"
    })
    .sort(
      (a, b) =>
        (b.repeat === "once") - (a.repeat === "once") ||
        b.priority - a.priority ||
        b.updatedAt - a.updatedAt
    )
  const schedules = matched.filter((rule) => rule.kind !== "leave")
  const leaves = matched.filter((rule) => rule.kind === "leave")
  const customHoliday = schedules.find((rule) =>
    ["holiday", "workday", "makeup"].includes(rule.kind)
  )
  // 自定义节假日与调休、公共安排、常规排班依次匹配。
  return {
    schedule: customHoliday || publicSchedule || schedules[0] || null,
    leaves
  }
}

// 农历月份用负数表示闰月，小月缺日按规则指定的方式处理。
function matchesLunarRule(rule, lunar) {
  const year = lunar.getYear()
  const month = lunar.getMonth()
  const day = lunar.getDay()
  if (month < 0 ? rule.leapMonth === "regular" : rule.leapMonth === "leap")
    return false
  if (rule.repeat === "once" && year !== rule.year) return false
  if (
    (rule.repeat === "once" || rule.repeat === "yearly") &&
    Math.abs(month) !== rule.month
  )
    return false
  const target = rule.repeat === "monthly" ? rule.monthDays : [rule.day]
  const lastDay =
    rule.missingDay === "last-day"
      ? LunarMonth.fromYm(year, month).getDayCount()
      : 30
  return target.some((value) => Math.min(value, lastDay) === day)
}

// 跨日时段只在首末日标明时刻，区间内其余日期为全天。
export function leaveDayLabel(rule, key) {
  if (!rule.startTime) return "全天"
  const endDate = rule.endDate || rule.startDate
  if (rule.startDate === endDate) return `${rule.startTime}–${rule.endTime}`
  if (key === rule.startDate) return `${rule.startTime} 起`
  if (key === endDate) return `至 ${rule.endTime}`
  return "全天"
}

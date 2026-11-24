import { HolidayUtil } from "lunar-javascript"

const requests = new Map()

// 年度数据按国务院通知整理，提供可直接访问的 JSON。
export function loadPublicHolidayYear(year, refresh = false) {
  if (refresh || !requests.has(year)) {
    const request = fetch(
      `https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/${year}.json`,
      { credentials: "omit" }
    )
      .then(async (response) => {
        if (!response.ok) return null
        const data = await response.json()
        if (data.year !== year || !Array.isArray(data.days) || !data.days.length)
          return null
        if (!data.days.every(({ date, name, isOffDay }) =>
          /^\d{4}-\d{2}-\d{2}$/.test(date) &&
          typeof name === "string" &&
          name.trim() &&
          typeof isOffDay === "boolean"
        )) return null
        return data.days.map(({ date, name, isOffDay }) =>
          publicSchedule(date, name, isOffDay)
        )
      })
      .catch(() => null)
    requests.set(year, request)
  }
  return requests.get(year)
}

// 联网结果到达前和离线时均使用随应用发布的已知日期。
export function bundledPublicHolidayYear(year) {
  return HolidayUtil.getHolidays(year).map((day) =>
    publicSchedule(day.getDay(), day.getName(), !day.isWork())
  )
}

function publicSchedule(date, title, isOffDay) {
  return {
    date,
    title,
    kind: isOffDay ? "holiday" : "workday",
    workStatus: isOffDay ? "rest" : "work",
    color: isOffDay ? "var(--color-danger)" : "var(--color-warning)",
    source: "public"
  }
}

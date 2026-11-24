import { z } from "zod"
import { Solar } from "lunar-javascript"
import modules from "./modules.json" with { type: "json" }
import operations from "./app-api.json" with { type: "json" }
import {
  calendarDay,
  leaveDayLabel
} from "../../../src/features/quick-notes/calendar-rules.js"
import {
  bundledPublicHolidayYear,
  loadPublicHolidayYear
} from "../../../src/features/calendar/public-holidays.js"
import { compareStrings } from "../../../src/features/tools/stringDiff/compare-strings.js"
import { extractImageLinks } from "../../../src/features/tools/imageLinkExtractor/extract-image-links.js"
import {
  buildBoxes,
  normalizeQuestionItems
} from "../../../src/features/tools/coordinateOverlay/question-boxes.js"

// 日期文本按应用本地时区换算，避免让模型自行计算时间戳。
function calendarDate(key) {
  if (typeof key === "string" && /^\d{4}-\d{2}-\d{2}$/.test(key)) {
    const date = new Date(`${key}T00:00:00`)
    if (Number.isFinite(+date) && date.getDate() === Number(key.slice(8)))
      return date
  }
  throw new Error("请选择有效的日历日期（YYYY-MM-DD）")
}

// 动作枚举让模型首轮即可选择业务工具，详细业务字段仍以现有接口契约为准。
export const moduleTools = Object.entries(modules).map(
  ([module, definition]) => {
    const items = operations
      .filter((item) => item.module === module)
      .map((item) => ({
        ...item,
        action: item.action || item.channel.split(":")[1]
      }))
    return {
      id: definition.id,
      description: `${definition.description}。后台直接执行。动作：${items
        .map(({ action, payload }) => {
          const fields = Object.keys(payload)
          return fields.length ? `${action}（${fields.join("、")}）` : action
        })
        .join("；")}。参数细节可用 describe 查询。`,
      inputSchema: z.object({
        action: z.enum(["describe", ...items.map((item) => item.action)]),
        payload: z.record(z.unknown()).default({})
      }),
      execute: async ({ action, payload }, nativeTool, signal) => {
        if (action === "describe")
          return items.filter(
            (item) => !payload.action || item.action === payload.action
          )
        const item = items.find((item) => item.action === action)
        if (module === "calendar" && action === "entries") {
          const { from, until } = payload
          const startAt = +calendarDate(from)
          const endAt = +calendarDate(until)
          return nativeTool(
            definition.id,
            { action, payload: { from, until, startAt, endAt } },
            signal
          )
        }
        if (item.channel === "local:usage-day") {
          const date = calendarDate(payload.date)
          const startAt = +date
          date.setDate(date.getDate() + 1)
          return nativeTool(
            "app_usage",
            {
              action: "stats",
              payload: { statsScope: "calendar", startAt, endAt: +date - 1 }
            },
            signal
          )
        }
        if (item.channel === "local:day") {
          const key = payload.date
          const date = calendarDate(key)
          const year = date.getFullYear()
          const lunar = Solar.fromYmd(
            year,
            date.getMonth() + 1,
            date.getDate()
          ).getLunar()
          const { rules } = await nativeTool(
            definition.id,
            { action: "calendar-rule-list", payload: {} },
            signal
          )
          const holidays = await loadPublicHolidayYear(year)
          const publicHoliday =
            (holidays ?? bundledPublicHolidayYear(year)).find(
              (day) => day.date === key
            ) ?? null
          const { schedule, leaves } = calendarDay(
            date,
            rules,
            lunar,
            publicHoliday
          )
          return {
            date: key,
            schedule,
            leaves: leaves.map((rule) => ({
              ...rule,
              timeLabel: leaveDayLabel(rule, key)
            })),
            publicHoliday,
            publicDataSource: holidays ? "online" : "bundled",
            lunar: lunar.toString(),
            suitable: lunar.getDayYi(),
            unsuitable: lunar.getDayJi()
          }
        }
        if (item.channel === "local:extract")
          return {
            links: extractImageLinks(
              z.string().max(40000).parse(payload.text)
            ).slice(0, 200)
          }
        if (item.channel === "local:compare") {
          const left = z.string().max(4000).parse(payload.left)
          const right = z.string().max(4000).parse(payload.right)
          const { rows, ...summary } = compareStrings(left, right)
          const differences = rows.filter((row) => !row.same)
          return {
            ...summary,
            totalRows: rows.length,
            differenceCount: differences.length,
            differences: differences.slice(0, 100)
          }
        }
        if (module === "json" || module === "coordinates") {
          const text = z.string().max(40000).parse(payload.text)
          let value
          try {
            value = JSON.parse(text)
          } catch (error) {
            return { valid: false, error: error.message }
          }
          if (action === "boxes") {
            const questions = normalizeQuestionItems(value)
            return {
              valid: questions.length > 0,
              count: questions.length,
              questions: questions
                .slice(0, 30)
                .map((question, index) =>
                  buildBoxes(question, index).slice(0, 100)
                )
            }
          }
          if (action === "validate") return { valid: true }
          return {
            valid: true,
            text: JSON.stringify(value, null, action === "format" ? 2 : 0)
          }
        }
        return nativeTool(definition.id, { action, payload }, signal)
      }
    }
  }
)

// 保留已有目录入口，旧版本会话仍可引用相同的接口。
export default [
  {
    id: "read_app_api",
    description:
      "查询软件可直接调用的内部接口及参数示例。便签日历的请假、休息日、排班及调休查询 calendar 模块。省略 module 返回模块目录，指定 module 返回接口详情。",
    inputSchema: z.object({ module: z.string().max(80).optional() })
  },
  {
    id: "call_app_api",
    description:
      "静默调用已通过 read_app_api 查到的内部接口，直接返回后台结果。channel 必须来自接口目录，payload 使用真实配置中的 ID。无需打开界面、读取控件或点击按钮；任务提交成功不代表任务完成，需调用查询或等待接口确认结果。",
    inputSchema: z.object({
      channel: z.string().min(1).max(120),
      payload: z.record(z.unknown()).default({})
    })
  }
]

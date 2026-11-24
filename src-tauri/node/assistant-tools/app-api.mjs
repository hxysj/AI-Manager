import { z } from "zod"

// 接口目录与原生路由共用一份清单，后台操作不依赖窗口或控件快照。
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

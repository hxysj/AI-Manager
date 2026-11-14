import { z } from "zod"

// 按能力集中维护工具定义，执行通过原生回调读取真实应用状态。
export default [
  {
    id: "set_pet_action",
    description:
      "指定桌宠动作或恢复自动播放；message 可更新桌宠气泡。仅使用用户要求的动作。",
    inputSchema: z.object({
      action: z.enum([
        "auto",
        "idle",
        "walk",
        "greeting",
        "jump",
        "read_book",
        "use_phone",
        "sleep",
        "think",
        "work",
        "work_laptop",
        "exercise",
        "salute",
        "steal",
        "peek",
        "run",
        "wake_up"
      ]),
      message: z.string().max(160).optional()
    })
  }
]

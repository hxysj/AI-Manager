import { z } from "zod"

// 按能力集中维护工具定义，执行通过原生回调读取真实应用状态。
export default [
{
      id: "update_app_settings",
      description:
        "按用户明确要求修改设置。仅允许 system、desktop、agents.translation；只传需要修改的字段。",
      inputSchema: z
        .object({
          system: z
            .object({
              closeAction: z.enum(["ask", "minimize", "quit"]).optional(),
              logEnabled: z.boolean().optional(),
              autoLaunchEnabled: z.boolean().optional(),
              quickSwitchVisible: z.boolean().optional(),
              quickSwitchTools: z.record(z.boolean()).optional()
            })
            .strict()
            .optional(),
          desktop: z
            .object({
              enabled: z.boolean().optional(),
              petId: z.enum(["monkey_animations"]).optional()
            })
            .strict()
            .optional(),
          agents: z
            .object({
              translation: z
                .object({
                  enabled: z.boolean().optional(),
                  targetId: z.string().max(200).optional(),
                  model: z.string().max(200).optional(),
                  sourceLanguage: z.string().max(100).optional(),
                  targetLanguage: z.string().max(100).optional(),
                  shortcut: z.string().max(80).optional(),
                  clipboardShortcut: z.string().max(80).optional()
                })
                .strict()
            })
            .strict()
            .optional()
        })
        .strict()
    }
]

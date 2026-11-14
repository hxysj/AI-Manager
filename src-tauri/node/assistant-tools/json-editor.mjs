import { z } from "zod"

// 按能力集中维护工具定义，执行通过原生回调读取真实应用状态。
export default [
  {
    id: "read_json_editor",
    description: "打开 JSON 工具并读取原文、当前格式化结果和校验错误。",
    inputSchema: z.object({})
  },
  {
    id: "write_json_editor",
    description:
      "把内容写入 JSON 工具。result 必须是完整合法 JSON；写入后回读校验结果。",
    inputSchema: z.object({
      source: z.string().max(60000).optional(),
      result: z.string().max(60000)
    })
  }
]

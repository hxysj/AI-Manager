import { z } from "zod"

// 按能力集中维护工具定义，执行通过原生回调读取真实应用状态。
export default [
  {
    id: "open_app_window",
    description:
      "显示并聚焦主面板或指定工具。应用助手在独立窗口打开，操作其他工具时对话仍保留。",
    inputSchema: z.object({
      toolId: z.enum([
        "main",
        "assistant",
        "json-agent",
        "image-workbench",
        "string-diff",
        "image-link-extractor",
        "coordinate-overlay",
        "port-monitor",
        "network-quality",
        "multi-protocol-gateway",
        "git",
        "lan-share",
        "codex-pets"
      ])
    })
  }
]

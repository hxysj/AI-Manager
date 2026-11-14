import { z } from "zod"

// 按能力集中维护工具定义，执行通过原生回调读取真实应用状态。
export default [
  {
    id: "open_app_window",
    description:
      "显示并聚焦主面板或指定工具，返回 windowLabel 供界面工具使用。首次创建的窗口会在页面就绪后显示，可用 read_app_ui 回读。应用助手在独立窗口打开，操作其他工具时对话仍保留。",
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

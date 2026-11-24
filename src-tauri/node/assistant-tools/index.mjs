import { createTool } from "@mastra/core/tools"
import config from "./config.mjs"
import settings from "./settings.mjs"
import windows from "./windows.mjs"
import pet from "./pet.mjs"
import jsonEditor from "./json-editor.mjs"
import skills from "./skills.mjs"
import ui from "./ui.mjs"
import appApi, { moduleTools } from "./app-api.mjs"
import catalog from "./catalog.json" with { type: "json" }
export { catalog }

// 注册、管理目录和模型可见工具使用同一组稳定 ID。
export function createTools(signal, nativeTool, disabled = []) {
  return Object.fromEntries(
    [
      ...config,
      ...settings,
      ...windows,
      ...pet,
      ...jsonEditor,
      ...skills,
      ...ui,
      ...appApi,
      ...moduleTools
    ]
      .filter(
        (definition) =>
          !disabled.includes(definition.id) &&
          !(
            definition.id.startsWith("app_") &&
            disabled.includes("call_app_api")
          )
      )
      .map((definition) => [
        definition.id,
        createTool({
          ...definition,
          execute: async (args) => {
            if (definition.execute)
              return definition.execute(args, nativeTool, signal)
            if (definition.id === "write_json_editor") JSON.parse(args.result)
            return nativeTool(definition.id, args, signal)
          }
        })
      ])
  )
}

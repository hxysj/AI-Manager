use serde_json::{json, Value};
use crate::core::{error::ManagerError, state::AppState};
use tauri::{AppHandle, Manager};

// 此模块只处理对应能力，任务身份和工具开关由统一入口校验。
pub(super) async fn execute(
    app: &AppHandle,
    args: &Value,
    surface: Option<&str>,
) -> Result<Value, ManagerError> {
    let state = app.state::<AppState>();
    let tool_id = args["toolId"].as_str().unwrap_or("");
    if tool_id != "main" && ![
        "assistant", "json-agent", "image-workbench", "string-diff", "image-link-extractor",
        "coordinate-overlay", "port-monitor", "network-quality",
        "multi-protocol-gateway", "git", "lan-share", "codex-pets",
    ].contains(&tool_id) {
        return Err(ManagerError::System("未知应用窗口".into()));
    }
    let independent = tool_id != "main"
        && (tool_id == "assistant" || surface == Some("desktop")
            || app.get_webview_window(&format!("tool-window-{tool_id}")).is_some());
    let result = if independent {
        crate::api::app::open_tool_window(app, tool_id)?
    } else {
        Box::pin(state.dispatch(app.clone(), "quick-switch:show-main",
            Some(if tool_id == "main" { json!({}) } else { json!({"toolId": tool_id}) }))).await?
    };
    // 首次创建工具窗口时尚未获得原生焦点，后续界面工具使用明确的窗口标识。
    Ok(json!({
        "windowLabel": if independent { format!("tool-window-{tool_id}") } else { "main".into() },
        "result": result
    }))
}

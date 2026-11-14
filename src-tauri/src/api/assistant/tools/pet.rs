use super::super::AssistantService;
use serde_json::{json, Value};
use crate::core::{error::ManagerError, state::AppState};
use tauri::{AppHandle, Manager};

// 此模块只处理对应能力，任务身份和工具开关由统一入口校验。
pub(super) async fn execute(service: &AssistantService, app: &AppHandle, args: &Value) -> Result<Value, ManagerError> {
    let state = app.state::<AppState>();
    let action = args["action"].as_str().unwrap_or("");
    if ![
        "auto", "idle", "walk", "greeting", "jump", "read_book", "use_phone",
        "sleep", "think", "work", "work_laptop", "exercise", "salute",
        "steal", "peek", "run", "wake_up",
    ].contains(&action) {
        return Err(ManagerError::System("桌宠动作不存在".into()));
    }
    let snapshot = state.state_snapshot().await;
    if snapshot["appSettings"]["desktop"]["enabled"] != true {
        return Err(ManagerError::System("桌宠未启用，请先在桌宠设置中启用".into()));
    }
    let message = args["message"].as_str().unwrap_or("");
    if message.chars().count() > 160 {
        return Err(ManagerError::System("桌宠气泡最多 160 个字符".into()));
    }
    let mut current = service.state.lock().await;
    current["petAction"] = json!(if action == "auto" { "" } else { action });
    current["petMessage"] = json!(message);
    drop(current);
    service.emit_state(app).await;
    Ok(json!({"action": action, "message": message}))
}

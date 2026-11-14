use super::super::AssistantService;
use super::redact_config;
use serde_json::{json, Value};
use crate::core::{error::ManagerError, state::AppState};
use tauri::{AppHandle, Manager};

// 此模块只处理对应能力，任务身份和工具开关由统一入口校验。
pub(super) async fn execute(service: &AssistantService, app: &AppHandle, args: &Value) -> Result<Value, ManagerError> {
    let state = app.state::<AppState>();
    let snapshot = state.state_snapshot().await;
    let result = match args["module"].as_str().unwrap_or("") {
        "overview" => json!({
            "name": "Monkey Thief",
            "settings": snapshot["appSettings"],
            "cliTargets": snapshot["cliTargets"],
            "paths": snapshot["paths"],
            "pet": service.state.lock().await["petAction"],
        }),
        "settings" => snapshot["appSettings"].clone(),
        "providers" => snapshot["providers"].clone(),
        "runtime" => json!({
            "models": snapshot["runtimeModels"], "profiles": snapshot["runtimeProfiles"],
            "state": snapshot["runtimeProviderState"],
            "schemas": snapshot["runtimeConfigSchemas"],
        }),
        "repos" => snapshot["repos"].clone(),
        "skills" => json!({
            "skills": snapshot["skills"], "groups": snapshot["skillGroups"],
            "repositories": snapshot["skillRepositories"],
        }),
        "rules" => snapshot["rules"].clone(),
        _ => return Err(ManagerError::System("未知配置模块".into())),
    };
    Ok(redact_config(result))
}

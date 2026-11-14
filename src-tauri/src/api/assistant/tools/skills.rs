use crate::core::{error::ManagerError, state::AppState};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

// 软件技能按软件中的名称读取，助手技能只允许加载自身已启用的正文。
pub(super) async fn execute(app: &AppHandle, args: &Value, name: &str) -> Result<Value, ManagerError> {
    let state = app.state::<AppState>();
    let snapshot = state.state_snapshot().await;
    if name == "read_assistant_skills" {
        let skills = snapshot["appSettings"]["agents"]["assistant"]["skills"].as_array().cloned().unwrap_or_default();
        if let Some(id) = args["id"].as_str() {
            return skills.into_iter().find(|skill| skill["id"] == id && skill["enabled"] != false).ok_or_else(|| ManagerError::System("助手技能不存在或已停用".into()));
        }
        return Ok(json!(skills.iter().map(|skill| json!({"id": skill["id"], "name": skill["name"], "description": skill["description"], "enabled": skill["enabled"]})).collect::<Vec<_>>()));
    }
    if let Some(skill_name) = args["skillName"].as_str() {
        let result = Box::pin(state.dispatch(app.clone(), "skill:content", Some(json!({"skillName":skill_name})))).await?;
        let text = result["content"].as_str().unwrap_or("");
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let limit = args["limit"].as_u64().unwrap_or(12000).clamp(1,24000) as usize;
        let total = text.chars().count();
        return Ok(json!({"skillName":skill_name,"content":text.chars().skip(offset).take(limit).collect::<String>(),"offset":offset,"totalCharacters":total,"hasMore":offset.saturating_add(limit)<total}));
    }
    Ok(super::redact_config(json!({"skills":snapshot["skills"],"groups":snapshot["skillGroups"],"repositories":snapshot["skillRepositories"]})))
}

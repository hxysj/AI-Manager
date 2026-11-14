use super::{redact_config, validate_settings_patch};
use serde_json::Value;
use crate::core::{error::ManagerError, state::AppState};
use tauri::{AppHandle, Manager};

// 此模块只处理对应能力，任务身份和工具开关由统一入口校验。
pub(super) async fn execute(app: &AppHandle, args: &Value) -> Result<Value, ManagerError> {
    let state = app.state::<AppState>();
    validate_settings_patch(args)?;
    let result = Box::pin(state.dispatch(
        app.clone(), "settings:save", Some(args.clone()),
    )).await?;
    Ok(redact_config(result))
}

use crate::core::error::ManagerError;
use crate::core::paths::path_text;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub async fn translate_text(
    user_data_path: &Path,
    workspace_root: &Path,
    resource_dir: &Path,
    payload: Value,
) -> Result<Value, ManagerError> {
    let source_text = payload
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    if source_text.is_empty() {
        return Err(ManagerError::System("没有可翻译的文本".to_string()));
    }

    let resource_script_path = resource_dir.join("node/translation-service.mjs");
    let script_path = if resource_script_path.exists() {
        resource_script_path
    } else {
        workspace_root.join("src-tauri/node/translation-service.mjs")
    };
    let node_command = if cfg!(windows) { "node.exe" } else { "node" };
    let mut command = Command::new(node_command);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .arg(&script_path)
        .arg(serde_json::to_string(&json!({
          "text": source_text,
          "userDataPath": path_text(user_data_path)
        }))?)
        .current_dir(workspace_root)
        .output()
        .await?;

    if !output.status.success() {
        let error_message = String::from_utf8_lossy(&output.stderr).trim().to_string();

        return Err(ManagerError::System(error_message));
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

pub fn workspace_root_from_current_dir() -> Result<PathBuf, ManagerError> {
    std::env::current_dir().map_err(ManagerError::Io)
}

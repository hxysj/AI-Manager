use super::redact_config;
use crate::core::{error::ManagerError, state::AppState};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

fn catalog() -> Vec<Value> {
    serde_json::from_str(include_str!("../../../../node/assistant-tools/app-api.json"))
        .expect("助手内部接口目录必须是合法 JSON")
}

fn lookup(channel: &str, payload: &Value) -> Result<(), ManagerError> {
    if !payload.is_object() || !catalog().iter().any(|item| item["channel"] == channel) {
        return Err(ManagerError::System("接口或参数无效，请先查询内部接口目录".into()));
    }
    Ok(())
}

// 复用页面使用的业务接口及其校验，不调用窗口、导航和控件操作。
pub(super) async fn execute(app: &AppHandle, args: &Value, name: &str) -> Result<Value, ManagerError> {
    if name == "read_app_api" {
        let module = args["module"].as_str().unwrap_or("");
        let items = catalog();
        if module.is_empty() {
            let modules = items.iter().filter_map(|item| item["module"].as_str())
                .collect::<std::collections::BTreeSet<_>>();
            return Ok(json!({"modules": modules}));
        }
        let items = items.into_iter().filter(|item| item["module"] == module).collect::<Vec<_>>();
        if items.is_empty() {
            return Err(ManagerError::System("未知接口模块，请先读取模块目录".into()));
        }
        return Ok(json!({"module": module, "operations": items}));
    }
    let channel = args["channel"].as_str().unwrap_or("");
    let payload = args.get("payload").cloned().unwrap_or(json!({}));
    lookup(channel, &payload)?;
    let state = app.state::<AppState>();
    let result = Box::pin(state.dispatch(app.clone(), channel, Some(payload))).await?;
    Ok(redact_config(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_api_catalog_excludes_window_navigation_and_secret_access() {
        let items = catalog();
        let channels = items.iter().map(|item| item["channel"].as_str().unwrap()).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(channels.len(), items.len());
        for item in items {
            assert!(lookup(item["channel"].as_str().unwrap(), &item["payload"]).is_ok());
        }
        for channel in ["app:open-tool-window", "quick-switch:show-main", "provider:key-value", "service-provider:file-backup", "settings:save", "assistant:send"] {
            assert!(lookup(channel, &json!({})).is_err());
        }
        assert!(lookup("tools:list-ports", &json!([])).is_err());
    }
}

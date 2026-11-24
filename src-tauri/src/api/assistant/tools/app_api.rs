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

    #[test]
    fn calendar_catalog_exposes_existing_rule_storage_and_date_parity() {
        let items=catalog();
        for channel in ["quick-notes:calendar-rule-list","quick-notes:calendar-rule-save","quick-notes:calendar-rule-batch","quick-notes:calendar-rule-delete","quick-notes:calendar-rule-history"] {
            assert!(items.iter().any(|item|item["module"]=="calendar" && item["channel"]==channel));
            assert!(lookup(channel,&json!({})).is_ok());
        }
        let example=items.iter().find(|item|item["channel"]=="quick-notes:calendar-rule-save").unwrap();
        let mut c=rusqlite::Connection::open_in_memory().unwrap();
        crate::core::quick_notes_calendar_store::initialize(&mut c).unwrap();
        let rule=crate::core::quick_notes_calendar_store::save_in(&c,&example["payload"],"ai").unwrap();
        assert_eq!(rule["kind"],"work");
        assert_eq!(rule["weekdays"],json!([5]));
        assert_eq!(rule["dateParity"],"even");
        let batch = items.iter().find(|item| item["channel"] == "quick-notes:calendar-rule-batch").unwrap();
        let saved = crate::core::quick_notes_calendar_store::batch_in(&c, &batch["payload"]["operations"], "ai").unwrap();
        assert_eq!(saved[0]["kind"], "leave");
        assert_eq!(saved[0]["startTime"], "14:00");
        assert_eq!(saved[0]["endDate"], "2026-11-27");
    }
}

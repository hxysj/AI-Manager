use super::{config, redact_config, AssistantService};
use crate::core::{error::ManagerError, state::AppState};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

fn catalog() -> Vec<Value> {
    serde_json::from_str(include_str!("../../../../node/assistant-tools/app-api.json"))
        .expect("助手内部接口目录必须是合法 JSON")
}

pub(super) fn modules() -> Value {
    serde_json::from_str(include_str!("../../../../node/assistant-tools/modules.json"))
        .expect("助手模块目录必须是合法 JSON")
}

fn action(item: &Value) -> &str {
    item["action"].as_str().unwrap_or_else(|| {
        item["channel"].as_str().unwrap().split_once(':').unwrap().1
    })
}

fn lookup<'a>(items: &'a [Value], modules: &Value, name: &str, args: &Value, disabled: &Value) -> Result<&'a Value, ManagerError> {
    items.iter().find(|item| {
        let tool = &modules[item["module"].as_str().unwrap()]["id"];
        let matches_tool = if name == "call_app_api" {
            item["channel"] == args["channel"]
        } else {
            tool == name && args["action"] == action(item)
        };
        matches_tool && !item["channel"].as_str().unwrap().starts_with("local:") &&
            !disabled.as_array().is_some_and(|ids| ids.iter().any(|id| id == tool || id == "call_app_api"))
    }).ok_or_else(|| ManagerError::System("接口未注册或已被停用".into()))
}

// 复用页面使用的业务接口及其校验，不调用窗口、导航和控件操作。
pub(super) async fn execute(service: &AssistantService, app: &AppHandle, args: &Value, name: &str, disabled: &Value) -> Result<Value, ManagerError> {
    let items = catalog();
    let modules = modules();
    if name == "read_app_api" {
        let module = args["module"].as_str().unwrap_or("");
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
    let item = lookup(&items, &modules, name, args, disabled)?;
    let channel = item["channel"].as_str().unwrap();
    let mut payload = args.get("payload").cloned().unwrap_or(json!({}));
    if !payload.is_object() {
        return Err(ManagerError::System("业务参数必须为对象".into()));
    }
    if let Some(module) = channel.strip_prefix("view:") {
        return config::execute(service, app, &json!({"module":module})).await;
    }
    if matches!(channel, "quick-notes:summary" | "quick-notes:capture" | "quick-notes:classify") {
        // 后台任务沿用便签的模型配置与来源，整理操作返回预览。
        payload.as_object_mut().unwrap().retain(|key, _| matches!(key.as_str(),
            "ids" | "text" | "noteId" | "requestId" | "idempotencyKey"));
        if channel != "quick-notes:summary" {
            payload["preview"] = json!(true);
        }
    }
    if channel == "git-tool:operation" {
        let allowed = matches!(payload["action"].as_str(), Some(
            "stage" | "unstage" | "commit" | "create-branch" | "switch-branch" |
            "stash-save" | "stash-apply" | "stash-pop" | "fetch" | "pull" | "push"
        ));
        if !allowed || !["expectedHead", "expectedBranch", "expectedWorktreePath"].iter()
            .all(|field| payload[*field].is_string()) {
            return Err(ManagerError::System("请先查询工作树状态，使用支持的 Git 操作及完整状态参数".into()));
        }
    }
    if channel == "lan-share:send-message" {
        // 文本发送与本地文件选择分开，防止路径参数被误作待发送附件。
        payload.as_object_mut().unwrap().retain(|key, _| matches!(key.as_str(),
            "deviceId" | "sessionId" | "content" | "messageId"));
        if payload["sessionId"].as_str().is_none_or(|id| id.is_empty()) {
            return Err(ManagerError::System("请先确定接收消息的会话".into()));
        }
    }
    if matches!(channel, "data:cloud-inspect" | "data:cloud-push" | "data:cloud-preview") {
        // 云备份只使用已配置目标，不接受模型生成的地址和认证信息。
        let snapshot = app.state::<AppState>().state_snapshot().await;
        let field = if payload["provider"] == "koofr" { "koofrSync" } else { "cloudSync" };
        payload = snapshot["appSettings"][field].clone();
    }
    if channel == "data:cloud-pull" {
        // 恢复使用预览时绑定的连接，不从对话重新填写连接配置。
        payload.as_object_mut().unwrap().remove("cloudSync");
    }
    let state = app.state::<AppState>();
    let mut result = match channel.strip_prefix("screenshot:") {
        Some(action) => crate::api::screenshot::dispatch(app.clone(), action.into(), payload).await?,
        None => Box::pin(state.dispatch(app.clone(), channel, Some(payload))).await?
    };
    if channel == "screenshot:list" {
        if let Some(entries) = result["entries"].as_array_mut() {
            for entry in entries.iter_mut().filter_map(Value::as_object_mut) {
                entry.remove("thumbnail");
            }
        }
    }
    if channel.starts_with("lan-share:") {
        if let Some(data) = result["data"].as_object_mut() {
            if let Some(service) = data.get_mut("service").and_then(Value::as_object_mut) {
                service.remove("qrSvg");
            }
            data.remove("qrSvg");
        }
    }
    if channel.starts_with("gateway:") {
        if let Some(items) = result.as_object_mut().and_then(|object| object.remove("tokens")) {
            // 保留可管理的调用方标识，明文令牌与密文仍由脱敏层过滤。
            result["callers"] = items.as_array().into_iter().flatten().map(|item| json!({
                "id": item["id"], "name": item["name"], "description": item["description"],
                "enabled": item["enabled"], "expiresAt": item["expiresAt"],
                "requestCount": item["requestCount"]
            })).collect::<Vec<_>>().into();
        }
        if let Some(created) = result.as_object_mut().and_then(|object| object.remove("createdToken")) {
            result["createdCaller"] = json!({"id":created["id"],"name":created["name"]});
        }
    }
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
        for item in &items {
            let module = &modules()[item["module"].as_str().unwrap()];
            if !item["channel"].as_str().unwrap().starts_with("local:") {
                assert!(lookup(&items, &modules(), module["id"].as_str().unwrap(), &json!({"action":action(item)}), &json!([])).is_ok());
                assert!(lookup(&items, &modules(), "call_app_api", &json!({"channel":item["channel"]}), &json!([module["id"]])).is_err());
            }
        }
        for channel in ["app:open-tool-window", "quick-switch:show-main", "provider:key-value", "service-provider:file-backup", "settings:save", "assistant:send"] {
            assert!(lookup(&items, &modules(), "call_app_api", &json!({"channel":channel}), &json!([])).is_err());
        }
        assert!(lookup(&items, &modules(), "app_ports", &json!({"action":"calendar-rule-delete"}), &json!([])).is_err());
        assert!(lookup(&items, &modules(), "app_ports", &json!({"action":"list-ports"}), &json!(["call_app_api"])).is_err());
    }

    #[test]
    fn calendar_catalog_exposes_existing_rule_storage_and_date_parity() {
        let items=catalog();
        for channel in ["quick-notes:calendar-rule-list","quick-notes:calendar-rule-save","quick-notes:calendar-rule-batch","quick-notes:calendar-rule-delete","quick-notes:calendar-rule-history"] {
            assert!(items.iter().any(|item|item["module"]=="calendar" && item["channel"]==channel));
            assert!(lookup(&items, &modules(), "call_app_api", &json!({"channel":channel}), &json!([])).is_ok());
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

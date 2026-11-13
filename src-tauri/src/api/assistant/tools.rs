use super::{AssistantService, EditorRequest};
use crate::core::{error::ManagerError, state::AppState};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

// 显式擦除 Future 类型，工具回调可复用应用 dispatch 而不形成异步类型递归。
pub(super) fn execute<'a>(
    service: &'a AssistantService,
    app: &'a AppHandle,
    request: &'a Value,
) -> futures_util::future::BoxFuture<'a, Result<Value, ManagerError>> {
    Box::pin(async move {
    let active = service.active.lock().await;
    if !active.as_ref().is_some_and(|(id, token)| {
        request["runId"] == *id && !token.is_cancelled()
    }) {
        return Err(ManagerError::System("任务已停止，未执行此操作".into()));
    }
    let cancellation = active.as_ref().unwrap().1.clone();
    drop(active);
    let state = app.state::<AppState>();
    let args = &request["args"];
    match request["name"].as_str().unwrap_or("") {
        "read_app_config" => {
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
        "update_app_settings" => {
            validate_settings_patch(args)?;
            let result = Box::pin(state.dispatch(
                app.clone(), "settings:save", Some(args.clone()),
            )).await?;
            Ok(redact_config(result))
        }
        "open_app_window" => {
            let tool_id = args["toolId"].as_str().unwrap_or("");
            if tool_id == "assistant" {
                return Box::pin(state.dispatch(app.clone(), "tool-window:open",
                    Some(json!({"toolId": "assistant"})))).await;
            }
            if tool_id != "main" && ![
                "json-agent", "image-workbench", "string-diff", "image-link-extractor",
                "coordinate-overlay", "port-monitor", "network-quality",
                "multi-protocol-gateway", "git", "lan-share", "codex-pets",
            ].contains(&tool_id) {
                return Err(ManagerError::System("未知应用窗口".into()));
            }
            if tool_id != "main" && app.get_webview_window(&format!("tool-window-{tool_id}")).is_some() {
                return crate::api::app::open_tool_window(app, tool_id);
            }
            Box::pin(state.dispatch(app.clone(), "quick-switch:show-main",
                Some(if tool_id == "main" { json!({}) } else { json!({"toolId": tool_id}) }))).await
        }
        "set_pet_action" => {
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
        "read_json_editor" | "write_json_editor" => {
            if request["name"] == "write_json_editor" {
                let result = args["result"].as_str().ok_or_else(|| ManagerError::System("缺少 JSON 结果".into()))?;
                if result.chars().count() > 60_000 || args["source"].as_str().is_some_and(|text| text.chars().count() > 60_000) {
                    return Err(ManagerError::System("编辑内容最多 60000 个字符".into()));
                }
                serde_json::from_str::<Value>(result)
                    .map_err(|error| ManagerError::System(format!("JSON 校验失败：{error}")))?;
            }
            let id = uuid::Uuid::new_v4().to_string();
            let payload = json!({
                "id": id, "operation": if request["name"] == "write_json_editor" { "write" } else { "read" },
                "source": args["source"], "result": args["result"], "runId": request["runId"],
            });
            let (reply, received) = oneshot::channel();
            service.editors.lock().await.insert(id.clone(), EditorRequest { payload: payload.clone(), reply });
            let result = async {
                if app.get_webview_window("tool-window-json-agent").is_some() {
                    crate::api::app::open_tool_window(app, "json-agent")?;
                } else {
                    Box::pin(state.dispatch(app.clone(), "quick-switch:show-main",
                        Some(json!({"toolId": "json-agent"})))).await?;
                }
                app.emit("assistant:editor-request", payload)
                    .map_err(|error| ManagerError::System(error.to_string()))?;
                // 窗口启动和编辑回执有界等待，未收到回执不能向模型宣称已写入。
                match futures_util::future::select(
                    Box::pin(cancellation.cancelled()),
                    Box::pin(tokio::time::timeout(std::time::Duration::from_secs(15), received)),
                ).await {
                    futures_util::future::Either::Left(_) => Err(ManagerError::System("任务已停止".into())),
                    futures_util::future::Either::Right((result, _)) =>
                        result.map_err(|_| ManagerError::System("JSON 工具未就绪，请打开工具后重试".into()))?
                            .map_err(|_| ManagerError::System("JSON 编辑请求已关闭".into()))?
                            .map_err(ManagerError::System),
                }
            }.await;
            service.editors.lock().await.remove(&id);
            result
        }
        _ => Err(ManagerError::System("未注册的助手工具".into())),
    }
    })
}

// 配置从当前应用状态读取；凭据、环境变量和自定义请求头不进入模型上下文。
pub(super) fn redact_config(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(object.into_iter().map(|(key, value)| {
            let name = key.to_ascii_lowercase().replace(['_', '-'], "");
            let sensitive = ["password", "secret", "token", "apikey", "authorization", "cookie"]
                .iter().any(|part| name.contains(part))
                || matches!(name.as_str(), "headers" | "env" | "auth" | "credentials");
            (key, if sensitive && !value.is_boolean() {
                json!(if value.is_null() || value.as_str() == Some("") { "" } else { "[已隐藏]" })
            } else {
                redact_config(value)
            })
        }).collect()),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_config).collect()),
        Value::String(text) => {
            if let Ok(mut url) = url::Url::parse(&text) {
                if matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") {
                    let _ = url.set_username("");
                    let _ = url.set_password(None);
                    // 代理或服务地址的 query 也可能携带无标准命名的凭据。
                    if url.query().is_some() {
                        url.set_query(Some("redacted"));
                    }
                    return json!(url.to_string());
                }
            }
            json!(text)
        }
        value => value,
    }
}

fn validate_settings_patch(patch: &Value) -> Result<(), ManagerError> {
    let object = patch.as_object().filter(|object| !object.is_empty())
        .ok_or_else(|| ManagerError::System("设置修改必须是非空对象".into()))?;
    for (section, value) in object {
        let fields = value.as_object().ok_or_else(|| ManagerError::System("设置分组必须是对象".into()))?;
        if !["system", "desktop", "agents"].contains(&section.as_str()) {
            return Err(ManagerError::System(format!("不允许的设置分组：{section}")));
        }
        for (key, value) in fields {
            let valid = match (section.as_str(), key.as_str()) {
                ("system", "logEnabled" | "autoLaunchEnabled" | "quickSwitchVisible") => value.is_boolean(),
                ("system", "closeAction") => matches!(value.as_str(), Some("ask" | "minimize" | "quit")),
                ("system", "quickSwitchTools") => value.as_object().is_some_and(|items|
                    items.iter().all(|(name, value)| name.len() <= 64 && value.is_boolean())),
                ("desktop", "enabled") => value.is_boolean(),
                ("desktop", "petId") => value == "monkey_animations",
                ("agents", "translation") => {
                    let Some(translation) = value.as_object() else {
                        return Err(ManagerError::System("翻译配置必须是对象".into()));
                    };
                    translation.iter().all(|(key, value)| match key.as_str() {
                        "enabled" => value.is_boolean(),
                        "targetId" | "model" | "sourceLanguage" | "targetLanguage" | "shortcut" | "clipboardShortcut" =>
                            value.as_str().is_some_and(|text| text.chars().count() <= 200),
                        _ => false,
                    })
                }
                _ => false,
            };
            if !valid {
                return Err(ManagerError::System(format!("不允许的设置字段或类型：{section}.{key}")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_projection_hides_nested_credentials_and_preserves_flags() {
        let result = redact_config(json!({
            "cloudSync": {"password": "private", "webdavUrl": "https://user:private@example.com?custom=private"},
            "providers": [{"apiKeys": [{"value": "private"}], "headers": {"x-key": "private"},
                "hasApiKey": true, "runtimeConfig": {"env": {"ANY_NAME": "private"}, "mainModel": "model"}}]
        }));
        assert!(!result.to_string().contains("private"));
        assert_eq!(result["providers"][0]["hasApiKey"], true);
        assert_eq!(result["providers"][0]["runtimeConfig"]["mainModel"], "model");
    }

    #[test]
    fn settings_tool_rejects_unknown_fields_and_wrong_types() {
        assert!(validate_settings_patch(&json!({"system": {"quickSwitchVisible": true}})).is_ok());
        for patch in [
            json!({"dataPath": "elsewhere"}), json!({"cloudSync": {"password": "x"}}),
            json!({"system": {"autoLaunchEnabled": "true"}}), json!({"agents": {"assistant": {}}}),
            json!({"agents": {"translation": {"unknown": true}}}),
        ] {
            assert!(validate_settings_patch(&patch).is_err());
        }
    }
}

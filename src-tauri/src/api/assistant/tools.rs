use super::AssistantService;
use crate::core::{error::ManagerError, state::AppState};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};


mod config;
mod settings;
mod windows;
mod pet;
mod json_editor;
mod skills;

pub(super) fn catalog() -> Value {
    serde_json::from_str(include_str!("../../../node/assistant-tools/catalog.json")).expect("助手工具目录必须是合法 JSON")
}

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
    let snapshot = state.state_snapshot().await;
    if snapshot["appSettings"]["agents"]["assistant"]["disabledTools"].as_array().is_some_and(|items| items.iter().any(|id| id == &request["name"])) {
        return Err(ManagerError::System("此助手工具已被停用".into()));
    }
    match request["name"].as_str().unwrap_or("") {
        "read_app_config" => config::execute(service, app, args).await,
        "update_app_settings" => settings::execute(app, args).await,
        "open_app_window" => windows::execute(app, args, request["surface"].as_str()).await,
        "set_pet_action" => pet::execute(service, app, args).await,
        "read_json_editor" | "write_json_editor" => json_editor::execute(service, app, args, cancellation, request).await,
        "read_software_skills" | "read_assistant_skills" => skills::execute(app, args, request["name"].as_str().unwrap_or("")).await,
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

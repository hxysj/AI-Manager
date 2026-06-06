use crate::api::{codex_account, rules, runtime_provider, skills, usage};
use crate::core::error::ManagerError;
use crate::core::paths::{public_paths, AppPaths};
use crate::core::settings::{
    bool_value, non_empty_string, number_value, resolve_portable_path, string_value, AppSettings,
};
use serde_json::{json, Value};
use std::path::Path;

pub fn create_initial_state(
    paths: &AppPaths,
    app_settings: &AppSettings,
) -> Result<Value, ManagerError> {
    let files = &paths.storage_files;
    let runtime_state = read_json_file(&files.runtime_provider_state, json!({}))?;
    let claude_proxy_config = normalize_proxy_config(
        read_json_file(&files.claude_proxy_config, json!({}))?,
        15722,
    );
    let codex_proxy_config =
        normalize_proxy_config(read_json_file(&files.codex_proxy_config, json!({}))?, 15721);
    let claude_proxy_logs = read_json_file(&files.claude_proxy_request_logs, json!([]))?;
    let codex_proxy_logs = read_json_file(&files.codex_proxy_request_logs, json!([]))?;
    let runtime_provider_state = runtime_state
        .get("runtimeProviderState")
        .cloned()
        .unwrap_or(runtime_state);

    let claude_proxy_state = build_proxy_state(
        "claude",
        claude_proxy_config,
        files.claude_proxy_live_backup.clone(),
        claude_proxy_logs,
    );
    let codex_proxy_state = build_proxy_state(
        "codex",
        codex_proxy_config,
        files.codex_proxy_live_backup.clone(),
        codex_proxy_logs,
    );
    let mut runtime_provider_state = runtime_provider_state;

    apply_proxy_runtime_state(&mut runtime_provider_state, "claude", &claude_proxy_state);
    apply_proxy_runtime_state(&mut runtime_provider_state, "codex", &codex_proxy_state);

    Ok(json!({
      "cliTargets": read_cli_targets(&files.cli_targets)?,
      "skills": read_json_file(&files.skills, json!([]))?,
      "skillRepositories": skills::load_repositories(paths)?,
      "repos": read_json_file(&files.repos, json!([]))?,
      "sessions": read_json_file(&files.sessions, json!([]))?,
      "usage": usage::build_state(paths)?,
      "codexAccounts": codex_account::read_public_accounts(paths)?,
      "codexLoginState": null,
      "providers": runtime_provider::read_public_providers(paths)?,
      "rules": rules::build_state(paths)?,
      "runtimeConfigSchemas": runtime_provider::runtime_config_schemas(),
      "runtimeModels": read_json_file(&files.runtime_models, json!([]))?,
      "runtimeProfiles": runtime_provider::read_public_profiles(paths)?,
      "runtimeProviderState": runtime_provider_state,
      "claudeProxyState": claude_proxy_state,
      "codexProxyState": codex_proxy_state,
      "diagnostics": [],
      "paths": public_paths(paths),
      "appSettings": app_settings,
      "refreshedAt": now_millis()
    }))
}

fn build_proxy_state(cli: &str, config: Value, live_backup_path: String, logs: Value) -> Value {
    let local_base_url = if cli == "claude" {
        build_anthropic_local_base_url(&config)
    } else {
        build_local_base_url(&config)
    };

    let mut state = config;

    state["localBaseUrl"] = json!(local_base_url);
    state["hasLiveBackup"] = json!(read_json_file(live_backup_path, Value::Null)
        .map(|value| !value.is_null())
        .unwrap_or(false));
    state["logs"] = logs;
    state
}

fn apply_proxy_runtime_state(runtime_provider_state: &mut Value, cli: &str, proxy_state: &Value) {
    if proxy_state.get("enabled").and_then(Value::as_bool) != Some(true) {
        return;
    }

    let mut item = runtime_provider_state
        .get(cli)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    item.insert(
        "activeProviderId".to_string(),
        proxy_state
            .get("activeProviderId")
            .cloned()
            .unwrap_or_else(|| json!("")),
    );
    item.insert("status".to_string(), json!("PROXY_MANAGED"));

    if let Some(map) = runtime_provider_state.as_object_mut() {
        map.insert(cli.to_string(), Value::Object(item));
    }
}

fn normalize_proxy_config(input: Value, default_port: u64) -> Value {
    json!({
      "enabled": bool_value(input.get("enabled"), false),
      "host": non_empty_string(input.get("host"), "127.0.0.1"),
      "port": number_value(input.get("port"), default_port),
      "activeProviderId": string_value(input.get("activeProviderId")),
      "failoverProviderIds": input
        .get("failoverProviderIds")
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([])),
      "accountModel": string_value(input.get("accountModel")),
      "retryCount": number_value(input.get("retryCount"), 1),
      "streamTimeoutMs": number_value(input.get("streamTimeoutMs"), 120000),
      "requestTimeoutMs": number_value(input.get("requestTimeoutMs"), 120000),
      "updatedAt": number_value(input.get("updatedAt"), 0)
    })
}

fn build_local_base_url(config: &Value) -> String {
    format!(
        "http://{}:{}/v1",
        format_host_for_url(&normalize_host_for_client(&string_value(
            config.get("host")
        ))),
        number_value(config.get("port"), 15721)
    )
}

fn build_anthropic_local_base_url(config: &Value) -> String {
    format!(
        "http://{}:{}",
        format_host_for_url(&normalize_host_for_client(&string_value(
            config.get("host")
        ))),
        number_value(config.get("port"), 15722)
    )
}

fn normalize_host_for_client(host: &str) -> String {
    match host {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "::1".to_string(),
        _ => host.to_string(),
    }
}

fn format_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{}]", host)
    } else {
        host.to_string()
    }
}

fn read_json_file(path: impl AsRef<Path>, fallback: Value) -> Result<Value, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(fallback),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn read_cli_targets(path: &str) -> Result<Value, ManagerError> {
    let targets = read_json_file(path, json!([]))?
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(resolve_cli_target_paths)
        .collect::<Vec<_>>();

    Ok(json!(targets))
}

fn resolve_cli_target_paths(mut target: Value) -> Value {
    for key in ["configPath", "skillsPath", "sessionsPath"] {
        let value = string_value(target.get(key));

        if !value.is_empty() {
            target[key] = json!(resolve_portable_path(&value));
        }
    }

    if let Some(items) = target.get("sessionPaths").and_then(Value::as_array) {
        target["sessionPaths"] = json!(items
            .iter()
            .map(|item| resolve_portable_path(&string_value(Some(item))))
            .collect::<Vec<_>>());
    }

    target
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

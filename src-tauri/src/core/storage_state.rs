use crate::api::{codex_account, rules, runtime_provider, skills, usage};
use crate::core::error::ManagerError;
use crate::core::paths::{path_text, public_paths, AppPaths};
use crate::core::provider_store;
use crate::core::settings::{
    bool_value, non_empty_string, number_value, resolve_portable_path, serialize_portable_path,
    string_value, AppSettings,
};
use crate::core::skill_store;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn create_initial_state(
    paths: &AppPaths,
    app_settings: &AppSettings,
) -> Result<Value, ManagerError> {
    let files = &paths.storage_files;
    let cli_targets = read_cli_targets(paths, app_settings)?;
    let runtime_state = Value::Object(provider_store::read_runtime_state(paths)?);
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
      "cliTargets": cli_targets.clone(),
      "skills": skill_store::read_skills(paths)?,
      "skillGroups": skills::load_skill_groups(paths)?,
      "skillRepositories": skills::load_repositories(paths)?,
      "repos": read_json_file(&files.repos, json!([]))?,
      "sessions": read_json_file(&files.sessions, json!([]))?,
      "usage": usage::build_state(paths)?,
      "codexAccounts": codex_account::read_public_accounts(paths)?,
      "codexLoginState": null,
      "providers": runtime_provider::read_public_providers(paths)?,
      "rules": rules::build_state(paths, &cli_targets)?,
      "runtimeConfigSchemas": runtime_provider::runtime_config_schemas(),
      "runtimeModels": provider_store::read_models(paths)?,
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

fn read_cli_targets(paths: &AppPaths, app_settings: &AppSettings) -> Result<Value, ManagerError> {
    let stored_targets = read_json_file(&paths.storage_files.cli_targets, json!([]))?
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(resolve_cli_target_paths)
        .collect::<Vec<_>>();
    let mut targets = Vec::new();

    for (id, name, icon, command_name) in [
        ("claude", "Claude", "claude.svg", "claude"),
        ("codex", "Codex", "codex.svg", "codex"),
    ] {
        let mut target = stored_targets
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let config_path = string_value(app_settings.cli_config_paths.get(id));
        let config_dir = PathBuf::from(&config_path);
        let executable_path = detect_executable_path(command_name);
        let version = executable_path
            .as_deref()
            .and_then(detect_cli_version)
            .unwrap_or_default();
        let skills_path = path_text(config_dir.join("skills"));
        let sessions_path = if id == "claude" {
            path_text(config_dir.join("projects"))
        } else {
            path_text(config_dir.join("sessions"))
        };
        let session_paths = if id == "claude" {
            json!([
                sessions_path.clone(),
                path_text(config_dir.join("sessions"))
            ])
        } else {
            json!([sessions_path.clone()])
        };

        target["id"] = json!(id);
        target["type"] = json!(id);
        target["name"] = json!(name);
        target["icon"] = json!(icon);
        target["configPath"] = json!(config_path.clone());
        target["skillsPath"] = json!(skills_path);
        target["sessionsPath"] = json!(sessions_path);
        target["sessionPaths"] = session_paths;
        target["executablePath"] = json!(executable_path.unwrap_or_default());
        target["version"] = json!(version);
        target["installed"] = json!(
            Path::new(&config_path).exists()
                || !string_value(target.get("executablePath")).is_empty()
        );
        targets.push(target);
    }

    for target in stored_targets {
        let id = string_value(target.get("id"));

        if id != "claude" && id != "codex" {
            targets.push(target);
        }
    }

    persist_cli_targets(&paths.storage_files.cli_targets, &targets)?;

    Ok(json!(targets))
}

fn persist_cli_targets(path: &str, targets: &[Value]) -> Result<(), ManagerError> {
    let payload = targets
        .iter()
        .cloned()
        .map(serialize_cli_target_paths)
        .collect::<Vec<_>>();

    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&json!(payload))?),
    )?;
    Ok(())
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

fn serialize_cli_target_paths(mut target: Value) -> Value {
    for key in ["configPath", "skillsPath", "sessionsPath"] {
        let value = string_value(target.get(key));

        if !value.is_empty() {
            target[key] = json!(serialize_portable_path(&value));
        }
    }

    if let Some(items) = target.get("sessionPaths").and_then(Value::as_array) {
        target["sessionPaths"] = json!(items
            .iter()
            .map(|item| serialize_portable_path(&string_value(Some(item))))
            .collect::<Vec<_>>());
    }

    target
}

fn detect_executable_path(command_name: &str) -> Option<String> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("where.exe");
        command.arg(command_name);
        command
    } else {
        let mut command = Command::new("which");
        command.arg(command_name);
        command
    };
    let output = command_output(&mut command).ok()?;

    if !output.status.success() {
        return None;
    }

    let paths = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if cfg!(windows) {
        for extension in [".cmd", ".exe", ".bat"] {
            if let Some(path) = paths
                .iter()
                .find(|item| item.to_lowercase().ends_with(extension))
            {
                return Some(path.clone());
            }
        }
    }

    paths.first().cloned()
}

fn detect_cli_version(executable_path: &str) -> Option<String> {
    let mut command = Command::new(executable_path);
    command.arg("--version");
    let output = command_output(&mut command).ok()?;

    if !output.status.success() {
        return None;
    }

    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    text.lines()
        .map(str::trim)
        .find(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn command_output(command: &mut Command) -> std::io::Result<Output> {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    command.output()
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

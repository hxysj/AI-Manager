use crate::api::proxy;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use crate::core::settings::{number_value, resolve_portable_path};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::process::Command;

static PROVIDER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub async fn save_provider(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let mut providers = read_array(&paths.storage_files.providers)?;
    let mut models = read_array(&paths.storage_files.runtime_models)?;
    let mut profiles = read_array(&paths.storage_files.runtime_profiles)?;
    let mut keys = read_object(&paths.storage_files.runtime_provider_keys)?;
    let previous = providers
        .iter()
        .find(|item| item.get("id") == payload.get("id"))
        .cloned();

    if previous
        .as_ref()
        .and_then(|item| item.get("enabled"))
        .and_then(Value::as_bool)
        == Some(false)
        && payload.get("enabled").and_then(Value::as_bool) != Some(true)
    {
        return Err(ManagerError::System(
            "Provider 已禁用，不能编辑".to_string(),
        ));
    }

    let provider = normalize_provider(&payload, previous.as_ref())?;
    let provider_id = string_value(provider.get("id"));

    if previous.is_some() {
        providers = providers
            .into_iter()
            .map(|item| {
                if item.get("id").and_then(Value::as_str) == Some(provider_id.as_str()) {
                    provider.clone()
                } else {
                    item
                }
            })
            .collect();
    } else {
        providers.push(provider.clone());
        models.extend(
            default_models(&string_value(provider.get("type")))
                .into_iter()
                .map(|model| {
                    let model_id = format!("{}:{}", provider_id, model);

                    normalize_model(
                        &json!({
                          "id": model_id,
                          "providerId": provider_id,
                          "name": model
                        }),
                        None,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }

    if provider.get("enabled").and_then(Value::as_bool) == Some(false) {
        profiles.retain(|item| {
            item.get("providerId").and_then(Value::as_str) != Some(provider_id.as_str())
        });
    }

    if payload.get("apiKey").is_some() {
        set_provider_key(&mut keys, &provider_id, string_value(payload.get("apiKey")))?;
    }

    let model_name = string_value(payload.get("model"));

    if !model_name.is_empty() {
        let model_id = format!("{}:{}", provider_id, model_name);
        let model = normalize_model(
            &json!({
              "id": model_id,
              "providerId": provider_id,
              "name": model_name
            }),
            models
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(model_id.as_str())),
        )?;

        if models.iter().any(|item| item.get("id") == model.get("id")) {
            models = models
                .into_iter()
                .map(|item| {
                    if item.get("id") == model.get("id") {
                        model.clone()
                    } else {
                        item
                    }
                })
                .collect();
        } else {
            models.push(model);
        }
    }

    write_json(&paths.storage_files.providers, &json!(providers)).await?;
    write_json(&paths.storage_files.runtime_models, &json!(models)).await?;
    write_json(&paths.storage_files.runtime_profiles, &json!(profiles)).await?;
    write_json(
        &paths.storage_files.runtime_provider_keys,
        &Value::Object(keys),
    )
    .await
}

pub async fn delete_provider(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let provider_id = string_value(payload.get("providerId"));
    let mut providers = read_array(&paths.storage_files.providers)?;
    let mut keys = read_object(&paths.storage_files.runtime_provider_keys)?;
    let provider = providers
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(provider_id.as_str()))
        .cloned();

    if provider
        .as_ref()
        .and_then(|item| item.get("enabled"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        return Err(ManagerError::System(
            "Provider 已禁用，不能删除".to_string(),
        ));
    }

    providers.retain(|item| item.get("id").and_then(Value::as_str) != Some(provider_id.as_str()));
    let mut models = read_array(&paths.storage_files.runtime_models)?;
    let mut profiles = read_array(&paths.storage_files.runtime_profiles)?;

    models.retain(|item| {
        item.get("providerId").and_then(Value::as_str) != Some(provider_id.as_str())
    });
    profiles.retain(|item| {
        item.get("providerId").and_then(Value::as_str) != Some(provider_id.as_str())
    });
    keys.remove(&provider_id);

    write_json(&paths.storage_files.providers, &json!(providers)).await?;
    write_json(&paths.storage_files.runtime_models, &json!(models)).await?;
    write_json(&paths.storage_files.runtime_profiles, &json!(profiles)).await?;
    write_json(
        &paths.storage_files.runtime_provider_keys,
        &Value::Object(keys),
    )
    .await
}

pub async fn save_runtime_model(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let providers = read_array(&paths.storage_files.providers)?;
    let mut models = read_array(&paths.storage_files.runtime_models)?;
    let previous = models
        .iter()
        .find(|item| item.get("id") == payload.get("id"))
        .cloned();
    let model = normalize_model(&payload, previous.as_ref())?;
    let provider_id = string_value(model.get("providerId"));

    if !providers
        .iter()
        .any(|item| item.get("id").and_then(Value::as_str) == Some(provider_id.as_str()))
    {
        return Err(ManagerError::System(
            "模型关联的 Provider 不存在".to_string(),
        ));
    }

    if previous.is_some() {
        models = models
            .into_iter()
            .map(|item| {
                if item.get("id") == model.get("id") {
                    model.clone()
                } else {
                    item
                }
            })
            .collect();
    } else {
        models.push(model);
    }

    write_json(&paths.storage_files.runtime_models, &json!(models)).await
}

pub async fn switch_runtime(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let cli = string_value(payload.get("cli"));

    ensure_proxy_disabled(paths, &cli)?;

    let providers = read_array(&paths.storage_files.providers)?;
    let mut profiles = read_array(&paths.storage_files.runtime_profiles)?;
    let provider_id = string_value(payload.get("providerId"));
    let provider = providers
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(provider_id.as_str()))
        .cloned()
        .ok_or_else(|| ManagerError::System("Provider 不存在".to_string()))?;

    if provider.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Err(ManagerError::System(
            "Provider 已禁用，不能启用".to_string(),
        ));
    }

    if provider.get("cli").and_then(Value::as_str) != Some(cli.as_str()) {
        return Err(ManagerError::System(
            "Runtime Profile 不能使用其他 CLI 的 Provider".to_string(),
        ));
    }

    let previous = profiles
        .iter()
        .find(|item| item.get("cli").and_then(Value::as_str) == Some(cli.as_str()))
        .cloned();
    let profile = normalize_profile(&payload, previous.as_ref())?;

    if previous.is_some() {
        profiles = profiles
            .into_iter()
            .map(|item| {
                if item.get("cli").and_then(Value::as_str) == Some(cli.as_str()) {
                    profile.clone()
                } else {
                    item
                }
            })
            .collect();
    } else {
        profiles.push(profile);
    }

    write_json(&paths.storage_files.runtime_profiles, &json!(profiles)).await?;
    write_cli_config(paths, &cli, find_cli_target(cli_targets, &cli)?).await?;

    if cli == "codex" {
        write_json(&paths.storage_files.codex_active_account_id, &json!("")).await?;
    }

    refresh_drift(paths, cli_targets).await
}

pub async fn clear_runtime(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let cli = string_value(payload.get("cli").or(Some(&payload)));

    ensure_proxy_disabled(paths, &cli)?;

    let mut profiles = read_array(&paths.storage_files.runtime_profiles)?;

    profiles.retain(|item| item.get("cli").and_then(Value::as_str) != Some(cli.as_str()));
    write_json(&paths.storage_files.runtime_profiles, &json!(profiles)).await?;
    refresh_drift(paths, cli_targets).await
}

pub async fn compare_runtime(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<Value, ManagerError> {
    let cli = string_value(payload.get("cli"));
    let cli_target = find_cli_target(cli_targets, &cli)?;
    let profile = find_runtime_profile(paths, &cli)?;
    let provider = find_provider(paths, &string_value(profile.get("providerId")))?;
    let config_path = string_value(cli_target.get("configPath"));

    if config_path.is_empty() {
        return Err(ManagerError::System("CLI 配置目录不存在".to_string()));
    }

    let manager_files = build_cli_config_files(paths, &cli, &provider, &profile)?;
    let runtime_files = read_runtime_config_files(&cli, &cli_target).await?;

    Ok(json!({
      "provider": provider,
      "profile": to_public_profile(paths, profile)?,
      "managerContent": combine_managed_config_contents(&cli, &manager_files)?,
      "runtimeContent": combine_managed_config_contents(&cli, &runtime_files)?,
      "runtimePath": format_runtime_path(&config_path, &manager_files)
    }))
}

pub async fn get_runtime_config(
    payload: Value,
    cli_targets: &Value,
) -> Result<Value, ManagerError> {
    let cli = string_value(payload.get("cli"));
    let cli_target = find_cli_target(cli_targets, &cli)?;
    let config_path = string_value(cli_target.get("configPath"));

    if config_path.is_empty() {
        return Err(ManagerError::System("CLI 配置目录不存在".to_string()));
    }

    let runtime_files = read_runtime_config_files(&cli, &cli_target).await?;

    Ok(json!({
      "runtimeContent": combine_config_contents(&runtime_files),
      "runtimePath": format_runtime_path(&config_path, &runtime_files)
    }))
}

pub async fn resolve_runtime_drift(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let cli = string_value(payload.get("cli"));
    let cli_target = find_cli_target(cli_targets, &cli)?;
    let source = string_value(payload.get("source"));

    if source == "runtime" {
        sync_runtime_config_to_manager(paths, &cli, cli_target).await?;
        refresh_drift(paths, cli_targets).await?;
        return Ok(());
    }

    if source != "manager" {
        return Err(ManagerError::System(
            "请选择 Runtime 配置同步方向".to_string(),
        ));
    }

    write_cli_config(paths, &cli, cli_target).await?;
    refresh_drift(paths, cli_targets).await
}

pub fn build_runtime_env(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let cli = string_value(payload.get("cli").or(Some(&payload)));
    let profile = find_runtime_profile(paths, &cli)?;
    let provider = find_provider(paths, &string_value(profile.get("providerId")))?;
    let provider_id = string_value(provider.get("id"));
    let keys = read_object(&paths.storage_files.runtime_provider_keys)?;
    let api_key = keys
        .get(&provider_id)
        .and_then(Value::as_str)
        .map(decrypt_provider_key)
        .transpose()?
        .unwrap_or_default();
    let base_url = first_string(profile.get("baseUrl"), provider.get("baseUrl"));
    let proxy = first_string(profile.get("proxy"), provider.get("proxy"));
    let mut env = profile
        .get("env")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if cli == "claude" {
        let auth_field = first_string(provider.get("authField"), None);
        let auth_field = if auth_field.is_empty() {
            "ANTHROPIC_AUTH_TOKEN".to_string()
        } else {
            auth_field
        };

        env.insert(auth_field, json!(api_key));
        env.insert("ANTHROPIC_MODEL".to_string(), profile["model"].clone());
        if !base_url.is_empty() {
            env.insert("ANTHROPIC_BASE_URL".to_string(), json!(base_url));
        }
    }

    if cli == "codex" || cli == "opencode" {
        env.insert("OPENAI_API_KEY".to_string(), json!(api_key));
        env.insert("OPENAI_MODEL".to_string(), profile["model"].clone());
        if !base_url.is_empty() {
            env.insert("OPENAI_BASE_URL".to_string(), json!(base_url));
        }
    }

    if cli == "gemini" {
        env.insert("GOOGLE_API_KEY".to_string(), json!(api_key));
        env.insert("GEMINI_MODEL".to_string(), profile["model"].clone());
    }

    if !proxy.is_empty() {
        env.insert("HTTP_PROXY".to_string(), json!(proxy));
        env.insert("HTTPS_PROXY".to_string(), json!(proxy));
    }

    Ok(Value::Object(env))
}

pub async fn launch_codex_provider_instance(
    paths: &AppPaths,
    cli_targets: &Value,
    proxy_server_registry: &proxy::ProxyServerRegistry,
    payload: Value,
) -> Result<Value, ManagerError> {
    let provider_id = string_value(payload.get("providerId"));
    let provider = read_array(&paths.storage_files.providers)?
        .into_iter()
        .find(|item| {
            item.get("id").and_then(Value::as_str) == Some(provider_id.as_str())
                && item.get("cli").and_then(Value::as_str) == Some("codex")
        })
        .ok_or_else(|| ManagerError::System("Codex Provider 不存在".to_string()))?;

    if provider.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Err(ManagerError::System("Codex Provider 已禁用".to_string()));
    }

    let api_key = get_provider_api_key(paths, &provider_id)?;

    if api_key.is_empty() {
        return Err(ManagerError::System(
            "当前 Codex Provider 缺少 API Key".to_string(),
        ));
    }

    if string_value(provider.get("baseUrl")).is_empty() {
        return Err(ManagerError::System(
            "当前 Codex Provider 缺少请求地址".to_string(),
        ));
    }

    let runtime_config = provider.get("runtimeConfig").cloned().unwrap_or_else(|| json!({}));
    let model = first_string(
        runtime_config.get("mainModel"),
        read_array(&paths.storage_files.runtime_models)?
            .iter()
            .find(|item| item.get("providerId").and_then(Value::as_str) == Some(provider_id.as_str()))
            .and_then(|item| item.get("name")),
    );

    if model.is_empty() {
        return Err(ManagerError::System(
            "当前 Codex Provider 缺少模型名称".to_string(),
        ));
    }

    let cli_target = find_cli_target(cli_targets, "codex")?;
    let executable_path = string_value(cli_target.get("executablePath"));

    if executable_path.is_empty() {
        return Err(ManagerError::System(
            "未检测到 Codex CLI 可执行文件".to_string(),
        ));
    }

    proxy::start_provider_instance_server(proxy_server_registry, paths, cli_targets).await?;

    let profile_dir = Path::new(&paths.workspace_root)
        .join("codex-instances")
        .join(format!(
            "{}-{}",
            slugify_name(&string_value(provider.get("name")))
                .if_empty_then(|| "provider".to_string()),
            slugify_name(&provider_id).if_empty_then(|| provider_id.clone())
        ));
    let token = proxy::create_provider_instance_token(&provider_id);
    let proxy_state = proxy::read_proxy_state(paths, "codex")?;
    let local_base_url = string_value(proxy_state.get("localBaseUrl"));
    let mut config_lines = vec![
        "model_provider = \"custom\"".to_string(),
        format!("model = {}", to_toml_string(model.clone())),
        format!(
            "model_reasoning_effort = {}",
            to_toml_string(first_string(
                runtime_config.get("modelReasoningEffort"),
                Some(&json!("low"))
            ))
        ),
        "disable_response_storage = true".to_string(),
    ];

    if runtime_config
        .get("serviceTierFast")
        .and_then(Value::as_bool)
        == Some(true)
    {
        config_lines.push("service_tier = \"fast\"".to_string());
    }

    if runtime_config
        .get("modelContextWindowEnabled")
        .and_then(Value::as_bool)
        == Some(true)
    {
        config_lines.push("model_context_window = 1000000".to_string());
        config_lines.push(format!(
            "model_auto_compact_token_limit = {}",
            number_value(runtime_config.get("modelAutoCompactTokenLimit"), 900000)
        ));
    }

    config_lines.extend([
        "".to_string(),
        "[model_providers]".to_string(),
        "[model_providers.custom]".to_string(),
        "name = \"custom\"".to_string(),
        "wire_api = \"responses\"".to_string(),
        "requires_openai_auth = true".to_string(),
        format!("base_url = {}", to_toml_string(local_base_url.clone())),
    ]);

    tokio::fs::create_dir_all(&profile_dir).await?;
    tokio::fs::write(
        profile_dir.join("auth.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({ "OPENAI_API_KEY": token }))?
        ),
    )
    .await?;
    tokio::fs::write(
        profile_dir.join("config.toml"),
        format!("{}\n", config_lines.join("\n")),
    )
    .await?;

    let sessions_path = profile_dir.join("sessions");
    tokio::fs::create_dir_all(&sessions_path).await?;

    let next_instance = json!({
      "id": provider["id"],
      "providerId": provider["id"],
      "providerName": provider["name"],
      "providerType": string_value(provider.get("type")),
      "profileDir": profile_dir.to_string_lossy().to_string(),
      "sessionsPath": sessions_path.to_string_lossy().to_string(),
      "updatedAt": now_millis()
    });
    let mut instances = read_array(&paths.storage_files.codex_provider_instances)?;

    instances.retain(|item| item.get("providerId").and_then(Value::as_str) != Some(provider_id.as_str()));
    instances.insert(0, next_instance);
    write_json(&paths.storage_files.codex_provider_instances, &json!(instances)).await?;

    let codex_executable_path = resolve_codex_executable_path(&executable_path).await;

    let mut version_command = Command::new(&codex_executable_path);

    #[cfg(windows)]
    version_command.creation_flags(CREATE_NO_WINDOW);

    match version_command
        .arg("--version")
        .kill_on_drop(true)
        .output()
        .await
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let message = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(ManagerError::System(format!(
                "全局 Codex 启动失败，请重新安装 npm 全局 Codex：{}",
                message
            )));
        }
        Err(error) => {
            return Err(ManagerError::System(format!(
                "全局 Codex 启动失败，请重新安装 npm 全局 Codex：{}",
                error
            )));
        }
    }

    let launcher_path = profile_dir.join("launch.cmd");
    let codex_run_command = codex_run_command(&codex_executable_path);

    tokio::fs::write(
        &launcher_path,
        [
            "@echo off".to_string(),
            "title Codex 实例".to_string(),
            format!("set \"CODEX_HOME={}\"", profile_dir.to_string_lossy()),
            format!("set \"OPENAI_API_KEY={}\"", token),
            format!("set \"OPENAI_BASE_URL={}\"", local_base_url),
            format!("set \"OPENAI_MODEL={}\"", model),
            format!("cd /d \"{}\"", paths.workspace_root),
            codex_run_command,
            String::new(),
        ]
        .join("\r\n"),
    )
    .await?;

    std::process::Command::new("cmd.exe")
        .args([
            "/d",
            "/c",
            "start",
            "",
            "cmd.exe",
            "/d",
            "/k",
            &launcher_path.to_string_lossy(),
        ])
        .current_dir(&paths.workspace_root)
        .env("CODEX_HOME", &profile_dir)
        .env("OPENAI_API_KEY", &token)
        .env("OPENAI_BASE_URL", &local_base_url)
        .env("OPENAI_MODEL", &model)
        .spawn()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(json!({
      "providerId": provider_id,
      "providerName": provider["name"],
      "profileDir": profile_dir.to_string_lossy().to_string()
    }))
}

pub async fn refresh_drift(paths: &AppPaths, cli_targets: &Value) -> Result<(), ManagerError> {
    let providers = read_array(&paths.storage_files.providers)?;
    let profiles = read_array(&paths.storage_files.runtime_profiles)?;
    let mut runtime_state = read_object(&paths.storage_files.runtime_provider_state)?;
    let schemas = runtime_config_schemas();
    let Some(schema_map) = schemas.as_object() else {
        return Ok(());
    };

    for (cli, schema) in schema_map {
        if schema.get("enabled").and_then(Value::as_bool) != Some(true)
            || !schema
                .get("configFiles")
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        {
            continue;
        }

        let cli_target = find_cli_target(cli_targets, cli).unwrap_or_else(|_| json!({}));
        let profile = profiles
            .iter()
            .find(|item| item.get("cli").and_then(Value::as_str) == Some(cli.as_str()));
        let previous_state = runtime_state
            .get(cli)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut next_state = previous_state.clone();

        if profile.is_none() {
            next_state.insert("activeProviderId".to_string(), json!(""));
            next_state.insert(
                "runtimePath".to_string(),
                json!(string_value(cli_target.get("configPath"))),
            );
            next_state.insert("status".to_string(), json!("NO_ACTIVE"));
            runtime_state.insert(cli.to_string(), Value::Object(next_state));
            continue;
        }

        let profile = profile.unwrap();
        let provider_id = string_value(profile.get("providerId"));
        let provider = providers
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(provider_id.as_str()));

        if provider.is_none() {
            next_state.insert("activeProviderId".to_string(), json!(provider_id));
            next_state.insert(
                "runtimePath".to_string(),
                json!(string_value(cli_target.get("configPath"))),
            );
            next_state.insert("status".to_string(), json!("NO_ACTIVE"));
            runtime_state.insert(cli.to_string(), Value::Object(next_state));
            continue;
        }

        let provider = provider.unwrap();
        let manager_files = build_cli_config_files(paths, cli, provider, profile)?;
        let config_path = string_value(cli_target.get("configPath"));
        let runtime_path = if config_path.is_empty() {
            String::new()
        } else {
            format_runtime_path(&config_path, &manager_files)
        };

        if config_path.is_empty() {
            next_state.insert("activeProviderId".to_string(), json!(provider["id"]));
            next_state.insert("runtimePath".to_string(), json!(runtime_path));
            next_state.insert("status".to_string(), json!("DIRTY_MANAGER"));
            runtime_state.insert(cli.to_string(), Value::Object(next_state));
            continue;
        }

        let runtime_files = read_runtime_config_files(cli, &cli_target).await?;
        let manager_hash = sha256_text(&combine_managed_config_contents(cli, &manager_files)?);
        let runtime_hash = sha256_text(&combine_managed_config_contents(cli, &runtime_files)?);
        let previous_hash = previous_state
            .get("runtimeHash")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut status = "SYNCED";

        if runtime_hash != manager_hash {
            if previous_hash.is_empty() {
                status = "MODIFIED_EXTERNALLY";
            } else if runtime_hash != previous_hash && manager_hash != previous_hash {
                status = "CONFLICT";
            } else if manager_hash != previous_hash {
                status = "DIRTY_MANAGER";
            } else {
                status = "MODIFIED_EXTERNALLY";
            }
        }

        next_state.insert("activeProviderId".to_string(), json!(provider["id"]));
        if status == "SYNCED" {
            next_state.insert("runtimeHash".to_string(), json!(runtime_hash));
        }
        next_state.insert("runtimePath".to_string(), json!(runtime_path));
        next_state.insert("status".to_string(), json!(status));
        runtime_state.insert(cli.to_string(), Value::Object(next_state));
    }

    write_json(
        &paths.storage_files.runtime_provider_state,
        &Value::Object(runtime_state),
    )
    .await
}

pub fn read_public_providers(paths: &AppPaths) -> Result<Value, ManagerError> {
    let providers = read_array(&paths.storage_files.providers)?;
    let keys = read_object(&paths.storage_files.runtime_provider_keys)?;

    Ok(json!(providers
        .into_iter()
        .map(|mut provider| {
            let provider_id = string_value(provider.get("id"));
            let api_key = keys
                .get(&provider_id)
                .and_then(Value::as_str)
                .and_then(|value| decrypt_provider_key(value).ok())
                .unwrap_or_default();

            provider["apiKey"] = json!(api_key);
            provider["hasApiKey"] = json!(keys.contains_key(&provider_id));
            provider
        })
        .collect::<Vec<_>>()))
}

pub fn read_public_profiles(paths: &AppPaths) -> Result<Value, ManagerError> {
    let providers = read_array(&paths.storage_files.providers)?;
    let profiles = read_array(&paths.storage_files.runtime_profiles)?;
    let keys = read_object(&paths.storage_files.runtime_provider_keys)?;

    Ok(json!(profiles
        .into_iter()
        .map(|mut profile| {
            let provider_id = string_value(profile.get("providerId"));

            if let Some(provider) = providers
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(provider_id.as_str()))
            {
                profile["providerName"] = provider.get("name").cloned().unwrap_or(Value::Null);
                profile["providerType"] = provider.get("type").cloned().unwrap_or(Value::Null);
                profile["hasApiKey"] = json!(keys.contains_key(&provider_id));
            }

            profile
        })
        .collect::<Vec<_>>()))
}

pub fn runtime_config_schemas() -> Value {
    json!({
      "claude": {
        "cli": "claude",
        "enabled": true,
        "defaultProviderType": "anthropic",
        "advancedFields": ["type", "authField"],
        "authFields": ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"],
        "modelFields": [
          { "key": "mainModel", "label": "主模型", "configKey": "ANTHROPIC_MODEL" },
          { "key": "haikuModel", "label": "Haiku 默认模型", "configKey": "ANTHROPIC_DEFAULT_HAIKU_MODEL" },
          { "key": "sonnetModel", "label": "Sonnet 默认模型", "configKey": "ANTHROPIC_DEFAULT_SONNET_MODEL" },
          { "key": "opusModel", "label": "Opus 默认模型", "configKey": "ANTHROPIC_DEFAULT_OPUS_MODEL" }
        ],
        "optionFields": [
          { "key": "hideAiSignature", "label": "隐藏 AI 署名", "type": "boolean" },
          { "key": "teammatesMode", "label": "Teammates 模式", "type": "boolean" },
          { "key": "toolSearch", "label": "启用 Tool Search", "type": "boolean" },
          { "key": "maxThinking", "label": "最大强度思考", "type": "boolean" },
          { "key": "disableUpgrade", "label": "禁用自动升级", "type": "boolean" }
        ],
        "configFiles": [
          {
            "name": "settings.json",
            "format": "JSON",
            "description": "Claude settings.json 配置内容",
            "template": "{\n  \"env\": {\n{{#hasApiKey}}\n    \"{{authField}}\": \"{{apiKey}}\",\n{{/hasApiKey}}\n{{#hasBaseUrl}}\n    \"ANTHROPIC_BASE_URL\": \"{{baseUrl}}\",\n{{/hasBaseUrl}}\n{{#hasMainModel}}\n    \"ANTHROPIC_MODEL\": \"{{mainModel}}\",\n{{/hasMainModel}}\n{{#hasHaikuModel}}\n    \"ANTHROPIC_DEFAULT_HAIKU_MODEL\": \"{{haikuModel}}\",\n{{/hasHaikuModel}}\n{{#hasOpusModel}}\n    \"ANTHROPIC_DEFAULT_OPUS_MODEL\": \"{{opusModel}}\",\n{{/hasOpusModel}}\n{{#hasSonnetModel}}\n    \"ANTHROPIC_DEFAULT_SONNET_MODEL\": \"{{sonnetModel}}\",\n{{/hasSonnetModel}}\n{{#toolSearch}}\n    \"ENABLE_TOOL_SEARCH\": \"{{toolSearchText}}\",\n{{/toolSearch}}\n{{#disableUpgrade}}\n    \"DISABLE_AUTOUPDATER\": \"{{disableUpgradeText}}\",\n{{/disableUpgrade}}\n  },\n  \"enabledPlugins\": {},\n  \"includeCoAuthoredBy\": {{includeCoAuthoredBy}},\n  \"pluginConfigs\": {},\n{{#teammatesMode}}\n  \"teammateMode\": \"{{teammateMode}}\",\n{{/teammatesMode}}\n  \"effortLevel\": \"{{effortLevel}}\"\n{{#hideAiSignature}}\n  ,\n  \"attribution\": {\n    \"commit\": \"\",\n    \"pr\": \"\"\n  }\n{{/hideAiSignature}}\n}"
          }
        ]
      },
      "codex": {
        "cli": "codex",
        "enabled": true,
        "defaultProviderType": "openai",
        "advancedFields": [],
        "authFields": ["OPENAI_API_KEY"],
        "modelFields": [
          {
            "key": "mainModel",
            "label": "模型名称",
            "configKey": "model",
            "description": "指定使用的模型，将自动更新到 config.toml 中"
          }
        ],
        "optionFields": [
          { "key": "modelContextWindowEnabled", "label": "1M 上下文窗口", "type": "boolean" },
          { "key": "modelAutoCompactTokenLimit", "label": "压缩阈值", "type": "number", "dependsOn": "modelContextWindowEnabled" },
          { "key": "serviceTierFast", "label": "开启 Fast 模式", "type": "boolean" },
          { "key": "modelReasoningEffort", "label": "思考强度", "type": "select", "options": ["low", "medium", "high", "xhigh"] }
        ],
        "configFiles": [
          {
            "name": "auth.json",
            "format": "JSON",
            "description": "Codex auth.json 配置内容",
            "template": "{\n  \"OPENAI_API_KEY\": \"{{apiKey}}\"\n}"
          },
          {
            "name": "config.toml",
            "format": "TOML",
            "description": "Codex config.toml 配置内容",
            "template": "model_provider = \"custom\"\nmodel = \"{{mainModel}}\"\nmodel_reasoning_effort = \"{{modelReasoningEffort}}\"\ndisable_response_storage = true\n{{#serviceTierFast}}\nservice_tier = \"fast\"\n{{/serviceTierFast}}\n{{#modelContextWindowEnabled}}\nmodel_context_window = 1000000\nmodel_auto_compact_token_limit = {{modelAutoCompactTokenLimit}}\n{{/modelContextWindowEnabled}}\n\n[model_providers]\n[model_providers.custom]\nname = \"custom\"\nwire_api = \"responses\"\nrequires_openai_auth = true\nbase_url = \"{{baseUrl}}\""
          }
        ]
      },
      "gemini": {
        "cli": "gemini",
        "enabled": false,
        "defaultProviderType": "gemini",
        "advancedFields": [],
        "authFields": ["GOOGLE_API_KEY"],
        "modelFields": [],
        "optionFields": [],
        "configFiles": []
      },
      "opencode": {
        "cli": "opencode",
        "enabled": false,
        "defaultProviderType": "openai",
        "advancedFields": [],
        "authFields": ["OPENAI_API_KEY"],
        "modelFields": [],
        "optionFields": [],
        "configFiles": []
      }
    })
}

fn normalize_provider(input: &Value, previous: Option<&Value>) -> Result<Value, ManagerError> {
    let cli = non_empty_string(
        input.get("cli"),
        previous.and_then(|item| item.get("cli")),
        "",
    );
    let name = non_empty_string(
        input.get("name"),
        previous.and_then(|item| item.get("name")),
        "",
    );
    let provider_type = non_empty_string(
        input.get("type"),
        previous.and_then(|item| item.get("type")),
        "custom",
    );

    if !["claude", "codex", "gemini", "opencode"].contains(&cli.as_str()) {
        return Err(ManagerError::System(format!(
            "不支持的 CLI Runtime：{}",
            cli
        )));
    }

    if name.is_empty() {
        return Err(ManagerError::System("Provider 名称不能为空".to_string()));
    }

    if ![
        "openai",
        "anthropic",
        "gemini",
        "openrouter",
        "deepseek",
        "custom",
    ]
    .contains(&provider_type.as_str())
    {
        return Err(ManagerError::System(format!(
            "不支持的 Provider 类型：{}",
            provider_type
        )));
    }

    Ok(json!({
      "id": non_empty_string(input.get("id"), previous.and_then(|item| item.get("id")), &create_provider_id()),
      "cli": cli,
      "icon": optional_string(input.get("icon"), previous.and_then(|item| item.get("icon"))),
      "name": name,
      "type": provider_type,
      "note": optional_string(input.get("note"), previous.and_then(|item| item.get("note"))),
      "website": optional_string(input.get("website"), previous.and_then(|item| item.get("website"))),
      "baseUrl": optional_string(input.get("baseUrl"), None),
      "proxy": optional_string(input.get("proxy"), None),
      "authField": optional_string(input.get("authField"), previous.and_then(|item| item.get("authField"))),
      "runtimeConfig": normalize_runtime_config(input.get("runtimeConfig").or_else(|| previous.and_then(|item| item.get("runtimeConfig")))),
      "headers": normalize_headers(input.get("headers")),
      "enabled": input.get("enabled").and_then(Value::as_bool).unwrap_or_else(|| previous.and_then(|item| item.get("enabled")).and_then(Value::as_bool).unwrap_or(true)),
      "createdAt": previous.and_then(|item| item.get("createdAt")).and_then(Value::as_u64).unwrap_or_else(now_millis),
      "updatedAt": now_millis()
    }))
}

fn normalize_model(input: &Value, previous: Option<&Value>) -> Result<Value, ManagerError> {
    let provider_id = non_empty_string(
        input.get("providerId"),
        previous.and_then(|item| item.get("providerId")),
        "",
    );
    let name = non_empty_string(
        input.get("name"),
        previous
            .and_then(|item| item.get("name"))
            .or_else(|| input.get("id")),
        "",
    );

    if provider_id.is_empty() {
        return Err(ManagerError::System("模型必须关联 Provider".to_string()));
    }

    if name.is_empty() {
        return Err(ManagerError::System("模型名称不能为空".to_string()));
    }

    Ok(json!({
      "id": non_empty_string(input.get("id"), previous.and_then(|item| item.get("id")), &name),
      "providerId": provider_id,
      "name": name,
      "contextWindow": optional_number(input.get("contextWindow"), previous.and_then(|item| item.get("contextWindow"))),
      "maxOutput": optional_number(input.get("maxOutput"), previous.and_then(|item| item.get("maxOutput"))),
      "supportsTools": input.get("supportsTools").and_then(Value::as_bool).or_else(|| previous.and_then(|item| item.get("supportsTools")).and_then(Value::as_bool)).unwrap_or(false),
      "supportsVision": input.get("supportsVision").and_then(Value::as_bool).or_else(|| previous.and_then(|item| item.get("supportsVision")).and_then(Value::as_bool)).unwrap_or(false),
      "supportsReasoning": input.get("supportsReasoning").and_then(Value::as_bool).or_else(|| previous.and_then(|item| item.get("supportsReasoning")).and_then(Value::as_bool)).unwrap_or(false)
    }))
}

fn normalize_profile(input: &Value, previous: Option<&Value>) -> Result<Value, ManagerError> {
    let cli = non_empty_string(
        input.get("cli"),
        previous.and_then(|item| item.get("cli")),
        "",
    );
    let provider_id = non_empty_string(
        input.get("providerId"),
        previous.and_then(|item| item.get("providerId")),
        "",
    );
    let model = non_empty_string(
        input.get("model"),
        previous.and_then(|item| item.get("model")),
        "",
    );

    if !["claude", "codex", "gemini", "opencode"].contains(&cli.as_str()) {
        return Err(ManagerError::System(format!(
            "不支持的 CLI Runtime：{}",
            cli
        )));
    }

    if provider_id.is_empty() {
        return Err(ManagerError::System(
            "Runtime Profile 必须选择 Provider".to_string(),
        ));
    }

    if model.is_empty() {
        return Err(ManagerError::System(
            "Runtime Profile 必须选择模型".to_string(),
        ));
    }

    Ok(json!({
      "id": non_empty_string(input.get("id"), previous.and_then(|item| item.get("id")), &cli),
      "cli": cli,
      "providerId": provider_id,
      "model": model,
      "baseUrl": optional_string(input.get("baseUrl"), None),
      "proxy": optional_string(input.get("proxy"), None),
      "env": normalize_headers(input.get("env")),
      "updatedAt": now_millis()
    }))
}

pub(crate) fn find_cli_target(cli_targets: &Value, cli: &str) -> Result<Value, ManagerError> {
    let mut cli_target = cli_targets
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(cli))
                .cloned()
        })
        .ok_or_else(|| ManagerError::System("CLI 配置目录不存在".to_string()))?;

    cli_target["configPath"] = json!(resolve_portable_path(&string_value(
        cli_target.get("configPath"),
    )));
    Ok(cli_target)
}

fn find_runtime_profile(paths: &AppPaths, cli: &str) -> Result<Value, ManagerError> {
    read_array(&paths.storage_files.runtime_profiles)?
        .into_iter()
        .find(|item| item.get("cli").and_then(Value::as_str) == Some(cli))
        .ok_or_else(|| ManagerError::System("Runtime Profile 不存在".to_string()))
}

pub(crate) fn find_provider(paths: &AppPaths, provider_id: &str) -> Result<Value, ManagerError> {
    read_array(&paths.storage_files.providers)?
        .into_iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(provider_id))
        .ok_or_else(|| ManagerError::System("Provider 不存在".to_string()))
}

fn to_public_profile(paths: &AppPaths, mut profile: Value) -> Result<Value, ManagerError> {
    let providers = read_array(&paths.storage_files.providers)?;
    let keys = read_object(&paths.storage_files.runtime_provider_keys)?;
    let provider_id = string_value(profile.get("providerId"));

    if let Some(provider) = providers
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(provider_id.as_str()))
    {
        profile["providerName"] = provider.get("name").cloned().unwrap_or(Value::Null);
        profile["providerType"] = provider.get("type").cloned().unwrap_or(Value::Null);
        profile["hasApiKey"] = json!(keys.contains_key(&provider_id));
    }

    Ok(profile)
}

fn ensure_proxy_disabled(paths: &AppPaths, cli: &str) -> Result<(), ManagerError> {
    let (path, cli_name) = match cli {
        "claude" => (&paths.storage_files.claude_proxy_config, "Claude"),
        "codex" => (&paths.storage_files.codex_proxy_config, "Codex"),
        _ => return Ok(()),
    };
    let config = read_object(path)?;

    if config.get("enabled").and_then(Value::as_bool) == Some(true) {
        return Err(ManagerError::System(format!(
            "请先关闭 {} 代理接管",
            cli_name
        )));
    }

    Ok(())
}

pub(crate) async fn write_cli_config(
    paths: &AppPaths,
    cli: &str,
    cli_target: Value,
) -> Result<(), ManagerError> {
    let profile = find_runtime_profile(paths, cli)?;
    let provider = find_provider(paths, &string_value(profile.get("providerId")))?;
    let config_path = string_value(cli_target.get("configPath"));

    if config_path.is_empty() {
        return Err(ManagerError::System("CLI 配置目录不存在".to_string()));
    }

    tokio::fs::create_dir_all(&config_path).await?;
    let files = build_cli_config_files(paths, cli, &provider, &profile)?;

    for file in &files {
        tokio::fs::write(
            Path::new(&config_path).join(string_value(file.get("name"))),
            string_value(file.get("content")),
        )
        .await?;
    }

    let mut runtime_state = read_object(&paths.storage_files.runtime_provider_state)?;
    runtime_state.insert(
        cli.to_string(),
        json!({
          "activeProviderId": provider["id"],
          "runtimeHash": sha256_text(&combine_managed_config_contents(cli, &files)?),
          "lastSyncAt": now_millis(),
          "runtimePath": format_runtime_path(&config_path, &files),
          "status": "SYNCED"
        }),
    );
    write_json(
        &paths.storage_files.runtime_provider_state,
        &Value::Object(runtime_state),
    )
    .await
}

pub(crate) fn build_cli_config_files(
    paths: &AppPaths,
    cli: &str,
    provider: &Value,
    profile: &Value,
) -> Result<Vec<Value>, ManagerError> {
    if cli == "claude" {
        return build_claude_config_files(paths, provider, profile);
    }

    if cli == "codex" {
        return build_codex_config_files(paths, provider, profile);
    }

    Ok(Vec::new())
}

fn build_claude_config_files(
    paths: &AppPaths,
    provider: &Value,
    profile: &Value,
) -> Result<Vec<Value>, ManagerError> {
    let api_key = get_provider_api_key(paths, &string_value(provider.get("id")))?;
    let values = create_template_values(provider, profile, &api_key);
    let mut env = Map::new();
    let auth_field = string_value(values.get("authField"));
    let base_url = string_value(values.get("baseUrl"));
    let main_model = string_value(values.get("mainModel"));
    let haiku_model = string_value(values.get("haikuModel"));
    let sonnet_model = string_value(values.get("sonnetModel"));
    let opus_model = string_value(values.get("opusModel"));
    let mut settings = Map::new();

    if !api_key.is_empty() {
        env.insert(auth_field, json!(api_key));
    }
    if !base_url.is_empty() {
        env.insert("ANTHROPIC_BASE_URL".to_string(), json!(base_url));
    }
    if !main_model.is_empty() {
        env.insert("ANTHROPIC_MODEL".to_string(), json!(main_model));
    }
    if !haiku_model.is_empty() {
        env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), json!(haiku_model));
    }
    if !opus_model.is_empty() {
        env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), json!(opus_model));
    }
    if !sonnet_model.is_empty() {
        env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), json!(sonnet_model));
    }
    if values.get("toolSearch").and_then(Value::as_bool) == Some(true) {
        env.insert("ENABLE_TOOL_SEARCH".to_string(), json!("true"));
    }
    if values.get("disableUpgrade").and_then(Value::as_bool) == Some(true) {
        env.insert("DISABLE_AUTOUPDATER".to_string(), json!("1"));
    }

    settings.insert("env".to_string(), Value::Object(env));
    settings.insert("enabledPlugins".to_string(), json!({}));
    settings.insert(
        "includeCoAuthoredBy".to_string(),
        json!(values.get("hideAiSignature").and_then(Value::as_bool) != Some(true)),
    );
    settings.insert("pluginConfigs".to_string(), json!({}));
    if values.get("teammatesMode").and_then(Value::as_bool) == Some(true) {
        settings.insert("teammateMode".to_string(), json!("tmux"));
    }
    settings.insert("effortLevel".to_string(), values["effortLevel"].clone());
    if values.get("hideAiSignature").and_then(Value::as_bool) == Some(true) {
        settings.insert(
            "attribution".to_string(),
            json!({
              "commit": "",
              "pr": ""
            }),
        );
    }

    Ok(vec![json!({
      "name": "settings.json",
      "content": format!("{}\n", serde_json::to_string_pretty(&Value::Object(settings))?)
    })])
}

fn build_codex_config_files(
    paths: &AppPaths,
    provider: &Value,
    profile: &Value,
) -> Result<Vec<Value>, ManagerError> {
    let api_key = get_provider_api_key(paths, &string_value(provider.get("id")))?;
    let values = create_template_values(provider, profile, &api_key);
    let mut config_lines = vec![
        "model_provider = \"custom\"".to_string(),
        format!("model = {}", to_toml_string(string_value(values.get("mainModel")))),
        format!(
            "model_reasoning_effort = {}",
            to_toml_string(string_value(values.get("modelReasoningEffort")))
        ),
        "disable_response_storage = true".to_string(),
    ];

    if values.get("serviceTierFast").and_then(Value::as_bool) == Some(true) {
        config_lines.push("service_tier = \"fast\"".to_string());
    }

    if values
        .get("modelContextWindowEnabled")
        .and_then(Value::as_bool)
        == Some(true)
    {
        config_lines.push("model_context_window = 1000000".to_string());
        config_lines.push(format!(
            "model_auto_compact_token_limit = {}",
            number_value(values.get("modelAutoCompactTokenLimit"), 900000)
        ));
    }

    config_lines.extend([
        String::new(),
        "[model_providers]".to_string(),
        "[model_providers.custom]".to_string(),
        "name = \"custom\"".to_string(),
        "wire_api = \"responses\"".to_string(),
        "requires_openai_auth = true".to_string(),
        format!("base_url = {}", to_toml_string(string_value(values.get("baseUrl")))),
    ]);

    Ok(vec![
        json!({
          "name": "auth.json",
          "content": format!("{}\n", serde_json::to_string_pretty(&json!({
            "OPENAI_API_KEY": api_key
          }))?)
        }),
        json!({
          "name": "config.toml",
          "content": format!("{}\n", config_lines.join("\n"))
        }),
    ])
}

pub(crate) fn get_provider_api_key(paths: &AppPaths, provider_id: &str) -> Result<String, ManagerError> {
    Ok(read_object(&paths.storage_files.runtime_provider_keys)?
        .get(provider_id)
        .and_then(Value::as_str)
        .map(decrypt_provider_key)
        .transpose()?
        .unwrap_or_default())
}

fn create_template_values(provider: &Value, profile: &Value, api_key: &str) -> Map<String, Value> {
    let runtime_config = provider.get("runtimeConfig").and_then(Value::as_object);
    let main_model = first_string(
        runtime_config.and_then(|item| item.get("mainModel")),
        if provider.get("cli").and_then(Value::as_str) == Some("claude") {
            None
        } else {
            profile.get("model")
        },
    );
    let haiku_model = string_from_map(runtime_config, "haikuModel", "");
    let sonnet_model = string_from_map(runtime_config, "sonnetModel", "");
    let opus_model = string_from_map(runtime_config, "opusModel", "");
    let base_url = first_string(profile.get("baseUrl"), provider.get("baseUrl"));
    let tool_search = bool_from_map(runtime_config, "toolSearch", false);
    let disable_upgrade = bool_from_map(runtime_config, "disableUpgrade", false);
    let hide_ai_signature = bool_from_map(runtime_config, "hideAiSignature", false);
    let teammates_mode = bool_from_map(runtime_config, "teammatesMode", true);
    let max_thinking = bool_from_map(runtime_config, "maxThinking", true);
    let model_context_window_enabled =
        bool_from_map(runtime_config, "modelContextWindowEnabled", false);
    let service_tier_fast = bool_from_map(runtime_config, "serviceTierFast", false);
    let model_reasoning_effort = string_from_map(runtime_config, "modelReasoningEffort", "low");
    let model_auto_compact_token_limit =
        number_from_map(runtime_config, "modelAutoCompactTokenLimit", 900000);
    let mut values = Map::new();

    values.insert(
        "authField".to_string(),
        json!(first_string(
            provider.get("authField"),
            Some(&json!("ANTHROPIC_AUTH_TOKEN"))
        )),
    );
    values.insert("apiKey".to_string(), json!(api_key));
    values.insert("hasApiKey".to_string(), json!(!api_key.is_empty()));
    values.insert("baseUrl".to_string(), json!(base_url));
    values.insert(
        "hasBaseUrl".to_string(),
        json!(!first_string(profile.get("baseUrl"), provider.get("baseUrl")).is_empty()),
    );
    values.insert("mainModel".to_string(), json!(main_model));
    values.insert(
        "hasMainModel".to_string(),
        json!(!string_value(values.get("mainModel")).is_empty()),
    );
    values.insert("haikuModel".to_string(), json!(haiku_model));
    values.insert(
        "hasHaikuModel".to_string(),
        json!(!string_value(values.get("haikuModel")).is_empty()),
    );
    values.insert("sonnetModel".to_string(), json!(sonnet_model));
    values.insert(
        "hasSonnetModel".to_string(),
        json!(!string_value(values.get("sonnetModel")).is_empty()),
    );
    values.insert("opusModel".to_string(), json!(opus_model));
    values.insert(
        "hasOpusModel".to_string(),
        json!(!string_value(values.get("opusModel")).is_empty()),
    );
    values.insert("toolSearch".to_string(), json!(tool_search));
    values.insert(
        "toolSearchText".to_string(),
        json!(if tool_search { "true" } else { "false" }),
    );
    values.insert("disableUpgrade".to_string(), json!(disable_upgrade));
    values.insert(
        "disableUpgradeText".to_string(),
        json!(if disable_upgrade { "1" } else { "0" }),
    );
    values.insert(
        "includeCoAuthoredBy".to_string(),
        json!(if hide_ai_signature { "false" } else { "true" }),
    );
    values.insert("hideAiSignature".to_string(), json!(hide_ai_signature));
    values.insert("teammatesMode".to_string(), json!(teammates_mode));
    values.insert("teammateMode".to_string(), json!("tmux"));
    values.insert(
        "effortLevel".to_string(),
        json!(if max_thinking { "max" } else { "default" }),
    );
    values.insert(
        "modelContextWindowEnabled".to_string(),
        json!(model_context_window_enabled),
    );
    values.insert("serviceTierFast".to_string(), json!(service_tier_fast));
    values.insert(
        "modelReasoningEffort".to_string(),
        json!(model_reasoning_effort),
    );
    values.insert(
        "modelAutoCompactTokenLimit".to_string(),
        json!(model_auto_compact_token_limit),
    );

    values
}

pub(crate) async fn read_runtime_config_files(
    cli: &str,
    cli_target: &Value,
) -> Result<Vec<Value>, ManagerError> {
    let schemas = runtime_config_schemas();
    let config_files = schemas[cli]["configFiles"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let config_path = string_value(cli_target.get("configPath"));
    let mut files = Vec::new();

    if config_files.is_empty() || config_path.is_empty() {
        return Ok(files);
    }

    for file in config_files {
        let name = string_value(file.get("name"));
        let file_path = Path::new(&config_path).join(&name);
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(ManagerError::Io(error)),
        };

        files.push(json!({
          "name": name,
          "content": content
        }));
    }

    Ok(files)
}

pub(crate) fn combine_config_contents(files: &[Value]) -> String {
    files
        .iter()
        .map(|file| {
            format!(
                "### {}\n{}",
                string_value(file.get("name")),
                file.get("content").and_then(Value::as_str).unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn combine_managed_config_contents(cli: &str, files: &[Value]) -> Result<String, ManagerError> {
    if cli == "claude" {
        return Ok(combine_config_contents(
            &files
                .iter()
                .map(|file| -> Result<Value, ManagerError> {
                    if file.get("name").and_then(Value::as_str) != Some("settings.json") {
                        return Ok(file.clone());
                    }

                    Ok(json!({
                      "name": "settings.json",
                      "content": normalize_claude_settings_content(string_value(file.get("content")))?
                    }))
                })
                .collect::<Result<Vec<_>, ManagerError>>()?,
        ));
    }

    if cli != "codex" {
        return Ok(combine_config_contents(files));
    }

    Ok(combine_config_contents(
        &files
            .iter()
            .map(|file| normalize_codex_config_file(file))
            .collect::<Result<Vec<_>, ManagerError>>()?,
    ))
}

fn normalize_claude_settings_content(content: String) -> Result<String, ManagerError> {
    let settings = if content.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&content)?
    };
    let mut normalized = Map::new();
    let mut normalized_env = Map::new();
    let managed_env_keys = [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ENABLE_TOOL_SEARCH",
        "DISABLE_AUTOUPDATER",
    ];

    if let Some(env) = settings.get("env").and_then(Value::as_object) {
        for key in managed_env_keys {
            if let Some(value) = env.get(key) {
                normalized_env.insert(key.to_string(), value.clone());
            }
        }
    }

    normalized.insert("env".to_string(), Value::Object(normalized_env));

    if let Some(value) = settings.get("includeCoAuthoredBy") {
        normalized.insert("includeCoAuthoredBy".to_string(), value.clone());
    }

    if let Some(value) = settings.get("teammateMode") {
        normalized.insert("teammateMode".to_string(), value.clone());
    }

    if let Some(value) = settings.get("effortLevel") {
        normalized.insert("effortLevel".to_string(), value.clone());
    }

    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&Value::Object(normalized))?
    ))
}

fn normalize_codex_config_file(file: &Value) -> Result<Value, ManagerError> {
    let name = string_value(file.get("name"));
    let content = string_value(file.get("content"));

    if name == "auth.json" {
        let auth = if content.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&content)?
        };

        return Ok(json!({
          "name": name,
          "content": format!("{}\n", serde_json::to_string_pretty(&json!({
            "OPENAI_API_KEY": string_value(auth.get("OPENAI_API_KEY"))
          }))?)
        }));
    }

    if name != "config.toml" {
        return Ok(file.clone());
    }

    let config = parse_simple_toml(&content);
    let root = config
        .get("root")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let sections = config
        .get("sections")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let custom_provider = sections
        .get("model_providers.custom")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut lines = vec![
        format!(
            "model_provider = {}",
            to_toml_string(first_string(
                root.get("model_provider"),
                Some(&json!("custom"))
            ))
        ),
        format!(
            "model = {}",
            to_toml_string(first_string(root.get("model"), Some(&json!(""))))
        ),
        format!(
            "model_reasoning_effort = {}",
            to_toml_string(first_string(
                root.get("model_reasoning_effort"),
                Some(&json!("low"))
            ))
        ),
        format!(
            "disable_response_storage = {}",
            if root
                .get("disable_response_storage")
                .and_then(Value::as_bool)
                == Some(false)
            {
                "false"
            } else {
                "true"
            }
        ),
    ];

    if root.get("service_tier").and_then(Value::as_str) == Some("fast") {
        lines.push("service_tier = \"fast\"".to_string());
    }

    if root.get("model_context_window").is_some() {
        lines.push(format!(
            "model_context_window = {}",
            number_value(root.get("model_context_window"), 0)
        ));
        lines.push(format!(
            "model_auto_compact_token_limit = {}",
            number_value(root.get("model_auto_compact_token_limit"), 900000)
        ));
    }

    lines.extend([
        "".to_string(),
        "[model_providers]".to_string(),
        "[model_providers.custom]".to_string(),
        format!(
            "name = {}",
            to_toml_string(first_string(
                custom_provider.get("name"),
                Some(&json!("custom"))
            ))
        ),
        format!(
            "wire_api = {}",
            to_toml_string(first_string(
                custom_provider.get("wire_api"),
                Some(&json!("responses"))
            ))
        ),
        format!(
            "requires_openai_auth = {}",
            if custom_provider
                .get("requires_openai_auth")
                .and_then(Value::as_bool)
                == Some(false)
            {
                "false"
            } else {
                "true"
            }
        ),
        format!(
            "base_url = {}",
            to_toml_string(first_string(
                custom_provider.get("base_url"),
                Some(&json!(""))
            ))
        ),
    ]);

    Ok(json!({
      "name": name,
      "content": format!("{}\n", lines.join("\n"))
    }))
}

fn format_runtime_path(config_path: &str, files: &[Value]) -> String {
    files
        .iter()
        .map(|file| {
            Path::new(config_path)
                .join(string_value(file.get("name")))
                .to_string_lossy()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn sync_runtime_config_to_manager(
    paths: &AppPaths,
    cli: &str,
    cli_target: Value,
) -> Result<(), ManagerError> {
    let profile = find_runtime_profile(paths, cli)?;
    let provider = find_provider(paths, &string_value(profile.get("providerId")))?;
    let config_path = string_value(cli_target.get("configPath"));

    if config_path.is_empty() {
        return Err(ManagerError::System("CLI 配置目录不存在".to_string()));
    }

    if cli == "claude" {
        return sync_claude_runtime_to_manager(paths, &config_path, provider, profile).await;
    }

    if cli == "codex" {
        return sync_codex_runtime_to_manager(paths, &config_path, provider, profile).await;
    }

    Ok(())
}

async fn sync_claude_runtime_to_manager(
    paths: &AppPaths,
    config_path: &str,
    provider: Value,
    profile: Value,
) -> Result<(), ManagerError> {
    let settings_path = Path::new(config_path).join("settings.json");
    let settings: Value = serde_json::from_str(&tokio::fs::read_to_string(settings_path).await?)?;
    let empty_env = Map::new();
    let env = settings
        .get("env")
        .and_then(Value::as_object)
        .unwrap_or(&empty_env);
    let auth_field = if env.contains_key("ANTHROPIC_API_KEY") {
        "ANTHROPIC_API_KEY".to_string()
    } else {
        first_string(
            provider.get("authField"),
            Some(&json!("ANTHROPIC_AUTH_TOKEN")),
        )
    };
    let main_model = first_string(env.get("ANTHROPIC_MODEL"), profile.get("model"));

    save_provider(
        paths,
        json!({
          "id": provider["id"],
          "cli": provider["cli"],
          "icon": provider["icon"],
          "name": provider["name"],
          "type": provider["type"],
          "note": provider["note"],
          "website": provider["website"],
          "baseUrl": string_value(env.get("ANTHROPIC_BASE_URL")),
          "authField": auth_field,
          "apiKey": string_value(env.get(&auth_field)),
          "runtimeConfig": {
            "mainModel": main_model,
            "haikuModel": string_value(env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")),
            "sonnetModel": string_value(env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")),
            "opusModel": string_value(env.get("ANTHROPIC_DEFAULT_OPUS_MODEL")),
            "toolSearch": string_value(env.get("ENABLE_TOOL_SEARCH")) == "true",
            "disableUpgrade": string_value(env.get("DISABLE_AUTOUPDATER")) == "1",
            "hideAiSignature": settings.get("includeCoAuthoredBy").and_then(Value::as_bool) == Some(false),
            "teammatesMode": settings.get("teammateMode").and_then(Value::as_str) == Some("tmux"),
            "maxThinking": settings.get("effortLevel").and_then(Value::as_str) == Some("max")
          },
          "headers": provider["headers"],
          "enabled": provider["enabled"]
        }),
    )
    .await?;

    if !main_model.is_empty() {
        save_runtime_model(
            paths,
            json!({
              "id": format!("{}:{}", string_value(provider.get("id")), main_model),
              "providerId": provider["id"],
              "name": main_model
            }),
        )
        .await?;
        switch_runtime_profile_only(
            paths,
            json!({
              "id": profile["id"],
              "cli": profile["cli"],
              "providerId": profile["providerId"],
              "model": main_model,
              "baseUrl": string_value(env.get("ANTHROPIC_BASE_URL"))
            }),
        )
        .await?;
    }

    Ok(())
}

async fn sync_codex_runtime_to_manager(
    paths: &AppPaths,
    config_path: &str,
    provider: Value,
    profile: Value,
) -> Result<(), ManagerError> {
    let auth_path = Path::new(config_path).join("auth.json");
    let config_path = Path::new(config_path).join("config.toml");
    let auth: Value = serde_json::from_str(&tokio::fs::read_to_string(auth_path).await?)?;
    let config = parse_simple_toml(&tokio::fs::read_to_string(config_path).await?);
    let root = config
        .get("root")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let sections = config
        .get("sections")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let custom_provider = sections
        .get("model_providers.custom")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let model = first_string(root.get("model"), profile.get("model"));
    let api_key = first_string(
        auth.get("OPENAI_API_KEY"),
        auth.get("tokens")
            .and_then(|tokens| tokens.get("access_token")),
    );

    save_provider(
        paths,
        json!({
          "id": provider["id"],
          "cli": provider["cli"],
          "icon": provider["icon"],
          "name": provider["name"],
          "type": provider["type"],
          "note": provider["note"],
          "website": provider["website"],
          "baseUrl": string_value(custom_provider.get("base_url")),
          "apiKey": api_key,
          "runtimeConfig": {
            "mainModel": model,
            "modelReasoningEffort": first_string(root.get("model_reasoning_effort"), Some(&json!("low"))),
            "serviceTierFast": root.get("service_tier").and_then(Value::as_str) == Some("fast"),
            "modelContextWindowEnabled": root.get("model_context_window").is_some(),
            "modelAutoCompactTokenLimit": number_value(root.get("model_auto_compact_token_limit"), 900000)
          },
          "headers": provider["headers"],
          "enabled": provider["enabled"]
        }),
    )
    .await?;

    if !model.is_empty() {
        save_runtime_model(
            paths,
            json!({
              "id": format!("{}:{}", string_value(provider.get("id")), model),
              "providerId": provider["id"],
              "name": model
            }),
        )
        .await?;
        switch_runtime_profile_only(
            paths,
            json!({
              "id": profile["id"],
              "cli": profile["cli"],
              "providerId": profile["providerId"],
              "model": model,
              "baseUrl": string_value(custom_provider.get("base_url"))
            }),
        )
        .await?;
    }

    Ok(())
}

async fn switch_runtime_profile_only(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let cli = string_value(payload.get("cli"));
    let mut profiles = read_array(&paths.storage_files.runtime_profiles)?;
    let previous = profiles
        .iter()
        .find(|item| item.get("cli").and_then(Value::as_str) == Some(cli.as_str()))
        .cloned();
    let profile = normalize_profile(&payload, previous.as_ref())?;

    if previous.is_some() {
        profiles = profiles
            .into_iter()
            .map(|item| {
                if item.get("cli").and_then(Value::as_str) == Some(cli.as_str()) {
                    profile.clone()
                } else {
                    item
                }
            })
            .collect();
    } else {
        profiles.push(profile);
    }

    write_json(&paths.storage_files.runtime_profiles, &json!(profiles)).await
}

pub(crate) fn parse_simple_toml(content: &str) -> Value {
    let mut root = Map::new();
    let mut sections: Map<String, Value> = Map::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let text = line.trim();

        if text.is_empty() || text.starts_with('#') {
            continue;
        }

        if text.starts_with('[') && text.ends_with(']') {
            current_section = text
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();
            sections
                .entry(current_section.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            continue;
        }

        let Some(equal_index) = text.find('=') else {
            continue;
        };
        let key = text[..equal_index].trim().to_string();
        let value = parse_toml_value(text[equal_index + 1..].trim());

        if current_section.is_empty() {
            root.insert(key, value);
            continue;
        }

        if let Some(section) = sections
            .get_mut(&current_section)
            .and_then(Value::as_object_mut)
        {
            section.insert(key, value);
        }
    }

    json!({
      "root": root,
      "sections": sections
    })
}

fn parse_toml_value(value: &str) -> Value {
    let text = value.trim();

    if text.starts_with('"') && text.ends_with('"') {
        return serde_json::from_str(text).unwrap_or_else(|_| json!(text.trim_matches('"')));
    }

    if text.chars().all(|item| item.is_ascii_digit()) {
        return text
            .parse::<u64>()
            .map(|value| json!(value))
            .unwrap_or_else(|_| json!(text));
    }

    if text == "true" {
        return json!(true);
    }

    if text == "false" {
        return json!(false);
    }

    json!(text)
}

pub(crate) fn to_toml_string(value: String) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| "\"\"".to_string())
}

pub(crate) fn first_string(value: Option<&Value>, fallback: Option<&Value>) -> String {
    let text = string_value(value);

    if text.is_empty() {
        string_value(fallback)
    } else {
        text
    }
}

fn sha256_text(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

fn normalize_runtime_config(value: Option<&Value>) -> Value {
    let input = value.and_then(Value::as_object);
    let model_reasoning_effort = string_from_map(input, "modelReasoningEffort", "low");

    json!({
      "mainModel": optional_string_from_map(input, "mainModel"),
      "haikuModel": optional_string_from_map(input, "haikuModel"),
      "sonnetModel": optional_string_from_map(input, "sonnetModel"),
      "opusModel": optional_string_from_map(input, "opusModel"),
      "toolSearch": bool_from_map(input, "toolSearch", false),
      "disableUpgrade": bool_from_map(input, "disableUpgrade", false),
      "hideAiSignature": bool_from_map(input, "hideAiSignature", false),
      "teammatesMode": bool_from_map(input, "teammatesMode", true),
      "maxThinking": bool_from_map(input, "maxThinking", true),
      "modelContextWindowEnabled": bool_from_map(input, "modelContextWindowEnabled", false),
      "serviceTierFast": bool_from_map(input, "serviceTierFast", false),
      "modelReasoningEffort": model_reasoning_effort,
      "modelAutoCompactTokenLimit": number_from_map(input, "modelAutoCompactTokenLimit", 900000)
    })
}

fn normalize_headers(value: Option<&Value>) -> Value {
    let Some(map) = value.and_then(Value::as_object) else {
        return json!({});
    };
    let mut headers = Map::new();

    for (key, item) in map {
        let key = key.trim().to_string();
        let value = string_value(Some(item));

        if !key.is_empty() && !value.is_empty() {
            headers.insert(key, json!(value));
        }
    }

    Value::Object(headers)
}

pub(crate) fn set_provider_key(
    keys: &mut Map<String, Value>,
    provider_id: &str,
    api_key: String,
) -> Result<(), ManagerError> {
    if api_key.is_empty() {
        keys.remove(provider_id);
    } else {
        keys.insert(
            provider_id.to_string(),
            json!(encrypt_provider_key(&api_key)?),
        );
    }

    Ok(())
}

fn encrypt_provider_key(value: &str) -> Result<String, ManagerError> {
    let mut iv = [0u8; 12];
    getrandom::getrandom(&mut iv).map_err(|error| ManagerError::System(error.to_string()))?;
    let secret = runtime_secret();
    let cipher = Aes256Gcm::new_from_slice(&secret)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&iv), value.as_bytes())
        .map_err(|error| ManagerError::System(format!("{:?}", error)))?;
    let tag_index = encrypted.len() - 16;
    let engine = base64::engine::general_purpose::STANDARD;

    Ok(format!(
        "{}.{}.{}",
        engine.encode(iv),
        engine.encode(&encrypted[tag_index..]),
        engine.encode(&encrypted[..tag_index])
    ))
}

pub(crate) fn decrypt_provider_key(value: &str) -> Result<String, ManagerError> {
    let mut parts = value.split('.');
    let engine = base64::engine::general_purpose::STANDARD;
    let iv = engine
        .decode(parts.next().unwrap_or(""))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let tag = engine
        .decode(parts.next().unwrap_or(""))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let encrypted = engine
        .decode(parts.next().unwrap_or(""))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let mut payload = encrypted;

    payload.extend(tag);

    let decrypted = decrypt_provider_payload(&iv, &payload, &runtime_secret())
        .or_else(|_| decrypt_provider_payload(&iv, &payload, &legacy_runtime_secret()))?;

    String::from_utf8(decrypted).map_err(|error| ManagerError::System(error.to_string()))
}

fn decrypt_provider_payload(
    iv: &[u8],
    payload: &[u8],
    secret: &[u8; 32],
) -> Result<Vec<u8>, ManagerError> {
    let cipher = Aes256Gcm::new_from_slice(secret)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    cipher
        .decrypt(Nonce::from_slice(iv), payload)
        .map_err(|error| ManagerError::System(format!("{:?}", error)))
}

fn runtime_secret() -> [u8; 32] {
    let digest = Sha256::digest(b"ai-manager-runtime-provider|v2|fixed");
    let mut secret = [0u8; 32];

    secret.copy_from_slice(&digest);
    secret
}

fn legacy_runtime_secret() -> [u8; 32] {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
    let digest = Sha256::digest(format!("{}|ai-manager-runtime-provider", user_profile));
    let mut secret = [0u8; 32];

    secret.copy_from_slice(&digest);
    secret
}

fn default_models(provider_type: &str) -> Vec<&'static str> {
    match provider_type {
        "openai" => vec!["gpt-5.2", "gpt-5.1"],
        "anthropic" => vec!["claude-sonnet-4-5", "claude-opus-4-1"],
        "gemini" => vec!["gemini-2.5-pro", "gemini-2.5-flash"],
        "openrouter" => vec!["openai/gpt-5.2", "anthropic/claude-sonnet-4.5"],
        "deepseek" => vec!["deepseek-chat", "deepseek-reasoner"],
        _ => Vec::new(),
    }
}

fn non_empty_string(
    value: Option<&Value>,
    fallback: Option<&Value>,
    default_value: &str,
) -> String {
    let text = string_value(value);

    if !text.is_empty() {
        return text;
    }

    let text = string_value(fallback);

    if !text.is_empty() {
        return text;
    }

    default_value.to_string()
}

fn optional_string(value: Option<&Value>, fallback: Option<&Value>) -> Value {
    let text = non_empty_string(value, fallback, "");

    if text.is_empty() {
        Value::Null
    } else {
        json!(text)
    }
}

fn optional_string_from_map(input: Option<&Map<String, Value>>, key: &str) -> Value {
    let text = input
        .and_then(|map| map.get(key))
        .map(|value| string_value(Some(value)))
        .unwrap_or_default();

    if text.is_empty() {
        Value::Null
    } else {
        json!(text)
    }
}

fn string_from_map(input: Option<&Map<String, Value>>, key: &str, fallback: &str) -> String {
    let text = input
        .and_then(|map| map.get(key))
        .map(|value| string_value(Some(value)))
        .unwrap_or_default();

    if text.is_empty() {
        fallback.to_string()
    } else {
        text
    }
}

fn optional_number(value: Option<&Value>, fallback: Option<&Value>) -> Value {
    let number = value
        .and_then(Value::as_u64)
        .or_else(|| fallback.and_then(Value::as_u64))
        .unwrap_or(0);

    if number == 0 {
        Value::Null
    } else {
        json!(number)
    }
}

fn bool_from_map(input: Option<&Map<String, Value>>, key: &str, fallback: bool) -> bool {
    input
        .and_then(|map| map.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(fallback)
}

fn number_from_map(input: Option<&Map<String, Value>>, key: &str, fallback: u64) -> u64 {
    input
        .and_then(|map| map.get(key))
        .and_then(Value::as_u64)
        .unwrap_or(fallback)
}

pub(crate) fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

pub(crate) fn read_array(path: &str) -> Result<Vec<Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

pub(crate) fn read_object(path: &str) -> Result<Map<String, Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str::<Value>(&content)?
            .as_object()
            .cloned()
            .unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

pub(crate) async fn write_json(path: &str, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )
    .await?;
    Ok(())
}

fn create_provider_id() -> String {
    format!(
        "provider-{}-{}",
        now_millis(),
        PROVIDER_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn slugify_name(value: &str) -> String {
    let mut output = String::new();

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        } else if character == '-' || character == '_' || character.is_whitespace() {
            output.push('-');
        }
    }

    output
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

async fn resolve_codex_executable_path(executable_path: &str) -> String {
    let path = Path::new(executable_path);

    if path.extension().is_none() {
        let cmd_path = format!("{}.cmd", executable_path);

        if tokio::fs::metadata(&cmd_path).await.is_ok() {
            return cmd_path;
        }
    }

    executable_path.to_string()
}

fn codex_run_command(executable_path: &str) -> String {
    let lower = executable_path.to_lowercase();

    if lower.ends_with(".cmd") || lower.ends_with(".bat") {
        return format!("call \"{}\"", executable_path);
    }

    if lower.ends_with(".js") {
        return format!("node \"{}\"", executable_path);
    }

    format!("\"{}\"", executable_path)
}

trait EmptyStringExt {
    fn if_empty_then(self, value: impl FnOnce() -> String) -> String;
}

impl EmptyStringExt for String {
    fn if_empty_then(self, value: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            value()
        } else {
            self
        }
    }
}

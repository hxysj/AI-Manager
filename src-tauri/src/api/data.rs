use crate::api::runtime_provider;
use crate::core::error::ManagerError;
use crate::core::paths::{ensure_app_directories, home_path, path_text, AppPaths};
use crate::core::settings::{
    non_empty_string, number_value, serialize_app_settings, serialize_portable_path, string_value,
    write_json_file, AppSettings, CloudSyncSettings,
};
use crate::core::storage_state::create_initial_state;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};

pub struct DataBackupCache {
    drafts: HashMap<String, Value>,
    local_backup_running: bool,
}

impl DataBackupCache {
    pub fn new() -> Self {
        Self {
            drafts: HashMap::new(),
            local_backup_running: false,
        }
    }

    fn cache_restore_backup(&mut self, content: String, source: Value) -> Result<String, ManagerError> {
        let restore_id = create_restore_id()?;

        self.drafts.insert(
            restore_id.clone(),
            json!({
              "content": content,
              "source": source,
              "createdAt": now_millis()
            }),
        );

        Ok(restore_id)
    }

    fn get_restore_backup_draft(&self, restore_id: &str) -> Result<Value, ManagerError> {
        self.drafts
            .get(restore_id)
            .cloned()
            .ok_or_else(|| ManagerError::System("恢复预览已失效，请重新选择备份".to_string()))
    }

    fn delete_restore_backup_draft(&mut self, restore_id: &str) {
        self.drafts.remove(restore_id);
    }

    pub fn begin_local_backup(&mut self) -> bool {
        if self.local_backup_running {
            return false;
        }

        self.local_backup_running = true;
        true
    }

    pub fn finish_local_backup(&mut self) {
        self.local_backup_running = false;
    }
}

pub async fn export_data_backup(app: &AppHandle, paths: &AppPaths, app_settings: &AppSettings) -> Result<Value, ManagerError> {
    let desktop_path = app
        .path()
        .desktop_dir()
        .unwrap_or_else(|_| home_path());
    let file_name = format!(
        "monkey-thief-{}.aimbackup",
        chrono::Local::now().format("%Y-%m-%d")
    );
    let mut dialog = app
        .dialog()
        .file()
        .set_title("导出配置数据")
        .set_directory(desktop_path)
        .set_file_name(file_name)
        .add_filter("Monkey Thief 备份", &["aimbackup"]);

    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }

    let Some(file_path) = dialog.blocking_save_file() else {
        return Ok(json!({ "canceled": true }));
    };
    let file_path = file_path_text(file_path)?;

    tokio::fs::write(&file_path, create_data_backup(paths, app_settings, false).await?).await?;

    Ok(json!({
      "canceled": false,
      "filePath": file_path
    }))
}

pub async fn preview_data_backup_restore(
    app: &AppHandle,
    paths: &AppPaths,
    cache: &mut DataBackupCache,
) -> Result<Value, ManagerError> {
    let desktop_path = app
        .path()
        .desktop_dir()
        .unwrap_or_else(|_| home_path());
    let mut dialog = app
        .dialog()
        .file()
        .set_title("恢复配置数据")
        .set_directory(desktop_path)
        .add_filter("Monkey Thief 备份", &["aimbackup"]);

    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }

    let Some(file_path) = dialog.blocking_pick_file() else {
        return Ok(json!({ "canceled": true }));
    };
    let file_path = file_path_text(file_path)?;
    let content = tokio::fs::read_to_string(&file_path).await?;
    let restore_id = cache.cache_restore_backup(
        content.clone(),
        json!({
          "type": "file",
          "filePath": file_path
        }),
    )?;

    Ok(json!({
      "canceled": false,
      "restoreId": restore_id,
      "filePath": file_path,
      "preview": preview_data_backup_restore_content(paths, &content).await?
    }))
}

pub async fn restore_data_backup(
    paths: &AppPaths,
    app_settings: &AppSettings,
    state: &mut Value,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let restore_id = string_value(payload.get("restoreId"));
    let draft = cache.get_restore_backup_draft(&restore_id)?;
    let choices = payload.get("choices").cloned().unwrap_or_else(|| json!({}));

    restore_data_backup_content(paths, &string_value(draft.get("content")), &choices).await?;
    cache.delete_restore_backup_draft(&restore_id);
    *state = create_initial_state(paths, app_settings)?;

    Ok(json!({
      "canceled": false,
      "state": state
    }))
}

pub async fn list_local_backups(app_data_path: &Path) -> Result<Value, ManagerError> {
    get_local_backups_payload(app_data_path).await
}

pub async fn create_local_backup_now(
    app_data_path: &Path,
    paths: &AppPaths,
    app_settings: &mut AppSettings,
    state: &mut Value,
    cache: &mut DataBackupCache,
) -> Result<Value, ManagerError> {
    let backup = create_local_backup(app_data_path, paths, app_settings, cache).await?;
    *state = create_initial_state(paths, app_settings)?;

    Ok(json!({
      "backup": backup,
      "directory": get_local_backup_directory(app_data_path),
      "backups": list_local_backup_files(app_data_path).await?,
      "state": state
    }))
}

pub async fn create_local_backup_if_due(
    app_data_path: &Path,
    paths: &AppPaths,
    app_settings: &mut AppSettings,
) -> Result<Option<Value>, ManagerError> {
    if !app_settings.local_backup.enabled {
        return Ok(None);
    }

    let interval_ms = app_settings.local_backup.interval_minutes * 60 * 1000;

    if now_millis().saturating_sub(app_settings.local_backup.last_backup_at) < interval_ms {
        return Ok(None);
    }

    Ok(Some(create_local_backup_inner(app_data_path, paths, app_settings).await?))
}

pub async fn preview_local_backup_restore(
    app_data_path: &Path,
    paths: &AppPaths,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let backup_id = string_value(payload.get("backupId"));
    let file_path = get_local_backup_path(app_data_path, &backup_id)?;
    let content = tokio::fs::read_to_string(&file_path).await?;
    let restore_id = cache.cache_restore_backup(
        content.clone(),
        json!({
          "type": "local",
          "backupId": backup_id,
          "filePath": path_text(&file_path)
        }),
    )?;

    Ok(json!({
      "restoreId": restore_id,
      "fileName": file_path.file_name().map(|value| value.to_string_lossy().to_string()).unwrap_or_default(),
      "filePath": path_text(&file_path),
      "preview": preview_data_backup_restore_content(paths, &content).await?
    }))
}

pub async fn restore_local_backup(
    app_data_path: &Path,
    paths: &AppPaths,
    app_settings: &AppSettings,
    state: &mut Value,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let restore_id = string_value(payload.get("restoreId"));
    let draft = cache.get_restore_backup_draft(&restore_id)?;
    let choices = payload.get("choices").cloned().unwrap_or_else(|| json!({}));

    restore_data_backup_content(paths, &string_value(draft.get("content")), &choices).await?;
    cache.delete_restore_backup_draft(&restore_id);
    *state = create_initial_state(paths, app_settings)?;

    Ok(json!({
      "canceled": false,
      "directory": get_local_backup_directory(app_data_path),
      "backups": list_local_backup_files(app_data_path).await?,
      "state": state
    }))
}

pub async fn push_cloud_backup(
    paths: &AppPaths,
    app_settings: &mut AppSettings,
    state: &mut Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let cloud_sync = normalize_cloud_sync_settings(&payload);
    let last_updated_at = now_millis();

    upload_webdav_backup(
        &cloud_sync,
        create_data_backup(paths, app_settings, false).await?,
    )
    .await?;
    app_settings.cloud_sync = CloudSyncSettings {
        last_updated_at,
        ..cloud_sync.clone()
    };
    write_json_file(
        Path::new(&app_settings.settings_file_path),
        &serialize_app_settings(app_settings),
    )
    .await?;
    state["appSettings"] = serde_json::to_value(&*app_settings)?;

    Ok(json!({
      "uploadedAt": last_updated_at,
      "fileName": cloud_sync.file_name,
      "state": state
    }))
}

pub async fn preview_cloud_backup_restore(
    paths: &AppPaths,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let cloud_sync = normalize_cloud_sync_settings(&payload);
    let content = download_webdav_backup(&cloud_sync).await?;
    let restore_id = cache.cache_restore_backup(
        content.clone(),
        json!({
          "type": "cloud",
          "cloudSync": cloud_sync
        }),
    )?;

    Ok(json!({
      "restoreId": restore_id,
      "fileName": cloud_sync.file_name,
      "preview": preview_data_backup_restore_content(paths, &content).await?
    }))
}

pub async fn inspect_cloud_backup(payload: Value) -> Result<Value, ManagerError> {
    let cloud_sync = normalize_cloud_sync_settings(&payload);
    let content = download_webdav_backup(&cloud_sync).await?;

    Ok(json!({
      "fileName": cloud_sync.file_name,
      "backup": inspect_data_backup(&content)?
    }))
}

pub async fn pull_cloud_backup(
    paths: &AppPaths,
    app_settings: &mut AppSettings,
    state: &mut Value,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let restore_id = string_value(payload.get("restoreId"));
    let draft = cache.get_restore_backup_draft(&restore_id)?;
    let cloud_sync_payload = payload
        .get("cloudSync")
        .cloned()
        .unwrap_or_else(|| draft["source"]["cloudSync"].clone());
    let cloud_sync = normalize_cloud_sync_settings(&cloud_sync_payload);
    let choices = payload.get("choices").cloned().unwrap_or_else(|| json!({}));
    let last_updated_at = now_millis();

    restore_data_backup_content(paths, &string_value(draft.get("content")), &choices).await?;
    cache.delete_restore_backup_draft(&restore_id);
    app_settings.cloud_sync = CloudSyncSettings {
        last_updated_at,
        ..cloud_sync.clone()
    };
    write_json_file(
        Path::new(&app_settings.settings_file_path),
        &serialize_app_settings(app_settings),
    )
    .await?;
    *state = create_initial_state(paths, app_settings)?;

    Ok(json!({
      "downloadedAt": last_updated_at,
      "fileName": cloud_sync.file_name,
      "state": state
    }))
}

pub async fn create_data_backup(
    paths: &AppPaths,
    app_settings: &AppSettings,
    include_git_tool_data: bool,
) -> Result<String, ManagerError> {
    let provider_keys = export_provider_keys(paths)?;

    encrypt_backup_payload(&json!({
      "version": 1,
      "createdAt": now_millis(),
      "appSettings": serialize_backup_app_settings(app_settings),
      "workspaceEntries": collect_backup_entries(paths, include_git_tool_data).await?,
      "runtimeProviderKeys": encrypt_backup_data(&provider_keys)?
    }))
}

fn serialize_backup_app_settings(app_settings: &AppSettings) -> Value {
    let mut payload = serialize_app_settings(app_settings);

    if let Some(payload) = payload.as_object_mut() {
        payload.remove("restartRequired");
    }

    payload
}

async fn preview_data_backup_restore_content(paths: &AppPaths, content: &str) -> Result<Value, ManagerError> {
    let backup = parse_backup(content)?;
    let mut preview = create_restore_preview(&paths.workspace_root, backup["workspaceEntries"].as_array().cloned().unwrap_or_default()).await?;

    preview["createdAt"] = json!(number_value(backup.get("createdAt"), 0));
    Ok(preview)
}

fn inspect_data_backup(content: &str) -> Result<Value, ManagerError> {
    let backup = parse_backup(content)?;
    let app_settings_content = format!(
        "{}\n",
        serde_json::to_string_pretty(backup.get("appSettings").unwrap_or(&json!({})))?
    );
    let runtime_provider_keys = if let Some(keys) = backup.get("runtimeProviderKeys").and_then(Value::as_str) {
        decrypt_backup_data(keys)?
    } else {
        json!({})
    };
    let mut entries = vec![create_backup_view_entry(
        "app-settings.json",
        "应用设置",
        app_settings_content,
    )];

    for entry in backup["workspaceEntries"].as_array().cloned().unwrap_or_default() {
        entries.push(create_backup_entry_view(&entry)?);
    }

    if backup.get("runtimeProviderKeys").is_some() {
        entries.push(create_backup_view_entry(
            "runtime-provider-keys",
            "Runtime 密钥",
            format!(
                "已加密保存 {} 个 Provider 密钥，查看器不展开密钥明文。\n",
                runtime_provider_keys
                    .as_object()
                    .map(|value| value.len())
                    .unwrap_or_default()
            ),
        ));
    }

    Ok(json!({
      "version": backup.get("version").cloned().unwrap_or(Value::Null),
      "createdAt": number_value(backup.get("createdAt"), 0),
      "entryCount": entries.len(),
      "fileCount": entries.iter().filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file")).count(),
      "directoryCount": entries.iter().filter(|entry| entry.get("type").and_then(Value::as_str) == Some("dir")).count(),
      "entries": entries
    }))
}

async fn restore_data_backup_content(
    paths: &AppPaths,
    content: &str,
    choices: &Value,
) -> Result<(), ManagerError> {
    let backup = parse_backup(content)?;
    let choices = choices.as_object().cloned().unwrap_or_default();
    let runtime_provider_keys = backup
        .get("runtimeProviderKeys")
        .and_then(Value::as_str)
        .map(decrypt_backup_data)
        .transpose()?;

    ensure_app_directories(paths).await?;
    restore_directory_entries(
        &paths.workspace_root,
        backup["workspaceEntries"].as_array().cloned().unwrap_or_default(),
        &choices,
    )
    .await?;
    migrate_workspace_data(paths).await?;
    write_json(&paths.storage_files.cli_targets, &json!([])).await?;

    if let Some(runtime_provider_keys) = runtime_provider_keys {
        merge_provider_keys(paths, &runtime_provider_keys, &choices).await?;
    }

    Ok(())
}

async fn collect_backup_entries(
    paths: &AppPaths,
    include_git_tool_data: bool,
) -> Result<Vec<Value>, ManagerError> {
    let storage_files = vec![
        (&paths.storage_files.skill_repositories, "storage/skill-repositories.json"),
        (&paths.storage_files.skills, "storage/skills.json"),
        (&paths.storage_files.installs, "storage/installs.json"),
        (&paths.storage_files.usage_pricing, "storage/usage-pricing.json"),
        (&paths.storage_files.providers, "storage/providers.json"),
        (&paths.storage_files.runtime_models, "storage/runtime-models.json"),
        (&paths.storage_files.runtime_profiles, "storage/runtime-profiles.json"),
        (
            &paths.storage_files.runtime_provider_state,
            "storage/runtime-provider-state.json",
        ),
        (
            &paths.storage_files.runtime_provider_keys,
            "storage/runtime-provider-keys.json",
        ),
        (
            &paths.storage_files.claude_proxy_config,
            "storage/claude-proxy-config.json",
        ),
        (
            &paths.storage_files.claude_proxy_request_logs,
            "storage/claude-proxy-request-logs.json",
        ),
        (
            &paths.storage_files.codex_proxy_config,
            "storage/codex-proxy-config.json",
        ),
        (
            &paths.storage_files.codex_proxy_request_logs,
            "storage/codex-proxy-request-logs.json",
        ),
        (&paths.storage_files.codex_accounts, "storage/codex-accounts.json"),
        (
            &paths.storage_files.codex_active_account_id,
            "storage/codex-active-account-id.json",
        ),
        (&paths.storage_files.rules, "storage/rules.json"),
        (
            &paths.storage_files.prompt_runtime_state,
            "storage/prompt-runtime-state.json",
        ),
    ];
    let mut entries = Vec::new();

    for (source_path, relative_path) in storage_files {
        if let Some(entry) = collect_file_entry(source_path, relative_path).await? {
            entries.push(entry);
        }
    }

    let mut source_dirs = vec![
        PathBuf::from(&paths.skills_dir),
        PathBuf::from(&paths.prompts_dir),
        PathBuf::from(&paths.prompt_profiles_dir),
    ];

    if include_git_tool_data {
        source_dirs.push(Path::new(&paths.workspace_root).join("git-tool"));
    }

    for source_path in source_dirs {
        let source_entries = collect_directory_entries(&source_path).await?;
        let root_name = path_text(
            source_path
                .strip_prefix(&paths.workspace_root)
                .unwrap_or(&source_path),
        )
        .replace('\\', "/");

        entries.push(json!({
          "path": root_name,
          "type": "dir"
        }));
        for mut entry in source_entries {
            let child_path = string_value(entry.get("path"));
            entry["path"] = json!(format!("{}/{}", root_name, child_path));
            entries.push(entry);
        }
    }

    sanitize_runtime_backup_entries(entries)
}

async fn collect_file_entry(source_path: &str, relative_path: &str) -> Result<Option<Value>, ManagerError> {
    if !Path::new(source_path).exists() {
        return Ok(None);
    }

    Ok(Some(json!({
      "path": relative_path,
      "type": "file",
      "content": base64::engine::general_purpose::STANDARD.encode(tokio::fs::read(source_path).await?)
    })))
}

async fn collect_directory_entries(root_path: &Path) -> Result<Vec<Value>, ManagerError> {
    let mut entries = Vec::new();

    if !root_path.exists() {
        return Ok(entries);
    }

    collect_directory_entries_inner(root_path, root_path, &mut entries).await?;
    Ok(entries)
}

async fn collect_directory_entries_inner(
    root_path: &Path,
    current_path: &Path,
    entries: &mut Vec<Value>,
) -> Result<(), ManagerError> {
    let mut children = std::fs::read_dir(current_path)?.collect::<Result<Vec<_>, _>>()?;

    children.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });

    for child in children {
        let child_path = child.path();
        let relative_path = path_text(child_path.strip_prefix(root_path).unwrap_or(&child_path))
            .replace('\\', "/");
        let stat = std::fs::symlink_metadata(&child_path)?;

        if stat.file_type().is_symlink() {
            entries.push(json!({
              "path": relative_path,
              "type": "symlink",
              "target": path_text(std::fs::read_link(&child_path)?)
            }));
            continue;
        }

        if stat.is_dir() {
            entries.push(json!({
              "path": relative_path,
              "type": "dir"
            }));
            Box::pin(collect_directory_entries_inner(root_path, &child_path, entries)).await?;
            continue;
        }

        if stat.is_file() {
            entries.push(json!({
              "path": relative_path,
              "type": "file",
              "content": base64::engine::general_purpose::STANDARD.encode(tokio::fs::read(&child_path).await?)
            }));
        }
    }

    Ok(())
}

fn parse_backup(content: &str) -> Result<Value, ManagerError> {
    let mut backup = decrypt_backup_payload(content)?;

    if number_value(backup.get("version"), 0) != 1 {
        return Err(ManagerError::System("备份版本不支持".to_string()));
    }

    if !backup.get("workspaceEntries").and_then(Value::as_array).is_some() {
        return Err(ManagerError::System("备份数据不完整".to_string()));
    }

    backup["workspaceEntries"] = json!(sanitize_runtime_backup_entries(
        backup["workspaceEntries"].as_array().cloned().unwrap_or_default()
    )?);

    Ok(backup)
}

fn sanitize_runtime_backup_entries(entries: Vec<Value>) -> Result<Vec<Value>, ManagerError> {
    let mut next_entries = Vec::new();

    for entry in entries {
        if is_ignored_backup_path(&string_value(entry.get("path"))) {
            continue;
        }

        next_entries.push(serialize_prompt_runtime_backup_paths(serialize_skill_backup_paths(
            strip_provider_enabled(entry)?,
        )?)?);
    }

    Ok(next_entries)
}

fn strip_provider_enabled(entry: Value) -> Result<Value, ManagerError> {
    if entry.get("path").and_then(Value::as_str) != Some("storage/providers.json")
        || entry.get("type").and_then(Value::as_str) != Some("file")
    {
        return Ok(entry);
    }

    map_backup_json_entry(entry, |providers| {
        json!(providers
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut provider| {
                if let Some(provider) = provider.as_object_mut() {
                    provider.remove("enabled");
                }
                provider
            })
            .collect::<Vec<_>>())
    })
}

fn serialize_skill_backup_paths(entry: Value) -> Result<Value, ManagerError> {
    if entry.get("path").and_then(Value::as_str) != Some("storage/skills.json") {
        return Ok(entry);
    }

    map_backup_json_entry(entry, |skills| {
        json!(skills
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut skill| {
                if let Some(skill) = skill.as_object_mut() {
                    skill.remove("installedTargets");
                    skill.remove("installStates");
                    skill.remove("status");
                    let source_path = string_value(skill.get("sourcePath"));
                    let entry_path = string_value(skill.get("entryPath"));
                    skill.insert("sourcePath".to_string(), json!(serialize_portable_path(&source_path)));
                    skill.insert("entryPath".to_string(), json!(serialize_portable_path(&entry_path)));
                }
                skill
            })
            .collect::<Vec<_>>())
    })
}

fn serialize_prompt_runtime_backup_paths(entry: Value) -> Result<Value, ManagerError> {
    if entry.get("path").and_then(Value::as_str) != Some("storage/prompt-runtime-state.json") {
        return Ok(entry);
    }

    map_backup_json_entry(entry, |runtime_state| {
        let mut next_state = Map::new();

        for (cli, state) in runtime_state.as_object().cloned().unwrap_or_default() {
            let mut state = state;
            if let Some(state) = state.as_object_mut() {
                let runtime_path = string_value(state.get("runtimePath"));
                state.insert("runtimePath".to_string(), json!(serialize_portable_path(&runtime_path)));
            }
            next_state.insert(cli, state);
        }

        Value::Object(next_state)
    })
}

fn map_backup_json_entry(
    mut entry: Value,
    map_value: impl FnOnce(Value) -> Value,
) -> Result<Value, ManagerError> {
    let value = read_backup_entry_json(&entry)?;
    entry["content"] = json!(base64::engine::general_purpose::STANDARD.encode(format!(
        "{}\n",
        serde_json::to_string_pretty(&map_value(value))?
    )));
    Ok(entry)
}

fn read_backup_entry_text(entry: &Value) -> Result<String, ManagerError> {
    let content = base64::engine::general_purpose::STANDARD
        .decode(string_value(entry.get("content")))
        .map_err(|error| ManagerError::System(error.to_string()))?;

    String::from_utf8(content).map_err(|error| ManagerError::System(error.to_string()))
}

fn read_backup_entry_json(entry: &Value) -> Result<Value, ManagerError> {
    Ok(serde_json::from_str(&read_backup_entry_text(entry)?)?)
}

fn create_backup_view_entry(path_name: &str, type_name: &str, content: String) -> Value {
    json!({
      "path": path_name,
      "type": "file",
      "typeName": type_name,
      "size": content.as_bytes().len(),
      "content": content
    })
}

fn create_backup_entry_view(entry: &Value) -> Result<Value, ManagerError> {
    if entry.get("type").and_then(Value::as_str) == Some("dir") {
        return Ok(json!({
          "path": entry.get("path").cloned().unwrap_or(Value::Null),
          "type": entry.get("type").cloned().unwrap_or(Value::Null),
          "typeName": "目录",
          "size": 0,
          "content": ""
        }));
    }

    if entry.get("type").and_then(Value::as_str) == Some("symlink") {
        let target = string_value(entry.get("target"));

        return Ok(json!({
          "path": entry.get("path").cloned().unwrap_or(Value::Null),
          "type": entry.get("type").cloned().unwrap_or(Value::Null),
          "typeName": "链接",
          "size": target.as_bytes().len(),
          "content": target
        }));
    }

    let buffer = base64::engine::general_purpose::STANDARD
        .decode(string_value(entry.get("content")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let text = String::from_utf8(buffer.clone()).map_err(|error| ManagerError::System(error.to_string()))?;
    let entry_path = string_value(entry.get("path"));
    let content = if is_storage_json_path(&entry_path) {
        serde_json::to_string_pretty(&serde_json::from_str::<Value>(&text)?)?
    } else {
        text
    };

    Ok(json!({
      "path": entry.get("path").cloned().unwrap_or(Value::Null),
      "type": entry.get("type").cloned().unwrap_or(Value::Null),
      "typeName": restore_storage_name(&entry_path).unwrap_or("文件"),
      "size": buffer.len(),
      "content": content
    }))
}

async fn create_restore_preview(root_path: &str, entries: Vec<Value>) -> Result<Value, ManagerError> {
    let mut added = Vec::new();
    let mut conflicts = Vec::new();

    for entry in entries
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("file"))
    {
        let entry_path = string_value(entry.get("path"));
        let current_content = read_current_file(root_path, &entry_path).await?;

        if is_mergeable_restore_json_path(&entry_path) {
            let backup_value = read_backup_entry_json(entry)?;
            let current_value = if let Some(content) = current_content {
                serde_json::from_slice(&content)?
            } else if backup_value.is_array() {
                json!([])
            } else {
                json!({})
            };

            append_json_restore_preview(
                &entry_path,
                &current_value,
                &backup_value,
                &mut added,
                &mut conflicts,
            )?;
            continue;
        }

        if current_content.is_none() {
            added.push(create_restore_file_preview_item(&entry_path, "added", "", ""));
            continue;
        }

        let current_content = current_content.unwrap_or_default();
        let backup_content = base64::engine::general_purpose::STANDARD
            .decode(string_value(entry.get("content")))
            .map_err(|error| ManagerError::System(error.to_string()))?;

        if sha256_bytes(&current_content) != sha256_bytes(&backup_content) {
            conflicts.push(create_restore_file_preview_item(
                &entry_path,
                "conflict",
                &String::from_utf8(current_content).map_err(|error| ManagerError::System(error.to_string()))?,
                &String::from_utf8(backup_content).map_err(|error| ManagerError::System(error.to_string()))?,
            ));
        }
    }

    for entry in entries
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("symlink"))
    {
        let entry_path = string_value(entry.get("path"));
        let target_path = assert_backup_path(root_path, &entry_path)?;

        if !target_path.exists() {
            added.push(create_restore_file_preview_item(&entry_path, "added", "", ""));
            continue;
        }

        let stat = std::fs::symlink_metadata(&target_path)?;
        let current_target = if stat.file_type().is_symlink() {
            path_text(std::fs::read_link(&target_path)?)
        } else {
            String::new()
        };
        let backup_target = string_value(entry.get("target"));

        if current_target != backup_target {
            conflicts.push(create_restore_file_preview_item(
                &entry_path,
                "conflict",
                &current_target,
                &backup_target,
            ));
        }
    }

    Ok(json!({
      "added": added,
      "conflicts": conflicts,
      "addedCount": added.len(),
      "conflictCount": conflicts.len()
    }))
}

fn append_json_restore_preview(
    entry_path: &str,
    current_value: &Value,
    backup_value: &Value,
    added: &mut Vec<Value>,
    conflicts: &mut Vec<Value>,
) -> Result<(), ManagerError> {
    if let Some(backup_items) = backup_value.as_array() {
        let current_items = current_value.as_array().cloned().unwrap_or_default();
        let mut current_map = HashMap::new();

        for (index, item) in current_items.iter().enumerate() {
            current_map.insert(get_restore_item_key(entry_path, item, index), item.clone());
        }

        for (index, item) in backup_items.iter().enumerate() {
            let item_key = get_restore_item_key(entry_path, item, index);
            let Some(current_item) = current_map.get(&item_key) else {
                added.push(create_restore_preview_item(
                    entry_path,
                    &item_key,
                    item,
                    "added",
                    None,
                )?);
                continue;
            };

            if create_restore_content_hash(entry_path, current_item)?
                != create_restore_content_hash(entry_path, item)?
            {
                conflicts.push(create_restore_preview_item(
                    entry_path,
                    &item_key,
                    item,
                    "conflict",
                    Some(current_item),
                )?);
            }
        }
        return Ok(());
    }

    if let Some(backup_object) = backup_value.as_object() {
        let current_object = current_value.as_object().cloned().unwrap_or_default();

        for (item_key, value) in backup_object {
            let Some(current_item) = current_object.get(item_key) else {
                added.push(create_restore_preview_item(
                    entry_path,
                    item_key,
                    value,
                    "added",
                    None,
                )?);
                continue;
            };

            if create_restore_content_hash(entry_path, current_item)?
                != create_restore_content_hash(entry_path, value)?
            {
                conflicts.push(create_restore_preview_item(
                    entry_path,
                    item_key,
                    value,
                    "conflict",
                    Some(current_item),
                )?);
            }
        }
        return Ok(());
    }

    if create_restore_content_hash(entry_path, current_value)?
        != create_restore_content_hash(entry_path, backup_value)?
    {
        conflicts.push(create_restore_preview_item(
            entry_path,
            entry_path,
            backup_value,
            "conflict",
            Some(current_value),
        )?);
    }

    Ok(())
}

async fn restore_directory_entries(
    root_path: &str,
    entries: Vec<Value>,
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    tokio::fs::create_dir_all(root_path).await?;

    for entry in entries
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("dir"))
    {
        let target_path = assert_backup_path(root_path, &string_value(entry.get("path")))?;
        tokio::fs::create_dir_all(target_path).await?;
    }

    for entry in entries
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("file"))
    {
        let entry_path = string_value(entry.get("path"));
        let target_path = assert_backup_path(root_path, &entry_path)?;

        if is_mergeable_restore_json_path(&entry_path) {
            restore_json_entry(root_path, entry, choices).await?;
            continue;
        }

        let current_content = read_current_file(root_path, &entry_path).await?;
        let backup_content = base64::engine::general_purpose::STANDARD
            .decode(string_value(entry.get("content")))
            .map_err(|error| ManagerError::System(error.to_string()))?;

        if current_content
            .as_ref()
            .map(|value| sha256_bytes(value) != sha256_bytes(&backup_content))
            .unwrap_or(false)
            && choice_text(choices, &create_restore_file_key(&entry_path)) != "backup"
        {
            continue;
        }

        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(target_path, backup_content).await?;
    }

    for entry in entries
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("symlink"))
    {
        let entry_path = string_value(entry.get("path"));
        let target_path = assert_backup_path(root_path, &entry_path)?;

        if target_path.exists() {
            let stat = std::fs::symlink_metadata(&target_path)?;
            let current_target = if stat.file_type().is_symlink() {
                path_text(std::fs::read_link(&target_path)?)
            } else {
                String::new()
            };

            if current_target != string_value(entry.get("target"))
                && choice_text(choices, &create_restore_file_key(&entry_path)) != "backup"
            {
                continue;
            }

            remove_existing_path(&target_path).await?;
        }

        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        create_symlink(Path::new(&string_value(entry.get("target"))), &target_path)?;
    }

    Ok(())
}

async fn restore_json_entry(
    root_path: &str,
    entry: &Value,
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    let entry_path = string_value(entry.get("path"));
    let target_path = assert_backup_path(root_path, &entry_path)?;
    let current_content = read_current_file(root_path, &entry_path).await?;
    let backup_value = read_backup_entry_json(entry)?;
    let current_value = if let Some(content) = current_content {
        serde_json::from_slice(&content)?
    } else if backup_value.is_array() {
        json!([])
    } else {
        json!({})
    };
    let merged = merge_json_backup_value(&entry_path, &current_value, &backup_value, choices)?;

    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(
        target_path,
        format!("{}\n", serde_json::to_string_pretty(&merged)?),
    )
    .await?;
    Ok(())
}

fn merge_json_backup_value(
    entry_path: &str,
    current_value: &Value,
    backup_value: &Value,
    choices: &Map<String, Value>,
) -> Result<Value, ManagerError> {
    if let Some(backup_items) = backup_value.as_array() {
        let mut next_items = current_value.as_array().cloned().unwrap_or_default();
        let mut next_index_map = HashMap::new();

        for (index, item) in next_items.iter().enumerate() {
            next_index_map.insert(get_restore_item_key(entry_path, item, index), index);
        }

        for (index, item) in backup_items.iter().enumerate() {
            let item_key = get_restore_item_key(entry_path, item, index);

            if let Some(next_index) = next_index_map.get(&item_key).cloned() {
                if choice_text(choices, &create_restore_choice_key(entry_path, &item_key)) == "backup" {
                    next_items[next_index] =
                        merge_restore_value(entry_path, &next_items[next_index], item);
                }
            } else {
                next_index_map.insert(item_key, next_items.len());
                next_items.push(item.clone());
            }
        }

        return Ok(json!(next_items));
    }

    if let Some(backup_object) = backup_value.as_object() {
        let mut next_value = current_value.as_object().cloned().unwrap_or_default();

        for (item_key, value) in backup_object {
            if !next_value.contains_key(item_key)
                || choice_text(choices, &create_restore_choice_key(entry_path, item_key)) == "backup"
            {
                let current_item = next_value.get(item_key).cloned().unwrap_or(Value::Null);
                next_value.insert(item_key.clone(), merge_restore_value(entry_path, &current_item, value));
            }
        }

        return Ok(Value::Object(next_value));
    }

    if choice_text(choices, &create_restore_choice_key(entry_path, entry_path)) == "backup" {
        Ok(backup_value.clone())
    } else {
        Ok(current_value.clone())
    }
}

fn merge_restore_value(entry_path: &str, current_value: &Value, backup_value: &Value) -> Value {
    if entry_path == "storage/skills.json" {
        let mut next_backup_value = backup_value.as_object().cloned().unwrap_or_default();

        next_backup_value.remove("installedTargets");
        next_backup_value.remove("installStates");
        next_backup_value.remove("status");
        next_backup_value.insert(
            "installedTargets".to_string(),
            current_value
                .get("installedTargets")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        next_backup_value.insert(
            "installStates".to_string(),
            current_value
                .get("installStates")
                .cloned()
                .unwrap_or_else(|| json!({})),
        );
        next_backup_value.insert(
            "status".to_string(),
            current_value
                .get("status")
                .cloned()
                .unwrap_or_else(|| json!("not-installed")),
        );
        return Value::Object(next_backup_value);
    }

    if entry_path == "storage/prompt-runtime-state.json" {
        let mut next_backup_value = backup_value.as_object().cloned().unwrap_or_default();

        next_backup_value.insert(
            "lastSyncAt".to_string(),
            truthy_or_backup(current_value, backup_value, "lastSyncAt"),
        );
        next_backup_value.insert(
            "runtimePath".to_string(),
            truthy_or_backup(current_value, backup_value, "runtimePath"),
        );
        return Value::Object(next_backup_value);
    }

    if entry_path == "storage/codex-accounts.json" {
        let mut next_backup_value = backup_value.as_object().cloned().unwrap_or_default();

        next_backup_value.insert(
            "usage".to_string(),
            truthy_or_backup(current_value, backup_value, "usage"),
        );
        return Value::Object(next_backup_value);
    }

    if entry_path == "storage/runtime-provider-state.json" {
        let mut next_backup_value = backup_value.as_object().cloned().unwrap_or_default();

        next_backup_value.insert(
            "runtimeHash".to_string(),
            truthy_or_backup(current_value, backup_value, "runtimeHash"),
        );
        return Value::Object(next_backup_value);
    }

    backup_value.clone()
}

fn truthy_or_backup(current_value: &Value, backup_value: &Value, key: &str) -> Value {
    let current_item = current_value.get(key).cloned().unwrap_or(Value::Null);

    if is_js_truthy(&current_item) {
        current_item
    } else {
        backup_value.get(key).cloned().unwrap_or(Value::Null)
    }
}

fn is_js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().map(|value| value != 0.0).unwrap_or(true),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

async fn create_local_backup(
    app_data_path: &Path,
    paths: &AppPaths,
    app_settings: &mut AppSettings,
    cache: &mut DataBackupCache,
) -> Result<Value, ManagerError> {
    if !cache.begin_local_backup() {
        return Err(ManagerError::System(
            "本地自动备份正在进行，请稍后再试".to_string(),
        ));
    }

    let result = create_local_backup_inner(app_data_path, paths, app_settings).await;
    cache.finish_local_backup();
    result
}

async fn create_local_backup_inner(
    app_data_path: &Path,
    paths: &AppPaths,
    app_settings: &mut AppSettings,
) -> Result<Value, ManagerError> {
    let backup_dir = get_local_backup_directory_path(app_data_path);
    let created_at = now_millis();
    let file_name = format!(
        "monkey-thief-auto-{}.aimbackup",
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(created_at as i64)
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            .replace([':', '.'], "-")
    );
    let file_path = backup_dir.join(&file_name);

    tokio::fs::create_dir_all(&backup_dir).await?;
    tokio::fs::write(
        &file_path,
        create_data_backup(paths, app_settings, true).await?,
    )
    .await?;
    prune_local_backups(app_data_path, app_settings).await?;
    app_settings.local_backup.last_backup_at = created_at;
    write_json_file(
        Path::new(&app_settings.settings_file_path),
        &serialize_app_settings(app_settings),
    )
    .await?;

    local_backup_file_payload(&file_path).await
}

async fn get_local_backups_payload(app_data_path: &Path) -> Result<Value, ManagerError> {
    Ok(json!({
      "directory": get_local_backup_directory(app_data_path),
      "backups": list_local_backup_files(app_data_path).await?
    }))
}

async fn list_local_backup_files(app_data_path: &Path) -> Result<Vec<Value>, ManagerError> {
    let backup_dir = get_local_backup_directory_path(app_data_path);
    let mut backups = Vec::new();

    if !backup_dir.exists() {
        return Ok(backups);
    }

    for entry in std::fs::read_dir(&backup_dir)? {
        let entry = entry?;
        let file_path = entry.path();

        if !entry.file_type()?.is_file()
            || file_path.extension().and_then(|value| value.to_str()) != Some("aimbackup")
        {
            continue;
        }

        backups.push(local_backup_file_payload(&file_path).await?);
    }

    backups.sort_by(|left, right| {
        number_value(right.get("createdAt"), 0).cmp(&number_value(left.get("createdAt"), 0))
    });
    Ok(backups)
}

async fn prune_local_backups(
    app_data_path: &Path,
    app_settings: &AppSettings,
) -> Result<(), ManagerError> {
    let backups = list_local_backup_files(app_data_path).await?;

    for backup in backups
        .into_iter()
        .skip(app_settings.local_backup.max_count as usize)
    {
        let file_path = string_value(backup.get("filePath"));

        if !file_path.is_empty() {
            match tokio::fs::remove_file(file_path).await {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(ManagerError::Io(error)),
            }
        }
    }

    Ok(())
}

async fn local_backup_file_payload(file_path: &Path) -> Result<Value, ManagerError> {
    let stat = tokio::fs::metadata(file_path).await?;
    let created_at = stat
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let file_name = file_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(json!({
      "id": file_name,
      "fileName": file_name,
      "filePath": path_text(file_path),
      "createdAt": created_at,
      "size": stat.len()
    }))
}

fn get_local_backup_directory(app_data_path: &Path) -> String {
    path_text(get_local_backup_directory_path(app_data_path))
}

fn get_local_backup_directory_path(app_data_path: &Path) -> PathBuf {
    app_data_path.join("local-backups")
}

fn get_local_backup_path(app_data_path: &Path, backup_id: &str) -> Result<PathBuf, ManagerError> {
    let file_name = Path::new(backup_id)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();

    if !file_name.ends_with(".aimbackup") {
        return Err(ManagerError::System("备份文件名非法".to_string()));
    }

    Ok(get_local_backup_directory_path(app_data_path).join(file_name))
}

fn normalize_cloud_sync_settings(input: &Value) -> CloudSyncSettings {
    CloudSyncSettings {
        provider: "jianguoyun".to_string(),
        webdav_url: non_empty_string(
            input.get("webdavUrl"),
            "https://dav.jianguoyun.com/dav/AI-Manager",
        ),
        username: string_value(input.get("username")),
        password: input
            .get("password")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        file_name: non_empty_string(input.get("fileName"), "ai-manager.aimbackup"),
        last_updated_at: number_value(input.get("lastUpdatedAt"), 0),
    }
}

async fn ensure_webdav_directory(config: &CloudSyncSettings) -> Result<(), ManagerError> {
    let response = reqwest::Client::new()
        .request(
            reqwest::Method::from_bytes(b"MKCOL")
                .map_err(|error| ManagerError::System(error.to_string()))?,
            &config.webdav_url,
        )
        .header(AUTHORIZATION, build_webdav_auth_header(config))
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if ![201, 405].contains(&response.status().as_u16()) {
        return Err(ManagerError::System(format!(
            "坚果云目录创建失败：{}",
            response.status().as_u16()
        )));
    }

    Ok(())
}

async fn upload_webdav_backup(
    config: &CloudSyncSettings,
    content: String,
) -> Result<(), ManagerError> {
    ensure_webdav_directory(config).await?;

    let response = reqwest::Client::new()
        .put(build_webdav_file_url(config)?)
        .header(AUTHORIZATION, build_webdav_auth_header(config))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(content)
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if ![200, 201, 204].contains(&response.status().as_u16()) {
        return Err(ManagerError::System(format!(
            "坚果云上传失败：{}",
            response.status().as_u16()
        )));
    }

    Ok(())
}

async fn download_webdav_backup(config: &CloudSyncSettings) -> Result<String, ManagerError> {
    let response = reqwest::Client::new()
        .get(build_webdav_file_url(config)?)
        .header(AUTHORIZATION, build_webdav_auth_header(config))
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let status = response.status().as_u16();

    if status == 404 {
        return Err(ManagerError::System("坚果云上未找到配置备份".to_string()));
    }

    if status != 200 {
        return Err(ManagerError::System(format!("坚果云下载失败：{}", status)));
    }

    response
        .text()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))
}

fn build_webdav_file_url(config: &CloudSyncSettings) -> Result<String, ManagerError> {
    let root_url = if config.webdav_url.ends_with('/') {
        config.webdav_url.clone()
    } else {
        format!("{}/", config.webdav_url)
    };
    let mut url = url::Url::parse(&root_url)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| ManagerError::System("WebDAV 地址非法".to_string()))?;

    for segment in config.file_name.split('/').filter(|item| !item.is_empty()) {
        segments.push(segment);
    }
    drop(segments);

    Ok(url.to_string())
}

fn build_webdav_auth_header(config: &CloudSyncSettings) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", config.username, config.password))
    )
}

fn encrypt_backup_payload(payload: &Value) -> Result<String, ManagerError> {
    let mut iv = [0u8; 12];

    getrandom::getrandom(&mut iv).map_err(|error| ManagerError::System(error.to_string()))?;

    let secret = backup_secret();
    let cipher = Aes256Gcm::new_from_slice(&secret)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&iv), serde_json::to_string(payload)?.as_bytes())
        .map_err(|error| ManagerError::System(format!("{:?}", error)))?;
    let tag_index = encrypted.len() - 16;
    let engine = base64::engine::general_purpose::STANDARD;

    Ok(serde_json::to_string_pretty(&json!({
      "version": 1,
      "algorithm": "aes-256-gcm",
      "iv": engine.encode(iv),
      "tag": engine.encode(&encrypted[tag_index..]),
      "content": engine.encode(&encrypted[..tag_index])
    }))?)
}

fn decrypt_backup_payload(content: &str) -> Result<Value, ManagerError> {
    let payload: Value = serde_json::from_str(content)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let iv = engine
        .decode(string_value(payload.get("iv")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let tag = engine
        .decode(string_value(payload.get("tag")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let encrypted = engine
        .decode(string_value(payload.get("content")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let mut content = encrypted;

    content.extend(tag);

    let cipher = Aes256Gcm::new_from_slice(&backup_secret())
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let decrypted = cipher
        .decrypt(Nonce::from_slice(&iv), content.as_ref())
        .map_err(|error| ManagerError::System(format!("{:?}", error)))?;

    Ok(serde_json::from_slice(&decrypted)?)
}

fn encrypt_backup_data(value: &Value) -> Result<String, ManagerError> {
    let mut iv = [0u8; 12];

    getrandom::getrandom(&mut iv).map_err(|error| ManagerError::System(error.to_string()))?;

    let secret = backup_secret();
    let cipher = Aes256Gcm::new_from_slice(&secret)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let content = format!("AI_MANAGER::RUNTIME_KEYS::{}", serde_json::to_string(value)?);
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&iv), content.as_bytes())
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

fn decrypt_backup_data(value: &str) -> Result<Value, ManagerError> {
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

    let cipher = Aes256Gcm::new_from_slice(&backup_secret())
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let decrypted = cipher
        .decrypt(Nonce::from_slice(&iv), payload.as_ref())
        .map_err(|error| ManagerError::System(format!("{:?}", error)))?;
    let text = String::from_utf8(decrypted)
        .map_err(|error| ManagerError::System(error.to_string()))?
        .replacen("AI_MANAGER::RUNTIME_KEYS::", "", 1);

    Ok(serde_json::from_str(&text)?)
}

fn backup_secret() -> [u8; 32] {
    let digest = Sha256::digest(b"ai-manager-data-backup-v1");
    let mut secret = [0u8; 32];

    secret.copy_from_slice(&digest);
    secret
}

fn export_provider_keys(paths: &AppPaths) -> Result<Value, ManagerError> {
    let providers = read_array(&paths.storage_files.providers)?;
    let keys = read_object(&paths.storage_files.runtime_provider_keys)?;
    let mut exported = Map::new();

    for provider in providers {
        let provider_id = string_value(provider.get("id"));
        let Some(encrypted_key) = keys.get(&provider_id).and_then(Value::as_str) else {
            continue;
        };
        let api_key = runtime_provider::decrypt_provider_key(encrypted_key)?;

        if !api_key.is_empty() {
            exported.insert(provider_id, json!(api_key));
        }
    }

    Ok(Value::Object(exported))
}

async fn merge_provider_keys(
    paths: &AppPaths,
    api_keys: &Value,
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    let mut next_keys = read_object(&paths.storage_files.runtime_provider_keys)?;

    for (provider_id, api_key) in api_keys.as_object().cloned().unwrap_or_default() {
        let key = string_value(Some(&api_key));

        if key.is_empty() {
            continue;
        }

        if next_keys.contains_key(&provider_id)
            && choice_text(choices, &create_restore_choice_key("storage/providers.json", &provider_id))
                != "backup"
        {
            continue;
        }

        runtime_provider::set_provider_key(&mut next_keys, &provider_id, key)?;
    }

    write_json(&paths.storage_files.runtime_provider_keys, &Value::Object(next_keys)).await
}

async fn migrate_workspace_data(paths: &AppPaths) -> Result<(), ManagerError> {
    migrate_json_array_file(
        &Path::new(&paths.storage_dir).join("usage-logs.json"),
        Path::new(&paths.storage_files.usage_logs),
        |item| {
            string_value(item.get("requestId"))
                .or_else_non_empty(|| string_value(item.get("id")))
        },
    )
    .await?;
    migrate_json_array_file(
        &Path::new(&paths.storage_dir).join("usage-request-records.json"),
        Path::new(&paths.storage_files.usage_request_records),
        |item| string_value(item.get("requestId")),
    )
    .await?;
    migrate_json_array_file(
        &Path::new(&paths.storage_dir).join("sessions.json"),
        Path::new(&paths.storage_files.sessions),
        |item| string_value(item.get("id")),
    )
    .await?;
    migrate_skill_repository_storage(paths).await
}

async fn migrate_json_array_file(
    source_path: &Path,
    target_path: &Path,
    key_selector: impl Fn(&Value) -> String,
) -> Result<(), ManagerError> {
    if !source_path.exists() {
        return Ok(());
    }

    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    if !target_path.exists() {
        tokio::fs::rename(source_path, target_path).await?;
        return Ok(());
    }

    let source_items: Value = serde_json::from_str(&tokio::fs::read_to_string(source_path).await?)?;
    let target_items: Value = serde_json::from_str(&tokio::fs::read_to_string(target_path).await?)?;

    if !source_items.is_array() || !target_items.is_array() {
        match tokio::fs::remove_file(source_path).await {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(ManagerError::Io(error)),
        }
        return Ok(());
    }

    let mut next_items = Vec::new();
    let mut item_index_map = HashMap::new();

    for item in target_items
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .chain(source_items.as_array().cloned().unwrap_or_default())
    {
        let item_key = key_selector(&item);

        if let Some(index) = item_index_map.get(&item_key).cloned() {
            next_items[index] = item;
        } else {
            item_index_map.insert(item_key, next_items.len());
            next_items.push(item);
        }
    }

    write_json(&path_text(target_path), &json!(next_items)).await?;
    tokio::fs::remove_file(source_path).await?;
    Ok(())
}

async fn migrate_skill_repository_storage(paths: &AppPaths) -> Result<(), ManagerError> {
    let storage_path = Path::new(&paths.storage_files.skill_repositories);
    let cache_path = Path::new(&paths.storage_files.skill_repository_cache);

    if !storage_path.exists() {
        return Ok(());
    }

    let repositories: Value = serde_json::from_str(&tokio::fs::read_to_string(storage_path).await?)?;

    if !repositories.is_array() {
        return Ok(());
    }

    let repositories = repositories.as_array().cloned().unwrap_or_default();
    let has_runtime_fields = repositories.iter().any(|repository| {
        repository.get("skills").is_some()
            || repository.get("status").is_some()
            || repository.get("error").is_some()
            || repository.get("lastSyncedAt").is_some()
    });

    if !has_runtime_fields {
        return Ok(());
    }

    if let Some(parent) = cache_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    write_json(
        &paths.storage_files.skill_repositories,
        &json!(repositories
            .iter()
            .map(create_skill_repository_storage_item)
            .collect::<Vec<_>>()),
    )
    .await?;

    let cached_repositories = repositories
        .iter()
        .filter(|repository| has_skill_repository_cache(repository))
        .map(create_skill_repository_cache_item)
        .collect::<Vec<_>>();

    if !cached_repositories.is_empty() {
        write_json(
            &paths.storage_files.skill_repository_cache,
            &json!(cached_repositories),
        )
        .await?;
    }

    Ok(())
}

fn create_skill_repository_storage_item(repository: &Value) -> Value {
    json!({
      "id": repository.get("id").cloned().unwrap_or(Value::Null),
      "type": repository.get("type").cloned().unwrap_or(Value::Null),
      "name": repository.get("name").cloned().unwrap_or(Value::Null),
      "source": repository.get("source").cloned().unwrap_or(Value::Null),
      "owner": repository.get("owner").cloned().unwrap_or(Value::Null),
      "repository": repository.get("repository").cloned().unwrap_or(Value::Null),
      "branch": repository.get("branch").cloned().unwrap_or(Value::Null),
      "rootPath": repository.get("rootPath").cloned().unwrap_or(Value::Null),
      "htmlUrl": repository.get("htmlUrl").cloned().unwrap_or(Value::Null),
      "createdAt": repository.get("createdAt").cloned().unwrap_or(Value::Null),
      "updatedAt": repository.get("updatedAt").cloned().unwrap_or(Value::Null)
    })
}

fn create_skill_repository_cache_item(repository: &Value) -> Value {
    json!({
      "id": repository.get("id").cloned().unwrap_or(Value::Null),
      "status": non_empty_string(repository.get("status"), "ready"),
      "skills": repository.get("skills").filter(|value| value.is_array()).cloned().unwrap_or_else(|| json!([])),
      "error": string_value(repository.get("error")),
      "lastSyncedAt": number_value(repository.get("lastSyncedAt"), 0),
      "updatedAt": number_value(repository.get("updatedAt"), 0)
    })
}

fn has_skill_repository_cache(repository: &Value) -> bool {
    repository
        .get("skills")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
        && number_value(repository.get("lastSyncedAt"), 0) > 0
}

async fn read_current_file(root_path: &str, entry_path: &str) -> Result<Option<Vec<u8>>, ManagerError> {
    let target_path = assert_backup_path(root_path, entry_path)?;

    if !target_path.exists() {
        return Ok(None);
    }

    Ok(Some(tokio::fs::read(target_path).await?))
}

fn assert_backup_path(root_path: &str, entry_path: &str) -> Result<PathBuf, ManagerError> {
    let root = Path::new(root_path);
    let mut target_path = root.to_path_buf();

    for component in Path::new(entry_path).components() {
        match component {
            Component::Normal(part) => target_path.push(part),
            Component::CurDir => {}
            _ => return Err(ManagerError::System("备份路径非法".to_string())),
        }
    }

    Ok(target_path)
}

fn is_ignored_backup_path(entry_path: &str) -> bool {
    let ignored_runtime_backup_paths = HashSet::from([
        "storage/installs.json",
        "storage/runtime-profiles.json",
        "storage/runtime-provider-state.json",
        "storage/runtime-provider-keys.json",
        "storage/sessions.json",
        "storage/usage-logs.json",
        "storage/usage-request-records.json",
        "storage/codex-active-account-id.json",
        "storage/codex-provider-instances.json",
    ]);
    let normalized_path = entry_path.to_lowercase();
    let file_name = Path::new(&normalized_path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();

    ignored_runtime_backup_paths.contains(entry_path)
        || normalized_path == "logs"
        || normalized_path.starts_with("logs/")
        || normalized_path.contains("/logs/")
        || file_name.ends_with(".log")
        || file_name.ends_with(".logs")
        || file_name.ends_with("-logs.json")
        || file_name.ends_with("_logs.json")
        || file_name == "logs.json"
}

fn restore_storage_name(entry_path: &str) -> Option<&'static str> {
    match entry_path {
        "storage/skill-repositories.json" => Some("Skill 仓库"),
        "storage/skills.json" => Some("Skill 索引"),
        "storage/installs.json" => Some("Skill 挂载"),
        "storage/usage-logs.json" => Some("用量日志"),
        "storage/usage-pricing.json" => Some("模型费用"),
        "storage/codex-provider-instances.json" => Some("Codex 独立实例"),
        "storage/providers.json" => Some("Provider"),
        "storage/runtime-models.json" => Some("模型"),
        "storage/codex-accounts.json" => Some("Codex 官方账号"),
        "storage/codex-proxy-config.json" => Some("Codex 代理配置"),
        "storage/rules.json" => Some("Prompt 索引"),
        "storage/prompt-runtime-state.json" => Some("Prompt Runtime 状态"),
        _ => None,
    }
}

fn is_storage_json_path(entry_path: &str) -> bool {
    entry_path.starts_with("storage/") && entry_path.ends_with(".json")
}

fn is_mergeable_restore_json_path(entry_path: &str) -> bool {
    HashSet::from([
        "storage/skill-repositories.json",
        "storage/skills.json",
        "storage/installs.json",
        "storage/providers.json",
        "storage/runtime-models.json",
        "storage/runtime-profiles.json",
        "storage/runtime-provider-state.json",
        "storage/runtime-provider-keys.json",
        "storage/codex-accounts.json",
        "storage/codex-active-account-id.json",
        "storage/rules.json",
        "storage/prompt-runtime-state.json",
    ])
    .contains(entry_path)
}

fn normalize_restore_value(entry_path: &str, value: &Value) -> Result<String, ManagerError> {
    let mut value = value.clone();

    if [
        "storage/providers.json",
        "storage/skills.json",
        "storage/runtime-models.json",
        "storage/runtime-profiles.json",
        "storage/runtime-provider-state.json",
        "storage/codex-accounts.json",
        "storage/rules.json",
        "storage/prompt-runtime-state.json",
    ]
    .contains(&entry_path)
        && value.is_object()
    {
        value = remove_runtime_time_fields(value);
    }

    if entry_path == "storage/providers.json" && value.is_object() {
        if let Some(value) = value.as_object_mut() {
            value.remove("enabled");
        }
        return Ok(serde_json::to_string_pretty(&value)?);
    }

    if entry_path == "storage/skills.json" {
        return Ok(serde_json::to_string_pretty(&normalize_skill_restore_value(value))?);
    }

    if entry_path == "storage/prompt-runtime-state.json" {
        return Ok(serde_json::to_string_pretty(
            &normalize_prompt_runtime_restore_value(value),
        )?);
    }

    if entry_path == "storage/codex-accounts.json" {
        return Ok(serde_json::to_string_pretty(
            &normalize_codex_account_restore_value(value),
        )?);
    }

    if entry_path == "storage/runtime-provider-state.json" {
        return Ok(serde_json::to_string_pretty(
            &normalize_runtime_provider_state_restore_value(value),
        )?);
    }

    Ok(serde_json::to_string_pretty(&value)?)
}

fn remove_runtime_time_fields(value: Value) -> Value {
    match value {
        Value::Array(items) => json!(items
            .into_iter()
            .map(remove_runtime_time_fields)
            .collect::<Vec<_>>()),
        Value::Object(map) => {
            let mut next = Map::new();

            for (key, value) in map {
                if [
                    "createdAt",
                    "updatedAt",
                    "lastUpdatedAt",
                    "lastSyncAt",
                    "uploadedAt",
                    "downloadedAt",
                    "lastBackupAt",
                    "created_at",
                    "updated_at",
                    "last_refresh",
                    "token_updated_at",
                ]
                .contains(&key.as_str())
                {
                    continue;
                }
                next.insert(key, remove_runtime_time_fields(value));
            }

            Value::Object(next)
        }
        value => value,
    }
}

fn normalize_skill_restore_value(value: Value) -> Value {
    if let Value::Object(mut map) = value {
        map.remove("installedTargets");
        map.remove("installStates");
        map.remove("status");
        return Value::Object(map);
    }

    value
}

fn normalize_prompt_runtime_restore_value(value: Value) -> Value {
    if let Value::Object(mut map) = value {
        map.remove("lastSyncAt");
        map.remove("runtimePath");
        return Value::Object(map);
    }

    value
}

fn normalize_codex_account_restore_value(value: Value) -> Value {
    if let Value::Object(mut map) = value {
        map.remove("usage");
        return Value::Object(map);
    }

    value
}

fn normalize_runtime_provider_state_restore_value(value: Value) -> Value {
    if let Value::Object(mut map) = value {
        map.remove("runtimeHash");
        return Value::Object(map);
    }

    value
}

fn create_restore_content_hash(entry_path: &str, value: &Value) -> Result<String, ManagerError> {
    Ok(sha256_text(&normalize_restore_value(entry_path, value)?))
}

fn get_restore_item_key(_entry_path: &str, item: &Value, index: usize) -> String {
    if let Some(item) = item.as_object() {
        if let Some(id) = item.get("id").and_then(Value::as_str) {
            return id.to_string();
        }

        if item.get("providerId").is_some() && item.get("name").is_some() {
            return format!(
                "{}:{}",
                string_value(item.get("providerId")),
                string_value(item.get("name"))
            );
        }

        return [
            string_value(item.get("name")),
            string_value(item.get("accountId")),
            string_value(item.get("account_id")),
            index.to_string(),
        ]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| index.to_string());
    }

    index.to_string()
}

fn get_restore_item_name(entry_path: &str, item_key: &str, value: &Value) -> String {
    if !value.is_object() {
        return item_key.to_string();
    }

    if entry_path == "storage/codex-accounts.json" {
        return [
            string_value(value.get("email")),
            string_value(value.get("accountId")),
            string_value(value.get("account_id")),
            item_key.to_string(),
        ]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| item_key.to_string());
    }

    [
        string_value(value.get("name")),
        string_value(value.get("id")),
        item_key.to_string(),
    ]
    .into_iter()
    .find(|value| !value.is_empty())
    .unwrap_or_else(|| item_key.to_string())
}

fn get_restore_group_path(entry_path: &str) -> String {
    let normalized_path = entry_path.replace('\\', "/");
    let parts = normalized_path
        .split('/')
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();

    if parts.first() == Some(&"skills") && parts.get(1).is_some() {
        return format!("skills/{}", parts[1]);
    }

    if parts.first() == Some(&"prompts") && parts.get(1).is_some() {
        return "prompts".to_string();
    }

    if parts.first() == Some(&"profiles") && parts.get(1).is_some() {
        return "profiles".to_string();
    }

    if parts.len() > 1 {
        parts[..parts.len() - 1].join("/")
    } else {
        "根目录".to_string()
    }
}

fn create_restore_preview_item(
    entry_path: &str,
    item_key: &str,
    value: &Value,
    status: &str,
    current_value: Option<&Value>,
) -> Result<Value, ManagerError> {
    Ok(json!({
      "key": create_restore_choice_key(entry_path, item_key),
      "type": restore_storage_name(entry_path).unwrap_or("配置项"),
      "name": get_restore_item_name(entry_path, item_key, value),
      "path": entry_path,
      "groupPath": get_restore_group_path(entry_path),
      "status": status,
      "currentContent": if status == "conflict" {
          normalize_restore_value(entry_path, current_value.unwrap_or(&Value::Null))?
      } else {
          String::new()
      },
      "backupContent": if status == "conflict" {
          normalize_restore_value(entry_path, value)?
      } else {
          String::new()
      }
    }))
}

fn create_restore_file_preview_item(
    entry_path: &str,
    status: &str,
    current_content: &str,
    backup_content: &str,
) -> Value {
    json!({
      "key": create_restore_file_key(entry_path),
      "type": if entry_path.starts_with("skills/") {
          "Skill 文件"
      } else if entry_path.starts_with("prompts/") {
          "Prompt 文件"
      } else if entry_path.starts_with("profiles/") {
          "Prompt 配置"
      } else {
          "文件"
      },
      "name": Path::new(entry_path).file_name().map(|value| value.to_string_lossy().to_string()).unwrap_or_default(),
      "path": entry_path,
      "groupPath": get_restore_group_path(entry_path),
      "status": status,
      "currentContent": current_content,
      "backupContent": backup_content
    })
}

fn create_restore_choice_key(entry_path: &str, item_key: &str) -> String {
    format!("json:{}:{}", entry_path, item_key)
}

fn create_restore_file_key(entry_path: &str) -> String {
    format!("file:{}", entry_path)
}

fn choice_text(choices: &Map<String, Value>, key: &str) -> String {
    string_value(choices.get(key))
}

fn sha256_bytes(content: &[u8]) -> String {
    let digest = Sha256::digest(content);

    format!("{:x}", digest)
}

fn sha256_text(content: &str) -> String {
    sha256_bytes(content.as_bytes())
}

fn create_restore_id() -> Result<String, ManagerError> {
    let mut bytes = [0u8; 16];

    getrandom::getrandom(&mut bytes).map_err(|error| ManagerError::System(error.to_string()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u64::from_be_bytes([
            0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        ])
    ))
}

async fn remove_existing_path(target_path: &Path) -> Result<(), ManagerError> {
    let stat = tokio::fs::symlink_metadata(target_path).await?;

    if stat.is_dir() && !stat.file_type().is_symlink() {
        tokio::fs::remove_dir_all(target_path).await?;
    } else {
        tokio::fs::remove_file(target_path).await?;
    }

    Ok(())
}

fn create_symlink(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(source_path, target_path)?;
    }

    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(source_path, target_path)?;
    }

    Ok(())
}

fn file_path_text(file_path: FilePath) -> Result<String, ManagerError> {
    file_path
        .simplified()
        .into_path()
        .map(path_text)
        .map_err(|error| ManagerError::Path(error.to_string()))
}

fn read_array(path: &str) -> Result<Vec<Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn read_object(path: &str) -> Result<Map<String, Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str::<Value>(&content)?
            .as_object()
            .cloned()
            .unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn write_json(path: &str, payload: &Value) -> Result<(), ManagerError> {
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

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

trait NonEmptyString {
    fn or_else_non_empty(self, value: impl FnOnce() -> String) -> String;
}

impl NonEmptyString for String {
    fn or_else_non_empty(self, value: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            value()
        } else {
            self
        }
    }
}

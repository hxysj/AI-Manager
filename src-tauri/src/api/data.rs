use crate::api::{runtime_provider, sessions, skills, tools};
use crate::core::error::ManagerError;
use crate::core::paths::{ensure_app_directories, home_path, path_text, AppPaths};
use crate::core::settings::{
    non_empty_string, normalize_cloud_sync_settings as normalize_provider_cloud_sync_settings,
    number_value, serialize_app_settings, string_value, write_json_file, AppSettings,
    CloudSyncSettings,
};
use crate::core::storage_state::create_initial_state;
use crate::core::{database, provider_store, rule_store, skill_store};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_ENCODING, ACCEPT_RANGES, AUTHORIZATION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, IF_MATCH, RANGE};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};

const MAX_COMPRESSED_BACKUP_PAYLOAD_SIZE: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BackupScope {
    Local,
    Cloud,
}

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

    fn cache_restore_backup(
        &mut self,
        content: String,
        source: Value,
    ) -> Result<String, ManagerError> {
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

pub async fn export_data_backup(
    app: &AppHandle,
    paths: &AppPaths,
    app_settings: &AppSettings,
) -> Result<Value, ManagerError> {
    let desktop_path = app.path().desktop_dir().unwrap_or_else(|_| home_path());
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

    tokio::fs::write(&file_path, create_data_backup(paths, app_settings).await?).await?;

    Ok(json!({
      "canceled": false,
      "filePath": file_path
    }))
}

pub async fn preview_data_backup_restore(
    app: &AppHandle,
    paths: &AppPaths,
    app_settings: &AppSettings,
    cache: &mut DataBackupCache,
) -> Result<Value, ManagerError> {
    let desktop_path = app.path().desktop_dir().unwrap_or_else(|_| home_path());
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
      "preview": preview_data_backup_restore_content(paths, app_settings, &content).await?
    }))
}

pub async fn restore_data_backup(
    paths: &AppPaths,
    app_settings: &mut AppSettings,
    state: &mut Value,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let restore_id = string_value(payload.get("restoreId"));
    let draft = cache.get_restore_backup_draft(&restore_id)?;
    let choices = payload.get("choices").cloned().unwrap_or_else(|| json!({}));

    let refresh_skills = restore_data_backup_content(
        paths,
        app_settings,
        &string_value(draft.get("content")),
        &choices,
    )
    .await?;
    cache.delete_restore_backup_draft(&restore_id);
    write_json_file(
        Path::new(&app_settings.settings_file_path),
        &serialize_app_settings(app_settings),
    )
    .await?;
    rebuild_state_after_restore(paths, app_settings, state, refresh_skills).await?;

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

    Ok(Some(
        create_local_backup_inner(app_data_path, paths, app_settings).await?,
    ))
}

pub async fn preview_local_backup_restore(
    app_data_path: &Path,
    paths: &AppPaths,
    app_settings: &AppSettings,
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
      "preview": preview_data_backup_restore_content(paths, app_settings, &content).await?
    }))
}

pub async fn restore_local_backup(
    app_data_path: &Path,
    paths: &AppPaths,
    app_settings: &mut AppSettings,
    state: &mut Value,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let restore_id = string_value(payload.get("restoreId"));
    let draft = cache.get_restore_backup_draft(&restore_id)?;
    let choices = payload.get("choices").cloned().unwrap_or_else(|| json!({}));

    let refresh_skills = restore_data_backup_content(
        paths,
        app_settings,
        &string_value(draft.get("content")),
        &choices,
    )
    .await?;
    cache.delete_restore_backup_draft(&restore_id);
    write_json_file(
        Path::new(&app_settings.settings_file_path),
        &serialize_app_settings(app_settings),
    )
    .await?;
    rebuild_state_after_restore(paths, app_settings, state, refresh_skills).await?;

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
    let updated_cloud_sync = CloudSyncSettings {
        last_updated_at,
        ..cloud_sync.clone()
    };
    upload_webdav_backup(
        &cloud_sync,
        create_scoped_data_backup(paths, app_settings, BackupScope::Cloud).await?,
    )
    .await?;
    set_cloud_sync_settings(app_settings, updated_cloud_sync);
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
    app_settings: &AppSettings,
    cache: &mut DataBackupCache,
    payload: Value,
) -> Result<Value, ManagerError> {
    let cloud_sync = normalize_cloud_sync_settings(&payload);
    let content = download_webdav_backup(&cloud_sync).await?;
    let mut preview_settings = app_settings.clone();
    set_cloud_sync_settings(&mut preview_settings, cloud_sync.clone());
    let preview = preview_data_backup_restore_content(paths, &preview_settings, &content).await?;
    let restore_id = cache.cache_restore_backup(
        content,
        json!({
          "type": "cloud",
          "cloudSync": cloud_sync
        }),
    )?;

    Ok(json!({
      "restoreId": restore_id,
      "fileName": cloud_sync.file_name,
      "preview": preview
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

    set_cloud_sync_settings(app_settings, cloud_sync.clone());
    let refresh_skills = restore_data_backup_content(
        paths,
        app_settings,
        &string_value(draft.get("content")),
        &choices,
    )
    .await?;
    cache.delete_restore_backup_draft(&restore_id);
    cloud_sync_settings_mut(app_settings, &cloud_sync.provider).last_updated_at = last_updated_at;
    write_json_file(
        Path::new(&app_settings.settings_file_path),
        &serialize_app_settings(app_settings),
    )
    .await?;
    rebuild_state_after_restore(paths, app_settings, state, refresh_skills).await?;

    Ok(json!({
      "downloadedAt": last_updated_at,
      "fileName": cloud_sync.file_name,
      "state": state
    }))
}

pub async fn create_data_backup(
    paths: &AppPaths,
    app_settings: &AppSettings,
) -> Result<String, ManagerError> {
    create_scoped_data_backup(paths, app_settings, BackupScope::Local).await
}

async fn create_scoped_data_backup(
    paths: &AppPaths,
    app_settings: &AppSettings,
    scope: BackupScope,
) -> Result<String, ManagerError> {
    let provider_keys = export_provider_keys(paths)?;
    let mut payload = json!({
      "version": 1,
      "createdAt": now_millis(),
      "workspaceEntries": collect_backup_entries(paths, scope).await?,
      "codexPetEntries": collect_codex_pet_entries(app_settings, scope).await?,
      "runtimeProviderKeys": encrypt_backup_data(&provider_keys)?
    });

    if scope == BackupScope::Local {
        payload["appSettings"] = serialize_backup_app_settings(app_settings);
    }
    encrypt_backup_payload(&payload, scope)
}

fn serialize_backup_app_settings(app_settings: &AppSettings) -> Value {
    json!({
      "cloudSync": serialize_backup_cloud_sync_settings(&app_settings.cloud_sync),
      "koofrSync": serialize_backup_cloud_sync_settings(&app_settings.koofr_sync)
    })
}

fn serialize_backup_cloud_sync_settings(cloud_sync: &CloudSyncSettings) -> Value {
    json!({
      "provider": cloud_sync.provider,
      "webdavUrl": cloud_sync.webdav_url,
      "username": cloud_sync.username,
      "password": cloud_sync.password,
      "fileName": cloud_sync.file_name
    })
}

fn normalize_backup_app_settings(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    let mut settings = Map::new();

    if let Some(cloud_sync) = value.get("cloudSync").filter(|value| value.is_object()) {
        settings.insert(
            "cloudSync".to_string(),
            serialize_backup_cloud_sync_settings(&normalize_provider_cloud_sync_settings(
                cloud_sync,
                "jianguoyun",
            )),
        );
    }
    if let Some(koofr_sync) = value.get("koofrSync").filter(|value| value.is_object()) {
        settings.insert(
            "koofrSync".to_string(),
            serialize_backup_cloud_sync_settings(&normalize_provider_cloud_sync_settings(
                koofr_sync,
                "koofr",
            )),
        );
    }

    (!settings.is_empty()).then(|| Value::Object(settings))
}

fn restore_backup_app_settings(
    app_settings: &mut AppSettings,
    backup: &Value,
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    let Some(backup_settings) = normalize_backup_app_settings(backup.get("appSettings")) else {
        return Ok(());
    };
    let current_settings = serialize_backup_app_settings(app_settings);
    let merged = merge_json_backup_value(
        "app-settings.json",
        &current_settings,
        &backup_settings,
        choices,
    )?;
    let cloud_sync_last_updated_at = app_settings.cloud_sync.last_updated_at;
    let koofr_sync_last_updated_at = app_settings.koofr_sync.last_updated_at;
    app_settings.cloud_sync =
        normalize_provider_cloud_sync_settings(&merged["cloudSync"], "jianguoyun");
    app_settings.cloud_sync.last_updated_at = cloud_sync_last_updated_at;
    app_settings.koofr_sync =
        normalize_provider_cloud_sync_settings(&merged["koofrSync"], "koofr");
    app_settings.koofr_sync.last_updated_at = koofr_sync_last_updated_at;
    Ok(())
}

fn redact_backup_app_settings(mut value: Value) -> Value {
    if let Some(value) = value.as_object_mut() {
        for key in ["cloudSync", "koofrSync"] {
            if let Some(password) = value
                .get_mut(key)
                .and_then(Value::as_object_mut)
                .and_then(|cloud_sync| cloud_sync.get_mut("password"))
            {
                *password = json!("********");
            }
        }
        if let Some(password) = value.get_mut("password") {
            *password = json!("********");
        }
    }
    value
}

async fn preview_data_backup_restore_content(
    paths: &AppPaths,
    app_settings: &AppSettings,
    content: &str,
) -> Result<Value, ManagerError> {
    let backup = parse_backup(content)?;
    let (workspace_entries, codex_pet_entries) = prepare_pet_restore_entries(
        paths,
        app_settings,
        backup["workspaceEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        backup["codexPetEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    )
    .await?;
    let mut preview = create_restore_preview(paths, workspace_entries).await?;
    if let Some(keys) = backup.get("runtimeProviderKeys").and_then(Value::as_str) {
        append_provider_keys_preview(paths, &decrypt_backup_data(keys)?, &mut preview)?;
    }
    append_app_settings_restore_preview(&mut preview, app_settings, &backup)?;
    append_codex_pets_restore_preview(&mut preview, app_settings, &codex_pet_entries).await?;

    preview["createdAt"] = json!(number_value(backup.get("createdAt"), 0));
    Ok(preview)
}

fn append_app_settings_restore_preview(
    preview: &mut Value,
    app_settings: &AppSettings,
    backup: &Value,
) -> Result<(), ManagerError> {
    let Some(backup_settings) = normalize_backup_app_settings(backup.get("appSettings")) else {
        return Ok(());
    };
    let mut added = preview["added"].as_array().cloned().unwrap_or_default();
    let mut conflicts = preview["conflicts"].as_array().cloned().unwrap_or_default();

    append_json_restore_preview(
        "app-settings.json",
        &serialize_backup_app_settings(app_settings),
        &backup_settings,
        &mut added,
        &mut conflicts,
    )?;
    preview["addedCount"] = json!(added.len());
    preview["conflictCount"] = json!(conflicts.len());
    preview["added"] = json!(added);
    preview["conflicts"] = json!(conflicts);
    Ok(())
}

async fn append_codex_pets_restore_preview(
    preview: &mut Value,
    app_settings: &AppSettings,
    entries: &[Value],
) -> Result<(), ManagerError> {
    let Some(codex_pets_dir) = codex_pets_backup_dir(app_settings) else {
        return Ok(());
    };
    let root_path = path_text(&codex_pets_dir);
    let mut added = preview["added"].as_array().cloned().unwrap_or_default();
    let mut conflicts = preview["conflicts"].as_array().cloned().unwrap_or_default();

    for entry in entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
    {
        let entry_path = string_value(entry.get("path"));
        let preview_path = codex_pet_backup_entry_path(&entry_path);
        let current_content = read_current_file(&root_path, &entry_path).await?;

        if current_content.is_none() {
            added.push(create_restore_file_preview_item(
                &preview_path,
                "added",
                "",
                "",
            ));
            continue;
        }

        let backup_content = base64::engine::general_purpose::STANDARD
            .decode(string_value(entry.get("content")))
            .map_err(|error| ManagerError::System(error.to_string()))?;
        let current_content = current_content.unwrap_or_default();

        if sha256_bytes(&current_content) != sha256_bytes(&backup_content) {
            conflicts.push(create_restore_file_preview_item(
                &preview_path,
                "conflict",
                &format_restore_file_content(&current_content),
                &format_restore_file_content(&backup_content),
            ));
        }
    }

    preview["addedCount"] = json!(added.len());
    preview["conflictCount"] = json!(conflicts.len());
    preview["added"] = json!(added);
    preview["conflicts"] = json!(conflicts);
    Ok(())
}

fn inspect_data_backup(content: &str) -> Result<Value, ManagerError> {
    let backup = parse_backup(content)?;
    let runtime_provider_keys =
        if let Some(keys) = backup.get("runtimeProviderKeys").and_then(Value::as_str) {
            decrypt_backup_data(keys)?
        } else {
            json!({})
        };
    let mut entries = Vec::new();

    if let Some(app_settings) = normalize_backup_app_settings(backup.get("appSettings")) {
        entries.push(create_backup_view_entry(
            "app-settings.json",
            "云同步设置",
            format!(
                "{}\n",
                serde_json::to_string_pretty(&redact_backup_app_settings(app_settings))?
            ),
        ));
    }

    for entry in backup["workspaceEntries"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        entries.push(create_backup_entry_view(&entry)?);
    }

    for mut entry in backup["codexPetEntries"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        entry["path"] = json!(codex_pet_backup_entry_path(&string_value(
            entry.get("path")
        )));
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
    app_settings: &mut AppSettings,
    content: &str,
    choices: &Value,
) -> Result<bool, ManagerError> {
    let backup = parse_backup(content)?;
    let choices = choices.as_object().cloned().unwrap_or_default();
    let runtime_provider_keys = backup
        .get("runtimeProviderKeys")
        .and_then(Value::as_str)
        .map(decrypt_backup_data)
        .transpose()?;

    ensure_app_directories(paths).await?;
    let (workspace_entries, codex_pet_entries) = prepare_pet_restore_entries(
        paths,
        app_settings,
        backup["workspaceEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        backup["codexPetEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    )
    .await?;
    let restored_paths = workspace_entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    let providers_changed = restored_paths.contains("storage/providers.json");
    let provider_models_changed = restored_paths.contains("storage/runtime-models.json");
    let codex_accounts_changed = restored_paths.contains("storage/codex-accounts.json");
    let rule_prompts_changed = restored_paths.contains("storage/rules.json");
    let refresh_skills = restored_paths.contains("storage/skills.json")
        || restored_paths
            .iter()
            .any(|path| path.starts_with("skills/"))
        || restored_paths.contains("storage/ai-manager.db");
    let current_restore_json_values =
        read_current_restore_json_values(paths, &workspace_entries).await?;
    restore_directory_entries(
        paths,
        workspace_entries
            .iter()
            .filter(|entry| !is_database_backup_path(&string_value(entry.get("path"))))
            .cloned()
            .collect(),
        &choices,
        &current_restore_json_values,
    )
    .await?;
    restore_codex_pet_entries(app_settings, codex_pet_entries, &choices).await?;
    migrate_skill_repository_storage(paths).await?;
    skill_store::initialize(paths)?;
    rule_store::initialize(paths)?;
    provider_store::initialize(paths)?;
    restore_database_entries(paths, &workspace_entries, &choices).await?;
    database::reconcile_local_state(
        paths,
        providers_changed,
        provider_models_changed,
        codex_accounts_changed,
        rule_prompts_changed,
    )?;

    if let Some(runtime_provider_keys) = runtime_provider_keys {
        merge_provider_keys(paths, &runtime_provider_keys, &choices).await?;
    }
    restore_backup_app_settings(app_settings, &backup, &choices)?;

    Ok(refresh_skills)
}

async fn rebuild_state_after_restore(
    paths: &AppPaths,
    app_settings: &AppSettings,
    state: &mut Value,
    refresh_skills: bool,
) -> Result<(), ManagerError> {
    *state = create_initial_state(paths, app_settings)?;
    if refresh_skills {
        skills::refresh_skills_state_after_restore(paths, state).await?;
    }
    sessions::refresh_sessions_state(paths, state).await
}

async fn collect_backup_entries(
    paths: &AppPaths,
    scope: BackupScope,
) -> Result<Vec<Value>, ManagerError> {
    skill_store::initialize(paths)?;
    rule_store::initialize(paths)?;
    provider_store::initialize(paths)?;
    let mut entries = Vec::new();

    let database = database::backup(paths)?;
    entries.push(json!({
      "path": "storage/ai-manager.db",
      "type": "file",
      "content": base64::engine::general_purpose::STANDARD.encode(database)
    }));

    let source_dirs = [
        PathBuf::from(&paths.skills_dir),
        PathBuf::from(&paths.prompts_dir),
        PathBuf::from(&paths.disabled_pets_dir),
    ];

    for source_path in source_dirs {
        let root_name = path_text(
            source_path
                .strip_prefix(&paths.workspace_root)
                .unwrap_or(&source_path),
        )
        .replace('\\', "/");
        let source_entries = collect_directory_entries(&source_path, scope, &root_name).await?;

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

// 启用宠物由 Codex 直接读取，因此单独保存并恢复到当前机器的 Codex 配置目录。
async fn collect_codex_pet_entries(
    app_settings: &AppSettings,
    scope: BackupScope,
) -> Result<Vec<Value>, ManagerError> {
    let Some(codex_pets_dir) = codex_pets_backup_dir(app_settings) else {
        return Ok(Vec::new());
    };
    if !codex_pets_dir.exists() {
        return Ok(Vec::new());
    }
    let mut children = tokio::fs::read_dir(&codex_pets_dir).await?;
    let mut pet_dirs = Vec::new();

    while let Some(child) = children.next_entry().await? {
        let pet_dir = child.path();

        if scope == BackupScope::Cloud && child.file_type().await?.is_symlink() {
            continue;
        }

        if tools::is_codex_pet_directory(&pet_dir).await {
            pet_dirs.push(pet_dir);
        }
    }
    pet_dirs.sort();
    let mut entries = Vec::new();

    for pet_dir in pet_dirs {
        let pet_id = pet_dir
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();

        entries.push(json!({
          "path": pet_id,
          "type": "dir"
        }));
        for mut entry in collect_directory_entries(&pet_dir, scope, &format!("pets/{pet_id}")).await? {
            let child_path = string_value(entry.get("path"));
            entry["path"] = json!(format!("{}/{}", pet_id, child_path));
            entries.push(entry);
        }
    }

    Ok(sanitize_codex_pet_entries(entries))
}

fn codex_pets_backup_dir(app_settings: &AppSettings) -> Option<PathBuf> {
    let config_path = string_value(app_settings.cli_config_paths.get("codex"));

    if config_path.is_empty() {
        return None;
    }

    Some(Path::new(&config_path).join("pets"))
}

fn codex_pet_backup_entry_path(entry_path: &str) -> String {
    format!(
        "codex-pets/{}",
        entry_path.replace('\\', "/").trim_matches('/')
    )
}

// 宠物启用状态属于本机状态，同步时任一目录已存在相同 ID 都保留本机宠物。
async fn prepare_pet_restore_entries(
    paths: &AppPaths,
    app_settings: &AppSettings,
    workspace_entries: Vec<Value>,
    codex_pet_entries: Vec<Value>,
) -> Result<(Vec<Value>, Vec<Value>), ManagerError> {
    let mut existing_ids = HashSet::new();
    collect_pet_directory_ids(Path::new(&paths.disabled_pets_dir), &mut existing_ids).await?;
    if let Some(codex_pets_dir) = codex_pets_backup_dir(app_settings) {
        collect_pet_directory_ids(&codex_pets_dir, &mut existing_ids).await?;
    }

    let disabled_backup_ids = workspace_entries
        .iter()
        .filter_map(disabled_pet_entry_id)
        .filter(|id| !existing_ids.contains(&pet_id_key(id)))
        .map(pet_id_key)
        .collect::<HashSet<_>>();
    existing_ids.extend(disabled_backup_ids.iter().cloned());

    let workspace_entries = workspace_entries
        .into_iter()
        .filter(|entry| {
            disabled_pet_entry_id(entry)
                .map(|id| disabled_backup_ids.contains(&pet_id_key(id)))
                .unwrap_or(true)
        })
        .collect();

    let enabled_backup_ids = codex_pet_entries
        .iter()
        .filter_map(codex_pet_entry_id)
        .filter(|id| !existing_ids.contains(&pet_id_key(id)))
        .map(pet_id_key)
        .collect::<HashSet<_>>();
    let codex_pet_entries = codex_pet_entries
        .into_iter()
        .filter(|entry| {
            codex_pet_entry_id(entry)
                .map(|id| enabled_backup_ids.contains(&pet_id_key(id)))
                .unwrap_or(false)
        })
        .collect();

    Ok((workspace_entries, codex_pet_entries))
}

async fn collect_pet_directory_ids(
    pets_dir: &Path,
    pet_ids: &mut HashSet<String>,
) -> Result<(), ManagerError> {
    if !pets_dir.exists() {
        return Ok(());
    }

    let mut entries = tokio::fs::read_dir(pets_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        pet_ids.insert(pet_id_key(&entry.file_name().to_string_lossy()));
    }
    Ok(())
}

fn disabled_pet_entry_id(entry: &Value) -> Option<String> {
    let path = entry.get("path")?.as_str()?.replace('\\', "/");
    let mut components = path.trim_matches('/').split('/');

    if !components.next()?.eq_ignore_ascii_case("pets-disabled") {
        return None;
    }
    components
        .next()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn codex_pet_entry_id(entry: &Value) -> Option<String> {
    entry
        .get("path")?
        .as_str()?
        .replace('\\', "/")
        .trim_matches('/')
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn pet_id_key(id: impl AsRef<str>) -> String {
    id.as_ref().to_lowercase()
}

fn is_cloud_backup_entry(entry_path: &str, is_dir: bool) -> bool {
    let components = entry_path.split('/').collect::<Vec<_>>();

    match components.first().copied() {
        Some("pets" | "pets-disabled") => {
            return if is_dir {
                components.len() <= 2
            } else {
                components.len() == 3 && matches!(components[2], "pet.json" | "spritesheet.webp")
            };
        }
        Some("prompts") => return true,
        Some("skills") => {}
        _ => return false,
    }
    let skill_path = &components[components.len().min(2)..];

    if components.iter().skip(1).any(|component| {
        matches!(
            *component,
            ".git"
                | "node_modules"
                | "__pycache__"
                | ".cache"
                | ".pytest_cache"
                | ".mypy_cache"
                | ".ruff_cache"
                | ".venv"
        )
    }) {
        return false;
    }
    if matches!(skill_path.first(), Some(&"logs" | &"cache" | &"backups")) {
        return false;
    }
    if skill_path.first() == Some(&"data") {
        if matches!(
            skill_path.get(1),
            Some(
                &"work-history"
                    | &"logs"
                    | &"cache"
                    | &"backups"
                    | &"persona-backups"
                    | &"system-changelog"
                    | &"self-improvement"
            )
        ) {
            return false;
        }
        if skill_path.len() == 2
            && matches!(
                skill_path[1],
                "log.json"
                    | "log-summary.json"
                    | "system-log.json"
                    | "system-changelog-summary.json"
            )
        {
            return false;
        }
    }
    let file_name = components.last().copied().unwrap_or_default();

    is_dir
        || !(file_name.ends_with('~')
            || [".pyc", ".pyo", ".tmp", ".temp", ".swp", ".bak"]
                .iter()
                .any(|suffix| file_name.ends_with(suffix)))
}

async fn collect_directory_entries(
    root_path: &Path,
    scope: BackupScope,
    prefix: &str,
) -> Result<Vec<Value>, ManagerError> {
    let mut entries = Vec::new();

    if !root_path.exists() {
        return Ok(entries);
    }

    collect_directory_entries_inner(root_path, root_path, &mut entries, scope, prefix).await?;
    Ok(entries)
}

async fn collect_directory_entries_inner(
    root_path: &Path,
    current_path: &Path,
    entries: &mut Vec<Value>,
    scope: BackupScope,
    prefix: &str,
) -> Result<(), ManagerError> {
    let mut children = std::fs::read_dir(current_path)?.collect::<Result<Vec<_>, _>>()?;

    children.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });

    for child in children {
        let child_path = child.path();
        let relative_path =
            path_text(child_path.strip_prefix(root_path).unwrap_or(&child_path)).replace('\\', "/");
        let stat = std::fs::symlink_metadata(&child_path)?;

        if scope == BackupScope::Cloud
            && (stat.file_type().is_symlink()
                || !is_cloud_backup_entry(&format!("{prefix}/{relative_path}"), stat.is_dir()))
        {
            continue;
        }

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
            Box::pin(collect_directory_entries_inner(
                root_path,
                &child_path,
                entries,
                scope,
                prefix,
            ))
            .await?;
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

    if !backup
        .get("workspaceEntries")
        .and_then(Value::as_array)
        .is_some()
    {
        return Err(ManagerError::System("备份数据不完整".to_string()));
    }

    backup["workspaceEntries"] = json!(sanitize_runtime_backup_entries(
        backup["workspaceEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    )?);
    backup["codexPetEntries"] = json!(sanitize_codex_pet_entries(
        backup["codexPetEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    ));

    Ok(backup)
}

fn sanitize_runtime_backup_entries(entries: Vec<Value>) -> Result<Vec<Value>, ManagerError> {
    let mut next_entries = Vec::new();

    for entry in entries {
        if !is_allowed_backup_path(&string_value(entry.get("path"))) {
            continue;
        }

        next_entries.push(strip_codex_account_usage(strip_skill_local_state(
            strip_provider_enabled(entry)?,
        )?)?);
    }

    Ok(next_entries)
}

fn sanitize_codex_pet_entries(entries: Vec<Value>) -> Vec<Value> {
    entries
        .into_iter()
        .filter(|entry| is_allowed_codex_pet_backup_entry(entry))
        .collect()
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

fn strip_skill_local_state(entry: Value) -> Result<Value, ManagerError> {
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
                    for field in [
                        "disabled",
                        "installedTargets",
                        "installStates",
                        "status",
                        "sourcePath",
                        "entryPath",
                        "repoName",
                    ] {
                        skill.remove(field);
                    }
                }
                skill
            })
            .collect::<Vec<_>>())
    })
}

fn strip_codex_account_usage(entry: Value) -> Result<Value, ManagerError> {
    if entry.get("path").and_then(Value::as_str) != Some("storage/codex-accounts.json")
        || entry.get("type").and_then(Value::as_str) != Some("file")
    {
        return Ok(entry);
    }

    map_backup_json_entry(entry, |accounts| {
        json!(accounts
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut account| {
                if let Some(account) = account.as_object_mut() {
                    account.remove("usage");
                    account.remove("disabled");
                }
                account
            })
            .collect::<Vec<_>>())
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
    let entry_path = string_value(entry.get("path"));
    if is_database_backup_path(&entry_path) {
        let type_name = if entry_path == "storage/ai-manager.db" {
            "主数据库"
        } else {
            "旧版用量数据库"
        };

        return Ok(json!({
          "path": entry_path,
          "type": "file",
          "typeName": type_name,
          "size": buffer.len(),
          "content": format!("SQLite 数据库，SHA-256：{}", sha256_bytes(&buffer))
        }));
    }

    let content = if is_storage_json_path(&entry_path) {
        let text = String::from_utf8(buffer.clone())
            .map_err(|error| ManagerError::System(error.to_string()))?;
        serde_json::to_string_pretty(&serde_json::from_str::<Value>(&text)?)?
    } else {
        format_restore_file_content(&buffer)
    };

    Ok(json!({
      "path": entry.get("path").cloned().unwrap_or(Value::Null),
      "type": entry.get("type").cloned().unwrap_or(Value::Null),
      "typeName": restore_storage_name(&entry_path).unwrap_or("文件"),
      "size": buffer.len(),
      "content": content
    }))
}

async fn create_restore_preview(
    paths: &AppPaths,
    entries: Vec<Value>,
) -> Result<Value, ManagerError> {
    let root_path = &paths.workspace_root;
    let current_restore_json_values = read_current_restore_json_values(paths, &entries).await?;
    let mut added = Vec::new();
    let mut conflicts = Vec::new();

    for entry in entries
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("file"))
    {
        let entry_path = string_value(entry.get("path"));

        if entry_path == "storage/ai-manager.db" {
            let backup_content = base64::engine::general_purpose::STANDARD
                .decode(string_value(entry.get("content")))
                .map_err(|error| ManagerError::System(error.to_string()))?;

            for difference in database::preview_rows(paths, &backup_content)? {
                let item = create_database_row_restore_preview(&entry_path, &difference)?;
                if difference.current.is_some() {
                    conflicts.push(item);
                } else {
                    added.push(item);
                }
            }
            continue;
        }
        if is_mergeable_restore_json_path(&entry_path) {
            let backup_value = read_backup_entry_json(entry)?;
            let current_value = current_restore_json_values
                .get(&entry_path)
                .cloned()
                .unwrap_or_else(|| {
                    if backup_value.is_array() {
                        json!([])
                    } else {
                        json!({})
                    }
                });

            append_json_restore_preview(
                &entry_path,
                &current_value,
                &backup_value,
                &mut added,
                &mut conflicts,
            )?;
            continue;
        }
        let current_content = read_current_file(root_path, &entry_path).await?;

        if current_content.is_none() {
            added.push(create_restore_file_preview_item(
                &entry_path,
                "added",
                "",
                "",
            ));
            continue;
        }

        let current_content = current_content.unwrap_or_default();
        let backup_content = base64::engine::general_purpose::STANDARD
            .decode(string_value(entry.get("content")))
            .map_err(|error| ManagerError::System(error.to_string()))?;

        if sha256_bytes(&current_content) != sha256_bytes(&backup_content) {
            if is_database_backup_path(&entry_path) {
                conflicts.push(create_restore_file_preview_item(
                    &entry_path,
                    "conflict",
                    &format!("SQLite 数据库，SHA-256：{}", sha256_bytes(&current_content)),
                    &format!("SQLite 数据库，SHA-256：{}", sha256_bytes(&backup_content)),
                ));
                continue;
            }

            conflicts.push(create_restore_file_preview_item(
                &entry_path,
                "conflict",
                &format_restore_file_content(&current_content),
                &format_restore_file_content(&backup_content),
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
            added.push(create_restore_file_preview_item(
                &entry_path,
                "added",
                "",
                "",
            ));
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
                    entry_path, &item_key, item, "added", None,
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
                    entry_path, item_key, value, "added", None,
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
    paths: &AppPaths,
    entries: Vec<Value>,
    choices: &Map<String, Value>,
    current_restore_json_values: &HashMap<String, Value>,
) -> Result<(), ManagerError> {
    let root_path = &paths.workspace_root;
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
            restore_json_entry(
                paths,
                entry,
                choices,
                current_restore_json_values.get(&entry_path),
            )
            .await?;
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
        let backup_target = string_value(entry.get("target"));
        validate_backup_symlink_target(root_path, &target_path, &backup_target)?;

        if target_path.exists() {
            let stat = std::fs::symlink_metadata(&target_path)?;
            let current_target = if stat.file_type().is_symlink() {
                path_text(std::fs::read_link(&target_path)?)
            } else {
                String::new()
            };

            if current_target != backup_target
                && choice_text(choices, &create_restore_file_key(&entry_path)) != "backup"
            {
                continue;
            }

            remove_existing_path(&target_path).await?;
        }

        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        create_symlink(Path::new(&backup_target), &target_path)?;
    }

    Ok(())
}

async fn restore_codex_pet_entries(
    app_settings: &AppSettings,
    entries: Vec<Value>,
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    if entries.is_empty() {
        return Ok(());
    }
    let Some(codex_pets_dir) = codex_pets_backup_dir(app_settings) else {
        return Ok(());
    };
    let root_path = path_text(&codex_pets_dir);
    tokio::fs::create_dir_all(&codex_pets_dir).await?;

    for entry in entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("dir"))
    {
        let target_path = assert_backup_path(&root_path, &string_value(entry.get("path")))?;
        tokio::fs::create_dir_all(target_path).await?;
    }

    for entry in entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
    {
        let entry_path = string_value(entry.get("path"));
        let target_path = assert_backup_path(&root_path, &entry_path)?;
        let backup_content = base64::engine::general_purpose::STANDARD
            .decode(string_value(entry.get("content")))
            .map_err(|error| ManagerError::System(error.to_string()))?;
        let current_content = read_current_file(&root_path, &entry_path).await?;
        let preview_path = codex_pet_backup_entry_path(&entry_path);

        if current_content
            .as_ref()
            .map(|content| sha256_bytes(content) != sha256_bytes(&backup_content))
            .unwrap_or(false)
            && choice_text(choices, &create_restore_file_key(&preview_path)) != "backup"
        {
            continue;
        }

        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(target_path, backup_content).await?;
    }

    Ok(())
}

async fn restore_database_entries(
    paths: &AppPaths,
    entries: &[Value],
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    let entry_path = "storage/ai-manager.db";
    let Some(entry) = entries.iter().find(|entry| {
        entry.get("path").and_then(Value::as_str) == Some(entry_path)
            && entry.get("type").and_then(Value::as_str) == Some("file")
    }) else {
        return Ok(());
    };
    let backup_content = base64::engine::general_purpose::STANDARD
        .decode(string_value(entry.get("content")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    database::restore_merged(paths, &backup_content, choices)?;
    Ok(())
}

async fn restore_json_entry(
    paths: &AppPaths,
    entry: &Value,
    choices: &Map<String, Value>,
    current_value: Option<&Value>,
) -> Result<(), ManagerError> {
    let root_path = &paths.workspace_root;
    let entry_path = string_value(entry.get("path"));
    let target_path = assert_backup_path(root_path, &entry_path)?;
    let backup_value = read_backup_entry_json(entry)?;
    let current_value = current_value.cloned().unwrap_or_else(|| {
        if backup_value.is_array() {
            json!([])
        } else {
            json!({})
        }
    });
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

async fn read_current_restore_json_values(
    paths: &AppPaths,
    entries: &[Value],
) -> Result<HashMap<String, Value>, ManagerError> {
    let mut current_values = HashMap::new();

    for entry in entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
    {
        let entry_path = string_value(entry.get("path"));

        if !is_mergeable_restore_json_path(&entry_path) || current_values.contains_key(&entry_path)
        {
            continue;
        }
        let backup_value = read_backup_entry_json(entry)?;
        let current_content = read_current_file(&paths.workspace_root, &entry_path).await?;
        current_values.insert(
            entry_path.clone(),
            read_current_restore_json_value(paths, &entry_path, current_content, &backup_value)?,
        );
    }
    Ok(current_values)
}

fn read_current_restore_json_value(
    paths: &AppPaths,
    entry_path: &str,
    current_content: Option<Vec<u8>>,
    backup_value: &Value,
) -> Result<Value, ManagerError> {
    if let Some(content) = current_content {
        return Ok(serde_json::from_slice(&content)?);
    }

    let current_value = match entry_path {
        "storage/providers.json" => json!(provider_store::read_providers(paths)?),
        "storage/runtime-models.json" => json!(provider_store::read_models(paths)?),
        "storage/codex-accounts.json" => json!(provider_store::read_codex_accounts(paths)?),
        "storage/skills.json" => json!(skill_store::read_skills(paths)?),
        "storage/skill-groups.json" => json!(skill_store::read_groups(paths)?),
        "storage/skill-repositories.json" => json!(skill_store::read_repositories(paths)?),
        "storage/rules.json" => json!(rule_store::read_prompts(paths)?),
        _ if backup_value.is_array() => json!([]),
        _ => json!({}),
    };
    Ok(current_value)
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
                if choice_text(choices, &create_restore_choice_key(entry_path, &item_key))
                    == "backup"
                {
                    next_items[next_index] =
                        merge_restore_value(entry_path, &next_items[next_index], item);
                }
            } else {
                next_index_map.insert(item_key, next_items.len());
                next_items.push(
                    if [
                        "storage/providers.json",
                        "storage/skills.json",
                        "storage/codex-accounts.json",
                    ]
                    .contains(&entry_path)
                    {
                        merge_restore_value(entry_path, &Value::Null, item)
                    } else {
                        item.clone()
                    },
                );
            }
        }

        return Ok(json!(next_items));
    }

    if let Some(backup_object) = backup_value.as_object() {
        let mut next_value = current_value.as_object().cloned().unwrap_or_default();

        for (item_key, value) in backup_object {
            if !next_value.contains_key(item_key)
                || choice_text(choices, &create_restore_choice_key(entry_path, item_key))
                    == "backup"
            {
                let current_item = next_value.get(item_key).cloned().unwrap_or(Value::Null);
                next_value.insert(
                    item_key.clone(),
                    merge_restore_value(entry_path, &current_item, value),
                );
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
    if entry_path == "storage/providers.json" {
        let mut next_backup_value = backup_value.as_object().cloned().unwrap_or_default();
        let current_exists = current_value.is_object();

        next_backup_value.insert(
            "enabled".to_string(),
            json!(current_value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(current_exists)),
        );
        return Value::Object(next_backup_value);
    }

    if entry_path == "storage/skills.json" {
        let mut next_backup_value = backup_value.as_object().cloned().unwrap_or_default();
        let current_exists = current_value.is_object();

        next_backup_value.remove("disabled");
        next_backup_value.remove("installedTargets");
        next_backup_value.remove("installStates");
        next_backup_value.remove("status");
        next_backup_value.insert(
            "disabled".to_string(),
            json!(current_value
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(!current_exists)),
        );
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
            current_value.get("status").cloned().unwrap_or_else(|| {
                if current_exists {
                    json!("not-installed")
                } else {
                    json!("disabled")
                }
            }),
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
        let current_exists = current_value.is_object();

        if let Some(usage) = current_value.get("usage") {
            next_backup_value.insert("usage".to_string(), usage.clone());
        } else {
            next_backup_value.remove("usage");
        }
        next_backup_value.insert(
            "disabled".to_string(),
            json!(current_value
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(!current_exists)),
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
    tokio::fs::write(&file_path, create_data_backup(paths, app_settings).await?).await?;
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
    let provider = match input.get("provider").and_then(Value::as_str) {
        Some("koofr") => "koofr",
        _ => "jianguoyun",
    };

    normalize_provider_cloud_sync_settings(input, provider)
}

fn set_cloud_sync_settings(app_settings: &mut AppSettings, cloud_sync: CloudSyncSettings) {
    if cloud_sync.provider == "koofr" {
        app_settings.koofr_sync = cloud_sync;
    } else {
        app_settings.cloud_sync = cloud_sync;
    }
}

fn cloud_sync_settings_mut<'a>(
    app_settings: &'a mut AppSettings,
    provider: &str,
) -> &'a mut CloudSyncSettings {
    if provider == "koofr" {
        &mut app_settings.koofr_sync
    } else {
        &mut app_settings.cloud_sync
    }
}

fn cloud_sync_provider_name(config: &CloudSyncSettings) -> &'static str {
    if config.provider == "koofr" {
        "Koofr"
    } else {
        "坚果云"
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

    let status = response.status().as_u16();

    if ![201, 405].contains(&status) {
        let detail = read_webdav_error_detail(response, config).await?;

        return Err(ManagerError::System(format!(
            "{}目录创建失败：{}{}",
            cloud_sync_provider_name(config),
            status,
            detail
        )));
    }

    Ok(())
}

async fn upload_webdav_backup(
    config: &CloudSyncSettings,
    content: String,
) -> Result<(), ManagerError> {
    ensure_webdav_directory(config).await?;
    let expected_size = content.len() as u64;

    let response = reqwest::Client::new()
        .put(build_webdav_file_url(config)?)
        .header(AUTHORIZATION, build_webdav_auth_header(config))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(content)
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    let status = response.status().as_u16();

    if ![200, 201, 204].contains(&status) {
        let detail = read_webdav_error_detail(response, config).await?;

        return Err(ManagerError::System(format!(
            "{}上传失败：{}{}",
            cloud_sync_provider_name(config),
            status,
            detail
        )));
    }

    let response = reqwest::Client::new()
        .head(build_webdav_file_url(config)?)
        .header(AUTHORIZATION, build_webdav_auth_header(config))
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let status = response.status().as_u16();

    if status != 200 {
        return Err(ManagerError::System(format!(
            "{}上传校验失败：{}",
            cloud_sync_provider_name(config),
            status
        )));
    }

    let actual_size = webdav_content_length(response.headers()).ok_or_else(|| {
        ManagerError::System(format!(
            "{}上传校验失败：云端未返回文件大小",
            cloud_sync_provider_name(config)
        ))
    })?;

    if actual_size != expected_size {
        return Err(ManagerError::System(format!(
            "{}上传不完整：本地备份 {} 字节，云端文件 {} 字节，请重新上传",
            cloud_sync_provider_name(config),
            expected_size,
            actual_size
        )));
    }

    Ok(())
}

fn webdav_content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()
}

async fn download_webdav_backup(config: &CloudSyncSettings) -> Result<String, ManagerError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .map_err(|cause| ManagerError::System(cause.to_string()))?;
    let response = client
        .head(build_webdav_file_url(config)?)
        .header(AUTHORIZATION, build_webdav_auth_header(config))
        .header(ACCEPT_ENCODING, "identity")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|cause| ManagerError::System(format!("{}读取备份信息失败：{}", cloud_sync_provider_name(config), cause.without_url())))?;
    let status = response.status().as_u16();
    if status == 404 {
        return Err(ManagerError::System(format!(
            "{}上未找到配置备份",
            cloud_sync_provider_name(config)
        )));
    }

    if ![200, 405, 501].contains(&status) {
        let detail = read_webdav_error_detail(response, config).await?;
        return Err(ManagerError::System(format!(
            "{}读取备份信息失败：{}{}",
            cloud_sync_provider_name(config),
            status,
            detail
        )));
    }
    let size = webdav_content_length(response.headers());
    let etag = response.headers().get(ETAG).filter(|value| {
        value.to_str().is_ok_and(|value| value.starts_with('"') && value.ends_with('"'))
    }).cloned();
    let supports_ranges = status == 200 && response.headers().get(ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok()).is_some_and(|value| value.eq_ignore_ascii_case("bytes"));
    drop(response);
    let mut content = Vec::new();
    if let (true, Some(total), Some(etag)) = (supports_ranges, size.filter(|size| *size > 0), etag) {
        while (content.len() as u64) < total {
            let start = content.len() as u64;
            let range = WebdavDownloadRange {
                start,
                end: (start + 4 * 1024 * 1024 - 1).min(total - 1),
                total,
                etag: etag.clone(),
            };
            let bytes = download_webdav_part(&client, config, Some(&range)).await?;
            content.extend_from_slice(&bytes);
        }
    } else {
        content = download_webdav_part(&client, config, None).await?;
    }
    String::from_utf8(content).map_err(|_| ManagerError::System(format!(
        "{}备份不是有效的 UTF-8 文本，请检查云端文件是否为完整备份；本地数据未恢复", cloud_sync_provider_name(config)
    )))
}

struct WebdavDownloadRange {
    start: u64,
    end: u64,
    total: u64,
    etag: HeaderValue,
}

async fn download_webdav_part(
    client: &reqwest::Client,
    config: &CloudSyncSettings,
    range: Option<&WebdavDownloadRange>,
) -> Result<Vec<u8>, ManagerError> {
    let provider = cloud_sync_provider_name(config);
    let mut failure = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * attempt)).await;
        }
        let mut request = client.get(build_webdav_file_url(config)?)
            .header(AUTHORIZATION, build_webdav_auth_header(config))
            .header(ACCEPT_ENCODING, "identity")
            .timeout(Duration::from_secs(if range.is_some() { 120 } else { 600 }));
        if let Some(range) = range {
            request = request.header(RANGE, format!("bytes={}-{}", range.start, range.end))
                .header(IF_MATCH, range.etag.clone());
        }
        let mut response = match request.send().await {
            Ok(response) => response,
            Err(cause) => {
                failure = format!("连接失败或超时：{}", cause.without_url());
                continue;
            }
        };
        let status = response.status().as_u16();
        if status == 404 {
            return Err(ManagerError::System(format!("{provider}上未找到配置备份")));
        }
        if status == 412 {
            return Err(ManagerError::System(format!("{provider}备份在下载期间已更新，请重新预览；本地数据未恢复")));
        }
        if [408, 502, 503, 504].contains(&status) {
            failure = format!("云端暂时无法完成下载（HTTP {status}）");
            continue;
        }
        if status != if range.is_some() { 206 } else { 200 } {
            let detail = read_webdav_error_detail(response, config).await?;
            return Err(ManagerError::System(format!("{provider}下载失败：HTTP {status}{detail}")));
        }
        let expected_size = if let Some(range) = range {
            let expected_range = format!("bytes {}-{}/{}", range.start, range.end, range.total);
            if response.headers().get(CONTENT_RANGE).and_then(|value| value.to_str().ok()) != Some(expected_range.as_str()) {
                return Err(ManagerError::System(format!("{provider}返回的下载分段范围不匹配，已停止下载，避免拼接错误备份")));
            }
            if response.headers().get(ETAG).is_some_and(|etag| etag != range.etag) {
                return Err(ManagerError::System(format!("{provider}备份版本发生变化，请重新预览；本地数据未恢复")));
            }
            Some(range.end - range.start + 1)
        } else {
            webdav_content_length(response.headers())
        };
        let mut content = Vec::new();
        let mut interrupted = false;
        loop {
            match response.chunk().await {
                Ok(Some(bytes)) => {
                    if expected_size.is_some_and(|expected| content.len() as u64 + bytes.len() as u64 > expected) {
                        return Err(ManagerError::System(format!("{provider}下载数据超过声明大小，已停止下载；本地数据未恢复")));
                    }
                    content.extend_from_slice(&bytes);
                }
                Ok(None) => break,
                Err(cause) => {
                    failure = format!("响应读取中断或超时，本段已接收 {} 字节：{}", content.len(), cause.without_url());
                    interrupted = true;
                    break;
                }
            }
        }
        if interrupted {
            continue;
        }
        if expected_size.is_some_and(|expected| content.len() as u64 != expected) {
            failure = format!("下载不完整，本段预期 {} 字节，实际 {} 字节", expected_size.unwrap(), content.len());
            continue;
        }
        if content.is_empty() {
            return Err(ManagerError::System(format!("{provider}云端备份为空，请重新上传完整备份")));
        }
        return Ok(content);
    }
    let progress = range.map(|range| format!("，已完成 {} / {} 字节", range.start, range.total)).unwrap_or_default();
    Err(ManagerError::System(format!("{provider}备份下载失败{progress}，已尝试 3 次：{failure}。本地数据未恢复，请检查网络后重新预览")))
}

async fn read_webdav_error_detail(
    response: reqwest::Response,
    config: &CloudSyncSettings,
) -> Result<String, ManagerError> {
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|cause| ManagerError::System(format!(
            "{}返回 HTTP {status}，错误响应读取失败：{}", cloud_sync_provider_name(config), cause.without_url()
        )))?;
    let body = body.trim();

    if body.is_empty() {
        if status == 400 {
            return Ok(format!(
                "，请确认 WebDAV 地址是{}目录地址，备份文件名没有包含非法路径，并且云端备份文件已存在",
                cloud_sync_provider_name(config)
            ));
        }

        return Ok(String::new());
    }

    Ok(format!("，{}", body.chars().take(300).collect::<String>()))
}

fn build_webdav_file_url(config: &CloudSyncSettings) -> Result<String, ManagerError> {
    let root_url = if config.webdav_url.ends_with('/') {
        config.webdav_url.clone()
    } else {
        format!("{}/", config.webdav_url)
    };
    let mut url =
        url::Url::parse(&root_url).map_err(|error| ManagerError::System(error.to_string()))?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| ManagerError::System("WebDAV 地址非法".to_string()))?;
    segments.pop_if_empty();

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

fn encrypt_backup_payload(payload: &Value, scope: BackupScope) -> Result<String, ManagerError> {
    let mut iv = [0u8; 12];
    let mut content = serde_json::to_vec(payload)?;
    let uncompressed_size = content.len() as u64;

    if scope == BackupScope::Cloud {
        if uncompressed_size > MAX_COMPRESSED_BACKUP_PAYLOAD_SIZE {
            return Err(ManagerError::System(
                "云备份原始数据超过 1 GiB，请缩减 Skill 资源后重试".to_string(),
            ));
        }
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&content)?;
        content = encoder.finish()?;
    }

    getrandom::getrandom(&mut iv).map_err(|error| ManagerError::System(error.to_string()))?;

    let secret = backup_secret();
    let cipher = Aes256Gcm::new_from_slice(&secret)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&iv), content.as_slice())
        .map_err(|error| ManagerError::System(format!("{:?}", error)))?;
    let tag_index = encrypted.len() - 16;
    let engine = base64::engine::general_purpose::STANDARD;

    let mut envelope = json!({
      "version": 1,
      "algorithm": "aes-256-gcm",
      "iv": engine.encode(iv),
      "tag": engine.encode(&encrypted[tag_index..]),
      "content": engine.encode(&encrypted[..tag_index])
    });

    if scope == BackupScope::Cloud {
        envelope["version"] = json!(2);
        envelope["compression"] = json!("gzip");
        envelope["uncompressedSize"] = json!(uncompressed_size);
    }
    Ok(serde_json::to_string_pretty(&envelope)?)
}

fn decrypt_backup_payload(content: &str) -> Result<Value, ManagerError> {
    let payload: Value = serde_json::from_str(content).map_err(|error| {
        if error.is_eof() {
            ManagerError::System("备份文件不完整，请重新生成或上传备份".to_string())
        } else {
            ManagerError::Json(error)
        }
    })?;
    let uncompressed_size = match payload.get("version").and_then(Value::as_u64) {
        Some(1) if payload.get("compression").is_none() => None,
        Some(2) => {
            if payload.get("compression").and_then(Value::as_str) != Some("gzip") {
                return Err(ManagerError::System("不支持的备份压缩格式".to_string()));
            }
            let size = payload
                .get("uncompressedSize")
                .and_then(Value::as_u64)
                .filter(|size| *size > 0 && *size <= MAX_COMPRESSED_BACKUP_PAYLOAD_SIZE)
                .ok_or_else(|| ManagerError::System("备份解压大小无效或超过 1 GiB".to_string()))?;
            Some(size)
        }
        _ => return Err(ManagerError::System("不支持的备份文件版本".to_string())),
    };
    if payload.get("algorithm").and_then(Value::as_str) != Some("aes-256-gcm") {
        return Err(ManagerError::System("不支持的备份加密格式".to_string()));
    }
    let engine = base64::engine::general_purpose::STANDARD;
    let iv = engine
        .decode(string_value(payload.get("iv")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let tag = engine
        .decode(string_value(payload.get("tag")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    if iv.len() != 12 || tag.len() != 16 {
        return Err(ManagerError::System("备份加密参数无效".to_string()));
    }
    let encrypted = engine
        .decode(string_value(payload.get("content")))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let mut content = encrypted;

    content.extend(tag);

    let cipher = Aes256Gcm::new_from_slice(&backup_secret())
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let mut decrypted = cipher
        .decrypt(Nonce::from_slice(&iv), content.as_ref())
        .map_err(|error| ManagerError::System(format!("{:?}", error)))?;

    if let Some(size) = uncompressed_size {
        let mut decoded = Vec::new();
        GzDecoder::new(decrypted.as_slice())
            .take(size + 1)
            .read_to_end(&mut decoded)?;
        if decoded.len() as u64 != size {
            return Err(ManagerError::System(
                "备份解压大小与声明不一致，文件可能已损坏".to_string(),
            ));
        }
        decrypted = decoded;
    }

    Ok(serde_json::from_slice(&decrypted)?)
}

fn encrypt_backup_data(value: &Value) -> Result<String, ManagerError> {
    let mut iv = [0u8; 12];

    getrandom::getrandom(&mut iv).map_err(|error| ManagerError::System(error.to_string()))?;

    let secret = backup_secret();
    let cipher = Aes256Gcm::new_from_slice(&secret)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let content = format!(
        "AI_MANAGER::RUNTIME_KEYS::{}",
        serde_json::to_string(value)?
    );
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
    let mut providers = provider_store::read_providers(paths)?;
    providers.extend(provider_store::read_desktop_providers(paths)?.iter().map(|provider| {
        json!({"id": format!("claude-desktop:{}", string_value(provider.get("id")))})
    }));
    let keys = provider_store::read_keys(paths)?;
    let mut exported = Map::new();

    for provider in providers {
        let provider_id = string_value(provider.get("id"));
        let stored_value = keys.get(&provider_id);
        let records = runtime_provider::provider_key_records(stored_value);

        if records.is_empty() {
            continue;
        }

        let active_key_id = runtime_provider::active_provider_key_id(stored_value, &records);
        let mut exported_keys = Vec::new();

        for record in records {
            let Some(encrypted_key) = record.get("value").and_then(Value::as_str) else {
                continue;
            };
            let api_key = runtime_provider::decrypt_provider_key(encrypted_key)?;

            if !api_key.is_empty() {
                exported_keys.push(json!({
                  "id": string_value(record.get("id")),
                  "name": string_value(record.get("name")),
                  "note": string_value(record.get("note")),
                  "apiKey": api_key
                }));
            }
        }

        if !exported_keys.is_empty() {
            exported.insert(
                provider_id,
                json!({
                  "activeApiKeyId": active_key_id,
                  "apiKeys": exported_keys
                }),
            );
        }
    }

    Ok(Value::Object(exported))
}

fn restore_provider_key_records(value: &Value) -> Vec<Value> {
    if let Some(api_key) = value.as_str().filter(|key| !key.is_empty()) {
        return vec![json!({"id": "default", "name": "默认 Key", "note": "", "apiKey": api_key})];
    }
    value
        .get("apiKeys")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn provider_key_choice_key(provider_id: &str, key_id: &str) -> String {
    format!("provider-key:{}", json!([provider_id, key_id]))
}

fn provider_key_preview_content(value: &Value) -> Result<String, ManagerError> {
    let mut value = value.clone();
    let secret = string_value(value.get("apiKey"));
    value["apiKey"] = json!(format!("密钥指纹：{}", &sha256_text(&secret)[..12]));
    Ok(serde_json::to_string_pretty(&value)?)
}

fn append_provider_keys_preview(
    paths: &AppPaths,
    backup: &Value,
    preview: &mut Value,
) -> Result<(), ManagerError> {
    let current = export_provider_keys(paths)?;
    for (provider_id, value) in backup.as_object().cloned().unwrap_or_default() {
        let current_keys = restore_provider_key_records(&current[&provider_id]);
        for key in restore_provider_key_records(&value) {
            let key_id = string_value(key.get("id"));
            let existing = current_keys.iter().find(|item| item["id"] == key_id);
            if existing == Some(&key) {
                continue;
            }
            let status = if existing.is_some() {
                "conflict"
            } else {
                "added"
            };
            let item = json!({
                "key": provider_key_choice_key(&provider_id, &key_id),
                "type": "API Key",
                "name": key.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()).unwrap_or(&key_id),
                "path": format!("providers/{provider_id}/keys/{key_id}"),
                "groupPath": format!("providers/{provider_id}/keys"),
                "status": status,
                "currentContent": existing.map(provider_key_preview_content).transpose()?.unwrap_or_default(),
                "backupContent": provider_key_preview_content(&key)?
            });
            preview[if existing.is_some() {
                "conflicts"
            } else {
                "added"
            }]
            .as_array_mut()
            .unwrap()
            .push(item);
        }
    }
    preview["addedCount"] = json!(preview["added"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default());
    preview["conflictCount"] = json!(preview["conflicts"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default());
    Ok(())
}

async fn merge_provider_keys(
    paths: &AppPaths,
    api_keys: &Value,
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    let mut next_keys = provider_store::read_keys(paths)?;
    let current = export_provider_keys(paths)?;
    let mut provider_ids = provider_store::read_providers(paths)?
        .iter()
        .map(|provider| string_value(provider.get("id")))
        .collect::<HashSet<_>>();
    provider_ids.extend(
        provider_store::read_desktop_providers(paths)?
            .iter()
            .map(|provider| format!("claude-desktop:{}", string_value(provider.get("id")))),
    );
    for (provider_id, backup) in api_keys.as_object().cloned().unwrap_or_default() {
        if !provider_ids.contains(&provider_id) {
            continue;
        }
        let mut merged = restore_provider_key_records(&current[&provider_id]);
        for key in restore_provider_key_records(&backup) {
            let key_id = string_value(key.get("id"));
            if let Some(index) = merged.iter().position(|item| item["id"] == key_id) {
                if choice_text(choices, &provider_key_choice_key(&provider_id, &key_id)) == "backup"
                {
                    merged[index] = key;
                }
            } else {
                merged.push(key);
            }
        }
        if !merged.is_empty() {
            let existing = runtime_provider::provider_key_records(next_keys.get(&provider_id));
            let active = if existing.is_empty() {
                string_value(backup.get("activeApiKeyId"))
            } else {
                runtime_provider::active_provider_key_id(next_keys.get(&provider_id), &existing)
            };
            runtime_provider::set_provider_keys(&mut next_keys, &provider_id, &merged, active)?;
        }
    }
    provider_store::write_keys(paths, &next_keys)
}

async fn migrate_skill_repository_storage(paths: &AppPaths) -> Result<(), ManagerError> {
    let storage_path = Path::new(&paths.storage_files.skill_repositories);
    let cache_path = Path::new(&paths.storage_files.skill_repository_cache);

    if !storage_path.exists() {
        return Ok(());
    }

    let repositories: Value =
        serde_json::from_str(&tokio::fs::read_to_string(storage_path).await?)?;

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

async fn read_current_file(
    root_path: &str,
    entry_path: &str,
) -> Result<Option<Vec<u8>>, ManagerError> {
    let target_path = assert_backup_path(root_path, entry_path)?;

    match std::fs::symlink_metadata(&target_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(ManagerError::System(format!(
                "备份文件目标不能是链接：{}",
                path_text(target_path)
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ManagerError::Io(error)),
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

    let mut parent = target_path.parent();
    while let Some(path) = parent.filter(|path| *path != root) {
        if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(ManagerError::System(format!(
                "备份路径不能穿过链接：{}",
                path_text(path)
            )));
        }
        parent = path.parent();
    }

    Ok(target_path)
}

fn validate_backup_symlink_target(
    root_path: &str,
    link_path: &Path,
    target: &str,
) -> Result<(), ManagerError> {
    let target_path = Path::new(target);

    if target.is_empty()
        || target_path.is_absolute()
        || target_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(ManagerError::System("备份链接目标非法".to_string()));
    }

    let resolved_target = link_path
        .parent()
        .unwrap_or_else(|| Path::new(root_path))
        .join(target_path);
    let relative_target = resolved_target
        .strip_prefix(root_path)
        .map_err(|_| ManagerError::System("备份链接目标超出工作区".to_string()))?;
    let checked_target = assert_backup_path(root_path, &path_text(relative_target))?;

    if checked_target.exists()
        && !std::fs::canonicalize(&checked_target)?.starts_with(std::fs::canonicalize(root_path)?)
    {
        return Err(ManagerError::System("备份链接目标超出工作区".to_string()));
    }
    Ok(())
}

fn is_allowed_backup_path(entry_path: &str) -> bool {
    let normalized_path = entry_path
        .replace('\\', "/")
        .trim_matches('/')
        .to_lowercase();

    matches!(
        normalized_path.as_str(),
        "storage/ai-manager.db"
            | "storage/providers.json"
            | "storage/runtime-models.json"
            | "storage/codex-accounts.json"
            | "storage/skills.json"
            | "storage/skill-groups.json"
            | "storage/skill-repositories.json"
            | "storage/rules.json"
            | "skills"
            | "prompts"
            | "pets-disabled"
    ) || normalized_path.starts_with("skills/")
        || normalized_path.starts_with("prompts/")
        || normalized_path.starts_with("pets-disabled/")
}

fn is_allowed_codex_pet_backup_entry(entry: &Value) -> bool {
    let entry_path = string_value(entry.get("path"));
    let path = Path::new(&entry_path);

    !entry_path.is_empty()
        && matches!(
            entry.get("type").and_then(Value::as_str),
            Some("dir") | Some("file")
        )
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn restore_storage_name(entry_path: &str) -> Option<&'static str> {
    match entry_path {
        "app-settings.json" => Some("云同步设置"),
        "storage/skill-groups.json" => Some("Skill 分组"),
        "storage/skill-repositories.json" => Some("Skill 仓库"),
        "storage/skills.json" => Some("Skill 索引"),
        "storage/ai-manager.db" => Some("主数据库"),
        "storage/providers.json" => Some("Provider"),
        "storage/runtime-models.json" => Some("模型"),
        "storage/codex-accounts.json" => Some("Codex 官方账号"),
        "storage/rules.json" => Some("Prompt 索引"),
        _ => None,
    }
}

fn is_storage_json_path(entry_path: &str) -> bool {
    entry_path.starts_with("storage/") && entry_path.ends_with(".json")
}

fn is_mergeable_restore_json_path(entry_path: &str) -> bool {
    HashSet::from([
        "storage/skill-groups.json",
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

    if entry_path == "app-settings.json" {
        return Ok(serde_json::to_string_pretty(&redact_backup_app_settings(
            value,
        ))?);
    }

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
        return Ok(serde_json::to_string_pretty(
            &normalize_skill_restore_value(value),
        )?);
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
        for field in [
            "disabled",
            "installedTargets",
            "installStates",
            "status",
            "sourcePath",
            "entryPath",
            "repoName",
        ] {
            map.remove(field);
        }
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
        map.remove("disabled");
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
    if entry_path == "app-settings.json" {
        return Ok(sha256_text(&serde_json::to_string(value)?));
    }
    Ok(sha256_text(&normalize_restore_value(entry_path, value)?))
}

fn get_restore_item_key(entry_path: &str, item: &Value, index: usize) -> String {
    if let Some(item) = item.as_object() {
        if entry_path == "storage/skills.json" {
            let name = string_value(item.get("name"));

            if !name.is_empty() {
                return name;
            }
        }

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

    if entry_path == "app-settings.json" {
        if item_key == "cloudSync" {
            return "坚果云".to_string();
        }
        if item_key == "koofrSync" {
            return "Koofr".to_string();
        }
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
          restore_storage_name(entry_path).unwrap_or("文件")
      },
      "name": Path::new(entry_path).file_name().map(|value| value.to_string_lossy().to_string()).unwrap_or_default(),
      "path": entry_path,
      "groupPath": get_restore_group_path(entry_path),
      "status": status,
      "currentContent": current_content,
      "backupContent": backup_content
    })
}

fn format_restore_file_content(content: &[u8]) -> String {
    match std::str::from_utf8(content) {
        Ok(content) => content.to_string(),
        Err(_) => format!(
            "二进制文件，大小：{} 字节，SHA-256：{}",
            content.len(),
            sha256_bytes(content)
        ),
    }
}

fn create_database_row_restore_preview(
    entry_path: &str,
    difference: &database::RestoreRowDifference,
) -> Result<Value, ManagerError> {
    let name = [
        "name",
        "displayName",
        "title",
        "email",
        "modelId",
        "model_id",
        "fileName",
    ]
    .iter()
    .find_map(|field| {
        difference
            .backup
            .get(*field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    })
    .unwrap_or(&difference.row_key);
    Ok(json!({
        "key": database::row_choice_key(&difference.table, &difference.row_key),
        "type": restore_database_table_name(&difference.table),
        "name": name,
        "path": format!("{entry_path}/{}/{}", difference.table, difference.row_key),
        "groupPath": format!("{entry_path}/{}", difference.table),
        "status": if difference.current.is_some() { "conflict" } else { "added" },
        "currentContent": difference.current.as_ref().map(serde_json::to_string_pretty).transpose()?.unwrap_or_default(),
        "backupContent": serde_json::to_string_pretty(&difference.backup)?
    }))
}

fn restore_database_table_name(table: &str) -> &'static str {
    match table {
        "usage_metadata" => "用量元数据",
        "usage_logs" => "使用记录",
        "usage_pricing_config" => "模型费用配置",
        "usage_pricing_items" => "模型费用明细",
        "skills" => "Skill 索引",
        "skill_groups" => "Skill 分组",
        "skill_repositories" => "Skill 仓库",
        "rule_prompts" => "Rule 内容",
        "rule_profiles" => "Rule 配置",
        "providers" => "Provider",
        "provider_models" => "Provider 模型",
        "claude_desktop_providers" => "Claude Desktop 供应商",
        "codex_accounts" => "Codex 官方账号",
        _ => "应用数据",
    }
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

fn is_database_backup_path(entry_path: &str) -> bool {
    entry_path == "storage/ai-manager.db"
}

#[cfg(test)]
mod tests {
    use super::{
        append_app_settings_restore_preview, collect_backup_entries, collect_codex_pet_entries,
        create_backup_entry_view, decrypt_backup_payload, encrypt_backup_payload,
        format_restore_file_content, is_allowed_backup_path, is_database_backup_path,
        merge_json_backup_value, merge_provider_keys, prepare_pet_restore_entries,
        preview_data_backup_restore_content, redact_backup_app_settings,
        restore_backup_app_settings, restore_codex_pet_entries, restore_data_backup_content,
        restore_directory_entries, sanitize_runtime_backup_entries, serialize_backup_app_settings,
        validate_backup_symlink_target, webdav_content_length, BackupScope,
    };
    use crate::api::runtime_provider;
    use crate::core::paths::resolve_app_paths;
    use crate::core::settings::normalize_app_settings;
    use crate::core::usage_store::{self, UsageSessionUpdate};
    use crate::core::{database, provider_store, skill_store};
    use base64::Engine;
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_LENGTH};
    use serde_json::{json, Map};
    use std::path::Path;

    async fn webdav_test_server(responses: Vec<Vec<u8>>) -> (crate::core::settings::CloudSyncSettings, tokio::task::JoinHandle<Vec<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept()).await.unwrap().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 2048];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).await.unwrap();
                    assert!(count > 0 && request.len() < 16384);
                    request.extend_from_slice(&buffer[..count]);
                }
                requests.push(String::from_utf8(request).unwrap().to_lowercase());
                stream.write_all(&response).await.unwrap();
                stream.shutdown().await.unwrap();
            }
            requests
        });
        let config = super::normalize_cloud_sync_settings(&json!({
            "provider": "koofr", "webdavUrl": format!("http://{address}/dav"),
            "username": "test", "password": "test-secret", "fileName": "test.aimbackup"
        }));
        (config, task)
    }

    fn webdav_test_response(status: u16, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!("HTTP/1.1 {status} Test\r\nConnection: close\r\n{headers}\r\n").into_bytes();
        response.extend_from_slice(body);
        response
    }

    #[test]
    fn webdav_download_retries_only_the_interrupted_range_and_preserves_utf8() {
        tauri::async_runtime::block_on(async {
            let chunk_size = 4 * 1024 * 1024;
            let content = format!("{}中文备份", "a".repeat(chunk_size - 1));
            let total = content.len();
            let head = webdav_test_response(200, &format!("Content-Length: {total}\r\nAccept-Ranges: bytes\r\nETag: \"version-1\"\r\n"), b"");
            let first_headers = format!("Content-Length: {chunk_size}\r\nContent-Range: bytes 0-{}/{total}\r\nETag: \"version-1\"\r\n", chunk_size - 1);
            let first = webdav_test_response(206, &first_headers, &content.as_bytes()[..chunk_size]);
            let last_headers = format!("Content-Length: {}\r\nContent-Range: bytes {chunk_size}-{}/{total}\r\nETag: \"version-1\"\r\n", total - chunk_size, total - 1);
            let interrupted = webdav_test_response(206, &last_headers, &content.as_bytes()[chunk_size..chunk_size + 1]);
            let last = webdav_test_response(206, &last_headers, &content.as_bytes()[chunk_size..]);
            let (config, server) = webdav_test_server(vec![head, first, interrupted, last]).await;
            assert_eq!(super::download_webdav_backup(&config).await.unwrap(), content);
            let requests = server.await.unwrap();
            assert!(requests[0].starts_with("head "));
            assert!(requests[1].contains(&format!("range: bytes=0-{}", chunk_size - 1)));
            assert_eq!(requests[2], requests[3]);
            assert!(requests[3].contains(&format!("range: bytes={chunk_size}-{}", total - 1)));
            for request in &requests[1..] {
                assert!(request.contains("if-match: \"version-1\""));
                assert!(request.contains("accept-encoding: identity"));
            }
        });
    }

    #[test]
    fn webdav_download_supports_servers_without_range_or_strong_etag() {
        tauri::async_runtime::block_on(async {
            for head in [
                webdav_test_response(405, "Content-Length: 0\r\n", b""),
                webdav_test_response(200, "Content-Length: 100\r\nAccept-Ranges: bytes\r\nETag: W/\"weak-version\"\r\n", b""),
            ] {
                let content = "完整备份文本";
                let get = webdav_test_response(200, &format!("Content-Length: {}\r\nContent-Type: text/plain; charset=iso-8859-1\r\n", content.len()), content.as_bytes());
                let (config, server) = webdav_test_server(vec![head, get]).await;
                assert_eq!(super::download_webdav_backup(&config).await.unwrap(), content);
                let requests = server.await.unwrap();
                assert!(!requests[1].contains("range:"));
            }
        });
    }

    #[test]
    fn webdav_download_refuses_changed_versions_wrong_ranges_and_partial_data() {
        tauri::async_runtime::block_on(async {
            for (status, headers, expected) in [
                (412, "Content-Length: 0\r\n", "已更新"),
                (206, "Content-Length: 4\r\nContent-Range: bytes 1-4/5\r\nETag: \"version-1\"\r\n", "分段范围不匹配"),
                (206, "Content-Length: 4\r\nContent-Range: bytes 0-3/4\r\nETag: \"version-2\"\r\n", "版本发生变化"),
                (200, "Content-Length: 0\r\n", "HTTP 200"),
                (401, "Content-Length: 0\r\n", "HTTP 401"),
                (401, "Content-Length: 4\r\n", "HTTP 401"),
            ] {
                let (config, server) = webdav_test_server(vec![webdav_test_response(status, headers, b"")]).await;
                let range = super::WebdavDownloadRange { start: 0, end: 3, total: 4, etag: HeaderValue::from_static("\"version-1\"") };
                let cause = super::download_webdav_part(&reqwest::Client::new(), &config, Some(&range)).await.unwrap_err().to_string();
                assert!(cause.contains(expected), "{cause}");
                assert!(!cause.contains("test-secret"));
                assert_eq!(server.await.unwrap().len(), 1);
            }
            let truncated = webdav_test_response(206, "Content-Length: 4\r\nContent-Range: bytes 0-3/4\r\nETag: \"version-1\"\r\n", b"ab");
            let (config, server) = webdav_test_server(vec![truncated.clone(), truncated.clone(), truncated]).await;
            let range = super::WebdavDownloadRange { start: 0, end: 3, total: 4, etag: HeaderValue::from_static("\"version-1\"") };
            let cause = super::download_webdav_part(&reqwest::Client::new(), &config, Some(&range)).await.unwrap_err().to_string();
            assert!(cause.contains("已尝试 3 次"));
            assert!(cause.contains("本地数据未恢复"));
            assert!(!cause.contains("test-secret"));
            assert_eq!(server.await.unwrap().len(), 3);
        });
    }

    #[test]
    fn webdav_preview_never_restores_or_caches_an_interrupted_backup() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!("ai-manager-cloud-preview-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let sentinel = root.join("keep.txt");
            std::fs::write(&sentinel, "original local data").unwrap();
            let head = webdav_test_response(200, "Content-Length: 4\r\nAccept-Ranges: bytes\r\nETag: \"version-1\"\r\n", b"");
            let truncated = webdav_test_response(206, "Content-Length: 4\r\nContent-Range: bytes 0-3/4\r\nETag: \"version-1\"\r\n", b"ab");
            let (config, server) = webdav_test_server(vec![head, truncated.clone(), truncated.clone(), truncated]).await;
            let paths = resolve_app_paths(&root);
            let settings = normalize_app_settings(root.join("settings.json"), None);
            let mut cache = super::DataBackupCache::new();
            let result = super::preview_cloud_backup_restore(&paths, &settings, &mut cache, serde_json::to_value(config).unwrap()).await;
            assert!(result.is_err());
            assert!(cache.drafts.is_empty());
            assert_eq!(server.await.unwrap().len(), 4);
            let invalid = b"{invalid-backup";
            let (config, server) = webdav_test_server(vec![
                webdav_test_response(405, "Content-Length: 0\r\n", b""),
                webdav_test_response(200, &format!("Content-Length: {}\r\n", invalid.len()), invalid),
            ]).await;
            assert!(super::preview_cloud_backup_restore(&paths, &settings, &mut cache, serde_json::to_value(config).unwrap()).await.is_err());
            assert!(cache.drafts.is_empty());
            assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "original local data");
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
            assert_eq!(server.await.unwrap().len(), 2);
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(resolved.file_name().unwrap().to_string_lossy().starts_with("ai-manager-cloud-preview-"));
            std::fs::remove_dir_all(resolved).unwrap();
        });
    }

    #[test]
    fn claude_desktop_backup_keeps_providers_and_restores_keys_without_local_gateway_state() {
        let root = std::env::temp_dir().join(format!("ai-manager-desktop-backup-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let mut keys = Map::new();
        runtime_provider::set_provider_key(&mut keys, "same-id", "cli-secret".to_string()).unwrap();
        runtime_provider::set_provider_key(&mut keys, "claude-desktop:same-id", "desktop-secret".to_string()).unwrap();
        provider_store::write_provider_bundle(&paths, &[json!({"id": "same-id"})], &[], &[], &keys).unwrap();
        provider_store::write_desktop_bundle(&paths, &[json!({"id": "same-id", "name": "Desktop"})], json!({"currentProviderId": "same-id", "claude-desktop:gateway": "local-only"}).as_object().unwrap(), &keys).unwrap();
        let exported = super::export_provider_keys(&paths).unwrap();
        assert_eq!(exported["same-id"]["apiKeys"][0]["apiKey"], "cli-secret");
        assert_eq!(exported["claude-desktop:same-id"]["apiKeys"][0]["apiKey"], "desktop-secret");
        assert!(!exported.to_string().contains("local-only"));
        let snapshot = database::backup(&paths).unwrap();
        let snapshot_path = root.join("snapshot.db");
        std::fs::write(&snapshot_path, snapshot).unwrap();
        let connection = rusqlite::Connection::open(&snapshot_path).unwrap();
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM claude_desktop_providers", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM claude_desktop_settings", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM provider_keys", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        drop(connection);
        runtime_provider::set_provider_key(&mut keys, "same-id", "keep-cli-secret".to_string()).unwrap();
        keys.remove("claude-desktop:same-id");
        provider_store::write_keys(&paths, &keys).unwrap();
        let choices = json!({"database:storage/ai-manager.db:claude_desktop_providers": "backup"});
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(
            merge_provider_keys(&paths, &exported, choices.as_object().unwrap())
        ).unwrap();
        assert_eq!(runtime_provider::get_provider_api_key(&paths, "same-id").unwrap(), "keep-cli-secret");
        assert_eq!(runtime_provider::get_provider_api_key(&paths, "claude-desktop:same-id").unwrap(), "desktop-secret");
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(resolved).unwrap();
    }

    #[test]
    fn formats_binary_restore_content_as_summary() {
        let content = [0x89, b'P', b'N', b'G'];
        let summary = format_restore_file_content(&content);

        assert!(summary.starts_with("二进制文件，大小：4 字节，SHA-256："));
    }

    #[test]
    fn reports_truncated_backup_as_incomplete() {
        let error = decrypt_backup_payload("{\n  \"version\": 1,\n  \"content\": \"abc")
            .expect_err("截断备份应返回错误");

        assert_eq!(
            error.to_string(),
            "系统调用失败：备份文件不完整，请重新生成或上传备份"
        );
    }

    #[test]
    fn reads_webdav_file_size_from_content_length_header() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("146938538"));

        assert_eq!(webdav_content_length(&headers), Some(146_938_538));
    }

    #[test]
    fn backup_symlink_targets_must_stay_inside_workspace() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-backup-link-safety-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let skills_root = root.join("skills");
        let link_path = skills_root.join("linked-skill");
        std::fs::create_dir_all(&skills_root).unwrap();
        std::fs::write(skills_root.join("target.txt"), "target").unwrap();

        assert!(
            validate_backup_symlink_target(&root.to_string_lossy(), &link_path, "target.txt")
                .is_ok()
        );
        assert!(validate_backup_symlink_target(
            &root.to_string_lossy(),
            &link_path,
            "../outside.txt"
        )
        .is_err());
        assert!(validate_backup_symlink_target(
            &root.to_string_lossy(),
            &link_path,
            &root.join("outside.txt").to_string_lossy()
        )
        .is_err());
    }

    #[test]
    fn exports_only_config_database_skills_and_prompts() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-data-backup-scope-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        provider_store::write_provider_bundle(
            &paths,
            &[json!({"id": "provider-a", "enabled": true})],
            &[],
            &[],
            &Map::new(),
        )
        .unwrap();
        usage_store::write_pricing(
            &paths,
            &json!({
              "exchangeRate": 7.4,
              "items": [{
                "id": "pricing-1",
                "modelId": "gpt-test",
                "currency": "USD"
              }]
            }),
        )
        .unwrap();
        usage_store::replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".to_string(),
                app_type: "codex".to_string(),
                updated_at: 100,
                logs: vec![json!({
                  "requestId": "request-1",
                  "rawPath": "session.jsonl",
                  "createdAt": 100,
                  "appType": "codex"
                })],
                records: vec![json!({
                  "requestId": "request-1",
                  "createdAt": 100,
                  "providerId": "provider-1"
                })],
            }],
        )
        .unwrap();
        let skill_file = Path::new(&paths.skills_dir)
            .join("skill-a")
            .join("SKILL.md");
        let prompt_file = Path::new(&paths.prompts_dir)
            .join("common")
            .join("rule-a.md");
        let git_tool_file = Path::new(&paths.workspace_root)
            .join("git-tool")
            .join("archive.git")
            .join("objects.pack");
        std::fs::create_dir_all(skill_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(prompt_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(git_tool_file.parent().unwrap()).unwrap();
        std::fs::write(&skill_file, "# Skill A").unwrap();
        std::fs::write(&prompt_file, "# Rule A").unwrap();
        std::fs::write(&git_tool_file, vec![1u8; 1024]).unwrap();
        std::fs::write(
            &paths.storage_files.codex_proxy_config,
            "{\"enabled\":true}",
        )
        .unwrap();
        std::fs::create_dir_all(&paths.sessions_dir).unwrap();
        std::fs::write(
            Path::new(&paths.sessions_dir).join("session.jsonl"),
            "session",
        )
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let entries = runtime
            .block_on(collect_backup_entries(&paths, BackupScope::Local))
            .unwrap();
        let entry_paths = entries
            .iter()
            .map(|entry| entry["path"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        let database_entry = entries
            .iter()
            .find(|entry| entry["path"] == "storage/ai-manager.db")
            .unwrap();
        let database = base64::engine::general_purpose::STANDARD
            .decode(database_entry["content"].as_str().unwrap())
            .unwrap();
        let database_view = create_backup_entry_view(database_entry).unwrap();
        let restore_root = std::env::temp_dir().join(format!(
            "monkey-thief-usage-backup-restore-{}",
            std::process::id()
        ));
        if restore_root.exists() {
            std::fs::remove_dir_all(&restore_root).unwrap();
        }
        let restore_paths = resolve_app_paths(Path::new(&restore_root));
        usage_store::initialize(&restore_paths).unwrap();
        provider_store::initialize(&restore_paths).unwrap();
        let restore_tables = database::preview_restore(&restore_paths, &database)
            .unwrap()
            .into_iter()
            .map(|difference| difference.table)
            .collect::<Vec<_>>();

        assert!(entry_paths.iter().all(|path| is_allowed_backup_path(path)));
        assert!(entry_paths
            .iter()
            .any(|path| path == "skills/skill-a/SKILL.md"));
        assert!(entry_paths
            .iter()
            .any(|path| path == "prompts/common/rule-a.md"));
        assert!(!entry_paths.iter().any(|path| path.starts_with("git-tool")));
        assert!(!entry_paths.iter().any(|path| path.starts_with("sessions")));
        assert!(!entry_paths.iter().any(|path| path.contains("proxy")));
        assert!(!entry_paths.iter().any(|path| path.contains("usage")));
        assert_eq!(
            restore_tables,
            vec!["providers", "usage_pricing_config", "usage_pricing_items"]
        );
        database::restore_selected(&restore_paths, &database, &restore_tables).unwrap();
        assert_eq!(
            usage_store::read_pricing(&restore_paths).unwrap(),
            usage_store::read_pricing(&paths).unwrap()
        );
        let restore_connection = database::open(&restore_paths).unwrap();
        for table in ["usage_logs", "usage_request_records"] {
            assert_eq!(
                restore_connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );
        }
        assert_eq!(database_view["typeName"], "主数据库");
        assert!(database_view["content"]
            .as_str()
            .is_some_and(|content| content.contains("SHA-256")));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| {
                    entry
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(is_database_backup_path)
                })
                .count(),
            1
        );
    }

    #[test]
    fn cloud_backup_filter_keeps_skill_resources_and_only_runtime_pet_files() {
        for path in [
            "skills/example/SKILL.md",
            "skills/example/scripts/run.py",
            "skills/example/references/api.md",
            "skills/example/assets/icon.png",
            "skills/example/data/colors.csv",
            "skills/example/data/catalog.json",
            "skills/example/data/stacks/vue.csv",
            "skills/example/data/profile.json",
            "skills/example/memory/build-user-profile.ps1",
            "skills/example/memory/confidence-index.md",
            "prompts/common/rule.md",
            "pets/pet-a/pet.json",
            "pets/pet-a/spritesheet.webp",
            "pets-disabled/pet-b/pet.json",
            "pets-disabled/pet-b/spritesheet.webp",
        ] {
            assert!(super::is_cloud_backup_entry(path, false), "应保留 {path}");
        }
        for path in [
            "skills/.git",
            "skills/example/.git",
            "skills/example/node_modules",
            "skills/example/scripts/__pycache__",
            "skills/example/.cache",
            "skills/example/.venv",
            "skills/example/logs",
            "skills/example/cache",
            "skills/example/backups",
            "skills/example/data/work-history",
            "skills/example/data/logs",
            "skills/example/data/persona-backups",
            "skills/example/data/system-changelog",
            "skills/example/data/self-improvement",
            "pets/pet-a/preview",
            "pets-disabled/pet-b/source",
        ] {
            assert!(
                !super::is_cloud_backup_entry(path, true),
                "应排除目录 {path}"
            );
        }
        for path in [
            "skills/example/data/log.json",
            "skills/example/data/system-log.json",
            "skills/example/data/log-summary.json",
            "skills/example/data/system-changelog-summary.json",
            "skills/example/scripts/run.pyc",
            "skills/example/SKILL.md.bak",
            "skills/example/SKILL.md~",
            "skills/example/download.tmp",
            "pets/pet-a/spritesheet.backup-2026.webp",
            "pets/pet-a/spritesheet.before-edit.webp",
            "pets/pet-a/preview.mp4",
            "pets-disabled/pet-b/preview.png",
            "pets-disabled/pet-b/preview/pet.json",
            "sessions/session.jsonl",
            "storage/settings.json",
        ] {
            assert!(
                !super::is_cloud_backup_entry(path, false),
                "应排除文件 {path}"
            );
        }
    }

    #[test]
    fn cloud_backup_scope_does_not_change_local_backups_or_source_files() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-cloud-backup-scope-{}-{}",
            std::process::id(),
            super::now_millis()
        ));
        let paths = resolve_app_paths(&root.join("data"));
        let codex_path = root.join("codex");
        let mut app_settings = normalize_app_settings(
            root.join("settings.json"),
            Some(json!({
                "cliConfigPaths": {"codex": codex_path.to_string_lossy()},
                "cloudSync": {"username": "local-user", "password": "local-password"}
            })),
        );
        let kept_paths = [
            "skills/example/SKILL.md",
            "skills/example/scripts/run.py",
            "skills/example/data/colors.csv",
            "prompts/common/rule.md",
            "pets-disabled/disabled/pet.json",
            "pets-disabled/disabled/spritesheet.webp",
        ];
        let excluded_paths = [
            "skills/example/data/work-history/project/batches/run.json",
            "skills/example/data/logs/run.json",
            "skills/example/.git/objects/pack.bin",
            "skills/example/node_modules/dependency/index.js",
            "skills/example/scripts/__pycache__/run.pyc",
            "pets-disabled/disabled/spritesheet.backup-old.webp",
            "pets-disabled/disabled/preview/preview.mp4",
        ];
        let file_content = "backup-scope-test\n".repeat(128);
        for path in kept_paths.iter().chain(excluded_paths.iter()) {
            let file = Path::new(&paths.workspace_root).join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, &file_content).unwrap();
        }
        let enabled_pet = codex_path.join("pets/enabled");
        std::fs::create_dir_all(&enabled_pet).unwrap();
        for name in [
            "pet.json",
            "spritesheet.webp",
            "spritesheet.before-edit.webp",
            "preview.png",
        ] {
            std::fs::write(enabled_pet.join(name), &file_content).unwrap();
        }
        let mut keys = Map::new();
        runtime_provider::set_provider_key(&mut keys, "provider-a", "test-secret".to_string())
            .unwrap();
        provider_store::write_provider_bundle(
            &paths,
            &[json!({"id": "provider-a"})],
            &[],
            &[],
            &keys,
        )
        .unwrap();
        let local = runtime
            .block_on(super::create_data_backup(&paths, &app_settings))
            .unwrap();
        let cloud = runtime
            .block_on(super::create_scoped_data_backup(
                &paths,
                &app_settings,
                BackupScope::Cloud,
            ))
            .unwrap();
        let local_payload = super::parse_backup(&local).unwrap();
        let cloud_payload = super::parse_backup(&cloud).unwrap();
        assert!(cloud.len() < local.len());
        assert!(cloud_payload.get("appSettings").is_none());
        assert_eq!(
            local_payload["appSettings"]["cloudSync"]["password"],
            "local-password"
        );
        assert_eq!(
            super::decrypt_backup_data(cloud_payload["runtimeProviderKeys"].as_str().unwrap())
                .unwrap(),
            super::export_provider_keys(&paths).unwrap()
        );
        let local_entries = local_payload["workspaceEntries"].as_array().unwrap();
        let cloud_entries = cloud_payload["workspaceEntries"].as_array().unwrap();
        for path in kept_paths {
            assert!(
                cloud_entries.iter().any(|entry| entry["path"] == path),
                "应保留 {path}"
            );
        }
        for path in excluded_paths {
            assert!(
                !cloud_entries.iter().any(|entry| entry["path"] == path),
                "云备份应排除 {path}"
            );
            assert!(
                local_entries.iter().any(|entry| entry["path"] == path),
                "本地备份应保留 {path}"
            );
            assert_eq!(
                std::fs::read_to_string(Path::new(&paths.workspace_root).join(path)).unwrap(),
                file_content
            );
        }
        let cloud_pets = cloud_payload["codexPetEntries"].as_array().unwrap();
        assert_eq!(cloud_pets.len(), 3);
        assert!(cloud_pets
            .iter()
            .any(|entry| entry["path"] == "enabled/pet.json"));
        assert!(cloud_pets
            .iter()
            .any(|entry| entry["path"] == "enabled/spritesheet.webp"));
        assert_eq!(
            local_payload["codexPetEntries"].as_array().unwrap().len(),
            5
        );
        assert_eq!(
            std::fs::read_to_string(enabled_pet.join("spritesheet.before-edit.webp")).unwrap(),
            file_content
        );

        let restore_paths = resolve_app_paths(&root.join("restore"));
        runtime
            .block_on(restore_directory_entries(
                &restore_paths,
                cloud_entries.clone(),
                &Map::new(),
                &std::collections::HashMap::new(),
            ))
            .unwrap();
        for path in kept_paths {
            assert_eq!(
                std::fs::read_to_string(Path::new(&restore_paths.workspace_root).join(path))
                    .unwrap(),
                file_content
            );
        }
        let original_settings = serialize_backup_app_settings(&app_settings);
        restore_backup_app_settings(&mut app_settings, &cloud_payload, &Map::new()).unwrap();
        assert_eq!(
            serialize_backup_app_settings(&app_settings),
            original_settings
        );
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(resolved).unwrap();
    }

    #[test]
    fn compressed_cloud_backups_and_legacy_backups_round_trip() {
        let payload =
            json!({"version": 1, "workspaceEntries": [], "testData": "compress-me".repeat(1000)});
        let legacy = encrypt_backup_payload(&payload, BackupScope::Local).unwrap();
        let compressed = encrypt_backup_payload(&payload, BackupScope::Cloud).unwrap();
        let legacy_envelope: serde_json::Value = serde_json::from_str(&legacy).unwrap();
        let compressed_envelope: serde_json::Value = serde_json::from_str(&compressed).unwrap();
        assert_eq!(legacy_envelope["version"], 1);
        assert!(legacy_envelope.get("compression").is_none());
        assert_eq!(compressed_envelope["version"], 2);
        assert_eq!(compressed_envelope["compression"], "gzip");
        assert_eq!(decrypt_backup_payload(&legacy).unwrap(), payload);
        assert_eq!(decrypt_backup_payload(&compressed).unwrap(), payload);
        assert!(compressed.len() < legacy.len() / 10);
    }

    #[test]
    fn cloud_backup_decoder_rejects_invalid_compression_and_sizes() {
        let content =
            encrypt_backup_payload(&json!({"value": "test"}), BackupScope::Cloud).unwrap();
        let envelope: serde_json::Value = serde_json::from_str(&content).unwrap();
        for (field, value) in [
            ("version", json!(99)),
            ("compression", json!("unknown")),
            ("uncompressedSize", json!(0)),
            (
                "uncompressedSize",
                json!(super::MAX_COMPRESSED_BACKUP_PAYLOAD_SIZE + 1),
            ),
            ("uncompressedSize", json!(1)),
            ("uncompressedSize", json!(1024)),
            ("iv", json!("AA==")),
            ("tag", json!("AA==")),
            ("content", json!("AAAA")),
        ] {
            let mut invalid = envelope.clone();
            invalid[field] = value;
            assert!(
                decrypt_backup_payload(&invalid.to_string()).is_err(),
                "应拒绝无效 {field}"
            );
        }
        let legacy = encrypt_backup_payload(&json!({"value": "test"}), BackupScope::Local).unwrap();
        let mut invalid: serde_json::Value = serde_json::from_str(&legacy).unwrap();
        invalid["version"] = json!(2);
        invalid["compression"] = json!("gzip");
        invalid["uncompressedSize"] = json!(16);
        assert!(decrypt_backup_payload(&invalid.to_string()).is_err());
    }

    #[test]
    fn syncing_codex_pets_preserves_local_enabled_state() {
        let root =
            std::env::temp_dir().join(format!("monkey-thief-pet-backup-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }

        let source_paths = resolve_app_paths(&root.join("source-data"));
        let source_codex_path = root.join("source-codex");
        let source_settings = normalize_app_settings(
            root.join("source-settings.json"),
            Some(json!({
              "cliConfigPaths": { "codex": source_codex_path.to_string_lossy() }
            })),
        );
        let enabled_pet = source_codex_path.join("pets").join("disabled-locally");
        let new_enabled_pet = source_codex_path.join("pets").join("new-enabled");
        let invalid_pet = source_codex_path.join("pets").join("$out");
        let disabled_pet = Path::new(&source_paths.disabled_pets_dir).join("enabled-locally");
        let new_disabled_pet = Path::new(&source_paths.disabled_pets_dir).join("new-disabled");
        std::fs::create_dir_all(&enabled_pet).unwrap();
        std::fs::create_dir_all(&new_enabled_pet).unwrap();
        std::fs::create_dir_all(&invalid_pet).unwrap();
        std::fs::create_dir_all(&disabled_pet).unwrap();
        std::fs::create_dir_all(&new_disabled_pet).unwrap();
        std::fs::write(enabled_pet.join("pet.json"), r#"{"id":"disabled-locally"}"#).unwrap();
        std::fs::write(enabled_pet.join("spritesheet.webp"), [1_u8, 2, 3]).unwrap();
        std::fs::write(new_enabled_pet.join("pet.json"), r#"{"id":"new-enabled"}"#).unwrap();
        std::fs::write(new_enabled_pet.join("spritesheet.webp"), [3_u8, 4, 5]).unwrap();
        std::fs::write(invalid_pet.join("build.js"), [7_u8, 8, 9]).unwrap();
        std::fs::write(disabled_pet.join("pet.json"), r#"{"id":"enabled-locally"}"#).unwrap();
        std::fs::write(disabled_pet.join("spritesheet.webp"), [4_u8, 5, 6]).unwrap();
        std::fs::write(
            new_disabled_pet.join("pet.json"),
            r#"{"id":"new-disabled"}"#,
        )
        .unwrap();
        std::fs::write(new_disabled_pet.join("spritesheet.webp"), [6_u8, 7, 8]).unwrap();

        let target_paths = resolve_app_paths(&root.join("target-data"));
        let target_codex_path = root.join("target-codex");
        let target_settings = normalize_app_settings(
            root.join("target-settings.json"),
            Some(json!({
              "cliConfigPaths": { "codex": target_codex_path.to_string_lossy() }
            })),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let backup_codex_entries = runtime
            .block_on(collect_codex_pet_entries(&source_settings, BackupScope::Local))
            .unwrap();
        let backup_workspace_entries = runtime
            .block_on(collect_backup_entries(&source_paths, BackupScope::Local))
            .unwrap();

        let current_enabled_pet = target_codex_path.join("pets").join("enabled-locally");
        let current_disabled_pet =
            Path::new(&target_paths.disabled_pets_dir).join("disabled-locally");
        std::fs::create_dir_all(&current_enabled_pet).unwrap();
        std::fs::create_dir_all(&current_disabled_pet).unwrap();
        std::fs::write(
            current_enabled_pet.join("pet.json"),
            r#"{"id":"enabled-locally"}"#,
        )
        .unwrap();
        std::fs::write(current_enabled_pet.join("spritesheet.webp"), [8_u8, 8, 8]).unwrap();
        std::fs::write(
            current_disabled_pet.join("pet.json"),
            r#"{"id":"disabled-locally"}"#,
        )
        .unwrap();
        std::fs::write(current_disabled_pet.join("spritesheet.webp"), [9_u8, 9, 9]).unwrap();

        let (workspace_entries, codex_entries) = runtime
            .block_on(prepare_pet_restore_entries(
                &target_paths,
                &target_settings,
                backup_workspace_entries,
                backup_codex_entries,
            ))
            .unwrap();

        assert!(codex_entries
            .iter()
            .any(|entry| entry["path"] == "new-enabled/pet.json"));
        assert!(codex_entries.iter().all(|entry| !entry["path"]
            .as_str()
            .unwrap_or_default()
            .starts_with("disabled-locally")));
        assert!(!codex_entries.iter().any(|entry| entry
            .get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| path.starts_with("$out"))));
        assert!(workspace_entries
            .iter()
            .any(|entry| entry["path"] == "pets-disabled/new-disabled/pet.json"));
        assert!(workspace_entries.iter().all(|entry| !entry["path"]
            .as_str()
            .unwrap_or_default()
            .starts_with("pets-disabled/enabled-locally")));

        runtime
            .block_on(restore_directory_entries(
                &target_paths,
                workspace_entries
                    .into_iter()
                    .filter(|entry| !is_database_backup_path(entry["path"].as_str().unwrap_or("")))
                    .collect(),
                &Map::new(),
                &std::collections::HashMap::new(),
            ))
            .unwrap();
        runtime
            .block_on(restore_codex_pet_entries(
                &target_settings,
                codex_entries,
                &Map::new(),
            ))
            .unwrap();

        assert_eq!(
            std::fs::read(current_enabled_pet.join("spritesheet.webp")).unwrap(),
            [8_u8, 8, 8]
        );
        assert_eq!(
            std::fs::read(current_disabled_pet.join("spritesheet.webp")).unwrap(),
            [9_u8, 9, 9]
        );
        assert!(!Path::new(&target_paths.disabled_pets_dir)
            .join("enabled-locally")
            .exists());
        assert!(!target_codex_path
            .join("pets")
            .join("disabled-locally")
            .exists());
        assert_eq!(
            std::fs::read(target_codex_path.join("pets/new-enabled/spritesheet.webp")).unwrap(),
            [3_u8, 4, 5]
        );
        assert_eq!(
            std::fs::read(
                Path::new(&target_paths.disabled_pets_dir).join("new-disabled/spritesheet.webp")
            )
            .unwrap(),
            [6_u8, 7, 8]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sqlite_restore_merges_rows_and_keys_for_local_and_cloud_backups() {
        let root =
            std::env::temp_dir().join(format!("ai-manager-merge-restore-{}", uuid::Uuid::new_v4()));
        let source_paths = resolve_app_paths(&root.join("source"));
        provider_store::write_provider_bundle(
            &source_paths,
            &[
                json!({"id": "conflict-a", "cli": "codex", "name": "备份 A"}),
                json!({"id": "conflict-b", "cli": "codex", "name": "备份 B"}),
                json!({"id": "backup-only", "cli": "codex", "name": "新增"}),
                json!({"id": "same", "cli": "codex", "name": "相同"}),
            ],
            &[],
            &[],
            &Map::new(),
        )
        .unwrap();
        let database_content = database::backup(&source_paths).unwrap();
        let backup_keys = json!({"conflict-a": {
            "activeApiKeyId": "backup-key", "apiKeys": [
                {"id": "shared-key", "name": "备份密钥", "note": "", "apiKey": "backup-shared-secret"},
                {"id": "backup-key", "name": "新增密钥", "note": "", "apiKey": "backup-only-secret"}
            ]
        }});
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for (scope, directory) in [(BackupScope::Local, "local"), (BackupScope::Cloud, "cloud")] {
            let target_root = root.join(directory);
            let paths = resolve_app_paths(&target_root);
            let mut settings = normalize_app_settings(
                target_root.join("app-settings.json"),
                Some(json!({"dataPath": target_root})),
            );
            let mut keys = Map::new();
            runtime_provider::set_provider_keys(&mut keys, "conflict-a", &[
                json!({"id": "shared-key", "name": "当前密钥", "note": "", "apiKey": "local-shared-secret"}),
                json!({"id": "local-key", "name": "本机密钥", "note": "", "apiKey": "local-only-secret"})
            ], "local-key".into()).unwrap();
            provider_store::write_provider_bundle(&paths, &[
                json!({"id": "conflict-a", "cli": "codex", "name": "本机 A", "enabled": true}),
                json!({"id": "conflict-b", "cli": "codex", "name": "本机 B", "enabled": false}),
                json!({"id": "local-only", "cli": "codex", "name": "本机独有", "enabled": true}),
                json!({"id": "same", "cli": "codex", "name": "相同", "enabled": true})
            ], &[], &[], &keys).unwrap();
            let backup = encrypt_backup_payload(&json!({
                "version": 1, "createdAt": 1, "workspaceEntries": [{
                    "path": "storage/ai-manager.db", "type": "file",
                    "content": base64::engine::general_purpose::STANDARD.encode(&database_content)
                }], "runtimeProviderKeys": super::encrypt_backup_data(&backup_keys).unwrap()
            }), scope).unwrap();
            let preview = runtime
                .block_on(preview_data_backup_restore_content(
                    &paths, &settings, &backup,
                ))
                .unwrap();
            assert_eq!(preview["conflictCount"], 3);
            assert_eq!(preview["addedCount"], 2);
            for item in preview["conflicts"].as_array().unwrap() {
                assert!(!item.to_string().contains("local-shared-secret"));
                assert!(!item.to_string().contains("backup-shared-secret"));
            }
            runtime
                .block_on(restore_data_backup_content(
                    &paths,
                    &mut settings,
                    &backup,
                    &json!({}),
                ))
                .unwrap();
            let providers = provider_store::read_providers(&paths).unwrap();
            assert_eq!(providers.len(), 5);
            assert_eq!(providers[0]["name"], "本机 A");
            assert_eq!(providers[4]["id"], "backup-only");
            assert_eq!(providers[4]["enabled"], false);
            let mut choices = Map::new();
            choices.insert(
                database::row_choice_key("providers", "conflict-a"),
                json!("backup"),
            );
            choices.insert(
                super::provider_key_choice_key("conflict-a", "shared-key"),
                json!("backup"),
            );
            for _ in 0..2 {
                runtime
                    .block_on(restore_data_backup_content(
                        &paths,
                        &mut settings,
                        &backup,
                        &serde_json::Value::Object(choices.clone()),
                    ))
                    .unwrap();
                let providers = provider_store::read_providers(&paths).unwrap();
                assert_eq!(providers.len(), 5);
                assert_eq!(providers[0]["name"], "备份 A");
                assert_eq!(providers[0]["enabled"], true);
                assert_eq!(providers[1]["name"], "本机 B");
                assert_eq!(providers[1]["enabled"], false);
                assert_eq!(providers[2]["name"], "本机独有");
                assert_eq!(providers[2]["enabled"], true);
                assert_eq!(
                    runtime_provider::get_provider_api_key(&paths, "conflict-a").unwrap(),
                    "local-only-secret"
                );
                let exported = super::export_provider_keys(&paths).unwrap();
                assert_eq!(
                    exported["conflict-a"]["apiKeys"].as_array().unwrap().len(),
                    3
                );
                assert_eq!(
                    exported["conflict-a"]["apiKeys"][0]["apiKey"],
                    "backup-shared-secret"
                );
                assert_eq!(
                    exported["conflict-a"]["apiKeys"][2]["apiKey"],
                    "backup-only-secret"
                );
            }
            let preview = runtime
                .block_on(preview_data_backup_restore_content(
                    &paths, &settings, &backup,
                ))
                .unwrap();
            assert_eq!(preview["addedCount"], 0);
            assert_eq!(preview["conflictCount"], 1);
            assert_eq!(
                preview["conflicts"][0]["key"],
                database::row_choice_key("providers", "conflict-b")
            );
        }
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(resolved).unwrap();
    }

    #[test]
    fn restore_keys_without_provider_table_only_fills_missing_keys() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-provider-key-restore-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        let mut current_keys = Map::new();
        runtime_provider::set_provider_key(
            &mut current_keys,
            "provider-a",
            "current-key".to_string(),
        )
        .unwrap();
        provider_store::write_provider_bundle(
            &paths,
            &[json!({"id": "provider-a"}), json!({"id": "provider-b"})],
            &[],
            &[],
            &current_keys,
        )
        .unwrap();
        let choices = json!({
          "database:storage/ai-manager.db:codex_accounts": "backup"
        })
        .as_object()
        .cloned()
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime
            .block_on(merge_provider_keys(
                &paths,
                &json!({
                  "provider-a": "backup-key-a",
                  "provider-b": {
                    "activeApiKeyId": "key-b2",
                    "apiKeys": [
                      { "id": "key-b1", "name": "主 Key", "note": "生产", "apiKey": "backup-key-b1" },
                      { "id": "key-b2", "name": "备用 Key", "note": "备用", "apiKey": "backup-key-b2" }
                    ]
                  },
                  "provider-c": "backup-key-c"
                }),
                &choices,
            ))
            .unwrap();

        assert_eq!(
            runtime_provider::get_provider_api_key(&paths, "provider-a").unwrap(),
            "current-key"
        );
        assert_eq!(
            runtime_provider::get_provider_api_key(&paths, "provider-b").unwrap(),
            "backup-key-b2"
        );
        assert_eq!(
            runtime_provider::provider_key_records(
                provider_store::read_keys(&paths).unwrap().get("provider-b")
            )[1]["note"],
            "备用"
        );
        assert!(!provider_store::read_keys(&paths)
            .unwrap()
            .contains_key("provider-c"));
    }

    #[test]
    fn legacy_backups_are_reduced_to_the_supported_scope() {
        let entries = sanitize_runtime_backup_entries(vec![
            json!({"path": "profiles/claude-profile.json", "type": "file"}),
            json!({"path": "storage/prompt-runtime-state.json", "type": "file"}),
            json!({"path": "storage/sessions.json", "type": "file"}),
            json!({"path": "storage/usage-pricing.json", "type": "file"}),
            json!({"path": "storage/codex-proxy-config.json", "type": "file"}),
            json!({"path": "logs/usage.db", "type": "file"}),
            json!({"path": "git-tool/project/archive.git", "type": "file"}),
            json!({"path": "prompts/common/rule-a.md", "type": "file"}),
            encoded_json_entry(
                "storage/providers.json",
                &json!([{"id": "provider-a", "enabled": true}]),
            ),
            encoded_json_entry(
                "storage/skills.json",
                &json!([{
                  "name": "skill-a",
                  "disabled": false,
                  "installedTargets": ["codex"],
                  "status": "installed"
                }]),
            ),
            encoded_json_entry(
                "storage/codex-accounts.json",
                &json!([{"id": "account-a", "usage": {"limit": 1}, "disabled": false}]),
            ),
        ])
        .unwrap();

        assert_eq!(entries.len(), 4);
        let provider = decode_json_entry(
            entries
                .iter()
                .find(|entry| entry["path"] == "storage/providers.json")
                .unwrap(),
        );
        let skill = decode_json_entry(
            entries
                .iter()
                .find(|entry| entry["path"] == "storage/skills.json")
                .unwrap(),
        );
        let account = decode_json_entry(
            entries
                .iter()
                .find(|entry| entry["path"] == "storage/codex-accounts.json")
                .unwrap(),
        );

        assert!(provider[0].get("enabled").is_none());
        assert!(skill[0].get("disabled").is_none());
        assert!(skill[0].get("installedTargets").is_none());
        assert!(skill[0].get("status").is_none());
        assert!(account[0].get("usage").is_none());
        assert!(account[0].get("disabled").is_none());
    }

    #[test]
    fn legacy_provider_restore_preserves_current_enabled_state() {
        let choices = json!({
          "json:storage/providers.json:provider-a": "backup"
        })
        .as_object()
        .cloned()
        .unwrap();
        let merged = merge_json_backup_value(
            "storage/providers.json",
            &json!([{"id": "provider-a", "name": "current"}]),
            &json!([
              {"id": "provider-a", "name": "backup"},
              {"id": "provider-b", "name": "new"}
            ]),
            &choices,
        )
        .unwrap();

        assert_eq!(merged[0]["name"], "backup");
        assert_eq!(merged[0]["enabled"], true);
        assert_eq!(merged[1]["enabled"], false);
    }

    #[test]
    fn legacy_skill_restore_preserves_current_state_and_disables_new_items() {
        let choices = json!({
          "json:storage/skills.json:skill-a": "backup"
        })
        .as_object()
        .cloned()
        .unwrap();
        let merged = merge_json_backup_value(
            "storage/skills.json",
            &json!([{
              "name": "skill-a",
              "description": "current",
              "disabled": false,
              "installedTargets": ["codex"],
              "status": "installed"
            }]),
            &json!([
              {"name": "skill-a", "description": "backup", "disabled": true},
              {"name": "skill-b", "description": "new", "disabled": false}
            ]),
            &choices,
        )
        .unwrap();

        assert_eq!(merged[0]["description"], "backup");
        assert_eq!(merged[0]["disabled"], false);
        assert_eq!(merged[0]["installedTargets"], json!(["codex"]));
        assert_eq!(merged[0]["status"], "installed");
        assert_eq!(merged[1]["disabled"], true);
        assert_eq!(merged[1]["installedTargets"], json!([]));
        assert_eq!(merged[1]["status"], "disabled");
    }

    #[test]
    fn legacy_codex_account_restore_preserves_current_disabled_state() {
        let choices = json!({
          "json:storage/codex-accounts.json:account-a": "backup"
        })
        .as_object()
        .cloned()
        .unwrap();
        let merged = merge_json_backup_value(
            "storage/codex-accounts.json",
            &json!([{"id": "account-a", "email": "current@example.com", "disabled": true}]),
            &json!([
              {"id": "account-a", "email": "backup@example.com", "disabled": false},
              {"id": "account-b", "email": "new@example.com", "disabled": false}
            ]),
            &choices,
        )
        .unwrap();

        assert_eq!(merged[0]["email"], "backup@example.com");
        assert_eq!(merged[0]["disabled"], true);
        assert_eq!(merged[1]["disabled"], true);
    }

    #[test]
    fn legacy_json_restore_reads_current_state_from_sqlite() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-legacy-json-sqlite-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        let mut app_settings = normalize_app_settings(
            root.join("app-settings.json"),
            Some(json!({"dataPath": root.to_string_lossy()})),
        );
        provider_store::write_provider_bundle(
            &paths,
            &[json!({"id": "provider-a", "cli": "codex", "name": "current"})],
            &[],
            &[json!({"id": "codex", "cli": "codex", "providerId": "provider-a"})],
            &Map::new(),
        )
        .unwrap();
        provider_store::write_runtime_state(
            &paths,
            &Map::from_iter([(
                "codex".to_string(),
                json!({"activeProviderId": "provider-a", "status": "SYNCED"}),
            )]),
        )
        .unwrap();
        provider_store::write_codex_accounts(
            &paths,
            &[json!({
              "id": "account-a",
              "email": "current@example.com",
              "usage": {"source": "current"},
              "disabled": true
            })],
        )
        .unwrap();
        skill_store::write_skills(
            &paths,
            &[json!({
              "id": "skill-id-a",
              "name": "skill-a",
              "description": "current",
              "disabled": false,
              "installedTargets": ["codex"],
              "installStates": {"codex": {"state": "installed"}},
              "status": "installed"
            })],
        )
        .unwrap();
        skill_store::write_installs(
            &paths,
            &Map::from_iter([("skill-a".to_string(), json!(["codex"]))]),
        )
        .unwrap();
        let backup = encrypt_backup_payload(&json!({
          "version": 1,
          "createdAt": 1,
          "workspaceEntries": [
            encoded_json_entry(
              "storage/providers.json",
              &json!([
                {"id": "provider-a", "cli": "claude", "name": "backup", "enabled": false},
                {"id": "provider-b", "cli": "codex", "name": "new", "enabled": true}
              ])
            ),
            encoded_json_entry(
              "storage/skills.json",
              &json!([
                {"id": "skill-id-a", "name": "skill-a", "description": "backup", "disabled": true},
                {"id": "skill-id-b", "name": "skill-b", "description": "new", "disabled": false}
              ])
            ),
            encoded_json_entry(
              "storage/codex-accounts.json",
              &json!([
                {"id": "account-a", "email": "backup@example.com", "usage": {"source": "backup"}, "disabled": false},
                {"id": "account-b", "email": "new@example.com", "usage": {"source": "backup"}, "disabled": false}
              ])
            )
          ]
        }), BackupScope::Local)
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let preview = runtime
            .block_on(preview_data_backup_restore_content(
                &paths,
                &app_settings,
                &backup,
            ))
            .unwrap();
        let conflict_keys = preview["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["key"].as_str())
            .collect::<Vec<_>>();

        assert!(conflict_keys.contains(&"json:storage/providers.json:provider-a"));
        assert!(conflict_keys.contains(&"json:storage/skills.json:skill-a"));
        assert!(conflict_keys.contains(&"json:storage/codex-accounts.json:account-a"));

        runtime
            .block_on(restore_data_backup_content(
                &paths,
                &mut app_settings,
                &backup,
                &json!({
                  "json:storage/providers.json:provider-a": "backup",
                  "json:storage/skills.json:skill-a": "backup",
                  "json:storage/codex-accounts.json:account-a": "backup"
                }),
            ))
            .unwrap();

        let providers = provider_store::read_providers(&paths).unwrap();
        let provider_a = providers
            .iter()
            .find(|item| item["id"] == "provider-a")
            .unwrap();
        let provider_b = providers
            .iter()
            .find(|item| item["id"] == "provider-b")
            .unwrap();
        let skills = skill_store::read_skills(&paths).unwrap();
        let skill_a = skills
            .iter()
            .find(|item| item["name"] == "skill-a")
            .unwrap();
        let skill_b = skills
            .iter()
            .find(|item| item["name"] == "skill-b")
            .unwrap();
        let accounts = provider_store::read_codex_accounts(&paths).unwrap();
        let account_a = accounts
            .iter()
            .find(|item| item["id"] == "account-a")
            .unwrap();
        let account_b = accounts
            .iter()
            .find(|item| item["id"] == "account-b")
            .unwrap();
        let runtime_state = provider_store::read_runtime_state(&paths).unwrap();
        let installs = skill_store::read_installs(&paths).unwrap();

        assert_eq!(provider_a["name"], "backup");
        assert_eq!(provider_a["enabled"], true);
        assert_eq!(provider_b["enabled"], false);
        assert!(provider_store::read_profiles(&paths).unwrap().is_empty());
        assert_eq!(runtime_state["codex"]["activeProviderId"], "");
        assert_eq!(skill_a["description"], "backup");
        assert_eq!(skill_a["disabled"], false);
        assert_eq!(skill_a["installedTargets"], json!(["codex"]));
        assert_eq!(skill_a["installStates"]["codex"]["state"], "installed");
        assert_eq!(skill_a["status"], "installed");
        assert_eq!(installs["skill-a"], json!(["codex"]));
        assert_eq!(skill_b["disabled"], true);
        assert_eq!(skill_b["installedTargets"], json!([]));
        assert_eq!(skill_b["installStates"], json!({}));
        assert_eq!(skill_b["status"], "disabled");
        assert!(!installs.contains_key("skill-b"));
        assert_eq!(account_a["email"], "backup@example.com");
        assert_eq!(account_a["usage"]["source"], "current");
        assert_eq!(account_a["disabled"], true);
        assert!(account_b.get("usage").is_none());
        assert_eq!(account_b["disabled"], true);
    }

    #[test]
    fn backup_and_restore_cloud_sync_settings() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-app-settings-backup-{}",
            std::process::id()
        ));
        let mut app_settings = normalize_app_settings(
            root.join("app-settings.json"),
            Some(json!({
              "dataPath": root.join("current-data").to_string_lossy(),
              "cliConfigPaths": {"claude": "current-claude", "codex": "current-codex"},
              "cloudSync": {
                "webdavUrl": "https://current.example/dav",
                "username": "current-user",
                "password": "current-password",
                "fileName": "current.aimbackup",
                "lastUpdatedAt": 900
              },
              "koofrSync": {
                "webdavUrl": "https://current-koofr.example/dav",
                "username": "current-koofr-user",
                "password": "current-koofr-password",
                "fileName": "current-koofr.aimbackup",
                "lastUpdatedAt": 800
              },
              "localBackup": {"enabled": false, "intervalMinutes": 30, "maxCount": 3},
              "system": {"closeAction": "quit", "quickSwitchVisible": false}
            })),
        );
        let serialized = serialize_backup_app_settings(&app_settings);

        assert_eq!(
            serialized.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["cloudSync", "koofrSync"]
        );
        assert_eq!(
            serialized["cloudSync"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["fileName", "password", "provider", "username", "webdavUrl"]
        );
        assert!(serialized["cloudSync"].get("lastUpdatedAt").is_none());
        assert_eq!(serialized["koofrSync"]["provider"], "koofr");
        assert!(serialized["koofrSync"].get("lastUpdatedAt").is_none());

        let original_data_path = app_settings.data_path.clone();
        let original_local_backup_enabled = app_settings.local_backup.enabled;
        let original_close_action = app_settings.system.close_action.clone();
        let choices = json!({
          "json:app-settings.json:cloudSync": "backup",
          "json:app-settings.json:koofrSync": "backup"
        })
        .as_object()
        .cloned()
        .unwrap();
        restore_backup_app_settings(
            &mut app_settings,
            &json!({
              "appSettings": {
                "dataPath": "ignored-data-path",
                "localBackup": {"enabled": true},
                "system": {"closeAction": "minimize"},
                "cloudSync": {
                  "webdavUrl": "https://backup.example/dav",
                  "username": "backup-user",
                  "password": "backup-password",
                  "fileName": "backup.aimbackup",
                  "lastUpdatedAt": 1
                },
                "koofrSync": {
                  "webdavUrl": "https://backup-koofr.example/dav",
                  "username": "backup-koofr-user",
                  "password": "backup-koofr-password",
                  "fileName": "backup-koofr.aimbackup",
                  "lastUpdatedAt": 2
                }
              }
            }),
            &choices,
        )
        .unwrap();

        assert_eq!(
            app_settings.cloud_sync.webdav_url,
            "https://backup.example/dav"
        );
        assert_eq!(app_settings.cloud_sync.username, "backup-user");
        assert_eq!(app_settings.cloud_sync.password, "backup-password");
        assert_eq!(app_settings.cloud_sync.file_name, "backup.aimbackup");
        assert_eq!(app_settings.cloud_sync.last_updated_at, 900);
        assert_eq!(
            app_settings.koofr_sync.webdav_url,
            "https://backup-koofr.example/dav"
        );
        assert_eq!(app_settings.koofr_sync.username, "backup-koofr-user");
        assert_eq!(app_settings.koofr_sync.password, "backup-koofr-password");
        assert_eq!(
            app_settings.koofr_sync.file_name,
            "backup-koofr.aimbackup"
        );
        assert_eq!(app_settings.koofr_sync.last_updated_at, 800);
        assert_eq!(app_settings.data_path, original_data_path);
        assert_eq!(
            app_settings.local_backup.enabled,
            original_local_backup_enabled
        );
        assert_eq!(app_settings.system.close_action, original_close_action);
    }

    #[test]
    fn old_jianguoyun_backup_preserves_koofr_settings() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-old-cloud-sync-backup-{}",
            std::process::id()
        ));
        let mut app_settings = normalize_app_settings(
            root.join("app-settings.json"),
            Some(json!({
              "cloudSync": {"username": "current-user"},
              "koofrSync": {
                "username": "current-koofr-user",
                "password": "current-koofr-password"
              }
            })),
        );
        let choices = json!({
          "json:app-settings.json:cloudSync": "backup"
        })
        .as_object()
        .cloned()
        .unwrap();

        restore_backup_app_settings(
            &mut app_settings,
            &json!({
              "appSettings": {
                "cloudSync": {"username": "backup-user"}
              }
            }),
            &choices,
        )
        .unwrap();

        assert_eq!(app_settings.cloud_sync.username, "backup-user");
        assert_eq!(app_settings.koofr_sync.username, "current-koofr-user");
        assert_eq!(
            app_settings.koofr_sync.password,
            "current-koofr-password"
        );
    }

    #[test]
    fn cloud_sync_preview_and_inspection_hide_passwords() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-app-settings-preview-{}",
            std::process::id()
        ));
        let app_settings = normalize_app_settings(
            root.join("app-settings.json"),
            Some(json!({
              "cloudSync": {
                "username": "current-user",
                "password": "current-secret"
              },
              "koofrSync": {
                "username": "current-koofr-user",
                "password": "current-koofr-secret"
              }
            })),
        );
        let mut preview = json!({
          "added": [],
          "conflicts": [],
          "addedCount": 0,
          "conflictCount": 0
        });
        append_app_settings_restore_preview(
            &mut preview,
            &app_settings,
            &json!({
              "appSettings": {
                "cloudSync": {
                  "username": "backup-user",
                  "password": "backup-secret"
                },
                "koofrSync": {
                  "username": "backup-koofr-user",
                  "password": "backup-koofr-secret"
                }
              }
            }),
        )
        .unwrap();
        let preview_text = serde_json::to_string(&preview).unwrap();
        let inspected_text = serde_json::to_string(&redact_backup_app_settings(json!({
          "cloudSync": {"password": "backup-secret"},
          "koofrSync": {"password": "backup-koofr-secret"}
        })))
        .unwrap();

        assert_eq!(preview["conflictCount"], 2);
        assert_eq!(
            preview["conflicts"][0]["key"],
            "json:app-settings.json:cloudSync"
        );
        assert_eq!(
            preview["conflicts"][1]["key"],
            "json:app-settings.json:koofrSync"
        );
        assert!(!preview_text.contains("current-secret"));
        assert!(!preview_text.contains("backup-secret"));
        assert!(!preview_text.contains("current-koofr-secret"));
        assert!(!preview_text.contains("backup-koofr-secret"));
        assert!(!inspected_text.contains("backup-secret"));
        assert!(!inspected_text.contains("backup-koofr-secret"));
        assert_eq!(inspected_text.matches("********").count(), 2);
    }

    fn encoded_json_entry(path: &str, value: &serde_json::Value) -> serde_json::Value {
        json!({
          "path": path,
          "type": "file",
          "content": base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_string(value).unwrap())
        })
    }

    fn decode_json_entry(entry: &serde_json::Value) -> serde_json::Value {
        serde_json::from_slice(
            &base64::engine::general_purpose::STANDARD
                .decode(entry["content"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap()
    }
}

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
    let mut backup_settings = app_settings.clone();

    set_cloud_sync_settings(&mut backup_settings, updated_cloud_sync.clone());
    upload_webdav_backup(
        &cloud_sync,
        create_data_backup(paths, &backup_settings).await?,
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
      "preview": preview_data_backup_restore_content(paths, &preview_settings, &content).await?
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
    let provider_keys = export_provider_keys(paths)?;

    encrypt_backup_payload(&json!({
      "version": 1,
      "createdAt": now_millis(),
      "appSettings": serialize_backup_app_settings(app_settings),
      "workspaceEntries": collect_backup_entries(paths).await?,
      "codexPetEntries": collect_codex_pet_entries(app_settings).await?,
      "runtimeProviderKeys": encrypt_backup_data(&provider_keys)?
    }))
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
    let mut preview = create_restore_preview(
        paths,
        backup["workspaceEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    )
    .await?;
    append_app_settings_restore_preview(&mut preview, app_settings, &backup)?;
    append_codex_pets_restore_preview(&mut preview, app_settings, &backup).await?;

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
    backup: &Value,
) -> Result<(), ManagerError> {
    let Some(codex_pets_dir) = codex_pets_backup_dir(app_settings) else {
        return Ok(());
    };
    let root_path = path_text(&codex_pets_dir);
    let mut added = preview["added"].as_array().cloned().unwrap_or_default();
    let mut conflicts = preview["conflicts"].as_array().cloned().unwrap_or_default();

    for entry in backup["codexPetEntries"]
        .as_array()
        .cloned()
        .unwrap_or_default()
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
    let workspace_entries = backup["workspaceEntries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
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
        || choice_text(
            &choices,
            &create_restore_database_table_key("storage/ai-manager.db", "skills"),
        ) == "backup";
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
    restore_codex_pet_entries(
        app_settings,
        backup["codexPetEntries"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        &choices,
    )
    .await?;
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

async fn collect_backup_entries(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
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

// 启用宠物由 Codex 直接读取，因此单独保存并恢复到当前机器的 Codex 配置目录。
async fn collect_codex_pet_entries(app_settings: &AppSettings) -> Result<Vec<Value>, ManagerError> {
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
        for mut entry in collect_directory_entries(&pet_dir).await? {
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
        let relative_path =
            path_text(child_path.strip_prefix(root_path).unwrap_or(&child_path)).replace('\\', "/");
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
            Box::pin(collect_directory_entries_inner(
                root_path,
                &child_path,
                entries,
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

            for difference in database::preview_restore(paths, &backup_content)? {
                conflicts.push(create_database_table_restore_preview(
                    &entry_path,
                    &difference,
                ));
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
    let selected_tables = database::preview_restore(paths, &backup_content)?
        .into_iter()
        .filter(|difference| {
            choice_text(
                choices,
                &create_restore_database_table_key(entry_path, &difference.table),
            ) == "backup"
        })
        .map(|difference| difference.table)
        .collect::<Vec<_>>();

    if !selected_tables.is_empty() {
        database::restore_selected(paths, &backup_content, &selected_tables)?;
    }
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
                    if ["storage/providers.json", "storage/skills.json"].contains(&entry_path) {
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

        if let Some(usage) = current_value.get("usage") {
            next_backup_value.insert("usage".to_string(), usage.clone());
        } else {
            next_backup_value.remove("usage");
        }
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

    let actual_size = response.content_length().ok_or_else(|| {
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

async fn download_webdav_backup(config: &CloudSyncSettings) -> Result<String, ManagerError> {
    let response = reqwest::Client::new()
        .get(build_webdav_file_url(config)?)
        .header(AUTHORIZATION, build_webdav_auth_header(config))
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let status = response.status().as_u16();

    if status == 404 {
        return Err(ManagerError::System(format!(
            "{}上未找到配置备份",
            cloud_sync_provider_name(config)
        )));
    }

    if status != 200 {
        let detail = read_webdav_error_detail(response, config).await?;

        return Err(ManagerError::System(format!(
            "{}下载失败：{}{}",
            cloud_sync_provider_name(config),
            status,
            detail
        )));
    }

    response
        .text()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))
}

async fn read_webdav_error_detail(
    response: reqwest::Response,
    config: &CloudSyncSettings,
) -> Result<String, ManagerError> {
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
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

fn encrypt_backup_payload(payload: &Value) -> Result<String, ManagerError> {
    let mut iv = [0u8; 12];

    getrandom::getrandom(&mut iv).map_err(|error| ManagerError::System(error.to_string()))?;

    let secret = backup_secret();
    let cipher = Aes256Gcm::new_from_slice(&secret)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&iv),
            serde_json::to_string(payload)?.as_bytes(),
        )
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
    let payload: Value = serde_json::from_str(content).map_err(|error| {
        if error.is_eof() {
            ManagerError::System("备份文件不完整，请重新生成或上传备份".to_string())
        } else {
            ManagerError::Json(error)
        }
    })?;
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
    let providers = provider_store::read_providers(paths)?;
    let keys = provider_store::read_keys(paths)?;
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
    let mut next_keys = provider_store::read_keys(paths)?;
    let provider_ids = provider_store::read_providers(paths)?
        .into_iter()
        .map(|provider| string_value(provider.get("id")))
        .filter(|provider_id| !provider_id.is_empty())
        .collect::<HashSet<_>>();
    let uses_database_choices = choices
        .keys()
        .any(|key| key.starts_with("database:storage/ai-manager.db:"));
    let restore_provider_table = choice_text(
        choices,
        &create_restore_database_table_key("storage/ai-manager.db", "providers"),
    ) == "backup";

    for (provider_id, api_key) in api_keys.as_object().cloned().unwrap_or_default() {
        let key = string_value(Some(&api_key));

        if key.is_empty() {
            continue;
        }

        // 保留当前 Provider 表时只补缺失密钥，不能覆盖当前设备已经保存的密钥。
        if uses_database_choices
            && !restore_provider_table
            && (next_keys.contains_key(&provider_id) || !provider_ids.contains(&provider_id))
        {
            continue;
        }
        if !uses_database_choices
            && next_keys.contains_key(&provider_id)
            && choice_text(
                choices,
                &create_restore_choice_key("storage/providers.json", &provider_id),
            ) != "backup"
        {
            continue;
        }

        runtime_provider::set_provider_key(&mut next_keys, &provider_id, key)?;
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

fn create_database_table_restore_preview(
    entry_path: &str,
    difference: &database::RestoreTableDifference,
) -> Value {
    let name = restore_database_table_name(&difference.table);

    json!({
      "key": create_restore_database_table_key(entry_path, &difference.table),
      "type": "数据库表",
      "name": format!("{name} ({})", difference.table),
      "path": format!("{entry_path}/{}", difference.table),
      "groupPath": entry_path,
      "status": "conflict",
      "currentContent": format!(
          "数据表：{name}\n记录数：{}\n仅当前存在或内容不同：{}",
          difference.current_rows,
          difference.current_only_rows
      ),
      "backupContent": format!(
          "数据表：{name}\n记录数：{}\n仅备份存在或内容不同：{}",
          difference.backup_rows,
          difference.backup_only_rows
      )
    })
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

fn create_restore_database_table_key(entry_path: &str, table: &str) -> String {
    format!("database:{entry_path}:{table}")
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
        merge_json_backup_value,
        merge_provider_keys, preview_data_backup_restore_content, redact_backup_app_settings,
        restore_backup_app_settings, restore_codex_pet_entries, restore_data_backup_content,
        restore_directory_entries, sanitize_runtime_backup_entries, serialize_backup_app_settings,
        validate_backup_symlink_target,
    };
    use crate::api::runtime_provider;
    use crate::core::paths::resolve_app_paths;
    use crate::core::settings::normalize_app_settings;
    use crate::core::usage_store::{self, UsageSessionUpdate};
    use crate::core::{database, provider_store, skill_store};
    use base64::Engine;
    use serde_json::{json, Map};
    use std::path::Path;

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
        let entries = runtime.block_on(collect_backup_entries(&paths)).unwrap();
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
        assert!(restore_tables
            .iter()
            .all(|table| !table.starts_with("usage_")));
        assert_eq!(restore_tables, vec!["providers".to_string()]);
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
    fn backs_up_and_restores_enabled_and_disabled_codex_pets() {
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
        let enabled_pet = source_codex_path.join("pets").join("enabled-pet");
        let invalid_pet = source_codex_path.join("pets").join("$out");
        let disabled_pet = Path::new(&source_paths.disabled_pets_dir).join("disabled-pet");
        std::fs::create_dir_all(&enabled_pet).unwrap();
        std::fs::create_dir_all(&invalid_pet).unwrap();
        std::fs::create_dir_all(&disabled_pet).unwrap();
        std::fs::write(enabled_pet.join("pet.json"), r#"{"id":"enabled-pet"}"#).unwrap();
        std::fs::write(enabled_pet.join("spritesheet.webp"), [1_u8, 2, 3]).unwrap();
        std::fs::write(invalid_pet.join("build.js"), [7_u8, 8, 9]).unwrap();
        std::fs::write(disabled_pet.join("pet.json"), r#"{"id":"disabled-pet"}"#).unwrap();
        std::fs::write(disabled_pet.join("spritesheet.webp"), [4_u8, 5, 6]).unwrap();

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
        let codex_entries = runtime
            .block_on(collect_codex_pet_entries(&source_settings))
            .unwrap();
        let workspace_entries = runtime
            .block_on(collect_backup_entries(&source_paths))
            .unwrap();

        assert!(codex_entries
            .iter()
            .any(|entry| entry["path"] == "enabled-pet/pet.json"));
        assert!(codex_entries
            .iter()
            .any(|entry| entry["path"] == "enabled-pet/spritesheet.webp"));
        assert!(!codex_entries.iter().any(|entry| entry
            .get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| path.starts_with("$out"))));
        assert!(workspace_entries
            .iter()
            .any(|entry| entry["path"] == "pets-disabled/disabled-pet/pet.json"));

        runtime
            .block_on(restore_codex_pet_entries(
                &target_settings,
                codex_entries,
                &Map::new(),
            ))
            .unwrap();
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

        assert_eq!(
            std::fs::read(target_codex_path.join("pets/enabled-pet/spritesheet.webp")).unwrap(),
            [1_u8, 2, 3]
        );
        assert_eq!(
            std::fs::read(
                Path::new(&target_paths.disabled_pets_dir).join("disabled-pet/spritesheet.webp")
            )
            .unwrap(),
            [4_u8, 5, 6]
        );

        let _ = std::fs::remove_dir_all(root);
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
                  "provider-b": "backup-key-b",
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
            "backup-key-b"
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
                &json!([{"id": "account-a", "usage": {"limit": 1}}]),
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
              "usage": {"source": "current"}
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
                {"id": "account-a", "email": "backup@example.com", "usage": {"source": "backup"}},
                {"id": "account-b", "email": "new@example.com", "usage": {"source": "backup"}}
              ])
            )
          ]
        }))
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
        assert!(account_b.get("usage").is_none());
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

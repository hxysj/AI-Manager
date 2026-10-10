use crate::core::error::ManagerError;
use crate::core::paths::{home_path, path_text, portable_home_prefix, DEFAULT_USER_DATA_PATH};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncSettings {
    pub provider: String,
    pub webdav_url: String,
    pub username: String,
    pub password: String,
    pub file_name: String,
    pub last_updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalBackupSettings {
    pub enabled: bool,
    pub interval_minutes: u64,
    pub max_count: u64,
    pub last_backup_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    pub close_action: String,
    pub quick_switch_visible: bool,
    #[serde(default)]
    pub quick_switch_tools: BTreeMap<String, bool>,
    pub auto_launch_enabled: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopSettings {
    pub enabled: bool,
    pub pet_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationAgentSettings {
    pub enabled: bool,
    pub target_id: String,
    pub model: String,
    pub target_language: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettings {
    pub translation: TranslationAgentSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub data_path: String,
    pub default_data_path: String,
    pub settings_file_path: String,
    pub cli_config_paths: Value,
    pub default_cli_config_paths: Value,
    pub cloud_sync: CloudSyncSettings,
    pub koofr_sync: CloudSyncSettings,
    pub local_backup: LocalBackupSettings,
    pub system: SystemSettings,
    #[serde(default)]
    pub desktop: DesktopSettings,
    pub agents: AgentSettings,
    pub restart_required: bool,
}

pub fn load_app_settings(settings_file_path: PathBuf) -> Result<AppSettings, ManagerError> {
    if !settings_file_path.exists() {
        return Ok(normalize_app_settings(settings_file_path, None));
    }

    let content = std::fs::read_to_string(&settings_file_path)?;
    let payload = serde_json::from_str(&content)?;
    Ok(normalize_app_settings(settings_file_path, Some(payload)))
}

pub fn normalize_app_settings(settings_file_path: PathBuf, payload: Option<Value>) -> AppSettings {
    let input = payload.unwrap_or_else(|| json!({}));
    let portable_home_prefix = portable_home_prefix();
    let default_cli_config_paths = json!({
      "claude": path_text(portable_home_prefix.join(".claude")),
      "codex": path_text(portable_home_prefix.join(".codex"))
    });
    let cli_config_paths = input
        .get("cliConfigPaths")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let cloud_sync = input.get("cloudSync").cloned().unwrap_or_else(|| json!({}));
    let koofr_sync = input.get("koofrSync").cloned().unwrap_or_else(|| json!({}));
    let local_backup = input
        .get("localBackup")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let system = input.get("system").cloned().unwrap_or_else(|| json!({}));
    let desktop = &input["desktop"];
    let translation = &input["agents"]["translation"];

    AppSettings {
        data_path: resolve_portable_path(
            input
                .get("dataPath")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_USER_DATA_PATH),
        ),
        default_data_path: DEFAULT_USER_DATA_PATH.to_string(),
        settings_file_path: path_text(settings_file_path),
        cli_config_paths: json!({
          "claude": resolve_portable_path(
            cli_config_paths
              .get("claude")
              .and_then(Value::as_str)
              .unwrap_or(default_cli_config_paths["claude"].as_str().unwrap_or(""))
          ),
          "codex": resolve_portable_path(
            cli_config_paths
              .get("codex")
              .and_then(Value::as_str)
              .unwrap_or(default_cli_config_paths["codex"].as_str().unwrap_or(""))
          )
        }),
        default_cli_config_paths,
        cloud_sync: normalize_cloud_sync_settings(&cloud_sync, "jianguoyun"),
        koofr_sync: normalize_cloud_sync_settings(&koofr_sync, "koofr"),
        local_backup: LocalBackupSettings {
            enabled: bool_value(local_backup.get("enabled"), true),
            interval_minutes: number_value(local_backup.get("intervalMinutes"), 60).max(1),
            max_count: number_value(local_backup.get("maxCount"), 20).max(1),
            last_backup_at: number_value(local_backup.get("lastBackupAt"), 0),
        },
        system: SystemSettings {
            close_action: normalize_close_action(system.get("closeAction")),
            quick_switch_visible: bool_value(system.get("quickSwitchVisible"), true),
            quick_switch_tools: system
                .get("quickSwitchTools")
                .and_then(Value::as_object)
                .map(|tools| {
                    tools
                        .iter()
                        .filter_map(|(id, value)| {
                            value.as_bool().map(|visible| (id.clone(), visible))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            auto_launch_enabled: bool_value(system.get("autoLaunchEnabled"), false),
        },
        desktop: DesktopSettings {
            enabled: bool_value(desktop.get("enabled"), false),
            pet_id: string_value(desktop.get("petId")),
        },
        agents: AgentSettings {
            translation: TranslationAgentSettings {
                enabled: bool_value(translation.get("enabled"), false),
                target_id: string_value(translation.get("targetId")),
                model: string_value(translation.get("model")),
                target_language: non_empty_string(translation.get("targetLanguage"), "简体中文"),
            },
        },
        restart_required: false,
    }
}

pub fn normalize_cloud_sync_settings(input: &Value, provider: &str) -> CloudSyncSettings {
    let (provider, default_url) = match provider {
        "koofr" => ("koofr", "https://app.koofr.net/dav/Koofr/AI-Manager"),
        _ => ("jianguoyun", "https://dav.jianguoyun.com/dav/AI-Manager"),
    };

    CloudSyncSettings {
        provider: provider.to_string(),
        webdav_url: non_empty_string(input.get("webdavUrl"), default_url),
        username: string_value(input.get("username")),
        password: string_value(input.get("password")),
        file_name: non_empty_string(input.get("fileName"), "ai-manager.aimbackup"),
        last_updated_at: number_value(input.get("lastUpdatedAt"), 0),
    }
}

pub fn serialize_app_settings(app_settings: &AppSettings) -> Value {
    let mut payload = serde_json::to_value(app_settings).unwrap_or_else(|_| json!({}));

    payload["dataPath"] = json!(serialize_portable_path(&app_settings.data_path));
    payload["cliConfigPaths"] = json!({
      "claude": serialize_portable_path(
        app_settings
          .cli_config_paths
          .get("claude")
          .and_then(Value::as_str)
          .unwrap_or("")
      ),
      "codex": serialize_portable_path(
        app_settings
          .cli_config_paths
          .get("codex")
          .and_then(Value::as_str)
          .unwrap_or("")
      )
    });
    payload["defaultCliConfigPaths"] = json!({
      "claude": serialize_portable_path(
        app_settings
          .default_cli_config_paths
          .get("claude")
          .and_then(Value::as_str)
          .unwrap_or("")
      ),
      "codex": serialize_portable_path(
        app_settings
          .default_cli_config_paths
          .get("codex")
          .and_then(Value::as_str)
          .unwrap_or("")
      )
    });

    payload
}

#[cfg(test)]
mod floating_tools_tests {
    use super::*;

    #[test]
    fn defaults_and_tool_visibility_survive_round_trip() {
        let path = PathBuf::from("settings.json");
        let defaults = normalize_app_settings(path.clone(), Some(json!({})));
        assert!(defaults.system.quick_switch_visible);
        assert!(defaults.system.quick_switch_tools.is_empty());

        let settings = normalize_app_settings(
            path.clone(),
            Some(json!({
                "system": { "quickSwitchTools": { "git": false, "string-diff": true, "future-tool": false } }
            })),
        );
        let restored = normalize_app_settings(path, Some(serialize_app_settings(&settings)));
        assert_eq!(
            restored.system.quick_switch_tools,
            settings.system.quick_switch_tools
        );
        assert_eq!(restored.system.quick_switch_tools.get("git"), Some(&false));
        assert_eq!(
            restored.system.quick_switch_tools.get("image-workbench"),
            None
        );
    }

    #[test]
    fn desktop_pet_defaults_and_last_selection_survive_round_trip() {
        let path = PathBuf::from("settings.json");
        let defaults = normalize_app_settings(path.clone(), None);
        assert!(!defaults.desktop.enabled);
        assert!(defaults.desktop.pet_id.is_empty());

        for enabled in [true, false] {
            let settings = normalize_app_settings(
                path.clone(),
                Some(json!({ "desktop": { "enabled": enabled, "petId": "monkey_animations" } })),
            );
            let restored =
                normalize_app_settings(path.clone(), Some(serialize_app_settings(&settings)));
            assert_eq!(restored.desktop.enabled, enabled);
            assert_eq!(restored.desktop.pet_id, "monkey_animations");
        }
        let mut old_payload = serialize_app_settings(&defaults);
        old_payload.as_object_mut().unwrap().remove("desktop");
        let restored: AppSettings = serde_json::from_value(old_payload).unwrap();
        assert!(!restored.desktop.enabled);
        assert!(restored.desktop.pet_id.is_empty());
    }
}

pub fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn non_empty_string(value: Option<&Value>, fallback: &str) -> String {
    let text = string_value(value);

    if text.is_empty() {
        fallback.to_string()
    } else {
        text
    }
}

pub fn number_value(value: Option<&Value>, fallback: u64) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(fallback)
}

pub fn bool_value(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn normalize_close_action(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str).unwrap_or("ask") {
        "minimize" => "minimize".to_string(),
        "quit" => "quit".to_string(),
        _ => "ask".to_string(),
    }
}

pub(crate) fn resolve_portable_path(value: &str) -> String {
    let text = value.trim();

    if text.is_empty() {
        return String::new();
    }

    let actual_prefix = path_text(home_path());
    let portable_prefix = path_text(portable_home_prefix());
    let normalized_text = text.replace('/', "\\");
    let normalized_portable = portable_prefix.replace('/', "\\");

    if same_path_text(&normalized_text, &normalized_portable) {
        return actual_prefix;
    }

    let portable_child_prefix = format!("{}\\", normalized_portable);

    if starts_with_path_text(&normalized_text, &portable_child_prefix) {
        return PathBuf::from(&actual_prefix)
            .join(&normalized_text[portable_child_prefix.len()..])
            .to_string_lossy()
            .to_string();
    }

    text.to_string()
}

pub(crate) fn serialize_portable_path(value: &str) -> String {
    let text = value.trim();

    if text.is_empty() {
        return String::new();
    }

    let actual_prefix = path_text(home_path());
    let portable_prefix = path_text(portable_home_prefix());
    let normalized_text = text.replace('/', "\\");
    let normalized_actual = actual_prefix.replace('/', "\\");

    if same_path_text(&normalized_text, &normalized_actual) {
        return portable_prefix;
    }

    let actual_child_prefix = format!("{}\\", normalized_actual);

    if starts_with_path_text(&normalized_text, &actual_child_prefix) {
        return PathBuf::from(&portable_prefix)
            .join(&normalized_text[actual_child_prefix.len()..])
            .to_string_lossy()
            .to_string();
    }

    text.to_string()
}

fn same_path_text(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn starts_with_path_text(value: &str, prefix: &str) -> bool {
    value.to_lowercase().starts_with(&prefix.to_lowercase())
}

pub async fn write_json_file(path: &Path, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let content = format!("{}\n", serde_json::to_string_pretty(payload)?);
    tokio::fs::write(path, content).await?;
    Ok(())
}

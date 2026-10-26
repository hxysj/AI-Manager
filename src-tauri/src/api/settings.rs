use crate::core::error::ManagerError;
use crate::core::paths::{ensure_app_directories, resolve_app_paths, AppPaths};
use crate::core::settings::{
    normalize_app_settings, serialize_app_settings, write_json_file, AppSettings,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub async fn save_settings(
    app_settings: &mut AppSettings,
    paths: &mut AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<(), ManagerError> {
    // 只合并本次提交的字段，未提交的模块和嵌套选项沿用最新配置。
    if !payload.is_object() {
        return Err(ManagerError::System("设置修改必须是对象".to_string()));
    }
    let mut merged = serialize_app_settings(app_settings);
    merge_settings(&mut merged, &payload);
    let next_settings = normalize_app_settings(
        PathBuf::from(&app_settings.settings_file_path),
        Some(merged),
    );
    if payload
        .get("agents")
        .and_then(|agents| agents.get("translation"))
        .is_some()
        && next_settings.agents.translation.enabled
    {
        crate::api::proxy::translation_target_info(paths, &next_settings.agents.translation)?;
    }
    let next_paths = resolve_app_paths(Path::new(&next_settings.data_path));
    if next_settings.data_path != app_settings.data_path {
        ensure_app_directories(&next_paths).await?;
    }
    // 落盘成功后才更新内存，写入失败不能被界面误认为已经保存。
    write_json_file(
        Path::new(&next_settings.settings_file_path),
        &serialize_app_settings(&next_settings),
    )
    .await?;
    *app_settings = next_settings;
    *paths = next_paths;
    state["appSettings"] = serde_json::to_value(&*app_settings)?;
    crate::api::app_logs::configure(app_settings.system.log_enabled).await;
    Ok(())
}

fn merge_settings(current: &mut Value, patch: &Value) {
    if let (Some(current), Some(patch)) = (current.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            merge_settings(current.entry(key.clone()).or_insert(Value::Null), value);
        }
    } else {
        *current = patch.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::settings::load_app_settings;
    use serde_json::json;

    struct Fixture {
        root: PathBuf,
        settings: AppSettings,
        paths: AppPaths,
        state: Value,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("settings-save-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let settings = normalize_app_settings(
                root.join("settings.json"),
                Some(json!({
                    "dataPath": root,
                    "desktop": { "enabled": true, "petId": "last-pet" },
                    "system": { "closeAction": "minimize", "quickSwitchTools": { "git": true, "lan-share": false } },
                    "cloudSync": { "username": "cloud-user", "lastUpdatedAt": 123 },
                    "koofrSync": { "password": "test-password", "lastUpdatedAt": 456 },
                    "localBackup": { "enabled": false, "intervalMinutes": 90, "maxCount": 8, "lastBackupAt": 789 }
                })),
            );
            let paths = resolve_app_paths(&root);
            let state = json!({ "appSettings": settings, "sessions": ["保留会话"], "diagnostics": ["保留诊断"] });
            Self {
                root,
                settings,
                paths,
                state,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // 测试只清理本次创建的临时目录。
            let root = self.root.canonicalize().unwrap();
            assert_eq!(
                root.parent().unwrap(),
                std::env::temp_dir().canonicalize().unwrap()
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn partial_settings_save_preserves_other_modules_and_nested_fields() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut fixture = Fixture::new();
                // 已失效的翻译配置不能阻止修改其他模块。
                fixture.settings.agents.translation.enabled = true;
                let mut expected = serialize_app_settings(&fixture.settings);
                expected["system"]["quickSwitchTools"]["git"] = json!(false);
                save_settings(
                    &mut fixture.settings,
                    &mut fixture.paths,
                    &mut fixture.state,
                    json!({ "system": { "quickSwitchTools": { "git": false } } }),
                )
                .await
                .unwrap();
                let restored = load_app_settings(fixture.root.join("settings.json")).unwrap();
                assert_eq!(serialize_app_settings(&restored), expected);
                assert_eq!(fixture.state["sessions"], json!(["保留会话"]));
                assert_eq!(fixture.state["diagnostics"], json!(["保留诊断"]));
            });
    }

    #[test]
    fn successive_partial_saves_merge_into_latest_persisted_settings() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut fixture = Fixture::new();
                for patch in [
                    json!({ "system": { "autoLaunchEnabled": true } }),
                    json!({ "system": { "quickSwitchTools": { "git": false } } }),
                    json!({ "cloudSync": { "username": "new-user" } }),
                    json!({ "localBackup": { "maxCount": 3 } }),
                    json!({ "desktop": { "enabled": false } }),
                ] {
                    save_settings(
                        &mut fixture.settings,
                        &mut fixture.paths,
                        &mut fixture.state,
                        patch,
                    )
                    .await
                    .unwrap();
                }
                let restored = load_app_settings(fixture.root.join("settings.json")).unwrap();
                assert!(restored.system.auto_launch_enabled);
                assert_eq!(restored.system.close_action, "minimize");
                assert_eq!(restored.system.quick_switch_tools.get("git"), Some(&false));
                assert_eq!(
                    restored.system.quick_switch_tools.get("lan-share"),
                    Some(&false)
                );
                assert_eq!(restored.cloud_sync.username, "new-user");
                assert_eq!(restored.cloud_sync.last_updated_at, 123);
                assert_eq!(restored.koofr_sync.password, "test-password");
                assert_eq!(restored.local_backup.max_count, 3);
                assert_eq!(restored.local_backup.last_backup_at, 789);
                assert!(!restored.desktop.enabled);
                assert_eq!(restored.desktop.pet_id, "last-pet");
            });
    }

    #[test]
    fn failed_settings_write_keeps_memory_and_state_unchanged() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut fixture = Fixture::new();
                std::fs::create_dir(fixture.root.join("settings.json")).unwrap();
                let before = serialize_app_settings(&fixture.settings);
                let state_before = fixture.state.clone();
                let paths_before = serde_json::to_value(&fixture.paths).unwrap();
                assert!(save_settings(
                    &mut fixture.settings,
                    &mut fixture.paths,
                    &mut fixture.state,
                    json!({ "system": { "autoLaunchEnabled": true } })
                )
                .await
                .is_err());
                assert_eq!(serialize_app_settings(&fixture.settings), before);
                assert_eq!(fixture.state, state_before);
                assert_eq!(serde_json::to_value(&fixture.paths).unwrap(), paths_before);
            });
    }

    #[test]
    fn invalid_translation_activation_is_rejected_without_saving() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut fixture = Fixture::new();
                let before = serialize_app_settings(&fixture.settings);
                assert!(save_settings(
                    &mut fixture.settings,
                    &mut fixture.paths,
                    &mut fixture.state,
                    json!({ "agents": { "translation": { "enabled": true } } })
                )
                .await
                .is_err());
                assert_eq!(serialize_app_settings(&fixture.settings), before);
                assert!(!fixture.root.join("settings.json").exists());
            });
    }
}

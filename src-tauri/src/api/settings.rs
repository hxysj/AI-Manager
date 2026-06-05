use crate::core::error::ManagerError;
use crate::core::paths::{ensure_app_directories, resolve_app_paths, AppPaths};
use crate::core::settings::{
    normalize_app_settings, serialize_app_settings, write_json_file, AppSettings,
};
use crate::core::storage_state::create_initial_state;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub async fn save_settings(
    app_settings: &mut AppSettings,
    paths: &mut AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<(), ManagerError> {
    *app_settings = normalize_app_settings(
        PathBuf::from(&app_settings.settings_file_path),
        Some(payload),
    );
    *paths = resolve_app_paths(Path::new(&app_settings.data_path));
    tokio::fs::create_dir_all(&app_settings.data_path).await?;
    write_json_file(
        Path::new(&app_settings.settings_file_path),
        &serialize_app_settings(app_settings),
    )
    .await?;
    ensure_app_directories(paths).await?;
    *state = create_initial_state(paths, app_settings)?;
    Ok(())
}

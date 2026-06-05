use crate::core::error::ManagerError;
use crate::core::paths::{home_path, path_text};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectPathPayload {
    title: Option<String>,
    default_path: Option<String>,
    filters: Option<Vec<SelectFileFilter>>,
}

#[derive(Clone, Debug, Deserialize)]
struct SelectFileFilter {
    name: String,
    extensions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPathPayload {
    target_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct OpenExternalPayload {
    url: Option<String>,
}

pub fn select_directory(app: &AppHandle, payload: Option<Value>) -> Result<Value, ManagerError> {
    let payload: SelectPathPayload = serde_json::from_value(payload.unwrap_or_else(|| json!({})))?;
    let title = payload
        .title
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "选择目录".to_string());
    let default_path = payload
        .default_path
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path_text(home_path()));
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title)
        .set_directory(PathBuf::from(default_path))
        .set_can_create_directories(true);

    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }

    let selected_path = dialog
        .blocking_pick_folder()
        .map(file_path_text)
        .transpose()?
        .unwrap_or_default();

    Ok(json!(selected_path))
}

pub fn select_file(app: &AppHandle, payload: Option<Value>) -> Result<Value, ManagerError> {
    let payload: SelectPathPayload = serde_json::from_value(payload.unwrap_or_else(|| json!({})))?;
    let title = payload
        .title
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "选择文件".to_string());
    let default_path = payload
        .default_path
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path_text(home_path()));
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title)
        .set_directory(PathBuf::from(default_path));

    if let Some(filters) = payload.filters {
        for filter in filters {
            let extensions = filter
                .extensions
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            dialog = dialog.add_filter(filter.name, &extensions);
        }
    }

    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }

    let selected_path = dialog
        .blocking_pick_file()
        .map(file_path_text)
        .transpose()?
        .unwrap_or_default();

    Ok(json!(selected_path))
}

pub fn open_path(app: &AppHandle, payload: Option<Value>) -> Result<Value, ManagerError> {
    let payload: OpenPathPayload = serde_json::from_value(payload.unwrap_or_else(|| json!({})))?;

    let Some(target_path) = payload.target_path else {
        return Ok(json!(false));
    };

    if target_path.is_empty() {
        return Ok(json!(false));
    }

    app.opener()
        .open_path(target_path, None::<&str>)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(json!(true))
}

pub fn open_external(app: &AppHandle, payload: Option<Value>) -> Result<Value, ManagerError> {
    let payload: OpenExternalPayload =
        serde_json::from_value(payload.unwrap_or_else(|| json!({})))?;

    let Some(url) = payload.url else {
        return Ok(json!(false));
    };

    if url.is_empty() {
        return Ok(json!(false));
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(json!(true))
}

fn file_path_text(file_path: FilePath) -> Result<String, ManagerError> {
    file_path
        .simplified()
        .into_path()
        .map(path_text)
        .map_err(|error| ManagerError::Path(error.to_string()))
}

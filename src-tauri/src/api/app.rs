use crate::core::error::ManagerError;
use crate::core::settings::{serialize_app_settings, write_json_file, AppSettings};
use serde::Deserialize;
use serde_json::{json, Value};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use tauri::window::Color;
use tauri::{
    Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

const QUICK_SWITCH_LABEL: &str = "quick-switch";
const QUICK_SWITCH_EXPANDED_WIDTH: u32 = 360;
const QUICK_SWITCH_EXPANDED_HEIGHT: u32 = 238;
const QUICK_SWITCH_COLLAPSED_WIDTH: u32 = 44;
const QUICK_SWITCH_COLLAPSED_HEIGHT: u32 = 44;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

struct DownloadedUpdate {
    update: Update,
    bytes: Vec<u8>,
}

static DOWNLOADED_UPDATE: Mutex<Option<DownloadedUpdate>> = Mutex::const_new(None);

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseActionPayload {
    #[serde(default)]
    action: String,
    #[serde(default)]
    remember: bool,
}

pub async fn update_status() -> Result<Value, ManagerError> {
    Ok(json!({
      "phase": "idle",
      "manual": false,
      "message": "",
      "version": "",
      "releaseNotes": "",
      "percent": 0,
      "transferred": 0,
      "total": 0,
      "bytesPerSecond": 0,
      "installDirectory": "",
      "configured": false,
      "isDev": true,
      "updatedAt": 0
    }))
}

pub fn apply_auto_launch_setting(
    app: &tauri::AppHandle,
    app_settings: &AppSettings,
) -> Result<(), ManagerError> {
    let autostart_manager = app.autolaunch();
    let auto_launch_enabled = autostart_manager
        .is_enabled()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if app_settings.system.auto_launch_enabled && !auto_launch_enabled {
        autostart_manager
            .enable()
            .map_err(|error| ManagerError::System(error.to_string()))?;
    }

    if !app_settings.system.auto_launch_enabled && auto_launch_enabled {
        autostart_manager
            .disable()
            .map_err(|error| ManagerError::System(error.to_string()))?;
    }

    Ok(())
}

pub async fn check_updates(app: &tauri::AppHandle) -> Result<Value, ManagerError> {
    let status = if cfg!(debug_assertions) {
        json!({
          "phase": "dev-disabled",
          "message": "开发模式没有打包后的更新元数据和安装器上下文，无法使用 Tauri 更新器完整检查并安装更新。请使用打包安装版验证更新流程。",
          "manual": true,
          "configured": false,
          "isDev": true
        })
    } else if !update_configured() {
        json!({
          "phase": "unconfigured",
          "message": "当前安装包未包含更新配置。",
          "manual": true,
          "configured": false,
          "isDev": false
        })
    } else {
        let updater = app
            .updater()
            .map_err(|error| ManagerError::System(error.to_string()))?;
        let update = updater
            .check()
            .await
            .map_err(|error| ManagerError::System(error.to_string()))?;

        *DOWNLOADED_UPDATE.lock().await = None;

        match update {
            Some(update) => json!({
              "phase": "available",
              "message": format!("发现新版本 {}。", update.version),
              "version": update.version,
              "releaseNotes": update.body.unwrap_or_default(),
              "manual": true,
              "configured": true,
              "isDev": false
            }),
            None => json!({
              "phase": "not-available",
              "message": "当前已是最新版本。",
              "manual": true,
              "configured": true,
              "isDev": false
            })
        }
    };

    emit_update_status(app, status)
}

pub async fn download_update(app: &tauri::AppHandle) -> Result<Value, ManagerError> {
    if cfg!(debug_assertions) {
        return emit_update_status(
            app,
            json!({
              "phase": "dev-disabled",
              "message": "开发模式没有打包后的更新元数据和安装器上下文，无法使用 Tauri 更新器完整检查并安装更新。请使用打包安装版验证更新流程。",
              "manual": true,
              "configured": false,
              "isDev": true
            }),
        );
    }

    if !update_configured() {
        return emit_update_status(
            app,
            json!({
              "phase": "unconfigured",
              "message": "当前安装包未包含更新配置。",
              "manual": true,
              "configured": false,
              "isDev": false
            }),
        );
    }

    let updater = app
        .updater()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?
    else {
        return emit_update_status(
            app,
            json!({
              "phase": "not-available",
              "message": "当前已是最新版本。",
              "manual": true,
              "configured": true,
              "isDev": false
            }),
        );
    };
    let mut transferred = 0_u64;
    let started_at = now_millis();
    let app_handle = app.clone();
    let version = update.version.clone();
    let bytes = update
        .download(
            |chunk_length, total| {
                transferred += chunk_length as u64;
                let elapsed = now_millis().saturating_sub(started_at).max(1);
                let percent = total
                    .filter(|item| *item > 0)
                    .map(|item| transferred as f64 * 100.0 / item as f64)
                    .unwrap_or(0.0);
                let _ = emit_update_status(
                    &app_handle,
                    json!({
                      "phase": "downloading",
                      "message": format!("正在下载新版本 {}。", version),
                      "version": version,
                      "manual": true,
                      "configured": true,
                      "isDev": false,
                      "percent": percent,
                      "transferred": transferred,
                      "total": total.unwrap_or(0),
                      "bytesPerSecond": transferred * 1000 / elapsed
                    }),
                );
            },
            || {},
        )
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    let version = update.version.clone();
    let release_notes = update.body.clone().unwrap_or_default();
    *DOWNLOADED_UPDATE.lock().await = Some(DownloadedUpdate { update, bytes });

    emit_update_status(
        app,
        json!({
          "phase": "downloaded",
          "message": format!("新版本 {} 已下载完成。", version),
          "version": version,
          "releaseNotes": release_notes,
          "manual": true,
          "configured": true,
          "isDev": false,
          "percent": 100,
          "transferred": transferred,
          "total": transferred,
          "bytesPerSecond": 0
        }),
    )
}

pub async fn install_update(
    app: &tauri::AppHandle,
    _payload: Value,
) -> Result<Value, ManagerError> {
    let downloaded_update = DOWNLOADED_UPDATE.lock().await.take();
    let Some(downloaded_update) = downloaded_update else {
        return Err(ManagerError::System("更新安装包未下载完成".to_string()));
    };

    let version = downloaded_update.update.version.clone();

    emit_update_status(
        app,
        json!({
          "phase": "installing",
          "message": "正在打开更新安装程序。",
          "version": version,
          "manual": true,
          "configured": true,
          "isDev": false
        }),
    )?;

    downloaded_update
        .update
        .install(downloaded_update.bytes)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(json!(true))
}

pub async fn dismiss_update() -> Result<Value, ManagerError> {
    Ok(json!({
      "phase": "idle",
      "message": "",
      "manual": false,
      "configured": false,
      "isDev": cfg!(debug_assertions),
      "percent": 0,
      "transferred": 0,
      "total": 0,
      "bytesPerSecond": 0,
      "updatedAt": now_millis()
    }))
}

pub async fn uninstall_without_trace(
    app: &tauri::AppHandle,
    app_settings: &AppSettings,
) -> Result<Value, ManagerError> {
    if cfg!(debug_assertions) {
        return Err(ManagerError::System("开发环境不执行无痕卸载".to_string()));
    }

    let exe_path = std::env::current_exe()?;
    let install_directory = exe_path
        .parent()
        .ok_or_else(|| ManagerError::Path("无法解析安装目录".to_string()))?;
    let uninstaller_path = find_app_uninstaller(install_directory).await?;
    let data_path = PathBuf::from(&app_settings.data_path).canonicalize()?;

    if data_path.parent().is_none() {
        return Err(ManagerError::System("数据目录不能是磁盘根目录".to_string()));
    }

    if uninstaller_path.starts_with(&data_path) {
        return Err(ManagerError::System(
            "数据目录不能包含应用卸载程序".to_string(),
        ));
    }

    let cleanup_script = [
        "$ErrorActionPreference = 'SilentlyContinue'".to_string(),
        format!("$processId = {}", std::process::id()),
        format!(
            "$dataPath = {}",
            to_powershell_literal(&data_path.to_string_lossy())
        ),
        format!(
            "$settingsPath = {}",
            to_powershell_literal(&app_settings.settings_file_path)
        ),
        format!(
            "$uninstallerPath = {}",
            to_powershell_literal(&uninstaller_path.to_string_lossy())
        ),
        "Wait-Process -Id $processId".to_string(),
        "Remove-Item -LiteralPath $dataPath -Recurse -Force".to_string(),
        "Remove-Item -LiteralPath $settingsPath -Force".to_string(),
        "Start-Process -FilePath $uninstallerPath -ArgumentList '/S'".to_string(),
    ]
    .join("; ");

    let mut command = std::process::Command::new("powershell.exe");

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &cleanup_script,
        ])
        .spawn()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    app.exit(0);
    Ok(json!(true))
}

pub async fn handle_close_action(
    app: &tauri::AppHandle,
    app_settings: &mut AppSettings,
    state: &mut Value,
    payload: Value,
    quick_switch_collapsed: bool,
) -> Result<Value, ManagerError> {
    let payload: CloseActionPayload = serde_json::from_value(payload)?;
    let action = if payload.action.is_empty() {
        "cancel"
    } else {
        payload.action.as_str()
    };

    if payload.remember && action != "cancel" {
        app_settings.system.close_action = action.to_string();
        write_json_file(
            Path::new(&app_settings.settings_file_path),
            &serialize_app_settings(app_settings),
        )
        .await?;
        state["appSettings"] = serde_json::to_value(&*app_settings)?;
    }

    if action == "minimize" {
        if let Some(window) = app.get_webview_window("main") {
            window
                .hide()
                .map_err(|error| ManagerError::System(error.to_string()))?;
        }
        sync_quick_switch_window(app, app_settings, quick_switch_collapsed)?;
        return Ok(json!(true));
    }

    if action == "quit" {
        app.exit(0);
    }

    Ok(json!(true))
}

pub fn show_main_panel(
    app: &tauri::AppHandle,
    _app_settings: &AppSettings,
    _quick_switch_collapsed: bool,
) -> Result<Value, ManagerError> {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => create_main_window(app)?,
    };

    if window
        .is_minimized()
        .map_err(|error| ManagerError::System(error.to_string()))?
    {
        window
            .unminimize()
            .map_err(|error| ManagerError::System(error.to_string()))?;
    }

    window
        .show()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    window
        .set_focus()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    destroy_quick_switch_window(app)?;
    Ok(json!(true))
}

pub fn set_quick_switch_collapsed(
    app: &tauri::AppHandle,
    payload: Value,
) -> Result<Value, ManagerError> {
    let collapsed = payload
        .get("collapsed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if let Some(window) = app.get_webview_window(QUICK_SWITCH_LABEL) {
        apply_quick_switch_collapsed(&window, collapsed)?;
    }

    Ok(json!(true))
}

pub fn move_quick_switch_by(app: &tauri::AppHandle, payload: Value) -> Result<Value, ManagerError> {
    let Some(window) = app.get_webview_window(QUICK_SWITCH_LABEL) else {
        return Ok(json!(true));
    };
    let position = window
        .outer_position()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let x = payload.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = payload.get("y").and_then(Value::as_f64).unwrap_or(0.0);

    window
        .set_position(PhysicalPosition::new(
            position.x + x.round() as i32,
            position.y + y.round() as i32,
        ))
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(json!(true))
}

pub fn sync_quick_switch_window(
    app: &tauri::AppHandle,
    app_settings: &AppSettings,
    quick_switch_collapsed: bool,
) -> Result<(), ManagerError> {
    let Some(main_window) = app.get_webview_window("main") else {
        destroy_quick_switch_window(app)?;
        return Ok(());
    };
    let main_visible = main_window
        .is_visible()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let main_minimized = main_window
        .is_minimized()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let should_show = app_settings.system.quick_switch_visible && (!main_visible || main_minimized);

    if should_show {
        create_quick_switch_window(app, quick_switch_collapsed)?;
    } else {
        destroy_quick_switch_window(app)?;
    }

    Ok(())
}

fn create_main_window(app: &tauri::AppHandle) -> Result<WebviewWindow, ManagerError> {
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Monkey Thief")
        .inner_size(1280.0, 860.0)
        .min_inner_size(1120.0, 720.0)
        .background_color(Color(255, 255, 255, 255))
        .build()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(window)
}

fn create_quick_switch_window(
    app: &tauri::AppHandle,
    quick_switch_collapsed: bool,
) -> Result<(), ManagerError> {
    if app.get_webview_window(QUICK_SWITCH_LABEL).is_some() {
        return Ok(());
    }
    let width = if quick_switch_collapsed {
        QUICK_SWITCH_COLLAPSED_WIDTH
    } else {
        QUICK_SWITCH_EXPANDED_WIDTH
    };
    let height = if quick_switch_collapsed {
        QUICK_SWITCH_COLLAPSED_HEIGHT
    } else {
        QUICK_SWITCH_EXPANDED_HEIGHT
    };

    let window = WebviewWindowBuilder::new(
        app,
        QUICK_SWITCH_LABEL,
        WebviewUrl::App("index.html?panel=quick-switch".into()),
    )
    .title("Monkey Thief")
    .inner_size(width as f64, height as f64)
    .decorations(false)
    .resizable(false)
    .minimizable(false)
    .maximizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .transparent(true)
    .background_color(Color(0, 0, 0, 0))
    .shadow(false)
    .focused(false)
    .build()
    .map_err(|error| ManagerError::System(error.to_string()))?;

    window
        .set_always_on_top(true)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    set_quick_switch_size(&window, width, height)?;
    position_quick_switch_window(&window)?;

    Ok(())
}

fn destroy_quick_switch_window(app: &tauri::AppHandle) -> Result<(), ManagerError> {
    if let Some(window) = app.get_webview_window(QUICK_SWITCH_LABEL) {
        window
            .destroy()
            .map_err(|error| ManagerError::System(error.to_string()))?;
    }

    Ok(())
}

fn apply_quick_switch_collapsed(
    window: &WebviewWindow,
    collapsed: bool,
) -> Result<(), ManagerError> {
    let width = if collapsed {
        QUICK_SWITCH_COLLAPSED_WIDTH
    } else {
        QUICK_SWITCH_EXPANDED_WIDTH
    };
    let height = if collapsed {
        QUICK_SWITCH_COLLAPSED_HEIGHT
    } else {
        QUICK_SWITCH_EXPANDED_HEIGHT
    };
    let position = window
        .outer_position()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let size = window
        .outer_size()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let monitor = window
        .current_monitor()
        .map_err(|error| ManagerError::System(error.to_string()))?
        .or_else(|| window.primary_monitor().ok().and_then(|monitor| monitor));
    let Some(monitor) = monitor else {
        set_quick_switch_size(window, width, height)?;
        return Ok(());
    };
    let work_area = monitor.work_area();
    let center_x = position.x as f64 + size.width as f64 / 2.0;
    let center_y = position.y as f64 + size.height as f64 / 2.0;
    let next_x = (center_x - width as f64 / 2.0).round() as i32;
    let next_y = (center_y - height as f64 / 2.0).round() as i32;
    let max_x = work_area.position.x + work_area.size.width as i32 - width as i32;
    let max_y = work_area.position.y + work_area.size.height as i32 - height as i32;
    let x = next_x.max(work_area.position.x).min(max_x);
    let y = next_y.max(work_area.position.y).min(max_y);

    set_quick_switch_size(window, width, height)?;
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(())
}

fn position_quick_switch_window(window: &WebviewWindow) -> Result<(), ManagerError> {
    let size = window
        .outer_size()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let monitor = window
        .primary_monitor()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let Some(monitor) = monitor else {
        return Ok(());
    };
    let work_area = monitor.work_area();
    let x = work_area.position.x + work_area.size.width as i32 - size.width as i32 - 16;
    let y = work_area.position.y + work_area.size.height as i32 - size.height as i32 - 12;

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(())
}

fn set_quick_switch_size(
    window: &WebviewWindow,
    width: u32,
    height: u32,
) -> Result<(), ManagerError> {
    let size = PhysicalSize::new(width, height);

    window
        .set_resizable(true)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    window
        .set_min_size(Some(PhysicalSize::new(1, 1)))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    window
        .set_max_size(None::<PhysicalSize<u32>>)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    window
        .set_size(size)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    window
        .set_min_size(Some(size))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    window
        .set_max_size(Some(size))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    window
        .set_resizable(false)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(())
}

fn emit_update_status(app: &tauri::AppHandle, patch: Value) -> Result<Value, ManagerError> {
    let mut status = patch.as_object().cloned().unwrap_or_default();

    status.insert("updatedAt".to_string(), json!(now_millis()));
    if !status.contains_key("percent") {
        status.insert("percent".to_string(), json!(0));
    }
    if !status.contains_key("transferred") {
        status.insert("transferred".to_string(), json!(0));
    }
    if !status.contains_key("total") {
        status.insert("total".to_string(), json!(0));
    }
    if !status.contains_key("bytesPerSecond") {
        status.insert("bytesPerSecond".to_string(), json!(0));
    }
    if !status.contains_key("installDirectory") {
        status.insert("installDirectory".to_string(), json!(""));
    }

    let payload = Value::Object(status);

    app.emit("app:update-status", payload.clone())
        .map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(payload)
}

fn update_configured() -> bool {
    !crate::update_config::GITHUB_TOKEN.trim().is_empty()
}

async fn find_app_uninstaller(install_directory: &Path) -> Result<PathBuf, ManagerError> {
    let mut entries = tokio::fs::read_dir(install_directory).await?;

    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        if metadata.is_file()
            && file_name.to_lowercase().starts_with("uninstall")
            && file_name.to_lowercase().ends_with(".exe")
        {
            return Ok(entry.path());
        }
    }

    Err(ManagerError::System("未找到应用卸载程序".to_string()))
}

fn to_powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

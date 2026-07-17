use crate::core::error::ManagerError;
use crate::core::settings::{serialize_app_settings, write_json_file, AppSettings};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use serde_json::{json, Value};
#[cfg(windows)]
use std::io::{Cursor, Read};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex as SyncMutex};
use std::time::Duration;
use tauri::window::Color;
use tauri::{
    Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_updater::{Update, Updater, UpdaterExt};
use tokio::sync::Mutex as AsyncMutex;
use url::Url;

const GITHUB_LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/hxysj/AI-Manager/releases/latest";
const GITHUB_RELEASE_ASSET_API_URL_PREFIX: &str =
    "https://api.github.com/repos/hxysj/AI-Manager/releases/assets";
const GITHUB_UPDATER_METADATA_ASSET: &str = "latest.json";
const UPDATE_REQUEST_TIMEOUT_SECS: u64 = 30;
const UPDATE_DOWNLOAD_TIMEOUT_SECS: u64 = 300;
const QUICK_SWITCH_LABEL: &str = "quick-switch";
const QUICK_SWITCH_EXPANDED_WIDTH: u32 = 360;
const QUICK_SWITCH_EXPANDED_HEIGHT: u32 = 238;
const QUICK_SWITCH_COLLAPSED_WIDTH: u32 = 44;
const QUICK_SWITCH_COLLAPSED_HEIGHT: u32 = 44;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
#[allow(dead_code)]
const AI_MANAGER_UNINSTALL_ROOT: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
#[cfg(windows)]
#[allow(dead_code)]
const AI_MANAGER_UNINSTALL_KEYS: [&str; 3] = [
    "a178c25c-9e1d-5bca-9cea-7f005c2da482",
    "Monkey Thief",
    "com.monkeythief.desktop",
];

struct DownloadedUpdate {
    update: Update,
    bytes: Vec<u8>,
}

static DOWNLOADED_UPDATE: AsyncMutex<Option<DownloadedUpdate>> = AsyncMutex::const_new(None);
static UPDATE_STATUS: LazyLock<SyncMutex<Value>> =
    LazyLock::new(|| SyncMutex::new(default_update_status()));
static UPDATE_BUSY: SyncMutex<bool> = SyncMutex::new(false);

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseActionPayload {
    #[serde(default)]
    action: String,
    #[serde(default)]
    remember: bool,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    id: u64,
    name: String,
}

pub async fn update_status() -> Result<Value, ManagerError> {
    let mut status = update_status_snapshot()?;

    if status
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("idle")
        == "idle"
    {
        status["configured"] = json!(update_configured());
        status["isDev"] = json!(cfg!(debug_assertions));
    }

    Ok(status)
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
    let _guard = begin_update_task(app, "checking", "正在检查更新。")?;
    let status_result = async {
        if !update_configured() {
            let message = if cfg!(debug_assertions) {
                "测试环境未配置 AI_MANAGER_GITHUB_TOKEN，无法访问私有 GitHub Release。"
            } else {
                "当前安装包未包含更新配置。"
            };

            return Ok(json!({
              "phase": "unconfigured",
              "message": message,
              "manual": true,
              "configured": false,
              "isDev": cfg!(debug_assertions)
            }));
        }

        if let Some(status) = downloaded_update_status(true).await {
            return Ok(status);
        }

        let release = fetch_latest_github_release().await?;
        let updater = create_github_release_updater(app, &release)?;
        let update = updater
            .check()
            .await
            .map_err(|error| ManagerError::System(error.to_string()))?;

        *DOWNLOADED_UPDATE.lock().await = None;

        Ok(match update {
            Some(update) => json!({
              "phase": "available",
              "message": format!("发现新版本 {}。", update.version),
              "version": update.version,
              "releaseNotes": update.body.unwrap_or_default(),
              "manual": true,
              "configured": true,
              "isDev": cfg!(debug_assertions)
            }),
            None => json!({
              "phase": "not-available",
              "message": "当前已是最新版本。",
              "manual": true,
              "configured": true,
              "isDev": cfg!(debug_assertions)
            }),
        })
    }
    .await;

    match status_result {
        Ok(status) => emit_update_status(app, status),
        Err(error) => {
            let _ = emit_update_status(app, update_error_status(&error));
            Err(error)
        }
    }
}

pub async fn download_update(app: &tauri::AppHandle) -> Result<Value, ManagerError> {
    let _guard = begin_update_task(app, "downloading", "正在准备下载更新。")?;
    let status_result = async {
        if !update_configured() {
            let message = if cfg!(debug_assertions) {
                "测试环境未配置 AI_MANAGER_GITHUB_TOKEN，无法访问私有 GitHub Release。"
            } else {
                "当前安装包未包含更新配置。"
            };

            return Ok(json!({
              "phase": "unconfigured",
              "message": message,
              "manual": true,
              "configured": false,
              "isDev": cfg!(debug_assertions)
            }));
        }

        if let Some(status) = downloaded_update_status(true).await {
            return Ok(status);
        }

        let release = fetch_latest_github_release().await?;
        let updater = create_github_release_updater(app, &release)?;
        let Some(mut update) = updater
            .check()
            .await
            .map_err(|error| ManagerError::System(error.to_string()))?
        else {
            return Ok(json!({
              "phase": "not-available",
              "message": "当前已是最新版本。",
              "manual": true,
              "configured": true,
              "isDev": cfg!(debug_assertions)
            }));
        };
        apply_github_release_download_url(&release, &mut update)?;
        update.timeout = Some(Duration::from_secs(UPDATE_DOWNLOAD_TIMEOUT_SECS));
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
                          "isDev": cfg!(debug_assertions),
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

        Ok(json!({
          "phase": "downloaded",
          "message": format!("新版本 {} 已下载完成。", version),
          "version": version,
          "releaseNotes": release_notes,
          "manual": true,
          "configured": true,
          "isDev": cfg!(debug_assertions),
          "percent": 100,
          "transferred": transferred,
          "total": transferred,
          "bytesPerSecond": 0
        }))
    }
    .await;

    match status_result {
        Ok(status) => emit_update_status(app, status),
        Err(error) => {
            let _ = emit_update_status(app, update_error_status(&error));
            Err(error)
        }
    }
}

pub async fn install_update(
    app: &tauri::AppHandle,
    _payload: Value,
) -> Result<Value, ManagerError> {
    let _guard = begin_update_task(app, "installing", "正在打开更新安装程序。")?;
    let downloaded_update = DOWNLOADED_UPDATE.lock().await.take();
    let Some(downloaded_update) = downloaded_update else {
        let error = ManagerError::System("更新安装包未下载完成".to_string());
        let _ = emit_update_status(app, update_error_status(&error));
        return Err(error);
    };

    #[cfg(windows)]
    {
        let version = downloaded_update.update.version.clone();
        let release_notes = downloaded_update.update.body.clone().unwrap_or_default();
        let transferred = downloaded_update.bytes.len() as u64;

        if let Err(error) = install_update_on_windows(&downloaded_update).await {
            *DOWNLOADED_UPDATE.lock().await = Some(downloaded_update);
            emit_update_status(
                app,
                json!({
                  "phase": "downloaded",
                  "message": format!("新版本 {} 已下载完成，可重新打开安装向导。", version),
                  "version": version,
                  "releaseNotes": release_notes,
                  "manual": true,
                  "configured": true,
                  "isDev": cfg!(debug_assertions),
                  "percent": 100,
                  "transferred": transferred,
                  "total": transferred,
                  "bytesPerSecond": 0
                }),
            )?;
            return Err(error);
        }

        *DOWNLOADED_UPDATE.lock().await = Some(downloaded_update);
        return emit_update_status(
            app,
            json!({
              "phase": "installer-opened",
              "message": "安装程序已打开，请在安装程序中完成升级。",
              "version": version,
              "releaseNotes": release_notes,
              "manual": true,
              "configured": true,
              "isDev": cfg!(debug_assertions),
              "percent": 100,
              "transferred": transferred,
              "total": transferred,
              "bytesPerSecond": 0
            }),
        );
    }

    #[cfg(not(windows))]
    {
        downloaded_update
            .update
            .install(&downloaded_update.bytes)
            .map_err(|error| ManagerError::System(error.to_string()))?;
        Ok(json!(true))
    }
}

pub async fn dismiss_update() -> Result<Value, ManagerError> {
    let mut status = update_status_snapshot()?;

    if status
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("idle")
        == "downloaded"
    {
        status["manual"] = json!(false);
        status["updatedAt"] = json!(now_millis());
        store_update_status(&status)?;
        return Ok(status);
    }

    store_update_status(&default_update_status())?;
    update_status().await
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
        .inner_size(1024.0, 688.0)
        .min_inner_size(896.0, 576.0)
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

    let quick_switch_url = format!(
        "index.html?panel=quick-switch&collapsed={}",
        if quick_switch_collapsed { "1" } else { "0" }
    );

    let window = WebviewWindowBuilder::new(
        app,
        QUICK_SWITCH_LABEL,
        WebviewUrl::App(quick_switch_url.into()),
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

fn default_update_status() -> Value {
    json!({
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
      "isDev": cfg!(debug_assertions),
      "updatedAt": 0
    })
}

fn update_status_snapshot() -> Result<Value, ManagerError> {
    UPDATE_STATUS
        .lock()
        .map(|status| status.clone())
        .map_err(|error| ManagerError::System(error.to_string()))
}

fn store_update_status(status: &Value) -> Result<(), ManagerError> {
    let mut current = UPDATE_STATUS
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    *current = status.clone();
    Ok(())
}

fn update_error_status(error: &ManagerError) -> Value {
    json!({
      "phase": "error",
      "message": error.to_string(),
      "manual": true,
      "configured": update_configured(),
      "isDev": cfg!(debug_assertions)
    })
}

struct UpdateTaskGuard;

impl Drop for UpdateTaskGuard {
    fn drop(&mut self) {
        if let Ok(mut busy) = UPDATE_BUSY.lock() {
            *busy = false;
        }
    }
}

fn begin_update_task(
    app: &tauri::AppHandle,
    phase: &str,
    message: &str,
) -> Result<UpdateTaskGuard, ManagerError> {
    let mut busy = UPDATE_BUSY
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if *busy {
        return Err(ManagerError::System(
            "已有更新任务正在执行，请稍后再试。".to_string(),
        ));
    }

    *busy = true;
    drop(busy);
    if let Err(error) = emit_update_status(
        app,
        json!({
          "phase": phase,
          "message": message,
          "manual": true,
          "configured": update_configured(),
          "isDev": cfg!(debug_assertions)
        }),
    ) {
        if let Ok(mut busy) = UPDATE_BUSY.lock() {
            *busy = false;
        }

        return Err(error);
    }

    Ok(UpdateTaskGuard)
}

async fn downloaded_update_status(manual: bool) -> Option<Value> {
    let downloaded_update = DOWNLOADED_UPDATE.lock().await;
    let downloaded_update = downloaded_update.as_ref()?;
    let transferred = downloaded_update.bytes.len() as u64;
    let version = downloaded_update.update.version.clone();

    Some(json!({
      "phase": "downloaded",
      "message": format!("新版本 {} 已下载完成。", version),
      "version": version,
      "releaseNotes": downloaded_update.update.body.clone().unwrap_or_default(),
      "manual": manual,
      "configured": true,
      "isDev": cfg!(debug_assertions),
      "percent": 100,
      "transferred": transferred,
      "total": transferred,
      "bytesPerSecond": 0
    }))
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
    store_update_status(&payload)?;

    app.emit("app:update-status", payload.clone())
        .map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(payload)
}

async fn fetch_latest_github_release() -> Result<GithubRelease, ManagerError> {
    let token = update_token();
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(UPDATE_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|error| ManagerError::System(error.to_string()))?
        .get(GITHUB_LATEST_RELEASE_API_URL)
        .header(USER_AGENT, "Monkey-Thief-Updater")
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if !response.status().is_success() {
        return Err(ManagerError::System(format!(
            "获取 GitHub 最新版本失败：{}",
            response.status()
        )));
    }

    response
        .json::<GithubRelease>()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))
}

fn create_github_release_updater(
    app: &tauri::AppHandle,
    release: &GithubRelease,
) -> Result<Updater, ManagerError> {
    let metadata_url = github_release_asset_api_url(release, GITHUB_UPDATER_METADATA_ASSET, None)?;
    let token = update_token();

    app.updater_builder()
        .endpoints(vec![metadata_url])
        .map_err(|error| ManagerError::System(error.to_string()))?
        .timeout(Duration::from_secs(UPDATE_REQUEST_TIMEOUT_SECS))
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .map_err(|error| ManagerError::System(error.to_string()))?
        .header(ACCEPT, "application/octet-stream")
        .map_err(|error| ManagerError::System(error.to_string()))?
        .build()
        .map_err(|error| ManagerError::System(error.to_string()))
}

fn apply_github_release_download_url(
    release: &GithubRelease,
    update: &mut Update,
) -> Result<(), ManagerError> {
    let download_file_name = update
        .download_url
        .path_segments()
        .and_then(|segments| segments.last())
        .unwrap_or_default()
        .to_string();
    let decoded_file_name = url::form_urlencoded::parse(download_file_name.as_bytes())
        .map(|(value, _)| value.to_string())
        .next()
        .unwrap_or_else(|| download_file_name.clone());

    update.download_url =
        github_release_asset_api_url(release, &download_file_name, Some(&decoded_file_name))?;
    Ok(())
}

fn github_release_asset_api_url(
    release: &GithubRelease,
    asset_name: &str,
    fallback_asset_name: Option<&str>,
) -> Result<Url, ManagerError> {
    let fallback_asset_name = fallback_asset_name.unwrap_or_default();
    let asset_id = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name || asset.name == fallback_asset_name)
        .map(|asset| asset.id)
        .ok_or_else(|| {
            ManagerError::System(format!("GitHub Release 缺少 {} 资产。", asset_name))
        })?;

    Url::parse(&format!(
        "{}/{}",
        GITHUB_RELEASE_ASSET_API_URL_PREFIX, asset_id
    ))
    .map_err(|error| ManagerError::System(error.to_string()))
}

fn update_configured() -> bool {
    !update_token().is_empty()
}

#[cfg(windows)]
async fn install_update_on_windows(
    downloaded_update: &DownloadedUpdate,
) -> Result<(), ManagerError> {
    let installer_path = persist_windows_update_installer(
        &downloaded_update.update.version,
        &downloaded_update.bytes,
    )
    .await?;
    std::process::Command::new(&installer_path)
        .spawn()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(())
}

#[cfg(windows)]
#[allow(dead_code)]
struct InstalledWindowsApp {
    uninstall_string: String,
    install_location: String,
}

#[cfg(windows)]
async fn persist_windows_update_installer(
    version: &str,
    bytes: &[u8],
) -> Result<PathBuf, ManagerError> {
    let temp_dir = windows_update_temp_dir(version)?;
    tokio::fs::create_dir_all(&temp_dir).await?;
    let installer_path = temp_dir.join(format!(
        "Monkey-Thief-{}-setup.exe",
        safe_file_part(version)
    ));
    let installer_bytes = extract_windows_update_installer(bytes)?;

    tokio::fs::write(&installer_path, installer_bytes).await?;
    Ok(installer_path)
}

#[cfg(windows)]
fn extract_windows_update_installer(bytes: &[u8]) -> Result<Vec<u8>, ManagerError> {
    if bytes.len() >= 2 && &bytes[0..2] == b"MZ" {
        return Ok(bytes.to_vec());
    }

    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|error| ManagerError::System(error.to_string()))?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| ManagerError::System(error.to_string()))?;
        let name = file.name().to_lowercase();

        if !name.ends_with(".exe") {
            continue;
        }

        let mut output = Vec::new();
        file.read_to_end(&mut output)?;
        return Ok(output);
    }

    Err(ManagerError::System(
        "更新包中未找到 Windows 安装程序".to_string(),
    ))
}

#[cfg(windows)]
fn windows_update_temp_dir(version: &str) -> Result<PathBuf, ManagerError> {
    Ok(std::env::temp_dir().join(format!("monkey-thief-update-{}", safe_file_part(version))))
}

#[cfg(windows)]
fn safe_file_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
}

#[cfg(windows)]
#[allow(dead_code)]
fn build_windows_update_script(
    current_process_id: u32,
    current_exe_path: &Path,
    installer_path: &Path,
    install_info: Option<&InstalledWindowsApp>,
) -> String {
    let uninstall_string = install_info
        .map(|info| info.uninstall_string.as_str())
        .unwrap_or("");
    let install_location = install_info
        .map(|info| info.install_location.as_str())
        .unwrap_or("");
    let registry_checks = AI_MANAGER_UNINSTALL_KEYS
        .iter()
        .map(|key| {
            format!(
                "    if (Test-Path -LiteralPath \"HKCU:\\$UninstallRoot\\{key}\") {{ return $true }}\n    if (Test-Path -LiteralPath \"HKLM:\\$UninstallRoot\\{key}\") {{ return $true }}",
                key = key.replace('`', "``")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"$ErrorActionPreference = 'Stop'
$ProcessId = {process_id}
$CurrentExePath = {current_exe_path}
$InstallerPath = {installer_path}
$UninstallString = {uninstall_string}
$InstallLocation = {install_location}
$UninstallRoot = {uninstall_root}

function Test-OldVersionExists {{
{registry_checks}
  if (-not [string]::IsNullOrWhiteSpace($InstallLocation)) {{
    if (Test-Path -LiteralPath (Join-Path $InstallLocation 'Monkey Thief.exe')) {{ return $true }}
    if (Test-Path -LiteralPath (Join-Path $InstallLocation 'monkey-thief.exe')) {{ return $true }}
  }}
  if (-not [string]::IsNullOrWhiteSpace($CurrentExePath)) {{
    if (Test-Path -LiteralPath $CurrentExePath) {{ return $true }}
  }}
  return $false
}}

function Split-CommandLine {{
  param([string]$CommandLine)
  $trimmed = $CommandLine.Trim()
  if ($trimmed.StartsWith('"')) {{
    $end = $trimmed.IndexOf('"', 1)
    if ($end -gt 0) {{
      return @($trimmed.Substring(1, $end - 1), $trimmed.Substring($end + 1).Trim())
    }}
  }}
  $parts = $trimmed.Split(' ', 2)
  if ($parts.Count -eq 1) {{
    return @($parts[0], '')
  }}
  return @($parts[0], $parts[1])
}}

Wait-Process -Id $ProcessId -ErrorAction SilentlyContinue

if (-not [string]::IsNullOrWhiteSpace($UninstallString)) {{
  $parts = Split-CommandLine $UninstallString
  $uninstallerPath = $parts[0]
  $uninstallerArgs = $parts[1]
  if ($uninstallerArgs -notmatch '(^|\s)/S(\s|$)') {{
    $uninstallerArgs = ($uninstallerArgs + ' /S').Trim()
  }}
  if (-not [string]::IsNullOrWhiteSpace($InstallLocation) -and $uninstallerArgs -notmatch '(^|\s)_\?=') {{
    # NSIS 原目录模式不会派生临时卸载器，-Wait 可以等待实际卸载彻底结束。
    $uninstallerArgs = ($uninstallerArgs + " _?=$InstallLocation").Trim()
  }}
  $process = Start-Process -FilePath $uninstallerPath -ArgumentList $uninstallerArgs -Wait -PassThru
  if ($process.ExitCode -ne 0) {{
    Add-Type -AssemblyName PresentationFramework
    [System.Windows.MessageBox]::Show('旧版本卸载失败，安装已取消。', 'Monkey Thief 更新', 'OK', 'Warning') | Out-Null
    exit 1
  }}
}}

for ($index = 0; $index -lt 120; $index++) {{
  if (-not (Test-OldVersionExists)) {{
    break
  }}
  Start-Sleep -Seconds 1
}}

if (Test-OldVersionExists) {{
  Add-Type -AssemblyName PresentationFramework
  [System.Windows.MessageBox]::Show('旧版本未卸载完成，安装已取消。', 'Monkey Thief 更新', 'OK', 'Warning') | Out-Null
  exit 1
}}

Start-Process -FilePath $InstallerPath
"#,
        process_id = current_process_id,
        current_exe_path = to_powershell_literal(&current_exe_path.to_string_lossy()),
        installer_path = to_powershell_literal(&installer_path.to_string_lossy()),
        uninstall_string = to_powershell_literal(uninstall_string),
        install_location = to_powershell_literal(install_location),
        uninstall_root = to_powershell_literal(AI_MANAGER_UNINSTALL_ROOT),
        registry_checks = registry_checks
    )
}

fn update_token() -> String {
    let built_in_token = crate::update_config::GITHUB_TOKEN.trim();

    if !built_in_token.is_empty() {
        return built_in_token.to_string();
    }

    let env_token = std::env::var("AI_MANAGER_GITHUB_TOKEN")
        .unwrap_or_default()
        .trim()
        .to_string();

    if !env_token.is_empty() {
        return env_token;
    }

    read_env_file_update_token()
}

fn read_env_file_update_token() -> String {
    let mut env_paths = Vec::new();

    if let Ok(current_dir) = std::env::current_dir() {
        env_paths.push(current_dir.join(".env"));
        env_paths.push(current_dir.join("..").join(".env"));
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    env_paths.push(manifest_dir.join(".env"));
    env_paths.push(manifest_dir.join("..").join(".env"));

    for env_path in env_paths {
        if let Ok(content) = std::fs::read_to_string(env_path) {
            let token = parse_env_update_token(&content);

            if !token.is_empty() {
                return token;
            }
        }
    }

    String::new()
}

fn parse_env_update_token(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };

        if key.trim() != "AI_MANAGER_GITHUB_TOKEN" {
            continue;
        }

        let value = value.trim();
        let quoted = (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''));

        return if quoted && value.len() >= 2 {
            value[1..value.len() - 1].trim().to_string()
        } else {
            value.to_string()
        };
    }

    String::new()
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

#[cfg(all(test, windows))]
mod tests {
    use super::extract_windows_update_installer;

    #[test]
    fn accepts_raw_windows_installer() {
        assert_eq!(
            extract_windows_update_installer(b"MZinstaller").unwrap(),
            b"MZinstaller"
        );
    }
}

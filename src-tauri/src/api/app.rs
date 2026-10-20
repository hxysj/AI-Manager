use crate::core::error::ManagerError;
use crate::core::settings::{serialize_app_settings, write_json_file, AppSettings};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
#[cfg(windows)]
use std::io::{Cursor, Read};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex as SyncMutex};
use tauri::window::Color;
use tauri::{
    Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalRect, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_updater::{Update, Updater, UpdaterExt};
use tokio::sync::Mutex as AsyncMutex;
const QUICK_SWITCH_LABEL: &str = "quick-switch";
const QUICK_SWITCH_EXPANDED_WIDTH: u32 = 560;
const QUICK_SWITCH_EXPANDED_HEIGHT: u32 = 560;
const QUICK_SWITCH_COLLAPSED_WIDTH: u32 = 128;
const QUICK_SWITCH_COLLAPSED_HEIGHT: u32 = 128;
// 主窗口的焦点和尺寸事件会并发触发同步，窗口检查与创建必须在同一临界区。
// 同时保留中心节点的物理坐标，重新创建时不受展开尺寸影响。
static QUICK_SWITCH_WINDOW_POSITION: SyncMutex<Option<PhysicalPosition<f64>>> =
    SyncMutex::new(None);
// 隐藏窗口提前加载，配置和布局就绪后才允许显示。
static QUICK_SWITCH_WINDOW_READY: AtomicBool = AtomicBool::new(false);
// 工具窗口与主面板共用窗口锁，避免迟到的初始化在主面板打开后重新显示。
static READY_TOOL_WINDOWS: LazyLock<SyncMutex<HashSet<String>>> =
    LazyLock::new(|| SyncMutex::new(HashSet::new()));
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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

pub async fn update_status() -> Result<Value, ManagerError> {
    let mut status = update_status_snapshot()?;

    if status
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("idle")
        == "idle"
    {
        status["configured"] = json!(true);
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
        if let Some(status) = downloaded_update_status(true).await {
            return Ok(status);
        }

        let updater = create_public_release_updater(app)?;
        let update = updater
            .check()
            .await
            .map_err(update_request_error)?;

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
        if let Some(status) = downloaded_update_status(true).await {
            return Ok(status);
        }

        let updater = create_public_release_updater(app)?;
        let Some(update) = updater
            .check()
            .await
            .map_err(update_request_error)?
        else {
            return Ok(json!({
              "phase": "not-available",
              "message": "当前已是最新版本。",
              "manual": true,
              "configured": true,
              "isDev": cfg!(debug_assertions)
            }));
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
            .map_err(update_request_error)?;

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
    let _guard = begin_update_task(app, "installing", "正在准备静默安装更新。")?;
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
                  "message": format!("新版本 {} 已下载完成，可重试静默安装。", version),
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

        let status = emit_update_status(
            app,
            json!({
              "phase": "installer-opened",
              "message": "正在静默安装更新，完成后将自动重启应用。",
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
        app.exit(0);
        return Ok(status);
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
    tool_id: Option<&str>,
) -> Result<Value, ManagerError> {
    let _guard = QUICK_SWITCH_WINDOW_POSITION
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let tool_id = tool_id.filter(|id| {
        !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    });
    let window = match app.get_webview_window("main") {
        Some(window) => {
            if let Some(tool_id) = tool_id {
                window
                    .emit("app:open-tool", json!({ "toolId": tool_id }))
                    .map_err(|error| ManagerError::System(error.to_string()))?;
            }
            window
        }
        None => create_main_window(app, tool_id)?,
    };

    hide_tool_windows(app)?;

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
    hide_quick_switch_window(app)?;
    Ok(json!(true))
}

fn tool_window_title(tool_id: &str) -> Result<&'static str, ManagerError> {
    match tool_id {
        "screenshot" => Ok("截图贴图"),
        "image-workbench" => Ok("图片工作台"),
        "json-agent" => Ok("JSON 智能解析"),
        "string-diff" => Ok("差异对比"),
        "image-link-extractor" => Ok("图片链接提取"),
        "coordinate-overlay" => Ok("坐标框预览"),
        "port-monitor" => Ok("端口监测"),
        "network-quality" => Ok("网络质量监测"),
        "git" => Ok("Git 管理"),
        "lan-share" => Ok("设备快传"),
        "codex-pets" => Ok("Codex 宠物"),
        _ => Err(ManagerError::System("工具不存在".to_string())),
    }
}

pub fn open_tool_window(app: &tauri::AppHandle, tool_id: &str) -> Result<Value, ManagerError> {
    let title = tool_window_title(tool_id)?;
    let _guard = QUICK_SWITCH_WINDOW_POSITION
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let label = format!("tool-window-{tool_id}");
    if let Some(window) = app.get_webview_window(&label) {
        let ready = READY_TOOL_WINDOWS
            .lock()
            .map_err(|error| ManagerError::System(error.to_string()))?
            .contains(&label);
        if ready {
            return show_tool_window(app, &window).map(|visible| json!({ "visible": visible }));
        }
        return Ok(json!({ "visible": false }));
    }

    READY_TOOL_WINDOWS
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?
        .remove(&label);
    let large = matches!(tool_id, "image-workbench" | "git" | "network-quality" | "lan-share");
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("index.html?panel=tool&tool={tool_id}").into()),
    )
    .title(format!("{title} · Monkey Thief"))
    .inner_size(if large { 1120.0 } else { 960.0 }, if large { 760.0 } else { 680.0 })
    .min_inner_size(896.0, 576.0)
    .center()
    .focused(false)
    .visible(false)
    .build()
    .map_err(|error| ManagerError::System(error.to_string()))?;

    // 多个工具错开摆放，保留原生标题栏、缩放、最小化和单独关闭能力。
    let count = app.webview_windows().keys().filter(|name| name.starts_with("tool-window-")).count();
    let offset = ((count.saturating_sub(1) % 5) * 24) as i32;
    let position = window.outer_position().map_err(|error| ManagerError::System(error.to_string()))?;
    window.set_position(PhysicalPosition::new(position.x + offset, position.y + offset))
        .map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(json!({ "visible": false }))
}

pub fn mark_tool_window_ready(app: &tauri::AppHandle, tool_id: &str) -> Result<Value, ManagerError> {
    tool_window_title(tool_id)?;
    let _guard = QUICK_SWITCH_WINDOW_POSITION
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let label = format!("tool-window-{tool_id}");
    let Some(window) = app.get_webview_window(&label) else {
        return Ok(json!({ "visible": false }));
    };
    READY_TOOL_WINDOWS
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?
        .insert(label);
    show_tool_window(app, &window).map(|visible| json!({ "visible": visible }))
}

fn show_tool_window(app: &tauri::AppHandle, window: &WebviewWindow) -> Result<bool, ManagerError> {
    if let Some(main) = app.get_webview_window("main") {
        if main.is_visible().map_err(|error| ManagerError::System(error.to_string()))?
            && !main.is_minimized().map_err(|error| ManagerError::System(error.to_string()))?
        {
            return Ok(false);
        }
    }
    window.unminimize().map_err(|error| ManagerError::System(error.to_string()))?;
    window.show().map_err(|error| ManagerError::System(error.to_string()))?;
    window.set_focus().map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(true)
}

fn hide_tool_windows(app: &tauri::AppHandle) -> Result<(), ManagerError> {
    // 主面板显示后销毁工具 WebView，截图窗口和桌面贴图使用独立生命周期。
    let labels = app
        .webview_windows()
        .keys()
        .filter(|label| label.starts_with("tool-window-"))
        .cloned()
        .collect::<Vec<_>>();
    for label in labels {
        if let Some(window) = app.get_webview_window(&label) {
            window.close().map_err(|error| ManagerError::System(error.to_string()))?;
        }
        READY_TOOL_WINDOWS
            .lock()
            .map_err(|error| ManagerError::System(error.to_string()))?
            .remove(&label);
    }
    Ok(())
}

pub fn mark_quick_switch_ready(
    app: &tauri::AppHandle,
    app_settings: &AppSettings,
    quick_switch_collapsed: bool,
) -> Result<Value, ManagerError> {
    QUICK_SWITCH_WINDOW_READY.store(true, Ordering::SeqCst);
    sync_quick_switch_window(app, app_settings, quick_switch_collapsed)?;
    // 已显示的窗口不会再次触发显示事件，重载前端时仍需恢复真实可见状态。
    let visible = app
        .get_webview_window(QUICK_SWITCH_LABEL)
        .map(|window| window.is_visible())
        .transpose()
        .map_err(|error| ManagerError::System(error.to_string()))?
        .unwrap_or(false);
    Ok(json!({ "visible": visible }))
}

pub fn set_quick_switch_collapsed(
    app: &tauri::AppHandle,
    payload: Value,
) -> Result<Value, ManagerError> {
    let mut center = QUICK_SWITCH_WINDOW_POSITION
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let collapsed = payload
        .get("collapsed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if let Some(window) = app.get_webview_window(QUICK_SWITCH_LABEL) {
        // 隐藏后的迟到悬浮请求不能重新展开窗口。
        let collapsed = collapsed
            || !window
                .is_visible()
                .map_err(|error| ManagerError::System(error.to_string()))?;
        let (center_x, center_y, edge) =
            apply_quick_switch_collapsed(&window, collapsed, &mut center)?;
        return Ok(json!({
            "collapsed": collapsed,
            "centerX": center_x,
            "centerY": center_y,
            "edge": edge
        }));
    }

    Ok(json!({ "collapsed": collapsed }))
}

pub fn move_quick_switch_by(app: &tauri::AppHandle, payload: Value) -> Result<Value, ManagerError> {
    let mut center = QUICK_SWITCH_WINDOW_POSITION
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let Some(window) = app.get_webview_window(QUICK_SWITCH_LABEL) else {
        return Ok(json!(true));
    };
    if !window
        .is_visible()
        .map_err(|error| ManagerError::System(error.to_string()))?
    {
        return Ok(json!(true));
    }
    let position = window
        .outer_position()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let x = payload.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = payload.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    let scale = window
        .scale_factor()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let size = window
        .outer_size()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let previous_center = center.unwrap_or_else(|| {
        PhysicalPosition::new(
            position.x as f64 + size.width as f64 / 2.0,
            position.y as f64 + size.height as f64 / 2.0,
        )
    });
    let mut next_center =
        PhysicalPosition::new(previous_center.x + x * scale, previous_center.y + y * scale);
    let mut next_position = PhysicalPosition::new(
        position.x + (x * scale).round() as i32,
        position.y + (y * scale).round() as i32,
    );
    let cursor = window
        .cursor_position()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let monitor = window
        .available_monitors()
        .map_err(|error| ManagerError::System(error.to_string()))?
        .into_iter()
        .find(|monitor| {
            let origin = monitor.position();
            let bounds = monitor.size();
            cursor.x >= origin.x as f64
                && cursor.x < origin.x as f64 + bounds.width as f64
                && cursor.y >= origin.y as f64
                && cursor.y < origin.y as f64 + bounds.height as f64
        })
        .or(window
            .monitor_from_point(previous_center.x, previous_center.y)
            .map_err(|error| ManagerError::System(error.to_string()))?);
    let mut edge = "";
    if let Some(monitor) = monitor {
        // 中心节点允许贴边，展开后的工具由前端根据剩余空间重新排布。
        (next_position, next_center) =
            quick_switch_window_layout(next_center, size, scale, monitor.work_area());
        edge = quick_switch_window_edge(next_center, scale, monitor.work_area());
    }

    window
        .set_position(next_position)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    *center = Some(next_center);

    Ok(json!({
        "centerX": (next_center.x - next_position.x as f64) / scale,
        "centerY": (next_center.y - next_position.y as f64) / scale,
        "edge": edge
    }))
}

pub fn sync_quick_switch_window(
    app: &tauri::AppHandle,
    app_settings: &AppSettings,
    _quick_switch_collapsed: bool,
) -> Result<(), ManagerError> {
    let mut center = QUICK_SWITCH_WINDOW_POSITION
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;
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
    // 托盘、任务栏还原和工具按钮打开主面板时，统一隐藏独立工具。
    if main_visible && !main_minimized {
        hide_tool_windows(app)?;
    }
    if !app_settings.system.quick_switch_visible {
        destroy_quick_switch_window(app)?;
        return Ok(());
    }
    // 主页面显示期间预热并保留 WebView，切换时只改变可见性。
    let window = create_quick_switch_window(app, true, &mut center)?;
    let visible = window
        .is_visible()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    if !main_visible || main_minimized {
        if QUICK_SWITCH_WINDOW_READY.load(Ordering::SeqCst) && !visible {
            let (center_x, center_y, edge) =
                apply_quick_switch_collapsed(&window, true, &mut center)?;
            window
                .emit("quick-switch:visibility-changed", json!({
                    "visible": true,
                    "centerX": center_x,
                    "centerY": center_y,
                    "edge": edge
                }))
                .map_err(|error| ManagerError::System(error.to_string()))?;
            window
                .show()
                .map_err(|error| ManagerError::System(error.to_string()))?;
        }
    } else if visible {
        hide_quick_switch_window(app)?;
    }

    Ok(())
}

fn create_main_window(
    app: &tauri::AppHandle,
    tool_id: Option<&str>,
) -> Result<WebviewWindow, ManagerError> {
    let url = tool_id
        .map(|id| format!("index.html?view=tools&tool={id}"))
        .unwrap_or_else(|| "index.html".to_string());
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
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
    center: &mut Option<PhysicalPosition<f64>>,
) -> Result<WebviewWindow, ManagerError> {
    if let Some(window) = app.get_webview_window(QUICK_SWITCH_LABEL) {
        return Ok(window);
    }
    QUICK_SWITCH_WINDOW_READY.store(false, Ordering::SeqCst);
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
    .visible(false)
    .build()
    .map_err(|error| ManagerError::System(error.to_string()))?;

    window
        .set_always_on_top(true)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    set_quick_switch_size(&window, width, height)?;
    *center = Some(position_quick_switch_window(&window, *center)?);

    Ok(window)
}

fn hide_quick_switch_window(app: &tauri::AppHandle) -> Result<(), ManagerError> {
    if let Some(window) = app.get_webview_window(QUICK_SWITCH_LABEL) {
        window
            .hide()
            .map_err(|error| ManagerError::System(error.to_string()))?;
        window
            .emit("quick-switch:visibility-changed", json!({ "visible": false }))
            .map_err(|error| ManagerError::System(error.to_string()))?;
    }
    Ok(())
}

fn destroy_quick_switch_window(app: &tauri::AppHandle) -> Result<(), ManagerError> {
    QUICK_SWITCH_WINDOW_READY.store(false, Ordering::SeqCst);
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
    center: &mut Option<PhysicalPosition<f64>>,
) -> Result<(f64, f64, &'static str), ManagerError> {
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
    let scale = window
        .scale_factor()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let previous_center = center.unwrap_or_else(|| {
        PhysicalPosition::new(
            position.x as f64 + size.width as f64 / 2.0,
            position.y as f64 + size.height as f64 / 2.0,
        )
    });
    let monitor = window
        .monitor_from_point(previous_center.x, previous_center.y)
        .map_err(|error| ManagerError::System(error.to_string()))?
        .or_else(|| window.primary_monitor().ok().and_then(|monitor| monitor));
    let Some(monitor) = monitor else {
        set_quick_switch_size(window, width, height)?;
        let next_position = window
            .outer_position()
            .map_err(|error| ManagerError::System(error.to_string()))?;
        *center = Some(previous_center);
        return Ok((
            (previous_center.x - next_position.x as f64) / scale,
            (previous_center.y - next_position.y as f64) / scale,
            "",
        ));
    };
    let physical_size = PhysicalSize::new(
        (width as f64 * scale).round() as u32,
        (height as f64 * scale).round() as u32,
    );
    let (next_position, next_center) =
        quick_switch_window_layout(previous_center, physical_size, scale, monitor.work_area());

    set_quick_switch_size(window, width, height)?;
    window
        .set_position(next_position)
        .map_err(|error| ManagerError::System(error.to_string()))?;
    *center = Some(next_center);

    Ok((
        (next_center.x - next_position.x as f64) / scale,
        (next_center.y - next_position.y as f64) / scale,
        quick_switch_window_edge(next_center, scale, monitor.work_area()),
    ))
}

fn position_quick_switch_window(
    window: &WebviewWindow,
    center: Option<PhysicalPosition<f64>>,
) -> Result<PhysicalPosition<f64>, ManagerError> {
    let size = window
        .outer_size()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let monitors = window
        .available_monitors()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let monitor = center
        .and_then(|center| {
            monitors.into_iter().find(|monitor| {
                let origin = monitor.position();
                let bounds = monitor.size();
                center.x >= origin.x as f64
                    && center.x < origin.x as f64 + bounds.width as f64
                    && center.y >= origin.y as f64
                    && center.y < origin.y as f64 + bounds.height as f64
            })
        })
        .or(window
            .primary_monitor()
            .map_err(|error| ManagerError::System(error.to_string()))?);
    let Some(monitor) = monitor else {
        let position = window
            .outer_position()
            .map_err(|error| ManagerError::System(error.to_string()))?;
        return Ok(PhysicalPosition::new(
            position.x as f64 + size.width as f64 / 2.0,
            position.y as f64 + size.height as f64 / 2.0,
        ));
    };
    let work_area = monitor.work_area();
    let center = center.unwrap_or_else(|| {
        PhysicalPosition::new(
            work_area.position.x as f64 + work_area.size.width as f64
                - size.width as f64 / 2.0
                - 16.0,
            work_area.position.y as f64 + work_area.size.height as f64
                - size.height as f64 / 2.0
                - 12.0,
        )
    });
    // 显示器移除或工作区变化后，将原位置限制在当前屏幕内。
    let scale = window
        .scale_factor()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let (position, center) = quick_switch_window_layout(center, size, scale, work_area);

    window
        .set_position(position)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(center)
}

// 贴边依据真实屏幕坐标判断，避免把收起后的局部中心误认为屏幕边缘。
fn quick_switch_window_edge(
    center: PhysicalPosition<f64>,
    scale: f64,
    work_area: &PhysicalRect<i32, u32>,
) -> &'static str {
    let x = center.x - work_area.position.x as f64;
    let y = center.y - work_area.position.y as f64;
    let half_width = QUICK_SWITCH_COLLAPSED_WIDTH as f64 * scale / 2.0;
    let half_height = QUICK_SWITCH_COLLAPSED_HEIGHT as f64 * scale / 2.0;
    if x <= half_width + scale {
        "left"
    } else if x >= work_area.size.width as f64 - half_width - scale {
        "right"
    } else if y <= half_height + scale {
        "top"
    } else if y >= work_area.size.height as f64 - half_height - scale {
        "bottom"
    } else {
        ""
    }
}

// 移动范围只限制中心节点，外层窗口贴边后中心仍可继续移动。
fn quick_switch_window_layout(
    mut center: PhysicalPosition<f64>,
    size: PhysicalSize<u32>,
    scale: f64,
    work_area: &PhysicalRect<i32, u32>,
) -> (PhysicalPosition<i32>, PhysicalPosition<f64>) {
    let min_center_x =
        work_area.position.x as f64 + QUICK_SWITCH_COLLAPSED_WIDTH as f64 * scale / 2.0;
    let min_center_y =
        work_area.position.y as f64 + QUICK_SWITCH_COLLAPSED_HEIGHT as f64 * scale / 2.0;
    let max_center_x = (work_area.position.x as f64 + work_area.size.width as f64
        - QUICK_SWITCH_COLLAPSED_WIDTH as f64 * scale / 2.0)
        .max(min_center_x);
    let max_center_y = (work_area.position.y as f64 + work_area.size.height as f64
        - QUICK_SWITCH_COLLAPSED_HEIGHT as f64 * scale / 2.0)
        .max(min_center_y);
    center.x = center.x.clamp(min_center_x, max_center_x);
    center.y = center.y.clamp(min_center_y, max_center_y);
    let max_x = (work_area.position.x + work_area.size.width as i32 - size.width as i32)
        .max(work_area.position.x);
    let max_y = (work_area.position.y + work_area.size.height as i32 - size.height as i32)
        .max(work_area.position.y);
    let x =
        ((center.x - size.width as f64 / 2.0).round() as i32).clamp(work_area.position.x, max_x);
    let y =
        ((center.y - size.height as f64 / 2.0).round() as i32).clamp(work_area.position.y, max_y);
    (PhysicalPosition::new(x, y), center)
}

fn set_quick_switch_size(
    window: &WebviewWindow,
    width: u32,
    height: u32,
) -> Result<(), ManagerError> {
    let size = LogicalSize::new(width as f64, height as f64);

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
      "configured": true,
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
      "configured": true,
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
          "configured": true,
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

fn create_public_release_updater(app: &tauri::AppHandle) -> Result<Updater, ManagerError> {
    app.updater_builder()
        .build()
        .map_err(update_request_error)
}

fn update_request_error(error: impl std::error::Error) -> ManagerError {
    // 保留网络错误链，避免只显示请求地址而丢失 DNS、代理或 TLS 失败原因。
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str("；");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    ManagerError::System(format!("更新请求失败：{message}"))
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
    let current_exe_path = std::env::current_exe()?;
    let temp_dir = installer_path
        .parent()
        .ok_or_else(|| ManagerError::Path("无法解析更新临时目录".to_string()))?;
    let script_path = temp_dir.join("install-update.ps1");
    let script = build_windows_silent_update_script(
        std::process::id(),
        &current_exe_path,
        &installer_path,
        &script_path,
    );
    tokio::fs::write(&script_path, script).await?;

    let mut command = std::process::Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            &script_path.to_string_lossy(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(())
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
fn build_windows_silent_update_script(
    current_process_id: u32,
    current_exe_path: &Path,
    installer_path: &Path,
    script_path: &Path,
) -> String {
    format!(
        r#"$ErrorActionPreference = 'Stop'
$ProcessId = {process_id}
$CurrentExePath = {current_exe_path}
$InstallerPath = {installer_path}
$ScriptPath = {script_path}

Wait-Process -Id $ProcessId -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300
try {{
  $installer = Start-Process -FilePath $InstallerPath -ArgumentList '/S /UPDATE' -WindowStyle Hidden -Wait -PassThru
}} catch {{
  $installer = $null
}}
if ($null -eq $installer -or $installer.ExitCode -ne 0) {{
  # 静默安装失败时打开同一个安装包，让用户按安装向导手动处理。
  Start-Process -FilePath $InstallerPath -WindowStyle Normal
}} elseif (Test-Path -LiteralPath $CurrentExePath) {{
  Start-Process -FilePath $CurrentExePath
}}
Remove-Item -LiteralPath $ScriptPath -Force -ErrorAction SilentlyContinue
"#,
        process_id = current_process_id,
        current_exe_path = to_powershell_literal(&current_exe_path.to_string_lossy()),
        installer_path = to_powershell_literal(&installer_path.to_string_lossy()),
        script_path = to_powershell_literal(&script_path.to_string_lossy())
    )
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

#[cfg(test)]
mod quick_switch_tests {
    use super::*;

    #[test]
    fn expanded_center_continues_after_window_reaches_edge() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1080),
        };
        let size = PhysicalSize::new(560, 560);
        let (first, _) =
            quick_switch_window_layout(PhysicalPosition::new(280.0, 540.0), size, 1.0, &area);
        let (next, center) =
            quick_switch_window_layout(PhysicalPosition::new(64.0, 540.0), size, 1.0, &area);
        assert_eq!(first.x, 0);
        assert_eq!(next.x, 0);
        assert_eq!(center.x - next.x as f64, 64.0);
    }

    #[test]
    fn pet_edges_follow_monitor_origin_and_scale() {
        let area = PhysicalRect {
            position: PhysicalPosition::new(-3840, 40),
            size: PhysicalSize::new(3840, 2120),
        };
        for scale in [1.0, 1.5, 2.0] {
            let left = area.position.x as f64 + 64.0 * scale;
            let right = area.position.x as f64 + area.size.width as f64 - 64.0 * scale;
            let top = area.position.y as f64 + 64.0 * scale;
            let bottom = area.position.y as f64 + area.size.height as f64 - 64.0 * scale;
            for (center, expected) in [
                (PhysicalPosition::new(left, 1000.0), "left"),
                (PhysicalPosition::new(right, 1000.0), "right"),
                (PhysicalPosition::new(-1920.0, top), "top"),
                (PhysicalPosition::new(-1920.0, bottom), "bottom"),
                (PhysicalPosition::new(left, top), "left"),
                (PhysicalPosition::new(-1920.0, 1000.0), ""),
                (PhysicalPosition::new(left + 16.0 * scale, 1000.0), ""),
            ] {
                assert_eq!(quick_switch_window_edge(center, scale, &area), expected);
            }
        }
    }

    #[test]
    fn center_reaches_all_edges_and_corners_at_different_scales() {
        for scale in [1.0, 1.5, 2.0] {
            let area = PhysicalRect {
                position: PhysicalPosition::new(-3840, 40),
                size: PhysicalSize::new(3840, 2120),
            };
            let left = area.position.x as f64 + 64.0 * scale;
            let right = area.position.x as f64 + area.size.width as f64 - 64.0 * scale;
            let top = area.position.y as f64 + 64.0 * scale;
            let bottom = area.position.y as f64 + area.size.height as f64 - 64.0 * scale;
            for (x, y) in [
                (left, 1000.0),
                (right, 1000.0),
                (left, top),
                (right, top),
                (-1920.0, top),
                (-1920.0, bottom),
                (left, bottom),
                (right, bottom),
            ] {
                let center = PhysicalPosition::new(x, y);
                let expanded_size =
                    PhysicalSize::new((560.0 * scale) as u32, (560.0 * scale) as u32);
                let (expanded_position, expanded_center) =
                    quick_switch_window_layout(center, expanded_size, scale, &area);
                assert_eq!(expanded_center, center);
                assert!(expanded_position.x >= area.position.x);
                assert!(expanded_position.y >= area.position.y);
                assert!(
                    expanded_position.x + expanded_size.width as i32
                        <= area.position.x + area.size.width as i32
                );
                assert!(
                    expanded_position.y + expanded_size.height as i32
                        <= area.position.y + area.size.height as i32
                );
                let collapsed_size =
                    PhysicalSize::new((128.0 * scale) as u32, (128.0 * scale) as u32);
                let (collapsed_position, collapsed_center) =
                    quick_switch_window_layout(expanded_center, collapsed_size, scale, &area);
                assert_eq!(collapsed_center, center);
                assert_eq!((center.x - collapsed_position.x as f64) / scale, 64.0);
                assert_eq!((center.y - collapsed_position.y as f64) / scale, 64.0);
                let (restored_position, restored_center) =
                    quick_switch_window_layout(collapsed_center, expanded_size, scale, &area);
                assert_eq!(restored_center, center);
                assert_eq!(restored_position, expanded_position);
            }
            let (_, clamped) = quick_switch_window_layout(
                PhysicalPosition::new(-10000.0, 10000.0),
                PhysicalSize::new((560.0 * scale) as u32, (560.0 * scale) as u32),
                scale,
                &area,
            );
            assert_eq!(clamped, PhysicalPosition::new(left, bottom));
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{extract_windows_update_installer, update_request_error};

    #[test]
    fn update_request_error_preserves_connection_failure_cause() {
        tauri::async_runtime::block_on(async {
            // 本地服务主动断开连接，验证真实请求错误的底层原因仍能传到界面。
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 1024];
                tokio::io::AsyncReadExt::read(&mut stream, &mut request)
                    .await
                    .unwrap();
            });
            let error = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(format!("http://{address}"))
                .send()
                .await
                .unwrap_err();
            let cause = std::error::Error::source(&error).unwrap().to_string();
            let message = update_request_error(error).to_string();
            assert!(message.contains("更新请求失败"));
            assert!(message.contains(&cause));
            assert!(message.contains("connection closed before message completed"));
            server.await.unwrap();
        });
    }

    #[test]
    fn accepts_raw_windows_installer() {
        assert_eq!(
            extract_windows_update_installer(b"MZinstaller").unwrap(),
            b"MZinstaller"
        );
    }
}

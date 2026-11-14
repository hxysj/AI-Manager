use crate::core::{error::ManagerError, settings::AssistantSettings};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

#[derive(Clone, Default)]
pub struct DesktopAssistant {
    inner: Arc<Mutex<DesktopState>>,
    #[cfg(windows)]
    wake: Arc<std::sync::OnceLock<std::sync::mpsc::SyncSender<()>>>,
}

#[derive(Default)]
struct DesktopState {
    enabled: bool,
    shortcut: String,
    error: String,
    ready: bool,
    requested: bool,
}

pub fn sync(app: &AppHandle, settings: &AssistantSettings) {
    let service = app.state::<DesktopAssistant>();
    {
        let mut state = service.inner.lock().unwrap();
        state.enabled = settings.enabled;
        state.shortcut = settings.shortcut.clone();
        if !settings.enabled {
            state.requested = false;
            state.ready = false;
            state.error.clear();
        }
    }
    #[cfg(windows)]
    {
        if settings.enabled {
            service
                .wake
                .get_or_init(|| start_hotkey(app.clone(), service.inner().clone()));
        }
        if let Some(wake) = service.wake.get() {
            let _ = wake.try_send(());
        }
    }
    if !settings.enabled {
        if let Some(window) = app.get_webview_window("assistant-desktop") {
            let _ = window.destroy();
        }
    }
}

pub fn dispatch(app: &AppHandle, action: &str) -> Result<Value, ManagerError> {
    let service = app.state::<DesktopAssistant>();
    match action {
        "status" => {
            let state = service.inner.lock().unwrap();
            Ok(
                json!({ "supported": cfg!(windows), "enabled": state.enabled, "error": state.error }),
            )
        }
        "open" => {
            open_input(app)?;
            Ok(Value::Null)
        }
        "ready" => {
            let mut state = service.inner.lock().unwrap();
            state.ready = true;
            let visible = state.enabled && state.requested;
            drop(state);
            if visible {
                if let Some(window) = app.get_webview_window("assistant-desktop") {
                    window.show().map_err(failure)?;
                    window.set_focus().map_err(failure)?;
                }
            }
            Ok(json!({ "visible": visible }))
        }
        "close" => {
            service.inner.lock().unwrap().requested = false;
            if let Some(window) = app.get_webview_window("assistant-desktop") {
                window.hide().map_err(failure)?;
            }
            Ok(Value::Null)
        }
        "drag" => {
            if let Some(window) = app.get_webview_window("assistant-desktop") {
                window.start_dragging().map_err(failure)?;
            }
            Ok(Value::Null)
        }
        _ => Err(ManagerError::UnknownChannel(format!(
            "assistant:desktop-{action}"
        ))),
    }
}

fn failure(error: impl std::fmt::Display) -> ManagerError {
    ManagerError::System(error.to_string())
}

fn open_input(app: &AppHandle) -> Result<(), ManagerError> {
    let service = app.state::<DesktopAssistant>();
    let mut state = service.inner.lock().unwrap();
    if !state.enabled {
        return Err(failure("应用助手已关闭"));
    }
    state.requested = true;
    let ready = state.ready;
    drop(state);
    let window = match app.get_webview_window("assistant-desktop") {
        Some(window) => window,
        None => WebviewWindowBuilder::new(
            app,
            "assistant-desktop",
            WebviewUrl::App("index.html?panel=assistant-input".into()),
        )
        .title("和助手说话")
        .inner_size(440.0, 300.0)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .transparent(true)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build()
        .map_err(failure)?,
    };
    let cursor = window.cursor_position().map_err(failure)?;
    if let Some(monitor) = window
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(failure)?
    {
        let scale = monitor.scale_factor();
        let size = PhysicalSize::new(
            (440.0 * scale).round() as u32,
            (300.0 * scale).round() as u32,
        );
        let position = input_position(cursor, monitor.work_area(), size, scale);
        window.set_position(position).map_err(failure)?;
        window.set_size(size).map_err(failure)?;
    }
    if ready {
        window.show().map_err(failure)?;
        window.set_focus().map_err(failure)?;
        window.emit("assistant:desktop-open", ()).map_err(failure)?;
    }
    Ok(())
}

// 鼠标坐标、屏幕工作区和窗口尺寸统一使用物理像素，支持负坐标副屏与缩放。
fn input_position(
    cursor: PhysicalPosition<f64>,
    area: &tauri::PhysicalRect<i32, u32>,
    size: PhysicalSize<u32>,
    scale: f64,
) -> PhysicalPosition<i32> {
    let min_x = area.position.x as f64;
    let min_y = area.position.y as f64;
    let max_x = (min_x + area.size.width as f64 - size.width as f64).max(min_x);
    let max_y = (min_y + area.size.height as f64 - size.height as f64).max(min_y);
    let x = if cursor.x + 12.0 * scale + size.width as f64 > min_x + area.size.width as f64 {
        cursor.x - size.width as f64 - 12.0 * scale
    } else {
        cursor.x + 12.0 * scale
    };
    let y = if cursor.y + 16.0 * scale + size.height as f64 > min_y + area.size.height as f64 {
        cursor.y - size.height as f64 - 16.0 * scale
    } else {
        cursor.y + 16.0 * scale
    };
    PhysicalPosition::new(
        x.clamp(min_x, max_x).round() as i32,
        y.clamp(min_y, max_y).round() as i32,
    )
}

pub fn window_event(app: &AppHandle, label: &str, event: &tauri::WindowEvent) {
    if label == "assistant-desktop" && matches!(event, tauri::WindowEvent::Destroyed) {
        let service = app.state::<DesktopAssistant>();
        let mut state = service.inner.lock().unwrap();
        state.ready = false;
        state.requested = false;
    }
}

#[cfg(windows)]
fn start_hotkey(app: AppHandle, service: DesktopAssistant) -> std::sync::mpsc::SyncSender<()> {
    use std::{ptr::null_mut, sync::mpsc, time::Duration};
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT},
        WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY},
    };
    let (wake, changes) = mpsc::sync_channel(1);
    let errors = service.clone();
    let result = std::thread::Builder::new()
        .name("应用助手快捷键".into())
        .spawn(move || {
            let mut signature = String::new();
            let mut registered = false;
            loop {
                let (enabled, shortcut) = {
                    let state = service.inner.lock().unwrap();
                    (state.enabled, state.shortcut.clone())
                };
                let next = format!("{enabled}:{shortcut}");
                if next != signature {
                    if registered {
                        unsafe {
                            UnregisterHotKey(null_mut(), 0x4d41);
                        }
                    }
                    registered = false;
                    signature = next;
                    let error = if enabled {
                        match crate::api::screenshot::parse_shortcut(&shortcut) {
                            Ok((modifiers, code)) => {
                                registered = unsafe {
                                    RegisterHotKey(
                                        null_mut(),
                                        0x4d41,
                                        modifiers | MOD_NOREPEAT,
                                        code,
                                    )
                                } != 0;
                                if registered {
                                    String::new()
                                } else {
                                    format!("助手快捷键 {shortcut} 已被占用，请更换快捷键")
                                }
                            }
                            Err(error) => error.to_string(),
                        }
                    } else {
                        String::new()
                    };
                    service.inner.lock().unwrap().error = error;
                    let _ = app.emit("assistant:desktop-status", ());
                }
                if !enabled {
                    if changes.recv().is_err() {
                        break;
                    }
                    continue;
                }
                match changes.recv_timeout(Duration::from_millis(50)) {
                    Ok(()) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let mut message: MSG = unsafe { std::mem::zeroed() };
                while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
                    if message.message == WM_HOTKEY && message.wParam == 0x4d41 && registered {
                        let handle = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            if let Err(error) = open_input(&handle) {
                                handle
                                    .state::<DesktopAssistant>()
                                    .inner
                                    .lock()
                                    .unwrap()
                                    .error = error.to_string();
                                let _ = handle.emit("assistant:desktop-status", ());
                            }
                        });
                    }
                }
            }
            if registered {
                unsafe {
                    UnregisterHotKey(null_mut(), 0x4d41);
                }
            }
        });
    if let Err(error) = result {
        errors.inner.lock().unwrap().error = format!("无法启动助手快捷键：{error}");
    }
    wake
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_popup_stays_inside_work_area_for_edges_and_scaled_negative_monitor() {
        for (origin, scale) in [((0, 0), 1.0), ((-2560, -200), 1.5)] {
            let area = tauri::PhysicalRect {
                position: PhysicalPosition::new(origin.0, origin.1),
                size: PhysicalSize::new(1920, 1080),
            };
            let size = PhysicalSize::new((440.0 * scale) as u32, (300.0 * scale) as u32);
            for (x, y) in [(0, 0), (1919, 0), (960, 540), (0, 1079), (1919, 1079)] {
                let position = input_position(
                    PhysicalPosition::new((origin.0 + x) as f64, (origin.1 + y) as f64),
                    &area,
                    size,
                    scale,
                );
                assert!(position.x >= origin.0 && position.y >= origin.1);
                assert!(position.x as i64 + size.width as i64 <= origin.0 as i64 + 1920);
                assert!(position.y as i64 + size.height as i64 <= origin.1 as i64 + 1080);
                if x == 0 && y == 0 {
                    assert_eq!(position.x, origin.0 + (12.0 * scale).round() as i32);
                    assert_eq!(position.y, origin.1 + (16.0 * scale).round() as i32);
                }
            }
        }
    }
}

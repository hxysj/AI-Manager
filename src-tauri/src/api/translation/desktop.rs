use crate::core::{error::ManagerError, settings::TranslationAgentSettings};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

#[cfg(windows)]
mod windows;

#[derive(Clone, Default)]
pub struct DesktopTranslator {
    inner: Arc<Mutex<DesktopState>>,
    #[cfg(windows)]
    wake: Arc<std::sync::OnceLock<std::sync::mpsc::SyncSender<()>>>,
}

#[derive(Default)]
struct DesktopState {
    enabled: bool,
    target_language: String,
    selection: Value,
    hotkey_error: String,
}

pub fn sync(app: &AppHandle, settings: &TranslationAgentSettings) {
    let service = app.state::<DesktopTranslator>();
    {
        let mut state = service.inner.lock().unwrap();
        state.enabled = settings.enabled;
        state.target_language = settings.target_language.clone();
        if !settings.enabled {
            state.selection = Value::Null;
            state.hotkey_error.clear();
        }
    }
    #[cfg(windows)]
    {
        if settings.enabled {
            service.wake.get_or_init(|| windows::start(app.clone(), service.inner().clone()));
        }
        if let Some(wake) = service.wake.get() {
            let _ = wake.try_send(());
        }
    }
    if !settings.enabled {
        if let Some(window) = app.get_webview_window("translation-desktop") {
            let _ = window.destroy();
        }
    }
}

pub fn dispatch(app: &AppHandle, action: &str) -> Result<Value, ManagerError> {
    let service = app.state::<DesktopTranslator>();
    match action {
        "status" => {
            let state = service.inner.lock().unwrap();
            Ok(json!({ "supported": cfg!(windows), "enabled": state.enabled, "error": state.hotkey_error }))
        }
        "ready" => {
            let state = service.inner.lock().unwrap();
            if !state.enabled {
                return Err(ManagerError::System("划词翻译已停用".into()));
            }
            let mut selection = state.selection.clone();
            selection["targetLanguage"] = json!(state.target_language);
            drop(state);
            if let Some(window) = app.get_webview_window("translation-desktop") {
                window.show().map_err(|error| ManagerError::System(error.to_string()))?;
                window.set_focus().map_err(|error| ManagerError::System(error.to_string()))?;
            }
            Ok(selection)
        }
        "close" => {
            service.inner.lock().unwrap().selection = Value::Null;
            if let Some(window) = app.get_webview_window("translation-desktop") {
                window.destroy().map_err(|error| ManagerError::System(error.to_string()))?;
            }
            Ok(Value::Null)
        }
        "drag" => {
            if let Some(window) = app.get_webview_window("translation-desktop") {
                window.start_dragging().map_err(|error| ManagerError::System(error.to_string()))?;
            }
            Ok(Value::Null)
        }
        _ => Err(ManagerError::UnknownChannel(format!("translation:desktop-{action}"))),
    }
}

fn checked_text(text: String) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("没有选中文字。请先选中文字，再按 Ctrl+Alt+T；也可复制后按 Ctrl+Alt+Shift+T。".into());
    }
    if text.chars().count() > 30_000 {
        return Err("单次最多翻译 30000 个字符，请缩小选择范围".into());
    }
    Ok(text)
}

#[cfg(windows)]
fn show_selection(app: &AppHandle, result: Result<String, String>, x: i32, y: i32) {
    let mut selection = match result.and_then(checked_text) {
        Ok(text) => json!({ "text": text, "error": "" }),
        Err(error) => json!({ "text": "", "error": error }),
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let service = handle.state::<DesktopTranslator>();
        let mut state = service.inner.lock().unwrap();
        if !state.enabled { return; }
        selection["targetLanguage"] = json!(state.target_language);
        state.selection = selection.clone();
        drop(state);
        let result = (|| -> tauri::Result<()> {
            let existing = handle.get_webview_window("translation-desktop");
            let window = match existing.as_ref() {
                Some(window) => window.clone(),
                None => WebviewWindowBuilder::new(
                    &handle,
                    "translation-desktop",
                    WebviewUrl::App("index.html?panel=translation".into()),
                )
                .title("划词翻译")
                .inner_size(468.0, 492.0)
                .min_inner_size(300.0, 240.0)
                .decorations(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .focused(false)
                .visible(false)
                .build()?,
            };
            // 坐标来自原生物理像素，使用对应屏幕的缩放和边界，兼容副屏负坐标。
            if let Some(monitor) = window.monitor_from_point(x as f64, y as f64)? {
                let origin = monitor.position();
                let size = monitor.size();
                let popup = window.outer_size()?;
                let position = fit_position((x, y), (origin.x, origin.y), (size.width, size.height), (popup.width, popup.height));
                window.set_position(PhysicalPosition::new(position.0, position.1))?;
            }
            if existing.is_some() && window.is_visible()? {
                window.emit("translation:desktop-selection", &selection)?;
                window.set_focus()?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            service.inner.lock().unwrap().hotkey_error = format!("无法打开桌面翻译窗口：{error}");
        }
    });
}

pub fn window_event(app: &AppHandle, label: &str, event: &tauri::WindowEvent) {
    if label == "translation-desktop" && matches!(event, tauri::WindowEvent::Destroyed) {
        app.state::<DesktopTranslator>().inner.lock().unwrap().selection = Value::Null;
    }
}

fn fit_position(cursor: (i32, i32), origin: (i32, i32), screen: (u32, u32), popup: (u32, u32)) -> (i32, i32) {
    let max_x = (origin.0 as i64 + screen.0 as i64 - popup.0 as i64).max(origin.0 as i64);
    let max_y = (origin.1 as i64 + screen.1 as i64 - popup.1 as i64).max(origin.1 as i64);
    ((cursor.0 as i64 + 12).clamp(origin.0 as i64, max_x) as i32,
     (cursor.1 as i64 + 16).clamp(origin.1 as i64, max_y) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_keeps_unicode_and_format_but_rejects_empty_or_oversized_text() {
        assert_eq!(checked_text("  第一行\nhello 😀".into()).unwrap(), "  第一行\nhello 😀");
        assert!(checked_text(" \n\t".into()).is_err());
        assert!(checked_text("字".repeat(30_000)).is_ok());
        assert!(checked_text("😀".repeat(30_001)).is_err());
    }

    #[test]
    fn popup_stays_on_the_selected_monitor_including_negative_coordinates() {
        assert_eq!(fit_position((1910, 1070), (0, 0), (1920, 1080), (468, 492)), (1452, 588));
        assert_eq!(fit_position((-20, 1070), (-1920, 0), (1920, 1080), (702, 738)), (-702, 342));
        assert_eq!(fit_position((10, 20), (0, 0), (320, 200), (468, 492)), (0, 0));
    }
}

use super::*;
use crate::core::desktop;
use clipboard_rs::{common::RustImage, Clipboard, RustImageData};

pub fn capture(
    monitors: &[tauri::Monitor], left: i32, top: i32, width: u32, height: u32,
) -> Result<DynamicImage, ManagerError> {
    let native = xcap::Monitor::all().map_err(failure)?;
    let mut output = image::RgbaImage::new(width, height);
    for (index, monitor) in monitors.iter().enumerate() {
        let screen = native.iter().find(|screen| {
            screen.name().ok().as_ref() == monitor.name()
        }).or_else(|| native.get(index)).ok_or_else(|| failure("显示器列表已改变，请重新截图"))?;
        let tile = screen.capture_image().map_err(|error| {
            failure(format!("桌面捕获失败，请检查屏幕录制或桌面门户权限：{error}"))
        })?;
        let tile = if tile.dimensions() == (monitor.size().width, monitor.size().height) {
            tile
        } else {
            image::imageops::resize(&tile, monitor.size().width, monitor.size().height,
                image::imageops::FilterType::Triangle)
        };
        image::imageops::replace(&mut output, &tile,
            i64::from(monitor.position().x - left), i64::from(monitor.position().y - top));
    }
    Ok(DynamicImage::ImageRgba8(output))
}

pub fn copy_image(_app: &AppHandle, image: &DynamicImage) -> Result<(), ManagerError> {
    desktop::clipboard(|context| context.set_image(RustImageData::from_dynamic_image(image.clone())))
}

pub fn read_clipboard() -> Result<Vec<DynamicImage>, ManagerError> {
    if let Ok(image) = desktop::clipboard(|context| context.get_image()?.get_dynamic_image()) {
        return Ok(vec![image]);
    }
    let mut images = Vec::new();
    for file in desktop::clipboard_files()? {
        if let Ok(image) = image::open(file) { images.push(image); }
    }
    if images.is_empty() { return Err(failure("剪贴板中没有图片或可读取的图片文件")); }
    Ok(images)
}

pub fn smart_selection(app: &AppHandle, x: i32, y: i32) -> Result<Vec<Value>, ManagerError> {
    let monitors = app.available_monitors().map_err(failure)?;
    let mut regions = Vec::new();
    for window in xcap::Window::all().map_err(failure)? {
        if window.pid().ok() == Some(std::process::id()) || window.is_minimized().unwrap_or(true) {
            continue;
        }
        let (mut wx, mut wy) = (window.x().map_err(failure)?, window.y().map_err(failure)?);
        let (mut width, mut height) = (window.width().map_err(failure)?, window.height().map_err(failure)?);
        if cfg!(target_os = "macos") {
            let screen = window.current_monitor().map_err(failure)?;
            if let Some(monitor) = monitors.iter().find(|monitor| monitor.name() == screen.name().ok().as_ref()) {
                let scale = monitor.scale_factor();
                wx = monitor.position().x + ((wx - screen.x().map_err(failure)?) as f64 * scale).round() as i32;
                wy = monitor.position().y + ((wy - screen.y().map_err(failure)?) as f64 * scale).round() as i32;
                width = (width as f64 * scale).round() as u32;
                height = (height as f64 * scale).round() as u32;
            }
        }
        if width > 0 && height > 0 && x >= wx && y >= wy
            && i64::from(x) < i64::from(wx) + i64::from(width)
            && i64::from(y) < i64::from(wy) + i64::from(height) {
            regions.push(json!({"x": wx, "y": wy, "width": width, "height": height,
                "kind": "window", "label": window.title().unwrap_or_default()}));
        }
    }
    Ok(regions)
}

pub fn cursor_in_rect(app: &AppHandle, rect: Option<&CaptureRect>) -> bool {
    let Some(rect) = rect else { return false; };
    desktop::physical_cursor_position(app).is_ok_and(|(x, y)| {
        x >= rect.x && y >= rect.y
            && i64::from(x) < i64::from(rect.x) + i64::from(rect.width)
            && i64::from(y) < i64::from(rect.y) + i64::from(rect.height)
    })
}

pub fn scroll_at(app: &AppHandle, mut x: i32, mut y: i32, delta: i32) -> Result<(), ManagerError> {
    if cfg!(target_os = "macos") || desktop::is_wayland() {
        if let Some(monitor) = app.monitor_from_point(x as f64, y as f64).map_err(failure)? {
            let native = xcap::Monitor::all().map_err(failure)?;
            if let Some(screen) = native.iter().find(|screen| screen.name().ok().as_ref() == monitor.name()) {
                x = screen.x().map_err(failure)? + ((x - monitor.position().x) as f64 / monitor.scale_factor()).round() as i32;
                y = screen.y().map_err(failure)? + ((y - monitor.position().y) as f64 / monitor.scale_factor()).round() as i32;
            }
        }
    }
    desktop::scroll_at(x, y, delta)
}

pub fn start_hotkeys(app: AppHandle) {
    desktop::start_shortcuts(app.clone(), |app| {
        let service = app.state::<ScreenshotService>();
        let settings = service.settings();
        if !settings.enabled || service.hotkeys_paused.load(Ordering::SeqCst) {
            return Vec::new();
        }
        let mut bindings = if settings.hotkeys {
            settings.shortcut_bindings().into_iter()
                .map(|(_, action, shortcut)| (action.to_string(), shortcut)).collect::<Vec<_>>()
        } else { Vec::new() };
        // 焦点在被截应用时也能结束长截图；Wayland 沿用已授权的长截图组合键。
        if service.long_capturing.load(Ordering::SeqCst) && !desktop::is_wayland() {
            bindings.push(("finishLongCapture".into(), "Escape".into()));
        }
        bindings
    }, |app, action| {
        let app = app.clone();
        let action = action.to_string();
        tauri::async_runtime::spawn(async move {
            let service = app.state::<ScreenshotService>();
            if !service.settings().enabled || service.hotkeys_paused.load(Ordering::SeqCst) { return; }
            if action == "finishLongCapture" || (action == "captureLong" && service.long_capturing.load(Ordering::SeqCst)) {
                if let Some(window) = app.get_webview_window("screenshot-capture") {
                    let _ = window.emit("screenshot:long-finish", ());
                }
                return;
            }
            let result = match action.as_str() {
                "captureAutoCopy" => begin_capture(&app, true, false).await,
                "capture" => begin_capture(&app, false, false).await,
                "captureLong" => begin_capture(&app, false, true).await,
                "clipboard" => paste_clipboard(&app),
                "toggle" => toggle_pins(&app),
                "switchGroup" => switch_pin_group(&app),
                "resetInput" => reset_input(&app),
                _ => Ok(()),
            };
            if let Err(error) = result { let _ = app.emit("screenshot:error", error.to_string()); }
        });
    }, |app, errors| {
        *app.state::<ScreenshotService>().hotkey_errors.lock().unwrap() = errors;
        changed(app);
    });
    // 长截图期间，透明覆盖层只在预览区接收鼠标事件。
    std::thread::spawn(move || loop {
        let service = app.state::<ScreenshotService>();
        if service.long_capturing.load(Ordering::SeqCst) && !desktop::is_wayland() {
            let preview = service.long_preview_rect.lock().unwrap();
            if let Some(window) = app.get_webview_window("screenshot-capture") {
                let _ = window.set_ignore_cursor_events(!cursor_in_rect(&app, preview.as_ref()));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    });
}

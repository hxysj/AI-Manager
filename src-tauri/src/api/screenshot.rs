use crate::core::error::ManagerError;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageDecoder, ImageReader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_opener::OpenerExt;

#[cfg(windows)]
mod windows;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    id: String,
    created_at: String,
    width: u32,
    height: u32,
    #[serde(default)]
    pin: Option<Pin>,
    #[serde(default)]
    origin: Option<Position>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Position {
    x: i32,
    y: i32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pin {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    opacity: f64,
    #[serde(default = "default_pin_group")]
    group: String,
    lifecycle: String,
    visible: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub hotkeys: bool,
    pub shortcut_style: String,
    pub restore_pins: bool,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default)]
    pub shortcuts: std::collections::HashMap<String, String>,
}

fn default_enabled() -> bool {
    true
}

fn default_history_limit() -> usize {
    50
}

fn default_pin_group() -> String {
    "Default".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            hotkeys: true,
            shortcut_style: "alt".into(),
            restore_pins: true,
            history_limit: default_history_limit(),
            shortcuts: std::collections::HashMap::new(),
        }
    }
}

impl Settings {
    pub fn shortcut_bindings(&self) -> Vec<(i32, &'static str, String)> {
        [
            (1, "captureAutoCopy", "Ctrl+F1"),
            (2, "capture", "Shift+F1"),
            (3, "clipboard", "F3"),
            (4, "toggle", "Shift+F3"),
            (5, "switchGroup", "Ctrl+F3"),
            (6, "resetInput", "Ctrl+Alt+R"),
            (7, "captureLong", "Ctrl+Alt+F1"),
        ]
        .into_iter()
        .map(|(id, name, default)| {
            (id, name, self.shortcuts.get(name).cloned().unwrap_or_else(|| default.into()))
        })
        .collect()
    }

    fn validate(&self) -> Result<(), ManagerError> {
        if !matches!(self.shortcut_style.as_str(), "alt" | "shift") {
            return Err(failure("不支持的快捷键组合"));
        }
        if !(1..=500).contains(&self.history_limit) {
            return Err(failure("历史记录上限必须为 1 到 500 的整数"));
        }
        let mut bindings = std::collections::HashSet::new();
        for (_, _, shortcut) in self.shortcut_bindings() {
            if !bindings.insert(parse_shortcut(&shortcut)?) {
                return Err(failure("截图快捷键不能重复"));
            }
        }
        Ok(())
    }
}

fn parse_shortcut(value: &str) -> Result<(u32, u32), ManagerError> {
    let mut modifiers = 0;
    let mut key = None;
    for part in value.split('+') {
        let part = part.trim().to_ascii_uppercase();
        let modifier = match part.as_str() {
            "ALT" => 1,
            "CTRL" => 2,
            "SHIFT" => 4,
            "WIN" => 8,
            _ => 0,
        };
        if modifier != 0 {
            if modifiers & modifier != 0 { return Err(failure("快捷键包含重复修饰键")); }
            modifiers |= modifier;
            continue;
        }
        let code = match part.as_str() {
            "PRINTSCREEN" => 0x2c,
            "SPACE" => 0x20,
            value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphanumeric() => u32::from(value.as_bytes()[0]),
            value if value.starts_with('F') => value[1..].parse::<u32>().ok().filter(|number| (1..=24).contains(number)).map(|number| 0x70 + number - 1).ok_or_else(|| failure("不支持的功能键"))?,
            _ => return Err(failure("快捷键请使用字母、数字、F1–F24、PrintScreen 或 Space")),
        };
        if key.replace(code).is_some() { return Err(failure("每组快捷键只能包含一个主键")); }
    }
    let key = key.ok_or_else(|| failure("快捷键缺少主键"))?;
    if modifiers == 0 && !(0x70..=0x87).contains(&key) && key != 0x2c {
        return Err(failure("字母、数字和空格快捷键至少需要一个修饰键"));
    }
    Ok((modifiers, key))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    schema_version: u32,
    settings: Settings,
    entries: Vec<Entry>,
}

#[derive(Clone, Copy, Deserialize)]
struct CaptureRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

pub struct ScreenshotService {
    root: PathBuf,
    _instance_lock: std::fs::File,
    session: Mutex<Session>,
    frame: Mutex<Option<Vec<u8>>>,
    capturing: AtomicBool,
    long_capture_requested: AtomicBool,
    long_capturing: AtomicBool,
    long_preview_rect: Mutex<Option<CaptureRect>>,
    // 长截图期间暂时隐藏已有贴图，避免贴图窗口被 GDI 一并截进长图。
    long_hidden_pins: Mutex<Vec<String>>,
    hotkeys_paused: AtomicBool,
    view_save_pending: AtomicBool,
    menu_windows: Mutex<HashMap<String, (PhysicalPosition<i32>, PhysicalSize<u32>)>>,
    pub hotkey_errors: Mutex<Vec<String>>,
    pub startup_errors: Mutex<Vec<String>>,
}

fn failure(error: impl std::fmt::Display) -> ManagerError {
    ManagerError::System(error.to_string())
}

impl ScreenshotService {
    pub fn new(data_root: &Path) -> Result<Self, ManagerError> {
        let root = data_root.join("screenshots");
        std::fs::create_dir_all(root.join("documents"))?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)] {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let instance_lock = options.open(root.join("runtime.lock")).map_err(|error| failure(format!("无法打开截图数据目录，可能已有应用实例正在使用：{error}")))?;
        let file = root.join("session.json");
        let mut errors = Vec::new();
        let session = if file.exists() {
            // 损坏清单保留原文件，不以空会话覆盖。
            let value: Session = serde_json::from_slice(&std::fs::read(&file)?)?;
            if value.schema_version != 1 {
                return Err(failure("截图贴图数据版本不受支持，原文件已保留"));
            }
            value.settings.validate()?;
            for entry in &value.entries {
                if uuid::Uuid::parse_str(&entry.id).is_err() {
                    return Err(failure("截图清单包含无效文档标识"));
                }
                if !root
                    .join("documents")
                    .join(&entry.id)
                    .join("output.png")
                    .exists()
                {
                    errors.push(format!("截图 {} 的图片缺失，其余条目仍可使用", entry.id));
                }
            }
            value
        } else {
            Session {
                schema_version: 1,
                settings: Settings::default(),
                entries: Vec::new(),
            }
        };
        let service = Self {
            root,
            _instance_lock: instance_lock,
            session: Mutex::new(session),
            frame: Mutex::new(None),
            capturing: AtomicBool::new(false),
            long_capture_requested: AtomicBool::new(false),
            long_capturing: AtomicBool::new(false),
            long_preview_rect: Mutex::new(None),
            long_hidden_pins: Mutex::new(Vec::new()),
            hotkeys_paused: AtomicBool::new(false),
            view_save_pending: AtomicBool::new(false),
            hotkey_errors: Mutex::new(Vec::new()),
            startup_errors: Mutex::new(errors),
            menu_windows: Mutex::new(HashMap::new()),
        };
        {
            let mut guard = service.session.lock().unwrap();
            let mut next = guard.clone();
            let expired = service.prune_history(&mut next);
            if !expired.is_empty() {
                service.commit(&next)?;
                *guard = next;
            }
            drop(guard);
            service.remove_expired_documents(&expired);
        }
        Ok(service)
    }

    fn commit(&self, session: &Session) -> Result<(), ManagerError> {
        atomic_write(
            &self.root.join("session.json"),
            &serde_json::to_vec_pretty(session)?,
        )
    }

    fn document_dir(&self, id: &str) -> Result<PathBuf, ManagerError> {
        uuid::Uuid::parse_str(id).map_err(|_| failure("无效截图标识"))?;
        Ok(self.root.join("documents").join(id))
    }

    fn prune_history(&self, session: &mut Session) -> Vec<String> {
        session.entries.sort_by_cached_key(|entry| {
            std::cmp::Reverse(
                chrono::DateTime::parse_from_rfc3339(&entry.created_at)
                    .map(|date| date.timestamp_millis())
                    .unwrap_or(i64::MIN),
            )
        });
        let mut retained = 0;
        let mut ordinary = 0;
        let mut history_bytes = 0_u64;
        let mut expired = Vec::new();
        session.entries.retain(|entry| {
            if retained >= session.settings.history_limit {
                expired.push(entry.id.clone());
                return false;
            }
            if entry
                .pin
                .as_ref()
                .is_some_and(|pin| pin.lifecycle != "destroyed")
            {
                retained += 1;
                return true;
            }
            ordinary += 1;
            let dir = self.root.join("documents").join(&entry.id);
            let bytes: u64 = ["source.png", "output.png", "thumbnail.png", "document.json"]
                .iter()
                .filter_map(|name| std::fs::metadata(dir.join(name)).ok())
                .map(|metadata| metadata.len())
                .sum();
            history_bytes = history_bytes.saturating_add(bytes);
            let old = chrono::DateTime::parse_from_rfc3339(&entry.created_at)
                .is_ok_and(|date| chrono::Utc::now().signed_duration_since(date).num_days() > 30);
            if ordinary > 1 && (history_bytes > 512 * 1024 * 1024 || old) {
                expired.push(entry.id.clone());
                return false;
            }
            retained += 1;
            true
        });
        expired
    }

    fn remove_expired_documents(&self, ids: &[String]) {
        for id in ids {
            let result = self.document_dir(id).and_then(|dir| {
                if dir.exists() {
                    std::fs::remove_dir_all(dir)?;
                }
                Ok(())
            });
            if let Err(error) = result {
                self.startup_errors
                    .lock()
                    .unwrap()
                    .push(format!("清理截图 {id} 的历史文件失败：{error}"));
            }
        }
    }

    pub fn settings(&self) -> Settings {
        self.session.lock().unwrap().settings.clone()
    }

    fn ensure_enabled(&self) -> Result<(), ManagerError> {
        if !self.settings().enabled {
            return Err(failure("截图贴图已关闭，请在功能扩展中启动"));
        }
        Ok(())
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), ManagerError> {
        let mut guard = self.session.lock().unwrap();
        let mut next = guard.clone();
        // 启停仅修改本模块的开关，保留快捷键、历史与贴图恢复状态。
        next.settings.enabled = enabled;
        self.commit(&next)?;
        *guard = next;
        Ok(())
    }

    fn update_pin(&self, id: &str, update: impl FnOnce(&mut Pin)) -> Result<(), ManagerError> {
        let mut guard = self.session.lock().unwrap();
        let mut next = guard.clone();
        if let Some(pin) = next
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .and_then(|e| e.pin.as_mut())
        {
            update(pin);
            self.commit(&next)?;
            *guard = next;
        }
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ManagerError> {
    use std::io::Write;
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn encode_rgba_png_fast(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, ManagerError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::FilterType::NoFilter);
        encoder
            .write_header()
            .map_err(failure)?
            .write_image_data(rgba)
            .map_err(failure)?;
    }
    Ok(bytes)
}

fn encode_png_fast(image: &DynamicImage) -> Result<Vec<u8>, ManagerError> {
    if let Some(rgba) = image.as_rgba8() {
        return encode_rgba_png_fast(rgba.as_raw(), image.width(), image.height());
    }
    let rgba = image.to_rgba8();
    encode_rgba_png_fast(rgba.as_raw(), image.width(), image.height())
}

fn image_data(image: &DynamicImage) -> Result<String, ManagerError> {
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(encode_png_fast(image)?)
    ))
}

fn decode_data(data: &str) -> Result<(Vec<u8>, DynamicImage), ManagerError> {
    if data.len() > 128 * 1024 * 1024 {
        return Err(failure("图片文件超过 96 MiB 限制"));
    }
    let bytes = STANDARD
        .decode(data.split_once(',').map(|(_, b)| b).unwrap_or(data))
        .map_err(failure)?;
    let image = decode_bytes(&bytes)?;
    Ok((bytes, image))
}

#[cfg(test)]
fn decode(data: &str) -> Result<DynamicImage, ManagerError> {
    decode_data(data).map(|(_, image)| image)
}

fn decode_bytes(bytes: &[u8]) -> Result<DynamicImage, ManagerError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(failure)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(failure)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > 40_000_000 {
        return Err(failure("图片超过 4000 万像素限制"));
    }
    DynamicImage::from_decoder(decoder).map_err(failure)
}

fn id_from(payload: &Value) -> Result<&str, ManagerError> {
    payload["id"]
        .as_str()
        .ok_or_else(|| failure("缺少截图标识"))
}

pub async fn dispatch(
    app: AppHandle,
    action: String,
    payload: Value,
) -> Result<Value, ManagerError> {
    let service = app.try_state::<ScreenshotService>().ok_or_else(|| {
        failure("截图贴图数据未能加载，请检查数据目录中的 session.json；原数据已保留")
    })?;
    if !matches!(action.as_str(), "status" | "set-enabled" | "list" | "settings" | "pause-hotkeys" | "document" | "delete" | "export" | "copy" | "cancel") {
        service.ensure_enabled()?;
    }
    match action.as_str() {
        "status" => Ok(json!(service.settings())),
        "set-enabled" => {
            let enabled = payload["enabled"].as_bool().ok_or_else(|| failure("缺少截图启停状态"))?;
            service.set_enabled(enabled)?;
            let result = sync_runtime(&app);
            changed(&app);
            result?;
            Ok(json!(service.settings()))
        }
        "list" => {
            let session = service.session.lock().unwrap().clone();
            let entries = session
                .entries
                .iter()
                .map(|entry| {
                    let thumbnail = service
                        .document_dir(&entry.id)
                        .ok()
                        .and_then(|dir| std::fs::read(dir.join("thumbnail.png")).ok())
                        .map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes)));
                    let mut value = serde_json::to_value(entry).unwrap();
                    value["thumbnail"] = json!(thumbnail);
                    value
                })
                .collect::<Vec<_>>();
            Ok(
                json!({"entries": entries, "settings": session.settings, "hotkeyErrors": *service.hotkey_errors.lock().unwrap(), "errors": *service.startup_errors.lock().unwrap(), "dataPath": service.root}),
            )
        }
        "settings" => {
            let settings: Settings = serde_json::from_value(payload)?;
            settings.validate()?;
            let mut guard = service.session.lock().unwrap();
            let mut next = guard.clone();
            next.settings = settings;
            // 配置弹框可能早于启停操作打开，不能用旧草稿覆盖模块开关。
            next.settings.enabled = guard.settings.enabled;
            let expired = service.prune_history(&mut next);
            service.commit(&next)?;
            *guard = next;
            drop(guard);
            remove_expired_history(&app, &service, &expired, None);
            changed(&app);
            Ok(json!(true))
        }
        "pause-hotkeys" => {
            service.hotkeys_paused.store(payload["paused"].as_bool().unwrap_or(false), Ordering::SeqCst);
            Ok(json!(true))
        }
        "capture" => {
            drop(service);
            begin_capture(&app, false, false).await?;
            Ok(json!(true))
        }
        "capture-auto-copy" => {
            drop(service);
            begin_capture(&app, true, false).await?;
            Ok(json!(true))
        }
        "capture-long" => {
            drop(service);
            begin_capture(&app, false, true).await?;
            Ok(json!(true))
        }
        "capture-mode" => {
            Ok(json!({
                "long": service.long_capture_requested.load(Ordering::SeqCst)
            }))
        }
        "long-start" => {
            let window = app
                .get_webview_window("screenshot-capture")
                .ok_or_else(|| failure("截图窗口已关闭"))?;
            // 覆盖层本身设置了 content_protected，长截图时保留透明边框和
            // 进度提示；仅让鼠标事件穿透到下方页面。已有贴图窗口必须
            // 暂时隐藏，否则 GDI 会把它们当成页面内容一起截入长图。
            window.set_ignore_cursor_events(true).map_err(failure)?;
            window.set_focusable(false).map_err(failure)?;
            *service.long_preview_rect.lock().unwrap() = None;
            service.long_capturing.store(true, Ordering::SeqCst);
            let mut hidden = service.long_hidden_pins.lock().unwrap();
            hidden.clear();
            for (label, pin) in app.webview_windows() {
                if !label.starts_with("screenshot-pin-")
                    || !pin.is_visible().unwrap_or(false)
                {
                    continue;
                }
                pin.hide().map_err(failure)?;
                hidden.push(label);
            }
            Ok(json!(true))
        }
        "long-preview" => {
            let rect: CaptureRect = serde_json::from_value(payload).map_err(failure)?;
            if service.long_capturing.load(Ordering::SeqCst) {
                *service.long_preview_rect.lock().unwrap() = Some(rect);
            }
            Ok(json!(true))
        }
        "long-scroll" => {
            #[cfg(windows)] {
                // 与鼠标穿透检测共用锁，注入滚轮期间不能重新拦截底层应用的鼠标事件。
                let preview = service.long_preview_rect.lock().unwrap();
                if service.long_capturing.load(Ordering::SeqCst) {
                    let x = payload["x"].as_i64().ok_or_else(|| failure("缺少滚动横坐标"))? as i32;
                    let y = payload["y"].as_i64().ok_or_else(|| failure("缺少滚动纵坐标"))? as i32;
                    let delta = payload["delta"].as_i64().unwrap_or(-120).clamp(-120, 120) as i32;
                    if let Some(window) = app.get_webview_window("screenshot-capture") {
                        let interactive = windows::cursor_in_rect(preview.as_ref());
                        window.set_ignore_cursor_events(true).map_err(failure)?;
                        let result = windows::scroll_at(x, y, delta);
                        window.set_ignore_cursor_events(!interactive).map_err(failure)?;
                        result?;
                    }
                }
            }
            Ok(json!(true))
        }
        "long-end" => {
            end_long_capture(&app)?;
            if service.capturing.load(Ordering::SeqCst) {
                if let Some(window) = app.get_webview_window("screenshot-capture") {
                    window.set_ignore_cursor_events(false).map_err(failure)?;
                    window.set_focusable(true).map_err(failure)?;
                    window.set_focus().map_err(failure)?;
                }
            }
            Ok(json!(true))
        }
        "ready" => {
            if !service.capturing.load(Ordering::SeqCst) { return Ok(json!(false)); }
            let window = app
                .get_webview_window("screenshot-capture")
                .ok_or_else(|| failure("截图窗口已关闭"))?;
            window.set_focusable(true).map_err(failure)?;
            window.set_ignore_cursor_events(false).map_err(failure)?;
            if !window.is_visible().unwrap_or(false) { window.show().map_err(failure)?; }
            window.set_focus().map_err(failure)?;
            Ok(json!(true))
        }
        "capture-window-ready" => {
            if service.capturing.load(Ordering::SeqCst) { return Ok(json!(false)); }
            let window = app
                .get_webview_window("screenshot-capture")
                .ok_or_else(|| failure("截图窗口已关闭"))?;
            window.set_focusable(false).map_err(failure)?;
            window.set_ignore_cursor_events(true).map_err(failure)?;
            if !window.is_visible().unwrap_or(false) { window.show().map_err(failure)?; }
            Ok(json!(true))
        }
        "smart-selection" => {
            let x = payload["x"].as_i64().ok_or_else(|| failure("缺少智能选区横坐标"))? as i32;
            let y = payload["y"].as_i64().ok_or_else(|| failure("缺少智能选区纵坐标"))? as i32;
            #[cfg(windows)]
            {
                let regions = windows::smart_selection(&app, x, y)?;
                return Ok(json!({"regions": regions}));
            }
            #[cfg(not(windows))]
            {
                let _ = (x, y);
                return Ok(json!({"regions": []}));
            }
        }
        "pin-show" | "pin-ready" => {
            let id = id_from(&payload)?;
            let window = app
                .get_webview_window(&format!("screenshot-pin-{id}"))
                .ok_or_else(|| failure("贴图窗口不存在"))?;
            let visible = service
                .session
                .lock()
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .and_then(|entry| entry.pin.as_ref())
                .is_some_and(|pin| pin.visible && pin.lifecycle == "open");
            if visible {
                if !window.is_visible().map_err(failure)? {
                    window.show().map_err(failure)?;
                }
                if action == "pin-ready" && payload["dismissCapture"].as_bool().unwrap_or(false)
                    && service.capturing.load(Ordering::SeqCst)
                {
                    cancel_capture(&app)?;
                }
            }
            if action == "pin-show" {
                let size = window.inner_size().map_err(failure)?;
                return Ok(json!({"visible": visible, "width": size.width, "height": size.height}));
            }
            Ok(json!(visible))
        }
        "cancel" => {
            cancel_capture(&app)?;
            Ok(json!(true))
        }
        "import" => {
            let path = payload["path"]
                .as_str()
                .ok_or_else(|| failure("请选择本地图片"))?;
            if std::fs::metadata(path)?.len() > 96 * 1024 * 1024 {
                return Err(failure("图片文件过大"));
            }
            let image = decode_bytes(&std::fs::read(path)?)?;
            drop(service);
            let id = save_document(
                &app,
                json!({"source": image_data(&image)?, "output": image_data(&image)?, "annotations": []}),
            )?;
            open_pin(&app, &id)?;
            Ok(json!(id))
        }
        "clipboard" => {
            drop(service);
            paste_clipboard(&app)?;
            Ok(json!(true))
        }
        "save-document" => {
            drop(service);
            // PNG 解码、缩略图和文件提交不占用处理窗口命令的线程。
            let id = tauri::async_runtime::spawn_blocking(move || save_document(&app, payload))
                .await.map_err(failure)??;
            Ok(json!(id))
        }
        "document" => {
            let id = id_from(&payload)?;
            let dir = service.document_dir(id)?;
            let mut document: Value =
                serde_json::from_slice(&std::fs::read(dir.join("document.json"))?)?;
            if !payload["metadataOnly"].as_bool().unwrap_or(false) {
                document["source"] = json!(format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(std::fs::read(dir.join("source.png"))?)
                ));
                document["output"] = json!(format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(std::fs::read(dir.join("output.png"))?)
                ));
            }
            let entry = service
                .session
                .lock()
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .cloned();
            document["pin"] = json!(entry.as_ref().and_then(|entry| entry.pin.clone()));
            document["width"] = json!(entry.as_ref().map(|entry| entry.width).unwrap_or(0));
            document["height"] = json!(entry.as_ref().map(|entry| entry.height).unwrap_or(0));
            Ok(document)
        }
        "pin" => {
            drop(service);
            open_pin_with_visibility(
                &app,
                id_from(&payload)?,
                true,
                payload["capture"].as_bool().unwrap_or(false),
            )?;
            Ok(json!(true))
        }
        "copy" => {
            let image = decode_bytes(&std::fs::read(
                service.document_dir(id_from(&payload)?)?.join("output.png"),
            )?)?;
            #[cfg(windows)]
            windows::copy_image(&app, &image)?;
            #[cfg(not(windows))]
            return Err(failure("图片剪贴板目前仅支持 Windows"));
            Ok(json!(true))
        }
        "export" => {
            let id = id_from(&payload)?.to_string();
            let dir = service.document_dir(&id)?;
            drop(service);
            use tauri_plugin_dialog::DialogExt;
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let mut dialog = app
                .dialog()
                .file()
                .set_title("保存截图")
                .set_file_name("monkey-tief-screenshot.png")
                .add_filter("PNG 图片", &["png"]);
            if let Some(window) = app
                .get_webview_window(&format!("screenshot-pin-{id}"))
                .or_else(|| {
                    app.get_webview_window("screenshot-capture")
                        .filter(|window| window.is_visible().unwrap_or(false))
                })
            {
                dialog = dialog.set_parent(&window);
            }
            dialog.save_file(move |path| {
                let _ = sender.send(path);
            });
            let path = receiver
                .await
                .map_err(|_| failure("保存对话框未返回结果"))?;
            if let Some(path) = path {
                let path = path.into_path().map_err(failure)?;
                tokio::fs::copy(dir.join("output.png"), &path).await?;
                Ok(json!({"saved": true, "path": path}))
            } else {
                Ok(json!({"saved": false}))
            }
        }
        "restore-previous" => {
            let restore_id = service
                .session
                .lock()
                .unwrap()
                .entries
                .iter()
                .find(|entry| {
                    entry
                        .pin
                        .as_ref()
                        .is_some_and(|pin| pin.lifecycle == "closed")
                })
                .map(|entry| entry.id.clone())
                .ok_or_else(|| failure("没有可恢复的上一张贴图"))?;
            drop(service);
            open_pin_with_visibility(&app, &restore_id, true, false)?;
            Ok(json!({"id": restore_id}))
        }
        "process-image" => {
            let id = id_from(&payload)?.to_string();
            let operation = payload["operation"].as_str().unwrap_or("");
            let dir = service.document_dir(&id)?;
            let image = decode_bytes(&std::fs::read(dir.join("output.png"))?)?;
            let image = match operation {
                "rotate-left" => image.rotate270(),
                "rotate-right" => image.rotate90(),
                "flip-horizontal" => image.fliph(),
                "flip-vertical" => image.flipv(),
                _ => return Err(failure("不支持的图像处理操作")),
            };
            persist_flattened_image(&app, &service, &id, &image)?;
            Ok(json!(true))
        }
        "replace-file" => {
            let id = id_from(&payload)?.to_string();
            let path = payload["path"]
                .as_str()
                .ok_or_else(|| failure("缺少图片路径"))?;
            if std::fs::metadata(path)?.len() > 96 * 1024 * 1024 {
                return Err(failure("图片文件过大"));
            }
            let image = decode_bytes(&std::fs::read(path)?)?;
            persist_flattened_image(&app, &service, &id, &image)?;
            Ok(json!(true))
        }
        "open-folder" => {
            let dir = service.document_dir(id_from(&payload)?)?;
            app.opener()
                .open_path(dir.to_string_lossy().to_string(), None::<&str>)
                .map_err(|error| failure(error.to_string()))?;
            Ok(json!(true))
        }
        "toggle" => {
            drop(service);
            toggle_pins(&app)?;
            Ok(json!(true))
        }
        "switch-group" => {
            drop(service);
            switch_pin_group(&app)?;
            Ok(json!(true))
        }
        "reset-input" => {
            drop(service);
            reset_input(&app)?;
            Ok(json!(true))
        }
        "pin-action" => {
            let id = id_from(&payload)?;
            service.document_dir(id)?;
            let window = app
                .get_webview_window(&format!("screenshot-pin-{id}"))
                .ok_or_else(|| failure("贴图窗口不存在"))?;
            match payload["action"].as_str().unwrap_or("") {
                "drag" => window.start_dragging().map_err(failure)?,
                "move" => {
                    let (x, y) = match payload["direction"].as_str() {
                        Some("up") => (0, -1),
                        Some("down") => (0, 1),
                        Some("left") => (-1, 0),
                        Some("right") => (1, 0),
                        _ => return Err(failure("无效的贴图移动方向")),
                    };
                    let (sender, receiver) = tokio::sync::oneshot::channel();
                    let moving = window.clone();
                    // 读取和设置位置在同一个窗口线程任务中完成，长按方向键不会丢步。
                    window.run_on_main_thread(move || {
                        let result = moving.outer_position().map_err(failure).and_then(|position| {
                            moving.set_position(PhysicalPosition::new(position.x + x, position.y + y)).map_err(failure)
                        });
                        let _ = sender.send(result);
                    }).map_err(failure)?;
                    receiver.await.map_err(failure)??;
                    return Ok(json!(true));
                }
                "hide" => {
                    window.hide().map_err(failure)?;
                    service.update_pin(id, |p| p.visible = false)?;
                }
                "close" => {
                    window.close().map_err(failure)?;
                }
                "context-menu-open" => {
                    let extra_width = payload["extraWidth"].as_u64().unwrap_or(560).min(4096) as u32;
                    let extra_height = payload["extraHeight"].as_u64().unwrap_or(700).min(4096) as u32;
                    let size = window.inner_size().map_err(failure)?;
                    let position = window.outer_position().map_err(failure)?;
                    let mut menu_windows = service.menu_windows.lock().unwrap();
                    if let Some((_, original_size)) = menu_windows.get(id) {
                        return Ok(json!({"width": original_size.width, "height": original_size.height}));
                    }
                    menu_windows.insert(id.to_string(), (position, size));
                    drop(menu_windows);
                    // 调整无边框贴图窗口时，Windows 可能先把旧的网页合成表面
                    // 拉伸到新尺寸，造成一帧巨大的截图闪烁。隐藏窗口后再调整，
                    // 等尺寸完成后显示，透明区域不会经过可见的拉伸帧。
                    if let Err(error) = window.hide() {
                        service.menu_windows.lock().unwrap().remove(id);
                        return Err(failure(error));
                    }
                    if let Err(error) = window.set_size(PhysicalSize::new(
                        size.width.saturating_add(extra_width),
                        size.height.saturating_add(extra_height),
                    )) {
                        let _ = window.show();
                        service.menu_windows.lock().unwrap().remove(id);
                        return Err(failure(error));
                    }
                    if let Err(error) = window.show() {
                        service.menu_windows.lock().unwrap().remove(id);
                        return Err(failure(error));
                    }
                    return Ok(json!({"width": size.width, "height": size.height}));
                }
                "context-menu-close" => {
                    let original = service.menu_windows.lock().unwrap().remove(id);
                    if let Some((position, size)) = original {
                        window.hide().map_err(failure)?;
                        if let Err(error) = window.set_size(size) {
                            let _ = window.show();
                            return Err(failure(error));
                        }
                        if let Err(error) = window.set_position(position) {
                            let _ = window.show();
                            return Err(failure(error));
                        }
                        window.show().map_err(failure)?;
                    }
                    return Ok(json!(true));
                }
                "destroy" => {
                    service.update_pin(id, |p| {
                        p.lifecycle = "destroyed".into();
                        p.visible = false;
                    })?;
                    window.destroy().map_err(failure)?;
                }
                "transparent" => {
                    window.set_ignore_cursor_events(true).map_err(failure)?;
                }
                "group" => {
                    let group = payload["group"]
                        .as_str()
                        .map(str::trim)
                        .filter(|group| !group.is_empty() && group.len() <= 64)
                        .ok_or_else(|| failure("分组名称无效"))?
                        .to_string();
                    service.update_pin(id, |pin| pin.group = group)?;
                }
                "resize" => {
                    let width = payload["width"]
                        .as_f64()
                        .ok_or_else(|| failure("无效宽度"))?
                        .clamp(48.0, 8192.0) as u32;
                    let height = payload["height"]
                        .as_f64()
                        .ok_or_else(|| failure("无效高度"))?
                        .clamp(48.0, 8192.0) as u32;
                    let old_size = window.inner_size().map_err(failure)?;
                    let old_position = window.outer_position().map_err(failure)?;
                    let anchor_x = payload["anchorX"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);
                    let anchor_y = payload["anchorY"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);
                    window
                        .set_size(PhysicalSize::new(width, height))
                        .map_err(failure)?;
                    window
                        .set_position(PhysicalPosition::new(
                            old_position.x
                                + ((old_size.width as f64 - width as f64) * anchor_x).round()
                                    as i32,
                            old_position.y
                                + ((old_size.height as f64 - height as f64) * anchor_y).round()
                                    as i32,
                        ))
                        .map_err(failure)?;
                }
                "edit" => {
                    window.set_focusable(true).map_err(failure)?;
                    window.set_focus().map_err(failure)?;
                }
                "display" => {
                    let retained = service.session.lock().unwrap().entries.iter().any(|entry| entry.id == id);
                    if retained {
                        window.set_focusable(true).map_err(failure)?;
                    } else {
                        window.destroy().map_err(failure)?;
                    }
                }
                "opacity" => {
                    let opacity = payload["opacity"]
                        .as_f64()
                        .ok_or_else(|| failure("无效透明度"))?
                        .clamp(0.15, 1.0);
                    service.update_pin(id, |p| p.opacity = opacity)?;
                    window
                        .emit("screenshot:opacity", opacity)
                        .map_err(failure)?;
                }
                _ => return Err(failure("不支持的贴图操作")),
            }
            changed(&app);
            Ok(json!(true))
        }
        "delete" => {
            let id = id_from(&payload)?;
            let dir = service.document_dir(id)?;
            let mut guard = service.session.lock().unwrap();
            let mut next = guard.clone();
            next.entries.retain(|e| e.id != id);
            service.commit(&next)?;
            *guard = next;
            drop(guard);
            if let Some(window) = app.get_webview_window(&format!("screenshot-pin-{id}")) {
                window.destroy().map_err(failure)?;
            }
            if dir.exists() {
                std::fs::remove_dir_all(dir)?;
            }
            changed(&app);
            Ok(json!(true))
        }
        _ => Err(failure("不支持的截图操作")),
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit("screenshot:changed", ());
}

fn remove_expired_history(app: &AppHandle, service: &ScreenshotService, ids: &[String], editing_pin_id: Option<&str>) {
    for id in ids {
        if editing_pin_id == Some(id.as_str()) { continue; }
        if let Some(window) = app.get_webview_window(&format!("screenshot-pin-{id}")) {
            if let Err(error) = window.destroy() {
                let _ = app.emit("screenshot:error", format!("关闭过期贴图失败：{error}"));
            }
        }
    }
    service.remove_expired_documents(ids);
}

fn save_png(
    path: &Path,
    bytes: &[u8],
    image: &DynamicImage,
) -> Result<(), ManagerError> {
    // 截图和编辑器都传 PNG，直接落盘可以避免再次解码后编码整张大图。
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        std::fs::write(path, bytes)?;
    } else {
        write_png(path, image)?;
    }
    Ok(())
}

fn write_png(path: &Path, image: &DynamicImage) -> Result<(), ManagerError> {
    std::fs::write(path, encode_png_fast(image)?)?;
    Ok(())
}

fn save_thumbnail(path: &Path, image: &DynamicImage, preview: Option<&str>) -> Result<(), ManagerError> {
    if let Some(data) = preview {
        let (bytes, thumbnail) = decode_data(data)?;
        if thumbnail.width() > 320 || thumbnail.height() > 200 {
            return Err(failure("截图缩略图尺寸无效"));
        }
        save_png(path, &bytes, &thumbnail)
    } else {
        // 导入文件、粘贴剪贴板等旧调用仍可由后端生成预览。
        write_png(path, &image.thumbnail(320, 200))
    }
}

fn save_document(app: &AppHandle, payload: Value) -> Result<String, ManagerError> {
    let service = app.state::<ScreenshotService>();
    let source_data = payload["source"]
        .as_str()
        .ok_or_else(|| failure("缺少原图"))?;
    // 未标注截图只传一份原图，兼容已有的双图调用。
    let output_data = match payload.get("output") {
        None | Some(Value::Null) => source_data,
        Some(value) => value.as_str().ok_or_else(|| failure("渲染结果无效"))?,
    };
    let (source_bytes, source) = decode_data(source_data)?;
    let rendered = if source_data == output_data {
        None
    } else {
        Some(decode_data(output_data)?)
    };
    let (output_bytes, output) = rendered.as_ref()
        .map(|(bytes, image)| (bytes.as_slice(), image))
        .unwrap_or((&source_bytes, &source));
    let annotations = payload
        .get("annotations")
        .cloned()
        .unwrap_or_else(|| json!([]));
    if !annotations.is_array() || serde_json::to_vec(&annotations)?.len() > 8 * 1024 * 1024 {
        return Err(failure("标注数据无效或过大"));
    }
    // 每次输出保存不可变版本，清单最后提交，失败时保留旧版本。
    let id = uuid::Uuid::new_v4().to_string();
    let dir = service.document_dir(&id)?;
    std::fs::create_dir_all(&dir)?;
    save_png(&dir.join("source.png"), &source_bytes, &source)?;
    save_png(&dir.join("output.png"), output_bytes, output)?;
    save_thumbnail(&dir.join("thumbnail.png"), output, payload["thumbnail"].as_str())?;
    atomic_write(
        &dir.join("document.json"),
        &serde_json::to_vec(&json!({"schemaVersion": 1, "annotations": annotations}))?,
    )?;
    let mut guard = service.session.lock().unwrap();
    let mut next = guard.clone();
    next.entries.insert(
        0,
        Entry {
            id: id.clone(),
            created_at: chrono::Local::now().to_rfc3339(),
            width: output.width(),
            height: output.height(),
            pin: None,
            origin: payload.get("origin").filter(|value| !value.is_null()).map(|value| serde_json::from_value(value.clone())).transpose()?,
        },
    );
    let expired = service.prune_history(&mut next);
    service.commit(&next)?;
    *guard = next;
    drop(guard);
    remove_expired_history(app, &service, &expired, payload["editingPinId"].as_str());
    changed(app);
    Ok(id)
}

fn persist_flattened_image(
    app: &AppHandle,
    service: &ScreenshotService,
    id: &str,
    image: &DynamicImage,
) -> Result<(), ManagerError> {
    let dir = service.document_dir(id)?;
    write_png(&dir.join("source.png"), image)?;
    write_png(&dir.join("output.png"), image)?;
    write_png(&dir.join("thumbnail.png"), &image.thumbnail(320, 200))?;
    atomic_write(
        &dir.join("document.json"),
        br#"{"schemaVersion":1,"annotations":[]}"#,
    )?;
    let width = image.width();
    let height = image.height();
    let mut guard = service.session.lock().unwrap();
    let mut next = guard.clone();
    let entry = next
        .entries
        .iter_mut()
        .find(|entry| entry.id == id)
        .ok_or_else(|| failure("截图记录不存在"))?;
    entry.width = width;
    entry.height = height;
    if let Some(pin) = entry.pin.as_mut() {
        pin.width = width;
        pin.height = height;
    }
    service.commit(&next)?;
    *guard = next;
    drop(guard);
    if let Some(window) = app.get_webview_window(&format!("screenshot-pin-{id}")) {
        window
            .set_size(PhysicalSize::new(width, height))
            .map_err(failure)?;
    }
    changed(app);
    Ok(())
}

fn ensure_capture_window(
    app: &AppHandle,
    bounds: Option<(i32, i32, u32, u32)>,
) -> Result<tauri::WebviewWindow, ManagerError> {
    app.state::<ScreenshotService>().ensure_enabled()?;
    if let Some(window) = app.get_webview_window("screenshot-capture") { return Ok(window); }
    let mut builder = WebviewWindowBuilder::new(
        app,
        "screenshot-capture",
        WebviewUrl::App("index.html?panel=screenshot&mode=capture".into()),
    )
    .title("截图贴图")
    .decorations(false)
    .resizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .transparent(true)
    .background_color(tauri::window::Color(0, 0, 0, 0))
    .focused(false)
    .shadow(false)
    // 框选画布和遮罩不参与桌面采集，连续截图不会把上一帧的暗色遮罩拍进去。
    .content_protected(true);
    if let Some((left, top, width, height)) = bounds {
        builder = builder
            .position(f64::from(left), f64::from(top))
            .inner_size(f64::from(width), f64::from(height));
    }
    builder.visible(false).build().map_err(failure)
}

fn capture_bounds(monitors: &[tauri::Monitor]) -> Result<(i32, i32, u32, u32), ManagerError> {
    if monitors.is_empty() { return Err(failure("没有可截图的显示器")); }
    let left = monitors.iter().map(|m| m.position().x).min().unwrap();
    let top = monitors.iter().map(|m| m.position().y).min().unwrap();
    let right = monitors.iter().map(|m| i64::from(m.position().x) + i64::from(m.size().width)).max().unwrap();
    let bottom = monitors.iter().map(|m| i64::from(m.position().y) + i64::from(m.size().height)).max().unwrap();
    let width = (right - i64::from(left)) as u32;
    let height = (bottom - i64::from(top)) as u32;
    if u64::from(width) * u64::from(height) > 40_000_000 {
        return Err(failure("虚拟桌面超过 4000 万像素，请减少显示区域"));
    }
    Ok((left, top, width, height))
}

fn capture_frame_data(image: DynamicImage, left: i32, top: i32, auto_copy: bool) -> Vec<u8> {
    let image = image.into_rgba8();
    // 20 字节小端帧头后直接跟 RGBA，首帧不再经过 PNG、Base64 和浏览器图片解码。
    let mut bytes = Vec::with_capacity(20 + image.as_raw().len());
    for field in [left.to_le_bytes(), top.to_le_bytes(), image.width().to_le_bytes(), image.height().to_le_bytes(), u32::from(auto_copy).to_le_bytes()] {
        bytes.extend_from_slice(&field);
    }
    bytes.extend_from_slice(image.as_raw());
    bytes
}

#[tauri::command]
pub async fn screenshot_frame(app: AppHandle) -> tauri::ipc::Response {
    // 二进制响应避免把整屏像素序列化为 JSON 数字数组或脚本字符串。
    let service = app.state::<ScreenshotService>();
    let bytes = service.frame.lock().unwrap().clone().unwrap_or_default();
    tauri::ipc::Response::new(bytes)
}

#[tauri::command]
pub async fn screenshot_region_frame(
    app: AppHandle,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
) -> Result<tauri::ipc::Response, String> {
    app.state::<ScreenshotService>().ensure_enabled().map_err(|error| error.to_string())?;
    if width == 0 || height == 0 || width > 8192 || height > 8192 || u64::from(width) * u64::from(height) > 20_000_000 {
        return Err("长截图区域尺寸无效或过大".into());
    }
    let monitors = app.available_monitors().map_err(|error| error.to_string())?;
    let image = tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        {
            windows::capture(&monitors, left, top, width, height)
        }
        #[cfg(not(windows))]
        {
            let _ = (monitors, left, top, width, height);
            Err(failure("桌面截图目前仅支持 Windows"))
        }
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;
    Ok(tauri::ipc::Response::new(capture_frame_data(image, left, top, false)))
}

fn document_image_path(service: &ScreenshotService, id: &str, kind: &str) -> Result<PathBuf, ManagerError> {
    let file = match kind {
        "source" => "source.png",
        "output" => "output.png",
        _ => return Err(failure("不支持的截图图片类型")),
    };
    Ok(service.document_dir(id)?.join(file))
}

#[tauri::command]
pub async fn screenshot_image(app: AppHandle, id: String, kind: String) -> Result<tauri::ipc::Response, String> {
    let path = {
        let service = app.try_state::<ScreenshotService>().ok_or("截图服务未就绪")?;
        document_image_path(&service, &id, &kind).map_err(|error| error.to_string())?
    };
    let bytes = tokio::fs::read(path).await.map_err(|error| error.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

pub async fn begin_capture(app: &AppHandle, auto_copy: bool, long_capture: bool) -> Result<(), ManagerError> {
    let service = app.state::<ScreenshotService>();
    service.ensure_enabled()?;
    if service.capturing.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    service.long_capture_requested.store(long_capture, Ordering::SeqCst);
    let result = async {
        let monitors = app.available_monitors().map_err(failure)?;
        let (left, top, width, height) = capture_bounds(&monitors)?;
        let frame = tauri::async_runtime::spawn_blocking(move || {
            #[cfg(windows)]
            { Ok::<_, ManagerError>(capture_frame_data(windows::capture(&monitors, left, top, width, height)?, left, top, auto_copy)) }
            #[cfg(not(windows))]
            { Err::<Vec<u8>, ManagerError>(failure("桌面截图目前仅支持 Windows")) }
        }).await.map_err(failure)??;
        // 采集期间用户可能已经关闭扩展，不能再创建截图窗口。
        service.ensure_enabled()?;
        let window = ensure_capture_window(app, Some((left, top, width, height)))?;
        if window.inner_size().map_err(failure)? != PhysicalSize::new(width, height) {
            window.set_size(PhysicalSize::new(width, height)).map_err(failure)?;
        }
        if window.outer_position().map_err(failure)? != PhysicalPosition::new(left, top) {
            window.set_position(PhysicalPosition::new(left, top)).map_err(failure)?;
        }
        *service.frame.lock().unwrap() = Some(frame);
        window.emit("screenshot:frame", ()).map_err(failure)?;
        Ok(())
    }.await;
    if result.is_err() {
        let _ = cancel_capture(app);
    }
    result
}

fn cancel_capture(app: &AppHandle) -> Result<(), ManagerError> {
    end_long_capture(app)?;
    if let Some(window) = app.get_webview_window("screenshot-capture") {
        window.set_focusable(false).map_err(failure)?;
        window.set_ignore_cursor_events(true).map_err(failure)?;
        window.emit("screenshot:reset", ()).map_err(failure)?;
    }
    let service = app.state::<ScreenshotService>();
    *service.frame.lock().unwrap() = None;
    service.capturing.store(false, Ordering::SeqCst);
    service.long_capture_requested.store(false, Ordering::SeqCst);
    Ok(())
}

fn end_long_capture(app: &AppHandle) -> Result<(), ManagerError> {
    let service = app.state::<ScreenshotService>();
    service.long_capturing.store(false, Ordering::SeqCst);
    *service.long_preview_rect.lock().unwrap() = None;
    let hidden = std::mem::take(&mut *service.long_hidden_pins.lock().unwrap());
    if !hidden.is_empty() && service.settings().enabled {
        let session = service.session.lock().unwrap().clone();
        for label in hidden {
            let should_show = label
                .strip_prefix("screenshot-pin-")
                .and_then(|id| session.entries.iter().find(|entry| entry.id == id))
                .and_then(|entry| entry.pin.as_ref())
                .is_some_and(|pin| pin.lifecycle == "open" && pin.visible);
            if should_show {
                if let Some(window) = app.get_webview_window(&label) {
                    window.show().map_err(failure)?;
                }
            }
        }
    }
    Ok(())
}

fn open_pin(app: &AppHandle, id: &str) -> Result<(), ManagerError> {
    open_pin_with_visibility(app, id, true, false)
}

fn open_pin_with_visibility(
    app: &AppHandle,
    id: &str,
    show: bool,
    dismiss_capture: bool,
) -> Result<(), ManagerError> {
    let label = format!("screenshot-pin-{id}");
    let service = app.state::<ScreenshotService>();
    service.ensure_enabled()?;
    service.document_dir(id)?;
    if let Some(window) = app.get_webview_window(&label) {
        window.show().map_err(failure)?;
        service.update_pin(id, |p| p.visible = true)?;
        if dismiss_capture && service.capturing.load(Ordering::SeqCst) {
            cancel_capture(app)?;
        }
        changed(app);
        return Ok(());
    }
    let entry = service
        .session
        .lock()
        .unwrap()
        .entries
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| failure("截图记录不存在"))?;
    let dir = service.document_dir(id)?;
    let document: Value = serde_json::from_slice(&std::fs::read(dir.join("document.json"))?)?;
    if document["schemaVersion"] != 1
        || !dir.join("source.png").is_file()
        || !dir.join("output.png").is_file()
    {
        return Err(failure("贴图文档版本不受支持或图片缺失；原文件已保留"));
    }
    let monitors = app.available_monitors().map_err(failure)?;
    let monitor = app
        .primary_monitor()
        .map_err(failure)?
        .or_else(|| monitors.first().cloned())
        .ok_or_else(|| failure("没有可用显示器"))?;
    let mut pin = entry
        .pin
        .filter(|p| p.lifecycle != "destroyed")
        .unwrap_or(Pin {
            x: entry.origin.as_ref().map(|origin| origin.x).unwrap_or(monitor.position().x + 80),
            y: entry.origin.as_ref().map(|origin| origin.y).unwrap_or(monitor.position().y + 80),
            width: entry.width.max(1),
            height: entry.height.max(1),
            opacity: 1.0,
            group: default_pin_group(),
            lifecycle: "open".into(),
            visible: true,
        });
    if pin.height == 0 {
        pin.height = ((pin.width as f64 / entry.width as f64 * entry.height as f64) as u32).max(48);
    }
    if !monitors.iter().any(|m| {
        i64::from(pin.x) < i64::from(m.position().x) + i64::from(m.size().width)
            && i64::from(pin.x) + i64::from(pin.width) > i64::from(m.position().x)
            && i64::from(pin.y) < i64::from(m.position().y) + i64::from(m.size().height)
            && i64::from(pin.y) + i64::from(pin.height) > i64::from(m.position().y)
    }) {
        pin.x = monitor.position().x + 40;
        pin.y = monitor.position().y + 40;
    }
    if show {
        pin.visible = true;
    }
    pin.lifecycle = "open".into();
    let scale = monitors.iter().find(|m| {
        i64::from(pin.x) >= i64::from(m.position().x)
            && i64::from(pin.x) < i64::from(m.position().x) + i64::from(m.size().width)
            && i64::from(pin.y) >= i64::from(m.position().y)
            && i64::from(pin.y) < i64::from(m.position().y) + i64::from(m.size().height)
    }).unwrap_or(&monitor).scale_factor();
    let url = if dismiss_capture {
        format!("index.html?panel=screenshot&mode=pin&id={id}&capture=1")
    } else {
        format!("index.html?panel=screenshot&mode=pin&id={id}")
    };
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(url.into()),
    )
    .title("桌面贴图")
    .decorations(false)
    .resizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .focused(false)
    .transparent(true)
    .background_color(tauri::window::Color(0, 0, 0, 0))
    .shadow(false)
    // 构造器使用逻辑像素，文档保存的是物理像素；不能直接混用。
    .position(f64::from(pin.x) / scale, f64::from(pin.y) / scale)
    .inner_size(f64::from(pin.width) / scale, f64::from(pin.height) / scale)
    .visible(false)
    .build()
    .map_err(failure)?;
    #[cfg(windows)]
    windows::disable_window_transitions(&window)?;
    window
        .set_position(PhysicalPosition::new(pin.x, pin.y))
        .map_err(failure)?;
    window
        .set_size(PhysicalSize::new(pin.width, pin.height))
        .map_err(failure)?;
    // 创建时不抢焦点，点击贴图后允许接收移动和复制快捷键。
    window.set_focusable(true).map_err(failure)?;
    {
        let mut guard = service.session.lock().unwrap();
        let mut next = guard.clone();
        next.entries.iter_mut().find(|e| e.id == id).unwrap().pin = Some(pin);
        service.commit(&next)?;
        *guard = next;
    }
    // 页面先透明显示并确认最终视口，再绘制图片、结束框选，避免首帧缩放和闪烁。
    changed(app);
    Ok(())
}

pub fn window_event(app: &AppHandle, label: &str, event: &tauri::WindowEvent) {
    let Some(service) = app.try_state::<ScreenshotService>() else {
        return;
    };
    if label == "screenshot-capture" {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            service.capturing.store(false, Ordering::SeqCst);
            *service.frame.lock().unwrap() = None;
            let _ = end_long_capture(app);
        }
        return;
    }
    let Some(id) = label.strip_prefix("screenshot-pin-") else {
        return;
    };
    if matches!(
        event,
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
    ) {
        if service.menu_windows.lock().unwrap().contains_key(id) {
            return;
        }
        {
            let mut session = service.session.lock().unwrap();
            if let Some(pin) = session
                .entries
                .iter_mut()
                .find(|e| e.id == id)
                .and_then(|e| e.pin.as_mut())
            {
                match event {
                    tauri::WindowEvent::Moved(p) => {
                        pin.x = p.x;
                        pin.y = p.y;
                    }
                    tauri::WindowEvent::Resized(s) => {
                        pin.width = s.width;
                        pin.height = s.height;
                    }
                    _ => {}
                }
            }
        }
        // 拖动和缩放仅更新内存，合并 250ms 内的视图变化再持锁提交。
        if !service.view_save_pending.swap(true, Ordering::SeqCst) {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let service = app.state::<ScreenshotService>();
                let session = service.session.lock().unwrap();
                if let Err(error) = service.commit(&session) {
                    let _ = app.emit("screenshot:error", error.to_string());
                }
                service.view_save_pending.store(false, Ordering::SeqCst);
            });
        }
        return;
    }
    let result = match event {
        tauri::WindowEvent::CloseRequested { .. } => service.update_pin(id, |pin| {
            pin.lifecycle = "closed".into();
            pin.visible = false;
        }),
        _ => Ok(()),
    };
    if let Err(error) = result {
        let _ = app.emit("screenshot:error", error.to_string());
    }
    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
        changed(app);
    }
}

pub fn toggle_pins(app: &AppHandle) -> Result<(), ManagerError> {
    let windows = app
        .webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with("screenshot-pin-"))
        .collect::<Vec<_>>();
    let visible = !windows.iter().any(|(_, w)| w.is_visible().unwrap_or(false));
    let service = app.state::<ScreenshotService>();
    for (label, window) in windows {
        if visible {
            window.show()
        } else {
            window.hide()
        }
        .map_err(failure)?;
        service.update_pin(label.trim_start_matches("screenshot-pin-"), |p| {
            p.visible = visible
        })?;
    }
    changed(app);
    Ok(())
}

pub fn switch_pin_group(app: &AppHandle) -> Result<(), ManagerError> {
    let service = app.state::<ScreenshotService>();
    let mut guard = service.session.lock().unwrap();
    let mut next = guard.clone();
    let mut groups = Vec::new();
    for entry in &next.entries {
        if let Some(pin) = entry.pin.as_ref().filter(|pin| pin.lifecycle == "open") {
            if !groups.contains(&pin.group) {
                groups.push(pin.group.clone());
            }
        }
    }
    if groups.len() < 2 {
        return Ok(());
    }
    let current = next
        .entries
        .iter()
        .find_map(|entry| {
            entry
                .pin
                .as_ref()
                .filter(|pin| pin.lifecycle == "open" && pin.visible)
                .map(|pin| pin.group.clone())
        })
        .unwrap_or_else(|| groups[0].clone());
    let current_index = groups.iter().position(|group| group == &current).unwrap_or(0);
    let next_group = &groups[(current_index + 1) % groups.len()];
    for entry in &mut next.entries {
        if let Some(pin) = entry.pin.as_mut().filter(|pin| pin.lifecycle == "open") {
            pin.visible = &pin.group == next_group;
        }
    }
    service.commit(&next)?;
    *guard = next;
    drop(guard);
    for (label, window) in app.webview_windows() {
        let Some(id) = label.strip_prefix("screenshot-pin-") else {
            continue;
        };
        let visible = service
            .session
            .lock()
            .unwrap()
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.pin.as_ref())
            .is_some_and(|pin| pin.lifecycle == "open" && pin.visible);
        if visible {
            window.show().map_err(failure)?;
        } else {
            window.hide().map_err(failure)?;
        }
    }
    changed(app);
    Ok(())
}

pub fn reset_input(app: &AppHandle) -> Result<(), ManagerError> {
    for (label, window) in app.webview_windows() {
        if label.starts_with("screenshot-pin-") {
            window.set_ignore_cursor_events(false).map_err(failure)?;
        }
    }
    Ok(())
}

pub fn paste_clipboard(app: &AppHandle) -> Result<(), ManagerError> {
    app.state::<ScreenshotService>().ensure_enabled()?;
    #[cfg(windows)]
    {
        let images = windows::read_clipboard()?;
        for image in images {
            let data = image_data(&image)?;
            let id = save_document(
                app,
                json!({"source": data, "output": data, "annotations": []}),
            )?;
            open_pin(app, &id)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    Err(failure("剪贴板贴图目前仅支持 Windows"))
}

pub fn start(app: AppHandle) {
    #[cfg(windows)]
    windows::start_hotkeys(app.clone());
    if let Err(error) = sync_runtime(&app) {
        let _ = app.emit("screenshot:error", error.to_string());
    }
}

fn sync_runtime(app: &AppHandle) -> Result<(), ManagerError> {
    let service = app.state::<ScreenshotService>();
    if !service.settings().enabled {
        cancel_capture(app)?;
        service.hotkeys_paused.store(false, Ordering::SeqCst);
        service.hotkey_errors.lock().unwrap().clear();
        // 直接销毁窗口，保留贴图的打开状态供下次启动恢复，不触发历史删除。
        for (label, window) in app.webview_windows() {
            if label.starts_with("screenshot-") {
                window.destroy().map_err(failure)?;
            }
        }
        service.menu_windows.lock().unwrap().clear();
        return Ok(());
    }
    let bounds = app
        .available_monitors()
        .ok()
        .and_then(|monitors| capture_bounds(&monitors).ok());
    ensure_capture_window(app, bounds)?;
    if !service.settings().restore_pins {
        return Ok(());
    }
    let ids = service
        .session
        .lock()
        .unwrap()
        .entries
        .iter()
        .filter(|e| e.pin.as_ref().is_some_and(|p| p.lifecycle == "open"))
        .map(|e| e.id.clone())
        .collect::<Vec<_>>();
    for id in ids {
        if app.get_webview_window(&format!("screenshot-pin-{id}")).is_some() {
            continue;
        }
        if let Err(error) = open_pin_with_visibility(app, &id, false, false) {
            service
                .startup_errors
                .lock()
                .unwrap()
                .push(error.to_string());
        }
    }
    Ok(())
}

pub fn flush(app: &AppHandle) {
    if let Some(service) = app.try_state::<ScreenshotService>() {
        let session = service.session.lock().unwrap();
        if let Err(error) = service.commit(&session) {
            eprintln!("保存截图贴图会话失败：{error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let path = std::env::current_dir()
                .unwrap()
                .join("target")
                .join("screenshot-tests")
                .join(uuid::Uuid::new_v4().to_string());
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn history_entry(age_seconds: i64, lifecycle: Option<&str>) -> Entry {
        Entry {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: (chrono::Utc::now() - chrono::Duration::seconds(age_seconds)).to_rfc3339(),
            width: 800,
            height: 600,
            origin: None,
            pin: lifecycle.map(|lifecycle| Pin {
                x: 0,
                y: 0,
                width: 400,
                height: 300,
                opacity: 1.0,
                group: default_pin_group(),
                lifecycle: lifecycle.into(),
                visible: lifecycle == "open",
            }),
        }
    }

    #[test]
    fn history_limit_counts_all_pin_lifecycles_and_evicts_oldest() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let mut session = service.session.lock().unwrap().clone();
        let lifecycles = [None, Some("open"), Some("closed"), Some("destroyed")];
        session.entries = (0..55)
            .rev()
            .map(|age| history_entry(age, lifecycles[age as usize % lifecycles.len()]))
            .collect();
        let expected: Vec<_> = session.entries.iter().rev().take(50).map(|entry| entry.id.clone()).collect();
        let expected_expired: Vec<_> = session.entries.iter().take(5).rev().map(|entry| entry.id.clone()).collect();
        let expired = service.prune_history(&mut session);
        assert_eq!(expired, expected_expired);
        assert_eq!(session.entries.iter().map(|entry| entry.id.clone()).collect::<Vec<_>>(), expected);
        assert!(service.prune_history(&mut session).is_empty());
        assert_eq!(session.entries.len(), 50);
        session.entries.clear();
        assert!(service.prune_history(&mut session).is_empty());
    }

    #[test]
    fn startup_prunes_history_files_and_persists_the_limit() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let mut session = service.session.lock().unwrap().clone();
        session.entries = (0..53).map(|age| history_entry(age, Some("closed"))).collect();
        for entry in &session.entries {
            let dir = service.document_dir(&entry.id).unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            for file in ["source.png", "output.png", "thumbnail.png", "document.json"] {
                std::fs::write(dir.join(file), b"test").unwrap();
            }
        }
        let retained: Vec<_> = session.entries[..50].iter().map(|entry| entry.id.clone()).collect();
        let expired: Vec<_> = session.entries[50..].iter().map(|entry| entry.id.clone()).collect();
        service.commit(&session).unwrap();
        drop(service);
        let restored = ScreenshotService::new(&root.0).unwrap();
        assert_eq!(restored.session.lock().unwrap().entries.len(), 50);
        assert!(restored.startup_errors.lock().unwrap().is_empty());
        for id in &retained {
            assert!(restored.document_dir(id).unwrap().join("output.png").exists());
        }
        for id in &expired {
            assert!(!restored.document_dir(id).unwrap().exists());
        }
        let persisted: Session = serde_json::from_slice(&std::fs::read(restored.root.join("session.json")).unwrap()).unwrap();
        assert_eq!(persisted.entries.iter().map(|entry| entry.id.clone()).collect::<Vec<_>>(), retained);
        drop(restored);
        let reloaded = ScreenshotService::new(&root.0).unwrap();
        assert_eq!(reloaded.session.lock().unwrap().entries.len(), 50);
    }

    #[test]
    fn ordinary_history_still_expires_but_keeps_latest_output() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let mut session = service.session.lock().unwrap().clone();
        session.entries = (40..43).map(|days| history_entry(days * 24 * 60 * 60, None)).collect();
        let latest_id = session.entries[0].id.clone();
        assert_eq!(service.prune_history(&mut session).len(), 2);
        assert_eq!(session.entries.len(), 1);
        assert_eq!(session.entries[0].id, latest_id);
    }

    #[test]
    fn custom_history_limit_is_applied_and_old_settings_keep_defaults() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let mut session = service.session.lock().unwrap().clone();
        session.entries = (0..5).map(|age| history_entry(age, Some("open"))).collect();
        session.settings.history_limit = 2;
        assert_eq!(service.prune_history(&mut session).len(), 3);
        assert_eq!(session.entries.len(), 2);
        session.settings.history_limit = 1;
        assert_eq!(service.prune_history(&mut session).len(), 1);
        assert_eq!(session.entries.len(), 1);
        let old: Settings = serde_json::from_value(json!({"hotkeys":true,"shortcutStyle":"shift","restorePins":true})).unwrap();
        assert!(old.enabled);
        assert_eq!(old.history_limit, 50);
        assert_eq!(old.shortcut_bindings()[0].2, "Ctrl+F1");
        assert_eq!(old.shortcut_bindings()[1].2, "Shift+F1");
        assert_eq!(old.shortcut_bindings()[2].2, "F3");
        assert_eq!(old.shortcut_bindings()[3].2, "Shift+F3");
        assert_eq!(old.shortcut_bindings()[4].2, "Ctrl+F3");
        old.validate().unwrap();
    }

    #[test]
    fn extension_toggle_persists_without_changing_history_or_configuration() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let mut session = service.session.lock().unwrap();
        session.settings.hotkeys = false;
        session.settings.history_limit = 25;
        session.settings.shortcuts.insert("capture".into(), "Ctrl+F8".into());
        let entry = history_entry(0, Some("open"));
        let dir = service.document_dir(&entry.id).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("output.png"), b"retained-image").unwrap();
        session.entries.push(entry);
        let mut expected = serde_json::to_value(&*session).unwrap();
        drop(session);

        service.set_enabled(false).unwrap();
        assert!(service.ensure_enabled().is_err());
        expected["settings"]["enabled"] = json!(false);
        assert_eq!(serde_json::to_value(&*service.session.lock().unwrap()).unwrap(), expected);
        drop(service);

        let restored = ScreenshotService::new(&root.0).unwrap();
        assert!(!restored.settings().enabled);
        assert!(restored.ensure_enabled().is_err());
        assert_eq!(std::fs::read(dir.join("output.png")).unwrap(), b"retained-image");
        restored.set_enabled(true).unwrap();
        restored.ensure_enabled().unwrap();
        expected["settings"]["enabled"] = json!(true);
        assert_eq!(serde_json::to_value(&*restored.session.lock().unwrap()).unwrap(), expected);
    }

    #[test]
    fn extension_toggle_write_failure_keeps_saved_state() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        std::fs::create_dir(service.root.join("session.json")).unwrap();
        assert!(service.set_enabled(false).is_err());
        assert!(service.settings().enabled);
        service.ensure_enabled().unwrap();
    }

    #[test]
    fn settings_validate_shortcuts_and_history_bounds() {
        let mut settings = Settings::default();
        for limit in [0, 501] {
            settings.history_limit = limit;
            assert!(settings.validate().is_err());
        }
        settings.history_limit = 100;
        settings.shortcuts.insert("capture".into(), "Shift+Alt+F12".into());
        settings.validate().unwrap();
        assert_eq!(parse_shortcut("Shift+Alt+F12").unwrap(), (5, 0x7b));
        assert_eq!(parse_shortcut("PrintScreen").unwrap(), (0, 0x2c));
        assert_eq!(parse_shortcut("Win+Space").unwrap(), (8, 0x20));
        for shortcut in ["A", "Ctrl", "Ctrl+Ctrl+A", "Ctrl+A+B", "F25", "Ctrl+Escape", ""] {
            assert!(parse_shortcut(shortcut).is_err(), "{shortcut}");
        }
        settings.shortcuts.insert("clipboard".into(), "alt+shift+F12".into());
        assert!(settings.validate().is_err());
    }

    #[test]
    fn capture_pixels_are_lossless_and_origin_survives_reload() {
        let image = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8 * 30, y as u8 * 60, 255, 255])));
        let pixels = image.to_rgba8().into_raw();
        for auto_copy in [false, true] {
            let frame = capture_frame_data(image.clone(), -1275, 146, auto_copy);
            assert_eq!(i32::from_le_bytes(frame[0..4].try_into().unwrap()), -1275);
            assert_eq!(i32::from_le_bytes(frame[4..8].try_into().unwrap()), 146);
            assert_eq!(u32::from_le_bytes(frame[8..12].try_into().unwrap()), 3);
            assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 2);
            assert_eq!(u32::from_le_bytes(frame[16..20].try_into().unwrap()), u32::from(auto_copy));
            assert_eq!(&frame[20..], pixels.as_slice());
        }
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let mut session = service.session.lock().unwrap().clone();
        let mut entry = history_entry(0, None);
        entry.origin = Some(Position { x: -1275, y: 146 });
        entry.width = 527;
        entry.height = 314;
        session.entries.push(entry);
        service.commit(&session).unwrap();
        drop(service);
        let restored = ScreenshotService::new(&root.0).unwrap();
        let session = restored.session.lock().unwrap();
        let entry = &session.entries[0];
        let origin = entry.origin.as_ref().unwrap();
        assert_eq!((origin.x, origin.y, entry.width, entry.height), (-1275, 146, 527, 314));
    }

    #[test]
    fn custom_shortcuts_survive_reload_and_missing_actions_use_defaults() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let mut session = service.session.lock().unwrap().clone();
        let shortcuts = [
            ("captureAutoCopy", "Alt+F6"),
            ("capture", "Ctrl+Shift+F8"),
            ("clipboard", "F9"),
            ("toggle", "Shift+F9"),
            ("switchGroup", "Ctrl+F9"),
            ("resetInput", "Ctrl+Alt+F10"),
        ];
        for (action, shortcut) in shortcuts {
            session.settings.shortcuts.insert(action.into(), shortcut.into());
        }
        session.settings.validate().unwrap();
        service.commit(&session).unwrap();
        drop(service);
        let restored = ScreenshotService::new(&root.0).unwrap();
        for ((_, action, shortcut), (expected_action, expected_shortcut)) in restored.settings().shortcut_bindings().into_iter().zip(shortcuts) {
            assert_eq!((action, shortcut.as_str()), (expected_action, expected_shortcut));
        }
        let old: Settings = serde_json::from_value(json!({
            "hotkeys": true, "shortcutStyle": "shift", "restorePins": true,
            "shortcuts": {"capture": "Ctrl+Shift+A", "clipboard": "Ctrl+Shift+P"}
        })).unwrap();
        old.validate().unwrap();
        assert_eq!(old.shortcut_bindings()[0].2, "Ctrl+F1");
        assert_eq!(old.shortcut_bindings()[1].2, "Ctrl+Shift+A");
        assert_eq!(old.shortcut_bindings()[2].2, "Ctrl+Shift+P");
        assert_eq!(old.shortcut_bindings()[4].2, "Ctrl+F3");
    }

    #[test]
    fn session_roundtrip_preserves_pin_lifecycle_and_negative_coordinates() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let mut session = service.session.lock().unwrap().clone();
        session.entries.push(Entry {
            id,
            width: 800,
            height: 600,
            created_at: chrono::Local::now().to_rfc3339(),
            origin: None,
            pin: Some(Pin {
                x: -1200,
                y: -90,
                width: 400,
                height: 300,
                opacity: 0.55,
                group: default_pin_group(),
                lifecycle: "closed".into(),
                visible: false,
            }),
        });
        service.commit(&session).unwrap();
        drop(service);
        let restored = ScreenshotService::new(&root.0).unwrap();
        let session = restored.session.lock().unwrap();
        let pin = session.entries[0].pin.as_ref().unwrap();
        assert_eq!((pin.x, pin.y), (-1200, -90));
        assert_eq!(pin.lifecycle, "closed");
        assert_eq!(pin.opacity, 0.55);
        assert!(!pin.visible);
        assert_eq!(restored.startup_errors.lock().unwrap().len(), 1);
    }

    #[test]
    fn atomic_commit_replaces_existing_manifest() {
        let root = TestRoot::new();
        let file = root.0.join("session.json");
        atomic_write(&file, b"old").unwrap();
        atomic_write(&file, b"new").unwrap();
        assert_eq!(std::fs::read(file).unwrap(), b"new");
        assert!(!root.0.join("session.tmp").exists());
    }

    #[test]
    fn corrupt_or_future_session_is_not_overwritten() {
        let root = TestRoot::new();
        std::fs::create_dir_all(root.0.join("screenshots")).unwrap();
        let file = root.0.join("screenshots/session.json");
        for bytes in [b"{broken".as_slice(), br#"{"schemaVersion":9,"settings":{"hotkeys":true,"shortcutStyle":"alt","restorePins":true},"entries":[]}"#.as_slice()] {
            std::fs::write(&file, bytes).unwrap();
            assert!(ScreenshotService::new(&root.0).is_err());
            assert_eq!(std::fs::read(&file).unwrap(), bytes);
        }
    }

    #[test]
    fn document_paths_cannot_escape_data_root() {
        let root = TestRoot::new();
        let service = ScreenshotService::new(&root.0).unwrap();
        for id in ["../settings", "C:\\Windows", "..\\other", ""] {
            assert!(service.document_dir(id).is_err());
        }
        let id = uuid::Uuid::new_v4().to_string();
        assert!(service
            .document_dir(&id)
            .unwrap()
            .starts_with(root.0.join("screenshots/documents")));
        for kind in ["source", "output"] {
            assert_eq!(document_image_path(&service, &id, kind).unwrap(), service.document_dir(&id).unwrap().join(format!("{kind}.png")));
        }
        assert!(document_image_path(&service, "../other", "output").is_err());
        assert!(document_image_path(&service, &id, "../session.json").is_err());
    }

    #[test]
    fn saved_png_reuses_bytes_and_preserves_transparency() {
        let root = TestRoot::new();
        let image = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8 * 60, y as u8 * 90, 100, x as u8 * 100])));
        let bytes = encode_png_fast(&image).unwrap();
        let path = root.0.join("image.png");
        save_png(&path, &bytes, &image).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(decode_bytes(&std::fs::read(&path).unwrap()).unwrap().to_rgba8(), image.to_rgba8());
        // 非 PNG 输入仍编码为 PNG，保留旧导入流程的格式转换能力。
        save_png(&path, &[], &image).unwrap();
        assert_eq!(decode_bytes(&std::fs::read(&path).unwrap()).unwrap().to_rgba8(), image.to_rgba8());
    }

    #[test]
    fn supplied_thumbnail_is_saved_without_resampling_the_original() {
        let root = TestRoot::new();
        let original = DynamicImage::new_rgba8(800, 600);
        let preview = DynamicImage::new_rgba8(320, 180);
        let data = image_data(&preview).unwrap();
        let path = root.0.join("thumbnail.png");
        save_thumbnail(&path, &original, Some(&data)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), encode_png_fast(&preview).unwrap());
        let oversized = image_data(&DynamicImage::new_rgba8(321, 180)).unwrap();
        assert!(save_thumbnail(&path, &original, Some(&oversized)).is_err());
        assert!(save_thumbnail(&path, &original, Some("broken")).is_err());
        save_thumbnail(&path, &original, None).unwrap();
        let fallback = decode_bytes(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!((fallback.width(), fallback.height()), (267, 200));
    }

    #[cfg(windows)]
    #[test]
    fn second_instance_cannot_overwrite_the_same_session() {
        let root = TestRoot::new();
        let first = ScreenshotService::new(&root.0).unwrap();
        assert!(ScreenshotService::new(&root.0).is_err());
        drop(first);
        assert!(ScreenshotService::new(&root.0).is_ok());
    }

    #[test]
    fn image_decode_preserves_rgba_and_rejects_invalid_input() {
        let mut image = image::RgbaImage::new(2, 1);
        image.put_pixel(0, 0, image::Rgba([10, 20, 30, 0]));
        image.put_pixel(1, 0, image::Rgba([40, 50, 60, 128]));
        let encoded = image_data(&DynamicImage::ImageRgba8(image.clone())).unwrap();
        assert_eq!(decode(&encoded).unwrap().to_rgba8(), image);
        assert!(decode("data:image/png;base64,broken!").is_err());
    }
}

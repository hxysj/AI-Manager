use crate::core::error::ManagerError;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
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
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pin {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    opacity: f64,
    lifecycle: String,
    visible: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub hotkeys: bool,
    pub shortcut_style: String,
    pub restore_pins: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkeys: true,
            shortcut_style: "alt".into(),
            restore_pins: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    schema_version: u32,
    settings: Settings,
    entries: Vec<Entry>,
}

pub struct ScreenshotService {
    root: PathBuf,
    _instance_lock: std::fs::File,
    session: Mutex<Session>,
    frame: Mutex<Option<Value>>,
    capturing: AtomicBool,
    view_save_pending: AtomicBool,
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
        Ok(Self {
            root,
            _instance_lock: instance_lock,
            session: Mutex::new(session),
            frame: Mutex::new(None),
            capturing: AtomicBool::new(false),
            view_save_pending: AtomicBool::new(false),
            hotkey_errors: Mutex::new(Vec::new()),
            startup_errors: Mutex::new(errors),
        })
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

    pub fn settings(&self) -> Settings {
        self.session.lock().unwrap().settings.clone()
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

fn image_data(image: &DynamicImage) -> Result<String, ManagerError> {
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(failure)?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(buffer.into_inner())
    ))
}

fn decode(data: &str) -> Result<DynamicImage, ManagerError> {
    if data.len() > 128 * 1024 * 1024 {
        return Err(failure("图片文件超过 96 MiB 限制"));
    }
    let bytes = STANDARD
        .decode(data.split_once(',').map(|(_, b)| b).unwrap_or(data))
        .map_err(failure)?;
    decode_bytes(&bytes)
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
    let (w, h) = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(failure)?
        .into_dimensions()
        .map_err(failure)?;
    if u64::from(w) * u64::from(h) > 40_000_000 {
        return Err(failure("图片超过 4000 万像素限制"));
    }
    reader.decode().map_err(failure)
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
    match action.as_str() {
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
            if !matches!(settings.shortcut_style.as_str(), "alt" | "shift") {
                return Err(failure("不支持的快捷键组合"));
            }
            let mut guard = service.session.lock().unwrap();
            let mut next = guard.clone();
            next.settings = settings;
            service.commit(&next)?;
            *guard = next;
            Ok(json!(true))
        }
        "capture" => {
            drop(service);
            begin_capture(&app).await?;
            Ok(json!(true))
        }
        "frame" => service
            .frame
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| failure("截图会话已结束")),
        "ready" => {
            let window = app
                .get_webview_window("screenshot-capture")
                .ok_or_else(|| failure("截图窗口已关闭"))?;
            window.show().map_err(failure)?;
            window.set_focus().map_err(failure)?;
            Ok(json!(true))
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
            Ok(json!(save_document(&app, payload)?))
        }
        "document" => {
            let dir = service.document_dir(id_from(&payload)?)?;
            let mut document: Value =
                serde_json::from_slice(&std::fs::read(dir.join("document.json"))?)?;
            document["source"] = json!(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(std::fs::read(dir.join("source.png"))?)
            ));
            document["output"] = json!(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(std::fs::read(dir.join("output.png"))?)
            ));
            document["pin"] = json!(service
                .session
                .lock()
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.id == id_from(&payload).unwrap_or(""))
                .and_then(|entry| entry.pin.clone()));
            Ok(document)
        }
        "pin" => {
            drop(service);
            open_pin(&app, id_from(&payload)?)?;
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
            let dir = service.document_dir(id_from(&payload)?)?;
            drop(service);
            use tauri_plugin_dialog::DialogExt;
            let path = app
                .dialog()
                .file()
                .set_title("保存截图")
                .set_file_name("截图.png")
                .add_filter("PNG 图片", &["png"])
                .blocking_save_file();
            if let Some(path) = path {
                let path = path.into_path().map_err(failure)?;
                std::fs::copy(dir.join("output.png"), &path)?;
                Ok(json!({"saved": true, "path": path}))
            } else {
                Ok(json!({"saved": false}))
            }
        }
        "toggle" => {
            drop(service);
            toggle_pins(&app)?;
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
                "hide" => {
                    window.hide().map_err(failure)?;
                    service.update_pin(id, |p| p.visible = false)?;
                }
                "close" => {
                    window.close().map_err(failure)?;
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
                    window.set_focusable(false).map_err(failure)?;
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

fn save_document(app: &AppHandle, payload: Value) -> Result<String, ManagerError> {
    let service = app.state::<ScreenshotService>();
    let source = decode(
        payload["source"]
            .as_str()
            .ok_or_else(|| failure("缺少原图"))?,
    )?;
    let output = decode(
        payload["output"]
            .as_str()
            .ok_or_else(|| failure("缺少渲染结果"))?,
    )?;
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
    source
        .save_with_format(dir.join("source.png"), ImageFormat::Png)
        .map_err(failure)?;
    output
        .save_with_format(dir.join("output.png"), ImageFormat::Png)
        .map_err(failure)?;
    output
        .thumbnail(320, 200)
        .save_with_format(dir.join("thumbnail.png"), ImageFormat::Png)
        .map_err(failure)?;
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
        },
    );
    // 有活动或可恢复贴图的文档保留；普通历史最多保留 50 条。
    let mut ordinary = 0;
    let mut history_bytes = 0_u64;
    let mut expired = Vec::new();
    next.entries.retain(|entry| {
        if entry
            .pin
            .as_ref()
            .is_some_and(|p| p.lifecycle != "destroyed")
        {
            return true;
        }
        ordinary += 1;
        let dir = service.root.join("documents").join(&entry.id);
        let bytes: u64 = ["source.png", "output.png", "thumbnail.png", "document.json"].iter()
            .filter_map(|name| std::fs::metadata(dir.join(name)).ok()).map(|m| m.len()).sum();
        history_bytes = history_bytes.saturating_add(bytes);
        let old = chrono::DateTime::parse_from_rfc3339(&entry.created_at)
            .is_ok_and(|date| chrono::Utc::now().signed_duration_since(date).num_days() > 30);
        // 至少保留刚提交的输出，防止超预算导致后续复制或贴图引用失效。
        if ordinary > 1 && (ordinary > 50 || history_bytes > 512 * 1024 * 1024 || old) {
            expired.push(entry.id.clone());
            false
        } else {
            true
        }
    });
    service.commit(&next)?;
    *guard = next;
    drop(guard);
    for old_id in expired {
        let _ = std::fs::remove_dir_all(service.document_dir(&old_id)?);
    }
    changed(app);
    Ok(id)
}

pub async fn begin_capture(app: &AppHandle) -> Result<(), ManagerError> {
    let service = app.state::<ScreenshotService>();
    if service.capturing.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = async {
        let monitors = app.available_monitors().map_err(failure)?;
        if monitors.is_empty() { return Err(failure("没有可截图的显示器")); }
        let left = monitors.iter().map(|m| m.position().x).min().unwrap();
        let top = monitors.iter().map(|m| m.position().y).min().unwrap();
        let right = monitors.iter().map(|m| i64::from(m.position().x) + i64::from(m.size().width)).max().unwrap();
        let bottom = monitors.iter().map(|m| i64::from(m.position().y) + i64::from(m.size().height)).max().unwrap();
        let width = (right - i64::from(left)) as u32;
        let height = (bottom - i64::from(top)) as u32;
        if u64::from(width) * u64::from(height) > 40_000_000 { return Err(failure("虚拟桌面超过 4000 万像素，请减少显示区域")); }
        #[cfg(windows)]
        let frame = windows::capture(&monitors, left, top, width, height)?;
        #[cfg(not(windows))]
        let frame: DynamicImage = return Err(failure("桌面截图目前仅支持 Windows"));
        *service.frame.lock().unwrap() = Some(json!({"source": image_data(&frame)?, "left": left, "top": top, "width": width, "height": height}));
        let window = WebviewWindowBuilder::new(app, "screenshot-capture", WebviewUrl::App("index.html?panel=screenshot&mode=capture".into()))
            .title("截图贴图").decorations(false).resizable(false).skip_taskbar(true).always_on_top(true).shadow(false).visible(false).build().map_err(failure)?;
        window.set_position(PhysicalPosition::new(left, top)).map_err(failure)?;
        window.set_size(PhysicalSize::new(width, height)).map_err(failure)?;
        Ok(())
    }.await;
    if result.is_err() {
        let _ = cancel_capture(app);
    }
    result
}

fn cancel_capture(app: &AppHandle) -> Result<(), ManagerError> {
    if let Some(window) = app.get_webview_window("screenshot-capture") {
        window.destroy().map_err(failure)?;
    }
    let service = app.state::<ScreenshotService>();
    *service.frame.lock().unwrap() = None;
    service.capturing.store(false, Ordering::SeqCst);
    Ok(())
}

fn open_pin(app: &AppHandle, id: &str) -> Result<(), ManagerError> {
    open_pin_with_visibility(app, id, true)
}

fn open_pin_with_visibility(app: &AppHandle, id: &str, show: bool) -> Result<(), ManagerError> {
    let label = format!("screenshot-pin-{id}");
    let service = app.state::<ScreenshotService>();
    service.document_dir(id)?;
    if let Some(window) = app.get_webview_window(&label) {
        window.show().map_err(failure)?;
        service.update_pin(id, |p| p.visible = true)?;
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
    let initial_scale = (monitor.size().width as f64 * 0.75 / entry.width.max(1) as f64)
        .min(monitor.size().height as f64 * 0.75 / entry.height.max(1) as f64)
        .min(1.0);
    let mut pin = entry
        .pin
        .filter(|p| p.lifecycle != "destroyed")
        .unwrap_or(Pin {
            x: monitor.position().x + 80,
            y: monitor.position().y + 80,
            width: (entry.width as f64 * initial_scale).round().max(48.0) as u32,
            height: (entry.height as f64 * initial_scale).round().max(48.0) as u32,
            opacity: 1.0,
            lifecycle: "open".into(),
            visible: true,
        });
    if pin.height == 0 {
        pin.height = ((pin.width as f64 / entry.width as f64 * entry.height as f64) as u32).max(48);
    }
    let fit = (monitor.size().width as f64 / pin.width.max(1) as f64)
        .min(monitor.size().height as f64 / pin.height.max(1) as f64)
        .min(1.0);
    pin.width = (pin.width as f64 * fit).round().max(48.0) as u32;
    pin.height = (pin.height as f64 * fit).round().max(48.0) as u32;
    if !monitors.iter().any(|m| {
        pin.x < m.position().x + m.size().width as i32 - 32
            && pin.x + 32 > m.position().x
            && pin.y < m.position().y + m.size().height as i32 - 32
            && pin.y + 32 > m.position().y
    }) {
        pin.x = monitor.position().x + 40;
        pin.y = monitor.position().y + 40;
    }
    if show {
        pin.visible = true;
    }
    let visible = pin.visible;
    pin.lifecycle = "open".into();
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("index.html?panel=screenshot&mode=pin&id={id}").into()),
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
    .visible(false)
    .build()
    .map_err(failure)?;
    window
        .set_position(PhysicalPosition::new(pin.x, pin.y))
        .map_err(failure)?;
    window
        .set_size(PhysicalSize::new(pin.width, pin.height))
        .map_err(failure)?;
    window.set_focusable(false).map_err(failure)?;
    {
        let mut guard = service.session.lock().unwrap();
        let mut next = guard.clone();
        next.entries.iter_mut().find(|e| e.id == id).unwrap().pin = Some(pin);
        service.commit(&next)?;
        *guard = next;
    }
    if visible {
        window.show().map_err(failure)?;
    }
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

pub fn reset_input(app: &AppHandle) -> Result<(), ManagerError> {
    for (label, window) in app.webview_windows() {
        if label.starts_with("screenshot-pin-") {
            window.set_ignore_cursor_events(false).map_err(failure)?;
        }
    }
    Ok(())
}

pub fn paste_clipboard(app: &AppHandle) -> Result<(), ManagerError> {
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
    let service = app.state::<ScreenshotService>();
    if !service.settings().restore_pins {
        return;
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
        if let Err(error) = open_pin_with_visibility(&app, &id, false) {
            service
                .startup_errors
                .lock()
                .unwrap()
                .push(error.to_string());
        }
    }
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
            pin: Some(Pin {
                x: -1200,
                y: -90,
                width: 400,
                height: 300,
                opacity: 0.55,
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

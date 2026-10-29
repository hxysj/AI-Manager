use super::error::ManagerError;
use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext};
use enigo::{Coordinate, Direction, Enigo, Key, Keyboard, Mouse};
use std::sync::{mpsc, Mutex, OnceLock};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

fn failure(error: impl std::fmt::Display) -> ManagerError {
    ManagerError::System(error.to_string())
}

// X11 的剪贴板所有权依赖连接存活，不能在每次写入后销毁上下文。
pub fn clipboard<T>(
    operation: impl FnOnce(&ClipboardContext) -> clipboard_rs::Result<T>,
) -> Result<T, ManagerError> {
    static CONTEXT: OnceLock<Mutex<Option<ClipboardContext>>> = OnceLock::new();
    let mut context = CONTEXT.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if context.is_none() {
        *context = Some(ClipboardContext::new().map_err(failure)?);
    }
    operation(context.as_ref().unwrap()).map_err(failure)
}

pub fn clipboard_files() -> Result<Vec<String>, ManagerError> {
    let files = if is_wayland() {
        String::from_utf8(wayland_clipboard(&["--type", "text/uri-list"], None)?)
            .map_err(failure)?
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(str::to_string)
            .collect()
    } else {
        clipboard(|context| context.get_files())?
    };
    Ok(files
        .into_iter()
        .filter_map(|file| {
            let path = if file.starts_with("file:") {
                url::Url::parse(&file).ok()?.to_file_path().ok()?
            } else {
                std::path::PathBuf::from(file)
            };
            path.is_file().then(|| path.to_string_lossy().into_owned())
        })
        .take(100)
        .collect())
}

fn wayland_clipboard(args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, ManagerError> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut command = Command::new(if input.is_some() {
        "wl-copy"
    } else {
        "wl-paste"
    });
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(bytes) = input {
        let mut child = command
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| failure(format!("无法运行 wl-copy，请安装 wl-clipboard：{error}")))?;
        let write = child
            .stdin
            .take()
            .ok_or_else(|| failure("无法写入系统剪贴板"))?
            .write_all(bytes);
        if let Err(error) = write {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(failure(format!(
                "系统剪贴板写入失败：{}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        return Ok(output.stdout);
    }
    let output = command
        .output()
        .map_err(|error| failure(format!("无法运行 wl-paste，请安装 wl-clipboard：{error}")))?;
    if !output.status.success() {
        return Err(failure(format!(
            "系统剪贴板读取失败：{}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(output.stdout)
}

pub fn clipboard_text() -> Result<String, ManagerError> {
    if is_wayland() {
        return String::from_utf8(wayland_clipboard(&["--no-newline"], None)?).map_err(failure);
    }
    clipboard(|context| context.get_text())
}

pub fn clipboard_image() -> Result<image::DynamicImage, ManagerError> {
    if is_wayland() {
        return image::load_from_memory(&wayland_clipboard(&["--type", "image/png"], None)?)
            .map_err(failure);
    }
    clipboard(|context| context.get_image()?.get_dynamic_image())
}

pub fn copy_image(image: &image::DynamicImage) -> Result<(), ManagerError> {
    if is_wayland() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(failure)?;
        wayland_clipboard(&["--type", "image/png"], Some(bytes.get_ref()))?;
        return Ok(());
    }
    clipboard(|context| {
        context.set_image(clipboard_rs::RustImageData::from_dynamic_image(
            image.clone(),
        ))
    })
}

pub fn is_wayland() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|value| value == "wayland"))
}

enum Input {
    Cursor,
    Copy,
    Scroll(i32, i32, i32),
}

fn input(action: Input) -> Result<(i32, i32), ManagerError> {
    type Request = (Input, mpsc::SyncSender<Result<(i32, i32), String>>);
    static INPUT: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
    let sender = INPUT.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Request>();
        // 原生输入连接在线程中创建和使用，避免跨线程访问 Cocoa 对象。
        std::thread::spawn(move || {
            let mut enigo = None;
            #[cfg(target_os = "linux")]
            let mut portal = None;
            for (action, reply) in receiver {
                let result = (|| -> Result<(i32, i32), String> {
                    #[cfg(target_os = "linux")]
                    if is_wayland() {
                        return tauri::async_runtime::block_on(wayland_input(&mut portal, action))
                            .map_err(|error| format!("桌面输入失败，请允许远程桌面权限：{error}"));
                    }
                    if enigo.is_none() {
                        enigo = Some(Enigo::new(&enigo::Settings::default()).map_err(|error| {
                            format!("无法连接桌面输入接口，请检查辅助功能权限：{error}")
                        })?);
                    }
                    let enigo = enigo.as_mut().unwrap();
                    let cursor = enigo.location().map_err(|error| error.to_string())?;
                    match action {
                        Input::Cursor => {}
                        Input::Copy => {
                            let modifier = if cfg!(target_os = "macos") {
                                Key::Meta
                            } else {
                                Key::Control
                            };
                            enigo
                                .key(modifier, Direction::Press)
                                .map_err(|error| error.to_string())?;
                            let result = enigo.key(Key::Unicode('c'), Direction::Click);
                            let release = enigo.key(modifier, Direction::Release);
                            result.and(release).map_err(|error| error.to_string())?;
                        }
                        Input::Scroll(x, y, delta) => {
                            enigo
                                .move_mouse(x, y, Coordinate::Abs)
                                .map_err(|error| error.to_string())?;
                            let result = enigo.scroll(-delta.signum() * 3, enigo::Axis::Vertical);
                            let restore = enigo.move_mouse(cursor.0, cursor.1, Coordinate::Abs);
                            result.and(restore).map_err(|error| error.to_string())?;
                        }
                    }
                    Ok(cursor)
                })();
                let _ = reply.send(result);
            }
        });
        sender
    });
    let (reply, result) = mpsc::sync_channel(1);
    sender.send((action, reply)).map_err(failure)?;
    result.recv().map_err(failure)?.map_err(failure)
}

pub fn cursor_position() -> Result<(i32, i32), ManagerError> {
    // Wayland 不提供全局光标查询；弹窗由调用方定位到当前显示器。
    if is_wayland() {
        return Err(failure("当前桌面不提供全局光标坐标"));
    }
    input(Input::Cursor)
}

pub fn physical_cursor_position(app: &AppHandle) -> Result<(i32, i32), ManagerError> {
    let (x, y) = cursor_position()?;
    if cfg!(target_os = "macos") {
        let screen = xcap::Monitor::from_point(x, y).map_err(failure)?;
        if let Some(monitor) = app
            .available_monitors()
            .map_err(failure)?
            .into_iter()
            .find(|monitor| monitor_matches(monitor, &screen))
        {
            return Ok((
                monitor.position().x
                    + ((x - screen.x().map_err(failure)?) as f64 * monitor.scale_factor()).round()
                        as i32,
                monitor.position().y
                    + ((y - screen.y().map_err(failure)?) as f64 * monitor.scale_factor()).round()
                        as i32,
            ));
        }
        return Err(failure("无法匹配光标所在显示器的物理边界"));
    }
    Ok((x, y))
}

pub fn monitor_matches(monitor: &tauri::Monitor, screen: &xcap::Monitor) -> bool {
    // Tauri 与 xcap 的屏幕名称格式不同，用物理边界匹配，避免同型号双屏错配。
    let Ok(scale) = screen.scale_factor().map(f64::from) else {
        return false;
    };
    let Ok(x) = screen.x() else {
        return false;
    };
    let Ok(y) = screen.y() else {
        return false;
    };
    let Ok(width) = screen.width() else {
        return false;
    };
    let Ok(height) = screen.height() else {
        return false;
    };
    [
        (f64::from(monitor.position().x), f64::from(x) * scale),
        (f64::from(monitor.position().y), f64::from(y) * scale),
        (f64::from(monitor.size().width), f64::from(width) * scale),
        (f64::from(monitor.size().height), f64::from(height) * scale),
    ]
    .into_iter()
    .all(|(actual, expected)| (actual - expected).abs() <= 2.0)
}

pub fn scroll_at(x: i32, y: i32, delta: i32) -> Result<(), ManagerError> {
    if delta != 0 {
        input(Input::Scroll(x, y, delta))?;
    }
    Ok(())
}

pub fn selected_text() -> Result<String, ManagerError> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"System Events\" to tell first application process whose frontmost is true to get value of attribute \"AXSelectedText\" of (value of attribute \"AXFocusedUIElement\")"])
            .output()?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout)
                .trim_end_matches('\n')
                .to_string();
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        // PRIMARY 是选区而非普通剪贴板，读取它不会破坏用户之前复制的内容。
        let mut command =
            std::process::Command::new(if is_wayland() { "wl-paste" } else { "xclip" });
        command.args(if is_wayland() {
            vec!["--primary", "--no-newline"]
        } else {
            vec!["-selection", "primary", "-out"]
        });
        if let Ok(output) = command.output() {
            if output.status.success() && !output.stdout.is_empty() {
                return String::from_utf8(output.stdout).map_err(failure);
            }
        }
    }
    // 应用未公开选区时，通过系统复制操作取词，超时不使用旧剪贴板冒充选区。
    let mut wayland_before = None;
    let mut wayland_marker = None;
    let before = if is_wayland() {
        if let Ok(types) = wayland_clipboard(&["--list-types"], None) {
            let types = String::from_utf8_lossy(&types);
            if let Some(mime) = [
                "text/uri-list",
                "image/png",
                "text/html",
                "text/plain;charset=utf-8",
                "text/plain",
            ]
            .into_iter()
            .find(|mime| types.lines().any(|offered| offered == *mime))
            {
                wayland_before = Some((
                    mime.to_string(),
                    wayland_clipboard(&["--type", mime], None)?,
                ));
            } else if !types.trim().is_empty() {
                return Err(failure(
                    "当前剪贴板格式无法恢复，请复制所选文字后使用剪贴板翻译",
                ));
            }
        } else {
            return Err(failure(
                "无法保存当前剪贴板，请复制所选文字后使用剪贴板翻译",
            ));
        }
        // 用可识别的文本替代清空，复制失败时仍能安全判断所有权并恢复原格式。
        let marker = format!("monkey-thief-selection-{}", uuid::Uuid::new_v4());
        wayland_clipboard(
            &["--type", "text/plain;charset=utf-8"],
            Some(marker.as_bytes()),
        )?;
        wayland_marker = Some(marker);
        Vec::new()
    } else {
        let before = clipboard(|context| {
            context.get(&[
                clipboard_rs::ContentFormat::Text,
                clipboard_rs::ContentFormat::Rtf,
                clipboard_rs::ContentFormat::Html,
                clipboard_rs::ContentFormat::Image,
                clipboard_rs::ContentFormat::Files,
            ])
        })?;
        clipboard(|context| context.clear())?;
        before
    };
    let result = (|| {
        input(Input::Copy)?;
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Ok(text) = clipboard_text() {
                if !text.is_empty() && wayland_marker.as_ref() != Some(&text) {
                    return Ok(text);
                }
            }
        }
        Err(failure(
            "未能复制所选文字，请确认目标应用允许复制或使用剪贴板翻译快捷键",
        ))
    })();
    // 仅在剪贴板仍为本次读取内容时恢复，避免覆盖用户期间主动复制的内容。
    let current = clipboard_text();
    // 非文字内容读取失败时保留现状，不能当作空剪贴板恢复并覆盖用户的新内容。
    if current.as_ref().is_ok_and(|current| {
        wayland_marker
            .as_ref()
            .map_or_else(|| current.is_empty(), |marker| marker == current)
            || result.as_ref().is_ok_and(|text| text == current)
    }) {
        if is_wayland() {
            if let Some((mime, bytes)) = wayland_before {
                let _ = wayland_clipboard(&["--type", &mime], Some(&bytes));
            } else {
                let _ = wayland_clipboard(&["--clear"], Some(&[]));
            }
        } else {
            let _ = clipboard(|context| context.set(before));
        }
    }
    result
}

pub fn start_shortcuts(
    app: AppHandle,
    bindings: fn(&AppHandle) -> Vec<(String, String)>,
    activate: fn(&AppHandle, &str),
    report: fn(&AppHandle, Vec<String>),
) {
    tauri::async_runtime::spawn(async move {
        let mut previous = Vec::new();
        let mut registered = Vec::<String>::new();
        #[cfg(target_os = "linux")]
        let mut portal_task: Option<tauri::async_runtime::JoinHandle<()>> = None;
        loop {
            let next = bindings(&app);
            if next != previous {
                for shortcut in registered.drain(..) {
                    let _ = app.global_shortcut().unregister(shortcut.as_str());
                }
                #[cfg(target_os = "linux")]
                if let Some(task) = portal_task.take() {
                    task.abort();
                }
                let mut errors = Vec::new();
                #[cfg(target_os = "linux")]
                if is_wayland() && !next.is_empty() {
                    report(&app, vec!["正在等待桌面门户授权快捷键".into()]);
                    let handle = app.clone();
                    let shortcuts = next.clone();
                    portal_task = Some(tauri::async_runtime::spawn(async move {
                        if let Err(error) =
                            portal_shortcuts(&handle, &shortcuts, activate, report).await
                        {
                            report(&handle, vec![format!("无法注册桌面门户快捷键：{error}")]);
                        }
                    }));
                }
                if !is_wayland() {
                    for (action, shortcut) in &next {
                        let shortcut = shortcut.replace("Win+", "Super+");
                        let action = action.clone();
                        match app.global_shortcut().on_shortcut(
                            shortcut.as_str(),
                            move |app, _, event| {
                                if event.state == ShortcutState::Pressed {
                                    activate(app, &action);
                                }
                            },
                        ) {
                            Ok(()) => registered.push(shortcut),
                            Err(error) => {
                                errors.push(format!("快捷键 {shortcut} 注册失败：{error}"))
                            }
                        }
                    }
                }
                if !is_wayland() || next.is_empty() {
                    report(&app, errors);
                }
                previous = next;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    });
}

#[cfg(target_os = "linux")]
fn portal_shortcut_labels() -> &'static Mutex<std::collections::HashMap<String, (String, String)>> {
    static LABELS: OnceLock<Mutex<std::collections::HashMap<String, (String, String)>>> =
        OnceLock::new();
    LABELS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

#[cfg(target_os = "linux")]
pub fn shortcut_label(action: &str) -> Option<String> {
    portal_shortcut_labels()
        .lock()
        .unwrap()
        .get(action)
        .map(|(_, label)| label.clone())
}

#[cfg(target_os = "linux")]
async fn portal_shortcuts(
    app: &AppHandle,
    bindings: &[(String, String)],
    activate: fn(&AppHandle, &str),
    report: fn(&AppHandle, Vec<String>),
) -> Result<(), ashpd::Error> {
    use ashpd::desktop::global_shortcuts::{
        Activated, GlobalShortcuts, NewShortcut, Shortcut, ShortcutsChanged,
    };
    use futures_util::StreamExt;
    let portal = GlobalShortcuts::new().await?;
    struct SessionGuard {
        session: Option<ashpd::desktop::Session<GlobalShortcuts>>,
        path: String,
    }
    impl Drop for SessionGuard {
        fn drop(&mut self) {
            portal_shortcut_labels()
                .lock()
                .unwrap()
                .retain(|_, (path, _)| path != &self.path);
            if let Some(session) = self.session.take() {
                tauri::async_runtime::spawn(async move {
                    let _ = session.close().await;
                });
            }
        }
    }
    let session = portal.create_session(Default::default()).await?;
    let session_path = serde_json::to_value(&session)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default();
    let guard = SessionGuard {
        session: Some(session),
        path: session_path.clone(),
    };
    let session = guard.session.as_ref().unwrap();
    let shortcuts = bindings
        .iter()
        .map(|(action, trigger)| {
            let trigger = trigger
                .replace("Ctrl+", "CTRL+")
                .replace("Alt+", "ALT+")
                .replace("Shift+", "SHIFT+")
                .replace("Win+", "LOGO+")
                .replace("PrintScreen", "Print");
            NewShortcut::new(action, action).preferred_trigger(Some(trigger.as_str()))
        })
        .collect::<Vec<_>>();
    enum Event {
        Activated(Activated),
        Changed(ShortcutsChanged),
        Closed,
    }
    let events = portal.receive_activated().await?.map(Event::Activated);
    let changes = portal
        .receive_shortcuts_changed()
        .await?
        .map(Event::Changed);
    let closed = session.receive_closed().await?.map(|_| Event::Closed);
    let mut events =
        futures_util::stream::select(events, futures_util::stream::select(changes, closed)).boxed();
    let update_labels = |shortcuts: &[Shortcut]| {
        let mut labels = portal_shortcut_labels().lock().unwrap();
        labels.retain(|_, (path, _)| path != &session_path);
        for shortcut in shortcuts {
            if bindings.iter().any(|(action, _)| action == shortcut.id())
                && !shortcut.trigger_description().trim().is_empty()
            {
                labels.insert(
                    shortcut.id().to_string(),
                    (
                        session_path.clone(),
                        shortcut.trigger_description().to_string(),
                    ),
                );
            }
        }
        let errors = bindings
            .iter()
            .filter(|(action, _)| !labels.contains_key(action))
            .map(|(action, _)| format!("桌面门户未授权快捷键：{action}"))
            .collect();
        drop(labels);
        report(app, errors);
    };
    let bound = portal
        .bind_shortcuts(&session, &shortcuts, None, Default::default())
        .await?
        .response()?;
    update_labels(bound.shortcuts());
    while let Some(event) = events.next().await {
        match event {
            Event::Activated(event) if event.session_handle().as_str() == session_path => {
                activate(app, event.shortcut_id())
            }
            Event::Changed(event) if event.session_handle().as_str() == session_path => {
                update_labels(event.shortcuts())
            }
            Event::Closed => break,
            _ => {}
        }
    }
    let _ = session.close().await;
    report(app, vec!["桌面快捷键会话已停止，请重新启用快捷键".into()]);
    Ok(())
}

#[cfg(target_os = "linux")]
struct PortalInput {
    portal: ashpd::desktop::remote_desktop::RemoteDesktop,
    session: std::sync::Arc<ashpd::desktop::Session<ashpd::desktop::remote_desktop::RemoteDesktop>>,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    watcher: tauri::async_runtime::JoinHandle<()>,
}

#[cfg(target_os = "linux")]
impl Drop for PortalInput {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

#[cfg(target_os = "linux")]
async fn wayland_input(
    state: &mut Option<PortalInput>,
    action: Input,
) -> Result<(i32, i32), String> {
    use ashpd::desktop::remote_desktop::{
        DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions,
    };
    if let Input::Scroll(x, y, delta) = action {
        crate::core::wayland::portal::scroll(x, y, delta)?;
        return Ok((0, 0));
    }
    if state
        .as_ref()
        .is_some_and(|state| state.closed.load(std::sync::atomic::Ordering::SeqCst))
    {
        *state = None;
    }
    if state.is_none() {
        let portal = RemoteDesktop::new()
            .await
            .map_err(|error| error.to_string())?;
        let session = std::sync::Arc::new(
            portal
                .create_session(Default::default())
                .await
                .map_err(|error| error.to_string())?,
        );
        let result = async {
            portal
                .select_devices(
                    &session,
                    SelectDevicesOptions::default().set_devices(Some(DeviceType::Keyboard.into())),
                )
                .await?
                .response()?;
            let response = portal
                .start(&session, None, Default::default())
                .await?
                .response()?;
            if !response.devices().contains(DeviceType::Keyboard) {
                return Err(ashpd::Error::NoResponse);
            }
            let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let signal = closed.clone();
            let watched_session = session.clone();
            let (ready, subscription) = tokio::sync::oneshot::channel();
            let watcher = tauri::async_runtime::spawn(async move {
                use futures_util::StreamExt;
                match watched_session.receive_closed().await {
                    Ok(mut events) => {
                        let _ = ready.send(Ok(()));
                        let _ = events.next().await;
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
                signal.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            if let Err(error) = subscription.await.unwrap_or(Err(ashpd::Error::NoResponse)) {
                watcher.abort();
                return Err(error);
            }
            Ok::<_, ashpd::Error>((closed, watcher))
        }
        .await;
        let (closed, watcher) = match result {
            Ok(value) => value,
            Err(error) => {
                let _ = session.close().await;
                return Err(format!("复制所选文字需要键盘输入授权：{error}"));
            }
        };
        *state = Some(PortalInput {
            portal,
            session,
            closed,
            watcher,
        });
    }
    let state = state.as_ref().unwrap();
    match action {
        Input::Copy => {
            let mut result = Ok(());
            for (code, key_state) in [
                (29, KeyState::Pressed),
                (46, KeyState::Pressed),
                (46, KeyState::Released),
                (29, KeyState::Released),
            ] {
                let sent = state
                    .portal
                    .notify_keyboard_keycode(&state.session, code, key_state, Default::default())
                    .await
                    .map_err(|error| error.to_string());
                if result.is_ok() {
                    result = sent;
                }
            }
            result?;
        }
        Input::Scroll(_, _, _) => {}
        Input::Cursor => {}
    }
    Ok((0, 0))
}

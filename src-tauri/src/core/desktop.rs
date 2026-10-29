use super::error::ManagerError;
use clipboard_rs::{Clipboard, ClipboardContext};
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
    clipboard(|context| context.get_files()).map(|files| {
        files.into_iter().filter_map(|file| {
            let path = if file.starts_with("file:") {
                url::Url::parse(&file).ok()?.to_file_path().ok()?
            } else {
                std::path::PathBuf::from(file)
            };
            path.is_file().then(|| path.to_string_lossy().into_owned())
        }).take(100).collect()
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
                            let modifier = if cfg!(target_os = "macos") { Key::Meta } else { Key::Control };
                            enigo.key(modifier, Direction::Press).map_err(|error| error.to_string())?;
                            let result = enigo.key(Key::Unicode('c'), Direction::Click);
                            let release = enigo.key(modifier, Direction::Release);
                            result.and(release).map_err(|error| error.to_string())?;
                        }
                        Input::Scroll(x, y, delta) => {
                            enigo.move_mouse(x, y, Coordinate::Abs).map_err(|error| error.to_string())?;
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
    if is_wayland() { return Err(failure("当前桌面不提供全局光标坐标")); }
    input(Input::Cursor)
}

pub fn physical_cursor_position(app: &AppHandle) -> Result<(i32, i32), ManagerError> {
    let (x, y) = cursor_position()?;
    if cfg!(target_os = "macos") {
        let screen = xcap::Monitor::from_point(x, y).map_err(failure)?;
        if let Some(monitor) = app.available_monitors().map_err(failure)?
            .into_iter().find(|monitor| monitor.name() == screen.name().ok().as_ref()) {
            return Ok((
                monitor.position().x + ((x - screen.x().map_err(failure)?) as f64 * monitor.scale_factor()).round() as i32,
                monitor.position().y + ((y - screen.y().map_err(failure)?) as f64 * monitor.scale_factor()).round() as i32,
            ));
        }
    }
    Ok((x, y))
}

pub fn scroll_at(x: i32, y: i32, delta: i32) -> Result<(), ManagerError> {
    if delta != 0 { input(Input::Scroll(x, y, delta))?; }
    Ok(())
}

pub fn selected_text() -> Result<String, ManagerError> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"System Events\" to tell first application process whose frontmost is true to get value of attribute \"AXSelectedText\" of (value of attribute \"AXFocusedUIElement\")"])
            .output()?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string();
            if !text.trim().is_empty() { return Ok(text); }
        }
    }
    #[cfg(target_os = "linux")]
    {
        // PRIMARY 是选区而非普通剪贴板，读取它不会破坏用户之前复制的内容。
        let mut command = std::process::Command::new(if is_wayland() { "wl-paste" } else { "xclip" });
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
    let before = clipboard(|context| context.get(&[
        clipboard_rs::ContentFormat::Text, clipboard_rs::ContentFormat::Rtf,
        clipboard_rs::ContentFormat::Html, clipboard_rs::ContentFormat::Image,
        clipboard_rs::ContentFormat::Files,
    ]))?;
    clipboard(|context| context.clear())?;
    let result = (|| {
        input(Input::Copy)?;
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Ok(text) = clipboard(|context| context.get_text()) {
                if !text.is_empty() { return Ok(text); }
            }
        }
        Err(failure("未能复制所选文字，请确认目标应用允许复制或使用剪贴板翻译快捷键"))
    })();
    // 仅在剪贴板仍为本次读取内容时恢复，避免覆盖用户期间主动复制的内容。
    let current = clipboard(|context| context.get_text()).unwrap_or_default();
    if current.is_empty() || result.as_ref().is_ok_and(|text| text == &current) {
        let _ = clipboard(|context| context.set(before));
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
                if let Some(task) = portal_task.take() { task.abort(); }
                let mut errors = Vec::new();
                #[cfg(target_os = "linux")]
                if is_wayland() && !next.is_empty() {
                    let handle = app.clone();
                    let shortcuts = next.clone();
                    portal_task = Some(tauri::async_runtime::spawn(async move {
                        if let Err(error) = portal_shortcuts(&handle, &shortcuts, activate).await {
                            report(&handle, vec![format!("无法注册桌面门户快捷键：{error}")]);
                        }
                    }));
                }
                if !is_wayland() {
                    for (action, shortcut) in &next {
                        let shortcut = shortcut.replace("Win+", "Super+");
                        let action = action.clone();
                        match app.global_shortcut().on_shortcut(shortcut.as_str(), move |app, _, event| {
                            if event.state == ShortcutState::Pressed { activate(app, &action); }
                        }) {
                            Ok(()) => registered.push(shortcut),
                            Err(error) => errors.push(format!("快捷键 {shortcut} 注册失败：{error}")),
                        }
                    }
                }
                report(&app, errors);
                previous = next;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    });
}

#[cfg(target_os = "linux")]
async fn portal_shortcuts(
    app: &AppHandle,
    bindings: &[(String, String)],
    activate: fn(&AppHandle, &str),
) -> Result<(), ashpd::Error> {
    use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
    use futures_util::StreamExt;
    let portal = GlobalShortcuts::new().await?;
    struct SessionGuard(Option<ashpd::desktop::Session<GlobalShortcuts>>);
    impl Drop for SessionGuard {
        fn drop(&mut self) {
            if let Some(session) = self.0.take() {
                tauri::async_runtime::spawn(async move { let _ = session.close().await; });
            }
        }
    }
    let guard = SessionGuard(Some(portal.create_session(Default::default()).await?));
    let session = guard.0.as_ref().unwrap();
    let session_path = serde_json::to_value(session).ok()
        .and_then(|value| value.as_str().map(str::to_string)).unwrap_or_default();
    let shortcuts = bindings.iter().map(|(action, trigger)| {
        let trigger = trigger.replace("Ctrl+", "CTRL+").replace("Alt+", "ALT+")
            .replace("Shift+", "SHIFT+").replace("Win+", "LOGO+")
            .replace("PrintScreen", "Print");
        NewShortcut::new(action, action).preferred_trigger(Some(trigger.as_str()))
    }).collect::<Vec<_>>();
    let mut events = portal.receive_activated().await?;
    portal.bind_shortcuts(&session, &shortcuts, None, Default::default()).await?.response()?;
    while let Some(event) = events.next().await {
        if event.session_handle().as_str() == session_path { activate(app, event.shortcut_id()); }
    }
    session.close().await?;
    Ok(())
}

#[cfg(target_os = "linux")]
struct PortalInput {
    portal: ashpd::desktop::remote_desktop::RemoteDesktop,
    session: ashpd::desktop::Session<ashpd::desktop::remote_desktop::RemoteDesktop>,
    streams: Vec<ashpd::desktop::screencast::Stream>,
}

#[cfg(target_os = "linux")]
async fn wayland_input(
    state: &mut Option<PortalInput>,
    action: Input,
) -> Result<(i32, i32), ashpd::Error> {
    use ashpd::desktop::{
        remote_desktop::{DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions},
        screencast::{Screencast, SelectSourcesOptions, SourceType},
    };
    if state.is_none() {
        let portal = RemoteDesktop::new().await?;
        let session = portal.create_session(Default::default()).await?;
        portal.select_devices(&session, SelectDevicesOptions::default()
            .set_devices(DeviceType::Keyboard | DeviceType::Pointer)).await?.response()?;
        let screencast = Screencast::new().await?;
        screencast.select_sources(&session, SelectSourcesOptions::default()
            .set_sources(SourceType::Monitor).set_multiple(true)).await?.response()?;
        let response = portal.start(&session, None, Default::default()).await?.response()?;
        *state = Some(PortalInput { portal, session, streams: response.streams().to_vec() });
    }
    let state = state.as_ref().unwrap();
    match action {
        Input::Copy => {
            for (code, key_state) in [(29, KeyState::Pressed), (46, KeyState::Pressed),
                (46, KeyState::Released), (29, KeyState::Released)] {
                state.portal.notify_keyboard_keycode(&state.session, code, key_state, Default::default()).await?;
            }
        }
        Input::Scroll(x, y, delta) => {
            for stream in &state.streams {
                if let (Some((sx, sy)), Some((width, height))) = (stream.position(), stream.size()) {
                    if x >= sx && y >= sy && x < sx + width && y < sy + height {
                        state.portal.notify_pointer_motion_absolute(&state.session, stream.pipe_wire_node_id(),
                            (x - sx) as f64, (y - sy) as f64, Default::default()).await?;
                        break;
                    }
                }
            }
            state.portal.notify_pointer_axis_discrete(&state.session,
                ashpd::desktop::remote_desktop::Axis::Vertical, -delta.signum() * 3, Default::default()).await?;
        }
        Input::Cursor => {}
    }
    Ok((0, 0))
}

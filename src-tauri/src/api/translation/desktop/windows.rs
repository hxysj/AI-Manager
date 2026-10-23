use super::{show_selection, DesktopTranslator};
use std::{ptr::null_mut, sync::mpsc, time::Duration};
use tauri::{AppHandle, Emitter};
use ::windows::{
    core::Interface,
    Win32::{
        System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED},
        UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationTextPattern, UIA_TextPatternId},
    },
};
use windows_sys::Win32::{
    Foundation::POINT,
    System::{DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard}, Memory::{GlobalLock, GlobalSize, GlobalUnlock}},
    UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN},
        WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY},
    },
};

fn parse_shortcut(value: &str) -> Result<(u32, u32), String> {
    let mut modifiers = 0;
    let mut key = None;
    for part in value.split('+') {
        let part = part.trim().to_ascii_uppercase();
        let modifier = match part.as_str() {
            "ALT" => MOD_ALT,
            "CTRL" | "CONTROL" => MOD_CONTROL,
            "SHIFT" => MOD_SHIFT,
            "WIN" | "META" => MOD_WIN,
            _ => 0,
        };
        if modifier != 0 {
            if modifiers & modifier != 0 { return Err("快捷键包含重复修饰键".into()); }
            modifiers |= modifier;
            continue;
        }
        let code = match part.as_str() {
            "PRINTSCREEN" => 0x2c,
            "SPACE" => 0x20,
            val if val.len() == 1 && val.as_bytes()[0].is_ascii_alphanumeric() => u32::from(val.as_bytes()[0]),
            val if val.starts_with('F') => val[1..].parse::<u32>().ok().filter(|number| (1..=24).contains(number)).map(|number| 0x70 + number - 1).ok_or_else(|| "不支持的功能键".to_string())?,
            _ => return Err("快捷键请使用字母、数字、F1–F24、PrintScreen 或 Space".into()),
        };
        if key.replace(code).is_some() { return Err("每组快捷键只能包含一个主键".into()); }
    }
    let key = key.ok_or_else(|| "快捷键缺少主键".to_string())?;
    if modifiers == 0 && !(0x70..=0x87).contains(&key) && key != 0x2c {
        return Err("字母、数字和空格快捷键至少需要一个修饰键".into());
    }
    Ok((modifiers, key))
}

pub(super) fn start(app: AppHandle, service: DesktopTranslator) -> mpsc::SyncSender<()> {
    let (wake, changes) = mpsc::sync_channel(1);
    let errors = service.clone();
    if let Err(error) = std::thread::Builder::new().name("桌面划词翻译".into()).spawn(move || {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        let mut automation: Option<IUIAutomation> = None;
        let mut enabled = false;
        let mut registered = Vec::new();
        let mut current_signature = String::new();
        loop {
            let (next_enabled, shortcut, clipboard_shortcut) = {
                let state = service.inner.lock().unwrap();
                (state.enabled, state.shortcut.clone(), state.clipboard_shortcut.clone())
            };
            let next_signature = if next_enabled {
                format!("1:{shortcut}:{clipboard_shortcut}")
            } else {
                "0".to_string()
            };
            if next_signature != current_signature {
                for id in registered.drain(..) { unsafe { UnregisterHotKey(null_mut(), id); } }
                current_signature = next_signature;
                enabled = next_enabled;
                let mut errors = Vec::new();
                if enabled {
                    let shortcuts = [
                        (0x4d54, shortcut.as_str(), "划词翻译"),
                        (0x4d55, clipboard_shortcut.as_str(), "剪贴板翻译"),
                    ];
                    for (id, sc, label) in shortcuts {
                        match parse_shortcut(sc) {
                            Ok((modifiers, code)) => {
                                if unsafe { RegisterHotKey(null_mut(), id, modifiers | MOD_NOREPEAT, code) } != 0 {
                                    registered.push(id);
                                } else {
                                    errors.push(format!("{label}快捷键 {sc} 注册失败，可能已被其他应用占用"));
                                }
                            }
                            Err(err) => {
                                errors.push(format!("{label}快捷键格式错误：{err}"));
                            }
                        }
                    }
                } else {
                    // 停用后释放 COM 接口和快捷键，线程阻塞等待配置变更。
                    automation = None;
                }
                service.inner.lock().unwrap().hotkey_error = errors.join("；");
                let _ = app.emit("translation:desktop-status", ());
            }
            if !enabled {
                if changes.recv().is_err() { break; }
                continue;
            }
            // 只泵送快捷键消息，不轮询选区、剪贴板或屏幕内容。
            match changes.recv_timeout(Duration::from_millis(50)) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
                if message.message != WM_HOTKEY || !registered.contains(&(message.wParam as i32)) {
                    continue;
                }
                if !service.inner.lock().unwrap().enabled { break; }
                let mut cursor = POINT { x: 0, y: 0 };
                unsafe { GetCursorPos(&mut cursor); }
                let text = if message.wParam == 0x4d55 {
                    // 只有显式按剪贴板快捷键才读取文本，绝不自动模拟复制或覆盖原剪贴板。
                    clipboard_text()
                } else if initialized {
                    if automation.is_none() {
                        automation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };
                        if let Some(automation) = &automation {
                            if let Ok(options) = automation.cast::<IUIAutomation2>() {
                                unsafe {
                                    let _ = options.SetConnectionTimeout(1000);
                                    let _ = options.SetTransactionTimeout(1000);
                                }
                            }
                        }
                    }
                    automation.as_ref().ok_or_else(|| "无法初始化 Windows 取词接口".to_string())
                        .and_then(selection_text)
                } else {
                    let csc = service.inner.lock().unwrap().clipboard_shortcut.clone();
                    let csc = if csc.is_empty() { "Ctrl+Alt+Shift+T".to_string() } else { csc };
                    Err(format!("Windows 取词接口初始化失败，请复制文字后按 {csc}"))
                };
                show_selection(&app, text, cursor.x, cursor.y);
            }
        }
        for id in registered { unsafe { UnregisterHotKey(null_mut(), id); } }
        drop(automation);
        if initialized { unsafe { CoUninitialize(); } }
    }) {
        errors.inner.lock().unwrap().hotkey_error = format!("无法启动桌面翻译：{error}");
    }
    wake
}

fn selection_text(automation: &IUIAutomation) -> Result<String, String> {
    let unavailable = || "此应用未提供可读取的选区。请复制文字后按 Ctrl+Alt+Shift+T 翻译剪贴板。".to_string();
    unsafe {
        let foreground = GetForegroundWindow();
        let mut process = 0;
        GetWindowThreadProcessId(foreground, &mut process);
        let mut element = automation.GetFocusedElement().map_err(|_| unavailable())?;
        if element.CurrentIsPassword().map_err(|_| unavailable())?.as_bool() {
            return Err("密码输入框不支持划词翻译".into());
        }
        let walker = automation.ControlViewWalker().map_err(|_| unavailable())?;
        // 浏览器和编辑器的焦点可能在文本子节点，仅向同一进程的父控件寻找选区。
        for _ in 0..8 {
            if element.CurrentProcessId().unwrap_or(0) != process as i32 { break; }
            if let Ok(pattern) = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                if let Ok(ranges) = pattern.GetSelection() {
                    let count = ranges.Length().map_err(|_| unavailable())?;
                    if count > 32 { return Err("选区过多，请缩小选择范围".into()); }
                    let mut parts = Vec::new();
                    let mut characters = 0;
                    for index in 0..count {
                        let text = ranges.GetElement(index).and_then(|range| range.GetText(60_002)).map_err(|_| unavailable())?.to_string();
                        characters += text.chars().count();
                        if characters > 30_000 { return Err("单次最多翻译 30000 个字符，请缩小选择范围".into()); }
                        if !text.trim().is_empty() { parts.push(text); }
                    }
                    if !parts.is_empty() {
                        if GetForegroundWindow() != foreground {
                            return Err("取词期间切换了窗口，请重新选中文字并按快捷键".into());
                        }
                        return Ok(parts.join("\n"));
                    }
                }
            }
            element = match walker.GetParentElement(&element) {
                Ok(parent) => parent,
                Err(_) => break,
            };
        }
    }
    Err(unavailable())
}

fn clipboard_text() -> Result<String, String> {
    struct Clipboard;
    impl Drop for Clipboard {
        fn drop(&mut self) { unsafe { CloseClipboard(); } }
    }
    unsafe {
        if OpenClipboard(null_mut()) == 0 { return Err("剪贴板正被其他应用占用，请重试".into()); }
        let _guard = Clipboard;
        // CF_UNICODETEXT = 13；读取长度有界，图片和其他格式不进入内存。
        let memory = GetClipboardData(13);
        if memory.is_null() { return Err("剪贴板没有文本，请先复制要翻译的文字".into()); }
        let length = (GlobalSize(memory) / 2).min(60_002);
        let pointer = GlobalLock(memory).cast::<u16>();
        if pointer.is_null() { return Err("无法读取剪贴板文本".into()); }
        let units = std::slice::from_raw_parts(pointer, length);
        let result = match units.iter().position(|unit| *unit == 0) {
            Some(end) => Ok(String::from_utf16_lossy(&units[..end])),
            None => Err("单次最多翻译 30000 个字符，请缩小复制范围".into()),
        };
        GlobalUnlock(memory);
        result
    }
}

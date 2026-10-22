use super::{decode_bytes, failure, ManagerError, ScreenshotService};
use image::{DynamicImage, RgbaImage};
use serde::Serialize;
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows_sys::{core::BOOL, Win32::{
    Foundation::{GlobalFree, HGLOBAL, HWND, LPARAM, POINT, RECT},
    Graphics::{Dwm::{DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_TRANSITIONS_FORCEDISABLED}, Gdi::*},
    System::{DataExchange::*, Memory::*},
    UI::{Input::KeyboardAndMouse::*, Shell::DragQueryFileW, WindowsAndMessaging::*},
}};

pub fn disable_window_transitions(window: &WebviewWindow) -> Result<(), ManagerError> {
    let hwnd = window.hwnd().map_err(failure)?;
    let disabled = 1_i32;
    // 仅关闭贴图窗口自身的系统过渡动画，不修改用户的 Windows 设置。
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd.0 as _,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            std::ptr::from_ref(&disabled).cast(),
            size_of::<i32>() as u32,
        )
    };
    if result < 0 {
        return Err(failure(format!("关闭贴图窗口过渡动画失败：{result:#x}")));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub kind: String,
    pub label: String,
}

struct SmartQuery {
    x: i32,
    y: i32,
    skip: isize,
    result: Vec<SmartRegion>,
}

unsafe extern "system" fn collect_smart_window(hwnd: HWND, param: LPARAM) -> BOOL {
    let query = &mut *(param as *mut SmartQuery);
    if hwnd as isize == query.skip || IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if ex_style & WS_EX_TOOLWINDOW != 0 || ex_style & WS_EX_TRANSPARENT != 0 {
        return 1;
    }
    let mut cloaked = 0_u32;
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED as u32,
        (&mut cloaked as *mut u32).cast(),
        size_of::<u32>() as u32,
    ) >= 0
        && cloaked != 0
    {
        return 1;
    }
    let mut title = [0_u16; 512];
    let title_len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
    if title_len <= 0 {
        return 1;
    }
    let title = String::from_utf16_lossy(&title[..title_len as usize]);
    let mut rect: RECT = zeroed();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS as u32,
        (&mut rect as *mut RECT).cast(),
        size_of::<RECT>() as u32,
    ) < 0
    {
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 30 || height < 30 || query.x < rect.left || query.x >= rect.right || query.y < rect.top || query.y >= rect.bottom {
        return 1;
    }
    let mut class_name = [0_u16; 256];
    let class_len = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
    let class_name = String::from_utf16_lossy(&class_name[..class_len.max(0) as usize]);
    if matches!(class_name.as_str(), "WorkerW" | "Progman" | "Windows.UI.Core.CoreWindow") {
        return 1;
    }
    query.result.push(SmartRegion {
        x: rect.left,
        y: rect.top,
        width: width as u32,
        height: height as u32,
        kind: "window".into(),
        label: title,
    });
    0
}

pub fn smart_selection(app: &AppHandle, x: i32, y: i32) -> Result<Vec<SmartRegion>, ManagerError> {
    unsafe {
        let skip = app
            .get_webview_window("screenshot-capture")
            .and_then(|window| window.hwnd().ok())
            .map(|hwnd| hwnd.0 as isize)
            .unwrap_or(0);
        let mut query = SmartQuery { x, y, skip, result: Vec::new() };
        EnumWindows(Some(collect_smart_window), (&mut query as *mut SmartQuery) as isize);
        let point = POINT { x, y };
        let mut monitor_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if !monitor.is_null() {
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                monitor_rect = info.rcMonitor;
            }
        }
        if monitor_rect.right > monitor_rect.left && monitor_rect.bottom > monitor_rect.top {
            query.result.push(SmartRegion {
                x: monitor_rect.left,
                y: monitor_rect.top,
                width: (monitor_rect.right - monitor_rect.left) as u32,
                height: (monitor_rect.bottom - monitor_rect.top) as u32,
                kind: "screen".into(),
                label: "当前显示器".into(),
            });
        }
        Ok(query.result)
    }
}

// 所有 GDI 所有权都在此处释放，剪贴板借用的位图不进入此包装。
struct CaptureDc {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl Drop for CaptureDc {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.memory.is_null() {
                DeleteDC(self.memory);
            }
            if !self.screen.is_null() {
                ReleaseDC(null_mut(), self.screen);
            }
        }
    }
}

pub fn capture(
    monitors: &[tauri::Monitor],
    left: i32,
    top: i32,
    width: u32,
    height: u32,
) -> Result<DynamicImage, ManagerError> {
    let mut output = RgbaImage::new(width, height);
    for monitor in monitors {
        unsafe {
            let (w, h) = (monitor.size().width, monitor.size().height);
            let mut dc = CaptureDc {
                screen: GetDC(null_mut()),
                memory: null_mut(),
                bitmap: null_mut(),
                previous: null_mut(),
            };
            if dc.screen.is_null() {
                return Err(failure("无法访问桌面，请确认会话未锁屏"));
            }
            dc.memory = CreateCompatibleDC(dc.screen);
            dc.bitmap = CreateCompatibleBitmap(dc.screen, w as i32, h as i32);
            if dc.memory.is_null() || dc.bitmap.is_null() {
                return Err(failure("创建截图缓冲区失败"));
            }
            dc.previous = SelectObject(dc.memory, dc.bitmap);
            if dc.previous.is_null() || dc.previous == -1_isize as HGDIOBJ {
                dc.previous = null_mut();
                return Err(failure("选择截图缓冲区失败"));
            }
            if BitBlt(
                dc.memory,
                0,
                0,
                w as i32,
                h as i32,
                dc.screen,
                monitor.position().x,
                monitor.position().y,
                SRCCOPY | CAPTUREBLT,
            ) == 0
            {
                return Err(failure("桌面捕获失败"));
            }
            SelectObject(dc.memory, dc.previous);
            dc.previous = null_mut();
            let tile = bitmap_image(dc.screen, dc.bitmap, w, h)?;
            image::imageops::replace(
                &mut output,
                &tile,
                i64::from(monitor.position().x - left),
                i64::from(monitor.position().y - top),
            );
        }
    }
    Ok(DynamicImage::ImageRgba8(output))
}

unsafe fn bitmap_image(
    dc: HDC,
    bitmap: HBITMAP,
    width: u32,
    height: u32,
) -> Result<RgbaImage, ManagerError> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 40_000_000 {
        return Err(failure("剪贴板图片尺寸无效或过大"));
    }
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width as i32;
    info.bmiHeader.biHeight = -(height as i32);
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bytes = vec![0_u8; width as usize * height as usize * 4];
    if GetDIBits(
        dc,
        bitmap,
        0,
        height,
        bytes.as_mut_ptr().cast(),
        &mut info,
        DIB_RGB_COLORS,
    ) != height as i32
    {
        return Err(failure("读取位图像素失败"));
    }
    for pixel in bytes.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    RgbaImage::from_raw(width, height, bytes).ok_or_else(|| failure("位图数据无效"))
}

struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}

unsafe fn set_bytes(format: u32, bytes: &[u8]) -> Result<(), ManagerError> {
    let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
    if memory.is_null() {
        return Err(failure("分配剪贴板内存失败"));
    }
    let pointer = GlobalLock(memory);
    if pointer.is_null() {
        GlobalFree(memory);
        return Err(failure("访问剪贴板内存失败"));
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
    GlobalUnlock(memory);
    if SetClipboardData(format, memory).is_null() {
        GlobalFree(memory);
        return Err(failure("写入剪贴板失败"));
    }
    Ok(())
}

pub fn copy_image(app: &AppHandle, image: &DynamicImage) -> Result<(), ManagerError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let png = super::image_data(image)?;
    let png = STANDARD
        .decode(png.split_once(',').unwrap().1)
        .map_err(failure)?;
    let rgba = image.to_rgba8();
    let mut header: BITMAPV5HEADER = unsafe { zeroed() };
    header.bV5Size = size_of::<BITMAPV5HEADER>() as u32;
    header.bV5Width = rgba.width() as i32;
    header.bV5Height = -(rgba.height() as i32);
    header.bV5Planes = 1;
    header.bV5BitCount = 32;
    header.bV5Compression = BI_BITFIELDS;
    header.bV5RedMask = 0x00ff0000;
    header.bV5GreenMask = 0x0000ff00;
    header.bV5BlueMask = 0x000000ff;
    header.bV5AlphaMask = 0xff000000;
    header.bV5CSType = 0x73524742;
    let mut dib = unsafe {
        std::slice::from_raw_parts(
            (&header as *const BITMAPV5HEADER).cast::<u8>(),
            size_of::<BITMAPV5HEADER>(),
        )
        .to_vec()
    };
    for p in rgba.pixels() {
        dib.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    let window = app
        .get_webview_window("screenshot-capture")
        .or_else(|| app.get_webview_window("main"))
        .or_else(|| app.webview_windows().into_values().next())
        .ok_or_else(|| failure("没有可用于剪贴板的窗口"))?;
    let hwnd = window.hwnd().map_err(failure)?;
    unsafe {
        if OpenClipboard(hwnd.0 as _) == 0 {
            return Err(failure("剪贴板正被其他程序占用，请重试"));
        }
        let _clipboard = Clipboard;
        if EmptyClipboard() == 0 {
            return Err(failure("清空剪贴板失败"));
        }
        let png_name: Vec<u16> = "PNG\0".encode_utf16().collect();
        let png_format = RegisterClipboardFormatW(png_name.as_ptr());
        if png_format == 0 {
            return Err(failure("注册 PNG 剪贴板格式失败"));
        }
        set_bytes(png_format, &png)?;
        set_bytes(17, &dib)?;
    }
    Ok(())
}

pub fn read_clipboard() -> Result<Vec<DynamicImage>, ManagerError> {
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return Err(failure("剪贴板正被其他程序占用，请重试"));
        }
        let _clipboard = Clipboard;
        let png_name: Vec<u16> = "PNG\0".encode_utf16().collect();
        let png_format = RegisterClipboardFormatW(png_name.as_ptr());
        if png_format != 0 && IsClipboardFormatAvailable(png_format) != 0 {
            let memory = GetClipboardData(png_format) as HGLOBAL;
            let length = GlobalSize(memory);
            if length == 0 || length > 96 * 1024 * 1024 {
                return Err(failure("剪贴板 PNG 大小无效"));
            }
            let pointer = GlobalLock(memory);
            if pointer.is_null() {
                return Err(failure("无法读取剪贴板图片"));
            }
            let result = decode_bytes(std::slice::from_raw_parts(pointer.cast(), length));
            GlobalUnlock(memory);
            return Ok(vec![result?]);
        }
        if IsClipboardFormatAvailable(2) != 0 {
            let bitmap = GetClipboardData(2) as HBITMAP;
            let mut info: BITMAP = zeroed();
            if GetObjectW(
                bitmap,
                size_of::<BITMAP>() as i32,
                (&mut info as *mut BITMAP).cast(),
            ) == 0
            {
                return Err(failure("读取剪贴板位图信息失败"));
            }
            let dc = GetDC(null_mut());
            if dc.is_null() {
                return Err(failure("无法读取剪贴板位图"));
            }
            let result = bitmap_image(
                dc,
                bitmap,
                info.bmWidth as u32,
                info.bmHeight.unsigned_abs(),
            );
            ReleaseDC(null_mut(), dc);
            return Ok(vec![DynamicImage::ImageRgba8(result?)]);
        }
        if IsClipboardFormatAvailable(15) != 0 {
            let files = GetClipboardData(15) as _;
            let count = DragQueryFileW(files, u32::MAX, null_mut(), 0);
            if count > 20 {
                return Err(failure("一次最多贴入 20 张本地图片"));
            }
            let mut paths = Vec::new();
            for index in 0..count {
                let length = DragQueryFileW(files, index, null_mut(), 0);
                let mut path = vec![0_u16; length as usize + 1];
                DragQueryFileW(files, index, path.as_mut_ptr(), path.len() as u32);
                paths.push(String::from_utf16_lossy(&path[..length as usize]));
            }
            // 在磁盘解码前释放系统剪贴板锁。
            drop(_clipboard);
            let mut images = Vec::new();
            let mut errors = Vec::new();
            for path in paths {
                let result = (|| {
                    if std::fs::metadata(&path)?.len() > 96 * 1024 * 1024 {
                        return Err(failure("文件过大"));
                    }
                    decode_bytes(&std::fs::read(&path)?)
                })();
                match result {
                    Ok(image) => images.push(image),
                    Err(error) => errors.push(format!("{path}: {error}")),
                }
            }
            // 混合内容逐项报告，避免把未导入文件误标为成功。
            if !errors.is_empty() {
                return Err(failure(format!(
                    "本次未导入，请仅复制图片文件：{}",
                    errors.join("；")
                )));
            }
            return Ok(images);
        }
        Err(failure("剪贴板中没有图片或本地图片文件"))
    }
}

pub fn start_hotkeys(app: AppHandle) {
    std::thread::spawn(move || {
        let mut previous = String::new();
        let mut registered = Vec::new();
        loop {
            let service = app.state::<ScreenshotService>();
            let settings = service.settings();
            let enabled = settings.hotkeys && !service.hotkeys_paused.load(std::sync::atomic::Ordering::SeqCst);
            let bindings = settings.shortcut_bindings();
            let key = format!("{enabled}:{bindings:?}");
            if key != previous {
                for id in registered.drain(..) {
                    unsafe {
                        UnregisterHotKey(null_mut(), id);
                    }
                }
                let mut errors = Vec::new();
                if enabled {
                    for (id, name, shortcut) in bindings {
                        let (modifiers, code) = match super::parse_shortcut(&shortcut) {
                            Ok(binding) => binding,
                            Err(error) => { errors.push(error.to_string()); continue; }
                        };
                        if unsafe { RegisterHotKey(null_mut(), id, modifiers | MOD_NOREPEAT, code) } != 0
                        {
                            registered.push(id);
                        } else {
                            errors.push(format!(
                                "{name} 快捷键 {shortcut} 注册失败，可能已被占用；请在截图设置中更换"
                            ));
                        }
                    }
                }
                *service.hotkey_errors.lock().unwrap() = errors;
                previous = key;
                super::changed(&app);
            }
            drop(service);
            unsafe {
                let mut message: MSG = zeroed();
                while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if message.message != WM_HOTKEY {
                        continue;
                    }
                    let app = app.clone();
                    let action = message.wParam;
                    tauri::async_runtime::spawn(async move {
                        let result = match action {
                            1 => super::begin_capture(&app, true).await,
                            2 => super::begin_capture(&app, false).await,
                            3 => super::paste_clipboard(&app),
                            4 => super::toggle_pins(&app),
                            5 => super::switch_pin_group(&app),
                            6 => super::reset_input(&app),
                            _ => Ok(()),
                        };
                        if let Err(error) = result {
                            let _ = app.emit("screenshot:error", error.to_string());
                        }
                    });
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_bitmap_readback_preserves_pixel_position_and_rgb() {
        unsafe {
            let mut dc = CaptureDc { screen: GetDC(null_mut()), memory: null_mut(), bitmap: null_mut(), previous: null_mut() };
            assert!(!dc.screen.is_null());
            dc.memory = CreateCompatibleDC(dc.screen);
            dc.bitmap = CreateCompatibleBitmap(dc.screen, 3, 2);
            assert!(!dc.memory.is_null() && !dc.bitmap.is_null());
            dc.previous = SelectObject(dc.memory, dc.bitmap);
            assert!(!dc.previous.is_null());
            assert_ne!(SetPixelV(dc.memory, 0, 0, 0x0000ff), 0);
            assert_ne!(SetPixelV(dc.memory, 2, 1, 0x00ff00), 0);
            SelectObject(dc.memory, dc.previous);
            dc.previous = null_mut();
            let image = bitmap_image(dc.screen, dc.bitmap, 3, 2).unwrap();
            assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
            assert_eq!(image.get_pixel(2, 1).0, [0, 255, 0, 255]);
        }
    }
}

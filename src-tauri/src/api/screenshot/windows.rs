use super::{decode_bytes, failure, CaptureRect, ManagerError, ScreenshotService};
use image::{DynamicImage, RgbaImage};
use serde::Serialize;
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::{GlobalFree, HGLOBAL, HWND, LPARAM, POINT, RECT},
        Graphics::{
            Dwm::{
                DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CLOAKED,
                DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_TRANSITIONS_FORCEDISABLED,
            },
            Gdi::*,
        },
        System::{DataExchange::*, Memory::*},
        UI::{Input::KeyboardAndMouse::*, Shell::DragQueryFileW, WindowsAndMessaging::*},
    },
};

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
    target: isize,
    result: Vec<SmartRegion>,
}

// 控件识别放到独立线程，避免无障碍提供程序卡住截图界面。
mod block_detection {
    use super::SmartRegion;
    use ::windows::Win32::{
        Foundation::{HWND, RECT},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_MULTITHREADED,
        },
        UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        },
    };
    use std::{
        collections::VecDeque,
        sync::{
            mpsc::{self, SyncSender},
            OnceLock,
        },
        thread,
        time::Duration,
    };

    struct Request {
        x: i32,
        y: i32,
        target: isize,
        reply: SyncSender<Vec<SmartRegion>>,
    }

    fn contains(rect: &RECT, x: i32, y: i32) -> bool {
        rect.left <= x && x < rect.right && rect.top <= y && y < rect.bottom
    }

    fn same_rect(left: &RECT, right: &RECT) -> bool {
        left.left == right.left
            && left.top == right.top
            && left.right == right.right
            && left.bottom == right.bottom
    }

    fn element_rect(element: &IUIAutomationElement, x: i32, y: i32) -> Option<RECT> {
        if unsafe { element.CurrentIsOffscreen().ok() }.is_some_and(|value| value.as_bool()) {
            return None;
        }
        let rect = unsafe { element.CurrentBoundingRectangle().ok()? };
        if !contains(&rect, x, y) || rect.right - rect.left < 8 || rect.bottom - rect.top < 8 {
            return None;
        }
        Some(rect)
    }

    fn element_label(element: &IUIAutomationElement) -> String {
        unsafe {
            element
                .CurrentName()
                .ok()
                .map(|value| value.to_string())
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    element
                        .CurrentLocalizedControlType()
                        .ok()
                        .map(|value| value.to_string())
                        .filter(|value| !value.trim().is_empty())
                })
                .unwrap_or_else(|| "识别到的界面块".into())
        }
    }

    fn child_at(
        walker: &IUIAutomationTreeWalker,
        parent: &IUIAutomationElement,
        x: i32,
        y: i32,
    ) -> Option<IUIAutomationElement> {
        let mut child = unsafe { walker.GetFirstChildElement(parent).ok()? };
        let mut best = None;
        let mut best_area = i64::MAX;
        for _ in 0..512 {
            if let Some(rect) = element_rect(&child, x, y) {
                let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
                if area < best_area {
                    best_area = area;
                    best = Some(child.clone());
                }
            }
            child = match unsafe { walker.GetNextSiblingElement(&child) } {
                Ok(next) => next,
                Err(_) => break,
            };
        }
        best
    }

    fn automation_chain(
        automation: &IUIAutomation,
        target: isize,
        x: i32,
        y: i32,
    ) -> Vec<SmartRegion> {
        if target == 0 {
            return Vec::new();
        }
        let Ok(mut element) =
            (unsafe { automation.ElementFromHandle(HWND(target as *mut core::ffi::c_void)) })
        else {
            return Vec::new();
        };
        let Ok(walker) = (unsafe { automation.ControlViewWalker() }) else {
            return Vec::new();
        };
        let mut regions = Vec::new();
        let mut rectangles = Vec::new();
        for _ in 0..32 {
            if let Some(rect) = element_rect(&element, x, y) {
                if !rectangles.iter().any(|item| same_rect(item, &rect)) {
                    rectangles.push(rect);
                    regions.push(SmartRegion {
                        x: rect.left,
                        y: rect.top,
                        width: (rect.right - rect.left) as u32,
                        height: (rect.bottom - rect.top) as u32,
                        kind: "element".into(),
                        label: element_label(&element),
                    });
                }
            }
            let Some(child) = child_at(&walker, &element, x, y) else {
                break;
            };
            element = child;
        }
        regions.reverse();
        regions
    }

    fn query(
        x: i32,
        y: i32,
        target: isize,
        automation: &mut Option<IUIAutomation>,
    ) -> Vec<SmartRegion> {
        if automation.is_none() {
            *automation =
                unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };
        }
        automation
            .as_ref()
            .map(|automation| automation_chain(automation, target, x, y))
            .unwrap_or_default()
    }

    fn sender() -> SyncSender<Request> {
        static CHANNEL: OnceLock<SyncSender<Request>> = OnceLock::new();
        CHANNEL
            .get_or_init(|| {
                let (input, output) = mpsc::sync_channel::<Request>(1);
                thread::Builder::new()
                    .name("截图块识别".into())
                    .spawn(move || {
                        let initialized =
                            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
                        let mut automation = None;
                        let mut cache = VecDeque::<(i32, i32, isize, Vec<SmartRegion>)>::new();
                        while let Ok(request) = output.recv() {
                            if let Some((_, _, _, regions)) =
                                cache.iter().find(|(x, y, target, _)| {
                                    *x == request.x && *y == request.y && *target == request.target
                                })
                            {
                                let _ = request.reply.send(regions.clone());
                                continue;
                            }
                            let regions =
                                query(request.x, request.y, request.target, &mut automation);
                            cache.retain(|(x, y, target, _)| {
                                *x != request.x || *y != request.y || *target != request.target
                            });
                            cache.push_front((
                                request.x,
                                request.y,
                                request.target,
                                regions.clone(),
                            ));
                            cache.truncate(24);
                            let _ = request.reply.send(regions);
                        }
                        if initialized {
                            unsafe {
                                CoUninitialize();
                            }
                        }
                    })
                    .expect("无法启动截图块识别线程");
                input
            })
            .clone()
    }

    pub fn query_async(x: i32, y: i32, target: isize) -> Vec<SmartRegion> {
        let (reply, result) = mpsc::sync_channel(1);
        if sender()
            .try_send(Request {
                x,
                y,
                target,
                reply,
            })
            .is_err()
        {
            return Vec::new();
        }
        result
            .recv_timeout(Duration::from_millis(220))
            .unwrap_or_default()
    }
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
    if width < 30
        || height < 30
        || query.x < rect.left
        || query.x >= rect.right
        || query.y < rect.top
        || query.y >= rect.bottom
    {
        return 1;
    }
    let mut class_name = [0_u16; 256];
    let class_len = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
    let class_name = String::from_utf16_lossy(&class_name[..class_len.max(0) as usize]);
    if matches!(
        class_name.as_str(),
        "WorkerW" | "Progman" | "Windows.UI.Core.CoreWindow"
    ) {
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
    query.target = hwnd as isize;
    0
}

pub fn smart_selection(app: &AppHandle, x: i32, y: i32) -> Result<Vec<SmartRegion>, ManagerError> {
    unsafe {
        let skip = app
            .get_webview_window("screenshot-capture")
            .and_then(|window| window.hwnd().ok())
            .map(|hwnd| hwnd.0 as isize)
            .unwrap_or(0);
        let mut query = SmartQuery {
            x,
            y,
            skip,
            target: 0,
            result: Vec::new(),
        };
        EnumWindows(
            Some(collect_smart_window),
            (&mut query as *mut SmartQuery) as isize,
        );
        let target = query.target;
        let windows = query.result;
        let mut result = block_detection::query_async(x, y, target);
        // 无障碍树的根节点通常就是窗口本身，去掉重复矩形后再补回窗口层级。
        result.retain(|region| {
            !windows.iter().any(|window| {
                region.x == window.x
                    && region.y == window.y
                    && region.width == window.width
                    && region.height == window.height
            })
        });
        result.extend(windows);
        let point = POINT { x, y };
        let mut monitor_rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if !monitor.is_null() {
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                monitor_rect = info.rcMonitor;
            }
        }
        if monitor_rect.right > monitor_rect.left && monitor_rect.bottom > monitor_rect.top {
            result.push(SmartRegion {
                x: monitor_rect.left,
                y: monitor_rect.top,
                width: (monitor_rect.right - monitor_rect.left) as u32,
                height: (monitor_rect.bottom - monitor_rect.top) as u32,
                kind: "screen".into(),
                label: "当前显示器".into(),
            });
        }
        Ok(result)
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

pub(super) fn cursor_in_rect(_app: &AppHandle, rect: Option<&CaptureRect>) -> bool {
    let Some(rect) = rect else {
        return false;
    };
    let mut cursor = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut cursor) != 0 })
        && i64::from(cursor.x) >= i64::from(rect.x)
        && i64::from(cursor.y) >= i64::from(rect.y)
        && i64::from(cursor.x) < i64::from(rect.x) + i64::from(rect.width)
        && i64::from(cursor.y) < i64::from(rect.y) + i64::from(rect.height)
}

pub fn scroll_at(_app: &AppHandle, x: i32, y: i32, delta: i32) -> Result<(), ManagerError> {
    if delta == 0 {
        return Ok(());
    }
    unsafe {
        let mut previous = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut previous) == 0 {
            return Err(failure("无法读取长截图滚轮位置"));
        }
        // 覆盖层已穿透，激活选区下的应用，保证滚轮落到正在截图的页面。
        let target = WindowFromPoint(POINT { x, y });
        if !target.is_null() {
            let root = GetAncestor(target, GA_ROOT);
            let _ = SetForegroundWindow(if root.is_null() { target } else { root });
        }
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let move_to = |px: i32, py: i32| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    // 对准像素中心，避免每次恢复鼠标时向左上方偏移一个像素。
                    dx: (((i64::from(px) - i64::from(left)) * 65536 + 32768) / i64::from(width))
                        as i32,
                    dy: (((i64::from(py) - i64::from(top)) * 65536 + 32768) / i64::from(height))
                        as i32,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                    ..zeroed()
                },
            },
        };
        // 移入选区、滚动并恢复鼠标放在同一批输入中，用户仍可在预览区点击暂停或完成。
        let inputs = [
            move_to(x, y),
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        mouseData: delta as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        ..zeroed()
                    },
                },
            },
            move_to(previous.x, previous.y),
        ];
        if SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        ) != inputs.len() as u32
        {
            let _ = SetCursorPos(previous.x, previous.y);
            return Err(failure("无法发送长截图自动滚动输入"));
        }
    }
    Ok(())
}

pub fn start_hotkeys(app: AppHandle) {
    std::thread::spawn(move || {
        let mut previous = String::new();
        let mut registered = Vec::new();
        let mut escape_pressed = false;
        loop {
            let service = app.state::<ScreenshotService>();
            let settings = service.settings();
            // 鼠标穿透后焦点可能在被截应用，仍要让 Esc 完成长截图；不依赖全局快捷键开关。
            let escape_down = settings.enabled && unsafe { GetAsyncKeyState(VK_ESCAPE as i32) } < 0;
            if escape_down
                && !escape_pressed
                && service
                    .long_capturing
                    .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                if let Some(window) = app.get_webview_window("screenshot-capture") {
                    let _ = window.emit("screenshot:long-finish", ());
                }
            }
            escape_pressed = escape_down;
            if service
                .long_capturing
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                let preview = service.long_preview_rect.lock().unwrap();
                // 等锁期间可能已经结束截图，不能再把普通截图窗口改回穿透。
                if let Some(window) = app.get_webview_window("screenshot-capture").filter(|_| {
                    service
                        .long_capturing
                        .load(std::sync::atomic::Ordering::SeqCst)
                }) {
                    if let Ok(hwnd) = window.hwnd() {
                        let transparent = unsafe { GetWindowLongPtrW(hwnd.0 as _, GWL_EXSTYLE) }
                            as u32
                            & WS_EX_TRANSPARENT
                            != 0;
                        let interactive = cursor_in_rect(&app, preview.as_ref());
                        // 只有预览区接收点击，其他位置始终交给底层页面手动滚动。
                        if transparent == interactive {
                            let _ = window.set_ignore_cursor_events(!interactive);
                        }
                    }
                }
            }
            let enabled = settings.enabled
                && settings.hotkeys
                && !service
                    .hotkeys_paused
                    .load(std::sync::atomic::Ordering::SeqCst);
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
                            Err(error) => {
                                errors.push(error.to_string());
                                continue;
                            }
                        };
                        if unsafe { RegisterHotKey(null_mut(), id, modifiers | MOD_NOREPEAT, code) }
                            != 0
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
                        if !app.state::<ScreenshotService>().settings().enabled {
                            return;
                        }
                        let result = match action {
                            1 => super::begin_capture(&app, true, false).await,
                            2 => super::begin_capture(&app, false, false).await,
                            3 => super::paste_clipboard(&app),
                            4 => super::toggle_pins(&app),
                            5 => super::switch_pin_group(&app),
                            6 => super::reset_input(&app),
                            7 => super::begin_capture(&app, false, true).await,
                            _ => Ok(()),
                        };
                        if let Err(error) = result {
                            let _ = app.emit("screenshot:error", error.to_string());
                        }
                    });
                }
            }
            std::thread::sleep(Duration::from_millis(if settings.enabled {
                5
            } else {
                100
            }));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_bitmap_readback_preserves_pixel_position_and_rgb() {
        unsafe {
            let mut dc = CaptureDc {
                screen: GetDC(null_mut()),
                memory: null_mut(),
                bitmap: null_mut(),
                previous: null_mut(),
            };
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

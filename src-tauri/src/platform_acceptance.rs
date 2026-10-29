use serde_json::{json, Value};
use std::path::PathBuf;

pub fn run() -> Result<(), String> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let output = arguments
        .windows(2)
        .find(|pair| pair[0] == "--output")
        .map(|pair| PathBuf::from(&pair[1]))
        .ok_or("请通过 --output 指定验收记录目录")?;
    let allowed = ["--output", "--capture", "--clipboard", "--ports"];
    let mut index = 0;
    while index < arguments.len() {
        if !allowed.contains(&arguments[index].as_str()) {
            return Err(format!("未知验收参数：{}", arguments[index]));
        }
        index += if arguments[index] == "--output" { 2 } else { 1 };
    }
    std::fs::create_dir_all(&output).map_err(|error| error.to_string())?;
    if std::fs::read_dir(&output)
        .map_err(|error| error.to_string())?
        .next()
        .is_some()
    {
        return Err("请使用空的验收目录，避免覆盖已有证据".into());
    }
    let mut checks = Vec::<Value>::new();
    #[cfg(unix)]
    {
        use crate::core::desktop;
        use clipboard_rs::Clipboard;
        use sha2::{Digest, Sha256};
        let mut record = |name: &str, result: Result<Value, String>| {
            checks.push(match result {
                Ok(evidence) => json!({"id": name, "status": "passed", "evidence": evidence}),
                Err(error) => json!({"id": name, "status": "failed", "error": error}),
            });
        };
        if arguments.iter().any(|argument| argument == "--ports") {
            let result = (|| {
                let listener = std::net::TcpListener::bind("127.0.0.1:0")
                    .map_err(|error| error.to_string())?;
                let port = listener
                    .local_addr()
                    .map_err(|error| error.to_string())?
                    .port();
                tauri::async_runtime::block_on(crate::api::tools::list_ports())
                    .map_err(|error| error.to_string())
                    .and_then(|value| {
                        let found = value["ports"].as_array().is_some_and(|ports| {
                            ports.iter().any(|item| {
                                item["pid"] == std::process::id()
                                    && item["localPort"] == port
                                    && item["canTerminate"] == false
                                    && item["startedAt"].as_u64().unwrap_or(0) > 0
                            })
                        });
                        if !found {
                            return Err("端口列表未识别当前测试监听器或未保护当前进程".into());
                        }
                        Ok(json!({"listenerFound": true, "selfTerminationBlocked": true}))
                    })
            })();
            record("native-ports", result);
        }
        if arguments.iter().any(|argument| argument == "--clipboard") {
            // 普通运行不改剪贴板，显式验收时写入图片并恢复原内容；记录不保存用户剪贴板。
            let result = (|| -> Result<Value, String> {
                let before = if desktop::is_wayland() {
                    None
                } else {
                    Some(
                        desktop::clipboard(|context| {
                            context.get(&[
                                clipboard_rs::ContentFormat::Text,
                                clipboard_rs::ContentFormat::Rtf,
                                clipboard_rs::ContentFormat::Html,
                                clipboard_rs::ContentFormat::Image,
                                clipboard_rs::ContentFormat::Files,
                            ])
                        })
                        .map_err(|error| error.to_string())?,
                    )
                };
                // Wayland 的诊断只读，避免无法完整恢复多种 MIME 格式时改变用户数据。
                if desktop::is_wayland() {
                    let readable = desktop::clipboard_text().is_ok()
                        || desktop::clipboard_image().is_ok()
                        || desktop::clipboard_files().is_ok();
                    return if readable {
                        Ok(json!({"mode": "read-only", "readable": true}))
                    } else {
                        Err("无法读取剪贴板；请先复制文字、图片或文件后重试".into())
                    };
                }
                let expected = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                    8,
                    8,
                    image::Rgba([24, 96, 168, 255]),
                ));
                let result = desktop::copy_image(&expected)
                    .and_then(|_| desktop::clipboard_image())
                    .map_err(|error| error.to_string())
                    .and_then(|actual| {
                        if actual.into_rgba8() != expected.into_rgba8() {
                            return Err("图片剪贴板往返后的像素不同".into());
                        }
                        Ok(json!({"mode": "image-roundtrip", "pixelsMatch": true}))
                    });
                if let Some(before) = before {
                    desktop::clipboard(|context| context.set(before))
                        .map_err(|error| format!("剪贴板验收完成，但恢复原内容失败：{error}"))?;
                }
                result
            })();
            record("native-clipboard", result);
        }
        if arguments.iter().any(|argument| argument == "--capture") {
            let result = (|| -> Result<Value, String> {
                let mut evidence = Vec::new();
                for index in 0..3 {
                    #[cfg(target_os = "linux")]
                    let image = if desktop::is_wayland() {
                        crate::core::wayland::portal::capture()?
                    } else {
                        xcap::Monitor::all()
                            .map_err(|error| error.to_string())?
                            .first()
                            .ok_or("没有可采集的显示器")?
                            .capture_image()
                            .map_err(|error| error.to_string())?
                    };
                    #[cfg(target_os = "macos")]
                    let image = xcap::Monitor::all()
                        .map_err(|error| error.to_string())?
                        .first()
                        .ok_or("没有可采集的显示器")?
                        .capture_image()
                        .map_err(|error| error.to_string())?;
                    let file = format!("desktop-frame-{}.png", index + 1);
                    image
                        .save(output.join(&file))
                        .map_err(|error| error.to_string())?;
                    evidence.push(
                        json!({"file": file, "width": image.width(), "height": image.height(),
                        "sha256": format!("{:x}", Sha256::digest(std::fs::read(output.join(&file)).map_err(|error| error.to_string())?)),
                        "pixelsSha256": format!("{:x}", Sha256::digest(image.as_raw()))}),
                    );
                }
                Ok(
                    json!({"frames": evidence, "backend": if desktop::is_wayland() { "portal-pipewire" } else { "xcap" }}),
                )
            })();
            #[cfg(target_os = "linux")]
            if desktop::is_wayland() {
                crate::core::wayland::portal::release();
            }
            record("native-capture", result);
        }
    }
    #[cfg(not(unix))]
    checks.push(json!({"id": "native-desktop", "status": "failed", "error": "该工具用于 macOS/Linux 实机验收"}));
    if checks.is_empty() {
        return Err("请至少选择 --ports、--clipboard 或 --capture 中的一项".into());
    }
    let passed = checks.iter().all(|check| check["status"] == "passed");
    let platform = match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        value => value,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        value => value,
    };
    let desktop_session = if cfg!(target_os = "macos") {
        "aqua".to_string()
    } else {
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| {
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                "wayland"
            } else if std::env::var_os("DISPLAY").is_some() {
                "x11"
            } else {
                "unknown"
            }
            .to_string()
        })
    };
    let report = json!({
        "schema": 1, "platform": platform, "arch": arch, "version": env!("CARGO_PKG_VERSION"),
        "desktopSession": desktop_session,
        "createdAt": chrono::Utc::now().to_rfc3339(), "checks": checks,
        "scope": "native-probes", "fullAcceptance": false,
    });
    std::fs::write(
        output.join("native-report.json"),
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    println!(
        "原生验收记录：{}",
        output.join("native-report.json").display()
    );
    if passed {
        Ok(())
    } else {
        Err("部分原生验收失败，请查看报告".into())
    }
}

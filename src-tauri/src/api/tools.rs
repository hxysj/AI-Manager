use crate::core::error::ManagerError;
use crate::core::paths::{path_text, AppPaths};
use base64::Engine;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::ImageDecoder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Write as FmtWrite;
use std::io::{Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
#[cfg(target_os = "windows")]
use tokio::process::Command;
use url::Url;
use zip::write::SimpleFileOptions;

const MAX_IMAGE_EXPORT_COUNT: usize = 100;
const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;
const MAX_EXPORT_BYTES: usize = 200 * 1024 * 1024;
const MAX_PNG_PIXELS: u64 = 40_000_000;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageExportPayload {
    format: String,
    urls: Vec<String>,
    target_path: String,
}

struct DownloadedImage {
    file_name: String,
    extension: String,
    bytes: Vec<u8>,
}

struct PdfImage {
    width: u32,
    height: u32,
    color_space: &'static str,
    filter: &'static str,
    decode: Option<&'static str>,
    data: Vec<u8>,
}

pub async fn export_images(payload: Value) -> Result<Value, ManagerError> {
    let payload: ImageExportPayload = serde_json::from_value(payload)?;
    let format = payload.format.trim().to_ascii_lowercase();

    if !matches!(format.as_str(), "pdf" | "zip") {
        return Err(ManagerError::System("仅支持导出 PDF 或 ZIP。".to_string()));
    }
    if payload.target_path.trim().is_empty() {
        return Err(ManagerError::System("请选择图片导出位置。".to_string()));
    }

    let images = download_export_images(&payload.urls).await?;
    let image_count = images.len();
    let output = if format == "zip" {
        create_images_zip(&images)?
    } else {
        create_images_pdf(&images)?
    };

    tokio::fs::write(&payload.target_path, output).await?;

    Ok(json!({
      "filePath": payload.target_path,
      "imageCount": image_count,
      "format": format
    }))
}

async fn download_export_images(urls: &[String]) -> Result<Vec<DownloadedImage>, ManagerError> {
    if urls.is_empty() {
        return Err(ManagerError::System("请选择要导出的图片。".to_string()));
    }
    if urls.len() > MAX_IMAGE_EXPORT_COUNT {
        return Err(ManagerError::System(format!(
            "单次最多导出 {} 张图片。",
            MAX_IMAGE_EXPORT_COUNT
        )));
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent("AI-Manager-Image-Exporter/1.0")
        .build()
        .map_err(|error| ManagerError::System(format!("创建图片下载客户端失败：{}", error)))?;
    let mut images = Vec::with_capacity(urls.len());
    let mut total_bytes = 0_usize;

    for (index, value) in urls.iter().enumerate() {
        let url = Url::parse(value.trim())
            .map_err(|_| ManagerError::System(format!("第 {} 个图片链接无效。", index + 1)))?;

        if !matches!(url.scheme(), "http" | "https") {
            return Err(ManagerError::System(format!(
                "第 {} 个图片链接不是 HTTP 地址。",
                index + 1
            )));
        }

        let response = client.get(url).send().await.map_err(|error| {
            ManagerError::System(format!("第 {} 张图片下载失败：{}", index + 1, error))
        })?;

        if !response.status().is_success() {
            return Err(ManagerError::System(format!(
                "第 {} 张图片下载失败，服务返回 {}。",
                index + 1,
                response.status()
            )));
        }
        if response.content_length().unwrap_or_default() > MAX_IMAGE_BYTES as u64 {
            return Err(ManagerError::System(format!(
                "第 {} 张图片超过 25 MB 限制。",
                index + 1
            )));
        }

        let bytes = response.bytes().await.map_err(|error| {
            ManagerError::System(format!("读取第 {} 张图片失败：{}", index + 1, error))
        })?;

        if bytes.is_empty() {
            return Err(ManagerError::System(format!(
                "第 {} 张图片内容为空。",
                index + 1
            )));
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(ManagerError::System(format!(
                "第 {} 张图片超过 25 MB 限制。",
                index + 1
            )));
        }

        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_EXPORT_BYTES {
            return Err(ManagerError::System(
                "所选图片总大小超过 200 MB 限制。".to_string(),
            ));
        }

        let extension = detect_image_extension(&bytes).ok_or_else(|| {
            ManagerError::System(format!("第 {} 个链接返回的内容不是支持的图片。", index + 1))
        })?;

        images.push(DownloadedImage {
            file_name: format!("image-{:03}.{}", index + 1, extension),
            extension: extension.to_string(),
            bytes: bytes.to_vec(),
        });
    }

    Ok(images)
}

fn detect_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("bmp");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && matches!(&bytes[8..12], b"avif" | b"avis") {
        return Some("avif");
    }

    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    if text
        .trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n'])
        .starts_with("<svg")
        || text.contains("<svg")
    {
        return Some("svg");
    }

    None
}

fn create_images_zip(images: &[DownloadedImage]) -> Result<Vec<u8>, ManagerError> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for image in images {
        archive
            .start_file(&image.file_name, options)
            .map_err(|error| ManagerError::System(format!("创建 ZIP 文件失败：{}", error)))?;
        archive.write_all(&image.bytes)?;
    }

    archive
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| ManagerError::System(format!("完成 ZIP 文件失败：{}", error)))
}

fn create_images_pdf(images: &[DownloadedImage]) -> Result<Vec<u8>, ManagerError> {
    let pdf_images = images
        .iter()
        .map(prepare_pdf_image)
        .collect::<Result<Vec<_>, _>>()?;
    let object_count = 2 + pdf_images.len() * 3;
    let mut offsets = vec![0_usize; object_count + 1];
    let mut output = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();

    append_pdf_object(
        &mut output,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    )?;

    let mut pages = format!("<< /Type /Pages /Count {} /Kids [", pdf_images.len());
    for index in 0..pdf_images.len() {
        write!(&mut pages, " {} 0 R", 3 + index * 3)
            .map_err(|error| ManagerError::System(error.to_string()))?;
    }
    pages.push_str(" ] >>");
    append_pdf_object(&mut output, &mut offsets, 2, pages.as_bytes())?;

    for (index, image) in pdf_images.iter().enumerate() {
        let page_object = 3 + index * 3;
        let content_object = page_object + 1;
        let image_object = page_object + 2;
        let (page_width, page_height) = if image.width > image.height {
            (841.89_f64, 595.28_f64)
        } else {
            (595.28_f64, 841.89_f64)
        };
        let scale = ((page_width - 48.0) / f64::from(image.width))
            .min((page_height - 48.0) / f64::from(image.height));
        let image_width = f64::from(image.width) * scale;
        let image_height = f64::from(image.height) * scale;
        let offset_x = (page_width - image_width) / 2.0;
        let offset_y = (page_height - image_height) / 2.0;
        let page = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",
            page_width, page_height, image_object, content_object
        );
        let content = format!(
            "q\n{:.3} 0 0 {:.3} {:.3} {:.3} cm\n/Im0 Do\nQ",
            image_width, image_height, offset_x, offset_y
        );
        let mut image_dictionary = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /{} /BitsPerComponent 8 /Filter /{}",
            image.width, image.height, image.color_space, image.filter
        );

        if let Some(decode) = image.decode {
            write!(&mut image_dictionary, " /Decode {}", decode)
                .map_err(|error| ManagerError::System(error.to_string()))?;
        }

        append_pdf_object(&mut output, &mut offsets, page_object, page.as_bytes())?;
        append_pdf_stream_object(
            &mut output,
            &mut offsets,
            content_object,
            "<<",
            content.as_bytes(),
        )?;
        append_pdf_stream_object(
            &mut output,
            &mut offsets,
            image_object,
            &image_dictionary,
            &image.data,
        )?;
    }

    let xref_offset = output.len();
    write!(&mut output, "xref\n0 {}\n", object_count + 1)?;
    output.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        writeln!(&mut output, "{:010} 00000 n ", offset)?;
    }
    write!(
        &mut output,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        object_count + 1,
        xref_offset
    )?;

    Ok(output)
}

fn prepare_pdf_image(image: &DownloadedImage) -> Result<PdfImage, ManagerError> {
    if image.extension == "jpg" {
        let (width, height, components) = jpeg_metadata(&image.bytes).ok_or_else(|| {
            ManagerError::System(format!("无法读取 JPEG 图片：{}", image.file_name))
        })?;
        let (color_space, decode) = match components {
            1 => ("DeviceGray", None),
            3 => ("DeviceRGB", None),
            4 => ("DeviceCMYK", Some("[1 0 1 0 1 0 1 0]")),
            _ => {
                return Err(ManagerError::System(format!(
                    "JPEG 色彩格式暂不支持：{}",
                    image.file_name
                )))
            }
        };

        return Ok(PdfImage {
            width,
            height,
            color_space,
            filter: "DCTDecode",
            decode,
            data: image.bytes.clone(),
        });
    }

    if image.extension == "webp" {
        let png_image = DownloadedImage {
            file_name: image.file_name.replace(".webp", ".png"),
            extension: "png".to_string(),
            bytes: convert_webp_to_png(image)?,
        };

        return prepare_pdf_image(&png_image);
    }

    if image.extension != "png" {
        return Err(ManagerError::System(format!(
            "PDF 导出暂不支持 {} 图片，请改用 ZIP 导出。",
            image.extension.to_ascii_uppercase()
        )));
    }

    // PNG 统一转换为 RGB，并将透明区域铺为白色，保证 PDF 阅读器兼容。
    let mut decoder = png::Decoder::new(Cursor::new(&image.bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| ManagerError::System(format!("无法读取 PNG 图片：{}", error)))?;
    let width = reader.info().width;
    let height = reader.info().height;
    let pixels = u64::from(width) * u64::from(height);

    if pixels > MAX_PNG_PIXELS {
        return Err(ManagerError::System(format!(
            "PNG 图片像素过大：{}",
            image.file_name
        )));
    }

    let mut source = vec![0_u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut source)
        .map_err(|error| ManagerError::System(format!("解码 PNG 图片失败：{}", error)))?;
    let source = &source[..info.buffer_size()];
    let mut rgb = Vec::with_capacity(pixels as usize * 3);

    match info.color_type {
        png::ColorType::Rgb => rgb.extend_from_slice(source),
        png::ColorType::Rgba => {
            for pixel in source.chunks_exact(4) {
                let alpha = u16::from(pixel[3]);
                for channel in &pixel[..3] {
                    rgb.push(((u16::from(*channel) * alpha + 255 * (255 - alpha)) / 255) as u8);
                }
            }
        }
        png::ColorType::Grayscale => {
            for value in source {
                rgb.extend_from_slice(&[*value, *value, *value]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pixel in source.chunks_exact(2) {
                let alpha = u16::from(pixel[1]);
                let value = ((u16::from(pixel[0]) * alpha + 255 * (255 - alpha)) / 255) as u8;
                rgb.extend_from_slice(&[value, value, value]);
            }
        }
        png::ColorType::Indexed => {
            return Err(ManagerError::System(format!(
                "无法展开 PNG 调色板：{}",
                image.file_name
            )))
        }
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&rgb)?;

    Ok(PdfImage {
        width,
        height,
        color_space: "DeviceRGB",
        filter: "FlateDecode",
        decode: None,
        data: encoder.finish()?,
    })
}

fn convert_webp_to_png(image: &DownloadedImage) -> Result<Vec<u8>, ManagerError> {
    let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(&image.bytes))
        .map_err(|error| ManagerError::System(format!("无法读取 WEBP 图片：{}", error)))?;
    let (width, height) = decoder.dimensions();
    let pixels = u64::from(width) * u64::from(height);

    if pixels > MAX_PNG_PIXELS {
        return Err(ManagerError::System(format!(
            "WEBP 图片像素过大：{}",
            image.file_name
        )));
    }

    let color_type = match decoder.color_type() {
        image::ColorType::Rgb8 => png::ColorType::Rgb,
        image::ColorType::Rgba8 => png::ColorType::Rgba,
        _ => {
            return Err(ManagerError::System(format!(
                "WEBP 色彩格式暂不支持：{}",
                image.file_name
            )))
        }
    };
    let mut source = vec![0_u8; decoder.total_bytes() as usize];
    decoder
        .read_image(&mut source)
        .map_err(|error| ManagerError::System(format!("解码 WEBP 图片失败：{}", error)))?;
    let mut png_bytes = Vec::new();

    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(color_type);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| ManagerError::System(format!("创建 PNG 图片失败：{}", error)))?;
        writer
            .write_image_data(&source)
            .map_err(|error| ManagerError::System(format!("转换 PNG 图片失败：{}", error)))?;
    }

    Ok(png_bytes)
}

fn jpeg_metadata(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }

    let mut cursor = 2_usize;
    while cursor + 3 < bytes.len() {
        while cursor < bytes.len() && bytes[cursor] != 0xff {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }

        let marker = bytes[cursor];
        cursor += 1;
        if marker == 0xd8 || marker == 0xd9 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if cursor + 2 > bytes.len() {
            break;
        }

        let length = usize::from(u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]));
        if length < 2 || cursor + length > bytes.len() {
            break;
        }

        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) && length >= 8
        {
            let height = u32::from(u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]));
            let components = bytes[cursor + 7];

            return (width > 0 && height > 0).then_some((width, height, components));
        }

        cursor += length;
    }

    None
}

fn append_pdf_object(
    output: &mut Vec<u8>,
    offsets: &mut [usize],
    number: usize,
    body: &[u8],
) -> Result<(), ManagerError> {
    offsets[number] = output.len();
    writeln!(output, "{} 0 obj", number)?;
    output.extend_from_slice(body);
    output.extend_from_slice(b"\nendobj\n");
    Ok(())
}

fn append_pdf_stream_object(
    output: &mut Vec<u8>,
    offsets: &mut [usize],
    number: usize,
    dictionary: &str,
    data: &[u8],
) -> Result<(), ManagerError> {
    offsets[number] = output.len();
    writeln!(output, "{} 0 obj", number)?;
    writeln!(output, "{} /Length {} >>", dictionary, data.len())?;
    output.extend_from_slice(b"stream\n");
    output.extend_from_slice(data);
    output.extend_from_slice(b"\nendstream\nendobj\n");
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminatePortProcessPayload {
    pid: u32,
    started_at: i64,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsPortRecord {
    protocol: String,
    local_address: String,
    local_port: u16,
    pid: u32,
    process_name: String,
    executable_path: String,
    service_names: Vec<String>,
    started_at: i64,
}

pub async fn list_ports() -> Result<Value, ManagerError> {
    #[cfg(not(target_os = "windows"))]
    {
        Err(ManagerError::System(
            "端口监测目前仅支持 Windows".to_string(),
        ))
    }

    #[cfg(target_os = "windows")]
    {
        let script = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$processMap = @{}
Get-Process -ErrorAction SilentlyContinue | ForEach-Object {
  $processMap[[string]$_.Id] = $_
}

$serviceMap = @{}
try {
  Get-CimInstance Win32_Service -ErrorAction Stop | Where-Object { $_.ProcessId -gt 0 } | ForEach-Object {
    $key = [string]$_.ProcessId
    if ($serviceMap.ContainsKey($key)) {
      $serviceMap[$key] = @($serviceMap[$key]) + [string]$_.DisplayName
    } else {
      $serviceMap[$key] = @([string]$_.DisplayName)
    }
  }
} catch {}

$records = @(
  Get-NetTCPConnection -State Listen -ErrorAction Stop | ForEach-Object {
    $ownerId = [int]$_.OwningProcess
    $process = $processMap[[string]$ownerId]
    $processName = ''
    $processPath = ''
    $startedAt = 0
    if ($null -ne $process) {
      $processName = [string]$process.ProcessName
      try { $processPath = [string]$process.Path } catch {}
      try {
        $started = [DateTimeOffset]$process.StartTime
        $startedAt = $started.ToUnixTimeSeconds()
      } catch {}
    }

    [PSCustomObject]@{
      protocol = 'TCP'
      localAddress = [string]$_.LocalAddress
      localPort = [int]$_.LocalPort
      pid = $ownerId
      processName = $processName
      executablePath = $processPath
      serviceNames = @($serviceMap[[string]$ownerId] | Where-Object { $null -ne $_ })
      startedAt = $startedAt
    }
  }

  Get-NetUDPEndpoint -ErrorAction Stop | ForEach-Object {
    $ownerId = [int]$_.OwningProcess
    $process = $processMap[[string]$ownerId]
    $processName = ''
    $processPath = ''
    $startedAt = 0
    if ($null -ne $process) {
      $processName = [string]$process.ProcessName
      try { $processPath = [string]$process.Path } catch {}
      try {
        $started = [DateTimeOffset]$process.StartTime
        $startedAt = $started.ToUnixTimeSeconds()
      } catch {}
    }

    [PSCustomObject]@{
      protocol = 'UDP'
      localAddress = [string]$_.LocalAddress
      localPort = [int]$_.LocalPort
      pid = $ownerId
      processName = $processName
      executablePath = $processPath
      serviceNames = @($serviceMap[[string]$ownerId] | Where-Object { $null -ne $_ })
      startedAt = $startedAt
    }
  }
)

ConvertTo-Json -InputObject @($records | Sort-Object localPort, protocol, pid) -Compress -Depth 4
"#;
        let mut command = Command::new("powershell.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(ManagerError::System(
                "读取端口失败，请确认系统网络管理服务可用".to_string(),
            ));
        }

        let content = String::from_utf8_lossy(&output.stdout);
        let records: Vec<WindowsPortRecord> = serde_json::from_str(content.trim())
            .map_err(|_| ManagerError::System("无法解析系统返回的端口信息".to_string()))?;
        let ports = records
            .into_iter()
            .map(|record| {
                let protected_reason = protected_process_reason(record.pid, &record.process_name)
                    .map(str::to_string)
                    .or_else(|| {
                        if record.process_name.trim().is_empty() {
                            Some("无法读取进程信息".to_string())
                        } else if record.started_at <= 0 {
                            Some("无法校验进程启动时间".to_string())
                        } else {
                            None
                        }
                    });

                json!({
                  "id": format!(
                    "{}:{}:{}:{}",
                    record.protocol, record.local_address, record.local_port, record.pid
                  ),
                  "protocol": record.protocol,
                  "localAddress": record.local_address,
                  "localPort": record.local_port,
                  "pid": record.pid,
                  "processName": record.process_name,
                  "executablePath": record.executable_path,
                  "serviceNames": record.service_names,
                  "startedAt": record.started_at,
                  "canTerminate": protected_reason.is_none(),
                  "protectedReason": protected_reason.unwrap_or_default()
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({ "ports": ports }))
    }
}

pub async fn terminate_port_process(payload: Value) -> Result<Value, ManagerError> {
    let payload: TerminatePortProcessPayload = serde_json::from_value(payload)?;

    #[cfg(not(target_os = "windows"))]
    {
        let _ = payload;
        Err(ManagerError::System(
            "进程关闭目前仅支持 Windows".to_string(),
        ))
    }

    #[cfg(target_os = "windows")]
    {
        if payload.started_at <= 0 {
            return Err(ManagerError::System("进程校验信息不完整".to_string()));
        }

        // 关闭前重新读取进程身份，避免端口列表中的 PID 已被系统复用。
        let inspect_script = format!(
            r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$process = Get-Process -Id {} -ErrorAction Stop
$started = [DateTimeOffset]$process.StartTime
[PSCustomObject]@{{
  processName = [string]$process.ProcessName
  startedAt = $started.ToUnixTimeSeconds()
}} | ConvertTo-Json -Compress
"#,
            payload.pid
        );
        let mut inspect_command = Command::new("powershell.exe");
        inspect_command.creation_flags(CREATE_NO_WINDOW);
        let inspect_output = inspect_command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &inspect_script,
            ])
            .output()
            .await?;

        if !inspect_output.status.success() {
            return Err(ManagerError::System(
                "进程已退出或当前无权读取该进程".to_string(),
            ));
        }

        let process: Value = serde_json::from_slice(&inspect_output.stdout)
            .map_err(|_| ManagerError::System("无法校验目标进程".to_string()))?;
        let process_name = process
            .get("processName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let started_at = process
            .get("startedAt")
            .and_then(Value::as_i64)
            .unwrap_or_default();

        if started_at != payload.started_at {
            return Err(ManagerError::System(
                "目标 PID 已被其他进程占用，请刷新列表后重试".to_string(),
            ));
        }
        if let Some(reason) = protected_process_reason(payload.pid, process_name) {
            return Err(ManagerError::System(reason.to_string()));
        }

        let mut terminate_command = Command::new("taskkill.exe");
        terminate_command.creation_flags(CREATE_NO_WINDOW);
        let output = terminate_command
            .args(["/PID", &payload.pid.to_string(), "/T", "/F"])
            .output()
            .await?;

        if !output.status.success() {
            return Err(ManagerError::System(format!(
                "无法关闭进程 {}，请尝试以管理员身份运行应用",
                payload.pid
            )));
        }

        Ok(json!({ "pid": payload.pid }))
    }
}

fn protected_process_reason(pid: u32, process_name: &str) -> Option<&'static str> {
    if pid == std::process::id() {
        return Some("不能在端口监测中关闭当前应用");
    }
    if pid <= 4 {
        return Some("系统核心进程不可关闭");
    }

    let process_name = process_name
        .trim()
        .trim_end_matches(".exe")
        .to_ascii_lowercase();
    if matches!(
        process_name.as_str(),
        "system"
            | "registry"
            | "smss"
            | "csrss"
            | "wininit"
            | "services"
            | "lsass"
            | "winlogon"
            | "svchost"
            | "fontdrvhost"
            | "secure system"
            | "memory compression"
    ) {
        return Some("系统关键进程不可关闭");
    }

    None
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameCodexPetPayload {
    id: String,
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToggleCodexPetPayload {
    id: String,
    enabled: bool,
}

#[derive(Deserialize)]
struct CodexPetIdPayload {
    id: String,
}

pub async fn list_codex_pets(paths: &AppPaths, cli_targets: &Value) -> Result<Value, ManagerError> {
    let codex_pets_dir = codex_pets_dir(cli_targets)?;

    tokio::fs::create_dir_all(&codex_pets_dir).await?;
    tokio::fs::create_dir_all(&paths.disabled_pets_dir).await?;
    migrate_legacy_codex_pets(paths, &codex_pets_dir).await?;

    let mut pets = read_pets(&codex_pets_dir, true).await?;
    let disabled_pets = read_pets(Path::new(&paths.disabled_pets_dir), false).await?;
    if disabled_pets.iter().any(|disabled_pet| {
        pets.iter()
            .any(|pet| pet_string(pet, "id") == pet_string(disabled_pet, "id"))
    }) {
        return Err(ManagerError::System(
            "Codex 目录与已禁用目录存在同名宠物".to_string(),
        ));
    }
    pets.extend(disabled_pets);
    pets.sort_by(|left, right| {
        pet_string(left, "displayName").cmp(&pet_string(right, "displayName"))
    });

    Ok(json!({
      "codexPetsPath": path_text(&codex_pets_dir),
      "disabledPetsPath": paths.disabled_pets_dir,
      "pets": pets
    }))
}

pub async fn rename_codex_pet(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let payload: RenameCodexPetPayload = serde_json::from_value(payload)?;
    let id = valid_pet_id(&payload.id)?;
    let display_name = payload.display_name.trim();

    if display_name.is_empty() {
        return Err(ManagerError::System("宠物名称不能为空".to_string()));
    }

    let codex_pets_dir = codex_pets_dir(cli_targets)?;
    let pet_dir = codex_pet_dir(&codex_pets_dir, Path::new(&paths.disabled_pets_dir), id)?;
    let pet_json_path = pet_dir.join("pet.json");
    let content = tokio::fs::read_to_string(&pet_json_path).await?;
    let mut pet_json: Value = serde_json::from_str(&content)?;
    let Some(pet) = pet_json.as_object_mut() else {
        return Err(ManagerError::System(format!(
            "宠物配置不是 JSON 对象：{}",
            path_text(&pet_json_path)
        )));
    };

    pet.insert("displayName".to_string(), json!(display_name));
    tokio::fs::write(
        &pet_json_path,
        format!("{}\n", serde_json::to_string_pretty(&pet_json)?),
    )
    .await?;

    Ok(json!({ "id": id, "displayName": display_name }))
}

pub async fn toggle_codex_pet(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let payload: ToggleCodexPetPayload = serde_json::from_value(payload)?;
    let id = valid_pet_id(&payload.id)?;
    let codex_pets_dir = codex_pets_dir(cli_targets)?;

    let active_path = codex_pets_dir.join(id);
    let disabled_path = Path::new(&paths.disabled_pets_dir).join(id);

    if payload.enabled {
        if active_path.exists() {
            return Ok(json!({ "id": id, "enabled": true }));
        }
        if !disabled_path.exists() {
            return Err(ManagerError::System(format!("未找到宠物：{}", id)));
        }

        move_pet_dir(&disabled_path, &active_path).await?;
    } else {
        if disabled_path.exists() {
            return Ok(json!({ "id": id, "enabled": false }));
        }
        if !active_path.exists() {
            return Err(ManagerError::System(format!("未找到宠物：{}", id)));
        }

        move_pet_dir(&active_path, &disabled_path).await?;
    }

    Ok(json!({ "id": id, "enabled": payload.enabled }))
}

pub async fn delete_codex_pet(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let payload: CodexPetIdPayload = serde_json::from_value(payload)?;
    let id = valid_pet_id(&payload.id)?;
    let codex_pets_dir = codex_pets_dir(cli_targets)?;

    let active_path = codex_pets_dir.join(id);
    let disabled_path = Path::new(&paths.disabled_pets_dir).join(id);

    if active_path.exists() {
        tokio::fs::remove_dir_all(&active_path).await?;
    } else if disabled_path.exists() {
        tokio::fs::remove_dir_all(&disabled_path).await?;
    } else {
        return Err(ManagerError::System(format!("未找到宠物：{}", id)));
    }

    Ok(json!({ "id": id }))
}

fn codex_pets_dir(cli_targets: &Value) -> Result<PathBuf, ManagerError> {
    let Some(codex_target) = cli_targets.as_array().and_then(|targets| {
        targets.iter().find(|target| {
            target.get("id").and_then(Value::as_str) == Some("codex")
                && target.get("installed").and_then(Value::as_bool) == Some(true)
        })
    }) else {
        return Err(ManagerError::System("未检测到已安装的 Codex".to_string()));
    };
    let config_path = pet_string(codex_target, "configPath");

    if config_path.is_empty() {
        return Err(ManagerError::System("Codex 配置目录不存在".to_string()));
    }

    Ok(Path::new(&config_path).join("pets"))
}

// 将旧版本遗留在应用目录中的启用宠物还原到 Codex 目录，后续不再创建链接。
async fn migrate_legacy_codex_pets(
    paths: &AppPaths,
    codex_pets_dir: &Path,
) -> Result<(), ManagerError> {
    let legacy_pets_dir = Path::new(&paths.pets_dir);
    let Ok(mut entries) = tokio::fs::read_dir(legacy_pets_dir).await else {
        return Ok(());
    };

    while let Some(entry) = entries.next_entry().await? {
        let source_path = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();

        if valid_pet_id(&id).is_err() || !is_codex_pet_directory(&source_path).await {
            continue;
        }

        let target_path = codex_pets_dir.join(&id);
        if target_path.exists() {
            let stat = tokio::fs::symlink_metadata(&target_path).await?;
            if !stat.file_type().is_symlink() || !linked_to(&target_path, &source_path).await {
                return Err(ManagerError::System(format!(
                    "Codex 宠物目录与旧版受管宠物冲突：{}",
                    path_text(&target_path)
                )));
            }

            remove_legacy_pet_link(&target_path).await?;
        }

        move_pet_dir(&source_path, &target_path).await?;
    }

    Ok(())
}

async fn read_pets(pets_dir: &Path, enabled: bool) -> Result<Vec<Value>, ManagerError> {
    let mut entries = tokio::fs::read_dir(pets_dir).await?;
    let mut pets = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let pet_dir = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();

        if valid_pet_id(&id).is_err() || !is_codex_pet_directory(&pet_dir).await {
            continue;
        }

        let pet_json_path = pet_dir.join("pet.json");
        let pet_json = tokio::fs::read_to_string(&pet_json_path)
            .await
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .unwrap_or_else(|| json!({}));
        let spritesheet = tokio::fs::read(pet_dir.join("spritesheet.webp")).await?;
        let display_name = pet_string(&pet_json, "displayName");

        pets.push(json!({
          "id": id,
          "displayName": if display_name.is_empty() { pet_string(&pet_json, "id") } else { display_name },
          "description": pet_string(&pet_json, "description"),
          "enabled": enabled,
          "shape": "8 x 9 动画精灵",
          "spritesheetData": format!("data:image/webp;base64,{}", base64::engine::general_purpose::STANDARD.encode(spritesheet))
        }));
    }

    Ok(pets)
}

pub(crate) async fn is_codex_pet_directory(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .map(|stat| stat.is_dir())
        .unwrap_or(false)
        && tokio::fs::metadata(path.join("pet.json"))
            .await
            .map(|stat| stat.is_file())
            .unwrap_or(false)
        && tokio::fs::metadata(path.join("spritesheet.webp"))
            .await
            .map(|stat| stat.is_file())
            .unwrap_or(false)
}

fn codex_pet_dir(
    codex_pets_dir: &Path,
    disabled_pets_dir: &Path,
    id: &str,
) -> Result<PathBuf, ManagerError> {
    let active_path = codex_pets_dir.join(id);

    if active_path.exists() {
        return Ok(active_path);
    }

    let disabled_path = disabled_pets_dir.join(id);
    if disabled_path.exists() {
        return Ok(disabled_path);
    }

    Err(ManagerError::System(format!("未找到宠物：{}", id)))
}

fn valid_pet_id(id: &str) -> Result<&str, ManagerError> {
    let id = id.trim();
    let path = Path::new(id);

    if id.is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(ManagerError::System("宠物标识不合法".to_string()));
    }

    Ok(id)
}

fn pet_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

async fn linked_to(target_path: &Path, source_path: &Path) -> bool {
    matches!(
        (
            tokio::fs::canonicalize(target_path).await,
            tokio::fs::canonicalize(source_path).await
        ),
        (Ok(target), Ok(source)) if target == source
    )
}

async fn remove_legacy_pet_link(target_path: &Path) -> Result<(), ManagerError> {
    let Ok(stat) = tokio::fs::symlink_metadata(target_path).await else {
        return Ok(());
    };

    if !stat.file_type().is_symlink() {
        return Err(ManagerError::System(format!(
            "Codex 宠物目录不是旧版链接：{}",
            path_text(target_path)
        )));
    }

    match tokio::fs::remove_dir(target_path).await {
        Ok(_) => Ok(()),
        Err(_) => {
            tokio::fs::remove_file(target_path).await?;
            Ok(())
        }
    }
}

async fn move_pet_dir(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    match tokio::fs::rename(source_path, target_path).await {
        Ok(_) => Ok(()),
        Err(_) => {
            copy_pet_dir(source_path, target_path).await?;
            tokio::fs::remove_dir_all(source_path).await?;
            Ok(())
        }
    }
}

async fn copy_pet_dir(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    tokio::fs::create_dir_all(target_path).await?;
    let mut entries = tokio::fs::read_dir(source_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let source_child = entry.path();
        let target_child = target_path.join(entry.file_name());
        let stat = tokio::fs::symlink_metadata(&source_child).await?;

        if stat.file_type().is_symlink() {
            return Err(ManagerError::System(format!(
                "宠物目录包含链接，已拒绝迁移：{}",
                path_text(&source_child)
            )));
        }

        if stat.is_dir() {
            Box::pin(copy_pet_dir(&source_child, &target_child)).await?;
        } else if stat.is_file() {
            tokio::fs::copy(&source_child, &target_child).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::resolve_app_paths;
    use std::io::Read;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn test_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&[255, 0, 0, 255, 0, 0, 255, 128])
                .unwrap();
        }
        bytes
    }

    fn test_webp() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .encode(
                &[255, 0, 0, 255, 0, 0, 255, 128],
                2,
                1,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn detects_supported_image_signatures() {
        assert_eq!(
            detect_image_extension(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("jpg")
        );
        assert_eq!(detect_image_extension(&test_png()), Some("png"));
        assert_eq!(detect_image_extension(b"GIF89a"), Some("gif"));
        assert_eq!(
            detect_image_extension(b"<svg viewBox=\"0 0 1 1\"></svg>"),
            Some("svg")
        );
        assert_eq!(detect_image_extension(b"plain text"), None);
    }

    #[test]
    fn creates_zip_with_ordered_image_names() {
        let images = vec![
            DownloadedImage {
                file_name: "image-001.jpg".to_string(),
                extension: "jpg".to_string(),
                bytes: vec![0xff, 0xd8, 0xff],
            },
            DownloadedImage {
                file_name: "image-002.png".to_string(),
                extension: "png".to_string(),
                bytes: test_png(),
            },
        ];
        let zip = create_images_zip(&images).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
        let mut first = Vec::new();

        archive
            .by_name("image-001.jpg")
            .unwrap()
            .read_to_end(&mut first)
            .unwrap();
        assert_eq!(first, vec![0xff, 0xd8, 0xff]);
        assert!(archive.by_name("image-002.png").is_ok());
    }

    #[test]
    fn creates_pdf_from_transparent_png() {
        let pdf = create_images_pdf(&[DownloadedImage {
            file_name: "image-001.png".to_string(),
            extension: "png".to_string(),
            bytes: test_png(),
        }])
        .unwrap();
        let text = String::from_utf8_lossy(&pdf);

        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(text.contains("/Count 1"));
        assert!(text.contains("/Filter /FlateDecode"));
        assert!(text.ends_with("%%EOF\n"));
    }

    #[test]
    fn converts_webp_to_png_before_creating_pdf() {
        let image = DownloadedImage {
            file_name: "image-001.webp".to_string(),
            extension: "webp".to_string(),
            bytes: test_webp(),
        };
        let png = convert_webp_to_png(&image).unwrap();
        let pdf = create_images_pdf(&[image]).unwrap();
        let text = String::from_utf8_lossy(&pdf);

        assert_eq!(detect_image_extension(&png), Some("png"));
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(text.contains("/Count 1"));
        assert!(text.contains("/Filter /FlateDecode"));
        assert!(text.ends_with("%%EOF\n"));
    }

    #[test]
    fn reads_jpeg_dimensions_and_components() {
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00,
            0x20, 0x00, 0x30, 0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00,
        ];

        assert_eq!(jpeg_metadata(&jpeg), Some((48, 32, 3)));
    }

    #[test]
    fn exports_images_to_selected_path() {
        tauri::async_runtime::block_on(async {
            let image = test_png();
            let source_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let source_url = format!(
                "http://127.0.0.1:{}/image.png",
                source_listener.local_addr().unwrap().port()
            );
            let source_task = tokio::spawn(async move {
                let (mut stream, _) = source_listener.accept().await.unwrap();
                let mut request = [0_u8; 1024];
                let _ = stream.read(&mut request).await.unwrap();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    image.len()
                );

                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(&image).await.unwrap();
            });
            let target_path = std::env::temp_dir().join(format!(
                "monkey-thief-image-export-{}.zip",
                std::process::id()
            ));
            let result = export_images(json!({
              "format": "zip",
              "urls": [source_url],
              "targetPath": path_text(&target_path)
            }))
            .await
            .unwrap();

            assert_eq!(result["imageCount"], 1);
            assert!(tokio::fs::read(&target_path)
                .await
                .unwrap()
                .starts_with(&[0x50, 0x4b]));
            source_task.await.unwrap();
            tokio::fs::remove_file(target_path).await.unwrap();
        });
    }

    #[test]
    fn protects_system_port_processes() {
        assert_eq!(
            protected_process_reason(4, "System"),
            Some("系统核心进程不可关闭")
        );
        assert_eq!(
            protected_process_reason(128, "svchost.exe"),
            Some("系统关键进程不可关闭")
        );
        assert_eq!(protected_process_reason(u32::MAX, "node.exe"), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn lists_windows_ports() {
        tauri::async_runtime::block_on(async {
            let result = list_ports().await.unwrap();
            let ports = result["ports"].as_array().unwrap();

            assert!(!ports.is_empty());
            assert!(ports.iter().all(|port| {
                port.get("protocol").and_then(Value::as_str).is_some()
                    && port.get("localPort").and_then(Value::as_u64).is_some()
                    && port.get("pid").and_then(Value::as_u64).is_some()
                    && port.get("canTerminate").and_then(Value::as_bool).is_some()
            }));
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn terminates_verified_port_process() {
        tauri::async_runtime::block_on(async {
            let mut child_command = Command::new("powershell.exe");
            child_command.creation_flags(CREATE_NO_WINDOW);
            let mut child = child_command
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Start-Sleep -Seconds 30",
                ])
                .spawn()
                .unwrap();
            let pid = child.id().unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            let script = format!(
                "$process = Get-Process -Id {}; $started = [DateTimeOffset]$process.StartTime; $started.ToUnixTimeSeconds()",
                pid
            );
            let mut inspect_command = Command::new("powershell.exe");
            inspect_command.creation_flags(CREATE_NO_WINDOW);
            let output = inspect_command
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &script,
                ])
                .output()
                .await
                .unwrap();
            let started_at = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<i64>()
                .unwrap();

            // 仅终止本测试创建的进程，验证 PID 身份校验和关闭链路。
            let result = terminate_port_process(json!({
              "pid": pid,
              "startedAt": started_at
            }))
            .await;
            if result.is_err() {
                let _ = child.kill().await;
            }

            result.unwrap();
            let status = child.wait().await.unwrap();
            assert!(!status.success());
        });
    }

    #[test]
    fn manages_codex_pet_lifecycle() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "ai-manager-codex-pets-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let paths = resolve_app_paths(&root);
            let config_path = root.join("codex");
            let runtime_pet = config_path.join("pets").join("demo");
            tokio::fs::create_dir_all(&runtime_pet).await.unwrap();
            tokio::fs::write(
                runtime_pet.join("pet.json"),
                r#"{"id":"demo","displayName":"演示宠物","description":"用于测试"}"#,
            )
            .await
            .unwrap();
            tokio::fs::write(runtime_pet.join("spritesheet.webp"), [0_u8, 1, 2])
                .await
                .unwrap();
            let cli_targets = json!([{
              "id": "codex",
              "installed": true,
              "configPath": path_text(&config_path)
            }]);

            let result = list_codex_pets(&paths, &cli_targets).await.unwrap();
            assert_eq!(result["pets"].as_array().unwrap().len(), 1);
            assert!(runtime_pet.exists());
            assert!(!tokio::fs::symlink_metadata(&runtime_pet)
                .await
                .unwrap()
                .file_type()
                .is_symlink());

            rename_codex_pet(
                &paths,
                &cli_targets,
                json!({ "id": "demo", "displayName": "新的名称" }),
            )
            .await
            .unwrap();
            let content = tokio::fs::read_to_string(runtime_pet.join("pet.json"))
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&content).unwrap()["displayName"],
                "新的名称"
            );

            toggle_codex_pet(
                &paths,
                &cli_targets,
                json!({ "id": "demo", "enabled": false }),
            )
            .await
            .unwrap();
            assert!(Path::new(&paths.disabled_pets_dir).join("demo").exists());
            assert!(!config_path.join("pets").join("demo").exists());

            toggle_codex_pet(
                &paths,
                &cli_targets,
                json!({ "id": "demo", "enabled": true }),
            )
            .await
            .unwrap();
            assert!(runtime_pet.exists());
            assert!(!tokio::fs::symlink_metadata(&runtime_pet)
                .await
                .unwrap()
                .file_type()
                .is_symlink());

            delete_codex_pet(&paths, &cli_targets, json!({ "id": "demo" }))
                .await
                .unwrap();
            assert!(!runtime_pet.exists());

            let _ = tokio::fs::remove_dir_all(&root).await;
        });
    }
}

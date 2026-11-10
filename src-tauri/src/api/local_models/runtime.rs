use super::{failure, is_link, AppPaths, ManagerError, FILES, MODEL_ID};
use futures_util::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

// 固定使用官方支持 MiniCPM-OCR 的版本，并校验官方发布包的 SHA-256。
const RUNTIME: &str = "llama-b11901-bin-win-cpu-x64";
const ARCHIVE_BYTES: usize = 24_228_082;
const ARCHIVE_SHA256: &str = "039951745399d7c1228b1e882d6e41b24d393f1795e3a6e176162bfb8193500a";
pub(super) const SUPPORTED: bool = cfg!(all(target_os = "windows", target_arch = "x86_64"));
const MAX_EXTRACTED_BYTES: u64 = 256 * 1024 * 1024;
const INFERENCE_ARGS: &[&str] = &[
    "--ctx-size",
    "8192",
    "--parallel",
    "1",
    "--n-gpu-layers",
    "0",
    "--no-mmproj-offload",
    "--jinja",
    "--reasoning",
    "off",
    "--no-webui",
];

pub(super) fn validation_profile() -> String {
    let mut digest = Sha256::new();
    digest.update(ARCHIVE_SHA256);
    digest.update(INFERENCE_ARGS.join("\0"));
    digest.update(include_bytes!("ocr-probe.png"));
    // 仅存指纹，不把计算机名或 CPU 标识写入模型记录。
    digest.update(std::env::var("COMPUTERNAME").unwrap_or_default());
    digest.update(std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default());
    if let Ok((total, _)) = super::requirements::memory_status() {
        digest.update(total.to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

pub(super) async fn validate(paths: &AppPaths, directory: &Path) -> Result<(), ManagerError> {
    use base64::Engine;
    let image = base64::engine::general_purpose::STANDARD.encode(include_bytes!("ocr-probe.png"));
    let result = tokio::time::timeout(
        Duration::from_secs(180),
        generate(
            paths,
            directory,
            json!({
                "prompt": "识别图片中的文字，只输出原文。",
                "image": format!("data:image/png;base64,{image}"),
            }),
        ),
    )
    .await
    .map_err(|_| failure("运行校验超时，请释放资源后重试"))??;
    let text: String = result["text"]
        .as_str()
        .unwrap_or("")
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if !text.contains("helloocr12345") {
        return Err(failure("模型已加载，但 OCR 校验没有返回预期文字"));
    }
    Ok(())
}

fn runtime_directory(paths: &AppPaths) -> Result<PathBuf, ManagerError> {
    let mut directory = PathBuf::from(&paths.workspace_root);
    for part in ["local-ai", "runtime", RUNTIME] {
        directory.push(part);
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => {
                return Err(failure("推理引擎目录不能是链接或普通文件"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(directory)
}

pub(super) fn required_disk_bytes(paths: &AppPaths) -> Result<u64, ManagerError> {
    let directory = runtime_directory(paths)?;
    if super::file_size(&directory.join("installed.json"))? > 0
        && super::file_size(&directory.join("llama-server.exe"))? > 0
    {
        return Ok(0);
    }
    // 压缩包在内存中校验，磁盘只需预留解压空间。
    Ok(MAX_EXTRACTED_BYTES)
}

async fn executable(paths: &AppPaths) -> Result<PathBuf, ManagerError> {
    if !SUPPORTED {
        return Err(failure("本地模型推理暂不支持当前系统"));
    }
    let directory = runtime_directory(paths)?;
    let executable = directory.join("llama-server.exe");
    if required_disk_bytes(paths)? == 0 {
        return Ok(executable);
    }
    let client = reqwest::Client::builder().build().map_err(failure)?;
    let response = client
        .get(format!(
            "https://github.com/ggml-org/llama.cpp/releases/download/b11901/{RUNTIME}.zip"
        ))
        .send()
        .await
        .map_err(|error| failure(format!("本地推理引擎下载失败：{error}")))?
        .error_for_status()
        .map_err(failure)?;
    let mut bytes = Vec::with_capacity(ARCHIVE_BYTES);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(failure)?;
        if bytes.len() + chunk.len() > ARCHIVE_BYTES {
            return Err(failure("本地推理引擎下载包大小无效"));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != ARCHIVE_BYTES || format!("{:x}", Sha256::digest(&bytes)) != ARCHIVE_SHA256 {
        return Err(failure("本地推理引擎下载校验失败，请重试"));
    }
    tokio::task::spawn_blocking(move || install(&bytes, &directory))
        .await
        .map_err(failure)??;
    Ok(executable)
}

fn install(bytes: &[u8], directory: &Path) -> Result<(), ManagerError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(failure)?;
    let mut total = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(failure)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| failure("推理引擎包包含非法路径"))?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(failure("推理引擎包包含链接"));
        }
        total += entry.size();
        if total > MAX_EXTRACTED_BYTES {
            return Err(failure("推理引擎包解压大小无效"));
        }
        let target = directory.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            std::fs::create_dir_all(target.parent().unwrap())?;
            // 不沿用可能由外部创建的链接文件。
            if std::fs::symlink_metadata(&target).is_ok_and(|metadata| is_link(&metadata)) {
                return Err(failure("推理引擎文件不能是链接"));
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(target)?)?;
        }
    }
    if super::file_size(&directory.join("llama-server.exe"))? == 0 {
        return Err(failure("推理引擎包缺少可执行文件"));
    }
    std::fs::write(
        directory.join("installed.json"),
        json!({"sha256": ARCHIVE_SHA256}).to_string(),
    )?;
    Ok(())
}

pub(super) async fn generate(
    paths: &AppPaths,
    directory: &Path,
    payload: Value,
) -> Result<Value, ManagerError> {
    let executable = executable(paths).await?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let base_url = format!("http://127.0.0.1:{port}");
    drop(listener);
    let key = uuid::Uuid::new_v4().to_string();
    let mut command = Command::new(executable);
    command
        .args(["--model"])
        .arg(directory.join(FILES[0].name))
        .args(["--mmproj"])
        .arg(directory.join(FILES[1].name))
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--alias",
            MODEL_ID,
            "--api-key",
            &key,
        ])
        .args(INFERENCE_ARGS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|error| failure(format!("本地模型加载失败：{error}")))?;
    let mut stderr = child.stderr.take().unwrap();
    // 持续排空日志，避免输出缓冲区阻塞推理；仅保留末尾供失败定位。
    let logs = tokio::spawn(async move {
        let mut tail = Vec::new();
        let mut buffer = [0; 1024];
        while let Ok(count) = stderr.read(&mut buffer).await {
            if count == 0 {
                break;
            }
            tail.extend_from_slice(&buffer[..count]);
            if tail.len() > 8192 {
                tail.drain(..tail.len() - 8192);
            }
        }
        String::from_utf8_lossy(&tail).into_owned()
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(failure)?;
    let result = async {
        let started = tokio::time::Instant::now();
        loop {
            if child.try_wait()?.is_some() {
                return Err(failure("本地模型加载失败"));
            }
            if client
                .get(format!("{base_url}/health"))
                .bearer_auth(&key)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                break;
            }
            if started.elapsed() > Duration::from_secs(180) {
                return Err(failure("本地模型加载未完成，请检查内存后重试"));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        completion(&client, &base_url, &key, payload).await
    }
    .await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    let logs = logs.await.unwrap_or_default();
    result.map_err(|error| {
        if logs.is_empty() {
            error
        } else {
            failure(format!("{error}\n{logs}"))
        }
    })
}

async fn completion(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .bearer_auth(key)
        .json(&json!({
            "model": MODEL_ID,
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": payload["prompt"]},
                {"type": "image_url", "image_url": {"url": payload["image"]}}
            ]}],
            "temperature": 0,
            "max_tokens": 4096,
            "stream": false
        }))
        .send()
        .await
        .map_err(failure)?;
    let status = response.status();
    let body: Value = response.json().await.map_err(failure)?;
    if !status.is_success() {
        return Err(failure(format!(
            "本地模型识别失败（HTTP {}）：{}",
            status.as_u16(),
            body["error"]["message"]
                .as_str()
                .unwrap_or("请检查模型与本机内存")
        )));
    }
    if body["choices"][0]["finish_reason"] != "stop" {
        return Err(failure("本地模型未完成识别，请缩小截图范围后重试"));
    }
    let text = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| failure("本地模型返回格式无效"))?;
    Ok(json!({"text": text, "model": MODEL_ID}))
}

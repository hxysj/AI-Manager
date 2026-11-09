use crate::core::{error::ManagerError, paths::AppPaths};
use futures_util::StreamExt;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, RANGE, USER_AGENT};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{watch, Mutex},
};

mod runtime;

const MODEL_ID: &str = "minicpm-ocr-1.0";
const REVISION: &str = "857af228a0b3cc79588bb90635bb6c00dd9542da";
// 大小和 SHA-256 来自 OpenBMB 官方仓库，语言权重与视觉模块必须配套安装。
const FILES: [ModelFile; 2] = [
    ModelFile {
        name: "MiniCPM-OCR-1_0-Q4_K_M.gguf",
        bytes: 529_106_816,
        sha256: "c80a703400633f298cc2fddf6d493a541be36918c0801c0b1bf3f14c5703ce49",
    },
    ModelFile {
        name: "mmproj-model-f16-4x.gguf",
        bytes: 901_017_536,
        sha256: "ceed82261b3b1380c3165f8b37c10117a50093be1362cbfeeb692fa036444682",
    },
];

struct ModelFile {
    name: &'static str,
    bytes: u64,
    sha256: &'static str,
}

struct DownloadTask {
    cancel: watch::Sender<bool>,
    phase: String,
    downloaded: u64,
    file: String,
    error: String,
}

#[derive(Clone, Default)]
pub struct LocalModelService {
    tasks: Arc<Mutex<HashMap<PathBuf, DownloadTask>>>,
    inference: Arc<Mutex<()>>,
}

fn failure(message: impl ToString) -> ManagerError {
    ManagerError::System(message.to_string())
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

// 路径只由内置清单生成；拒绝穿过符号链接或 Windows 目录联接。
fn model_directory(paths: &AppPaths) -> Result<PathBuf, ManagerError> {
    let mut directory = PathBuf::from(&paths.workspace_root);
    for part in ["local-ai", "models", MODEL_ID] {
        directory.push(part);
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => {
                return Err(failure("模型目录不能是链接或普通文件"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(directory)
}

fn file_size(path: &Path) -> Result<u64, ManagerError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if is_link(&metadata) || !metadata.is_file() => {
            Err(failure("模型文件不能是链接或目录"))
        }
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn directory_size(directory: &Path) -> Result<u64, ManagerError> {
    if !directory.exists() {
        return Ok(0);
    }
    let mut bytes = 0;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if is_link(&metadata) {
            return Err(failure("模型目录中存在链接，请先移除链接"));
        }
        bytes += if metadata.is_dir() {
            directory_size(&entry.path())?
        } else {
            metadata.len()
        };
    }
    Ok(bytes)
}

fn file_stamp(path: &Path) -> Result<String, ManagerError> {
    file_size(path)?;
    Ok(std::fs::metadata(path)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(failure)?
        .as_nanos()
        .to_string())
}

fn active(phase: &str) -> bool {
    matches!(phase, "downloading" | "verifying" | "cancelling")
}

impl LocalModelService {
    pub async fn dispatch(
        &self,
        app: &AppHandle,
        paths: &AppPaths,
        action: &str,
        payload: Value,
    ) -> Result<Value, ManagerError> {
        if action == "list" {
            return self.list(paths).await;
        }
        if action == "installed" {
            return self
                .installed(paths, payload["capability"].as_str().unwrap_or(""))
                .await;
        }
        if action == "generate" {
            return self.generate(paths, payload).await;
        }
        if payload["id"].as_str() != Some(MODEL_ID) {
            return Err(failure("不支持的本地模型"));
        }
        let directory = model_directory(paths)?;
        match action {
            "download" => {
                let source = payload["source"].as_str().unwrap_or("modelscope");
                let base = match source {
                    "modelscope" => format!(
                        "https://modelscope.cn/models/OpenBMB/MiniCPM-OCR-1.0-GGUF/resolve/{REVISION}"
                    ),
                    "huggingface" => {
                        "https://huggingface.co/openbmb/MiniCPM-OCR-1.0-gguf/resolve/main".into()
                    }
                    _ => return Err(failure("不支持的下载源")),
                };
                let mut tasks = self.tasks.lock().await;
                if tasks
                    .get(&directory)
                    .is_some_and(|task| active(&task.phase))
                {
                    return Err(failure("该模型正在下载或校验"));
                }
                let (cancel, mut cancelled) = watch::channel(false);
                tasks.insert(
                    directory.clone(),
                    DownloadTask {
                        cancel,
                        phase: "downloading".into(),
                        downloaded: 0,
                        file: String::new(),
                        error: String::new(),
                    },
                );
                drop(tasks);
                let service = self.clone();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = {
                        let transfer = service.download(&directory, &base, &app);
                        let stop = cancelled.changed();
                        futures_util::pin_mut!(transfer, stop);
                        match futures_util::future::select(transfer, stop).await {
                            futures_util::future::Either::Left((result, _)) => result,
                            futures_util::future::Either::Right(_) => Err(failure("下载已暂停")),
                        }
                    };
                    // 取消时丢弃网络流和文件句柄，保留 .part 文件供下次续传。
                    let mut tasks = service.tasks.lock().await;
                    if let Some(task) = tasks.get_mut(&directory) {
                        task.phase = if *cancelled.borrow() {
                            "paused"
                        } else if result.is_ok() {
                            "downloaded"
                        } else {
                            "error"
                        }
                        .into();
                        task.error = if *cancelled.borrow() {
                            String::new()
                        } else {
                            result
                                .err()
                                .map(|error| error.to_string())
                                .unwrap_or_default()
                        };
                    }
                    drop(tasks);
                    let _ = app.emit("local-models:changed", ());
                });
            }
            "cancel" => {
                let mut tasks = self.tasks.lock().await;
                if let Some(task) = tasks.get_mut(&directory).filter(|task| active(&task.phase)) {
                    task.phase = "cancelling".into();
                    let _ = task.cancel.send(true);
                }
            }
            "delete" => {
                self.delete(&directory).await?;
            }
            _ => {
                return Err(ManagerError::UnknownChannel(format!(
                    "local-models:{action}"
                )))
            }
        }
        let _ = app.emit("local-models:changed", ());
        Ok(json!({ "success": true }))
    }

    async fn delete(&self, directory: &Path) -> Result<(), ManagerError> {
        let _inference = self.inference.lock().await;
        // 与开始下载共用同一把锁，避免删除和下载在多窗口间交错。
        let mut tasks = self.tasks.lock().await;
        if tasks.get(directory).is_some_and(|task| active(&task.phase)) {
            return Err(failure("请先暂停下载，待任务结束后再删除"));
        }
        if directory.exists() {
            directory_size(directory)?;
            let root = directory.parent().ok_or_else(|| failure("模型路径非法"))?;
            if !std::fs::canonicalize(directory)?.starts_with(std::fs::canonicalize(root)?) {
                return Err(failure("模型路径超出存储目录"));
            }
            tokio::fs::remove_dir_all(directory).await?;
        }
        tasks.remove(directory);
        Ok(())
    }

    pub async fn list(&self, paths: &AppPaths) -> Result<Value, ManagerError> {
        let directory = model_directory(paths)?;
        let tasks = self.tasks.lock().await;
        let task = tasks.get(&directory);
        let receipt_path = directory.join("installed.json");
        let receipt = if file_size(&receipt_path)? > 0 {
            serde_json::from_slice::<Value>(&std::fs::read(&receipt_path)?).unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let mut complete = receipt["revision"] == REVISION;
        let mut downloaded = 0;
        let mut partial_bytes = 0;
        let mut files = Vec::new();
        for file in &FILES {
            let target = directory.join(file.name);
            let bytes = file_size(&target)?;
            let cached = file_size(&target.with_extension("gguf.part"))?;
            let verified = bytes == file.bytes
                && receipt["files"][file.name]["sha256"] == file.sha256
                && receipt["files"][file.name]["modified"] == file_stamp(&target)?;
            complete &= verified;
            downloaded += bytes.min(file.bytes) + cached.min(file.bytes - bytes.min(file.bytes));
            partial_bytes += cached;
            files.push(json!({
                "name": file.name,
                "bytes": file.bytes,
                "localBytes": bytes,
                "cachedBytes": cached,
                "verified": verified,
            }));
        }
        let used_bytes = directory_size(&directory)?;
        let phase = match task {
            Some(task) if active(&task.phase) => task.phase.as_str(),
            _ if complete => "downloaded",
            Some(task) if task.phase == "error" => "error",
            _ if used_bytes > 0 => "paused",
            _ => "not-downloaded",
        };
        let total: u64 = FILES.iter().map(|file| file.bytes).sum();
        let transferred = task
            .filter(|task| active(&task.phase))
            .map(|task| task.downloaded)
            .unwrap_or(downloaded);
        Ok(json!({
            "storagePath": directory.parent(),
            "usedBytes": used_bytes,
            "partialBytes": partial_bytes,
            "installedCount": usize::from(complete),
            "models": [{
                "id": MODEL_ID,
                "name": "MiniCPM-OCR 1.0",
                "publisher": "OpenBMB / 面壁智能",
                "description": "中英文文档识别，支持文字、表格、公式和图表。",
                "format": "GGUF · Q4_K_M",
                "license": "Apache-2.0",
                "capabilities": ["ocr"],
                "downloadBytes": total,
                "usedBytes": used_bytes,
                "downloadedBytes": transferred,
                "percent": (transferred as f64 * 100.0 / total as f64).min(100.0),
                "phase": phase,
                "currentFile": task.map(|task| task.file.as_str()).unwrap_or(""),
                "error": task.filter(|task| task.phase == "error")
                    .map(|task| task.error.as_str()).unwrap_or(""),
                "path": directory,
                "files": files,
            }]
        }))
    }

    pub async fn installed(
        &self,
        paths: &AppPaths,
        capability: &str,
    ) -> Result<Value, ManagerError> {
        let snapshot = self.list(paths).await?;
        let models: Vec<Value> = snapshot["models"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|model| {
                model["phase"] == "downloaded"
                    && (capability.is_empty()
                        || model["capabilities"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|value| value == capability))
            })
            .map(|model| {
                json!({
                    "id": model["id"], "name": model["name"], "capabilities": model["capabilities"]
                })
            })
            .collect();
        Ok(json!({ "models": models }))
    }

    pub async fn generate(&self, paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
        if payload["id"].as_str() != Some(MODEL_ID) {
            return Err(failure("不支持的本地模型"));
        }
        if payload["prompt"]
            .as_str()
            .is_none_or(|prompt| prompt.trim().is_empty() || prompt.len() > 32_768)
            || payload["image"].as_str().is_none_or(|image| {
                !image.starts_with("data:image/") || image.len() > 64 * 1024 * 1024
            })
        {
            return Err(failure("本地 OCR 模型需要有效的识别提示和图片"));
        }
        // 下载、删除和识别共享文件访问锁；仅在调用时运行，完成或取消后释放模型。
        let _inference = self.inference.lock().await;
        let snapshot = self.list(paths).await?;
        if snapshot["models"][0]["phase"] != "downloaded" {
            return Err(failure("所选本地模型尚未完整下载或已被删除"));
        }
        runtime::generate(paths, &model_directory(paths)?, payload).await
    }

    async fn progress(
        &self,
        directory: &Path,
        app: &AppHandle,
        phase: &str,
        downloaded: u64,
        file: &str,
    ) {
        if let Some(task) = self.tasks.lock().await.get_mut(directory) {
            if task.phase != "cancelling" {
                task.phase = phase.into();
            }
            task.downloaded = downloaded;
            task.file = file.into();
        }
        let _ = app.emit("local-models:changed", ());
    }

    async fn download(
        &self,
        directory: &Path,
        base: &str,
        app: &AppHandle,
    ) -> Result<(), ManagerError> {
        let _inference = self.inference.lock().await;
        tokio::fs::create_dir_all(directory).await?;
        let client = reqwest::Client::builder().build().map_err(failure)?;
        let mut verified_bytes = 0;
        let mut receipt = json!({ "revision": REVISION, "files": {} });
        for file in &FILES {
            let target = directory.join(file.name);
            let partial = target.with_extension("gguf.part");
            let existing = file_size(&target)?;
            if existing != file.bytes {
                if existing > 0 {
                    tokio::fs::remove_file(&target).await?;
                }
                self.progress(directory, app, "downloading", verified_bytes, file.name)
                    .await;
                download_file(
                    &client,
                    &format!("{base}/{}", file.name),
                    &partial,
                    file,
                    |bytes| {
                        self.progress(
                            directory,
                            app,
                            "downloading",
                            verified_bytes + bytes,
                            file.name,
                        )
                    },
                )
                .await?;
            }
            self.progress(
                directory,
                app,
                "verifying",
                verified_bytes + file.bytes,
                file.name,
            )
            .await;
            let candidate = if existing == file.bytes {
                &target
            } else {
                &partial
            };
            if !verify_file(candidate, file).await? {
                tokio::fs::remove_file(candidate).await?;
                return Err(failure(format!("{} 校验失败，请重新下载", file.name)));
            }
            if candidate == &partial {
                tokio::fs::rename(&partial, &target).await?;
            } else if file_size(&partial)? > 0 {
                tokio::fs::remove_file(&partial).await?;
            }
            verified_bytes += file.bytes;
            receipt["files"][file.name] = json!({
                "sha256": file.sha256,
                "modified": file_stamp(&target)?,
            });
        }
        let receipt_path = directory.join("installed.json");
        file_size(&receipt_path)?;
        tokio::fs::write(receipt_path, serde_json::to_vec(&receipt)?).await?;
        Ok(())
    }
}

async fn verify_file(path: &Path, file: &ModelFile) -> Result<bool, ManagerError> {
    if file_size(path)? != file.bytes {
        return Ok(false);
    }
    let mut reader = tokio::fs::File::open(path).await?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()) == file.sha256)
}

async fn download_file<F, Fut>(
    client: &reqwest::Client,
    url: &str,
    partial: &Path,
    file: &ModelFile,
    mut progress: F,
) -> Result<(), ManagerError>
where
    F: FnMut(u64) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut offset = file_size(partial)?;
    if offset > file.bytes {
        tokio::fs::remove_file(partial).await?;
        offset = 0;
    }
    progress(offset).await;
    if offset == file.bytes {
        return Ok(());
    }
    // ModelScope CDN 会拒绝没有 User-Agent 的请求，所有下载源统一携带应用标识。
    let mut request = client.get(url).header(ACCEPT_ENCODING, "identity").header(
        USER_AGENT,
        concat!("AI-Manager/", env!("CARGO_PKG_VERSION")),
    );
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|error| failure(format!("连接模型下载源失败：{}", error.without_url())))?;
    if !response.status().is_success() {
        return Err(failure(match response.status().as_u16() {
            403 => "下载源拒绝请求（HTTP 403），请重试或切换下载源".into(),
            429 => "下载源请求过于频繁（HTTP 429），请稍后重试或切换下载源".into(),
            status => format!("模型下载失败（HTTP {status}），请重试或切换下载源"),
        }));
    }
    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let expected = format!("bytes {offset}-{}/{}", file.bytes - 1, file.bytes);
        if response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            != Some(expected.as_str())
        {
            return Err(failure("下载源返回了不匹配的续传范围"));
        }
    } else if response.status() == reqwest::StatusCode::OK {
        offset = 0;
    } else {
        return Err(failure("下载源未返回模型文件"));
    }
    if response
        .content_length()
        .is_some_and(|bytes| bytes != file.bytes - offset)
    {
        return Err(failure("下载源返回的文件大小与模型清单不一致"));
    }
    let mut output = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(offset == 0)
        .append(offset > 0)
        .open(partial)
        .await?;
    let mut stream = response.bytes_stream();
    let mut last_progress = Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| failure(format!("模型传输中断：{}", error.without_url())))?;
        if offset + chunk.len() as u64 > file.bytes {
            return Err(failure("下载内容超出模型文件大小"));
        }
        output.write_all(&chunk).await?;
        offset += chunk.len() as u64;
        if last_progress.elapsed() >= Duration::from_millis(300) {
            progress(offset).await;
            last_progress = Instant::now();
        }
    }
    output.flush().await?;
    if offset != file.bytes {
        return Err(failure("模型文件下载中断，可点击继续下载"));
    }
    progress(offset).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("local-models-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const TEST_FILE: ModelFile = ModelFile {
        name: "test.gguf",
        bytes: 3,
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    };

    #[test]
    fn model_selection_requires_verified_download_and_matching_capability() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        tauri::async_runtime::block_on(async {
            let service = LocalModelService::default();
            assert_eq!(
                service.installed(&paths, "ocr").await.unwrap()["models"],
                json!([])
            );
            let directory = model_directory(&paths).unwrap();
            std::fs::create_dir_all(&directory).unwrap();
            let mut receipt = json!({"revision": REVISION, "files": {}});
            // 稀疏文件加校验凭据模拟完整安装；读取列表不重新计算大文件哈希。
            for file in &FILES {
                let target = directory.join(file.name);
                std::fs::File::create(&target)
                    .unwrap()
                    .set_len(file.bytes)
                    .unwrap();
                receipt["files"][file.name] = json!({
                    "sha256": file.sha256, "modified": file_stamp(&target).unwrap()
                });
            }
            std::fs::write(directory.join("installed.json"), receipt.to_string()).unwrap();
            let installed = service.installed(&paths, "ocr").await.unwrap();
            assert_eq!(installed["models"][0]["id"], MODEL_ID);
            assert_eq!(installed["models"][0]["capabilities"], json!(["ocr"]));
            assert_eq!(
                service.installed(&paths, "chat").await.unwrap()["models"],
                json!([])
            );
            service.delete(&directory).await.unwrap();
            assert_eq!(
                service.installed(&paths, "ocr").await.unwrap()["models"],
                json!([])
            );
            let error = service
                .generate(
                    &paths,
                    json!({
                        "id": MODEL_ID, "prompt": "识别图片", "image": "data:image/png;base64,test"
                    }),
                )
                .await
                .unwrap_err();
            assert!(error.to_string().contains("尚未完整下载"));
        });
    }

    #[test]
    #[ignore = "使用显式指定的本地权重和图片验证真实推理"]
    fn downloaded_model_recognizes_image() {
        use base64::Engine;
        let paths = crate::core::paths::resolve_app_paths(&PathBuf::from(
            std::env::var("AI_MANAGER_MODEL_TEST_DATA").expect("请指定模型数据目录"),
        ));
        let image =
            std::fs::read(std::env::var("AI_MANAGER_MODEL_TEST_IMAGE").expect("请指定测试图片"))
                .unwrap();
        let result = tauri::async_runtime::block_on(
            LocalModelService::default().generate(&paths, json!({
                "id": MODEL_ID,
                "prompt": "识别图片中的全部可见文字，按阅读顺序保留换行、标点及表格结构，只输出原文。",
                "image": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(image))
            }))
        ).unwrap();
        let text = result["text"].as_str().unwrap();
        println!("识别结果：{text}");
        assert!(text.contains("Hello"));
        assert!(text.contains("12345"));
    }

    fn serve(
        response: &'static str,
        expected_range: Option<&'static str>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                let bytes = socket.read(&mut buffer).unwrap();
                assert!(bytes > 0);
                request.extend_from_slice(&buffer[..bytes]);
            }
            let headers = String::from_utf8(request).unwrap().to_lowercase();
            assert!(
                headers.contains("\r\nuser-agent: ai-manager/"),
                "模型下载必须携带应用标识，避免被 CDN 拒绝"
            );
            if let Some(range) = expected_range {
                assert!(headers.contains(range));
            } else {
                assert!(!headers.contains("\r\nrange:"));
            }
            socket.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}/model.gguf"), thread)
    }

    #[test]
    fn resumes_partial_and_verifies_hash() {
        let root = Fixture::new();
        let partial = root.0.join("test.gguf.part");
        std::fs::write(&partial, b"a").unwrap();
        let (url, server) = serve(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: 2\r\nContent-Range: bytes 1-2/3\r\nConnection: close\r\n\r\nbc",
            Some("range: bytes=1-"),
        );
        tauri::async_runtime::block_on(async {
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            download_file(&client, &url, &partial, &TEST_FILE, |_| async {})
                .await
                .unwrap();
            assert!(verify_file(&partial, &TEST_FILE).await.unwrap());
            std::fs::write(&partial, b"abd").unwrap();
            assert!(!verify_file(&partial, &TEST_FILE).await.unwrap());
        });
        server.join().unwrap();
    }

    #[test]
    fn restarts_when_server_ignores_range() {
        let root = Fixture::new();
        let partial = root.0.join("test.gguf.part");
        std::fs::write(&partial, b"a").unwrap();
        let (url, server) = serve(
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
            Some("range: bytes=1-"),
        );
        tauri::async_runtime::block_on(async {
            download_file(
                &reqwest::Client::new(),
                &url,
                &partial,
                &TEST_FILE,
                |_| async {},
            )
            .await
            .unwrap();
        });
        server.join().unwrap();
        assert_eq!(std::fs::read(partial).unwrap(), b"abc");
    }

    #[test]
    fn refuses_wrong_range_size_and_http_errors() {
        for response in [
            "HTTP/1.1 206 Partial Content\r\nContent-Length: 2\r\nContent-Range: bytes 0-1/3\r\nConnection: close\r\n\r\nab",
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nab",
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ] {
            let root = Fixture::new();
            let partial = root.0.join("test.gguf.part");
            std::fs::write(&partial, b"a").unwrap();
            let (url, server) = serve(response, Some("range: bytes=1-"));
            tauri::async_runtime::block_on(async {
                let error = download_file(
                    &reqwest::Client::new(), &url, &partial, &TEST_FILE, |_| async {},
                ).await.unwrap_err().to_string();
                assert!(!error.contains(&url), "错误提示不能暴露临时下载地址");
                if response.contains("403") {
                    assert!(error.contains("HTTP 403"));
                    assert!(error.contains("切换下载源"));
                }
            });
            server.join().unwrap();
            assert_eq!(std::fs::read(partial).unwrap(), b"a");
        }
    }

    #[test]
    fn refuses_html_and_truncated_content() {
        let root = Fixture::new();
        let partial = root.0.join("test.gguf.part");
        let (url, server) = serve(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n<html>error</html>",
            None,
        );
        tauri::async_runtime::block_on(async {
            assert!(download_file(
                &reqwest::Client::new(),
                &url,
                &partial,
                &TEST_FILE,
                |_| async {}
            )
            .await
            .is_err());
        });
        server.join().unwrap();
        let (url, server) = serve("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\na", None);
        tauri::async_runtime::block_on(async {
            assert!(download_file(
                &reqwest::Client::new(),
                &url,
                &partial,
                &TEST_FILE,
                |_| async {}
            )
            .await
            .is_err());
        });
        server.join().unwrap();
        assert_eq!(std::fs::read(partial).unwrap(), b"a");
    }

    #[test]
    fn disk_statistics_include_partial_and_untracked_files() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        let directory = model_directory(&paths).unwrap();
        tauri::async_runtime::block_on(async {
            let service = LocalModelService::default();
            let before = service.list(&paths).await.unwrap();
            assert_eq!(before["usedBytes"], 0);
            assert_eq!(before["models"][0]["phase"], "not-downloaded");
            std::fs::create_dir_all(directory.join("extra")).unwrap();
            std::fs::write(
                directory.join(FILES[0].name).with_extension("gguf.part"),
                b"12345",
            )
            .unwrap();
            std::fs::write(directory.join("extra/readme"), b"123").unwrap();
            let after = service.list(&paths).await.unwrap();
            assert_eq!(after["usedBytes"], 8);
            assert_eq!(after["partialBytes"], 5);
            assert_eq!(after["models"][0]["phase"], "paused");
            assert_eq!(after["installedCount"], 0);
            // 没有匹配文件和校验记录时，不能仅凭“任务已完成”显示已下载。
            let (cancel, _) = watch::channel(false);
            service.tasks.lock().await.insert(
                directory,
                DownloadTask {
                    cancel,
                    phase: "downloaded".into(),
                    downloaded: 999,
                    file: "".into(),
                    error: "".into(),
                },
            );
            assert_eq!(service.list(&paths).await.unwrap()["installedCount"], 0);
        });
    }

    #[test]
    fn deletion_blocks_active_tasks_and_removes_only_model_files() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        let directory = model_directory(&paths).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        let sentinel = directory.parent().unwrap().join("other-model.txt");
        std::fs::write(&sentinel, b"keep").unwrap();
        std::fs::write(directory.join("test.gguf.part"), b"123").unwrap();
        tauri::async_runtime::block_on(async {
            let service = LocalModelService::default();
            let (cancel, _) = watch::channel(false);
            service.tasks.lock().await.insert(
                directory.clone(),
                DownloadTask {
                    cancel,
                    phase: "verifying".into(),
                    downloaded: 0,
                    file: "".into(),
                    error: "".into(),
                },
            );
            assert!(service.delete(&directory).await.is_err());
            assert!(directory.exists());
            service
                .tasks
                .lock()
                .await
                .get_mut(&directory)
                .unwrap()
                .phase = "paused".into();
            service.delete(&directory).await.unwrap();
            assert!(!directory.exists());
            assert!(sentinel.exists());
            assert_eq!(service.list(&paths).await.unwrap()["usedBytes"], 0);
            service.delete(&directory).await.unwrap();
        });
    }

    #[test]
    fn rejects_model_directory_links() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        let directory = PathBuf::from(&paths.workspace_root)
            .join("local-ai")
            .join("models");
        std::fs::create_dir_all(&directory).unwrap();
        let destination = root.0.join("outside");
        std::fs::create_dir(&destination).unwrap();
        #[cfg(windows)]
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(directory.join(MODEL_ID))
            .arg(&destination)
            .output()
            .unwrap();
        #[cfg(windows)]
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        #[cfg(not(windows))]
        let linked = std::os::unix::fs::symlink(&destination, directory.join(MODEL_ID)).is_ok();
        #[cfg(not(windows))]
        assert!(linked);
        assert!(model_directory(&paths).is_err());
        #[cfg(windows)]
        std::fs::remove_dir(directory.join(MODEL_ID)).unwrap();
        #[cfg(not(windows))]
        std::fs::remove_file(directory.join(MODEL_ID)).unwrap();
        assert!(destination.exists());
    }
}

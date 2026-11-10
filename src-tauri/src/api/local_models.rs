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

mod catalog;
mod translation;
pub(crate) use translation::{
    check_languages as check_translation_languages, plan as translation_plan,
};
mod requirements;
use catalog::{ModelDefinition, MODELS};
mod runtime;

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
fn model_directory(paths: &AppPaths, model: &ModelDefinition) -> Result<PathBuf, ManagerError> {
    let mut directory = PathBuf::from(&paths.workspace_root);
    for part in ["local-ai", "models", model.id] {
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

fn runtime_validation(directory: &Path, model: &ModelDefinition) -> Result<Value, ManagerError> {
    // 校验凭据留在模型目录之外，删除权重后仍可判断本机是否曾运行过同一版本。
    let path = directory.with_file_name(format!("{}.runtime-check.json", model.id));
    let bytes = file_size(&path)?;
    let record = if bytes > 0 && bytes <= 64 * 1024 {
        serde_json::from_slice::<Value>(&std::fs::read(path)?).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let valid = record["revision"] == model.revision
        && record["profile"] == runtime::validation_profile(model)
        && record["passed"].is_boolean()
        // 运行能力绑定预期权重版本；重新下载与续传时，文件完整性仍由安装校验负责。
        && model.requirements.files
            .iter()
            .all(|file| record["files"][file.name]["sha256"] == file.sha256);
    if valid {
        return Ok(json!({
            "passed": record["passed"], "checkedAt": record["checkedAt"],
            "detail": if record["passed"] == true {
                format!("本机模型加载和基础{}实测通过", if model.capability == "ocr" { " OCR " } else { "翻译" })
            } else {
                format!("本机运行校验未通过：{}", record["error"].as_str().unwrap_or("请重新校验"))
            },
        }));
    }
    Ok(json!({
        "passed": null,
        "detail": if bytes > 0 {
            "旧校验结果已失效，请重新运行校验"
        } else {
            "尚未进行本机运行校验，可在下载完成后校验"
        },
    }))
}

fn model_snapshot(
    paths: &AppPaths,
    model: &ModelDefinition,
    task: Option<&DownloadTask>,
) -> Result<Value, ManagerError> {
    let directory = model_directory(paths, model)?;
    let receipt_path = directory.join("installed.json");
    let receipt = if file_size(&receipt_path)? > 0 {
        serde_json::from_slice::<Value>(&std::fs::read(&receipt_path)?).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let mut complete = receipt["revision"] == model.revision;
    let mut downloaded = 0;
    let mut partial_bytes = 0;
    let mut files = Vec::new();
    for file in model.requirements.files {
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
    let total: u64 = model.requirements.files.iter().map(|file| file.bytes).sum();
    let transferred = task
        .filter(|task| active(&task.phase))
        .map(|task| task.downloaded)
        .unwrap_or(downloaded);

    Ok(json!({
        "id": model.id, "name": model.name, "publisher": model.publisher,
        "description": model.description, "format": "GGUF · Q4_K_M", "license": "Apache-2.0",
        "capabilities": [model.capability], "downloadBytes": total,
        "usedBytes": used_bytes, "partialBytes": partial_bytes,
        "downloadedBytes": transferred,
        "percent": (transferred as f64 * 100.0 / total as f64).min(100.0),
        "phase": phase, "path": directory, "files": files,
        "currentFile": task.map(|task| task.file.as_str()).unwrap_or(""),
        "error": task.filter(|task| task.phase == "error").map(|task| task.error.as_str()).unwrap_or(""),
        "sources": model.sources.iter().map(|source| json!({"id": source.id, "name": source.name})).collect::<Vec<_>>(),
        "compatibility": requirements::inspect(paths, &directory, model, true)?,
        "languages": if model.capability == "translation" { translation::languages() } else { json!([]) },
    }))
}

// 保存配置也检查完整安装和用途，避免已删除模型或 OCR 模型进入翻译调用。
pub(crate) fn translation_target_info(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let model = catalog::find(id)?;
    if model.capability != "translation" {
        return Err(failure("所选本地模型不支持翻译"));
    }
    if model_snapshot(paths, model, None)?["phase"] != "downloaded" {
        return Err(failure("所选本地翻译模型尚未完整下载或已被删除"));
    }
    Ok(
        json!({"targetId": "local", "providerName": "本地模型", "model": model.id, "engine": "local"}),
    )
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
        let model = catalog::find(payload["id"].as_str().unwrap_or(""))?;
        let directory = model_directory(paths, model)?;
        match action {
            "validate" => {
                let result = self.validate(paths, model).await;
                let _ = app.emit("local-models:changed", ());
                return result;
            }
            "download" => {
                let source = payload["source"].as_str().unwrap_or(model.sources[0].id);
                let base = model
                    .sources
                    .iter()
                    .find(|item| item.id == source)
                    .ok_or_else(|| failure("不支持的下载源"))?
                    .base;
                let mut tasks = self.tasks.lock().await;
                if tasks
                    .get(&directory)
                    .is_some_and(|task| active(&task.phase))
                {
                    return Err(failure("该模型正在下载或校验"));
                }
                requirements::inspect(paths, &directory, model, true)?.ensure_allowed()?;
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
                let paths = paths.clone();
                tauri::async_runtime::spawn(async move {
                    let result = {
                        let transfer =
                            service.download(&paths, &directory, base, Some(&app), model);
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
        let tasks = self.tasks.lock().await;
        let mut models = Vec::new();
        for model in MODELS {
            let directory = model_directory(paths, model)?;
            models.push(model_snapshot(paths, model, tasks.get(&directory))?);
        }
        Ok(json!({
            "storagePath": PathBuf::from(&paths.workspace_root).join("local-ai/models"),
            "usedBytes": models.iter().map(|model| model["usedBytes"].as_u64().unwrap_or(0)).sum::<u64>(),
            "partialBytes": models.iter().map(|model| model["partialBytes"].as_u64().unwrap_or(0)).sum::<u64>(),
            "installedCount": models.iter().filter(|model| model["phase"] == "downloaded").count(),
            "models": models,
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
                    "id": model["id"], "name": model["name"], "capabilities": model["capabilities"],
                    "languages": model["languages"]
                })
            })
            .collect();
        Ok(json!({ "models": models }))
    }

    pub async fn generate(&self, paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
        let model = catalog::find(payload["id"].as_str().unwrap_or(""))?;
        if model.capability != "ocr" {
            return Err(failure("所选本地模型不支持 OCR 识别"));
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
        if !snapshot["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == model.id && item["phase"] == "downloaded")
        {
            return Err(failure("所选本地模型尚未完整下载或已被删除"));
        }
        // 调用时重新检查可用内存；已安装文件不再占用下载空间预算。
        requirements::inspect(paths, &model_directory(paths, model)?, model, false)?
            .ensure_runtime_prerequisites()?;
        runtime::generate(paths, &model_directory(paths, model)?, model, payload).await
    }

    pub(crate) async fn translate(
        &self,
        paths: &AppPaths,
        payload: Value,
    ) -> Result<Value, ManagerError> {
        let model = catalog::find(payload["id"].as_str().unwrap_or(""))?;
        if model.capability != "translation" {
            return Err(failure("所选本地模型不支持翻译"));
        }
        let text = payload["text"].as_str().unwrap_or("");
        if text.trim().is_empty() || text.chars().count() > 30_000 {
            return Err(failure("请输入有效文本，单次最多翻译 30000 个字符"));
        }
        let prompt = translation::prompt(
            text,
            payload["sourceLanguage"].as_str().unwrap_or(""),
            payload["targetLanguage"].as_str().unwrap_or(""),
        )?;
        let _inference = self.inference.lock().await;
        translation_target_info(paths, model.id)?;
        let directory = model_directory(paths, model)?;
        requirements::inspect(paths, &directory, model, false)?.ensure_runtime_prerequisites()?;
        runtime::generate(paths, &directory, model, json!({"prompt": prompt})).await
    }

    async fn validate(
        &self,
        paths: &AppPaths,
        model: &ModelDefinition,
    ) -> Result<Value, ManagerError> {
        let _inference = self
            .inference
            .try_lock()
            .map_err(|_| failure("模型正在使用，请稍后运行校验"))?;
        let directory = model_directory(paths, model)?;
        let snapshot = self.list(paths).await?;
        if !snapshot["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == model.id && item["phase"] == "downloaded")
        {
            return Err(failure("模型尚未完整下载，无法执行本机运行校验"));
        }
        requirements::inspect(paths, &directory, model, false)?.ensure_runtime_prerequisites()?;
        let result = runtime::validate(paths, &directory, model).await;
        let receipt: Value =
            serde_json::from_slice(&std::fs::read(directory.join("installed.json"))?)?;
        let record = json!({
            "revision": model.revision, "profile": runtime::validation_profile(model),
            "files": receipt["files"], "passed": result.is_ok(),
            "checkedAt": chrono::Utc::now().timestamp_millis(),
            "error": result.err().map(|error| error.to_string()).unwrap_or_default(),
        });
        let path = directory.with_file_name(format!("{}.runtime-check.json", model.id));
        file_size(&path)?;
        tokio::fs::write(&path, serde_json::to_vec(&record)?).await?;
        runtime_validation(&directory, model)
    }

    async fn progress(
        &self,
        directory: &Path,
        app: Option<&AppHandle>,
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
        if let Some(app) = app {
            let _ = app.emit("local-models:changed", ());
        }
    }

    async fn download(
        &self,
        paths: &AppPaths,
        directory: &Path,
        base: &str,
        app: Option<&AppHandle>,
        model: &ModelDefinition,
    ) -> Result<(), ManagerError> {
        let _inference = self.inference.lock().await;
        // 等待其他识别任务结束后，按该模型的要求重新检测，再创建目录和发起网络请求。
        requirements::inspect(paths, directory, model, true)?.ensure_allowed()?;
        tokio::fs::create_dir_all(directory).await?;
        let client = reqwest::Client::builder().build().map_err(failure)?;
        let mut verified_bytes = 0;
        let mut receipt = json!({ "revision": model.revision, "files": {} });
        for file in model.requirements.files {
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
pub(crate) mod tests {
    use super::*;
    const MODEL_ID: &str = catalog::OCR.id;
    const REVISION: &str = catalog::OCR.revision;
    pub(super) const FILES: &[ModelFile] = catalog::OCR.requirements.files;
    pub(super) const MODEL_REQUIREMENTS: requirements::ModelRequirements =
        catalog::OCR.requirements;
    use std::io::{Read, Write};

    pub(crate) struct Fixture(pub(crate) PathBuf);

    impl Fixture {
        pub(crate) fn new() -> Self {
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

    pub(crate) fn install_model(paths: &AppPaths, id: &str) -> PathBuf {
        let model = catalog::find(id).unwrap();
        install_definition(paths, model)
    }

    pub(crate) async fn prepare_real_model(
        paths: &AppPaths,
        service: &LocalModelService,
        id: &str,
    ) -> Result<(), ManagerError> {
        let model = catalog::find(id)?;
        let directory = model_directory(paths, model)?;
        if model_snapshot(paths, model, None)?["phase"] != "downloaded" {
            service
                .download(paths, &directory, model.sources[0].base, None, model)
                .await?;
        }
        let validation = service.validate(paths, model).await?;
        if validation["passed"] != true {
            return Err(failure(
                validation["detail"].as_str().unwrap_or("运行校验失败"),
            ));
        }
        Ok(())
    }

    fn install_definition(paths: &AppPaths, model: &ModelDefinition) -> PathBuf {
        let directory = model_directory(paths, model).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        let mut receipt = json!({"revision": model.revision, "files": {}});
        // 仅模拟安装元数据，真实下载测试仍必须校验完整文件哈希。
        for file in model.requirements.files {
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
        directory
    }

    #[test]
    fn models_with_same_capability_keep_their_own_files_and_installation_state() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        for original in [&catalog::OCR, &catalog::LMT] {
            let second = ModelDefinition {
                id: if original.capability == "ocr" {
                    "test-ocr-secondary"
                } else {
                    "test-translation-secondary"
                },
                ..if original.capability == "ocr" {
                    catalog::OCR
                } else {
                    catalog::LMT
                }
            };
            let original_directory = install_definition(&paths, original);
            let second_directory = install_definition(&paths, &second);
            assert_ne!(original_directory, second_directory);
            assert_eq!(
                model_snapshot(&paths, original, None).unwrap()["phase"],
                "downloaded"
            );
            assert_eq!(
                model_snapshot(&paths, &second, None).unwrap()["phase"],
                "downloaded"
            );
            tauri::async_runtime::block_on(LocalModelService::default().delete(&second_directory))
                .unwrap();
            assert_eq!(
                model_snapshot(&paths, original, None).unwrap()["phase"],
                "downloaded"
            );
            assert_eq!(
                model_snapshot(&paths, &second, None).unwrap()["phase"],
                "not-downloaded"
            );
        }
    }

    #[test]
    fn two_models_keep_installation_capabilities_sources_and_deletion_separate() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        tauri::async_runtime::block_on(async {
            let service = LocalModelService::default();
            let before = service.list(&paths).await.unwrap();
            assert_eq!(before["models"].as_array().unwrap().len(), 2);
            assert_eq!(before["models"][0]["capabilities"], json!(["ocr"]));
            assert_eq!(before["models"][1]["capabilities"], json!(["translation"]));
            assert_eq!(before["models"][1]["sources"][0]["id"], "hf-mirror");
            assert_eq!(before["models"][1]["downloadBytes"], 484_220_000);
            assert_eq!(before["models"][1]["files"].as_array().unwrap().len(), 1);
            assert_eq!(
                before["models"][1]["languages"].as_array().unwrap().len(),
                60
            );
            assert!(translation_target_info(&paths, catalog::LMT.id).is_err());
            assert!(translation_target_info(&paths, catalog::OCR.id)
                .unwrap_err()
                .to_string()
                .contains("不支持翻译"));
            let ocr_directory = install_model(&paths, catalog::OCR.id);
            let lmt_directory = install_model(&paths, catalog::LMT.id);
            let snapshot = service.list(&paths).await.unwrap();
            assert_eq!(snapshot["installedCount"], 2);
            assert_eq!(
                snapshot["usedBytes"].as_u64().unwrap(),
                directory_size(&ocr_directory).unwrap() + directory_size(&lmt_directory).unwrap()
            );
            assert_eq!(
                service.installed(&paths, "ocr").await.unwrap()["models"][0]["id"],
                catalog::OCR.id
            );
            assert_eq!(
                service.installed(&paths, "translation").await.unwrap()["models"][0]["id"],
                catalog::LMT.id
            );
            assert_eq!(
                service.installed(&paths, "translation").await.unwrap()["models"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                translation_target_info(&paths, catalog::LMT.id).unwrap()["engine"],
                "local"
            );
            assert!(service
                .generate(&paths, json!({"id": catalog::LMT.id}))
                .await
                .unwrap_err()
                .to_string()
                .contains("不支持 OCR"));
            assert!(service
                .translate(&paths, json!({"id": catalog::OCR.id}))
                .await
                .unwrap_err()
                .to_string()
                .contains("不支持翻译"));
            service.delete(&lmt_directory).await.unwrap();
            assert!(ocr_directory.exists());
            assert_eq!(service.list(&paths).await.unwrap()["installedCount"], 1);
            assert!(translation_target_info(&paths, catalog::LMT.id).is_err());
            assert_eq!(
                service.installed(&paths, "translation").await.unwrap()["models"],
                json!([])
            );
        });
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
            let directory = model_directory(&paths, &catalog::OCR).unwrap();
            std::fs::create_dir_all(&directory).unwrap();
            let mut receipt = json!({"revision": REVISION, "files": {}});
            // 稀疏文件加校验凭据模拟完整安装；读取列表不重新计算大文件哈希。
            for file in FILES {
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
    fn runtime_validation_is_bound_to_model_files_and_runtime_profile() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        let directory = model_directory(&paths, &catalog::OCR).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        let mut receipt = json!({"revision": REVISION, "files": {}});
        for file in FILES {
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
        assert!(runtime_validation(&directory, &catalog::OCR).unwrap()["passed"].is_null());
        let mut record = json!({
            "revision": REVISION, "profile": runtime::validation_profile(&catalog::OCR),
            "files": receipt["files"], "passed": false, "error": "内存分配失败",
            "checkedAt": 1,
        });
        let validation_path = directory.with_file_name(format!("{MODEL_ID}.runtime-check.json"));
        let write_record =
            |record: &Value| std::fs::write(&validation_path, record.to_string()).unwrap();
        write_record(&record);
        assert_eq!(
            runtime_validation(&directory, &catalog::OCR).unwrap()["passed"],
            false
        );
        assert!(
            runtime_validation(&directory, &catalog::OCR).unwrap()["detail"]
                .as_str()
                .unwrap()
                .contains("内存分配失败")
        );
        record["passed"] = json!(true);
        write_record(&record);
        assert_eq!(
            runtime_validation(&directory, &catalog::OCR).unwrap()["passed"],
            true
        );
        record["profile"] = json!("旧引擎配置");
        write_record(&record);
        assert!(runtime_validation(&directory, &catalog::OCR).unwrap()["passed"].is_null());
        record["profile"] = json!(runtime::validation_profile(&catalog::OCR));
        record["files"][FILES[0].name]["sha256"] = json!("其他模型权重");
        write_record(&record);
        assert!(runtime_validation(&directory, &catalog::OCR).unwrap()["passed"].is_null());
        record["files"] = receipt["files"].clone();
        write_record(&record);
        let service = LocalModelService::default();
        std::fs::OpenOptions::new()
            .write(true)
            .open(directory.join(FILES[0].name))
            .unwrap()
            .set_len(123)
            .unwrap();
        let snapshot = tauri::async_runtime::block_on(service.list(&paths)).unwrap();
        assert_eq!(snapshot["models"][0]["phase"], "paused");
        assert_eq!(
            snapshot["models"][0]["compatibility"]["validation"]["passed"], true,
            "权重未完成时仍可展示同一版本的历史运行结果"
        );
        tauri::async_runtime::block_on(service.delete(&directory)).unwrap();
        assert_eq!(
            runtime_validation(&directory, &catalog::OCR).unwrap()["passed"],
            true,
            "删除权重后保留本机对同一版本的运行结论，允许重新下载"
        );
        std::fs::write(&validation_path, b"invalid-json").unwrap();
        assert!(runtime_validation(&directory, &catalog::OCR).unwrap()["passed"].is_null());
    }

    #[test]
    fn validation_rejects_incomplete_models_and_concurrent_inference() {
        let root = Fixture::new();
        let paths = crate::core::paths::resolve_app_paths(&root.0);
        let service = LocalModelService::default();
        tauri::async_runtime::block_on(async {
            assert!(service
                .validate(&paths, &catalog::OCR)
                .await
                .unwrap_err()
                .to_string()
                .contains("尚未完整下载"));
            let _inference = service.inference.lock().await;
            assert!(service
                .validate(&paths, &catalog::OCR)
                .await
                .unwrap_err()
                .to_string()
                .contains("正在使用"));
        });
    }

    #[test]
    #[ignore = "使用显式指定的本地权重验证并保存真实运行校验结果"]
    fn downloaded_model_passes_runtime_validation() {
        let paths = crate::core::paths::resolve_app_paths(&PathBuf::from(
            std::env::var("AI_MANAGER_MODEL_TEST_DATA").expect("请指定模型数据目录"),
        ));
        let service = LocalModelService::default();
        tauri::async_runtime::block_on(async {
            let result = service.validate(&paths, &catalog::OCR).await.unwrap();
            println!("运行校验：{result}");
            assert_eq!(result["passed"], true);
            let snapshot = service.list(&paths).await.unwrap();
            assert_eq!(
                snapshot["models"][0]["compatibility"]["validation"]["passed"],
                true
            );
            assert_eq!(snapshot["models"][0]["phase"], "downloaded");
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
        let directory = model_directory(&paths, &catalog::OCR).unwrap();
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
        let directory = model_directory(&paths, &catalog::OCR).unwrap();
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
        assert!(model_directory(&paths, &catalog::OCR).is_err());
        #[cfg(windows)]
        std::fs::remove_dir(directory.join(MODEL_ID)).unwrap();
        #[cfg(not(windows))]
        std::fs::remove_file(directory.join(MODEL_ID)).unwrap();
        assert!(destination.exists());
    }
}

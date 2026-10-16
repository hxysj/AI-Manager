use super::*;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

pub(super) const MAX_ATTACHMENTS: usize = 100;
pub(crate) const FILE_CHUNK_BYTES: u64 = 4 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const PARTIAL_UPLOAD_LIFETIME: u128 = 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: String,
}

impl From<&LanShareFile> for Attachment {
    fn from(file: &LanShareFile) -> Self {
        Self {
            id: file.id.clone(),
            name: file.name.clone(),
            size: file.size,
            mime_type: file.mime_type.clone(),
        }
    }
}

pub(super) async fn prepare(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    session_id: &str,
    payload: &Value,
) -> Result<Vec<LanShareFile>, ManagerError> {
    let selected_paths = string_array_value(payload.get("paths"));
    let identifiers = string_array_value(payload.get("attachmentIds"));
    if selected_paths.len() + identifiers.len() > MAX_ATTACHMENTS {
        return Err(ManagerError::System("一条消息最多添加 100 个附件。".into()));
    }
    let saved = {
        let _storage = registry.storage.lock().await;
        read_array::<LanShareFile>(&paths.lan_share_files.files)?
    };
    let mut selected = Vec::new();
    for identifier in identifiers {
        let file = saved
            .iter()
            .find(|file| file.id == identifier && file.session_id == session_id)
            .ok_or_else(|| {
                ManagerError::System("附件已失效或不属于当前会话，请重新添加。".into())
            })?;
        selected.push(file.clone());
    }
    for selected_path in selected_paths {
        let file = file_payload(&selected_path, session_id).await?;
        if !selected.iter().any(|current| current.id == file.id) {
            selected.push(file);
        }
    }
    for file in &selected {
        let metadata = tokio::fs::metadata(&file.path).await?;
        if !metadata.is_file() || metadata.len() != file.size {
            return Err(ManagerError::System(format!(
                "附件已改变或不是文件：{}",
                file.name
            )));
        }
        if file.size > MAX_FILE_BYTES {
            return Err(ManagerError::System("单个附件不能超过 10 GiB。".into()));
        }
    }
    if let Some(order) = payload.get("attachmentOrder").and_then(Value::as_array) {
        selected.sort_by_key(|file| {
            order
                .iter()
                .position(|entry| {
                    entry.get("id").and_then(Value::as_str) == Some(&file.id)
                        || entry.get("path").and_then(Value::as_str) == Some(&file.path)
                })
                .unwrap_or(order.len())
        });
    }
    Ok(selected)
}

pub async fn clipboard_files() -> Result<Value, ManagerError> {
    #[cfg(windows)]
    {
        let files = tauri::async_runtime::spawn_blocking(|| {
            use std::os::windows::process::CommandExt;
            let executable = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32/WindowsPowerShell/v1.0/powershell.exe");
            let mut command = std::process::Command::new(executable);
            command.creation_flags(0x08000000);
            command.args(["-NoProfile", "-NonInteractive", "-STA", "-WindowStyle", "Hidden", "-Command",
                "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $items = @(Get-Clipboard -Format FileDropList | Select-Object -First 100 | ForEach-Object { $_.FullName }); ConvertTo-Json -InputObject $items -Compress"]);
            let output = command.output()?;
            if !output.status.success() { return Err(ManagerError::System("无法读取剪贴板文件，请使用添加附件或拖拽。".into())); }
            let files: Vec<String> = serde_json::from_slice(&output.stdout)?;
            Ok::<_, ManagerError>(files.into_iter().filter(|file| Path::new(file).is_file()).collect::<Vec<_>>())
        }).await.map_err(|error| ManagerError::System(error.to_string()))??;
        return Ok(lan_share_response(json!(files)));
    }
    #[cfg(not(windows))]
    Ok(lan_share_response(json!([])))
}

pub(super) async fn publish(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    selected: &[LanShareFile],
) -> Result<(), ManagerError> {
    if selected.is_empty() {
        return Ok(());
    }
    let _storage = registry.storage.lock().await;
    let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
    for selected_file in selected {
        let mut file = selected_file.clone();
        file.enabled = true;
        if let Some(current) = files.iter_mut().find(|current| current.id == file.id) {
            *current = file;
        } else {
            files.push(file);
        }
    }
    write_json(&paths.lan_share_files.files, &json!(files)).await
}

pub(super) async fn upload(
    mut body: Incoming,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    session_id: &str,
    name: &str,
) -> Result<LanShareFile, ManagerError> {
    if !read_array::<LanShareSession>(&paths.lan_share_files.sessions)?
        .iter()
        .any(|session| session.id == session_id)
    {
        return Err(ManagerError::System("请先选择有效会话。".into()));
    }
    let _permit = registry.transfer.acquire_upload().await;
    cleanup_uploads(registry, paths).await?;
    let safe_name = safe_name(name)?;
    let directory = Path::new(&paths.lan_share_dir).join("uploads");
    tokio::fs::create_dir_all(&directory).await?;
    let target = directory.join(format!("{}-{safe_name}", create_id("attachment")));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .await?;
    let result = async {
        let mut received = 0u64;
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|error| ManagerError::System(error.to_string()))?;
            if let Ok(bytes) = frame.into_data() {
                received = received.saturating_add(bytes.len() as u64);
                if received > MAX_FILE_BYTES {
                    return Err(ManagerError::System("单个附件不能超过 10 GiB。".into()));
                }
                registry.transfer.receive_bytes(bytes.len()).await;
                file.write_all(&bytes).await?;
            }
        }
        file.flush().await?;
        Ok::<(), ManagerError>(())
    }
    .await;
    drop(file);
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(error);
    }
    let mut uploaded = file_payload(&target.to_string_lossy(), session_id).await?;
    uploaded.name = safe_name;
    uploaded.enabled = false;
    let _storage = registry.storage.lock().await;
    let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
    files.push(uploaded.clone());
    if let Err(error) = write_json(&paths.lan_share_files.files, &json!(files)).await {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(error);
    }
    Ok(uploaded)
}

pub(super) async fn upload_chunk<B>(
    mut body: B,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    session_id: &str,
    upload_id: &str,
    name: &str,
    chunk_index: u64,
    chunk_total: u64,
    offset: u64,
    total_size: u64,
    content_length: u64,
    content_range: &str,
    chunk_sha256: &str,
) -> Result<Option<LanShareFile>, ManagerError>
where
    B: hyper::body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    if !read_array::<LanShareSession>(&paths.lan_share_files.sessions)?
        .iter()
        .any(|session| session.id == session_id)
    {
        return Err(ManagerError::System("请先选择有效会话。".into()));
    }
    let safe_name = safe_name(name)?;
    if upload_id.is_empty()
        || upload_id.len() > 120
        || !upload_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(ManagerError::System("分块上传标识无效。".into()));
    }
    if chunk_total != total_size.div_ceil(FILE_CHUNK_BYTES).max(1)
        || chunk_index >= chunk_total
        || total_size > MAX_FILE_BYTES
        || offset != chunk_index * FILE_CHUNK_BYTES
    {
        return Err(ManagerError::System("分块上传参数无效。".into()));
    }
    if content_length > FILE_CHUNK_BYTES
        || offset > total_size
        || content_length > total_size.saturating_sub(offset)
        || (total_size > 0 && content_length == 0)
        || (chunk_index + 1 < chunk_total && content_length != FILE_CHUNK_BYTES)
        || (chunk_index + 1 == chunk_total && content_length != total_size.saturating_sub(offset))
    {
        return Err(ManagerError::System("分块大小或偏移无效。".into()));
    }
    if chunk_sha256.len() != 64
        || !chunk_sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ManagerError::System("分块校验值无效。".into()));
    }
    let expected_range = if total_size == 0 {
        "bytes 0-0/0".to_string()
    } else {
        format!(
            "bytes {}-{}/{}",
            offset,
            offset + content_length - 1,
            total_size
        )
    };
    if content_range != expected_range {
        return Err(ManagerError::System("分块范围无效。".into()));
    }

    // 同一上传的校验和写入串行执行，避免重叠请求覆盖已经接收的分块。
    let upload_lock = {
        let mut uploads = registry.uploads.lock().await;
        uploads.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = uploads.get(upload_id).and_then(std::sync::Weak::upgrade) {
            lock
        } else {
            let lock = Arc::new(Mutex::new(()));
            uploads.insert(upload_id.to_string(), Arc::downgrade(&lock));
            lock
        }
    };
    let _upload = upload_lock.lock().await;
    if chunk_index == 0 {
        cleanup_uploads(registry, paths).await?;
    }
    let directory = Path::new(&paths.lan_share_dir).join("uploads");
    tokio::fs::create_dir_all(&directory).await?;
    let part_path = directory.join(format!(".lan-upload-{upload_id}.part"));
    let metadata_path = directory.join(format!(".lan-upload-{upload_id}.json"));
    let stored_metadata = match tokio::fs::read(&metadata_path).await {
        Ok(bytes) => Some(serde_json::from_slice::<Value>(&bytes)?),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(metadata) = stored_metadata.as_ref() {
        if metadata["sessionId"] != session_id
            || metadata["name"] != safe_name
            || metadata["chunkTotal"] != chunk_total
            || metadata["totalSize"] != total_size
            || metadata["nextIndex"] != chunk_index
        {
            return Err(ManagerError::System("分块顺序或上传信息不匹配。".into()));
        }
    } else if chunk_index != 0 || offset != 0 {
        return Err(ManagerError::System("缺少首个分块，无法继续拼接。".into()));
    } else {
        write_json(
            &metadata_path.to_string_lossy(),
            &json!({
                "sessionId": session_id,
                "name": safe_name,
                "chunkTotal": chunk_total,
                "totalSize": total_size,
                "nextIndex": 0,
                "updatedAt": now_millis()
            }),
        )
        .await?;
    }

    let mut file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&part_path)
        .await?;
    let current_size = file.metadata().await?.len();
    if current_size != offset {
        return Err(ManagerError::System("分块偏移与已接收内容不匹配。".into()));
    }
    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let result = async {
        let mut received = 0u64;
        let mut digest = Sha256::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|error| ManagerError::System(error.to_string()))?;
            if let Ok(bytes) = frame.into_data() {
                received = received.saturating_add(bytes.len() as u64);
                if received > content_length {
                    return Err(ManagerError::System("分块大小超过声明值。".into()));
                }
                digest.update(&bytes);
                file.write_all(&bytes).await?;
            }
        }
        if received != content_length
            || format!("{:x}", digest.finalize()) != chunk_sha256.to_lowercase()
        {
            return Err(ManagerError::System("分块校验失败。".into()));
        }
        file.flush().await?;
        Ok::<(), ManagerError>(())
    }
    .await;
    if let Err(error) = result {
        let _ = file.set_len(current_size).await;
        drop(file);
        if chunk_index == 0 && current_size == 0 {
            let _ = tokio::fs::remove_file(&part_path).await;
            let _ = tokio::fs::remove_file(&metadata_path).await;
        }
        return Err(error);
    }
    drop(file);

    if chunk_index + 1 < chunk_total {
        write_json(
            &metadata_path.to_string_lossy(),
            &json!({
                "sessionId": session_id,
                "name": safe_name,
                "chunkTotal": chunk_total,
                "totalSize": total_size,
                "nextIndex": chunk_index + 1,
                "updatedAt": now_millis()
            }),
        )
        .await?;
        return Ok(None);
    }

    let final_size = tokio::fs::metadata(&part_path).await?.len();
    if final_size != total_size {
        return Err(ManagerError::System("文件拼接后的大小不匹配。".into()));
    }
    let target = directory.join(format!("{}-{safe_name}", create_id("attachment")));
    tokio::fs::rename(&part_path, &target).await?;
    let mut uploaded = match file_payload(&target.to_string_lossy(), session_id).await {
        Ok(file) => file,
        Err(error) => {
            let _ = tokio::fs::remove_file(&target).await;
            return Err(error);
        }
    };
    uploaded.name = safe_name;
    uploaded.enabled = false;
    let _storage = registry.storage.lock().await;
    let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
    files.push(uploaded.clone());
    if let Err(error) = write_json(&paths.lan_share_files.files, &json!(files)).await {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(error);
    }
    let _ = tokio::fs::remove_file(&metadata_path).await;
    Ok(Some(uploaded))
}

fn safe_name(name: &str) -> Result<String, ManagerError> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 240
        || name
            .chars()
            .any(|character| character.is_control() || "/\\:*?\"<>|".contains(character))
        || name == "."
        || name == ".."
    {
        return Err(ManagerError::System("附件文件名无效。".into()));
    }
    Ok(name.to_string())
}

pub async fn discard_uploads(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let identifiers = string_array_value(payload.get("attachmentIds"));
    let _storage = registry.storage.lock().await;
    let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
    let directory = Path::new(&paths.lan_share_dir).join("uploads");
    let messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
    let discard = files
        .iter()
        .filter(|file| {
            !file.enabled
                && identifiers.contains(&file.id)
                && Path::new(&file.path).parent() == Some(directory.as_path())
                && !messages.iter().any(|message| {
                    message
                        .attachments
                        .iter()
                        .any(|attachment| attachment.id == file.id)
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    for file in &discard {
        if Path::new(&file.path).parent() == Some(directory.as_path()) {
            match tokio::fs::remove_file(&file.path).await {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    files.retain(|file| !discard.iter().any(|removed| removed.id == file.id));
    write_json(&paths.lan_share_files.files, &json!(files)).await?;
    Ok(lan_share_response(json!(true)))
}

pub(super) async fn cleanup_uploads(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<(), ManagerError> {
    let identifiers = {
        let _storage = registry.storage.lock().await;
        read_array::<LanShareFile>(&paths.lan_share_files.files)?
            .into_iter()
            .filter(|file| {
                !file.enabled && now_millis().saturating_sub(file.updated_at) > 24 * 60 * 60 * 1000
            })
            .map(|file| file.id)
            .collect::<Vec<_>>()
    };
    if !identifiers.is_empty() {
        discard_uploads(registry, paths, json!({ "attachmentIds": identifiers })).await?;
    }
    let directory = Path::new(&paths.lan_share_dir).join("uploads");
    let mut entries = match tokio::fs::read_dir(&directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !(name.starts_with(".lan-upload-")
            && (name.ends_with(".part") || name.ends_with(".json")))
        {
            continue;
        }
        let stale = entry
            .metadata()
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .map(|elapsed| elapsed.as_millis() > PARTIAL_UPLOAD_LIFETIME)
            .unwrap_or(false);
        if stale {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::CONTENT_RANGE;

    #[test]
    fn browser_upload_is_rate_limited_without_blocking_state_requests() {
        tauri::async_runtime::block_on(async {
            let temp_root = std::env::temp_dir();
            let root = temp_root.join(create_id("browser-upload-rate-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            let device = upsert_device(&registry, &paths, "", "测试浏览器", "browser", "127.0.0.1").await.unwrap();
            let state = create_session(&registry, &paths, json!({ "deviceId": device.id })).await.unwrap();
            let session_id = state["data"]["currentSession"]["id"].as_str().unwrap().to_string();
            registry.transfer.configure(transfer::Settings { send_mib_per_second: 1, max_concurrent_files: 1 });
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (started_tx, mut started_rx) = mpsc::unbounded_channel();
            let server_registry = registry.clone();
            let server_paths = paths.clone();
            let server = tauri::async_runtime::spawn(async move {
                loop {
                    let (socket, _) = listener.accept().await.unwrap();
                    let registry = server_registry.clone();
                    let paths = server_paths.clone();
                    let session_id = session_id.clone();
                    let started = started_tx.clone();
                    tauri::async_runtime::spawn(async move {
                        let service = service_fn(move |request: Request<Incoming>| {
                            let registry = registry.clone();
                            let paths = paths.clone();
                            let session_id = session_id.clone();
                            let started = started.clone();
                            async move {
                                if request.method() == Method::PUT {
                                    started.send(()).unwrap();
                                    let file = upload(request.into_body(), &registry, &paths, &session_id, "browser.bin").await.unwrap();
                                    Ok::<_, Infallible>(api_success(json!(file.size)))
                                } else {
                                    Ok(api_success(get_state(&registry, &paths).await.unwrap()))
                                }
                            }
                        });
                        let _ = http1::Builder::new().serve_connection(TokioIo::new(socket), service).await;
                    });
                }
            });
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let content = vec![8; 512 * 1024];
            let request = client.put(format!("http://{address}/upload")).body(content.clone());
            registry.transfer.configure(transfer::Settings { send_mib_per_second: 1, max_concurrent_files: 1 });
            let started = tokio::time::Instant::now();
            let upload = tauri::async_runtime::spawn(async move { request.send().await.unwrap().json::<Value>().await.unwrap() });
            started_rx.recv().await.unwrap();
            let state = tokio::time::timeout(std::time::Duration::from_millis(250), client.get(format!("http://{address}/state")).send()).await.unwrap().unwrap();
            assert!(state.status().is_success());
            assert_eq!(upload.await.unwrap()["data"], content.len());
            assert!(started.elapsed() >= std::time::Duration::from_millis(475));
            let files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files).unwrap();
            assert_eq!(tokio::fs::read(&files[0].path).await.unwrap(), content);
            server.abort();
            let _ = server.await;
            drop(client);
            assert_eq!(root.parent(), Some(temp_root.as_path()));
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn batch_files_create_one_message_and_preserve_order() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("attachment-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            let device = upsert_device(&registry, &paths, "", "测试访客", "browser", "192.168.1.7")
                .await
                .unwrap();
            let state = create_session(&registry, &paths, json!({ "deviceId": device.id }))
                .await
                .unwrap();
            let session_id = state["data"]["currentSession"]["id"].as_str().unwrap();
            let first = root.join("first.txt");
            let second = root.join("second.png");
            tokio::fs::write(&first, b"first").await.unwrap();
            tokio::fs::write(&second, b"second").await.unwrap();
            let state = add_files(
                &registry,
                &paths,
                json!({ "sessionId": session_id, "paths": [first, second] }),
            )
            .await
            .unwrap();
            let messages = state["data"]["messages"].as_array().unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0]["attachments"].as_array().unwrap().len(), 2);
            assert_eq!(messages[0]["attachments"][0]["name"], "first.txt");
            assert_eq!(messages[0]["attachments"][1]["name"], "second.png");
            assert!(!messages[0]["attachments"].to_string().contains("path"));
            let before = read_array::<LanShareMessage>(&paths.lan_share_files.messages)
                .unwrap()
                .len();
            assert!(add_files(&registry, &paths, json!({ "sessionId": session_id, "paths": [root.join("first.txt"), root.join("missing.txt")] })).await.is_err());
            assert_eq!(
                read_array::<LanShareMessage>(&paths.lan_share_files.messages)
                    .unwrap()
                    .len(),
                before
            );
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn attachments_cannot_cross_sessions_and_retry_does_not_duplicate_messages() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("attachment-scope-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            let device = upsert_device(&registry, &paths, "", "设备", "browser", "192.168.1.8")
                .await
                .unwrap();
            let state = create_session(&registry, &paths, json!({ "deviceId": device.id }))
                .await
                .unwrap();
            let session_id = state["data"]["currentSession"]["id"].as_str().unwrap();
            let source = root.join("image.png");
            tokio::fs::write(&source, b"image").await.unwrap();
            let file = file_payload(&source.to_string_lossy(), session_id)
                .await
                .unwrap();
            publish(&registry, &paths, &[file.clone()]).await.unwrap();
            assert!(prepare(
                &registry,
                &paths,
                "another-session",
                &json!({ "attachmentIds": [file.id] })
            )
            .await
            .is_err());
            for _attempt in 0..2 {
                append_message_with_attachments(
                    &registry,
                    &paths,
                    &device.id,
                    session_id,
                    "mobile-to-desktop",
                    "",
                    true,
                    &[file.clone()],
                    Some("same-message"),
                )
                .await
                .unwrap();
            }
            assert_eq!(
                read_array::<LanShareMessage>(&paths.lan_share_files.messages)
                    .unwrap()
                    .len(),
                1
            );
            discard_uploads(&registry, &paths, json!({ "attachmentIds": [file.id] }))
                .await
                .unwrap();
            assert!(source.exists());
            assert_eq!(
                read_array::<LanShareFile>(&paths.lan_share_files.files)
                    .unwrap()
                    .len(),
                1
            );
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn mixed_attachment_sources_keep_composer_order() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("attachment-order-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            let first = root.join("selected.txt");
            let second = root.join("pasted.png");
            tokio::fs::write(&first, b"selected").await.unwrap();
            tokio::fs::write(&second, b"pasted").await.unwrap();
            let pasted = file_payload(&second.to_string_lossy(), "session")
                .await
                .unwrap();
            publish(&registry, &paths, &[pasted.clone()]).await.unwrap();
            let selected = prepare(
                &registry,
                &paths,
                "session",
                &json!({
                    "paths": [first], "attachmentIds": [pasted.id],
                    "attachmentOrder": [{ "path": first }, { "id": pasted.id }]
                }),
            )
            .await
            .unwrap();
            assert_eq!(selected[0].name, "selected.txt");
            assert_eq!(selected[1].name, "pasted.png");
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn expired_upload_cleanup_preserves_original_published_and_referenced_files() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("attachment-cleanup-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let directory = Path::new(&paths.lan_share_dir).join("uploads");
            tokio::fs::create_dir_all(&directory).await.unwrap();
            let registry = LanShareServerRegistry::new();
            let mut files = Vec::new();
            for name in [
                "expired.txt",
                "fresh.txt",
                "published.txt",
                "referenced.txt",
                "original.txt",
            ] {
                let source = if name == "original.txt" {
                    root.join(name)
                } else {
                    directory.join(name)
                };
                tokio::fs::write(&source, b"keep-or-clean").await.unwrap();
                let mut file = file_payload(&source.to_string_lossy(), "session")
                    .await
                    .unwrap();
                file.enabled = name == "published.txt";
                file.updated_at = if name == "fresh.txt" { now_millis() } else { 1 };
                files.push(file);
            }
            write_json(&paths.lan_share_files.files, &json!(files))
                .await
                .unwrap();
            let referenced: LanShareMessage = serde_json::from_value(json!({
                "id": "stored", "sessionId": "session", "deviceId": "peer", "deviceName": "测试设备",
                "direction": "mobile-to-desktop", "messageType": "file", "content": "",
                "createdAt": 1, "delivered": true, "read": false,
                "attachments": [Attachment::from(&files[3])]
            })).unwrap();
            write_json(&paths.lan_share_files.messages, &json!([referenced]))
                .await
                .unwrap();
            cleanup_uploads(&registry, &paths).await.unwrap();
            assert!(!Path::new(&files[0].path).exists());
            for file in files.iter().skip(1) {
                assert!(Path::new(&file.path).exists());
            }
            assert_eq!(
                read_array::<LanShareFile>(&paths.lan_share_files.files)
                    .unwrap()
                    .len(),
                4
            );
            discard_uploads(&registry, &paths, json!({ "attachmentIds": [files[1].id] }))
                .await
                .unwrap();
            assert!(!Path::new(&files[1].path).exists());
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn rejects_upload_path_traversal() {
        for name in [
            "../private",
            "C:\\secret.txt",
            "..",
            "",
            "file\0.txt",
            "a/b.png",
        ] {
            assert!(safe_name(name).is_err());
        }
        assert_eq!(safe_name("截图 1.png").unwrap(), "截图 1.png");
    }

    #[test]
    fn legacy_messages_have_no_attachments() {
        let message: LanShareMessage = serde_json::from_value(json!({
            "id": "old", "sessionId": "session", "deviceId": "peer", "deviceName": "Peer",
            "direction": "desktop-to-mobile", "messageType": "text", "content": "旧消息",
            "createdAt": 1, "delivered": true, "read": false
        }))
        .unwrap();
        assert!(message.attachments.is_empty());
        assert_eq!(message.content, "旧消息");
    }

    #[test]
    fn chunk_upload_reassembles_file_and_cleans_failed_chunk() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("attachment-chunk-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            let device = upsert_device(
                &registry,
                &paths,
                "native-chunk-peer",
                "分块设备",
                "MonkeyThief/Desktop",
                "127.0.0.1",
            )
            .await
            .unwrap();
            let state = create_session(&registry, &paths, json!({ "deviceId": device.id }))
                .await
                .unwrap();
            let session_id = state["data"]["currentSession"]["id"]
                .as_str()
                .unwrap()
                .to_string();
            let chunks = vec![
                vec![b'a'; FILE_CHUNK_BYTES as usize],
                vec![b'b'; FILE_CHUNK_BYTES as usize],
                b"tail".to_vec(),
            ];
            let total_size = chunks.iter().map(|chunk| chunk.len() as u64).sum::<u64>();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server_registry = registry.clone();
            let server_paths = paths.clone();
            let server_session = session_id.clone();
            let server = tauri::async_runtime::spawn(async move {
                loop {
                    let (socket, _) = listener.accept().await.unwrap();
                    let server_registry = server_registry.clone();
                    let server_paths = server_paths.clone();
                    let server_session = server_session.clone();
                    let service = service_fn(move |request: Request<Incoming>| {
                        let registry = server_registry.clone();
                        let paths = server_paths.clone();
                        let session_id = server_session.clone();
                        async move {
                            let content_length = request
                                .headers()
                                .get(CONTENT_LENGTH)
                                .and_then(|value| value.to_str().ok())
                                .and_then(|value| value.parse::<u64>().ok())
                                .unwrap();
                            let content_range = request
                                .headers()
                                .get(CONTENT_RANGE)
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default()
                                .to_string();
                            let upload_id = query_value(request.uri(), "uploadId");
                            let name = query_value(request.uri(), "name");
                            let chunk_index = query_value(request.uri(), "chunkIndex")
                                .parse::<u64>()
                                .unwrap();
                            let chunk_total = query_value(request.uri(), "chunkTotal")
                                .parse::<u64>()
                                .unwrap();
                            let offset =
                                query_value(request.uri(), "offset").parse::<u64>().unwrap();
                            let total_size = query_value(request.uri(), "totalSize")
                                .parse::<u64>()
                                .unwrap();
                            let chunk_sha256 = query_value(request.uri(), "sha256");
                            let response = match upload_chunk(
                                request.into_body(),
                                &registry,
                                &paths,
                                &session_id,
                                &upload_id,
                                &name,
                                chunk_index,
                                chunk_total,
                                offset,
                                total_size,
                                content_length,
                                &content_range,
                                &chunk_sha256,
                            )
                            .await
                            {
                                Ok(file) => api_success(json!({
                                    "complete": file.is_some(),
                                    "file": file.as_ref().map(Attachment::from),
                                    "chunkIndex": chunk_index
                                })),
                                Err(error) => api_error(StatusCode::BAD_REQUEST, error.to_string()),
                            };
                            Ok::<_, Infallible>(response)
                        }
                    });
                    http1::Builder::new()
                        .serve_connection(TokioIo::new(socket), service)
                        .await
                        .unwrap();
                }
            });
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let upload_id = "chunk-test-upload";
            let chunk_total = chunks.len();
            let send_chunk = |index: usize, chunk: Vec<u8>, sha256: String| {
                let client = client.clone();
                let session_id = session_id.clone();
                async move {
                    let offset = if index == 0 {
                        0
                    } else if index == 1 {
                        FILE_CHUNK_BYTES
                    } else {
                        FILE_CHUNK_BYTES * 2
                    };
                    let range = if total_size == 0 {
                        "bytes 0-0/0".to_string()
                    } else {
                        format!(
                            "bytes {}-{}/{}",
                            offset,
                            offset + chunk.len() as u64 - 1,
                            total_size
                        )
                    };
                    let response = client
                        .put(format!("http://{address}/test/chunk"))
                        .query(&[
                            ("uploadId", upload_id.to_string()),
                            ("sessionId", session_id),
                            ("name", "assembled.bin".to_string()),
                            ("chunkIndex", index.to_string()),
                            ("chunkTotal", chunk_total.to_string()),
                            ("offset", offset.to_string()),
                            ("totalSize", total_size.to_string()),
                            ("sha256", sha256),
                        ])
                        .header(CONTENT_LENGTH, chunk.len())
                        .header(CONTENT_RANGE, range)
                        .body(chunk)
                        .send()
                        .await
                        .unwrap();
                    let status = response.status();
                    let body = response.text().await.unwrap();
                    let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|error| {
                        panic!("status={status}, body={body:?}, error={error}")
                    });
                    parsed
                }
            };
            let failed = send_chunk(0, chunks[0].clone(), "0".repeat(64)).await;
            assert_eq!(failed["status"], "error");
            let missing = send_chunk(
                1,
                chunks[1].clone(),
                format!("{:x}", Sha256::digest(&chunks[1])),
            )
            .await;
            assert_eq!(missing["status"], "error");
            let first = send_chunk(
                0,
                chunks[0].clone(),
                format!("{:x}", Sha256::digest(&chunks[0])),
            )
            .await;
            assert_eq!(first["data"]["complete"], false);
            let duplicate = send_chunk(
                0,
                chunks[0].clone(),
                format!("{:x}", Sha256::digest(&chunks[0])),
            )
            .await;
            assert_eq!(duplicate["status"], "error");
            let out_of_order = send_chunk(
                2,
                chunks[2].clone(),
                format!("{:x}", Sha256::digest(&chunks[2])),
            )
            .await;
            assert_eq!(out_of_order["status"], "error");
            let corrupted = send_chunk(1, chunks[1].clone(), "0".repeat(64)).await;
            assert_eq!(corrupted["status"], "error");
            let second = send_chunk(
                1,
                chunks[1].clone(),
                format!("{:x}", Sha256::digest(&chunks[1])),
            )
            .await;
            assert_eq!(second["data"]["complete"], false);
            let third = send_chunk(
                2,
                chunks[2].clone(),
                format!("{:x}", Sha256::digest(&chunks[2])),
            )
            .await;
            assert_eq!(third["data"]["complete"], true);
            drop(client);
            server.abort();
            let _ = server.await;
            let files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files).unwrap();
            assert_eq!(files.len(), 1);
            let assembled = tokio::fs::read(&files[0].path).await.unwrap();
            assert_eq!(assembled.len(), total_size as usize);
            assert_eq!(&assembled[..FILE_CHUNK_BYTES as usize], &chunks[0]);
            assert_eq!(
                &assembled[FILE_CHUNK_BYTES as usize..FILE_CHUNK_BYTES as usize * 2],
                &chunks[1]
            );
            assert_eq!(&assembled[FILE_CHUNK_BYTES as usize * 2..], &chunks[2]);
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }
}

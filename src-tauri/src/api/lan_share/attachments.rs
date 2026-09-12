use super::*;
use tokio::io::AsyncWriteExt;

pub(super) const MAX_ATTACHMENTS: usize = 100;
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024 * 1024;

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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

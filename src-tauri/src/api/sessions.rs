use crate::api::usage;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use crate::core::settings::string_value;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub async fn refresh_sessions_state(
    paths: &AppPaths,
    state: &mut Value,
) -> Result<(), ManagerError> {
    let previous_sessions = state
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let previous_session_map = previous_sessions
        .into_iter()
        .filter_map(|session| {
            let raw_path = string_value(session.get("rawPath"));
            (!raw_path.is_empty()).then_some((raw_path, session))
        })
        .collect::<HashMap<_, _>>();
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut seen_paths = HashSet::new();
    let mut sessions = Vec::new();
    let mut diagnostics = Vec::new();

    reconcile_recycled_cli_state(paths, &cli_targets, &mut diagnostics).await?;

    for item in usage::collect_cli_session_files(&cli_targets)? {
        let raw_path = string_value(item.get("filePath"));

        if raw_path.is_empty() || !seen_paths.insert(raw_path.clone()) {
            continue;
        }

        let source_updated_at = std::fs::metadata(&raw_path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_millis)
            .unwrap_or(0);

        if let Some(previous) = previous_session_map.get(&raw_path).filter(|session| {
            source_updated_at > 0
                && session.get("updatedAt").and_then(Value::as_u64) == Some(source_updated_at)
        }) {
            sessions.push(previous.clone());
            continue;
        }

        match scan_session_metadata(&item).await {
            Ok(Some(session)) => sessions.push(session),
            Ok(None) => {}
            Err(error) => diagnostics.push(json!({
              "type": "session-parse-error",
              "message": error.to_string(),
              "sourcePath": raw_path
            })),
        }
    }

    sessions.sort_by(|left, right| {
        right
            .get("updatedAt")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .cmp(&left.get("updatedAt").and_then(Value::as_u64).unwrap_or(0))
    });
    write_json(&paths.storage_files.sessions, &json!(sessions)).await?;
    state["sessions"] = json!(sessions);
    state["diagnostics"] = json!(diagnostics);
    Ok(())
}

async fn reconcile_recycled_cli_state(
    paths: &AppPaths,
    cli_targets: &[Value],
    diagnostics: &mut Vec<Value>,
) -> Result<(), ManagerError> {
    let metadata_dir = Path::new(&paths.session_recycle_metadata_dir);

    if !metadata_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(metadata_dir)? {
        let entry = entry?;
        let metadata_path = entry.path();

        if !metadata_path.is_file()
            || metadata_path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            continue;
        }

        let mut metadata: Value =
            serde_json::from_str(&tokio::fs::read_to_string(&metadata_path).await?)?;

        if metadata
            .get("cliStateSynchronized")
            .and_then(Value::as_bool)
            == Some(true)
        {
            continue;
        }

        if let Err(error) = reconcile_recycled_session(&mut metadata, cli_targets).await {
            diagnostics.push(json!({
              "type": "session-recycle-sync-error",
              "message": error.to_string(),
              "sourcePath": string_value(metadata.get("recycledPath"))
            }));
            continue;
        }
        write_json(metadata_path, &metadata).await?;
    }
    Ok(())
}

async fn reconcile_recycled_session(
    metadata: &mut Value,
    cli_targets: &[Value],
) -> Result<(), ManagerError> {
    let cli = string_value(metadata.get("cli"));
    let recycled_path = PathBuf::from(string_value(metadata.get("recycledPath")));
    let original_path = PathBuf::from(string_value(metadata.get("originalPath")));

    if cli.is_empty() || !recycled_path.exists() {
        return Ok(());
    }

    let mut source = metadata.clone();

    source["rawPath"] = json!(recycled_path.to_string_lossy().to_string());
    let cli_session_id = read_cli_session_id(&source).await?;
    let cli_target = cli_targets
        .iter()
        .find(|item| string_value(item.get("id")) == cli)
        .cloned()
        .unwrap_or_else(|| json!({}));
    let cli_config_path = string_value(cli_target.get("configPath"));
    let cli_executable_path = string_value(cli_target.get("executablePath"));

    if ["claude", "codex"].contains(&cli.as_str()) && cli_config_path.is_empty() {
        return Err(ManagerError::System(format!(
            "{cli} 配置目录未识别，无法同步回收站 Session。"
        )));
    }

    if cli == "codex" && !cli_session_id.is_empty() {
        if let Some(parent) = original_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        move_file(&recycled_path, &original_path).await?;
        if let Err(error) = run_codex_session_command(
            &cli_executable_path,
            &cli_config_path,
            "archive",
            &cli_session_id,
        )
        .await
        {
            move_file(&original_path, &recycled_path).await?;
            return Err(error);
        }
        let archived_path = Path::new(&cli_config_path)
            .join("archived_sessions")
            .join(original_path.file_name().unwrap_or_default());

        if !archived_path.exists() {
            run_codex_session_command(
                &cli_executable_path,
                &cli_config_path,
                "unarchive",
                &cli_session_id,
            )
            .await?;
            move_file(&original_path, &recycled_path).await?;
            return Err(ManagerError::System(
                "Codex 已归档 Session，但未找到归档文件。".to_string(),
            ));
        }
        move_file(&archived_path, &recycled_path).await?;
        metadata["cliArchivedPath"] = json!(archived_path.to_string_lossy().to_string());
    }

    metadata["cliConfigPath"] = json!(cli_config_path);
    metadata["cliExecutablePath"] = json!(cli_executable_path);
    metadata["cliSessionId"] = json!(cli_session_id);
    metadata["cliHistoryEntries"] = json!(
        remove_cli_history_entries(&cli_config_path, &cli_session_id).await?
    );
    metadata["cliStateSynchronized"] = json!(true);
    Ok(())
}

async fn scan_session_metadata(item: &Value) -> Result<Option<Value>, ManagerError> {
    let raw_path = string_value(item.get("filePath"));
    let cli = string_value(item.get("cli"));
    let content = tokio::fs::read_to_string(&raw_path).await?;
    let metadata = scan_session_metadata_content(&raw_path, &cli, &content)?;

    if metadata.message_count == 0
        || (["claude", "codex", "opencode"].contains(&cli.as_str())
            && !metadata.has_conversation)
    {
        return Ok(None);
    }

    let title = if metadata.title.is_empty() {
        let title = truncate_text(&metadata.first_user_message, 50);

        if title.is_empty() {
            Path::new(&raw_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&raw_path)
                .to_string()
        } else {
            title
        }
    } else {
        metadata.title
    };
    let summary = truncate_text(&metadata.first_assistant_message, 120);
    let file_metadata = std::fs::metadata(&raw_path)?;
    let created_at = file_metadata
        .created()
        .ok()
        .and_then(system_time_millis)
        .unwrap_or(0);
    let updated_at = file_metadata
        .modified()
        .ok()
        .and_then(system_time_millis)
        .unwrap_or(0);
    let project_name = Path::new(&metadata.project_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string();

    Ok(Some(json!({
      "id": create_session_id(&raw_path),
      "cli": cli,
      "cliName": first_string(item.get("cliName"), item.get("cli")),
      "title": title,
      "summary": summary,
      "projectPath": metadata.project_path,
      "projectName": project_name,
      "model": metadata.model,
      "rawPath": raw_path,
      "createdAt": created_at,
      "updatedAt": updated_at,
      "messageCount": metadata.message_count,
      "tokenCount": metadata.token_count,
      "pinned": false,
      "archived": false,
      "deleted": false
    })))
}

#[derive(Default)]
struct SessionMetadataSummary {
    title: String,
    first_user_message: String,
    first_assistant_message: String,
    project_path: String,
    model: String,
    token_count: u64,
    message_count: usize,
    has_conversation: bool,
}

fn scan_session_metadata_content(
    raw_path: &str,
    cli: &str,
    content: &str,
) -> Result<SessionMetadataSummary, ManagerError> {
    let extension = Path::new(raw_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mut summary = SessionMetadataSummary::default();

    if extension == "json" {
        let payload: Value = serde_json::from_str(content)?;

        if payload.is_object() {
            append_session_metadata_fields(&mut summary, &payload);
        }

        for record in payload
            .as_array()
            .or_else(|| payload.get("messages").and_then(Value::as_array))
            .into_iter()
            .flatten()
        {
            append_session_metadata_record(&mut summary, record, cli);
        }
        return Ok(summary);
    }

    if extension == "md" && !content.to_lowercase().contains("messages") {
        return Ok(summary);
    }

    for line in content.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Ok(record) = serde_json::from_str::<Value>(line) {
            append_session_metadata_record(&mut summary, &record, cli);
        }
    }
    Ok(summary)
}

fn append_session_metadata_record(
    summary: &mut SessionMetadataSummary,
    record: &Value,
    cli: &str,
) {
    append_session_metadata_fields(summary, record);

    let payload = record.get("payload").unwrap_or(record);

    if cli == "codex"
        && !matches!(
            payload.get("type").and_then(Value::as_str),
            Some("message") | Some("function_call")
        )
    {
        return;
    }

    let message = normalize_message(record);
    let content = string_value(message.get("content"));
    let has_tool_calls = message
        .get("toolCalls")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty());

    if content.is_empty() && !has_tool_calls {
        return;
    }

    summary.message_count += 1;

    match message.get("role").and_then(Value::as_str) {
        Some("user") => {
            summary.has_conversation = true;
            if summary.first_user_message.is_empty() {
                summary.first_user_message = content;
            }
        }
        Some("assistant") => {
            summary.has_conversation = true;
            if summary.first_assistant_message.is_empty() {
                summary.first_assistant_message = content;
            }
        }
        Some("tool") => summary.has_conversation = true,
        _ => {}
    }
}

fn append_session_metadata_fields(summary: &mut SessionMetadataSummary, record: &Value) {
    let payload = record.get("payload").unwrap_or(record);
    let metadata = payload.get("metadata");

    if summary.title.is_empty() {
        summary.title = first_string(
            payload.get("title"),
            metadata.and_then(|item| item.get("title")),
        );
    }
    if summary.project_path.is_empty() {
        summary.project_path = first_string(
            payload
                .get("cwd")
                .or_else(|| payload.get("workspace"))
                .or_else(|| payload.get("projectPath")),
            metadata.and_then(|item| {
                item.get("cwd")
                    .or_else(|| item.get("workspace"))
                    .or_else(|| item.get("projectPath"))
            }),
        );
    }
    if summary.model.is_empty() {
        summary.model = first_string(
            payload
                .get("model")
                .or_else(|| payload.get("message").and_then(|item| item.get("model"))),
            metadata.and_then(|item| item.get("model")),
        );
    }
    if summary.token_count == 0 {
        summary.token_count = payload
            .get("tokenCount")
            .and_then(Value::as_u64)
            .or_else(|| {
                payload
                    .get("usage")
                    .and_then(|item| item.get("total_tokens"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0);
    }
}

pub async fn search_sessions(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let query = string_value(payload.get("query"));
    let keyword = query.to_lowercase();
    let sessions = read_sessions(paths)?;

    if keyword.trim().is_empty() {
        return Ok(json!(sessions));
    }

    let mut results = Vec::new();

    for session in sessions {
        let metadata_text = [
            string_value(session.get("title")),
            string_value(session.get("summary")),
            string_value(session.get("projectName")),
            string_value(session.get("projectPath")),
            string_value(session.get("model")),
            string_value(session.get("cliName")),
        ]
        .join(" ")
        .to_lowercase();

        if metadata_text.contains(&keyword) {
            results.push(session);
            continue;
        }

        let messages = load_messages_for_session(&session).await?;
        let message_text = messages
            .iter()
            .map(|item| {
                let mut parts = vec![
                    string_value(item.get("role")),
                    string_value(item.get("content")),
                ];

                if let Some(tool_calls) = item.get("toolCalls").and_then(Value::as_array) {
                    for tool in tool_calls {
                        parts.push(
                            [
                                string_value(tool.get("name")),
                                string_value(tool.get("arguments")),
                                string_value(tool.get("result")),
                            ]
                            .join(" "),
                        );
                    }
                }

                if let Some(files) = item.get("files").and_then(Value::as_array) {
                    for file in files {
                        parts.push(string_value(Some(file)));
                    }
                }

                parts.join(" ")
            })
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();

        if message_text.contains(&keyword) {
            results.push(session);
        }
    }

    Ok(json!(results))
}

pub async fn load_session_messages(
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let session_id = string_value(payload.get("sessionId"));
    let session = read_sessions(paths)?
        .into_iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(session_id.as_str()))
        .ok_or_else(|| ManagerError::System("Session 不存在".to_string()))?;

    Ok(json!(load_messages_for_session(&session).await?))
}

pub async fn delete_session(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let session_id = string_value(payload.get("sessionId"));
    let mut sessions = read_sessions(paths)?;
    let session = sessions
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(session_id.as_str()))
        .cloned()
        .ok_or_else(|| ManagerError::System("Session 不存在".to_string()))?;
    let recycled_path = get_recycle_session_path(paths, &session);
    let cli = string_value(session.get("cli"));
    let cli_target = cli_targets
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| string_value(item.get("id")) == cli)
        })
        .cloned()
        .unwrap_or_else(|| json!({}));
    let cli_config_path = string_value(cli_target.get("configPath"));
    let cli_executable_path = string_value(cli_target.get("executablePath"));

    if ["claude", "codex"].contains(&cli.as_str()) && cli_config_path.is_empty() {
        return Err(ManagerError::System(format!(
            "{} 配置目录未识别，无法同步 Session 状态。",
            first_string(session.get("cliName"), session.get("cli"))
        )));
    }
    let cli_session_id = read_cli_session_id(&session).await?;
    let mut metadata = {
        let mut metadata = session.clone();

        metadata["originalPath"] = session.get("rawPath").cloned().unwrap_or(Value::Null);
        metadata["recycledPath"] = json!(recycled_path.to_string_lossy().to_string());
        metadata["recycledAt"] = json!(now_millis());
        metadata["cliConfigPath"] = json!(cli_config_path);
        metadata["cliExecutablePath"] = json!(cli_executable_path);
        metadata["cliSessionId"] = json!(cli_session_id);
        metadata["cliStateSynchronized"] = json!(true);
        metadata
    };

    if let Some(parent) = recycled_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if cli == "codex" && !cli_session_id.is_empty() {
        run_codex_session_command(
            &cli_executable_path,
            &cli_config_path,
            "archive",
            &cli_session_id,
        )
        .await?;
        let archived_path = Path::new(&cli_config_path)
            .join("archived_sessions")
            .join(
                Path::new(&string_value(session.get("rawPath")))
                    .file_name()
                    .unwrap_or_default(),
            );

        if !archived_path.exists() {
            return Err(ManagerError::System(
                "Codex 已归档 Session，但未找到归档文件。".to_string(),
            ));
        }
        move_file(&archived_path, &recycled_path).await?;
        metadata["cliArchivedPath"] = json!(archived_path.to_string_lossy().to_string());
    } else {
        move_file(string_value(session.get("rawPath")), &recycled_path).await?;
    }
    metadata["cliHistoryEntries"] = json!(
        remove_cli_history_entries(&cli_config_path, &cli_session_id).await?
    );
    write_json(get_recycle_metadata_path(paths, &session_id), &metadata).await?;
    sessions.retain(|item| item.get("id").and_then(Value::as_str) != Some(session_id.as_str()));
    write_json(&paths.storage_files.sessions, &json!(sessions)).await
}

pub async fn list_recycled_sessions(paths: &AppPaths) -> Result<Value, ManagerError> {
    let metadata_dir = Path::new(&paths.session_recycle_metadata_dir);
    let mut sessions = Vec::new();

    if !metadata_dir.exists() {
        return Ok(json!(sessions));
    }

    for entry in std::fs::read_dir(metadata_dir)? {
        let entry = entry?;
        let entry_path = entry.path();

        if !entry_path.is_file()
            || entry_path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            continue;
        }

        sessions.push(serde_json::from_str::<Value>(&std::fs::read_to_string(
            entry_path,
        )?)?);
    }

    sessions.sort_by(|left, right| {
        right
            .get("recycledAt")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .cmp(&left.get("recycledAt").and_then(Value::as_u64).unwrap_or(0))
    });

    Ok(json!(sessions))
}

pub async fn restore_session(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let session_id = string_value(payload.get("sessionId"));
    let metadata_path = get_recycle_metadata_path(paths, &session_id);
    let metadata: Value = serde_json::from_str(&tokio::fs::read_to_string(&metadata_path).await?)?;
    let original_path = PathBuf::from(string_value(metadata.get("originalPath")));
    let archived_path = PathBuf::from(string_value(metadata.get("cliArchivedPath")));
    let cli_session_id = string_value(metadata.get("cliSessionId"));

    if string_value(metadata.get("cli")) == "codex"
        && !cli_session_id.is_empty()
        && !archived_path.as_os_str().is_empty()
    {
        if let Some(parent) = archived_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        move_file(string_value(metadata.get("recycledPath")), &archived_path).await?;
        if let Err(error) = run_codex_session_command(
            &string_value(metadata.get("cliExecutablePath")),
            &string_value(metadata.get("cliConfigPath")),
            "unarchive",
            &cli_session_id,
        )
        .await
        {
            move_file(&archived_path, string_value(metadata.get("recycledPath"))).await?;
            return Err(error);
        }
    } else {
        if let Some(parent) = original_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        move_file(string_value(metadata.get("recycledPath")), &original_path).await?;
    }
    restore_cli_history_entries(&metadata).await?;
    tokio::fs::remove_file(metadata_path).await?;
    Ok(())
}

pub async fn purge_session(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let session_id = string_value(payload.get("sessionId"));
    let metadata_path = get_recycle_metadata_path(paths, &session_id);
    let metadata: Value = serde_json::from_str(&tokio::fs::read_to_string(&metadata_path).await?)?;

    remove_file_if_exists(string_value(metadata.get("recycledPath"))).await?;
    remove_file_if_exists(metadata_path).await
}

async fn load_messages_for_session(session: &Value) -> Result<Vec<Value>, ManagerError> {
    Ok(parse_session_file(session).await?.0)
}

async fn parse_session_file(session: &Value) -> Result<(Vec<Value>, Vec<Value>), ManagerError> {
    let raw_path = string_value(session.get("rawPath"));
    let content = tokio::fs::read_to_string(&raw_path).await?;
    let extension = Path::new(&raw_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if extension == "json" {
        let payload: Value = serde_json::from_str(&content)?;
        let items = if let Some(items) = payload.as_array() {
            items.clone()
        } else {
            payload
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let mut records = if payload.is_object() {
            vec![payload]
        } else {
            Vec::new()
        };

        records.extend(items.clone());

        return Ok((
            items
                .iter()
                .map(normalize_message)
                .filter(|item| !string_value(item.get("content")).is_empty())
                .collect(),
            records,
        ));
    }

    if extension == "md" && !content.to_lowercase().contains("messages") {
        return Ok((Vec::new(), Vec::new()));
    }

    let records = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let cli = string_value(session.get("cli"));

    Ok((
        records
            .iter()
            .filter(|record| {
                if cli != "codex" {
                    return true;
                }

                let payload = record.get("payload").unwrap_or(record);
                matches!(
                    payload.get("type").and_then(Value::as_str),
                    Some("message") | Some("function_call")
                )
            })
            .map(normalize_message)
            .filter(|item| {
                !string_value(item.get("content")).is_empty()
                    || item
                        .get("toolCalls")
                        .and_then(Value::as_array)
                        .is_some_and(|items| !items.is_empty())
            })
            .collect(),
        records,
    ))
}

async fn read_cli_session_id(session: &Value) -> Result<String, ManagerError> {
    let (_, records) = parse_session_file(session).await?;

    for record in records {
        let payload = record.get("payload").unwrap_or(&record);
        let value = first_string(
            record
                .get("sessionId")
                .or_else(|| record.get("session_id"))
                .or_else(|| {
                    (record.get("type").and_then(Value::as_str) == Some("session_meta"))
                        .then(|| payload.get("id"))
                        .flatten()
                }),
            payload
                .get("sessionId")
                .or_else(|| payload.get("session_id")),
        );

        if !value.is_empty() {
            return Ok(value);
        }
    }

    Ok(String::new())
}

async fn remove_cli_history_entries(
    config_path: &str,
    session_id: &str,
) -> Result<Vec<String>, ManagerError> {
    if config_path.is_empty() || session_id.is_empty() {
        return Ok(Vec::new());
    }

    let history_path = Path::new(config_path).join("history.jsonl");
    let content = match tokio::fs::read_to_string(&history_path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ManagerError::Io(error)),
    };
    let mut retained = Vec::new();
    let mut removed = Vec::new();

    for line in content.lines() {
        if history_line_session_id(line).as_deref() == Some(session_id) {
            removed.push(line.to_string());
        } else {
            retained.push(line.to_string());
        }
    }

    if !removed.is_empty() {
        write_json_lines(&history_path, &retained).await?;
    }
    Ok(removed)
}

async fn restore_cli_history_entries(metadata: &Value) -> Result<(), ManagerError> {
    let entries = metadata
        .get("cliHistoryEntries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(ToString::to_string))
        .collect::<Vec<_>>();
    let config_path = string_value(metadata.get("cliConfigPath"));
    let session_id = string_value(metadata.get("cliSessionId"));

    if entries.is_empty() || config_path.is_empty() || session_id.is_empty() {
        return Ok(());
    }

    let history_path = Path::new(&config_path).join("history.jsonl");
    let current = match tokio::fs::read_to_string(&history_path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(ManagerError::Io(error)),
    };
    let mut lines = current
        .lines()
        .filter(|line| history_line_session_id(line).as_deref() != Some(session_id.as_str()))
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    lines.extend(entries);
    write_json_lines(&history_path, &lines).await
}

fn history_line_session_id(line: &str) -> Option<String> {
    let record = serde_json::from_str::<Value>(line).ok()?;

    record
        .get("session_id")
        .or_else(|| record.get("sessionId"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

async fn write_json_lines(path: &Path, lines: &[String]) -> Result<(), ManagerError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    tokio::fs::write(path, content).await?;
    Ok(())
}

async fn run_codex_session_command(
    executable_path: &str,
    config_path: &str,
    action: &str,
    session_id: &str,
) -> Result<(), ManagerError> {
    if config_path.is_empty() {
        return Err(ManagerError::System(
            "Codex 配置目录未识别，无法同步 Session 状态。".to_string(),
        ));
    }

    let executable = Path::new(executable_path);
    let mut command = Command::new(
        if executable_path.is_empty() || (executable.is_absolute() && !executable.exists()) {
            "codex"
        } else {
            executable_path
        },
    );

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .env("CODEX_HOME", config_path)
        .arg(action)
        .arg(session_id)
        .kill_on_drop(true)
        .output()
        .await?;

    if output.status.success() {
        return Ok(());
    }

    let message = if output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    };
    Err(ManagerError::System(if message.is_empty() {
        format!("Codex {action} Session 失败。")
    } else {
        message
    }))
}

fn normalize_message(record: &Value) -> Value {
    let payload = record.get("payload").unwrap_or(record);
    let message = payload.get("message").unwrap_or(payload);
    let role = normalize_role(first_string(
        message.get("role"),
        payload.get("role").or_else(|| payload.get("type")),
    ));
    let timestamp = first_string(
        record.get("timestamp"),
        payload
            .get("timestamp")
            .or_else(|| message.get("timestamp"))
            .or_else(|| payload.get("createdAt")),
    );
    let tool_calls = extract_tool_calls(record);
    let content = if payload.get("type").and_then(Value::as_str) == Some("function_call") {
        format!(
            "{}\n{}",
            first_string(payload.get("name"), payload.get("call_id")),
            string_value(payload.get("arguments"))
        )
        .trim()
        .to_string()
    } else {
        normalize_content(message.get("content").or_else(|| payload.get("content")))
    };

    json!({
      "role": role,
      "content": content,
      "timestamp": parse_timestamp(&timestamp),
      "toolCalls": tool_calls,
      "files": record.get("files").and_then(Value::as_array).cloned().unwrap_or_default()
    })
}

fn extract_tool_calls(record: &Value) -> Vec<Value> {
    let payload = record.get("payload").unwrap_or(record);
    let content = payload
        .get("message")
        .and_then(|message| message.get("content"))
        .or_else(|| payload.get("content"));

    if !content.is_some_and(Value::is_array) {
        if payload.get("type").and_then(Value::as_str) == Some("function_call") {
            return vec![json!({
              "name": first_string(payload.get("name"), payload.get("call_id")),
              "arguments": payload.get("arguments").cloned().unwrap_or(Value::Null),
              "result": Value::Null
            })];
        }

        return Vec::new();
    }

    content
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("tool_use") | Some("tool_result")
            )
        })
        .map(|item| {
            json!({
              "name": first_string(item.get("name"), item.get("toolName").or_else(|| item.get("type"))),
              "arguments": item.get("input").map(Value::to_string).unwrap_or_default(),
              "result": normalize_content(item.get("content"))
            })
        })
        .collect()
}

fn normalize_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                if let Some(text) = item.as_str() {
                    return text.to_string();
                }

                first_string(
                    item.get("text"),
                    item.get("content")
                        .or_else(|| item.get("input"))
                        .or_else(|| item.get("result")),
                )
            })
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Some(Value::Object(_)) => first_string(
            content.and_then(|item| item.get("text")),
            content.and_then(|item| item.get("content")),
        )
        .if_empty_with(|| content.map(Value::to_string).unwrap_or_default()),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn normalize_role(value: String) -> String {
    match value.as_str() {
        "function_call" | "tool_use" | "tool_result" => "tool".to_string(),
        "user" | "assistant" | "tool" | "system" => value,
        _ => "system".to_string(),
    }
}

fn get_recycle_session_path(paths: &AppPaths, session: &Value) -> PathBuf {
    let raw_path = string_value(session.get("rawPath"));
    let extension = Path::new(&raw_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value))
        .unwrap_or_else(|| ".session".to_string());

    Path::new(&paths.session_recycle_sessions_dir).join(format!(
        "{}{}",
        string_value(session.get("id")),
        extension
    ))
}

fn get_recycle_metadata_path(paths: &AppPaths, session_id: &str) -> PathBuf {
    Path::new(&paths.session_recycle_metadata_dir).join(format!("{}.json", session_id))
}

fn read_sessions(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    match std::fs::read_to_string(&paths.storage_files.sessions) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn write_json(path: impl AsRef<Path>, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = path.as_ref().parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )
    .await?;
    Ok(())
}

async fn move_file(
    source_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
) -> Result<(), ManagerError> {
    tokio::fs::copy(source_path.as_ref(), target_path.as_ref()).await?;
    tokio::fs::remove_file(source_path.as_ref()).await?;
    Ok(())
}

async fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<(), ManagerError> {
    match tokio::fs::remove_file(path).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn first_string(value: Option<&Value>, fallback: Option<&Value>) -> String {
    let text = string_value(value);

    if text.is_empty() {
        string_value(fallback)
    } else {
        text
    }
}

fn parse_timestamp(value: &str) -> Value {
    value.parse::<u64>().map(Value::from).unwrap_or(Value::Null)
}

fn create_session_id(raw_path: &str) -> String {
    let path = Path::new(raw_path);
    let normalized_path = if path.is_absolute() {
        raw_path.to_string()
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join(path)
            .to_string_lossy()
            .to_string()
    };
    let mut hasher = Sha1::new();

    hasher.update(normalized_path.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn truncate_text(value: &str, length: usize) -> String {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");

    if text.chars().count() > length {
        format!("{}...", text.chars().take(length).collect::<String>())
    } else {
        text
    }
}

fn system_time_millis(value: SystemTime) -> Option<u64> {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

trait EmptyStringExt {
    fn if_empty_with(self, fallback: impl FnOnce() -> String) -> String;
}

impl EmptyStringExt for String {
    fn if_empty_with(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        reconcile_recycled_cli_state, refresh_sessions_state, remove_cli_history_entries,
        restore_cli_history_entries,
    };
    use crate::core::paths::resolve_app_paths;
    use serde_json::{json, Value};
    use std::path::Path;

    #[test]
    fn refreshes_device_sessions_and_reports_invalid_files() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "monkey-thief-session-refresh-{}-{}",
                std::process::id(),
                super::now_millis()
            ));
            let paths = resolve_app_paths(Path::new(&root));
            let source_dir = root.join("codex-sessions");
            let valid_path = source_dir.join("valid.jsonl");

            std::fs::create_dir_all(&source_dir).unwrap();
            std::fs::write(
                &valid_path,
                concat!(
                    "{\"timestamp\":\"1\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"修复恢复问题\",\"model\":\"gpt-test\",\"cwd\":\"D:\\\\project\"}}\n",
                    "{\"timestamp\":\"2\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"已经完成\"}}\n"
                ),
            )
            .unwrap();
            std::fs::write(source_dir.join("invalid.json"), "{").unwrap();
            std::fs::create_dir_all(&paths.temp_dir).unwrap();
            std::fs::write(
                &paths.storage_files.sessions,
                "[{\"id\":\"stale\",\"rawPath\":\"missing.jsonl\"}]\n",
            )
            .unwrap();
            let mut state = json!({
              "cliTargets": [{
                "id": "codex",
                "type": "codex",
                "name": "Codex",
                "sessionPaths": [source_dir.to_string_lossy().to_string()]
              }],
              "sessions": [],
              "diagnostics": []
            });

            refresh_sessions_state(&paths, &mut state).await.unwrap();

            assert_eq!(state["sessions"].as_array().unwrap().len(), 1);
            assert_eq!(state["sessions"][0]["title"], "修复恢复问题");
            assert_eq!(state["sessions"][0]["summary"], "已经完成");
            assert_eq!(state["sessions"][0]["model"], "gpt-test");
            assert_eq!(state["diagnostics"].as_array().unwrap().len(), 1);
            let stored: Value = serde_json::from_str(
                &std::fs::read_to_string(&paths.storage_files.sessions).unwrap(),
            )
            .unwrap();
            assert_eq!(stored.as_array().unwrap().len(), 1);
            assert_eq!(stored[0]["rawPath"], valid_path.to_string_lossy().as_ref());

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn reuses_unchanged_session_metadata_without_reparsing() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "monkey-thief-session-cache-{}-{}",
                std::process::id(),
                super::now_millis()
            ));
            let paths = resolve_app_paths(Path::new(&root));
            let source_dir = root.join("codex-sessions");
            let session_path = source_dir.join("cached.jsonl");

            std::fs::create_dir_all(&source_dir).unwrap();
            std::fs::write(&session_path, "invalid json that must not be reparsed\n").unwrap();
            let updated_at = std::fs::metadata(&session_path)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let mut state = json!({
              "cliTargets": [{
                "id": "codex",
                "type": "codex",
                "name": "Codex",
                "sessionPaths": [source_dir.to_string_lossy().to_string()]
              }],
              "sessions": [{
                "id": "cached-session",
                "cli": "codex",
                "title": "缓存标题",
                "rawPath": session_path.to_string_lossy().to_string(),
                "updatedAt": updated_at
              }],
              "diagnostics": []
            });

            refresh_sessions_state(&paths, &mut state).await.unwrap();

            assert_eq!(state["sessions"].as_array().unwrap().len(), 1);
            assert_eq!(state["sessions"][0]["title"], "缓存标题");
            assert!(state["diagnostics"].as_array().unwrap().is_empty());

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn removes_and_restores_cli_history_entries() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "monkey-thief-session-history-{}-{}",
                std::process::id(),
                super::now_millis()
            ));
            let history_path = root.join("history.jsonl");

            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                &history_path,
                concat!(
                    "{\"session_id\":\"session-a\",\"text\":\"first\"}\n",
                    "{\"session_id\":\"session-b\",\"text\":\"keep\"}\n",
                    "{\"session_id\":\"session-a\",\"text\":\"second\"}\n"
                ),
            )
            .unwrap();

            let removed = remove_cli_history_entries(
                root.to_string_lossy().as_ref(),
                "session-a",
            )
            .await
            .unwrap();

            assert_eq!(removed.len(), 2);
            assert!(!std::fs::read_to_string(&history_path)
                .unwrap()
                .contains("session-a"));
            let metadata = json!({
              "cliConfigPath": root.to_string_lossy().to_string(),
              "cliSessionId": "session-a",
              "cliHistoryEntries": removed
            });

            restore_cli_history_entries(&metadata).await.unwrap();
            let restored = std::fs::read_to_string(&history_path).unwrap();

            assert_eq!(restored.matches("session-a").count(), 2);
            assert_eq!(restored.matches("session-b").count(), 1);

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn migrates_legacy_claude_recycle_metadata() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "monkey-thief-session-recycle-migration-{}-{}",
                std::process::id(),
                super::now_millis()
            ));
            let paths = resolve_app_paths(Path::new(&root));
            let config_path = root.join("claude");
            let recycled_path = Path::new(&paths.session_recycle_sessions_dir).join("session.jsonl");
            let metadata_path = Path::new(&paths.session_recycle_metadata_dir).join("session.json");

            std::fs::create_dir_all(recycled_path.parent().unwrap()).unwrap();
            std::fs::create_dir_all(metadata_path.parent().unwrap()).unwrap();
            std::fs::create_dir_all(&config_path).unwrap();
            std::fs::write(
                &recycled_path,
                "{\"sessionId\":\"claude-session\",\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
            )
            .unwrap();
            std::fs::write(
                config_path.join("history.jsonl"),
                "{\"sessionId\":\"claude-session\",\"display\":\"hello\"}\n",
            )
            .unwrap();
            std::fs::write(
                &metadata_path,
                format!(
                    "{{\"id\":\"session\",\"cli\":\"claude\",\"originalPath\":\"{}\",\"recycledPath\":\"{}\"}}",
                    config_path.join("projects/session.jsonl").to_string_lossy().replace('\\', "\\\\"),
                    recycled_path.to_string_lossy().replace('\\', "\\\\")
                ),
            )
            .unwrap();
            let cli_targets = vec![json!({
              "id": "claude",
              "configPath": config_path.to_string_lossy().to_string()
            })];
            let mut diagnostics = Vec::new();

            reconcile_recycled_cli_state(&paths, &cli_targets, &mut diagnostics)
                .await
                .unwrap();

            assert!(diagnostics.is_empty());
            assert!(std::fs::read_to_string(config_path.join("history.jsonl"))
                .unwrap()
                .is_empty());
            let metadata: Value =
                serde_json::from_str(&std::fs::read_to_string(metadata_path).unwrap()).unwrap();
            assert_eq!(metadata["cliSessionId"], "claude-session");
            assert_eq!(metadata["cliStateSynchronized"], true);
            assert_eq!(metadata["cliHistoryEntries"].as_array().unwrap().len(), 1);

            let _ = std::fs::remove_dir_all(root);
        });
    }
}

use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use crate::core::settings::string_value;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

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

pub async fn delete_session(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let session_id = string_value(payload.get("sessionId"));
    let mut sessions = read_sessions(paths)?;
    let session = sessions
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(session_id.as_str()))
        .cloned()
        .ok_or_else(|| ManagerError::System("Session 不存在".to_string()))?;
    let recycled_path = get_recycle_session_path(paths, &session);
    let metadata = {
        let mut metadata = session.clone();

        metadata["originalPath"] = session.get("rawPath").cloned().unwrap_or(Value::Null);
        metadata["recycledPath"] = json!(recycled_path.to_string_lossy().to_string());
        metadata["recycledAt"] = json!(now_millis());
        metadata
    };

    if let Some(parent) = recycled_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    move_file(string_value(session.get("rawPath")), recycled_path).await?;
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

    if let Some(parent) = original_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    move_file(string_value(metadata.get("recycledPath")), original_path).await?;
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

        return Ok(items
            .iter()
            .map(normalize_message)
            .filter(|item| !string_value(item.get("content")).is_empty())
            .collect());
    }

    if extension == "md" && !content.to_lowercase().contains("messages") {
        return Ok(Vec::new());
    }

    let records = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let cli = string_value(session.get("cli"));

    Ok(records
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
        .collect())
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

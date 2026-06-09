use crate::core::error::ManagerError;
use crate::core::paths::path_text;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};
use tokio::sync::Mutex;

static LOG_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static LOG_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub struct AppLogStart {
    pub trace_id: String,
    pub started_at: std::time::Instant,
}

pub fn log_path(user_data_path: &Path) -> PathBuf {
    user_data_path
        .join("workspace")
        .join("logs")
        .join("app-call-logs.json")
}

pub async fn list_logs(user_data_path: &Path) -> Result<Value, ManagerError> {
    let _guard = log_file_lock().lock().await;
    let file_path = log_path(user_data_path);
    migrate_legacy_logs(user_data_path, &file_path).await?;
    let logs = read_logs(&file_path).await?;

    Ok(json!({
      "logs": logs,
      "filePath": path_text(file_path)
    }))
}

pub async fn clear_logs(user_data_path: &Path) -> Result<Value, ManagerError> {
    let _guard = log_file_lock().lock().await;
    let file_path = log_path(user_data_path);

    migrate_legacy_logs(user_data_path, &file_path).await?;
    write_logs(&file_path, &json!([])).await?;
    Ok(json!({
      "logs": [],
      "filePath": path_text(file_path)
    }))
}

pub async fn record_start(
    user_data_path: &Path,
    channel: &str,
    payload: Option<&Value>,
) -> Result<AppLogStart, ManagerError> {
    let trace_id = create_log_id();
    let started_at = std::time::Instant::now();

    append_log(
        user_data_path,
        json!({
          "id": create_log_id(),
          "traceId": trace_id,
          "scope": "backend",
          "service": "IpcMain",
          "method": channel,
          "channel": channel,
          "action": "start",
          "status": "pending",
          "durationMs": 0,
          "message": "",
          "payload": sanitize_log_value(payload.cloned().unwrap_or(Value::Null)),
          "result": null,
          "createdAt": now_millis()
        }),
    )
    .await?;

    Ok(AppLogStart {
        trace_id,
        started_at,
    })
}

pub async fn record_finish(
    user_data_path: &Path,
    channel: &str,
    started: AppLogStart,
    result: &Result<Value, String>,
) -> Result<(), ManagerError> {
    match result {
        Ok(value) => {
            append_log(
                user_data_path,
                json!({
                  "id": create_log_id(),
                  "traceId": started.trace_id,
                  "scope": "backend",
                  "service": "IpcMain",
                  "method": channel,
                  "channel": channel,
                  "action": "finish",
                  "status": "success",
                  "durationMs": started.started_at.elapsed().as_millis(),
                  "message": "",
                  "payload": null,
                  "result": summarize_log_value(value),
                  "createdAt": now_millis()
                }),
            )
            .await
        }
        Err(message) => {
            append_log(
                user_data_path,
                json!({
                  "id": create_log_id(),
                  "traceId": started.trace_id,
                  "scope": "backend",
                  "service": "IpcMain",
                  "method": channel,
                  "channel": channel,
                  "action": "finish",
                  "status": "error",
                  "durationMs": started.started_at.elapsed().as_millis(),
                  "message": message,
                  "payload": null,
                  "result": null,
                  "createdAt": now_millis()
                }),
            )
            .await
        }
    }
}

fn legacy_log_path(user_data_path: &Path) -> PathBuf {
    user_data_path
        .join("workspace")
        .join("storage")
        .join("app-call-logs.json")
}

async fn migrate_legacy_logs(user_data_path: &Path, file_path: &Path) -> Result<(), ManagerError> {
    let source_path = legacy_log_path(user_data_path);

    if !source_path.exists() {
        return Ok(());
    }

    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    if !file_path.exists() {
        tokio::fs::rename(&source_path, file_path).await?;
        return Ok(());
    }

    let source_items = read_logs(&source_path).await?;
    let target_items = read_logs(file_path).await?;

    if !source_items.is_array() || !target_items.is_array() {
        tokio::fs::remove_file(source_path).await?;
        return Ok(());
    }

    let mut merged = target_items.as_array().cloned().unwrap_or_default();

    for item in source_items.as_array().cloned().unwrap_or_default() {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("");

        if !merged
            .iter()
            .any(|target| target.get("id").and_then(Value::as_str) == Some(id))
        {
            merged.push(item);
        }
    }

    write_logs(file_path, &json!(merged)).await?;
    tokio::fs::remove_file(source_path).await?;
    Ok(())
}

async fn append_log(user_data_path: &Path, entry: Value) -> Result<(), ManagerError> {
    let _guard = log_file_lock().lock().await;
    let file_path = log_path(user_data_path);

    migrate_legacy_logs(user_data_path, &file_path).await?;

    let mut logs = read_logs(&file_path)
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default();

    logs.insert(0, entry);
    logs.truncate(1000);
    write_logs(&file_path, &json!(logs)).await
}

fn sanitize_log_value(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if is_sensitive_key(&key) {
                        (key, if value.is_null() { value } else { json!("***") })
                    } else {
                        (key, sanitize_log_value(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(sanitize_log_value).collect()),
        value => value,
    }
}

fn summarize_log_value(value: &Value) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(text) => {
            if text.chars().count() > 500 {
                json!(format!("{}...", text.chars().take(500).collect::<String>()))
            } else {
                value.clone()
            }
        }
        Value::Array(items) => json!({
          "type": "array",
          "length": items.len()
        }),
        Value::Object(map) => json!({
          "type": "object",
          "keys": map.keys().take(30).cloned().collect::<Vec<_>>()
        }),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_lowercase();

    key.contains("password")
        || key.contains("token")
        || key.contains("key")
        || key.contains("secret")
}

async fn read_logs(file_path: &Path) -> Result<Value, ManagerError> {
    match tokio::fs::read_to_string(file_path).await {
        Ok(content) => {
            let (logs, has_trailing_content) = parse_log_content(&content)?;

            if has_trailing_content && logs.is_array() {
                write_logs(file_path, &logs).await?;
            }

            Ok(logs)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!([])),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn parse_log_content(content: &str) -> Result<(Value, bool), serde_json::Error> {
    let mut stream = serde_json::Deserializer::from_str(content).into_iter::<Value>();
    let value = match stream.next() {
        Some(result) => result?,
        None => Value::Null,
    };
    let trailing = !content[stream.byte_offset()..].trim().is_empty();

    Ok((value, trailing))
}

fn create_log_id() -> String {
    format!(
        "{}-{}-{}",
        now_millis(),
        std::process::id(),
        LOG_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

async fn write_logs(file_path: &Path, logs: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        file_path,
        format!("{}\n", serde_json::to_string_pretty(logs)?),
    )
    .await?;
    Ok(())
}

fn log_file_lock() -> &'static Mutex<()> {
    LOG_FILE_LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::parse_log_content;

    #[test]
    fn parses_log_content_without_trailing_text() {
        let (value, trailing) = parse_log_content("[{\"id\":1}]").unwrap();

        assert!(value.is_array());
        assert!(!trailing);
    }

    #[test]
    fn parses_log_content_with_trailing_text() {
        let (value, trailing) = parse_log_content("[{\"id\":1}]\n\"").unwrap();

        assert!(value.is_array());
        assert!(trailing);
    }
}

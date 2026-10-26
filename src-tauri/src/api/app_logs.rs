use crate::core::error::ManagerError;
use crate::core::paths::path_text;
use chrono::{Local, NaiveDate};
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

const LOG_RETENTION_DAYS: i64 = 7;
const LOG_RETAIN_COUNT: usize = 1000;

static LOG_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static LOG_SESSION_ID: OnceLock<String> = OnceLock::new();
static LOG_ENABLED: AtomicBool = AtomicBool::new(true);

pub struct AppLogStart {
    pub trace_id: String,
    pub started_at: std::time::Instant,
    pub payload: Value,
    pub context: Value,
}

pub fn log_path(user_data_path: &Path) -> PathBuf {
    user_data_path.join("workspace").join("logs")
}

pub fn set_enabled(enabled: bool) {
    LOG_ENABLED.store(enabled, Ordering::SeqCst);
}

// 等待已开始的写入完成，确保关闭开关返回后不会继续落盘。
pub async fn configure(enabled: bool) {
    let _guard = log_file_lock().lock().await;
    set_enabled(enabled);
}

pub async fn list_logs(user_data_path: &Path, date: Option<&str>) -> Result<Value, ManagerError> {
    let _guard = log_file_lock().lock().await;
    let files = maintain_logs_locked(user_data_path, Local::now().date_naive()).await?;
    let date = date.filter(|value| !value.is_empty());
    if let Some(date) = date {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| ManagerError::System("日志日期无效".into()))?;
    }
    let mut logs = Vec::new();
    let mut has_more = false;
    for (file_date, file_path, _) in &files {
        if date.is_some_and(|date| file_date.to_string() != date) {
            continue;
        }
        let items = read_daily_logs(file_path, LOG_RETAIN_COUNT + 1).await?;
        logs.extend(items);
        if logs.len() > LOG_RETAIN_COUNT {
            has_more = true;
            logs.truncate(LOG_RETAIN_COUNT);
            break;
        }
    }

    Ok(json!({
      "logs": logs,
      "filePath": path_text(log_path(user_data_path)),
      "dates": files.iter().map(|(date, _, _)| date.to_string()).collect::<Vec<_>>(),
      "date": date,
      "retentionDays": LOG_RETENTION_DAYS,
      "limit": LOG_RETAIN_COUNT,
      "hasMore": has_more
    }))
}

pub async fn clear_logs(user_data_path: &Path) -> Result<Value, ManagerError> {
    let _guard = log_file_lock().lock().await;
    let files = maintain_logs_locked(user_data_path, Local::now().date_naive()).await?;
    for (_, file_path, _) in files {
        tokio::fs::remove_file(file_path).await?;
    }
    Ok(json!({
      "logs": [],
      "filePath": path_text(log_path(user_data_path)),
      "dates": [],
      "hasMore": false
    }))
}

pub async fn stats(user_data_path: &Path) -> Result<Value, ManagerError> {
    let _guard = log_file_lock().lock().await;
    let files = maintain_logs_locked(user_data_path, Local::now().date_naive()).await?;
    Ok(json!({
      "enabled": LOG_ENABLED.load(Ordering::SeqCst),
      "bytes": files.iter().map(|(_, _, bytes)| bytes).sum::<u64>(),
      "fileCount": files.len(),
      "retentionDays": LOG_RETENTION_DAYS,
      "directory": path_text(log_path(user_data_path))
    }))
}

pub async fn cleanup(user_data_path: &Path) -> Result<(), ManagerError> {
    let _guard = log_file_lock().lock().await;
    maintain_logs_locked(user_data_path, Local::now().date_naive()).await?;
    Ok(())
}

pub fn begin_call(payload: Option<&Value>, window: &str, context: Option<Value>) -> AppLogStart {
    let mut context = context
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let trace_id = context["traceId"]
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned)
        .unwrap_or_else(create_log_id);
    context["window"] = json!(window);
    context["sessionId"] = json!(LOG_SESSION_ID.get_or_init(create_log_id));
    context["appVersion"] = json!(env!("CARGO_PKG_VERSION"));
    context["platform"] = json!(std::env::consts::OS);
    context["arch"] = json!(std::env::consts::ARCH);
    context["processId"] = json!(std::process::id());

    AppLogStart {
        trace_id,
        started_at: std::time::Instant::now(),
        payload: sanitize_log_value(payload.unwrap_or(&Value::Null)),
        context: sanitize_log_value(&context),
    }
}

// 高频窗口操作只在失败时落盘，识别、翻译、设置等业务调用保留完整调用链。
pub fn should_record_call(channel: &str, payload: Option<&Value>) -> bool {
    if channel == "settings:save"
        && payload.is_some_and(|value| value["system"].get("logEnabled").is_some())
    {
        return false;
    }
    if channel.starts_with("translation:desktop-") || channel.starts_with("network-quality:") {
        return false;
    }
    if matches!(
        channel,
        "quick-switch:bootstrap"
            | "tool-window:bootstrap"
            | "quick-switch:move-by"
            | "quick-switch:set-collapsed"
            | "quick-switch:ready"
            | "tool-window:open"
            | "tool-window:ready"
            | "quick-switch:show-main"
            | "screenshot:ready"
            | "screenshot:capture-window-ready"
            | "screenshot:smart-selection"
            | "screenshot:long-preview"
            | "screenshot:long-scroll"
            | "screenshot:pin-ready"
            | "screenshot:pause-hotkeys"
    ) {
        return false;
    }
    if channel == "screenshot:pin-action" {
        return !matches!(
            payload.and_then(|value| value["action"].as_str()),
            Some(
                "drag" | "move" | "resize" | "opacity" | "context-menu-open" | "context-menu-close"
            )
        );
    }
    true
}

pub async fn record_start(
    user_data_path: &Path,
    channel: &str,
    started: &AppLogStart,
) -> Result<(), ManagerError> {
    append_log(
        user_data_path,
        json!({
          "id": create_log_id(),
          "traceId": started.trace_id,
          "scope": "backend",
          "service": channel.split(':').next().unwrap_or("app"),
          "method": channel,
          "channel": channel,
          "action": "start",
          "status": "pending",
          "level": "info",
          "durationMs": 0,
          "message": format!("开始调用 {channel}"),
          "payload": started.payload,
          "context": started.context,
          "result": null,
          "createdAt": now_millis()
        }),
    )
    .await
}

pub async fn record_finish(
    user_data_path: &Path,
    channel: &str,
    started: &AppLogStart,
    result: &Result<Value, ManagerError>,
) -> Result<(), ManagerError> {
    let error = result.as_ref().err().map(error_details);
    append_log(
        user_data_path,
        json!({
          "id": create_log_id(),
          "traceId": started.trace_id,
          "scope": "backend",
          "service": channel.split(':').next().unwrap_or("app"),
          "method": channel,
          "channel": channel,
          "action": "finish",
          "status": if result.is_ok() { "success" } else { "error" },
          "level": if result.is_ok() { "info" } else { "error" },
          "durationMs": started.started_at.elapsed().as_millis(),
          "message": result.as_ref().map(|_| json!(format!("调用完成 {channel}")))
            .unwrap_or_else(|error| sanitize_log_value(&json!(error.to_string()))),
          "payload": started.payload,
          "context": started.context,
          "result": result.as_ref().ok().map(sanitize_log_value),
          "error": error,
          "createdAt": now_millis()
        }),
    )
    .await
}

pub async fn record_renderer(
    user_data_path: &Path,
    window: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let started = begin_call(None, window, Some(payload["context"].clone()));
    append_log(
        user_data_path,
        json!({
          "id": create_log_id(),
          "traceId": started.trace_id,
          "scope": "renderer",
          "service": "renderer",
          "method": payload["kind"].as_str().unwrap_or("runtime-error"),
          "channel": payload["channel"],
          "action": "error",
          "status": "error",
          "level": "error",
          "message": sanitize_log_value(&payload["message"]),
          "payload": sanitize_log_value(&payload["payload"]),
          "error": sanitize_log_value(&payload["error"]),
          "context": started.context,
          "durationMs": payload["durationMs"].as_u64().unwrap_or(0),
          "createdAt": now_millis()
        }),
    )
    .await?;
    Ok(Value::Null)
}

pub async fn record_background_error(
    user_data_path: &Path,
    channel: &str,
    error: ManagerError,
) -> Result<(), ManagerError> {
    let started = begin_call(None, "backend", Some(json!({ "source": "background" })));
    record_finish(user_data_path, channel, &started, &Err(error)).await
}

fn error_details(error: &ManagerError) -> Value {
    let mut details = json!({
      "code": match error {
          ManagerError::UnknownChannel(_) => "unknown-channel",
          ManagerError::Path(_) => "path",
          ManagerError::Io(_) => "io",
          ManagerError::System(_) => "system",
          ManagerError::OpenAiRequest { .. } => "http",
          ManagerError::Json(_) => "json",
          ManagerError::Sqlite(_) => "sqlite",
      },
      "message": error.to_string()
    });
    if let ManagerError::Io(error) = error {
        details["ioKind"] = json!(format!("{:?}", error.kind()));
        details["osCode"] = json!(error.raw_os_error());
    }
    if let ManagerError::OpenAiRequest { status_code, .. } = error {
        details["httpStatus"] = json!(status_code);
    }
    let mut causes = Vec::new();
    let mut source = std::error::Error::source(error);
    while let Some(error) = source {
        causes.push(error.to_string());
        if causes.len() >= 8 {
            break;
        }
        source = error.source();
    }
    details["causes"] = json!(causes);
    sanitize_log_value(&details)
}

async fn append_log(user_data_path: &Path, entry: Value) -> Result<(), ManagerError> {
    if !LOG_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }
    let _guard = log_file_lock().lock().await;
    if !LOG_ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }
    let today = Local::now().date_naive();
    maintain_logs_locked(user_data_path, today).await?;
    append_log_line(&daily_log_path(user_data_path, today), &entry).await
}

async fn append_log_line(file_path: &Path, entry: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .read(true)
        .open(&file_path)
        .await?;
    if file.metadata().await?.len() > 0 {
        file.seek(std::io::SeekFrom::End(-1)).await?;
        let mut last = [0];
        file.read_exact(&mut last).await?;
        if last[0] != b'\n' {
            // 只修复意外退出留下的末行，避免下一条日志拼在半个 JSON 后无法读取。
            let mut reader = BufReader::new(tokio::fs::File::open(file_path).await?);
            let mut line = Vec::new();
            let mut complete_bytes = 0;
            loop {
                line.clear();
                let size = reader.read_until(b'\n', &mut line).await?;
                if size == 0 {
                    break;
                }
                if !line.ends_with(b"\n") {
                    if serde_json::from_slice::<Value>(&line).is_ok() {
                        file.write_all(b"\n").await?;
                    } else {
                        file.set_len(complete_bytes).await?;
                    }
                    break;
                }
                complete_bytes += size as u64;
            }
        }
    }
    file.seek(std::io::SeekFrom::End(0)).await?;
    file.write_all(format!("{}\n", serde_json::to_string(&entry)?).as_bytes())
        .await?;
    file.flush().await?;
    Ok(())
}

fn daily_log_path(user_data_path: &Path, date: NaiveDate) -> PathBuf {
    log_path(user_data_path).join(format!("app-{date}.jsonl"))
}

async fn maintain_logs_locked(
    user_data_path: &Path,
    today: NaiveDate,
) -> Result<Vec<(NaiveDate, PathBuf, u64)>, ManagerError> {
    let earliest = today - chrono::Duration::days(LOG_RETENTION_DAYS - 1);
    // 两种旧目录都迁移，按记录本身的本地日期归档；迁移重试按 id 去重。
    for source_path in [
        log_path(user_data_path).join("app-call-logs.json"),
        user_data_path.join("workspace/storage/app-call-logs.json"),
    ] {
        if !source_path.exists() {
            continue;
        }
        let items = read_legacy_logs(&source_path).await?;
        let mut groups = std::collections::BTreeMap::<NaiveDate, Vec<Value>>::new();
        for item in items {
            let date = item["createdAt"]
                .as_i64()
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|time| time.with_timezone(&Local).date_naive())
                .unwrap_or(today);
            if date >= earliest {
                groups.entry(date).or_default().push(item);
            }
        }
        for (date, items) in groups {
            let target = daily_log_path(user_data_path, date);
            let existing = read_daily_logs(&target, usize::MAX).await?;
            let mut ids: HashSet<String> = existing
                .iter()
                .filter_map(|item| item["id"].as_str().map(str::to_owned))
                .collect();
            for item in items.into_iter().rev() {
                if item["id"]
                    .as_str()
                    .is_some_and(|id| !ids.insert(id.to_owned()))
                {
                    continue;
                }
                append_log_line(&target, &sanitize_log_value(&item)).await?;
            }
        }
        tokio::fs::remove_file(source_path).await?;
    }

    let mut files = Vec::new();
    let mut entries = match tokio::fs::read_dir(log_path(user_data_path)).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(date) = name
            .strip_prefix("app-")
            .and_then(|name| name.strip_suffix(".jsonl"))
            .filter(|date| date.len() == 10)
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        else {
            continue;
        };
        // 仅处理本模块的日期文件，不递归、不触碰目录中的其他文件。
        if date < earliest {
            tokio::fs::remove_file(entry.path()).await?;
        } else {
            files.push((date, entry.path(), entry.metadata().await?.len()));
        }
    }
    files.sort_by(|left, right| right.0.cmp(&left.0));
    Ok(files)
}

async fn read_daily_logs(file_path: &Path, limit: usize) -> Result<Vec<Value>, ManagerError> {
    let file = match tokio::fs::File::open(file_path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut logs = VecDeque::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(item) => logs.push_back(item),
            // 意外退出可能留下最后一条未写完的日志，不阻止读取此前的完整记录。
            Err(_) if !line.ends_with('\n') => break,
            Err(error) => return Err(error.into()),
        }
        if logs.len() > limit {
            logs.pop_front();
        }
    }
    Ok(logs.into_iter().rev().collect())
}

fn sanitize_log_value(value: &Value) -> Value {
    snapshot_log_value(value, 0, &mut 12_000)
}

// 保留可排查的字段值和数组样本，同时限制深度、总量和大文本，避免快照挤掉错误日志。
fn snapshot_log_value(value: &Value, depth: usize, budget: &mut usize) -> Value {
    if *budget == 0 {
        return json!("[日志大小上限，已省略]");
    }
    *budget = budget.saturating_sub(8);
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(text) => {
            // 图片正文只存入工作台，不把完整参考图复制到操作日志。
            if text.starts_with("data:") {
                return json!({ "omitted": "二进制内容", "bytes": text.len() });
            }
            let mut preview = redact_log_text(text);
            if preview.chars().count() > (*budget).min(4000) {
                preview = preview.chars().take((*budget).min(4000)).collect();
                preview.push_str(&format!("…[已截断，原始 {} 字节]", text.len()));
            }
            *budget = budget.saturating_sub(preview.len());
            json!(preview)
        }
        Value::Array(items) if depth >= 5 => json!({
          "type": "array", "length": items.len(), "omitted": "达到日志深度上限"
        }),
        Value::Array(items) => {
            let mut sample = Vec::new();
            for item in items.iter().take(5) {
                if *budget == 0 {
                    break;
                }
                sample.push(snapshot_log_value(item, depth + 1, budget));
            }
            if sample.len() == items.len() {
                json!(sample)
            } else {
                json!({ "type": "array", "length": items.len(), "omitted": items.len() - sample.len(), "items": sample })
            }
        }
        Value::Object(map) if depth >= 5 => json!({
          "type": "object", "keys": map.keys().take(30).collect::<Vec<_>>(), "omitted": "达到日志深度上限"
        }),
        Value::Object(map) => {
            let mut snapshot = serde_json::Map::new();
            for (key, value) in map.iter().take(30) {
                if *budget == 0 {
                    break;
                }
                *budget = budget.saturating_sub(key.len());
                snapshot.insert(
                    key.clone(),
                    if is_sensitive_key(key) && !value.is_null() {
                        json!("***")
                    } else {
                        snapshot_log_value(value, depth + 1, budget)
                    },
                );
            }
            if snapshot.len() < map.len() {
                snapshot.insert(
                    "$logOmittedFields".into(),
                    json!(map.len() - snapshot.len()),
                );
            }
            Value::Object(snapshot)
        }
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_lowercase().replace(['_', '-'], "");

    key.contains("password")
        || key.ends_with("token")
        || key.ends_with("secret")
        || key.ends_with("apikey")
        || key.ends_with("privatekey")
        || key.ends_with("accesskey")
        || key.ends_with("secretkey")
        || matches!(
            key.as_str(),
            "key"
                | "authorization"
                | "proxyauthorization"
                | "cookie"
                | "setcookie"
                | "credential"
                | "credentials"
        )
}

fn redact_log_text(text: &str) -> String {
    static PATTERNS: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| [
        r#"(?i)(https?://)[^/\s@]+@"#,
        r#"(?i)\b(?:bearer|basic)\s+[a-z0-9._~+/\-=]+"#,
        r#"\bsk-[A-Za-z0-9_-]{8,}"#,
        r#"(?i)["'](?:api[_-]?key|access[_-]?key|secret[_-]?key|private[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|password|client[_-]?secret|token|secret|key|authorization|cookie)["']\s*:\s*["'][^"']*["']"#,
        r#"(?im)\b(?:authorization|proxy-authorization|cookie|set-cookie)\s*:\s*[^\r\n]+"#,
        r#"(?i)\b(?:[a-z0-9]+[_-])*(?:api[_-]?key|access[_-]?key|secret[_-]?key|private[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|password|client[_-]?secret|token|secret|key)\s*=\s*(?:"[^"]*"|'[^']*'|[^&\s"';#]+)"#,
    ].into_iter().map(|pattern| regex::Regex::new(pattern).unwrap()).collect());
    let mut text = text.to_owned();
    for (index, pattern) in patterns.iter().enumerate() {
        text = pattern
            .replace_all(
                &text,
                if index == 0 {
                    "$1***@"
                } else {
                    "[凭据已隐藏]"
                },
            )
            .into_owned();
    }
    text
}

async fn read_legacy_logs(file_path: &Path) -> Result<Vec<Value>, ManagerError> {
    match tokio::fs::read_to_string(file_path).await {
        Ok(content) => {
            if !content.trim_start().starts_with('[') {
                return read_daily_logs(file_path, usize::MAX).await;
            }
            let (logs, _) = parse_log_content(&content)?;
            logs.as_array()
                .cloned()
                .ok_or_else(|| ManagerError::System("旧日志格式无效".into()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
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
    uuid::Uuid::new_v4().to_string()
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn log_file_lock() -> &'static Mutex<()> {
    LOG_FILE_LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("app-logs-{}", create_log_id()));
            std::fs::create_dir_all(log_path(&root)).unwrap();
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            set_enabled(true);
            let root = self.0.canonicalize().unwrap();
            assert_eq!(
                root.parent().unwrap(),
                std::env::temp_dir().canonicalize().unwrap()
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    fn run_test(test: impl std::future::Future<Output = ()>) {
        let _guard = TEST_LOCK.lock().unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(test);
    }

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

    #[test]
    fn snapshots_retain_diagnostic_values_and_redact_credentials() {
        let value = sanitize_log_value(&json!({
            "providerId": "provider-a", "model": "vision-model",
            "nested": { "status": 401, "apiKey": "secret-key", "access_token": "secret-token" },
            "tokenCount": 1200, "keyboardShortcut": "Ctrl+T",
            "items": [1, 2, 3, 4, 5, 6],
            "image": "data:image/png;base64,secret-image",
            "error": "HTTP 401 https://user:pass@example.test?api_key=private-value Bearer hidden-value",
            "config": "{\"api_key\":\"with spaces private\", \"model\":\"visible-model\"}",
            "toml": "OPENAI_API_KEY = \"toml-private\"\nmodel = \"visible-model\"",
            "url": "https://example.test?key=query-private&model=visible-model"
        }));
        assert_eq!(value["providerId"], "provider-a");
        assert_eq!(value["nested"]["status"], 401);
        assert_eq!(value["nested"]["apiKey"], "***");
        assert_eq!(value["nested"]["access_token"], "***");
        assert_eq!(value["tokenCount"], 1200);
        assert_eq!(value["keyboardShortcut"], "Ctrl+T");
        assert_eq!(value["items"]["items"][4], 5);
        assert_eq!(value["items"]["omitted"], 1);
        let text = value.to_string();
        for secret in [
            "secret-key",
            "secret-token",
            "secret-image",
            "user:pass",
            "private-value",
            "hidden-value",
            "with spaces private",
            "toml-private",
            "query-private",
        ] {
            assert!(!text.contains(secret), "{secret}");
        }
        assert!(text.contains("visible-model"));
        let large = sanitize_log_value(&json!({ "result": "文".repeat(100_000) }));
        assert!(large["result"].as_str().unwrap().contains("已截断"));
        assert!(large.to_string().len() < 16_000);
    }

    #[test]
    fn keeps_seven_calendar_days_and_removes_only_owned_expired_files() {
        run_test(async {
            let fixture = Fixture::new();
            let today = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
            for age in 0..10 {
                let date = today - chrono::Duration::days(age);
                std::fs::write(daily_log_path(&fixture.0, date), "{\"id\":1}\n").unwrap();
            }
            let unrelated = log_path(&fixture.0).join("other-log.jsonl");
            std::fs::write(&unrelated, "保留").unwrap();
            let files = maintain_logs_locked(&fixture.0, today).await.unwrap();
            assert_eq!(files.len(), 7);
            assert!(daily_log_path(&fixture.0, today - chrono::Duration::days(6)).exists());
            assert!(!daily_log_path(&fixture.0, today - chrono::Duration::days(7)).exists());
            assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "保留");
            assert_eq!(files[0].0, today);
        });
    }

    #[test]
    fn migrates_both_legacy_locations_by_local_date_without_duplicates() {
        run_test(async {
            let fixture = Fixture::new();
            let today = Local::now().date_naive();
            let yesterday = today - chrono::Duration::days(1);
            let timestamp = |date: NaiveDate| {
                date.and_hms_opt(12, 0, 0)
                    .unwrap()
                    .and_local_timezone(Local)
                    .earliest()
                    .unwrap()
                    .timestamp_millis()
            };
            let source = log_path(&fixture.0).join("app-call-logs.json");
            let old_source = fixture.0.join("workspace/storage/app-call-logs.json");
            std::fs::create_dir_all(old_source.parent().unwrap()).unwrap();
            let item = json!({ "id": "today", "createdAt": timestamp(today), "message": "保留" });
            std::fs::write(
                &source,
                json!([
                    item,
                    { "id": "yesterday", "createdAt": timestamp(yesterday) },
                    { "id": "expired", "createdAt": timestamp(today - chrono::Duration::days(7)) }
                ])
                .to_string(),
            )
            .unwrap();
            std::fs::write(&old_source, format!("{item}\n")).unwrap();
            append_log_line(&daily_log_path(&fixture.0, today), &item)
                .await
                .unwrap();
            let result = list_logs(&fixture.0, None).await.unwrap();
            assert_eq!(result["logs"].as_array().unwrap().len(), 2);
            assert_eq!(result["dates"].as_array().unwrap().len(), 2);
            assert!(!source.exists());
            assert!(!old_source.exists());
            assert_eq!(
                list_logs(&fixture.0, Some(&yesterday.to_string()))
                    .await
                    .unwrap()["logs"][0]["id"],
                "yesterday"
            );
            assert!(list_logs(&fixture.0, Some("../invalid")).await.is_err());
        });
    }

    #[test]
    fn disabled_logging_keeps_storage_unchanged_and_cleanup_still_runs() {
        run_test(async {
            let fixture = Fixture::new();
            let today = Local::now().date_naive();
            let expired = daily_log_path(&fixture.0, today - chrono::Duration::days(7));
            std::fs::write(&expired, "{\"id\":\"expired\"}\n").unwrap();
            configure(false).await;
            let started = begin_call(
                Some(&json!({"model": "vision"})),
                "main",
                Some(json!({"traceId": "test-trace"})),
            );
            record_start(&fixture.0, "screenshot:ocr", &started)
                .await
                .unwrap();
            record_finish(
                &fixture.0,
                "screenshot:ocr",
                &started,
                &Err(ManagerError::System("测试错误".into())),
            )
            .await
            .unwrap();
            record_renderer(&fixture.0, "main", json!({"message": "前端错误"}))
                .await
                .unwrap();
            let result = stats(&fixture.0).await.unwrap();
            assert_eq!(result["bytes"], 0);
            assert_eq!(result["enabled"], false);
            assert!(!expired.exists());
            assert!(!daily_log_path(&fixture.0, today).exists());

            configure(true).await;
            let result = Err(ManagerError::OpenAiRequest {
                status_code: 401,
                message: "鉴权失败".into(),
            });
            record_finish(&fixture.0, "screenshot:ocr", &started, &result)
                .await
                .unwrap();
            let logs = list_logs(&fixture.0, None).await.unwrap();
            assert_eq!(logs["logs"][0]["traceId"], "test-trace");
            assert_eq!(logs["logs"][0]["payload"]["model"], "vision");
            assert_eq!(logs["logs"][0]["error"]["httpStatus"], 401);
            assert_eq!(logs["logs"][0]["context"]["window"], "main");
            assert_eq!(
                stats(&fixture.0).await.unwrap()["bytes"],
                std::fs::metadata(daily_log_path(&fixture.0, today))
                    .unwrap()
                    .len()
            );
            clear_logs(&fixture.0).await.unwrap();
            assert_eq!(stats(&fixture.0).await.unwrap()["bytes"], 0);
        });
    }

    #[test]
    fn display_limit_never_discards_retained_disk_history() {
        run_test(async {
            let fixture = Fixture::new();
            let today = Local::now().date_naive();
            let path = daily_log_path(&fixture.0, today);
            let content: String = (0..1005).map(|id| format!("{{\"id\":{id}}}\n")).collect();
            std::fs::write(&path, &content).unwrap();
            let result = list_logs(&fixture.0, None).await.unwrap();
            assert_eq!(result["logs"].as_array().unwrap().len(), 1000);
            assert_eq!(result["logs"][0]["id"], 1004);
            assert_eq!(result["hasMore"], true);
            assert_eq!(std::fs::read_to_string(path).unwrap(), content);
        });
    }

    #[test]
    fn business_calls_are_logged_while_window_motion_stays_quiet() {
        assert!(should_record_call("screenshot:ocr", None));
        assert!(should_record_call("translation:translate", None));
        assert!(should_record_call("screenshot:settings", None));
        assert!(!should_record_call("translation:desktop-resize", None));
        assert!(!should_record_call(
            "screenshot:pin-action",
            Some(&json!({"action": "move"}))
        ));
        assert!(should_record_call(
            "screenshot:pin-action",
            Some(&json!({"action": "close"}))
        ));
        assert!(!should_record_call(
            "settings:save",
            Some(&json!({"system": {"logEnabled": false}}))
        ));
    }

    #[test]
    fn io_errors_preserve_kind_and_underlying_cause() {
        let error = ManagerError::Io(std::io::Error::from_raw_os_error(2));
        let details = error_details(&error);
        assert_eq!(details["code"], "io");
        assert_eq!(details["osCode"], 2);
        assert!(!details["causes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn partial_last_line_does_not_hide_completed_records() {
        run_test(async {
            let fixture = Fixture::new();
            let path = daily_log_path(&fixture.0, Local::now().date_naive());
            std::fs::write(&path, "{\"id\":1}\n{\"id\":").unwrap();
            assert_eq!(read_daily_logs(&path, 1000).await.unwrap()[0]["id"], 1);
            append_log_line(&path, &json!({"id": 2})).await.unwrap();
            let items = read_daily_logs(&path, 1000).await.unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0]["id"], 2);
            std::fs::write(&path, "{\"id\":3}").unwrap();
            append_log_line(&path, &json!({"id": 4})).await.unwrap();
            let items = read_daily_logs(&path, 1000).await.unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[1]["id"], 3);
            assert_eq!(items[0]["id"], 4);
        });
    }
}

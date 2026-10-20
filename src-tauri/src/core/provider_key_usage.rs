use crate::core::database;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static IN_FLIGHT: LazyLock<Mutex<HashMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const MAX_ERROR_BUFFER: usize = 64 * 1024;

#[derive(Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyBinding {
    api_key_id: String,
    api_key_hash: String,
    single_key: bool,
}

pub struct KeyRequest {
    paths: AppPaths,
    provider_id: String,
    key_id: String,
    key_hash: String,
    storage_provider_id: Option<String>,
    started: bool,
    finished: bool,
    status: Option<u16>,
    streaming: bool,
    error_buffer: Vec<u8>,
    skip_frame: bool,
    api_error: bool,
    expects_completion: bool,
    completed_stream: bool,
}

fn open(paths: &AppPaths) -> Result<Connection, ManagerError> {
    let mut connection = database::open(paths)?;
    let ready: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('provider_keys') WHERE name='is_current')
            AND NOT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='provider_key_usage')
            AND EXISTS(SELECT 1 FROM pragma_table_info('provider_key_bindings') WHERE name='api_key_id')
            AND NOT EXISTS(SELECT 1 FROM pragma_table_info('provider_key_bindings') WHERE name='payload_json')",
        [],|row|row.get(0))?;
    if !ready {
        crate::core::provider_store::migrate_connection(&mut connection)?;
    }
    Ok(connection)
}

impl KeyRequest {
    pub fn new(paths: &AppPaths, provider_id: &str, key_id: &str, api_key: &str) -> Self {
        Self {
            paths: paths.clone(),
            provider_id: provider_id.to_string(),
            key_id: key_id.to_string(),
            key_hash: format!("{:x}", Sha256::digest(api_key.as_bytes())),
            storage_provider_id: None,
            started: false,
            finished: false,
            status: None,
            streaming: false,
            error_buffer: Vec::new(),
            skip_frame: false,
            api_error: false,
            expects_completion: false,
            completed_stream: false,
        }
    }

    fn identity(&self) -> String {
        json!([
            self.paths.storage_files.database,
            self.provider_id,
            self.key_id,
            self.key_hash
        ])
        .to_string()
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn key_hash(&self) -> &str {
        &self.key_hash
    }

    pub fn start(&mut self) {
        if self.started || self.finished {
            return;
        }
        let result = open(&self.paths).and_then(|connection| {
            let provider_id=crate::core::provider_store::usage_provider_id(&connection, &self.provider_id)?;
            connection.execute(
                "INSERT INTO provider_keys(provider_id, id, key_hash, is_current, request_count, last_used_at)
                 VALUES (?1, ?2, ?3, 0, 1, ?4)
                 ON CONFLICT(provider_id, id, key_hash) DO UPDATE SET
                   request_count = request_count + 1, last_used_at = MAX(last_used_at, excluded.last_used_at)",
                params![provider_id, self.key_id, self.key_hash, chrono::Utc::now().timestamp_millis()],
            )?;
            Ok(provider_id)
        });
        match result {
            Ok(provider_id) => self.storage_provider_id = Some(provider_id),
            Err(_) => {
                eprintln!("API Key 请求统计写入失败，请检查数据目录");
                return;
            }
        }
        self.started = true;
        if let Ok(mut active) = IN_FLIGHT.lock() {
            *active.entry(self.identity()).or_default() += 1;
        }
    }

    pub fn response(&mut self, status: u16, streaming: bool) {
        self.status = Some(status);
        self.streaming = streaming;
    }

    fn inspect_error(&mut self) {
        if self.skip_frame {
            return;
        }
        let frame_data;
        let bytes = if self.streaming {
            let Ok(frame) = std::str::from_utf8(&self.error_buffer) else {
                return;
            };
            self.api_error |= frame.lines().any(|line| {
                line.strip_prefix("event:")
                    .is_some_and(|event| event.trim() == "error")
            });
            frame_data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>()
                .join("\n");
            frame_data.as_bytes()
        } else {
            self.error_buffer.as_slice()
        };
        if self.streaming && std::str::from_utf8(bytes).is_ok_and(|text| text.trim() == "[DONE]") {
            self.completed_stream = true;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
            self.api_error |= value.get("error").is_some_and(|error| !error.is_null())
                || matches!(value["type"].as_str(), Some("error" | "response.failed"))
                || value["status"] == "failed";
            if self.streaming {
                self.expects_completion |= matches!(
                    value["type"].as_str(),
                    Some("message_start" | "response.created" | "response.in_progress")
                ) || value["object"] == "chat.completion.chunk";
                self.completed_stream |= matches!(
                    value["type"].as_str(),
                    Some("message_stop" | "response.completed")
                );
            }
        }
    }

    pub fn observe(&mut self, bytes: &[u8]) {
        if self.finished || self.api_error || !self.started {
            return;
        }
        if !self.streaming {
            if self.error_buffer.len() + bytes.len() <= MAX_ERROR_BUFFER && !self.skip_frame {
                self.error_buffer.extend_from_slice(bytes);
            } else {
                self.error_buffer.clear();
                self.skip_frame = true;
            }
            return;
        }
        for part in bytes.split_inclusive(|byte| *byte == b'\n') {
            if self.error_buffer.len() + part.len() > MAX_ERROR_BUFFER {
                self.error_buffer.clear();
                self.skip_frame = true;
            }
            if self.skip_frame {
                self.error_buffer
                    .extend_from_slice(&part[part.len().saturating_sub(4)..]);
                if self.error_buffer.len() > 4 {
                    self.error_buffer.drain(..self.error_buffer.len() - 4);
                }
            } else {
                self.error_buffer.extend_from_slice(part);
            }
            if self.error_buffer.ends_with(b"\n\n") || self.error_buffer.ends_with(b"\r\n\r\n") {
                self.inspect_error();
                self.error_buffer.clear();
                self.skip_frame = false;
            }
        }
    }

    pub fn finish(&mut self) {
        self.complete(None);
    }

    pub fn fail(&mut self, kind: &str) {
        self.complete(Some(kind));
    }

    fn complete(&mut self, failure: Option<&str>) {
        if !self.started || self.finished {
            return;
        }
        self.inspect_error();
        let kind = match self.status {
            Some(401 | 403) => "auth",
            Some(429) => "rate_limit",
            Some(500..=599) => "upstream",
            Some(status) if !(200..300).contains(&status) => "http",
            _ if self.api_error => "api",
            _ => failure.unwrap_or(
                if self.streaming && self.expects_completion && !self.completed_stream {
                    "stream"
                } else {
                    ""
                },
            ),
        };
        let failed = !kind.is_empty();
        let now = chrono::Utc::now().timestamp_millis();
        let result = open(&self.paths).and_then(|connection| {
            connection.execute(
                "UPDATE provider_keys SET
                   success_count = success_count + ?4,
                   failure_count = failure_count + ?5,
                   consecutive_failures = CASE WHEN ?5 = 1 THEN consecutive_failures + 1 ELSE 0 END,
                   last_success_at = CASE WHEN ?4 = 1 THEN ?6 ELSE last_success_at END,
                   last_failure_at = CASE WHEN ?5 = 1 THEN ?6 ELSE last_failure_at END,
                   last_status_code = ?7, last_error_kind = ?8,
                   last_failure_kind = CASE WHEN ?5 = 1 THEN ?8 ELSE last_failure_kind END,
                   last_outcome = ?9
                 WHERE provider_id = ?1 AND id = ?2 AND key_hash = ?3",
                params![
                    self.storage_provider_id
                        .as_deref()
                        .unwrap_or(&self.provider_id),
                    self.key_id,
                    self.key_hash,
                    i64::from(!failed),
                    i64::from(failed),
                    now,
                    self.status,
                    kind,
                    if failed { "failure" } else { "success" }
                ],
            )?;
            Ok(())
        });
        if result.is_err() {
            eprintln!("API Key 请求结果统计写入失败，请检查数据目录");
        }
        self.finished = true;
        self.error_buffer.clear();
        if let Ok(mut active) = IN_FLIGHT.lock() {
            let identity = self.identity();
            if let Some(count) = active.get_mut(&identity) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    active.remove(&identity);
                }
            }
        }
    }
}

impl Drop for KeyRequest {
    fn drop(&mut self) {
        self.fail("cancelled");
    }
}

pub fn read(
    paths: &AppPaths,
    provider_id: &str,
    key_id: &str,
    api_key: &str,
) -> Result<Value, ManagerError> {
    let identity = KeyRequest::new(paths, provider_id, key_id, api_key);
    let connection = open(paths)?;
    let mut usage = connection.query_row(
        "SELECT request_count, success_count, failure_count, consecutive_failures,
           last_used_at, last_success_at, last_failure_at, last_status_code,
           last_error_kind, last_failure_kind, last_outcome
         FROM provider_keys WHERE provider_id = ?1 AND id = ?2 AND key_hash = ?3",
        params![crate::core::provider_store::usage_provider_id(&connection, provider_id)?, key_id, identity.key_hash],
        |row| Ok(json!({
            "requestCount": row.get::<_, i64>(0)?, "successCount": row.get::<_, i64>(1)?,
            "failureCount": row.get::<_, i64>(2)?, "consecutiveFailures": row.get::<_, i64>(3)?,
            "lastUsedAt": row.get::<_, i64>(4)?, "lastSuccessAt": row.get::<_, i64>(5)?,
            "lastFailureAt": row.get::<_, i64>(6)?, "lastStatusCode": row.get::<_, Option<u16>>(7)?,
            "lastErrorKind": row.get::<_, String>(8)?, "lastFailureKind": row.get::<_, String>(9)?,
            "lastOutcome": row.get::<_, String>(10)?
        })),
    ).optional()?.unwrap_or_else(|| json!({
        "requestCount": 0, "successCount": 0, "failureCount": 0, "consecutiveFailures": 0,
        "lastUsedAt": 0, "lastSuccessAt": 0, "lastFailureAt": 0, "lastStatusCode": null,
        "lastErrorKind": "", "lastFailureKind": "", "lastOutcome": ""
    }));
    usage["inFlightCount"] = json!(IN_FLIGHT
        .lock()
        .ok()
        .and_then(|active| active.get(&identity.identity()).copied())
        .unwrap_or(0));
    Ok(usage)
}

pub fn fingerprint(api_key: &str) -> String {
    format!("{:x}", Sha256::digest(api_key.as_bytes()))
}

pub(crate) fn migrate_bindings(connection: &mut Connection) -> Result<(), ManagerError> {
    let ready: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('provider_key_bindings') WHERE name='api_key_id')
            AND NOT EXISTS(SELECT 1 FROM pragma_table_info('provider_key_bindings') WHERE name='payload_json')",
        [],
        |row| row.get(0),
    )?;
    if ready {
        return Ok(());
    }
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    // 获取写锁后重新检查，兼容多个连接同时触发迁移。
    let legacy: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('provider_key_bindings') WHERE name='payload_json')",
        [],
        |row| row.get(0),
    )?;
    if legacy {
        transaction.execute_batch(
            "ALTER TABLE provider_key_bindings RENAME TO provider_key_bindings_legacy;",
        )?;
    }
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS provider_key_bindings (
           provider_id TEXT NOT NULL,
           observed_at INTEGER NOT NULL,
           api_key_id TEXT NOT NULL DEFAULT '',
           api_key_hash TEXT NOT NULL DEFAULT '',
           single_key INTEGER NOT NULL DEFAULT 0 CHECK(single_key IN (0,1)),
           PRIMARY KEY(provider_id, observed_at)
         );",
    )?;
    if legacy {
        {
            let mut statement = transaction.prepare(
                "SELECT provider_id, observed_at, payload_json FROM provider_key_bindings_legacy",
            )?;
            let mut rows = statement.query([])?;
            let mut insert = transaction.prepare(
                "INSERT INTO provider_key_bindings(provider_id, observed_at, api_key_id, api_key_hash, single_key)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            while let Some(row) = rows.next()? {
                let binding: KeyBinding = serde_json::from_str(&row.get::<_, String>(2)?)?;
                insert.execute(params![
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    binding.api_key_id,
                    binding.api_key_hash,
                    binding.single_key
                ])?;
            }
        }
        // 转换和删除旧表在同一事务内完成，格式错误时保留全部原记录。
        transaction.execute_batch("DROP TABLE provider_key_bindings_legacy;")?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn sync_bindings(
    paths: &AppPaths,
    bindings: &Map<String, Value>,
    observed_at: i64,
) -> Result<(), ManagerError> {
    let mut connection = open(paths)?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let current = {
        let mut statement = transaction.prepare(
            "SELECT provider_id, api_key_id, api_key_hash, single_key FROM provider_key_bindings AS binding
             WHERE observed_at = (SELECT MAX(observed_at) FROM provider_key_bindings WHERE provider_id = binding.provider_id)",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    KeyBinding {
                        api_key_id: row.get(1)?,
                        api_key_hash: row.get(2)?,
                        single_key: row.get(3)?,
                    },
                ))
            })?
            .collect::<Result<HashMap<_, _>, _>>()?;
        rows
    };
    for provider_id in bindings.keys().chain(
        current
            .keys()
            .filter(|provider_id| !bindings.contains_key(*provider_id)),
    ) {
        let binding = bindings
            .get(provider_id)
            .cloned()
            .map(serde_json::from_value::<KeyBinding>)
            .transpose()?
            .unwrap_or_default();
        if current.get(provider_id) == Some(&binding) {
            continue;
        }
        transaction.execute(
            "INSERT INTO provider_key_bindings(provider_id, observed_at, api_key_id, api_key_hash, single_key)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(provider_id, observed_at) DO UPDATE SET
               api_key_id = excluded.api_key_id, api_key_hash = excluded.api_key_hash,
               single_key = excluded.single_key",
            params![provider_id, observed_at, binding.api_key_id, binding.api_key_hash, binding.single_key],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn bind_logs(paths: &AppPaths, logs: &mut [Value]) -> Result<(), ManagerError> {
    let connection = open(paths)?;
    let mut statement = connection.prepare(
        "SELECT provider_id, observed_at, api_key_id, api_key_hash, single_key
         FROM provider_key_bindings ORDER BY observed_at",
    )?;
    let mut bindings: HashMap<String, Vec<(i64, KeyBinding)>> = HashMap::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            KeyBinding {
                api_key_id: row.get(2)?,
                api_key_hash: row.get(3)?,
                single_key: row.get(4)?,
            },
        ))
    })? {
        let (provider_id, observed_at, binding) = row?;
        bindings
            .entry(provider_id)
            .or_default()
            .push((observed_at, binding));
    }
    for log in logs {
        if log["apiKeyAttribution"] == "mixed-keys" {
            continue;
        }
        if log
            .get("apiKeyHash")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
        {
            continue;
        }
        let provider_id = log["providerId"].as_str().unwrap_or_default();
        let storage_id = if log["appType"] == "claude-desktop" {
            format!("claude-desktop:{provider_id}")
        } else {
            provider_id.to_string()
        };
        let Some(history) = bindings.get(&storage_id) else {
            continue;
        };
        let created_at = log["createdAt"].as_i64().unwrap_or_default();
        let position = history.partition_point(|(observed_at, _)| *observed_at <= created_at);
        let historical = position.checked_sub(1).and_then(|index| history.get(index));
        let inferred = historical.is_none();
        let Some((_, binding)) =
            historical.or_else(|| history.first().filter(|(_, binding)| binding.single_key))
        else {
            continue;
        };
        let key_id = binding.api_key_id.as_str();
        if key_id.is_empty()
            || log
                .get("apiKeyId")
                .and_then(Value::as_str)
                .is_some_and(|existing| !existing.is_empty() && existing != key_id)
        {
            continue;
        }
        log["apiKeyId"] = json!(binding.api_key_id);
        log["apiKeyHash"] = json!(binding.api_key_hash);
        log["apiKeyAttribution"] = json!(if inferred {
            "single-key-inferred"
        } else {
            "active-key"
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::resolve_app_paths;

    fn with_paths(action: impl FnOnce(&AppPaths)) {
        let root =
            std::env::temp_dir().join(format!("monkey-thief-key-usage-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        action(&paths);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(resolved).unwrap();
    }

    fn create_legacy_bindings(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE provider_key_bindings (
               provider_id TEXT NOT NULL, observed_at INTEGER NOT NULL, payload_json TEXT NOT NULL,
               PRIMARY KEY(provider_id, observed_at)
             );",
            )
            .unwrap();
    }

    #[test]
    fn migrates_binding_fields_and_preserves_historical_attribution() {
        with_paths(|paths| {
            let connection = database::open(paths).unwrap();
            create_legacy_bindings(&connection);
            let snapshots = [
                (
                    "provider",
                    100,
                    json!({"apiKeyId":"first", "apiKeyHash":"first-hash", "singleKey":true}),
                ),
                (
                    "provider",
                    200,
                    json!({"apiKeyId":"second", "apiKeyHash":"second-hash", "singleKey":false}),
                ),
                (
                    "provider",
                    300,
                    json!({"apiKeyId":"", "apiKeyHash":"", "singleKey":false}),
                ),
                (
                    "claude-desktop:provider",
                    100,
                    json!({"apiKeyId":"desktop", "apiKeyHash":"desktop-hash", "singleKey":false}),
                ),
            ];
            for (provider, observed_at, payload) in &snapshots {
                connection
                    .execute(
                        "INSERT INTO provider_key_bindings VALUES (?1, ?2, ?3)",
                        params![provider, observed_at, payload.to_string()],
                    )
                    .unwrap();
            }
            crate::core::provider_store::initialize(paths).unwrap();
            crate::core::provider_store::initialize(paths).unwrap();
            for (provider, observed_at, expected) in &snapshots {
                let actual = connection
                    .query_row(
                        "SELECT api_key_id, api_key_hash, single_key FROM provider_key_bindings
                     WHERE provider_id=?1 AND observed_at=?2",
                        params![provider, observed_at],
                        |row| {
                            Ok(json!({"apiKeyId":row.get::<_,String>(0)?,
                        "apiKeyHash":row.get::<_,String>(1)?, "singleKey":row.get::<_,bool>(2)?}))
                        },
                    )
                    .unwrap();
                assert_eq!(&actual, expected);
            }
            let columns = connection
                .prepare("PRAGMA table_info(provider_key_bindings)")
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(
                columns,
                [
                    "provider_id",
                    "observed_at",
                    "api_key_id",
                    "api_key_hash",
                    "single_key"
                ]
            );
            assert_eq!(connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='provider_key_bindings_legacy'",
                [], |row| row.get::<_,i64>(0),
            ).unwrap(), 0);

            let mut logs = vec![
                json!({"providerId":"provider", "createdAt":50}),
                json!({"providerId":"provider", "createdAt":100}),
                json!({"providerId":"provider", "createdAt":200}),
                json!({"providerId":"provider", "createdAt":300}),
                json!({"providerId":"provider", "appType":"claude-desktop", "createdAt":100}),
            ];
            bind_logs(paths, &mut logs).unwrap();
            assert_eq!(logs[0]["apiKeyAttribution"], "single-key-inferred");
            assert_eq!(logs[1]["apiKeyHash"], "first-hash");
            assert_eq!(logs[2]["apiKeyHash"], "second-hash");
            assert!(logs[3].get("apiKeyId").is_none());
            assert_eq!(logs[4]["apiKeyHash"], "desktop-hash");

            sync_bindings(
                paths,
                &Map::from_iter([
                    ("provider".into(), snapshots[2].2.clone()),
                    ("claude-desktop:provider".into(), snapshots[3].2.clone()),
                ]),
                400,
            )
            .unwrap();
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM provider_key_bindings", [], |row| row
                        .get::<_, i64>(
                        0
                    ),)
                    .unwrap(),
                4
            );
        });
    }

    #[test]
    fn invalid_binding_payload_rolls_back_without_losing_history() {
        for invalid in [
            "{",
            "[]",
            r#"{"apiKeyId":"key","apiKeyHash":"hash","singleKey":"false"}"#,
        ] {
            let mut connection = Connection::open_in_memory().unwrap();
            create_legacy_bindings(&connection);
            let valid =
                json!({"apiKeyId":"key", "apiKeyHash":"hash", "singleKey":false}).to_string();
            connection.execute(
                "INSERT INTO provider_key_bindings VALUES ('provider',100,?1),('provider',200,?2)",
                params![valid, invalid],
            ).unwrap();
            assert!(migrate_bindings(&mut connection).is_err());
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM provider_key_bindings", [], |row| row
                        .get::<_, i64>(
                        0
                    ),)
                    .unwrap(),
                2
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT payload_json FROM provider_key_bindings WHERE observed_at=200",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap(),
                invalid
            );
            assert_eq!(connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='provider_key_bindings_legacy'", [], |row| row.get::<_,i64>(0),
            ).unwrap(), 0);
            connection
                .execute(
                    "UPDATE provider_key_bindings SET payload_json=?1 WHERE observed_at=200",
                    [valid],
                )
                .unwrap();
            migrate_bindings(&mut connection).unwrap();
            migrate_bindings(&mut connection).unwrap();
            assert_eq!(connection.query_row(
                "SELECT COUNT(*) FROM provider_key_bindings WHERE api_key_id='key' AND api_key_hash='hash' AND single_key=0",
                [], |row| row.get::<_,i64>(0),
            ).unwrap(), 2);
        }
    }

    #[test]
    fn concurrent_binding_migrations_and_syncs_do_not_duplicate_history() {
        with_paths(|paths| {
            let connection = database::open(paths).unwrap();
            create_legacy_bindings(&connection);
            connection
                .execute(
                    "INSERT INTO provider_key_bindings VALUES ('provider',100,?1)",
                    [
                        json!({"apiKeyId":"first", "apiKeyHash":"first-hash", "singleKey":true})
                            .to_string(),
                    ],
                )
                .unwrap();
            let bindings = Map::from_iter([(
                "provider".into(),
                json!({"apiKeyId":"second", "apiKeyHash":"second-hash", "singleKey":true}),
            )]);
            let barrier = std::sync::Barrier::new(4);
            std::thread::scope(|scope| {
                for _ in 0..4 {
                    let bindings = &bindings;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        sync_bindings(paths, bindings, 200).unwrap();
                    });
                }
            });
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM provider_key_bindings", [], |row| row
                        .get::<_, i64>(
                        0
                    ),)
                    .unwrap(),
                2
            );
            let mut logs = [
                json!({"providerId":"provider", "createdAt":199}),
                json!({"providerId":"provider", "createdAt":200}),
            ];
            bind_logs(paths, &mut logs).unwrap();
            assert_eq!(logs[0]["apiKeyId"], "first");
            assert_eq!(logs[1]["apiKeyId"], "second");
        });
    }

    #[test]
    fn binds_usage_at_request_time_without_moving_history_to_the_current_key() {
        with_paths(|paths| {
            let mut bindings = Map::from_iter([(
                "provider".into(),
                json!({
                    "apiKeyId": "first", "apiKeyHash": fingerprint("first-secret"), "singleKey": false
                }),
            )]);
            sync_bindings(paths, &bindings, 100).unwrap();
            sync_bindings(paths, &bindings, 120).unwrap();
            bindings["provider"] = json!({"apiKeyId": "second", "apiKeyHash": fingerprint("second-secret"), "singleKey": false});
            sync_bindings(paths, &bindings, 200).unwrap();
            bindings["provider"]["apiKeyHash"] = json!(fingerprint("replacement-secret"));
            sync_bindings(paths, &bindings, 300).unwrap();
            sync_bindings(paths, &Map::new(), 400).unwrap();
            let mut logs = vec![
                json!({"providerId": "provider", "createdAt": 50}),
                json!({"providerId": "provider", "createdAt": 100}),
                json!({"providerId": "provider", "createdAt": 199}),
                json!({"providerId": "provider", "createdAt": 200}),
                json!({"providerId": "provider", "createdAt": 300}),
                json!({"providerId": "provider", "createdAt": 400}),
                json!({"providerId": "provider", "createdAt": 150, "apiKeyId": "exact", "apiKeyHash": "exact-hash"}),
                json!({"providerId": "provider", "createdAt": 150, "apiKeyAttribution": "mixed-keys"}),
                json!({"providerId": "provider", "createdAt": 150, "apiKeyId": "other"}),
            ];
            bind_logs(paths, &mut logs).unwrap();
            assert!(logs[0].get("apiKeyId").is_none());
            assert_eq!(logs[1]["apiKeyId"], "first");
            assert_eq!(logs[2]["apiKeyId"], "first");
            assert_eq!(logs[3]["apiKeyHash"], fingerprint("second-secret"));
            assert_eq!(logs[4]["apiKeyHash"], fingerprint("replacement-secret"));
            assert!(logs[5].get("apiKeyId").is_none());
            assert_eq!(logs[6]["apiKeyHash"], "exact-hash");
            assert!(logs[7].get("apiKeyId").is_none());
            assert!(logs[8].get("apiKeyHash").is_none());
            let original = logs.clone();
            bind_logs(paths, &mut logs).unwrap();
            assert_eq!(logs, original);
            assert_eq!(
                open(paths)
                    .unwrap()
                    .query_row("SELECT COUNT(*) FROM provider_key_bindings", [], |row| row
                        .get::<_, i64>(
                        0
                    ))
                    .unwrap(),
                4
            );
        });
    }

    #[test]
    fn infers_only_single_key_history_and_keeps_desktop_bindings_separate() {
        with_paths(|paths| {
            sync_bindings(paths, &Map::from_iter([
                ("provider".into(), json!({"apiKeyId": "cli-key", "apiKeyHash": "cli-hash", "singleKey": true})),
                ("claude-desktop:provider".into(), json!({"apiKeyId": "desktop-key", "apiKeyHash": "desktop-hash", "singleKey": false}))
            ]), 100).unwrap();
            let mut logs = vec![
                json!({"providerId": "provider", "appType": "claude", "createdAt": 50}),
                json!({"providerId": "provider", "appType": "claude-desktop", "createdAt": 50}),
                json!({"providerId": "provider", "appType": "claude-desktop", "createdAt": 100}),
            ];
            bind_logs(paths, &mut logs).unwrap();
            assert_eq!(logs[0]["apiKeyId"], "cli-key");
            assert_eq!(logs[0]["apiKeyAttribution"], "single-key-inferred");
            assert!(logs[1].get("apiKeyId").is_none());
            assert_eq!(logs[2]["apiKeyId"], "desktop-key");
            assert_eq!(logs[2]["apiKeyAttribution"], "active-key");
        });
    }

    #[test]
    fn tracks_the_exact_key_and_keeps_desktop_and_replaced_credentials_separate() {
        with_paths(|paths| {
            let mut first = KeyRequest::new(paths, "provider", "first", "first-secret");
            first.start();
            first.start();
            assert_eq!(
                read(paths, "provider", "first", "first-secret").unwrap()["inFlightCount"],
                1
            );
            let mut second = KeyRequest::new(paths, "provider", "second", "second-secret");
            second.start();
            second.response(401, false);
            second.finish();
            let mut desktop =
                KeyRequest::new(paths, "claude-desktop:provider", "first", "first-secret");
            desktop.start();
            desktop.response(429, false);
            desktop.finish();
            first.response(200, false);
            first.observe(b"{\"ok\":true}");
            first.finish();
            first.finish();
            let usage = read(paths, "provider", "first", "first-secret").unwrap();
            assert_eq!(usage["requestCount"], 1);
            assert_eq!(usage["successCount"], 1);
            assert_eq!(usage["failureCount"], 0);
            assert_eq!(usage["inFlightCount"], 0);
            assert_eq!(
                read(paths, "provider", "first", "replacement-secret").unwrap()["requestCount"],
                0
            );
            assert_eq!(
                read(paths, "provider", "second", "second-secret").unwrap()["lastErrorKind"],
                "auth"
            );
            assert_eq!(
                read(paths, "claude-desktop:provider", "first", "first-secret").unwrap()
                    ["lastErrorKind"],
                "rate_limit"
            );
            assert!(!usage.to_string().contains("secret"));
            assert!(usage.get("key_hash").is_none());
        });
    }

    #[test]
    fn failures_cancellation_and_success_update_consecutive_counts() {
        with_paths(|paths| {
            for status in [429, 503] {
                let mut request = KeyRequest::new(paths, "provider", "key", "secret");
                request.start();
                request.response(status, false);
                request.finish();
            }
            assert_eq!(
                read(paths, "provider", "key", "secret").unwrap()["consecutiveFailures"],
                2
            );
            let mut request = KeyRequest::new(paths, "provider", "key", "secret");
            request.start();
            request.response(200, false);
            request.finish();
            let success = read(paths, "provider", "key", "secret").unwrap();
            assert_eq!(success["consecutiveFailures"], 0);
            assert_eq!(success["lastOutcome"], "success");
            assert_eq!(success["lastFailureKind"], "upstream");
            let mut cancelled = KeyRequest::new(paths, "provider", "key", "secret");
            cancelled.start();
            drop(cancelled);
            let usage = read(paths, "provider", "key", "secret").unwrap();
            assert_eq!(usage["requestCount"], 4);
            assert_eq!(usage["failureCount"], 3);
            assert_eq!(usage["successCount"], 1);
            assert_eq!(usage["consecutiveFailures"], 1);
            assert_eq!(usage["lastErrorKind"], "cancelled");
            assert_eq!(usage["inFlightCount"], 0);
            let mut local_error = KeyRequest::new(paths, "provider", "unused", "secret");
            local_error.fail("network");
            assert_eq!(
                read(paths, "provider", "unused", "secret").unwrap()["requestCount"],
                0
            );
        });
    }

    #[test]
    fn streaming_and_json_errors_are_failures_even_with_http_200() {
        with_paths(|paths| {
            let cases = [
                (
                    "json",
                    false,
                    b"{\"error\":{\"message\":\"private response\"}}".as_slice(),
                    "api",
                ),
                (
                    "sse",
                    true,
                    b"data: {\"type\":\"response.failed\"}\r\n\r\n".as_slice(),
                    "api",
                ),
                (
                    "event",
                    true,
                    b"event: error\ndata: upstream failed\n\n".as_slice(),
                    "api",
                ),
                (
                    "multiline",
                    true,
                    b"data: {\ndata: \"type\": \"error\"\ndata: }\n\n".as_slice(),
                    "api",
                ),
                (
                    "incomplete",
                    true,
                    b"data: {\"type\":\"message_start\"}\n\n".as_slice(),
                    "stream",
                ),
                (
                    "complete",
                    true,
                    b"data: {\"type\":\"message_start\"}\n\ndata: {\"type\":\"message_stop\"}\n\n"
                        .as_slice(),
                    "",
                ),
                (
                    "chat",
                    true,
                    b"data: {\"object\":\"chat.completion.chunk\"}\n\ndata: [DONE]\n\n".as_slice(),
                    "",
                ),
            ];
            for (key_id, streaming, body, expected) in cases {
                let mut request = KeyRequest::new(paths, "provider", key_id, "secret");
                request.start();
                request.response(200, streaming);
                for chunk in body.chunks(3) {
                    request.observe(chunk);
                }
                request.finish();
                let usage = read(paths, "provider", key_id, "secret").unwrap();
                assert_eq!(usage["lastErrorKind"], expected, "{key_id}");
                assert_eq!(usage["failureCount"], i64::from(!expected.is_empty()));
                assert!(!usage.to_string().contains("private response"));
            }
            let mut oversized = KeyRequest::new(paths, "provider", "oversized", "secret");
            oversized.start();
            oversized.response(200, true);
            oversized.observe(&vec![b'a'; MAX_ERROR_BUFFER + 1]);
            assert!(oversized.error_buffer.len() <= MAX_ERROR_BUFFER);
            oversized.observe(b"\n\ndata: {\"type\":\"error\"}\n\n");
            oversized.finish();
            assert_eq!(
                read(paths, "provider", "oversized", "secret").unwrap()["failureCount"],
                1
            );
            let mut closed_after_error =
                KeyRequest::new(paths, "provider", "closed-after-error", "secret");
            closed_after_error.start();
            closed_after_error.response(200, true);
            closed_after_error.observe(b"event: error\ndata: failed\n\n");
            drop(closed_after_error);
            assert_eq!(
                read(paths, "provider", "closed-after-error", "secret").unwrap()["lastErrorKind"],
                "api"
            );
        });
    }

    #[test]
    fn concurrent_results_are_atomic_and_usage_never_enters_backups() {
        with_paths(|paths| {
            std::thread::scope(|scope| {
                for index in 0..12 {
                    scope.spawn(move || {
                        let mut request = KeyRequest::new(paths, "provider", "key", "secret");
                        request.start();
                        request.response(if index % 3 == 0 { 429 } else { 200 }, false);
                        request.finish();
                    });
                }
            });
            let usage = read(paths, "provider", "key", "secret").unwrap();
            assert_eq!(usage["requestCount"], 12);
            assert_eq!(usage["successCount"], 8);
            assert_eq!(usage["failureCount"], 4);
            assert_eq!(usage["inFlightCount"], 0);
            let snapshot_path = std::path::Path::new(&paths.temp_dir).join("key-usage-snapshot.db");
            std::fs::write(&snapshot_path, database::backup(paths).unwrap()).unwrap();
            let snapshot = Connection::open(snapshot_path).unwrap();
            let count: i64 = snapshot
                .query_row("SELECT COUNT(*) FROM provider_keys", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 0);
            assert_eq!(
                read(paths, "provider", "key", "secret").unwrap()["requestCount"],
                12
            );
        });
    }
}

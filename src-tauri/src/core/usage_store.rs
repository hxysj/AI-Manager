use crate::core::database;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

mod requests;

pub struct UsageLogQuery {
    pub app_type: String,
    pub provider_id: String,
    pub provider_ids: Vec<String>,
    pub model: String,
    pub request_source: String,
    pub start_at: u64,
    pub end_at: u64,
}

pub struct UsageSessionUpdate {
    pub raw_path: String,
    pub app_type: String,
    pub updated_at: u64,
    pub logs: Vec<Value>,
    pub records: Vec<Value>,
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = open_connection(paths)?;
    migrate_connection(&mut connection)?;
    migrate_legacy_database(paths, &mut connection)?;
    migrate_legacy_json(paths, &mut connection)?;
    remove_legacy_usage_files(paths)
}

// 备份恢复和旧独立库同样经过这一入口，避免恢复后重新出现旧表。
pub(crate) fn migrate_connection(connection: &mut Connection) -> Result<(), ManagerError> {
    create_schema(connection)?;
    requests::migrate(connection)?;
    connection.execute(
        "INSERT OR IGNORE INTO app_metadata(key,value) VALUES ('usage:revision','0')",
        [],
    )?;
    migrate_session_fields(connection)?;
    // 迁移根据实际表结构判断，完成后移除旧的版本记录表。
    connection.execute_batch("DROP TABLE IF EXISTS schema_migrations;")?;
    Ok(())
}

pub fn read_pricing(paths: &AppPaths) -> Result<Value, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let exchange_rate = connection
        .query_row(
            "SELECT CAST(value AS REAL) FROM app_metadata WHERE key = 'usage:exchange_rate'",
            [],
            |row| row.get::<_, f64>(0),
        )
        .optional()?
        .unwrap_or(7.2);
    let mut statement = connection.prepare(
        "SELECT id, model_id, model_category, currency,
                input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
         FROM usage_pricing_items
         ORDER BY sort_order ASC, id ASC",
    )?;
    let items = statement
        .query_map([], |row| {
            Ok(json!({
              "id": row.get::<_, String>(0)?,
              "modelId": row.get::<_, String>(1)?,
              "modelCategory": row.get::<_, String>(2)?,
              "currency": row.get::<_, String>(3)?,
              "inputCostPerMillion": row.get::<_, f64>(4)?,
              "outputCostPerMillion": row.get::<_, f64>(5)?,
              "cacheReadCostPerMillion": row.get::<_, f64>(6)?,
              "cacheCreationCostPerMillion": row.get::<_, f64>(7)?
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(json!({
      "exchangeRate": exchange_rate,
      "items": items
    }))
}

pub fn write_pricing(paths: &AppPaths, pricing: &Value) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;

    replace_pricing(&transaction, pricing)?;
    bump_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn query_logs(paths: &AppPaths, query: &UsageLogQuery) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let (where_sql, parameters) = build_log_where(query);
    let sql = format!(
        "SELECT {} FROM usage_requests {where_sql} ORDER BY created_at DESC, request_id ASC",
        requests::columns()
    );
    read_log_query(&connection, &sql, parameters)
}

pub fn read_all_logs(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    read_log_query(
        &connection,
        &format!("SELECT {} FROM usage_requests WHERE record_state=1 ORDER BY created_at DESC, request_id ASC", requests::columns()),
        Vec::new(),
    )
}

pub fn bind_key_usage(paths: &AppPaths) -> Result<(), ManagerError> {
    let mut logs = read_all_logs(paths)?;
    let key_fields = |log: &Value| {
        [
            text(log.get("apiKeyId")),
            text(log.get("apiKeyHash")),
            text(log.get("apiKeyAttribution")),
        ]
    };
    let original = logs.iter().map(key_fields).collect::<Vec<_>>();
    crate::core::provider_key_usage::bind_logs(paths, &mut logs)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;
    let mut changed = false;
    for (log, before) in logs
        .iter()
        .zip(&original)
        .filter(|(log, before)| key_fields(log) != **before)
    {
        let request_id = text(log.get("requestId"));
        transaction.execute(
            "UPDATE usage_requests SET api_key_id=?1, api_key_hash=?2, api_key_attribution=?3 WHERE request_id=?4",
            params![text(log.get("apiKeyId")), text(log.get("apiKeyHash")), text(log.get("apiKeyAttribution")), request_id],
        )?;
        changed |= key_fields(log) != *before;
    }
    if changed {
        bump_revision(&transaction)?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn write_usage_cost_snapshots(paths: &AppPaths, logs: &[Value]) -> Result<(), ManagerError> {
    if logs.is_empty() {
        return Ok(());
    }

    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;
    let mut changed = false;
    let mut changed_paths = std::collections::HashSet::new();

    for log in logs {
        let request_id = text(log.get("requestId"));

        if request_id.is_empty() || number(log.get("costLockedAt")) <= 0 {
            continue;
        }

        let updated_path: Option<String> = transaction
            .query_row(
                "SELECT COALESCE(raw_path,'') FROM usage_requests WHERE request_id=?1 AND record_state=1",
                [&request_id],
                |row| row.get(0),
            )
            .optional()?;
        if updated_path.is_some() {
            insert_usage_log(&transaction, log)?;
        }
        if let Some(path) = updated_path {
            changed = true;
            changed_paths.insert(path);
        }
    }

    for path in changed_paths {
        update_session_totals(&transaction, Some(&path))?;
    }
    if changed {
        bump_revision(&transaction)?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn record_codex_quota_stages(
    paths: &AppPaths,
    account_id: &str,
    stages: &[Value],
) -> Result<(), ManagerError> {
    if account_id.is_empty() || stages.is_empty() {
        return Ok(());
    }

    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;

    for stage in stages {
        let window_key = text(stage.get("windowKey"));
        let reset_at = number(stage.get("resetAt"));
        let starts_at = number(stage.get("startsAt"));
        let observed_at = number(stage.get("observedAt"));

        if window_key.is_empty() || reset_at <= 0 || observed_at <= 0 {
            continue;
        }

        let completed_at = if starts_at > 0 {
            starts_at
        } else {
            observed_at
        };
        transaction.execute(
            "UPDATE codex_quota_stages
             SET completed_at = ?1
             WHERE account_id = ?2 AND window_key = ?3
               AND reset_at <> ?4 AND completed_at = 0",
            params![completed_at, account_id, window_key, reset_at],
        )?;

        let existing = transaction
            .query_row(
                "SELECT payload_json FROM codex_quota_stages
                 WHERE account_id = ?1 AND window_key = ?2 AND reset_at = ?3",
                params![account_id, window_key, reset_at],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|payload| serde_json::from_str::<Value>(&payload))
            .transpose()?;
        let first_observed_at = existing
            .as_ref()
            .map(|item| number(item.get("firstObservedAt")))
            .filter(|value| *value > 0)
            .unwrap_or(observed_at);
        let first_used_percent = existing
            .as_ref()
            .and_then(|item| item.get("firstUsedPercent"))
            .and_then(Value::as_f64)
            .unwrap_or_else(|| decimal(stage.get("usedPercent"), 0.0));
        let used_percent = existing
            .as_ref()
            .map(|item| decimal(item.get("usedPercent"), 0.0))
            .unwrap_or(0.0)
            .max(decimal(stage.get("usedPercent"), 0.0))
            .clamp(0.0, 100.0);
        let payload = json!({
          "id": format!("{account_id}:{window_key}:{reset_at}"),
          "accountId": account_id,
          "windowKey": window_key,
          "limitWindowSeconds": number(stage.get("limitWindowSeconds")),
          "startsAt": starts_at,
          "resetAt": reset_at,
          "firstObservedAt": first_observed_at,
          "lastObservedAt": observed_at,
          "firstUsedPercent": first_used_percent,
          "usedPercent": used_percent,
          "remainingPercent": (100.0 - used_percent).max(0.0)
        });

        transaction.execute(
            "INSERT INTO codex_quota_stages(
               stage_id, account_id, window_key, reset_at, starts_at,
               observed_at, completed_at, payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
             ON CONFLICT(account_id, window_key, reset_at) DO UPDATE SET
               starts_at = excluded.starts_at,
               observed_at = excluded.observed_at,
               completed_at = 0,
               payload_json = excluded.payload_json",
            params![
                format!("{account_id}:{window_key}:{reset_at}"),
                account_id,
                window_key,
                reset_at,
                starts_at,
                observed_at,
                serde_json::to_string(&payload)?
            ],
        )?;
    }

    transaction.commit()?;
    Ok(())
}

pub fn read_codex_quota_stages(
    paths: &AppPaths,
    account_id: &str,
) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut statement = connection.prepare(
        "SELECT completed_at, payload_json
         FROM codex_quota_stages
         WHERE account_id = ?1
         ORDER BY reset_at DESC, window_key ASC",
    )?;
    let rows = statement.query_map(params![account_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut stages = Vec::new();

    for row in rows {
        let (completed_at, payload) = row?;
        let mut stage = serde_json::from_str::<Value>(&payload)?;

        stage["active"] = json!(completed_at == 0);
        stage["completedAt"] = json!(completed_at.max(0));
        stages.push(stage);
    }
    Ok(stages)
}

pub fn read_app_types(paths: &AppPaths) -> Result<Vec<String>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut statement = connection.prepare(
        "SELECT DISTINCT app_type FROM usage_requests WHERE record_state=1 AND app_type <> '' ORDER BY app_type ASC",
    )?;

    let items = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

pub fn read_session_versions(
    paths: &AppPaths,
) -> Result<HashMap<String, (String, u64)>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut statement =
        connection.prepare("SELECT raw_path, app_type, updated_at FROM usage_sessions")?;
    let items = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            (row.get::<_, String>(1)?, row.get::<_, i64>(2)? as u64),
        ))
    })?;

    Ok(items.collect::<Result<HashMap<_, _>, _>>()?)
}

pub const SESSION_SKILL_PARSER_VERSION: u64 = 1;

pub struct SessionSkillUpdate {
    pub raw_path: String,
    pub app_type: String,
    pub session_id: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub calls: Vec<Value>,
}

pub fn read_session_skill_versions(
    paths: &AppPaths,
) -> Result<HashMap<String, (u64, u64)>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut statement = connection
        .prepare("SELECT raw_path, skill_updated_at, skill_parser_version FROM usage_sessions")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)? as u64),
            ))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(rows)
}

pub fn read_session_skill_calls(
    paths: &AppPaths,
    cli: &str,
    start_at: u64,
    end_at: u64,
) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut statement = connection.prepare(
        "SELECT s.raw_path, s.session_id, s.app_type, j.value FROM usage_sessions s, json_each(s.skill_calls) j
         WHERE (?1 = '' OR ?1 = 'all' OR s.app_type = ?1)
           AND (?2 = 0 OR json_extract(j.value, '$.createdAt') >= ?2)
           AND (?3 = 0 OR json_extract(j.value, '$.createdAt') <= ?3)
         ORDER BY json_extract(j.value, '$.createdAt'), s.raw_path, j.key"
    )?;
    let rows = statement.query_map(params![cli, to_i64(start_at), to_i64(end_at)], |row| {
        let raw: String = row.get(3)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            raw,
        ))
    })?;
    let mut calls = Vec::new();
    for row in rows {
        let (path, session, cli, raw) = row?;
        let mut call: Value = serde_json::from_str(&raw)?;
        call["rawPath"] = json!(path);
        call["sessionId"] = json!(session);
        call["cli"] = json!(cli);
        calls.push(call);
    }
    Ok(calls)
}

pub fn write_session_skills(
    paths: &AppPaths,
    updates: &[SessionSkillUpdate],
) -> Result<(), ManagerError> {
    if updates.is_empty() {
        return Ok(());
    }
    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;
    for update in updates {
        transaction.execute(
            "INSERT INTO usage_sessions(raw_path, app_type, updated_at, session_id, created_at, skill_calls, skill_updated_at, skill_parser_version)
             VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(raw_path) DO UPDATE SET
               app_type=excluded.app_type, session_id=excluded.session_id,
               created_at=CASE WHEN usage_sessions.created_at > 0 THEN usage_sessions.created_at ELSE excluded.created_at END,
               skill_calls=excluded.skill_calls, skill_updated_at=excluded.skill_updated_at, skill_parser_version=excluded.skill_parser_version",
            params![update.raw_path, update.app_type, update.session_id, to_i64(update.created_at),
                serde_json::to_string(&update.calls)?, to_i64(update.updated_at), to_i64(SESSION_SKILL_PARSER_VERSION)],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn ensure_session_parser_version(
    paths: &AppPaths,
    app_type: &str,
    version: u64,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let key = format!("usage:session_parser_version:{app_type}");
    let current_version = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if current_version.as_deref() == Some(&version.to_string()) {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    if app_type == "claude-desktop" {
        transaction.execute(
            "UPDATE usage_sessions SET updated_at = 0 WHERE app_type = ?1",
            params![app_type],
        )?;
    } else {
        transaction.execute(
            "UPDATE usage_requests SET record_state=2
             WHERE record_state=1 AND raw_path IN (
               SELECT raw_path FROM usage_sessions WHERE app_type = ?1
             )",
            params![app_type],
        )?;
        transaction.execute(
            "UPDATE usage_sessions SET updated_at = 0 WHERE app_type = ?1",
            params![app_type],
        )?;
    }
    transaction.execute("UPDATE usage_sessions SET request_count=0, input_tokens=0, output_tokens=0, cache_read_tokens=0,
        cache_creation_tokens=0, actual_tokens=0, total_cost_usd=0 WHERE app_type=?1 AND NOT EXISTS(
        SELECT 1 FROM usage_requests WHERE record_state=1 AND usage_requests.raw_path=usage_sessions.raw_path)", [app_type])?;
    transaction.execute(
        "INSERT INTO app_metadata(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, version.to_string()],
    )?;
    bump_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_request_records(
    paths: &AppPaths,
    request_ids: &[String],
) -> Result<HashMap<String, Value>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut output = HashMap::new();

    for chunk in request_ids.chunks(500) {
        if chunk.is_empty() {
            continue;
        }

        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {} FROM usage_requests WHERE request_id IN ({placeholders})",
            requests::columns()
        );
        let records = read_log_query(
            &connection,
            &sql,
            chunk.iter().cloned().map(SqlValue::Text).collect(),
        )?;
        for record in records {
            output.insert(text(record.get("requestId")), record);
        }
    }

    Ok(output)
}

pub fn write_request_record(paths: &AppPaths, record: &Value) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;
    insert_request_record(&transaction, record)?;
    transaction.commit()?;
    Ok(())
}

pub fn replace_sessions(
    paths: &AppPaths,
    updates: &[UsageSessionUpdate],
) -> Result<(), ManagerError> {
    if updates.is_empty() {
        return Ok(());
    }

    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;

    for update in updates {
        let existing_logs = read_log_query(
            &transaction,
            &format!(
                "SELECT {} FROM usage_requests WHERE raw_path=?",
                requests::columns()
            ),
            vec![SqlValue::Text(update.raw_path.clone())],
        )?
        .into_iter()
        .map(|log| (text(log.get("requestId")), log))
        .collect::<HashMap<_, _>>();
        transaction.execute(
            "UPDATE usage_requests SET record_state=2 WHERE raw_path=?1 AND record_state=1",
            params![update.raw_path],
        )?;

        for log in &update.logs {
            let mut next_log = log.clone();
            let request_id = text(log.get("requestId"));

            if let Some(existing) = existing_logs
                .get(&request_id)
                .filter(|item| number(item.get("costLockedAt")) > 0)
                .filter(|item| update.app_type != "claude-desktop" || item["model"] == log["model"])
            {
                for field in [
                    "pricingSnapshot",
                    "actualTokens",
                    "inputCostUsd",
                    "outputCostUsd",
                    "cacheReadCostUsd",
                    "cacheCreationCostUsd",
                    "totalCostUsd",
                    "costLockedAt",
                ] {
                    if let Some(value) = existing.get(field) {
                        next_log[field] = value.clone();
                    }
                }
            }
            insert_usage_log(&transaction, &next_log)?;
        }

        for record in &update.records {
            insert_request_record(&transaction, record)?;
        }

        transaction.execute(
            "INSERT INTO usage_sessions(raw_path, app_type, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(raw_path) DO UPDATE SET
               app_type = excluded.app_type,
               updated_at = excluded.updated_at",
            params![update.raw_path, update.app_type, to_i64(update.updated_at)],
        )?;
        update_session_totals(&transaction, Some(&update.raw_path))?;
    }

    bump_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn revision(paths: &AppPaths) -> Result<u64, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;

    Ok(connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = 'usage:revision'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0))
}

fn open_connection(paths: &AppPaths) -> Result<Connection, ManagerError> {
    database::open(paths)
}

fn create_schema(connection: &Connection) -> Result<(), ManagerError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS usage_sessions (
           raw_path TEXT PRIMARY KEY,
           app_type TEXT NOT NULL DEFAULT '',
           updated_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS usage_pricing_items (
           id TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           model_id TEXT NOT NULL,
           model_category TEXT NOT NULL DEFAULT '',
           currency TEXT NOT NULL DEFAULT 'USD',
           input_cost_per_million REAL NOT NULL DEFAULT 0,
           output_cost_per_million REAL NOT NULL DEFAULT 0,
           cache_read_cost_per_million REAL NOT NULL DEFAULT 0,
           cache_creation_cost_per_million REAL NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_usage_pricing_model_id
           ON usage_pricing_items(model_id);
         CREATE TABLE IF NOT EXISTS codex_quota_stages (
           stage_id TEXT PRIMARY KEY,
           account_id TEXT NOT NULL,
           window_key TEXT NOT NULL,
           reset_at INTEGER NOT NULL,
           starts_at INTEGER NOT NULL DEFAULT 0,
           observed_at INTEGER NOT NULL,
           completed_at INTEGER NOT NULL DEFAULT 0,
           payload_json TEXT NOT NULL,
           UNIQUE(account_id, window_key, reset_at)
         );
         CREATE INDEX IF NOT EXISTS idx_codex_quota_stages_account_reset
           ON codex_quota_stages(account_id, reset_at DESC);
         CREATE TABLE IF NOT EXISTS app_metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    requests::create_schema(connection)
}

fn migrate_session_fields(connection: &mut Connection) -> Result<(), ManagerError> {
    let columns = connection
        .prepare("PRAGMA table_info(usage_sessions)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    let has_old: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='skill_usage_session_records')", [], |row| row.get(0),
    )?;
    if columns.iter().any(|column| column == "skill_calls") && !has_old {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    for (column, definition) in [
        ("session_id", "TEXT NOT NULL DEFAULT ''"),
        ("created_at", "INTEGER NOT NULL DEFAULT 0"),
        ("skill_calls", "TEXT NOT NULL DEFAULT '[]'"),
        ("skill_updated_at", "INTEGER NOT NULL DEFAULT 0"),
        ("skill_parser_version", "INTEGER NOT NULL DEFAULT 0"),
        ("request_count", "INTEGER NOT NULL DEFAULT 0"),
        ("input_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("output_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("cache_read_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("cache_creation_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("actual_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("total_cost_usd", "REAL NOT NULL DEFAULT 0"),
    ] {
        if !columns.iter().any(|name| name == column) {
            transaction.execute_batch(&format!(
                "ALTER TABLE usage_sessions ADD COLUMN {column} {definition}"
            ))?;
        }
    }
    // 将请求明细汇总到现有会话，不复制正文；明细仍保留模型和 Provider 的准确归属。
    update_session_totals(&transaction, None)?;
    if has_old {
        let mut statement = transaction.prepare(
            "SELECT raw_path, updated_at, payload_json FROM skill_usage_session_records",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let updated_at: i64 = row.get(1)?;
            let payload: Vec<Value> = serde_json::from_str(&row.get::<_, String>(2)?)?;
            let calls = crate::api::usage::compact_skill_records(&payload);
            let lowered = path.to_lowercase();
            let cli = if lowered.contains("codex") {
                "codex"
            } else if lowered.contains("claude") {
                "claude"
            } else if lowered.contains("gemini") {
                "gemini"
            } else {
                ""
            };
            let created_at = calls
                .iter()
                .filter_map(|call| call["createdAt"].as_u64())
                .filter(|time| *time > 0)
                .min()
                .unwrap_or(0);
            transaction.execute(
                "INSERT INTO usage_sessions(raw_path, app_type, updated_at, session_id, created_at, skill_calls, skill_updated_at, skill_parser_version)
                 VALUES (?1, ?2, 0, ?1, ?3, ?4, ?5, ?6)
                 ON CONFLICT(raw_path) DO UPDATE SET
                   app_type=CASE WHEN usage_sessions.app_type='' THEN excluded.app_type ELSE usage_sessions.app_type END,
                   created_at=CASE WHEN usage_sessions.created_at>0 THEN usage_sessions.created_at ELSE excluded.created_at END,
                   skill_calls=excluded.skill_calls, skill_updated_at=excluded.skill_updated_at, skill_parser_version=excluded.skill_parser_version",
                params![path, cli, to_i64(created_at), serde_json::to_string(&calls)?, updated_at, to_i64(SESSION_SKILL_PARSER_VERSION)],
            )?;
        }
        drop(rows);
        drop(statement);
        transaction.execute_batch("DROP TABLE skill_usage_session_records;")?;
    }
    transaction.execute_batch("CREATE INDEX IF NOT EXISTS idx_usage_sessions_cli_session ON usage_sessions(app_type, session_id);")?;
    transaction.commit()?;
    Ok(())
}

fn update_session_totals(
    transaction: &Transaction<'_>,
    raw_path: Option<&str>,
) -> Result<(), ManagerError> {
    transaction.execute(
        "INSERT INTO usage_sessions(raw_path, app_type, updated_at, session_id, created_at, request_count,
           input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, actual_tokens, total_cost_usd)
         SELECT raw_path, COALESCE(MAX(app_type),''), COALESCE(MAX(session_updated_at),0),
           COALESCE(MAX(NULLIF(session_id, '')), raw_path), COALESCE(MIN(created_at),0), COUNT(*),
           SUM(COALESCE(input_tokens,0)), SUM(COALESCE(output_tokens,0)),
           SUM(COALESCE(cache_read_tokens,0)), SUM(COALESCE(cache_creation_tokens,0)),
           SUM(COALESCE(actual_tokens,0)), SUM(COALESCE(total_cost_usd,0))
         FROM usage_requests WHERE record_state=1 AND raw_path <> '' AND (?1 IS NULL OR raw_path=?1) GROUP BY raw_path
         ON CONFLICT(raw_path) DO UPDATE SET app_type=excluded.app_type, session_id=excluded.session_id, created_at=excluded.created_at,
           request_count=excluded.request_count, input_tokens=excluded.input_tokens, output_tokens=excluded.output_tokens,
           cache_read_tokens=excluded.cache_read_tokens, cache_creation_tokens=excluded.cache_creation_tokens,
           actual_tokens=excluded.actual_tokens, total_cost_usd=excluded.total_cost_usd",
        params![raw_path],
    )?;
    if let Some(path) = raw_path {
        transaction.execute(
            "UPDATE usage_sessions SET request_count=0, input_tokens=0, output_tokens=0, cache_read_tokens=0,
             cache_creation_tokens=0, actual_tokens=0, total_cost_usd=0 WHERE raw_path=?1
             AND NOT EXISTS(SELECT 1 FROM usage_requests WHERE record_state=1 AND raw_path=?1)", [path],
        )?;
    }
    Ok(())
}

fn migrate_legacy_json(paths: &AppPaths, connection: &mut Connection) -> Result<(), ManagerError> {
    let log_path = Path::new(&paths.storage_files.usage_logs);
    let record_path = Path::new(&paths.storage_files.usage_request_records);
    let pricing_path = Path::new(&paths.storage_files.usage_pricing);

    if !log_path.exists() && !record_path.exists() && !pricing_path.exists() {
        return Ok(());
    }

    let logs = read_json_array(log_path)?;
    let records = read_json_array(record_path)?;
    let pricing = read_json_value(pricing_path)?;
    let transaction = connection.transaction()?;

    for log in &logs {
        insert_usage_log(&transaction, log)?;
    }

    for record in &records {
        insert_request_record(&transaction, record)?;
    }

    if let Some(pricing) = pricing.as_ref() {
        replace_pricing(&transaction, pricing)?;
    }

    if !logs.is_empty() {
        rebuild_session_versions(&transaction)?;
        update_session_totals(&transaction, None)?;
    }
    bump_revision(&transaction)?;
    transaction.commit()?;

    Ok(())
}

fn migrate_legacy_database(
    paths: &AppPaths,
    connection: &mut Connection,
) -> Result<(), ManagerError> {
    let legacy_path = Path::new(&paths.storage_files.usage_database);

    if !legacy_path.exists() || Path::new(&paths.storage_files.database) == legacy_path {
        return Ok(());
    }

    let mut legacy_connection = Connection::open(legacy_path)?;
    let integrity =
        legacy_connection.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;

    if integrity != "ok" {
        return Err(ManagerError::System(format!(
            "旧 Usage 数据库校验失败：{integrity}"
        )));
    }
    migrate_connection(&mut legacy_connection)?;
    legacy_connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(legacy_connection);

    connection.execute(
        "ATTACH DATABASE ?1 AS legacy_usage",
        params![paths.storage_files.usage_database],
    )?;
    let migration_result = (|| {
        let transaction = connection.transaction()?;
        let request_columns = format!("{},record_state", requests::columns());
        let session_columns = "raw_path,app_type,updated_at,session_id,created_at,skill_calls,skill_updated_at,skill_parser_version,
            request_count,input_tokens,output_tokens,cache_read_tokens,cache_creation_tokens,actual_tokens,total_cost_usd";
        let pricing_columns = "id,sort_order,model_id,model_category,currency,input_cost_per_million,output_cost_per_million,
            cache_read_cost_per_million,cache_creation_cost_per_million";
        transaction.execute_batch(&format!(
            "INSERT OR REPLACE INTO usage_requests({request_columns}) SELECT {request_columns} FROM legacy_usage.usage_requests;
             INSERT OR REPLACE INTO usage_sessions({session_columns}) SELECT {session_columns} FROM legacy_usage.usage_sessions;
             INSERT OR REPLACE INTO usage_pricing_items({pricing_columns}) SELECT {pricing_columns} FROM legacy_usage.usage_pricing_items;
             INSERT INTO app_metadata(key,value) SELECT key,value FROM legacy_usage.app_metadata
             WHERE key LIKE 'usage:%' AND key<>'usage:revision'
             ON CONFLICT(key) DO UPDATE SET value=excluded.value;",
        ))?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok::<(), ManagerError>(())
    })();
    connection.execute_batch("DETACH DATABASE legacy_usage")?;
    migration_result
}

fn insert_usage_log(transaction: &Transaction<'_>, log: &Value) -> Result<(), ManagerError> {
    requests::upsert(transaction, log, true)
}

fn insert_request_record(
    transaction: &Transaction<'_>,
    record: &Value,
) -> Result<(), ManagerError> {
    requests::upsert(transaction, record, false)
}

fn replace_pricing(transaction: &Transaction<'_>, pricing: &Value) -> Result<(), ManagerError> {
    let exchange_rate = decimal(pricing.get("exchangeRate"), 7.2);

    transaction.execute(
        "INSERT INTO app_metadata(key, value) VALUES ('usage:exchange_rate', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![exchange_rate.to_string()],
    )?;
    transaction.execute("DELETE FROM usage_pricing_items", [])?;

    for (index, item) in pricing
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let id = non_empty_text(item.get("id"), &format!("pricing-legacy-{index}"));
        let model_id = text(item.get("modelId"));

        if model_id.is_empty() {
            continue;
        }

        transaction.execute(
            "INSERT OR REPLACE INTO usage_pricing_items(
               id, sort_order, model_id, model_category, currency,
               input_cost_per_million, output_cost_per_million,
               cache_read_cost_per_million, cache_creation_cost_per_million
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                index as i64,
                model_id,
                non_empty_text(
                    item.get("modelCategory").or_else(|| item.get("category")),
                    ""
                ),
                non_empty_text(item.get("currency"), "USD").to_uppercase(),
                decimal(item.get("inputCostPerMillion"), 0.0),
                decimal(item.get("outputCostPerMillion"), 0.0),
                decimal(item.get("cacheReadCostPerMillion"), 0.0),
                decimal(item.get("cacheCreationCostPerMillion"), 0.0)
            ],
        )?;
    }
    Ok(())
}

fn rebuild_session_versions(transaction: &Transaction<'_>) -> Result<(), ManagerError> {
    transaction.execute_batch(
        "INSERT INTO usage_sessions(raw_path, app_type, updated_at)
         SELECT raw_path, COALESCE(MAX(app_type),''), COALESCE(MAX(session_updated_at),0)
         FROM usage_requests
         WHERE record_state=1 AND raw_path <> ''
         GROUP BY raw_path
         ON CONFLICT(raw_path) DO UPDATE SET
           app_type = excluded.app_type,
           updated_at = excluded.updated_at;",
    )?;
    Ok(())
}

fn bump_revision(transaction: &Transaction<'_>) -> Result<(), ManagerError> {
    transaction.execute(
        "UPDATE app_metadata
         SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
         WHERE key = 'usage:revision'",
        [],
    )?;
    Ok(())
}

fn build_log_where(query: &UsageLogQuery) -> (String, Vec<SqlValue>) {
    let mut clauses: Vec<String> = vec!["record_state=1".to_string()];
    let mut parameters = Vec::new();

    if query.app_type != "all" {
        clauses.push("app_type = ?".to_string());
        parameters.push(SqlValue::Text(query.app_type.clone()));
    }

    if !query.provider_ids.is_empty() {
        let placeholders = std::iter::repeat_n("?", query.provider_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("provider_id IN ({placeholders})"));
        parameters.extend(query.provider_ids.iter().cloned().map(SqlValue::Text));
    } else if query.provider_id != "all" {
        clauses.push("provider_id = ?".to_string());
        parameters.push(SqlValue::Text(query.provider_id.clone()));
    }

    if query.model != "all" {
        clauses.push("model = ?".to_string());
        parameters.push(SqlValue::Text(query.model.clone()));
    }

    if query.request_source != "all" {
        clauses.push("request_source = ?".to_string());
        parameters.push(SqlValue::Text(query.request_source.clone()));
    }

    if query.start_at > 0 {
        clauses.push("created_at >= ?".to_string());
        parameters.push(SqlValue::Integer(to_i64(query.start_at)));
    }

    if query.end_at > 0 {
        clauses.push("created_at <= ?".to_string());
        parameters.push(SqlValue::Integer(to_i64(query.end_at)));
    }

    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    (where_sql, parameters)
}

fn read_log_query(
    connection: &Connection,
    sql: &str,
    parameters: Vec<SqlValue>,
) -> Result<Vec<Value>, ManagerError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params_from_iter(parameters.iter()), requests::from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn read_json_array(path: &Path) -> Result<Vec<Value>, ManagerError> {
    Ok(read_json_value(path)?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default())
}

fn read_json_value(path: &Path) -> Result<Option<Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(serde_json::from_str(&content)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn remove_legacy_usage_files(paths: &AppPaths) -> Result<(), ManagerError> {
    for path in [
        &paths.storage_files.usage_logs,
        &paths.storage_files.usage_request_records,
        &paths.storage_files.usage_pricing,
    ] {
        let path = Path::new(path);
        let Some(parent) = path.parent() else {
            continue;
        };
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };

        if !parent.exists() {
            continue;
        }

        for entry in std::fs::read_dir(parent)? {
            let entry = entry?;
            let entry_name = entry.file_name().to_string_lossy().to_string();

            if entry.path().is_file()
                && (entry_name == file_name || entry_name.starts_with(&format!("{file_name}.")))
            {
                std::fs::remove_file(entry.path())?;
            }
        }
    }

    let legacy_database = Path::new(&paths.storage_files.usage_database);
    for path in [
        legacy_database.to_path_buf(),
        legacy_database.with_extension("db-wal"),
        legacy_database.with_extension("db-shm"),
    ] {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn non_empty_text(value: Option<&Value>, fallback: &str) -> String {
    let value = text(value);
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn number(value: Option<&Value>) -> i64 {
    value
        .and_then(Value::as_i64)
        .or_else(|| value.and_then(Value::as_u64).map(to_i64))
        .or_else(|| value.and_then(Value::as_f64).map(|value| value as i64))
        .unwrap_or(0)
}

fn decimal(value: Option<&Value>, fallback: f64) -> f64 {
    value
        .and_then(Value::as_f64)
        .or_else(|| {
            value
                .and_then(Value::as_str)
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(fallback)
}

fn to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

#[cfg(test)]
mod tests {
    use super::{
        create_schema, initialize, query_logs, read_codex_quota_stages, read_pricing,
        read_request_records, read_session_skill_calls, read_session_skill_versions,
        read_session_versions, record_codex_quota_stages, replace_sessions, write_pricing,
        write_session_skills, SessionSkillUpdate, UsageLogQuery, UsageSessionUpdate,
    };
    use crate::core::paths::resolve_app_paths;
    use rusqlite::{params, Connection};
    use serde_json::json;
    use std::path::Path;

    fn create_test_paths(name: &str) -> crate::core::paths::AppPaths {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-usage-store-{name}-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(root.join("workspace/logs")).unwrap();
        std::fs::create_dir_all(root.join("workspace/storage")).unwrap();
        resolve_app_paths(Path::new(&root))
    }

    fn create_legacy_schema(connection: &Connection) {
        connection.execute_batch(
            "CREATE TABLE usage_logs(request_id TEXT PRIMARY KEY, raw_path TEXT NOT NULL DEFAULT '',
             created_at INTEGER NOT NULL, session_updated_at INTEGER NOT NULL DEFAULT 0,
             app_type TEXT NOT NULL DEFAULT '', provider_id TEXT NOT NULL DEFAULT '', model TEXT NOT NULL DEFAULT '',
             request_source TEXT NOT NULL DEFAULT 'session', payload_json TEXT NOT NULL);
             CREATE TABLE usage_request_records(request_id TEXT PRIMARY KEY, created_at INTEGER NOT NULL DEFAULT 0, payload_json TEXT NOT NULL);
             CREATE TABLE usage_sessions(raw_path TEXT PRIMARY KEY, app_type TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE usage_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             CREATE TABLE usage_pricing_config(id INTEGER PRIMARY KEY,exchange_rate REAL NOT NULL);
             CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at INTEGER NOT NULL);
             INSERT INTO schema_migrations VALUES(1,100),(2,200);"
        ).unwrap();
    }

    fn all_query() -> UsageLogQuery {
        UsageLogQuery {
            app_type: "all".to_string(),
            provider_id: "all".to_string(),
            provider_ids: Vec::new(),
            model: "all".to_string(),
            request_source: "all".to_string(),
            start_at: 0,
            end_at: 0,
        }
    }

    #[test]
    fn migrates_legacy_usage_json_into_sqlite() {
        let paths = create_test_paths("migration");
        std::fs::write(
            &paths.storage_files.usage_logs,
            serde_json::to_string(&json!([{
              "requestId": "request-1",
              "rawPath": "session-1.jsonl",
              "createdAt": 100,
              "sessionUpdatedAt": 120,
              "appType": "codex",
              "providerId": "provider-1",
              "model": "gpt-test",
              "requestSource": "session"
            }]))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.usage_request_records,
            serde_json::to_string(&json!([{
              "requestId": "request-1",
              "createdAt": 100,
              "providerId": "provider-1"
            }]))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.usage_pricing,
            serde_json::to_string(&json!({
              "exchangeRate": 7.3,
              "items": [{
                "id": "pricing-1",
                "modelId": "gpt-test",
                "currency": "USD",
                "inputCostPerMillion": 1
              }]
            }))
            .unwrap(),
        )
        .unwrap();

        initialize(&paths).unwrap();

        assert_eq!(query_logs(&paths, &all_query()).unwrap().len(), 1);
        assert_eq!(read_pricing(&paths).unwrap()["exchangeRate"], 7.3);
        assert!(Path::new(&paths.storage_files.database).exists());
        assert!(!Path::new(&paths.storage_files.usage_database).exists());
        assert!(!Path::new(&paths.storage_files.usage_logs).exists());
        assert!(!Path::new(&format!("{}.migrated", paths.storage_files.usage_logs)).exists());
        let connection = Connection::open(&paths.storage_files.database).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='schema_migrations'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn migrates_legacy_usage_database_into_main_database() {
        let paths = create_test_paths("database-migration");
        let legacy = Connection::open(&paths.storage_files.usage_database).unwrap();
        create_legacy_schema(&legacy);
        legacy
            .execute(
                "INSERT INTO usage_logs(
                   request_id, raw_path, created_at, session_updated_at,
                   app_type, provider_id, model, request_source, payload_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    "request-legacy",
                    "session-legacy.jsonl",
                    100,
                    120,
                    "codex",
                    "provider-legacy",
                    "gpt-legacy",
                    "session",
                    serde_json::to_string(&json!({"requestId": "request-legacy"})).unwrap()
                ],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO usage_sessions(raw_path, app_type, updated_at)
                 VALUES (?1, ?2, ?3)",
                params!["session-legacy.jsonl", "codex", 120],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO usage_request_records(request_id, created_at, payload_json)
                 VALUES (?1, ?2, ?3)",
                params![
                    "request-legacy",
                    100,
                    serde_json::to_string(&json!({"requestId": "request-legacy"})).unwrap()
                ],
            )
            .unwrap();
        drop(legacy);

        initialize(&paths).unwrap();

        assert_eq!(query_logs(&paths, &all_query()).unwrap().len(), 1);
        assert_eq!(read_session_versions(&paths).unwrap().len(), 1);
        assert_eq!(
            read_request_records(&paths, &["request-legacy".to_string()])
                .unwrap()
                .len(),
            1
        );
        assert!(!Path::new(&paths.storage_files.usage_database).exists());
        assert!(Path::new(&paths.storage_files.database).exists());
    }

    #[test]
    fn replaces_only_the_changed_session() {
        let paths = create_test_paths("incremental");
        initialize(&paths).unwrap();
        replace_sessions(
            &paths,
            &[
                UsageSessionUpdate {
                    raw_path: "session-1.jsonl".to_string(),
                    app_type: "codex".to_string(),
                    updated_at: 100,
                    logs: vec![json!({
                      "requestId": "request-1",
                      "rawPath": "session-1.jsonl",
                      "createdAt": 100,
                      "appType": "codex"
                    })],
                    records: Vec::new(),
                },
                UsageSessionUpdate {
                    raw_path: "session-2.jsonl".to_string(),
                    app_type: "codex".to_string(),
                    updated_at: 100,
                    logs: vec![json!({
                      "requestId": "request-2",
                      "rawPath": "session-2.jsonl",
                      "createdAt": 200,
                      "appType": "codex"
                    })],
                    records: Vec::new(),
                },
            ],
        )
        .unwrap();
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session-1.jsonl".to_string(),
                app_type: "codex".to_string(),
                updated_at: 200,
                logs: vec![json!({
                  "requestId": "request-3",
                  "rawPath": "session-1.jsonl",
                  "createdAt": 300,
                  "appType": "codex"
                })],
                records: Vec::new(),
            }],
        )
        .unwrap();

        let logs = query_logs(&paths, &all_query()).unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0]["requestId"], "request-3");
        assert_eq!(logs[1]["requestId"], "request-2");
    }

    #[test]
    fn filters_logs_and_persists_pricing() {
        let paths = create_test_paths("filters");
        initialize(&paths).unwrap();
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".to_string(),
                app_type: "codex".to_string(),
                updated_at: 100,
                logs: vec![
                    json!({
                      "requestId": "request-1",
                      "rawPath": "session.jsonl",
                      "createdAt": 100,
                      "appType": "codex",
                      "providerId": "provider-1",
                      "model": "gpt-a",
                      "requestSource": "session"
                    }),
                    json!({
                      "requestId": "request-2",
                      "rawPath": "session.jsonl",
                      "createdAt": 200,
                      "appType": "codex",
                      "providerId": "provider-2",
                      "model": "gpt-b",
                      "requestSource": "proxy-managed"
                    }),
                ],
                records: Vec::new(),
            }],
        )
        .unwrap();
        let query = UsageLogQuery {
            app_type: "codex".to_string(),
            provider_id: "provider-2".to_string(),
            provider_ids: Vec::new(),
            model: "gpt-b".to_string(),
            request_source: "proxy-managed".to_string(),
            start_at: 150,
            end_at: 250,
        };

        let logs = query_logs(&paths, &query).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["requestId"], "request-2");

        write_pricing(
            &paths,
            &json!({
              "exchangeRate": 7.5,
              "items": [{
                "id": "pricing-1",
                "modelId": "gpt-b",
                "modelCategory": "text",
                "currency": "CNY",
                "inputCostPerMillion": 2.5,
                "outputCostPerMillion": 5
              }]
            }),
        )
        .unwrap();
        let pricing = read_pricing(&paths).unwrap();
        assert_eq!(pricing["exchangeRate"], 7.5);
        assert_eq!(pricing["items"][0]["modelId"], "gpt-b");
        assert_eq!(pricing["items"][0]["currency"], "CNY");
    }

    #[test]
    fn records_codex_quota_stage_progress_and_reset_history() {
        let paths = create_test_paths("codex-quota-stage");

        record_codex_quota_stages(
            &paths,
            "account-1",
            &[json!({
              "windowKey": "primary",
              "limitWindowSeconds": 1,
              "startsAt": 1_000,
              "resetAt": 2_000,
              "observedAt": 1_100,
              "usedPercent": 10.0
            })],
        )
        .unwrap();
        record_codex_quota_stages(
            &paths,
            "account-1",
            &[json!({
              "windowKey": "primary",
              "limitWindowSeconds": 1,
              "startsAt": 1_000,
              "resetAt": 2_000,
              "observedAt": 1_200,
              "usedPercent": 40.0
            })],
        )
        .unwrap();

        let active_stages = read_codex_quota_stages(&paths, "account-1").unwrap();
        assert_eq!(active_stages.len(), 1);
        assert_eq!(active_stages[0]["firstObservedAt"], 1_100);
        assert_eq!(active_stages[0]["lastObservedAt"], 1_200);
        assert_eq!(active_stages[0]["firstUsedPercent"], 10.0);
        assert_eq!(active_stages[0]["usedPercent"], 40.0);
        assert_eq!(active_stages[0]["remainingPercent"], 60.0);
        assert_eq!(active_stages[0]["active"], true);

        record_codex_quota_stages(
            &paths,
            "account-1",
            &[json!({
              "windowKey": "primary",
              "limitWindowSeconds": 1,
              "startsAt": 2_000,
              "resetAt": 3_000,
              "observedAt": 2_100,
              "usedPercent": 5.0
            })],
        )
        .unwrap();

        let stages = read_codex_quota_stages(&paths, "account-1").unwrap();
        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0]["resetAt"], 3_000);
        assert_eq!(stages[0]["active"], true);
        assert_eq!(stages[1]["resetAt"], 2_000);
        assert_eq!(stages[1]["active"], false);
        assert_eq!(stages[1]["completedAt"], 2_000);
    }

    #[test]
    fn preserves_locked_cost_when_replacing_session_logs() {
        let paths = create_test_paths("locked-cost-reparse");

        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".to_string(),
                app_type: "codex".to_string(),
                updated_at: 100,
                logs: vec![json!({
                  "requestId": "request-1",
                  "rawPath": "session.jsonl",
                  "createdAt": 100,
                  "appType": "codex",
                  "inputTokens": 100,
                  "actualTokens": 120,
                  "totalCostUsd": 0.25,
                  "costLockedAt": 90,
                  "pricingSnapshot": { "pricingId": "pricing-old" }
                })],
                records: Vec::new(),
            }],
        )
        .unwrap();
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".to_string(),
                app_type: "codex".to_string(),
                updated_at: 200,
                logs: vec![json!({
                  "requestId": "request-1",
                  "rawPath": "session.jsonl",
                  "createdAt": 100,
                  "appType": "codex",
                  "inputTokens": 150
                })],
                records: Vec::new(),
            }],
        )
        .unwrap();

        let logs = query_logs(&paths, &all_query()).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["inputTokens"], 150);
        assert_eq!(logs[0]["actualTokens"], 120);
        assert_eq!(logs[0]["totalCostUsd"], 0.25);
        assert_eq!(logs[0]["costLockedAt"], 90);
        assert_eq!(logs[0]["pricingSnapshot"]["pricingId"], "pricing-old");
    }

    #[test]
    fn session_skill_updates_preserve_usage_and_duplicate_calls() {
        let paths = create_test_paths("session-skills");
        let update = SessionSkillUpdate {
            raw_path: "session.jsonl".into(),
            app_type: "codex".into(),
            session_id: "session-a".into(),
            created_at: 100,
            updated_at: 123,
            calls: vec![
                json!({"alias":"imagegen", "discoverable":true, "createdAt":100}),
                json!({"alias":"imagegen", "discoverable":true, "createdAt":110}),
            ],
        };
        write_session_skills(&paths, &[update]).unwrap();
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".into(),
                app_type: "codex".into(),
                updated_at: 124,
                logs: vec![
                    json!({"requestId":"r1", "sessionId":"session-a", "rawPath":"session.jsonl",
                "appType":"codex", "createdAt":100, "inputTokens":40, "outputTokens":10,
                "actualTokens":50, "totalCostUsd":0.5}),
                ],
                records: vec![],
            }],
        )
        .unwrap();
        let calls = read_session_skill_calls(&paths, "codex", 0, 0).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["sessionId"], "session-a");
        assert_eq!(
            read_session_skill_calls(&paths, "codex", 105, 115)
                .unwrap()
                .len(),
            1
        );
        assert!(read_session_skill_calls(&paths, "claude", 0, 0)
            .unwrap()
            .is_empty());
        let connection = Connection::open(&paths.storage_files.database).unwrap();
        let totals: (i64, i64, f64) = connection.query_row(
            "SELECT request_count, actual_tokens, total_cost_usd FROM usage_sessions WHERE raw_path='session.jsonl'",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
        assert_eq!(totals, (1, 50, 0.5));
        assert_eq!(
            read_session_skill_versions(&paths).unwrap()["session.jsonl"].0,
            123
        );
        write_session_skills(
            &paths,
            &[SessionSkillUpdate {
                raw_path: "session.jsonl".into(),
                app_type: "codex".into(),
                session_id: "session-a".into(),
                created_at: 100,
                updated_at: 125,
                calls: vec![],
            }],
        )
        .unwrap();
        assert_eq!(
            read_session_versions(&paths).unwrap()["session.jsonl"].1,
            124
        );
        assert_eq!(
            connection
                .query_row("SELECT actual_tokens FROM usage_sessions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            50
        );
    }

    #[test]
    fn migrates_legacy_skill_cache_without_session_text() {
        let paths = create_test_paths("legacy-skill-cache");
        let connection = Connection::open(&paths.storage_files.database).unwrap();
        create_schema(&connection).unwrap();
        connection.execute_batch("CREATE TABLE skill_usage_session_records(raw_path TEXT PRIMARY KEY, updated_at INTEGER, payload_json TEXT);").unwrap();
        connection
            .execute(
                "INSERT INTO skill_usage_session_records VALUES (?1,?2,?3)",
                params![
                    "codex/session.jsonl",
                    123,
                    json!([
                        {"display":"$imagegen", "timestamp":100, "rawPath":"codex/session.jsonl"},
                        {"display":"$imagegen", "timestamp":110, "rawPath":"codex/session.jsonl"}
                    ])
                    .to_string()
                ],
            )
            .unwrap();
        initialize(&paths).unwrap();
        initialize(&paths).unwrap();
        let calls = read_session_skill_calls(&paths, "codex", 0, 0).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["alias"], "imagegen");
        assert!(calls[0].get("display").is_none());
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='skill_usage_session_records'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        let stored: String = connection
            .query_row("SELECT skill_calls FROM usage_sessions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!stored.contains("display"));
        assert!(!stored.contains("rawPath"));
        assert!(stored.len() < 200);
    }
    #[test]
    fn merges_legacy_requests_without_payload_and_removes_old_tables() {
        let paths = create_test_paths("merged-requests");
        let connection = Connection::open(&paths.storage_files.database).unwrap();
        create_legacy_schema(&connection);
        let log = json!({"requestId":"shared", "rawPath":"session.jsonl", "sessionId":"session-a",
            "appType":"codex", "providerId":"historical", "apiKeyId":"key-old", "createdAt":100,
            "inputTokens":20, "outputTokens":2, "actualTokens":22, "totalCostUsd":0.5, "costLockedAt":90,
            "latencyMs":250, "pricingSnapshot":{"pricingId":"old-price", "modelId":"gpt-test",
            "currency":"CNY", "exchangeRate":7.3, "inputCostPerMillion":1.5,
            "outputCostPerMillion":3.5, "cacheReadCostPerMillion":0.2, "cacheCreationCostPerMillion":0.8, "lockedAt":90}});
        connection.execute("INSERT INTO usage_logs(request_id,raw_path,created_at,payload_json) VALUES ('shared','session.jsonl',100,?1)", [log.to_string()]).unwrap();
        connection.execute("INSERT INTO usage_logs(request_id,created_at,payload_json) VALUES ('log-only',101,?1)",
            [json!({"requestId":"log-only", "createdAt":101}).to_string()]).unwrap();
        for record in [
            json!({"requestId":"shared", "requestTime":100, "providerId":"historical"}),
            json!({"requestId":"pending", "createdAt":102, "providerId":"captured", "inputTokens":5}),
        ] {
            connection
                .execute(
                    "INSERT INTO usage_request_records VALUES (?1,100,?2)",
                    params![record["requestId"].as_str().unwrap(), record.to_string()],
                )
                .unwrap();
        }
        connection.execute_batch("INSERT INTO usage_metadata VALUES ('revision','451'),('session_parser_version:codex','2');
            INSERT INTO usage_pricing_config VALUES (1,7.3);").unwrap();
        initialize(&paths).unwrap();
        initialize(&paths).unwrap();
        let logs = query_logs(&paths, &all_query()).unwrap();
        assert_eq!(logs.len(), 2);
        let migrated = logs
            .iter()
            .find(|item| item["requestId"] == "shared")
            .unwrap();
        assert_eq!(migrated["pricingSnapshot"], log["pricingSnapshot"]);
        assert_eq!(migrated["latencyMs"], 250);
        assert_eq!(migrated["inputTokens"], 20);
        assert_eq!(migrated["totalCostUsd"], 0.5);
        assert_eq!(migrated["requestTime"], 100);
        assert_eq!(super::revision(&paths).unwrap(), 451);
        assert_eq!(read_pricing(&paths).unwrap()["exchangeRate"], 7.3);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM usage_requests", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
        let columns = connection
            .prepare("PRAGMA table_info(usage_requests)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!columns
            .iter()
            .any(|column| column.contains("json") || column.contains("payload")));
        for table in [
            "usage_logs",
            "usage_request_records",
            "usage_metadata",
            "usage_pricing_config",
            "schema_migrations",
        ] {
            assert_eq!(
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE name=?1",
                        [table],
                        |row| row.get::<_, i64>(0)
                    )
                    .unwrap(),
                0
            );
        }
        super::ensure_session_parser_version(&paths, "codex", 2).unwrap();
        assert_eq!(query_logs(&paths, &all_query()).unwrap().len(), 2);
        assert_eq!(
            connection
                .query_row(
                    "SELECT request_count FROM usage_sessions WHERE raw_path='session.jsonl'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn failed_request_migration_rolls_back_and_keeps_old_tables() {
        let paths = create_test_paths("merged-rollback");
        let connection = Connection::open(&paths.storage_files.database).unwrap();
        create_legacy_schema(&connection);
        connection
            .execute(
                "INSERT INTO usage_request_records VALUES ('good',100,?1)",
                [json!({"requestId":"good","providerId":"historical"}).to_string()],
            )
            .unwrap();
        connection.execute_batch("INSERT INTO usage_logs(request_id,created_at,payload_json) VALUES ('bad',100,'invalid-json');
            INSERT INTO usage_metadata VALUES ('revision','99'); INSERT INTO usage_pricing_config VALUES (1,7.4);").unwrap();
        assert!(initialize(&paths).is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM usage_requests", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('usage_logs','usage_request_records','usage_metadata','usage_pricing_config')",[],|row| row.get::<_,i64>(0)).unwrap(),4);
        connection
            .execute(
                "UPDATE usage_logs SET payload_json=?1",
                [json!({"requestId":"bad","createdAt":100}).to_string()],
            )
            .unwrap();
        initialize(&paths).unwrap();
        assert_eq!(super::revision(&paths).unwrap(), 99);
        assert_eq!(query_logs(&paths, &all_query()).unwrap().len(), 1);
    }

    #[test]
    fn parser_upgrade_preserves_attribution_and_locked_cost_for_rescan() {
        let paths = create_test_paths("merged-parser-upgrade");
        let log = json!({"requestId":"r1", "rawPath":"session.jsonl", "sessionId":"s1", "createdAt":100,
            "appType":"codex", "providerId":"historical", "apiKeyId":"historical-key", "model":"gpt-test",
            "inputTokens":10, "actualTokens":12, "totalCostUsd":0.25, "costLockedAt":90,
            "pricingSnapshot":{"pricingId":"old-price"}});
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".into(),
                app_type: "codex".into(),
                updated_at: 100,
                logs: vec![log],
                records: vec![],
            }],
        )
        .unwrap();
        super::ensure_session_parser_version(&paths, "codex", 3).unwrap();
        assert!(query_logs(&paths, &all_query()).unwrap().is_empty());
        let records = read_request_records(&paths, &["r1".into()]).unwrap();
        assert_eq!(records["r1"]["providerId"], "historical");
        assert_eq!(records["r1"]["apiKeyId"], "historical-key");
        // 重扫暂时不再产出该请求，也不能删除历史快照。
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".into(),
                app_type: "codex".into(),
                updated_at: 101,
                logs: vec![],
                records: vec![],
            }],
        )
        .unwrap();
        let mut rescanned = records["r1"].clone();
        rescanned.as_object_mut().unwrap().remove("costLockedAt");
        rescanned.as_object_mut().unwrap().remove("pricingSnapshot");
        rescanned["inputTokens"] = json!(20);
        rescanned["totalCostUsd"] = json!(99);
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "session.jsonl".into(),
                app_type: "codex".into(),
                updated_at: 102,
                logs: vec![rescanned],
                records: vec![json!({"requestId":"r1","inputTokens":999,"totalCostUsd":999})],
            }],
        )
        .unwrap();
        let logs = query_logs(&paths, &all_query()).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["inputTokens"], 20);
        assert_eq!(logs[0]["totalCostUsd"], 0.25);
        assert_eq!(logs[0]["costLockedAt"], 90);
        assert_eq!(logs[0]["pricingSnapshot"]["pricingId"], "old-price");
        assert_eq!(logs[0]["providerId"], "historical");
    }

    #[test]
    fn captured_request_is_completed_in_place_and_late_snapshot_does_not_overwrite_usage() {
        let paths = create_test_paths("merged-captured-request");
        let captured = json!({"requestId":"desktop-response:message-a", "appType":"claude-desktop",
            "providerId":"captured", "apiKeyId":"captured-key", "createdAt":100});
        super::write_request_record(&paths, &captured).unwrap();
        assert!(query_logs(&paths, &all_query()).unwrap().is_empty());
        let mut completed = read_request_records(&paths, &["desktop-response:message-a".into()])
            .unwrap()
            .remove("desktop-response:message-a")
            .unwrap();
        completed["inputTokens"] = json!(50);
        completed["rawPath"] = json!("desktop.json");
        completed["costLockedAt"] = json!(110);
        completed["totalCostUsd"] = json!(0.75);
        replace_sessions(
            &paths,
            &[UsageSessionUpdate {
                raw_path: "desktop.json".into(),
                app_type: "claude-desktop".into(),
                updated_at: 120,
                logs: vec![completed],
                records: vec![],
            }],
        )
        .unwrap();
        super::write_request_record(&paths, &captured).unwrap();
        let connection = Connection::open(&paths.storage_files.database).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM usage_requests", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let logs = query_logs(&paths, &all_query()).unwrap();
        assert_eq!(logs[0]["inputTokens"], 50);
        assert_eq!(logs[0]["totalCostUsd"], 0.75);
        assert_eq!(logs[0]["apiKeyId"], "captured-key");
    }
}

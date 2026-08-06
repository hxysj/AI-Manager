use crate::core::database;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

const SCHEMA_VERSION: i64 = 2;

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
    create_schema(&connection)?;
    migrate_legacy_database(paths, &mut connection)?;
    migrate_legacy_json(paths, &mut connection)?;
    remove_legacy_usage_files(paths)
}

pub fn read_pricing(paths: &AppPaths) -> Result<Value, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let exchange_rate = connection
        .query_row(
            "SELECT exchange_rate FROM usage_pricing_config WHERE id = 1",
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
        "SELECT payload_json FROM usage_logs {where_sql} ORDER BY created_at DESC, request_id ASC"
    );
    read_log_query(&connection, &sql, parameters)
}

pub fn read_all_logs(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    read_log_query(
        &connection,
        "SELECT payload_json FROM usage_logs ORDER BY created_at DESC, request_id ASC",
        Vec::new(),
    )
}

pub fn write_usage_cost_snapshots(
    paths: &AppPaths,
    logs: &[Value],
) -> Result<(), ManagerError> {
    if logs.is_empty() {
        return Ok(());
    }

    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;
    let mut changed = false;

    for log in logs {
        let request_id = text(log.get("requestId"));

        if request_id.is_empty() || number(log.get("costLockedAt")) <= 0 {
            continue;
        }

        changed |= transaction.execute(
            "UPDATE usage_logs SET payload_json = ?1 WHERE request_id = ?2",
            params![serde_json::to_string(log)?, request_id],
        )? > 0;
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

        let completed_at = if starts_at > 0 { starts_at } else { observed_at };
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
        "SELECT DISTINCT app_type FROM usage_logs WHERE app_type <> '' ORDER BY app_type ASC",
    )?;

    let items = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

pub fn read_session_versions(paths: &AppPaths) -> Result<HashMap<String, u64>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut statement = connection.prepare("SELECT raw_path, updated_at FROM usage_sessions")?;
    let items = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
    })?;

    Ok(items.collect::<Result<HashMap<_, _>, _>>()?)
}

pub fn read_skill_session_records(
    paths: &AppPaths,
) -> Result<HashMap<String, (u64, Vec<Value>)>, ManagerError> {
    initialize(paths)?;
    let connection = open_connection(paths)?;
    let mut statement = connection.prepare(
        "SELECT raw_path, updated_at, payload_json FROM skill_usage_session_records",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)? as u64,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut records = HashMap::new();

    for row in rows {
        let (raw_path, updated_at, payload) = row?;
        let payload = serde_json::from_str::<Value>(&payload)?;
        records.insert(
            raw_path,
            (updated_at, payload.as_array().cloned().unwrap_or_default()),
        );
    }

    Ok(records)
}

pub fn write_skill_session_records(
    paths: &AppPaths,
    records: &[(String, u64, Vec<Value>)],
) -> Result<(), ManagerError> {
    if records.is_empty() {
        return Ok(());
    }

    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let transaction = connection.transaction()?;

    for (raw_path, updated_at, payload) in records {
        transaction.execute(
            "INSERT INTO skill_usage_session_records(raw_path, updated_at, payload_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(raw_path) DO UPDATE SET
               updated_at = excluded.updated_at,
               payload_json = excluded.payload_json",
            params![
                raw_path,
                to_i64(*updated_at),
                serde_json::to_string(payload)?
            ],
        )?;
    }

    transaction.commit()?;
    Ok(())
}

pub fn ensure_skill_session_parser_version(
    paths: &AppPaths,
    version: u64,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = open_connection(paths)?;
    let key = "skill_session_parser_version";
    let current_version = connection
        .query_row(
            "SELECT value FROM usage_metadata WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if current_version.as_deref() == Some(&version.to_string()) {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM skill_usage_session_records", [])?;
    transaction.execute(
        "INSERT INTO usage_metadata(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, version.to_string()],
    )?;
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
    let key = format!("session_parser_version:{app_type}");
    let current_version = connection
        .query_row(
            "SELECT value FROM usage_metadata WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if current_version.as_deref() == Some(&version.to_string()) {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    transaction.execute(
        "DELETE FROM usage_logs
         WHERE raw_path IN (
           SELECT raw_path FROM usage_sessions WHERE app_type = ?1
         )",
        params![app_type],
    )?;
    transaction.execute(
        "DELETE FROM usage_sessions WHERE app_type = ?1",
        params![app_type],
    )?;
    transaction.execute(
        "INSERT INTO usage_metadata(key, value) VALUES (?1, ?2)
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
            "SELECT request_id, payload_json FROM usage_request_records WHERE request_id IN ({placeholders})"
        );
        let mut statement = connection.prepare(&sql)?;
        let parameters = chunk
            .iter()
            .cloned()
            .map(SqlValue::Text)
            .collect::<Vec<_>>();
        let rows = statement.query_map(params_from_iter(parameters.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (request_id, payload) = row?;
            output.insert(request_id, serde_json::from_str(&payload)?);
        }
    }

    Ok(output)
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
        let existing_logs = {
            let mut statement = transaction.prepare(
                "SELECT request_id, payload_json FROM usage_logs WHERE raw_path = ?1",
            )?;
            let rows = statement.query_map(params![update.raw_path], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut logs = HashMap::new();

            for row in rows {
                let (request_id, payload) = row?;
                logs.insert(request_id, serde_json::from_str::<Value>(&payload)?);
            }
            logs
        };
        transaction.execute(
            "DELETE FROM usage_logs WHERE raw_path = ?1",
            params![update.raw_path],
        )?;

        for log in &update.logs {
            let mut next_log = log.clone();
            let request_id = text(log.get("requestId"));

            if let Some(existing) = existing_logs
                .get(&request_id)
                .filter(|item| number(item.get("costLockedAt")) > 0)
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
            "SELECT value FROM usage_metadata WHERE key = 'revision'",
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
        "CREATE TABLE IF NOT EXISTS schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS usage_metadata (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS usage_logs (
           request_id TEXT PRIMARY KEY,
           raw_path TEXT NOT NULL DEFAULT '',
           created_at INTEGER NOT NULL,
           session_updated_at INTEGER NOT NULL DEFAULT 0,
           app_type TEXT NOT NULL DEFAULT '',
           provider_id TEXT NOT NULL DEFAULT '',
           model TEXT NOT NULL DEFAULT '',
           request_source TEXT NOT NULL DEFAULT 'session',
           payload_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_usage_logs_created_at
           ON usage_logs(created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_usage_logs_raw_path
           ON usage_logs(raw_path);
         CREATE INDEX IF NOT EXISTS idx_usage_logs_app_created_at
           ON usage_logs(app_type, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_usage_logs_provider_created_at
           ON usage_logs(provider_id, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_usage_logs_model_created_at
           ON usage_logs(model, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_usage_logs_source_created_at
           ON usage_logs(request_source, created_at DESC);
         CREATE TABLE IF NOT EXISTS usage_sessions (
           raw_path TEXT PRIMARY KEY,
           app_type TEXT NOT NULL DEFAULT '',
           updated_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS usage_request_records (
           request_id TEXT PRIMARY KEY,
           created_at INTEGER NOT NULL DEFAULT 0,
           payload_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_usage_request_records_created_at
           ON usage_request_records(created_at DESC);
         CREATE TABLE IF NOT EXISTS skill_usage_session_records (
           raw_path TEXT PRIMARY KEY,
           updated_at INTEGER NOT NULL DEFAULT 0,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS usage_pricing_config (
           id INTEGER PRIMARY KEY CHECK(id = 1),
           exchange_rate REAL NOT NULL
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
         INSERT OR IGNORE INTO usage_metadata(key, value) VALUES ('revision', '0');",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
        params![SCHEMA_VERSION, now_millis() as i64],
    )?;
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

    let legacy_connection = Connection::open(legacy_path)?;
    let integrity =
        legacy_connection.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;

    if integrity != "ok" {
        return Err(ManagerError::System(format!(
            "旧 Usage 数据库校验失败：{integrity}"
        )));
    }
    legacy_connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(legacy_connection);

    connection.execute(
        "ATTACH DATABASE ?1 AS legacy_usage",
        params![paths.storage_files.usage_database],
    )?;
    let migration_result = (|| {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "INSERT OR REPLACE INTO usage_logs
               SELECT * FROM legacy_usage.usage_logs;
             INSERT OR REPLACE INTO usage_sessions
               SELECT * FROM legacy_usage.usage_sessions;
             INSERT OR REPLACE INTO usage_request_records
               SELECT * FROM legacy_usage.usage_request_records;
             INSERT OR REPLACE INTO usage_pricing_config
               SELECT * FROM legacy_usage.usage_pricing_config;
             INSERT OR REPLACE INTO usage_pricing_items
               SELECT * FROM legacy_usage.usage_pricing_items;",
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok::<(), ManagerError>(())
    })();
    connection.execute_batch("DETACH DATABASE legacy_usage")?;
    migration_result
}

fn insert_usage_log(transaction: &Transaction<'_>, log: &Value) -> Result<(), ManagerError> {
    let request_id = non_empty_text(log.get("requestId"), &text(log.get("id")));

    if request_id.is_empty() {
        return Ok(());
    }

    let mut stored_log = log.clone();
    stored_log["requestId"] = json!(request_id);

    transaction.execute(
        "INSERT INTO usage_logs(
           request_id, raw_path, created_at, session_updated_at,
           app_type, provider_id, model, request_source, payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(request_id) DO UPDATE SET
           raw_path = excluded.raw_path,
           created_at = excluded.created_at,
           session_updated_at = excluded.session_updated_at,
           app_type = excluded.app_type,
           provider_id = excluded.provider_id,
           model = excluded.model,
           request_source = excluded.request_source,
           payload_json = excluded.payload_json",
        params![
            request_id,
            text(stored_log.get("rawPath")),
            number(stored_log.get("createdAt")),
            number(stored_log.get("sessionUpdatedAt")),
            text(stored_log.get("appType")),
            text(stored_log.get("providerId")),
            text(stored_log.get("model")),
            non_empty_text(stored_log.get("requestSource"), "session"),
            serde_json::to_string(&stored_log)?
        ],
    )?;
    Ok(())
}

fn insert_request_record(
    transaction: &Transaction<'_>,
    record: &Value,
) -> Result<(), ManagerError> {
    let request_id = text(record.get("requestId"));

    if request_id.is_empty() {
        return Ok(());
    }

    transaction.execute(
        "INSERT INTO usage_request_records(request_id, created_at, payload_json)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(request_id) DO UPDATE SET
           created_at = excluded.created_at,
           payload_json = excluded.payload_json",
        params![
            request_id,
            number(record.get("createdAt")),
            serde_json::to_string(record)?
        ],
    )?;
    Ok(())
}

fn replace_pricing(transaction: &Transaction<'_>, pricing: &Value) -> Result<(), ManagerError> {
    let exchange_rate = decimal(pricing.get("exchangeRate"), 7.2);

    transaction.execute(
        "INSERT INTO usage_pricing_config(id, exchange_rate) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET exchange_rate = excluded.exchange_rate",
        params![exchange_rate],
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
         SELECT raw_path, MAX(app_type), MAX(session_updated_at)
         FROM usage_logs
         WHERE raw_path <> ''
         GROUP BY raw_path
         ON CONFLICT(raw_path) DO UPDATE SET
           app_type = excluded.app_type,
           updated_at = excluded.updated_at;",
    )?;
    Ok(())
}

fn bump_revision(transaction: &Transaction<'_>) -> Result<(), ManagerError> {
    transaction.execute(
        "UPDATE usage_metadata
         SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
         WHERE key = 'revision'",
        [],
    )?;
    Ok(())
}

fn build_log_where(query: &UsageLogQuery) -> (String, Vec<SqlValue>) {
    let mut clauses: Vec<String> = Vec::new();
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
    let rows = statement.query_map(params_from_iter(parameters.iter()), |row| {
        row.get::<_, String>(0)
    })?;
    let mut logs = Vec::new();

    for row in rows {
        logs.push(serde_json::from_str(&row?)?);
    }
    Ok(logs)
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

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        create_schema, ensure_skill_session_parser_version, initialize, query_logs, read_pricing,
        read_codex_quota_stages, read_request_records, read_session_versions,
        read_skill_session_records, record_codex_quota_stages, replace_sessions, write_pricing,
        write_skill_session_records, UsageLogQuery, UsageSessionUpdate,
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
    }

    #[test]
    fn migrates_legacy_usage_database_into_main_database() {
        let paths = create_test_paths("database-migration");
        let legacy = Connection::open(&paths.storage_files.usage_database).unwrap();
        create_schema(&legacy).unwrap();
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
    fn persists_skill_session_records_by_file_version() {
        let paths = create_test_paths("skill-session-records");

        write_skill_session_records(
            &paths,
            &[(
                "session.jsonl".to_string(),
                123,
                vec![json!({"display": "$imagegen", "timestamp": 100})],
            )],
        )
        .unwrap();

        let records = read_skill_session_records(&paths).unwrap();
        assert_eq!(records["session.jsonl"].0, 123);
        assert_eq!(records["session.jsonl"].1.len(), 1);
        assert_eq!(records["session.jsonl"].1[0]["display"], "$imagegen");
    }

    #[test]
    fn invalidates_skill_records_when_parser_version_changes() {
        let paths = create_test_paths("skill-session-parser-version");
        let records = [(
            "session.jsonl".to_string(),
            123,
            vec![json!({"display": "$imagegen", "timestamp": 100})],
        )];

        write_skill_session_records(&paths, &records).unwrap();
        ensure_skill_session_parser_version(&paths, 1).unwrap();
        assert!(read_skill_session_records(&paths).unwrap().is_empty());

        write_skill_session_records(&paths, &records).unwrap();
        ensure_skill_session_parser_version(&paths, 1).unwrap();
        assert_eq!(read_skill_session_records(&paths).unwrap().len(), 1);

        ensure_skill_session_parser_version(&paths, 2).unwrap();
        assert!(read_skill_session_records(&paths).unwrap().is_empty());
    }
}

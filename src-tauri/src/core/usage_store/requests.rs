use super::*;

// 同一请求只保存一份。0：提前捕获的归属；1：参与统计；2：重扫后暂不参与统计。
// 状态 2 保留历史归属和锁价，重新解析时不会按当前供应商或价格覆盖历史。
pub(super) const FIELDS: &[(&str, &str)] = &[
    ("requestId", "request_id"),
    ("appType", "app_type"),
    ("sessionId", "session_id"),
    ("sessionTitle", "session_title"),
    ("sessionCreatedAt", "session_created_at"),
    ("sessionUpdatedAt", "session_updated_at"),
    ("rawPath", "raw_path"),
    ("projectName", "project_name"),
    ("createdAt", "created_at"),
    ("requestTime", "request_time"),
    ("providerId", "provider_id"),
    ("providerName", "provider_name"),
    ("providerType", "provider_type"),
    ("instanceProviderId", "instance_provider_id"),
    ("instanceProviderName", "instance_provider_name"),
    ("instanceProviderType", "instance_provider_type"),
    ("apiKeyId", "api_key_id"),
    ("apiKeyHash", "api_key_hash"),
    ("apiKeyAttribution", "api_key_attribution"),
    ("model", "model"),
    ("requestModel", "request_model"),
    ("requestSource", "request_source"),
    ("dataSource", "data_source"),
    ("inputTokens", "input_tokens"),
    ("outputTokens", "output_tokens"),
    ("cacheReadTokens", "cache_read_tokens"),
    ("cacheCreationTokens", "cache_creation_tokens"),
    ("actualTokens", "actual_tokens"),
    ("inputCostUsd", "input_cost_usd"),
    ("outputCostUsd", "output_cost_usd"),
    ("cacheReadCostUsd", "cache_read_cost_usd"),
    ("cacheCreationCostUsd", "cache_creation_cost_usd"),
    ("totalCostUsd", "total_cost_usd"),
    ("costLockedAt", "cost_locked_at"),
    ("statusCode", "status_code"),
    ("errorMessage", "error_message"),
    ("latencyMs", "latency_ms"),
    ("pricingSnapshot.pricingId", "pricing_id"),
    ("pricingSnapshot.modelId", "pricing_model_id"),
    ("pricingSnapshot.currency", "pricing_currency"),
    ("pricingSnapshot.exchangeRate", "pricing_exchange_rate"),
    (
        "pricingSnapshot.inputCostPerMillion",
        "pricing_input_per_million",
    ),
    (
        "pricingSnapshot.outputCostPerMillion",
        "pricing_output_per_million",
    ),
    (
        "pricingSnapshot.cacheReadCostPerMillion",
        "pricing_cache_read_per_million",
    ),
    (
        "pricingSnapshot.cacheCreationCostPerMillion",
        "pricing_cache_creation_per_million",
    ),
    ("pricingSnapshot.lockedAt", "pricing_locked_at"),
];

pub(super) fn create_schema(connection: &Connection) -> Result<(), ManagerError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS usage_requests (
           request_id TEXT PRIMARY KEY,
           record_state INTEGER NOT NULL DEFAULT 0 CHECK(record_state IN (0,1,2)),
           app_type TEXT,
           session_id TEXT,
           session_title TEXT,
           session_created_at INTEGER,
           session_updated_at INTEGER,
           raw_path TEXT,
           project_name TEXT,
           created_at INTEGER,
           request_time INTEGER,
           provider_id TEXT,
           provider_name TEXT,
           provider_type TEXT,
           instance_provider_id TEXT,
           instance_provider_name TEXT,
           instance_provider_type TEXT,
           api_key_id TEXT,
           api_key_hash TEXT,
           api_key_attribution TEXT,
           model TEXT,
           request_model TEXT,
           request_source TEXT,
           data_source TEXT,
           input_tokens INTEGER,
           output_tokens INTEGER,
           cache_read_tokens INTEGER,
           cache_creation_tokens INTEGER,
           actual_tokens INTEGER,
           input_cost_usd REAL,
           output_cost_usd REAL,
           cache_read_cost_usd REAL,
           cache_creation_cost_usd REAL,
           total_cost_usd REAL,
           cost_locked_at INTEGER,
           status_code INTEGER,
           error_message TEXT,
           latency_ms INTEGER,
           pricing_id TEXT,
           pricing_model_id TEXT,
           pricing_currency TEXT,
           pricing_exchange_rate REAL,
           pricing_input_per_million REAL,
           pricing_output_per_million REAL,
           pricing_cache_read_per_million REAL,
           pricing_cache_creation_per_million REAL,
           pricing_locked_at INTEGER
         );
         CREATE INDEX IF NOT EXISTS idx_usage_requests_raw_path ON usage_requests(raw_path);
         CREATE INDEX IF NOT EXISTS idx_usage_requests_created_at ON usage_requests(created_at DESC) WHERE record_state=1;
         CREATE INDEX IF NOT EXISTS idx_usage_requests_app_time ON usage_requests(app_type, created_at DESC) WHERE record_state=1;
         CREATE INDEX IF NOT EXISTS idx_usage_requests_provider_time ON usage_requests(provider_id, created_at DESC) WHERE record_state=1;
         CREATE INDEX IF NOT EXISTS idx_usage_requests_model_time ON usage_requests(model, created_at DESC) WHERE record_state=1;
         CREATE INDEX IF NOT EXISTS idx_usage_requests_source_time ON usage_requests(request_source, created_at DESC) WHERE record_state=1;",
    )?;
    Ok(())
}

pub(super) fn columns() -> String {
    FIELDS
        .iter()
        .map(|(_, column)| *column)
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut result = serde_json::Map::new();
    let mut pricing = serde_json::Map::new();
    for (index, (field, _)) in FIELDS.iter().enumerate() {
        let value = match row.get::<_, SqlValue>(index)? {
            SqlValue::Null => continue,
            SqlValue::Integer(value) => json!(value),
            SqlValue::Real(value) => json!(value),
            SqlValue::Text(value) => json!(value),
            SqlValue::Blob(_) => return Err(rusqlite::Error::InvalidQuery),
        };
        if let Some(field) = field.strip_prefix("pricingSnapshot.") {
            pricing.insert(field.to_string(), value);
        } else {
            result.insert(field.to_string(), value);
        }
    }
    if !pricing.is_empty() {
        result.insert("pricingSnapshot".to_string(), Value::Object(pricing));
    }
    Ok(Value::Object(result))
}

pub(super) fn upsert(
    connection: &Connection,
    value: &Value,
    is_log: bool,
) -> Result<(), ManagerError> {
    let request_id = non_empty_text(value.get("requestId"), &text(value.get("id")));
    if request_id.is_empty() {
        return Ok(());
    }
    let mut values = Vec::with_capacity(FIELDS.len() + 1);
    for (field, _) in FIELDS {
        let item = if let Some(field) = field.strip_prefix("pricingSnapshot.") {
            value
                .get("pricingSnapshot")
                .and_then(|snapshot| snapshot.get(field))
        } else {
            value.get(*field)
        };
        values.push(if *field == "requestId" {
            SqlValue::Text(request_id.clone())
        } else {
            match item {
                Some(Value::String(value)) => SqlValue::Text(value.clone()),
                Some(Value::Number(value)) if value.is_i64() => {
                    SqlValue::Integer(value.as_i64().unwrap())
                }
                Some(Value::Number(value)) if value.is_u64() => {
                    SqlValue::Integer(to_i64(value.as_u64().unwrap()))
                }
                Some(Value::Number(value)) => SqlValue::Real(value.as_f64().unwrap_or(0.0)),
                _ => SqlValue::Null,
            }
        });
    }
    values.push(SqlValue::Integer(if is_log { 1 } else { 0 }));
    static LOG_SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| upsert_sql(true));
    static RECORD_SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| upsert_sql(false));
    let sql = if is_log { &*LOG_SQL } else { &*RECORD_SQL };
    connection
        .prepare_cached(sql)?
        .execute(params_from_iter(values.iter()))?;
    Ok(())
}

pub(super) fn import_legacy_log(
    connection: &Connection,
    value: &Value,
) -> Result<(), ManagerError> {
    let mut value = value.clone();
    // 旧日志可能没有归属字段，不能用缺省空值覆盖提前捕获的供应商和密钥。
    let request_id = non_empty_text(value.get("requestId"), &text(value.get("id")));
    let previous = connection
        .query_row(
            &format!(
                "SELECT {} FROM usage_requests WHERE request_id=?1",
                columns()
            ),
            [request_id],
            from_row,
        )
        .optional()?;
    if let Some(previous) = previous {
        for (field, item) in previous.as_object().into_iter().flatten() {
            if value
                .get(field)
                .is_none_or(|item| item.is_null() || item.as_str() == Some(""))
            {
                value[field] = item.clone();
            }
        }
    }
    upsert(connection, &value, true)
}

fn upsert_sql(is_log: bool) -> String {
    let updates = FIELDS
        .iter()
        .skip(1)
        .map(|(_, column)| {
            if *column == "request_time" && is_log {
                format!("{column}=COALESCE(excluded.{column},usage_requests.{column})")
            } else {
                format!("{column}=excluded.{column}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "INSERT INTO usage_requests({},record_state) VALUES ({})
         ON CONFLICT(request_id) DO UPDATE SET {updates} {state_update} {condition}",
        columns(),
        vec!["?"; FIELDS.len() + 1].join(","),
        state_update = if is_log { ",record_state=1" } else { "" },
        // 提前捕获或由最终日志派生的快照不能覆盖已解析的用量及锁价。
        condition = if is_log {
            ""
        } else {
            "WHERE usage_requests.record_state=0"
        },
    )
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, ManagerError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

// 逐行迁移，避免把两张旧表的 JSON 同时装入内存；全部成功才删除旧表。
pub(super) fn migrate(connection: &mut Connection) -> Result<(), ManagerError> {
    let has_legacy: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name IN
         ('usage_request_records','usage_logs','usage_metadata','usage_pricing_config'))",
        [],
        |row| row.get(0),
    )?;
    if !has_legacy {
        return Ok(());
    }
    // 先取得写锁再检查旧表，其他线程等待后会看到迁移完成的结构。
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let old_records = table_exists(&transaction, "usage_request_records")?;
    let old_logs = table_exists(&transaction, "usage_logs")?;
    let old_metadata = table_exists(&transaction, "usage_metadata")?;
    let old_pricing = table_exists(&transaction, "usage_pricing_config")?;
    for (table, exists, is_log) in [
        ("usage_request_records", old_records, false),
        ("usage_logs", old_logs, true),
    ] {
        if !exists {
            continue;
        }
        let mut statement = transaction.prepare(&format!("SELECT * FROM {table}"))?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let mut value: Value = serde_json::from_str(&row.get::<_, String>("payload_json")?)?;
            if !value.is_object() {
                return Err(ManagerError::System(format!(
                    "旧用量表 {table} 包含无效请求记录"
                )));
            }
            // 较早版本的索引字段可能没有同时写入 JSON。
            for (field, column) in FIELDS.iter().filter(|(_, column)| {
                [
                    "request_id",
                    "created_at",
                    "raw_path",
                    "session_updated_at",
                    "app_type",
                    "provider_id",
                    "model",
                    "request_source",
                ]
                .contains(column)
            }) {
                if value.get(*field).is_none() {
                    if let Ok(item) = row.get::<_, SqlValue>(*column) {
                        match item {
                            SqlValue::Text(item) => value[*field] = json!(item),
                            SqlValue::Integer(item) => value[*field] = json!(item),
                            _ => {}
                        }
                    }
                }
            }
            if is_log {
                import_legacy_log(&transaction, &value)?;
            } else {
                upsert(&transaction, &value, false)?;
            }
        }
        drop(rows);
        drop(statement);
        let missing: i64 =
            transaction.query_row(
                &format!(
            "SELECT COUNT(*) FROM {table} old LEFT JOIN usage_requests new USING(request_id)
             WHERE new.request_id IS NULL {}", if is_log { "OR new.record_state<>1" } else { "" }),
                [],
                |row| row.get(0),
            )?;
        if missing > 0 {
            return Err(ManagerError::System(format!(
                "旧用量表 {table} 有 {missing} 条请求未迁移，保留旧表"
            )));
        }
    }
    if old_metadata {
        transaction.execute_batch(
            "INSERT INTO app_metadata(key,value) SELECT 'usage:' || key,value FROM usage_metadata WHERE 1
             ON CONFLICT(key) DO NOTHING; DROP TABLE usage_metadata;",
        )?;
    }
    if old_pricing {
        transaction.execute_batch(
            "INSERT INTO app_metadata(key,value) SELECT 'usage:exchange_rate',CAST(exchange_rate AS TEXT)
             FROM usage_pricing_config WHERE id=1 ON CONFLICT(key) DO NOTHING;
             DROP TABLE usage_pricing_config;",
        )?;
    }
    if old_logs {
        transaction.execute_batch("DROP TABLE usage_logs;")?;
    }
    if old_records {
        transaction.execute_batch("DROP TABLE usage_request_records;")?;
    }
    transaction.commit()?;
    Ok(())
}

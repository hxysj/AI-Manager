use crate::core::{database, error::ManagerError, paths::AppPaths};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn open(paths: &AppPaths) -> Result<Connection, ManagerError> {
    let connection = database::open(paths)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS translation_history (
           id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS translation_requests (
           id TEXT PRIMARY KEY, translation_id TEXT NOT NULL,
           created_at INTEGER NOT NULL, payload_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_translation_history_created ON translation_history(created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_translation_requests_history ON translation_requests(translation_id, created_at DESC);",
    )?;
    Ok(connection)
}

// 进程退出后的未完成记录保留为中断，不伪装成成功或零消耗。
pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    let connection = open(paths)?;
    connection.execute(
        "UPDATE translation_history SET payload_json = json_set(payload_json,
         '$.status', 'interrupted', '$.errorMessage', '应用退出，翻译未完成')
         WHERE json_extract(payload_json, '$.status') = 'running'",
        [],
    )?;
    connection.execute(
        "UPDATE translation_requests SET payload_json = json_set(payload_json,
         '$.status', 'interrupted', '$.errorMessage', '应用退出，请求未完成')
         WHERE json_extract(payload_json, '$.status') = 'running'",
        [],
    )?;
    Ok(())
}

pub fn save_history(paths: &AppPaths, item: &Value) -> Result<(), ManagerError> {
    open(paths)?.execute(
        "INSERT INTO translation_history(id, created_at, payload_json) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET payload_json = excluded.payload_json",
        params![item["id"].as_str(), item["createdAt"].as_i64(), item.to_string()],
    )?;
    Ok(())
}

pub fn save_request(paths: &AppPaths, item: &Value) -> Result<(), ManagerError> {
    open(paths)?.execute(
        "INSERT INTO translation_requests(id, translation_id, created_at, payload_json) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET payload_json = excluded.payload_json",
        params![item["id"].as_str(), item["translationId"].as_str(), item["createdAt"].as_i64(), item.to_string()],
    )?;
    Ok(())
}

pub fn list(paths: &AppPaths, payload: &Value) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let kind = payload["kind"].as_str().unwrap_or("history");
    let table = if kind == "usage" { "translation_requests" } else { "translation_history" };
    let page = payload["page"].as_u64().unwrap_or(1).clamp(1, 1_000_000);
    let total: u64 = connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))?;
    let mut statement = connection.prepare(&format!(
        "SELECT payload_json FROM {table} ORDER BY created_at DESC, id DESC LIMIT 20 OFFSET ?1"
    ))?;
    let rows = statement.query_map(params![(page - 1) * 20], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let items = rows.iter().map(|row| serde_json::from_str::<Value>(row)).collect::<Result<Vec<_>, _>>()?;
    let summary = connection.query_row(
        "SELECT COUNT(*),
         COALESCE(SUM(json_extract(payload_json, '$.inputTokens')), 0),
         COALESCE(SUM(json_extract(payload_json, '$.outputTokens')), 0),
         COALESCE(SUM(json_extract(payload_json, '$.cacheReadTokens')), 0),
         COALESCE(SUM(json_extract(payload_json, '$.reasoningTokens')), 0),
         COALESCE(SUM(CASE WHEN json_extract(payload_json, '$.usageKnown') = 1 THEN 0 ELSE 1 END), 0)
         FROM translation_requests",
        [],
        |row| Ok(json!({
            "requestCount": row.get::<_, u64>(0)?, "inputTokens": row.get::<_, u64>(1)?,
            "outputTokens": row.get::<_, u64>(2)?, "cacheReadTokens": row.get::<_, u64>(3)?,
            "reasoningTokens": row.get::<_, u64>(4)?, "unknownUsageCount": row.get::<_, u64>(5)?
        })),
    )?;
    Ok(json!({ "items": items, "total": total, "page": page, "summary": summary }))
}

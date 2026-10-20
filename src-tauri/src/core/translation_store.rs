use crate::core::{database, error::ManagerError, paths::AppPaths};
use rusqlite::{params, Connection, TransactionBehavior};
use serde_json::{json, Value};

const COLUMNS: &str = "id, record_type, translation_id, created_at, target_id, provider_name,
    model, engine, status, error_message, duration_ms, source_text, translated_text,
    target_language, request_count, status_code, api_key_id, api_key_hash, usage_known,
    input_tokens, output_tokens, cache_read_tokens, reasoning_tokens";

fn open(paths: &AppPaths) -> Result<Connection, ManagerError> {
    let mut connection = database::open(paths)?;
    // 正常读写只检查结构，不反复执行 DDL，也不依赖额外的版本表。
    let (ready, legacy): (bool, bool) = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='translation_records'),
                EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table'
                       AND name IN ('translation_history', 'translation_requests'))",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if !ready || legacy {
        migrate(&mut connection)?;
    }
    Ok(connection)
}

fn migrate(connection: &mut Connection) -> Result<(), ManagerError> {
    // 先取得写锁，再检查旧表；并发首次使用时不会重复迁移。
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS translation_records (
           id TEXT PRIMARY KEY NOT NULL,
           record_type TEXT NOT NULL CHECK(record_type IN ('history', 'request')),
           translation_id TEXT,
           created_at INTEGER NOT NULL,
           target_id TEXT NOT NULL,
           provider_name TEXT NOT NULL,
           model TEXT NOT NULL,
           engine TEXT NOT NULL,
           status TEXT NOT NULL,
           error_message TEXT NOT NULL,
           duration_ms INTEGER,
           source_text TEXT,
           translated_text TEXT,
           target_language TEXT,
           request_count INTEGER,
           status_code INTEGER,
           api_key_id TEXT,
           api_key_hash TEXT,
           usage_known INTEGER CHECK(usage_known IN (0, 1)),
           input_tokens INTEGER,
           output_tokens INTEGER,
           cache_read_tokens INTEGER,
           reasoning_tokens INTEGER,
           CHECK((record_type='history' AND translation_id IS NULL)
              OR (record_type='request' AND length(translation_id)>0 AND translation_id IS NOT NULL))
         );
         CREATE INDEX IF NOT EXISTS idx_translation_records_created
           ON translation_records(record_type, created_at DESC, id DESC);",
    )?;
    for (table, kind) in [
        ("translation_history", "history"),
        ("translation_requests", "request"),
    ] {
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )?;
        if !exists {
            continue;
        }
        let parent = if kind == "request" {
            "translation_id"
        } else {
            "NULL"
        };
        let mut statement = transaction.prepare(&format!(
            "SELECT id, created_at, payload_json, {parent} FROM {table}"
        ))?;
        let mut rows = statement.query([])?;
        // 逐条迁移，避免一次把全部原文和译文加载进内存。
        while let Some(row) = rows.next()? {
            let mut item: Value = serde_json::from_str(&row.get::<_, String>(2)?)?;
            if !item.is_object() {
                return Err(ManagerError::System(format!(
                    "{table} 的翻译记录格式无效，原表已保留"
                )));
            }
            item["id"] = json!(row.get::<_, String>(0)?);
            item["createdAt"] = json!(row.get::<_, i64>(1)?);
            if kind == "request" {
                item["translationId"] = json!(row.get::<_, String>(3)?);
            }
            write_record(&transaction, kind, &item, false)?;
        }
    }
    // 复制、约束检查和删表在同一事务中，任一步失败都会完整回滚。
    transaction.execute_batch(
        "DROP TABLE IF EXISTS translation_requests;
         DROP TABLE IF EXISTS translation_history;",
    )?;
    transaction.commit()?;
    Ok(())
}

// 进程退出后的未完成记录保留为中断，不伪装成成功或零消耗。
pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    open(paths)?.execute(
        "UPDATE translation_records SET status='interrupted', error_message=
         CASE record_type WHEN 'history' THEN '应用退出，翻译未完成' ELSE '应用退出，请求未完成' END
         WHERE status='running'",
        [],
    )?;
    Ok(())
}

pub fn save_history(paths: &AppPaths, item: &Value) -> Result<(), ManagerError> {
    write_record(&open(paths)?, "history", item, true)
}

pub fn save_request(paths: &AppPaths, item: &Value) -> Result<(), ManagerError> {
    write_record(&open(paths)?, "request", item, true)
}

fn write_record(
    connection: &Connection,
    kind: &str,
    item: &Value,
    update: bool,
) -> Result<(), ManagerError> {
    let id = item["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ManagerError::System("翻译记录缺少 ID".into()))?;
    let created_at = item["createdAt"]
        .as_i64()
        .ok_or_else(|| ManagerError::System("翻译记录缺少创建时间".into()))?;
    let history = kind == "history";
    let translation_id = if history {
        None
    } else {
        Some(
            item["translationId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| ManagerError::System("请求记录缺少翻译 ID".into()))?,
        )
    };
    let conflict = if update {
        "ON CONFLICT(id) DO UPDATE SET
           target_id=excluded.target_id, provider_name=excluded.provider_name,
           model=excluded.model, engine=excluded.engine, status=excluded.status,
           error_message=excluded.error_message, duration_ms=excluded.duration_ms,
           source_text=excluded.source_text, translated_text=excluded.translated_text,
           target_language=excluded.target_language, request_count=excluded.request_count,
           status_code=excluded.status_code, api_key_id=excluded.api_key_id,
           api_key_hash=excluded.api_key_hash, usage_known=excluded.usage_known,
           input_tokens=excluded.input_tokens, output_tokens=excluded.output_tokens,
           cache_read_tokens=excluded.cache_read_tokens, reasoning_tokens=excluded.reasoning_tokens
         WHERE translation_records.record_type=excluded.record_type
           AND translation_records.translation_id IS excluded.translation_id"
    } else {
        ""
    };
    let changed = connection
        .prepare_cached(&format!(
            "INSERT INTO translation_records({COLUMNS}) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
           ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23) {conflict}"
        ))?
        .execute(params![
            id,
            kind,
            translation_id,
            created_at,
            item["targetId"].as_str().unwrap_or(""),
            item["providerName"].as_str().unwrap_or(""),
            item["model"].as_str().unwrap_or(""),
            item["engine"].as_str().unwrap_or(""),
            item["status"].as_str().unwrap_or(""),
            item["errorMessage"].as_str().unwrap_or(""),
            item["durationMs"].as_i64(),
            if history {
                item["sourceText"].as_str()
            } else {
                None
            },
            if history {
                item["translatedText"].as_str()
            } else {
                None
            },
            if history {
                item["targetLanguage"].as_str()
            } else {
                None
            },
            if history {
                item["requestCount"].as_i64()
            } else {
                None
            },
            if history {
                None
            } else {
                item["statusCode"].as_i64()
            },
            if history {
                None
            } else {
                item["apiKeyId"].as_str()
            },
            if history {
                None
            } else {
                item["apiKeyHash"].as_str()
            },
            item["usageKnown"].as_bool(),
            item["inputTokens"].as_i64(),
            item["outputTokens"].as_i64(),
            item["cacheReadTokens"].as_i64(),
            item["reasoningTokens"].as_i64(),
        ])?;
    if changed != 1 {
        return Err(ManagerError::System(
            "翻译记录 ID 已属于其他记录，未覆盖原数据".into(),
        ));
    }
    Ok(())
}

fn record_value(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut item = json!({
        "id": row.get::<_, String>("id")?,
        "createdAt": row.get::<_, i64>("created_at")?,
        "targetId": row.get::<_, String>("target_id")?,
        "providerName": row.get::<_, String>("provider_name")?,
        "model": row.get::<_, String>("model")?,
        "engine": row.get::<_, String>("engine")?,
        "status": row.get::<_, String>("status")?,
        "errorMessage": row.get::<_, String>("error_message")?,
    });
    for (key, column) in [
        ("translationId", "translation_id"),
        ("sourceText", "source_text"),
        ("translatedText", "translated_text"),
        ("targetLanguage", "target_language"),
        ("apiKeyId", "api_key_id"),
        ("apiKeyHash", "api_key_hash"),
    ] {
        if let Some(value) = row.get::<_, Option<String>>(column)? {
            item[key] = json!(value);
        }
    }
    for (key, column) in [
        ("durationMs", "duration_ms"),
        ("requestCount", "request_count"),
        ("statusCode", "status_code"),
        ("inputTokens", "input_tokens"),
        ("outputTokens", "output_tokens"),
        ("cacheReadTokens", "cache_read_tokens"),
        ("reasoningTokens", "reasoning_tokens"),
    ] {
        if let Some(value) = row.get::<_, Option<i64>>(column)? {
            item[key] = json!(value);
        }
    }
    if let Some(value) = row.get::<_, Option<bool>>("usage_known")? {
        item["usageKnown"] = json!(value);
    }
    Ok(item)
}

pub fn list(paths: &AppPaths, payload: &Value) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let kind = if payload["kind"] == "usage" {
        "request"
    } else {
        "history"
    };
    let page = payload["page"].as_u64().unwrap_or(1).clamp(1, 1_000_000);
    let total: u64 = connection.query_row(
        "SELECT COUNT(*) FROM translation_records WHERE record_type=?1",
        [kind],
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM translation_records WHERE record_type=?1
         ORDER BY created_at DESC, id DESC LIMIT 20 OFFSET ?2"
    ))?;
    let items = statement
        .query_map(params![kind, (page - 1) * 20], record_value)?
        .collect::<Result<Vec<_>, _>>()?;
    // 只汇总实际请求，避免将历史记录中的合计再次计入；未知用量保留为未知。
    let summary = connection.query_row(
        "SELECT COUNT(*),
         COALESCE(SUM(CASE WHEN usage_known=1 THEN input_tokens ELSE 0 END), 0),
         COALESCE(SUM(CASE WHEN usage_known=1 THEN output_tokens ELSE 0 END), 0),
         COALESCE(SUM(CASE WHEN usage_known=1 THEN cache_read_tokens ELSE 0 END), 0),
         COALESCE(SUM(CASE WHEN usage_known=1 THEN reasoning_tokens ELSE 0 END), 0),
         COALESCE(SUM(CASE WHEN usage_known=1 THEN 0 ELSE 1 END), 0)
         FROM translation_records WHERE record_type='request'",
        [],
        |row| {
            Ok(json!({
                "requestCount": row.get::<_, u64>(0)?, "inputTokens": row.get::<_, u64>(1)?,
                "outputTokens": row.get::<_, u64>(2)?, "cacheReadTokens": row.get::<_, u64>(3)?,
                "reasoningTokens": row.get::<_, u64>(4)?, "unknownUsageCount": row.get::<_, u64>(5)?
            }))
        },
    )?;
    Ok(json!({ "items": items, "total": total, "page": page, "summary": summary }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: std::path::PathBuf,
        paths: AppPaths,
    }

    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("translation-store-{}", uuid::Uuid::new_v4()));
            Self {
                paths: crate::core::paths::resolve_app_paths(&root),
                root,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }

    fn history() -> Value {
        json!({
            "id": uuid::Uuid::new_v4().to_string(), "createdAt": 1234,
            "targetId": "provider", "providerName": "翻译供应商", "model": "model", "engine": "langchain",
            "sourceText": "Hello", "translatedText": "你好", "targetLanguage": "简体中文",
            "status": "success", "errorMessage": "", "durationMs": 100,
            "requestCount": 2, "usageKnown": false, "inputTokens": 12,
            "outputTokens": 6, "cacheReadTokens": 3, "reasoningTokens": 2
        })
    }

    fn request(parent: &Value, known: bool) -> Value {
        json!({
            "id": uuid::Uuid::new_v4().to_string(), "translationId": parent["id"], "createdAt": 1235,
            "targetId": "provider", "providerName": "翻译供应商", "model": "model", "engine": "langchain",
            "status": if known { "success" } else { "failed" }, "errorMessage": if known { "" } else { "网络中断" },
            "durationMs": 80, "statusCode": if known { 200 } else { 0 },
            "apiKeyId": "key", "apiKeyHash": "hash", "usageKnown": known,
            "inputTokens": 12, "outputTokens": 6, "cacheReadTokens": 3, "reasoningTokens": 2
        })
    }

    fn legacy_schema(connection: &Connection) {
        connection.execute_batch(
            "CREATE TABLE translation_history(id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, payload_json TEXT NOT NULL);
             CREATE TABLE translation_requests(id TEXT PRIMARY KEY, translation_id TEXT NOT NULL, created_at INTEGER NOT NULL, payload_json TEXT NOT NULL);
             CREATE INDEX idx_translation_history_created ON translation_history(created_at DESC);
             CREATE INDEX idx_translation_requests_history ON translation_requests(translation_id, created_at DESC);",
        ).unwrap();
    }

    fn insert_legacy(connection: &Connection, history: &Value, requests: &[Value]) {
        connection
            .execute(
                "INSERT INTO translation_history VALUES (?1, ?2, ?3)",
                params![
                    history["id"].as_str(),
                    history["createdAt"].as_i64(),
                    history.to_string()
                ],
            )
            .unwrap();
        for item in requests {
            connection
                .execute(
                    "INSERT INTO translation_requests VALUES (?1, ?2, ?3, ?4)",
                    params![
                        item["id"].as_str(),
                        item["translationId"].as_str(),
                        item["createdAt"].as_i64(),
                        item.to_string()
                    ],
                )
                .unwrap();
        }
    }

    #[test]
    fn merges_legacy_records_and_removes_old_tables_and_json() {
        let fixture = Fixture::new();
        let parent = history();
        let requests = [request(&parent, true), request(&parent, false)];
        {
            let connection = database::open(&fixture.paths).unwrap();
            legacy_schema(&connection);
            insert_legacy(&connection, &parent, &requests);
        }
        initialize(&fixture.paths).unwrap();
        assert_eq!(
            list(&fixture.paths, &json!({})).unwrap()["items"],
            json!([parent])
        );
        let usage = list(&fixture.paths, &json!({"kind":"usage"})).unwrap();
        assert_eq!(usage["total"], 2);
        for item in &requests {
            assert!(usage["items"].as_array().unwrap().contains(item));
        }
        assert_eq!(
            usage["summary"],
            json!({"requestCount":2,"inputTokens":12,"outputTokens":6,
            "cacheReadTokens":3,"reasoningTokens":2,"unknownUsageCount":1})
        );
        for _ in 0..2 {
            initialize(&fixture.paths).unwrap();
        }
        let connection = database::open(&fixture.paths).unwrap();
        let tables: Vec<String> = connection
            .prepare("SELECT name FROM sqlite_schema WHERE type='table'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(tables, vec!["translation_records"]);
        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(translation_records)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!columns.iter().any(|column| column.contains("json")));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM translation_records", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }

    #[test]
    fn invalid_legacy_data_rolls_back_all_copies_and_drops() {
        for duplicate_id in [false, true] {
            let fixture = Fixture::new();
            let connection = database::open(&fixture.paths).unwrap();
            legacy_schema(&connection);
            let parent = history();
            let mut child = request(&parent, true);
            if duplicate_id {
                child["id"] = parent["id"].clone();
            }
            insert_legacy(&connection, &parent, &[child]);
            if !duplicate_id {
                connection
                    .execute("UPDATE translation_requests SET payload_json='broken'", [])
                    .unwrap();
            }
            assert!(initialize(&fixture.paths).is_err());
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM translation_history", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM translation_requests", [], |row| row
                        .get::<_, i64>(
                        0
                    ))
                    .unwrap(),
                1
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_schema WHERE name='translation_records'",
                        [],
                        |row| row.get::<_, i64>(0)
                    )
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    fn updates_requests_without_double_counting_and_interrupts_running_records() {
        let fixture = Fixture::new();
        let mut parent = history();
        parent["status"] = json!("running");
        save_history(&fixture.paths, &parent).unwrap();
        let mut child = request(&parent, false);
        child["status"] = json!("running");
        save_request(&fixture.paths, &child).unwrap();
        // 列表查询不应把仍在进行中的翻译标记为中断。
        assert_eq!(
            list(&fixture.paths, &json!({})).unwrap()["items"][0]["status"],
            "running"
        );
        initialize(&fixture.paths).unwrap();
        assert_eq!(
            list(&fixture.paths, &json!({})).unwrap()["items"][0]["status"],
            "interrupted"
        );
        let interrupted = list(&fixture.paths, &json!({"kind":"usage"})).unwrap();
        assert_eq!(interrupted["items"][0]["status"], "interrupted");
        assert_eq!(interrupted["summary"]["unknownUsageCount"], 1);
        child["status"] = json!("success");
        child["errorMessage"] = json!("");
        child["usageKnown"] = json!(true);
        save_request(&fixture.paths, &child).unwrap();
        save_request(&fixture.paths, &child).unwrap();
        let usage = list(&fixture.paths, &json!({"kind":"usage"})).unwrap();
        assert_eq!(usage["items"], json!([child]));
        assert_eq!(usage["summary"]["requestCount"], 1);
        assert_eq!(usage["summary"]["inputTokens"], 12);
        let mut collision = request(&parent, true);
        collision["id"] = parent["id"].clone();
        assert!(save_request(&fixture.paths, &collision).is_err());
        child["translationId"] = json!(uuid::Uuid::new_v4().to_string());
        assert!(save_request(&fixture.paths, &child).is_err());
    }

    #[test]
    fn paginates_each_record_type_and_supports_translations_without_requests() {
        let fixture = Fixture::new();
        let mut parent = history();
        for key in [
            "requestCount",
            "durationMs",
            "usageKnown",
            "inputTokens",
            "outputTokens",
            "cacheReadTokens",
            "reasoningTokens",
        ] {
            parent.as_object_mut().unwrap().remove(key);
        }
        save_history(&fixture.paths, &parent).unwrap();
        assert_eq!(
            list(&fixture.paths, &json!({})).unwrap()["items"],
            json!([parent])
        );
        for i in 0..23 {
            let mut child = request(&parent, true);
            child["createdAt"] = json!(2000 + i);
            save_request(&fixture.paths, &child).unwrap();
        }
        let first = list(&fixture.paths, &json!({"kind":"usage"})).unwrap();
        let second = list(&fixture.paths, &json!({"kind":"usage","page":2})).unwrap();
        assert_eq!(first["total"], 23);
        assert_eq!(first["items"].as_array().unwrap().len(), 20);
        assert_eq!(first["items"][0]["createdAt"], 2022);
        assert_eq!(second["items"].as_array().unwrap().len(), 3);
        assert_eq!(second["items"][0]["createdAt"], 2002);
        assert_eq!(second["summary"]["requestCount"], 23);
        assert_eq!(list(&fixture.paths, &json!({})).unwrap()["total"], 1);
    }
}

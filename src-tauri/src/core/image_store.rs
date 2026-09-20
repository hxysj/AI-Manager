use crate::core::{database, error::ManagerError, paths::AppPaths};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub struct StoredImage {
    pub bytes: Vec<u8>,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub revised_prompt: String,
    pub thumbnail: String,
}

fn open(paths: &AppPaths) -> Result<Connection, ManagerError> {
    let connection = database::open(paths)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS image_tasks (
           id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, status TEXT NOT NULL,
           payload_json TEXT NOT NULL, thumbnail TEXT NOT NULL DEFAULT ''
         );
         CREATE TABLE IF NOT EXISTS image_outputs (
           task_id TEXT NOT NULL, position INTEGER NOT NULL, format TEXT NOT NULL,
           bytes BLOB NOT NULL, PRIMARY KEY(task_id, position)
         );
         CREATE INDEX IF NOT EXISTS idx_image_tasks_created ON image_tasks(created_at DESC);
         CREATE TABLE IF NOT EXISTS image_inputs (
           task_id TEXT NOT NULL, position INTEGER NOT NULL, data_url TEXT NOT NULL,
           PRIMARY KEY(task_id, position)
         );
         CREATE INDEX IF NOT EXISTS idx_image_tasks_status ON image_tasks(status);",
    )?;
    Ok(connection)
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    // 重启后不能确认上游是否已消耗额度，不自动重放请求。
    open(paths)?.execute(
        "UPDATE image_tasks SET status = 'interrupted', payload_json = json_set(payload_json,
         '$.status', 'interrupted', '$.error.message', '应用已退出，生成结果未知；请确认后重新提交')
         WHERE status IN ('processing', 'queued')",
        [],
    )?;
    Ok(())
}

// 一轮任务原子入队，共享输入图片，避免 100 张生成重复保存 100 份参考图。
pub fn create_batch(paths: &AppPaths, tasks: &[Value], images: &[String], mask: &str) -> Result<(), ManagerError> {
    if tasks.is_empty() || tasks.len() > 100 { return Err(ManagerError::System("一轮可提交 1–100 个任务".into())); }
    let mut connection = open(paths)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for task in tasks {
        transaction.execute(
            "INSERT INTO image_tasks(id, created_at, status, payload_json) VALUES (?1, ?2, 'queued', ?3)",
            params![task["id"].as_str(), task["createdAt"].as_i64(), task.to_string()],
        )?;
    }
    let input_id = tasks[0]["inputId"].as_str();
    for (index, url) in images.iter().enumerate() {
        transaction.execute("INSERT INTO image_inputs(task_id, position, data_url) VALUES (?1, ?2, ?3)", params![input_id, index, url])?;
    }
    if !mask.is_empty() {
        transaction.execute("INSERT INTO image_inputs(task_id, position, data_url) VALUES (?1, -1, ?2)", params![input_id, mask])?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn start(paths: &AppPaths, id: &str) -> Result<bool, ManagerError> {
    Ok(open(paths)?.execute("UPDATE image_tasks SET status = 'processing', payload_json = json_set(payload_json, '$.status', 'processing') WHERE id = ?1 AND status = 'queued'", [id])? == 1)
}

pub fn checkpoint(paths: &AppPaths, id: &str, recovery: &Value) -> Result<(), ManagerError> {
    open(paths)?.execute("UPDATE image_tasks SET payload_json = json_set(payload_json, '$.recovery', json(?2), '$.canResume', json('true')) WHERE id = ?1 AND status = 'processing'", params![id, recovery.to_string()])?;
    Ok(())
}

pub fn requeue(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let mut connection = open(paths)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let raw: String = transaction.query_row("SELECT payload_json FROM image_tasks WHERE id = ?1 AND status IN ('failed', 'interrupted')", [id], |row| row.get(0))?;
    let mut task: Value = serde_json::from_str(&raw)?;
    if task["request"]["generationMode"] != "web" || !task["recovery"]["conversationId"].is_string() {
        return Err(ManagerError::System("此任务没有可继续查询的 Web 会话".into()));
    }
    task["status"] = json!("queued");
    task["error"] = Value::Null;
    task["finishedAt"] = Value::Null;
    transaction.execute("UPDATE image_tasks SET status = 'queued', payload_json = ?2 WHERE id = ?1", params![id, task.to_string()])?;
    transaction.commit()?;
    Ok(task)
}

// 历史索引只返回小型元数据；缩略图和原图仍按当前会话按需读取。
pub fn history(paths: &AppPaths) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let mut statement = connection.prepare("SELECT id, created_at, status, json_extract(payload_json, '$.request.conversationId'), json_extract(payload_json, '$.request.roundId'), json_extract(payload_json, '$.request.prompt') FROM image_tasks ORDER BY created_at")?;
    let rows = statement.query_map([], |row| Ok(json!({
        "id": row.get::<_, String>(0)?, "createdAt": row.get::<_, i64>(1)?, "status": row.get::<_, String>(2)?,
        "conversationId": row.get::<_, Option<String>>(3)?, "roundId": row.get::<_, Option<String>>(4)?, "prompt": row.get::<_, Option<String>>(5)?
    })))?.collect::<Result<Vec<_>, _>>()?;
    Ok(json!(rows))
}

pub fn finish(paths: &AppPaths, task: &Value, images: &[StoredImage]) -> Result<(), ManagerError> {
    let mut connection = open(paths)?;
    let transaction = connection.transaction()?;
    let mut task = task.clone();
    let saved: String = transaction.query_row("SELECT payload_json FROM image_tasks WHERE id = ?1", [task["id"].as_str()], |row| row.get(0))?;
    let saved: Value = serde_json::from_str(&saved)?;
    task["recovery"] = saved["recovery"].clone();
    task["canResume"] = json!(task["status"] == "failed" && task["recovery"]["conversationId"].is_string());
    for (index, image) in images.iter().enumerate() {
        transaction.execute(
            "INSERT INTO image_outputs(task_id, position, format, bytes) VALUES (?1, ?2, ?3, ?4)",
            params![task["id"].as_str(), index, image.format, image.bytes],
        )?;
    }
    let changed = transaction.execute(
        "UPDATE image_tasks SET status = ?2, payload_json = ?3, thumbnail = ?4 WHERE id = ?1 AND status = 'processing'",
        params![task["id"].as_str(), task["status"].as_str(), task.to_string(),
            images.first().map(|image| image.thumbnail.as_str()).unwrap_or("")],
    )?;
    if changed != 1 {
        return Err(ManagerError::System(
            "图片任务状态已变化，无法保存结果".into(),
        ));
    }
    transaction.commit()?;
    Ok(())
}

pub fn list(paths: &AppPaths, payload: &Value) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let status = payload["status"].as_str().unwrap_or("");
    let page = payload["page"].as_u64().unwrap_or(1).clamp(1, 1_000_000);
    let conversation_id = payload["conversationId"].as_str().unwrap_or("");
    let page_size = payload["pageSize"].as_u64().unwrap_or(12).clamp(1, 100);
    let grouped = payload["groupByRound"] == true;
    let filter = "(?1 = '' OR status = ?1) AND (?2 = '' OR COALESCE(NULLIF(json_extract(payload_json, '$.request.conversationId'), ''), 'legacy') = ?2)";
    let round_key = "COALESCE(NULLIF(json_extract(payload_json, '$.request.roundId'), ''), id)";
    let count = if grouped { format!("COUNT(DISTINCT {round_key})") } else { "COUNT(*)".to_string() };
    let total: u64 = connection.query_row(
        &format!("SELECT {count} FROM image_tasks WHERE {filter}"),
        params![status, conversation_id], |row| row.get(0),
    )?;
    // 按轮分页，100 张图片始终一起展示，不在任务分页边界拆开。
    let query = if grouped {
        format!("WITH filtered AS (SELECT *, {round_key} AS round_key FROM image_tasks WHERE {filter}),
            page_rounds AS (SELECT round_key FROM filtered GROUP BY round_key ORDER BY MAX(created_at) DESC, round_key DESC LIMIT ?3 OFFSET ?4)
            SELECT payload_json, thumbnail FROM filtered WHERE round_key IN (SELECT round_key FROM page_rounds)
            ORDER BY created_at DESC, json_extract(payload_json, '$.batchIndex') ASC, id DESC")
    } else {
        format!("SELECT payload_json, thumbnail FROM image_tasks WHERE {filter} ORDER BY created_at DESC, id DESC LIMIT ?3 OFFSET ?4")
    };
    let mut statement = connection.prepare(&query)?;
    let rows = statement
        .query_map(params![status, conversation_id, page_size, (page - 1) * page_size], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let items = rows
        .into_iter()
        .map(|(raw, thumbnail)| {
            let mut task: Value = serde_json::from_str(&raw)?;
            task["thumbnail"] = json!(thumbnail);
            Ok(task)
        })
        .collect::<Result<Vec<Value>, serde_json::Error>>()?;
    Ok(json!({ "items": items, "total": total, "pageSize": page_size }))
}

pub fn detail(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let raw: String = connection
        .query_row(
            "SELECT payload_json FROM image_tasks WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| ManagerError::System("图片任务不存在".into()))?;
    let mut task: Value = serde_json::from_str(&raw)?;
    let images = read_images(paths, &[id.to_string()])?;
    task["data"] = json!(images.into_iter().map(|(_, _, format, bytes)| {
        let encoded = STANDARD.encode(bytes);
        if task["request"]["responseFormat"] == "b64_json" {
            json!({ "b64_json": encoded, "output_format": format })
        } else {
            json!({ "url": format!("data:image/{format};base64,{encoded}"), "output_format": format })
        }
    }).collect::<Vec<_>>());
    Ok(task)
}

pub fn inputs(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let input_id: String = connection.query_row("SELECT COALESCE(json_extract(payload_json, '$.inputId'), id) FROM image_tasks WHERE id = ?1", [id], |row| row.get(0))?;
    let mut statement = connection.prepare(
        "SELECT position, data_url FROM image_inputs WHERE task_id = ?1 ORDER BY position",
    )?;
    let rows = statement.query_map([input_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut images = Vec::new();
    let mut mask = String::new();
    for row in rows {
        let (position, url) = row?;
        if position == -1 {
            mask = url;
        } else {
            images.push(url);
        }
    }
    Ok(json!({ "images": images, "mask": mask }))
}

pub fn read_images(
    paths: &AppPaths,
    ids: &[String],
) -> Result<Vec<(String, usize, String, Vec<u8>)>, ManagerError> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(ManagerError::System("请选取 1–100 个任务".into()));
    }
    let connection = open(paths)?;
    let mut output = Vec::new();
    let mut total_bytes = 0;
    let mut statement = connection.prepare(
        "SELECT position, format, bytes FROM image_outputs WHERE task_id = ?1 ORDER BY position",
    )?;
    for id in ids {
        let rows = statement.query_map([id], |row| {
            Ok((
                id.clone(),
                row.get::<_, usize>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        for row in rows {
            let row = row?;
            total_bytes += row.3.len();
            if total_bytes > 200 * 1024 * 1024 {
                return Err(ManagerError::System(
                    "图片总大小超过 200 MB，请分批导出".into(),
                ));
            }
            output.push(row);
        }
    }
    Ok(output)
}

pub fn delete(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }
    let mut connection = open(paths)?;
    let transaction = connection.transaction()?;
    for id in ids {
        let status: Option<String> = transaction
            .query_row(
                "SELECT status FROM image_tasks WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        if status.as_deref() == Some("processing") {
            return Err(ManagerError::System("生成中的任务不能删除".into()));
        }
        transaction.execute("DELETE FROM image_outputs WHERE task_id = ?1", [id])?;
        transaction.execute("DELETE FROM image_tasks WHERE id = ?1", [id])?;
    }
    transaction.execute("DELETE FROM image_inputs WHERE task_id NOT IN (SELECT COALESCE(json_extract(payload_json, '$.inputId'), id) FROM image_tasks)", [])?;
    transaction.commit()?;
    Ok(json!({ "deleted": ids.len() }))
}

pub fn clear_results(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选择要删除结果的任务".into()));
    }
    let mut connection = open(paths)?;
    let transaction = connection.transaction()?;
    for id in ids {
        let status: String = transaction.query_row("SELECT status FROM image_tasks WHERE id = ?1", [id], |row| row.get(0))?;
        if matches!(status.as_str(), "processing" | "queued") {
            return Err(ManagerError::System("请等待本轮任务完成后删除结果".into()));
        }
        transaction.execute("DELETE FROM image_outputs WHERE task_id = ?1", [id])?;
        transaction.execute("UPDATE image_tasks SET thumbnail = '', payload_json = json_set(payload_json, '$.imageCount', 0, '$.images', json('[]'), '$.resultsDeleted', json('true')) WHERE id = ?1", [id])?;
    }
    transaction.commit()?;
    Ok(json!({"deleted": ids.len()}))
}

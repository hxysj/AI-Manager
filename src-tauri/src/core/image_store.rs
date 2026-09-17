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
         WHERE status = 'processing'",
        [],
    )?;
    Ok(())
}

pub fn create(
    paths: &AppPaths,
    task: &Value,
    images: &[String],
    mask: &str,
) -> Result<(), ManagerError> {
    let mut connection = open(paths)?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let active: u64 = transaction.query_row(
        "SELECT COUNT(*) FROM image_tasks WHERE status = 'processing'",
        [],
        |row| row.get(0),
    )?;
    if active >= 2 {
        return Err(ManagerError::System(
            "已有 2 个图片任务正在生成，请等待完成后再提交".into(),
        ));
    }
    transaction.execute(
        "INSERT INTO image_tasks(id, created_at, status, payload_json) VALUES (?1, ?2, 'processing', ?3)",
        params![task["id"].as_str(), task["createdAt"].as_i64(), task.to_string()],
    )?;
    // 参考图独立存储，列表只读参数；复用编辑任务时按需加载。
    for (index, url) in images.iter().enumerate() {
        transaction.execute(
            "INSERT INTO image_inputs(task_id, position, data_url) VALUES (?1, ?2, ?3)",
            params![task["id"].as_str(), index, url],
        )?;
    }
    if !mask.is_empty() {
        transaction.execute(
            "INSERT INTO image_inputs(task_id, position, data_url) VALUES (?1, -1, ?2)",
            params![task["id"].as_str(), mask],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn finish(paths: &AppPaths, task: &Value, images: &[StoredImage]) -> Result<(), ManagerError> {
    let mut connection = open(paths)?;
    let transaction = connection.transaction()?;
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
    let total: u64 = connection.query_row(
        "SELECT COUNT(*) FROM image_tasks WHERE (?1 = '' OR status = ?1)",
        [status],
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(
        "SELECT payload_json, thumbnail FROM image_tasks WHERE (?1 = '' OR status = ?1)
         ORDER BY created_at DESC, id DESC LIMIT 12 OFFSET ?2",
    )?;
    let rows = statement
        .query_map(params![status, (page - 1) * 12], |row| {
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
    Ok(json!({ "items": items, "total": total, "pageSize": 12 }))
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
    let mut statement = connection.prepare(
        "SELECT position, data_url FROM image_inputs WHERE task_id = ?1 ORDER BY position",
    )?;
    let rows = statement.query_map([id], |row| {
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
    if ids.is_empty() || ids.len() > 12 {
        return Err(ManagerError::System("请选取 1–12 个任务".into()));
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
    if ids.is_empty() || ids.len() > 12 {
        return Err(ManagerError::System("请选取 1–12 个任务".into()));
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
        transaction.execute("DELETE FROM image_inputs WHERE task_id = ?1", [id])?;
        transaction.execute("DELETE FROM image_tasks WHERE id = ?1", [id])?;
    }
    transaction.commit()?;
    Ok(json!({ "deleted": ids.len() }))
}

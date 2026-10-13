use crate::core::{database, error::ManagerError, paths::AppPaths};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{collections::HashSet, fs::File, io::Write};

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
         '$.status', 'interrupted', '$.canResume', json('true'), '$.error.message', '应用已退出，生成结果未知；请恢复后继续')
         WHERE status IN ('processing', 'queued')",
        [],
    )?;
    Ok(())
}

const MAX_TASK_BACKUP_BYTES: usize = 512 * 1024 * 1024;

// 任务备份包含恢复上下文、参考图和已生成图片，独立于通用配置备份保存。
pub fn export_task_bundle(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }

    let connection = open(paths)?;
    let mut task_statement = connection.prepare(
        "SELECT id, created_at, status, payload_json, thumbnail
         FROM image_tasks WHERE id = ?1",
    )?;
    let mut output_statement = connection.prepare(
        "SELECT position, format, bytes FROM image_outputs
         WHERE task_id = ?1 ORDER BY position",
    )?;
    let mut tasks = Vec::new();
    let mut outputs = Vec::new();
    let mut input_ids = HashSet::new();
    let mut raw_bytes = 0usize;

    for id in ids {
        let Some((task_id, created_at, status, payload_json, thumbnail)) = task_statement
            .query_row([id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .optional()?
        else {
            continue;
        };

        raw_bytes = raw_bytes
            .saturating_add(payload_json.len())
            .saturating_add(thumbnail.len());
        if raw_bytes > MAX_TASK_BACKUP_BYTES {
            return Err(ManagerError::System(
                "任务备份超过 512 MB，请分批导出".into(),
            ));
        }
        let payload: Value = serde_json::from_str(&payload_json)?;
        let input_id = payload["inputId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(&task_id);
        input_ids.insert(input_id.to_string());
        tasks.push(json!({
            "id": task_id,
            "createdAt": created_at,
            "status": status,
            "payload": payload,
            "thumbnail": thumbnail
        }));

        let rows = output_statement.query_map([id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        for row in rows {
            let (position, format, bytes) = row?;
            raw_bytes = raw_bytes.saturating_add(bytes.len());
            if raw_bytes > MAX_TASK_BACKUP_BYTES {
                return Err(ManagerError::System(
                    "任务备份超过 512 MB，请分批导出".into(),
                ));
            }
            outputs.push(json!({
                "taskId": id,
                "position": position,
                "format": format,
                "data": STANDARD.encode(bytes)
            }));
        }
    }

    if tasks.is_empty() {
        return Err(ManagerError::System("所选任务不存在或已被删除".into()));
    }

    let mut input_statement = connection.prepare(
        "SELECT position, data_url FROM image_inputs WHERE task_id = ?1 ORDER BY position",
    )?;
    let mut inputs = Vec::new();
    for input_id in input_ids {
        let rows = input_statement.query_map([&input_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (position, data_url) = row?;
            raw_bytes = raw_bytes.saturating_add(data_url.len());
            if raw_bytes > MAX_TASK_BACKUP_BYTES {
                return Err(ManagerError::System(
                    "任务备份超过 512 MB，请分批导出".into(),
                ));
            }
            items.push(json!({ "position": position, "dataUrl": data_url }));
        }
        inputs.push(json!({ "id": input_id, "items": items }));
    }

    Ok(json!({
        "format": "ai-manager-image-task-bundle",
        "version": 1,
        "tasks": tasks,
        "inputs": inputs,
        "outputs": outputs
    }))
}

// 导入时把快照中的排队/生成中任务标成中断，避免导入后误以为上游仍在执行。
pub fn import_task_bundle(
    paths: &AppPaths,
    bundle: &Value,
    target_conversation_id: &str,
) -> Result<Value, ManagerError> {
    if bundle["format"] != "ai-manager-image-task-bundle" || bundle["version"] != 1 {
        return Err(ManagerError::System("任务备份格式或版本不支持".into()));
    }
    let task_entries = bundle["tasks"]
        .as_array()
        .filter(|items| !items.is_empty() && items.len() <= 10000)
        .ok_or_else(|| ManagerError::System("任务备份缺少有效任务".into()))?;

    let mut connection = open(paths)?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut seen_ids = HashSet::new();
    let mut imported = 0usize;
    let mut updated = 0usize;
    let mut resumable = 0usize;

    for entry in task_entries {
        let id = entry["id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ManagerError::System("任务备份包含无效任务 ID".into()))?;
        if !seen_ids.insert(id.to_string()) {
            continue;
        }
        let mut payload = entry["payload"].clone();
        if !payload.is_object() {
            return Err(ManagerError::System("任务备份包含无效任务参数".into()));
        }
        payload["id"] = json!(id);
        if !target_conversation_id.trim().is_empty() {
            payload["request"]["conversationId"] = json!(target_conversation_id);
            if payload["batchKey"].is_string() {
                if let Some(round_id) = payload["request"]["roundId"].as_str() {
                    payload["batchKey"] = json!(format!(
                        "{}:{}",
                        target_conversation_id, round_id
                    ));
                }
            }
        }
        let status = entry["status"]
            .as_str()
            .map(str::to_string)
            .or_else(|| payload["status"].as_str().map(str::to_string))
            .unwrap_or_else(|| "interrupted".to_string());
        let requested_count = payload["request"]["n"].as_u64().unwrap_or(1);
        let saved_count = payload["imageCount"].as_u64().unwrap_or(0);
        let resumable_status = matches!(status.as_str(), "queued" | "processing")
            || (status == "partial" && saved_count < requested_count);
        let imported_status = if resumable_status {
            "interrupted".to_string()
        } else {
            status
        };
        payload["status"] = json!(imported_status);
        if resumable_status {
            resumable += 1;
            payload["canResume"] = json!(true);
            payload["finishedAt"] = Value::Null;
            payload["error"] = json!({
                "source": "backup",
                "message": "任务从备份恢复，点击继续等待以恢复生成"
            });
        }
        let payload_json = serde_json::to_string(&payload)?;
        let created_at = entry["createdAt"]
            .as_i64()
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let thumbnail = entry["thumbnail"].as_str().unwrap_or("");
        let existed = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM image_tasks WHERE id = ?1)",
            [id],
            |row| row.get::<_, bool>(0),
        )?;
        transaction.execute(
            "INSERT INTO image_tasks(id, created_at, status, payload_json, thumbnail)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               created_at = excluded.created_at,
               status = excluded.status,
               payload_json = excluded.payload_json,
               thumbnail = excluded.thumbnail",
            params![id, created_at, imported_status, payload_json, thumbnail],
        )?;
        if existed {
            updated += 1;
        } else {
            imported += 1;
        }
        transaction.execute("DELETE FROM image_outputs WHERE task_id = ?1", [id])?;
    }

    if let Some(inputs) = bundle["inputs"].as_array() {
        for input in inputs {
            let input_id = input["id"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ManagerError::System("任务备份包含无效输入 ID".into()))?;
            for item in input["items"].as_array().cloned().unwrap_or_default() {
                let position = item["position"]
                    .as_i64()
                    .ok_or_else(|| ManagerError::System("任务备份包含无效输入位置".into()))?;
                let data_url = item["dataUrl"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| ManagerError::System("任务备份包含空输入图片".into()))?;
                transaction.execute(
                    "INSERT OR REPLACE INTO image_inputs(task_id, position, data_url)
                     VALUES (?1, ?2, ?3)",
                    params![input_id, position, data_url],
                )?;
            }
        }
    }

    if let Some(outputs) = bundle["outputs"].as_array() {
        for output in outputs {
            let task_id = output["taskId"]
                .as_str()
                .filter(|value| seen_ids.contains(*value))
                .ok_or_else(|| ManagerError::System("任务备份包含无效输出任务".into()))?;
            let position = output["position"]
                .as_i64()
                .ok_or_else(|| ManagerError::System("任务备份包含无效输出位置".into()))?;
            let format = output["format"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ManagerError::System("任务备份包含无效图片格式".into()))?;
            let encoded = output["data"]
                .as_str()
                .ok_or_else(|| ManagerError::System("任务备份缺少图片数据".into()))?;
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|error| ManagerError::System(error.to_string()))?;
            transaction.execute(
                "INSERT OR REPLACE INTO image_outputs(task_id, position, format, bytes)
                 VALUES (?1, ?2, ?3, ?4)",
                params![task_id, position, format, bytes],
            )?;
        }
    }

    transaction.commit()?;
    Ok(json!({
        "imported": imported,
        "updated": updated,
        "resumable": resumable
    }))
}

// 一轮任务原子入队，共享输入图片，避免 100 张生成重复保存 100 份参考图。
pub fn create_batch(paths: &AppPaths, tasks: &[Value], images: &[String], mask: &str) -> Result<(), ManagerError> {
    if tasks.is_empty() || tasks.len() > 10000 { return Err(ManagerError::System("一轮可提交 1–10000 个任务".into())); }
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

pub fn start(paths: &AppPaths, task: &Value) -> Result<Option<Value>, ManagerError> {
    let raw: Option<String> = open(paths)?.query_row(
        "UPDATE image_tasks SET status = 'processing', payload_json = json_set(payload_json, '$.status', 'processing', '$.revision', COALESCE(json_extract(payload_json, '$.revision'), 0) + 1)
         WHERE id = ?1 AND status = 'queued' AND COALESCE(json_extract(payload_json, '$.revision'), 0) = ?2 RETURNING payload_json",
        params![task["id"].as_str(), task["revision"].as_u64().unwrap_or(0)],
        |row| row.get(0),
    ).optional()?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(ManagerError::from)).transpose()
}

pub fn checkpoint(paths: &AppPaths, id: &str, revision: u64, recovery: &Value) -> Result<(), ManagerError> {
    open(paths)?.execute("UPDATE image_tasks SET payload_json = json_set(payload_json, '$.recovery', json(?2), '$.canResume', json('true')) WHERE id = ?1 AND status = 'processing' AND COALESCE(json_extract(payload_json, '$.revision'), 0) = ?3", params![id, recovery.to_string(), revision])?;
    Ok(())
}

#[allow(dead_code)]
pub fn requeue(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    requeue_with_concurrency(paths, id, None, None)
}

pub fn requeue_with_concurrency(
    paths: &AppPaths,
    id: &str,
    concurrency: Option<u8>,
    generation_mode: Option<&str>,
) -> Result<Value, ManagerError> {
    let mut connection = open(paths)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (status, raw): (String, String) = transaction.query_row(
        "SELECT status, payload_json FROM image_tasks WHERE id = ?1 AND status IN ('failed', 'interrupted')",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut task: Value = serde_json::from_str(&raw)?;
    if let Some(generation_mode) = generation_mode {
        task["request"]["generationMode"] = json!(generation_mode);
    }
    // 失败任务必须重新提交请求创建新会话；只有中断任务才继续查询原 Web 会话。
    if status != "interrupted"
        || task["request"]["generationMode"] != "web"
        || !task["recovery"]["conversationId"].is_string()
    {
        task["recovery"] = Value::Null;
    }
    if let Some(concurrency) = concurrency {
        task["batchConcurrency"] = json!(concurrency);
    }
    task["status"] = json!("queued");
    task["error"] = Value::Null;
    task["finishedAt"] = Value::Null;
    task["revision"] = json!(task["revision"].as_u64().unwrap_or(0) + 1);
    transaction.execute("UPDATE image_tasks SET status = 'queued', payload_json = ?2 WHERE id = ?1", params![id, task.to_string()])?;
    transaction.commit()?;
    Ok(task)
}

// 历史索引只返回小型元数据；缩略图和原图仍按当前会话按需读取。
pub fn history(paths: &AppPaths) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let mut statement = connection.prepare("SELECT id, created_at, status, json_extract(payload_json, '$.request.conversationId'), json_extract(payload_json, '$.request.roundId'), json_extract(payload_json, '$.request.prompt'), json_extract(payload_json, '$.request.batchName'), COALESCE(json_extract(payload_json, '$.revision'), 0) FROM image_tasks ORDER BY created_at")?;
    let rows = statement.query_map([], |row| Ok(json!({
        "id": row.get::<_, String>(0)?, "createdAt": row.get::<_, i64>(1)?, "status": row.get::<_, String>(2)?,
        "conversationId": row.get::<_, Option<String>>(3)?, "roundId": row.get::<_, Option<String>>(4)?, "prompt": row.get::<_, Option<String>>(5)?, "batchName": row.get::<_, Option<String>>(6)?, "revision": row.get::<_, u64>(7)?
    })))?.collect::<Result<Vec<_>, _>>()?;
    Ok(json!(rows))
}

pub fn finish(paths: &AppPaths, task: &Value, images: &[StoredImage]) -> Result<Option<Value>, ManagerError> {
    let mut connection = open(paths)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut task = task.clone();
    let saved: Option<String> = transaction.query_row("SELECT payload_json FROM image_tasks WHERE id = ?1", [task["id"].as_str()], |row| row.get(0)).optional()?;
    let Some(saved) = saved else { return Ok(None) };
    let saved: Value = serde_json::from_str(&saved)?;
    // 取消或恢复已经改变版本，旧请求既不能写入图片，也不能发布完成状态。
    if saved["status"] != "processing" || saved["revision"] != task["revision"] {
        return Ok(None);
    }
    task["recovery"] = saved["recovery"].clone();
    task["canResume"] = json!(task["status"] == "failed" && task["recovery"]["conversationId"].is_string());
    task["revision"] = json!(saved["revision"].as_u64().unwrap_or(0) + 1);
    for (index, image) in images.iter().enumerate() {
        transaction.execute(
            "INSERT INTO image_outputs(task_id, position, format, bytes)
             VALUES (?1, ?2, ?3, ?4)",
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
    task["thumbnail"] = json!(images.first().map(|image| image.thumbnail.as_str()).unwrap_or(""));
    Ok(Some(task))
}

// 等待批次只读取小型状态快照，不重复传输任务详情和图片。
pub fn task_states(paths: &AppPaths, ids: &[String]) -> Result<Vec<Value>, ManagerError> {
    let connection = open(paths)?;
    let mut statement = connection.prepare(
        "SELECT id, status, COALESCE(json_extract(payload_json, '$.revision'), 0), COALESCE(json_extract(payload_json, '$.imageCount'), 0), json_extract(payload_json, '$.error')
         FROM image_tasks WHERE id IN (SELECT value FROM json_each(?1))",
    )?;
    let rows = statement.query_map([serde_json::to_string(ids)?], |row| {
        let error = row.get::<_, Option<String>>(4)?.map(|raw| serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw)));
        Ok(json!({ "id": row.get::<_, String>(0)?, "status": row.get::<_, String>(1)?, "revision": row.get::<_, u64>(2)?, "imageCount": row.get::<_, u64>(3)?, "error": error }))
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn previews(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 100 { return Err(ManagerError::System("请选取 1–100 个任务".into())); }
    let connection = open(paths)?;
    let mut statement = connection.prepare("SELECT id, thumbnail FROM image_tasks WHERE id IN (SELECT value FROM json_each(?1))")?;
    let rows = statement.query_map([serde_json::to_string(ids)?], |row| Ok(json!({ "id": row.get::<_, String>(0)?, "thumbnail": row.get::<_, String>(1)? })))?.collect::<Result<Vec<_>, _>>()?;
    Ok(json!(rows))
}

pub fn list(paths: &AppPaths, payload: &Value) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let status = payload["status"].as_str().unwrap_or("");
    let page = payload["page"].as_u64().unwrap_or(1).clamp(1, 1_000_000);
    let conversation_id = payload["conversationId"].as_str().unwrap_or("");
    let page_size = payload["pageSize"].as_u64().unwrap_or(12).clamp(1, 100);
    let grouped = payload["groupByRound"] == true;
    let thumbnail = if payload["includeThumbnails"] == false { "''" } else { "thumbnail" };
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
            SELECT payload_json, {thumbnail} FROM filtered WHERE round_key IN (SELECT round_key FROM page_rounds)
            ORDER BY created_at DESC, json_extract(payload_json, '$.batchIndex') ASC, id DESC")
    } else {
        format!("SELECT payload_json, {thumbnail} FROM image_tasks WHERE {filter} ORDER BY created_at DESC, id DESC LIMIT ?3 OFFSET ?4")
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

pub fn export_image_count(paths: &AppPaths, ids: &[String]) -> Result<usize, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }
    let connection = open(paths)?;
    let count = connection.query_row(
        "SELECT COUNT(*) FROM image_outputs WHERE task_id IN (SELECT value FROM json_each(?1))",
        [serde_json::to_string(ids)?],
        |row| row.get::<_, usize>(0),
    )?;
    Ok(count)
}

fn sanitize_export_stem(value: &str) -> String {
    let mut stem = value
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    stem = stem.trim().trim_matches('.').chars().take(80).collect();
    if stem.is_empty() {
        "image".into()
    } else {
        stem
    }
}

// 合集导出逐张写入 ZIP，避免把 100 张以上图片一次性读进内存。
pub fn write_export_zip_with_progress<F>(
    paths: &AppPaths,
    ids: &[String],
    target: &str,
    mut progress: F,
) -> Result<usize, ManagerError>
where
    F: FnMut(usize),
{
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }
    let connection = open(paths)?;
    let mut statement = connection.prepare(
        "SELECT position, format, bytes FROM image_outputs WHERE task_id = ?1 ORDER BY position",
    )?;
    let mut task_statement = connection.prepare(
        "SELECT json_extract(payload_json, '$.request.batchName'), json_extract(payload_json, '$.batchIndex')
         FROM image_tasks WHERE id = ?1",
    )?;
    let file = File::create(target)?;
    let mut archive = zip::ZipWriter::new(file);
    // PNG、JPEG、WebP 已经压缩过，直接存储可避免再次 Deflate 消耗 CPU。
    let file_options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    let mut entry_names = HashSet::new();
    let mut count = 0;
    for id in ids {
        let (batch_name, batch_index): (Option<String>, Option<i64>) = task_statement
            .query_row([id], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?
            .unwrap_or((None, None));
        let stem = sanitize_export_stem(batch_name.as_deref().unwrap_or("image"));
        let id_prefix = id.chars().take(8).collect::<String>();
        let rows = statement.query_map([id], |row| {
            Ok((
                row.get::<_, usize>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        for row in rows {
            let (index, format, bytes) = row?;
            let mut file_name = if batch_name.as_deref().is_some_and(|name| !name.trim().is_empty()) {
                if let Some(batch_index) = batch_index {
                    format!(
                        "{stem}-{:03}-image-{:03}.{}",
                        batch_index.saturating_add(1),
                        index + 1,
                        format
                    )
                } else {
                    format!("{stem}-{id_prefix}-{:03}.{}", index + 1, format)
                }
            } else {
                format!("{stem}-{id_prefix}-{:03}.{}", index + 1, format)
            };
            if !entry_names.insert(file_name.clone()) {
                let (base, extension) = file_name
                    .rsplit_once('.')
                    .unwrap_or((file_name.as_str(), "png"));
                let mut suffix = format!("{base}-{id_prefix}.{extension}");
                let mut duplicate = 2;
                while !entry_names.insert(suffix.clone()) {
                    suffix = format!("{base}-{id_prefix}-{duplicate}.{extension}");
                    duplicate += 1;
                }
                file_name = suffix;
            }
            archive
                .start_file(file_name, file_options)
                .map_err(|_| ManagerError::System("创建图片压缩包失败".into()))?;
            archive.write_all(&bytes)?;
            count += 1;
            progress(count);
        }
    }
    if count == 0 {
        let file = archive
            .finish()
            .map_err(|_| ManagerError::System("完成图片压缩包失败".into()))?;
        drop(file);
        let _ = std::fs::remove_file(target);
        return Err(ManagerError::System("所选任务没有可导出的图片".into()));
    }
    archive
        .finish()
        .map_err(|_| ManagerError::System("完成图片压缩包失败".into()))?;
    Ok(count)
}

pub fn write_export_zip(paths: &AppPaths, ids: &[String], target: &str) -> Result<usize, ManagerError> {
    write_export_zip_with_progress(paths, ids, target, |_| {})
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

pub fn cancel(paths: &AppPaths, ids: &[String], recoverable: bool) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }
    let mut connection = open(paths)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let ids_json = serde_json::to_string(ids)?;
    let error_message = if recoverable {
        "批量任务已暂停"
    } else {
        "任务已由用户取消"
    };
    let items = {
        let mut statement = transaction.prepare(
            "UPDATE image_tasks SET status = 'interrupted', payload_json = json_set(payload_json, '$.status', 'interrupted', '$.canResume', json(?2), '$.error.message', ?3, '$.finishedAt', ?4, '$.revision', COALESCE(json_extract(payload_json, '$.revision'), 0) + 1)
             WHERE id IN (SELECT value FROM json_each(?1)) AND status IN ('queued', 'processing') RETURNING payload_json",
        )?;
        let raw = statement.query_map(params![
            ids_json,
            if recoverable { "true" } else { "false" },
            error_message,
            chrono::Utc::now().timestamp_millis()
        ], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        raw.iter().map(|raw| serde_json::from_str::<Value>(raw)).collect::<Result<Vec<_>, _>>()?
    };
    transaction.commit()?;
    Ok(json!({ "cancelled": items.len(), "ids": items.iter().map(|task| &task["id"]).collect::<Vec<_>>(), "items": items }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::resolve_app_paths;
    use std::path::Path;

    #[test]
    fn cancelled_and_old_attempts_cannot_commit_or_overwrite_recovery() {
        let root = std::env::temp_dir().join(format!("image-cancel-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let queued = json!({ "id": "task", "createdAt": 1, "status": "queued", "request": { "generationMode": "web" }, "imageCount": 0 });
        create_batch(&paths, std::slice::from_ref(&queued), &[], "").unwrap();
        let mut old = start(&paths, &queued).unwrap().unwrap();
        checkpoint(&paths, "task", old["revision"].as_u64().unwrap(), &json!({"conversationId": "old"})).unwrap();
        let cancelled = cancel(&paths, &["task".into()], true).unwrap();
        assert_eq!(cancelled["cancelled"], 1);
        assert_eq!(cancelled["items"][0]["status"], "interrupted");
        assert_eq!(cancel(&paths, &["task".into()], true).unwrap()["ids"], json!([]));
        let resumed = requeue(&paths, "task").unwrap();
        assert!(start(&paths, &queued).unwrap().is_none());
        let mut current = start(&paths, &resumed).unwrap().unwrap();
        checkpoint(&paths, "task", current["revision"].as_u64().unwrap(), &json!({"conversationId": "new"})).unwrap();
        checkpoint(&paths, "task", old["revision"].as_u64().unwrap(), &json!({"conversationId": "stale"})).unwrap();
        old["status"] = json!("completed");
        let image = StoredImage { bytes: vec![1], format: "png".into(), width: 1, height: 1, revised_prompt: String::new(), thumbnail: "preview".into() };
        assert!(finish(&paths, &old, std::slice::from_ref(&image)).unwrap().is_none());
        assert!(read_images(&paths, &["task".into()]).unwrap().is_empty());
        current["status"] = json!("failed");
        let saved = finish(&paths, &current, &[]).unwrap().unwrap();
        assert_eq!(saved["canResume"], true);
        assert_eq!(saved["recovery"]["conversationId"], "new");
        assert_eq!(saved["revision"], current["revision"].as_u64().unwrap() + 1);
        assert_eq!(cancel(&paths, &["task".into()], false).unwrap()["cancelled"], 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_output_write_rolls_back_and_previews_are_read_on_demand() {
        let root = std::env::temp_dir().join(format!("image-save-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let queued = json!({ "id": "task", "createdAt": 1, "status": "queued", "imageCount": 0 });
        create_batch(&paths, std::slice::from_ref(&queued), &[], "").unwrap();
        let mut task = start(&paths, &queued).unwrap().unwrap();
        task["status"] = json!("completed");
        task["imageCount"] = json!(1);
        let image = StoredImage { bytes: vec![1], format: "png".into(), width: 1, height: 1, revised_prompt: String::new(), thumbnail: "preview".into() };
        let connection = open(&paths).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_image_save BEFORE INSERT ON image_outputs BEGIN SELECT RAISE(ABORT, 'save failed'); END;").unwrap();
        assert!(finish(&paths, &task, std::slice::from_ref(&image)).is_err());
        assert_eq!(task_states(&paths, &["task".into()]).unwrap()[0]["status"], "processing");
        assert!(read_images(&paths, &["task".into()]).unwrap().is_empty());
        connection.execute_batch("DROP TRIGGER fail_image_save").unwrap();
        drop(connection);
        finish(&paths, &task, &[image]).unwrap().unwrap();
        assert_eq!(list(&paths, &json!({ "includeThumbnails": false })).unwrap()["items"][0]["thumbnail"], "");
        assert_eq!(previews(&paths, &["task".into()]).unwrap()[0]["thumbnail"], "preview");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_zip_supports_more_than_one_hundred_completed_tasks() {
        let root = std::env::temp_dir().join(format!("image-export-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let tasks = (0..101)
            .map(|index| {
                json!({
                    "id": format!("task-{index}"),
                    "createdAt": index,
                    "status": "completed",
                    "inputId": format!("input-{index}"),
                    "request": { "n": 1 },
                    "imageCount": 1
                })
            })
            .collect::<Vec<_>>();
        let ids = tasks
            .iter()
            .map(|task| task["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        create_batch(&paths, &tasks, &[], "").unwrap();
        let connection = open(&paths).unwrap();
        for id in &ids {
            connection
                .execute(
                    "INSERT INTO image_outputs(task_id, position, format, bytes) VALUES (?1, 0, 'png', ?2)",
                    params![id, vec![1u8, 2, 3]],
                )
                .unwrap();
        }
        drop(connection);
        let target = root.join("images.zip");
        assert_eq!(write_export_zip(&paths, &ids, target.to_str().unwrap()).unwrap(), 101);
        let archive = zip::ZipArchive::new(File::open(&target).unwrap()).unwrap();
        assert_eq!(archive.len(), 101);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_zip_reports_progress_and_names_batch_images() {
        let root = std::env::temp_dir().join(format!("image-export-progress-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let task = json!({
            "id": "named-task",
            "createdAt": 1,
            "status": "completed",
            "request": { "batchName": "春季/海报", "n": 2 },
            "batchIndex": 2,
            "imageCount": 2
        });
        create_batch(&paths, std::slice::from_ref(&task), &[], "").unwrap();
        let connection = open(&paths).unwrap();
        for position in 0..2 {
            connection
                .execute(
                    "INSERT INTO image_outputs(task_id, position, format, bytes) VALUES (?1, ?2, 'png', ?3)",
                    params!["named-task", position, vec![position as u8]],
                )
                .unwrap();
        }
        drop(connection);
        assert_eq!(export_image_count(&paths, &["named-task".into()]).unwrap(), 2);
        let target = root.join("images.zip");
        let mut progress = Vec::new();
        assert_eq!(
            write_export_zip_with_progress(
                &paths,
                &["named-task".into()],
                target.to_str().unwrap(),
                |completed| progress.push(completed),
            )
            .unwrap(),
            2
        );
        assert_eq!(progress, vec![1, 2]);
        let mut archive = zip::ZipArchive::new(File::open(&target).unwrap()).unwrap();
        assert_eq!(
            archive
                .by_name("春季_海报-003-image-001.png")
                .unwrap()
                .compression(),
            zip::CompressionMethod::Stored
        );
        assert_eq!(
            archive
                .by_name("春季_海报-003-image-002.png")
                .unwrap()
                .compression(),
            zip::CompressionMethod::Stored
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn task_bundle_round_trip_preserves_inputs_outputs_and_resume_state() {
        let suffix = uuid::Uuid::new_v4().to_string();
        let source_root = std::env::temp_dir().join(format!("image-task-source-{suffix}"));
        let target_root = std::env::temp_dir().join(format!("image-task-target-{suffix}"));
        let source_paths = resolve_app_paths(Path::new(&source_root));
        let target_paths = resolve_app_paths(Path::new(&target_root));
        let task = json!({
            "id": "task-1",
            "createdAt": 1,
            "status": "queued",
            "request": {
                "conversationId": "source-conversation",
                "generationMode": "codex",
                "n": 1
            },
            "inputId": "input-1",
            "imageCount": 0
        });

        create_batch(
            &source_paths,
            std::slice::from_ref(&task),
            &["data:image/png;base64,reference".to_string()],
            "",
        )
        .unwrap();
        open(&source_paths)
            .unwrap()
            .execute(
                "INSERT INTO image_outputs(task_id, position, format, bytes)
                 VALUES ('task-1', 0, 'png', ?1)",
                [vec![1u8, 2, 3]],
            )
            .unwrap();

        let bundle = export_task_bundle(&source_paths, &["task-1".to_string()]).unwrap();
        let result = import_task_bundle(&target_paths, &bundle, "target-conversation").unwrap();
        assert_eq!(result["imported"], 1);
        assert_eq!(result["resumable"], 1);

        let listed = list(
            &target_paths,
            &json!({
                "conversationId": "target-conversation",
                "page": 1,
                "pageSize": 10
            }),
        )
        .unwrap();
        assert_eq!(listed["items"][0]["status"], "interrupted");
        assert_eq!(
            inputs(&target_paths, "task-1").unwrap()["images"][0],
            "data:image/png;base64,reference"
        );
        assert_eq!(read_images(&target_paths, &["task-1".to_string()]).unwrap()[0].3, vec![1, 2, 3]);

        std::fs::remove_dir_all(source_root).unwrap();
        std::fs::remove_dir_all(target_root).unwrap();
    }
}

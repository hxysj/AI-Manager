use crate::core::{database, error::ManagerError, paths::AppPaths};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};
use std::{
    collections::{HashSet, VecDeque},
    fs::{self, File},
    io::{Cursor, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

pub struct StoredImage {
    pub bytes: Vec<u8>,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub revised_prompt: String,
}

const IMAGE_OUTPUT_DIR: &str = "image-outputs";
const IMAGE_INPUT_DIR: &str = "image-inputs";

#[derive(Default)]
struct ThumbnailCache {
    entries: VecDeque<(PathBuf, SystemTime, u64, String)>,
    bytes: usize,
}

impl ThumbnailCache {
    fn load(&mut self, path: &Path) -> Result<String, ManagerError> {
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;
        if let Some(index) = self.entries.iter().position(|entry| entry.0 == path) {
            let entry = self.entries.remove(index).unwrap();
            if entry.1 == modified && entry.2 == metadata.len() {
                let thumbnail = entry.3.clone();
                self.entries.push_back(entry);
                return Ok(thumbnail);
            }
            self.bytes -= entry.3.len();
        }

        // 直接从文件解码，缩小后立即释放原图，不在内存中额外保留原始文件副本。
        let mut reader = image::ImageReader::open(path)?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let thumbnail = reader
            .decode()
            .map_err(|error| ManagerError::System(format!("读取预览图片失败：{error}")))?
            .thumbnail(320, 240);
        let mut bytes = Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|error| ManagerError::System(format!("生成缩略图失败：{error}")))?;
        let data_url = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(bytes.into_inner())
        );

        // 只缓存压缩后的临时预览，按最近使用顺序淘汰，同时限制容量和条目数。
        while self.bytes + data_url.len() > 8 * 1024 * 1024 || self.entries.len() >= 128 {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= entry.3.len();
        }
        self.bytes += data_url.len();
        self.entries.push_back((
            path.to_path_buf(),
            modified,
            metadata.len(),
            data_url.clone(),
        ));
        Ok(data_url)
    }
}

fn task_thumbnail(paths: &AppPaths, meta: &TaskMeta) -> Result<String, ManagerError> {
    let Some((_, image_id, format)) = image_records_from_json(&meta.images_json)
        .into_iter()
        .next()
    else {
        return Ok(String::new());
    };
    let group_id = if meta.conversation_id.is_empty() {
        "legacy"
    } else {
        &meta.conversation_id
    };
    let path = output_path(
        paths,
        &output_relative_path(group_id, &meta.id, &image_id, &format),
    )?;
    // 整个解码过程共用一把锁，即使多个页面同时请求，也只解码一张原图。
    static CACHE: OnceLock<Mutex<ThumbnailCache>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(ThumbnailCache::default()))
        .lock()
        .map_err(|_| ManagerError::System("预览缓存不可用".into()))?;
    cache.load(&path)
}

fn output_root(paths: &AppPaths) -> PathBuf {
    Path::new(&paths.workspace_root).join(IMAGE_OUTPUT_DIR)
}

fn input_root(paths: &AppPaths) -> PathBuf {
    Path::new(&paths.workspace_root).join(IMAGE_INPUT_DIR)
}

fn safe_path_part(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "image".into()
    } else {
        value
    }
}

fn output_group_id(task: &Value) -> String {
    task["request"]["conversationId"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task["conversationId"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or("legacy")
        .to_string()
}

fn output_relative_path(group_id: &str, task_id: &str, image_id: &str, format: &str) -> String {
    format!(
        "{}/{}-{}.{}",
        safe_path_part(group_id),
        safe_path_part(task_id),
        safe_path_part(image_id),
        safe_path_part(format)
    )
}

fn output_path(paths: &AppPaths, relative: &str) -> Result<PathBuf, ManagerError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ManagerError::System("图片文件路径无效".into()));
    }
    Ok(output_root(paths).join(relative_path))
}

fn input_relative_path(owner_id: &str, image_id: &str, format: &str) -> String {
    format!(
        "{}/{}.{}",
        safe_path_part(owner_id),
        safe_path_part(image_id),
        safe_path_part(format)
    )
}

fn input_path(paths: &AppPaths, relative: &str) -> Result<PathBuf, ManagerError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ManagerError::System("引用图片文件路径无效".into()));
    }
    Ok(input_root(paths).join(relative_path))
}

fn write_input_file(
    paths: &AppPaths,
    owner_id: &str,
    image_id: &str,
    data_url: &str,
) -> Result<(String, String), ManagerError> {
    let (format, bytes) = if let Some((header, encoded)) = data_url.split_once(',') {
        let format = header
            .strip_prefix("data:image/")
            .and_then(|value| value.split(';').next())
            .filter(|value| !value.is_empty())
            .unwrap_or("url")
            .replace("+xml", "");
        if header.contains(";base64") {
            match STANDARD.decode(encoded) {
                Ok(bytes) => (format, bytes),
                Err(_) => ("url".into(), data_url.as_bytes().to_vec()),
            }
        } else {
            ("url".into(), data_url.as_bytes().to_vec())
        }
    } else {
        ("url".into(), data_url.as_bytes().to_vec())
    };
    let relative = input_relative_path(owner_id, image_id, &format);
    let target = input_path(paths, &relative)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, bytes)?;
    Ok((format, relative))
}

fn write_output_file(
    paths: &AppPaths,
    group_id: &str,
    task_id: &str,
    image_id: &str,
    format: &str,
    bytes: &[u8],
) -> Result<String, ManagerError> {
    let relative = output_relative_path(group_id, task_id, image_id, format);
    let target = output_path(paths, &relative)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = target.with_extension(format!(
        "{}.part-{}",
        safe_path_part(format),
        std::process::id()
    ));
    fs::write(&temporary, bytes)?;
    if target.exists() {
        fs::remove_file(&target)?;
    }
    if let Err(error) = fs::rename(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(relative)
}

fn read_output_file(
    paths: &AppPaths,
    group_id: &str,
    task_id: &str,
    image_id: &str,
    format: &str,
) -> Result<Vec<u8>, ManagerError> {
    let relative = output_relative_path(group_id, task_id, image_id, format);
    Ok(fs::read(output_path(paths, &relative)?)?)
}

fn remove_output_files(paths: &AppPaths, file_paths: &[String]) {
    let mut parent_directories = HashSet::new();
    for file_path in file_paths {
        if let Ok(path) = output_path(paths, file_path) {
            if let Some(parent) = path.parent() {
                parent_directories.insert(parent.to_path_buf());
            }
            let _ = fs::remove_file(path);
        }
    }
    for directory in parent_directories {
        let _ = fs::remove_dir(directory);
    }
}

fn remove_input_files(paths: &AppPaths, file_paths: &[String]) {
    let mut parent_directories = HashSet::new();
    for file_path in file_paths {
        if let Ok(path) = input_path(paths, file_path) {
            if let Some(parent) = path.parent() {
                parent_directories.insert(parent.to_path_buf());
            }
            let _ = fs::remove_file(path);
        }
    }
    for directory in parent_directories {
        let _ = fs::remove_dir(directory);
    }
}

fn move_output_file(
    paths: &AppPaths,
    source_relative: &str,
    target_relative: &str,
) -> Result<(), ManagerError> {
    if source_relative == target_relative {
        return Ok(());
    }
    let source = output_path(paths, source_relative)?;
    let target = output_path(paths, target_relative)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if target.exists() {
        fs::remove_file(&source)?;
    } else if let Err(rename_error) = fs::rename(&source, &target) {
        if let Err(copy_error) = fs::copy(&source, &target) {
            return Err(ManagerError::System(format!(
                "移动图片文件失败: {rename_error}; 复制失败: {copy_error}"
            )));
        }
        if let Err(remove_error) = fs::remove_file(&source) {
            let _ = fs::remove_file(&target);
            return Err(remove_error.into());
        }
    }
    if let Some(parent) = source.parent() {
        let _ = fs::remove_dir(parent);
    }
    Ok(())
}

#[derive(Clone)]
struct TaskMeta {
    id: String,
    created_at: i64,
    status: String,
    conversation_id: String,
    round_id: String,
    input_id: String,
    account_id: String,
    generation_mode: String,
    mode: String,
    prompt: String,
    model: String,
    size: String,
    quality: String,
    requested_count: i64,
    output_format: String,
    background: String,
    response_format: String,
    batch_name: String,
    ratio: String,
    tier: String,
    batch_count: i64,
    batch_index: i64,
    batch_key: String,
    batch_concurrency: i64,
    input_count: i64,
    has_mask: bool,
    image_count: i64,
    revision: i64,
    can_resume: bool,
    error_json: String,
    recovery_json: String,
    finished_at: Option<i64>,
    endpoint: String,
    request_id: String,
    usage_json: String,
    images_json: String,
    updated_at: i64,
    storage_version: i64,
    edit_mode: bool,
    reference_image_ids: String,
    web_session_id: String,
    web_session_deleted: bool,
}

struct TaskRecord {
    meta: TaskMeta,
}

const TASK_SELECT: &str = "id, created_at, status,
    conversation_id, round_id, input_id, account_id, generation_mode, mode,
    prompt, model, size, quality, requested_count, output_format, background,
    response_format, batch_name, ratio, tier, batch_count, batch_index,
    batch_key, batch_concurrency, input_count, has_mask, image_count, revision,
    can_resume, error_json, recovery_json, finished_at, endpoint, request_id,
    usage_json, images_json, updated_at, storage_version, edit_mode,
    reference_image_ids, web_session_id, web_session_deleted";

fn task_meta_from_value(task: &Value, created_at: i64, status: Option<&str>) -> TaskMeta {
    let request = &task["request"];
    let edit_mode = matches!(
        request["mode"]
            .as_str()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "edit" | "image-to-image" | "image_to_image" | "img2img" | "img-to-img"
    );
    let string = |value: &Value| value.as_str().unwrap_or("").to_string();
    let json_text = |value: &Value, fallback: &str| {
        if value.is_null() {
            fallback.to_string()
        } else {
            serde_json::to_string(value).unwrap_or_else(|_| fallback.to_string())
        }
    };
    TaskMeta {
        id: string(&task["id"]),
        created_at: task["createdAt"].as_i64().unwrap_or(created_at),
        status: status
            .map(str::to_string)
            .or_else(|| task["status"].as_str().map(str::to_string))
            .unwrap_or_else(|| "queued".into()),
        conversation_id: request["conversationId"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                task["conversationId"]
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or("")
            .to_string(),
        round_id: string(&request["roundId"]),
        input_id: task["inputId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .or_else(|| task["id"].as_str().filter(|value| !value.is_empty()))
            .unwrap_or("")
            .to_string(),
        account_id: string(&request["accountId"]),
        generation_mode: string(&request["generationMode"]),
        mode: string(&request["mode"]),
        prompt: string(&request["prompt"]),
        model: string(&request["model"]),
        size: string(&request["size"]),
        quality: string(&request["quality"]),
        requested_count: request["n"].as_i64().unwrap_or(1),
        output_format: string(&request["outputFormat"]),
        background: string(&request["background"]),
        response_format: string(&request["responseFormat"]),
        batch_name: string(&request["batchName"]),
        ratio: string(&request["ratio"]),
        tier: string(&request["tier"]),
        batch_count: task["batchCount"].as_i64().unwrap_or(1),
        batch_index: task["batchIndex"].as_i64().unwrap_or(0),
        batch_key: string(&task["batchKey"]),
        batch_concurrency: task["batchConcurrency"].as_i64().unwrap_or(0),
        input_count: task["inputCount"].as_i64().unwrap_or(0),
        has_mask: task["hasMask"] == true,
        image_count: task["imageCount"].as_i64().unwrap_or(0),
        revision: task["revision"].as_i64().unwrap_or(0),
        can_resume: task["canResume"] == true,
        error_json: json_text(&task["error"], "null"),
        recovery_json: json_text(&task["recovery"], "null"),
        finished_at: task["finishedAt"].as_i64(),
        endpoint: string(&task["endpoint"]),
        request_id: string(&task["requestId"]),
        usage_json: json_text(&task["usage"], "{}"),
        images_json: json_text(&task["images"], "[]"),
        updated_at: chrono::Utc::now().timestamp_millis(),
        storage_version: 1,
        edit_mode,
        reference_image_ids: task["referenceImageIds"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default(),
        web_session_id: task["webSessionId"].as_str().unwrap_or("").to_string(),
        web_session_deleted: task["webSessionDeleted"] == true,
    }
}

fn parse_json_column(raw: &str, fallback: Value) -> Value {
    serde_json::from_str(raw).unwrap_or(fallback)
}

fn image_records_from_json(raw: &str) -> Vec<(usize, String, String)> {
    parse_json_column(raw, json!([]))
        .as_array()
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter_map(|(position, item)| {
                    let image_id = item["imageId"].as_str()?.trim();
                    if image_id.is_empty() {
                        return None;
                    }
                    let format = item["format"]
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .unwrap_or("png");
                    Some((position, image_id.to_string(), format.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn task_image_records(
    connection: &Connection,
    task_id: &str,
) -> Result<Vec<(usize, String, String)>, ManagerError> {
    let raw: String = connection.query_row(
        "SELECT images_json FROM image_tasks WHERE id = ?1",
        [task_id],
        |row| row.get(0),
    )?;
    Ok(image_records_from_json(&raw))
}

fn update_task_images(
    transaction: &rusqlite::Transaction<'_>,
    task_id: &str,
    records: &[(usize, String, String, Option<String>)],
) -> Result<(), ManagerError> {
    let mut items = records
        .iter()
        .map(|(position, image_id, format, revised_prompt)| {
            let mut item = json!({ "imageId": image_id, "format": format, "position": position });
            if let Some(prompt) = revised_prompt {
                item["revisedPrompt"] = json!(prompt);
            }
            item
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item["position"].as_u64().unwrap_or(0));
    for (position, item) in items.iter_mut().enumerate() {
        item["position"] = json!(position);
    }
    transaction.execute(
        "UPDATE image_tasks SET images_json = ?2, image_count = ?3, updated_at = ?4 WHERE id = ?1",
        params![
            task_id,
            Value::Array(items).to_string(),
            records.len() as i64,
            chrono::Utc::now().timestamp_millis()
        ],
    )?;
    Ok(())
}

fn find_input_file(
    paths: &AppPaths,
    owner_id: &str,
    image_id: &str,
) -> Result<(String, PathBuf), ManagerError> {
    let directory = input_path(paths, &format!("{}/", safe_path_part(owner_id)))?;
    if directory.is_dir() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.file_stem().and_then(|value| value.to_str()) == Some(image_id) && path.is_file()
            {
                let format = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .filter(|value| !value.is_empty())
                    .unwrap_or("url")
                    .to_string();
                return Ok((format, path));
            }
        }
    }
    Err(ManagerError::System("引用图片文件不存在".into()))
}

fn hydrate_task(meta: &TaskMeta) -> Result<Value, ManagerError> {
    let mut task = json!({});
    let mut request = json!({});
    request["accountId"] = json!(meta.account_id);
    request["generationMode"] = json!(meta.generation_mode);
    request["mode"] = json!(meta.mode);
    request["prompt"] = json!(meta.prompt);
    request["model"] = json!(meta.model);
    request["size"] = json!(meta.size);
    request["quality"] = json!(meta.quality);
    request["n"] = json!(meta.requested_count);
    request["outputFormat"] = json!(meta.output_format);
    request["background"] = json!(meta.background);
    request["responseFormat"] = json!(meta.response_format);
    request["batchName"] = json!(meta.batch_name);
    request["conversationId"] = json!(meta.conversation_id);
    request["roundId"] = json!(meta.round_id);
    request["ratio"] = json!(meta.ratio);
    request["tier"] = json!(meta.tier);
    task["id"] = json!(meta.id);
    task["createdAt"] = json!(meta.created_at);
    task["status"] = json!(meta.status);
    task["request"] = request;
    task["inputId"] = json!(meta.input_id);
    task["batchCount"] = json!(meta.batch_count);
    task["batchIndex"] = json!(meta.batch_index);
    task["batchKey"] = json!(meta.batch_key);
    task["batchConcurrency"] = json!(meta.batch_concurrency);
    task["inputCount"] = json!(meta.input_count);
    task["hasMask"] = json!(meta.has_mask);
    task["images"] = parse_json_column(&meta.images_json, json!([]));
    task["imageCount"] = json!(meta.image_count);
    task["revision"] = json!(meta.revision);
    task["canResume"] = json!(meta.can_resume);
    task["error"] = parse_json_column(&meta.error_json, Value::Null);
    task["recovery"] = parse_json_column(&meta.recovery_json, Value::Null);
    task["finishedAt"] = meta.finished_at.map_or(Value::Null, |value| json!(value));
    task["endpoint"] = json!(meta.endpoint);
    task["requestId"] = json!(meta.request_id);
    task["usage"] = parse_json_column(&meta.usage_json, json!({}));
    task["editMode"] = json!(meta.edit_mode);
    task["referenceImageIds"] = Value::Array(
        meta.reference_image_ids
            .split(',')
            .filter(|id| !id.is_empty())
            .map(|id| json!(id))
            .collect(),
    );
    task["webSessionId"] = json!(meta.web_session_id);
    task["webSessionDeleted"] = json!(meta.web_session_deleted);
    Ok(task)
}

fn task_record_from_row(row: &Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        meta: TaskMeta {
            id: row.get(0)?,
            created_at: row.get(1)?,
            status: row.get(2)?,
            conversation_id: row.get(3)?,
            round_id: row.get(4)?,
            input_id: row.get(5)?,
            account_id: row.get(6)?,
            generation_mode: row.get(7)?,
            mode: row.get(8)?,
            prompt: row.get(9)?,
            model: row.get(10)?,
            size: row.get(11)?,
            quality: row.get(12)?,
            requested_count: row.get(13)?,
            output_format: row.get(14)?,
            background: row.get(15)?,
            response_format: row.get(16)?,
            batch_name: row.get(17)?,
            ratio: row.get(18)?,
            tier: row.get(19)?,
            batch_count: row.get(20)?,
            batch_index: row.get(21)?,
            batch_key: row.get(22)?,
            batch_concurrency: row.get(23)?,
            input_count: row.get(24)?,
            has_mask: row.get(25)?,
            image_count: row.get(26)?,
            revision: row.get(27)?,
            can_resume: row.get(28)?,
            error_json: row.get(29)?,
            recovery_json: row.get(30)?,
            finished_at: row.get(31)?,
            endpoint: row.get(32)?,
            request_id: row.get(33)?,
            usage_json: row.get(34)?,
            images_json: row.get(35)?,
            updated_at: row.get(36)?,
            storage_version: row.get(37)?,
            edit_mode: row.get(38)?,
            reference_image_ids: row.get(39)?,
            web_session_id: row.get(40)?,
            web_session_deleted: row.get(41)?,
        },
    })
}

impl TaskRecord {
    fn value(&self) -> Result<Value, ManagerError> {
        hydrate_task(&self.meta)
    }
}

struct OutputFileGuard<'a> {
    paths: &'a AppPaths,
    files: Vec<String>,
    committed: bool,
}

impl<'a> OutputFileGuard<'a> {
    fn new(paths: &'a AppPaths) -> Self {
        Self {
            paths,
            files: Vec::new(),
            committed: false,
        }
    }
}

impl Drop for OutputFileGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            remove_output_files(self.paths, &self.files);
        }
    }
}

struct InputFileGuard<'a> {
    paths: &'a AppPaths,
    files: Vec<String>,
    committed: bool,
}

impl<'a> InputFileGuard<'a> {
    fn new(paths: &'a AppPaths) -> Self {
        Self {
            paths,
            files: Vec::new(),
            committed: false,
        }
    }
}

impl Drop for InputFileGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            remove_input_files(self.paths, &self.files);
        }
    }
}

fn task_column_definitions() -> [(&'static str, &'static str); 39] {
    [
        ("conversation_id", "TEXT NOT NULL DEFAULT ''"),
        ("round_id", "TEXT NOT NULL DEFAULT ''"),
        ("input_id", "TEXT NOT NULL DEFAULT ''"),
        ("account_id", "TEXT NOT NULL DEFAULT ''"),
        ("generation_mode", "TEXT NOT NULL DEFAULT ''"),
        ("mode", "TEXT NOT NULL DEFAULT ''"),
        ("prompt", "TEXT NOT NULL DEFAULT ''"),
        ("model", "TEXT NOT NULL DEFAULT ''"),
        ("size", "TEXT NOT NULL DEFAULT ''"),
        ("quality", "TEXT NOT NULL DEFAULT ''"),
        ("requested_count", "INTEGER NOT NULL DEFAULT 1"),
        ("output_format", "TEXT NOT NULL DEFAULT ''"),
        ("background", "TEXT NOT NULL DEFAULT ''"),
        ("response_format", "TEXT NOT NULL DEFAULT ''"),
        ("batch_name", "TEXT NOT NULL DEFAULT ''"),
        ("ratio", "TEXT NOT NULL DEFAULT ''"),
        ("tier", "TEXT NOT NULL DEFAULT ''"),
        ("batch_count", "INTEGER NOT NULL DEFAULT 1"),
        ("batch_index", "INTEGER NOT NULL DEFAULT 0"),
        ("batch_key", "TEXT NOT NULL DEFAULT ''"),
        ("batch_concurrency", "INTEGER NOT NULL DEFAULT 0"),
        ("input_count", "INTEGER NOT NULL DEFAULT 0"),
        ("has_mask", "INTEGER NOT NULL DEFAULT 0"),
        ("image_count", "INTEGER NOT NULL DEFAULT 0"),
        ("revision", "INTEGER NOT NULL DEFAULT 0"),
        ("can_resume", "INTEGER NOT NULL DEFAULT 0"),
        ("error_json", "TEXT NOT NULL DEFAULT 'null'"),
        ("recovery_json", "TEXT NOT NULL DEFAULT 'null'"),
        ("finished_at", "INTEGER"),
        ("endpoint", "TEXT NOT NULL DEFAULT ''"),
        ("request_id", "TEXT NOT NULL DEFAULT ''"),
        ("usage_json", "TEXT NOT NULL DEFAULT '{}'"),
        ("images_json", "TEXT NOT NULL DEFAULT '[]'"),
        ("updated_at", "INTEGER NOT NULL DEFAULT 0"),
        ("storage_version", "INTEGER NOT NULL DEFAULT 0"),
        ("edit_mode", "INTEGER NOT NULL DEFAULT 0"),
        ("reference_image_ids", "TEXT NOT NULL DEFAULT ''"),
        ("web_session_id", "TEXT NOT NULL DEFAULT ''"),
        ("web_session_deleted", "INTEGER NOT NULL DEFAULT 0"),
    ]
}

fn upsert_task(
    transaction: &rusqlite::Transaction<'_>,
    meta: &TaskMeta,
) -> Result<(), ManagerError> {
    transaction.execute(
        "INSERT INTO image_tasks(
           id, created_at, status,
           conversation_id, round_id, input_id, account_id, generation_mode, mode,
           prompt, model, size, quality, requested_count, output_format, background,
           response_format, batch_name, ratio, tier, batch_count, batch_index,
           batch_key, batch_concurrency, input_count, has_mask, image_count, revision,
           can_resume, error_json, recovery_json, finished_at, endpoint, request_id,
           usage_json, images_json, updated_at, storage_version
           , edit_mode, reference_image_ids, web_session_id, web_session_deleted
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
           ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
           ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, 1, ?38, ?39, ?40, ?41
         )
         ON CONFLICT(id) DO UPDATE SET
           created_at = excluded.created_at, status = excluded.status,
           conversation_id = excluded.conversation_id, round_id = excluded.round_id,
           input_id = excluded.input_id, account_id = excluded.account_id,
           generation_mode = excluded.generation_mode, mode = excluded.mode,
           prompt = excluded.prompt, model = excluded.model, size = excluded.size,
           quality = excluded.quality, requested_count = excluded.requested_count,
           output_format = excluded.output_format, background = excluded.background,
           response_format = excluded.response_format, batch_name = excluded.batch_name,
           ratio = excluded.ratio, tier = excluded.tier, batch_count = excluded.batch_count,
           batch_index = excluded.batch_index, batch_key = excluded.batch_key,
           batch_concurrency = excluded.batch_concurrency, input_count = excluded.input_count,
           has_mask = excluded.has_mask, image_count = excluded.image_count,
           revision = excluded.revision, can_resume = excluded.can_resume,
           error_json = excluded.error_json, recovery_json = excluded.recovery_json,
           finished_at = excluded.finished_at, endpoint = excluded.endpoint,
           request_id = excluded.request_id, usage_json = excluded.usage_json,
           images_json = excluded.images_json, updated_at = excluded.updated_at,
           storage_version = 1, edit_mode = excluded.edit_mode,
           reference_image_ids = excluded.reference_image_ids,
           web_session_id = excluded.web_session_id,
           web_session_deleted = excluded.web_session_deleted",
        params![
            meta.id,
            meta.created_at,
            meta.status,
            meta.conversation_id,
            meta.round_id,
            meta.input_id,
            meta.account_id,
            meta.generation_mode,
            meta.mode,
            meta.prompt,
            meta.model,
            meta.size,
            meta.quality,
            meta.requested_count,
            meta.output_format,
            meta.background,
            meta.response_format,
            meta.batch_name,
            meta.ratio,
            meta.tier,
            meta.batch_count,
            meta.batch_index,
            meta.batch_key,
            meta.batch_concurrency,
            meta.input_count,
            meta.has_mask,
            meta.image_count,
            meta.revision,
            meta.can_resume,
            meta.error_json,
            meta.recovery_json,
            meta.finished_at,
            meta.endpoint,
            meta.request_id,
            meta.usage_json,
            meta.images_json,
            meta.updated_at,
            meta.edit_mode,
            meta.reference_image_ids,
            meta.web_session_id,
            meta.web_session_deleted,
        ],
    )?;
    Ok(())
}

fn migrate_task_columns(connection: &Connection) -> Result<(), ManagerError> {
    let legacy_table_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'image_tasks_legacy')",
        [], |row| row.get(0)
    )?;
    if legacy_table_exists {
        // 上次迁移中断时，新表可能已复制部分任务；补齐缺失记录后才能删除旧表。
        connection.execute_batch(
            "INSERT OR IGNORE INTO image_tasks(
               id, created_at, status, conversation_id, round_id, input_id, account_id,
               generation_mode, mode, prompt, model, size, quality, requested_count, output_format,
               background, response_format, batch_name, ratio, tier, batch_count, batch_index,
               batch_key, batch_concurrency, input_count, has_mask, image_count, revision, can_resume,
               error_json, recovery_json, finished_at, endpoint, request_id, usage_json, images_json,
               updated_at, storage_version, edit_mode, reference_image_ids, web_session_id, web_session_deleted
             ) SELECT
               id, created_at, status, conversation_id, round_id, input_id, account_id,
               generation_mode, mode, prompt, model, size, quality, requested_count, output_format,
               background, response_format, batch_name, ratio, tier, batch_count, batch_index,
               batch_key, batch_concurrency, input_count, has_mask, image_count, revision, can_resume,
               error_json, recovery_json, finished_at, endpoint, request_id, usage_json, images_json,
               CASE WHEN updated_at = 0 THEN created_at ELSE updated_at END, 1, edit_mode,
               reference_image_ids, web_session_id, web_session_deleted
             FROM image_tasks_legacy;
             DROP TABLE image_tasks_legacy;"
        )?;
    }
    let has_legacy_payload = {
        let mut statement = connection.prepare("PRAGMA table_info(image_tasks)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<HashSet<_>, _>>()?
            .contains("payload_json")
    };
    if has_legacy_payload {
        connection.execute_batch(
        "UPDATE image_tasks SET
           conversation_id = COALESCE(NULLIF(conversation_id, ''), NULLIF(json_extract(payload_json, '$.request.conversationId'), ''), NULLIF(json_extract(payload_json, '$.conversationId'), ''), ''),
           round_id = COALESCE(NULLIF(round_id, ''), NULLIF(json_extract(payload_json, '$.request.roundId'), ''), ''),
           input_id = COALESCE(NULLIF(input_id, ''), NULLIF(json_extract(payload_json, '$.inputId'), ''), id),
           account_id = COALESCE(NULLIF(account_id, ''), NULLIF(json_extract(payload_json, '$.request.accountId'), ''), ''),
           generation_mode = COALESCE(NULLIF(generation_mode, ''), NULLIF(json_extract(payload_json, '$.request.generationMode'), ''), ''),
           mode = COALESCE(NULLIF(mode, ''), NULLIF(json_extract(payload_json, '$.request.mode'), ''), ''),
           prompt = COALESCE(NULLIF(prompt, ''), NULLIF(json_extract(payload_json, '$.request.prompt'), ''), ''),
           model = COALESCE(NULLIF(model, ''), NULLIF(json_extract(payload_json, '$.request.model'), ''), ''),
           size = COALESCE(NULLIF(size, ''), NULLIF(json_extract(payload_json, '$.request.size'), ''), ''),
           quality = COALESCE(NULLIF(quality, ''), NULLIF(json_extract(payload_json, '$.request.quality'), ''), ''),
           requested_count = CASE WHEN requested_count = 1 AND json_extract(payload_json, '$.request.n') IS NOT NULL THEN json_extract(payload_json, '$.request.n') ELSE requested_count END,
           output_format = COALESCE(NULLIF(output_format, ''), NULLIF(json_extract(payload_json, '$.request.outputFormat'), ''), ''),
           background = COALESCE(NULLIF(background, ''), NULLIF(json_extract(payload_json, '$.request.background'), ''), ''),
           response_format = COALESCE(NULLIF(response_format, ''), NULLIF(json_extract(payload_json, '$.request.responseFormat'), ''), ''),
           batch_name = COALESCE(NULLIF(batch_name, ''), NULLIF(json_extract(payload_json, '$.request.batchName'), ''), ''),
           ratio = COALESCE(NULLIF(ratio, ''), NULLIF(json_extract(payload_json, '$.request.ratio'), ''), ''),
           tier = COALESCE(NULLIF(tier, ''), NULLIF(json_extract(payload_json, '$.request.tier'), ''), ''),
           batch_count = CASE WHEN batch_count = 1 AND json_extract(payload_json, '$.batchCount') IS NOT NULL THEN json_extract(payload_json, '$.batchCount') ELSE batch_count END,
           batch_index = COALESCE(json_extract(payload_json, '$.batchIndex'), batch_index),
           batch_key = COALESCE(NULLIF(batch_key, ''), NULLIF(json_extract(payload_json, '$.batchKey'), ''), ''),
           batch_concurrency = COALESCE(json_extract(payload_json, '$.batchConcurrency'), batch_concurrency),
           input_count = COALESCE(json_extract(payload_json, '$.inputCount'), input_count),
           has_mask = COALESCE(json_extract(payload_json, '$.hasMask'), has_mask),
           image_count = COALESCE(json_extract(payload_json, '$.imageCount'), image_count),
           revision = COALESCE(json_extract(payload_json, '$.revision'), revision),
           can_resume = COALESCE(json_extract(payload_json, '$.canResume'), can_resume),
           error_json = COALESCE(json_extract(payload_json, '$.error'), error_json),
           recovery_json = COALESCE(json_extract(payload_json, '$.recovery'), recovery_json),
           finished_at = COALESCE(json_extract(payload_json, '$.finishedAt'), finished_at),
           endpoint = COALESCE(NULLIF(endpoint, ''), NULLIF(json_extract(payload_json, '$.endpoint'), ''), ''),
           request_id = COALESCE(NULLIF(request_id, ''), NULLIF(json_extract(payload_json, '$.requestId'), ''), ''),
           usage_json = COALESCE(json_extract(payload_json, '$.usage'), usage_json),
           images_json = COALESCE(json_extract(payload_json, '$.images'), images_json),
           edit_mode = COALESCE(json_extract(payload_json, '$.editMode'), CASE
             WHEN lower(COALESCE(NULLIF(mode, ''), json_extract(payload_json, '$.request.mode')))
               IN ('edit','image-to-image','image_to_image','img2img','img-to-img') THEN 1 ELSE edit_mode END),
           reference_image_ids = COALESCE(NULLIF(reference_image_ids, ''), ''),
           web_session_id = COALESCE(NULLIF(web_session_id, ''), NULLIF(json_extract(payload_json, '$.webSessionId'), ''),
             NULLIF(json_extract(payload_json, '$.recovery.conversationId'), ''), ''),
           web_session_deleted = COALESCE(json_extract(payload_json, '$.webSessionDeleted'), web_session_deleted),
           updated_at = CASE WHEN updated_at = 0 THEN created_at ELSE updated_at END
         WHERE storage_version = 0;",
      )?;
        connection.execute_batch(
        "DROP TRIGGER IF EXISTS trg_image_tasks_payload_state_sync;
         ALTER TABLE image_tasks RENAME TO image_tasks_legacy;
         CREATE TABLE image_tasks (
           id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, status TEXT NOT NULL,
           conversation_id TEXT NOT NULL DEFAULT '',
           round_id TEXT NOT NULL DEFAULT '', input_id TEXT NOT NULL DEFAULT '', account_id TEXT NOT NULL DEFAULT '',
           generation_mode TEXT NOT NULL DEFAULT '', mode TEXT NOT NULL DEFAULT '', prompt TEXT NOT NULL DEFAULT '',
           model TEXT NOT NULL DEFAULT '', size TEXT NOT NULL DEFAULT '', quality TEXT NOT NULL DEFAULT '',
           requested_count INTEGER NOT NULL DEFAULT 1, output_format TEXT NOT NULL DEFAULT '',
           background TEXT NOT NULL DEFAULT '', response_format TEXT NOT NULL DEFAULT '', batch_name TEXT NOT NULL DEFAULT '',
           ratio TEXT NOT NULL DEFAULT '', tier TEXT NOT NULL DEFAULT '', batch_count INTEGER NOT NULL DEFAULT 1,
           batch_index INTEGER NOT NULL DEFAULT 0, batch_key TEXT NOT NULL DEFAULT '', batch_concurrency INTEGER NOT NULL DEFAULT 0,
           input_count INTEGER NOT NULL DEFAULT 0, has_mask INTEGER NOT NULL DEFAULT 0, image_count INTEGER NOT NULL DEFAULT 0,
           revision INTEGER NOT NULL DEFAULT 0, can_resume INTEGER NOT NULL DEFAULT 0,
           error_json TEXT NOT NULL DEFAULT 'null', recovery_json TEXT NOT NULL DEFAULT 'null', finished_at INTEGER,
           endpoint TEXT NOT NULL DEFAULT '', request_id TEXT NOT NULL DEFAULT '', usage_json TEXT NOT NULL DEFAULT '{}',
           images_json TEXT NOT NULL DEFAULT '[]', updated_at INTEGER NOT NULL DEFAULT 0,
           storage_version INTEGER NOT NULL DEFAULT 1, edit_mode INTEGER NOT NULL DEFAULT 0,
           reference_image_ids TEXT NOT NULL DEFAULT '', web_session_id TEXT NOT NULL DEFAULT '',
           web_session_deleted INTEGER NOT NULL DEFAULT 0
         );
         INSERT INTO image_tasks SELECT id, created_at, status, conversation_id, round_id, input_id,
           account_id, generation_mode, mode, prompt, model, size, quality, requested_count, output_format,
           background, response_format, batch_name, ratio, tier, batch_count, batch_index, batch_key,
           batch_concurrency, input_count, has_mask, image_count, revision, can_resume, error_json,
           recovery_json, finished_at, endpoint, request_id, usage_json, images_json,
           CASE WHEN updated_at = 0 THEN created_at ELSE updated_at END, 1, edit_mode,
           reference_image_ids, web_session_id, web_session_deleted
         FROM image_tasks_legacy;
         DROP TABLE image_tasks_legacy;"
      )?;
    }
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_image_tasks_created ON image_tasks(created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_image_tasks_status ON image_tasks(status);
         CREATE INDEX IF NOT EXISTS idx_image_tasks_conversation ON image_tasks(conversation_id, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_image_tasks_round ON image_tasks(conversation_id, round_id, created_at DESC);
         UPDATE image_tasks SET storage_version = 1 WHERE storage_version = 0;",
    )?;
    Ok(())
}

fn ensure_task_columns(connection: &Connection) -> Result<(), ManagerError> {
    let existing = {
        let mut statement = connection.prepare("PRAGMA table_info(image_tasks)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<std::collections::HashSet<_>, _>>()?
    };
    for (name, definition) in task_column_definitions() {
        if !existing.contains(name) {
            connection.execute_batch(&format!(
                "ALTER TABLE image_tasks ADD COLUMN {name} {definition}"
            ))?;
        }
    }
    migrate_task_columns(connection)?;
    if existing.contains("thumbnail") {
        let still_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('image_tasks') WHERE name = 'thumbnail')",
            [], |row| row.get(0),
        )?;
        if still_exists {
            connection.execute_batch("ALTER TABLE image_tasks DROP COLUMN thumbnail;")?;
        }
        // 迁移完成后在启动流程统一压缩一次数据库，释放旧缩略图占用的页。
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT OR REPLACE INTO app_metadata(key, value) VALUES ('image_thumbnail_compaction_pending', '1');",
        )?;
    }
    Ok(())
}

fn ensure_output_columns(connection: &Connection) -> Result<(), ManagerError> {
    let _ = connection;
    Ok(())
}

fn ensure_input_columns(connection: &Connection) -> Result<(), ManagerError> {
    let _ = connection;
    Ok(())
}

fn ensure_web_session_schema(connection: &Connection) -> Result<(), ManagerError> {
    let session_type: Option<String> = connection
        .query_row(
            "SELECT type FROM sqlite_master WHERE name = 'image_web_sessions'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if session_type.as_deref() == Some("table") {
        let columns = {
            let mut statement = connection.prepare("PRAGMA table_info(image_web_sessions)")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        let required = [
            "id",
            "account_id",
            "local_conversation_id",
            "conversation_id",
            "parent_message_id",
            "created_at",
            "updated_at",
            "task_ids",
        ];
        if !columns.contains("payload_json")
            && required[..required.len() - 1]
                .iter()
                .all(|column| columns.contains(*column))
            && !columns.contains("task_ids")
        {
            connection.execute_batch(
                "ALTER TABLE image_web_sessions ADD COLUMN task_ids TEXT NOT NULL DEFAULT '';",
            )?;
        } else if required.iter().any(|column| !columns.contains(*column)) {
            if !columns.contains("payload_json") {
                return Err(ManagerError::System(
                    "图片会话表缺少必要字段，已保留原表".into(),
                ));
            }
            let legacy_rows = if columns.contains("payload_json") {
                let local_column = if columns.contains("local_id") {
                    "local_id"
                } else {
                    "''"
                };
                let mut statement = connection.prepare(&format!(
                    "SELECT account_id, {local_column}, payload_json FROM image_web_sessions
                     ORDER BY CASE WHEN json_valid(payload_json) THEN COALESCE(json_extract(payload_json, '$.updatedAt'), 0) ELSE 0 END"
                ))?;
                let rows = statement.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            } else {
                Vec::new()
            };
            connection.execute_batch(
                "ALTER TABLE image_web_sessions RENAME TO image_web_sessions_legacy;
                 CREATE TABLE image_web_sessions (
                   id TEXT PRIMARY KEY, account_id TEXT NOT NULL,
                   local_conversation_id TEXT NOT NULL, conversation_id TEXT NOT NULL,
                   parent_message_id TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL, task_ids TEXT NOT NULL DEFAULT ''
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_image_web_sessions_upstream
                   ON image_web_sessions(account_id, conversation_id);",
            )?;
            for (account_id, legacy_local_id, payload_json) in legacy_rows {
                let payload: Value = if payload_json.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&payload_json)?
                };
                let local_conversation_id = payload["localConversationId"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&legacy_local_id);
                let conversation_id = payload["conversationId"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        payload["conversation_id"]
                            .as_str()
                            .filter(|value| !value.is_empty())
                    })
                    .unwrap_or(local_conversation_id);
                if conversation_id.is_empty() {
                    continue;
                }
                let id = payload["id"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| {
                        if legacy_local_id.is_empty() {
                            ""
                        } else {
                            &legacy_local_id
                        }
                    });
                let id = if id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    id.to_string()
                };
                let now = chrono::Utc::now().timestamp_millis();
                let created_at = payload["createdAt"].as_i64().unwrap_or(now);
                let updated_at = payload["updatedAt"].as_i64().unwrap_or(created_at);
                let parent_message_id = payload["parentMessageId"].as_str().unwrap_or("");
                let task_ids = payload["taskIds"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| {
                        payload["taskIds"].as_array().map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                    })
                    .unwrap_or_default();
                connection.execute(
                    "INSERT OR REPLACE INTO image_web_sessions(
                       id, account_id, local_conversation_id, conversation_id,
                       parent_message_id, created_at, updated_at, task_ids
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        account_id,
                        local_conversation_id,
                        conversation_id,
                        parent_message_id,
                        created_at,
                        updated_at,
                        task_ids
                    ],
                )?;
            }
            connection.execute_batch("DROP TABLE image_web_sessions_legacy;")?;
        }
    }
    let old_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'image_web_conversations')",
        [],
        |row| row.get(0),
    )?;
    if !old_exists {
        return Ok(());
    }
    let old_columns = {
        let mut statement = connection.prepare("PRAGMA table_info(image_web_conversations)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<HashSet<_>, _>>()?
    };
    let deleted_filter = if old_columns.contains("is_deleted") {
        connection.execute_batch(
            "UPDATE image_tasks SET web_session_deleted=1
             WHERE EXISTS(SELECT 1 FROM image_web_conversations old
               WHERE old.account_id=image_tasks.account_id AND old.conversation_id=image_tasks.web_session_id
                 AND old.is_deleted=1);",
        )?;
        "WHERE is_deleted = 0"
    } else {
        ""
    };
    connection.execute_batch(&format!(
        "INSERT OR REPLACE INTO image_web_sessions(
           id, account_id, local_conversation_id, conversation_id, parent_message_id,
           created_at, updated_at, task_ids
         ) SELECT id, account_id, local_conversation_id, conversation_id,
                  parent_message_id, created_at, updated_at, ''
           FROM image_web_conversations {deleted_filter};
         DROP TABLE image_web_conversations;"
    ))?;
    connection.execute_batch(
        "UPDATE image_web_sessions SET task_ids=COALESCE(
           (SELECT group_concat(id, ',') FROM image_tasks
            WHERE account_id=image_web_sessions.account_id AND web_session_id=image_web_sessions.conversation_id), '')
         WHERE task_ids='';",
    )?;
    Ok(())
}

fn ensure_asset_storage(connection: &Connection) -> Result<(), ManagerError> {
    // 新结构只在 image_tasks 保存图片 ID；输入、输出不再创建兼容表或视图。
    // 旧表由 initialize() 在文件迁移完成后统一删除。
    let _ = connection;
    Ok(())
}

fn migrate_legacy_assets(connection: &Connection) -> Result<(), ManagerError> {
    let output_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'image_outputs')",
        [],
        |row| row.get(0),
    )?;
    if output_table {
        migrate_output_metadata(connection, "image_outputs")?;
        connection.execute_batch("DROP TABLE image_outputs;")?;
    }
    let input_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'image_inputs')",
        [],
        |row| row.get(0),
    )?;
    if input_table {
        migrate_input_metadata(connection, "image_inputs")?;
        connection.execute_batch("DROP TABLE image_inputs;")?;
    }
    let asset_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'image_assets')",
        [],
        |row| row.get(0),
    )?;
    if asset_table {
        migrate_output_metadata(connection, "image_assets")?;
        migrate_input_metadata(connection, "image_assets")?;
        connection.execute_batch("DROP TABLE image_assets;")?;
    }
    // 旧版本曾把 image_inputs/image_outputs 建成依赖 image_assets 的视图；迁移后连同视图一起移除。
    for name in ["image_inputs", "image_outputs"] {
        let object_type: Option<String> = connection
            .query_row(
                "SELECT type FROM sqlite_master WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .optional()?;
        if object_type.as_deref() == Some("view") {
            connection.execute_batch(&format!("DROP VIEW {name}"))?;
        }
    }
    ensure_asset_storage(connection)
}

fn migrate_legacy_inputs(paths: &AppPaths, connection: &Connection) -> Result<usize, ManagerError> {
    // 兼容视图只用于旧代码读取，不能当作待迁移的物理表处理。
    let object_type: Option<String> = connection
        .query_row(
            "SELECT type FROM sqlite_master WHERE name = 'image_inputs'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if object_type.as_deref() != Some("table") {
        return Ok(0);
    }
    let mut columns = {
        let mut statement = connection.prepare("PRAGMA table_info(image_inputs)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<HashSet<_>, _>>()?
    };
    if !columns.contains("image_id") {
        connection.execute(
            "ALTER TABLE image_inputs ADD COLUMN image_id TEXT NOT NULL DEFAULT ''",
            [],
        )?;
        columns.insert("image_id".into());
    }
    if !columns.contains("format") {
        connection.execute(
            "ALTER TABLE image_inputs ADD COLUMN format TEXT NOT NULL DEFAULT 'url'",
            [],
        )?;
        columns.insert("format".into());
    }
    if !columns.contains("data_url") {
        return Ok(0);
    }
    let missing_ids = connection
        .prepare("SELECT rowid FROM image_inputs WHERE image_id = ''")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for row_id in missing_ids {
        connection.execute(
            "UPDATE image_inputs SET image_id = ?1 WHERE rowid = ?2",
            params![uuid::Uuid::new_v4().to_string(), row_id],
        )?;
    }
    let mut statement = connection.prepare(
        "SELECT task_id, position, image_id, format, data_url
         FROM image_inputs WHERE data_url <> '' ORDER BY task_id, position",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut migrated = 0;
    // 逐张写出并提交进度，旧库包含大量图片时也不同时加载全部图片。
    for row in rows {
        let (owner_id, position, image_id, format, data_url) = row?;
        let actual_format = if format == "url" || format.is_empty() {
            write_input_file(paths, &owner_id, &image_id, &data_url)?.0
        } else {
            let relative = input_relative_path(&owner_id, &image_id, &format);
            let target = input_path(paths, &relative)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(target, data_url.as_bytes())?;
            format
        };
        connection.execute(
            "UPDATE image_inputs SET data_url = '', format = ?3
             WHERE task_id = ?1 AND position = ?2 AND image_id = ?4",
            params![owner_id, position, actual_format, image_id],
        )?;
        migrated += 1;
    }
    drop(statement);
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch("DROP TRIGGER IF EXISTS trg_image_inputs_assign_id;")?;
    transaction.execute_batch(
        "CREATE TABLE image_inputs_compact (
           task_id TEXT NOT NULL, position INTEGER NOT NULL,
           image_id TEXT NOT NULL, format TEXT NOT NULL,
           PRIMARY KEY(task_id, image_id), UNIQUE(task_id, position)
         );
         INSERT INTO image_inputs_compact(task_id, position, image_id, format)
         SELECT task_id, position, image_id, format FROM image_inputs;
         DROP TABLE image_inputs;
         ALTER TABLE image_inputs_compact RENAME TO image_inputs;",
    )?;
    transaction.commit()?;
    Ok(migrated)
}

fn output_schema_columns(connection: &Connection) -> Result<HashSet<String>, ManagerError> {
    let mut statement = connection.prepare("PRAGMA table_info(image_outputs)")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<Result<HashSet<_>, _>>()?)
}

fn migrate_legacy_outputs(
    paths: &AppPaths,
    connection: &Connection,
) -> Result<usize, ManagerError> {
    // 兼容视图只用于旧代码读取，不能当作待迁移的物理表处理。
    let object_type: Option<String> = connection
        .query_row(
            "SELECT type FROM sqlite_master WHERE name = 'image_outputs'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if object_type.as_deref() != Some("table") {
        return Ok(0);
    }
    let mut columns = output_schema_columns(connection)?;
    if !columns.contains("image_id") {
        connection.execute(
            "ALTER TABLE image_outputs ADD COLUMN image_id TEXT NOT NULL DEFAULT ''",
            [],
        )?;
        columns.insert("image_id".into());
    }
    let has_bytes = columns.contains("bytes");
    let has_file_path = columns.contains("file_path");
    let mut migrated = 0;

    // 为旧记录补发图片 ID，之后所有文件路径都只依赖会话、任务和图片 ID。
    let missing_ids = connection
        .prepare("SELECT rowid FROM image_outputs WHERE image_id = ''")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for row_id in missing_ids {
        connection.execute(
            "UPDATE image_outputs SET image_id = ?1 WHERE rowid = ?2",
            params![uuid::Uuid::new_v4().to_string(), row_id],
        )?;
    }

    if has_bytes {
        let mut statement = connection.prepare(
            "SELECT image_outputs.task_id, image_outputs.position, image_outputs.image_id,
                    image_outputs.format, image_outputs.bytes,
                    COALESCE(NULLIF(image_tasks.conversation_id, ''), 'legacy')
             FROM image_outputs
             LEFT JOIN image_tasks ON image_tasks.id = image_outputs.task_id
             WHERE length(image_outputs.bytes) > 0
             ORDER BY image_outputs.task_id, image_outputs.position",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        for row in rows {
            let (task_id, position, image_id, format, bytes, group_id) = row?;
            let relative = output_relative_path(&group_id, &task_id, &image_id, &format);
            let target = output_path(paths, &relative)?;
            if !target.is_file() {
                write_output_file(paths, &group_id, &task_id, &image_id, &format, &bytes)?;
            }
            connection.execute(
                "UPDATE image_outputs SET bytes = X'' WHERE task_id = ?1 AND position = ?2 AND image_id = ?3",
                params![task_id, position, image_id],
            )?;
            migrated += 1;
        }
    }

    if has_file_path {
        // 第一版文件存储使用 image-outputs/<task-id>/<position>.<format>。
        // 启动时按任务会话合并到新路径，数据库只保留可推导的图片 ID。
        let legacy_files = {
            let mut statement = connection.prepare(
                "SELECT image_outputs.task_id, image_outputs.position, image_outputs.image_id,
                        image_outputs.format, image_outputs.file_path,
                        COALESCE(NULLIF(image_tasks.conversation_id, ''), 'legacy')
                 FROM image_outputs
                 LEFT JOIN image_tasks ON image_tasks.id = image_outputs.task_id
                 WHERE COALESCE(image_outputs.file_path, '') <> ''
                 ORDER BY image_outputs.task_id, image_outputs.position",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (task_id, position, image_id, format, old_file_path, group_id) in legacy_files {
            let expected_file_path = output_relative_path(&group_id, &task_id, &image_id, &format);
            if old_file_path != expected_file_path && output_path(paths, &old_file_path)?.is_file()
            {
                move_output_file(paths, &old_file_path, &expected_file_path)?;
                migrated += 1;
            }
            connection.execute(
                "UPDATE image_outputs SET file_path = ?4
                 WHERE task_id = ?1 AND position = ?2 AND image_id = ?3",
                params![task_id, position, image_id, expected_file_path],
            )?;
        }
    }

    if has_bytes || has_file_path {
        // 重建表删除 BLOB 和路径列；VACUUM 会把历史图片占用的 SQLite 页真正还给磁盘。
        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(
            "DROP TRIGGER IF EXISTS trg_image_outputs_assign_id;
             CREATE TABLE image_outputs_compact (
               task_id TEXT NOT NULL,
               position INTEGER NOT NULL,
               image_id TEXT NOT NULL,
               format TEXT NOT NULL,
               PRIMARY KEY(task_id, image_id),
               UNIQUE(task_id, position)
             );
             INSERT INTO image_outputs_compact(task_id, position, image_id, format)
             SELECT task_id, position, image_id, format FROM image_outputs;
             DROP TABLE image_outputs;
             ALTER TABLE image_outputs_compact RENAME TO image_outputs;",
        )?;
        transaction.commit()?;
    }
    Ok(migrated)
}

fn migrate_output_metadata(connection: &Connection, table: &str) -> Result<(), ManagerError> {
    let columns = {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<HashSet<_>, _>>()?
    };
    let (owner, asset, kind_filter) = if table == "image_assets" {
        ("owner_id", "asset_id", "AND kind = 'output'")
    } else {
        ("task_id", "image_id", "")
    };
    if !columns.contains(owner) || !columns.contains(asset) || !columns.contains("position") {
        return Ok(());
    }
    let query = format!(
        "SELECT {owner}, position, {asset}, COALESCE(format, 'png')
         FROM {table} WHERE 1 = 1 {kind_filter} ORDER BY {owner}, position"
    );
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    let mut grouped = std::collections::BTreeMap::<String, Vec<(i64, String, String)>>::new();
    for (task_id, position, image_id, format) in rows {
        grouped
            .entry(task_id)
            .or_default()
            .push((position, image_id, format));
    }
    for (task_id, entries) in grouped {
        let current: String = connection
            .query_row(
                "SELECT images_json FROM image_tasks WHERE id = ?1",
                [&task_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "[]".into());
        let mut images = parse_json_column(&current, json!([]));
        let Some(items) = images.as_array_mut() else {
            continue;
        };
        for (position, image_id, format) in entries {
            let position = position.max(0) as usize;
            while items.len() <= position {
                items.push(json!({}));
            }
            items[position]["imageId"] = json!(image_id);
            items[position]["format"] = json!(format);
        }
        let image_count = items.len() as i64;
        let images_json = images.to_string();
        connection.execute(
            "UPDATE image_tasks SET images_json = ?2, image_count = ?3 WHERE id = ?1",
            params![task_id, images_json, image_count],
        )?;
    }
    Ok(())
}

fn migrate_input_metadata(connection: &Connection, table: &str) -> Result<(), ManagerError> {
    let columns = {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<HashSet<_>, _>>()?
    };
    let (owner, asset, kind_filter) = if table == "image_assets" {
        ("owner_id", "asset_id", "AND kind = 'input'")
    } else {
        ("task_id", "image_id", "")
    };
    if !columns.contains(owner) || !columns.contains(asset) || !columns.contains("position") {
        return Ok(());
    }
    let query = format!(
        "SELECT {owner}, position, {asset}
         FROM {table} WHERE 1 = 1 {kind_filter} ORDER BY {owner}, position"
    );
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    let mut grouped = std::collections::BTreeMap::<String, Vec<(i64, String)>>::new();
    for (input_id, position, image_id) in rows {
        grouped
            .entry(input_id)
            .or_default()
            .push((position, image_id));
    }
    for (input_id, entries) in grouped {
        // 新结构约定遮罩放在引用列表末尾，不能在过滤负位置时丢掉旧遮罩。
        let ids = entries
            .iter()
            .filter(|(position, _)| *position >= 0)
            .chain(entries.iter().filter(|(position, _)| *position < 0))
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>()
            .join(",");
        let has_mask = entries.iter().any(|(position, _)| *position == -1);
        connection.execute(
            "UPDATE image_tasks SET reference_image_ids = CASE
                WHEN reference_image_ids = '' THEN ?2
                ELSE reference_image_ids || CASE WHEN ?2 = '' THEN '' ELSE ',' || ?2 END END,
                input_count = CASE WHEN ?2 = '' THEN input_count ELSE ?3 END,
                has_mask = CASE WHEN ?4 = 1 THEN 1 ELSE has_mask END
             WHERE input_id = ?1",
            params![
                input_id,
                ids,
                entries
                    .iter()
                    .filter(|(position, _)| *position >= 0)
                    .count() as i64,
                has_mask as i64
            ],
        )?;
    }
    Ok(())
}

fn open(paths: &AppPaths) -> Result<Connection, ManagerError> {
    let mut connection = database::open(paths)?;
    // 结构检查后可能写入迁移，提前申请写锁，避免并发快照连接升级读锁时直接报忙。
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    // 旧版本的兼容视图可能引用已经删除的 image_assets，先移除，避免 SQLite 在重建任务表时解析失败。
    for name in ["image_inputs", "image_outputs"] {
        let object_type: Option<String> = transaction
            .query_row(
                "SELECT type FROM sqlite_master WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .optional()?;
        if object_type.as_deref() == Some("view") {
            transaction.execute_batch(&format!("DROP VIEW {name}"))?;
        }
    }
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS image_tasks (
           id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, status TEXT NOT NULL,
           conversation_id TEXT NOT NULL DEFAULT '', round_id TEXT NOT NULL DEFAULT '',
           input_id TEXT NOT NULL DEFAULT '', account_id TEXT NOT NULL DEFAULT '',
           generation_mode TEXT NOT NULL DEFAULT '', mode TEXT NOT NULL DEFAULT '',
           prompt TEXT NOT NULL DEFAULT '', model TEXT NOT NULL DEFAULT '',
           size TEXT NOT NULL DEFAULT '', quality TEXT NOT NULL DEFAULT '',
           requested_count INTEGER NOT NULL DEFAULT 1,
           output_format TEXT NOT NULL DEFAULT '', background TEXT NOT NULL DEFAULT '',
           response_format TEXT NOT NULL DEFAULT '', batch_name TEXT NOT NULL DEFAULT '',
           ratio TEXT NOT NULL DEFAULT '', tier TEXT NOT NULL DEFAULT '',
           batch_count INTEGER NOT NULL DEFAULT 1, batch_index INTEGER NOT NULL DEFAULT 0,
           batch_key TEXT NOT NULL DEFAULT '', batch_concurrency INTEGER NOT NULL DEFAULT 0,
           input_count INTEGER NOT NULL DEFAULT 0, has_mask INTEGER NOT NULL DEFAULT 0,
           image_count INTEGER NOT NULL DEFAULT 0, revision INTEGER NOT NULL DEFAULT 0,
           can_resume INTEGER NOT NULL DEFAULT 0,
           error_json TEXT NOT NULL DEFAULT 'null', recovery_json TEXT NOT NULL DEFAULT 'null',
           finished_at INTEGER, endpoint TEXT NOT NULL DEFAULT '', request_id TEXT NOT NULL DEFAULT '',
           usage_json TEXT NOT NULL DEFAULT '{}', images_json TEXT NOT NULL DEFAULT '[]',
           updated_at INTEGER NOT NULL DEFAULT 0, storage_version INTEGER NOT NULL DEFAULT 0,
           edit_mode INTEGER NOT NULL DEFAULT 0, reference_image_ids TEXT NOT NULL DEFAULT '',
           web_session_id TEXT NOT NULL DEFAULT '', web_session_deleted INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_image_tasks_created ON image_tasks(created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_image_tasks_status ON image_tasks(status);
          CREATE TABLE IF NOT EXISTS image_web_sessions (
           id TEXT PRIMARY KEY, account_id TEXT NOT NULL,
           local_conversation_id TEXT NOT NULL, conversation_id TEXT NOT NULL,
           parent_message_id TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL, task_ids TEXT NOT NULL DEFAULT ''
          );",
    )?;
    ensure_task_columns(&transaction)?;
    ensure_web_session_schema(&transaction)?;
    transaction.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_image_web_sessions_upstream
            ON image_web_sessions(account_id, conversation_id);
          CREATE INDEX IF NOT EXISTS idx_image_web_sessions_local
            ON image_web_sessions(account_id, local_conversation_id, updated_at DESC);
          CREATE INDEX IF NOT EXISTS idx_image_web_sessions_reuse
            ON image_web_sessions(account_id, updated_at DESC);",
    )?;
    ensure_asset_storage(&transaction)?;
    ensure_output_columns(&transaction)?;
    ensure_input_columns(&transaction)?;
    transaction.commit()?;
    Ok(connection)
}

fn web_conversation_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "accountId": row.get::<_, String>(1)?,
        "localConversationId": row.get::<_, String>(2)?,
        "conversationId": row.get::<_, String>(3)?,
        "parentMessageId": row.get::<_, String>(4)?,
        "createdAt": row.get::<_, i64>(5)?,
        "updatedAt": row.get::<_, i64>(6)?,
        "taskIds": row.get::<_, String>(7)?,
        "deleted": false
    }))
}

pub fn find_web_conversation(
    paths: &AppPaths,
    account_id: &str,
    conversation_id: &str,
) -> Result<Option<Value>, ManagerError> {
    let connection = open(paths)?;
    connection
        .query_row(
            "SELECT id, account_id, local_conversation_id, conversation_id,
                    parent_message_id, created_at, updated_at, task_ids
             FROM image_web_sessions
             WHERE account_id = ?1
               AND (?2 = '' OR conversation_id = ?2)
             ORDER BY updated_at DESC LIMIT 1",
            params![account_id, conversation_id],
            web_conversation_json,
        )
        .optional()
        .map_err(ManagerError::from)
}

pub fn save_web_conversation(paths: &AppPaths, conversation: &Value) -> Result<(), ManagerError> {
    let account_id = conversation["accountId"].as_str().unwrap_or("").trim();
    let local_conversation_id = conversation["localConversationId"]
        .as_str()
        .unwrap_or("")
        .trim();
    let conversation_id = conversation["conversationId"].as_str().unwrap_or("").trim();
    if account_id.is_empty() || conversation_id.is_empty() {
        return Err(ManagerError::System("Web 会话记录缺少账号或会话 ID".into()));
    }
    let now = chrono::Utc::now().timestamp_millis();
    let created_at = conversation["createdAt"].as_i64().unwrap_or(now);
    let updated_at = conversation["updatedAt"].as_i64().unwrap_or(now);
    let record_id = conversation["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    open(paths)?.execute(
        "INSERT INTO image_web_sessions
           (id, account_id, local_conversation_id, conversation_id, parent_message_id,
            created_at, updated_at, task_ids)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '')
         ON CONFLICT(account_id, conversation_id) DO UPDATE SET
           local_conversation_id = excluded.local_conversation_id,
           parent_message_id = CASE WHEN excluded.parent_message_id = ''
           THEN image_web_sessions.parent_message_id ELSE excluded.parent_message_id END,
           updated_at = excluded.updated_at",
        params![
            record_id,
            account_id,
            local_conversation_id,
            conversation_id,
            conversation["parentMessageId"].as_str().unwrap_or(""),
            created_at,
            updated_at
        ],
    )?;
    Ok(())
}

pub fn list_web_conversations(paths: &AppPaths) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let mut statement = connection.prepare(
        "SELECT id, account_id, local_conversation_id, conversation_id,
                parent_message_id, created_at, updated_at, task_ids
         FROM image_web_sessions ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = statement
        .query_map([], web_conversation_json)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!(rows))
}

pub fn read_web_conversations(
    paths: &AppPaths,
    ids: &[String],
) -> Result<Vec<Value>, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个 Web 会话".into()));
    }
    let connection = open(paths)?;
    let mut statement = connection.prepare(
        "SELECT id, account_id, local_conversation_id, conversation_id,
                parent_message_id, created_at, updated_at, task_ids
         FROM image_web_sessions WHERE id IN (SELECT value FROM json_each(?1))",
    )?;
    let rows = statement
        .query_map([serde_json::to_string(ids)?], web_conversation_json)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn mark_web_conversations_deleted(
    paths: &AppPaths,
    ids: &[String],
    deleted: bool,
) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个 Web 会话".into()));
    }
    let now = chrono::Utc::now().timestamp_millis();
    let connection = open(paths)?;
    let count = if deleted {
        let conversation_ids = {
            let mut statement = connection.prepare(
                "SELECT conversation_id FROM image_web_sessions
                 WHERE id IN (SELECT value FROM json_each(?1))",
            )?;
            let rows = statement
                .query_map([serde_json::to_string(ids)?], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let conversation_json = serde_json::to_string(&conversation_ids)?;
        connection.execute(
            "DELETE FROM image_web_sessions WHERE id IN (SELECT value FROM json_each(?1))",
            [serde_json::to_string(ids)?],
        )?;
        connection.execute(
            "UPDATE image_tasks SET web_session_deleted = 1,
             updated_at = ?2
             WHERE web_session_id IN (SELECT value FROM json_each(?1))",
            params![conversation_json, now],
        )?
    } else {
        0
    };
    Ok(json!({ "deleted": count, "restored": false }))
}

pub fn mark_web_conversation_deleted_by_upstream(
    paths: &AppPaths,
    account_id: &str,
    conversation_id: &str,
) -> Result<(), ManagerError> {
    let mut connection = open(paths)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "DELETE FROM image_web_sessions WHERE account_id = ?1 AND conversation_id = ?2",
        params![account_id, conversation_id],
    )?;
    transaction.execute(
        "UPDATE image_tasks SET web_session_deleted = 1,
         updated_at = ?3
         WHERE account_id = ?1 AND web_session_id = ?2",
        params![
            account_id,
            conversation_id,
            chrono::Utc::now().timestamp_millis()
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    // 重启后不能确认上游是否已消耗额度，不自动重放请求。
    let connection = open(paths)?;
    let compacted: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let thumbnail_compaction: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'app_metadata')",
        [], |row| row.get(0),
    )? && connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key = 'image_thumbnail_compaction_pending')",
        [], |row| row.get(0),
    )?;
    connection.execute(
        "UPDATE image_tasks SET status = 'interrupted', can_resume = 1,
         error_json = ?1, updated_at = ?2 WHERE status IN ('processing', 'queued')",
        params![
            serde_json::to_string(&json!({
                "message": "应用已退出，生成结果未知；请恢复后继续",
                "source": "lifecycle"
            }))?,
            chrono::Utc::now().timestamp_millis()
        ],
    )?;
    let _ = migrate_legacy_inputs(paths, &connection)?;
    let _ = migrate_legacy_outputs(paths, &connection)?;
    // 元数据合并和旧表删除一起提交，失败后仍可从旧表继续迁移。
    let transaction = connection.unchecked_transaction()?;
    migrate_legacy_assets(&transaction)?;
    transaction.commit()?;
    if compacted < 2 || thumbnail_compaction {
        if let Err(error) = connection.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE);
             VACUUM;
             PRAGMA user_version = 2;",
        ) {
            // 图片和元数据已经迁移成功；空间不足等原因只延后压缩，不阻止应用启动。
            eprintln!("图片迁移已完成，数据库空间回收将在下次启动重试：{error}");
            return Ok(());
        }
        if thumbnail_compaction {
            connection.execute(
                "DELETE FROM app_metadata WHERE key = 'image_thumbnail_compaction_pending'",
                [],
            )?;
        }
    }
    Ok(())
}

const MAX_TASK_BACKUP_BYTES: usize = 512 * 1024 * 1024;

// 任务备份包含恢复上下文、参考图和已生成图片，独立于通用配置备份保存。
pub fn export_task_bundle(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }

    let connection = open(paths)?;
    let mut task_statement = connection.prepare(&format!(
        "SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1"
    ))?;
    let mut tasks = Vec::new();
    let mut outputs = Vec::new();
    let mut input_ids = HashSet::new();
    let mut raw_bytes = 0usize;

    for id in ids {
        let Some(record) = task_statement
            .query_row([id], task_record_from_row)
            .optional()?
        else {
            continue;
        };
        let task = record.value()?;
        let task_id = record.meta.id.clone();
        let created_at = record.meta.created_at;
        let status = record.meta.status.clone();
        let task_json = serde_json::to_string(&task)?;

        raw_bytes = raw_bytes.saturating_add(task_json.len());
        if raw_bytes > MAX_TASK_BACKUP_BYTES {
            return Err(ManagerError::System(
                "任务备份超过 512 MB，请分批导出".into(),
            ));
        }
        let payload = task.clone();
        let input_id = if record.meta.input_id.is_empty() {
            task_id.clone()
        } else {
            record.meta.input_id.clone()
        };
        input_ids.insert(input_id.to_string());
        tasks.push(json!({
            "id": task_id,
            "createdAt": created_at,
            "status": status,
            "payload": payload
        }));

        let group_id = output_group_id(&task);
        for (position, image_id, format) in task_image_records(&connection, id)? {
            let bytes = read_output_file(paths, &group_id, &task_id, &image_id, &format)?;
            raw_bytes = raw_bytes.saturating_add(bytes.len());
            if raw_bytes > MAX_TASK_BACKUP_BYTES {
                return Err(ManagerError::System(
                    "任务备份超过 512 MB，请分批导出".into(),
                ));
            }
            outputs.push(json!({
                "taskId": id,
                "position": position,
                "imageId": image_id,
                "format": format,
                "data": STANDARD.encode(bytes)
            }));
        }
    }

    if tasks.is_empty() {
        return Err(ManagerError::System("所选任务不存在或已被删除".into()));
    }

    let mut inputs = Vec::new();
    for input_id in input_ids {
        let mut items = Vec::new();
        let input_task: Option<(String, String, i64)> = connection.query_row(
            "SELECT reference_image_ids, input_id, has_mask FROM image_tasks WHERE input_id = ?1 LIMIT 1",
            [&input_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        ).optional()?;
        let ids_for_input = input_task
            .map(|(ids, _, _)| {
                ids.split(',')
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (position, image_id) in ids_for_input.into_iter().enumerate() {
            let (format, path) = find_input_file(paths, &input_id, &image_id)?;
            let bytes = fs::read(path)?;
            let data_url = if format == "url" {
                String::from_utf8(bytes)
                    .map_err(|_| ManagerError::System("引用图片数据损坏".into()))?
            } else {
                format!("data:image/{};base64,{}", format, STANDARD.encode(bytes))
            };
            raw_bytes = raw_bytes.saturating_add(data_url.len());
            if raw_bytes > MAX_TASK_BACKUP_BYTES {
                return Err(ManagerError::System(
                    "任务备份超过 512 MB，请分批导出".into(),
                ));
            }
            items.push(json!({ "position": position, "imageId": image_id, "format": format, "dataUrl": data_url }));
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
    let mut output_guard = OutputFileGuard::new(paths);
    let mut input_guard = InputFileGuard::new(paths);
    let mut stale_output_files = Vec::new();

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
                    payload["batchKey"] = json!(format!("{}:{}", target_conversation_id, round_id));
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
        let created_at = entry["createdAt"]
            .as_i64()
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let existed = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM image_tasks WHERE id = ?1)",
            [id],
            |row| row.get::<_, bool>(0),
        )?;
        let mut meta = task_meta_from_value(&payload, created_at, Some(&imported_status));
        meta.id = id.to_string();
        meta.status = imported_status.clone();
        meta.updated_at = chrono::Utc::now().timestamp_millis();
        upsert_task(&transaction, &meta)?;
        if existed {
            updated += 1;
        } else {
            imported += 1;
        }
        let old_json: String = transaction.query_row(
            "SELECT images_json FROM image_tasks WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        let old_files = image_records_from_json(&old_json)
            .into_iter()
            .map(|(_, image_id, format)| (image_id, format))
            .collect::<Vec<_>>();
        let group_id = output_group_id(&payload);
        for (image_id, format) in old_files {
            stale_output_files.push(output_relative_path(&group_id, id, &image_id, &format));
        }
        transaction.execute(
            "UPDATE image_tasks SET images_json = '[]', image_count = 0 WHERE id = ?1",
            [id],
        )?;
    }

    if let Some(inputs) = bundle["inputs"].as_array() {
        for input in inputs {
            let input_id = input["id"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ManagerError::System("任务备份包含无效输入 ID".into()))?;
            let mut imported_ids = Vec::new();
            let mut has_mask = false;
            for item in input["items"].as_array().cloned().unwrap_or_default() {
                let position = item["position"]
                    .as_i64()
                    .ok_or_else(|| ManagerError::System("任务备份包含无效输入位置".into()))?;
                let data_url = item["dataUrl"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| ManagerError::System("任务备份包含空输入图片".into()))?;
                let image_id = item["imageId"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let (format, relative) = write_input_file(paths, input_id, &image_id, data_url)?;
                input_guard.files.push(relative);
                if position < 0 {
                    has_mask = true;
                } else {
                    imported_ids.push(image_id);
                }
                let _ = format;
            }
            let joined = imported_ids.join(",");
            let owner_ids = transaction
                .prepare("SELECT id FROM image_tasks WHERE input_id = ?1")?
                .query_map([input_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for owner_id in owner_ids {
                transaction.execute(
                    "UPDATE image_tasks SET reference_image_ids = ?2, input_count = ?3, has_mask = ?4 WHERE id = ?1",
                    params![owner_id, joined, imported_ids.len() as i64, has_mask as i64],
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
            let image_id = output["imageId"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
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
            let task_payload = transaction
                .query_row(
                    &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1"),
                    [task_id],
                    task_record_from_row,
                )?
                .value()?;
            let file_path = write_output_file(
                paths,
                output_group_id(&task_payload).as_str(),
                task_id,
                &image_id,
                format,
                &bytes,
            )?;
            output_guard.files.push(file_path.clone());
            let current_json: String = transaction.query_row(
                "SELECT images_json FROM image_tasks WHERE id = ?1",
                [task_id],
                |row| row.get(0),
            )?;
            let mut records = image_records_from_json(&current_json)
                .into_iter()
                .filter(|(old_position, _, _)| *old_position != position.max(0) as usize)
                .map(|(old_position, old_id, old_format)| (old_position, old_id, old_format, None))
                .collect::<Vec<_>>();
            records.push((position.max(0) as usize, image_id, format.to_string(), None));
            update_task_images(&transaction, task_id, &records)?;
        }
    }

    transaction.commit()?;
    output_guard.committed = true;
    input_guard.committed = true;
    let created_files = output_guard.files.iter().collect::<HashSet<_>>();
    for file_path in stale_output_files {
        if !created_files.contains(&file_path) {
            remove_output_files(paths, &[file_path]);
        }
    }
    Ok(json!({
        "imported": imported,
        "updated": updated,
        "resumable": resumable
    }))
}

// 一轮任务原子入队，共享输入图片，避免 100 张生成重复保存 100 份参考图。
pub fn create_batch(
    paths: &AppPaths,
    tasks: &[Value],
    images: &[String],
    mask: &str,
) -> Result<(), ManagerError> {
    if tasks.is_empty() || tasks.len() > 10000 {
        return Err(ManagerError::System("一轮可提交 1–10000 个任务".into()));
    }
    let mut connection = open(paths)?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let input_id = tasks[0]["inputId"]
        .as_str()
        .filter(|value| !value.is_empty())
        .or_else(|| tasks[0]["id"].as_str())
        .unwrap_or("input");
    let mut input_image_ids = Vec::with_capacity(images.len() + usize::from(!mask.is_empty()));
    let mut input_files = InputFileGuard::new(paths);
    for data_url in images {
        let image_id = uuid::Uuid::new_v4().to_string();
        let (format, relative) = write_input_file(paths, input_id, &image_id, data_url)?;
        input_files.files.push(relative);
        let _ = format;
        input_image_ids.push(image_id);
    }
    if !mask.is_empty() {
        let image_id = uuid::Uuid::new_v4().to_string();
        let (format, relative) = write_input_file(paths, input_id, &image_id, mask)?;
        input_files.files.push(relative);
        let _ = format;
        input_image_ids.push(image_id);
    }
    let reference_image_ids = Value::Array(input_image_ids.iter().map(|id| json!(id)).collect());
    for task in tasks {
        let mut stored_task = task.clone();
        stored_task["referenceImageIds"] = reference_image_ids.clone();
        stored_task["editMode"] = json!(matches!(
            stored_task["request"]["mode"]
                .as_str()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "edit" | "image-to-image" | "image_to_image" | "img2img" | "img-to-img"
        ));
        let mut meta = task_meta_from_value(
            &stored_task,
            chrono::Utc::now().timestamp_millis(),
            Some("queued"),
        );
        meta.status = "queued".into();
        meta.input_id = input_id.to_string();
        meta.input_count = images.len() as i64;
        meta.has_mask = !mask.is_empty();
        upsert_task(&transaction, &meta)?;
    }
    transaction.commit()?;
    input_files.committed = true;
    Ok(())
}

pub fn start(paths: &AppPaths, task: &Value) -> Result<Option<Value>, ManagerError> {
    let connection = open(paths)?;
    let query = format!(
        "UPDATE image_tasks SET status = 'processing', revision = revision + 1, updated_at = ?3
         WHERE id = ?1 AND status = 'queued' AND revision = ?2 RETURNING {TASK_SELECT}"
    );
    connection
        .query_row(
            &query,
            params![
                task["id"].as_str(),
                task["revision"].as_u64().unwrap_or(0),
                chrono::Utc::now().timestamp_millis()
            ],
            task_record_from_row,
        )
        .optional()?
        .map(|record| record.value())
        .transpose()
}

pub fn checkpoint(
    paths: &AppPaths,
    id: &str,
    revision: u64,
    recovery: &Value,
) -> Result<(), ManagerError> {
    open(paths)?.execute(
        "UPDATE image_tasks SET recovery_json = ?2, can_resume = 1,
         web_session_id = COALESCE(NULLIF(json_extract(?2, '$.conversationId'), ''), web_session_id),
         web_session_deleted = 0, updated_at = ?4
         WHERE id = ?1 AND status = 'processing' AND revision = ?3",
        params![id, serde_json::to_string(recovery)?, revision, chrono::Utc::now().timestamp_millis()],
    )?;
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
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let record: TaskRecord = transaction.query_row(
        &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1 AND status IN ('failed', 'interrupted')"),
        [id],
        task_record_from_row,
    )?;
    let status = record.meta.status.clone();
    let mut task = record.value()?;
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
    let mut meta = task_meta_from_value(&task, record.meta.created_at, Some("queued"));
    meta.id = id.to_string();
    meta.status = "queued".into();
    meta.updated_at = chrono::Utc::now().timestamp_millis();
    upsert_task(&transaction, &meta)?;
    transaction.commit()?;
    Ok(task)
}

// 历史索引只返回小型元数据；缩略图和原图仍按当前会话按需读取。
pub fn history(paths: &AppPaths) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let mut statement = connection.prepare("SELECT id, created_at, status, conversation_id, round_id, prompt, batch_name, revision FROM image_tasks ORDER BY created_at")?;
    let rows = statement.query_map([], |row| Ok(json!({
        "id": row.get::<_, String>(0)?, "createdAt": row.get::<_, i64>(1)?, "status": row.get::<_, String>(2)?,
        "conversationId": row.get::<_, String>(3)?, "roundId": row.get::<_, String>(4)?, "prompt": row.get::<_, String>(5)?, "batchName": row.get::<_, String>(6)?, "revision": row.get::<_, u64>(7)?
    })))?.collect::<Result<Vec<_>, _>>()?;
    Ok(json!(rows))
}

pub fn finish(
    paths: &AppPaths,
    task: &Value,
    images: &[StoredImage],
) -> Result<Option<Value>, ManagerError> {
    let mut created_files = Vec::new();
    let result = (|| {
        let mut connection = open(paths)?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut task = task.clone();
        let saved: Option<TaskRecord> = transaction
            .query_row(
                &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1"),
                [task["id"].as_str()],
                task_record_from_row,
            )
            .optional()?;
        let Some(saved) = saved else { return Ok(None) };
        let saved_task = saved.value()?;
        // 取消或恢复已经改变版本，旧请求既不能写入图片，也不能发布完成状态。
        if saved_task["status"] != "processing" || saved_task["revision"] != task["revision"] {
            return Ok(None);
        }
        task["recovery"] = saved_task["recovery"].clone();
        task["canResume"] =
            json!(task["status"] == "failed" && task["recovery"]["conversationId"].is_string());
        task["revision"] = json!(saved_task["revision"].as_u64().unwrap_or(0) + 1);
        let mut image_records = Vec::with_capacity(images.len());
        for (index, image) in images.iter().enumerate() {
            let task_id = task["id"].as_str().unwrap_or("image");
            let image_id = uuid::Uuid::new_v4().to_string();
            write_output_file(
                paths,
                output_group_id(&task).as_str(),
                task_id,
                &image_id,
                &image.format,
                &image.bytes,
            )?;
            created_files.push(output_relative_path(
                output_group_id(&task).as_str(),
                task_id,
                &image_id,
                &image.format,
            ));
            image_records.push(json!({
                "imageId": image_id,
                "format": image.format,
                "size": format!("{}x{}", image.width, image.height),
                "revisedPrompt": image.revised_prompt,
                "position": index
            }));
        }
        task["images"] = json!(image_records);
        let mut meta = task_meta_from_value(&task, saved.meta.created_at, None);
        meta.id = saved.meta.id.clone();
        meta.status = task["status"].as_str().unwrap_or("failed").to_string();
        meta.revision = saved.meta.revision + 1;
        meta.updated_at = chrono::Utc::now().timestamp_millis();
        upsert_task(&transaction, &meta)?;
        transaction.commit()?;
        task.as_object_mut().unwrap().remove("thumbnail");
        Ok(Some(task))
    })();
    if result.is_err()
        || result
            .as_ref()
            .map(|value| value.is_none())
            .unwrap_or(false)
    {
        remove_output_files(paths, &created_files);
    }
    result
}

// 等待批次只读取小型状态快照，不重复传输任务详情和图片。
pub fn task_states(paths: &AppPaths, ids: &[String]) -> Result<Vec<Value>, ManagerError> {
    let connection = open(paths)?;
    let mut statement = connection.prepare(
        "SELECT id, status, revision, image_count, error_json
         FROM image_tasks WHERE id IN (SELECT value FROM json_each(?1))",
    )?;
    let rows = statement.query_map([serde_json::to_string(ids)?], |row| {
        let error = row.get::<_, Option<String>>(4)?.map(|raw| serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw)));
        Ok(json!({ "id": row.get::<_, String>(0)?, "status": row.get::<_, String>(1)?, "revision": row.get::<_, u64>(2)?, "imageCount": row.get::<_, u64>(3)?, "error": error }))
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn previews(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(ManagerError::System("请选取 1–100 个任务".into()));
    }
    let connection = open(paths)?;
    let mut statement = connection.prepare(&format!(
        "SELECT {TASK_SELECT} FROM image_tasks WHERE id IN (SELECT value FROM json_each(?1))"
    ))?;
    let rows = statement
        .query_map([serde_json::to_string(ids)?], task_record_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    drop(connection);
    Ok(json!(rows
        .into_iter()
        .map(|record| json!({
            "id": record.meta.id,
            "thumbnail": task_thumbnail(paths, &record.meta).unwrap_or_default()
        }))
        .collect::<Vec<_>>()))
}

pub fn list(paths: &AppPaths, payload: &Value) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let status = payload["status"].as_str().unwrap_or("");
    let page = payload["page"].as_u64().unwrap_or(1).clamp(1, 1_000_000);
    let conversation_id = payload["conversationId"].as_str().unwrap_or("");
    let page_size = payload["pageSize"].as_u64().unwrap_or(12).clamp(1, 100);
    let grouped = payload["groupByRound"] == true;
    let include_thumbnails = payload["includeThumbnails"] != false;
    let filter = "(?1 = '' OR status = ?1) AND (?2 = '' OR conversation_id = ?2)";
    let round_key = "COALESCE(NULLIF(round_id, ''), id)";
    let count = if grouped {
        format!("COUNT(DISTINCT {round_key})")
    } else {
        "COUNT(*)".to_string()
    };
    let total: u64 = connection.query_row(
        &format!("SELECT {count} FROM image_tasks WHERE {filter}"),
        params![status, conversation_id],
        |row| row.get(0),
    )?;
    // 按轮分页，100 张图片始终一起展示，不在任务分页边界拆开。
    let query = if grouped {
        format!("WITH filtered AS (SELECT *, {round_key} AS round_key FROM image_tasks WHERE {filter}),
            page_rounds AS (SELECT round_key FROM filtered GROUP BY round_key ORDER BY MAX(created_at) DESC, round_key DESC LIMIT ?3 OFFSET ?4)
            SELECT {TASK_SELECT} FROM filtered WHERE round_key IN (SELECT round_key FROM page_rounds)
            ORDER BY created_at DESC, batch_index ASC, id DESC")
    } else {
        format!("SELECT {TASK_SELECT} FROM image_tasks WHERE {filter} ORDER BY created_at DESC, id DESC LIMIT ?3 OFFSET ?4")
    };
    let mut statement = connection.prepare(&query)?;
    let rows = statement
        .query_map(
            params![status, conversation_id, page_size, (page - 1) * page_size],
            task_record_from_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    drop(connection);
    let items = rows
        .into_iter()
        .map(|record| {
            let mut task = record.value()?;
            task["thumbnail"] = if include_thumbnails {
                json!(task_thumbnail(paths, &record.meta).unwrap_or_default())
            } else {
                json!("")
            };
            Ok(task)
        })
        .collect::<Result<Vec<Value>, ManagerError>>()?;
    Ok(json!({ "items": items, "total": total, "pageSize": page_size }))
}

pub fn detail(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let connection = open(paths)?;
    let mut task = connection
        .query_row(
            &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1"),
            [id],
            task_record_from_row,
        )
        .optional()?
        .ok_or_else(|| ManagerError::System("图片任务不存在".into()))?
        .value()?;
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
    let (input_id, reference_ids, has_mask): (String, String, bool) = connection.query_row(
        "SELECT COALESCE(NULLIF(input_id, ''), id), reference_image_ids, has_mask FROM image_tasks WHERE id = ?1",
        [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    )?;
    let ids = reference_ids
        .split(',')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let mut images = Vec::new();
    let mut mask = String::new();
    for (index, image_id) in ids.iter().enumerate() {
        let (format, path) = find_input_file(paths, &input_id, image_id)?;
        let bytes = fs::read(path)?;
        let url = if format == "url" {
            String::from_utf8(bytes).map_err(|_| ManagerError::System("引用图片数据损坏".into()))?
        } else {
            format!("data:image/{};base64,{}", format, STANDARD.encode(bytes))
        };
        if has_mask && index + 1 == ids.len() {
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
    for id in ids {
        let Some(task) = connection
            .query_row(
                &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1"),
                [id],
                task_record_from_row,
            )
            .optional()?
        else {
            continue;
        };
        let group_id = output_group_id(&task.value()?);
        for (position, image_id, format) in task_image_records(&connection, id)? {
            let bytes = read_output_file(paths, &group_id, id, &image_id, &format)?;
            total_bytes += bytes.len();
            if total_bytes > 200 * 1024 * 1024 {
                return Err(ManagerError::System(
                    "图片总大小超过 200 MB，请分批导出".into(),
                ));
            }
            output.push((id.clone(), position, format, bytes));
        }
    }
    Ok(output)
}

pub fn export_image_count(paths: &AppPaths, ids: &[String]) -> Result<usize, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }
    let connection = open(paths)?;
    Ok(ids
        .iter()
        .map(|id| task_image_records(&connection, id).map(|records| records.len()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum())
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
    let mut task_statement = connection.prepare(
        "SELECT batch_name, batch_index
         FROM image_tasks WHERE id = ?1",
    )?;
    let file = File::create(target)?;
    let mut archive = zip::ZipWriter::new(file);
    // PNG、JPEG、WebP 已经压缩过，直接存储可避免再次 Deflate 消耗 CPU。
    let file_options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut entry_names = HashSet::new();
    let mut count = 0;
    for id in ids {
        let (batch_name, batch_index): (String, i64) = task_statement
            .query_row([id], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?
            .unwrap_or_default();
        let stem = sanitize_export_stem(&batch_name);
        let id_prefix = id.chars().take(8).collect::<String>();
        let task: TaskRecord = connection.query_row(
            &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1"),
            [id],
            task_record_from_row,
        )?;
        let group_id = output_group_id(&task.value()?);
        for (index, image_id, format) in task_image_records(&connection, id)? {
            let bytes = read_output_file(paths, &group_id, id, &image_id, &format)?;
            let mut file_name = if !batch_name.trim().is_empty() {
                {
                    format!(
                        "{stem}-{:03}-image-{:03}.{}",
                        batch_index.saturating_add(1),
                        index + 1,
                        format
                    )
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

pub fn write_export_zip(
    paths: &AppPaths,
    ids: &[String],
    target: &str,
) -> Result<usize, ManagerError> {
    write_export_zip_with_progress(paths, ids, target, |_| {})
}

fn output_files_for_tasks(
    connection: &Connection,
    ids: &[String],
) -> Result<Vec<String>, ManagerError> {
    let mut files = Vec::new();
    for task_id in ids {
        let Some(record) = connection
            .query_row(
                &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = ?1"),
                [task_id],
                task_record_from_row,
            )
            .optional()?
        else {
            continue;
        };
        let group_id = output_group_id(&record.value()?);
        for (_, image_id, format) in task_image_records(connection, task_id)? {
            files.push(output_relative_path(&group_id, task_id, &image_id, &format));
        }
    }
    Ok(files)
}

pub fn delete(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }
    let mut connection = open(paths)?;
    let output_files = output_files_for_tasks(&connection, ids)?;
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
        transaction.execute("DELETE FROM image_tasks WHERE id = ?1", [id])?;
        // 图片 ID 只存在任务字段；删除任务后文件由下方统一清理。
    }
    transaction.commit()?;
    remove_output_files(paths, &output_files);
    Ok(json!({ "deleted": ids.len() }))
}

pub fn cancel(paths: &AppPaths, ids: &[String], recoverable: bool) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选取 1–10000 个任务".into()));
    }
    let mut connection = open(paths)?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let ids_json = serde_json::to_string(ids)?;
    let error_message = if recoverable {
        "批量任务已暂停"
    } else {
        "任务已由用户取消"
    };
    let error_json = serde_json::to_string(&json!({
        "source": "user",
        "message": error_message
    }))?;
    let items = {
        let mut statement = transaction.prepare(&format!(
            "UPDATE image_tasks SET status = 'interrupted', can_resume = ?2,
             error_json = ?3, finished_at = ?4, revision = revision + 1, updated_at = ?4
             WHERE id IN (SELECT value FROM json_each(?1)) AND status IN ('queued', 'processing')
             RETURNING {TASK_SELECT}"
        ))?;
        let rows = statement.query_map(
            params![
                ids_json,
                if recoverable { 1 } else { 0 },
                error_json,
                chrono::Utc::now().timestamp_millis()
            ],
            task_record_from_row,
        )?;
        let records = rows.collect::<Result<Vec<_>, _>>()?;
        records
            .into_iter()
            .map(|record| record.value())
            .collect::<Result<Vec<_>, _>>()?
    };
    transaction.commit()?;
    Ok(
        json!({ "cancelled": items.len(), "ids": items.iter().map(|task| &task["id"]).collect::<Vec<_>>(), "items": items }),
    )
}

pub fn clear_results(paths: &AppPaths, ids: &[String]) -> Result<Value, ManagerError> {
    if ids.is_empty() || ids.len() > 10000 {
        return Err(ManagerError::System("请选择要删除结果的任务".into()));
    }
    let mut connection = open(paths)?;
    let output_files = output_files_for_tasks(&connection, ids)?;
    let transaction = connection.transaction()?;
    for id in ids {
        let status: String = transaction.query_row(
            "SELECT status FROM image_tasks WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        if matches!(status.as_str(), "processing" | "queued") {
            return Err(ManagerError::System("请等待本轮任务完成后删除结果".into()));
        }
        transaction.execute(
            "UPDATE image_tasks SET image_count = 0, images_json = '[]',
             updated_at = ?2 WHERE id = ?1",
            params![id, chrono::Utc::now().timestamp_millis()],
        )?;
    }
    transaction.commit()?;
    remove_output_files(paths, &output_files);
    Ok(json!({"deleted": ids.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::resolve_app_paths;
    use std::path::Path;

    #[test]
    fn open_waits_for_concurrent_schema_writer() {
        let root =
            std::env::temp_dir().join(format!("image-schema-lock-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let mut connection = open(&paths).unwrap();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = open(&paths).map(drop).map_err(|error| error.to_string());
            sender.send(result).unwrap();
        });
        // 另一连接持有写锁时应等待，不能在读取结构后升级写锁并立即报忙。
        let pending = receiver.recv_timeout(std::time::Duration::from_millis(250));
        transaction.commit().unwrap();
        let (waited, result) = match pending {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (
                true,
                receiver
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
            ),
            Ok(result) => (false, result),
            Err(error) => panic!("存储连接线程异常：{error}"),
        };
        worker.join().unwrap();
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
        assert!(waited, "结构初始化没有等待并发写入：{result:?}");
        assert!(result.is_ok(), "写锁释放后应正常打开：{result:?}");
    }

    fn insert_output(paths: &AppPaths, task_id: &str, position: i64, format: &str, bytes: &[u8]) {
        let connection = open(paths).unwrap();
        let task: Value = connection
            .query_row(
                "SELECT conversation_id FROM image_tasks WHERE id = ?1",
                [task_id],
                |row| {
                    let conversation_id: String = row.get(0)?;
                    Ok(json!({ "conversationId": conversation_id }))
                },
            )
            .unwrap();
        let group_id = output_group_id(&task);
        let image_id = uuid::Uuid::new_v4().to_string();
        write_output_file(paths, &group_id, task_id, &image_id, format, bytes).unwrap();
        let current: String = connection
            .query_row(
                "SELECT images_json FROM image_tasks WHERE id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .unwrap();
        let mut records = image_records_from_json(&current).into_iter()
            .filter(|(old_position, _, _)| *old_position != position.max(0) as usize)
            .map(|(old_position, old_id, old_format)| json!({ "position": old_position, "imageId": old_id, "format": old_format }))
            .collect::<Vec<_>>();
        records.push(json!({ "position": position.max(0), "imageId": image_id, "format": format }));
        let image_count = records.len() as i64;
        connection
            .execute(
                "UPDATE image_tasks SET images_json = ?2, image_count = ?3 WHERE id = ?1",
                params![task_id, Value::Array(records).to_string(), image_count],
            )
            .unwrap();
    }

    #[test]
    fn web_conversation_records_keep_reuse_and_deleted_state() {
        let root =
            std::env::temp_dir().join(format!("image-web-session-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        save_web_conversation(
            &paths,
            &json!({
                "accountId": "account",
                "localConversationId": "workbench",
                "conversationId": "upstream",
                "parentMessageId": "message-1"
            }),
        )
        .unwrap();
        let saved = find_web_conversation(&paths, "account", "")
            .unwrap()
            .unwrap();
        assert_eq!(saved["conversationId"], "upstream");
        assert_eq!(saved["deleted"], false);
        assert_eq!(
            list_web_conversations(&paths)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(find_web_conversation(&paths, "other-account", "")
            .unwrap()
            .is_none());

        // 切换本地历史分组仍使用同一个上游记录，早期 SSE 检查点不能清空父消息。
        save_web_conversation(
            &paths,
            &json!({
                "accountId": "account", "localConversationId": "another-workbench",
                "conversationId": "upstream", "parentMessageId": ""
            }),
        )
        .unwrap();
        let reused = find_web_conversation(&paths, "account", "")
            .unwrap()
            .unwrap();
        assert_eq!(reused["id"], saved["id"]);
        assert_eq!(reused["parentMessageId"], "message-1");

        let record_id = saved["id"].as_str().unwrap().to_string();
        mark_web_conversations_deleted(&paths, &[record_id.clone()], true).unwrap();
        assert!(find_web_conversation(&paths, "account", "")
            .unwrap()
            .is_none());
        assert!(list_web_conversations(&paths)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        assert!(find_web_conversation(&paths, "account", "upstream")
            .unwrap()
            .is_none());
        // 删除后重新收到同一上游会话，只能创建新的本地记录。
        save_web_conversation(&paths, &json!({
            "accountId": "account", "conversationId": "upstream", "parentMessageId": "late-message"
        })).unwrap();
        assert_eq!(
            find_web_conversation(&paths, "account", "")
                .unwrap()
                .unwrap()["parentMessageId"],
            "late-message"
        );
        save_web_conversation(&paths, &json!({
            "accountId": "account", "conversationId": "new-upstream", "parentMessageId": "new-message"
        })).unwrap();
        assert_eq!(
            find_web_conversation(&paths, "account", "")
                .unwrap()
                .unwrap()["conversationId"],
            "new-upstream"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_task_payload_is_migrated_and_removed() {
        let root = std::env::temp_dir().join(format!(
            "image-task-payload-migrate-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = resolve_app_paths(&root);
        let connection = database::open(&paths).unwrap();
        connection.execute_batch(
            "CREATE TABLE image_tasks (
               id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, status TEXT NOT NULL,
               payload_json TEXT NOT NULL, thumbnail TEXT NOT NULL DEFAULT ''
             );
             INSERT INTO image_tasks(id, created_at, status, payload_json) VALUES
               ('legacy', 1, 'queued', '{\"request\":{\"conversationId\":\"c\",\"prompt\":\"p\",\"n\":2},\"imageCount\":0}');"
        ).unwrap();
        drop(connection);
        let connection = open(&paths).unwrap();
        let columns = {
            let mut statement = connection
                .prepare("PRAGMA table_info(image_tasks)")
                .unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(!columns.iter().any(|column| column == "payload_json"));
        assert!(!columns.iter().any(|column| column == "thumbnail"));
        let row: (String, String, i64) = connection.query_row(
            "SELECT conversation_id, prompt, requested_count FROM image_tasks WHERE id = 'legacy'",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        ).unwrap();
        assert_eq!(row, ("c".into(), "p".into(), 2));
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_migrates_legacy_web_session_payload_before_creating_indexes() {
        let root =
            std::env::temp_dir().join(format!("image-web-schema-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let connection = database::open(&paths).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE image_web_sessions(
                   local_id TEXT NOT NULL, account_id TEXT NOT NULL,
                   payload_json TEXT NOT NULL,
                   PRIMARY KEY(local_id, account_id)
                 );
                 INSERT INTO image_web_sessions(local_id, account_id, payload_json)
                 VALUES ('local', 'account',
                   '{\"conversationId\":\"upstream\",\"parentMessageId\":\"parent\"}');
                 CREATE TABLE image_web_conversations(
                   id TEXT PRIMARY KEY, account_id TEXT NOT NULL,
                   local_conversation_id TEXT NOT NULL, conversation_id TEXT NOT NULL,
                   parent_message_id TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL, deleted_at INTEGER,
                   is_deleted INTEGER NOT NULL DEFAULT 0
                 );",
            )
            .unwrap();
        drop(connection);

        let connection = open(&paths).unwrap();
        let session: (String, String, String) = connection
            .query_row(
                "SELECT account_id, local_conversation_id, conversation_id
                 FROM image_web_sessions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            session,
            ("account".into(), "local".into(), "upstream".into())
        );
        let old_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE name IN ('image_web_conversations', 'image_web_sessions_legacy')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_count, 0);
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelled_and_old_attempts_cannot_commit_or_overwrite_recovery() {
        let root = std::env::temp_dir().join(format!("image-cancel-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let queued = json!({ "id": "task", "createdAt": 1, "status": "queued", "request": { "generationMode": "web" }, "imageCount": 0 });
        create_batch(&paths, std::slice::from_ref(&queued), &[], "").unwrap();
        let mut old = start(&paths, &queued).unwrap().unwrap();
        checkpoint(
            &paths,
            "task",
            old["revision"].as_u64().unwrap(),
            &json!({"conversationId": "old"}),
        )
        .unwrap();
        let cancelled = cancel(&paths, &["task".into()], true).unwrap();
        assert_eq!(cancelled["cancelled"], 1);
        assert_eq!(cancelled["items"][0]["status"], "interrupted");
        assert_eq!(
            cancel(&paths, &["task".into()], true).unwrap()["ids"],
            json!([])
        );
        let resumed = requeue(&paths, "task").unwrap();
        assert!(start(&paths, &queued).unwrap().is_none());
        let mut current = start(&paths, &resumed).unwrap().unwrap();
        checkpoint(
            &paths,
            "task",
            current["revision"].as_u64().unwrap(),
            &json!({"conversationId": "new"}),
        )
        .unwrap();
        checkpoint(
            &paths,
            "task",
            old["revision"].as_u64().unwrap(),
            &json!({"conversationId": "stale"}),
        )
        .unwrap();
        old["status"] = json!("completed");
        let image = StoredImage {
            bytes: vec![1],
            format: "png".into(),
            width: 1,
            height: 1,
            revised_prompt: String::new(),
        };
        assert!(finish(&paths, &old, std::slice::from_ref(&image))
            .unwrap()
            .is_none());
        assert!(read_images(&paths, &["task".into()]).unwrap().is_empty());
        current["status"] = json!("failed");
        let saved = finish(&paths, &current, &[]).unwrap().unwrap();
        assert_eq!(saved["canResume"], true);
        assert_eq!(saved["recovery"]["conversationId"], "new");
        assert_eq!(saved["revision"], current["revision"].as_u64().unwrap() + 1);
        assert_eq!(
            cancel(&paths, &["task".into()], false).unwrap()["cancelled"],
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn previews_are_generated_from_original_files_without_persisting_thumbnails() {
        let root = std::env::temp_dir().join(format!("image-save-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let queued = json!({ "id": "task", "createdAt": 1, "status": "queued", "imageCount": 0 });
        create_batch(&paths, std::slice::from_ref(&queued), &[], "").unwrap();
        let mut task = start(&paths, &queued).unwrap().unwrap();
        task["status"] = json!("completed");
        task["imageCount"] = json!(1);
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(640, 480)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let image = StoredImage {
            bytes: bytes.into_inner(),
            format: "png".into(),
            width: 640,
            height: 480,
            revised_prompt: String::new(),
        };
        let saved = finish(&paths, &task, std::slice::from_ref(&image))
            .unwrap()
            .unwrap();
        assert!(saved.get("thumbnail").is_none());
        assert_eq!(
            task_states(&paths, &["task".into()]).unwrap()[0]["status"],
            "completed"
        );
        assert_eq!(read_images(&paths, &["task".into()]).unwrap().len(), 1);
        assert_eq!(
            list(&paths, &json!({ "includeThumbnails": false })).unwrap()["items"][0]["thumbnail"],
            ""
        );
        let preview = previews(&paths, &["task".into()]).unwrap()[0]["thumbnail"]
            .as_str()
            .unwrap()
            .to_string();
        let decoded = STANDARD
            .decode(preview.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        let thumbnail = image::load_from_memory(&decoded).unwrap();
        assert_eq!((thumbnail.width(), thumbnail.height()), (320, 240));
        assert_eq!(
            list(&paths, &json!({})).unwrap()["items"][0]["thumbnail"],
            preview
        );
        let connection = open(&paths).unwrap();
        assert_eq!(connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('image_tasks') WHERE name = 'thumbnail'",
            [], |row| row.get::<_, i64>(0),
        ).unwrap(), 0);
        let bundle = export_task_bundle(&paths, &["task".into()]).unwrap();
        assert!(bundle["tasks"][0].get("thumbnail").is_none());
        assert!(bundle["tasks"][0]["payload"].get("thumbnail").is_none());
        assert_eq!(
            fs::read_dir(output_root(&paths).join("legacy"))
                .unwrap()
                .count(),
            1
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initialize_removes_existing_thumbnail_column_and_keeps_originals() {
        let root =
            std::env::temp_dir().join(format!("image-thumbnail-migrate-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let task = json!({ "id": "old-task", "request": { "conversationId": "old-conversation", "prompt": "原有提示词" } });
        create_batch(&paths, &[task], &[], "").unwrap();
        insert_output(&paths, "old-task", 0, "png", &[1, 2, 3]);
        let connection = open(&paths).unwrap();
        connection
            .execute_batch(
                "ALTER TABLE image_tasks ADD COLUMN thumbnail TEXT NOT NULL DEFAULT '';
             UPDATE image_tasks SET status = 'completed', thumbnail = 'data:image/png;base64,b2xk';
             PRAGMA user_version = 2;",
            )
            .unwrap();
        drop(connection);

        initialize(&paths).unwrap();
        initialize(&paths).unwrap();
        let connection = open(&paths).unwrap();
        assert_eq!(connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('image_tasks') WHERE name = 'thumbnail'",
            [], |row| row.get::<_, i64>(0),
        ).unwrap(), 0);
        let saved = connection
            .query_row(
                &format!("SELECT {TASK_SELECT} FROM image_tasks WHERE id = 'old-task'"),
                [],
                task_record_from_row,
            )
            .unwrap()
            .value()
            .unwrap();
        assert_eq!(saved["request"]["conversationId"], "old-conversation");
        assert_eq!(saved["request"]["prompt"], "原有提示词");
        assert_eq!(saved["status"], "completed");
        assert_eq!(saved["imageCount"], 1);
        assert_eq!(
            read_images(&paths, &["old-task".into()]).unwrap()[0].3,
            vec![1, 2, 3]
        );
        assert_eq!(
            connection
                .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(connection.query_row(
            "SELECT COUNT(*) FROM app_metadata WHERE key = 'image_thumbnail_compaction_pending'",
            [], |row| row.get::<_, i64>(0),
        ).unwrap(), 0);
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn thumbnail_cache_refreshes_changed_files_and_does_not_hide_missing_originals() {
        let root =
            std::env::temp_dir().join(format!("image-thumbnail-cache-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("original.png");
        image::DynamicImage::new_rgba8(640, 480)
            .save(&path)
            .unwrap();
        let mut cache = ThumbnailCache::default();
        let first = cache.load(&path).unwrap();
        assert_eq!(cache.load(&path).unwrap(), first);
        assert_eq!(cache.entries.len(), 1);
        image::DynamicImage::new_rgba8(16, 32).save(&path).unwrap();
        assert_ne!(cache.load(&path).unwrap(), first);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.bytes, cache.entries[0].3.len());
        fs::remove_file(&path).unwrap();
        assert!(cache.load(&path).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initialize_migrates_legacy_outputs_to_workspace_files() {
        let root =
            std::env::temp_dir().join(format!("image-migrate-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let task =
            json!({ "id": "legacy-task", "createdAt": 1, "status": "completed", "imageCount": 1 });
        create_batch(&paths, std::slice::from_ref(&task), &[], "").unwrap();
        let connection = open(&paths).unwrap();
        connection
            .execute_batch(
                "DROP VIEW IF EXISTS image_outputs;
                 CREATE TABLE image_outputs(
                   task_id TEXT NOT NULL, position INTEGER NOT NULL, format TEXT NOT NULL,
                   bytes BLOB NOT NULL, file_path TEXT NOT NULL DEFAULT '',
                   PRIMARY KEY(task_id, position)
                 );
                 INSERT INTO image_outputs(task_id, position, format, bytes)
                 VALUES ('legacy-task', 0, 'png', X'040506');",
            )
            .unwrap();
        drop(connection);

        initialize(&paths).unwrap();
        let connection = open(&paths).unwrap();
        assert!(connection
            .query_row(
                "SELECT type FROM sqlite_master WHERE name = 'image_outputs'",
                [],
                |row| row.get::<_, String>(0)
            )
            .optional()
            .unwrap()
            .is_none());
        let stored = task_image_records(&connection, "legacy-task")
            .unwrap()
            .remove(0);
        drop(connection);
        let relative = output_relative_path("legacy", "legacy-task", &stored.1, &stored.2);
        assert!(output_path(&paths, &relative).unwrap().is_file());
        assert_eq!(
            read_images(&paths, &["legacy-task".into()]).unwrap()[0].3,
            vec![4, 5, 6]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initialize_merges_old_task_directories_by_conversation() {
        let root = std::env::temp_dir().join(format!(
            "image-directory-migrate-test-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = resolve_app_paths(&root);
        let tasks = vec![
            json!({
                "id": "old-task-1",
                "createdAt": 1,
                "status": "completed",
                "request": { "conversationId": "shared-conversation" },
                "imageCount": 1
            }),
            json!({
                "id": "old-task-2",
                "createdAt": 2,
                "status": "completed",
                "conversationId": "shared-conversation",
                "imageCount": 1
            }),
        ];
        create_batch(&paths, &tasks, &[], "").unwrap();
        let output_root = root.join("workspace/image-outputs");
        std::fs::create_dir_all(output_root.join("old-task-1")).unwrap();
        std::fs::create_dir_all(output_root.join("old-task-2")).unwrap();
        std::fs::write(output_root.join("old-task-1/0.png"), [1u8, 2, 3]).unwrap();
        std::fs::write(output_root.join("old-task-2/0.png"), [4u8, 5, 6]).unwrap();
        let connection = open(&paths).unwrap();
        connection
            .execute_batch(
                "DROP VIEW IF EXISTS image_outputs;
                 CREATE TABLE image_outputs(
                   task_id TEXT NOT NULL, position INTEGER NOT NULL, format TEXT NOT NULL,
                   bytes BLOB NOT NULL, file_path TEXT NOT NULL DEFAULT '',
                   PRIMARY KEY(task_id, position)
                 );",
            )
            .unwrap();
        for id in ["old-task-1", "old-task-2"] {
            connection
                .execute(
                    "INSERT INTO image_outputs(task_id, position, format, bytes, file_path)
                     VALUES (?1, 0, 'png', X'', ?2)",
                    params![id, format!("{id}/0.png")],
                )
                .unwrap();
        }
        drop(connection);

        initialize(&paths).unwrap();

        let connection = open(&paths).unwrap();
        assert!(connection
            .query_row(
                "SELECT type FROM sqlite_master WHERE name = 'image_outputs'",
                [],
                |row| row.get::<_, String>(0)
            )
            .optional()
            .unwrap()
            .is_none());
        let records = ["old-task-1", "old-task-2"]
            .iter()
            .flat_map(|task_id| {
                task_image_records(&connection, task_id)
                    .unwrap()
                    .into_iter()
                    .map(move |(_, image_id, format)| ((*task_id).to_string(), image_id, format))
            })
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 2);
        let file_paths = records
            .iter()
            .map(|(task_id, image_id, format)| {
                output_relative_path("shared-conversation", task_id, image_id, format)
            })
            .collect::<Vec<_>>();
        assert!(file_paths
            .iter()
            .all(|path| path.starts_with("shared-conversation/old-task-")));
        drop(connection);
        assert_eq!(
            std::fs::read(output_root.join(&file_paths[0])).unwrap(),
            vec![1, 2, 3]
        );
        assert_eq!(
            std::fs::read(output_root.join(&file_paths[1])).unwrap(),
            vec![4, 5, 6]
        );
        assert!(!output_root.join("old-task-1").exists());
        assert!(!output_root.join("old-task-2").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn outputs_from_one_conversation_share_one_directory() {
        let root = std::env::temp_dir().join(format!(
            "image-conversation-output-test-{}",
            uuid::Uuid::new_v4()
        ));
        let paths = resolve_app_paths(&root);
        let tasks = (0..2)
            .map(|index| {
                json!({
                    "id": format!("task-{index}"),
                    "createdAt": index,
                    "status": "queued",
                    "request": { "conversationId": "conversation-a" },
                    "imageCount": 0
                })
            })
            .collect::<Vec<_>>();
        create_batch(&paths, &tasks, &[], "").unwrap();
        for task in &tasks {
            let mut running = start(&paths, task).unwrap().unwrap();
            running["status"] = json!("completed");
            running["imageCount"] = json!(1);
            finish(
                &paths,
                &running,
                &[StoredImage {
                    bytes: vec![1, 2, 3],
                    format: "png".into(),
                    width: 1,
                    height: 1,
                    revised_prompt: String::new(),
                }],
            )
            .unwrap();
        }
        let connection = open(&paths).unwrap();
        let file_paths = ["task-0", "task-1"]
            .iter()
            .flat_map(|task_id| {
                task_image_records(&connection, task_id)
                    .unwrap()
                    .into_iter()
                    .map(move |(_, image_id, format)| {
                        output_relative_path("conversation-a", task_id, &image_id, &format)
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(file_paths.len(), 2);
        assert!(file_paths
            .iter()
            .all(|path| path.starts_with("conversation-a/")));
        assert_ne!(file_paths[0], file_paths[1]);
        assert!(root.join("workspace/image-outputs/conversation-a").is_dir());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn task_core_fields_are_structured_without_payload_column() {
        let root =
            std::env::temp_dir().join(format!("image-task-schema-test-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let task = json!({
            "id": "structured-task",
            "createdAt": 10,
            "status": "queued",
            "accountName": "account@example.com",
            "request": {
                "accountId": "account",
                "generationMode": "codex",
                "mode": "generate",
                "prompt": "a red fox",
                "model": "gpt-image-2",
                "size": "auto",
                "quality": "high",
                "n": 1,
                "outputFormat": "png",
                "background": "",
                "responseFormat": "url",
                "conversationId": "conversation-structured",
                "roundId": "round-1",
                "ratio": "",
                "tier": ""
            },
            "inputId": "input-1",
            "imageCount": 0,
            "images": [],
            "revision": 0,
            "extension": { "providerFlag": true }
        });
        create_batch(&paths, std::slice::from_ref(&task), &[], "").unwrap();

        let connection = open(&paths).unwrap();
        let stored: (String, String, String, i64) = connection
            .query_row(
                "SELECT conversation_id, prompt, model, revision
                 FROM image_tasks WHERE id = 'structured-task'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        let payload_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('image_tasks') WHERE name = 'payload_json'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(payload_columns, 0);
        assert_eq!(stored.0, "conversation-structured");
        assert_eq!(stored.1, "a red fox");
        assert_eq!(stored.2, "gpt-image-2");
        assert_eq!(stored.3, 0);
        drop(connection);

        let hydrated = detail(&paths, "structured-task").unwrap();
        assert_eq!(hydrated["request"]["prompt"], "a red fox");
        assert_eq!(
            hydrated["request"]["conversationId"],
            "conversation-structured"
        );
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
        for id in &ids {
            insert_output(&paths, id, 0, "png", &[1u8, 2, 3]);
        }
        let target = root.join("images.zip");
        assert_eq!(
            write_export_zip(&paths, &ids, target.to_str().unwrap()).unwrap(),
            101
        );
        let archive = zip::ZipArchive::new(File::open(&target).unwrap()).unwrap();
        assert_eq!(archive.len(), 101);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_zip_reports_progress_and_names_batch_images() {
        let root = std::env::temp_dir().join(format!(
            "image-export-progress-test-{}",
            uuid::Uuid::new_v4()
        ));
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
        for position in 0..2 {
            insert_output(&paths, "named-task", position, "png", &[position as u8]);
        }
        assert_eq!(
            export_image_count(&paths, &["named-task".into()]).unwrap(),
            2
        );
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
        insert_output(&source_paths, "task-1", 0, "png", &[1u8, 2, 3]);

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
        assert_eq!(
            read_images(&target_paths, &["task-1".to_string()]).unwrap()[0].3,
            vec![1, 2, 3]
        );
        let stored = task_image_records(&open(&target_paths).unwrap(), "task-1")
            .unwrap()
            .remove(0);
        let relative = output_relative_path("target-conversation", "task-1", &stored.1, &stored.2);
        assert!(target_root
            .join("workspace/image-outputs")
            .join(relative)
            .is_file());

        std::fs::remove_dir_all(source_root).unwrap();
        std::fs::remove_dir_all(target_root).unwrap();
    }
}

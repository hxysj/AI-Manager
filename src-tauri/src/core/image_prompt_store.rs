use crate::core::{database, error::ManagerError, paths::AppPaths};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::Path,
    sync::Mutex,
};

// 词库首次展开与后续导入串行执行，查询不会看到导入中的半成品。
static CATALOG_LOCK: Mutex<()> = Mutex::new(());
const MAX_JSON_BYTES: u64 = 100 * 1024 * 1024;

pub fn dispatch(
    paths: &AppPaths,
    seed_path: &Path,
    channel: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let _guard = CATALOG_LOCK
        .lock()
        .map_err(|_| ManagerError::System("提示词库锁不可用".into()))?;
    let mut connection = database::open(paths)?;
    ensure_prompt_schema(&connection)?;
    if channel == "tools:image-prompts-import" {
        let path = Path::new(payload["path"].as_str().unwrap_or(""));
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            return Err(ManagerError::System("请选择 JSON 提示词文件".into()));
        }
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(MAX_JSON_BYTES + 1).read_to_end(&mut bytes)?;
        return replace(
            &mut connection,
            &bytes,
            &path.file_name().unwrap_or_default().to_string_lossy(),
        );
    }
    let prompt_count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM image_prompt_templates",
            [],
            |row| row.get(0),
        )?;
    if prompt_count == 0 {
        let mut bytes = Vec::new();
        std::fs::File::open(seed_path)?
            .take(MAX_JSON_BYTES + 1)
            .read_to_end(&mut bytes)?;
        replace(&mut connection, &bytes, "gpt-image-2-prompts.json")?;
    }
    match channel {
        "tools:image-prompts-catalog" => {
            catalog_value(&connection)
        }
        "tools:image-prompts-detail" => {
            connection
                .query_row(
                    "SELECT id, title, title_zh, prompt, prompt_localized, model, input_mode,
                            preview_image_url, high_quality_image_url, variables,
                            reference_image_count, aspect_ratio, tags, source, category_ids
                     FROM image_prompt_templates WHERE id = ?1",
                    [payload["id"].as_str().unwrap_or("")],
                    |row| template_value(row, true),
                )
                .optional()?
                .ok_or_else(|| ManagerError::System("提示词不存在，请刷新词库".into()))
        }
        "tools:image-prompts-list" => list(&connection, &payload),
        _ => Err(ManagerError::UnknownChannel(channel.into())),
    }
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    let _guard = CATALOG_LOCK
        .lock()
        .map_err(|_| ManagerError::System("提示词库锁不可用".into()))?;
    let connection = database::open(paths)?;
    ensure_prompt_schema(&connection)
}

fn ensure_prompt_schema(connection: &Connection) -> Result<(), ManagerError> {
    // 先迁移字段再建立新索引，词库复制失败时保留旧模板、分类和目录。
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS image_prompt_templates (
           id TEXT PRIMARY KEY, position INTEGER NOT NULL, search_text TEXT NOT NULL,
           title TEXT NOT NULL, title_zh TEXT NOT NULL DEFAULT '', prompt TEXT NOT NULL,
           prompt_localized TEXT NOT NULL DEFAULT '{}', model TEXT NOT NULL DEFAULT '',
           input_mode TEXT NOT NULL DEFAULT '', preview_image_url TEXT NOT NULL DEFAULT '',
           high_quality_image_url TEXT NOT NULL DEFAULT '', variables TEXT NOT NULL DEFAULT '[]',
           reference_image_count INTEGER NOT NULL DEFAULT 0, aspect_ratio TEXT NOT NULL DEFAULT '',
           tags TEXT NOT NULL DEFAULT '[]', source TEXT NOT NULL DEFAULT '',
           category_ids TEXT NOT NULL DEFAULT ''
         );
         CREATE TABLE IF NOT EXISTS image_prompt_categories (
           id TEXT PRIMARY KEY, group_name TEXT NOT NULL DEFAULT '',
           title TEXT NOT NULL, original_title TEXT NOT NULL DEFAULT ''
         );",
    )?;
    ensure_prompt_category_schema(&transaction)?;
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_image_prompt_category ON image_prompt_categories(id);
         DROP TABLE IF EXISTS image_prompt_catalog;",
    )?;
    transaction.commit()?;
    Ok(())
}

fn ensure_prompt_category_schema(connection: &Connection) -> Result<(), ManagerError> {
    let template_columns = {
        let mut statement = connection.prepare("PRAGMA table_info(image_prompt_templates)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<HashSet<_>, _>>()?
    };
    let has_payload = template_columns.contains("payload_json");
    let has_legacy_templates = !table_columns(connection, "image_prompt_templates_legacy")?.is_empty();
    if has_payload || has_legacy_templates {
        let source = if has_payload { "image_prompt_templates" } else { "image_prompt_templates_legacy" };
        let legacy_rows = {
            let mut statement = connection.prepare(&format!(
                "SELECT id, position, search_text, summary_json, payload_json FROM {source} ORDER BY position"
            ))?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if has_payload {
            connection.execute_batch(
            "ALTER TABLE image_prompt_templates RENAME TO image_prompt_templates_legacy;
             CREATE TABLE image_prompt_templates (
               id TEXT PRIMARY KEY, position INTEGER NOT NULL, search_text TEXT NOT NULL,
               title TEXT NOT NULL, title_zh TEXT NOT NULL DEFAULT '', prompt TEXT NOT NULL,
               prompt_localized TEXT NOT NULL DEFAULT '{}', model TEXT NOT NULL DEFAULT '',
               input_mode TEXT NOT NULL DEFAULT '', preview_image_url TEXT NOT NULL DEFAULT '',
               high_quality_image_url TEXT NOT NULL DEFAULT '', variables TEXT NOT NULL DEFAULT '[]',
               reference_image_count INTEGER NOT NULL DEFAULT 0, aspect_ratio TEXT NOT NULL DEFAULT '',
               tags TEXT NOT NULL DEFAULT '[]', source TEXT NOT NULL DEFAULT '',
               category_ids TEXT NOT NULL DEFAULT ''
             );",
            )?;
        }
        for (id, position, search, summary_raw, detail_raw) in legacy_rows {
            let summary: Value = serde_json::from_str(&summary_raw)?;
            let detail: Value = serde_json::from_str(&detail_raw)?;
            let category_ids = summary["categoryIds"].as_array().map(|items| {
                items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(",")
            }).unwrap_or_default();
            connection.execute(
                "INSERT INTO image_prompt_templates(
                   id, position, search_text, title, title_zh, prompt, prompt_localized,
                   model, input_mode, preview_image_url, high_quality_image_url, variables,
                   reference_image_count, aspect_ratio, tags, source, category_ids
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                 ON CONFLICT(id) DO NOTHING",
                params![
                    id, position, search,
                    text(&detail["title"]), text(&summary["title"]), text(&detail["prompt"]),
                    detail["promptLocalized"].to_string(), text(&summary["model"]), text(&summary["inputMode"]),
                    text(&summary["previewImageUrl"]), text(&summary["highQualityImageUrl"]),
                    detail["variables"].to_string(), detail["referenceImageCount"].as_i64().unwrap_or(0),
                    text(&detail["aspectRatio"]), detail["tags"].to_string(), text(&detail["source"]), category_ids
                ],
            )?;
        }
        connection.execute_batch("DROP TABLE image_prompt_templates_legacy;")?;
    }
    let template_columns = table_columns(connection, "image_prompt_templates")?;
    if !template_columns.contains("category_ids") {
        connection.execute(
            "ALTER TABLE image_prompt_templates ADD COLUMN category_ids TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    let category_columns = {
        let mut statement = connection.prepare("PRAGMA table_info(image_prompt_categories)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<Result<HashSet<_>, _>>()?
    };
    if category_columns.contains("template_id") {
        connection.execute_batch("ALTER TABLE image_prompt_categories RENAME TO image_prompt_categories_legacy;")?;
    }
    if !table_columns(connection, "image_prompt_categories_legacy")?.is_empty() {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS image_prompt_categories(
               id TEXT PRIMARY KEY, group_name TEXT NOT NULL DEFAULT '',
               title TEXT NOT NULL, original_title TEXT NOT NULL DEFAULT ''
             );
             INSERT OR IGNORE INTO image_prompt_categories(id, group_name, title, original_title)
             SELECT DISTINCT category_id, 'Category', category_id, category_id
             FROM image_prompt_categories_legacy;
             UPDATE image_prompt_templates SET category_ids = COALESCE(
               (SELECT group_concat(category_id, ',') FROM image_prompt_categories_legacy
                WHERE template_id = image_prompt_templates.id), '') WHERE category_ids='';
             DROP TABLE image_prompt_categories_legacy;
             CREATE INDEX IF NOT EXISTS idx_image_prompt_category ON image_prompt_categories(id);",
        )?;
    }
    let has_catalog: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='image_prompt_catalog')",
        [], |row| row.get(0),
    )?;
    if has_catalog {
        let catalog: Option<String> = connection.query_row(
            "SELECT payload_json FROM image_prompt_catalog WHERE id=1", [], |row| row.get(0),
        ).optional()?;
        if let Some(catalog) = catalog {
            let catalog: Value = serde_json::from_str(&catalog)?;
            for category in catalog["categories"].as_array().into_iter().flatten() {
                connection.execute(
                    "INSERT INTO image_prompt_categories(id,group_name,title,original_title) VALUES(?1,?2,?3,?4)
                     ON CONFLICT(id) DO UPDATE SET group_name=excluded.group_name,title=excluded.title,original_title=excluded.original_title",
                    params![text(&category["id"]),text(&category["group"]),text(&category["title"]),text(&category["originalTitle"])],
                )?;
            }
        }
    }
    Ok(())
}

fn table_columns(connection: &Connection, table: &str) -> Result<HashSet<String>, ManagerError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<Result<HashSet<_>, _>>()?)
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("").trim()
}

fn image_url(value: &Value) -> String {
    let source = text(value);
    if url::Url::parse(source).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
        source.into()
    } else {
        String::new()
    }
}

fn catalog_value(connection: &Connection) -> Result<Value, ManagerError> {
    let count: u64 = connection.query_row("SELECT COUNT(*) FROM image_prompt_templates", [], |row| row.get(0))?;
    let mut statement = connection.prepare(
        "SELECT c.id,
                (SELECT COUNT(*) FROM image_prompt_templates t
                 WHERE instr(',' || t.category_ids || ',', ',' || c.id || ',') > 0),
                c.title, c.group_name, c.original_title
         FROM image_prompt_categories c ORDER BY c.id",
    )?;
    let categories = statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(json!({
                "id": id,
                "group": row.get::<_, String>(3)?,
                "title": row.get::<_, String>(2)?,
                "originalTitle": row.get::<_, String>(4)?,
                "count": row.get::<_, u64>(1)?
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "filename": "", "count": count, "categories": categories }))
}

fn template_value(row: &rusqlite::Row<'_>, detail: bool) -> rusqlite::Result<Value> {
    let title: String = row.get(1)?;
    let title_zh: String = row.get(2)?;
    let title = if title_zh.is_empty() { title } else { title_zh };
    let mut value = json!({
        "id": row.get::<_, String>(0)?,
        "title": title,
        "model": row.get::<_, String>(5)?,
        "inputMode": row.get::<_, String>(6)?,
        "previewImageUrl": row.get::<_, String>(7)?,
        "highQualityImageUrl": row.get::<_, String>(8)?,
        "categoryIds": row.get::<_, String>(14)?.split(',').filter(|id| !id.is_empty()).collect::<Vec<_>>()
    });
    if detail {
        value["prompt"] = json!(row.get::<_, String>(3)?);
        value["promptLocalized"] = serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_else(|_| json!({}));
        value["variables"] = serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_else(|_| json!([]));
        value["referenceImageCount"] = json!(row.get::<_, i64>(10)?);
        value["aspectRatio"] = json!(row.get::<_, String>(11)?);
        value["tags"] = serde_json::from_str(&row.get::<_, String>(12)?).unwrap_or_else(|_| json!([]));
        value["source"] = json!(row.get::<_, String>(13)?);
    }
    Ok(value)
}

fn replace(
    connection: &mut Connection,
    bytes: &[u8],
    filename: &str,
) -> Result<Value, ManagerError> {
    if bytes.len() as u64 > MAX_JSON_BYTES {
        return Err(ManagerError::System("提示词文件不能超过 100 MB".into()));
    }
    let source: Value =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))?;
    let templates = source["templates"]
        .as_array()
        .ok_or_else(|| ManagerError::System("JSON 需要包含 templates 数组".into()))?;
    if templates.is_empty() || templates.len() > 20_000 {
        return Err(ManagerError::System(
            "提示词库需要包含 1–20000 条模板".into(),
        ));
    }
    let transaction = connection.transaction()?;
    let mut previous_categories = transaction.prepare("SELECT group_name, original_title, id FROM image_prompt_categories")?
        .query_map([], |row| Ok(((row.get::<_, String>(0)?, row.get::<_, String>(1)?), row.get::<_, String>(2)?)))?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    transaction.execute("DELETE FROM image_prompt_templates", [])?;
    transaction.execute("DELETE FROM image_prompt_categories", [])?;
    let mut ids = HashSet::new();
    let mut categories = BTreeMap::<String, Value>::new();
    for (index, item) in templates.iter().enumerate() {
        let id = text(&item["id"]);
        if id.is_empty()
            || text(&item["title"]).is_empty()
            || text(&item["prompt"]).is_empty()
            || !ids.insert(id.to_string())
        {
            return Err(ManagerError::System(format!(
                "第 {} 条模板缺少 id、title、prompt，或 id 重复；原词库未更改",
                index + 1
            )));
        }
        let mut category_items = item["sourceCategories"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if category_items.is_empty() {
            let title = text(&item["category"]);
            category_items.push(json!({ "group": "Category", "slug": if title.is_empty() { "uncategorized" } else { title }, "title": if title.is_empty() { "未分类" } else { title } }));
        }
        let mut item_categories = Vec::new();
        for category in &category_items {
            let group = text(&category["group"]);
            let title = text(&category["title"]);
            if title.is_empty() {
                continue;
            }
            let slug = text(&category["slug"]);
            let category_key = format!("{group}:{}", if slug.is_empty() { title } else { slug });
            let localized = text(&category["localized"]["zh"]["title"]);
            let localized_group = text(&category["localized"]["zh"]["group"]);
            let group_name = if localized_group.is_empty() { group } else { localized_group };
            let category_id = categories.get(&category_key).map(|item| text(&item["id"]).to_string())
                .or_else(|| previous_categories.remove(&(group_name.to_string(), title.to_string())))
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            if item_categories.contains(&category_id) {
                continue;
            }
            let entry = categories.entry(category_key).or_insert_with(|| json!({
                "id": category_id, "group": group_name,
                "title": if localized.is_empty() { title } else { localized }, "originalTitle": title, "count": 0
            }));
            entry["count"] = json!(entry["count"].as_u64().unwrap_or(0) + 1);
            item_categories.push(category_id);
        }
        let title_zh = text(&item["titleLocalized"]["zh"]);
        let summary = json!({
            "id": id, "title": if title_zh.is_empty() { text(&item["title"]) } else { title_zh },
            "categoryIds": item_categories.clone(), "model": item["model"], "inputMode": item["inputMode"],
            "previewImageUrl": image_url(&item["previewImageUrl"]),
            "highQualityImageUrl": image_url(&item["highQualityImageUrl"])
        });
        let mut detail = summary.clone();
        for key in [
            "prompt",
            "promptLocalized",
            "variables",
            "referenceImageCount",
            "aspectRatio",
            "tags",
            "source",
        ] {
            detail[key] = item[key].clone();
        }
        // 原始提示词只作为文本保存和展示，不执行其中的指令或 HTML。
        let search = format!(
            "{} {} {} {} {} {}",
            text(&item["title"]),
            title_zh,
            text(&item["prompt"]),
            text(&item["promptLocalized"]["zh"]),
            item["tags"],
            item["category"]
        )
        .to_lowercase();
        transaction.execute(
            "INSERT INTO image_prompt_templates(
               id, position, search_text, title, title_zh, prompt, prompt_localized,
               model, input_mode, preview_image_url, high_quality_image_url, variables,
               reference_image_count, aspect_ratio, tags, source, category_ids
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                id, index, search, text(&item["title"]), title_zh, text(&item["prompt"]),
                item["promptLocalized"].to_string(), text(&item["model"]), text(&item["inputMode"]),
                image_url(&item["previewImageUrl"]), image_url(&item["highQualityImageUrl"]),
                item["variables"].to_string(), item["referenceImageCount"].as_i64().unwrap_or(0),
                text(&item["aspectRatio"]), item["tags"].to_string(), text(&item["source"]),
                item_categories.join(",")
            ],
        )?;
    }
    for category in categories.values() {
        transaction.execute(
            "INSERT INTO image_prompt_categories(id, group_name, title, original_title)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                category["id"].as_str().unwrap_or(""),
                category["group"].as_str().unwrap_or(""),
                category["title"].as_str().unwrap_or(""),
                category["originalTitle"].as_str().unwrap_or("")
            ],
        )?;
    }
    let catalog = json!({ "filename": filename, "count": templates.len(), "categories": categories.into_values().collect::<Vec<_>>(),
        "generatedAt": source["generatedAt"], "importedAt": chrono::Utc::now().timestamp_millis() });
    transaction.commit()?;
    Ok(catalog)
}

fn list(connection: &Connection, payload: &Value) -> Result<Value, ManagerError> {
    let category = text(&payload["category"]);
    let query = text(&payload["query"]).to_lowercase();
    let page = payload["page"].as_u64().unwrap_or(1).clamp(1, 100_000);
    let filter = "(?1 = '' OR instr(',' || t.category_ids || ',', ',' || ?1 || ',') > 0) AND (?2 = '' OR instr(t.search_text, ?2) > 0)";
    let total: u64 = connection.query_row(
        &format!("SELECT COUNT(*) FROM image_prompt_templates t WHERE {filter}"),
        params![category, query],
        |row| row.get(0),
    )?;
    let page = page.min(total.div_ceil(24).max(1));
    let mut statement = connection.prepare(&format!(
        "SELECT id, title, title_zh, prompt, prompt_localized, model, input_mode,
                preview_image_url, high_quality_image_url, variables, reference_image_count,
                aspect_ratio, tags, source, category_ids
         FROM image_prompt_templates t WHERE {filter} ORDER BY position LIMIT 24 OFFSET ?3"
    ))?;
    let rows = statement
        .query_map(params![category, query, (page - 1) * 24], |row| template_value(row, false))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": rows, "total": total, "page": page, "pageSize": 24 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch(paths: &AppPaths, channel: &str, payload: Value) -> Result<Value, ManagerError> {
        super::dispatch(
            paths,
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../assets/image-prompts/gpt-image-2-prompts.json"),
            channel,
            payload,
        )
    }

    #[test]
    fn image_prompt_seed_categories_pagination_and_atomic_import() {
        let root =
            std::env::temp_dir().join(format!("image-prompts-test-{}", uuid::Uuid::new_v4()));
        let paths = crate::core::paths::resolve_app_paths(&root);
        let catalog = dispatch(&paths, "tools:image-prompts-catalog", json!({})).unwrap();
        assert_eq!(catalog["count"], 7902);
        assert_eq!(catalog["categories"].as_array().unwrap().len(), 71);
        let page = dispatch(&paths, "tools:image-prompts-list", json!({})).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 24);
        assert!(page["items"][0].get("prompt").is_none());
        let id = &page["items"][0]["id"];
        let detail = dispatch(&paths, "tools:image-prompts-detail", json!({ "id": id })).unwrap();
        assert!(!text(&detail["prompt"]).is_empty());
        let category = &catalog["categories"][0];
        assert_eq!(uuid::Uuid::parse_str(category["id"].as_str().unwrap()).unwrap().get_version_num(), 4);
        let filtered = dispatch(
            &paths,
            "tools:image-prompts-list",
            json!({"category":category["id"]}),
        )
        .unwrap();
        assert_eq!(filtered["total"], category["count"]);
        let empty = dispatch(
            &paths,
            "tools:image-prompts-list",
            json!({"query":"nonexistent___template","page":99}),
        )
        .unwrap();
        assert_eq!(empty["total"], 0);
        assert_eq!(empty["page"], 1);
        let bad = root.join("bad.json");
        std::fs::write(&bad, br#"{"templates":[{"id":"broken"}]}"#).unwrap();
        assert!(dispatch(&paths, "tools:image-prompts-import", json!({"path":bad})).is_err());
        assert_eq!(
            dispatch(&paths, "tools:image-prompts-catalog", json!({})).unwrap()["count"],
            7902
        );
        let custom = root.join("custom.json");
        std::fs::write(&custom, br#"{"templates":[{"id":"custom","title":"Custom","prompt":"Text","category":"Test","previewImageUrl":"javascript:bad"}]}"#).unwrap();
        assert_eq!(
            dispatch(&paths, "tools:image-prompts-import", json!({"path":custom})).unwrap()
                ["count"],
            1
        );
        let detail =
            dispatch(&paths, "tools:image-prompts-detail", json!({"id":"custom"})).unwrap();
        assert_eq!(detail["previewImageUrl"], "");
        assert_eq!(detail["prompt"], "Text");
        let before = dispatch(&paths, "tools:image-prompts-catalog", json!({})).unwrap();
        dispatch(&paths, "tools:image-prompts-import", json!({"path":custom})).unwrap();
        assert_eq!(dispatch(&paths, "tools:image-prompts-catalog", json!({})).unwrap()["categories"], before["categories"]);
        std::fs::remove_dir_all(root).unwrap();
    }
}

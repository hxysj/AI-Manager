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
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS image_prompt_templates (
           id TEXT PRIMARY KEY, position INTEGER NOT NULL, search_text TEXT NOT NULL,
           summary_json TEXT NOT NULL, payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS image_prompt_categories (
           template_id TEXT NOT NULL, category_id TEXT NOT NULL,
           PRIMARY KEY(template_id, category_id)
         );
         CREATE INDEX IF NOT EXISTS idx_image_prompt_category ON image_prompt_categories(category_id, template_id);
         CREATE TABLE IF NOT EXISTS image_prompt_catalog (id INTEGER PRIMARY KEY, payload_json TEXT NOT NULL);",
    )?;
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
    let catalog: Option<String> = connection
        .query_row(
            "SELECT payload_json FROM image_prompt_catalog WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if catalog.is_none() {
        let mut bytes = Vec::new();
        std::fs::File::open(seed_path)?
            .take(MAX_JSON_BYTES + 1)
            .read_to_end(&mut bytes)?;
        replace(&mut connection, &bytes, "gpt-image-2-prompts.json")?;
    }
    match channel {
        "tools:image-prompts-catalog" => {
            let catalog: String = connection.query_row(
                "SELECT payload_json FROM image_prompt_catalog WHERE id = 1",
                [],
                |row| row.get(0),
            )?;
            Ok(serde_json::from_str(&catalog)?)
        }
        "tools:image-prompts-detail" => {
            let body: String = connection
                .query_row(
                    "SELECT payload_json FROM image_prompt_templates WHERE id = ?1",
                    [payload["id"].as_str().unwrap_or("")],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| ManagerError::System("提示词不存在，请刷新词库".into()))?;
            Ok(serde_json::from_str(&body)?)
        }
        "tools:image-prompts-list" => list(&connection, &payload),
        _ => Err(ManagerError::UnknownChannel(channel.into())),
    }
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
            let category_id = format!("{group}:{}", if slug.is_empty() { title } else { slug });
            if item_categories.contains(&category_id) {
                continue;
            }
            let localized = text(&category["localized"]["zh"]["title"]);
            let localized_group = text(&category["localized"]["zh"]["group"]);
            let entry = categories.entry(category_id.clone()).or_insert_with(|| json!({
                "id": category_id, "group": if localized_group.is_empty() { group } else { localized_group },
                "title": if localized.is_empty() { title } else { localized }, "originalTitle": title, "count": 0
            }));
            entry["count"] = json!(entry["count"].as_u64().unwrap_or(0) + 1);
            transaction.execute(
                "INSERT INTO image_prompt_categories(template_id, category_id) VALUES (?1, ?2)",
                params![id, category_id],
            )?;
            item_categories.push(category_id);
        }
        let title_zh = text(&item["titleLocalized"]["zh"]);
        let summary = json!({
            "id": id, "title": if title_zh.is_empty() { text(&item["title"]) } else { title_zh },
            "categoryIds": item_categories, "model": item["model"], "inputMode": item["inputMode"],
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
        transaction.execute("INSERT INTO image_prompt_templates(id, position, search_text, summary_json, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, index, search, summary.to_string(), detail.to_string()])?;
    }
    let catalog = json!({ "filename": filename, "count": templates.len(), "categories": categories.into_values().collect::<Vec<_>>(),
        "generatedAt": source["generatedAt"], "importedAt": chrono::Utc::now().timestamp_millis() });
    transaction.execute("INSERT INTO image_prompt_catalog(id, payload_json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET payload_json = excluded.payload_json", [catalog.to_string()])?;
    transaction.commit()?;
    Ok(catalog)
}

fn list(connection: &Connection, payload: &Value) -> Result<Value, ManagerError> {
    let category = text(&payload["category"]);
    let query = text(&payload["query"]).to_lowercase();
    let page = payload["page"].as_u64().unwrap_or(1).clamp(1, 100_000);
    let filter = "(?1 = '' OR EXISTS (SELECT 1 FROM image_prompt_categories c WHERE c.template_id = t.id AND c.category_id = ?1)) AND (?2 = '' OR instr(t.search_text, ?2) > 0)";
    let total: u64 = connection.query_row(
        &format!("SELECT COUNT(*) FROM image_prompt_templates t WHERE {filter}"),
        params![category, query],
        |row| row.get(0),
    )?;
    let page = page.min(total.div_ceil(24).max(1));
    let mut statement = connection.prepare(&format!("SELECT summary_json FROM image_prompt_templates t WHERE {filter} ORDER BY position LIMIT 24 OFFSET ?3"))?;
    let rows = statement
        .query_map(params![category, query, (page - 1) * 24], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let items = rows
        .iter()
        .map(|row| serde_json::from_str::<Value>(row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items, "total": total, "page": page, "pageSize": 24 }))
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
        std::fs::remove_dir_all(root).unwrap();
    }
}

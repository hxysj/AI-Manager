use crate::core::{database, error::ManagerError, paths::AppPaths};
use rusqlite::{
    params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension, Row,
    Transaction,
};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const SKILL_TRASH_RETENTION_MS: u128 = 10 * 24 * 60 * 60 * 1000;

#[derive(Clone, Copy)]
enum FieldKind {
    Text,
    Integer,
    Boolean,
    List,
    Array,
}
use FieldKind::{Array, Boolean, Integer, List, Text};
type Field = (&'static str, &'static str, FieldKind);

// 每个字段有固定含义；列表和状态映射独立存储，不再序列化整个业务对象。
fn fields(table: &str) -> &'static [Field] {
    match table {
        "skills" => &[
            ("id", "id", Text),
            ("name", "name", Text),
            ("group_id", "groupId", Text),
            ("description", "description", Text),
            ("version", "version", Text),
            ("author", "author", Text),
            ("homepage", "homepage", Text),
            ("icon", "icon", Text),
            ("tags", "tags", List),
            ("entry", "entry", Text),
            ("entry_path", "entryPath", Text),
            ("source_path", "sourcePath", Text),
            ("repo_id", "repoId", Text),
            ("repo_name", "repoName", Text),
            ("repository", "repository", Text),
            ("disabled", "disabled", Boolean),
            ("status", "status", Text),
            ("installed_targets", "installedTargets", List),
            ("created_at", "createdAt", Integer),
            ("updated_at", "updatedAt", Integer),
            ("is_deleted", "isDeleted", Boolean),
            ("deleted_at", "deletedAt", Integer),
            ("trash_id", "trashId", Text),
            ("trash_path", "trashPath", Text),
        ],
        "skill_groups" => &[
            ("id", "id", Text),
            ("name", "name", Text),
            ("created_at", "createdAt", Integer),
            ("updated_at", "updatedAt", Integer),
        ],
        "skill_repositories" => &[
            ("id", "id", Text),
            ("type", "type", Text),
            ("name", "name", Text),
            ("source", "source", Text),
            ("owner", "owner", Text),
            ("repository", "repository", Text),
            ("branch", "branch", Text),
            ("root_path", "rootPath", Text),
            ("html_url", "htmlUrl", Text),
            ("created_at", "createdAt", Integer),
            ("updated_at", "updatedAt", Integer),
        ],
        "skill_repository_cache" => &[
            ("id", "id", Text),
            ("status", "status", Text),
            ("skills", "skills", Array),
            ("error", "error", Text),
            ("last_synced_at", "lastSyncedAt", Integer),
            ("updated_at", "updatedAt", Integer),
        ],
        _ => unreachable!("未知的 Skill 数据表"),
    }
}

fn create_table(connection: &Connection, table: &str) -> Result<(), ManagerError> {
    let columns = fields(table)
        .iter()
        .map(|(column, _, kind)| {
            let definition = match kind {
                Integer | Boolean => "INTEGER NOT NULL DEFAULT 0",
                Array => "TEXT NOT NULL DEFAULT '[]'",
                _ => "TEXT NOT NULL DEFAULT ''",
            };
            format!("{column} {definition}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    connection.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {table} (item_key TEXT PRIMARY KEY, sort_order INTEGER NOT NULL, {columns});"
    ))?;
    Ok(())
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = database::open(paths)?;
    migrate_connection(&mut connection)?;
    migrate_legacy_json(paths, &mut connection)?;
    remove_legacy_files(paths)
}

pub(crate) fn migrate_connection(connection: &mut Connection) -> Result<(), ManagerError> {
    let tables = [
        "skills",
        "skill_groups",
        "skill_repositories",
        "skill_repository_cache",
    ];
    let mut legacy = Vec::new();
    let mut removed_columns = Vec::new();
    let mut add_group_id = false;
    let mut group_memberships = Vec::new();
    let mut remove_group_members = false;
    for table in tables {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if table == "skills"
            && !columns.is_empty()
            && !columns.iter().any(|column| column == "payload_json")
        {
            add_group_id = !columns.iter().any(|column| column == "group_id");
            for column in ["content", "install_states"] {
                if columns.iter().any(|name| name == column) {
                    removed_columns.push(column);
                }
            }
        }
        if table == "skill_groups" && columns.iter().any(|column| column == "skill_ids") {
            remove_group_members = true;
            group_memberships = connection.prepare("SELECT id, skill_ids FROM skill_groups")?
                .query_map([], |row| Ok(json!({"id":row.get::<_,String>(0)?,"skillIds":row.get::<_,String>(1)?.split(',').filter(|id| !id.is_empty()).collect::<Vec<_>>()})))?
                .collect::<Result<Vec<_>,_>>()?;
        }
        if columns.is_empty() || columns.iter().any(|column| column == "payload_json") {
            legacy.push((table, !columns.is_empty()));
        }
    }
    let obsolete: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('skill_installs', 'skill_trash', 'skill_schema_migrations')",
        [], |row| row.get(0),
    )?;
    if legacy.is_empty()
        && obsolete == 0
        && removed_columns.is_empty()
        && !add_group_id
        && !remove_group_members
    {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    if add_group_id {
        transaction
            .execute_batch("ALTER TABLE skills ADD COLUMN group_id TEXT NOT NULL DEFAULT ''")?;
    }
    if remove_group_members {
        transaction.execute_batch("ALTER TABLE skill_groups DROP COLUMN skill_ids")?;
    }
    for column in &removed_columns {
        transaction.execute_batch(&format!("ALTER TABLE skills DROP COLUMN {column}"))?;
    }
    for (table, exists) in legacy {
        if exists {
            transaction.execute_batch(&format!(
                "ALTER TABLE {table} RENAME TO {table}_legacy_structured;"
            ))?;
        }
        create_table(&transaction, table)?;
        if exists {
            let mut statement = transaction.prepare(&format!(
                "SELECT item_key, sort_order, payload_json FROM {table}_legacy_structured"
            ))?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let key: String = row.get(0)?;
                let order: i64 = row.get(1)?;
                let value: Value = serde_json::from_str(&row.get::<_, String>(2)?)?;
                if table == "skill_groups" {
                    group_memberships.push(value.clone());
                }
                upsert_item(&transaction, table, &key, order, &value)?;
            }
            drop(rows);
            drop(statement);
            transaction.execute_batch(&format!("DROP TABLE {table}_legacy_structured;"))?;
        }
    }
    if !group_memberships.is_empty() {
        update_group_memberships(&transaction, &group_memberships)?;
    }
    if !removed_columns.is_empty() {
        let items = transaction
            .prepare("SELECT item_key, sort_order, skills FROM skill_repository_cache")?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (key, _, skills) in items {
            let mut skills: Value = serde_json::from_str(&skills)?;
            remove_cached_content(&mut skills);
            transaction.execute(
                "UPDATE skill_repository_cache SET skills=?1 WHERE item_key=?2",
                params![skills.to_string(), key],
            )?;
        }
    }
    let has_installs: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='skill_installs')",
        [],
        |row| row.get(0),
    )?;
    if has_installs {
        let installs = transaction
            .prepare("SELECT skill_name, payload_json FROM skill_installs")?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (name, payload) in installs {
            merge_install(&transaction, &name, &serde_json::from_str(&payload)?)?;
        }
    }
    let has_trash: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='skill_trash')",
        [],
        |row| row.get(0),
    )?;
    if has_trash {
        let items = transaction
            .prepare("SELECT payload_json FROM skill_trash ORDER BY sort_order")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for payload in items {
            upsert_trash(&transaction, &serde_json::from_str(&payload)?)?;
        }
    }
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_skills_deleted ON skills(is_deleted, deleted_at);
         CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name);
         CREATE INDEX IF NOT EXISTS idx_skills_group ON skills(group_id, is_deleted);
         DROP TABLE IF EXISTS skill_installs;
         DROP TABLE IF EXISTS skill_trash;
         DROP TABLE IF EXISTS skill_schema_migrations;",
    )?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn item_from_row(table: &str, row: &Row<'_>) -> rusqlite::Result<Value> {
    let mut item = Map::new();
    for (column, key, kind) in fields(table) {
        let value = match kind {
            Integer => json!(row.get::<_, i64>(*column)?),
            Boolean => json!(row.get::<_, bool>(*column)?),
            List => json!(row
                .get::<_, String>(*column)?
                .split(',')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()),
            Array => serde_json::from_str(&row.get::<_, String>(*column)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
            Text => json!(row.get::<_, String>(*column)?),
        };
        item.insert((*key).into(), value);
    }
    Ok(Value::Object(item))
}

fn remove_cached_content(skills: &mut Value) {
    if let Some(items) = skills.as_array_mut() {
        for item in items {
            if let Some(item) = item.as_object_mut() {
                item.remove("content");
                item.remove("installStates");
            }
        }
    }
}

fn upsert_item(
    transaction: &Transaction<'_>,
    table: &str,
    key: &str,
    order: i64,
    item: &Value,
) -> Result<(), ManagerError> {
    let mut columns = vec!["item_key", "sort_order"];
    let mut values = vec![SqlValue::Text(key.into()), SqlValue::Integer(order)];
    for (column, json_key, kind) in fields(table) {
        columns.push(column);
        let value = &item[*json_key];
        values.push(match kind {
            Integer => SqlValue::Integer(value.as_i64().unwrap_or(0)),
            Boolean => SqlValue::Integer(i64::from(value.as_bool().unwrap_or(false))),
            List => SqlValue::Text(
                value
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default(),
            ),
            Array => {
                let mut items = if value.is_array() {
                    value.clone()
                } else {
                    json!([])
                };
                remove_cached_content(&mut items);
                SqlValue::Text(serde_json::to_string(&items)?)
            }
            Text => SqlValue::Text(value.as_str().unwrap_or("").into()),
        });
    }
    let updates = columns
        .iter()
        .skip(1)
        .filter(|column| table != "skills" || **column != "group_id")
        .map(|column| format!("{column}=excluded.{column}"))
        .collect::<Vec<_>>()
        .join(",");
    let changed = columns
        .iter()
        .skip(1)
        .filter(|column| table != "skills" || **column != "group_id")
        .map(|column| format!("{column} IS NOT excluded.{column}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    transaction.execute(&format!(
        "INSERT INTO {table}({}) VALUES ({}) ON CONFLICT(item_key) DO UPDATE SET {updates} WHERE {changed}",
        columns.join(","), vec!["?"; values.len()].join(",")
    ), params_from_iter(values))?;
    Ok(())
}

fn replace_collection(
    transaction: &Transaction<'_>,
    table: &str,
    items: &[Value],
    key_fields: &[&str],
) -> Result<(), ManagerError> {
    let keys = items
        .iter()
        .enumerate()
        .map(|(index, item)| collection_key(item, key_fields, index))
        .collect::<Vec<_>>();
    let filter = if table == "skills" {
        "is_deleted = 0 AND "
    } else {
        ""
    };
    transaction.execute(
        &format!(
            "DELETE FROM {table} WHERE {filter}item_key NOT IN (SELECT value FROM json_each(?1))"
        ),
        [serde_json::to_string(&keys)?],
    )?;
    for (index, (item, key)) in items.iter().zip(&keys).enumerate() {
        upsert_item(transaction, table, key, index as i64, item)?;
    }
    if table == "skill_groups" {
        update_group_memberships(transaction, items)?;
    }
    Ok(())
}

fn update_group_memberships(
    transaction: &Transaction<'_>,
    groups: &[Value],
) -> Result<(), ManagerError> {
    transaction.execute("UPDATE skills SET group_id='' WHERE group_id<>''", [])?;
    for group in groups {
        let members = group.get("skillIds").cloned().unwrap_or_else(|| json!([]));
        transaction.execute("UPDATE skills SET group_id=?1 WHERE is_deleted=0 AND (id IN (SELECT value FROM json_each(?2)) OR (id='' AND name IN (SELECT value FROM json_each(?2))))",
            params![group["id"].as_str().unwrap_or(""), members.to_string()])?;
    }
    Ok(())
}

fn read_collection(paths: &AppPaths, table: &str) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let filter = if table == "skills" {
        "WHERE is_deleted = 0"
    } else {
        ""
    };
    let mut statement = connection.prepare(&format!(
        "SELECT * FROM {table} {filter} ORDER BY sort_order, item_key"
    ))?;
    let rows = statement
        .query_map([], |row| item_from_row(table, row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn write_collection(
    paths: &AppPaths,
    table: &str,
    items: &[Value],
    key_fields: &[&str],
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction()?;
    replace_collection(&transaction, table, items, key_fields)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_skills(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "skills")
}
pub fn write_skills(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "skills", items, &["name", "id", "sourcePath"])
}
pub fn read_groups(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    let mut groups = read_collection(paths, "skill_groups")?;
    let connection = database::open(paths)?;
    let mut statement = connection.prepare("SELECT CASE WHEN id='' THEN name ELSE id END FROM skills WHERE group_id=?1 AND is_deleted=0 ORDER BY sort_order,item_key")?;
    for group in &mut groups {
        let members = statement
            .query_map([group["id"].as_str().unwrap_or("")], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        group["skillIds"] = json!(members);
    }
    Ok(groups)
}
pub fn write_groups(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "skill_groups", items, &["id"])
}
pub fn read_repositories(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "skill_repositories")
}
pub fn write_repositories(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "skill_repositories", items, &["id"])
}
pub fn read_repository_cache(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "skill_repository_cache")
}
pub fn write_repository_cache(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "skill_repository_cache", items, &["id"])
}

pub fn read_installs(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    Ok(read_skills(paths)?
        .into_iter()
        .filter(|item| {
            item["installedTargets"]
                .as_array()
                .is_some_and(|targets| !targets.is_empty())
        })
        .map(|item| {
            (
                item["name"].as_str().unwrap_or("").to_string(),
                item["installedTargets"].clone(),
            )
        })
        .collect())
}

fn merge_install(
    transaction: &Transaction<'_>,
    name: &str,
    targets: &Value,
) -> Result<(), ManagerError> {
    let existing: Option<(i64, Value)> = transaction
        .query_row(
            "SELECT * FROM skills WHERE name=?1 AND is_deleted=0",
            [name],
            |row| Ok((row.get("sort_order")?, item_from_row("skills", row)?)),
        )
        .optional()?;
    let (order, mut item) = match existing {
        Some(value) => value,
        None => (
            transaction.query_row(
                "SELECT COALESCE(MAX(sort_order),-1)+1 FROM skills",
                [],
                |row| row.get(0),
            )?,
            json!({"name": name, "id": name}),
        ),
    };
    item["installedTargets"] = targets.clone();
    upsert_item(transaction, "skills", name, order, &item)
}

#[cfg(test)]
pub fn write_installs(paths: &AppPaths, installs: &Map<String, Value>) -> Result<(), ManagerError> {
    let mut skills = read_skills(paths)?;
    for skill in &mut skills {
        skill["installedTargets"] = installs
            .get(skill["name"].as_str().unwrap_or(""))
            .cloned()
            .unwrap_or_else(|| json!([]));
    }
    for (name, targets) in installs {
        if !skills.iter().any(|skill| skill["name"] == *name) {
            skills.push(json!({"name": name, "id": name, "installedTargets": targets}));
        }
    }
    write_skills(paths, &skills)
}

fn upsert_trash(transaction: &Transaction<'_>, item: &Value) -> Result<(), ManagerError> {
    let trash_id = item["id"].as_str().unwrap_or("");
    let name = item["name"].as_str().unwrap_or("");
    let source = item["sourcePath"].as_str().unwrap_or("");
    let key = format!("trash:{trash_id}");
    let old: Option<Value> = transaction
        .query_row("SELECT * FROM skills WHERE item_key=?1", [&key], |row| {
            item_from_row("skills", row)
        })
        .optional()?;
    let active: Option<(String, Value)> = if old.is_none() && !Path::new(source).exists() {
        transaction
            .query_row(
                "SELECT * FROM skills WHERE name=?1 AND source_path=?2 AND is_deleted=0",
                params![name, source],
                |row| Ok((row.get(0)?, item_from_row("skills", row)?)),
            )
            .optional()?
    } else {
        None
    };
    let mut value = old
        .or_else(|| active.as_ref().map(|(_, value)| value.clone()))
        .unwrap_or_else(
            || json!({"name": name, "description": item["description"], "sourcePath": source}),
        );
    value["isDeleted"] = json!(true);
    value["deletedAt"] = item["deletedAt"].clone();
    value["trashId"] = json!(trash_id);
    value["trashPath"] = item["trashPath"].clone();
    value["installedTargets"] = json!([]);
    if let Some((active_key, _)) = active {
        transaction.execute("DELETE FROM skills WHERE item_key=?1", [active_key])?;
    }
    upsert_item(transaction, "skills", &key, 0, &value)
}

pub fn read_trash(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut statement = connection
        .prepare("SELECT * FROM skills WHERE is_deleted=1 ORDER BY deleted_at, item_key")?;
    let rows = statement
        .query_map([], |row| item_from_row("skills", row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().map(|value| json!({
        "id": value["trashId"], "name": value["name"], "description": value["description"],
        "sourcePath": value["sourcePath"], "trashPath": value["trashPath"], "deletedAt": value["deletedAt"],
        "expiresAt": value["deletedAt"].as_u64().unwrap_or(0) as u128 + SKILL_TRASH_RETENTION_MS
    })).collect())
}

pub fn write_trash(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction()?;
    let ids = items
        .iter()
        .map(|item| item["id"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    transaction.execute("DELETE FROM skills WHERE is_deleted=1 AND trash_id NOT IN (SELECT value FROM json_each(?1))", [serde_json::to_string(&ids)?])?;
    for item in items {
        upsert_trash(&transaction, item)?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn restore_trashed_skill(
    paths: &AppPaths,
    trash_id: &str,
    source: &str,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction()?;
    let mut item = transaction.query_row(
        "SELECT * FROM skills WHERE trash_id=?1 AND is_deleted=1",
        [trash_id],
        |row| item_from_row("skills", row),
    )?;
    transaction.execute(
        "DELETE FROM skills WHERE trash_id=?1 AND is_deleted=1",
        [trash_id],
    )?;
    item["sourcePath"] = json!(source);
    item["isDeleted"] = json!(false);
    item["deletedAt"] = json!(0);
    item["trashId"] = json!("");
    item["trashPath"] = json!("");
    upsert_item(
        &transaction,
        "skills",
        item["name"].as_str().unwrap_or(""),
        0,
        &item,
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_legacy_json(paths: &AppPaths, connection: &mut Connection) -> Result<(), ManagerError> {
    let trash_path = legacy_trash_path(paths);
    let legacy_files = [
        Path::new(&paths.storage_files.skills),
        Path::new(&paths.storage_files.skill_groups),
        Path::new(&paths.storage_files.skill_repositories),
        Path::new(&paths.storage_files.skill_repository_cache),
        Path::new(&paths.storage_files.installs),
        trash_path.as_path(),
    ];
    if !legacy_files.iter().any(|path| path.exists()) {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    for (index, table) in [
        "skills",
        "skill_groups",
        "skill_repositories",
        "skill_repository_cache",
    ]
    .iter()
    .enumerate()
    {
        if legacy_files[index].exists() {
            replace_collection(
                &transaction,
                table,
                &read_array_file(legacy_files[index])?,
                if *table == "skills" {
                    &["name", "id", "sourcePath"]
                } else {
                    &["id"]
                },
            )?;
        }
    }
    for (name, targets) in read_object_file(legacy_files[4])? {
        merge_install(&transaction, &name, &targets)?;
    }
    for item in read_array_file(legacy_files[5])? {
        upsert_trash(&transaction, &item)?;
    }
    transaction.commit()?;
    Ok(())
}

fn remove_legacy_files(paths: &AppPaths) -> Result<(), ManagerError> {
    for path in [
        PathBuf::from(&paths.storage_files.skills),
        PathBuf::from(&paths.storage_files.skill_groups),
        PathBuf::from(&paths.storage_files.skill_repositories),
        PathBuf::from(&paths.storage_files.skill_repository_cache),
        PathBuf::from(&paths.storage_files.installs),
        legacy_trash_path(paths),
    ] {
        remove_file_family(&path)?;
    }
    Ok(())
}

fn remove_file_family(path: &Path) -> Result<(), ManagerError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return Ok(());
    };

    if !parent.exists() {
        return Ok(());
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
    Ok(())
}

fn legacy_trash_path(paths: &AppPaths) -> PathBuf {
    Path::new(&paths.temp_dir)
        .join("skill-trash")
        .join("trash.json")
}

fn read_array_file(path: &Path) -> Result<Vec<Value>, ManagerError> {
    Ok(read_json_file(path)?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default())
}

fn read_object_file(path: &Path) -> Result<Map<String, Value>, ManagerError> {
    Ok(read_json_file(path)?
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default())
}

fn read_json_file(path: &Path) -> Result<Option<Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(serde_json::from_str(&content)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn collection_key(item: &Value, key_fields: &[&str], index: usize) -> String {
    key_fields
        .iter()
        .find_map(|field| {
            item.get(*field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| format!("item-{index}"))
}

#[cfg(test)]
mod tests {
    use super::{read_groups, read_installs, read_repository_cache, read_skills, write_groups};
    use crate::core::paths::resolve_app_paths;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn migrates_skill_json_into_main_database() {
        let root =
            std::env::temp_dir().join(format!("monkey-thief-skill-store-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        std::fs::create_dir_all(&paths.storage_dir).unwrap();
        std::fs::create_dir_all(&paths.temp_dir).unwrap();
        let trash_index = Path::new(&paths.temp_dir)
            .join("skill-trash")
            .join("trash.json");
        std::fs::create_dir_all(trash_index.parent().unwrap()).unwrap();
        std::fs::write(
            &paths.storage_files.skills,
            serde_json::to_string(&json!([{"name": "skill-a"}])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.skill_groups,
            serde_json::to_string(&json!([{"id": "group-a", "skillIds": ["skill-a"]}])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.skill_repository_cache,
            serde_json::to_string(&json!([{"id": "repo-a", "status": "ready"}])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.skill_repositories,
            serde_json::to_string(&json!([{"id": "repo-a", "name": "Repo A"}])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.installs,
            serde_json::to_string(&json!({"skill-a": ["codex"]})).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &trash_index,
            serde_json::to_string(&json!([{"id": "trash-a", "trashPath": "missing"}])).unwrap(),
        )
        .unwrap();

        assert_eq!(read_skills(&paths).unwrap().len(), 1);
        assert_eq!(read_groups(&paths).unwrap().len(), 1);
        assert_eq!(read_repository_cache(&paths).unwrap().len(), 1);
        assert_eq!(read_installs(&paths).unwrap()["skill-a"], json!(["codex"]));
        assert!(!Path::new(&paths.storage_files.skills).exists());
        assert!(!Path::new(&paths.storage_files.skill_groups).exists());
        assert!(!Path::new(&paths.storage_files.skill_repositories).exists());
        assert!(!Path::new(&paths.storage_files.skill_repository_cache).exists());
        assert!(!Path::new(&paths.storage_files.installs).exists());
        assert!(!trash_index.exists());

        write_groups(&paths, &[json!({"id": "group-b"})]).unwrap();
        assert_eq!(read_groups(&paths).unwrap()[0]["id"], "group-b");
    }

    fn test_paths(name: &str) -> crate::core::paths::AppPaths {
        let root =
            std::env::temp_dir().join(format!("monkey-thief-skill-{name}-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(&root);
        std::fs::create_dir_all(&paths.storage_dir).unwrap();
        paths
    }

    #[test]
    fn migrates_payload_tables_and_merges_local_state() {
        let paths = test_paths("structured-migration");
        let mut connection = crate::core::database::open(&paths).unwrap();
        for (table, key, payload) in [
            (
                "skills",
                "skill-a",
                json!({"id":"stable-id","name":"skill-a","description":"说明","content":"正文","installedTargets":[],"tags":["a","b"],"disabled":true}),
            ),
            (
                "skill_groups",
                "group-a",
                json!({"id":"group-a","name":"分组","skillIds":["stable-id"]}),
            ),
            (
                "skill_repositories",
                "repo-a",
                json!({"id":"repo-a","owner":"owner","repository":"repo","branch":"main","rootPath":"skills"}),
            ),
            (
                "skill_repository_cache",
                "repo-a",
                json!({"id":"repo-a","status":"ready","skills":[{"name":"cached-skill","path":"SKILL.md"}],"lastSyncedAt":123}),
            ),
        ] {
            connection.execute_batch(&format!("CREATE TABLE {table}(item_key TEXT PRIMARY KEY, sort_order INTEGER NOT NULL, payload_json TEXT NOT NULL)")).unwrap();
            connection
                .execute(
                    &format!("INSERT INTO {table} VALUES (?1,3,?2)"),
                    rusqlite::params![key, payload.to_string()],
                )
                .unwrap();
        }
        connection.execute_batch(r#"CREATE TABLE skill_installs(skill_name TEXT PRIMARY KEY, payload_json TEXT);
            INSERT INTO skill_installs VALUES ('skill-a','["codex"]');
            CREATE TABLE skill_trash(item_key TEXT PRIMARY KEY, sort_order INTEGER, payload_json TEXT);
            CREATE TABLE skill_schema_migrations(version INTEGER);"#).unwrap();
        connection.execute("INSERT INTO skill_trash VALUES ('trash-a',0,?1)", [json!({"id":"trash-a","name":"old-skill","deletedAt":100,"trashPath":"missing","sourcePath":"missing-source"}).to_string()]).unwrap();
        super::migrate_connection(&mut connection).unwrap();
        super::migrate_connection(&mut connection).unwrap();
        let skills = read_skills(&paths).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0]["id"], "stable-id");
        assert_eq!(skills[0]["installedTargets"], json!(["codex"]));
        assert_eq!(skills[0]["tags"], json!(["a", "b"]));
        assert_eq!(skills[0]["disabled"], true);
        assert_eq!(
            read_groups(&paths).unwrap()[0]["skillIds"],
            json!(["stable-id"])
        );
        assert_eq!(
            super::read_repositories(&paths).unwrap()[0]["branch"],
            "main"
        );
        assert_eq!(
            read_repository_cache(&paths).unwrap()[0]["skills"][0]["name"],
            "cached-skill"
        );
        let trash = super::read_trash(&paths).unwrap();
        assert_eq!(
            trash[0]["expiresAt"],
            json!(100 + super::SKILL_TRASH_RETENTION_MS)
        );
        for table in [
            "skills",
            "skill_groups",
            "skill_repositories",
            "skill_repository_cache",
        ] {
            let columns = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(!columns.iter().any(|column| column == "payload_json"));
        }
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('skill_trash','skill_installs','skill_schema_migrations')", [], |row| row.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(
            connection
                .query_row(
                    "SELECT sort_order FROM skills WHERE item_key='skill-a'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
    }

    #[test]
    fn failed_migration_rolls_back_legacy_schema_and_rows() {
        let paths = test_paths("migration-rollback");
        let mut connection = crate::core::database::open(&paths).unwrap();
        connection.execute_batch("CREATE TABLE skills(item_key TEXT PRIMARY KEY, sort_order INTEGER, payload_json TEXT);
            INSERT INTO skills VALUES ('a',0,'invalid json');").unwrap();
        assert!(super::migrate_connection(&mut connection).is_err());
        assert_eq!(
            connection
                .query_row("SELECT payload_json FROM skills", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "invalid json"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='skills_legacy_structured'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn soft_delete_and_restore_keep_metadata_without_separate_tables() {
        let paths = test_paths("soft-delete");
        let source = Path::new(&paths.skills_dir).join("a");
        let skill = json!({"id":"stable-id","name":"a","content":"正文","disabled":true,"sourcePath":source.to_string_lossy(),"installedTargets":["codex"]});
        super::write_skills(&paths, &[skill]).unwrap();
        super::write_trash(&paths,&[json!({"id":"trash-a","name":"a","sourcePath":source.to_string_lossy(),"trashPath":"trash-path","deletedAt":100})]).unwrap();
        assert!(read_skills(&paths).unwrap().is_empty());
        super::write_skills(&paths, &[]).unwrap();
        assert_eq!(super::read_trash(&paths).unwrap().len(), 1);
        super::restore_trashed_skill(&paths, "trash-a", &source.to_string_lossy()).unwrap();
        let skills = read_skills(&paths).unwrap();
        assert_eq!(skills[0]["id"], "stable-id");
        assert!(skills[0].get("content").is_none());
        assert_eq!(skills[0]["disabled"], true);
        assert_eq!(skills[0]["installedTargets"], json!([]));
        assert!(super::read_trash(&paths).unwrap().is_empty());
    }
    #[test]
    fn migrates_structured_cache_and_keeps_group_membership_during_refresh() {
        let paths = test_paths("structured-v2");
        let mut connection = crate::core::database::open(&paths).unwrap();
        for table in [
            "skills",
            "skill_groups",
            "skill_repositories",
            "skill_repository_cache",
        ] {
            super::create_table(&connection, table).unwrap();
        }
        connection.execute_batch("ALTER TABLE skills DROP COLUMN group_id;
            ALTER TABLE skills ADD COLUMN content TEXT DEFAULT '';
            ALTER TABLE skills ADD COLUMN install_states TEXT DEFAULT '{}';
            INSERT INTO skills(item_key,sort_order,id,name,description,content) VALUES ('a',0,'stable-id','a','描述','正文');
            ALTER TABLE skill_groups ADD COLUMN skill_ids TEXT DEFAULT '';
            INSERT INTO skill_groups(item_key,sort_order,id,name,skill_ids) VALUES ('g',0,'g','分组','stable-id');").unwrap();
        connection.execute("INSERT INTO skill_repository_cache(item_key,sort_order,id,skills) VALUES ('repo',0,'repo',?1)",
            [json!([{"name":"demo","description":"描述","content":"正文"}]).to_string()]).unwrap();
        super::migrate_connection(&mut connection).unwrap();
        super::migrate_connection(&mut connection).unwrap();
        let skill = read_skills(&paths).unwrap().remove(0);
        assert_eq!(skill["groupId"], "g");
        assert!(skill.get("content").is_none());
        assert!(skill.get("installStates").is_none());
        assert!(read_repository_cache(&paths).unwrap()[0]["skills"][0]
            .get("content")
            .is_none());
        super::write_skills(
            &paths,
            &[json!({"id":"stable-id","name":"a","description":"新描述"})],
        )
        .unwrap();
        assert_eq!(
            read_groups(&paths).unwrap()[0]["skillIds"],
            json!(["stable-id"])
        );
        assert!(connection.prepare("SELECT content FROM skills").is_err());
        assert!(connection
            .prepare("SELECT install_states FROM skills")
            .is_err());
        assert!(connection
            .prepare("SELECT skill_ids FROM skill_groups")
            .is_err());
        super::write_groups(&paths, &[]).unwrap();
        assert_eq!(read_skills(&paths).unwrap()[0]["groupId"], "");
    }
}

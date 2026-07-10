use crate::core::database;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection, Transaction};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 1;

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = database::open(paths)?;
    create_schema(&connection)?;
    migrate_legacy_json(paths, &mut connection)?;
    remove_legacy_files(paths)
}

pub fn read_skills(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "skills")
}

pub fn write_skills(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "skills", items, &["name", "id", "sourcePath"])
}

pub fn read_groups(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "skill_groups")
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
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut statement =
        connection.prepare("SELECT skill_name, payload_json FROM skill_installs ORDER BY skill_name")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut installs = Map::new();

    for row in rows {
        let (skill_name, payload) = row?;
        installs.insert(skill_name, serde_json::from_str(&payload)?);
    }
    Ok(installs)
}

pub fn write_installs(paths: &AppPaths, installs: &Map<String, Value>) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction()?;

    transaction.execute("DELETE FROM skill_installs", [])?;
    for (skill_name, payload) in installs {
        transaction.execute(
            "INSERT INTO skill_installs(skill_name, payload_json) VALUES (?1, ?2)",
            params![skill_name, serde_json::to_string(payload)?],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn read_trash(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "skill_trash")
}

pub fn write_trash(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "skill_trash", items, &["id", "trashPath"])
}

fn create_schema(connection: &Connection) -> Result<(), ManagerError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS skill_schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skills (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skill_groups (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skill_repositories (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skill_repository_cache (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skill_installs (
           skill_name TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skill_trash (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO skill_schema_migrations(version, applied_at)
         VALUES (?1, ?2)",
        params![SCHEMA_VERSION, now_millis() as i64],
    )?;
    Ok(())
}

fn read_collection(paths: &AppPaths, table: &str) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let sql = format!(
        "SELECT payload_json FROM {table} ORDER BY sort_order ASC, item_key ASC"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut items = Vec::new();

    for row in rows {
        items.push(serde_json::from_str(&row?)?);
    }
    Ok(items)
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

fn replace_collection(
    transaction: &Transaction<'_>,
    table: &str,
    items: &[Value],
    key_fields: &[&str],
) -> Result<(), ManagerError> {
    transaction.execute(&format!("DELETE FROM {table}"), [])?;
    let sql = format!(
        "INSERT INTO {table}(item_key, sort_order, payload_json) VALUES (?1, ?2, ?3)"
    );

    for (index, item) in items.iter().enumerate() {
        let item_key = collection_key(item, key_fields, index);
        transaction.execute(
            &sql,
            params![item_key, index as i64, serde_json::to_string(item)?],
        )?;
    }
    Ok(())
}

fn migrate_legacy_json(
    paths: &AppPaths,
    connection: &mut Connection,
) -> Result<(), ManagerError> {
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

    let skills = read_array_file(legacy_files[0])?;
    let groups = read_array_file(legacy_files[1])?;
    let repositories = read_array_file(legacy_files[2])?;
    let repository_cache = read_array_file(legacy_files[3])?;
    let installs = read_object_file(legacy_files[4])?;
    let trash = read_array_file(legacy_files[5])?;
    let transaction = connection.transaction()?;

    if legacy_files[0].exists() {
        replace_collection(&transaction, "skills", &skills, &["name", "id", "sourcePath"])?;
    }
    if legacy_files[1].exists() {
        replace_collection(&transaction, "skill_groups", &groups, &["id"])?;
    }
    if legacy_files[2].exists() {
        replace_collection(
            &transaction,
            "skill_repositories",
            &repositories,
            &["id"],
        )?;
    }
    if legacy_files[3].exists() {
        replace_collection(
            &transaction,
            "skill_repository_cache",
            &repository_cache,
            &["id"],
        )?;
    }
    if legacy_files[4].exists() {
        transaction.execute("DELETE FROM skill_installs", [])?;
        for (skill_name, payload) in installs {
            transaction.execute(
                "INSERT INTO skill_installs(skill_name, payload_json) VALUES (?1, ?2)",
                params![skill_name, serde_json::to_string(&payload)?],
            )?;
        }
    }
    if legacy_files[5].exists() {
        replace_collection(&transaction, "skill_trash", &trash, &["id", "trashPath"])?;
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

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{read_groups, read_installs, read_repository_cache, read_skills, write_groups};
    use crate::core::paths::resolve_app_paths;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn migrates_skill_json_into_main_database() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-skill-store-{}",
            std::process::id()
        ));
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
            serde_json::to_string(&json!([{"id": "group-a", "skillIds": ["skill-a"]}]))
                .unwrap(),
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
}

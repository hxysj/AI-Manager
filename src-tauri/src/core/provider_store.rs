use crate::core::database;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 1;

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = database::open(paths)?;
    create_schema(&connection)?;
    migrate_legacy_json(paths, &mut connection)
}

pub fn read_providers(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "providers")
}

pub fn read_models(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "provider_models")
}

pub fn write_models(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "provider_models", items, &["id"])
}

pub fn read_profiles(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "provider_runtime_profiles")
}

pub fn write_profiles(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "provider_runtime_profiles", items, &["cli", "id"])
}

pub fn read_keys(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    read_map(paths, "provider_keys", "provider_id")
}

pub fn write_keys(paths: &AppPaths, items: &Map<String, Value>) -> Result<(), ManagerError> {
    write_map(paths, "provider_keys", "provider_id", items)
}

pub fn read_runtime_state(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    read_map(paths, "provider_runtime_state", "cli")
}

pub fn write_runtime_state(
    paths: &AppPaths,
    items: &Map<String, Value>,
) -> Result<(), ManagerError> {
    write_map(paths, "provider_runtime_state", "cli", items)
}

pub fn read_instances(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "provider_instances")
}

pub fn write_instances(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "provider_instances", items, &["id", "providerId"])
}

pub fn read_codex_accounts(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_collection(paths, "codex_accounts")
}

pub fn write_codex_accounts(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    write_collection(paths, "codex_accounts", items, &["id", "accountId"])
}

pub fn read_active_codex_account_id(paths: &AppPaths) -> Result<String, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;

    Ok(connection
        .query_row(
            "SELECT account_id FROM codex_active_account WHERE singleton_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_default())
}

pub fn write_active_codex_account_id(
    paths: &AppPaths,
    account_id: &str,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    connection.execute(
        "INSERT INTO codex_active_account(singleton_id, account_id) VALUES (1, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET account_id = excluded.account_id",
        params![account_id],
    )?;
    Ok(())
}

pub fn write_provider_bundle(
    paths: &AppPaths,
    providers: &[Value],
    models: &[Value],
    profiles: &[Value],
    keys: &Map<String, Value>,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    // Provider 及其模型、Profile、密钥必须一次提交，防止关联数据出现部分更新。
    let transaction = connection.transaction()?;

    replace_collection(&transaction, "providers", providers, &["id"])?;
    replace_collection(&transaction, "provider_models", models, &["id"])?;
    replace_collection(
        &transaction,
        "provider_runtime_profiles",
        profiles,
        &["cli", "id"],
    )?;
    replace_map(&transaction, "provider_keys", "provider_id", keys)?;
    transaction.commit()?;
    Ok(())
}

fn create_schema(connection: &Connection) -> Result<(), ManagerError> {
    // Provider 配置、模型和 Codex 账号属于持久化数据；其余表是本机运行态，备份主库时会清空。
    // provider_keys 不进入数据库快照，仍由数据备份中的独立加密字段保存。
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS provider_schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS providers (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS provider_models (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS provider_runtime_profiles (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS provider_keys (
           provider_id TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS provider_runtime_state (
           cli TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS provider_instances (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS codex_accounts (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS codex_active_account (
           singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
           account_id TEXT NOT NULL
         );",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO provider_schema_migrations(version, applied_at)
         VALUES (?1, ?2)",
        params![SCHEMA_VERSION, now_millis()],
    )?;
    Ok(())
}

fn read_collection(paths: &AppPaths, table: &str) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let sql = format!("SELECT payload_json FROM {table} ORDER BY sort_order, item_key");
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
    let sql =
        format!("INSERT INTO {table}(item_key, sort_order, payload_json) VALUES (?1, ?2, ?3)");

    for (index, item) in items.iter().enumerate() {
        transaction.execute(
            &sql,
            params![
                collection_key(item, key_fields, index),
                index as i64,
                serde_json::to_string(item)?
            ],
        )?;
    }
    Ok(())
}

fn read_map(
    paths: &AppPaths,
    table: &str,
    key_column: &str,
) -> Result<Map<String, Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let sql = format!("SELECT {key_column}, payload_json FROM {table} ORDER BY {key_column}");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut items = Map::new();

    for row in rows {
        let (key, payload) = row?;
        items.insert(key, serde_json::from_str(&payload)?);
    }
    Ok(items)
}

fn write_map(
    paths: &AppPaths,
    table: &str,
    key_column: &str,
    items: &Map<String, Value>,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction()?;

    replace_map(&transaction, table, key_column, items)?;
    transaction.commit()?;
    Ok(())
}

fn replace_map(
    transaction: &Transaction<'_>,
    table: &str,
    key_column: &str,
    items: &Map<String, Value>,
) -> Result<(), ManagerError> {
    transaction.execute(&format!("DELETE FROM {table}"), [])?;
    let sql = format!("INSERT INTO {table}({key_column}, payload_json) VALUES (?1, ?2)");

    for (key, payload) in items {
        transaction.execute(&sql, params![key, serde_json::to_string(payload)?])?;
    }
    Ok(())
}

fn migrate_legacy_json(paths: &AppPaths, connection: &mut Connection) -> Result<(), ManagerError> {
    let legacy_files = [
        Path::new(&paths.storage_files.providers),
        Path::new(&paths.storage_files.runtime_models),
        Path::new(&paths.storage_files.runtime_profiles),
        Path::new(&paths.storage_files.runtime_provider_keys),
        Path::new(&paths.storage_files.runtime_provider_state),
        Path::new(&paths.storage_files.codex_provider_instances),
        Path::new(&paths.storage_files.codex_accounts),
        Path::new(&paths.storage_files.codex_active_account_id),
    ];

    if !legacy_files.iter().any(|path| path.exists()) {
        return Ok(());
    }

    let providers = read_array_file(legacy_files[0])?;
    let models = read_array_file(legacy_files[1])?;
    let profiles = read_array_file(legacy_files[2])?;
    let keys = read_object_file(legacy_files[3])?;
    let runtime_state = read_object_file(legacy_files[4])?;
    let instances = read_array_file(legacy_files[5])?;
    let accounts = read_array_file(legacy_files[6])?;
    let active_account_id = read_json_file(legacy_files[7])?
        .and_then(|value| value.as_str().map(str::trim).map(ToString::to_string))
        .unwrap_or_default();
    // 所有旧文件共用一个迁移事务，任一数据写入失败都会整体回滚。
    let transaction = connection.transaction()?;

    if legacy_files[0].exists() {
        replace_collection(&transaction, "providers", &providers, &["id"])?;
    }
    if legacy_files[1].exists() {
        replace_collection(&transaction, "provider_models", &models, &["id"])?;
    }
    if legacy_files[2].exists() {
        replace_collection(
            &transaction,
            "provider_runtime_profiles",
            &profiles,
            &["cli", "id"],
        )?;
    }
    if legacy_files[3].exists() {
        replace_map(&transaction, "provider_keys", "provider_id", &keys)?;
    }
    if legacy_files[4].exists() {
        replace_map(
            &transaction,
            "provider_runtime_state",
            "cli",
            &runtime_state,
        )?;
    }
    if legacy_files[5].exists() {
        replace_collection(
            &transaction,
            "provider_instances",
            &instances,
            &["id", "providerId"],
        )?;
    }
    if legacy_files[6].exists() {
        replace_collection(
            &transaction,
            "codex_accounts",
            &accounts,
            &["id", "accountId"],
        )?;
    }
    if legacy_files[7].exists() {
        transaction.execute(
            "INSERT INTO codex_active_account(singleton_id, account_id) VALUES (1, ?1)
             ON CONFLICT(singleton_id) DO UPDATE SET account_id = excluded.account_id",
            params![active_account_id],
        )?;
    }
    transaction.commit()?;
    // 只有迁移事务成功提交后才删除旧 JSON，避免迁移失败时丢失原始数据。
    remove_legacy_files(paths)
}

fn remove_legacy_files(paths: &AppPaths) -> Result<(), ManagerError> {
    for path in [
        PathBuf::from(&paths.storage_files.providers),
        PathBuf::from(&paths.storage_files.runtime_models),
        PathBuf::from(&paths.storage_files.runtime_profiles),
        PathBuf::from(&paths.storage_files.runtime_provider_keys),
        PathBuf::from(&paths.storage_files.runtime_provider_state),
        PathBuf::from(&paths.storage_files.codex_provider_instances),
        PathBuf::from(&paths.storage_files.codex_accounts),
        PathBuf::from(&paths.storage_files.codex_active_account_id),
    ] {
        remove_file_family(&path)?;
    }
    Ok(())
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

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        read_active_codex_account_id, read_codex_accounts, read_instances, read_keys, read_models,
        read_profiles, read_providers, read_runtime_state,
    };
    use crate::core::paths::resolve_app_paths;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn migrates_provider_json_into_main_database() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-provider-store-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        std::fs::create_dir_all(&paths.storage_dir).unwrap();

        for (path, payload) in [
            (
                &paths.storage_files.providers,
                json!([{"id": "provider-a"}]),
            ),
            (
                &paths.storage_files.runtime_models,
                json!([{"id": "model-a", "providerId": "provider-a"}]),
            ),
            (
                &paths.storage_files.runtime_profiles,
                json!([{"id": "codex", "cli": "codex", "providerId": "provider-a"}]),
            ),
            (
                &paths.storage_files.runtime_provider_keys,
                json!({"provider-a": "encrypted-key"}),
            ),
            (
                &paths.storage_files.runtime_provider_state,
                json!({"codex": {"status": "SYNCED"}}),
            ),
            (
                &paths.storage_files.codex_provider_instances,
                json!([{"id": "provider-a", "providerId": "provider-a"}]),
            ),
            (
                &paths.storage_files.codex_accounts,
                json!([{"id": "account-a", "email": "test@example.com"}]),
            ),
            (
                &paths.storage_files.codex_active_account_id,
                json!("account-a"),
            ),
        ] {
            std::fs::write(path, serde_json::to_string(&payload).unwrap()).unwrap();
        }

        assert_eq!(read_providers(&paths).unwrap().len(), 1);
        assert_eq!(read_models(&paths).unwrap().len(), 1);
        assert_eq!(read_profiles(&paths).unwrap().len(), 1);
        assert_eq!(read_keys(&paths).unwrap()["provider-a"], "encrypted-key");
        assert_eq!(
            read_runtime_state(&paths).unwrap()["codex"]["status"],
            "SYNCED"
        );
        assert_eq!(read_instances(&paths).unwrap().len(), 1);
        assert_eq!(read_codex_accounts(&paths).unwrap().len(), 1);
        assert_eq!(read_active_codex_account_id(&paths).unwrap(), "account-a");

        for path in [
            &paths.storage_files.providers,
            &paths.storage_files.runtime_models,
            &paths.storage_files.runtime_profiles,
            &paths.storage_files.runtime_provider_keys,
            &paths.storage_files.runtime_provider_state,
            &paths.storage_files.codex_provider_instances,
            &paths.storage_files.codex_accounts,
            &paths.storage_files.codex_active_account_id,
        ] {
            assert!(!Path::new(path).exists());
        }
    }
}

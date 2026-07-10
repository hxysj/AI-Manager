use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection, OptionalExtension, MAIN_DB};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

// 这些表只描述当前设备的运行状态，备份时仅保留表结构。
const BACKUP_EXCLUDED_TABLES: [&str; 12] = [
    "usage_sessions",
    "usage_request_records",
    "skill_repository_cache",
    "skill_installs",
    "skill_trash",
    "rule_profiles",
    "rule_runtime_state",
    "provider_runtime_profiles",
    "provider_runtime_state",
    "provider_keys",
    "provider_instances",
    "codex_active_account",
];

// 数据库结构和迁移版本由当前应用维护，恢复时不能被旧备份回退。
const RESTORE_EXCLUDED_TABLES: [&str; 6] = [
    "app_metadata",
    "schema_migrations",
    "usage_metadata",
    "skill_schema_migrations",
    "rule_schema_migrations",
    "provider_schema_migrations",
];

#[derive(Debug, PartialEq, Eq)]
pub struct RestoreTableDifference {
    pub table: String,
    pub current_rows: i64,
    pub backup_rows: i64,
    pub current_only_rows: i64,
    pub backup_only_rows: i64,
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    let connection = open(paths)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_metadata (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );",
    )?;
    Ok(())
}

pub fn open(paths: &AppPaths) -> Result<Connection, ManagerError> {
    if let Some(parent) = Path::new(&paths.storage_files.database).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&paths.storage_files.database)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(connection)
}

pub fn backup(paths: &AppPaths) -> Result<Vec<u8>, ManagerError> {
    initialize(paths)?;
    std::fs::create_dir_all(&paths.temp_dir)?;
    let snapshot_path = Path::new(&paths.temp_dir).join(format!(
        "ai-manager-backup-{}-{}.db",
        std::process::id(),
        now_millis()
    ));
    let result = (|| {
        let connection = open(paths)?;
        connection.backup(MAIN_DB, &snapshot_path, None)?;
        strip_non_backup_tables(&snapshot_path)?;
        Ok(std::fs::read(&snapshot_path)?)
    })();

    if snapshot_path.exists() {
        std::fs::remove_file(snapshot_path)?;
    }
    result
}

fn strip_non_backup_tables(snapshot_path: &Path) -> Result<(), ManagerError> {
    let mut connection = Connection::open(snapshot_path)?;
    let transaction = connection.transaction()?;

    for table in BACKUP_EXCLUDED_TABLES {
        let exists = transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM sqlite_master
               WHERE type = 'table' AND name = ?1
             )",
            params![table],
            |row| row.get::<_, bool>(0),
        )?;

        if exists {
            transaction.execute(&format!("DELETE FROM {table}"), [])?;
        }
    }
    // enabled 只表示当前设备是否启用 Provider，不能随跨设备备份恢复。
    strip_provider_enabled(&transaction)?;

    transaction.commit()?;
    connection.execute_batch("VACUUM;")?;
    Ok(())
}

fn strip_provider_enabled(transaction: &rusqlite::Transaction<'_>) -> Result<(), ManagerError> {
    let exists = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_master
           WHERE type = 'table' AND name = 'providers'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;

    if !exists {
        return Ok(());
    }

    let rows = {
        let mut statement = transaction.prepare("SELECT item_key, payload_json FROM providers")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    for (item_key, payload) in rows {
        let mut provider: serde_json::Value = serde_json::from_str(&payload)?;

        if let Some(provider) = provider.as_object_mut() {
            provider.remove("enabled");
        }
        transaction.execute(
            "UPDATE providers SET payload_json = ?1 WHERE item_key = ?2",
            params![serde_json::to_string(&provider)?, item_key],
        )?;
    }
    Ok(())
}

pub fn preview_restore(
    paths: &AppPaths,
    content: &[u8],
) -> Result<Vec<RestoreTableDifference>, ManagerError> {
    with_restore_snapshot(paths, content, |snapshot_path| {
        let connection = open_restore_connection(paths, snapshot_path)?;
        let mut differences = Vec::new();

        for table in read_restorable_tables(&connection)? {
            let columns = read_common_columns(&connection, &table)?;
            let current_rows = count_table_rows(&connection, "main", &table)?;
            let backup_rows = count_table_rows(&connection, "restore_source", &table)?;
            let current_only_rows =
                count_table_difference(&connection, "main", "restore_source", &table, &columns)?;
            let backup_only_rows =
                count_table_difference(&connection, "restore_source", "main", &table, &columns)?;

            if current_only_rows > 0 || backup_only_rows > 0 {
                differences.push(RestoreTableDifference {
                    table,
                    current_rows,
                    backup_rows,
                    current_only_rows,
                    backup_only_rows,
                });
            }
        }

        Ok(differences)
    })
}

pub fn restore_selected(
    paths: &AppPaths,
    content: &[u8],
    selected_tables: &[String],
) -> Result<(), ManagerError> {
    let selected_tables = selected_tables.iter().cloned().collect::<HashSet<_>>();
    restore_tables(paths, content, &selected_tables)
}

fn restore_tables(
    paths: &AppPaths,
    content: &[u8],
    selected_tables: &HashSet<String>,
) -> Result<(), ManagerError> {
    with_restore_snapshot(paths, content, |snapshot_path| {
        let mut connection = open_restore_connection(paths, snapshot_path)?;
        let restore_tables = read_restorable_tables(&connection)?
            .into_iter()
            .filter(|table| selected_tables.contains(table))
            .collect::<Vec<_>>();
        let restores_usage_logs = restore_tables.iter().any(|table| table == "usage_logs");
        let transaction = connection.transaction()?;

        // 只替换用户选择的业务表，保留当前库结构和本机运行态数据。
        for table in restore_tables {
            if table == "providers" {
                restore_providers(&transaction)?;
            } else if table == "codex_accounts" {
                restore_codex_accounts(&transaction)?;
            } else if table == "rule_prompts" {
                restore_rule_prompts(&transaction)?;
            } else {
                restore_table(&transaction, &table)?;
            }
        }
        if restores_usage_logs {
            transaction.execute(
                "UPDATE usage_metadata
                 SET value = CAST(value AS INTEGER) + 1
                 WHERE key = 'revision'",
                [],
            )?;
        }
        transaction.execute(
            "INSERT INTO app_metadata(key, value) VALUES ('restored_at', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![now_millis().to_string()],
        )?;
        transaction.commit()?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    })
}

fn with_restore_snapshot<T>(
    paths: &AppPaths,
    content: &[u8],
    action: impl FnOnce(&Path) -> Result<T, ManagerError>,
) -> Result<T, ManagerError> {
    initialize(paths)?;
    std::fs::create_dir_all(&paths.temp_dir)?;
    let snapshot_path = Path::new(&paths.temp_dir).join(format!(
        "ai-manager-restore-{}-{}.db",
        std::process::id(),
        now_millis()
    ));
    std::fs::write(&snapshot_path, content)?;
    let result = (|| {
        validate_restore_snapshot(&snapshot_path)?;
        action(&snapshot_path)
    })();

    if snapshot_path.exists() {
        std::fs::remove_file(snapshot_path)?;
    }
    result
}

fn validate_restore_snapshot(snapshot_path: &Path) -> Result<(), ManagerError> {
    let source = Connection::open(snapshot_path)?;
    let integrity =
        source.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;

    if integrity != "ok" {
        return Err(ManagerError::System(format!(
            "主数据库备份校验失败：{integrity}"
        )));
    }

    let has_metadata = source.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_master
           WHERE type = 'table' AND name = 'app_metadata'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;

    if !has_metadata {
        return Err(ManagerError::System(
            "主数据库备份缺少应用元数据表。".to_string(),
        ));
    }
    Ok(())
}

fn open_restore_connection(
    paths: &AppPaths,
    snapshot_path: &Path,
) -> Result<Connection, ManagerError> {
    let connection = open(paths)?;
    connection.execute(
        "ATTACH DATABASE ?1 AS restore_source",
        params![snapshot_path.to_string_lossy().to_string()],
    )?;
    Ok(connection)
}

fn read_restorable_tables(connection: &Connection) -> Result<Vec<String>, ManagerError> {
    let source_tables = read_table_names(connection, "restore_source")?
        .into_iter()
        .collect::<HashSet<_>>();

    Ok(read_table_names(connection, "main")?
        .into_iter()
        .filter(|table| {
            source_tables.contains(table)
                && !BACKUP_EXCLUDED_TABLES.contains(&table.as_str())
                && !RESTORE_EXCLUDED_TABLES.contains(&table.as_str())
        })
        .collect())
}

fn read_table_names(connection: &Connection, schema: &str) -> Result<Vec<String>, ManagerError> {
    let sql = format!(
        "SELECT name FROM {schema}.sqlite_schema
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn restore_table(transaction: &rusqlite::Transaction<'_>, table: &str) -> Result<(), ManagerError> {
    let columns = read_common_columns(transaction, table)?
        .into_iter()
        .map(|column| quote_identifier(&column))
        .collect::<Vec<_>>()
        .join(", ");
    let table = quote_identifier(table);
    transaction.execute(&format!("DELETE FROM main.{table}"), [])?;
    transaction.execute(
        &format!(
            "INSERT INTO main.{table} ({columns})
             SELECT {columns} FROM restore_source.{table}"
        ),
        [],
    )?;
    Ok(())
}

fn restore_providers(transaction: &rusqlite::Transaction<'_>) -> Result<(), ManagerError> {
    let current_enabled = {
        let mut statement = transaction.prepare("SELECT item_key, payload_json FROM providers")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut enabled = HashMap::new();

        for (item_key, payload) in rows {
            let provider: serde_json::Value = serde_json::from_str(&payload)?;
            if let Some(value) = provider.get("enabled").and_then(serde_json::Value::as_bool) {
                enabled.insert(item_key, value);
            }
        }
        enabled
    };

    restore_table(transaction, "providers")?;
    let restored = {
        let mut statement = transaction.prepare("SELECT item_key, payload_json FROM providers")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    // enabled 属于本机状态：已有 Provider 保留当前值，备份新增的 Provider 默认禁用。
    for (item_key, payload) in restored {
        let mut provider: serde_json::Value = serde_json::from_str(&payload)?;
        let provider = provider
            .as_object_mut()
            .ok_or_else(|| ManagerError::System(format!("Provider {item_key} 的数据格式无效。")))?;
        provider.insert(
            "enabled".to_string(),
            serde_json::Value::Bool(current_enabled.get(&item_key).copied().unwrap_or(false)),
        );
        transaction.execute(
            "UPDATE providers SET payload_json = ?1 WHERE item_key = ?2",
            params![serde_json::to_string(&provider)?, item_key],
        )?;
    }

    let active_account_id = transaction
        .query_row(
            "SELECT account_id FROM codex_active_account WHERE singleton_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_default();
    if !active_account_id.is_empty()
        && !transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM codex_accounts WHERE item_key = ?1)",
            params![active_account_id],
            |row| row.get::<_, bool>(0),
        )?
    {
        // 当前活动账号已不在恢复后的账号表中，清空悬空引用。
        transaction.execute(
            "UPDATE codex_active_account SET account_id = '' WHERE singleton_id = 1",
            [],
        )?;
    }
    Ok(())
}

fn restore_rule_prompts(transaction: &rusqlite::Transaction<'_>) -> Result<(), ManagerError> {
    restore_table(transaction, "rule_prompts")?;
    let profiles = {
        let mut statement = transaction.prepare("SELECT cli, payload_json FROM rule_profiles")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    for (cli, payload) in profiles {
        let mut profile: serde_json::Value = serde_json::from_str(&payload)?;
        let active_prompt_id = profile
            .get("activePromptId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if active_prompt_id.is_empty()
            || transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM rule_prompts WHERE prompt_id = ?1)",
                params![active_prompt_id],
                |row| row.get::<_, bool>(0),
            )?
        {
            continue;
        }

        // 当前启用的 Rule 已不在恢复结果中，只清空悬空状态，不启用备份中的新 Rule。
        let profile = profile
            .as_object_mut()
            .ok_or_else(|| ManagerError::System(format!("Rule Profile {cli} 的数据格式无效。")))?;
        profile.insert(
            "activePromptId".to_string(),
            serde_json::Value::String(String::new()),
        );
        transaction.execute(
            "UPDATE rule_profiles SET payload_json = ?1 WHERE cli = ?2",
            params![serde_json::to_string(&profile)?, cli],
        )?;
    }
    Ok(())
}

fn restore_codex_accounts(transaction: &rusqlite::Transaction<'_>) -> Result<(), ManagerError> {
    let current_usage = {
        let mut statement =
            transaction.prepare("SELECT item_key, payload_json FROM codex_accounts")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut usage = HashMap::new();

        for (item_key, payload) in rows {
            let account: serde_json::Value = serde_json::from_str(&payload)?;
            if let Some(value) = account.get("usage").filter(|value| is_json_truthy(value)) {
                usage.insert(item_key, value.clone());
            }
        }
        usage
    };

    restore_table(transaction, "codex_accounts")?;
    let restored = {
        let mut statement =
            transaction.prepare("SELECT item_key, payload_json FROM codex_accounts")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    // usage 是当前设备刚刷新的运行数据，账号凭据恢复时优先保留当前值。
    for (item_key, payload) in restored {
        let Some(usage) = current_usage.get(&item_key) else {
            continue;
        };
        let mut account: serde_json::Value = serde_json::from_str(&payload)?;
        let account = account.as_object_mut().ok_or_else(|| {
            ManagerError::System(format!("Codex 账号 {item_key} 的数据格式无效。"))
        })?;
        account.insert("usage".to_string(), usage.clone());
        transaction.execute(
            "UPDATE codex_accounts SET payload_json = ?1 WHERE item_key = ?2",
            params![serde_json::to_string(&account)?, item_key],
        )?;
    }
    Ok(())
}

fn read_common_columns(connection: &Connection, table: &str) -> Result<Vec<String>, ManagerError> {
    let source_columns = read_table_columns(connection, "restore_source", table)?
        .into_iter()
        .collect::<HashSet<_>>();
    let columns = read_table_columns(connection, "main", table)?
        .into_iter()
        .filter(|column| source_columns.contains(column))
        .collect::<Vec<_>>();

    if columns.is_empty() {
        return Err(ManagerError::System(format!(
            "主数据库表 {table} 与备份表没有可恢复的共同字段。"
        )));
    }
    Ok(columns)
}

fn read_table_columns(
    connection: &Connection,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, ManagerError> {
    let sql = format!("PRAGMA {schema}.table_info({})", quote_identifier(table));
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn count_table_rows(
    connection: &Connection,
    schema: &str,
    table: &str,
) -> Result<i64, ManagerError> {
    let sql = format!("SELECT COUNT(*) FROM {schema}.{}", quote_identifier(table));
    Ok(connection.query_row(&sql, [], |row| row.get::<_, i64>(0))?)
}

fn count_table_difference(
    connection: &Connection,
    left_schema: &str,
    right_schema: &str,
    table: &str,
    columns: &[String],
) -> Result<i64, ManagerError> {
    let columns = columns
        .iter()
        .map(|column| comparison_column(table, column))
        .collect::<Vec<_>>()
        .join(", ");
    let table = quote_identifier(table);
    let sql = format!(
        "SELECT COUNT(*) FROM (
           SELECT {columns} FROM {left_schema}.{table}
           EXCEPT
           SELECT {columns} FROM {right_schema}.{table}
         )"
    );

    Ok(connection.query_row(&sql, [], |row| row.get::<_, i64>(0))?)
}

fn comparison_column(table: &str, column: &str) -> String {
    let column = quote_identifier(column);

    if column != "\"payload_json\"" {
        return column;
    }

    // 与旧 JSON 恢复一致，运行态字段不参与跨设备备份差异比较。
    let runtime_paths = "'$.createdAt', '$.updatedAt', '$.lastUpdatedAt', '$.lastSyncAt',
         '$.uploadedAt', '$.downloadedAt', '$.lastBackupAt', '$.created_at', '$.updated_at',
         '$.last_refresh', '$.token_updated_at'";
    match table {
        "providers" => format!("json_remove({column}, '$.enabled', {runtime_paths})"),
        "codex_accounts" => format!(
            "json_remove({column}, '$.usage', {runtime_paths},
             '$.auth.createdAt', '$.auth.updatedAt', '$.auth.lastUpdatedAt', '$.auth.lastSyncAt',
             '$.auth.created_at', '$.auth.updated_at', '$.auth.last_refresh', '$.auth.token_updated_at')"
        ),
        _ => column,
    }
}

fn is_json_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(value) => value.as_f64().is_none_or(|value| value != 0.0),
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => true,
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('\"', "\"\""))
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        backup, initialize, open, preview_restore, restore_selected, BACKUP_EXCLUDED_TABLES,
    };
    use crate::core::paths::resolve_app_paths;
    use rusqlite::Connection;
    use std::path::Path;

    #[test]
    fn backup_keeps_schema_and_clears_excluded_table_rows() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-main-database-backup-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        let connection = open(&paths).unwrap();
        connection
            .execute_batch(
                r#"CREATE TABLE retained_data(id INTEGER PRIMARY KEY);
                 INSERT INTO retained_data(id) VALUES (1);
                 CREATE TABLE usage_sessions(id INTEGER PRIMARY KEY);
                 CREATE TABLE usage_request_records(id INTEGER PRIMARY KEY);
                 CREATE TABLE skill_repository_cache(id INTEGER PRIMARY KEY);
                 CREATE TABLE skill_installs(id INTEGER PRIMARY KEY);
                 CREATE TABLE skill_trash(id INTEGER PRIMARY KEY);
                 CREATE TABLE rule_profiles(id INTEGER PRIMARY KEY);
                 CREATE TABLE rule_runtime_state(id INTEGER PRIMARY KEY);
                 CREATE TABLE provider_runtime_profiles(id INTEGER PRIMARY KEY);
                 CREATE TABLE provider_runtime_state(id INTEGER PRIMARY KEY);
                 CREATE TABLE provider_keys(id INTEGER PRIMARY KEY);
                 CREATE TABLE provider_instances(id INTEGER PRIMARY KEY);
                 CREATE TABLE codex_active_account(id INTEGER PRIMARY KEY);
                 CREATE TABLE providers(
                   item_key TEXT PRIMARY KEY,
                   sort_order INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 CREATE TABLE codex_accounts(
                   item_key TEXT PRIMARY KEY,
                   sort_order INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 INSERT INTO usage_sessions(id) VALUES (1);
                 INSERT INTO usage_request_records(id) VALUES (1);
                 INSERT INTO skill_repository_cache(id) VALUES (1);
                 INSERT INTO skill_installs(id) VALUES (1);
                 INSERT INTO skill_trash(id) VALUES (1);
                 INSERT INTO rule_profiles(id) VALUES (1);
                 INSERT INTO rule_runtime_state(id) VALUES (1);
                 INSERT INTO provider_runtime_profiles(id) VALUES (1);
                 INSERT INTO provider_runtime_state(id) VALUES (1);
                 INSERT INTO provider_keys(id) VALUES (1);
                 INSERT INTO provider_instances(id) VALUES (1);
                 INSERT INTO codex_active_account(id) VALUES (1);
                 INSERT INTO providers(item_key, sort_order, payload_json)
                   VALUES ('provider-a', 0, '{"id":"provider-a","enabled":true}');
                 INSERT INTO codex_accounts(item_key, sort_order, payload_json)
                   VALUES ('account-a', 0, '{"id":"account-a"}');"#,
            )
            .unwrap();
        drop(connection);

        let snapshot_path = root.join("snapshot.db");
        std::fs::write(&snapshot_path, backup(&paths).unwrap()).unwrap();
        let snapshot = Connection::open(snapshot_path).unwrap();

        assert_eq!(
            snapshot
                .query_row("SELECT COUNT(*) FROM retained_data", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        for table in BACKUP_EXCLUDED_TABLES {
            assert_eq!(
                snapshot
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0,
                "{table} 不应包含备份数据"
            );
        }
        let provider: serde_json::Value = serde_json::from_str(
            &snapshot
                .query_row(
                    "SELECT payload_json FROM providers WHERE item_key = 'provider-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert!(provider.get("enabled").is_none());
        assert_eq!(
            snapshot
                .query_row("SELECT COUNT(*) FROM codex_accounts", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn restore_selected_table_and_preserve_other_tables() {
        let source_root = std::env::temp_dir().join(format!(
            "monkey-thief-main-database-restore-source-{}",
            std::process::id()
        ));
        let target_root = std::env::temp_dir().join(format!(
            "monkey-thief-main-database-restore-target-{}",
            std::process::id()
        ));
        for root in [&source_root, &target_root] {
            if root.exists() {
                std::fs::remove_dir_all(root).unwrap();
            }
        }

        let source_paths = resolve_app_paths(Path::new(&source_root));
        initialize(&source_paths).unwrap();
        let source = open(&source_paths).unwrap();
        source
            .execute_batch(
                r#"CREATE TABLE retained_data(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE retained_other(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE usage_sessions(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY);
                 CREATE TABLE rule_prompts(
                   prompt_id TEXT PRIMARY KEY,
                   cli TEXT NOT NULL,
                   file_name TEXT NOT NULL,
                   updated_at INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 CREATE TABLE rule_profiles(
                   cli TEXT PRIMARY KEY,
                   payload_json TEXT NOT NULL
                 );
                 CREATE TABLE providers(
                   item_key TEXT PRIMARY KEY,
                   sort_order INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 CREATE TABLE codex_accounts(
                   item_key TEXT PRIMARY KEY,
                   sort_order INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 INSERT INTO retained_data(id, value) VALUES (1, 'backup');
                 INSERT INTO retained_other(id, value) VALUES (1, 'backup');
                 INSERT INTO usage_sessions(id, value) VALUES (1, 'backup');
                 INSERT INTO schema_migrations(version) VALUES (1);
                 INSERT INTO providers(item_key, sort_order, payload_json) VALUES
                   ('provider-a', 0, '{"id":"provider-a","name":"backup","enabled":true}'),
                   ('provider-b', 1, '{"id":"provider-b","name":"new","enabled":false}');
                 INSERT INTO codex_accounts(item_key, sort_order, payload_json) VALUES
                   ('account-a', 0, '{"id":"account-a","email":"backup@example.com","usage":{"source":"backup"},"updatedAt":1,"auth":{"token_updated_at":1}}'),
                   ('account-b', 1, '{"id":"account-b","email":"new@example.com","usage":{"source":"backup"},"updatedAt":1}'),
                   ('account-c', 2, '{"id":"account-c","email":"same@example.com","usage":{"source":"backup"},"updatedAt":1}');
                 INSERT INTO rule_prompts(prompt_id, cli, file_name, updated_at, payload_json) VALUES
                   ('rule-a', 'claude', 'rule-a.md', 1, '{"id":"rule-a","name":"backup"}'),
                   ('rule-b', 'claude', 'rule-b.md', 1, '{"id":"rule-b","name":"new"}');
                 INSERT INTO rule_profiles(cli, payload_json)
                   VALUES ('claude', '{"activePromptId":"rule-b"}');"#,
            )
            .unwrap();
        drop(source);
        let backup_content = backup(&source_paths).unwrap();

        let target_paths = resolve_app_paths(Path::new(&target_root));
        initialize(&target_paths).unwrap();
        let target = open(&target_paths).unwrap();
        target
            .execute_batch(
                r#"CREATE TABLE retained_data(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE retained_other(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE usage_sessions(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY);
                 CREATE TABLE providers(
                   item_key TEXT PRIMARY KEY,
                   sort_order INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 CREATE TABLE codex_accounts(
                   item_key TEXT PRIMARY KEY,
                   sort_order INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 CREATE TABLE codex_active_account(
                   singleton_id INTEGER PRIMARY KEY,
                   account_id TEXT NOT NULL
                 );
                 CREATE TABLE rule_prompts(
                   prompt_id TEXT PRIMARY KEY,
                   cli TEXT NOT NULL,
                   file_name TEXT NOT NULL,
                   updated_at INTEGER NOT NULL,
                   payload_json TEXT NOT NULL
                 );
                 CREATE TABLE rule_profiles(
                   cli TEXT PRIMARY KEY,
                   payload_json TEXT NOT NULL
                 );
                 INSERT INTO retained_data(id, value) VALUES (1, 'current');
                 INSERT INTO retained_other(id, value) VALUES (1, 'current');
                 INSERT INTO usage_sessions(id, value) VALUES (1, 'current');
                 INSERT INTO schema_migrations(version) VALUES (2);
                 INSERT INTO providers(item_key, sort_order, payload_json)
                   VALUES ('provider-a', 0, '{"id":"provider-a","name":"current","enabled":false}');
                 INSERT INTO codex_accounts(item_key, sort_order, payload_json) VALUES
                   ('account-a', 0, '{"id":"account-a","email":"current@example.com","usage":{"source":"current"},"updatedAt":2,"auth":{"token_updated_at":2}}'),
                   ('account-c', 2, '{"id":"account-c","email":"same@example.com","usage":{"source":"current"},"updatedAt":2}'),
                   ('account-local', 3, '{"id":"account-local","email":"local@example.com"}');
                 INSERT INTO codex_active_account(singleton_id, account_id)
                   VALUES (1, 'account-local');
                 INSERT INTO rule_prompts(prompt_id, cli, file_name, updated_at, payload_json) VALUES
                   ('rule-a', 'claude', 'rule-a.md', 2, '{"id":"rule-a","name":"current"}'),
                   ('rule-local', 'codex', 'rule-local.md', 2, '{"id":"rule-local","name":"local"}');
                 INSERT INTO rule_profiles(cli, payload_json) VALUES
                   ('claude', '{"activePromptId":"rule-a"}'),
                   ('codex', '{"activePromptId":"rule-local"}');"#,
            )
            .unwrap();
        drop(target);

        let differences = preview_restore(&target_paths, &backup_content).unwrap();
        let retained_difference = differences
            .iter()
            .find(|difference| difference.table == "retained_data")
            .unwrap();
        assert_eq!(retained_difference.current_rows, 1);
        assert_eq!(retained_difference.backup_rows, 1);
        assert_eq!(retained_difference.current_only_rows, 1);
        assert_eq!(retained_difference.backup_only_rows, 1);
        let provider_difference = differences
            .iter()
            .find(|difference| difference.table == "providers")
            .unwrap();
        assert_eq!(provider_difference.current_rows, 1);
        assert_eq!(provider_difference.backup_rows, 2);
        let account_difference = differences
            .iter()
            .find(|difference| difference.table == "codex_accounts")
            .unwrap();
        assert_eq!(account_difference.current_rows, 3);
        assert_eq!(account_difference.backup_rows, 3);
        assert_eq!(account_difference.current_only_rows, 2);
        assert_eq!(account_difference.backup_only_rows, 2);
        assert!(differences
            .iter()
            .any(|difference| difference.table == "rule_prompts"));
        assert!(!differences
            .iter()
            .any(|difference| difference.table == "rule_profiles"));

        restore_selected(
            &target_paths,
            &backup_content,
            &[
                "retained_data".to_string(),
                "providers".to_string(),
                "codex_accounts".to_string(),
                "rule_prompts".to_string(),
            ],
        )
        .unwrap();
        let restored = open(&target_paths).unwrap();
        assert_eq!(
            restored
                .query_row("SELECT value FROM retained_data WHERE id = 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "backup"
        );
        assert_eq!(
            restored
                .query_row("SELECT value FROM retained_other WHERE id = 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "current"
        );
        assert_eq!(
            restored
                .query_row("SELECT value FROM usage_sessions WHERE id = 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "current"
        );
        assert_eq!(
            restored
                .query_row("SELECT version FROM schema_migrations", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            2
        );
        let provider_a: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM providers WHERE item_key = 'provider-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(provider_a["name"], "backup");
        assert_eq!(provider_a["enabled"], false);
        let provider_b: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM providers WHERE item_key = 'provider-b'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(provider_b["enabled"], false);
        let account_a: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM codex_accounts WHERE item_key = 'account-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(account_a["email"], "backup@example.com");
        assert_eq!(account_a["usage"]["source"], "current");
        let account_b: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM codex_accounts WHERE item_key = 'account-b'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(account_b["usage"]["source"], "backup");
        assert_eq!(
            restored
                .query_row(
                    "SELECT account_id FROM codex_active_account WHERE singleton_id = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            ""
        );
        let claude_profile: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM rule_profiles WHERE cli = 'claude'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(claude_profile["activePromptId"], "rule-a");
        let codex_profile: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM rule_profiles WHERE cli = 'codex'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(codex_profile["activePromptId"], "");
        assert_eq!(
            restored
                .query_row(
                    "SELECT COUNT(*) FROM rule_prompts WHERE prompt_id = 'rule-b'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }
}

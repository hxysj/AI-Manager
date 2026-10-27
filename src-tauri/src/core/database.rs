use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use crate::core::provider_store::{
    local_default as provider_local_default, LOCAL_COLUMNS as PROVIDER_LOCAL_COLUMNS,
};
use crate::core::rule_store::LOCAL_COLUMNS as RULE_LOCAL_COLUMNS;
use crate::core::skill_store::{repository_local_default, REPOSITORY_LOCAL_COLUMNS};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

#[cfg(test)]
mod legacy_tests;
mod restore;
pub use restore::{preview_rows, row_choice_key, RestoreRowDifference};

// 仅跨设备恢复的业务数据进入备份，其他表只保留结构。
const BACKUP_INCLUDED_TABLES: [&str; 7] = [
    "providers",
    "skills",
    "skill_groups",
    "skill_repositories",
    "rules",
    "app_metadata",
    "usage_pricing_items",
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
    // WAL 在文件级持久化。已有连接无需重复切换；首次并发打开时可能发生
    // 读锁升级冲突，SQLite 的 busy_timeout 不会等待这类冲突，需要短暂重试。
    let started = std::time::Instant::now();
    loop {
        let result = connection
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .and_then(|mode| {
                if mode.eq_ignore_ascii_case("wal") {
                    Ok(())
                } else {
                    connection.pragma_update(None, "journal_mode", "WAL")
                }
            });
        match result {
            Ok(()) => break,
            Err(error)
                if matches!(
                    error.sqlite_error_code(),
                    Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
                ) && started.elapsed() < Duration::from_secs(5) =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    Ok(connection)
}

pub fn backup(paths: &AppPaths) -> Result<Vec<u8>, ManagerError> {
    crate::core::skill_store::initialize(paths)?;
    crate::core::usage_store::initialize(paths)?;
    crate::core::rule_store::initialize(paths)?;
    crate::core::provider_store::initialize(paths)?;
    std::fs::create_dir_all(&paths.temp_dir)?;
    cleanup_backup_snapshots(Path::new(&paths.temp_dir))?;
    let snapshot_path = Path::new(&paths.temp_dir).join(format!(
        "ai-manager-backup-{}-{}.db",
        std::process::id(),
        now_millis()
    ));
    let result = (|| {
        create_filtered_backup(paths, &snapshot_path)?;
        Ok(std::fs::read(&snapshot_path)?)
    })();

    if snapshot_path.exists() {
        std::fs::remove_file(snapshot_path)?;
    }
    result
}

fn cleanup_backup_snapshots(temp_dir: &Path) -> Result<(), ManagerError> {
    for entry in std::fs::read_dir(temp_dir)? {
        let path = entry?.path();
        let is_backup = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("ai-manager-backup-") && name.ends_with(".db"));
        if is_backup {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

fn create_filtered_backup(paths: &AppPaths, snapshot_path: &Path) -> Result<(), ManagerError> {
    let mut connection = Connection::open(snapshot_path)?;
    connection.pragma_update(None, "foreign_keys", false)?;
    connection.execute(
        "ATTACH DATABASE ?1 AS source",
        params![paths.storage_files.database.clone()],
    )?;

    let schema = {
        let mut statement = connection.prepare(
            "SELECT type, name, sql
             FROM source.sqlite_schema
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
             ORDER BY CASE type
               WHEN 'table' THEN 0
               WHEN 'index' THEN 1
               WHEN 'trigger' THEN 2
               WHEN 'view' THEN 3
               ELSE 4
             END, name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    for (_, _, sql) in schema
        .iter()
        .filter(|(object_type, _, _)| object_type == "table")
    {
        connection.execute_batch(sql)?;
    }

    let transaction = connection.transaction()?;
    for table in BACKUP_INCLUDED_TABLES {
        let columns = read_table_columns(&transaction, "source", table)?;
        if columns.is_empty() {
            continue;
        }
        let selected_columns = columns
            .iter()
            .map(|column| {
                if table == "skill_repositories"
                    && REPOSITORY_LOCAL_COLUMNS.contains(&column.as_str())
                {
                    match repository_local_default(column) {
                        serde_json::Value::String(value) => {
                            format!("'{}'", value.replace('\'', "''"))
                        }
                        value => value.to_string(),
                    }
                } else {
                    quote_identifier(column)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let columns = columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        transaction.execute(
            &format!(
                "INSERT INTO main.{table} ({columns})
                 SELECT {selected_columns} FROM source.{table} {filter}",
                filter = if table == "skills" {
                    "WHERE is_deleted = 0"
                } else {
                    restore_row_filter(table)
                },
                table = quote_identifier(table),
            ),
            [],
        )?;
    }

    // 本机启用态、安装态和用量不随业务数据跨设备恢复。
    transaction.execute_batch("UPDATE providers SET enabled=0,is_active=0,oauth_credentials='',
      profile_id='',selected_model=NULL,profile_base_url=NULL,profile_proxy=NULL,profile_env=NULL,profile_updated_at=NULL,
      synced_hash='',synced_at=0,runtime_path='',sync_status='NO_ACTIVE',
      instance_profile_dir='',instance_sessions_path='',instance_updated_at=0,quota=NULL;")?;
    transaction.execute_batch(
        "UPDATE skills SET disabled=1, installed_targets='', status='disabled',
         source_path='', entry_path='', repo_name='', is_deleted=0, deleted_at=0, trash_id='', trash_path='';"
    )?;
    transaction.execute_batch(
        "UPDATE rules SET enabled_clis='', claude_synced_hash='', claude_synced_at=0,
         codex_synced_hash='', codex_synced_at=0;",
    )?;

    transaction.commit()?;

    for (_, _, sql) in schema
        .iter()
        .filter(|(object_type, _, _)| object_type != "table")
    {
        connection.execute_batch(sql)?;
    }
    connection.execute_batch("DETACH DATABASE source; VACUUM;")?;
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
    restore_tables(paths, content, &selected_tables, None)
}

pub fn restore_merged(
    paths: &AppPaths,
    content: &[u8],
    choices: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), ManagerError> {
    let tables = BACKUP_INCLUDED_TABLES
        .iter()
        .map(|table| table.to_string())
        .collect();
    restore_tables(paths, content, &tables, Some(choices))
}

fn restore_tables(
    paths: &AppPaths,
    content: &[u8],
    selected_tables: &HashSet<String>,
    choices: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<(), ManagerError> {
    with_restore_snapshot(paths, content, |snapshot_path| {
        let mut connection = open_restore_connection(paths, snapshot_path)?;
        let restore_tables = read_restorable_tables(&connection)?
            .into_iter()
            .filter(|table| selected_tables.contains(table))
            .collect::<Vec<_>>();
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TEMP TABLE restore_choices(choice_key TEXT PRIMARY KEY);
            CREATE TEMP TABLE restore_applied(table_name TEXT NOT NULL, row_key TEXT NOT NULL);",
        )?;
        if let Some(choices) = choices {
            for (key, choice) in choices {
                if choice == "backup" {
                    transaction.execute("INSERT INTO restore_choices VALUES (?1)", [key])?;
                }
            }
        } else {
            transaction.execute("INSERT INTO restore_choices VALUES ('*')", [])?;
        }

        for table in restore_tables {
            if table == "skills" {
                restore_skills(&transaction)?;
            } else {
                restore_table(&transaction, &table)?;
            }
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
    crate::core::skill_store::initialize(paths)?;
    crate::core::usage_store::initialize(paths)?;
    crate::core::rule_store::initialize(paths)?;
    crate::core::provider_store::initialize(paths)?;
    std::fs::create_dir_all(&paths.temp_dir)?;
    let snapshot_path = Path::new(&paths.temp_dir).join(format!(
        "ai-manager-restore-{}-{}.db",
        std::process::id(),
        now_millis()
    ));
    std::fs::write(&snapshot_path, content)?;
    let result = (|| {
        validate_restore_snapshot(&snapshot_path)?;
        let mut source = Connection::open(&snapshot_path)?;
        crate::core::skill_store::migrate_connection(&mut source)?;
        crate::core::usage_store::migrate_connection(&mut source)?;
        crate::core::rule_store::migrate_connection(&mut source)?;
        crate::core::provider_store::migrate_connection(&mut source)?;
        drop(source);
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
            source_tables.contains(table) && BACKUP_INCLUDED_TABLES.contains(&table.as_str())
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
    restore::merge_table(transaction, table)
}

// 安装、启用和回收站信息属于本机，恢复业务数据时保留当前值。
const SKILL_LOCAL_COLUMNS: &[&str] = &[
    "disabled",
    "installed_targets",
    "status",
    "source_path",
    "entry_path",
    "repo_name",
    "is_deleted",
    "deleted_at",
    "trash_id",
    "trash_path",
];

fn restore_skills(transaction: &rusqlite::Transaction<'_>) -> Result<(), ManagerError> {
    use rusqlite::{params_from_iter, types::Value as SqlValue};
    let current_state = {
        let mut statement = transaction.prepare(&format!(
            "SELECT item_key, {} FROM skills",
            SKILL_LOCAL_COLUMNS.join(",")
        ))?;
        let rows = statement
            .query_map([], |row| {
                let values = (1..=SKILL_LOCAL_COLUMNS.len())
                    .map(|index| row.get::<_, SqlValue>(index))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((row.get::<_, String>(0)?, values))
            })?
            .collect::<Result<HashMap<_, _>, _>>()?;
        rows
    };
    restore_table(transaction, "skills")?;
    let restored = transaction.prepare("SELECT item_key FROM skills WHERE item_key IN (SELECT row_key FROM restore_applied WHERE table_name='skills')")?
        .query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    let updates = SKILL_LOCAL_COLUMNS
        .iter()
        .map(|column| format!("{column}=?"))
        .collect::<Vec<_>>()
        .join(",");
    for key in restored {
        let mut values = current_state.get(&key).cloned().unwrap_or_else(|| {
            vec![
                SqlValue::Integer(1),
                SqlValue::Text(String::new()),
                SqlValue::Text("disabled".into()),
                SqlValue::Text(String::new()),
                SqlValue::Text(String::new()),
                SqlValue::Text(String::new()),
                SqlValue::Integer(0),
                SqlValue::Integer(0),
                SqlValue::Text(String::new()),
                SqlValue::Text(String::new()),
            ]
        });
        values.push(SqlValue::Text(key));
        transaction.execute(
            &format!("UPDATE skills SET {updates} WHERE item_key=?"),
            params_from_iter(values),
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

// 只跨设备同步明确的业务配置，解析器版本、缓存修订号等始终属于本机。
fn restore_row_filter(table: &str) -> &'static str {
    if table == "app_metadata" {
        "WHERE key='usage:exchange_rate'"
    } else {
        ""
    }
}

fn count_table_rows(
    connection: &Connection,
    schema: &str,
    table: &str,
) -> Result<i64, ManagerError> {
    let sql = format!(
        "SELECT COUNT(*) FROM {schema}.{} {}",
        quote_identifier(table),
        restore_row_filter(table)
    );
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
    let filter = restore_row_filter(table);
    let table = quote_identifier(table);
    let sql = format!(
        "SELECT COUNT(*) FROM (
           SELECT {columns} FROM {left_schema}.{table} {filter}
           EXCEPT
           SELECT {columns} FROM {right_schema}.{table} {filter}
         )"
    );

    Ok(connection.query_row(&sql, [], |row| row.get::<_, i64>(0))?)
}

fn comparison_column(table: &str, column: &str) -> String {
    let column = quote_identifier(column);

    if (table == "skill_repositories"
        && REPOSITORY_LOCAL_COLUMNS.contains(&column.trim_matches('"')))
        || (table == "skills" && SKILL_LOCAL_COLUMNS.contains(&column.trim_matches('"')))
        || (table == "rules" && RULE_LOCAL_COLUMNS.contains(&column.trim_matches('"')))
        || (table == "providers" && PROVIDER_LOCAL_COLUMNS.contains(&column.trim_matches('"')))
        || (table == "providers"
            && [
                "created_at",
                "updated_at",
                "last_refresh",
                "token_updated_at",
            ]
            .contains(&column.trim_matches('"')))
        || ((table.starts_with("skill") || table == "rules")
            && ["created_at", "updated_at"].contains(&column.trim_matches('"')))
    {
        return "NULL".to_string();
    }
    column
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
        backup, open, preview_restore, read_table_names, restore_selected, BACKUP_INCLUDED_TABLES,
    };
    use crate::core::paths::resolve_app_paths;
    use crate::core::{provider_store, rule_store, skill_store, usage_store};
    use rusqlite::{params, Connection};
    use serde_json::{json, Map};
    use std::path::Path;

    #[test]
    fn backup_keeps_only_allowed_business_data() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-main-database-backup-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        provider_store::write_provider_bundle(
            &paths,
            &[json!({"id": "provider-a", "enabled": true})],
            &[json!({"id": "model-a", "providerId": "provider-a"})],
            &[json!({"id": "profile-a", "cli": "codex", "providerId": "provider-a"})],
            &Map::from_iter([("provider-a".to_string(), json!("encrypted-key"))]),
        )
        .unwrap();
        provider_store::write_desktop_bundle(
            &paths,
            &[json!({"id": "desktop-a", "name": "Desktop"})],
            &Map::from_iter([("currentProviderId".to_string(), json!("desktop-a"))]),
            &provider_store::read_keys(&paths).unwrap(),
        )
        .unwrap();
        provider_store::write_codex_accounts(
            &paths,
            &[json!({"id": "account-a", "usage": {"source": "current"}, "disabled": true})],
        )
        .unwrap();
        skill_store::write_skills(
            &paths,
            &[json!({
              "id": "skill-id-a",
              "name": "skill-a",
              "disabled": false,
              "installedTargets": ["codex"],
              "installStates": {"codex": {"state": "installed"}},
              "status": "installed",
              "sourcePath": "C:\\old-device\\skills\\skill-a",
              "entryPath": "C:\\old-device\\skills\\skill-a\\SKILL.md",
              "repoName": "Managed"
            })],
        )
        .unwrap();
        skill_store::write_groups(&paths, &[json!({"id": "group-a"})]).unwrap();
        skill_store::write_repositories(&paths, &[json!({"id": "repository-a"})]).unwrap();
        skill_store::write_installs(
            &paths,
            &Map::from_iter([("skill-a".to_string(), json!(["codex"]))]),
        )
        .unwrap();
        rule_store::upsert_prompt(
            &paths,
            &json!({
              "id": "rule-a",
              "cli": "claude",
              "fileName": "rule-a.md",
              "updatedAt": 1
            }),
        )
        .unwrap();
        usage_store::write_pricing(
            &paths,
            &json!({
              "exchangeRate": 7.2,
              "items": [{"id": "price-a", "modelId": "gpt-test"}]
            }),
        )
        .unwrap();
        let connection = open(&paths).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE future_runtime_data(id INTEGER PRIMARY KEY);
                 INSERT INTO future_runtime_data(id) VALUES (1);
                 CREATE TABLE image_outputs(
                   task_id TEXT NOT NULL,
                   position INTEGER NOT NULL,
                   format TEXT NOT NULL,
                   bytes BLOB NOT NULL,
                   file_path TEXT NOT NULL DEFAULT '',
                   PRIMARY KEY(task_id, position)
                 );
                 INSERT INTO image_outputs(task_id, position, format, bytes)
                 VALUES ('large-image', 0, 'png', zeroblob(1048576));",
            )
            .unwrap();
        drop(connection);

        rule_store::set_active_prompt(&paths, "claude", "rule-a", "local-hash", 100).unwrap();
        let snapshot_path = root.join("snapshot.db");
        std::fs::write(&snapshot_path, backup(&paths).unwrap()).unwrap();
        let snapshot = Connection::open(snapshot_path).unwrap();

        assert_eq!(snapshot.query_row(
            "SELECT enabled_clis,claude_synced_hash,claude_synced_at FROM rules WHERE id='rule-a'",
            [], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))
        ).unwrap(), ("".into(), "".into(), 0));
        for table in read_table_names(&snapshot, "main").unwrap() {
            let count = snapshot
                .query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap();

            if BACKUP_INCLUDED_TABLES.contains(&table.as_str()) {
                assert!(count > 0, "{table} 应包含备份数据");
            } else {
                assert_eq!(count, 0, "{table} 不应包含备份数据");
            }
        }
        let provider = snapshot
            .query_row(
                "SELECT * FROM providers WHERE id='provider-a'",
                [],
                provider_store::item_from_row,
            )
            .unwrap();
        let skill = snapshot
            .query_row("SELECT * FROM skills WHERE item_key='skill-a'", [], |row| {
                skill_store::item_from_row("skills", row)
            })
            .unwrap();
        let account = snapshot
            .query_row(
                "SELECT * FROM providers WHERE id='account:account-a'",
                [],
                provider_store::item_from_row,
            )
            .unwrap();

        assert_eq!(provider["enabled"], false);
        assert_eq!(skill["disabled"], true);
        assert_eq!(skill["installedTargets"], json!([]));
        assert!(skill.get("installStates").is_none());
        for field in [
            "sourcePath",
            "entryPath",
            "repoName",
            "trashId",
            "trashPath",
        ] {
            assert_eq!(skill[field], "", "Skill 本机字段 {field} 不应进入备份");
        }
        assert!(account.get("usage").is_none());
        assert_eq!(account["disabled"], true);
    }

    #[test]
    fn restores_legacy_provider_backup_without_old_tables_or_backup_activation() {
        let root =
            std::env::temp_dir().join(format!("legacy-provider-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let legacy_path = root.join("legacy.db");
        let source = Connection::open(&legacy_path).unwrap();
        source.execute_batch("CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
          CREATE TABLE providers(item_key TEXT PRIMARY KEY,sort_order INTEGER,payload_json TEXT);
          CREATE TABLE claude_desktop_providers(item_key TEXT PRIMARY KEY,sort_order INTEGER,payload_json TEXT);
          CREATE TABLE codex_accounts(item_key TEXT PRIMARY KEY,sort_order INTEGER,payload_json TEXT);
          CREATE TABLE provider_models(item_key TEXT PRIMARY KEY,sort_order INTEGER,payload_json TEXT);
          CREATE TABLE provider_keys(provider_id TEXT PRIMARY KEY,payload_json TEXT);").unwrap();
        for (table, id, value) in [
            (
                "providers",
                "p",
                json!({"id":"p","cli":"codex","name":"备份配置","enabled":true}),
            ),
            (
                "claude_desktop_providers",
                "p",
                json!({"id":"p","mode":"direct","name":"桌面配置"}),
            ),
            (
                "codex_accounts",
                "official",
                json!({"id":"official","email":"backup@example.test","auth":{"accessToken":"backup-access"}}),
            ),
            ("provider_models", "m", json!({"id":"m","providerId":"p"})),
        ] {
            source
                .execute(
                    &format!("INSERT INTO {table} VALUES(?1,0,?2)"),
                    params![id, value.to_string()],
                )
                .unwrap();
        }
        source
            .execute(
                "INSERT INTO provider_keys VALUES('p','\"backup-secret\"')",
                [],
            )
            .unwrap();
        drop(source);
        let content = std::fs::read(&legacy_path).unwrap();
        let paths = resolve_app_paths(&root.join("target"));
        provider_store::write_provider_bundle(
            &paths,
            &[json!({"id":"p","cli":"codex","name":"本机配置"})],
            &[],
            &[json!({"id":"codex","cli":"codex","providerId":"p"})],
            &Map::from_iter([("p".into(), json!("local-secret"))]),
        )
        .unwrap();
        restore_selected(&paths, &content, &["providers".into()]).unwrap();
        assert_eq!(
            provider_store::read_providers(&paths).unwrap()[0]["name"],
            "备份配置"
        );
        assert_eq!(provider_store::read_models(&paths).unwrap()[0]["id"], "m");
        assert_eq!(
            provider_store::read_keys(&paths).unwrap()["p"]["keys"][0]["value"],
            "local-secret"
        );
        assert_eq!(
            provider_store::read_profiles(&paths).unwrap()[0]["providerId"],
            "p"
        );
        assert_eq!(
            provider_store::read_codex_accounts(&paths).unwrap()[0]["auth"]["accessToken"],
            "backup-access"
        );
        assert_eq!(
            provider_store::read_active_codex_account_id(&paths).unwrap(),
            ""
        );
        assert_eq!(
            provider_store::read_desktop_settings(&paths).unwrap()["currentProviderId"],
            ""
        );
        let connection = open(&paths).unwrap();
        for table in [
            "provider_models",
            "codex_accounts",
            "claude_desktop_providers",
            "provider_schema_migrations",
        ] {
            assert!(!read_table_names(&connection, "main")
                .unwrap()
                .contains(&table.to_string()));
        }
        assert!(connection
            .prepare("SELECT * FROM pragma_foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restore_preserves_local_state_and_disables_new_items() {
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
        provider_store::write_provider_bundle(
            &source_paths,
            &[
                json!({"id": "provider-a", "name": "backup", "enabled": false}),
                json!({"id": "provider-b", "name": "new", "enabled": true}),
            ],
            &[],
            &[],
            &Map::new(),
        )
        .unwrap();
        provider_store::write_codex_accounts(
            &source_paths,
            &[
                json!({"id": "account-a", "email": "backup@example.com", "usage": {"source": "backup"}, "disabled": false}),
                json!({"id": "account-b", "email": "new@example.com", "usage": {"source": "backup"}, "disabled": false}),
            ],
        )
        .unwrap();
        skill_store::write_skills(
            &source_paths,
            &[
                json!({"name": "skill-a", "description": "backup", "disabled": true}),
                json!({"name": "skill-b", "description": "new", "disabled": false}),
            ],
        )
        .unwrap();
        rule_store::upsert_prompt(
            &source_paths,
            &json!({"id": "rule-a", "cli": "claude", "fileName": "rule-a.md", "name": "backup"}),
        )
        .unwrap();
        rule_store::upsert_prompt(
            &source_paths,
            &json!({"id": "rule-b", "cli": "claude", "fileName": "rule-b.md", "name": "new"}),
        )
        .unwrap();
        let backup_content = backup(&source_paths).unwrap();

        let target_paths = resolve_app_paths(Path::new(&target_root));
        provider_store::write_provider_bundle(
            &target_paths,
            &[
                json!({"id": "provider-a", "name": "current"}),
                json!({"id": "provider-local", "name": "local", "enabled": true}),
            ],
            &[],
            &[json!({"id": "profile-local", "cli": "codex", "providerId": "provider-local"})],
            &Map::from_iter([
                ("provider-a".to_string(), json!("current-key")),
                ("provider-local".to_string(), json!("local-key")),
            ]),
        )
        .unwrap();
        provider_store::write_runtime_state(
            &target_paths,
            &Map::from_iter([(
                "codex".to_string(),
                json!({"activeProviderId": "provider-local", "status": "SYNCED"}),
            )]),
        )
        .unwrap();
        provider_store::write_codex_accounts(
            &target_paths,
            &[
                json!({"id": "account-a", "email": "current@example.com", "usage": {"source": "current"}, "disabled": true}),
                json!({"id": "account-local", "email": "local@example.com"}),
            ],
        )
        .unwrap();
        provider_store::write_active_codex_account_id(&target_paths, "account-local").unwrap();
        skill_store::write_skills(
            &target_paths,
            &[
                json!({
                  "name": "skill-a",
                  "description": "current",
                  "disabled": false,
                  "installedTargets": ["codex"],
                  "installStates": {"codex": {"state": "installed"}},
                  "status": "installed"
                }),
                json!({"name": "skill-local", "disabled": false}),
            ],
        )
        .unwrap();
        skill_store::write_installs(
            &target_paths,
            &Map::from_iter([
                ("skill-a".to_string(), json!(["codex"])),
                ("skill-local".to_string(), json!(["codex"])),
            ]),
        )
        .unwrap();
        rule_store::upsert_prompt(
            &target_paths,
            &json!({"id": "rule-a", "cli": "claude", "fileName": "rule-a.md", "name": "current"}),
        )
        .unwrap();
        rule_store::upsert_prompt(
            &target_paths,
            &json!({"id": "rule-local", "cli": "codex", "fileName": "rule-local.md"}),
        )
        .unwrap();
        rule_store::set_active_prompt(&target_paths, "claude", "rule-a", "claude-hash", 123)
            .unwrap();
        rule_store::set_active_prompt(&target_paths, "codex", "rule-local", "codex-hash", 456)
            .unwrap();
        let target = open(&target_paths).unwrap();
        target
            .execute_batch(
                "CREATE TABLE retained_data(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO retained_data(id, value) VALUES (1, 'current');",
            )
            .unwrap();
        drop(target);

        let differences = preview_restore(&target_paths, &backup_content).unwrap();
        assert!(!differences.iter().any(|item| item.table == "retained_data"));
        assert!(!differences
            .iter()
            .any(|item| item.table.starts_with("usage_")));
        assert!(differences.iter().any(|item| item.table == "providers"));
        assert!(differences.iter().any(|item| item.table == "skills"));
        assert!(differences
            .iter()
            .any(|difference| difference.table == "rules"));
        assert!(!differences
            .iter()
            .any(|difference| difference.table == "rule_profiles"));

        restore_selected(
            &target_paths,
            &backup_content,
            &[
                "retained_data".to_string(),
                "providers".to_string(),
                "skills".to_string(),
                "rules".to_string(),
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
            "current"
        );
        let provider_a = restored
            .query_row(
                "SELECT * FROM providers WHERE id='provider-a'",
                [],
                provider_store::item_from_row,
            )
            .unwrap();
        assert_eq!(provider_a["name"], "backup");
        assert_eq!(provider_a["enabled"], true);
        let provider_b = restored
            .query_row(
                "SELECT * FROM providers WHERE id='provider-b'",
                [],
                provider_store::item_from_row,
            )
            .unwrap();
        assert_eq!(provider_b["enabled"], false);
        assert!(provider_store::read_profiles(&target_paths)
            .unwrap()
            .is_empty());
        assert_eq!(
            provider_store::read_runtime_state(&target_paths).unwrap()["codex"]["activeProviderId"],
            ""
        );
        assert_eq!(
            provider_store::read_runtime_state(&target_paths).unwrap()["codex"]["status"],
            "NO_ACTIVE"
        );
        assert!(provider_store::read_keys(&target_paths)
            .unwrap()
            .contains_key("provider-a"));
        assert!(provider_store::read_keys(&target_paths)
            .unwrap()
            .contains_key("provider-local"));
        let account_a = restored
            .query_row(
                "SELECT * FROM providers WHERE id='account:account-a'",
                [],
                provider_store::item_from_row,
            )
            .unwrap();
        assert_eq!(account_a["email"], "backup@example.com");
        assert_eq!(account_a["usage"]["source"], "current");
        assert_eq!(account_a["disabled"], true);
        let account_b = restored
            .query_row(
                "SELECT * FROM providers WHERE id='account:account-b'",
                [],
                provider_store::item_from_row,
            )
            .unwrap();
        assert!(account_b.get("usage").is_none());
        assert_eq!(account_b["disabled"], true);
        assert_eq!(
            restored
                .query_row(
                    "SELECT substr(id,9) FROM providers WHERE type='codex-account' AND is_active=1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "account-local"
        );
        let skill_a = restored
            .query_row("SELECT * FROM skills WHERE item_key='skill-a'", [], |row| {
                skill_store::item_from_row("skills", row)
            })
            .unwrap();
        let skill_b = restored
            .query_row("SELECT * FROM skills WHERE item_key='skill-b'", [], |row| {
                skill_store::item_from_row("skills", row)
            })
            .unwrap();
        assert_eq!(skill_a["description"], "backup");
        assert_eq!(skill_a["disabled"], false);
        assert_eq!(skill_a["installedTargets"], json!(["codex"]));
        assert_eq!(skill_a["status"], "installed");
        assert_eq!(skill_b["disabled"], true);
        assert_eq!(skill_b["installedTargets"], json!([]));
        assert!(skill_b.get("installStates").is_none());
        assert_eq!(skill_b["status"], "disabled");
        assert_eq!(
            restored
                .query_row(
                    "SELECT COUNT(*) FROM skills WHERE item_key = 'skill-local'",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            1
        );
        let skill_local = restored
            .query_row(
                "SELECT * FROM skills WHERE item_key='skill-local'",
                [],
                |row| skill_store::item_from_row("skills", row),
            )
            .unwrap();
        assert_eq!(skill_local["disabled"], false);
        assert_eq!(skill_local["status"], "");
        let installs = skill_store::read_installs(&target_paths).unwrap();
        assert_eq!(installs["skill-a"], json!(["codex"]));
        assert_eq!(installs["skill-local"], json!(["codex"]));
        let profiles = rule_store::read_profiles(&target_paths).unwrap();
        assert_eq!(profiles["claude"]["activePromptId"], "rule-a");
        assert_eq!(profiles["codex"]["activePromptId"], "rule-local");
        let runtime = rule_store::read_runtime_state(&target_paths).unwrap();
        assert_eq!(runtime["claude"]["runtimeHash"], "claude-hash");
        assert_eq!(runtime["claude"]["lastSyncAt"], 123);
        assert_eq!(runtime["codex"]["runtimeHash"], "codex-hash");
        assert_eq!(
            restored
                .query_row(
                    "SELECT enabled_clis FROM rules WHERE id='rule-b'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            ""
        );
        assert_eq!(
            restored
                .query_row(
                    "SELECT COUNT(*) FROM rules WHERE id = 'rule-b'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn restore_reconciles_provider_profiles_state_and_models_after_all_tables() {
        let source_root = std::env::temp_dir().join(format!(
            "monkey-thief-provider-relations-source-{}",
            std::process::id()
        ));
        let target_root = std::env::temp_dir().join(format!(
            "monkey-thief-provider-relations-target-{}",
            std::process::id()
        ));
        for root in [&source_root, &target_root] {
            if root.exists() {
                std::fs::remove_dir_all(root).unwrap();
            }
        }
        let source_paths = resolve_app_paths(Path::new(&source_root));
        let target_paths = resolve_app_paths(Path::new(&target_root));

        provider_store::write_provider_bundle(
            &source_paths,
            &[
                json!({"id": "provider-a", "cli": "claude", "enabled": false}),
                json!({"id": "provider-valid", "cli": "codex", "enabled": true}),
                json!({"id": "provider-new", "cli": "gemini", "enabled": true}),
            ],
            &[
                json!({"id": "model-a", "providerId": "provider-a"}),
                json!({"id": "model-valid", "providerId": "provider-valid"}),
                json!({"id": "model-new", "providerId": "provider-new"}),
            ],
            &[],
            &Map::new(),
        )
        .unwrap();
        provider_store::write_provider_bundle(
            &target_paths,
            &[
                json!({"id": "provider-a", "cli": "codex", "enabled": true}),
                json!({"id": "provider-valid", "cli": "codex", "enabled": true}),
                json!({"id": "provider-removed", "cli": "gemini", "enabled": true}),
            ],
            &[],
            &[
                json!({"id": "profile-a", "cli": "codex", "providerId": "provider-a"}),
                json!({"id": "profile-removed", "cli": "gemini", "providerId": "provider-removed"}),
            ],
            &Map::new(),
        )
        .unwrap();
        provider_store::write_runtime_state(
            &target_paths,
            &Map::from_iter([
                (
                    "codex".to_string(),
                    json!({"activeProviderId": "provider-valid", "status": "SYNCED"}),
                ),
                (
                    "gemini".to_string(),
                    json!({"activeProviderId": "provider-removed", "status": "SYNCED"}),
                ),
            ]),
        )
        .unwrap();

        restore_selected(
            &target_paths,
            &backup(&source_paths).unwrap(),
            &["providers".to_string()],
        )
        .unwrap();

        let providers = provider_store::read_providers(&target_paths).unwrap();
        let provider_a = providers
            .iter()
            .find(|item| item["id"] == "provider-a")
            .unwrap();
        let provider_new = providers
            .iter()
            .find(|item| item["id"] == "provider-new")
            .unwrap();
        let profile_ids = provider_store::read_profiles(&target_paths)
            .unwrap()
            .into_iter()
            .map(|item| item["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let runtime_state = provider_store::read_runtime_state(&target_paths).unwrap();
        let model_ids = provider_store::read_models(&target_paths)
            .unwrap()
            .into_iter()
            .map(|item| item["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();

        assert_eq!(provider_a["enabled"], true);
        assert_eq!(provider_new["enabled"], false);
        assert_eq!(profile_ids, vec!["profile-removed"]);
        assert_eq!(runtime_state["codex"]["activeProviderId"], "");
        assert_eq!(
            runtime_state["gemini"]["activeProviderId"],
            "provider-removed"
        );
        assert_eq!(runtime_state["gemini"]["status"], "SYNCED");
        assert_eq!(model_ids, vec!["model-a", "model-valid", "model-new"]);
    }

    #[test]
    fn restoring_providers_restores_models_and_preserves_local_only_providers() {
        let source_root = std::env::temp_dir().join(format!(
            "monkey-thief-provider-model-cleanup-source-{}",
            std::process::id()
        ));
        let target_root = std::env::temp_dir().join(format!(
            "monkey-thief-provider-model-cleanup-target-{}",
            std::process::id()
        ));
        for root in [&source_root, &target_root] {
            if root.exists() {
                std::fs::remove_dir_all(root).unwrap();
            }
        }
        let source_paths = resolve_app_paths(Path::new(&source_root));
        let target_paths = resolve_app_paths(Path::new(&target_root));

        provider_store::write_provider_bundle(
            &source_paths,
            &[json!({"id": "provider-a", "cli": "codex"})],
            &[],
            &[],
            &Map::new(),
        )
        .unwrap();
        provider_store::write_provider_bundle(
            &target_paths,
            &[
                json!({"id": "provider-a", "cli": "codex"}),
                json!({"id": "provider-removed", "cli": "codex"}),
            ],
            &[
                json!({"id": "model-a", "providerId": "provider-a"}),
                json!({"id": "model-removed", "providerId": "provider-removed"}),
            ],
            &[],
            &Map::new(),
        )
        .unwrap();

        restore_selected(
            &target_paths,
            &backup(&source_paths).unwrap(),
            &["providers".to_string()],
        )
        .unwrap();

        let models = provider_store::read_models(&target_paths).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], "model-removed");
    }

    #[test]
    fn preview_ignores_provider_skill_and_codex_account_local_state() {
        let source_root = std::env::temp_dir().join(format!(
            "monkey-thief-main-database-preview-source-{}",
            std::process::id()
        ));
        let target_root = std::env::temp_dir().join(format!(
            "monkey-thief-main-database-preview-target-{}",
            std::process::id()
        ));
        for root in [&source_root, &target_root] {
            if root.exists() {
                std::fs::remove_dir_all(root).unwrap();
            }
        }
        let source_paths = resolve_app_paths(Path::new(&source_root));
        let target_paths = resolve_app_paths(Path::new(&target_root));

        provider_store::write_provider_bundle(
            &source_paths,
            &[json!({"id": "provider-a", "name": "same", "enabled": false})],
            &[],
            &[],
            &Map::new(),
        )
        .unwrap();
        provider_store::write_provider_bundle(
            &target_paths,
            &[json!({"id": "provider-a", "name": "same", "enabled": true})],
            &[],
            &[],
            &Map::new(),
        )
        .unwrap();
        skill_store::write_skills(
            &source_paths,
            &[json!({"name": "skill-a", "disabled": true, "status": "disabled"})],
        )
        .unwrap();
        skill_store::write_skills(
            &target_paths,
            &[json!({
              "name": "skill-a",
              "disabled": false,
              "installedTargets": ["codex"],
              "status": "installed"
            })],
        )
        .unwrap();
        provider_store::write_codex_accounts(
            &source_paths,
            &[json!({
              "id": "account-a",
              "email": "same@example.com",
              "usage": {"source": "backup"},
              "disabled": false
            })],
        )
        .unwrap();
        provider_store::write_codex_accounts(
            &target_paths,
            &[json!({
              "id": "account-a",
              "email": "same@example.com",
              "usage": {"source": "current"},
              "disabled": true
            })],
        )
        .unwrap();

        for paths in [&source_paths, &target_paths] {
            rule_store::upsert_prompt(
                paths,
                &json!({"id":"rule-a","cli":"common","fileName":"rule-a.md","name":"same"}),
            )
            .unwrap();
        }
        rule_store::set_active_prompt(&source_paths, "claude", "rule-a", "source-hash", 100)
            .unwrap();
        rule_store::set_active_prompt(&target_paths, "codex", "rule-a", "target-hash", 200)
            .unwrap();
        let differences = preview_restore(&target_paths, &backup(&source_paths).unwrap()).unwrap();

        assert!(!differences.iter().any(|item| item.table == "rules"));
        assert!(
            super::preview_rows(&target_paths, &backup(&source_paths).unwrap())
                .unwrap()
                .iter()
                .all(|item| item.table != "rules")
        );
        assert!(!differences.iter().any(|item| item.table == "providers"));
        assert!(!differences.iter().any(|item| item.table == "skills"));
        assert!(!differences
            .iter()
            .any(|item| item.table == "codex_accounts"));
    }
    #[test]
    fn restores_legacy_skill_backup_into_structured_fields() {
        let root =
            std::env::temp_dir().join(format!("legacy-skill-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let legacy_path = root.join("legacy.db");
        let source = Connection::open(&legacy_path).unwrap();
        source.execute_batch("CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE skills(item_key TEXT PRIMARY KEY,sort_order INTEGER,payload_json TEXT);
            CREATE TABLE skill_groups(item_key TEXT PRIMARY KEY,sort_order INTEGER,payload_json TEXT);").unwrap();
        source.execute("INSERT INTO skills VALUES ('a',0,?1)",[json!({"id":"stable-id","name":"a","description":"描述","content":"旧正文","installedTargets":["codex"]}).to_string()]).unwrap();
        source
            .execute(
                "INSERT INTO skill_groups VALUES ('g',0,?1)",
                [json!({"id":"g","name":"分组","skillIds":["stable-id"]}).to_string()],
            )
            .unwrap();
        drop(source);
        let paths = resolve_app_paths(&root.join("target"));
        let snapshot = std::fs::read(legacy_path).unwrap();
        restore_selected(&paths, &snapshot, &["skills".into(), "skill_groups".into()]).unwrap();
        let skills = skill_store::read_skills(&paths).unwrap();
        assert_eq!(skills[0]["description"], "描述");
        assert_eq!(skills[0]["groupId"], "g");
        assert_eq!(skills[0]["disabled"], true);
        assert!(skills[0].get("content").is_none());
        assert_eq!(
            skill_store::read_groups(&paths).unwrap()[0]["skillIds"],
            json!(["stable-id"])
        );
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn restores_legacy_exchange_rate_without_restoring_local_usage_metadata() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-legacy-usage-pricing-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        let legacy_path = root.join("legacy.db");
        let source = Connection::open(&legacy_path).unwrap();
        source.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at INTEGER NOT NULL);
            INSERT INTO schema_migrations VALUES (1,100),(2,200);
            CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            INSERT INTO app_metadata VALUES ('local_guard','from-other-device');
            CREATE TABLE usage_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            INSERT INTO usage_metadata VALUES ('revision','777'),('session_parser_version:codex','99');
            CREATE TABLE usage_pricing_config(id INTEGER PRIMARY KEY,exchange_rate REAL NOT NULL);
            INSERT INTO usage_pricing_config VALUES (1,7.4);").unwrap();
        drop(source);
        let paths = resolve_app_paths(&root.join("target"));
        usage_store::initialize(&paths).unwrap();
        let local = open(&paths).unwrap();
        local
            .execute_batch(
                "UPDATE app_metadata SET value='42' WHERE key='usage:revision';
            INSERT INTO app_metadata VALUES ('local_guard','this-device');",
            )
            .unwrap();
        let snapshot = std::fs::read(&legacy_path).unwrap();
        let differences = preview_restore(&paths, &snapshot).unwrap();
        assert_eq!(
            differences
                .iter()
                .map(|item| item.table.as_str())
                .collect::<Vec<_>>(),
            vec!["app_metadata"]
        );
        assert_eq!(differences[0].current_rows, 0);
        assert_eq!(differences[0].backup_rows, 1);
        let rows = super::preview_rows(&paths, &snapshot).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row_key, "usage:exchange_rate");
        restore_selected(&paths, &snapshot, &["app_metadata".into()]).unwrap();
        assert_eq!(
            usage_store::read_pricing(&paths).unwrap()["exchangeRate"],
            7.4
        );
        assert_eq!(usage_store::revision(&paths).unwrap(), 42);
        assert_eq!(
            local
                .query_row(
                    "SELECT value FROM app_metadata WHERE key='local_guard'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "this-device"
        );
        assert!(preview_restore(&paths, &snapshot).unwrap().is_empty());
        let new_path = root.join("new-backup.db");
        std::fs::write(&new_path, backup(&paths).unwrap()).unwrap();
        let backup = Connection::open(new_path).unwrap();
        assert_eq!(
            backup
                .query_row("SELECT COUNT(*) FROM app_metadata", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            backup
                .query_row(
                    "SELECT value FROM app_metadata WHERE key='usage:exchange_rate'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "7.4"
        );
        assert_eq!(backup.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('schema_migrations','usage_metadata','usage_pricing_config','usage_logs','usage_request_records')",[],|row| row.get::<_,i64>(0)).unwrap(),0);
    }
    #[test]
    fn restores_legacy_rule_backup_without_reintroducing_old_tables_or_local_state() {
        let root =
            std::env::temp_dir().join(format!("legacy-rule-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let snapshot_path = root.join("legacy.db");
        let snapshot = Connection::open(&snapshot_path).unwrap();
        snapshot.execute_batch(r#"
            CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE rule_prompts(prompt_id TEXT PRIMARY KEY,cli TEXT NOT NULL,
              file_name TEXT NOT NULL,updated_at INTEGER NOT NULL,payload_json TEXT NOT NULL);
            CREATE TABLE rule_profiles(cli TEXT PRIMARY KEY,payload_json TEXT NOT NULL);
            CREATE TABLE rule_runtime_state(cli TEXT PRIMARY KEY,payload_json TEXT NOT NULL);
            CREATE TABLE rule_schema_migrations(version INTEGER PRIMARY KEY,applied_at INTEGER NOT NULL);
            INSERT INTO rule_prompts VALUES('legacy','common','legacy.md',42,
              '{"id":"legacy","cli":"common","name":"Legacy","fileName":"legacy.md","createdAt":12}');
            INSERT INTO rule_profiles VALUES('claude','{"activePromptId":"legacy"}');
            INSERT INTO rule_runtime_state VALUES('claude',
              '{"activePromptId":"legacy","runtimeHash":"old-device","lastSyncAt":100}');
        "#).unwrap();
        drop(snapshot);
        let content = std::fs::read(&snapshot_path).unwrap();
        let paths = resolve_app_paths(&root.join("target"));
        rule_store::upsert_prompt(
            &paths,
            &json!({"id":"local","cli":"claude","fileName":"local.md"}),
        )
        .unwrap();
        rule_store::set_active_prompt(&paths, "claude", "local", "local-hash", 999).unwrap();
        let differences = super::preview_rows(&paths, &content).unwrap();
        assert!(differences
            .iter()
            .any(|row| row.table == "rules" && row.row_key == "legacy"));
        restore_selected(&paths, &content, &["rules".into()]).unwrap();
        let connection = open(&paths).unwrap();
        let tables = read_table_names(&connection, "main").unwrap();
        assert!(!tables.iter().any(|table| table.starts_with("rule_")));
        assert_eq!(
            connection
                .query_row(
                    "SELECT type,name,enabled_clis FROM rules WHERE id='legacy'",
                    [],
                    |row| Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?
                    ))
                )
                .unwrap(),
            ("common".into(), "Legacy".into(), "".into())
        );
        assert_eq!(
            rule_store::read_runtime_state(&paths).unwrap()["claude"]["runtimeHash"],
            "local-hash"
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restoring_rules_keeps_local_activation_and_clears_incompatible_targets() {
        let root =
            std::env::temp_dir().join(format!("rule-restore-scope-{}", uuid::Uuid::new_v4()));
        let source_paths = resolve_app_paths(&root.join("source"));
        let paths = resolve_app_paths(&root.join("target"));
        for (id, cli) in [("shared", "common"), ("new", "common")] {
            rule_store::upsert_prompt(
                &source_paths,
                &json!({"id":id,"cli":cli,"name":"Backup","fileName":format!("{id}.md")}),
            )
            .unwrap();
        }
        // 模拟直接复制的旧设备数据库，包含与本机不同的启用关系。
        rule_store::set_active_prompt(&source_paths, "claude", "new", "source-hash", 100).unwrap();
        rule_store::upsert_prompt(
            &paths,
            &json!({"id":"shared","cli":"common","name":"Local","fileName":"shared.md"}),
        )
        .unwrap();
        rule_store::set_active_prompt(&paths, "claude", "shared", "claude-local", 11).unwrap();
        rule_store::set_active_prompt(&paths, "codex", "shared", "codex-local", 22).unwrap();
        let source = open(&source_paths).unwrap();
        source
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        drop(source);
        let raw_backup = std::fs::read(&source_paths.storage_files.database).unwrap();
        restore_selected(&paths, &raw_backup, &["rules".into()]).unwrap();
        let runtime = rule_store::read_runtime_state(&paths).unwrap();
        assert_eq!(runtime["claude"]["activePromptId"], "shared");
        assert_eq!(runtime["claude"]["runtimeHash"], "claude-local");
        assert_eq!(runtime["codex"]["runtimeHash"], "codex-local");
        // 备份把公共规则改为 Claude 专用时，保留 Claude 并清除 Codex 的挂载。
        rule_store::upsert_prompt(
            &source_paths,
            &json!({"id":"shared","cli":"claude","name":"Backup scoped","fileName":"shared.md"}),
        )
        .unwrap();
        let content = backup(&source_paths).unwrap();
        let choices = Map::from_iter([(super::row_choice_key("rules", "shared"), json!("backup"))]);
        super::restore_merged(&paths, &content, &choices).unwrap();
        let runtime = rule_store::read_runtime_state(&paths).unwrap();
        assert_eq!(runtime["claude"]["activePromptId"], "shared");
        assert_eq!(runtime["claude"]["runtimeHash"], "claude-local");
        assert_eq!(runtime["codex"]["activePromptId"], "");
        std::fs::remove_dir_all(root).unwrap();
    }
}

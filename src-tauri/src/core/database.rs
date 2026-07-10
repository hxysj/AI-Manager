use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection, MAIN_DB};
use std::path::Path;
use std::time::Duration;

const BACKUP_EXCLUDED_TABLES: [&str; 6] = [
    "usage_sessions",
    "usage_request_records",
    "skill_repository_cache",
    "skill_installs",
    "skill_trash",
    "rule_runtime_state",
];

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

    transaction.commit()?;
    connection.execute_batch("VACUUM;")?;
    Ok(())
}

pub fn restore(paths: &AppPaths, content: &[u8]) -> Result<(), ManagerError> {
    initialize(paths)?;
    std::fs::create_dir_all(&paths.temp_dir)?;
    let snapshot_path = Path::new(&paths.temp_dir).join(format!(
        "ai-manager-restore-{}-{}.db",
        std::process::id(),
        now_millis()
    ));
    std::fs::write(&snapshot_path, content)?;
    let result = (|| {
        let source = Connection::open(&snapshot_path)?;
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
        drop(source);

        let mut connection = open(paths)?;
        connection.restore(
            MAIN_DB,
            &snapshot_path,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        connection.execute(
            "INSERT INTO app_metadata(key, value) VALUES ('restored_at', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![now_millis().to_string()],
        )?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    })();

    if snapshot_path.exists() {
        std::fs::remove_file(snapshot_path)?;
    }
    result
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{backup, open, BACKUP_EXCLUDED_TABLES};
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
                "CREATE TABLE retained_data(id INTEGER PRIMARY KEY);
                 INSERT INTO retained_data(id) VALUES (1);
                 CREATE TABLE usage_sessions(id INTEGER PRIMARY KEY);
                 CREATE TABLE usage_request_records(id INTEGER PRIMARY KEY);
                 CREATE TABLE skill_repository_cache(id INTEGER PRIMARY KEY);
                 CREATE TABLE skill_installs(id INTEGER PRIMARY KEY);
                 CREATE TABLE skill_trash(id INTEGER PRIMARY KEY);
                 CREATE TABLE rule_runtime_state(id INTEGER PRIMARY KEY);
                 INSERT INTO usage_sessions(id) VALUES (1);
                 INSERT INTO usage_request_records(id) VALUES (1);
                 INSERT INTO skill_repository_cache(id) VALUES (1);
                 INSERT INTO skill_installs(id) VALUES (1);
                 INSERT INTO skill_trash(id) VALUES (1);
                 INSERT INTO rule_runtime_state(id) VALUES (1);",
            )
            .unwrap();
        drop(connection);

        let snapshot_path = root.join("snapshot.db");
        std::fs::write(&snapshot_path, backup(&paths).unwrap()).unwrap();
        let snapshot = Connection::open(snapshot_path).unwrap();

        assert_eq!(
            snapshot
                .query_row("SELECT COUNT(*) FROM retained_data", [], |row| row.get::<_, i64>(0))
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
    }
}

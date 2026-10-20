use super::*;

// 当前配置与历史统计共用版本行；历史版本不保存密钥内容。
pub(super) fn create(connection: &Connection) -> Result<(), ManagerError> {
    connection.execute_batch(
        // 历史统计允许供应商已经删除，触发器只约束当前配置的供应商关联。
        "CREATE TABLE IF NOT EXISTS provider_keys (
           provider_id TEXT NOT NULL, id TEXT NOT NULL, key_hash TEXT NOT NULL DEFAULT '',
           name TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT '', value TEXT NOT NULL DEFAULT '',
           is_current INTEGER NOT NULL DEFAULT 1 CHECK(is_current IN (0,1)),
           is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0,1)),
           sort_order INTEGER NOT NULL DEFAULT 0,
           request_count INTEGER NOT NULL DEFAULT 0, success_count INTEGER NOT NULL DEFAULT 0,
           failure_count INTEGER NOT NULL DEFAULT 0, consecutive_failures INTEGER NOT NULL DEFAULT 0,
           last_used_at INTEGER NOT NULL DEFAULT 0, last_success_at INTEGER NOT NULL DEFAULT 0,
           last_failure_at INTEGER NOT NULL DEFAULT 0, last_status_code INTEGER,
           last_error_kind TEXT NOT NULL DEFAULT '', last_failure_kind TEXT NOT NULL DEFAULT '',
           last_outcome TEXT NOT NULL DEFAULT '',
           PRIMARY KEY(provider_id,id,key_hash),
           CHECK(is_current=1 OR (is_active=0 AND value=''))
         );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_keys_current
           ON provider_keys(provider_id,id) WHERE is_current=1;
         CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_keys_active
           ON provider_keys(provider_id) WHERE is_active=1;
         CREATE TRIGGER IF NOT EXISTS provider_keys_owner_insert BEFORE INSERT ON provider_keys
         WHEN NEW.is_current=1 AND NOT EXISTS(SELECT 1 FROM providers WHERE id=NEW.provider_id)
         BEGIN SELECT RAISE(ABORT,'API Key 缺少对应供应商'); END;
         CREATE TRIGGER IF NOT EXISTS provider_keys_owner_update BEFORE UPDATE OF provider_id,is_current ON provider_keys
         WHEN NEW.is_current=1 AND NOT EXISTS(SELECT 1 FROM providers WHERE id=NEW.provider_id)
         BEGIN SELECT RAISE(ABORT,'API Key 缺少对应供应商'); END;
         CREATE TRIGGER IF NOT EXISTS provider_keys_owner_delete AFTER DELETE ON providers
         BEGIN
           UPDATE provider_keys SET is_current=0,is_active=0,value='' WHERE provider_id=OLD.id;
           DELETE FROM provider_keys WHERE provider_id=OLD.id AND request_count=0;
         END;",
    )?;
    Ok(())
}

pub(super) fn credential_hash(encrypted: &str) -> String {
    crate::api::runtime_provider::decrypt_provider_key(encrypted)
        .map(|key| crate::core::provider_key_usage::fingerprint(&key))
        // 无法解密的已有记录仍保留原值，不能把密文哈希当成密钥指纹。
        .unwrap_or_default()
}

pub(crate) fn usage_provider_id(connection: &Connection, id: &str) -> Result<String, ManagerError> {
    if let Some(local_id) = id.strip_prefix("claude-desktop:") {
        if !connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1)",
            [id],
            |row| row.get::<_, bool>(0),
        )? && connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1 AND cli='claude-desktop')",
            [local_id],
            |row| row.get::<_, bool>(0),
        )? {
            return Ok(local_id.to_string());
        }
    }
    Ok(id.to_string())
}

// 调用者持有迁移事务，配置、统计复制和旧表删除一起提交。
pub(super) fn migrate(connection: &Connection) -> Result<(), ManagerError> {
    use super::migration::{has_column, table_exists};
    if table_exists(connection, "provider_keys")?
        && !has_column(connection, "provider_keys", "is_current")?
    {
        connection.execute_batch(
            "DROP TRIGGER IF EXISTS provider_keys_owner_delete;
            ALTER TABLE provider_keys RENAME TO provider_keys_before_usage;
            DROP INDEX IF EXISTS idx_provider_keys_active;",
        )?;
        create(connection)?;
        let mut statement = connection.prepare("SELECT provider_id,id,name,note,value,is_active,sort_order FROM provider_keys_before_usage")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let encrypted: String = row.get(4)?;
            connection.execute("INSERT INTO provider_keys(provider_id,id,key_hash,name,note,value,is_active,sort_order)
                VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![
                row.get::<_,String>(0)?,row.get::<_,String>(1)?,credential_hash(&encrypted),
                row.get::<_,String>(2)?,row.get::<_,String>(3)?,encrypted,row.get::<_,bool>(5)?,row.get::<_,i64>(6)?])?;
        }
        drop(rows);
        drop(statement);
        connection.execute_batch("DROP TABLE provider_keys_before_usage;")?;
    } else {
        create(connection)?;
    }
    if table_exists(connection, "provider_key_usage")? {
        let mut statement = connection.prepare("SELECT * FROM provider_key_usage")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let provider = usage_provider_id(connection, &row.get::<_, String>("provider_id")?)?;
            connection.execute("INSERT INTO provider_keys(provider_id,id,key_hash,is_current,
                request_count,success_count,failure_count,consecutive_failures,last_used_at,last_success_at,
                last_failure_at,last_status_code,last_error_kind,last_failure_kind,last_outcome)
                VALUES(?1,?2,?3,0,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
                ON CONFLICT(provider_id,id,key_hash) DO UPDATE SET
                request_count=provider_keys.request_count+excluded.request_count,
                success_count=provider_keys.success_count+excluded.success_count,
                failure_count=provider_keys.failure_count+excluded.failure_count,
                consecutive_failures=CASE WHEN excluded.last_used_at>=provider_keys.last_used_at THEN excluded.consecutive_failures ELSE provider_keys.consecutive_failures END,
                last_status_code=CASE WHEN excluded.last_used_at>=provider_keys.last_used_at THEN excluded.last_status_code ELSE provider_keys.last_status_code END,
                last_error_kind=CASE WHEN excluded.last_used_at>=provider_keys.last_used_at THEN excluded.last_error_kind ELSE provider_keys.last_error_kind END,
                last_outcome=CASE WHEN excluded.last_used_at>=provider_keys.last_used_at THEN excluded.last_outcome ELSE provider_keys.last_outcome END,
                last_failure_kind=CASE WHEN excluded.last_failure_at>=provider_keys.last_failure_at THEN excluded.last_failure_kind ELSE provider_keys.last_failure_kind END,
                last_used_at=MAX(provider_keys.last_used_at,excluded.last_used_at),
                last_success_at=MAX(provider_keys.last_success_at,excluded.last_success_at),
                last_failure_at=MAX(provider_keys.last_failure_at,excluded.last_failure_at)", params![
                provider,row.get::<_,String>("key_id")?,row.get::<_,String>("key_hash")?,
                row.get::<_,i64>("request_count")?,row.get::<_,i64>("success_count")?,row.get::<_,i64>("failure_count")?,
                row.get::<_,i64>("consecutive_failures")?,row.get::<_,i64>("last_used_at")?,row.get::<_,i64>("last_success_at")?,
                row.get::<_,i64>("last_failure_at")?,row.get::<_,Option<i64>>("last_status_code")?,
                row.get::<_,String>("last_error_kind")?,row.get::<_,String>("last_failure_kind")?,row.get::<_,String>("last_outcome")?])?;
        }
        drop(rows);
        drop(statement);
        connection.execute_batch("DROP TABLE provider_key_usage;")?;
    }
    Ok(())
}

pub(super) fn archive(
    connection: &Connection,
    provider_id: &str,
    key_id: &str,
) -> Result<(), ManagerError> {
    connection.execute("UPDATE provider_keys SET is_current=0,is_active=0,value='' WHERE provider_id=?1 AND id=?2 AND is_current=1",params![provider_id,key_id])?;
    connection.execute("DELETE FROM provider_keys WHERE provider_id=?1 AND id=?2 AND is_current=0 AND request_count=0",params![provider_id,key_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::runtime_provider::encrypt_provider_key;
    use crate::core::provider_key_usage::{self, KeyRequest};

    fn fixture() -> (std::path::PathBuf, AppPaths) {
        let root = std::env::temp_dir().join(format!("merged-key-{}", uuid::Uuid::new_v4()));
        let paths = crate::core::paths::resolve_app_paths(&root);
        write_provider_bundle(
            &paths,
            &[json!({"id":"p","cli":"codex"})],
            &[],
            &[],
            &Map::new(),
        )
        .unwrap();
        (root, paths)
    }

    fn set_credential(paths: &AppPaths, secret: &str) {
        write_keys(
            paths,
            &Map::from_iter([(
                "p".into(),
                json!({"keys":[{
            "id":"k","name":"主 Key","value":encrypt_provider_key(secret).unwrap()
        }],"activeKeyId":"k"}),
            )]),
        )
        .unwrap();
    }

    fn legacy_schema(connection: &Connection) {
        connection.execute_batch("DROP TRIGGER IF EXISTS provider_keys_owner_delete;
            DROP TABLE provider_keys;
            CREATE TABLE provider_keys(provider_id TEXT NOT NULL,id TEXT NOT NULL,name TEXT NOT NULL DEFAULT '',
                note TEXT NOT NULL DEFAULT '',value TEXT NOT NULL,is_active INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(provider_id,id));
            CREATE UNIQUE INDEX idx_provider_keys_active ON provider_keys(provider_id) WHERE is_active=1;
            CREATE TABLE provider_key_usage(provider_id TEXT NOT NULL,key_id TEXT NOT NULL,key_hash TEXT NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 0,success_count INTEGER NOT NULL DEFAULT 0,
                failure_count INTEGER NOT NULL DEFAULT 0,consecutive_failures INTEGER NOT NULL DEFAULT 0,
                last_used_at INTEGER NOT NULL DEFAULT 0,last_success_at INTEGER NOT NULL DEFAULT 0,
                last_failure_at INTEGER NOT NULL DEFAULT 0,last_status_code INTEGER,last_error_kind TEXT NOT NULL DEFAULT '',
                last_failure_kind TEXT NOT NULL DEFAULT '',last_outcome TEXT NOT NULL DEFAULT '',PRIMARY KEY(provider_id,key_id,key_hash));").unwrap();
    }

    #[test]
    fn rotations_keep_versioned_usage_and_in_flight_requests() {
        let (root, paths) = fixture();
        set_credential(&paths, "first-secret");
        let mut pending = KeyRequest::new(&paths, "p", "k", "first-secret");
        pending.start();
        set_credential(&paths, "replacement");
        pending.response(401, false);
        pending.finish();
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "first-secret").unwrap()["failureCount"],
            1
        );
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "replacement").unwrap()["requestCount"],
            0
        );
        let mut request = KeyRequest::new(&paths, "p", "k", "replacement");
        request.start();
        request.response(200, false);
        request.finish();
        set_credential(&paths, "replacement");
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "replacement").unwrap()["successCount"],
            1
        );
        set_credential(&paths, "first-secret");
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "first-secret").unwrap()["failureCount"],
            1
        );
        let connection = database::open(&paths).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM provider_keys", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM provider_keys WHERE is_current=0 AND (value<>'' OR is_active=1)",[],|row|row.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(
            read_keys(&paths).unwrap()["p"]["keys"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        write_keys(&paths, &Map::new()).unwrap();
        assert!(read_keys(&paths).unwrap().is_empty());
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM provider_keys WHERE value<>''",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        set_credential(&paths, "first-secret");
        let mut deleted = KeyRequest::new(&paths, "p", "k", "first-secret");
        deleted.start();
        write_provider_bundle(&paths, &[], &[], &[], &Map::new()).unwrap();
        deleted.response(200, false);
        deleted.finish();
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "first-secret").unwrap()["requestCount"],
            2
        );
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "first-secret").unwrap()["successCount"],
            1
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_key_and_usage_tables_merge_without_losing_versions() {
        let (root, paths) = fixture();
        let mut connection = database::open(&paths).unwrap();
        legacy_schema(&connection);
        connection.execute("INSERT INTO provider_keys(provider_id,id,name,value,is_active) VALUES('p','k','主 Key',?1,1)",
            [encrypt_provider_key("first-secret").unwrap()]).unwrap();
        for (provider, secret, count) in [
            ("p", "first-secret", 11),
            ("p", "replacement", 7),
            ("removed-provider", "removed", 3),
        ] {
            connection.execute("INSERT INTO provider_key_usage(provider_id,key_id,key_hash,request_count,success_count,last_used_at,last_outcome)
                VALUES(?1,'k',?2,?3,?3,123,'success')",params![provider,provider_key_usage::fingerprint(secret),count]).unwrap();
        }
        migrate_connection(&mut connection).unwrap();
        migrate_connection(&mut connection).unwrap();
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "first-secret").unwrap()["requestCount"],
            11
        );
        assert_eq!(
            provider_key_usage::read(&paths, "p", "k", "replacement").unwrap()["requestCount"],
            7
        );
        assert_eq!(
            provider_key_usage::read(&paths, "removed-provider", "k", "removed").unwrap()
                ["requestCount"],
            3
        );
        assert_eq!(read_keys(&paths).unwrap()["p"]["activeKeyId"], "k");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM provider_keys", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert!(!migration::table_exists(&connection, "provider_key_usage").unwrap());
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM provider_keys WHERE is_current=0 AND value<>''",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_current_owner_rolls_back_the_entire_key_merge() {
        let (root, paths) = fixture();
        let mut connection = database::open(&paths).unwrap();
        legacy_schema(&connection);
        connection
            .execute(
                "INSERT INTO provider_keys(provider_id,id,value) VALUES('missing','k',?1)",
                [encrypt_provider_key("secret").unwrap()],
            )
            .unwrap();
        assert!(migrate_connection(&mut connection).is_err());
        assert!(!migration::has_column(&connection, "provider_keys", "is_current").unwrap());
        assert!(migration::table_exists(&connection, "provider_key_usage").unwrap());
        assert!(!migration::table_exists(&connection, "provider_keys_before_usage").unwrap());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM provider_keys", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }
}

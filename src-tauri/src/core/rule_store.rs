use crate::core::database;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection, TransactionBehavior};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

const PROMPT_CLIS: [&str; 3] = ["common", "claude", "codex"];
const LEGACY_TABLES: [&str; 4] = [
    "rule_prompts",
    "rule_profiles",
    "rule_runtime_state",
    "rule_schema_migrations",
];
// 启用关系和同步基线属于本机，不参与跨设备备份恢复。
pub(crate) const LOCAL_COLUMNS: &[&str] = &[
    "enabled_clis",
    "claude_synced_hash",
    "claude_synced_at",
    "codex_synced_hash",
    "codex_synced_at",
];

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = database::open(paths)?;
    migrate_connection(&mut connection)?;
    migrate_legacy_json(paths, &mut connection)
}

pub fn read_prompts(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut statement = connection.prepare(
        "SELECT id, type, name, description, file_name, created_at, updated_at
         FROM rules ORDER BY type ASC, updated_at DESC, id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "cli": row.get::<_, String>(1)?,
            "name": row.get::<_, String>(2)?,
            "description": row.get::<_, Option<String>>(3)?,
            "fileName": row.get::<_, String>(4)?,
            "createdAt": row.get::<_, i64>(5)?,
            "updatedAt": row.get::<_, i64>(6)?
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn upsert_prompt(paths: &AppPaths, prompt: &Value) -> Result<(), ManagerError> {
    initialize(paths)?;
    upsert_metadata(&database::open(paths)?, prompt)
}

fn upsert_metadata(connection: &Connection, prompt: &Value) -> Result<(), ManagerError> {
    let id = string_field(prompt, "id");
    let cli = string_field(prompt, "cli");
    let file_name = string_field(prompt, "fileName");
    if id.is_empty() || !PROMPT_CLIS.contains(&cli.as_str()) || file_name.is_empty() {
        return Err(ManagerError::System(
            "规则元数据缺少有效的 id、cli 或 fileName".into(),
        ));
    }
    connection.execute(
        "INSERT INTO rules(id, type, name, description, file_name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
           type=excluded.type, name=excluded.name, description=excluded.description,
           file_name=excluded.file_name, created_at=excluded.created_at, updated_at=excluded.updated_at,
           enabled_clis=CASE excluded.type WHEN 'common' THEN rules.enabled_clis
             WHEN 'claude' THEN CASE WHEN instr(','||rules.enabled_clis||',',',claude,')>0 THEN 'claude' ELSE '' END
             ELSE CASE WHEN instr(','||rules.enabled_clis||',',',codex,')>0 THEN 'codex' ELSE '' END END,
           claude_synced_hash=CASE WHEN excluded.type IN ('common','claude') THEN rules.claude_synced_hash ELSE '' END,
           claude_synced_at=CASE WHEN excluded.type IN ('common','claude') THEN rules.claude_synced_at ELSE 0 END,
           codex_synced_hash=CASE WHEN excluded.type IN ('common','codex') THEN rules.codex_synced_hash ELSE '' END,
           codex_synced_at=CASE WHEN excluded.type IN ('common','codex') THEN rules.codex_synced_at ELSE 0 END",
        params![id, cli, string_field(prompt, "name"),
            prompt.get("description").and_then(Value::as_str), file_name,
            prompt.get("createdAt").and_then(Value::as_i64).unwrap_or(0),
            prompt.get("updatedAt").and_then(Value::as_i64).unwrap_or(0)],
    )?;
    Ok(())
}

pub fn delete_prompt(paths: &AppPaths, prompt_id: &str) -> Result<(), ManagerError> {
    initialize(paths)?;
    database::open(paths)?.execute("DELETE FROM rules WHERE id=?1", [prompt_id])?;
    Ok(())
}

pub fn read_profiles(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    Ok(read_runtime_state(paths)?
        .into_iter()
        .map(|(cli, state)| (cli, json!({"activePromptId": state["activePromptId"]})))
        .collect())
}

// 只重建 API 所需的视图，路径和漂移状态由调用方实时计算。
pub fn read_runtime_state(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut states = Map::new();
    for cli in ["claude", "codex"] {
        let mut statement = connection.prepare(&format!(
            "SELECT id, {cli}_synced_hash, {cli}_synced_at FROM rules WHERE instr(','||enabled_clis||',',',{cli},')>0"
        ))?;
        let mut rows = statement.query([])?;
        let state = if let Some(row) = rows.next()? {
            json!({"activePromptId": row.get::<_, String>(0)?,
                "runtimeHash": row.get::<_, String>(1)?, "lastSyncAt": row.get::<_, i64>(2)?})
        } else {
            json!({"activePromptId": "", "runtimeHash": "", "lastSyncAt": 0})
        };
        states.insert(cli.into(), state);
    }
    Ok(states)
}

// 同一个事务内切换启用关系和同步基线，公共规则可同时供两个 CLI 使用。
pub fn set_active_prompt(
    paths: &AppPaths,
    cli: &str,
    prompt_id: &str,
    runtime_hash: &str,
    synced_at: i64,
) -> Result<(), ManagerError> {
    validate_runtime_cli(cli)?;
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    set_active(&transaction, cli, prompt_id, runtime_hash, synced_at)?;
    transaction.commit()?;
    Ok(())
}

fn set_active(
    connection: &Connection,
    cli: &str,
    id: &str,
    hash: &str,
    synced_at: i64,
) -> Result<(), ManagerError> {
    if !id.is_empty()
        && !connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM rules WHERE id=?1 AND type IN ('common',?2))",
            params![id, cli],
            |row| row.get::<_, bool>(0),
        )?
    {
        return Err(ManagerError::System(format!("规则不存在或不适用于 {cli}")));
    }
    let other = if cli == "claude" { "codex" } else { "claude" };
    connection.execute(
        &format!(
            "UPDATE rules SET enabled_clis=CASE WHEN enabled_clis=?1 THEN '' ELSE ?2 END,
         {cli}_synced_hash='', {cli}_synced_at=0
         WHERE instr(','||enabled_clis||',',',{cli},')>0"
        ),
        params![cli, other],
    )?;
    if !id.is_empty() {
        connection.execute(&format!(
            "UPDATE rules SET enabled_clis=CASE WHEN enabled_clis=?1 THEN 'claude,codex' ELSE ?2 END,
             {cli}_synced_hash=?3, {cli}_synced_at=?4 WHERE id=?5"
        ), params![other, cli, hash, synced_at, id])?;
    }
    Ok(())
}

pub fn update_synced_hash(
    paths: &AppPaths,
    cli: &str,
    id: &str,
    hash: &str,
) -> Result<(), ManagerError> {
    validate_runtime_cli(cli)?;
    let connection = database::open(paths)?;
    // 只在双方内容重新一致且基线变化时写入，避免刷新产生无效写入。
    connection.execute(&format!(
        "UPDATE rules SET {cli}_synced_hash=?1 WHERE id=?2 AND instr(','||enabled_clis||',',',{cli},')>0 AND {cli}_synced_hash<>?1"
    ), params![hash, id])?;
    Ok(())
}

fn validate_runtime_cli(cli: &str) -> Result<(), ManagerError> {
    if !["claude", "codex"].contains(&cli) {
        return Err(ManagerError::System(
            "规则仅支持挂载到 Claude 或 Codex".into(),
        ));
    }
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, ManagerError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

// 主库与旧备份使用同一入口，建表、迁移、校验和删除旧表一起提交。
pub(crate) fn migrate_connection(connection: &mut Connection) -> Result<(), ManagerError> {
    let mut legacy_exists = false;
    for table in LEGACY_TABLES {
        legacy_exists |= table_exists(connection, table)?;
    }
    if table_exists(connection, "rules")? && !legacy_exists {
        return Ok(());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS rules (
           id TEXT PRIMARY KEY NOT NULL,
           type TEXT NOT NULL CHECK(type IN ('common','claude','codex')),
           name TEXT NOT NULL,
           description TEXT,
           file_name TEXT NOT NULL,
           created_at INTEGER NOT NULL DEFAULT 0,
           updated_at INTEGER NOT NULL DEFAULT 0,
           enabled_clis TEXT NOT NULL DEFAULT '' CHECK(enabled_clis IN ('','claude','codex','claude,codex')),
           claude_synced_hash TEXT NOT NULL DEFAULT '',
           claude_synced_at INTEGER NOT NULL DEFAULT 0,
           codex_synced_hash TEXT NOT NULL DEFAULT '',
           codex_synced_at INTEGER NOT NULL DEFAULT 0,
           CHECK(type='common' OR enabled_clis='' OR enabled_clis=type)
         );
         CREATE INDEX IF NOT EXISTS idx_rules_type_updated ON rules(type, updated_at DESC);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_rules_claude_enabled ON rules((1)) WHERE instr(','||enabled_clis||',',',claude,')>0;
         CREATE UNIQUE INDEX IF NOT EXISTS idx_rules_codex_enabled ON rules((1)) WHERE instr(','||enabled_clis||',',',codex,')>0;"
    )?;
    if table_exists(&transaction, "rule_prompts")? {
        let prompts = transaction
            .prepare(
                "SELECT prompt_id, cli, file_name, updated_at, payload_json FROM rule_prompts",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, cli, file_name, updated_at, payload) in prompts {
            let mut prompt: Value = serde_json::from_str(&payload)?;
            let object = prompt
                .as_object_mut()
                .ok_or_else(|| ManagerError::System(format!("规则 {id} 的旧数据格式无效")))?;
            object.insert("id".into(), json!(id));
            object.insert("cli".into(), json!(cli));
            object.insert("fileName".into(), json!(file_name));
            object.insert("updatedAt".into(), json!(updated_at));
            upsert_metadata(&transaction, &prompt)?;
        }
        let missing: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM rule_prompts old LEFT JOIN rules new ON new.id=old.prompt_id WHERE new.id IS NULL",
            [], |row| row.get(0),
        )?;
        if missing != 0 {
            return Err(ManagerError::System("规则迁移校验失败，保留旧表".into()));
        }
    }
    let profiles = read_legacy_map(&transaction, "rule_profiles")?;
    let runtime = read_legacy_map(&transaction, "rule_runtime_state")?;
    migrate_active_state(&transaction, &profiles, &runtime)?;
    for table in LEGACY_TABLES {
        transaction.execute_batch(&format!("DROP TABLE IF EXISTS {table}"))?;
    }
    transaction.commit()?;
    Ok(())
}

fn read_legacy_map(
    connection: &Connection,
    table: &str,
) -> Result<Map<String, Value>, ManagerError> {
    let mut values = Map::new();
    if !table_exists(connection, table)? {
        return Ok(values);
    }
    let mut statement = connection.prepare(&format!("SELECT cli, payload_json FROM {table}"))?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (cli, payload) = row?;
        values.insert(cli, serde_json::from_str(&payload)?);
    }
    Ok(values)
}

fn migrate_active_state(
    connection: &Connection,
    profiles: &Map<String, Value>,
    runtime: &Map<String, Value>,
) -> Result<(), ManagerError> {
    for cli in ["claude", "codex"] {
        let Some(profile) = profiles.get(cli) else {
            continue;
        };
        let id = string_field(profile, "activePromptId");
        // 旧缓存可能已悬空，只迁移有效的启用关系。
        if !id.is_empty()
            && !connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM rules WHERE id=?1 AND type IN ('common',?2))",
                params![id, cli],
                |row| row.get::<_, bool>(0),
            )?
        {
            continue;
        }
        let state = runtime
            .get(cli)
            .filter(|state| string_field(state, "activePromptId") == id);
        let hash = state
            .map(|state| string_field(state, "runtimeHash"))
            .unwrap_or_default();
        let synced_at = state
            .and_then(|state| state.get("lastSyncAt"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        set_active(connection, cli, &id, &hash, synced_at)?;
    }
    Ok(())
}

fn migrate_legacy_json(paths: &AppPaths, connection: &mut Connection) -> Result<(), ManagerError> {
    let legacy_prompts = read_legacy_prompts(paths)?;
    let legacy_profiles = read_legacy_profiles(paths)?;
    let runtime_path = PathBuf::from(&paths.storage_files.prompt_runtime_state);
    let legacy_runtime_state = read_json_file(&runtime_path)?;
    if legacy_prompts.is_empty() && legacy_profiles.is_empty() && legacy_runtime_state.is_none() {
        return Ok(());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (_, prompt) in &legacy_prompts {
        upsert_metadata(&transaction, prompt)?;
    }
    let profiles = legacy_profiles
        .iter()
        .map(|(cli, _, profile)| (cli.clone(), profile.clone()))
        .collect();
    let runtime = legacy_runtime_state
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    migrate_active_state(&transaction, &profiles, &runtime)?;
    transaction.commit()?;

    for (path, _) in legacy_prompts {
        remove_file_family(&path)?;
    }
    for (_, path, _) in legacy_profiles {
        remove_file_family(&path)?;
    }
    if legacy_runtime_state.is_some() {
        remove_file_family(&runtime_path)?;
    }
    Ok(())
}

fn read_legacy_prompts(paths: &AppPaths) -> Result<Vec<(PathBuf, Value)>, ManagerError> {
    let mut prompts = Vec::new();

    for cli in PROMPT_CLIS {
        let prompt_dir = Path::new(&paths.prompts_dir).join(cli);

        if !prompt_dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(prompt_dir)? {
            let path = entry?.path();

            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }

            let prompt = read_json_file(&path)?.unwrap_or(Value::Null);
            if string_field(&prompt, "id").is_empty()
                || string_field(&prompt, "cli").is_empty()
                || string_field(&prompt, "fileName").is_empty()
            {
                continue;
            }
            prompts.push((path, prompt));
        }
    }
    Ok(prompts)
}

fn read_legacy_profiles(paths: &AppPaths) -> Result<Vec<(String, PathBuf, Value)>, ManagerError> {
    let mut profiles = Vec::new();

    for cli in ["claude", "codex"] {
        let path = Path::new(&paths.prompt_profiles_dir).join(format!("{cli}-profile.json"));

        if let Some(profile) = read_json_file(&path)? {
            profiles.push((cli.to_string(), path, profile));
        }
    }
    Ok(profiles)
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

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::resolve_app_paths;

    fn test_paths() -> AppPaths {
        resolve_app_paths(
            &std::env::temp_dir().join(format!("rule-store-{}", uuid::Uuid::new_v4())),
        )
    }

    fn legacy_schema(connection: &Connection) {
        connection.execute_batch(
            "CREATE TABLE rule_prompts(prompt_id TEXT PRIMARY KEY, cli TEXT NOT NULL,
             file_name TEXT NOT NULL, updated_at INTEGER NOT NULL, payload_json TEXT NOT NULL);
             CREATE TABLE rule_profiles(cli TEXT PRIMARY KEY, payload_json TEXT NOT NULL);
             CREATE TABLE rule_runtime_state(cli TEXT PRIMARY KEY, payload_json TEXT NOT NULL);
             CREATE TABLE rule_schema_migrations(version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
             INSERT INTO rule_schema_migrations VALUES(1,100);"
        ).unwrap();
    }

    fn prompt(id: &str, cli: &str) -> Value {
        json!({"id": id, "cli": cli, "name": id, "description": "说明",
            "fileName": format!("{id}.md"), "createdAt": 12, "updatedAt": 34})
    }

    #[test]
    fn merges_legacy_tables_and_drops_them_after_validation() {
        let mut connection = Connection::open_in_memory().unwrap();
        legacy_schema(&connection);
        connection
            .execute(
                "INSERT INTO rule_prompts VALUES('shared','common','shared.md',34,?1)",
                [prompt("shared", "common").to_string()],
            )
            .unwrap();
        for (cli, hash, time) in [("claude", "hash-a", 50), ("codex", "hash-b", 60)] {
            connection
                .execute(
                    "INSERT INTO rule_profiles VALUES(?1,?2)",
                    params![cli, json!({"activePromptId":"shared"}).to_string()],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO rule_runtime_state VALUES(?1,?2)",
                    params![
                        cli,
                        json!({"activePromptId":"shared", "runtimeHash":hash,"lastSyncAt":time,
                    "runtimePath":"C:/old/AGENTS.md", "status":"SYNCED"})
                        .to_string()
                    ],
                )
                .unwrap();
        }
        migrate_connection(&mut connection).unwrap();
        migrate_connection(&mut connection).unwrap();
        for table in LEGACY_TABLES {
            assert!(!table_exists(&connection, table).unwrap());
        }
        let row = connection.query_row(
            "SELECT type, enabled_clis, name, description, created_at, updated_at,
             claude_synced_hash, claude_synced_at, codex_synced_hash, codex_synced_at FROM rules WHERE id='shared'",
            [], |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?,
                row.get::<_,String>(3)?, row.get::<_,i64>(4)?, row.get::<_,i64>(5)?,
                row.get::<_,String>(6)?, row.get::<_,i64>(7)?, row.get::<_,String>(8)?, row.get::<_,i64>(9)?))
        ).unwrap();
        assert_eq!(
            row,
            (
                "common".into(),
                "claude,codex".into(),
                "shared".into(),
                "说明".into(),
                12,
                34,
                "hash-a".into(),
                50,
                "hash-b".into(),
                60
            )
        );
        let columns = connection
            .prepare("PRAGMA table_info(rules)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for removed in ["payload_json", "runtime_path", "status", "cli"] {
            assert!(!columns.iter().any(|column| column == removed));
        }
    }

    #[test]
    fn invalid_legacy_data_rolls_back_schema_and_keeps_source() {
        let mut connection = Connection::open_in_memory().unwrap();
        legacy_schema(&connection);
        connection
            .execute_batch(
                "INSERT INTO rule_prompts VALUES('broken','common','broken.md',1,'invalid json')",
            )
            .unwrap();
        assert!(migrate_connection(&mut connection).is_err());
        assert!(!table_exists(&connection, "rules").unwrap());
        for table in LEGACY_TABLES {
            assert!(table_exists(&connection, table).unwrap());
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM rule_prompts", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn migration_does_not_attach_stale_runtime_hash_to_another_rule() {
        let mut connection = Connection::open_in_memory().unwrap();
        legacy_schema(&connection);
        connection
            .execute(
                "INSERT INTO rule_prompts VALUES('shared','common','shared.md',34,?1)",
                [prompt("shared", "common").to_string()],
            )
            .unwrap();
        connection.execute_batch(
            r#"INSERT INTO rule_profiles VALUES('claude','{"activePromptId":"shared"}');
            INSERT INTO rule_profiles VALUES('codex','{"activePromptId":"missing"}');
            INSERT INTO rule_runtime_state VALUES('claude','{"activePromptId":"other","runtimeHash":"stale","lastSyncAt":55}');"#
        ).unwrap();
        migrate_connection(&mut connection).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT enabled_clis,claude_synced_hash,claude_synced_at FROM rules",
                    [],
                    |row| Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?
                    ))
                )
                .unwrap(),
            ("claude".into(), "".into(), 0)
        );
    }

    #[test]
    fn migrates_rule_json_metadata_and_preserves_markdown() {
        let paths = test_paths();
        let prompt_dir = Path::new(&paths.prompts_dir).join("common");
        std::fs::create_dir_all(&prompt_dir).unwrap();
        std::fs::create_dir_all(&paths.prompt_profiles_dir).unwrap();
        std::fs::create_dir_all(&paths.storage_dir).unwrap();
        std::fs::write(prompt_dir.join("shared.md"), "Prompt A\n").unwrap();
        std::fs::write(
            prompt_dir.join("shared.json"),
            prompt("shared", "common").to_string(),
        )
        .unwrap();
        std::fs::write(prompt_dir.join("shared.json.bak"), "backup").unwrap();
        let profile_path = Path::new(&paths.prompt_profiles_dir).join("claude-profile.json");
        std::fs::write(
            &profile_path,
            json!({"activePromptId":"shared"}).to_string(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.prompt_runtime_state,
            json!({"claude":{"activePromptId":"shared","runtimeHash":"hash","lastSyncAt":100}})
                .to_string(),
        )
        .unwrap();
        assert_eq!(
            read_prompts(&paths).unwrap(),
            vec![prompt("shared", "common")]
        );
        assert_eq!(
            read_profiles(&paths).unwrap()["claude"]["activePromptId"],
            "shared"
        );
        assert_eq!(
            read_runtime_state(&paths).unwrap()["claude"]["runtimeHash"],
            "hash"
        );
        assert!(prompt_dir.join("shared.md").exists());
        assert!(!prompt_dir.join("shared.json").exists());
        assert!(!prompt_dir.join("shared.json.bak").exists());
        assert!(!profile_path.exists());
        assert!(!Path::new(&paths.storage_files.prompt_runtime_state).exists());
        std::fs::remove_dir_all(&paths.user_data_path).unwrap();
    }

    #[test]
    fn cli_switches_are_independent_and_metadata_edits_preserve_local_state() {
        let paths = test_paths();
        upsert_prompt(&paths, &prompt("shared", "common")).unwrap();
        upsert_prompt(&paths, &prompt("claude-only", "claude")).unwrap();
        set_active_prompt(&paths, "claude", "shared", "hash-a", 100).unwrap();
        set_active_prompt(&paths, "codex", "shared", "hash-b", 200).unwrap();
        upsert_prompt(&paths, &prompt("shared", "common")).unwrap();
        let connection = database::open(&paths).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT enabled_clis FROM rules WHERE id='shared'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "claude,codex"
        );
        // 数据库也约束一个 CLI 同时只能挂载一个规则。
        assert!(connection
            .execute(
                "UPDATE rules SET enabled_clis='claude' WHERE id='claude-only'",
                []
            )
            .is_err());
        assert!(set_active_prompt(&paths, "codex", "claude-only", "", 0).is_err());
        set_active_prompt(&paths, "claude", "claude-only", "hash-c", 300).unwrap();
        let runtime = read_runtime_state(&paths).unwrap();
        assert_eq!(runtime["claude"]["activePromptId"], "claude-only");
        assert_eq!(runtime["codex"]["activePromptId"], "shared");
        assert_eq!(runtime["codex"]["runtimeHash"], "hash-b");
        set_active_prompt(&paths, "claude", "", "", 0).unwrap();
        assert_eq!(
            read_profiles(&paths).unwrap()["claude"]["activePromptId"],
            ""
        );
        // 缩小规则类型时，自动移除不适用的启用关系与基线。
        upsert_prompt(&paths, &prompt("shared", "claude")).unwrap();
        assert_eq!(
            read_profiles(&paths).unwrap()["codex"]["activePromptId"],
            ""
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT codex_synced_hash,codex_synced_at FROM rules WHERE id='shared'",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                )
                .unwrap(),
            ("".into(), 0)
        );
        delete_prompt(&paths, "shared").unwrap();
        assert_eq!(read_prompts(&paths).unwrap().len(), 1);
        drop(connection);
        std::fs::remove_dir_all(&paths.user_data_path).unwrap();
    }

    #[test]
    fn concurrent_initialization_is_idempotent() {
        let paths = test_paths();
        database::initialize(&paths).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let threads = (0..4)
            .map(|_| {
                let paths = paths.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    initialize(&paths).unwrap();
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(read_prompts(&paths).unwrap().is_empty());
        std::fs::remove_dir_all(&paths.user_data_path).unwrap();
    }
}

use crate::core::database;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 1;
const PROMPT_CLIS: [&str; 3] = ["common", "claude", "codex"];

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = database::open(paths)?;
    create_schema(&connection)?;
    migrate_legacy_json(paths, &mut connection)
}

pub fn read_prompts(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut statement = connection.prepare(
        "SELECT payload_json FROM rule_prompts
         ORDER BY cli ASC, updated_at DESC, prompt_id ASC",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut prompts = Vec::new();

    for row in rows {
        prompts.push(serde_json::from_str(&row?)?);
    }
    Ok(prompts)
}

pub fn upsert_prompt(paths: &AppPaths, prompt: &Value) -> Result<(), ManagerError> {
    initialize(paths)?;
    let prompt_id = string_field(prompt, "id");
    let cli = string_field(prompt, "cli");
    let file_name = string_field(prompt, "fileName");

    if prompt_id.is_empty() || cli.is_empty() || file_name.is_empty() {
        return Err(ManagerError::System(
            "Prompt 元数据缺少 id、cli 或 fileName".to_string(),
        ));
    }

    let connection = database::open(paths)?;
    connection.execute(
        "INSERT INTO rule_prompts(
           prompt_id, cli, file_name, updated_at, payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(prompt_id) DO UPDATE SET
           cli = excluded.cli,
           file_name = excluded.file_name,
           updated_at = excluded.updated_at,
           payload_json = excluded.payload_json",
        params![
            prompt_id,
            cli,
            file_name,
            prompt.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
            serde_json::to_string(prompt)?
        ],
    )?;
    Ok(())
}

pub fn delete_prompt(paths: &AppPaths, prompt_id: &str) -> Result<(), ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    connection.execute(
        "DELETE FROM rule_prompts WHERE prompt_id = ?1",
        params![prompt_id],
    )?;
    Ok(())
}

pub fn read_profiles(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    read_map_table(paths, "rule_profiles", "cli")
}

pub fn write_profile(paths: &AppPaths, cli: &str, profile: &Value) -> Result<(), ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    connection.execute(
        "INSERT INTO rule_profiles(cli, payload_json) VALUES (?1, ?2)
         ON CONFLICT(cli) DO UPDATE SET payload_json = excluded.payload_json",
        params![cli, serde_json::to_string(profile)?],
    )?;
    Ok(())
}

pub fn read_runtime_state(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    read_map_table(paths, "rule_runtime_state", "cli")
}

pub fn write_runtime_state(
    paths: &AppPaths,
    runtime_state: &Map<String, Value>,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction()?;

    transaction.execute("DELETE FROM rule_runtime_state", [])?;
    for (cli, payload) in runtime_state {
        transaction.execute(
            "INSERT INTO rule_runtime_state(cli, payload_json) VALUES (?1, ?2)",
            params![cli, serde_json::to_string(payload)?],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn create_schema(connection: &Connection) -> Result<(), ManagerError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS rule_schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS rule_prompts (
           prompt_id TEXT PRIMARY KEY,
           cli TEXT NOT NULL,
           file_name TEXT NOT NULL,
           updated_at INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_rule_prompts_cli_updated_at
           ON rule_prompts(cli, updated_at DESC);
         CREATE TABLE IF NOT EXISTS rule_profiles (
           cli TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS rule_runtime_state (
           cli TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO rule_schema_migrations(version, applied_at)
         VALUES (?1, ?2)",
        params![SCHEMA_VERSION, now_millis()],
    )?;
    Ok(())
}

fn read_map_table(
    paths: &AppPaths,
    table: &str,
    key_column: &str,
) -> Result<Map<String, Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let sql = format!("SELECT {key_column}, payload_json FROM {table} ORDER BY {key_column} ASC");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut values = Map::new();

    for row in rows {
        let (key, payload) = row?;
        values.insert(key, serde_json::from_str(&payload)?);
    }
    Ok(values)
}

fn migrate_legacy_json(paths: &AppPaths, connection: &mut Connection) -> Result<(), ManagerError> {
    let legacy_prompts = read_legacy_prompts(paths)?;
    let legacy_profiles = read_legacy_profiles(paths)?;
    let runtime_path = PathBuf::from(&paths.storage_files.prompt_runtime_state);
    let legacy_runtime_state = read_json_file(&runtime_path)?;

    if legacy_prompts.is_empty() && legacy_profiles.is_empty() && legacy_runtime_state.is_none() {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    for (_, prompt) in &legacy_prompts {
        transaction.execute(
            "INSERT INTO rule_prompts(
               prompt_id, cli, file_name, updated_at, payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(prompt_id) DO UPDATE SET
               cli = excluded.cli,
               file_name = excluded.file_name,
               updated_at = excluded.updated_at,
               payload_json = excluded.payload_json",
            params![
                string_field(prompt, "id"),
                string_field(prompt, "cli"),
                string_field(prompt, "fileName"),
                prompt.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
                serde_json::to_string(prompt)?
            ],
        )?;
    }
    for (cli, _, profile) in &legacy_profiles {
        transaction.execute(
            "INSERT INTO rule_profiles(cli, payload_json) VALUES (?1, ?2)
             ON CONFLICT(cli) DO UPDATE SET payload_json = excluded.payload_json",
            params![cli, serde_json::to_string(profile)?],
        )?;
    }
    if let Some(runtime_state) = &legacy_runtime_state {
        for (cli, payload) in runtime_state.as_object().cloned().unwrap_or_default() {
            transaction.execute(
                "INSERT INTO rule_runtime_state(cli, payload_json) VALUES (?1, ?2)
                 ON CONFLICT(cli) DO UPDATE SET payload_json = excluded.payload_json",
                params![cli, serde_json::to_string(&payload)?],
            )?;
        }
    }
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

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{read_profiles, read_prompts, read_runtime_state};
    use crate::core::paths::resolve_app_paths;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn migrates_rule_json_metadata_into_main_database() {
        let root =
            std::env::temp_dir().join(format!("monkey-thief-rule-store-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let paths = resolve_app_paths(Path::new(&root));
        let prompt_dir = Path::new(&paths.prompts_dir).join("common");
        std::fs::create_dir_all(&prompt_dir).unwrap();
        std::fs::create_dir_all(&paths.prompt_profiles_dir).unwrap();
        std::fs::create_dir_all(&paths.storage_dir).unwrap();
        std::fs::write(prompt_dir.join("prompt-a.md"), "Prompt A\n").unwrap();
        std::fs::write(
            prompt_dir.join("prompt-a.json"),
            serde_json::to_string(&json!({
              "id": "prompt-a",
              "cli": "common",
              "fileName": "prompt-a.md",
              "updatedAt": 100
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            Path::new(&paths.prompt_profiles_dir).join("claude-profile.json"),
            serde_json::to_string(&json!({"activePromptId": "prompt-a"})).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &paths.storage_files.prompt_runtime_state,
            serde_json::to_string(&json!({"claude": {"status": "SYNCED"}})).unwrap(),
        )
        .unwrap();

        assert_eq!(read_prompts(&paths).unwrap()[0]["id"], "prompt-a");
        assert_eq!(
            read_profiles(&paths).unwrap()["claude"]["activePromptId"],
            "prompt-a"
        );
        assert_eq!(
            read_runtime_state(&paths).unwrap()["claude"]["status"],
            "SYNCED"
        );
        assert!(prompt_dir.join("prompt-a.md").exists());
        assert!(!prompt_dir.join("prompt-a.json").exists());
        assert!(!Path::new(&paths.prompt_profiles_dir)
            .join("claude-profile.json")
            .exists());
        assert!(!Path::new(&paths.storage_files.prompt_runtime_state).exists());
    }
}

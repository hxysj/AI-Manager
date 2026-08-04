use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use rusqlite::{params, Connection, OptionalExtension, MAIN_DB};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

// 仅跨设备恢复的业务数据进入备份，其他表只保留结构。
const BACKUP_INCLUDED_TABLES: [&str; 7] = [
    "providers",
    "provider_models",
    "codex_accounts",
    "skills",
    "skill_groups",
    "skill_repositories",
    "rule_prompts",
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
    let tables = read_table_names(&connection, "main")?;
    let transaction = connection.transaction()?;

    for table in tables {
        if !BACKUP_INCLUDED_TABLES.contains(&table.as_str()) {
            transaction.execute(&format!("DELETE FROM {}", quote_identifier(&table)), [])?;
        }
    }
    // 本机启用态、安装态和用量不随业务数据跨设备恢复。
    strip_json_fields(&transaction, "providers", &["enabled"])?;
    strip_json_fields(
        &transaction,
        "skills",
        &[
            "disabled",
            "installedTargets",
            "installStates",
            "status",
            "sourcePath",
            "entryPath",
            "repoName",
        ],
    )?;
    strip_json_fields(&transaction, "codex_accounts", &["usage", "disabled"])?;

    transaction.commit()?;
    connection.execute_batch("VACUUM;")?;
    Ok(())
}

fn strip_json_fields(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
    fields: &[&str],
) -> Result<(), ManagerError> {
    let exists = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_master
           WHERE type = 'table' AND name = ?1
         )",
        params![table],
        |row| row.get::<_, bool>(0),
    )?;

    if !exists {
        return Ok(());
    }

    let rows = {
        let mut statement = transaction.prepare(&format!(
            "SELECT item_key, payload_json FROM {}",
            quote_identifier(table)
        ))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    for (item_key, payload) in rows {
        let mut value: serde_json::Value = serde_json::from_str(&payload)?;

        if let Some(value) = value.as_object_mut() {
            for field in fields {
                value.remove(*field);
            }
        }
        transaction.execute(
            &format!(
                "UPDATE {} SET payload_json = ?1 WHERE item_key = ?2",
                quote_identifier(table)
            ),
            params![serde_json::to_string(&value)?, item_key],
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

pub fn reconcile_local_state(
    paths: &AppPaths,
    providers_changed: bool,
    provider_models_changed: bool,
    codex_accounts_changed: bool,
    rule_prompts_changed: bool,
) -> Result<(), ManagerError> {
    if !providers_changed
        && !provider_models_changed
        && !codex_accounts_changed
        && !rule_prompts_changed
    {
        return Ok(());
    }
    initialize(paths)?;
    let mut connection = open(paths)?;
    let transaction = connection.transaction()?;

    if providers_changed || provider_models_changed {
        reconcile_provider_relations(&transaction, providers_changed)?;
    }
    if codex_accounts_changed {
        clear_missing_codex_active_account(&transaction)?;
    }
    if rule_prompts_changed {
        clear_missing_rule_prompt_state(&transaction)?;
    }
    transaction.commit()?;
    Ok(())
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
        let restores_providers = restore_tables.iter().any(|table| table == "providers");
        let restores_provider_models = restore_tables
            .iter()
            .any(|table| table == "provider_models");
        let transaction = connection.transaction()?;

        // 只替换用户选择的业务表，保留当前库结构和本机运行态数据。
        for table in restore_tables {
            if table == "providers" {
                restore_providers(&transaction)?;
            } else if table == "skills" {
                restore_skills(&transaction)?;
            } else if table == "codex_accounts" {
                restore_codex_accounts(&transaction)?;
            } else if table == "rule_prompts" {
                restore_rule_prompts(&transaction)?;
            } else {
                restore_table(&transaction, &table)?;
            }
        }
        if restores_providers || restores_provider_models {
            reconcile_provider_relations(&transaction, restores_providers)?;
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
            enabled.insert(
                item_key,
                provider
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
            );
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

    Ok(())
}

fn reconcile_provider_relations(
    transaction: &rusqlite::Transaction<'_>,
    providers_changed: bool,
) -> Result<(), ManagerError> {
    let providers = {
        let mut statement = transaction.prepare("SELECT item_key, payload_json FROM providers")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut providers = HashMap::new();

        for (item_key, payload) in rows {
            let provider: serde_json::Value = serde_json::from_str(&payload)?;
            providers.insert(
                item_key,
                provider
                    .get("cli")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        providers
    };
    if providers_changed {
        reconcile_provider_runtime_state(transaction, &providers)?;
        transaction.execute(
            "DELETE FROM provider_keys
             WHERE provider_id NOT IN (SELECT item_key FROM providers)",
            [],
        )?;
    }
    transaction.execute(
        "DELETE FROM provider_models
         WHERE COALESCE(json_extract(payload_json, '$.providerId'), '') = ''
            OR json_extract(payload_json, '$.providerId') NOT IN (
              SELECT item_key FROM providers
            )",
        [],
    )?;

    Ok(())
}

fn reconcile_provider_runtime_state(
    transaction: &rusqlite::Transaction<'_>,
    providers: &HashMap<String, String>,
) -> Result<(), ManagerError> {
    let profiles = {
        let mut statement = transaction.prepare(
            "SELECT item_key, payload_json FROM provider_runtime_profiles ORDER BY item_key",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    for (item_key, payload) in profiles {
        let profile: serde_json::Value = serde_json::from_str(&payload)?;
        let provider_id = profile
            .get("providerId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let cli = profile
            .get("cli")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let provider_matches_cli = providers
            .get(provider_id)
            .is_some_and(|provider_cli| provider_cli == cli);

        if provider_matches_cli {
            continue;
        }

        transaction.execute(
            "DELETE FROM provider_runtime_profiles WHERE item_key = ?1",
            params![item_key],
        )?;
    }

    let runtime_states = {
        let mut statement = transaction.prepare(
            "SELECT cli, payload_json FROM provider_runtime_state ORDER BY cli",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (cli, payload) in runtime_states {
        let mut runtime_state: serde_json::Value = serde_json::from_str(&payload)?;
        let active_provider_id = runtime_state
            .get("activeProviderId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        if active_provider_id.is_empty()
            || providers
                .get(active_provider_id)
                .is_some_and(|provider_cli| provider_cli == &cli)
        {
            continue;
        }
        let runtime_state = runtime_state.as_object_mut().ok_or_else(|| {
            ManagerError::System(format!("Provider Runtime State {cli} 的数据格式无效。"))
        })?;
        runtime_state.insert(
            "activeProviderId".to_string(),
            serde_json::Value::String(String::new()),
        );
        runtime_state.insert(
            "status".to_string(),
            serde_json::Value::String("NO_ACTIVE".to_string()),
        );
        transaction.execute(
            "UPDATE provider_runtime_state SET payload_json = ?1 WHERE cli = ?2",
            params![serde_json::to_string(&runtime_state)?, cli],
        )?;
    }

    Ok(())
}

fn restore_skills(transaction: &rusqlite::Transaction<'_>) -> Result<(), ManagerError> {
    let current_state = {
        let mut statement =
            transaction.prepare("SELECT item_key, sort_order, payload_json FROM skills")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut state = HashMap::new();

        for (item_key, sort_order, payload) in rows {
            let skill: serde_json::Value = serde_json::from_str(&payload)?;
            state.insert(item_key, (sort_order, skill));
        }
        state
    };

    restore_table(transaction, "skills")?;
    let restored = {
        let mut statement = transaction.prepare("SELECT item_key, payload_json FROM skills")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut restored_keys = HashSet::new();

    for (item_key, payload) in restored {
        restored_keys.insert(item_key.clone());
        let mut skill: serde_json::Value = serde_json::from_str(&payload)?;
        let skill = skill
            .as_object_mut()
            .ok_or_else(|| ManagerError::System(format!("Skill {item_key} 的数据格式无效。")))?;
        let current = current_state.get(&item_key).map(|(_, value)| value);
        let disabled = current
            .and_then(|value| value.get("disabled"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(current.is_none());
        skill.insert("disabled".to_string(), serde_json::Value::Bool(disabled));
        skill.insert(
            "installedTargets".to_string(),
            current
                .and_then(|value| value.get("installedTargets"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        );
        skill.insert(
            "installStates".to_string(),
            current
                .and_then(|value| value.get("installStates"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        );
        skill.insert(
            "status".to_string(),
            current
                .and_then(|value| value.get("status"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!("disabled")),
        );
        transaction.execute(
            "UPDATE skills SET payload_json = ?1 WHERE item_key = ?2",
            params![serde_json::to_string(&skill)?, item_key],
        )?;
    }

    for (item_key, (sort_order, current_skill)) in &current_state {
        if restored_keys.contains(item_key) {
            continue;
        }
        let mut skill = current_skill.clone();
        let skill = skill
            .as_object_mut()
            .ok_or_else(|| ManagerError::System(format!("Skill {item_key} 的数据格式无效。")))?;
        skill.insert("disabled".to_string(), serde_json::Value::Bool(true));
        skill.insert("installedTargets".to_string(), serde_json::json!([]));
        skill.insert("installStates".to_string(), serde_json::json!({}));
        skill.insert("status".to_string(), serde_json::json!("disabled"));
        transaction.execute(
            "INSERT INTO skills(item_key, sort_order, payload_json) VALUES (?1, ?2, ?3)",
            params![item_key, sort_order, serde_json::to_string(&skill)?],
        )?;
        // 保留安装索引到恢复后的 Skill 重扫阶段，用于准确卸载当前 CLI 中的旧链接。
    }

    let installed_skills = {
        let mut statement = transaction.prepare("SELECT skill_name FROM skill_installs")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for skill_name in installed_skills {
        let exists = transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM skills
               WHERE item_key = ?1 OR json_extract(payload_json, '$.name') = ?1
             )",
            params![skill_name],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            transaction.execute(
                "DELETE FROM skill_installs WHERE skill_name = ?1",
                params![skill_name],
            )?;
        }
    }

    Ok(())
}

fn restore_rule_prompts(transaction: &rusqlite::Transaction<'_>) -> Result<(), ManagerError> {
    restore_table(transaction, "rule_prompts")?;
    clear_missing_rule_prompt_state(transaction)
}

fn clear_missing_rule_prompt_state(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<(), ManagerError> {
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
    let current_state = {
        let mut statement =
            transaction.prepare("SELECT item_key, payload_json FROM codex_accounts")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut state = HashMap::new();

        for (item_key, payload) in rows {
            let account: serde_json::Value = serde_json::from_str(&payload)?;
            state.insert(
                item_key,
                (
                    account
                        .get("usage")
                        .filter(|value| is_json_truthy(value))
                        .cloned(),
                    account
                        .get("disabled")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                ),
            );
        }
        state
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

    // usage 和 disabled 都属于本机状态；备份新增的官方账号默认禁用。
    for (item_key, payload) in restored {
        let mut account: serde_json::Value = serde_json::from_str(&payload)?;
        let account = account.as_object_mut().ok_or_else(|| {
            ManagerError::System(format!("Codex 账号 {item_key} 的数据格式无效。"))
        })?;
        let current = current_state.get(&item_key);
        if let Some(usage) = current.and_then(|(usage, _)| usage.as_ref()) {
            account.insert("usage".to_string(), usage.clone());
        } else {
            account.remove("usage");
        }
        account.insert(
            "disabled".to_string(),
            serde_json::Value::Bool(current.map(|(_, disabled)| *disabled).unwrap_or(true)),
        );
        transaction.execute(
            "UPDATE codex_accounts SET payload_json = ?1 WHERE item_key = ?2",
            params![serde_json::to_string(&account)?, item_key],
        )?;
    }

    clear_missing_codex_active_account(transaction)
}

fn clear_missing_codex_active_account(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<(), ManagerError> {
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
        transaction.execute(
            "UPDATE codex_active_account SET account_id = '' WHERE singleton_id = 1",
            [],
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
        "skills" => format!(
            "json_remove({column}, '$.disabled', '$.installedTargets', '$.installStates', '$.status',
             '$.sourcePath', '$.entryPath', '$.repoName', {runtime_paths})"
        ),
        "codex_accounts" => format!(
            "json_remove({column}, '$.usage', '$.disabled', {runtime_paths},
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
        backup, open, preview_restore, read_table_names, restore_selected, BACKUP_INCLUDED_TABLES,
    };
    use crate::core::paths::resolve_app_paths;
    use crate::core::{provider_store, rule_store, skill_store, usage_store};
    use rusqlite::Connection;
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
                 INSERT INTO future_runtime_data(id) VALUES (1);",
            )
            .unwrap();
        drop(connection);

        let snapshot_path = root.join("snapshot.db");
        std::fs::write(&snapshot_path, backup(&paths).unwrap()).unwrap();
        let snapshot = Connection::open(snapshot_path).unwrap();

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
        let skill: serde_json::Value = serde_json::from_str(
            &snapshot
                .query_row(
                    "SELECT payload_json FROM skills WHERE item_key = 'skill-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        let account: serde_json::Value = serde_json::from_str(
            &snapshot
                .query_row(
                    "SELECT payload_json FROM codex_accounts WHERE item_key = 'account-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();

        assert!(provider.get("enabled").is_none());
        for field in [
            "disabled",
            "installedTargets",
            "installStates",
            "status",
            "sourcePath",
            "entryPath",
            "repoName",
        ] {
            assert_eq!(
                skill.get(field),
                None,
                "Skill 本机字段 {field} 不应进入备份"
            );
        }
        assert!(account.get("usage").is_none());
        assert!(account.get("disabled").is_none());
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
        rule_store::write_profile(
            &target_paths,
            "claude",
            &json!({"activePromptId": "rule-a"}),
        )
        .unwrap();
        rule_store::write_profile(
            &target_paths,
            "codex",
            &json!({"activePromptId": "rule-local"}),
        )
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
                "skills".to_string(),
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
            "current"
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
        assert_eq!(provider_a["enabled"], true);
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
        assert!(!provider_store::read_keys(&target_paths)
            .unwrap()
            .contains_key("provider-local"));
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
        assert_eq!(account_a["disabled"], true);
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
        assert!(account_b.get("usage").is_none());
        assert_eq!(account_b["disabled"], true);
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
        let skill_a: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM skills WHERE item_key = 'skill-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        let skill_b: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM skills WHERE item_key = 'skill-b'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(skill_a["description"], "backup");
        assert_eq!(skill_a["disabled"], false);
        assert_eq!(skill_a["installedTargets"], json!(["codex"]));
        assert_eq!(skill_a["status"], "installed");
        assert_eq!(skill_b["disabled"], true);
        assert_eq!(skill_b["installedTargets"], json!([]));
        assert_eq!(skill_b["installStates"], json!({}));
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
        let skill_local: serde_json::Value = serde_json::from_str(
            &restored
                .query_row(
                    "SELECT payload_json FROM skills WHERE item_key = 'skill-local'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(skill_local["disabled"], true);
        assert_eq!(skill_local["status"], "disabled");
        let installs = skill_store::read_installs(&target_paths).unwrap();
        assert_eq!(installs["skill-a"], json!(["codex"]));
        assert_eq!(installs["skill-local"], json!(["codex"]));
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
            &["providers".to_string(), "provider_models".to_string()],
        )
        .unwrap();

        let providers = provider_store::read_providers(&target_paths).unwrap();
        let provider_a = providers.iter().find(|item| item["id"] == "provider-a").unwrap();
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
        assert!(profile_ids.is_empty());
        assert_eq!(runtime_state["codex"]["activeProviderId"], "provider-valid");
        assert_eq!(runtime_state["gemini"]["activeProviderId"], "");
        assert_eq!(runtime_state["gemini"]["status"], "NO_ACTIVE");
        assert_eq!(model_ids, vec!["model-a", "model-valid", "model-new"]);
    }

    #[test]
    fn restoring_providers_only_removes_models_for_missing_providers() {
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
        assert_eq!(models[0]["id"], "model-a");
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

        let differences = preview_restore(&target_paths, &backup(&source_paths).unwrap()).unwrap();

        assert!(!differences.iter().any(|item| item.table == "providers"));
        assert!(!differences.iter().any(|item| item.table == "skills"));
        assert!(!differences
            .iter()
            .any(|item| item.table == "codex_accounts"));
    }
}

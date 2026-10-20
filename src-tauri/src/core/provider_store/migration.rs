use super::*;

pub(super) fn table_exists(connection: &Connection, table: &str) -> Result<bool, ManagerError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

pub(super) fn has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, ManagerError> {
    let columns = connection
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

fn legacy_items(connection: &Connection, table: &str) -> Result<Vec<Value>, ManagerError> {
    if !table_exists(connection, table)? {
        return Ok(Vec::new());
    }
    let mut statement = connection.prepare(&format!(
        "SELECT item_key,payload_json FROM {table} ORDER BY sort_order,item_key"
    ))?;
    let mut items = Vec::new();
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (id, raw) = row?;
        let mut item: Value = serde_json::from_str(&raw)?;
        if !item.is_object() {
            return Err(ManagerError::System(format!(
                "{table} 中的 {id} 格式无效，迁移已回滚"
            )));
        }
        if item.get("id").is_none() {
            item["id"] = json!(id);
        }
        items.push(item);
    }
    Ok(items)
}

fn legacy_map(
    connection: &Connection,
    table: &str,
    key: &str,
) -> Result<Map<String, Value>, ManagerError> {
    if !table_exists(connection, table)? {
        return Ok(Map::new());
    }
    let mut result = Map::new();
    for row in connection
        .prepare(&format!("SELECT {key},payload_json FROM {table}"))?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
    {
        let (id, raw) = row?;
        result.insert(id, serde_json::from_str(&raw)?);
    }
    Ok(result)
}

pub(crate) fn migrate_connection(connection: &mut Connection) -> Result<(), ManagerError> {
    crate::core::provider_key_usage::migrate_bindings(connection)?;
    let columns = connection
        .prepare("PRAGMA table_info(providers)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    let old = columns.iter().any(|column| column == "payload_json");
    let mut extra = false;
    for table in OLD_TABLES {
        extra |= table_exists(connection, table)?;
    }
    let legacy_keys = has_column(connection, "provider_keys", "payload_json")?;
    if !old
        && columns.iter().any(|column| column == "source_provider_id")
        && !extra
        && !legacy_keys
        && has_column(connection, "provider_keys", "is_current")?
        && !table_exists(connection, "provider_key_usage")?
    {
        return Ok(());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // 获取写锁后重新检查全部旧表，等待者不能用空旧集合覆盖已迁移数据。
    let old = has_column(&transaction, "providers", "payload_json")?;
    let legacy_keys = has_column(&transaction, "provider_keys", "payload_json")?;
    let mut extra = false;
    for table in OLD_TABLES {
        extra |= table_exists(&transaction, table)?;
    }
    if !old && !legacy_keys && !extra && table_exists(&transaction, "providers")? {
        if !has_column(&transaction, "providers", "source_provider_id")? {
            transaction.execute(
                "ALTER TABLE providers ADD COLUMN source_provider_id TEXT",
                [],
            )?;
        }
        super::keys::migrate(&transaction)?;
        transaction.commit()?;
        return Ok(());
    }
    if old {
        transaction.execute_batch("ALTER TABLE providers RENAME TO providers_legacy;")?;
    }
    if legacy_keys {
        transaction.execute_batch("ALTER TABLE provider_keys RENAME TO provider_keys_legacy;")?;
    }
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);",
    )?;
    schema::create(&transaction)?;
    let sources = [
        ("providers_legacy", "api"),
        ("claude_desktop_providers", "desktop"),
        ("codex_accounts", "account"),
    ];
    let mut expected = HashSet::new();
    for (table, scope) in sources {
        for (index, item) in legacy_items(&transaction, table)?.iter().enumerate() {
            let id = upsert_record(&transaction, item, scope, index as i64)?;
            if !expected.insert(id) {
                return Err(ManagerError::System(
                    "供应商或账号标识冲突，迁移已回滚".into(),
                ));
            }
        }
    }
    let keys = legacy_map(&transaction, "provider_keys_legacy", "provider_id")?;
    for id in keys.keys() {
        let owner = id.strip_prefix("google-oauth:").unwrap_or(id);
        if !transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1)",
            [owner],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(ManagerError::System(format!(
                "密钥 {id} 缺少对应供应商，保留旧表等待修复"
            )));
        }
    }
    if table_exists(&transaction, "provider_keys_legacy")? {
        set_keys(&transaction, &keys)?;
    }
    if table_exists(&transaction, "provider_models")? {
        set_models(
            &transaction,
            &legacy_items(&transaction, "provider_models")?,
        )?;
    }
    if table_exists(&transaction, "provider_runtime_profiles")? {
        set_profiles(
            &transaction,
            &legacy_items(&transaction, "provider_runtime_profiles")?,
        )?;
    }
    if table_exists(&transaction, "provider_runtime_state")? {
        set_runtime_state(
            &transaction,
            &legacy_map(&transaction, "provider_runtime_state", "cli")?,
        )?;
    }
    if table_exists(&transaction, "provider_instances")? {
        set_instances(
            &transaction,
            &legacy_items(&transaction, "provider_instances")?,
        )?;
    }
    if table_exists(&transaction, "claude_desktop_settings")? {
        set_desktop_settings(
            &transaction,
            &legacy_map(&transaction, "claude_desktop_settings", "setting_key")?,
        )?;
    }
    if table_exists(&transaction, "codex_active_account")? {
        let active = transaction
            .query_row(
                "SELECT account_id FROM codex_active_account WHERE singleton_id=1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_default();
        if !active.is_empty()
            && transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1)",
                [format!("account:{active}")],
                |row| row.get::<_, bool>(0),
            )?
        {
            set_active_account(&transaction, &active)?;
        }
    }
    for id in expected {
        if !transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1)",
            [id],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(ManagerError::System("供应商迁移数量校验失败".into()));
        }
    }
    for table in OLD_TABLES
        .iter()
        .copied()
        .chain(std::iter::once("providers_legacy"))
    {
        transaction.execute_batch(&format!("DROP TABLE IF EXISTS {table};"))?;
    }
    transaction.commit()?;
    Ok(())
}

pub(super) fn migrate_legacy_json(
    paths: &AppPaths,
    connection: &mut Connection,
) -> Result<(), ManagerError> {
    let files = [
        &paths.storage_files.providers,
        &paths.storage_files.runtime_models,
        &paths.storage_files.runtime_profiles,
        &paths.storage_files.runtime_provider_keys,
        &paths.storage_files.runtime_provider_state,
        &paths.storage_files.codex_provider_instances,
        &paths.storage_files.codex_accounts,
        &paths.storage_files.codex_active_account_id,
    ];
    if !files.iter().any(|file| Path::new(file).exists()) {
        return Ok(());
    }
    let providers = read_array_file(Path::new(files[0]))?;
    let models = read_array_file(Path::new(files[1]))?;
    let profiles = read_array_file(Path::new(files[2]))?;
    let keys = read_object_file(Path::new(files[3]))?;
    let states = read_object_file(Path::new(files[4]))?;
    let instances = read_array_file(Path::new(files[5]))?;
    let accounts = read_array_file(Path::new(files[6]))?;
    let active = read_json_file(Path::new(files[7]))?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if Path::new(files[0]).exists() {
        for (index, item) in providers.iter().enumerate() {
            upsert_record(&transaction, item, "api", index as i64)?;
        }
    }
    if Path::new(files[6]).exists() {
        for (index, item) in accounts.iter().enumerate() {
            upsert_record(&transaction, item, "account", index as i64)?;
        }
    }
    if Path::new(files[1]).exists() {
        set_models(&transaction, &models)?;
    }
    if Path::new(files[2]).exists() {
        set_profiles(&transaction, &profiles)?;
    }
    if Path::new(files[3]).exists() {
        set_keys(&transaction, &keys)?;
    }
    if Path::new(files[4]).exists() {
        set_runtime_state(&transaction, &states)?;
    }
    if Path::new(files[5]).exists() {
        set_instances(&transaction, &instances)?;
    }
    if let Some(active) = active
        .as_ref()
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        set_active_account(&transaction, active)?;
    }
    transaction.commit()?;
    for file in files {
        remove_file_family(Path::new(file))?;
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

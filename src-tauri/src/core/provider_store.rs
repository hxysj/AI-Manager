use crate::core::{database, error::ManagerError, paths::AppPaths};
use rusqlite::{
    params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension,
    TransactionBehavior,
};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

mod keys;
mod migration;
mod schema;
pub(crate) use keys::usage_provider_id;
pub(crate) use migration::migrate_connection;
use migration::migrate_legacy_json;
pub(crate) use schema::{item_from_row, local_default, LOCAL_COLUMNS};
const API_ROWS: &str = "cli<>'claude-desktop' AND type<>'codex-account'";
const OLD_TABLES: &[&str] = &[
    "claude_desktop_providers",
    "claude_desktop_settings",
    "codex_accounts",
    "codex_active_account",
    "provider_instances",
    "provider_keys_legacy",
    "provider_models",
    "provider_runtime_profiles",
    "provider_runtime_state",
    "provider_schema_migrations",
];

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = database::open(paths)?;
    migrate_connection(&mut connection)?;
    migrate_legacy_json(paths, &mut connection)
}

fn text(item: &Value, field: &str) -> String {
    item.get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn rows(connection: &Connection, filter: &str) -> Result<Vec<Value>, ManagerError> {
    let mut statement = connection.prepare(&format!(
        "SELECT * FROM providers WHERE {filter} ORDER BY sort_order,id"
    ))?;
    let result = statement
        .query_map([], item_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}

pub fn read_providers(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    rows(&database::open(paths)?, API_ROWS)
}

pub fn read_desktop_providers(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    rows(&database::open(paths)?, "cli='claude-desktop'")
}

pub fn read_codex_accounts(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    rows(&database::open(paths)?, "type='codex-account'")
}

fn canonical_id(
    connection: &Connection,
    item: &Value,
    scope: &str,
) -> Result<String, ManagerError> {
    let id = text(item, "id");
    if id.is_empty() {
        return Err(ManagerError::System(
            "供应商或账号缺少 id，保留原始数据".into(),
        ));
    }
    let legacy = match scope {
        "account" => format!("account:{id}"),
        "desktop" => format!("claude-desktop:{id}"),
        _ => return Ok(id),
    };
    // 旧记录保留原主键；新记录的 UUID 直接入库，不再拼接类型前缀。
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1)",
        [&legacy],
        |row| row.get::<_, bool>(0),
    )?;
    let conflicts = connection.query_row("SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1 AND ((?2='account' AND type<>'codex-account') OR (?2='desktop' AND cli<>'claude-desktop')))", params![id, scope], |row| row.get::<_, bool>(0))?;
    Ok(
        if exists || conflicts || uuid::Uuid::parse_str(&id).is_err() {
            legacy
        } else {
            id
        },
    )
}

// 每次只更新这一类记录的业务列，本机激活、密钥、模型和同步信息分别维护。
fn upsert_record(
    connection: &Connection,
    source: &Value,
    scope: &str,
    order: i64,
) -> Result<String, ManagerError> {
    let id = canonical_id(connection, source, scope)?;
    let mut item = source.clone();
    let cli = match scope {
        "account" => "codex".to_string(),
        "desktop" => "claude-desktop".to_string(),
        _ => text(&item, "cli"),
    };
    let kind = if scope == "account" {
        "codex-account".to_string()
    } else if scope == "desktop" {
        let format = text(&item, "apiFormat");
        if text(&item, "mode") == "official" {
            "official".into()
        } else if format.is_empty() {
            "anthropic".into()
        } else {
            format
        }
    } else {
        source
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("custom")
            .to_string()
    };
    if scope == "account" {
        item["accountKind"] = source.get("type").cloned().unwrap_or(json!("codex"));
        if item.get("accountId").is_none() {
            if let Some(value) = source.get("account_id") {
                item["accountId"] = value.clone();
            }
        }
        // 令牌沿用账号读取逻辑的优先级，避免旧的顶层别名覆盖有效凭据。
        for (field, alias) in [
            ("access_token", "accessToken"),
            ("refresh_token", "refreshToken"),
            ("id_token", "idToken"),
        ] {
            if let Some(value) = source["auth"]
                .get(alias)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                item[field] = json!(value);
            }
        }
        for (field, alias) in [
            ("access_token", "accessToken"),
            ("refresh_token", "refreshToken"),
            ("id_token", "idToken"),
            ("last_refresh", "last_refresh"),
            ("expired", "expired"),
            ("token_generation", "token_generation"),
            ("token_updated_at", "token_updated_at"),
        ] {
            if item.get(field).is_none() || item[field].is_null() {
                if let Some(value) = source["auth"]
                    .get(alias)
                    .or_else(|| source["auth"].get(field))
                {
                    item[field] = value.clone();
                }
            }
        }
    }
    let previous: Option<(String, String)> = connection
        .query_row("SELECT cli,type FROM providers WHERE id=?1", [&id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?;
    if let Some((previous_cli, previous_kind)) = previous {
        if (previous_cli == "claude-desktop") != (scope == "desktop")
            || (previous_kind == "codex-account") != (scope == "account")
        {
            return Err(ManagerError::System(format!(
                "供应商标识 {id} 与已有账号冲突，迁移已回滚"
            )));
        }
        if previous_cli != cli || previous_kind != kind {
            connection.execute("UPDATE providers SET is_active=0,profile_id='',synced_hash='',synced_at=0,runtime_path='',sync_status='NO_ACTIVE' WHERE id=?1",[&id])?;
        }
    }
    let enabled = if scope == "account" {
        source["disabled"] != true
    } else {
        source["enabled"] != false
    };
    let mut columns = vec!["id", "cli", "type", "sort_order", "enabled"];
    let mut values = vec![
        SqlValue::Text(id.clone()),
        SqlValue::Text(cli),
        SqlValue::Text(kind),
        SqlValue::Integer(order),
        SqlValue::Integer(i64::from(enabled)),
    ];
    for (column, path, kind) in schema::FIELDS {
        columns.push(column);
        values.push(schema::sql_value(item.pointer(path), kind)?);
    }
    let changes = columns
        .iter()
        .skip(1)
        .map(|column| format!("{column}=excluded.{column}"))
        .collect::<Vec<_>>()
        .join(",");
    let different = columns
        .iter()
        .skip(1)
        .map(|column| format!("providers.{column} IS NOT excluded.{column}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    connection.execute(&format!("INSERT INTO providers({}) VALUES({}) ON CONFLICT(id) DO UPDATE SET {changes} WHERE {different}",
        columns.join(","),vec!["?";columns.len()].join(",")),params_from_iter(values))?;
    Ok(id)
}

pub(crate) fn upsert_provider(connection: &Connection, item: &Value) -> Result<(), ManagerError> {
    let order = connection
        .query_row(
            "SELECT sort_order FROM providers WHERE id=?1",
            [text(item, "id")],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(connection.query_row(
            "SELECT COALESCE(MAX(sort_order),-1)+1 FROM providers",
            [],
            |row| row.get(0),
        )?);
    upsert_record(connection, item, "api", order)?;
    Ok(())
}

pub(crate) fn update_provider(paths: &AppPaths, item: &Value) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1)",
        [text(item, "id")],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(ManagerError::System("供应商或账号已删除".into()));
    }
    upsert_provider(&transaction, item)?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn delete_provider(paths: &AppPaths, provider_id: &str) -> Result<(), ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    // 模型和运行配置随供应商行删除，触发器同时清除凭据，避免重写其他供应商的 Key。
    connection.execute(
        &format!("DELETE FROM providers WHERE id=?1 AND {API_ROWS}"),
        [provider_id],
    )?;
    Ok(())
}

fn replace_records(
    connection: &Connection,
    items: &[Value],
    scope: &str,
) -> Result<(), ManagerError> {
    let filter = match scope {
        "account" => "type='codex-account'",
        "desktop" => "cli='claude-desktop'",
        _ => API_ROWS,
    };
    let retained = items
        .iter()
        .map(|item| canonical_id(connection, item, scope))
        .collect::<Result<HashSet<_>, _>>()?;
    if retained.len() != items.len() {
        return Err(ManagerError::System("供应商或账号 id 重复，未保存".into()));
    }
    let existing = connection
        .prepare(&format!("SELECT id FROM providers WHERE {filter}"))?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in existing {
        if !retained.contains(&id) {
            connection.execute("DELETE FROM providers WHERE id=?1", [id])?;
        }
    }
    for (order, item) in items.iter().enumerate() {
        upsert_record(connection, item, scope, order as i64)?;
    }
    Ok(())
}

pub fn write_codex_accounts(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    replace_records(&transaction, items, "account")?;
    transaction.commit()?;
    Ok(())
}

pub fn read_active_codex_account_id(paths: &AppPaths) -> Result<String, ManagerError> {
    initialize(paths)?;
    Ok(database::open(paths)?
        .query_row(
            "SELECT CASE WHEN id LIKE 'account:%' THEN substr(id,9) ELSE id END FROM providers WHERE type='codex-account' AND is_active=1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default())
}

fn set_active_account(connection: &Connection, id: &str) -> Result<(), ManagerError> {
    if id.is_empty() {
        connection.execute(
            "UPDATE providers SET is_active=0 WHERE type='codex-account' AND is_active=1",
            [],
        )?;
    } else {
        let canonical = canonical_id(connection, &json!({"id":id}), "account")?;
        if !connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?1 AND type='codex-account')",
            [&canonical],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(ManagerError::System("Codex 官方账号不存在".into()));
        }
        connection.execute(
            "UPDATE providers SET is_active=0 WHERE cli='codex' AND is_active=1",
            [],
        )?;
        connection.execute("UPDATE providers SET is_active=1 WHERE id=?1", [canonical])?;
    }
    Ok(())
}

pub fn write_active_codex_account_id(paths: &AppPaths, id: &str) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    set_active_account(&transaction, id)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_models(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut result = Vec::new();
    for raw in connection
        .prepare(&format!(
            "SELECT models FROM providers WHERE {API_ROWS} ORDER BY sort_order,id"
        ))?
        .query_map([], |row| row.get::<_, String>(0))?
    {
        result.extend(serde_json::from_str::<Vec<Value>>(&raw?)?);
    }
    Ok(result)
}

pub(crate) fn set_provider_models(
    connection: &Connection,
    id: &str,
    models: &[Value],
) -> Result<(), ManagerError> {
    let encoded = serde_json::to_string(models)?;
    connection.execute(
        "UPDATE providers SET models=?2 WHERE id=?1 AND models<>?2",
        params![id, encoded],
    )?;
    Ok(())
}

fn set_models(connection: &Connection, items: &[Value]) -> Result<(), ManagerError> {
    let ids = connection
        .prepare(&format!("SELECT id FROM providers WHERE {API_ROWS}"))?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut grouped: HashMap<String, Vec<Value>> = HashMap::new();
    for item in items {
        grouped
            .entry(text(item, "providerId"))
            .or_default()
            .push(item.clone());
    }
    for id in ids {
        set_provider_models(
            connection,
            &id,
            grouped.get(&id).map(Vec::as_slice).unwrap_or(&[]),
        )?;
    }
    Ok(())
}

pub fn write_models(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    set_models(&transaction, items)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_profiles(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut statement=connection.prepare(&format!("SELECT id,cli,profile_id,selected_model,profile_base_url,profile_proxy,profile_env,profile_updated_at FROM providers WHERE {API_ROWS} AND is_active=1 AND profile_id<>'' ORDER BY cli"))?;
    let mut result = Vec::new();
    let mut query = statement.query([])?;
    while let Some(row) = query.next()? {
        let env: Option<String> = row.get(6)?;
        result.push(json!({"providerId":row.get::<_,String>(0)?,"cli":row.get::<_,String>(1)?,"id":row.get::<_,String>(2)?,
            "model":row.get::<_,Option<String>>(3)?.unwrap_or_default(),"baseUrl":row.get::<_,Option<String>>(4)?,
            "proxy":row.get::<_,Option<String>>(5)?,"env":env.map(|raw|serde_json::from_str::<Value>(&raw)).transpose()?.unwrap_or(json!({})),
            "updatedAt":row.get::<_,Option<i64>>(7)?.unwrap_or(0)}));
    }
    Ok(result)
}

fn set_profiles(connection: &Connection, items: &[Value]) -> Result<(), ManagerError> {
    let keep = items
        .iter()
        .map(|item| (text(item, "providerId"), text(item, "cli")))
        .collect::<HashSet<_>>();
    let current = connection
        .prepare(&format!(
            "SELECT id,cli FROM providers WHERE {API_ROWS} AND is_active=1"
        ))?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, cli) in current {
        if !keep.contains(&(id.clone(), cli)) {
            connection.execute(
                "UPDATE providers SET is_active=0,profile_id='' WHERE id=?1",
                [id],
            )?;
        }
    }
    for item in items {
        let id = text(item, "providerId");
        let cli = text(item, "cli");
        let scope = connection
            .query_row(
                &format!("SELECT cli FROM providers WHERE id=?1 AND {API_ROWS}"),
                [&id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(scope) = scope else {
            continue;
        };
        if !scope.is_empty() && scope != cli {
            continue;
        }
        connection.execute(
            "UPDATE providers SET is_active=0 WHERE cli=?1 AND id<>?2 AND is_active=1",
            params![cli, id],
        )?;
        let values = vec![
            SqlValue::Text(cli),
            SqlValue::Text(
                item.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
            schema::sql_value(item.get("model"), "TEXT")?,
            schema::sql_value(item.get("baseUrl"), "TEXT")?,
            schema::sql_value(item.get("proxy"), "TEXT")?,
            schema::sql_value(item.get("env"), "JSON")?,
            schema::sql_value(item.get("updatedAt"), "INTEGER")?,
            SqlValue::Text(id),
        ];
        connection.execute("UPDATE providers SET cli=?1,is_active=1,profile_id=?2,selected_model=?3,profile_base_url=?4,profile_proxy=?5,profile_env=?6,profile_updated_at=?7
            WHERE id=?8 AND (cli IS NOT ?1 OR is_active<>1 OR profile_id IS NOT ?2 OR selected_model IS NOT ?3 OR profile_base_url IS NOT ?4 OR profile_proxy IS NOT ?5 OR profile_env IS NOT ?6 OR profile_updated_at IS NOT ?7)",params_from_iter(values))?;
    }
    Ok(())
}

pub fn write_profiles(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    set_profiles(&transaction, items)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_runtime_state(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut states = Map::new();
    for cli in ["claude", "codex", "gemini", "opencode"] {
        states.insert(cli.into(),json!({"activeProviderId":"","runtimeHash":"","lastSyncAt":0,"runtimePath":"","status":"NO_ACTIVE"}));
    }
    let mut statement=connection.prepare(&format!("SELECT cli,id,synced_hash,synced_at,runtime_path,sync_status FROM providers WHERE {API_ROWS} AND is_active=1"))?;
    for row in statement.query_map([],|row|Ok((row.get::<_,String>(0)?,json!({"activeProviderId":row.get::<_,String>(1)?,
        "runtimeHash":row.get::<_,String>(2)?,"lastSyncAt":row.get::<_,i64>(3)?,"runtimePath":row.get::<_,String>(4)?,"status":row.get::<_,String>(5)?}))))? {
        let (cli,state)=row?; states.insert(cli,state);
    }
    Ok(states)
}

fn set_runtime_state(
    connection: &Connection,
    states: &Map<String, Value>,
) -> Result<(), ManagerError> {
    for (cli, state) in states {
        let id = text(state, "activeProviderId");
        if id.is_empty() {
            continue;
        }
        let hash = text(state, "runtimeHash");
        let time = state["lastSyncAt"].as_i64().unwrap_or(0);
        let path = text(state, "runtimePath");
        let status = text(state, "status");
        connection.execute("UPDATE providers SET synced_hash=?3,synced_at=?4,runtime_path=?5,sync_status=?6
          WHERE id=?1 AND cli=?2 AND (synced_hash<>?3 OR synced_at<>?4 OR runtime_path<>?5 OR sync_status<>?6)",params![id,cli,hash,time,path,status])?;
    }
    Ok(())
}

pub fn write_runtime_state(
    paths: &AppPaths,
    states: &Map<String, Value>,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    set_runtime_state(&transaction, states)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_keys(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut result = Map::new();
    let mut statement=connection.prepare("SELECT CASE WHEN p.cli='claude-desktop' AND p.id NOT LIKE 'claude-desktop:%' THEN 'claude-desktop:'||p.id ELSE p.id END,k.id,k.name,k.note,k.value,k.is_active FROM provider_keys k JOIN providers p ON p.id=k.provider_id WHERE k.is_current=1 ORDER BY k.provider_id,k.sort_order,k.id")?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, bool>(5)?,
        ))
    })? {
        let (provider, id, name, note, value, active) = row?;
        let entry = result
            .entry(provider)
            .or_insert_with(|| json!({"keys":[],"activeKeyId":""}));
        if active {
            entry["activeKeyId"] = json!(id);
        }
        entry["keys"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"name":name,"note":note,"value":value}));
    }
    for row in connection
        .prepare("SELECT id,oauth_credentials FROM providers WHERE oauth_credentials<>''")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
    {
        let (id, credentials) = row?;
        result.insert(format!("google-oauth:{id}"), json!(credentials));
    }
    Ok(result)
}

pub(crate) fn set_key(
    connection: &Connection,
    provider_id: &str,
    value: Option<&Value>,
) -> Result<(), ManagerError> {
    if let Some(id) = provider_id.strip_prefix("google-oauth:") {
        let credentials = value
            .and_then(Value::as_str)
            .or_else(|| {
                let value = value?;
                let records = value["keys"].as_array()?;
                records
                    .iter()
                    .find(|key| key["id"] == value["activeKeyId"])
                    .or_else(|| records.first())?
                    .get("value")?
                    .as_str()
            })
            .unwrap_or("");
        connection.execute(
            "UPDATE providers SET oauth_credentials=?2 WHERE id=?1 AND oauth_credentials<>?2",
            params![id, credentials],
        )?;
        return Ok(());
    }
    let resolved = if let Some(id) = provider_id.strip_prefix("claude-desktop:") {
        canonical_id(connection, &json!({"id":id}), "desktop")?
    } else {
        provider_id.to_string()
    };
    let provider_id = resolved.as_str();
    let records = if let Some(raw) = value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        vec![json!({"id":"default","name":"默认 Key","note":"","value":raw})]
    } else {
        value
            .and_then(|value| value["keys"].as_array())
            .cloned()
            .unwrap_or_default()
    };
    let mut retained = HashSet::new();
    for record in &records {
        if !record.is_object()
            || text(record, "id").is_empty()
            || text(record, "value").is_empty()
            || !retained.insert(text(record, "id"))
        {
            return Err(ManagerError::System(
                "API Key 缺少 id、加密内容或 id 重复".into(),
            ));
        }
    }
    let requested = value
        .map(|value| text(value, "activeKeyId"))
        .unwrap_or_default();
    let active = if retained.contains(&requested) {
        requested
    } else {
        records
            .first()
            .map(|record| text(record, "id"))
            .unwrap_or_default()
    };
    let existing = connection
        .prepare("SELECT id FROM provider_keys WHERE provider_id=?1 AND is_current=1")?
        .query_map([provider_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in existing {
        if !retained.contains(&id) {
            keys::archive(connection, provider_id, &id)?;
        }
    }
    // 先取消旧选择，避免切换时触发同一供应商只能有一个激活 Key 的约束。
    connection.execute(
        "UPDATE provider_keys SET is_active=0 WHERE provider_id=?1 AND id<>?2 AND is_active=1",
        params![provider_id, active],
    )?;
    for (order, record) in records.iter().enumerate() {
        let id = text(record, "id");
        let encrypted = text(record, "value");
        let hash = keys::credential_hash(&encrypted);
        let current_hash: Option<String> = connection.query_row(
            "SELECT key_hash FROM provider_keys WHERE provider_id=?1 AND id=?2 AND is_current=1",
            params![provider_id,id],|row|row.get(0)).optional()?;
        if current_hash
            .as_ref()
            .is_some_and(|current| current != &hash)
        {
            keys::archive(connection, provider_id, &id)?;
        }
        connection.execute("INSERT INTO provider_keys(provider_id,id,key_hash,name,note,value,is_current,is_active,sort_order)
          VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8)
          ON CONFLICT(provider_id,id,key_hash) DO UPDATE SET name=excluded.name,note=excluded.note,value=excluded.value,
          is_current=1,is_active=excluded.is_active,sort_order=excluded.sort_order
          WHERE name IS NOT excluded.name OR note IS NOT excluded.note OR value IS NOT excluded.value
          OR is_current<>1 OR is_active IS NOT excluded.is_active OR sort_order IS NOT excluded.sort_order",
          params![provider_id,id,hash,text(record,"name"),text(record,"note"),encrypted,id==active,order as i64])?;
    }
    Ok(())
}

fn set_keys(connection: &Connection, keys: &Map<String, Value>) -> Result<(), ManagerError> {
    let ids = connection
        .prepare("SELECT CASE WHEN cli='claude-desktop' AND id NOT LIKE 'claude-desktop:%' THEN 'claude-desktop:'||id ELSE id END FROM providers")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        set_key(connection, &id, keys.get(&id))?;
        let oauth_id = format!("google-oauth:{id}");
        set_key(connection, &oauth_id, keys.get(&oauth_id))?;
    }
    Ok(())
}

pub fn write_keys(paths: &AppPaths, keys: &Map<String, Value>) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    set_keys(&transaction, keys)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_desktop_settings(paths: &AppPaths) -> Result<Map<String, Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut settings = Map::new();
    for row in connection
        .prepare("SELECT substr(key,9),value FROM app_metadata WHERE key LIKE 'desktop:%'")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
    {
        let (key, raw) = row?;
        settings.insert(key, serde_json::from_str(&raw)?);
    }
    let active = connection
        .query_row(
            "SELECT CASE WHEN id LIKE 'claude-desktop:%' THEN substr(id,16) ELSE id END FROM providers WHERE cli='claude-desktop' AND is_active=1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_default();
    settings.insert("currentProviderId".into(), json!(active));
    Ok(settings)
}

fn set_desktop_settings(
    connection: &Connection,
    settings: &Map<String, Value>,
) -> Result<(), ManagerError> {
    let old = connection
        .prepare("SELECT key FROM app_metadata WHERE key LIKE 'desktop:%'")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for key in old {
        if !settings.contains_key(&key[8..]) {
            connection.execute("DELETE FROM app_metadata WHERE key=?1", [key])?;
        }
    }
    for (key, value) in settings {
        if key == "currentProviderId" {
            continue;
        }
        connection.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value<>excluded.value",
            params![format!("desktop:{key}"),value.to_string()])?;
    }
    let requested = settings
        .get("currentProviderId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let id = if requested.is_empty() {
        String::new()
    } else {
        canonical_id(connection, &json!({"id":requested}), "desktop")?
    };
    connection.execute(
        "UPDATE providers SET is_active=0 WHERE cli='claude-desktop' AND is_active=1 AND id<>?1",
        [&id],
    )?;
    if !requested.is_empty() {
        connection.execute(
            "UPDATE providers SET is_active=1 WHERE id=?1 AND cli='claude-desktop' AND is_active=0",
            [id],
        )?;
    }
    Ok(())
}

pub fn write_desktop_bundle(
    paths: &AppPaths,
    providers: &[Value],
    settings: &Map<String, Value>,
    keys: &Map<String, Value>,
) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    replace_records(&transaction, providers, "desktop")?;
    set_desktop_settings(&transaction, settings)?;
    set_keys(&transaction, keys)?;
    transaction.commit()?;
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
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    replace_records(&transaction, providers, "api")?;
    set_models(&transaction, models)?;
    set_profiles(&transaction, profiles)?;
    set_keys(&transaction, keys)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_instances(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    initialize(paths)?;
    let connection = database::open(paths)?;
    let mut statement=connection.prepare("SELECT CASE WHEN type='codex-account' AND id NOT LIKE 'account:%' THEN 'account:'||id ELSE id END,name,type,instance_profile_dir,instance_sessions_path,instance_updated_at FROM providers WHERE instance_updated_at>0 ORDER BY instance_updated_at DESC")?;
    let result=statement.query_map([],|row|Ok(json!({"id":row.get::<_,String>(0)?,"providerId":row.get::<_,String>(0)?,
        "providerName":row.get::<_,Option<String>>(1)?.unwrap_or_default(),"providerType":row.get::<_,String>(2)?,"profileDir":row.get::<_,String>(3)?,
        "sessionsPath":row.get::<_,String>(4)?,"updatedAt":row.get::<_,i64>(5)?})))?.collect::<Result<Vec<_>,_>>()?;
    Ok(result)
}

fn set_instances(connection: &Connection, items: &[Value]) -> Result<(), ManagerError> {
    for item in items {
        let id = item
            .get("providerId")
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let resolved = if let Some(id) = id.strip_prefix("account:") {
            canonical_id(connection, &json!({"id":id}), "account")?
        } else {
            id.to_string()
        };
        connection.execute("UPDATE providers SET instance_profile_dir=?2,instance_sessions_path=?3,instance_updated_at=?4 WHERE id=?1 AND (instance_profile_dir<>?2 OR instance_sessions_path<>?3 OR instance_updated_at<>?4)",
            params![resolved,text(item,"profileDir"),text(item,"sessionsPath"),item["updatedAt"].as_i64().unwrap_or(1)])?;
    }
    Ok(())
}

pub fn write_instances(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    initialize(paths)?;
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    set_instances(&transaction, items)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests;

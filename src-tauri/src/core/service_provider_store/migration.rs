use super::*;
use std::collections::BTreeMap;

pub(super) fn add_account_columns(c: &Connection) -> Result<(), ManagerError> {
    if !table_exists(c, "service_provider_configs")? {
        return Ok(());
    }
    let columns = query(c, "SELECT name FROM pragma_table_info('service_provider_configs')", &[])?;
    // 沿用唯一建表定义，已有配置只增加字段，不改变主键及模型、Usage 关联。
    for definition in SCHEMA.lines().map(str::trim) {
        let name = definition.split_whitespace().next().unwrap_or("");
        if ACCOUNT_LOCAL_COLUMNS.contains(&name)
            && !columns.iter().any(|column| text(column, "name") == name)
        {
            c.execute_batch(&format!(
                "ALTER TABLE service_provider_configs ADD COLUMN {}",
                definition.trim_end_matches(',')
            ))?;
        }
    }
    Ok(())
}

pub(super) fn merge_account_storage(c: &Connection) -> Result<(), ManagerError> {
    if table_exists(c, "service_provider_accounts")? {
        for row in query(c, "SELECT * FROM service_provider_accounts", &[])? {
            let mut credentials = json!({});
            for (column, key) in [
                ("access_ciphertext", "access_token"),
                ("refresh_ciphertext", "refresh_token"),
                ("id_ciphertext", "id_token"),
            ] {
                let ciphertext = text(&row, column);
                credentials[key] = json!(if ciphertext.is_empty() {
                    String::new()
                } else {
                    decrypt_provider_key(ciphertext)
                        .map_err(|_| fail("官方账号凭据无法解密，已保留原账号表并回滚升级"))?
                });
            }
            let changed = c.execute(
                "UPDATE service_provider_configs SET account_id=?2,account_email=?3,
                credentials_ciphertext=?4,token_expires_at=?5,credential_generation=?6,
                refresh_status=?7,quota_json=?8,account_updated_at=?9
                WHERE id=?1 AND kind='codex_account'",
                params![row["provider_id"].as_str(), row["external_account_id"].as_str(),
                    row["email"].as_str(), encrypt_provider_key(&credentials.to_string())?,
                    row["expires_at"].as_i64(), row["token_generation"].as_i64(),
                    row["refresh_status"].as_str(), row["quota_json"].as_str(), row["updated_at"].as_i64()],
            )?;
            if changed != 1 {
                return Err(fail("官方账号与服务商类型不一致，已回滚升级"));
            }
        }
    }
    if table_exists(c, "service_google_accounts")? {
        for row in query(c, "SELECT * FROM service_google_accounts", &[])? {
            let changed = c.execute(
                "UPDATE service_provider_configs SET account_id=?2,account_email=?3,
                credentials_ciphertext=?4,token_expires_at=?5,credential_generation=?6,
                account_metadata_json=?7,google_local_key_ciphertext=?8,google_gateway_port=?9,
                account_updated_at=?10 WHERE id=?1 AND kind='google_account'",
                params![row["provider_id"].as_str(), row["external_subject"].as_str(),
                    row["email"].as_str(), row["oauth_ciphertext"].as_str(), row["expires_at"].as_i64().and_then(|seconds| seconds.checked_mul(1000)),
                    row["token_generation"].as_i64(), row["metadata_json"].as_str(),
                    row["local_key_ciphertext"].as_str(), row["gateway_port"].as_i64(), row["updated_at"].as_i64()],
            )?;
            if changed != 1 {
                return Err(fail("Google 账号与服务商类型不一致，已回滚升级"));
            }
        }
    }
    // 由调用方同一事务提交；任意迁入失败时，原字段和两张账号表均保持原样。
    c.execute_batch("DROP TABLE IF EXISTS service_provider_accounts;
        DROP TABLE IF EXISTS service_google_accounts;
        CREATE UNIQUE INDEX IF NOT EXISTS service_config_google_subject ON service_provider_configs(account_id)
            WHERE kind='google_account' AND account_id<>'';")?;
    Ok(())
}

pub(super) fn merge_operation_storage(c: &Connection) -> Result<(), ManagerError> {
    // 原 ID、时间和证据全部保留；写入、改外键及移除旧表由初始化事务统一提交。
    for (table, sql) in [
        ("service_provider_migrations", "INSERT INTO service_operations
            (id,operation_type,source_schema,status,created_at,completed_at,error_message)
            SELECT id,'migration',source_schema,status,created_at,completed_at,error_message FROM service_provider_migrations"),
        ("service_switch_operations", "INSERT INTO service_operations
            (id,operation_type,from_backend,to_backend,generation,status,created_at,completed_at,error_message)
            SELECT id,'switch',from_backend,to_backend,generation,stage,created_at,completed_at,error_message FROM service_switch_operations"),
        ("service_compatibility_tests", "INSERT INTO service_operations
            (id,operation_type,provider_id,agent_type,model_id,provider_revision,agent_version,adapter_version,
                client_protocol,upstream_protocol,capability,status,evidence_json,created_at,completed_at)
            SELECT id,'compatibility',provider_id,agent_type,model_id,provider_revision,agent_version,adapter_version,
                client_protocol,upstream_protocol,capability,status,evidence_json,tested_at,tested_at FROM service_compatibility_tests"),
    ] {
        if table_exists(c, table)? {
            c.execute_batch(sql)?;
        }
    }
    if query(c, "PRAGMA foreign_key_list(service_provider_migration_items)", &[])?
        .iter().any(|row| text(row, "table") == "service_provider_migrations")
    {
        let definition = SCHEMA.split("CREATE TABLE IF NOT EXISTS service_provider_migration_items ")
            .nth(1).unwrap().split("CREATE UNIQUE INDEX").next().unwrap();
        c.execute_batch(&format!("CREATE TABLE service_provider_migration_items_v5 {definition}"))?;
        c.execute_batch("INSERT INTO service_provider_migration_items_v5
            (id,migration_id,source_table,source_id,source_fingerprint,target_id,status,reason)
            SELECT id,migration_id,source_table,source_id,source_fingerprint,target_id,status,reason
            FROM service_provider_migration_items;
            DROP TABLE service_provider_migration_items;
            ALTER TABLE service_provider_migration_items_v5 RENAME TO service_provider_migration_items;")?;
        c.execute_batch(SCHEMA.lines()
            .find(|line| line.starts_with("CREATE UNIQUE INDEX IF NOT EXISTS service_migration_source ")).unwrap())?;
    }
    c.execute_batch("DROP TABLE IF EXISTS service_provider_migrations;
        DROP TABLE IF EXISTS service_switch_operations;
        DROP TABLE IF EXISTS service_compatibility_tests;")?;
    Ok(())
}

// 在同一事务中读取旧表、写入新表；不调用旧初始化、迁移或保存接口。
pub fn migrate(paths: &AppPaths) -> Result<Value, ManagerError> {
    initialize(paths)?;
    let c = database::open(paths)?;
    let migration = id();
    c.execute("INSERT INTO service_operations(id,operation_type,source_schema,status,created_at) VALUES(?1,'migration','providers-v1','running',?2)",params![migration,now()])?;
    let result = copy_configuration(paths, &migration);
    if let Err(error) = &result {
        c.execute("UPDATE service_operations SET status='failed',error_message=?2,completed_at=?3 WHERE id=?1 AND operation_type='migration'",params![migration,error.to_string(),now()])?;
    }
    result
}

fn copy_configuration(paths: &AppPaths, migration: &str) -> Result<Value, ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if table_exists(&tx, "providers")? {
        let columns = query(&tx, "SELECT name FROM pragma_table_info('providers')", &[])?;
        if !["cli", "type", "model_catalog", "models", "base_url"]
            .iter()
            .all(|key| columns.iter().any(|r| text(r, "name") == *key))
        {
            return Err(fail(
                "旧 Provider 表不属于当前已支持的结构，请先在旧模式完成原有升级",
            ));
        }
        let rows = query(&tx, "SELECT * FROM providers ORDER BY sort_order,id", &[])?;
        for source in rows {
            let source_id = text(&source, "id");
            let existing:Option<String>=tx.query_row("SELECT target_id FROM service_provider_migration_items WHERE source_table='providers'
                AND source_id=?1 AND status IN ('copied','unreadable')",[source_id],|r|r.get(0)).optional()?;
            if let Some(target) = existing {
                item(
                    &tx,
                    &migration,
                    "providers",
                    source_id,
                    &source,
                    Some(&target),
                    "already_copied",
                    "保留新模式中现有配置，未覆盖",
                )?;
                continue;
            }
            let kind = if text(&source, "type") == "google-account" {
                "google_account"
            } else if text(&source, "type") == "codex-account" {
                "codex_account"
            } else {
                "api"
            };
            if text(&source, "cli") != "codex"
                || ["anthropic", "gemini"].contains(&text(&source, "type"))
                || (kind == "codex_account"
                    && source["account_kind"]
                        .as_str()
                        .is_some_and(|v| !v.is_empty() && v != "codex"))
            {
                item(
                    &tx,
                    &migration,
                    "providers",
                    source_id,
                    &source,
                    None,
                    "skipped",
                    "新配置中心迁移 Codex API 服务商、GPT 官方账号和 Google 账号",
                )?;
                continue;
            }
            let provider = source_id.to_string();
            let headers = decode(&source, "headers", json!({}))?;
            let protocol = if text(&source, "base_url")
                .trim_end_matches('/')
                .ends_with("/chat/completions")
            {
                "chat"
            } else {
                "responses"
            };
            tx.execute("INSERT INTO service_provider_configs(id,name,kind,upstream_protocol,base_url,proxy,headers_json,note,enabled,created_at,updated_at,icon)
                VALUES(?1,?2,?3,?10,?4,?5,?6,?7,?8,?9,?9,?11)",params![provider,
                source["name"].as_str().filter(|v|!v.trim().is_empty()).unwrap_or(source_id),kind,text(&source,"base_url"),
                if kind=="google_account"{text(&source,"google_proxy")}else{text(&source,"proxy")},headers.to_string(),text(&source,"note"),source["enabled"].as_i64().unwrap_or(1)!=0,now(),if kind=="api"{protocol}else{"responses"},text(&source,"icon")])?;
            let mut unreadable = false;
            if kind == "google_account" {
                let mut metadata = json!({});
                for key in ["subject","email","name","picture","project","plan","port","status","message","quota_warning"] {
                    metadata[key] = source[format!("google_{key}")].clone();
                }
                metadata["models"] = decode(&source, "google_models", json!([]))?;
                metadata["quotaGroups"] = decode(&source, "google_quota_groups", json!([]))?;
                let result = decrypt_provider_key(text(&source, "oauth_credentials"))
                    .and_then(|plain| Ok(serde_json::from_str::<Value>(&plain)?))
                    .and_then(|tokens| save_google_credentials(&tx, &provider, &metadata, &tokens));
                unreadable = result.is_err();
                for model in metadata["models"].as_array().into_iter().flatten() {
                    if !text(model,"id").is_empty() {
                        save_model(&tx,&provider,&json!({"upstreamModelId":model["id"],
                            "displayName":model["displayName"],"contextWindow":model["maxTokens"].as_i64().filter(|v|*v>0)}))?;
                    }
                }
            } else if kind == "codex_account" {
                let mut auth = source.clone();
                auth["account_id"] = source["account_id"].clone();
                auth["auth"] = json!({"expiresAt":source["auth_expires_at"]});
                if save_account(&tx, &provider, &auth).is_err() {
                    unreadable = true;
                }
                tx.execute(
                    "UPDATE service_provider_configs SET quota_json=?2 WHERE id=?1",
                    params![provider, decode(&source, "quota", json!({}))?.to_string()],
                )?;
            } else if table_exists(&tx, "provider_keys")? {
                let keys = query(
                    &tx,
                    "SELECT * FROM provider_keys WHERE provider_id=?1 ORDER BY sort_order,id",
                    &[&source_id],
                )?;
                for key in keys {
                    let ciphertext = text(&key, "value");
                    let plain = if ciphertext.is_empty() {
                        None
                    } else {
                        decrypt_provider_key(ciphertext).ok()
                    };
                    let fingerprint = plain
                        .as_ref()
                        .map(|s| hash(s))
                        .unwrap_or_else(|| text(&key, "key_hash").to_string());
                    let current = key["is_current"].as_i64().unwrap_or(1) == 1;
                    let active =
                        key["is_active"].as_i64().unwrap_or(0) == 1 && current && plain.is_some();
                    if current && plain.is_none() {
                        unreadable = true;
                    }
                    tx.execute("INSERT INTO service_provider_keys(id,provider_id,logical_id,name,note,secret_ciphertext,fingerprint,is_current,is_active,status,created_at,updated_at)
                        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",params![id(),provider,text(&key,"id"),text(&key,"name"),text(&key,"note"),
                        if current{ciphertext}else{""},fingerprint,current,active,if !current{"archived"}else if plain.is_some(){"active"}else{"unreadable"},now()])?;
                }
            }
            if kind == "api" && credential(&tx, &provider).is_err() {
                unreadable = true;
            }
            let mut models = BTreeMap::<String, Value>::new();
            let catalog = decode(&source, "model_catalog", json!({}))?;
            for entry in catalog["models"].as_array().into_iter().flatten() {
                let name = entry["slug"]
                    .as_str()
                    .or(entry["id"].as_str())
                    .unwrap_or("");
                if !name.is_empty() {
                    models.insert(name.to_string(), entry.clone());
                }
            }
            for entry in decode(&source, "model_names", json!([]))?
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(name) = entry.as_str().filter(|s| !s.is_empty()) {
                    models.entry(name.to_string()).or_insert(json!({}));
                }
            }
            for entry in decode(&source, "models", json!([]))?
                .as_array()
                .into_iter()
                .flatten()
            {
                let name = entry["name"]
                    .as_str()
                    .or(entry["model"].as_str())
                    .unwrap_or("");
                if !name.is_empty() {
                    models.entry(name.to_string()).or_insert(json!({}));
                }
            }
            let main = text(&source, "main_model");
            if !main.is_empty() {
                models.entry(main.into()).or_insert(json!({}));
            }
            let mut default = None;
            for (name, metadata) in models {
                let model = save_model(
                    &tx,
                    &provider,
                    &json!({"upstreamModelId":name,"displayName":metadata["display_name"].as_str().unwrap_or(&name),
                    "contextWindow":metadata["context_window"].as_i64().filter(|n|*n>0),"catalog":metadata}),
                )?;
                if name == main {
                    default = Some(model);
                }
            }
            let options = json!({"modelReasoningEffort":source["model_reasoning_effort"],"serviceTierFast":source["service_tier_fast"],
                "modelContextWindowEnabled":source["model_context_window_enabled"],"modelAutoCompactTokenLimit":source["model_auto_compact_token_limit"],
                "syncModelList":source["sync_model_list"] == 1 || source["sync_model_list"] == true});
            tx.execute("INSERT INTO service_provider_agent_bindings(provider_id,agent_type,enabled,default_model_id,options_json,updated_at)
                VALUES(?1,'codex',1,?2,?3,?4)",params![provider,default,options.to_string(),now()])?;
            for agent in ["claude", "claude-desktop"] {
                tx.execute("INSERT INTO service_provider_agent_bindings(provider_id,agent_type,enabled,updated_at) VALUES(?1,?2,0,?3)",params![provider,agent,now()])?;
            }
            item(
                &tx,
                &migration,
                "providers",
                source_id,
                &source,
                Some(&provider),
                if unreadable { "unreadable" } else { "copied" },
                if unreadable {
                    "配置已复制，部分凭据缺失或无法解密，请重新填写"
                } else {
                    "仅复制配置，未接管客户端；旧统计仍保留在旧表"
                },
            )?;
        }
    }
    tx.execute(
        "UPDATE service_operations SET status='completed',completed_at=?2 WHERE id=?1 AND operation_type='migration'",
        params![migration, now()],
    )?;
    tx.commit()?;
    Ok(json!({"migrationId":migration}))
}

fn decode(source: &Value, key: &str, default: Value) -> Result<Value, ManagerError> {
    match source.get(key).filter(|v| !v.is_null()) {
        Some(Value::String(raw)) if !raw.is_empty() => Ok(serde_json::from_str(raw)?),
        Some(v) if v.is_object() || v.is_array() => Ok(v.clone()),
        _ => Ok(default),
    }
}

fn item(
    c: &Connection,
    migration: &str,
    table: &str,
    source_id: &str,
    source: &Value,
    target: Option<&str>,
    status: &str,
    reason: &str,
) -> Result<(), ManagerError> {
    c.execute("INSERT INTO service_provider_migration_items(id,migration_id,source_table,source_id,source_fingerprint,target_id,status,reason)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id(),migration,table,source_id,hash(&source.to_string()),target,status,reason])?;
    Ok(())
}

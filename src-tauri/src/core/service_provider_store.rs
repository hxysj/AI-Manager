use crate::api::runtime_provider::{decrypt_provider_key, encrypt_provider_key};
use crate::core::{database, error::ManagerError, paths::AppPaths};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

mod migration;
mod google;
pub(crate) use google::*;
#[cfg(test)]
mod tests;

pub const SCHEMA: &str = include_str!("service_provider_store/schema.sql");
pub const MODE_KEY: &str = "service_provider:active_backend";
pub const GENERATION_KEY: &str = "service_provider:mode_generation";
// 官方账号随配置存储，但凭据、身份及额度属于本机数据，不参与业务备份和覆盖恢复。
pub const ACCOUNT_LOCAL_COLUMNS: &[&str] = &[
    "account_id", "account_email", "credentials_ciphertext", "token_expires_at",
    "credential_generation", "refresh_status", "quota_json", "account_updated_at",
    "account_metadata_json", "google_local_key_ciphertext", "google_gateway_port",
];

pub fn account_local_default(column: &str) -> Value {
    match column {
        "token_expires_at" | "account_updated_at" | "google_gateway_port" => Value::Null,
        "credential_generation" => json!(0),
        "quota_json" | "account_metadata_json" => json!("{}"),
        _ => json!(""),
    }
}
// 避开旧 Codex、Claude 和 Desktop 代理的 15721～15723 默认端口。
pub const PORT: u16 = 15724;
static LEGACY_REQUESTS: OnceLock<Mutex<HashMap<String, (usize, bool)>>> = OnceLock::new();

// 请求登记和切换闸门共用锁，流式响应结束或任务取消后才释放登记。
pub(crate) struct LegacyRequest {
    path: String,
}
impl Drop for LegacyRequest {
    fn drop(&mut self) {
        if let Ok(mut entries) = LEGACY_REQUESTS.get_or_init(Default::default).lock() {
            if let Some(entry) = entries.get_mut(&self.path) {
                entry.0 = entry.0.saturating_sub(1);
            }
        }
    }
}
pub(crate) fn legacy_request(paths: &AppPaths) -> Result<LegacyRequest, ManagerError> {
    register_gateway_request(paths, false)
}

// 多协议反代在两种模式下共用原有功能，只在请求期间固定当前数据源。
pub(crate) fn gateway_request(paths: &AppPaths) -> Result<LegacyRequest, ManagerError> {
    register_gateway_request(paths, true)
}

fn register_gateway_request(paths: &AppPaths, shared_sources: bool) -> Result<LegacyRequest, ManagerError> {
    let mut entries = LEGACY_REQUESTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| fail("请求登记不可用"))?;
    let entry = entries
        .entry(paths.storage_files.database.clone())
        .or_default();
    if entry.1 || (!shared_sources && active(paths)?) {
        return Err(fail("运行模式正在切换或旧网关已停用"));
    }
    entry.0 += 1;
    Ok(LegacyRequest {
        path: paths.storage_files.database.clone(),
    })
}
pub(crate) struct LegacySwitch {
    path: String,
}
impl Drop for LegacySwitch {
    fn drop(&mut self) {
        if let Ok(mut entries) = LEGACY_REQUESTS.get_or_init(Default::default).lock() {
            if let Some(entry) = entries.get_mut(&self.path) {
                entry.1 = false;
            }
        }
    }
}
pub(crate) fn pause_legacy(paths: &AppPaths) -> Result<LegacySwitch, ManagerError> {
    let mut entries = LEGACY_REQUESTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| fail("请求登记不可用"))?;
    let entry = entries
        .entry(paths.storage_files.database.clone())
        .or_default();
    if entry.0 > 0 || entry.1 {
        return Err(fail("仍有网关请求进行中，请结束请求后重试切换"));
    }
    entry.1 = true;
    Ok(LegacySwitch {
        path: paths.storage_files.database.clone(),
    })
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
pub fn fail(message: impl Into<String>) -> ManagerError {
    ManagerError::System(message.into())
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    database::initialize(paths)?;
    let mut connection = database::open(paths)?;
    let version: Option<String> = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key='service_provider:schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if version.as_deref().is_some_and(|v| !matches!(v, "1" | "2" | "3" | "4" | "5")) {
        return Err(fail("服务商配置数据库版本高于当前程序支持的版本"));
    }
    if version.as_deref() != Some("5") {
        connection.pragma_update(None, "foreign_keys", "OFF")?;
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !matches!(version.as_deref(), Some("4" | "5")) {
        migration::add_account_columns(&transaction)?;
    }
    transaction.execute_batch(SCHEMA)?;
    if !matches!(version.as_deref(), Some("2" | "3" | "4" | "5")) {
        let columns = query(
            &transaction,
            "SELECT name FROM pragma_table_info('service_provider_configs')",
            &[],
        )?;
        if !columns.iter().any(|column| text(column, "name") == "icon") {
            transaction.execute(
                "ALTER TABLE service_provider_configs ADD COLUMN icon TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        // 已迁移的配置只在升级时补齐缺失图标，不覆盖新模式的图标，也不回写旧表。
        if table_exists(&transaction, "providers")?
            && query(
                &transaction,
                "SELECT name FROM pragma_table_info('providers') WHERE name='icon'",
                &[],
            )?
            .iter()
            .any(|column| text(column, "name") == "icon")
        {
            transaction.execute(
                "UPDATE service_provider_configs SET icon=coalesce((
                    SELECT p.icon FROM providers p
                    JOIN service_provider_migration_items m ON m.source_id=p.id
                    WHERE m.source_table='providers' AND m.status IN ('copied','unreadable')
                        AND m.target_id=service_provider_configs.id LIMIT 1
                ),'') WHERE icon=''",
                [],
            )?;
        }
    }
    if !matches!(version.as_deref(), Some("3" | "4" | "5")) {
        let definition: String = transaction.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='service_provider_configs'",
            [], |row| row.get(0))?;
        if !definition.contains("google_account") {
            // 只重建新模式配置表的类型约束，原 Provider、Usage 和所有关联 ID 保持原值。
            let ddl = SCHEMA.split("CREATE TABLE IF NOT EXISTS service_provider_keys").next().unwrap()
                .replace("service_provider_configs", "service_provider_configs_v3");
            transaction.execute_batch(&ddl)?;
            transaction.execute_batch("INSERT INTO service_provider_configs_v3
                (id,name,icon,kind,upstream_protocol,base_url,proxy,headers_json,note,enabled,revision,deleted_at,created_at,updated_at)
                SELECT id,name,icon,kind,upstream_protocol,base_url,proxy,headers_json,note,enabled,revision,deleted_at,created_at,updated_at
                FROM service_provider_configs;
                DROP TABLE service_provider_configs;
                ALTER TABLE service_provider_configs_v3 RENAME TO service_provider_configs;")?;
        }
        if !query(&transaction, "SELECT * FROM pragma_foreign_key_check WHERE \"table\" LIKE 'service_%'", &[])?.is_empty() {
            return Err(fail("服务商配置升级发现关联数据不完整，已回滚升级"));
        }
    }
    if !matches!(version.as_deref(), Some("4" | "5")) {
        migration::merge_account_storage(&transaction)?;
    }
    if version.as_deref() != Some("5") {
        migration::merge_operation_storage(&transaction)?;
        if !query(&transaction, "SELECT * FROM pragma_foreign_key_check WHERE \"table\" LIKE 'service_%'", &[])?.is_empty() {
            return Err(fail("服务商操作记录升级发现关联数据不完整，已回滚升级"));
        }
        transaction.execute("INSERT INTO app_metadata(key,value) VALUES('service_provider:schema_version','5')
            ON CONFLICT(key) DO UPDATE SET value=excluded.value", [])?;
    }
    transaction.commit()?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    backfill_usage_costs(&mut connection)?;
    Ok(())
}

pub fn active(paths: &AppPaths) -> Result<bool, ManagerError> {
    let connection = database::open(paths)?;
    if !table_exists(&connection, "app_metadata")? {
        return Ok(false);
    }
    Ok(connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [MODE_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .as_deref()
        == Some("service"))
}

pub fn generation(connection: &Connection) -> Result<i64, ManagerError> {
    Ok(connection
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM app_metadata WHERE key=?1",
            [GENERATION_KEY],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

pub fn table_exists(connection: &Connection, table: &str) -> Result<bool, ManagerError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}

// 只用于内部固定 SQL；公开状态显式选择字段，凭据不会被带入界面或请求日志。
pub fn query(
    connection: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Value>, ManagerError> {
    let mut statement = connection.prepare(sql)?;
    let columns = statement
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut rows = statement.query(values)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let mut item = Map::new();
        for (index, name) in columns.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(v) => json!(v),
                ValueRef::Real(v) => json!(v),
                ValueRef::Text(v) => json!(String::from_utf8_lossy(v)),
                ValueRef::Blob(_) => return Err(fail("服务商配置包含不支持的二进制字段")),
            };
            if let Some(key) = name.strip_suffix("Json") {
                item.insert(
                    key.into(),
                    serde_json::from_str(value.as_str().unwrap_or("null"))?,
                );
            } else {
                item.insert(name.clone(), value);
            }
        }
        result.push(Value::Object(item));
    }
    Ok(result)
}

pub fn state(paths: &AppPaths) -> Result<Value, ManagerError> {
    initialize(paths)?;
    let c = database::open(paths)?;
    // 已在新模式中的用户升级后也能补齐旧 Google 账号，已复制项仍保持幂等。
    if active(paths)? && table_exists(&c, "providers")? && c.query_row(
        "SELECT EXISTS(SELECT 1 FROM providers p WHERE p.cli='codex' AND p.type='google-account'
        AND NOT EXISTS(SELECT 1 FROM service_provider_migration_items m WHERE m.source_table='providers'
        AND m.source_id=p.id AND m.status IN ('copied','unreadable')))", [], |row| row.get::<_, bool>(0))? {
        migrate(paths)?;
    }
    let providers = query(&c, "SELECT id,name,icon,kind,upstream_protocol AS upstreamProtocol,base_url AS baseUrl,proxy,
        note,enabled,revision,created_at AS createdAt,updated_at AS updatedAt,
        CASE WHEN kind='api' THEN EXISTS(SELECT 1 FROM service_provider_keys k WHERE k.provider_id=p.id AND k.is_active=1 AND k.status='active' AND k.secret_ciphertext<>'')
        ELSE credentials_ciphertext<>'' END AS hasCredential
        FROM service_provider_configs p WHERE deleted_at IS NULL ORDER BY created_at,id", &[])?;
    let mut models = query(&c, "SELECT id,provider_id AS providerId,upstream_model_id AS upstreamModelId,display_name AS displayName,
        context_window AS contextWindow,catalog_json AS catalogJson,enabled,input_price_micros AS inputPriceMicros,
        cached_input_price_micros AS cachedInputPriceMicros,output_price_micros AS outputPriceMicros,currency FROM service_provider_models", &[])?;
    let pricing = crate::core::usage_store::read_pricing_connection(&c)?;
    for model in &mut models {
        model["effectivePricing"] = model_pricing(model, &pricing);
    }
    let bindings = query(&c, "SELECT provider_id AS providerId,agent_type AS agentType,enabled,default_model_id AS defaultModelId,options_json AS optionsJson,revision FROM service_provider_agent_bindings", &[])?;
    let keys = query(&c, "SELECT id,provider_id AS providerId,logical_id AS logicalId,name,note,is_current AS isCurrent,is_active AS isActive,status,substr(fingerprint,1,12) AS fingerprint FROM service_provider_keys WHERE is_current=1", &[])?;
    let instances = query(&c, "SELECT id,agent_type AS agentType,name,config_path AS configPath,agent_version AS agentVersion,
        client_protocol AS clientProtocol,provider_id AS providerId,model_id AS modelId,status,sync_status AS syncStatus,
        applied_revision AS appliedRevision,last_synced_at AS lastSyncedAt,last_error AS lastError,revision FROM service_agent_instances", &[])?;
    let migrations = query(&c, "SELECT m.id,m.status,m.created_at AS createdAt,m.completed_at AS completedAt,
        (SELECT count(*) FROM service_provider_migration_items i WHERE i.migration_id=m.id AND i.status='copied') AS copied,
        (SELECT count(*) FROM service_provider_migration_items i WHERE i.migration_id=m.id AND i.status='skipped') AS skipped,
        (SELECT count(*) FROM service_provider_migration_items i WHERE i.migration_id=m.id AND i.status='unreadable') AS unreadable
        FROM service_operations m WHERE m.operation_type='migration' ORDER BY m.created_at DESC,m.id LIMIT 10", &[])?;
    let migration_items = query(&c, "SELECT source_table AS sourceTable,source_id AS sourceId,target_id AS targetId,status,reason
        FROM service_provider_migration_items WHERE migration_id=(SELECT id FROM service_operations WHERE operation_type='migration' ORDER BY created_at DESC,id LIMIT 1)", &[])?;
    let compatibility = query(&c, "SELECT id,provider_id AS providerId,agent_type AS agentType,model_id AS modelId,
        provider_revision AS providerRevision,agent_version AS agentVersion,client_protocol AS clientProtocol,capability,status,evidence_json AS evidenceJson,completed_at AS testedAt
        FROM service_operations WHERE operation_type='compatibility' ORDER BY completed_at DESC,id LIMIT 100", &[])?;
    let routes=query(&c,"SELECT id,provider_id AS providerId,agent_type AS agentType,client_model_name AS clientModelName,model_id AS modelId,enabled FROM service_agent_model_routes",&[])?;
    let accounts=query(&c,"SELECT id AS providerId,account_email AS email,account_id AS accountId,
        token_expires_at AS expiresAt,quota_json AS quotaJson,refresh_status AS refreshStatus
        FROM service_provider_configs WHERE kind='codex_account' AND account_id<>'' AND deleted_at IS NULL",&[])?;
    let google_accounts=query(&c,"SELECT id AS providerId,account_email AS email,account_id AS subject,
        token_expires_at AS expiresAt,account_metadata_json AS metadataJson
        FROM service_provider_configs WHERE kind='google_account' AND account_id<>'' AND deleted_at IS NULL",&[])?;
    let files=query(&c,"SELECT id,instance_id AS instanceId,file_path AS filePath,write_status AS writeStatus,restore_status AS restoreStatus,
        error_message AS errorMessage FROM service_managed_files WHERE restore_status<>'restored'",&[])?;
    Ok(
        json!({"backend": if active(paths)? {"service"} else {"legacy"}, "generation":generation(&c)?,
        "baseUrl":format!("http://127.0.0.1:{PORT}"), "providers":providers,"models":models,"bindings":bindings,
        "keys":keys,"instances":instances,"migrations":migrations,"migrationItems":migration_items,
        "compatibility":compatibility,"routes":routes,"accounts":accounts,"googleAccounts":google_accounts,"managedFiles":files,"usage":usage(&c)?,
        "desktopModelRoutes":crate::api::claude_desktop::ROUTES.iter().map(|(id,label,_)|json!({"id":id,"label":label})).collect::<Vec<_>>()}),
    )
}

pub fn config(connection: &Connection, provider_id: &str) -> Result<Value, ManagerError> {
    query(connection, "SELECT id,name,icon,kind,upstream_protocol AS upstreamProtocol,base_url AS baseUrl,proxy,
        headers_json AS headersJson,note,enabled,revision,deleted_at AS deletedAt FROM service_provider_configs WHERE id=?1", &[&provider_id])?
        .pop().ok_or_else(|| fail("服务商配置不存在"))
}

// 按模型继承整套全局价格；显式零价和不完整的服务商报价均保留原意，避免混用币种。
pub(crate) fn model_pricing(model: &Value, pricing: &Value) -> Value {
    let mut snapshot = json!({
        "inputPriceMicros": model["inputPriceMicros"],
        "cachedInputPriceMicros": model["cachedInputPriceMicros"],
        "outputPriceMicros": model["outputPriceMicros"],
        "currency": model["currency"].as_str().unwrap_or("USD"),
        "source": "provider"
    });
    if ["inputPriceMicros", "cachedInputPriceMicros", "outputPriceMicros"]
        .iter().any(|key| model[*key].is_number()) {
        return snapshot;
    }
    snapshot["source"] = json!("unknown");
    let name = text(model, "upstreamModelId").trim().to_lowercase();
    let global = pricing["items"].as_array().into_iter().flatten()
        .find(|item| !name.is_empty() && text(item, "modelId").trim().to_lowercase() == name);
    if let Some(global) = global {
        for (local, shared) in [
            ("inputPriceMicros", "inputCostPerMillion"),
            ("cachedInputPriceMicros", "cacheReadCostPerMillion"),
            ("outputPriceMicros", "outputCostPerMillion"),
        ] {
            snapshot[local] = json!(global[shared].as_f64().map(|price| price * 1_000_000.0));
        }
        snapshot["currency"] = global["currency"].clone();
        snapshot["source"] = json!("global");
        snapshot["usagePricingSnapshot"] = json!({
            "model": global,
            "exchangeRate": pricing["exchangeRate"]
        });
    }
    snapshot
}

// 新请求与历史补算共用同一计价规则，缺少用量或单价时继续保留未知。
pub(crate) fn estimate_request_cost(
    input: Option<i64>,
    output: Option<i64>,
    cached: Option<i64>,
    price: &Value,
) -> Option<i64> {
    let (i, o, k) = (input?, output?, cached?);
    if i < 0 || o < 0 || k < 0 || k > i {
        return None;
    }
    if price["source"] == "global" {
        let pi = price["inputPriceMicros"].as_f64()?;
        let po = price["outputPriceMicros"].as_f64()?;
        let pc = if k == 0 { 0.0 } else { price["cachedInputPriceMicros"].as_f64()? };
        if [pi, po, pc].iter().any(|value| !value.is_finite() || *value < 0.0) {
            return None;
        }
        let cost = (((i - k) as f64 * pi + o as f64 * po + k as f64 * pc)
            / 1_000_000.0).ceil();
        return (cost.is_finite() && cost >= 0.0 && cost < i64::MAX as f64)
            .then_some(cost as i64);
    }
    let pi = price["inputPriceMicros"].as_i64()?;
    let po = price["outputPriceMicros"].as_i64()?;
    let pc = if k == 0 { 0 } else { price["cachedInputPriceMicros"].as_i64()? };
    if pi < 0 || po < 0 || pc < 0 {
        return None;
    }
    i64::try_from(((i - k) as i128 * pi as i128 + o as i128 * po as i128
        + k as i128 * pc as i128 + 999_999) / 1_000_000).ok()
}

// 只补齐已结束且费用未锁定的账本；优先使用请求快照，旧空快照再继承 Usage。
pub(crate) fn backfill_usage_costs(c: &mut Connection) -> Result<(), ManagerError> {
    let pending: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM service_gateway_attempts
        WHERE estimated_cost_micros IS NULL AND finished_at IS NOT NULL
        AND input_tokens IS NOT NULL AND output_tokens IS NOT NULL AND cached_input_tokens IS NOT NULL)",
        [], |row| row.get(0))?;
    if !pending {
        return Ok(());
    }
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let pricing = crate::core::usage_store::read_pricing_connection(&tx)?;
    let rows = query(&tx, "SELECT id,upstream_model AS upstreamModelId,currency,input_tokens AS input,
        output_tokens AS output,cached_input_tokens AS cached,price_snapshot_json AS priceJson
        FROM service_gateway_attempts WHERE estimated_cost_micros IS NULL AND finished_at IS NOT NULL
        AND input_tokens IS NOT NULL AND output_tokens IS NOT NULL AND cached_input_tokens IS NOT NULL", &[])?;
    for row in rows {
        let mut snapshot = row["price"].as_object().cloned().unwrap_or_default();
        if !["inputPriceMicros", "cachedInputPriceMicros", "outputPriceMicros"]
            .iter().any(|key| snapshot.get(*key).is_some_and(Value::is_number)) {
            let saved = snapshot.get("usagePricingSnapshot");
            let historical = saved.filter(|saved| saved["model"].is_object()).map(|saved|
                json!({"exchangeRate":saved["exchangeRate"],"items":[saved["model"]]}));
            let inherited = model_pricing(&row, historical.as_ref().unwrap_or(&pricing));
            snapshot.extend(inherited.as_object().unwrap().clone());
        }
        let mut snapshot = Value::Object(snapshot);
        if let Some(cost) = estimate_request_cost(row["input"].as_i64(), row["output"].as_i64(),
            row["cached"].as_i64(), &snapshot) {
            snapshot["backfilledAt"] = json!(now());
            tx.execute("UPDATE service_gateway_attempts SET estimated_cost_micros=?2,
                currency=?3,price_snapshot_json=?4 WHERE id=?1
                AND estimated_cost_micros IS NULL AND finished_at IS NOT NULL",
                params![text(&row,"id"),cost,snapshot["currency"].as_str().unwrap_or(text(&row,"currency")),
                    snapshot.to_string()])?;
        }
    }
    tx.commit()?;
    Ok(())
}

// 旧官方账号存储 ID 自带 account:，公开账号 ID 与历史设置继续兼容。
pub(crate) fn source_provider_id(c: &Connection, target: &str) -> Result<String, ManagerError> {
    if !query(
        c,
        "SELECT id FROM service_provider_configs WHERE id=?1",
        &[&target],
    )?
    .is_empty()
    {
        return Ok(target.into());
    }
    for candidate in [
        target
            .strip_prefix("account:")
            .unwrap_or(target)
            .to_string(),
        format!("account:{target}"),
    ] {
        if !query(
            c,
            "SELECT id FROM service_provider_configs WHERE id=?1 AND kind='codex_account'",
            &[&candidate],
        )?
        .is_empty()
        {
            return Ok(candidate);
        }
    }
    Err(fail("服务商配置不存在"))
}

// 为现有助手、翻译和图片选择器提供新表的只读投影，不经过旧 Provider 存储。
pub fn public_catalog(paths: &AppPaths) -> Result<Value, ManagerError> {
    let c = database::open(paths)?;
    let rows = query(&c, "SELECT p.id,p.name,p.icon,p.kind,p.enabled,p.proxy,b.enabled AS authorized,
        p.account_id AS accountId,p.account_email AS email,p.token_expires_at AS expiresAt,p.quota_json AS usageJson,
        CASE WHEN p.kind='api' THEN EXISTS(SELECT 1 FROM service_provider_keys k WHERE k.provider_id=p.id
            AND k.is_active=1 AND k.status='active' AND k.secret_ciphertext<>'')
        ELSE p.credentials_ciphertext<>'' END AS hasCredential
        FROM service_provider_configs p LEFT JOIN service_provider_agent_bindings b ON b.provider_id=p.id AND b.agent_type='codex'
        WHERE p.deleted_at IS NULL ORDER BY p.created_at,p.id", &[])?;
    let models = query(&c, "SELECT id,provider_id AS providerId,upstream_model_id AS name FROM service_provider_models WHERE enabled=1", &[])?;
    let mut providers = Vec::new();
    let mut accounts = Vec::new();
    for row in rows {
        let names = models
            .iter()
            .filter(|m| m["providerId"] == row["id"])
            .map(|m| m["name"].clone())
            .collect::<Vec<_>>();
        let available = row["enabled"] == 1 && row["authorized"] == 1 && row["hasCredential"] == 1;
        if row["kind"] != "codex_account" {
            providers.push(json!({"id":row["id"],"name":row["name"],"icon":row["icon"],"cli":"codex","enabled":available,
                "type":if row["kind"]=="google_account"{"google-account"}else{"openai"},
                "runtimeConfig":{"mainModel":names.first(),"modelList":names},"serviceManaged":true}));
        } else {
            accounts.push(json!({"id":text(&row,"id").strip_prefix("account:").unwrap_or(text(&row,"id")),"serviceProviderId":row["id"],"type":"codex","email":row["email"],"accountId":row["accountId"],
                "name":row["name"],"icon":row["icon"],"proxy":row["proxy"],"disabled":!available,"requires_reauth":row["hasCredential"]!=1,
                "active":false,"usage":row["usage"],"plan":row["usage"]["plan_type"],"expiresAt":row["expiresAt"],"modelNames":names,"serviceManaged":true}));
        }
    }
    let profiles=query(&c,"SELECT i.id,i.agent_type AS cli,i.provider_id AS providerId,m.upstream_model_id AS model
        FROM service_agent_instances i JOIN service_provider_models m ON m.id=i.model_id
        WHERE i.status='active' AND i.sync_status='synced' AND i.config_path NOT LIKE 'internal:%'
        AND i.config_path NOT LIKE 'test:%' AND i.config_path NOT LIKE 'external:%' ORDER BY i.last_synced_at DESC",&[])?;
    Ok(
        json!({"providers":providers,"codexAccounts":accounts,"runtimeModels":models,"runtimeProfiles":profiles}),
    )
}

fn valid_url(value: &str, name: &str) -> Result<(), ManagerError> {
    let url = url::Url::parse(value).map_err(|_| fail(format!("{name} 必须是完整的 HTTP 地址")))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err(fail(format!("{name} 不支持地址凭据、查询参数或片段")));
    }
    if url.port_or_known_default() == Some(PORT)
        && matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1" | "0.0.0.0")
        )
    {
        return Err(fail("上游地址不能指向共享网关自身"));
    }
    Ok(())
}

pub fn save(paths: &AppPaths, value: &Value) -> Result<Value, ManagerError> {
    initialize(paths)?;
    let provider_id = if text(value, "id").is_empty() {
        id()
    } else {
        text(value, "id").to_string()
    };
    let name = text(value, "name").trim();
    if name.is_empty() {
        return Err(fail("请填写服务商名称"));
    }
    let kind = value["kind"].as_str().unwrap_or("api");
    let protocol = value["upstreamProtocol"].as_str().unwrap_or("responses");
    if !matches!(kind, "api" | "codex_account" | "google_account")
        || !matches!(protocol, "chat" | "responses")
        || (kind != "api" && protocol != "responses")
    {
        return Err(fail("服务商类型或上游协议无效"));
    }
    if kind == "api" {
        valid_url(text(value, "baseUrl"), "上游地址")?;
    }
    let headers = value.get("headers").cloned().unwrap_or(json!({}));
    let header_map = headers
        .as_object()
        .ok_or_else(|| fail("自定义请求头必须为对象"))?;
    for (key, val) in header_map {
        if [
            "authorization",
            "x-api-key",
            "host",
            "content-length",
            "connection",
            "transfer-encoding",
        ]
        .contains(&key.to_ascii_lowercase().as_str())
            || !val.is_string()
            || reqwest::header::HeaderName::from_bytes(key.as_bytes()).is_err()
            || reqwest::header::HeaderValue::from_str(val.as_str().unwrap_or("")).is_err()
        {
            return Err(fail("自定义请求头无效，认证信息请使用独立凭据字段"));
        }
    }
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous = query(
        &tx,
        "SELECT revision,kind FROM service_provider_configs WHERE id=?1 AND deleted_at IS NULL",
        &[&provider_id],
    )?
    .pop();
    if value["enabled"] == false && tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM service_gateway_requests r
        JOIN service_agent_instances i ON i.id=r.instance_id
        WHERE i.provider_id=?1 AND r.finished_at IS NULL)",
        [&provider_id], |row| row.get::<_, bool>(0))? {
        return Err(fail("服务商仍有请求进行中，请结束请求后再禁用"));
    }
    if let Some(p) = &previous {
        if value["revision"].as_i64() != p["revision"].as_i64() {
            return Err(fail("配置已被修改，请刷新后重试"));
        }
        if text(p, "kind") != kind {
            return Err(fail("已有服务商不能改变凭据类型，请新建配置"));
        }
        tx.execute("UPDATE service_provider_configs SET name=?2,upstream_protocol=?3,base_url=?4,proxy=?5,headers_json=?6,
            note=?7,enabled=?8,revision=revision+1,updated_at=?9,icon=coalesce(?10,icon) WHERE id=?1",
            params![provider_id,name,protocol,text(value,"baseUrl"),text(value,"proxy"),headers.to_string(),text(value,"note"),value["enabled"].as_bool().unwrap_or(true),now(),value["icon"].as_str()])?;
    } else {
        if !text(value, "id").is_empty() {
            return Err(fail("配置已删除或不存在"));
        }
        tx.execute("INSERT INTO service_provider_configs(id,name,kind,upstream_protocol,base_url,proxy,headers_json,note,enabled,created_at,updated_at,icon)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11)",params![provider_id,name,kind,protocol,text(value,"baseUrl"),text(value,"proxy"),headers.to_string(),text(value,"note"),value["enabled"].as_bool().unwrap_or(true),now(),text(value,"icon")])?;
    }
    if let Some(secret) = value["apiKey"].as_str().filter(|s| !s.is_empty()) {
        if kind != "api" {
            return Err(fail("官方账号请导入 auth.json"));
        }
        save_key(
            &tx,
            &provider_id,
            value["keyLogicalId"].as_str().unwrap_or("default"),
            value["keyName"].as_str().unwrap_or("默认 Key"),
            secret,
        )?;
    }
    if let Some(models) = value["models"].as_array() {
        for model in models {
            save_model(&tx, &provider_id, model)?;
        }
    }
    if let Some(bindings) = value["bindings"].as_array() {
        for binding in bindings {
            let agent = text(binding, "agentType");
            if !["codex", "claude", "claude-desktop", "gateway"].contains(&agent) {
                return Err(fail("Agent 标识无效"));
            }
            let options = binding.get("options").cloned().unwrap_or(json!({}));
            validate_agent_options(
                &tx,
                &provider_id,
                agent,
                binding["enabled"].as_bool().unwrap_or(false),
                &options,
            )?;
            // 表单用上游模型名选择默认项，新模型在同一事务中保存后再解析本地 ID。
            // 未提供模型名的调用方继续使用原有 defaultModelId 契约。
            let default = if let Some(name) = binding["defaultModelName"].as_str() {
                if name.is_empty() {
                    None
                } else {
                    Some(
                        tx.query_row(
                            "SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id=?2 AND (?3=0 OR enabled=1)",
                            params![provider_id, name, binding["enabled"].as_bool().unwrap_or(false)],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                        .ok_or_else(|| fail("默认模型不存在或已停用，请重新选择"))?,
                    )
                }
            } else {
                binding["defaultModelId"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            };
            tx.execute("INSERT INTO service_provider_agent_bindings(provider_id,agent_type,enabled,default_model_id,options_json,updated_at)
                VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(provider_id,agent_type) DO UPDATE SET enabled=excluded.enabled,
                default_model_id=excluded.default_model_id,options_json=excluded.options_json,revision=revision+1,updated_at=excluded.updated_at",
                params![provider_id,agent,binding["enabled"].as_bool().unwrap_or(false),default,options.to_string(),now()])?;
        }
    }
    if let Some(routes) = value["routes"].as_array() {
        tx.execute(
            "DELETE FROM service_agent_model_routes WHERE provider_id=?1",
            [&provider_id],
        )?;
        for route in routes {
            tx.execute("INSERT INTO service_agent_model_routes(id,provider_id,agent_type,client_model_name,model_id,enabled)
                VALUES(?1,?2,?3,?4,?5,?6)",params![id(),provider_id,text(route,"agentType"),text(route,"clientModelName"),
                text(route,"modelId"),route["enabled"].as_bool().unwrap_or(true)])?;
        }
    }
    if value["enabled"] == false {
        // 停用即撤销访问和启动状态；模型参数与文件备份保留用于恢复。
        tx.execute("UPDATE service_provider_agent_bindings SET enabled=0,revision=revision+1,updated_at=?2
            WHERE provider_id=?1", params![provider_id, now()])?;
        tx.execute("UPDATE service_gateway_tokens SET status='revoked',revoked_at=?2
            WHERE instance_id IN (SELECT id FROM service_agent_instances WHERE provider_id=?1)
            AND status='active'", params![provider_id, now()])?;
        tx.execute("UPDATE service_agent_instances SET status='error',sync_status='pending',
            last_error='服务商已禁用，等待恢复客户端配置',updated_at=?2
            WHERE provider_id=?1 AND status<>'detached'", params![provider_id, now()])?;
    }
    tx.commit()?;
    if previous.is_some() && value["enabled"] != false {
        crate::api::service_provider::sync_provider(paths, &provider_id)?;
    }
    Ok(json!({"id":provider_id}))
}

pub(crate) fn validate_credential(c: &Connection, provider: &str) -> Result<(), ManagerError> {
    match text(&config(c, provider)?, "kind") {
        "api" => { credential(c, provider)?; }
        "google_account" => { google_tokens(c, provider)?; }
        _ => { account(c, provider)?; }
    }
    Ok(())
}

// Agent 模型规则与公共目录在同一事务校验，避免跨服务商引用或应用已停用的模型。
pub(crate) fn validate_agent_options(
    c: &Connection,
    provider: &str,
    agent: &str,
    enabled: bool,
    options: &Value,
) -> Result<(), ManagerError> {
    let fields = options
        .as_object()
        .ok_or_else(|| fail("Agent 选项必须为对象"))?;
    let mut model_names = Vec::new();
    for (key, val) in fields {
        let valid = match (agent, key.as_str()) {
            ("codex", "modelReasoningEffort") => {
                val.is_null()
                    || matches!(val.as_str(), Some("minimal" | "low" | "medium" | "high" | "xhigh" | "ultra" | "none"))
            }
            ("codex", "serviceTierFast" | "modelContextWindowEnabled" | "syncModelList") => {
                val.is_null() || val.is_boolean() || matches!(val.as_i64(), Some(0 | 1))
            }
            ("codex", "modelAutoCompactTokenLimit") => {
                val.is_null() || val.as_i64().is_some_and(|n| n > 0)
            }
            // 兼容已保存的旧子列表，仅校验类型，不再作为生效模型引用。
            ("codex", "modelList") => val.as_array().is_some_and(|names| {
                let mut seen = std::collections::HashSet::new();
                names.iter().all(|name| {
                    name.as_str().is_some_and(|name| {
                        if name.is_empty() || name.trim() != name || !seen.insert(name) {
                            return false;
                        }
                        true
                    })
                })
            }),
            ("claude", "haikuModel" | "sonnetModel" | "opusModel" | "subagentModel") => {
                val.is_null() || val.as_str().is_some_and(|name| {
                    if !name.is_empty() {
                        model_names.push(name);
                    }
                    name.trim() == name
                })
            }
            ("claude-desktop", "modelMappings") => val.as_object().is_some_and(|mappings| {
                mappings.iter().all(|(route, mapping)| {
                    crate::api::claude_desktop::ROUTES.iter().any(|(id, _, _)| route == id)
                        && mapping.as_object().is_some_and(|mapping| {
                            mapping.iter().all(|(key, val)| match key.as_str() {
                                "model" => val.as_str().is_some_and(|name| {
                                    if !name.is_empty() {
                                        model_names.push(name);
                                    }
                                    name.trim() == name
                                }),
                                "labelOverride" => val.is_string(),
                                "supports1m" => val.is_boolean(),
                                _ => false,
                            })
                        })
                })
            }),
            _ => false,
        };
        if !valid {
            return Err(fail(format!("{agent} 参数 {key} 尚不支持或值无效")));
        }
    }
    for name in model_names {
        let exists: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id=?2 AND (?3=0 OR enabled=1))",
            params![provider, name, enabled],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(fail(format!("{agent} 模型 {name} 不存在于此服务商或已停用")));
        }
    }
    Ok(())
}

// Desktop 客户端目录和网关共同使用同一份映射，空档位继承首个已配置档位的模型和能力。
pub(crate) fn desktop_model_mappings(options: &Value, default_model: &str) -> Vec<Value> {
    let Some(mappings) = options["modelMappings"].as_object() else {
        return Vec::new();
    };
    let first = crate::api::claude_desktop::ROUTES
        .iter()
        .filter_map(|(route, _, _)| mappings.get(*route))
        .find(|mapping| !text(mapping, "model").is_empty());
    crate::api::claude_desktop::ROUTES.iter().map(|(route, _, _)| {
        let own = mappings.get(*route);
        let mapping = own.filter(|mapping| !text(mapping, "model").is_empty()).or(first);
        let model = mapping.map(|mapping| text(mapping, "model")).unwrap_or(default_model);
        json!({
            "clientModelName":route,
            "upstreamModelId":model,
            "supports1m":own.and_then(|mapping|mapping["supports1m"].as_bool())
                .unwrap_or_else(||mapping.is_some_and(|mapping| mapping["supports1m"] == true)),
            "labelOverride":own.map(|mapping|text(mapping,"labelOverride")).unwrap_or("")
        })
    }).collect()
}

pub fn save_key(
    c: &Connection,
    provider: &str,
    logical: &str,
    name: &str,
    secret: &str,
) -> Result<(), ManagerError> {
    if logical.is_empty() || secret.is_empty() {
        return Err(fail("Key 标识和内容不能为空"));
    }
    let fingerprint = hash(secret);
    c.execute("UPDATE service_provider_keys SET is_current=0,is_active=0,status='archived',secret_ciphertext='',updated_at=?3
        WHERE provider_id=?1 AND logical_id=?2 AND fingerprint<>?4",params![provider,logical,now(),fingerprint])?;
    c.execute(
        "UPDATE service_provider_keys SET is_active=0 WHERE provider_id=?1",
        [provider],
    )?;
    c.execute("INSERT INTO service_provider_keys(id,provider_id,logical_id,name,secret_ciphertext,fingerprint,is_active,created_at,updated_at)
        VALUES(?1,?2,?3,?4,?5,?6,1,?7,?7) ON CONFLICT(provider_id,logical_id,fingerprint) DO UPDATE SET name=excluded.name,
        secret_ciphertext=excluded.secret_ciphertext,is_current=1,is_active=1,status='active',updated_at=excluded.updated_at",
        params![id(),provider,logical,name,encrypt_provider_key(secret)?,fingerprint,now()])?;
    Ok(())
}

pub fn save_model(c: &Connection, provider: &str, model: &Value) -> Result<String, ManagerError> {
    let name = model
        .as_str()
        .unwrap_or_else(|| text(model, "upstreamModelId"))
        .trim();
    if name.is_empty() {
        return Err(fail("模型标识不能为空"));
    }
    let catalog = model.get("catalog").cloned().unwrap_or(json!({}));
    if !catalog.is_object() {
        return Err(fail("模型目录扩展必须为对象"));
    }
    let context = model
        .get("contextWindow")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_i64()
                .filter(|n| *n > 0)
                .ok_or_else(|| fail("上下文窗口必须为正整数"))
        })
        .transpose()?;
    let existing: Option<String> = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id=?2",
            params![provider, name],
            |r| r.get(0),
        )
        .optional()?;
    if model.is_string() {
        if let Some(model_id) = existing {
            return Ok(model_id);
        }
    }
    let model_id = existing.unwrap_or_else(id);
    let prices = [
        "inputPriceMicros",
        "cachedInputPriceMicros",
        "outputPriceMicros",
    ]
    .map(|key| {
        model
            .get(key)
            .filter(|v| !v.is_null())
            .map(|v| {
                v.as_i64()
                    .filter(|v| *v >= 0)
                    .ok_or_else(|| fail("模型单价必须为非负整数"))
            })
            .transpose()
    });
    let [input, cached, output] = prices;
    c.execute("INSERT INTO service_provider_models(id,provider_id,upstream_model_id,display_name,context_window,catalog_json,
        enabled,input_price_micros,cached_input_price_micros,output_price_micros,currency,created_at,updated_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12) ON CONFLICT(provider_id,upstream_model_id) DO UPDATE SET
        display_name=excluded.display_name,context_window=excluded.context_window,catalog_json=excluded.catalog_json,
        enabled=excluded.enabled,input_price_micros=excluded.input_price_micros,cached_input_price_micros=excluded.cached_input_price_micros,
        output_price_micros=excluded.output_price_micros,currency=excluded.currency,updated_at=excluded.updated_at",
        params![model_id,provider,name,model["displayName"].as_str().unwrap_or(name),context,catalog.to_string(),
            model["enabled"].as_bool().unwrap_or(true),input?,cached?,output?,model["currency"].as_str().unwrap_or("USD"),now()])?;
    Ok(model_id)
}

pub fn remove(paths: &AppPaths, provider: &str, revision: i64) -> Result<(), ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM service_agent_instances WHERE provider_id=?1 AND status='active')",[provider],|r|r.get::<_,bool>(0))? {
        return Err(fail("请先解除使用该服务商的 Agent 配置"));
    }
    let changed=tx.execute("UPDATE service_provider_configs SET deleted_at=?3,enabled=0,revision=revision+1,updated_at=?3
        WHERE id=?1 AND revision=?2 AND deleted_at IS NULL",params![provider,revision,now()])?;
    if changed != 1 {
        return Err(fail("配置版本已变化，请刷新后重试"));
    }
    tx.commit()?;
    Ok(())
}

pub fn credential(c: &Connection, provider: &str) -> Result<Value, ManagerError> {
    let mut key = query(
        c,
        "SELECT id,name,fingerprint,secret_ciphertext AS ciphertext FROM service_provider_keys
        WHERE provider_id=?1 AND is_active=1 AND is_current=1 AND status='active'",
        &[&provider],
    )?
    .pop()
    .ok_or_else(|| fail("服务商缺少可用的 API Key"))?;
    key["secret"] = json!(decrypt_provider_key(text(&key, "ciphertext"))?);
    Ok(key)
}

pub fn account(c: &Connection, provider: &str) -> Result<Value, ManagerError> {
    let mut item = query(
        c,
        "SELECT id,id AS provider_id,account_id AS external_account_id,account_email AS email,
        token_expires_at AS expires_at,credential_generation AS token_generation,
        credential_generation AS revision,credentials_ciphertext FROM service_provider_configs
        WHERE id=?1 AND kind='codex_account' AND deleted_at IS NULL AND credentials_ciphertext<>''",
        &[&provider],
    )?
    .pop()
    .ok_or_else(|| fail("官方账号凭据不存在"))?;
    let credentials: Value = serde_json::from_str(&decrypt_provider_key(text(&item, "credentials_ciphertext"))?)?;
    item.as_object_mut().unwrap().remove("credentials_ciphertext");
    for (source, target) in [
        ("access_token", "accessToken"),
        ("refresh_token", "refreshToken"),
        ("id_token", "idToken"),
    ] {
        item[target] = json!(text(&credentials, source));
    }
    Ok(item)
}

// 只在授权成功后的事务中调用；复用同一官方身份，保留用户已修改的名称与 Agent 关联。
pub(crate) fn prepare_official_login(
    c: &Connection,
    target: &str,
    kind: &str,
    identity: &str,
    name: &str,
    proxy: &str,
) -> Result<String, ManagerError> {
    if !matches!(kind, "codex_account" | "google_account") || identity.trim().is_empty() {
        return Err(fail("官方账号身份无效"));
    }
    // 新增 Codex 账号只复用有效记录，已删除记录保留历史，不作为重新登录的更新目标。
    let existing = query(c,
        "SELECT id,kind,account_id,name,deleted_at FROM service_provider_configs
        WHERE kind=?1 AND account_id=?2 AND (kind<>'codex_account' OR deleted_at IS NULL)
        ORDER BY created_at,id LIMIT 1",
        &[&kind, &identity])?.pop();
    let provider = if target.is_empty() {
        existing.as_ref().map(|row| text(row, "id").to_string()).unwrap_or_default()
    } else {
        target.to_string()
    };
    let default_name = if kind == "google_account" { "Google 官方账号" } else { "Codex 官方账号" };
    let name = if name.trim().is_empty() { default_name } else { name.trim() };
    if provider.is_empty() {
        let provider = id();
        let icon = if kind == "google_account" { "google.svg" } else { "codex.svg" };
        c.execute("INSERT INTO service_provider_configs(id,name,kind,upstream_protocol,proxy,icon,enabled,created_at,updated_at)
            VALUES(?1,?2,?3,'responses',?4,?5,1,?6,?6)",
            params![provider,name,kind,proxy,icon,now()])?;
        return Ok(provider);
    }
    let config = config(c, &provider)?;
    if config["kind"] != kind || !config["deletedAt"].is_null() {
        return Err(fail("官方账号不存在、类型不匹配或已删除"));
    }
    let old_identity: String = c.query_row("SELECT account_id FROM service_provider_configs WHERE id=?1",
        [&provider], |row| row.get(0))?;
    if !old_identity.is_empty() && old_identity != identity {
        return Err(fail("登录账号与待更新的官方账号不一致"));
    }
    if existing.as_ref().is_some_and(|row| text(row, "id") != provider) {
        return Err(fail("此官方账号已存在，请选择已有账号"));
    }
    let old_name = text(&config, "name");
    let automatic_name = old_name.is_empty() || [
        provider.as_str(), identity, "GPT / Codex 官方账号", "Codex 官方账号",
        "Google 账号", "Google 官方账号",
    ].contains(&old_name);
    c.execute("UPDATE service_provider_configs SET name=?2,proxy=?3,revision=revision+1,updated_at=?4 WHERE id=?1",
        params![provider,if automatic_name { name } else { old_name },proxy,now()])?;
    Ok(provider)
}

// 刷新失败附着到对应账号，保留最近额度；凭据已更新时忽略旧请求的错误。
pub fn record_account_refresh_error(c: &Connection, provider: &str, generation: i64, error: &ManagerError) -> Result<(), ManagerError> {
    let status_code = match error {
        ManagerError::OpenAiRequest { status_code, .. } => *status_code,
        _ => 0,
    };
    c.execute("UPDATE service_provider_configs SET refresh_status='failed',
        quota_json=json_set(quota_json,'$.refreshError',json(?2)),account_updated_at=?3
        WHERE id=?1 AND kind='codex_account' AND deleted_at IS NULL AND credential_generation=?4",
        params![provider,json!({"message":error.to_string(),"statusCode":status_code}).to_string(),now(),generation])?;
    Ok(())
}

pub fn save_account(c: &Connection, provider: &str, value: &Value) -> Result<(), ManagerError> {
    let access = value["access_token"]
        .as_str()
        .or(value["tokens"]["access_token"].as_str())
        .or(value["auth"]["accessToken"].as_str())
        .unwrap_or("");
    let refresh = value["refresh_token"]
        .as_str()
        .or(value["tokens"]["refresh_token"].as_str())
        .or(value["auth"]["refreshToken"].as_str())
        .unwrap_or("");
    let identity = value["account_id"]
        .as_str()
        .or(value["tokens"]["account_id"].as_str())
        .or(value["accountId"].as_str())
        .unwrap_or("");
    let id_token = value["id_token"]
        .as_str()
        .or(value["tokens"]["id_token"].as_str())
        .or(value["auth"]["idToken"].as_str())
        .unwrap_or("");
    if access.is_empty() || identity.is_empty() {
        return Err(fail("官方账号缺少 access_token 或 account_id"));
    }
    let expires = value["expires_at"]
        .as_i64()
        .or(value["auth"]["expiresAt"].as_i64());
    let credentials = json!({"access_token":access,"refresh_token":refresh,"id_token":id_token});
    let changed = c.execute("UPDATE service_provider_configs SET account_id=?2,account_email=?3,
        credentials_ciphertext=?4,token_expires_at=?5,credential_generation=credential_generation+1,
        quota_json=CASE WHEN account_id=?2 THEN json_remove(quota_json,'$.refreshError') ELSE '{}' END,
        refresh_status='',account_updated_at=?6 WHERE id=?1 AND kind='codex_account' AND deleted_at IS NULL",
        params![provider,identity,text(value,"email"),encrypt_provider_key(&credentials.to_string())?,expires,now()])?;
    if changed != 1 {
        return Err(fail("官方账号不存在、类型不匹配或已删除"));
    }
    Ok(())
}

pub fn usage(c: &Connection) -> Result<Value, ManagerError> {
    let totals=query(c,"SELECT count(*) AS requestCount,
        sum(CASE WHEN state='completed' THEN 1 ELSE 0 END) AS successCount,
        sum(CASE WHEN state IN ('failed','rejected') THEN 1 ELSE 0 END) AS failureCount,
        sum(CASE WHEN state='unknown' THEN 1 ELSE 0 END) AS unknownCount,
        sum(CASE WHEN state='cancelled' THEN 1 ELSE 0 END) AS cancelledCount FROM service_gateway_requests",&[])?.pop().unwrap_or(json!({}));
    let attempts=query(c,"SELECT count(*) AS attemptCount,sum(CASE WHEN input_tokens IS NOT NULL AND output_tokens IS NOT NULL THEN 1 ELSE 0 END) AS measuredCount,
        sum(input_tokens) AS inputTokens,sum(output_tokens) AS outputTokens,sum(cached_input_tokens) AS cachedInputTokens
        FROM service_gateway_attempts",&[])?.pop().unwrap_or(json!({}));
    let logs=query(c,"SELECT r.id,r.created_at AS createdAt,r.agent_type AS agentType,r.instance_id AS instanceId,
        r.requested_model AS requestedModel,r.state,r.status_code AS statusCode,r.error_message AS errorMessage,
        a.provider_id AS providerId,a.provider_name AS providerName,a.upstream_model AS upstreamModel,a.credential_name AS credentialName,
        a.input_tokens AS inputTokens,a.output_tokens AS outputTokens,a.usage_source AS usageSource,a.estimated_cost_micros AS estimatedCostMicros,a.currency
        FROM service_gateway_requests r LEFT JOIN service_gateway_attempts a ON a.request_id=r.id ORDER BY r.created_at DESC LIMIT 100",&[])?;
    let mut result = json!({"requests":totals,"attempts":attempts,"logs":logs});
    for (key,identity,label) in [
        ("byProvider","a.provider_id","a.provider_name"),
        ("byKey","a.credential_id","a.provider_name || ' / ' || a.credential_name || ' / ' || substr(a.credential_fingerprint,1,8)"),
        ("byModel","a.provider_id || ':' || a.upstream_model","a.provider_name || ' / ' || a.upstream_model"),
        ("byAgent","r.agent_type","r.agent_type"),
        ("byToken","r.token_id","r.instance_id"),
        ("byDay","date(r.created_at/1000,'unixepoch','localtime')","date(r.created_at/1000,'unixepoch','localtime')")
    ] {
        result[key]=json!(query(c,&format!("SELECT {identity} AS groupKey,{label} AS groupName,a.currency,count(*) AS attemptCount,
            sum(CASE WHEN a.state='succeeded' THEN 1 ELSE 0 END) AS successCount,
            sum(CASE WHEN a.input_tokens IS NULL OR a.output_tokens IS NULL THEN 1 ELSE 0 END) AS unknownUsageCount,
            sum(a.input_tokens) AS inputTokens,sum(a.output_tokens) AS outputTokens,sum(a.cached_input_tokens) AS cachedInputTokens,
            count(a.estimated_cost_micros) AS pricedCount,sum(a.estimated_cost_micros) AS estimatedCostMicros
            FROM service_gateway_attempts a JOIN service_gateway_requests r ON r.id=a.request_id
            GROUP BY {identity},{label},a.currency ORDER BY groupKey"),&[])?);
    }
    Ok(result)
}

pub fn recover_requests(paths: &AppPaths) -> Result<(), ManagerError> {
    let c = database::open(paths)?;
    c.execute("UPDATE service_gateway_requests SET state='unknown',finished_at=?1,error_message='应用退出时请求尚未结束'
        WHERE finished_at IS NULL",[now()])?;
    c.execute("UPDATE service_gateway_attempts SET state='unknown',finished_at=?1,error_message='应用退出时上游结果未确认'
        WHERE finished_at IS NULL",[now()])?;
    Ok(())
}

pub(crate) fn period_usage(paths: &AppPaths, value: &Value) -> Result<Value, ManagerError> {
    let c = database::open(paths)?;
    let summary=query(&c,"SELECT count(*) AS attemptCount,
        count(CASE WHEN a.input_tokens IS NOT NULL AND a.output_tokens IS NOT NULL THEN 1 END) AS measuredCount,
        count(CASE WHEN a.input_tokens IS NULL OR a.output_tokens IS NULL THEN 1 END) AS unknownUsageCount,
        CASE WHEN count(*)=0 THEN 0 WHEN count(a.input_tokens)=count(*) AND count(a.output_tokens)=count(*)
        THEN sum(a.input_tokens+a.output_tokens) END AS actualTokens,
        CASE WHEN count(*)=0 THEN 0 WHEN count(a.estimated_cost_micros)=count(*) AND min(a.currency)='USD' AND max(a.currency)='USD'
        THEN sum(a.estimated_cost_micros)/1000000.0 END AS totalCostUsd
        FROM service_gateway_attempts a JOIN service_gateway_requests r ON r.id=a.request_id
        WHERE r.created_at>=?1 AND r.created_at<=?2 AND (?3='all' OR r.agent_type=?3)",
        &[&value["startAt"].as_i64().unwrap_or(0),&value["endAt"].as_i64().unwrap_or(i64::MAX),
            &value["appType"].as_str().unwrap_or("all")])?.pop().unwrap_or(json!({}));
    Ok(json!({"data":{"summary":summary}}))
}

pub use migration::migrate;

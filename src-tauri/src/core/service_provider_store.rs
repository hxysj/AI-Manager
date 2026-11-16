use crate::api::runtime_provider::{decrypt_provider_key, encrypt_provider_key};
use crate::core::{database, error::ManagerError, paths::AppPaths};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

mod migration;
#[cfg(test)]
mod tests;

pub const SCHEMA: &str = include_str!("service_provider_store/schema.sql");
pub const MODE_KEY: &str = "service_provider:active_backend";
pub const GENERATION_KEY: &str = "service_provider:mode_generation";
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
    let mut entries = LEGACY_REQUESTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| fail("请求登记不可用"))?;
    let entry = entries
        .entry(paths.storage_files.database.clone())
        .or_default();
    if entry.1 || active(paths)? {
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
        return Err(fail("旧模式仍有请求进行中，请结束请求后重试切换"));
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
    if version.as_deref().is_some_and(|v| v != "1") {
        return Err(fail("服务商配置数据库版本高于当前程序支持的版本"));
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(SCHEMA)?;
    transaction.execute("INSERT OR IGNORE INTO app_metadata(key,value) VALUES('service_provider:schema_version','1')", [])?;
    transaction.commit()?;
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
    let providers = query(&c, "SELECT id,name,kind,upstream_protocol AS upstreamProtocol,base_url AS baseUrl,proxy,
        note,enabled,revision,created_at AS createdAt,updated_at AS updatedAt,
        CASE WHEN kind='api' THEN EXISTS(SELECT 1 FROM service_provider_keys k WHERE k.provider_id=p.id AND k.is_active=1 AND k.status='active' AND k.secret_ciphertext<>'')
        ELSE EXISTS(SELECT 1 FROM service_provider_accounts a WHERE a.provider_id=p.id AND a.access_ciphertext<>'') END AS hasCredential
        FROM service_provider_configs p WHERE deleted_at IS NULL ORDER BY created_at,id", &[])?;
    let models = query(&c, "SELECT id,provider_id AS providerId,upstream_model_id AS upstreamModelId,display_name AS displayName,
        context_window AS contextWindow,catalog_json AS catalogJson,enabled,input_price_micros AS inputPriceMicros,
        cached_input_price_micros AS cachedInputPriceMicros,output_price_micros AS outputPriceMicros,currency FROM service_provider_models", &[])?;
    let bindings = query(&c, "SELECT provider_id AS providerId,agent_type AS agentType,enabled,default_model_id AS defaultModelId,options_json AS optionsJson,revision FROM service_provider_agent_bindings", &[])?;
    let keys = query(&c, "SELECT id,provider_id AS providerId,logical_id AS logicalId,name,note,is_current AS isCurrent,is_active AS isActive,status,substr(fingerprint,1,12) AS fingerprint FROM service_provider_keys WHERE is_current=1", &[])?;
    let instances = query(&c, "SELECT id,agent_type AS agentType,name,config_path AS configPath,agent_version AS agentVersion,
        client_protocol AS clientProtocol,provider_id AS providerId,model_id AS modelId,status,sync_status AS syncStatus,
        applied_revision AS appliedRevision,last_synced_at AS lastSyncedAt,last_error AS lastError,revision FROM service_agent_instances", &[])?;
    let migrations = query(&c, "SELECT m.id,m.status,m.created_at AS createdAt,m.completed_at AS completedAt,
        (SELECT count(*) FROM service_provider_migration_items i WHERE i.migration_id=m.id AND i.status='copied') AS copied,
        (SELECT count(*) FROM service_provider_migration_items i WHERE i.migration_id=m.id AND i.status='skipped') AS skipped,
        (SELECT count(*) FROM service_provider_migration_items i WHERE i.migration_id=m.id AND i.status='unreadable') AS unreadable
        FROM service_provider_migrations m ORDER BY m.created_at DESC LIMIT 10", &[])?;
    let migration_items = query(&c, "SELECT source_table AS sourceTable,source_id AS sourceId,target_id AS targetId,status,reason
        FROM service_provider_migration_items WHERE migration_id=(SELECT id FROM service_provider_migrations ORDER BY created_at DESC LIMIT 1)", &[])?;
    let compatibility = query(&c, "SELECT id,provider_id AS providerId,agent_type AS agentType,model_id AS modelId,
        provider_revision AS providerRevision,agent_version AS agentVersion,client_protocol AS clientProtocol,capability,status,evidence_json AS evidenceJson,tested_at AS testedAt
        FROM service_compatibility_tests ORDER BY tested_at DESC LIMIT 100", &[])?;
    let routes=query(&c,"SELECT id,provider_id AS providerId,agent_type AS agentType,client_model_name AS clientModelName,model_id AS modelId,enabled FROM service_agent_model_routes",&[])?;
    let accounts=query(&c,"SELECT provider_id AS providerId,email,external_account_id AS accountId,expires_at AS expiresAt,quota_json AS quotaJson,refresh_status AS refreshStatus FROM service_provider_accounts",&[])?;
    let files=query(&c,"SELECT id,instance_id AS instanceId,file_path AS filePath,write_status AS writeStatus,restore_status AS restoreStatus,
        error_message AS errorMessage FROM service_managed_files WHERE restore_status<>'restored'",&[])?;
    Ok(
        json!({"backend": if active(paths)? {"service"} else {"legacy"}, "generation":generation(&c)?,
        "baseUrl":format!("http://127.0.0.1:{PORT}"), "providers":providers,"models":models,"bindings":bindings,
        "keys":keys,"instances":instances,"migrations":migrations,"migrationItems":migration_items,
        "compatibility":compatibility,"routes":routes,"accounts":accounts,"managedFiles":files,"usage":usage(&c)?}),
    )
}

pub fn config(connection: &Connection, provider_id: &str) -> Result<Value, ManagerError> {
    query(connection, "SELECT id,name,kind,upstream_protocol AS upstreamProtocol,base_url AS baseUrl,proxy,
        headers_json AS headersJson,note,enabled,revision,deleted_at AS deletedAt FROM service_provider_configs WHERE id=?1", &[&provider_id])?
        .pop().ok_or_else(|| fail("服务商配置不存在"))
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
    let rows = query(&c, "SELECT p.id,p.name,p.kind,p.enabled,p.proxy,b.enabled AS authorized,
        a.external_account_id AS accountId,a.email,a.expires_at AS expiresAt,a.quota_json AS usageJson,
        CASE WHEN p.kind='api' THEN EXISTS(SELECT 1 FROM service_provider_keys k WHERE k.provider_id=p.id
            AND k.is_active=1 AND k.status='active' AND k.secret_ciphertext<>'')
        ELSE coalesce(a.access_ciphertext,'')<>'' END AS hasCredential
        FROM service_provider_configs p LEFT JOIN service_provider_agent_bindings b ON b.provider_id=p.id AND b.agent_type='codex'
        LEFT JOIN service_provider_accounts a ON a.provider_id=p.id WHERE p.deleted_at IS NULL ORDER BY p.created_at,p.id", &[])?;
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
        if row["kind"] == "api" {
            providers.push(json!({"id":row["id"],"name":row["name"],"cli":"codex","enabled":available,
                "runtimeConfig":{"mainModel":names.first(),"modelList":names},"serviceManaged":true}));
        } else {
            accounts.push(json!({"id":text(&row,"id").strip_prefix("account:").unwrap_or(text(&row,"id")),"serviceProviderId":row["id"],"type":"codex","email":row["email"],"accountId":row["accountId"],
                "name":row["name"],"proxy":row["proxy"],"disabled":!available,"requires_reauth":row["hasCredential"]!=1,
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
    if !matches!(kind, "api" | "codex_account")
        || !matches!(protocol, "chat" | "responses")
        || (kind == "codex_account" && protocol != "responses")
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
    if let Some(p) = &previous {
        if value["revision"].as_i64() != p["revision"].as_i64() {
            return Err(fail("配置已被修改，请刷新后重试"));
        }
        if text(p, "kind") != kind {
            return Err(fail("已有服务商不能改变凭据类型，请新建配置"));
        }
        tx.execute("UPDATE service_provider_configs SET name=?2,upstream_protocol=?3,base_url=?4,proxy=?5,headers_json=?6,
            note=?7,enabled=?8,revision=revision+1,updated_at=?9 WHERE id=?1",
            params![provider_id,name,protocol,text(value,"baseUrl"),text(value,"proxy"),headers.to_string(),text(value,"note"),value["enabled"].as_bool().unwrap_or(true),now()])?;
    } else {
        if !text(value, "id").is_empty() {
            return Err(fail("配置已删除或不存在"));
        }
        tx.execute("INSERT INTO service_provider_configs(id,name,kind,upstream_protocol,base_url,proxy,headers_json,note,enabled,created_at,updated_at)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",params![provider_id,name,kind,protocol,text(value,"baseUrl"),text(value,"proxy"),headers.to_string(),text(value,"note"),value["enabled"].as_bool().unwrap_or(true),now()])?;
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
            if !options.is_object() {
                return Err(fail("Agent 选项必须为对象"));
            }
            for (key, val) in options.as_object().unwrap() {
                let valid = agent == "codex"
                    && match key.as_str() {
                        "modelReasoningEffort" => {
                            val.is_null()
                                || matches!(
                                    val.as_str(),
                                    Some("minimal" | "low" | "medium" | "high" | "xhigh" | "none")
                                )
                        }
                        "serviceTierFast" | "modelContextWindowEnabled" => {
                            val.is_null() || val.is_boolean() || matches!(val.as_i64(), Some(0 | 1))
                        }
                        "modelAutoCompactTokenLimit" => {
                            val.is_null() || val.as_i64().is_some_and(|n| n > 0)
                        }
                        _ => false,
                    };
                if !valid {
                    return Err(fail(format!("{agent} 参数 {key} 尚不支持或值无效")));
                }
            }
            let default = binding["defaultModelId"].as_str().filter(|v| !v.is_empty());
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
    if previous.is_some() {
        tx.execute("UPDATE service_agent_instances SET sync_status='pending',last_error='服务商配置已更新，请解除接管后重新应用'
            WHERE provider_id=?1 AND status='active' AND EXISTS(SELECT 1 FROM service_managed_files f WHERE f.instance_id=service_agent_instances.id AND f.restore_status<>'restored')",[&provider_id])?;
    }
    tx.commit()?;
    Ok(json!({"id":provider_id}))
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
        "SELECT * FROM service_provider_accounts WHERE provider_id=?1",
        &[&provider],
    )?
    .pop()
    .ok_or_else(|| fail("官方账号凭据不存在"))?;
    for (source, target) in [
        ("access_ciphertext", "accessToken"),
        ("refresh_ciphertext", "refreshToken"),
        ("id_ciphertext", "idToken"),
    ] {
        let encrypted = text(&item, source);
        item[target] = json!(if encrypted.is_empty() {
            String::new()
        } else {
            decrypt_provider_key(encrypted)?
        });
        item.as_object_mut().unwrap().remove(source);
    }
    Ok(item)
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
    c.execute("INSERT INTO service_provider_accounts(id,provider_id,external_account_id,email,access_ciphertext,refresh_ciphertext,
        id_ciphertext,expires_at,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)
        ON CONFLICT(provider_id) DO UPDATE SET external_account_id=excluded.external_account_id,email=excluded.email,
        access_ciphertext=excluded.access_ciphertext,refresh_ciphertext=excluded.refresh_ciphertext,id_ciphertext=excluded.id_ciphertext,
        quota_json=CASE WHEN external_account_id=excluded.external_account_id THEN quota_json ELSE '{}' END,
        expires_at=excluded.expires_at,token_generation=token_generation+1,revision=revision+1,refresh_status='',updated_at=excluded.updated_at",
        params![id(),provider,identity,text(value,"email"),encrypt_provider_key(access)?,encrypt_provider_key(refresh)?,
            encrypt_provider_key(id_token)?,expires,now()])?;
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

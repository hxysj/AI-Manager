use crate::core::error::ManagerError;
use rusqlite::{types::Value as SqlValue, Connection, Row};
use serde_json::{json, Map, Value};

// 明确列出每个业务字段；JSON 仅用于具名的列表、请求头和外部额度对象。
pub(super) const FIELDS: &[(&str, &str, &str)] = &[
    ("name", "/name", "TEXT"),
    ("icon", "/icon", "TEXT"),
    ("note", "/note", "TEXT"),
    ("website", "/website", "TEXT"),
    ("base_url", "/baseUrl", "TEXT"),
    ("proxy", "/proxy", "TEXT"),
    ("auth_field", "/authField", "TEXT"),
    ("headers", "/headers", "JSON"),
    ("created_at", "/createdAt", "INTEGER"),
    ("updated_at", "/updatedAt", "INTEGER"),
    ("main_model", "/runtimeConfig/mainModel", "TEXT"),
    ("model_names", "/runtimeConfig/modelList", "JSON"),
    ("sync_model_list", "/runtimeConfig/syncModelList", "BOOL"),
    ("model_catalog", "/runtimeConfig/modelCatalog", "JSON"),
    ("haiku_model", "/runtimeConfig/haikuModel", "TEXT"),
    ("sonnet_model", "/runtimeConfig/sonnetModel", "TEXT"),
    ("opus_model", "/runtimeConfig/opusModel", "TEXT"),
    ("tool_search", "/runtimeConfig/toolSearch", "BOOL"),
    ("disable_upgrade", "/runtimeConfig/disableUpgrade", "BOOL"),
    (
        "hide_ai_signature",
        "/runtimeConfig/hideAiSignature",
        "BOOL",
    ),
    ("teammates_mode", "/runtimeConfig/teammatesMode", "BOOL"),
    ("max_thinking", "/runtimeConfig/maxThinking", "BOOL"),
    (
        "model_context_window_enabled",
        "/runtimeConfig/modelContextWindowEnabled",
        "BOOL",
    ),
    (
        "service_tier_fast",
        "/runtimeConfig/serviceTierFast",
        "BOOL",
    ),
    (
        "model_reasoning_effort",
        "/runtimeConfig/modelReasoningEffort",
        "TEXT",
    ),
    (
        "model_auto_compact_token_limit",
        "/runtimeConfig/modelAutoCompactTokenLimit",
        "INTEGER",
    ),
    ("mode", "/mode", "TEXT"),
    ("api_format", "/apiFormat", "TEXT"),
    ("routes", "/routes", "JSON"),
    ("account_kind", "/accountKind", "TEXT"),
    ("account_provider", "/provider", "TEXT"),
    ("account_id", "/accountId", "TEXT"),
    ("email", "/email", "TEXT"),
    ("plan", "/plan", "TEXT"),
    ("access_token", "/access_token", "TEXT"),
    ("refresh_token", "/refresh_token", "TEXT"),
    ("id_token", "/id_token", "TEXT"),
    ("auth_expires_at", "/auth/expiresAt", "INTEGER"),
    ("last_refresh", "/last_refresh", "TEXT"),
    ("expired", "/expired", "TEXT"),
    ("token_generation", "/token_generation", "INTEGER"),
    ("token_updated_at", "/token_updated_at", "INTEGER"),
    ("auto_refresh", "/autoRefresh", "BOOL"),
    ("refresh_status", "/refresh_status", "TEXT"),
    ("refresh_status_code", "/refresh_status_code", "INTEGER"),
    ("refresh_message", "/refresh_message", "TEXT"),
    ("requires_reauth", "/requires_reauth", "BOOL"),
    ("reauth_reason", "/reauth_reason", "TEXT"),
    ("reauth_message", "/reauth_message", "TEXT"),
    ("quota", "/usage", "JSON"),
    ("google_subject", "/google/subject", "TEXT"),
    ("google_email", "/google/email", "TEXT"),
    ("google_name", "/google/name", "TEXT"),
    ("google_picture", "/google/picture", "TEXT"),
    ("google_project", "/google/project", "TEXT"),
    ("google_plan", "/google/plan", "TEXT"),
    ("google_proxy", "/google/proxy", "TEXT"),
    ("google_port", "/google/port", "INTEGER"),
    ("google_status", "/google/status", "TEXT"),
    ("google_message", "/google/message", "TEXT"),
    ("google_updated_at", "/google/updatedAt", "INTEGER"),
    ("google_models", "/google/models", "JSON"),
    ("google_quota_groups", "/google/quotaGroups", "JSON"),
    ("google_quota_warning", "/google/quotaWarning", "TEXT"),
    (
        "google_deprecated_model_ids",
        "/google/deprecatedModelIds",
        "JSON",
    ),
];

pub(crate) const LOCAL_COLUMNS: &[&str] = &[
    "enabled",
    "is_active",
    "oauth_credentials",
    "profile_id",
    "selected_model",
    "profile_base_url",
    "profile_proxy",
    "profile_env",
    "profile_updated_at",
    "synced_hash",
    "synced_at",
    "runtime_path",
    "sync_status",
    "instance_profile_dir",
    "instance_sessions_path",
    "instance_updated_at",
    "quota",
];

pub(crate) fn local_default(column: &str) -> Value {
    match column {
        "enabled" | "is_active" | "synced_at" | "instance_updated_at" => json!(0),
        "sync_status" => json!("NO_ACTIVE"),
        "profile_id"
        | "oauth_credentials"
        | "synced_hash"
        | "runtime_path"
        | "instance_profile_dir"
        | "instance_sessions_path" => json!(""),
        _ => Value::Null,
    }
}

pub(super) fn create(connection: &Connection) -> Result<(), ManagerError> {
    let columns = FIELDS
        .iter()
        .map(|(name, _, kind)| {
            format!(
                "{name} {}",
                match *kind {
                    "BOOL" => "INTEGER",
                    "JSON" => "TEXT",
                    other => other,
                }
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    connection.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS providers (
         id TEXT PRIMARY KEY NOT NULL, cli TEXT NOT NULL DEFAULT '', type TEXT NOT NULL DEFAULT 'custom',
         sort_order INTEGER NOT NULL DEFAULT 0,
         enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
         is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0,1)),
         {columns},
         oauth_credentials TEXT NOT NULL DEFAULT '',
         models TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(models) AND json_type(models)='array'),
         profile_id TEXT NOT NULL DEFAULT '', selected_model TEXT, profile_base_url TEXT,
         profile_proxy TEXT, profile_env TEXT, profile_updated_at INTEGER,
         synced_hash TEXT NOT NULL DEFAULT '', synced_at INTEGER NOT NULL DEFAULT 0,
         runtime_path TEXT NOT NULL DEFAULT '', sync_status TEXT NOT NULL DEFAULT 'NO_ACTIVE',
         instance_profile_dir TEXT NOT NULL DEFAULT '', instance_sessions_path TEXT NOT NULL DEFAULT '',
         instance_updated_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_providers_cli_type ON providers(cli,type,sort_order);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_providers_active_cli ON providers(cli) WHERE is_active=1;
         CREATE TABLE IF NOT EXISTS provider_keys (
           provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
           id TEXT NOT NULL, name TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT '',
           value TEXT NOT NULL, is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0,1)),
           sort_order INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(provider_id,id)
         );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_keys_active ON provider_keys(provider_id) WHERE is_active=1;"
    ))?;
    Ok(())
}

pub(super) fn sql_value(value: Option<&Value>, kind: &str) -> Result<SqlValue, ManagerError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(SqlValue::Null);
    };
    Ok(match kind {
        "JSON" => SqlValue::Text(serde_json::to_string(value)?),
        "BOOL" => SqlValue::Integer(i64::from(
            value
                .as_bool()
                .ok_or_else(|| ManagerError::System("供应商布尔字段格式无效".into()))?,
        )),
        "INTEGER" => SqlValue::Integer(
            value
                .as_i64()
                .ok_or_else(|| ManagerError::System("供应商整数字段格式无效".into()))?,
        ),
        _ => SqlValue::Text(
            value
                .as_str()
                .ok_or_else(|| ManagerError::System("供应商文本字段格式无效".into()))?
                .to_string(),
        ),
    })
}

pub(crate) fn item_from_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let kind: String = row.get("type")?;
    let cli: String = row.get("cli")?;
    let storage_id: String = row.get("id")?;
    let id = if kind == "codex-account" {
        storage_id.strip_prefix("account:").unwrap_or(&storage_id)
    } else if cli == "claude-desktop" {
        storage_id
            .strip_prefix("claude-desktop:")
            .unwrap_or(&storage_id)
    } else {
        &storage_id
    };
    let mut item = json!({"id":id});
    for (column, path, format) in FIELDS {
        let raw: SqlValue = row.get(*column)?;
        let value = match raw {
            SqlValue::Null => continue,
            SqlValue::Text(text) if *format == "JSON" => {
                serde_json::from_str(&text).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
            }
            SqlValue::Text(text) => json!(text),
            SqlValue::Integer(number) if *format == "BOOL" => json!(number != 0),
            SqlValue::Integer(number) => json!(number),
            SqlValue::Real(number) => json!(number),
            SqlValue::Blob(_) => {
                return Err(rusqlite::Error::InvalidColumnType(
                    0,
                    column.to_string(),
                    rusqlite::types::Type::Blob,
                ))
            }
        };
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() == 1 {
            item[parts[0]] = value;
        } else {
            if !item[parts[0]].is_object() {
                item[parts[0]] = Value::Object(Map::new());
            }
            item[parts[0]][parts[1]] = value;
        }
    }
    if kind == "codex-account" {
        let account_kind = item
            .as_object_mut()
            .unwrap()
            .remove("accountKind")
            .unwrap_or(json!("codex"));
        item["type"] = account_kind;
        item["disabled"] = json!(row.get::<_, i64>("enabled")? == 0);
        if let Some(account_id) = item.get("accountId").cloned() {
            item["account_id"] = account_id;
        }
        for (token, alias) in [
            ("access_token", "accessToken"),
            ("refresh_token", "refreshToken"),
            ("id_token", "idToken"),
            ("last_refresh", "last_refresh"),
            ("expired", "expired"),
            ("token_updated_at", "token_updated_at"),
            ("token_generation", "token_generation"),
        ] {
            if let Some(value) = item.get(token).cloned() {
                if !item["auth"].is_object() {
                    item["auth"] = json!({});
                }
                item["auth"][token] = value.clone();
                item["auth"][alias] = value;
            }
        }
    } else {
        item["cli"] = json!(cli);
        item["type"] = json!(kind);
        item["enabled"] = json!(row.get::<_, i64>("enabled")? != 0);
    }
    Ok(item)
}

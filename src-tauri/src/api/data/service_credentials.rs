use super::{
    decrypt_backup_data, provider_key_preview_content, AppPaths, ManagerError, Map, Value,
};
use crate::api::runtime_provider::decrypt_provider_key;
use crate::core::{database, service_provider_store as store};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub(super) const FIELD: &str = "serviceProviderCredentials";

#[derive(Deserialize, Serialize)]
pub(super) struct Credential {
    provider_id: String,
    provider_name: String,
    active: bool,
    #[serde(flatten)]
    data: CredentialData,
}

#[derive(Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CredentialData {
    Api {
        logical_id: String,
        name: String,
        note: String,
        secret: Option<String>,
    },
    Official {
        account_kind: String,
        identity: String,
        email: String,
        expires_at: Option<i64>,
        project: String,
        tokens: Value,
    },
}

impl Credential {
    fn item_id(&self) -> &str {
        match &self.data {
            CredentialData::Api { logical_id, .. } => logical_id,
            CredentialData::Official { .. } => "official",
        }
    }

    fn choice_key(&self) -> String {
        let category = match self.data {
            CredentialData::Api { .. } => "api-key",
            CredentialData::Official { .. } => "official-account",
        };
        format!(
            "service-credential:{}",
            json!([self.provider_id, category, self.item_id()])
        )
    }

    fn preview_content(&self) -> Result<String, ManagerError> {
        match &self.data {
            CredentialData::Api {
                name,
                note,
                secret: Some(secret),
                ..
            } => {
                provider_key_preview_content(&json!({"名称": name, "备注": note, "apiKey": secret}))
            }
            CredentialData::Api { secret: None, .. } => Ok("本机密钥已撤销或不可用".into()),
            CredentialData::Official {
                tokens, expires_at, ..
            } => {
                let fingerprint = super::sha256_text(&serde_json::to_string(tokens)?);
                Ok(serde_json::to_string_pretty(&json!({
                    "凭据指纹": &fingerprint[..12], "过期时间": expires_at,
                }))?)
            }
        }
    }
}

pub(super) fn decode(backup: &Value) -> Result<Vec<Credential>, ManagerError> {
    let Some(encrypted) = backup.get(FIELD) else {
        return Ok(Vec::new());
    };
    let encrypted = encrypted
        .as_str()
        .ok_or_else(|| store::fail("服务商凭据备份格式不正确"))?;
    Ok(serde_json::from_value(decrypt_backup_data(encrypted)?)?)
}

pub(super) fn collect(paths: &AppPaths) -> Result<Vec<Credential>, ManagerError> {
    read(&database::open(paths)?, false)
}

fn read(c: &Connection, include_unavailable: bool) -> Result<Vec<Credential>, ManagerError> {
    let mut credentials = Vec::new();
    for row in store::query(
        c,
        "SELECT k.provider_id AS providerId,p.name AS providerName,k.logical_id AS logicalId,
        k.name,k.note,k.is_active AS active,k.secret_ciphertext AS ciphertext,k.status
        FROM service_provider_keys k JOIN service_provider_configs p ON p.id=k.provider_id
        WHERE p.kind='api' AND p.deleted_at IS NULL AND k.is_current=1
        ORDER BY k.provider_id,k.logical_id",
        &[],
    )? {
        let ciphertext = store::text(&row, "ciphertext");
        let available = row["status"] == "active" && !ciphertext.is_empty();
        if !available && !include_unavailable {
            continue;
        }
        credentials.push(Credential {
            provider_id: store::text(&row, "providerId").into(),
            provider_name: store::text(&row, "providerName").into(),
            active: row["active"] == 1,
            data: CredentialData::Api {
                logical_id: store::text(&row, "logicalId").into(),
                name: store::text(&row, "name").into(),
                note: store::text(&row, "note").into(),
                secret: available
                    .then(|| decrypt_provider_key(ciphertext))
                    .transpose()?,
            },
        });
    }
    for row in store::query(
        c,
        "SELECT id,name,kind,account_id AS identity,account_email AS email,
        token_expires_at AS expiresAt,credentials_ciphertext AS ciphertext,
        json_extract(account_metadata_json,'$.project') AS project FROM service_provider_configs
        WHERE kind IN ('codex_account','google_account') AND deleted_at IS NULL
        AND credentials_ciphertext<>'' ORDER BY id",
        &[],
    )? {
        credentials.push(Credential {
            provider_id: store::text(&row, "id").into(),
            provider_name: store::text(&row, "name").into(),
            active: false,
            data: CredentialData::Official {
                account_kind: store::text(&row, "kind").into(),
                identity: store::text(&row, "identity").into(),
                email: store::text(&row, "email").into(),
                expires_at: row["expiresAt"].as_i64(),
                project: store::text(&row, "project").into(),
                tokens: serde_json::from_str(&decrypt_provider_key(store::text(
                    &row,
                    "ciphertext",
                ))?)?,
            },
        });
    }
    Ok(credentials)
}

pub(super) fn append_preview(
    paths: &AppPaths,
    backup: &[Credential],
    preview: &mut Value,
) -> Result<(), ManagerError> {
    if backup.is_empty() {
        return Ok(());
    }
    let c = database::open(paths)?;
    let current = read(&c, true)?;
    for credential in backup {
        let key = credential.choice_key();
        let existing = current.iter().find(|item| item.choice_key() == key);
        if existing.is_some_and(|item| item.data == credential.data)
            || linked_to_another_provider(&c, credential)?
        {
            continue;
        }
        let (category, name) = match &credential.data {
            CredentialData::Api { name, .. } => ("API Key", name.as_str()),
            CredentialData::Official { account_kind, .. } if account_kind == "google_account" => {
                ("Google 登录凭据", "")
            }
            CredentialData::Official { .. } => ("Codex 登录凭据", ""),
        };
        let title = if name.is_empty() {
            credential.provider_name.clone()
        } else {
            format!("{} / {name}", credential.provider_name)
        };
        let group_path = format!("service-providers/{}/credentials", credential.provider_id);
        let item = json!({
            "key": key, "type": category, "name": title,
            "path": format!("{group_path}/{}", credential.item_id()), "groupPath": group_path,
            "status": if existing.is_some() {"conflict"} else {"added"},
            "currentContent": existing.map(Credential::preview_content).transpose()?.unwrap_or_default(),
            "backupContent": credential.preview_content()?,
        });
        preview[if existing.is_some() {
            "conflicts"
        } else {
            "added"
        }]
        .as_array_mut()
        .unwrap()
        .push(item);
    }
    preview["addedCount"] = json!(preview["added"].as_array().unwrap().len());
    preview["conflictCount"] = json!(preview["conflicts"].as_array().unwrap().len());
    Ok(())
}

fn linked_to_another_provider(
    c: &Connection,
    credential: &Credential,
) -> Result<bool, ManagerError> {
    let CredentialData::Official {
        account_kind,
        identity,
        ..
    } = &credential.data
    else {
        return Ok(false);
    };
    Ok(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM service_provider_configs
         WHERE kind=?1 AND account_id=?2 AND account_id<>'' AND id<>?3
         AND (deleted_at IS NULL OR kind='google_account'))",
        params![account_kind, identity, credential.provider_id],
        |row| row.get(0),
    )?)
}

pub(super) fn merge(
    c: &Connection,
    backup: &[Credential],
    choices: &Map<String, Value>,
) -> Result<(), ManagerError> {
    if backup.is_empty() {
        return Ok(());
    }
    let current = read(c, true)?;
    for credential in backup {
        let key = credential.choice_key();
        let provider_id = &credential.provider_id;
        let existing = current.iter().find(|item| item.choice_key() == key);
        let use_backup = super::choice_text(choices, &key) == "backup"
            || super::choice_text(choices, "*") == "backup";
        if linked_to_another_provider(c, credential)?
            || existing.is_some_and(|item| item.data == credential.data || !use_backup)
        {
            continue;
        }
        let kind: Option<String> = c
            .query_row(
                "SELECT kind FROM service_provider_configs WHERE id=?1 AND deleted_at IS NULL",
                [provider_id],
                |row| row.get(0),
            )
            .optional()?;
        match (&credential.data, kind.as_deref()) {
            (
                CredentialData::Api {
                    logical_id,
                    name,
                    note,
                    secret: Some(secret),
                },
                Some("api"),
            ) => {
                let has_active = current
                    .iter()
                    .any(|item| item.provider_id == *provider_id && item.active);
                let activate =
                    existing.is_some_and(|item| item.active) || (!has_active && credential.active);
                store::save_key_version(c, provider_id, logical_id, name, secret, activate)?;
                c.execute("UPDATE service_provider_keys SET note=?3 WHERE provider_id=?1 AND logical_id=?2 AND is_current=1",
                    params![provider_id, logical_id, note])?;
            }
            (
                CredentialData::Official {
                    account_kind,
                    identity,
                    email,
                    expires_at,
                    tokens,
                    ..
                },
                Some("codex_account"),
            ) if account_kind == "codex_account" => {
                let mut login = tokens.clone();
                if !login.is_object() {
                    return Err(store::fail("Codex 登录凭据备份格式不正确"));
                }
                login["account_id"] = json!(identity);
                login["email"] = json!(email);
                login["expires_at"] = json!(expires_at);
                store::save_account(c, provider_id, &login)?;
            }
            (
                CredentialData::Official {
                    account_kind,
                    identity,
                    email,
                    project,
                    tokens,
                    ..
                },
                Some("google_account"),
            ) if account_kind == "google_account" => {
                let mut metadata = store::google_provider(c, provider_id)?["google"].clone();
                let port = match metadata["port"].as_u64() {
                    Some(port) => port,
                    None => u64::from(
                        std::net::TcpListener::bind("127.0.0.1:0")?
                            .local_addr()?
                            .port(),
                    ),
                };
                metadata["subject"] = json!(identity);
                metadata["email"] = json!(email);
                metadata["port"] = json!(port);
                if !project.is_empty() {
                    metadata["project"] = json!(project);
                }
                store::save_google_credentials(c, provider_id, &metadata, tokens)?;
            }
            _ => {}
        }
    }
    Ok(())
}

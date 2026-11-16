use super::*;

// 复用原 Google 协议适配器的数据形状，存储和凭据全部来自新模式表。
pub(crate) fn google_provider(c: &Connection, provider: &str) -> Result<Value, ManagerError> {
    let config = config(c, provider)?;
    if config["kind"] != "google_account" || !config["deletedAt"].is_null() {
        return Err(fail("Google 账号不存在或已删除"));
    }
    let row = query(c, "SELECT account_id AS subject,account_email AS email,google_gateway_port AS port,
        account_metadata_json AS metadataJson FROM service_provider_configs WHERE id=?1", &[&provider])?
        .pop().unwrap_or(json!({}));
    let mut metadata = row["metadata"].as_object().cloned().unwrap_or_default();
    for key in ["subject", "email", "port"] {
        if !row[key].is_null() { metadata.insert(key.into(), row[key].clone()); }
    }
    metadata.insert("proxy".into(), config["proxy"].clone());
    Ok(json!({"id":provider,"type":"google-account","cli":"codex","name":config["name"],
        "icon":config["icon"],"enabled":config["enabled"]==1,"proxy":config["proxy"],
        "baseUrl":config["baseUrl"],"google":metadata,"runtimeConfig":{}}))
}

pub(crate) fn google_tokens(c: &Connection, provider: &str) -> Result<Value, ManagerError> {
    Ok(google_auth(c, provider)?.0)
}

pub(crate) fn google_auth(c: &Connection, provider: &str) -> Result<(Value, i64), ManagerError> {
    let (ciphertext, generation): (String, i64) = c.query_row(
        "SELECT credentials_ciphertext,credential_generation FROM service_provider_configs
        WHERE id=?1 AND kind='google_account' AND deleted_at IS NULL AND credentials_ciphertext<>''",
        [provider], |row| Ok((row.get(0)?, row.get(1)?))).optional()?
        .ok_or_else(|| fail("Google 授权信息不存在，请重新登录"))?;
    Ok((serde_json::from_str(&decrypt_provider_key(&ciphertext)?)?, generation))
}

pub(crate) fn google_local_key(c: &Connection, provider: &str) -> Result<String, ManagerError> {
    let ciphertext: String = c.query_row(
        "SELECT google_local_key_ciphertext FROM service_provider_configs
        WHERE id=?1 AND kind='google_account' AND deleted_at IS NULL AND google_local_key_ciphertext<>''",
        [provider], |row| row.get(0))?;
    decrypt_provider_key(&ciphertext)
}

pub(crate) fn save_google_credentials(
    c: &Connection,
    provider: &str,
    metadata: &Value,
    tokens: &Value,
) -> Result<(), ManagerError> {
    let config = config(c, provider)?;
    if config["kind"] != "google_account" || !config["deletedAt"].is_null()
        || text(metadata, "subject").is_empty()
        || text(tokens, "access_token").is_empty()
        || text(tokens, "refresh_token").is_empty() {
        return Err(fail("Google 账号或授权信息无效"));
    }
    let port = metadata["port"].as_u64().filter(|port| (1..=65535).contains(port))
        .ok_or_else(|| fail("Google 本地转发端口无效"))?;
    if c.query_row("SELECT EXISTS(SELECT 1 FROM service_provider_configs WHERE kind='google_account' AND account_id=?1 AND id<>?2)",
        params![text(metadata, "subject"),provider], |row| row.get::<_, bool>(0))? {
        return Err(fail("此 Google 账号已存在，请选择已有账号"));
    }
    let changed = c.execute("UPDATE service_provider_configs SET account_id=?2,account_email=?3,credentials_ciphertext=?4,
        google_local_key_ciphertext=CASE WHEN account_id=?2 AND google_local_key_ciphertext<>'' THEN google_local_key_ciphertext ELSE ?5 END,
        token_expires_at=?6,google_gateway_port=?7,account_metadata_json=?8,
        credential_generation=credential_generation+1,refresh_status='',account_updated_at=?9
        WHERE id=?1 AND kind='google_account' AND deleted_at IS NULL",
        params![provider,text(metadata,"subject"),text(metadata,"email"),encrypt_provider_key(&tokens.to_string())?,
            encrypt_provider_key(&id())?,tokens["expires_at"].as_i64().and_then(|seconds| seconds.checked_mul(1000)),port,metadata.to_string(),now()])?;
    if changed != 1 {
        return Err(fail("Google 账号不存在或已删除"));
    }
    Ok(())
}

pub(crate) fn save_google_tokens(paths: &AppPaths, provider: &str, tokens: &Value, expected_generation: i64) -> Result<(), ManagerError> {
    let c = database::open(paths)?;
    let changed = c.execute("UPDATE service_provider_configs SET credentials_ciphertext=?2,token_expires_at=?3,
        credential_generation=credential_generation+1,refresh_status='',account_updated_at=?4
        WHERE id=?1 AND kind='google_account' AND deleted_at IS NULL AND enabled=1 AND credential_generation=?5",
        params![provider,encrypt_provider_key(&tokens.to_string())?,tokens["expires_at"].as_i64().and_then(|seconds| seconds.checked_mul(1000)),now(),expected_generation])?;
    if changed != 1 {
        return Err(fail("刷新期间 Google 账号已更新、禁用或删除，请重试"));
    }
    Ok(())
}

pub(crate) fn update_google_metadata(paths: &AppPaths, provider: &str, patch: &Value) -> Result<(), ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut metadata = google_provider(&tx, provider)?["google"].clone();
    for (key, value) in patch.as_object().ok_or_else(|| fail("Google 元数据无效"))? {
        metadata[key] = value.clone();
    }
    tx.execute("UPDATE service_provider_configs SET account_metadata_json=?2,
        account_updated_at=?3 WHERE id=?1", params![provider,metadata.to_string(),now()])?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn sync_google_models(paths: &AppPaths, provider: &str, models: &[Value]) -> Result<(), ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let config = config(&tx, provider)?;
    if config["kind"] != "google_account" || !config["deletedAt"].is_null() {
        return Err(fail("Google 账号已删除"));
    }
    tx.execute("UPDATE service_provider_models SET enabled=0 WHERE provider_id=?1", [provider])?;
    for model in models {
        save_model(&tx, provider, &json!({"upstreamModelId":model["id"],"displayName":model["displayName"],
            "contextWindow":model["maxTokens"].as_i64().filter(|size|*size>0),"catalog":{},"enabled":true}))?;
    }
    tx.execute("UPDATE service_provider_configs SET revision=revision+1,updated_at=?2 WHERE id=?1",
        params![provider,now()])?;
    tx.commit()?;
    Ok(())
}

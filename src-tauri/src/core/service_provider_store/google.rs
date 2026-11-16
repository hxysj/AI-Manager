use super::*;

// 复用原 Google 协议适配器的数据形状，存储和凭据全部来自新模式表。
pub(crate) fn google_provider(c: &Connection, provider: &str) -> Result<Value, ManagerError> {
    let config = config(c, provider)?;
    if config["kind"] != "google_account" || !config["deletedAt"].is_null() {
        return Err(fail("Google 账号不存在或已删除"));
    }
    let row = query(c, "SELECT external_subject AS subject,email,gateway_port AS port,
        metadata_json AS metadataJson FROM service_google_accounts WHERE provider_id=?1", &[&provider])?
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
    let ciphertext: String = c.query_row(
        "SELECT oauth_ciphertext FROM service_google_accounts WHERE provider_id=?1",
        [provider], |row| row.get(0)).optional()?
        .ok_or_else(|| fail("Google 授权信息不存在，请重新登录"))?;
    Ok(serde_json::from_str(&decrypt_provider_key(&ciphertext)?)?)
}

pub(crate) fn google_local_key(c: &Connection, provider: &str) -> Result<String, ManagerError> {
    let ciphertext: String = c.query_row(
        "SELECT local_key_ciphertext FROM service_google_accounts WHERE provider_id=?1",
        [provider], |row| row.get(0))?;
    decrypt_provider_key(&ciphertext)
}

pub(crate) fn save_google_credentials(
    c: &Connection,
    provider: &str,
    metadata: &Value,
    tokens: &Value,
) -> Result<(), ManagerError> {
    if config(c, provider)?["kind"] != "google_account"
        || text(metadata, "subject").is_empty()
        || text(tokens, "access_token").is_empty()
        || text(tokens, "refresh_token").is_empty() {
        return Err(fail("Google 账号或授权信息无效"));
    }
    let port = metadata["port"].as_u64().filter(|port| (1..=65535).contains(port))
        .ok_or_else(|| fail("Google 本地转发端口无效"))?;
    if c.query_row("SELECT EXISTS(SELECT 1 FROM service_google_accounts WHERE external_subject=?1 AND provider_id<>?2)",
        params![text(metadata, "subject"),provider], |row| row.get::<_, bool>(0))? {
        return Err(fail("此 Google 账号已存在，请选择已有账号"));
    }
    c.execute("INSERT INTO service_google_accounts(provider_id,external_subject,email,oauth_ciphertext,
        local_key_ciphertext,expires_at,gateway_port,metadata_json,created_at,updated_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9) ON CONFLICT(provider_id) DO UPDATE SET
        external_subject=excluded.external_subject,email=excluded.email,oauth_ciphertext=excluded.oauth_ciphertext,
        expires_at=excluded.expires_at,gateway_port=excluded.gateway_port,metadata_json=excluded.metadata_json,
        token_generation=token_generation+1,revision=revision+1,updated_at=excluded.updated_at",
        params![provider,text(metadata,"subject"),text(metadata,"email"),encrypt_provider_key(&tokens.to_string())?,
            encrypt_provider_key(&id())?,tokens["expires_at"].as_i64(),port,metadata.to_string(),now()])?;
    Ok(())
}

pub(crate) fn save_google_tokens(paths: &AppPaths, provider: &str, tokens: &Value) -> Result<(), ManagerError> {
    let c = database::open(paths)?;
    let config = config(&c, provider)?;
    if config["kind"] != "google_account" || !config["deletedAt"].is_null() {
        return Err(fail("Google 账号已删除"));
    }
    c.execute("UPDATE service_google_accounts SET oauth_ciphertext=?2,expires_at=?3,
        token_generation=token_generation+1,revision=revision+1,updated_at=?4 WHERE provider_id=?1",
        params![provider,encrypt_provider_key(&tokens.to_string())?,tokens["expires_at"].as_i64(),now()])?;
    Ok(())
}

pub(crate) fn update_google_metadata(paths: &AppPaths, provider: &str, patch: &Value) -> Result<(), ManagerError> {
    let c = database::open(paths)?;
    let mut metadata = google_provider(&c, provider)?["google"].clone();
    for (key, value) in patch.as_object().ok_or_else(|| fail("Google 元数据无效"))? {
        metadata[key] = value.clone();
    }
    c.execute("UPDATE service_google_accounts SET metadata_json=?2,revision=revision+1,
        updated_at=?3 WHERE provider_id=?1", params![provider,metadata.to_string(),now()])?;
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

use crate::api::{
    claude_desktop, codex_account, codex_models, multi_protocol_gateway, runtime_provider,
};
use crate::core::{
    database, error::ManagerError, paths::AppPaths, service_provider_store as store,
};
use rusqlite::{params, TransactionBehavior};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub async fn dispatch(
    paths: &AppPaths,
    targets: &Value,
    action: &str,
    value: Value,
) -> Result<Value, ManagerError> {
    store::initialize(paths)?;
    match action {
        "state" => return store::state(paths),
        "period-usage" => return store::period_usage(paths, &value),
        "migrate" => {
            store::migrate(paths)?;
        }
        "save" => {
            return store::save(paths, &value);
        }
        "delete" => {
            store::remove(
                paths,
                store::text(&value, "id"),
                value["revision"].as_i64().unwrap_or(0),
            )?;
        }
        "detail" => return store::config(&database::open(paths)?, store::text(&value, "id")),
        "file-backup" => {
            let c = database::open(paths)?;
            let file=store::query(&c,"SELECT file_path,before_exists,before_ciphertext,after_ciphertext FROM service_managed_files WHERE id=?1",
                &[&store::text(&value,"id")])?.pop().ok_or_else(||store::fail("文件备份不存在"))?;
            return Ok(
                json!({"path":file["file_path"],"beforeExists":file["before_exists"],
                "before":runtime_provider::decrypt_provider_key(store::text(&file,"before_ciphertext"))?,
                "after":runtime_provider::decrypt_provider_key(store::text(&file,"after_ciphertext"))?}),
            );
        }
        "import-account" => {
            let provider = store::text(&value, "id");
            let c = database::open(paths)?;
            if store::config(&c, provider)?["kind"] != "codex_account" {
                return Err(store::fail("请选择官方账号服务商"));
            }
            codex_account::import_service_account(paths, provider, store::text(&value, "content"))?;
        }
        "key-select" | "key-revoke" => {
            let mut c = database::open(paths)?;
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let provider = store::text(&value, "providerId");
            let key = store::text(&value, "id");
            if action == "key-select" {
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM service_provider_keys WHERE id=?1 AND provider_id=?2
                    AND is_current=1 AND status='active' AND secret_ciphertext<>'')",params![key,provider],|r|r.get(0))?;
                if !exists {
                    return Err(store::fail("Key 不可用"));
                }
                tx.execute(
                    "UPDATE service_provider_keys SET is_active=0 WHERE provider_id=?1",
                    [provider],
                )?;
                tx.execute(
                    "UPDATE service_provider_keys SET is_active=1 WHERE id=?1",
                    [key],
                )?;
            } else {
                tx.execute("UPDATE service_provider_keys SET is_active=0,status='revoked',secret_ciphertext='',updated_at=?3
                    WHERE id=?1 AND provider_id=?2",params![key,provider,store::now()])?;
            }
            tx.execute(
                "UPDATE service_provider_configs SET revision=revision+1,updated_at=?2 WHERE id=?1",
                params![provider, store::now()],
            )?;
            tx.commit()?;
        }
        "activate" => return activate(paths, targets, &value),
        "quick-switch" => return quick_switch(paths, targets, &value),
        "issue-token" => return issue_token(paths, &value),
        "refresh-quota" => {
            return codex_account::refresh_service_quota(paths, store::text(&value, "providerId"))
                .await
        }
        "detach" => {
            detach(paths, store::text(&value, "id"))?;
        }
        "fetch-models" => {
            return multi_protocol_gateway::service::fetch_models(paths, &value).await
        }
        "test" => return multi_protocol_gateway::service::test(paths, &value).await,
        _ => {
            return Err(ManagerError::UnknownChannel(format!(
                "service-provider:{action}"
            )))
        }
    }
    store::state(paths)
}

// 托盘只切换默认目录；多个独立目录继续由配置中心逐个管理。
pub(crate) fn quick_switch(
    paths: &AppPaths,
    targets: &Value,
    value: &Value,
) -> Result<Value, ManagerError> {
    if !store::active(paths)? {
        return Err(store::fail("请先启用服务商配置中心"));
    }
    let agent = store::text(value, "agentType");
    if !["codex", "claude", "claude-desktop"].contains(&agent) {
        return Err(store::fail("Agent 类型无效"));
    }
    let provider = store::text(value, "providerId");
    let c = database::open(paths)?;
    let model = store::query(
        &c,
        "SELECT m.id FROM service_provider_configs p JOIN service_provider_agent_bindings b
        ON b.provider_id=p.id AND b.agent_type=?2 AND b.enabled=1 JOIN service_provider_models m
        ON m.provider_id=p.id AND m.enabled=1 WHERE p.id=?1 AND p.enabled=1 AND p.deleted_at IS NULL
        ORDER BY CASE WHEN m.id=b.default_model_id THEN 0 ELSE 1 END,m.created_at,m.id LIMIT 1",
        &[&provider, &agent],
    )?
    .pop()
    .ok_or_else(|| store::fail("服务商未授权此 Agent 或没有可用模型"))?;
    if store::config(&c, provider)?["kind"] == "api" {
        store::credential(&c, provider)?;
    } else {
        store::account(&c, provider)?;
    }
    let directory = targets
        .as_array()
        .into_iter()
        .flatten()
        .find(|v| v["id"] == agent)
        .map(|v| store::text(v, "configPath"))
        .unwrap_or("");
    let path = if agent == "claude-desktop" {
        String::new()
    } else {
        normalized(Path::new(directory))?
    };
    for instance in store::query(
        &c,
        "SELECT id FROM service_agent_instances WHERE agent_type=?1 AND status<>'detached'
        AND (config_path=?2 OR ?1='claude-desktop')",
        &[&agent, &path],
    )? {
        detach(paths, store::text(&instance, "id"))?;
    }
    activate(
        paths,
        targets,
        &json!({"agentType":agent,"providerId":provider,"modelId":model["id"],"configPath":directory}),
    )
}

pub(crate) fn issue_token(paths: &AppPaths, value: &Value) -> Result<Value, ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let enabled:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1 AND value='service')
        AND NOT EXISTS(SELECT 1 FROM app_metadata WHERE key='service_provider:accepting' AND value='0')",[store::MODE_KEY],|r|r.get(0))?;
    if !enabled {
        return Err(store::fail("共享网关未启用或正在切换"));
    }
    let provider = store::text(value, "providerId");
    let model = store::text(value, "modelId");
    let allowed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM service_provider_configs p
        JOIN service_provider_agent_bindings b ON b.provider_id=p.id AND b.agent_type='gateway' AND b.enabled=1
        JOIN service_provider_models m ON m.provider_id=p.id AND m.id=?2 AND m.enabled=1
        WHERE p.id=?1 AND p.enabled=1 AND p.deleted_at IS NULL)",params![provider,model],|r|r.get(0))?;
    if !allowed {
        return Err(store::fail("服务商或模型不可用，请先授权多协议工具"));
    }
    let instance = store::id();
    let token = format!("sk-tool-{}{}", store::id(), store::id());
    tx.execute("INSERT INTO service_agent_instances(id,agent_type,name,config_path,client_protocol,provider_id,model_id,status,sync_status,created_at,updated_at)
        VALUES(?1,'gateway',?2,?3,'all',?4,?5,'active','synced',?6,?6)",params![instance,
            value["name"].as_str().filter(|v|!v.trim().is_empty()).unwrap_or("多协议工具"),format!("external:{instance}"),provider,model,store::now()])?;
    tx.execute("INSERT INTO service_gateway_tokens(id,token_hash,token_prefix,instance_id,created_at) VALUES(?1,?2,?3,?4,?5)",
        params![store::id(),store::hash(&token),&token[..14],instance,store::now()])?;
    tx.commit()?;
    Ok(
        json!({"instanceId":instance,"token":token,"baseUrl":format!("http://127.0.0.1:{}",store::PORT)}),
    )
}

fn read_file(path: &Path) -> Result<Option<String>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn normalized(path: &Path) -> Result<String, ManagerError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|v| matches!(v, std::path::Component::ParentDir))
    {
        return Err(store::fail("客户端配置目录必须为不含 .. 的绝对路径"));
    }
    let resolved = if path.exists() {
        path.canonicalize()?
    } else if let Some(parent) = path.parent().filter(|p| p.exists()) {
        parent.canonicalize()?.join(
            path.file_name()
                .ok_or_else(|| store::fail("配置文件名无效"))?,
        )
    } else {
        path.to_path_buf()
    };
    let text = resolved.to_string_lossy().to_string();
    Ok(if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    })
}

fn active_requests(paths: &AppPaths, instance: Option<&str>) -> Result<bool, ManagerError> {
    let c = database::open(paths)?;
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM service_gateway_requests WHERE finished_at IS NULL AND (?1 IS NULL OR instance_id=?1))",
        [instance],|r|r.get(0))?)
}

pub(crate) fn activate(
    paths: &AppPaths,
    targets: &Value,
    value: &Value,
) -> Result<Value, ManagerError> {
    if !store::active(paths)? {
        return Err(store::fail("请先启用服务商配置中心"));
    }
    let agent = store::text(value, "agentType");
    if !["codex", "claude", "claude-desktop"].contains(&agent) {
        return Err(store::fail("Agent 类型无效"));
    }
    let provider = store::text(value, "providerId");
    let c = database::open(paths)?;
    let config = store::config(&c, provider)?;
    if config["enabled"] != 1 || !config["deletedAt"].is_null() {
        return Err(store::fail("服务商已禁用或删除"));
    }
    let binding = store::query(
        &c,
        "SELECT default_model_id AS defaultModelId,options_json AS optionsJson
        FROM service_provider_agent_bindings WHERE provider_id=?1 AND agent_type=?2 AND enabled=1",
        &[&provider, &agent],
    )?
    .pop()
    .ok_or_else(|| store::fail("该服务商尚未授权此 Agent"))?;
    let model_id = value["modelId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or(binding["defaultModelId"].as_str())
        .unwrap_or("");
    let models=store::query(&c,"SELECT id,upstream_model_id AS upstreamModelId,catalog_json AS catalogJson,context_window AS contextWindow
        FROM service_provider_models WHERE provider_id=?1 AND enabled=1",&[&provider])?;
    let model = models
        .iter()
        .find(|m| store::text(m, "id") == model_id)
        .ok_or_else(|| store::fail("请选择已启用的模型"))?;
    if config["kind"] == "api" {
        store::credential(&c, provider)?;
    } else {
        store::account(&c, provider)?;
    }
    let default_path = targets
        .as_array()
        .into_iter()
        .flatten()
        .find(|v| v["id"] == agent)
        .map(|v| store::text(v, "configPath"))
        .unwrap_or("");
    let directory = PathBuf::from(
        value["configPath"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or(default_path),
    );
    let token = format!(
        "sk-am-{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let base_url = format!("http://127.0.0.1:{}", store::PORT);
    let name = store::text(model, "upstreamModelId");
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let mut originals = std::collections::HashMap::new();
    if agent == "codex" {
        let file = directory.join("config.toml");
        let before = read_file(&file)?;
        let mut content = before.clone().unwrap_or_default();
        originals.insert(file.clone(), before);
        let catalog_path = directory.join(".ai_manager_service_models.json");
        let mut catalog = Vec::new();
        for item in &models {
            let mut entry = item["catalog"].clone();
            entry["slug"] = item["upstreamModelId"].clone();
            if !item["contextWindow"].is_null() {
                entry["context_window"] = item["contextWindow"].clone();
            }
            catalog.push(entry);
        }
        let catalog = codex_models::normalize_catalog(&json!({"models":catalog}))?;
        // 只接管模型和独立 provider 节点，保留用户已有 MCP、权限和项目配置。
        let root = [
            ("model_provider", json!("ai_manager_service")),
            ("model", json!(name)),
            ("model_catalog_json", json!(catalog_path.to_string_lossy())),
            ("disable_response_storage", json!(true)),
        ];
        for (key, val) in root {
            content =
                runtime_provider::set_toml_root_line(&content, key, &format!("{key} = {val}"));
        }
        for (key, option) in [("model_reasoning_effort", "modelReasoningEffort")] {
            if let Some(val) = binding["options"].get(option).filter(|v| !v.is_null()) {
                content =
                    runtime_provider::set_toml_root_line(&content, key, &format!("{key} = {val}"));
            }
        }
        if binding["options"]["serviceTierFast"] == true
            || binding["options"]["serviceTierFast"] == 1
        {
            content = runtime_provider::set_toml_root_line(
                &content,
                "service_tier",
                "service_tier = \"fast\"",
            );
        } else {
            content = runtime_provider::remove_toml_root_line(&content, "service_tier");
        }
        if binding["options"]["modelContextWindowEnabled"] == true
            || binding["options"]["modelContextWindowEnabled"] == 1
        {
            content = runtime_provider::set_toml_root_line(
                &content,
                "model_context_window",
                "model_context_window = 1000000",
            );
            let limit = binding["options"]["modelAutoCompactTokenLimit"]
                .as_i64()
                .unwrap_or(900000);
            content = runtime_provider::set_toml_root_line(
                &content,
                "model_auto_compact_token_limit",
                &format!("model_auto_compact_token_limit = {limit}"),
            );
        } else {
            for key in ["model_context_window", "model_auto_compact_token_limit"] {
                content = runtime_provider::remove_toml_root_line(&content, key);
            }
        }
        let section = "model_providers.ai_manager_service";
        if !content
            .lines()
            .any(|line| line.trim() == format!("[{section}]"))
        {
            content.push_str(&format!("\n[{section}]\n"));
        }
        for (key, val) in [
            ("name", json!("AI Manager")),
            ("base_url", json!(format!("{base_url}/v1"))),
            ("wire_api", json!("responses")),
            ("requires_openai_auth", json!(false)),
            ("experimental_bearer_token", json!(token)),
            ("supports_websockets", json!(false)),
        ] {
            content = runtime_provider::set_toml_section_line(
                &content,
                section,
                key,
                &format!("{key} = {val}"),
            );
        }
        files.push((file, format!("{}\n", content.trim_end())));
        files.push((
            catalog_path,
            format!("{}\n", serde_json::to_string_pretty(&catalog)?),
        ));
    } else if agent == "claude" {
        let file = directory.join("settings.json");
        let before = read_file(&file)?;
        let mut content: Value = serde_json::from_str(before.as_deref().unwrap_or("{}"))?;
        originals.insert(file.clone(), before);
        if !content.is_object() || content.get("env").is_some_and(|v| !v.is_object()) {
            return Err(store::fail("Claude settings.json / env 必须是对象"));
        }
        if content.get("env").is_none() {
            content["env"] = json!({});
        }
        content["env"]
            .as_object_mut()
            .unwrap()
            .remove("ANTHROPIC_API_KEY");
        for (key, val) in [
            ("ANTHROPIC_BASE_URL", base_url.as_str()),
            ("ANTHROPIC_AUTH_TOKEN", token.as_str()),
            ("ANTHROPIC_MODEL", name),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL", name),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL", name),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL", name),
            ("CLAUDE_CODE_SUBAGENT_MODEL", name),
        ] {
            content["env"][key] = json!(val);
        }
        content["model"] = json!(name);
        files.push((
            file,
            format!("{}\n", serde_json::to_string_pretty(&content)?),
        ));
    } else {
        let desktop_models=models.iter().map(|v|json!({"name":v["upstreamModelId"],"supports1m":v["contextWindow"].as_i64().is_some_and(|n|n>=1_000_000)})).collect::<Vec<_>>();
        for (path, before, content) in
            claude_desktop::service_configuration(&base_url, &token, &desktop_models)?
        {
            originals.insert(path.clone(), before);
            files.push((
                path,
                format!("{}\n", serde_json::to_string_pretty(&content)?),
            ));
        }
    }
    let instance_path = normalized(
        files
            .first()
            .ok_or_else(|| store::fail("没有可接管的文件"))?
            .0
            .parent()
            .unwrap(),
    )?;
    let existing = store::query(
        &c,
        "SELECT id,status FROM service_agent_instances WHERE agent_type=?1 AND config_path=?2",
        &[&agent, &instance_path],
    )?
    .pop();
    if let Some(item) = &existing {
        let pending:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM service_managed_files WHERE instance_id=?1 AND restore_status<>'restored')",
            [store::text(item,"id")],|r|r.get(0))?;
        if item["status"] == "active" || pending {
            return Err(store::fail(
                "该实例已被接管，请先解除接管再切换服务商或重新同步",
            ));
        }
    }
    let instance = existing
        .as_ref()
        .map(|v| store::text(v, "id").to_string())
        .unwrap_or_else(store::id);
    let operation = store::id();
    let version = targets
        .as_array()
        .into_iter()
        .flatten()
        .find(|v| v["id"] == agent)
        .map(|v| store::text(v, "version"))
        .unwrap_or("");
    let protocol = if agent == "codex" {
        "responses"
    } else {
        "messages"
    };
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if tx.query_row(
        "SELECT value FROM app_metadata WHERE key=?1",
        [store::MODE_KEY],
        |r| r.get::<_, String>(0),
    )? != "service"
        || store::config(&tx, provider)?["revision"] != config["revision"]
    {
        return Err(store::fail("运行模式或服务商配置已变化，请重新应用"));
    }
    tx.execute("INSERT INTO service_agent_instances(id,agent_type,name,config_path,agent_version,client_protocol,provider_id,model_id,status,created_at,updated_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'error',?9,?9) ON CONFLICT(id) DO UPDATE SET provider_id=excluded.provider_id,
        model_id=excluded.model_id,status='error',sync_status='pending',agent_version=excluded.agent_version,revision=revision+1,updated_at=excluded.updated_at",
        params![instance,agent,value["name"].as_str().filter(|s|!s.is_empty()).unwrap_or(agent),instance_path,version,protocol,provider,model_id,store::now()])?;
    tx.execute("UPDATE service_gateway_tokens SET status='revoked',revoked_at=?2 WHERE instance_id=?1 AND status='active'",params![instance,store::now()])?;
    tx.execute("INSERT INTO service_gateway_tokens(id,token_hash,token_prefix,instance_id,created_at) VALUES(?1,?2,?3,?4,?5)",
        params![store::id(),store::hash(&token),&token[..14],instance,store::now()])?;
    let mut prepared = Vec::new();
    for (path, after) in files {
        let expected = originals.get(&path);
        let path = normalized(&path)?;
        let before = read_file(Path::new(&path))?;
        if expected.is_some_and(|snapshot| *snapshot != before) {
            return Err(store::fail("生成配置期间客户端文件已变化，请重新应用"));
        }
        let entry = store::id();
        tx.execute("INSERT INTO service_managed_files(id,operation_id,instance_id,file_path,before_exists,before_sha256,before_ciphertext,
            after_sha256,after_ciphertext,managed_fields_json,write_status,created_at,updated_at)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'prepared',?11,?11)",
            params![entry,operation,instance,path,before.is_some(),store::hash(before.as_deref().unwrap_or("")),
                runtime_provider::encrypt_provider_key(before.as_deref().unwrap_or(""))?,store::hash(&after),
                runtime_provider::encrypt_provider_key(&after)?,json!({"agent":agent,"snapshot":"whole-file-with-hash-conflict-check"}).to_string(),store::now()])?;
        prepared.push((entry, path, before, after));
    }
    tx.commit()?;
    let result = (|| {
        for (entry, path, before, after) in &prepared {
            if read_file(Path::new(path))? != *before {
                return Err(store::fail("准备接管期间客户端文件已发生变化"));
            }
            claude_desktop::atomic_write(Path::new(path), after.as_bytes())?;
            c.execute(
                "UPDATE service_managed_files SET write_status='written',updated_at=?2 WHERE id=?1",
                params![entry, store::now()],
            )?;
        }
        if store::config(&c, provider)?["revision"] != config["revision"] || !store::active(paths)?
        {
            return Err(store::fail("写入期间运行模式或服务商配置已变化"));
        }
        let applied_hash = store::hash(
            &prepared
                .iter()
                .map(|(_, path, _, after)| format!("{path}:{}", store::hash(after)))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        c.execute("UPDATE service_agent_instances SET status='active',sync_status='synced',applied_revision=?2,
            last_synced_at=?3,last_error='',updated_at=?3,applied_hash=?4 WHERE id=?1",params![instance,config["revision"].as_i64(),store::now(),applied_hash])?;
        Ok::<_, ManagerError>(())
    })();
    if let Err(error) = result {
        let restore = detach(paths, &instance);
        return Err(store::fail(format!(
            "接管失败：{error}；恢复结果：{}",
            restore
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "已恢复".into())
        )));
    }
    Ok(json!({"instanceId":instance,"baseUrl":base_url}))
}

pub(crate) fn detach(paths: &AppPaths, instance: &str) -> Result<(), ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // 与网关入站共用写事务，检查在途请求和撤销 Token 之间不能再插入请求。
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM service_gateway_requests WHERE instance_id=?1 AND finished_at IS NULL)",
        [instance],|r|r.get::<_,bool>(0))?{return Err(store::fail("该实例仍有请求进行中，请结束请求后重试"));}
    tx.execute("UPDATE service_gateway_tokens SET status='revoked',revoked_at=?2 WHERE instance_id=?1 AND status='active'",params![instance,store::now()])?;
    tx.execute(
        "UPDATE service_agent_instances SET status='error',sync_status='pending' WHERE id=?1",
        [instance],
    )?;
    tx.commit()?;
    let files=store::query(&c,"SELECT * FROM service_managed_files WHERE instance_id=?1 AND restore_status<>'restored' ORDER BY created_at DESC",&[&instance])?;
    let mut conflicts = Vec::new();
    for file in files {
        let path = Path::new(store::text(&file, "file_path"));
        let result = (|| {
            let current = read_file(path)?;
            let before =
                runtime_provider::decrypt_provider_key(store::text(&file, "before_ciphertext"))?;
            let original = if file["before_exists"] == 1 {
                Some(before.as_str())
            } else {
                None
            };
            if current.as_deref() == original {
                Ok(())
            } else if current.as_ref().map(|v| store::hash(v)).as_deref()
                == Some(store::text(&file, "after_sha256"))
            {
                if original.is_some() {
                    claude_desktop::atomic_write(path, before.as_bytes())
                } else {
                    std::fs::remove_file(path).map_err(ManagerError::from)
                }
            } else {
                Err(store::fail("文件被外部修改，请手动对照备份处理后重试"))
            }
        })();
        match result {
            Ok(()) => {
                c.execute("UPDATE service_managed_files SET restore_status='restored',error_message='',updated_at=?2 WHERE id=?1",params![file["id"].as_str(),store::now()])?;
            }
            Err(e) => {
                conflicts.push(format!("{}：{e}", path.display()));
                c.execute("UPDATE service_managed_files SET restore_status='conflict',error_message=?2,updated_at=?3 WHERE id=?1",params![file["id"].as_str(),e.to_string(),store::now()])?;
            }
        }
    }
    if !conflicts.is_empty() {
        c.execute(
            "UPDATE service_agent_instances SET sync_status='conflict',last_error=?2 WHERE id=?1",
            params![instance, conflicts.join("；")],
        )?;
        return Err(store::fail(conflicts.join("；")));
    }
    c.execute("UPDATE service_agent_instances SET status='detached',sync_status='restored',provider_id=NULL,model_id=NULL,
        last_error='',revision=revision+1,updated_at=?2 WHERE id=?1",params![instance,store::now()])?;
    Ok(())
}

pub(crate) fn switch_backend(paths: &AppPaths, backend: &str) -> Result<(), ManagerError> {
    if !["legacy", "service"].contains(&backend) {
        return Err(store::fail("模式无效"));
    }
    store::initialize(paths)?;
    let current = if store::active(paths)? {
        "service"
    } else {
        "legacy"
    };
    if backend == current {
        return Ok(());
    }
    let _legacy_gate = if current == "legacy" {
        Some(store::pause_legacy(paths)?)
    } else {
        None
    };
    let c = database::open(paths)?;
    let operation = store::id();
    c.execute("INSERT INTO service_switch_operations(id,from_backend,to_backend,generation,stage,created_at)
        VALUES(?1,?2,?3,?4,'preparing',?5)",params![operation,current,backend,store::generation(&c)?+1,store::now()])?;
    c.execute("INSERT INTO app_metadata(key,value) VALUES('service_provider:accepting','0') ON CONFLICT(key) DO UPDATE SET value='0'",[])?;
    let result = (|| {
        if active_requests(paths, None)? {
            return Err(store::fail(
                "仍有网关请求进行中，本次切换已取消，请结束请求后重试",
            ));
        }
        if backend == "service" {
            // 每次从旧模式启用时幂等复制，失败时不提交模式；新表中的编辑始终保留。
            store::migrate(paths)?;
        }
        if backend == "legacy" {
            for instance in store::query(
                &c,
                "SELECT id FROM service_agent_instances WHERE status<>'detached'",
                &[],
            )? {
                detach(paths, store::text(&instance, "id"))?;
            }
        }
        let mut c = database::open(paths)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![store::MODE_KEY,backend])?;
        tx.execute("INSERT INTO app_metadata(key,value) VALUES(?1,'1') ON CONFLICT(key) DO UPDATE SET value=CAST(CAST(value AS INTEGER)+1 AS TEXT)",[store::GENERATION_KEY])?;
        tx.execute(
            "UPDATE service_switch_operations SET stage='completed',completed_at=?2 WHERE id=?1",
            params![operation, store::now()],
        )?;
        tx.commit()?;
        Ok::<_, ManagerError>(())
    })();
    if let Err(error) = &result {
        c.execute("UPDATE service_switch_operations SET stage='failed',error_message=?2,completed_at=?3 WHERE id=?1",params![operation,error.to_string(),store::now()])?;
    }
    c.execute(
        "UPDATE app_metadata SET value='1' WHERE key='service_provider:accepting'",
        [],
    )?;
    result
}

pub(crate) fn recover(paths: &AppPaths) -> Result<(), ManagerError> {
    store::initialize(paths)?;
    store::recover_requests(paths)?;
    let c = database::open(paths)?;
    c.execute("UPDATE service_gateway_tokens SET status='revoked',revoked_at=?1 WHERE instance_id IN
        (SELECT id FROM service_agent_instances WHERE config_path LIKE 'internal:%' OR config_path LIKE 'test:%')",[store::now()])?;
    c.execute("UPDATE service_agent_instances SET status='detached',sync_status='restored' WHERE config_path LIKE 'internal:%' OR config_path LIKE 'test:%'",[])?;
    c.execute("UPDATE service_switch_operations SET stage='interrupted',error_message='应用退出导致切换中断',completed_at=?1 WHERE completed_at IS NULL",[store::now()])?;
    c.execute(
        "UPDATE app_metadata SET value='1' WHERE key='service_provider:accepting'",
        [],
    )?;
    for instance in store::query(&c,"SELECT DISTINCT i.id FROM service_agent_instances i JOIN service_managed_files f ON f.instance_id=i.id
        WHERE (i.status='error' OR f.write_status='prepared') AND f.restore_status<>'restored'",&[])?{
        // 中断的接管只恢复自身写入的文件，外部修改继续保持冲突供用户处理。
        let _=detach(paths,store::text(&instance,"id"));
    }
    Ok(())
}

pub(crate) fn internal_session(
    paths: &AppPaths,
    provider: &str,
    model: &str,
    api: &str,
) -> Result<Value, ManagerError> {
    if !store::active(paths)? {
        return Err(store::fail("共享网关未启用"));
    }
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let source=store::query(&tx,"SELECT p.name,m.id AS modelId FROM service_provider_configs p
        JOIN service_provider_models m ON m.provider_id=p.id AND m.upstream_model_id=?2 AND m.enabled=1
        JOIN service_provider_agent_bindings b ON b.provider_id=p.id AND b.agent_type='codex' AND b.enabled=1
        WHERE p.id=?1 AND p.enabled=1 AND p.deleted_at IS NULL",&[&provider,&model])?.pop()
        .ok_or_else(||store::fail("服务商、模型不可用或未授权 Codex；内置文本 Agent 复用 Codex 授权"))?;
    if !["responses", "chat", "messages"].contains(&api) {
        return Err(store::fail("入站协议无效"));
    }
    let instance = store::id();
    let token = format!("sk-internal-{}{}", store::id(), store::id());
    tx.execute("INSERT INTO service_agent_instances(id,agent_type,name,config_path,client_protocol,provider_id,model_id,status,sync_status,created_at,updated_at)
        VALUES(?1,'codex','内置文本 Agent',?2,?3,?4,?5,'active','synced',?6,?6)",params![instance,format!("internal:{instance}"),api,provider,source["modelId"].as_str(),store::now()])?;
    tx.execute(
        "INSERT INTO service_gateway_tokens(id,token_hash,token_prefix,instance_id,created_at)
        VALUES(?1,?2,'internal',?3,?4)",
        params![store::id(), store::hash(&token), instance, store::now()],
    )?;
    tx.commit()?;
    Ok(json!({"instanceId":instance,"token":token,"providerName":source["name"]}))
}

pub(crate) fn release_session(paths: &AppPaths, instance: &str) {
    if let Ok(mut c) = database::open(paths) {
        if let Ok(tx) = c.transaction() {
            let _=tx.execute("UPDATE service_gateway_tokens SET status='revoked',revoked_at=?2 WHERE instance_id=?1",params![instance,store::now()]);
            let _=tx.execute("UPDATE service_agent_instances SET status='detached',sync_status='restored' WHERE id=?1",[instance]);
            let _ = tx.commit();
        }
    }
}

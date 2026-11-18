use super::*;
use crate::core::{database, service_provider_store as store, usage_store};
use rusqlite::{params, Connection, TransactionBehavior};

// 转换后的 Messages 使用可回溯的本地 ID，Usage 可与客户端会话精确去重。
pub(super) fn client_message_id(record: &Value) -> String {
    format!("msg_aim_{}", store::text(record, "serviceRequestId").replace('-', ""))
}

// Hyper 可能在响应头返回前取消处理任务；最后一个上下文退出时关闭遗留账本。
pub(super) struct RequestLease {
    paths: AppPaths,
    request_id: String,
}

impl Drop for RequestLease {
    fn drop(&mut self) {
        let result = (|| -> Result<(), ManagerError> {
            let mut c = database::open(&self.paths)?;
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = store::now();
            tx.execute(
                "UPDATE service_gateway_attempts SET state='unknown',finished_at=?2,
                 error_message='请求处理已中断，最终用量未知'
                 WHERE request_id=?1 AND finished_at IS NULL",
                params![self.request_id, now],
            )?;
            tx.execute(
                "UPDATE service_gateway_requests SET state='unknown',finished_at=?2,
                 error_message='请求处理已中断，最终用量未知'
                 WHERE id=?1 AND finished_at IS NULL",
                params![self.request_id, now],
            )?;
            tx.commit()?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("共享网关中断请求保存失败：{error}");
        }
    }
}

pub async fn start(registry: &GatewayServerRegistry, paths: &AppPaths) -> Result<(), ManagerError> {
    let address = SocketAddr::from(([127, 0, 0, 1], store::PORT));
    let mut guard = registry.inner.lock().await;
    if guard.as_ref().is_some_and(|(_, task)| !task.is_finished()) {
        return Ok(());
    }
    let listener = TcpListener::bind(address).await?;
    *guard = Some((
        address,
        spawn_listener(
            listener,
            GatewayContext {
                paths: paths.clone(),
                cli_targets: json!([]),
                store_lock: registry.store_lock.clone(),
                service_mode: true,
                legacy_request: None,
                _service_request: None,
            },
        ),
    ));
    Ok(())
}

fn accepting(c: &Connection) -> Result<bool, ManagerError> {
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1 AND value='service')
        AND NOT EXISTS(SELECT 1 FROM app_metadata WHERE key='service_provider:accepting' AND value='0')",
        [store::MODE_KEY], |r| r.get(0))?)
}

pub(crate) fn authorize(c: &Connection, secret: &str) -> Result<Value, ManagerError> {
    if secret.is_empty() {
        return Err(store::fail("缺少共享网关 Token"));
    }
    store::query(c, "SELECT t.id AS tokenId,i.id AS instanceId,i.agent_type AS agentType,i.provider_id AS providerId,
        i.model_id AS modelId,i.client_protocol AS clientProtocol,i.agent_version AS agentVersion,p.revision AS providerRevision
        FROM service_gateway_tokens t JOIN service_agent_instances i ON i.id=t.instance_id
        JOIN service_provider_configs p ON p.id=i.provider_id
        JOIN service_provider_agent_bindings b ON b.provider_id=p.id AND b.agent_type=i.agent_type
        WHERE t.token_hash=?1 AND t.status='active' AND (t.expires_at IS NULL OR t.expires_at>?2)
        AND i.status='active' AND i.sync_status='synced' AND p.enabled=1 AND p.deleted_at IS NULL AND b.enabled=1",
        &[&store::hash(secret), &store::now()])?.pop().ok_or_else(|| store::fail("Token 无效、实例未同步或 Agent 未获授权"))
}

fn receive(
    paths: &AppPaths,
    secret: &str,
    path: &str,
    protocol: &str,
    client_id: &str,
) -> Result<(Value, Value), ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !accepting(&tx)? {
        return Err(store::fail("共享网关未启用或正在切换"));
    }
    let auth = authorize(&tx, secret);
    let identity = auth.as_ref().ok().cloned().unwrap_or(json!({}));
    let request_id = store::id();
    let generation = store::generation(&tx)?;
    tx.execute("INSERT INTO service_gateway_requests(id,token_id,instance_id,agent_type,generation,client_request_id,
        protocol_in,request_path,state,status_code,error_message,created_at,finished_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![request_id,identity["tokenId"].as_str(),identity["instanceId"].as_str(),store::text(&identity,"agentType"),
            generation,client_id,protocol,path,if auth.is_ok(){"received"}else{"rejected"},
            if auth.is_ok(){None}else{Some(401)},auth.as_ref().err().map(ToString::to_string).unwrap_or_default(),
            store::now(),if auth.is_ok(){None}else{Some(store::now())}])?;
    tx.commit()?;
    let identity = auth?;
    Ok((
        identity.clone(),
        json!({"serviceRequestId":request_id,"generation":generation,"id":request_id,
        "tokenId":identity["tokenId"],"instanceId":identity["instanceId"],"agentType":identity["agentType"],
        "providerRevision":identity["providerRevision"],"usageKnown":false}),
    ))
}

pub(super) fn resolve(
    paths: &AppPaths,
    identity: &Value,
    requested: &str,
) -> Result<(GatewayTarget, Value), ManagerError> {
    let c = database::open(paths)?;
    let provider = store::text(identity, "providerId");
    let agent = store::text(identity, "agentType");
    let config = store::config(&c, provider)?;
    let desktop_model = if agent == "claude-desktop" {
        let binding = store::query(&c,
            "SELECT options_json AS optionsJson FROM service_provider_agent_bindings WHERE provider_id=?1 AND agent_type=?2 AND enabled=1",
            &[&provider, &agent])?.pop().ok_or_else(|| store::fail("此服务商尚未授权 Desktop"))?;
        let fallback = store::query(&c,
            "SELECT upstream_model_id AS name FROM service_provider_models WHERE id=?1 AND provider_id=?2 AND enabled=1",
            &[&store::text(identity,"modelId"),&provider])?.pop().unwrap_or(json!({}));
        store::desktop_model_mappings(&binding["options"], store::text(&fallback, "name"))
            .into_iter()
            .find(|mapping| mapping["clientModelName"] == requested)
            .map(|mapping| store::text(&mapping, "upstreamModelId").to_owned())
    } else {
        None
    };
    // 客户端显式选择的模型按原请求解析；实例模型只用于未指定模型的请求。
    // Desktop 档位和多协议工具仍保留用户主动配置的别名映射。
    let models = store::query(&c, "SELECT id,upstream_model_id AS upstreamModelId FROM service_provider_models
        WHERE provider_id=?1 AND enabled=1 AND id=coalesce(
        (SELECT model_id FROM service_agent_model_routes WHERE provider_id=?1 AND agent_type=?3 AND client_model_name=?2 AND enabled=1),
        CASE WHEN ?5 IS NOT NULL THEN
            (SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id=?5)
        ELSE (SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id=?2 AND enabled=1) END,
        CASE WHEN ?2='' THEN ?4 END)",
        &[&provider,&requested,&agent,&store::text(identity,"modelId"),&desktop_model])?;
    let model = models
        .into_iter()
        .next()
        .ok_or_else(|| store::fail("模型未配置、已禁用或未映射到此服务商"))?;
    Ok((
        GatewayTarget {
            id: provider.into(),
            name: store::text(&config, "name").into(),
            provider_type: format!("service-{}", store::text(&config, "upstreamProtocol")),
            base_url: store::text(&config, "baseUrl").into(),
            proxy: store::text(&config, "proxy").into(),
            auth_field: String::new(),
            headers: config["headers"].clone(),
            cli: agent.into(),
            kind: match store::text(&config, "kind") {
                "codex_account" => TargetKind::CodexAccount,
                "google_account" => TargetKind::GoogleAccount,
                _ => TargetKind::Provider,
            },
            service_source: true,
        },
        model,
    ))
}

pub(super) async fn handle_request(
    request: Request<Incoming>,
    mut context: GatewayContext,
    _peer: SocketAddr,
) -> Response<GatewayBody> {
    let path = request.uri().path().to_string();
    if request.method() == Method::GET && path == "/health" {
        return json_response(
            StatusCode::OK,
            json!({"ok":store::active(&context.paths).unwrap_or(false),"service":"service-provider-gateway"}),
        );
    }
    let secret = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            request
                .headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
        })
        .or_else(|| {
            request
                .headers()
                .get("x-goog-api-key")
                .and_then(|v| v.to_str().ok())
        })
        .unwrap_or("")
        .to_string();
    if request.method() == Method::GET && ["/models", "/v1/models"].contains(&path.as_str()) {
        let result = (|| {
            let c = database::open(&context.paths)?;
            if !accepting(&c)? {
                return Err(store::fail("共享网关未启用"));
            }
            let auth = authorize(&c, &secret)?;
            let models = store::query(
                &c,
                "SELECT upstream_model_id AS id,'model' AS object FROM service_provider_models
                WHERE provider_id=?1 AND enabled=1",
                &[&store::text(&auth, "providerId")],
            )?;
            Ok::<_, ManagerError>(json!({"object":"list","data":models}))
        })();
        return match result {
            Ok(v) => json_response(StatusCode::OK, v),
            Err(e) => error_response(StatusCode::UNAUTHORIZED, &e.to_string()),
        };
    }
    let compact = ["/responses/compact", "/v1/responses/compact"].contains(&path.as_str());
    let route = if compact {
        Some(GatewayRoute {
            protocol: Protocol::Responses,
            stream: false,
            requested_model: String::new(),
        })
    } else {
        parse_route(&request)
    };
    let protocol = route
        .as_ref()
        .map(|v| protocol_name(v.protocol))
        .unwrap_or("unsupported");
    let client_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let session_id = ["session_id", "x-session-id", "x-codex-session-id",
        "conversation_id", "x-conversation-id", "x-claude-code-session-id"]
        .iter().find_map(|name| request.headers().get(*name)
            .and_then(|value| value.to_str().ok()).filter(|value| !value.is_empty()))
        .unwrap_or("").to_string();
    let (identity, mut record) = match receive(&context.paths, &secret, &path, protocol, &client_id)
    {
        Ok(v) => v,
        Err(e) => return error_response(StatusCode::UNAUTHORIZED, &e.to_string()),
    };
    context._service_request = Some(Arc::new(RequestLease {
        paths: context.paths.clone(),
        request_id: store::text(&record, "serviceRequestId").into(),
    }));
    let started = Instant::now();
    record["serviceCompact"] = json!(compact);
    let result=async {
        let route=route.ok_or_else(||GatewayFailure{status:StatusCode::NOT_FOUND,message:"共享网关不支持该路径或 WebSocket，请使用 HTTP Responses / Messages / Chat".into()})?;
        if request.method()!=Method::POST { return Err(GatewayFailure{status:StatusCode::METHOD_NOT_ALLOWED,message:"该接口需要 POST".into()}); }
        let expected=match store::text(&identity,"clientProtocol") {"messages"=>Protocol::Claude,"chat"=>Protocol::Chat,_=>Protocol::Responses};
        if store::text(&identity,"clientProtocol")!="all" && route.protocol!=expected {return Err(GatewayFailure{status:StatusCode::FORBIDDEN,message:"此 Token 未授权使用该入站协议".into()});}
        let bytes=http_body_util::Limited::new(request.into_body(),MAX_BODY_SIZE).collect().await
            .map_err(|_|GatewayFailure{status:StatusCode::PAYLOAD_TOO_LARGE,message:"请求体过大或读取失败".into()})?.to_bytes();
        let mut payload:Value=serde_json::from_slice(&bytes).ok().filter(Value::is_object)
            .ok_or_else(||GatewayFailure{status:StatusCode::BAD_REQUEST,message:"请求体必须是 JSON 对象".into()})?;
        crate::api::usage::record_gateway_skills(&context.paths, &record, &payload, &session_id)?;
        if compact && payload["stream"]==true {
            return Err(GatewayFailure{status:StatusCode::BAD_REQUEST,message:"Responses 压缩接口不支持流式请求".into()});
        }
        let requested=if route.protocol==Protocol::Gemini{route.requested_model.clone()}else{store::text(&payload,"model").to_string()};
        let (target,model)=resolve(&context.paths,&identity,&requested).map_err(|e|GatewayFailure{status:StatusCode::BAD_REQUEST,message:e.to_string()})?;
        if compact && upstream_format(&target)!=UpstreamFormat::OpenAiResponses {
            return Err(GatewayFailure{status:StatusCode::UNPROCESSABLE_ENTITY,message:"当前 Chat 上游不支持 Responses 原生压缩，请在客户端使用本地压缩或改用 Responses 服务商".into()});
        }
        let physical=store::text(&model,"upstreamModelId");
        if requested.is_empty(){payload["model"]=json!(physical);}
        record["requestedModel"]=json!(if requested.is_empty(){physical}else{&requested});
        record["modelId"]=model["id"].clone();
        record["providerId"]=json!(target.id);
        record["providerName"]=json!(target.name);
        record["physicalModel"]=json!(physical);
        // 相同 Responses 协议保留原生字段；跨协议仍执行语义校验。
        let canonical=if route.protocol==Protocol::Responses && upstream_format(&target)==UpstreamFormat::OpenAiResponses {
            let mut canonical=canonical_responses(&payload)?;
            canonical.stream|=route.stream;
            canonical
        } else {
            canonical_request(&route,&payload).map_err(|e|GatewayFailure{status:StatusCode::BAD_REQUEST,message:e.to_string()})?
        };
        send_to_target(&context,&target,physical,&canonical,&mut record,started).await
    }.await;
    match result {
        Ok(response) => response,
        Err(error) => {
            finish_usage(
                &context,
                record,
                None,
                error.status,
                Some(&error.message),
                started,
            )
            .await;
            error_response(error.status, &error.message)
        }
    }
}

pub(super) async fn target_auth(
    paths: &AppPaths,
    target: &GatewayTarget,
) -> Result<TargetAuth, ManagerError> {
    if target.kind == TargetKind::CodexAccount {
        let auth = codex_account::service_proxy_auth(paths, &target.id).await?;
        Ok(TargetAuth {
            token: store::text(&auth, "accessToken").into(),
            key_id: target.id.clone(),
            account_id: store::text(&auth, "accountId").into(),
        })
    } else if target.kind == TargetKind::GoogleAccount {
        let provider = crate::api::google_account::find_account(paths, &target.id)?;
        crate::api::google_gateway::ensure_started(paths, &provider).await?;
        Ok(TargetAuth {
            token: store::google_local_key(&database::open(paths)?, &target.id)?,
            key_id: target.id.clone(),
            account_id: String::new(),
        })
    } else {
        let key = store::credential(&database::open(paths)?, &target.id)?;
        Ok(TargetAuth {
            token: store::text(&key, "secret").into(),
            key_id: store::text(&key, "id").into(),
            account_id: String::new(),
        })
    }
}

pub(super) fn start_attempt(
    paths: &AppPaths,
    target: &GatewayTarget,
    auth: &TargetAuth,
    record: &mut Value,
) -> Result<(), ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !accepting(&tx)? || store::generation(&tx)? != record["generation"].as_i64().unwrap_or(-1) {
        return Err(store::fail("运行模式已经变化，请重试请求"));
    }
    let current = store::query(
        &tx,
        "SELECT p.revision FROM service_provider_configs p
        JOIN service_provider_agent_bindings b ON b.provider_id=p.id AND b.agent_type=?2
        JOIN service_agent_instances i ON i.provider_id=p.id AND i.id=?3
        JOIN service_gateway_tokens t ON t.instance_id=i.id AND t.id=?4 AND t.status='active'
        WHERE p.id=?1 AND p.enabled=1 AND p.deleted_at IS NULL AND b.enabled=1 AND i.status='active'
        AND i.sync_status='synced' AND (t.expires_at IS NULL OR t.expires_at>?5)",
        &[
            &target.id,
            &store::text(record, "agentType"),
            &store::text(record, "instanceId"),
            &store::text(record, "tokenId"),
            &store::now(),
        ],
    )?;
    if current.first().and_then(|v| v["revision"].as_i64()) != record["providerRevision"].as_i64() {
        return Err(store::fail("服务商配置或授权已变化，请重试请求"));
    }
    let model=store::query(&tx,"SELECT upstream_model_id AS upstreamModelId,input_price_micros AS inputPriceMicros,cached_input_price_micros AS cachedInputPriceMicros,
        output_price_micros AS outputPriceMicros,currency FROM service_provider_models WHERE provider_id=?1 AND id=?2 AND enabled=1",
        &[&target.id,&store::text(record,"modelId")])?.pop().ok_or_else(||store::fail("模型已禁用"))?;
    let model = store::model_pricing(&model, &usage_store::read_pricing_connection(&tx)?);
    let (credential, name, fingerprint) = if target.kind == TargetKind::CodexAccount {
        if store::account(&tx, &target.id)?["accessToken"] != auth.token {
            return Err(store::fail("账号令牌已轮换，请重试"));
        }
        let row = store::query(
            &tx,
            "SELECT id,account_email AS email,credential_generation AS token_generation FROM service_provider_configs WHERE id=?1",
            &[&target.id],
        )?
        .pop()
        .ok_or_else(|| store::fail("账号不存在"))?;
        (
            format!("{}:{}", store::text(&row, "id"), row["token_generation"]),
            store::text(&row, "email").to_string(),
            String::new(),
        )
    } else if target.kind == TargetKind::GoogleAccount {
        if store::google_local_key(&tx, &target.id)? != auth.token {
            return Err(store::fail("Google 转发凭据已变化，请重试"));
        }
        let row = store::query(&tx, "SELECT account_email AS email,credential_generation AS token_generation FROM service_provider_configs WHERE id=?1", &[&target.id])?
            .pop().ok_or_else(|| store::fail("Google 账号不存在"))?;
        (format!("google:{}:{}", target.id, row["token_generation"]), store::text(&row, "email").into(), String::new())
    } else {
        let row = store::credential(&tx, &target.id)?;
        if store::text(&row, "id") != auth.key_id || store::text(&row, "secret") != auth.token {
            return Err(store::fail("Key 已轮换，请重试请求"));
        }
        (
            auth.key_id.clone(),
            store::text(&row, "name").into(),
            store::text(&row, "fingerprint").into(),
        )
    };
    let attempt = store::id();
    tx.execute("INSERT INTO service_gateway_attempts(id,request_id,attempt_no,provider_id,provider_name,credential_id,
        credential_name,credential_fingerprint,model_id,upstream_model,protocol_out,state,price_snapshot_json,currency,started_at)
        VALUES(?1,?2,1,?3,?4,?5,?6,?7,?8,?9,?10,'started',?11,?12,?13)",
        params![attempt,store::text(record,"serviceRequestId"),target.id,target.name,credential,name,fingerprint,
            store::text(record,"modelId"),store::text(record,"physicalModel"),store::text(record,"upstreamProtocol"),
            model.to_string(),store::text(&model,"currency"),store::now()])?;
    tx.execute("UPDATE service_gateway_requests SET state='routed',requested_model=?2 WHERE id=?1 AND finished_at IS NULL",
        params![store::text(record,"serviceRequestId"),store::text(record,"requestedModel")])?;
    tx.execute(
        "UPDATE service_gateway_tokens SET last_used_at=?2 WHERE id=?1",
        params![store::text(record, "tokenId"), store::now()],
    )?;
    tx.commit()?;
    record["attemptId"] = json!(attempt);
    Ok(())
}

pub(super) fn response_started(
    paths: &AppPaths,
    record: &Value,
    streaming: bool,
) -> Result<(), ManagerError> {
    database::open(paths)?.execute(
        "UPDATE service_gateway_attempts SET state=?2,status_code=?3,upstream_request_id=?4
        WHERE id=?1 AND finished_at IS NULL",
        params![
            store::text(record, "attemptId"),
            if streaming { "streaming" } else { "started" },
            record["upstreamStatusCode"].as_i64(),
            store::text(record, "upstreamRequestId")
        ],
    )?;
    Ok(())
}

pub(crate) fn settle(paths: &AppPaths, record: &Value) -> Result<(), ManagerError> {
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let success = record["ok"] == true;
    let attempt = store::text(record, "attemptId");
    let input = record["measuredInput"].as_i64();
    let output = record["measuredOutput"].as_i64();
    let cached = record["measuredCache"].as_i64();
    let known = input.is_some() || output.is_some();
    let cancelled = store::text(record, "errorMessage") == "客户端已断开";
    if !attempt.is_empty() {
        let price = store::query(
            &tx,
            "SELECT price_snapshot_json AS priceJson FROM service_gateway_attempts WHERE id=?1",
            &[&attempt],
        )?
        .pop()
        .unwrap_or(json!({}))["price"]
            .clone();
        let cost = store::estimate_request_cost(input, output, cached, &price);
        tx.execute("UPDATE service_gateway_attempts SET state=?2,status_code=?3,upstream_request_id=?4,upstream_response_id=?5,
            actual_model=?6,usage_source=?7,input_tokens=?8,output_tokens=?9,cached_input_tokens=?10,
            estimated_cost_micros=?11,error_message=?12,finished_at=?13,reasoning_tokens=?14,service_tier=?15 WHERE id=?1 AND finished_at IS NULL",
            params![attempt,if success{"succeeded"}else if cancelled{"cancelled"}else{"failed"},record["upstreamStatusCode"].as_i64().or(record["statusCode"].as_i64()),
                store::text(record,"upstreamRequestId"),store::text(record,"upstreamResponseId"),store::text(record,"upstreamModel"),
                if known{"upstream"}else{"unknown"},input,output,cached,cost,store::text(record,"errorMessage"),store::now(),
                record["measuredReasoning"].as_i64(),store::text(record,"serviceTier")])?;
    }
    tx.execute("UPDATE service_gateway_requests SET state=?2,status_code=?3,error_message=?4,requested_model=?5,finished_at=?6
        WHERE id=?1 AND finished_at IS NULL",params![store::text(record,"serviceRequestId"),
        if success{"completed"}else if cancelled{"cancelled"}else if attempt.is_empty(){"rejected"}else{"failed"},record["statusCode"].as_i64(),
        store::text(record,"errorMessage"),store::text(record,"requestedModel"),store::now()])?;
    tx.commit()?;
    Ok(())
}

pub(super) fn configured_target(
    paths: &AppPaths,
    provider: &str,
) -> Result<(GatewayTarget, i64), ManagerError> {
    let config = store::config(&database::open(paths)?, provider)?;
    if config["enabled"] != 1 || !config["deletedAt"].is_null() {
        return Err(store::fail("服务商已禁用或删除"));
    }
    Ok((
        GatewayTarget {
            id: provider.into(),
            name: store::text(&config, "name").into(),
            provider_type: format!("service-{}", store::text(&config, "upstreamProtocol")),
            base_url: store::text(&config, "baseUrl").into(),
            proxy: store::text(&config, "proxy").into(),
            headers: config["headers"].clone(),
            auth_field: String::new(),
            cli: String::new(),
            kind: match store::text(&config, "kind") {
                "codex_account" => TargetKind::CodexAccount,
                "google_account" => TargetKind::GoogleAccount,
                _ => TargetKind::Provider,
            },
            service_source: true,
        },
        config["revision"].as_i64().unwrap_or(0),
    ))
}

pub(super) fn target_models(paths: &AppPaths, target: &GatewayTarget) -> Result<Vec<String>, ManagerError> {
    let models = store::query(
        &database::open(paths)?,
        "SELECT upstream_model_id AS name FROM service_provider_models WHERE provider_id=?1 AND enabled=1 ORDER BY created_at,id",
        &[&target.id],
    )?;
    let mut names = models.iter().map(|model| store::text(model, "name").to_owned()).collect::<Vec<_>>();
    if target.kind == TargetKind::CodexAccount && names.is_empty() {
        names = runtime_provider::codex_model_list(&json!({}));
    }
    Ok(names)
}

pub async fn fetch_models(paths: &AppPaths, value: &Value) -> Result<Value, ManagerError> {
    let (target, revision) = configured_target(paths, store::text(value, "providerId"))?;
    if target.kind == TargetKind::GoogleAccount {
        crate::api::google_account::refresh(paths, &target.id).await?;
        crate::api::service_provider::sync_provider(paths, &target.id)?;
        return store::state(paths);
    }
    let auth = target_auth(paths, &target).await?;
    let client = target_client(&target)?;
    let mut headers = HeaderMap::new();
    add_custom_headers(&mut headers, &target.headers)?;
    let url = if target.kind == TargetKind::CodexAccount {
        headers.insert(
            "chatgpt-account-id",
            HeaderValue::from_str(&auth.account_id).map_err(|_| store::fail("账号标识无效"))?,
        );
        headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
        let version = value["agentVersion"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or("0.153.4");
        let mut url = url::Url::parse(&format!("{CODEX_OFFICIAL_BASE_URL}/models"))
            .map_err(|_| store::fail("官方模型地址无效"))?;
        url.query_pairs_mut().append_pair("client_version", version);
        url.to_string()
    } else {
        codex_models::models_url(&target.base_url)?.to_string()
    };
    let response = client
        .get(url)
        .headers(headers)
        .bearer_auth(auth.token)
        .send()
        .await
        .map_err(|_| store::fail("无法获取模型列表，请检查网络与凭据"))?;
    if !response.status().is_success() {
        return Err(store::fail(format!(
            "获取模型失败：HTTP {}",
            response.status().as_u16()
        )));
    }
    let catalog = codex_models::normalize_catalog(
        &response
            .json::<Value>()
            .await
            .map_err(|_| store::fail("模型响应格式无效"))?,
    )?;
    // 先完整校验，再一次性写入；已有模型的价格和启用状态保持不变。
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if store::config(&tx, &target.id)?["revision"].as_i64() != Some(revision) {
        return Err(store::fail("获取模型期间服务商已发生变化，请重新获取"));
    }
    for item in catalog["models"].as_array().into_iter().flatten() {
        let existing = store::query(
            &tx,
            "SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id=?2",
            &[&target.id, &store::text(item, "slug")],
        )?
        .pop();
        if let Some(existing) = existing {
            tx.execute("UPDATE service_provider_models SET catalog_json=?2,context_window=?3,updated_at=?4 WHERE id=?1",
                params![store::text(&existing,"id"),item.to_string(),item["context_window"].as_i64(),store::now()])?;
        } else {
            store::save_model(
                &tx,
                &target.id,
                &json!({"upstreamModelId":item["slug"],"displayName":item["display_name"],
                "contextWindow":item["context_window"],"catalog":item}),
            )?;
        }
    }
    tx.execute(
        "UPDATE service_provider_configs SET revision=revision+1,updated_at=?2 WHERE id=?1",
        params![target.id, store::now()],
    )?;
    tx.commit()?;
    crate::api::service_provider::sync_provider(paths, &target.id)?;
    Ok(catalog)
}

pub async fn test(paths: &AppPaths, value: &Value) -> Result<Value, ManagerError> {
    if !store::active(paths)? {
        return Err(store::fail("请先启用新模式再运行兼容性测试"));
    }
    let provider = store::text(value, "providerId");
    let agent = store::text(value, "agentType");
    if !["codex", "claude", "claude-desktop", "gateway"].contains(&agent) {
        return Err(store::fail("Agent 类型无效"));
    }
    let (target, revision) = configured_target(paths, provider)?;
    let model = {
        let c = database::open(paths)?;
        store::query(&c,"SELECT m.id,m.upstream_model_id AS name,p.revision FROM service_provider_models m
            JOIN service_provider_configs p ON p.id=m.provider_id JOIN service_provider_agent_bindings b ON b.provider_id=p.id
            AND b.agent_type=?3 AND b.enabled=1 WHERE m.provider_id=?1 AND m.id=?2 AND m.enabled=1",
            &[&provider,&store::text(value,"modelId"),&agent])?.pop().ok_or_else(||store::fail("模型或 Agent 授权无效"))?
    };
    if model["revision"].as_i64() != Some(revision) {
        return Err(store::fail("准备测试期间配置已变化，请重新测试"));
    }
    let instance = store::id();
    let token = format!("sk-test-{}{}", store::id(), store::id());
    let protocol = if ["codex", "gateway"].contains(&agent) {
        "responses"
    } else {
        "messages"
    };
    {
        let mut c = database::open(paths)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO service_agent_instances(id,agent_type,name,config_path,agent_version,client_protocol,provider_id,
            model_id,status,sync_status,created_at,updated_at) VALUES(?1,?2,'兼容性测试',?3,?4,?5,?6,?7,'active','synced',?8,?8)",
            params![instance,agent,format!("test:{instance}"),store::text(value,"agentVersion"),protocol,provider,store::text(&model,"id"),store::now()])?;
        tx.execute("INSERT INTO service_gateway_tokens(id,token_hash,token_prefix,instance_id,expires_at,created_at)
            VALUES(?1,?2,'test',?3,?4,?5)",params![store::id(),store::hash(&token),instance,store::now()+300_000,store::now()])?;
        tx.commit()?;
    }
    let result=async{
        let tool=json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false});
        let mut body=if protocol=="responses"{
            json!({"model":model["name"],"input":"Call echo with value ping.","tools":[{"type":"function","name":"echo","description":"Echo a value","parameters":tool}],
                "tool_choice":{"type":"function","name":"echo"}})
        }else{
            json!({"model":model["name"],"max_tokens":128,"messages":[{"role":"user","content":"Call echo with value ping."}],
                "tools":[{"name":"echo","description":"Echo a value","input_schema":tool}],"tool_choice":{"type":"tool","name":"echo"}})
        };
        let client=Client::builder().no_proxy().build().map_err(|e|store::fail(e.to_string()))?;
        let url=format!("http://127.0.0.1:{}/v1/{}",store::PORT,protocol);
        let first=client.post(&url).bearer_auth(&token).json(&body).send().await.map_err(|_|store::fail("测试网关连接失败"))?;
        if !first.status().is_success(){return Err(store::fail(format!("工具调用测试失败：HTTP {}",first.status().as_u16())));}
        let response:Value=first.json().await.map_err(|_|store::fail("工具调用响应无效"))?;
        if protocol=="responses"{
            let call=response["output"].as_array().into_iter().flatten().find(|v|v["type"]=="function_call" && v["name"]=="echo")
                .ok_or_else(||store::fail("上游未返回预期 function_call，不能判定工具兼容"))?;
            let mut input=vec![json!({"role":"user","content":"Call echo with value ping."})];
            input.extend(response["output"].as_array().cloned().unwrap_or_default());
            input.push(json!({"type":"function_call_output","call_id":call["call_id"],"output":"ping"}));
            body["input"]=json!(input);
            body["tool_choice"]=json!("none");
        }else{
            let call=response["content"].as_array().into_iter().flatten().find(|v|v["type"]=="tool_use" && v["name"]=="echo")
                .ok_or_else(||store::fail("上游未返回预期 tool_use，不能判定工具兼容"))?;
            body["messages"]=json!([{"role":"user","content":"Call echo with value ping."},
                {"role":"assistant","content":response["content"]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":call["id"],"content":"ping"}]}]);
            body["tool_choice"]=json!({"type":"auto"});
        }
        body["stream"]=json!(true);
        let second=client.post(&url).bearer_auth(&token).json(&body).send().await.map_err(|_|store::fail("工具结果测试连接失败"))?;
        if !second.status().is_success(){return Err(store::fail(format!("工具结果测试失败：HTTP {}",second.status().as_u16())));}
        let is_sse=second.headers().get("content-type").and_then(|v|v.to_str().ok()).is_some_and(|v|v.contains("text/event-stream"));
        let mut parser=stream::Parser::new(if protocol=="responses"{UpstreamFormat::OpenAiResponses}else{UpstreamFormat::Anthropic});
        stream::consume(second,is_sse,&mut parser,None,true,&mut None).await.map_err(store::fail)?;
        if parser.response.text.is_empty(){return Err(store::fail("工具结果返回后未收到完整文本，测试未通过"));}
        if store::config(&database::open(paths)?,provider)?["revision"].as_i64()!=Some(revision){
            return Err(store::fail("测试期间配置已变化，不能确认当前修订兼容"));
        }
        Ok::<_,ManagerError>(())
    }.await;
    {
        let c = database::open(paths)?;
        c.execute(
            "UPDATE service_gateway_tokens SET status='revoked',revoked_at=?2 WHERE instance_id=?1",
            params![instance, store::now()],
        )?;
        c.execute("UPDATE service_agent_instances SET status='detached',sync_status='restored' WHERE id=?1",[&instance])?;
        c.execute("INSERT INTO service_operations(id,operation_type,provider_id,agent_type,model_id,provider_revision,agent_version,
            client_protocol,upstream_protocol,capability,status,evidence_json,created_at,completed_at)
            VALUES(?1,'compatibility',?2,?3,?4,?5,?6,?7,?8,'tool_roundtrip_stream',?9,?10,?11,?11)",
            params![store::id(),provider,agent,store::text(&model,"id"),model["revision"].as_i64(),store::text(value,"agentVersion"),
                protocol,if upstream_format(&target)==UpstreamFormat::OpenAiChat{"chat"}else{"responses"},
                if result.is_ok(){"passed"}else{"failed"},
                json!({"instanceId":instance,"error":result.as_ref().err().map(ToString::to_string)}).to_string(),store::now()])?;
    }
    result?;
    Ok(json!({"status":"passed","capability":"tool_roundtrip_stream"}))
}

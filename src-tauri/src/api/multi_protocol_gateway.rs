use crate::api::{codex_account, codex_models, google_account, google_gateway, runtime_provider};
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use crate::core::provider_key_usage::KeyRequest;
use crate::core::provider_store;
use bytes::Bytes;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

mod stream;
type GatewayBody = UnsyncBoxBody<Bytes, Infallible>;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u64 = 8046;
const MAX_BODY_SIZE: usize = 32 * 1024 * 1024;
const MAX_LOGS: usize = 5000;
const CODEX_OFFICIAL_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const TOKEN_PREFIX: &str = "mpg_";

#[derive(Clone)]
pub struct GatewayServerRegistry {
    inner: Arc<Mutex<Option<(SocketAddr, tokio::task::JoinHandle<()>)>>>,
    store_lock: Arc<Mutex<()>>,
}

impl GatewayServerRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            store_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn ensure_started(
        &self,
        paths: &AppPaths,
        cli_targets: &Value,
    ) -> Result<(), ManagerError> {
        let config = read_config(paths)?;
        if config["enabled"] != true {
            return Ok(());
        }
        self.apply(paths, cli_targets, &config).await
    }

    async fn apply(
        &self,
        paths: &AppPaths,
        cli_targets: &Value,
        config: &Value,
    ) -> Result<(), ManagerError> {
        let address = bind_address(config)?;
        let mut guard = self.inner.lock().await;
        if guard
            .as_ref()
            .is_some_and(|(bound, task)| *bound == address && !task.is_finished())
        {
            return Ok(());
        }
        // 先绑定新端口，再替换监听任务；端口冲突时旧服务继续工作。
        let listener = TcpListener::bind(address).await?;
        let context = GatewayContext {
            paths: paths.clone(),
            cli_targets: cli_targets.clone(),
            store_lock: self.store_lock.clone(),
        };
        if let Some((_, task)) = guard.take() {
            task.abort();
            let _ = task.await;
        }
        *guard = Some((address, spawn_listener(listener, context)));
        Ok(())
    }

    pub async fn stop(&self) {
        if let Some((_, task)) = self.inner.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

#[derive(Clone)]
struct GatewayContext {
    paths: AppPaths,
    cli_targets: Value,
    store_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Chat,
    Responses,
    Claude,
    Gemini,
}

#[derive(Clone)]
struct GatewayRoute {
    protocol: Protocol,
    stream: bool,
    requested_model: String,
}

#[derive(Clone)]
struct GatewayTarget {
    id: String,
    name: String,
    provider_type: String,
    base_url: String,
    proxy: String,
    auth_field: String,
    headers: Value,
    cli: String,
    kind: TargetKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Provider,
    CodexAccount,
    GoogleAccount,
}

struct TargetAuth {
    token: String,
    key_id: String,
    account_id: String,
}

#[derive(Clone)]
struct CanonicalRequest {
    protocol: Protocol,
    original: Value,
    requested_model: String,
    messages: Vec<Value>,
    tools: Vec<Value>,
    temperature: Option<Value>,
    top_p: Option<Value>,
    max_tokens: Option<Value>,
    stream: bool,
}

#[derive(Default)]
struct ModelResponse {
    text: String,
    tool_calls: Vec<Value>,
    finish_reason: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    usage_known: bool,
    upstream_model: String,
    native: Option<Value>,
}

struct GatewayFailure {
    status: StatusCode,
    message: String,
}

impl From<ManagerError> for GatewayFailure {
    fn from(error: ManagerError) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: error.to_string(),
        }
    }
}

pub async fn start_enabled(
    registry: &GatewayServerRegistry,
    paths: &AppPaths,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    registry.ensure_started(paths, cli_targets).await
}

pub fn read_state(paths: &AppPaths) -> Result<Value, ManagerError> {
    let config = read_config(paths)?;
    let logs = read_logs(paths)?;
    let mut state = public_config(&config);
    state["baseUrl"] = json!(base_url(&state));
    state["models"] = json!(list_models(paths, &config)?);
    state["usage"] = config
        .get("totals")
        .cloned()
        .unwrap_or_else(|| usage_summary(&logs));
    state["byToken"] = json!(aggregate_logs(&logs, "tokenId"));
    state["byProvider"] = json!(aggregate_logs(&logs, "providerId"));
    state["byModel"] = json!(aggregate_logs(&logs, "physicalModel"));
    state["logs"] = json!(logs.iter().take(100).cloned().collect::<Vec<_>>());
    Ok(state)
}

pub async fn save_config(
    registry: &GatewayServerRegistry,
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let _guard = registry.store_lock.lock().await;
    let current = read_config(paths)?;
    let mut next = current.clone();
    for field in ["host", "port", "providerIds", "modelMappings", "enabled"] {
        if let Some(value) = payload.get(field) {
            next[field] = value.clone();
        }
    }
    validate_bind(&next)?;
    let next = normalize_config(&next);
    validate_mappings(paths, &next)?;
    if next["enabled"] == true {
        registry.apply(paths, cli_targets, &next).await?;
    }
    if let Err(error) = write_config(paths, &next).await {
        registry.stop().await;
        if current["enabled"] == true {
            registry.apply(paths, cli_targets, &current).await?;
        }
        return Err(error);
    }
    if next["enabled"] != true {
        registry.stop().await;
    }
    read_state(paths)
}

pub async fn enable(
    registry: &GatewayServerRegistry,
    paths: &AppPaths,
    cli_targets: &Value,
    mut payload: Value,
) -> Result<Value, ManagerError> {
    payload["enabled"] = json!(true);
    save_config(registry, paths, cli_targets, payload).await
}

pub async fn disable(
    registry: &GatewayServerRegistry,
    paths: &AppPaths,
) -> Result<Value, ManagerError> {
    save_config(registry, paths, &Value::Null, json!({"enabled": false})).await
}

pub async fn create_token(
    registry: &GatewayServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let _guard = registry.store_lock.lock().await;
    let mut config = read_config(paths)?;
    let mut tokens = config["tokens"].as_array().cloned().unwrap_or_default();
    let raw = format!("{}{}", TOKEN_PREFIX, uuid::Uuid::new_v4().simple());
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_millis();
    let item = json!({
        "id": id,
        "name": string_or(payload.get("name"), "未命名调用方"),
        "description": string_value(payload.get("description")),
        "tokenHint": token_hint(&raw),
        "tokenHash": hash_token(&raw),
        "enabled": payload.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        "createdAt": now,
        "expiresAt": number_value(payload.get("expiresAt"), 0),
        "requestCount": 0,
        "successCount": 0,
        "failureCount": 0,
        "inputTokens": 0,
        "outputTokens": 0,
        "lastUsedAt": 0
    });
    tokens.push(item.clone());
    config["tokens"] = json!(tokens);
    write_config(paths, &normalize_config(&config)).await?;
    let mut result = read_state(paths)?;
    result["createdToken"] = json!({
        "id": id,
        "name": item["name"],
        "token": raw,
        "tokenHint": item["tokenHint"]
    });
    Ok(result)
}

pub async fn delete_token(
    registry: &GatewayServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let _guard = registry.store_lock.lock().await;
    let token_id = string_value(payload.get("tokenId"));
    if token_id.is_empty() {
        return Err(ManagerError::System("缺少网关 Token ID".into()));
    }
    let mut config = read_config(paths)?;
    let mut tokens = config["tokens"].as_array().cloned().unwrap_or_default();
    let previous = tokens.len();
    tokens.retain(|item| item["id"].as_str() != Some(token_id.as_str()));
    if previous == tokens.len() {
        return Err(ManagerError::System("网关 Token 不存在".into()));
    }
    config["tokens"] = json!(tokens);
    write_config(paths, &normalize_config(&config)).await?;
    read_state(paths)
}

pub async fn set_token_enabled(
    registry: &GatewayServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let _guard = registry.store_lock.lock().await;
    let token_id = string_value(payload.get("tokenId"));
    let enabled = payload
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut config = read_config(paths)?;
    let tokens = config["tokens"]
        .as_array_mut()
        .ok_or_else(|| ManagerError::System("网关 Token 配置损坏".into()))?;
    let token = tokens
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(token_id.as_str()))
        .ok_or_else(|| ManagerError::System("网关 Token 不存在".into()))?;
    token["enabled"] = json!(enabled);
    write_config(paths, &normalize_config(&config)).await?;
    read_state(paths)
}

pub fn usage(paths: &AppPaths) -> Result<Value, ManagerError> {
    let logs = read_logs(paths)?;
    Ok(json!({
        "summary": read_config(paths)?["totals"],
        "byToken": aggregate_logs(&logs, "tokenId"),
        "byProvider": aggregate_logs(&logs, "providerId"),
        "byModel": aggregate_logs(&logs, "physicalModel"),
        "recent": logs.into_iter().take(200).collect::<Vec<_>>()
    }))
}

fn spawn_listener(listener: TcpListener, context: GatewayContext) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            while connections.try_join_next().is_some() {}
            let context = context.clone();
            connections.spawn(async move {
                let service = service_fn(move |request| {
                    let context = context.clone();
                    async move { Ok::<_, Infallible>(handle_request(request, context).await) }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    })
}

async fn handle_request(
    request: Request<Incoming>,
    context: GatewayContext,
) -> Response<GatewayBody> {
    let path = request.uri().path().to_string();
    if request.method() == Method::GET && path == "/health" {
        return json_response(
            StatusCode::OK,
            json!({"ok": true, "service": "multi-protocol-gateway"}),
        );
    }

    let config = match read_config(&context.paths) {
        Ok(value) => value,
        Err(error) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    if config["enabled"] != true {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "统一网关未开启");
    }

    let token = match authenticate(&request, &config) {
        Ok(token) => token,
        Err(response) => return response,
    };

    if request.method() == Method::GET
        && ["/v1/models", "/models", "/v1beta/models"].contains(&path.as_str())
    {
        return model_list_response(&context.paths, &config, path == "/v1beta/models");
    }

    if request.method() != Method::POST {
        return error_response(StatusCode::NOT_FOUND, "统一网关不支持该请求");
    }
    let Some(route) = parse_route(&request) else {
        return error_response(StatusCode::NOT_FOUND, "统一网关不支持该请求路径");
    };
    let started = Instant::now();
    let mut record = json!({
        "id": uuid::Uuid::new_v4().to_string(), "createdAt": now_millis(),
        "requestSource": "multi-protocol-gateway", "protocol": protocol_name(route.protocol),
        "tokenId": token["id"], "tokenName": token["name"],
        "requestedModel": route.requested_model, "physicalModel": "", "upstreamModel": "",
        "inputTokens": 0, "outputTokens": 0, "cacheReadTokens": 0, "usageKnown": false
    });
    let result = async {
        let body = http_body_util::Limited::new(request.into_body(), MAX_BODY_SIZE)
            .collect()
            .await
            .map_err(|_| GatewayFailure {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                message: "请求体过大或读取失败".into(),
            })?
            .to_bytes();
        let payload: Value = serde_json::from_slice(&body)
            .ok()
            .filter(Value::is_object)
            .ok_or_else(|| GatewayFailure {
                status: StatusCode::BAD_REQUEST,
                message: "请求体必须是 JSON 对象".into(),
            })?;
        record["requestedModel"] = json!(if route.requested_model.is_empty() {
            string_value(payload.get("model"))
        } else {
            route.requested_model.clone()
        });
        let canonical = canonical_request(&route, &payload).map_err(|error| GatewayFailure {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        })?;
        let requested_model = record["requestedModel"].as_str().unwrap_or("");
        let target = select_target(
            &context.paths,
            &config,
            &context.cli_targets,
            requested_model,
        )
        .map_err(|error| GatewayFailure {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: error.to_string(),
        })?;
        let physical_model = mapped_model(&config, &target.id, requested_model);
        record["physicalModel"] = json!(physical_model);
        record["providerId"] = json!(target.id);
        record["providerName"] = json!(target.name);
        record["providerType"] = json!(target.provider_type);
        send_to_target(
            &context,
            &target,
            &physical_model,
            &canonical,
            record.clone(),
            started,
        )
        .await
    }
    .await;
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

// 流结束后再落账；保留上游实际报告的模型与用量，缺失用量不伪造为已统计。
async fn finish_usage(
    context: &GatewayContext,
    mut record: Value,
    response: Option<&ModelResponse>,
    status: StatusCode,
    error: Option<&str>,
    started: Instant,
) {
    record["statusCode"] = json!(status.as_u16());
    record["ok"] = json!(error.is_none() && status.is_success());
    record["latencyMs"] = json!(started.elapsed().as_millis() as u64);
    if let Some(error) = error {
        record["errorMessage"] = json!(error);
    }
    if let Some(response) = response {
        record["inputTokens"] = json!(response.input_tokens);
        record["outputTokens"] = json!(response.output_tokens);
        record["cacheReadTokens"] = json!(response.cache_read_tokens);
        record["usageKnown"] = json!(response.usage_known);
        record["upstreamModel"] = json!(response.upstream_model);
    }
    if let Err(error) = update_usage(context, record).await {
        eprintln!("统一网关用量保存失败：{error}");
    }
}

fn authenticate(
    request: &Request<Incoming>,
    config: &Value,
) -> Result<Value, Response<GatewayBody>> {
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| {
            request
                .headers()
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
        })
        .or_else(|| {
            request
                .headers()
                .get("x-goog-api-key")
                .and_then(|value| value.to_str().ok())
        })
        .unwrap_or("");
    if supplied.is_empty() {
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "缺少统一网关 Token",
        ));
    }
    let hash = hash_token(supplied);
    let now = now_millis();
    let token = config["tokens"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| {
            item["enabled"] != false
                && item["tokenHash"].as_str() == Some(hash.as_str())
                && (number_value(item.get("expiresAt"), 0) == 0
                    || number_value(item.get("expiresAt"), 0) > now)
        })
        .cloned();
    token.ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "统一网关 Token 无效或已过期"))
}

fn parse_route(request: &Request<Incoming>) -> Option<GatewayRoute> {
    let path = request.uri().path();
    let query = request.uri().query().unwrap_or("");
    match path {
        "/v1/chat/completions" | "/chat/completions" => Some(GatewayRoute {
            protocol: Protocol::Chat,
            stream: query.contains("stream=true"),
            requested_model: String::new(),
        }),
        "/v1/responses" | "/responses" => Some(GatewayRoute {
            protocol: Protocol::Responses,
            stream: query.contains("stream=true"),
            requested_model: String::new(),
        }),
        "/v1/messages" | "/messages" => Some(GatewayRoute {
            protocol: Protocol::Claude,
            stream: query.contains("stream=true"),
            requested_model: String::new(),
        }),
        path if path.starts_with("/v1beta/models/") && path.ends_with(":generateContent") => {
            let model = path
                .trim_start_matches("/v1beta/models/")
                .trim_end_matches(":generateContent")
                .to_string();
            Some(GatewayRoute {
                protocol: Protocol::Gemini,
                stream: false,
                requested_model: model,
            })
        }
        path if path.starts_with("/v1beta/models/") && path.ends_with(":streamGenerateContent") => {
            let model = path
                .trim_start_matches("/v1beta/models/")
                .trim_end_matches(":streamGenerateContent")
                .to_string();
            Some(GatewayRoute {
                protocol: Protocol::Gemini,
                stream: true,
                requested_model: model,
            })
        }
        _ => None,
    }
}

fn canonical_request(
    route: &GatewayRoute,
    payload: &Value,
) -> Result<CanonicalRequest, ManagerError> {
    if payload
        .get("previous_response_id")
        .is_some_and(|v| !v.is_null())
        || payload.get("conversation").is_some_and(|v| !v.is_null())
    {
        return Err(ManagerError::System(
            "统一网关请发送完整历史，暂不支持远程会话引用".into(),
        ));
    }
    if payload["n"].as_u64().is_some_and(|n| n != 1) {
        return Err(ManagerError::System("统一网关每次只生成一个候选".into()));
    }
    let mut request = match route.protocol {
        Protocol::Chat => canonical_chat(payload),
        Protocol::Responses => canonical_responses(payload),
        Protocol::Claude => canonical_claude(payload),
        Protocol::Gemini => canonical_gemini(payload, &route.requested_model),
    }?;
    if request.requested_model.is_empty() {
        return Err(ManagerError::System("缺少 model".into()));
    }
    request.stream |= route.stream;
    Ok(request)
}

fn canonical_chat(payload: &Value) -> Result<CanonicalRequest, ManagerError> {
    let messages = payload["messages"]
        .as_array()
        .cloned()
        .ok_or_else(|| ManagerError::System("messages 必须是数组".into()))?;
    Ok(CanonicalRequest {
        protocol: Protocol::Chat,
        original: payload.clone(),
        requested_model: string_value(payload.get("model")),
        messages,
        tools: payload["tools"].as_array().cloned().unwrap_or_default(),
        temperature: payload.get("temperature").cloned(),
        top_p: payload.get("top_p").cloned(),
        max_tokens: payload
            .get("max_tokens")
            .cloned()
            .or_else(|| payload.get("max_completion_tokens").cloned()),
        stream: payload["stream"] == true,
    })
}

fn canonical_responses(payload: &Value) -> Result<CanonicalRequest, ManagerError> {
    let mut messages = Vec::new();
    if let Some(instructions) = payload.get("instructions").and_then(Value::as_str) {
        if !instructions.is_empty() {
            messages.push(json!({"role": "system", "content": instructions}));
        }
    }
    match payload.get("input") {
        Some(Value::String(text)) => messages.push(json!({"role": "user", "content": text})),
        Some(Value::Array(items)) => {
            for item in items {
                match string_value(item.get("type")).as_str() {
                    "message" | "" if item.get("role").is_some() => {
                        let role = string_or(item.get("role"), "user");
                        messages.push(json!({"role": role, "content": item["content"]}));
                    }
                    "function_call" => {
                        let call_id = string_or(item.get("call_id"), "");
                        messages.push(json!({
                            "role": "assistant",
                            "tool_calls": [{
                                "id": if call_id.is_empty() {
                                    string_or(item.get("id"), "call")
                                } else {
                                    call_id
                                },
                                "type": "function",
                                "function": {
                                    "name": string_value(item.get("name")),
                                    "arguments": string_or(item.get("arguments"), "{}")
                                }
                            }]
                        }));
                    }
                    "function_call_output" => {
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": string_value(item.get("call_id")),
                            "content": string_value(item.get("output"))
                        }));
                    }
                    _ => {} // 同协议保留原始输入项；跨协议在构建请求时验证。
                }
            }
        }
        Some(_) => return Err(ManagerError::System("Responses input 格式无效".into())),
        None => {}
    }
    Ok(CanonicalRequest {
        protocol: Protocol::Responses,
        original: payload.clone(),
        requested_model: string_value(payload.get("model")),
        messages,
        tools: payload["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|tool| {
                let mut function = tool.clone();
                if let Some(object) = function.as_object_mut() {
                    object.remove("type");
                }
                json!({"type":"function","function":function})
            })
            .collect(),
        temperature: payload.get("temperature").cloned(),
        top_p: payload.get("top_p").cloned(),
        max_tokens: payload
            .get("max_output_tokens")
            .cloned()
            .or_else(|| payload.get("max_tokens").cloned()),
        stream: payload["stream"] == true,
    })
}

fn canonical_claude(payload: &Value) -> Result<CanonicalRequest, ManagerError> {
    if !payload["messages"].is_array() {
        return Err(ManagerError::System("messages 必须是数组".into()));
    }
    // 同协议请求保留原生内容；只有选中跨协议目标后才调用现有转换器。
    Ok(CanonicalRequest {
        protocol: Protocol::Claude,
        original: payload.clone(),
        requested_model: string_value(payload.get("model")),
        messages: Vec::new(),
        tools: Vec::new(),
        temperature: payload.get("temperature").cloned(),
        top_p: payload.get("top_p").cloned(),
        max_tokens: payload.get("max_tokens").cloned(),
        stream: payload["stream"] == true,
    })
}

fn canonical_gemini(payload: &Value, route_model: &str) -> Result<CanonicalRequest, ManagerError> {
    let mut messages = Vec::new();
    let mut pending_calls: Vec<(String, String)> = Vec::new();
    if let Some(system) = payload.get("systemInstruction") {
        let text = response_item_text(system.get("parts"));
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }
    for content in payload["contents"]
        .as_array()
        .ok_or_else(|| ManagerError::System("contents 必须是数组".into()))?
    {
        let role = if content["role"] == "model" {
            "assistant"
        } else {
            "user"
        };
        let mut text = Vec::new();
        let mut calls = Vec::new();
        for part in content["parts"].as_array().into_iter().flatten() {
            if let Some(value) = part.get("text").and_then(Value::as_str) {
                text.push(value.to_string());
            } else if let Some(call) = part.get("functionCall") {
                let name = string_value(call.get("name"));
                let id = string_or(
                    call.get("id"),
                    &format!("call_{}", uuid::Uuid::new_v4().simple()),
                );
                pending_calls.push((name, id.clone()));
                calls.push(json!({"id":id,"type":"function","function":{"name":call["name"],"arguments":call["args"].to_string()}}));
            } else if let Some(output) = part.get("functionResponse") {
                let name = string_value(output.get("name"));
                let explicit = string_value(output.get("id"));
                let matching = pending_calls.iter().position(|(call_name, id)| {
                    if explicit.is_empty() {
                        *call_name == name
                    } else {
                        *id == explicit
                    }
                });
                let id = matching
                    .map(|index| pending_calls.remove(index).1)
                    .unwrap_or(explicit);
                messages.push(json!({"role":"tool","tool_call_id":id,"content":output["response"].to_string()}));
            } else {
                messages.push(json!({"role":role,"content":[{"type":"unsupported_gemini_part"}]}));
            }
        }
        if !text.is_empty() || !calls.is_empty() {
            let mut message = json!({"role":role,"content":join_text(text)});
            if !calls.is_empty() {
                message["tool_calls"] = json!(calls);
            }
            messages.push(message);
        }
    }
    let generation = payload.get("generationConfig").unwrap_or(&Value::Null);
    Ok(CanonicalRequest {
        protocol: Protocol::Gemini,
        original: payload.clone(),
        requested_model: string_or(payload.get("model"), route_model),
        messages,
        tools: payload["tools"].as_array().into_iter().flatten().flat_map(|tool| tool["functionDeclarations"].as_array().into_iter().flatten()).map(|function| json!({"type":"function","function":{"name":function["name"],"description":function["description"],"parameters":function.get("parameters").or_else(||function.get("parametersJsonSchema")).cloned().unwrap_or_else(||json!({"type":"object","properties":{}}))}})).collect(),
        temperature: generation.get("temperature").cloned(),
        top_p: generation.get("topP").cloned(),
        max_tokens: generation.get("maxOutputTokens").cloned(),
        stream: false,
    })
}

async fn send_to_target(
    context: &GatewayContext,
    target: &GatewayTarget,
    physical_model: &str,
    request: &CanonicalRequest,
    record: Value,
    started: Instant,
) -> Result<Response<GatewayBody>, GatewayFailure> {
    let auth = target_auth(context, target).await?;
    let client = target_client(target)?;
    let (url, body, headers, format, mut tracking) =
        build_upstream_request(context, target, &auth, physical_model, request).map_err(
            |error| GatewayFailure {
                status: StatusCode::BAD_REQUEST,
                message: error.to_string(),
            },
        )?;
    if let Some(tracking) = &mut tracking {
        tracking.start();
    }
    let upstream = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|_| GatewayFailure {
            status: StatusCode::BAD_GATEWAY,
            message: "无法连接上游，请检查目标地址、网络与代理配置".into(),
        })?;
    let status = upstream.status();
    let is_sse = upstream
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/event-stream"));
    if let Some(tracking) = &mut tracking {
        tracking.response(status.as_u16(), is_sse);
    }
    if !status.is_success() {
        if let Some(tracking) = &mut tracking {
            tracking.fail("http");
        }
        return Err(GatewayFailure {
            status,
            message: format!(
                "上游返回 HTTP {}（{}）",
                status.as_u16(),
                status.canonical_reason().unwrap_or("请求失败")
            ),
        });
    }
    if request.stream {
        let context = context.clone();
        let request = request.clone();
        let model = physical_model.to_string();
        let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Frame<Bytes>, Infallible>>(16);
        tokio::spawn(async move {
            let mut parser = stream::Parser::new(format);
            let mut encoder = stream::Encoder::new(request.protocol, &model);
            let result = stream::consume(
                upstream,
                is_sse,
                &mut parser,
                Some((&sender, &mut encoder)),
                native_protocol(request.protocol, format),
                &mut tracking,
            )
            .await;
            let error = result.err();
            if let Some(tracking) = &mut tracking {
                if error.is_some() {
                    tracking.fail("stream");
                } else {
                    tracking.finish();
                }
            }
            // 客户端已经收到 200 时，记录生成失败并发送协议内错误事件。
            if let Some(error) = &error {
                let _ = sender
                    .send(Ok(Frame::data(Bytes::from(encoder.error(error)))))
                    .await;
            }
            finish_usage(
                &context,
                record,
                Some(&parser.response),
                if error.is_some() {
                    StatusCode::BAD_GATEWAY
                } else {
                    status
                },
                error.as_deref(),
                started,
            )
            .await;
        });
        let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|frame| (frame, receiver))
        });
        return Ok(Response::builder()
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("x-accel-buffering", "no")
            .body(StreamBody::new(stream).boxed_unsync())
            .unwrap());
    }
    let mut parser = stream::Parser::new(format);
    let result = stream::consume(
        upstream,
        is_sse,
        &mut parser,
        None,
        native_protocol(request.protocol, format),
        &mut tracking,
    )
    .await;
    if let Err(message) = result {
        if let Some(tracking) = &mut tracking {
            tracking.fail("response");
        }
        // 保留已经收到的部分用量，但只在这里记一次失败。
        finish_usage(
            context,
            record,
            Some(&parser.response),
            StatusCode::BAD_GATEWAY,
            Some(&message),
            started,
        )
        .await;
        return Ok(error_response(StatusCode::BAD_GATEWAY, &message));
    }
    if let Some(tracking) = &mut tracking {
        tracking.finish();
    }
    let body =
        if native_protocol(request.protocol, format) {
            parser.response.native.clone().unwrap_or_else(|| {
                response_value(request.protocol, &parser.response, physical_model)
            })
        } else {
            response_value(request.protocol, &parser.response, physical_model)
        };
    finish_usage(
        context,
        record,
        Some(&parser.response),
        status,
        None,
        started,
    )
    .await;
    Ok(json_response(StatusCode::OK, body))
}

fn native_protocol(protocol: Protocol, format: UpstreamFormat) -> bool {
    matches!(
        (protocol, format),
        (Protocol::Chat, UpstreamFormat::OpenAiChat)
            | (Protocol::Responses, UpstreamFormat::OpenAiResponses)
            | (Protocol::Claude, UpstreamFormat::Anthropic)
            | (Protocol::Gemini, UpstreamFormat::Gemini)
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpstreamFormat {
    OpenAiChat,
    OpenAiResponses,
    Anthropic,
    Gemini,
}

fn upstream_format(target: &GatewayTarget) -> UpstreamFormat {
    if target.kind != TargetKind::Provider || target.cli == "codex" {
        return UpstreamFormat::OpenAiResponses;
    }
    match target.provider_type.as_str() {
        "anthropic" => UpstreamFormat::Anthropic,
        "gemini" => UpstreamFormat::Gemini,
        _ if target.cli == "claude" => UpstreamFormat::Anthropic,
        _ => UpstreamFormat::OpenAiChat,
    }
}

fn upstream_body(
    model: &str,
    request: &CanonicalRequest,
    format: UpstreamFormat,
) -> Result<Value, ManagerError> {
    let native = native_protocol(request.protocol, format);
    let converted;
    let request = if !native && request.protocol == Protocol::Claude {
        let value = crate::api::claude_desktop::gateway_chat_request(&request.original)?;
        converted = CanonicalRequest {
            protocol: Protocol::Claude,
            original: request.original.clone(),
            ..canonical_chat(&value)?
        };
        &converted
    } else {
        request
    };
    let mut body = if native {
        request.original.clone()
    } else {
        // 不能无损转换的输入明确拒绝，防止图片、思考签名或结构化约束静默丢失。
        if request.protocol == Protocol::Claude
            && request.original["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|message| {
                    message["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|part| {
                            matches!(
                                part["type"].as_str(),
                                Some("thinking" | "redacted_thinking")
                            )
                        })
                })
        {
            return Err(ManagerError::System(
                "跨协议暂不支持思考历史，请使用同协议目标".into(),
            ));
        }
        if request.protocol == Protocol::Gemini
            && request.original["contents"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|content| {
                    content["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|part| {
                            part.as_object().is_some_and(|part| {
                                part.keys().any(|key| {
                                    !matches!(
                                        key.as_str(),
                                        "text" | "functionCall" | "functionResponse"
                                    )
                                })
                            })
                        })
                })
        {
            return Err(ManagerError::System(
                "跨协议暂不支持此 Gemini 内容块".into(),
            ));
        }
        if request.protocol == Protocol::Responses
            && request.original["input"].as_array().is_some_and(|items| {
                items.iter().any(|item| {
                    !matches!(
                        item["type"].as_str().unwrap_or("message"),
                        "message" | "function_call" | "function_call_output"
                    )
                })
            })
        {
            return Err(ManagerError::System(
                "跨协议转发不支持此 Responses 输入项".into(),
            ));
        }
        if request.protocol == Protocol::Gemini
            && request.original["tools"].as_array().is_some_and(|tools| {
                tools.iter().any(|tool| {
                    tool.as_object().is_some_and(|object| {
                        object.keys().any(|key| key != "functionDeclarations")
                    })
                })
            })
        {
            return Err(ManagerError::System(
                "跨协议转发只支持自定义函数工具".into(),
            ));
        }
        for key in [
            "thinking",
            "reasoning",
            "response_format",
            "text",
            "audio",
            "stop",
            "stop_sequences",
            "tool_choice",
            "toolConfig",
            "parallel_tool_calls",
            "seed",
            "presence_penalty",
            "frequency_penalty",
            "logprobs",
            "top_logprobs",
            "truncation",
            "safetySettings",
            "cachedContent",
        ] {
            if request.original.get(key).is_some_and(|v| !v.is_null()) {
                return Err(ManagerError::System(format!(
                    "跨协议转发暂不支持 {key}，请使用同协议目标"
                )));
            }
        }
        if request.original["generationConfig"]
            .as_object()
            .is_some_and(|config| {
                config
                    .keys()
                    .any(|key| !matches!(key.as_str(), "temperature" | "topP" | "maxOutputTokens"))
            })
        {
            return Err(ManagerError::System(
                "跨协议转发暂不支持此 Gemini 生成配置".into(),
            ));
        }
        for message in &request.messages {
            if message["role"] == "tool" && string_value(message.get("tool_call_id")).is_empty() {
                return Err(ManagerError::System(
                    "工具结果缺少对应的函数调用，请发送完整历史".into(),
                ));
            }
            if let Some(parts) = message["content"].as_array() {
                if parts.iter().any(|part| {
                    !matches!(
                        part["type"].as_str(),
                        Some("text" | "input_text" | "output_text")
                    )
                }) {
                    return Err(ManagerError::System(
                        "跨协议转发目前只支持文本和自定义函数调用".into(),
                    ));
                }
            }
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                if call["function"]["name"].as_str().is_none()
                    || serde_json::from_str::<Value>(
                        call["function"]["arguments"].as_str().unwrap_or(""),
                    )
                    .is_err()
                {
                    return Err(ManagerError::System(
                        "函数调用必须包含名称和有效 JSON 参数".into(),
                    ));
                }
            }
        }
        if request
            .tools
            .iter()
            .any(|tool| tool["function"]["name"].as_str().is_none())
        {
            return Err(ManagerError::System(
                "跨协议转发只支持自定义函数工具".into(),
            ));
        }
        match format {
            UpstreamFormat::OpenAiChat => chat_body(model, request),
            UpstreamFormat::OpenAiResponses => responses_body(model, request),
            UpstreamFormat::Anthropic => anthropic_body(model, request),
            UpstreamFormat::Gemini => gemini_body(model, request)?,
        }
    };
    if format != UpstreamFormat::Gemini {
        body["model"] = json!(model);
        body["stream"] = json!(request.stream);
        if format == UpstreamFormat::OpenAiResponses {
            body["store"] = json!(false);
        }
        if format == UpstreamFormat::OpenAiChat && request.stream {
            body["stream_options"] = json!({"include_usage":true});
        }
    } else if let Some(object) = body.as_object_mut() {
        object.remove("model");
        object.remove("stream");
    }
    Ok(body)
}

fn build_upstream_request(
    context: &GatewayContext,
    target: &GatewayTarget,
    auth: &TargetAuth,
    model: &str,
    request: &CanonicalRequest,
) -> Result<(String, Value, HeaderMap, UpstreamFormat, Option<KeyRequest>), ManagerError> {
    let format = upstream_format(target);
    let mut body = upstream_body(model, request, format)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept",
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("accept-encoding", HeaderValue::from_static("identity"));
    add_custom_headers(&mut headers, &target.headers)?;
    let auth_name =
        if format == UpstreamFormat::Anthropic && target.auth_field != "ANTHROPIC_AUTH_TOKEN" {
            "x-api-key"
        } else if format == UpstreamFormat::Gemini {
            "x-goog-api-key"
        } else {
            "authorization"
        };
    let auth_value = if auth_name == "authorization" {
        format!("Bearer {}", auth.token)
    } else {
        auth.token.clone()
    };
    headers.insert(
        auth_name,
        HeaderValue::from_str(&auth_value).map_err(|e| ManagerError::System(e.to_string()))?,
    );
    if format == UpstreamFormat::Anthropic {
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    }
    let url = match target.kind {
        TargetKind::CodexAccount => {
            headers.insert(
                "chatgpt-account-id",
                HeaderValue::from_str(&auth.account_id)
                    .map_err(|e| ManagerError::System(e.to_string()))?,
            );
            headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
            body["stream"] = json!(true);
            if body.get("instructions").is_none() {
                body["instructions"] = json!("");
            }
            format!("{CODEX_OFFICIAL_BASE_URL}/responses")
        }
        TargetKind::GoogleAccount => format!(
            "http://127.0.0.1:{}/v1/responses",
            google_port(context, target)?
        ),
        TargetKind::Provider => match format {
            UpstreamFormat::OpenAiChat => append_path(&target.base_url, "/chat/completions"),
            UpstreamFormat::OpenAiResponses => append_path(&target.base_url, "/responses"),
            UpstreamFormat::Anthropic => append_path(&target.base_url, "/messages"),
            UpstreamFormat::Gemini => {
                let url = append_gemini_path(&target.base_url, model);
                if request.stream {
                    format!(
                        "{}?alt=sse",
                        url.replace(":generateContent", ":streamGenerateContent")
                    )
                } else {
                    url
                }
            }
        },
    };
    let tracking = if target.kind == TargetKind::Provider {
        Some(KeyRequest::new(
            &context.paths,
            &target.id,
            &auth.key_id,
            &auth.token,
        ))
    } else {
        None
    };
    Ok((url, body, headers, format, tracking))
}

async fn target_auth(
    context: &GatewayContext,
    target: &GatewayTarget,
) -> Result<TargetAuth, ManagerError> {
    match target.kind {
        TargetKind::CodexAccount => {
            let cli_target = runtime_provider::find_cli_target(&context.cli_targets, "codex")?;
            let auth = codex_account::get_proxy_auth(
                &context.paths,
                target.id.trim_start_matches("account:"),
                &cli_target,
            )
            .await?;
            Ok(TargetAuth {
                token: string_value(auth.get("accessToken")),
                key_id: String::new(),
                account_id: string_value(auth.get("accountId")),
            })
        }
        TargetKind::GoogleAccount => {
            let provider = google_account::find_account(&context.paths, &target.id)?;
            google_gateway::ensure_started(&context.paths, &provider).await?;
            let (_, token) =
                runtime_provider::get_provider_api_key_with_id(&context.paths, &target.id)?;
            Ok(TargetAuth {
                token,
                key_id: String::new(),
                account_id: String::new(),
            })
        }
        TargetKind::Provider => {
            let (key_id, token) =
                runtime_provider::get_provider_api_key_with_id(&context.paths, &target.id)?;
            if token.is_empty() {
                return Err(ManagerError::System("Provider 缺少 API Key".into()));
            }
            Ok(TargetAuth {
                token,
                key_id,
                account_id: String::new(),
            })
        }
    }
}

fn target_client(target: &GatewayTarget) -> Result<Client, ManagerError> {
    let mut builder = Client::builder().redirect(reqwest::redirect::Policy::none());
    if target.kind == TargetKind::GoogleAccount {
        builder = builder.no_proxy();
    } else if !target.proxy.is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(&target.proxy)
                .map_err(|error| ManagerError::System(error.to_string()))?,
        );
    }
    builder
        .build()
        .map_err(|error| ManagerError::System(error.to_string()))
}

fn select_target(
    paths: &AppPaths,
    config: &Value,
    cli_targets: &Value,
    requested_model: &str,
) -> Result<GatewayTarget, ManagerError> {
    let ids = target_ids(paths, config)?;
    if ids.is_empty() {
        return Err(ManagerError::System(
            "统一网关没有可用的 Provider 或官方账号".into(),
        ));
    }
    let mapping = config["modelMappings"].get(requested_model);
    if let Some(mapping) = mapping {
        let id = string_or(
            mapping
                .get("targetId")
                .or_else(|| mapping.get("providerId")),
            "",
        );
        if !id.is_empty() {
            if !ids.contains(&id) {
                return Err(ManagerError::System("模型路由目标不在底层池中".into()));
            }
            return build_target(paths, cli_targets, &id);
        }
    }
    for id in &ids {
        let Ok(target) = build_target(paths, cli_targets, id) else {
            continue;
        };
        if model_is_available(paths, &target, requested_model) {
            return Ok(target);
        }
    }
    Err(ManagerError::System(format!(
        "底层池没有匹配模型 {requested_model}，请配置模型路由"
    )))
}

fn build_target(
    paths: &AppPaths,
    _cli_targets: &Value,
    id: &str,
) -> Result<GatewayTarget, ManagerError> {
    if let Some(account_id) = id.strip_prefix("account:") {
        let account = provider_store::read_codex_accounts(paths)?
            .into_iter()
            .find(|item| string_value(item.get("id")) == account_id)
            .ok_or_else(|| ManagerError::System("Codex 官方账号不存在".into()))?;
        if account["disabled"] == true {
            return Err(ManagerError::System("Codex 官方账号已禁用".into()));
        }
        return Ok(GatewayTarget {
            id: id.to_string(),
            name: string_or(account.get("email"), account_id),
            provider_type: "codex-account".into(),
            base_url: CODEX_OFFICIAL_BASE_URL.into(),
            proxy: string_value(account.get("proxy")),
            auth_field: String::new(),
            headers: json!({}),
            cli: "codex".into(),
            kind: TargetKind::CodexAccount,
        });
    }
    let provider = runtime_provider::find_provider(paths, id)?;
    if provider["enabled"] == false {
        return Err(ManagerError::System("Provider 已禁用".into()));
    }
    let kind = if provider["type"] == "google-account" {
        TargetKind::GoogleAccount
    } else {
        TargetKind::Provider
    };
    Ok(GatewayTarget {
        id: id.to_string(),
        name: string_or(provider.get("name"), id),
        provider_type: string_or(provider.get("type"), "custom"),
        base_url: string_value(provider.get("baseUrl")),
        proxy: string_value(provider.get("proxy")),
        auth_field: string_or(provider.get("authField"), "ANTHROPIC_API_KEY"),
        headers: provider
            .get("headers")
            .cloned()
            .unwrap_or_else(|| json!({})),
        cli: string_value(provider.get("cli")),
        kind,
    })
}

fn target_ids(_paths: &AppPaths, config: &Value) -> Result<Vec<String>, ManagerError> {
    let configured = config["providerIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(configured)
}

fn model_is_available(paths: &AppPaths, target: &GatewayTarget, requested: &str) -> bool {
    list_target_models(paths, target)
        .iter()
        .any(|model| model == requested)
}

fn list_target_models(paths: &AppPaths, target: &GatewayTarget) -> Vec<String> {
    if target.kind == TargetKind::CodexAccount {
        // 复用已有 Codex 模型目录，避免网关维护另一份易过期的硬编码模型名。
        return runtime_provider::codex_model_list(&json!({}));
    }
    let mut names = provider_store::read_models(paths)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| item["providerId"] == target.id)
        .filter_map(|item| item["name"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    if let Ok(provider) = runtime_provider::find_provider(paths, &target.id) {
        if let Some(model) = provider["runtimeConfig"]["mainModel"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            names.push(model.to_string());
        }
        for model in provider["google"]["models"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(name) = model["id"].as_str() {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn mapped_model(config: &Value, target_id: &str, requested: &str) -> String {
    config["modelMappings"]
        .get(requested)
        .and_then(|mapping| {
            if string_or(
                mapping
                    .get("targetId")
                    .or_else(|| mapping.get("providerId")),
                "",
            ) == target_id
            {
                Some(string_or(
                    mapping
                        .get("model")
                        .or_else(|| mapping.get("physicalModel")),
                    requested,
                ))
            } else {
                None
            }
        })
        .unwrap_or_else(|| requested.to_string())
}

fn chat_body(model: &str, request: &CanonicalRequest) -> Value {
    let messages = request
        .messages
        .iter()
        .map(|message| {
            let mut message = message.clone();
            if message["content"].is_array() {
                message["content"] = json!(content_text(message.get("content")));
            }
            message
        })
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": request.stream
    });
    if !request.tools.is_empty() {
        body["tools"] = json!(request.tools);
    }
    if request.stream {
        body["stream_options"] = json!({"include_usage":true});
    }
    copy_optional(&mut body, "temperature", request.temperature.clone());
    copy_optional(&mut body, "top_p", request.top_p.clone());
    copy_optional(&mut body, "max_tokens", request.max_tokens.clone());
    body
}

fn responses_body(model: &str, request: &CanonicalRequest) -> Value {
    let mut input = Vec::new();
    let mut instructions = Vec::new();
    for message in &request.messages {
        let role = string_or(message.get("role"), "user");
        let text = content_text(message.get("content"));
        if role == "system" || role == "developer" {
            instructions.push(text);
            continue;
        }
        if role != "tool" && !text.is_empty() {
            input.push(json!({
                "type": "message",
                "role": role,
                "content": [{"type": if role == "assistant" { "output_text" } else { "input_text" }, "text": text}]
            }));
        }
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            input.push(json!({
                "type": "function_call",
                "call_id": string_or(call.get("id"), "call"),
                "name": string_value(call["function"].get("name")),
                "arguments": string_or(call["function"].get("arguments"), "{}")
            }));
        }
        if role == "tool" {
            input.push(json!({
                "type": "function_call_output",
                "call_id": string_value(message.get("tool_call_id")),
                "output": text
            }));
        }
    }
    let mut body =
        json!({"model": model, "input": input, "stream": request.stream, "store": false});
    if !instructions.is_empty() {
        body["instructions"] = json!(instructions.join("\n"));
    }
    if !request.tools.is_empty() {
        body["tools"] = json!(request
            .tools
            .iter()
            .map(|tool| {
                let mut function = tool["function"].clone();
                function["type"] = json!("function");
                function
            })
            .collect::<Vec<_>>());
    }
    copy_optional(&mut body, "temperature", request.temperature.clone());
    copy_optional(&mut body, "top_p", request.top_p.clone());
    copy_optional(&mut body, "max_output_tokens", request.max_tokens.clone());
    body
}

fn anthropic_body(model: &str, request: &CanonicalRequest) -> Value {
    let system = request
        .messages
        .iter()
        .filter(|message| message["role"] == "system" || message["role"] == "developer")
        .map(|message| content_text(message.get("content")))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let messages = request
        .messages
        .iter()
        .filter(|message| message["role"] != "system" && message["role"] != "developer")
        .map(|message| {
            let role = string_or(message.get("role"), "user");
            let text = content_text(message.get("content"));
            if role == "tool" { return json!({"role":"user","content":[{"type":"tool_result","tool_use_id":message["tool_call_id"],"content":text}]}); }
            let mut content = Vec::new();
            if !text.is_empty() {
                content.push(json!({"type": "text", "text": text}));
            }
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                content.push(json!({
                    "type": "tool_use",
                    "id": string_or(call.get("id"), "call"),
                    "name": string_value(call["function"].get("name")),
                    "input": serde_json::from_str::<Value>(&string_or(call["function"].get("arguments"), "{}")).unwrap_or_else(|_| json!({}))
                }));
            }
            let output_role = if role == "tool" {
                "user".to_string()
            } else {
                role
            };
            json!({"role": output_role, "content": content})
        })
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": request.max_tokens.clone().unwrap_or_else(|| json!(4096)),
        "stream": request.stream
    });
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    if !request.tools.is_empty() {
        body["tools"] = json!(request.tools.iter().map(|tool| json!({"name":tool["function"]["name"],"description":tool["function"]["description"].as_str().unwrap_or(""),"input_schema":tool["function"]["parameters"]})).collect::<Vec<_>>());
    }
    copy_optional(&mut body, "temperature", request.temperature.clone());
    copy_optional(&mut body, "top_p", request.top_p.clone());
    body
}

fn gemini_body(model: &str, request: &CanonicalRequest) -> Result<Value, ManagerError> {
    let converted = crate::api::google_protocol::to_google(&responses_body(model, request), "")
        .map_err(ManagerError::System)?;
    Ok(converted["request"].clone())
}

fn parse_upstream_response(
    value: &Value,
    format: UpstreamFormat,
) -> Result<ModelResponse, ManagerError> {
    match format {
        UpstreamFormat::OpenAiChat => {
            let choice = value["choices"]
                .as_array()
                .and_then(|items| items.first())
                .ok_or_else(|| ManagerError::System("OpenAI 上游缺少 choices".into()))?;
            let message = &choice["message"];
            Ok(ModelResponse {
                native: Some(value.clone()),
                usage_known: value["usage"].is_object(),
                upstream_model: string_value(value.get("model")),
                text: content_text(message.get("content")),
                tool_calls: message["tool_calls"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
                finish_reason: string_or(choice.get("finish_reason"), "stop"),
                input_tokens: value["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: value["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                cache_read_tokens: value["usage"]["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0),
            })
        }
        UpstreamFormat::OpenAiResponses => {
            let mut text = String::new();
            let mut calls = Vec::new();
            for item in value["output"].as_array().into_iter().flatten() {
                match string_value(item.get("type")).as_str() {
                    "message" => {
                        text.push_str(&response_item_text(item.get("content")));
                    }
                    "function_call" => calls.push(json!({
                        "id": string_or(item.get("call_id"), "call"),
                        "type": "function",
                        "function": {
                            "name": string_value(item.get("name")),
                            "arguments": string_or(item.get("arguments"), "{}")
                        }
                    })),
                    _ => {}
                }
            }
            Ok(ModelResponse {
                native: Some(value.clone()),
                usage_known: value["usage"].is_object(),
                upstream_model: string_value(value.get("model")),
                text,
                tool_calls: calls,
                finish_reason: if value["status"] == "incomplete" {
                    "length".into()
                } else {
                    "stop".into()
                },
                input_tokens: value["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output_tokens: value["usage"]["output_tokens"].as_u64().unwrap_or(0),
                cache_read_tokens: value["usage"]["input_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0),
            })
        }
        UpstreamFormat::Anthropic => {
            let mut text = String::new();
            let mut calls = Vec::new();
            for block in value["content"].as_array().into_iter().flatten() {
                match string_value(block.get("type")).as_str() {
                    "text" => text.push_str(block["text"].as_str().unwrap_or("")),
                    "tool_use" => calls.push(json!({
                        "id": string_value(block.get("id")),
                        "type": "function",
                        "function": {
                            "name": string_value(block.get("name")),
                            "arguments": block.get("input").cloned().unwrap_or_else(|| json!({})).to_string()
                        }
                    })),
                    _ => {}
                }
            }
            Ok(ModelResponse {
                native: Some(value.clone()),
                usage_known: value["usage"].is_object(),
                upstream_model: string_value(value.get("model")),
                text,
                tool_calls: calls,
                finish_reason: string_or(value.get("stop_reason"), "end_turn"),
                input_tokens: value["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output_tokens: value["usage"]["output_tokens"].as_u64().unwrap_or(0),
                cache_read_tokens: value["usage"]["cache_read_input_tokens"]
                    .as_u64()
                    .unwrap_or(0),
            })
        }
        UpstreamFormat::Gemini => {
            let candidate = value["candidates"]
                .as_array()
                .and_then(|items| items.first())
                .ok_or_else(|| ManagerError::System("Gemini 上游缺少 candidates".into()))?;
            let mut text = String::new();
            let mut calls = Vec::new();
            for part in candidate["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(value) = part.get("text").and_then(Value::as_str) {
                    text.push_str(value);
                }
                if let Some(call) = part.get("functionCall") {
                    calls.push(json!({
                        "id": call.get("id").cloned().unwrap_or_else(|| json!(format!("call_{}", uuid::Uuid::new_v4().simple()))),
                        "type": "function",
                        "function": {
                            "name": string_value(call.get("name")),
                            "arguments": call.get("args").cloned().unwrap_or_else(|| json!({})).to_string()
                        }
                    }));
                }
            }
            Ok(ModelResponse {
                native: Some(value.clone()),
                usage_known: value["usageMetadata"].is_object(),
                upstream_model: string_value(value.get("modelVersion")),
                text,
                tool_calls: calls,
                finish_reason: string_or(candidate.get("finishReason"), "STOP"),
                input_tokens: value["usageMetadata"]["promptTokenCount"]
                    .as_u64()
                    .unwrap_or(0),
                output_tokens: value["usageMetadata"]["candidatesTokenCount"]
                    .as_u64()
                    .unwrap_or(0),
                cache_read_tokens: value["usageMetadata"]["cachedContentTokenCount"]
                    .as_u64()
                    .unwrap_or(0),
            })
        }
    }
}

fn response_value(protocol: Protocol, response: &ModelResponse, model: &str) -> Value {
    let model = if response.upstream_model.is_empty() {
        model
    } else {
        &response.upstream_model
    };
    let finish = finish_reason(response);
    match protocol {
        Protocol::Chat => {
            let mut message = json!({"role":"assistant", "content": response.text});
            if !response.tool_calls.is_empty() {
                message["tool_calls"] = json!(response.tool_calls);
            }
            json!({"id":format!("chatcmpl_{}", uuid::Uuid::new_v4().simple()), "object":"chat.completion", "created":chrono::Utc::now().timestamp(), "model":model,
                "choices":[{"index":0,"message":message,"finish_reason":finish}], "usage":openai_usage(response)})
        }
        Protocol::Responses => {
            let mut output = vec![
                json!({"id":format!("msg_{}", uuid::Uuid::new_v4().simple()),"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":response.text,"annotations":[]}]}),
            ];
            for call in &response.tool_calls {
                output.push(json!({"type":"function_call","id":format!("fc_{}",uuid::Uuid::new_v4().simple()),"call_id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"],"status":"completed"}));
            }
            json!({"id":format!("resp_{}",uuid::Uuid::new_v4().simple()),"object":"response","created_at":chrono::Utc::now().timestamp(),"status":if finish == "length" {"incomplete"} else {"completed"},
                "incomplete_details":if finish == "length" {json!({"reason":"max_output_tokens"})} else {Value::Null},"model":model,"output":output,"usage":{"input_tokens":response.input_tokens,"output_tokens":response.output_tokens,"total_tokens":response.input_tokens+response.output_tokens,"input_tokens_details":{"cached_tokens":response.cache_read_tokens}}})
        }
        Protocol::Claude => {
            let mut content = Vec::new();
            if !response.text.is_empty() {
                content.push(json!({"type":"text","text":response.text}));
            }
            for call in &response.tool_calls {
                content.push(json!({"type":"tool_use","id":call["id"],"name":call["function"]["name"],"input":serde_json::from_str::<Value>(call["function"]["arguments"].as_str().unwrap_or("{}")).unwrap_or(Value::Null)}));
            }
            json!({"id":format!("msg_{}",uuid::Uuid::new_v4().simple()),"type":"message","role":"assistant","model":model,"content":content,"stop_reason":if finish == "length" {"max_tokens"} else if finish == "tool_calls" {"tool_use"} else {"end_turn"},"stop_sequence":null,
                "usage":{"input_tokens":response.input_tokens,"output_tokens":response.output_tokens,"cache_read_input_tokens":response.cache_read_tokens}})
        }
        Protocol::Gemini => {
            let mut parts = Vec::new();
            if !response.text.is_empty() {
                parts.push(json!({"text":response.text}));
            }
            for call in &response.tool_calls {
                parts.push(json!({"functionCall":{"id":call["id"],"name":call["function"]["name"],"args":serde_json::from_str::<Value>(call["function"]["arguments"].as_str().unwrap_or("{}")).unwrap_or(Value::Null)}}));
            }
            json!({"candidates":[{"content":{"role":"model","parts":parts},"finishReason":if finish == "length" {"MAX_TOKENS"} else {"STOP"},"index":0}],"modelVersion":model,
                "usageMetadata":{"promptTokenCount":response.input_tokens,"candidatesTokenCount":response.output_tokens,"totalTokenCount":response.input_tokens+response.output_tokens,"cachedContentTokenCount":response.cache_read_tokens}})
        }
    }
}

fn finish_reason(response: &ModelResponse) -> &str {
    match response.finish_reason.as_str() {
        "length" | "max_tokens" | "MAX_TOKENS" => "length",
        "content_filter" | "SAFETY" | "RECITATION" => "content_filter",
        _ if !response.tool_calls.is_empty() => "tool_calls",
        _ => "stop",
    }
}

fn openai_usage(response: &ModelResponse) -> Value {
    json!({"prompt_tokens":response.input_tokens,"completion_tokens":response.output_tokens,"total_tokens":response.input_tokens+response.output_tokens,"prompt_tokens_details":{"cached_tokens":response.cache_read_tokens}})
}

fn model_list_response(paths: &AppPaths, config: &Value, gemini: bool) -> Response<GatewayBody> {
    let models = list_models(paths, config).unwrap_or_default();
    if gemini {
        let models = models
            .into_iter()
            .map(|model| {
                json!({
                    "name": format!("models/{}", string_value(model.get("id"))),
                    "displayName": string_value(model.get("id")),
                    "supportedGenerationMethods": ["generateContent", "streamGenerateContent"]
                })
            })
            .collect::<Vec<_>>();
        json_response(StatusCode::OK, json!({"models": models}))
    } else {
        json_response(StatusCode::OK, json!({"object": "list", "data": models}))
    }
}

fn list_models(paths: &AppPaths, config: &Value) -> Result<Vec<Value>, ManagerError> {
    let ids = target_ids(paths, config)?;
    let configured_aliases = config["modelMappings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        let target = match build_target(paths, &json!({}), &id) {
            Ok(target) => target,
            Err(_) => continue,
        };
        for name in list_target_models(paths, &target) {
            if seen.insert(name.clone()) {
                names.push(json!({"id":name,"object":"model","owned_by":target.name}));
            }
        }
    }
    for (alias, mapping) in configured_aliases {
        let target_id = string_value(mapping.get("targetId"));
        if !target_ids(paths, config)?.contains(&target_id)
            || build_target(paths, &Value::Null, &target_id).is_err()
        {
            continue;
        }
        if seen.insert(alias.clone()) {
            names.push(json!({"id": alias, "object": "model", "owned_by": string_or(mapping.get("targetId"), "gateway")}));
        }
    }
    Ok(names)
}

async fn update_usage(context: &GatewayContext, record: Value) -> Result<(), ManagerError> {
    let _guard = context.store_lock.lock().await;
    let mut config = read_config(&context.paths)?;
    let token_id = string_value(record.get("tokenId"));
    let increment = usage_summary(&[record.clone()]);
    for (key, value) in increment.as_object().unwrap() {
        config["totals"][key] =
            json!(number_value(config["totals"].get(key), 0)
                .saturating_add(value.as_u64().unwrap_or(0)));
    }
    if let Some(token) = config["tokens"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .find(|item| item["id"].as_str() == Some(token_id.as_str()))
    {
        for (key, value) in increment.as_object().unwrap() {
            token[key] =
                json!(number_value(token.get(key), 0).saturating_add(value.as_u64().unwrap_or(0)));
        }
        token["lastUsedAt"] = json!(now_millis());
    }
    write_config(&context.paths, &normalize_config(&config)).await?;
    let mut logs = read_logs(&context.paths)?;
    logs.insert(0, record);
    logs.truncate(MAX_LOGS);
    codex_models::write_atomic(
        std::path::Path::new(&context.paths.storage_files.gateway_request_logs),
        &format!("{}\n", serde_json::to_string(&logs)?),
    )
    .await
}

fn usage_summary(logs: &[Value]) -> Value {
    let mut result = json!({
        "requestCount": 0,
        "successCount": 0,
        "failureCount": 0,
        "inputTokens": 0,
        "outputTokens": 0
    });
    for log in logs {
        result["requestCount"] = json!(number_value(result.get("requestCount"), 0) + 1);
        if log["ok"] == true {
            result["successCount"] = json!(number_value(result.get("successCount"), 0) + 1);
        } else {
            result["failureCount"] = json!(number_value(result.get("failureCount"), 0) + 1);
        }
        result["inputTokens"] = json!(
            number_value(result.get("inputTokens"), 0) + number_value(log.get("inputTokens"), 0)
        );
        result["outputTokens"] = json!(
            number_value(result.get("outputTokens"), 0) + number_value(log.get("outputTokens"), 0)
        );
    }
    result
}

fn aggregate_logs(logs: &[Value], field: &str) -> Vec<Value> {
    let mut map: Map<String, Value> = Map::new();
    for log in logs {
        let key = string_or(log.get(field), "unknown");
        let item = map.entry(key.clone()).or_insert_with(|| {
            json!({
                "key": key,
                "name": if field == "tokenId" { log["tokenName"].clone() } else if field == "providerId" { log["providerName"].clone() } else { json!(key) },
                "requestCount": 0,
                "successCount": 0,
                "failureCount": 0,
                "inputTokens": 0,
                "outputTokens": 0
            })
        });
        item["requestCount"] = json!(number_value(item.get("requestCount"), 0) + 1);
        if log["ok"] == true {
            item["successCount"] = json!(number_value(item.get("successCount"), 0) + 1);
        } else {
            item["failureCount"] = json!(number_value(item.get("failureCount"), 0) + 1);
        }
        item["inputTokens"] = json!(
            number_value(item.get("inputTokens"), 0) + number_value(log.get("inputTokens"), 0)
        );
        item["outputTokens"] = json!(
            number_value(item.get("outputTokens"), 0) + number_value(log.get("outputTokens"), 0)
        );
    }
    map.into_values().collect()
}

fn read_config(paths: &AppPaths) -> Result<Value, ManagerError> {
    match std::fs::read_to_string(&paths.storage_files.gateway_config) {
        Ok(content) => Ok(normalize_config(&serde_json::from_str(&content)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(normalize_config(&json!({})))
        }
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn write_config(paths: &AppPaths, config: &Value) -> Result<(), ManagerError> {
    codex_models::write_atomic(
        std::path::Path::new(&paths.storage_files.gateway_config),
        &format!("{}\n", serde_json::to_string_pretty(config)?),
    )
    .await
}

fn read_logs(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    match std::fs::read_to_string(&paths.storage_files.gateway_request_logs) {
        Ok(content) => Ok(serde_json::from_str::<Value>(&content)?
            .as_array()
            .cloned()
            .unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn normalize_config(input: &Value) -> Value {
    json!({
        "enabled": input["enabled"].as_bool().unwrap_or(false),
        "host": string_or(input.get("host"), DEFAULT_HOST),
        "port": number_value(input.get("port"), DEFAULT_PORT),
        "providerIds": unique_strings(input.get("providerIds")),
        "modelMappings": input.get("modelMappings").filter(|value| value.is_object()).cloned().unwrap_or_else(|| json!({})),
        "tokens": input.get("tokens").filter(|value| value.is_array()).cloned().unwrap_or_else(|| json!([])),
        "updatedAt": number_value(input.get("updatedAt"), 0),
        "totals": input.get("totals").cloned().unwrap_or_else(|| usage_summary(&[]))
    })
}

fn public_config(config: &Value) -> Value {
    let tokens = config["tokens"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let mut item = item.clone();
            if let Some(object) = item.as_object_mut() {
                object.remove("tokenHash");
            }
            item
        })
        .collect::<Vec<_>>();
    let mut result = config.clone();
    result["tokens"] = json!(tokens);
    result
}

fn bind_address(config: &Value) -> Result<SocketAddr, ManagerError> {
    let host: IpAddr = string_or(config.get("host"), DEFAULT_HOST)
        .parse()
        .map_err(|_| ManagerError::System("监听 Host 必须是 IPv4 或 IPv6 地址".into()))?;
    let port = config["port"]
        .as_u64()
        .filter(|port| *port > 0 && *port <= 65535)
        .ok_or_else(|| ManagerError::System("端口必须是 1 到 65535 的整数".into()))?;
    Ok(SocketAddr::new(host, port as u16))
}

fn validate_bind(config: &Value) -> Result<(), ManagerError> {
    bind_address(config).map(|_| ())
}

fn validate_mappings(paths: &AppPaths, config: &Value) -> Result<(), ManagerError> {
    for (alias, mapping) in config["modelMappings"].as_object().into_iter().flatten() {
        let target_id = string_value(mapping.get("targetId"));
        if alias.is_empty()
            || string_value(mapping.get("model")).is_empty()
            || !target_ids(paths, config)?.contains(&target_id)
        {
            return Err(ManagerError::System(format!(
                "模型路由 {alias} 必须指向池中目标并填写真实模型"
            )));
        }
    }
    Ok(())
}

fn google_port(context: &GatewayContext, target: &GatewayTarget) -> Result<u64, ManagerError> {
    let provider = google_account::find_account(&context.paths, &target.id)?;
    let port = number_value(provider["google"].get("port"), 0);
    if !(1..=65535).contains(&port) {
        return Err(ManagerError::System(
            "Google 官方账号本地转发端口无效".into(),
        ));
    }
    Ok(port)
}

fn add_custom_headers(headers: &mut HeaderMap, custom: &Value) -> Result<(), ManagerError> {
    if let Some(items) = custom.as_object() {
        for (name, value) in items {
            let lower = name.to_ascii_lowercase();
            if [
                "authorization",
                "x-api-key",
                "host",
                "content-length",
                "connection",
                "transfer-encoding",
                "accept-encoding",
            ]
            .contains(&lower.as_str())
            {
                continue;
            }
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|error| ManagerError::System(error.to_string()))?;
            let value = HeaderValue::from_str(&string_value(Some(value)))
                .map_err(|error| ManagerError::System(error.to_string()))?;
            headers.insert(name, value);
        }
    }
    Ok(())
}

fn append_path(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with(path) {
        return base.to_string();
    }
    let root = url::Url::parse(base).is_ok_and(|url| url.path().is_empty() || url.path() == "/");
    format!("{base}{}{path}", if root { "/v1" } else { "" })
}

fn append_gemini_path(base: &str, model: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.contains(":generateContent") {
        base.to_string()
    } else if base.ends_with("/v1beta") || base.ends_with("/v1") {
        format!("{base}/models/{model}:generateContent")
    } else {
        format!("{base}/v1beta/models/{model}:generateContent")
    }
}

fn error_response(status: StatusCode, message: &str) -> Response<GatewayBody> {
    json_response(
        status,
        json!({"error": {"type": "gateway_error", "message": message}}),
    )
}

fn json_response(status: StatusCode, body: Value) -> Response<GatewayBody> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())).boxed_unsync())
        .unwrap()
}

fn response_item_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item["text"].as_str().unwrap_or("").to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(""),
        Some(value) if value.is_object() => {
            response_item_text(value.get("content").or_else(|| value.get("parts")))
        }
        _ => String::new(),
    }
}

fn content_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("content").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn join_text(values: Vec<String>) -> String {
    values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn copy_optional(body: &mut Value, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        body[key] = value;
    }
}

fn protocol_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Chat => "chat",
        Protocol::Responses => "responses",
        Protocol::Claude => "claude",
        Protocol::Gemini => "gemini",
    }
}

fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn string_or(value: Option<&Value>, fallback: &str) -> String {
    let value = string_value(value);
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn number_value(value: Option<&Value>, fallback: u64) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(fallback)
}

fn unique_strings(value: Option<&Value>) -> Vec<String> {
    let mut seen = HashSet::new();
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .filter(|item| !item.is_empty() && seen.insert(item.clone()))
        .collect()
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn token_hint(token: &str) -> String {
    let end = token.len().min(8);
    format!("{}…", &token[..end])
}

fn base_url(config: &Value) -> String {
    let host = string_or(config.get("host"), DEFAULT_HOST);
    let host = match host.as_str() {
        "0.0.0.0" => "127.0.0.1",
        "::" => "::1",
        _ => &host,
    };
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    format!(
        "http://{host}:{}",
        number_value(config.get("port"), DEFAULT_PORT)
    )
}

fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

#[cfg(test)]
mod tests;

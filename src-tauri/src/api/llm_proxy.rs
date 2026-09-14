use crate::api::proxy::LlmTarget;
use crate::core::{error::ManagerError, paths::AppPaths, translation_store};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex, task::{JoinHandle, JoinSet}};

// 每次翻译使用独立回环代理和随机凭据，消费记录不依赖前端上报。
pub(crate) struct LlmProxy {
    pub base_url: String,
    pub token: String,
    pub records: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl Drop for LlmProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl LlmProxy {
    pub async fn start(paths: AppPaths, target: LlmTarget, translation_id: String) -> Result<Self, ManagerError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("http://{}/v1", listener.local_addr()?);
        let token = uuid::Uuid::new_v4().to_string();
        let records = Arc::new(Mutex::new(Vec::new()));
        let context = Arc::new(ProxyContext { paths, target, translation_id, token: token.clone(), records: records.clone() });
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                while connections.try_join_next().is_some() {}
                let context = context.clone();
                connections.spawn(async move {
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service_fn(move |request| {
                        let context = context.clone();
                        async move { Ok::<_, Infallible>(handle(request, context).await) }
                    })).await;
                });
            }
        });
        Ok(Self { base_url, token, records, task })
    }
}

struct ProxyContext {
    paths: AppPaths,
    target: LlmTarget,
    translation_id: String,
    token: String,
    records: Arc<Mutex<Vec<Value>>>,
}

fn error_response(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    Response::builder().status(status).header("content-type", "application/json")
        .body(Full::new(Bytes::from(json!({"error": {"message": message}}).to_string()))).unwrap()
}

async fn handle(request: Request<Incoming>, context: Arc<ProxyContext>) -> Response<Full<Bytes>> {
    if request.headers().get("authorization").and_then(|value| value.to_str().ok()) != Some(&format!("Bearer {}", context.token)) {
        return error_response(StatusCode::UNAUTHORIZED, "翻译代理凭据无效");
    }
    if request.method() != hyper::Method::POST || request.uri().path() != "/v1/responses" {
        return error_response(StatusCode::NOT_FOUND, "翻译代理仅支持 POST /v1/responses");
    }
    let body = match http_body_util::Limited::new(request.into_body(), 2 * 1024 * 1024).collect().await {
        Ok(body) => body.to_bytes(),
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "翻译请求过大或不完整"),
    };
    let body = match serde_json::from_slice::<Value>(&body) {
        Ok(body) if body.is_object() => body,
        _ => return error_response(StatusCode::BAD_REQUEST, "翻译请求格式无效"),
    };
    let started = std::time::Instant::now();
    let mut record = json!({
        "id": uuid::Uuid::new_v4().to_string(), "translationId": context.translation_id,
        "createdAt": chrono::Utc::now().timestamp_millis(),
        "targetId": context.target.info["targetId"], "providerName": context.target.info["providerName"],
        "model": context.target.info["model"], "engine": context.target.info["engine"],
        "status": "running", "statusCode": 0, "usageKnown": false, "errorMessage": "",
        "inputTokens": 0, "outputTokens": 0, "cacheReadTokens": 0, "reasoningTokens": 0
    });
    let mut tracking = context.target.tracking(&context.paths);
    if let Some(tracking) = &tracking {
        record["apiKeyId"] = json!(tracking.key_id());
        record["apiKeyHash"] = json!(tracking.key_hash());
    }
    if let Err(error) = translation_store::save_request(&context.paths, &record) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    if let Some(tracking) = &mut tracking { tracking.start(); }
    let mut bytes = Vec::new();
    let mut streaming = false;
    let result = async {
        let response = context.target.send(body).await?;
        let status = response.status();
        record["statusCode"] = json!(status.as_u16());
        let content_type = response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or("application/json").to_string();
        streaming = content_type.contains("text/event-stream");
        if let Some(tracking) = &mut tracking { tracking.response(status.as_u16(), content_type.contains("text/event-stream")); }
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| ManagerError::System(error.to_string()))?;
            if let Some(tracking) = &mut tracking { tracking.observe(&chunk); }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            record["errorMessage"] = json!(format!("上游返回 HTTP {}", status.as_u16()));
        }
        Ok::<_, ManagerError>((status, content_type))
    }.await;
    let completed = inspect_response(&bytes, &mut record);
    if streaming && !completed && record["errorMessage"] == "" {
        record["errorMessage"] = json!("上游流在翻译完成前结束");
    }
    record["status"] = json!(if record["errorMessage"] == "" { "success" } else { "failed" });
    if let Err(error) = &result {
        if let Some(tracking) = &mut tracking { tracking.fail("network"); }
        record["status"] = json!("failed");
        record["errorMessage"] = json!(error.to_string());
    } else if let Some(tracking) = &mut tracking { tracking.finish(); }
    record["durationMs"] = json!(started.elapsed().as_millis() as u64);
    if let Err(error) = translation_store::save_request(&context.paths, &record) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    context.records.lock().await.push(record);
    match result {
        Ok((status, content_type)) => Response::builder().status(status).header("content-type", content_type).body(Full::new(Bytes::from(bytes))).unwrap(),
        Err(error) => error_response(StatusCode::BAD_GATEWAY, &error.to_string()),
    }
}

// Responses 用量只取终态响应一次；缓存和推理 token 是输入、输出的子集。
fn inspect_response(bytes: &[u8], record: &mut Value) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let mut responses = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        responses.push(value);
    } else {
        for frame in text.replace("\r\n", "\n").split("\n\n") {
            let data = frame.lines().filter_map(|line| line.strip_prefix("data:").map(str::trim_start)).collect::<Vec<_>>().join("\n");
            if let Ok(value) = serde_json::from_str::<Value>(&data) { responses.push(value); }
        }
    }
    let mut completed = false;
    for value in responses {
        let response = value.get("response").unwrap_or(&value);
        completed |= value["type"] == "response.completed" || response["status"] == "completed";
        if let Some(usage) = response.get("usage").filter(|value| value.is_object()) {
            if usage["input_tokens"].is_u64() && usage["output_tokens"].is_u64() {
                record["usageKnown"] = json!(true);
                record["inputTokens"] = usage["input_tokens"].clone();
                record["outputTokens"] = usage["output_tokens"].clone();
                record["cacheReadTokens"] = json!(usage["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0));
                record["reasoningTokens"] = json!(usage["output_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0));
            }
        }
        if matches!(value["type"].as_str(), Some("error" | "response.failed" | "response.incomplete"))
            || matches!(response["status"].as_str(), Some("failed" | "incomplete"))
            || response.get("error").is_some_and(|error| !error.is_null()) {
            record["errorMessage"] = json!("模型未完成翻译，请查看账号状态或重试");
        }
    }
    completed
}

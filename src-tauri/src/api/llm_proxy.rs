use crate::api::proxy::LlmTarget;
use crate::core::{error::ManagerError, paths::AppPaths, translation_store};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Frame, Incoming}, server::conn::http1, service::service_fn, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::Mutex,
    task::{JoinHandle, JoinSet},
};

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
    pub async fn start(
        paths: AppPaths,
        target: LlmTarget,
        translation_id: String,
    ) -> Result<Self, ManagerError> {
        Self::start_mode(paths, target, translation_id, false).await
    }

    pub async fn start_assistant(paths: AppPaths, target: LlmTarget) -> Result<Self, ManagerError> {
        Self::start_mode(paths, target, String::new(), true).await
    }

    async fn start_mode(
        paths: AppPaths,
        target: LlmTarget,
        translation_id: String,
        assistant: bool,
    ) -> Result<Self, ManagerError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("http://{}/v1", listener.local_addr()?);
        let token = uuid::Uuid::new_v4().to_string();
        let records = Arc::new(Mutex::new(Vec::new()));
        let context = Arc::new(ProxyContext {
            paths,
            target,
            translation_id,
            token: token.clone(),
            records: records.clone(),
            assistant,
        });
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                while connections.try_join_next().is_some() {}
                let context = context.clone();
                connections.spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(
                            TokioIo::new(stream),
                            service_fn(move |request| {
                                let context = context.clone();
                                async move { Ok::<_, Infallible>(handle(request, context).await) }
                            }),
                        )
                        .await;
                });
            }
        });
        Ok(Self {
            base_url,
            token,
            records,
            task,
        })
    }
}

struct ProxyContext {
    paths: AppPaths,
    target: LlmTarget,
    translation_id: String,
    token: String,
    records: Arc<Mutex<Vec<Value>>>,
    assistant: bool,
}

type ProxyBody = UnsyncBoxBody<Bytes, reqwest::Error>;

fn error_response(status: StatusCode, message: &str) -> Response<ProxyBody> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(
            json!({"error": {"message": message}}).to_string(),
        )).map_err(|error| match error {}).boxed_unsync())
        .unwrap()
}

async fn handle(request: Request<Incoming>, context: Arc<ProxyContext>) -> Response<ProxyBody> {
    let authorized = request.headers().get("authorization").and_then(|value| value.to_str().ok())
        == Some(&format!("Bearer {}", context.token))
        || (context.assistant && context.target.info["api"] == "messages"
            && request.headers().get("x-api-key").and_then(|value| value.to_str().ok()) == Some(&context.token));
    if !authorized {
        return error_response(StatusCode::UNAUTHORIZED, "模型代理凭据无效");
    }
    let endpoint = match context.target.info["api"].as_str().filter(|_| context.assistant) {
        Some("messages") => "/v1/messages",
        Some("chat") => "/v1/chat/completions",
        _ => "/v1/responses",
    };
    if request.method() != hyper::Method::POST || request.uri().path() != endpoint {
        return error_response(StatusCode::NOT_FOUND, "模型代理请求路径无效");
    }
    let body = match http_body_util::Limited::new(request.into_body(), 2 * 1024 * 1024)
        .collect()
        .await
    {
        Ok(body) => body.to_bytes(),
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "翻译请求过大或不完整"),
    };
    let body = match serde_json::from_slice::<Value>(&body) {
        Ok(body) if body.is_object() => body,
        _ => return error_response(StatusCode::BAD_REQUEST, "翻译请求格式无效"),
    };
    // 助手保留工具定义并逐块转发，不写入翻译记录；API Key 统计仍共用原有链路。
    if context.assistant {
        let mut tracking = context.target.tracking(&context.paths);
        if let Some(tracking) = &mut tracking {
            tracking.start();
        }
        let response = match context.target.send_agent(body).await {
            Ok(response) => response,
            Err(error) => {
                if let Some(tracking) = &mut tracking {
                    tracking.fail("network");
                }
                return error_response(StatusCode::BAD_GATEWAY, &error.to_string());
            }
        };
        let status = response.status();
        let content_type = response.headers().get("content-type")
            .and_then(|value| value.to_str().ok()).unwrap_or("application/json").to_string();
        if let Some(tracking) = &mut tracking {
            tracking.response(status.as_u16(), content_type.contains("text/event-stream"));
        }
        if !status.is_success() {
            let bytes = match response.bytes().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    if let Some(tracking) = &mut tracking {
                        tracking.fail("network");
                    }
                    return error_response(
                        StatusCode::BAD_GATEWAY,
                        &format!("上游返回 HTTP {}，且错误响应读取失败：{error}", status.as_u16()),
                    );
                }
            };
            if let Some(tracking) = &mut tracking {
                tracking.observe(&bytes);
                tracking.finish();
            }
            return Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(
                    Full::new(enrich_assistant_error(status, &bytes))
                        .map_err(|error| match error {})
                        .boxed_unsync(),
                )
                .unwrap();
        }
        let stream = futures_util::stream::unfold(
            (response.bytes_stream(), tracking),
            |(mut stream, mut tracking)| async move {
                match stream.next().await {
                    Some(chunk) => {
                        if let Some(tracking) = &mut tracking {
                            match &chunk {
                                Ok(bytes) => tracking.observe(bytes),
                                Err(_) => tracking.fail("network"),
                            }
                        }
                        Some((chunk.map(Frame::data), (stream, tracking)))
                    }
                    None => {
                        if let Some(tracking) = &mut tracking {
                            tracking.finish();
                        }
                        None
                    }
                }
            },
        );
        return Response::builder().status(status).header("content-type", content_type)
            .body(StreamBody::new(stream).boxed_unsync()).unwrap();
    }
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
    if let Some(tracking) = &mut tracking {
        tracking.start();
    }
    let mut bytes = Vec::new();
    let mut streaming = false;
    let result = async {
        let response = context.target.send(body).await?;
        let status = response.status();
        record["statusCode"] = json!(status.as_u16());
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/json")
            .to_string();
        streaming = content_type.contains("text/event-stream");
        if let Some(tracking) = &mut tracking {
            tracking.response(status.as_u16(), content_type.contains("text/event-stream"));
        }
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| ManagerError::System(error.to_string()))?;
            if let Some(tracking) = &mut tracking {
                tracking.observe(&chunk);
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            record["errorMessage"] = json!(format!("上游返回 HTTP {}", status.as_u16()));
        }
        Ok::<_, ManagerError>((status, content_type))
    }
    .await;
    let completed = inspect_response(&bytes, &mut record);
    if streaming && !completed && record["errorMessage"] == "" {
        record["errorMessage"] = json!("上游流在翻译完成前结束");
    }
    record["status"] = json!(if record["errorMessage"] == "" {
        "success"
    } else {
        "failed"
    });
    if let Err(error) = &result {
        if let Some(tracking) = &mut tracking {
            tracking.fail("network");
        }
        record["status"] = json!("failed");
        record["errorMessage"] = json!(error.to_string());
    } else if let Some(tracking) = &mut tracking {
        tracking.finish();
    }
    record["durationMs"] = json!(started.elapsed().as_millis() as u64);
    if let Err(error) = translation_store::save_request(&context.paths, &record) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    context.records.lock().await.push(record);
    match result {
        Ok((status, content_type)) => Response::builder()
            .status(status)
            .header("content-type", content_type)
            .body(Full::new(Bytes::from(bytes)).map_err(|error| match error {}).boxed_unsync())
            .unwrap(),
        Err(error) => error_response(StatusCode::BAD_GATEWAY, &error.to_string()),
    }
}

fn enrich_assistant_error(status: StatusCode, bytes: &[u8]) -> Bytes {
    let raw = String::from_utf8_lossy(bytes).trim().to_string();
    let mut value = serde_json::from_slice::<Value>(bytes).unwrap_or_else(|_| {
        json!({
            "error": {
                "message": raw
            }
        })
    });
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .unwrap_or("模型服务请求失败")
        .trim()
        .to_string();
    let detail = value
        .pointer("/error/detail")
        .and_then(Value::as_str)
        .or_else(|| value.get("detail").and_then(Value::as_str))
        .or_else(|| value.pointer("/error/reason").and_then(Value::as_str))
        .or_else(|| value.get("reason").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("");
    let status_text = format!("HTTP {}", status.as_u16());
    let message = if message.contains(&status_text) {
        if detail.is_empty() || message.contains(detail) {
            message
        } else {
            format!("{message}：{detail}")
        }
    } else if detail.is_empty() || message.contains(detail) {
        format!("{message}（HTTP {}）", status.as_u16())
    } else {
        format!("{message}（HTTP {}）：{detail}", status.as_u16())
    };
    if !value.is_object() {
        value = json!({"error": {}});
    }
    if !value["error"].is_object() {
        value["error"] = json!({});
    }
    value["error"]["message"] = json!(message);
    Bytes::from(value.to_string())
}

// Responses 用量只取终态响应一次；缓存和推理 token 是输入、输出的子集。
fn inspect_response(bytes: &[u8], record: &mut Value) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let mut responses = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        responses.push(value);
    } else {
        for frame in text.replace("\r\n", "\n").split("\n\n") {
            let data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>()
                .join("\n");
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                responses.push(value);
            }
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
                record["cacheReadTokens"] = json!(usage["input_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0));
                record["reasoningTokens"] = json!(usage["output_tokens_details"]
                    ["reasoning_tokens"]
                    .as_u64()
                    .unwrap_or(0));
            }
        }
        if matches!(
            value["type"].as_str(),
            Some("error" | "response.failed" | "response.incomplete")
        ) || matches!(response["status"].as_str(), Some("failed" | "incomplete"))
            || response.get("error").is_some_and(|error| !error.is_null())
        {
            record["errorMessage"] = json!("模型未完成翻译，请查看账号状态或重试");
        }
    }
    completed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_error_preserves_upstream_detail_and_status() {
        let body = r#"{"error":{"message":"Upstream request failed","detail":"工具定义不受支持"}}"#
            .as_bytes();
        let value: Value =
            serde_json::from_slice(&enrich_assistant_error(StatusCode::BAD_GATEWAY, body))
                .unwrap();
        assert_eq!(
            value["error"]["message"],
            "Upstream request failed（HTTP 502）：工具定义不受支持"
        );
    }

    #[test]
    fn assistant_error_does_not_duplicate_existing_status() {
        let body = r#"{"error":{"message":"Upstream request failed（HTTP 502）"}}"#.as_bytes();
        let value: Value =
            serde_json::from_slice(&enrich_assistant_error(StatusCode::BAD_GATEWAY, body))
                .unwrap();
        assert_eq!(value["error"]["message"], "Upstream request failed（HTTP 502）");
    }
}

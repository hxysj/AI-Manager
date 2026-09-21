use crate::api::{
    google_account,
    google_protocol::{self, ResponseMapper},
    google_session::{self, ConnectionSessions},
    runtime_provider,
};
use crate::core::{error::ManagerError, paths::AppPaths};
use bytes::Bytes;
use futures_util::{FutureExt, SinkExt, StreamExt};
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    server::conn::http1,
    service::service_fn,
    Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, Mutex},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::{
    tungstenite::{
        protocol::{Role, WebSocketConfig},
        Message,
    },
    WebSocketStream,
};
use tokio_util::sync::CancellationToken;

type Body = UnsyncBoxBody<Bytes, Infallible>;
type ConnectionCache = Arc<Mutex<ConnectionSessions>>;
static SERVERS: OnceLock<Mutex<HashMap<String, JoinHandle<()>>>> = OnceLock::new();

fn server_key(paths: &AppPaths, id: &str) -> String {
    format!("{}:{id}", paths.storage_files.database)
}

pub(crate) async fn ensure_started(paths: &AppPaths, provider: &Value) -> Result<(), ManagerError> {
    if provider["type"] != "google-account" {
        return Ok(());
    }
    if provider["enabled"] == false {
        return Err(ManagerError::System("Google 账号已禁用".into()));
    }
    let id = provider["id"].as_str().unwrap_or("");
    let key = server_key(paths, id);
    let mut servers = SERVERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .await;
    if servers.get(&key).is_some_and(|task| !task.is_finished()) {
        return Ok(());
    }
    let port = provider["google"]["port"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| ManagerError::System("Google 本地转发端口无效，请重新登录账号".into()))?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|error| {
            ManagerError::System(format!("Google 本地转发端口 {port} 无法启动：{error}"))
        })?;
    let paths = Arc::new(paths.clone());
    let id = Arc::new(id.to_string());
    servers.insert(
        key,
        tokio::spawn(async move {
            let cancel = CancellationToken::new();
            let _guard = cancel.clone().drop_guard();
            let mut connections = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                while connections.try_join_next().is_some() {}
                let paths = paths.clone();
                let id = id.clone();
                let cancel = cancel.clone();
                connections.spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(
                            TokioIo::new(stream),
                            service_fn(move |request| {
                                let paths = paths.clone();
                                let id = id.clone();
                                let cancel = cancel.clone();
                                async move {
                                    Ok::<_, Infallible>(handle(request, &paths, &id, cancel).await)
                                }
                            }),
                        )
                        .with_upgrades()
                        .await;
                });
            }
        }),
    );
    Ok(())
}

pub(crate) async fn stop(paths: &AppPaths, id: &str) {
    if let Some(task) = SERVERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .await
        .remove(&server_key(paths, id))
    {
        task.abort();
        let _ = task.await;
    }
}

fn json_reply(status: StatusCode, value: Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(value.to_string())).boxed_unsync())
        .unwrap()
}

fn failure(status: StatusCode, message: &str) -> Response<Body> {
    json_reply(
        status,
        json!({"error": {"type": "google_proxy_error", "message": message}}),
    )
}

async fn handle(
    mut request: Request<Incoming>,
    paths: &AppPaths,
    id: &str,
    cancel: CancellationToken,
) -> Response<Body> {
    let provider = match google_account::find_account(paths, id) {
        Ok(provider) if provider["enabled"] != false => provider,
        _ => return failure(StatusCode::FORBIDDEN, "Google 账号已禁用或删除"),
    };
    let key = match runtime_provider::get_provider_api_key(paths, id) {
        Ok(key) if !key.is_empty() => key,
        _ => return failure(StatusCode::UNAUTHORIZED, "本地转发凭据不存在，请重新登录"),
    };
    if request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some(&format!("Bearer {key}"))
    {
        return failure(StatusCode::UNAUTHORIZED, "本地转发凭据无效");
    }
    // 不接受网页跨域调用，OAuth Token 从不作为本地访问凭据。
    if request.headers().contains_key("origin") {
        return failure(StatusCode::FORBIDDEN, "本地转发不接受浏览器跨域请求");
    }
    if request.method() == hyper::Method::GET && request.uri().path() == "/v1/models" {
        let models: Vec<_> = provider["google"]["models"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|model| json!({"id": model["id"], "object": "model", "owned_by": "google"}))
            .collect();
        return json_reply(StatusCode::OK, json!({"object": "list", "data": models}));
    }
    let responses_path = matches!(request.uri().path(), "/v1/responses" | "/responses");
    if request.method() == hyper::Method::GET && responses_path {
        let upgrade = request
            .headers()
            .get("upgrade")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let connection = request
            .headers()
            .get("connection")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let websocket_key = request
            .headers()
            .get("sec-websocket-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        use base64::Engine;
        if !upgrade.eq_ignore_ascii_case("websocket")
            || !connection
                .split(',')
                .any(|v| v.trim().eq_ignore_ascii_case("upgrade"))
            || request
                .headers()
                .get("sec-websocket-version")
                .and_then(|v| v.to_str().ok())
                != Some("13")
            || base64::engine::general_purpose::STANDARD
                .decode(&websocket_key)
                .map_or(true, |v| v.len() != 16)
        {
            return failure(StatusCode::BAD_REQUEST, "WebSocket 握手参数无效");
        }
        let upgraded = hyper::upgrade::on(&mut request);
        let paths = paths.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            let work = async {
                if let Ok(stream) = upgraded.await {
                    websocket(stream, paths, id, cancel.clone()).await;
                }
            };
            futures_util::select! { _ = cancel.cancelled().fuse() => {}, _ = work.fuse() => {} }
        });
        return Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header("upgrade", "websocket")
            .header("connection", "Upgrade")
            .header(
                "sec-websocket-accept",
                tokio_tungstenite::tungstenite::handshake::derive_accept_key(
                    websocket_key.as_bytes(),
                ),
            )
            .body(Full::new(Bytes::new()).boxed_unsync())
            .unwrap();
    }
    if matches!(
        request.uri().path(),
        "/v1/responses/compact" | "/responses/compact"
    ) {
        return failure(
            StatusCode::NOT_IMPLEMENTED,
            "Google 转发暂不支持远程 compact，请使用客户端上下文压缩",
        );
    }
    if request.method() != hyper::Method::POST || !responses_path {
        return failure(
            StatusCode::NOT_FOUND,
            "Google 转发支持 /v1/responses（HTTP / WebSocket）、/responses 和 GET /v1/models",
        );
    }
    let bytes = match http_body_util::Limited::new(request.into_body(), 32 * 1024 * 1024)
        .collect()
        .await
    {
        Ok(body) => body.to_bytes(),
        Err(_) => return failure(StatusCode::PAYLOAD_TOO_LARGE, "请求过大或不完整"),
    };
    let body: Value = match serde_json::from_slice(&bytes) {
        Ok(Value::Object(body)) => Value::Object(body),
        _ => return failure(StatusCode::BAD_REQUEST, "请求必须是 JSON 对象"),
    };
    if body.get("generate").is_some() || body.get("stream_id").is_some() {
        return failure(
            StatusCode::BAD_REQUEST,
            "generate 和 stream_id 仅用于 WebSocket 请求",
        );
    }
    respond(body, paths, id, None, cancel).await
}

async fn respond(
    mut body: Value,
    paths: &AppPaths,
    id: &str,
    cache: Option<ConnectionCache>,
    cancel: CancellationToken,
) -> Response<Body> {
    let provider = match google_account::find_account(paths, id) {
        Ok(provider) if provider["enabled"] != false => provider,
        _ => return failure(StatusCode::FORBIDDEN, "Google 账号已禁用或删除"),
    };
    if body["background"] == true || body.get("conversation").is_some_and(|v| !v.is_null()) {
        return failure(
            StatusCode::BAD_REQUEST,
            "本地转发不支持 background 或 conversation，请使用 previous_response_id",
        );
    }
    let previous = body["previous_response_id"].clone();
    let parent = if let Some(previous) = previous.as_str() {
        let cached = match &cache {
            Some(cache) => cache.lock().await.get(previous),
            None => None,
        };
        match cached.map(Ok).unwrap_or_else(|| {
            google_session::load(paths, id, previous).and_then(|v| {
                v.ok_or_else(|| ManagerError::System("previous_response_not_found".into()))
            })
        }) {
            Ok(parent) => Some(parent),
            Err(ManagerError::System(message)) if message == "previous_response_not_found" => {
                return json_reply(
                    StatusCode::BAD_REQUEST,
                    json!({"error": {"type": "invalid_request_error", "code": "previous_response_not_found", "param": "previous_response_id", "message": "前序响应不存在或已过期，请清空 previous_response_id 并发送完整历史"}}),
                )
            }
            Err(error) => return failure(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        }
    } else if previous.is_null() {
        None
    } else {
        return failure(
            StatusCode::BAD_REQUEST,
            "previous_response_id 必须是字符串或 null",
        );
    };
    if let Err(message) = google_session::merge(&mut body, parent) {
        return failure(StatusCode::BAD_REQUEST, &message);
    }
    let mut model = body["model"].as_str().unwrap_or("").to_string();
    if model.is_empty() {
        model = provider["runtimeConfig"]["mainModel"]
            .as_str()
            .unwrap_or("")
            .to_string();
    }
    if let Some(replacement) =
        provider["google"]["deprecatedModelIds"][&model]["newModelId"].as_str()
    {
        model = replacement.to_string();
    }
    let selected = provider["google"]["models"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["id"] == model);
    if selected.is_none() {
        return failure(
            StatusCode::BAD_REQUEST,
            "模型不在当前 Google 账号的可用列表中，请刷新模型列表后重新选择",
        );
    }
    if provider["google"]["status"] == "reauth_required" {
        return failure(StatusCode::UNAUTHORIZED, "Google 授权已失效，请重新登录");
    }
    let project = provider["google"]["project"].as_str().unwrap_or("");
    if project.is_empty() {
        return failure(
            StatusCode::BAD_REQUEST,
            "Google 账号缺少 Cloud Code 项目，请完成开通并刷新账号",
        );
    }
    body["model"] = json!(model);
    if body["generate"] == false && cache.is_some() {
        let mut mapper = ResponseMapper::new(&body);
        let mut events = mapper.start();
        mapper.finished = true;
        let terminal = mapper.finish().unwrap();
        if let Err(message) = remember(paths, id, &body, &mapper.response, cache.as_ref()).await {
            return failure(StatusCode::INTERNAL_SERVER_ERROR, &message);
        }
        events.extend(terminal);
        return Response::builder()
            .header("content-type", "text/event-stream")
            .body(
                Full::new(Bytes::from(
                    events.iter().map(google_protocol::sse).collect::<String>(),
                ))
                .boxed_unsync(),
            )
            .unwrap();
    }
    let mut payload = match google_protocol::to_google(&body, project) {
        Ok(payload) => payload,
        Err(message) => return failure(StatusCode::BAD_REQUEST, &message),
    };
    if selected.is_some_and(|model| model["supportsThinking"] == true) {
        payload["request"]["generationConfig"]["thinkingConfig"] = json!({"includeThoughts": true});
    }
    let streaming = body["stream"] == true;
    let response = match google_account::upstream(
        paths,
        id,
        if streaming {
            "streamGenerateContent?alt=sse"
        } else {
            "generateContent"
        },
        &payload,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => return failure(StatusCode::BAD_GATEWAY, &error.to_string()),
    };
    if !response.status().is_success() {
        let status = response.status();
        let retry_after = response.headers().get("retry-after").cloned();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        let message = value["error"]["message"]
            .as_str()
            .unwrap_or("Google 模型请求失败");
        let state = match status.as_u16() {
            401 => "reauth_required",
            403 => "blocked",
            429 => "rate_limited",
            _ => "error",
        };
        let _ = google_account::update_metadata(
            paths,
            id,
            json!({"status": state, "message": message, "lastStatusCode": status.as_u16()}),
        );
        let mut result = failure(status, message);
        if let Some(retry_after) = retry_after {
            result.headers_mut().insert("retry-after", retry_after);
        }
        return result;
    }
    if !streaming {
        let value = match google_account::json_response(response).await {
            Ok(value) => value,
            Err(error) => return failure(StatusCode::BAD_GATEWAY, &error.to_string()),
        };
        let mut mapper = ResponseMapper::new(&body);
        if let Err(message) = mapper.chunk(&value).and_then(|_| mapper.finish()) {
            return failure(StatusCode::BAD_GATEWAY, &message);
        }
        if let Err(message) = remember(paths, id, &body, &mapper.response, cache.as_ref()).await {
            return failure(StatusCode::INTERNAL_SERVER_ERROR, &message);
        }
        return json_reply(StatusCode::OK, mapper.response);
    }
    let (sender, receiver) = mpsc::channel::<Result<Frame<Bytes>, Infallible>>(16);
    // 有界通道保留背压；客户端断开后取消上游读取，避免继续消耗额度。
    let paths = paths.clone();
    let id = id.to_owned();
    tokio::spawn(async move {
        let mut mapper = ResponseMapper::new(&body);
        if !send_events(&sender, mapper.start()).await {
            return;
        }
        let result = futures_util::select! {
            _ = cancel.cancelled().fuse() => return,
            result = stream_google(response, &sender, &mut mapper).fuse() => result,
        };
        match result.and_then(|_| mapper.finish()) {
            Ok(events) => {
                if let Err(message) =
                    remember(&paths, &id, &body, &mapper.response, cache.as_ref()).await
                {
                    send_events(&sender, vec![mapper.fail(&message)]).await;
                    return;
                }
                send_events(&sender, events).await;
            }
            Err(message) => {
                send_events(&sender, vec![mapper.fail(&message)]).await;
            }
        }
    });
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(StreamBody::new(stream).boxed_unsync())
        .unwrap()
}

async fn remember(
    paths: &AppPaths,
    id: &str,
    body: &Value,
    response: &Value,
    cache: Option<&ConnectionCache>,
) -> Result<(), String> {
    if response["status"] != "completed" {
        return Ok(());
    }
    let response_id = response["id"].as_str().ok_or("响应缺少 ID")?;
    let session = google_session::snapshot(body, response);
    if body["store"] != false {
        google_session::save(paths, id, response_id, &session)
            .map_err(|error| error.to_string())?;
    }
    if let Some(cache) = cache {
        cache.lock().await.insert(response_id, session)?;
    }
    Ok(())
}

async fn websocket(
    stream: hyper::upgrade::Upgraded,
    paths: AppPaths,
    id: String,
    cancel: CancellationToken,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(32 * 1024 * 1024))
        .max_frame_size(Some(32 * 1024 * 1024));
    let socket =
        WebSocketStream::from_raw_socket(TokioIo::new(stream), Role::Server, Some(config)).await;
    let (mut writer, mut reader) = socket.split();
    let (requests, mut pending) = mpsc::channel::<Value>(1);
    let (outgoing, mut events) = mpsc::channel::<Option<Message>>(16);
    let cache = Arc::new(Mutex::new(ConnectionSessions::default()));
    let read = async {
        while let Some(Ok(message)) = reader.next().await {
            match message {
                Message::Text(text) => match serde_json::from_str::<Value>(&text) {
                    Ok(body) if body["type"] == "response.create" && body.is_object() => {
                        if let Err(error) = requests.try_send(body) {
                            let body = error.into_inner();
                            let event = json!({"type": "error", "status": 429, "stream_id": body["stream_id"], "error": {"type": "invalid_request_error", "code": "too_many_requests", "message": "本地连接按顺序生成，请等待当前响应完成后重试"}});
                            if outgoing
                                .send(Some(Message::Text(event.to_string().into())))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    _ => {
                        let event = json!({"type": "error", "status": 400, "error": {"type": "invalid_request_error", "code": "invalid_event", "message": "需要 JSON response.create 事件"}});
                        if outgoing
                            .send(Some(Message::Text(event.to_string().into())))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                },
                // tungstenite 已自动排队 Pong，这里只触发写端刷新，避免重复回复。
                Message::Ping(_) => {
                    if outgoing.send(None).await.is_err() {
                        break;
                    }
                }
                Message::Pong(_) => {}
                _ => break,
            }
        }
    };
    let write = async {
        while let Some(message) = events.recv().await {
            let result = match message {
                Some(message) => writer.send(message).await,
                None => writer.flush().await,
            };
            if result.is_err() {
                break;
            }
        }
    };
    let process = async {
        while let Some(mut body) = pending.recv().await {
            let lane = body.get("stream_id").cloned();
            body["stream"] = json!(true);
            let response = respond(body, &paths, &id, Some(cache.clone()), cancel.clone()).await;
            let status = response.status();
            if !status.is_success() {
                let value = response.into_body().collect().await.unwrap().to_bytes();
                let error: Value = serde_json::from_slice(&value).unwrap_or(Value::Null);
                let mut event =
                    json!({"type": "error", "status": status.as_u16(), "error": error["error"]});
                if let Some(lane) = &lane {
                    event["stream_id"] = lane.clone();
                }
                if outgoing
                    .send(Some(Message::Text(event.to_string().into())))
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
            let mut response = response.into_body();
            let mut buffer = Vec::new();
            while let Some(Ok(frame)) = response.frame().await {
                if let Ok(data) = frame.into_data() {
                    buffer.extend_from_slice(&data);
                }
                while let Some((index, size)) = frame_boundary(&buffer) {
                    let frame: Vec<_> = buffer.drain(..index + size).collect();
                    let frame = String::from_utf8_lossy(&frame);
                    for line in frame.lines().filter_map(|line| line.strip_prefix("data:")) {
                        if let Ok(mut event) = serde_json::from_str::<Value>(line) {
                            if let Some(lane) = &lane {
                                event["stream_id"] = lane.clone();
                            }
                            if outgoing
                                .send(Some(Message::Text(event.to_string().into())))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
            }
        }
    };
    // 三个 future 共用连接生命周期；断开、禁用时丢弃生成 future 与 SSE 接收端。
    futures_util::pin_mut!(read, write, process);
    futures_util::select! { _ = read.fuse() => {}, _ = write.fuse() => {}, _ = process.fuse() => {} }
}

async fn send_events(
    sender: &mpsc::Sender<Result<Frame<Bytes>, Infallible>>,
    events: Vec<Value>,
) -> bool {
    for event in events {
        if sender
            .send(Ok(Frame::data(Bytes::from(google_protocol::sse(&event)))))
            .await
            .is_err()
        {
            return false;
        }
    }
    true
}

async fn stream_google(
    response: reqwest::Response,
    sender: &mpsc::Sender<Result<Frame<Bytes>, Infallible>>,
    mapper: &mut ResponseMapper,
) -> Result<(), String> {
    let is_sse = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    loop {
        futures_util::select! {
            _ = sender.closed().fuse() => return Err("客户端已断开".into()),
            _ = heartbeat.tick().fuse() => { if sender.send(Ok(Frame::data(Bytes::from_static(b": keepalive\n\n")))).await.is_err() { return Err("客户端已断开".into()); } },
            chunk = stream.next().fuse() => {
                let Some(chunk) = chunk else { break; };
                buffer.extend_from_slice(&chunk.map_err(|_| "Google 流式连接中断")?);
                if buffer.len() > 32 * 1024 * 1024 { return Err("Google 单个响应片段过大".into()); }
                if is_sse {
                    while let Some((index, size)) = frame_boundary(&buffer) {
                        let frame: Vec<_> = buffer.drain(..index + size).collect();
                        let events = parse_frame(&frame, mapper)?;
                        if !send_events(sender, events).await { return Err("客户端已断开".into()); }
                    }
                }
            }
        }
    }
    if is_sse {
        if !buffer.is_empty() {
            send_events(sender, parse_frame(&buffer, mapper)?).await;
        }
    } else {
        let value: Value =
            serde_json::from_slice(&buffer).map_err(|_| "Google 返回的流不是有效 SSE 或 JSON")?;
        let values = match value {
            Value::Array(values) => values,
            value => vec![value],
        };
        for value in values {
            if !send_events(sender, mapper.chunk(&value)?).await {
                return Err("客户端已断开".into());
            }
        }
    }
    Ok(())
}

fn frame_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    bytes
        .windows(2)
        .position(|value| value == b"\n\n")
        .map(|index| (index, 2))
        .into_iter()
        .chain(
            bytes
                .windows(4)
                .position(|value| value == b"\r\n\r\n")
                .map(|index| (index, 4)),
        )
        .min_by_key(|(index, _)| *index)
}

fn parse_frame(bytes: &[u8], mapper: &mut ResponseMapper) -> Result<Vec<Value>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Google SSE 包含无效字符")?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(vec![]);
    }
    mapper.chunk(&serde_json::from_str::<Value>(&data).map_err(|_| "Google SSE 包含无效 JSON")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestSocket = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    async fn test_socket(port: u16) -> TestSocket {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://127.0.0.1:{port}/v1/responses")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("authorization", "Bearer ws-secret".parse().unwrap());
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }

    async fn terminal(socket: &mut TestSocket) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Message::Text(text) = socket.next().await.unwrap().unwrap() {
                    let event: Value = serde_json::from_str(&text).unwrap();
                    if matches!(
                        event["type"].as_str(),
                        Some("response.completed" | "response.failed" | "error")
                    ) {
                        return event;
                    }
                }
            }
        })
        .await
        .expect("本地 WebSocket 没有返回终态")
    }

    #[test]
    fn google_websocket_warmup_continuation_privacy_and_shutdown() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let root = std::env::temp_dir().join(format!("google-ws-test-{}", uuid::Uuid::new_v4()));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            let provider = json!({"id": "google-ws", "type": "google-account", "cli": "codex", "enabled": true,
                "google": {"port": port, "project": "p", "models": [{"id": "gemini"}]}, "runtimeConfig": {"mainModel": "gemini"}});
            let mut keys = serde_json::Map::new();
            runtime_provider::set_provider_key(&mut keys, "google-ws", "ws-secret".into()).unwrap();
            crate::core::provider_store::write_provider_bundle(&paths, &[provider.clone()], &[], &[], &keys).unwrap();
            ensure_started(&paths, &provider).await.unwrap();
            let mut socket = test_socket(port).await;
            socket.send(Message::Text(json!({"type": "response.create", "stream_id": "main", "generate": false, "store": false, "instructions": "仅本轮", "input": "第一轮"}).to_string().into())).await.unwrap();
            let first = terminal(&mut socket).await;
            assert_eq!(first["type"], "response.completed");
            assert_eq!(first["stream_id"], "main");
            let first_id = first["response"]["id"].as_str().unwrap();
            assert!(google_session::load(&paths, "google-ws", first_id).unwrap().is_none());
            let mut other = test_socket(port).await;
            other.send(Message::Text(json!({"type": "response.create", "generate": false, "previous_response_id": first_id, "input": "跨连接"}).to_string().into())).await.unwrap();
            assert_eq!(terminal(&mut other).await["error"]["code"], "previous_response_not_found");
            other.close(None).await.unwrap();
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let http = client.post(format!("http://127.0.0.1:{port}/responses")).bearer_auth("ws-secret").json(&json!({"previous_response_id": first_id, "input": "HTTP"})).send().await.unwrap();
            assert_eq!(http.status(), StatusCode::BAD_REQUEST);
            assert_eq!(http.json::<Value>().await.unwrap()["error"]["code"], "previous_response_not_found");
            assert_eq!(client.post(format!("http://127.0.0.1:{port}/v1/responses/compact")).bearer_auth("ws-secret").json(&json!({})).send().await.unwrap().status(), StatusCode::NOT_IMPLEMENTED);
            socket.send(Message::Text(json!({"type": "response.create", "generate": false, "previous_response_id": first_id, "input": "第二轮"}).to_string().into())).await.unwrap();
            let second = terminal(&mut socket).await;
            assert_eq!(second["type"], "response.completed");
            let second_id = second["response"]["id"].as_str().unwrap();
            let saved = google_session::load(&paths, "google-ws", second_id).unwrap().unwrap();
            assert_eq!(saved["input"].as_array().unwrap().len(), 2);
            assert!(saved.get("instructions").is_none());
            socket.send(Message::Ping(Bytes::from_static(b"ping"))).await.unwrap();
            assert!(matches!(tokio::time::timeout(Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap(), Message::Pong(_)));
            stop(&paths, "google-ws").await;
            let closed = tokio::time::timeout(Duration::from_secs(5), socket.next()).await.unwrap();
            assert!(closed.is_none() || closed.as_ref().is_some_and(|v| v.is_err() || matches!(v, Ok(Message::Close(_)))));
            ensure_started(&paths, &provider).await.unwrap();
            let mut restored = test_socket(port).await;
            restored.send(Message::Text(json!({"type": "response.create", "generate": false, "store": false, "previous_response_id": second_id, "input": "重启后"}).to_string().into())).await.unwrap();
            assert_eq!(terminal(&mut restored).await["type"], "response.completed");
            stop(&paths, "google-ws").await;
            std::fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn google_sse_parses_fragmented_utf8_and_crlf_frames() {
        let mut mapper = ResponseMapper::new(&json!({"model": "test"}));
        let frame = format!(
            "data: {}\r\n\r\n",
            json!({"response": {"candidates": [{"content": {"parts": [{"text": "中文🙂"}]}, "finishReason": "STOP"}]}})
        );
        let mut pending = Vec::new();
        for byte in frame.as_bytes() {
            pending.push(*byte);
            if let Some((index, size)) = frame_boundary(&pending) {
                let frame: Vec<_> = pending.drain(..index + size).collect();
                parse_frame(&frame, &mut mapper).unwrap();
            }
        }
        mapper.finish().unwrap();
        assert_eq!(mapper.response["output"][0]["content"][0]["text"], "中文🙂");
        assert!(pending.is_empty());
        assert!(parse_frame(b"data: invalid\n\n", &mut mapper).is_err());
    }

    #[test]
    fn google_gateway_requires_local_key_and_stops_on_disable() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let root = std::env::temp_dir().join(format!("google-gateway-test-{}", uuid::Uuid::new_v4()));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            let provider = json!({"id": "google-test", "type": "google-account", "cli": "codex", "enabled": true,
                "baseUrl": format!("http://127.0.0.1:{port}/v1"), "runtimeConfig": {"mainModel": "gemini"},
                "google": {"port": port, "models": [{"id": "gemini"}]}});
            let mut keys = serde_json::Map::new();
            runtime_provider::set_provider_key(&mut keys, "google-test", "local-secret".into()).unwrap();
            crate::core::provider_store::write_provider_bundle(&paths, &[provider.clone()], &[], &[], &keys).unwrap();
            ensure_started(&paths, &provider).await.unwrap();
            ensure_started(&paths, &provider).await.unwrap();
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let url = format!("http://127.0.0.1:{port}/v1/models");
            assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
            let models: Value = client.get(&url).bearer_auth("local-secret").send().await.unwrap().json().await.unwrap();
            assert_eq!(models["data"][0]["id"], "gemini");
            assert_eq!(client.get(&url).bearer_auth("local-secret").header("origin", "https://example.com").send().await.unwrap().status(), 403);
            runtime_provider::save_provider(&paths, json!({"id": "google-test", "enabled": false})).await.unwrap();
            assert!(ensure_started(&paths, &crate::api::google_account::find_account(&paths, "google-test").unwrap()).await.is_err());
            std::fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn google_runtime_enable_writes_loopback_config_and_restores_listener() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let root = std::env::temp_dir().join(format!("google-runtime-test-{}", uuid::Uuid::new_v4()));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let config_dir = root.join("codex");
            std::fs::create_dir_all(&config_dir).unwrap();
            std::fs::write(config_dir.join("config.toml"), "model = \"old\"\n[projects.\"D:/work\"]\ntrust_level = \"trusted\"\n").unwrap();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let provider = json!({"id": "google-runtime", "name": "Google", "type": "google-account", "cli": "codex", "enabled": true,
                "baseUrl": format!("http://127.0.0.1:{port}/v1"), "runtimeConfig": {"mainModel": "gemini"},
                "google": {"port": port, "project": "project", "models": [{"id": "gemini"}]}});
            let mut keys = serde_json::Map::new();
            runtime_provider::set_provider_key(&mut keys, "google-runtime", "local-runtime-token".into()).unwrap();
            crate::core::provider_store::write_provider_bundle(&paths, &[provider], &[], &[], &keys).unwrap();
            let targets = json!([{"id": "codex", "configPath": config_dir.to_str().unwrap()}]);
            let payload = json!({"cli": "codex", "providerId": "google-runtime", "model": "gemini"});
            assert!(runtime_provider::switch_runtime(&paths, payload.clone(), &targets).await.is_err());
            assert!(crate::core::provider_store::read_profiles(&paths).unwrap().is_empty());
            assert!(std::fs::read_to_string(config_dir.join("config.toml")).unwrap().contains("model = \"old\""));
            drop(listener);
            runtime_provider::switch_runtime(&paths, payload, &targets).await.unwrap();
            let config = std::fs::read_to_string(config_dir.join("config.toml")).unwrap();
            assert!(config.contains(&format!("http://127.0.0.1:{port}/v1")));
            assert!(config.contains("wire_api = \"responses\""));
            assert!(config.contains("trust_level = \"trusted\""));
            let auth: Value = serde_json::from_slice(&std::fs::read(config_dir.join("auth.json")).unwrap()).unwrap();
            assert_eq!(auth["OPENAI_API_KEY"], "local-runtime-token");
            stop(&paths, "google-runtime").await;
            tokio::task::yield_now().await;
            google_account::start_enabled(&paths).await.unwrap();
            let response = reqwest::Client::builder().no_proxy().build().unwrap().get(format!("http://127.0.0.1:{port}/v1/models"))
                .bearer_auth("local-runtime-token").send().await.unwrap();
            assert_eq!(response.status(), 200);
            stop(&paths, "google-runtime").await;
            std::fs::remove_dir_all(root).unwrap();
        });
    }
}

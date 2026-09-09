use super::*;
use http_body_util::Limited;
use std::time::Duration;
use tokio::net::UdpSocket;

const DISCOVERY_PORT: u16 = 17632;
const DISCOVERY_PROTOCOL: &str = "monkey-thief-device-drop-v1";
const PAIRING_LIFETIME: u64 = 90_000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Peer {
    id: String,
    name: String,
    ip: String,
    port: u16,
    #[serde(default)]
    last_seen_at: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct TrustedPeer {
    peer: Peer,
    secret: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    id: String,
    name: String,
    peers: Vec<TrustedPeer>,
    ignored_peers: Vec<String>,
}

struct Pairing {
    peer: Peer,
    secret: String,
    created_at: u64,
    status: String,
}

#[derive(Default)]
pub(super) struct NativeRuntime {
    config: Config,
    nearby: HashMap<String, Peer>,
    pairing: HashMap<String, Pairing>,
    connecting: HashMap<String, String>,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
    tasks: HashMap<String, tauri::async_runtime::JoinHandle<()>>,
    pub(super) error: String,
}

impl NativeRuntime {
    pub(super) fn stop(&mut self) {
        if let Some(handle) = self.handle.take() { handle.abort(); }
        for (_, task) in self.tasks.drain() { task.abort(); }
        self.nearby.clear();
        self.pairing.clear();
        self.connecting.clear();
    }

    pub(super) fn online(&self, id: &str) -> bool {
        self.nearby.get(id).map(|peer| now_millis().saturating_sub(peer.last_seen_at) < 15_000).unwrap_or(false)
    }

    pub(super) fn online_count(&self) -> usize {
        self.config.peers.iter().filter(|trusted| self.online(&trusted.peer.id)).count()
    }

    fn authenticate(&self, ip: &str, secret: &str) -> Option<Peer> {
        self.config.peers.iter().find(|trusted| trusted.peer.ip == ip && is_valid_token(&trusted.secret, secret)).map(|trusted| trusted.peer.clone())
    }

    pub(super) async fn forget(&mut self, paths: &AppPaths, device_id: &str) -> Result<(), ManagerError> {
        let mut config = if self.config.id.is_empty() {
            match tokio::fs::read(&paths.lan_share_files.config).await {
                Ok(bytes) => serde_json::from_slice::<Config>(&bytes)?,
                Err(error) if error.kind() == ErrorKind::NotFound => self.config.clone(),
                Err(error) => return Err(error.into()),
            }
        } else {
            self.config.clone()
        };
        if config.id == device_id {
            return Err(ManagerError::System("不能删除当前设备自身。".into()));
        }
        config.peers.retain(|trusted| trusted.peer.id != device_id);
        if !config.ignored_peers.iter().any(|id| id == device_id) {
            config.ignored_peers.push(device_id.to_string());
        }
        write_json(&paths.lan_share_files.config, &json!(config)).await?;
        self.config = config;
        self.nearby.remove(device_id);
        self.pairing.retain(|_, request| request.peer.id != device_id);
        self.connecting.remove(device_id);
        if let Some(task) = self.tasks.remove(device_id) { task.abort(); }
        Ok(())
    }

    pub(super) fn snapshot(&self) -> Value {
        let mut peers = self.config.peers.iter().map(|trusted| trusted.peer.clone()).collect::<Vec<_>>();
        for discovered in self.nearby.values().filter(|peer| self.online(&peer.id) && !self.config.ignored_peers.contains(&peer.id)) {
            if let Some(peer) = peers.iter_mut().find(|peer| peer.id == discovered.id) { *peer = discovered.clone(); }
            else { peers.push(discovered.clone()); }
        }
        peers.sort_by(|left, right| left.name.cmp(&right.name));
        json!({
            "deviceName": self.config.name,
            "deviceId": self.config.id,
            "error": self.error,
            "peers": peers.iter().map(|peer| json!({
                "id": peer.id, "name": peer.name, "ip": peer.ip, "port": peer.port,
                "online": self.online(&peer.id),
                "paired": self.config.peers.iter().any(|trusted| trusted.peer.id == peer.id && trusted.peer.ip == peer.ip && trusted.peer.port == peer.port),
                "pairingCode": self.connecting.get(&peer.id)
            })).collect::<Vec<_>>(),
            "pairingRequests": self.pairing.iter().filter(|(_, request)| request.status == "pending" && now_millis().saturating_sub(request.created_at) < PAIRING_LIFETIME).map(|(id, request)| json!({
                "id": id, "name": request.peer.name, "ip": request.peer.ip, "code": pairing_code(&request.secret)
            })).collect::<Vec<_>>()
        })
    }
}

fn pairing_code(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(secret.as_bytes());
    format!("{:02X}{:02X}{:02X}", digest[0], digest[1], digest[2])
}

fn peer_origin(peer: &Peer) -> Result<String, ManagerError> {
    let ip: Ipv4Addr = peer.ip.parse().map_err(|_| ManagerError::System("请输入局域网 IPv4 地址。".into()))?;
    if !(ip.is_private() || ip.is_loopback() || ip.is_link_local()) || peer.port == 0 {
        return Err(ManagerError::System("设备快传仅连接局域网地址。".into()));
    }
    Ok(format!("http://{}:{}", ip, peer.port))
}

fn discovery_peer(bytes: &[u8], source: SocketAddr, own_id: &str) -> Option<Peer> {
    let payload: Value = serde_json::from_slice(bytes).ok()?;
    if payload.get("protocol")?.as_str()? != DISCOVERY_PROTOCOL { return None; }
    let mut peer: Peer = serde_json::from_value(payload.get("peer")?.clone()).ok()?;
    if peer.id == own_id || !peer.id.starts_with("native-") || peer.id.len() > 80 || peer.name.len() > 128 { return None; }
    peer.ip = source.ip().to_string();
    peer_origin(&peer).ok()?;
    peer.last_seen_at = now_millis();
    Some(peer)
}

pub(super) async fn start(app: tauri::AppHandle, registry: &LanShareServerRegistry, paths: &AppPaths) -> Result<(), ManagerError> {
    let mut config: Config = match tokio::fs::read(&paths.lan_share_files.config).await {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == ErrorKind::NotFound => Config::default(),
        Err(error) => return Err(error.into()),
    };
    if config.id.is_empty() { config.id = create_id("native"); }
    if config.name.is_empty() {
        config.name = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "我的电脑".into());
    }
    write_json(&paths.lan_share_files.config, &json!(config)).await?;
    let (own_id, peer) = {
        let mut runtime = registry.inner.lock().await;
        runtime.native.config = config.clone();
        runtime.native.error.clear();
        (config.id.clone(), Peer { id: config.id, name: config.name, ip: runtime.lan_ip.clone(), port: runtime.port, last_seen_at: 0 })
    };
    let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DISCOVERY_PORT))).await {
        Ok(socket) => socket,
        Err(_) => {
            registry.inner.lock().await.native.error = "自动发现暂不可用，可通过局域网地址连接设备。".into();
            return Ok(());
        }
    };
    socket.set_broadcast(true)?;
    let announcement = serde_json::to_vec(&json!({ "protocol": DISCOVERY_PROTOCOL, "peer": peer }))?;
    let task_registry = registry.clone();
    let task_paths = paths.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        let mut buffer = [0u8; 2048];
        loop {
            let packet = match futures_util::future::select(Box::pin(interval.tick()), Box::pin(socket.recv_from(&mut buffer))).await {
                futures_util::future::Either::Left((_, pending)) => {
                    drop(pending);
                    let _ = socket.send_to(&announcement, SocketAddr::from(([255, 255, 255, 255], DISCOVERY_PORT))).await;
                    {
                        let mut runtime = task_registry.inner.lock().await;
                        runtime.native.nearby.retain(|_, peer| now_millis().saturating_sub(peer.last_seen_at) < 15_000);
                        runtime.native.pairing.retain(|_, request| now_millis().saturating_sub(request.created_at) < PAIRING_LIFETIME);
                    }
                    let _ = emit_state_changed(&app, &task_registry, &task_paths).await;
                    None
                },
                futures_util::future::Either::Right((packet, pending)) => {
                    drop(pending);
                    Some(packet)
                }
            };
            if let Some(Ok((size, source))) = packet {
                if let Some(peer) = discovery_peer(&buffer[..size], source, &own_id) {
                    let mut runtime = task_registry.inner.lock().await;
                    if runtime.native.nearby.len() < 100 || runtime.native.nearby.contains_key(&peer.id) {
                        runtime.native.nearby.insert(peer.id.clone(), peer);
                    }
                }
            }
        }
    });
    registry.inner.lock().await.native.handle = Some(handle);
    Ok(())
}

async fn response_data(response: reqwest::Response) -> Result<Value, ManagerError> {
    let status = response.status();
    let payload: Value = response.json().await.map_err(|_| ManagerError::System("对方没有返回有效的设备快传响应。".into()))?;
    if !status.is_success() || payload["status"] != "success" {
        return Err(ManagerError::System(string_value(payload.get("message"))));
    }
    Ok(payload["data"].clone())
}

fn network_error(error: reqwest::Error) -> ManagerError {
    ManagerError::System(format!("设备连接失败：{}", error.without_url()))
}

async fn remember(registry: &LanShareServerRegistry, paths: &AppPaths, trusted: TrustedPeer) -> Result<(), ManagerError> {
    upsert_device(registry, paths, &trusted.peer.id, &trusted.peer.name, "MonkeyThief/Desktop", &trusted.peer.ip).await?;
    let _storage = registry.storage.lock().await;
    ensure_session(paths, &trusted.peer.id, "", &trusted.peer.name, &trusted.peer.ip)?;
    let mut runtime = registry.inner.lock().await;
    let mut config = runtime.native.config.clone();
    config.peers.retain(|current| current.peer.id != trusted.peer.id);
    config.ignored_peers.retain(|id| id != &trusted.peer.id);
    config.peers.push(trusted.clone());
    write_json(&paths.lan_share_files.config, &json!(config)).await?;
    runtime.native.nearby.insert(trusted.peer.id.clone(), trusted.peer.clone());
    runtime.native.config = config;
    Ok(())
}

pub async fn connect_peer(app: tauri::AppHandle, registry: &LanShareServerRegistry, paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let peer_id = string_value(payload.get("peerId"));
    let address = string_value(payload.get("address"));
    let peer = if !address.is_empty() {
        let address = if address.contains("://") { address } else { format!("http://{address}") };
        let url = Url::parse(&address).map_err(|_| ManagerError::System("连接地址格式不正确。".into()))?;
        Peer { ip: url.host_str().unwrap_or_default().into(), port: url.port().unwrap_or(DEFAULT_PORT), ..Peer::default() }
    } else {
        registry.inner.lock().await.native.nearby.get(&peer_id).cloned().ok_or_else(|| ManagerError::System("设备暂未发现，请检查对方是否已打开设备快传。".into()))?
    };
    let origin = peer_origin(&peer)?;
    let client = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().map_err(network_error)?;
    let info = response_data(client.get(format!("{origin}/api/native/info")).send().await.map_err(network_error)?).await?;
    let endpoint = peer;
    let mut peer: Peer = serde_json::from_value(info)?;
    peer.ip = endpoint.ip;
    peer.port = endpoint.port;
    peer.last_seen_at = now_millis();
    let (identity, own_port) = {
        let runtime = registry.inner.lock().await;
        if runtime.handle.is_none() { return Err(ManagerError::System("请先上线设备快传。".into())); }
        if runtime.native.config.id == peer.id { return Err(ManagerError::System("不能连接当前设备自身。".into())); }
        if runtime.native.connecting.contains_key(&peer.id) { return Ok(lan_share_response(json!(true))); }
        (runtime.native.config.clone(), runtime.port)
    };
    if !peer.id.starts_with("native-") || peer.id.len() > 80 { return Err(ManagerError::System("对方不是兼容的设备快传客户端。".into())); }
    let secret = create_token()?;
    let request_id = create_id("pair");
    response_data(client.post(format!("{origin}/api/native/pair")).json(&json!({
        "requestId": request_id, "secret": secret, "peer": { "id": identity.id, "name": identity.name, "ip": "", "port": own_port }
    })).send().await.map_err(network_error)?).await?;
    {
        let mut runtime = registry.inner.lock().await;
        runtime.native.error.clear();
        runtime.native.nearby.insert(peer.id.clone(), peer.clone());
        runtime.native.connecting.insert(peer.id.clone(), pairing_code(&secret));
    }
    let task_registry = registry.clone();
    let task_paths = paths.clone();
    let task_peer_id = peer.id.clone();
    let task_code = pairing_code(&secret);
    let handle = tauri::async_runtime::spawn(async move {
        let result = async {
            let started = now_millis();
            while now_millis().saturating_sub(started) < PAIRING_LIFETIME {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let status = response_data(client.get(format!("{origin}/api/native/pair")).query(&[("id", &request_id)]).bearer_auth(&secret).send().await.map_err(network_error)?).await?;
                match status["status"].as_str() {
                    Some("accepted") => return remember(&task_registry, &task_paths, TrustedPeer { peer: peer.clone(), secret: secret.clone() }).await,
                    Some("rejected") => return Err(ManagerError::System("对方拒绝了连接请求。".into())),
                    _ => {},
                }
            }
            Err(ManagerError::System("连接确认已过期，请重新连接。".into()))
        }.await;
        {
            let mut runtime = task_registry.inner.lock().await;
            runtime.native.connecting.remove(&peer.id);
            if let Err(error) = result { runtime.native.error = error.to_string(); }
        }
        let _ = emit_state_changed(&app, &task_registry, &task_paths).await;
    });
    {
        let mut runtime = registry.inner.lock().await;
        if runtime.native.connecting.get(&task_peer_id) == Some(&task_code) {
            if let Some(previous) = runtime.native.tasks.insert(task_peer_id, handle) { previous.abort(); }
        } else {
            handle.abort();
        }
    }
    get_state(registry, paths).await
}

pub async fn respond_pairing(app: tauri::AppHandle, registry: &LanShareServerRegistry, paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let identifier = string_value(payload.get("requestId"));
    let accepted = payload.get("accept").and_then(Value::as_bool).unwrap_or(false);
    let trusted = {
        let runtime = registry.inner.lock().await;
        let request = runtime.native.pairing.get(&identifier).ok_or_else(|| ManagerError::System("连接请求已过期。".into()))?;
        if now_millis().saturating_sub(request.created_at) >= PAIRING_LIFETIME || request.status != "pending" { return Err(ManagerError::System("连接请求已过期或已处理。".into())); }
        TrustedPeer { peer: request.peer.clone(), secret: request.secret.clone() }
    };
    if accepted { remember(registry, paths, trusted).await?; }
    if let Some(request) = registry.inner.lock().await.native.pairing.get_mut(&identifier) { request.status = if accepted { "accepted" } else { "rejected" }.into(); }
    emit_state_changed(&app, registry, paths).await?;
    get_state(registry, paths).await
}

async fn read_payload(request: Request<Incoming>) -> Result<Value, ManagerError> {
    let bytes = Limited::new(request.into_body(), 256 * 1024).collect().await
        .map_err(|_| ManagerError::System("消息内容过大或接收中断。".into()))?.to_bytes();
    Ok(serde_json::from_slice(&bytes)?)
}

async fn peer_session(context: &HttpContext, peer: &Peer, requested: &str) -> Result<LanShareSession, ManagerError> {
    let _storage = context.registry.storage.lock().await;
    if !requested.is_empty() {
        return read_array::<LanShareSession>(&context.paths.lan_share_files.sessions)?.into_iter()
            .find(|session| session.id == requested && session.device_id == peer.id && session.mode != "group")
            .ok_or_else(|| ManagerError::System("会话不属于当前设备。".into()));
    }
    ensure_session(&context.paths, &peer.id, "", &peer.name, &peer.ip)
}

pub(super) async fn handle_request(request: Request<Incoming>, context: &HttpContext, address: SocketAddr) -> Result<Response<BoxBody>, ManagerError> {
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    let authorization = request.headers().get("authorization").and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer ")).unwrap_or_default().to_string();
    if method == Method::GET && path == "/api/native/info" {
        let runtime = context.registry.inner.lock().await;
        return Ok(api_success(json!(Peer {
            id: runtime.native.config.id.clone(), name: runtime.native.config.name.clone(),
            ip: runtime.lan_ip.clone(), port: runtime.port, last_seen_at: now_millis()
        })));
    }
    if method == Method::POST && path == "/api/native/pair" {
        let payload = read_payload(request).await?;
        let identifier = string_value(payload.get("requestId"));
        let secret = string_value(payload.get("secret"));
        let mut peer: Peer = serde_json::from_value(payload["peer"].clone())?;
        peer.ip = client_ip(address);
        peer.last_seen_at = now_millis();
        peer_origin(&peer)?;
        if !peer.id.starts_with("native-") || peer.id.len() > 80 || peer.name.len() > 128 || identifier.len() > 80 || identifier.is_empty() || secret.len() < 32 || secret.len() > 128 {
            return Ok(api_error(StatusCode::BAD_REQUEST, "连接请求无效。"));
        }
        {
            let mut runtime = context.registry.inner.lock().await;
            runtime.native.pairing.retain(|_, request| now_millis().saturating_sub(request.created_at) < PAIRING_LIFETIME);
            if runtime.native.pairing.len() >= 20 || runtime.native.pairing.contains_key(&identifier) {
                return Ok(api_error(StatusCode::TOO_MANY_REQUESTS, "连接请求过多，请稍后重试。"));
            }
            runtime.native.pairing.insert(identifier, Pairing { peer, secret, created_at: now_millis(), status: "pending".into() });
        }
        emit_state_changed(&context.app, &context.registry, &context.paths).await?;
        return Ok(api_success(json!(true)));
    }
    if method == Method::GET && path == "/api/native/pair" {
        let identifier = query_value(request.uri(), "id");
        let runtime = context.registry.inner.lock().await;
        let Some(pairing) = runtime.native.pairing.get(&identifier).filter(|pairing| pairing.peer.ip == client_ip(address) && is_valid_token(&pairing.secret, &authorization) && now_millis().saturating_sub(pairing.created_at) < PAIRING_LIFETIME) else {
            return Ok(api_error(StatusCode::UNAUTHORIZED, "连接确认无效或已过期。"));
        };
        return Ok(api_success(json!({ "status": pairing.status })));
    }
    let peer = {
        let runtime = context.registry.inner.lock().await;
        runtime.native.authenticate(&client_ip(address), &authorization)
    };
    let Some(peer) = peer else { return Ok(api_error(StatusCode::UNAUTHORIZED, "设备尚未配对，请在客户端确认连接。")); };
    if method == Method::GET && path == "/api/native/messages" {
        let message_id = format!("{}-{}", peer.id, query_value(request.uri(), "messageId"));
        let _storage = context.registry.storage.lock().await;
        let exists = read_array::<LanShareMessage>(&context.paths.lan_share_files.messages)?.iter().any(|message| message.id == message_id && message.device_id == peer.id);
        return Ok(api_success(json!({ "exists": exists })));
    }
    if method == Method::PUT && path == "/api/native/files" {
        let session = peer_session(context, &peer, &query_value(request.uri(), "sessionId")).await?;
        let name = query_value(request.uri(), "name");
        let file = attachments::upload(request.into_body(), &context.registry, &context.paths, &session.id, &name).await?;
        return Ok(api_success(json!({ "file": attachments::Attachment::from(&file), "sessionId": session.id })));
    }
    if method == Method::POST && path == "/api/native/messages" {
        let payload = read_payload(request).await?;
        let source_id = string_value(payload.get("messageId"));
        if source_id.is_empty() || source_id.len() > 100 || !source_id.chars().all(|character| character.is_ascii_alphanumeric() || character == '-') {
            return Ok(api_error(StatusCode::BAD_REQUEST, "消息标识无效。"));
        }
        let message_id = format!("{}-{source_id}", peer.id);
        let session = peer_session(context, &peer, &string_value(payload.get("sessionId"))).await?;
        let content = string_value(payload.get("content"));
        let files = attachments::prepare(&context.registry, &context.paths, &session.id, &json!({ "attachmentIds": payload["attachmentIds"] })).await?;
        let stored = append_message_with_attachments(&context.registry, &context.paths, &peer.id, &session.id, "mobile-to-desktop", &content, true, &files, Some(&message_id)).await?;
        attachments::publish(&context.registry, &context.paths, &files).await?;
        emit_message_created(&context.app, &stored);
        emit_state_changed(&context.app, &context.registry, &context.paths).await?;
        return Ok(api_success(json!({ "messageId": stored.id })));
    }
    Ok(api_error(StatusCode::NOT_FOUND, "请求路径不存在。"))
}

pub(super) async fn deliver(registry: &LanShareServerRegistry, device_id: &str, content: &str, files: &[LanShareFile], message_id: &str) -> Result<bool, ManagerError> {
    let trusted = {
        let runtime = registry.inner.lock().await;
        runtime.native.config.peers.iter().find(|trusted| trusted.peer.id == device_id).cloned()
    };
    let Some(trusted) = trusted else { return Ok(false); };
    let origin = peer_origin(&trusted.peer)?;
    let client = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().map_err(network_error)?;
    let mut remote_session = String::new();
    let existing = response_data(client.get(format!("{origin}/api/native/messages")).query(&[("messageId", message_id)]).bearer_auth(&trusted.secret).send().await.map_err(network_error)?).await?;
    if existing["exists"] == true { return Ok(true); }
    let mut identifiers = Vec::new();
    for file in files {
        let body = reqwest::Body::wrap_stream(ReaderStream::new(File::open(&file.path).await?));
        let uploaded = response_data(client.put(format!("{origin}/api/native/files"))
            .query(&[("name", &file.name), ("sessionId", &remote_session)])
            .bearer_auth(&trusted.secret).header(CONTENT_LENGTH, file.size).body(body).send().await.map_err(network_error)?).await?;
        identifiers.push(string_value(uploaded["file"].get("id")));
        remote_session = string_value(uploaded.get("sessionId"));
    }
    response_data(client.post(format!("{origin}/api/native/messages")).bearer_auth(&trusted.secret).json(&json!({
        "messageId": message_id, "sessionId": remote_session, "content": content, "attachmentIds": identifiers
    })).send().await.map_err(network_error)?).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_native_device_revokes_pairing_and_hides_future_discovery() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-delete-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let removed = Peer { id: "native-removed".into(), name: "移除设备".into(), ip: "192.168.1.5".into(), port: DEFAULT_PORT, last_seen_at: now_millis() };
            let kept = Peer { id: "native-kept".into(), name: "保留设备".into(), ip: "192.168.1.6".into(), port: DEFAULT_PORT, last_seen_at: now_millis() };
            let config = Config {
                id: "native-local".into(), name: "本机".into(),
                peers: vec![TrustedPeer { peer: removed.clone(), secret: "removed-secret".into() }, TrustedPeer { peer: kept.clone(), secret: "kept-secret".into() }],
                ..Config::default()
            };
            write_json(&paths.lan_share_files.config, &json!(config)).await.unwrap();
            let mut native = NativeRuntime::default();
            native.nearby.insert(removed.id.clone(), removed.clone());
            native.connecting.insert(removed.id.clone(), "123456".into());
            native.pairing.insert("pending-request".into(), Pairing { peer: removed.clone(), secret: "pending-secret".into(), created_at: now_millis(), status: "pending".into() });
            native.tasks.insert(removed.id.clone(), tauri::async_runtime::spawn(std::future::pending::<()>()));
            native.forget(&paths, &removed.id).await.unwrap();
            assert!(native.authenticate(&removed.ip, "removed-secret").is_none());
            assert!(native.authenticate(&kept.ip, "kept-secret").is_some());
            assert!(!native.tasks.contains_key(&removed.id));
            assert!(!native.connecting.contains_key(&removed.id));
            assert!(native.pairing.is_empty());
            let saved: Config = serde_json::from_slice(&tokio::fs::read(&paths.lan_share_files.config).await.unwrap()).unwrap();
            assert_eq!(saved.id, "native-local");
            assert_eq!(saved.peers.len(), 1);
            native.config = saved;
            native.nearby.insert(removed.id.clone(), removed.clone());
            assert!(native.snapshot()["peers"].as_array().unwrap().iter().all(|peer| peer["id"] != removed.id));
            assert!(native.forget(&paths, "native-local").await.is_err());
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native = native;
            remember(&registry, &paths, TrustedPeer { peer: removed.clone(), secret: "new-approved-secret".into() }).await.unwrap();
            let snapshot = registry.inner.lock().await.native.snapshot();
            assert!(snapshot["peers"].as_array().unwrap().iter().any(|peer| peer["id"] == removed.id && peer["paired"] == true));
            assert!(!registry.inner.lock().await.native.config.ignored_peers.contains(&removed.id));
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn paired_credentials_are_bound_to_the_approved_peer_address() {
        let mut runtime = NativeRuntime::default();
        runtime.config.peers.push(TrustedPeer {
            peer: Peer { id: "native-approved".into(), ip: "192.168.1.5".into(), port: DEFAULT_PORT, ..Peer::default() },
            secret: "approved-secret".into(),
        });
        assert!(runtime.authenticate("192.168.1.5", "approved-secret").is_some());
        assert!(runtime.authenticate("192.168.1.6", "approved-secret").is_none());
        assert!(runtime.authenticate("192.168.1.5", "wrong-secret").is_none());
        assert!(runtime.authenticate("192.168.1.5", "").is_none());
    }

    #[test]
    fn native_and_browser_devices_on_same_address_keep_separate_histories() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-identity-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir).await.unwrap();
            let registry = LanShareServerRegistry::new();
            upsert_device(&registry, &paths, "native-computer", "电脑", "MonkeyThief/Desktop", "192.168.1.6").await.unwrap();
            upsert_device(&registry, &paths, "", "网页", "browser", "192.168.1.6").await.unwrap();
            let devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices).unwrap();
            assert_eq!(devices.len(), 2);
            assert!(devices.iter().any(|device| device.id == "native-computer"));
            assert!(devices.iter().any(|device| device.id == device_id_from_ip("192.168.1.6")));
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn native_batch_streams_two_files_then_one_message_and_retry_is_idempotent() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-transfer-test"));
            tokio::fs::create_dir_all(&root).await.unwrap();
            let first_path = root.join("first.txt");
            let second_path = root.join("second.png");
            tokio::fs::write(&first_path, b"first-file").await.unwrap();
            tokio::fs::write(&second_path, b"second-file").await.unwrap();
            let files = vec![file_payload(&first_path.to_string_lossy(), "local").await.unwrap(), file_payload(&second_path.to_string_lossy(), "local").await.unwrap()];
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let records = Arc::new(Mutex::new(Vec::<(String, String, Vec<u8>)>::new()));
            let server_records = records.clone();
            let server = tauri::async_runtime::spawn(async move {
                for _request in 0..5 {
                    let (socket, _) = listener.accept().await.unwrap();
                    let records = server_records.clone();
                    let service = service_fn(move |request: Request<Incoming>| {
                        let records = records.clone();
                        async move {
                            assert_eq!(request.headers()["authorization"], "Bearer fixture-secret");
                            let method = request.method().to_string();
                            let path = request.uri().path().to_string();
                            let body = request.into_body().collect().await.unwrap().to_bytes().to_vec();
                            let mut records = records.lock().await;
                            let data = if method == "GET" {
                                json!({ "exists": records.iter().any(|(method, path, _)| method == "POST" && path == "/api/native/messages") })
                            } else if method == "PUT" {
                                json!({ "file": { "id": format!("remote-{}", records.len()) }, "sessionId": "remote-session" })
                            } else { json!({ "messageId": "received" }) };
                            records.push((method, path, body));
                            let mut response = api_success(data);
                            response.headers_mut().insert(CONNECTION, HeaderValue::from_static("close"));
                            Ok::<_, Infallible>(response)
                        }
                    });
                    http1::Builder::new().keep_alive(false).serve_connection(TokioIo::new(socket), service).await.unwrap();
                }
            });
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.peers.push(TrustedPeer {
                peer: Peer { id: "native-remote".into(), ip: "127.0.0.1".into(), port, ..Peer::default() }, secret: "fixture-secret".into(),
            });
            assert!(deliver(&registry, "native-remote", "两个附件", &files, "batch-message").await.unwrap());
            assert!(deliver(&registry, "native-remote", "两个附件", &files, "batch-message").await.unwrap());
            server.await.unwrap();
            let records = records.lock().await;
            assert_eq!(records.len(), 5);
            let uploads = records.iter().filter(|record| record.0 == "PUT").collect::<Vec<_>>();
            assert_eq!(uploads.len(), 2);
            assert_eq!(uploads[0].2, b"first-file");
            assert_eq!(uploads[1].2, b"second-file");
            let posts = records.iter().filter(|record| record.0 == "POST").collect::<Vec<_>>();
            assert_eq!(posts.len(), 1);
            let payload: Value = serde_json::from_slice(&posts[0].2).unwrap();
            assert_eq!(payload["attachmentIds"].as_array().unwrap().len(), 2);
            assert_eq!(payload["sessionId"], "remote-session");
            assert_eq!(payload["content"], "两个附件");
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn discovery_uses_packet_source_and_does_not_publish_secrets() {
        let payload = json!({ "protocol": DISCOVERY_PROTOCOL, "peer": { "id": "native-other", "name": "另一台电脑", "ip": "8.8.8.8", "port": DEFAULT_PORT } });
        let peer = discovery_peer(&serde_json::to_vec(&payload).unwrap(), "192.168.1.9:17632".parse().unwrap(), "native-self").unwrap();
        assert_eq!(peer.ip, "192.168.1.9");
        assert!(discovery_peer(&serde_json::to_vec(&payload).unwrap(), "8.8.8.8:17632".parse().unwrap(), "native-self").is_none());
        assert!(discovery_peer(&serde_json::to_vec(&payload).unwrap(), "192.168.1.9:17632".parse().unwrap(), "native-other").is_none());
        let mut runtime = NativeRuntime::default();
        runtime.config.peers.push(TrustedPeer { peer, secret: "do-not-expose".into() });
        assert!(!runtime.snapshot().to_string().contains("do-not-expose"));
    }

    #[test]
    fn native_connections_only_target_local_ipv4_addresses() {
        for ip in ["8.8.8.8", "example.com", "0.0.0.0", "255.255.255.255"] {
            assert!(peer_origin(&Peer { ip: ip.into(), port: DEFAULT_PORT, ..Peer::default() }).is_err());
        }
        assert!(peer_origin(&Peer { ip: "192.168.1.2".into(), port: DEFAULT_PORT, ..Peer::default() }).is_ok());
    }
}

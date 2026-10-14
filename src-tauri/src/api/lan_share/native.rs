use super::*;
use futures_util::FutureExt;
use http_body_util::Limited;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;

const DISCOVERY_PORT: u16 = 17632;
const DISCOVERY_PROTOCOL: &str = "monkey-thief-device-drop-v2";
const PAIRING_LIFETIME: u64 = 90_000;

mod discovery;
mod transport;
use transport::{secure_call, TransportAuth};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiscoveryPacketKind {
    Announce,
    Probe,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Peer {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) ip: String,
    pub(super) port: u16,
    #[serde(default)]
    pub(super) last_seen_at: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct TrustedPeer {
    pub(super) peer: Peer,
    pub(super) secret: String,
    #[serde(default)]
    pub(super) secure: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Config {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(default = "default_auto_discovery")]
    pub(super) auto_discovery: bool,
    #[serde(default)]
    pub(super) discovery_configured: bool,
    pub(super) peers: Vec<TrustedPeer>,
    pub(super) ignored_peers: Vec<String>,
    pub(super) transfer: transfer::Settings,
}

fn default_auto_discovery() -> bool {
    true
}

fn migrate_discovery_config(config: &mut Config) {
    if !config.discovery_configured {
        config.auto_discovery = true;
        config.discovery_configured = true;
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            auto_discovery: default_auto_discovery(),
            discovery_configured: true,
            peers: Vec::new(),
            ignored_peers: Vec::new(),
            transfer: transfer::Settings::default(),
        }
    }
}

pub(super) struct Pairing {
    pub(super) peer: Peer,
    pub(super) secret: String,
    pub(super) created_at: u64,
    pub(super) status: String,
}

#[derive(Default)]
pub(super) struct NativeRuntime {
    pub(super) config: Config,
    pub(super) nearby: HashMap<String, Peer>,
    pub(super) pairing: HashMap<String, Pairing>,
    pub(super) connecting: HashMap<String, String>,
    pub(super) scan_trigger: Option<tokio::sync::mpsc::Sender<()>>,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
    tasks: HashMap<String, tauri::async_runtime::JoinHandle<()>>,
    discovery_tasks: HashMap<String, (Peer, tauri::async_runtime::JoinHandle<()>)>,
    delivery_tasks: HashMap<String, tauri::async_runtime::JoinHandle<()>>,
    replay: transport::ReplayGuard,
    pub(super) error: String,
}

impl NativeRuntime {
    pub(super) fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
        for (_, task) in self.tasks.drain() {
            task.abort();
        }
        for (_, (_, task)) in self.discovery_tasks.drain() {
            task.abort();
        }
        for (_, task) in self.delivery_tasks.drain() {
            task.abort();
        }
        self.replay = transport::ReplayGuard::default();
        self.scan_trigger = None;
        self.nearby.clear();
        self.pairing.clear();
        self.connecting.clear();
    }

    pub(super) fn online(&self, id: &str) -> bool {
        self.nearby
            .get(id)
            .map(|peer| now_millis().saturating_sub(peer.last_seen_at) < 15_000)
            .unwrap_or(false)
    }

    pub(super) fn online_count(&self) -> usize {
        self.config
            .peers
            .iter()
            .filter(|trusted| self.online(&trusted.peer.id))
            .count()
    }

    fn authenticate(&self, id: &str) -> Option<TrustedPeer> {
        self.config
            .peers
            .iter()
            .find(|trusted| trusted.peer.id == id && trusted.secure && !trusted.secret.is_empty())
            .cloned()
    }

    pub(super) async fn forget(
        &mut self,
        paths: &AppPaths,
        device_id: &str,
    ) -> Result<(), ManagerError> {
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
        self.pairing
            .retain(|_, request| request.peer.id != device_id);
        self.connecting.remove(device_id);
        if let Some(task) = self.tasks.remove(device_id) {
            task.abort();
        }
        if let Some((_, task)) = self.discovery_tasks.remove(device_id) {
            task.abort();
        }
        if let Some(task) = self.delivery_tasks.remove(device_id) {
            task.abort();
        }
        Ok(())
    }

    pub(super) fn snapshot(&self) -> Value {
        let mut peers = self
            .config
            .peers
            .iter()
            .map(|trusted| trusted.peer.clone())
            .collect::<Vec<_>>();
        for discovered in self
            .nearby
            .values()
            .filter(|peer| self.online(&peer.id) && !self.config.ignored_peers.contains(&peer.id))
        {
            if let Some(peer) = peers.iter_mut().find(|peer| peer.id == discovered.id) {
                *peer = discovered.clone();
            } else {
                peers.push(discovered.clone());
            }
        }
        peers.sort_by(|left, right| left.name.cmp(&right.name));
        json!({
            "deviceName": self.config.name,
            "deviceId": self.config.id,
            "autoDiscovery": self.config.auto_discovery,
            "transfer": self.config.transfer,
            "error": self.error,
            "peers": peers.iter().map(|peer| json!({
                "id": peer.id, "name": peer.name, "ip": peer.ip, "port": peer.port,
                "online": self.online(&peer.id),
                "paired": self.config.peers.iter().any(|trusted| trusted.peer.id == peer.id && trusted.secure),
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

fn is_native_id(id: &str) -> bool {
    (id.starts_with("native-") || id.starts_with("native_"))
        && id.len() <= 80
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

fn create_native_id() -> String {
    format!("native-{}_{}", now_millis(), uuid::Uuid::new_v4())
}

fn normalize_native_id(id: &mut String) {
    if id.is_empty() || id.starts_with("native_") {
        *id = create_native_id();
    }
}

fn peer_origin(peer: &Peer) -> Result<String, ManagerError> {
    let ip: Ipv4Addr = peer
        .ip
        .parse()
        .map_err(|_| ManagerError::System("请输入局域网 IPv4 地址。".into()))?;
    if !(ip.is_private() || ip.is_loopback() || ip.is_link_local()) || peer.port == 0 {
        return Err(ManagerError::System("设备快传仅连接局域网地址。".into()));
    }
    Ok(format!("http://{}:{}", ip, peer.port))
}

pub(super) async fn start(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<(), ManagerError> {
    let mut config: Config = match tokio::fs::read(&paths.lan_share_files.config).await {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == ErrorKind::NotFound => Config::default(),
        Err(error) => return Err(error.into()),
    };
    migrate_discovery_config(&mut config);
    config.transfer.validate()?;
    registry.transfer.configure(config.transfer);
    normalize_native_id(&mut config.id);
    if config.name.is_empty() {
        config.name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "我的电脑".into());
    }
    write_json(&paths.lan_share_files.config, &json!(config)).await?;
    let (own_id, peer) = {
        let mut runtime = registry.inner.lock().await;
        runtime.native.config = config.clone();
        runtime.native.error.clear();
        (
            config.id.clone(),
            Peer {
                id: config.id,
                name: config.name,
                ip: runtime.lan_ip.clone(),
                port: runtime.port,
                last_seen_at: 0,
            },
        )
    };
    let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DISCOVERY_PORT))).await {
        Ok(socket) => socket,
        Err(_) => {
            registry.inner.lock().await.native.error =
                "自动发现暂不可用，可通过局域网地址连接设备。".into();
            return Ok(());
        }
    };
    socket.set_broadcast(true)?;
    let (scan_tx, mut scan_rx) = tokio::sync::mpsc::channel::<()>(8);
    registry.inner.lock().await.native.scan_trigger = Some(scan_tx);
    let task_registry = registry.clone();
    let task_paths = paths.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        let mut buffer = [0u8; 2048];
        let mut broadcast_targets = discovery::targets()
            .unwrap_or_else(|_| vec![SocketAddr::from(([255, 255, 255, 255], DISCOVERY_PORT))]);
        let mut targets_refreshed_at = 0u64;
        loop {
            let packet = futures_util::select! {
                _ = interval.tick().fuse() => {
                    if now_millis().saturating_sub(targets_refreshed_at) >= 15_000 {
                        if let Ok(targets) = discovery::targets() {
                            broadcast_targets = targets;
                        }
                        targets_refreshed_at = now_millis();
                    }
                    let should_broadcast = {
                        let runtime = task_registry.inner.lock().await;
                        runtime.native.config.auto_discovery
                    };
                    if should_broadcast {
                        if let Ok(packet) = discovery::packet("announce", &peer) {
                            for target in &broadcast_targets {
                                let _ = socket.send_to(&packet, target).await;
                            }
                        }
                    }
                    {
                        let mut runtime = task_registry.inner.lock().await;
                        runtime.native.nearby.retain(|_, peer| {
                            now_millis().saturating_sub(peer.last_seen_at) < 15_000
                        });
                        runtime.native.pairing.retain(|_, request| {
                            now_millis().saturating_sub(request.created_at) < PAIRING_LIFETIME
                        });
                    }
                    let _ = emit_state_changed(&app, &task_registry, &task_paths).await;
                    None
                },
                scan_req = scan_rx.recv().fuse() => {
                    if scan_req.is_some() {
                        if let Ok(targets) = discovery::targets() {
                            broadcast_targets = targets;
                        }
                        if let Ok(packet) = discovery::packet("probe", &peer) {
                            for target in &broadcast_targets {
                                let _ = socket.send_to(&packet, target).await;
                            }
                        }
                    }
                    None
                },
                recv = socket.recv_from(&mut buffer).fuse() => {
                    Some(recv)
                }
            };
            if let Some(Ok((size, source))) = packet {
                if let Some((packet_kind, candidate, sent_at)) =
                    discovery::parse(&buffer[..size], source, &own_id)
                {
                    let (trusted, inserted) = {
                        let mut runtime = task_registry.inner.lock().await;
                        if runtime.native.config.ignored_peers.contains(&candidate.id)
                            || !runtime.native.replay.accept(
                                "discovery",
                                &buffer[..12],
                                sent_at,
                                now_millis(),
                            )
                        {
                            continue;
                        }
                        let trusted = runtime.native.authenticate(&candidate.id);
                        let inserted = trusted.is_none()
                            && (runtime.native.nearby.len() < 100
                                || runtime.native.nearby.contains_key(&candidate.id));
                        if inserted {
                            runtime
                                .native
                                .nearby
                                .insert(candidate.id.clone(), candidate.clone());
                        }
                        (trusted, inserted)
                    };
                    if packet_kind == DiscoveryPacketKind::Probe {
                        if let Ok(packet) = discovery::packet("announce", &peer) {
                            let _ = socket.send_to(&packet, source).await;
                        }
                    }
                    if inserted {
                        let _ = emit_state_changed(&app, &task_registry, &task_paths).await;
                    }
                    if let Some(trusted) = trusted {
                        schedule_peer_check(
                            &task_registry,
                            &task_paths,
                            Some(app.clone()),
                            candidate,
                            trusted,
                        )
                        .await;
                    }
                }
            }
        }
    });
    registry.inner.lock().await.native.handle = Some(handle);
    Ok(())
}

async fn schedule_peer_check(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    app: Option<tauri::AppHandle>,
    candidate: Peer,
    trusted: TrustedPeer,
) {
    let mut runtime = registry.inner.lock().await;
    if let Some((checking, task)) = runtime.native.discovery_tasks.get(&candidate.id) {
        if !task.inner().is_finished()
            && checking.ip == candidate.ip
            && checking.port == candidate.port
        {
            return;
        }
    }
    if let Some((_, task)) = runtime.native.discovery_tasks.remove(&candidate.id) {
        task.abort();
    }
    let registry = registry.clone();
    let paths = paths.clone();
    let auth = TransportAuth {
        sender: runtime.native.config.id.clone(),
        receiver: candidate.id.clone(),
        secret: trusted.secret.clone(),
        pairing_id: String::new(),
    };
    let checking = candidate.clone();
    // 地址认证与文件发送分别运行，长传输不能阻塞发现心跳。
    let task = tauri::async_runtime::spawn(async move {
        let Ok(client) = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
        else {
            return;
        };
        let Ok(response) =
            secure_call(&client, &candidate, &auth, "ping", json!({}), &[], None).await
        else {
            return;
        };
        let Ok(mut verified) = serde_json::from_value::<Peer>(response) else {
            return;
        };
        if verified.id != candidate.id || verified.name.len() > 128 {
            return;
        }
        verified.ip = candidate.ip;
        verified.port = candidate.port;
        verified.last_seen_at = now_millis();
        if refresh_peer(&registry, &paths, &verified, &trusted.secret)
            .await
            .is_err()
        {
            return;
        }
        let Some(app) = app else {
            return;
        };
        let _ = emit_state_changed(&app, &registry, &paths).await;
        let mut runtime = registry.inner.lock().await;
        if !runtime
            .native
            .authenticate(&verified.id)
            .map(|current| is_valid_token(&current.secret, &trusted.secret))
            .unwrap_or(false)
            || runtime
                .native
                .delivery_tasks
                .get(&verified.id)
                .map(|task| !task.inner().is_finished())
                .unwrap_or(false)
        {
            return;
        }
        let transfer_registry = registry.clone();
        let peer_id = verified.id.clone();
        let transfer = tauri::async_runtime::spawn(async move {
            deliver_pending_messages_for_peer(&app, &transfer_registry, &paths, &peer_id).await;
        });
        runtime.native.delivery_tasks.insert(verified.id, transfer);
    });
    runtime
        .native
        .discovery_tasks
        .insert(checking.id.clone(), (checking, task));
}

pub(super) async fn set_auto_discovery(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    enabled: bool,
) -> Result<Value, ManagerError> {
    let snapshot = {
        let mut runtime = registry.inner.lock().await;
        runtime.native.config.auto_discovery = enabled;
        runtime.native.config.discovery_configured = true;
        write_json(&paths.lan_share_files.config, &json!(runtime.native.config)).await?;
        runtime.native.snapshot()
    };
    Ok(snapshot)
}

pub(super) async fn set_transfer_settings(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    settings: transfer::Settings,
) -> Result<(), ManagerError> {
    settings.validate()?;
    let mut runtime = registry.inner.lock().await;
    let mut config = runtime.native.config.clone();
    if config.id.is_empty() {
        config = match tokio::fs::read(&paths.lan_share_files.config).await {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == ErrorKind::NotFound => Config::default(),
            Err(error) => return Err(error.into()),
        };
    }
    config.transfer = settings;
    write_json(&paths.lan_share_files.config, &json!(config)).await?;
    runtime.native.config = config;
    registry.transfer.configure(settings);
    Ok(())
}

pub(super) async fn scan_nearby(
    registry: &LanShareServerRegistry,
) -> Result<Value, ManagerError> {
    let scan_tx = {
        let runtime = registry.inner.lock().await;
        runtime.native.scan_trigger.clone()
    };
    if let Some(tx) = scan_tx {
        let _ = tx.send(()).await;
    }
    Ok(json!(true))
}

fn network_error(error: reqwest::Error) -> ManagerError {
    ManagerError::System(format!("设备连接失败：{}", error.without_url()))
}

async fn remember(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    trusted: TrustedPeer,
) -> Result<(), ManagerError> {
    upsert_device(
        registry,
        paths,
        &trusted.peer.id,
        &trusted.peer.name,
        "MonkeyThief/Desktop",
        &trusted.peer.ip,
    )
    .await?;
    let _storage = registry.storage.lock().await;
    ensure_session(
        paths,
        &trusted.peer.id,
        "",
        &trusted.peer.name,
        &trusted.peer.ip,
    )?;
    let mut runtime = registry.inner.lock().await;
    let mut config = runtime.native.config.clone();
    config
        .peers
        .retain(|current| current.peer.id != trusted.peer.id);
    config.ignored_peers.retain(|id| id != &trusted.peer.id);
    config.peers.push(trusted.clone());
    write_json(&paths.lan_share_files.config, &json!(config)).await?;
    runtime
        .native
        .nearby
        .insert(trusted.peer.id.clone(), trusted.peer.clone());
    runtime.native.config = config;
    Ok(())
}

async fn refresh_peer(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    peer: &Peer,
    secret: &str,
) -> Result<(), ManagerError> {
    let _storage = registry.storage.lock().await;
    let mut runtime = registry.inner.lock().await;
    let Some(trusted) = runtime.native.config.peers.iter_mut().find(|trusted| {
        trusted.peer.id == peer.id && trusted.secure && is_valid_token(&trusted.secret, secret)
    }) else {
        return Err(ManagerError::System("设备配对已撤销。".into()));
    };
    let address_changed = trusted.peer.ip != peer.ip || trusted.peer.port != peer.port;
    let changed = address_changed || trusted.peer.name != peer.name;
    trusted.peer = peer.clone();
    if address_changed {
        if let Some(task) = runtime.native.delivery_tasks.remove(&peer.id) {
            task.abort();
        }
    }
    if changed {
        write_json(&paths.lan_share_files.config, &json!(runtime.native.config)).await?;
        let mut devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices)?;
        if let Some(device) = devices.iter_mut().find(|device| device.id == peer.id) {
            device.ip = peer.ip.clone();
            device.last_seen_at = now_millis();
        }
        write_json(&paths.lan_share_files.devices, &json!(devices)).await?;
    }
    runtime.native.nearby.insert(peer.id.clone(), peer.clone());
    Ok(())
}

pub async fn connect_peer(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let peer_id = string_value(payload.get("peerId"));
    let address = string_value(payload.get("address"));
    let peer = if !address.is_empty() {
        let address = if address.contains("://") {
            address
        } else {
            format!("http://{address}")
        };
        let url = Url::parse(&address)
            .map_err(|_| ManagerError::System("连接地址格式不正确。".into()))?;
        Peer {
            ip: url.host_str().unwrap_or_default().into(),
            port: url.port().unwrap_or(DEFAULT_PORT),
            ..Peer::default()
        }
    } else {
        registry
            .inner
            .lock()
            .await
            .native
            .nearby
            .get(&peer_id)
            .cloned()
            .ok_or_else(|| {
                ManagerError::System("设备暂未发现，请检查对方是否已打开设备快传。".into())
            })?
    };
    peer_origin(&peer)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(network_error)?;
    let (identity, own_port) = {
        let runtime = registry.inner.lock().await;
        if runtime.handle.is_none() {
            return Err(ManagerError::System("请先上线设备快传。".into()));
        }
        (runtime.native.config.clone(), runtime.port)
    };
    let mut auth = TransportAuth {
        sender: identity.id.clone(),
        receiver: peer.id.clone(),
        secret: String::new(),
        pairing_id: String::new(),
    };
    let info = secure_call(&client, &peer, &auth, "info", json!({}), &[], None).await?;
    let endpoint = peer;
    let mut peer: Peer = serde_json::from_value(info)?;
    peer.ip = endpoint.ip;
    peer.port = endpoint.port;
    peer.last_seen_at = now_millis();
    {
        let runtime = registry.inner.lock().await;
        if runtime.handle.is_none() {
            return Err(ManagerError::System("请先上线设备快传。".into()));
        }
        if runtime.native.config.id == peer.id {
            return Err(ManagerError::System("不能连接当前设备自身。".into()));
        }
        if runtime.native.connecting.contains_key(&peer.id) {
            return Ok(lan_share_response(json!(true)));
        }
    }
    if !is_native_id(&peer.id) || peer.id.len() > 80 {
        return Err(ManagerError::System(
            "对方不是兼容的设备快传客户端。".into(),
        ));
    }
    let (private_key, public_key) = transport::keypair()?;
    let request_id = create_id("pair");
    auth.receiver = peer.id.clone();
    let response = secure_call(&client, &peer, &auth, "pair", json!({
        "requestId": request_id, "publicKey": public_key, "peer": { "id": identity.id, "name": identity.name, "ip": "", "port": own_port }
    }), &[], None).await?;
    let receiver_key = string_value(response.get("publicKey"));
    let secret = transport::pair_secret(
        private_key,
        &receiver_key,
        &identity.id,
        &peer.id,
        &request_id,
        &public_key,
        &receiver_key,
    )?;
    auth.secret = secret.clone();
    auth.pairing_id = request_id.clone();
    {
        let mut runtime = registry.inner.lock().await;
        runtime.native.error.clear();
        runtime.native.nearby.insert(peer.id.clone(), peer.clone());
        runtime
            .native
            .connecting
            .insert(peer.id.clone(), pairing_code(&secret));
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
                let status = secure_call(
                    &client,
                    &peer,
                    &auth,
                    "pair-status",
                    json!({ "requestId": request_id }),
                    &[],
                    None,
                )
                .await?;
                match status["status"].as_str() {
                    Some("accepted") => {
                        return remember(
                            &task_registry,
                            &task_paths,
                            TrustedPeer {
                                peer: peer.clone(),
                                secret: secret.clone(),
                                secure: true,
                            },
                        )
                        .await
                    }
                    Some("rejected") => {
                        return Err(ManagerError::System("对方拒绝了连接请求。".into()))
                    }
                    _ => {}
                }
            }
            Err(ManagerError::System("连接确认已过期，请重新连接。".into()))
        }
        .await;
        {
            let mut runtime = task_registry.inner.lock().await;
            runtime.native.connecting.remove(&peer.id);
            if let Err(error) = result {
                runtime.native.error = error.to_string();
            }
        }
        let _ = emit_state_changed(&app, &task_registry, &task_paths).await;
    });
    {
        let mut runtime = registry.inner.lock().await;
        if runtime.native.connecting.get(&task_peer_id) == Some(&task_code) {
            if let Some(previous) = runtime.native.tasks.insert(task_peer_id, handle) {
                previous.abort();
            }
        } else {
            handle.abort();
        }
    }
    get_state(registry, paths).await
}

pub async fn respond_pairing(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let identifier = string_value(payload.get("requestId"));
    let accepted = payload
        .get("accept")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let trusted = {
        let runtime = registry.inner.lock().await;
        let request = runtime
            .native
            .pairing
            .get(&identifier)
            .ok_or_else(|| ManagerError::System("连接请求已过期。".into()))?;
        if now_millis().saturating_sub(request.created_at) >= PAIRING_LIFETIME
            || request.status != "pending"
        {
            return Err(ManagerError::System("连接请求已过期或已处理。".into()));
        }
        TrustedPeer {
            peer: request.peer.clone(),
            secret: request.secret.clone(),
            secure: true,
        }
    };
    if accepted {
        remember(registry, paths, trusted).await?;
    }
    if let Some(request) = registry
        .inner
        .lock()
        .await
        .native
        .pairing
        .get_mut(&identifier)
    {
        request.status = if accepted { "accepted" } else { "rejected" }.into();
    }
    emit_state_changed(&app, registry, paths).await?;
    get_state(registry, paths).await
}

async fn peer_session(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    peer: &Peer,
    requested: &str,
) -> Result<LanShareSession, ManagerError> {
    let _storage = registry.storage.lock().await;
    if !requested.is_empty() {
        return read_array::<LanShareSession>(&paths.lan_share_files.sessions)?
            .into_iter()
            .find(|session| {
                session.id == requested && session.device_id == peer.id && session.mode != "group"
            })
            .ok_or_else(|| ManagerError::System("会话不属于当前设备。".into()));
    }
    ensure_session(paths, &peer.id, "", &peer.name, &peer.ip)
}

async fn process_secure_operation(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    app: Option<&tauri::AppHandle>,
    peer: &Peer,
    operation: &str,
    payload: Value,
    bytes: Bytes,
) -> Result<Value, ManagerError> {
    match operation {
        "info" | "ping" => {
            let runtime = registry.inner.lock().await;
            Ok(json!(Peer {
                id: runtime.native.config.id.clone(),
                name: runtime.native.config.name.clone(),
                ip: runtime.lan_ip.clone(),
                port: runtime.port,
                last_seen_at: now_millis()
            }))
        }
        "pair" => {
            let identifier = string_value(payload.get("requestId"));
            let initiator_key = string_value(payload.get("publicKey"));
            let mut offered: Peer = serde_json::from_value(payload["peer"].clone())?;
            offered.ip = peer.ip.clone();
            offered.last_seen_at = now_millis();
            peer_origin(&offered)?;
            if offered.id != peer.id
                || offered.name.len() > 128
                || identifier.is_empty()
                || identifier.len() > 80
                || initiator_key.len() != 43
            {
                return Err(ManagerError::System("连接请求无效。".into()));
            }
            let (private_key, public_key) = transport::keypair()?;
            let own_id = registry.inner.lock().await.native.config.id.clone();
            let secret = transport::pair_secret(
                private_key,
                &initiator_key,
                &offered.id,
                &own_id,
                &identifier,
                &initiator_key,
                &public_key,
            )?;
            {
                let mut runtime = registry.inner.lock().await;
                runtime.native.pairing.retain(|_, pairing| {
                    now_millis().saturating_sub(pairing.created_at) < PAIRING_LIFETIME
                });
                if runtime.native.pairing.len() >= 20
                    || runtime.native.pairing.contains_key(&identifier)
                {
                    return Err(ManagerError::System(
                        "连接请求过多或已存在，请稍后重试。".into(),
                    ));
                }
                runtime.native.pairing.insert(
                    identifier,
                    Pairing {
                        peer: offered,
                        secret,
                        created_at: now_millis(),
                        status: "pending".into(),
                    },
                );
            }
            if let Some(app) = app {
                emit_state_changed(app, registry, paths).await?;
            }
            Ok(json!({ "publicKey": public_key }))
        }
        "pair-status" => {
            let identifier = string_value(payload.get("requestId"));
            let runtime = registry.inner.lock().await;
            let pairing = runtime
                .native
                .pairing
                .get(&identifier)
                .filter(|pairing| {
                    pairing.peer.id == peer.id
                        && now_millis().saturating_sub(pairing.created_at) < PAIRING_LIFETIME
                })
                .ok_or_else(|| ManagerError::System("连接确认已过期。".into()))?;
            Ok(json!({ "status": pairing.status }))
        }
        "message-exists" => {
            let message_id = format!("{}-{}", peer.id, string_value(payload.get("messageId")));
            let _storage = registry.storage.lock().await;
            let exists = read_array::<LanShareMessage>(&paths.lan_share_files.messages)?
                .iter()
                .any(|message| message.id == message_id && message.device_id == peer.id);
            Ok(json!({ "exists": exists }))
        }
        "file-chunk" => {
            let session = peer_session(
                registry,
                paths,
                peer,
                &string_value(payload.get("sessionId")),
            )
            .await?;
            let number = |key: &str| {
                payload[key]
                    .as_u64()
                    .ok_or_else(|| ManagerError::System(format!("分块参数 {key} 无效。")))
            };
            let upload_id = string_value(payload.get("uploadId"));
            let chunk_index = number("chunkIndex")?;
            let file = attachments::upload_chunk(
                Full::new(bytes),
                registry,
                paths,
                &session.id,
                &upload_id,
                &string_value(payload.get("name")),
                chunk_index,
                number("chunkTotal")?,
                number("offset")?,
                number("totalSize")?,
                number("contentLength")?,
                &string_value(payload.get("contentRange")),
                &string_value(payload.get("sha256")),
            )
            .await?;
            Ok(json!({
                "complete": file.is_some(), "file": file.as_ref().map(attachments::Attachment::from),
                "sessionId": session.id, "uploadId": upload_id, "chunkIndex": chunk_index
            }))
        }
        "message" => {
            let source_id = string_value(payload.get("messageId"));
            if source_id.is_empty()
                || source_id.len() > 100
                || !source_id.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                })
            {
                return Err(ManagerError::System("消息标识无效。".into()));
            }
            let session = peer_session(
                registry,
                paths,
                peer,
                &string_value(payload.get("sessionId")),
            )
            .await?;
            let files = attachments::prepare(
                registry,
                paths,
                &session.id,
                &json!({ "attachmentIds": payload["attachmentIds"] }),
            )
            .await?;
            let stored = append_message_with_attachments(
                registry,
                paths,
                &peer.id,
                &session.id,
                "mobile-to-desktop",
                &string_value(payload.get("content")),
                true,
                &files,
                Some(&format!("{}-{source_id}", peer.id)),
            )
            .await?;
            attachments::publish(registry, paths, &files).await?;
            if let Some(app) = app {
                emit_message_created(app, &stored);
                emit_state_changed(app, registry, paths).await?;
            }
            Ok(json!({ "messageId": stored.id }))
        }
        _ => Err(ManagerError::System("不支持的设备快传操作。".into())),
    }
}

pub(super) async fn handle_request(
    request: Request<Incoming>,
    context: &HttpContext,
    address: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    handle_secure_request(
        request,
        &context.registry,
        &context.paths,
        Some(&context.app),
        address,
    )
    .await
}

async fn handle_secure_request<B>(
    request: Request<B>,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    app: Option<&tauri::AppHandle>,
    address: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError>
where
    B: hyper::body::Body<Data = Bytes> + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // 禁止旧明文接口回退，避免配对密钥和附件再次暴露。
    if request.method() != Method::POST || request.uri().path() != "/api/native/secure" {
        return Ok(api_error(
            StatusCode::UPGRADE_REQUIRED,
            "设备快传已启用加密，请更新双方软件并重新配对。",
        ));
    }
    let header = |key: &str| {
        request
            .headers()
            .get(key)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    if header("x-lan-version") != transport::VERSION {
        return Ok(api_error(
            StatusCode::UPGRADE_REQUIRED,
            "设备快传加密版本不匹配。",
        ));
    }
    let sender = header("x-lan-sender");
    let receiver = header("x-lan-receiver");
    let scope = header("x-lan-scope");
    let pairing_id = header("x-lan-pairing");
    if !is_native_id(&sender)
        || sender.len() > 80
        || !sender.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Ok(api_error(StatusCode::UNAUTHORIZED, "设备身份无效。"));
    }
    let (mut peer, secret) = {
        let runtime = registry.inner.lock().await;
        if (!receiver.is_empty() && receiver != runtime.native.config.id)
            || sender == runtime.native.config.id
        {
            return Ok(api_error(StatusCode::UNAUTHORIZED, "接收设备不匹配。"));
        }
        match scope.as_str() {
            "software" if pairing_id.is_empty() => (
                Peer {
                    id: sender.clone(),
                    ip: client_ip(address),
                    ..Peer::default()
                },
                String::new(),
            ),
            "paired" if pairing_id.is_empty() => {
                let Some(trusted) = runtime.native.authenticate(&sender) else {
                    return Ok(api_error(StatusCode::UNAUTHORIZED, "设备尚未安全配对。"));
                };
                (trusted.peer, trusted.secret)
            }
            "pairing" => {
                let Some(pairing) = runtime.native.pairing.get(&pairing_id).filter(|pairing| {
                    pairing.peer.id == sender
                        && pairing.peer.ip == client_ip(address)
                        && now_millis().saturating_sub(pairing.created_at) < PAIRING_LIFETIME
                }) else {
                    return Ok(api_error(
                        StatusCode::UNAUTHORIZED,
                        "连接确认无效或已过期。",
                    ));
                };
                (pairing.peer.clone(), pairing.secret.clone())
            }
            _ => return Ok(api_error(StatusCode::UNAUTHORIZED, "设备密钥类型无效。")),
        }
    };
    let auth = TransportAuth {
        sender: sender.clone(),
        receiver,
        secret,
        pairing_id: pairing_id.clone(),
    };
    let aad = auth.aad();
    let ciphertext = match Limited::new(request.into_body(), transport::MAX_BODY)
        .collect()
        .await
    {
        Ok(body) => body.to_bytes(),
        Err(_) => {
            return Ok(api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "加密载荷过大或接收中断。",
            ))
        }
    };
    let plaintext = match transport::decrypt(&auth.secret, &aad, &ciphertext) {
        Ok(plaintext) => plaintext,
        Err(_) => {
            return Ok(api_error(
                StatusCode::UNAUTHORIZED,
                "密钥不匹配或内容已被篡改。",
            ))
        }
    };
    // 通过认证后的操作结果和错误都加密返回。
    let result = async {
        let (metadata, bytes) = transport::unpack(plaintext)?;
        let operation = string_value(metadata.get("operation"));
        let permitted = match scope.as_str() {
            "software" => matches!(operation.as_str(), "info" | "pair"),
            "pairing" => {
                operation == "pair-status" && metadata["payload"]["requestId"] == pairing_id
            }
            "paired" => matches!(
                operation.as_str(),
                "ping" | "message-exists" | "file-chunk" | "message"
            ),
            _ => false,
        } && !(auth.receiver.is_empty() && operation != "info");
        if !permitted {
            return Err(ManagerError::System("当前密钥无权执行此操作。".into()));
        }
        if !registry.inner.lock().await.native.replay.accept(
            &format!("{sender}|{scope}|{pairing_id}"),
            &ciphertext[..12],
            metadata["sentAt"].as_u64().unwrap_or(0),
            now_millis(),
        ) {
            return Err(ManagerError::System(
                "加密请求已重复或过期，请检查双方时间。".into(),
            ));
        }
        peer.ip = client_ip(address);
        if scope == "paired" {
            peer.last_seen_at = now_millis();
            peer_origin(&peer)?;
            refresh_peer(registry, paths, &peer, &auth.secret).await?;
        }
        process_secure_operation(
            registry,
            paths,
            app,
            &peer,
            &operation,
            metadata["payload"].clone(),
            bytes,
        )
        .await
    }
    .await;
    let (status, payload) = match result {
        Ok(data) => (StatusCode::OK, lan_share_response(data)),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            json!({ "status": "error", "data": null, "message": error.to_string() }),
        ),
    };
    let request_nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&ciphertext[..12]);
    let response = transport::encrypt(
        &auth.secret,
        &format!("{aad}|response|{request_nonce}"),
        &serde_json::to_vec(&payload)?,
    )?;
    Ok(Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header("x-lan-version", transport::VERSION)
        .body(boxed_full(response))
        .unwrap())
}

pub(super) async fn deliver(
    registry: &LanShareServerRegistry,
    device_id: &str,
    content: &str,
    files: &[LanShareFile],
    message_id: &str,
) -> Result<bool, ManagerError> {
    let (trusted, own_id) = {
        let runtime = registry.inner.lock().await;
        (
            runtime.native.authenticate(device_id),
            runtime.native.config.id.clone(),
        )
    };
    let Some(trusted) = trusted else {
        return Ok(false);
    };
    let auth = TransportAuth {
        sender: own_id,
        receiver: trusted.peer.id.clone(),
        secret: trusted.secret.clone(),
        pairing_id: String::new(),
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(network_error)?;
    let existing = secure_call(
        &client,
        &trusted.peer,
        &auth,
        "message-exists",
        json!({ "messageId": message_id }),
        &[],
        None,
    )
    .await?;
    if existing["exists"] == true {
        return Ok(true);
    }
    let mut remote_session = String::new();
    let mut identifiers = Vec::new();
    for file in files {
        let _permit = registry.transfer.acquire_file().await;
        let mut source = File::open(&file.path).await?;
        let metadata = source.metadata().await?;
        if !metadata.is_file() || metadata.len() != file.size {
            return Err(ManagerError::System(format!(
                "附件已改变或不是文件：{}",
                file.name
            )));
        }
        let chunk_total = file.size.div_ceil(attachments::FILE_CHUNK_BYTES).max(1);
        let upload_id = create_id("upload");
        for chunk_index in 0..chunk_total {
            let offset = chunk_index * attachments::FILE_CHUNK_BYTES;
            let chunk_length = file
                .size
                .saturating_sub(offset)
                .min(attachments::FILE_CHUNK_BYTES);
            let mut chunk = vec![0u8; chunk_length as usize];
            source.read_exact(&mut chunk).await?;
            let chunk_sha256 = format!("{:x}", Sha256::digest(&chunk));
            let content_range = if file.size == 0 {
                "bytes 0-0/0".to_string()
            } else {
                format!(
                    "bytes {}-{}/{}",
                    offset,
                    offset + chunk_length - 1,
                    file.size
                )
            };
            let uploaded = secure_call(
                &client,
                &trusted.peer,
                &auth,
                "file-chunk",
                json!({
                    "name": file.name, "sessionId": remote_session, "uploadId": upload_id,
                    "chunkIndex": chunk_index, "chunkTotal": chunk_total, "offset": offset,
                    "totalSize": file.size, "sha256": chunk_sha256,
                    "contentLength": chunk_length, "contentRange": content_range,
                }),
                &chunk,
                Some(&registry.transfer),
            )
            .await?;
            let complete = chunk_index + 1 == chunk_total;
            if uploaded["uploadId"] != upload_id
                || uploaded["chunkIndex"].as_u64() != Some(chunk_index)
                || uploaded["complete"].as_bool() != Some(complete)
            {
                return Err(ManagerError::System("对方返回的分块确认不一致。".into()));
            }
            let session = string_value(uploaded.get("sessionId"));
            if session.is_empty() || (!remote_session.is_empty() && session != remote_session) {
                return Err(ManagerError::System("对方返回的会话不一致。".into()));
            }
            remote_session = session;
            if complete {
                let identifier = string_value(uploaded["file"].get("id"));
                if identifier.is_empty() {
                    return Err(ManagerError::System("对方未返回完整附件。".into()));
                }
                identifiers.push(identifier);
            }
        }
    }
    secure_call(&client, &trusted.peer, &auth, "message", json!({
        "messageId": message_id, "sessionId": remote_session, "content": content, "attachmentIds": identifiers
    }), &[], None).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encrypted_test_packet(auth: &TransportAuth, operation: &str, payload: Value) -> Vec<u8> {
        let packet = transport::pack(
            &json!({
                "operation": operation, "payload": payload, "sentAt": now_millis()
            }),
            &[],
        )
        .unwrap();
        transport::encrypt(&auth.secret, &auth.aad(), &packet).unwrap()
    }

    fn encrypted_test_request(auth: &TransportAuth, packet: &[u8]) -> Request<Full<Bytes>> {
        Request::builder()
            .method(Method::POST)
            .uri("/api/native/secure")
            .header("x-lan-version", transport::VERSION)
            .header("x-lan-sender", &auth.sender)
            .header("x-lan-receiver", &auth.receiver)
            .header("x-lan-scope", auth.scope())
            .header("x-lan-pairing", &auth.pairing_id)
            .body(Full::new(Bytes::copy_from_slice(packet)))
            .unwrap()
    }

    async fn decoded_test_response(
        response: Response<BoxBody>,
        auth: &TransportAuth,
        packet: &[u8],
    ) -> (StatusCode, Value) {
        let status = response.status();
        assert_eq!(response.headers()["x-lan-version"], transport::VERSION);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&packet[..12]);
        let decrypted = transport::decrypt(
            &auth.secret,
            &format!("{}|response|{nonce}", auth.aad()),
            &body,
        )
        .unwrap();
        (status, serde_json::from_slice(&decrypted).unwrap())
    }

    #[test]
    fn secure_pairing_negotiates_keys_and_rejects_wrong_keys_plaintext_and_replays() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-handshake-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            {
                let mut runtime = registry.inner.lock().await;
                runtime.native.config.id = "native-receiver".into();
                runtime.native.config.name = "接收方".into();
                runtime.port = DEFAULT_PORT;
            }
            let address = "127.0.0.1:12345".parse().unwrap();
            let software = TransportAuth {
                sender: "native-sender".into(),
                receiver: "native-receiver".into(),
                secret: String::new(),
                pairing_id: String::new(),
            };
            let (private, public) = transport::keypair().unwrap();
            let packet = encrypted_test_packet(
                &software,
                "pair",
                json!({
                    "requestId": "pair-handshake", "publicKey": public,
                    "peer": { "id": "native-sender", "name": "发送方", "ip": "8.8.8.8", "port": DEFAULT_PORT }
                }),
            );
            let response = handle_secure_request(
                encrypted_test_request(&software, &packet),
                &registry,
                &paths,
                None,
                address,
            )
            .await
            .unwrap();
            let (status, response) = decoded_test_response(response, &software, &packet).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(response["data"].as_object().unwrap().len(), 1);
            assert!(response["data"].get("secret").is_none());
            let remote_public = response["data"]["publicKey"].as_str().unwrap();
            let secret = transport::pair_secret(
                private,
                remote_public,
                "native-sender",
                "native-receiver",
                "pair-handshake",
                &public,
                remote_public,
            )
            .unwrap();
            let trusted = {
                let runtime = registry.inner.lock().await;
                let pairing = &runtime.native.pairing["pair-handshake"];
                assert_eq!(pairing.secret, secret);
                assert_eq!(pairing.peer.ip, "127.0.0.1");
                assert!(runtime.native.authenticate("native-sender").is_none());
                TrustedPeer {
                    peer: pairing.peer.clone(),
                    secret: pairing.secret.clone(),
                    secure: true,
                }
            };
            assert!(!response.to_string().contains(&secret));
            let mut paired = TransportAuth {
                secret: secret.clone(),
                pairing_id: "pair-handshake".into(),
                ..software.clone()
            };
            for accepted in [false, true] {
                if accepted {
                    remember(&registry, &paths, trusted.clone()).await.unwrap();
                    registry
                        .inner
                        .lock()
                        .await
                        .native
                        .pairing
                        .get_mut("pair-handshake")
                        .unwrap()
                        .status = "accepted".into();
                }
                let packet = encrypted_test_packet(
                    &paired,
                    "pair-status",
                    json!({ "requestId": "pair-handshake" }),
                );
                let response = handle_secure_request(
                    encrypted_test_request(&paired, &packet),
                    &registry,
                    &paths,
                    None,
                    address,
                )
                .await
                .unwrap();
                let (status, response) = decoded_test_response(response, &paired, &packet).await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(
                    response["data"]["status"],
                    if accepted { "accepted" } else { "pending" }
                );
            }
            paired.pairing_id.clear();
            let unauthorized = TransportAuth {
                secret: "wrong-secret".into(),
                ..paired.clone()
            };
            let packet = encrypted_test_packet(&unauthorized, "ping", json!({}));
            let response = handle_secure_request(
                encrypted_test_request(&unauthorized, &packet),
                &registry,
                &paths,
                None,
                address,
            )
            .await
            .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

            let packet = encrypted_test_packet(
                &software,
                "message",
                json!({ "messageId": "attempt", "content": "无权写入" }),
            );
            let response = handle_secure_request(
                encrypted_test_request(&software, &packet),
                &registry,
                &paths,
                None,
                address,
            )
            .await
            .unwrap();
            assert_eq!(
                decoded_test_response(response, &software, &packet).await.0,
                StatusCode::BAD_REQUEST
            );
            assert!(
                read_array::<LanShareMessage>(&paths.lan_share_files.messages)
                    .unwrap()
                    .is_empty()
            );

            let packet = encrypted_test_packet(&paired, "ping", json!({}));
            for expected in [StatusCode::OK, StatusCode::BAD_REQUEST] {
                let response = handle_secure_request(
                    encrypted_test_request(&paired, &packet),
                    &registry,
                    &paths,
                    None,
                    address,
                )
                .await
                .unwrap();
                let (status, response) = decoded_test_response(response, &paired, &packet).await;
                assert_eq!(status, expected);
                if expected == StatusCode::BAD_REQUEST {
                    assert!(response["message"].as_str().unwrap().contains("重复"));
                }
            }
            let mut tampered = encrypted_test_packet(&paired, "ping", json!({}));
            tampered[30] ^= 1;
            let response = handle_secure_request(
                encrypted_test_request(&paired, &tampered),
                &registry,
                &paths,
                None,
                address,
            )
            .await
            .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            for path in [
                "/api/native/info",
                "/api/native/pair",
                "/api/native/upload",
                "/api/native/message",
            ] {
                let request = Request::builder()
                    .method(Method::POST)
                    .uri(path)
                    .header("authorization", format!("Bearer {secret}"))
                    .body(Full::new(Bytes::from_static(b"plaintext")))
                    .unwrap();
                assert_eq!(
                    handle_secure_request(request, &registry, &paths, None, address)
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::UPGRADE_REQUIRED
                );
            }
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn verified_address_changes_are_saved_and_revoked_pairings_cannot_return() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-address-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-local".into();
            let original = Peer {
                id: "native-remote".into(),
                name: "旧名称".into(),
                ip: "192.168.1.2".into(),
                port: DEFAULT_PORT,
                last_seen_at: now_millis(),
            };
            remember(
                &registry,
                &paths,
                TrustedPeer {
                    peer: original.clone(),
                    secret: "paired-secret".into(),
                    secure: true,
                },
            )
            .await
            .unwrap();
            let changed = Peer {
                ip: "192.168.2.3".into(),
                port: DEFAULT_PORT + 1,
                name: "新名称".into(),
                ..original.clone()
            };
            assert!(refresh_peer(&registry, &paths, &changed, "wrong-secret")
                .await
                .is_err());
            assert_eq!(
                registry.inner.lock().await.native.config.peers[0].peer.ip,
                original.ip
            );
            registry.inner.lock().await.native.delivery_tasks.insert(
                original.id.clone(),
                tauri::async_runtime::spawn(std::future::pending::<()>()),
            );
            refresh_peer(&registry, &paths, &changed, "paired-secret")
                .await
                .unwrap();
            let saved: Config = serde_json::from_slice(
                &tokio::fs::read(&paths.lan_share_files.config)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(saved.peers[0].peer.ip, changed.ip);
            assert_eq!(saved.peers[0].peer.port, changed.port);
            let devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices).unwrap();
            assert_eq!(devices[0].ip, changed.ip);
            let auth = TransportAuth {
                sender: original.id.clone(),
                receiver: "native-local".into(),
                secret: "paired-secret".into(),
                pairing_id: String::new(),
            };
            let packet = encrypted_test_packet(&auth, "ping", json!({}));
            let response = handle_secure_request(
                encrypted_test_request(&auth, &packet),
                &registry,
                &paths,
                None,
                "192.168.2.4:12345".parse().unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(
                decoded_test_response(response, &auth, &packet).await.0,
                StatusCode::OK
            );
            assert_eq!(
                registry.inner.lock().await.native.config.peers[0].peer.ip,
                "192.168.2.4"
            );
            let response = handle_secure_request(
                encrypted_test_request(&auth, &packet),
                &registry,
                &paths,
                None,
                "192.168.2.5:12345".parse().unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(
                decoded_test_response(response, &auth, &packet).await.0,
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                registry.inner.lock().await.native.config.peers[0].peer.ip,
                "192.168.2.4"
            );
            {
                let mut runtime = registry.inner.lock().await;
                assert!(runtime.native.online(&original.id));
                assert!(runtime.native.delivery_tasks.is_empty());
                assert_eq!(runtime.native.snapshot()["peers"][0]["paired"], true);
                runtime.native.forget(&paths, &original.id).await.unwrap();
            }
            assert!(refresh_peer(&registry, &paths, &original, "paired-secret")
                .await
                .is_err());
            assert!(registry.inner.lock().await.native.config.peers.is_empty());
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn discovery_heartbeats_continue_while_delivery_is_pending() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-heartbeat-test"));
            let paths = crate::core::paths::resolve_app_paths(&root.join("local"));
            let remote_paths = crate::core::paths::resolve_app_paths(&root.join("remote"));
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-local".into();
            let peer = Peer {
                id: "native-remote".into(),
                name: "对方".into(),
                ip: "127.0.0.1".into(),
                port,
                last_seen_at: now_millis(),
            };
            let trusted = TrustedPeer {
                peer: peer.clone(),
                secret: "paired-secret".into(),
                secure: true,
            };
            remember(&registry, &paths, trusted.clone()).await.unwrap();
            {
                let mut runtime = registry.inner.lock().await;
                runtime.native.config.auto_discovery = false;
                runtime.native.delivery_tasks.insert(
                    peer.id.clone(),
                    tauri::async_runtime::spawn(std::future::pending::<()>()),
                );
                let stale = Peer {
                    ip: "192.168.1.99".into(),
                    ..peer.clone()
                };
                runtime.native.discovery_tasks.insert(
                    peer.id.clone(),
                    (
                        stale,
                        tauri::async_runtime::spawn(std::future::pending::<()>()),
                    ),
                );
            }
            let remote = LanShareServerRegistry::new();
            {
                let mut runtime = remote.inner.lock().await;
                runtime.native.config.id = peer.id.clone();
                runtime.native.config.name = peer.name.clone();
                runtime.port = port;
                runtime.native.config.peers.push(TrustedPeer {
                    peer: Peer {
                        id: "native-local".into(),
                        ip: "127.0.0.1".into(),
                        port: DEFAULT_PORT,
                        ..Peer::default()
                    },
                    secret: "paired-secret".into(),
                    secure: true,
                });
            }
            let server = tauri::async_runtime::spawn(async move {
                for _ in 0..2 {
                    let (socket, address) = listener.accept().await.unwrap();
                    let remote = remote.clone();
                    let paths = remote_paths.clone();
                    let service = service_fn(move |request: Request<Incoming>| {
                        let remote = remote.clone();
                        let paths = paths.clone();
                        async move {
                            Ok::<_, Infallible>(
                                handle_secure_request(request, &remote, &paths, None, address)
                                    .await
                                    .unwrap(),
                            )
                        }
                    });
                    http1::Builder::new()
                        .serve_connection(TokioIo::new(socket), service)
                        .await
                        .unwrap();
                }
            });
            for _ in 0..2 {
                registry
                    .inner
                    .lock()
                    .await
                    .native
                    .nearby
                    .get_mut(&peer.id)
                    .unwrap()
                    .last_seen_at = 0;
                schedule_peer_check(&registry, &paths, None, peer.clone(), trusted.clone()).await;
                let (_, task) = registry
                    .inner
                    .lock()
                    .await
                    .native
                    .discovery_tasks
                    .remove(&peer.id)
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(5), task)
                    .await
                    .unwrap()
                    .unwrap();
                let runtime = registry.inner.lock().await;
                assert!(runtime.native.online(&peer.id));
                assert!(!runtime.native.delivery_tasks[&peer.id]
                    .inner()
                    .is_finished());
            }
            tokio::time::timeout(Duration::from_secs(5), server)
                .await
                .unwrap()
                .unwrap();
            registry.inner.lock().await.native.stop();
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn native_ids_accept_current_and_legacy_formats() {
        assert!(is_native_id("native-1789038628947-12096-997"));
        assert!(is_native_id("native_1789038628947_12096_997"));
        assert!(!is_native_id("desktop_1789038628947"));
        assert!(create_native_id().starts_with("native-"));
        let mut legacy_id = "native_1789038628947_12096_997".to_string();
        normalize_native_id(&mut legacy_id);
        assert!(legacy_id.starts_with("native-"));
    }

    #[test]
    fn deleting_native_device_revokes_pairing_and_hides_future_discovery() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-delete-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let removed = Peer {
                id: "native-removed".into(),
                name: "移除设备".into(),
                ip: "192.168.1.5".into(),
                port: DEFAULT_PORT,
                last_seen_at: now_millis(),
            };
            let kept = Peer {
                id: "native-kept".into(),
                name: "保留设备".into(),
                ip: "192.168.1.6".into(),
                port: DEFAULT_PORT,
                last_seen_at: now_millis(),
            };
            let config = Config {
                id: "native-local".into(),
                name: "本机".into(),
                peers: vec![
                    TrustedPeer {
                        peer: removed.clone(),
                        secret: "removed-secret".into(),
                        secure: true,
                    },
                    TrustedPeer {
                        peer: kept.clone(),
                        secret: "kept-secret".into(),
                        secure: true,
                    },
                ],
                ..Config::default()
            };
            write_json(&paths.lan_share_files.config, &json!(config))
                .await
                .unwrap();
            let mut native = NativeRuntime::default();
            native.nearby.insert(removed.id.clone(), removed.clone());
            native
                .connecting
                .insert(removed.id.clone(), "123456".into());
            native.pairing.insert(
                "pending-request".into(),
                Pairing {
                    peer: removed.clone(),
                    secret: "pending-secret".into(),
                    created_at: now_millis(),
                    status: "pending".into(),
                },
            );
            native.tasks.insert(
                removed.id.clone(),
                tauri::async_runtime::spawn(std::future::pending::<()>()),
            );
            native.forget(&paths, &removed.id).await.unwrap();
            assert!(native.authenticate(&removed.id).is_none());
            assert!(native.authenticate(&kept.id).is_some());
            assert!(!native.tasks.contains_key(&removed.id));
            assert!(!native.connecting.contains_key(&removed.id));
            assert!(native.pairing.is_empty());
            let saved: Config = serde_json::from_slice(
                &tokio::fs::read(&paths.lan_share_files.config)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(saved.id, "native-local");
            assert_eq!(saved.peers.len(), 1);
            native.config = saved;
            native.nearby.insert(removed.id.clone(), removed.clone());
            assert!(native.snapshot()["peers"]
                .as_array()
                .unwrap()
                .iter()
                .all(|peer| peer["id"] != removed.id));
            assert!(native.forget(&paths, "native-local").await.is_err());
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native = native;
            remember(
                &registry,
                &paths,
                TrustedPeer {
                    peer: removed.clone(),
                    secret: "new-approved-secret".into(),
                    secure: true,
                },
            )
            .await
            .unwrap();
            let snapshot = registry.inner.lock().await.native.snapshot();
            assert!(snapshot["peers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|peer| peer["id"] == removed.id && peer["paired"] == true));
            assert!(!registry
                .inner
                .lock()
                .await
                .native
                .config
                .ignored_peers
                .contains(&removed.id));
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn paired_credentials_require_safe_pairing_and_stable_device_identity() {
        let mut runtime = NativeRuntime::default();
        runtime.config.peers.push(TrustedPeer {
            peer: Peer {
                id: "native-approved".into(),
                ip: "192.168.1.5".into(),
                port: DEFAULT_PORT,
                ..Peer::default()
            },
            secret: "approved-secret".into(),
            secure: true,
        });
        assert!(runtime.authenticate("native-approved").is_some());
        assert!(runtime.authenticate("native-other").is_none());
        runtime.config.peers[0].secure = false;
        assert!(runtime.authenticate("native-approved").is_none());
        assert_eq!(runtime.snapshot()["peers"][0]["paired"], false);
        let legacy: TrustedPeer = serde_json::from_value(json!({
            "peer": { "id": "native-legacy", "name": "旧设备", "ip": "192.168.1.6", "port": DEFAULT_PORT }, "secret": "leaked-old-secret"
        })).unwrap();
        assert!(!legacy.secure);
    }

    #[test]
    fn native_and_browser_devices_on_same_address_keep_separate_histories() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-identity-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            upsert_device(
                &registry,
                &paths,
                "native-computer",
                "电脑",
                "MonkeyThief/Desktop",
                "192.168.1.6",
            )
            .await
            .unwrap();
            upsert_device(&registry, &paths, "", "网页", "browser", "192.168.1.6")
                .await
                .unwrap();
            let devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices).unwrap();
            assert_eq!(devices.len(), 2);
            assert!(devices.iter().any(|device| device.id == "native-computer"));
            assert!(devices
                .iter()
                .any(|device| device.id == device_id_from_ip("192.168.1.6")));
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn native_batch_encrypts_chunks_reassembles_and_retry_is_idempotent() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("native-secure-transfer-test"));
            let paths = crate::core::paths::resolve_app_paths(&root.join("receiver"));
            tokio::fs::create_dir_all(&root).await.unwrap();
            let first_content = vec![b'a'; attachments::FILE_CHUNK_BYTES as usize + 3];
            let first_path = root.join("first.txt");
            let second_path = root.join("second.png");
            let empty_path = root.join("empty.txt");
            tokio::fs::write(&first_path, &first_content).await.unwrap();
            tokio::fs::write(&second_path, b"second-file")
                .await
                .unwrap();
            tokio::fs::write(&empty_path, b"").await.unwrap();
            let files = vec![
                file_payload(&first_path.to_string_lossy(), "local")
                    .await
                    .unwrap(),
                file_payload(&second_path.to_string_lossy(), "local")
                    .await
                    .unwrap(),
                file_payload(&empty_path.to_string_lossy(), "local")
                    .await
                    .unwrap(),
            ];
            let receiver = LanShareServerRegistry::new();
            {
                let mut runtime = receiver.inner.lock().await;
                runtime.native.config.id = "native-receiver".into();
                runtime.native.config.name = "接收方".into();
                runtime.native.config.peers.push(TrustedPeer {
                    peer: Peer {
                        id: "native-sender".into(),
                        name: "发送方".into(),
                        ip: "127.0.0.1".into(),
                        port: DEFAULT_PORT,
                        ..Peer::default()
                    },
                    secret: "fixture-secret".into(),
                    secure: true,
                });
            }
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let records = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
            let server_records = records.clone();
            let server_registry = receiver.clone();
            let server_paths = paths.clone();
            let server = tauri::async_runtime::spawn(async move {
                for _connection in 0..2 {
                    let (socket, address) = listener.accept().await.unwrap();
                    let records = server_records.clone();
                    let registry = server_registry.clone();
                    let paths = server_paths.clone();
                    let service = service_fn(move |request: Request<Incoming>| {
                        let records = records.clone();
                        let registry = registry.clone();
                        let paths = paths.clone();
                        async move {
                            assert_eq!(request.uri().path(), "/api/native/secure");
                            assert!(request.uri().query().is_none());
                            assert!(request.headers().get("authorization").is_none());
                            let (parts, body) = request.into_parts();
                            let body = body.collect().await.unwrap().to_bytes();
                            records.lock().await.push(body.to_vec());
                            let response = handle_secure_request(
                                Request::from_parts(parts, Full::new(body)),
                                &registry,
                                &paths,
                                None,
                                address,
                            )
                            .await
                            .unwrap();
                            Ok::<_, Infallible>(response)
                        }
                    });
                    http1::Builder::new()
                        .serve_connection(TokioIo::new(socket), service)
                        .await
                        .unwrap();
                }
            });
            let registry = LanShareServerRegistry::new();
            {
                let mut runtime = registry.inner.lock().await;
                runtime.native.config.id = "native-sender".into();
                runtime.native.config.peers.push(TrustedPeer {
                    peer: Peer {
                        id: "native-receiver".into(),
                        ip: "127.0.0.1".into(),
                        port,
                        ..Peer::default()
                    },
                    secret: "fixture-secret".into(),
                    secure: true,
                });
            }
            registry.transfer.configure(transfer::Settings {
                send_mib_per_second: 4,
                max_concurrent_files: 1,
            });
            let started = tokio::time::Instant::now();
            for _attempt in 0..2 {
                assert!(deliver(
                    &registry,
                    "native-receiver",
                    "三个附件",
                    &files,
                    "batch_message"
                )
                .await
                .unwrap());
            }
            assert!(started.elapsed() >= Duration::from_millis(950));
            server.await.unwrap();
            let records = records.lock().await;
            assert_eq!(records.len(), 7);
            for body in records.iter() {
                for plaintext in [
                    b"fixture-secret".as_slice(),
                    b"second-file",
                    b"first.txt",
                    "三个附件".as_bytes(),
                ] {
                    assert!(!body.windows(plaintext.len()).any(|part| part == plaintext));
                }
                assert!(transport::decrypt("wrong-secret", "wrong-context", body).is_err());
            }
            let uploaded: Vec<LanShareFile> = read_array(&paths.lan_share_files.files).unwrap();
            assert_eq!(uploaded.len(), 3);
            assert_eq!(
                tokio::fs::read(&uploaded[0].path).await.unwrap(),
                first_content
            );
            assert_eq!(
                tokio::fs::read(&uploaded[1].path).await.unwrap(),
                b"second-file"
            );
            assert!(tokio::fs::read(&uploaded[2].path).await.unwrap().is_empty());
            let messages: Vec<LanShareMessage> =
                read_array(&paths.lan_share_files.messages).unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].content, "三个附件");
            assert_eq!(messages[0].attachments.len(), 3);
            assert_eq!(messages[0].attachments[0].name, "first.txt");
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn transfer_settings_persist_without_replacing_existing_pairing() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("transfer-settings-test"));
            let paths = crate::core::paths::resolve_app_paths(&root);
            let old_config = json!({
                "id": "native-local", "name": "我的电脑", "peers": [{
                    "peer": { "id": "native-remote", "name": "对方", "ip": "192.168.1.8", "port": DEFAULT_PORT },
                    "secret": "fixture-secret", "secure": true
                }]
            });
            write_json(&paths.lan_share_files.config, &old_config)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            let settings = transfer::Settings {
                send_mib_per_second: 5,
                max_concurrent_files: 2,
            };
            set_transfer_settings(&registry, &paths, settings)
                .await
                .unwrap();
            let config: Config = serde_json::from_slice(
                &tokio::fs::read(&paths.lan_share_files.config)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(config.transfer, settings);
            assert_eq!(config.id, "native-local");
            assert_eq!(config.peers[0].secret, "fixture-secret");
            assert!(config.peers[0].secure);
            assert_eq!(
                registry.inner.lock().await.native.snapshot()["transfer"],
                json!(settings)
            );
            let first = registry.transfer.acquire_file().await;
            let second = registry.transfer.acquire_file().await;
            drop((first, second));
            assert!(set_transfer_settings(
                &registry,
                &paths,
                transfer::Settings {
                    send_mib_per_second: 0,
                    ..settings
                }
            )
            .await
            .is_err());
            let saved: Config = serde_json::from_slice(
                &tokio::fs::read(&paths.lan_share_files.config)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(saved.transfer, settings);
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn discovery_accepts_only_encrypted_packets_and_uses_source_address() {
        let peer = Peer {
            id: "native-other".into(),
            name: "另一台电脑".into(),
            ip: "8.8.8.8".into(),
            port: DEFAULT_PORT,
            ..Peer::default()
        };
        let announcement = discovery::packet("announce", &peer).unwrap();
        let probe = discovery::packet("probe", &peer).unwrap();
        let source = "192.168.1.9:17632".parse().unwrap();
        let (kind, discovered, _) = discovery::parse(&announcement, source, "native-self").unwrap();
        assert_eq!(kind, DiscoveryPacketKind::Announce);
        assert_eq!(discovered.ip, "192.168.1.9");
        assert_eq!(
            discovery::parse(&probe, source, "native-self").unwrap().0,
            DiscoveryPacketKind::Probe
        );
        let plaintext = serde_json::to_vec(
            &json!({ "protocol": DISCOVERY_PROTOCOL, "kind": "announce", "peer": peer }),
        )
        .unwrap();
        assert!(discovery::parse(&plaintext, source, "native-self").is_none());
        let mut modified = announcement.clone();
        modified[20] ^= 1;
        assert!(discovery::parse(&modified, source, "native-self").is_none());
        assert!(discovery::parse(
            &announcement,
            "8.8.8.8:17632".parse().unwrap(),
            "native-self"
        )
        .is_none());
        assert!(discovery::parse(&announcement, source, "native-other").is_none());
        let mut runtime = NativeRuntime::default();
        runtime.config.peers.push(TrustedPeer {
            peer: discovered,
            secret: "do-not-expose".into(),
            secure: true,
        });
        assert!(!runtime.snapshot().to_string().contains("do-not-expose"));
    }

    #[test]
    fn discovery_is_enabled_for_new_and_legacy_configs() {
        assert!(Config::default().auto_discovery);
        assert!(Config::default().discovery_configured);
        let legacy = serde_json::from_value::<Config>(json!({
            "id": "native-self",
            "name": "本机"
        }))
        .unwrap();
        let mut legacy = legacy;
        migrate_discovery_config(&mut legacy);
        assert!(legacy.auto_discovery);
        assert!(legacy.discovery_configured);
        assert!(!serde_json::from_value::<Config>(json!({
            "id": "native-self",
            "name": "本机",
            "auto_discovery": false,
            "discovery_configured": true
        }))
        .unwrap()
        .auto_discovery);
    }

    #[test]
    fn native_connections_only_target_local_ipv4_addresses() {
        for ip in ["8.8.8.8", "example.com", "0.0.0.0", "255.255.255.255"] {
            assert!(peer_origin(&Peer {
                ip: ip.into(),
                port: DEFAULT_PORT,
                ..Peer::default()
            })
            .is_err());
        }
        assert!(peer_origin(&Peer {
            ip: "192.168.1.2".into(),
            port: DEFAULT_PORT,
            ..Peer::default()
        })
        .is_ok());
    }
}

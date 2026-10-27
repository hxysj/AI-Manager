use super::*;
use futures_util::FutureExt;
use http_body_util::Limited;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const DISCOVERY_PROTOCOL: &str = "monkey-thief-device-drop-v2";
const PAIRING_LIFETIME: u64 = 90_000;

mod discovery;
mod transport;
use transport::{secure_call, TransportAuth};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Peer {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) ip: String,
    pub(super) port: u16,
    #[serde(default)]
    pub(super) last_seen_at: u64,
    #[serde(default)]
    pub(super) supports_groups: bool,
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
    pub(super) hidden: bool,
    pub(super) peers: Vec<TrustedPeer>,
    pub(super) ignored_peers: Vec<String>,
    #[serde(default)]
    pub(super) remote_deleted_peers: Vec<String>,
    #[serde(default)]
    pub(super) revoked_peers: Vec<TrustedPeer>,
    pub(super) transfer: transfer::Settings,
    #[serde(default)]
    pub(super) transfer_configured: bool,
}

fn migrate_transfer_config(config: &mut Config) {
    // 传输策略不再开放配置，启动时统一恢复固定默认值。
    config.transfer = transfer::Settings::default();
    config.transfer_configured = true;
}

impl Default for Config {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            hidden: false,
            peers: Vec::new(),
            ignored_peers: Vec::new(),
            remote_deleted_peers: Vec::new(),
            revoked_peers: Vec::new(),
            transfer: transfer::Settings::default(),
            transfer_configured: true,
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
    scan_generation: u64,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
    scan_task: Option<tauri::async_runtime::JoinHandle<()>>,
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
        if let Some(task) = self.scan_task.take() {
            task.abort();
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
        self.scan_generation = self.scan_generation.wrapping_add(1);
        self.nearby.clear();
        self.pairing.clear();
        self.connecting.clear();
    }

    pub(super) fn online(&self, id: &str) -> bool {
        let lifetime = if self
            .config
            .peers
            .iter()
            .any(|trusted| trusted.peer.id == id)
        {
            15_000
        } else {
            60_000
        };
        self.nearby
            .get(id)
            .map(|peer| now_millis().saturating_sub(peer.last_seen_at) < lifetime)
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
            .find(|trusted| {
                trusted.peer.id == id
                    && trusted.secure
                    && !trusted.secret.is_empty()
                    && !self
                        .config
                        .remote_deleted_peers
                        .iter()
                        .any(|remote_id| remote_id == id)
            })
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
        if let Some(removed) = config
            .peers
            .iter()
            .find(|trusted| trusted.peer.id == device_id)
            .cloned()
        {
            config.revoked_peers.retain(|trusted| trusted.peer.id != device_id);
            config.revoked_peers.push(removed);
            if config.revoked_peers.len() > 100 {
                let excess = config.revoked_peers.len() - 100;
                config.revoked_peers.drain(..excess);
            }
        }
        config.peers.retain(|trusted| trusted.peer.id != device_id);
        config
            .remote_deleted_peers
            .retain(|remote_id| remote_id != device_id);
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
            .filter(|peer| self.online(&peer.id) || self.connecting.contains_key(&peer.id))
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
            "hidden": self.config.hidden,
            "scanning": self.scan_task.as_ref().map(|task| !task.inner().is_finished()).unwrap_or(false),
            "error": self.error,
            "peers": peers.iter().map(|peer| json!({
                "id": peer.id, "name": peer.name, "ip": peer.ip, "port": peer.port,
                "groupSupported": peer.supports_groups,
                "online": self.online(&peer.id),
                "paired": self.authenticate(&peer.id).is_some(),
                "remoteDeleted": self.config.remote_deleted_peers.iter().any(|id| id == &peer.id),
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
    (uuid::Uuid::parse_str(id).is_ok() || id.starts_with("native-") || id.starts_with("native_"))
        && id.len() <= 80
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

fn create_native_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn normalize_native_id(id: &mut String) {
    if id.is_empty() {
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
    migrate_transfer_config(&mut config);
    registry.transfer.configure(config.transfer);
    normalize_native_id(&mut config.id);
    if config.name.is_empty() {
        config.name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "我的电脑".into());
    }
    write_json(&paths.lan_share_files.config, &json!(config)).await?;
    let own_id = config.id.clone();
    {
        let mut runtime = registry.inner.lock().await;
        runtime.native.config = config;
        runtime.native.error.clear();
    }
    let (scan_tx, mut scan_rx) = tokio::sync::mpsc::channel::<()>(1);
    registry.inner.lock().await.native.scan_trigger = Some(scan_tx);
    let task_registry = registry.clone();
    let task_paths = paths.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        let mut known_checked_at = 0u64;
        loop {
            futures_util::select! {
                _ = interval.tick().fuse() => {
                    // 仅对已配对设备做加密心跳，不广播本机，也不在后台扫描网段。
                    if now_millis().saturating_sub(known_checked_at) >= 9_000 {
                        let known = task_registry.inner.lock().await.native.config.peers.clone();
                        for trusted in known.into_iter().filter(|trusted| trusted.secure && !trusted.secret.is_empty()) {
                            schedule_peer_check(&task_registry, &task_paths, Some(app.clone()), trusted.peer.clone(), trusted).await;
                        }
                        known_checked_at = now_millis();
                    }
                    {
                        let mut runtime = task_registry.inner.lock().await;
                        let native = &mut runtime.native;
                        if native
                            .scan_task
                            .as_ref()
                            .map(|task| task.inner().is_finished())
                            .unwrap_or(false)
                        {
                            native.scan_task = None;
                        }
                        native.nearby.retain(|id, peer| {
                            now_millis().saturating_sub(peer.last_seen_at) < 60_000 || native.connecting.contains_key(id)
                        });
                        native.pairing.retain(|_, request| {
                            now_millis().saturating_sub(request.created_at) < PAIRING_LIFETIME
                        });
                    }
                    emit_native_changed(&app, &task_registry).await;
                },
                scan_req = scan_rx.recv().fuse() => {
                    if scan_req.is_none() { break; }
                    let mut runtime = task_registry.inner.lock().await;
                    if runtime.native.scan_task.as_ref().map(|task| task.inner().is_finished()).unwrap_or(true) {
                        let registry = task_registry.clone();
                        let paths = task_paths.clone();
                        let app = app.clone();
                        let own_id = own_id.clone();
                        runtime.native.scan_generation = runtime.native.scan_generation.wrapping_add(1);
                        let scan_generation = runtime.native.scan_generation;
                        runtime.native.scan_task = Some(tauri::async_runtime::spawn(async move {
                            emit_native_changed(&app, &registry).await;
                            discovery::scan(registry.clone(), paths, app.clone(), own_id).await;
                            let mut runtime = registry.inner.lock().await;
                            if runtime.native.scan_generation == scan_generation {
                                runtime.native.scan_task = None;
                            }
                            drop(runtime);
                            emit_native_changed(&app, &registry).await;
                        }));
                    }
                },
            }
        }
    });
    registry.inner.lock().await.native.handle = Some(handle);
    Ok(())
}

// 发现心跳只推送小型运行态，不重复读取消息、文件历史和生成二维码。
async fn emit_native_changed(app: &tauri::AppHandle, registry: &LanShareServerRegistry) {
    let native = registry.inner.lock().await.native.snapshot();
    let _ = app.emit(EVENT_NATIVE_CHANGED, native);
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
        let response = match tokio::time::timeout(
            Duration::from_secs(2),
            secure_call(&client, &candidate, &auth, "ping", json!({}), &[], None),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                if is_remote_deleted_error(&error) {
                    let _ = mark_remote_deleted(&registry, &paths, &candidate.id).await;
                    if let Some(app) = app.as_ref() {
                        emit_native_changed(app, &registry).await;
                    }
                }
                return;
            }
            Err(_) => return,
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
        let peer_changed = match refresh_peer(&registry, &paths, &verified, &trusted.secret).await {
            Ok(changed) => changed,
            Err(_) => return,
        };
        let Some(app) = app else {
            return;
        };
        emit_native_changed(&app, &registry).await;
        if peer_changed {
            let _ = super::emit_state_changed(&app, &registry, &paths).await;
        }
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

pub(super) async fn set_hidden(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    hidden: bool,
) -> Result<Value, ManagerError> {
    let mut runtime = registry.inner.lock().await;
    let mut config = runtime.native.config.clone();
    if config.id.is_empty() {
        config = match tokio::fs::read(&paths.lan_share_files.config).await {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == ErrorKind::NotFound => Config::default(),
            Err(error) => return Err(error.into()),
        };
    }
    config.hidden = hidden;
    write_json(&paths.lan_share_files.config, &json!(config)).await?;
    runtime.native.config = config;
    if hidden {
        for request in runtime
            .native
            .pairing
            .values_mut()
            .filter(|request| request.status == "pending")
        {
            request.status = "rejected".into();
        }
    }
    Ok(runtime.native.snapshot())
}

pub(super) async fn scan_nearby(registry: &LanShareServerRegistry) -> Result<Value, ManagerError> {
    let scan_tx = {
        let mut runtime = registry.inner.lock().await;
        let native = &mut runtime.native;
        if native
            .scan_task
            .as_ref()
            .map(|task| task.inner().is_finished())
            .unwrap_or(true)
        {
            // 新一轮扫描不沿用公开候选，已隐藏或离线的设备不能由缓存继续出现。
            native.nearby.retain(|id, _| {
                native.connecting.contains_key(id)
                    || native
                        .config
                        .peers
                        .iter()
                        .any(|trusted| trusted.peer.id == *id)
            });
            native.error.clear();
        }
        runtime.native.scan_trigger.clone()
    };
    if let Some(tx) = scan_tx {
        let _ = tx.try_send(());
    }
    Ok(json!(true))
}

pub(super) async fn probe_known_peers(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    app: Option<tauri::AppHandle>,
) {
    let known = {
        let runtime = registry.inner.lock().await;
        runtime
            .native
            .config
            .peers
            .iter()
            .filter(|trusted| trusted.secure && !trusted.secret.is_empty())
            .cloned()
            .collect::<Vec<_>>()
    };

    for trusted in known {
        schedule_peer_check(
            registry,
            paths,
            app.clone(),
            trusted.peer.clone(),
            trusted,
        )
        .await;
    }
}

fn network_error(error: reqwest::Error) -> ManagerError {
    ManagerError::System(format!("设备连接失败：{}", error.without_url()))
}

const REMOTE_DELETED_MESSAGE: &str = "对方已删除此设备，无法发送消息，请重新连接。";

fn remote_deleted_error() -> ManagerError {
    ManagerError::System(REMOTE_DELETED_MESSAGE.into())
}

pub(super) fn is_remote_deleted_error(error: &ManagerError) -> bool {
    error.to_string() == REMOTE_DELETED_MESSAGE
}

pub(super) async fn ensure_send_allowed(
    registry: &LanShareServerRegistry,
    device_id: &str,
) -> Result<(), ManagerError> {
    let runtime = registry.inner.lock().await;
    if runtime
        .native
        .config
        .remote_deleted_peers
        .iter()
        .any(|id| id == device_id)
    {
        return Err(remote_deleted_error());
    }
    Ok(())
}

async fn mark_remote_deleted(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    device_id: &str,
) -> Result<(), ManagerError> {
    let mut runtime = registry.inner.lock().await;
    if !runtime
        .native
        .config
        .peers
        .iter()
        .any(|trusted| trusted.peer.id == device_id)
    {
        return Ok(());
    }
    if !runtime
        .native
        .config
        .remote_deleted_peers
        .iter()
        .any(|id| id == device_id)
    {
        runtime
            .native
            .config
            .remote_deleted_peers
            .push(device_id.to_string());
        write_json(
            &paths.lan_share_files.config,
            &json!(runtime.native.config),
        )
        .await?;
    }
    if let Some(task) = runtime.native.delivery_tasks.remove(device_id) {
        task.abort();
    }
    Ok(())
}

pub(super) async fn notify_peer_removed(
    registry: &LanShareServerRegistry,
    trusted: &TrustedPeer,
) -> Result<(), ManagerError> {
    if !trusted.secure || trusted.secret.is_empty() {
        return Ok(());
    }
    let own_id = registry.inner.lock().await.native.config.id.clone();
    let auth = TransportAuth {
        sender: own_id,
        receiver: trusted.peer.id.clone(),
        secret: trusted.secret.clone(),
        pairing_id: String::new(),
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .http1_only()
        .tcp_nodelay(true)
        .build()
        .map_err(network_error)?;
    secure_call(
        &client,
        &trusted.peer,
        &auth,
        "device-removed",
        json!({}),
        &[],
        None,
    )
    .await?;
    Ok(())
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
    config
        .remote_deleted_peers
        .retain(|remote_id| remote_id != &trusted.peer.id);
    config
        .revoked_peers
        .retain(|revoked| revoked.peer.id != trusted.peer.id);
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
) -> Result<bool, ManagerError> {
    let _storage = registry.storage.lock().await;
    let mut runtime = registry.inner.lock().await;
    let Some(trusted) = runtime.native.config.peers.iter_mut().find(|trusted| {
        trusted.peer.id == peer.id && trusted.secure && is_valid_token(&trusted.secret, secret)
    }) else {
        return Err(ManagerError::System("设备配对已撤销。".into()));
    };
    let address_changed = trusted.peer.ip != peer.ip || trusted.peer.port != peer.port;
    let changed = address_changed
        || trusted.peer.name != peer.name
        || trusted.peer.supports_groups != peer.supports_groups;
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
        // 心跳发现地址变化时同步直接会话，消息时间线才能标出新旧 IP。
        super::ensure_session(paths, &peer.id, "", &peer.name, &peer.ip)?;
    }
    runtime.native.nearby.insert(peer.id.clone(), peer.clone());
    Ok(changed)
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
    let request_id = create_id();
    auth.receiver = peer.id.clone();
    let response = secure_call(&client, &peer, &auth, "pair", json!({
        "requestId": request_id, "publicKey": public_key, "peer": {
            "id": identity.id, "name": identity.name, "ip": "", "port": own_port,
            "supportsGroups": true
        }
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
    group_payload: Option<&Value>,
) -> Result<LanShareSession, ManagerError> {
    if let Some(group_payload) = group_payload.filter(|value| !value.is_null()) {
        let group: LanShareGroup = serde_json::from_value(group_payload.clone())
            .map_err(|_| ManagerError::System("群聊信息格式无效。".into()))?;
        if group.id.is_empty()
            || group.owner_device_id.is_empty()
            || (group.owner_device_id != peer.id
                && !group.members.iter().any(|member| member.device_id == peer.id))
        {
            return Err(ManagerError::System("群聊成员身份无效。".into()));
        }
        return ensure_native_group_session(registry, paths, &group).await;
    }
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

async fn ensure_native_group_session(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    incoming: &LanShareGroup,
) -> Result<LanShareSession, ManagerError> {
    let _storage = registry.storage.lock().await;
    let mut group = incoming.clone();
    let mut groups: Vec<LanShareGroup> = read_array(&paths.lan_share_files.groups)?;
    let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
    let now = now_millis();

    let owner_session = if let Some(session) = sessions
        .iter_mut()
        .find(|session| session.mode == "group" && session.group_id == group.id && session.device_id.is_empty())
    {
        session.group_name = group.name.clone();
        session.message_visibility = group.message_visibility.clone();
        session.updated_at = now;
        session.clone()
    } else {
        let session = LanShareSession {
            id: create_id(),
            device_id: String::new(),
            device_name: "群聊".to_string(),
            ip: String::new(),
            mode: "group".to_string(),
            group_id: group.id.clone(),
            group_name: group.name.clone(),
            message_visibility: group.message_visibility.clone(),
            joined_at: now,
            created_at: now,
            updated_at: now,
        };
        sessions.push(session.clone());
        session
    };

    for member in group.members.clone() {
        let device = LanShareDevice {
            id: member.device_id.clone(),
            name: member.device_name.clone(),
            auto_name: member.device_name.clone(),
            user_agent: "MonkeyThief/Desktop".to_string(),
            ip: member.ip.clone(),
            first_seen_at: member.joined_at,
            last_seen_at: member.last_seen_at,
        };
        upsert_group_session(&mut sessions, &mut group, &device, &member.ip);
    }

    if let Some(existing) = groups.iter_mut().find(|item| item.id == group.id) {
        *existing = group.clone();
    } else {
        groups.insert(0, group);
    }
    sort_sessions(&mut sessions);
    write_json(&paths.lan_share_files.groups, &json!(groups)).await?;
    write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
    Ok(owner_session)
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
                last_seen_at: now_millis(),
                supports_groups: true,
            }))
        }
        "group-sync" => {
            let group = payload
                .get("group")
                .ok_or_else(|| ManagerError::System("群聊信息缺失。".into()))?;
            let group: LanShareGroup = serde_json::from_value(group.clone())?;
            if group.id.is_empty() || group.owner_device_id != peer.id {
                return Err(ManagerError::System("群聊邀请来源无效。".into()));
            }
            let session = ensure_native_group_session(registry, paths, &group).await?;
            if let Some(app) = app {
                emit_state_changed(app, registry, paths).await?;
            }
            Ok(json!({ "groupId": group.id, "sessionId": session.id }))
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
        "device-removed" => {
            mark_remote_deleted(registry, paths, &peer.id).await?;
            if let Some(app) = app {
                emit_state_changed(app, registry, paths).await?;
            }
            Ok(json!({ "removed": true }))
        }
        "message-exists" => {
            let message_id = format!("{}-{}", peer.id, string_value(payload.get("messageId")));
            let uuid_id = create_stable_id(&format!("lan-message:{message_id}"));
            let _storage = registry.storage.lock().await;
            let exists = read_array::<LanShareMessage>(&paths.lan_share_files.messages)?
                .iter()
                .any(|message| {
                    (message.id == message_id || message.id == uuid_id)
                        && message.device_id == peer.id
                });
            Ok(json!({ "exists": exists }))
        }
        "upload-status" => {
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
                payload.get("group"),
            )
            .await?;
            let requests = payload["files"]
                .as_array()
                .filter(|files| {
                    !files.is_empty() && files.len() <= attachments::MAX_ATTACHMENTS
                })
                .ok_or_else(|| ManagerError::System("附件状态查询无效。".into()))?;
            attachments::cleanup_uploads(registry, paths).await?;
            let mut statuses = Vec::with_capacity(requests.len());
            let mut event_files = Vec::with_capacity(requests.len());
            let mut total_bytes = 0u64;
            for (index, request) in requests.iter().enumerate() {
                if request["fileIndex"].as_u64() != Some(index as u64) {
                    return Err(ManagerError::System("附件状态顺序无效。".into()));
                }
                let total_size = request["totalSize"]
                    .as_u64()
                    .ok_or_else(|| ManagerError::System("附件大小无效。".into()))?;
                let name = string_value(request.get("name"));
                let mime_type = mime_guess::from_path(&name)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string();
                let status = attachments::upload_status(
                    registry,
                    paths,
                    &session.id,
                    &string_value(request.get("uploadId")),
                    &name,
                    request["chunkTotal"]
                        .as_u64()
                        .ok_or_else(|| ManagerError::System("附件分块数无效。".into()))?,
                    total_size,
                )
                .await?;
                total_bytes = total_bytes.saturating_add(total_size);
                event_files.push(json!({
                    "index": index,
                    "name": name,
                    "size": total_size,
                    "mimeType": mime_type,
                    "loadedBytes": status["loadedBytes"],
                    "complete": status["complete"]
                }));
                statuses.push(status);
            }
            if let Some(app) = app {
                let _ = app.emit(
                    EVENT_INCOMING_TRANSFER_PROGRESS,
                    json!({
                        "phase": "start",
                        "messageId": source_id,
                        "sessionId": session.id,
                        "deviceId": peer.id,
                        "deviceName": peer.name,
                        "totalBytes": total_bytes,
                        "files": event_files
                    }),
                );
            }
            Ok(json!({ "sessionId": session.id, "files": statuses }))
        }
        "file-chunk" => {
            let session = peer_session(
                registry,
                paths,
                peer,
                &string_value(payload.get("sessionId")),
                payload.get("group"),
            )
            .await?;
            let number = |key: &str| {
                payload[key]
                    .as_u64()
                    .ok_or_else(|| ManagerError::System(format!("分块参数 {key} 无效。")))
            };
            let source_id = string_value(payload.get("messageId"));
            if !source_id.is_empty()
                && (source_id.len() > 100
                    || !source_id.chars().all(|character| {
                        character.is_ascii_alphanumeric() || character == '-' || character == '_'
                    }))
            {
                return Err(ManagerError::System("消息标识无效。".into()));
            }
            let upload_id = string_value(payload.get("uploadId"));
            let chunk_index = number("chunkIndex")?;
            let total_size = number("totalSize")?;
            let progress = if source_id.is_empty() {
                None
            } else {
                let file_index = number("fileIndex")?;
                let file_count = number("fileCount")?;
                let message_total_bytes = number("messageTotalBytes")?;
                if file_count == 0
                    || file_count > attachments::MAX_ATTACHMENTS as u64
                    || file_index >= file_count
                    || message_total_bytes < total_size
                {
                    return Err(ManagerError::System("附件进度参数无效。".into()));
                }
                Some((file_index, file_count, message_total_bytes))
            };
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
                total_size,
                number("contentLength")?,
                &string_value(payload.get("contentRange")),
                &string_value(payload.get("sha256")),
            )
            .await?;
            if let (Some(app), Some((file_index, file_count, message_total_bytes))) =
                (app, progress)
            {
                let _ = app.emit(
                    EVENT_INCOMING_TRANSFER_PROGRESS,
                    json!({
                        "phase": "progress",
                        "messageId": source_id,
                        "sessionId": session.id,
                        "deviceId": peer.id,
                        "deviceName": peer.name,
                        "fileIndex": file_index,
                        "fileCount": file_count,
                        "fileName": string_value(payload.get("name")),
                        "fileSize": total_size,
                        "fileLoadedBytes": (chunk_index + 1)
                            .saturating_mul(attachments::FILE_CHUNK_BYTES)
                            .min(total_size),
                        "fileComplete": file.is_some(),
                        "totalBytes": message_total_bytes
                    }),
                );
            }
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
                payload.get("group"),
            )
            .await?;
            let files = attachments::prepare(
                registry,
                paths,
                &session.id,
                &json!({ "attachmentIds": payload["attachmentIds"] }),
            )
            .await?;
            let legacy_id = format!("{}-{source_id}", peer.id);
            let message_id = {
                let _storage = registry.storage.lock().await;
                if read_array::<LanShareMessage>(&paths.lan_share_files.messages)?
                    .iter()
                    .any(|message| message.id == legacy_id && message.device_id == peer.id)
                {
                    legacy_id
                } else {
                    create_stable_id(&format!("lan-message:{legacy_id}"))
                }
            };
            let stored = append_message_with_attachments(
                registry,
                paths,
                &peer.id,
                &session.id,
                "mobile-to-desktop",
                &string_value(payload.get("content")),
                true,
                &files,
                Some(&message_id),
            )
            .await?;
            attachments::publish(registry, paths, &files).await?;
            if let Some(app) = app {
                emit_message_created(app, &stored);
                if !files.is_empty() {
                    let _ = app.emit(
                        EVENT_INCOMING_TRANSFER_PROGRESS,
                        json!({
                            "phase": "complete",
                            "messageId": source_id,
                            "storedMessageId": stored.id,
                            "sessionId": stored.session_id,
                            "deviceId": peer.id
                        }),
                    );
                }
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

fn device_removed_response() -> Response<BoxBody> {
    let payload = json!({
        "status": "error",
        "data": Value::Null,
        "message": REMOTE_DELETED_MESSAGE
    });
    Response::builder()
        .status(StatusCode::GONE)
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .header("x-lan-device-removed", "true")
        .header("x-lan-version", transport::VERSION)
        .body(boxed_full(Bytes::from(serde_json::to_vec(&payload).unwrap())))
        .unwrap()
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
    let (mut peer, secret, revoked) = {
        let runtime = registry.inner.lock().await;
        if (!receiver.is_empty() && receiver != runtime.native.config.id)
            || sender == runtime.native.config.id
        {
            return Ok(api_error(StatusCode::UNAUTHORIZED, "接收设备不匹配。"));
        }
        // 隐藏节点不向未配对设备返回身份，也不接受新的发现和配对请求。
        if runtime.native.config.hidden && scope == "software" {
            return Ok(api_error(StatusCode::NOT_FOUND, "设备不可用。"));
        }
        match scope.as_str() {
            "software" if pairing_id.is_empty() => (
                Peer {
                    id: sender.clone(),
                    ip: client_ip(address),
                    ..Peer::default()
                },
                String::new(),
                false,
            ),
            "paired" if pairing_id.is_empty() => {
                if let Some(trusted) = runtime.native.authenticate(&sender) {
                    (trusted.peer, trusted.secret, false)
                } else if runtime
                    .native
                    .config
                    .remote_deleted_peers
                    .iter()
                    .any(|id| id == &sender)
                {
                    let Some(deleted) = runtime
                        .native
                        .config
                        .peers
                        .iter()
                        .find(|trusted| {
                            trusted.peer.id == sender
                                && trusted.secure
                                && !trusted.secret.is_empty()
                        })
                    else {
                        return Ok(api_error(StatusCode::UNAUTHORIZED, "设备尚未安全配对。"));
                    };
                    (deleted.peer.clone(), deleted.secret.clone(), true)
                } else if let Some(revoked) = runtime
                    .native
                    .config
                    .revoked_peers
                    .iter()
                    .find(|trusted| {
                        trusted.peer.id == sender
                            && trusted.secure
                            && !trusted.secret.is_empty()
                    })
                {
                    (revoked.peer.clone(), revoked.secret.clone(), true)
                } else {
                    return Ok(api_error(StatusCode::UNAUTHORIZED, "设备尚未安全配对。"));
                }
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
                (pairing.peer.clone(), pairing.secret.clone(), false)
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
    // 文件大包单独排队并按帧限速，短消息和心跳无需等待文件许可。
    let bulk = request
        .body()
        .size_hint()
        .upper()
        .map(|size| size > 64 * 1024)
        .unwrap_or(true);
    let mut _permit = if bulk {
        Some(registry.transfer.acquire_upload().await)
    } else {
        None
    };
    let capacity = request
        .body()
        .size_hint()
        .upper()
        .unwrap_or(64 * 1024)
        .min(transport::MAX_BODY as u64) as usize;
    let mut body = Limited::new(request.into_body(), transport::MAX_BODY);
    let mut ciphertext = Vec::with_capacity(capacity);
    while let Some(frame) = body.frame().await {
        let Ok(frame) = frame else {
            return Ok(api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "加密载荷过大或接收中断。",
            ));
        };
        if let Ok(bytes) = frame.into_data() {
            if bulk {
                registry.transfer.receive_bytes(bytes.len()).await;
            }
            ciphertext.extend_from_slice(&bytes);
        }
    }
    let received_length = ciphertext.len();
    let request_nonce = ciphertext
        .get(..12)
        .map(|nonce| <[u8; 12]>::try_from(nonce).unwrap());
    let plaintext = match transport::decrypt_owned(&auth.secret, &aad, ciphertext) {
        Ok(plaintext) => plaintext,
        Err(_) => {
            return Ok(api_error(
                StatusCode::UNAUTHORIZED,
                "密钥不匹配或内容已被篡改。",
            ))
        }
    };
    if revoked {
        return Ok(device_removed_response());
    }
    // 通过认证后的操作结果和错误都加密返回。
    let result = async {
        let (metadata, bytes) = transport::unpack(plaintext)?;
        let operation = string_value(metadata.get("operation"));
        if !bulk && operation == "file-chunk" {
            // 尾块和小文件在解密后计费，控制消息仍可直接通过。
            _permit = Some(registry.transfer.acquire_upload().await);
            registry.transfer.receive_bytes(received_length).await;
        }
        let permitted = match scope.as_str() {
            "software" => matches!(operation.as_str(), "info" | "pair"),
            "pairing" => {
                operation == "pair-status" && metadata["payload"]["requestId"] == pairing_id
            }
            "paired" => matches!(
                operation.as_str(),
                "ping"
                    | "message-exists"
                    | "upload-status"
                    | "file-chunk"
                    | "message"
                    | "group-sync"
                    | "device-removed"
            ),
            _ => false,
        } && !(auth.receiver.is_empty() && operation != "info");
        if !permitted {
            return Err(ManagerError::System("当前密钥无权执行此操作。".into()));
        }
        if !registry.inner.lock().await.native.replay.accept(
            &format!("{sender}|{scope}|{pairing_id}"),
            &request_nonce.unwrap(),
            metadata["sentAt"].as_u64().unwrap_or(0),
            now_millis(),
        ) {
            return Err(ManagerError::System(
                "加密请求已重复或过期，请检查双方时间。".into(),
            ));
        }
        peer.ip = client_ip(address);
        let mut peer_changed = false;
        if scope == "paired" {
            peer.last_seen_at = now_millis();
            peer_origin(&peer)?;
            peer_changed = refresh_peer(registry, paths, &peer, &auth.secret).await?;
        }
        if peer_changed {
            if let Some(app) = app {
                emit_state_changed(app, registry, paths).await?;
            }
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
    let request_nonce =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(request_nonce.unwrap());
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
    paths: &AppPaths,
    device_id: &str,
    content: &str,
    files: &[LanShareFile],
    message_id: &str,
    app: Option<&tauri::AppHandle>,
) -> Result<bool, ManagerError> {
    deliver_to_peer(registry, paths, device_id, content, files, message_id, app, None).await
}

pub(super) async fn deliver_group(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    group_id: &str,
    content: &str,
    files: &[LanShareFile],
    message_id: &str,
    app: Option<&tauri::AppHandle>,
) -> Result<usize, ManagerError> {
    let group = {
        let _storage = registry.storage.lock().await;
        read_array::<LanShareGroup>(&paths.lan_share_files.groups)?
            .into_iter()
            .find(|group| group.id == group_id)
            .ok_or_else(|| ManagerError::System("群聊不存在。".into()))?
    };
    let own_id = registry.inner.lock().await.native.config.id.clone();
    let mut target_ids = Vec::with_capacity(group.members.len() + 1);
    if !group.owner_device_id.is_empty() && group.owner_device_id != own_id {
        target_ids.push(group.owner_device_id.clone());
    }
    for member in &group.members {
        if member.device_id != own_id && !target_ids.contains(&member.device_id) {
            target_ids.push(member.device_id.clone());
        }
    }

    let mut delivered = 0;
    for device_id in target_ids {
        match deliver_to_peer(
            registry,
            paths,
            &device_id,
            content,
            files,
            message_id,
            app,
            Some(&group),
        )
        .await
        {
            Ok(true) => delivered += 1,
            Ok(false) => {}
            Err(error) if is_remote_deleted_error(&error) => {
                let _ = mark_remote_deleted(registry, paths, &device_id).await;
            }
            Err(_) => {}
        }
    }
    Ok(delivered)
}

pub(super) async fn sync_group_to_peer(
    registry: &LanShareServerRegistry,
    _paths: &AppPaths,
    group: &LanShareGroup,
    device_id: &str,
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
    if !trusted.peer.supports_groups {
        return Ok(false);
    }
    let auth = TransportAuth {
        sender: own_id,
        receiver: trusted.peer.id.clone(),
        secret: trusted.secret.clone(),
        pairing_id: String::new(),
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .http1_only()
        .tcp_nodelay(true)
        .build()
        .map_err(network_error)?;
    secure_call(
        &client,
        &trusted.peer,
        &auth,
        "group-sync",
        json!({ "group": group }),
        &[],
        None,
    )
    .await?;
    Ok(true)
}

fn transfer_upload_id(
    sender_id: &str,
    group_id: &str,
    message_id: &str,
    file_id: &str,
) -> String {
    create_stable_id(&format!(
        "lan-upload:{sender_id}:{group_id}:{message_id}:{file_id}"
    ))
}

async fn deliver_to_peer(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    device_id: &str,
    content: &str,
    files: &[LanShareFile],
    message_id: &str,
    app: Option<&tauri::AppHandle>,
    group: Option<&LanShareGroup>,
) -> Result<bool, ManagerError> {
    let (trusted, own_id) = {
        let runtime = registry.inner.lock().await;
        if runtime
            .native
            .config
            .remote_deleted_peers
            .iter()
            .any(|id| id == device_id)
        {
            return Err(remote_deleted_error());
        }
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
        .http1_only()
        .tcp_nodelay(true)
        .build()
        .map_err(network_error)?;
    let existing = match secure_call(
        &client,
        &trusted.peer,
        &auth,
        "message-exists",
        json!({
            "messageId": message_id,
            "groupId": group.map(|item| item.id.clone()).unwrap_or_default()
        }),
        &[],
        None,
    )
    .await
    {
        Ok(existing) => existing,
        Err(error) => {
            if is_remote_deleted_error(&error) {
                let _ = mark_remote_deleted(registry, paths, device_id).await;
                if let Some(app) = app {
                    emit_state_changed(app, registry, paths).await.ok();
                }
            }
            return Err(error);
        }
    };
    if existing["exists"] == true {
        return Ok(true);
    }
    let total_bytes = files.iter().map(|file| file.size).sum::<u64>();
    let group_id = group.map(|item| item.id.as_str()).unwrap_or("direct");
    let mut remote_session = String::new();
    let mut resume_by_upload = HashMap::<String, (u64, String)>::new();
    if !files.is_empty() {
        let requests = files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let chunk_total = file
                    .size
                    .div_ceil(attachments::FILE_CHUNK_BYTES)
                    .max(1);
                json!({
                    "fileIndex": index,
                    "uploadId": transfer_upload_id(
                        &auth.sender,
                        group_id,
                        message_id,
                        &file.id
                    ),
                    "name": file.name,
                    "chunkTotal": chunk_total,
                    "totalSize": file.size
                })
            })
            .collect::<Vec<_>>();
        match secure_call(
            &client,
            &trusted.peer,
            &auth,
            "upload-status",
            json!({
                "messageId": message_id,
                "sessionId": "",
                "group": group,
                "files": requests
            }),
            &[],
            None,
        )
        .await
        {
            Ok(status) => {
                remote_session = string_value(status.get("sessionId"));
                if remote_session.is_empty() {
                    return Err(ManagerError::System("对方返回的会话无效。".into()));
                }
                let statuses = status["files"]
                    .as_array()
                    .filter(|items| items.len() == files.len())
                    .ok_or_else(|| ManagerError::System("对方返回的附件状态无效。".into()))?;
                for (file, item) in files.iter().zip(statuses) {
                    let upload_id = string_value(item.get("uploadId"));
                    let expected_upload_id =
                        transfer_upload_id(&auth.sender, group_id, message_id, &file.id);
                    let next_index = item["nextIndex"].as_u64().ok_or_else(|| {
                        ManagerError::System("对方返回的分块检查点无效。".into())
                    })?;
                    if upload_id != expected_upload_id
                        || resume_by_upload
                            .insert(
                                upload_id,
                                (next_index, string_value(item["file"].get("id"))),
                            )
                            .is_some()
                    {
                        return Err(ManagerError::System(
                            "对方返回的附件状态重复或缺失。".into(),
                        ));
                    }
                }
            }
            Err(ManagerError::System(message)) if message == "不支持的设备快传操作。" => {}
            Err(error) => return Err(error),
        }
    }
    // 进度以对端已确认的字节为准，只推送小事件，不写盘或刷新聊天历史。
    let confirmed_bytes = files
        .iter()
        .map(|file| {
            let upload_id = transfer_upload_id(&auth.sender, group_id, message_id, &file.id);
            resume_by_upload
                .get(&upload_id)
                .map(|(next_index, _)| {
                    next_index
                        .saturating_mul(attachments::FILE_CHUNK_BYTES)
                        .min(file.size)
                })
                .unwrap_or(0)
        })
        .sum::<u64>();
    let started = tokio::time::Instant::now();
    let progress =
        std::sync::Mutex::new((confirmed_bytes, started - Duration::from_secs(1)));
    let report_progress = |index: usize, size: u64, complete: bool| {
        let mut progress = progress.lock().unwrap();
        progress.0 += size;
        if complete || progress.1.elapsed() >= Duration::from_millis(200) {
            progress.1 = tokio::time::Instant::now();
            if let Some(app) = app {
                let elapsed = started.elapsed().as_millis().max(1) as u64;
                let _ = app.emit(
                    EVENT_TRANSFER_PROGRESS,
                    json!({
                        "messageId": message_id, "fileIndex": index, "fileComplete": complete,
                        "loadedBytes": progress.0, "totalBytes": total_bytes,
                        "bytesPerSecond": progress.0.saturating_sub(confirmed_bytes)
                            .saturating_mul(1000) / elapsed
                    }),
                );
            }
        }
    };
    // 最多挂起八个文件任务；真正读取文件前领取全局许可，设置的并发立即生效。
    let mut uploaded_files = futures_util::stream::iter(files.to_vec().into_iter().enumerate())
        .map(|(index, file)| {
            let client = &client;
            let trusted = &trusted;
            let auth = &auth;
            let report_progress = &report_progress;
            let resume_by_upload = &resume_by_upload;
            let initial_session = &remote_session;
            async move {
                let mut remote_session = initial_session.clone();
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
                let upload_id =
                    transfer_upload_id(&auth.sender, group_id, message_id, &file.id);
                let (next_index, mut identifier) = resume_by_upload
                    .get(&upload_id)
                    .cloned()
                    .unwrap_or((0, String::new()));
                if next_index > chunk_total {
                    return Err(ManagerError::System(
                        "对方返回的分块检查点超过附件范围。".into(),
                    ));
                }
                if next_index == chunk_total {
                    if identifier.is_empty() {
                        return Err(ManagerError::System(
                            "对方未返回已接收附件的信息。".into(),
                        ));
                    }
                    report_progress(index, 0, true);
                    return Ok((index, remote_session, identifier));
                }
                for chunk_index in next_index..chunk_total {
                    if registry
                        .inner
                        .lock()
                        .await
                        .native
                        .authenticate(device_id)
                        .map(|current| is_valid_token(&current.secret, &auth.secret))
                        != Some(true)
                    {
                        return Err(ManagerError::System("设备配对已撤销，传输已停止。".into()));
                    }
                    let offset = chunk_index * attachments::FILE_CHUNK_BYTES;
                    let chunk_length = file
                        .size
                        .saturating_sub(offset)
                        .min(attachments::FILE_CHUNK_BYTES);
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
                    let mut attempt = 0;
                    let uploaded = loop {
                        // 每次重试重新读取该块并使用新 nonce，避免保留整块备份或触发重放保护。
                        source.seek(std::io::SeekFrom::Start(offset)).await?;
                        let mut chunk = Vec::with_capacity(chunk_length as usize + 4096);
                        chunk.resize(chunk_length as usize, 0);
                        source.read_exact(&mut chunk).await?;
                        let chunk_sha256 = format!("{:x}", Sha256::digest(&chunk));
                        let result = match secure_call(
                            client,
                            &trusted.peer,
                            auth,
                            "file-chunk",
                            json!({
                                "messageId": message_id,
                                "fileIndex": index,
                                "fileCount": files.len(),
                                "messageTotalBytes": total_bytes,
                                "name": file.name, "sessionId": remote_session, "uploadId": upload_id,
                                "group": group,
                                "chunkIndex": chunk_index, "chunkTotal": chunk_total, "offset": offset,
                                "totalSize": file.size, "sha256": chunk_sha256,
                                "contentLength": chunk_length, "contentRange": content_range,
                            }),
                            chunk,
                            Some(&registry.transfer),
                        )
                        .await
                        {
                            Ok(result) => Ok(result),
                            Err(error) => {
                                if is_remote_deleted_error(&error) {
                                    let _ = mark_remote_deleted(registry, paths, device_id).await;
                                    if let Some(app) = app {
                                        emit_state_changed(app, registry, paths).await.ok();
                                    }
                                }
                                Err(error)
                            }
                        };
                        match result {
                            Ok(uploaded) => break uploaded,
                            Err(ManagerError::System(message))
                                if attempt < 2 && message.starts_with("设备连接失败：") =>
                            {
                                attempt += 1;
                                tokio::time::sleep(Duration::from_millis(200 * attempt)).await;
                            }
                            Err(error) => return Err(error),
                        }
                    };
                    let complete = chunk_index + 1 == chunk_total;
                    if uploaded["uploadId"] != upload_id
                        || uploaded["chunkIndex"].as_u64() != Some(chunk_index)
                        || uploaded["complete"].as_bool() != Some(complete)
                    {
                        return Err(ManagerError::System("对方返回的分块确认不一致。".into()));
                    }
                    let session = string_value(uploaded.get("sessionId"));
                    if session.is_empty()
                        || (!remote_session.is_empty() && session != remote_session)
                    {
                        return Err(ManagerError::System("对方返回的会话不一致。".into()));
                    }
                    remote_session = session;
                    if complete {
                        identifier = string_value(uploaded["file"].get("id"));
                        if identifier.is_empty() {
                            return Err(ManagerError::System("对方未返回完整附件。".into()));
                        }
                    }
                    report_progress(index, chunk_length, complete);
                }
                Ok::<_, ManagerError>((index, remote_session, identifier))
            }
        })
        .buffer_unordered(8)
        .try_collect::<Vec<_>>()
        .await?;
    uploaded_files.sort_by_key(|(index, _, _)| *index);
    let remote_session = uploaded_files
        .first()
        .map(|(_, session, _)| session.as_str())
        .unwrap_or("");
    if uploaded_files
        .iter()
        .any(|(_, session, _)| session != remote_session)
    {
        return Err(ManagerError::System("对方返回的会话不一致。".into()));
    }
    let identifiers = uploaded_files
        .iter()
        .map(|(_, _, identifier)| identifier)
        .collect::<Vec<_>>();
    let result = secure_call(&client, &trusted.peer, &auth, "message", json!({
        "messageId": message_id, "sessionId": remote_session, "content": content,
        "attachmentIds": identifiers, "group": group
    }), &[], None).await;
    if let Err(error) = &result {
        if is_remote_deleted_error(error) {
            let _ = mark_remote_deleted(registry, paths, device_id).await;
            if let Some(app) = app {
                emit_state_changed(app, registry, paths).await.ok();
            }
        }
    }
    result?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_nodes_reject_discovery_but_keep_authenticated_peers_and_persist_settings() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-local".into();
            remember(
                &registry,
                &paths,
                TrustedPeer {
                    peer: Peer {
                        id: "native-known".into(),
                        ip: "127.0.0.1".into(),
                        port: DEFAULT_PORT,
                        ..Peer::default()
                    },
                    secret: "trusted-secret".into(),
                    secure: true,
                },
            )
            .await
            .unwrap();
            let snapshot = set_hidden(&registry, &paths, true).await.unwrap();
            assert_eq!(snapshot["hidden"], true);
            let saved: Config = serde_json::from_slice(
                &tokio::fs::read(&paths.lan_share_files.config)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert!(saved.hidden);
            assert_eq!(saved.peers.len(), 1);
            assert_eq!(saved.id, "native-local");
            let public = TransportAuth {
                sender: "native-stranger".into(),
                receiver: String::new(),
                secret: String::new(),
                pairing_id: String::new(),
            };
            for operation in ["info", "pair"] {
                let packet = encrypted_test_packet(&public, operation, json!({}));
                let response = handle_secure_request(
                    encrypted_test_request(&public, &packet),
                    &registry,
                    &paths,
                    None,
                    "127.0.0.1:9000".parse().unwrap(),
                )
                .await
                .unwrap();
                assert_eq!(response.status(), StatusCode::NOT_FOUND);
            }
            assert!(registry.inner.lock().await.native.pairing.is_empty());
            let known = TransportAuth {
                sender: "native-known".into(),
                receiver: "native-local".into(),
                secret: "trusted-secret".into(),
                pairing_id: String::new(),
            };
            let packet = encrypted_test_packet(&known, "ping", json!({}));
            let response = handle_secure_request(
                encrypted_test_request(&known, &packet),
                &registry,
                &paths,
                None,
                "127.0.0.1:9000".parse().unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(
                decoded_test_response(response, &known, &packet).await.0,
                StatusCode::OK
            );
            assert!(registry.inner.lock().await.native.online("native-known"));
            assert_eq!(
                set_hidden(&registry, &paths, false).await.unwrap()["hidden"],
                false
            );
            let packet = encrypted_test_packet(&public, "info", json!({}));
            let response = handle_secure_request(
                encrypted_test_request(&public, &packet),
                &registry,
                &paths,
                None,
                "127.0.0.1:9000".parse().unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(
                decoded_test_response(response, &public, &packet).await.0,
                StatusCode::OK
            );
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn transfer_config_is_always_reset_to_default() {
        for (rate, concurrent, configured) in
            [(10, 1, false), (5, 1, false), (10, 2, false), (10, 1, true)]
        {
            let mut value = json!({ "transfer": { "sendMibPerSecond": rate, "maxConcurrentFiles": concurrent } });
            if configured {
                value["transfer_configured"] = json!(true);
            }
            let mut config: Config = serde_json::from_value(value).unwrap();
            migrate_transfer_config(&mut config);
            assert_eq!(config.transfer, transfer::Settings::default());
            assert!(config.transfer_configured);
            migrate_transfer_config(&mut config);
            assert_eq!(config.transfer, transfer::Settings::default());
        }
    }

    #[test]
    fn encrypted_uploads_are_limited_while_heartbeats_remain_responsive() {
        tauri::async_runtime::block_on(async {
            let temp_root = std::env::temp_dir();
            let root = temp_root.join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-receiver".into();
            remember(
                &registry,
                &paths,
                TrustedPeer {
                    peer: Peer {
                        id: "native-sender".into(),
                        ip: "127.0.0.1".into(),
                        port: DEFAULT_PORT,
                        ..Peer::default()
                    },
                    secret: "upload-test-secret".into(),
                    secure: true,
                },
            )
            .await
            .unwrap();
            let auth = TransportAuth {
                sender: "native-sender".into(),
                receiver: "native-receiver".into(),
                secret: "upload-test-secret".into(),
                pairing_id: String::new(),
            };
            registry.transfer.configure(transfer::Settings {
                send_mib_per_second: 1,
                max_concurrent_files: 1,
            });
            // 大块与短尾块都必须计入接收预算，心跳不能等待它们的文件许可。
            for size in [512 * 1024, 32 * 1024] {
                let content = vec![7; size];
                let packet = transport::pack(&json!({
                    "operation": "file-chunk", "sentAt": now_millis(),
                    "payload": {
                        "name": "rate-test.bin", "sessionId": "", "uploadId": create_id(),
                        "chunkIndex": 0, "chunkTotal": 1, "offset": 0, "totalSize": size,
                        "sha256": format!("{:x}", Sha256::digest(&content)),
                        "contentLength": size, "contentRange": format!("bytes 0-{}/{}", size - 1, size)
                    }
                }), content).unwrap();
                let packet = transport::encrypt(&auth.secret, &auth.aad(), &packet).unwrap();
                registry.transfer.configure(transfer::Settings {
                    send_mib_per_second: 1,
                    max_concurrent_files: 1,
                });
                let started = tokio::time::Instant::now();
                let request = encrypted_test_request(&auth, &packet);
                let mut upload = Box::pin(handle_secure_request(
                    request,
                    &registry,
                    &paths,
                    None,
                    "127.0.0.1:12345".parse().unwrap(),
                ));
                assert!(upload.as_mut().now_or_never().is_none());
                let ping_packet = encrypted_test_packet(&auth, "ping", json!({}));
                let ping = tokio::time::timeout(
                    Duration::from_millis(250),
                    handle_secure_request(
                        encrypted_test_request(&auth, &ping_packet),
                        &registry,
                        &paths,
                        None,
                        "127.0.0.1:12345".parse().unwrap(),
                    ),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(
                    decoded_test_response(ping, &auth, &ping_packet).await.0,
                    StatusCode::OK
                );
                let result = upload.await.unwrap();
                assert_eq!(
                    decoded_test_response(result, &auth, &packet).await.0,
                    StatusCode::OK
                );
                assert!(
                    started.elapsed()
                        >= Duration::from_secs_f64(size as f64 / (1024.0 * 1024.0) * 0.95)
                );
            }
            assert_eq!(root.parent(), Some(temp_root.as_path()));
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

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
            let root = std::env::temp_dir().join(create_id());
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
            let root = std::env::temp_dir().join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-local".into();
            let original = Peer {
                id: "native-remote".into(),
                name: "旧名称".into(),
                ip: "192.168.1.2".into(),
                port: DEFAULT_PORT,
                last_seen_at: now_millis(),
                supports_groups: false,
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
                supports_groups: true,
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
            assert!(saved.peers[0].peer.supports_groups);
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
            let root = std::env::temp_dir().join(create_id());
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
                supports_groups: false,
            };
            let trusted = TrustedPeer {
                peer: peer.clone(),
                secret: "paired-secret".into(),
                secure: true,
            };
            remember(&registry, &paths, trusted.clone()).await.unwrap();
            {
                let mut runtime = registry.inner.lock().await;
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
        assert_eq!(
            uuid::Uuid::parse_str(&create_native_id())
                .unwrap()
                .get_version_num(),
            4
        );
        let mut legacy_id = "native_1789038628947_12096_997".to_string();
        normalize_native_id(&mut legacy_id);
        assert_eq!(legacy_id, "native_1789038628947_12096_997");
    }

    #[test]
    fn deleting_native_device_revokes_pairing_and_manual_scan_requires_new_approval() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root);
            let removed = Peer {
                id: "native-removed".into(),
                name: "移除设备".into(),
                ip: "192.168.1.5".into(),
                port: DEFAULT_PORT,
                last_seen_at: now_millis(),
                supports_groups: false,
            };
            let kept = Peer {
                id: "native-kept".into(),
                name: "保留设备".into(),
                ip: "192.168.1.6".into(),
                port: DEFAULT_PORT,
                last_seen_at: now_millis(),
                supports_groups: false,
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
                .any(|peer| peer["id"] == removed.id && peer["paired"] == false));
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
        runtime.config.peers[0].secure = true;
        runtime.config.peers[0].secret.clear();
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
            let root = std::env::temp_dir().join(create_id());
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
            let root = std::env::temp_dir().join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root.join("receiver"));
            tokio::fs::create_dir_all(&root).await.unwrap();
            let first_content = vec![b'a'; attachments::FILE_CHUNK_BYTES as usize * 2 + 3];
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
            // 配对完成时设备记录与会话已经建立，测试也走同一初始化路径。
            let trusted = receiver
                .inner
                .lock()
                .await
                .native
                .authenticate("native-sender")
                .unwrap();
            remember(&receiver, &paths, trusted).await.unwrap();
            let receiver_session = read_array::<LanShareSession>(&paths.lan_share_files.sessions)
                .unwrap()
                .into_iter()
                .find(|session| session.device_id == "native-sender")
                .unwrap();
            let resumed_upload_id = transfer_upload_id(
                "native-sender",
                "direct",
                "batch_message",
                &files[0].id,
            );
            let resumed_chunk = &first_content[..attachments::FILE_CHUNK_BYTES as usize];
            attachments::upload_chunk(
                Full::new(Bytes::copy_from_slice(resumed_chunk)),
                &receiver,
                &paths,
                &receiver_session.id,
                &resumed_upload_id,
                &files[0].name,
                0,
                3,
                0,
                first_content.len() as u64,
                attachments::FILE_CHUNK_BYTES,
                &format!(
                    "bytes 0-{}/{}",
                    attachments::FILE_CHUNK_BYTES - 1,
                    first_content.len()
                ),
                &format!("{:x}", Sha256::digest(resumed_chunk)),
            )
            .await
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let records = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
            use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
            let dropped_ack = Arc::new(AtomicBool::new(false));
            let active = Arc::new(AtomicUsize::new(0));
            let maximum = Arc::new(AtomicUsize::new(0));
            let server_dropped_ack = dropped_ack.clone();
            let server_active = active.clone();
            let server_maximum = maximum.clone();
            let server_records = records.clone();
            let server_registry = receiver.clone();
            let server_paths = paths.clone();
            let server = tauri::async_runtime::spawn(async move {
                let mut connections = tokio::task::JoinSet::new();
                loop {
                    let (socket, address) = listener.accept().await.unwrap();
                    let records = server_records.clone();
                    let registry = server_registry.clone();
                    let paths = server_paths.clone();
                    let dropped_ack = server_dropped_ack.clone();
                    let active = server_active.clone();
                    let maximum = server_maximum.clone();
                    connections.spawn(async move {
                        let service = service_fn(move |request: Request<Incoming>| {
                            let records = records.clone();
                            let registry = registry.clone();
                            let paths = paths.clone();
                            let dropped_ack = dropped_ack.clone();
                            let active = active.clone();
                            let maximum = maximum.clone();
                            async move {
                                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                                maximum.fetch_max(current, Ordering::SeqCst);
                                assert_eq!(request.uri().path(), "/api/native/secure");
                                assert!(request.uri().query().is_none());
                                assert!(request.headers().get("authorization").is_none());
                                let (parts, body) = request.into_parts();
                                let body = body.collect().await.unwrap().to_bytes();
                                let auth = TransportAuth {
                                    sender: "native-sender".into(),
                                    receiver: "native-receiver".into(),
                                    secret: "fixture-secret".into(),
                                    pairing_id: String::new(),
                                };
                                let (metadata, _) = transport::unpack(
                                    transport::decrypt(&auth.secret, &auth.aad(), &body).unwrap(),
                                )
                                .unwrap();
                                let file_chunk = metadata["operation"] == "file-chunk";
                                tokio::time::sleep(Duration::from_millis(40)).await;
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
                                active.fetch_sub(1, Ordering::SeqCst);
                                // 模拟文件已经接收成功，但确认响应在途中丢失。
                                if file_chunk && !dropped_ack.swap(true, Ordering::SeqCst) {
                                    return Err(std::io::Error::new(
                                        ErrorKind::ConnectionReset,
                                        "测试丢失确认响应",
                                    ));
                                }
                                Ok::<_, std::io::Error>(response)
                            }
                        });
                        http1::Builder::new()
                            .serve_connection(TokioIo::new(socket), service)
                            .await
                            .ok();
                    });
                    while connections.try_join_next().is_some() {}
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
                max_concurrent_files: 2,
            });
            let started = tokio::time::Instant::now();
            for _attempt in 0..2 {
                assert!(deliver(
                    &registry,
                    &paths,
                    "native-receiver",
                    "三个附件",
                    &files,
                    "batch_message",
                    None
                )
                .await
                .unwrap());
            }
            assert!(started.elapsed() >= Duration::from_millis(950));
            server.abort();
            let _ = server.await;
            assert!(dropped_ack.load(Ordering::SeqCst));
            assert_eq!(maximum.load(Ordering::SeqCst), 2);
            let records = records.lock().await;
            assert_eq!(records.len(), 9);
            let audit_auth = TransportAuth {
                sender: "native-sender".into(),
                receiver: "native-receiver".into(),
                secret: "fixture-secret".into(),
                pairing_id: String::new(),
            };
            let mut resumed_chunks = Vec::new();
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
                let (metadata, _) = transport::unpack(
                    transport::decrypt(&audit_auth.secret, &audit_auth.aad(), body).unwrap(),
                )
                .unwrap();
                if metadata["operation"] == "file-chunk"
                    && metadata["payload"]["uploadId"] == resumed_upload_id
                {
                    resumed_chunks.push(metadata["payload"]["chunkIndex"].as_u64().unwrap());
                }
            }
            assert!(!resumed_chunks.is_empty());
            assert!(resumed_chunks.iter().all(|index| *index > 0));
            let uploaded: Vec<LanShareFile> = read_array(&paths.lan_share_files.files).unwrap();
            assert_eq!(uploaded.len(), 3);
            assert_eq!(
                tokio::fs::read(
                    &uploaded
                        .iter()
                        .find(|file| file.name == "first.txt")
                        .unwrap()
                        .path
                )
                .await
                .unwrap(),
                first_content
            );
            assert_eq!(
                tokio::fs::read(
                    &uploaded
                        .iter()
                        .find(|file| file.name == "second.png")
                        .unwrap()
                        .path
                )
                .await
                .unwrap(),
                b"second-file"
            );
            assert!(tokio::fs::read(
                &uploaded
                    .iter()
                    .find(|file| file.name == "empty.txt")
                    .unwrap()
                    .path
            )
            .await
            .unwrap()
            .is_empty());
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
    fn legacy_broadcast_settings_are_discarded_without_losing_pairing() {
        let legacy: Config = serde_json::from_value(json!({
            "id": "native-self", "name": "本机", "auto_discovery": true,
            "discovery_configured": true, "hidden": true,
            "peers": [{ "peer": { "id": "native-known", "name": "对方", "ip": "192.168.1.9", "port": DEFAULT_PORT }, "secret": "do-not-expose", "secure": true }]
        })).unwrap();
        let mut runtime = NativeRuntime::default();
        runtime.config = legacy;
        assert!(runtime.authenticate("native-known").is_some());
        assert!(runtime.config.hidden);
        assert!(runtime.config.remote_deleted_peers.is_empty());
        assert!(runtime.config.revoked_peers.is_empty());
        assert!(!runtime.snapshot().to_string().contains("do-not-expose"));
        let saved = serde_json::to_value(&runtime.config).unwrap();
        assert!(saved.get("auto_discovery").is_none());
        assert!(saved.get("discovery_configured").is_none());
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

    #[test]
    fn remote_deleted_state_is_persisted_and_repair_clears_it() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-local".into();
            let peer = Peer {
                id: "native-remote".into(),
                ip: "127.0.0.1".into(),
                port: DEFAULT_PORT,
                ..Peer::default()
            };
            remember(
                &registry,
                &paths,
                TrustedPeer {
                    peer: peer.clone(),
                    secret: "old-secret".into(),
                    secure: true,
                },
            )
            .await
            .unwrap();
            process_secure_operation(
                &registry,
                &paths,
                None,
                &peer,
                "device-removed",
                json!({}),
                Bytes::new(),
            )
            .await
            .unwrap();
            assert!(registry.inner.lock().await.native.authenticate(&peer.id).is_none());
            assert_eq!(
                registry.inner.lock().await.native.snapshot()["peers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["id"] == peer.id)
                    .unwrap()["remoteDeleted"],
                true
            );
            assert!(ensure_send_allowed(&registry, &peer.id).await.is_err());
            let saved: Config = serde_json::from_slice(
                &tokio::fs::read(&paths.lan_share_files.config)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(saved.remote_deleted_peers, vec![peer.id.clone()]);

            remember(
                &registry,
                &paths,
                TrustedPeer {
                    peer: peer.clone(),
                    secret: "new-secret".into(),
                    secure: true,
                },
            )
            .await
            .unwrap();
            assert!(ensure_send_allowed(&registry, &peer.id).await.is_ok());
            assert!(registry.inner.lock().await.native.authenticate(&peer.id).is_some());
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn revoked_pairing_returns_device_removed_marker_after_offline_delete() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-local".into();
            let peer = Peer {
                id: "native-remote".into(),
                ip: "127.0.0.1".into(),
                port: DEFAULT_PORT,
                ..Peer::default()
            };
            remember(
                &registry,
                &paths,
                TrustedPeer {
                    peer: peer.clone(),
                    secret: "revoked-secret".into(),
                    secure: true,
                },
            )
            .await
            .unwrap();
            {
                let mut runtime = registry.inner.lock().await;
                runtime.native.forget(&paths, &peer.id).await.unwrap();
            }
            let auth = TransportAuth {
                sender: peer.id,
                receiver: "native-local".into(),
                secret: "revoked-secret".into(),
                pairing_id: String::new(),
            };
            let packet = encrypted_test_packet(&auth, "ping", json!({}));
            let response = handle_secure_request(
                encrypted_test_request(&auth, &packet),
                &registry,
                &paths,
                None,
                "127.0.0.1:12345".parse().unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(response.status(), StatusCode::GONE);
            assert_eq!(
                response
                    .headers()
                    .get("x-lan-device-removed")
                    .and_then(|value| value.to_str().ok()),
                Some("true")
            );
            let wrong_auth = TransportAuth {
                secret: "wrong-secret".into(),
                ..auth
            };
            let wrong_packet = encrypted_test_packet(&wrong_auth, "ping", json!({}));
            let wrong_response = handle_secure_request(
                encrypted_test_request(&wrong_auth, &wrong_packet),
                &registry,
                &paths,
                None,
                "127.0.0.1:12345".parse().unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(wrong_response.status(), StatusCode::UNAUTHORIZED);
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn native_info_advertises_group_support() {
        tauri::async_runtime::block_on(async {
            let registry = LanShareServerRegistry::new();
            registry.inner.lock().await.native.config.id = "native-local".into();
            let paths = crate::core::paths::resolve_app_paths(
                &std::env::temp_dir().join(create_id()),
            );
            let info = process_secure_operation(
                &registry,
                &paths,
                None,
                &Peer::default(),
                "info",
                json!({}),
                Bytes::new(),
            )
            .await
            .unwrap();
            assert_eq!(info["supportsGroups"], true);
        });
    }

    #[test]
    fn native_group_payload_creates_a_local_group_session() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = crate::core::paths::resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            let owner = Peer {
                id: "native-owner".into(),
                name: "群主设备".into(),
                ip: "127.0.0.1".into(),
                port: DEFAULT_PORT,
                supports_groups: true,
                ..Peer::default()
            };
            let group = LanShareGroup {
                id: "group-1".into(),
                name: "测试群聊".into(),
                invite_code: "invite-1".into(),
                owner_device_id: owner.id.clone(),
                owner_device_name: owner.name.clone(),
                message_visibility: "afterJoin".into(),
                created_at: now_millis(),
                updated_at: now_millis(),
                members: vec![LanShareGroupMember {
                    device_id: "native-member".into(),
                    device_name: "成员设备".into(),
                    ip: "127.0.0.2".into(),
                    joined_at: now_millis(),
                    last_seen_at: now_millis(),
                    active: true,
                }],
            };
            let session = peer_session(
                &registry,
                &paths,
                &owner,
                "",
                Some(&json!(group)),
            )
            .await
            .unwrap();
            assert_eq!(session.mode, "group");
            assert_eq!(session.group_id, "group-1");
            let groups: Vec<LanShareGroup> = read_array(&paths.lan_share_files.groups).unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].members.len(), 1);
            let sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions).unwrap();
            assert_eq!(sessions.iter().filter(|item| item.group_id == "group-1").count(), 2);
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }
}

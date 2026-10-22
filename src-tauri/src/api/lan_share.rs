use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use base64::Engine;
use bytes::Bytes;
use futures_util::{FutureExt, SinkExt, StreamExt, TryStreamExt};
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::header::{
    HeaderValue, CONNECTION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE,
    SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use qrcode::render::svg;
use qrcode::QrCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::io::{ErrorKind, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::protocol::{Message, Role};
use tokio_tungstenite::WebSocketStream;
use tokio_util::io::ReaderStream;
use url::Url;
use zip::write::SimpleFileOptions;

type BoxBody = http_body_util::combinators::BoxBody<Bytes, std::io::Error>;

mod attachments;
mod native;
mod transfer;
pub use attachments::{clipboard_files, discard_uploads};
pub use native::{connect_peer, respond_pairing};

pub const DEFAULT_PORT: u16 = 17631;
pub const EVENT_STATE_CHANGED: &str = "lan-share:state-changed";
pub const EVENT_MESSAGE_CREATED: &str = "lan-share:message-created";
pub const EVENT_DEVICES_CHANGED: &str = "lan-share:devices-changed";
pub const EVENT_NATIVE_CHANGED: &str = "lan-share:native-changed";
pub const EVENT_TRANSFER_PROGRESS: &str = "lan-share:transfer-progress";

#[derive(Clone)]
pub struct LanShareServerRegistry {
    inner: Arc<Mutex<LanShareRuntime>>,
    storage: Arc<Mutex<()>>,
    uploads: Arc<Mutex<HashMap<String, std::sync::Weak<Mutex<()>>>>>,
    transfer: Arc<transfer::Control>,
}

pub struct LanShareRuntime {
    native: native::NativeRuntime,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
    maintenance: Option<tauri::async_runtime::JoinHandle<()>>,
    token: String,
    access_url: String,
    lan_ip: String,
    port: u16,
    active_sessions: HashMap<String, String>,
    clients: HashMap<String, mpsc::UnboundedSender<WsOutbound>>,
    connections: Vec<tauri::async_runtime::JoinHandle<()>>,
}

#[derive(Clone, Debug)]
pub struct WsOutbound {
    payload: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareFile {
    pub id: String,
    #[serde(default)]
    pub session_id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub updated_at: u64,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareDevice {
    pub id: String,
    pub name: String,
    pub auto_name: String,
    pub user_agent: String,
    pub ip: String,
    pub first_seen_at: u64,
    pub last_seen_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareSession {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default = "default_session_mode")]
    pub mode: String,
    #[serde(default)]
    pub group_id: String,
    #[serde(default)]
    pub group_name: String,
    #[serde(default = "default_message_visibility")]
    pub message_visibility: String,
    #[serde(default)]
    pub joined_at: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareGroupMember {
    pub device_id: String,
    pub device_name: String,
    pub ip: String,
    pub joined_at: u64,
    pub last_seen_at: u64,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareGroup {
    pub id: String,
    pub name: String,
    pub invite_code: String,
    #[serde(default)]
    pub owner_device_id: String,
    #[serde(default)]
    pub owner_device_name: String,
    #[serde(default = "default_message_visibility")]
    pub message_visibility: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub members: Vec<LanShareGroupMember>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareMessage {
    pub id: String,
    pub session_id: String,
    pub device_id: String,
    pub device_name: String,
    pub direction: String,
    pub message_type: String,
    #[serde(default)]
    pub attachments: Vec<attachments::Attachment>,
    pub content: String,
    pub created_at: u64,
    pub delivered: bool,
    pub read: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareDownload {
    pub id: String,
    pub file_id: String,
    pub file_name: String,
    pub device_id: String,
    pub device_name: String,
    pub ip: String,
    pub created_at: u64,
}

impl LanShareServerRegistry {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(())),
            uploads: Arc::new(Mutex::new(HashMap::new())),
            transfer: Arc::new(transfer::Control::default()),
            inner: Arc::new(Mutex::new(LanShareRuntime {
                native: native::NativeRuntime::default(),
                handle: None,
                maintenance: None,
                token: String::new(),
                access_url: String::new(),
                lan_ip: String::new(),
                port: DEFAULT_PORT,
                active_sessions: HashMap::new(),
                clients: HashMap::new(),
                connections: Vec::new(),
            })),
        }
    }
}

pub fn create_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn number_value(value: Option<&Value>, fallback: u64) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(fallback)
}

pub fn string_array_value(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| string_value(Some(item)))
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn default_session_mode() -> String {
    "direct".to_string()
}

fn default_message_visibility() -> String {
    "all".to_string()
}

fn normalize_message_visibility(value: &str) -> String {
    match value.trim() {
        "afterJoin" | "recent10" | "all" => value.trim().to_string(),
        _ => default_message_visibility(),
    }
}

pub fn normalize_device_name(value: &str) -> String {
    value.trim().chars().take(40).collect()
}

fn normalize_group_name(value: &str) -> String {
    let name = value.trim().chars().take(40).collect::<String>();

    if name.is_empty() {
        "未命名群聊".to_string()
    } else {
        name
    }
}

pub fn auto_device_name(user_agent: &str, ip: &str) -> String {
    let lower = user_agent.to_lowercase();
    let device = if lower.contains("iphone") {
        "iPhone"
    } else if lower.contains("ipad") {
        "iPad"
    } else if lower.contains("android") {
        "Android"
    } else {
        "Device"
    };
    let browser = if lower.contains("edg/") || lower.contains("edge/") {
        "Edge"
    } else if lower.contains("chrome/") || lower.contains("crios/") {
        "Chrome"
    } else if lower.contains("safari/") {
        "Safari"
    } else {
        "Browser"
    };

    format!("{} {} · {}", device, browser, ip)
}

pub fn lan_share_response(data: Value) -> Value {
    json!({
        "status": "success",
        "data": data,
        "message": ""
    })
}

pub fn read_array<T>(path: &str) -> Result<Vec<T>, ManagerError>
where
    T: for<'de> Deserialize<'de>,
{
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    Ok(serde_json::from_str(&content)?)
}

pub async fn write_json(path: &str, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )
    .await?;
    Ok(())
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn format_file_size(size: u64) -> String {
    let size = size as f64;

    if size >= 1024.0 * 1024.0 * 1024.0 {
        return format!("{:.2} GB", size / 1024.0 / 1024.0 / 1024.0);
    }
    if size >= 1024.0 * 1024.0 {
        return format!("{:.2} MB", size / 1024.0 / 1024.0);
    }
    if size >= 1024.0 {
        return format!("{:.1} KB", size / 1024.0);
    }

    format!("{} B", size as u64)
}

pub fn create_file_id(path: &str) -> String {
    create_stable_id(&format!("lan-file:{path}"))
}

pub fn device_id_from_ip(ip: &str) -> String {
    create_stable_id(&format!("lan-device:{}", ip.trim()))
}

fn create_stable_id(value: &str) -> String {
    use sha1::{Digest, Sha1};
    // 复用已有 SHA-1 依赖，按 UUID v5 规范计算稳定身份，避免扫描时重复创建记录。
    let mut hasher = Sha1::new();
    hasher.update(uuid::Uuid::NAMESPACE_URL.as_bytes());
    hasher.update(value.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hasher.finalize()[..16]);
    uuid::Builder::from_sha1_bytes(bytes)
        .into_uuid()
        .to_string()
}

fn create_session_file_id(session_id: &str, path: &str) -> String {
    if session_id.is_empty() {
        return create_file_id(path);
    }

    create_stable_id(&format!("lan-session-file:{session_id}:{path}"))
}

fn create_invite_code() -> String {
    let value = uuid::Uuid::new_v4().simple().to_string();

    value[..8].to_uppercase()
}

fn group_invite_url(access_url: &str, invite_code: &str) -> String {
    if access_url.is_empty() || invite_code.is_empty() {
        return String::new();
    }

    match Url::parse(access_url) {
        Ok(mut url) => {
            url.query_pairs_mut().append_pair("invite", invite_code);
            url.to_string()
        }
        Err(_) => String::new(),
    }
}

pub async fn file_payload(path: &str, session_id: &str) -> Result<LanShareFile, ManagerError> {
    let target = PathBuf::from(path);
    let metadata = tokio::fs::metadata(&target).await?;
    let updated_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_else(now_millis);

    Ok(LanShareFile {
        id: create_session_file_id(session_id, path),
        session_id: session_id.to_string(),
        path: path.to_string(),
        name: file_name(&target),
        size: metadata.len(),
        mime_type: mime_guess::from_path(&target)
            .first_or_octet_stream()
            .essence_str()
            .to_string(),
        updated_at,
        enabled: true,
    })
}

pub async fn add_files(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let session_id = string_value(payload.get("sessionId"));
    let session = read_array::<LanShareSession>(&paths.lan_share_files.sessions)?
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| ManagerError::System("请先选择有效会话。".into()))?;
    native::ensure_send_allowed(registry, &session.device_id).await?;
    let files = attachments::prepare(registry, paths, &session.id, &payload).await?;
    if files.is_empty() {
        return get_state(registry, paths).await;
    }
    let message_id = create_id();
    let content = if files.len() == 1 {
        format!(
            "共享文件：{}（{}）",
            files[0].name,
            format_file_size(files[0].size)
        )
    } else {
        format!("共享了 {} 个文件", files.len())
    };
    let native_delivery = native::deliver(
        registry,
        paths,
        &session.device_id,
        &content,
        &files,
        &message_id,
        None,
    )
    .await?;
    let sender = registry
        .inner
        .lock()
        .await
        .clients
        .get(&session.device_id)
        .cloned();
    let stored = append_message_with_attachments(
        registry,
        paths,
        &session.device_id,
        &session.id,
        "desktop-to-mobile",
        &content,
        native_delivery || sender.is_some(),
        &files,
        Some(&message_id),
    )
    .await?;
    attachments::publish(registry, paths, &files).await?;
    if let Some(sender) = sender {
        let _ = send_stored_message_to_client(&sender, &stored);
    }
    if session.mode == "group" {
        for (_, sender) in group_online_targets(registry, paths, &session.group_id).await? {
            let _ = send_stored_message_to_client(&sender, &stored);
        }
    }
    get_state(registry, paths).await
}

pub async fn get_state(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<Value, ManagerError> {
    let (files, devices, mut sessions, groups, messages) = {
        let _storage = registry.storage.lock().await;
        let files = read_array::<LanShareFile>(&paths.lan_share_files.files)?;
        let devices = read_array::<LanShareDevice>(&paths.lan_share_files.devices)?;
        let mut sessions = read_array::<LanShareSession>(&paths.lan_share_files.sessions)?;
        let groups = read_array::<LanShareGroup>(&paths.lan_share_files.groups)?;
        let messages = read_array::<LanShareMessage>(&paths.lan_share_files.messages)?;

        fill_session_ips(paths, &devices, &mut sessions).await?;
        (files, devices, sessions, groups, messages)
    };
    sort_sessions(&mut sessions);
    // 删除设备时默认保留历史会话，但这些历史不应继续占用悬浮窗未读数。
    // 设备重新连接并重新出现在设备清单后，历史消息仍可正常查看。
    let unread_counts = session_unread_counts(&sessions, &messages, &devices);
    let lan_ips = available_lan_ips()?;
    let mut runtime = registry.inner.lock().await;
    refresh_access_address(&mut runtime, &lan_ips);
    let online_device_ids = runtime.clients.keys().cloned().collect::<Vec<_>>();
    let access_url = runtime.access_url.clone();
    let devices = devices
        .into_iter()
        .map(|device| {
            json!({
                "id": device.id,
                "name": device.name,
                "autoName": device.auto_name,
                "userAgent": device.user_agent,
                "ip": device.ip,
                "firstSeenAt": device.first_seen_at,
                "lastSeenAt": device.last_seen_at,
                "online": online_device_ids.contains(&device.id) || runtime.native.online(&device.id),
                "native": device.user_agent == "MonkeyThief/Desktop"
            })
        })
        .collect::<Vec<_>>();
    let groups = groups
        .into_iter()
        .map(|group| {
            let invite_url = group_invite_url(&access_url, &group.invite_code);
            let qr_svg = if invite_url.is_empty() {
                String::new()
            } else {
                qr_svg(&invite_url).unwrap_or_default()
            };
            let owner_id = if group.owner_device_id.is_empty() {
                runtime.native.config.id.clone()
            } else {
                group.owner_device_id.clone()
            };
            let owner_name = if group.owner_device_name.is_empty() {
                if runtime.native.config.name.is_empty() {
                    "本机".to_string()
                } else {
                    runtime.native.config.name.clone()
                }
            } else {
                group.owner_device_name.clone()
            };
            let mut members = Vec::with_capacity(group.members.len() + 1);
            if !owner_id.is_empty() {
                members.push(json!({
                    "deviceId": owner_id,
                    "deviceName": owner_name,
                    "ip": runtime.lan_ip,
                    "joinedAt": group.created_at,
                    "lastSeenAt": now_millis(),
                    "active": true,
                    "owner": true,
                    "online": true
                }));
            }
            members.extend(group.members.into_iter().filter_map(|member| {
                if member.device_id == group.owner_device_id && !group.owner_device_id.is_empty() {
                    return None;
                }
                Some(json!({
                    "deviceId": member.device_id,
                    "deviceName": member.device_name,
                    "ip": member.ip,
                    "joinedAt": member.joined_at,
                    "lastSeenAt": member.last_seen_at,
                    "active": member.active,
                    "owner": false,
                    "online": online_device_ids.contains(&member.device_id)
                        || runtime.native.online(&member.device_id)
                }))
            }));

            json!({
                "id": group.id,
                "name": group.name,
                "inviteCode": group.invite_code,
                "ownerDeviceId": owner_id,
                "ownerDeviceName": owner_name,
                "inviteUrl": invite_url,
                "qrSvg": qr_svg,
                "messageVisibility": group.message_visibility,
                "createdAt": group.created_at,
                "updatedAt": group.updated_at,
                "members": members
            })
        })
        .collect::<Vec<_>>();
    let qr_svg = if runtime.access_url.is_empty() {
        String::new()
    } else {
        qr_svg(&runtime.access_url)?
    };

    Ok(lan_share_response(json!({
        "service": {
            "running": runtime.handle.is_some(),
            "accessUrl": runtime.access_url,
            "qrSvg": qr_svg,
            "lanIp": runtime.lan_ip,
            "lanIps": lan_ips,
            "port": runtime.port,
            "onlineDevices": runtime.clients.len() + runtime.native.online_count()
        },
        "native": runtime.native.snapshot(),
        "files": files,
        "devices": devices,
        "sessions": sessions
            .iter()
            .map(|session| {
                session_state_payload(
                    session,
                    unread_counts.get(&session.id).copied().unwrap_or(0),
                )
            })
            .collect::<Vec<_>>(),
        "unreadCounts": unread_counts,
        "totalUnread": unread_counts.values().sum::<usize>(),
        "groups": groups,
        "messages": messages
    })))
}

pub async fn remove_file(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let file_id = string_value(payload.get("fileId"));

    {
        let _storage = registry.storage.lock().await;
        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
        files.retain(|file| file.id != file_id);
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
    }

    get_state(registry, paths).await
}

pub async fn remove_files(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let file_ids = string_array_value(payload.get("fileIds"));

    if file_ids.is_empty() {
        return Err(ManagerError::System("请选择要删除的文件。".to_string()));
    }

    {
        let _storage = registry.storage.lock().await;
        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
        files.retain(|file| !file_ids.contains(&file.id));
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
    }

    get_state(registry, paths).await
}

pub async fn refresh_files(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<Value, ManagerError> {
    let _storage = registry.storage.lock().await;
    let files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
    let mut next_files = Vec::new();

    for file in files {
        if let Ok(next_file) = file_payload(&file.path, &file.session_id).await {
            next_files.push(LanShareFile {
                name: next_file.name,
                size: next_file.size,
                mime_type: next_file.mime_type,
                updated_at: next_file.updated_at,
                enabled: true,
                ..file
            });
        } else {
            next_files.push(LanShareFile {
                enabled: false,
                ..file
            });
        }
    }

    write_json(&paths.lan_share_files.files, &json!(next_files)).await?;
    Ok(lan_share_response(json!(next_files)))
}

pub async fn list_messages(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let keyword = string_value(payload.get("keyword")).to_lowercase();
    let device_id = string_value(payload.get("deviceId"));
    let session_id = string_value(payload.get("sessionId"));
    let from = number_value(payload.get("from"), 0);
    let to = number_value(payload.get("to"), 0);
    let mut messages: Vec<LanShareMessage> = {
        let _storage = registry.storage.lock().await;
        read_array(&paths.lan_share_files.messages)?
    };

    let session_scope_ids = if session_id.is_empty() {
        Vec::new()
    } else {
        visible_session_ids_for_session(paths, &session_id)?
    };

    messages.retain(|message| {
        let matches_keyword =
            keyword.is_empty() || message.content.to_lowercase().contains(&keyword);
        let matches_device = device_id.is_empty() || message.device_id == device_id;
        let matches_session =
            session_id.is_empty() || session_scope_ids.contains(&message.session_id);
        let matches_time = message.created_at >= from && (to == 0 || message.created_at <= to);

        matches_keyword && matches_device && matches_session && matches_time
    });
    messages.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    Ok(lan_share_response(json!(messages)))
}

pub async fn delete_message(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let message_id = string_value(payload.get("messageId"));

    {
        let _storage = registry.storage.lock().await;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        messages.retain(|message| message.id != message_id);
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
    }

    list_messages(registry, paths, json!({})).await
}

pub async fn delete_messages(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let message_ids = string_array_value(payload.get("messageIds"));
    let session_id = string_value(payload.get("sessionId"));

    if message_ids.is_empty() {
        return Err(ManagerError::System("请选择要删除的消息。".to_string()));
    }

    {
        let _storage = registry.storage.lock().await;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        messages.retain(|message| !message_ids.contains(&message.id));
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
    }

    list_messages(registry, paths, json!({ "sessionId": session_id, "to": 0 })).await
}

pub async fn clear_session(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let session_id = string_value(payload.get("sessionId"));

    {
        let _storage = registry.storage.lock().await;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
        messages.retain(|message| message.session_id != session_id);
        files.retain(|file| file.session_id != session_id);
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
    }

    list_messages(registry, paths, json!({})).await
}

pub async fn delete_session(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let session_id = string_value(payload.get("sessionId"));

    if session_id.is_empty() {
        return Err(ManagerError::System("会话不能为空。".to_string()));
    }

    {
        let _storage = registry.storage.lock().await;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;

        sessions.retain(|session| session.id != session_id);
        messages.retain(|message| message.session_id != session_id);
        files.retain(|file| file.session_id != session_id);

        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
    }

    {
        let mut runtime = registry.inner.lock().await;
        runtime
            .active_sessions
            .retain(|_, active_session_id| active_session_id != &session_id);
    }

    get_state(registry, paths).await
}

pub async fn delete_device_history(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let device_id = string_value(payload.get("deviceId"));

    {
        let _storage = registry.storage.lock().await;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
        let session_ids = sessions
            .iter()
            .filter(|session| session.device_id == device_id)
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        messages.retain(|message| message.device_id != device_id);
        sessions.retain(|session| session.device_id != device_id);
        files.retain(|file| !session_ids.contains(&file.session_id));

        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
    }

    get_state(registry, paths).await
}

pub async fn delete_device(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let device_id = string_value(payload.get("deviceId"));
    if device_id.trim().is_empty() {
        return Err(ManagerError::System("设备标识不能为空。".into()));
    }
    let delete_history = payload
        .get("deleteHistory")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let trusted_peer = registry
        .inner
        .lock()
        .await
        .native
        .config
        .peers
        .iter()
        .find(|trusted| trusted.peer.id == device_id)
        .cloned();
    if let Some(trusted) = trusted_peer {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            native::notify_peer_removed(registry, &trusted),
        )
        .await;
    }
    {
        let _storage = registry.storage.lock().await;
        let mut devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices)?;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
        let mut groups: Vec<LanShareGroup> = read_array(&paths.lan_share_files.groups)?;
        {
            let mut runtime = registry.inner.lock().await;
            if runtime.native.config.peers.iter().any(|trusted| trusted.peer.id == device_id) {
                runtime.native.forget(paths, &device_id).await?;
            }
        }
        let direct_session_ids = sessions
            .iter()
            .filter(|session| session.device_id == device_id && session.mode != "group")
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        let device_group_sessions = sessions
            .iter()
            .filter(|session| session.device_id == device_id && session.mode == "group")
            .cloned()
            .collect::<Vec<_>>();
        for removed in device_group_sessions {
            let host_id = sessions
                .iter()
                .find(|session| {
                    session.mode == "group"
                        && session.group_id == removed.group_id
                        && session.device_id.is_empty()
                })
                .map(|session| session.id.clone());
            if let Some(host_id) = host_id {
                for message in messages
                    .iter_mut()
                    .filter(|message| message.session_id == removed.id)
                {
                    message.session_id = host_id.clone();
                }
                for file in files
                    .iter_mut()
                    .filter(|file| file.session_id == removed.id)
                {
                    file.session_id = host_id.clone();
                }
            } else if let Some(session) =
                sessions.iter_mut().find(|session| session.id == removed.id)
            {
                session.device_id.clear();
                session.device_name = "群聊".into();
                session.ip.clear();
            }
        }
        devices.retain(|device| device.id != device_id);
        sessions.retain(|session| {
            session.device_id != device_id || (!delete_history && session.mode != "group")
        });
        if delete_history {
            messages.retain(|message| !direct_session_ids.contains(&message.session_id));
            let removed_files = files
                .iter()
                .filter(|file| direct_session_ids.contains(&file.session_id))
                .cloned()
                .collect::<Vec<_>>();
            files.retain(|file| !direct_session_ids.contains(&file.session_id));
            // 只删除快传管理的副本；仍被其他会话引用的文件继续保留。
            let uploads = Path::new(&paths.lan_share_dir).join("uploads");
            let uploads = match tokio::fs::canonicalize(&uploads).await {
                Ok(directory) => Some(directory),
                Err(error) if error.kind() == ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            if let Some(directory) = uploads {
                for file in &removed_files {
                    if files.iter().any(|other| other.path == file.path) {
                        continue;
                    }
                    let path = match tokio::fs::canonicalize(&file.path).await {
                        Ok(path) => path,
                        Err(error) if error.kind() == ErrorKind::NotFound => continue,
                        Err(error) => return Err(error.into()),
                    };
                    if path.parent() == Some(directory.as_path()) {
                        tokio::fs::remove_file(path).await?;
                    }
                }
                // 清除该设备所有单聊会话的分块回执和未完成接收内容。
                let mut entries = tokio::fs::read_dir(&directory).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if !name.starts_with(".lan-upload-") || !name.ends_with(".json") {
                        continue;
                    }
                    let metadata: Value = serde_json::from_slice(&tokio::fs::read(&path).await?)?;
                    if !direct_session_ids
                        .iter()
                        .any(|id| metadata["sessionId"] == *id)
                    {
                        continue;
                    }
                    let part = path.with_extension("part");
                    match tokio::fs::remove_file(part).await {
                        Ok(()) => {}
                        Err(error) if error.kind() == ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                    }
                    tokio::fs::remove_file(path).await?;
                }
            }
            let mut downloads: Vec<LanShareDownload> =
                read_array(&paths.lan_share_files.downloads)?;
            downloads.retain(|download| {
                download.device_id != device_id
                    && !removed_files.iter().any(|file| file.id == download.file_id)
            });
            write_json(&paths.lan_share_files.downloads, &json!(downloads)).await?;
        }
        for group in &mut groups {
            let count = group.members.len();
            group.members.retain(|member| member.device_id != device_id);
            if group.members.len() != count {
                group.updated_at = now_millis();
            }
        }
        let mut runtime = registry.inner.lock().await;
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        write_json(&paths.lan_share_files.groups, &json!(groups)).await?;
        write_json(&paths.lan_share_files.devices, &json!(devices)).await?;
        runtime.active_sessions.remove(&device_id);
        if let Some(sender) = runtime.clients.remove(&device_id) {
            let _ = sender.send(WsOutbound {
                payload: json!({ "type": "deviceRemoved" }),
            });
        }
        for sender in runtime.clients.values() {
            let _ = sender.send(WsOutbound {
                payload: json!({ "type": "groupsChanged" }),
            });
        }
    }
    broadcast_files_changed(registry).await?;
    get_state(registry, paths).await
}

pub async fn create_group(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let mut device_ids = Vec::new();
    for device_id in string_array_value(payload.get("deviceIds")) {
        if !device_id.is_empty() && !device_ids.contains(&device_id) {
            device_ids.push(device_id);
        }
    }
    if device_ids.len() < 2 {
        return Err(ManagerError::System(
            "群聊至少需要 3 人（包含本机），请至少邀请两台设备。".into(),
        ));
    }
    let name = normalize_group_name(&string_value(payload.get("name")));
    let message_visibility =
        normalize_message_visibility(&string_value(payload.get("messageVisibility")));
    let now = now_millis();
    let group_id = create_id();
    let (owner_device_id, owner_device_name) = {
        let runtime = registry.inner.lock().await;
        (
            if runtime.native.config.id.is_empty() {
                create_id()
            } else {
                runtime.native.config.id.clone()
            },
            if runtime.native.config.name.is_empty() {
                "本机".to_string()
            } else {
                runtime.native.config.name.clone()
            },
        )
    };
    let session = LanShareSession {
        id: create_id(),
        device_id: String::new(),
        device_name: "群聊".to_string(),
        ip: String::new(),
        mode: "group".to_string(),
        group_id: group_id.clone(),
        group_name: name.clone(),
        message_visibility: message_visibility.clone(),
        joined_at: now,
        created_at: now,
        updated_at: now,
    };
    let mut group = LanShareGroup {
        id: group_id,
        name,
        invite_code: create_invite_code(),
        owner_device_id,
        owner_device_name,
        message_visibility,
        created_at: now,
        updated_at: now,
        members: Vec::new(),
    };

    let native_group_capabilities = {
        let runtime = registry.inner.lock().await;
        runtime
            .native
            .config
            .peers
            .iter()
            .map(|trusted| (trusted.peer.id.clone(), trusted.peer.supports_groups))
            .collect::<HashMap<_, _>>()
    };

    {
        let _storage = registry.storage.lock().await;
        let devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices)?;
        let mut invited = Vec::new();
        for device_id in &device_ids {
            let device = devices
                .iter()
                .find(|device| &device.id == device_id)
                .ok_or_else(|| ManagerError::System("邀请的设备已不存在，请重新选择。".into()))?;
            if device.user_agent == "MonkeyThief/Desktop"
                && !native_group_capabilities
                    .get(&device.id)
                    .copied()
                    .unwrap_or(false)
            {
                return Err(ManagerError::System(
                    "该客户端版本不支持群聊，请先更新对方设备后再邀请。".into(),
                ));
            }
            if !invited
                .iter()
                .any(|current: &&LanShareDevice| current.id == device.id)
            {
                invited.push(device);
            }
        }
        if invited.len() < 2 {
            return Err(ManagerError::System(
                "群聊至少需要 3 人（包含本机），请至少邀请两台有效设备。".into(),
            ));
        }
        let mut groups: Vec<LanShareGroup> = read_array(&paths.lan_share_files.groups)?;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;

        sessions.insert(0, session.clone());
        for device in invited {
            upsert_group_session(&mut sessions, &mut group, device, &device.ip);
        }
        groups.insert(0, group.clone());
        sort_sessions(&mut sessions);
        write_json(&paths.lan_share_files.groups, &json!(groups)).await?;
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
    }

    {
        let runtime = registry.inner.lock().await;
        for member in &group.members {
            if let Some(sender) = runtime.clients.get(&member.device_id) {
                let _ = sender.send(WsOutbound {
                    payload: json!({ "type": "groupsChanged" }),
                });
            }
        }
    }
    for member in &group.members {
        if native_group_capabilities
            .get(&member.device_id)
            .copied()
            .unwrap_or(false)
        {
            let _ = native::sync_group_to_peer(registry, paths, &group, &member.device_id).await;
        }
    }
    let mut state = get_state(registry, paths).await?;
    state["data"]["currentSession"] = json!(session);
    Ok(state)
}

pub async fn update_group(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let group_id = string_value(payload.get("groupId"));
    let name = normalize_group_name(&string_value(payload.get("name")));
    let message_visibility =
        normalize_message_visibility(&string_value(payload.get("messageVisibility")));

    if group_id.is_empty() {
        return Err(ManagerError::System("群聊不能为空。".to_string()));
    }

    {
        let _storage = registry.storage.lock().await;
        let mut groups: Vec<LanShareGroup> = read_array(&paths.lan_share_files.groups)?;
        let Some(group) = groups.iter_mut().find(|group| group.id == group_id) else {
            return Err(ManagerError::System("群聊不存在。".to_string()));
        };

        group.name = name.clone();
        group.message_visibility = message_visibility.clone();
        group.updated_at = now_millis();

        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        for session in sessions
            .iter_mut()
            .filter(|session| session.group_id == group_id)
        {
            session.group_name = name.clone();
            session.message_visibility = message_visibility.clone();
            session.updated_at = now_millis();
        }

        write_json(&paths.lan_share_files.groups, &json!(groups)).await?;
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
    }

    get_state(registry, paths).await
}

pub async fn remove_group_member(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let group_id = string_value(payload.get("groupId"));
    let device_id = string_value(payload.get("deviceId"));

    if group_id.is_empty() || device_id.is_empty() {
        return Err(ManagerError::System("群聊和成员不能为空。".to_string()));
    }

    let own_device_id = registry.inner.lock().await.native.config.id.clone();

    {
        let _storage = registry.storage.lock().await;
        let mut groups: Vec<LanShareGroup> = read_array(&paths.lan_share_files.groups)?;
        let Some(group) = groups.iter_mut().find(|group| group.id == group_id) else {
            return Err(ManagerError::System("群聊不存在。".to_string()));
        };

        let owner_id = if group.owner_device_id.is_empty() {
            own_device_id.as_str()
        } else {
            group.owner_device_id.as_str()
        };
        if device_id == owner_id {
            return Err(ManagerError::System(
                "群主不能移出群聊，只能解散群聊。".to_string(),
            ));
        }

        group.members.retain(|member| member.device_id != device_id);
        group.updated_at = now_millis();

        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        sessions.retain(|session| {
            !(session.group_id == group_id
                && session.device_id == device_id
                && session.mode == "group")
        });

        write_json(&paths.lan_share_files.groups, &json!(groups)).await?;
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
    }

    get_state(registry, paths).await
}

pub async fn delete_group(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let group_id = string_value(payload.get("groupId"));

    if group_id.is_empty() {
        return Err(ManagerError::System("群聊不能为空。".to_string()));
    }

    {
        let _storage = registry.storage.lock().await;
        let mut groups: Vec<LanShareGroup> = read_array(&paths.lan_share_files.groups)?;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
        let group_session_ids = sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        groups.retain(|group| group.id != group_id);
        sessions.retain(|session| session.group_id != group_id);
        messages.retain(|message| !group_session_ids.contains(&message.session_id));
        files.retain(|file| !group_session_ids.contains(&file.session_id));

        write_json(&paths.lan_share_files.groups, &json!(groups)).await?;
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
    }

    get_state(registry, paths).await
}

pub async fn clear_group_messages(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let group_id = string_value(payload.get("groupId"));

    if group_id.is_empty() {
        return Err(ManagerError::System("群聊不能为空。".to_string()));
    }

    {
        let _storage = registry.storage.lock().await;
        let sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let group_session_ids = sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        messages.retain(|message| !group_session_ids.contains(&message.session_id));
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
    }

    get_state(registry, paths).await
}

pub async fn save_file(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let file_id = string_value(payload.get("fileId"));
    let target_path = string_value(payload.get("targetPath"));
    if target_path.is_empty() {
        return Err(ManagerError::System("请选择文件保存路径。".into()));
    }
    let file = {
        let _storage = registry.storage.lock().await;
        read_array::<LanShareFile>(&paths.lan_share_files.files)?
            .into_iter()
            .find(|file| file.id == file_id && file.enabled)
            .ok_or_else(|| ManagerError::System("文件记录不存在或已移除。".into()))?
    };
    let source_path = tokio::fs::canonicalize(&file.path).await?;
    // 不能将副本写回源文件，避免复制时截断已接收的附件。
    if Path::new(&target_path).exists()
        && tokio::fs::canonicalize(&target_path).await? == source_path
    {
        return Err(ManagerError::System(
            "保存位置与原文件相同，请选择其他位置。".into(),
        ));
    }
    tokio::fs::copy(&source_path, &target_path).await?;
    Ok(lan_share_response(json!({ "targetPath": target_path })))
}

pub async fn export_files_zip(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let file_ids = string_array_value(payload.get("fileIds"));
    let target_path = string_value(payload.get("targetPath"));

    if file_ids.is_empty() {
        return Err(ManagerError::System("请选择要打包的文件。".to_string()));
    }
    if target_path.is_empty() {
        return Err(ManagerError::System("请选择 ZIP 保存路径。".to_string()));
    }

    let selected_files = {
        let _storage = registry.storage.lock().await;
        let files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;

        file_ids
            .iter()
            .filter_map(|file_id| files.iter().find(|file| &file.id == file_id).cloned())
            .collect::<Vec<_>>()
    };

    if selected_files.len() != file_ids.len() {
        return Err(ManagerError::System("部分文件记录不存在。".to_string()));
    }

    write_files_zip(&selected_files, &target_path)?;

    Ok(lan_share_response(json!({
        "targetPath": target_path,
        "count": selected_files.len()
    })))
}

fn write_files_zip(files: &[LanShareFile], target_path: &str) -> Result<(), ManagerError> {
    if let Some(parent) = Path::new(target_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let target = std::fs::File::create(target_path)?;
    let mut archive = zip::ZipWriter::new(target);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for file in files {
        let source_path = Path::new(&file.path);

        if !source_path.is_file() {
            return Err(ManagerError::System(format!(
                "文件不存在或不可读取：{}",
                file.name
            )));
        }

        archive
            .start_file(safe_zip_entry_name(&file.name), options)
            .map_err(|error| ManagerError::System(error.to_string()))?;
        let content = std::fs::read(source_path)?;
        archive.write_all(&content)?;
    }

    archive
        .finish()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(())
}

fn safe_zip_entry_name(name: &str) -> String {
    let value = name.replace('\\', "_").replace('/', "_").trim().to_string();

    if value.is_empty() {
        "file".to_string()
    } else {
        value
    }
}

fn is_valid_token(expected: &str, actual: &str) -> bool {
    !expected.is_empty() && expected == actual
}

fn create_token() -> Result<String, ManagerError> {
    let mut bytes = [0u8; 24];

    getrandom::getrandom(&mut bytes).map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn qr_svg(access_url: &str) -> Result<String, ManagerError> {
    let code = QrCode::new(access_url.as_bytes())
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(code.render::<svg::Color>().min_dimensions(220, 220).build())
}

fn available_lan_ips() -> Result<Vec<String>, ManagerError> {
    let interfaces = local_ip_address::list_afinet_netifas()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let mut ips = Vec::new();
    // 保留网卡顺序，同一地址只展示一次。
    for (_, ip) in interfaces {
        if let IpAddr::V4(ipv4) = ip {
            if is_lan_ipv4(ipv4) {
                let address = ipv4.to_string();
                if !ips.contains(&address) {
                    ips.push(address);
                }
            }
        }
    }
    Ok(ips)
}

fn refresh_access_address(runtime: &mut LanShareRuntime, lan_ips: &[String]) -> bool {
    if runtime.handle.is_none() {
        return false;
    }
    // 手选地址仍可用时保持原选择；地址失效或断网时不再发布旧链接。
    let lan_ip = if lan_ips.contains(&runtime.lan_ip) {
        runtime.lan_ip.clone()
    } else {
        lan_ips.first().cloned().unwrap_or_default()
    };
    let access_url = if lan_ip.is_empty() {
        String::new()
    } else {
        format!("http://{}:{}/?token={}", lan_ip, runtime.port, runtime.token)
    };
    if runtime.lan_ip == lan_ip && runtime.access_url == access_url {
        return false;
    }
    runtime.lan_ip = lan_ip;
    runtime.access_url = access_url;
    true
}

pub async fn set_access_ip(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let lan_ip = string_value(payload.get("lanIp"));
    if !available_lan_ips()?.contains(&lan_ip) {
        return Err(ManagerError::System(
            "所选 IP 已不可用，请刷新网络设置后重试。".into(),
        ));
    }
    let mut runtime = registry.inner.lock().await;
    if runtime.handle.is_none() {
        return Err(ManagerError::System("请先上线设备快传。".into()));
    }
    // 服务监听全部网卡；保留端口和访问凭据，让已连接设备继续传输。
    runtime.lan_ip = lan_ip.clone();
    runtime.access_url = format!(
        "http://{}:{}/?token={}",
        lan_ip, runtime.port, runtime.token
    );
    drop(runtime);
    emit_state_changed(&app, registry, paths).await?;
    get_state(registry, paths).await
}

fn is_lan_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private()
}

fn safe_header_file_name(name: &str) -> String {
    let value = name
        .chars()
        .map(|ch| if ch == '\r' || ch == '\n' { '_' } else { ch })
        .collect::<String>()
        .trim()
        .to_string();

    if value.is_empty() {
        "download".to_string()
    } else {
        value
    }
}

fn is_previewable_mime(mime_type: &str) -> bool {
    let value = mime_type.trim().to_lowercase();

    value.starts_with("image/")
        || value.starts_with("video/")
        || value.starts_with("audio/")
        || value.starts_with("text/")
        || value == "application/pdf"
        || value == "application/json"
        || value == "application/xml"
        || value == "application/javascript"
        || value == "application/x-javascript"
        || value == "application/xhtml+xml"
        || value == "image/svg+xml"
}

fn is_text_preview_extension(name: &str) -> bool {
    let lower_name = name.trim().to_lowercase();
    let extensions = [
        ".txt", ".md", ".json", ".xml", ".csv", ".log", ".js", ".ts", ".css", ".html", ".vue",
        ".rs", ".py", ".java", ".c", ".cpp", ".h", ".go", ".yaml", ".yml", ".toml", ".ini",
        ".conf", ".sql", ".sh", ".ps1",
    ];

    extensions
        .iter()
        .any(|extension| lower_name.ends_with(extension))
}

fn is_previewable_file(file: &LanShareFile) -> bool {
    is_previewable_mime(&file.mime_type) || is_text_preview_extension(&file.name)
}

fn mobile_page_html() -> String {
    concat!(
        r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover" />
  <title>设备快传</title>
  <style>
"##,
        include_str!("../../../src/styles/typography.css"),
        r##"
    :root {
      --bg: #edf3f8;
      --bg-gradient: radial-gradient(circle at 20% 0%, #f8fbff 0%, #edf3f8 42%, #e5edf5 100%);
      --panel: #ffffff;
      --panel-soft: #f7fafc;
      --panel-glass: rgba(255, 255, 255, 0.94);
      --line: #d8e3ee;
      --line-strong: #bed0e2;
      --text: #142033;
      --muted: #64748b;
      --primary: #2563eb;
      --primary-hover: #1d4ed8;
      --primary-soft: #eff6ff;
      --primary-border: #bfdbfe;
      --success: #10b981;
      --success-soft: #ecfdf5;
      --warning: #f59e0b;
      --danger: #ef4444;
      --danger-soft: #fef2f2;
      --shadow-sm: 0 2px 6px rgba(15, 23, 42, 0.04);
      --shadow-md: 0 8px 24px rgba(15, 23, 42, 0.06);
      --shadow-lg: 0 16px 36px rgba(15, 23, 42, 0.1);
      --radius-sm: 8px;
      --radius-md: 12px;
      --radius-lg: 16px;
    }

    [data-theme="dark"] {
      --bg: #0b1120;
      --bg-gradient: radial-gradient(circle at 20% 0%, #172554 0%, #0b1120 50%, #030712 100%);
      --panel: #131d32;
      --panel-soft: #17233d;
      --panel-glass: rgba(19, 29, 50, 0.94);
      --line: #1e2e4a;
      --line-strong: #2a3e63;
      --text: #f8fafc;
      --muted: #94a3b8;
      --primary: #3b82f6;
      --primary-hover: #60a5fa;
      --primary-soft: rgba(59, 130, 246, 0.15);
      --primary-border: rgba(59, 130, 246, 0.3);
      --success: #34d399;
      --success-soft: rgba(52, 211, 153, 0.14);
      --warning: #fbbf24;
      --danger: #f87171;
      --danger-soft: rgba(248, 113, 113, 0.14);
      --shadow-sm: 0 2px 6px rgba(0, 0, 0, 0.25);
      --shadow-md: 0 8px 24px rgba(0, 0, 0, 0.35);
      --shadow-lg: 0 16px 36px rgba(0, 0, 0, 0.45);
    }

    * { box-sizing: border-box; -webkit-tap-highlight-color: transparent; }
    html, body { height: 100%; margin: 0; padding: 0; }
    body {
      min-height: 100vh;
      background: var(--bg-gradient);
      color: var(--text);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
      font-size: var(--font-size-base);
      line-height: 1.5;
      overflow: hidden;
    }

    button, input, textarea, select { font: inherit; color: inherit; }
    button { cursor: pointer; border: none; background: transparent; }

    /* Shell Layout (satisfies test asserts) */
    .page-shell {
      position: relative;
      display: flex;
      height: 100vh;
      min-height: 0;
      flex-direction: column;
      overflow: hidden;
    }
    .page-head {
      position: sticky;
      top: 0;
      z-index: 20;
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 12px 16px;
      background: var(--panel-glass);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border-bottom: 1px solid var(--line);
      box-shadow: var(--shadow-sm);
    }
    .page-main {
      display: flex;
      min-height: 0;
      flex: 1;
      flex-direction: column;
      gap: 12px;
      overflow-y: auto;
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
      padding: 12px 14px;
    }
    .content-shell {
      display: flex;
      min-width: 0;
      min-height: 0;
      flex: 1;
      flex-direction: column;
      gap: 12px;
    }
    .content-detail {
      display: flex;
      min-width: 0;
      min-height: 0;
      flex: 1;
      flex-direction: column;
      gap: 12px;
    }
    .content-detail.hidden {
      display: none;
    }

    /* Floating Toast */
    .mobile-toast {
      position: fixed;
      top: 20px;
      left: 50%;
      transform: translateX(-50%) translateY(-24px);
      padding: 9px 18px;
      border-radius: 999px;
      background: rgba(15, 23, 42, 0.88);
      backdrop-filter: blur(12px);
      color: #ffffff;
      font-size: 13.5px;
      font-weight: 500;
      box-shadow: 0 10px 28px rgba(0, 0, 0, 0.22);
      z-index: 1000;
      opacity: 0;
      pointer-events: none;
      transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
      white-space: nowrap;
    }
    .mobile-toast.show {
      transform: translateX(-50%) translateY(0);
      opacity: 1;
    }

    /* Brand Header Components (4.png) */
    .brand-row {
      display: flex;
      align-items: center;
      gap: 10px;
      min-width: 0;
    }
    .app-logo {
      width: 38px;
      height: 38px;
      border-radius: 10px;
      background: linear-gradient(135deg, #3b82f6 0%, #1d4ed8 100%);
      display: flex;
      align-items: center;
      justify-content: center;
      color: #ffffff;
      flex-shrink: 0;
      box-shadow: 0 4px 12px rgba(37, 99, 235, 0.3);
    }
    .brand-info {
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .brand-title {
      margin: 0;
      font-size: 17px;
      font-weight: 700;
      color: var(--text);
      letter-spacing: -0.01em;
      line-height: 1.25;
    }
    .brand-status {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 11.5px;
      color: var(--muted);
      margin-top: 2px;
    }
    .status-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--warning);
      flex-shrink: 0;
      transition: background 0.2s ease;
    }
    .status-dot.is-connected {
      background: var(--success);
      box-shadow: 0 0 6px rgba(16, 185, 129, 0.4);
    }
    .status-dot.is-disconnected {
      background: var(--danger);
    }

    .header-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      flex-shrink: 0;
    }
    .header-pill-btn {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      height: 32px;
      padding: 0 12px;
      border-radius: 999px;
      border: 1px solid var(--line);
      background: var(--panel);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
      transition: all 0.15s ease;
    }
    .header-pill-btn:hover {
      border-color: var(--line-strong);
      background: var(--panel-soft);
    }
    .header-pill-btn.primary {
      border-color: var(--primary);
      background: var(--primary);
      color: #ffffff;
      box-shadow: 0 2px 8px rgba(37, 99, 235, 0.25);
    }
    .header-pill-btn.primary:hover {
      background: var(--primary-hover);
    }

    /* Sub-header (Back + Title + Action) */
    .sub-header {
      position: sticky;
      top: 0;
      z-index: 20;
      display: flex;
      flex: none;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 10px 14px;
      background: var(--panel-glass);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border-bottom: 1px solid var(--line);
      box-shadow: var(--shadow-sm);
    }
    .nav-back-btn {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      height: 34px;
      padding: 0 10px 0 6px;
      border-radius: 8px;
      color: var(--primary);
      font-size: 14px;
      font-weight: 500;
      transition: background 0.15s ease;
    }
    .nav-back-btn:hover {
      background: var(--primary-soft);
    }
    .sub-header-center {
      display: flex;
      flex-direction: column;
      align-items: center;
      text-align: center;
      min-width: 0;
      flex: 1;
    }
    .sub-header-title {
      margin: 0;
      font-size: 16px;
      font-weight: 700;
      color: var(--text);
      line-height: 1.25;
    }
    .sub-header-desc {
      font-size: 11px;
      color: var(--muted);
      margin-top: 2px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 220px;
    }
    .nav-action-btn, .icon-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 34px;
      height: 34px;
      border-radius: 8px;
      color: var(--muted);
      border: 1px solid var(--line);
      background: var(--panel);
      transition: all 0.15s ease;
    }
    .nav-action-btn:hover, .icon-button:hover {
      color: var(--text);
      border-color: var(--line-strong);
    }
    .sub-header-placeholder {
      width: 34px;
    }

    /* Card Component */
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: var(--radius-md);
      box-shadow: var(--shadow-sm);
      overflow: hidden;
      flex-shrink: 0;
    }
    .card-head {
      display: flex;
      min-height: 44px;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 10px 14px;
      border-bottom: 1px solid var(--line);
      background: var(--panel);
    }
    .card-head [data-emphasis] {
      color: var(--text);
      font-size: 14.5px;
      font-weight: 700;
    }
    .card-head span:not([data-emphasis]) {
      color: var(--muted);
      font-size: 11.5px;
    }
    .card-head-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 11px 14px;
      border-bottom: 1px solid var(--line);
      background: var(--panel);
    }
    .card-title {
      font-size: 14.5px;
      font-weight: 700;
      color: var(--text);
      line-height: 1.2;
    }
    .card-subtitle {
      font-size: 11.5px;
      color: var(--muted);
    }
    .title-with-badge {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .count-badge {
      display: inline-flex;
      align-items: center;
      padding: 2px 7px;
      border-radius: 999px;
      background: var(--primary-soft);
      color: var(--primary);
      font-size: 11px;
      font-weight: 600;
    }
    .type-badge {
      display: inline-flex;
      align-items: center;
      padding: 2px 8px;
      border-radius: 4px;
      background: var(--panel-soft);
      border: 1px solid var(--line);
      color: var(--muted);
      font-size: 11px;
      font-weight: 500;
    }

    /* Device Card Row (4.png) */
    .device-card {
      flex: none;
    }
    .device-row {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 10px 14px;
    }
    .field-input {
      width: 100%;
      height: 36px;
      padding: 0 12px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--line-strong);
      background: var(--panel);
      color: var(--text);
      font-size: var(--font-size-sm);
      outline: none;
      transition: all 0.15s ease;
    }
    .field-input:focus {
      border-color: var(--primary);
      box-shadow: 0 0 0 3px var(--primary-soft);
    }

    /* View Tabs & Mode Tabs */
    .view-tabs {
      display: flex;
      flex: none;
      padding: 3px;
      background: var(--panel-soft);
      border: 1px solid var(--line);
      border-radius: 10px;
      gap: 4px;
    }
    .view-tab, .mode-tab {
      flex: 1;
      height: 34px;
      display: flex;
      align-items: center;
      justify-content: center;
      border-radius: 8px;
      font-size: var(--font-size-sm);
      font-weight: 500;
      color: var(--muted);
      background: transparent;
      transition: all 0.18s ease;
    }
    .view-tab.active, .mode-tab.active {
      background: var(--panel);
      color: var(--primary);
      font-weight: 600;
      box-shadow: 0 2px 6px rgba(0, 0, 0, 0.05);
    }
    .view-panel {
      display: none;
      min-height: 0;
      flex: 1;
      flex-direction: column;
    }
    .view-panel.active {
      display: flex;
    }

    /* Buttons */
    .text-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 5px;
      height: 36px;
      padding: 0 16px;
      border-radius: var(--radius-sm);
      font-size: var(--font-size-sm);
      font-weight: 600;
      border: 1px solid var(--line);
      background: var(--panel-soft);
      color: var(--text);
      transition: all 0.15s ease;
      white-space: nowrap;
    }
    .text-button:hover {
      border-color: var(--line-strong);
    }
    .primary-button {
      border-color: var(--primary);
      background: var(--primary);
      color: #ffffff;
      box-shadow: 0 2px 8px rgba(37, 99, 235, 0.22);
    }
    .primary-button:hover {
      background: var(--primary-hover);
    }
    .text-button.group-back {
      display: none;
      flex: none;
      min-width: 52px;
      padding: 0 10px;
    }
    .text-button.group-back.active {
      display: inline-flex;
    }
    .btn-xs {
      display: inline-flex;
      align-items: center;
      height: 26px;
      padding: 0 10px;
      border-radius: 6px;
      font-size: 11.5px;
      font-weight: 500;
      border: 1px solid var(--line);
      background: var(--panel);
      color: var(--text);
      text-decoration: none;
      transition: all 0.15s ease;
    }
    .btn-xs:hover {
      border-color: var(--line-strong);
      background: var(--panel-soft);
    }
    .btn-xs.primary {
      background: var(--primary);
      border-color: var(--primary);
      color: #ffffff;
    }

    /* Group List View */
    .group-list-view {
      display: none;
      min-height: 0;
      flex: 1;
      flex-direction: column;
    }
    .group-list-view.active {
      display: flex;
    }
    .group-join-row {
      display: flex;
      gap: 8px;
      padding: 10px 14px;
      background: var(--panel-soft);
      border-bottom: 1px solid var(--line);
    }
    .group-session-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding: 10px 14px;
      overflow-y: auto;
      flex: 1;
    }
    .group-session-item {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 10px 12px;
      border-radius: 8px;
      border: 1px solid var(--line);
      background: var(--panel);
      text-align: left;
    }
    .group-session-main {
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .group-session-name {
      font-size: var(--font-size-sm);
      font-weight: 600;
      color: var(--text);
    }
    .group-session-meta {
      font-size: 11px;
      color: var(--muted);
      margin-top: 2px;
    }

    /* Chat Messages Viewport (4.png) */
    .message-list {
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 12px;
      padding: 14px;
      min-height: 0;
      overflow-y: auto;
      overscroll-behavior: contain;
    }
    .chat-empty-state {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 36px 20px;
      text-align: center;
      user-select: none;
    }
    .empty-illustration {
      width: 140px;
      height: 110px;
      margin-bottom: 14px;
    }
    .empty-title {
      margin: 0;
      font-size: 16px;
      font-weight: 700;
      color: var(--text);
    }
    .empty-desc {
      margin: 6px 0 0;
      font-size: 12.5px;
      color: var(--muted);
      max-width: 250px;
      line-height: 1.5;
    }

    .message {
      display: flex;
      flex-direction: column;
      gap: 6px;
      max-width: 82%;
      min-width: 90px;
      padding: 10px 13px;
      border-radius: 12px;
      background: var(--panel-soft);
      border: 1px solid var(--line);
      color: var(--text);
      font-size: var(--font-size-sm);
      line-height: 1.5;
      text-align: left;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.03);
      cursor: pointer;
      word-break: break-word;
      transition: transform 0.1s ease;
    }
    .message:active {
      transform: scale(0.99);
    }
    .message.me {
      align-self: flex-end;
      background: linear-gradient(135deg, #eff6ff 0%, #e0f2fe 100%);
      border-color: #bfdbfe;
      box-shadow: 0 2px 10px rgba(37, 99, 235, 0.08);
    }
    [data-theme="dark"] .message.me {
      background: linear-gradient(135deg, #1e3a8a 0%, #1e293b 100%);
      border-color: #2563eb;
    }
    .message small {
      align-self: flex-end;
      font-size: 11px;
      color: var(--muted);
    }

    .message-attachment {
      display: flex;
      flex-direction: column;
      gap: 6px;
      margin-top: 6px;
      padding: 8px;
      border-radius: 8px;
      background: var(--panel);
      border: 1px solid var(--line);
    }
    .message-attachment img, .message-attachment video {
      display: block;
      width: 100%;
      max-height: 220px;
      object-fit: contain;
      border-radius: 6px;
    }
    .message-attachment audio {
      width: 100%;
    }
    .message-attachment-name {
      font-size: 11.5px;
      color: var(--muted);
      overflow-wrap: anywhere;
    }
    .message-attachment-actions {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 6px;
    }

    /* Composer Bar (4.png) */
    .pending-attachments {
      display: flex;
      gap: 6px;
      padding: 6px 12px 0;
      overflow-x: auto;
    }
    .pending-attachment {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      max-width: 200px;
      padding: 4px 8px;
      border-radius: 6px;
      background: var(--primary-soft);
      border: 1px solid var(--primary-border);
      color: var(--primary);
      font-size: 11.5px;
      flex-shrink: 0;
    }
    .pending-attachment span {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .pending-remove-btn {
      color: var(--muted);
      font-weight: 700;
      font-size: 14px;
      cursor: pointer;
    }
    .composer {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      border-top: 1px solid var(--line);
      background: var(--panel);
    }
    .btn-icon-label {
      display: inline-flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      width: 44px;
      height: 38px;
      border-radius: 8px;
      border: 1px solid var(--line);
      background: var(--panel);
      color: var(--text);
      font-size: 10.5px;
      font-weight: 500;
      gap: 2px;
      flex-shrink: 0;
      box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
    }
    .btn-icon-label:hover {
      border-color: var(--line-strong);
      background: var(--panel-soft);
    }
    .composer-input {
      flex: 1;
      min-width: 0;
      min-height: 38px;
      max-height: 100px;
      padding: 8px 12px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--line-strong);
      background: var(--panel);
      color: var(--text);
      font-size: var(--font-size-sm);
      line-height: 1.4;
      resize: none;
      outline: none;
      transition: all 0.15s ease;
    }
    .composer-input:focus {
      border-color: var(--primary);
      box-shadow: 0 0 0 3px var(--primary-soft);
    }

    /* Screen 2: Received Content Page (3.png) */
    .files-container-wrap {
      display: flex;
      flex-direction: column;
      gap: 12px;
      flex: 1;
      min-height: 0;
      overflow-y: auto;
      padding: 12px;
    }
    .files-list-card {
      max-height: 240px;
      display: flex;
      flex-direction: column;
    }
    .file-list {
      flex: 1;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
    }
    .file-row-item {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 10px 14px;
      border-bottom: 1px solid var(--line);
      background: var(--panel);
      cursor: pointer;
      transition: background 0.15s ease;
    }
    .file-row-item:last-child {
      border-bottom: none;
    }
    .file-row-item:hover, .file-row-item.is-active {
      background: var(--primary-soft);
    }
    .file-row-icon {
      width: 34px;
      height: 34px;
      border-radius: 8px;
      display: grid;
      place-items: center;
      flex-shrink: 0;
      color: #ffffff;
    }
    .file-row-icon.txt { background: #3b82f6; }
    .file-row-icon.img { background: #8b5cf6; }
    .file-row-icon.zip { background: #f59e0b; }
    .file-row-icon.media { background: #06b6d4; }
    .file-row-icon.other { background: #64748b; }
    .file-row-info {
      flex: 1;
      min-width: 0;
      display: flex;
      flex-direction: column;
    }
    .file-row-name {
      font-size: var(--font-size-sm);
      font-weight: 600;
      color: var(--text);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .file-row-meta {
      font-size: 11px;
      color: var(--muted);
      margin-top: 2px;
    }
    .file-row-actions {
      display: flex;
      align-items: center;
      gap: 6px;
      flex-shrink: 0;
    }
    .empty-placeholder {
      padding: 24px 14px;
      text-align: center;
      color: var(--muted);
      font-size: var(--font-size-sm);
    }

    /* Active File Preview Card (3.png) */
    .preview-card {
      flex: 1;
      display: flex;
      flex-direction: column;
      min-height: 240px;
    }
    .preview-title-row {
      display: flex;
      align-items: center;
      gap: 8px;
      min-width: 0;
    }
    .preview-actions {
      display: flex;
      align-items: center;
      gap: 6px;
      flex-shrink: 0;
    }
    .preview-content-box {
      flex: 1;
      display: flex;
      min-height: 0;
      overflow: auto;
      background: var(--panel-soft);
      position: relative;
    }
    .code-preview-viewer {
      display: flex;
      width: 100%;
      min-height: 100%;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 12px;
      line-height: 1.65;
    }
    .code-gutter {
      padding: 10px 8px 10px 10px;
      text-align: right;
      color: var(--muted);
      opacity: 0.6;
      border-right: 1px solid var(--line);
      background: rgba(0, 0, 0, 0.02);
      user-select: none;
      flex-shrink: 0;
    }
    .code-content {
      margin: 0;
      padding: 10px 14px;
      flex: 1;
      overflow-x: auto;
      white-space: pre-wrap;
      word-break: break-word;
      color: var(--text);
    }
    .preview-media-box {
      width: 100%;
      height: 100%;
      display: grid;
      place-items: center;
      padding: 14px;
    }
    .preview-media-box img, .preview-media-box video {
      max-width: 100%;
      max-height: 320px;
      object-fit: contain;
      border-radius: 8px;
      box-shadow: var(--shadow-sm);
    }

    /* Screen 3 & 4: Settings (5.png & 6.png) */
    .settings-layer {
      position: fixed;
      inset: 0;
      z-index: 50;
      display: none;
      flex-direction: column;
      background: var(--bg-gradient);
      overflow: hidden;
    }
    .settings-layer.active {
      display: flex;
    }
    .settings-scroll {
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 14px;
      padding: 12px 14px 88px;
      overflow-y: auto;
      overscroll-behavior: contain;
      -webkit-overflow-scrolling: touch;
    }
    .settings-group {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .settings-group-title {
      margin: 4px 4px 0;
      font-size: 12px;
      font-weight: 600;
      color: var(--muted);
      text-transform: uppercase;
      letter-spacing: 0.02em;
    }
    .settings-card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: var(--radius-md);
      box-shadow: var(--shadow-sm);
      overflow: hidden;
    }
    .settings-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 12px 14px;
      border-bottom: 1px solid var(--line);
    }
    .settings-row:last-child {
      border-bottom: none;
    }
    .settings-row.clickable {
      cursor: pointer;
    }
    .settings-row.clickable:active {
      background: var(--panel-soft);
    }
    .settings-label-box {
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .settings-label {
      font-size: 13.5px;
      font-weight: 500;
      color: var(--text);
    }
    .settings-sublabel {
      font-size: 11px;
      color: var(--muted);
      margin-top: 2px;
    }
    .settings-value {
      font-size: 13px;
      color: var(--muted);
      text-align: right;
    }
    .settings-value.monospace {
      font-family: ui-monospace, SFMono-Regular, monospace;
      font-size: 12px;
    }
    .settings-arrow {
      display: flex;
      align-items: center;
      gap: 4px;
      color: var(--muted);
    }
    .settings-value-hint {
      font-size: 12px;
    }
    .badge-soft {
      display: inline-flex;
      padding: 2px 7px;
      border-radius: 4px;
      background: var(--primary-soft);
      color: var(--primary);
      font-size: 11px;
      font-weight: 600;
    }
    .badge-green {
      display: inline-flex;
      padding: 2px 7px;
      border-radius: 4px;
      background: var(--success-soft);
      color: var(--success);
      font-size: 11px;
      font-weight: 600;
    }

    /* Switch Toggle */
    .switch {
      position: relative;
      display: inline-block;
      width: 44px;
      height: 24px;
      flex-shrink: 0;
    }
    .switch input {
      opacity: 0;
      width: 0;
      height: 0;
    }
    .switch-slider {
      position: absolute;
      cursor: pointer;
      inset: 0;
      background-color: var(--line-strong);
      border-radius: 999px;
      transition: all 0.22s cubic-bezier(0.16, 1, 0.3, 1);
    }
    .switch-slider:before {
      position: absolute;
      content: "";
      height: 18px;
      width: 18px;
      left: 3px;
      bottom: 3px;
      background-color: #ffffff;
      border-radius: 50%;
      box-shadow: 0 2px 5px rgba(0, 0, 0, 0.16);
      transition: all 0.22s cubic-bezier(0.16, 1, 0.3, 1);
    }
    .switch input:checked + .switch-slider {
      background-color: var(--primary);
    }
    .switch input:checked + .switch-slider:before {
      transform: translateX(20px);
    }

    /* Screen 4: Connection Status Card (6.png) */
    .conn-status-card {
      padding: 14px;
      display: flex;
      flex-direction: column;
      gap: 10px;
      background: linear-gradient(135deg, var(--panel) 0%, var(--primary-soft) 100%);
      border-color: var(--primary-border);
    }
    .conn-status-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
    }
    .conn-status-left {
      display: flex;
      flex-direction: column;
    }
    .conn-status-label {
      font-size: 11.5px;
      color: var(--muted);
    }
    .conn-host-ip {
      font-size: 16px;
      font-weight: 700;
      color: var(--text);
      font-family: ui-monospace, SFMono-Regular, monospace;
      margin-top: 2px;
    }
    .conn-status-badge {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      padding: 3px 9px;
      border-radius: 999px;
      background: var(--success-soft);
      color: var(--success);
      font-size: 12px;
      font-weight: 600;
    }
    .badge-dot {
      font-size: 10px;
    }
    .conn-status-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding-top: 8px;
      border-top: 1px solid var(--line);
      font-size: 11.5px;
      color: var(--muted);
    }
    .conn-mode-tag {
      font-weight: 500;
    }

    .settings-select, .settings-number-input {
      height: 32px;
      padding: 0 10px;
      border-radius: 6px;
      border: 1px solid var(--line-strong);
      background: var(--panel);
      color: var(--text);
      font-size: 13px;
      outline: none;
    }
    .settings-number-input {
      width: 90px;
      text-align: right;
    }
    .btn-outline-danger {
      width: 100%;
      height: 40px;
      border: 1px solid var(--danger);
      color: var(--danger);
      border-radius: var(--radius-sm);
      font-size: 13.5px;
      font-weight: 600;
      transition: all 0.15s ease;
    }
    .btn-outline-danger:hover {
      background: var(--danger-soft);
    }
    .conn-save-footer {
      position: fixed;
      bottom: 0;
      left: 0;
      right: 0;
      padding: 10px 14px max(12px, env(safe-area-inset-bottom));
      background: var(--panel-glass);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border-top: 1px solid var(--line);
      box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.05);
      z-index: 60;
    }
    .btn-block {
      width: 100%;
      height: 44px;
      font-size: 15px;
      border-radius: var(--radius-md);
    }

    /* Modal Dialog */
    .preview-dialog {
      position: fixed;
      inset: 0;
      z-index: 100;
      display: none;
      align-items: center;
      justify-content: center;
      padding: 16px;
    }
    .preview-dialog.open {
      display: flex;
    }
    .preview-overlay {
      position: absolute;
      inset: 0;
      background: rgba(15, 23, 42, 0.55);
      backdrop-filter: blur(4px);
    }
    .preview-panel {
      position: relative;
      display: flex;
      width: min(720px, 100%);
      max-height: calc(100vh - 32px);
      flex-direction: column;
      overflow: hidden;
      border: 1px solid var(--line);
      border-radius: var(--radius-md);
      background: var(--panel);
      box-shadow: var(--shadow-lg);
    }
    .preview-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 14px;
      border-bottom: 1px solid var(--line);
    }
    .preview-title {
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .preview-name-text {
      font-size: 14px;
      font-weight: 700;
      color: var(--text);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .preview-meta-text {
      font-size: 11px;
      color: var(--muted);
      margin-top: 2px;
    }
    .preview-close-btn {
      width: 28px;
      height: 28px;
      border-radius: 6px;
      border: 1px solid var(--line);
      display: grid;
      place-items: center;
      font-size: 18px;
      color: var(--muted);
    }
    .preview-body {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      min-height: 240px;
      overflow: auto;
      background: var(--panel-soft);
    }
    .preview-body img, .preview-body video {
      max-width: 100%;
      max-height: 65vh;
    }
    .preview-body pre {
      width: 100%;
      margin: 0;
      padding: 14px;
      color: var(--text);
      font-family: ui-monospace, SFMono-Regular, monospace;
      font-size: 12px;
      line-height: 1.6;
      white-space: pre-wrap;
      word-break: break-word;
    }
    .preview-foot {
      display: flex;
      justify-content: flex-end;
      padding: 10px 14px;
      border-top: 1px solid var(--line);
    }
  </style>
</head>
<body>
  <!-- Floating Toast Notification -->
  <div id="mobileToast" class="mobile-toast" role="status" aria-live="polite"></div>

  <!-- Main Shell Application -->
  <div class="page-shell">
    <!-- Header (4.png) -->
    <header class="page-head">
      <div class="brand-row">
        <div class="app-logo">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>
          </svg>
        </div>
        <div class="brand-info">
          <h1 class="brand-title" data-emphasis>设备快传</h1>
          <div class="brand-status">
            <span id="statusDot" class="status-dot"></span>
            <span id="status">正在连接电脑端服务...</span>
          </div>
        </div>
      </div>
      <div class="header-actions">
        <button id="openSettingsBtn" class="header-pill-btn" type="button" aria-label="设置">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
          <span>设置</span>
        </button>
        <button id="openFilesBtn" class="header-pill-btn primary" type="button" aria-label="获取内容">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>
          </svg>
          <span>获取内容</span>
        </button>
      </div>
    </header>

    <!-- Main Content Area -->
    <main class="page-main">
      <!-- Device Name Card (4.png) -->
      <section class="card device-card">
        <div class="card-head">
          <span data-emphasis>设备名称</span>
          <span>用于电脑端识别</span>
        </div>
        <div class="device-row">
          <input id="deviceName" class="field-input" maxlength="40" placeholder="输入设备名称" />
          <button id="saveDeviceName" class="text-button primary-button" type="button">保存</button>
        </div>
      </section>

      <!-- Content Shell -->
      <section class="content-shell">
        <!-- Direct / Group Mode Switcher (4.png) -->
        <nav class="view-tabs" aria-label="设备快传模式">
          <button class="mode-tab active" data-mode="direct" type="button">单聊</button>
          <button class="mode-tab" data-mode="group" type="button">群聊</button>
        </nav>

        <!-- Group List Section (hidden unless mode is group and no current group) -->
        <section id="groupListView" class="card group-list-view">
          <div class="card-head">
            <span data-emphasis>已加入群聊</span>
            <span>输入群邀请码加入</span>
          </div>
          <div class="group-join-row">
            <input id="inviteCode" class="field-input" maxlength="16" placeholder="输入群邀请码" />
            <button id="joinGroup" class="text-button" type="button">加入</button>
          </div>
          <div id="groups" class="group-session-list"></div>
        </section>

        <!-- Content Detail Section -->
        <section id="detailView" class="content-detail">
          <!-- Detail View Tabs (Messages / Files) -->
          <nav class="view-tabs" aria-label="设备快传详情">
            <button id="backGroups" class="text-button group-back" type="button" title="返回群列表">返回群列表</button>
            <button class="view-tab active" data-view="messages" type="button">消息</button>
            <button class="view-tab" data-view="files" type="button">共享文件</button>
            <button id="leaveGroup" class="text-button group-back" type="button" title="退出群聊">退出</button>
          </nav>

          <!-- Messages View Panel (4.png) -->
          <section id="messagesView" class="card view-panel active">
            <div class="card-head">
              <span data-emphasis id="messagesTitle">消息</span>
              <button id="createSession" class="text-button" type="button">新会话</button>
            </div>

            <!-- Empty State Illustration (4.png) -->
            <div id="chatEmptyState" class="chat-empty-state">
              <div class="empty-illustration">
                <svg viewBox="0 0 160 120" fill="none" xmlns="http://www.w3.org/2000/svg" style="width: 100%; height: 100%;">
                  <defs>
                    <filter id="msgBubbleShadow" x="-20%" y="-20%" width="150%" height="150%">
                      <feDropShadow dx="0" dy="8" stdDeviation="10" flood-color="#2563eb" flood-opacity="0.18"/>
                    </filter>
                    <linearGradient id="bubbleGrad1" x1="20" y1="20" x2="110" y2="90" gradientUnits="userSpaceOnUse">
                      <stop offset="0%" stop-color="#93c5fd" stop-opacity="0.9"/>
                      <stop offset="100%" stop-color="#3b82f6" stop-opacity="0.85"/>
                    </linearGradient>
                    <linearGradient id="bubbleGrad2" x1="50" y1="30" x2="140" y2="100" gradientUnits="userSpaceOnUse">
                      <stop offset="0%" stop-color="#ffffff"/>
                      <stop offset="100%" stop-color="#eff6ff"/>
                    </linearGradient>
                    <linearGradient id="bubbleBorder" x1="50" y1="30" x2="140" y2="100" gradientUnits="userSpaceOnUse">
                      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.9"/>
                      <stop offset="100%" stop-color="#bfdbfe" stop-opacity="0.6"/>
                    </linearGradient>
                  </defs>
                  <!-- Back Bubble -->
                  <g transform="translate(18, 14) rotate(-5)">
                    <rect x="0" y="0" width="76" height="54" rx="14" fill="url(#bubbleGrad1)" opacity="0.65"/>
                    <circle cx="24" cy="27" r="4" fill="#ffffff" opacity="0.9"/>
                    <circle cx="38" cy="27" r="4" fill="#ffffff" opacity="0.9"/>
                    <circle cx="52" cy="27" r="4" fill="#ffffff" opacity="0.9"/>
                  </g>
                  <!-- Front Main Bubble -->
                  <g filter="url(#msgBubbleShadow)">
                    <rect x="42" y="32" width="88" height="62" rx="16" fill="url(#bubbleGrad2)" stroke="url(#bubbleBorder)" stroke-width="1.5"/>
                    <path d="M 60 48 Q 80 48 100 48" stroke="#3b82f6" stroke-width="3.5" stroke-linecap="round" opacity="0.85"/>
                    <path d="M 60 59 Q 74 59 88 59" stroke="#93c5fd" stroke-width="3.5" stroke-linecap="round" opacity="0.75"/>
                    <circle cx="112" cy="72" r="3.5" fill="#2563eb" opacity="0.7"/>
                  </g>
                  <circle cx="138" cy="24" r="3" fill="#60a5fa" opacity="0.8"/>
                  <circle cx="28" cy="80" r="2.5" fill="#93c5fd" opacity="0.6"/>
                  <path d="M 132 40 L 134 44 L 138 46 L 134 48 L 132 52 L 130 48 L 126 46 L 130 44 Z" fill="#38bdf8" opacity="0.75"/>
                </svg>
              </div>
              <h3 class="empty-title">暂无消息</h3>
              <p class="empty-desc">输入文字或点击下方附件，快速传输至已连接设备</p>
            </div>

            <div id="messages" class="message-list" style="display:none;" aria-label="点击消息可复制"></div>
            <div id="pendingAttachments" class="pending-attachments"></div>

            <!-- Composer (4.png) -->
            <div class="composer">
              <button id="chooseAttachments" class="btn-icon-label" type="button" aria-label="添加附件">
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/>
                </svg>
                <span>附件</span>
              </button>
              <input id="attachmentInput" type="file" multiple hidden />
              <textarea id="messageInput" class="composer-input" rows="1" placeholder="输入消息或粘贴图片、文件"></textarea>
              <button id="sendMessage" class="text-button primary-button" type="button">发送</button>
            </div>
          </section>

          <!-- Files View Panel (3.png: 获取内容) -->
          <section id="filesView" class="card view-panel">
            <div class="sub-header">
              <button id="filesBackBtn" class="nav-back-btn" type="button">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="15 18 9 12 15 6"/>
                </svg>
                <span>返回</span>
              </button>
              <div class="sub-header-center">
                <h2 class="sub-header-title" data-emphasis id="filesTitle">获取内容</h2>
                <span id="filesPeerInfo" class="sub-header-desc">来自电脑端 · 局域网服务</span>
              </div>
              <button id="refreshFiles" class="icon-button" type="button" aria-label="刷新">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/>
                </svg>
              </button>
            </div>

            <div class="files-container-wrap">
              <!-- Received Files List Card (3.png) -->
              <section class="card files-list-card">
                <div class="card-head-row">
                  <div class="title-with-badge">
                    <span class="card-title">收到的文件</span>
                    <span id="filesCountBadge" class="count-badge">0 项</span>
                  </div>
                  <span class="card-subtitle">点击文件可在下方预览</span>
                </div>
                <div id="files" class="file-list">
                  <div class="empty-placeholder">正在读取文件列表...</div>
                </div>
              </section>

              <!-- Active File Content Preview Card (3.png) -->
              <section id="activeFilePreviewCard" class="card preview-card">
                <div class="card-head-row">
                  <div class="preview-title-row">
                    <span id="activeFileName" class="card-title">文件预览</span>
                    <span id="activeFileTypeBadge" class="type-badge">就绪</span>
                  </div>
                  <div class="preview-actions">
                    <button id="copyPreviewTextBtn" class="btn-xs" type="button" style="display:none;">复制全文</button>
                    <a id="activeFileDownloadBtn" class="btn-xs primary" download style="display:none;">下载</a>
                  </div>
                </div>
                <div id="activeFileContent" class="preview-content-box">
                  <div class="empty-placeholder">
                    <span>点击上方文件卡片可在此快速预览内容</span>
                  </div>
                </div>
              </section>
            </div>
          </section>
        </section>
      </section>
    </main>
  </div>

  <!-- Screen 3: Settings Layer (5.png) -->
  <div id="settingsPage" class="settings-layer">
    <header class="sub-header">
      <button id="settingsBackBtn" class="nav-back-btn" type="button">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="15 18 9 12 15 6"/>
        </svg>
        <span>返回</span>
      </button>
      <div class="sub-header-center">
        <h2 class="sub-header-title">设置</h2>
        <span class="sub-header-desc">设备快传个性化与连接设置</span>
      </div>
      <div class="sub-header-placeholder"></div>
    </header>

    <div class="settings-scroll">
      <!-- Section: 设备信息 -->
      <section class="settings-group">
        <h3 class="settings-group-title">设备信息</h3>
        <div class="settings-card">
          <div class="settings-row">
            <span class="settings-label">设备名称</span>
            <span id="settingsDeviceNameDisplay" class="settings-value">-</span>
          </div>
          <div class="settings-row">
            <span class="settings-label">设备标识</span>
            <span id="settingsDeviceIdDisplay" class="settings-value monospace">-</span>
          </div>
          <div class="settings-row">
            <span class="settings-label">运行平台</span>
            <span id="settingsPlatformDisplay" class="settings-value">Mobile Web</span>
          </div>
        </div>
      </section>

      <!-- Section: 连接设置 -->
      <section class="settings-group">
        <h3 class="settings-group-title">连接设置</h3>
        <div class="settings-card">
          <div class="settings-row">
            <div class="settings-label-box">
              <span class="settings-label">局域网发现</span>
              <span class="settings-sublabel">允许同一 Wi-Fi 网络内被其它节点自动发现</span>
            </div>
            <label class="switch">
              <input type="checkbox" id="settingLanDiscovery" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
          <div id="openConnSettingsRow" class="settings-row clickable">
            <div class="settings-label-box">
              <span class="settings-label">连接方式</span>
              <span class="settings-sublabel">配置同网访问、端口与连接确认</span>
            </div>
            <div class="settings-arrow">
              <span class="settings-value-hint">已配置</span>
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="9 18 15 12 9 6"/>
              </svg>
            </div>
          </div>
        </div>
      </section>

      <!-- Section: 传输设置 -->
      <section class="settings-group">
        <h3 class="settings-group-title">传输设置</h3>
        <div class="settings-card">
          <div class="settings-row">
            <div class="settings-label-box">
              <span class="settings-label">自动保存文件</span>
              <span class="settings-sublabel">接收到文件后自动触发浏览器保存</span>
            </div>
            <label class="switch">
              <input type="checkbox" id="settingAutoSave" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
        </div>
      </section>

      <!-- Section: 通知与外观 -->
      <section class="settings-group">
        <h3 class="settings-group-title">通知与外观</h3>
        <div class="settings-card">
          <div class="settings-row">
            <div class="settings-label-box">
              <span class="settings-label">接收消息提示</span>
              <span class="settings-sublabel">收到新消息或传输完成时轻触反馈</span>
            </div>
            <label class="switch">
              <input type="checkbox" id="settingSound" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
          <div class="settings-row">
            <div class="settings-label-box">
              <span class="settings-label">深色模式</span>
              <span class="settings-sublabel">开启低眩光暗黑夜间主题</span>
            </div>
            <label class="switch">
              <input type="checkbox" id="settingDarkMode" />
              <span class="switch-slider"></span>
            </label>
          </div>
        </div>
      </section>

      <!-- Section: 安全与权限 -->
      <section class="settings-group">
        <h3 class="settings-group-title">安全与权限</h3>
        <div class="settings-card">
          <div class="settings-row">
            <span class="settings-label">传输加密</span>
            <span class="settings-value badge-green">● 局域网鉴权验证</span>
          </div>
          <div class="settings-row">
            <span class="settings-label">访问令牌状态</span>
            <span class="settings-value">已校验 · 安全有效</span>
          </div>
        </div>
      </section>

      <!-- Section: 关于设备快传 -->
      <section class="settings-group">
        <h3 class="settings-group-title">关于设备快传</h3>
        <div class="settings-card">
          <div class="settings-row">
            <span class="settings-label">版本信息</span>
            <span class="settings-value">v2.0.26</span>
          </div>
          <div class="settings-row">
            <span class="settings-label">直连协议</span>
            <span class="settings-value">WebSocket + HTTP/2</span>
          </div>
          <div class="settings-row">
            <span class="settings-label">运行状态</span>
            <span class="settings-value">局域网极速互传已就绪</span>
          </div>
        </div>
      </section>
    </div>
  </div>

  <!-- Screen 4: Connection Settings Subpage (6.png) -->
  <div id="connSettingsPage" class="settings-layer">
    <header class="sub-header">
      <button id="connSettingsBackBtn" class="nav-back-btn" type="button">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="15 18 9 12 15 6"/>
        </svg>
        <span>返回</span>
      </button>
      <div class="sub-header-center">
        <h2 class="sub-header-title">连接设置</h2>
        <span class="sub-header-desc">配置设备连接与传输相关选项</span>
      </div>
      <div class="sub-header-placeholder"></div>
    </header>

    <div class="settings-scroll">
      <!-- Current Device Status Card (6.png) -->
      <section class="card conn-status-card">
        <div class="conn-status-header">
          <div class="conn-status-left">
            <span class="conn-status-label">当前连接服务</span>
            <span id="connHostIpDisplay" class="conn-host-ip">192.168.1.1:17631</span>
          </div>
          <span class="conn-status-badge">
            <span class="badge-dot">●</span> 已连接
          </span>
        </div>
        <div class="conn-status-footer">
          <span class="conn-mode-tag">局域网直连模式</span>
          <span class="conn-ping">延迟 &lt; 5ms</span>
        </div>
      </section>

      <!-- Switch Options Card (6.png) -->
      <section class="settings-group">
        <h3 class="settings-group-title">连接模式开关</h3>
        <div class="settings-card">
          <div class="settings-row">
            <span class="settings-label">同网访问</span>
            <label class="switch">
              <input type="checkbox" id="connSwitchSameNet" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
          <div class="settings-row">
            <span class="settings-label">自动发现设备</span>
            <label class="switch">
              <input type="checkbox" id="connSwitchAutoDiscover" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
          <div class="settings-row">
            <span class="settings-label">连接确认</span>
            <label class="switch">
              <input type="checkbox" id="connSwitchRequireConfirm" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
          <div class="settings-row">
            <span class="settings-label">仅 Wi-Fi 传输</span>
            <label class="switch">
              <input type="checkbox" id="connSwitchWifiOnly" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
          <div class="settings-row">
            <span class="settings-label">显示本机 IP</span>
            <label class="switch">
              <input type="checkbox" id="connSwitchShowLocalIp" checked />
              <span class="switch-slider"></span>
            </label>
          </div>
        </div>
      </section>

      <!-- Parameters Card (6.png) -->
      <section class="settings-group">
        <h3 class="settings-group-title">传输参数</h3>
        <div class="settings-card">
          <div class="settings-row">
            <span class="settings-label">默认接收方式</span>
            <select id="connSettingReceiveMode" class="settings-select">
              <option value="auto">自动保存</option>
              <option value="prompt">每次询问</option>
              <option value="notify">仅接收通知</option>
            </select>
          </div>
          <div class="settings-row">
            <span class="settings-label">局域网端口</span>
            <input type="number" id="connSettingPort" class="settings-number-input" value="17631" min="1024" max="65535" />
          </div>
        </div>
      </section>

      <!-- Danger / Reset Button (6.png) -->
      <div>
        <button id="resetConnBtn" class="btn-outline-danger" type="button">重置连接记录</button>
      </div>
    </div>

    <!-- Floating Bottom Action (6.png) -->
    <footer class="conn-save-footer">
      <button id="saveConnSettingsBtn" class="text-button primary-button btn-block" type="button">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="20 6 9 17 4 12"/>
        </svg>
        <span>保存设置</span>
      </button>
    </footer>
  </div>

  <!-- Fullscreen Preview Modal (for images / popups) -->
  <div id="previewDialog" class="preview-dialog" role="dialog" aria-modal="true">
    <div id="previewOverlay" class="preview-overlay"></div>
    <section class="preview-panel">
      <header class="preview-head">
        <div class="preview-title">
          <span data-emphasis id="previewName">文件预览</span>
          <span id="previewMeta"></span>
        </div>
        <button id="closePreview" class="preview-close-btn" type="button" aria-label="关闭">×</button>
      </header>
      <div id="previewBody" class="preview-body"></div>
      <footer class="preview-foot">
        <a id="previewDownload" class="text-button primary-button" download>下载文件</a>
      </footer>
    </section>
  </div>

  <script>
    window.__LAN_SHARE_APP__ = true;
    const query = new URLSearchParams(location.search);
    const token = query.get("token") || "";
    const initialInviteCode = query.get("invite") || "";

    // DOM Elements
    const toastEl = document.getElementById("mobileToast");
    const statusEl = document.getElementById("status");
    const statusDotEl = document.getElementById("statusDot");
    const filesEl = document.getElementById("files");
    const filesCountBadgeEl = document.getElementById("filesCountBadge");
    const filesPeerInfoEl = document.getElementById("filesPeerInfo");
    const messagesEl = document.getElementById("messages");
    const chatEmptyStateEl = document.getElementById("chatEmptyState");
    const inputEl = document.getElementById("messageInput");
    const deviceNameEl = document.getElementById("deviceName");
    const inviteCodeEl = document.getElementById("inviteCode");
    const groupsEl = document.getElementById("groups");
    const groupListViewEl = document.getElementById("groupListView");
    const detailViewEl = document.getElementById("detailView");
    const backGroupsEl = document.getElementById("backGroups");
    const leaveGroupEl = document.getElementById("leaveGroup");
    const createSessionEl = document.getElementById("createSession");
    const messagesTitleEl = document.getElementById("messagesTitle");
    const filesTitleEl = document.getElementById("filesTitle");
    const modeTabs = Array.from(document.querySelectorAll(".mode-tab"));
    const viewTabs = Array.from(document.querySelectorAll(".view-tab"));
    const viewPanels = {
      files: document.getElementById("filesView"),
      messages: document.getElementById("messagesView")
    };

    // Preview Elements
    const activeFileNameEl = document.getElementById("activeFileName");
    const activeFileTypeBadgeEl = document.getElementById("activeFileTypeBadge");
    const copyPreviewTextBtnEl = document.getElementById("copyPreviewTextBtn");
    const activeFileDownloadBtnEl = document.getElementById("activeFileDownloadBtn");
    const activeFileContentEl = document.getElementById("activeFileContent");
    const previewDialogEl = document.getElementById("previewDialog");
    const previewNameEl = document.getElementById("previewName");
    const previewMetaEl = document.getElementById("previewMeta");
    const previewBodyEl = document.getElementById("previewBody");
    const previewDownloadEl = document.getElementById("previewDownload");

    // Settings Layer Elements
    const settingsPageEl = document.getElementById("settingsPage");
    const connSettingsPageEl = document.getElementById("connSettingsPage");
    const settingsDeviceNameDisplayEl = document.getElementById("settingsDeviceNameDisplay");
    const settingsDeviceIdDisplayEl = document.getElementById("settingsDeviceIdDisplay");
    const settingsPlatformDisplayEl = document.getElementById("settingsPlatformDisplay");
    const settingsDarkModeEl = document.getElementById("settingDarkMode");
    const connHostIpDisplayEl = document.getElementById("connHostIpDisplay");
    const connSettingPortEl = document.getElementById("connSettingPort");
    const connSettingReceiveModeEl = document.getElementById("connSettingReceiveMode");

    let ws = null;
    let deviceRemoved = false;
    let deviceId = "";
    let currentSessionId = "";
    let currentMode = "direct";
    let currentGroupId = "";
    let joinedGroups = [];
    let currentFilesList = [];
    let activePreviewFileItem = null;
    let toastTimer = null;
    const messageIds = new Set();

    // Init Theme
    const savedTheme = localStorage.getItem("lanShareTheme") || (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
    document.documentElement.setAttribute("data-theme", savedTheme);
    if (settingsDarkModeEl) settingsDarkModeEl.checked = savedTheme === "dark";

    // Set initial device name
    deviceNameEl.value = localStorage.getItem("lanShareDeviceName") || "";
    inviteCodeEl.value = initialInviteCode;
    if (connHostIpDisplayEl) connHostIpDisplayEl.textContent = location.host || "192.168.1.1:17631";
    if (connSettingPortEl && location.port) connSettingPortEl.value = location.port;
    if (filesPeerInfoEl) filesPeerInfoEl.textContent = "来自 电脑端 · " + (location.hostname || "局域网服务");

    // Detect platform
    const ua = navigator.userAgent;
    let platform = "Mobile Web";
    if (/android/i.test(ua)) platform = "Android · Chrome";
    else if (/iphone|ipad|ipod/i.test(ua)) platform = "iOS · Safari";
    else if (/macintosh/i.test(ua)) platform = "macOS · Web";
    else if (/windows/i.test(ua)) platform = "Windows · Web";
    if (settingsPlatformDisplayEl) settingsPlatformDisplayEl.textContent = platform;

    // Toast
    function showToast(msg, duration = 2400) {
      if (!msg) return;
      toastEl.textContent = msg;
      toastEl.classList.add("show");
      clearTimeout(toastTimer);
      toastTimer = setTimeout(() => {
        toastEl.classList.remove("show");
      }, duration);
    }

    function formatSize(size) {
      const value = Number(size || 0);
      if (value >= 1024 * 1024 * 1024) return (value / 1024 / 1024 / 1024).toFixed(2) + " GB";
      if (value >= 1024 * 1024) return (value / 1024 / 1024).toFixed(2) + " MB";
      if (value >= 1024) return (value / 1024).toFixed(1) + " KB";
      return value + " B";
    }

    function setStatus(text) {
      statusEl.textContent = text;
      if (text.includes("已连接")) {
        statusDotEl.className = "status-dot is-connected";
      } else if (text.includes("断开") || text.includes("失败")) {
        statusDotEl.className = "status-dot is-disconnected";
      } else {
        statusDotEl.className = "status-dot";
      }
    }

    function setActiveView(view) {
      viewTabs.forEach(tab => {
        tab.classList.toggle("active", tab.dataset.view === view);
      });
      Object.entries(viewPanels).forEach(([name, panel]) => {
        panel.classList.toggle("active", name === view);
      });
      if (view === "files") {
        loadFiles().catch(error => setStatus(error.message));
      }
    }

    function copyMessageText(text) {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(() => showToast("已复制到剪贴板"));
        return;
      }
      const element = document.createElement("textarea");
      element.value = text;
      document.body.appendChild(element);
      element.select();
      document.execCommand("copy");
      document.body.removeChild(element);
      showToast("已复制到剪贴板");
    }

    function renderMode() {
      modeTabs.forEach(tab => {
        tab.classList.toggle("active", tab.dataset.mode === currentMode);
      });
      if (currentMode === "group") {
        groupListViewEl.classList.toggle("active", !currentGroupId);
        detailViewEl.classList.toggle("hidden", !currentGroupId);
        backGroupsEl.classList.toggle("active", Boolean(currentGroupId));
        leaveGroupEl.classList.toggle("active", Boolean(currentGroupId));
        createSessionEl.style.display = "none";
        const group = joinedGroups.find(item => item.id === currentGroupId);
        messagesTitleEl.textContent = group ? group.name || "群聊" : "群聊";
        filesTitleEl.textContent = group ? (group.name || "群聊") + " 共享文件" : "获取内容";
      } else {
        groupListViewEl.classList.remove("active");
        detailViewEl.classList.remove("hidden");
        backGroupsEl.classList.remove("active");
        leaveGroupEl.classList.remove("active");
        createSessionEl.style.display = "";
        messagesTitleEl.textContent = "单聊消息";
        filesTitleEl.textContent = "获取内容";
      }
    }

    function switchMode(mode) {
      currentMode = mode;
      if (mode === "group") {
        currentGroupId = "";
        currentSessionId = "";
        renderMode();
        loadGroups().catch(error => setStatus(error.message));
        return;
      }
      currentGroupId = "";
      currentSessionId = "";
      renderMode();
      loadMessages().catch(error => setStatus(error.message));
    }

    async function api(path, options = {}) {
      const response = await fetch(path, options);
      if (!response.ok) {
        let message = "请求失败 (" + response.status + ")";
        try {
          const data = await response.json();
          message = data.error || data.message || message;
        } catch (_) {}
        throw new Error(message);
      }
      const text = await response.text();
      return text ? JSON.parse(text) : null;
    }

    function fileUrl(file, kind) {
      const endpoint = kind === "preview" ? "/api/files/preview" : "/api/files/download";
      return endpoint + "?token=" + encodeURIComponent(token) + "&id=" + encodeURIComponent(file.id);
    }

    function getFileTypeCategory(file) {
      const name = (file.name || "").toLowerCase();
      const mime = (file.mimeType || "").toLowerCase();
      if (mime.startsWith("image/") || /\.(png|jpe?g|gif|webp|svg|bmp)$/.test(name)) return { kind: "img", label: "图片", ext: "IMG" };
      if (mime.startsWith("video/") || /\.(mp4|webm|mkv|mov|avi)$/.test(name)) return { kind: "media", label: "视频", ext: "VID" };
      if (mime.startsWith("audio/") || /\.(mp3|wav|ogg|m4a|flac)$/.test(name)) return { kind: "media", label: "音频", ext: "AUD" };
      if (/\.(zip|tar|gz|7z|rar)$/.test(name)) return { kind: "zip", label: "压缩包", ext: "ZIP" };
      if (mime.startsWith("text/") || /\.(txt|md|js|ts|json|html|css|py|rs|vue|c|cpp|h|java|sh|yml|yaml|xml|sql)$/.test(name)) return { kind: "txt", label: "文本 · UTF-8", ext: "TXT" };
      if (mime.includes("pdf") || /\.pdf$/.test(name)) return { kind: "txt", label: "PDF 文档", ext: "PDF" };
      return { kind: "other", label: "文件", ext: "DOC" };
    }

    function renderFileRowIcon(category) {
      return '<div class="file-row-icon ' + category.kind + '">' +
        '<span style="font-size:10px;font-weight:700;">' + category.ext + '</span>' +
      '</div>';
    }

    async function loadActivePreview(file) {
      activePreviewFileItem = file;
      const category = getFileTypeCategory(file);
      activeFileNameEl.textContent = file.name || "文件预览";
      activeFileTypeBadgeEl.textContent = category.label;
      activeFileDownloadBtnEl.href = fileUrl(file, "download");
      activeFileDownloadBtnEl.style.display = "inline-flex";

      document.querySelectorAll(".file-row-item").forEach(el => {
        el.classList.toggle("is-active", el.dataset.fileId === file.id);
      });

      if (category.kind === "txt") {
        copyPreviewTextBtnEl.style.display = "inline-flex";
        activeFileContentEl.innerHTML = '<div class="empty-placeholder">正在读取文件内容...</div>';
        try {
          const res = await fetch(fileUrl(file, "preview"));
          if (!res.ok) throw new Error("无法读取预览文本");
          const text = await res.text();
          copyPreviewTextBtnEl.onclick = () => copyMessageText(text);

          const lines = text.split("\n");
          const gutterHtml = lines.map((_, i) => '<div>' + (i + 1) + '</div>').join("");
          activeFileContentEl.innerHTML =
            '<div class="code-preview-viewer">' +
              '<div class="code-gutter">' + gutterHtml + '</div>' +
              '<pre class="code-content"></pre>' +
            '</div>';
          activeFileContentEl.querySelector(".code-content").textContent = text;
        } catch (err) {
          activeFileContentEl.innerHTML = '<div class="empty-placeholder">' + (err.message || "预览失败，可下载到本地打开") + '</div>';
        }
        return;
      }

      copyPreviewTextBtnEl.style.display = "none";
      if (category.kind === "img") {
        activeFileContentEl.innerHTML = '<div class="preview-media-box"><img src="' + fileUrl(file, "preview") + '" alt="' + (file.name || "") + '" /></div>';
        return;
      }
      if (category.kind === "media") {
        const isVideo = (file.mimeType || "").startsWith("video/") || /\.(mp4|webm)$/i.test(file.name);
        const tag = isVideo ? "video" : "audio";
        activeFileContentEl.innerHTML = '<div class="preview-media-box"><' + tag + ' controls src="' + fileUrl(file, "preview") + '"></' + tag + '></div>';
        return;
      }

      activeFileContentEl.innerHTML = '<div class="empty-placeholder">当前文件类型不支持在线直接渲染，请点击右上方【下载】查看。</div>';
    }

    function closePreview() {
      previewBodyEl.innerHTML = "";
      previewDialogEl.classList.remove("open");
    }

    async function previewFile(file) {
      const category = getFileTypeCategory(file);
      const previewUrl = fileUrl(file, "preview");
      const downloadUrl = fileUrl(file, "download");
      previewNameEl.textContent = file.name || "文件预览";
      previewMetaEl.textContent = formatSize(file.size) + " · " + (file.mimeType || "文件");
      previewDownloadEl.href = downloadUrl;
      previewBodyEl.innerHTML = '<div class="empty-placeholder">正在准备预览...</div>';
      previewDialogEl.classList.add("open");

      if (category.kind === "img") {
        previewBodyEl.innerHTML = '<img src="' + previewUrl + '" alt="' + (file.name || "") + '" />';
        return;
      }
      if (category.kind === "media") {
        const isVideo = (file.mimeType || "").startsWith("video/") || /\.(mp4|webm)$/i.test(file.name);
        const tag = isVideo ? "video" : "audio";
        previewBodyEl.innerHTML = '<' + tag + ' controls src="' + previewUrl + '"></' + tag + '>';
        return;
      }
      if (category.kind === "txt") {
        try {
          const res = await fetch(previewUrl);
          if (!res.ok) throw new Error("无法读取预览文本");
          const text = await res.text();
          const pre = document.createElement("pre");
          pre.textContent = text;
          previewBodyEl.innerHTML = "";
          previewBodyEl.appendChild(pre);
        } catch (err) {
          previewBodyEl.innerHTML = '<div class="empty-placeholder">' + (err.message || "预览失败，请下载后打开。") + '</div>';
        }
        return;
      }
      previewBodyEl.innerHTML = '<div class="empty-placeholder">浏览器暂不支持在线解析这种文件格式，可下载到本地后打开。</div>';
    }

    async function registerDevice() {
      const data = await api("/api/devices", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          token,
          deviceId,
          deviceName: deviceNameEl.value,
          userAgent: navigator.userAgent
        })
      });
      deviceId = data.id || deviceId;
      deviceNameEl.value = data.name || data.autoName || "";
      localStorage.setItem("lanShareDeviceName", deviceNameEl.value);
      updateSettingsInfo();
    }

    function updateSettingsInfo() {
      if (settingsDeviceNameDisplayEl) settingsDeviceNameDisplayEl.textContent = deviceNameEl.value || "未命名设备";
      if (settingsDeviceIdDisplayEl) settingsDeviceIdDisplayEl.textContent = deviceId ? (deviceId.slice(0, 16) + "...") : "自动生成中";
    }

    function renderGroups() {
      groupsEl.innerHTML = "";
      if (!joinedGroups.length) {
        groupsEl.innerHTML = '<div class="empty-placeholder">还没有加入任何群聊，可输入上方邀请码加入</div>';
        renderMode();
        return;
      }
      for (const group of joinedGroups) {
        const button = document.createElement("div");
        button.className = "group-session-item";
        button.innerHTML = '<div class="group-session-main"><span class="group-session-name"></span><span class="group-session-meta"></span></div><button class="btn-xs primary" type="button">进入</button>';
        button.querySelector(".group-session-name").textContent = group.name || "群聊";
        button.querySelector(".group-session-meta").textContent = "邀请码 " + (group.inviteCode || "") + " · " + (group.messageVisibility === "afterJoin" ? "仅加入后消息" : "全部历史消息");
        button.onclick = () => openGroupSession(group);
        groupsEl.appendChild(button);
      }
      renderMode();
    }

    function openGroupSession(group) {
      currentMode = "group";
      currentGroupId = group.id || "";
      currentSessionId = group.sessionId || "";
      setActiveView("messages");
      renderMode();
      loadMessages().catch(error => setStatus(error.message));
    }

    async function loadGroups() {
      joinedGroups = await api("/api/groups?token=" + encodeURIComponent(token) + "&deviceId=" + encodeURIComponent(deviceId));
      renderGroups();
    }

    async function joinGroup(inviteCode) {
      const code = String(inviteCode || inviteCodeEl.value || "").trim();
      if (!code) {
        showToast("请输入群邀请码");
        return;
      }
      const data = await api("/api/groups/join", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          token,
          inviteCode: code,
          deviceId,
          deviceName: deviceNameEl.value,
          userAgent: navigator.userAgent
        })
      });
      deviceId = data.device?.id || deviceId;
      currentMode = "group";
      currentGroupId = data.group?.id || "";
      currentSessionId = data.currentSession?.id || currentSessionId;
      await loadGroups();
      await loadMessages();
      showToast("已加入群聊 " + (data.group?.name || ""));
    }

    async function leaveCurrentGroup() {
      const group = joinedGroups.find(item => item.sessionId === currentSessionId);
      if (!group) {
        showToast("请先选择要退出的群聊");
        return;
      }
      await api("/api/groups/leave", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          token,
          groupId: group.id,
          deviceId
        })
      });
      currentMode = "group";
      currentGroupId = "";
      currentSessionId = "";
      await loadGroups();
      renderMode();
      showToast("已退出群聊 " + (group.name || ""));
    }

    async function loadFiles() {
      if (currentMode === "group" && !currentGroupId) {
        filesEl.innerHTML = '<div class="empty-placeholder">请选择群聊后查看共享文件</div>';
        filesCountBadgeEl.textContent = "0 项";
        return;
      }
      const files = await api("/api/files?token=" + encodeURIComponent(token) + "&sessionId=" + encodeURIComponent(currentSessionId));
      currentFilesList = files || [];
      filesEl.innerHTML = "";
      filesCountBadgeEl.textContent = (currentFilesList.length) + " 项";

      if (!currentFilesList.length) {
        filesEl.innerHTML = '<div class="empty-placeholder">电脑端还没有共享文件</div>';
        activeFileNameEl.textContent = "文件预览";
        activeFileTypeBadgeEl.textContent = "暂无文件";
        activeFileContentEl.innerHTML = '<div class="empty-placeholder">电脑端发送文件后将在此显示并支持即时预览</div>';
        copyPreviewTextBtnEl.style.display = "none";
        activeFileDownloadBtnEl.style.display = "none";
        return;
      }

      for (const file of currentFilesList) {
        const item = document.createElement("div");
        const category = getFileTypeCategory(file);
        const downloadUrl = fileUrl(file, "download");
        item.className = "file-row-item";
        item.dataset.fileId = file.id;
        item.innerHTML = renderFileRowIcon(category) +
          '<div class="file-row-info">' +
            '<span class="file-row-name"></span>' +
            '<span class="file-row-meta"></span>' +
          '</div>' +
          '<div class="file-row-actions">' +
            '<button class="btn-xs file-preview-btn" type="button">预览</button>' +
            '<a class="btn-xs file-download-btn" download>下载</a>' +
          '</div>';
        item.querySelector(".file-row-name").textContent = file.name;
        item.querySelector(".file-row-meta").textContent = formatSize(file.size) + " · " + (file.mimeType || "文件");
        item.querySelector(".file-preview-btn").onclick = (e) => { e.stopPropagation(); loadActivePreview(file); };
        item.querySelector(".file-download-btn").href = downloadUrl;
        item.onclick = () => loadActivePreview(file);
        filesEl.appendChild(item);
      }

      if (!activePreviewFileItem || !currentFilesList.some(f => f.id === activePreviewFileItem.id)) {
        loadActivePreview(currentFilesList[0]);
      }
    }

    async function loadMessages() {
      if (currentMode === "group" && !currentGroupId) {
        chatEmptyStateEl.style.display = "flex";
        messagesEl.style.display = "none";
        messagesEl.innerHTML = "";
        await loadFiles();
        return;
      }
      const data = await api("/api/messages?token=" + encodeURIComponent(token) + "&deviceId=" + encodeURIComponent(deviceId) + "&sessionId=" + encodeURIComponent(currentSessionId));
      currentSessionId = data.currentSession?.id || currentSessionId;
      messagesEl.innerHTML = "";
      messageIds.clear();
      for (const message of data.messages || []) {
        appendMessage(message);
      }
      if (!messageIds.size) {
        chatEmptyStateEl.style.display = "flex";
        messagesEl.style.display = "none";
      } else {
        chatEmptyStateEl.style.display = "none";
        messagesEl.style.display = "flex";
      }
      renderMode();
      await loadFiles();
    }

    async function createSession() {
      const data = await api("/api/sessions", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          token,
          deviceId,
          deviceName: deviceNameEl.value,
          userAgent: navigator.userAgent
        })
      });
      deviceId = data.device?.id || deviceId;
      currentMode = "direct";
      currentGroupId = "";
      currentSessionId = data.currentSession?.id || "";
      renderMode();
      chatEmptyStateEl.style.display = "flex";
      messagesEl.style.display = "none";
      messageIds.clear();
      await loadFiles();
      showToast("新会话已创建");
    }

    function appendMessage(message) {
      if (messageIds.has(message.id)) return;
      messageIds.add(message.id);
      chatEmptyStateEl.style.display = "none";
      messagesEl.style.display = "flex";

      const item = document.createElement("article");
      item.className = "message" + (message.direction === "mobile-to-desktop" ? " me" : "");
      item.title = "点击消息可复制";

      const text = document.createElement("div");
      text.textContent = message.content || "";
      text.onclick = () => copyMessageText(message.content || "");
      item.appendChild(text);

      if (message.attachments?.length) {
        const gallery = document.createElement("div");
        gallery.className = "message-attachment";
        let activeIndex = 0;
        const renderAttachment = () => {
          gallery.replaceChildren();
          const file = message.attachments[activeIndex];
          const mime = file.mimeType || "";
          if (mime.startsWith("image/") || mime.startsWith("video/") || mime.startsWith("audio/")) {
            const media = document.createElement(mime.startsWith("image/") ? "img" : mime.startsWith("video/") ? "video" : "audio");
            media.src = fileUrl(file, "preview");
            media.alt = file.name;
            media.controls = true;
            media.preload = "metadata";
            if (mime.startsWith("image/")) media.onclick = (e) => { e.stopPropagation(); previewFile(file); };
            gallery.appendChild(media);
          }
          const name = document.createElement("span");
          name.className = "message-attachment-name";
          name.textContent = file.name + " · " + formatSize(file.size);
          gallery.appendChild(name);
          const actions = document.createElement("div");
          actions.className = "message-attachment-actions";
          const previous = document.createElement("button");
          previous.textContent = "上一项";
          previous.className = "btn-xs";
          previous.disabled = activeIndex === 0;
          previous.onclick = (e) => { e.stopPropagation(); activeIndex--; renderAttachment(); };
          const next = document.createElement("button");
          next.textContent = "下一项";
          next.className = "btn-xs";
          next.disabled = activeIndex === message.attachments.length - 1;
          next.onclick = (e) => { e.stopPropagation(); activeIndex++; renderAttachment(); };
          const position = document.createElement("span");
          position.style.fontSize = "11px";
          position.style.color = "var(--muted)";
          position.textContent = (activeIndex + 1) + " / " + message.attachments.length;
          const download = document.createElement("a");
          download.className = "btn-xs primary";
          download.href = fileUrl(file, "download");
          download.download = file.name;
          download.textContent = "下载";
          download.onclick = (e) => e.stopPropagation();
          if (message.attachments.length > 1) actions.append(previous, position, next);
          actions.append(download);
          gallery.appendChild(actions);
        };
        renderAttachment();
        item.appendChild(gallery);
      }

      const time = document.createElement("small");
      time.textContent = new Date(message.createdAt || Date.now()).toLocaleTimeString();
      item.appendChild(time);
      messagesEl.appendChild(item);
      messagesEl.scrollTop = messagesEl.scrollHeight;
    }

    function connectWs() {
      if (deviceRemoved) return;
      const protocol = location.protocol === "https:" ? "wss:" : "ws:";
      ws = new WebSocket(protocol + "//" + location.host + "/ws?token=" + encodeURIComponent(token) + "&deviceId=" + encodeURIComponent(deviceId));
      ws.onopen = () => {
        setStatus("已连接电脑端服务");
        loadGroups().catch(error => setStatus(error.message));
      };
      ws.onclose = () => {
        if (deviceRemoved) return;
        setStatus("连接已断开，正在重连...");
        window.setTimeout(connectWs, 1600);
      };
      ws.onmessage = event => {
        const payload = JSON.parse(event.data);
        if (payload.type === "deviceRemoved") {
          deviceRemoved = true;
          currentSessionId = "";
          setStatus("此设备已被电脑端移除，请重新访问后加入。");
          showToast("此设备已被电脑端移除");
          ws.close();
          return;
        }
        if (payload.type === "message" && (!currentSessionId || payload.message?.sessionId === currentSessionId)) {
          appendMessage(payload.message);
          setActiveView("messages");
          if (document.getElementById("settingSound")?.checked) {
            try { navigator.vibrate?.(40); } catch (_) {}
          }
        }
        if (payload.type === "filesChanged") loadFiles().catch(error => setStatus(error.message));
        if (payload.type === "groupsChanged") loadGroups().then(() => showToast("收到新的群聊邀请")).catch(error => setStatus(error.message));
      };
    }

    // Attachments Handling
    const pendingAttachments = [];
    const attachmentInput = document.getElementById("attachmentInput");
    let sendingAttachments = false;
    let pendingMessageId = "";

    function renderPendingAttachments() {
      const list = document.getElementById("pendingAttachments");
      list.replaceChildren();
      pendingAttachments.forEach((item, index) => {
        const chip = document.createElement("div");
        chip.className = "pending-attachment";
        const name = document.createElement("span");
        name.textContent = item.file.name;
        const remove = document.createElement("span");
        remove.className = "pending-remove-btn";
        remove.textContent = "×";
        remove.onclick = () => {
          if (sendingAttachments) return;
          pendingAttachments.splice(index, 1);
          pendingMessageId = "";
          renderPendingAttachments();
        };
        chip.append(name, remove);
        list.appendChild(chip);
      });
    }

    function addPendingAttachments(files) {
      if (sendingAttachments) return;
      if (pendingAttachments.length + files.length > 100) {
        showToast("一条消息最多添加 100 个附件");
        return;
      }
      pendingAttachments.push(...files.map(file => ({ file })));
      pendingMessageId = "";
      renderPendingAttachments();
    }

    // Events
    document.getElementById("openSettingsBtn").onclick = () => {
      updateSettingsInfo();
      settingsPageEl.classList.add("active");
    };
    document.getElementById("settingsBackBtn").onclick = () => {
      settingsPageEl.classList.remove("active");
    };
    document.getElementById("openConnSettingsRow").onclick = () => {
      connSettingsPageEl.classList.add("active");
    };
    document.getElementById("connSettingsBackBtn").onclick = () => {
      connSettingsPageEl.classList.remove("active");
    };

    document.getElementById("openFilesBtn").onclick = () => {
      setActiveView("files");
    };
    document.getElementById("filesBackBtn").onclick = () => {
      setActiveView("messages");
    };

    document.getElementById("saveDeviceName").onclick = () => {
      localStorage.setItem("lanShareDeviceName", deviceNameEl.value);
      registerDevice()
        .then(() => {
          showToast("设备名称已保存");
          loadMessages();
        })
        .catch(error => setStatus(error.message));
    };

    viewTabs.forEach(tab => {
      tab.onclick = () => setActiveView(tab.dataset.view);
    });
    modeTabs.forEach(tab => {
      tab.onclick = () => switchMode(tab.dataset.mode);
    });

    document.getElementById("refreshFiles").onclick = () => {
      loadFiles().then(() => showToast("已刷新文件列表")).catch(error => setStatus(error.message));
    };

    document.getElementById("createSession").onclick = () => createSession().catch(error => setStatus(error.message));
    document.getElementById("joinGroup").onclick = () => joinGroup().catch(error => setStatus(error.message));
    backGroupsEl.onclick = () => {
      currentMode = "group";
      currentGroupId = "";
      currentSessionId = "";
      renderMode();
    };
    leaveGroupEl.onclick = () => leaveCurrentGroup().catch(error => setStatus(error.message));

    document.getElementById("closePreview").onclick = closePreview;
    document.getElementById("previewOverlay").onclick = closePreview;

    document.getElementById("chooseAttachments").onclick = () => attachmentInput.click();
    attachmentInput.onchange = () => {
      addPendingAttachments([...attachmentInput.files]);
      attachmentInput.value = "";
    };

    inputEl.addEventListener("paste", event => {
      if (event.clipboardData?.files.length) {
        event.preventDefault();
        addPendingAttachments([...event.clipboardData.files]);
      }
    });

    inputEl.addEventListener("input", () => { pendingMessageId = ""; });

    // Send Message
    document.getElementById("sendMessage").onclick = async () => {
      const content = inputEl.value.trim();
      if ((!content && !pendingAttachments.length) || !currentSessionId || sendingAttachments) return;
      sendingAttachments = true;
      document.getElementById("sendMessage").disabled = true;
      document.getElementById("chooseAttachments").disabled = true;
      inputEl.disabled = true;
      renderPendingAttachments();

      if (!pendingMessageId) {
        const bytes = crypto.getRandomValues(new Uint8Array(16));
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
        pendingMessageId = [hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16), hex.slice(16, 20), hex.slice(20)].join("-");
      }

      try {
        const sessionId = currentSessionId;
        for (const [index, item] of pendingAttachments.entries()) {
          setStatus("准备附件 " + (index + 1) + " / " + pendingAttachments.length);
          if (!item.uploadedId || item.sessionId !== sessionId) {
            const uploaded = await api("/api/files/upload?token=" + encodeURIComponent(token) + "&sessionId=" + encodeURIComponent(sessionId) + "&name=" + encodeURIComponent(item.file.name), {
              method: "PUT",
              body: item.file
            });
            item.uploadedId = uploaded.id;
            item.sessionId = sessionId;
          }
        }
        const message = await api("/api/messages?token=" + encodeURIComponent(token), {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            messageId: pendingMessageId,
            content,
            sessionId,
            attachmentIds: pendingAttachments.map(item => item.uploadedId)
          })
        });
        if (currentSessionId === sessionId) appendMessage(message);
        inputEl.value = "";
        pendingAttachments.splice(0);
        pendingMessageId = "";
        setStatus("已发送");
      } catch (error) {
        setStatus("发送失败，草稿已保留：" + error.message);
        showToast("发送失败，草稿已保留");
      } finally {
        sendingAttachments = false;
        document.getElementById("sendMessage").disabled = false;
        document.getElementById("chooseAttachments").disabled = false;
        inputEl.disabled = false;
        renderPendingAttachments();
      }
    };

    inputEl.addEventListener("keydown", event => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        document.getElementById("sendMessage").click();
      }
    });

    // Dark Mode Toggle
    if (settingsDarkModeEl) {
      settingsDarkModeEl.onchange = (e) => {
        const theme = e.target.checked ? "dark" : "light";
        document.documentElement.setAttribute("data-theme", theme);
        localStorage.setItem("lanShareTheme", theme);
        showToast(theme === "dark" ? "已开启深色模式" : "已切换为浅色模式");
      };
    }

    // Connection Settings Save
    document.getElementById("saveConnSettingsBtn").onclick = () => {
      const connConfig = {
        sameNet: document.getElementById("connSwitchSameNet")?.checked,
        autoDiscover: document.getElementById("connSwitchAutoDiscover")?.checked,
        requireConfirm: document.getElementById("connSwitchRequireConfirm")?.checked,
        wifiOnly: document.getElementById("connSwitchWifiOnly")?.checked,
        showLocalIp: document.getElementById("connSwitchShowLocalIp")?.checked,
        receiveMode: connSettingReceiveModeEl?.value,
        port: connSettingPortEl?.value
      };
      localStorage.setItem("lanShareConnSettings", JSON.stringify(connConfig));
      showToast("连接设置已保存");
      setTimeout(() => {
        connSettingsPageEl.classList.remove("active");
      }, 400);
    };

    // Connection Reset
    document.getElementById("resetConnBtn").onclick = () => {
      localStorage.removeItem("lanShareConnSettings");
      localStorage.removeItem("lanShareDeviceName");
      showToast("已重置连接记录与本地配置");
    };

    renderMode();
    registerDevice()
      .then(() => initialInviteCode ? joinGroup(initialInviteCode) : Promise.resolve())
      .then(() => Promise.all([loadGroups(), loadMessages()]))
      .then(connectWs)
      .catch(error => setStatus(error.message));
  </script>
</body>
</html>"##
    )
    .to_string()
}

fn boxed_full(body: impl Into<Bytes>) -> BoxBody {
    Full::new(body.into())
        .map_err(|never| match never {})
        .boxed()
}

fn response(status: StatusCode, content_type: &str, body: impl Into<Bytes>) -> Response<BoxBody> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .body(boxed_full(body))
        .unwrap_or_else(|_| Response::new(boxed_full("response build failed")))
}

fn mobile_page_response() -> Response<BoxBody> {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .header("cache-control", "no-store, no-cache, must-revalidate")
        .header("pragma", "no-cache")
        .header("expires", "0")
        .body(boxed_full(mobile_page_html()))
        .unwrap_or_else(|_| Response::new(boxed_full("response build failed")))
}

fn json_http_response(status: StatusCode, payload: Value) -> Response<BoxBody> {
    response(
        status,
        "application/json; charset=utf-8",
        Bytes::from(format!("{}\n", payload)),
    )
}

fn api_success(data: Value) -> Response<BoxBody> {
    json_http_response(
        StatusCode::OK,
        json!({
            "status": "success",
            "data": data,
            "message": ""
        }),
    )
}

fn api_error(status: StatusCode, message: impl Into<String>) -> Response<BoxBody> {
    json_http_response(
        status,
        json!({
            "status": "error",
            "data": Value::Null,
            "message": message.into()
        }),
    )
}

fn query_value(uri: &hyper::Uri, key: &str) -> String {
    Url::parse(&format!("http://127.0.0.1{}", uri))
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find(|(item_key, _)| item_key == key)
                .map(|(_, value)| value.to_string())
        })
        .unwrap_or_default()
}

fn client_ip(addr: SocketAddr) -> String {
    addr.ip().to_string()
}

async fn is_request_token_valid(registry: &LanShareServerRegistry, actual: &str) -> bool {
    let runtime = registry.inner.lock().await;

    is_valid_token(&runtime.token, actual)
}

#[derive(Clone)]
struct HttpContext {
    app: tauri::AppHandle,
    paths: AppPaths,
    registry: LanShareServerRegistry,
}

pub async fn start_service(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    _payload: Value,
) -> Result<Value, ManagerError> {
    attachments::cleanup_uploads(registry, paths).await?;
    let lan_ips = available_lan_ips()?;
    let mut runtime = registry.inner.lock().await;

    if runtime.handle.is_some() {
        refresh_access_address(&mut runtime, &lan_ips);
        return Ok(lan_share_response(json!({
            "running": true,
            "accessUrl": runtime.access_url,
            "qrSvg": if runtime.access_url.is_empty() { String::new() } else { qr_svg(&runtime.access_url)? },
            "lanIp": runtime.lan_ip,
            "lanIps": lan_ips,
            "port": runtime.port,
            "onlineDevices": runtime.clients.len() + runtime.native.online_count()
        })));
    }

    let lan_ip = lan_ips.first().cloned().ok_or_else(|| ManagerError::System(
        "未找到可用局域网 IP，请确认电脑已连接 Wi-Fi、局域网或热点。".to_string()
    ))?;
    let token = create_token()?;
    let mut port = DEFAULT_PORT;
    let listener = loop {
        match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await {
            Ok(listener) => break listener,
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse && port < DEFAULT_PORT + 20 =>
            {
                port += 1;
            }
            Err(error) => return Err(ManagerError::Io(error)),
        }
    };
    let access_url = format!("http://{}:{}/?token={}", lan_ip, port, token);
    let context = HttpContext {
        app: app.clone(),
        paths: paths.clone(),
        registry: registry.clone(),
    };
    let handle = tauri::async_runtime::spawn(async move {
        let connection_slots = Arc::new(tokio::sync::Semaphore::new(64));
        loop {
            let Ok((stream, addr)) = listener.accept().await else {
                break;
            };
            // 限制连接任务数量，慢连接不能无限占用运行内存。
            let Ok(permit) = connection_slots.clone().try_acquire_owned() else {
                continue;
            };
            let _ = stream.set_nodelay(true);
            let io = TokioIo::new(stream);
            let context = context.clone();

            let connection_registry = context.registry.clone();
            let connection_handle = tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let service =
                    service_fn(move |request| handle_http_request(request, context.clone(), addr));

                if let Err(error) = http1::Builder::new()
                    .serve_connection(io, service)
                    .with_upgrades()
                    .await
                {
                    eprintln!("{error}");
                }
            });
            let mut runtime = connection_registry.inner.lock().await;
            runtime
                .connections
                .retain(|task| !task.inner().is_finished());
            runtime.connections.push(connection_handle);
        }
    });

    runtime.handle = Some(handle);
    runtime.token = token;
    runtime.access_url = access_url.clone();
    runtime.lan_ip = lan_ip.clone();
    runtime.port = port;
    let rotation_registry = registry.clone();
    let rotation_app = app.clone();
    let rotation_paths = paths.clone();
    let mut previous_ips = lan_ips.clone();
    runtime.maintenance = Some(tauri::async_runtime::spawn(async move {
        let mut network_interval = tokio::time::interval(Duration::from_secs(3));
        network_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut token_interval = tokio::time::interval(Duration::from_secs(15 * 60));
        // 跳过令牌首次立即触发，网络检查与令牌轮换共用一个任务。
        token_interval.tick().await;
        loop {
            futures_util::select! {
                _ = network_interval.tick().fuse() => {
                    // 只读取本机网卡，不广播、不扫描；网络变化时才推送完整状态。
                    let Ok(ips) = available_lan_ips() else { continue };
                    let mut runtime = rotation_registry.inner.lock().await;
                    if runtime.handle.is_none() { break; }
                    let changed = refresh_access_address(&mut runtime, &ips);
                    drop(runtime);
                    if changed || ips != previous_ips {
                        previous_ips = ips;
                        let _ = emit_state_changed(&rotation_app, &rotation_registry, &rotation_paths).await;
                    }
                },
                _ = token_interval.tick().fuse() => {
                    let _ = rotate_access_token(&rotation_app, &rotation_registry, &rotation_paths).await;
                },
            }
        }
    }));
    drop(runtime);
    if let Err(error) = native::start(app.clone(), registry, paths).await {
        registry.inner.lock().await.native.error = format!("设备快传节点启动失败：{error}");
    }
    emit_state_changed(&app, registry, paths).await?;

    Ok(lan_share_response(json!({
        "running": true,
        "accessUrl": access_url,
        "qrSvg": qr_svg(&access_url)?,
        "lanIp": lan_ip,
        "lanIps": lan_ips,
        "port": port,
        "onlineDevices": 0
    })))
}

pub async fn stop_service(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<Value, ManagerError> {
    let mut runtime = registry.inner.lock().await;

    stop_runtime(&mut runtime);
    drop(runtime);
    emit_state_changed(&app, registry, paths).await?;

    Ok(lan_share_response(json!(true)))
}

fn stop_runtime(runtime: &mut LanShareRuntime) {
    runtime.native.stop();
    if let Some(handle) = runtime.handle.take() {
        handle.abort();
    }
    if let Some(handle) = runtime.maintenance.take() {
        handle.abort();
    }
    for connection in runtime.connections.drain(..) {
        connection.abort();
    }
    runtime.token.clear();
    runtime.access_url.clear();
    runtime.clients.clear();
    runtime.active_sessions.clear();
}

async fn rotate_access_token(
    app: &tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<(), ManagerError> {
    let mut runtime = registry.inner.lock().await;
    if runtime.handle.is_none() {
        return Ok(());
    }
    let token = create_token()?;
    runtime.token = token.clone();
    runtime.access_url = if runtime.lan_ip.is_empty() {
        String::new()
    } else {
        format!("http://{}:{}/?token={}", runtime.lan_ip, runtime.port, token)
    };
    drop(runtime);
    emit_state_changed(app, registry, paths).await?;
    Ok(())
}

pub async fn emit_state_changed(
    app: &tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<(), ManagerError> {
    let state = get_state(registry, paths).await?;
    let _ = app.emit(EVENT_STATE_CHANGED, state["data"].clone());
    Ok(())
}

async fn emit_devices_changed(
    app: &tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<(), ManagerError> {
    let state = get_state(registry, paths).await?;
    let _ = app.emit(EVENT_DEVICES_CHANGED, state["data"]["devices"].clone());
    Ok(())
}

fn emit_message_created(app: &tauri::AppHandle, message: &LanShareMessage) {
    let _ = app.emit(EVENT_MESSAGE_CREATED, message.clone());
    if message.direction != "mobile-to-desktop" {
        return;
    }
    let floating_visible = app
        .get_webview_window("quick-switch")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    if floating_visible {
        return;
    }
    let content = if message.content.is_empty() {
        if message.attachments.len() == 1 {
            format!("收到文件：{}", message.attachments[0].name)
        } else {
            format!("收到 {} 个附件", message.attachments.len())
        }
    } else {
        message.content.clone()
    };
    if let Err(error) = app
        .notification()
        .builder()
        .title(format!("设备快传 · {}", message.device_name))
        .body(content)
        .show()
    {
        eprintln!("设备快传系统通知发送失败：{error}");
    }
}

pub async fn upsert_device(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    device_id: &str,
    device_name: &str,
    user_agent: &str,
    ip: &str,
) -> Result<LanShareDevice, ManagerError> {
    let _storage = registry.storage.lock().await;
    let mut devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices)?;
    let now = now_millis();
    let normalized_name = normalize_device_name(device_name);
    let next_auto_name = auto_device_name(user_agent, ip);
    let native_device = user_agent == "MonkeyThief/Desktop";
    let canonical_device_id = if ip.is_empty() || native_device {
        device_id.to_string()
    } else {
        device_id_from_ip(ip)
    };

    let device_index = devices
        .iter()
        .position(|device| device.id == canonical_device_id)
        .or_else(|| devices.iter().position(|device| device.id == device_id))
        .or_else(|| {
            if ip.is_empty() || native_device {
                None
            } else {
                devices.iter().position(|device| {
                    device.ip == ip && device.user_agent != "MonkeyThief/Desktop"
                })
            }
        });

    if let Some(index) = device_index {
        let old_device_id = devices[index].id.clone();
        let next_device = {
            let device = &mut devices[index];
            device.id = canonical_device_id.clone();
            device.name = normalized_name;
            device.auto_name = next_auto_name;
            device.user_agent = user_agent.to_string();
            device.ip = ip.to_string();
            device.last_seen_at = now;
            device.clone()
        };

        devices.retain(|device| {
            native_device
                || device.user_agent == "MonkeyThief/Desktop"
                || device.id == canonical_device_id
                || device.ip != ip
                || device.ip.is_empty()
        });
        rewrite_device_references(paths, &old_device_id, &canonical_device_id, ip).await?;
        write_json(&paths.lan_share_files.devices, &json!(devices)).await?;
        return Ok(next_device);
    }

    let device = LanShareDevice {
        id: canonical_device_id,
        name: normalized_name,
        auto_name: next_auto_name,
        user_agent: user_agent.to_string(),
        ip: ip.to_string(),
        first_seen_at: now,
        last_seen_at: now,
    };

    devices.insert(0, device.clone());
    write_json(&paths.lan_share_files.devices, &json!(devices)).await?;
    Ok(device)
}

async fn rewrite_device_references(
    paths: &AppPaths,
    old_device_id: &str,
    next_device_id: &str,
    ip: &str,
) -> Result<(), ManagerError> {
    if old_device_id == next_device_id {
        return Ok(());
    }

    let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
    let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
    let mut downloads: Vec<LanShareDownload> = read_array(&paths.lan_share_files.downloads)?;
    let mut target_session_ids = Vec::new();
    let mut sessions_changed = false;
    let mut messages_changed = false;
    let mut downloads_changed = false;

    for session in sessions.iter_mut() {
        if session.device_id == old_device_id || (!ip.is_empty() && session.ip == ip) {
            target_session_ids.push(session.id.clone());
            session.device_id = next_device_id.to_string();
            sessions_changed = true;
        }
    }

    for message in messages.iter_mut() {
        if message.device_id == old_device_id || target_session_ids.contains(&message.session_id) {
            message.device_id = next_device_id.to_string();
            messages_changed = true;
        }
    }

    for download in downloads.iter_mut() {
        if download.device_id == old_device_id || (!ip.is_empty() && download.ip == ip) {
            download.device_id = next_device_id.to_string();
            downloads_changed = true;
        }
    }

    if sessions_changed {
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
    }
    if messages_changed {
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
    }
    if downloads_changed {
        write_json(&paths.lan_share_files.downloads, &json!(downloads)).await?;
    }

    Ok(())
}

fn ensure_session(
    paths: &AppPaths,
    device_id: &str,
    session_id: &str,
    device_name: &str,
    ip: &str,
) -> Result<LanShareSession, ManagerError> {
    let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
    let now = now_millis();
    let target_index = if session_id.is_empty() {
        sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                session.device_id == device_id
                    && session.mode != "group"
                    && (ip.is_empty() || session.ip == ip || session.ip.is_empty())
            })
            .max_by_key(|(_, session)| session.updated_at)
            .map(|(index, _)| index)
    } else {
        sessions
            .iter()
            .position(|session| session.id == session_id && session.device_id == device_id)
    };

    if let Some(index) = target_index {
        let session = &mut sessions[index];
        session.device_name = device_name.to_string();
        if session.ip.is_empty() {
            session.ip = ip.to_string();
        }
        session.updated_at = now;
        let next_session = session.clone();
        sort_sessions(&mut sessions);
        write_json_blocking(&paths.lan_share_files.sessions, &json!(sessions))?;
        return Ok(next_session);
    }

    let session = LanShareSession {
        id: create_id(),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        ip: ip.to_string(),
        mode: "direct".to_string(),
        group_id: String::new(),
        group_name: String::new(),
        message_visibility: default_message_visibility(),
        joined_at: now,
        created_at: now,
        updated_at: now,
    };

    sessions.insert(0, session.clone());
    sort_sessions(&mut sessions);
    write_json_blocking(&paths.lan_share_files.sessions, &json!(sessions))?;
    Ok(session)
}

fn sort_sessions(sessions: &mut Vec<LanShareSession>) {
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
}

fn session_unread_counts(
    sessions: &[LanShareSession],
    messages: &[LanShareMessage],
    devices: &[LanShareDevice],
) -> HashMap<String, usize> {
    let device_ids = devices
        .iter()
        .map(|device| device.id.as_str())
        .collect::<HashSet<_>>();
    let session_by_id = sessions
        .iter()
        .filter(|session| {
            // 群聊会话的 device_id 为空；单聊只统计仍存在于设备清单的设备。
            session.mode == "group"
                || session.device_id.is_empty()
                || device_ids.contains(session.device_id.as_str())
        })
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let mut counts = HashMap::new();
    for message in messages
        .iter()
        .filter(|message| message.direction == "mobile-to-desktop" && !message.read)
    {
        if session_by_id.contains_key(message.session_id.as_str()) {
            *counts.entry(message.session_id.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn session_state_payload(session: &LanShareSession, unread_count: usize) -> Value {
    let mut payload = serde_json::to_value(session).unwrap_or_else(|_| json!({}));
    payload["unreadCount"] = json!(unread_count);
    payload
}

async fn fill_session_ips(
    paths: &AppPaths,
    devices: &[LanShareDevice],
    sessions: &mut Vec<LanShareSession>,
) -> Result<(), ManagerError> {
    let mut changed = false;

    for session in sessions.iter_mut() {
        if !session.ip.is_empty() {
            continue;
        }

        if let Some(device) = devices.iter().find(|device| device.id == session.device_id) {
            session.ip = device.ip.clone();
            changed = true;
        }
    }

    if changed {
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
    }

    Ok(())
}

fn device_display_name(device: &LanShareDevice) -> String {
    if device.name.is_empty() {
        device.auto_name.clone()
    } else {
        device.name.clone()
    }
}

fn find_device_for_session(
    paths: &AppPaths,
    device_id: &str,
) -> Result<LanShareDevice, ManagerError> {
    let devices: Vec<LanShareDevice> = read_array(&paths.lan_share_files.devices)?;

    Ok(devices
        .into_iter()
        .find(|device| device.id == device_id)
        .unwrap_or(LanShareDevice {
            id: device_id.to_string(),
            name: String::new(),
            auto_name: String::new(),
            user_agent: String::new(),
            ip: String::new(),
            first_seen_at: now_millis(),
            last_seen_at: now_millis(),
        }))
}

fn visible_session_ids_for_session(
    paths: &AppPaths,
    session_id: &str,
) -> Result<Vec<String>, ManagerError> {
    if session_id.is_empty() {
        return Ok(Vec::new());
    }

    let sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
    let Some(session) = sessions.iter().find(|session| session.id == session_id) else {
        return Ok(vec![session_id.to_string()]);
    };

    if session.mode != "group" || session.group_id.is_empty() {
        return Ok(vec![session_id.to_string()]);
    }
    let group_id = session.group_id.clone();

    Ok(sessions
        .into_iter()
        .filter(|item| item.group_id == group_id)
        .map(|item| item.id)
        .collect::<Vec<_>>())
}

fn group_session_by_id(
    paths: &AppPaths,
    session_id: &str,
) -> Result<Option<LanShareSession>, ManagerError> {
    if session_id.is_empty() {
        return Ok(None);
    }

    let sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;

    Ok(sessions
        .into_iter()
        .find(|session| session.id == session_id && session.mode == "group"))
}

fn find_group_session_by_member_session(
    paths: &AppPaths,
    session_id: &str,
    device_id: &str,
    device_name: &str,
    ip: &str,
) -> Result<Option<LanShareSession>, ManagerError> {
    if session_id.is_empty() {
        return Ok(None);
    }

    let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
    let Some(index) = sessions.iter().position(|session| {
        session.id == session_id && session.mode == "group" && session.device_id == device_id
    }) else {
        return Ok(None);
    };

    sessions[index].device_name = device_name.to_string();
    sessions[index].ip = ip.to_string();
    sessions[index].updated_at = now_millis();
    let session = sessions[index].clone();
    sort_sessions(&mut sessions);
    write_json_blocking(&paths.lan_share_files.sessions, &json!(sessions))?;
    Ok(Some(session))
}

async fn group_online_targets(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    group_id: &str,
) -> Result<Vec<(String, mpsc::UnboundedSender<WsOutbound>)>, ManagerError> {
    let sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
    let device_ids = sessions
        .into_iter()
        .filter(|session| session.group_id == group_id && !session.device_id.is_empty())
        .map(|session| session.device_id)
        .collect::<Vec<_>>();
    let runtime = registry.inner.lock().await;

    Ok(device_ids
        .into_iter()
        .filter_map(|device_id| {
            runtime
                .clients
                .get(&device_id)
                .cloned()
                .map(|sender| (device_id, sender))
        })
        .collect::<Vec<_>>())
}

fn group_messages_for_member(
    messages: &[LanShareMessage],
    group_session_ids: &[String],
    session: &LanShareSession,
) -> Vec<LanShareMessage> {
    let mut messages = messages
        .iter()
        .filter(|message| group_session_ids.contains(&message.session_id))
        .cloned()
        .collect::<Vec<_>>();

    messages.sort_by(|left, right| left.created_at.cmp(&right.created_at));

    match session.message_visibility.as_str() {
        "afterJoin" => messages
            .into_iter()
            .filter(|message| message.created_at >= session.joined_at)
            .collect::<Vec<_>>(),
        "recent10" => {
            let keep_from = messages.len().saturating_sub(10);

            messages.into_iter().skip(keep_from).collect::<Vec<_>>()
        }
        _ => messages,
    }
}

fn upsert_group_session_blocking(
    paths: &AppPaths,
    group: &mut LanShareGroup,
    device: &LanShareDevice,
    ip: &str,
) -> Result<LanShareSession, ManagerError> {
    let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
    let session = upsert_group_session(&mut sessions, group, device, ip);
    write_json_blocking(&paths.lan_share_files.sessions, &json!(sessions))?;
    Ok(session)
}

fn upsert_group_session(
    sessions: &mut Vec<LanShareSession>,
    group: &mut LanShareGroup,
    device: &LanShareDevice,
    ip: &str,
) -> LanShareSession {
    let now = now_millis();
    let device_name = device_display_name(device);

    if let Some(member) = group
        .members
        .iter_mut()
        .find(|member| member.device_id == device.id)
    {
        member.device_name = device_name.clone();
        member.ip = ip.to_string();
        member.last_seen_at = now;
        member.active = true;
    } else {
        group.members.push(LanShareGroupMember {
            device_id: device.id.clone(),
            device_name: device_name.clone(),
            ip: ip.to_string(),
            joined_at: now,
            last_seen_at: now,
            active: true,
        });
    }

    group.updated_at = now;
    let member_joined_at = group
        .members
        .iter()
        .find(|member| member.device_id == device.id)
        .map(|member| member.joined_at)
        .unwrap_or(now);

    if let Some(session) = sessions.iter_mut().find(|session| {
        session.mode == "group" && session.group_id == group.id && session.device_id == device.id
    }) {
        session.device_name = device_name;
        session.ip = ip.to_string();
        session.group_name = group.name.clone();
        session.message_visibility = group.message_visibility.clone();
        session.updated_at = now;
        let next_session = session.clone();
        sort_sessions(sessions);
        return next_session;
    }

    let session = LanShareSession {
        id: create_id(),
        device_id: device.id.clone(),
        device_name,
        ip: ip.to_string(),
        mode: "group".to_string(),
        group_id: group.id.clone(),
        group_name: group.name.clone(),
        message_visibility: group.message_visibility.clone(),
        joined_at: member_joined_at,
        created_at: now,
        updated_at: now,
    };

    sessions.insert(0, session.clone());
    sort_sessions(sessions);
    session
}

pub async fn create_session(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let device_id = string_value(payload.get("deviceId"));

    if device_id.is_empty() {
        return Err(ManagerError::System("设备不能为空。".to_string()));
    }

    let session = {
        let _storage = registry.storage.lock().await;
        let device = find_device_for_session(paths, &device_id)?;
        let device_name = device_display_name(&device);
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let now = now_millis();
        let session = LanShareSession {
            id: create_id(),
            device_id: device_id.to_string(),
            device_name,
            ip: device.ip,
            mode: "direct".to_string(),
            group_id: String::new(),
            group_name: String::new(),
            message_visibility: default_message_visibility(),
            joined_at: now,
            created_at: now,
            updated_at: now,
        };

        sessions.insert(0, session.clone());
        sort_sessions(&mut sessions);
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        session
    };
    registry
        .inner
        .lock()
        .await
        .active_sessions
        .insert(device_id, session.id.clone());

    let mut state = get_state(registry, paths).await?;
    state["data"]["currentSession"] = json!(session);
    Ok(state)
}

pub async fn activate_session(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let session_id = string_value(payload.get("sessionId"));

    if session_id.is_empty() {
        return Err(ManagerError::System("会话不能为空。".to_string()));
    }

    let current_session = {
        let _storage = registry.storage.lock().await;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let Some(index) = sessions.iter().position(|session| session.id == session_id) else {
            return Err(ManagerError::System("会话不存在。".to_string()));
        };

        sessions[index].updated_at = now_millis();
        let current_session = sessions[index].clone();
        sort_sessions(&mut sessions);
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        let visible_ids = if current_session.mode == "group" {
            sessions
                .iter()
                .filter(|session| session.group_id == current_session.group_id)
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>()
        } else {
            vec![current_session.id.as_str()]
        };
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let mut changed = false;
        for message in messages.iter_mut() {
            if message.direction == "mobile-to-desktop"
                && !message.read
                && visible_ids.contains(&message.session_id.as_str())
            {
                message.read = true;
                changed = true;
            }
        }
        if changed {
            write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
        }
        current_session
    };
    registry.inner.lock().await.active_sessions.insert(
        current_session.device_id.clone(),
        current_session.id.clone(),
    );

    let mut state = get_state(registry, paths).await?;
    state["data"]["currentSession"] = json!(current_session);
    Ok(state)
}

fn write_json_blocking(path: &str, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )?;
    Ok(())
}

pub async fn append_message(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    device_id: &str,
    session_id: &str,
    direction: &str,
    content: &str,
    delivered: bool,
) -> Result<LanShareMessage, ManagerError> {
    append_message_with_attachments(
        registry,
        paths,
        device_id,
        session_id,
        direction,
        content,
        delivered,
        &[],
        None,
    )
    .await
}

async fn append_message_with_attachments(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    device_id: &str,
    session_id: &str,
    direction: &str,
    content: &str,
    delivered: bool,
    files: &[LanShareFile],
    message_id: Option<&str>,
) -> Result<LanShareMessage, ManagerError> {
    let content = content.trim().to_string();

    if content.is_empty() && files.is_empty() {
        return Err(ManagerError::System("消息内容不能为空。".to_string()));
    }

    let selected_session_id = if session_id.is_empty() && direction == "desktop-to-mobile" {
        registry
            .inner
            .lock()
            .await
            .active_sessions
            .get(device_id)
            .cloned()
            .unwrap_or_default()
    } else {
        session_id.to_string()
    };
    let _storage = registry.storage.lock().await;
    let device = find_device_for_session(paths, device_id)?;
    if registry
        .inner
        .lock()
        .await
        .native
        .config
        .ignored_peers
        .iter()
        .any(|id| id == device_id)
    {
        return Err(ManagerError::System(
            "设备已删除，请重新连接后发送。".into(),
        ));
    }
    let device_name = device_display_name(&device);
    let session = if let Some(group_session) = group_session_by_id(paths, &selected_session_id)? {
        group_session
    } else if let Some(group_session) = find_group_session_by_member_session(
        paths,
        &selected_session_id,
        device_id,
        &device_name,
        &device.ip,
    )? {
        group_session
    } else {
        ensure_session(
            paths,
            device_id,
            &selected_session_id,
            &device_name,
            &device.ip,
        )?
    };
    let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
    if let Some(existing) =
        message_id.and_then(|id| messages.iter().find(|message| message.id == id))
    {
        return Ok(existing.clone());
    }
    let message = LanShareMessage {
        id: message_id
            .map(str::to_string)
            .unwrap_or_else(|| create_id()),
        session_id: session.id,
        device_id: device_id.to_string(),
        device_name,
        direction: direction.to_string(),
        message_type: if files.is_empty() { "text" } else { "file" }.to_string(),
        attachments: files.iter().map(attachments::Attachment::from).collect(),
        content,
        created_at: now_millis(),
        delivered,
        read: false,
    };

    messages.insert(0, message.clone());
    messages.truncate(1000);
    write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
    Ok(message)
}

pub async fn broadcast_files_changed(
    registry: &LanShareServerRegistry,
) -> Result<(), ManagerError> {
    let senders = {
        let runtime = registry.inner.lock().await;
        runtime.clients.values().cloned().collect::<Vec<_>>()
    };

    for sender in senders {
        let _ = sender.send(WsOutbound {
            payload: json!({ "type": "filesChanged" }),
        });
    }

    Ok(())
}

fn send_stored_message_to_client(
    sender: &mpsc::UnboundedSender<WsOutbound>,
    message: &LanShareMessage,
) -> bool {
    sender
        .send(WsOutbound {
            payload: json!({ "type": "message", "message": message }),
        })
        .is_ok()
}

pub async fn send_message(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let content = string_value(payload.get("content"));
    let target_device_id = string_value(payload.get("deviceId"));
    let target_session_id = string_value(payload.get("sessionId"));
    let is_trusted_peer = registry
        .inner
        .lock()
        .await
        .native
        .config
        .peers
        .iter()
        .any(|trusted| trusted.peer.id == target_device_id);
    let is_native = is_trusted_peer
        || target_device_id.starts_with("native-")
        || target_device_id.starts_with("native_")
        || read_array::<LanShareDevice>(&paths.lan_share_files.devices)?
            .iter()
            .any(|device| {
                device.id == target_device_id && device.user_agent == "MonkeyThief/Desktop"
            });
    if is_native && is_trusted_peer {
        native::ensure_send_allowed(registry, &target_device_id).await?;
    }
    if is_native && !is_trusted_peer {
        return Err(ManagerError::System("请先连接并配对这台设备。".into()));
    }
    let files = attachments::prepare(registry, paths, &target_session_id, &payload).await?;
    if content.is_empty() && files.is_empty() {
        return Err(ManagerError::System("请填写消息或添加附件。".into()));
    }
    let requested_id = string_value(payload.get("messageId"));
    let message_id = if requested_id.is_empty() {
        create_id()
    } else {
        requested_id
    };
    if let Some(group_session) = group_session_by_id(paths, &target_session_id)? {
        let targets = group_online_targets(registry, paths, &group_session.group_id).await?;
        let native_deliveries = native::deliver_group(
            registry,
            paths,
            &group_session.group_id,
            &content,
            &files,
            &message_id,
            Some(&app),
        )
        .await?;
        let stored = append_message_with_attachments(
            registry,
            paths,
            if group_session.device_id.is_empty() {
                "desktop"
            } else {
                &group_session.device_id
            },
            &group_session.id,
            "desktop-to-mobile",
            &content,
            native_deliveries > 0 || !targets.is_empty(),
            &files,
            Some(&message_id),
        )
        .await?;
        attachments::publish(registry, paths, &files).await?;
        for (_, sender) in targets {
            let _ = send_stored_message_to_client(&sender, &stored);
        }
        emit_message_created(&app, &stored);
        emit_state_changed(&app, registry, paths).await?;
        return Ok(lan_share_response(json!([stored])));
    }
    let native_delivery = if is_native {
        match native::deliver(
            registry,
            paths,
            &target_device_id,
            &content,
            &files,
            &message_id,
            Some(&app),
        )
        .await
        {
            Ok(delivered) => delivered,
            Err(error) if native::is_remote_deleted_error(&error) => return Err(error),
            Err(_) => false,
        }
    } else {
        false
    };
    let targets = {
        let runtime = registry.inner.lock().await;
        if target_device_id.is_empty() {
            runtime
                .clients
                .iter()
                .map(|(device_id, sender)| (device_id.clone(), Some(sender.clone())))
                .collect::<Vec<_>>()
        } else {
            vec![(
                target_device_id.clone(),
                runtime.clients.get(&target_device_id).cloned(),
            )]
        }
    };
    let mut sent_messages = Vec::new();
    for (device_id, sender) in targets {
        let stored_id = if device_id == target_device_id {
            message_id.clone()
        } else {
            create_stable_id(&format!("lan-message:{message_id}:{device_id}"))
        };
        let stored = append_message_with_attachments(
            registry,
            paths,
            &device_id,
            if device_id == target_device_id {
                &target_session_id
            } else {
                ""
            },
            "desktop-to-mobile",
            &content,
            native_delivery || sender.is_some(),
            &files,
            Some(&stored_id),
        )
        .await?;
        attachments::publish(registry, paths, &files).await?;
        if let Some(sender) = sender {
            let _ = send_stored_message_to_client(&sender, &stored);
        }
        emit_message_created(&app, &stored);
        sent_messages.push(stored);
    }
    emit_state_changed(&app, registry, paths).await?;
    Ok(lan_share_response(json!(sent_messages)))
}

pub async fn retry_message(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let message_id = string_value(payload.get("messageId"));
    if message_id.is_empty() {
        return Err(ManagerError::System("缺少消息 ID。".into()));
    }
    let (message, files) = {
        let _storage = registry.storage.lock().await;
        let messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let msg = messages
            .into_iter()
            .find(|m| m.id == message_id)
            .ok_or_else(|| ManagerError::System("消息不存在。".into()))?;
        if msg.delivered {
            return Ok(lan_share_response(json!(msg)));
        }
        let all_files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;
        let files = msg
            .attachments
            .iter()
            .filter_map(|att| all_files.iter().find(|f| f.id == att.id).cloned())
            .collect::<Vec<_>>();
        (msg, files)
    };

    native::ensure_send_allowed(registry, &message.device_id).await?;

    let delivered = native::deliver(
        registry,
        paths,
        &message.device_id,
        &message.content,
        &files,
        &message.id,
        Some(&app),
    )
    .await?;
    if !delivered {
        return Err(ManagerError::System("对方设备离线或未配对。".into()));
    }

    let updated_message = {
        let _storage = registry.storage.lock().await;
        let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
        let mut updated = message.clone();
        if let Some(m) = messages.iter_mut().find(|m| m.id == message_id) {
            m.delivered = true;
            updated = m.clone();
        }
        write_json(&paths.lan_share_files.messages, &json!(messages)).await?;
        updated
    };

    emit_state_changed(&app, registry, paths).await?;
    Ok(lan_share_response(json!(updated_message)))
}

pub(super) async fn deliver_pending_messages_for_peer(
    app: &tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    device_id: &str,
) {
    let pending_items = {
        let _storage = registry.storage.lock().await;
        let Ok(messages) = read_array::<LanShareMessage>(&paths.lan_share_files.messages) else {
            return;
        };
        let Ok(all_files) = read_array::<LanShareFile>(&paths.lan_share_files.files) else {
            return;
        };
        messages
            .into_iter()
            .filter(|m| {
                m.device_id == device_id && !m.delivered && m.direction == "desktop-to-mobile"
            })
            .map(|m| {
                let files = m
                    .attachments
                    .iter()
                    .filter_map(|att| all_files.iter().find(|f| f.id == att.id).cloned())
                    .collect::<Vec<_>>();
                (m, files)
            })
            .collect::<Vec<_>>()
    };

    if pending_items.is_empty() {
        return;
    }

    let mut delivered_ids = Vec::new();
    for (m, files) in pending_items {
        match native::deliver(registry, paths, device_id, &m.content, &files, &m.id, Some(app)).await {
            Ok(true) => delivered_ids.push(m.id),
            _ => break,
        }
    }

    if !delivered_ids.is_empty() {
        let _storage = registry.storage.lock().await;
        if let Ok(mut messages) = read_array::<LanShareMessage>(&paths.lan_share_files.messages) {
            for m in messages.iter_mut() {
                if delivered_ids.contains(&m.id) {
                    m.delivered = true;
                }
            }
            let _ = write_json(&paths.lan_share_files.messages, &json!(messages)).await;
        }
        // 状态刷新会重新读取存储，必须先释放补发结果的写入锁。
        drop(_storage);
        let _ = emit_state_changed(app, registry, paths).await;
    }
}

pub async fn set_hidden(
    app: tauri::AppHandle,
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let hidden = payload
        .get("hidden")
        .and_then(Value::as_bool)
        .ok_or_else(|| ManagerError::System("缺少节点可见性设置。".into()))?;
    let result = native::set_hidden(registry, paths, hidden).await?;
    emit_state_changed(&app, registry, paths).await?;
    Ok(lan_share_response(result))
}

pub async fn scan_devices(registry: &LanShareServerRegistry) -> Result<Value, ManagerError> {
    let result = native::scan_nearby(registry).await?;
    Ok(lan_share_response(result))
}

async fn handle_http_request(
    request: Request<Incoming>,
    context: HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, Infallible> {
    let origin = request.headers().get("origin").cloned();
    let mut response = match process_http_request(request, context, addr).await {
        Ok(response) => response,
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };

    if let Some(origin) = origin.filter(|value| {
        value
            .to_str()
            .ok()
            .and_then(|value| Url::parse(value).ok())
            .map(|url| {
                matches!(
                    url.host_str(),
                    Some("tauri.localhost" | "localhost" | "127.0.0.1")
                )
            })
            .unwrap_or(false)
    }) {
        response
            .headers_mut()
            .insert("access-control-allow-origin", origin);
        response.headers_mut().insert(
            "access-control-allow-methods",
            HeaderValue::from_static("GET, PUT, POST, OPTIONS"),
        );
        response.headers_mut().insert(
            "access-control-allow-headers",
            HeaderValue::from_static("content-type"),
        );
        response
            .headers_mut()
            .insert("vary", HeaderValue::from_static("Origin"));
    }

    Ok(response)
}

async fn process_http_request(
    request: Request<Incoming>,
    context: HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let path = request.uri().path().to_string();
    if request.method() == Method::OPTIONS {
        return Ok(response(StatusCode::NO_CONTENT, "text/plain", ""));
    }
    if path.starts_with("/api/native/") {
        return native::handle_request(request, &context, addr).await;
    }
    let request_token = if request.method() == Method::POST
        && (path == "/api/devices"
            || path == "/api/sessions"
            || path == "/api/groups/join"
            || path == "/api/groups/leave")
    {
        String::new()
    } else {
        query_value(request.uri(), "token")
    };

    if request.method() != Method::POST
        || (path != "/api/devices"
            && path != "/api/sessions"
            && path != "/api/groups/join"
            && path != "/api/groups/leave")
    {
        if !is_request_token_valid(&context.registry, &request_token).await {
            return Ok(api_error(
                StatusCode::UNAUTHORIZED,
                "访问已失效，请重新扫码。",
            ));
        }
    }

    match (request.method(), path.as_str()) {
        (&Method::GET, "/") => Ok(mobile_page_response()),
        (&Method::GET, "/api/files") => Ok(api_success(
            mobile_files_payload(
                &context.registry,
                &context.paths,
                &query_value(request.uri(), "sessionId"),
            )
            .await?,
        )),
        (&Method::GET, "/api/files/download") => download_file(request.uri(), &context, addr).await,
        (&Method::GET, "/api/files/preview") => preview_file(request.uri(), &context, addr).await,
        (&Method::PUT, "/api/files/upload") => {
            let session_id = query_value(request.uri(), "sessionId");
            let name = query_value(request.uri(), "name");
            let file = attachments::upload(
                request.into_body(),
                &context.registry,
                &context.paths,
                &session_id,
                &name,
            )
            .await?;
            Ok(api_success(json!(attachments::Attachment::from(&file))))
        }
        (&Method::GET, "/api/messages") => mobile_messages(request.uri(), &context, addr).await,
        (&Method::POST, "/api/messages") => {
            let body = http_body_util::Limited::new(request.into_body(), 256 * 1024)
                .collect()
                .await
                .map_err(|_| ManagerError::System("消息过大或接收中断。".into()))?
                .to_bytes();
            let payload: Value = serde_json::from_slice(&body)?;
            let stored =
                store_guest_message(&context, &device_id_from_ip(&client_ip(addr)), &payload)
                    .await?;
            Ok(api_success(json!(stored)))
        }
        (&Method::GET, "/api/groups") => mobile_groups(request.uri(), &context, addr).await,
        (&Method::POST, "/api/devices") => register_device_request(request, &context, addr).await,
        (&Method::POST, "/api/sessions") => create_mobile_session(request, &context, addr).await,
        (&Method::POST, "/api/groups/join") => join_mobile_group(request, &context, addr).await,
        (&Method::POST, "/api/groups/leave") => leave_mobile_group(request, &context, addr).await,
        (&Method::GET, "/ws") => websocket_response(request, context, addr).await,
        _ => Ok(api_error(StatusCode::NOT_FOUND, "请求路径不存在。")),
    }
}

async fn mobile_files_payload(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    session_id: &str,
) -> Result<Value, ManagerError> {
    if session_id.is_empty() {
        return Ok(json!([]));
    }

    let session_scope_ids = visible_session_ids_for_session(paths, session_id)?;
    let files = {
        let _storage = registry.storage.lock().await;
        read_array::<LanShareFile>(&paths.lan_share_files.files)?
            .into_iter()
            .filter(|file| file.enabled && session_scope_ids.contains(&file.session_id))
            .map(|file| {
                json!({
                    "id": file.id,
                    "sessionId": file.session_id,
                    "name": file.name,
                    "size": file.size,
                    "mimeType": file.mime_type,
                    "updatedAt": file.updated_at
                })
            })
            .collect::<Vec<_>>()
    };

    Ok(json!(files))
}

async fn download_file(
    uri: &hyper::Uri,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let file_id = query_value(uri, "id");
    let device_id = query_value(uri, "deviceId");
    let session_id = query_value(uri, "sessionId");
    let session_scope_ids = visible_session_ids_for_session(&context.paths, &session_id)?;
    let file = {
        let _storage = context.registry.storage.lock().await;
        read_array::<LanShareFile>(&context.paths.lan_share_files.files)?
            .into_iter()
            .find(|file| {
                file.id == file_id
                    && file.enabled
                    && (session_id.is_empty() || session_scope_ids.contains(&file.session_id))
            })
    };
    let Some(file) = file else {
        return Ok(api_error(StatusCode::NOT_FOUND, "文件不存在或未共享。"));
    };
    let stream_file = File::open(&file.path).await?;
    let metadata = stream_file.metadata().await?;
    let stream = context
        .registry
        .transfer
        .file_stream(stream_file)
        .await
        .map_ok(Frame::data);
    let body = BodyExt::boxed(StreamBody::new(stream));
    append_download_record(
        &context.registry,
        &context.paths,
        &file,
        &device_id,
        &client_ip(addr),
    )
    .await?;
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, file.mime_type.clone())
        .header(CONTENT_LENGTH, metadata.len().to_string())
        .header(
            CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"",
                safe_header_file_name(&file.name).replace('"', "'")
            ),
        )
        .body(body)
        .unwrap_or_else(|_| Response::new(boxed_full("response build failed")));

    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn preview_file(
    uri: &hyper::Uri,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let file_id = query_value(uri, "id");
    let device_id = query_value(uri, "deviceId");
    let session_id = query_value(uri, "sessionId");
    let session_scope_ids = visible_session_ids_for_session(&context.paths, &session_id)?;
    let file = {
        let _storage = context.registry.storage.lock().await;
        read_array::<LanShareFile>(&context.paths.lan_share_files.files)?
            .into_iter()
            .find(|file| {
                file.id == file_id
                    && file.enabled
                    && (session_id.is_empty() || session_scope_ids.contains(&file.session_id))
            })
    };
    let Some(file) = file else {
        return Ok(api_error(StatusCode::NOT_FOUND, "文件不存在或未共享。"));
    };

    if !is_previewable_file(&file) {
        return Ok(api_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "当前文件类型不支持在线预览，请下载后打开。",
        ));
    }

    let stream_file = File::open(&file.path).await?;
    let metadata = stream_file.metadata().await?;
    let stream = context
        .registry
        .transfer
        .file_stream(stream_file)
        .await
        .map_ok(Frame::data);
    let body = BodyExt::boxed(StreamBody::new(stream));
    append_download_record(
        &context.registry,
        &context.paths,
        &file,
        &device_id,
        &client_ip(addr),
    )
    .await?;
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, file.mime_type.clone())
        .header(CONTENT_LENGTH, metadata.len().to_string())
        .header(
            CONTENT_DISPOSITION,
            format!(
                "inline; filename=\"{}\"",
                safe_header_file_name(&file.name).replace('"', "'")
            ),
        )
        .body(body)
        .unwrap_or_else(|_| Response::new(boxed_full("response build failed")));

    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn mobile_messages(
    uri: &hyper::Uri,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let request_ip = client_ip(addr);
    let requested_device_id = query_value(uri, "deviceId");
    let device_id = if request_ip.is_empty() {
        requested_device_id
    } else {
        device_id_from_ip(&request_ip)
    };
    let session_id = query_value(uri, "sessionId");

    if device_id.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "设备标识不能为空。"));
    }

    let (session, messages) = {
        let _storage = context.registry.storage.lock().await;
        let device = find_device_for_session(&context.paths, &device_id)?;
        let device_name = device_display_name(&device);
        let session = ensure_session(
            &context.paths,
            &device_id,
            &session_id,
            &device_name,
            &request_ip,
        )?;
        let mut messages: Vec<LanShareMessage> =
            read_array(&context.paths.lan_share_files.messages)?;

        if session.mode == "group" {
            let sessions: Vec<LanShareSession> =
                read_array(&context.paths.lan_share_files.sessions)?;
            let group_session_ids = sessions
                .iter()
                .filter(|item| item.group_id == session.group_id)
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();

            messages = group_messages_for_member(&messages, &group_session_ids, &session);
        } else {
            messages.retain(|message| message.session_id == session.id);
            messages.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        }
        (session, messages)
    };
    context
        .registry
        .inner
        .lock()
        .await
        .active_sessions
        .insert(device_id, session.id.clone());

    Ok(api_success(json!({
        "currentSession": session,
        "messages": messages
    })))
}

async fn mobile_groups(
    uri: &hyper::Uri,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let request_ip = client_ip(addr);
    let requested_device_id = query_value(uri, "deviceId");
    let device_id = if request_ip.is_empty() {
        requested_device_id
    } else {
        device_id_from_ip(&request_ip)
    };

    if device_id.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "设备标识不能为空。"));
    }

    let groups = {
        let _storage = context.registry.storage.lock().await;
        let groups: Vec<LanShareGroup> = read_array(&context.paths.lan_share_files.groups)?;
        let sessions: Vec<LanShareSession> = read_array(&context.paths.lan_share_files.sessions)?;

        groups
            .into_iter()
            .filter_map(|group| {
                let session = sessions.iter().find(|session| {
                    session.group_id == group.id
                        && session.device_id == device_id
                        && session.mode == "group"
                })?;

                Some(json!({
                    "id": group.id,
                    "name": group.name,
                    "inviteCode": group.invite_code,
                    "messageVisibility": group.message_visibility,
                    "sessionId": session.id,
                    "joinedAt": session.joined_at
                }))
            })
            .collect::<Vec<_>>()
    };

    Ok(api_success(json!(groups)))
}

async fn append_download_record(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    file: &LanShareFile,
    device_id: &str,
    ip: &str,
) -> Result<(), ManagerError> {
    let _storage = registry.storage.lock().await;
    let canonical_device_id = if ip.is_empty() {
        device_id.to_string()
    } else {
        device_id_from_ip(ip)
    };
    let device = read_array::<LanShareDevice>(&paths.lan_share_files.devices)?
        .into_iter()
        .find(|device| device.id == canonical_device_id)
        .unwrap_or(LanShareDevice {
            id: canonical_device_id,
            name: String::new(),
            auto_name: String::new(),
            user_agent: String::new(),
            ip: ip.to_string(),
            first_seen_at: now_millis(),
            last_seen_at: now_millis(),
        });
    let mut downloads = read_array::<LanShareDownload>(&paths.lan_share_files.downloads)?;

    downloads.insert(
        0,
        LanShareDownload {
            id: create_id(),
            file_id: file.id.clone(),
            file_name: file.name.clone(),
            device_id: device.id,
            device_name: device.name,
            ip: ip.to_string(),
            created_at: now_millis(),
        },
    );
    downloads.truncate(1000);
    write_json(&paths.lan_share_files.downloads, &json!(downloads)).await
}

async fn create_mobile_session(
    request: Request<Incoming>,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let body = request
        .into_body()
        .collect()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body)?;
    let token = string_value(payload.get("token"));

    if !is_request_token_valid(&context.registry, &token).await {
        return Ok(api_error(
            StatusCode::UNAUTHORIZED,
            "访问已失效，请重新扫码。",
        ));
    }

    let ip = client_ip(addr);
    let device_id = if ip.is_empty() {
        string_value(payload.get("deviceId"))
    } else {
        device_id_from_ip(&ip)
    };

    if device_id.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "设备标识不能为空。"));
    }

    let device_name = normalize_device_name(&string_value(payload.get("deviceName")));
    let user_agent = string_value(payload.get("userAgent"));
    let device = upsert_device(
        &context.registry,
        &context.paths,
        &device_id,
        &device_name,
        &user_agent,
        &ip,
    )
    .await?;
    let state = create_session(
        &context.registry,
        &context.paths,
        json!({ "deviceId": device.id }),
    )
    .await?;
    let current_session = state["data"]["currentSession"].clone();

    emit_state_changed(&context.app, &context.registry, &context.paths).await?;

    Ok(api_success(json!({
        "device": device,
        "currentSession": current_session,
        "messages": [],
        "files": []
    })))
}

async fn join_mobile_group(
    request: Request<Incoming>,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let body = request
        .into_body()
        .collect()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body)?;
    let token = string_value(payload.get("token"));

    if !is_request_token_valid(&context.registry, &token).await {
        return Ok(api_error(
            StatusCode::UNAUTHORIZED,
            "访问已失效，请重新扫码。",
        ));
    }

    let invite_code = string_value(payload.get("inviteCode")).to_uppercase();

    if invite_code.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "请输入群邀请码。"));
    }

    let ip = client_ip(addr);
    let requested_device_id = string_value(payload.get("deviceId"));
    let device_id = if ip.is_empty() {
        requested_device_id
    } else {
        device_id_from_ip(&ip)
    };

    if device_id.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "设备标识不能为空。"));
    }

    let device_name = normalize_device_name(&string_value(payload.get("deviceName")));
    let user_agent = string_value(payload.get("userAgent"));
    let device = upsert_device(
        &context.registry,
        &context.paths,
        &device_id,
        &device_name,
        &user_agent,
        &ip,
    )
    .await?;
    let (group, session, messages) = {
        let _storage = context.registry.storage.lock().await;
        let mut groups: Vec<LanShareGroup> = read_array(&context.paths.lan_share_files.groups)?;
        let Some(group_index) = groups
            .iter()
            .position(|group| group.invite_code.eq_ignore_ascii_case(&invite_code))
        else {
            return Ok(api_error(StatusCode::NOT_FOUND, "群邀请码不存在。"));
        };

        let session =
            upsert_group_session_blocking(&context.paths, &mut groups[group_index], &device, &ip)?;
        let group = groups[group_index].clone();
        write_json(&context.paths.lan_share_files.groups, &json!(groups)).await?;
        let all_messages: Vec<LanShareMessage> =
            read_array(&context.paths.lan_share_files.messages)?;
        let sessions: Vec<LanShareSession> = read_array(&context.paths.lan_share_files.sessions)?;
        let group_session_ids = sessions
            .iter()
            .filter(|item| item.group_id == group.id)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        let messages = group_messages_for_member(&all_messages, &group_session_ids, &session);

        (group, session, messages)
    };

    context
        .registry
        .inner
        .lock()
        .await
        .active_sessions
        .insert(device.id.clone(), session.id.clone());
    emit_state_changed(&context.app, &context.registry, &context.paths).await?;

    Ok(api_success(json!({
        "device": device,
        "group": group,
        "currentSession": session,
        "messages": messages,
        "files": []
    })))
}

async fn leave_mobile_group(
    request: Request<Incoming>,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let body = request
        .into_body()
        .collect()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body)?;
    let token = string_value(payload.get("token"));

    if !is_request_token_valid(&context.registry, &token).await {
        return Ok(api_error(
            StatusCode::UNAUTHORIZED,
            "访问已失效，请重新扫码。",
        ));
    }

    let group_id = string_value(payload.get("groupId"));

    if group_id.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "群聊不能为空。"));
    }

    let local_owner_id = context.registry.inner.lock().await.native.config.id.clone();

    let ip = client_ip(addr);
    let requested_device_id = string_value(payload.get("deviceId"));
    let device_id = if ip.is_empty() {
        requested_device_id
    } else {
        device_id_from_ip(&ip)
    };

    if device_id.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "设备标识不能为空。"));
    }

    {
        let _storage = context.registry.storage.lock().await;
        let mut groups: Vec<LanShareGroup> = read_array(&context.paths.lan_share_files.groups)?;
        let Some(group) = groups.iter_mut().find(|group| group.id == group_id) else {
            return Ok(api_error(StatusCode::NOT_FOUND, "群聊不存在。"));
        };

        let owner_id = if group.owner_device_id.is_empty() {
            local_owner_id.clone()
        } else {
            group.owner_device_id.clone()
        };
        if !owner_id.is_empty() && device_id == owner_id {
            return Ok(api_error(
                StatusCode::CONFLICT,
                "群主不能退出群聊，请由群主解散群聊。",
            ));
        }

        let mut removed_session_ids = Vec::new();
        let sessions_before: Vec<LanShareSession> =
            read_array(&context.paths.lan_share_files.sessions)?;
        for session in sessions_before.iter().filter(|session| {
            session.mode == "group" && session.group_id == group_id && session.device_id == device_id
        }) {
            removed_session_ids.push(session.id.clone());
        }

        group.members.retain(|member| member.device_id != device_id);
        group.updated_at = now_millis();

        let mut sessions = sessions_before;
        sessions.retain(|session| {
            !(session.mode == "group"
                && session.group_id == group_id
                && session.device_id == device_id)
        });

        // 访客退出后彻底清除其本机群聊缓存，避免退出后仍能通过旧会话读取历史。
        let mut messages: Vec<LanShareMessage> =
            read_array(&context.paths.lan_share_files.messages)?;
        messages.retain(|message| !removed_session_ids.contains(&message.session_id));
        let mut files: Vec<LanShareFile> = read_array(&context.paths.lan_share_files.files)?;
        let removed_files = files
            .iter()
            .filter(|file| removed_session_ids.contains(&file.session_id))
            .cloned()
            .collect::<Vec<_>>();
        files.retain(|file| !removed_session_ids.contains(&file.session_id));

        // 仅删除快传上传目录中的缓存，用户原始文件路径不会被退出操作触碰。
        let uploads = Path::new(&context.paths.lan_share_dir).join("uploads");
        if let Ok(directory) = tokio::fs::canonicalize(&uploads).await {
            for file in &removed_files {
                let Ok(path) = tokio::fs::canonicalize(&file.path).await else {
                    continue;
                };
                if path.parent() == Some(directory.as_path()) {
                    let _ = tokio::fs::remove_file(path).await;
                }
            }
        }

        write_json(&context.paths.lan_share_files.groups, &json!(groups)).await?;
        write_json(&context.paths.lan_share_files.sessions, &json!(sessions)).await?;
        write_json(&context.paths.lan_share_files.messages, &json!(messages)).await?;
        write_json(&context.paths.lan_share_files.files, &json!(files)).await?;
    }

    {
        let mut runtime = context.registry.inner.lock().await;
        if runtime
            .active_sessions
            .get(&device_id)
            .and_then(|session_id| {
                group_session_by_id(&context.paths, session_id)
                    .ok()
                    .flatten()
            })
            .map(|session| session.group_id == group_id)
            .unwrap_or(false)
        {
            runtime.active_sessions.remove(&device_id);
        }
    }

    emit_state_changed(&context.app, &context.registry, &context.paths).await?;
    Ok(api_success(json!(true)))
}

async fn register_device_request(
    request: Request<Incoming>,
    context: &HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let body = request
        .into_body()
        .collect()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body)?;
    let token = string_value(payload.get("token"));

    if !is_request_token_valid(&context.registry, &token).await {
        return Ok(api_error(
            StatusCode::UNAUTHORIZED,
            "访问已失效，请重新扫码。",
        ));
    }

    let ip = client_ip(addr);
    let device_id = string_value(payload.get("deviceId"));
    let device_name = normalize_device_name(&string_value(payload.get("deviceName")));
    let user_agent = string_value(payload.get("userAgent"));
    let device = upsert_device(
        &context.registry,
        &context.paths,
        &device_id,
        &device_name,
        &user_agent,
        &ip,
    )
    .await?;

    emit_devices_changed(&context.app, &context.registry, &context.paths).await?;
    emit_state_changed(&context.app, &context.registry, &context.paths).await?;

    Ok(api_success(json!(device)))
}

async fn websocket_response(
    request: Request<Incoming>,
    context: HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let token = query_value(request.uri(), "token");
    let device_id = query_value(request.uri(), "deviceId");

    if !is_request_token_valid(&context.registry, &token).await {
        return Ok(api_error(
            StatusCode::UNAUTHORIZED,
            "访问已失效，请重新扫码。",
        ));
    }

    if device_id.is_empty() {
        return Ok(api_error(StatusCode::BAD_REQUEST, "设备标识不能为空。"));
    }

    let Some(key) = request.headers().get(SEC_WEBSOCKET_KEY).cloned() else {
        return Ok(api_error(StatusCode::BAD_REQUEST, "缺少 WebSocket Key。"));
    };
    let accept_key = websocket_accept_key(key.to_str().unwrap_or_default());
    let upgraded = hyper::upgrade::on(request);
    let request_token = token.clone();
    let ip = client_ip(addr);
    let connection_registry = context.registry.clone();

    let connection_handle = tauri::async_runtime::spawn(async move {
        match upgraded.await {
            Ok(stream) => {
                if let Err(error) =
                    handle_websocket_connection(context, device_id, request_token, ip, stream).await
                {
                    eprintln!("{error}");
                }
            }
            Err(error) => eprintln!("{error}"),
        }
    });
    connection_registry
        .inner
        .lock()
        .await
        .connections
        .push(connection_handle);

    Ok(Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(UPGRADE, "websocket")
        .header(CONNECTION, "Upgrade")
        .header(SEC_WEBSOCKET_ACCEPT, accept_key)
        .header(SEC_WEBSOCKET_VERSION, "13")
        .body(boxed_full(Bytes::new()))
        .unwrap_or_else(|_| Response::new(boxed_full("response build failed"))))
}

fn websocket_accept_key(key: &str) -> String {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

async fn handle_websocket_connection(
    context: HttpContext,
    device_id: String,
    request_token: String,
    ip: String,
    stream: hyper::upgrade::Upgraded,
) -> Result<(), ManagerError> {
    let device_id = if ip.is_empty() {
        device_id
    } else {
        device_id_from_ip(&ip)
    };
    let stream = TokioIo::new(stream);
    let websocket = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
    let (mut writer, mut reader) = websocket.split();
    let (sender, mut receiver) = mpsc::unbounded_channel::<WsOutbound>();
    let client_sender = sender.clone();

    {
        let _storage = context.registry.storage.lock().await;
        let devices: Vec<LanShareDevice> = read_array(&context.paths.lan_share_files.devices)?;
        if !devices.iter().any(|device| device.id == device_id) {
            return Ok(());
        }
        let mut runtime = context.registry.inner.lock().await;
        if !is_valid_token(&runtime.token, &request_token) {
            return Ok(());
        }
        runtime.clients.insert(device_id.clone(), sender);
    }
    emit_state_changed(&context.app, &context.registry, &context.paths).await?;

    let writer_handle = tauri::async_runtime::spawn(async move {
        while let Some(outbound) = receiver.recv().await {
            let removed = outbound.payload["type"] == "deviceRemoved";
            if writer
                .send(Message::Text(outbound.payload.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
            if removed {
                let _ = writer.close().await;
                break;
            }
        }
    });

    loop {
        let message = reader
            .try_next()
            .await
            .map_err(|error| ManagerError::System(error.to_string()))?;
        let Some(message) = message else {
            break;
        };

        if !message.is_text() {
            continue;
        }
        if !is_request_token_valid(&context.registry, &request_token).await {
            break;
        }
        if !context
            .registry
            .inner
            .lock()
            .await
            .clients
            .get(&device_id)
            .map(|sender| sender.same_channel(&client_sender))
            .unwrap_or(false)
        {
            break;
        }

        let payload = serde_json::from_str::<Value>(&message.to_text().unwrap_or_default())
            .unwrap_or_else(|_| json!({}));

        if payload.get("type").and_then(Value::as_str) == Some("message") {
            if let Err(error) = store_guest_message(&context, &device_id, &payload).await {
                let _ = client_sender.send(WsOutbound {
                    payload: json!({ "type": "error", "message": error.to_string() }),
                });
            }
        }
    }

    writer_handle.abort();
    {
        let mut runtime = context.registry.inner.lock().await;
        if runtime
            .clients
            .get(&device_id)
            .map(|sender| sender.same_channel(&client_sender))
            .unwrap_or(false)
        {
            runtime.clients.remove(&device_id);
        }
    }
    emit_state_changed(&context.app, &context.registry, &context.paths).await?;
    Ok(())
}

async fn store_guest_message(
    context: &HttpContext,
    device_id: &str,
    payload: &Value,
) -> Result<LanShareMessage, ManagerError> {
    let session_id = string_value(payload.get("sessionId"));
    let session = read_array::<LanShareSession>(&context.paths.lan_share_files.sessions)?
        .into_iter()
        .find(|session| session.id == session_id && session.device_id == device_id)
        .ok_or_else(|| ManagerError::System("请先选择当前设备的有效会话。".into()))?;
    let content = string_value(payload.get("content"));
    let files = attachments::prepare(
        &context.registry,
        &context.paths,
        &session.id,
        &json!({ "attachmentIds": payload["attachmentIds"] }),
    )
    .await?;
    let requested_id = string_value(payload.get("messageId"));
    let message_id = if requested_id.is_empty() {
        create_id()
    } else {
        format!("{device_id}-{requested_id}")
    };
    let message = append_message_with_attachments(
        &context.registry,
        &context.paths,
        device_id,
        &session.id,
        "mobile-to-desktop",
        &content,
        true,
        &files,
        Some(&message_id),
    )
    .await?;
    attachments::publish(&context.registry, &context.paths, &files).await?;
    if let Some(group_session) = group_session_by_id(&context.paths, &message.session_id)? {
        for (_, sender) in
            group_online_targets(&context.registry, &context.paths, &group_session.group_id).await?
        {
            let _ = send_stored_message_to_client(&sender, &message);
        }
    } else if let Some(sender) = context.registry.inner.lock().await.clients.get(device_id) {
        let _ = send_stored_message_to_client(sender, &message);
    }
    emit_message_created(&context.app, &message);
    emit_state_changed(&context.app, &context.registry, &context.paths).await?;
    Ok(message)
}

#[cfg(test)]
mod message_tests {
    use super::{auto_device_name, normalize_device_name};

    #[test]
    fn creates_readable_auto_device_names() {
        assert_eq!(
            auto_device_name(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Version/17 Mobile Safari/604.1",
                "192.168.1.23"
            ),
            "iPhone Safari · 192.168.1.23"
        );
        assert_eq!(
            auto_device_name("Mozilla/5.0 Android Chrome/125", "192.168.1.35"),
            "Android Chrome · 192.168.1.35"
        );
    }

    #[test]
    fn normalizes_device_alias() {
        assert_eq!(normalize_device_name("  我的手机  "), "我的手机");
        assert_eq!(normalize_device_name(""), "");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        activate_session, add_files, append_message, create_file_id, create_group, create_id,
        create_session, delete_messages, delete_session, device_id_from_ip, export_files_zip,
        get_state, list_messages, mobile_files_payload, read_array, refresh_files, remove_files,
        string_value, upsert_device, LanShareDevice, LanShareFile, LanShareMessage,
        LanShareServerRegistry, LanShareSession,
    };
    use crate::core::paths::resolve_app_paths;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn deleting_device_optionally_purges_owned_files_and_keeps_originals() {
        tauri::async_runtime::block_on(async {
            for delete_history in [false, true] {
                let root = std::env::temp_dir().join(create_id());
                let paths = resolve_app_paths(&root);
                let registry = LanShareServerRegistry::new();
                let device = upsert_device(
                    &registry,
                    &paths,
                    "native-delete",
                    "对方",
                    "MonkeyThief/Desktop",
                    "192.168.1.8",
                )
                .await
                .unwrap();
                let state = create_session(&registry, &paths, json!({ "deviceId": device.id }))
                    .await
                    .unwrap();
                let session = state["data"]["currentSession"]["id"].as_str().unwrap();
                let uploads = Path::new(&paths.lan_share_dir).join("uploads");
                tokio::fs::create_dir_all(&uploads).await.unwrap();
                let received = uploads.join("received.txt");
                let source = root.join("original.txt");
                let partial = uploads.join(".lan-upload-partial.part");
                let checkpoint = uploads.join(".lan-upload-partial.json");
                for path in [&received, &source, &partial] {
                    tokio::fs::write(path, "聊天文件").await.unwrap();
                }
                super::write_json(
                    &checkpoint.to_string_lossy(),
                    &json!({ "sessionId": session, "nextIndex": 1 }),
                )
                .await
                .unwrap();
                let mut files = Vec::new();
                for path in [&received, &source] {
                    files.push(
                        super::file_payload(&path.to_string_lossy(), session)
                            .await
                            .unwrap(),
                    );
                }
                super::write_json(&paths.lan_share_files.files, &json!(files))
                    .await
                    .unwrap();
                super::append_message_with_attachments(
                    &registry,
                    &paths,
                    &device.id,
                    session,
                    "mobile-to-desktop",
                    "记录",
                    true,
                    &files,
                    None,
                )
                .await
                .unwrap();
                super::delete_device(
                    &registry,
                    &paths,
                    json!({ "deviceId": device.id, "deleteHistory": delete_history }),
                )
                .await
                .unwrap();
                // 删除设备但保留历史时，历史消息仍在，但不能继续显示为悬浮窗未读。
                let state = super::get_state(&registry, &paths).await.unwrap();
                assert_eq!(state["data"]["totalUnread"], 0);
                assert!(read_array::<LanShareDevice>(&paths.lan_share_files.devices)
                    .unwrap()
                    .is_empty());
                assert_eq!(received.exists(), !delete_history);
                assert_eq!(partial.exists(), !delete_history);
                assert_eq!(checkpoint.exists(), !delete_history);
                assert!(source.exists());
                assert_eq!(
                    read_array::<LanShareMessage>(&paths.lan_share_files.messages)
                        .unwrap()
                        .is_empty(),
                    delete_history
                );
                assert_eq!(
                    read_array::<LanShareFile>(&paths.lan_share_files.files)
                        .unwrap()
                        .is_empty(),
                    delete_history
                );
                assert_eq!(
                    read_array::<LanShareSession>(&paths.lan_share_files.sessions)
                        .unwrap()
                        .is_empty(),
                    delete_history
                );
                tokio::fs::remove_dir_all(root).await.unwrap();
            }
        });
    }

    #[test]
    fn delete_device_removes_direct_records_without_losing_shared_group_history() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            let removed = upsert_device(&registry, &paths, "", "待删除", "Chrome", "192.168.1.31")
                .await
                .unwrap();
            let kept = upsert_device(&registry, &paths, "", "保留", "Chrome", "192.168.1.32")
                .await
                .unwrap();
            let removed_session =
                create_session(&registry, &paths, json!({ "deviceId": removed.id }))
                    .await
                    .unwrap();
            let removed_session_id = removed_session["data"]["currentSession"]["id"]
                .as_str()
                .unwrap()
                .to_string();
            let kept_session = create_session(&registry, &paths, json!({ "deviceId": kept.id }))
                .await
                .unwrap();
            let kept_session_id = kept_session["data"]["currentSession"]["id"]
                .as_str()
                .unwrap()
                .to_string();
            let group = create_group(
                &registry,
                &paths,
                json!({ "name": "保留群聊", "deviceIds": [removed.id, kept.id] }),
            )
            .await
            .unwrap();
            let host_id = group["data"]["currentSession"]["id"]
                .as_str()
                .unwrap()
                .to_string();
            let sessions: Vec<LanShareSession> =
                read_array(&paths.lan_share_files.sessions).unwrap();
            let member_id = sessions
                .iter()
                .find(|session| session.device_id == removed.id && session.mode == "group")
                .unwrap()
                .id
                .clone();
            let original = root.join("original.txt");
            tokio::fs::write(&original, "保留磁盘上的文件")
                .await
                .unwrap();
            let mut fixture_messages = Vec::new();
            let mut fixture_files = Vec::new();
            for (identifier, session_id, device_id) in [
                (
                    "removed-direct",
                    removed_session_id.as_str(),
                    removed.id.as_str(),
                ),
                ("kept-direct", kept_session_id.as_str(), kept.id.as_str()),
                ("shared-group", member_id.as_str(), removed.id.as_str()),
            ] {
                fixture_messages.push(json!({ "id": identifier, "sessionId": session_id, "deviceId": device_id, "deviceName": "测试", "direction": "mobile-to-desktop", "messageType": "text", "content": identifier, "createdAt": 1, "delivered": true, "read": false }));
                fixture_files.push(json!({ "id": identifier, "sessionId": session_id, "path": original, "name": "original.txt", "size": 1, "mimeType": "text/plain", "updatedAt": 1, "enabled": true }));
            }
            super::write_json(&paths.lan_share_files.messages, &json!(fixture_messages))
                .await
                .unwrap();
            super::write_json(&paths.lan_share_files.files, &json!(fixture_files))
                .await
                .unwrap();
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            registry
                .inner
                .lock()
                .await
                .clients
                .insert(removed.id.clone(), sender);
            registry
                .inner
                .lock()
                .await
                .active_sessions
                .insert(removed.id.clone(), removed_session_id);
            let result = super::delete_device(
                &registry,
                &paths,
                json!({ "deviceId": removed.id, "deleteHistory": true }),
            )
            .await
            .unwrap();
            assert_eq!(result["data"]["devices"].as_array().unwrap().len(), 1);
            assert_eq!(result["data"]["devices"][0]["id"], kept.id);
            assert_eq!(
                receiver.try_recv().unwrap().payload["type"],
                "deviceRemoved"
            );
            assert!(!registry
                .inner
                .lock()
                .await
                .active_sessions
                .contains_key(&removed.id));
            let remaining: Vec<LanShareSession> =
                read_array(&paths.lan_share_files.sessions).unwrap();
            assert!(remaining
                .iter()
                .all(|session| session.device_id != removed.id));
            let messages: Vec<LanShareMessage> =
                read_array(&paths.lan_share_files.messages).unwrap();
            assert_eq!(messages.len(), 2);
            assert_eq!(
                messages
                    .iter()
                    .find(|message| message.id == "shared-group")
                    .unwrap()
                    .session_id,
                host_id
            );
            let files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files).unwrap();
            assert_eq!(files.len(), 2);
            assert_eq!(
                files
                    .iter()
                    .find(|file| file.id == "shared-group")
                    .unwrap()
                    .session_id,
                host_id
            );
            assert!(original.exists());
            let groups: Vec<super::LanShareGroup> =
                read_array(&paths.lan_share_files.groups).unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].members.len(), 1);
            assert_eq!(groups[0].members[0].device_id, kept.id);
            assert!(
                super::delete_device(&registry, &paths, json!({ "deviceId": " " }))
                    .await
                    .is_err()
            );
            assert_eq!(
                read_array::<LanShareDevice>(&paths.lan_share_files.devices)
                    .unwrap()
                    .len(),
                1
            );
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn group_creation_rejects_missing_or_invalid_invitees_without_creating_records() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            let browser =
                upsert_device(&registry, &paths, "", "网页设备", "Chrome", "192.168.1.11")
                    .await
                    .unwrap();
            let native = upsert_device(
                &registry,
                &paths,
                "native-peer",
                "客户端",
                "MonkeyThief/Desktop",
                "192.168.1.12",
            )
            .await
            .unwrap();
            for identifiers in [
                json!([]),
                json!([browser.id, "missing"]),
                json!([native.id]),
            ] {
                assert!(create_group(
                    &registry,
                    &paths,
                    json!({ "name": "不应创建", "deviceIds": identifiers })
                )
                .await
                .is_err());
                assert!(
                    read_array::<super::LanShareGroup>(&paths.lan_share_files.groups)
                        .unwrap()
                        .is_empty()
                );
                assert!(
                    read_array::<LanShareSession>(&paths.lan_share_files.sessions)
                        .unwrap()
                        .is_empty()
                );
            }
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn group_creation_adds_unique_invitees_persists_sessions_and_notifies_online_devices() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let registry = LanShareServerRegistry::new();
            let online = upsert_device(&registry, &paths, "", "在线设备", "Chrome", "192.168.1.11")
                .await
                .unwrap();
            let offline =
                upsert_device(&registry, &paths, "", "离线设备", "Chrome", "192.168.1.12")
                    .await
                    .unwrap();
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            registry
                .inner
                .lock()
                .await
                .clients
                .insert(online.id.clone(), sender);
            let result = create_group(&registry, &paths, json!({ "name": "测试群聊", "messageVisibility": "afterJoin", "deviceIds": [online.id, offline.id, online.id] })).await.unwrap();
            let group_id = result["data"]["currentSession"]["groupId"]
                .as_str()
                .unwrap();
            let groups: Vec<super::LanShareGroup> =
                read_array(&paths.lan_share_files.groups).unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].members.len(), 2);
            let sessions: Vec<LanShareSession> =
                read_array(&paths.lan_share_files.sessions).unwrap();
            assert_eq!(
                sessions
                    .iter()
                    .filter(|session| session.group_id == group_id)
                    .count(),
                3
            );
            assert!(sessions
                .iter()
                .any(|session| session.device_id == offline.id
                    && session.message_visibility == "afterJoin"));
            assert_eq!(
                receiver.try_recv().unwrap().payload["type"],
                "groupsChanged"
            );
            assert!(receiver.try_recv().is_err());
            assert!(super::mobile_page_html().contains("payload.type === \"groupsChanged\""));
            tokio::fs::remove_dir_all(root).await.unwrap();
        });
    }

    #[test]
    fn creates_stable_file_id_from_path() {
        assert_eq!(
            create_file_id(r"D:\share\a.txt"),
            create_file_id(r"D:\share\a.txt")
        );
        assert_ne!(
            create_file_id(r"D:\share\a.txt"),
            create_file_id(r"D:\share\b.txt")
        );
    }

    #[test]
    fn creates_stable_device_id_from_ip() {
        assert_eq!(
            device_id_from_ip("192.168.1.8"),
            device_id_from_ip("192.168.1.8")
        );
        assert_ne!(
            device_id_from_ip("192.168.1.8"),
            device_id_from_ip("192.168.1.9")
        );
        assert_eq!(
            uuid::Uuid::parse_str(&device_id_from_ip("192.168.1.8"))
                .unwrap()
                .get_version_num(),
            5
        );
    }

    #[test]
    fn trims_string_values() {
        assert_eq!(string_value(Some(&json!("  hello  "))), "hello");
        assert_eq!(string_value(None), "");
    }

    #[test]
    fn create_id_returns_uuid() {
        let id = create_id();
        assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
        assert_ne!(id, create_id());
    }

    #[test]
    fn appends_message_without_content_length_limit() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let content = "长".repeat(1200);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();

            let registry = LanShareServerRegistry::new();
            let message = append_message(
                &registry,
                &paths,
                "device_1",
                "session_1",
                "mobile-to-desktop",
                &content,
                true,
            )
            .await
            .unwrap();

            assert_eq!(message.content, content);

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn incoming_messages_are_unread_until_session_activation() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            let device = upsert_device(
                &registry,
                &paths,
                "unread-device",
                "未读设备",
                "MonkeyThief/Desktop",
                "192.168.1.12",
            )
            .await
            .unwrap();
            let created = create_session(&registry, &paths, json!({ "deviceId": device.id }))
                .await
                .unwrap();
            let session_id = created["data"]["currentSession"]["id"]
                .as_str()
                .unwrap()
                .to_string();

            append_message(
                &registry,
                &paths,
                &device.id,
                &session_id,
                "mobile-to-desktop",
                "第一条",
                true,
            )
            .await
            .unwrap();
            append_message(
                &registry,
                &paths,
                &device.id,
                &session_id,
                "mobile-to-desktop",
                "第二条",
                true,
            )
            .await
            .unwrap();
            let second = create_session(&registry, &paths, json!({ "deviceId": device.id }))
                .await
                .unwrap();
            let second_session_id = second["data"]["currentSession"]["id"]
                .as_str()
                .unwrap()
                .to_string();
            append_message(
                &registry,
                &paths,
                &device.id,
                &second_session_id,
                "mobile-to-desktop",
                "第三条",
                true,
            )
            .await
            .unwrap();

            let state = get_state(&registry, &paths).await.unwrap();
            assert_eq!(state["data"]["totalUnread"], 3);
            let sessions = state["data"]["sessions"].as_array().unwrap();
            assert_eq!(
                sessions
                    .iter()
                    .find(|session| session["id"] == session_id)
                    .unwrap()["unreadCount"],
                2
            );
            assert_eq!(
                sessions
                    .iter()
                    .find(|session| session["id"] == second_session_id)
                    .unwrap()["unreadCount"],
                1
            );

            let cleared = activate_session(
                &registry,
                &paths,
                json!({ "sessionId": session_id }),
            )
            .await
            .unwrap();
            assert_eq!(cleared["data"]["totalUnread"], 1);
            assert_eq!(
                cleared["data"]["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|session| session["id"] == session_id)
                    .unwrap()["unreadCount"],
                0
            );
            assert_eq!(
                cleared["data"]["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|session| session["id"] == second_session_id)
                    .unwrap()["unreadCount"],
                1
            );

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn list_messages_searches_keyword_in_content_only() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.messages,
                &json!([
                    LanShareMessage {
                        id: "message_1".to_string(),
                        session_id: "session_1".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "needle device".to_string(),
                        direction: "inbound".to_string(),
                        message_type: "text".to_string(),
                        attachments: Vec::new(),
                        content: "ordinary text".to_string(),
                        created_at: 20,
                        delivered: true,
                        read: false,
                    },
                    LanShareMessage {
                        id: "message_2".to_string(),
                        session_id: "session_1".to_string(),
                        device_id: "device_2".to_string(),
                        device_name: "plain device".to_string(),
                        direction: "inbound".to_string(),
                        message_type: "text".to_string(),
                        attachments: Vec::new(),
                        content: "needle content".to_string(),
                        created_at: 10,
                        delivered: true,
                        read: false,
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let result = list_messages(&registry, &paths, json!({ "keyword": "needle", "to": 0 }))
                .await
                .unwrap();
            let messages = result["data"].as_array().unwrap();

            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0]["id"], "message_2");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn bulk_actions_delete_selected_messages_only() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.messages,
                &json!([
                    LanShareMessage {
                        id: "message_1".to_string(),
                        session_id: "session_1".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "设备一".to_string(),
                        direction: "mobile-to-desktop".to_string(),
                        message_type: "text".to_string(),
                        attachments: Vec::new(),
                        content: "删除我".to_string(),
                        created_at: 10,
                        delivered: true,
                        read: false,
                    },
                    LanShareMessage {
                        id: "message_2".to_string(),
                        session_id: "session_1".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "设备一".to_string(),
                        direction: "desktop-to-mobile".to_string(),
                        message_type: "text".to_string(),
                        attachments: Vec::new(),
                        content: "保留我".to_string(),
                        created_at: 20,
                        delivered: true,
                        read: false,
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            delete_messages(
                &registry,
                &paths,
                json!({ "messageIds": ["message_1"], "sessionId": "session_1" }),
            )
            .await
            .unwrap();
            let result = list_messages(
                &registry,
                &paths,
                json!({ "sessionId": "session_1", "to": 0 }),
            )
            .await
            .unwrap();
            let messages = result["data"].as_array().unwrap();

            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0]["id"], "message_2");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn empty_mobile_session_id_prefers_direct_session() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    LanShareSession {
                        id: "group_session".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "设备一".to_string(),
                        ip: "192.168.1.8".to_string(),
                        mode: "group".to_string(),
                        group_id: "group_1".to_string(),
                        group_name: "群聊".to_string(),
                        message_visibility: "all".to_string(),
                        joined_at: 20,
                        created_at: 20,
                        updated_at: 90,
                    },
                    LanShareSession {
                        id: "direct_session".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "设备一".to_string(),
                        ip: "192.168.1.8".to_string(),
                        mode: "direct".to_string(),
                        group_id: String::new(),
                        group_name: String::new(),
                        message_visibility: "all".to_string(),
                        joined_at: 10,
                        created_at: 10,
                        updated_at: 30,
                    }
                ]),
            )
            .await
            .unwrap();

            let session =
                super::ensure_session(&paths, "device_1", "", "设备一", "192.168.1.8").unwrap();

            assert_eq!(session.id, "direct_session");
            assert_eq!(session.mode, "direct");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn bulk_actions_delete_session_removes_related_records() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    LanShareSession {
                        id: "session_1".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "设备一".to_string(),
                        ip: "192.168.1.8".to_string(),
                        mode: "direct".to_string(),
                        group_id: String::new(),
                        group_name: String::new(),
                        message_visibility: "all".to_string(),
                        joined_at: 10,
                        created_at: 10,
                        updated_at: 20,
                    },
                    LanShareSession {
                        id: "session_2".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "设备一".to_string(),
                        ip: "192.168.1.8".to_string(),
                        mode: "direct".to_string(),
                        group_id: String::new(),
                        group_name: String::new(),
                        message_visibility: "all".to_string(),
                        joined_at: 30,
                        created_at: 30,
                        updated_at: 40,
                    }
                ]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.messages,
                &json!([
                    {
                        "id": "message_1",
                        "sessionId": "session_1",
                        "deviceId": "device_1",
                        "deviceName": "设备一",
                        "direction": "mobile-to-desktop",
                        "messageType": "text",
                        "content": "删除",
                        "createdAt": 10,
                        "delivered": true,
                        "read": false
                    },
                    {
                        "id": "message_2",
                        "sessionId": "session_2",
                        "deviceId": "device_1",
                        "deviceName": "设备一",
                        "direction": "mobile-to-desktop",
                        "messageType": "text",
                        "content": "保留",
                        "createdAt": 20,
                        "delivered": true,
                        "read": false
                    }
                ]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.files,
                &json!([
                    {
                        "id": "file_1",
                        "sessionId": "session_1",
                        "path": "a.txt",
                        "name": "a.txt",
                        "size": 1,
                        "mimeType": "text/plain",
                        "updatedAt": 10,
                        "enabled": true
                    },
                    {
                        "id": "file_2",
                        "sessionId": "session_2",
                        "path": "b.txt",
                        "name": "b.txt",
                        "size": 1,
                        "mimeType": "text/plain",
                        "updatedAt": 20,
                        "enabled": true
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let state = delete_session(&registry, &paths, json!({ "sessionId": "session_1" }))
                .await
                .unwrap();

            assert_eq!(state["data"]["sessions"].as_array().unwrap().len(), 1);
            assert_eq!(state["data"]["sessions"][0]["id"], "session_2");
            assert_eq!(state["data"]["messages"].as_array().unwrap().len(), 1);
            assert_eq!(state["data"]["messages"][0]["id"], "message_2");
            assert_eq!(state["data"]["files"].as_array().unwrap().len(), 1);
            assert_eq!(state["data"]["files"][0]["id"], "file_2");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn bulk_actions_remove_selected_files_only() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.files,
                &json!([
                    {
                        "id": "file_1",
                        "sessionId": "session_1",
                        "path": "a.txt",
                        "name": "a.txt",
                        "size": 1,
                        "mimeType": "text/plain",
                        "updatedAt": 10,
                        "enabled": true
                    },
                    {
                        "id": "file_2",
                        "sessionId": "session_1",
                        "path": "b.txt",
                        "name": "b.txt",
                        "size": 1,
                        "mimeType": "text/plain",
                        "updatedAt": 20,
                        "enabled": true
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let state = remove_files(&registry, &paths, json!({ "fileIds": ["file_1"] }))
                .await
                .unwrap();

            assert_eq!(state["data"]["files"].as_array().unwrap().len(), 1);
            assert_eq!(state["data"]["files"][0]["id"], "file_2");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn save_file_copies_received_attachments_without_running_service() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            let uploads = Path::new(&paths.lan_share_dir).join("uploads");
            let downloads = root.join("downloads");
            tokio::fs::create_dir_all(&uploads).await.unwrap();
            tokio::fs::create_dir_all(&downloads).await.unwrap();
            let contents = [
                ("收到的文本.txt", "完整的文本内容\n第二行".as_bytes()),
                ("收到的图片.png", &[0x89, 0x50, 0x4e, 0x47, 0, 0xff][..]),
            ];
            let mut files = Vec::new();
            for (name, content) in contents {
                let source = uploads.join(format!("upload-{name}"));
                tokio::fs::write(&source, content).await.unwrap();
                let mut file = super::file_payload(&source.to_string_lossy(), "session_1")
                    .await
                    .unwrap();
                file.name = name.into();
                files.push(file);
            }
            super::attachments::publish(&registry, &paths, &files)
                .await
                .unwrap();
            for (file, (name, content)) in files.iter().zip(contents) {
                let target = downloads.join(name);
                let result = super::save_file(
                    &registry,
                    &paths,
                    json!({"fileId": file.id, "targetPath": target}),
                )
                .await
                .unwrap();
                assert_eq!(result["data"]["targetPath"], json!(target));
                assert_eq!(tokio::fs::read(&target).await.unwrap(), content);
                assert_eq!(tokio::fs::read(&file.path).await.unwrap(), content);
            }
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            tokio::fs::remove_dir_all(resolved).await.unwrap();
        });
    }

    #[test]
    fn save_file_rejects_invalid_records_and_preserves_source_file() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let registry = LanShareServerRegistry::new();
            tokio::fs::create_dir_all(&root).await.unwrap();
            let source = root.join("source.txt");
            let target = root.join("saved.txt");
            tokio::fs::write(&source, "原文件内容").await.unwrap();
            tokio::fs::write(&target, "已有内容").await.unwrap();
            let file = super::file_payload(&source.to_string_lossy(), "session_1")
                .await
                .unwrap();
            let mut unpublished = file.clone();
            unpublished.id = "unpublished".into();
            unpublished.enabled = false;
            super::write_json(&paths.lan_share_files.files, &json!([file, unpublished]))
                .await
                .unwrap();
            for payload in [
                json!({"fileId": "missing", "targetPath": target}),
                json!({"fileId": "unpublished", "targetPath": target}),
                json!({"fileId": file.id, "targetPath": ""}),
                json!({"fileId": file.id, "targetPath": source}),
            ] {
                assert!(super::save_file(&registry, &paths, payload).await.is_err());
                assert_eq!(
                    tokio::fs::read_to_string(&source).await.unwrap(),
                    "原文件内容"
                );
                assert_eq!(
                    tokio::fs::read_to_string(&target).await.unwrap(),
                    "已有内容"
                );
            }
            tokio::fs::remove_file(&source).await.unwrap();
            assert!(super::save_file(
                &registry,
                &paths,
                json!({"fileId": file.id, "targetPath": target}),
            )
            .await
            .is_err());
            assert_eq!(
                tokio::fs::read_to_string(&target).await.unwrap(),
                "已有内容"
            );
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            tokio::fs::remove_dir_all(resolved).await.unwrap();
        });
    }

    #[test]
    fn bulk_actions_export_selected_files_to_zip() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let first_file = root.join("first.txt");
            let second_file = root.join("second.txt");
            let archive_path = root.join("selected.zip");
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            tokio::fs::write(&first_file, "first").await.unwrap();
            tokio::fs::write(&second_file, "second").await.unwrap();
            super::write_json(
                &paths.lan_share_files.files,
                &json!([
                    LanShareFile {
                        id: "file_1".to_string(),
                        session_id: "session_1".to_string(),
                        path: first_file.to_string_lossy().to_string(),
                        name: "first.txt".to_string(),
                        size: 5,
                        mime_type: "text/plain".to_string(),
                        updated_at: 10,
                        enabled: true,
                    },
                    LanShareFile {
                        id: "file_2".to_string(),
                        session_id: "session_1".to_string(),
                        path: second_file.to_string_lossy().to_string(),
                        name: "second.txt".to_string(),
                        size: 6,
                        mime_type: "text/plain".to_string(),
                        updated_at: 20,
                        enabled: true,
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let result = export_files_zip(
                &registry,
                &paths,
                json!({
                    "fileIds": ["file_1", "file_2"],
                    "targetPath": archive_path.to_string_lossy().to_string()
                }),
            )
            .await
            .unwrap();
            let archive = tokio::fs::read(&archive_path).await.unwrap();

            assert_eq!(result["data"]["count"], 2);
            assert!(archive.starts_with(&[0x50, 0x4b]));

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn creates_new_session_bound_to_device_ip_without_reusing_previous_one() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "device_1".to_string(),
                    name: "我的设备".to_string(),
                    auto_name: "Device Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let first = create_session(&registry, &paths, json!({ "deviceId": "device_1" }))
                .await
                .unwrap();
            let second = create_session(&registry, &paths, json!({ "deviceId": "device_1" }))
                .await
                .unwrap();
            let sessions = second["data"]["sessions"].as_array().unwrap();

            assert_ne!(
                first["data"]["currentSession"]["id"],
                second["data"]["currentSession"]["id"]
            );
            assert_eq!(sessions.len(), 2);
            assert_eq!(sessions[0]["ip"], "192.168.1.8");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn upsert_device_uses_ip_bound_id_and_keeps_identity_when_name_changes() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            let expected_device_id = device_id_from_ip("192.168.1.8");
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "random_browser_device".to_string(),
                    name: "旧设备".to_string(),
                    auto_name: "Android Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let device = upsert_device(
                &registry,
                &paths,
                "device_new",
                "重新扫码设备",
                "Android Chrome",
                "192.168.1.8",
            )
            .await
            .unwrap();
            let devices: Vec<LanShareDevice> =
                super::read_array(&paths.lan_share_files.devices).unwrap();

            assert_eq!(device.id, expected_device_id);
            assert_eq!(device.name, "重新扫码设备");
            assert_eq!(devices.len(), 1);
            assert_eq!(devices[0].id, expected_device_id);

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn state_backfills_missing_session_ip_from_device_record() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "device_1".to_string(),
                    name: "我的设备".to_string(),
                    auto_name: "Device Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    {
                        "id": "session_old",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "createdAt": 10,
                        "updatedAt": 10
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let state = super::get_state(&registry, &paths).await.unwrap();

            assert_eq!(state["data"]["sessions"][0]["ip"], "192.168.1.8");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn appends_message_to_requested_session() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "device_1".to_string(),
                    name: "我的设备".to_string(),
                    auto_name: "Device Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    {
                        "id": "session_new",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.8",
                        "createdAt": 20,
                        "updatedAt": 20
                    },
                    {
                        "id": "session_old",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.7",
                        "createdAt": 10,
                        "updatedAt": 10
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let message = append_message(
                &registry,
                &paths,
                "device_1",
                "session_old",
                "desktop-to-mobile",
                "继续旧会话",
                true,
            )
            .await
            .unwrap();

            assert_eq!(message.session_id, "session_old");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn appends_desktop_message_to_group_session_without_new_direct_session() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    LanShareSession {
                        id: "group_owner_session".to_string(),
                        device_id: String::new(),
                        device_name: "群聊".to_string(),
                        ip: String::new(),
                        mode: "group".to_string(),
                        group_id: "group_1".to_string(),
                        group_name: "测试群".to_string(),
                        message_visibility: "all".to_string(),
                        joined_at: 10,
                        created_at: 10,
                        updated_at: 10,
                    },
                    LanShareSession {
                        id: "group_member_session".to_string(),
                        device_id: "device_1".to_string(),
                        device_name: "我的设备".to_string(),
                        ip: "192.168.1.8".to_string(),
                        mode: "group".to_string(),
                        group_id: "group_1".to_string(),
                        group_name: "测试群".to_string(),
                        message_visibility: "all".to_string(),
                        joined_at: 20,
                        created_at: 20,
                        updated_at: 20,
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let message = append_message(
                &registry,
                &paths,
                "desktop",
                "group_owner_session",
                "desktop-to-mobile",
                "群聊服务端消息",
                true,
            )
            .await
            .unwrap();
            let messages = list_messages(
                &registry,
                &paths,
                json!({ "sessionId": "group_owner_session", "to": 0 }),
            )
            .await
            .unwrap();
            let sessions = read_array::<LanShareSession>(&paths.lan_share_files.sessions).unwrap();

            assert_eq!(message.session_id, "group_owner_session");
            assert_eq!(messages["data"][0]["content"], "群聊服务端消息");
            assert_eq!(sessions.len(), 2);
            assert!(!sessions
                .iter()
                .any(|session| session.mode == "direct" && session.device_id == "desktop"));

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn appends_desktop_message_to_last_activated_session_by_default() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "device_1".to_string(),
                    name: "我的设备".to_string(),
                    auto_name: "Device Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    {
                        "id": "session_new",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.8",
                        "createdAt": 20,
                        "updatedAt": 20
                    },
                    {
                        "id": "session_old",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.7",
                        "createdAt": 10,
                        "updatedAt": 10
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            activate_session(&registry, &paths, json!({ "sessionId": "session_old" }))
                .await
                .unwrap();
            let message = append_message(
                &registry,
                &paths,
                "device_1",
                "",
                "desktop-to-mobile",
                "继续旧会话",
                true,
            )
            .await
            .unwrap();

            assert_eq!(message.session_id, "session_old");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn appends_mobile_message_to_latest_session_matching_current_ip() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "device_1".to_string(),
                    name: "我的设备".to_string(),
                    auto_name: "Device Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    {
                        "id": "session_other_ip",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.99",
                        "createdAt": 30,
                        "updatedAt": 30
                    },
                    {
                        "id": "session_same_ip",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.8",
                        "createdAt": 20,
                        "updatedAt": 20
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let message = append_message(
                &registry,
                &paths,
                "device_1",
                "",
                "mobile-to-desktop",
                "恢复当前 IP 会话",
                true,
            )
            .await
            .unwrap();

            assert_eq!(message.session_id, "session_same_ip");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn refresh_files_keeps_existing_id_and_path() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let shared_file = root.join("shared.txt");
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            tokio::fs::write(&shared_file, "hello").await.unwrap();

            super::write_json(
                &paths.lan_share_files.files,
                &json!([LanShareFile {
                    id: "custom_id".to_string(),
                    session_id: "session_1".to_string(),
                    path: shared_file.to_string_lossy().to_string(),
                    name: "old.txt".to_string(),
                    size: 1,
                    mime_type: "text/plain".to_string(),
                    updated_at: 1,
                    enabled: false,
                }]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let result = refresh_files(&registry, &paths).await.unwrap();
            let files = result["data"].as_array().unwrap();

            assert_eq!(files[0]["id"], "custom_id");
            assert_eq!(files[0]["path"], shared_file.to_string_lossy().to_string());
            assert_eq!(files[0]["name"], "shared.txt");
            assert_eq!(files[0]["enabled"], true);

            let _ = tokio::fs::remove_dir_all(Path::new(&root)).await;
        });
    }

    #[test]
    fn session_files_are_only_visible_for_their_session() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let first_file = root.join("first.txt");
            let second_file = root.join("second.txt");
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            tokio::fs::write(&first_file, "first").await.unwrap();
            tokio::fs::write(&second_file, "second").await.unwrap();
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "device_1".to_string(),
                    name: "我的设备".to_string(),
                    auto_name: "Device Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    {
                        "id": "session_1",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.8",
                        "createdAt": 10,
                        "updatedAt": 10
                    },
                    {
                        "id": "session_2",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.8",
                        "createdAt": 20,
                        "updatedAt": 20
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            add_files(
                &registry,
                &paths,
                json!({
                    "sessionId": "session_1",
                    "paths": [first_file.to_string_lossy().to_string()]
                }),
            )
            .await
            .unwrap();
            add_files(
                &registry,
                &paths,
                json!({
                    "sessionId": "session_2",
                    "paths": [second_file.to_string_lossy().to_string()]
                }),
            )
            .await
            .unwrap();

            let first_files = mobile_files_payload(&registry, &paths, "session_1")
                .await
                .unwrap();
            let second_files = mobile_files_payload(&registry, &paths, "session_2")
                .await
                .unwrap();

            assert_eq!(first_files.as_array().unwrap().len(), 1);
            assert_eq!(first_files[0]["name"], "first.txt");
            assert_eq!(second_files.as_array().unwrap().len(), 1);
            assert_eq!(second_files[0]["name"], "second.txt");

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn adding_files_creates_file_message() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            let shared_file = root.join("shared.txt");
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            tokio::fs::write(&shared_file, "hello").await.unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([LanShareSession {
                    id: "session_1".to_string(),
                    device_id: "device_1".to_string(),
                    device_name: "我的设备".to_string(),
                    ip: "192.168.1.8".to_string(),
                    mode: "direct".to_string(),
                    group_id: String::new(),
                    group_name: String::new(),
                    message_visibility: "all".to_string(),
                    joined_at: 10,
                    created_at: 10,
                    updated_at: 10,
                }]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            add_files(
                &registry,
                &paths,
                json!({
                    "sessionId": "session_1",
                    "paths": [shared_file.to_string_lossy().to_string()]
                }),
            )
            .await
            .unwrap();
            let messages = list_messages(
                &registry,
                &paths,
                json!({ "sessionId": "session_1", "to": 0 }),
            )
            .await
            .unwrap();

            assert_eq!(messages["data"][0]["messageType"], "file");
            assert_eq!(
                messages["data"][0]["content"],
                "共享文件：shared.txt（5 B）"
            );

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn desktop_message_to_offline_session_is_kept_for_later_sync() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);
            tokio::fs::create_dir_all(&paths.lan_share_dir)
                .await
                .unwrap();
            super::write_json(
                &paths.lan_share_files.devices,
                &json!([LanShareDevice {
                    id: "device_1".to_string(),
                    name: "我的设备".to_string(),
                    auto_name: "Device Chrome · 192.168.1.8".to_string(),
                    user_agent: "Chrome".to_string(),
                    ip: "192.168.1.8".to_string(),
                    first_seen_at: 1,
                    last_seen_at: 2,
                }]),
            )
            .await
            .unwrap();
            super::write_json(
                &paths.lan_share_files.sessions,
                &json!([
                    {
                        "id": "session_1",
                        "deviceId": "device_1",
                        "deviceName": "我的设备",
                        "ip": "192.168.1.8",
                        "createdAt": 10,
                        "updatedAt": 10
                    }
                ]),
            )
            .await
            .unwrap();

            let registry = LanShareServerRegistry::new();
            let message = append_message(
                &registry,
                &paths,
                "device_1",
                "session_1",
                "desktop-to-mobile",
                "离线留言",
                false,
            )
            .await
            .unwrap();
            let messages = list_messages(
                &registry,
                &paths,
                json!({ "sessionId": "session_1", "to": 0 }),
            )
            .await
            .unwrap();

            assert_eq!(message.session_id, "session_1");
            assert!(!message.delivered);
            assert_eq!(messages["data"][0]["content"], "离线留言");
            assert_eq!(messages["data"][0]["delivered"], false);

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }
}

#[cfg(test)]
mod service_tests {
    use std::net::Ipv4Addr;

    use super::{
        create_id, get_state, is_lan_ipv4, is_previewable_mime, is_request_token_valid,
        is_text_preview_extension, is_valid_token, mobile_page_html, mobile_page_response, qr_svg,
        safe_header_file_name, send_stored_message_to_client, stop_runtime, LanShareMessage,
        LanShareServerRegistry, WsOutbound,
    };
    use crate::core::paths::resolve_app_paths;

    #[test]
    fn rejects_empty_or_wrong_token() {
        assert!(is_valid_token("abc", "abc"));
        assert!(!is_valid_token("abc", ""));
        assert!(!is_valid_token("abc", "def"));
    }

    #[test]
    fn mobile_page_contains_runtime_bootstrap() {
        let html = mobile_page_html();

        assert!(html.contains("window.__LAN_SHARE_APP__"));
        assert!(html.contains("WebSocket"));
        assert!(html.contains("/api/files"));
        assert!(html.contains("/api/messages"));
    }

    #[test]
    fn mobile_page_contains_file_preview_entry() {
        let html = mobile_page_html();

        assert!(html.contains("\"preview\""));
        assert!(html.contains("previewDialog"));
        assert!(html.contains("预览"));
    }

    #[test]
    fn mobile_page_contains_message_copy_handler() {
        let html = mobile_page_html();

        assert!(html.contains("copyMessageText"));
        assert!(html.contains("点击消息可复制"));
    }

    #[test]
    fn mobile_page_contains_client_session_creation() {
        let html = mobile_page_html();

        assert!(html.contains("/api/sessions"));
        assert!(html.contains("createSession"));
        assert!(html.contains("sessionId"));
    }

    #[test]
    fn mobile_page_uses_separate_file_and_message_views() {
        let html = mobile_page_html();

        assert!(html.contains("data-view=\"files\""));
        assert!(html.contains("data-view=\"messages\""));
        assert!(html.contains("setActiveView"));
        assert!(html.contains("view-panel active"));
        assert!(!html.contains("card card-files"));
        assert!(!html.contains("card card-messages"));
    }

    #[test]
    fn mobile_page_defaults_to_message_view_first() {
        let html = mobile_page_html();
        let direct_mode_index = html.find("data-mode=\"direct\"").unwrap();
        let group_mode_index = html.find("data-mode=\"group\"").unwrap();
        let messages_tab_index = html.find("data-view=\"messages\"").unwrap();
        let files_tab_index = html.find("data-view=\"files\"").unwrap();
        let messages_panel_index = html.find("id=\"messagesView\"").unwrap();
        let files_panel_index = html.find("id=\"filesView\"").unwrap();

        assert!(direct_mode_index < group_mode_index);
        assert!(messages_tab_index < files_tab_index);
        assert!(messages_panel_index < files_panel_index);
        assert!(html.contains("class=\"mode-tab active\" data-mode=\"direct\""));
        assert!(html.contains("class=\"view-tab active\" data-view=\"messages\""));
        assert!(html.contains("id=\"messagesView\" class=\"card view-panel active\""));
    }

    #[test]
    fn mobile_page_group_mode_starts_from_joined_group_records() {
        let html = mobile_page_html();

        assert!(html.contains("id=\"groupListView\""));
        assert!(html.contains("已加入群聊"));
        assert!(html.contains("openGroupSession"));
        assert!(html.contains("返回群列表"));
        assert!(html.contains("currentMode === \"group\" && !currentGroupId"));
    }

    #[test]
    fn mobile_page_adapts_layout_for_desktop_width() {
        let html = mobile_page_html();

        assert!(html.contains("class=\"page-shell\""));
        assert!(html.contains("class=\"card device-card\""));
        assert!(html.contains("class=\"content-shell\""));
        assert!(html.contains("class=\"content-detail\""));
        assert!(!html.contains("@media"));
        assert!(!html.contains("@container"));
    }

    #[test]
    fn mobile_page_reuses_shared_font_sizes() {
        let html = mobile_page_html();
        let typography = include_str!("../../../src/styles/typography.css");

        assert!(html.contains(typography));
        for size in ["base", "lg", "sm", "xs", "xl"] {
            assert_eq!(html.matches(&format!("--font-size-{size}:")).count(), 1);
        }
        assert!(html.contains("font-size: var(--font-size-base)"));
        assert!(html.contains("font-size: var(--font-size-sm)"));
    }

    #[test]
    fn mobile_page_message_input_has_no_length_limit() {
        let html = mobile_page_html();

        assert!(html.contains("id=\"messageInput\""));
        assert!(!html.contains("id=\"messageInput\" class=\"field-input\" maxlength="));
    }

    #[test]
    fn mobile_page_response_disables_browser_cache() {
        let response = mobile_page_response();

        assert_eq!(
            response.headers()["cache-control"],
            "no-store, no-cache, must-revalidate"
        );
        assert_eq!(response.headers()["pragma"], "no-cache");
        assert_eq!(response.headers()["expires"], "0");
    }

    #[test]
    fn mobile_page_does_not_create_browser_random_device_id() {
        let html = mobile_page_html();

        assert!(!html.contains("lanShareDeviceId"));
        assert!(!html.contains("Math.random"));
    }

    #[test]
    fn detects_browser_previewable_mime_types() {
        assert!(is_previewable_mime("image/png"));
        assert!(is_previewable_mime("application/pdf"));
        assert!(is_previewable_mime("text/plain"));
        assert!(is_previewable_mime("video/mp4"));
        assert!(!is_previewable_mime(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        ));
    }

    #[test]
    fn detects_text_previewable_file_extensions() {
        assert!(is_text_preview_extension("README.md"));
        assert!(is_text_preview_extension("component.vue"));
        assert!(is_text_preview_extension("config.toml"));
        assert!(!is_text_preview_extension("archive.zip"));
    }

    #[test]
    fn mobile_page_loads_files_without_waiting_for_device_registration() {
        let html = mobile_page_html();

        assert!(html.contains("loadFiles().catch"));
        assert!(!html.contains("registerDevice()\n      .then(loadFiles)"));
    }

    #[test]
    fn creates_qr_svg_for_access_url() {
        let svg = qr_svg("http://192.168.1.2:17631/?token=abc").unwrap();

        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn sanitizes_header_file_names() {
        assert_eq!(safe_header_file_name("a\r\nb.txt"), "a__b.txt");
        assert_eq!(safe_header_file_name(""), "download");
    }

    #[test]
    fn excludes_link_local_ipv4_from_lan_candidates() {
        assert!(is_lan_ipv4(Ipv4Addr::new(192, 168, 3, 178)));
        assert!(is_lan_ipv4(Ipv4Addr::new(10, 0, 0, 8)));
        assert!(!is_lan_ipv4(Ipv4Addr::new(169, 254, 54, 75)));
    }

    #[test]
    fn network_changes_refresh_access_link_without_restarting_service() {
        tauri::async_runtime::block_on(async {
            let registry = LanShareServerRegistry::new();
            let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<WsOutbound>();
            let mut runtime = registry.inner.lock().await;
            runtime.handle = Some(tauri::async_runtime::spawn(std::future::pending()));
            runtime.token = "stable-token".into();
            runtime.port = 17639;
            runtime.lan_ip = "192.168.1.10".into();
            runtime.access_url = "http://192.168.1.10:17639/?token=stable-token".into();
            runtime.native.config.hidden = true;
            runtime.clients.insert("connected-device".into(), sender);
            runtime.active_sessions.insert("connected-device".into(), "session".into());

            // 多网卡顺序变化不覆盖仍然有效的手选地址。
            assert!(!super::refresh_access_address(&mut runtime,
                &["10.0.0.2".into(), "192.168.1.10".into()]));
            assert_eq!(runtime.lan_ip, "192.168.1.10");
            assert!(super::refresh_access_address(&mut runtime, &["192.168.2.20".into()]));
            assert_eq!(runtime.lan_ip, "192.168.2.20");
            assert_eq!(runtime.access_url, "http://192.168.2.20:17639/?token=stable-token");
            assert!(!super::refresh_access_address(&mut runtime, &["192.168.2.20".into()]));
            assert_eq!(runtime.token, "stable-token");
            assert_eq!(runtime.port, 17639);
            assert!(runtime.handle.is_some());
            assert!(runtime.native.config.hidden);
            assert!(runtime.clients.contains_key("connected-device"));
            assert_eq!(runtime.active_sessions["connected-device"], "session");
            stop_runtime(&mut runtime);
        });
    }

    #[test]
    fn network_disconnect_clears_links_and_reconnect_restores_them() {
        tauri::async_runtime::block_on(async {
            let registry = LanShareServerRegistry::new();
            let mut runtime = registry.inner.lock().await;
            runtime.handle = Some(tauri::async_runtime::spawn(std::future::pending()));
            runtime.token = "stable-token".into();
            runtime.lan_ip = "192.168.1.10".into();
            runtime.access_url = "http://192.168.1.10:17631/?token=stable-token".into();
            assert!(super::refresh_access_address(&mut runtime, &[]));
            assert!(runtime.lan_ip.is_empty());
            assert!(runtime.access_url.is_empty());
            assert!(runtime.handle.is_some());
            assert!(!super::refresh_access_address(&mut runtime, &[]));
            assert!(super::refresh_access_address(&mut runtime, &["10.0.0.20".into()]));
            assert_eq!(runtime.access_url, "http://10.0.0.20:17631/?token=stable-token");
            stop_runtime(&mut runtime);
            assert!(!super::refresh_access_address(&mut runtime, &["10.0.0.20".into()]));
            assert!(runtime.access_url.is_empty());
        });
    }

    #[test]
    fn state_includes_qr_svg_for_running_access_url() {
        tauri::async_runtime::block_on(async {
            let registry = LanShareServerRegistry::new();
            let root = std::env::temp_dir().join(create_id());
            let paths = resolve_app_paths(&root);

            registry.inner.lock().await.access_url =
                "http://192.168.3.178:17631/?token=abc".to_string();

            let state = get_state(&registry, &paths).await.unwrap();

            assert!(state["data"]["service"]["qrSvg"]
                .as_str()
                .unwrap_or_default()
                .contains("<svg"));

            let _ = tokio::fs::remove_dir_all(root).await;
        });
    }

    #[test]
    fn request_token_validation_reads_runtime_token() {
        tauri::async_runtime::block_on(async {
            let registry = LanShareServerRegistry::new();

            registry.inner.lock().await.token = "abc".to_string();
            assert!(is_request_token_valid(&registry, "abc").await);

            registry.inner.lock().await.token.clear();
            assert!(!is_request_token_valid(&registry, "abc").await);
        });
    }

    #[test]
    fn queues_stored_message_payload_for_client() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<WsOutbound>();
        let message = LanShareMessage {
            id: "message_1".to_string(),
            session_id: "session_1".to_string(),
            device_id: "device_1".to_string(),
            device_name: "我的设备".to_string(),
            direction: "mobile-to-desktop".to_string(),
            message_type: "text".to_string(),
            attachments: Vec::new(),
            content: "hello".to_string(),
            created_at: 123,
            delivered: true,
            read: false,
        };

        assert!(send_stored_message_to_client(&sender, &message));

        let outbound = receiver.try_recv().unwrap();

        assert_eq!(outbound.payload["type"], "message");
        assert_eq!(outbound.payload["message"]["id"], "message_1");
        assert_eq!(outbound.payload["message"]["content"], "hello");
    }

    #[test]
    fn stop_runtime_clears_connection_tasks_and_clients() {
        tauri::async_runtime::block_on(async {
            let registry = LanShareServerRegistry::new();
            let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<WsOutbound>();
            let connection = tauri::async_runtime::spawn(async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            });

            {
                let mut runtime = registry.inner.lock().await;
                runtime.token = "abc".to_string();
                runtime.access_url = "http://127.0.0.1".to_string();
                runtime.clients.insert("device".to_string(), sender);
                runtime.connections.push(connection);
                runtime.maintenance = Some(tauri::async_runtime::spawn(std::future::pending()));

                stop_runtime(&mut runtime);

                assert!(runtime.token.is_empty());
                assert!(runtime.access_url.is_empty());
                assert!(runtime.clients.is_empty());
                assert!(runtime.connections.is_empty());
                assert!(runtime.maintenance.is_none());
            }

            assert!(registry.inner.lock().await.clients.get("device").is_none());
        });
    }
}

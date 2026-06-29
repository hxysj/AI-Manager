use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use base64::Engine;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
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
use std::collections::HashMap;
use std::convert::Infallible;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::protocol::{Message, Role};
use tokio_tungstenite::WebSocketStream;
use tokio_util::io::ReaderStream;
use url::Url;

type BoxBody = http_body_util::combinators::BoxBody<Bytes, std::io::Error>;

pub const DEFAULT_PORT: u16 = 17631;
pub const EVENT_STATE_CHANGED: &str = "lan-share:state-changed";
pub const EVENT_MESSAGE_CREATED: &str = "lan-share:message-created";
pub const EVENT_DEVICES_CHANGED: &str = "lan-share:devices-changed";

#[derive(Clone)]
pub struct LanShareServerRegistry {
    inner: Arc<Mutex<LanShareRuntime>>,
    storage: Arc<Mutex<()>>,
}

pub struct LanShareRuntime {
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
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
    pub created_at: u64,
    pub updated_at: u64,
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
            inner: Arc::new(Mutex::new(LanShareRuntime {
                handle: None,
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

pub fn create_id(prefix: &str) -> String {
    format!("{}_{}_{}", prefix, now_millis(), uuid::Uuid::new_v4())
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

pub fn normalize_device_name(value: &str) -> String {
    value.trim().chars().take(40).collect()
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

pub fn create_file_id(path: &str) -> String {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(path.as_bytes());
    format!("file_{:x}", hasher.finalize())
}

pub fn device_id_from_ip(ip: &str) -> String {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(ip.trim().as_bytes());
    format!("device_{:x}", hasher.finalize())
}

fn create_session_file_id(session_id: &str, path: &str) -> String {
    use sha1::{Digest, Sha1};

    if session_id.is_empty() {
        return create_file_id(path);
    }

    let mut hasher = Sha1::new();
    hasher.update(session_id.as_bytes());
    hasher.update(b":");
    hasher.update(path.as_bytes());
    format!("file_{:x}", hasher.finalize())
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

    if session_id.is_empty() {
        return Err(ManagerError::System(
            "请先选择会话后再共享文件。".to_string(),
        ));
    }

    let selected_paths = payload
        .get("paths")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|value| string_value(Some(&value)))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let mut selected_files = Vec::new();

    for selected_path in selected_paths {
        selected_files.push(file_payload(&selected_path, &session_id).await?);
    }

    {
        let _storage = registry.storage.lock().await;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let Some(session_index) = sessions.iter().position(|session| session.id == session_id)
        else {
            return Err(ManagerError::System("会话不存在。".to_string()));
        };

        let mut files: Vec<LanShareFile> = read_array(&paths.lan_share_files.files)?;

        for next_file in selected_files {
            if let Some(current) = files.iter_mut().find(|file| file.id == next_file.id) {
                *current = next_file;
            } else {
                files.insert(0, next_file);
            }
        }

        sessions[session_index].updated_at = now_millis();
        sort_sessions(&mut sessions);
        write_json(&paths.lan_share_files.files, &json!(files)).await?;
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
    }

    get_state(registry, paths).await
}

pub async fn get_state(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
) -> Result<Value, ManagerError> {
    let (files, devices, mut sessions, messages) = {
        let _storage = registry.storage.lock().await;
        let files = read_array::<LanShareFile>(&paths.lan_share_files.files)?;
        let devices = read_array::<LanShareDevice>(&paths.lan_share_files.devices)?;
        let mut sessions = read_array::<LanShareSession>(&paths.lan_share_files.sessions)?;
        let messages = read_array::<LanShareMessage>(&paths.lan_share_files.messages)?;

        fill_session_ips(paths, &devices, &mut sessions).await?;
        (files, devices, sessions, messages)
    };
    sort_sessions(&mut sessions);
    let runtime = registry.inner.lock().await;
    let online_device_ids = runtime.clients.keys().cloned().collect::<Vec<_>>();
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
                "online": online_device_ids.contains(&device.id)
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
            "port": runtime.port,
            "onlineDevices": runtime.clients.len()
        },
        "files": files,
        "devices": devices,
        "sessions": sessions,
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

    messages.retain(|message| {
        let matches_keyword =
            keyword.is_empty() || message.content.to_lowercase().contains(&keyword);
        let matches_device = device_id.is_empty() || message.device_id == device_id;
        let matches_session = session_id.is_empty() || message.session_id == session_id;
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

fn find_lan_ip() -> Result<String, ManagerError> {
    let interfaces = local_ip_address::list_afinet_netifas()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    for (_, ip) in interfaces {
        if let IpAddr::V4(ipv4) = ip {
            if is_lan_ipv4(ipv4) {
                return Ok(ipv4.to_string());
            }
        }
    }

    Err(ManagerError::System(
        "未找到可用局域网 IP，请确认电脑已连接 Wi-Fi、局域网或热点。".to_string(),
    ))
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
    r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>设备快传</title>
  <style>
    * { box-sizing: border-box; }
    html, body { height: 100%; }
    body { min-height: 100vh; margin: 0; background: #eef2f7; color: #172033; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; overflow: hidden; }
    button, input { font: inherit; }
    button { cursor: pointer; }
    .page-head { position: sticky; top: 0; z-index: 4; display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 14px 16px; border-bottom: 1px solid #dbe4ee; background: rgba(255, 255, 255, 0.96); backdrop-filter: blur(10px); }
    .page-title { display: flex; min-width: 0; flex-direction: column; gap: 3px; }
    .page-title strong { color: #172033; font-size: 19px; line-height: 1.2; }
    .page-title span { overflow: hidden; color: #697789; font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
    .page-badge { display: inline-flex; height: 28px; align-items: center; justify-content: center; padding: 0 9px; border: 1px solid #c7d5e4; border-radius: 999px; background: #f8fbff; color: #315f8f; font-size: 12px; font-weight: 800; }
    .page-main { display: flex; height: calc(100vh - 57px); min-height: 0; flex-direction: column; gap: 12px; overflow: hidden; padding: 12px; }
    .card { display: flex; min-height: 0; flex-direction: column; overflow: hidden; border: 1px solid #dbe4ee; border-radius: 8px; background: #ffffff; box-shadow: 0 8px 22px rgba(35, 55, 80, 0.05); }
    .view-tabs { display: flex; flex: none; gap: 8px; padding: 4px; border: 1px solid #dbe4ee; border-radius: 8px; background: #f7fafd; }
    .view-tab { display: inline-flex; height: 36px; flex: 1; align-items: center; justify-content: center; border: 1px solid transparent; border-radius: 7px; background: transparent; color: #607089; font-size: 14px; font-weight: 800; }
    .view-tab.active { border-color: #aac4df; background: #ffffff; color: #244f7c; box-shadow: 0 5px 14px rgba(35, 55, 80, 0.08); }
    .view-panel { display: none; flex: 1; }
    .view-panel.active { display: flex; }
    .card-head { display: flex; min-height: 48px; align-items: center; justify-content: space-between; gap: 10px; padding: 10px 12px; border-bottom: 1px solid #edf2f7; background: #fbfcfe; }
    .card-head strong { color: #172033; font-size: 15px; }
    .card-head span { color: #8793a4; font-size: 12px; }
    .device-row { display: flex; gap: 8px; padding: 12px; }
    .field-input { min-width: 0; height: 38px; flex: 1; padding: 0 10px; border: 1px solid #cbd6e2; border-radius: 7px; background: #ffffff; color: #172033; font-size: 14px; }
    .text-button, .icon-button { display: inline-flex; height: 36px; align-items: center; justify-content: center; gap: 5px; border: 1px solid #c7d5e4; border-radius: 7px; background: #f8fbff; color: #244f7c; font-size: 13px; font-weight: 800; text-decoration: none; }
    .primary-button { border-color: #2f6fa9; background: #2f6fa9; color: #ffffff; }
    .file-list, .message-list { display: flex; min-height: 0; flex: 1; flex-direction: column; overflow: auto; }
    .file-item { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 12px; border-bottom: 1px solid #edf2f7; }
    .file-item:last-child { border-bottom: 0; }
    .file-main { min-width: 0; flex: 1; }
    .file-name { display: block; overflow: hidden; color: #172033; font-size: 14px; font-weight: 800; text-overflow: ellipsis; white-space: nowrap; }
    .file-meta { display: block; margin-top: 4px; color: #748195; font-size: 12px; }
    .file-actions { display: flex; flex: none; gap: 6px; }
    .message-list { padding: 12px; gap: 9px; background: linear-gradient(180deg, #ffffff 0%, #f8fbff 100%); }
    .message { display: flex; max-width: 84%; flex-direction: column; gap: 5px; padding: 9px 11px; border: 0; border-radius: 8px; background: #eef4fb; color: #172033; font-size: 14px; line-height: 1.45; text-align: left; box-shadow: 0 6px 18px rgba(35, 55, 80, 0.06); }
    .message.me { align-self: flex-end; background: #dff3e7; }
    .message small { color: #708197; font-size: 11px; }
    .composer { display: flex; gap: 8px; padding: 10px; border-top: 1px solid #edf2f7; background: #ffffff; }
    .empty { padding: 26px 12px; color: #748195; text-align: center; }
    .preview-dialog { position: fixed; inset: 0; z-index: 10; display: none; align-items: center; justify-content: center; padding: 14px; }
    .preview-dialog.open { display: flex; }
    .preview-overlay { position: absolute; inset: 0; background: rgba(15, 23, 42, 0.42); }
    .preview-panel { position: relative; display: flex; width: min(720px, 100%); max-height: calc(100vh - 28px); flex-direction: column; overflow: hidden; border: 1px solid #dbe4ee; border-radius: 8px; background: #ffffff; box-shadow: 0 20px 54px rgba(15, 23, 42, 0.26); }
    .preview-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; padding: 12px; border-bottom: 1px solid #edf2f7; }
    .preview-title { display: flex; min-width: 0; flex-direction: column; gap: 4px; }
    .preview-title strong { overflow: hidden; color: #172033; font-size: 15px; text-overflow: ellipsis; white-space: nowrap; }
    .preview-title span { color: #748195; font-size: 12px; }
    .preview-body { display: flex; min-height: 260px; flex: 1; align-items: center; justify-content: center; overflow: auto; background: #f7fafd; }
    .preview-body img, .preview-body video { display: block; max-width: 100%; max-height: 68vh; }
    .preview-body audio { width: calc(100% - 28px); }
    .preview-body iframe { width: 100%; height: 68vh; border: 0; background: #ffffff; }
    .preview-body pre { width: 100%; min-height: 260px; max-height: 68vh; margin: 0; overflow: auto; padding: 14px; color: #172033; font-family: "SFMono-Regular", Consolas, monospace; font-size: 12px; line-height: 1.55; white-space: pre-wrap; word-break: break-word; }
    .preview-empty { padding: 28px 18px; color: #697789; line-height: 1.7; text-align: center; }
    .preview-foot { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 12px; border-top: 1px solid #edf2f7; background: #ffffff; }
  </style>
</head>
<body>
  <header class="page-head">
    <div class="page-title">
      <strong>设备快传</strong>
      <span id="status">正在连接电脑端服务...</span>
    </div>
    <span class="page-badge">同网访问</span>
  </header>
  <main class="page-main">
    <section class="card">
      <div class="card-head"><strong>设备名称</strong><span>用于电脑端识别</span></div>
      <div class="device-row">
        <input id="deviceName" class="field-input" maxlength="40" />
        <button id="saveDeviceName" class="text-button" type="button">保存</button>
      </div>
    </section>
    <nav class="view-tabs" aria-label="设备快传详情">
      <button class="view-tab active" data-view="files" type="button">共享文件</button>
      <button class="view-tab" data-view="messages" type="button">消息</button>
    </nav>
    <section id="filesView" class="card view-panel active">
      <div class="card-head"><strong>共享文件</strong><button id="refreshFiles" class="text-button" type="button">刷新</button></div>
      <div id="files" class="file-list"><div class="empty">正在读取文件列表</div></div>
    </section>
    <section id="messagesView" class="card view-panel">
      <div class="card-head"><strong>消息</strong><button id="createSession" class="text-button" type="button">新会话</button></div>
      <div id="messages" class="message-list" aria-label="点击消息可复制"></div>
      <div class="composer">
        <input id="messageInput" class="field-input" maxlength="1000" placeholder="输入消息" />
        <button id="sendMessage" class="text-button primary-button" type="button">发送</button>
      </div>
    </section>
  </main>
  <div id="previewDialog" class="preview-dialog" role="dialog" aria-modal="true">
    <div id="previewOverlay" class="preview-overlay"></div>
    <section class="preview-panel">
      <header class="preview-head">
        <div class="preview-title">
          <strong id="previewName">文件预览</strong>
          <span id="previewMeta"></span>
        </div>
        <button id="closePreview" class="icon-button" type="button">关闭</button>
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
    const statusEl = document.getElementById("status");
    const filesEl = document.getElementById("files");
    const messagesEl = document.getElementById("messages");
    const inputEl = document.getElementById("messageInput");
    const deviceNameEl = document.getElementById("deviceName");
    const viewTabs = Array.from(document.querySelectorAll(".view-tab"));
    const viewPanels = {
      files: document.getElementById("filesView"),
      messages: document.getElementById("messagesView")
    };
    const previewDialogEl = document.getElementById("previewDialog");
    const previewNameEl = document.getElementById("previewName");
    const previewMetaEl = document.getElementById("previewMeta");
    const previewBodyEl = document.getElementById("previewBody");
    const previewDownloadEl = document.getElementById("previewDownload");
    let ws = null;
    let deviceId = "";
    let currentSessionId = "";
    const messageIds = new Set();
    deviceNameEl.value = localStorage.getItem("lanShareDeviceName") || "";

    function formatSize(size) {
      const value = Number(size || 0);
      if (value >= 1024 * 1024 * 1024) return (value / 1024 / 1024 / 1024).toFixed(2) + " GB";
      if (value >= 1024 * 1024) return (value / 1024 / 1024).toFixed(2) + " MB";
      if (value >= 1024) return (value / 1024).toFixed(1) + " KB";
      return value + " B";
    }

    function setStatus(text) {
      statusEl.textContent = text;
    }

    function setActiveView(view) {
      viewTabs.forEach(tab => {
        tab.classList.toggle("active", tab.dataset.view === view);
      });
      Object.entries(viewPanels).forEach(([name, panel]) => {
        panel.classList.toggle("active", name === view);
      });
    }

    async function copyMessageText(text) {
      try {
        await navigator.clipboard.writeText(text || "");
        setStatus("消息已复制");
      } catch (error) {
        setStatus("复制失败，请手动选择文本复制");
      }
    }

    async function api(path, options) {
      const response = await fetch(path, options);
      const payload = await response.json();
      if (!response.ok || payload.status !== "success") {
        throw new Error(payload.message || "请求失败");
      }
      return payload.data;
    }

    function fileUrl(file, action) {
      return "/api/files/" + action + "?id=" + encodeURIComponent(file.id) + "&token=" + encodeURIComponent(token) + "&deviceId=" + encodeURIComponent(deviceId) + "&sessionId=" + encodeURIComponent(currentSessionId);
    }

    function previewKind(file) {
      const name = String(file.name || "").toLowerCase();
      const mime = String(file.mimeType || "").toLowerCase();
      if (mime.startsWith("image/")) return "image";
      if (mime.startsWith("video/")) return "video";
      if (mime.startsWith("audio/")) return "audio";
      if (mime === "application/pdf" || name.endsWith(".pdf")) return "pdf";
      if (mime.startsWith("text/")) return "text";
      if (["application/json", "application/xml", "application/javascript", "application/x-javascript", "application/xhtml+xml", "image/svg+xml"].includes(mime)) return "text";
      if ([".txt", ".md", ".json", ".xml", ".csv", ".log", ".js", ".ts", ".css", ".html", ".vue", ".rs", ".py", ".java", ".c", ".cpp", ".h", ".go", ".yaml", ".yml", ".toml"].some(ext => name.endsWith(ext))) return "text";
      return "unsupported";
    }

    function closePreview() {
      previewBodyEl.innerHTML = "";
      previewDialogEl.classList.remove("open");
    }

    async function previewFile(file) {
      const kind = previewKind(file);
      const previewUrl = fileUrl(file, "preview");
      const downloadUrl = fileUrl(file, "download");
      previewNameEl.textContent = file.name || "文件预览";
      previewMetaEl.textContent = formatSize(file.size) + " · " + (file.mimeType || "文件");
      previewDownloadEl.href = downloadUrl;
      previewBodyEl.innerHTML = '<div class="preview-empty">正在准备预览...</div>';
      previewDialogEl.classList.add("open");

      if (kind === "image") {
        previewBodyEl.innerHTML = "";
        const image = document.createElement("img");
        image.alt = file.name || "图片预览";
        image.src = previewUrl;
        previewBodyEl.appendChild(image);
        return;
      }
      if (kind === "video") {
        previewBodyEl.innerHTML = "";
        const video = document.createElement("video");
        video.controls = true;
        video.src = previewUrl;
        previewBodyEl.appendChild(video);
        return;
      }
      if (kind === "audio") {
        previewBodyEl.innerHTML = "";
        const audio = document.createElement("audio");
        audio.controls = true;
        audio.src = previewUrl;
        previewBodyEl.appendChild(audio);
        return;
      }
      if (kind === "pdf") {
        previewBodyEl.innerHTML = "";
        const frame = document.createElement("iframe");
        frame.title = file.name || "PDF 预览";
        frame.src = previewUrl;
        previewBodyEl.appendChild(frame);
        return;
      }
      if (kind === "text") {
        try {
          const response = await fetch(previewUrl);
          if (!response.ok) throw new Error("当前文件类型不支持在线预览");
          const text = await response.text();
          const pre = document.createElement("pre");
          pre.textContent = text;
          previewBodyEl.innerHTML = "";
          previewBodyEl.appendChild(pre);
        } catch (error) {
          previewBodyEl.innerHTML = '<div class="preview-empty">' + (error.message || "预览失败，请下载后打开。") + '</div>';
        }
        return;
      }

      previewBodyEl.innerHTML = '<div class="preview-empty">浏览器暂不支持直接预览这种文件，可下载到本地后打开。</div>';
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
    }

    async function loadFiles() {
      const files = await api("/api/files?token=" + encodeURIComponent(token) + "&sessionId=" + encodeURIComponent(currentSessionId));
      filesEl.innerHTML = "";
      if (!files.length) {
        filesEl.innerHTML = '<div class="empty">电脑端还没有共享文件</div>';
        return;
      }
      for (const file of files) {
        const item = document.createElement("div");
        const downloadUrl = fileUrl(file, "download");
        item.className = "file-item";
        item.innerHTML = '<span class="file-main"><span class="file-name"></span><span class="file-meta"></span></span><span class="file-actions"><button class="file-preview text-button" type="button">预览</button><a class="file-download text-button">下载</a></span>';
        item.querySelector(".file-name").textContent = file.name;
        item.querySelector(".file-meta").textContent = formatSize(file.size) + " · " + (file.mimeType || "文件");
        item.querySelector(".file-preview").onclick = () => previewFile(file);
        item.querySelector(".file-download").href = downloadUrl;
        filesEl.appendChild(item);
      }
    }

    async function loadMessages() {
      const data = await api("/api/messages?token=" + encodeURIComponent(token) + "&deviceId=" + encodeURIComponent(deviceId) + "&sessionId=" + encodeURIComponent(currentSessionId));
      currentSessionId = data.currentSession?.id || currentSessionId;
      messagesEl.innerHTML = "";
      messageIds.clear();
      for (const message of data.messages || []) {
        appendMessage(message);
      }
      if (!messageIds.size) {
        messagesEl.innerHTML = '<div class="empty">暂无消息，发送一条开始对话</div>';
      }
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
      currentSessionId = data.currentSession?.id || "";
      messagesEl.innerHTML = '<div class="empty">新会话已创建，可以开始发送消息</div>';
      messageIds.clear();
      await loadFiles();
    }

    function appendMessage(message) {
      if (messageIds.has(message.id)) {
        return;
      }
      if (!messageIds.size) {
        messagesEl.innerHTML = "";
      }
      messageIds.add(message.id);
      const item = document.createElement("button");
      item.className = "message" + (message.direction === "mobile-to-desktop" ? " me" : "");
      item.type = "button";
      item.title = "点击消息可复制";
      item.textContent = message.content || "";
      item.onclick = () => copyMessageText(message.content || "");
      const time = document.createElement("small");
      time.textContent = new Date(message.createdAt || Date.now()).toLocaleTimeString();
      item.appendChild(time);
      messagesEl.appendChild(item);
      messagesEl.scrollTop = messagesEl.scrollHeight;
    }

    function connectWs() {
      const protocol = location.protocol === "https:" ? "wss:" : "ws:";
      ws = new WebSocket(protocol + "//" + location.host + "/ws?token=" + encodeURIComponent(token) + "&deviceId=" + encodeURIComponent(deviceId));
      ws.onopen = () => setStatus("已连接电脑端服务");
      ws.onclose = () => {
        setStatus("连接已断开，正在重连...");
        window.setTimeout(connectWs, 1600);
      };
      ws.onmessage = event => {
        const payload = JSON.parse(event.data);
        if (payload.type === "message" && (!currentSessionId || payload.message?.sessionId === currentSessionId)) {
          appendMessage(payload.message);
          setActiveView("messages");
        }
        if (payload.type === "filesChanged") loadFiles().catch(error => setStatus(error.message));
      };
    }

    document.getElementById("saveDeviceName").onclick = () => {
      localStorage.setItem("lanShareDeviceName", deviceNameEl.value);
      registerDevice()
        .then(loadMessages)
        .catch(error => setStatus(error.message));
    };
    viewTabs.forEach(tab => {
      tab.onclick = () => setActiveView(tab.dataset.view);
    });
    document.getElementById("refreshFiles").onclick = () => loadFiles().catch(error => setStatus(error.message));
    document.getElementById("createSession").onclick = () => createSession().catch(error => setStatus(error.message));
    document.getElementById("closePreview").onclick = closePreview;
    document.getElementById("previewOverlay").onclick = closePreview;
    document.getElementById("sendMessage").onclick = () => {
      const content = inputEl.value.trim();
      if (!content || !ws || ws.readyState !== WebSocket.OPEN) return;
      ws.send(JSON.stringify({ type: "message", content, sessionId: currentSessionId }));
      inputEl.value = "";
    };
    inputEl.addEventListener("keydown", event => {
      if (event.key === "Enter") document.getElementById("sendMessage").click();
    });

    loadFiles().catch(error => setStatus(error.message));
    registerDevice()
      .then(() => Promise.all([loadMessages(), loadFiles()]))
      .then(connectWs)
      .catch(error => setStatus(error.message));
  </script>
</body>
</html>"#
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
    let mut runtime = registry.inner.lock().await;

    if runtime.handle.is_some() {
        return Ok(lan_share_response(json!({
            "running": true,
            "accessUrl": runtime.access_url,
            "qrSvg": qr_svg(&runtime.access_url)?,
            "lanIp": runtime.lan_ip,
            "port": runtime.port,
            "onlineDevices": runtime.clients.len()
        })));
    }

    let lan_ip = find_lan_ip()?;
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
        loop {
            let Ok((stream, addr)) = listener.accept().await else {
                break;
            };
            let io = TokioIo::new(stream);
            let context = context.clone();

            let connection_registry = context.registry.clone();
            let connection_handle = tauri::async_runtime::spawn(async move {
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
            connection_registry
                .inner
                .lock()
                .await
                .connections
                .push(connection_handle);
        }
    });

    runtime.handle = Some(handle);
    runtime.token = token;
    runtime.access_url = access_url.clone();
    runtime.lan_ip = lan_ip.clone();
    runtime.port = port;
    drop(runtime);
    emit_state_changed(&app, registry, paths).await?;

    Ok(lan_share_response(json!({
        "running": true,
        "accessUrl": access_url,
        "qrSvg": qr_svg(&access_url)?,
        "lanIp": lan_ip,
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
    if let Some(handle) = runtime.handle.take() {
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

async fn emit_state_changed(
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
    let canonical_device_id = if ip.is_empty() {
        device_id.to_string()
    } else {
        device_id_from_ip(ip)
    };

    let device_index = devices
        .iter()
        .position(|device| device.id == canonical_device_id)
        .or_else(|| devices.iter().position(|device| device.id == device_id))
        .or_else(|| {
            if ip.is_empty() {
                None
            } else {
                devices.iter().position(|device| device.ip == ip)
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
            device.id == canonical_device_id || device.ip != ip || device.ip.is_empty()
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
        id: create_id("session"),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        ip: ip.to_string(),
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

pub async fn create_session(
    registry: &LanShareServerRegistry,
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let device_id = string_value(payload.get("deviceId"));

    if device_id.is_empty() {
        return Err(ManagerError::System("设备不能为空。".to_string()));
    }

    let (session, sessions) = {
        let _storage = registry.storage.lock().await;
        let device = find_device_for_session(paths, &device_id)?;
        let device_name = device_display_name(&device);
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let now = now_millis();
        let session = LanShareSession {
            id: create_id("session"),
            device_id: device_id.to_string(),
            device_name,
            ip: device.ip,
            created_at: now,
            updated_at: now,
        };

        sessions.insert(0, session.clone());
        sort_sessions(&mut sessions);
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        (session, sessions)
    };
    registry
        .inner
        .lock()
        .await
        .active_sessions
        .insert(device_id, session.id.clone());

    let mut state = get_state(registry, paths).await?;
    state["data"]["currentSession"] = json!(session);
    state["data"]["sessions"] = json!(sessions);
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

    let (current_session, sessions) = {
        let _storage = registry.storage.lock().await;
        let mut sessions: Vec<LanShareSession> = read_array(&paths.lan_share_files.sessions)?;
        let Some(index) = sessions.iter().position(|session| session.id == session_id) else {
            return Err(ManagerError::System("会话不存在。".to_string()));
        };

        sessions[index].updated_at = now_millis();
        let current_session = sessions[index].clone();
        sort_sessions(&mut sessions);
        write_json(&paths.lan_share_files.sessions, &json!(sessions)).await?;
        (current_session, sessions)
    };
    registry.inner.lock().await.active_sessions.insert(
        current_session.device_id.clone(),
        current_session.id.clone(),
    );

    let mut state = get_state(registry, paths).await?;
    state["data"]["currentSession"] = json!(current_session);
    state["data"]["sessions"] = json!(sessions);
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
    let content = content.trim().chars().take(1000).collect::<String>();

    if content.is_empty() {
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
    let device_name = device_display_name(&device);
    let session = ensure_session(
        paths,
        device_id,
        &selected_session_id,
        &device_name,
        &device.ip,
    )?;
    let mut messages: Vec<LanShareMessage> = read_array(&paths.lan_share_files.messages)?;
    let message = LanShareMessage {
        id: create_id("message"),
        session_id: session.id,
        device_id: device_id.to_string(),
        device_name,
        direction: direction.to_string(),
        message_type: "text".to_string(),
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

    if content.is_empty() {
        return Err(ManagerError::System("消息内容不能为空。".to_string()));
    }

    let target_device_id = string_value(payload.get("deviceId"));
    let target_session_id = string_value(payload.get("sessionId"));
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
        let delivered = sender.is_some();
        let stored = append_message(
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
            delivered,
        )
        .await?;

        if let Some(sender) = sender {
            let _ = send_stored_message_to_client(&sender, &stored);
        }

        emit_message_created(&app, &stored);
        sent_messages.push(stored);
    }

    Ok(lan_share_response(json!(sent_messages)))
}

async fn handle_http_request(
    request: Request<Incoming>,
    context: HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, Infallible> {
    let response = match process_http_request(request, context, addr).await {
        Ok(response) => response,
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };

    Ok(response)
}

async fn process_http_request(
    request: Request<Incoming>,
    context: HttpContext,
    addr: SocketAddr,
) -> Result<Response<BoxBody>, ManagerError> {
    let path = request.uri().path().to_string();
    let request_token = if request.method() == Method::POST
        && (path == "/api/devices" || path == "/api/sessions")
    {
        String::new()
    } else {
        query_value(request.uri(), "token")
    };

    if request.method() != Method::POST || (path != "/api/devices" && path != "/api/sessions") {
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
        (&Method::GET, "/api/messages") => mobile_messages(request.uri(), &context, addr).await,
        (&Method::POST, "/api/devices") => register_device_request(request, &context, addr).await,
        (&Method::POST, "/api/sessions") => create_mobile_session(request, &context, addr).await,
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

    let files = {
        let _storage = registry.storage.lock().await;
        read_array::<LanShareFile>(&paths.lan_share_files.files)?
            .into_iter()
            .filter(|file| file.enabled && file.session_id == session_id)
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
    let file = {
        let _storage = context.registry.storage.lock().await;
        read_array::<LanShareFile>(&context.paths.lan_share_files.files)?
            .into_iter()
            .find(|file| {
                file.id == file_id
                    && file.enabled
                    && (session_id.is_empty() || file.session_id == session_id)
            })
    };
    let Some(file) = file else {
        return Ok(api_error(StatusCode::NOT_FOUND, "文件不存在或未共享。"));
    };
    let stream_file = File::open(&file.path).await?;
    let metadata = stream_file.metadata().await?;
    let stream = ReaderStream::new(stream_file).map_ok(Frame::data);
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
    let file = {
        let _storage = context.registry.storage.lock().await;
        read_array::<LanShareFile>(&context.paths.lan_share_files.files)?
            .into_iter()
            .find(|file| {
                file.id == file_id
                    && file.enabled
                    && (session_id.is_empty() || file.session_id == session_id)
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
    let stream = ReaderStream::new(stream_file).map_ok(Frame::data);
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

        messages.retain(|message| message.session_id == session.id);
        messages.sort_by(|left, right| left.created_at.cmp(&right.created_at));
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
            id: create_id("download"),
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
        let mut runtime = context.registry.inner.lock().await;
        if !is_valid_token(&runtime.token, &request_token) {
            return Ok(());
        }
        runtime.clients.insert(device_id.clone(), sender);
    }
    emit_state_changed(&context.app, &context.registry, &context.paths).await?;

    let writer_handle = tauri::async_runtime::spawn(async move {
        while let Some(outbound) = receiver.recv().await {
            if writer
                .send(Message::Text(outbound.payload.to_string().into()))
                .await
                .is_err()
            {
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

        let payload = serde_json::from_str::<Value>(&message.to_text().unwrap_or_default())
            .unwrap_or_else(|_| json!({}));

        if payload.get("type").and_then(Value::as_str) == Some("message") {
            let content = string_value(payload.get("content"));
            let session_id = string_value(payload.get("sessionId"));

            if content.is_empty() {
                continue;
            }

            let message = append_message(
                &context.registry,
                &context.paths,
                &device_id,
                &session_id,
                "mobile-to-desktop",
                &content,
                true,
            )
            .await?;

            let _ = send_stored_message_to_client(&client_sender, &message);
            emit_message_created(&context.app, &message);
            emit_state_changed(&context.app, &context.registry, &context.paths).await?;
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
        activate_session, add_files, append_message, create_file_id, create_id, create_session,
        device_id_from_ip, list_messages, mobile_files_payload, refresh_files, string_value,
        upsert_device, LanShareDevice, LanShareFile, LanShareMessage, LanShareServerRegistry,
    };
    use crate::core::paths::resolve_app_paths;
    use serde_json::json;
    use std::path::Path;

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
        assert!(device_id_from_ip("192.168.1.8").starts_with("device_"));
    }

    #[test]
    fn trims_string_values() {
        assert_eq!(string_value(Some(&json!("  hello  "))), "hello");
        assert_eq!(string_value(None), "");
    }

    #[test]
    fn create_id_includes_uuid_suffix() {
        let id = create_id("msg");
        let parts: Vec<&str> = id.splitn(3, '_').collect();

        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "msg");
        uuid::Uuid::parse_str(parts[2]).expect("create_id should include a uuid suffix");
    }

    #[test]
    fn list_messages_searches_keyword_in_content_only() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
    fn creates_new_session_bound_to_device_ip_without_reusing_previous_one() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
    fn appends_desktop_message_to_last_activated_session_by_default() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
    fn desktop_message_to_offline_session_is_kept_for_later_sync() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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
    fn state_includes_qr_svg_for_running_access_url() {
        tauri::async_runtime::block_on(async {
            let registry = LanShareServerRegistry::new();
            let root = std::env::temp_dir().join(create_id("lan_share_test"));
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

                stop_runtime(&mut runtime);

                assert!(runtime.token.is_empty());
                assert!(runtime.access_url.is_empty());
                assert!(runtime.clients.is_empty());
                assert!(runtime.connections.is_empty());
            }

            assert!(registry.inner.lock().await.clients.get("device").is_none());
        });
    }
}

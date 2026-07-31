use crate::core::error::ManagerError;
use crate::core::paths::{path_text, AppPaths};
use base64::Engine;
use bytes::Bytes;
use http::{Method, StatusCode};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::net::TcpListener;
#[cfg(target_os = "windows")]
use tokio::process::Command;
use tokio::task::JoinHandle;

const INDEX_HTML: &str = include_str!("../../toolbox-panel/index.html");
const STYLE_CSS: &str = include_str!("../../toolbox-panel/styles.css");
const APP_JS: &str = include_str!("../../toolbox-panel/app.js");
const TOOL_REGISTRY_JS: &str = include_str!("../../toolbox-panel/tools/registry.js");
const IMAGE_LINK_EXTRACTOR_JS: &str =
    include_str!("../../toolbox-panel/tools/image-link-extractor.js");
const STRING_DIFF_JS: &str = include_str!("../../toolbox-panel/tools/string-diff.js");

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct ToolboxServerRegistry {
    runtime: Option<ToolboxServerRuntime>,
}

struct ToolboxServerRuntime {
    url: String,
    _handle: JoinHandle<()>,
}

impl ToolboxServerRegistry {
    pub fn new() -> Self {
        Self { runtime: None }
    }
}

pub async fn open_toolbox(
    app: &AppHandle,
    registry: &mut ToolboxServerRegistry,
) -> Result<Value, ManagerError> {
    let url = ensure_toolbox_server(registry).await?;

    app.opener()
        .open_url(url.clone(), None::<&str>)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(json!({
      "url": url
    }))
}

async fn ensure_toolbox_server(
    registry: &mut ToolboxServerRegistry,
) -> Result<String, ManagerError> {
    if let Some(runtime) = &registry.runtime {
        if !runtime._handle.is_finished() {
            return Ok(runtime.url.clone());
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let url = format!("http://127.0.0.1:{}/", port);
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _addr)) = listener.accept().await else {
                break;
            };

            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                if let Err(error) = http1::Builder::new()
                    .serve_connection(io, service_fn(handle_toolbox_request))
                    .await
                {
                    eprintln!("[实用工具服务] 请求处理失败: {}", error);
                }
            });
        }
    });

    registry.runtime = Some(ToolboxServerRuntime {
        url: url.clone(),
        _handle: handle,
    });

    Ok(url)
}

async fn handle_toolbox_request(
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    if request.method() != Method::GET {
        return Ok(response(
            StatusCode::METHOD_NOT_ALLOWED,
            "text/plain; charset=utf-8",
            "仅支持 GET 请求",
        ));
    }

    let path = request.uri().path();
    let response = match path {
        "/" | "/index.html" => response(StatusCode::OK, "text/html; charset=utf-8", INDEX_HTML),
        "/styles.css" => response(StatusCode::OK, "text/css; charset=utf-8", STYLE_CSS),
        "/app.js" => response(StatusCode::OK, "text/javascript; charset=utf-8", APP_JS),
        "/tools/registry.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            TOOL_REGISTRY_JS,
        ),
        "/tools/image-link-extractor.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            IMAGE_LINK_EXTRACTOR_JS,
        ),
        "/tools/string-diff.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            STRING_DIFF_JS,
        ),
        _ => response(
            StatusCode::NOT_FOUND,
            "text/plain; charset=utf-8",
            "未找到工具资源",
        ),
    };

    Ok(response)
}

fn response(status: StatusCode, content_type: &str, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", content_type)
        .header("cache-control", "no-store")
        .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminatePortProcessPayload {
    pid: u32,
    started_at: i64,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsPortRecord {
    protocol: String,
    local_address: String,
    local_port: u16,
    pid: u32,
    process_name: String,
    executable_path: String,
    service_names: Vec<String>,
    started_at: i64,
}

pub async fn list_ports() -> Result<Value, ManagerError> {
    #[cfg(not(target_os = "windows"))]
    {
        Err(ManagerError::System(
            "端口监测目前仅支持 Windows".to_string(),
        ))
    }

    #[cfg(target_os = "windows")]
    {
        let script = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$processMap = @{}
Get-Process -ErrorAction SilentlyContinue | ForEach-Object {
  $processMap[[string]$_.Id] = $_
}

$serviceMap = @{}
try {
  Get-CimInstance Win32_Service -ErrorAction Stop | Where-Object { $_.ProcessId -gt 0 } | ForEach-Object {
    $key = [string]$_.ProcessId
    if ($serviceMap.ContainsKey($key)) {
      $serviceMap[$key] = @($serviceMap[$key]) + [string]$_.DisplayName
    } else {
      $serviceMap[$key] = @([string]$_.DisplayName)
    }
  }
} catch {}

$records = @(
  Get-NetTCPConnection -State Listen -ErrorAction Stop | ForEach-Object {
    $ownerId = [int]$_.OwningProcess
    $process = $processMap[[string]$ownerId]
    $processName = ''
    $processPath = ''
    $startedAt = 0
    if ($null -ne $process) {
      $processName = [string]$process.ProcessName
      try { $processPath = [string]$process.Path } catch {}
      try {
        $started = [DateTimeOffset]$process.StartTime
        $startedAt = $started.ToUnixTimeSeconds()
      } catch {}
    }

    [PSCustomObject]@{
      protocol = 'TCP'
      localAddress = [string]$_.LocalAddress
      localPort = [int]$_.LocalPort
      pid = $ownerId
      processName = $processName
      executablePath = $processPath
      serviceNames = @($serviceMap[[string]$ownerId] | Where-Object { $null -ne $_ })
      startedAt = $startedAt
    }
  }

  Get-NetUDPEndpoint -ErrorAction Stop | ForEach-Object {
    $ownerId = [int]$_.OwningProcess
    $process = $processMap[[string]$ownerId]
    $processName = ''
    $processPath = ''
    $startedAt = 0
    if ($null -ne $process) {
      $processName = [string]$process.ProcessName
      try { $processPath = [string]$process.Path } catch {}
      try {
        $started = [DateTimeOffset]$process.StartTime
        $startedAt = $started.ToUnixTimeSeconds()
      } catch {}
    }

    [PSCustomObject]@{
      protocol = 'UDP'
      localAddress = [string]$_.LocalAddress
      localPort = [int]$_.LocalPort
      pid = $ownerId
      processName = $processName
      executablePath = $processPath
      serviceNames = @($serviceMap[[string]$ownerId] | Where-Object { $null -ne $_ })
      startedAt = $startedAt
    }
  }
)

ConvertTo-Json -InputObject @($records | Sort-Object localPort, protocol, pid) -Compress -Depth 4
"#;
        let mut command = Command::new("powershell.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(ManagerError::System(
                "读取端口失败，请确认系统网络管理服务可用".to_string(),
            ));
        }

        let content = String::from_utf8_lossy(&output.stdout);
        let records: Vec<WindowsPortRecord> = serde_json::from_str(content.trim())
            .map_err(|_| ManagerError::System("无法解析系统返回的端口信息".to_string()))?;
        let ports = records
            .into_iter()
            .map(|record| {
                let protected_reason = protected_process_reason(record.pid, &record.process_name)
                    .map(str::to_string)
                    .or_else(|| {
                        if record.process_name.trim().is_empty() {
                            Some("无法读取进程信息".to_string())
                        } else if record.started_at <= 0 {
                            Some("无法校验进程启动时间".to_string())
                        } else {
                            None
                        }
                    });

                json!({
                  "id": format!(
                    "{}:{}:{}:{}",
                    record.protocol, record.local_address, record.local_port, record.pid
                  ),
                  "protocol": record.protocol,
                  "localAddress": record.local_address,
                  "localPort": record.local_port,
                  "pid": record.pid,
                  "processName": record.process_name,
                  "executablePath": record.executable_path,
                  "serviceNames": record.service_names,
                  "startedAt": record.started_at,
                  "canTerminate": protected_reason.is_none(),
                  "protectedReason": protected_reason.unwrap_or_default()
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({ "ports": ports }))
    }
}

pub async fn terminate_port_process(payload: Value) -> Result<Value, ManagerError> {
    let payload: TerminatePortProcessPayload = serde_json::from_value(payload)?;

    #[cfg(not(target_os = "windows"))]
    {
        let _ = payload;
        Err(ManagerError::System(
            "进程关闭目前仅支持 Windows".to_string(),
        ))
    }

    #[cfg(target_os = "windows")]
    {
        if payload.started_at <= 0 {
            return Err(ManagerError::System("进程校验信息不完整".to_string()));
        }

        // 关闭前重新读取进程身份，避免端口列表中的 PID 已被系统复用。
        let inspect_script = format!(
            r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$process = Get-Process -Id {} -ErrorAction Stop
$started = [DateTimeOffset]$process.StartTime
[PSCustomObject]@{{
  processName = [string]$process.ProcessName
  startedAt = $started.ToUnixTimeSeconds()
}} | ConvertTo-Json -Compress
"#,
            payload.pid
        );
        let mut inspect_command = Command::new("powershell.exe");
        inspect_command.creation_flags(CREATE_NO_WINDOW);
        let inspect_output = inspect_command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &inspect_script,
            ])
            .output()
            .await?;

        if !inspect_output.status.success() {
            return Err(ManagerError::System(
                "进程已退出或当前无权读取该进程".to_string(),
            ));
        }

        let process: Value = serde_json::from_slice(&inspect_output.stdout)
            .map_err(|_| ManagerError::System("无法校验目标进程".to_string()))?;
        let process_name = process
            .get("processName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let started_at = process
            .get("startedAt")
            .and_then(Value::as_i64)
            .unwrap_or_default();

        if started_at != payload.started_at {
            return Err(ManagerError::System(
                "目标 PID 已被其他进程占用，请刷新列表后重试".to_string(),
            ));
        }
        if let Some(reason) = protected_process_reason(payload.pid, process_name) {
            return Err(ManagerError::System(reason.to_string()));
        }

        let mut terminate_command = Command::new("taskkill.exe");
        terminate_command.creation_flags(CREATE_NO_WINDOW);
        let output = terminate_command
            .args(["/PID", &payload.pid.to_string(), "/T", "/F"])
            .output()
            .await?;

        if !output.status.success() {
            return Err(ManagerError::System(format!(
                "无法关闭进程 {}，请尝试以管理员身份运行应用",
                payload.pid
            )));
        }

        Ok(json!({ "pid": payload.pid }))
    }
}

fn protected_process_reason(pid: u32, process_name: &str) -> Option<&'static str> {
    if pid == std::process::id() {
        return Some("不能在端口监测中关闭当前应用");
    }
    if pid <= 4 {
        return Some("系统核心进程不可关闭");
    }

    let process_name = process_name
        .trim()
        .trim_end_matches(".exe")
        .to_ascii_lowercase();
    if matches!(
        process_name.as_str(),
        "system"
            | "registry"
            | "smss"
            | "csrss"
            | "wininit"
            | "services"
            | "lsass"
            | "winlogon"
            | "svchost"
            | "fontdrvhost"
            | "secure system"
            | "memory compression"
    ) {
        return Some("系统关键进程不可关闭");
    }

    None
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameCodexPetPayload {
    id: String,
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToggleCodexPetPayload {
    id: String,
    enabled: bool,
}

#[derive(Deserialize)]
struct CodexPetIdPayload {
    id: String,
}

pub async fn list_codex_pets(paths: &AppPaths, cli_targets: &Value) -> Result<Value, ManagerError> {
    let codex_pets_dir = codex_pets_dir(cli_targets)?;

    tokio::fs::create_dir_all(&codex_pets_dir).await?;
    tokio::fs::create_dir_all(&paths.disabled_pets_dir).await?;
    migrate_legacy_codex_pets(paths, &codex_pets_dir).await?;

    let mut pets = read_pets(&codex_pets_dir, true).await?;
    let disabled_pets = read_pets(Path::new(&paths.disabled_pets_dir), false).await?;
    if disabled_pets.iter().any(|disabled_pet| {
        pets.iter()
            .any(|pet| pet_string(pet, "id") == pet_string(disabled_pet, "id"))
    }) {
        return Err(ManagerError::System(
            "Codex 目录与已禁用目录存在同名宠物".to_string(),
        ));
    }
    pets.extend(disabled_pets);
    pets.sort_by(|left, right| {
        pet_string(left, "displayName").cmp(&pet_string(right, "displayName"))
    });

    Ok(json!({
      "codexPetsPath": path_text(&codex_pets_dir),
      "disabledPetsPath": paths.disabled_pets_dir,
      "pets": pets
    }))
}

pub async fn rename_codex_pet(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let payload: RenameCodexPetPayload = serde_json::from_value(payload)?;
    let id = valid_pet_id(&payload.id)?;
    let display_name = payload.display_name.trim();

    if display_name.is_empty() {
        return Err(ManagerError::System("宠物名称不能为空".to_string()));
    }

    let codex_pets_dir = codex_pets_dir(cli_targets)?;
    let pet_dir = codex_pet_dir(&codex_pets_dir, Path::new(&paths.disabled_pets_dir), id)?;
    let pet_json_path = pet_dir.join("pet.json");
    let content = tokio::fs::read_to_string(&pet_json_path).await?;
    let mut pet_json: Value = serde_json::from_str(&content)?;
    let Some(pet) = pet_json.as_object_mut() else {
        return Err(ManagerError::System(format!(
            "宠物配置不是 JSON 对象：{}",
            path_text(&pet_json_path)
        )));
    };

    pet.insert("displayName".to_string(), json!(display_name));
    tokio::fs::write(
        &pet_json_path,
        format!("{}\n", serde_json::to_string_pretty(&pet_json)?),
    )
    .await?;

    Ok(json!({ "id": id, "displayName": display_name }))
}

pub async fn toggle_codex_pet(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let payload: ToggleCodexPetPayload = serde_json::from_value(payload)?;
    let id = valid_pet_id(&payload.id)?;
    let codex_pets_dir = codex_pets_dir(cli_targets)?;

    let active_path = codex_pets_dir.join(id);
    let disabled_path = Path::new(&paths.disabled_pets_dir).join(id);

    if payload.enabled {
        if active_path.exists() {
            return Ok(json!({ "id": id, "enabled": true }));
        }
        if !disabled_path.exists() {
            return Err(ManagerError::System(format!("未找到宠物：{}", id)));
        }

        move_pet_dir(&disabled_path, &active_path).await?;
    } else {
        if disabled_path.exists() {
            return Ok(json!({ "id": id, "enabled": false }));
        }
        if !active_path.exists() {
            return Err(ManagerError::System(format!("未找到宠物：{}", id)));
        }

        move_pet_dir(&active_path, &disabled_path).await?;
    }

    Ok(json!({ "id": id, "enabled": payload.enabled }))
}

pub async fn delete_codex_pet(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let payload: CodexPetIdPayload = serde_json::from_value(payload)?;
    let id = valid_pet_id(&payload.id)?;
    let codex_pets_dir = codex_pets_dir(cli_targets)?;

    let active_path = codex_pets_dir.join(id);
    let disabled_path = Path::new(&paths.disabled_pets_dir).join(id);

    if active_path.exists() {
        tokio::fs::remove_dir_all(&active_path).await?;
    } else if disabled_path.exists() {
        tokio::fs::remove_dir_all(&disabled_path).await?;
    } else {
        return Err(ManagerError::System(format!("未找到宠物：{}", id)));
    }

    Ok(json!({ "id": id }))
}

fn codex_pets_dir(cli_targets: &Value) -> Result<PathBuf, ManagerError> {
    let Some(codex_target) = cli_targets.as_array().and_then(|targets| {
        targets.iter().find(|target| {
            target.get("id").and_then(Value::as_str) == Some("codex")
                && target.get("installed").and_then(Value::as_bool) == Some(true)
        })
    }) else {
        return Err(ManagerError::System("未检测到已安装的 Codex".to_string()));
    };
    let config_path = pet_string(codex_target, "configPath");

    if config_path.is_empty() {
        return Err(ManagerError::System("Codex 配置目录不存在".to_string()));
    }

    Ok(Path::new(&config_path).join("pets"))
}

// 将旧版本遗留在应用目录中的启用宠物还原到 Codex 目录，后续不再创建链接。
async fn migrate_legacy_codex_pets(
    paths: &AppPaths,
    codex_pets_dir: &Path,
) -> Result<(), ManagerError> {
    let legacy_pets_dir = Path::new(&paths.pets_dir);
    let Ok(mut entries) = tokio::fs::read_dir(legacy_pets_dir).await else {
        return Ok(());
    };

    while let Some(entry) = entries.next_entry().await? {
        let source_path = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();

        if valid_pet_id(&id).is_err() || !is_codex_pet_directory(&source_path).await {
            continue;
        }

        let target_path = codex_pets_dir.join(&id);
        if target_path.exists() {
            let stat = tokio::fs::symlink_metadata(&target_path).await?;
            if !stat.file_type().is_symlink() || !linked_to(&target_path, &source_path).await {
                return Err(ManagerError::System(format!(
                    "Codex 宠物目录与旧版受管宠物冲突：{}",
                    path_text(&target_path)
                )));
            }

            remove_legacy_pet_link(&target_path).await?;
        }

        move_pet_dir(&source_path, &target_path).await?;
    }

    Ok(())
}

async fn read_pets(pets_dir: &Path, enabled: bool) -> Result<Vec<Value>, ManagerError> {
    let mut entries = tokio::fs::read_dir(pets_dir).await?;
    let mut pets = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let pet_dir = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();

        if valid_pet_id(&id).is_err() || !is_codex_pet_directory(&pet_dir).await {
            continue;
        }

        let pet_json_path = pet_dir.join("pet.json");
        let pet_json = tokio::fs::read_to_string(&pet_json_path)
            .await
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .unwrap_or_else(|| json!({}));
        let spritesheet = tokio::fs::read(pet_dir.join("spritesheet.webp")).await?;
        let display_name = pet_string(&pet_json, "displayName");

        pets.push(json!({
          "id": id,
          "displayName": if display_name.is_empty() { pet_string(&pet_json, "id") } else { display_name },
          "description": pet_string(&pet_json, "description"),
          "enabled": enabled,
          "shape": "8 x 9 动画精灵",
          "spritesheetData": format!("data:image/webp;base64,{}", base64::engine::general_purpose::STANDARD.encode(spritesheet))
        }));
    }

    Ok(pets)
}

pub(crate) async fn is_codex_pet_directory(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .map(|stat| stat.is_dir())
        .unwrap_or(false)
        && tokio::fs::metadata(path.join("pet.json"))
            .await
            .map(|stat| stat.is_file())
            .unwrap_or(false)
        && tokio::fs::metadata(path.join("spritesheet.webp"))
            .await
            .map(|stat| stat.is_file())
            .unwrap_or(false)
}

fn codex_pet_dir(
    codex_pets_dir: &Path,
    disabled_pets_dir: &Path,
    id: &str,
) -> Result<PathBuf, ManagerError> {
    let active_path = codex_pets_dir.join(id);

    if active_path.exists() {
        return Ok(active_path);
    }

    let disabled_path = disabled_pets_dir.join(id);
    if disabled_path.exists() {
        return Ok(disabled_path);
    }

    Err(ManagerError::System(format!("未找到宠物：{}", id)))
}

fn valid_pet_id(id: &str) -> Result<&str, ManagerError> {
    let id = id.trim();
    let path = Path::new(id);

    if id.is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(ManagerError::System("宠物标识不合法".to_string()));
    }

    Ok(id)
}

fn pet_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

async fn linked_to(target_path: &Path, source_path: &Path) -> bool {
    matches!(
        (
            tokio::fs::canonicalize(target_path).await,
            tokio::fs::canonicalize(source_path).await
        ),
        (Ok(target), Ok(source)) if target == source
    )
}

async fn remove_legacy_pet_link(target_path: &Path) -> Result<(), ManagerError> {
    let Ok(stat) = tokio::fs::symlink_metadata(target_path).await else {
        return Ok(());
    };

    if !stat.file_type().is_symlink() {
        return Err(ManagerError::System(format!(
            "Codex 宠物目录不是旧版链接：{}",
            path_text(target_path)
        )));
    }

    match tokio::fs::remove_dir(target_path).await {
        Ok(_) => Ok(()),
        Err(_) => {
            tokio::fs::remove_file(target_path).await?;
            Ok(())
        }
    }
}

async fn move_pet_dir(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    match tokio::fs::rename(source_path, target_path).await {
        Ok(_) => Ok(()),
        Err(_) => {
            copy_pet_dir(source_path, target_path).await?;
            tokio::fs::remove_dir_all(source_path).await?;
            Ok(())
        }
    }
}

async fn copy_pet_dir(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    tokio::fs::create_dir_all(target_path).await?;
    let mut entries = tokio::fs::read_dir(source_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let source_child = entry.path();
        let target_child = target_path.join(entry.file_name());
        let stat = tokio::fs::symlink_metadata(&source_child).await?;

        if stat.file_type().is_symlink() {
            return Err(ManagerError::System(format!(
                "宠物目录包含链接，已拒绝迁移：{}",
                path_text(&source_child)
            )));
        }

        if stat.is_dir() {
            Box::pin(copy_pet_dir(&source_child, &target_child)).await?;
        } else if stat.is_file() {
            tokio::fs::copy(&source_child, &target_child).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::resolve_app_paths;

    #[test]
    fn protects_system_port_processes() {
        assert_eq!(
            protected_process_reason(4, "System"),
            Some("系统核心进程不可关闭")
        );
        assert_eq!(
            protected_process_reason(128, "svchost.exe"),
            Some("系统关键进程不可关闭")
        );
        assert_eq!(protected_process_reason(u32::MAX, "node.exe"), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn lists_windows_ports() {
        tauri::async_runtime::block_on(async {
            let result = list_ports().await.unwrap();
            let ports = result["ports"].as_array().unwrap();

            assert!(!ports.is_empty());
            assert!(ports.iter().all(|port| {
                port.get("protocol").and_then(Value::as_str).is_some()
                    && port.get("localPort").and_then(Value::as_u64).is_some()
                    && port.get("pid").and_then(Value::as_u64).is_some()
                    && port.get("canTerminate").and_then(Value::as_bool).is_some()
            }));
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn terminates_verified_port_process() {
        tauri::async_runtime::block_on(async {
            let mut child_command = Command::new("powershell.exe");
            child_command.creation_flags(CREATE_NO_WINDOW);
            let mut child = child_command
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Start-Sleep -Seconds 30",
                ])
                .spawn()
                .unwrap();
            let pid = child.id().unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            let script = format!(
                "$process = Get-Process -Id {}; $started = [DateTimeOffset]$process.StartTime; $started.ToUnixTimeSeconds()",
                pid
            );
            let mut inspect_command = Command::new("powershell.exe");
            inspect_command.creation_flags(CREATE_NO_WINDOW);
            let output = inspect_command
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &script,
                ])
                .output()
                .await
                .unwrap();
            let started_at = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<i64>()
                .unwrap();

            // 仅终止本测试创建的进程，验证 PID 身份校验和关闭链路。
            let result = terminate_port_process(json!({
              "pid": pid,
              "startedAt": started_at
            }))
            .await;
            if result.is_err() {
                let _ = child.kill().await;
            }

            result.unwrap();
            let status = child.wait().await.unwrap();
            assert!(!status.success());
        });
    }

    #[test]
    fn manages_codex_pet_lifecycle() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "ai-manager-codex-pets-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let paths = resolve_app_paths(&root);
            let config_path = root.join("codex");
            let runtime_pet = config_path.join("pets").join("demo");
            tokio::fs::create_dir_all(&runtime_pet).await.unwrap();
            tokio::fs::write(
                runtime_pet.join("pet.json"),
                r#"{"id":"demo","displayName":"演示宠物","description":"用于测试"}"#,
            )
            .await
            .unwrap();
            tokio::fs::write(runtime_pet.join("spritesheet.webp"), [0_u8, 1, 2])
                .await
                .unwrap();
            let cli_targets = json!([{
              "id": "codex",
              "installed": true,
              "configPath": path_text(&config_path)
            }]);

            let result = list_codex_pets(&paths, &cli_targets).await.unwrap();
            assert_eq!(result["pets"].as_array().unwrap().len(), 1);
            assert!(runtime_pet.exists());
            assert!(!tokio::fs::symlink_metadata(&runtime_pet)
                .await
                .unwrap()
                .file_type()
                .is_symlink());

            rename_codex_pet(
                &paths,
                &cli_targets,
                json!({ "id": "demo", "displayName": "新的名称" }),
            )
            .await
            .unwrap();
            let content = tokio::fs::read_to_string(runtime_pet.join("pet.json"))
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&content).unwrap()["displayName"],
                "新的名称"
            );

            toggle_codex_pet(
                &paths,
                &cli_targets,
                json!({ "id": "demo", "enabled": false }),
            )
            .await
            .unwrap();
            assert!(Path::new(&paths.disabled_pets_dir).join("demo").exists());
            assert!(!config_path.join("pets").join("demo").exists());

            toggle_codex_pet(
                &paths,
                &cli_targets,
                json!({ "id": "demo", "enabled": true }),
            )
            .await
            .unwrap();
            assert!(runtime_pet.exists());
            assert!(!tokio::fs::symlink_metadata(&runtime_pet)
                .await
                .unwrap()
                .file_type()
                .is_symlink());

            delete_codex_pet(&paths, &cli_targets, json!({ "id": "demo" }))
                .await
                .unwrap();
            assert!(!runtime_pet.exists());

            let _ = tokio::fs::remove_dir_all(&root).await;
        });
    }
}

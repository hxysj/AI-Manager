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
use tokio::process::Command;
use tokio::task::JoinHandle;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const INDEX_HTML: &str = include_str!("../../toolbox-panel/index.html");
const STYLE_CSS: &str = include_str!("../../toolbox-panel/styles.css");
const APP_JS: &str = include_str!("../../toolbox-panel/app.js");
const TOOL_REGISTRY_JS: &str = include_str!("../../toolbox-panel/tools/registry.js");
const IMAGE_LINK_EXTRACTOR_JS: &str =
    include_str!("../../toolbox-panel/tools/image-link-extractor.js");
const STRING_DIFF_JS: &str = include_str!("../../toolbox-panel/tools/string-diff.js");

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

    sync_codex_pets(paths, &codex_pets_dir).await?;

    let mut pets = read_managed_pets(Path::new(&paths.pets_dir), true).await?;
    pets.extend(read_managed_pets(Path::new(&paths.disabled_pets_dir), false).await?);
    pets.sort_by(|left, right| {
        pet_string(left, "displayName").cmp(&pet_string(right, "displayName"))
    });

    Ok(json!({
      "codexPetsPath": path_text(&codex_pets_dir),
      "managedPetsPath": paths.pets_dir,
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
    sync_codex_pets(paths, &codex_pets_dir).await?;
    let pet_dir = managed_pet_dir(paths, id)?;
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

    sync_codex_pets(paths, &codex_pets_dir).await?;
    let active_path = Path::new(&paths.pets_dir).join(id);
    let disabled_path = Path::new(&paths.disabled_pets_dir).join(id);

    if payload.enabled {
        if active_path.exists() {
            return Ok(json!({ "id": id, "enabled": true }));
        }
        if !disabled_path.exists() {
            return Err(ManagerError::System(format!("未找到宠物：{}", id)));
        }

        move_pet_dir(&disabled_path, &active_path).await?;
        if let Err(error) = ensure_codex_pet_link(&active_path, &codex_pets_dir.join(id)).await {
            let _ = move_pet_dir(&active_path, &disabled_path).await;
            return Err(error);
        }
    } else {
        if disabled_path.exists() {
            return Ok(json!({ "id": id, "enabled": false }));
        }
        if !active_path.exists() {
            return Err(ManagerError::System(format!("未找到宠物：{}", id)));
        }

        remove_codex_pet_link(&codex_pets_dir.join(id)).await?;
        if let Err(error) = move_pet_dir(&active_path, &disabled_path).await {
            let _ = ensure_codex_pet_link(&active_path, &codex_pets_dir.join(id)).await;
            return Err(error);
        }
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

    sync_codex_pets(paths, &codex_pets_dir).await?;
    let active_path = Path::new(&paths.pets_dir).join(id);
    let disabled_path = Path::new(&paths.disabled_pets_dir).join(id);

    if active_path.exists() {
        remove_codex_pet_link(&codex_pets_dir.join(id)).await?;
        if let Err(error) = tokio::fs::remove_dir_all(&active_path).await {
            let _ = ensure_codex_pet_link(&active_path, &codex_pets_dir.join(id)).await;
            return Err(error.into());
        }
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

async fn sync_codex_pets(paths: &AppPaths, codex_pets_dir: &Path) -> Result<(), ManagerError> {
    let active_dir = Path::new(&paths.pets_dir);
    let disabled_dir = Path::new(&paths.disabled_pets_dir);

    tokio::fs::create_dir_all(active_dir).await?;
    tokio::fs::create_dir_all(disabled_dir).await?;
    tokio::fs::create_dir_all(codex_pets_dir).await?;

    let mut entries = tokio::fs::read_dir(codex_pets_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let runtime_path = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();

        if valid_pet_id(&id).is_err() || !is_pet_directory(&runtime_path).await {
            continue;
        }

        let active_path = active_dir.join(&id);
        let disabled_path = disabled_dir.join(&id);
        let stat = tokio::fs::symlink_metadata(&runtime_path).await?;

        if active_path.exists() {
            if stat.file_type().is_symlink() && linked_to(&runtime_path, &active_path).await {
                continue;
            }

            return Err(ManagerError::System(format!(
                "Codex 宠物目录与受管宠物冲突：{}",
                path_text(&runtime_path)
            )));
        }

        if disabled_path.exists() {
            return Err(ManagerError::System(format!(
                "Codex 宠物目录与已禁用宠物冲突：{}",
                path_text(&runtime_path)
            )));
        }

        if stat.file_type().is_symlink() {
            continue;
        }

        move_pet_dir(&runtime_path, &active_path).await?;
    }

    let mut active_entries = tokio::fs::read_dir(active_dir).await?;
    while let Some(entry) = active_entries.next_entry().await? {
        let source_path = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();

        if valid_pet_id(&id).is_err() || !is_pet_directory(&source_path).await {
            continue;
        }

        ensure_codex_pet_link(&source_path, &codex_pets_dir.join(id)).await?;
    }

    Ok(())
}

async fn read_managed_pets(pets_dir: &Path, enabled: bool) -> Result<Vec<Value>, ManagerError> {
    let mut entries = tokio::fs::read_dir(pets_dir).await?;
    let mut pets = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let pet_dir = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();

        if valid_pet_id(&id).is_err() || !is_pet_directory(&pet_dir).await {
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

async fn is_pet_directory(path: &Path) -> bool {
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

fn managed_pet_dir<'a>(paths: &'a AppPaths, id: &str) -> Result<PathBuf, ManagerError> {
    let active_path = Path::new(&paths.pets_dir).join(id);

    if active_path.exists() {
        return Ok(active_path);
    }

    let disabled_path = Path::new(&paths.disabled_pets_dir).join(id);
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

async fn ensure_codex_pet_link(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    if let Ok(stat) = tokio::fs::symlink_metadata(target_path).await {
        if !stat.file_type().is_symlink() {
            return Err(ManagerError::System(format!(
                "Codex 宠物目录已被真实目录占用：{}",
                path_text(target_path)
            )));
        }

        if linked_to(target_path, source_path).await {
            return Ok(());
        }

        return Err(ManagerError::System(format!(
            "Codex 宠物目录已链接到其他位置：{}",
            path_text(target_path)
        )));
    }

    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    create_junction(source_path, target_path).await
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

async fn remove_codex_pet_link(target_path: &Path) -> Result<(), ManagerError> {
    let Ok(stat) = tokio::fs::symlink_metadata(target_path).await else {
        return Ok(());
    };

    if !stat.file_type().is_symlink() {
        return Err(ManagerError::System(format!(
            "Codex 宠物目录不是可管理的链接：{}",
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

async fn create_junction(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.creation_flags(CREATE_NO_WINDOW);

        let output = command
            .args([
                "/C",
                "mklink",
                "/J",
                &path_text(target_path),
                &path_text(source_path),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let output_message = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let message = if message.is_empty() {
                output_message
            } else {
                message
            };

            return Err(ManagerError::System(if message.is_empty() {
                "创建 Codex 宠物链接失败".to_string()
            } else {
                message
            }));
        }
    }

    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(source_path, target_path)?;
    }

    Ok(())
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
            assert!(Path::new(&paths.pets_dir).join("demo").exists());
            assert!(
                tokio::fs::symlink_metadata(config_path.join("pets").join("demo"))
                    .await
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );

            rename_codex_pet(
                &paths,
                &cli_targets,
                json!({ "id": "demo", "displayName": "新的名称" }),
            )
            .await
            .unwrap();
            let content =
                tokio::fs::read_to_string(Path::new(&paths.pets_dir).join("demo").join("pet.json"))
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
            assert!(Path::new(&paths.pets_dir).join("demo").exists());

            delete_codex_pet(&paths, &cli_targets, json!({ "id": "demo" }))
                .await
                .unwrap();
            assert!(!Path::new(&paths.pets_dir).join("demo").exists());
            assert!(!config_path.join("pets").join("demo").exists());

            let _ = tokio::fs::remove_dir_all(&root).await;
        });
    }
}

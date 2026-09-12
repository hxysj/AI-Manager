use crate::api::{proxy, runtime_provider};
use crate::core::provider_key_usage::KeyRequest;
use crate::core::{error::ManagerError, paths::AppPaths, provider_store, usage_store};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, Limited, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{json, Map, Value};
use std::convert::Infallible;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[path = "claude_desktop_protocol.rs"]
mod protocol;

const PROFILE_ID: &str = "a1796a13-0ec3-4dd0-b3a0-000000157230";
const OFFICIAL_ID: &str = "claude-desktop-official";
const GATEWAY_KEY: &str = "claude-desktop:gateway";
const DEFAULT_PORT: u16 = 15723;
static REQUEST_LOG_LOCK: Mutex<()> = Mutex::const_new(());
const ROUTES: [(&str, &str, &str); 4] = [
    ("claude-sonnet-4-6", "Sonnet", "sonnetModel"),
    ("claude-opus-4-8", "Opus", "opusModel"),
    ("claude-fable-5", "Fable", "fableModel"),
    ("claude-haiku-4-5", "Haiku", "haikuModel"),
];

type GatewayBody = UnsyncBoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
struct DesktopPaths {
    normal: PathBuf,
    threep: PathBuf,
    profile: PathBuf,
    meta: PathBuf,
}

impl DesktopPaths {
    fn resolve(platform: &str, home: &Path, local: Option<&Path>) -> Option<Self> {
        let root = match platform {
            "windows" => local
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join("AppData/Local")),
            "macos" => home.join("Library/Application Support"),
            _ => return None,
        };
        let mut normal = root.join("Claude");
        let mut threep = root.join("Claude-3p");
        if platform == "windows" {
            let mut candidates = std::fs::read_dir(&root)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect::<Vec<_>>();
            candidates.sort();
            for (directory, third_party) in [(&mut normal, false), (&mut threep, true)] {
                if directory.join("claude_desktop_config.json").exists() {
                    continue;
                }
                let matches = candidates
                    .iter()
                    .filter(|path| {
                        let name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();
                        (if third_party {
                            name.starts_with("claude-3p")
                        } else {
                            name.starts_with("claude") && !name.starts_with("claude-3p")
                        }) && path.join("claude_desktop_config.json").is_file()
                    })
                    .collect::<Vec<_>>();
                if matches.len() == 1 {
                    *directory = matches[0].clone();
                }
            }
        }
        Some(Self {
            normal: normal.join("claude_desktop_config.json"),
            threep: threep.join("claude_desktop_config.json"),
            profile: threep
                .join("configLibrary")
                .join(format!("{PROFILE_ID}.json")),
            meta: threep.join("configLibrary/_meta.json"),
        })
    }

    fn current() -> Result<Option<Self>, ManagerError> {
        if !["windows", "macos"].contains(&std::env::consts::OS) {
            return Ok(None);
        }
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .filter(|home| !home.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| error("无法确定用户目录"))?;
        let local = std::env::var_os("LOCALAPPDATA")
            .filter(|local| !local.is_empty())
            .map(PathBuf::from);
        Ok(Self::resolve(std::env::consts::OS, &home, local.as_deref()))
    }
}

pub(super) fn skills_data_root(
    platform: &str,
    home: &Path,
    local: Option<&Path>,
) -> Option<PathBuf> {
    DesktopPaths::resolve(platform, home, local)?
        .threep
        .parent()
        .map(Path::to_path_buf)
}

fn error(message: &str) -> ManagerError {
    ManagerError::System(message.to_string())
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

fn is_safe_model(model: &str) -> bool {
    let model = model.strip_prefix("anthropic/").unwrap_or(model);
    let Some(model) = model.strip_prefix("claude-") else {
        return false;
    };
    ["sonnet-", "opus-", "fable-", "haiku-"]
        .iter()
        .any(|prefix| {
            model.strip_prefix(prefix).is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_alphanumeric())
                    && suffix.chars().all(|character| {
                        character.is_ascii_alphanumeric() || character == '-' || character == '.'
                    })
            })
        })
}

fn model_routes(provider: &Value) -> Result<Map<String, Value>, ManagerError> {
    let routes = provider
        .get("routes")
        .and_then(Value::as_object)
        .ok_or_else(|| error("模型映射必须是对象"))?;
    let mut routes = routes.clone();
    for (route, mapping) in &mut routes {
        if !is_safe_model(route) || !mapping.is_object() {
            return Err(error("模型路由必须使用 Desktop 安全模型名"));
        }
        let model = text(mapping, "model").to_string();
        let supports1m = model.to_lowercase().ends_with("[1m]");
        let model = if supports1m {
            model[..model.len() - 4].trim()
        } else {
            &model
        };
        if model.is_empty() {
            return Err(error("模型映射不能为空"));
        }
        if text(provider, "mode") == "direct" && model != route {
            return Err(error("直连模式不能映射到不同模型，请使用本地路由"));
        }
        mapping["model"] = json!(model);
        mapping["supports1m"] =
            json!(supports1m || mapping["supports1m"].as_bool().unwrap_or(false));
    }
    if text(provider, "mode") == "proxy" {
        let main = ROUTES
            .iter()
            .find_map(|(route, _, _)| routes.get(*route))
            .cloned()
            .or_else(|| routes.values().next().cloned())
            .ok_or_else(|| error("本地路由至少需要一个模型映射"))?;
        for (route, _, _) in ROUTES {
            routes
                .entry(route.to_string())
                .or_insert_with(|| main.clone());
        }
    }
    Ok(routes)
}

fn normalize_provider(input: &Value, api_key: &str) -> Result<Value, ManagerError> {
    if !input.is_object() || text(input, "name").is_empty() {
        return Err(error("供应商名称不能为空"));
    }
    if !["direct", "proxy"].contains(&text(input, "mode")) {
        return Err(error("请选择直连或本地路由模式"));
    }
    if !["", "anthropic", "openai_chat", "openai_responses"].contains(&text(input, "apiFormat"))
        || input
            .get("authBinding")
            .is_some_and(|binding| !binding.is_null())
        || ["codex_oauth", "github_copilot"].contains(&text(input, "providerType"))
    {
        return Err(error("不支持的上游协议或 OAuth 账号绑定"));
    }
    if text(input, "mode") == "direct" && !["", "anthropic"].contains(&text(input, "apiFormat")) {
        return Err(error("OpenAI 协议需要本地路由转换，不能使用直连模式"));
    }
    let base_url = text(input, "baseUrl").trim_end_matches('/');
    let url = url::Url::parse(base_url).map_err(|_| error("请输入有效的上游地址"))?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error(
            "上游地址必须为不带认证、查询参数或片段的 HTTP(S) 地址",
        ));
    }
    if text(input, "mode") == "direct"
        && (url.path().ends_with("/messages") || url.path().ends_with("/count_tokens"))
    {
        return Err(error("直连模式请填写网关根地址，而不是完整 API 端点"));
    }
    if api_key.trim().is_empty() {
        return Err(error("供应商 API Key 不能为空"));
    }
    reqwest::header::HeaderValue::from_str(api_key).map_err(|_| error("API Key 包含无效字符"))?;
    let headers = input.get("headers").cloned().unwrap_or_else(|| json!({}));
    if !headers.is_object()
        || headers
            .as_object()
            .unwrap()
            .values()
            .any(|value| !value.is_string())
    {
        return Err(error("自定义请求头必须是字符串对象"));
    }
    if text(input, "mode") == "direct"
        && (!headers.as_object().unwrap().is_empty() || !text(input, "proxy").is_empty())
    {
        return Err(error(
            "直连模式不支持自定义请求头或网络代理，请使用本地路由",
        ));
    }
    for (name, value) in headers.as_object().unwrap() {
        reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| error("自定义请求头名称无效"))?;
        reqwest::header::HeaderValue::from_str(value.as_str().unwrap())
            .map_err(|_| error("自定义请求头值无效"))?;
    }
    if !text(input, "proxy").is_empty() {
        reqwest::Proxy::all(text(input, "proxy")).map_err(|_| error("网络代理地址无效"))?;
    }
    let routes = model_routes(input)?;
    Ok(json!({
        "id": if text(input, "id").is_empty() { uuid::Uuid::new_v4().to_string() } else { text(input, "id").to_string() },
        "name": text(input, "name"), "mode": text(input, "mode"), "icon": text(input, "icon"),
        "baseUrl": base_url, "apiFormat": if text(input, "apiFormat").is_empty() { "anthropic" } else { text(input, "apiFormat") }, "routes": routes,
        "authField": if text(input, "authField") == "ANTHROPIC_API_KEY" && ["", "anthropic"].contains(&text(input, "apiFormat")) { "ANTHROPIC_API_KEY" } else { "ANTHROPIC_AUTH_TOKEN" },
        "proxy": text(input, "proxy"), "headers": headers, "note": text(input, "note"),
        "website": text(input, "website"), "enabled": input["enabled"] != false
    }))
}

fn build_profile(
    provider: &Value,
    api_key: &str,
    gateway_token: &str,
    port: u16,
) -> Result<Value, ManagerError> {
    let provider = normalize_provider(provider, api_key)?;
    let local = text(&provider, "mode") == "proxy";
    if local && gateway_token.is_empty() {
        return Err(error("本地网关 token 未配置"));
    }
    let upstream =
        url::Url::parse(text(&provider, "baseUrl")).map_err(|_| error("上游地址无效"))?;
    if local
        && upstream.port_or_known_default() == Some(port)
        && [Some("127.0.0.1"), Some("localhost"), Some("[::1]")].contains(&upstream.host_str())
    {
        return Err(error("上游地址不能指向 Desktop 本地网关自身"));
    }
    let mut profile = json!({
        "coworkEgressAllowedHosts": ["*"], "disableDeploymentModeChooser": true,
        "inferenceProvider": "gateway", "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayBaseUrl": if local { gateway_url(port) } else { text(&provider, "baseUrl").to_string() },
        "inferenceGatewayApiKey": if local { gateway_token } else { api_key }
    });
    let routes = model_routes(&provider)?;
    if !routes.is_empty() {
        profile["inferenceModels"] = json!(routes
            .iter()
            .map(|(name, mapping)| {
                let mut model =
                    json!({ "name": name, "supports1m": mapping["supports1m"] == true });
                if !text(mapping, "labelOverride").is_empty() {
                    model["labelOverride"] = json!(text(mapping, "labelOverride"));
                }
                model
            })
            .collect::<Vec<_>>());
    }
    Ok(profile)
}

fn gateway_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/claude-desktop")
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ManagerError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(cause) => Err(cause.into()),
    }
}

fn read_object(bytes: Option<&[u8]>) -> Result<Value, ManagerError> {
    let value = bytes
        .map(serde_json::from_slice)
        .transpose()?
        .unwrap_or_else(|| json!({}));
    if !value.is_object() {
        return Err(error("Desktop 配置不是 JSON 对象，已取消写入以保护原文件"));
    }
    Ok(value)
}

fn redact_config(value: Value) -> Value {
    match value {
        Value::Object(entries) => Value::Object(
            entries
                .into_iter()
                .map(|(name, value)| {
                    let lower = name.to_lowercase();
                    let sensitive = [
                        "key",
                        "token",
                        "secret",
                        "password",
                        "authorization",
                        "cookie",
                        "credential",
                    ]
                    .iter()
                    .any(|part| lower.contains(part));
                    (
                        name,
                        if sensitive && !value.is_null() {
                            json!("***")
                        } else {
                            redact_config(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_config).collect()),
        value => value,
    }
}

fn config_files(desktop: &DesktopPaths) -> Result<Value, ManagerError> {
    [&desktop.normal, &desktop.threep, &desktop.profile, &desktop.meta]
        .into_iter().zip(["普通配置", "3P 配置", "Profile", "配置索引"])
        .map(|(path, label)| {
            let bytes = read_optional(path)?;
            let content = bytes.as_deref().map(|bytes| {
                read_object(Some(bytes)).and_then(|value| {
                    serde_json::to_string_pretty(&redact_config(value)).map_err(ManagerError::from)
                })
            }).transpose()?;
            Ok(json!({"label": label, "path": path.to_string_lossy(), "content": content, "exists": bytes.is_some()}))
        }).collect::<Result<Vec<_>, ManagerError>>().map(|files| json!({"files": files}))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ManagerError> {
    let parent = path.parent().ok_or_else(|| error("配置路径没有父目录"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".ai-manager-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn apply_configuration(
    paths: &DesktopPaths,
    profile: Option<&Value>,
    commit: impl FnOnce() -> Result<(), ManagerError>,
) -> Result<(), ManagerError> {
    let files = [&paths.normal, &paths.threep, &paths.profile, &paths.meta];
    let snapshots = files
        .iter()
        .map(|path| read_optional(path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut normal = read_object(snapshots[0].as_deref())?;
    let mut threep = read_object(snapshots[1].as_deref())?;
    let mut meta = read_object(snapshots[3].as_deref())?;
    normal["deploymentMode"] = json!(if profile.is_some() { "3p" } else { "1p" });
    threep["deploymentMode"] = normal["deploymentMode"].clone();
    if meta
        .get("entries")
        .is_some_and(|entries| !entries.is_array())
    {
        return Err(error("Desktop profile 索引 entries 格式错误，已取消写入"));
    }
    let mut entries = meta["entries"].as_array().cloned().unwrap_or_default();
    entries.retain(|entry| text(entry, "id") != PROFILE_ID);
    if profile.is_some() {
        entries.push(json!({"id": PROFILE_ID, "name": "AI Manager"}));
        meta["appliedId"] = json!(PROFILE_ID);
    } else {
        meta.as_object_mut().unwrap().remove("appliedId");
    }
    meta.as_object_mut().unwrap().remove("hybridPointer");
    meta["entries"] = json!(entries);
    let contents = [Some(&normal), Some(&threep), profile, Some(&meta)];
    let mut changed = Vec::new();
    let result = (|| {
        for (index, content) in contents.iter().enumerate() {
            if let Some(content) = content {
                atomic_write(files[index], &serde_json::to_vec_pretty(content)?)?;
                changed.push(index);
            } else if snapshots[index].is_some() {
                std::fs::remove_file(files[index])?;
                changed.push(index);
            }
        }
        commit()
    })();
    if let Err(cause) = result {
        let mut failures = Vec::new();
        for index in changed.into_iter().rev() {
            let rollback = match &snapshots[index] {
                Some(bytes) => atomic_write(files[index], bytes),
                None => std::fs::remove_file(files[index]).map_err(ManagerError::from),
            };
            if let Err(rollback) = rollback {
                failures.push(format!("{}：{rollback}", files[index].display()));
            }
        }
        return if failures.is_empty() {
            Err(cause)
        } else {
            Err(error(&format!(
                "写入失败：{cause}；部分回滚失败：{}",
                failures.join("；")
            )))
        };
    }
    Ok(())
}

struct DesktopData {
    providers: Vec<Value>,
    settings: Map<String, Value>,
    keys: Map<String, Value>,
}

impl DesktopData {
    fn load(paths: &AppPaths) -> Result<Self, ManagerError> {
        let mut data = Self {
            providers: provider_store::read_desktop_providers(paths)?,
            settings: provider_store::read_desktop_settings(paths)?,
            keys: provider_store::read_keys(paths)?,
        };
        if !data
            .providers
            .iter()
            .any(|provider| text(provider, "id") == OFFICIAL_ID)
        {
            data.providers.insert(
                0,
                json!({"id": OFFICIAL_ID, "name": "Claude 官方", "mode": "official"}),
            );
            data.save(paths)?;
        }
        Ok(data)
    }

    fn save(&self, paths: &AppPaths) -> Result<(), ManagerError> {
        provider_store::write_desktop_bundle(paths, &self.providers, &self.settings, &self.keys)
    }

    fn current_id(&self) -> &str {
        self.settings
            .get("currentProviderId")
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    fn port(&self) -> u16 {
        self.settings
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port > 0)
            .unwrap_or(DEFAULT_PORT)
    }

    fn key(&self, provider_id: &str) -> Result<String, ManagerError> {
        let stored = self.keys.get(&format!("claude-desktop:{provider_id}"));
        let records = runtime_provider::provider_key_records(stored);
        let active = runtime_provider::active_provider_key_id(stored, &records);
        records
            .iter()
            .find(|record| text(record, "id") == active)
            .and_then(|record| record["value"].as_str())
            .map(runtime_provider::decrypt_provider_key)
            .transpose()
            .map(|key| key.unwrap_or_default())
    }

    fn upsert_provider(&mut self, payload: &Value) -> Result<String, ManagerError> {
        let id = if text(payload, "id").is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            text(payload, "id").to_string()
        };
        if id == OFFICIAL_ID {
            return Err(error("官方供应商不可编辑"));
        }
        let mut input = self
            .providers
            .iter()
            .find(|provider| text(provider, "id") == id)
            .cloned()
            .unwrap_or_else(|| json!({}));
        input.as_object_mut().unwrap().extend(
            payload
                .as_object()
                .ok_or_else(|| error("供应商配置必须为对象"))?
                .clone(),
        );
        input["id"] = json!(id);
        let storage_id = format!("claude-desktop:{id}");
        let mut keys = self.keys.clone();
        if let Some(requested) = payload.get("apiKeys") {
            let requested = requested
                .as_array()
                .ok_or_else(|| error("API Key 列表格式无效"))?;
            let existing = runtime_provider::provider_key_records(keys.get(&storage_id));
            let mut ids = std::collections::HashSet::new();
            for record in requested {
                let key_id = text(record, "id");
                if key_id.is_empty() || !ids.insert(key_id) {
                    return Err(error("API Key ID 不能为空或重复"));
                }
                let key = text(record, "apiKey");
                if key.is_empty() && !existing.iter().any(|saved| text(saved, "id") == key_id) {
                    return Err(error("请填写新增的 API Key，或删除空白项"));
                }
                reqwest::header::HeaderValue::from_str(key)
                    .map_err(|_| error("API Key 包含无效字符"))?;
            }
            if requested.is_empty() || !ids.contains(text(payload, "activeApiKeyId")) {
                return Err(error("至少保留一个 API Key，并选择当前生效的 Key"));
            }
            runtime_provider::set_provider_keys(
                &mut keys,
                &storage_id,
                requested,
                text(payload, "activeApiKeyId").to_string(),
            )?;
        } else if !text(payload, "apiKey").is_empty() {
            runtime_provider::set_provider_key(
                &mut keys,
                &storage_id,
                text(payload, "apiKey").to_string(),
            )?;
        }
        let (_, _, key) = runtime_provider::public_provider_keys(keys.get(&storage_id));
        let provider = normalize_provider(&input, &key)?;
        self.keys = keys;
        if let Some(previous) = self
            .providers
            .iter_mut()
            .find(|provider| text(provider, "id") == id)
        {
            *previous = provider;
        } else {
            self.providers.push(provider);
        }
        Ok(id)
    }

    fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<bool, ManagerError> {
        if id == OFFICIAL_ID {
            return Err(error("官方供应商不支持禁用"));
        }
        let provider = self
            .providers
            .iter_mut()
            .find(|provider| text(provider, "id") == id)
            .ok_or_else(|| error("供应商不存在"))?;
        provider["enabled"] = json!(enabled);
        let deactivate = !enabled && self.current_id() == id;
        if deactivate {
            self.settings
                .insert("currentProviderId".to_string(), json!(""));
        }
        Ok(deactivate)
    }

    fn gateway_token(&self) -> Result<String, ManagerError> {
        self.settings
            .get(GATEWAY_KEY)
            .and_then(Value::as_str)
            .map(runtime_provider::decrypt_provider_key)
            .transpose()
            .map(|key| key.unwrap_or_default())
    }

    fn status(&self, paths: Option<&DesktopPaths>, running: bool) -> Value {
        let mut warnings = Vec::new();
        let mut read = |path: &Path| {
            read_optional(path)
                .and_then(|bytes| read_object(bytes.as_deref()))
                .unwrap_or_else(|cause| {
                    warnings.push(format!("无法读取 {}：{cause}", path.display()));
                    json!({})
                })
        };
        let (profile, meta, normal, threep) = paths
            .map(|paths| {
                (
                    read(&paths.profile),
                    read(&paths.meta),
                    read(&paths.normal),
                    read(&paths.threep),
                )
            })
            .unwrap_or_else(|| (json!({}), json!({}), json!({}), json!({})));
        let provider = self
            .providers
            .iter()
            .find(|provider| text(provider, "id") == self.current_id());
        let mode = provider
            .map(|provider| text(provider, "mode"))
            .unwrap_or("");
        let registered = meta["entries"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| text(entry, "id") == PROFILE_ID));
        let configured = paths.is_some_and(|paths| paths.profile.exists()) || registered;
        let stale = profile.get("inferenceModels").is_some_and(|models| {
            models.as_array().is_none_or(|models| {
                models
                    .iter()
                    .any(|model| !is_safe_model(text(model, "name")))
            })
        });
        let missing =
            mode == "proxy" && provider.is_none_or(|provider| model_routes(provider).is_err());
        let token = self.gateway_token().unwrap_or_default();
        let expected = provider.filter(|_| mode != "official").map(|provider| {
            if mode == "proxy" {
                gateway_url(self.port())
            } else {
                text(provider, "baseUrl").to_string()
            }
        });
        if paths.is_none() {
            warnings.push("仅支持 Windows 和 macOS 写入 Claude Desktop 配置".to_string());
        }
        if provider.is_none() && !self.current_id().is_empty() {
            warnings.push("当前供应商记录已丢失，请重新选择供应商".to_string());
        }
        if stale {
            warnings.push("profile 中存在不安全的模型名，请选择供应商并点击启用".to_string());
        }
        if missing {
            warnings.push("本地路由缺少有效的模型映射".to_string());
        }
        if mode == "proxy" && token.is_empty() {
            warnings.push("本地网关 token 缺失，当前供应商未启用，请点击启用".to_string());
        }
        if mode == "proxy" && !running {
            warnings.push("本地网关未运行，Desktop 暂时无法访问上游".to_string());
        }
        if paths.is_some() && mode != "official" && provider.is_some() {
            let desired = provider.and_then(|provider| {
                self.key(self.current_id())
                    .ok()
                    .and_then(|key| build_profile(provider, &key, &token, self.port()).ok())
            });
            if desired.as_ref() != Some(&profile)
                || text(&meta, "appliedId") != PROFILE_ID
                || meta.get("hybridPointer").is_some()
                || !registered
                || normal["deploymentMode"] != "3p"
                || threep["deploymentMode"] != "3p"
            {
                warnings.push("Desktop 配置与所选供应商不一致，当前未启用，请点击启用".to_string());
            }
        }
        let needs_repair = !warnings.is_empty()
            || ((mode == "official" || self.current_id().is_empty())
                && (configured
                    || !text(&meta, "appliedId").is_empty()
                    || meta.get("hybridPointer").is_some()
                    || normal["deploymentMode"] == "3p"
                    || threep["deploymentMode"] == "3p"));
        json!({
            "supported": paths.is_some(), "configured": configured, "mode": mode,
            "appliedId": meta.get("appliedId"), "profilePath": paths.map(|paths| paths.profile.to_string_lossy()),
            "configLibraryPath": paths.and_then(|paths| paths.profile.parent()).map(|path| path.to_string_lossy()),
            "normalConfigPath": paths.map(|paths| paths.normal.to_string_lossy()),
            "threepConfigPath": paths.map(|paths| paths.threep.to_string_lossy()),
            "expectedBaseUrl": expected, "actualBaseUrl": profile.get("inferenceGatewayBaseUrl"),
            "proxyRunning": running, "staleRawModels": stale, "missingRouteMappings": missing,
            "gatewayTokenConfigured": !token.is_empty(), "port": self.port(),
            "needsRepair": needs_repair, "warnings": warnings
        })
    }
}

pub struct DesktopManager {
    server: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl DesktopManager {
    pub fn new() -> Self {
        Self {
            server: Mutex::new(None),
        }
    }

    async fn running(&self) -> bool {
        self.server.lock().await.is_some()
    }

    async fn stop(&self) {
        if let Some(server) = self.server.lock().await.take() {
            server.abort();
        }
    }

    async fn start(&self, paths: &AppPaths, port: u16) -> Result<(), ManagerError> {
        let mut server = self.server.lock().await;
        if server.is_some() {
            return Ok(());
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|cause| error(&format!("无法启动 Desktop 网关（端口 {port}）：{cause}")))?;
        let paths = paths.clone();
        *server = Some(tauri::async_runtime::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                while connections.try_join_next().is_some() {}
                let paths = paths.clone();
                connections.spawn(async move {
                    let service = service_fn(move |request| handle_gateway(request, paths.clone()));
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        }));
        Ok(())
    }

    pub async fn start_enabled(&self, paths: &AppPaths) -> Result<(), ManagerError> {
        let data = DesktopData::load(paths)?;
        if DesktopPaths::current()?.is_some()
            && data.settings.get("gatewayEnabled") == Some(&json!(true))
            && data.providers.iter().any(|provider| {
                text(provider, "id") == data.current_id()
                    && text(provider, "mode") == "proxy"
                    && provider["enabled"] != false
            })
        {
            self.start(paths, data.port()).await?;
        }
        Ok(())
    }

    async fn apply(&self, paths: &AppPaths, data: &mut DesktopData) -> Result<(), ManagerError> {
        let desktop = DesktopPaths::current()?
            .ok_or_else(|| error("当前平台不支持 Claude Desktop 配置写入"))?;
        self.apply_at(paths, data, &desktop).await
    }

    async fn apply_at(
        &self,
        paths: &AppPaths,
        data: &mut DesktopData,
        desktop: &DesktopPaths,
    ) -> Result<(), ManagerError> {
        if data.current_id().is_empty() {
            data.settings
                .insert("gatewayEnabled".to_string(), json!(false));
            apply_configuration(desktop, None, || data.save(paths))?;
            self.stop().await;
            return Ok(());
        }
        let provider = data
            .providers
            .iter()
            .find(|provider| text(provider, "id") == data.current_id())
            .cloned()
            .ok_or_else(|| error("供应商不存在"))?;
        let local = text(&provider, "mode") == "proxy";
        if provider["enabled"] == false {
            return Err(error("供应商已禁用，请先取消禁用"));
        }
        if local && data.gateway_token()?.is_empty() {
            runtime_provider::set_provider_key(
                &mut data.settings,
                GATEWAY_KEY,
                format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                ),
            )?;
        }
        let profile = if data.current_id() == OFFICIAL_ID {
            None
        } else {
            Some(build_profile(
                &provider,
                &data.key(data.current_id())?,
                &data.gateway_token()?,
                data.port(),
            )?)
        };
        let was_running = self.running().await;
        if local {
            self.start(paths, data.port()).await?;
        }
        data.settings
            .insert("gatewayEnabled".to_string(), json!(local));
        let result = apply_configuration(desktop, profile.as_ref(), || data.save(paths));
        if result.is_err() && local && !was_running {
            self.stop().await;
        }
        result?;
        if !local {
            self.stop().await;
        }
        Ok(())
    }

    pub async fn dispatch(
        &self,
        paths: &AppPaths,
        action: &str,
        payload: Value,
    ) -> Result<Value, ManagerError> {
        if action == "import-preview" {
            return preview_import_providers(paths);
        }
        let mut data = DesktopData::load(paths)?;
        let mut written = false;
        let mut imported = 0;
        let mut skipped = Vec::new();
        match action {
            "state" => {}
            "config" => {
                let desktop = DesktopPaths::current()?
                    .ok_or_else(|| error("当前平台不支持 Claude Desktop 配置"))?;
                return config_files(&desktop);
            }
            "save" => {
                let id = data.upsert_provider(&payload)?;
                if data.current_id() == id {
                    if data
                        .providers
                        .iter()
                        .any(|provider| text(provider, "id") == id && provider["enabled"] == false)
                    {
                        data.set_enabled(&id, false)?;
                    }
                    self.apply(paths, &mut data).await?;
                    written = true;
                } else {
                    data.save(paths)?;
                }
            }
            "set-enabled" => {
                let enabled = payload["enabled"]
                    .as_bool()
                    .ok_or_else(|| error("禁用状态必须为布尔值"))?;
                if data.set_enabled(text(&payload, "providerId"), enabled)? {
                    self.apply(paths, &mut data).await?;
                    written = true;
                } else {
                    data.save(paths)?;
                }
            }
            "switch" => {
                let id = text(&payload, "providerId");
                if !data
                    .providers
                    .iter()
                    .any(|provider| text(provider, "id") == id && provider["enabled"] != false)
                {
                    return Err(error("供应商不存在或已禁用"));
                }
                data.settings
                    .insert("currentProviderId".to_string(), json!(id));
                self.apply(paths, &mut data).await?;
                written = true;
            }
            "clear" => {
                data.settings
                    .insert("currentProviderId".to_string(), json!(""));
                self.apply(paths, &mut data).await?;
                written = true;
            }
            "delete" => {
                let id = text(&payload, "providerId");
                if id == OFFICIAL_ID || id == data.current_id() {
                    return Err(error("不能删除官方或当前启用的供应商，请先切换"));
                }
                data.providers.retain(|provider| text(provider, "id") != id);
                data.keys.remove(&format!("claude-desktop:{id}"));
                data.save(paths)?;
            }
            "import" => {
                let selected = payload["providerIds"]
                    .as_array()
                    .filter(|items| !items.is_empty())
                    .ok_or_else(|| error("请先选择需要导入的供应商"))?;
                let selected_ids = selected
                    .iter()
                    .map(|id| {
                        id.as_str()
                            .filter(|id| !id.is_empty())
                            .ok_or_else(|| error("供应商选择格式无效，请重新选择"))
                    })
                    .collect::<Result<std::collections::HashSet<_>, _>>()?;
                let sources = provider_store::read_providers(paths)?;
                if selected_ids.iter().any(|id| {
                    !sources
                        .iter()
                        .any(|source| text(source, "id") == *id && text(source, "cli") == "claude")
                }) {
                    return Err(error(
                        "部分已选供应商已不存在或不属于 Claude Code，请刷新后重新选择",
                    ));
                }
                for source in sources.iter().filter(|provider| {
                    text(provider, "cli") == "claude" && selected_ids.contains(text(provider, "id"))
                }) {
                    if data
                        .providers
                        .iter()
                        .any(|provider| text(provider, "id") == text(source, "id"))
                    {
                        skipped.push(format!(
                            "{}：已导入，保留 Desktop 现有配置",
                            text(source, "name")
                        ));
                        continue;
                    }
                    let key = runtime_provider::get_provider_api_key(paths, text(source, "id"))?;
                    match import_provider(source, &key) {
                        Ok(provider) => {
                            if let Some(keys) = data.keys.get(text(source, "id")).cloned() {
                                data.keys.insert(
                                    format!("claude-desktop:{}", text(&provider, "id")),
                                    keys,
                                );
                            }
                            data.providers.push(provider);
                            imported += 1;
                        }
                        Err(cause) => skipped.push(format!("{}：{cause}", text(source, "name"))),
                    }
                }
                data.save(paths)?;
            }
            "gateway" => {
                let enabled = payload["enabled"].as_bool().unwrap_or(false);
                if enabled {
                    if !data.providers.iter().any(|provider| {
                        text(provider, "id") == data.current_id()
                            && text(provider, "mode") == "proxy"
                            && provider["enabled"] != false
                    }) {
                        return Err(error("请先启用一个本地路由供应商"));
                    }
                    let port = payload
                        .get("port")
                        .and_then(Value::as_u64)
                        .unwrap_or(data.port() as u64);
                    if port == 0 || port > u16::MAX as u64 {
                        return Err(error("端口必须在 1–65535 之间"));
                    }
                    let previous_port = data.port();
                    let was_running = self.running().await;
                    if port != previous_port as u64 {
                        self.stop().await;
                    }
                    data.settings.insert("port".to_string(), json!(port));
                    if let Err(cause) = self.apply(paths, &mut data).await {
                        if was_running && port != previous_port as u64 {
                            self.start(paths, previous_port).await?;
                        }
                        return Err(cause);
                    }
                    written = true;
                } else {
                    data.settings
                        .insert("gatewayEnabled".to_string(), json!(false));
                    data.save(paths)?;
                    self.stop().await;
                }
            }
            _ => return Err(error("未知 Claude Desktop 操作")),
        }
        let desktop = DesktopPaths::current();
        let mut status = data.status(
            desktop.as_ref().ok().and_then(Option::as_ref),
            self.running().await,
        );
        if let Err(cause) = desktop {
            status["warnings"]
                .as_array_mut()
                .unwrap()
                .push(json!(cause.to_string()));
        }
        let providers = data
            .providers
            .iter()
            .map(|provider| {
                let mut provider = provider.clone();
                let storage_id = format!("claude-desktop:{}", text(&provider, "id"));
                let (active_id, keys, _) =
                    runtime_provider::public_provider_keys(data.keys.get(&storage_id));
                provider["hasApiKey"] = json!(!keys.is_empty());
                provider["apiKeys"] = json!(keys);
                provider["activeApiKeyId"] = json!(active_id);
                provider["cli"] = json!("claude-desktop");
                provider
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "providers": providers, "currentProviderId": data.current_id(), "officialProviderId": OFFICIAL_ID,
            "status": status, "configurationWritten": written, "importedCount": imported, "skipped": skipped,
            "defaultRoutes": ROUTES.iter().map(|(id, label, field)| json!({"id": id, "label": label, "field": field})).collect::<Vec<_>>()
        }))
    }
}

fn preview_import_providers(paths: &AppPaths) -> Result<Value, ManagerError> {
    let existing = provider_store::read_desktop_providers(paths)?;
    let keys = provider_store::read_keys(paths)?;
    let items = provider_store::read_providers(paths)?.iter()
        .filter(|source| text(source, "cli") == "claude")
        .map(|source| {
            let mut item = json!({
                "id": source["id"], "name": source["name"], "baseUrl": source["baseUrl"],
                "enabled": source["enabled"] != false,
                "apiKeyCount": runtime_provider::provider_key_records(keys.get(text(source, "id"))).len(),
                "status": "available", "reason": ""
            });
            if existing.iter().any(|provider| text(provider, "id") == text(source, "id")) {
                item["status"] = json!("existing");
                item["reason"] = json!("已导入，保留 Desktop 现有配置");
                return item;
            }
            match runtime_provider::get_provider_api_key(paths, text(source, "id"))
                .and_then(|key| import_provider(source, &key)) {
                Ok(provider) => {
                    item["mode"] = provider["mode"].clone();
                    item["apiFormat"] = provider["apiFormat"].clone();
                }
                Err(cause) => {
                    item["status"] = json!("unavailable");
                    item["reason"] = json!(cause.to_string());
                }
            }
            item
        }).collect::<Vec<_>>();
    Ok(json!({"items": items}))
}

fn import_provider(source: &Value, key: &str) -> Result<Value, ManagerError> {
    let mut provider = source.clone();
    provider["routes"] = json!({});
    let config = &source["runtimeConfig"];
    let models = ROUTES
        .iter()
        .map(|(_, _, field)| {
            let model = text(config, field);
            if model.is_empty() {
                text(config, "mainModel")
            } else {
                model
            }
        })
        .collect::<Vec<_>>();
    let direct = ["", "anthropic"].contains(&text(source, "apiFormat"))
        && models.iter().all(|model| {
            model.is_empty()
                || is_safe_model(
                    model
                        .trim_end_matches("[1M]")
                        .trim_end_matches("[1m]")
                        .trim(),
                )
        })
        && source["headers"].as_object().is_none_or(Map::is_empty)
        && text(source, "proxy").is_empty()
        && !text(source, "baseUrl")
            .trim_end_matches('/')
            .ends_with("/messages");
    provider["mode"] = json!(if direct { "direct" } else { "proxy" });
    for ((route, _, _), raw) in ROUTES.iter().zip(models) {
        if raw.is_empty() {
            continue;
        }
        let supports1m = raw.to_lowercase().ends_with("[1m]");
        let model = if supports1m {
            raw[..raw.len() - 4].trim()
        } else {
            raw
        };
        let route = if direct { model } else { route };
        provider["routes"][route] = json!({"model": model, "labelOverride": if direct { "" } else { model }, "supports1m": supports1m});
    }
    if provider.get("headers").is_none() {
        provider["headers"] = json!({});
    }
    normalize_provider(&provider, key)
}

fn json_response(status: StatusCode, payload: Value) -> Response<GatewayBody> {
    let body = Full::new(Bytes::from(payload.to_string()))
        .map_err(|never| match never {})
        .boxed_unsync();
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(
        "content-type",
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}

fn gateway_error(status: StatusCode, message: &str) -> Response<GatewayBody> {
    json_response(
        status,
        json!({"type": "error", "error": {
            "type": if status == StatusCode::UNAUTHORIZED { "authentication_error" } else { "invalid_request_error" },
            "message": message
        }}),
    )
}

async fn handle_gateway(
    request: Request<Incoming>,
    paths: AppPaths,
) -> Result<Response<GatewayBody>, Infallible> {
    let started = std::time::Instant::now();
    let mut log = json!({"method": request.method().as_str(), "endpoint": request.uri().path(),
        "requestUrl": request.uri().path(), "requestSource": "proxy-managed", "targetType": "provider"});
    let mut key_request = None;
    let response = match process_gateway(request, &paths, &mut log, &mut key_request).await {
        Ok(response) => response,
        Err(_) => {
            if let Some(tracking) = &mut key_request {
                tracking.fail("network");
            }
            gateway_error(
                StatusCode::BAD_GATEWAY,
                "Desktop 上游请求失败，请检查供应商地址、凭据和网络连接",
            )
        }
    };
    log["statusCode"] = json!(response.status().as_u16());
    log["ok"] = json!(response.status().is_success());
    log["latencyMs"] = json!(started.elapsed().as_millis() as u64);
    if !response.status().is_success() {
        log["errorMessage"] = json!(format!(
            "Desktop 网关请求失败：HTTP {}",
            response.status().as_u16()
        ));
    }
    let response = track_gateway_key(response, key_request);
    let response = track_gateway_provider(response, paths.clone(), &log);
    let _guard = REQUEST_LOG_LOCK.lock().await;
    if proxy::append_log(&paths, "claude-desktop", log)
        .await
        .is_err()
    {
        eprintln!("Desktop 请求记录写入失败，请检查数据目录权限");
    }
    Ok(response)
}

fn track_gateway_key(
    response: Response<GatewayBody>,
    key_request: Option<KeyRequest>,
) -> Response<GatewayBody> {
    let Some(mut tracking) = key_request else {
        return response;
    };
    let streaming = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    tracking.response(response.status().as_u16(), streaming);
    let (parts, body) = response.into_parts();
    let stream = futures_util::stream::unfold(
        (body.into_data_stream(), tracking),
        |(mut body, mut tracking)| async move {
            match body.next().await {
                Some(Ok(bytes)) => {
                    tracking.observe(&bytes);
                    Some((Ok(Frame::data(bytes)), (body, tracking)))
                }
                Some(Err(cause)) => {
                    tracking.fail("stream");
                    Some((Err(cause), (body, tracking)))
                }
                None => {
                    tracking.finish();
                    None
                }
            }
        },
    );
    Response::from_parts(parts, StreamBody::new(Box::pin(stream)).boxed_unsync())
}

fn track_gateway_provider(
    response: Response<GatewayBody>,
    paths: AppPaths,
    log: &Value,
) -> Response<GatewayBody> {
    if !response.status().is_success()
        || log["endpoint"] != "/claude-desktop/v1/messages"
        || text(log, "providerId").is_empty()
    {
        return response;
    }
    let streaming = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    let mut record = json!({
        "appType": "claude-desktop", "providerId": log["providerId"],
        "apiKeyId": log["apiKeyId"],
        "apiKeyHash": log["apiKeyHash"],
        "providerName": log["providerName"], "providerType": log["providerType"],
        "model": log["model"], "requestModel": log["requestModel"],
        "requestSource": "proxy-managed", "createdAt": chrono::Utc::now().timestamp_millis()
    });
    let (parts, body) = response.into_parts();
    let mut buffer = Vec::new();
    let mut captured = false;
    let stream = body.into_data_stream().map(move |chunk| {
        if let Ok(bytes) = &chunk {
            if !captured {
                if buffer.len() + bytes.len() > 32 * 1024 * 1024 {
                    captured = true;
                    buffer.clear();
                } else {
                    buffer.extend_from_slice(bytes);
                    let mut message = None;
                    if streaming {
                        while let Some(boundary) = protocol::frame_boundary(&buffer) {
                            let frame = buffer.drain(..boundary).collect::<Vec<_>>();
                            let data = String::from_utf8_lossy(&frame)
                                .lines()
                                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                                .collect::<Vec<_>>()
                                .join("\n");
                            if let Ok(event) = serde_json::from_str::<Value>(&data) {
                                if event["type"] == "message_start" {
                                    message = Some(event["message"].clone());
                                    break;
                                }
                            }
                        }
                    } else {
                        message = serde_json::from_slice::<Value>(&buffer).ok();
                    }
                    if let Some(message) = message {
                        let message_id = text(&message, "id");
                        if message["type"] == "message" && !message_id.is_empty() {
                            record["requestId"] = json!(format!("desktop-response:{message_id}"));
                            if usage_store::write_request_record(&paths, &record).is_err() {
                                eprintln!("Desktop 请求供应商归属写入失败，请检查数据目录");
                            }
                        }
                        captured = true;
                        buffer.clear();
                    }
                }
            }
        }
        chunk.map(Frame::data)
    });
    Response::from_parts(parts, StreamBody::new(stream).boxed_unsync())
}

async fn process_gateway(
    request: Request<Incoming>,
    paths: &AppPaths,
    log: &mut Value,
    key_request: &mut Option<KeyRequest>,
) -> Result<Response<GatewayBody>, ManagerError> {
    let data = DesktopData::load(paths)?;
    let token = data.gateway_token()?;
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .unwrap_or("");
    if token.is_empty()
        || supplied.len() != token.len()
        || supplied
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            != 0
    {
        return Ok(gateway_error(
            StatusCode::UNAUTHORIZED,
            "本地网关 token 无效",
        ));
    }
    let Some(provider) = data.providers.iter().find(|provider| {
        text(provider, "id") == data.current_id()
            && text(provider, "mode") == "proxy"
            && provider["enabled"] != false
    }) else {
        return Ok(gateway_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "当前没有启用的 Desktop 本地路由供应商",
        ));
    };
    log["providerId"] = provider["id"].clone();
    log["providerName"] = provider["name"].clone();
    log["providerType"] = provider["type"].clone();
    let routes = match model_routes(provider) {
        Ok(routes) => routes,
        Err(_) => {
            return Ok(gateway_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "当前供应商缺少有效模型路由",
            ))
        }
    };
    let path = request.uri().path();
    if request.method() == hyper::Method::GET && path == "/claude-desktop/v1/models" {
        let models = routes.iter().map(|(name, mapping)| json!({
            "id": name, "type": "model", "display_name": if text(mapping, "labelOverride").is_empty() { name } else { text(mapping, "labelOverride") },
            "created_at": "1970-01-01T00:00:00Z"
        })).collect::<Vec<_>>();
        return Ok(json_response(
            StatusCode::OK,
            json!({
                "data": models, "has_more": false, "first_id": routes.keys().next(), "last_id": routes.keys().last()
            }),
        ));
    }
    if request.method() != hyper::Method::POST
        || ![
            "/claude-desktop/v1/messages",
            "/claude-desktop/v1/messages/count_tokens",
        ]
        .contains(&path)
    {
        return Ok(gateway_error(
            StatusCode::NOT_FOUND,
            "不支持的 Desktop 网关接口",
        ));
    }
    let endpoint = path.trim_start_matches("/claude-desktop/v1").to_string();
    let (parts, body) = request.into_parts();
    let bytes = match Limited::new(body, 32 * 1024 * 1024).collect().await {
        Ok(body) => body.to_bytes(),
        Err(_) => {
            return Ok(gateway_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "请求体读取失败或超过 32 MiB",
            ))
        }
    };
    let mut payload: Value = match serde_json::from_slice(&bytes) {
        Ok(payload) => payload,
        Err(_) => {
            return Ok(gateway_error(
                StatusCode::BAD_REQUEST,
                "请求体必须是有效 JSON",
            ))
        }
    };
    let Some(mapping) = routes.get(text(&payload, "model")) else {
        return Ok(gateway_error(
            StatusCode::BAD_REQUEST,
            "请求的模型没有配置 Desktop 路由",
        ));
    };
    let desktop_model = text(&payload, "model").to_string();
    log["requestModel"] = json!(desktop_model);
    log["model"] = mapping["model"].clone();
    log["streaming"] = json!(payload["stream"] == true);
    payload["model"] = mapping["model"].clone();
    let storage_id = format!("claude-desktop:{}", data.current_id());
    let stored = data.keys.get(&storage_id);
    let key_id = runtime_provider::active_provider_key_id(
        stored,
        &runtime_provider::provider_key_records(stored),
    );
    let key = data.key(data.current_id())?;
    log["apiKeyId"] = json!(key_id);
    log["apiKeyHash"] = json!(crate::core::provider_key_usage::fingerprint(&key));
    *key_request = Some(KeyRequest::new(paths, &storage_id, &key_id, &key));
    if ["openai_chat", "openai_responses"].contains(&text(provider, "apiFormat")) {
        if endpoint != "/messages" {
            return Ok(gateway_error(
                StatusCode::NOT_IMPLEMENTED,
                "OpenAI 上游不提供 Anthropic 精确 token 计数接口",
            ));
        }
        return protocol::forward(provider, &key, payload, key_request).await;
    }
    let search = parts
        .uri
        .query()
        .map(|query| format!("?{query}"))
        .unwrap_or_default();
    let upstream =
        proxy::build_anthropic_upstream_url(text(provider, "baseUrl"), &endpoint, &search)?;
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in &parts.headers {
        if ![
            "authorization",
            "x-api-key",
            "host",
            "content-length",
            "connection",
            "transfer-encoding",
            "accept-encoding",
            "proxy-authorization",
            "proxy-connection",
            "upgrade",
            "te",
            "trailer",
            "cookie",
        ]
        .contains(&name.as_str())
        {
            headers.insert(name.clone(), value.clone());
        }
    }
    if let Some(custom) = provider["headers"].as_object() {
        for (name, value) in custom {
            if [
                "authorization",
                "x-api-key",
                "host",
                "content-length",
                "connection",
                "transfer-encoding",
                "proxy-authorization",
            ]
            .contains(&name.to_lowercase().as_str())
            {
                continue;
            }
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| error("自定义请求头名称无效"))?;
            let value = reqwest::header::HeaderValue::from_str(value.as_str().unwrap_or(""))
                .map_err(|_| error("自定义请求头值无效"))?;
            headers.insert(name, value);
        }
    }
    if key.is_empty() {
        return Ok(gateway_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "当前供应商 API Key 缺失",
        ));
    }
    let (auth_name, auth_value) = if text(provider, "authField") == "ANTHROPIC_API_KEY" {
        ("x-api-key", key)
    } else {
        ("authorization", format!("Bearer {key}"))
    };
    headers.insert(
        auth_name,
        reqwest::header::HeaderValue::from_str(&auth_value)
            .map_err(|_| error("供应商凭据格式无效"))?,
    );
    headers
        .entry("anthropic-version")
        .or_insert(reqwest::header::HeaderValue::from_static("2023-06-01"));
    headers.insert(
        "accept-encoding",
        reqwest::header::HeaderValue::from_static("identity"),
    );
    let outgoing = proxy::http_client(text(provider, "proxy"))?
        .post(upstream)
        .headers(headers)
        .json(&payload);
    if let Some(tracking) = key_request {
        tracking.start();
    }
    let upstream = outgoing
        .send()
        .await
        .map_err(|cause| error(&cause.to_string()))?;
    let mut response = Response::builder().status(upstream.status());
    for (name, value) in upstream.headers() {
        if ![
            "connection",
            "transfer-encoding",
            "content-length",
            "set-cookie",
            "keep-alive",
            "upgrade",
            "trailer",
        ]
        .contains(&name.as_str())
        {
            response = response.header(name, value);
        }
    }
    let stream = upstream.bytes_stream().map(|chunk| {
        chunk
            .map(Frame::data)
            .map_err(|cause| Box::new(cause) as Box<dyn std::error::Error + Send + Sync>)
    });
    response
        .body(StreamBody::new(stream).boxed_unsync())
        .map_err(|cause| error(&cause.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::resolve_app_paths;

    struct Fixture {
        root: PathBuf,
        paths: AppPaths,
        desktop: DesktopPaths,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("ai-manager-desktop-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let paths = resolve_app_paths(&root.join("data"));
            let desktop =
                DesktopPaths::resolve("windows", &root, Some(&root.join("desktop"))).unwrap();
            Self {
                root,
                paths,
                desktop,
            }
        }

        fn contents(&self) -> Vec<Option<Vec<u8>>> {
            [
                &self.desktop.normal,
                &self.desktop.threep,
                &self.desktop.profile,
                &self.desktop.meta,
            ]
            .iter()
            .map(|path| read_optional(path).unwrap())
            .collect()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let root = self.root.canonicalize().unwrap();
            assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ai-manager-desktop-test-"));
            let _ = std::fs::remove_dir_all(root);
        }
    }

    fn provider(mode: &str) -> Value {
        json!({"id": "example", "name": "测试供应商", "mode": mode, "apiFormat": "anthropic",
            "baseUrl": "https://example.invalid/", "routes": if mode == "proxy" {
                json!({"claude-sonnet-4-6": {"model": "upstream-model[1M]", "labelOverride": "测试模型"}})
            } else { json!({}) }})
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn no_provider_is_enabled_by_default() {
        let fixture = Fixture::new();
        let data = DesktopData::load(&fixture.paths).unwrap();
        assert_eq!(data.current_id(), "");
        assert_eq!(data.providers[0]["id"], OFFICIAL_ID);
        let status = data.status(Some(&fixture.desktop), false);
        assert_eq!(status["mode"], "");
        assert_eq!(status["warnings"], json!([]));
        assert!(fixture.contents().iter().all(Option::is_none));
    }

    #[test]
    fn existing_third_party_configuration_is_silent_but_not_marked_enabled() {
        for source in ["normal", "threep", "profile", "meta", "external", "hybrid"] {
            let fixture = Fixture::new();
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            let (path, content) = match source {
                "normal" => (&fixture.desktop.normal, json!({"deploymentMode": "3p"})),
                "threep" => (&fixture.desktop.threep, json!({"deploymentMode": "3p"})),
                "profile" => (
                    &fixture.desktop.profile,
                    json!({"inferenceGatewayBaseUrl": "https://example.invalid"}),
                ),
                "external" => (
                    &fixture.desktop.meta,
                    json!({"appliedId": "00000000-0000-4000-8000-000000157210"}),
                ),
                "hybrid" => (
                    &fixture.desktop.meta,
                    json!({"hybridPointer": {"bootstrapUrl": "https://example.invalid"}}),
                ),
                _ => (
                    &fixture.desktop.meta,
                    json!({"entries": [{"id": PROFILE_ID}]}),
                ),
            };
            atomic_write(path, &serde_json::to_vec(&content).unwrap()).unwrap();
            let before = fixture.contents();
            for selected in ["", OFFICIAL_ID] {
                data.settings
                    .insert("currentProviderId".to_string(), json!(selected));
                let status = data.status(Some(&fixture.desktop), false);
                assert_eq!(status["warnings"], json!([]), "{source}: {selected}");
                assert_eq!(status["needsRepair"], true, "{source}: {selected}");
            }
            assert_eq!(fixture.contents(), before);
        }
    }

    #[test]
    fn malformed_configuration_still_reports_read_errors() {
        let fixture = Fixture::new();
        let data = DesktopData::load(&fixture.paths).unwrap();
        atomic_write(&fixture.desktop.normal, b"not-json").unwrap();
        let status = data.status(Some(&fixture.desktop), false);
        assert_eq!(status["needsRepair"], true);
        let warnings = status["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].as_str().unwrap().contains("无法读取"));
    }

    #[test]
    fn canceling_usage_clears_selection_for_every_provider_mode() {
        runtime().block_on(async {
            for mode in ["official", "direct", "proxy"] {
                let fixture = Fixture::new();
                let manager = DesktopManager::new();
                let mut data = DesktopData::load(&fixture.paths).unwrap();
                atomic_write(
                    &fixture.desktop.normal,
                    br#"{"mcpServers":{"keep":{"command":"test"}}}"#,
                )
                .unwrap();
                let id = if mode == "official" {
                    OFFICIAL_ID
                } else {
                    let mut input = provider(mode);
                    input["apiKey"] = json!("test-secret");
                    data.upsert_provider(&input).unwrap();
                    "example"
                };
                if mode == "proxy" {
                    let listener =
                        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
                    data.settings.insert(
                        "port".to_string(),
                        json!(listener.local_addr().unwrap().port()),
                    );
                    drop(listener);
                }
                data.settings
                    .insert("currentProviderId".to_string(), json!(id));
                manager
                    .apply_at(&fixture.paths, &mut data, &fixture.desktop)
                    .await
                    .unwrap();
                assert_eq!(manager.running().await, mode == "proxy");
                data.settings
                    .insert("currentProviderId".to_string(), json!(""));
                manager
                    .apply_at(&fixture.paths, &mut data, &fixture.desktop)
                    .await
                    .unwrap();
                let saved = DesktopData::load(&fixture.paths).unwrap();
                assert_eq!(saved.current_id(), "");
                assert_eq!(saved.settings["gatewayEnabled"], false);
                assert!(!manager.running().await);
                assert!(!fixture.desktop.profile.exists());
                assert_eq!(
                    saved.status(Some(&fixture.desktop), false)["warnings"],
                    json!([])
                );
                let normal =
                    read_object(read_optional(&fixture.desktop.normal).unwrap().as_deref())
                        .unwrap();
                assert_eq!(normal["mcpServers"]["keep"]["command"], "test");
                if mode != "official" {
                    assert_eq!(saved.key("example").unwrap(), "test-secret");
                    assert_eq!(saved.providers.len(), 2);
                }
            }
        });
    }

    #[test]
    fn cancel_failure_preserves_active_selection_files_and_gateway() {
        runtime().block_on(async {
            let fixture = Fixture::new();
            let manager = DesktopManager::new();
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            let mut input = provider("proxy");
            input["apiKey"] = json!("test-secret");
            data.upsert_provider(&input).unwrap();
            let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            data.settings.insert("port".to_string(), json!(listener.local_addr().unwrap().port()));
            drop(listener);
            data.settings.insert("currentProviderId".to_string(), json!("example"));
            manager.apply_at(&fixture.paths, &mut data, &fixture.desktop).await.unwrap();
            let before = fixture.contents();
            let connection = crate::core::database::open(&fixture.paths).unwrap();
            connection.execute_batch("CREATE TRIGGER reject_desktop_settings BEFORE INSERT ON claude_desktop_settings BEGIN SELECT RAISE(FAIL, '模拟事务失败'); END;").unwrap();
            drop(connection);
            data.settings.insert("currentProviderId".to_string(), json!(""));
            assert!(manager.apply_at(&fixture.paths, &mut data, &fixture.desktop).await.is_err());
            assert_eq!(DesktopData::load(&fixture.paths).unwrap().current_id(), "example");
            assert_eq!(fixture.contents(), before);
            assert!(manager.running().await);
            manager.stop().await;
        });
    }

    #[test]
    fn platform_paths_and_windows_variants_are_independent() {
        let fixture = Fixture::new();
        let home = &fixture.root;
        assert!(DesktopPaths::resolve("linux", home, None).is_none());
        let mac = DesktopPaths::resolve("macos", home, None).unwrap();
        assert_eq!(
            mac.normal,
            home.join("Library/Application Support/Claude/claude_desktop_config.json")
        );
        assert_eq!(
            mac.meta,
            home.join("Library/Application Support/Claude-3p/configLibrary/_meta.json")
        );
        let windows = DesktopPaths::resolve("windows", home, None).unwrap();
        assert_eq!(
            windows.normal,
            home.join("AppData/Local/Claude/claude_desktop_config.json")
        );
        let variant = home.join("Claude-preview/claude_desktop_config.json");
        atomic_write(&variant, b"{}").unwrap();
        assert_eq!(
            DesktopPaths::resolve("windows", home, Some(home))
                .unwrap()
                .normal,
            variant
        );
    }

    #[test]
    fn multiple_keys_preserve_metadata_selection_and_encrypted_storage() {
        let fixture = Fixture::new();
        let mut data = DesktopData::load(&fixture.paths).unwrap();
        let mut input = provider("direct");
        input["website"] = json!("https://provider.invalid");
        input["apiKeys"] = json!([
            {"id": "primary", "name": "主账号", "note": "生产", "apiKey": "primary-secret"},
            {"id": "backup", "name": "备用账号", "note": "备用额度", "apiKey": "backup-secret"}
        ]);
        input["activeApiKeyId"] = json!("primary");
        data.upsert_provider(&input).unwrap();
        data.save(&fixture.paths).unwrap();
        let mut data = DesktopData::load(&fixture.paths).unwrap();
        assert_eq!(data.key("example").unwrap(), "primary-secret");
        let stored = &data.keys["claude-desktop:example"];
        assert!(!stored.to_string().contains("primary-secret"));
        let (active, public, _) = runtime_provider::public_provider_keys(Some(stored));
        assert_eq!(active, "primary");
        assert_eq!(public[1]["note"], "备用额度");
        assert!(!serde_json::to_string(&public)
            .unwrap()
            .contains("backup-secret"));
        assert!(data
            .providers
            .iter()
            .all(|provider| provider.get("apiKeys").is_none()));
        data.upsert_provider(&json!({
            "id": "example", "activeApiKeyId": "backup",
            "apiKeys": [{"id": "primary", "name": "主账号", "note": "生产"}, {"id": "backup", "name": "备用账号", "note": "备用额度"}]
        })).unwrap();
        assert_eq!(data.key("example").unwrap(), "backup-secret");
        assert_eq!(data.providers[1]["website"], "https://provider.invalid");
        let before = data.keys.clone();
        for invalid in [
            json!({"id": "example", "activeApiKeyId": "", "apiKeys": []}),
            json!({"id": "example", "activeApiKeyId": "missing", "apiKeys": [{"id": "missing"}]}),
            json!({"id": "example", "activeApiKeyId": "backup", "apiKeys": [{"id": "backup"}, {"id": "backup"}]}),
            json!({"id": "example", "activeApiKeyId": "backup", "apiKeys": [{"id": "backup", "apiKey": "bad\nkey"}]}),
        ] {
            assert!(data.upsert_provider(&invalid).is_err());
            assert_eq!(data.keys, before);
        }
        data.upsert_provider(&json!({"id": "example", "apiKeys": [{"id": "backup", "name": "保留"}], "activeApiKeyId": "backup"})).unwrap();
        assert_eq!(
            runtime_provider::provider_key_records(data.keys.get("claude-desktop:example")).len(),
            1
        );
        assert_eq!(data.key("example").unwrap(), "backup-secret");
        assert!(fixture.contents().iter().all(Option::is_none));
    }

    #[test]
    fn legacy_key_edits_keep_secret_and_migrate_to_named_keys() {
        let fixture = Fixture::new();
        let mut data = DesktopData::load(&fixture.paths).unwrap();
        let mut input = provider("direct");
        input["apiKey"] = json!("legacy-secret");
        data.upsert_provider(&input).unwrap();
        let encrypted =
            runtime_provider::provider_key_records(data.keys.get("claude-desktop:example"))[0]
                ["value"]
                .clone();
        data.keys
            .insert("claude-desktop:example".to_string(), encrypted);
        data.upsert_provider(&json!({"id": "example", "name": "重命名"}))
            .unwrap();
        assert_eq!(data.key("example").unwrap(), "legacy-secret");
        data.upsert_provider(&json!({"id": "example", "activeApiKeyId": "default", "apiKeys": [{"id": "default", "name": "迁移 Key", "note": "原密钥"}]})).unwrap();
        assert_eq!(data.key("example").unwrap(), "legacy-secret");
        assert_eq!(
            data.keys["claude-desktop:example"]["keys"][0]["name"],
            "迁移 Key"
        );
    }

    #[test]
    fn disabled_current_provider_clears_selection_and_can_be_reenabled() {
        runtime().block_on(async {
            let fixture = Fixture::new();
            let manager = DesktopManager::new();
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            let mut input = provider("direct");
            input["apiKey"] = json!("first-secret");
            data.upsert_provider(&input).unwrap();
            data.settings.insert("currentProviderId".to_string(), json!("example"));
            manager.apply_at(&fixture.paths, &mut data, &fixture.desktop).await.unwrap();
            data.upsert_provider(&json!({"id": "example", "apiKeys": [{"id": "second", "apiKey": "second-secret"}], "activeApiKeyId": "second"})).unwrap();
            manager.apply_at(&fixture.paths, &mut data, &fixture.desktop).await.unwrap();
            let config = read_object(read_optional(&fixture.desktop.profile).unwrap().as_deref()).unwrap();
            assert_eq!(config["inferenceGatewayApiKey"], "second-secret");
            assert!(data.set_enabled("example", false).unwrap());
            manager.apply_at(&fixture.paths, &mut data, &fixture.desktop).await.unwrap();
            let saved = DesktopData::load(&fixture.paths).unwrap();
            assert_eq!(saved.current_id(), "");
            assert_eq!(saved.providers[1]["enabled"], false);
            assert!(!fixture.desktop.profile.exists());
            assert!(manager.dispatch(&fixture.paths, "switch", json!({"providerId": "example"})).await.is_err());
            assert!(!data.set_enabled("example", true).unwrap());
            assert_eq!(data.current_id(), "");
            assert!(data.set_enabled(OFFICIAL_ID, false).is_err());
            assert!(data.set_enabled("missing", false).is_err());
            assert!(provider_store::read_providers(&fixture.paths).unwrap().is_empty());
        });
    }

    #[test]
    fn import_preview_is_read_only_and_marks_existing_and_invalid_candidates() {
        tauri::async_runtime::block_on(async {
            let fixture = Fixture::new();
            let mut keys = Map::new();
            for id in ["first", "second", "existing", "codex"] {
                runtime_provider::set_provider_key(&mut keys, id, format!("secret-{id}")).unwrap();
            }
            let sources = [
                json!({"id": "first", "name": "供应商一", "cli": "claude", "baseUrl": "https://example.invalid", "headers": {"Authorization": "secret-header"}, "runtimeConfig": {"mainModel": "claude-sonnet-4"}}),
                json!({"id": "second", "name": "供应商二", "cli": "claude", "baseUrl": "https://example.invalid", "enabled": false}),
                json!({"id": "existing", "name": "原供应商", "cli": "claude", "baseUrl": "https://example.invalid"}),
                json!({"id": "invalid", "name": "缺少密钥", "cli": "claude", "baseUrl": "https://example.invalid"}),
                json!({"id": "codex", "name": "不属于导入来源", "cli": "codex", "baseUrl": "https://example.invalid"}),
            ];
            provider_store::write_provider_bundle(&fixture.paths, &sources, &[], &[], &keys)
                .unwrap();
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            data.upsert_provider(&json!({"id": "existing", "name": "Desktop 已编辑", "mode": "direct", "baseUrl": "https://desktop.invalid", "apiKey": "desktop-secret", "routes": {}})).unwrap();
            data.save(&fixture.paths).unwrap();
            let before_providers = provider_store::read_desktop_providers(&fixture.paths).unwrap();
            let before_keys = provider_store::read_keys(&fixture.paths).unwrap();
            let before_settings = provider_store::read_desktop_settings(&fixture.paths).unwrap();
            let manager = DesktopManager::new();
            let preview = manager
                .dispatch(&fixture.paths, "import-preview", json!({}))
                .await
                .unwrap();
            let items = preview["items"].as_array().unwrap();
            assert_eq!(items.len(), 4);
            assert_eq!(items[0]["status"], "available");
            assert_eq!(items[0]["mode"], "proxy");
            assert_eq!(items[0]["apiKeyCount"], 1);
            assert_eq!(items[1]["status"], "available");
            assert_eq!(items[1]["enabled"], false);
            assert_eq!(items[2]["status"], "existing");
            assert_eq!(items[3]["status"], "unavailable");
            assert!(!preview.to_string().contains("secret"));
            assert_eq!(
                provider_store::read_desktop_providers(&fixture.paths).unwrap(),
                before_providers
            );
            assert_eq!(
                provider_store::read_keys(&fixture.paths).unwrap(),
                before_keys
            );
            assert_eq!(
                provider_store::read_desktop_settings(&fixture.paths).unwrap(),
                before_settings
            );
            let result = manager
                .dispatch(
                    &fixture.paths,
                    "import",
                    json!({"providerIds": ["second", "second", "existing"]}),
                )
                .await
                .unwrap();
            assert_eq!(result["importedCount"], 1);
            assert_eq!(result["skipped"].as_array().unwrap().len(), 1);
            let after = DesktopData::load(&fixture.paths).unwrap();
            assert_eq!(after.providers.len(), 3);
            assert!(after
                .providers
                .iter()
                .all(|provider| text(provider, "id") != "first"));
            assert_eq!(
                after
                    .providers
                    .iter()
                    .find(|provider| text(provider, "id") == "existing")
                    .unwrap()["name"],
                "Desktop 已编辑"
            );
            assert!(after.keys.contains_key("claude-desktop:second"));
            assert!(!after.keys.contains_key("claude-desktop:first"));
            assert_eq!(after.current_id(), "");
            assert_eq!(
                provider_store::read_providers(&fixture.paths).unwrap(),
                sources
            );
            assert!(fixture.contents().iter().all(Option::is_none));
        });
    }

    #[test]
    fn import_rejects_missing_empty_invalid_and_stale_selections() {
        tauri::async_runtime::block_on(async {
            let fixture = Fixture::new();
            let mut keys = Map::new();
            runtime_provider::set_provider_key(&mut keys, "source", "test-secret".into()).unwrap();
            provider_store::write_provider_bundle(&fixture.paths, &[
                json!({"id": "source", "cli": "claude", "name": "供应商", "baseUrl": "https://example.invalid"}),
                json!({"id": "codex", "cli": "codex", "name": "其他应用"})
            ], &[], &[], &keys).unwrap();
            let before = DesktopData::load(&fixture.paths).unwrap();
            let manager = DesktopManager::new();
            for payload in [
                json!({}),
                json!({"providerIds": []}),
                json!({"providerIds": "source"}),
                json!({"providerIds": [null]}),
                json!({"providerIds": [""]}),
                json!({"providerIds": ["source", "missing"]}),
                json!({"providerIds": ["source", "codex"]}),
            ] {
                assert!(manager
                    .dispatch(&fixture.paths, "import", payload)
                    .await
                    .is_err());
                assert_eq!(
                    provider_store::read_desktop_providers(&fixture.paths).unwrap(),
                    before.providers
                );
                assert_eq!(
                    provider_store::read_keys(&fixture.paths).unwrap(),
                    before.keys
                );
            }
            assert!(fixture.contents().iter().all(Option::is_none));
        });
    }

    #[test]
    fn import_preserves_all_keys_and_disabled_state_without_linking_cli_edits() {
        runtime().block_on(async {
            let fixture = Fixture::new();
            let mut keys = Map::new();
            runtime_provider::set_provider_keys(&mut keys, "cli-source", &[
                json!({"id": "primary", "name": "主账号", "note": "生产", "apiKey": "cli-primary"}),
                json!({"id": "backup", "name": "备用", "note": "备用额度", "apiKey": "cli-backup"})
            ], "backup".to_string()).unwrap();
            provider_store::write_keys(&fixture.paths, &keys).unwrap();
            provider_store::write_provider_bundle(&fixture.paths, &[json!({"id": "cli-source", "cli": "claude", "name": "导入供应商", "baseUrl": "https://example.invalid", "website": "https://website.invalid", "enabled": false})], &[], &[], &keys).unwrap();
            let manager = DesktopManager::new();
            let result = manager.dispatch(&fixture.paths, "import", json!({"providerIds": ["cli-source"]})).await.unwrap();
            assert_eq!(result["importedCount"], 1);
            assert_eq!(result["providers"][1]["apiKeys"].as_array().unwrap().len(), 2);
            assert_eq!(result["providers"][1]["activeApiKeyId"], "backup");
            assert_eq!(result["providers"][1]["enabled"], false);
            assert!(!result.to_string().contains("cli-backup"));
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            assert_eq!(data.keys["claude-desktop:cli-source"], data.keys["cli-source"]);
            data.upsert_provider(&json!({"id": "cli-source", "apiKeys": [{"id": "backup", "apiKey": "desktop-new-secret"}], "activeApiKeyId": "backup"})).unwrap();
            data.save(&fixture.paths).unwrap();
            assert_eq!(runtime_provider::get_provider_api_key(&fixture.paths, "cli-source").unwrap(), "cli-backup");
            assert_eq!(data.key("cli-source").unwrap(), "desktop-new-secret");
            assert!(fixture.contents().iter().all(Option::is_none));
        });
    }

    #[test]
    fn config_view_reads_real_files_redacts_credentials_and_never_writes() {
        let fixture = Fixture::new();
        let missing = config_files(&fixture.desktop).unwrap();
        assert!(missing["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["exists"] == false && file["content"].is_null()));
        atomic_write(&fixture.desktop.normal, br#"{"deploymentMode":"3p","mcpServers":{"test":{"env":{"API_KEY":"secret","NORMAL":"preserved"}}}}"#).unwrap();
        atomic_write(&fixture.desktop.profile, br#"{"inferenceGatewayApiKey":"gateway-secret","headers":{"Authorization":"Bearer hidden"},"inferenceGatewayBaseUrl":"https://example.invalid"}"#).unwrap();
        let before = fixture.contents();
        let result = config_files(&fixture.desktop).unwrap();
        assert_eq!(result["files"].as_array().unwrap().len(), 4);
        let content = result.to_string();
        assert!(!content.contains("secret"));
        assert!(!content.contains("Bearer hidden"));
        assert!(content.contains("preserved"));
        assert!(content.contains("https://example.invalid"));
        assert_eq!(fixture.contents(), before);
    }

    #[test]
    fn validates_models_and_direct_provider_constraints() {
        for model in [
            "claude-sonnet-4-6",
            "anthropic/claude-opus-4-8",
            "claude-fable-5",
        ] {
            assert!(is_safe_model(model));
        }
        for model in [
            "",
            "claude-sonnet-",
            "claude-sonnet--",
            "claude-3-5-sonnet-latest",
            "gpt-4",
            "claude-haiku-4-5[1m]",
        ] {
            assert!(!is_safe_model(model));
        }
        let direct = provider("direct");
        assert!(normalize_provider(&direct, "").is_err());
        for (field, value) in [
            ("baseUrl", json!("")),
            ("baseUrl", json!("file:///tmp/test")),
            ("baseUrl", json!("https://example.invalid/v1/messages")),
            ("apiFormat", json!("openai_responses")),
            ("authBinding", json!({"accountId": "test"})),
            ("headers", json!({"x-test": "yes"})),
            (
                "routes",
                json!({"claude-sonnet-4-6": {"model": "different"}}),
            ),
        ] {
            let mut invalid = direct.clone();
            invalid[field] = value;
            assert!(normalize_provider(&invalid, "secret").is_err(), "{field}");
        }
        let profile = build_profile(&direct, "secret", "", DEFAULT_PORT).unwrap();
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            "https://example.invalid"
        );
        assert_eq!(profile["inferenceGatewayApiKey"], "secret");
        assert!(profile.get("inferenceModels").is_none());
    }

    #[test]
    fn proxy_routes_inherit_and_never_expose_raw_models_in_profile() {
        let source = provider("proxy");
        let routes = model_routes(&source).unwrap();
        assert_eq!(routes.len(), 4);
        assert!(routes
            .values()
            .all(|route| route["model"] == "upstream-model" && route["supports1m"] == true));
        let profile =
            build_profile(&source, "upstream-secret", "local-secret", DEFAULT_PORT).unwrap();
        assert_eq!(profile["inferenceGatewayApiKey"], "local-secret");
        assert!(!profile.to_string().contains("upstream-secret"));
        assert!(!profile.to_string().contains("upstream-model"));
        assert!(profile["inferenceModels"]
            .as_array()
            .unwrap()
            .iter()
            .all(|model| is_safe_model(text(model, "name"))));
        let mut empty = source.clone();
        empty["routes"] = json!({});
        assert!(model_routes(&empty).is_err());
        empty["routes"] = source["routes"].clone();
        empty["baseUrl"] = json!(gateway_url(DEFAULT_PORT));
        assert!(build_profile(&empty, "secret", "local", DEFAULT_PORT).is_err());
    }

    #[test]
    fn official_and_cancel_release_external_selection_without_deleting_profiles() {
        runtime().block_on(async {
            for selected in [OFFICIAL_ID, ""] {
                for hybrid in [false, true] {
                    let fixture = Fixture::new();
                    let manager = DesktopManager::new();
                    let mut data = DesktopData::load(&fixture.paths).unwrap();
                    data.settings
                        .insert("currentProviderId".to_string(), json!(selected));
                    let external_id = "00000000-0000-4000-8000-000000157210";
                    let external_path = fixture
                        .desktop
                        .profile
                        .parent()
                        .unwrap()
                        .join(format!("{external_id}.json"));
                    let external = build_profile(&provider("direct"), "secret", "", DEFAULT_PORT)
                        .unwrap();
                    let external_bytes = serde_json::to_vec(&external).unwrap();
                    atomic_write(&external_path, &external_bytes).unwrap();
                    let mut meta = json!({
                        "appliedId": external_id,
                        "entries": [{"id": external_id, "name": "CC Switch"}],
                        "custom": true
                    });
                    if hybrid {
                        meta["hybridPointer"] =
                            json!({"bootstrapUrl": "https://bootstrap.invalid"});
                    }
                    atomic_write(&fixture.desktop.meta, &serde_json::to_vec(&meta).unwrap())
                        .unwrap();
                    for path in [&fixture.desktop.normal, &fixture.desktop.threep] {
                        atomic_write(
                            path,
                            br#"{"deploymentMode":"1p","mcpServers":{"keep":{}},"preferences":{"keep":true}}"#,
                        )
                        .unwrap();
                    }
                    let before = fixture.contents();
                    assert!(apply_configuration(&fixture.desktop, None, || {
                        Err(error("模拟数据库失败"))
                    })
                    .is_err());
                    assert_eq!(fixture.contents(), before);
                    manager
                        .apply_at(&fixture.paths, &mut data, &fixture.desktop)
                        .await
                        .unwrap();
                    let applied_meta =
                        read_object(read_optional(&fixture.desktop.meta).unwrap().as_deref())
                            .unwrap();
                    meta.as_object_mut().unwrap().remove("appliedId");
                    meta.as_object_mut().unwrap().remove("hybridPointer");
                    assert_eq!(applied_meta, meta);
                    assert_eq!(std::fs::read(&external_path).unwrap(), external_bytes);
                    assert!(!fixture.desktop.profile.exists());
                    for path in [&fixture.desktop.normal, &fixture.desktop.threep] {
                        let config = read_object(read_optional(path).unwrap().as_deref()).unwrap();
                        assert_eq!(config["deploymentMode"], "1p");
                        assert_eq!(config["mcpServers"], json!({"keep": {}}));
                        assert_eq!(config["preferences"], json!({"keep": true}));
                    }
                    let restored = DesktopData::load(&fixture.paths).unwrap();
                    assert_eq!(restored.current_id(), selected);
                    assert_eq!(restored.settings["gatewayEnabled"], false);
                    assert_eq!(restored.status(Some(&fixture.desktop), false)["needsRepair"], false);
                    assert!(!manager.running().await);
                }
            }
        });
    }

    #[test]
    fn switch_and_official_restore_preserve_unmanaged_configuration() {
        let fixture = Fixture::new();
        atomic_write(
            &fixture.desktop.normal,
            br#"{"mcpServers":{"keep":{}},"deploymentMode":"1p"}"#,
        )
        .unwrap();
        atomic_write(
            &fixture.desktop.meta,
            br#"{"appliedId":"another","entries":[{"id":"another","name":"Other"}],"custom":true,"hybridPointer":{"bootstrapUrl":"https://example.invalid"}}"#,
        )
        .unwrap();
        let profile = build_profile(&provider("direct"), "secret", "", DEFAULT_PORT).unwrap();
        for _ in 0..2 {
            apply_configuration(&fixture.desktop, Some(&profile), || Ok(())).unwrap();
        }
        let normal =
            read_object(read_optional(&fixture.desktop.normal).unwrap().as_deref()).unwrap();
        assert_eq!(normal["deploymentMode"], "3p");
        assert_eq!(normal["mcpServers"]["keep"], json!({}));
        let meta = read_object(read_optional(&fixture.desktop.meta).unwrap().as_deref()).unwrap();
        assert_eq!(meta["entries"].as_array().unwrap().len(), 2);
        assert_eq!(meta["appliedId"], PROFILE_ID);
        assert!(meta.get("hybridPointer").is_none());
        assert_eq!(
            read_object(read_optional(&fixture.desktop.profile).unwrap().as_deref()).unwrap(),
            profile
        );
        apply_configuration(&fixture.desktop, None, || Ok(())).unwrap();
        assert!(!fixture.desktop.profile.exists());
        let meta = read_object(read_optional(&fixture.desktop.meta).unwrap().as_deref()).unwrap();
        assert_eq!(meta["entries"], json!([{"id": "another", "name": "Other"}]));
        assert_eq!(meta["custom"], true);
        assert!(meta.get("appliedId").is_none());
        for path in [&fixture.desktop.normal, &fixture.desktop.threep] {
            assert_eq!(
                read_object(read_optional(path).unwrap().as_deref()).unwrap()["deploymentMode"],
                "1p"
            );
        }
    }

    #[test]
    fn database_failure_rolls_back_all_files_including_deleted_profile() {
        let fixture = Fixture::new();
        let profile = build_profile(&provider("direct"), "secret", "", DEFAULT_PORT).unwrap();
        let before = fixture.contents();
        assert!(
            apply_configuration(&fixture.desktop, Some(&profile), || Err(error(
                "模拟数据库失败"
            )))
            .is_err()
        );
        assert_eq!(fixture.contents(), before);
        apply_configuration(&fixture.desktop, Some(&profile), || Ok(())).unwrap();
        let before = fixture.contents();
        assert!(
            apply_configuration(&fixture.desktop, None, || Err(error("模拟数据库失败"))).is_err()
        );
        assert_eq!(fixture.contents(), before);
    }

    #[test]
    fn malformed_config_and_blocked_profile_parent_do_not_modify_files() {
        let fixture = Fixture::new();
        atomic_write(&fixture.desktop.normal, b"[]").unwrap();
        let before = fixture.contents();
        assert!(apply_configuration(&fixture.desktop, Some(&json!({})), || Ok(())).is_err());
        assert_eq!(fixture.contents(), before);
        atomic_write(&fixture.desktop.normal, b"{}").unwrap();
        atomic_write(fixture.desktop.profile.parent().unwrap(), b"blocked").unwrap();
        assert!(apply_configuration(&fixture.desktop, Some(&json!({})), || Ok(())).is_err());
        assert_eq!(std::fs::read(&fixture.desktop.normal).unwrap(), b"{}");
        assert!(!fixture.desktop.threep.exists());
    }

    #[cfg(windows)]
    #[test]
    fn readonly_meta_rolls_back_previously_written_configs_and_profile() {
        let fixture = Fixture::new();
        atomic_write(&fixture.desktop.normal, b"{\"keep\":true}").unwrap();
        atomic_write(&fixture.desktop.meta, b"{}").unwrap();
        let before = fixture.contents();
        let mut permissions = std::fs::metadata(&fixture.desktop.meta)
            .unwrap()
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&fixture.desktop.meta, permissions.clone()).unwrap();
        let result = apply_configuration(&fixture.desktop, Some(&json!({"test": true})), || Ok(()));
        permissions.set_readonly(false);
        std::fs::set_permissions(&fixture.desktop.meta, permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(fixture.contents(), before);
    }

    #[test]
    fn import_uses_existing_runtime_fields_and_preserves_model_capabilities() {
        let mut source = json!({"id": "cli-provider", "name": "CLI", "baseUrl": "https://example.invalid", "runtimeConfig": {"mainModel": "claude-sonnet-4-6[1M]"}});
        let direct = import_provider(&source, "secret").unwrap();
        assert_eq!(direct["mode"], "direct");
        assert_eq!(direct["routes"]["claude-sonnet-4-6"]["supports1m"], true);
        source["runtimeConfig"]["mainModel"] = json!("custom-model[1m]");
        let proxy = import_provider(&source, "secret").unwrap();
        assert_eq!(proxy["mode"], "proxy");
        assert_eq!(proxy["routes"]["claude-fable-5"]["model"], "custom-model");
        assert_eq!(
            proxy["routes"]["claude-fable-5"]["labelOverride"],
            "custom-model"
        );
        source["apiFormat"] = json!("openai_chat");
        let imported = import_provider(&source, "secret").unwrap();
        assert_eq!(imported["mode"], "proxy");
        assert_eq!(imported["apiFormat"], "openai_chat");
    }

    #[test]
    fn active_provider_updates_rewrite_profile_and_persist_without_touching_cli() {
        runtime().block_on(async {
            let fixture = Fixture::new();
            let manager = DesktopManager::new();
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            assert_eq!(data.providers.len(), 1);
            data.providers
                .push(normalize_provider(&provider("direct"), "secret").unwrap());
            runtime_provider::set_provider_key(
                &mut data.keys,
                "claude-desktop:example",
                "secret".to_string(),
            )
            .unwrap();
            data.settings
                .insert("currentProviderId".to_string(), json!("example"));
            manager
                .apply_at(&fixture.paths, &mut data, &fixture.desktop)
                .await
                .unwrap();
            data.providers[1]["baseUrl"] = json!("https://changed.invalid");
            manager
                .apply_at(&fixture.paths, &mut data, &fixture.desktop)
                .await
                .unwrap();
            assert_eq!(
                read_object(read_optional(&fixture.desktop.profile).unwrap().as_deref()).unwrap()
                    ["inferenceGatewayBaseUrl"],
                "https://changed.invalid"
            );
            let restored = DesktopData::load(&fixture.paths).unwrap();
            assert_eq!(restored.current_id(), "example");
            assert_eq!(restored.key("example").unwrap(), "secret");
            assert!(provider_store::read_providers(&fixture.paths)
                .unwrap()
                .is_empty());
            assert!(provider_store::read_profiles(&fixture.paths)
                .unwrap()
                .is_empty());
            assert!(
                !restored.status(Some(&fixture.desktop), false)["needsRepair"]
                    .as_bool()
                    .unwrap()
            );
            assert!(manager
                .dispatch(&fixture.paths, "delete", json!({"providerId": "example"}))
                .await
                .is_err());
            atomic_write(&fixture.desktop.profile, b"null").unwrap();
            assert!(
                restored.status(Some(&fixture.desktop), false)["needsRepair"]
                    .as_bool()
                    .unwrap()
            );
        });
    }

    #[test]
    fn database_transaction_failure_restores_external_files_and_stops_new_gateway() {
        runtime().block_on(async {
            let fixture = Fixture::new();
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            let before = fixture.contents();
            let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            data.providers.push(normalize_provider(&provider("proxy"), "secret").unwrap());
            runtime_provider::set_provider_key(&mut data.keys, "claude-desktop:example", "secret".to_string()).unwrap();
            data.settings.insert("port".to_string(), json!(port));
            data.settings.insert("currentProviderId".to_string(), json!("example"));
            let connection = crate::core::database::open(&fixture.paths).unwrap();
            connection.execute_batch("CREATE TRIGGER reject_desktop_settings BEFORE INSERT ON claude_desktop_settings BEGIN SELECT RAISE(FAIL, '模拟事务失败'); END;").unwrap();
            drop(connection);
            let manager = DesktopManager::new();
            assert!(manager.apply_at(&fixture.paths, &mut data, &fixture.desktop).await.is_err());
            assert!(!manager.running().await);
            assert_eq!(fixture.contents(), before);
            let restored = DesktopData::load(&fixture.paths).unwrap();
            assert_eq!(restored.current_id(), "");
            assert_eq!(restored.providers.len(), 1);
            assert!(restored.key("example").unwrap().is_empty());
        });
    }

    #[test]
    fn desktop_response_attribution_preserves_json_sse_and_excludes_secrets() {
        runtime().block_on(async {
            for streaming in [false, true] {
                let fixture = Fixture::new();
                let message = json!({"type": "message", "id": "msg-attribution", "model": "upstream-model", "content": [{"type": "text", "text": "private-response-body"}]});
                let body = if streaming {
                    format!("event: ping\r\ndata: {{\"type\":\"ping\"}}\r\n\r\nevent: message_start\r\ndata: {}\r\n\r\nevent: message_stop\r\ndata: {{\"type\":\"message_stop\"}}\r\n\r\n", json!({"type": "message_start", "message": message}))
                } else {
                    message.to_string()
                };
                let chunks = body.as_bytes().chunks(3).map(Bytes::copy_from_slice).collect::<Vec<_>>();
                let stream = futures_util::stream::iter(chunks.into_iter().map(|bytes| {
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(Frame::data(bytes))
                }));
                let response = Response::builder()
                    .header("content-type", if streaming { "text/event-stream" } else { "application/json" })
                    .body(StreamBody::new(stream).boxed_unsync()).unwrap();
                let mut log = json!({"endpoint": "/claude-desktop/v1/messages", "providerId": "desktop-provider", "providerName": "测试供应商", "model": "upstream-model", "requestModel": "claude-sonnet-4-6", "apiKey": "private-key", "requestBody": "private-request-body"});
                let response = track_gateway_provider(response, fixture.paths.clone(), &log);
                log["providerId"] = json!("new-provider");
                let output = response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(output.as_ref(), body.as_bytes());
                let records = usage_store::read_request_records(&fixture.paths, &["desktop-response:msg-attribution".to_string()]).unwrap();
                let record = &records["desktop-response:msg-attribution"];
                assert_eq!(record["providerId"], "desktop-provider");
                assert_eq!(record["requestModel"], "claude-sonnet-4-6");
                assert_eq!(record["model"], "upstream-model");
                assert!(!record.to_string().contains("private-"));
                assert!(usage_store::read_all_logs(&fixture.paths).unwrap().is_empty());
                let error_response = json_response(StatusCode::BAD_GATEWAY, json!({"type": "message", "id": "error-message"}));
                track_gateway_provider(error_response, fixture.paths.clone(), &log)
                    .into_body().collect().await.unwrap();
                assert!(usage_store::read_request_records(&fixture.paths, &["desktop-response:error-message".to_string()]).unwrap().is_empty());
            }
        });
    }

    #[test]
    fn gateway_authentication_routes_and_streaming_are_isolated() {
        runtime().block_on(async {
            let fixture = Fixture::new();
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let upstream_port = listener.local_addr().unwrap().port();
            let upstream_task = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let service = service_fn(|request: Request<Incoming>| async move {
                    assert_eq!(request.uri().path(), "/v1/messages");
                    assert_eq!(request.headers()["authorization"], "Bearer upstream-secret");
                    assert!(request.headers().get("x-api-key").is_none());
                    let payload: Value = serde_json::from_slice(
                        &request.into_body().collect().await.unwrap().to_bytes(),
                    )
                    .unwrap();
                    assert_eq!(payload["model"], "upstream-model");
                    assert_eq!(payload["stream"], true);
                    let stream = futures_util::stream::unfold(0, |index| async move {
                        if index == 2 {
                            return None;
                        }
                        if index == 1 {
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                        Some((
                            Ok::<_, Infallible>(Frame::data(Bytes::from(format!(
                                "data: {index}\n\n"
                            )))),
                            index + 1,
                        ))
                    });
                    Ok::<_, Infallible>(
                        Response::builder()
                            .header("content-type", "text/event-stream")
                            .body(StreamBody::new(Box::pin(stream)))
                            .unwrap(),
                    )
                });
                http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await
                    .unwrap();
            });
            let mut data = DesktopData::load(&fixture.paths).unwrap();
            let mut source = provider("proxy");
            source["baseUrl"] = json!(format!("http://127.0.0.1:{upstream_port}"));
            data.providers
                .push(normalize_provider(&source, "upstream-secret").unwrap());
            runtime_provider::set_provider_key(
                &mut data.keys,
                "claude-desktop:example",
                "upstream-secret".to_string(),
            )
            .unwrap();
            runtime_provider::set_provider_key(
                &mut data.settings,
                GATEWAY_KEY,
                "local-secret".to_string(),
            )
            .unwrap();
            data.settings
                .insert("currentProviderId".to_string(), json!("example"));
            data.save(&fixture.paths).unwrap();
            let reservation =
                std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = reservation.local_addr().unwrap().port();
            drop(reservation);
            let manager = DesktopManager::new();
            manager.start(&fixture.paths, port).await.unwrap();
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let base = gateway_url(port);
            assert_eq!(
                client
                    .get(format!("{base}/v1/models"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                client
                    .get(format!("{base}/v1/models"))
                    .bearer_auth("wrong-secret")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            let models: Value = client
                .get(format!("{base}/v1/models"))
                .bearer_auth("local-secret")
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(models["data"].as_array().unwrap().len(), 4);
            assert!(models["data"]
                .as_array()
                .unwrap()
                .iter()
                .all(|model| is_safe_model(text(model, "id"))));
            assert_eq!(
                client
                    .post(format!("{base}/v1/messages"))
                    .bearer_auth("local-secret")
                    .json(&json!({"model": "unknown"}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
            let response = client
                .post(format!("{base}/v1/messages"))
                .bearer_auth("local-secret")
                .header("x-api-key", "must-not-leak")
                .json(&json!({"model": "claude-opus-4-8", "stream": true, "messages": []}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["content-type"], "text/event-stream");
            let mut stream = response.bytes_stream();
            let first = tokio::time::timeout(std::time::Duration::from_millis(300), stream.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(first, Bytes::from_static(b"data: 0\n\n"));
            assert_eq!(
                stream.next().await.unwrap().unwrap(),
                Bytes::from_static(b"data: 1\n\n")
            );
            assert!(stream.next().await.is_none());
            drop(stream);
            let key_usage = runtime_provider::read_provider_key_usage(
                &fixture.paths,
                &json!({"providerId": "example", "cli": "claude-desktop"}),
            )
            .unwrap();
            let key_id = key_usage["activeApiKeyId"].as_str().unwrap();
            assert_eq!(key_usage["keys"][key_id]["requestCount"], 1);
            assert_eq!(key_usage["keys"][key_id]["successCount"], 1);
            assert_eq!(key_usage["keys"][key_id]["inFlightCount"], 0);
            let logs = proxy::read_logs(&fixture.paths, "claude-desktop").unwrap();
            assert_eq!(logs.as_array().unwrap().len(), 5);
            assert_eq!(logs[0]["requestModel"], "claude-opus-4-8");
            assert_eq!(logs[0]["model"], "upstream-model");
            assert_eq!(logs[0]["statusCode"], 200);
            assert_eq!(logs[0]["streaming"], true);
            for secret in [
                "upstream-secret",
                "local-secret",
                "wrong-secret",
                "must-not-leak",
            ] {
                assert!(!logs.to_string().contains(secret));
            }
            assert_eq!(
                proxy::read_logs(&fixture.paths, "claude").unwrap(),
                json!([])
            );
            assert_eq!(
                proxy::read_logs(&fixture.paths, "codex").unwrap(),
                json!([])
            );
            std::fs::write(
                &fixture.paths.storage_files.claude_desktop_request_logs,
                "invalid-json",
            )
            .unwrap();
            assert_eq!(
                client
                    .get(format!("{base}/v1/models"))
                    .bearer_auth("local-secret")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            manager.stop().await;
            assert!(!manager.running().await);
            upstream_task.abort();
        });
    }
}

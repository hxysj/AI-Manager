use crate::api::codex_account;
use crate::core::{
    error::ManagerError,
    image_store::{self, StoredImage},
    paths::AppPaths,
    provider_store,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha3::{Digest, Sha3_512};
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Write},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Emitter;
use uuid::Uuid;

const DIRECT_MODELS: &[&str] = &[
    "gpt-image-2",
    "gpt-image-1.5",
    "gpt-image-2.5-flare",
    "gpt-image-2.5-sunburst",
    "gpt-image-2.5-flare-2026-09-08",
    "gpt-image-2.5-sunburst-2026-09-08",
];
const DEFAULT_IMAGE_MODEL: &str = "gpt-image-2";
const MAX_RESPONSE_BYTES: usize = 128 * 1024 * 1024;
const WEB_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";
const DEFAULT_POW_SCRIPT: &str = "https://chatgpt.com/backend-api/sentinel/sdk.js";

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageRequest {
    account_id: String,
    generation_mode: String,
    mode: String,
    prompt: String,
    model: String,
    size: String,
    quality: String,
    n: u8,
    output_format: String,
    background: String,
    response_format: String,
    #[serde(default)]
    images: Vec<String>,
    #[serde(default)]
    mask: String,
}

impl ImageRequest {
    fn validate(&self) -> Result<(), ManagerError> {
        if self.account_id.is_empty() || self.prompt.trim().is_empty() || self.prompt.len() > 32_000
        {
            return Err(failure("请选择官方账号，并填写不超过 32000 字节的提示词"));
        }
        if !matches!(self.generation_mode.as_str(), "web" | "codex")
            || !matches!(self.mode.as_str(), "generate" | "edit")
            || !is_image_model(&self.model)
            || self.model.len() > 128
            || !self
                .model
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
            || !matches!(
                self.size.as_str(),
                "auto" | "1024x1024" | "1024x1536" | "1536x1024" | "2048x2048"
            )
            || !matches!(self.quality.as_str(), "auto" | "low" | "medium" | "high")
            || !(1..=10).contains(&self.n)
            || !matches!(self.output_format.as_str(), "png" | "jpeg" | "webp")
            || !matches!(
                self.background.as_str(),
                "" | "auto" | "opaque" | "transparent"
            )
            || !matches!(self.response_format.as_str(), "url" | "b64_json")
        {
            return Err(failure("图片任务参数不合法"));
        }
        if self.background == "transparent" && self.output_format == "jpeg" {
            return Err(failure("透明背景请使用 PNG 或 WebP 格式"));
        }
        if self.mode == "edit" && self.images.is_empty() {
            return Err(failure("图片编辑至少需要一张参考图"));
        }
        if self.mode == "generate" && (!self.images.is_empty() || !self.mask.is_empty()) {
            return Err(failure("文生图不能携带参考图或蒙版"));
        }
        if self.images.len() > 10
            || self.images.iter().map(String::len).sum::<usize>() + self.mask.len()
                > 32 * 1024 * 1024
        {
            return Err(failure(
                "最多上传 10 张参考图，编码后的图片总大小不能超过 32 MB",
            ));
        }
        for url in self
            .images
            .iter()
            .chain((!self.mask.is_empty()).then_some(&self.mask))
        {
            let (prefix, encoded) = url
                .split_once(',')
                .ok_or_else(|| failure("参考图必须是有效图片"))?;
            if !matches!(
                prefix,
                "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/webp;base64"
            ) || encoded.len() > 28 * 1024 * 1024
            {
                return Err(failure("参考图只支持 PNG、JPEG、WebP，单张不能超过 20 MB"));
            }
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|_| failure("参考图编码损坏"))?;
            if bytes.len() > 20 * 1024 * 1024 {
                return Err(failure("单张图片不能超过 20 MB"));
            }
            decode_image(&bytes)?;
            if std::ptr::eq(url, &self.mask) && !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(failure("蒙版必须是 PNG 图片"));
            }
        }
        if !self.mask.is_empty() {
            let source = STANDARD
                .decode(self.images[0].split_once(',').unwrap().1)
                .map_err(|_| failure("参考图编码损坏"))?;
            let mask = STANDARD
                .decode(self.mask.split_once(',').unwrap().1)
                .map_err(|_| failure("蒙版编码损坏"))?;
            let source = decode_image(&source)?;
            let mask = decode_image(&mask)?;
            if source.width() != mask.width()
                || source.height() != mask.height()
                || !mask.color().has_alpha()
            {
                return Err(failure("PNG 蒙版需要透明通道，尺寸须与第一张参考图一致"));
            }
        }
        Ok(())
    }

    fn body(&self, responses: bool, driver: &str) -> Value {
        let mut options = json!({ "model": self.model, "size": self.size, "quality": self.quality, "output_format": self.output_format });
        if !self.background.is_empty() {
            options["background"] = json!(self.background);
        }
        if self.n > 1 {
            options["n"] = json!(self.n);
        }
        if !responses {
            options["prompt"] = json!(self.prompt);
            if self.mode == "edit" {
                options["images"] = json!(self
                    .images
                    .iter()
                    .map(|url| json!({ "image_url": url }))
                    .collect::<Vec<_>>());
                if !self.mask.is_empty() {
                    options["mask"] = json!({ "image_url": self.mask });
                }
            }
            return options;
        }
        options["type"] = json!("image_generation");
        options["action"] = json!(self.mode);
        if !self.mask.is_empty() {
            options["input_image_mask"] = json!({ "image_url": self.mask });
        }
        let mut content = vec![json!({ "type": "input_text", "text": self.prompt.trim() })];
        content.extend(
            self.images
                .iter()
                .map(|url| json!({ "type": "input_image", "image_url": url })),
        );
        json!({
            "model": driver,
            "instructions": "When invoking the image_generation tool, use the user's image prompt verbatim. Do not rewrite, expand, summarize, embellish, translate, normalize punctuation, or add or remove visual details or constraints. Preserve the original language, wording, capitalization, quotes, and punctuation exactly.",
            "stream": true, "store": false,
            "reasoning": { "effort": "medium", "summary": "auto" },
            "parallel_tool_calls": true, "include": ["reasoning.encrypted_content"],
            "tool_choice": { "type": "image_generation" },
            "input": [{ "type": "message", "role": "user", "content": content }],
            "tools": [options]
        })
    }
}

fn failure(message: &str) -> ManagerError {
    ManagerError::System(message.into())
}

fn is_image_model(model: &str) -> bool {
    model.starts_with("gpt-image-")
}

pub fn accounts(paths: &AppPaths) -> Result<Value, ManagerError> {
    let accounts = codex_account::read_public_accounts(paths)?;
    Ok(json!(accounts.as_array().unwrap().iter().filter(|account| account["type"] == "codex").map(|account| json!({
        "id": account["id"], "email": account["email"], "plan": account["plan"],
        "active": account["active"], "disabled": account["disabled"], "requiresReauth": account["requires_reauth"]
    })).collect::<Vec<_>>()))
}

pub async fn models(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let account = provider_store::read_codex_accounts(paths)?
        .into_iter()
        .find(|account| account["id"] == payload["accountId"] && account["type"] == "codex")
        .ok_or_else(|| failure("请选择 Codex 官方登录账号"))?;
    if account["disabled"] == true || account["requires_reauth"] == true {
        return Err(failure("所选官方账号不可用，请恢复或重新登录"));
    }
    // 独立匿名会话只共享本次请求的 Cookie，不读取账号凭证。
    let mut builder = reqwest::Client::builder()
        .cookie_store(true)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36");
    let proxy_url = account["proxy"].as_str().unwrap_or("").trim();
    if !proxy_url.is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(proxy_url)
                .map_err(|error| ManagerError::System(error.to_string()))?,
        );
    }
    let client = builder
        .build()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    fetch_image_models(&client, "https://chatgpt.com").await
}

async fn fetch_image_models(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<Value, ManagerError> {
    client
        .get(format!("{base_url}/"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| ManagerError::System(format!("ChatGPT 会话初始化失败：{error}")))?;

    let response = client
        .get(format!("{base_url}/backend-anon/models"))
        .query(&[("iim", "false"), ("is_gizmo", "false")])
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::REFERER, format!("{base_url}/"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| ManagerError::System(format!("获取 OpenAI 模型失败：{error}")))?
        .json::<Value>()
        .await
        .map_err(|error| ManagerError::System(format!("OpenAI 模型响应解析失败：{error}")))?;
    let models = response
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| ManagerError::System("OpenAI 模型响应缺少 models 列表".to_string()))?;
    let mut data = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for slug in DIRECT_MODELS {
        ids.insert((*slug).to_string());
        data.push(json!({ "id": slug, "object": "model", "owned_by": "openai" }));
    }
    for model in models {
        let slug = model["slug"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if is_image_model(&slug) && ids.insert(slug.clone()) {
            data.push(json!({ "id": slug, "object": "model", "owned_by": "openai" }));
        }
    }

    Ok(json!({
        "object": "list",
        "data": data,
        "default_image_model": DEFAULT_IMAGE_MODEL
    }))
}

pub async fn quota(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let account_id = payload["accountId"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| failure("请选择官方登录账号"))?;
    let (client, auth) = image_account_session(paths, cli_targets, account_id, "web").await?;
    let (remaining, reset_after) = web_quota(&client, &auth, &WebContext::new()).await?;
    Ok(json!({
        "accountId": account_id,
        "remaining": remaining,
        "resetAfter": reset_after,
        "updatedAt": chrono::Utc::now().timestamp_millis()
    }))
}

async fn image_account_session(
    paths: &AppPaths,
    cli_targets: &Value,
    account_id: &str,
    generation_mode: &str,
) -> Result<(reqwest::Client, Value), ManagerError> {
    let account = provider_store::read_codex_accounts(paths)?
        .into_iter()
        .find(|account| account["id"] == account_id && account["type"] == "codex")
        .ok_or_else(|| failure("图片工作台仅支持 Codex 官方登录账号，其他 Provider 暂不支持"))?;
    if account["disabled"] == true || account["requires_reauth"] == true {
        return Err(failure(
            "所选官方账号不可用，请在 Provider 页面恢复或重新登录",
        ));
    }
    let cli_target = cli_targets
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == "codex"))
        .ok_or_else(|| failure("未找到 Codex 配置"))?;
    // 复用账号真相源和续期流程，凭据不进入前端或任务记录。
    let auth = codex_account::get_proxy_auth(paths, account_id, cli_target).await?;
    let mut builder = reqwest::Client::builder();
    if generation_mode == "web" {
        builder = builder.cookie_store(true).user_agent(WEB_USER_AGENT);
    } else {
        builder = builder
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("codex_cli_rs/0.146.0 (Windows; image-workbench)");
    }
    let proxy = account["proxy"].as_str().unwrap_or("").trim();
    if !proxy.is_empty() {
        builder =
            builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| failure("官方账号代理配置无效"))?);
    }
    let client = builder
        .build()
        .map_err(|_| failure("无法创建图片请求客户端"))?;
    Ok((client, auth))
}

pub async fn submit(
    app: &tauri::AppHandle,
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let mut request: ImageRequest = serde_json::from_value(payload)?;
    request.model = request.model.trim().to_string();
    request.validate()?;
    let (client, auth) = image_account_session(
        paths,
        cli_targets,
        &request.account_id,
        &request.generation_mode,
    )
    .await?;
    let mut metadata = serde_json::to_value(&request)?;
    metadata.as_object_mut().unwrap().remove("images");
    metadata.as_object_mut().unwrap().remove("mask");
    let task = json!({
        "id": uuid::Uuid::new_v4().to_string(), "createdAt": chrono::Utc::now().timestamp_millis(),
        "status": "processing", "accountName": auth["name"], "request": metadata,
        "inputCount": request.images.len(), "hasMask": !request.mask.is_empty(), "images": [], "imageCount": 0
    });
    image_store::create(paths, &task, &request.images, &request.mask)?;
    let paths = paths.clone();
    let app = app.clone();
    let task_copy = task.clone();
    tauri::async_runtime::spawn(async move {
        let mut task = task_copy;
        let result = if request.generation_mode == "web" {
            execute_web(
                &client,
                &auth,
                &request,
                task["id"].as_str().unwrap(),
                &mut WebContext::new(),
            )
            .await
        } else {
            execute(
                &client,
                &auth,
                &request,
                task["id"].as_str().unwrap(),
                "https://chatgpt.com/backend-api/codex",
            )
            .await
        };
        let images = match result {
            Ok(result) => {
                task["endpoint"] = json!(result.endpoint);
                task["requestId"] = json!(result.request_id);
                task["usage"] = result.usage;
                task["error"] = result.error;
                task["imageCount"] = json!(result.images.len());
                task["images"] =
                    json!(result.images.iter().map(|image| json!({
                    "size": format!("{}x{}", image.width, image.height), "format": image.format,
                    "revisedPrompt": image.revised_prompt
                })).collect::<Vec<_>>());
                task["status"] = json!(if result.images.is_empty() {
                    "failed"
                } else if !task["error"].is_null() || result.images.len() < request.n as usize {
                    "partial"
                } else {
                    "completed"
                });
                if task["status"] == "partial" && task["error"].is_null() {
                    task["error"] = json!({ "message": format!("请求 {} 张，实际返回 {} 张", request.n, result.images.len()), "source": "upstream" });
                }
                result.images
            }
            Err(error) => {
                task["status"] = json!("failed");
                task["error"] = json!({ "message": error.to_string(), "source": "transport" });
                Vec::new()
            }
        };
        task["finishedAt"] = json!(chrono::Utc::now().timestamp_millis());
        if let Err(error) = image_store::finish(&paths, &task, &images) {
            // 保存失败不能声称生成成功；重启仍会把未写回任务标成中断。
            let _ = app.emit(
                "images:storage-error",
                json!({ "taskId": task["id"], "message": error.to_string() }),
            );
        }
        let _ = app.emit(
            "images:changed",
            json!({
                "taskId": task["id"],
                "accountId": request.account_id,
                "generationMode": request.generation_mode
            }),
        );
    });
    Ok(task)
}

struct GenerationResult {
    images: Vec<StoredImage>,
    usage: Value,
    error: Value,
    endpoint: String,
    request_id: String,
}

struct WebRequirements {
    token: String,
    proof_token: String,
    turnstile_diagnostic: Option<String>,
}

struct WebContext {
    base_url: String,
    device_id: String,
    session_id: String,
    script_sources: Vec<String>,
    data_build: String,
}

impl WebContext {
    fn new() -> Self {
        Self {
            base_url: "https://chatgpt.com".to_string(),
            device_id: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4().to_string(),
            script_sources: vec![DEFAULT_POW_SCRIPT.to_string()],
            data_build: String::new(),
        }
    }
}

#[derive(Default)]
struct WebAssetIds {
    conversation_id: String,
    file_ids: Vec<String>,
    sediment_ids: Vec<String>,
    blocked: bool,
    tool_invoked: bool,
    text: String,
}

fn web_builder(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    method: reqwest::Method,
    path: &str,
) -> reqwest::RequestBuilder {
    let account_id = auth["accountId"].as_str().unwrap_or("");
    let mut request = client
        .request(method, format!("{}{path}", context.base_url))
        .bearer_auth(auth["accessToken"].as_str().unwrap_or(""))
        .header("Origin", "https://chatgpt.com")
        .header("Referer", "https://chatgpt.com/")
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8,en-US;q=0.7")
        .header("Cache-Control", "no-cache")
        .header("Pragma", "no-cache")
        .header("Priority", "u=1, i")
        .header("Sec-Ch-Ua", r#""Chromium";v="143", "Not A(Brand";v="24""#)
        .header("Sec-Ch-Ua-Arch", r#""x86""#)
        .header("Sec-Ch-Ua-Bitness", r#""64""#)
        .header("Sec-Ch-Ua-Mobile", "?0")
        .header("Sec-Ch-Ua-Platform", r#""Windows""#)
        .header("Sec-Fetch-Dest", "empty")
        .header("Sec-Fetch-Mode", "cors")
        .header("Sec-Fetch-Site", "same-origin")
        .header("OAI-Language", "zh-CN")
        .header(
            "OAI-Client-Version",
            "prod-a194cd50d4416d3c0b47c740f206b12ce60f5887",
        )
        .header("OAI-Client-Build-Number", "6708908")
        .header("OAI-Device-Id", &context.device_id)
        .header("OAI-Session-Id", &context.session_id)
        .header("X-OpenAI-Target-Path", path)
        .header("X-OpenAI-Target-Route", path);
    if !account_id.is_empty() {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    request
}

async fn execute_web(
    client: &reqwest::Client,
    auth: &Value,
    request: &ImageRequest,
    session_id: &str,
    context: &mut WebContext,
) -> Result<GenerationResult, ManagerError> {
    let (remaining, reset_after) = web_quota(client, auth, &context).await?;
    if remaining == 0 {
        return Err(failure(&format!(
            "Web 生图额度已用尽{}",
            if reset_after.is_empty() {
                String::new()
            } else {
                format!("，预计恢复时间：{reset_after}")
            }
        )));
    }

    let model = if request.model == "gpt-image-2" {
        "gpt-5-5-thinking".to_string()
    } else {
        request.model.clone()
    };
    let references = web_upload_references(client, auth, &context, request).await?;
    web_bootstrap(client, auth, context).await?;
    let mut images = Vec::new();
    let mut conversation_ids = Vec::new();
    let mut last_error = Value::Null;
    let mut sentinel_diagnostics = Vec::new();

    // 网页端没有 n 参数；每张请求都创建一个独立会话。
    for _ in 0..request.n {
        let requirements = match web_requirements(client, auth, &context).await {
            Ok(value) => value,
            Err(error) if !images.is_empty() => {
                last_error = json!({ "source": "web", "message": error.to_string() });
                break;
            }
            Err(error) => return Err(error),
        };
        if let Some(diagnostic) = &requirements.turnstile_diagnostic {
            sentinel_diagnostics.push(diagnostic.clone());
        }
        match web_execute_once(
            client,
            auth,
            &context,
            request,
            &requirements,
            &model,
            &references,
        )
        .await
        {
            Ok((conversation_id, mut generated, error)) if !generated.is_empty() => {
                conversation_ids.push(conversation_id);
                images.append(&mut generated);
                if !error.is_null() {
                    last_error = error;
                    break;
                }
            }
            Ok((conversation_id, _, _)) => {
                conversation_ids.push(conversation_id);
                last_error = json!({
                    "source": "web",
                    "message": "Web 生图会话结束，但未生成可下载的完整图片"
                });
                break;
            }
            Err(error) if !images.is_empty() => {
                last_error = json!({ "source": "web", "message": error.to_string() });
                break;
            }
            Err(error) => return Err(error),
        }
    }

    if images.is_empty() && last_error.is_null() {
        last_error = json!({
            "source": "web",
            "message": "Web 生图未返回可下载的完整图片"
        });
    }
    // 额度只使用上游快照，不能按会话数或图片数推算真实扣减。
    let quota_after = web_quota(client, auth, &context).await.ok();
    Ok(GenerationResult {
        images,
        usage: json!({
            "web_quota_before": {"remaining": remaining, "resetAfter": reset_after},
            "web_remaining": quota_after.as_ref().map(|quota| quota.0),
            "web_reset_after": quota_after.as_ref().map(|quota| &quota.1),
            "web_conversations": conversation_ids,
            "web_sentinel_diagnostics": sentinel_diagnostics
        }),
        error: last_error,
        endpoint: "/backend-api/f/conversation".into(),
        request_id: session_id.to_string(),
    })
}

async fn web_bootstrap(
    client: &reqwest::Client,
    auth: &Value,
    context: &mut WebContext,
) -> Result<(), ManagerError> {
    let response = client
        .get(format!("{}/", context.base_url))
        .bearer_auth(auth["accessToken"].as_str().unwrap_or(""))
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        )
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
        .header("Sec-Ch-Ua", r#""Chromium";v="143", "Not A(Brand";v="24""#)
        .header("Sec-Ch-Ua-Mobile", "?0")
        .header("Sec-Ch-Ua-Platform", r#""Windows""#)
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "none")
        .header("Sec-Fetch-User", "?1")
        .header("Upgrade-Insecure-Requests", "1")
        .header("OAI-Device-Id", &context.device_id)
        .header("OAI-Session-Id", &context.session_id)
        .send()
        .await
        .map_err(|error| failure(&format!("ChatGPT 网页会话初始化失败：{error}")))?;
    let (status, bytes) = read_response_limited(response, "ChatGPT 网页首页").await?;
    if !status.is_success() {
        return Err(failure(&format!(
            "ChatGPT 网页会话初始化失败：HTTP {}",
            status.as_u16()
        )));
    }
    let (scripts, data_build) = web_parse_pow_resources(&String::from_utf8_lossy(&bytes));
    context.script_sources = scripts;
    context.data_build = data_build;
    Ok(())
}

fn web_parse_pow_resources(html: &str) -> (Vec<String>, String) {
    let script_pattern = regex::Regex::new(r#"(?i)<script[^>]+src=["']([^"']+)["']"#).unwrap();
    let build_pattern = regex::Regex::new(r#"c/[^/'"]*/_"#).unwrap();
    let html_build_pattern =
        regex::Regex::new(r#"(?i)<html[^>]*data-build=["']([^"']*)["']"#).unwrap();
    let scripts = script_pattern
        .captures_iter(html)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .collect::<Vec<_>>();
    let data_build = scripts
        .iter()
        .find_map(|script| {
            build_pattern
                .find(script)
                .map(|value| value.as_str().to_string())
        })
        .or_else(|| {
            html_build_pattern
                .captures(html)
                .and_then(|capture| capture.get(1))
                .map(|value| value.as_str().to_string())
        })
        .unwrap_or_default();
    (
        if scripts.is_empty() {
            vec![DEFAULT_POW_SCRIPT.to_string()]
        } else {
            scripts
        },
        data_build,
    )
}

fn web_parse_quota(payload: &Value) -> Result<(u64, String), ManagerError> {
    let item = payload["limits_progress"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["feature_name"] == "image_gen")
        })
        .ok_or_else(|| failure("Web 额度响应缺少 image_gen 记录"))?;
    let remaining = item["remaining"]
        .as_u64()
        .ok_or_else(|| failure("Web 额度响应缺少 image_gen.remaining"))?;
    Ok((
        remaining,
        item["reset_after"].as_str().unwrap_or("").to_string(),
    ))
}

async fn web_quota(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
) -> Result<(u64, String), ManagerError> {
    let init_path = "/backend-api/conversation/init";
    let init_response = web_builder(client, auth, context, reqwest::Method::POST, init_path)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&json!({
            "gizmo_id": null,
            "requested_default_model": null,
            "conversation_id": null,
            "timezone_offset_min": -480
        }))
        .send()
        .await
        .map_err(|error| failure(&format!("Web 额度查询失败：{error}")))?;
    let init_status = init_response.status();
    let init_payload = init_response
        .json::<Value>()
        .await
        .map_err(|error| failure(&format!("Web 额度响应解析失败：{error}")))?;
    if !init_status.is_success() {
        return Err(failure(&format!(
            "Web 额度查询失败：HTTP {}",
            init_status.as_u16()
        )));
    }
    web_parse_quota(&init_payload)
}

async fn web_execute_once(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    request: &ImageRequest,
    requirements: &WebRequirements,
    model: &str,
    references: &[WebReference],
) -> Result<(String, Vec<StoredImage>, Value), ManagerError> {
    let conduit =
        web_prepare_conversation(client, auth, context, request, requirements, model).await?;
    let response = web_start_conversation(
        client,
        auth,
        context,
        request,
        requirements,
        model,
        references,
        &conduit,
    )
    .await?;
    let status = response.status();
    let (_, bytes) = read_response_limited(response, "Web 生图 SSE").await?;
    if !status.is_success() {
        return Err(failure(&format!(
            "Web 生图提交失败：HTTP {}：{}",
            status.as_u16(),
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(800)
                .collect::<String>()
        )));
    }
    let mut ids = web_parse_sse(&bytes, &references);
    if ids.blocked && ids.file_ids.is_empty() && ids.sediment_ids.is_empty() {
        return Err(failure("Web 生图被上游内容策略拒绝"));
    }
    if ids.conversation_id.is_empty() {
        return Err(failure("Web 生图响应缺少 conversation_id"));
    }
    let input_ids = references
        .iter()
        .map(|item| item.file_id.clone())
        .collect::<HashSet<_>>();
    web_poll_assets(client, auth, context, &input_ids, &mut ids).await?;
    let mut images = Vec::new();
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    // 两种资源都尝试；单张下载失败不能丢弃已经成功取得的图片。
    for (sediment, id) in ids
        .file_ids
        .iter()
        .map(|id| (false, id))
        .chain(ids.sediment_ids.iter().map(|id| (true, id)))
    {
        let downloaded = if sediment {
            web_download_sediment(client, auth, context, &ids.conversation_id, id).await
        } else {
            web_download_file(client, auth, context, id).await
        };
        match downloaded {
            Ok(Some(bytes)) => {
                if seen.insert(Sha3_512::digest(&bytes).to_vec()) {
                    match store_image(&STANDARD.encode(bytes), &json!({})) {
                        Ok(image) => images.push(image),
                        Err(error) => errors.push(error.to_string()),
                    }
                }
            }
            Ok(None) => errors.push(format!("图片资源 {id} 暂不可下载")),
            Err(error) => errors.push(error.to_string()),
        }
    }
    if images.is_empty() && !errors.is_empty() {
        return Err(failure(&format!("Web 图片下载失败：{}", errors.join("；"))));
    }
    let error = if errors.is_empty() {
        Value::Null
    } else {
        json!({"source": "web", "message": format!("部分图片下载失败：{}", errors.join("；"))})
    };
    Ok((ids.conversation_id, images, error))
}

async fn read_response_limited(
    response: reqwest::Response,
    label: &str,
) -> Result<(reqwest::StatusCode, Vec<u8>), ManagerError> {
    let status = response.status();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| failure(&format!("{label} 读取失败：{error}")))?;
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(failure(&format!("{label} 超过 128 MB 限制")));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((status, bytes))
}

async fn web_requirements(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
) -> Result<WebRequirements, ManagerError> {
    let p_config = web_build_pow_config(context);
    let encoded = STANDARD.encode(serde_json::to_vec(&p_config)?);
    let p_token = format!("gAAAAAC{encoded}");
    let prepare = web_builder(
        client,
        auth,
        context,
        reqwest::Method::POST,
        "/backend-api/sentinel/chat-requirements/prepare",
    )
    .header("Content-Type", "application/json")
    .json(&json!({ "p": p_token }))
    .send()
    .await
    .map_err(|error| failure(&format!("Sentinel prepare 失败：{error}")))?;
    let prepare_status = prepare.status();
    let payload = prepare
        .json::<Value>()
        .await
        .map_err(|error| failure(&format!("Sentinel prepare 响应解析失败：{error}")))?;
    if !prepare_status.is_success() {
        return Err(failure(&format!(
            "Sentinel prepare 返回错误：HTTP {}",
            prepare_status.as_u16()
        )));
    }
    let prepare_token = payload["prepare_token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| failure("Sentinel prepare 缺少 prepare_token"))?;
    if payload["arkose"]["required"].as_bool() == Some(true) {
        return Err(failure("Web 生图需要 Arkose challenge，当前未实现"));
    }
    let mut proof_token = String::new();
    if payload["proofofwork"]["required"].as_bool() == Some(true) {
        let proof_config = web_build_pow_config(context);
        proof_token = web_proof_token(
            payload["proofofwork"]["seed"].as_str().unwrap_or(""),
            payload["proofofwork"]["difficulty"].as_str().unwrap_or(""),
            &proof_config,
        )?;
    }
    let mut turnstile_diagnostic = None;
    let turnstile_token = if payload["turnstile"]["required"].as_bool() == Some(true) {
        // 参考项目的有限解释器可能返回空值，仍将真实结果交给 finalize 判定。
        // 只有上游签发 requirements token 才能继续，不伪造挑战凭证。
        match web_turnstile_token(payload["turnstile"]["dx"].as_str().unwrap_or(""), &p_token) {
            Ok(token) => token,
            Err(error) => {
                turnstile_diagnostic = Some(error.to_string());
                String::new()
            }
        }
    } else {
        String::new()
    };
    let finalize = web_builder(
        client,
        auth,
        context,
        reqwest::Method::POST,
        "/backend-api/sentinel/chat-requirements/finalize",
    )
    .header("Content-Type", "application/json")
    .json(&json!({
        "prepare_token": prepare_token,
        "proof_token": proof_token,
        "turnstile_token": turnstile_token
    }))
    .send()
    .await
    .map_err(|error| failure(&format!("Sentinel finalize 失败：{error}")))?;
    let (finalize_status, final_bytes) =
        read_response_limited(finalize, "Sentinel finalize").await?;
    let diagnostic = turnstile_diagnostic
        .as_ref()
        .map(|message| format!("；本地 dx 诊断：{message}"))
        .unwrap_or_default();
    if !finalize_status.is_success() {
        return Err(failure(&format!(
            "Sentinel finalize 返回错误：HTTP {}{diagnostic}",
            finalize_status.as_u16()
        )));
    }
    let final_payload: Value = serde_json::from_slice(&final_bytes)
        .map_err(|_| failure(&format!("Sentinel finalize 响应不是有效 JSON{diagnostic}")))?;
    let token = final_payload["token"].as_str().unwrap_or("").to_string();
    if token.is_empty() {
        return Err(failure(&format!(
            "Sentinel finalize 缺少 requirements token{diagnostic}"
        )));
    }
    Ok(WebRequirements {
        token,
        proof_token,
        turnstile_diagnostic,
    })
}

fn web_random_u64() -> u64 {
    let mut bytes = [0_u8; 8];
    if getrandom::getrandom(&mut bytes).is_ok() {
        u64::from_le_bytes(bytes)
    } else {
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64
    }
}

fn web_build_pow_config(context: &WebContext) -> Value {
    const RESOLUTIONS: [u64; 4] = [3000, 2340, 4000, 6000];
    const CORES: [u64; 4] = [8, 16, 24, 32];
    const NAVIGATOR_KEYS: [&str; 6] = [
        "webdriver−false",
        "vendor−Google Inc.",
        "cookieEnabled−true",
        "product−Gecko",
        "pdfViewerEnabled−true",
        "hardwareConcurrency−32",
    ];
    const DOCUMENT_KEYS: [&str; 3] = [
        "__reactContainer$fzelfjyxej8",
        "_reactListening5dehydibo78",
        "location",
    ];
    const WINDOW_KEYS: [&str; 8] = [
        "window",
        "document",
        "location",
        "innerWidth",
        "screen",
        "navigator",
        "performance",
        "crypto",
    ];

    let random = web_random_u64();
    let now = chrono::Utc::now();
    let eastern = chrono::FixedOffset::west_opt(5 * 60 * 60).unwrap();
    let date_text = format!(
        "{} GMT-0500 (Eastern Standard Time)",
        now.with_timezone(&eastern).format("%a %b %d %Y %H:%M:%S")
    );
    let wall_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0;
    let performance_ms = (random % 30_000) as f64 + (random as f64 / u64::MAX as f64);
    let script = &context.script_sources[(random as usize) % context.script_sources.len()];
    json!([
        RESOLUTIONS[(random as usize) % RESOLUTIONS.len()],
        date_text,
        4294705152_u64,
        1,
        WEB_USER_AGENT,
        script,
        context.data_build,
        "en-US",
        "en-US,es-US,en,es",
        random as f64 / u64::MAX as f64,
        NAVIGATOR_KEYS[(random as usize) % NAVIGATOR_KEYS.len()],
        DOCUMENT_KEYS[((random >> 8) as usize) % DOCUMENT_KEYS.len()],
        WINDOW_KEYS[((random >> 16) as usize) % WINDOW_KEYS.len()],
        performance_ms,
        Uuid::new_v4().to_string(),
        "",
        CORES[((random >> 24) as usize) % CORES.len()],
        wall_ms - performance_ms,
        0,
        0,
        0,
        0,
        0,
        0,
        0
    ])
}

#[derive(Clone, Debug, PartialEq)]
enum TurnstileValue {
    Json(Value),
    Ordered(Vec<(String, TurnstileValue)>),
    Function(u8),
}

fn turnstile_key(value: &Value) -> Result<String, ManagerError> {
    match value {
        // Python 的数字键 2、2.0、2e0 相等，字符串键仍独立。
        Value::Number(number) => Ok(format!(
            "number:{}",
            if number.is_f64() {
                let value = number.as_f64().unwrap();
                if value == 0.0 {
                    "0".to_string()
                } else {
                    value.to_string()
                }
            } else {
                number.to_string()
            }
        )),
        Value::String(text) => Ok(format!("string:{text}")),
        _ => Err(failure("Turnstile 指令包含无效寄存器")),
    }
}

fn turnstile_to_string(value: &TurnstileValue) -> String {
    match value {
        TurnstileValue::Json(Value::Null) => "undefined".to_string(),
        TurnstileValue::Json(Value::String(text)) => match text.as_str() {
            "window.Math" => "[object Math]".to_string(),
            "window.Reflect" => "[object Reflect]".to_string(),
            "window.performance" => "[object Performance]".to_string(),
            "window.localStorage" => "[object Storage]".to_string(),
            "window.Object" => "function Object() { [native code] }".to_string(),
            "window.Reflect.set" => "function set() { [native code] }".to_string(),
            "window.performance.now" => "function () { [native code] }".to_string(),
            "window.Object.create" => "function create() { [native code] }".to_string(),
            "window.Object.keys" => "function keys() { [native code] }".to_string(),
            "window.Math.random" => "function random() { [native code] }".to_string(),
            _ => text.clone(),
        },
        TurnstileValue::Json(Value::Array(items)) if items.iter().all(|item| item.is_string()) => {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(",")
        }
        TurnstileValue::Json(Value::Bool(value)) => {
            if *value { "True" } else { "False" }.to_string()
        }
        TurnstileValue::Json(value) => value.to_string(),
        TurnstileValue::Ordered(_) => "[object Object]".to_string(),
        TurnstileValue::Function(_) => "function () { [native code] }".to_string(),
    }
}

fn turnstile_raw_string(value: &TurnstileValue) -> Option<&str> {
    match value {
        TurnstileValue::Json(Value::String(text)) => Some(text),
        _ => None,
    }
}

fn turnstile_json(value: &TurnstileValue) -> Result<String, ManagerError> {
    match value {
        TurnstileValue::Json(Value::Array(items)) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(|item| turnstile_json(&TurnstileValue::Json(item.clone())))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        TurnstileValue::Json(Value::Object(map)) => Ok(format!(
            "{{{}}}",
            map.iter()
                .map(|(key, item)| Ok(format!(
                    "{}: {}",
                    serde_json::to_string(key)?,
                    turnstile_json(&TurnstileValue::Json(item.clone()))?
                )))
                .collect::<Result<Vec<_>, ManagerError>>()?
                .join(", ")
        )),
        TurnstileValue::Json(value) => Ok(serde_json::to_string(value)?),
        TurnstileValue::Ordered(items) => Ok(format!(
            "{{{}}}",
            items
                .iter()
                .map(|(key, item)| Ok(format!(
                    "{}: {}",
                    serde_json::to_string(key)?,
                    turnstile_json(item)?
                )))
                .collect::<Result<Vec<_>, ManagerError>>()?
                .join(", ")
        )),
        TurnstileValue::Function(_) => Err(failure("Turnstile 无法序列化函数")),
    }
}

fn turnstile_xor(text: &str, key: &str) -> Result<String, ManagerError> {
    if key.is_empty() {
        return Ok(text.to_string());
    }
    let keys = key.chars().map(u32::from).collect::<Vec<_>>();
    text.chars()
        .enumerate()
        .map(|(index, character)| {
            char::from_u32(u32::from(character) ^ keys[index % keys.len()])
                .ok_or_else(|| failure("Turnstile XOR 结果不是有效 Unicode"))
        })
        .collect()
}

fn turnstile_register(
    registers: &HashMap<String, TurnstileValue>,
    argument: &Value,
) -> Result<TurnstileValue, ManagerError> {
    registers
        .get(&turnstile_key(argument)?)
        .cloned()
        .ok_or_else(|| failure("Turnstile 指令引用了空寄存器"))
}

fn turnstile_value_to_json(value: TurnstileValue) -> Value {
    match value {
        TurnstileValue::Json(value) => value,
        TurnstileValue::Ordered(items) => Value::Object(
            items
                .into_iter()
                .map(|(key, value)| (key, turnstile_value_to_json(value)))
                .collect(),
        ),
        TurnstileValue::Function(code) => json!(code),
    }
}

fn turnstile_apply(
    opcode: u8,
    args: &[Value],
    registers: &mut HashMap<String, TurnstileValue>,
    started: Instant,
    result: &mut String,
) -> Result<(), ManagerError> {
    let minimum_args = match opcode {
        1 | 2 | 5 | 8 | 14 | 15 | 17 | 23 => 2,
        3 | 7 | 18 | 19 => 1,
        6 | 20 | 24 => 3,
        21 => 0,
        _ => return Err(failure(&format!("Turnstile 包含未知指令：{opcode}"))),
    };
    if args.len() < minimum_args {
        return Err(failure("Turnstile 指令参数不足"));
    }
    let destination = || {
        args.first()
            .ok_or_else(|| failure("Turnstile 指令缺少目标寄存器"))
            .and_then(turnstile_key)
    };
    match opcode {
        1 => {
            let key = destination()?;
            let left = registers
                .get(&key)
                .map(turnstile_to_string)
                .ok_or_else(|| failure("Turnstile XOR 目标为空"))?;
            let right = turnstile_to_string(&turnstile_register(registers, &args[1])?);
            registers.insert(
                key,
                TurnstileValue::Json(json!(turnstile_xor(&left, &right)?)),
            );
        }
        2 => {
            registers.insert(destination()?, TurnstileValue::Json(args[1].clone()));
        }
        3 => {
            let value = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| failure("Turnstile 结果不是字符串"))?;
            *result = STANDARD.encode(value.as_bytes());
        }
        5 => {
            let key = destination()?;
            let current = registers
                .get(&key)
                .cloned()
                .ok_or_else(|| failure("Turnstile 加法目标为空"))?;
            let incoming = turnstile_register(registers, &args[1])?;
            let next = match current {
                TurnstileValue::Json(Value::Array(mut items)) => {
                    items.push(turnstile_value_to_json(incoming));
                    TurnstileValue::Json(Value::Array(items))
                }
                TurnstileValue::Json(Value::String(_)) | TurnstileValue::Json(Value::Number(_)) => {
                    TurnstileValue::Json(json!(format!(
                        "{}{}",
                        turnstile_to_string(&current),
                        turnstile_to_string(&incoming)
                    )))
                }
                _ if matches!(
                    incoming,
                    TurnstileValue::Json(Value::String(_) | Value::Number(_))
                ) =>
                {
                    TurnstileValue::Json(json!(format!(
                        "{}{}",
                        turnstile_to_string(&current),
                        turnstile_to_string(&incoming)
                    )))
                }
                _ => TurnstileValue::Json(json!("NaN")),
            };
            registers.insert(key, next);
        }
        6 | 24 => {
            let left = turnstile_register(registers, &args[1])?;
            let right = turnstile_register(registers, &args[2])?;
            if let (
                TurnstileValue::Json(Value::String(left)),
                TurnstileValue::Json(Value::String(right)),
            ) = (left, right)
            {
                let joined = format!("{left}.{right}");
                registers.insert(
                    destination()?,
                    TurnstileValue::Json(json!(if opcode == 6
                        && joined == "window.document.location"
                    {
                        "https://chatgpt.com/".to_string()
                    } else {
                        joined
                    })),
                );
            }
        }
        7 => {
            let target = turnstile_register(registers, &args[0])?;
            if turnstile_raw_string(&target) == Some("window.Reflect.set") {
                if args.len() < 4 {
                    return Err(failure("Turnstile Reflect.set 参数不足"));
                }
                let object_key = turnstile_key(&args[1])?;
                let property = turnstile_to_string(&turnstile_register(registers, &args[2])?);
                let value = turnstile_register(registers, &args[3])?;
                if let Some(TurnstileValue::Ordered(items)) = registers.get_mut(&object_key) {
                    if let Some((_, current)) = items.iter_mut().find(|(key, _)| key == &property) {
                        *current = value;
                    } else {
                        items.push((property, value));
                    }
                }
            } else if let TurnstileValue::Function(function) = target {
                let values = args[1..]
                    .iter()
                    .map(|argument| {
                        turnstile_register(registers, argument).map(turnstile_value_to_json)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                turnstile_apply(function, &values, registers, started, result)?;
            }
        }
        8 => {
            registers.insert(destination()?, turnstile_register(registers, &args[1])?);
        }
        14 => {
            let source = turnstile_to_string(&turnstile_register(registers, &args[1])?);
            registers.insert(
                destination()?,
                TurnstileValue::Json(serde_json::from_str(&source)?),
            );
        }
        15 => {
            let serialized = turnstile_json(&turnstile_register(registers, &args[1])?)?;
            registers.insert(destination()?, TurnstileValue::Json(json!(serialized)));
        }
        17 => {
            let target = turnstile_register(registers, &args[1])?;
            if let TurnstileValue::Function(function) = target {
                let values = args[2..]
                    .iter()
                    .map(|argument| {
                        turnstile_register(registers, argument).map(turnstile_value_to_json)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                turnstile_apply(function, &values, registers, started, result)?;
                registers.insert(destination()?, TurnstileValue::Json(Value::Null));
                return Ok(());
            }
            let value = match turnstile_raw_string(&target).unwrap_or("") {
                "window.performance.now" => TurnstileValue::Json(json!(
                    started.elapsed().as_secs_f64() * 1000.0
                        + web_random_u64() as f64 / u64::MAX as f64
                )),
                "window.Object.create" => TurnstileValue::Ordered(Vec::new()),
                "window.Object.keys"
                    if args.get(2).map(|argument| {
                        turnstile_register(registers, argument)
                            .map(|value| {
                                turnstile_raw_string(&value) == Some("window.localStorage")
                            })
                            .unwrap_or(false)
                    }) == Some(true) =>
                {
                    TurnstileValue::Json(json!([
                        "STATSIG_LOCAL_STORAGE_INTERNAL_STORE_V4",
                        "STATSIG_LOCAL_STORAGE_STABLE_ID",
                        "client-correlated-secret",
                        "oai/apps/capExpiresAt",
                        "oai-did",
                        "STATSIG_LOCAL_STORAGE_LOGGING_REQUEST",
                        "UiState.isNavigationCollapsed.1"
                    ]))
                }
                "window.Math.random" => {
                    TurnstileValue::Json(json!(web_random_u64() as f64 / u64::MAX as f64))
                }
                _ => return Ok(()),
            };
            registers.insert(destination()?, value);
        }
        18 => {
            let key = destination()?;
            let decoded = STANDARD
                .decode(turnstile_to_string(
                    registers
                        .get(&key)
                        .ok_or_else(|| failure("Turnstile Base64 目标为空"))?,
                ))
                .map_err(|_| failure("Turnstile Base64 解码失败"))?;
            registers.insert(
                key,
                TurnstileValue::Json(json!(String::from_utf8(decoded)
                    .map_err(|_| failure("Turnstile Base64 文本不是 UTF-8"))?)),
            );
        }
        19 => {
            let key = destination()?;
            let encoded = STANDARD.encode(
                turnstile_to_string(
                    registers
                        .get(&key)
                        .ok_or_else(|| failure("Turnstile Base64 目标为空"))?,
                )
                .as_bytes(),
            );
            registers.insert(key, TurnstileValue::Json(json!(encoded)));
        }
        20 => {
            if turnstile_register(registers, &args[0])? == turnstile_register(registers, &args[1])?
            {
                if let TurnstileValue::Function(function) = turnstile_register(registers, &args[2])?
                {
                    let values = args[3..]
                        .iter()
                        .map(|argument| {
                            turnstile_register(registers, argument).map(turnstile_value_to_json)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    turnstile_apply(function, &values, registers, started, result)?;
                }
            }
        }
        21 => {}
        23 => {
            if turnstile_register(registers, &args[0])? != TurnstileValue::Json(Value::Null) {
                if let TurnstileValue::Function(function) = turnstile_register(registers, &args[1])?
                {
                    turnstile_apply(function, &args[2..], registers, started, result)?;
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn web_turnstile_token(dx: &str, p: &str) -> Result<String, ManagerError> {
    let decoded = STANDARD
        .decode(dx)
        .map_err(|_| failure("Turnstile dx Base64 无效"))?;
    let decoded = String::from_utf8(decoded).map_err(|_| failure("Turnstile dx 不是 UTF-8"))?;
    let instructions: Value = serde_json::from_str(&turnstile_xor(&decoded, p)?)
        .map_err(|_| failure("Turnstile dx 指令 JSON 无效"))?;
    let instructions = instructions
        .as_array()
        .ok_or_else(|| failure("Turnstile dx 指令不是数组"))?;
    let mut registers = HashMap::new();
    for opcode in [1_u8, 2, 3, 5, 6, 7, 8, 14, 15, 17, 18, 19, 20, 21, 23, 24] {
        registers.insert(
            turnstile_key(&json!(opcode))?,
            TurnstileValue::Function(opcode),
        );
    }
    registers.insert(
        turnstile_key(&json!(9))?,
        TurnstileValue::Json(json!(instructions)),
    );
    registers.insert(
        turnstile_key(&json!(10))?,
        TurnstileValue::Json(json!("window")),
    );
    registers.insert(turnstile_key(&json!(16))?, TurnstileValue::Json(json!(p)));
    let started = Instant::now();
    let mut result = String::new();
    let mut skipped = 0;
    let mut failed = 0;
    let mut first_error = String::new();
    for (index, instruction) in instructions.iter().enumerate() {
        let Some(parts) = instruction.as_array() else {
            skipped += 1;
            continue;
        };
        let Some(target) = parts.first() else {
            skipped += 1;
            continue;
        };
        // 首项是函数寄存器，可以是浮点编号或经指令 8 复制的别名。
        let Ok(TurnstileValue::Function(opcode)) = turnstile_register(&registers, target) else {
            skipped += 1;
            continue;
        };
        // 与参考解释器一致地继续执行，但只保留不含挑战内容的诊断。
        if let Err(error) =
            turnstile_apply(opcode, &parts[1..], &mut registers, started, &mut result)
        {
            failed += 1;
            if first_error.is_empty() {
                first_error = format!("；首个失败位于第 {} 条，操作 {opcode}：{error}", index + 1);
            }
        }
    }
    if result.is_empty() {
        return Err(failure(&format!("Turnstile 本地解释器未生成 token（共 {} 条，跳过 {skipped} 条，失败 {failed} 条）{first_error}", instructions.len())));
    }
    Ok(result)
}

fn web_proof_token(seed: &str, difficulty: &str, config: &Value) -> Result<String, ManagerError> {
    let target = difficulty
        .as_bytes()
        .chunks(2)
        .map(|item| {
            if item.len() != 2 {
                return None;
            }
            let high = (item[0] as char).to_digit(16)?;
            let low = (item[1] as char).to_digit(16)?;
            Some(((high << 4) | low) as u8)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| failure("Sentinel difficulty 无效"))?;
    if target.is_empty() || target.len() > 64 {
        return Err(failure("Sentinel difficulty 为空或超过 SHA3-512 长度"));
    }
    let mut work = config
        .as_array()
        .cloned()
        .ok_or_else(|| failure("Sentinel 配置无效"))?;
    if work.len() != 25 {
        return Err(failure("Sentinel 配置必须包含 25 项"));
    }
    for nonce in 0u64..500_000 {
        work[3] = json!(nonce);
        work[9] = json!(nonce >> 1);
        let payload = STANDARD.encode(serde_json::to_vec(&work)?);
        let mut input = seed.as_bytes().to_vec();
        input.extend(payload.as_bytes());
        let digest = Sha3_512::digest(input);
        if digest[..target.len()] <= target[..] {
            return Ok(format!("gAAAAAB{payload}"));
        }
    }
    Err(failure("Sentinel proof token 在计算上限内未找到"))
}

#[derive(Clone)]
struct WebReference {
    file_id: String,
    file_name: String,
    mime_type: String,
    file_size: usize,
    width: u32,
    height: u32,
}

fn web_prompt(request: &ImageRequest) -> String {
    let mut prompt = request.prompt.trim().to_string();
    if request.size != "auto" && !request.size.is_empty() {
        prompt.push_str(&format!("\n\n输出图片尺寸为 {}。", request.size));
    }
    if request.quality != "auto" && !request.quality.is_empty() {
        prompt.push_str(&format!("\n输出图片质量为 {}。", request.quality));
    }
    prompt
}

async fn web_upload_references(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    request: &ImageRequest,
) -> Result<Vec<WebReference>, ManagerError> {
    let mut references = Vec::new();
    if request.mode != "edit" {
        return Ok(references);
    }
    for (index, input) in request.images.iter().enumerate() {
        let (header, encoded) = input
            .split_once(',')
            .ok_or_else(|| failure("Web 图片编辑参考图格式无效"))?;
        let mut mime_type = header
            .strip_prefix("data:")
            .and_then(|value| value.strip_suffix(";base64"))
            .unwrap_or("image/png")
            .to_string();
        if !matches!(
            mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/webp"
        ) {
            return Err(failure("Web 图片编辑只支持 PNG、JPEG、WebP"));
        }
        let mut bytes = STANDARD
            .decode(encoded)
            .map_err(|_| failure("Web 参考图 Base64 编码无效"))?;
        if index == 0 && !request.mask.is_empty() {
            bytes = web_apply_mask(&bytes, &request.mask)?;
            mime_type = "image/png".to_string();
        }
        let image = decode_image(&bytes)?;
        let file_size = bytes.len();
        let file_name = format!(
            "reference-{}.{}",
            index + 1,
            mime_type.rsplit('/').next().unwrap_or("png")
        );
        let path = "/backend-api/files";
        let response = web_builder(client, auth, context, reqwest::Method::POST, path)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "file_name": file_name,
                "file_size": file_size,
                "use_case": "multimodal",
                "width": image.width(),
                "height": image.height()
            }))
            .send()
            .await
            .map_err(|error| failure(&format!("Web 参考图申请失败：{error}")))?;
        let status = response.status();
        let payload = response
            .json::<Value>()
            .await
            .map_err(|error| failure(&format!("Web 参考图申请响应解析失败：{error}")))?;
        if !status.is_success() {
            return Err(failure(&format!(
                "Web 参考图申请失败：HTTP {}",
                status.as_u16()
            )));
        }
        let file_id = payload["file_id"].as_str().unwrap_or("").to_string();
        let upload_url = payload["upload_url"].as_str().unwrap_or("").to_string();
        if file_id.is_empty() || upload_url.is_empty() {
            return Err(failure("Web 参考图申请响应缺少 file_id 或 upload_url"));
        }
        let upload = client
            .put(upload_url)
            .header("Content-Type", &mime_type)
            .header("x-ms-blob-type", "BlockBlob")
            .header("x-ms-version", "2020-04-08")
            .header("Origin", "https://chatgpt.com")
            .header("Referer", "https://chatgpt.com/")
            .body(bytes)
            .send()
            .await
            .map_err(|error| failure(&format!("Web 参考图上传失败：{error}")))?;
        if !upload.status().is_success() {
            return Err(failure(&format!(
                "Web 参考图上传失败：HTTP {}",
                upload.status().as_u16()
            )));
        }
        let confirm_path = format!("/backend-api/files/{file_id}/uploaded");
        let confirm = web_builder(client, auth, context, reqwest::Method::POST, &confirm_path)
            .header("Content-Type", "application/json")
            .json(&json!({}))
            .send()
            .await
            .map_err(|error| failure(&format!("Web 参考图确认失败：{error}")))?;
        if !confirm.status().is_success() {
            return Err(failure(&format!(
                "Web 参考图确认失败：HTTP {}",
                confirm.status().as_u16()
            )));
        }
        references.push(WebReference {
            file_id,
            file_name,
            mime_type,
            file_size,
            width: image.width(),
            height: image.height(),
        });
    }
    Ok(references)
}

fn web_apply_mask(source: &[u8], mask_data_url: &str) -> Result<Vec<u8>, ManagerError> {
    let mask_bytes = STANDARD
        .decode(
            mask_data_url
                .split_once(',')
                .ok_or_else(|| failure("Web 蒙版格式无效"))?
                .1,
        )
        .map_err(|_| failure("Web 蒙版 Base64 编码无效"))?;
    let mut source = decode_image(source)?.to_rgba8();
    let mask = decode_image(&mask_bytes)?.to_rgba8();
    if source.dimensions() != mask.dimensions() {
        return Err(failure("Web 蒙版尺寸须与第一张参考图一致"));
    }
    for (pixel, mask_pixel) in source.pixels_mut().zip(mask.pixels()) {
        pixel.0[3] = mask_pixel.0[3];
    }
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(source)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| failure("Web 蒙版合成失败"))?;
    Ok(output.into_inner())
}

async fn web_prepare_conversation(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    request: &ImageRequest,
    requirements: &WebRequirements,
    model: &str,
) -> Result<String, ManagerError> {
    let path = "/backend-api/f/conversation/prepare";
    let prompt = web_prompt(request);
    let mut outgoing = web_builder(client, auth, context, reqwest::Method::POST, path)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header(
            "OpenAI-Sentinel-Chat-Requirements-Token",
            &requirements.token,
        );
    if !requirements.proof_token.is_empty() {
        outgoing = outgoing.header("OpenAI-Sentinel-Proof-Token", &requirements.proof_token);
    }
    let response = outgoing
        .json(&json!({
            "action": "next",
            "fork_from_shared_post": false,
            "parent_message_id": Uuid::new_v4().to_string(),
            "model": model,
            "client_prepare_state": "success",
            "timezone_offset_min": -480,
            "timezone": "Asia/Shanghai",
            "conversation_mode": {"kind": "primary_assistant"},
            "system_hints": ["picture_v2"],
            "partial_query": {
                "id": Uuid::new_v4().to_string(),
                "author": {"role": "user"},
                "content": {"content_type": "text", "parts": [prompt]}
            },
            "supports_buffering": true,
            "supported_encodings": ["v1"],
            "client_contextual_info": {"app_name": "chatgpt.com"}
        }))
        .send()
        .await
        .map_err(|error| failure(&format!("Web 会话准备失败：{error}")))?;
    let status = response.status();
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| failure(&format!("Web 会话准备响应解析失败：{error}")))?;
    if !status.is_success() {
        return Err(failure(&format!(
            "Web 会话准备失败：HTTP {}",
            status.as_u16()
        )));
    }
    // 参考链路允许 prepare 不返回 conduit，只在有真实值时附加请求头。
    Ok(payload["conduit_token"].as_str().unwrap_or("").to_string())
}

async fn web_start_conversation(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    request: &ImageRequest,
    requirements: &WebRequirements,
    model: &str,
    references: &[WebReference],
    conduit: &str,
) -> Result<reqwest::Response, ManagerError> {
    let path = "/backend-api/f/conversation";
    let prompt = web_prompt(request);
    let mut parts = references
        .iter()
        .map(|item| {
            json!({
                "content_type": "image_asset_pointer",
                "asset_pointer": format!("file-service://{}", item.file_id),
                "width": item.width,
                "height": item.height,
                "size_bytes": item.file_size
            })
        })
        .collect::<Vec<_>>();
    parts.push(json!(prompt));
    let content_type = if references.is_empty() {
        "text"
    } else {
        "multimodal_text"
    };
    let mut outgoing = web_builder(client, auth, context, reqwest::Method::POST, path)
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header(
            "OpenAI-Sentinel-Chat-Requirements-Token",
            &requirements.token,
        )
        .header("X-Oai-Turn-Trace-Id", Uuid::new_v4().to_string());
    if !conduit.is_empty() {
        outgoing = outgoing.header("X-Conduit-Token", conduit);
    }
    if !requirements.proof_token.is_empty() {
        outgoing = outgoing.header("OpenAI-Sentinel-Proof-Token", &requirements.proof_token);
    }
    let response = outgoing
        .json(&json!({
            "action": "next",
            "messages": [{
                "id": Uuid::new_v4().to_string(),
                "author": {"role": "user"},
                "create_time": chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                "content": {"content_type": content_type, "parts": parts},
                "metadata": {
                    "developer_mode_connector_ids": [],
                    "selected_github_repos": [],
                    "selected_all_github_repos": false,
                    "system_hints": ["picture_v2"],
                    "serialization_metadata": {"custom_symbol_offsets": []},
                    "attachments": references.iter().map(|item| json!({
                        "id": item.file_id,
                        "mimeType": item.mime_type,
                        "name": item.file_name,
                        "size": item.file_size,
                        "width": item.width,
                        "height": item.height
                    })).collect::<Vec<_>>()
                }
            }],
            "parent_message_id": Uuid::new_v4().to_string(),
            "model": model,
            "client_prepare_state": "sent",
            "timezone_offset_min": -480,
            "timezone": "Asia/Shanghai",
            "conversation_mode": {"kind": "primary_assistant"},
            "enable_message_followups": true,
            "system_hints": ["picture_v2"],
            "supports_buffering": true,
            "supported_encodings": ["v1"],
            "client_contextual_info": {
                "is_dark_mode": false,
                "time_since_loaded": 1200,
                "page_height": 1072,
                "page_width": 1724,
                "pixel_ratio": 1.2,
                "screen_height": 1440,
                "screen_width": 2560,
                "app_name": "chatgpt.com"
            },
            "paragen_cot_summary_display_override": "allow",
            "force_parallel_switch": "auto"
        }))
        .send()
        .await
        .map_err(|error| failure(&format!("Web 生图提交失败：{error}")))?;
    Ok(response)
}

fn web_parse_sse(bytes: &[u8], references: &[WebReference]) -> WebAssetIds {
    let input_ids = references
        .iter()
        .map(|item| item.file_id.clone())
        .collect::<HashSet<_>>();
    let mut result = WebAssetIds::default();
    let mut document = json!({});
    let mut last_path = String::new();
    for frame in String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .split("\n\n")
    {
        let data = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:").map(str::trim))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(payload) = serde_json::from_str::<Value>(&data) {
            if result.conversation_id.is_empty() {
                result.conversation_id = payload["conversation_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
            }
            if payload["type"] == "moderation" && payload["moderation_response"]["blocked"] == true
            {
                result.blocked = true;
            }
            if payload["metadata"]["tool_invoked"] == true
                || payload["metadata"]["turn_use_case"] == "image gen"
            {
                result.tool_invoked = true;
            }
            // 完整消息建立角色上下文，后续 patch 只能归入当前消息。
            if let Some(message) = payload
                .get("message")
                .or_else(|| payload["v"].get("message"))
            {
                if document["message"]["id"] != message["id"]
                    || document["message"]["author"] != message["author"]
                {
                    web_collect_assets(&document, &input_ids, &mut result);
                }
                document = json!({"message": message});
                last_path.clear();
            } else if payload["o"].is_string() {
                web_apply_message_patch(&mut document, &payload, "");
                if let Some(path) = payload["p"].as_str().filter(|path| !path.is_empty()) {
                    last_path = path.to_string();
                }
            } else if payload["v"].is_string() && !last_path.is_empty() {
                web_apply_message_patch(
                    &mut document,
                    &json!({"p": last_path, "o": "append", "v": payload["v"]}),
                    "",
                );
            }
            if payload.get("message").is_none()
                && payload["v"].get("message").is_none()
                && !payload["o"].is_string()
            {
                web_collect_assets(&payload, &input_ids, &mut result);
            }
        }
    }
    // 指针本身也会分片，消息合并完成前不能保存半截资源 ID。
    web_collect_assets(&document, &input_ids, &mut result);
    result
}

fn web_apply_message_patch(document: &mut Value, event: &Value, prefix: &str) {
    let path = format!("{prefix}{}", event["p"].as_str().unwrap_or(""));
    if event["o"] == "patch" {
        if let Some(patches) = event["v"].as_array() {
            for patch in patches {
                web_apply_message_patch(document, patch, &path);
            }
        }
        return;
    }
    if !matches!(event["o"].as_str(), Some("append" | "replace" | "add")) {
        return;
    }
    let mut target = document;
    if !path.is_empty() {
        if !path.starts_with('/') {
            return;
        }
        for part in path[1..].split('/') {
            let key = part.replace("~1", "/").replace("~0", "~");
            target = match target {
                Value::Object(map) => map.entry(key).or_insert(Value::Null),
                Value::Array(items) => {
                    let index = if key == "-" {
                        items.len()
                    } else if let Ok(index) = key.parse::<usize>() {
                        index
                    } else {
                        return;
                    };
                    if index == items.len() {
                        items.push(Value::Null);
                    }
                    let Some(item) = items.get_mut(index) else {
                        return;
                    };
                    item
                }
                _ => return,
            };
        }
    }
    let incoming = &event["v"];
    if event["o"] == "append" {
        match (&mut *target, incoming) {
            (Value::String(text), Value::String(part)) => {
                text.push_str(part);
                return;
            }
            (Value::Array(items), Value::Array(parts)) => {
                items.extend(parts.iter().cloned());
                return;
            }
            (Value::Array(items), value) => {
                items.push(value.clone());
                return;
            }
            _ => {}
        }
    }
    *target = incoming.clone();
}

fn web_collect_assets(value: &Value, input_ids: &HashSet<String>, result: &mut WebAssetIds) {
    match value {
        Value::Object(map) => {
            if result.conversation_id.is_empty() {
                result.conversation_id = map
                    .get("conversation_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
            let role = map
                .get("author")
                .and_then(Value::as_object)
                .and_then(|author| author.get("role"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            if role == "user" {
                return;
            }
            if matches!(role.as_str(), "assistant" | "tool") {
                let content = map.get("content").unwrap_or(&Value::Null);
                let metadata = map.get("metadata").unwrap_or(&Value::Null);
                let is_image_gen = metadata["async_task_type"] == "image_gen";
                result.tool_invoked |= is_image_gen;
                if role == "assistant" {
                    if let Some(parts) = content["parts"].as_array() {
                        let text = parts
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n");
                        if !text.is_empty() {
                            result.text = text;
                        }
                    }
                }
                if is_image_gen || web_has_asset_pointer(content) || web_has_asset_pointer(metadata)
                {
                    web_collect_pointers(content, input_ids, result);
                    web_collect_pointers(metadata, input_ids, result);
                }
            }
            for (key, item) in map {
                if key == "input_message" {
                    continue;
                }
                if key == "mapping" {
                    if let Some(nodes) = item.as_object() {
                        let mut messages = nodes.values().collect::<Vec<_>>();
                        messages.sort_by(|left, right| {
                            left["message"]["create_time"]
                                .as_f64()
                                .unwrap_or(0.0)
                                .total_cmp(&right["message"]["create_time"].as_f64().unwrap_or(0.0))
                        });
                        for node in messages {
                            web_collect_assets(node, input_ids, result);
                        }
                    }
                } else {
                    web_collect_assets(item, input_ids, result);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                web_collect_assets(item, input_ids, result);
            }
        }
        _ => {}
    }
}

fn web_has_asset_pointer(value: &Value) -> bool {
    match value {
        Value::String(text) => text.contains("file-service://") || text.contains("sediment://"),
        Value::Array(items) => items.iter().any(web_has_asset_pointer),
        Value::Object(map) => {
            map.get("content_type").and_then(Value::as_str) == Some("image_asset_pointer")
                || map.values().any(web_has_asset_pointer)
        }
        _ => false,
    }
}

fn web_collect_pointers(value: &Value, input_ids: &HashSet<String>, result: &mut WebAssetIds) {
    match value {
        Value::String(text) => {
            for (prefix, target) in [
                ("file-service://", &mut result.file_ids),
                ("sediment://", &mut result.sediment_ids),
            ] {
                let mut remaining = text.as_str();
                while let Some(index) = remaining.find(prefix) {
                    let candidate = &remaining[index + prefix.len()..];
                    let id = candidate
                        .chars()
                        .take_while(|character| {
                            character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                        })
                        .collect::<String>();
                    if !id.is_empty()
                        && !matches!(id.as_str(), "file-service" | "file_upload")
                        && !input_ids.contains(&id)
                        && !target.iter().any(|item| item == &id)
                    {
                        target.push(id);
                    }
                    let advance = candidate
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .unwrap_or_default();
                    remaining = &candidate[advance..];
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                web_collect_pointers(item, input_ids, result);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                web_collect_pointers(item, input_ids, result);
            }
        }
        _ => {}
    }
}

async fn web_poll_assets(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    input_ids: &HashSet<String>,
    ids: &mut WebAssetIds,
) -> Result<(), ManagerError> {
    let mut previous = (ids.file_ids.clone(), ids.sediment_ids.clone());
    let mut last_error = String::new();
    let mut wait_secs = 5;
    for _ in 0..24 {
        tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
        // tasks 只提供本会话的辅助错误，不能覆盖有效输出或使主查询失败。
        if let Ok(response) = web_builder(
            client,
            auth,
            context,
            reqwest::Method::GET,
            "/backend-api/tasks",
        )
        .header("Accept", "application/json")
        .send()
        .await
        {
            if response.status().is_success() {
                if let Ok(payload) = response.json::<Value>().await {
                    if let Some(tasks) = payload["tasks"].as_array() {
                        for task in tasks {
                            if (task["conversation_id"] == ids.conversation_id
                                || task["original_conversation_id"] == ids.conversation_id)
                                && task["image_gen_message"]["metadata"]["is_error"] == true
                            {
                                last_error = task["image_gen_message"]["content"]["parts"]
                                    .as_array()
                                    .map(|parts| {
                                        parts
                                            .iter()
                                            .filter_map(Value::as_str)
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                    })
                                    .unwrap_or_default();
                            }
                        }
                    }
                }
            }
        }
        let path = format!("/backend-api/conversation/{}", ids.conversation_id);
        let response = web_builder(client, auth, context, reqwest::Method::GET, &path)
            .header("Accept", "application/json")
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("Web 会话查询失败：{error}");
                continue;
            }
        };
        wait_secs = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5)
            .min(16);
        if response.status().is_success() {
            let payload = match response.json::<Value>().await {
                Ok(payload) => payload,
                Err(error) => {
                    last_error = format!("Web 会话查询响应解析失败：{error}");
                    continue;
                }
            };
            web_collect_assets(&payload, input_ids, ids);
            let current = (ids.file_ids.clone(), ids.sediment_ids.clone());
            if (!current.0.is_empty() || !current.1.is_empty()) && previous == current {
                return Ok(());
            }
            previous = current;
        } else {
            last_error = format!("Web 会话查询失败：HTTP {}", response.status().as_u16());
            if matches!(response.status().as_u16(), 400 | 401 | 403) {
                break;
            }
        }
    }
    if !ids.file_ids.is_empty() || !ids.sediment_ids.is_empty() {
        return Ok(());
    }
    let detail = if !last_error.is_empty() {
        last_error
    } else {
        ids.text.chars().take(500).collect()
    };
    Err(failure(&format!(
        "Web 会话 {} 尚未取得图片{}",
        ids.conversation_id,
        if detail.is_empty() {
            String::new()
        } else {
            format!("：{detail}")
        }
    )))
}

async fn web_download_file(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    file_id: &str,
) -> Result<Option<Vec<u8>>, ManagerError> {
    let path = format!("/backend-api/files/{file_id}/download");
    let response = web_builder(client, auth, context, reqwest::Method::GET, &path)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| failure(&format!("Web 图片地址查询失败：{error}")))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| failure(&format!("Web 图片地址响应解析失败：{error}")))?;
    let Some(url) = payload["download_url"]
        .as_str()
        .or_else(|| payload["url"].as_str())
    else {
        return Ok(None);
    };
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| failure(&format!("Web 图片下载失败：{error}")))?;
    let (status, bytes) = read_response_limited(response, "Web 图片").await?;
    if !status.is_success() {
        return Ok(None);
    }
    Ok(Some(bytes))
}

async fn web_download_sediment(
    client: &reqwest::Client,
    auth: &Value,
    context: &WebContext,
    conversation_id: &str,
    sediment_id: &str,
) -> Result<Option<Vec<u8>>, ManagerError> {
    let path =
        format!("/backend-api/conversation/{conversation_id}/attachment/{sediment_id}/download");
    let response = web_builder(client, auth, context, reqwest::Method::GET, &path)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| failure(&format!("Web 图片地址查询失败：{error}")))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| failure(&format!("Web 图片地址响应解析失败：{error}")))?;
    let Some(url) = payload["download_url"]
        .as_str()
        .or_else(|| payload["url"].as_str())
    else {
        return Ok(None);
    };
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| failure(&format!("Web 图片下载失败：{error}")))?;
    let (status, bytes) = read_response_limited(response, "Web 图片").await?;
    if !status.is_success() {
        return Ok(None);
    }
    Ok(Some(bytes))
}

async fn execute(
    client: &reqwest::Client,
    auth: &Value,
    request: &ImageRequest,
    session: &str,
    base_url: &str,
) -> Result<GenerationResult, ManagerError> {
    let driver = std::env::var("SUB2API_IMAGES_MAIN_MODEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "gpt-5.6-luna".into());
    let mut responses = !DIRECT_MODELS.contains(&request.model.as_str());
    loop {
        let endpoint = if responses {
            "responses"
        } else if request.mode == "edit" {
            "images/edits"
        } else {
            "images/generations"
        };
        let mut outgoing = client
            .post(format!("{base_url}/{endpoint}"))
            .bearer_auth(auth["accessToken"].as_str().unwrap_or(""))
            .header(
                "chatgpt-account-id",
                auth["accountId"].as_str().unwrap_or(""),
            )
            .header("originator", "codex_cli_rs")
            .header("version", "0.146.0")
            .header("session_id", session)
            .header("conversation_id", session)
            .header(
                "accept",
                if responses {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .json(&request.body(responses, &driver));
        if responses {
            outgoing = outgoing.header("OpenAI-Beta", "responses=experimental");
        }
        let response = outgoing
            .send()
            .await
            .map_err(|_| failure("图片请求连接失败，请检查网络和官方账号代理；未自动重试"))?;
        let status = response.status().as_u16();
        // 仅端点不存在时回退一次，不将鉴权、额度或政策错误伪装为协议错误。
        if !responses && matches!(status, 404 | 405) {
            responses = true;
            continue;
        }
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let is_sse = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .contains("text/event-stream");
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        let mut transport_error = None;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) if bytes.len() + chunk.len() <= MAX_RESPONSE_BYTES => {
                    bytes.extend_from_slice(&chunk)
                }
                Ok(_) => {
                    transport_error = Some("上游图片响应超过 128 MB 限制");
                    break;
                }
                Err(_) => {
                    transport_error = Some("读取上游图片响应时连接中断");
                    break;
                }
            }
        }
        let mut result = if !(200..300).contains(&status) {
            let body = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
            GenerationResult {
                images: vec![],
                usage: Value::Null,
                error: upstream_error(
                    &body,
                    "http",
                    status,
                    &format!("图片服务返回 HTTP {status}，请检查账号权限与额度"),
                ),
                endpoint: String::new(),
                request_id: String::new(),
            }
        } else if responses && is_sse {
            parse_responses(&bytes)?
        } else if responses {
            GenerationResult {
                images: vec![],
                usage: Value::Null,
                error: json!({ "message": "Responses 未返回有效事件流", "source": "sse" }),
                endpoint: String::new(),
                request_id: String::new(),
            }
        } else {
            parse_direct(&bytes)?
        };
        if let Some(message) = transport_error {
            result.error = json!({ "message": message, "source": "transport" });
        }
        result.endpoint = format!("/backend-api/codex/{endpoint}");
        result.request_id = request_id;
        return Ok(result);
    }
}

fn upstream_error(body: &Value, source: &str, status: u16, fallback: &str) -> Value {
    let error = body
        .get("error")
        .filter(|value| !value.is_null())
        .unwrap_or(body);
    json!({ "source": source, "httpStatus": status,
        "code": error["code"], "type": error["type"],
        "message": error["message"].as_str().unwrap_or(fallback).chars().take(800).collect::<String>() })
}

fn parse_direct(bytes: &[u8]) -> Result<GenerationResult, ManagerError> {
    let body: Value =
        serde_json::from_slice(bytes).map_err(|_| failure("图片服务返回的 JSON 损坏或不完整"))?;
    let mut result = GenerationResult {
        images: vec![],
        usage: body["usage"].clone(),
        error: Value::Null,
        endpoint: String::new(),
        request_id: String::new(),
    };
    if !body["error"].is_null() {
        result.error = upstream_error(&body, "http", 200, "图片生成失败");
    }
    if let Some(items) = body["data"].as_array() {
        for item in items {
            match store_image(item["b64_json"].as_str().unwrap_or(""), item) {
                Ok(image) => result.images.push(image),
                Err(error) => {
                    result.error = json!({ "source": "image", "message": error.to_string() })
                }
            }
        }
    }
    if result.images.is_empty() && result.error.is_null() {
        result.error = json!({ "source": "http", "message": "上游没有返回完整图片" });
    }
    Ok(result)
}

fn parse_responses(bytes: &[u8]) -> Result<GenerationResult, ManagerError> {
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    let mut pending = Vec::new();
    let mut completed = Vec::new();
    let mut result = GenerationResult {
        images: vec![],
        usage: Value::Null,
        error: Value::Null,
        endpoint: String::new(),
        request_id: String::new(),
    };
    let mut seen_completed = false;
    for frame in text.split("\n\n") {
        let data = frame
            .lines()
            .filter_map(|line| {
                line.strip_prefix("data:")
                    .map(|value| value.strip_prefix(' ').unwrap_or(value))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let event: Value = match serde_json::from_str(&data) {
            Ok(value) => value,
            Err(_) => {
                result.error = json!({ "source": "sse", "message": "上游事件数据损坏或不完整" });
                continue;
            }
        };
        let kind = event["type"].as_str().unwrap_or("");
        match kind {
            "response.output_item.done" if event["item"]["type"] == "image_generation_call" => {
                pending.push(event["item"].clone())
            }
            "response.completed" => {
                seen_completed = true;
                result.usage = event["response"]["usage"].clone();
                if let Some(output) = event["response"]["output"].as_array() {
                    completed.extend(
                        output
                            .iter()
                            .filter(|item| item["type"] == "image_generation_call")
                            .cloned(),
                    );
                }
                if !event["response"]["error"].is_null()
                    || matches!(
                        event["response"]["status"].as_str(),
                        Some("failed" | "incomplete")
                    )
                {
                    result.error =
                        upstream_error(&event["response"], "sse", 200, "图片响应未完整完成");
                }
            }
            "error" | "response.failed" | "response.incomplete" => {
                let body = if event["response"].is_object() {
                    &event["response"]
                } else {
                    &event
                };
                result.error = upstream_error(body, "sse", 200, "图片响应失败或未完整完成");
                if !body["usage"].is_null() {
                    result.usage = body["usage"].clone();
                }
            }
            _ => {}
        }
    }
    let items = if completed.is_empty() {
        pending
    } else {
        completed
    };
    let mut seen = HashSet::new();
    for item in items {
        let encoded = item["result"].as_str().unwrap_or("");
        if !seen.insert(encoded.to_string()) {
            continue;
        }
        match store_image(encoded, &item) {
            Ok(image) => result.images.push(image),
            Err(error) => result.error = json!({ "source": "image", "message": error.to_string() }),
        }
    }
    if result.images.is_empty() && result.error.is_null() {
        result.error = json!({ "source": "sse", "code": "image_generation_unavailable", "message": "上游未返回完整图片，请检查模型权限或调整提示词" });
    } else if !seen_completed && result.error.is_null() {
        result.error =
            json!({ "source": "sse", "message": "连接在完成事件前结束，已保留收到的完整图片" });
    }
    Ok(result)
}

fn decode_image(bytes: &[u8]) -> Result<image::DynamicImage, ManagerError> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| failure("无法识别图片格式"))?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err(failure("图片仅支持 PNG、JPEG、WebP"));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| failure("图片损坏或尺寸超过 8192 像素限制"))
}

fn store_image(encoded: &str, metadata: &Value) -> Result<StoredImage, ManagerError> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| failure("上游图片 Base64 编码无效"))?;
    let image = decode_image(&bytes)?;
    let format = match image::guess_format(&bytes).map_err(|_| failure("无法识别图片格式"))?
    {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpeg",
        image::ImageFormat::WebP => "webp",
        _ => return Err(failure("不支持的图片格式")),
    };
    let mut thumbnail = Cursor::new(Vec::new());
    image
        .thumbnail(320, 240)
        .write_to(&mut thumbnail, image::ImageFormat::Png)
        .map_err(|_| failure("生成缩略图失败"))?;
    Ok(StoredImage {
        bytes,
        format: format.into(),
        width: image.width(),
        height: image.height(),
        revised_prompt: metadata["revised_prompt"].as_str().unwrap_or("").into(),
        thumbnail: format!(
            "data:image/png;base64,{}",
            STANDARD.encode(thumbnail.into_inner())
        ),
    })
}

pub async fn export(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let ids: Vec<String> = serde_json::from_value(payload["ids"].clone())?;
    let target = payload["targetPath"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| failure("请选择导出位置"))?;
    let images = image_store::read_images(paths, &ids)?;
    if images.is_empty() {
        return Err(failure("所选任务没有可导出的图片"));
    }
    let count = images.len();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (id, index, format, bytes) in images {
        archive
            .start_file(
                format!("{id}/image-{}.{}", index + 1, format),
                zip::write::SimpleFileOptions::default(),
            )
            .map_err(|_| failure("创建图片压缩包失败"))?;
        archive.write_all(&bytes)?;
    }
    let output = archive
        .finish()
        .map_err(|_| failure("完成图片压缩包失败"))?
        .into_inner();
    tokio::fs::write(target, output).await?;
    Ok(json!({ "imageCount": count, "filePath": target }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_models_use_anonymous_session_and_validate_responses() {
        use std::io::{Read, Write};

        tauri::async_runtime::block_on(async {
            for (homepage_status, models_status, body, expected_error, discovered_ids) in [
                (
                    200,
                    200,
                    r#"{"models":[{"slug":"gpt-image-example"},{"slug":"gpt-image-example"},{"slug":"gpt-image-other"},{"slug":"gpt-5"},{"slug":"gpt-5-mini"},{"slug":"gpt-4o"},{"slug":""},{}]}"#,
                    "",
                    vec!["gpt-image-example", "gpt-image-other"],
                ),
                (403, 200, "{}", "会话初始化失败", vec![]),
                (200, 403, "{}", "获取 OpenAI 模型失败", vec![]),
                (200, 200, "<html>challenge</html>", "响应解析失败", vec![]),
                (200, 200, "{}", "缺少 models 列表", vec![]),
                (200, 200, r#"{"models":[]}"#, "", vec![]),
                (200, 200, r#"{"models":[{"slug":"gpt-5"}]}"#, "", vec![]),
            ] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let base_url = format!("http://{}", listener.local_addr().unwrap());
                let server = std::thread::spawn(move || {
                    let mut requests = Vec::new();
                    let mut responses = vec![(homepage_status, "")];
                    if homepage_status == 200 {
                        responses.push((models_status, body));
                    }
                    for (status, body) in responses {
                        let (mut socket, _) = listener.accept().unwrap();
                        let mut request = Vec::new();
                        let mut buffer = [0u8; 1024];
                        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                            let count = socket.read(&mut buffer).unwrap();
                            assert!(count > 0);
                            request.extend_from_slice(&buffer[..count]);
                        }
                        requests.push(String::from_utf8(request).unwrap().to_ascii_lowercase());
                        write!(socket, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nSet-Cookie: anon-session=test; Path=/\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                    }
                    requests
                });
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .cookie_store(true)
                    .build()
                    .unwrap();
                let result = fetch_image_models(&client, &base_url).await;
                let requests = server.join().unwrap();
                assert!(requests[0].starts_with("get / http/1.1"));
                assert!(requests
                    .iter()
                    .all(|request| !request.contains("authorization:")));
                if homepage_status == 200 {
                    assert!(requests[1]
                        .starts_with("get /backend-anon/models?iim=false&is_gizmo=false http/1.1"));
                    assert!(requests[1].contains("cookie: anon-session=test"));
                }
                if expected_error.is_empty() {
                    let result = result.unwrap();
                    assert_eq!(result["object"], "list");
                    assert_eq!(result["default_image_model"], DEFAULT_IMAGE_MODEL);
                    assert_eq!(
                        result["data"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|item| item["id"].as_str().unwrap())
                            .collect::<Vec<_>>(),
                        DIRECT_MODELS
                            .iter()
                            .copied()
                            .chain(discovered_ids)
                            .collect::<Vec<_>>()
                    );
                } else {
                    assert!(result.unwrap_err().to_string().contains(expected_error));
                }
            }
        });
    }

    fn request() -> ImageRequest {
        serde_json::from_value(
            json!({ "accountId": "official", "generationMode": "codex", "mode": "generate", "prompt": "  画一个苹果  ",
            "model": "gpt-image-2", "size": "1024x1024", "quality": "high", "n": 1,
            "outputFormat": "png", "background": "", "responseFormat": "url" }),
        )
        .unwrap()
    }

    fn png() -> String {
        let image = image::DynamicImage::new_rgba8(2, 3);
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        STANDARD.encode(bytes.into_inner())
    }

    fn events(items: &[Value]) -> Vec<u8> {
        items
            .iter()
            .map(|item| format!("data: {item}\r\n\r\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn image_request_custom_model_and_validation() {
        let mut request = request();
        request.generation_mode = "web".into();
        request.model = "gpt-image-custom-snapshot".into();
        assert!(request.validate().is_ok());
        request.model = "gpt-5-5-thinking".into();
        assert!(request.validate().is_err());
        request.generation_mode = "other".into();
        assert!(request.validate().is_err());
        request.generation_mode = "codex".into();
        request.model = "gpt-image-custom-snapshot".into();
        assert!(request.validate().is_ok());
        assert!(!DIRECT_MODELS.contains(&request.model.as_str()));
        request.model = "gpt-5.6-luna".into();
        assert!(request.validate().is_err());
        request.model = "gpt-image-2".into();
        request.n = 0;
        assert!(request.validate().is_err());
        request.n = 1;
        request.mode = "edit".into();
        assert!(request.validate().is_err());
        request
            .images
            .push(format!("data:image/png;base64,{}", png()));
        request.mask = request.images[0].clone();
        assert!(request.validate().is_ok());
        request.output_format = "jpeg".into();
        request.background = "transparent".into();
        assert!(request.validate().is_err());
    }

    #[test]
    fn web_quota_requires_a_trusted_image_gen_value() {
        assert_eq!(
            web_parse_quota(&json!({
                "limits_progress": [
                    {"feature_name": "other", "remaining": 99},
                    {"feature_name": "image_gen", "remaining": 3, "reset_after": "tomorrow"}
                ]
            }))
            .unwrap(),
            (3, "tomorrow".to_string())
        );
        assert_eq!(
            web_parse_quota(&json!({
                "limits_progress": [{"feature_name": "image_gen", "remaining": 0}]
            }))
            .unwrap(),
            (0, String::new())
        );
        assert!(web_parse_quota(&json!({"limits_progress": []})).is_err());
        assert!(web_parse_quota(&json!({
            "limits_progress": [{"feature_name": "image_gen"}]
        }))
        .is_err());
        for remaining in [json!(-1), json!(1.5), json!(true), Value::Null, json!("3")] {
            assert!(web_parse_quota(&json!({
                "limits_progress": [{"feature_name": "image_gen", "remaining": remaining}]
            }))
            .is_err());
        }
    }

    #[test]
    fn web_pow_resources_and_known_vector_match_reference_algorithm() {
        let (scripts, build) = web_parse_pow_resources(
            r#"<html data-build="fallback"><script src="/assets/c/build-42/_/main.js"></script><script src="/other.js"></script></html>"#,
        );
        assert_eq!(scripts, vec!["/assets/c/build-42/_/main.js", "/other.js"]);
        assert_eq!(build, "c/build-42/_");
        let (scripts, build) = web_parse_pow_resources("<html></html>");
        assert_eq!(scripts, vec![DEFAULT_POW_SCRIPT]);
        assert!(build.is_empty());

        let config = json!([
            3000,
            "date",
            4294705152_u64,
            1,
            "ua",
            "script",
            "build",
            "en-US",
            "langs",
            0.5,
            "nav",
            "doc",
            "window",
            1,
            "uuid",
            "",
            8,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
        ]);
        assert_eq!(
            web_proof_token("known-seed", "00", &config).unwrap(),
            "gAAAAABWzMwMDAsImRhdGUiLDQyOTQ3MDUxNTIsMzUyLCJ1YSIsInNjcmlwdCIsImJ1aWxkIiwiZW4tVVMiLCJsYW5ncyIsMTc2LCJuYXYiLCJkb2MiLCJ3aW5kb3ciLDEsInV1aWQiLCIiLDgsMCwwLDAsMCwwLDAsMCwwXQ=="
        );
    }

    #[test]
    fn web_turnstile_executes_the_finite_instruction_format() {
        let p = "test-p";
        let plain = r#"[[3,"token"]]"#;
        let dx = STANDARD.encode(turnstile_xor(plain, p).unwrap());
        assert_eq!(web_turnstile_token(&dx, p).unwrap(), "dG9rZW4=");

        let plain = r#"[[2,30,"window"],[2,31,"Math"],[24,32,30,31],[2,33,"random"],[24,34,32,33],[17,35,34],[15,36,35],[7,3,36]]"#;
        let dx = STANDARD.encode(turnstile_xor(plain, p).unwrap());
        let token = web_turnstile_token(&dx, p).unwrap();
        let value = String::from_utf8(STANDARD.decode(token).unwrap())
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((0.0..=1.0).contains(&value));
    }

    #[test]
    fn web_turnstile_resolves_function_registers_like_the_reference() {
        let p = "test-p";
        // 覆盖浮点指令、宽编号/字符串别名、覆盖原编号和间接调用。
        for plain in [
            r#"[[2.0,30.0,"token"],[7e0,3.0,30]]"#,
            r#"[[8,300.5,2],[300.5,30,"token"],[8,512,3],[7,512,30]]"#,
            r#"[[8,"finish",3],["finish","token"]]"#,
            r#"[[8,300,3],[8,3,2],[3,30,"token"],[7,300,30]]"#,
            r#"[[8,-2.5,3],[-2.5,"token"]]"#,
            r#"[[2,"3","ignored"],[3,"token"]]"#,
            r#"[[2,30,"token"],[17,31,3,30]]"#,
            r#"[[8,-0.0,3],[0,"token"]]"#,
            r#"[null,[],[{},1],[999],[2],[3,"token"],[999.5]]"#,
        ] {
            let dx = STANDARD.encode(turnstile_xor(plain, p).unwrap());
            assert_eq!(web_turnstile_token(&dx, p).unwrap(), "dG9rZW4=", "{plain}");
        }
        let dx = STANDARD.encode(turnstile_xor(r#"[[999],[],[2]]"#, p).unwrap());
        assert!(web_turnstile_token(&dx, p).is_err());
        assert!(web_turnstile_token("invalid-base64", p).is_err());
    }

    fn web_reference(file_id: &str) -> WebReference {
        WebReference {
            file_id: file_id.to_string(),
            file_name: "input.png".to_string(),
            mime_type: "image/png".to_string(),
            file_size: 1,
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn web_sse_only_collects_output_messages_and_excludes_inputs() {
        let bytes = events(&[json!({
            "conversation_id": "conversation-1",
            "mapping": {
                "input": {"message": {
                    "author": {"role": "user"},
                    "content": {"parts": [{"asset_pointer": "file-service://input-file"}]}
                }},
                "tool": {"message": {
                    "author": {"role": "tool"},
                    "metadata": {"async_task_type": "image_gen"},
                    "content": {"parts": [
                        {"content_type": "image_asset_pointer", "asset_pointer": "file-service://output-file"},
                        "sediment://output_attachment"
                    ]}
                }},
                "untrusted": {"asset_pointer": "file-service://nested-without-role"}
            }
        })]);
        let ids = web_parse_sse(&bytes, &[web_reference("input-file")]);
        assert_eq!(ids.conversation_id, "conversation-1");
        assert_eq!(ids.file_ids, vec!["output-file"]);
        assert_eq!(ids.sediment_ids, vec!["output_attachment"]);
    }

    #[test]
    fn web_mask_replaces_the_source_alpha_channel() {
        let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]));
        let mask = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 7]));
        let mut source_bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut source_bytes, image::ImageFormat::Png)
            .unwrap();
        let mut mask_bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(mask)
            .write_to(&mut mask_bytes, image::ImageFormat::Png)
            .unwrap();
        let result = web_apply_mask(
            &source_bytes.into_inner(),
            &format!(
                "data:image/png;base64,{}",
                STANDARD.encode(mask_bytes.into_inner())
            ),
        )
        .unwrap();
        assert_eq!(
            decode_image(&result).unwrap().to_rgba8().get_pixel(0, 0).0,
            [10, 20, 30, 7]
        );
    }

    #[test]
    fn image_protocol_preserves_prompt_and_separates_models() {
        let mut request = request();
        let direct = request.body(false, "driver");
        assert_eq!(direct["prompt"], "  画一个苹果  ");
        assert!(direct.get("response_format").is_none());
        assert!(direct.get("stream").is_none());
        request.mode = "edit".into();
        request.images = vec!["data:image/png;base64,reference".into()];
        request.mask = "data:image/png;base64,mask".into();
        let response = request.body(true, "driver");
        assert_eq!(response["model"], "driver");
        assert_eq!(response["tools"][0]["model"], "gpt-image-2");
        assert_eq!(response["tools"][0]["action"], "edit");
        assert_eq!(
            response["tools"][0]["input_image_mask"]["image_url"],
            request.mask
        );
        assert_eq!(response["input"][0]["content"].as_array().unwrap().len(), 2);
        assert_eq!(response["stream"], true);
    }

    #[test]
    fn image_direct_validates_real_bytes_and_http_200_errors() {
        let parsed = parse_direct(json!({ "data": [{ "b64_json": png(), "size": "999x999" }], "usage": { "output_tokens": 12 } }).to_string().as_bytes()).unwrap();
        assert_eq!(parsed.images.len(), 1);
        assert_eq!((parsed.images[0].width, parsed.images[0].height), (2, 3));
        assert_eq!(parsed.usage["output_tokens"], 12);
        let empty = parse_direct(br#"{"data":[]}"#).unwrap();
        assert!(!empty.error.is_null());
        let error =
            parse_direct(br#"{"error":{"code":"quota","message":"quota exceeded"}}"#).unwrap();
        assert_eq!(error.error["code"], "quota");
        assert!(error.images.is_empty());
        let corrupt = parse_direct(br#"{"data":[{"b64_json":"aGVsbG8="}]}"#).unwrap();
        assert!(corrupt.images.is_empty());
        assert!(!corrupt.error.is_null());
    }

    #[test]
    fn image_sse_deduplicates_completed_and_preserves_partial_failure() {
        let item = json!({ "type": "image_generation_call", "result": png() });
        let result = parse_responses(&events(&[
            json!({ "type": "response.output_item.done", "item": item }),
            json!({ "type": "response.completed", "response": { "output": [item, item], "usage": { "output_tokens": 3 } } }),
        ])).unwrap();
        assert_eq!(result.images.len(), 1);
        assert!(result.error.is_null());
        assert_eq!(result.usage["output_tokens"], 3);
        let partial = parse_responses(&events(&[
            json!({ "type": "response.output_item.done", "item": item }),
            json!({ "type": "response.incomplete", "response": { "error": { "message": "incomplete" } } }),
        ])).unwrap();
        assert_eq!(partial.images.len(), 1);
        assert_eq!(partial.error["message"], "incomplete");
        let preview_only = parse_responses(&events(&[json!({ "type": "response.image_generation_call.partial_image", "partial_image_b64": png() })])).unwrap();
        assert!(preview_only.images.is_empty());
        assert!(!preview_only.error.is_null());
    }

    #[test]
    fn image_store_persists_results_and_handles_restart() {
        let root =
            std::env::temp_dir().join(format!("image-workbench-test-{}", uuid::Uuid::new_v4()));
        let paths = crate::core::paths::resolve_app_paths(&root);
        image_store::initialize(&paths).unwrap();
        let task = json!({ "id": "one", "createdAt": 1, "status": "processing", "request": { "responseFormat": "url", "generationMode": "web" } });
        image_store::create(
            &paths,
            &task,
            &[format!("data:image/png;base64,{}", png())],
            "",
        )
        .unwrap();
        assert_eq!(
            image_store::inputs(&paths, "one").unwrap()["images"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let task2 = json!({ "id": "two", "createdAt": 2, "status": "processing", "request": { "generationMode": "codex" } });
        image_store::create(&paths, &task2, &[], "").unwrap();
        assert!(
            image_store::create(&paths, &json!({ "id": "three", "createdAt": 3 }), &[], "")
                .is_err()
        );
        assert!(image_store::delete(&paths, &["one".into()]).is_err());
        let mut task = task;
        task["status"] = json!("completed");
        image_store::finish(&paths, &task, &[store_image(&png(), &json!({})).unwrap()]).unwrap();
        let detail = image_store::detail(&paths, "one").unwrap();
        assert_eq!(detail["request"]["generationMode"], "web");
        assert!(detail["data"][0]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        image_store::initialize(&paths).unwrap();
        assert_eq!(
            image_store::detail(&paths, "two").unwrap()["status"],
            "interrupted"
        );
        let history = image_store::list(&paths, &json!({})).unwrap();
        assert_eq!(history["items"][0]["request"]["generationMode"], "codex");
        assert_eq!(history["items"][1]["request"]["generationMode"], "web");
        assert_eq!(
            image_store::list(&paths, &json!({"status": "completed"})).unwrap()["total"],
            1
        );
        image_store::delete(&paths, &["one".into(), "two".into()]).unwrap();
        assert!(image_store::read_images(&paths, &["one".into()])
            .unwrap()
            .is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn web_sentinel_uses_finalize_as_the_acceptance_result() {
        tauri::async_runtime::block_on(async {
            for (solvable, status, final_body, accepted) in [
                (false, 200, r#"{"token":"accepted"}"#, true),
                (true, 200, r#"{"token":"accepted"}"#, true),
                (false, 403, "not-json", false),
                (false, 200, "{}", false),
            ] {
                let (address, handle) = mock_http_responder(2, move |index, raw, _| {
                    let body: Value =
                        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
                    if index == 0 {
                        assert!(raw
                            .starts_with("POST /backend-api/sentinel/chat-requirements/prepare "));
                        let program = if solvable {
                            r#"[[3,"token"]]"#
                        } else {
                            r#"[[8,123.5,999],[123.5,"unused"]]"#
                        };
                        let dx = STANDARD
                            .encode(turnstile_xor(program, body["p"].as_str().unwrap()).unwrap());
                        (200, "application/json".into(), serde_json::to_vec(&json!({
                            "prepare_token": "prepared", "turnstile": {"required": true, "dx": dx}
                        })).unwrap())
                    } else {
                        assert!(raw
                            .starts_with("POST /backend-api/sentinel/chat-requirements/finalize "));
                        assert_eq!(body["prepare_token"], "prepared");
                        assert_eq!(body["proof_token"], "");
                        assert_eq!(
                            body["turnstile_token"],
                            if solvable { "dG9rZW4=" } else { "" }
                        );
                        (
                            status,
                            "application/json".into(),
                            final_body.as_bytes().to_vec(),
                        )
                    }
                })
                .await;
                let context = WebContext {
                    base_url: address,
                    ..WebContext::new()
                };
                let result = web_requirements(&reqwest::Client::new(), &json!({}), &context).await;
                assert_eq!(result.is_ok(), accepted);
                if let Ok(requirements) = result {
                    assert_eq!(requirements.token, "accepted");
                    assert_eq!(requirements.turnstile_diagnostic.is_some(), !solvable);
                } else {
                    let message = result.err().unwrap().to_string();
                    assert!(message.contains("Sentinel finalize"));
                    assert!(message.contains("本地 dx 诊断"));
                    assert!(!message.contains("unused"));
                }
                assert_eq!(handle.join().unwrap().len(), 2);
            }
            for body in [
                json!({}),
                json!({"prepare_token":"prepared","arkose":{"required":true}}),
            ] {
                let (address, handle) =
                    mock_http(vec![(200, "application/json".into(), body.to_string())]).await;
                let context = WebContext {
                    base_url: address,
                    ..WebContext::new()
                };
                assert!(
                    web_requirements(&reqwest::Client::new(), &json!({}), &context)
                        .await
                        .is_err()
                );
                assert_eq!(handle.join().unwrap().len(), 1);
            }
        });
    }

    #[test]
    fn web_sse_merges_patches_without_collecting_partial_or_input_pointers() {
        let bytes = events(&[
            json!({"conversation_id":"conversation-1", "v":{"message":{"id":"input", "author":{"role":"user"}, "content":{"parts":[]}}}}),
            json!({"p":"/message/content/parts", "o":"append", "v":[{"asset_pointer":"file-service://input"}]}),
            json!({"v":{"message":{"id":"output", "author":{"role":"tool"}, "metadata":{"async_task_type":"image_gen"}, "content":{"parts":[{"asset_pointer":"file-service://out"}]}}}}),
            json!({"p":"/message/content/parts/0/asset_pointer", "o":"append", "v":"put"}),
            json!({"v":"-1"}),
            json!({"p":"", "o":"patch", "v":[
                {"p":"/message/content/parts", "o":"append", "v":[{"asset_pointer":"sediment://attachment-1"}]},
                {"p":"/message/status", "o":"replace", "v":"finished_successfully"}
            ]}),
            json!({"type":"moderation","moderation_response":{"blocked":true}}),
        ]);
        let result = web_parse_sse(&bytes, &[web_reference("input")]);
        assert_eq!(result.file_ids, vec!["output-1"]);
        assert_eq!(result.sediment_ids, vec!["attachment-1"]);
        assert!(result.blocked);
        assert!(result.tool_invoked);
    }

    #[test]
    fn web_image_flow_keeps_outputs_after_transient_poll_and_partial_download_failure() {
        tauri::async_runtime::block_on(async {
            let (address, handle) = mock_http_responder(14, move |index, raw, address| {
                let path = raw.lines().next().unwrap().split_whitespace().nth(1).unwrap();
                let json_response = |value: Value| (200, "application/json".into(), serde_json::to_vec(&value).unwrap());
                match index {
                    0 | 13 => {
                        assert_eq!(path, "/backend-api/conversation/init");
                        json_response(json!({"limits_progress":[{"feature_name":"image_gen", "remaining": if index == 0 { 3 } else { 1 }}]}))
                    }
                    1 => { assert_eq!(path, "/"); (200, "text/html".into(), b"<html></html>".to_vec()) }
                    2 => {
                        assert_eq!(path, "/backend-api/sentinel/chat-requirements/prepare");
                        let body: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
                        let dx = STANDARD.encode(turnstile_xor(r#"[[8,123.5,999]]"#, body["p"].as_str().unwrap()).unwrap());
                        json_response(json!({"prepare_token":"prepared", "turnstile":{"required":true,"dx":dx}, "proofofwork":{"required":true,"seed":"seed","difficulty":"ff"}}))
                    }
                    3 => {
                        assert_eq!(path, "/backend-api/sentinel/chat-requirements/finalize");
                        let body: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
                        assert_eq!(body["turnstile_token"], "");
                        assert!(body["proof_token"].as_str().unwrap().starts_with("gAAAAAB"));
                        json_response(json!({"token":"accepted"}))
                    }
                    4 => { assert_eq!(path, "/backend-api/f/conversation/prepare"); json_response(json!({})) }
                    5 => {
                        assert_eq!(path, "/backend-api/f/conversation");
                        assert!(!raw.to_ascii_lowercase().contains("x-conduit-token:"));
                        assert!(raw.to_ascii_lowercase().contains("openai-sentinel-chat-requirements-token: accepted"));
                        let body: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
                        assert_eq!(body["model"], "gpt-5-5-thinking");
                        assert_eq!(body["system_hints"], json!(["picture_v2"]));
                        assert_eq!(body["client_contextual_info"]["screen_width"], 2560);
                        (200, "text/event-stream".into(), events(&[json!({"conversation_id":"conversation-1", "message":{
                            "author":{"role":"tool"}, "metadata":{"async_task_type":"image_gen"},
                            "content":{"parts":["file-service://output-1", "sediment://attachment-1"]}
                        }})]))
                    }
                    6 | 8 => {
                        assert_eq!(path, "/backend-api/tasks");
                        json_response(json!({"tasks":[{"conversation_id":"conversation-1", "image_gen_message":{"metadata":{"is_error":true}, "content":{"parts":["暂时错误"]}}}]}))
                    }
                    7 => { assert_eq!(path, "/backend-api/conversation/conversation-1"); (503, "application/json".into(), b"{}".to_vec()) }
                    9 => { assert_eq!(path, "/backend-api/conversation/conversation-1"); json_response(json!({"mapping":{}})) }
                    10 => { assert_eq!(path, "/backend-api/files/output-1/download"); json_response(json!({"download_url":format!("{address}/asset")})) }
                    11 => { assert_eq!(path, "/asset"); (200, "image/png".into(), STANDARD.decode(png()).unwrap()) }
                    12 => { assert_eq!(path, "/backend-api/conversation/conversation-1/attachment/attachment-1/download"); (403, "application/json".into(), b"{}".to_vec()) }
                    _ => unreachable!(),
                }
            }).await;
            let mut context = WebContext {
                base_url: address,
                ..WebContext::new()
            };
            let mut request = request();
            request.generation_mode = "web".into();
            let client = reqwest::Client::builder()
                .cookie_store(true)
                .build()
                .unwrap();
            let result = execute_web(
                &client,
                &json!({"accessToken":"test-token"}),
                &request,
                "local-task",
                &mut context,
            )
            .await
            .unwrap();
            assert_eq!(result.images.len(), 1);
            assert_eq!(result.images[0].width, 2);
            assert!(result.error["message"]
                .as_str()
                .unwrap()
                .contains("部分图片下载失败"));
            assert_eq!(result.usage["web_remaining"], 1);
            assert_eq!(result.usage["web_conversations"], json!(["conversation-1"]));
            assert_eq!(
                result.usage["web_sentinel_diagnostics"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(handle.join().unwrap().len(), 14);
        });
    }

    #[test]
    #[ignore = "需显式指定本机账号，仅核验 Sentinel，不提交图片会话"]
    fn web_live_sentinel_preparation() {
        tauri::async_runtime::block_on(async {
            let data_root =
                std::env::var("AI_MANAGER_WEB_DIAGNOSTIC_DATA_ROOT").expect("缺少诊断数据目录");
            let account_id =
                std::env::var("AI_MANAGER_WEB_DIAGNOSTIC_ACCOUNT_ID").expect("缺少诊断账号 ID");
            let paths = crate::core::paths::resolve_app_paths(std::path::Path::new(&data_root));
            // 只读已有凭据，不切换账号、不写数据库、不执行 token 续期。
            let connection = rusqlite::Connection::open_with_flags(
                &paths.storage_files.database,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            let raw: String = connection.query_row("SELECT payload_json FROM codex_accounts WHERE json_extract(payload_json, '$.id') = ?1", [account_id], |row| row.get(0)).unwrap();
            let account: Value = serde_json::from_str(&raw).unwrap();
            let auth = json!({
                "accessToken": account["auth"]["accessToken"].as_str().or_else(|| account["access_token"].as_str()).unwrap(),
                "accountId": account["account_id"].as_str().or_else(|| account["accountId"].as_str()).unwrap_or("")
            });
            let mut builder = reqwest::Client::builder()
                .cookie_store(true)
                .user_agent(WEB_USER_AGENT);
            if let Some(proxy) = account["proxy"].as_str().filter(|proxy| !proxy.is_empty()) {
                builder = builder.proxy(reqwest::Proxy::all(proxy).unwrap());
            }
            let client = builder.build().unwrap();
            let mut context = WebContext::new();
            web_bootstrap(&client, &auth, &mut context).await.unwrap();
            let requirements = web_requirements(&client, &auth, &context).await.unwrap();
            assert!(!requirements.token.is_empty());
            eprintln!(
                "Sentinel prepare/finalize 已通过；本地 dx 诊断存在：{}；未提交图片会话",
                requirements.turnstile_diagnostic.is_some()
            );
        });
    }

    async fn mock_http(
        statuses: Vec<(u16, String, String)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        mock_http_responder(statuses.len(), move |index, _, _| {
            let (status, content_type, body) = &statuses[index];
            (*status, content_type.clone(), body.as_bytes().to_vec())
        })
        .await
    }

    async fn mock_http_responder(
        count: usize,
        mut respond: impl FnMut(usize, &str, &str) -> (u16, String, Vec<u8>) + Send + 'static,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let server_address = address.clone();
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..count {
                let (mut socket, _) = listener.accept().unwrap();
                let mut raw = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let read = socket.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&raw);
                    if let Some((headers, content)) = text.split_once("\r\n\r\n") {
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .map(|value| value.parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if content.len() >= length {
                            break;
                        }
                    }
                }
                let raw = String::from_utf8_lossy(&raw).into_owned();
                let (status, content_type, body) = respond(index, &raw, &server_address);
                requests.push(raw);
                write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                socket.write_all(&body).unwrap();
            }
            requests
        });
        (address, handle)
    }

    #[test]
    fn image_http_fallback_only_on_404_and_405() {
        tauri::async_runtime::block_on(async {
            for status in [404, 405] {
                let body = String::from_utf8(events(&[json!({ "type": "response.completed", "response": { "output": [{ "type": "image_generation_call", "result": png() }] } })])).unwrap();
                let (address, handle) = mock_http(vec![
                    (status, "application/json".into(), "{}".into()),
                    (200, "text/event-stream".into(), body),
                ])
                .await;
                let result = execute(
                    &reqwest::Client::new(),
                    &json!({ "accessToken": "test-token", "accountId": "test-account" }),
                    &request(),
                    "test-session",
                    &address,
                )
                .await
                .unwrap();
                assert_eq!(result.images.len(), 1);
                let requests = handle.join().unwrap();
                assert!(requests[0].starts_with("POST /images/generations"));
                assert!(requests[1].starts_with("POST /responses"));
                assert!(
                    requests[1]
                        .to_ascii_lowercase()
                        .contains("chatgpt-account-id: test-account"),
                    "{}",
                    requests[1]
                );
                assert!(requests[1]
                    .to_ascii_lowercase()
                    .contains("openai-beta: responses=experimental"));
            }
            for status in [400, 401, 403, 429, 500] {
                let (address, handle) =
                    mock_http(vec![(status, "application/json".into(), "{}".into())]).await;
                let result = execute(
                    &reqwest::Client::new(),
                    &json!({}),
                    &request(),
                    "test-session",
                    &address,
                )
                .await
                .unwrap();
                assert!(result.images.is_empty());
                assert_eq!(result.error["httpStatus"], status);
                assert_eq!(handle.join().unwrap().len(), 1);
            }
        });
    }
}

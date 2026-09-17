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
use std::{
    collections::HashSet,
    io::{Cursor, Write},
};
use tauri::Emitter;

const DIRECT_MODELS: &[&str] = &[
    "gpt-image-1.5",
    "gpt-image-2",
    "gpt-image-2.5-flare",
    "gpt-image-2.5-sunburst",
    "gpt-image-2.5-flare-2026-09-08",
    "gpt-image-2.5-sunburst-2026-09-08",
];
const MAX_RESPONSE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageRequest {
    account_id: String,
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
        if !matches!(self.mode.as_str(), "generate" | "edit")
            || !self.model.starts_with("gpt-image-")
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

pub fn accounts(paths: &AppPaths) -> Result<Value, ManagerError> {
    let accounts = codex_account::read_public_accounts(paths)?;
    Ok(json!(accounts.as_array().unwrap().iter().filter(|account| account["type"] == "codex").map(|account| json!({
        "id": account["id"], "email": account["email"], "plan": account["plan"],
        "active": account["active"], "disabled": account["disabled"], "requiresReauth": account["requires_reauth"]
    })).collect::<Vec<_>>()))
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
    let account = provider_store::read_codex_accounts(paths)?
        .into_iter()
        .find(|account| account["id"] == request.account_id && account["type"] == "codex")
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
    let auth = codex_account::get_proxy_auth(paths, &request.account_id, cli_target).await?;
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("codex_cli_rs/0.146.0 (Windows; image-workbench)");
    let proxy = account["proxy"].as_str().unwrap_or("").trim();
    if !proxy.is_empty() {
        builder =
            builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| failure("官方账号代理配置无效"))?);
    }
    let client = builder
        .build()
        .map_err(|_| failure("无法创建图片请求客户端"))?;
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
        let result = execute(
            &client,
            &auth,
            &request,
            task["id"].as_str().unwrap(),
            "https://chatgpt.com/backend-api/codex",
        )
        .await;
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
        let _ = app.emit("images:changed", json!({ "taskId": task["id"] }));
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

    fn request() -> ImageRequest {
        serde_json::from_value(
            json!({ "accountId": "official", "mode": "generate", "prompt": "  画一个苹果  ",
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
        let task = json!({ "id": "one", "createdAt": 1, "status": "processing", "request": { "responseFormat": "url" } });
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
        let task2 = json!({ "id": "two", "createdAt": 2, "status": "processing" });
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
        assert!(detail["data"][0]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        image_store::initialize(&paths).unwrap();
        assert_eq!(
            image_store::detail(&paths, "two").unwrap()["status"],
            "interrupted"
        );
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

    async fn mock_http(
        statuses: Vec<(u16, String, String)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, content_type, body) in statuses {
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
                requests.push(String::from_utf8(raw).unwrap());
                write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
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

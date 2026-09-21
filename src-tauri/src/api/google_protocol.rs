use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone)]
struct Tool {
    name: String,
    namespace: Option<String>,
    google_name: String,
    custom: bool,
    declaration: Value,
}

fn tool_name(name: &str, namespace: Option<&str>) -> String {
    if namespace.is_none()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return name.to_owned();
    }
    use sha2::{Digest, Sha256};
    format!(
        "tool_{:x}",
        Sha256::digest(format!("{}:{name}", namespace.unwrap_or("")))
    )[..64]
        .to_string()
}

fn tools(body: &Value) -> Result<Vec<Tool>, String> {
    fn collect(
        items: &[Value],
        namespace: Option<&str>,
        result: &mut Vec<Tool>,
    ) -> Result<(), String> {
        for item in items {
            let kind = item["type"].as_str().unwrap_or("");
            if kind == "namespace" {
                let name = item["name"].as_str().ok_or("工具命名空间缺少 name")?;
                let full = namespace
                    .map(|prefix| format!("{prefix}.{name}"))
                    .unwrap_or_else(|| name.to_owned());
                collect(
                    item["tools"].as_array().ok_or("命名空间缺少 tools")?,
                    Some(&full),
                    result,
                )?;
                continue;
            }
            if matches!(kind, "web_search" | "web_search_preview") {
                continue;
            }
            if !matches!(kind, "function" | "custom") {
                return Err(format!(
                    "Google 转发暂不支持内置工具 {kind}，请关闭此工具后重试"
                ));
            }
            let name = item["name"]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or("工具缺少 name")?;
            let google_name = tool_name(name, namespace);
            if result.iter().any(|tool| tool.google_name == google_name) {
                return Err(format!("重复的工具名称：{name}"));
            }
            let custom = kind == "custom";
            let schema = if custom {
                json!({"type": "object", "properties": {"input": {"type": "string"}}, "required": ["input"]})
            } else {
                item.get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}}))
            };
            let mut description = item["description"].as_str().unwrap_or("").to_string();
            if custom && !item["format"].is_null() {
                description.push_str(&format!("\n工具输入格式：{}", item["format"]));
            }
            result.push(Tool { name: name.to_owned(), namespace: namespace.map(str::to_owned), google_name: google_name.clone(), custom,
                declaration: json!({"name": google_name, "description": description, "parametersJsonSchema": schema}) });
        }
        Ok(())
    }
    let mut result = Vec::new();
    collect(
        body["tools"].as_array().map(Vec::as_slice).unwrap_or(&[]),
        None,
        &mut result,
    )?;
    Ok(result)
}

// call_id 随 Codex 历史往返，保留 Google 工具调用的签名，重启转发服务后也能继续会话。
fn call_metadata(call_id: &str) -> Value {
    call_id
        .strip_prefix("call_google_")
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .and_then(|value| serde_json::from_slice(&value).ok())
        .unwrap_or(Value::Null)
}

pub(crate) fn to_google(body: &Value, project: &str) -> Result<Value, String> {
    if body
        .get("previous_response_id")
        .is_some_and(|value| !value.is_null())
    {
        return Err("Google 转发不保存远程会话，请发送完整 input 历史".into());
    }
    let mut system = Vec::new();
    if let Some(text) = body["instructions"]
        .as_str()
        .filter(|text| !text.is_empty())
    {
        system.push(json!({"text": text}));
    }
    let input = match &body["input"] {
        Value::String(text) => vec![json!({"role": "user", "content": text})],
        Value::Array(items) => items.clone(),
        _ => return Err("Responses input 必须是文本或消息数组".into()),
    };
    let mut names = HashMap::new();
    // 先建立整轮调用关系，工具结果解析不依赖遍历时才发现名称。
    for item in &input {
        if matches!(
            item["type"].as_str(),
            Some("function_call" | "custom_tool_call" | "local_shell_call")
        ) {
            let name = if item["type"] == "local_shell_call" {
                "shell"
            } else {
                item["name"].as_str().ok_or("工具调用缺少 name")?
            };
            names.insert(
                item["call_id"].as_str().unwrap_or("").to_owned(),
                tool_name(name, item["namespace"].as_str()),
            );
        }
    }
    let mut contents: Vec<Value> = Vec::new();
    for item in &input {
        let kind = item["type"].as_str().unwrap_or("message");
        let (role, parts) = match kind {
            "reasoning" => {
                let Some(encoded) = item["encrypted_content"]
                    .as_str()
                    .and_then(|v| v.strip_prefix("google_reasoning_"))
                else {
                    continue;
                };
                let decoded = crate::api::runtime_provider::decrypt_provider_key(encoded)
                    .map_err(|_| "Google 思考签名无法解密，请发送完整历史或重新开始会话")?;
                let parts: Vec<Value> =
                    serde_json::from_str(&decoded).map_err(|_| "Google 思考签名格式无效")?;
                ("model", parts)
            }
            "function_call" | "custom_tool_call" | "local_shell_call" => {
                let call_id = item["call_id"].as_str().unwrap_or("");
                let name = names.get(call_id).ok_or("工具调用缺少名称")?;
                let args = if kind == "local_shell_call" {
                    let mut action = item["action"].clone();
                    if let Some(action) = action.as_object_mut() {
                        action.remove("type");
                    }
                    action
                } else if kind == "custom_tool_call" {
                    json!({"input": item["input"].as_str().unwrap_or("")})
                } else {
                    serde_json::from_str::<Value>(item["arguments"].as_str().unwrap_or("{}"))
                        .map_err(|_| "工具调用 arguments 不是有效 JSON")?
                };
                if !args.is_object() {
                    return Err("工具调用参数必须是 JSON 对象".into());
                }
                let mut part = json!({"functionCall": {"name": name, "args": args}});
                let metadata = call_metadata(call_id);
                if let Some(signature) = metadata["signature"].as_str() {
                    part["thoughtSignature"] = json!(signature);
                }
                if let Some(id) = metadata["id"].as_str() {
                    part["functionCall"]["id"] = json!(id);
                }
                ("model", vec![part])
            }
            "function_call_output" | "custom_tool_call_output" | "local_shell_call_output" => {
                let id = item["call_id"].as_str().unwrap_or("");
                let metadata = call_metadata(id);
                let name = names
                    .get(id)
                    .map(String::as_str)
                    .or_else(|| metadata["name"].as_str())
                    .ok_or("工具结果缺少对应调用，请发送完整工具调用历史")?;
                let output = item.get("output").cloned().unwrap_or(Value::Null);
                let mut part =
                    json!({"functionResponse": {"name": name, "response": {"output": output}}});
                if let Some(id) = metadata["id"].as_str() {
                    part["functionResponse"]["id"] = json!(id);
                }
                ("user", vec![part])
            }
            "message" => {
                let role = item["role"].as_str().unwrap_or("user");
                let parts = message_parts(&item["content"])?;
                if role == "assistant"
                    && (item["id"]
                        .as_str()
                        .is_some_and(|id| id.starts_with("msg_thought_"))
                        || parts
                            .first()
                            .and_then(|p| p["text"].as_str())
                            .is_some_and(|text| text.trim_start().starts_with("**Thinking**")))
                {
                    continue;
                }
                if matches!(role, "system" | "developer") {
                    system.extend(parts);
                    continue;
                }
                if !matches!(role, "user" | "assistant") {
                    return Err(format!("不支持的消息角色：{role}"));
                }
                (if role == "assistant" { "model" } else { "user" }, parts)
            }
            _ => return Err(format!("Google 转发暂不支持 input 类型：{kind}")),
        };
        if parts.is_empty() {
            continue;
        }
        if let Some(previous) = contents
            .last_mut()
            .filter(|previous| previous["role"] == role)
        {
            previous["parts"].as_array_mut().unwrap().extend(parts);
        } else {
            contents.push(json!({"role": role, "parts": parts}));
        }
    }
    if contents.is_empty() {
        return Err("请求没有可发送的消息".into());
    }
    if contents.last().is_some_and(|item| item["role"] == "model") {
        contents.push(json!({"role": "user", "parts": [{"text": "请继续。"}]}));
    }
    let mut request = json!({"contents": contents});
    if !system.is_empty() {
        request["systemInstruction"] = json!({"parts": system});
    }
    let catalog = tools(body)?;
    let declarations: Vec<_> = catalog
        .iter()
        .map(|tool| tool.declaration.clone())
        .collect();
    if matches!(
        body["tool_choice"]["type"].as_str(),
        Some("web_search" | "web_search_preview")
    ) {
        return Err("Google 账号不提供 OpenAI 内置搜索，请使用 MCP 搜索工具".into());
    }
    if body["tool_choice"] == "required" && declarations.is_empty() {
        return Err("没有可用的函数工具".into());
    }
    if !declarations.is_empty() {
        request["tools"] = json!([{"functionDeclarations": declarations}]);
        let choice = &body["tool_choice"];
        let mode = match choice.as_str() {
            Some("none") => "NONE",
            Some("required") => "ANY",
            _ if choice.is_object() => "ANY",
            _ => "AUTO",
        };
        request["toolConfig"] = json!({"functionCallingConfig": {"mode": mode}});
        if let Some(name) = choice["name"].as_str() {
            let name = tool_name(name, choice["namespace"].as_str());
            if !catalog.iter().any(|tool| tool.google_name == name) {
                return Err("tool_choice 指定的工具未声明".into());
            }
            request["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"] = json!([name]);
        }
    }
    let mut generation = json!({});
    for (source, target) in [
        ("temperature", "temperature"),
        ("top_p", "topP"),
        ("max_output_tokens", "maxOutputTokens"),
    ] {
        if let Some(value) = body.get(source).filter(|value| value.is_number()) {
            generation[target] = value.clone();
        }
    }
    if body["text"]["format"]["type"] == "json_schema" {
        generation["responseMimeType"] = json!("application/json");
        generation["responseJsonSchema"] = body["text"]["format"]["schema"].clone();
    } else if body["text"]["format"]["type"] == "json_object" {
        generation["responseMimeType"] = json!("application/json");
    }
    request["generationConfig"] = generation;
    Ok(json!({"model": body["model"], "project": project, "request": request}))
}

fn message_parts(content: &Value) -> Result<Vec<Value>, String> {
    if let Some(text) = content.as_str() {
        return Ok(if text.is_empty() {
            vec![]
        } else {
            vec![json!({"text": text})]
        });
    }
    let mut parts = Vec::new();
    for part in content.as_array().ok_or("消息 content 必须是文本或数组")? {
        match part["type"].as_str().unwrap_or("") {
            "input_text" | "output_text" | "text" => {
                if let Some(text) = part["text"].as_str().filter(|text| !text.is_empty()) {
                    parts.push(json!({"text": text}));
                }
            }
            "input_image" => {
                let url = part["image_url"].as_str().unwrap_or("");
                let (mime, data) = url
                    .strip_prefix("data:")
                    .and_then(|url| url.split_once(";base64,"))
                    .ok_or("Google 转发的图片需要使用 data:...;base64 格式")?;
                if !mime.starts_with("image/") {
                    return Err("图片 MIME 类型无效".into());
                }
                if !data.is_empty() {
                    parts.push(json!({"inlineData": {"mimeType": mime, "data": data}}));
                }
            }
            kind => return Err(format!("Google 转发暂不支持内容类型：{kind}")),
        }
    }
    Ok(parts)
}

// 同一转换器处理流式和普通响应，保证工具 ID、用量和终态一致。
pub(crate) struct ResponseMapper {
    pub response: Value,
    sequence: u64,
    text_index: Option<usize>,
    reasoning_index: Option<usize>,
    reasoning_parts: Vec<Value>,
    closed: std::collections::HashSet<usize>,
    tools: Vec<Tool>,
    received_bytes: usize,
    pub finished: bool,
}

impl ResponseMapper {
    pub fn new(body: &Value) -> Self {
        Self {
            response: json!({"id": format!("resp_{}", uuid::Uuid::new_v4().simple()), "object": "response",
                "created_at": chrono::Utc::now().timestamp(), "status": "in_progress", "model": body["model"],
                "output": [], "error": null, "incomplete_details": null, "usage": null}),
            sequence: 0,
            text_index: None,
            reasoning_index: None,
            reasoning_parts: Vec::new(),
            closed: std::collections::HashSet::new(),
            finished: false,
            tools: tools(body).unwrap_or_default(),
            received_bytes: 0,
        }
    }

    fn event(&mut self, kind: &str, mut value: Value) -> Value {
        value["type"] = json!(kind);
        value["sequence_number"] = json!(self.sequence);
        self.sequence += 1;
        value
    }

    pub fn start(&mut self) -> Vec<Value> {
        vec![
            self.event("response.created", json!({"response": self.response})),
            self.event("response.in_progress", json!({"response": self.response})),
        ]
    }

    fn close_item(&mut self, index: usize, events: &mut Vec<Value>) {
        if !self.closed.insert(index) {
            return;
        }
        self.response["output"][index]["status"] = json!("completed");
        let item = self.response["output"][index].clone();
        if item["type"] == "message" {
            events.push(self.event("response.output_text.done", json!({"item_id": item["id"], "output_index": index, "content_index": 0, "text": item["content"][0]["text"], "logprobs": []})));
            events.push(self.event("response.content_part.done", json!({"item_id": item["id"], "output_index": index, "content_index": 0, "part": item["content"][0]})));
        } else if item["type"] == "reasoning" {
            events.push(self.event("response.reasoning_summary_text.done", json!({"item_id": item["id"], "output_index": index, "summary_index": 0, "text": item["summary"][0]["text"]})));
            events.push(self.event("response.reasoning_summary_part.done", json!({"item_id": item["id"], "output_index": index, "summary_index": 0, "part": item["summary"][0]})));
        }
        events.push(self.event(
            "response.output_item.done",
            json!({"output_index": index, "item": item}),
        ));
    }

    fn close_text(&mut self, events: &mut Vec<Value>, phase: &str) {
        if let Some(index) = self.text_index.take() {
            self.response["output"][index]["phase"] = json!(phase);
            self.close_item(index, events);
        }
    }

    fn close_reasoning(&mut self, events: &mut Vec<Value>) -> Result<(), String> {
        if let Some(index) = self.reasoning_index.take() {
            let payload = crate::api::runtime_provider::encrypt_provider_key(
                &serde_json::to_string(&self.reasoning_parts).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            self.response["output"][index]["encrypted_content"] =
                json!(format!("google_reasoning_{payload}"));
            self.reasoning_parts.clear();
            self.close_item(index, events);
        }
        Ok(())
    }

    pub fn chunk(&mut self, value: &Value) -> Result<Vec<Value>, String> {
        self.received_bytes += value.to_string().len();
        if self.received_bytes > 32 * 1024 * 1024 {
            return Err("Google 响应超过 32 MiB，请缩小请求后重试".into());
        }
        let value = value.get("response").unwrap_or(value);
        if let Some(error) = value.get("error") {
            return Err(format!("Google 返回错误：{error}"));
        }
        if value["promptFeedback"]["blockReason"].is_string() {
            return Err(format!(
                "Google 拒绝了请求：{}",
                value["promptFeedback"]["blockReason"]
            ));
        }
        let mut events = Vec::new();
        if let Some(usage) = value.get("usageMetadata") {
            let input = usage["promptTokenCount"].as_u64().unwrap_or(0);
            let reasoning = usage["thoughtsTokenCount"].as_u64().unwrap_or(0);
            let output = usage["candidatesTokenCount"].as_u64().unwrap_or(0) + reasoning;
            self.response["usage"] = json!({"input_tokens": input, "output_tokens": output, "total_tokens": input + output,
                "input_tokens_details": {"cached_tokens": usage["cachedContentTokenCount"].as_u64().unwrap_or(0)},
                "output_tokens_details": {"reasoning_tokens": reasoning}});
        }
        let candidate = &value["candidates"][0];
        for part in candidate["content"]["parts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if part["thought"] == true {
                self.close_text(&mut events, "commentary");
                let index = if let Some(index) = self.reasoning_index {
                    index
                } else {
                    let index = self.response["output"].as_array().unwrap().len();
                    let item = json!({"id": format!("rs_{}", uuid::Uuid::new_v4().simple()), "type": "reasoning", "status": "in_progress", "summary": []});
                    self.response["output"]
                        .as_array_mut()
                        .unwrap()
                        .push(item.clone());
                    events.push(self.event(
                        "response.output_item.added",
                        json!({"output_index": index, "item": item}),
                    ));
                    let summary = json!({"type": "summary_text", "text": ""});
                    self.response["output"][index]["summary"] = json!([summary]);
                    events.push(self.event("response.reasoning_summary_part.added", json!({"item_id": item["id"], "output_index": index, "summary_index": 0, "part": summary})));
                    self.reasoning_index = Some(index);
                    index
                };
                self.reasoning_parts.push(part.clone());
                if let Some(text) = part["text"].as_str().filter(|text| !text.is_empty()) {
                    let next = format!(
                        "{}{text}",
                        self.response["output"][index]["summary"][0]["text"]
                            .as_str()
                            .unwrap_or("")
                    );
                    self.response["output"][index]["summary"][0]["text"] = json!(next);
                    events.push(self.event("response.reasoning_summary_text.delta", json!({"item_id": self.response["output"][index]["id"], "output_index": index, "summary_index": 0, "delta": text})));
                }
                continue;
            }
            if part["text"].is_string() || part.get("functionCall").is_some() {
                self.close_reasoning(&mut events)?;
            }
            if let Some(text) = part["text"].as_str().filter(|text| !text.is_empty()) {
                let index = match self.text_index {
                    Some(index) => index,
                    None => {
                        let index = self.response["output"].as_array().unwrap().len();
                        let item = json!({"id": format!("msg_{}", uuid::Uuid::new_v4().simple()), "type": "message", "role": "assistant", "status": "in_progress", "content": []});
                        self.response["output"]
                            .as_array_mut()
                            .unwrap()
                            .push(item.clone());
                        events.push(self.event(
                            "response.output_item.added",
                            json!({"output_index": index, "item": item}),
                        ));
                        let content = json!({"type": "output_text", "text": "", "annotations": [], "logprobs": []});
                        self.response["output"][index]["content"] = json!([content]);
                        events.push(self.event("response.content_part.added", json!({"item_id": item["id"], "output_index": index, "content_index": 0, "part": content})));
                        self.text_index = Some(index);
                        index
                    }
                };
                let next = format!(
                    "{}{text}",
                    self.response["output"][index]["content"][0]["text"]
                        .as_str()
                        .unwrap_or("")
                );
                self.response["output"][index]["content"][0]["text"] = json!(next);
                events.push(self.event("response.output_text.delta", json!({"item_id": self.response["output"][index]["id"], "output_index": index, "content_index": 0, "delta": text, "logprobs": []})));
            }
            if let Some(call) = part.get("functionCall") {
                self.close_text(&mut events, "commentary");
                let google_name = call["name"].as_str().ok_or("Google 工具调用缺少名称")?;
                let tool = self
                    .tools
                    .iter()
                    .find(|tool| tool.google_name == google_name)
                    .cloned()
                    .ok_or_else(|| format!("Google 返回了未声明的工具：{google_name}"))?;
                let name = &tool.name;
                let metadata = json!({"name": google_name, "signature": part["thoughtSignature"], "id": call["id"], "nonce": uuid::Uuid::new_v4().simple().to_string()});
                let call_id = format!(
                    "call_google_{}",
                    URL_SAFE_NO_PAD.encode(metadata.to_string())
                );
                let custom = tool.custom;
                let field = if custom { "input" } else { "arguments" };
                let content = if custom {
                    call["args"]["input"]
                        .as_str()
                        .ok_or("Google 自定义工具缺少 input")?
                        .to_string()
                } else {
                    if !call["args"].is_object() {
                        return Err("Google 工具参数必须是完整 JSON 对象".into());
                    }
                    call["args"].to_string()
                };
                if name == "apply_patch" {
                    let patch = if custom {
                        content.as_str()
                    } else {
                        call["args"]["input"]
                            .as_str()
                            .ok_or("apply_patch 缺少 input")?
                    };
                    validate_patch(patch)?;
                }
                let index = self.response["output"].as_array().unwrap().len();
                let mut item = json!({"id": format!("fc_{}", uuid::Uuid::new_v4().simple()), "type": if custom { "custom_tool_call" } else { "function_call" }, "status": "in_progress", "call_id": call_id, "name": name});
                if let Some(namespace) = tool.namespace {
                    item["namespace"] = json!(namespace);
                }
                item[field] = json!("");
                events.push(self.event(
                    "response.output_item.added",
                    json!({"output_index": index, "item": item}),
                ));
                let event_prefix = if custom {
                    "response.custom_tool_call_input"
                } else {
                    "response.function_call_arguments"
                };
                events.push(self.event(
                    &format!("{event_prefix}.delta"),
                    json!({"item_id": item["id"], "output_index": index, "delta": content}),
                ));
                let mut done = json!({"item_id": item["id"], "output_index": index});
                done[field] = json!(content);
                events.push(self.event(&format!("{event_prefix}.done"), done));
                item[field] = json!(content);
                item["status"] = json!("completed");
                self.response["output"].as_array_mut().unwrap().push(item);
                self.close_item(index, &mut events);
            }
        }
        if let Some(reason) = candidate["finishReason"].as_str() {
            self.finished = true;
            if reason == "MAX_TOKENS" {
                self.response["incomplete_details"] = json!({"reason": "max_output_tokens"});
            } else if reason != "STOP" {
                return Err(format!("Google 未完成生成：{reason}"));
            }
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<Value>, String> {
        if !self.finished {
            return Err("Google 响应在完成标记前中断，请重试".into());
        }
        let mut events = Vec::new();
        self.close_reasoning(&mut events)?;
        self.close_text(&mut events, "final_answer");
        let incomplete = !self.response["incomplete_details"].is_null();
        self.response["status"] = json!(if incomplete {
            "incomplete"
        } else {
            "completed"
        });
        events.push(self.event(
            if incomplete {
                "response.incomplete"
            } else {
                "response.completed"
            },
            json!({"response": self.response}),
        ));
        Ok(events)
    }

    pub fn fail(&mut self, message: &str) -> Value {
        self.response["status"] = json!("failed");
        self.response["error"] = json!({"code": "google_upstream_error", "message": message});
        self.event("response.failed", json!({"response": self.response}))
    }
}

// 只校验工具输入，不读工作目录、不重写补丁；文件匹配与执行权限仍由 Codex 检查。
fn validate_patch(patch: &str) -> Result<(), String> {
    let invalid = || {
        "Google 返回的 apply_patch 格式无效，未下发工具调用，请让模型重新生成完整 V4A 补丁"
            .to_string()
    };
    let lines: Vec<_> = patch.trim().lines().collect();
    if lines.first() != Some(&"*** Begin Patch")
        || lines.last() != Some(&"*** End Patch")
        || lines.len() < 3
    {
        return Err(invalid());
    }
    let mut index = 1;
    while index < lines.len() - 1 {
        let header = lines[index];
        index += 1;
        if let Some(path) = header.strip_prefix("*** Delete File: ") {
            if path.trim().is_empty() {
                return Err(invalid());
            }
        } else if let Some(path) = header.strip_prefix("*** Add File: ") {
            if path.trim().is_empty() {
                return Err(invalid());
            }
            let start = index;
            while index < lines.len() - 1 && !lines[index].starts_with("*** ") {
                if !lines[index].starts_with('+') {
                    return Err(invalid());
                }
                index += 1;
            }
            if index == start {
                return Err(invalid());
            }
        } else if let Some(path) = header.strip_prefix("*** Update File: ") {
            if path.trim().is_empty() {
                return Err(invalid());
            }
            if lines[index].starts_with("*** Move to: ") {
                if lines[index]
                    .trim_start_matches("*** Move to: ")
                    .trim()
                    .is_empty()
                {
                    return Err(invalid());
                }
                index += 1;
            }
            let mut hunk_lines = 0;
            let mut previous_header = false;
            while index < lines.len() - 1 {
                let line = lines[index];
                if line == "*** End of File" {
                    if hunk_lines == 0 || previous_header {
                        return Err(invalid());
                    }
                    index += 1;
                    break;
                }
                if line.starts_with("*** ") {
                    break;
                }
                if line == "@@" || line.starts_with("@@ ") {
                    if previous_header {
                        return Err(invalid());
                    }
                    previous_header = true;
                } else if line.is_empty() || line.starts_with([' ', '+', '-']) {
                    hunk_lines += 1;
                    previous_header = false;
                } else {
                    return Err(invalid());
                }
                index += 1;
            }
            if hunk_lines == 0 || previous_header {
                return Err(invalid());
            }
        } else {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(crate) fn sse(event: &Value) -> String {
    format!(
        "event: {}\ndata: {}\n\n",
        event["type"].as_str().unwrap_or("error"),
        event
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_reasoning_and_tools_close_before_the_next_output_item() {
        let body = json!({"model": "gemini", "tools": [{"type": "function", "name": "read_file"}]});
        let mut mapper = ResponseMapper::new(&body);
        let mut events = mapper.start();
        for part in [
            json!({"thought": true, "text": "检查代码", "thoughtSignature": "signature"}),
            json!({"text": "开始读取"}),
            json!({"functionCall": {"name": "read_file", "args": {"path": "a.rs"}}}),
        ] {
            events.extend(
                mapper
                    .chunk(&json!({"candidates": [{"content": {"parts": [part]}}]}))
                    .unwrap(),
            );
        }
        events.extend(
            mapper
                .chunk(&json!({"candidates": [{"finishReason": "STOP"}]}))
                .unwrap(),
        );
        events.extend(mapper.finish().unwrap());
        let mut active = None;
        for event in &events {
            if event["type"] == "response.output_item.added" {
                assert!(active.is_none());
                active = Some(event["output_index"].clone());
            }
            if event["type"] == "response.output_item.done" {
                assert_eq!(active.take(), Some(event["output_index"].clone()));
            }
        }
        assert!(active.is_none());
        assert!(events
            .iter()
            .any(|event| event["type"] == "response.reasoning_summary_text.delta"));
        assert!(!mapper.response["output"][0]["encrypted_content"]
            .as_str()
            .unwrap()
            .contains("signature"));
        let mut history = mapper.response["output"].as_array().unwrap().clone();
        history.push(json!({"type": "function_call_output", "call_id": history[2]["call_id"], "output": "代码"}));
        let request = to_google(&json!({"input": history}), "project").unwrap();
        assert_eq!(
            request["request"]["contents"][0]["parts"][0]["thoughtSignature"],
            "signature"
        );
        assert_eq!(
            request["request"]["contents"][0]["parts"][1]["text"],
            "开始读取"
        );
    }

    #[test]
    fn google_namespaced_tools_round_trip_without_name_collisions() {
        let body = json!({"input": "hello", "tools": [
            {"type": "namespace", "name": "one", "tools": [{"type": "function", "name": "read"}]},
            {"type": "namespace", "name": "two", "tools": [{"type": "custom", "name": "read"}]}
        ]});
        let request = to_google(&body, "p").unwrap();
        let declarations = request["request"]["tools"][0]["functionDeclarations"]
            .as_array()
            .unwrap();
        assert_ne!(declarations[0]["name"], declarations[1]["name"]);
        let mut mapper = ResponseMapper::new(&body);
        mapper.chunk(&json!({"candidates": [{"content": {"parts": [{"functionCall": {"name": declarations[1]["name"], "args": {"input": "a.rs"}}}]}, "finishReason": "STOP"}]})).unwrap();
        mapper.finish().unwrap();
        let call = mapper.response["output"][0].clone();
        assert_eq!(call["namespace"], "two");
        assert_eq!(call["name"], "read");
        assert_eq!(call["type"], "custom_tool_call");
        let request = to_google(&json!({"input": [call.clone(), {"type": "custom_tool_call_output", "call_id": call["call_id"], "output": "ok"}]}), "p").unwrap();
        assert_eq!(
            request["request"]["contents"][0]["parts"][0]["functionCall"]["name"],
            declarations[1]["name"]
        );
        assert_eq!(
            request["request"]["contents"][1]["parts"][0]["functionResponse"]["name"],
            declarations[1]["name"]
        );
    }

    #[test]
    fn google_transcript_only_messages_are_removed_but_commentary_is_kept() {
        let request = to_google(&json!({"input": [
            {"role": "user", "content": "hello"},
            {"id": "msg_thought_private", "role": "assistant", "content": "private"},
            {"role": "assistant", "content": "**Thinking** local"},
            {"role": "assistant", "phase": "commentary", "content": "运行测试"},
            {"type": "local_shell_call", "call_id": "shell_1", "action": {"type": "exec", "command": ["pwsh", "-c", "pwd"]}},
            {"type": "local_shell_call_output", "call_id": "shell_1", "output": "D:/code"}
        ]}), "p").unwrap();
        let text = request.to_string();
        assert!(!text.contains("private"));
        assert!(!text.contains("**Thinking**"));
        assert!(text.contains("运行测试"));
        assert_eq!(
            request["request"]["contents"][2]["parts"][0]["functionResponse"]["name"],
            "shell"
        );
    }

    #[test]
    fn google_invalid_patches_and_undeclared_tools_are_not_emitted() {
        for patch in ["*** Begin Patch\n*** Add File: a\n+x\n*** End Patch",
            "*** Begin Patch\n*** Delete File: a\n*** End Patch",
            "*** Begin Patch\n*** Update File: a\n*** Move to: b\n@@\n-old\n+new\n*** End of File\n*** End Patch"] { validate_patch(patch).unwrap(); }
        for patch in [
            "*** Begin Patch\n*** End Patch",
            "*** Begin Patch\n*** Add File: a\nx\n*** End Patch",
            "*** Begin Patch\n*** Update File: a\n@@\n*** End Patch",
            "*** Begin Patch\n*** Update File: a\n@@\n+x\n@@\n*** End Patch",
        ] {
            assert!(validate_patch(patch).is_err());
        }
        let mut mapper =
            ResponseMapper::new(&json!({"tools": [{"type": "custom", "name": "apply_patch"}]}));
        assert!(mapper.chunk(&json!({"candidates": [{"content": {"parts": [{"functionCall": {"name": "apply_patch", "args": {"input": "broken"}}}]}}]})).is_err());
        assert!(mapper.response["output"].as_array().unwrap().is_empty());
        assert!(mapper.chunk(&json!({"candidates": [{"content": {"parts": [{"functionCall": {"name": "unexpected", "args": {}}}]}}]})).is_err());
    }

    #[test]
    fn google_maps_instructions_images_and_function_results() {
        let body = json!({"model": "gemini", "instructions": "系统规则", "input": [
            {"role": "developer", "content": [{"type": "input_text", "text": "开发规则"}]},
            {"role": "user", "content": [{"type": "input_text", "text": "检查图片"}, {"type": "input_image", "image_url": "data:image/png;base64,YQ=="}]},
            {"type": "function_call", "name": "read_file", "call_id": "call_1", "arguments": "{\"path\":\"a.rs\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "文件内容"}
        ], "tools": [{"type": "function", "name": "read_file", "parameters": {"type": "object"}}], "tool_choice": "required"});
        let result = to_google(&body, "project-1").unwrap();
        assert_eq!(result["project"], "project-1");
        assert_eq!(
            result["request"]["systemInstruction"]["parts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            result["request"]["contents"][0]["parts"][1]["inlineData"]["mimeType"],
            "image/png"
        );
        assert_eq!(
            result["request"]["contents"][1]["parts"][0]["functionCall"]["args"]["path"],
            "a.rs"
        );
        assert_eq!(
            result["request"]["contents"][2]["parts"][0]["functionResponse"]["name"],
            "read_file"
        );
        assert_eq!(
            result["request"]["toolConfig"]["functionCallingConfig"]["mode"],
            "ANY"
        );
    }

    #[test]
    fn google_custom_tool_signature_survives_responses_round_trip() {
        let body = json!({"model": "gemini", "input": "修改文件", "tools": [{"type": "custom", "name": "apply_patch"}]});
        let mut mapper = ResponseMapper::new(&body);
        mapper.start();
        mapper.chunk(&json!({"response": {"candidates": [{"content": {"parts": [
            {"functionCall": {"name": "apply_patch", "id": "upstream-id", "args": {"input": "*** Begin Patch\n*** Add File: a.txt\n+ok\n*** End Patch"}}, "thoughtSignature": "signed-google-content"}
        ]}, "finishReason": "STOP"}]}})).unwrap();
        let events = mapper.finish().unwrap();
        assert_eq!(events.last().unwrap()["type"], "response.completed");
        let call = mapper.response["output"][0].clone();
        assert_eq!(call["type"], "custom_tool_call");
        let continuation = json!({"model": "gemini", "input": [call.clone(), {"type": "custom_tool_call_output", "call_id": call["call_id"], "output": "完成"}]});
        let google = to_google(&continuation, "project").unwrap();
        assert_eq!(
            google["request"]["contents"][0]["parts"][0]["thoughtSignature"],
            "signed-google-content"
        );
        assert_eq!(
            google["request"]["contents"][0]["parts"][0]["functionCall"]["args"]["input"],
            "*** Begin Patch\n*** Add File: a.txt\n+ok\n*** End Patch"
        );
        assert_eq!(
            google["request"]["contents"][1]["parts"][0]["functionResponse"]["id"],
            "upstream-id"
        );
    }

    #[test]
    fn google_stream_preserves_text_usage_and_event_order() {
        let mut mapper = ResponseMapper::new(&json!({"model": "gemini"}));
        let mut events = mapper.start();
        for text in ["你好，", "世界"] {
            events.extend(
                mapper
                    .chunk(&json!({"candidates": [{"content": {"parts": [{"text": text}]}}]}))
                    .unwrap(),
            );
        }
        events.extend(mapper.chunk(&json!({"candidates": [{"finishReason": "STOP"}], "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 4, "thoughtsTokenCount": 3, "cachedContentTokenCount": 2}})).unwrap());
        events.extend(mapper.finish().unwrap());
        assert_eq!(
            mapper.response["output"][0]["content"][0]["text"],
            "你好，世界"
        );
        assert_eq!(mapper.response["usage"]["output_tokens"], 7);
        assert_eq!(mapper.response["usage"]["total_tokens"], 17);
        assert!(events
            .iter()
            .enumerate()
            .all(|(index, event)| event["sequence_number"] == index));
        assert_eq!(events.first().unwrap()["type"], "response.created");
        assert_eq!(events.last().unwrap()["type"], "response.completed");
    }

    #[test]
    fn google_never_reports_truncated_or_blocked_stream_as_complete() {
        let mut mapper = ResponseMapper::new(&json!({"model": "gemini"}));
        mapper
            .chunk(&json!({"candidates": [{"content": {"parts": [{"text": "未完成"}]}}]}))
            .unwrap();
        assert!(mapper.finish().is_err());
        assert_eq!(mapper.fail("连接中断")["type"], "response.failed");
        let mut mapper = ResponseMapper::new(&json!({"model": "gemini"}));
        mapper
            .chunk(&json!({"candidates": [{"finishReason": "MAX_TOKENS"}]}))
            .unwrap();
        assert_eq!(
            mapper.finish().unwrap().last().unwrap()["type"],
            "response.incomplete"
        );
        assert_eq!(
            mapper.response["incomplete_details"]["reason"],
            "max_output_tokens"
        );
        assert!(mapper
            .chunk(&json!({"promptFeedback": {"blockReason": "SAFETY"}}))
            .is_err());
    }

    #[test]
    fn google_default_search_declaration_does_not_block_local_tools() {
        let result = to_google(&json!({"input": "hello", "tools": [{"type": "web_search"}, {"type": "function", "name": "exec_command"}]}), "project").unwrap();
        assert_eq!(
            result["request"]["tools"][0]["functionDeclarations"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(to_google(&json!({"input": "hello", "tools": [{"type": "web_search"}], "tool_choice": {"type": "web_search"}}), "project").is_err());
    }

    #[test]
    fn google_rejects_unsupported_inputs_instead_of_silently_dropping_them() {
        for body in [
            json!({"input": "hello", "previous_response_id": "resp_old"}),
            json!({"input": "hello", "tools": [{"type": "image_generation"}]}),
            json!({"input": [{"type": "item_reference", "id": "old"}]}),
            json!({"input": [{"role": "user", "content": [{"type": "input_image", "image_url": "https://example.com/private.png"}]}]}),
            json!({"input": [{"type": "function_call_output", "call_id": "unknown", "output": "result"}]}),
        ] {
            assert!(to_google(&body, "project").is_err());
        }
    }
}

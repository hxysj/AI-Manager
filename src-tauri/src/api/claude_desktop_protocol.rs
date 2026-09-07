use super::{error, gateway_error, json_response, text, GatewayBody, ManagerError};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use hyper::{Response, StatusCode};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::pin::Pin;

fn raw<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn content_blocks(content: &Value) -> Result<Vec<Value>, ManagerError> {
    if let Some(content) = content.as_str() {
        return Ok(vec![json!({"type": "text", "text": content})]);
    }
    content
        .as_array()
        .cloned()
        .ok_or_else(|| error("消息 content 必须是字符串或内容块数组"))
}

fn image_url(block: &Value) -> Result<String, ManagerError> {
    let source = &block["source"];
    match text(source, "type") {
        "base64" => Ok(format!(
            "data:{};base64,{}",
            text(source, "media_type"),
            raw(source, "data")
        )),
        "url" => Ok(text(source, "url").to_string()),
        _ => Err(error("OpenAI 转换只支持 base64 或 URL 图片")),
    }
}

fn convert_content(
    block: &Value,
    responses: bool,
    assistant: bool,
) -> Result<Option<Value>, ManagerError> {
    Ok(match text(block, "type") {
        "text" => Some(
            json!({"type": if responses { if assistant { "output_text" } else { "input_text" } } else { "text" }, "text": raw(block, "text")}),
        ),
        "image" if !assistant => Some(if responses {
            json!({"type": "input_image", "image_url": image_url(block)?})
        } else {
            json!({"type": "image_url", "image_url": {"url": image_url(block)?}})
        }),
        "document" if !assistant && responses => {
            let source = &block["source"];
            Some(match text(source, "type") {
                "base64" => {
                    json!({"type": "input_file", "filename": "attachment.pdf", "file_data": format!("data:{};base64,{}", text(source, "media_type"), raw(source, "data"))})
                }
                "url" => json!({"type": "input_file", "file_url": text(source, "url")}),
                "text" => json!({"type": "input_text", "text": raw(source, "data")}),
                _ => return Err(error("当前文档内容类型不能转换为 Responses 输入")),
            })
        }
        "thinking" | "redacted_thinking" => None,
        _ => {
            return Err(error(&format!(
                "OpenAI 协议暂不支持此内容块：{}",
                text(block, "type")
            )))
        }
    })
}

pub(super) fn convert_request(payload: &Value, responses: bool) -> Result<Value, ManagerError> {
    let mut messages = Vec::new();
    if let Some(system) = payload.get("system") {
        let blocks = content_blocks(system)?;
        if blocks.iter().any(|block| text(block, "type") != "text") {
            return Err(error("system 只支持文本内容"));
        }
        let content = blocks
            .iter()
            .map(|block| raw(block, "text"))
            .collect::<Vec<_>>()
            .join("\n");
        if !content.is_empty() {
            messages.push(json!({"role": "system", "content": content}));
        }
    }
    let source = payload["messages"]
        .as_array()
        .ok_or_else(|| error("messages 必须为数组"))?;
    for message in source {
        let role = text(message, "role");
        if !["user", "assistant"].contains(&role) {
            return Err(error("消息 role 必须为 user 或 assistant"));
        }
        let assistant = role == "assistant";
        let mut content = Vec::new();
        let mut tools = Vec::new();
        let mut tool_outputs = Vec::new();
        for block in content_blocks(&message["content"])? {
            match text(&block, "type") {
                "tool_use" if assistant => {
                    if text(&block, "id").is_empty()
                        || text(&block, "name").is_empty()
                        || !block["input"].is_object()
                    {
                        return Err(error("工具调用缺少 id、name 或对象参数"));
                    }
                    tools.push(if responses {
                        json!({"type": "function_call", "call_id": text(&block, "id"), "name": text(&block, "name"), "arguments": block["input"].to_string()})
                    } else { json!({"id": text(&block, "id"), "type": "function", "function": {"name": text(&block, "name"), "arguments": block["input"].to_string()}}) });
                }
                "tool_result" if !assistant => {
                    if text(&block, "tool_use_id").is_empty() {
                        return Err(error("工具结果缺少 tool_use_id"));
                    }
                    let mut output = Vec::new();
                    if block["is_error"] == true {
                        output.push("工具执行失败".to_string());
                    }
                    for result in content_blocks(block.get("content").unwrap_or(&json!("")))? {
                        if text(&result, "type") == "text" {
                            output.push(raw(&result, "text").to_string());
                        } else if let Some(part) = convert_content(&result, responses, false)? {
                            content.push(part);
                        }
                    }
                    tool_outputs.push(if responses {
                        json!({"type": "function_call_output", "call_id": text(&block, "tool_use_id"), "output": output.join("\n")})
                    } else { json!({"role": "tool", "tool_call_id": text(&block, "tool_use_id"), "content": output.join("\n")}) });
                }
                _ => {
                    if let Some(part) = convert_content(&block, responses, assistant)? {
                        content.push(part);
                    }
                }
            }
        }
        messages.extend(tool_outputs);
        if responses {
            if !content.is_empty() {
                messages.push(json!({"role": role, "content": content}));
            }
            messages.extend(tools);
        } else if !content.is_empty() || !tools.is_empty() {
            let mut message = json!({"role": role, "content": if content.is_empty() { Value::Null } else { json!(content) }});
            if !tools.is_empty() {
                message["tool_calls"] = json!(tools);
            }
            messages.push(message);
        }
    }
    let mut request = json!({"model": payload["model"], "stream": payload["stream"].as_bool().unwrap_or(false), "store": false});
    request[if responses { "input" } else { "messages" }] = json!(messages);
    for (source, target) in [
        (
            "max_tokens",
            if responses {
                "max_output_tokens"
            } else {
                "max_completion_tokens"
            },
        ),
        ("temperature", "temperature"),
        ("top_p", "top_p"),
    ] {
        if let Some(value) = payload.get(source) {
            request[target] = value.clone();
        }
    }
    if !responses && request["stream"] == true {
        request["stream_options"] = json!({"include_usage": true});
    }
    if let Some(stop) = payload
        .get("stop_sequences")
        .filter(|stop| stop.as_array().is_some_and(|stop| !stop.is_empty()))
    {
        if responses {
            return Err(error(
                "Responses 上游不支持 stop_sequences，请移除自定义停止词",
            ));
        }
        request["stop"] = stop.clone();
    }
    if ["enabled", "adaptive"].contains(&text(&payload["thinking"], "type")) {
        let effort = match payload["thinking"]["budget_tokens"].as_u64() {
            Some(budget) if budget < 4096 => "low",
            Some(budget) if budget < 16384 => "medium",
            _ => "high",
        };
        if responses {
            request["reasoning"] = json!({"effort": effort});
        } else {
            request["reasoning_effort"] = json!(effort);
        }
    }
    if let Some(tools) = payload.get("tools") {
        let mut converted = Vec::new();
        for tool in tools.as_array().ok_or_else(|| error("tools 必须为数组"))? {
            if text(tool, "name").is_empty() || !tool["input_schema"].is_object() {
                return Err(error(
                    "OpenAI 转换支持自定义函数工具，不支持 Anthropic 内置服务端工具",
                ));
            }
            let function = json!({"name": text(tool, "name"), "description": text(tool, "description"), "parameters": tool["input_schema"], "strict": false});
            converted.push(if responses {
                let mut function = function;
                function["type"] = json!("function");
                function
            } else {
                json!({"type": "function", "function": function})
            });
        }
        request["tools"] = json!(converted);
    }
    if let Some(choice) = payload.get("tool_choice") {
        request["tool_choice"] = match text(choice, "type") {
            "auto" => json!("auto"),
            "any" => json!("required"),
            "none" => json!("none"),
            "tool" if !text(choice, "name").is_empty() => {
                if responses {
                    json!({"type": "function", "name": text(choice, "name")})
                } else {
                    json!({"type": "function", "function": {"name": text(choice, "name")}})
                }
            }
            _ => return Err(error("不支持的工具选择方式")),
        };
        if let Some(disable) = choice["disable_parallel_tool_use"].as_bool() {
            request["parallel_tool_calls"] = json!(!disable);
        }
    }
    Ok(request)
}

fn usage(value: &Value, responses: bool) -> Value {
    let input = value[if responses {
        "input_tokens"
    } else {
        "prompt_tokens"
    }]
    .as_u64()
    .unwrap_or(0);
    let cached = value[if responses {
        "input_tokens_details"
    } else {
        "prompt_tokens_details"
    }]["cached_tokens"]
        .as_u64()
        .unwrap_or(0);
    json!({"input_tokens": input.saturating_sub(cached), "output_tokens": value[if responses { "output_tokens" } else { "completion_tokens" }].as_u64().unwrap_or(0), "cache_read_input_tokens": cached, "cache_creation_input_tokens": 0})
}

fn tool_block(id: &str, name: &str, arguments: &str) -> Result<Value, ManagerError> {
    if id.is_empty() || name.is_empty() {
        return Err(error("上游工具调用缺少标识或名称"));
    }
    let input: Value = serde_json::from_str(if arguments.is_empty() {
        "{}"
    } else {
        arguments
    })?;
    if !input.is_object() {
        return Err(error("上游工具参数不是 JSON 对象"));
    }
    Ok(json!({"type": "tool_use", "id": id, "name": name, "input": input}))
}

pub(super) fn convert_response(
    payload: &Value,
    responses: bool,
    model: &str,
) -> Result<Value, ManagerError> {
    if payload.get("error").is_some_and(|value| !value.is_null()) {
        return Err(error("上游返回错误响应"));
    }
    let mut content = Vec::new();
    let mut stop = "end_turn";
    if responses {
        if !["completed", "incomplete"].contains(&text(payload, "status")) {
            return Err(error("Responses 未正常完成"));
        }
        for item in payload["output"]
            .as_array()
            .ok_or_else(|| error("Responses 缺少 output"))?
        {
            match text(item, "type") {
                "message" => {
                    for block in item["content"]
                        .as_array()
                        .ok_or_else(|| error("Responses 消息缺少 content"))?
                    {
                        if ["output_text", "refusal"].contains(&text(block, "type")) {
                            content.push(json!({"type": "text", "text": if text(block, "type") == "refusal" { raw(block, "refusal") } else { raw(block, "text") }}));
                        }
                    }
                }
                "function_call" => content.push(tool_block(
                    text(item, "call_id"),
                    text(item, "name"),
                    raw(item, "arguments"),
                )?),
                "reasoning" => {}
                _ => return Err(error("Responses 返回了不支持的输出类型")),
            }
        }
        if payload["incomplete_details"]["reason"] == "max_output_tokens" {
            stop = "max_tokens";
        }
    } else {
        let choice = payload["choices"]
            .as_array()
            .and_then(|choices| choices.first())
            .ok_or_else(|| error("Chat Completions 缺少 choices"))?;
        let message = &choice["message"];
        if let Some(value) = message["content"].as_str() {
            if !value.is_empty() {
                content.push(json!({"type": "text", "text": value}));
            }
        }
        if let Some(value) = message["refusal"].as_str() {
            if !value.is_empty() {
                content.push(json!({"type": "text", "text": value}));
            }
        }
        if let Some(tools) = message["tool_calls"].as_array() {
            for tool in tools {
                content.push(tool_block(
                    text(tool, "id"),
                    text(&tool["function"], "name"),
                    raw(&tool["function"], "arguments"),
                )?);
            }
        }
        if choice["finish_reason"] == "length" {
            stop = "max_tokens";
        }
    }
    if stop != "max_tokens" && content.iter().any(|block| block["type"] == "tool_use") {
        stop = "tool_use";
    }
    Ok(
        json!({"id": format!("msg_{}", uuid::Uuid::new_v4().simple()), "type": "message", "role": "assistant", "model": model,
        "content": content, "stop_reason": stop, "stop_sequence": null, "usage": usage(&payload["usage"], responses)}),
    )
}

pub(super) async fn forward(
    provider: &Value,
    key: &str,
    payload: Value,
    model: &str,
) -> Result<Response<GatewayBody>, ManagerError> {
    if key.is_empty() {
        return Ok(gateway_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "供应商 API Key 缺失",
        ));
    }
    let responses = text(provider, "apiFormat") == "openai_responses";
    let request = match convert_request(&payload, responses) {
        Ok(request) => request,
        Err(cause) => return Ok(gateway_error(StatusCode::BAD_REQUEST, &cause.to_string())),
    };
    let endpoint = if responses {
        "/responses"
    } else {
        "/chat/completions"
    };
    let base = text(provider, "baseUrl").trim_end_matches('/');
    let upstream_url = if base.ends_with(endpoint) {
        base.to_string()
    } else {
        format!(
            "{base}{}{endpoint}",
            if base.ends_with("/v1") { "" } else { "/v1" }
        )
    };
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(15));
    if !text(provider, "proxy").is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(text(provider, "proxy"))
                .map_err(|cause| error(&cause.to_string()))?,
        );
    }
    let mut outgoing = builder
        .build()
        .map_err(|cause| error(&cause.to_string()))?
        .post(upstream_url)
        .bearer_auth(key)
        .header("accept-encoding", "identity")
        .json(&request);
    if let Some(headers) = provider["headers"].as_object() {
        for (name, value) in headers {
            if ![
                "authorization",
                "x-api-key",
                "host",
                "content-length",
                "connection",
                "transfer-encoding",
                "proxy-authorization",
                "accept-encoding",
            ]
            .contains(&name.to_lowercase().as_str())
            {
                outgoing = outgoing.header(name, value.as_str().unwrap_or(""));
            }
        }
    }
    let upstream = outgoing
        .send()
        .await
        .map_err(|cause| error(&cause.to_string()))?;
    if !upstream.status().is_success() {
        let status = upstream.status();
        return Ok(gateway_error(
            status,
            &format!(
                "OpenAI 上游返回 HTTP {}，请检查协议、模型、密钥和请求参数",
                status.as_u16()
            ),
        ));
    }
    if request["stream"] == true {
        if !upstream
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("text/event-stream"))
        {
            return Ok(gateway_error(
                StatusCode::BAD_GATEWAY,
                "OpenAI 上游没有返回 SSE 流",
            ));
        }
        return Ok(stream_response(upstream, responses, model));
    }
    let body: Value = upstream
        .json()
        .await
        .map_err(|cause| error(&cause.to_string()))?;
    match convert_response(&body, responses, model) {
        Ok(message) => Ok(json_response(StatusCode::OK, message)),
        Err(cause) => Ok(gateway_error(StatusCode::BAD_GATEWAY, &cause.to_string())),
    }
}

#[derive(Default)]
struct ToolStream {
    id: String,
    name: String,
    arguments: String,
    block: Option<usize>,
}

struct StreamConverter {
    responses: bool,
    finished: bool,
    finish_reason: Option<String>,
    blocks: usize,
    text_blocks: BTreeMap<String, usize>,
    tools: BTreeMap<u64, ToolStream>,
    usage: Value,
    events: VecDeque<Bytes>,
}

impl StreamConverter {
    fn new(responses: bool, model: &str) -> Self {
        let mut converter = Self {
            responses,
            finished: false,
            finish_reason: None,
            blocks: 0,
            text_blocks: BTreeMap::new(),
            tools: BTreeMap::new(),
            usage: usage(&json!({}), responses),
            events: VecDeque::new(),
        };
        converter.emit(json!({"type": "message_start", "message": {"id": format!("msg_{}", uuid::Uuid::new_v4().simple()), "type": "message", "role": "assistant", "model": model, "content": [], "stop_reason": null, "stop_sequence": null, "usage": converter.usage}}));
        converter
    }

    fn emit(&mut self, event: Value) {
        self.events.push_back(Bytes::from(format!(
            "event: {}\ndata: {}\n\n",
            text(&event, "type"),
            event
        )));
    }

    fn fail(&mut self, message: &str) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.emit(json!({"type": "error", "error": {"type": "api_error", "message": message}}));
    }

    fn text_delta(&mut self, key: String, delta: &str) {
        if delta.is_empty() {
            return;
        }
        let index = match self.text_blocks.get(&key) {
            Some(index) => *index,
            None => {
                let index = self.blocks;
                self.blocks += 1;
                self.text_blocks.insert(key, index);
                self.emit(json!({"type": "content_block_start", "index": index, "content_block": {"type": "text", "text": ""}}));
                index
            }
        };
        self.emit(json!({"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": delta}}));
    }

    fn tool_delta(
        &mut self,
        key: u64,
        id: &str,
        name: &str,
        arguments: &str,
        complete: bool,
    ) -> Result<(), ManagerError> {
        let tool = self.tools.entry(key).or_default();
        if !id.is_empty() && tool.id.is_empty() {
            tool.id = id.to_string();
        }
        if !name.is_empty() && tool.block.is_none() {
            tool.name.push_str(name);
        }
        if tool.block.is_none() && (!arguments.is_empty() || complete) {
            if tool.id.is_empty() || tool.name.is_empty() {
                return Err(error("上游流式工具缺少调用标识或名称"));
            }
            let index = self.blocks;
            self.blocks += 1;
            tool.block = Some(index);
            let event = json!({"type": "content_block_start", "index": index, "content_block": {"type": "tool_use", "id": tool.id, "name": tool.name, "input": {}}});
            self.emit(event);
        }
        let tool = self.tools.get_mut(&key).unwrap();
        if !arguments.is_empty() {
            tool.arguments.push_str(arguments);
            if tool.arguments.len() > 8 * 1024 * 1024 {
                return Err(error("上游工具参数超过 8 MiB"));
            }
            let event = json!({"type": "content_block_delta", "index": tool.block, "delta": {"type": "input_json_delta", "partial_json": arguments}});
            self.emit(event);
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ManagerError> {
        if self.finished {
            return Ok(());
        }
        if self.finish_reason.is_none() {
            return Err(error("上游流提前结束，未收到正常完成标记"));
        }
        for key in self.tools.keys().copied().collect::<Vec<_>>() {
            self.tool_delta(key, "", "", "", true)?;
            let tool = &self.tools[&key];
            if self.finish_reason.as_deref() != Some("max_tokens") {
                tool_block(&tool.id, &tool.name, &tool.arguments)?;
            }
        }
        for index in 0..self.blocks {
            self.emit(json!({"type": "content_block_stop", "index": index}));
        }
        let stop = if self.finish_reason.as_deref() == Some("max_tokens") {
            "max_tokens"
        } else if !self.tools.is_empty() {
            "tool_use"
        } else {
            "end_turn"
        };
        self.emit(json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null}, "usage": self.usage}));
        self.emit(json!({"type": "message_stop"}));
        self.finished = true;
        Ok(())
    }

    fn consume(&mut self, frame: &[u8]) -> Result<(), ManagerError> {
        if self.finished {
            return Ok(());
        }
        let frame = std::str::from_utf8(frame).map_err(|_| error("上游 SSE 包含无效 UTF-8"))?;
        let data = frame
            .lines()
            .filter_map(|line| {
                line.strip_prefix("data:")
                    .map(|value| value.strip_prefix(' ').unwrap_or(value))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            return Ok(());
        }
        if data.trim() == "[DONE]" {
            return self.finish();
        }
        let event: Value = serde_json::from_str(&data)?;
        if event.get("error").is_some_and(|value| !value.is_null())
            || ["error", "response.failed"].contains(&text(&event, "type"))
        {
            self.fail("OpenAI 上游返回流式错误，响应未正常完成");
            return Ok(());
        }
        if !self.responses {
            if !event["usage"].is_null() {
                self.usage = usage(&event["usage"], false);
            }
            for choice in event["choices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|choice| choice["index"].as_u64().unwrap_or(0) == 0)
            {
                for field in ["content", "refusal"] {
                    self.text_delta("text".to_string(), raw(&choice["delta"], field));
                }
                if let Some(tools) = choice["delta"]["tool_calls"].as_array() {
                    for tool in tools {
                        self.tool_delta(
                            tool["index"].as_u64().unwrap_or(0),
                            text(tool, "id"),
                            text(&tool["function"], "name"),
                            raw(&tool["function"], "arguments"),
                            false,
                        )?;
                    }
                }
                if let Some(reason) = choice["finish_reason"].as_str() {
                    self.finish_reason = Some(
                        if reason == "length" {
                            "max_tokens"
                        } else {
                            "end_turn"
                        }
                        .to_string(),
                    );
                }
            }
            return Ok(());
        }
        match text(&event, "type") {
            "response.output_text.delta" | "response.refusal.delta" => self.text_delta(
                format!("{}:{}", event["output_index"], event["content_index"]),
                raw(&event, "delta"),
            ),
            "response.output_item.added" | "response.output_item.done"
                if event["item"]["type"] == "function_call" =>
            {
                let key = event["output_index"].as_u64().unwrap_or(0);
                let item = &event["item"];
                if !self.tools.contains_key(&key) {
                    self.tool_delta(
                        key,
                        text(item, "call_id"),
                        text(item, "name"),
                        raw(item, "arguments"),
                        false,
                    )?;
                } else if self.tools[&key].arguments.is_empty()
                    && !raw(item, "arguments").is_empty()
                {
                    self.tool_delta(key, "", "", raw(item, "arguments"), false)?;
                }
            }
            "response.function_call_arguments.delta" => self.tool_delta(
                event["output_index"].as_u64().unwrap_or(0),
                "",
                "",
                raw(&event, "delta"),
                false,
            )?,
            "response.function_call_arguments.done" => {
                let key = event["output_index"].as_u64().unwrap_or(0);
                if self
                    .tools
                    .get(&key)
                    .is_some_and(|tool| tool.arguments.is_empty())
                {
                    self.tool_delta(key, "", "", raw(&event, "arguments"), false)?;
                }
            }
            "response.completed" | "response.incomplete" => {
                if !event["response"]["error"].is_null() {
                    self.fail("Responses 上游返回错误，响应未正常完成");
                    return Ok(());
                }
                self.usage = usage(&event["response"]["usage"], true);
                self.finish_reason = Some(
                    if event["response"]["incomplete_details"]["reason"] == "max_output_tokens" {
                        "max_tokens"
                    } else {
                        "end_turn"
                    }
                    .to_string(),
                );
                self.finish()?;
            }
            _ => {}
        }
        Ok(())
    }
}

struct StreamState {
    upstream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: Vec<u8>,
    converter: StreamConverter,
    ended: bool,
}

fn frame_boundary(buffer: &[u8]) -> Option<usize> {
    let unix = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| index + 2);
    let windows = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4);
    match (unix, windows) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    }
}

fn stream_response(
    upstream: reqwest::Response,
    responses: bool,
    model: &str,
) -> Response<GatewayBody> {
    let state = StreamState {
        upstream: Box::pin(upstream.bytes_stream()),
        buffer: Vec::new(),
        converter: StreamConverter::new(responses, model),
        ended: false,
    };
    let stream = futures_util::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.converter.events.pop_front() {
                return Some((
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(Frame::data(event)),
                    state,
                ));
            }
            if state.converter.finished {
                return None;
            }
            if let Some(boundary) = frame_boundary(&state.buffer) {
                let frame = state.buffer.drain(..boundary).collect::<Vec<_>>();
                if let Err(cause) = state.converter.consume(&frame) {
                    state.converter.fail(&cause.to_string());
                }
                continue;
            }
            if state.ended {
                if !state.buffer.is_empty() {
                    if let Err(cause) = state.converter.consume(&std::mem::take(&mut state.buffer))
                    {
                        state.converter.fail(&cause.to_string());
                    }
                }
                if let Err(cause) = state.converter.finish() {
                    state.converter.fail(&cause.to_string());
                }
                continue;
            }
            match state.upstream.next().await {
                Some(Ok(bytes)) => {
                    state.buffer.extend_from_slice(&bytes);
                    if state.buffer.len() > 8 * 1024 * 1024 {
                        state.converter.fail("上游 SSE 事件超过 8 MiB");
                    }
                }
                Some(Err(_)) => state.converter.fail("OpenAI 上游连接中断，响应未正常完成"),
                None => state.ended = true,
            }
        }
    });
    let mut response = Response::new(StreamBody::new(Box::pin(stream)).boxed_unsync());
    response.headers_mut().insert(
        "content-type",
        hyper::header::HeaderValue::from_static("text/event-stream"),
    );
    response.headers_mut().insert(
        "cache-control",
        hyper::header::HeaderValue::from_static("no-cache"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::body::Incoming;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;
    use tokio::net::TcpListener;

    fn payload() -> Value {
        json!({"model": "upstream-model", "max_tokens": 1024, "system": [{"type": "text", "text": " preserve whitespace \n"}], "messages": [
            {"role": "user", "content": [{"type": "text", "text": " hello "}, {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "aGVsbG8="}}]},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "call-1", "name": "weather", "input": {"city": "New York"}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call-1", "content": " sunny "}]}
        ], "tools": [{"name": "weather", "description": "查询天气", "input_schema": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}], "tool_choice": {"type": "any", "disable_parallel_tool_use": true}})
    }

    fn chat_response() -> Value {
        json!({"choices": [{"index": 0, "message": {"content": " hello ", "tool_calls": [{"id": "call-1", "type": "function", "function": {"name": "weather", "arguments": "{\"city\":\"New York\"}"}}]}, "finish_reason": "tool_calls"}], "usage": {"prompt_tokens": 20, "completion_tokens": 8, "prompt_tokens_details": {"cached_tokens": 5}}})
    }

    fn responses_response() -> Value {
        json!({"status": "completed", "output": [{"type": "message", "content": [{"type": "output_text", "text": " hello "}]}, {"type": "function_call", "call_id": "call-1", "name": "weather", "arguments": "{\"city\":\"New York\"}"}], "usage": {"input_tokens": 20, "output_tokens": 8, "input_tokens_details": {"cached_tokens": 5}}})
    }

    fn consume(converter: &mut StreamConverter, event: Value) {
        converter
            .consume(format!("data: {event}\r\n\r\n").as_bytes())
            .unwrap();
    }

    fn events(converter: &mut StreamConverter) -> Vec<Value> {
        converter
            .events
            .drain(..)
            .map(|frame| {
                let text = std::str::from_utf8(&frame).unwrap();
                serde_json::from_str(
                    text.lines()
                        .find_map(|line| line.strip_prefix("data: "))
                        .unwrap(),
                )
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn chat_request_preserves_text_images_and_tool_round_trip() {
        let request = convert_request(&payload(), false).unwrap();
        assert_eq!(request["messages"][0]["content"], " preserve whitespace \n");
        assert_eq!(request["messages"][1]["content"][0]["text"], " hello ");
        assert_eq!(
            request["messages"][1]["content"][1]["image_url"]["url"],
            "data:image/png;base64,aGVsbG8="
        );
        assert_eq!(request["messages"][2]["tool_calls"][0]["id"], "call-1");
        assert_eq!(request["messages"][3]["tool_call_id"], "call-1");
        assert_eq!(request["messages"][3]["content"], " sunny ");
        assert_eq!(
            request["tools"][0]["function"]["parameters"]["required"],
            json!(["city"])
        );
        assert_eq!(request["tool_choice"], "required");
        assert_eq!(request["parallel_tool_calls"], false);
        assert_eq!(request["max_completion_tokens"], 1024);
    }

    #[test]
    fn responses_request_uses_items_flat_tools_and_no_server_storage() {
        let request = convert_request(&payload(), true).unwrap();
        assert_eq!(request["store"], false);
        assert_eq!(request["input"][1]["content"][0]["type"], "input_text");
        assert_eq!(request["input"][1]["content"][1]["type"], "input_image");
        assert_eq!(request["input"][2]["type"], "function_call");
        assert_eq!(request["input"][3]["type"], "function_call_output");
        assert_eq!(
            request["input"][2]["call_id"],
            request["input"][3]["call_id"]
        );
        assert_eq!(request["tools"][0]["name"], "weather");
        assert_eq!(request["tools"][0]["strict"], false);
        assert_eq!(request["max_output_tokens"], 1024);
        assert!(request.get("messages").is_none());
    }

    #[test]
    fn unsupported_native_tools_and_invalid_inputs_fail_explicitly() {
        let mut source = payload();
        source["tools"] = json!([{"type": "web_search_20250305", "name": "web_search"}]);
        assert!(convert_request(&source, false).is_err());
        source = payload();
        source["messages"][0]["content"] = json!([{"type": "unknown"}]);
        assert!(convert_request(&source, true).is_err());
        source = payload();
        source["stop_sequences"] = json!(["END"]);
        assert!(convert_request(&source, true).is_err());
        assert_eq!(
            convert_request(&source, false).unwrap()["stop"],
            json!(["END"])
        );
    }

    #[test]
    fn both_json_responses_return_anthropic_models_tools_and_usage() {
        for (responses, upstream) in [(false, chat_response()), (true, responses_response())] {
            let result = convert_response(&upstream, responses, "claude-sonnet-4-6").unwrap();
            assert_eq!(result["model"], "claude-sonnet-4-6");
            assert_eq!(result["content"][0]["text"], " hello ");
            assert_eq!(result["content"][1]["type"], "tool_use");
            assert_eq!(result["content"][1]["input"]["city"], "New York");
            assert_eq!(result["stop_reason"], "tool_use");
            assert_eq!(result["usage"]["input_tokens"], 15);
            assert_eq!(result["usage"]["cache_read_input_tokens"], 5);
        }
        let mut limited = responses_response();
        limited["status"] = json!("incomplete");
        limited["incomplete_details"] = json!({"reason": "max_output_tokens"});
        assert_eq!(
            convert_response(&limited, true, "route").unwrap()["stop_reason"],
            "max_tokens"
        );
        let mut malformed = chat_response();
        malformed["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"] =
            json!("not json");
        assert!(convert_response(&malformed, false, "route").is_err());
    }

    #[test]
    fn chat_stream_preserves_parallel_tool_ids_argument_whitespace_and_final_usage() {
        let mut converter = StreamConverter::new(false, "claude-sonnet-4-6");
        consume(
            &mut converter,
            json!({"choices": [{"index": 0, "delta": {"content": " hello "}}]}),
        );
        consume(
            &mut converter,
            json!({"choices": [{"delta": {"tool_calls": [
                {"index": 0, "id": "call-a", "function": {"name": "first", "arguments": "{\"value\":\"hello "}},
                {"index": 1, "id": "call-b", "function": {"name": "second", "arguments": "{}"}}
            ]}}]}),
        );
        consume(
            &mut converter,
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"arguments": " world\"}"}}]}, "finish_reason": "tool_calls"}]}),
        );
        consume(
            &mut converter,
            json!({"choices": [], "usage": {"prompt_tokens": 12, "completion_tokens": 4}}),
        );
        converter.consume(b"data: [DONE]\n\n").unwrap();
        let frames = events(&mut converter);
        assert_eq!(frames.first().unwrap()["type"], "message_start");
        assert_eq!(frames.last().unwrap()["type"], "message_stop");
        assert!(frames
            .iter()
            .any(|frame| frame["delta"]["text"] == " hello "));
        assert!(frames
            .iter()
            .any(|frame| frame["delta"]["partial_json"] == " world\"}"));
        assert_eq!(
            frames
                .iter()
                .filter(|frame| frame["type"] == "content_block_start")
                .count(),
            3
        );
        assert_eq!(
            frames
                .iter()
                .find(|frame| frame["type"] == "message_delta")
                .unwrap()["usage"]["output_tokens"],
            4
        );
        assert_eq!(
            converter.tools[&0].arguments,
            "{\"value\":\"hello  world\"}"
        );
    }

    #[test]
    fn responses_stream_converts_function_events_and_does_not_fake_completion() {
        let mut converter = StreamConverter::new(true, "claude-sonnet-4-6");
        consume(
            &mut converter,
            json!({"type": "response.output_item.added", "output_index": 2, "item": {"type": "function_call", "call_id": "call-1", "name": "weather", "arguments": ""}}),
        );
        consume(
            &mut converter,
            json!({"type": "response.function_call_arguments.delta", "output_index": 2, "delta": "{\"city\":\"New "}),
        );
        consume(
            &mut converter,
            json!({"type": "response.function_call_arguments.delta", "output_index": 2, "delta": "York\"}"}),
        );
        consume(
            &mut converter,
            json!({"type": "response.completed", "response": responses_response()}),
        );
        let frames = events(&mut converter);
        assert!(frames
            .iter()
            .any(|frame| frame["content_block"]["id"] == "call-1"));
        assert!(frames
            .iter()
            .any(|frame| frame["delta"]["stop_reason"] == "tool_use"));
        assert_eq!(frames.last().unwrap()["type"], "message_stop");
        let mut broken = StreamConverter::new(true, "route");
        consume(
            &mut broken,
            json!({"type": "response.output_text.delta", "delta": "partial"}),
        );
        assert!(broken.finish().is_err());
        consume(
            &mut broken,
            json!({"type": "response.failed", "response": {"error": {"message": "failure"}}}),
        );
        let frames = events(&mut broken);
        assert!(frames.iter().any(|frame| frame["type"] == "error"));
        assert!(!frames.iter().any(|frame| frame["type"] == "message_stop"));
    }

    #[test]
    fn http_adapters_convert_json_and_fragmented_utf8_sse_for_both_protocols() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            for responses in [false, true] {
                for streaming in [false, true] {
                    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
                    let port = listener.local_addr().unwrap().port();
                    let server = tokio::spawn(async move {
                        let (stream, _) = listener.accept().await.unwrap();
                        let service = service_fn(move |request: hyper::Request<Incoming>| async move {
                            assert_eq!(request.uri().path(), if responses { "/v1/responses" } else { "/v1/chat/completions" });
                            assert_eq!(request.headers()["authorization"], "Bearer upstream-secret");
                            assert!(request.headers().get("anthropic-version").is_none());
                            let body: Value = serde_json::from_slice(&request.into_body().collect().await.unwrap().to_bytes()).unwrap();
                            assert_eq!(body["model"], "upstream-model");
                            assert_eq!(body["stream"], streaming);
                            let output = if streaming {
                                if responses { format!("data: {}\r\n\r\ndata: {}\r\n\r\n", json!({"type": "response.output_text.delta", "output_index": 0, "content_index": 0, "delta": " 你好 "}), json!({"type": "response.completed", "response": {"status": "completed", "usage": {"input_tokens": 5, "output_tokens": 2}}})) }
                                else { format!("data: {}\n\ndata: {}\n\ndata: [DONE]\n\n", json!({"choices": [{"index": 0, "delta": {"content": " 你好 "}}]}), json!({"choices": [{"delta": {}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 5, "completion_tokens": 2}})) }
                            } else { if responses { responses_response() } else { chat_response() }.to_string() };
                            let chunks = output.as_bytes().chunks(3).map(Bytes::copy_from_slice).collect::<Vec<_>>();
                            let body = StreamBody::new(futures_util::stream::iter(chunks.into_iter().map(|bytes| Ok::<_, Infallible>(Frame::data(bytes)))));
                            Ok::<_, Infallible>(Response::builder().header("content-type", if streaming { "text/event-stream" } else { "application/json" }).body(body).unwrap())
                        });
                        let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
                    });
                    let provider = json!({"apiFormat": if responses { "openai_responses" } else { "openai_chat" }, "baseUrl": format!("http://127.0.0.1:{port}/v1")});
                    let mut input = payload(); input["stream"] = json!(streaming);
                    let response = forward(&provider, "upstream-secret", input, "claude-sonnet-4-6").await.unwrap();
                    assert_eq!(response.status(), StatusCode::OK);
                    let output = response.into_body().collect().await.unwrap().to_bytes();
                    let output = String::from_utf8(output.to_vec()).unwrap();
                    if streaming {
                        assert!(output.contains(" 你好 "));
                        assert!(output.contains("event: message_stop"));
                        assert!(!output.contains("event: error"), "{output}");
                    } else {
                        let output: Value = serde_json::from_str(&output).unwrap();
                        assert_eq!(output["model"], "claude-sonnet-4-6");
                        assert_eq!(output["content"][1]["input"]["city"], "New York");
                    }
                    server.abort();
                }
            }
        });
    }
}

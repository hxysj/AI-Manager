use super::*;
use futures_util::{FutureExt, StreamExt};
use std::collections::BTreeMap;
use tokio::sync::mpsc;

enum Delta {
    Text(String),
    Tool(usize, Value),
}

pub(super) struct Parser {
    format: UpstreamFormat,
    pub response: ModelResponse,
    done: bool,
    calls: BTreeMap<usize, usize>,
}

impl Parser {
    pub fn new(format: UpstreamFormat) -> Self {
        Self {
            format,
            response: ModelResponse::default(),
            done: false,
            calls: BTreeMap::new(),
        }
    }

    fn text(&mut self, text: &str, deltas: &mut Vec<Delta>) {
        if !text.is_empty() {
            self.response.text.push_str(text);
            deltas.push(Delta::Text(text.to_string()));
        }
    }

    fn tool(
        &mut self,
        source_index: usize,
        id: &str,
        name: &str,
        args: &str,
        deltas: &mut Vec<Delta>,
    ) {
        let index = *self.calls.entry(source_index).or_insert_with(|| {
            let index = self.response.tool_calls.len();
            self.response
                .tool_calls
                .push(json!({"id":"","type":"function","function":{"name":"","arguments":""}}));
            index
        });
        let call = &mut self.response.tool_calls[index];
        if !id.is_empty() {
            call["id"] = json!(id);
        }
        if !name.is_empty() {
            call["function"]["name"] = json!(name);
        }
        let mut arguments = call["function"]["arguments"]
            .as_str()
            .unwrap_or("")
            .to_string();
        arguments.push_str(args);
        call["function"]["arguments"] = json!(arguments);
        deltas.push(Delta::Tool(index, json!({"id":call["id"],"type":"function","function":{"name":call["function"]["name"],"arguments":args}})));
    }

    fn usage(&mut self, usage: &Value, input: &str, output: &str, cache: u64) {
        if !usage.is_object() {
            return;
        }
        self.response.usage_known = true;
        if let Some(value) = usage[input].as_u64() {
            self.response.input_tokens = value;
        }
        if let Some(value) = usage[output].as_u64() {
            self.response.output_tokens = value;
        }
        self.response.cache_read_tokens = self.response.cache_read_tokens.max(cache);
    }

    fn snapshot(&mut self, value: &Value, native: bool) -> Result<Vec<Delta>, String> {
        if self.format == UpstreamFormat::OpenAiResponses && value["status"] == "failed" {
            return Err("上游 Responses 生成失败".into());
        }
        if !native {
            let unsupported = match self.format {
                UpstreamFormat::OpenAiChat => {
                    ["refusal", "audio", "reasoning_content"].iter().any(|key| {
                        value["choices"][0]["message"]
                            .get(key)
                            .is_some_and(|v| !v.is_null() && v != "")
                    })
                }
                UpstreamFormat::OpenAiResponses => value["output"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|item| {
                        // 空 reasoning 项只携带上游内部状态，没有需要投影到客户端的内容。
                        (!matches!(item["type"].as_str(), Some("message" | "function_call"))
                            && !(item["type"] == "reasoning"
                                && item["summary"].as_array().is_some_and(Vec::is_empty)))
                            || item["content"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|part| part["type"] != "output_text")
                    }),
                UpstreamFormat::Anthropic => value["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|block| !matches!(block["type"].as_str(), Some("text" | "tool_use"))),
                UpstreamFormat::Gemini => value["candidates"][0]["content"]["parts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|part| {
                        part["thought"] == true
                            || part.get("thoughtSignature").is_some()
                            || (part.get("text").is_none() && part.get("functionCall").is_none())
                    }),
            };
            if unsupported {
                return Err("此上游内容需要使用同协议客户端".into());
            }
        }
        let parsed = parse_upstream_response(value, self.format).map_err(|_| "上游响应结构无效")?;
        let mut deltas = Vec::new();
        if let Some(suffix) = parsed.text.strip_prefix(&self.response.text) {
            let suffix = suffix.to_string();
            self.text(&suffix, &mut deltas);
        } else {
            return Err("上游最终文本与增量文本不一致".into());
        }
        if parsed.tool_calls.len() < self.response.tool_calls.len() {
            return Err("上游最终结果缺少已收到的函数调用".into());
        }
        for (index, call) in parsed.tool_calls.iter().enumerate() {
            if index >= self.response.tool_calls.len() {
                self.tool(
                    index,
                    string(call.get("id")),
                    string(call["function"].get("name")),
                    string(call["function"].get("arguments")),
                    &mut deltas,
                );
            } else {
                let previous = string(self.response.tool_calls[index]["function"].get("arguments"));
                let next = string(call["function"].get("arguments"));
                if let Some(suffix) = next.strip_prefix(previous) {
                    if !suffix.is_empty() {
                        let source = self
                            .calls
                            .iter()
                            .find(|(_, value)| **value == index)
                            .map(|(key, _)| *key)
                            .unwrap_or(index);
                        self.tool(
                            source,
                            string(call.get("id")),
                            string(call["function"].get("name")),
                            suffix,
                            &mut deltas,
                        );
                    }
                } else {
                    return Err("上游最终函数参数与增量参数不一致".into());
                }
            }
        }
        self.response = parsed;
        self.done = true;
        Ok(deltas)
    }

    fn event(&mut self, event: &Value, native: bool) -> Result<Vec<Delta>, String> {
        if event.get("error").is_some_and(|value| !value.is_null()) {
            return Err("上游返回生成错误，请检查目标账号和上游服务".into());
        }
        let mut deltas = Vec::new();
        match self.format {
            UpstreamFormat::OpenAiChat => {
                if let Some(model) = event["model"].as_str() {
                    self.response.upstream_model = model.to_string();
                }
                self.usage(
                    &event["usage"],
                    "prompt_tokens",
                    "completion_tokens",
                    event["usage"]["prompt_tokens_details"]["cached_tokens"]
                        .as_u64()
                        .unwrap_or(0),
                );
                for choice in event["choices"].as_array().into_iter().flatten() {
                    if choice["index"].as_u64().unwrap_or(0) != 0 {
                        return Err("只支持单候选上游响应".into());
                    }
                    let delta = &choice["delta"];
                    if !native
                        && ["refusal", "audio", "reasoning_content"]
                            .iter()
                            .any(|key| delta.get(key).is_some_and(|v| !v.is_null() && v != ""))
                    {
                        return Err("此上游内容需要使用同协议客户端".into());
                    }
                    self.text(string(delta.get("content")), &mut deltas);
                    for call in delta["tool_calls"].as_array().into_iter().flatten() {
                        self.tool(
                            call["index"].as_u64().unwrap_or(0) as usize,
                            string(call.get("id")),
                            string(call["function"].get("name")),
                            string(call["function"].get("arguments")),
                            &mut deltas,
                        );
                    }
                    if let Some(reason) = choice["finish_reason"].as_str() {
                        self.response.finish_reason = reason.to_string();
                    }
                }
            }
            UpstreamFormat::OpenAiResponses => {
                let kind = string(event.get("type"));
                match kind {
                    "response.created" | "response.in_progress" => {
                        if let Some(model) = event["response"]["model"].as_str() {
                            self.response.upstream_model = model.to_string();
                        }
                    }
                    "response.output_text.delta" => {
                        self.text(string(event.get("delta")), &mut deltas)
                    }
                    "response.output_item.added" if event["item"]["type"] == "function_call" => {
                        let item = &event["item"];
                        self.tool(
                            event["output_index"].as_u64().unwrap_or(0) as usize,
                            string(item.get("call_id")),
                            string(item.get("name")),
                            string(item.get("arguments")),
                            &mut deltas,
                        );
                    }
                    "response.output_item.added" if event["item"]["type"] == "reasoning" => {}
                    "response.output_item.added"
                        if !native && event["item"]["type"] != "message" =>
                    {
                        return Err("此上游输出项需要使用 Responses 客户端".into())
                    }
                    "response.function_call_arguments.delta" => self.tool(
                        event["output_index"].as_u64().unwrap_or(0) as usize,
                        "",
                        "",
                        string(event.get("delta")),
                        &mut deltas,
                    ),
                    "response.completed" | "response.incomplete" => {
                        return self.snapshot(&event["response"], native)
                    }
                    "response.failed" | "error" => return Err("上游 Responses 生成失败".into()),
                    _ if !native
                        && (kind.contains("refusal")
                            || kind.contains("reasoning")
                            || kind.contains("audio")) =>
                    {
                        return Err("此上游内容需要使用 Responses 客户端".into())
                    }
                    _ => {}
                }
            }
            UpstreamFormat::Anthropic => match string(event.get("type")) {
                "message_start" => {
                    self.response.upstream_model =
                        string(event["message"].get("model")).to_string();
                    self.usage(
                        &event["message"]["usage"],
                        "input_tokens",
                        "output_tokens",
                        event["message"]["usage"]["cache_read_input_tokens"]
                            .as_u64()
                            .unwrap_or(0),
                    );
                }
                "content_block_start" => {
                    let block = &event["content_block"];
                    match string(block.get("type")) {
                        "text" => self.text(string(block.get("text")), &mut deltas),
                        "tool_use" => {
                            let args = block
                                .get("input")
                                .filter(|v| v.as_object().is_some_and(|v| !v.is_empty()))
                                .map(Value::to_string)
                                .unwrap_or_default();
                            self.tool(
                                event["index"].as_u64().unwrap_or(0) as usize,
                                string(block.get("id")),
                                string(block.get("name")),
                                &args,
                                &mut deltas,
                            );
                        }
                        _ if !native => return Err("此上游内容需要使用 Anthropic 客户端".into()),
                        _ => {}
                    }
                }
                "content_block_delta" => {
                    let delta = &event["delta"];
                    match string(delta.get("type")) {
                        "text_delta" => self.text(string(delta.get("text")), &mut deltas),
                        "input_json_delta" => self.tool(
                            event["index"].as_u64().unwrap_or(0) as usize,
                            "",
                            "",
                            string(delta.get("partial_json")),
                            &mut deltas,
                        ),
                        _ if !native => return Err("此上游内容需要使用 Anthropic 客户端".into()),
                        _ => {}
                    }
                }
                "message_delta" => {
                    self.response.finish_reason =
                        string(event["delta"].get("stop_reason")).to_string();
                    self.usage(
                        &event["usage"],
                        "input_tokens",
                        "output_tokens",
                        event["usage"]["cache_read_input_tokens"]
                            .as_u64()
                            .unwrap_or(0),
                    );
                }
                "message_stop" => self.done = true,
                "error" => return Err("上游 Anthropic 生成失败".into()),
                _ => {}
            },
            UpstreamFormat::Gemini => {
                if let Some(model) = event["modelVersion"].as_str() {
                    self.response.upstream_model = model.to_string();
                }
                self.usage(
                    &event["usageMetadata"],
                    "promptTokenCount",
                    "candidatesTokenCount",
                    event["usageMetadata"]["cachedContentTokenCount"]
                        .as_u64()
                        .unwrap_or(0),
                );
                for candidate in event["candidates"].as_array().into_iter().flatten() {
                    for part in candidate["content"]["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        if !native
                            && (part["thought"] == true
                                || part.get("inlineData").is_some()
                                || part.get("thoughtSignature").is_some())
                        {
                            return Err("此上游内容需要使用 Gemini 客户端".into());
                        }
                        self.text(string(part.get("text")), &mut deltas);
                        if let Some(call) = part.get("functionCall") {
                            let index = self.response.tool_calls.len();
                            let id = call["id"].as_str().map(str::to_string).unwrap_or_else(|| {
                                format!("call_{}", uuid::Uuid::new_v4().simple())
                            });
                            self.tool(
                                index,
                                &id,
                                string(call.get("name")),
                                &call
                                    .get("args")
                                    .cloned()
                                    .unwrap_or_else(|| json!({}))
                                    .to_string(),
                                &mut deltas,
                            );
                        }
                    }
                    if let Some(reason) = candidate["finishReason"].as_str() {
                        self.response.finish_reason = reason.to_string();
                        self.done = true;
                    }
                }
                if event["promptFeedback"]["blockReason"].is_string() {
                    return Err("Gemini 拒绝了此请求".into());
                }
            }
        }
        Ok(deltas)
    }
}

fn string(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).unwrap_or("")
}

// 字节级缓存直到完整帧，避免网络分块拆开中文或 CRLF。
fn boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    bytes
        .windows(2)
        .position(|v| v == b"\n\n")
        .map(|index| (index, 2))
        .into_iter()
        .chain(
            bytes
                .windows(4)
                .position(|v| v == b"\r\n\r\n")
                .map(|index| (index, 4)),
        )
        .min_by_key(|(index, _)| *index)
}

fn frame_data(frame: &[u8]) -> Result<String, String> {
    let frame = std::str::from_utf8(frame).map_err(|_| "上游 SSE 包含无效 UTF-8")?;
    Ok(frame
        .lines()
        .filter_map(|line| {
            line.strip_prefix("data:")
                .map(|value| value.strip_prefix(' ').unwrap_or(value))
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(super) async fn consume(
    upstream: reqwest::Response,
    is_sse: bool,
    parser: &mut Parser,
    mut output: Option<(
        &mpsc::Sender<Result<Frame<Bytes>, Infallible>>,
        &mut Encoder,
    )>,
    native: bool,
    tracking: &mut Option<KeyRequest>,
) -> Result<(), String> {
    let mut pending = Vec::new();
    let mut stream = upstream.bytes_stream();
    let mut total = 0;
    loop {
        let chunk = if let Some((sender, _)) = output.as_ref() {
            futures_util::select! {
                _ = sender.closed().fuse() => return Err("客户端已断开".into()),
                chunk = stream.next().fuse() => chunk,
            }
        } else {
            stream.next().await
        };
        let Some(chunk) = chunk else {
            break;
        };
        let chunk = chunk.map_err(|_| "上游连接中断")?;
        if let Some(tracking) = tracking {
            tracking.observe(&chunk);
        }
        total += chunk.len();
        if total > MAX_BODY_SIZE {
            return Err("上游响应超过 32 MiB".into());
        }
        pending.extend_from_slice(&chunk);
        if !is_sse {
            continue;
        }
        while let Some((index, size)) = boundary(&pending) {
            let frame: Vec<_> = pending.drain(..index + size).collect();
            relay_frame(&frame, parser, &mut output, native).await?;
        }
        if pending.len() > 4 * 1024 * 1024 {
            return Err("上游单个 SSE 帧过大".into());
        }
    }
    if is_sse {
        if !pending.is_empty() {
            relay_frame(&pending, parser, &mut output, native).await?;
        }
        if !parser.done {
            return Err("上游流提前结束，缺少完成事件".into());
        }
    } else {
        let value: Value =
            serde_json::from_slice(&pending).map_err(|_| "上游返回不是有效 JSON 或 SSE")?;
        if value.get("error").is_some_and(|v| !v.is_null()) {
            return Err("上游返回生成错误".into());
        }
        if parser.format == UpstreamFormat::OpenAiResponses
            && !matches!(value["status"].as_str(), Some("completed" | "incomplete"))
        {
            return Err("上游 Responses 未完成".into());
        }
        let deltas = parser.snapshot(&value, native)?;
        if let Some((sender, encoder)) = &mut output {
            // 上游忽略 stream 返回 JSON 时仍用完整生命周期输出，无法提前产生内容。
            send(sender, encoder.begin()).await?;
            for delta in deltas {
                send(sender, encoder.delta(delta)).await?;
            }
        }
    }
    if let Some((sender, encoder)) = &mut output {
        if !native || !is_sse {
            send(sender, encoder.finish(&parser.response)).await?;
        }
    }
    Ok(())
}

async fn relay_frame(
    frame: &[u8],
    parser: &mut Parser,
    output: &mut Option<(
        &mpsc::Sender<Result<Frame<Bytes>, Infallible>>,
        &mut Encoder,
    )>,
    native: bool,
) -> Result<(), String> {
    let data = frame_data(frame)?;
    let deltas = if data == "[DONE]" {
        if parser.format != UpstreamFormat::OpenAiChat || parser.response.finish_reason.is_empty() {
            return Err("上游流缺少结束原因".into());
        }
        parser.done = true;
        Vec::new()
    } else if data.is_empty() {
        Vec::new()
    } else {
        let event: Value = serde_json::from_str(&data).map_err(|_| "上游 SSE 不是有效 JSON")?;
        parser.event(&event, native)?
    };
    if let Some((sender, encoder)) = output {
        if native {
            send(
                sender,
                String::from_utf8(frame.to_vec()).map_err(|_| "上游 SSE 编码无效")?,
            )
            .await?;
        } else {
            if !parser.response.upstream_model.is_empty() {
                encoder.model = parser.response.upstream_model.clone();
            }
            send(sender, encoder.begin()).await?;
            for delta in deltas {
                send(sender, encoder.delta(delta)).await?;
            }
        }
    }
    Ok(())
}

async fn send(
    sender: &mpsc::Sender<Result<Frame<Bytes>, Infallible>>,
    data: String,
) -> Result<(), String> {
    if !data.is_empty() {
        sender
            .send(Ok(Frame::data(Bytes::from(data))))
            .await
            .map_err(|_| "客户端已断开")?;
    }
    Ok(())
}

pub(super) struct Encoder {
    protocol: Protocol,
    model: String,
    id: String,
    message_id: String,
    started: bool,
    calls: BTreeMap<usize, Value>,
    sequence: u64,
}

impl Encoder {
    pub fn new(protocol: Protocol, model: &str) -> Self {
        Self {
            protocol,
            model: model.to_string(),
            id: format!("resp_{}", uuid::Uuid::new_v4().simple()),
            message_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            started: false,
            calls: BTreeMap::new(),
            sequence: 0,
        }
    }

    fn event(&mut self, mut value: Value) -> String {
        if self.protocol == Protocol::Responses {
            value["sequence_number"] = json!(self.sequence);
            self.sequence += 1;
        }
        if matches!(self.protocol, Protocol::Responses | Protocol::Claude) {
            format!("event: {}\ndata: {value}\n\n", string(value.get("type")))
        } else {
            format!("data: {value}\n\n")
        }
    }

    fn chat(&mut self, delta: Value, finish: Value) -> String {
        self.event(json!({"id":self.id,"object":"chat.completion.chunk","created":chrono::Utc::now().timestamp(),"model":self.model,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]}))
    }

    fn begin(&mut self) -> String {
        if self.started {
            return String::new();
        }
        self.started = true;
        match self.protocol {
            Protocol::Chat => self.chat(json!({"role":"assistant","content":""}), Value::Null),
            Protocol::Responses => {
                let response = json!({"id":self.id,"object":"response","created_at":chrono::Utc::now().timestamp(),"status":"in_progress","model":self.model,"output":[]});
                let mut result = self.event(json!({"type":"response.created","response":response}));
                result.push_str(
                    &self.event(json!({"type":"response.in_progress","response":response})),
                );
                result.push_str(&self.event(json!({"type":"response.output_item.added","output_index":0,"item":{"id":self.message_id,"type":"message","role":"assistant","status":"in_progress","content":[]}})));
                result.push_str(&self.event(json!({"type":"response.content_part.added","item_id":self.message_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}})));
                result
            }
            Protocol::Claude => {
                let mut result = self.event(json!({"type":"message_start","message":{"id":self.message_id,"type":"message","role":"assistant","model":self.model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}}));
                result.push_str(&self.event(json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
                result
            }
            Protocol::Gemini => String::new(),
        }
    }

    fn delta(&mut self, delta: Delta) -> String {
        match delta {
            Delta::Text(text) => match self.protocol {
                Protocol::Chat => self.chat(json!({"content":text}), Value::Null),
                Protocol::Responses => self.event(json!({"type":"response.output_text.delta","item_id":self.message_id,"output_index":0,"content_index":0,"delta":text})),
                Protocol::Claude => self.event(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}})),
                Protocol::Gemini => self.event(json!({"candidates":[{"index":0,"content":{"role":"model","parts":[{"text":text}]}}],"modelVersion":self.model})),
            },
            Delta::Tool(index, call) => {
                let first = !self.calls.contains_key(&index);
                self.calls.insert(index, call.clone());
                match self.protocol {
                    Protocol::Chat => {
                        let mut delta = json!({"index":index,"type":"function","function":{"arguments":call["function"]["arguments"]}});
                        if first { delta["id"] = call["id"].clone(); delta["function"]["name"] = call["function"]["name"].clone(); }
                        self.chat(json!({"tool_calls":[delta]}), Value::Null)
                    }
                    Protocol::Responses => {
                        let mut result = String::new();
                        if first { result.push_str(&self.event(json!({"type":"response.output_item.added","output_index":index+1,"item":{"id":format!("fc_{}_{}",self.id,index),"type":"function_call","call_id":call["id"],"name":call["function"]["name"],"arguments":"","status":"in_progress"}}))); }
                        if let Some(args) = call["function"]["arguments"].as_str().filter(|v| !v.is_empty()) { result.push_str(&self.event(json!({"type":"response.function_call_arguments.delta","item_id":format!("fc_{}_{}",self.id,index),"output_index":index+1,"delta":args}))); }
                        result
                    }
                    // Anthropic 块需要依次闭合，Gemini 函数参数需要完整 JSON。
                    Protocol::Claude | Protocol::Gemini => String::new(),
                }
            }
        }
    }

    fn finish(&mut self, response: &ModelResponse) -> String {
        match self.protocol {
            Protocol::Chat => {
                let mut result = self.chat(json!({}), json!(finish_reason(response)));
                result.push_str(&self.event(json!({"id":self.id,"object":"chat.completion.chunk","created":chrono::Utc::now().timestamp(),"model":self.model,"choices":[],"usage":openai_usage(response)})));
                result.push_str("data: [DONE]\n\n");
                result
            }
            Protocol::Responses => {
                let mut snapshot = response_value(self.protocol, response, &self.model);
                snapshot["id"] = json!(self.id);
                snapshot["output"][0]["id"] = json!(self.message_id);
                let mut result = self.event(json!({"type":"response.output_text.done","item_id":self.message_id,"output_index":0,"content_index":0,"text":response.text}));
                result.push_str(&self.event(json!({"type":"response.content_part.done","item_id":self.message_id,"output_index":0,"content_index":0,"part":snapshot["output"][0]["content"][0]})));
                result.push_str(&self.event(json!({"type":"response.output_item.done","output_index":0,"item":snapshot["output"][0]})));
                for (index, item) in snapshot["output"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .enumerate()
                    .skip(1)
                {
                    item["id"] = json!(format!("fc_{}_{}", self.id, index - 1));
                    result.push_str(&self.event(json!({"type":"response.function_call_arguments.done","item_id":item["id"],"output_index":index,"arguments":item["arguments"],"name":item["name"]})));
                    result.push_str(&self.event(json!({"type":"response.output_item.done","output_index":index,"item":item})));
                }
                result.push_str(&self.event(json!({"type":if snapshot["status"] == "incomplete" {"response.incomplete"} else {"response.completed"},"response":snapshot})));
                result
            }
            Protocol::Claude => {
                let snapshot = response_value(self.protocol, response, &self.model);
                let mut result = self.event(json!({"type":"content_block_stop","index":0}));
                for (index, call) in response.tool_calls.iter().enumerate() {
                    result.push_str(&self.event(json!({"type":"content_block_start","index":index+1,"content_block":{"type":"tool_use","id":call["id"],"name":call["function"]["name"],"input":{}}})));
                    result.push_str(&self.event(json!({"type":"content_block_delta","index":index+1,"delta":{"type":"input_json_delta","partial_json":call["function"]["arguments"]}})));
                    result.push_str(
                        &self.event(json!({"type":"content_block_stop","index":index+1})),
                    );
                }
                result.push_str(&self.event(json!({"type":"message_delta","delta":{"stop_reason":snapshot["stop_reason"],"stop_sequence":null},"usage":snapshot["usage"]})));
                result.push_str(&self.event(json!({"type":"message_stop"})));
                result
            }
            Protocol::Gemini => {
                let mut snapshot = response_value(self.protocol, response, &self.model);
                snapshot["candidates"][0]["content"]["parts"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|part| part.get("text").is_none());
                self.event(snapshot)
            }
        }
    }

    pub fn error(&mut self, message: &str) -> String {
        if self.protocol == Protocol::Responses {
            self.event(json!({"type":"response.failed","response":{"id":self.id,"object":"response","status":"failed","error":{"code":"gateway_error","message":message}}}))
        } else {
            self.event(json!({"type":"error","error":{"type":"gateway_error","message":message}}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_reasoning_is_accepted_and_inconsistent_final_output_is_rejected() {
        let mut parser = Parser::new(UpstreamFormat::OpenAiResponses);
        parser.event(&json!({"type":"response.output_item.added","item":{"type":"reasoning","summary":[]}}), false).unwrap();
        parser
            .event(
                &json!({"type":"response.output_text.delta","delta":" 正文 "}),
                false,
            )
            .unwrap();
        parser.event(&json!({"type":"response.completed","response":{"status":"completed","model":"real","output":[{"type":"reasoning","summary":[],"encrypted_content":"opaque"},{"type":"message","content":[{"type":"output_text","text":" 正文 "}]}],"usage":{"input_tokens":2,"output_tokens":1}}}), false).unwrap();
        assert_eq!(parser.response.text, " 正文 ");
        assert!(parser.done);
        let mut parser = Parser::new(UpstreamFormat::OpenAiResponses);
        parser
            .event(
                &json!({"type":"response.output_text.delta","delta":"不能丢失"}),
                false,
            )
            .unwrap();
        assert!(parser
            .event(
                &json!({"type":"response.completed","response":{"status":"completed","output":[]}}),
                false
            )
            .is_err());
        assert!(Parser::new(UpstreamFormat::OpenAiResponses)
            .event(
                &json!({"type":"response.reasoning_summary_text.delta","delta":"可见思考"}),
                false
            )
            .is_err());
    }
}

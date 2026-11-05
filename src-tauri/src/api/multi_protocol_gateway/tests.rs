use super::*;
use futures_util::StreamExt;
use std::path::PathBuf;
use tokio::sync::mpsc;

struct Fixture {
    root: PathBuf,
    paths: AppPaths,
    registry: GatewayServerRegistry,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("multi-gateway-test-{}", uuid::Uuid::new_v4()));
        let paths = crate::core::paths::resolve_app_paths(&root);
        provider_store::initialize(&paths).unwrap();
        Self {
            root,
            paths,
            registry: GatewayServerRegistry::new(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(future: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
}

fn reply(format: UpstreamFormat) -> Value {
    match format {
        UpstreamFormat::OpenAiChat => {
            json!({"model":"reported-model","choices":[{"index":0,"message":{"role":"assistant","content":" 中文🙂 ","tool_calls":[{"id":"call_test","type":"function","function":{"name":"lookup","arguments":"{\"city\":\"杭州\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":12,"completion_tokens":6}})
        }
        UpstreamFormat::OpenAiResponses => {
            json!({"id":"resp_original","object":"response","model":"reported-model","status":"completed","output":[{"id":"rs_test","type":"reasoning","summary":[{"type":"summary_text","text":"可见思考🙂"}],"encrypted_content":"opaque-reasoning"},{"type":"message","role":"assistant","content":[{"type":"output_text","text":" 中文🙂 "}]},{"type":"function_call","call_id":"call_test","name":"lookup","arguments":"{\"city\":\"杭州\"}"}],"usage":{"input_tokens":12,"output_tokens":6}})
        }
        UpstreamFormat::Anthropic => {
            json!({"id":"msg_original","type":"message","role":"assistant","model":"reported-model","content":[{"type":"text","text":" 中文🙂 "},{"type":"tool_use","id":"call_test","name":"lookup","input":{"city":"杭州"}}],"stop_reason":"tool_use","usage":{"input_tokens":12,"output_tokens":6}})
        }
        UpstreamFormat::Gemini => {
            json!({"modelVersion":"reported-model","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":" 中文🙂 "},{"functionCall":{"id":"call_test","name":"lookup","args":{"city":"杭州"}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":6}})
        }
    }
}

fn sse_reply(format: UpstreamFormat) -> String {
    let events = match format {
        UpstreamFormat::OpenAiChat => vec![
            json!({"model":"reported-model","choices":[{"index":0,"delta":{"role":"assistant","content":" 中文🙂 "},"finish_reason":null}]}),
            json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_test","type":"function","function":{"name":"lookup","arguments":"{\"city\":"}}]},"finish_reason":null}]}),
            json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"杭州\"}"}}]},"finish_reason":"tool_calls"}]}),
            json!({"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":6}}),
        ],
        UpstreamFormat::OpenAiResponses => vec![
            json!({"type":"response.created","response":{"model":"reported-model"}}),
            json!({"type":"response.output_item.added","output_index":0,"item":{"id":"rs_test","type":"reasoning","summary":[]}}),
            json!({"type":"response.reasoning_summary_part.added","output_index":0,"summary_index":0,"part":{"type":"summary_text","text":""}}),
            json!({"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"可见思考🙂"}),
            json!({"type":"response.reasoning_summary_text.done","output_index":0,"summary_index":0,"text":"可见思考🙂"}),
            json!({"type":"response.reasoning_summary_part.done","output_index":0,"summary_index":0,"part":{"type":"summary_text","text":"可见思考🙂"}}),
            json!({"type":"response.output_text.delta","delta":" 中文🙂 "}),
            json!({"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","call_id":"call_test","name":"lookup","arguments":""}}),
            json!({"type":"response.function_call_arguments.delta","output_index":2,"delta":"{\"city\":\"杭州\"}"}),
            json!({"type":"response.completed","response":reply(format)}),
        ],
        UpstreamFormat::Anthropic => vec![
            json!({"type":"message_start","message":{"model":"reported-model","usage":{"input_tokens":12,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" 中文🙂 "}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call_test","name":"lookup","input":{}}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"city\":\"杭州\"}"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":6}}),
            json!({"type":"message_stop"}),
        ],
        UpstreamFormat::Gemini => vec![reply(format)],
    };
    let mut result = events
        .into_iter()
        .map(|event| {
            let name = event["type"]
                .as_str()
                .map(|name| format!("event: {name}\r\n"))
                .unwrap_or_default();
            format!("{name}data: {event}\r\n\r\n")
        })
        .collect::<String>();
    if format == UpstreamFormat::OpenAiChat {
        result.push_str("data: [DONE]\r\n\r\n");
    }
    result
}

// 只监听随机本机端口；以 7 字节切片模拟网络拆开 UTF-8 和 CRLF。
async fn mock(
    format: UpstreamFormat,
    status: StatusCode,
    force_sse: bool,
    truncate: bool,
    gate: Option<Arc<tokio::sync::Notify>>,
) -> (
    String,
    mpsc::Receiver<(String, HeaderMap, Value)>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = mpsc::channel(64);
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let sender = sender.clone();
            let gate = gate.clone();
            connections.spawn(async move {
                let service = service_fn(move |request: Request<Incoming>| {
                    let sender = sender.clone();
                    let gate = gate.clone();
                    async move {
                        let path = request.uri().to_string();
                        let headers = request.headers().clone();
                        let bytes = request.into_body().collect().await.unwrap().to_bytes();
                        let value: Value = serde_json::from_slice(&bytes).unwrap();
                        let streaming = force_sse
                            || value["stream"] == true
                            || path.contains("streamGenerateContent");
                        sender.send((path, headers, value)).await.unwrap();
                        if !status.is_success() {
                            return Ok::<_, Infallible>(json_response(
                                status,
                                json!({"error":"upstream-secret-do-not-log"}),
                            ));
                        }
                        if !streaming {
                            return Ok(json_response(status, reply(format)));
                        }
                        let text = sse_reply(format);
                        let text = if truncate {
                            format!("{}\r\n\r\n", text.split_once("\r\n\r\n").unwrap().0)
                        } else {
                            text
                        };
                        let (first, rest) = text.split_once("\r\n\r\n").unwrap();
                        let chunks: Vec<_> = format!("{first}\r\n\r\n")
                            .as_bytes()
                            .chunks(7)
                            .map(Bytes::copy_from_slice)
                            .collect();
                        let rest: Vec<_> = rest
                            .as_bytes()
                            .chunks(7)
                            .map(Bytes::copy_from_slice)
                            .collect();
                        let stream = futures_util::stream::iter(chunks)
                            .chain(
                                futures_util::stream::once(async move {
                                    if let Some(gate) = gate {
                                        gate.notified().await;
                                    }
                                    rest
                                })
                                .flat_map(futures_util::stream::iter),
                            )
                            .map(|bytes| Ok::<_, Infallible>(Frame::data(bytes)));
                        Ok(Response::builder()
                            .header("content-type", "text/event-stream")
                            .body(StreamBody::new(stream).boxed_unsync())
                            .unwrap())
                    }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(socket), service)
                    .await;
            });
        }
    });
    (format!("http://{address}"), receiver, task)
}

async fn setup(fixture: &Fixture, url: &str, format: UpstreamFormat) -> (String, String) {
    let (kind, cli) = match format {
        UpstreamFormat::OpenAiChat => ("openai", "custom"),
        UpstreamFormat::OpenAiResponses => ("openai", "codex"),
        UpstreamFormat::Anthropic => ("anthropic", "claude"),
        UpstreamFormat::Gemini => ("gemini", "gemini"),
    };
    let provider = json!({"id":"provider-test","name":"本地上游","type":kind,"cli":cli,"baseUrl":url,"enabled":true,"proxy":"","runtimeConfig":{"mainModel":"real-model"}});
    let mut keys = Map::new();
    runtime_provider::set_provider_key(&mut keys, "provider-test", "private-upstream-key".into())
        .unwrap();
    provider_store::write_provider_bundle(&fixture.paths, &[provider], &[], &[], &keys).unwrap();
    let token = create_token(
        &fixture.registry,
        &fixture.paths,
        json!({"name":"测试调用方"}),
    )
    .await
    .unwrap()["createdToken"]["token"]
        .as_str()
        .unwrap()
        .to_string();
    let port = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let state = enable(&fixture.registry, &fixture.paths, &json!({}), json!({"port":port,"providerIds":["provider-test"],"modelMappings":{"alias":{"targetId":"provider-test","model":"real-model"}}})).await.unwrap();
    (state["baseUrl"].as_str().unwrap().to_string(), token)
}

fn input(protocol: Protocol, stream: bool) -> (&'static str, Value) {
    match protocol {
        Protocol::Chat => (
            "/v1/chat/completions",
            json!({"model":"alias","stream":stream,"messages":[{"role":"user","content":"你好"}],"tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{"city":{"type":"string"}}}}}]}),
        ),
        Protocol::Responses => (
            "/v1/responses",
            json!({"model":"alias","stream":stream,"input":[{"role":"user","content":"你好"}],"tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{"city":{"type":"string"}}}}]}),
        ),
        Protocol::Claude => (
            "/v1/messages",
            json!({"model":"alias","stream":stream,"max_tokens":32,"messages":[{"role":"user","content":"你好"}],"tools":[{"name":"lookup","input_schema":{"type":"object","properties":{"city":{"type":"string"}}}}]}),
        ),
        Protocol::Gemini => (
            if stream {
                "/v1beta/models/alias:streamGenerateContent?alt=sse"
            } else {
                "/v1beta/models/alias:generateContent"
            },
            json!({"contents":[{"role":"user","parts":[{"text":"你好"}]}],"tools":[{"functionDeclarations":[{"name":"lookup","parameters":{"type":"object","properties":{"city":{"type":"string"}}}}]}]}),
        ),
    }
}

#[test]
fn protocol_matrix_preserves_tools_usage_model_and_streams() {
    run(async {
        for format in [
            UpstreamFormat::OpenAiChat,
            UpstreamFormat::OpenAiResponses,
            UpstreamFormat::Anthropic,
            UpstreamFormat::Gemini,
        ] {
            let fixture = Fixture::new();
            let (url, mut captures, task) = mock(format, StatusCode::OK, false, false, None).await;
            let (base, token) = setup(&fixture, &url, format).await;
            let client = Client::builder().no_proxy().build().unwrap();
            for protocol in [
                Protocol::Chat,
                Protocol::Responses,
                Protocol::Claude,
                Protocol::Gemini,
            ] {
                for streaming in [false, true] {
                    let (path, body) = input(protocol, streaming);
                    let response = client
                        .post(format!("{base}{path}"))
                        .bearer_auth(&token)
                        .json(&body)
                        .send()
                        .await
                        .unwrap();
                    assert_eq!(response.status(), 200);
                    if streaming {
                        let text = response.text().await.unwrap();
                        assert!(text.contains("中文🙂"), "{text}");
                        assert!(text.contains("lookup"), "{text}");
                        assert!(text.contains("杭州"), "{text}");
                        assert!(text.contains("reported-model"), "{text}");
                        assert!(text.contains('6'), "{text}");
                        assert!(!text.contains("gateway_error"), "{text}");
                        if format == UpstreamFormat::OpenAiResponses {
                            assert!(text.contains("可见思考🙂"), "{text}");
                            if protocol != Protocol::Responses {
                                assert!(!text.contains("opaque-reasoning"), "{text}");
                            }
                        }
                    } else {
                        let value = response.json::<Value>().await.unwrap();
                        let parsed = parse_upstream_response(
                            &value,
                            match protocol {
                                Protocol::Chat => UpstreamFormat::OpenAiChat,
                                Protocol::Responses => UpstreamFormat::OpenAiResponses,
                                Protocol::Claude => UpstreamFormat::Anthropic,
                                Protocol::Gemini => UpstreamFormat::Gemini,
                            },
                        )
                        .unwrap();
                        assert_eq!(parsed.text, " 中文🙂 ");
                        assert_eq!(parsed.tool_calls[0]["function"]["name"], "lookup");
                        assert_eq!(
                            serde_json::from_str::<Value>(
                                parsed.tool_calls[0]["function"]["arguments"]
                                    .as_str()
                                    .unwrap()
                            )
                            .unwrap()["city"],
                            "杭州"
                        );
                        assert_eq!(parsed.input_tokens, 12);
                        assert_eq!(parsed.output_tokens, 6);
                        if format == UpstreamFormat::OpenAiResponses {
                            assert_eq!(parsed.reasoning, "可见思考🙂");
                        }
                    }
                    let (path, headers, body) = captures.recv().await.unwrap();
                    assert!(!headers
                        .values()
                        .any(|value| value.as_bytes() == token.as_bytes()));
                    assert!(body["tools"].is_array());
                    if format != UpstreamFormat::Gemini {
                        assert_eq!(body["model"], "real-model");
                    } else {
                        assert!(path.contains("/real-model:"));
                    }
                }
            }
            let state = read_state(&fixture.paths).unwrap();
            assert_eq!(state["usage"]["requestCount"], 8);
            assert_eq!(state["usage"]["failureCount"], 0);
            assert_eq!(state["usage"]["inputTokens"], 96);
            assert_eq!(state["usage"]["outputTokens"], 48);
            assert_eq!(state["logs"][0]["requestedModel"], "alias");
            assert_eq!(state["logs"][0]["physicalModel"], "real-model");
            assert_eq!(state["logs"][0]["upstreamModel"], "reported-model");
            assert_eq!(state["logs"][0]["providerId"], "provider-test");
            assert!(!state.to_string().contains("private-upstream-key"));
            for log in state["logs"].as_array().unwrap() {
                assert_eq!(log["clientIp"], "127.0.0.1");
                assert!(log.get("requestBody").is_none());
                assert!(log.get("responseBody").is_none());
                let detail = read_log_detail(&fixture.paths, &json!({"logId":log["id"]})).unwrap();
                let request: Value =
                    serde_json::from_str(detail["requestBody"].as_str().unwrap()).unwrap();
                assert!(request.is_object());
                let forwarded: Value =
                    serde_json::from_str(detail["upstreamRequestBody"].as_str().unwrap()).unwrap();
                if format != UpstreamFormat::Gemini {
                    assert_eq!(forwarded["model"], "real-model");
                }
                assert!(detail["upstreamResponseBody"]
                    .as_str()
                    .unwrap()
                    .contains("reported-model"));
                assert!(detail["responseBody"]
                    .as_str()
                    .unwrap()
                    .contains("reported-model"));
                assert!(!detail.to_string().contains("private-upstream-key"));
                assert!(!detail.to_string().contains(&token));
                assert!(gateway_log_dir(&fixture.paths)
                    .join(format!("{}.json", log["id"].as_str().unwrap()))
                    .is_file());
            }
            let index = std::fs::read_to_string(gateway_log_dir(&fixture.paths).join("index.json"))
                .unwrap();
            assert!(!index.contains("requestBody"));
            assert!(!index.contains("可见思考"));
            fixture.registry.stop().await;
            task.abort();
        }
    });
}

#[test]
fn request_details_keep_large_bodies_outside_the_summary_store() {
    run(async {
        let fixture = Fixture::new();
        let question = format!("请求开始{}请求末尾", "完整中文🙂".repeat(20_000));
        let answer = format!("响应开始{}响应末尾", "完整响应🙂".repeat(20_000));
        assert!(question.len() > 128 * 1024);
        assert!(answer.len() > 128 * 1024);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let expected_question = question.clone();
        let upstream_answer = answer.clone();
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            while let Ok((socket, _)) = listener.accept().await {
                let question = expected_question.clone();
                let answer = upstream_answer.clone();
                connections.spawn(async move {
                    let service = service_fn(move |request: Request<Incoming>| {
                        let question = question.clone();
                        let answer = answer.clone();
                        async move {
                            let bytes = request.into_body().collect().await.unwrap().to_bytes();
                            let value: Value = serde_json::from_slice(&bytes).unwrap();
                            assert_eq!(value["messages"][0]["content"], question);
                            Ok::<_, Infallible>(json_response(StatusCode::OK, json!({
                                "model":"large-reported-model",
                                "choices":[{"index":0,"message":{"role":"assistant","content":answer},"finish_reason":"stop"}],
                                "usage":{"prompt_tokens":42,"completion_tokens":84}
                            })))
                        }
                    });
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(socket), service).await;
                });
            }
        });
        let (base, token) = setup(&fixture, &url, UpstreamFormat::OpenAiChat).await;
        let client = Client::builder().no_proxy().build().unwrap();
        for streaming in [false, true] {
            let request = json!({"model":"alias","stream":streaming,"messages":[{"role":"user","content":question}]});
            let response = client
                .post(format!("{base}/v1/chat/completions"))
                .bearer_auth(&token)
                .header("x-forwarded-for", "203.0.113.9")
                .json(&request)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let received = response.text().await.unwrap();
            assert!(received.contains("响应末尾"));
            let state = read_state(&fixture.paths).unwrap();
            let summary = &state["logs"][0];
            assert_eq!(summary["clientIp"], "127.0.0.1");
            assert!(summary.get("requestBody").is_none());
            let detail = read_log_detail(&fixture.paths, &json!({"logId":summary["id"]})).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(detail["requestBody"].as_str().unwrap()).unwrap(),
                request
            );
            let forwarded: Value =
                serde_json::from_str(detail["upstreamRequestBody"].as_str().unwrap()).unwrap();
            assert_eq!(forwarded["messages"][0]["content"], question);
            let upstream: Value =
                serde_json::from_str(detail["upstreamResponseBody"].as_str().unwrap()).unwrap();
            assert_eq!(upstream["choices"][0]["message"]["content"], answer);
            assert_eq!(detail["responseBody"], received);
            assert_eq!(detail["upstreamModel"], "large-reported-model");
            assert_eq!(detail["inputTokens"], 42);
            assert!(!detail.to_string().contains(&token));
        }
        let index =
            std::fs::read_to_string(gateway_log_dir(&fixture.paths).join("index.json")).unwrap();
        assert!(!index.contains("请求开始"));
        assert!(!index.contains("响应开始"));
        assert!(
            read_log_detail(&fixture.paths, &json!({"logId":"../storage/ai-manager.db"})).is_err()
        );
        assert!(read_log_detail(
            &fixture.paths,
            &json!({"logId":uuid::Uuid::new_v4().to_string()})
        )
        .is_err());
        fixture.registry.stop().await;
        task.abort();
    });
}

#[test]
fn token_keys_are_recoverable_without_leaking_into_public_state() {
    run(async {
        let fixture = Fixture::new();
        let created = create_token(
            &fixture.registry,
            &fixture.paths,
            json!({"name":"可重复读取的密钥"}),
        )
        .await
        .unwrap();
        let id = created["createdToken"]["id"].clone();
        let raw = created["createdToken"]["token"].as_str().unwrap();
        assert!(raw.starts_with("sk-"));
        assert_eq!(raw.len(), 35);
        let public = read_state(&fixture.paths).unwrap();
        assert_eq!(public["tokens"][0]["canReveal"], true);
        assert!(public["tokens"][0]["maskedKey"]
            .as_str()
            .unwrap()
            .starts_with(&raw[..8]));
        for hidden in [raw, "tokenEncrypted", "tokenHash"] {
            assert!(!public.to_string().contains(hidden));
        }
        let disk = std::fs::read_to_string(&fixture.paths.storage_files.gateway_config).unwrap();
        assert!(!disk.contains(raw));
        let config = read_config(&fixture.paths).unwrap();
        assert_eq!(
            runtime_provider::decrypt_provider_key(
                config["tokens"][0]["tokenEncrypted"].as_str().unwrap()
            )
            .unwrap(),
            raw
        );
        // 新建管理对象并重新读取磁盘后，仍可取回完整密钥，停用只影响调用权限。
        let registry = GatewayServerRegistry::new();
        set_token_enabled(
            &registry,
            &fixture.paths,
            json!({"tokenId":id,"enabled":false}),
        )
        .await
        .unwrap();
        assert_eq!(
            read_token_value(&fixture.paths, &json!({"tokenId":id})).unwrap()["apiKey"],
            raw
        );
        assert!(read_token_value(&fixture.paths, &json!({})).is_err());
        assert!(read_token_value(&fixture.paths, &json!({"tokenId":"missing"})).is_err());

        let legacy_raw = "mpg_legacy-token-without-recoverable-storage";
        let mut config = read_config(&fixture.paths).unwrap();
        config["tokens"].as_array_mut().unwrap().push(json!({
            "id":"legacy","name":"旧 Token","enabled":true,
            "tokenHash":hash_token(legacy_raw),"tokenHint":token_hint(legacy_raw)
        }));
        write_config(&fixture.paths, &config).await.unwrap();
        let before = read_config(&fixture.paths).unwrap()["tokens"][1].clone();
        let error = read_token_value(&fixture.paths, &json!({"tokenId":"legacy"})).unwrap_err();
        assert!(error.to_string().contains("无法还原"));
        assert_eq!(
            read_state(&fixture.paths).unwrap()["tokens"][1]["canReveal"],
            false
        );
        assert_eq!(read_config(&fixture.paths).unwrap()["tokens"][1], before);
        let port = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let state = enable(&registry, &fixture.paths, &json!({}), json!({"port":port}))
            .await
            .unwrap();
        let response = Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("{}/v1/models", state["baseUrl"].as_str().unwrap()))
            .bearer_auth(legacy_raw)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        delete_token(&registry, &fixture.paths, json!({"tokenId":id}))
            .await
            .unwrap();
        assert!(read_token_value(&fixture.paths, &json!({"tokenId":id})).is_err());
        registry.stop().await;
    });
}

#[test]
fn auth_routing_http_errors_and_port_conflict_are_honest() {
    run(async {
        let fixture = Fixture::new();
        let (url, _captures, task) = mock(
            UpstreamFormat::OpenAiChat,
            StatusCode::TOO_MANY_REQUESTS,
            false,
            false,
            None,
        )
        .await;
        let (base, token) = setup(&fixture, &url, UpstreamFormat::OpenAiChat).await;
        let client = Client::builder().no_proxy().build().unwrap();
        assert_eq!(
            client
                .get(format!("{base}/v1/models"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        assert_eq!(
            client
                .get(format!("{base}/v1/models"))
                .bearer_auth("bad")
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        let models = client
            .get(format!("{base}/v1/models"))
            .header("x-api-key", &token)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert!(models["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["id"] == "alias"));
        let id = read_state(&fixture.paths).unwrap()["tokens"][0]["id"].clone();
        set_token_enabled(
            &fixture.registry,
            &fixture.paths,
            json!({"tokenId":id,"enabled":false}),
        )
        .await
        .unwrap();
        assert_eq!(
            client
                .get(format!("{base}/v1/models"))
                .bearer_auth(&token)
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        set_token_enabled(
            &fixture.registry,
            &fixture.paths,
            json!({"tokenId":id,"enabled":true}),
        )
        .await
        .unwrap();
        let expired = create_token(&fixture.registry, &fixture.paths, json!({"expiresAt":1}))
            .await
            .unwrap();
        assert_eq!(
            client
                .get(format!("{base}/v1/models"))
                .bearer_auth(expired["createdToken"]["token"].as_str().unwrap())
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        let (path, body) = input(Protocol::Chat, false);
        let response = client
            .post(format!("{base}{path}"))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 429);
        assert!(!response.text().await.unwrap().contains("upstream-secret"));
        assert_eq!(
            read_state(&fixture.paths).unwrap()["logs"][0]["statusCode"],
            429
        );
        let failed = read_state(&fixture.paths).unwrap()["logs"][0].clone();
        let detail = read_log_detail(&fixture.paths, &json!({"logId":failed["id"]})).unwrap();
        assert!(detail["upstreamResponseBody"]
            .as_str()
            .unwrap()
            .contains("upstream-secret"));
        assert!(!detail["responseBody"]
            .as_str()
            .unwrap()
            .contains("upstream-secret"));
        let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
        assert!(save_config(
            &fixture.registry,
            &fixture.paths,
            &json!({}),
            json!({"port":occupied.local_addr().unwrap().port()})
        )
        .await
        .is_err());
        assert_eq!(read_state(&fixture.paths).unwrap()["baseUrl"], base);
        save_config(
            &fixture.registry,
            &fixture.paths,
            &json!({}),
            json!({"providerIds":[],"modelMappings":{}}),
        )
        .await
        .unwrap();
        assert_eq!(
            client
                .post(format!("{base}{path}"))
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            503
        );
        assert_eq!(
            read_state(&fixture.paths).unwrap()["usage"]["failureCount"],
            2
        );
        let state = read_state(&fixture.paths).unwrap().to_string();
        assert!(!state.contains(&token));
        assert!(!state.contains("tokenHash"));
        let disk = std::fs::read_to_string(&fixture.paths.storage_files.gateway_config).unwrap();
        assert!(!disk.contains(&token));
        delete_token(&fixture.registry, &fixture.paths, json!({"tokenId":id}))
            .await
            .unwrap();
        assert_eq!(
            read_state(&fixture.paths).unwrap()["usage"]["requestCount"],
            2
        );
        fixture.registry.stop().await;
        task.abort();
    });
}

#[test]
fn sse_is_incremental_and_official_style_sse_can_be_collected_as_json() {
    run(async {
        let fixture = Fixture::new();
        let gate = Arc::new(tokio::sync::Notify::new());
        let (url, _captures, task) = mock(
            UpstreamFormat::OpenAiChat,
            StatusCode::OK,
            true,
            false,
            Some(gate.clone()),
        )
        .await;
        let (base, token) = setup(&fixture, &url, UpstreamFormat::OpenAiChat).await;
        let (path, body) = input(Protocol::Responses, true);
        let client = Client::builder().no_proxy().build().unwrap();
        let mut stream = client
            .post(format!("{base}{path}"))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .unwrap()
            .bytes_stream();
        let first = tokio::time::timeout(std::time::Duration::from_secs(3), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!first.is_empty());
        assert_eq!(
            read_state(&fixture.paths).unwrap()["usage"]["requestCount"],
            0
        );
        gate.notify_one();
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
        }
        fixture.registry.stop().await;
        task.abort();

        let fixture = Fixture::new();
        let (url, _captures, task) = mock(
            UpstreamFormat::OpenAiResponses,
            StatusCode::OK,
            true,
            false,
            None,
        )
        .await;
        let (base, token) = setup(&fixture, &url, UpstreamFormat::OpenAiResponses).await;
        let (path, body) = input(Protocol::Chat, false);
        let response = client
            .post(format!("{base}{path}"))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(response["choices"][0]["message"]["content"], " 中文🙂 ");
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "lookup"
        );
        assert_eq!(
            read_state(&fixture.paths).unwrap()["usage"]["requestCount"],
            1
        );
        fixture.registry.stop().await;
        task.abort();
    });
}

#[test]
fn concurrent_usage_does_not_overwrite_token_management() {
    run(async {
        let fixture = Fixture::new();
        let token = create_token(
            &fixture.registry,
            &fixture.paths,
            json!({"name":"并发调用"}),
        )
        .await
        .unwrap();
        let id = token["createdToken"]["id"].clone();
        let context = GatewayContext {
            paths: fixture.paths.clone(),
            cli_targets: json!({}),
            store_lock: fixture.registry.store_lock.clone(),
        };
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..20 {
            let context = context.clone();
            let id = id.clone();
            tasks.spawn(async move {
                update_usage(
                    &context,
                    json!({"id":uuid::Uuid::new_v4().to_string(),"tokenId":id,"ok":true,"inputTokens":3,"outputTokens":2}),
                )
                .await
                .unwrap();
            });
        }
        create_token(
            &fixture.registry,
            &fixture.paths,
            json!({"name":"同时创建"}),
        )
        .await
        .unwrap();
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
        let state = read_state(&fixture.paths).unwrap();
        assert_eq!(state["tokens"].as_array().unwrap().len(), 2);
        assert_eq!(state["usage"]["requestCount"], 20);
        assert_eq!(state["tokens"][0]["inputTokens"], 60);
        assert_eq!(state["tokens"][0]["outputTokens"], 40);
        assert_eq!(state["logs"].as_array().unwrap().len(), 20);
    });
}

#[test]
fn truncated_stream_and_client_disconnect_are_logged_as_failures() {
    run(async {
        let client = Client::builder().no_proxy().build().unwrap();
        for disconnect in [false, true] {
            let fixture = Fixture::new();
            let gate = Arc::new(tokio::sync::Notify::new());
            let (url, _captures, task) = mock(
                UpstreamFormat::OpenAiChat,
                StatusCode::OK,
                true,
                !disconnect,
                if disconnect { Some(gate.clone()) } else { None },
            )
            .await;
            let (base, token) = setup(&fixture, &url, UpstreamFormat::OpenAiChat).await;
            let (path, body) = input(Protocol::Responses, true);
            let response = client
                .post(format!("{base}{path}"))
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await
                .unwrap();
            if disconnect {
                let mut stream = response.bytes_stream();
                tokio::time::timeout(std::time::Duration::from_secs(3), stream.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                drop(stream);
            } else {
                assert!(response.text().await.unwrap().contains("response.failed"));
            }
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while read_state(&fixture.paths).unwrap()["usage"]["requestCount"] == 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let state = read_state(&fixture.paths).unwrap();
            assert_eq!(state["usage"]["failureCount"], 1);
            assert_eq!(state["logs"][0]["ok"], false);
            assert_eq!(state["logs"][0]["statusCode"], 502);
            gate.notify_one();
            fixture.registry.stop().await;
            task.abort();
        }
    });
}

#[test]
fn account_identity_and_pool_membership_are_preserved() {
    let fixture = Fixture::new();
    provider_store::write_codex_accounts(
        &fixture.paths,
        &[json!({"id":"official-one","email":"test@example.com","disabled":false})],
    )
    .unwrap();
    let config = normalize_config(
        &json!({"providerIds":["account:official-one"],"modelMappings":{"alias":{"targetId":"account:official-one","model":"physical"}}}),
    );
    let target = select_target(&fixture.paths, &config, &json!({}), "alias").unwrap();
    assert_eq!(target.id, "account:official-one");
    assert_eq!(mapped_model(&config, &target.id, "alias"), "physical");
    assert!(target.kind == TargetKind::CodexAccount);
    let mut invalid = config;
    invalid["providerIds"] = json!([]);
    assert!(select_target(&fixture.paths, &invalid, &json!({}), "alias").is_err());
}

#[test]
fn tool_history_and_multimodal_boundaries_are_explicit() {
    let route = GatewayRoute {
        protocol: Protocol::Responses,
        stream: false,
        requested_model: String::new(),
    };
    let request = canonical_request(&route,&json!({"model":"alias","input":[{"type":"function_call","call_id":"call_one","name":"lookup","arguments":"{}"},{"type":"function_call_output","call_id":"call_one","output":"结果"}]})).unwrap();
    let claude = upstream_body("real", &request, UpstreamFormat::Anthropic).unwrap();
    assert_eq!(claude["messages"][0]["content"][0]["id"], "call_one");
    assert_eq!(
        claude["messages"][1]["content"][0]["tool_use_id"],
        "call_one"
    );
    let google = upstream_body("real", &request, UpstreamFormat::Gemini).unwrap();
    assert!(google.to_string().contains("functionResponse"));
    let request = canonical_request(&route,&json!({"model":"alias","input":[{"role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,abc"}]}]})).unwrap();
    assert!(upstream_body("real", &request, UpstreamFormat::OpenAiChat).is_err());
    assert_eq!(
        upstream_body("real", &request, UpstreamFormat::OpenAiResponses).unwrap()["input"][0]
            ["content"][0]["type"],
        "input_image"
    );
    let native = json!({"model":"claude","messages":[{"role":"user","content":"查询"}],"tools":[{"type":"web_search_20250305","name":"web_search"}]});
    let request = canonical_claude(&native).unwrap();
    assert_eq!(
        upstream_body("real", &request, UpstreamFormat::Anthropic).unwrap()["tools"],
        native["tools"]
    );
    assert!(upstream_body("real", &request, UpstreamFormat::OpenAiChat).is_err());
    let history = json!({"contents":[
        {"role":"model","parts":[{"functionCall":{"name":"lookup","args":{"city":"杭州"}}},{"functionCall":{"name":"lookup","args":{"city":"上海"}}}]},
        {"role":"user","parts":[{"functionResponse":{"name":"lookup","response":{"value":1}}},{"functionResponse":{"name":"lookup","response":{"value":2}}}]}
    ]});
    let request = canonical_gemini(&history, "model").unwrap();
    let calls = request.messages[0]["tool_calls"].as_array().unwrap();
    assert_ne!(calls[0]["id"], calls[1]["id"]);
    assert_eq!(request.messages[1]["tool_call_id"], calls[0]["id"]);
    assert_eq!(request.messages[2]["tool_call_id"], calls[1]["id"]);
}

#[test]
fn canonical_chat_preserves_tools_and_limits() {
    let value = canonical_chat(&json!({
        "model": "alias",
        "messages": [{"role": "user", "content": "hello"}],
        "tools": [{"type": "function"}],
        "max_tokens": 32,
        "stream": true
    }))
    .unwrap();
    assert_eq!(value.requested_model, "alias");
    assert_eq!(value.messages.len(), 1);
    assert_eq!(value.tools.len(), 1);
    assert_eq!(value.max_tokens, Some(json!(32)));
    assert!(value.stream);
}

#[test]
fn config_normalization_deduplicates_pool() {
    let config = normalize_config(&json!({
        "enabled": true,
        "providerIds": ["a", "a", "", "b"],
        "port": 8047
    }));
    assert_eq!(config["providerIds"], json!(["a", "b"]));
    assert_eq!(config["port"], 8047);
    assert_eq!(config["tokens"], json!([]));
}

#[test]
fn usage_aggregates_cache_tokens_and_client_ips() {
    let logs = vec![
        json!({
            "id": "one",
            "ok": true,
            "usageKnown": true,
            "inputTokens": 100,
            "outputTokens": 20,
            "cacheInputTokens": 100,
            "cacheReadTokens": 40,
            "tokenId": "token-a",
            "tokenName": "桌面客户端",
            "providerId": "provider-a",
            "providerName": "Provider A",
            "physicalModel": "model-a",
            "clientIp": "192.168.1.20"
        }),
        json!({
            "id": "two",
            "ok": true,
            "usageKnown": true,
            "inputTokens": 80,
            "outputTokens": 10,
            "cacheInputTokens": 80,
            "cacheReadTokens": 0,
            "tokenId": "token-a",
            "tokenName": "桌面客户端",
            "providerId": "provider-a",
            "providerName": "Provider A",
            "physicalModel": "model-a",
            "clientIp": "192.168.1.20"
        }),
        json!({
            "id": "three",
            "ok": false,
            "usageKnown": false,
            "inputTokens": 0,
            "outputTokens": 0,
            "tokenId": "token-b",
            "tokenName": "脚本调用",
            "clientIp": "10.0.0.8"
        }),
    ];
    let summary = usage_summary(&logs);
    assert_eq!(summary["requestCount"], 3);
    assert_eq!(summary["successCount"], 2);
    assert_eq!(summary["cacheReadTokens"], 40);
    assert_eq!(summary["cacheInputTokens"], 180);
    assert_eq!(summary["cacheHitRequestCount"], 1);
    assert_eq!(summary["cacheMeasuredRequestCount"], 2);

    let by_ip = aggregate_logs(&logs, "clientIp");
    let lan_ip = by_ip
        .iter()
        .find(|item| item["key"] == "192.168.1.20")
        .unwrap();
    assert_eq!(lan_ip["requestCount"], 2);
    assert_eq!(lan_ip["cacheReadTokens"], 40);
    assert!(by_ip.iter().any(|item| item["key"] == "10.0.0.8"));
}

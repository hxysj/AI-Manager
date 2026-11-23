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
                            if let Some(gate) = &gate {
                                gate.notified().await;
                            }
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

async fn service_setup(fixture:&Fixture,url:&str,format:UpstreamFormat)->(String,String,String,tokio::task::JoinHandle<()>){
    use crate::core::{database,service_provider_store as store};
    store::initialize(&fixture.paths).unwrap();
    let provider=store::save(&fixture.paths,&json!({"name":"共享测试","baseUrl":url,"apiKey":"private-upstream-key",
        "upstreamProtocol":if format==UpstreamFormat::OpenAiChat{"chat"}else{"responses"},
        "models":["real-model","alias"],"bindings":[{"agentType":"gateway","enabled":true}]})).unwrap()["id"].as_str().unwrap().to_string();
    let c=database::open(&fixture.paths).unwrap();
    let model:String=c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id='real-model'",[&provider],|r|r.get(0)).unwrap();
    c.execute("INSERT INTO service_agent_model_routes(id,provider_id,agent_type,client_model_name,model_id)
        VALUES('alias',?1,'gateway','alias',?2)",rusqlite::params![provider,model]).unwrap();
    crate::api::service_provider::switch_backend(&fixture.paths,"service").unwrap();
    let token=crate::api::service_provider::issue_token(&fixture.paths,&json!({"providerId":provider,"modelId":model})).unwrap()["token"].as_str().unwrap().to_string();
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base=format!("http://{}",listener.local_addr().unwrap());
    let server=spawn_listener(listener,GatewayContext{paths:fixture.paths.clone(),cli_targets:json!([]),
        store_lock:fixture.registry.store_lock.clone(),service_mode:true,legacy_request:None,_service_request:None});
    (base,token,provider,server)
}

#[test]
fn service_gateway_transmits_reasoning_and_enforces_limits_before_forwarding() {
    run(async {
        use crate::core::{database, service_provider_store as store};
        let fixture = Fixture::new();
        let (url, mut captures, upstream) =
            mock(UpstreamFormat::OpenAiChat, StatusCode::OK, false, false, None).await;
        let (base, _, provider, server) = service_setup(&fixture, &url, UpstreamFormat::OpenAiChat).await;
        let c = database::open(&fixture.paths).unwrap();
        let model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id='real-model'",
            [&provider], |row| row.get(0)).unwrap();
        let issued = crate::api::service_provider::issue_token(&fixture.paths,
            &json!({"providerId":provider,"modelId":model,"reasoningEffortLimit":"medium"})).unwrap();
        let token = issued["token"].as_str().unwrap();
        let client = Client::builder().no_proxy().build().unwrap();
        let endpoint = format!("{base}/v1/responses");
        let allowed = client.post(&endpoint).bearer_auth(token)
            .json(&json!({"model":"alias","input":"test","reasoning":{"effort":"medium"}}))
            .send().await.unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        allowed.bytes().await.unwrap();
        let (path, _, sent) = captures.try_recv().unwrap();
        assert!(path.ends_with("/chat/completions"));
        assert_eq!(sent["model"], "real-model");
        assert_eq!(sent["reasoning_effort"], "medium");
        assert!(sent.get("reasoning").is_none());
        let suffixed = client.post(&endpoint).bearer_auth(token)
            .json(&json!({"model":"alias(8192)","input":"test","reasoning":{"effort":"low"}}))
            .send().await.unwrap();
        assert_eq!(suffixed.status(), StatusCode::OK);
        suffixed.bytes().await.unwrap();
        assert_eq!(captures.try_recv().unwrap().2["reasoning_effort"], "medium");
        let gemini = client.post(format!("{base}/v1beta/models/alias:generateContent"))
            .header("x-goog-api-key",token)
            .json(&json!({"contents":[{"role":"user","parts":[{"text":"test"}]}],
                "generationConfig":{"thinkingConfig":{"thinkingBudget":8192}}}))
            .send().await.unwrap();
        assert_eq!(gemini.status(), StatusCode::OK);
        gemini.bytes().await.unwrap();
        assert_eq!(captures.try_recv().unwrap().2["reasoning_effort"], "medium");
        for (reasoning, code) in [(json!({"effort":"high"}), "reasoning_effort_exceeds_key_limit"),
            (Value::Null, "reasoning_effort_unverifiable")] {
            let rejected = client.post(&endpoint).bearer_auth(token)
                .json(&json!({"model":"alias","input":"test","reasoning":reasoning})).send().await.unwrap();
            assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
            assert!(rejected.text().await.unwrap().contains(code));
        }
        assert!(captures.try_recv().is_err());
        let history = store::usage(&c).unwrap();
        assert!(history["logs"].as_array().unwrap().iter().any(|log| log["requestedReasoningEffort"] == "medium"
            && log["sentReasoningEffort"] == "medium"));
        server.abort();
        upstream.abort();
    });
}

#[test]
fn service_claude_to_openai_preserves_instruction_roles_attachments_and_tools() {
    run(async {
        for format in [UpstreamFormat::OpenAiChat, UpstreamFormat::OpenAiResponses] {
            let fixture = Fixture::new();
            let (url, mut captures, upstream) = mock(format, StatusCode::OK, false, false, None).await;
            let (base, token, _, server) = service_setup(&fixture, &url, format).await;
            let client = Client::builder().no_proxy().build().unwrap();
            for stream in [false, true] {
                let response = client.post(format!("{base}/v1/messages")).bearer_auth(&token).json(&json!({
                    "model":"alias","stream":stream,"max_tokens":512,
                    "system":"顶层指令",
                    "messages":[
                        {"role":"system","content":"系统指令"},
                        {"role":"developer","content":[{"type":"text","text":"开发指令"}]},
                        {"role":"user","content":[{"type":"text","text":"解释源码"},
                            {"type":"document","source":{"type":"text","data":"export const homework = 1"}},
                            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}}]},
                        {"role":"assistant","content":[{"type":"tool_use","id":"call_one","name":"lookup","input":{"value":"ping"}}]},
                        {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_one","content":"pong"}]}
                    ],
                    "tools":[{"name":"lookup","input_schema":{"type":"object","properties":{"value":{"type":"string"}}}}],
                    "tool_choice":{"type":"any","disable_parallel_tool_use":true}
                })).send().await.unwrap();
                let status = response.status();
                let result = response.text().await.unwrap();
                assert_eq!(status, StatusCode::OK, "{result}");
                assert!(result.contains(if stream { "message_stop" } else { "tool_use" }), "{result}");
                let (path, _, body) = captures.recv().await.unwrap();
                let responses = format == UpstreamFormat::OpenAiResponses;
                assert!(path.ends_with(if responses { "/responses" } else { "/chat/completions" }));
                assert_eq!(body["model"], "real-model");
                let messages = &body[if responses { "input" } else { "messages" }];
                assert_eq!(messages[1]["role"], "system");
                assert_eq!(messages[2]["role"], "developer");
                assert_eq!(messages[3]["content"][1]["text"], "export const homework = 1");
                assert_eq!(messages[3]["content"][2]["type"], if responses { "input_image" } else { "image_url" });
                assert_eq!(body["tool_choice"], "required");
                assert_eq!(body["parallel_tool_calls"], false);
                if responses {
                    assert_eq!(messages[4]["type"], "function_call");
                    assert_eq!(messages[5]["type"], "function_call_output");
                    assert_eq!(messages[4]["call_id"], messages[5]["call_id"]);
                } else {
                    assert_eq!(messages[4]["tool_calls"][0]["id"], messages[5]["tool_call_id"]);
                }
            }
            server.abort();
            upstream.abort();
        }
    });
}

#[test]
fn service_google_account_routes_through_shared_gateway_and_keeps_usage() {
    run(async {
        use crate::core::{database, service_provider_store as store};
        let fixture = Fixture::new();
        store::initialize(&fixture.paths).unwrap();
        let provider = store::save(&fixture.paths,&json!({"name":"Google 共享账号","kind":"google_account",
            "models":["gemini-test"],"bindings":[{"agentType":"gateway","enabled":true}]})).unwrap()["id"].as_str().unwrap().to_owned();
        let c = database::open(&fixture.paths).unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        store::save_google_credentials(&c,&provider,
            &json!({"subject":"google-test","email":"fixture@example.test","port":port,"project":"test",
                "models":[{"id":"gemini-test"}]}),
            &json!({"access_token":"private-google-access","refresh_token":"private-google-refresh","expires_at":9999999999i64})).unwrap();
        crate::api::service_provider::switch_backend(&fixture.paths,"service").unwrap();
        let model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",[&provider],|row|row.get(0)).unwrap();
        let token = crate::api::service_provider::issue_token(&fixture.paths,&json!({"providerId":provider,"modelId":model})).unwrap()["token"].as_str().unwrap().to_owned();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}",listener.local_addr().unwrap());
        let server = spawn_listener(listener,GatewayContext{paths:fixture.paths.clone(),cli_targets:json!([]),
            store_lock:fixture.registry.store_lock.clone(),service_mode:true,legacy_request:None,_service_request:None});
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        // background 在本地 Google 适配器中拒绝，验证路由和失败记账，不访问外部接口。
        let response = client.post(format!("{base}/v1/responses")).bearer_auth(&token)
            .json(&json!({"model":"gemini-test","input":"本地校验","background":true,"stream":false}))
            .send().await.unwrap();
        let status = response.status();
        let result: Value = response.json().await.unwrap();
        assert_eq!(status,StatusCode::BAD_REQUEST,"{result}");
        let usage = store::state(&fixture.paths).unwrap()["usage"].clone();
        assert_eq!(usage["attempts"]["attemptCount"],1);
        assert_eq!(usage["requests"]["failureCount"],1);
        assert_eq!(usage["logs"][0]["providerId"],provider);
        assert_eq!(usage["logs"][0]["credentialName"],"fixture@example.test");
        let models = client.get(format!("http://127.0.0.1:{port}/v1/models"))
            .bearer_auth(store::google_local_key(&c,&provider).unwrap()).send().await.unwrap();
        assert_eq!(models.status(),StatusCode::OK);
        assert_eq!(models.json::<Value>().await.unwrap()["data"][0]["id"],"gemini-test");
        let denied = client.get(format!("http://127.0.0.1:{port}/v1/models"))
            .bearer_auth("private-google-access").send().await.unwrap();
        assert_eq!(denied.status(),StatusCode::UNAUTHORIZED);
        assert!(!store::state(&fixture.paths).unwrap().to_string().contains("private-google"));
        server.abort();
        crate::api::google_gateway::stop(&fixture.paths,&provider).await;
    });
}

#[test]
fn service_desktop_model_rules_resolve_safe_aliases_and_reject_disabled_targets() {
    use crate::core::{database, service_provider_store as store};
    let fixture = Fixture::new();
    store::initialize(&fixture.paths).unwrap();
    let provider = store::save(&fixture.paths, &json!({
        "name":"Desktop 模型映射","baseUrl":"http://127.0.0.1:1/v1",
        "models":["default","mapped","override","claude-sonnet-4-6"],
        "bindings":[{"agentType":"claude-desktop","enabled":true,"defaultModelName":"default","options":{
            "modelMappings":{"claude-fable-5":{"model":"mapped","supports1m":true}}
        }}]
    })).unwrap()["id"].as_str().unwrap().to_string();
    let c = database::open(&fixture.paths).unwrap();
    let model_id = |name: &str| c.query_row(
        "SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id=?2",
        rusqlite::params![provider, name], |row| row.get::<_,String>(0)).unwrap();
    let identity = json!({"providerId":provider,"agentType":"claude-desktop","modelId":model_id("default")});
    for (alias, _, _) in crate::api::claude_desktop::ROUTES {
        assert_eq!(service::resolve(&fixture.paths, &identity, alias).unwrap().1["upstreamModelId"], "mapped");
    }
    assert_eq!(service::resolve(&fixture.paths, &identity, "").unwrap().1["upstreamModelId"], "default");
    assert!(service::resolve(&fixture.paths, &identity, "unknown").is_err());
    c.execute("INSERT INTO service_agent_model_routes(id,provider_id,agent_type,client_model_name,model_id)
        VALUES('explicit',?1,'claude-desktop','claude-sonnet-4-6',?2)",
        rusqlite::params![provider,model_id("override")]).unwrap();
    assert_eq!(service::resolve(&fixture.paths, &identity, "claude-sonnet-4-6").unwrap().1["upstreamModelId"], "override");
    c.execute("UPDATE service_provider_models SET enabled=0 WHERE id=?1", [model_id("override")]).unwrap();
    assert!(service::resolve(&fixture.paths, &identity, "claude-sonnet-4-6").is_err());
    c.execute("DELETE FROM service_agent_model_routes WHERE id='explicit'", []).unwrap();
    c.execute("UPDATE service_provider_models SET enabled=0 WHERE id=?1", [model_id("mapped")]).unwrap();
    assert!(service::resolve(&fixture.paths, &identity, "claude-sonnet-4-6").is_err());
    c.execute("UPDATE service_provider_agent_bindings SET options_json='{}' WHERE provider_id=?1", [&provider]).unwrap();
    assert_eq!(service::resolve(&fixture.paths, &identity, "claude-sonnet-4-6").unwrap().1["upstreamModelId"], "claude-sonnet-4-6");
}

#[test]
fn service_agent_model_selection_overrides_the_initial_model_without_falling_back() {
    use crate::core::{database, service_provider_store as store};
    let fixture = Fixture::new();
    let provider = store::save(&fixture.paths, &json!({
        "name":"Agent 模型切换","baseUrl":"http://127.0.0.1:1/v1",
        "models":["initial","selected","disabled"],
        "bindings":[{"agentType":"codex","enabled":true,"defaultModelName":"initial"},
            {"agentType":"claude","enabled":true,"defaultModelName":"initial"}]
    })).unwrap()["id"].as_str().unwrap().to_string();
    let c = database::open(&fixture.paths).unwrap();
    let initial: String = c.query_row("SELECT id FROM service_provider_models
        WHERE provider_id=?1 AND upstream_model_id='initial'", [&provider], |row| row.get(0)).unwrap();
    c.execute("UPDATE service_provider_models SET enabled=0
        WHERE provider_id=?1 AND upstream_model_id='disabled'", [&provider]).unwrap();
    for agent in ["codex", "claude"] {
        let identity = json!({"providerId":provider,"agentType":agent,"modelId":initial});
        assert_eq!(service::resolve(&fixture.paths, &identity, "").unwrap().1["upstreamModelId"], "initial");
        assert_eq!(service::resolve(&fixture.paths, &identity, "selected").unwrap().1["upstreamModelId"], "selected");
        assert!(service::resolve(&fixture.paths, &identity, "disabled").is_err());
        assert!(service::resolve(&fixture.paths, &identity, "missing").is_err());
        // 默认模型失效也不阻止用户切换到其他可用模型。
        c.execute("UPDATE service_provider_models SET enabled=0 WHERE id=?1", [&initial]).unwrap();
        assert_eq!(service::resolve(&fixture.paths, &identity, "selected").unwrap().1["upstreamModelId"], "selected");
        assert!(service::resolve(&fixture.paths, &identity, "").is_err());
        c.execute("UPDATE service_provider_models SET enabled=1 WHERE id=?1", [&initial]).unwrap();
    }
}

#[test]
fn client_keeps_one_token_while_provider_route_key_and_protocol_change() {
    run(async {
        use crate::api::service_provider;
        use crate::core::{database,service_provider_store as store};
        let fixture=Fixture::new();
        store::initialize(&fixture.paths).unwrap();
        let (first_url,mut first_requests,first_upstream)=mock(UpstreamFormat::OpenAiResponses,StatusCode::OK,false,false,None).await;
        let (second_url,mut second_requests,second_upstream)=mock(UpstreamFormat::OpenAiChat,StatusCode::OK,false,false,None).await;
        let create=|url:&str,protocol:&str,key:&str|store::save(&fixture.paths,&json!({"name":"测试来源","baseUrl":url,
            "upstreamProtocol":protocol,"apiKey":key,"models":["real-model"],
            "bindings":[{"agentType":"codex","enabled":true,"defaultModelName":"real-model"}]})).unwrap()["id"].as_str().unwrap().to_string();
        let first=create(&first_url,"responses","first-upstream-key");
        let second=create(&second_url,"chat","second-upstream-key");
        service_provider::switch_backend(&fixture.paths,"service").unwrap();
        let directory=fixture.root.join("new-parent").join("codex-client");
        let targets=json!([{"id":"codex","configPath":directory}]);
        let instance=service_provider::quick_switch(&fixture.paths,&targets,&json!({"agentType":"codex","providerId":first})).unwrap();
        let applied=toml_edit::de::from_str::<Value>(&std::fs::read_to_string(directory.join("config.toml")).unwrap()).unwrap();
        let original_token=applied["model_providers"]["ai_manager_service"]["experimental_bearer_token"].as_str().unwrap().to_string();
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let server=spawn_listener(listener,GatewayContext{paths:fixture.paths.clone(),cli_targets:targets.clone(),
            store_lock:fixture.registry.store_lock.clone(),service_mode:true,legacy_request:None,_service_request:None});
        let client=Client::builder().no_proxy().build().unwrap();
        let c=database::open(&fixture.paths).unwrap();
        if cfg!(windows) {
            // 模拟旧版本首次创建目录时未记录长路径前缀的快照，更新后仍应识别同一实例。
            for table in ["service_managed_files","service_agent_instances"] {
                let (column,owner)=if table=="service_managed_files" {("file_path","instance_id")} else {("config_path","id")};
                c.execute(&format!("UPDATE {table} SET {column}=substr({column},5) WHERE {owner}=?1 AND substr({column},1,4)=?2"),
                    rusqlite::params![store::text(&instance,"instanceId"),r"\\?\"]).unwrap();
            }
        }
        for step in 0..4 {
            if step==1 {
                let mut config=store::config(&c,&first).unwrap();
                config["name"]=json!("编辑服务商");
                config["baseUrl"]=json!(second_url);
                config["upstreamProtocol"]=json!("chat");
                config["apiKey"]=json!("rotated-upstream-key");
                store::save(&fixture.paths,&config).unwrap();
            } else if step==2 {
                service_provider::quick_switch(&fixture.paths,&targets,&json!({"agentType":"codex","providerId":second})).unwrap();
            } else if step==3 {
                service_provider::quick_switch(&fixture.paths,&targets,&json!({"agentType":"codex","providerId":first})).unwrap();
            }
            // 客户端一直发送首次启动时缓存的 Token，不重读配置，不重启。
            let response=client.post(format!("{base}/v1/responses")).bearer_auth(&original_token)
                .json(&json!({"model":"real-model","input":"hello","stream":false})).send().await.unwrap();
            let status=response.status();
            let body=response.text().await.unwrap();
            assert_eq!(status,StatusCode::OK,"step={step}: {body}");
            let (path,headers,_)=if step==0 {first_requests.recv().await.unwrap()} else {second_requests.recv().await.unwrap()};
            assert!(path.ends_with(if step==0 {"/responses"} else {"/chat/completions"}));
            assert_eq!(headers["authorization"],match step {0=>"Bearer first-upstream-key",2=>"Bearer second-upstream-key",_=>"Bearer rotated-upstream-key"});
            assert_eq!(store::query(&c,"SELECT count(*) AS count FROM service_gateway_tokens WHERE instance_id=?1",&[&store::text(&instance,"instanceId")]).unwrap()[0]["count"],1);
        }
        service_provider::detach(&fixture.paths,store::text(&instance,"instanceId")).unwrap();
        let response=client.post(format!("{base}/v1/responses")).bearer_auth(&original_token)
            .json(&json!({"model":"real-model","input":"hello"})).send().await.unwrap();
        assert_eq!(response.status(),StatusCode::UNAUTHORIZED);
        // 客户端仍缓存首次 Token，关闭、重启恢复、重新启用后直接恢复调用。
        service_provider::recover(&fixture.paths).unwrap();
        service_provider::quick_switch(&fixture.paths,&targets,&json!({"agentType":"codex","providerId":second})).unwrap();
        let response=client.post(format!("{base}/v1/responses")).bearer_auth(&original_token)
            .json(&json!({"model":"real-model","input":"resumed","stream":false})).send().await.unwrap();
        let status=response.status();
        let body=response.text().await.unwrap();
        assert_eq!(status,StatusCode::OK,"重新启用应接受原 Token：{body}");
        let (_,headers,_)=second_requests.recv().await.unwrap();
        assert_eq!(headers["authorization"],"Bearer second-upstream-key");
        assert_eq!(store::query(&c,"SELECT count(*) AS count FROM service_gateway_tokens WHERE instance_id=?1",&[&store::text(&instance,"instanceId")]).unwrap()[0]["count"],1);
        service_provider::detach(&fixture.paths,store::text(&instance,"instanceId")).unwrap();
        server.abort();first_upstream.abort();second_upstream.abort();
    });
}

#[test]
fn service_model_fetch_preserves_local_prices_and_rejects_stale_configuration() {
    run(async {
        use crate::core::{database, service_provider_store as store};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let fixture=Fixture::new();
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url=format!("http://{}/v1",listener.local_addr().unwrap());
        let saved=store::save(&fixture.paths,&json!({"name":"模型目录测试","baseUrl":url,"apiKey":"model-test-key",
            "models":[{"upstreamModelId":"real-model","enabled":false,"inputPriceMicros":2000000,"catalog":{"local":true}},"client-model"],
            "bindings":[{"agentType":"codex","enabled":true,"defaultModelName":"client-model"}]})).unwrap();
        let provider=store::text(&saved,"id").to_string();
        crate::api::service_provider::switch_backend(&fixture.paths,"service").unwrap();
        let directory=fixture.root.join("fetch-client");
        let targets=json!([{"id":"codex","configPath":directory}]);
        crate::api::service_provider::quick_switch(&fixture.paths,&targets,&json!({"agentType":"codex","providerId":provider})).unwrap();
        let applied=toml_edit::de::from_str::<Value>(&std::fs::read_to_string(directory.join("config.toml")).unwrap()).unwrap();
        let token=applied["model_providers"]["ai_manager_service"]["experimental_bearer_token"].as_str().unwrap().to_string();
        let (sent,mut received)=mpsc::unbounded_channel();
        let gate=Arc::new(tokio::sync::Notify::new());
        let release=gate.clone();
        let variant = Arc::new(AtomicUsize::new(0));
        let response_variant = variant.clone();
        let server=tokio::spawn(async move {
            loop {
                let (socket,_)=listener.accept().await.unwrap();
                let sent=sent.clone();
                let gate=gate.clone();
                let response_variant = response_variant.clone();
                tokio::spawn(async move {
                    let _=http1::Builder::new().serve_connection(TokioIo::new(socket),service_fn(move |request:Request<Incoming>| {
                        let sent=sent.clone();
                        let gate=gate.clone();
                        let variant = response_variant.load(Ordering::Relaxed);
                        async move {
                            assert_eq!(request.uri().path(),"/v1/models");
                            assert_eq!(request.headers()["authorization"],"Bearer model-test-key");
                            sent.send(()).unwrap();
                            gate.notified().await;
                            let data = match variant {
                                1 => json!([{"id":"real-model"},{"id":" REAL-MODEL "}]),
                                2 => json!([{"id":"real-model","supported_reasoning_levels":["low"],"default_reasoning_level":"high"}]),
                                _ => json!([{"id":"real-model","context_window":32000},{"id":"client-model"}]),
                            };
                            Ok::<_,Infallible>(json_response(StatusCode::OK,json!({"data":data})))
                        }
                    })).await;
                });
            }
        });
        for stale in [true,false] {
            let paths=fixture.paths.clone();
            let fetch_provider=provider.clone();
            let fetch=tokio::spawn(async move {service::fetch_models(&paths,&json!({"providerId":fetch_provider})).await});
            received.recv().await.unwrap();
            let paths=fixture.paths.clone();
            let concurrent_provider=provider.clone();
            let concurrent = tokio::spawn(async move {
                service::fetch_models(&paths, &json!({"providerId":concurrent_provider})).await
            });
            tokio::task::yield_now().await;
            if stale {
                let c=database::open(&fixture.paths).unwrap();
                let mut config=store::config(&c,&provider).unwrap();
                config["name"]=json!("查询期间编辑");
                store::save(&fixture.paths,&config).unwrap();
            }
            release.notify_one();
            let result=fetch.await.unwrap();
            let shared = concurrent.await.unwrap();
            assert_eq!(
                result.as_ref().map_err(ToString::to_string),
                shared.as_ref().map_err(ToString::to_string),
            );
            assert!(received.try_recv().is_err(), "并发目录刷新只发送一次请求");
            let c=database::open(&fixture.paths).unwrap();
            let model=store::query(&c,"SELECT enabled,input_price_micros,context_window FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id='real-model'",&[&provider]).unwrap().remove(0);
            assert_eq!(model["enabled"],0);
            assert_eq!(model["input_price_micros"],2000000);
            if stale {
                assert!(result.unwrap_err().to_string().contains("已发生变化"));
                assert!(model["context_window"].is_null());
            } else {
                result.unwrap();
                assert_eq!(model["context_window"],32000);
            }
            assert!(service::authorize(&c,&token).is_ok(),"刷新目录不能使客户端 Token 失效");
        }
        let catalog = directory.join(".ai_manager_service_models.json");
        let previous = std::fs::read(&catalog).unwrap();
        let c = database::open(&fixture.paths).unwrap();
        let snapshot = store::config(&c, &provider).unwrap();
        for malformed in [1, 2] {
            variant.store(malformed, Ordering::Relaxed);
            let paths = fixture.paths.clone();
            let provider_id = provider.clone();
            let fetch = tokio::spawn(async move {
                service::fetch_models(&paths, &json!({"providerId":provider_id})).await
            });
            received.recv().await.unwrap();
            release.notify_one();
            assert!(fetch.await.unwrap().is_err());
            assert_eq!(store::config(&c, &provider).unwrap(), snapshot);
            assert_eq!(std::fs::read(&catalog).unwrap(), previous);
        }
        server.abort();
    });
}

#[test]
fn service_start_refreshes_the_selected_key_and_keeps_catalogs_independent() {
    run(async {
        use crate::api::service_provider;
        use crate::core::{database, service_provider_store as store};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let fixture = Fixture::new();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let mode = Arc::new(AtomicUsize::new(0));
        let response_mode = mode.clone();
        let (sent, mut requests) = mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let response_mode = response_mode.clone();
                let sent = sent.clone();
                tokio::spawn(async move {
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(socket), service_fn(move |request: Request<Incoming>| {
                        let key = request.headers()["authorization"].to_str().unwrap().to_owned();
                        let mode = response_mode.load(Ordering::Relaxed);
                        let sent = sent.clone();
                        async move {
                            assert_eq!(request.uri().path(), "/v1/models");
                            sent.send(key.clone()).unwrap();
                            let (status, data) = match (key.as_str(), mode) {
                                ("Bearer key-a", _) => (StatusCode::OK, json!(["a", "common"])),
                                ("Bearer key-b", 1) => (StatusCode::UNAUTHORIZED, json!([])),
                                ("Bearer key-b", 2) => (StatusCode::OK, json!(["b-new"])),
                                ("Bearer key-b", 3) => (StatusCode::OK, json!([])),
                                ("Bearer key-b", _) => (StatusCode::OK, json!(["b", "common"])),
                                _ => panic!("请求使用了未选中的凭据"),
                            };
                            Ok::<_, Infallible>(json_response(status, json!({"data":data})))
                        }
                    })).await;
                });
            }
        });
        let saved = store::save(&fixture.paths, &json!({
            "name":"服务商","baseUrl":url,"apiKey":"key-a","models":["old"],
            "bindings":[{"agentType":"codex","enabled":true,"defaultModelName":"old"}]
        })).unwrap();
        let provider = store::text(&saved, "id");
        let c = database::open(&fixture.paths).unwrap();
        store::save_key_version(&c, provider, "second", "第二个 Key", "key-b", false).unwrap();
        service_provider::switch_backend(&fixture.paths, "service").unwrap();
        let targets = json!([{"id":"codex","installed":true,"configPath":fixture.root.join("model-key-client")}]);
        let config_path = fixture.root.join("model-key-client/config.toml");
        let start = json!({"agentType":"codex","providerId":provider});
        let models = || store::query(&c, "SELECT upstream_model_id AS name FROM service_provider_model_catalog
            WHERE provider_id=?1 AND enabled=1 ORDER BY name", &[&provider]).unwrap()
            .into_iter().map(|model| model["name"].clone()).collect::<Vec<_>>();

        let first = service_provider::start_provider(&fixture.paths, &targets, "quick-switch", start.clone()).await.unwrap();
        assert_eq!(requests.try_recv().unwrap(), "Bearer key-a");
        assert_eq!(models(), vec![json!("a"), json!("common")]);
        let before = std::fs::read(&config_path).unwrap();
        let key_b = store::query(&c, "SELECT id FROM service_provider_keys WHERE logical_id='second'", &[]).unwrap().remove(0);
        let select = json!({"providerId":provider,"id":key_b["id"]});

        mode.store(1, Ordering::Relaxed);
        assert!(service_provider::dispatch(&fixture.paths, &targets, "key-select", select.clone()).await.is_err());
        assert_eq!(requests.try_recv().unwrap(), "Bearer key-b");
        assert_eq!(models(), vec![json!("a"), json!("common")]);
        assert_eq!(std::fs::read(&config_path).unwrap(), before);

        mode.store(0, Ordering::Relaxed);
        service_provider::dispatch(&fixture.paths, &targets, "key-select", select).await.unwrap();
        assert_eq!(requests.try_recv().unwrap(), "Bearer key-b");
        assert_eq!(models(), vec![json!("b"), json!("common")]);
        assert!(service::resolve(&fixture.paths, &json!({"providerId":provider,"agentType":"codex"}), "a").is_err());
        let before = std::fs::read(&config_path).unwrap();
        mode.store(1, Ordering::Relaxed);
        assert!(service_provider::start_provider(&fixture.paths, &targets, "quick-switch", start.clone()).await.is_err());
        assert_eq!(requests.try_recv().unwrap(), "Bearer key-b");
        assert_eq!(std::fs::read(&config_path).unwrap(), before);
        assert_eq!(models(), vec![json!("b"), json!("common")]);

        mode.store(2, Ordering::Relaxed);
        let selected = json!({"agentType":"codex","providerId":provider,
            "revision":store::config(&c, provider).unwrap()["revision"],"defaultModelName":"b","options":{}});
        assert!(service_provider::dispatch(&fixture.paths, &targets, "configure-agent", selected).await.is_err());
        assert_eq!(requests.try_recv().unwrap(), "Bearer key-b");
        assert_eq!(models(), vec![json!("b-new")]);
        assert_eq!(std::fs::read(&config_path).unwrap(), before);

        let next = service_provider::start_provider(&fixture.paths, &targets, "quick-switch", start.clone()).await.unwrap();
        assert_eq!(requests.try_recv().unwrap(), "Bearer key-b");
        assert_eq!(next["instanceId"], first["instanceId"]);
        assert_eq!(models(), vec![json!("b-new")]);
        assert!(std::fs::read_to_string(&config_path).unwrap().contains("model = \"b-new\""));
        mode.store(3, Ordering::Relaxed);
        assert!(service_provider::start_provider(&fixture.paths, &targets, "quick-switch", start).await.is_err());
        assert_eq!(requests.try_recv().unwrap(), "Bearer key-b");
        assert!(models().is_empty());
        let snapshots = store::query(&c, "SELECT models_json AS modelsJson FROM service_provider_keys WHERE provider_id=?1 ORDER BY logical_id", &[&provider]).unwrap();
        assert_eq!(snapshots[0]["models"], json!({"a":1,"common":1}));
        assert_eq!(snapshots[1]["models"], json!({}));
        server.abort();
    });
}

#[test]
fn service_gateway_matrix_routes_only_new_sources_and_records_usage(){
    run(async{
        use crate::core::{database,service_provider_store as store};
        for format in [UpstreamFormat::OpenAiChat,UpstreamFormat::OpenAiResponses]{
            let fixture=Fixture::new();
            let (url,mut captures,upstream)=mock(format,StatusCode::OK,false,false,None).await;
            let (base,token,provider,server)=service_setup(&fixture,&url,format).await;
            let c=database::open(&fixture.paths).unwrap();
            // 用旧表写保护证明转发、结算和 Token 撤销都不会写回旧来源。
            for table in ["providers","provider_keys"]{
                for event in ["INSERT","UPDATE","DELETE"]{
                    c.execute_batch(&format!("CREATE TRIGGER readonly_{table}_{event} BEFORE {event} ON {table} BEGIN SELECT RAISE(ABORT,'禁止写旧表'); END;")).unwrap();
                }
            }
            let client=Client::builder().no_proxy().build().unwrap();
            for protocol in [Protocol::Chat,Protocol::Responses,Protocol::Claude,Protocol::Gemini]{
                for streaming in [false,true]{
                    let (path,body)=input(protocol,streaming);
                    let request=client.post(format!("{base}{path}"));
                    let request=match protocol {
                        Protocol::Claude=>request.header("x-api-key",&token),
                        Protocol::Gemini=>request.header("x-goog-api-key",&token),
                        _=>request.bearer_auth(&token),
                    };
                    let response=request.json(&body).send().await.unwrap();
                    let status=response.status();
                    let text=response.text().await.unwrap();
                    assert_eq!(status,200,"{text}");
                    assert!(text.contains("中文🙂"),"{text}");
                    assert!(text.contains("lookup"),"{text}");
                    assert!(!text.contains("gateway_error"),"{text}");
                    let (path,headers,body)=captures.recv().await.unwrap();
                    assert!(path.ends_with(if format==UpstreamFormat::OpenAiChat{"/chat/completions"}else{"/responses"}));
                    assert_eq!(headers["authorization"],"Bearer private-upstream-key");
                    assert_eq!(body["model"],"real-model");
                }
            }
            let blocked=client.post(format!("{base}/v1/responses")).bearer_auth(&token)
                .json(&json!({"model":"other-provider-model","input":"test"})).send().await.unwrap();
            assert_eq!(blocked.status(),400);
            let rejected=client.post(format!("{base}/v1/responses")).bearer_auth("wrong-token")
                .json(&json!({"model":"alias","input":"test"})).send().await.unwrap();
            assert_eq!(rejected.status(),401);
            let usage=store::usage(&c).unwrap();
            assert_eq!(usage["requests"]["requestCount"],10);
            assert_eq!(usage["requests"]["successCount"],8);
            assert_eq!(usage["attempts"]["attemptCount"],8);
            assert_eq!(usage["attempts"]["inputTokens"],96);
            assert_eq!(usage["attempts"]["outputTokens"],48);
            assert_eq!(usage["byProvider"][0]["groupKey"],provider);
            assert!(usage["logs"].as_array().unwrap().iter().all(|r|!r.to_string().contains("private-upstream-key")));
            // 请求结束即可从原 Usage 接口读取；无需任何会话日志或同步导入。
            let stats = crate::api::usage::get_stats(&fixture.paths,json!({"includeAllLogs":true})).await.unwrap();
            assert_eq!(stats["data"]["summary"]["requestCount"],10);
            assert_eq!(stats["data"]["logTotalCount"],10);
            assert_eq!(stats["data"]["logs"].as_array().unwrap().iter()
                .filter(|log|log["usageKnown"]==true).count(),8);
            // 显式别名优先于同名物理模型；目标禁用后不能绕过别名转发。
            c.execute("UPDATE service_provider_models SET enabled=0 WHERE provider_id=?1 AND upstream_model_id='real-model'",[&provider]).unwrap();
            assert_eq!(client.post(format!("{base}/v1/responses")).bearer_auth(&token)
                .json(&json!({"model":"alias","input":"test"})).send().await.unwrap().status(),400);
            c.execute("UPDATE service_provider_agent_bindings SET enabled=0 WHERE provider_id=?1",[provider]).unwrap();
            assert_eq!(client.get(format!("{base}/v1/models")).bearer_auth(token).send().await.unwrap().status(),401);
            server.abort();upstream.abort();
        }
    });
}

#[test]
fn tool_gateway_keeps_pool_tokens_protocols_and_history_with_service_sources() {
    run(async {
        use crate::api::service_provider;
        use crate::core::{database, service_provider_store as store};

        for format in [UpstreamFormat::OpenAiChat, UpstreamFormat::OpenAiResponses] {
            let fixture = Fixture::new();
            let (url, mut captures, upstream) = mock(format, StatusCode::OK, false, false, None).await;
            let (legacy_url, mut legacy_captures, legacy_upstream) =
                mock(UpstreamFormat::OpenAiResponses, StatusCode::OK, false, false, None).await;
            let (base, token) = setup(&fixture, &legacy_url, UpstreamFormat::OpenAiResponses).await;
            let client = Client::builder().no_proxy().build().unwrap();
            let (path, body) = input(Protocol::Chat, false);
            let previous = client.post(format!("{base}{path}")).bearer_auth(&token)
                .json(&body).send().await.unwrap();
            assert_eq!(previous.status(), 200);
            previous.text().await.unwrap();
            legacy_captures.recv().await.unwrap();
            let before = read_state(&fixture.paths).unwrap();
            let previous_log = before["logs"][0]["id"].clone();

            service_provider::switch_backend(&fixture.paths, "service").unwrap();
            let c = database::open(&fixture.paths).unwrap();
            let revision = store::config(&c, "provider-test").unwrap()["revision"].clone();
            store::save(&fixture.paths, &json!({
                "id":"provider-test","revision":revision,"name":"新表服务商","baseUrl":url,
                "upstreamProtocol":if format == UpstreamFormat::OpenAiChat {"chat"} else {"responses"},
                "apiKey":"service-only-key","models":["real-model","service-only-model"]
            })).unwrap();
            let second = store::save(&fixture.paths, &json!({
                "name":"第二来源","baseUrl":url,"apiKey":"second-only-key","models":["second-model"],
                "upstreamProtocol":if format == UpstreamFormat::OpenAiChat {"chat"} else {"responses"}
            })).unwrap()["id"].as_str().unwrap().to_owned();
            // 工具池无需启动任何 Agent，也不能写回旧 Provider 和 Key。
            c.execute("UPDATE service_provider_agent_bindings SET enabled=0", []).unwrap();
            for table in ["providers", "provider_keys"] {
                for event in ["INSERT", "UPDATE", "DELETE"] {
                    c.execute_batch(&format!("CREATE TRIGGER tool_readonly_{table}_{event} BEFORE {event} ON {table} BEGIN SELECT RAISE(ABORT,'禁止写旧表'); END;")).unwrap();
                }
            }
            let state = read_state(&fixture.paths).unwrap();
            for field in ["host", "port", "providerIds", "modelMappings", "tokens", "usage", "logs"] {
                assert_eq!(state[field], before[field], "{field}");
            }
            assert!(state["models"].as_array().unwrap().iter().any(|m| m["id"] == "service-only-model"));
            save_config(&fixture.registry, &fixture.paths, &json!({}),
                json!({"providerIds":["provider-test",second]})).await.unwrap();
            let models = client.get(format!("{base}/v1/models")).bearer_auth(&token)
                .send().await.unwrap().json::<Value>().await.unwrap();
            assert!(models["data"].as_array().unwrap().iter().any(|m| m["id"] == "second-model"));
            for protocol in [Protocol::Chat, Protocol::Responses, Protocol::Claude, Protocol::Gemini] {
                for streaming in [false, true] {
                    let (path, body) = input(protocol, streaming);
                    let request = client.post(format!("{base}{path}"));
                    let request = match protocol {
                        Protocol::Claude => request.header("x-api-key", &token),
                        Protocol::Gemini => request.header("x-goog-api-key", &token),
                        _ => request.bearer_auth(&token),
                    };
                    let response = request.json(&body).send().await.unwrap();
                    let status = response.status();
                    let text = response.text().await.unwrap();
                    assert_eq!(status, 200, "{text}");
                    assert!(text.contains("中文🙂") && text.contains("lookup"), "{text}");
                    assert!(!text.contains("gateway_error"), "{text}");
                    let (path, headers, body) = captures.recv().await.unwrap();
                    assert!(path.ends_with(if format == UpstreamFormat::OpenAiChat {"/chat/completions"} else {"/responses"}));
                    assert_eq!(headers["authorization"], "Bearer service-only-key");
                    assert_eq!(body["model"], "real-model");
                }
            }
            let response = client.post(format!("{base}/v1/responses")).bearer_auth(&token)
                .json(&json!({"model":"second-model","input":"test"})).send().await.unwrap();
            assert_eq!(response.status(), 200);
            response.text().await.unwrap();
            let (_, headers, body) = captures.recv().await.unwrap();
            assert_eq!(headers["authorization"], "Bearer second-only-key");
            assert_eq!(body["model"], "second-model");
            let state = read_state(&fixture.paths).unwrap();
            assert_eq!(state["usage"]["requestCount"], 10);
            assert_eq!(state["usage"]["successCount"], 10);
            assert_eq!(state["tokens"][0]["requestCount"], 10);
            assert!(state["logs"].as_array().unwrap().iter().any(|log| log["id"] == previous_log));
            let detail = read_log_detail(&fixture.paths, &json!({"logId":state["logs"][0]["id"]})).unwrap();
            for field in ["requestBody", "upstreamRequestBody", "upstreamResponseBody", "responseBody"] {
                assert!(detail[field].as_str().is_some_and(|s| !s.is_empty()), "{field}");
            }
            assert!(!detail.to_string().contains("second-only-key"));
            let token_id = state["tokens"][0]["id"].clone();
            set_token_enabled(&fixture.registry, &fixture.paths, json!({"tokenId":token_id,"enabled":false})).await.unwrap();
            assert_eq!(client.get(format!("{base}/v1/models")).bearer_auth(&token).send().await.unwrap().status(), 401);
            set_token_enabled(&fixture.registry, &fixture.paths, json!({"tokenId":token_id,"enabled":true})).await.unwrap();
            c.execute("UPDATE service_provider_configs SET enabled=0 WHERE id='provider-test'", []).unwrap();
            assert!(build_target(&fixture.paths, &json!({}), "provider-test").is_err());
            assert_eq!(client.post(format!("{base}/v1/responses")).bearer_auth(&token)
                .json(&json!({"model":"alias","input":"test"})).send().await.unwrap().status(), 503);

            service_provider::switch_backend(&fixture.paths, "legacy").unwrap();
            let response = client.post(format!("{base}/v1/responses")).bearer_auth(&token)
                .json(&json!({"model":"alias","input":"test"})).send().await.unwrap();
            assert_eq!(response.status(), 200);
            response.text().await.unwrap();
            assert_eq!(legacy_captures.recv().await.unwrap().1["authorization"], "Bearer private-upstream-key");
            fixture.registry.stop().await;
            upstream.abort();
            legacy_upstream.abort();
        }
    });
}

#[test]
fn tool_gateway_reads_official_accounts_and_model_catalogs_from_service_tables() {
    run(async {
        use crate::core::{database, service_provider_store as store};
        let fixture = Fixture::new();
        store::initialize(&fixture.paths).unwrap();
        let codex = store::save(&fixture.paths, &json!({
            "name":"GPT 官方","kind":"codex_account","models":["official-codex-model"]
        })).unwrap()["id"].as_str().unwrap().to_owned();
        let google = store::save(&fixture.paths, &json!({
            "name":"Google 官方","kind":"google_account","models":["official-google-model"]
        })).unwrap()["id"].as_str().unwrap().to_owned();
        let c = database::open(&fixture.paths).unwrap();
        store::save_account(&c, &codex, &json!({
            "account_id":"new-official-id","access_token":"new-official-access"
        })).unwrap();
        crate::api::service_provider::switch_backend(&fixture.paths, "service").unwrap();
        let target = build_target(&fixture.paths, &json!({}), &codex).unwrap();
        assert!(target.service_source && target.kind == TargetKind::CodexAccount);
        assert_eq!(list_target_models(&fixture.paths, &target), ["official-codex-model"]);
        let auth = service::target_auth(&fixture.paths, &target).await.unwrap();
        assert_eq!(auth.token, "new-official-access");
        assert_eq!(auth.account_id, "new-official-id");
        let target = build_target(&fixture.paths, &json!({}), &google).unwrap();
        assert!(target.service_source && target.kind == TargetKind::GoogleAccount);
        assert_eq!(list_target_models(&fixture.paths, &target), ["official-google-model"]);
        c.execute("UPDATE service_provider_configs SET enabled=0 WHERE id=?1", [&google]).unwrap();
        assert!(build_target(&fixture.paths, &json!({}), &google).is_err());
    });
}

#[test]
fn service_gateway_skill_usage_is_available_without_session_logs() {
    run(async {
        use crate::api::usage;
        use crate::core::{database, service_provider_store as store};
        let fixture=Fixture::new();
        let (url,mut captures,upstream)=mock(UpstreamFormat::OpenAiResponses,StatusCode::OK,false,false,None).await;
        let (base,token,_,server)=service_setup(&fixture,&url,UpstreamFormat::OpenAiResponses).await;
        let client=Client::builder().no_proxy().build().unwrap();
        let state=json!({"skills":[],"cliTargets":[],"sessions":[]});
        for (session,streaming) in [("session-one",false),("session-one",true),("session-two",false)] {
            let response=client.post(format!("{base}/v1/responses")).bearer_auth(&token)
                .header("session_id",session)
                .json(&json!({"model":"alias","input":[{"role":"user","content":"请使用 $imagegen"}],"stream":streaming}))
                .send().await.unwrap();
            assert_eq!(response.status(),StatusCode::OK);
            response.text().await.unwrap();
            captures.recv().await.unwrap();
        }
        let c=database::open(&fixture.paths).unwrap();
        assert_eq!(store::query(&c,"SELECT * FROM service_gateway_skill_events",&[]).unwrap().len(),2);
        for _ in 0..2 {
            let stats=usage::get_skill_usage_stats(&fixture.paths,json!({}),&state).await.unwrap();
            let row=stats["data"]["skills"].as_array().unwrap().iter().find(|row|row["name"]=="imagegen").unwrap();
            assert_eq!(row["usageCount"],2);
            assert_eq!(row["requestCount"],3);
            assert_eq!(row["actualTokens"],54);
            assert!(stats["data"]["diagnostics"].as_array().unwrap().is_empty());
            let synced=usage::sync_usage(&fixture.paths,json!({}),&state).await.unwrap();
            assert_eq!(synced["data"]["summary"]["requestCount"],3);
        }
        assert!(crate::core::usage_store::read_all_logs(&fixture.paths).unwrap().is_empty());
        // 增量 Responses 只提交新内容时，沿明确的上游 response ID 关联已观测技能。
        let response=client.post(format!("{base}/v1/responses")).bearer_auth(&token)
            .json(&json!({"model":"alias","input":"继续","previous_response_id":"resp_original"}))
            .send().await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        response.text().await.unwrap();
        captures.recv().await.unwrap();
        let stats=usage::get_skill_usage_stats(&fixture.paths,json!({}),&state).await.unwrap();
        assert_eq!(stats["data"]["summary"]["usageCount"],2);
        assert_eq!(stats["data"]["summary"]["requestCount"],4);
        assert_eq!(stats["data"]["summary"]["actualTokens"],72);
        server.abort();upstream.abort();
    });
}

#[test]
fn service_gateway_cancelled_wait_does_not_block_backend_switch() {
    run(async {
        use crate::core::{database, service_provider_store as store};
        let fixture = Fixture::new();
        let gate = Arc::new(tokio::sync::Notify::new());
        let (url, mut captures, upstream) = mock(
            UpstreamFormat::OpenAiResponses,
            StatusCode::OK,
            false,
            false,
            Some(gate.clone()),
        ).await;
        let (base, token, _, server) =
            service_setup(&fixture, &url, UpstreamFormat::OpenAiResponses).await;
        let request = tokio::spawn(async move {
            Client::new().post(format!("{base}/v1/responses")).bearer_auth(token)
                .json(&json!({"model":"alias","input":"test"})).send().await
        });
        captures.recv().await.unwrap();
        request.abort();
        let _ = request.await;
        // 客户端在上游响应头到达前取消，账本仍应关闭在途记录并保留未知用量。
        let c = database::open(&fixture.paths).unwrap();
        let mut finished = false;
        for _ in 0..100 {
            finished = c.query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM service_gateway_requests WHERE finished_at IS NULL)",
                [], |row| row.get(0),
            ).unwrap();
            if finished { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        gate.notify_waiters();
        server.abort();
        upstream.abort();
        assert!(finished, "客户端取消后遗留在途账本，会阻止模式切换");
        let attempt = store::query(
            &c, "SELECT state,input_tokens,output_tokens FROM service_gateway_attempts", &[],
        ).unwrap().remove(0);
        assert_eq!(attempt["state"], "unknown");
        assert!(attempt["input_tokens"].is_null());
        assert!(attempt["output_tokens"].is_null());
        crate::api::service_provider::switch_backend(&fixture.paths, "legacy").unwrap();
    });
}

#[test]
fn service_gateway_compact_is_native_only_and_ledger_survives_http_failure(){
    run(async{
        use crate::core::{database,service_provider_store as store};
        for format in [UpstreamFormat::OpenAiChat,UpstreamFormat::OpenAiResponses]{
            let fixture=Fixture::new();
            let (url,mut captures,upstream)=mock(format,StatusCode::OK,false,false,None).await;
            let (base,token,_,server)=service_setup(&fixture,&url,format).await;
            let streaming=Client::new().post(format!("{base}/v1/responses/compact")).bearer_auth(&token)
                .json(&json!({"model":"alias","input":"test","stream":true})).send().await.unwrap();
            assert_eq!(streaming.status(),400);
            let response=Client::new().post(format!("{base}/v1/responses/compact")).bearer_auth(token)
                .json(&json!({"model":"alias","input":[{"type":"compaction","encrypted_content":"opaque"}]})).send().await.unwrap();
            if format==UpstreamFormat::OpenAiChat{
                assert_eq!(response.status(),422);
            }else{
                assert_eq!(response.status(),200);
                let _=response.bytes().await.unwrap();
                let (path,_,body)=captures.recv().await.unwrap();
                assert!(path.ends_with("/responses/compact"));
                assert_eq!(body["input"][0]["encrypted_content"],"opaque");
                assert!(body.get("stream").is_none());
            }
            server.abort();upstream.abort();
        }
        let fixture=Fixture::new();
        let (url,_captures,upstream)=mock(UpstreamFormat::OpenAiResponses,StatusCode::TOO_MANY_REQUESTS,false,false,None).await;
        let (base,token,_,server)=service_setup(&fixture,&url,UpstreamFormat::OpenAiResponses).await;
        let response=Client::new().post(format!("{base}/v1/responses")).bearer_auth(token)
            .json(&json!({"model":"alias","input":"test"})).send().await.unwrap();
        assert_eq!(response.status(),429);
        let _=response.bytes().await.unwrap();
        let c=database::open(&fixture.paths).unwrap();
        let attempt=store::query(&c,"SELECT state,input_tokens,status_code FROM service_gateway_attempts",&[]).unwrap().remove(0);
        assert_eq!(attempt["state"],"failed");
        assert_eq!(attempt["status_code"],429);
        assert!(attempt["input_tokens"].is_null());
        server.abort();upstream.abort();
    });
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
            service_mode: false,
            legacy_request: None,
            _service_request: None,
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

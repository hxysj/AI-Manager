use super::*;
use crate::api::runtime_provider;
use crate::core::{provider_store,settings::AssistantSettings};
use std::convert::Infallible;
use bytes::Bytes;
use http_body_util::{BodyExt,Full};
use hyper::{Response,server::conn::http1,service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

#[test]
fn capture_uses_real_model_proxy_for_chat_responses_and_claude() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        for (cli,api,endpoint) in [("codex","chat","/v1/chat/completions"),("codex","responses","/v1/responses"),("claude","messages","/v1/messages")] {
            let root=std::env::temp_dir().join(format!("quick-note-ai-{}",uuid::Uuid::new_v4()));
            let paths=crate::core::paths::resolve_app_paths(&root);
            store::initialize(&paths).unwrap();
            let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut keys=serde_json::Map::new();runtime_provider::set_provider_key(&mut keys,"test","upstream-secret".into()).unwrap();
            provider_store::write_provider_bundle(&paths,&[json!({"id":"test","name":"测试","cli":cli,"baseUrl":format!("http://{}/v1",listener.local_addr().unwrap()),"enabled":true})],&[],&[],&keys).unwrap();
            let future=store::now()+3_600_000;
            let generated=json!({"title":"回归测试","content":"验证速记","type":"worklog","groupId":"unknown","categoryId":"debugging","tags":["回归"],"tasks":[{"title":"验证模型","status":"pending"}],"reminders":[{"at":chrono::DateTime::<chrono::Utc>::from_timestamp_millis(future).unwrap().to_rfc3339(),"repeat":"once","taskIndex":0}]}).to_string();
            let reply=match api {"chat"=>json!({"choices":[{"message":{"content":generated}}]}),"messages"=>json!({"content":[{"type":"text","text":generated}]}),_=>json!({"output":[{"type":"message","content":[{"type":"output_text","text":generated}]}]})};
            let server=tokio::spawn(async move {
                let (stream,_)=listener.accept().await.unwrap();
                http1::Builder::new().serve_connection(TokioIo::new(stream),service_fn(move |request:hyper::Request<hyper::body::Incoming>|{
                    let reply=reply.clone();async move {
                        assert_eq!(request.uri().path(),endpoint);
                        assert_eq!(request.headers().get(if cli=="claude"{"x-api-key"}else{"authorization"}).unwrap(),if cli=="claude"{"upstream-secret"}else{"Bearer upstream-secret"});
                        let bytes=request.into_body().collect().await.unwrap().to_bytes();let body:Value=serde_json::from_slice(&bytes).unwrap();
                        assert_eq!(body["model"],"notes-model");assert!(body.get("tools").is_none());
                        Ok::<_,Infallible>(Response::builder().header("content-type","application/json").body(Full::new(Bytes::from(reply.to_string()))).unwrap())
                    }
                })).await.unwrap();
            });
            let context=AssistantContext{paths:paths.clone(),workspace_root:root.clone(),resource_dir:root.clone(),cli_targets:json!([]),settings:AssistantSettings{target_id:"test".into(),model:"notes-model".into(),protocol:if api=="messages"{"auto"}else{api}.into(),..Default::default()},local_models:Default::default()};
            let input=json!({"requestId":"capture","text":"明天提醒我回归测试","source":{"timeZone":"Asia/Shanghai"}});
            let capture=store::capture(&paths,&input,&json!({"model":"notes-model"})).unwrap();
            let output=generate(&context,&capture,&input).await.unwrap();
            assert_eq!(output["groupId"],"inbox");assert_eq!(output["categoryId"],"debugging");
            let note=store::complete_capture(&paths,store::text(&capture,"id"),&output).unwrap();
            assert_eq!(note["createdAt"],capture["capturedAt"]);
            assert_eq!(store::detail(&paths,store::text(&note,"id")).unwrap()["reminders"][0]["nextFireAt"],future);
            server.abort();
            assert_eq!(root.parent(),Some(std::env::temp_dir().as_path()));std::fs::remove_dir_all(root).unwrap();
        }
    });
}

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
fn model_folder_ids_require_matching_kind_and_names_can_reuse_aliases() {
    let folders=vec![json!({"id":"project","kind":"group","name":"AI Manager","aliases":["桌宠"]}),json!({"id":"debugging","kind":"category","name":"问题排查"})];
    let capture=json!({"source":{}});
    let normalized=normalize_item(json!({"content":"验证","groupId":"debugging","categoryId":"project"}),&folders,&capture,&json!({})).unwrap();
    assert_eq!(normalized["groupId"],"inbox");assert_eq!(normalized["categoryId"],"uncategorized");
    let normalized=normalize_item(json!({"content":"验证","groupId":"桌宠","categoryId":"问题排查","newCategory":{"name":"问题排查"}}),&folders,&capture,&json!({})).unwrap();
    assert_eq!(normalized["groupId"],"project");assert_eq!(normalized["categoryId"],"debugging");
}

#[test]
fn reminder_offsets_use_capture_time_and_invalid_formats_request_model_correction() {
    let captured=store::now()-5_000;
    let capture=json!({"capturedAt":captured,"source":{"timeZone":"Asia/Shanghai"}});
    for seconds in [30,120,1800,3600] {
        let normalized=normalize_item(json!({"content":"提醒吃饭","pendingTime":"旧的待确认提示","tasks":[{"title":"吃饭"}],
            "reminders":[{"afterSeconds":seconds,"at":null,"taskIndex":0,"uncertain":false}]}),&[],&capture,&json!({})).unwrap();
        assert_eq!(normalized["reminders"][0]["nextFireAt"],captured+seconds*1000);
        assert_eq!(normalized["reminders"][0]["schedule"]["at"],captured+seconds*1000);
        assert_eq!(normalized["reminders"][0]["taskId"],normalized["tasks"][0]["id"]);
        assert_eq!(normalized["reminders"][0]["schedule"]["timeZone"],"Asia/Shanghai");
        assert_eq!(normalized["pendingTime"],"");
    }
    for plan in [
        json!({"at":"2 分钟后"}),json!({"at":"2026-11-18 15:00"}),json!({"at":null}),
        json!({"afterSeconds":0}),json!({"afterSeconds":-1}),json!({"afterSeconds":1.5}),
        json!({"afterSeconds":"120"}),json!({"afterSeconds":i64::MAX}),
        json!({"afterSeconds":120,"at":"2099-01-01T00:00:00+08:00"})
    ] {
        assert!(normalize_item(json!({"content":"验证","reminders":[plan]}),&[],&capture,&json!({})).is_err());
    }
    let uncertain=normalize_item(json!({"content":"三点提醒我","reminders":[{"at":null,"uncertain":true}]}),&[],&capture,&json!({})).unwrap();
    assert!(uncertain["reminders"].as_array().unwrap().is_empty());
    assert!(!store::text(&uncertain,"pendingTime").is_empty());
    let expired=normalize_item(json!({"content":"两分钟前的请求","reminders":[{"afterSeconds":120}]}),&[],&json!({"capturedAt":store::now()-180_000}),&json!({})).unwrap();
    assert!(expired["reminders"].as_array().unwrap().is_empty());
    assert!(store::text(&expired,"pendingTime").contains("已经过去"));
    let date_only=normalize_item(json!({"content":"明天开会","occurredOn":"2026-11-19"}),&[],&capture,&json!({})).unwrap();
    assert!(date_only["reminders"].as_array().unwrap().is_empty());
}

#[test]
fn explicit_relative_reminder_survives_missing_or_uncertain_model_output() {
    let captured=store::now()-5_000;
    let capture=json!({"capturedAt":captured,"sourceText":"2分钟后提醒我吃饭","source":{"timeZone":"Asia/Shanghai"}});
    for plans in [Value::Null,json!([]),json!([{"uncertain":true,"at":null}])] {
        let normalized=normalize_item(json!({"content":"吃饭","pendingTime":"提醒时间待确认","reminders":plans}),&[],&capture,&json!({})).unwrap();
        assert_eq!(normalized["reminders"][0]["nextFireAt"],captured+120_000);
        assert_eq!(normalized["pendingTime"],"");
    }
    let root=std::env::temp_dir().join(format!("quick-note-relative-{}",uuid::Uuid::new_v4()));
    let paths=crate::core::paths::resolve_app_paths(&root);
    store::initialize(&paths).unwrap();
    let capture=store::capture(&paths,&json!({"requestId":"relative","text":"两分钟后提醒我吃饭","source":{"timeZone":"Asia/Shanghai"}}),&json!({})).unwrap();
    let normalized=normalize_item(json!({"content":"吃饭","reminders":[]}),&[],&capture,&json!({})).unwrap();
    let note=store::complete_capture(&paths,store::text(&capture,"id"),&normalized).unwrap();
    let at=capture["capturedAt"].as_i64().unwrap()+120_000;
    assert_eq!(store::detail(&paths,store::text(&note,"id")).unwrap()["reminders"][0]["nextFireAt"],at);
    let events=store::due_events(&paths,at).unwrap();
    assert_eq!(events.len(),1);
    assert_eq!(events[0]["scheduledAt"],at);
    assert!(store::due_events(&paths,at+1).unwrap().is_empty());
    assert_eq!(root.parent(),Some(std::env::temp_dir().as_path()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn capture_uses_real_model_proxy_for_chat_responses_and_claude() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        for (cli,api,endpoint) in [("codex","chat","/v1/chat/completions"),("codex","responses","/v1/responses"),("claude","messages","/v1/messages")] {
          for preview in [false,true] {
            let root=std::env::temp_dir().join(format!("quick-note-ai-{}",uuid::Uuid::new_v4()));
            let paths=crate::core::paths::resolve_app_paths(&root);
            store::initialize(&paths).unwrap();
            let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut keys=serde_json::Map::new();runtime_provider::set_provider_key(&mut keys,"test","upstream-secret".into()).unwrap();
            provider_store::write_provider_bundle(&paths,&[json!({"id":"test","name":"测试","cli":cli,"baseUrl":format!("http://{}/v1",listener.local_addr().unwrap()),"enabled":true})],&[],&[],&keys).unwrap();
            let future=store::now()+3_600_000;
            let reminder=if preview {json!({"afterSeconds":120,"at":null,"repeat":"once","taskIndex":0})}
                else {json!({"at":chrono::DateTime::<chrono::Utc>::from_timestamp_millis(future).unwrap().to_rfc3339(),"repeat":"once","taskIndex":0})};
            let generated=json!({"title":"回归测试","content":"验证速记","type":"worklog","groupId":"unknown","categoryId":"debugging","tags":["回归"],"tasks":[{"title":"验证模型","status":"pending"}],"reminders":[reminder]}).to_string();
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
            let input=json!({"requestId":"capture","text":if preview{"2 分钟后提醒我回归测试"}else{"明天提醒我回归测试"},"source":{"timeZone":"Asia/Shanghai"},"preview":preview});
            let capture=store::capture(&paths,&input,&json!({"model":"notes-model"})).unwrap();
            let generated=generate_and_save(&context,&capture,&input).await.unwrap();
            let note=if preview {
                assert_eq!(store::capture_state(&paths,"capture").unwrap()["status"],"preview");
                assert!(store::list(&paths,&json!({})).unwrap()["notes"].as_array().unwrap().is_empty());
                store::dispatch(&paths,"capture-save",&json!({"id":capture["id"],"note":generated})).unwrap()
            }else{generated};
            assert_eq!(note["groupId"],"inbox");assert_eq!(note["categoryId"],"debugging");
            assert_eq!(note["createdAt"],capture["capturedAt"]);
            let expected=if preview{capture["capturedAt"].as_i64().unwrap()+120_000}else{future};
            assert_eq!(store::detail(&paths,store::text(&note,"id")).unwrap()["reminders"][0]["nextFireAt"],expected);
            server.abort();
            assert_eq!(root.parent(),Some(std::env::temp_dir().as_path()));std::fs::remove_dir_all(root).unwrap();
          }
        }
    });
}

#[test]
fn capture_repairs_model_output_atomically_and_stops_on_limits_cancellation_and_conflicts() {
    use std::sync::atomic::{AtomicUsize,Ordering};
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        for (cli,api) in [("codex","chat"),("codex","responses"),("claude","messages")] {
            for case in ["single","split","json","reminder","limit","http","cancel","conflict"] {
                let root=std::env::temp_dir().join(format!("quick-note-repair-{}",uuid::Uuid::new_v4()));
                let paths=crate::core::paths::resolve_app_paths(&root);
                store::initialize(&paths).unwrap();
                let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
                let mut keys=serde_json::Map::new();runtime_provider::set_provider_key(&mut keys,"test","upstream-secret".into()).unwrap();
                provider_store::write_provider_bundle(&paths,&[json!({"id":"test","name":"测试","cli":cli,"baseUrl":format!("http://{}/v1",listener.local_addr().unwrap()),"enabled":true})],&[],&[],&keys).unwrap();
                let mut input=json!({"requestId":"repair","text":"修复登录页与预约页分值不同步的问题","source":{"timeZone":"Asia/Shanghai","scope":if case=="split"{"split"}else{"input"}}});
                if case=="conflict" {
                    let note=store::save(&paths,&json!({"content":"人工原文"}),"user").unwrap();
                    input["noteId"]=note["id"].clone();input["baseRevision"]=note["revision"].clone();
                }
                let capture=store::capture(&paths,&input,&json!({"model":"notes-model"})).unwrap();
                let calls=Arc::new(AtomicUsize::new(0));
                let server_calls=calls.clone();let server_paths=paths.clone();let server_capture=capture.clone();
                let server=tokio::spawn(async move {
                    loop {
                        let (stream,_)=listener.accept().await.unwrap();
                        let calls=server_calls.clone();let paths=server_paths.clone();let capture=server_capture.clone();
                        tokio::spawn(async move {
                            http1::Builder::new().serve_connection(TokioIo::new(stream),service_fn(move |request:hyper::Request<hyper::body::Incoming>| {
                                let calls=calls.clone();let paths=paths.clone();let capture=capture.clone();
                                async move {
                                    let attempt=calls.fetch_add(1,Ordering::SeqCst);
                                    let bytes=request.into_body().collect().await.unwrap().to_bytes();
                                    let body:Value=serde_json::from_slice(&bytes).unwrap();
                                    let instruction=if api=="chat"{&body["messages"][0]["content"]}else if api=="messages"{&body["system"]}else{&body["instructions"]};
                                    assert!(instruction.as_str().unwrap().contains("纠正原文中的错别字"));
                                    assert!(instruction.as_str().unwrap().contains("按因果和执行顺序重组"));
                                    assert!(instruction.as_str().unwrap().contains("没有完成就不写已完成"));
                                    assert!(body.get("tools").is_none());
                                    let user=if api=="responses"{&body["input"]}else{&body["messages"][if api=="chat"{1}else{0}]["content"]};
                                    if attempt>0 {
                                        let feedback:Value=serde_json::from_str(user.as_str().unwrap()).unwrap();
                                        assert_eq!(feedback["原文"],capture["sourceText"]);
                                        assert!(!store::text(&feedback,"上次输出").is_empty());
                                        let expected=if case=="json"{"JSON"}else if case=="reminder"{"reminders[0].at"}else if attempt==1 || case=="limit"{"occurredOn"}else{"tasks.dueDate"};
                                        assert!(store::text(&feedback,"校验错误").contains(expected));
                                        assert!(store::list(&paths,&json!({})).unwrap()["notes"].as_array().unwrap().is_empty());
                                        assert!(!store::dispatch(&paths,"folders",&json!({})).unwrap()["folders"].as_array().unwrap().iter().any(|f|f["name"]=="联调项目"));
                                    }else{assert_eq!(user,&capture["sourceText"]);}
                                    if case=="cancel" {store::capture_failure(&paths,store::text(&capture,"id"),"cancelled","已取消整理").unwrap();}
                                    if case=="conflict" {
                                        let mut note=store::detail(&paths,store::text(&capture,"noteId")).unwrap();
                                        note["content"]=json!("人工更新");store::save(&paths,&note,"user").unwrap();
                                    }
                                    let mut item=json!({"title":"修复分值同步问题","content":"修复登录页与预约页分值不同步的问题。","type":"todo","groupId":"debugging","categoryId":"inbox","newGroup":{"name":"联调项目"},"newCategory":{"name":"同步修复"},"occurredOn":null,"tasks":[{"title":"修复分值同步问题","status":"pending","dueDate":null}]});
                                    if ["single","split","limit"].contains(&case) {
                                        item["occurredOn"]=json!(if attempt==0 || case=="limit"{"今天"}else{"2026-11-18"});
                                        item["tasks"][0]["dueDate"]=json!(if attempt<2{"2026/11/19"}else{"2026-11-19"});
                                    }
                                    if case=="reminder" {
                                        item["reminders"]=if attempt==0{json!([{"at":"2 分钟后"}])}else{json!([{"afterSeconds":120,"at":null,"taskIndex":0}])};
                                    }
                                    let output=if case=="json" && attempt==0{"整理结果不是 JSON".to_string()}else if case=="split"{json!({"notes":[{"content":"独立事项","newGroup":{"name":"联调项目"}},item]}).to_string()}else{item.to_string()};
                                    let reply=match api {"chat"=>json!({"choices":[{"message":{"content":output}}]}),"messages"=>json!({"content":[{"type":"text","text":output}]}),_=>json!({"output_text":output})};
                                    Ok::<_,Infallible>(Response::builder().status(if case=="http"{500}else{200}).header("content-type","application/json").body(Full::new(Bytes::from(reply.to_string()))).unwrap())
                                }
                            })).await.unwrap();
                        });
                    }
                });
                let context=AssistantContext{paths:paths.clone(),workspace_root:root.clone(),resource_dir:root.clone(),cli_targets:json!([]),settings:AssistantSettings{target_id:"test".into(),model:"notes-model".into(),protocol:if api=="messages"{"auto"}else{api}.into(),..Default::default()},local_models:Default::default()};
                let result=generate_and_save(&context,&capture,&input).await;
                let expected_calls=if ["single","split","limit"].contains(&case){3}else if ["json","reminder"].contains(&case){2}else{1};
                assert_eq!(calls.load(Ordering::SeqCst),expected_calls,"{api}/{case}");
                let notes=store::list(&paths,&json!({})).unwrap()["notes"].as_array().unwrap().clone();
                let state=store::capture_state(&paths,"repair").unwrap();
                assert_eq!(state["sourceText"],input["text"]);
                if ["single","split","json","reminder"].contains(&case) {
                    let note=result.unwrap();
                    assert_eq!(state["status"],"completed");assert_eq!(note["createdAt"],capture["capturedAt"]);
                    assert_eq!(notes.len(),if case=="split"{2}else{1});
                    assert!(notes.iter().all(|n|n["revision"]==1));
                    let saved=if case=="split"{&note["notes"][1]}else{&note};
                    if ["single","split"].contains(&case) {assert_eq!(saved["occurredOn"],"2026-11-18");assert_eq!(saved["tasks"][0]["dueDate"],"2026-11-19");}
                    if case=="reminder" {assert_eq!(store::detail(&paths,store::text(saved,"id")).unwrap()["reminders"][0]["nextFireAt"],capture["capturedAt"].as_i64().unwrap()+120_000);}
                    let folders=store::dispatch(&paths,"folders",&json!({})).unwrap();
                    assert_eq!(folders["folders"].as_array().unwrap().iter().filter(|f|f["name"]=="联调项目").count(),1);
                    assert_eq!(folders["folders"].as_array().unwrap().iter().find(|f|f["id"]==saved["groupId"]).unwrap()["kind"],"group");
                    assert_eq!(folders["folders"].as_array().unwrap().iter().find(|f|f["id"]==saved["categoryId"]).unwrap()["kind"],"category");
                }else{
                    let error=result.unwrap_err().to_string();
                    if case=="cancel" {assert_eq!(state["status"],"cancelled");}
                    if case=="conflict" {assert!(error.contains("其他窗口更新"));assert_eq!(notes[0]["content"],"人工更新");}
                    else {assert!(notes.is_empty());}
                    if case=="limit" {assert!(error.contains("occurredOn"));store::capture_failure(&paths,store::text(&capture,"id"),"failed",&error).unwrap();assert_eq!(store::capture_state(&paths,"repair").unwrap()["status"],"failed");}
                    if case=="http" {assert!(error.contains("HTTP 500"));}
                }
                server.abort();
                assert_eq!(root.parent(),Some(std::env::temp_dir().as_path()));std::fs::remove_dir_all(root).unwrap();
            }
        }
    });
}

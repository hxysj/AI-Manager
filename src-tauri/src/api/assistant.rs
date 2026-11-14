use crate::api::{llm_proxy::LlmProxy, proxy};
use crate::core::{
    error::ManagerError, paths::AppPaths, settings::AssistantSettings,
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
    sync::{oneshot, Mutex},
};
use tokio_util::sync::CancellationToken;

mod tools;
mod images;
pub mod desktop;

type Reply = oneshot::Sender<Result<Value, String>>;
#[derive(Default)]
struct PendingReplies {
    replies: HashMap<String, Reply>,
    closed_error: Option<String>,
}
type Pending = Arc<Mutex<PendingReplies>>;

pub struct AssistantContext {
    pub paths: AppPaths,
    pub workspace_root: PathBuf,
    pub resource_dir: PathBuf,
    pub cli_targets: Value,
    pub settings: AssistantSettings,
}

struct AssistantRuntime {
    child: Child,
    input: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    storage_dir: String,
}

pub(super) struct EditorRequest {
    payload: Value,
    reply: Reply,
}

#[derive(Clone)]
pub struct AssistantService {
    runtime: Arc<Mutex<Option<AssistantRuntime>>>,
    state: Arc<Mutex<Value>>,
    active: Arc<Mutex<Option<(String, CancellationToken)>>>,
    editors: Arc<Mutex<HashMap<String, EditorRequest>>>,
    desktop_thread: Arc<Mutex<(String, String)>>,
}

impl Default for AssistantService {
    fn default() -> Self {
        Self {
            runtime: Default::default(),
            state: Arc::new(Mutex::new(json!({
                "status": "idle", "runId": "", "threadId": "", "message": "",
                "text": "", "error": "", "tools": [], "petAction": "", "petMessage": "",
                "enabled": true, "ready": false, "startupError": ""
            }))),
            active: Default::default(),
            editors: Default::default(),
            desktop_thread: Default::default(),
        }
    }
}

impl AssistantService {
    pub async fn sync(&self, app: &AppHandle, context: &AssistantContext) -> Result<(), ManagerError> {
        {
            let mut state = self.state.lock().await;
            state["enabled"] = json!(context.settings.enabled);
            state["startupError"] = json!("");
            state["ready"] = json!(false);
        }
        let result = if context.settings.enabled {
            self.rpc(app, context, "threads", json!({})).await.map(|_| ())
        } else {
            self.shutdown().await;
            Ok(())
        };
        {
            let mut state = self.state.lock().await;
            state["ready"] = json!(context.settings.enabled && result.is_ok());
            if let Err(error) = &result {
                state["startupError"] = json!(error.to_string());
            }
            if !context.settings.enabled {
                state["petAction"] = json!("");
                state["petMessage"] = json!("");
            }
        }
        self.emit_state(app).await;
        result
    }

    pub async fn shutdown(&self) {
        if let Some((_, token)) = self.active.lock().await.as_ref() {
            token.cancel();
        }
        if let Some(mut runtime) = self.runtime.lock().await.take() {
            // 关闭输入让 Node 结束会话落盘；退出应用时有界回收本次创建的子进程。
            let _ = runtime.input.lock().await.shutdown().await;
            if tokio::time::timeout(std::time::Duration::from_secs(3), runtime.child.wait()).await.is_err() {
                let _ = runtime.child.kill().await;
                let _ = runtime.child.wait().await;
            }
        }
    }

    async fn rpc(
        &self,
        app: &AppHandle,
        context: &AssistantContext,
        method: &str,
        params: Value,
    ) -> Result<Value, ManagerError> {
        if !context.settings.enabled {
            return Err(ManagerError::System("应用助手已关闭，请在设置 → 功能扩展中启用".into()));
        }
        let mut runtime = self.runtime.lock().await;
        if runtime.as_ref().is_some_and(|previous| previous.storage_dir != context.paths.storage_dir)
            && self.active.lock().await.is_some()
        {
            return Err(ManagerError::System("助手正在执行任务，请结束后再切换数据目录".into()));
        }
        let replace = match runtime.as_mut() {
            Some(runtime) => runtime.storage_dir != context.paths.storage_dir
                || runtime.child.try_wait()?.is_some(),
            None => true,
        };
        if replace {
            if let Some(mut previous) = runtime.take() {
                let _ = previous.child.kill().await;
                let _ = previous.child.wait().await;
            }
            let development_entry = context.workspace_root.join("src-tauri/node/assistant/agent.mjs");
            let entry = [
                // 开发时从源码资源启动，避免常驻原生模块锁住 Cargo 正在复制的资源。
                if cfg!(debug_assertions) { development_entry.clone() } else { context.resource_dir.join("node/assistant/agent.mjs") },
                context.resource_dir.join("node/assistant/agent.mjs"),
                development_entry,
                std::env::current_exe()?.parent().unwrap_or(std::path::Path::new(""))
                    .join("resources/node/assistant/agent.mjs"),
            ].into_iter().find(|path| path.is_file()).ok_or_else(|| ManagerError::System(
                "应用助手运行文件缺失，请重新构建或安装应用".into(),
            ))?;
            let mut command = crate::api::translation::node_command(&entry);
            command
                .env_remove("OPENAI_API_KEY").env_remove("ANTHROPIC_API_KEY")
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            let mut child = command.spawn().map_err(|error| ManagerError::System(format!(
                "无法启动应用助手，请安装 Node.js 22.13 或更高版本：{error}",
            )))?;
            let input = Arc::new(Mutex::new(child.stdin.take().unwrap()));
            let pending: Pending = Default::default();
            let output = child.stdout.take().unwrap();
            let errors = child.stderr.take().unwrap();
            // 保留有限的进程错误尾部，启动失败时返回真实原因，不落盘对话或工具内容。
            let errors_task = tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(errors).lines();
                let mut tail = String::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    tail.push_str(&line);
                    tail.push('\n');
                    if tail.len() > 8192 {
                        let mut start = tail.len() - 8192;
                        while !tail.is_char_boundary(start) { start += 1; }
                        tail.drain(..start);
                    }
                }
                tail
            });
            let service = self.clone();
            let handle = app.clone();
            let writer = input.clone();
            let replies = pending.clone();
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(output).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                    if let Some(id) = message["id"].as_str() {
                        if message["type"] != "tool" {
                            if let Some(reply) = replies.lock().await.replies.remove(id) {
                                let result = if let Some(error) = message["error"].as_str() {
                                    Err(error.to_string())
                                } else {
                                    Ok(message["result"].clone())
                                };
                                let _ = reply.send(result);
                            }
                            continue;
                        }
                    }
                    match message["type"].as_str() {
                        Some("event") => service.publish(&handle, message["event"].clone()).await,
                        Some("tool") => {
                            let service = service.clone();
                            let app = handle.clone();
                            let writer = writer.clone();
                            tauri::async_runtime::spawn(async move {
                                let id = message["id"].clone();
                                let result = tools::execute(&service, &app, &message).await;
                                let response = match result {
                                    Ok(result) => json!({"type": "tool-result", "id": id, "result": result}),
                                    Err(error) => json!({"type": "tool-result", "id": id, "error": error.to_string()}),
                                };
                                let _ = write_message(&writer, &response).await;
                            });
                        }
                        _ => {}
                    }
                }
                let details = errors_task.await.unwrap_or_default();
                let error = if details.trim().is_empty() {
                    "应用助手进程已退出，请重试".to_string()
                } else {
                    format!("应用助手进程已退出：\n{}", details.trim())
                };
                let mut pending = replies.lock().await;
                pending.closed_error = Some(error.clone());
                for (_, reply) in pending.replies.drain() {
                    let _ = reply.send(Err(error.clone()));
                }
            });
            let next = AssistantRuntime {
                child, input, pending, storage_dir: context.paths.storage_dir.clone(),
            };
            let initialized = request_on(&next.input, &next.pending, "initialize", json!({
                "storageDir": PathBuf::from(&context.paths.storage_dir).join("assistant")
            })).await;
            if let Err(error) = initialized {
                return Err(error);
            }
            *runtime = Some(next);
        }
        let connection = runtime.as_ref().unwrap();
        let input = connection.input.clone();
        let pending = connection.pending.clone();
        drop(runtime);
        // 释放运行时锁后再等待模型，取消、历史查询和工具回包可并行处理。
        request_on(&input, &pending, method, params).await
    }

    pub async fn dispatch(
        &self,
        app: AppHandle,
        context: AssistantContext,
        action: &str,
        payload: Value,
    ) -> Result<Value, ManagerError> {
        match action {
            "state" => Ok(self.state.lock().await.clone()),
            "editor-pending" => Ok(json!(self.editors.lock().await.values()
                .map(|request| request.payload.clone()).collect::<Vec<_>>())),
            "editor-result" => {
                let id = payload["id"].as_str().unwrap_or("");
                if let Some(request) = self.editors.lock().await.remove(id) {
                    let result = if let Some(error) = payload["error"].as_str() {
                        Err(error.to_string())
                    } else {
                        Ok(payload["result"].clone())
                    };
                    let _ = request.reply.send(result);
                }
                Ok(json!(true))
            }
            "editor-claim" => {
                let active = self.active.lock().await;
                let allowed = active.as_ref().is_some_and(|(id, token)| {
                    payload["runId"] == *id && !token.is_cancelled()
                });
                let mut editors = self.editors.lock().await;
                let request = editors.get_mut(payload["id"].as_str().unwrap_or(""));
                if allowed {
                    if let Some(request) = request {
                        if request.payload["claimed"] != true {
                            request.payload["claimed"] = json!(true);
                            return Ok(json!(true));
                        }
                    }
                }
                Ok(json!(false))
            }
            "catalog" => {
                let snapshot = app.state::<crate::core::state::AppState>().state_snapshot().await;
                Ok(json!({
                    "tools": tools::catalog(),
                    "providers": snapshot["providers"].as_array().map(|items| items.iter().map(|item| json!({"id": item["id"], "name": item["name"], "cli": item["cli"], "enabled": item["enabled"], "runtimeConfig": {"mainModel": item["runtimeConfig"]["mainModel"]}})).collect::<Vec<_>>()).unwrap_or_default(),
                    "codexAccounts": snapshot["codexAccounts"].as_array().map(|items| items.iter().map(|item| json!({"id": item["id"], "email": item["email"], "accountId": item["accountId"], "disabled": item["disabled"]})).collect::<Vec<_>>()).unwrap_or_default(),
                    "runtimeModels": snapshot["runtimeModels"],
                    "softwareSkills": snapshot["skills"].as_array().map(|items| items.iter().map(|item| json!({"id": item["id"], "name": item["name"], "description": item["description"], "disabled": item["disabled"], "installedTargets": item["installedTargets"]})).collect::<Vec<_>>()).unwrap_or_default(),
                }))
            }
            "bootstrap" => {
                if !context.settings.enabled {
                    return Ok(json!({"threads": {"threads": [], "hasMore": false},
                        "state": self.state.lock().await.clone(), "settings": context.settings}));
                }
                let threads = self.rpc(&app, &context, "threads", json!({})).await?;
                Ok(json!({"threads": threads, "state": self.state.lock().await.clone(),
                    "settings": context.settings}))
            }
            "public-memory" | "save-public-memory" | "context" => {
                let mut params = payload;
                if action == "context" {
                    validate_thread_id(&params)?;
                    params["settings"] = json!(context.settings);
                }
                self.rpc(&app, &context, action, params).await
            }
            "threads" | "create-thread" | "messages" | "delete-thread" => {
                if matches!(action, "messages" | "delete-thread") {
                    validate_thread_id(&payload)?;
                }
                if action == "delete-thread" {
                    let state = self.state.lock().await;
                    if is_running(&state) && state["threadId"] == payload["threadId"] {
                        return Err(ManagerError::System("请先停止当前会话的任务".into()));
                    }
                    drop(state);
                    let mut thread = self.desktop_thread.lock().await;
                    if payload["threadId"] == thread.1 {
                        thread.1.clear();
                    }
                }
                self.rpc(&app, &context, action, payload).await
            }
            "send" | "desktop-send" | "compress" => {
                let desktop = action == "desktop-send";
                let compress = action == "compress";
                if !context.settings.enabled {
                    return Err(ManagerError::System("应用助手已关闭，请在设置 → 功能扩展中启用".into()));
                }
                if !desktop {
                    validate_thread_id(&payload)?;
                }
                let message = payload["message"].as_str().unwrap_or("").trim().to_string();
                let images = if compress { Vec::new() } else { images::validate(&payload["images"])? };
                if !compress && ((message.is_empty() && images.is_empty()) || message.chars().count() > 30_000) {
                    return Err(ManagerError::System("请输入消息或粘贴图片，文字最多 30000 个字符".into()));
                }
                if context.settings.target_id.is_empty() || context.settings.model.is_empty() {
                    return Err(ManagerError::System("请先选择模型来源和模型".into()));
                }
                let mut active = self.active.lock().await;
                if active.is_some() {
                    return Err(ManagerError::System("已有任务正在执行，请先停止或等待完成".into()));
                }
                let reply_at_pet = if desktop {
                    app.state::<crate::core::state::AppState>().show_assistant_reply(&app, true).await?
                } else { false };
                let run_id = uuid::Uuid::new_v4().to_string();
                let cancel = CancellationToken::new();
                *active = Some((run_id.clone(), cancel.clone()));
                drop(active);
                {
                    let mut state = self.state.lock().await;
                    state["status"] = json!("thinking");
                    state["runId"] = json!(run_id);
                    state["threadId"] = payload["threadId"].clone();
                    state["message"] = json!(message);
                    state["images"] = json!(images);
                    state["text"] = json!("");
                    state["error"] = json!("");
                    state["finishedAt"] = json!(0);
                    state["tools"] = json!([]);
                    state["surface"] = json!(if desktop { "desktop" } else { "panel" });
                    state["replyAtPet"] = json!(reply_at_pet);
                    state["operation"] = json!(if compress { "compress" } else { "chat" });
                    state["contextPhase"] = json!("");
                }
                self.emit_state(&app).await;
                let service = self.clone();
                let thread_id = payload["threadId"].clone();
                let response = self.state.lock().await.clone();
                tauri::async_runtime::spawn(async move {
                    let result = async {
                        // 桌面对话复用同一会话；先返回任务，再在后台建会话，输入框可继续编辑。
                        let thread_id = if desktop {
                            let mut thread = service.desktop_thread.lock().await;
                            if thread.0 != context.paths.storage_dir || thread.1.is_empty() {
                                let created = service.rpc(&app, &context, "create-thread", json!({})).await?;
                                *thread = (context.paths.storage_dir.clone(), created["id"].as_str().unwrap_or("").to_string());
                            }
                            let id = json!(thread.1);
                            service.state.lock().await["threadId"] = id.clone();
                            drop(thread);
                            service.emit_state(&app).await;
                            id
                        } else { thread_id };
                        let target = proxy::prepare_assistant_target(
                            &context.paths, &context.cli_targets, &context.settings,
                        ).await?;
                        let api = target.info["api"].clone();
                        if cancel.is_cancelled() {
                            return Ok(json!({"cancelled": true}));
                        }
                        let proxy = LlmProxy::start_assistant(context.paths.clone(), target).await?;
                        service.rpc(&app, &context, if compress { "compress" } else { "run" }, json!({
                            "runId": run_id, "threadId": thread_id, "message": message, "images": images,
                            "targetId": context.settings.target_id, "model": context.settings.model,
                            "baseURL": proxy.base_url, "token": proxy.token, "api": api,
                            "settings": context.settings
                        })).await
                    }.await;
                    {
                        let mut state = service.state.lock().await;
                        match result {
                            _ if cancel.is_cancelled() => state["status"] = json!("cancelled"),
                            Ok(result) if result["cancelled"] == true => state["status"] = json!("cancelled"),
                            Ok(result) => {
                                state["status"] = json!("completed");
                                if let Some(text) = result["text"].as_str() {
                                    state["text"] = json!(text);
                                }
                            }
                            Err(error) => {
                                state["status"] = json!("failed");
                                state["error"] = json!(error.to_string());
                            }
                        }
                    }
                    service.state.lock().await["finishedAt"] = json!(std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64);
                    *service.active.lock().await = None;
                    service.emit_state(&app).await;
                });
                Ok(response)
            }
            "cancel" => {
                let active = self.active.lock().await;
                let run_id = if let Some((id, token)) = active.as_ref() {
                    if payload["runId"].as_str().is_some_and(|expected| expected != id) {
                        return Err(ManagerError::System("任务已更新，请刷新后重试".into()));
                    }
                    token.cancel();
                    id.clone()
                } else {
                    return Ok(self.state.lock().await.clone());
                };
                drop(active);
                {
                    let mut state = self.state.lock().await;
                    if state["runId"] == run_id && is_running(&state) {
                        state["status"] = json!("cancelling");
                    }
                }
                self.emit_state(&app).await;
                self.rpc(&app, &context, "cancel", json!({"runId": run_id})).await?;
                Ok(self.state.lock().await.clone())
            }
            _ => Err(ManagerError::System("未知助手操作".into())),
        }
    }

    async fn publish(&self, app: &AppHandle, event: Value) {
        let mut state = self.state.lock().await;
        if event["runId"] != state["runId"] || !is_running(&state) {
            return;
        }
        match event["type"].as_str() {
            Some("context") => {
                state["contextPhase"] = event["phase"].clone();
            }
            Some("text-delta") => {
                let text = state["text"].as_str().unwrap_or("").to_string()
                    + event["text"].as_str().unwrap_or("");
                state["text"] = json!(text);
                if state["status"] != "cancelling" {
                    state["status"] = json!("replying");
                }
                drop(state);
                let _ = app.emit("assistant:event", event);
                return;
            }
            Some("tool-start") => {
                if state["status"] != "cancelling" {
                    state["status"] = json!("working");
                }
                let mut tool = event.clone();
                tool["status"] = json!("running");
                state["tools"].as_array_mut().unwrap().push(tool);
            }
            Some("tool-end" | "tool-error") => {
                if let Some(tool) = state["tools"].as_array_mut().unwrap().iter_mut()
                    .find(|tool| tool["id"] == event["id"])
                {
                    tool["status"] = json!(if event["type"] == "tool-error" { "failed" } else { "completed" });
                    tool["output"] = event["output"].clone();
                    tool["error"] = event["error"].clone();
                }
                if state["status"] != "cancelling" {
                    state["status"] = json!("thinking");
                }
            }
            _ => return,
        }
        drop(state);
        self.emit_state(app).await;
    }

    pub(super) async fn emit_state(&self, app: &AppHandle) {
        let _ = app.emit("assistant:event", json!({
            "type": "state", "state": self.state.lock().await.clone()
        }));
    }
}

fn is_running(state: &Value) -> bool {
    matches!(state["status"].as_str(), Some("thinking" | "working" | "replying" | "cancelling"))
}

fn validate_thread_id(payload: &Value) -> Result<(), ManagerError> {
    uuid::Uuid::parse_str(payload["threadId"].as_str().unwrap_or(""))
        .map_err(|_| ManagerError::System("会话标识无效".into()))?;
    Ok(())
}

async fn write_message(input: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<(), ManagerError> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    input.lock().await.write_all(&bytes).await?;
    Ok(())
}

async fn request_on(
    input: &Arc<Mutex<ChildStdin>>,
    pending: &Pending,
    method: &str,
    params: Value,
) -> Result<Value, ManagerError> {
    let id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    {
        let mut pending = pending.lock().await;
        if let Some(error) = &pending.closed_error {
            return Err(ManagerError::System(error.clone()));
        }
        pending.replies.insert(id.clone(), sender);
    }
    if let Err(error) = write_message(input, &json!({"id": id, "method": method, "params": params})).await {
        pending.lock().await.replies.remove(&id);
        return Err(error);
    }
    receiver.await.map_err(|_| ManagerError::System("助手连接已关闭".into()))?
        .map_err(ManagerError::System)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn assistant_starts_from_tauri_extended_resource_path() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let root = std::env::temp_dir().join(format!("assistant-startup-{}", uuid::Uuid::new_v4()));
            let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("node/assistant/agent.mjs").canonicalize().unwrap();
            assert!(entry.to_string_lossy().starts_with(r"\\?\"));
            let mut command = crate::api::translation::node_command(&entry);
            command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
                .kill_on_drop(true).creation_flags(0x08000000);
            let mut child = command.spawn().unwrap();
            let mut input = child.stdin.take().unwrap();
            let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
            let request = json!({"id": "startup", "method": "initialize", "params": {"storageDir": root}});
            input.write_all(format!("{request}\n").as_bytes()).await.unwrap();
            let line = tokio::time::timeout(std::time::Duration::from_secs(15), lines.next_line())
                .await.unwrap().unwrap().expect("助手应从真实资源路径启动");
            let response: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["result"]["ready"], true, "{response}");
            drop(input);
            assert!(tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
                .await.unwrap().unwrap().success());
            let resolved = root.canonicalize().unwrap();
            assert_eq!(resolved.parent().unwrap(), std::env::temp_dir().canonicalize().unwrap());
            std::fs::remove_dir_all(resolved).unwrap();
        });
    }
}

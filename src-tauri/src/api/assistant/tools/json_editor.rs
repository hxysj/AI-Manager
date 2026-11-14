use super::super::{AssistantService, EditorRequest};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use crate::core::{error::ManagerError, state::AppState};
use tauri::{AppHandle, Manager, Emitter};

// 此模块只处理对应能力，任务身份和工具开关由统一入口校验。
pub(super) async fn execute(service: &AssistantService, app: &AppHandle, args: &Value, cancellation: CancellationToken, request: &Value) -> Result<Value, ManagerError> {
    let state = app.state::<AppState>();
    if request["name"] == "write_json_editor" {
        let result = args["result"].as_str().ok_or_else(|| ManagerError::System("缺少 JSON 结果".into()))?;
        if result.chars().count() > 60_000 || args["source"].as_str().is_some_and(|text| text.chars().count() > 60_000) {
            return Err(ManagerError::System("编辑内容最多 60000 个字符".into()));
        }
        serde_json::from_str::<Value>(result)
            .map_err(|error| ManagerError::System(format!("JSON 校验失败：{error}")))?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let payload = json!({
        "id": id, "operation": if request["name"] == "write_json_editor" { "write" } else { "read" },
        "source": args["source"], "result": args["result"], "runId": request["runId"],
    });
    let (reply, received) = oneshot::channel();
    service.editors.lock().await.insert(id.clone(), EditorRequest { payload: payload.clone(), reply });
    let result = async {
        if app.get_webview_window("tool-window-json-agent").is_some() {
            crate::api::app::open_tool_window(app, "json-agent")?;
        } else {
            Box::pin(state.dispatch(app.clone(), "quick-switch:show-main",
                Some(json!({"toolId": "json-agent"})))).await?;
        }
        app.emit("assistant:editor-request", payload)
            .map_err(|error| ManagerError::System(error.to_string()))?;
        // 窗口启动和编辑回执有界等待，未收到回执不能向模型宣称已写入。
        match futures_util::future::select(
            Box::pin(cancellation.cancelled()),
            Box::pin(tokio::time::timeout(std::time::Duration::from_secs(15), received)),
        ).await {
            futures_util::future::Either::Left(_) => Err(ManagerError::System("任务已停止".into())),
            futures_util::future::Either::Right((result, _)) =>
                result.map_err(|_| ManagerError::System("JSON 工具未就绪，请打开工具后重试".into()))?
                    .map_err(|_| ManagerError::System("JSON 编辑请求已关闭".into()))?
                    .map_err(ManagerError::System),
        }
    }.await;
    service.editors.lock().await.remove(&id);
    result
}

use super::super::{AssistantService, EditorRequest};
use crate::core::error::ManagerError;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

pub(in crate::api::assistant) async fn dispatch(
    service: &AssistantService,
    action: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let label = payload["windowLabel"].as_str().unwrap_or("");
    if action == "ui-pending" {
        return Ok(json!(service.ui_requests.lock().await.values()
            .filter(|request| request.payload["windowLabel"] == label)
            .map(|request| request.payload.clone()).collect::<Vec<_>>()));
    }
    let active = service.active.lock().await;
    if !active.as_ref().is_some_and(|(run_id, token)| {
        payload["runId"] == *run_id && !token.is_cancelled()
    }) {
        return Ok(json!(false));
    }
    let mut requests = service.ui_requests.lock().await;
    let id = payload["id"].as_str().unwrap_or("");
    let Some(request) = requests.get_mut(id).filter(|request| {
        request.payload["windowLabel"] == label && request.payload["runId"] == payload["runId"]
    }) else {
        return Ok(json!(false));
    };
    match action {
        "ui-claim" if request.payload["claimed"] != true => {
            request.payload["claimed"] = json!(true);
            Ok(json!(true))
        }
        "ui-result" if request.payload["claimed"] == true => {
            let request = requests.remove(id).unwrap();
            let result = if let Some(error) = payload["error"].as_str() {
                Err(error.to_string())
            } else if !payload["result"].is_object() {
                Err("界面回执无效，请重新读取界面".to_string())
            } else {
                Ok(payload["result"].clone())
            };
            let _ = request.reply.send(result);
            Ok(json!(true))
        }
        _ => Ok(json!(false)),
    }
}

pub(super) async fn execute(
    service: &AssistantService,
    app: &AppHandle,
    args: &Value,
    cancellation: CancellationToken,
    request: &Value,
) -> Result<Value, ManagerError> {
    let mut windows = Vec::new();
    for (label, window) in app.webview_windows() {
        if label != "main" && !label.starts_with("tool-window-") {
            continue;
        }
        windows.push(json!({
            "windowLabel": label,
            "title": window.title().map_err(failure)?,
            "visible": window.is_visible().map_err(failure)?,
            "focused": window.is_focused().map_err(failure)?
        }));
    }
    windows.sort_by_key(|window| window["windowLabel"].as_str().unwrap_or("").to_string());
    let label = args["windowLabel"].as_str().filter(|label| !label.is_empty())
        .or_else(|| windows.iter().find(|window| window["focused"] == true)
            .and_then(|window| window["windowLabel"].as_str()))
        .unwrap_or("main");
    if !windows.iter().any(|window| window["windowLabel"] == label) {
        return Err(failure("目标应用窗口尚未打开，请先使用打开窗口工具"));
    }
    let window = app.get_webview_window(label)
        .ok_or_else(|| failure("目标应用窗口已关闭"))?;
    let interact = request["name"] == "interact_app_ui";
    if interact && (!matches!(args["action"].as_str(), Some("focus" | "fill" | "click" | "select" | "scroll"))
        || args["snapshotId"].as_str().unwrap_or("").is_empty()
        || args["nodeId"].as_str().unwrap_or("").is_empty()
        || args["text"].as_str().is_some_and(|text| text.chars().count() > 30_000))
    {
        return Err(failure("界面操作参数无效，请先读取当前控件"));
    }
    if cancellation.is_cancelled() {
        return Err(failure("任务已停止"));
    }
    if interact {
        window.unminimize().map_err(failure)?;
        window.show().map_err(failure)?;
        window.set_focus().map_err(failure)?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let payload = json!({
        "id": id, "runId": request["runId"], "windowLabel": label,
        "operation": if interact { "interact" } else { "read" }, "args": args
    });
    let (reply, received) = oneshot::channel();
    service.ui_requests.lock().await.insert(id.clone(), EditorRequest { payload: payload.clone(), reply });
    let result = async {
        window.emit("assistant:ui-request", payload).map_err(failure)?;
        // 页面首次加载时由待处理快照补领；取消或没有回执时不能宣称操作完成。
        match futures_util::future::select(
            Box::pin(cancellation.cancelled()),
            Box::pin(tokio::time::timeout(std::time::Duration::from_secs(15), received)),
        ).await {
            futures_util::future::Either::Left(_) => Err(failure("任务已停止")),
            futures_util::future::Either::Right((response, _)) =>
                response.map_err(|_| failure("应用界面未就绪，请稍后重新读取"))?
                    .map_err(|_| failure("界面操作请求已关闭"))?
                    .map_err(failure),
        }
    }.await;
    service.ui_requests.lock().await.remove(&id);
    let mut result = result?;
    result["windowLabel"] = json!(label);
    result["windows"] = json!(windows);
    Ok(result)
}

fn failure(error: impl std::fmt::Display) -> ManagerError {
    ManagerError::System(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pending(service: &AssistantService) -> oneshot::Receiver<Result<Value, String>> {
        let (reply, received) = oneshot::channel();
        service.ui_requests.lock().await.insert("request".into(), EditorRequest {
            payload: json!({
                "id": "request", "runId": "run", "windowLabel": "tool-window-image-workbench"
            }),
            reply,
        });
        received
    }

    #[test]
    fn ui_reply_requires_matching_task_window_and_single_claim() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let service = AssistantService::default();
            *service.active.lock().await = Some(("run".into(), CancellationToken::new()));
            let received = pending(&service).await;
            let identity = json!({
                "id": "request", "runId": "run", "windowLabel": "tool-window-image-workbench",
                "result": {"snapshotId": "snapshot"}
            });
            assert_eq!(dispatch(&service, "ui-pending", json!({"windowLabel": "main"})).await.unwrap(), json!([]));
            assert_eq!(dispatch(&service, "ui-pending", identity.clone()).await.unwrap().as_array().unwrap().len(), 1);
            assert_eq!(dispatch(&service, "ui-result", identity.clone()).await.unwrap(), false);
            for (field, value) in [("windowLabel", "main"), ("runId", "other-run"), ("id", "other-id")] {
                let mut wrong = identity.clone();
                wrong[field] = json!(value);
                assert_eq!(dispatch(&service, "ui-claim", wrong).await.unwrap(), false);
            }
            assert_eq!(dispatch(&service, "ui-claim", identity.clone()).await.unwrap(), true);
            assert_eq!(dispatch(&service, "ui-claim", identity.clone()).await.unwrap(), false);
            let mut wrong = identity.clone();
            wrong["windowLabel"] = json!("main");
            assert_eq!(dispatch(&service, "ui-result", wrong).await.unwrap(), false);
            assert_eq!(dispatch(&service, "ui-result", identity.clone()).await.unwrap(), true);
            assert_eq!(received.await.unwrap().unwrap(), json!({"snapshotId": "snapshot"}));
            assert!(service.ui_requests.lock().await.is_empty());
            assert_eq!(dispatch(&service, "ui-result", identity.clone()).await.unwrap(), false);
            let malformed = pending(&service).await;
            assert_eq!(dispatch(&service, "ui-claim", identity.clone()).await.unwrap(), true);
            let mut invalid = identity;
            invalid["result"] = json!(true);
            assert_eq!(dispatch(&service, "ui-result", invalid).await.unwrap(), true);
            assert!(malformed.await.unwrap().unwrap_err().contains("回执无效"));
        });
    }

    #[test]
    fn ui_cancelled_or_replaced_task_cannot_claim_or_complete() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let service = AssistantService::default();
            let cancellation = CancellationToken::new();
            *service.active.lock().await = Some(("run".into(), cancellation.clone()));
            let received = pending(&service).await;
            let identity = json!({
                "id": "request", "runId": "run", "windowLabel": "tool-window-image-workbench",
                "error": "目标控件当前不可用"
            });
            cancellation.cancel();
            assert_eq!(dispatch(&service, "ui-claim", identity.clone()).await.unwrap(), false);
            *service.active.lock().await = Some(("run".into(), CancellationToken::new()));
            assert_eq!(dispatch(&service, "ui-claim", identity.clone()).await.unwrap(), true);
            *service.active.lock().await = Some(("new-run".into(), CancellationToken::new()));
            assert_eq!(dispatch(&service, "ui-result", identity.clone()).await.unwrap(), false);
            *service.active.lock().await = Some(("run".into(), CancellationToken::new()));
            assert_eq!(dispatch(&service, "ui-result", identity).await.unwrap(), true);
            assert_eq!(received.await.unwrap().unwrap_err(), "目标控件当前不可用");
        });
    }
}

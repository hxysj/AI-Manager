use crate::core::{quick_notes_store as store, paths::AppPaths};
use serde_json::json;
use rusqlite::params;
use tauri::{AppHandle, Emitter};
#[cfg(not(windows))]
use tauri_plugin_notification::NotificationExt;

// 调度只由应用后端运行，不依赖工具页面；每次通知先持久化并领取唯一事件。
pub fn tick(app: &AppHandle,paths: &AppPaths) -> Result<(),crate::core::error::ManagerError> {
    let events=store::due_events(paths,store::now())?;
    if events.is_empty(){return Ok(());}
    for event in events {
        let late=store::now()-event["scheduledAt"].as_i64().unwrap_or(0)>60_000;
        let body=format!("{}{}\n可在速记便签的提醒中心查看、完成或稍后提醒。",if late{"错过的提醒 · "}else{""},store::text(&event["note"],"content").chars().take(180).collect::<String>());
        let result=notify(app,&event,&body);
        store::open(paths)?.execute("UPDATE quick_note_reminder_events SET status=?2,error=?3 WHERE id=?1",
            params![store::text(&event,"id"),if result.is_ok(){"sent"}else{"failed"},result.err().map(|e|e.to_string()).unwrap_or_default()])?;
    }
    app.emit("quick-notes:changed",json!({"reminders":true})).ok();
    Ok(())
}

#[cfg(not(windows))]
fn notify(app: &AppHandle,event: &serde_json::Value,body: &str) -> Result<(),String> {
    app.notification().builder().title(format!("速记 · {}",store::text(event,"title"))).body(body).show().map_err(|e|e.to_string())
}

// Windows 通知保留稳定标识，点击后定位原便签；通知本身不打开或聚焦窗口。
#[cfg(windows)]
fn notify(app: &AppHandle,event: &serde_json::Value,body: &str) -> Result<(),String> {
    use windows::{core::HSTRING, Data::Xml::Dom::XmlDocument, Foundation::TypedEventHandler, UI::Notifications::{ToastNotification,ToastNotificationManager}};
    static SHOWN:std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String,ToastNotification>>>=std::sync::OnceLock::new();
    let escape=|s:&str|s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;");
    let xml=XmlDocument::new().map_err(|e|e.to_string())?;
    let audio=if event["schedule"]["sound"]==false{"<audio silent=\"true\"/>"}else{""};
    xml.LoadXml(&HSTRING::from(format!("<toast><visual><binding template=\"ToastGeneric\"><text>速记 · {}</text><text>{}</text></binding></visual>{audio}</toast>",escape(store::text(event,"title")),escape(body)))).map_err(|e|e.to_string())?;
    let toast=ToastNotification::CreateToastNotification(&xml).map_err(|e|e.to_string())?;
    toast.SetTag(&HSTRING::from(store::text(event,"id").chars().take(16).collect::<String>())).map_err(|e|e.to_string())?;
    toast.SetGroup(&HSTRING::from("quick-notes")).map_err(|e|e.to_string())?;
    let handle=app.clone();let note_id=store::text(event,"noteId").to_string();
    toast.Activated(&TypedEventHandler::new(move |_,_| {
        let app=handle.clone();let id=note_id.clone();
        tauri::async_runtime::spawn(async move {
            use tauri::Manager;
            if let Err(error)=app.state::<crate::core::state::AppState>().dispatch(app.clone(),"quick-notes:open",Some(json!({"id":id}))).await{eprintln!("打开提醒便签失败：{error}");}
        });
        Ok(())
    })).map_err(|e|e.to_string())?;
    // 开发程序未安装应用快捷方式，沿用通知插件的 PowerShell 系统标识。
    let app_id=if cfg!(debug_assertions){r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe".to_owned()}else{app.config().identifier.clone()};
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id)).and_then(|notifier|notifier.Show(&toast)).map_err(|e|e.to_string())?;
    // 保留回调对象到通知关闭，防止 Rust 作用域结束后点击事件丢失。
    let key=store::text(event,"id").to_string();
    let dismissed=key.clone();
    toast.Dismissed(&TypedEventHandler::new(move |_,_| {if let Some(shown)=SHOWN.get(){shown.lock().unwrap().remove(&dismissed);}Ok(())})).map_err(|e|e.to_string())?;
    SHOWN.get_or_init(Default::default).lock().map_err(|e|e.to_string())?.insert(key,toast);
    Ok(())
}

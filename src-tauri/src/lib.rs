mod api;
mod core;

use crate::api::{app, app_logs};
use crate::core::state::{AppState, ManagerState};
use serde_json::Value;
use tauri::{Emitter, Manager};

#[tauri::command]
async fn dispatch_api(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    channel: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    if channel.starts_with("app-log:") {
        return state
            .dispatch(app, &channel, payload)
            .await
            .map_err(|error| error.to_string());
    }

    let app_data_path = state.app_data_path().await;
    let started = app_logs::record_start(&app_data_path, &channel, payload.as_ref())
        .await
        .ok();
    let result = state
        .dispatch(app, &channel, payload)
        .await
        .map_err(|error| error.to_string());

    if let Some(started) = started {
        let _ = app_logs::record_finish(&app_data_path, &channel, started, &result).await;
    }

    result
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if matches!(event, tauri::WindowEvent::Focused(_)) {
                let app_handle = window.app_handle().clone();

                tauri::async_runtime::spawn(async move {
                    let Some(state) = app_handle.try_state::<AppState>() else {
                        return;
                    };
                    let (app_settings, quick_switch_collapsed) =
                        state.quick_switch_settings().await;
                    if let Err(error) = app::sync_quick_switch_window(
                        &app_handle,
                        &app_settings,
                        quick_switch_collapsed,
                    ) {
                        eprintln!("{error}");
                    }
                });
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app_handle = window.app_handle().clone();
                let window = window.clone();

                tauri::async_runtime::spawn(async move {
                    let Some(state) = app_handle.try_state::<AppState>() else {
                        return;
                    };
                    let close_action = state.close_action().await;

                    if close_action == "minimize" {
                        if let Err(error) = window.hide() {
                            eprintln!("{error}");
                        }
                        let (app_settings, quick_switch_collapsed) =
                            state.quick_switch_settings().await;
                        if let Err(error) = app::sync_quick_switch_window(
                            &app_handle,
                            &app_settings,
                            quick_switch_collapsed,
                        ) {
                            eprintln!("{error}");
                        }
                        return;
                    }

                    if close_action == "quit" {
                        app_handle.exit(0);
                        return;
                    }

                    if let Err(error) = window.emit("app:close-requested", Value::Bool(true)) {
                        eprintln!("{error}");
                    }
                });
            }
        })
        .setup(|app| {
            let manager_state = ManagerState::new(app.handle())?;
            app::sync_quick_switch_window(
                app.handle(),
                manager_state.app_settings(),
                manager_state.quick_switch_collapsed(),
            )?;
            app.manage(AppState::new(manager_state));
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Err(error) = state.start_enabled_proxy_servers().await {
                        eprintln!("{error}");
                    }
                }
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        if let Err(error) = state.create_local_backup_if_due().await {
                            eprintln!("{error}");
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![dispatch_api])
        .run(tauri::generate_context!())
        .expect("启动 Monkey Thief 失败")
}

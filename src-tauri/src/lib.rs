mod api;
mod core;

use crate::api::{app, app_logs};
use crate::core::state::{AppState, ManagerState};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
};

static CLOSE_DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

#[tauri::command]
async fn dispatch_api(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    channel: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    if channel.starts_with("app-log:") {
        return state
            .dispatch(app.clone(), &channel, payload)
            .await
            .map_err(|error| error.to_string());
    }

    let app_data_path = state.app_data_path().await;
    let started = app_logs::record_start(&app_data_path, &channel, payload.as_ref())
        .await
        .ok();
    let result = state
        .dispatch(app.clone(), &channel, payload)
        .await
        .map_err(|error| error.to_string());

    if let Some(started) = started {
        let _ = app_logs::record_finish(&app_data_path, &channel, started, &result).await;
    }

    if result.is_ok() {
        let snapshot = state.state_snapshot().await;
        if let Err(error) = update_tray_menu(&app, &snapshot) {
            eprintln!("{error}");
        }
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

            if matches!(
                event,
                tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_)
            ) {
                sync_quick_switch_from_window_event(window.app_handle().clone());
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

                    if CLOSE_DIALOG_OPEN.swap(true, Ordering::SeqCst) {
                        return;
                    }

                    if let Err(error) =
                        app_handle.emit("app:close-requested", Value::Bool(true))
                    {
                        CLOSE_DIALOG_OPEN.store(false, Ordering::SeqCst);
                        eprintln!("{error}");
                    }
                });
            }
        })
        .setup(|app| {
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ))?;
            let manager_state = ManagerState::new(app.handle())?;
            app::apply_auto_launch_setting(app.handle(), manager_state.app_settings())?;
            app::sync_quick_switch_window(
                app.handle(),
                manager_state.app_settings(),
                manager_state.quick_switch_collapsed(),
            )?;
            create_tray(app.handle())?;
            app.manage(AppState::new(manager_state));
            if let Some(state) = app.handle().try_state::<AppState>() {
                let snapshot = tauri::async_runtime::block_on(state.state_snapshot());
                update_tray_menu(app.handle(), &snapshot)?;
            }
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

fn create_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let menu = build_tray_menu(app, &Value::Null)?;

    TrayIconBuilder::with_id("main")
        .tooltip("Monkey Thief")
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let item_id = event.id().as_ref().to_string();

            match item_id.as_str() {
                "show-main" => show_main_from_tray(app),
                "quit" => app.exit(0),
                _ if item_id.starts_with("tray:") => run_tray_action(app, item_id),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_from_tray(&tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn update_tray_menu(app: &tauri::AppHandle, state: &Value) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main") else {
        return Ok(());
    };

    tray.set_menu(Some(build_tray_menu(app, state)?))?;
    tray.set_tooltip(Some(build_tray_tooltip(state)))?;
    Ok(())
}

fn build_tray_menu(app: &tauri::AppHandle, state: &Value) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let show_item = MenuItem::with_id(app, "show-main", "打开主面板", true, None::<&str>)?;
    let switch_menu = Submenu::new(app, "Provider 快速切换", true)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    append_quick_switch_tray_items(app, &switch_menu, state)?;
    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&switch_menu)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit_item)?;
    Ok(menu)
}

fn append_quick_switch_tray_items(
    app: &tauri::AppHandle,
    menu: &Submenu<tauri::Wry>,
    state: &Value,
) -> tauri::Result<()> {
    let cli_targets = visible_cli_targets(state);

    if cli_targets.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "tray:none",
            "暂无可用 CLI",
            false,
            None::<&str>,
        )?)?;
        return Ok(());
    }

    for cli in cli_targets {
        let cli_id = string_value(cli.get("id"));
        let cli_name = string_value(cli.get("name"));
        let cli_menu = Submenu::new(
            app,
            format!("{}：{}", cli_name, active_cli_name(state, &cli_id)),
            true,
        )?;

        cli_menu.append(&MenuItem::with_id(
            app,
            format!("tray:{}:current", cli_id),
            format!("当前应用：{}", cli_name),
            false,
            None::<&str>,
        )?)?;
        cli_menu.append(&PredefinedMenuItem::separator(app)?)?;

        if cli_id == "codex" {
            append_codex_tray_items(app, &cli_menu, state)?;
        } else {
            append_provider_tray_items(app, &cli_menu, state, &cli_id)?;
        }

        menu.append(&cli_menu)?;
    }

    Ok(())
}

fn append_provider_tray_items(
    app: &tauri::AppHandle,
    menu: &Submenu<tauri::Wry>,
    state: &Value,
    cli_id: &str,
) -> tauri::Result<()> {
    let providers = state_array(state, "providers")
        .into_iter()
        .filter(|provider| {
            string_value(provider.get("cli")) == cli_id
                && provider.get("enabled").and_then(Value::as_bool) != Some(false)
        })
        .collect::<Vec<_>>();

    if providers.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            format!("tray:{}:empty", cli_id),
            "暂无可用 Provider",
            false,
            None::<&str>,
        )?)?;
        return Ok(());
    }

    let active_provider_id = active_provider_id(state, cli_id);

    for provider in providers {
        let provider_id = string_value(provider.get("id"));
        let provider_name = string_value(provider.get("name"));
        let model = runtime_model(state, provider, cli_id);
        let active = provider_id == active_provider_id;
        let label = if active {
            format!("{}（已启用）", provider_name)
        } else if model.is_empty() {
            format!("{}（缺少模型）", provider_name)
        } else {
            provider_name
        };
        let item_id = if active {
            format!("tray:runtime:clear:{}", cli_id)
        } else {
            format!("tray:runtime:switch:{}:{}", cli_id, provider_id)
        };

        menu.append(&MenuItem::with_id(
            app,
            item_id,
            label,
            active || !model.is_empty(),
            None::<&str>,
        )?)?;
    }

    Ok(())
}

fn append_codex_tray_items(
    app: &tauri::AppHandle,
    menu: &Submenu<tauri::Wry>,
    state: &Value,
) -> tauri::Result<()> {
    let active_account = active_codex_account(state);
    let proxy_enabled = state
        .get("codexProxyState")
        .and_then(|proxy| proxy.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true);

    menu.append(&MenuItem::with_id(
        app,
        "tray:codex:current",
        format!("当前启用：{}", active_cli_name(state, "codex")),
        false,
        None::<&str>,
    )?)?;

    if proxy_enabled {
        menu.append(&MenuItem::with_id(
            app,
            "tray:codex:proxy:disable",
            "关闭 Proxy 接管",
            true,
            None::<&str>,
        )?)?;
    }

    if let Some(account) = active_account {
        let account_id = string_value(account.get("id"));

        menu.append(&MenuItem::with_id(
            app,
            format!("tray:codex:account:refresh:{}", account_id),
            "刷新当前账号额度",
            true,
            None::<&str>,
        )?)?;
        menu.append(&MenuItem::with_id(
            app,
            "tray:codex:account:clear",
            "取消启用",
            true,
            None::<&str>,
        )?)?;
    } else if !active_provider_id(state, "codex").is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "tray:runtime:clear:codex",
            "取消启用",
            true,
            None::<&str>,
        )?)?;
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    append_provider_tray_items(app, menu, state, "codex")?;

    let accounts = state_array(state, "codexAccounts");
    if !accounts.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    for account in accounts {
        let account_id = string_value(account.get("id"));
        let active = account.get("active").and_then(Value::as_bool) == Some(true);
        let disabled = account.get("disabled").and_then(Value::as_bool) == Some(true);
        let label = if active {
            format!("{}（已启用）", account_label(account))
        } else {
            account_label(account)
        };

        menu.append(&MenuItem::with_id(
            app,
            format!("tray:codex:account:enable:{}", account_id),
            label,
            !active && !disabled,
            None::<&str>,
        )?)?;
    }

    Ok(())
}

fn run_tray_action(app: &tauri::AppHandle, item_id: String) {
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        let Some(state) = app_handle.try_state::<AppState>() else {
            return;
        };

        match state.handle_tray_action(app_handle.clone(), &item_id).await {
            Ok(next_state) => {
                if let Err(error) = update_tray_menu(&app_handle, &next_state) {
                    eprintln!("{error}");
                }
            }
            Err(error) => eprintln!("{error}"),
        }
    });
}

fn show_main_from_tray(app: &tauri::AppHandle) {
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        let Some(state) = app_handle.try_state::<AppState>() else {
            return;
        };
        let (app_settings, quick_switch_collapsed) = state.quick_switch_settings().await;
        if let Err(error) =
            app::show_main_panel(&app_handle, &app_settings, quick_switch_collapsed)
        {
            eprintln!("{error}");
        }
    });
}

fn sync_quick_switch_from_window_event(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app_handle.try_state::<AppState>() else {
            return;
        };
        let (app_settings, quick_switch_collapsed) = state.quick_switch_settings().await;
        if let Err(error) =
            app::sync_quick_switch_window(&app_handle, &app_settings, quick_switch_collapsed)
        {
            eprintln!("{error}");
        }
    });
}

fn build_tray_tooltip(state: &Value) -> String {
    let mut lines = vec!["Monkey Thief".to_string()];

    for cli in visible_cli_targets(state) {
        let cli_id = string_value(cli.get("id"));
        let cli_name = string_value(cli.get("name"));

        lines.push(format!("{}: {}", cli_name, active_cli_name(state, &cli_id)));
    }

    lines.join("\n")
}

fn active_cli_name(state: &Value, cli_id: &str) -> String {
    if cli_id == "codex" {
        if state
            .get("codexProxyState")
            .and_then(|proxy| proxy.get("enabled"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            let target_id = state
                .get("codexProxyState")
                .and_then(|proxy| proxy.get("activeProviderId"))
                .and_then(Value::as_str)
                .unwrap_or("");

            if let Some(account_id) = target_id.strip_prefix("account:") {
                if let Some(account) = state_array(state, "codexAccounts")
                    .into_iter()
                    .find(|account| string_value(account.get("id")) == account_id)
                {
                    return format!("Proxy 接管中：{}", account_label(account));
                }
            }

            if let Some(provider) = state_array(state, "providers")
                .into_iter()
                .find(|provider| string_value(provider.get("id")) == target_id)
            {
                return format!("Proxy 接管中：{}", string_value(provider.get("name")));
            }

            return "Proxy 接管中".to_string();
        }

        if let Some(account) = active_codex_account(state) {
            return account_label(account);
        }
    }

    let provider_id = active_provider_id(state, cli_id);

    if provider_id.is_empty() {
        return "未启用".to_string();
    }

    state_array(state, "providers")
        .into_iter()
        .find(|provider| string_value(provider.get("id")) == provider_id)
        .map(|provider| string_value(provider.get("name")))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "未启用".to_string())
}

fn active_provider_id(state: &Value, cli_id: &str) -> String {
    state_array(state, "runtimeProfiles")
        .into_iter()
        .find(|profile| string_value(profile.get("cli")) == cli_id)
        .map(|profile| string_value(profile.get("providerId")))
        .unwrap_or_default()
}

fn active_codex_account(state: &Value) -> Option<&Value> {
    state_array(state, "codexAccounts")
        .into_iter()
        .find(|account| account.get("active").and_then(Value::as_bool) == Some(true))
}

fn runtime_model(state: &Value, provider: &Value, cli_id: &str) -> String {
    let runtime_config_model = provider
        .get("runtimeConfig")
        .and_then(|runtime_config| runtime_config.get("mainModel"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if !runtime_config_model.is_empty() {
        return runtime_config_model.to_string();
    }

    let profile_model = state_array(state, "runtimeProfiles")
        .into_iter()
        .find(|profile| string_value(profile.get("cli")) == cli_id)
        .and_then(|profile| profile.get("model"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if !profile_model.is_empty() {
        return profile_model.to_string();
    }

    let provider_id = string_value(provider.get("id"));

    state_array(state, "runtimeModels")
        .into_iter()
        .find(|model| string_value(model.get("providerId")) == provider_id)
        .and_then(|model| model.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn visible_cli_targets(state: &Value) -> Vec<&Value> {
    state_array(state, "cliTargets")
        .into_iter()
        .filter(|cli| {
            let cli_id = string_value(cli.get("id"));

            state
                .get("runtimeConfigSchemas")
                .and_then(|schemas| schemas.get(&cli_id))
                .and_then(|schema| schema.get("enabled"))
                .and_then(Value::as_bool)
                == Some(true)
        })
        .collect()
}

fn account_label(account: &Value) -> String {
    let name = first_text(account.get("email"), account.get("accountId"), "Codex 账号");
    let plan = first_text(account.get("plan"), None, "未知套餐");

    format!("{} · {}", name, plan)
}

fn first_text(first: Option<&Value>, second: Option<&Value>, fallback: &str) -> String {
    let first_text = string_value(first);

    if !first_text.is_empty() {
        return first_text;
    }

    let second_text = string_value(second);

    if !second_text.is_empty() {
        return second_text;
    }

    fallback.to_string()
}

fn state_array<'a>(state: &'a Value, key: &str) -> Vec<&'a Value> {
    state
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn reset_close_dialog_open() {
    CLOSE_DIALOG_OPEN.store(false, Ordering::SeqCst);
}

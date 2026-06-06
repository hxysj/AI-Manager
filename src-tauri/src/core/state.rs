use crate::api::{
    app, app_logs, codex_account, data, git_tool, proxy, repos, rules, runtime_provider, sessions,
    settings, skills, system, translation, usage,
};
use crate::core::error::ManagerError;
use crate::core::paths::{
    ensure_app_directories, resolve_app_paths, AppPaths, DEFAULT_USER_DATA_PATH,
};
use crate::core::settings::{load_app_settings, AppSettings};
use crate::core::storage_state::create_initial_state;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

pub struct ManagerState {
    app_data_path: PathBuf,
    workspace_root: PathBuf,
    resource_dir: PathBuf,
    paths: AppPaths,
    app_settings: AppSettings,
    quick_switch_collapsed: bool,
    data_backup_cache: data::DataBackupCache,
    codex_login_cache: codex_account::CodexLoginCache,
    proxy_server_registry: proxy::ProxyServerRegistry,
    state: Value,
}

pub struct AppState {
    manager: Mutex<ManagerState>,
}

impl AppState {
    pub fn new(manager: ManagerState) -> Self {
        Self {
            manager: Mutex::new(manager),
        }
    }

    pub async fn dispatch(
        &self,
        app: AppHandle,
        channel: &str,
        payload: Option<Value>,
    ) -> Result<Value, ManagerError> {
        let mut manager = self.manager.lock().await;
        manager.dispatch(app, channel, payload).await
    }

    pub async fn app_data_path(&self) -> PathBuf {
        let manager = self.manager.lock().await;

        manager.app_data_path.clone()
    }

    pub async fn close_action(&self) -> String {
        let manager = self.manager.lock().await;

        manager.app_settings.system.close_action.clone()
    }

    pub async fn quick_switch_settings(&self) -> (AppSettings, bool) {
        let manager = self.manager.lock().await;

        (manager.app_settings.clone(), manager.quick_switch_collapsed)
    }

    pub async fn create_local_backup_if_due(&self) -> Result<(), ManagerError> {
        let mut manager = self.manager.lock().await;
        let app_data_path = manager.app_data_path.clone();
        let paths = manager.paths.clone();

        if !manager.data_backup_cache.begin_local_backup() {
            return Ok(());
        }
        let backup_result =
            data::create_local_backup_if_due(&app_data_path, &paths, &mut manager.app_settings)
                .await;
        manager.data_backup_cache.finish_local_backup();
        backup_result?;
        manager.state["appSettings"] = serde_json::to_value(&manager.app_settings)?;
        Ok(())
    }

    pub async fn start_enabled_proxy_servers(&self) -> Result<(), ManagerError> {
        let manager = self.manager.lock().await;

        proxy::start_enabled_servers(
            &manager.proxy_server_registry,
            &manager.paths,
            &manager.state["cliTargets"],
        )
        .await
    }

    pub async fn state_snapshot(&self) -> Value {
        let manager = self.manager.lock().await;

        manager.state.clone()
    }

    pub async fn handle_tray_action(
        &self,
        app: AppHandle,
        item_id: &str,
    ) -> Result<Value, ManagerError> {
        let mut manager = self.manager.lock().await;

        manager.handle_tray_action(app, item_id).await
    }
}

impl ManagerState {
    pub fn new(app: &AppHandle) -> Result<Self, ManagerError> {
        let app_data_path = app
            .path()
            .app_data_dir()
            .map_err(|error| ManagerError::Path(error.to_string()))?;
        let settings_file_path = PathBuf::from(DEFAULT_USER_DATA_PATH).join("app-settings.json");
        let app_settings = load_app_settings(settings_file_path)?;
        let paths = resolve_app_paths(Path::new(&app_settings.data_path));
        let state = create_initial_state(&paths, &app_settings)?;
        let workspace_root = translation::workspace_root_from_current_dir()?;
        let resource_dir = app
            .path()
            .resource_dir()
            .map_err(|error| ManagerError::Path(error.to_string()))?;

        std::fs::create_dir_all(&app_data_path)?;

        Ok(Self {
            app_data_path,
            workspace_root,
            resource_dir,
            paths,
            app_settings,
            quick_switch_collapsed: false,
            data_backup_cache: data::DataBackupCache::new(),
            codex_login_cache: codex_account::CodexLoginCache::new(),
            proxy_server_registry: proxy::ProxyServerRegistry::new(),
            state,
        })
    }

    pub fn app_settings(&self) -> &AppSettings {
        &self.app_settings
    }

    pub fn quick_switch_collapsed(&self) -> bool {
        self.quick_switch_collapsed
    }

    pub async fn dispatch(
        &mut self,
        app: AppHandle,
        channel: &str,
        payload: Option<Value>,
    ) -> Result<Value, ManagerError> {
        match channel {
            "app:bootstrap"
            | "app:refresh"
            | "app:ensure-sessions-ready"
            | "app:ensure-tools-ready" => {
                self.refresh_state().await?;
                Ok(self.state.clone())
            }
            "app:ensure-skills-ready" => {
                self.refresh_state().await?;
                skills::refresh_skills_state(&self.paths, &mut self.state).await?;
                Ok(self.state.clone())
            }
            "settings:save" => {
                settings::save_settings(
                    &mut self.app_settings,
                    &mut self.paths,
                    &mut self.state,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                app::apply_auto_launch_setting(&app, &self.app_settings)?;
                let app_data_path = self.app_data_path.clone();
                let paths = self.paths.clone();
                if self.data_backup_cache.begin_local_backup() {
                    let backup_result = data::create_local_backup_if_due(
                        &app_data_path,
                        &paths,
                        &mut self.app_settings,
                    )
                    .await;
                    self.data_backup_cache.finish_local_backup();
                    backup_result?;
                }
                self.refresh_state().await?;
                app::sync_quick_switch_window(
                    &app,
                    &self.app_settings,
                    self.quick_switch_collapsed,
                )?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "app:update-status" => app::update_status().await,
            "app:check-updates" => app::check_updates(&app).await,
            "app:update-download" => app::download_update().await,
            "app:update-install" => {
                app::install_update(&app, payload.unwrap_or_else(|| json!({}))).await
            }
            "app:update-dismiss" => app::dismiss_update().await,
            "app:uninstall-without-trace" => {
                app::uninstall_without_trace(&app, &self.app_settings).await
            }
            "app:close-action" => {
                crate::reset_close_dialog_open();
                let payload = payload.unwrap_or_else(|| json!({}));
                let should_emit = payload
                    .get("remember")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    && payload
                        .get("action")
                        .and_then(Value::as_str)
                        .unwrap_or("cancel")
                        != "cancel";
                let result = app::handle_close_action(
                    &app,
                    &mut self.app_settings,
                    &mut self.state,
                    payload,
                    self.quick_switch_collapsed,
                )
                .await?;
                if should_emit {
                    app.emit("state:changed", self.state.clone())
                        .map_err(|error| ManagerError::Path(error.to_string()))?;
                }
                Ok(result)
            }
            "quick-switch:show-main" => {
                app::show_main_panel(&app, &self.app_settings, self.quick_switch_collapsed)
            }
            "quick-switch:set-collapsed" => {
                let payload = payload.unwrap_or_else(|| json!({}));
                self.quick_switch_collapsed = payload
                    .get("collapsed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                app::set_quick_switch_collapsed(&app, payload)
            }
            "quick-switch:move-by" => {
                app::move_quick_switch_by(&app, payload.unwrap_or_else(|| json!({})))
            }
            "data:export" => {
                data::export_data_backup(&app, &self.paths, &self.app_settings).await
            }
            "data:preview-restore" => {
                data::preview_data_backup_restore(&app, &self.paths, &mut self.data_backup_cache)
                    .await
            }
            "data:restore" => {
                data::restore_data_backup(
                    &self.paths,
                    &self.app_settings,
                    &mut self.state,
                    &mut self.data_backup_cache,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "data:local-backups" => data::list_local_backups(&self.app_data_path).await,
            "data:local-backup-now" => {
                data::create_local_backup_now(
                    &self.app_data_path,
                    &self.paths,
                    &mut self.app_settings,
                    &mut self.state,
                    &mut self.data_backup_cache,
                )
                .await
            }
            "data:local-backup-preview" => {
                data::preview_local_backup_restore(
                    &self.app_data_path,
                    &self.paths,
                    &mut self.data_backup_cache,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "data:local-backup-restore" => {
                data::restore_local_backup(
                    &self.app_data_path,
                    &self.paths,
                    &self.app_settings,
                    &mut self.state,
                    &mut self.data_backup_cache,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "data:cloud-push" => {
                data::push_cloud_backup(
                    &self.paths,
                    &mut self.app_settings,
                    &mut self.state,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "data:cloud-preview" => {
                data::preview_cloud_backup_restore(
                    &self.paths,
                    &mut self.data_backup_cache,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "data:cloud-inspect" => data::inspect_cloud_backup(payload.unwrap_or_else(|| json!({}))).await,
            "data:cloud-pull" => {
                data::pull_cloud_backup(
                    &self.paths,
                    &mut self.app_settings,
                    &mut self.state,
                    &mut self.data_backup_cache,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "app-log:list" => app_logs::list_logs(&self.app_data_path).await,
            "app-log:clear" => app_logs::clear_logs(&self.app_data_path).await,
            "repo:add" => {
                repos::add_repo(&self.paths, payload.unwrap_or_else(|| json!({}))).await?;
                self.refresh_state().await?;
                Ok(self.state.clone())
            }
            "repo:sync" => {
                repos::sync_repo(&self.paths, payload.unwrap_or_else(|| json!({}))).await?;
                self.refresh_state().await?;
                Ok(self.state.clone())
            }
            "repo:sync-all" => {
                repos::sync_all_repos(&self.paths).await?;
                self.refresh_state().await?;
                Ok(self.state.clone())
            }
            "repo:remove" => {
                repos::remove_repo(&self.paths, payload.unwrap_or_else(|| json!({}))).await?;
                self.refresh_state().await?;
                Ok(self.state.clone())
            }
            "git-tool:branches" => {
                git_tool::scan_branches(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:commits" => {
                git_tool::list_commits(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:commit-detail" => {
                git_tool::get_commit_detail(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:update-check-branch" => {
                git_tool::update_check_branch(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:clear-check-cache" => {
                git_tool::clear_commit_check_cache(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:check-commit" => {
                git_tool::check_commit_on_branch_api(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:archive-branch" => {
                git_tool::archive_branch(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:archives" => {
                git_tool::list_archives(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:archive-commits" => {
                git_tool::list_archive_commits(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:archive-commit-detail" => {
                git_tool::get_archive_commit_detail(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:restore-archive" => {
                git_tool::restore_archive(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:delete-archive" => {
                git_tool::delete_archive(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:stashes" => {
                git_tool::list_stashes(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:stash-archives" => {
                git_tool::list_stash_archives(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:stash-detail" => {
                git_tool::get_stash_detail(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:stash-archive-detail" => {
                git_tool::get_stash_archive_detail(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:archive-stash" => {
                git_tool::archive_stash(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:restore-stash-archive" => {
                git_tool::restore_stash_archive(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "git-tool:delete-stash-archive" => {
                git_tool::delete_stash_archive(
                    &self.paths,
                    &self.state["repos"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "codex-account:login" => {
                let result = codex_account::start_login(
                    &app,
                    &self.paths,
                    &self.codex_login_cache,
                    &self.state["cliTargets"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.state["codexLoginState"] =
                    codex_account::state_patch(&self.paths, &self.codex_login_cache).await?
                        ["codexLoginState"]
                        .clone();
                self.emit_state_changed(&app)?;
                Ok(result)
            }
            "codex-account:cancel" => {
                let patch = codex_account::cancel_login(&self.paths, &self.codex_login_cache).await?;
                self.refresh_state().await?;
                self.state["codexLoginState"] = patch["codexLoginState"].clone();
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:import-auth-json" => {
                let patch = codex_account::import_auth_json(
                    &self.paths,
                    &self.codex_login_cache,
                    &self.state["cliTargets"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.state["codexLoginState"] = patch["codexLoginState"].clone();
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:enable" => {
                if self.state["codexProxyState"]
                    .get("enabled")
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    return Err(ManagerError::System("请先关闭 Codex 代理接管".to_string()));
                }
                codex_account::enable_account(
                    &self.paths,
                    &self.state["cliTargets"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                runtime_provider::clear_runtime(
                    &self.paths,
                    json!({ "cli": "codex" }),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:clear" => {
                codex_account::clear_account(&self.paths).await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:refresh" => {
                codex_account::refresh_account(
                    &self.paths,
                    &self.state["cliTargets"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:disable" => {
                codex_account::disable_account(&self.paths, payload.unwrap_or_else(|| json!({})))
                    .await?;
                runtime_provider::refresh_drift(&self.paths, &self.state["cliTargets"]).await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:restore" => {
                codex_account::restore_account(&self.paths, payload.unwrap_or_else(|| json!({})))
                    .await?;
                runtime_provider::refresh_drift(&self.paths, &self.state["cliTargets"]).await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:update-proxy" => {
                codex_account::update_account_proxy(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-account:detail" => {
                codex_account::account_detail(&self.paths, payload.unwrap_or_else(|| json!({}))).await
            }
            "codex-account:delete" => {
                codex_account::delete_account(
                    &self.paths,
                    &self.state["cliTargets"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "claude-proxy:add-provider" => {
                proxy::add_provider(
                    &self.paths,
                    &self.state["cliTargets"],
                    "claude",
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-proxy:add-provider" => {
                proxy::add_provider(
                    &self.paths,
                    &self.state["cliTargets"],
                    "codex",
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "claude-proxy:remove-provider" => {
                proxy::remove_provider(&self.paths, "claude", payload.unwrap_or_else(|| json!({})))
                    .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-proxy:remove-provider" => {
                proxy::remove_provider(&self.paths, "codex", payload.unwrap_or_else(|| json!({})))
                    .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "claude-proxy:activate-provider" => {
                proxy::activate_provider(
                    &self.paths,
                    &self.state["cliTargets"],
                    "claude",
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-proxy:activate-provider" => {
                proxy::activate_provider(
                    &self.paths,
                    &self.state["cliTargets"],
                    "codex",
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "claude-proxy:enable" => {
                let payload = self.build_proxy_enable_payload("claude", payload);
                proxy::enable_proxy(
                    &self.proxy_server_registry,
                    &self.paths,
                    &self.state["cliTargets"],
                    "claude",
                    payload,
                )
                .await?;
                runtime_provider::clear_runtime(
                    &self.paths,
                    json!({ "cli": "claude" }),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-proxy:enable" => {
                let payload = self.build_proxy_enable_payload("codex", payload);
                proxy::enable_proxy(
                    &self.proxy_server_registry,
                    &self.paths,
                    &self.state["cliTargets"],
                    "codex",
                    payload,
                )
                .await?;
                runtime_provider::clear_runtime(
                    &self.paths,
                    json!({ "cli": "codex" }),
                    &self.state["cliTargets"],
                )
                .await?;
                codex_account::clear_account(&self.paths).await?;
                runtime_provider::refresh_drift(&self.paths, &self.state["cliTargets"]).await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "claude-proxy:disable" => {
                let result = proxy::disable_proxy(
                    &self.proxy_server_registry,
                    &self.paths,
                    &self.state["cliTargets"],
                    "claude",
                )
                .await?;
                self.restore_after_proxy_disabled("claude", result).await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-proxy:disable" => {
                let result = proxy::disable_proxy(
                    &self.proxy_server_registry,
                    &self.paths,
                    &self.state["cliTargets"],
                    "codex",
                )
                .await?;
                self.restore_after_proxy_disabled("codex", result).await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex-proxy:save-account-model" => {
                proxy::update_account_model(
                    &self.paths,
                    &self.state["cliTargets"],
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(self.state.clone())
            }
            "codex:launch-provider-instance" => {
                let result = runtime_provider::launch_codex_provider_instance(
                    &self.paths,
                    &self.state["cliTargets"],
                    &self.proxy_server_registry,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                self.emit_state_changed(&app)?;
                Ok(result)
            }
            "provider:save" => {
                runtime_provider::save_provider(&self.paths, payload.unwrap_or_else(|| json!({})))
                    .await?;
                self.refresh_state().await?;
                Ok(self.state.clone())
            }
            "provider:delete" => {
                runtime_provider::delete_provider(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                Ok(self.state.clone())
            }
            "runtime-model:save" => {
                runtime_provider::save_runtime_model(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "runtime:switch" => {
                runtime_provider::switch_runtime(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "runtime:clear" => {
                runtime_provider::clear_runtime(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "runtime:compare" => {
                runtime_provider::compare_runtime(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await
            }
            "runtime:config" => {
                runtime_provider::get_runtime_config(
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await
            }
            "runtime:resolve-drift" => {
                runtime_provider::resolve_runtime_drift(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "runtime:env" => runtime_provider::build_runtime_env(
                &self.paths,
                payload.unwrap_or_else(|| json!({})),
            ),
            "rule:save" => {
                rules::save_rule(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "rule:delete" => {
                rules::delete_rule(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "rule:toggle" => {
                rules::toggle_rule(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "rule:enable" => {
                rules::enable_rule(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "rule:move" => {
                rules::move_rule(&self.paths).await?;
                Ok(self.state.clone())
            }
            "rule:import-global" => {
                rules::import_global_rule(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "rule:preview-import-global" => {
                rules::preview_import_global_rule(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await
            }
            "rule:resolve-import-conflict" => {
                rules::resolve_import_conflict(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "rule:compare" => {
                rules::compare_rule(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await
            }
            "rule:resolve-drift" => {
                rules::resolve_rule_drift(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state["cliTargets"],
                )
                .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "session:search" => {
                sessions::search_sessions(&self.paths, payload.unwrap_or_else(|| json!({}))).await
            }
            "session:messages" => {
                sessions::load_session_messages(&self.paths, payload.unwrap_or_else(|| json!({})))
                    .await
            }
            "session:delete" => {
                sessions::delete_session(&self.paths, payload.unwrap_or_else(|| json!({}))).await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "session:recycle-list" => sessions::list_recycled_sessions(&self.paths).await,
            "session:restore" => {
                sessions::restore_session(&self.paths, payload.unwrap_or_else(|| json!({})))
                    .await?;
                self.refresh_state().await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "session:purge" => {
                sessions::purge_session(&self.paths, payload.unwrap_or_else(|| json!({}))).await?;
                Ok(json!(true))
            }
            "skill:create" => {
                skills::create_skill(
                    &self.paths,
                    &mut self.state,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "skill:preview-import-from-cli" => {
                skills::preview_skills_from_cli(
                    &self.paths,
                    &self.state,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            "skill:import-from-cli" => {
                skills::import_skills_from_cli(
                    &self.paths,
                    &mut self.state,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "skill:import-from-zip" => {
                skills::import_skill_from_zip(
                    &self.paths,
                    &mut self.state,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "skill:install" => {
                skills::install_skill(&self.state, payload.unwrap_or_else(|| json!({}))).await?;
                skills::refresh_skills_state(&self.paths, &mut self.state).await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "skill:uninstall" => {
                skills::uninstall_skill(&self.state, payload.unwrap_or_else(|| json!({}))).await?;
                skills::refresh_skills_state(&self.paths, &mut self.state).await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "skill:repair" => {
                skills::repair_skill(&self.state, payload.unwrap_or_else(|| json!({}))).await?;
                skills::refresh_skills_state(&self.paths, &mut self.state).await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "skill:files" => {
                skills::get_skill_files(&self.state, payload.unwrap_or_else(|| json!({})))
            }
            "skill-repository:add" => {
                let result =
                    skills::add_skill_repository(&self.paths, payload.unwrap_or_else(|| json!({})))
                        .await?;
                self.state["skillRepositories"] = result
                    .get("skillRepositories")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                self.state["refreshedAt"] = result
                    .get("refreshedAt")
                    .cloned()
                    .unwrap_or_else(|| json!(0));
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(result)
            }
            "skill-repository:refresh" => {
                let result = skills::refresh_skill_repository(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.state["skillRepositories"] = result
                    .get("skillRepositories")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                self.state["refreshedAt"] = result
                    .get("refreshedAt")
                    .cloned()
                    .unwrap_or_else(|| json!(0));
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(result)
            }
            "skill-repository:remove" => {
                let result = skills::remove_skill_repository(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                self.state["skillRepositories"] = result
                    .get("skillRepositories")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                self.state["refreshedAt"] = result
                    .get("refreshedAt")
                    .cloned()
                    .unwrap_or_else(|| json!(0));
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(result)
            }
            "skill-repository:install-skill" => {
                skills::install_skill_from_repository(
                    &self.paths,
                    &mut self.state,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await?;
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(self.state.clone())
            }
            "usage:stats" => {
                usage::get_stats(&self.paths, payload.unwrap_or_else(|| json!({}))).await
            }
            "skill-usage:stats" => {
                usage::get_skill_usage_stats(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state,
                )
                .await
            }
            "usage:sync" => {
                let result = usage::sync_usage(
                    &self.paths,
                    payload.unwrap_or_else(|| json!({})),
                    &self.state,
                )
                .await?;
                self.refresh_state().await?;
                self.state["usage"] = result.get("data").cloned().unwrap_or_else(|| json!({}));

                let mut diagnostics = self
                    .state
                    .get("diagnostics")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|item| {
                        item.get("type").and_then(Value::as_str) != Some("usage-parse-error")
                    })
                    .collect::<Vec<_>>();
                diagnostics.extend(
                    result
                        .get("diagnostics")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                );
                self.state["diagnostics"] = json!(diagnostics);
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(result)
            }
            "usage:export-image" => {
                usage::export_report_image(payload.unwrap_or_else(|| json!({}))).await
            }
            "usage:pricing" => usage::get_pricing(&self.paths).await,
            "usage:save-pricing" => {
                let result =
                    usage::save_pricing(&self.paths, payload.unwrap_or_else(|| json!({}))).await?;
                let stats = usage::get_stats(&self.paths, json!({})).await?;

                self.state["usage"] = stats.get("data").cloned().unwrap_or_else(|| json!({}));
                app.emit("state:changed", self.state.clone())
                    .map_err(|error| ManagerError::Path(error.to_string()))?;
                Ok(result)
            }
            "system:select-directory" => system::select_directory(&app, payload),
            "system:select-file" => system::select_file(&app, payload),
            "system:open-path" => system::open_path(&app, payload),
            "system:open-external" => system::open_external(&app, payload),
            "translation:translate" => {
                translation::translate_text(
                    &self.app_data_path,
                    &self.workspace_root,
                    &self.resource_dir,
                    payload.unwrap_or_else(|| json!({})),
                )
                .await
            }
            _ => Err(ManagerError::UnknownChannel(channel.to_string())),
        }
    }

    async fn refresh_state(&mut self) -> Result<(), ManagerError> {
        ensure_app_directories(&self.paths).await?;
        self.state = create_initial_state(&self.paths, &self.app_settings)?;
        runtime_provider::refresh_drift(&self.paths, &self.state["cliTargets"]).await?;
        self.state = create_initial_state(&self.paths, &self.app_settings)?;
        Ok(())
    }

    fn emit_state_changed(&self, app: &AppHandle) -> Result<(), ManagerError> {
        app.emit("state:changed", self.state.clone())
            .map_err(|error| ManagerError::Path(error.to_string()))
    }

    async fn handle_tray_action(
        &mut self,
        app: AppHandle,
        item_id: &str,
    ) -> Result<Value, ManagerError> {
        let parts = item_id.split(':').collect::<Vec<_>>();

        match parts.as_slice() {
            ["tray", "runtime", "clear", cli] => {
                runtime_provider::clear_runtime(
                    &self.paths,
                    json!({ "cli": *cli }),
                    &self.state["cliTargets"],
                )
                .await?;
            }
            ["tray", "runtime", "switch", cli, provider_id] => {
                if *cli == "codex" {
                    self.disable_codex_proxy_for_tray().await?;
                }
                if *cli == "codex" {
                    codex_account::clear_account(&self.paths).await?;
                }
                let model = self.tray_runtime_model(provider_id, cli);
                runtime_provider::switch_runtime(
                    &self.paths,
                    json!({
                      "cli": *cli,
                      "providerId": *provider_id,
                      "model": model
                    }),
                    &self.state["cliTargets"],
                )
                .await?;
            }
            ["tray", "codex", "account", "enable", account_id] => {
                self.disable_codex_proxy_for_tray().await?;
                runtime_provider::clear_runtime(
                    &self.paths,
                    json!({ "cli": "codex" }),
                    &self.state["cliTargets"],
                )
                .await?;
                codex_account::enable_account(
                    &self.paths,
                    &self.state["cliTargets"],
                    json!({ "accountId": *account_id }),
                )
                .await?;
            }
            ["tray", "codex", "account", "clear"] => {
                self.disable_codex_proxy_for_tray().await?;
                codex_account::clear_account(&self.paths).await?;
            }
            ["tray", "codex", "account", "refresh", account_id] => {
                codex_account::refresh_account(
                    &self.paths,
                    &self.state["cliTargets"],
                    json!({ "accountId": *account_id, "syncAuth": false }),
                )
                .await?;
            }
            ["tray", "codex", "proxy", "disable"] => {
                self.disable_codex_proxy_for_tray().await?;
            }
            _ => return Err(ManagerError::UnknownChannel(item_id.to_string())),
        }

        runtime_provider::refresh_drift(&self.paths, &self.state["cliTargets"]).await?;
        self.refresh_state().await?;
        self.emit_state_changed(&app)?;
        Ok(self.state.clone())
    }

    fn tray_runtime_model(&self, provider_id: &str, cli: &str) -> String {
        let provider_model = self
            .state
            .get("providers")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("id").and_then(Value::as_str) == Some(provider_id))
            })
            .and_then(|provider| provider.get("runtimeConfig"))
            .and_then(|runtime_config| runtime_config.get("mainModel"))
            .and_then(Value::as_str)
            .unwrap_or("");

        if !provider_model.is_empty() {
            return provider_model.to_string();
        }

        let profile_model = self
            .state
            .get("runtimeProfiles")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("cli").and_then(Value::as_str) == Some(cli))
            })
            .and_then(|profile| profile.get("model"))
            .and_then(Value::as_str)
            .unwrap_or("");

        if !profile_model.is_empty() {
            return profile_model.to_string();
        }

        self.state
            .get("runtimeModels")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("providerId").and_then(Value::as_str) == Some(provider_id))
            })
            .and_then(|model| model.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    async fn disable_codex_proxy_for_tray(&self) -> Result<(), ManagerError> {
        if self
            .state
            .get("codexProxyState")
            .and_then(|state| state.get("enabled"))
            .and_then(Value::as_bool)
            != Some(true)
        {
            return Ok(());
        }

        proxy::disable_proxy(
            &self.proxy_server_registry,
            &self.paths,
            &self.state["cliTargets"],
            "codex",
        )
        .await?;
        Ok(())
    }

    fn build_proxy_enable_payload(&self, cli: &str, payload: Option<Value>) -> Value {
        let mut payload = payload.unwrap_or_else(|| json!({}));
        let previous_profile = self
            .state
            .get("runtimeProfiles")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("cli").and_then(Value::as_str) == Some(cli))
                    .cloned()
            })
            .unwrap_or(Value::Null);
        let previous_account_id = if cli == "codex" {
            self.state
                .get("codexAccounts")
                .and_then(Value::as_array)
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item.get("active").and_then(Value::as_bool) == Some(true))
                        .and_then(|item| item.get("id"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };

        payload["previousProfile"] = previous_profile;
        payload["previousAccountId"] = json!(previous_account_id);
        payload
    }

    async fn restore_after_proxy_disabled(
        &self,
        cli: &str,
        result: Value,
    ) -> Result<(), ManagerError> {
        let previous_account_id = result
            .get("previousAccountId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let previous_profile = result.get("previousProfile").cloned().unwrap_or(Value::Null);

        if cli == "codex" && !previous_account_id.is_empty() {
            codex_account::enable_account(
                &self.paths,
                &self.state["cliTargets"],
                json!({ "accountId": previous_account_id }),
            )
            .await?;
            runtime_provider::clear_runtime(
                &self.paths,
                json!({ "cli": "codex" }),
                &self.state["cliTargets"],
            )
            .await?;
        } else if !previous_profile.is_null() {
            runtime_provider::switch_runtime(
                &self.paths,
                previous_profile,
                &self.state["cliTargets"],
            )
            .await?;
        } else {
            runtime_provider::clear_runtime(
                &self.paths,
                json!({ "cli": cli }),
                &self.state["cliTargets"],
            )
            .await?;
        }

        runtime_provider::refresh_drift(&self.paths, &self.state["cliTargets"]).await
    }
}

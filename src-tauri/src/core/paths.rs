use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn default_user_data_path() -> PathBuf {
    static DEFAULT_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DEFAULT_PATH
        .get_or_init(|| {
            select_writable_data_path(&user_data_candidates())
                .unwrap_or_else(|| home_path().join("ai-manager-data"))
        })
        .clone()
}

pub fn app_settings_path() -> PathBuf {
    // 优先找回已有设置；新增磁盘不能改变用户已经配置的数据位置。
    user_data_candidates()
        .iter()
        .map(|root| root.join("app-settings.json"))
        .find(|path| path.is_file())
        .unwrap_or_else(|| default_user_data_path().join("app-settings.json"))
}

fn user_data_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        const DRIVE_FIXED: u32 = 3;
        // D 盘优先，其余数据盘按盘符排序，系统盘 C 放在最后。
        for letter in windows_data_drive_letters() {
            let root = format!("{}:\\", letter as char);
            let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            if unsafe { GetDriveTypeW(wide.as_ptr()) } == DRIVE_FIXED {
                candidates.push(PathBuf::from(root).join("ai-manager-data"));
            }
        }
    }
    candidates.push(home_path().join("ai-manager-data"));
    candidates
}

#[cfg(any(windows, test))]
fn windows_data_drive_letters() -> impl Iterator<Item = u8> {
    std::iter::once(b'D').chain(b'E'..=b'Z').chain(b'A'..=b'C')
}

fn select_writable_data_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find_map(|path| {
        if std::fs::create_dir_all(path).is_err() {
            return None;
        }
        let probe = path.join(format!(".write-check-{}", uuid::Uuid::new_v4()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
            .ok()?;
        drop(file);
        std::fs::remove_file(probe).ok()?;
        Some(path.clone())
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub user_data_path: String,
    pub workspace_root: String,
    pub skills_dir: String,
    pub prompts_dir: String,
    pub prompt_profiles_dir: String,
    pub repos_dir: String,
    pub sessions_dir: String,
    pub session_recycle_dir: String,
    pub session_recycle_sessions_dir: String,
    pub session_recycle_metadata_dir: String,
    pub pets_dir: String,
    pub disabled_pets_dir: String,
    pub logs_dir: String,
    pub temp_dir: String,
    pub storage_dir: String,
    pub lan_share_dir: String,
    pub storage_files: StorageFiles,
    pub lan_share_files: LanShareFiles,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageFiles {
    pub database: String,
    pub usage_database: String,
    pub skill_repositories: String,
    pub skill_groups: String,
    pub skill_repository_cache: String,
    pub repos: String,
    pub skills: String,
    pub installs: String,
    pub cli_targets: String,
    pub sessions: String,
    pub usage_logs: String,
    pub usage_request_records: String,
    pub usage_pricing: String,
    pub codex_provider_instances: String,
    pub providers: String,
    pub rules: String,
    pub prompt_runtime_state: String,
    pub runtime_models: String,
    pub runtime_profiles: String,
    pub runtime_provider_state: String,
    pub runtime_provider_keys: String,
    pub claude_proxy_config: String,
    pub claude_proxy_live_backup: String,
    pub claude_proxy_request_logs: String,
    pub claude_desktop_request_logs: String,
    pub codex_proxy_config: String,
    pub codex_proxy_live_backup: String,
    pub codex_proxy_request_logs: String,
    pub codex_accounts: String,
    pub codex_active_account_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanShareFiles {
    pub config: String,
    pub files: String,
    pub devices: String,
    pub sessions: String,
    pub groups: String,
    pub messages: String,
    pub downloads: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicPaths {
    pub workspace_root: String,
    pub skills_dir: String,
    pub prompts_dir: String,
    pub prompt_profiles_dir: String,
    pub repos_dir: String,
    pub sessions_dir: String,
    pub session_recycle_dir: String,
    pub storage_dir: String,
}

pub fn resolve_app_paths(user_data_path: &Path) -> AppPaths {
    let workspace_root = user_data_path.join("workspace");
    let storage_dir = workspace_root.join("storage");
    let logs_dir = workspace_root.join("logs");
    let temp_dir = workspace_root.join("temp");
    let sessions_dir = workspace_root.join("sessions");
    let session_recycle_dir = sessions_dir.join("recycle");
    let prompts_dir = workspace_root.join("prompts");
    let profiles_dir = workspace_root.join("profiles");
    let pets_dir = workspace_root.join("pets");
    let disabled_pets_dir = workspace_root.join("pets-disabled");
    let lan_share_dir = workspace_root.join("lan-share");

    AppPaths {
        user_data_path: path_text(user_data_path),
        workspace_root: path_text(&workspace_root),
        skills_dir: path_text(workspace_root.join("skills")),
        prompts_dir: path_text(&prompts_dir),
        prompt_profiles_dir: path_text(&profiles_dir),
        repos_dir: path_text(workspace_root.join("repos")),
        sessions_dir: path_text(&sessions_dir),
        session_recycle_dir: path_text(&session_recycle_dir),
        session_recycle_sessions_dir: path_text(session_recycle_dir.join("sessions")),
        session_recycle_metadata_dir: path_text(session_recycle_dir.join("metadata")),
        pets_dir: path_text(&pets_dir),
        disabled_pets_dir: path_text(&disabled_pets_dir),
        logs_dir: path_text(&logs_dir),
        temp_dir: path_text(&temp_dir),
        storage_dir: path_text(&storage_dir),
        lan_share_dir: path_text(&lan_share_dir),
        storage_files: StorageFiles {
            database: path_text(storage_dir.join("ai-manager.db")),
            usage_database: path_text(logs_dir.join("usage.db")),
            skill_repositories: path_text(storage_dir.join("skill-repositories.json")),
            skill_groups: path_text(storage_dir.join("skill-groups.json")),
            skill_repository_cache: path_text(temp_dir.join("skill-repositories-cache.json")),
            repos: path_text(storage_dir.join("repos.json")),
            skills: path_text(storage_dir.join("skills.json")),
            installs: path_text(storage_dir.join("installs.json")),
            cli_targets: path_text(storage_dir.join("cli-targets.json")),
            sessions: path_text(temp_dir.join("sessions.json")),
            usage_logs: path_text(logs_dir.join("usage-logs.json")),
            usage_request_records: path_text(logs_dir.join("usage-request-records.json")),
            usage_pricing: path_text(storage_dir.join("usage-pricing.json")),
            codex_provider_instances: path_text(storage_dir.join("codex-provider-instances.json")),
            providers: path_text(storage_dir.join("providers.json")),
            rules: path_text(storage_dir.join("rules.json")),
            prompt_runtime_state: path_text(storage_dir.join("prompt-runtime-state.json")),
            runtime_models: path_text(storage_dir.join("runtime-models.json")),
            runtime_profiles: path_text(storage_dir.join("runtime-profiles.json")),
            runtime_provider_state: path_text(storage_dir.join("runtime-provider-state.json")),
            runtime_provider_keys: path_text(storage_dir.join("runtime-provider-keys.json")),
            claude_proxy_config: path_text(storage_dir.join("claude-proxy-config.json")),
            claude_proxy_live_backup: path_text(storage_dir.join("claude-proxy-live-backup.json")),
            claude_proxy_request_logs: path_text(
                storage_dir.join("claude-proxy-request-logs.json"),
            ),
            codex_proxy_config: path_text(storage_dir.join("codex-proxy-config.json")),
            claude_desktop_request_logs: path_text(
                storage_dir.join("claude-desktop-request-logs.json"),
            ),
            codex_proxy_live_backup: path_text(storage_dir.join("codex-proxy-live-backup.json")),
            codex_proxy_request_logs: path_text(storage_dir.join("codex-proxy-request-logs.json")),
            codex_accounts: path_text(storage_dir.join("codex-accounts.json")),
            codex_active_account_id: path_text(storage_dir.join("codex-active-account-id.json")),
        },
        lan_share_files: LanShareFiles {
            config: path_text(lan_share_dir.join("config.json")),
            files: path_text(lan_share_dir.join("files.json")),
            devices: path_text(lan_share_dir.join("devices.json")),
            sessions: path_text(lan_share_dir.join("sessions.json")),
            groups: path_text(lan_share_dir.join("groups.json")),
            messages: path_text(lan_share_dir.join("messages.json")),
            downloads: path_text(lan_share_dir.join("downloads.json")),
        },
    }
}

pub async fn ensure_app_directories(paths: &AppPaths) -> Result<(), std::io::Error> {
    for dir in [
        &paths.workspace_root,
        &paths.skills_dir,
        &paths.prompts_dir,
        &path_text(Path::new(&paths.prompts_dir).join("common")),
        &path_text(Path::new(&paths.prompts_dir).join("claude")),
        &path_text(Path::new(&paths.prompts_dir).join("codex")),
        &paths.prompt_profiles_dir,
        &paths.repos_dir,
        &paths.session_recycle_sessions_dir,
        &paths.session_recycle_metadata_dir,
        &paths.pets_dir,
        &paths.disabled_pets_dir,
        &paths.logs_dir,
        &paths.temp_dir,
        &paths.storage_dir,
        &paths.lan_share_dir,
    ] {
        tokio::fs::create_dir_all(dir).await?;
    }

    Ok(())
}

pub fn public_paths(paths: &AppPaths) -> PublicPaths {
    PublicPaths {
        workspace_root: paths.workspace_root.clone(),
        skills_dir: paths.skills_dir.clone(),
        prompts_dir: paths.prompts_dir.clone(),
        prompt_profiles_dir: paths.prompt_profiles_dir.clone(),
        repos_dir: paths.repos_dir.clone(),
        sessions_dir: paths.sessions_dir.clone(),
        session_recycle_dir: paths.session_recycle_dir.clone(),
        storage_dir: paths.storage_dir.clone(),
    }
}

pub fn portable_home_prefix() -> PathBuf {
    #[cfg(not(windows))]
    return PathBuf::from("~");

    #[cfg(windows)]
    {
    let home = home_path();

    home.parent()
        .map(|parent| parent.join("%USERNAME%"))
        .unwrap_or(home)
    }
}

pub fn home_path() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(""))
}

pub fn path_text(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::{resolve_app_paths, select_writable_data_path};
    use std::path::Path;

    #[test]
    fn cross_platform_data_directory_selection_uses_first_writable_candidate() {
        let root = std::env::temp_dir().join(format!("data-path-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let blocked = root.join("blocked");
        std::fs::write(&blocked, b"file").unwrap();
        let preferred = root.join("preferred");
        let other = root.join("other");
        assert_eq!(
            select_writable_data_path(&[blocked, preferred.clone(), other.clone()]),
            Some(preferred.clone())
        );
        assert!(!other.exists());
        assert_eq!(std::fs::read_dir(preferred).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cross_platform_windows_drive_priority_covers_every_fixed_drive() {
        let letters = super::windows_data_drive_letters().collect::<Vec<_>>();
        assert_eq!(letters[0], b'D');
        assert_eq!(letters[1], b'E');
        assert_eq!(letters.last(), Some(&b'C'));
        assert_eq!(letters.len(), 26);
        assert_eq!(letters.iter().collect::<std::collections::HashSet<_>>().len(), 26);
    }
    #[test]
    fn resolves_lan_share_paths_under_workspace() {
        let paths = resolve_app_paths(Path::new(r"D:\ai-manager-data"));

        assert_eq!(
            paths.lan_share_dir.replace('/', "\\"),
            r"D:\ai-manager-data\workspace\lan-share"
        );
        assert_eq!(
            paths.lan_share_files.files.replace('/', "\\"),
            r"D:\ai-manager-data\workspace\lan-share\files.json"
        );
        assert_eq!(
            paths.lan_share_files.messages.replace('/', "\\"),
            r"D:\ai-manager-data\workspace\lan-share\messages.json"
        );
        assert_eq!(
            paths.storage_files.usage_database.replace('/', "\\"),
            r"D:\ai-manager-data\workspace\logs\usage.db"
        );
        assert_eq!(
            paths.storage_files.database.replace('/', "\\"),
            r"D:\ai-manager-data\workspace\storage\ai-manager.db"
        );
        assert_eq!(
            paths.pets_dir.replace('/', "\\"),
            r"D:\ai-manager-data\workspace\pets"
        );
        assert_eq!(
            paths.disabled_pets_dir.replace('/', "\\"),
            r"D:\ai-manager-data\workspace\pets-disabled"
        );
    }
}

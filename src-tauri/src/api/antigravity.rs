use crate::core::{error::ManagerError, paths::AppPaths};
use serde_json::Value;

#[cfg(not(windows))]
pub(crate) fn installed_clients() -> Vec<Value> {
    Vec::new()
}

#[cfg(not(windows))]
pub(crate) async fn dispatch(_: &AppPaths, _: &str, _: &Value) -> Result<Value, ManagerError> {
    Err(ManagerError::System(
        "当前仅支持 Windows 上的 Antigravity".into(),
    ))
}

#[cfg(windows)]
pub(crate) use windows::{dispatch, installed_clients};

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::api::{claude_desktop, google_account};
    use crate::core::service_provider_store as store;
    use serde_json::json;
    use std::{
        env, fs, io,
        os::windows::{ffi::OsStrExt, process::CommandExt},
        path::{Path, PathBuf},
        process::Command,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        Security::Credentials::{
            CredWriteW, CREDENTIALW, CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE,
            CRED_TYPE_GENERIC,
        },
        Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
        },
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
    };
    use winreg::{
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
        RegKey,
    };

    const CLIENTS: [(&str, &str, &str); 2] = [
        ("antigravity", "Antigravity", "Antigravity.exe"),
        ("antigravity-ide", "Antigravity IDE", "Antigravity IDE.exe"),
    ];
    static SWITCH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn detect(name: &str, executable: &str) -> Option<PathBuf> {
        let valid = |path: &PathBuf| {
            path.is_absolute()
                && path
                    .file_name()
                    .is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(executable))
                && path.is_file()
        };
        for (hive, view) in [
            (HKEY_CURRENT_USER, 0),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
        ] {
            let root = RegKey::predef(hive);
            let key = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{executable}");
            if let Ok(path) = root
                .open_subkey_with_flags(key, KEY_READ | view)
                .and_then(|key| key.get_value::<String, _>(""))
            {
                let path = PathBuf::from(path.trim().trim_matches('"'));
                if valid(&path) {
                    return Some(path);
                }
            }
            let Ok(uninstall) = root.open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                KEY_READ | view,
            ) else {
                continue;
            };
            for key in uninstall.enum_keys().flatten() {
                let Ok(entry) = uninstall.open_subkey(key) else {
                    continue;
                };
                let label = entry
                    .get_value::<String, _>("DisplayName")
                    .unwrap_or_default();
                if label != name
                    && !label.strip_prefix(name).is_some_and(|suffix| {
                        suffix.starts_with(' ')
                            && suffix
                                .trim_start()
                                .starts_with(|c: char| c.is_ascii_digit())
                    })
                {
                    continue;
                }
                if let Ok(icon) = entry.get_value::<String, _>("DisplayIcon") {
                    let icon = icon.trim();
                    let icon = icon
                        .rsplit_once(',')
                        .filter(|(_, index)| index.trim().parse::<i32>().is_ok())
                        .map_or(icon, |(path, _)| path);
                    let path = PathBuf::from(icon.trim().trim_matches('"'));
                    if valid(&path) {
                        return Some(path);
                    }
                }
                if let Ok(location) = entry.get_value::<String, _>("InstallLocation") {
                    let path = Path::new(location.trim().trim_matches('"')).join(executable);
                    if valid(&path) {
                        return Some(path);
                    }
                }
            }
        }
        env::var_os("LOCALAPPDATA")
            .map(|root| PathBuf::from(root).join("Programs"))
            .into_iter()
            .chain(
                ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                    .into_iter()
                    .filter_map(env::var_os)
                    .map(PathBuf::from),
            )
            .map(|root| root.join(name).join(executable))
            .chain(
                env::var_os("PATH")
                    .iter()
                    .flat_map(|path| env::split_paths(path))
                    .map(|dir| dir.join(executable)),
            )
            .find(valid)
    }

    fn version(path: &Path, ide: bool) -> Option<(u64, u64, u64)> {
        if ide {
            let product = path.parent()?.join("resources/app/product.json");
            let manifest: Value = serde_json::from_slice(&fs::read(product).ok()?).ok()?;
            let mut version = manifest["ideVersion"].as_str()?.split('.');
            return Some((
                version.next()?.parse().ok()?,
                version.next()?.parse().ok()?,
                version
                    .next()
                    .and_then(|part| part.parse().ok())
                    .unwrap_or(0),
            ));
        }
        let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // 版本信息来自可执行文件资源，不需要启动客户端。
        unsafe {
            let len = GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut());
            if len == 0 {
                return None;
            }
            let mut buffer = vec![0u8; len as usize];
            let mut value = std::ptr::null_mut();
            let mut size = 0;
            if GetFileVersionInfoW(path.as_ptr(), 0, len, buffer.as_mut_ptr().cast()) == 0
                || VerQueryValueW(
                    buffer.as_ptr().cast(),
                    [b'\\' as u16, 0].as_ptr(),
                    &mut value,
                    &mut size,
                ) == 0
                || value.is_null()
                || (size as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
            {
                return None;
            }
            let info = value.cast::<VS_FIXEDFILEINFO>().read_unaligned();
            Some((
                (info.dwProductVersionMS >> 16) as u64,
                (info.dwProductVersionMS & 0xffff) as u64,
                (info.dwProductVersionLS >> 16) as u64,
            ))
        }
    }

    fn uses_shared_login(version: Option<(u64, u64, u64)>, ide: bool) -> bool {
        version.is_some_and(|(major, minor, _)| (major, minor) >= if ide { (2, 6) } else { (2, 0) })
    }

    pub(crate) fn installed_clients() -> Vec<Value> {
        CLIENTS
            .into_iter()
            .filter_map(|(id, name, executable)| {
                let path = detect(name, executable)?;
                let ide = id == "antigravity-ide";
                let version = version(&path, ide);
                Some(json!({
                    "id": id, "label": name, "installed": true, "configPath": path,
                    "version": version.map(|(major, minor, patch)| format!("{major}.{minor}.{patch}")),
                    "supportsAccountSwitch": uses_shared_login(version, ide)
                }))
            })
            .collect()
    }

    fn clients_running() -> io::Result<bool> {
        // 桌面版与新版 IDE 可能共享登录，更新前检查两种客户端是否已退出。
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut has_entry = Process32FirstW(snapshot, &mut entry);
            while has_entry != 0 {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if CLIENTS
                    .iter()
                    .any(|(_, _, exe)| name.eq_ignore_ascii_case(exe))
                {
                    CloseHandle(snapshot);
                    return Ok(true);
                }
                has_entry = Process32NextW(snapshot, &mut entry);
            }
            let error = io::Error::last_os_error();
            CloseHandle(snapshot);
            if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                Ok(false)
            } else {
                Err(error)
            }
        }
    }

    fn credential_json(tokens: &Value) -> Result<Vec<u8>, ManagerError> {
        let access = store::text(tokens, "access_token");
        let refresh = store::text(tokens, "refresh_token");
        let expiry = tokens["expires_at"]
            .as_i64()
            .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
            .filter(|date| *date > chrono::Utc::now())
            .ok_or_else(|| store::fail("Google 登录凭据已过期，请重新授权"))?;
        if access.is_empty() || refresh.is_empty() {
            return Err(store::fail("Google 登录凭据不完整，请重新授权"));
        }
        let mut value = json!({
            "auth_method": "consumer",
            "token": {
                "access_token": access, "refresh_token": refresh, "token_type": "Bearer",
                "expiry": expiry.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            }
        });
        if !store::text(tokens, "id_token").is_empty() {
            value["id_token"] = tokens["id_token"].clone();
        }
        Ok(serde_json::to_vec(&value)?)
    }

    fn write_credentials(mut bytes: Vec<u8>) -> Result<(), ManagerError> {
        if bytes.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
            return Err(store::fail(
                "Google 登录凭据超过 Windows 凭据管理器的容量限制",
            ));
        }
        let home = env::var_os("USERPROFILE").ok_or_else(|| store::fail("无法确定用户目录"))?;
        let gemini = PathBuf::from(home).join(".gemini");
        let cli = gemini.join("antigravity-cli");
        let mut files = vec![gemini.join("jetski-standalone-oauth-token")];
        if cli.is_dir() {
            files.push(cli.join("antigravity-oauth-token"));
        }
        let snapshots = files
            .iter()
            .map(|path| match fs::read(path) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            })
            .collect::<io::Result<Vec<_>>>()?;

        let mut written = 0;
        let result = (|| {
            for path in &files {
                claude_desktop::atomic_write(path, &bytes)?;
                written += 1;
            }
            let mut target: Vec<u16> = "gemini:antigravity".encode_utf16().chain([0]).collect();
            let mut username: Vec<u16> = "antigravity".encode_utf16().chain([0]).collect();
            let credential = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: target.as_mut_ptr(),
                UserName: username.as_mut_ptr(),
                CredentialBlobSize: bytes.len() as u32,
                CredentialBlob: bytes.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                ..Default::default()
            };
            if unsafe { CredWriteW(&credential, 0) } == 0 {
                return Err(ManagerError::Io(io::Error::last_os_error()));
            }
            Ok(())
        })();
        if let Err(error) = result {
            let mut restored = true;
            for (path, original) in files.iter().zip(snapshots).take(written).rev() {
                let result = match original {
                    Some(bytes) => claude_desktop::atomic_write(path, &bytes),
                    None => fs::remove_file(path).map_err(ManagerError::Io),
                };
                restored &= result.is_ok();
            }
            if !restored {
                return Err(store::fail(format!(
                    "Antigravity 凭据写入失败，部分本地文件未能恢复。{error}"
                )));
            }
            return Err(store::fail(format!("Antigravity 凭据写入失败。{error}")));
        }
        Ok(())
    }

    pub(crate) async fn dispatch(
        paths: &AppPaths,
        action: &str,
        payload: &Value,
    ) -> Result<Value, ManagerError> {
        if !store::active(paths)? {
            return Err(store::fail("请先启用服务商配置中心"));
        }
        let _guard = SWITCH_LOCK
            .try_lock()
            .map_err(|_| store::fail("Antigravity 正在应用账号，请稍后重试"))?;
        let id = store::text(payload, "clientId");
        let (_, name, exe) = CLIENTS
            .into_iter()
            .find(|(client, _, _)| *client == id)
            .ok_or_else(|| store::fail("请选择有效的 Antigravity 客户端"))?;
        let path = detect(name, exe)
            .ok_or_else(|| store::fail(format!("未检测到 {name}，请安装后刷新列表")))?;
        let apply = action == "antigravity-apply";
        if apply {
            let ide = id == "antigravity-ide";
            if !uses_shared_login(version(&path, ide), ide) {
                return Err(store::fail("此版本请在客户端内登录 Google 账号"));
            }
            if clients_running()? {
                return Err(store::fail(
                    "请先退出 Antigravity 和 Antigravity IDE，再应用 Google 账号",
                ));
            }
            let tokens =
                google_account::antigravity_tokens(paths, store::text(payload, "providerId"))
                    .await?;
            if clients_running()? {
                return Err(store::fail("Antigravity 已打开，请退出后重试"));
            }
            if !path.is_file() {
                return Err(store::fail(format!("{name} 安装路径已失效，请刷新列表")));
            }
            write_credentials(credential_json(&tokens)?)?;
        }
        let mut command = Command::new(&path);
        if let Some(dir) = path.parent() {
            command.current_dir(dir);
        }
        command.creation_flags(0x08000000);
        command.spawn().map_err(|error| {
            let prefix = if apply { "凭据已写入，" } else { "" };
            store::fail(format!("{prefix}{name} 启动失败。{error}"))
        })?;
        Ok(json!({"message": if apply {
            format!("Google 凭据已写入，已启动 {name}")
        } else {
            format!("已启动 {name}")
        }}))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{api::service_provider, core::paths::resolve_app_paths};

        #[test]
        fn detected_clients_point_to_existing_executables() {
            let clients = installed_clients();
            for client in clients {
                let id = store::text(&client, "id");
                let (_, _, executable) = CLIENTS.iter().find(|(item, _, _)| *item == id).unwrap();
                let path = Path::new(store::text(&client, "configPath"));
                assert!(path.is_file());
                assert!(path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(executable));
                println!("{id}: {}", store::text(&client, "version"));
            }
        }

        #[test]
        fn google_account_gate_rejects_other_sources_and_missing_credentials() {
            let root = env::temp_dir().join(format!(
                "ai-manager-antigravity-test-{}",
                uuid::Uuid::new_v4()
            ));
            let paths = resolve_app_paths(&root);
            store::initialize(&paths).unwrap();
            service_provider::switch_backend(&paths, "service").unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            for (kind, expected) in [
                ("api", "仅支持 Google 官方 OAuth 账号"),
                ("codex_account", "仅支持 Google 官方 OAuth 账号"),
                ("google_account", "Google 授权信息不存在"),
            ] {
                let provider = store::save(
                    &paths,
                    &json!({
                        "name": kind, "kind": kind, "baseUrl": "http://127.0.0.1:1/v1"
                    }),
                )
                .unwrap();
                let error = runtime
                    .block_on(google_account::antigravity_tokens(
                        &paths,
                        store::text(&provider, "id"),
                    ))
                    .unwrap_err();
                assert!(error.to_string().contains(expected), "{error}");
            }
            assert_eq!(root.parent(), Some(env::temp_dir().as_path()));
            fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn shared_login_uses_distinct_version_thresholds() {
            assert!(uses_shared_login(Some((2, 0, 0)), false));
            assert!(!uses_shared_login(Some((1, 99, 9)), false));
            assert!(!uses_shared_login(Some((2, 5, 99)), true));
            assert!(uses_shared_login(Some((2, 6, 0)), true));
            assert!(!uses_shared_login(None, false));
        }

        #[test]
        fn credentials_require_unexpired_google_tokens() {
            let expiry = chrono::Utc::now().timestamp() + 3600;
            assert!(credential_json(&json!({"access_token":"key","expires_at":expiry})).is_err());
            assert!(credential_json(
                &json!({"access_token":"a","refresh_token":"r","expires_at":1})
            )
            .is_err());
            let bytes = credential_json(&json!({
                "access_token":"a","refresh_token":"r","id_token":"i","expires_at":expiry,
                "client_secret":"internal"
            }))
            .unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["auth_method"], "consumer");
            assert_eq!(value["token"]["token_type"], "Bearer");
            assert_eq!(value["id_token"], "i");
            assert!(value["client_secret"].is_null());
        }
    }
}

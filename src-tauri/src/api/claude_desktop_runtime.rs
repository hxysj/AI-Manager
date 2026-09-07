use crate::core::error::ManagerError;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) const APP_ID: &str = "claude-desktop";

pub(crate) fn current_target(paths: &crate::core::paths::AppPaths) -> Option<Value> {
    let home = crate::core::paths::home_path();
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let mut target = target(std::env::consts::OS, &home, local.as_deref())?;
    target["managedSkillsPath"] =
        json!(Path::new(&paths.workspace_root).join("claude-desktop-plugins"));
    Some(target)
}

fn target(platform: &str, home: &Path, local: Option<&Path>) -> Option<Value> {
    let root = match platform {
        "windows" => local
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join("AppData/Local")),
        "macos" => home.join("Library/Application Support"),
        _ => return None,
    };
    let mut roots = vec![root.join("Claude"), root.join("Claude-3p")];
    if platform == "windows" {
        roots.extend([
            home.join("AppData/Roaming/Claude"),
            home.join("AppData/Roaming/Claude-3p"),
        ]);
        if let Ok(packages) = std::fs::read_dir(root.join("Packages")) {
            for package in packages.flatten() {
                if package.file_name().to_string_lossy().starts_with("Claude_") {
                    roots.push(package.path().join("LocalCache/Roaming/Claude"));
                    roots.push(package.path().join("LocalCache/Roaming/Claude-3p"));
                }
            }
        }
    }
    let sessions = roots
        .iter()
        .flat_map(|root| {
            [
                root.join("local-agent-mode-sessions"),
                root.join("claude-code-sessions"),
            ]
        })
        .collect::<Vec<_>>();
    let skills = if platform == "windows" {
        PathBuf::from(r"C:\Program Files\Claude\org-plugins")
    } else {
        PathBuf::from("/Library/Application Support/Claude/org-plugins")
    };
    Some(json!({
        "id": APP_ID, "type": APP_ID, "name": "Claude Desktop", "icon": "claude.svg",
        "configPath": root.join("Claude"), "skillsPath": skills,
        "skillsHint": "由本项目打包到原生第三方插件目录，安装可能需要管理员权限。挂载后重启 Desktop；官方模式不使用此目录。源文件修改后需点击修复同步。",
        "sessionsPath": sessions[0], "sessionPaths": sessions,
        "installed": roots.iter().any(|root| root.is_dir()),
        "executablePath": "", "version": ""
    }))
}

pub(crate) fn is_session_file(name: &str) -> bool {
    name.strip_prefix("local_")
        .and_then(|name| name.strip_suffix(".json"))
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
}

fn audit_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_stem()?.to_str()?;
    let id = name.strip_prefix("local_")?;
    uuid::Uuid::parse_str(id).ok()?;
    let parent = path.parent()?;
    [
        parent.join(&id[..8]).join("audit.jsonl"),
        parent.join(name).join("audit.jsonl"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

pub(crate) fn modified_at(path: &Path) -> u64 {
    std::iter::once(path.to_path_buf())
        .chain(audit_path(path))
        .filter_map(|path| {
            std::fs::metadata(path)
                .ok()?
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as u64)
        })
        .max()
        .unwrap_or(0)
}

pub(crate) fn read_content(path: &Path) -> Result<String, ManagerError> {
    if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
        return Ok(std::fs::read_to_string(path)?);
    }
    let metadata: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let mut content = json!({
        "type": "desktop_session_meta", "title": metadata["title"], "cwd": metadata["cwd"],
        "model": metadata["model"], "timestamp": metadata["createdAt"],
        "sessionId": metadata["sessionId"], "cliSessionId": metadata["cliSessionId"]
    })
    .to_string();
    content.push('\n');
    if let Some(audit) = audit_path(path) {
        let transcript = std::fs::read_to_string(audit)?;
        let mut lines = transcript.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim().is_empty() {
                continue;
            }
            if let Err(cause) = serde_json::from_str::<Value>(line) {
                if cause.is_eof() && lines.peek().is_none() && !transcript.ends_with('\n') {
                    break;
                }
                return Err(cause.into());
            }
            content.push_str(line);
            content.push('\n');
        }
    } else if let Some(initial) = metadata["initialMessage"]
        .as_str()
        .filter(|value| !value.is_empty())
    {
        content.push_str(
            &json!({"type": "user", "message": {"role": "user", "content": initial}}).to_string(),
        );
        content.push('\n');
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_target_has_separate_native_session_and_skill_paths() {
        let home = Path::new("/test-home");
        let windows = target("windows", home, Some(Path::new("/test-local"))).unwrap();
        assert_eq!(windows["id"], APP_ID);
        assert!(windows["sessionPaths"].as_array().unwrap().len() >= 4);
        assert!(windows["skillsPath"]
            .as_str()
            .unwrap()
            .ends_with("org-plugins"));
        assert!(target("linux", home, None).is_none());
        let mac = target("macos", home, None).unwrap();
        assert_eq!(mac["sessionPaths"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn only_native_session_metadata_is_scanned() {
        assert!(is_session_file(
            "local_3d54adab-bdb3-48b2-a739-9044f873ecee.json"
        ));
        for name in [
            "audit.jsonl",
            "scheduled-tasks.json",
            "local_settings.json",
            "manifest.json",
        ] {
            assert!(!is_session_file(name));
        }
    }

    #[test]
    fn desktop_code_transcripts_are_reclassified_without_duplicate_sessions_or_tokens() {
        tauri::async_runtime::block_on(async {
            use crate::api::{sessions, usage};
            use crate::core::{
                paths::{ensure_app_directories, resolve_app_paths},
                usage_store,
            };
            let root = std::env::temp_dir()
                .join(format!("ai-manager-desktop-code-{}", uuid::Uuid::new_v4()));
            let paths = resolve_app_paths(&root.join("data"));
            ensure_app_directories(&paths).await.unwrap();
            let cli_root = root.join("cli/projects/project");
            let desktop_root = root.join("Claude-3p/claude-code-sessions/account/org");
            std::fs::create_dir_all(&cli_root).unwrap();
            std::fs::create_dir_all(&desktop_root).unwrap();
            let session_id = "0dc3ca83-01b3-4593-ac78-4a20660054c1";
            let transcript = cli_root.join(format!("{session_id}.jsonl"));
            let content = [
                json!({"type": "user", "message": {"role": "user", "content": "测试 Code"}, "timestamp": 10}),
                json!({"type": "assistant", "message": {"id": "code-message", "role": "assistant", "model": "upstream-model", "content": "完成", "stop_reason": "end_turn", "usage": {"input_tokens": 12, "output_tokens": 4}}, "timestamp": 11})
            ].iter().map(|record| format!("{record}\n")).collect::<String>();
            std::fs::write(&transcript, &content).unwrap();
            let mut state = json!({"cliTargets": [{"id": "claude", "type": "claude", "name": "Claude", "sessionPaths": [cli_root]}], "sessions": []});
            sessions::refresh_sessions_state(&paths, &mut state)
                .await
                .unwrap();
            usage::sync_usage(&paths, json!({}), &state).await.unwrap();
            assert_eq!(
                usage_store::read_all_logs(&paths).unwrap()[0]["appType"],
                "claude"
            );
            std::fs::write(
                desktop_root.join("local_3f94b90c-5493-4487-b579-7ce8fd5f2b31.json"),
                json!({"cliSessionId": session_id, "title": "Desktop Code 测试", "model": "claude-sonnet-4-6"}).to_string(),
            )
            .unwrap();
            state["cliTargets"].as_array_mut().unwrap().push(json!({"id": APP_ID, "type": APP_ID, "name": "Claude Desktop", "sessionPaths": [desktop_root]}));
            let files =
                usage::collect_cli_session_files(state["cliTargets"].as_array().unwrap()).unwrap();
            assert_eq!(files.len(), 1);
            assert_eq!(files[0]["cli"], APP_ID);
            usage_store::write_request_record(&paths, &json!({
                "requestId": "desktop-response:code-message", "appType": APP_ID,
                "providerId": "desktop-provider", "providerName": "测试供应商",
                "model": "upstream-model", "requestModel": "claude-sonnet-4-6",
                "requestSource": "proxy-managed"
            })).unwrap();
            for _ in 0..2 {
                usage::sync_usage(&paths, json!({}), &state).await.unwrap();
            }
            let logs = usage_store::read_all_logs(&paths).unwrap();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0]["appType"], APP_ID);
            assert_eq!(logs[0]["inputTokens"], 12);
            assert_eq!(logs[0]["dataSource"], "desktop_code");
            assert_eq!(logs[0]["providerId"], "desktop-provider");
            assert_eq!(logs[0]["model"], "upstream-model");
            assert_eq!(logs[0]["requestModel"], "claude-sonnet-4-6");
            sessions::refresh_sessions_state(&paths, &mut state)
                .await
                .unwrap();
            assert_eq!(state["sessions"].as_array().unwrap().len(), 1);
            assert_eq!(state["sessions"][0]["title"], "Desktop Code 测试");
            assert_eq!(state["sessions"][0]["readOnly"], true);
            assert_eq!(std::fs::read_to_string(&transcript).unwrap(), content);
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ai-manager-desktop-code-"));
            std::fs::remove_dir_all(resolved).unwrap();
        });
    }

    #[test]
    fn desktop_sessions_usage_and_request_records_share_read_only_incremental_scan() {
        tauri::async_runtime::block_on(async {
            use crate::api::{sessions, usage};
            use crate::core::{
                paths::{ensure_app_directories, resolve_app_paths},
                usage_store,
            };
            let root = std::env::temp_dir()
                .join(format!("ai-manager-desktop-scan-{}", uuid::Uuid::new_v4()));
            let paths = resolve_app_paths(&root.join("data"));
            ensure_app_directories(&paths).await.unwrap();
            let source = root.join("Claude-3p/local-agent-mode-sessions/account/org");
            let metadata_path = source.join("local_3d54adab-bdb3-48b2-a739-9044f873ecee.json");
            let audit = source.join("3d54adab/audit.jsonl");
            std::fs::create_dir_all(audit.parent().unwrap()).unwrap();
            let metadata = json!({"sessionId": "local_3d54adab-bdb3-48b2-a739-9044f873ecee", "title": "Desktop 测试", "createdAt": 1, "cwd": "test-project", "model": "claude-sonnet-4-6", "systemPrompt": "private-system-prompt", "remoteMcpServersConfig": {"token": "private-secret"}}).to_string();
            std::fs::write(&metadata_path, &metadata).unwrap();
            std::fs::write(source.join("scheduled-tasks.json"), "{}").unwrap();
            let first_turn = [
                json!({"type": "user", "message": {"role": "user", "content": "开始测试"}, "timestamp": 10}),
                json!({"type": "assistant", "message": {"id": "desktop-message-one", "role": "assistant", "content": "第一轮完成", "model": "upstream-model", "stop_reason": "end_turn", "usage": {"input_tokens": 10, "output_tokens": 2}}, "timestamp": 11}),
                json!({"type": "system", "model": "claude-sonnet-4-6", "timestamp": 11}),
                json!({"type": "result", "uuid": "turn-one", "usage": {"input_tokens": 10, "output_tokens": 2, "cache_read_input_tokens": 3}, "modelUsage": {"claude-sonnet-4-6": {"inputTokens": 1000}}, "timestamp": 12})
            ].iter().map(|record| format!("{record}\n")).collect::<String>();
            std::fs::write(&audit, &first_turn).unwrap();
            let mut state = json!({"cliTargets": [{"id": APP_ID, "type": APP_ID, "name": "Claude Desktop", "sessionPaths": [source]}], "sessions": []});
            sessions::refresh_sessions_state(&paths, &mut state)
                .await
                .unwrap();
            assert_eq!(state["sessions"].as_array().unwrap().len(), 1);
            let session = state["sessions"][0].clone();
            assert_eq!(session["title"], "Desktop 测试");
            assert_eq!(session["readOnly"], true);
            let messages =
                sessions::load_session_messages(&paths, json!({"sessionId": session["id"]}))
                    .await
                    .unwrap();
            assert!(messages.to_string().contains("第一轮完成"));
            assert!(!messages.to_string().contains("private-secret"));
            assert!(!read_content(&metadata_path)
                .unwrap()
                .contains("private-system-prompt"));
            assert!(sessions::delete_session(
                &paths,
                &state["cliTargets"],
                json!({"sessionId": session["id"]})
            )
            .await
            .is_err());
            for _ in 0..2 {
                let result = usage::sync_usage(&paths, json!({"appType": APP_ID}), &state)
                    .await
                    .unwrap();
                assert_eq!(result["diagnostics"], json!([]));
            }
            let logs = usage_store::read_all_logs(&paths).unwrap();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0]["inputTokens"], 10);
            assert_eq!(logs[0]["outputTokens"], 2);
            assert_eq!(logs[0]["appType"], APP_ID);
            assert_eq!(logs[0]["dataSource"], "desktop_audit");
            assert_eq!(logs[0]["model"], "upstream-model");
            assert_eq!(logs[0]["requestModel"], "claude-sonnet-4-6");
            assert_eq!(
                usage_store::read_request_records(&paths, &["desktop-turn:turn-one".to_string()])
                    .unwrap()
                    .len(),
                1
            );
            for (model, cost, expected_cost) in [
                ("upstream-model", 5, 5),
                ("claude-sonnet-4-6", 99, 0),
            ] {
                let mut cached_log = logs[0].clone();
                cached_log["model"] = json!(model);
                cached_log["totalCostUsd"] = json!(cost);
                cached_log["costLockedAt"] = json!(1);
                usage_store::write_usage_cost_snapshots(&paths, &[cached_log.clone()]).unwrap();
                usage_store::ensure_session_parser_version(&paths, APP_ID, 0).unwrap();
                usage_store::replace_sessions(&paths, &[usage_store::UsageSessionUpdate {
                    raw_path: session["rawPath"].as_str().unwrap().to_string(),
                    app_type: APP_ID.to_string(),
                    updated_at: modified_at(&metadata_path),
                    logs: vec![cached_log],
                    records: Vec::new(),
                }]).unwrap();
                state["providers"] = json!([{"id": "new-provider", "cli": APP_ID, "runtimeConfig": {"mainModel": "different-model"}}]);
                for _ in 0..2 {
                    usage::sync_usage(&paths, json!({"appType": APP_ID}), &state).await.unwrap();
                    let refreshed = usage_store::read_all_logs(&paths).unwrap();
                    assert_eq!(refreshed.len(), 1);
                    assert_eq!(refreshed[0]["model"], "upstream-model");
                    assert_eq!(refreshed[0]["requestModel"], "claude-sonnet-4-6");
                    assert_eq!(refreshed[0]["totalCostUsd"].as_f64().unwrap(), expected_cost as f64);
                    let records = usage_store::read_request_records(&paths, &["desktop-turn:turn-one".to_string()]).unwrap();
                    assert_eq!(records["desktop-turn:turn-one"]["model"], "upstream-model");
                }
            }
            usage_store::write_request_record(&paths, &json!({
                "requestId": "desktop-response:desktop-message-one", "appType": APP_ID,
                "providerId": "desktop-provider", "providerName": "测试供应商",
                "model": "upstream-model", "requestModel": "claude-sonnet-4-6",
                "requestSource": "proxy-managed"
            })).unwrap();
            usage_store::ensure_session_parser_version(&paths, APP_ID, 0).unwrap();
            for _ in 0..2 {
                usage::sync_usage(&paths, json!({}), &state).await.unwrap();
                let stats = usage::get_stats(&paths, json!({"statsScope": "provider", "appType": APP_ID, "providerId": "desktop-provider"})).await.unwrap();
                assert_eq!(stats["data"]["summary"]["requestCount"], 1);
                assert_eq!(stats["data"]["summary"]["inputTokens"], 10);
                assert_eq!(stats["data"]["modelStats"][0]["model"], "upstream-model");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            let second_turn = json!({"type": "result", "uuid": "turn-two", "usage": {"input_tokens": 5, "output_tokens": 3}, "modelUsage": {"upstream-model": {"inputTokens": 2000}}, "timestamp": 20});
            let transcript = format!("{first_turn}{second_turn}\n");
            std::fs::write(&audit, &transcript).unwrap();
            sessions::refresh_sessions_state(&paths, &mut state)
                .await
                .unwrap();
            assert_ne!(state["sessions"][0]["updatedAt"], session["updatedAt"]);
            usage::sync_usage(&paths, json!({"appType": APP_ID}), &state)
                .await
                .unwrap();
            let logs = usage_store::read_all_logs(&paths).unwrap();
            assert_eq!(logs.len(), 2);
            let second_log = logs.iter().find(|log| log["requestId"] == "desktop-turn:turn-two").unwrap();
            assert_eq!(second_log["model"], "claude-sonnet-4-6");
            assert_eq!(second_log["providerId"], APP_ID);
            let first_log = logs.iter().find(|log| log["requestId"] == "desktop-turn:turn-one").unwrap();
            assert_eq!(first_log["providerId"], "desktop-provider");
            assert_eq!(
                logs.iter()
                    .map(|log| log["inputTokens"].as_u64().unwrap())
                    .sum::<u64>(),
                15
            );
            assert_eq!(std::fs::read_to_string(&metadata_path).unwrap(), metadata);
            assert_eq!(std::fs::read_to_string(&audit).unwrap(), transcript);
            std::fs::write(&audit, format!("{transcript}{{\"type\":")).unwrap();
            assert!(read_content(&metadata_path).is_ok());
            std::fs::write(&audit, "not-json\n").unwrap();
            assert!(read_content(&metadata_path).is_err());
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ai-manager-desktop-scan-"));
            std::fs::remove_dir_all(resolved).unwrap();
        });
    }
}

use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use crate::core::settings::{resolve_portable_path, string_value};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static PROMPT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
const COMMON_PROMPT_CLI: &str = "common";

pub fn build_state(paths: &AppPaths, cli_targets: &Value) -> Result<Value, ManagerError> {
    let runtime_state = refresh_drift_state(paths, cli_targets)?;

    write_json_sync(
        &paths.storage_files.prompt_runtime_state,
        &Value::Object(runtime_state.clone()),
    )?;

    let prompts = load_prompts(paths)?;
    let profiles = load_profiles(paths)?;

    Ok(json!({
      "supportedClis": supported_clis(),
      "prompts": prompts,
      "profiles": profiles,
      "runtimeState": runtime_state
    }))
}

pub async fn save_rule(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let prompt = save_prompt(paths, payload).await?;
    let state = build_state(paths, cli_targets)?;
    let prompt_id = string_value(prompt.get("id"));
    let prompt_cli = string_value(prompt.get("cli"));
    let mut synced_active_prompt = false;

    for cli in ["claude", "codex"] {
        let active_prompt_id = string_value(state["profiles"][cli].get("activePromptId"));

        if active_prompt_id == prompt_id && (prompt_cli == COMMON_PROMPT_CLI || prompt_cli == cli) {
            enable_prompt(paths, &prompt_id, cli, cli_targets).await?;
            synced_active_prompt = true;
        }
    }

    if !synced_active_prompt {
        refresh_drift(paths, cli_targets).await?;
    }

    Ok(())
}

pub async fn delete_rule(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let rule_id = string_value(payload.get("ruleId").or(Some(&payload)));

    delete_prompt(paths, &rule_id).await?;
    refresh_drift(paths, cli_targets).await
}

pub async fn toggle_rule(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    if payload.get("enabled").and_then(Value::as_bool) == Some(false) {
        disable_prompt(paths, payload, cli_targets).await?;
        refresh_drift(paths, cli_targets).await?;
        return Ok(());
    }

    enable_rule(paths, payload, cli_targets).await
}

pub async fn enable_rule(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let rule_id = string_value(payload.get("ruleId").or(Some(&payload)));
    let prompt = find_prompt(paths, &rule_id)?
        .ok_or_else(|| ManagerError::System("Prompt 不存在".to_string()))?;
    let prompt_cli = string_value(prompt.get("cli"));
    let cli = if prompt_cli == COMMON_PROMPT_CLI {
        normalize_runtime_cli(payload.get("targetCli").or_else(|| payload.get("cli")))?
    } else {
        normalize_runtime_cli(prompt.get("cli"))?
    };

    enable_prompt(paths, &rule_id, &cli, cli_targets).await
}

pub async fn import_global_rule(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let cli = normalize_runtime_cli(payload.get("cli"))?;
    let cli_target = find_cli_target(cli_targets, &cli);
    let content = read_global_prompt_content(paths, &cli, cli_target.as_ref()).await?;

    if find_same_content_prompt(paths, &cli, &content)?.is_some() {
        return Err(ManagerError::System(
            "当前全局 Prompt 内容已存在于规则库中，无需重复导入".to_string(),
        ));
    }

    save_prompt(
        paths,
        json!({
          "cli": cli,
          "name": payload.get("name").cloned().unwrap_or(Value::Null),
          "description": payload.get("description").cloned().unwrap_or(Value::Null),
          "content": content
        }),
    )
    .await?;
    refresh_drift(paths, cli_targets).await
}

pub async fn preview_import_global_rule(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<Value, ManagerError> {
    let cli = normalize_runtime_cli(payload.get("cli"))?;
    let cli_target = find_cli_target(cli_targets, &cli);
    let runtime_path = get_runtime_path(&cli, cli_target.as_ref());
    let content = read_global_prompt_content(paths, &cli, cli_target.as_ref()).await?;

    if let Some(prompt) = find_same_content_prompt(paths, &cli, &content)? {
        return Ok(json!({
          "status": "SAME_CONTENT",
          "prompt": prompt,
          "runtimeContent": content,
          "runtimePath": runtime_path
        }));
    }

    let scoped_prompts = load_prompts(paths)?
        .into_iter()
        .filter(|item| item.get("cli").and_then(Value::as_str) == Some(cli.as_str()))
        .collect::<Vec<_>>();

    if scoped_prompts.is_empty() {
        return Ok(json!({
          "status": "NEW",
          "runtimeContent": content,
          "runtimePath": runtime_path
        }));
    }

    if let Some((prompt, similarity)) = find_similar_content_prompt(paths, &cli, &content)? {
        if similarity > 0.8 {
            return Ok(json!({
              "status": "DIFF",
              "prompt": prompt,
              "similarity": similarity,
              "managerContent": prompt.get("content").cloned().unwrap_or(Value::Null),
              "runtimeContent": content,
              "runtimePath": runtime_path
            }));
        }
    }

    Ok(json!({
      "status": "NEW",
      "runtimeContent": content,
      "runtimePath": runtime_path
    }))
}

pub async fn resolve_import_conflict(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    if string_value(payload.get("source")) == "manager" {
        return Ok(());
    }

    if string_value(payload.get("source")) != "runtime" {
        return Err(ManagerError::System(
            "请选择要保存的 Prompt 版本".to_string(),
        ));
    }

    let rule_id = string_value(payload.get("ruleId"));
    let prompt = find_prompt(paths, &rule_id)?
        .ok_or_else(|| ManagerError::System("Prompt 不存在".to_string()))?;

    save_prompt(
        paths,
        json!({
          "id": prompt["id"],
          "cli": prompt["cli"],
          "name": prompt["name"],
          "description": prompt["description"],
          "content": payload.get("runtimeContent").cloned().unwrap_or(Value::Null)
        }),
    )
    .await?;
    refresh_drift(paths, cli_targets).await
}

pub async fn compare_rule(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<Value, ManagerError> {
    let rule_id = string_value(payload.get("ruleId"));
    let prompt = find_prompt(paths, &rule_id)?
        .ok_or_else(|| ManagerError::System("Prompt 不存在".to_string()))?;
    let prompt_cli = string_value(prompt.get("cli"));
    let cli = if prompt_cli == COMMON_PROMPT_CLI {
        normalize_runtime_cli(payload.get("targetCli").or_else(|| payload.get("cli")))?
    } else {
        normalize_runtime_cli(prompt.get("cli"))?
    };
    let cli_target = find_cli_target(cli_targets, &cli);
    let runtime_path = get_runtime_path(&cli, cli_target.as_ref());
    let runtime_content = if runtime_path.is_empty() || !Path::new(&runtime_path).exists() {
        String::new()
    } else {
        tokio::fs::read_to_string(&runtime_path).await?
    };

    Ok(json!({
      "prompt": prompt,
      "managerContent": build_runtime_content(&string_value(prompt.get("content"))),
      "runtimeContent": runtime_content,
      "runtimePath": runtime_path
    }))
}

pub async fn resolve_rule_drift(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let cli = normalize_runtime_cli(payload.get("cli"))?;
    let state = build_state(paths, cli_targets)?;
    let active_prompt_id = string_value(state["profiles"][&cli].get("activePromptId"));

    if active_prompt_id.is_empty() {
        return Err(ManagerError::System(
            "当前 CLI 没有启用的 Prompt".to_string(),
        ));
    }

    let cli_target = find_cli_target(cli_targets, &cli);
    let runtime_path = get_runtime_path(&cli, cli_target.as_ref());

    if string_value(payload.get("source")) == "runtime" {
        let runtime_content = tokio::fs::read_to_string(&runtime_path).await?;
        let active_prompt = find_prompt(paths, &active_prompt_id)?
            .ok_or_else(|| ManagerError::System("Prompt 不存在".to_string()))?;

        save_prompt(
            paths,
            json!({
              "id": active_prompt["id"],
              "cli": active_prompt["cli"],
              "name": active_prompt["name"],
              "description": active_prompt["description"],
              "content": runtime_content
            }),
        )
        .await?;
    }

    enable_prompt(paths, &active_prompt_id, &cli, cli_targets).await
}

pub async fn move_rule(_: &AppPaths) -> Result<(), ManagerError> {
    Ok(())
}

pub async fn refresh_drift(paths: &AppPaths, cli_targets: &Value) -> Result<(), ManagerError> {
    let runtime_state = refresh_drift_state(paths, cli_targets)?;

    write_json(
        &paths.storage_files.prompt_runtime_state,
        &Value::Object(runtime_state),
    )
    .await
}

fn refresh_drift_state(
    paths: &AppPaths,
    cli_targets: &Value,
) -> Result<serde_json::Map<String, Value>, ManagerError> {
    let prompts = load_prompts(paths)?;
    let profiles = load_profiles(paths)?;
    let mut runtime_state = read_json_object(&paths.storage_files.prompt_runtime_state)?;

    for cli in ["claude", "codex"] {
        let cli_target = find_cli_target(cli_targets, cli);
        let active_prompt_id = string_value(profiles[cli].get("activePromptId"));
        let active_prompt = prompts
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(active_prompt_id.as_str()));
        let runtime_path = get_runtime_path(cli, cli_target.as_ref());
        let previous_state = runtime_state
            .get(cli)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut next_state = previous_state.clone();

        if active_prompt.is_none() {
            next_state.insert("activePromptId".to_string(), json!(""));
            next_state.insert("runtimePath".to_string(), json!(runtime_path));
            next_state.insert("status".to_string(), json!("NO_ACTIVE"));
            runtime_state.insert(cli.to_string(), Value::Object(next_state));
            continue;
        }

        let active_prompt = active_prompt.unwrap();
        let manager_content = build_runtime_content(&string_value(active_prompt.get("content")));

        if runtime_path.is_empty() || !Path::new(&runtime_path).exists() {
            next_state.insert("activePromptId".to_string(), json!(active_prompt_id));
            next_state.insert("runtimePath".to_string(), json!(runtime_path));
            next_state.insert("status".to_string(), json!("DIRTY_MANAGER"));
            runtime_state.insert(cli.to_string(), Value::Object(next_state));
            continue;
        }

        let runtime_content = std::fs::read_to_string(&runtime_path)?;
        let manager_hash = sha256_text(&manager_content);
        let runtime_hash = sha256_text(&runtime_content);
        let previous_hash = previous_state
            .get("runtimeHash")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut status = "SYNCED";

        if runtime_hash != manager_hash {
            if previous_hash.is_empty() {
                status = "MODIFIED_EXTERNALLY";
            } else if runtime_hash != previous_hash && manager_hash != previous_hash {
                status = "CONFLICT";
            } else if manager_hash != previous_hash {
                status = "DIRTY_MANAGER";
            } else {
                status = "MODIFIED_EXTERNALLY";
            }
        }

        next_state.insert("activePromptId".to_string(), json!(active_prompt_id));
        if status == "SYNCED" {
            next_state.insert("runtimeHash".to_string(), json!(runtime_hash));
        }
        next_state.insert("runtimePath".to_string(), json!(runtime_path));
        next_state.insert("status".to_string(), json!(status));
        runtime_state.insert(cli.to_string(), Value::Object(next_state));
    }

    Ok(runtime_state)
}

async fn save_prompt(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let cli = normalize_cli(payload.get("cli"))?;
    let prompts = load_prompts(paths)?;
    let previous = payload
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| {
            prompts
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
        })
        .cloned();
    let name = first_string(
        payload.get("name"),
        previous.as_ref().and_then(|item| item.get("name")),
    );
    let content = string_value(payload.get("content"));

    if name.is_empty() {
        return Err(ManagerError::System("Prompt 名称不能为空".to_string()));
    }

    if content.trim().is_empty() {
        return Err(ManagerError::System("Prompt 内容不能为空".to_string()));
    }

    if prompts.iter().any(|item| {
        item.get("cli").and_then(Value::as_str) == Some(cli.as_str())
            && item.get("id") != previous.as_ref().and_then(|prompt| prompt.get("id"))
            && string_value(item.get("name")).eq_ignore_ascii_case(&name)
    }) {
        return Err(ManagerError::System(
            "当前 CLI 已存在同名 Prompt".to_string(),
        ));
    }

    let prompt_id = previous
        .as_ref()
        .map(|item| string_value(item.get("id")))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| create_prompt_id(&prompts, &cli, &name));
    let file_name = previous
        .as_ref()
        .map(|item| string_value(item.get("fileName")))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{}.md", prompt_id));
    let metadata = json!({
      "id": prompt_id,
      "name": name,
      "description": optional_string(payload.get("description"), previous.as_ref().and_then(|item| item.get("description"))),
      "cli": cli,
      "fileName": file_name,
      "createdAt": previous.as_ref().and_then(|item| item.get("createdAt")).and_then(Value::as_u64).unwrap_or_else(now_millis),
      "updatedAt": now_millis()
    });
    let prompt_dir = prompt_dir(paths, &cli);

    tokio::fs::create_dir_all(&prompt_dir).await?;
    tokio::fs::write(prompt_dir.join(&file_name), format!("{}\n", content.trim())).await?;
    write_json(&metadata_path(paths, &cli, &prompt_id), &metadata).await?;

    if let Some(previous) = previous {
        let metadata_file_name = string_value(previous.get("metadataFileName"));

        if !metadata_file_name.is_empty() && metadata_file_name != format!("{}.json", prompt_id) {
            remove_file_if_exists(prompt_dir.join(metadata_file_name)).await?;
        }
    }

    let mut prompt = metadata;

    prompt["content"] = json!(format!("{}\n", content.trim()));
    prompt["metadataFileName"] = json!(format!("{}.json", prompt_id));
    prompt["storageDir"] = json!(prompt_dir.to_string_lossy().to_string());
    Ok(prompt)
}

async fn delete_prompt(paths: &AppPaths, prompt_id: &str) -> Result<(), ManagerError> {
    let Some(prompt) = find_prompt(paths, prompt_id)? else {
        return Ok(());
    };
    let profiles = load_profiles(paths)?;
    let cli = string_value(prompt.get("cli"));

    if ["claude", "codex"].iter().any(|cli| {
        string_value(profiles[*cli].get("activePromptId")) == string_value(prompt.get("id"))
    }) {
        return Err(ManagerError::System(
            "当前 Prompt 已启用，请先切换到其他 Prompt 后再删除".to_string(),
        ));
    }

    let prompt_dir = prompt_dir(paths, &cli);

    remove_file_if_exists(prompt_dir.join(first_string(
        prompt.get("metadataFileName"),
        Some(&json!(format!("{}.json", prompt_id))),
    )))
    .await?;
    remove_file_if_exists(prompt_dir.join(string_value(prompt.get("fileName")))).await
}

async fn enable_prompt(
    paths: &AppPaths,
    prompt_id: &str,
    cli: &str,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let prompt = find_prompt(paths, prompt_id)?
        .ok_or_else(|| ManagerError::System("Prompt 不存在".to_string()))?;
    let cli_target = find_cli_target(cli_targets, &cli);
    let runtime_path = get_runtime_path(&cli, cli_target.as_ref());

    if runtime_path.is_empty() {
        return Err(ManagerError::System(
            "未找到对应 CLI 的全局配置目录".to_string(),
        ));
    }

    let runtime_content = build_runtime_content(&string_value(prompt.get("content")));

    if let Some(parent) = Path::new(&runtime_path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&runtime_path, &runtime_content).await?;
    write_json(
        &profile_path(paths, &cli),
        &json!({ "activePromptId": prompt_id }),
    )
    .await?;

    let mut runtime_state = read_json_object(&paths.storage_files.prompt_runtime_state)?;

    runtime_state.insert(
        cli.to_string(),
        json!({
          "activePromptId": prompt_id,
          "runtimeHash": sha256_text(&runtime_content),
          "lastSyncAt": now_millis(),
          "runtimePath": runtime_path,
          "status": "SYNCED"
        }),
    );
    write_json(
        &paths.storage_files.prompt_runtime_state,
        &Value::Object(runtime_state),
    )
    .await
}

async fn disable_prompt(
    paths: &AppPaths,
    payload: Value,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    let prompts = load_prompts(paths)?;
    let prompt_id = string_value(payload.get("promptId").or_else(|| payload.get("ruleId")));
    let prompt = prompts
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(prompt_id.as_str()))
        .cloned();
    let prompt_cli = prompt
        .as_ref()
        .map(|item| string_value(item.get("cli")))
        .unwrap_or_default();
    let cli = if prompt_cli == COMMON_PROMPT_CLI {
        normalize_runtime_cli(payload.get("targetCli").or_else(|| payload.get("cli")))?
    } else {
        normalize_runtime_cli(
            payload
                .get("targetCli")
                .or_else(|| payload.get("cli"))
                .or_else(|| prompt.as_ref().and_then(|item| item.get("cli"))),
        )?
    };
    let profiles = load_profiles(paths)?;

    if prompt.is_some() && string_value(profiles[&cli].get("activePromptId")) != prompt_id {
        return Ok(());
    }

    let cli_target = find_cli_target(cli_targets, &cli);
    let runtime_path = get_runtime_path(&cli, cli_target.as_ref());

    if !runtime_path.is_empty() {
        remove_file_if_exists(&runtime_path).await?;
    }

    write_json(&profile_path(paths, &cli), &json!({ "activePromptId": "" })).await?;
    let mut runtime_state = read_json_object(&paths.storage_files.prompt_runtime_state)?;
    let mut next_state = runtime_state
        .get(&cli)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    next_state.insert("activePromptId".to_string(), json!(""));
    next_state.insert("runtimePath".to_string(), json!(runtime_path));
    next_state.insert("status".to_string(), json!("NO_ACTIVE"));
    runtime_state.insert(cli, Value::Object(next_state));
    write_json(
        &paths.storage_files.prompt_runtime_state,
        &Value::Object(runtime_state),
    )
    .await
}

async fn read_global_prompt_content(
    paths: &AppPaths,
    cli: &str,
    cli_target: Option<&Value>,
) -> Result<String, ManagerError> {
    let runtime_path = get_runtime_path(cli, cli_target);

    if runtime_path.is_empty() || !Path::new(&runtime_path).exists() {
        return Err(ManagerError::System(
            "未找到可导入的全局 Prompt 文件".to_string(),
        ));
    }

    let content = tokio::fs::read_to_string(runtime_path).await?;

    if content.trim().is_empty() {
        return Err(ManagerError::System(
            "全局 Prompt 文件为空，无法导入".to_string(),
        ));
    }

    let _ = paths;
    Ok(normalize_prompt_content(&content))
}

fn load_prompts(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    let mut prompts = Vec::new();

    for cli in [COMMON_PROMPT_CLI, "claude", "codex"] {
        let prompt_dir = prompt_dir(paths, cli);

        if !prompt_dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(&prompt_dir)? {
            let entry = entry?;
            let entry_path = entry.path();

            if !entry_path.is_file()
                || entry_path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }

            let metadata: Value = serde_json::from_str(&std::fs::read_to_string(&entry_path)?)?;

            if string_value(metadata.get("id")).is_empty()
                || string_value(metadata.get("fileName")).is_empty()
            {
                continue;
            }

            let content =
                std::fs::read_to_string(prompt_dir.join(string_value(metadata.get("fileName"))))
                    .unwrap_or_default();
            let mut prompt = metadata;

            prompt["content"] = json!(content);
            prompt["metadataFileName"] = json!(entry_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default());
            prompt["storageDir"] = json!(prompt_dir.to_string_lossy().to_string());
            prompts.push(prompt);
        }
    }

    prompts.sort_by(|left, right| {
        let left_cli = string_value(left.get("cli"));
        let right_cli = string_value(right.get("cli"));

        if left_cli != right_cli {
            return left_cli.cmp(&right_cli);
        }

        number_desc(right.get("updatedAt")).cmp(&number_desc(left.get("updatedAt")))
    });

    Ok(prompts)
}

fn load_profiles(paths: &AppPaths) -> Result<Value, ManagerError> {
    Ok(json!({
      "claude": read_json_file(profile_path(paths, "claude"), json!({ "activePromptId": "" }))?,
      "codex": read_json_file(profile_path(paths, "codex"), json!({ "activePromptId": "" }))?
    }))
}

fn supported_clis() -> Value {
    json!([
      {
        "id": "claude",
        "name": "Claude",
        "icon": "claude.svg",
        "runtimeFileName": "CLAUDE.md"
      },
      {
        "id": "codex",
        "name": "Codex",
        "icon": "codex.svg",
        "runtimeFileName": "AGENTS.md"
      }
    ])
}

fn find_prompt(paths: &AppPaths, prompt_id: &str) -> Result<Option<Value>, ManagerError> {
    Ok(load_prompts(paths)?
        .into_iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(prompt_id)))
}

fn find_same_content_prompt(
    paths: &AppPaths,
    cli: &str,
    content: &str,
) -> Result<Option<Value>, ManagerError> {
    let normalized_content = normalize_prompt_content(content);

    Ok(load_prompts(paths)?.into_iter().find(|item| {
        item.get("cli").and_then(Value::as_str) == Some(cli)
            && normalize_prompt_content(&string_value(item.get("content"))) == normalized_content
    }))
}

fn find_similar_content_prompt(
    paths: &AppPaths,
    cli: &str,
    content: &str,
) -> Result<Option<(Value, f64)>, ManagerError> {
    let mut items = load_prompts(paths)?
        .into_iter()
        .filter(|item| item.get("cli").and_then(Value::as_str) == Some(cli))
        .map(|item| {
            let similarity = calculate_similarity(&string_value(item.get("content")), content);

            (item, similarity)
        })
        .filter(|(_, similarity)| *similarity < 1.0)
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(items.into_iter().next())
}

fn normalize_cli(value: Option<&Value>) -> Result<String, ManagerError> {
    let cli = string_value(value);

    if ![COMMON_PROMPT_CLI, "claude", "codex"].contains(&cli.as_str()) {
        return Err(ManagerError::System(
            "Prompt 仅支持通用、Claude 和 Codex".to_string(),
        ));
    }

    Ok(cli)
}

fn normalize_runtime_cli(value: Option<&Value>) -> Result<String, ManagerError> {
    let cli = string_value(value);

    if !["claude", "codex"].contains(&cli.as_str()) {
        return Err(ManagerError::System(
            "请选择要挂载的 Claude 或 Codex".to_string(),
        ));
    }

    Ok(cli)
}

fn get_runtime_path(cli: &str, cli_target: Option<&Value>) -> String {
    let config_path = cli_target
        .and_then(|target| target.get("configPath"))
        .map(|value| string_value(Some(value)))
        .unwrap_or_default();

    if config_path.is_empty() {
        return String::new();
    }

    let file_name = if cli == "claude" {
        "CLAUDE.md"
    } else {
        "AGENTS.md"
    };

    let config_path = resolve_portable_path(&config_path);

    Path::new(&config_path)
        .join(file_name)
        .to_string_lossy()
        .to_string()
}

fn find_cli_target(cli_targets: &Value, cli: &str) -> Option<Value> {
    cli_targets.as_array().and_then(|items| {
        items
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(cli))
            .cloned()
    })
}

fn build_runtime_content(content: &str) -> String {
    content.trim().to_string()
}

fn normalize_prompt_content(content: &str) -> String {
    content
        .replace("\r\n", "\n")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn calculate_similarity(left: &str, right: &str) -> f64 {
    let left_text = normalize_prompt_content(left);
    let right_text = normalize_prompt_content(right);

    if left_text.is_empty() && right_text.is_empty() {
        return 1.0;
    }

    if left_text.is_empty() || right_text.is_empty() {
        return 0.0;
    }

    if left_text == right_text {
        return 1.0;
    }

    let left_bigrams = create_bigrams(&left_text);
    let right_bigrams = create_bigrams(&right_text);
    let mut right_counts = std::collections::HashMap::new();
    let mut intersection = 0;

    for item in &right_bigrams {
        *right_counts.entry(item.clone()).or_insert(0) += 1;
    }

    for item in &left_bigrams {
        let count = right_counts.get(item).copied().unwrap_or(0);

        if count > 0 {
            intersection += 1;
            right_counts.insert(item.clone(), count - 1);
        }
    }

    (2.0 * intersection as f64) / (left_bigrams.len() + right_bigrams.len()) as f64
}

fn create_bigrams(value: &str) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();

    if chars.len() < 2 {
        return if value.is_empty() {
            Vec::new()
        } else {
            vec![value.to_string()]
        };
    }

    chars
        .windows(2)
        .map(|items| items.iter().collect::<String>())
        .collect()
}

fn create_prompt_id(prompts: &[Value], cli: &str, name: &str) -> String {
    let base_id = slugify_name(name);

    if base_id.is_empty() {
        return format!(
            "prompt-{}-{}",
            now_millis(),
            PROMPT_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
    }

    let used_ids = prompts
        .iter()
        .map(|item| string_value(item.get("id")))
        .collect::<std::collections::HashSet<_>>();

    if !used_ids.contains(&base_id) {
        return base_id;
    }

    for index in 2..1000 {
        let next_id = format!("{}-{}", base_id, index);

        if !used_ids.contains(&next_id) {
            return next_id;
        }
    }

    format!(
        "{}-{}-{}",
        cli,
        base_id,
        PROMPT_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn slugify_name(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;

    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn prompt_dir(paths: &AppPaths, cli: &str) -> PathBuf {
    Path::new(&paths.prompts_dir).join(cli)
}

fn metadata_path(paths: &AppPaths, cli: &str, prompt_id: &str) -> PathBuf {
    prompt_dir(paths, cli).join(format!("{}.json", prompt_id))
}

fn profile_path(paths: &AppPaths, cli: &str) -> PathBuf {
    Path::new(&paths.prompt_profiles_dir).join(format!("{}-profile.json", cli))
}

fn read_json_file(path: impl AsRef<Path>, fallback: Value) -> Result<Value, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(fallback),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn read_json_object(
    path: impl AsRef<Path>,
) -> Result<serde_json::Map<String, Value>, ManagerError> {
    Ok(read_json_file(path, json!({}))?
        .as_object()
        .cloned()
        .unwrap_or_default())
}

async fn write_json(path: impl AsRef<Path>, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = path.as_ref().parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )
    .await?;
    Ok(())
}

fn write_json_sync(path: impl AsRef<Path>, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )?;
    Ok(())
}

async fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<(), ManagerError> {
    match tokio::fs::remove_file(path).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn optional_string(value: Option<&Value>, fallback: Option<&Value>) -> Value {
    let text = first_string(value, fallback);

    if text.is_empty() {
        Value::Null
    } else {
        json!(text)
    }
}

fn first_string(value: Option<&Value>, fallback: Option<&Value>) -> String {
    let text = string_value(value);

    if text.is_empty() {
        string_value(fallback)
    } else {
        text
    }
}

fn sha256_text(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn number_desc(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(0)
}

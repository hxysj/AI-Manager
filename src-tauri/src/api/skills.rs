use crate::core::error::ManagerError;
use crate::core::paths::{path_text, AppPaths};
use crate::core::settings::serialize_portable_path;
use crate::core::skill_store;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use url::Url;

const IGNORE_DIRS: [&str; 6] = [".git", "node_modules", "dist", "build", ".cache", "temp"];
const SKILL_PREVIEW_MAX_SIZE: u64 = 512 * 1024;
const SKILL_TRASH_RETENTION_MS: u128 = 10 * 24 * 60 * 60 * 1000;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub async fn refresh_skills_state(paths: &AppPaths, state: &mut Value) -> Result<(), ManagerError> {
    cleanup_expired_skill_trash(paths).await?;

    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let repos = state
        .get("repos")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut repo_skill_counts = repos
        .iter()
        .filter_map(|repo| {
            Some((
                string_value(repo.get("id")),
                repo.get("skillCount").and_then(Value::as_u64).unwrap_or(0),
            ))
        })
        .collect::<HashMap<_, _>>();
    let previous_skills = skill_store::read_skills(paths)?;
    let mut install_index = skill_store::read_installs(paths)?
        .into_iter()
        .map(|(key, value)| (key, value.as_array().cloned().unwrap_or_default()))
        .collect::<HashMap<_, _>>();
    let scanned_items = scan_many(
        std::iter::once((paths.skills_dir.clone(), Value::Null))
            .chain(repos.iter().map(|repo| {
                (
                    string_value(repo.get("localPath")),
                    repo.get("id").cloned().unwrap_or(Value::Null),
                )
            }))
            .collect(),
    )?;
    let mut diagnostics = Vec::new();
    let mut parsed_skills = Vec::new();
    let mut used_names = HashSet::new();

    for scanned_item in scanned_items {
        match parse_skill(&scanned_item.skill_root, scanned_item.repo_id.clone()) {
            Ok(parsed) => {
                let skill_name = string_value(parsed.get("name"));

                if used_names.contains(&skill_name) {
                    diagnostics.push(json!({
                      "type": "duplicate-skill-name",
                      "message": format!("发现重复 Skill 名称：{}", skill_name),
                      "sourcePath": parsed["sourcePath"]
                    }));
                    continue;
                }

                used_names.insert(skill_name);
                parsed_skills.push(parsed);
            }
            Err(error) => diagnostics.push(json!({
              "type": "metadata-error",
              "message": error.to_string(),
              "sourcePath": scanned_item.skill_root
            })),
        }
    }

    let previous_skill_map = previous_skills
        .iter()
        .map(|item| (string_value(item.get("name")), item.clone()))
        .collect::<HashMap<_, _>>();
    let scanned_skill_names = parsed_skills
        .iter()
        .map(|item| string_value(item.get("name")))
        .collect::<HashSet<_>>();

    for (skill_name, target_ids) in install_index.clone() {
        if target_ids.is_empty() || scanned_skill_names.contains(&skill_name) {
            continue;
        }

        for target_id in target_ids {
            let target_id = string_value(Some(&target_id));

            if let Err(error) = uninstall_skill_link(&cli_targets, &skill_name, &target_id).await {
                diagnostics.push(json!({
                  "type": "cleanup-error",
                  "message": format!("清理失效链接失败：{}", error),
                  "sourcePath": format!("{} -> {}", skill_name, target_id)
                }));
            }
        }

        install_index.remove(&skill_name);
        diagnostics.push(json!({
          "type": "orphan-skill-cleaned",
          "message": format!("Skill 源目录已删除，已自动清理挂载：{}", skill_name),
          "sourcePath": previous_skill_map
            .get(&skill_name)
            .map(|item| string_value(item.get("sourcePath")))
            .unwrap_or(skill_name)
        }));
    }

    parsed_skills.sort_by(|left, right| {
        string_value(left.get("name")).cmp(&string_value(right.get("name")))
    });
    repo_skill_counts.values_mut().for_each(|count| *count = 0);
    let mut skills = Vec::new();

    for mut skill in parsed_skills {
        let mut install_states = serde_json::Map::new();
        let mut installed_targets = Vec::new();
        let skill_name = string_value(skill.get("name"));
        let disabled = previous_skill_map
            .get(&skill_name)
            .and_then(|item| item.get("disabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        for cli_target in &cli_targets {
            let state = get_install_state(&skill, cli_target, disabled).await;

            if matches!(
                state.get("state").and_then(Value::as_str),
                Some("installed") | Some("broken-link")
            ) {
                installed_targets.push(cli_target.get("id").cloned().unwrap_or(Value::Null));
            }

            install_states.insert(string_value(cli_target.get("id")), state);
        }

        let repo_id = string_value(skill.get("repoId"));
        let repo_name = repos
            .iter()
            .find(|repo| repo.get("id").and_then(Value::as_str) == Some(repo_id.as_str()))
            .map(|repo| string_value(repo.get("name")))
            .unwrap_or_else(|| "Managed".to_string());

        if !repo_id.is_empty() && repo_skill_counts.contains_key(&repo_id) {
            *repo_skill_counts.entry(repo_id).or_insert(0) += 1;
        }

        skill["installedTargets"] = json!(installed_targets);
        skill["installStates"] = Value::Object(install_states);
        skill["disabled"] = json!(disabled);
        skill["status"] = json!(resolve_skill_status(disabled, skill.get("installStates")));
        skill["repoName"] = json!(repo_name);
        skills.push(skill);
    }

    let repos = repos
        .into_iter()
        .map(|mut repo| {
            let repo_id = string_value(repo.get("id"));

            repo["skillCount"] = json!(repo_skill_counts.get(&repo_id).cloned().unwrap_or(0));
            repo
        })
        .collect::<Vec<_>>();

    persist_skills(paths, &skills, &cli_targets, install_index).await?;
    state["skills"] = json!(skills);
    state["repos"] = json!(repos);
    merge_diagnostics(state, diagnostics);
    state["refreshedAt"] = json!(now_millis());
    Ok(())
}

pub async fn create_skill(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let skill_name = string_value(payload.get("name"));

    if skill_name.is_empty() {
        return Err(ManagerError::System("Skill 名称不能为空".to_string()));
    }

    if state
        .get("skills")
        .and_then(Value::as_array)
        .is_some_and(|skills| {
            skills
                .iter()
                .any(|skill| skill.get("name").and_then(Value::as_str) == Some(skill_name.as_str()))
        })
    {
        return Err(ManagerError::System(format!(
            "Skill 名称已存在：{}",
            skill_name
        )));
    }

    let directory_name = non_empty_slug(&skill_name, &format!("skill-{}", now_millis()));
    let skill_root = Path::new(&paths.skills_dir).join(directory_name);

    if skill_root.exists() {
        return Err(ManagerError::System(
            "同名目录已存在，请修改 Skill 名称".to_string(),
        ));
    }

    let tags = payload
        .get("tags")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let mut frontmatter_lines = vec![
        "---".to_string(),
        format!("name: {}", yaml_scalar(&skill_name)),
    ];
    let description = string_value(payload.get("description"));
    let author = string_value(payload.get("author"));

    if !description.is_empty() {
        frontmatter_lines.push(format!("description: {}", yaml_scalar(&description)));
    }

    if !author.is_empty() {
        frontmatter_lines.push(format!("author: {}", yaml_scalar(&author)));
    }

    if !tags.is_empty() {
        frontmatter_lines.push("tags:".to_string());
        frontmatter_lines.extend(tags.iter().map(|tag| format!("  - {}", yaml_scalar(tag))));
    }

    frontmatter_lines.extend([
        "entry: prompt.md".to_string(),
        "---".to_string(),
        "".to_string(),
        format!("# {}", skill_name),
        "".to_string(),
        "这个 Skill 由 Monkey Thief 创建。".to_string(),
    ]);

    tokio::fs::create_dir_all(&skill_root).await?;
    tokio::fs::write(
        skill_root.join("SKILL.md"),
        format!("{}\n", frontmatter_lines.join("\n")),
    )
    .await?;
    tokio::fs::write(
        skill_root.join("prompt.md"),
        format!("# {}\n\n在这里补充你的 Skill 提示词。\n", skill_name),
    )
    .await?;
    refresh_skills_state(paths, state).await
}

pub async fn install_skill(state: &Value, payload: Value) -> Result<(), ManagerError> {
    let skill_name = string_value(payload.get("skillName"));
    let target_id = string_value(payload.get("targetId"));
    let skill = find_skill(state, &skill_name)?;

    ensure_skill_enabled(&skill)?;
    install_skill_link(state, &skill, &target_id).await?;
    Ok(())
}

pub async fn batch_skill_action(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let action = string_value(payload.get("action"));
    let skill_names = payload
        .get("skillNames")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let installed_target_ids = cli_targets
        .iter()
        .filter(|item| item.get("installed").and_then(Value::as_bool) == Some(true))
        .map(|item| string_value(item.get("id")))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let selected_target_ids = payload
        .get("targetIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let target_ids = if selected_target_ids.is_empty() {
        installed_target_ids
    } else {
        selected_target_ids
    };
    let mut successes = Vec::new();
    let mut errors = Vec::new();

    if skill_names.is_empty() {
        return Err(ManagerError::System("请选择 Skill".to_string()));
    }
    if action == "install-all" && target_ids.is_empty() {
        return Err(ManagerError::System("请选择要安装到的 CLI".to_string()));
    }

    for skill_name in skill_names {
        let error_count = errors.len();
        let result = match action.as_str() {
            "install-all" => {
                let skill = find_skill(state, &skill_name);
                let skill = skill.and_then(|skill| {
                    ensure_skill_enabled(&skill)?;
                    Ok(skill)
                });
                let skill = match skill {
                    Ok(skill) => skill,
                    Err(error) => {
                        errors.push(json!({
                          "name": skill_name,
                          "message": error.to_string()
                        }));
                        continue;
                    }
                };

                for target_id in &target_ids {
                    if let Err(error) = install_skill_link(state, &skill, target_id).await {
                        errors.push(json!({
                          "name": skill_name,
                          "targetId": target_id,
                          "message": error.to_string()
                        }));
                    }
                }

                Ok(())
            }
            "uninstall-all" => {
                for target_id in &target_ids {
                    if let Err(error) =
                        uninstall_skill_link(&cli_targets, &skill_name, target_id).await
                    {
                        errors.push(json!({
                          "name": skill_name,
                          "targetId": target_id,
                          "message": error.to_string()
                        }));
                    }
                }

                Ok(())
            }
            "enable" => {
                set_skill_enabled(
                    paths,
                    state,
                    json!({
                      "skillName": skill_name,
                      "enabled": true
                    }),
                )
                .await
            }
            "disable" => {
                set_skill_enabled(
                    paths,
                    state,
                    json!({
                      "skillName": skill_name,
                      "enabled": false
                    }),
                )
                .await
            }
            _ => Err(ManagerError::System("不支持的批量操作".to_string())),
        };

        match result {
            Ok(_) if errors.len() == error_count => successes.push(json!({ "name": skill_name })),
            Ok(_) => {}
            Err(error) => errors.push(json!({
              "name": skill_name,
              "message": error.to_string()
            })),
        }
    }

    refresh_skills_state(paths, state).await?;
    Ok(json!({
      "successes": successes,
      "errors": errors
    }))
}

pub(crate) fn load_skill_groups(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    Ok(skill_store::read_groups(paths)?
        .into_iter()
        .map(normalize_skill_group)
        .filter(|item| !string_value(item.get("id")).is_empty())
        .collect())
}

pub async fn save_skill_group(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let group_id = string_value(payload.get("groupId").or_else(|| payload.get("id")));
    let group_name = string_value(payload.get("name"));
    let skill_ids = unique_string_values(payload.get("skillIds"));
    let now = now_millis();
    let mut groups = load_skill_groups(paths)?;

    if group_name.is_empty() {
        return Err(ManagerError::System("分组名称不能为空".to_string()));
    }
    if groups.iter().any(|item| {
        string_value(item.get("name")) == group_name && string_value(item.get("id")) != group_id
    }) {
        return Err(ManagerError::System(format!(
            "Skill 分组已存在：{}",
            group_name
        )));
    }

    let next_group_id = if group_id.is_empty() {
        format!("skill-group-{}", create_uuid_like_id())
    } else {
        group_id
    };
    let mut group = json!({
      "id": next_group_id,
      "name": group_name,
      "skillIds": skill_ids,
      "createdAt": now,
      "updatedAt": now
    });

    remove_skill_ids_from_groups(
        &mut groups,
        group
            .get("skillIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|item| string_value(Some(item)))
            .collect::<Vec<_>>()
            .as_slice(),
        group.get("id").and_then(Value::as_str).unwrap_or_default(),
    );

    if let Some(index) = groups
        .iter()
        .position(|item| {
            item.get("id").and_then(Value::as_str) == group.get("id").and_then(Value::as_str)
        })
    {
        group["createdAt"] = groups[index]
            .get("createdAt")
            .cloned()
            .unwrap_or_else(|| json!(now));
        groups[index] = group.clone();
    } else {
        groups.push(group.clone());
    }

    groups.sort_by(|left, right| string_value(left.get("name")).cmp(&string_value(right.get("name"))));
    skill_store::write_groups(paths, &groups)?;

    Ok(json!({
      "groups": groups,
      "group": group
    }))
}

pub async fn remove_skill_group(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let group_id = string_value(
        payload
            .get("groupId")
            .or_else(|| payload.get("id"))
            .or(Some(&payload)),
    );
    let mut groups = load_skill_groups(paths)?;
    let before_count = groups.len();

    if group_id.is_empty() {
        return Err(ManagerError::System("缺少 Skill 分组 ID".to_string()));
    }

    groups.retain(|item| item.get("id").and_then(Value::as_str) != Some(group_id.as_str()));

    if groups.len() == before_count {
        return Err(ManagerError::System("Skill 分组不存在".to_string()));
    }

    skill_store::write_groups(paths, &groups)?;
    Ok(json!({
      "groups": groups
    }))
}

pub async fn remove_skill_group_items(
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let group_id = string_value(payload.get("groupId"));
    let skill_ids = unique_string_values(payload.get("skillIds"));
    let mut groups = load_skill_groups(paths)?;

    if skill_ids.is_empty() {
        return Err(ManagerError::System("请选择要移出的 Skill".to_string()));
    }

    for group in &mut groups {
        if !group_id.is_empty() && group.get("id").and_then(Value::as_str) != Some(group_id.as_str())
        {
            continue;
        }

        let next_skill_ids = unique_string_values(group.get("skillIds"))
            .into_iter()
            .filter(|item| !skill_ids.contains(item))
            .collect::<Vec<_>>();

        group["skillIds"] = json!(next_skill_ids);
        group["updatedAt"] = json!(now_millis());
    }

    skill_store::write_groups(paths, &groups)?;
    Ok(json!({
      "groups": groups
    }))
}

pub async fn set_skill_enabled(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let skill_name = string_value(payload.get("skillName"));
    let enabled = payload
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| ManagerError::System("缺少 Skill 启用状态".to_string()))?;
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut skills = state
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(index) = skills
        .iter()
        .position(|item| item.get("name").and_then(Value::as_str) == Some(skill_name.as_str()))
    else {
        return Err(ManagerError::System("Skill 不存在".to_string()));
    };

    if !enabled {
        for target_id in skills[index]
            .get("installedTargets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            uninstall_skill_link(&cli_targets, &skill_name, &string_value(Some(&target_id)))
                .await?;
        }
    }

    skills[index]["disabled"] = json!(!enabled);
    skill_store::write_skills(paths, &skills)?;
    refresh_skills_state(paths, state).await
}

pub async fn uninstall_skill(state: &Value, payload: Value) -> Result<(), ManagerError> {
    let skill_name = string_value(payload.get("skillName"));
    let target_id = string_value(payload.get("targetId"));
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    uninstall_skill_link(&cli_targets, &skill_name, &target_id).await
}

pub async fn delete_skills(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    cleanup_expired_skill_trash(paths).await?;

    let skill_names = payload
        .get("skillNames")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut trash_items = read_skill_trash(paths)?;
    let mut deleted_items = Vec::new();
    let mut errors = Vec::new();

    if skill_names.is_empty() {
        return Err(ManagerError::System("请选择要删除的 Skill".to_string()));
    }

    for skill_name in skill_names {
        match trash_skill(paths, state, &cli_targets, &mut trash_items, &skill_name).await {
            Ok(item) => deleted_items.push(item),
            Err(error) => errors.push(json!({
              "name": skill_name,
              "message": error.to_string()
            })),
        }
    }

    write_skill_trash(paths, &trash_items).await?;
    refresh_skills_state(paths, state).await?;

    Ok(json!({
      "deleted": deleted_items,
      "errors": errors,
      "trash": trash_items
    }))
}

pub async fn repair_skill(state: &Value, payload: Value) -> Result<(), ManagerError> {
    let skill_name = string_value(payload.get("skillName"));
    let target_id = string_value(payload.get("targetId"));
    let skill = find_skill(state, &skill_name)?;
    let source_path = string_value(skill.get("sourcePath"));

    ensure_skill_enabled(&skill)?;
    if !Path::new(&source_path).exists() {
        return Err(ManagerError::System(
            "Skill 源目录不存在，当前无法修复".to_string(),
        ));
    }

    install_skill_link(state, &skill, &target_id).await?;
    Ok(())
}

pub async fn import_skill_from_zip(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let source_zip_path = string_value(payload.get("zipPath").or(Some(&payload)));

    if source_zip_path.is_empty() {
        return Err(ManagerError::System("请选择 Skill zip 压缩包".to_string()));
    }

    if Path::new(&source_zip_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip"))
        != Some(true)
    {
        return Err(ManagerError::System("只能导入 zip 压缩包".to_string()));
    }

    if !Path::new(&source_zip_path).exists() {
        return Err(ManagerError::System("zip 压缩包不存在".to_string()));
    }

    let temp_root = create_temp_dir(&paths.temp_dir, "monkey-thief-skill").await?;

    let result = async {
        extract_zip(&source_zip_path, &temp_root).await?;
        let scanned_items = scan_root_collect(&temp_root, Value::Null)?;

        if scanned_items.is_empty() {
            return Err(ManagerError::System(
                "zip 压缩包中未找到 SKILL.md".to_string(),
            ));
        }

        let mut parsed_skills = Vec::new();
        let mut seen_names = HashSet::new();

        for scanned_item in scanned_items {
            let mut parsed = parse_skill(&scanned_item.skill_root, Value::Null)?;
            let skill_name = string_value(parsed.get("name"));

            if seen_names.contains(&skill_name) {
                return Err(ManagerError::System(format!(
                    "zip 压缩包中存在重复 Skill 名称：{}",
                    skill_name
                )));
            }

            seen_names.insert(skill_name);
            parsed["sourcePath"] = json!(scanned_item.skill_root);
            parsed_skills.push(parsed);
        }

        let mut import_items = Vec::new();
        let mut skipped_items = Vec::new();

        for parsed in parsed_skills {
            let skill_name = string_value(parsed.get("name"));
            let source_path = string_value(parsed.get("sourcePath"));
            let directory_name = non_empty_slug(
                &skill_name,
                &Path::new(&source_path)
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("skill-{}", now_millis())),
            );
            let managed_path = Path::new(&paths.skills_dir).join(directory_name);
            let existing_skill = state
                .get("skills")
                .and_then(Value::as_array)
                .and_then(|skills| {
                    skills.iter().find(|item| {
                        item.get("name").and_then(Value::as_str) == Some(skill_name.as_str())
                    })
                });

            if let Some(existing_skill) = existing_skill {
                let incoming_signature = create_skill_signature(&parsed)?;
                let existing_signature = create_skill_signature(existing_skill)?;

                if incoming_signature == existing_signature {
                    skipped_items.push(json!({
                      "name": skill_name,
                      "reason": "same-signature"
                    }));
                    continue;
                }

                return Err(ManagerError::System(format!(
                    "Skill 名称已存在，请先处理同名 Skill：{}",
                    skill_name
                )));
            }

            if managed_path.exists() {
                return Err(ManagerError::System(format!(
                    "集中目录已存在同名目录：{}",
                    path_text(&managed_path)
                )));
            }

            import_items.push((skill_name, PathBuf::from(source_path), managed_path));
        }

        if import_items.is_empty() {
            refresh_skills_state(paths, state).await?;

            return Ok(json!({
              "imported": [],
              "skipped": skipped_items
            }));
        }

        let mut imported_items = Vec::new();

        for (skill_name, source_path, managed_path) in import_items {
            copy_dir_all(&source_path, &managed_path).await?;
            imported_items.push(json!({
              "name": skill_name,
              "sourcePath": path_text(&managed_path)
            }));
        }

        refresh_skills_state(paths, state).await?;
        Ok(json!({
          "imported": imported_items,
          "skipped": skipped_items
        }))
    }
    .await;

    let _ = tokio::fs::remove_dir_all(&temp_root).await;
    result
}

pub async fn preview_skills_from_cli(
    paths: &AppPaths,
    state: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let target_id = string_value(payload.get("targetId").or(Some(&payload)));
    let (imports, mounts) = collect_cli_skill_imports(paths, state, &target_id)?;
    let mut managed_signatures = HashMap::new();
    let mut candidate_groups: HashMap<String, HashMap<String, CandidateGroup>> = HashMap::new();

    for skill in state
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        managed_signatures.insert(
            string_value(skill.get("name")),
            create_skill_signature(&skill)?,
        );
    }

    for candidate in imports.iter().chain(mounts.iter()) {
        let name_groups = candidate_groups.entry(candidate.name.clone()).or_default();
        let group = name_groups
            .entry(candidate.signature.clone())
            .or_insert_with(|| CandidateGroup {
                name: candidate.name.clone(),
                description: candidate.description.clone(),
                signature: candidate.signature.clone(),
                cli_names: Vec::new(),
                source_paths: Vec::new(),
                items: Vec::new(),
            });

        group.cli_names.push(candidate.cli_name.clone());
        group.source_paths.push(candidate.source_path.clone());
        group.items.push(candidate.clone());
    }

    let managed_skills = state
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut candidates = Vec::new();
    let mut conflicts = Vec::new();

    for (name, signature_groups) in candidate_groups {
        let groups = signature_groups.into_values().collect::<Vec<_>>();
        let managed_signature = managed_signatures.get(&name);
        let managed_skill = managed_skills
            .iter()
            .find(|item| item.get("name").and_then(Value::as_str) == Some(name.as_str()));
        let managed_groups = groups
            .iter()
            .filter(|group| managed_signature == Some(&group.signature))
            .cloned()
            .collect::<Vec<_>>();
        let new_groups = groups
            .iter()
            .filter(|group| managed_signature != Some(&group.signature))
            .cloned()
            .collect::<Vec<_>>();

        for group in managed_groups {
            candidates.push(group.to_value(true));
        }

        if new_groups.is_empty() {
            continue;
        }

        if let Some(managed_skill) = managed_skill {
            let mut options = vec![json!({
              "id": format!("managed:{}", string_value(managed_skill.get("sourcePath"))),
              "name": managed_skill["name"],
              "description": managed_skill["description"],
              "signature": managed_signature.cloned().unwrap_or_default(),
              "cliNames": ["Monkey Thief"],
              "sourcePaths": [managed_skill["sourcePath"].clone()],
              "alreadyManaged": true
            })];

            options.extend(new_groups.into_iter().map(|group| group.to_value(true)));
            conflicts.push(json!({
              "name": name,
              "options": options
            }));
            continue;
        }

        if new_groups.len() == 1 {
            candidates.push(new_groups[0].to_value(false));
            continue;
        }

        conflicts.push(json!({
          "name": name,
          "options": new_groups
            .into_iter()
            .map(|group| group.to_value(managed_signatures.contains_key(&name)))
            .collect::<Vec<_>>()
        }));
    }

    candidates.sort_by(|left, right| {
        string_value(left.get("name")).cmp(&string_value(right.get("name")))
    });
    conflicts.sort_by(|left, right| {
        string_value(left.get("name")).cmp(&string_value(right.get("name")))
    });

    Ok(json!({
      "candidates": candidates,
      "conflicts": conflicts
    }))
}

pub async fn import_skills_from_cli(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let target_id = string_value(payload.get("targetId"));
    let (imports, mounts) = collect_cli_skill_imports(paths, state, &target_id)?;
    let all_candidates = imports
        .iter()
        .chain(mounts.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut selected_sources = HashSet::new();
    let mut replacement_sources = HashMap::new();

    if let Some(items) = payload.as_array() {
        for name in items.iter().map(|item| string_value(Some(item))) {
            for candidate in all_candidates.iter().filter(|item| item.name == name) {
                selected_sources.insert(candidate.source_path.clone());
            }
        }
    } else if payload.get("sourcePaths").is_some() {
        for source_path in payload
            .get("sourcePaths")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            selected_sources.insert(string_value(Some(&source_path)));
        }

        for choice in payload
            .get("choices")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let choice_id = string_value(choice.get("id"));
            let choice_name = string_value(choice.get("name"));

            if choice_id.starts_with("managed:") {
                for candidate in mounts.iter().filter(|item| item.name == choice_name) {
                    selected_sources.insert(candidate.source_path.clone());
                }
                continue;
            }

            let source_paths = choice
                .get("sourcePaths")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![json!(choice_id)]);

            for source_path in source_paths {
                let source_path = string_value(Some(&source_path));

                if let Some(selected) = all_candidates
                    .iter()
                    .find(|item| item.source_path == source_path)
                {
                    selected_sources.insert(selected.source_path.clone());
                    replacement_sources.insert(selected.name.clone(), selected.source_path.clone());
                }
            }
        }
    } else {
        for candidate in &all_candidates {
            selected_sources.insert(candidate.source_path.clone());
        }
    }

    let selected_imports = imports
        .iter()
        .filter(|item| selected_sources.contains(&item.source_path))
        .cloned()
        .collect::<Vec<_>>();
    let selected_mounts = mounts
        .iter()
        .filter(|item| selected_sources.contains(&item.source_path))
        .cloned()
        .collect::<Vec<_>>();

    if selected_imports.is_empty() && selected_mounts.is_empty() {
        refresh_skills_state(paths, state).await?;
        return Ok(());
    }

    for (_skill_name, source_path) in replacement_sources {
        if let Some(source) = all_candidates
            .iter()
            .find(|item| item.source_path == source_path)
        {
            remove_dir_all_if_exists(&source.managed_path).await?;
            copy_dir_all(
                Path::new(&source.source_path),
                Path::new(&source.managed_path),
            )
            .await?;
        }
    }

    for candidate in &selected_imports {
        copy_dir_all(
            Path::new(&candidate.source_path),
            Path::new(&candidate.managed_path),
        )
        .await?;
    }

    for candidate in selected_imports.iter().chain(selected_mounts.iter()) {
        remove_dir_all_if_exists(&candidate.source_path).await?;

        if !same_path(
            Path::new(&candidate.source_path),
            Path::new(&candidate.mounted_path),
        ) {
            remove_dir_all_if_exists(&candidate.mounted_path).await?;
        }

        create_junction(
            Path::new(&candidate.managed_path),
            Path::new(&candidate.mounted_path),
        )
        .await?;
    }

    refresh_skills_state(paths, state).await
}

pub async fn add_skill_repository(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let github = parse_github_source(&string_value(payload.get("source")))?;
    let branch = first_non_empty(&[string_value(payload.get("branch")), github.branch.clone()]);
    let name = first_non_empty(&[
        string_value(payload.get("name")),
        format!("{}/{}", github.owner, github.repository),
    ]);
    let now = now_millis();
    let mut repository = json!({
      "id": format!("skill-repo-{}", create_uuid_like_id()),
      "type": "github",
      "name": name,
      "source": github.source,
      "owner": github.owner,
      "repository": github.repository,
      "branch": branch,
      "rootPath": github.root_path,
      "htmlUrl": github.html_url,
      "status": "ready",
      "skills": [],
      "error": "",
      "createdAt": now,
      "updatedAt": now,
      "lastSyncedAt": 0
    });

    match scan_repository(paths, &repository).await {
        Ok(scanned) => {
            repository["branch"] = scanned["branch"].clone();
            repository["skills"] = scanned["skills"].clone();
            repository["status"] = json!("ready");
            repository["error"] = json!("");
        }
        Err(error) => {
            repository["status"] = json!("error");
            repository["error"] = json!(error.to_string());
        }
    }

    repository["lastSyncedAt"] = json!(now_millis());
    repository["updatedAt"] = json!(now_millis());

    let mut repositories = load_repositories(paths)?;

    repositories.insert(0, repository);
    persist_repositories(paths, &repositories).await?;

    Ok(repository_state_patch(&repositories))
}

pub async fn refresh_skill_repository(
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repository_id = string_value(payload.get("repositoryId").or(Some(&payload)));
    let mut repositories = load_repositories(paths)?;
    let Some(index) = repositories
        .iter()
        .position(|item| item.get("id").and_then(Value::as_str) == Some(repository_id.as_str()))
    else {
        return Err(ManagerError::System("Skill 仓库不存在".to_string()));
    };

    match scan_repository(paths, &repositories[index]).await {
        Ok(scanned) => {
            repositories[index]["branch"] = scanned["branch"].clone();
            repositories[index]["skills"] = scanned["skills"].clone();
            repositories[index]["status"] = json!("ready");
            repositories[index]["error"] = json!("");
        }
        Err(error) => {
            repositories[index]["status"] = json!("error");
            repositories[index]["skills"] = json!([]);
            repositories[index]["error"] = json!(error.to_string());
        }
    }

    repositories[index]["lastSyncedAt"] = json!(now_millis());
    repositories[index]["updatedAt"] = json!(now_millis());
    persist_repositories(paths, &repositories).await?;

    Ok(repository_state_patch(&repositories))
}

pub async fn remove_skill_repository(
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repository_id = string_value(payload.get("repositoryId").or(Some(&payload)));
    let repositories = load_repositories(paths)?
        .into_iter()
        .filter(|item| item.get("id").and_then(Value::as_str) != Some(repository_id.as_str()))
        .collect::<Vec<_>>();

    persist_repositories(paths, &repositories).await?;
    Ok(repository_state_patch(&repositories))
}

pub async fn install_skill_from_repository(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let repository_id = string_value(payload.get("repositoryId"));
    let skill_id = string_value(payload.get("skillId"));
    let repositories = load_repositories(paths)?;
    let Some(repository) = repositories
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(repository_id.as_str()))
    else {
        return Err(ManagerError::System("Skill 仓库不存在".to_string()));
    };
    let Some(skill) = repository
        .get("skills")
        .and_then(Value::as_array)
        .and_then(|skills| {
            skills
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(skill_id.as_str()))
        })
    else {
        return Err(ManagerError::System("仓库 Skill 不存在".to_string()));
    };
    let skill_name = string_value(skill.get("name"));

    if let Some(existing_skill) = state
        .get("skills")
        .and_then(Value::as_array)
        .and_then(|skills| {
            skills
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some(skill_name.as_str()))
        })
    {
        ensure_skill_enabled(existing_skill)?;

        return Err(ManagerError::System(format!(
            "Skill 名称已存在：{}",
            skill_name
        )));
    }

    let skill_path = string_value(skill.get("skillPath"));
    let directory_name = non_empty_slug(
        &skill_name,
        posix_basename(&skill_path)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "skill".to_string())
            .as_str(),
    );
    let managed_path = Path::new(&paths.skills_dir).join(directory_name);

    if managed_path.exists() {
        return Err(ManagerError::System(format!(
            "集中目录已存在同名目录：{}",
            path_text(&managed_path)
        )));
    }

    let archive = download_repository_archive(repository, paths, true).await?;
    let install_result = async {
        let source_path = resolve_archive_skill_source_dir(&archive.content_dir, &skill_path)?;

        if !source_path.join("SKILL.md").is_file() {
            return Err(ManagerError::System(
                "仓库 Skill 目录下没有 SKILL.md".to_string(),
            ));
        }

        copy_dir_all(&source_path, &managed_path).await
    }
    .await;
    let cleanup = remove_dir_all_if_exists(&archive.temp_root).await;

    if install_result.is_err() {
        let _ = remove_dir_all_if_exists(&managed_path).await;
    }

    install_result?;
    cleanup?;

    refresh_skills_state(paths, state).await
}

pub fn get_skill_files(state: &Value, payload: Value) -> Result<Value, ManagerError> {
    let skill_name = string_value(payload.get("skillName"));
    let skill = find_skill(state, &skill_name)?;
    let source_path = string_value(skill.get("sourcePath"));

    if !Path::new(&source_path).exists() {
        return Err(ManagerError::System("Skill 源目录不存在".to_string()));
    }

    Ok(json!({
      "sourcePath": source_path,
      "entries": collect_skill_view_entries(Path::new(&source_path))?
    }))
}

pub async fn get_skill_trash(paths: &AppPaths) -> Result<Value, ManagerError> {
    cleanup_expired_skill_trash(paths).await?;

    Ok(json!({
      "items": read_skill_trash(paths)?
    }))
}

pub async fn restore_skill_trash(
    paths: &AppPaths,
    state: &mut Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    cleanup_expired_skill_trash(paths).await?;

    let item_ids = payload_item_ids(payload);
    let mut trash_items = read_skill_trash(paths)?;
    let mut restored_items = Vec::new();
    let mut errors = Vec::new();

    if item_ids.is_empty() {
        return Err(ManagerError::System("请选择要恢复的 Skill".to_string()));
    }

    for item_id in item_ids {
        let Some(index) = trash_items
            .iter()
            .position(|item| item.get("id").and_then(Value::as_str) == Some(item_id.as_str()))
        else {
            errors.push(json!({
              "id": item_id,
              "message": "回收站项目不存在"
            }));
            continue;
        };

        match restore_skill_trash_item(paths, &trash_items[index]).await {
            Ok(restored) => {
                trash_items.remove(index);
                restored_items.push(restored);
            }
            Err(error) => errors.push(json!({
              "id": item_id,
              "message": error.to_string()
            })),
        }
    }

    write_skill_trash(paths, &trash_items).await?;
    refresh_skills_state(paths, state).await?;

    Ok(json!({
      "restored": restored_items,
      "errors": errors,
      "trash": trash_items
    }))
}

pub async fn purge_skill_trash(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    cleanup_expired_skill_trash(paths).await?;

    let item_ids = payload_item_ids(payload);
    let mut trash_items = read_skill_trash(paths)?;
    let mut purged_items = Vec::new();
    let mut errors = Vec::new();

    if item_ids.is_empty() {
        return Err(ManagerError::System("请选择要永久删除的 Skill".to_string()));
    }

    for item_id in item_ids {
        let Some(index) = trash_items
            .iter()
            .position(|item| item.get("id").and_then(Value::as_str) == Some(item_id.as_str()))
        else {
            errors.push(json!({
              "id": item_id,
              "message": "回收站项目不存在"
            }));
            continue;
        };
        let item = trash_items.remove(index);

        match remove_skill_trash_path(paths, &item).await {
            Ok(_) => purged_items.push(json!({
              "id": item["id"],
              "name": item["name"]
            })),
            Err(error) => errors.push(json!({
              "id": item_id,
              "message": error.to_string()
            })),
        }
    }

    write_skill_trash(paths, &trash_items).await?;

    Ok(json!({
      "purged": purged_items,
      "errors": errors,
      "trash": trash_items
    }))
}

struct ScannedSkill {
    skill_root: String,
    repo_id: Value,
}

#[derive(Clone)]
struct CliSkillCandidate {
    name: String,
    description: String,
    cli_name: String,
    source_path: String,
    managed_path: String,
    mounted_path: String,
    signature: String,
}

#[derive(Clone)]
struct CandidateGroup {
    name: String,
    description: String,
    signature: String,
    cli_names: Vec<String>,
    source_paths: Vec<String>,
    items: Vec<CliSkillCandidate>,
}

impl CandidateGroup {
    fn to_value(&self, already_managed: bool) -> Value {
        json!({
          "id": self.source_paths.first().cloned().unwrap_or_default(),
          "name": self.name,
          "description": self.description,
          "signature": self.signature,
          "cliNames": self.cli_names,
          "sourcePaths": self.source_paths,
          "alreadyManaged": already_managed
        })
    }
}

struct GitHubSource {
    owner: String,
    repository: String,
    branch: String,
    root_path: String,
    source: String,
    html_url: String,
}

struct RepositoryArchive {
    branch: String,
    temp_root: PathBuf,
    content_dir: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepositoryArchiveZipSource {
    Cache,
    Download,
}

fn scan_many(items: Vec<(String, Value)>) -> Result<Vec<ScannedSkill>, ManagerError> {
    let mut results = Vec::new();

    for (root_path, repo_id) in items {
        if !Path::new(&root_path).is_dir() {
            continue;
        }

        scan_root(Path::new(&root_path), &root_path, &repo_id, 0, &mut results)?;
    }

    Ok(results)
}

fn scan_root_collect(root_path: &Path, repo_id: Value) -> Result<Vec<ScannedSkill>, ManagerError> {
    let mut results = Vec::new();

    scan_root(root_path, &path_text(root_path), &repo_id, 0, &mut results)?;
    Ok(results)
}

fn scan_root(
    current_path: &Path,
    root_path: &str,
    repo_id: &Value,
    depth: usize,
    results: &mut Vec<ScannedSkill>,
) -> Result<(), ManagerError> {
    if depth > 6 {
        return Ok(());
    }

    let skill_manifest = current_path.join("SKILL.md");

    if skill_manifest.is_file() {
        results.push(ScannedSkill {
            skill_root: path_text(current_path),
            repo_id: repo_id.clone(),
        });
        return Ok(());
    }

    for entry in std::fs::read_dir(current_path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();

        if !file_type.is_dir() || IGNORE_DIRS.contains(&name.as_str()) {
            continue;
        }

        let metadata = std::fs::symlink_metadata(entry.path())?;

        if metadata.file_type().is_symlink() {
            continue;
        }

        scan_root(&entry.path(), root_path, repo_id, depth + 1, results)?;
    }

    let _ = root_path;
    Ok(())
}

fn parse_skill(skill_root: &str, repo_id: Value) -> Result<Value, ManagerError> {
    let skill_file = Path::new(skill_root).join("SKILL.md");
    let content = std::fs::read_to_string(&skill_file)?;
    let (metadata, body) = parse_frontmatter(&content);
    let skill_name = metadata.get("name").cloned().unwrap_or_default();

    if skill_name.is_empty() {
        return Err(ManagerError::System(format!(
            "Missing required frontmatter field \"name\" in {}",
            path_text(&skill_file)
        )));
    }

    let entry_value = metadata
        .get("entry")
        .cloned()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "SKILL.md".to_string());
    let entry_path = Path::new(skill_root).join(&entry_value);
    let icon_candidate = metadata
        .get("icon")
        .map(|value| Path::new(skill_root).join(value));
    let fallback_icon = Path::new(skill_root).join("icon.png");
    let icon_path = icon_candidate
        .filter(|path| path.exists())
        .or_else(|| fallback_icon.exists().then_some(fallback_icon));
    let stat = std::fs::metadata(skill_root)?;

    Ok(json!({
      "id": sha1_hex(skill_root)[..16].to_string(),
      "name": skill_name,
      "description": metadata.get("description").cloned().unwrap_or_default(),
      "content": body.trim(),
      "version": metadata.get("version").cloned().unwrap_or_default(),
      "author": metadata.get("author").cloned().unwrap_or_default(),
      "tags": metadata
        .get("tags")
        .map(|value| parse_tags(value))
        .unwrap_or_default(),
      "icon": icon_path.map(path_text),
      "entry": entry_value,
      "entryPath": if entry_path.exists() { path_text(entry_path) } else { path_text(skill_file) },
      "homepage": metadata.get("homepage").cloned().unwrap_or_default(),
      "repository": metadata.get("repository").cloned().unwrap_or_default(),
      "repoId": repo_id,
      "sourcePath": skill_root,
      "installedTargets": [],
      "createdAt": file_time_millis(stat.created().ok()),
      "updatedAt": file_time_millis(stat.modified().ok())
    }))
}

fn parse_github_source(value: &str) -> Result<GitHubSource, ManagerError> {
    let mut source = value.trim().to_string();

    if source.is_empty() {
        return Err(ManagerError::System("仓库地址不能为空".to_string()));
    }

    if !source.contains("://") && !source.starts_with("github.com/") {
        source = format!("github.com/{}", source);
    }

    let url = if source.starts_with("http://") || source.starts_with("https://") {
        Url::parse(&source)
    } else {
        Url::parse(&format!("https://{}", source))
    }
    .map_err(|_| ManagerError::System("GitHub 仓库地址格式不正确".to_string()))?;

    if url.host_str() != Some("github.com") {
        return Err(ManagerError::System(
            "当前只支持 GitHub 仓库地址".to_string(),
        ));
    }

    let segments = url
        .path()
        .split('/')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let owner = segments.first().copied().unwrap_or("").to_string();
    let repository = segments
        .get(1)
        .copied()
        .unwrap_or("")
        .trim_end_matches(".git")
        .to_string();

    if owner.is_empty() || repository.is_empty() {
        return Err(ManagerError::System(
            "GitHub 仓库地址格式不正确".to_string(),
        ));
    }

    let mut branch = String::new();
    let mut root_path = String::new();

    if segments.get(2) == Some(&"tree") {
        branch = segments.get(3).copied().unwrap_or("").to_string();
        root_path = segments.get(4..).unwrap_or(&[]).join("/");
    }

    if segments.get(2) == Some(&"blob") {
        branch = segments.get(3).copied().unwrap_or("").to_string();
        let file_path = segments.get(4..).unwrap_or(&[]).join("/");

        root_path = if posix_basename(&file_path).as_deref() == Some("SKILL.md") {
            posix_dirname(&file_path)
        } else {
            file_path
        };

        if root_path == "." {
            root_path.clear();
        }
    }

    Ok(GitHubSource {
        owner: owner.clone(),
        repository: repository.clone(),
        branch,
        root_path: normalize_repository_path(&root_path),
        source: url.as_str().trim_end_matches('/').to_string(),
        html_url: format!("https://github.com/{}/{}", owner, repository),
    })
}

async fn scan_repository(paths: &AppPaths, repository: &Value) -> Result<Value, ManagerError> {
    let archive = download_repository_archive(repository, paths, false).await?;
    let result = scan_repository_archive(repository, &archive);
    let cleanup = remove_dir_all_if_exists(&archive.temp_root).await;

    match (result, cleanup) {
        (Ok(scanned), Ok(_)) => Ok(scanned),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn scan_repository_archive(
    repository: &Value,
    archive: &RepositoryArchive,
) -> Result<Value, ManagerError> {
    let root_path = normalize_repository_path(&string_value(repository.get("rootPath")));
    let scan_root = resolve_archive_skill_source_dir(&archive.content_dir, &root_path)?;
    let scanned_items = scan_root_collect(&scan_root, Value::Null)?;
    let mut skills = Vec::new();

    for scanned_item in scanned_items {
        let parsed = parse_skill(&scanned_item.skill_root, Value::Null)?;
        let skill_path =
            archive_relative_path(&archive.content_dir, Path::new(&scanned_item.skill_root))?;
        let display_path = if root_path.is_empty() {
            if skill_path.is_empty() {
                ".".to_string()
            } else {
                skill_path.clone()
            }
        } else {
            let relative = posix_relative(&root_path, &skill_path);

            if relative.is_empty() {
                ".".to_string()
            } else {
                relative
            }
        };
        let skill_file_path = if skill_path.is_empty() {
            "SKILL.md".to_string()
        } else {
            format!("{}/SKILL.md", skill_path)
        };

        skills.push(json!({
          "id": sha1_hex(&format!(
            "{}:{}:{}",
            string_value(repository.get("source")),
            archive.branch,
            skill_path
          ))[..16].to_string(),
          "name": parsed["name"],
          "description": parsed["description"],
          "content": parsed["content"],
          "version": parsed["version"],
          "author": parsed["author"],
          "tags": parsed["tags"],
          "entry": parsed["entry"],
          "homepage": parsed["homepage"],
          "repository": parsed["repository"],
          "readmeUrl": create_github_blob_url(repository, &archive.branch, &skill_file_path),
          "skillPath": skill_path,
          "displayPath": display_path,
          "updatedAt": now_millis()
        }));
    }

    skills.sort_by(|left, right| {
        string_value(left.get("name")).cmp(&string_value(right.get("name")))
    });

    Ok(json!({
      "branch": archive.branch,
      "skills": skills
    }))
}

async fn download_repository_archive(
    repository: &Value,
    paths: &AppPaths,
    use_cache: bool,
) -> Result<RepositoryArchive, ManagerError> {
    let temp_root = create_temp_dir(&paths.temp_dir, "skill-repo-archive").await?;
    let zip_path = temp_root.join("repository.zip");
    let extract_dir = temp_root.join("extract");
    let branches = repository_branch_candidates(repository);
    let mut errors = Vec::new();

    tokio::fs::create_dir_all(&extract_dir).await?;

    for branch in branches {
        let cache_path = repository_archive_cache_path(paths, repository, &branch);

        match load_repository_archive_zip(repository, &branch, &zip_path, &cache_path, use_cache)
            .await
        {
            Ok(source) => {
                let extract_result = extract_repository_archive(&zip_path, &extract_dir);
                let extract_result = if extract_result.is_err()
                    && source == RepositoryArchiveZipSource::Cache
                {
                    let _ = tokio::fs::remove_file(&cache_path).await;
                    let _ = tokio::fs::remove_file(&zip_path).await;
                    remove_dir_all_if_exists(&extract_dir).await?;
                    tokio::fs::create_dir_all(&extract_dir).await?;
                    load_repository_archive_zip(repository, &branch, &zip_path, &cache_path, false)
                        .await
                        .and_then(|_| extract_repository_archive(&zip_path, &extract_dir))
                } else {
                    extract_result
                };

                if let Err(error) = extract_result {
                    errors.push(format!("{}: {}", branch, error));
                    let _ = tokio::fs::remove_file(&cache_path).await;
                    let _ = tokio::fs::remove_file(&zip_path).await;
                    remove_dir_all_if_exists(&extract_dir).await?;
                    tokio::fs::create_dir_all(&extract_dir).await?;
                    continue;
                }

                let content_dir = resolve_archive_content_dir(&extract_dir)?;

                return Ok(RepositoryArchive {
                    branch,
                    temp_root,
                    content_dir,
                });
            }
            Err(error) => errors.push(format!("{}: {}", branch, error)),
        }
    }

    let _ = remove_dir_all_if_exists(&temp_root).await;
    Err(ManagerError::System(format!(
        "GitHub 仓库 archive 下载失败：{}",
        errors.join("；")
    )))
}

async fn load_repository_archive_zip(
    repository: &Value,
    branch: &str,
    zip_path: &Path,
    cache_path: &Path,
    use_cache: bool,
) -> Result<RepositoryArchiveZipSource, ManagerError> {
    if use_cache && try_load_cached_repository_archive_zip(cache_path, zip_path).await? {
        return Ok(RepositoryArchiveZipSource::Cache);
    }

    download_repository_archive_zip(repository, branch, zip_path).await?;

    if !repository_archive_zip_is_valid(zip_path) {
        return Err(ManagerError::System(
            "GitHub 仓库 archive 不是有效 zip 文件".to_string(),
        ));
    }

    if let Some(parent) = cache_path.parent() {
        if tokio::fs::create_dir_all(parent).await.is_ok() {
            let _ = tokio::fs::copy(zip_path, cache_path).await;
        }
    }

    Ok(RepositoryArchiveZipSource::Download)
}

async fn try_load_cached_repository_archive_zip(
    cache_path: &Path,
    zip_path: &Path,
) -> Result<bool, ManagerError> {
    if !cache_path.is_file() {
        return Ok(false);
    }

    if copy_cached_repository_archive_zip(cache_path, zip_path)
        .await
        .is_err()
    {
        let _ = tokio::fs::remove_file(cache_path).await;
        return Ok(false);
    }

    if repository_archive_zip_is_valid(zip_path) {
        return Ok(true);
    }

    let _ = tokio::fs::remove_file(cache_path).await;
    let _ = tokio::fs::remove_file(zip_path).await;
    Ok(false)
}

async fn copy_cached_repository_archive_zip(
    cache_path: &Path,
    zip_path: &Path,
) -> Result<(), ManagerError> {
    if let Some(parent) = zip_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::copy(cache_path, zip_path).await?;
    Ok(())
}

async fn download_repository_archive_zip(
    repository: &Value,
    branch: &str,
    zip_path: &Path,
) -> Result<(), ManagerError> {
    let url = create_repository_archive_url(repository, branch);
    let response = github_download_client()?
        .get(&url)
        .header("User-Agent", "Monkey-Thief")
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let status = response.status();

    if !status.is_success() {
        return Err(ManagerError::System(format!(
            "GitHub 下载失败：{} {}",
            status.as_u16(),
            url
        )));
    }

    tokio::fs::write(
        zip_path,
        response
            .bytes()
            .await
            .map_err(|error| ManagerError::System(error.to_string()))?,
    )
    .await?;
    Ok(())
}

fn repository_archive_zip_is_valid(zip_path: &Path) -> bool {
    File::open(zip_path)
        .ok()
        .and_then(|file| zip::ZipArchive::new(file).ok())
        .is_some()
}

fn github_download_client() -> Result<reqwest::Client, ManagerError> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(10));

    if environment_proxy_points_to_local_app_proxy() {
        builder = builder.no_proxy();
    }

    builder
        .build()
        .map_err(|error| ManagerError::System(error.to_string()))
}

fn extract_repository_archive(zip_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    std::fs::create_dir_all(target_path)?;

    let file = File::open(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ManagerError::System(error.to_string()))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ManagerError::System(error.to_string()))?;
        let Some(enclosed_name) = entry.enclosed_name() else {
            return Err(ManagerError::System(
                "GitHub 仓库压缩包包含不安全路径".to_string(),
            ));
        };
        let target = target_path.join(enclosed_name);

        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut output = File::create(&target)?;
        io::copy(&mut entry, &mut output)?;
    }

    Ok(())
}

fn resolve_archive_content_dir(extract_dir: &Path) -> Result<PathBuf, ManagerError> {
    let mut dirs = std::fs::read_dir(extract_dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let stat = std::fs::symlink_metadata(&path).ok()?;

            (stat.is_dir() && !stat.file_type().is_symlink()).then_some(path)
        })
        .collect::<Vec<_>>();

    dirs.sort();

    if dirs.len() == 1 {
        Ok(dirs.remove(0))
    } else {
        Ok(extract_dir.to_path_buf())
    }
}

fn resolve_archive_skill_source_dir(
    archive_root: &Path,
    skill_path: &str,
) -> Result<PathBuf, ManagerError> {
    let source_path = join_safe_archive_path(archive_root, skill_path)?;

    if !source_path.join("SKILL.md").is_file() && !skill_path.is_empty() && !source_path.is_dir() {
        return Err(ManagerError::System("仓库链接下的目录不存在".to_string()));
    }

    if !source_path.is_dir() {
        return Err(ManagerError::System("仓库链接下的目录不存在".to_string()));
    }

    Ok(source_path)
}

fn join_safe_archive_path(root_path: &Path, relative_path: &str) -> Result<PathBuf, ManagerError> {
    let normalized_path = validate_archive_relative_path(relative_path)?;
    let mut target_path = root_path.to_path_buf();

    for segment in normalized_path.split('/').filter(|item| !item.is_empty()) {
        target_path.push(segment);
    }

    let root_canonical = std::fs::canonicalize(root_path)?;
    let target_canonical = std::fs::canonicalize(&target_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ManagerError::System("仓库链接下的目录不存在".to_string())
        } else {
            ManagerError::Io(error)
        }
    })?;

    if !target_canonical.starts_with(&root_canonical) {
        return Err(ManagerError::System(
            "仓库 Skill 目录路径不安全".to_string(),
        ));
    }

    Ok(target_canonical)
}

fn validate_archive_relative_path(value: &str) -> Result<String, ManagerError> {
    let normalized_path = normalize_repository_path(value);

    if normalized_path.is_empty() {
        return Ok(String::new());
    }

    if Path::new(&normalized_path).is_absolute() {
        return Err(ManagerError::System(
            "仓库 Skill 目录路径不安全".to_string(),
        ));
    }

    if normalized_path
        .split('/')
        .any(|item| item.is_empty() || item == "." || item == "..")
    {
        return Err(ManagerError::System(
            "仓库 Skill 目录路径不安全".to_string(),
        ));
    }

    Ok(normalized_path)
}

fn archive_relative_path(root_path: &Path, target_path: &Path) -> Result<String, ManagerError> {
    let root_path = std::fs::canonicalize(root_path)?;
    let target_path = std::fs::canonicalize(target_path)?;

    target_path
        .strip_prefix(root_path)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .map_err(|_| ManagerError::System("仓库 Skill 目录路径不安全".to_string()))
}

fn repository_branch_candidates(repository: &Value) -> Vec<String> {
    let mut branches = Vec::new();

    for branch in [
        string_value(repository.get("branch")),
        "main".to_string(),
        "master".to_string(),
    ] {
        if !branch.is_empty() && !branches.contains(&branch) {
            branches.push(branch);
        }
    }

    branches
}

fn create_repository_archive_url(repository: &Value, branch: &str) -> String {
    format!(
        "https://github.com/{}/{}/archive/refs/heads/{}.zip",
        string_value(repository.get("owner")),
        string_value(repository.get("repository")),
        normalize_path_segment(branch)
    )
}

fn repository_archive_cache_path(paths: &AppPaths, repository: &Value, branch: &str) -> PathBuf {
    let cache_key = sha1_hex(&format!(
        "{}:{}:{}",
        string_value(repository.get("owner")).to_lowercase(),
        string_value(repository.get("repository")).to_lowercase(),
        branch
    ));

    Path::new(&paths.temp_dir)
        .join("skill-repository-archives")
        .join(format!("{}.zip", cache_key))
}

fn create_github_blob_url(repository: &Value, branch: &str, file_path: &str) -> String {
    format!(
        "https://github.com/{}/{}/blob/{}/{}",
        string_value(repository.get("owner")),
        string_value(repository.get("repository")),
        normalize_path_segment(branch),
        normalize_path_segment(file_path)
    )
}

fn environment_proxy_points_to_local_app_proxy() -> bool {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .filter_map(|key| std::env::var(key).ok())
    .any(|value| proxy_points_to_local_app_proxy(&value))
}

fn proxy_points_to_local_app_proxy(value: &str) -> bool {
    let proxy = value.trim();

    if proxy.is_empty() {
        return false;
    }

    let parsed = if proxy.contains("://") {
        Url::parse(proxy)
    } else {
        Url::parse(&format!("http://{}", proxy))
    };
    let Ok(url) = parsed else {
        return false;
    };
    let host = url.host_str().unwrap_or("");
    let port = url.port().unwrap_or(0);

    matches!(host, "127.0.0.1" | "localhost" | "::1") && matches!(port, 15721 | 15722)
}

fn parse_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let mut metadata = HashMap::new();
    let mut lines = content.lines();
    let mut body = content.to_string();

    if lines.next() != Some("---") {
        return (metadata, body);
    }

    let mut frontmatter = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_frontmatter = true;

    for line in lines {
        if in_frontmatter && line.trim() == "---" {
            in_frontmatter = false;
            continue;
        }

        if in_frontmatter {
            frontmatter.push(line.to_string());
        } else {
            body_lines.push(line.to_string());
        }
    }

    if in_frontmatter {
        return (metadata, body);
    }

    let mut index = 0;

    while index < frontmatter.len() {
        let line = &frontmatter[index];
        let Some(separator) = line.find(':') else {
            index += 1;
            continue;
        };
        let key = line[..separator].trim().to_string();
        let value = line[separator + 1..].trim();

        if value.is_empty() && key == "tags" {
            let mut tags = Vec::new();
            index += 1;

            while index < frontmatter.len() {
                let item = frontmatter[index].trim();

                if !item.starts_with('-') {
                    break;
                }

                tags.push(unquote_yaml(item.trim_start_matches('-').trim()));
                index += 1;
            }

            metadata.insert(key, tags.join("\n"));
            continue;
        }

        metadata.insert(key, unquote_yaml(value));
        index += 1;
    }

    body = body_lines.join("\n");
    (metadata, body)
}

fn parse_tags(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn unquote_yaml(value: &str) -> String {
    let text = value.trim();

    if text.starts_with('"') && text.ends_with('"') {
        serde_json::from_str::<String>(text).unwrap_or_else(|_| text.trim_matches('"').to_string())
    } else {
        text.to_string()
    }
}

fn collect_cli_skill_imports(
    paths: &AppPaths,
    state: &Value,
    target_id: &str,
) -> Result<(Vec<CliSkillCandidate>, Vec<CliSkillCandidate>), ManagerError> {
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            (target_id.is_empty() || item.get("id").and_then(Value::as_str) == Some(target_id))
                && item.get("installed").and_then(Value::as_bool) == Some(true)
                && !string_value(item.get("skillsPath")).is_empty()
        })
        .collect::<Vec<_>>();
    let mut managed_paths = state
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            (
                string_value(item.get("name")),
                string_value(item.get("sourcePath")),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut imports = Vec::new();
    let mut mounts = Vec::new();

    for cli_target in cli_targets {
        let skills_path = PathBuf::from(string_value(cli_target.get("skillsPath")));

        if !skills_path.is_dir() {
            continue;
        }

        for entry in std::fs::read_dir(&skills_path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;

            if !file_type.is_dir() {
                continue;
            }

            let source_path = entry.path();
            let source_stat = std::fs::symlink_metadata(&source_path)?;

            if source_stat.file_type().is_symlink() || !source_path.join("SKILL.md").exists() {
                continue;
            }

            let parsed = parse_skill(&path_text(&source_path), Value::Null)?;
            let parsed_name = string_value(parsed.get("name"));
            let directory_name = non_empty_slug(&parsed_name, &entry.file_name().to_string_lossy());
            let managed_path = managed_paths
                .get(&parsed_name)
                .cloned()
                .unwrap_or_else(|| path_text(Path::new("")));
            let managed_path = if managed_path.is_empty() {
                path_text(Path::new(&paths.skills_dir).join(directory_name))
            } else {
                managed_path
            };
            let mounted_path = path_text(skills_path.join(&parsed_name));
            let candidate = CliSkillCandidate {
                name: parsed_name.clone(),
                description: string_value(parsed.get("description")),
                cli_name: string_value(cli_target.get("name")),
                source_path: path_text(&source_path),
                managed_path: managed_path.clone(),
                mounted_path,
                signature: create_skill_signature(&parsed)?,
            };

            if managed_paths.contains_key(&parsed_name) {
                mounts.push(candidate);
                continue;
            }

            if Path::new(&managed_path).exists() {
                return Err(ManagerError::System(format!(
                    "集中目录已存在同名目录：{}",
                    managed_path
                )));
            }

            managed_paths.insert(parsed_name, managed_path);
            imports.push(candidate);
        }
    }

    Ok((imports, mounts))
}

async fn get_install_state(skill: &Value, cli_target: &Value, skill_disabled: bool) -> Value {
    let target_id = string_value(cli_target.get("id"));
    let skills_path = string_value(cli_target.get("skillsPath"));
    let target_path = Path::new(&skills_path).join(string_value(skill.get("name")));

    if skill_disabled {
        return json!({
          "targetId": target_id,
          "state": "disabled",
          "targetPath": path_text(target_path),
          "reason": "Skill 已禁用"
        });
    }

    if cli_target.get("installed").and_then(Value::as_bool) != Some(true) {
        return json!({
          "targetId": target_id,
          "state": "disabled",
          "targetPath": path_text(target_path),
          "reason": "CLI 未安装"
        });
    }

    let Ok(target_stat) = tokio::fs::symlink_metadata(&target_path).await else {
        return json!({
          "targetId": target_id,
          "state": "not-installed",
          "targetPath": path_text(target_path)
        });
    };
    let source_path = PathBuf::from(string_value(skill.get("sourcePath")));

    if !source_path.exists() {
        return json!({
          "targetId": target_id,
          "state": "broken-link",
          "targetPath": path_text(target_path)
        });
    }

    if !target_stat.file_type().is_symlink() {
        return json!({
          "targetId": target_id,
          "state": "disabled",
          "targetPath": path_text(target_path),
          "reason": "目标路径已被真实目录占用"
        });
    }

    let resolved_target = tokio::fs::canonicalize(&target_path).await;
    let resolved_source = tokio::fs::canonicalize(&source_path).await;

    match (resolved_target, resolved_source) {
        (Ok(target), Ok(source)) if same_path(&target, &source) => json!({
          "targetId": target_id,
          "state": "installed",
          "targetPath": path_text(target_path)
        }),
        (Ok(_), Ok(_)) => json!({
          "targetId": target_id,
          "state": "disabled",
          "targetPath": path_text(target_path),
          "reason": "目标路径已被其他内容占用"
        }),
        _ => json!({
          "targetId": target_id,
          "state": "broken-link",
          "targetPath": path_text(target_path)
        }),
    }
}

async fn install_skill_link(
    state: &Value,
    skill: &Value,
    target_id: &str,
) -> Result<(), ManagerError> {
    let cli_target = find_cli_target(state, target_id)?;

    if cli_target.get("installed").and_then(Value::as_bool) != Some(true) {
        return Err(ManagerError::System(format!(
            "{} 未安装，无法挂载 Skill",
            string_value(cli_target.get("name"))
        )));
    }

    let source_path = PathBuf::from(string_value(skill.get("sourcePath")));

    if !source_path.exists() {
        return Err(ManagerError::System(format!(
            "Skill 源目录不存在：{}",
            path_text(&source_path)
        )));
    }

    let skills_path = PathBuf::from(string_value(cli_target.get("skillsPath")));
    let target_path = skills_path.join(string_value(skill.get("name")));

    tokio::fs::create_dir_all(&skills_path).await?;

    if let Ok(target_stat) = tokio::fs::symlink_metadata(&target_path).await {
        if !target_stat.file_type().is_symlink() {
            return Err(ManagerError::System(format!(
                "目标路径已被真实目录占用，无法覆盖：{}",
                path_text(&target_path)
            )));
        }

        let resolved_target = tokio::fs::canonicalize(&target_path).await;
        let resolved_source = tokio::fs::canonicalize(&source_path).await;

        if matches!(
            (resolved_target, resolved_source),
            (Ok(ref target), Ok(ref source)) if same_path(target, source)
        ) {
            return Ok(());
        }

        remove_managed_link(&target_path).await?;
    }

    create_junction(&source_path, &target_path).await
}

async fn uninstall_skill_link(
    cli_targets: &[Value],
    skill_name: &str,
    target_id: &str,
) -> Result<(), ManagerError> {
    let Some(cli_target) = cli_targets
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(target_id))
    else {
        return Err(ManagerError::System(format!(
            "Unsupported CLI target: {}",
            target_id
        )));
    };
    let skills_path = string_value(cli_target.get("skillsPath"));

    if skills_path.is_empty() {
        return Ok(());
    }

    remove_managed_link(&Path::new(&skills_path).join(skill_name)).await
}

async fn remove_managed_link(target_path: &Path) -> Result<(), ManagerError> {
    let Ok(stat) = tokio::fs::symlink_metadata(target_path).await else {
        return Ok(());
    };

    if !stat.file_type().is_symlink() {
        return Err(ManagerError::System(format!(
            "目标路径不是可管理的链接，已拒绝删除：{}",
            path_text(target_path)
        )));
    }

    match tokio::fs::remove_dir(target_path).await {
        Ok(_) => Ok(()),
        Err(_) => {
            tokio::fs::remove_file(target_path).await?;
            Ok(())
        }
    }
}

async fn create_junction(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");

        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);

        let output = command
            .args([
                "/C",
                "mklink",
                "/J",
                &path_text(target_path),
                &path_text(source_path),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let output_message = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let message =
                first_non_empty(&[message, output_message, "创建 junction 失败".to_string()]);

            return Err(ManagerError::System(message));
        }
    }

    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(source_path, target_path)?;
    }

    Ok(())
}

async fn copy_dir_all(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::create_dir_all(target_path).await?;

    for entry in std::fs::read_dir(source_path)? {
        let entry = entry?;
        let source_child = entry.path();
        let target_child = target_path.join(entry.file_name());
        let stat = std::fs::symlink_metadata(&source_child)?;

        if stat.file_type().is_symlink() {
            continue;
        }

        if stat.is_dir() {
            Box::pin(copy_dir_all(&source_child, &target_child)).await?;
            continue;
        }

        if stat.is_file() {
            if let Some(parent) = target_child.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            tokio::fs::copy(&source_child, &target_child).await?;
        }
    }

    Ok(())
}

async fn move_dir_all(source_path: &Path, target_path: &Path) -> Result<(), ManagerError> {
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    match tokio::fs::rename(source_path, target_path).await {
        Ok(_) => Ok(()),
        Err(_) => {
            copy_dir_all(source_path, target_path).await?;
            remove_dir_all_if_exists(source_path).await
        }
    }
}

async fn remove_dir_all_if_exists(target_path: impl AsRef<Path>) -> Result<(), ManagerError> {
    match tokio::fs::remove_dir_all(target_path.as_ref()).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn extract_zip(zip_path: &str, target_path: &Path) -> Result<(), ManagerError> {
    let mut command = Command::new("powershell.exe");

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .args(extract_zip_command_args(zip_path, target_path))
        .output()
        .await?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();

        return Err(ManagerError::System(if message.is_empty() {
            "Skill zip 解压失败".to_string()
        } else {
            message
        }));
    }

    Ok(())
}

fn extract_zip_command_args(zip_path: &str, target_path: &Path) -> Vec<String> {
    vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        "Expand-Archive".to_string(),
        "-LiteralPath".to_string(),
        zip_path.to_string(),
        "-DestinationPath".to_string(),
        path_text(target_path),
        "-Force".to_string(),
    ]
}

async fn create_temp_dir(root_path: &str, prefix: &str) -> Result<PathBuf, ManagerError> {
    let temp_root = Path::new(root_path).join(format!("{}-{}", prefix, now_millis()));

    tokio::fs::create_dir_all(&temp_root).await?;
    Ok(temp_root)
}

fn find_skill(state: &Value, skill_name: &str) -> Result<Value, ManagerError> {
    state
        .get("skills")
        .and_then(Value::as_array)
        .and_then(|skills| {
            skills
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some(skill_name))
        })
        .cloned()
        .ok_or_else(|| ManagerError::System("Skill 不存在".to_string()))
}

fn find_cli_target<'a>(state: &'a Value, target_id: &str) -> Result<&'a Value, ManagerError> {
    state
        .get("cliTargets")
        .and_then(Value::as_array)
        .and_then(|targets| {
            targets
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(target_id))
        })
        .ok_or_else(|| ManagerError::System(format!("Unsupported CLI target: {}", target_id)))
}

fn ensure_skill_enabled(skill: &Value) -> Result<(), ManagerError> {
    if skill.get("disabled").and_then(Value::as_bool) == Some(true) {
        return Err(ManagerError::System(format!(
            "Skill 已禁用，恢复后才能使用：{}",
            string_value(skill.get("name"))
        )));
    }

    Ok(())
}

fn resolve_skill_status(skill_disabled: bool, install_states: Option<&Value>) -> String {
    if skill_disabled {
        return "disabled".to_string();
    }

    let states = install_states
        .and_then(Value::as_object)
        .map(|items| {
            items
                .values()
                .filter_map(|item| item.get("state").and_then(Value::as_str))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if states.contains(&"broken-link") {
        return "broken-link".to_string();
    }

    if states.contains(&"installed") {
        return "installed".to_string();
    }

    if !states.is_empty() && states.iter().all(|item| *item == "disabled") {
        return "disabled".to_string();
    }

    "not-installed".to_string()
}

fn collect_skill_view_entries(root_path: &Path) -> Result<Vec<Value>, ManagerError> {
    let mut entries = Vec::new();

    collect_skill_view_entries_inner(root_path, root_path, &mut entries)?;
    Ok(entries)
}

fn collect_skill_view_entries_inner(
    root_path: &Path,
    current_path: &Path,
    entries: &mut Vec<Value>,
) -> Result<(), ManagerError> {
    let mut children = std::fs::read_dir(current_path)?.collect::<Result<Vec<_>, _>>()?;

    children.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

    for child in children {
        let child_path = child.path();
        let child_name = child.file_name().to_string_lossy().to_string();
        let relative_path = child_path
            .strip_prefix(root_path)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| child_name.clone());
        let stat = std::fs::symlink_metadata(&child_path)?;

        if stat.file_type().is_symlink() {
            entries.push(json!({
              "path": relative_path,
              "name": child_name,
              "type": "symlink",
              "target": std::fs::read_link(&child_path)
                .map(path_text)
                .unwrap_or_default()
            }));
            continue;
        }

        if stat.is_dir() {
            entries.push(json!({
              "path": relative_path,
              "name": child_name,
              "type": "dir"
            }));

            if ![".git", "node_modules"].contains(&child_name.as_str()) {
                collect_skill_view_entries_inner(root_path, &child_path, entries)?;
            }
            continue;
        }

        if stat.is_file() {
            let ext = child_path
                .extension()
                .map(|value| format!(".{}", value.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let previewable = can_preview_skill_file(&child_name, &ext, stat.len());
            let mut entry = json!({
              "path": relative_path,
              "name": child_name,
              "type": "file",
              "ext": ext,
              "size": stat.len(),
              "previewable": previewable
            });

            if previewable {
                entry["content"] = json!(std::fs::read_to_string(&child_path)?);
            }

            entries.push(entry);
        }
    }

    Ok(())
}

fn can_preview_skill_file(file_name: &str, ext: &str, size: u64) -> bool {
    if size > SKILL_PREVIEW_MAX_SIZE {
        return false;
    }

    let extensions = [
        ".bat", ".cjs", ".cmd", ".css", ".csv", ".html", ".ini", ".js", ".json", ".jsonl", ".less",
        ".md", ".mjs", ".ps1", ".py", ".scss", ".sh", ".toml", ".ts", ".tsx", ".txt", ".vue",
        ".xml", ".yaml", ".yml",
    ];

    extensions.contains(&ext) || [".env", ".gitignore"].contains(&file_name)
}

fn create_skill_signature(skill: &Value) -> Result<String, ManagerError> {
    let source_path = string_value(skill.get("sourcePath"));
    let files = collect_skill_files(Path::new(&source_path))?;
    let payload = json!({
      "name": skill["name"],
      "description": string_value(skill.get("description")),
      "files": files
    });

    Ok(sha1_hex(&serde_json::to_string(&payload)?))
}

fn collect_skill_files(root_path: &Path) -> Result<Vec<Value>, ManagerError> {
    let mut files = Vec::new();

    collect_skill_files_inner(root_path, root_path, &mut files)?;
    files.sort_by(|left, right| {
        string_value(left.get("path")).cmp(&string_value(right.get("path")))
    });
    Ok(files)
}

fn collect_skill_files_inner(
    root_path: &Path,
    current_path: &Path,
    files: &mut Vec<Value>,
) -> Result<(), ManagerError> {
    let mut children = std::fs::read_dir(current_path)?.collect::<Result<Vec<_>, _>>()?;

    children.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

    for child in children {
        let child_path = child.path();
        let stat = std::fs::symlink_metadata(&child_path)?;

        if stat.file_type().is_symlink() {
            continue;
        }

        if stat.is_dir() {
            collect_skill_files_inner(root_path, &child_path, files)?;
            continue;
        }

        if stat.is_file() {
            let content = std::fs::read(&child_path)?;
            let relative_path = child_path
                .strip_prefix(root_path)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| path_text(&child_path));
            let ext = child_path
                .extension()
                .map(|value| format!(".{}", value.to_string_lossy().to_lowercase()))
                .unwrap_or_default();

            files.push(json!({
              "path": relative_path,
              "ext": ext,
              "hash": sha1_bytes(&content)
            }));
        }
    }

    Ok(())
}

async fn trash_skill(
    paths: &AppPaths,
    state: &Value,
    cli_targets: &[Value],
    trash_items: &mut Vec<Value>,
    skill_name: &str,
) -> Result<Value, ManagerError> {
    let skill = find_skill(state, skill_name)?;
    let source_path = PathBuf::from(string_value(skill.get("sourcePath")));

    if !source_path.is_dir() {
        return Err(ManagerError::System("Skill 源目录不存在".to_string()));
    }

    for target_id in skill
        .get("installedTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        uninstall_skill_link(cli_targets, skill_name, &string_value(Some(&target_id))).await?;
    }

    let now = now_millis();
    let trash_id = format!("skill-trash-{}", create_uuid_like_id());
    let source_dir_name = source_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| non_empty_slug(skill_name, &trash_id));
    let trash_path = skill_trash_root(paths).join(format!(
        "{}-{}",
        non_empty_slug(skill_name, &source_dir_name),
        trash_id
    ));

    move_dir_all(&source_path, &trash_path).await?;

    let item = json!({
      "id": trash_id,
      "name": skill_name,
      "description": string_value(skill.get("description")),
      "sourcePath": path_text(source_path),
      "trashPath": path_text(&trash_path),
      "deletedAt": now,
      "expiresAt": now + SKILL_TRASH_RETENTION_MS
    });

    trash_items.push(item.clone());
    Ok(item)
}

async fn restore_skill_trash_item(paths: &AppPaths, item: &Value) -> Result<Value, ManagerError> {
    let trash_path = validate_skill_trash_path(paths, &string_value(item.get("trashPath")))?;
    let skill_name = string_value(item.get("name"));
    let original_source_path = PathBuf::from(string_value(item.get("sourcePath")));
    let restore_path = if can_restore_skill_to_original_path(paths, &original_source_path)? {
        original_source_path
    } else {
        let restore_dir_name = non_empty_slug(
            &skill_name,
            &trash_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("skill-{}", now_millis())),
        );

        Path::new(&paths.skills_dir).join(restore_dir_name)
    };

    if restore_path.exists() {
        return Err(ManagerError::System(format!(
            "集中目录已存在同名目录：{}",
            path_text(&restore_path)
        )));
    }

    if state_skill_name_exists(paths, &skill_name)? {
        return Err(ManagerError::System(format!(
            "Skill 名称已存在：{}",
            skill_name
        )));
    }

    move_dir_all(&trash_path, &restore_path).await?;

    Ok(json!({
      "id": item["id"],
      "name": skill_name,
      "sourcePath": path_text(restore_path)
    }))
}

fn state_skill_name_exists(paths: &AppPaths, skill_name: &str) -> Result<bool, ManagerError> {
    Ok(skill_store::read_skills(paths)?.iter().any(|item| {
        item.get("name").and_then(Value::as_str) == Some(skill_name)
            && Path::new(&string_value(item.get("sourcePath"))).exists()
    }))
}

fn can_restore_skill_to_original_path(
    paths: &AppPaths,
    source_path: &Path,
) -> Result<bool, ManagerError> {
    if source_path.as_os_str().is_empty() || source_path.exists() {
        return Ok(false);
    }

    let skills_root = std::fs::canonicalize(&paths.skills_dir)?;
    let Some(parent) = source_path.parent() else {
        return Ok(false);
    };

    if !parent.exists() {
        return Ok(false);
    }

    Ok(std::fs::canonicalize(parent)?.starts_with(skills_root))
}

async fn remove_skill_trash_path(paths: &AppPaths, item: &Value) -> Result<(), ManagerError> {
    let trash_path = validate_skill_trash_path(paths, &string_value(item.get("trashPath")))?;

    remove_dir_all_if_exists(trash_path).await
}

async fn cleanup_expired_skill_trash(paths: &AppPaths) -> Result<(), ManagerError> {
    let now = now_millis();
    let mut active_items = Vec::new();

    for item in read_skill_trash(paths)? {
        if item.get("expiresAt").and_then(Value::as_u64).unwrap_or(0) as u128 <= now {
            let _ = remove_skill_trash_path(paths, &item).await;
            continue;
        }

        if !Path::new(&string_value(item.get("trashPath"))).exists() {
            continue;
        }

        active_items.push(item);
    }

    write_skill_trash(paths, &active_items).await
}

fn read_skill_trash(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    skill_store::read_trash(paths)
}

async fn write_skill_trash(paths: &AppPaths, items: &[Value]) -> Result<(), ManagerError> {
    skill_store::write_trash(paths, items)
}

fn skill_trash_root(paths: &AppPaths) -> PathBuf {
    Path::new(&paths.temp_dir).join("skill-trash")
}

fn validate_skill_trash_path(paths: &AppPaths, value: &str) -> Result<PathBuf, ManagerError> {
    let trash_root = skill_trash_root(paths);
    let trash_path = PathBuf::from(value);

    if value.is_empty() || !trash_path.exists() {
        return Err(ManagerError::System("回收站项目不存在".to_string()));
    }

    let root = std::fs::canonicalize(&trash_root)?;
    let target = std::fs::canonicalize(&trash_path)?;

    if !target.starts_with(root) {
        return Err(ManagerError::System("回收站项目路径不安全".to_string()));
    }

    Ok(target)
}

fn payload_item_ids(payload: Value) -> Vec<String> {
    payload
        .get("ids")
        .or_else(|| payload.get("itemIds"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .filter(|item| !item.is_empty())
        .collect()
}

fn normalize_skill_group(item: Value) -> Value {
    let now = now_millis();
    let group_id = string_value(item.get("id"));
    let name = string_value(item.get("name"));
    let skill_ids = unique_string_values(item.get("skillIds"));

    json!({
      "id": group_id,
      "name": name,
      "skillIds": skill_ids,
      "createdAt": item.get("createdAt").and_then(Value::as_u64).unwrap_or(now as u64),
      "updatedAt": item.get("updatedAt").and_then(Value::as_u64).unwrap_or(now as u64)
    })
}

fn remove_skill_ids_from_groups(groups: &mut [Value], skill_ids: &[String], except_group_id: &str) {
    for group in groups {
        if group.get("id").and_then(Value::as_str) == Some(except_group_id) {
            continue;
        }

        let next_skill_ids = unique_string_values(group.get("skillIds"))
            .into_iter()
            .filter(|item| !skill_ids.contains(item))
            .collect::<Vec<_>>();

        group["skillIds"] = json!(next_skill_ids);
    }
}

fn unique_string_values(value: Option<&Value>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for item in value
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let text = string_value(Some(&item));

        if text.is_empty() || seen.contains(&text) {
            continue;
        }

        seen.insert(text.clone());
        items.push(text);
    }

    items
}

async fn persist_skills(
    paths: &AppPaths,
    skills: &[Value],
    cli_targets: &[Value],
    mut install_index: HashMap<String, Vec<Value>>,
) -> Result<(), ManagerError> {
    for skill in skills {
        let skill_name = string_value(skill.get("name"));
        let installed_targets = skill
            .get("installedTargets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if installed_targets.is_empty() {
            install_index.remove(&skill_name);
        } else {
            install_index.insert(skill_name, installed_targets);
        }
    }

    skill_store::write_skills(paths, skills)?;
    write_json(
        &paths.storage_files.cli_targets,
        &json!(serialize_cli_targets(cli_targets)),
    )
    .await?;
    skill_store::write_installs(
        paths,
        &install_index
            .into_iter()
            .map(|(key, value)| (key, json!(value)))
            .collect(),
    )
}

fn serialize_cli_targets(cli_targets: &[Value]) -> Vec<Value> {
    cli_targets
        .iter()
        .cloned()
        .map(|mut target| {
            for key in ["configPath", "skillsPath", "sessionsPath"] {
                let value = string_value(target.get(key));

                if !value.is_empty() {
                    target[key] = json!(serialize_portable_path(&value));
                }
            }

            if let Some(items) = target.get("sessionPaths").and_then(Value::as_array) {
                target["sessionPaths"] = json!(items
                    .iter()
                    .map(|item| serialize_portable_path(&string_value(Some(item))))
                    .collect::<Vec<_>>());
            }

            target
        })
        .collect()
}

pub(crate) fn load_repositories(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    let caches = skill_store::read_repository_cache(paths)?;
    let cache_map = caches
        .into_iter()
        .map(|item| (string_value(item.get("id")), item))
        .collect::<HashMap<_, _>>();

    Ok(skill_store::read_repositories(paths)?
        .into_iter()
        .map(|item| {
            let repository = create_repository_runtime_item(&item);
            let repository_id = string_value(repository.get("id"));

            apply_repository_cache(repository, cache_map.get(&repository_id))
        })
        .collect())
}

async fn persist_repositories(
    paths: &AppPaths,
    repositories: &[Value],
) -> Result<(), ManagerError> {
    let storage_items = repositories
        .iter()
        .map(create_repository_storage_item)
        .collect::<Vec<_>>();
    let cache_items = repositories
        .iter()
        .map(|item| {
            json!({
              "id": item["id"],
              "status": item["status"],
              "skills": item["skills"],
              "error": item["error"],
              "lastSyncedAt": item["lastSyncedAt"],
              "updatedAt": item["updatedAt"]
            })
        })
        .collect::<Vec<_>>();

    skill_store::write_repositories(paths, &storage_items)?;
    skill_store::write_repository_cache(paths, &cache_items)
}

fn create_repository_storage_item(repository: &Value) -> Value {
    json!({
      "id": repository["id"],
      "type": repository["type"],
      "name": repository["name"],
      "source": repository["source"],
      "owner": repository["owner"],
      "repository": repository["repository"],
      "branch": repository["branch"],
      "rootPath": repository["rootPath"],
      "htmlUrl": repository["htmlUrl"],
      "createdAt": repository["createdAt"],
      "updatedAt": repository["updatedAt"]
    })
}

fn create_repository_runtime_item(repository: &Value) -> Value {
    let mut item = create_repository_storage_item(repository);

    item["status"] = json!("ready");
    item["skills"] = json!([]);
    item["error"] = json!("");
    item["lastSyncedAt"] = json!(0);
    item
}

fn apply_repository_cache(mut repository: Value, cache: Option<&Value>) -> Value {
    let Some(cache) = cache else {
        return repository;
    };

    repository["status"] = json!(first_non_empty(&[
        string_value(cache.get("status")),
        "ready".to_string(),
    ]));
    repository["skills"] = cache
        .get("skills")
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));
    repository["error"] = json!(string_value(cache.get("error")));
    repository["lastSyncedAt"] = json!(cache
        .get("lastSyncedAt")
        .and_then(Value::as_u64)
        .unwrap_or(0));
    repository["updatedAt"] = json!(cache
        .get("updatedAt")
        .and_then(Value::as_u64)
        .or_else(|| repository.get("updatedAt").and_then(Value::as_u64))
        .unwrap_or(0));
    repository
}

fn repository_state_patch(repositories: &[Value]) -> Value {
    json!({
      "skillRepositories": repositories,
      "refreshedAt": now_millis()
    })
}

fn merge_diagnostics(state: &mut Value, diagnostics: Vec<Value>) {
    let next = state
        .get("diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            !matches!(
                item.get("type").and_then(Value::as_str),
                Some(
                    "duplicate-skill-name"
                        | "metadata-error"
                        | "cleanup-error"
                        | "orphan-skill-cleaned"
                )
            )
        })
        .chain(diagnostics)
        .collect::<Vec<_>>();

    state["diagnostics"] = json!(next);
}

async fn write_json(path: &str, payload: &Value) -> Result<(), ManagerError> {
    if let Some(parent) = Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )
    .await?;
    Ok(())
}

fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

fn normalize_repository_path(value: &str) -> String {
    value.replace('\\', "/").trim_matches('/').to_string()
}

fn normalize_path_segment(value: &str) -> String {
    value
        .split('/')
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_path_segment(value: &str) -> String {
    let mut url = Url::parse("https://example.com/").unwrap_or_else(|_| unreachable!());

    url.path_segments_mut()
        .unwrap_or_else(|_| unreachable!())
        .push(value);

    url.path().trim_start_matches('/').to_string()
}

fn posix_basename(value: &str) -> Option<String> {
    value
        .split('/')
        .filter(|item| !item.is_empty())
        .last()
        .map(str::to_string)
}

fn posix_dirname(value: &str) -> String {
    let mut segments = value.split('/').collect::<Vec<_>>();

    if segments.is_empty() {
        return ".".to_string();
    }

    segments.pop();

    if segments.is_empty() {
        ".".to_string()
    } else {
        segments.join("/")
    }
}

fn posix_relative(base: &str, path: &str) -> String {
    let normalized_base = normalize_repository_path(base);
    let normalized_path = normalize_repository_path(path);

    if normalized_base.is_empty() {
        return normalized_path;
    }

    if normalized_path == normalized_base {
        return String::new();
    }

    normalized_path
        .strip_prefix(&format!("{}/", normalized_base))
        .unwrap_or(&normalized_path)
        .to_string()
}

fn non_empty_slug(value: &str, fallback: &str) -> String {
    let slug = slugify_name(value);

    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug
    }
}

fn slugify_name(value: &str) -> String {
    let mut result = String::new();
    let mut last_dash = false;

    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch);
            last_dash = false;
        } else if !last_dash {
            result.push('-');
            last_dash = true;
        }
    }

    result.trim_matches('-').to_string()
}

fn yaml_scalar(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn create_uuid_like_id() -> String {
    let now = now_millis();
    let process_id = std::process::id() as u128;
    let seed = now ^ (process_id << 32);
    let text = format!("{:032x}", seed);

    format!(
        "{}-{}-{}-{}-{}",
        &text[0..8],
        &text[8..12],
        &text[12..16],
        &text[16..20],
        &text[20..32]
    )
}

fn sha1_hex(value: &str) -> String {
    let mut hasher = Sha1::new();

    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn sha1_bytes(value: &[u8]) -> String {
    let mut hasher = Sha1::new();

    hasher.update(value);
    format!("{:x}", hasher.finalize())
}

fn file_time_millis(value: Option<std::time::SystemTime>) -> u128 {
    value
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or_else(now_millis)
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_text(left).eq_ignore_ascii_case(&path_text(right))
}

#[cfg(test)]
mod tests {
    use crate::core::paths::{path_text, resolve_app_paths};

    use super::{
        batch_skill_action, cleanup_expired_skill_trash, create_github_blob_url,
        create_repository_archive_url, delete_skills, ensure_skill_enabled,
        extract_zip_command_args, load_skill_groups,
        proxy_points_to_local_app_proxy, read_skill_trash, repository_archive_cache_path,
        repository_archive_zip_is_valid, repository_branch_candidates, resolve_archive_content_dir,
        resolve_archive_skill_source_dir, resolve_skill_status, save_skill_group,
        remove_skill_group_items, scan_repository_archive, skill_trash_root,
        try_load_cached_repository_archive_zip, RepositoryArchive,
    };
    use serde_json::json;
    use std::io::Write;
    use std::path::Path;

    fn write_test_repository_archive(zip_path: &Path) {
        let file = std::fs::File::create(zip_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        archive
            .start_file("skills-main/skills/demo/SKILL.md", options)
            .unwrap();
        archive
            .write_all(b"---\nname: demo\n---\nDemo skill")
            .unwrap();
        archive.finish().unwrap();
    }

    fn write_test_skill(root_path: &Path, name: &str) {
        std::fs::create_dir_all(root_path).unwrap();
        std::fs::write(
            root_path.join("SKILL.md"),
            format!("---\nname: {}\ndescription: test\n---\ncontent\n", name),
        )
        .unwrap();
    }

    #[test]
    fn disabled_skill_status_overrides_install_states() {
        let install_states = json!({
          "codex": {
            "state": "installed"
          }
        });

        assert_eq!(
            resolve_skill_status(true, Some(&install_states)),
            "disabled"
        );
    }

    #[test]
    fn disabled_skill_is_rejected_before_install() {
        let error = ensure_skill_enabled(&json!({
          "name": "demo-skill",
          "disabled": true
        }))
        .unwrap_err();

        assert!(error.to_string().contains("Skill 已禁用"));
    }

    #[test]
    fn extract_zip_command_passes_paths_as_literal_parameters() {
        let args = extract_zip_command_args("D:\\demo skill.zip", Path::new("D:\\target dir"));

        assert!(args.contains(&"-LiteralPath".to_string()));
        assert!(args.contains(&"D:\\demo skill.zip".to_string()));
        assert!(args.contains(&"-DestinationPath".to_string()));
        assert!(args.contains(&"D:\\target dir".to_string()));
        assert!(!args.iter().any(|item| item.contains("$args")));
    }

    #[test]
    fn repository_branch_candidates_try_configured_branch_before_defaults() {
        let repository = json!({
          "branch": "develop"
        });

        assert_eq!(
            repository_branch_candidates(&repository),
            vec![
                "develop".to_string(),
                "main".to_string(),
                "master".to_string()
            ]
        );
    }

    #[test]
    fn repository_branch_candidates_deduplicates_default_branch() {
        let repository = json!({
          "branch": "main"
        });

        assert_eq!(
            repository_branch_candidates(&repository),
            vec!["main".to_string(), "master".to_string()]
        );
    }

    #[test]
    fn repository_archive_url_uses_github_zip_archive_endpoint() {
        let repository = json!({
          "owner": "vuejs-ai",
          "repository": "skills"
        });

        assert_eq!(
            create_repository_archive_url(&repository, "feature/test branch"),
            "https://github.com/vuejs-ai/skills/archive/refs/heads/feature/test%20branch.zip"
        );
    }

    #[test]
    fn repository_archive_cache_path_uses_temp_archive_directory_and_hash_name() {
        let root = std::env::temp_dir().join(format!(
            "skill_archive_cache_path_test_{}",
            super::create_uuid_like_id()
        ));
        let paths = resolve_app_paths(&root);
        let repository = json!({
          "owner": "VueJS-AI",
          "repository": "skills"
        });
        let cache_path = repository_archive_cache_path(&paths, &repository, "feature/test branch");
        let cache_dir = Path::new(&paths.temp_dir).join("skill-repository-archives");
        let cache_file_name = cache_path.file_name().unwrap().to_string_lossy();

        assert!(cache_path.starts_with(&cache_dir));
        assert!(cache_file_name.ends_with(".zip"));
        assert!(!cache_file_name.contains("feature"));
        assert_ne!(
            cache_path,
            repository_archive_cache_path(&paths, &repository, "main")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cached_repository_archive_zip_is_reused_when_valid() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_archive_cache_reuse_test_{}",
                super::create_uuid_like_id()
            ));
            let cache_path = root.join("cache.zip");
            let zip_path = root.join("temp").join("repository.zip");

            std::fs::create_dir_all(&root).unwrap();
            write_test_repository_archive(&cache_path);

            assert!(
                try_load_cached_repository_archive_zip(&cache_path, &zip_path)
                    .await
                    .unwrap()
            );
            assert!(repository_archive_zip_is_valid(&zip_path));

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn invalid_cached_repository_archive_zip_is_removed() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_archive_bad_cache_test_{}",
                super::create_uuid_like_id()
            ));
            let cache_path = root.join("cache.zip");
            let zip_path = root.join("temp").join("repository.zip");

            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(&cache_path, "not a zip").unwrap();

            assert!(
                !try_load_cached_repository_archive_zip(&cache_path, &zip_path)
                    .await
                    .unwrap()
            );
            assert!(!cache_path.exists());
            assert!(!zip_path.exists());

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn delete_skill_moves_source_directory_to_trash() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_delete_test_{}",
                super::create_uuid_like_id()
            ));
            let paths = resolve_app_paths(&root);
            let skill_root = Path::new(&paths.skills_dir).join("demo-skill");
            let mut state = json!({
              "skills": [],
              "repos": [],
              "cliTargets": []
            });

            write_test_skill(&skill_root, "demo-skill");
            super::refresh_skills_state(&paths, &mut state)
                .await
                .unwrap();

            let result = delete_skills(
                &paths,
                &mut state,
                json!({
                  "skillNames": ["demo-skill"]
                }),
            )
            .await
            .unwrap();
            let deleted = result
                .get("deleted")
                .and_then(serde_json::Value::as_array)
                .unwrap();
            let trash_items = read_skill_trash(&paths).unwrap();
            let trash_source_path = Path::new(
                trash_items[0]
                    .get("trashPath")
                    .and_then(serde_json::Value::as_str)
                    .unwrap(),
            );

            assert_eq!(deleted.len(), 1);
            assert!(!skill_root.exists());
            assert!(trash_source_path.join("SKILL.md").exists());
            assert_eq!(
                state
                    .get("skills")
                    .and_then(serde_json::Value::as_array)
                    .unwrap()
                    .len(),
                0
            );

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn expired_skill_trash_items_are_removed_after_ten_days() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_trash_expired_test_{}",
                super::create_uuid_like_id()
            ));
            let paths = resolve_app_paths(&root);
            let trash_root = skill_trash_root(&paths);
            let expired_path = trash_root.join("expired-skill");
            let now = super::now_millis();

            write_test_skill(&expired_path, "expired-skill");
            super::write_json(
                &path_text(trash_root.join("trash.json")),
                &json!([
                  {
                    "id": "expired",
                    "name": "expired-skill",
                    "trashPath": path_text(&expired_path),
                    "deletedAt": now - 11 * 24 * 60 * 60 * 1000,
                    "expiresAt": now - 24 * 60 * 60 * 1000
                  }
                ]),
            )
            .await
            .unwrap();

            cleanup_expired_skill_trash(&paths).await.unwrap();

            assert!(!expired_path.exists());
            assert!(read_skill_trash(&paths).unwrap().is_empty());

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn skill_group_persists_unique_skill_ids() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_group_save_test_{}",
                super::create_uuid_like_id()
            ));
            let paths = resolve_app_paths(&root);

            save_skill_group(
                &paths,
                json!({
                  "name": "debug suite",
                  "skillIds": ["skill-a", "skill-b", "skill-a", ""]
                }),
            )
            .await
            .unwrap();

            let groups = load_skill_groups(&paths).unwrap();

            assert_eq!(groups.len(), 1);
            assert_eq!(
                groups[0]
                    .get("skillIds")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                vec![json!("skill-a"), json!("skill-b")]
            );

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn moving_skill_to_group_removes_it_from_other_groups() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_group_move_test_{}",
                super::create_uuid_like_id()
            ));
            let paths = resolve_app_paths(&root);

            let first = save_skill_group(
                &paths,
                json!({
                  "name": "first",
                  "skillIds": ["skill-a", "skill-b"]
                }),
            )
            .await
            .unwrap();
            save_skill_group(
                &paths,
                json!({
                  "name": "second",
                  "skillIds": ["skill-b", "skill-c"]
                }),
            )
            .await
            .unwrap();

            let groups = first
                .get("groups")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let first_id = groups[0].get("id").and_then(serde_json::Value::as_str).unwrap();
            let groups = load_skill_groups(&paths).unwrap();
            let first_group = groups
                .iter()
                .find(|item| item.get("id").and_then(serde_json::Value::as_str) == Some(first_id))
                .unwrap();

            assert_eq!(
                first_group
                    .get("skillIds")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                vec![json!("skill-a")]
            );

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn skill_group_items_can_be_removed() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_group_remove_items_test_{}",
                super::create_uuid_like_id()
            ));
            let paths = resolve_app_paths(&root);
            let result = save_skill_group(
                &paths,
                json!({
                  "name": "debug suite",
                  "skillIds": ["skill-a", "skill-b"]
                }),
            )
            .await
            .unwrap();
            let group_id = result
                .get("group")
                .and_then(|item| item.get("id"))
                .and_then(serde_json::Value::as_str)
                .unwrap();

            remove_skill_group_items(
                &paths,
                json!({
                  "groupId": group_id,
                  "skillIds": ["skill-a"]
                }),
            )
            .await
            .unwrap();

            let groups = load_skill_groups(&paths).unwrap();

            assert_eq!(
                groups[0]
                    .get("skillIds")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                vec![json!("skill-b")]
            );

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn empty_skill_group_can_be_renamed_without_changing_id() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_group_rename_test_{}",
                super::create_uuid_like_id()
            ));
            let paths = resolve_app_paths(&root);
            let result = save_skill_group(
                &paths,
                json!({
                  "name": "old name",
                  "skillIds": ["skill-a"]
                }),
            )
            .await
            .unwrap();
            let group_id = result
                .get("group")
                .and_then(|item| item.get("id"))
                .and_then(serde_json::Value::as_str)
                .unwrap();

            remove_skill_group_items(
                &paths,
                json!({
                  "groupId": group_id,
                  "skillIds": ["skill-a"]
                }),
            )
            .await
            .unwrap();
            save_skill_group(
                &paths,
                json!({
                  "groupId": group_id,
                  "name": "new name",
                  "skillIds": []
                }),
            )
            .await
            .unwrap();

            let groups = load_skill_groups(&paths).unwrap();

            assert_eq!(groups[0].get("id").and_then(serde_json::Value::as_str), Some(group_id));
            assert_eq!(
                groups[0].get("name").and_then(serde_json::Value::as_str),
                Some("new name")
            );

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn batch_install_uses_selected_cli_targets() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "skill_batch_target_test_{}",
                super::create_uuid_like_id()
            ));
            let paths = resolve_app_paths(&root);
            let skill_root = Path::new(&paths.skills_dir).join("demo-skill");
            let target_a = root.join("target-a");
            let target_b = root.join("target-b");
            let mut state = json!({
              "skills": [],
              "repos": [],
              "cliTargets": [
                {
                  "id": "a",
                  "name": "CLI A",
                  "installed": true,
                  "skillsPath": path_text(&target_a)
                },
                {
                  "id": "b",
                  "name": "CLI B",
                  "installed": true,
                  "skillsPath": path_text(&target_b)
                }
              ]
            });

            write_test_skill(&skill_root, "demo-skill");
            super::refresh_skills_state(&paths, &mut state)
                .await
                .unwrap();

            let result = batch_skill_action(
                &paths,
                &mut state,
                json!({
                  "action": "install-all",
                  "skillNames": ["demo-skill"],
                  "targetIds": ["a"]
                }),
            )
            .await
            .unwrap();

            assert_eq!(
                result
                    .get("successes")
                    .and_then(serde_json::Value::as_array)
                    .unwrap()
                    .len(),
                1
            );
            assert!(target_a.join("demo-skill").exists());
            assert!(!target_b.join("demo-skill").exists());

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn local_app_proxy_ports_are_detected_to_avoid_self_loop() {
        assert!(proxy_points_to_local_app_proxy("http://127.0.0.1:15721"));
        assert!(proxy_points_to_local_app_proxy("localhost:15722"));
        assert!(!proxy_points_to_local_app_proxy("http://127.0.0.1:7890"));
        assert!(!proxy_points_to_local_app_proxy(
            "http://proxy.example.com:15721"
        ));
    }

    #[test]
    fn github_blob_url_is_for_readme_display_only() {
        let repository = json!({
          "owner": "vuejs-ai",
          "repository": "skills"
        });

        assert_eq!(
            create_github_blob_url(
                &repository,
                "main",
                "skills/vue-debug-guides/reference/cleanup-side-effects.md"
            ),
            "https://github.com/vuejs-ai/skills/blob/main/skills/vue-debug-guides/reference/cleanup-side-effects.md"
        );
    }

    #[test]
    fn archive_content_dir_removes_single_archive_root_prefix() {
        let root = std::env::temp_dir().join(format!(
            "skill_archive_root_test_{}",
            super::create_uuid_like_id()
        ));
        let archive_root = root.join("skills-main");

        std::fs::create_dir_all(&archive_root).unwrap();

        let resolved = resolve_archive_content_dir(&root).unwrap();

        assert_eq!(resolved, archive_root);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn archive_skill_source_dir_rejects_path_traversal() {
        let root = std::env::temp_dir().join(format!(
            "skill_archive_path_test_{}",
            super::create_uuid_like_id()
        ));

        std::fs::create_dir_all(&root).unwrap();

        let error = resolve_archive_skill_source_dir(&root, "../outside")
            .unwrap_err()
            .to_string();

        assert!(error.contains("路径不安全"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn archive_skill_source_dir_resolves_safe_nested_directory() {
        let root = std::env::temp_dir().join(format!(
            "skill_archive_safe_path_test_{}",
            super::create_uuid_like_id()
        ));
        let skill_dir = root.join("skills").join("vue-debug-guides");

        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: demo\n---\n").unwrap();

        let resolved = resolve_archive_skill_source_dir(&root, "skills/vue-debug-guides").unwrap();

        assert_eq!(resolved, std::fs::canonicalize(&skill_dir).unwrap());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn scan_repository_archive_reads_skill_from_local_archive_content() {
        let root = std::env::temp_dir().join(format!(
            "skill_archive_scan_test_{}",
            super::create_uuid_like_id()
        ));
        let content_dir = root.join("skills-main");
        let skill_dir = content_dir.join("skills").join("vue-debug-guides");

        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: vue-debug-guides\ndescription: Vue 调试指南\ntags:\n  - vue\n---\n正文内容\n",
        )
        .unwrap();

        let repository = json!({
          "source": "https://github.com/vuejs-ai/skills",
          "owner": "vuejs-ai",
          "repository": "skills",
          "rootPath": "skills"
        });
        let archive = RepositoryArchive {
            branch: "main".to_string(),
            temp_root: root.clone(),
            content_dir: content_dir.clone(),
        };
        let scanned = scan_repository_archive(&repository, &archive).unwrap();
        let skills = scanned
            .get("skills")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        let skill = &skills[0];

        assert_eq!(
            scanned.get("branch").and_then(serde_json::Value::as_str),
            Some("main")
        );
        assert_eq!(
            skill.get("name").and_then(serde_json::Value::as_str),
            Some("vue-debug-guides")
        );
        assert_eq!(
            skill.get("skillPath").and_then(serde_json::Value::as_str),
            Some("skills/vue-debug-guides")
        );
        assert_eq!(
            skill.get("displayPath").and_then(serde_json::Value::as_str),
            Some("vue-debug-guides")
        );
        assert_eq!(
            skill.get("content").and_then(serde_json::Value::as_str),
            Some("正文内容")
        );
        assert_eq!(
            skill.get("readmeUrl").and_then(serde_json::Value::as_str),
            Some("https://github.com/vuejs-ai/skills/blob/main/skills/vue-debug-guides/SKILL.md")
        );

        let _ = std::fs::remove_dir_all(root);
    }
}

use crate::core::error::ManagerError;
use crate::core::paths::{path_text, AppPaths};
use crate::core::settings::serialize_portable_path;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use url::Url;

const IGNORE_DIRS: [&str; 6] = [".git", "node_modules", "dist", "build", ".cache", "temp"];
const SKILL_PREVIEW_MAX_SIZE: u64 = 512 * 1024;

pub async fn refresh_skills_state(paths: &AppPaths, state: &mut Value) -> Result<(), ManagerError> {
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
    let previous_skills = read_array(&paths.storage_files.skills)?;
    let mut install_index = read_object_index(&paths.storage_files.installs)?;
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

        for cli_target in &cli_targets {
            let state = get_install_state(&skill, cli_target).await;

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
        skill["status"] = json!(resolve_skill_status(skill.get("installStates")));
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

    install_skill_link(state, &skill, &target_id).await?;
    Ok(())
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

pub async fn repair_skill(state: &Value, payload: Value) -> Result<(), ManagerError> {
    let skill_name = string_value(payload.get("skillName"));
    let target_id = string_value(payload.get("targetId"));
    let skill = find_skill(state, &skill_name)?;
    let source_path = string_value(skill.get("sourcePath"));

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
) -> Result<(), ManagerError> {
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

    let temp_root = create_temp_dir("monkey-thief-skill").await?;

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

            import_items.push((PathBuf::from(source_path), managed_path));
        }

        if import_items.is_empty() {
            return Err(ManagerError::System(
                "zip 压缩包中的 Skill 已存在，无需重复导入".to_string(),
            ));
        }

        for (source_path, managed_path) in import_items {
            copy_dir_all(&source_path, &managed_path).await?;
        }

        refresh_skills_state(paths, state).await
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

    match scan_repository(&repository).await {
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

    match scan_repository(&repositories[index]).await {
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

    if state
        .get("skills")
        .and_then(Value::as_array)
        .is_some_and(|skills| {
            skills
                .iter()
                .any(|item| item.get("name").and_then(Value::as_str) == Some(skill_name.as_str()))
        })
    {
        return Err(ManagerError::System(format!(
            "Skill 名称已存在：{}",
            skill_name
        )));
    }

    let tree_info = fetch_repository_tree(repository).await?;
    let skill_path = string_value(skill.get("skillPath"));
    let directory_name = non_empty_slug(
        &skill_name,
        posix_basename(&skill_path)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "skill".to_string())
            .as_str(),
    );
    let managed_path = Path::new(&paths.skills_dir).join(directory_name);
    let files = tree_info
        .tree
        .iter()
        .filter(|item| {
            item.get("type").and_then(Value::as_str) == Some("blob")
                && is_path_inside_directory(&string_value(item.get("path")), &skill_path)
        })
        .cloned()
        .collect::<Vec<_>>();

    if managed_path.exists() {
        return Err(ManagerError::System(format!(
            "集中目录已存在同名目录：{}",
            path_text(&managed_path)
        )));
    }

    if files.is_empty() {
        return Err(ManagerError::System(
            "仓库 Skill 目录下没有可下载文件".to_string(),
        ));
    }

    for file in files {
        let file_path = string_value(file.get("path"));
        let relative_path = if skill_path.is_empty() {
            file_path.clone()
        } else {
            posix_relative(&skill_path, &file_path)
        };
        let target_path = managed_path.join(relative_path.replace('/', "\\"));

        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        download_file(
            &create_raw_file_url(repository, &tree_info.ref_name, &file_path),
            &target_path,
        )
        .await?;
    }

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

struct RepositoryTree {
    ref_name: String,
    tree: Vec<Value>,
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

async fn scan_repository(repository: &Value) -> Result<Value, ManagerError> {
    let tree_info = fetch_repository_tree(repository).await?;
    let root_path = normalize_repository_path(&string_value(repository.get("rootPath")));
    let root_exists = root_path.is_empty()
        || tree_info
            .tree
            .iter()
            .any(|item| is_path_inside_directory(&string_value(item.get("path")), &root_path));

    if !root_exists {
        return Err(ManagerError::System("仓库链接下的目录不存在".to_string()));
    }

    let skill_files = tree_info
        .tree
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("blob"))
        .filter(|item| {
            posix_basename(&string_value(item.get("path"))).as_deref() == Some("SKILL.md")
        })
        .filter(|item| is_path_inside_directory(&string_value(item.get("path")), &root_path))
        .cloned()
        .collect::<Vec<_>>();
    let mut skills = Vec::new();

    for skill_file in skill_files {
        let skill_file_path = string_value(skill_file.get("path"));
        let skill_path = posix_dirname(&skill_file_path);
        let normalized_skill_path = if skill_path == "." {
            String::new()
        } else {
            skill_path
        };
        let content = fetch_raw_file(repository, &tree_info.ref_name, &skill_file_path).await?;
        let parsed = parse_skill_content(&content, &skill_file_path)?;

        skills.push(json!({
          "id": sha1_hex(&format!(
            "{}:{}:{}",
            string_value(repository.get("source")),
            tree_info.ref_name,
            normalized_skill_path
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
          "skillPath": normalized_skill_path,
          "displayPath": if root_path.is_empty() {
              if normalized_skill_path.is_empty() { ".".to_string() } else { normalized_skill_path.clone() }
          } else {
              let relative = posix_relative(&root_path, &normalized_skill_path);
              if relative.is_empty() { ".".to_string() } else { relative }
          },
          "updatedAt": now_millis()
        }));
    }

    skills.sort_by(|left, right| {
        string_value(left.get("name")).cmp(&string_value(right.get("name")))
    });

    Ok(json!({
      "branch": tree_info.ref_name,
      "skills": skills
    }))
}

fn parse_skill_content(content: &str, skill_file_path: &str) -> Result<Value, ManagerError> {
    let (metadata, body) = parse_frontmatter(content);
    let skill_name = metadata.get("name").cloned().unwrap_or_default();

    if skill_name.is_empty() {
        return Err(ManagerError::System(format!(
            "Missing required frontmatter field \"name\" in {}",
            skill_file_path
        )));
    }

    Ok(json!({
      "name": skill_name,
      "description": metadata.get("description").cloned().unwrap_or_default(),
      "content": body.trim(),
      "version": metadata.get("version").cloned().unwrap_or_default(),
      "author": metadata.get("author").cloned().unwrap_or_default(),
      "tags": metadata
        .get("tags")
        .map(|value| parse_tags(value))
        .unwrap_or_default(),
      "entry": metadata
        .get("entry")
        .cloned()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "SKILL.md".to_string()),
      "homepage": metadata.get("homepage").cloned().unwrap_or_default(),
      "repository": metadata.get("repository").cloned().unwrap_or_default()
    }))
}

async fn fetch_repository_tree(repository: &Value) -> Result<RepositoryTree, ManagerError> {
    let ref_name = resolve_repository_ref(repository).await?;
    let url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        string_value(repository.get("owner")),
        string_value(repository.get("repository")),
        normalize_path_segment(&ref_name)
    );
    let data = fetch_json(&url).await?;
    let tree = data
        .get("tree")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if data.get("truncated").and_then(Value::as_bool) == Some(true) {
        return Err(ManagerError::System(
            "GitHub 返回的仓库文件树已截断，无法完整扫描".to_string(),
        ));
    }

    Ok(RepositoryTree { ref_name, tree })
}

async fn resolve_repository_ref(repository: &Value) -> Result<String, ManagerError> {
    let branch = string_value(repository.get("branch"));

    if !branch.is_empty() {
        return Ok(branch);
    }

    let data = fetch_json(&format!(
        "https://api.github.com/repos/{}/{}",
        string_value(repository.get("owner")),
        string_value(repository.get("repository"))
    ))
    .await?;
    let branch = string_value(data.get("default_branch"));

    if branch.is_empty() {
        return Err(ManagerError::System(
            "GitHub 仓库默认分支不存在".to_string(),
        ));
    }

    Ok(branch)
}

async fn fetch_json(url: &str) -> Result<Value, ManagerError> {
    let response = reqwest::Client::new()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Monkey-Thief")
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if !status.is_success() {
        let payload = parse_json_text(&text);
        let message = payload
            .as_ref()
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str)
            .unwrap_or(url);

        return Err(ManagerError::System(format!(
            "GitHub 请求失败：{} {}",
            status.as_u16(),
            message
        )));
    }

    Ok(parse_json_text(&text).unwrap_or(Value::Null))
}

async fn fetch_raw_file(
    repository: &Value,
    ref_name: &str,
    file_path: &str,
) -> Result<String, ManagerError> {
    let url = create_raw_file_url(repository, ref_name, file_path);
    let response = reqwest::get(&url)
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let status = response.status();

    if !status.is_success() {
        return Err(ManagerError::System(format!(
            "GitHub 文件读取失败：{} {}",
            status.as_u16(),
            file_path
        )));
    }

    response
        .text()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))
}

async fn download_file(url: &str, target_path: &Path) -> Result<(), ManagerError> {
    let response = reqwest::get(url)
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
        target_path,
        response
            .bytes()
            .await
            .map_err(|error| ManagerError::System(error.to_string()))?,
    )
    .await?;
    Ok(())
}

fn create_raw_file_url(repository: &Value, ref_name: &str, file_path: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        string_value(repository.get("owner")),
        string_value(repository.get("repository")),
        normalize_path_segment(ref_name),
        normalize_path_segment(file_path)
    )
}

fn parse_json_text(text: &str) -> Option<Value> {
    if text.trim().is_empty() {
        return None;
    }

    serde_json::from_str(text).ok()
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

async fn get_install_state(skill: &Value, cli_target: &Value) -> Value {
    let target_id = string_value(cli_target.get("id"));
    let skills_path = string_value(cli_target.get("skillsPath"));
    let target_path = Path::new(&skills_path).join(string_value(skill.get("name")));

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
        let output = Command::new("cmd")
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
            let message = first_non_empty(&[
                message,
                output_message,
                "创建 junction 失败".to_string(),
            ]);

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

async fn remove_dir_all_if_exists(target_path: impl AsRef<Path>) -> Result<(), ManagerError> {
    match tokio::fs::remove_dir_all(target_path.as_ref()).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn extract_zip(zip_path: &str, target_path: &Path) -> Result<(), ManagerError> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Expand-Archive -LiteralPath $args[0] -DestinationPath $args[1] -Force",
            zip_path,
            &path_text(target_path),
        ])
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

async fn create_temp_dir(prefix: &str) -> Result<PathBuf, ManagerError> {
    let temp_root = std::env::temp_dir().join(format!("{}-{}", prefix, now_millis()));

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

fn resolve_skill_status(install_states: Option<&Value>) -> String {
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

    write_json(&paths.storage_files.skills, &json!(skills)).await?;
    write_json(
        &paths.storage_files.cli_targets,
        &json!(serialize_cli_targets(cli_targets)),
    )
    .await?;
    write_json(&paths.storage_files.installs, &json!(install_index)).await
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
    let caches = read_array(&paths.storage_files.skill_repository_cache)?;
    let cache_map = caches
        .into_iter()
        .map(|item| (string_value(item.get("id")), item))
        .collect::<HashMap<_, _>>();

    Ok(read_array(&paths.storage_files.skill_repositories)?
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

    write_json(
        &paths.storage_files.skill_repositories,
        &json!(storage_items),
    )
    .await?;
    write_json(
        &paths.storage_files.skill_repository_cache,
        &json!(cache_items),
    )
    .await
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

fn read_array(path: &str) -> Result<Vec<Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn read_object_index(path: &str) -> Result<HashMap<String, Vec<Value>>, ManagerError> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(ManagerError::Io(error)),
    };
    let value: Value = serde_json::from_str(&content)?;
    let index = value
        .as_object()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key, value.as_array().cloned().unwrap_or_default()))
        .collect();

    Ok(index)
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
        .map(|item| url::form_urlencoded::byte_serialize(item.as_bytes()).collect::<String>())
        .collect::<Vec<_>>()
        .join("/")
}

fn is_path_inside_directory(file_path: &str, directory_path: &str) -> bool {
    if directory_path.is_empty() {
        return true;
    }

    file_path == directory_path || file_path.starts_with(&format!("{}/", directory_path))
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

use crate::core::error::ManagerError;
use crate::core::paths::{path_text, AppPaths};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

pub async fn scan_branches(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let project = get_repo_project(paths, repos, &repo_id)?;
    let branch_scan = get_local_branch_scan(&string_value(project.get("projectPath"))).await?;
    let archives = list_archives(paths, repos, json!({ "repoId": repo_id })).await?;
    let stash_archives = list_stash_archives(paths, repos, json!({ "repoId": repo_id })).await?;

    Ok(json!({
      "project": project,
      "currentBranch": branch_scan["currentBranch"],
      "branches": branch_scan["branches"],
      "archives": archives,
      "stashes": [],
      "stashArchives": stash_archives
    }))
}

pub async fn list_commits(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let branch_name = string_value(payload.get("branchName"));
    let skip_check = payload.get("skipCheck").and_then(Value::as_bool) == Some(true);
    let project = get_repo_project(paths, repos, &repo_id)?;
    let project_path = string_value(project.get("projectPath"));
    let commits = get_commits(&project_path, &branch_name).await?;
    let check_branch_name = string_value(project.get("checkBranchName"));

    if skip_check || check_branch_name.is_empty() || check_branch_name == branch_name {
        return Ok(json!(commits));
    }

    let commit_check_cache = read_commit_check_cache(paths, &repo_id)?;
    let mut commit_check_cache_map = commit_check_cache
        .into_iter()
        .map(|item| {
            (
                format!(
                    "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
                    string_value(item.get("sourceBranchName")),
                    string_value(item.get("commitHash")),
                    string_value(item.get("subject")),
                    string_value(item.get("date")),
                    string_value(item.get("targetBranchName"))
                ),
                item,
            )
        })
        .collect::<HashMap<_, _>>();
    let mut commits_for_check = Vec::new();
    let cached_commits = commits
        .into_iter()
        .map(|commit| {
            if commit.get("isGraphOnly").and_then(Value::as_bool) == Some(true) {
                return commit;
            }

            let cache_key = format!(
                "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
                branch_name,
                string_value(commit.get("hash")),
                string_value(commit.get("subject")),
                string_value(commit.get("date")),
                check_branch_name
            );

            if let Some(cached_commit) = commit_check_cache_map.get(&cache_key) {
                let mut commit = commit.clone();
                commit["checkStatus"] =
                    json!(if string_value(cached_commit.get("matchedBy")) == "hash" {
                        "exists-hash"
                    } else {
                        "exists-subject"
                    });
                commit["checkTargetBranch"] = json!(check_branch_name);
                return commit;
            }

            commits_for_check.push(commit.clone());
            commit
        })
        .collect::<Vec<_>>();

    if commits_for_check.is_empty() {
        return Ok(json!(cached_commits));
    }

    let check_result =
        check_commits_on_branch(&project_path, &check_branch_name, commits_for_check).await?;
    let mut checked_commit_index = 0usize;
    let checked_items = check_result
        .get("checkedCommits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let checked_commits = cached_commits
        .into_iter()
        .map(|commit| {
            if commit.get("isGraphOnly").and_then(Value::as_bool) == Some(true)
                || !string_value(commit.get("checkTargetBranch")).is_empty()
            {
                return commit;
            }

            let checked_commit = checked_items
                .get(checked_commit_index)
                .cloned()
                .unwrap_or(commit);
            checked_commit_index += 1;
            checked_commit
        })
        .collect::<Vec<_>>();

    let matched_commits = check_result
        .get("matchedCommits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if !matched_commits.is_empty() {
        let checked_at = now_millis();

        for item in matched_commits {
            let key = format!(
                "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
                branch_name,
                string_value(item.get("commitHash")),
                string_value(item.get("subject")),
                string_value(item.get("date")),
                string_value(item.get("targetBranchName"))
            );
            let mut cache_item = item.clone();

            cache_item["sourceBranchName"] = json!(branch_name);
            cache_item["checkedAt"] = json!(checked_at);
            commit_check_cache_map.insert(key, cache_item);
        }

        write_commit_check_cache(
            paths,
            &repo_id,
            commit_check_cache_map.into_values().collect(),
        )
        .await?;
    }

    Ok(json!(checked_commits))
}

pub async fn get_commit_detail(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let commit_hash = string_value(payload.get("commitHash"));
    let file_path = string_value(payload.get("filePath"));
    let project = get_repo_project(paths, repos, &repo_id)?;

    get_commit_detail_by_args(
        Vec::new(),
        &string_value(project.get("projectPath")),
        &commit_hash,
        &file_path,
    )
    .await
}

pub async fn update_check_branch(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let branch_name = string_value(payload.get("branchName"));

    save_project_patch(
        paths,
        repos,
        &repo_id,
        json!({ "checkBranchName": branch_name }),
    )
    .await
}

pub async fn clear_commit_check_cache(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let source_branch_name = string_value(payload.get("sourceBranchName"));
    let target_branch_name = string_value(payload.get("targetBranchName"));

    get_repo_project(paths, repos, &repo_id)?;

    let commit_check_cache = read_commit_check_cache(paths, &repo_id)?;
    let previous_len = commit_check_cache.len();
    let next_commit_check_cache = commit_check_cache
        .into_iter()
        .filter(|item| {
            string_value(item.get("sourceBranchName")) != source_branch_name
                || string_value(item.get("targetBranchName")) != target_branch_name
        })
        .collect::<Vec<_>>();
    let cleared = previous_len - next_commit_check_cache.len();

    write_commit_check_cache(paths, &repo_id, next_commit_check_cache).await?;
    Ok(json!(cleared))
}

pub async fn check_commit_on_branch_api(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let source_branch_name = string_value(payload.get("sourceBranchName"));
    let target_branch_name = string_value(payload.get("targetBranchName"));
    let commit_hash = string_value(payload.get("commitHash"));
    let subject = string_value(payload.get("subject"));
    let date = string_value(payload.get("date"));
    let project = get_repo_project(paths, repos, &repo_id)?;
    let mut commit_check_cache = read_commit_check_cache(paths, &repo_id)?;
    let cache_key = format!(
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        source_branch_name, commit_hash, subject, date, target_branch_name
    );

    if let Some(cached_commit) = commit_check_cache.iter().find(|item| {
        format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
            string_value(item.get("sourceBranchName")),
            string_value(item.get("commitHash")),
            string_value(item.get("subject")),
            string_value(item.get("date")),
            string_value(item.get("targetBranchName"))
        ) == cache_key
    }) {
        return Ok(json!({
          "matchedBy": cached_commit["matchedBy"],
          "commit": cached_commit["matchedCommit"]
        }));
    }

    let matched_result = check_commit_on_branch(
        &string_value(project.get("projectPath")),
        &target_branch_name,
        &commit_hash,
        &subject,
        &date,
    )
    .await?;

    if !matched_result.is_null() {
        commit_check_cache.push(json!({
          "sourceBranchName": source_branch_name,
          "commitHash": commit_hash,
          "subject": subject,
          "date": date,
          "targetBranchName": target_branch_name,
          "matchedBy": matched_result["matchedBy"],
          "matchedCommit": matched_result["commit"],
          "checkedAt": now_millis()
        }));
        write_commit_check_cache(paths, &repo_id, commit_check_cache).await?;
    }

    Ok(matched_result)
}

pub async fn archive_branch(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let branch_name = string_value(payload.get("branchName"));
    let project = get_repo_project(paths, repos, &repo_id)?;
    let project_path = string_value(project.get("projectPath"));
    let current_branch = get_current_branch(&project_path).await?;

    if branch_name == current_branch {
        return Err(ManagerError::System(
            "当前 checkout 分支不能归档，请先切换到其他分支".to_string(),
        ));
    }

    if !branch_exists(&project_path, &branch_name).await? {
        return Err(ManagerError::System("本地分支不存在".to_string()));
    }

    let commit_hash = get_branch_commit(&project_path, &branch_name).await?;
    let archive_git_dir = ensure_archive_git(paths, &repo_id).await?;
    let archive_id = create_archive_id(&repo_id, &branch_name, &commit_hash);
    let archive_ref = format!("refs/archive/branches/{}", archive_id);
    let data_dir = git_tool_data_dir(paths);

    run_git(
        &[
            "--git-dir",
            &archive_git_dir,
            "fetch",
            "--no-tags",
            &project_path,
            &format!("refs/heads/{}:{}", branch_name, archive_ref),
        ],
        &data_dir,
    )
    .await?;

    let archived_commit_hash = run_git(
        &["--git-dir", &archive_git_dir, "rev-parse", &archive_ref],
        &data_dir,
    )
    .await?;

    if archived_commit_hash != commit_hash {
        return Err(ManagerError::System("归档提交校验失败".to_string()));
    }

    run_git(&["branch", "-D", &branch_name], &project_path).await?;

    let archive = json!({
      "archiveId": archive_id,
      "repoId": repo_id,
      "projectPath": project_path,
      "branchName": branch_name,
      "commitHash": commit_hash,
      "archiveRef": archive_ref,
      "archivedAt": now_millis(),
      "restoredAt": 0
    });
    let mut archives = read_archives(paths, &string_value(archive.get("repoId")))?;

    archives.insert(0, archive.clone());
    write_archives(paths, &string_value(archive.get("repoId")), archives).await?;
    Ok(archive)
}

pub async fn list_archives(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId").or(Some(&payload)));

    get_repo_project(paths, repos, &repo_id)?;
    Ok(json!(read_archives(paths, &repo_id)?))
}

pub async fn list_archive_commits(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let archive_id = string_value(payload.get("archiveId").or(Some(&payload)));
    let (repo_id, archive, _) = find_archive(paths, repos, &archive_id)?;
    let archive_git_dir = get_archive_git_dir(paths, &repo_id);
    let data_dir = git_tool_data_dir(paths);
    let output = run_git(
        &[
            "--git-dir",
            &archive_git_dir,
            "log",
            &string_value(archive.get("archiveRef")),
            "--date=iso-strict",
            "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
            "-n",
            "80",
        ],
        &data_dir,
    )
    .await?;

    Ok(json!(parse_commit_log(&output)?))
}

pub async fn get_archive_commit_detail(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let archive_id = string_value(payload.get("archiveId"));
    let commit_hash = string_value(payload.get("commitHash"));
    let file_path = string_value(payload.get("filePath"));
    let (repo_id, _, _) = find_archive(paths, repos, &archive_id)?;

    get_commit_detail_by_args(
        vec![
            "--git-dir".to_string(),
            get_archive_git_dir(paths, &repo_id),
        ],
        &git_tool_data_dir(paths),
        &commit_hash,
        &file_path,
    )
    .await
}

pub async fn restore_archive(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let archive_id = string_value(payload.get("archiveId"));
    let target_branch_name = string_value(payload.get("targetBranchName"));

    if target_branch_name.is_empty() {
        return Err(ManagerError::System("请输入恢复分支名".to_string()));
    }

    let (repo_id, project, archive) = find_archive_with_project(paths, repos, &archive_id)?;
    let project_path = string_value(project.get("projectPath"));

    if branch_exists(&project_path, &target_branch_name).await? {
        return Err(ManagerError::System(
            "目标分支名已存在，请输入新的分支名".to_string(),
        ));
    }

    let archive_git_dir = get_archive_git_dir(paths, &repo_id);

    run_git(
        &[
            "fetch",
            "--no-tags",
            &archive_git_dir,
            &format!(
                "{}:refs/heads/{}",
                string_value(archive.get("archiveRef")),
                target_branch_name
            ),
        ],
        &project_path,
    )
    .await?;

    let restored_commit_hash = get_branch_commit(&project_path, &target_branch_name).await?;

    if restored_commit_hash != string_value(archive.get("commitHash")) {
        return Err(ManagerError::System("恢复提交校验失败".to_string()));
    }

    run_git(
        &[
            "--git-dir",
            &archive_git_dir,
            "update-ref",
            "-d",
            &string_value(archive.get("archiveRef")),
        ],
        &git_tool_data_dir(paths),
    )
    .await?;

    let archives = read_archives(paths, &repo_id)?
        .into_iter()
        .filter(|item| item.get("archiveId").and_then(Value::as_str) != Some(archive_id.as_str()))
        .collect::<Vec<_>>();
    write_archives(paths, &repo_id, archives).await?;
    Ok(archive)
}

pub async fn delete_archive(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let archive_id = string_value(payload.get("archiveId").or(Some(&payload)));
    let (repo_id, archive, archives) = find_archive(paths, repos, &archive_id)?;
    let archive_git_dir = get_archive_git_dir(paths, &repo_id);

    run_git(
        &[
            "--git-dir",
            &archive_git_dir,
            "update-ref",
            "-d",
            &string_value(archive.get("archiveRef")),
        ],
        &git_tool_data_dir(paths),
    )
    .await?;

    let next_archives = archives
        .into_iter()
        .filter(|item| item.get("archiveId").and_then(Value::as_str) != Some(archive_id.as_str()))
        .collect::<Vec<_>>();
    write_archives(paths, &repo_id, next_archives.clone()).await?;
    Ok(json!(next_archives))
}

pub async fn list_stashes(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId").or(Some(&payload)));
    let project = get_repo_project(paths, repos, &repo_id)?;

    Ok(json!(
        get_stashes(&string_value(project.get("projectPath"))).await?
    ))
}

pub async fn list_stash_archives(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId").or(Some(&payload)));

    get_repo_project(paths, repos, &repo_id)?;
    Ok(json!(read_stash_archives(paths, &repo_id)?))
}

pub async fn get_stash_detail(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let stash_hash = string_value(payload.get("stashHash"));
    let file_path = string_value(payload.get("filePath"));

    if stash_hash.is_empty() {
        return Err(ManagerError::System("请选择要查看的 stash".to_string()));
    }

    let project = get_repo_project(paths, repos, &repo_id)?;
    let project_path = string_value(project.get("projectPath"));
    let stashes = get_stashes(&project_path).await?;
    let Some(stash) = stashes
        .iter()
        .find(|item| item.get("hash").and_then(Value::as_str) == Some(stash_hash.as_str()))
    else {
        return Err(ManagerError::System(
            "stash 记录不存在，请刷新后重试".to_string(),
        ));
    };

    get_stash_commit_detail_by_args(
        Vec::new(),
        &project_path,
        &string_value(stash.get("hash")),
        &file_path,
    )
    .await
}

pub async fn get_stash_archive_detail(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let stash_archive_id = string_value(payload.get("stashArchiveId"));
    let file_path = string_value(payload.get("filePath"));
    let (repo_id, stash_archive, _) = find_stash_archive(paths, repos, &stash_archive_id)?;

    get_stash_commit_detail_by_args(
        vec![
            "--git-dir".to_string(),
            get_archive_git_dir(paths, &repo_id),
        ],
        &git_tool_data_dir(paths),
        &string_value(stash_archive.get("commitHash")),
        &file_path,
    )
    .await
}

pub async fn archive_stash(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let stash_ref = string_value(payload.get("stashRef"));
    let stash_hash = string_value(payload.get("stashHash"));

    if stash_hash.is_empty() {
        return Err(ManagerError::System("请选择要归档的 stash".to_string()));
    }

    let project = get_repo_project(paths, repos, &repo_id)?;
    let project_path = string_value(project.get("projectPath"));
    let stashes = get_stashes(&project_path).await?;
    let Some(stash) = stashes
        .iter()
        .find(|item| item.get("hash").and_then(Value::as_str) == Some(stash_hash.as_str()))
    else {
        return Err(ManagerError::System(
            "stash 记录不存在，请刷新后重试".to_string(),
        ));
    };
    let commit_hash = get_stash_commit(&project_path, &string_value(stash.get("stashRef"))).await?;

    if commit_hash != string_value(stash.get("hash")) {
        return Err(ManagerError::System(
            "stash 记录已变化，请刷新后重试".to_string(),
        ));
    }

    let archive_git_dir = ensure_archive_git(paths, &repo_id).await?;
    let stash_archive_id = create_stash_archive_id(&repo_id, &commit_hash);
    let archive_ref = format!("refs/archive/stashes/{}", stash_archive_id);
    let data_dir = git_tool_data_dir(paths);

    run_git(
        &[
            "--git-dir",
            &archive_git_dir,
            "fetch",
            "--no-tags",
            &project_path,
            &format!("{}:{}", commit_hash, archive_ref),
        ],
        &data_dir,
    )
    .await?;

    let archived_commit_hash = run_git(
        &["--git-dir", &archive_git_dir, "rev-parse", &archive_ref],
        &data_dir,
    )
    .await?;

    if archived_commit_hash != commit_hash {
        return Err(ManagerError::System("stash 归档提交校验失败".to_string()));
    }

    run_git(
        &["stash", "drop", &string_value(stash.get("stashRef"))],
        &project_path,
    )
    .await?;

    let stash_archive = json!({
      "stashArchiveId": stash_archive_id,
      "repoId": repo_id,
      "projectPath": project_path,
      "stashRef": stash_ref,
      "message": stash["subject"],
      "commitHash": commit_hash,
      "archiveRef": archive_ref,
      "archivedAt": now_millis(),
      "restoredAt": 0
    });
    let repo_id = string_value(stash_archive.get("repoId"));
    let mut stash_archives = read_stash_archives(paths, &repo_id)?;

    stash_archives.insert(0, stash_archive.clone());
    write_stash_archives(paths, &repo_id, stash_archives).await?;
    Ok(stash_archive)
}

pub async fn restore_stash_archive(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let stash_archive_id = string_value(payload.get("stashArchiveId").or(Some(&payload)));
    let (repo_id, project, stash_archive) =
        find_stash_archive_with_project(paths, repos, &stash_archive_id)?;
    let archive_git_dir = get_archive_git_dir(paths, &repo_id);
    let project_path = string_value(project.get("projectPath"));
    let restore_ref = format!("refs/git-tool/stash-restore/{}", stash_archive_id);

    run_git(
        &[
            "fetch",
            "--no-tags",
            &archive_git_dir,
            &format!(
                "{}:{}",
                string_value(stash_archive.get("archiveRef")),
                restore_ref
            ),
        ],
        &project_path,
    )
    .await?;

    let restored_commit_hash = run_git(&["rev-parse", &restore_ref], &project_path).await?;

    if restored_commit_hash != string_value(stash_archive.get("commitHash")) {
        return Err(ManagerError::System("stash 恢复提交校验失败".to_string()));
    }

    run_git(
        &[
            "stash",
            "store",
            "-m",
            &string_value(stash_archive.get("message")),
            &restore_ref,
        ],
        &project_path,
    )
    .await?;
    run_git(&["update-ref", "-d", &restore_ref], &project_path).await?;
    run_git(
        &[
            "--git-dir",
            &archive_git_dir,
            "update-ref",
            "-d",
            &string_value(stash_archive.get("archiveRef")),
        ],
        &git_tool_data_dir(paths),
    )
    .await?;

    let stash_archives = read_stash_archives(paths, &repo_id)?
        .into_iter()
        .filter(|item| {
            item.get("stashArchiveId").and_then(Value::as_str) != Some(stash_archive_id.as_str())
        })
        .collect::<Vec<_>>();
    write_stash_archives(paths, &repo_id, stash_archives).await?;
    Ok(stash_archive)
}

pub async fn delete_stash_archive(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let stash_archive_id = string_value(payload.get("stashArchiveId").or(Some(&payload)));
    let (repo_id, stash_archive, stash_archives) =
        find_stash_archive(paths, repos, &stash_archive_id)?;
    let archive_git_dir = get_archive_git_dir(paths, &repo_id);

    run_git(
        &[
            "--git-dir",
            &archive_git_dir,
            "update-ref",
            "-d",
            &string_value(stash_archive.get("archiveRef")),
        ],
        &git_tool_data_dir(paths),
    )
    .await?;

    let next_stash_archives = stash_archives
        .into_iter()
        .filter(|item| {
            item.get("stashArchiveId").and_then(Value::as_str) != Some(stash_archive_id.as_str())
        })
        .collect::<Vec<_>>();
    write_stash_archives(paths, &repo_id, next_stash_archives.clone()).await?;
    Ok(json!(next_stash_archives))
}

fn git_tool_data_dir(paths: &AppPaths) -> String {
    path_text(Path::new(&paths.workspace_root).join("git-tool"))
}

fn git_tool_projects_dir(paths: &AppPaths) -> String {
    path_text(Path::new(&git_tool_data_dir(paths)).join("projects"))
}

fn projects_file(paths: &AppPaths) -> String {
    path_text(Path::new(&git_tool_data_dir(paths)).join("projects.json"))
}

fn get_project_dir(paths: &AppPaths, repo_id: &str) -> String {
    path_text(
        Path::new(&git_tool_projects_dir(paths)).join(
            Path::new(repo_id)
                .file_name()
                .map(|item| item.to_string_lossy().to_string())
                .unwrap_or_default(),
        ),
    )
}

fn get_archive_git_dir(paths: &AppPaths, repo_id: &str) -> String {
    path_text(Path::new(&get_project_dir(paths, repo_id)).join("archive.git"))
}

fn archives_file(paths: &AppPaths, repo_id: &str) -> String {
    path_text(Path::new(&get_project_dir(paths, repo_id)).join("archives.json"))
}

fn stash_archives_file(paths: &AppPaths, repo_id: &str) -> String {
    path_text(Path::new(&get_project_dir(paths, repo_id)).join("stash-archives.json"))
}

fn commit_check_cache_file(paths: &AppPaths, repo_id: &str) -> String {
    path_text(Path::new(&get_project_dir(paths, repo_id)).join("commit-check-cache.json"))
}

fn get_repo_project(paths: &AppPaths, repos: &Value, repo_id: &str) -> Result<Value, ManagerError> {
    let repo = repos
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(repo_id))
        })
        .cloned()
        .ok_or_else(|| ManagerError::System("仓库不存在".to_string()))?;
    let project_path = normalize_project_path(&string_value(repo.get("localPath")));
    let projects = read_projects(paths)?;
    let project = projects
        .iter()
        .find(|item| item.get("repoId").and_then(Value::as_str) == Some(repo_id))
        .cloned()
        .unwrap_or_else(|| json!({}));

    Ok(json!({
      "repoId": repo_id,
      "name": repo["name"],
      "projectPath": project_path,
      "gitPath": string_value(project.get("gitPath")),
      "originUrl": first_non_empty(&[
        string_value(project.get("originUrl")),
        string_value(repo.get("source")),
      ]),
      "checkBranchName": string_value(project.get("checkBranchName")),
      "addedAt": project
        .get("addedAt")
        .and_then(Value::as_u64)
        .or_else(|| repo.get("createdAt").and_then(Value::as_u64))
        .unwrap_or_else(|| now_millis() as u64),
      "lastScannedAt": project
        .get("lastScannedAt")
        .and_then(Value::as_u64)
        .or_else(|| repo.get("lastSyncedAt").and_then(Value::as_u64))
        .unwrap_or(0)
    }))
}

async fn save_project_patch(
    paths: &AppPaths,
    repos: &Value,
    repo_id: &str,
    patch: Value,
) -> Result<Value, ManagerError> {
    let repo = repos
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(repo_id))
        })
        .cloned()
        .ok_or_else(|| ManagerError::System("仓库不存在".to_string()))?;
    let mut projects = read_projects(paths)?;
    let index = projects
        .iter()
        .position(|item| item.get("repoId").and_then(Value::as_str) == Some(repo_id));
    let previous = index
        .and_then(|index| projects.get(index).cloned())
        .unwrap_or_else(|| {
            json!({
              "repoId": repo_id,
              "name": repo["name"],
              "projectPath": normalize_project_path(&string_value(repo.get("localPath"))),
              "checkBranchName": "",
              "addedAt": repo.get("createdAt").cloned().unwrap_or_else(|| json!(now_millis()))
            })
        });
    let mut next_project = previous.as_object().cloned().unwrap_or_default();

    if let Some(patch) = patch.as_object() {
        for (key, value) in patch {
            next_project.insert(key.clone(), value.clone());
        }
    }

    next_project.insert("repoId".to_string(), json!(repo_id));
    next_project.insert("name".to_string(), repo["name"].clone());
    next_project.insert(
        "projectPath".to_string(),
        json!(normalize_project_path(&string_value(repo.get("localPath")))),
    );

    let next_project = Value::Object(next_project);

    if let Some(index) = index {
        projects[index] = next_project.clone();
    } else {
        projects.push(next_project.clone());
    }

    write_projects(paths, projects).await?;
    Ok(next_project)
}

fn normalize_project_path(project_path: &str) -> String {
    std::fs::canonicalize(project_path)
        .map(path_text)
        .unwrap_or_else(|_| path_text(project_path))
}

async fn branch_exists(project_path: &str, branch_name: &str) -> Result<bool, ManagerError> {
    match run_git(
        &[
            "show-ref",
            "--verify",
            &format!("refs/heads/{}", branch_name),
        ],
        project_path,
    )
    .await
    {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

async fn get_branch_commit(project_path: &str, branch_name: &str) -> Result<String, ManagerError> {
    run_git(
        &["rev-parse", &format!("refs/heads/{}", branch_name)],
        project_path,
    )
    .await
}

async fn get_current_branch(project_path: &str) -> Result<String, ManagerError> {
    run_git(&["branch", "--show-current"], project_path).await
}

async fn get_local_branch_scan(project_path: &str) -> Result<Value, ManagerError> {
    let output = run_git(
        &[
            "for-each-ref",
            "--format=%(HEAD)%00%(refname:short)%00%(objectname)",
            "refs/heads",
        ],
        project_path,
    )
    .await?;

    if output.is_empty() {
        return Ok(json!({
          "currentBranch": "",
          "branches": []
        }));
    }

    let mut current_branch = String::new();
    let branches = output
        .lines()
        .map(|line| {
            let parts = line.split('\0').collect::<Vec<_>>();
            let head = parts.first().copied().unwrap_or("");
            let name = parts.get(1).copied().unwrap_or("").to_string();
            let commit_hash = parts.get(2).copied().unwrap_or("").to_string();
            let is_current = head.trim() == "*";

            if is_current {
                current_branch = name.clone();
            }

            json!({
              "name": name,
              "commitHash": commit_hash,
              "isCurrent": is_current
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
      "currentBranch": current_branch,
      "branches": branches
    }))
}

async fn get_commits(project_path: &str, branch_name: &str) -> Result<Vec<Value>, ManagerError> {
    let output = run_git(
        &[
            "log",
            "--graph",
            branch_name,
            "--date=iso-strict",
            "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
            "-n",
            "80",
        ],
        project_path,
    )
    .await?;

    parse_commit_log(&output)
}

fn parse_commit_log(output: &str) -> Result<Vec<Value>, ManagerError> {
    if output.is_empty() {
        return Ok(Vec::new());
    }

    output
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if !line.contains('\0') {
                return Ok(json!({
                  "rowId": format!("graph-{}", index),
                  "hash": "",
                  "shortHash": "",
                  "subject": "",
                  "author": "",
                  "date": "",
                  "graph": line.trim_end(),
                  "isGraphOnly": true,
                  "checkStatus": "none",
                  "checkTargetBranch": ""
                }));
            }

            let parts = line.split('\0').collect::<Vec<_>>();
            let hash_text = parts.first().copied().unwrap_or("");
            let hash = hash_text
                .split_whitespace()
                .last()
                .filter(|value| value.len() == 40 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
                .ok_or_else(|| ManagerError::System("提交日志解析失败".to_string()))?;
            let graph = hash_text
                .strip_suffix(hash)
                .unwrap_or("")
                .trim_end()
                .to_string();

            Ok(json!({
              "rowId": hash,
              "hash": hash,
              "shortHash": parts.get(1).copied().unwrap_or(""),
              "subject": parts.get(2).copied().unwrap_or(""),
              "author": parts.get(3).copied().unwrap_or(""),
              "date": parts.get(4).copied().unwrap_or(""),
              "graph": graph,
              "isGraphOnly": false,
              "checkStatus": "none",
              "checkTargetBranch": ""
            }))
        })
        .collect()
}

async fn find_commit_by_subject_and_date(
    project_path: &str,
    branch_name: &str,
    subject: &str,
    date: &str,
) -> Result<Value, ManagerError> {
    let output = match run_git(
        &[
            "log",
            branch_name,
            "--date=iso-strict",
            "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
            "--fixed-strings",
            "--grep",
            subject,
        ],
        project_path,
    )
    .await
    {
        Ok(output) => output,
        Err(_) => return Ok(Value::Null),
    };

    Ok(parse_commit_log(&output)?
        .into_iter()
        .find(|item| {
            string_value(item.get("subject")) == subject && string_value(item.get("date")) == date
        })
        .unwrap_or(Value::Null))
}

async fn check_commits_on_branch(
    project_path: &str,
    branch_name: &str,
    commits: Vec<Value>,
) -> Result<Value, ManagerError> {
    let target_output = run_git(
        &[
            "log",
            branch_name,
            "--date=iso-strict",
            "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
        ],
        project_path,
    )
    .await?;
    let target_commits = parse_commit_log(&target_output)?
        .into_iter()
        .filter(|item| item.get("isGraphOnly").and_then(Value::as_bool) != Some(true))
        .collect::<Vec<_>>();
    let target_hash_map = target_commits
        .iter()
        .map(|commit| (string_value(commit.get("hash")), commit.clone()))
        .collect::<HashMap<_, _>>();
    let mut target_subject_date_map = HashMap::new();

    for commit in &target_commits {
        let subject_date_key = format!(
            "{}\u{0}{}",
            string_value(commit.get("subject")),
            string_value(commit.get("date"))
        );

        target_subject_date_map
            .entry(subject_date_key)
            .or_insert_with(|| commit.clone());
    }

    let mut checked_commits = Vec::new();
    let mut matched_commits = Vec::new();

    for commit in commits {
        if commit.get("isGraphOnly").and_then(Value::as_bool) == Some(true) {
            checked_commits.push(commit);
            continue;
        }

        let commit_hash = string_value(commit.get("hash"));

        if target_hash_map.contains_key(&commit_hash) {
            let mut checked_commit = commit.clone();

            checked_commit["checkStatus"] = json!("exists-hash");
            checked_commit["checkTargetBranch"] = json!(branch_name);
            checked_commits.push(checked_commit.clone());
            matched_commits.push(json!({
              "commitHash": commit_hash,
              "subject": commit["subject"],
              "date": commit["date"],
              "targetBranchName": branch_name,
              "matchedBy": "hash",
              "matchedCommit": checked_commit
            }));
            continue;
        }

        let matched_commit = target_subject_date_map.get(&format!(
            "{}\u{0}{}",
            string_value(commit.get("subject")),
            string_value(commit.get("date"))
        ));
        let mut checked_commit = commit.clone();

        checked_commit["checkStatus"] = json!(if matched_commit.is_some() {
            "exists-subject"
        } else {
            "missing"
        });
        checked_commit["checkTargetBranch"] = json!(branch_name);
        checked_commits.push(checked_commit);

        if let Some(matched_commit) = matched_commit {
            matched_commits.push(json!({
              "commitHash": commit_hash,
              "subject": commit["subject"],
              "date": commit["date"],
              "targetBranchName": branch_name,
              "matchedBy": "subject-date",
              "matchedCommit": {
                "rowId": matched_commit["rowId"],
                "hash": matched_commit["hash"],
                "shortHash": matched_commit["shortHash"],
                "subject": matched_commit["subject"],
                "author": matched_commit["author"],
                "date": matched_commit["date"],
                "graph": matched_commit["graph"],
                "isGraphOnly": matched_commit["isGraphOnly"],
                "checkStatus": "exists-subject",
                "checkTargetBranch": branch_name
              }
            }));
        }
    }

    Ok(json!({
      "checkedCommits": checked_commits,
      "matchedCommits": matched_commits
    }))
}

async fn check_commit_on_branch(
    project_path: &str,
    branch_name: &str,
    commit_hash: &str,
    subject: &str,
    date: &str,
) -> Result<Value, ManagerError> {
    let hash_result = run_git(
        &["merge-base", "--is-ancestor", commit_hash, branch_name],
        project_path,
    )
    .await;

    if hash_result.is_ok() {
        let output = run_git(
            &[
                "show",
                "-s",
                "--date=iso-strict",
                "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
                commit_hash,
            ],
            project_path,
        )
        .await?;

        return Ok(json!({
          "matchedBy": "hash",
          "commit": parse_commit_log(&output)?
            .into_iter()
            .next()
            .unwrap_or(Value::Null)
        }));
    }

    let matched_commit =
        find_commit_by_subject_and_date(project_path, branch_name, subject, date).await?;

    if matched_commit.is_null() {
        return Ok(Value::Null);
    }

    Ok(json!({
      "matchedBy": "subject-date",
      "commit": matched_commit
    }))
}

fn parse_commit_files(output: &str) -> Vec<Value> {
    if output.is_empty() {
        return Vec::new();
    }

    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            let status = parts.first()?.chars().next()?.to_string();

            if status == "R" || status == "C" {
                return Some(json!({
                  "status": status,
                  "path": parts.get(2).copied().unwrap_or(""),
                  "oldPath": parts.get(1).copied().unwrap_or("")
                }));
            }

            Some(json!({
              "status": status,
              "path": parts.get(1).copied().unwrap_or(""),
              "oldPath": ""
            }))
        })
        .filter(|file| !string_value(file.get("path")).is_empty())
        .collect()
}

async fn get_commit_detail_by_args(
    base_args: Vec<String>,
    cwd: &str,
    commit_hash: &str,
    file_path: &str,
) -> Result<Value, ManagerError> {
    let mut info_args = base_args.clone();
    info_args.extend([
        "show".to_string(),
        "-s".to_string(),
        "--date=iso-strict".to_string(),
        "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad".to_string(),
        commit_hash.to_string(),
    ]);
    let info_output = run_git_vec(&info_args, cwd).await?;
    let info_parts = info_output.split('\0').collect::<Vec<_>>();
    let mut files_args = base_args.clone();
    files_args.extend([
        "show".to_string(),
        "--format=".to_string(),
        "--name-status".to_string(),
        "--first-parent".to_string(),
        "--find-renames".to_string(),
        "--find-copies".to_string(),
        commit_hash.to_string(),
    ]);
    let files = parse_commit_files(&run_git_vec(&files_args, cwd).await?);
    let selected_file_path = if file_path.is_empty() {
        files
            .first()
            .map(|file| string_value(file.get("path")))
            .unwrap_or_default()
    } else {
        file_path.to_string()
    };
    let mut patch_args = base_args;

    patch_args.extend([
        "show".to_string(),
        "--format=".to_string(),
        "--patch".to_string(),
        "--first-parent".to_string(),
        "--find-renames".to_string(),
        "--find-copies".to_string(),
        "--no-ext-diff".to_string(),
        "--no-color".to_string(),
        commit_hash.to_string(),
    ]);

    if !selected_file_path.is_empty() {
        patch_args.push("--".to_string());
        patch_args.push(selected_file_path.clone());
    }

    Ok(json!({
      "rowId": info_parts.first().copied().unwrap_or(""),
      "hash": info_parts.first().copied().unwrap_or(""),
      "shortHash": info_parts.get(1).copied().unwrap_or(""),
      "subject": info_parts.get(2).copied().unwrap_or(""),
      "author": info_parts.get(3).copied().unwrap_or(""),
      "date": info_parts.get(4).copied().unwrap_or(""),
      "graph": "",
      "isGraphOnly": false,
      "checkStatus": "none",
      "checkTargetBranch": "",
      "files": files,
      "selectedFilePath": selected_file_path,
      "patch": run_git_raw_vec(&patch_args, cwd).await?.trim_end()
    }))
}

async fn get_stashes(project_path: &str) -> Result<Vec<Value>, ManagerError> {
    let output = run_git(
        &[
            "stash",
            "list",
            "--date=iso-strict",
            "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
        ],
        project_path,
    )
    .await?;

    if output.is_empty() {
        return Ok(Vec::new());
    }

    Ok(output
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let parts = line.split('\0').collect::<Vec<_>>();

            json!({
              "stashRef": format!("stash@{{{}}}", index),
              "index": index,
              "hash": parts.first().copied().unwrap_or(""),
              "shortHash": parts.get(1).copied().unwrap_or(""),
              "subject": parts.get(2).copied().unwrap_or(""),
              "author": parts.get(3).copied().unwrap_or(""),
              "date": parts.get(4).copied().unwrap_or("")
            })
        })
        .collect())
}

async fn get_stash_commit(project_path: &str, stash_ref: &str) -> Result<String, ManagerError> {
    run_git(&["rev-parse", stash_ref], project_path).await
}

async fn get_stash_commit_detail_by_args(
    base_args: Vec<String>,
    cwd: &str,
    commit_hash: &str,
    file_path: &str,
) -> Result<Value, ManagerError> {
    let mut info_args = base_args.clone();
    info_args.extend([
        "show".to_string(),
        "-s".to_string(),
        "--date=iso-strict".to_string(),
        "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad".to_string(),
        commit_hash.to_string(),
    ]);
    let info_output = run_git_vec(&info_args, cwd).await?;
    let info_parts = info_output.split('\0').collect::<Vec<_>>();
    let mut parent_args = base_args.clone();
    parent_args.extend([
        "rev-list".to_string(),
        "--parents".to_string(),
        "-n".to_string(),
        "1".to_string(),
        commit_hash.to_string(),
    ]);
    let parent_output = run_git_vec(&parent_args, cwd).await?;
    let parents = parent_output.split(' ').skip(1).collect::<Vec<_>>();

    if parents.is_empty() {
        return Err(ManagerError::System("stash 结构解析失败".to_string()));
    }

    let mut tracked_args = base_args.clone();
    tracked_args.extend([
        "diff".to_string(),
        "--name-status".to_string(),
        "--find-renames".to_string(),
        "--find-copies".to_string(),
        parents[0].to_string(),
        commit_hash.to_string(),
    ]);
    let tracked_files = parse_commit_files(&run_git_vec(&tracked_args, cwd).await?);
    let untracked_files = if parents.len() >= 3 {
        let mut untracked_args = base_args.clone();
        untracked_args.extend([
            "diff".to_string(),
            "--name-status".to_string(),
            "--find-renames".to_string(),
            "--find-copies".to_string(),
            EMPTY_TREE_HASH.to_string(),
            parents[2].to_string(),
        ]);
        parse_commit_files(&run_git_vec(&untracked_args, cwd).await?)
    } else {
        Vec::new()
    };
    let files = tracked_files
        .iter()
        .chain(untracked_files.iter())
        .cloned()
        .collect::<Vec<_>>();
    let selected_file_path = if file_path.is_empty() {
        files
            .first()
            .map(|file| string_value(file.get("path")))
            .unwrap_or_default()
    } else {
        file_path.to_string()
    };
    let selected_in_untracked = untracked_files
        .iter()
        .any(|file| string_value(file.get("path")) == selected_file_path);
    let mut patch_args = base_args;

    patch_args.extend([
        "diff".to_string(),
        "--patch".to_string(),
        "--find-renames".to_string(),
        "--find-copies".to_string(),
        "--no-ext-diff".to_string(),
        "--no-color".to_string(),
        if selected_in_untracked {
            EMPTY_TREE_HASH.to_string()
        } else {
            parents[0].to_string()
        },
        if selected_in_untracked {
            parents[2].to_string()
        } else {
            commit_hash.to_string()
        },
    ]);

    if !selected_file_path.is_empty() {
        patch_args.push("--".to_string());
        patch_args.push(selected_file_path.clone());
    }

    Ok(json!({
      "rowId": info_parts.first().copied().unwrap_or(""),
      "hash": info_parts.first().copied().unwrap_or(""),
      "shortHash": info_parts.get(1).copied().unwrap_or(""),
      "subject": info_parts.get(2).copied().unwrap_or(""),
      "author": info_parts.get(3).copied().unwrap_or(""),
      "date": info_parts.get(4).copied().unwrap_or(""),
      "graph": "",
      "isGraphOnly": false,
      "checkStatus": "none",
      "checkTargetBranch": "",
      "files": files,
      "selectedFilePath": selected_file_path,
      "patch": run_git_raw_vec(&patch_args, cwd).await?.trim_end()
    }))
}

async fn ensure_archive_git(paths: &AppPaths, repo_id: &str) -> Result<String, ManagerError> {
    let archive_git_dir = get_archive_git_dir(paths, repo_id);

    if Path::new(&archive_git_dir).join("HEAD").exists() {
        return Ok(archive_git_dir);
    }

    tokio::fs::create_dir_all(
        Path::new(&archive_git_dir)
            .parent()
            .unwrap_or_else(|| Path::new(&archive_git_dir)),
    )
    .await?;
    run_git(
        &["init", "--bare", &archive_git_dir],
        &git_tool_data_dir(paths),
    )
    .await?;
    Ok(archive_git_dir)
}

fn find_archive(
    paths: &AppPaths,
    repos: &Value,
    archive_id: &str,
) -> Result<(String, Value, Vec<Value>), ManagerError> {
    let repo_items = repos.as_array().cloned().unwrap_or_default();

    for repo in repo_items {
        let repo_id = string_value(repo.get("id"));
        let archives = read_archives(paths, &repo_id)?;

        if let Some(archive) = archives
            .iter()
            .find(|item| item.get("archiveId").and_then(Value::as_str) == Some(archive_id))
            .cloned()
        {
            return Ok((repo_id, archive, archives));
        }
    }

    Err(ManagerError::System("未找到归档记录".to_string()))
}

fn find_archive_with_project(
    paths: &AppPaths,
    repos: &Value,
    archive_id: &str,
) -> Result<(String, Value, Value), ManagerError> {
    let (repo_id, archive, _) = find_archive(paths, repos, archive_id)?;
    let project = get_repo_project(paths, repos, &repo_id)?;

    Ok((repo_id, project, archive))
}

fn find_stash_archive(
    paths: &AppPaths,
    repos: &Value,
    stash_archive_id: &str,
) -> Result<(String, Value, Vec<Value>), ManagerError> {
    let repo_items = repos.as_array().cloned().unwrap_or_default();

    for repo in repo_items {
        let repo_id = string_value(repo.get("id"));
        let stash_archives = read_stash_archives(paths, &repo_id)?;

        if let Some(stash_archive) = stash_archives
            .iter()
            .find(|item| {
                item.get("stashArchiveId").and_then(Value::as_str) == Some(stash_archive_id)
            })
            .cloned()
        {
            return Ok((repo_id, stash_archive, stash_archives));
        }
    }

    Err(ManagerError::System("未找到 stash 归档记录".to_string()))
}

fn find_stash_archive_with_project(
    paths: &AppPaths,
    repos: &Value,
    stash_archive_id: &str,
) -> Result<(String, Value, Value), ManagerError> {
    let (repo_id, stash_archive, _) = find_stash_archive(paths, repos, stash_archive_id)?;
    let project = get_repo_project(paths, repos, &repo_id)?;

    Ok((repo_id, project, stash_archive))
}

fn read_projects(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    read_json_array(&projects_file(paths))
}

async fn write_projects(paths: &AppPaths, projects: Vec<Value>) -> Result<(), ManagerError> {
    write_json(&projects_file(paths), &json!(projects)).await
}

fn read_archives(paths: &AppPaths, repo_id: &str) -> Result<Vec<Value>, ManagerError> {
    read_json_array(&archives_file(paths, repo_id))
}

async fn write_archives(
    paths: &AppPaths,
    repo_id: &str,
    archives: Vec<Value>,
) -> Result<(), ManagerError> {
    write_json(&archives_file(paths, repo_id), &json!(archives)).await
}

fn read_stash_archives(paths: &AppPaths, repo_id: &str) -> Result<Vec<Value>, ManagerError> {
    read_json_array(&stash_archives_file(paths, repo_id))
}

async fn write_stash_archives(
    paths: &AppPaths,
    repo_id: &str,
    stash_archives: Vec<Value>,
) -> Result<(), ManagerError> {
    write_json(&stash_archives_file(paths, repo_id), &json!(stash_archives)).await
}

fn read_commit_check_cache(paths: &AppPaths, repo_id: &str) -> Result<Vec<Value>, ManagerError> {
    read_json_array(&commit_check_cache_file(paths, repo_id))
}

async fn write_commit_check_cache(
    paths: &AppPaths,
    repo_id: &str,
    commit_check_cache: Vec<Value>,
) -> Result<(), ManagerError> {
    write_json(
        &commit_check_cache_file(paths, repo_id),
        &json!(commit_check_cache),
    )
    .await
}

fn read_json_array(path: &str) -> Result<Vec<Value>, ManagerError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
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

async fn run_git(args: &[&str], cwd: &str) -> Result<String, ManagerError> {
    Ok(run_git_raw(args, cwd).await?.trim().to_string())
}

async fn run_git_raw(args: &[&str], cwd: &str) -> Result<String, ManagerError> {
    let mut command = Command::new("git");

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();

        return Err(ManagerError::System(if message.is_empty() {
            "Git 命令执行失败".to_string()
        } else {
            message
        }));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_git_vec(args: &[String], cwd: &str) -> Result<String, ManagerError> {
    Ok(run_git_raw_vec(args, cwd).await?.trim().to_string())
}

async fn run_git_raw_vec(args: &[String], cwd: &str) -> Result<String, ManagerError> {
    let mut command = Command::new("git");

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();

        return Err(ManagerError::System(if message.is_empty() {
            "Git 命令执行失败".to_string()
        } else {
            message
        }));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn create_archive_id(repo_id: &str, branch_name: &str, commit_hash: &str) -> String {
    sha1_hex(&format!(
        "{}:{}:{}:{}",
        repo_id,
        branch_name,
        commit_hash,
        now_millis()
    ))[..16]
        .to_string()
}

fn create_stash_archive_id(repo_id: &str, stash_hash: &str) -> String {
    sha1_hex(&format!(
        "{}:stash:{}:{}",
        repo_id,
        stash_hash,
        now_millis()
    ))[..16]
        .to_string()
}

fn sha1_hex(value: &str) -> String {
    let mut hasher = Sha1::new();

    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

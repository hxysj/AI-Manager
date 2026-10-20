use super::{get_stashes, run_git, run_git_output, run_git_raw, run_git_raw_vec, string_value, worktrees};
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

pub async fn get_status(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let project = worktrees::project(paths, repos, &string_value(payload.get("repoId")), &payload).await?;
    let mut status = read_status(&string_value(project.get("projectPath"))).await?;
    status["worktrees"] = project["worktrees"].clone();
    Ok(status)
}

pub async fn get_worktree_diff(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let cwd = project_directory(paths, repos, &payload).await?;
    worktree_diff(&cwd, &payload).await
}

pub async fn get_branch_diff(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let cwd = project_directory(paths, repos, &payload).await?;
    branch_diff(&cwd, &payload).await
}

pub async fn preview_revision(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let cwd = project_directory(paths, repos, &payload).await?;
    revision_preview(&cwd, &string_value(payload.get("revision"))).await
}

pub async fn execute_operation(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let cwd = project_directory(paths, repos, &payload).await?;
    execute_in_repository(&cwd, &payload).await
}

async fn project_directory(
    paths: &AppPaths,
    repos: &Value,
    payload: &Value,
) -> Result<String, ManagerError> {
    let project = worktrees::project(paths, repos, &string_value(payload.get("repoId")), payload).await?;
    Ok(string_value(project.get("projectPath")))
}

// 使用 NUL 分隔记录，保留重命名以及带空格、换行和中文的文件路径。
fn parse_status(output: &str) -> Value {
    let mut status = json!({
        "branch": "", "headHash": "", "upstream": "", "ahead": 0, "behind": 0,
        "detached": false, "hasHead": false, "files": [], "remotes": [], "operation": ""
    });
    let records = output.split('\0').collect::<Vec<_>>();
    let mut files = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if let Some(hash) = record.strip_prefix("# branch.oid ") {
            if hash != "(initial)" {
                status["headHash"] = json!(hash);
                status["hasHead"] = json!(true);
            }
        } else if let Some(branch) = record.strip_prefix("# branch.head ") {
            status["detached"] = json!(branch == "(detached)");
            status["branch"] = json!(if branch == "(detached)" { "" } else { branch });
        } else if let Some(upstream) = record.strip_prefix("# branch.upstream ") {
            status["upstream"] = json!(upstream);
        } else if let Some(counts) = record.strip_prefix("# branch.ab ") {
            let parts = counts.split_whitespace().collect::<Vec<_>>();
            status["ahead"] = json!(parts
                .first()
                .and_then(|value| value.trim_start_matches('+').parse::<u64>().ok())
                .unwrap_or(0));
            status["behind"] = json!(parts
                .get(1)
                .and_then(|value| value.trim_start_matches('-').parse::<u64>().ok())
                .unwrap_or(0));
        } else if let Some(path) = record.strip_prefix("? ") {
            files.push(json!({ "path": path, "oldPath": "", "indexStatus": "?", "worktreeStatus": "?", "staged": false, "unstaged": true, "untracked": true, "conflicted": false }));
        } else if record.starts_with("1 ") || record.starts_with("2 ") || record.starts_with("u ") {
            let renamed = record.starts_with("2 ");
            let conflicted = record.starts_with("u ");
            let field_count = if renamed {
                10
            } else if conflicted {
                11
            } else {
                9
            };
            let parts = record.splitn(field_count, ' ').collect::<Vec<_>>();
            if parts.len() != field_count {
                continue;
            }
            let xy = parts[1].chars().collect::<Vec<_>>();
            let x = xy.first().copied().unwrap_or('.');
            let y = xy.get(1).copied().unwrap_or('.');
            let old_path = if renamed {
                let value = records.get(index).copied().unwrap_or("");
                index += 1;
                value
            } else {
                ""
            };
            files.push(json!({
                "path": parts[field_count - 1], "oldPath": old_path,
                "indexStatus": x.to_string(), "worktreeStatus": y.to_string(),
                "staged": !conflicted && x != '.', "unstaged": !conflicted && y != '.',
                "untracked": false, "conflicted": conflicted, "submodule": parts[2]
            }));
        }
    }
    status["files"] = json!(files);
    status
}

async fn read_status(cwd: &str) -> Result<Value, ManagerError> {
    let output = run_git_raw(
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ],
        cwd,
    )
    .await?;
    let mut status = parse_status(&output);
    status["projectPath"] = json!(cwd);
    let git_dir = run_git(&["rev-parse", "--absolute-git-dir"], cwd).await?;
    let git_path = Path::new(&git_dir);
    let operation =
        if git_path.join("rebase-merge").exists() || git_path.join("rebase-apply").exists() {
            "rebase"
        } else if git_path.join("CHERRY_PICK_HEAD").exists() {
            "cherry-pick"
        } else if git_path.join("REVERT_HEAD").exists() {
            "revert"
        } else if git_path.join("MERGE_HEAD").exists() {
            "merge"
        } else if git_path.join("sequencer").exists() {
            let todo = std::fs::read_to_string(git_path.join("sequencer/todo")).unwrap_or_default();
            if todo.starts_with("revert ") {
                "revert"
            } else {
                "cherry-pick"
            }
        } else {
            ""
        };
    status["operation"] = json!(operation);
    let output = run_git(&["remote", "-v"], cwd).await?;
    let mut remotes: Vec<Value> = Vec::new();
    for line in output.lines() {
        if let Some((name, rest)) = line.split_once('\t') {
            if let Some((url, kind)) = rest.rsplit_once(' ') {
                let position = remotes
                    .iter()
                    .position(|remote| remote["name"] == name)
                    .unwrap_or_else(|| {
                        remotes.push(json!({ "name": name, "fetchUrl": "", "pushUrl": "" }));
                        remotes.len() - 1
                    });
                remotes[position][if kind == "(push)" {
                    "pushUrl"
                } else {
                    "fetchUrl"
                }] = json!(url);
            }
        }
    }
    status["remotes"] = json!(remotes);
    Ok(status)
}

async fn resolve_commit(cwd: &str, revision: &str) -> Result<String, ManagerError> {
    if revision.trim().is_empty() {
        return Err(ManagerError::System("请选择目标分支或提交".to_string()));
    }
    run_git(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", revision.trim()),
        ],
        cwd,
    )
    .await
}

async fn revision_preview(cwd: &str, revision: &str) -> Result<Value, ManagerError> {
    let hash = resolve_commit(cwd, revision).await?;
    let info = run_git(
        &[
            "show",
            "--no-patch",
            "--format=%H%x00%s%x00%an%x00%ad%x00%P",
            "--date=iso-strict",
            &hash,
        ],
        cwd,
    )
    .await?;
    let parts = info.split('\0').collect::<Vec<_>>();
    let head = read_status(cwd).await?;
    let changed = if head["hasHead"] == true {
        run_git_raw(
            &[
                "diff",
                "--name-only",
                "--no-renames",
                "-z",
                "HEAD",
                &hash,
                "--",
            ],
            cwd,
        )
        .await?
    } else {
        String::new()
    };
    Ok(json!({
        "hash": hash, "subject": parts.get(1).copied().unwrap_or(""), "author": parts.get(2).copied().unwrap_or(""),
        "date": parts.get(3).copied().unwrap_or(""), "parents": parts.get(4).copied().unwrap_or("").split_whitespace().collect::<Vec<_>>(),
        "changedFiles": changed.split('\0').filter(|path| !path.is_empty()).collect::<Vec<_>>()
    }))
}

async fn worktree_diff(cwd: &str, payload: &Value) -> Result<Value, ManagerError> {
    let status = read_status(cwd).await?;
    let path = string_value(payload.get("filePath"));
    let staged = payload
        .get("staged")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file = status["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == path)
        .ok_or_else(|| ManagerError::System("文件状态已变化，请刷新工作区".to_string()))?;
    let mut args = vec![
        "--literal-pathspecs".to_string(),
        "diff".to_string(),
        "--no-ext-diff".to_string(),
        "--no-textconv".to_string(),
        "--no-color".to_string(),
    ];
    if file["untracked"] == true && !staged {
        args.extend([
            "--no-index".to_string(),
            "--".to_string(),
            "/dev/null".to_string(),
            path.clone(),
        ]);
    } else {
        if staged {
            args.push("--cached".to_string());
        } else if file["conflicted"] == true {
            args.push("--ours".to_string());
        }
        args.extend(["--".to_string(), path.clone()]);
        let old_path = string_value(file.get("oldPath"));
        if !old_path.is_empty() {
            args.push(old_path);
        }
    }
    let output = run_git_output(&args, cwd).await?;
    if !output.status.success() && !(file["untracked"] == true && output.status.code() == Some(1)) {
        return Err(ManagerError::System(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(
        json!({ "selectedFilePath": path, "patch": String::from_utf8_lossy(&output.stdout), "staged": staged }),
    )
}

async fn branch_diff(cwd: &str, payload: &Value) -> Result<Value, ManagerError> {
    let base_branch = string_value(payload.get("baseBranch"));
    let target_branch = string_value(payload.get("targetBranch"));
    if base_branch.is_empty() || target_branch.is_empty() {
        return Err(ManagerError::System("请选择基准分支和对比分支".to_string()));
    }
    let base_hash = resolve_commit(cwd, &base_branch).await?;
    let target_hash = resolve_commit(cwd, &target_branch).await?;
    let numstat_args = vec![
        "diff".to_string(),
        "--no-ext-diff".to_string(),
        "--no-textconv".to_string(),
        "--numstat".to_string(),
        "-z".to_string(),
        "--no-renames".to_string(),
        base_hash.clone(),
        target_hash.clone(),
        "--".to_string(),
    ];
    let numstat = run_git_raw_vec(&numstat_args, cwd).await?;
    let status_args = vec![
        "diff".to_string(),
        "--no-ext-diff".to_string(),
        "--no-textconv".to_string(),
        "--name-status".to_string(),
        "-z".to_string(),
        "--no-renames".to_string(),
        base_hash.clone(),
        target_hash.clone(),
        "--".to_string(),
    ];
    let status_output = run_git_raw_vec(&status_args, cwd).await?;
    let status_records = status_output
        .split('\0')
        .filter(|record| !record.is_empty())
        .collect::<Vec<_>>();
    let mut statuses = HashMap::new();
    let mut status_index = 0;
    while status_index + 1 < status_records.len() {
        statuses.insert(
            status_records[status_index + 1].to_string(),
            status_records[status_index].to_string(),
        );
        status_index += 2;
    }
    let mut files = Vec::new();
    let mut additions = 0u64;
    let mut deletions = 0u64;
    for record in numstat.split('\0').filter(|record| !record.is_empty()) {
        let mut parts = record.splitn(3, '\t');
        let additions_value = parts.next().unwrap_or("-");
        let deletions_value = parts.next().unwrap_or("-");
        let path = parts.next().unwrap_or("");
        if path.is_empty() {
            continue;
        }
        let binary = additions_value == "-" || deletions_value == "-";
        let file_additions = additions_value.parse::<u64>().unwrap_or(0);
        let file_deletions = deletions_value.parse::<u64>().unwrap_or(0);
        additions += file_additions;
        deletions += file_deletions;
        files.push(json!({
            "path": path,
            "status": statuses.get(path).cloned().unwrap_or_else(|| "M".to_string()),
            "additions": file_additions,
            "deletions": file_deletions,
            "binary": binary
        }));
    }

    let patch_args = vec![
        "diff".to_string(),
        "--no-ext-diff".to_string(),
        "--no-textconv".to_string(),
        "--no-color".to_string(),
        "--unified=3".to_string(),
        base_hash,
        target_hash,
        "--".to_string(),
    ];
    let patch = run_git_raw_vec(&patch_args, cwd)
        .await?
        .trim_end_matches(['\r', '\n'])
        .to_string();

    Ok(json!({
        "baseBranch": base_branch,
        "targetBranch": target_branch,
        "files": files,
        "fileCount": files.len(),
        "additions": additions,
        "deletions": deletions,
        "changedLines": additions + deletions,
        "command": format!("git diff --stat {}..{}", string_value(payload.get("baseBranch")), string_value(payload.get("targetBranch"))),
        "patch": patch
    }))
}

async fn validate_branch(cwd: &str, name: &str) -> Result<(), ManagerError> {
    if name.is_empty() || name.starts_with('-') {
        return Err(ManagerError::System(
            "分支名称不能为空或以 - 开头".to_string(),
        ));
    }
    run_git(&["check-ref-format", "--branch", name], cwd).await?;
    Ok(())
}

// 写入前重新核对 HEAD 和文件清单，防止确认期间切换了分支或路径状态。
async fn execute_in_repository(cwd: &str, payload: &Value) -> Result<Value, ManagerError> {
    let status = read_status(cwd).await?;
    if let Some(expected) = payload.get("expectedWorktreePath").and_then(Value::as_str) {
        if super::normalize_project_path(expected) != super::normalize_project_path(cwd) {
            return Err(ManagerError::System("操作工作树已变化，请重新确认。".to_string()));
        }
    }
    if let Some(head) = payload.get("expectedHead").and_then(Value::as_str) {
        if status["headHash"] != head {
            return Err(ManagerError::System(
                "当前提交已变化，请刷新后重新确认".to_string(),
            ));
        }
    }
    if let Some(branch) = payload.get("expectedBranch").and_then(Value::as_str) {
        if status["branch"] != branch {
            return Err(ManagerError::System(
                "当前分支已变化，请刷新后重新确认".to_string(),
            ));
        }
    }
    let action = string_value(payload.get("action"));
    let operation = string_value(status.get("operation"));
    let files = status["files"].as_array().unwrap();
    let conflicts = files.iter().any(|file| file["conflicted"] == true);
    if !operation.is_empty()
        && !matches!(
            action.as_str(),
            "stage"
                | "unstage"
                | "resolve-ours"
                | "resolve-theirs"
                | "resolve-delete"
                | "continue"
                | "abort"
                | "skip"
        )
    {
        return Err(ManagerError::System(format!(
            "请先完成或中止正在进行的 {} 操作",
            operation
        )));
    }
    if conflicts
        && !matches!(
            action.as_str(),
            "stage" | "resolve-ours" | "resolve-theirs" | "resolve-delete" | "abort" | "skip"
        )
    {
        return Err(ManagerError::System(
            "请先解决并暂存所有冲突文件".to_string(),
        ));
    }
    let mut args: Vec<String>;
    let mut after_args: Option<Vec<String>> = None;
    match action.as_str() {
        "stage" | "unstage" | "discard" | "resolve-ours" | "resolve-theirs" | "resolve-delete" => {
            let requested = payload["filePaths"]
                .as_array()
                .ok_or_else(|| ManagerError::System("请选择文件".to_string()))?;
            if requested.is_empty() {
                return Err(ManagerError::System("请选择文件".to_string()));
            }
            let mut selected_paths = Vec::new();
            for requested_path in requested {
                let path = requested_path
                    .as_str()
                    .ok_or_else(|| ManagerError::System("文件路径格式错误".to_string()))?;
                let file = files
                    .iter()
                    .find(|file| file["path"] == path)
                    .ok_or_else(|| {
                        ManagerError::System("文件状态已变化，请刷新工作区".to_string())
                    })?;
                if path.is_empty()
                    || Path::new(path).is_absolute()
                    || path.split(['/', '\\']).any(|part| part == "..")
                {
                    return Err(ManagerError::System(
                        "文件路径必须位于当前仓库内".to_string(),
                    ));
                }
                if action == "discard" && (file["conflicted"] == true || file["untracked"] == true)
                {
                    return Err(ManagerError::System(
                        "丢弃改动仅支持未冲突的已跟踪文件；未跟踪文件请使用删除操作".to_string(),
                    ));
                }
                if action.starts_with("resolve-") && file["conflicted"] != true {
                    return Err(ManagerError::System("文件已不再处于冲突状态".to_string()));
                }
                selected_paths.push(path.to_string());
                let old_path = string_value(file.get("oldPath"));
                // 已暂存重命名按新路径处理内容，取消暂存才恢复原路径引用。
                if !old_path.is_empty()
                    && ((action == "unstage" && file["indexStatus"] == "R")
                        || (action == "stage"
                            && file["indexStatus"] != "R"
                            && file["indexStatus"] != "C"))
                {
                    selected_paths.push(old_path);
                }
            }
            selected_paths.sort();
            selected_paths.dedup();
            args = vec!["--literal-pathspecs".to_string()];
            args.extend(
                match action.as_str() {
                    "stage" => vec!["add", "-A"],
                    "unstage" if status["hasHead"] == false => {
                        vec!["rm", "--cached", "-f", "--ignore-unmatch"]
                    }
                    "unstage" => vec!["restore", "--staged"],
                    "discard" => vec!["restore", "--worktree"],
                    "resolve-ours" => vec!["restore", "--ours", "--worktree"],
                    "resolve-theirs" => vec!["restore", "--theirs", "--worktree"],
                    _ => vec!["rm", "-f"],
                }
                .into_iter()
                .map(str::to_string),
            );
            args.push("--".to_string());
            args.extend(selected_paths.clone());
            if matches!(action.as_str(), "resolve-ours" | "resolve-theirs") {
                let mut next = vec![
                    "--literal-pathspecs".to_string(),
                    "add".to_string(),
                    "--".to_string(),
                ];
                next.extend(selected_paths);
                after_args = Some(next);
            }
        }
        "delete-untracked" => {
            let requested = payload["filePaths"]
                .as_array()
                .ok_or_else(|| ManagerError::System("请选择未跟踪文件".to_string()))?;
            if requested.is_empty() {
                return Err(ManagerError::System("请选择未跟踪文件".to_string()));
            }
            args = vec![
                "--literal-pathspecs".to_string(),
                "clean".to_string(),
                "-f".to_string(),
                "--".to_string(),
            ];
            for path in requested {
                let path = path
                    .as_str()
                    .ok_or_else(|| ManagerError::System("文件路径格式错误".to_string()))?;
                if !files
                    .iter()
                    .any(|file| file["path"] == path && file["untracked"] == true)
                {
                    return Err(ManagerError::System(
                        "未跟踪文件状态已变化，请刷新工作区".to_string(),
                    ));
                }
                args.push(path.to_string());
            }
        }
        "commit" => {
            let message = string_value(payload.get("message"));
            if message.trim().is_empty() {
                return Err(ManagerError::System("请输入提交信息".to_string()));
            }
            if !files.iter().any(|file| file["staged"] == true) {
                return Err(ManagerError::System("暂存区没有可提交的改动".to_string()));
            }
            args = vec![
                "commit".to_string(),
                "-m".to_string(),
                message.trim().to_string(),
            ];
        }
        "create-branch" => {
            let name = string_value(payload.get("branchName"));
            validate_branch(cwd, &name).await?;
            let revision = string_value(payload.get("revision"));
            let hash = resolve_commit(
                cwd,
                if revision.is_empty() {
                    "HEAD"
                } else {
                    &revision
                },
            )
            .await?;
            args = if payload["switchAfterCreate"] == true {
                vec!["switch".to_string(), "-c".to_string(), name, hash]
            } else {
                vec!["branch".to_string(), name, hash]
            };
        }
        "switch-branch" => {
            let name = string_value(payload.get("branchName"));
            validate_branch(cwd, &name).await?;
            args = vec!["switch".to_string(), "--".to_string(), name];
        }
        "checkout-commit" => {
            let hash = resolve_commit(cwd, &string_value(payload.get("revision"))).await?;
            args = vec!["switch".to_string(), "--detach".to_string(), "--".to_string(), hash];
        }
        "rename-branch" => {
            let name = string_value(payload.get("branchName"));
            validate_branch(cwd, &name).await?;
            let new_name = string_value(payload.get("newBranchName"));
            validate_branch(cwd, &new_name).await?;
            args = vec![
                "branch".to_string(),
                "--move".to_string(),
                "--".to_string(),
                name,
                new_name,
            ];
        }
        "stash-save" => {
            if files.is_empty() {
                return Err(ManagerError::System("工作区没有需要保存的改动".to_string()));
            }
            args = vec!["stash".to_string(), "push".to_string()];
            if payload["includeUntracked"] == true {
                args.push("--include-untracked".to_string());
            }
            let message = string_value(payload.get("message"));
            if !message.trim().is_empty() {
                args.extend(["-m".to_string(), message.trim().to_string()]);
            }
        }
        "stash-apply" | "stash-pop" => {
            let hash = string_value(payload.get("stashHash"));
            let stashes = get_stashes(cwd).await?;
            let stash = stashes
                .iter()
                .find(|stash| stash["hash"] == hash)
                .ok_or_else(|| {
                    ManagerError::System("Stash 已变化或不存在，请刷新后重试".to_string())
                })?;
            args = vec![
                "stash".to_string(),
                if action == "stash-pop" {
                    "pop"
                } else {
                    "apply"
                }
                .to_string(),
            ];
            if payload["restoreIndex"] == true {
                args.push("--index".to_string());
            }
            args.push(string_value(stash.get("stashRef")));
        }
        "fetch" | "pull" | "push" => {
            let remote = string_value(payload.get("remote"));
            if !status["remotes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["name"] == remote)
            {
                return Err(ManagerError::System(
                    "请选择当前仓库已配置的远程仓库".to_string(),
                ));
            }
            if action == "fetch" {
                args = vec![
                    "fetch".to_string(),
                    "--prune".to_string(),
                    "--".to_string(),
                    remote,
                ];
            } else {
                let branch = string_value(payload.get("remoteBranch"));
                validate_branch(cwd, &branch).await?;
                if status["detached"] == true {
                    return Err(ManagerError::System("请先切换到本地分支再同步".to_string()));
                }
                if action == "push" {
                    let local = string_value(status.get("branch"));
                    args = vec![
                        "push".to_string(),
                        "--set-upstream".to_string(),
                        "--".to_string(),
                        remote,
                        format!("refs/heads/{}:refs/heads/{}", local, branch),
                    ];
                } else {
                    if files.iter().any(|file| file["untracked"] != true) {
                        return Err(ManagerError::System(
                            "请先提交或 Stash 未提交改动，再执行 Pull".to_string(),
                        ));
                    }
                    args = vec!["pull".to_string()];
                    args.extend(
                        match string_value(payload.get("pullMode")).as_str() {
                            "merge" => vec!["--no-rebase", "--ff", "--no-edit"],
                            "rebase" => vec!["--rebase", "--ff"],
                            "ff-only" => vec!["--ff-only"],
                            _ => return Err(ManagerError::System("请选择 Pull 策略".to_string())),
                        }
                        .into_iter()
                        .map(str::to_string),
                    );
                    args.extend(["--".to_string(), remote, branch]);
                }
            }
        }
        "merge" | "cherry-pick" | "revert" | "reset" => {
            let hash = resolve_commit(cwd, &string_value(payload.get("revision"))).await?;
            if status["hasHead"] != true {
                return Err(ManagerError::System("请先创建首次提交".to_string()));
            }
            if action == "reset" {
                let mode = string_value(payload.get("resetMode"));
                if !matches!(mode.as_str(), "soft" | "mixed" | "hard") {
                    return Err(ManagerError::System("请选择 Reset 模式".to_string()));
                }
                if mode == "hard" && payload["confirmedDiscard"] != true {
                    return Err(ManagerError::System("请确认丢弃改动的影响范围".to_string()));
                }
                args = vec![
                    "reset".to_string(),
                    format!("--{}", mode),
                    hash,
                    "--".to_string(),
                ];
            } else {
                if files.iter().any(|file| file["untracked"] != true) {
                    return Err(ManagerError::System(
                        "请先提交或 Stash 未提交改动".to_string(),
                    ));
                }
                args = vec![action.clone()];
                if action != "cherry-pick" {
                    args.push("--no-edit".to_string());
                }
                if action != "merge" {
                    let parents =
                        run_git(&["show", "--no-patch", "--format=%P", &hash], cwd).await?;
                    let parent_count = parents.split_whitespace().count();
                    if parent_count > 1 {
                        let mainline = payload.get("mainline").and_then(Value::as_u64).unwrap_or(0);
                        if mainline == 0 || mainline as usize > parent_count {
                            return Err(ManagerError::System(
                                "目标是合并提交，请选择有效的主线父提交".to_string(),
                            ));
                        }
                        args.extend(["-m".to_string(), mainline.to_string()]);
                    }
                }
                args.push(hash);
            }
        }
        "continue" | "abort" | "skip" => {
            if operation.is_empty() {
                return Err(ManagerError::System(
                    "当前没有待完成的 Git 操作".to_string(),
                ));
            }
            if action == "continue" && conflicts {
                return Err(ManagerError::System("请先解决并暂存冲突".to_string()));
            }
            if action == "skip" && operation == "merge" {
                return Err(ManagerError::System("Merge 不支持跳过提交".to_string()));
            }
            args = vec![operation.clone(), format!("--{}", action)];
        }
        _ => return Err(ManagerError::System("不支持的 Git 操作".to_string())),
    }
    let mut output = run_git_output(&args, cwd).await?;
    if output.status.success() {
        if let Some(next) = after_args {
            output = run_git_output(&next, cwd).await?;
        }
    }
    let refreshed = read_status(cwd).await?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok(
        json!({ "success": output.status.success(), "output": ([stdout, stderr].into_iter().filter(|value| !value.is_empty()).collect::<Vec<_>>().join("\n")), "status": refreshed }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    macro_rules! git_test {
        ($name:ident, $body:block) => {
            #[test]
            fn $name() {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all().build().unwrap().block_on(async $body)
            }
        };
    }

    struct Repository {
        root: PathBuf,
        path: PathBuf,
    }

    impl Repository {
        async fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("ai-manager-git-test-{}", uuid::Uuid::new_v4()));
            let path = root.join("work");
            std::fs::create_dir_all(&path).unwrap();
            let repo = Self { root, path };
            repo.git(&["init", "-b", "main"]).await;
            configure(repo.cwd()).await;
            repo
        }

        fn cwd(&self) -> &str {
            self.path.to_str().unwrap()
        }

        fn write(&self, path: &str, content: &str) {
            let target = self.path.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, content).unwrap();
        }

        async fn git(&self, args: &[&str]) -> String {
            run_git(args, self.cwd()).await.unwrap()
        }

        async fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]).await;
            self.git(&["commit", "-m", message]).await;
            self.git(&["rev-parse", "HEAD"]).await
        }

        async fn perform(&self, mut payload: Value) -> Value {
            let status = read_status(self.cwd()).await.unwrap();
            payload["expectedHead"] = status["headHash"].clone();
            payload["expectedBranch"] = status["branch"].clone();
            execute_in_repository(self.cwd(), &payload).await.unwrap()
        }
    }

    impl Drop for Repository {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    async fn configure(cwd: &str) {
        for (key, value) in [
            ("user.name", "Git 工具测试"),
            ("user.email", "git-tool-test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
            ("core.hooksPath", ".git/no-test-hooks"),
        ] {
            run_git(&["config", key, value], cwd).await.unwrap();
        }
    }

    async fn conflicting_repository() -> Repository {
        let repo = Repository::new().await;
        repo.write("shared.txt", "base\n");
        repo.commit("base").await;
        repo.git(&["switch", "-c", "feature"]).await;
        repo.write("shared.txt", "feature\n");
        repo.commit("feature").await;
        repo.git(&["switch", "main"]).await;
        repo.write("shared.txt", "main\n");
        repo.commit("main").await;
        repo
    }

    git_test!(linked_worktree_apis_keep_head_index_and_files_separate, {
        let repo = Repository::new().await;
        repo.write("tracked.txt", "base\n");
        let base = repo.commit("base").await;
        let linked = repo.root.join("linked 中文 worktree");
        let linked_path = linked.to_str().unwrap();
        repo.git(&["worktree", "add", "-b", "linked", linked_path]).await;
        assert!(linked.join(".git").is_file());
        repo.git(&["branch", "other"]).await;
        repo.write("main-only.txt", "keep main\n");
        std::fs::write(linked.join("tracked.txt"), "linked change\n").unwrap();
        let paths = crate::core::paths::resolve_app_paths(&repo.root.join("manager"));
        let repos = json!([{"id":"repo", "name":"测试", "localPath":repo.cwd()}]);
        let mut payload = json!({"repoId":"repo", "worktreePath":linked_path});
        let status = get_status(&paths, &repos, payload.clone()).await.unwrap();
        assert_eq!(status["branch"], "linked");
        assert_eq!(status["worktrees"].as_array().unwrap().len(), 2);
        assert!(!status["files"].as_array().unwrap().iter().any(|file| file["path"] == "main-only.txt"));
        let scan = super::super::scan_branches(&paths, &repos, payload.clone()).await.unwrap();
        assert_eq!(scan["currentBranch"], "linked");
        assert_eq!(scan["project"]["projectPath"], super::super::normalize_project_path(linked_path));
        assert!(scan["branches"].as_array().unwrap().iter().any(|branch| branch["name"] == "main" && branch["worktreePath"] != ""));
        payload["filePath"] = json!("tracked.txt");
        let diff = get_worktree_diff(&paths, &repos, payload.clone()).await.unwrap();
        assert!(diff["patch"].as_str().unwrap().contains("linked change"));
        payload["action"] = json!("stage");
        payload["filePaths"] = json!(["tracked.txt"]);
        payload["expectedHead"] = status["headHash"].clone();
        payload["expectedBranch"] = status["branch"].clone();
        payload["expectedWorktreePath"] = status["projectPath"].clone();
        assert_eq!(execute_operation(&paths, &repos, payload.clone()).await.unwrap()["success"], true);
        assert!(repo.git(&["diff", "--cached"]).await.is_empty());
        assert!(run_git(&["diff", "--cached"], linked_path).await.unwrap().contains("linked change"));
        payload["action"] = json!("switch-branch");
        payload["branchName"] = json!("other");
        let result = execute_operation(&paths, &repos, payload.clone()).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["branch"], "other");
        assert_eq!(repo.git(&["branch", "--show-current"]).await, "main");
        payload["expectedBranch"] = json!("other");
        payload["branchName"] = json!("main");
        assert_eq!(execute_operation(&paths, &repos, payload.clone()).await.unwrap()["success"], false);
        payload["expectedWorktreePath"] = json!(repo.cwd());
        assert!(execute_operation(&paths, &repos, payload.clone()).await.is_err());
        payload["expectedWorktreePath"] = status["projectPath"].clone();
        payload["action"] = json!("checkout-commit");
        payload["revision"] = json!(base);
        let result = execute_operation(&paths, &repos, payload.clone()).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["detached"], true);
        assert_eq!(repo.git(&["branch", "--show-current"]).await, "main");
        let imported = json!([{"id":"linked", "localPath":linked_path}]);
        assert_eq!(get_status(&paths, &imported, json!({"repoId":"linked"})).await.unwrap()["detached"], true);
        payload["worktreePath"] = json!(repo.root.join("manager").to_str().unwrap());
        assert!(get_status(&paths, &repos, payload.clone()).await.is_err());
        repo.git(&["worktree", "remove", "--force", linked_path]).await;
        payload["worktreePath"] = json!(linked_path);
        assert!(get_status(&paths, &repos, payload).await.is_err());
        assert_eq!(std::fs::read_to_string(repo.path.join("main-only.txt")).unwrap(), "keep main\n");
    });

    git_test!(checkout_commit_refuses_to_overwrite_uncommitted_changes, {
        let repo = Repository::new().await;
        repo.write("tracked.txt", "base\n");
        let base = repo.commit("base").await;
        repo.write("tracked.txt", "next\n");
        let head = repo.commit("next").await;
        repo.write("tracked.txt", "uncommitted\n");
        let result = repo.perform(json!({"action":"checkout-commit", "revision":base})).await;
        assert_eq!(result["success"], false);
        assert_eq!(repo.git(&["rev-parse", "HEAD"]).await, head);
        assert_eq!(std::fs::read_to_string(repo.path.join("tracked.txt")).unwrap(), "uncommitted\n");
    });

    #[test]
    fn status_preserves_paths_and_distinguishes_both_sides() {
        let status = parse_status("# branch.oid 123\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +3 -2\01 MM N... 100644 100644 100644 abc def space name.txt\02 R. N... 100644 100644 100644 abc def R100 新\n名称.txt\0old name.txt\0u UU N... 100644 100644 100644 100644 a b c conflict.txt\0? new file.txt\0");
        assert_eq!(status["ahead"], 3);
        assert_eq!(status["behind"], 2);
        assert_eq!(status["files"][0]["staged"], true);
        assert_eq!(status["files"][0]["unstaged"], true);
        assert_eq!(status["files"][1]["path"], "新\n名称.txt");
        assert_eq!(status["files"][1]["oldPath"], "old name.txt");
        assert_eq!(status["files"][2]["conflicted"], true);
        assert_eq!(status["files"][3]["untracked"], true);
    }

    git_test!(comparing_current_branch_with_itself_is_empty, {
        let repo = Repository::new().await;
        repo.write("main.txt", "base\n");
        repo.commit("base").await;
        let result = branch_diff(repo.cwd(), &json!({ "baseBranch": "main", "targetBranch": "main" }))
            .await.unwrap();
        assert_eq!(result["fileCount"], 0);
        assert_eq!(result["changedLines"], 0);
        assert_eq!(result["patch"], "");
    });

    git_test!(unborn_repository_stages_unstages_previews_and_commits, {
        let repo = Repository::new().await;
        repo.write("新 文件.txt", "first\n");
        let diff = worktree_diff(
            repo.cwd(),
            &json!({ "filePath": "新 文件.txt", "staged": false }),
        )
        .await
        .unwrap();
        assert!(diff["patch"].as_str().unwrap().contains("+first"));
        assert_eq!(
            repo.perform(json!({ "action": "stage", "filePaths": ["新 文件.txt"] }))
                .await["success"],
            true
        );
        let diff = worktree_diff(
            repo.cwd(),
            &json!({ "filePath": "新 文件.txt", "staged": true }),
        )
        .await
        .unwrap();
        assert!(diff["patch"].as_str().unwrap().contains("+first"));
        assert_eq!(
            repo.perform(json!({ "action": "unstage", "filePaths": ["新 文件.txt"] }))
                .await["success"],
            true
        );
        assert!(repo.path.join("新 文件.txt").exists());
        assert_eq!(read_status(repo.cwd()).await.unwrap()["hasHead"], false);
        repo.perform(json!({ "action": "stage", "filePaths": ["新 文件.txt"] }))
            .await;
        let result = repo
            .perform(json!({ "action": "commit", "message": "feat: 首次提交\n\n提交正文" }))
            .await;
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["hasHead"], true);
        assert!(result["status"]["files"].as_array().unwrap().is_empty());
        assert!(repo
            .git(&["log", "-1", "--format=%B"])
            .await
            .contains("提交正文"));
    });

    git_test!(partial_staging_discard_and_rename_preserve_worktree, {
        let repo = Repository::new().await;
        repo.write("tracked.txt", "base\n");
        repo.commit("base").await;
        repo.write("tracked.txt", "staged\n");
        repo.perform(json!({ "action": "stage", "filePaths": ["tracked.txt"] }))
            .await;
        repo.write("tracked.txt", "unstaged\n");
        let status = read_status(repo.cwd()).await.unwrap();
        assert_eq!(status["files"][0]["staged"], true);
        assert_eq!(status["files"][0]["unstaged"], true);
        let staged = worktree_diff(
            repo.cwd(),
            &json!({ "filePath": "tracked.txt", "staged": true }),
        )
        .await
        .unwrap();
        let unstaged = worktree_diff(
            repo.cwd(),
            &json!({ "filePath": "tracked.txt", "staged": false }),
        )
        .await
        .unwrap();
        assert!(staged["patch"].as_str().unwrap().contains("+staged"));
        assert!(unstaged["patch"].as_str().unwrap().contains("+unstaged"));
        repo.perform(json!({ "action": "discard", "filePaths": ["tracked.txt"] }))
            .await;
        assert_eq!(
            std::fs::read_to_string(repo.path.join("tracked.txt")).unwrap(),
            "staged\n"
        );
        repo.perform(json!({ "action": "unstage", "filePaths": ["tracked.txt"] }))
            .await;
        assert!(repo.git(&["diff", "--cached"]).await.is_empty());
        repo.commit("changed").await;
        repo.git(&["mv", "tracked.txt", "renamed file.txt"]).await;
        let status = read_status(repo.cwd()).await.unwrap();
        assert_eq!(status["files"][0]["oldPath"], "tracked.txt");
        repo.write("renamed file.txt", "staged\nmore\n");
        assert_eq!(
            repo.perform(json!({ "action": "discard", "filePaths": ["renamed file.txt"] }))
                .await["success"],
            true
        );
        assert_eq!(
            std::fs::read_to_string(repo.path.join("renamed file.txt")).unwrap(),
            "staged\n"
        );
        repo.write("renamed file.txt", "staged\nmore\n");
        assert_eq!(
            repo.perform(json!({ "action": "stage", "filePaths": ["renamed file.txt"] }))
                .await["success"],
            true
        );
        assert_eq!(
            read_status(repo.cwd()).await.unwrap()["files"][0]["oldPath"],
            "tracked.txt"
        );
        assert_eq!(
            repo.perform(json!({ "action": "unstage", "filePaths": ["renamed file.txt"] }))
                .await["success"],
            true
        );
        assert!(repo.path.join("renamed file.txt").exists());
        assert!(repo.git(&["diff", "--cached"]).await.is_empty());
    });

    git_test!(branch_operations_and_literal_paths_are_bounded, {
        let repo = Repository::new().await;
        repo.write("base.txt", "base\n");
        repo.commit("base").await;
        let result = repo.perform(json!({ "action": "create-branch", "branchName": "feature", "revision": "HEAD", "switchAfterCreate": true })).await;
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["branch"], "feature");
        assert_eq!(repo.perform(json!({ "action": "rename-branch", "branchName": "feature", "newBranchName": "renamed" })).await["status"]["branch"], "renamed");
        assert_eq!(
            repo.perform(json!({ "action": "switch-branch", "branchName": "main" }))
                .await["status"]["branch"],
            "main"
        );
        repo.write("literal[1].txt", "literal\n");
        repo.write("literal1.txt", "other\n");
        repo.perform(json!({ "action": "stage", "filePaths": ["literal[1].txt"] }))
            .await;
        assert_eq!(
            repo.git(&["diff", "--cached", "--name-only"]).await,
            "literal[1].txt"
        );
        repo.perform(json!({ "action": "unstage", "filePaths": ["literal[1].txt"] }))
            .await;
        repo.perform(json!({ "action": "delete-untracked", "filePaths": ["literal[1].txt"] }))
            .await;
        assert!(!repo.path.join("literal[1].txt").exists());
        assert!(repo.path.join("literal1.txt").exists());
        assert!(execute_in_repository(
            repo.cwd(),
            &json!({ "action": "stage", "filePaths": ["../outside.txt"] })
        )
        .await
        .is_err());
        assert!(execute_in_repository(
            repo.cwd(),
            &json!({ "action": "switch-branch", "branchName": "--help" })
        )
        .await
        .is_err());
        assert!(resolve_commit(repo.cwd(), "--help").await.is_err());
        assert!(execute_in_repository(
            repo.cwd(),
            &json!({ "action": "stage", "expectedHead": "stale", "filePaths": ["literal1.txt"] })
        )
        .await
        .is_err());
    });

    git_test!(
        stash_save_apply_and_pop_restore_index_and_untracked_files,
        {
            let repo = Repository::new().await;
            repo.write("tracked.txt", "base\n");
            repo.commit("base").await;
            repo.write("tracked.txt", "saved\n");
            repo.git(&["add", "tracked.txt"]).await;
            repo.write("new.txt", "new\n");
            assert_eq!(repo.perform(json!({ "action": "stash-save", "message": "保存改动", "includeUntracked": true })).await["success"], true);
            assert!(read_status(repo.cwd()).await.unwrap()["files"]
                .as_array()
                .unwrap()
                .is_empty());
            let stashes = get_stashes(repo.cwd()).await.unwrap();
            let hash = string_value(stashes[0].get("hash"));
            assert_eq!(
                repo.perform(
                    json!({ "action": "stash-apply", "stashHash": hash, "restoreIndex": true })
                )
                .await["success"],
                true
            );
            assert_eq!(get_stashes(repo.cwd()).await.unwrap().len(), 1);
            assert!(repo.path.join("new.txt").exists());
            assert!(!repo.git(&["diff", "--cached"]).await.is_empty());
            repo.git(&["reset", "--hard", "HEAD"]).await;
            repo.git(&["clean", "-f"]).await;
            assert_eq!(
                repo.perform(
                    json!({ "action": "stash-pop", "stashHash": hash, "restoreIndex": true })
                )
                .await["success"],
                true
            );
            assert!(get_stashes(repo.cwd()).await.unwrap().is_empty());
            assert!(repo.path.join("new.txt").exists());
        }
    );

    git_test!(reset_modes_have_distinct_index_and_worktree_effects, {
        let repo = Repository::new().await;
        repo.write("tracked.txt", "zero\n");
        let zero = repo.commit("zero").await;
        repo.write("tracked.txt", "one\n");
        let one = repo.commit("one").await;
        repo.write("tracked.txt", "two\n");
        let two = repo.commit("two").await;
        let result = repo
            .perform(json!({ "action": "reset", "revision": one, "resetMode": "soft" }))
            .await;
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["files"][0]["staged"], true);
        assert_eq!(
            std::fs::read_to_string(repo.path.join("tracked.txt")).unwrap(),
            "two\n"
        );
        let result = repo
            .perform(json!({ "action": "reset", "revision": zero, "resetMode": "mixed" }))
            .await;
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["files"][0]["staged"], false);
        assert_eq!(result["status"]["files"][0]["unstaged"], true);
        assert!(execute_in_repository(
            repo.cwd(),
            &json!({ "action": "reset", "revision": two, "resetMode": "hard" })
        )
        .await
        .is_err());
        let result = repo.perform(json!({ "action": "reset", "revision": two, "resetMode": "hard", "confirmedDiscard": true })).await;
        assert_eq!(result["success"], true);
        assert!(result["status"]["files"].as_array().unwrap().is_empty());
    });

    git_test!(cherry_pick_and_revert_create_expected_history, {
        let repo = Repository::new().await;
        repo.write("tracked.txt", "base\n");
        let base = repo.commit("base").await;
        repo.write("tracked.txt", "change\n");
        let change = repo.commit("change").await;
        repo.perform(json!({ "action": "create-branch", "branchName": "target", "revision": base, "switchAfterCreate": true })).await;
        assert_eq!(
            repo.perform(json!({ "action": "cherry-pick", "revision": change }))
                .await["success"],
            true
        );
        assert_eq!(
            std::fs::read_to_string(repo.path.join("tracked.txt")).unwrap(),
            "change\n"
        );
        assert_eq!(
            repo.perform(json!({ "action": "revert", "revision": "HEAD" }))
                .await["success"],
            true
        );
        assert_eq!(
            std::fs::read_to_string(repo.path.join("tracked.txt")).unwrap(),
            "base\n"
        );
        assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).await, "3");
        let preview = revision_preview(repo.cwd(), "main").await.unwrap();
        assert_eq!(preview["hash"], change);
        assert_eq!(preview["changedFiles"][0], "tracked.txt");
    });

    git_test!(merge_conflicts_can_be_resolved_continued_and_aborted, {
        let repo = conflicting_repository().await;
        let result = repo
            .perform(json!({ "action": "merge", "revision": "feature" }))
            .await;
        assert_eq!(result["success"], false);
        assert_eq!(result["status"]["operation"], "merge");
        assert_eq!(result["status"]["files"][0]["conflicted"], true);
        let diff = worktree_diff(repo.cwd(), &json!({ "filePath": "shared.txt" }))
            .await
            .unwrap();
        assert!(diff["patch"].as_str().unwrap().contains("<<<<<<<"));
        assert!(
            execute_in_repository(repo.cwd(), &json!({ "action": "continue" }))
                .await
                .is_err()
        );
        assert_eq!(
            repo.perform(json!({ "action": "resolve-theirs", "filePaths": ["shared.txt"] }))
                .await["success"],
            true
        );
        let result = repo.perform(json!({ "action": "continue" })).await;
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["operation"], "");
        assert_eq!(
            repo.git(&["show", "--no-patch", "--format=%P", "HEAD"])
                .await
                .split_whitespace()
                .count(),
            2
        );
        assert_eq!(
            std::fs::read_to_string(repo.path.join("shared.txt")).unwrap(),
            "feature\n"
        );
        let other = conflicting_repository().await;
        other
            .perform(json!({ "action": "merge", "revision": "feature" }))
            .await;
        let result = other.perform(json!({ "action": "abort" })).await;
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["operation"], "");
        assert_eq!(
            std::fs::read_to_string(other.path.join("shared.txt")).unwrap(),
            "main\n"
        );
    });

    git_test!(
        cherry_pick_and_revert_conflicts_support_skip_abort_and_continue,
        {
            let repo = conflicting_repository().await;
            let pick = json!({ "action": "cherry-pick", "revision": "feature" });
            assert_eq!(
                repo.perform(pick.clone()).await["status"]["operation"],
                "cherry-pick"
            );
            assert_eq!(
                repo.perform(json!({ "action": "skip" })).await["success"],
                true
            );
            repo.perform(pick.clone()).await;
            assert_eq!(
                repo.perform(json!({ "action": "abort" })).await["success"],
                true
            );
            repo.perform(pick).await;
            repo.perform(json!({ "action": "resolve-theirs", "filePaths": ["shared.txt"] }))
                .await;
            assert_eq!(
                repo.perform(json!({ "action": "continue" })).await["success"],
                true
            );
            let applied = repo.git(&["rev-parse", "HEAD"]).await;
            repo.write("shared.txt", "later\n");
            repo.commit("later").await;
            assert_eq!(
                repo.perform(json!({ "action": "revert", "revision": applied }))
                    .await["status"]["operation"],
                "revert"
            );
            assert_eq!(
                repo.perform(json!({ "action": "abort" })).await["success"],
                true
            );
            repo.perform(json!({ "action": "revert", "revision": applied }))
                .await;
            repo.perform(json!({ "action": "resolve-theirs", "filePaths": ["shared.txt"] }))
                .await;
            assert_eq!(
                repo.perform(json!({ "action": "continue" })).await["success"],
                true
            );
        }
    );

    git_test!(rebase_conflicts_support_continue_skip_and_abort, {
        for action in ["continue", "skip", "abort"] {
            let repo = conflicting_repository().await;
            let original = repo.git(&["rev-parse", "HEAD"]).await;
            let bare = repo.root.join("remote.git");
            run_git(
                &["init", "--bare", "-b", "main", bare.to_str().unwrap()],
                repo.cwd(),
            )
            .await
            .unwrap();
            repo.git(&["remote", "add", "origin", bare.to_str().unwrap()])
                .await;
            repo.git(&["push", "origin", "feature:main"]).await;
            let result = repo.perform(json!({ "action": "pull", "remote": "origin", "remoteBranch": "main", "pullMode": "rebase" })).await;
            assert_eq!(result["success"], false);
            assert_eq!(result["status"]["operation"], "rebase");
            assert_eq!(result["status"]["files"][0]["conflicted"], true);
            if action == "continue" {
                assert_eq!(
                    repo.perform(
                        json!({ "action": "resolve-theirs", "filePaths": ["shared.txt"] })
                    )
                    .await["success"],
                    true
                );
            }
            let result = repo.perform(json!({ "action": action })).await;
            assert_eq!(result["success"], true, "{}", result["output"]);
            assert_eq!(result["status"]["operation"], "");
            assert_eq!(result["status"]["branch"], "main");
            assert_eq!(
                std::fs::read_to_string(repo.path.join("shared.txt")).unwrap(),
                if action == "skip" {
                    "feature\n"
                } else {
                    "main\n"
                }
            );
            if action == "abort" {
                assert_eq!(result["status"]["headHash"], original);
            }
        }
    });

    git_test!(
        stash_pop_conflicts_keep_stash_and_allow_normal_resolution,
        {
            let repo = Repository::new().await;
            repo.write("shared.txt", "base\n");
            repo.commit("base").await;
            repo.write("shared.txt", "saved\n");
            repo.perform(json!({ "action": "stash-save", "includeUntracked": false }))
                .await;
            let stashes = get_stashes(repo.cwd()).await.unwrap();
            let hash = string_value(stashes[0].get("hash"));
            repo.write("shared.txt", "later\n");
            repo.commit("later").await;
            let result = repo
                .perform(json!({ "action": "stash-pop", "stashHash": hash, "restoreIndex": false }))
                .await;
            assert_eq!(result["success"], false);
            assert_eq!(result["status"]["operation"], "");
            assert_eq!(result["status"]["files"][0]["conflicted"], true);
            assert_eq!(get_stashes(repo.cwd()).await.unwrap().len(), 1);
            assert_eq!(
                repo.perform(json!({ "action": "resolve-theirs", "filePaths": ["shared.txt"] }))
                    .await["success"],
                true
            );
            assert_eq!(
                repo.perform(json!({ "action": "commit", "message": "fix: 解决 Stash 冲突" }))
                    .await["success"],
                true
            );
            assert_eq!(
                std::fs::read_to_string(repo.path.join("shared.txt")).unwrap(),
                "saved\n"
            );
            assert_eq!(get_stashes(repo.cwd()).await.unwrap().len(), 1);
        }
    );

    git_test!(local_remote_fetch_push_and_all_pull_policies, {
        let repo = Repository::new().await;
        repo.write("base.txt", "base\n");
        repo.commit("base").await;
        let bare = repo.root.join("remote.git");
        run_git(
            &["init", "--bare", "-b", "main", bare.to_str().unwrap()],
            repo.cwd(),
        )
        .await
        .unwrap();
        repo.git(&["remote", "add", "origin", bare.to_str().unwrap()])
            .await;
        let sync = json!({ "remote": "origin", "remoteBranch": "main" });
        assert_eq!(repo.perform(json!({ "action": "push", "remote": sync["remote"], "remoteBranch": sync["remoteBranch"] })).await["success"], true);
        let peer = repo.root.join("peer");
        run_git(
            &["clone", bare.to_str().unwrap(), peer.to_str().unwrap()],
            repo.cwd(),
        )
        .await
        .unwrap();
        configure(peer.to_str().unwrap()).await;
        std::fs::write(peer.join("remote.txt"), "remote\n").unwrap();
        run_git(&["add", "-A"], peer.to_str().unwrap())
            .await
            .unwrap();
        run_git(&["commit", "-m", "remote"], peer.to_str().unwrap())
            .await
            .unwrap();
        run_git(&["push", "origin", "main"], peer.to_str().unwrap())
            .await
            .unwrap();
        let result = repo
            .perform(json!({ "action": "fetch", "remote": "origin" }))
            .await;
        assert_eq!(result["success"], true);
        assert_eq!(result["status"]["behind"], 1);
        assert_eq!(repo.perform(json!({ "action": "pull", "remote": "origin", "remoteBranch": "main", "pullMode": "ff-only" })).await["success"], true);
        assert!(repo.path.join("remote.txt").exists());
        // 显式选择整合策略时覆盖仓库的仅快进默认值。
        repo.git(&["config", "pull.ff", "only"]).await;
        for mode in ["merge", "rebase"] {
            run_git(&["fetch", "origin"], peer.to_str().unwrap())
                .await
                .unwrap();
            run_git(&["reset", "--hard", "origin/main"], peer.to_str().unwrap())
                .await
                .unwrap();
            std::fs::write(peer.join(format!("remote-{}.txt", mode)), "remote\n").unwrap();
            run_git(&["add", "-A"], peer.to_str().unwrap())
                .await
                .unwrap();
            run_git(&["commit", "-m", mode], peer.to_str().unwrap())
                .await
                .unwrap();
            run_git(&["push", "origin", "main"], peer.to_str().unwrap())
                .await
                .unwrap();
            repo.write(&format!("local-{}.txt", mode), "local\n");
            repo.commit(mode).await;
            let refused = repo.perform(json!({ "action": "pull", "remote": "origin", "remoteBranch": "main", "pullMode": "ff-only" })).await;
            assert_eq!(refused["success"], false);
            assert_eq!(refused["status"]["operation"], "");
            let result = repo.perform(json!({ "action": "pull", "remote": "origin", "remoteBranch": "main", "pullMode": mode })).await;
            assert_eq!(result["success"], true, "{}", result["output"]);
            assert!(repo.path.join(format!("remote-{}.txt", mode)).exists());
            assert!(repo.path.join(format!("local-{}.txt", mode)).exists());
            assert_eq!(
                repo.perform(
                    json!({ "action": "push", "remote": "origin", "remoteBranch": "main" })
                )
                .await["success"],
                true
            );
        }
    });
}

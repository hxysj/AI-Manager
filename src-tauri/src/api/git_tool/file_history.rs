use super::{parse_commit_log, run_git, run_git_raw_vec, string_value, worktrees};
use crate::core::{error::ManagerError, paths::AppPaths};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

fn relative_file_path(cwd: &str, input: &str) -> Result<String, ManagerError> {
    let invalid = || {
        ManagerError::System("请选择工作树内的单个文件，或输入它在仓库中的相对路径。".to_string())
    };
    if input.is_empty() || input.contains('\0') {
        return Err(invalid());
    }
    let path = Path::new(input);
    let path = if path.is_absolute() {
        path.strip_prefix(cwd).map_err(|_| invalid())?
    } else {
        path
    };
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => relative.push(value),
            Component::CurDir => {}
            Component::ParentDir if relative.pop() => {}
            _ => return Err(invalid()),
        }
    }
    if relative.as_os_str().is_empty() || Path::new(cwd).join(&relative).is_dir() {
        return Err(invalid());
    }
    let relative = relative.to_string_lossy().into_owned();
    #[cfg(windows)]
    let relative = relative.replace('\\', "/");
    Ok(relative)
}

// 元信息和文件状态均以 NUL 分隔，保留路径里的空格、中文和换行。
fn parse_history(output: &str, requested_path: &str) -> Result<Vec<Value>, ManagerError> {
    let fields = output.split('\0').collect::<Vec<_>>();
    let mut index = 0;
    let mut commits = Vec::new();
    while index < fields.len() {
        if fields[index].is_empty() {
            index += 1;
            continue;
        }
        if index + 5 > fields.len() {
            return Err(ManagerError::System("文件历史解析失败".to_string()));
        }
        let mut commit = parse_commit_log(&fields[index..index + 5].join("\0"))?
            .into_iter()
            .next()
            .unwrap();
        index += 5;
        commit["filePath"] = json!(requested_path);
        commit["fileStatus"] = json!("");
        commit["oldPath"] = json!("");
        while index < fields.len() && !fields[index].is_empty() {
            let status = fields[index].trim_start_matches('\n');
            index += 1;
            let count = if status.starts_with('R') || status.starts_with('C') {
                2
            } else {
                1
            };
            if index + count > fields.len() {
                return Err(ManagerError::System("文件历史路径解析失败".to_string()));
            }
            if commit["fileStatus"] == "" {
                commit["fileStatus"] = json!(status);
                commit["filePath"] = json!(fields[index + count - 1]);
                if count == 2 {
                    commit["oldPath"] = json!(fields[index]);
                }
            }
            index += count;
        }
        commits.push(commit);
    }
    Ok(commits)
}

pub async fn list_file_history(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let project = worktrees::project(paths, repos, &repo_id, &payload).await?;
    let cwd = string_value(project.get("projectPath"));
    let file_path = relative_file_path(
        &cwd,
        payload
            .get("filePath")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )?;
    let branch = string_value(payload.get("branchName"));
    let all_branches = payload.get("allBranches").and_then(Value::as_bool) == Some(true);
    let follow =
        !all_branches && payload.get("followRenames").and_then(Value::as_bool) != Some(false);
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(60)
        .clamp(1, 200);
    let offset = payload.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let mut args = vec![
        "--literal-pathspecs".to_string(),
        "log".to_string(),
        "--full-history".to_string(),
        "--diff-merges=first-parent".to_string(),
        "--name-status".to_string(),
        "--find-renames".to_string(),
        "-z".to_string(),
        "--no-ext-diff".to_string(),
        "--no-textconv".to_string(),
        "--no-color".to_string(),
        "--date=iso-strict".to_string(),
        "--format=%x00%H%x00%h%x00%s%x00%an%x00%ad".to_string(),
        // --follow 使用 --skip 会跳过重命名检测，必须先追踪路径，再截取本页。
        format!(
            "--max-count={}",
            if follow {
                offset.saturating_add(limit).saturating_add(1)
            } else {
                limit + 1
            }
        ),
    ];
    let revision = if all_branches {
        args.push("--all".to_string());
        String::new()
    } else {
        let snapshot = string_value(payload.get("revision"));
        let reference = if !snapshot.is_empty() {
            &snapshot
        } else if branch.is_empty() {
            "HEAD"
        } else {
            &branch
        };
        let hash = run_git(
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}^{{commit}}", reference),
            ],
            &cwd,
        )
        .await?;
        if follow {
            args.push("--follow".to_string());
        }
        args.push(hash.clone());
        hash
    };
    if !follow {
        args.push(format!("--skip={}", offset));
    }
    args.extend(["--".to_string(), file_path.clone()]);
    let output = run_git_raw_vec(&args, &cwd).await?;
    let mut commits = parse_history(&output, &file_path)?;
    if follow {
        commits.drain(..(offset as usize).min(commits.len()));
    }
    let has_more = commits.len() > limit as usize;
    commits.truncate(limit as usize);
    let check_branch = string_value(project.get("checkBranchName"));
    if !check_branch.is_empty() && !commits.is_empty() {
        let checked = super::check_commits_on_branch(&cwd, &check_branch, commits).await?;
        commits = checked["checkedCommits"]
            .as_array()
            .cloned()
            .unwrap_or_default();
    }
    Ok(json!({
        "commits": commits, "hasMore": has_more, "offset": offset,
        "filePath": file_path, "revision": revision, "followRenames": follow,
        "branchName": if all_branches { "所有分支" } else if branch.is_empty() { "HEAD" } else { &branch }
    }))
}

pub async fn list_worktree_files(
    paths: &AppPaths,
    repos: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let repo_id = string_value(payload.get("repoId"));
    let project = worktrees::project(paths, repos, &repo_id, &payload).await?;
    let cwd = string_value(project.get("projectPath"));
    // 只读取 Git 管理的文件和未被忽略的新文件，不扫描依赖、构建产物或文件内容。
    let output = run_git_raw_vec(
        &[
            "ls-files".to_string(),
            "--cached".to_string(),
            "--others".to_string(),
            "--exclude-standard".to_string(),
            "--deduplicate".to_string(),
            "-z".to_string(),
        ],
        &cwd,
    )
    .await?;
    Ok(json!(output
        .split('\0')
        .filter(|path| !path.is_empty() && !path.ends_with('/'))
        .collect::<Vec<_>>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! git_test {
        ($name:ident, $body:block) => {
            #[test]
            fn $name() {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all().build().unwrap().block_on(async $body)
            }
        };
    }

    struct Repository(PathBuf);

    impl Repository {
        async fn new() -> Self {
            let repo = Self(std::env::temp_dir().join(format!(
                "ai-manager-file-history-test-{}",
                uuid::Uuid::new_v4()
            )));
            std::fs::create_dir_all(&repo.0).unwrap();
            repo.git(&["init", "-b", "main"]).await;
            for (name, value) in [
                ("user.name", "文件历史测试"),
                ("user.email", "history@example.invalid"),
                ("commit.gpgsign", "false"),
                ("core.autocrlf", "false"),
                ("core.hooksPath", ".git/no-test-hooks"),
            ] {
                repo.git(&["config", name, value]).await;
            }
            repo
        }

        fn cwd(&self) -> &str {
            self.0.to_str().unwrap()
        }

        async fn git(&self, args: &[&str]) -> String {
            run_git(args, self.cwd()).await.unwrap()
        }

        fn write(&self, name: &str, content: &str) {
            std::fs::write(self.0.join(name), content).unwrap();
        }

        async fn commit(&self, title: &str) -> String {
            self.git(&["add", "-A"]).await;
            self.git(&["commit", "-m", title]).await;
            self.git(&["rev-parse", "HEAD"]).await
        }

        async fn history(&self, mut payload: Value) -> Result<Value, ManagerError> {
            payload["repoId"] = json!("repo");
            let paths = crate::core::paths::resolve_app_paths(&self.0.join("manager-data"));
            let repos = json!([{ "id": "repo", "name": "测试", "localPath": self.cwd() }]);
            list_file_history(&paths, &repos, payload).await
        }
    }

    impl Drop for Repository {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_rename_records_without_trimming_file_names() {
        let hash = "a".repeat(40);
        let output = format!("\0{}\0aaaaaaa\0重命名\0作者\02026-10-20T08:00:00+08:00\0\nR100\0旧文件\n[1].txt\0 新文件.txt \0", hash);
        let records = parse_history(&output, " 新文件.txt ").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["oldPath"], "旧文件\n[1].txt");
        assert_eq!(records[0]["filePath"], " 新文件.txt ");
        assert_eq!(records[0]["fileStatus"], "R100");
    }

    git_test!(follows_renames_deletions_and_snapshot_pagination, {
        let repo = Repository::new().await;
        let old = "旧文件 [1].txt";
        let new = "新文件 [1].txt";
        repo.write(old, "base\n");
        repo.write("unrelated.txt", "other\n");
        let created = repo.commit("创建文件").await;
        repo.write(old, "base\nchanged\n");
        let changed = repo.commit("修改旧文件").await;
        repo.write("unrelated.txt", "unrelated change\n");
        repo.commit("无关修改").await;
        repo.git(&["mv", old, new]).await;
        let renamed = repo.commit("重命名文件").await;
        repo.write(new, "base\nchanged\nnew change\n");
        let modified = repo.commit("修改新文件").await;
        repo.git(&["rm", "--", new]).await;
        let deleted = repo.commit("删除文件").await;
        let first = repo
            .history(json!({ "filePath": new, "limit": 2 }))
            .await
            .unwrap();
        assert_eq!(first["commits"][0]["hash"], deleted);
        assert_eq!(first["commits"][1]["hash"], modified);
        assert_eq!(first["hasMore"], true);
        let second = repo
            .history(
                json!({ "filePath": new, "offset": 2, "limit": 2, "revision": first["revision"] }),
            )
            .await
            .unwrap();
        assert_eq!(second["commits"][0]["hash"], renamed);
        assert_eq!(second["commits"][0]["oldPath"], old);
        assert_eq!(second["commits"][1]["hash"], changed);
        assert_eq!(second["commits"][1]["filePath"], old);
        repo.write(new, "recreated\n");
        repo.commit("查询之后新增的提交").await;
        let last = repo
            .history(
                json!({ "filePath": new, "offset": 4, "limit": 2, "revision": first["revision"] }),
            )
            .await
            .unwrap();
        assert_eq!(last["commits"].as_array().unwrap().len(), 1);
        assert_eq!(last["commits"][0]["hash"], created);
        assert_eq!(last["hasMore"], false);
        let literal = repo
            .history(json!({ "filePath": "旧文件 1.txt" }))
            .await
            .unwrap();
        assert!(literal["commits"].as_array().unwrap().is_empty());
        let no_follow = repo
            .history(
                json!({ "filePath": new, "revision": first["revision"], "followRenames": false }),
            )
            .await
            .unwrap();
        assert_eq!(no_follow["commits"].as_array().unwrap().len(), 3);
        let detail = super::super::get_commit_detail_by_args(
            vec!["--literal-pathspecs".to_string()],
            repo.cwd(),
            &created,
            old,
        )
        .await
        .unwrap();
        assert!(detail["patch"].as_str().unwrap().contains("+base"));
        assert!(!detail["patch"].as_str().unwrap().contains("+other"));
    });

    git_test!(
        all_branches_and_linked_worktrees_use_the_requested_context,
        {
            let repo = Repository::new().await;
            repo.write("file.txt", "base\n");
            let base = repo.commit("创建文件").await;
            repo.git(&["switch", "-c", "feature"]).await;
            repo.write("file.txt", "feature\n");
            let feature = repo.commit("其他分支的修改").await;
            repo.git(&["switch", "main"]).await;
            let current = repo
                .history(json!({ "filePath": "file.txt" }))
                .await
                .unwrap();
            assert_eq!(current["commits"].as_array().unwrap().len(), 1);
            let all = repo
                .history(json!({ "filePath": "file.txt", "allBranches": true }))
                .await
                .unwrap();
            assert_eq!(all["commits"].as_array().unwrap().len(), 2);
            assert!(all["commits"]
                .as_array()
                .unwrap()
                .iter()
                .any(|commit| commit["hash"] == feature));
            assert_eq!(all["followRenames"], false);
            let linked = repo.0.join("linked 中文 worktree");
            repo.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()])
                .await;
            std::fs::write(linked.join("file.txt"), "linked\n").unwrap();
            run_git(&["add", "-A"], linked.to_str().unwrap())
                .await
                .unwrap();
            run_git(
                &["commit", "-m", "工作树中的修改"],
                linked.to_str().unwrap(),
            )
            .await
            .unwrap();
            let linked_hash = run_git(&["rev-parse", "HEAD"], linked.to_str().unwrap())
                .await
                .unwrap();
            let records = repo
                .history(json!({ "worktreePath": linked, "filePath": linked.join("file.txt") }))
                .await
                .unwrap();
            assert_eq!(records["commits"][0]["hash"], linked_hash);
            assert_eq!(records["filePath"], "file.txt");
            assert_eq!(repo.git(&["rev-parse", "HEAD"]).await, base);
            std::fs::write(linked.join("new 中文.txt"), "untracked\n").unwrap();
            std::fs::write(linked.join(".gitignore"), "ignored/\n").unwrap();
            std::fs::create_dir_all(linked.join("ignored")).unwrap();
            std::fs::write(linked.join("ignored/cache.bin"), "ignored").unwrap();
            let paths = crate::core::paths::resolve_app_paths(&repo.0.join("manager-data"));
            let repos = json!([{ "id": "repo", "name": "测试", "localPath": repo.cwd() }]);
            let files = list_worktree_files(
                &paths,
                &repos,
                json!({ "repoId": "repo", "worktreePath": linked }),
            )
            .await
            .unwrap();
            let files = files.as_array().unwrap();
            assert!(files.contains(&json!("file.txt")));
            assert!(files.contains(&json!("new 中文.txt")));
            assert!(!files
                .iter()
                .any(|file| file.as_str().unwrap().starts_with("ignored/")));
            super::super::save_project_patch(
                &paths,
                &repos,
                "repo",
                json!({ "checkBranchName": "main" }),
            )
            .await
            .unwrap();
            let checked = repo
                .history(json!({ "filePath": "file.txt", "worktreePath": linked }))
                .await
                .unwrap();
            assert_eq!(checked["commits"][0]["checkStatus"], "missing");
            assert_eq!(checked["commits"][1]["checkStatus"], "exists-hash");
            assert_eq!(checked["commits"][1]["checkTargetBranch"], "main");
            assert!(repo
                .history(json!({ "filePath": "../outside.txt" }))
                .await
                .is_err());
            assert!(repo
                .history(json!({ "filePath": "file.txt", "branchName": "--all" }))
                .await
                .is_err());
            assert!(repo
                .history(json!({ "filePath": "file.txt", "worktreePath": repo.0.join("missing") }))
                .await
                .is_err());
        }
    );
}

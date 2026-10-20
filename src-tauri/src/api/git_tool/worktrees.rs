use super::{get_repo_project, normalize_project_path, run_git, run_git_raw, string_value};
use crate::core::{error::ManagerError, paths::AppPaths};
use serde_json::{json, Value};

// porcelain -z 保留路径中的空格、中文和换行，不依赖 .git 是文件还是目录。
fn parse_worktrees(output: &str) -> Vec<Value> {
    let mut trees = Vec::new();
    let mut current = Value::Null;
    for field in output.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            if !current.is_null() {
                trees.push(current);
            }
            current = json!({
                "path": path, "branch": "", "headHash": "", "detached": false,
                "bare": false, "locked": false, "lockReason": "",
                "prunable": false, "pruneReason": "", "isMain": trees.is_empty()
            });
        } else if !current.is_null() {
            if let Some(hash) = field.strip_prefix("HEAD ") {
                current["headHash"] = json!(hash);
            } else if let Some(branch) = field.strip_prefix("branch refs/heads/") {
                current["branch"] = json!(branch);
            } else if field == "detached" {
                current["detached"] = json!(true);
            } else if field == "bare" {
                current["bare"] = json!(true);
            } else if field == "locked" || field.starts_with("locked ") {
                current["locked"] = json!(true);
                current["lockReason"] = json!(field.strip_prefix("locked ").unwrap_or(""));
            } else if field == "prunable" || field.starts_with("prunable ") {
                current["prunable"] = json!(true);
                current["pruneReason"] = json!(field.strip_prefix("prunable ").unwrap_or(""));
            }
        }
    }
    if !current.is_null() {
        trees.push(current);
    }
    trees
}

pub(super) async fn project(
    paths: &AppPaths,
    repos: &Value,
    repo_id: &str,
    payload: &Value,
) -> Result<Value, ManagerError> {
    let mut project = get_repo_project(paths, repos, repo_id)?;
    let base = string_value(project.get("projectPath"));
    let output = run_git_raw(&["worktree", "list", "--porcelain", "-z"], &base).await?;
    let mut worktrees = parse_worktrees(&output);
    let requested = string_value(payload.get("worktreePath"));
    let selected = if requested.is_empty() {
        run_git(&["rev-parse", "--show-toplevel"], &base).await?
    } else {
        requested
    };
    let selected = normalize_project_path(&selected);
    // 只能操作此仓库登记的工作树，失效路径不能悄悄回落到主工作树。
    let mut found = false;
    for tree in &mut worktrees {
        let path = normalize_project_path(&string_value(tree.get("path")));
        let active = path == selected && tree["bare"] != true && tree["prunable"] != true;
        tree["path"] = json!(path);
        tree["isCurrent"] = json!(active);
        found |= active;
    }
    if !found || !std::path::Path::new(&selected).is_dir() {
        return Err(ManagerError::System(
            "所选工作树已移除或不属于当前仓库，请重新选择工作树。".to_string(),
        ));
    }
    project["projectPath"] = json!(selected);
    project["worktrees"] = json!(worktrees);
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worktree_paths_and_states_without_unquoting() {
        let trees = parse_worktrees("worktree D:/主项目\0HEAD abc\0branch refs/heads/main\0\0worktree D:/linked dir\n中文\0HEAD def\0detached\0locked 保留\0\0worktree D:/gone\0HEAD abc\0branch refs/heads/old\0prunable missing gitdir\0\0");
        assert_eq!(trees.len(), 3);
        assert_eq!(trees[0]["isMain"], true);
        assert_eq!(trees[0]["branch"], "main");
        assert_eq!(trees[1]["path"], "D:/linked dir\n中文");
        assert_eq!(trees[1]["detached"], true);
        assert_eq!(trees[1]["locked"], true);
        assert_eq!(trees[2]["prunable"], true);
    }
}

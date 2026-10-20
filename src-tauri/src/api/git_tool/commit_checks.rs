use super::{string_value, Command, ManagerError};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;

// 按 Unicode 字符计算编辑距离，忽略大小写和空格，两个字段都须达到 85%。
fn highly_similar(left: &str, right: &str) -> bool {
    let normalize = |text: &str| {
        text.chars()
            .filter(|ch| !ch.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect::<Vec<_>>()
    };
    let left = normalize(left);
    let right = normalize(right);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    let allowed = left.len().max(right.len()) * 15 / 100;
    if left.len().abs_diff(right.len()) > allowed {
        return false;
    }
    let mut row = (0..=right.len()).collect::<Vec<_>>();
    for (index, ch) in left.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = index + 1;
        let mut minimum = row[0];
        for (column, other) in right.iter().enumerate() {
            let previous = row[column + 1];
            row[column + 1] = (diagonal + usize::from(ch != other))
                .min(previous + 1)
                .min(row[column] + 1);
            diagonal = previous;
            minimum = minimum.min(row[column + 1]);
        }
        if minimum > allowed {
            return false;
        }
    }
    row[right.len()] <= allowed
}

async fn patch_id(project_path: &str, hash: &str) -> Result<Option<String>, ManagerError> {
    // 直接通过管道传递补丁，避免把完整改动（尤其二进制图片）读进应用内存。
    let mut diff_command = Command::new("git");
    let mut patch_command = Command::new("git");
    #[cfg(windows)]
    {
        diff_command.creation_flags(super::CREATE_NO_WINDOW);
        patch_command.creation_flags(super::CREATE_NO_WINDOW);
    }
    let mut diff = diff_command
        .args([
            "-c",
            "core.quotePath=true",
            "diff-tree",
            "--root",
            "--no-commit-id",
            "-r",
            "-p",
            "--binary",
            "--full-index",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-relative",
            "--unified=0",
            "--inter-hunk-context=0",
            "--diff-algorithm=myers",
            "--no-indent-heuristic",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--ignore-submodules=none",
            "--submodule=short",
            hash,
            "--",
        ])
        .current_dir(project_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let pipe: Stdio = diff.stdout.take().unwrap().try_into()?;
    // --verbatim 保留空白差异；补丁指纹忽略提交时间、提交哈希和改动行号。
    let patch = patch_command
        .args(["patch-id", "--verbatim"])
        .current_dir(project_path)
        .stdin(pipe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let (diff, patch) =
        futures_util::future::try_join(diff.wait_with_output(), patch.wait_with_output()).await?;
    for output in [&diff, &patch] {
        if !output.status.success() {
            return Err(ManagerError::System(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
    }
    // 空提交及未展开的合并提交没有补丁，不参与内容匹配。
    Ok(String::from_utf8_lossy(&patch.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string))
}

pub(super) async fn find_similar_content_commit(
    project_path: &str,
    source: &Value,
    targets: &[Value],
    patches: &mut HashMap<String, Option<String>>,
) -> Result<Option<Value>, ManagerError> {
    let subject = string_value(source.get("subject"));
    let author = string_value(source.get("author"));
    let candidates = targets
        .iter()
        .filter(|target| {
            highly_similar(&author, &string_value(target.get("author")))
                && highly_similar(&subject, &string_value(target.get("subject")))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(None);
    }

    let source_hash = string_value(source.get("hash"));
    if !patches.contains_key(&source_hash) {
        patches.insert(
            source_hash.clone(),
            patch_id(project_path, &source_hash).await?,
        );
    }
    let Some(source_patch) = patches[&source_hash].clone() else {
        return Ok(None);
    };
    for target in candidates {
        let hash = string_value(target.get("hash"));
        if !patches.contains_key(&hash) {
            patches.insert(hash.clone(), patch_id(project_path, &hash).await?);
        }
        if patches[&hash].as_ref() == Some(&source_patch) {
            return Ok(Some(target.clone()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::super::{
        check_commit_on_branch, check_commits_on_branch, parse_commit_log, run_git,
    };
    use super::*;
    use serde_json::json;
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

    struct Repository(PathBuf);

    impl Repository {
        async fn new() -> Self {
            let repo = Self(
                std::env::temp_dir()
                    .join(format!("ai-manager-check-test-{}", uuid::Uuid::new_v4())),
            );
            std::fs::create_dir_all(&repo.0).unwrap();
            repo.git(&["init", "-b", "main"]).await;
            for (key, value) in [
                ("user.name", "developer-a"),
                ("user.email", "check@example.invalid"),
                ("commit.gpgsign", "false"),
                ("core.autocrlf", "false"),
                ("core.hooksPath", ".git/no-test-hooks"),
            ] {
                repo.git(&["config", key, value]).await;
            }
            repo
        }

        fn cwd(&self) -> &str {
            self.0.to_str().unwrap()
        }

        fn write(&self, path: &str, content: &[u8]) {
            std::fs::write(self.0.join(path), content).unwrap();
        }

        async fn git(&self, args: &[&str]) -> String {
            run_git(args, self.cwd()).await.unwrap()
        }

        async fn commit(&self, title: &str) -> Value {
            self.git(&["add", "-A"]).await;
            self.git(&["commit", "-m", title]).await;
            let output = self
                .git(&[
                    "show",
                    "-s",
                    "--date=iso-strict",
                    "--pretty=format:%H%x00%h%x00%s%x00%an%x00%ad",
                ])
                .await;
            parse_commit_log(&output).unwrap().remove(0)
        }

        async fn check(&self, source: &Value, branch: &str) -> Value {
            check_commit_on_branch(
                self.cwd(),
                branch,
                source["hash"].as_str().unwrap(),
                source["subject"].as_str().unwrap(),
                source["date"].as_str().unwrap(),
            )
            .await
            .unwrap()
        }
    }

    impl Drop for Repository {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn similarity_requires_close_nonempty_unicode_text() {
        assert!(highly_similar(
            "fix: 修复图片工作台内存峰值",
            "fix: 修复图片工作台内存峰值！"
        ));
        assert!(highly_similar("Read Boy", "readboy"));
        assert!(highly_similar("developer-a", "developer-b"));
        assert!(!highly_similar("张三", "李四"));
        assert!(!highly_similar("", ""));
        assert!(!highly_similar(
            "fix: 修复图片工作台内存峰值",
            "feat: 增加分支管理功能"
        ));
    }

    git_test!(matches_shifted_changes_in_single_batch_and_cached_checks, {
        let repo = Repository::new().await;
        repo.write("shared.txt", b"start\nvalue=0\nend\n");
        repo.commit("base").await;
        repo.git(&["switch", "-c", "feature"]).await;
        repo.write("shared.txt", b"start\nvalue=1\nend\n");
        let source = repo.commit("fix: 修复图片工作台的运行内存占用").await;
        repo.git(&["switch", "main"]).await;
        repo.write("shared.txt", b"prefix\nstart\nvalue=0\nend\n");
        repo.commit("增加无关的前置行").await;
        repo.git(&["config", "user.name", "developer-b"]).await;
        repo.write("shared.txt", b"prefix\nstart\nvalue=1\nend\n");
        let target = repo.commit("fix: 修复图片工作台的运行内存占用！").await;
        assert_ne!(source["hash"], target["hash"]);
        assert_eq!(
            repo.check(&source, "main").await["matchedBy"],
            "similar-content"
        );
        let batch = check_commits_on_branch(repo.cwd(), "main", vec![source.clone()])
            .await
            .unwrap();
        assert_eq!(batch["checkedCommits"][0]["checkStatus"], "exists-content");
        assert_eq!(
            batch["matchedCommits"][0]["matchedCommit"]["hash"],
            target["hash"]
        );
        assert_eq!(repo.check(&target, "main").await["matchedBy"], "hash");

        let paths = crate::core::paths::resolve_app_paths(&repo.0.join("manager"));
        let repos = json!([{ "id": "repo", "name": "测试仓库", "localPath": repo.cwd() }]);
        super::super::save_project_patch(
            &paths,
            &repos,
            "repo",
            json!({ "checkBranchName": "main" }),
        )
        .await
        .unwrap();
        let payload = json!({ "repoId": "repo", "branchName": "feature", "limit": 1 });
        for _ in 0..2 {
            let result = super::super::list_commits(&paths, &repos, payload.clone())
                .await
                .unwrap();
            assert_eq!(result[0]["checkStatus"], "exists-content");
        }
        let cached = super::super::check_commit_on_branch_api(
            &paths,
            &repos,
            json!({
                "repoId": "repo", "sourceBranchName": "feature", "targetBranchName": "main",
                "commitHash": source["hash"], "subject": source["subject"], "date": source["date"]
            }),
        )
        .await
        .unwrap();
        assert_eq!(cached["matchedBy"], "similar-content");
        assert_eq!(cached["commit"]["hash"], target["hash"]);
    });

    git_test!(rejects_different_changes_authors_titles_and_extra_files, {
        let repo = Repository::new().await;
        repo.write("shared.txt", b"value=0\n");
        repo.commit("base").await;
        repo.git(&["switch", "-c", "feature"]).await;
        repo.write("shared.txt", b"value=1\n");
        let source = repo.commit("fix: 修复图片工作台的运行内存占用").await;
        for (index, content, author, title, extra) in [
            (
                0,
                "value=2\n",
                "developer-a",
                "fix: 修复图片工作台的运行内存占用！",
                false,
            ),
            (
                1,
                "value = 1\n",
                "developer-a",
                "fix: 修复图片工作台的运行内存占用！",
                false,
            ),
            (
                2,
                "value=1\n",
                "完全不同的作者",
                "fix: 修复图片工作台的运行内存占用！",
                false,
            ),
            (
                3,
                "value=1\n",
                "developer-a",
                "feat: 增加远程分支管理",
                false,
            ),
            (
                4,
                "value=1\n",
                "developer-a",
                "fix: 修复图片工作台的运行内存占用！",
                true,
            ),
        ] {
            let branch = format!("target-{}", index);
            repo.git(&["switch", "-c", &branch, "main"]).await;
            repo.git(&["config", "user.name", author]).await;
            repo.write("shared.txt", content.as_bytes());
            if extra {
                repo.write("extra.txt", b"extra\n");
            }
            let target = repo.commit(title).await;
            assert!(repo.check(&source, &branch).await.is_null(), "{}", branch);
            let batch = check_commits_on_branch(repo.cwd(), &branch, vec![source.clone()])
                .await
                .unwrap();
            assert_eq!(
                batch["checkedCommits"][0]["checkStatus"], "missing",
                "{}",
                branch
            );
            if index == 2 || index == 3 {
                let mut patches = HashMap::new();
                assert!(
                    find_similar_content_commit(repo.cwd(), &source, &[target], &mut patches)
                        .await
                        .unwrap()
                        .is_none()
                );
                assert!(patches.is_empty());
            }
        }
    });

    git_test!(compares_binary_changes_and_skips_empty_and_merge_commits, {
        let repo = Repository::new().await;
        repo.write("image.bin", b"base\0image");
        let base = repo.commit("base").await;
        assert!(patch_id(repo.cwd(), base["hash"].as_str().unwrap())
            .await
            .unwrap()
            .is_some());
        repo.git(&["switch", "-c", "feature"]).await;
        repo.write("image.bin", b"changed\0image");
        let source = repo.commit("fix: 修复图片工作台的运行内存占用").await;
        repo.git(&["switch", "main"]).await;
        repo.write("image.bin", b"changed\0image");
        repo.commit("fix: 修复图片工作台的运行内存占用！").await;
        assert_eq!(
            repo.check(&source, "main").await["matchedBy"],
            "similar-content"
        );
        repo.write("image.bin", b"different\0image");
        let different = repo.commit("fix: 修复图片工作台的运行内存占用！！").await;
        assert_ne!(
            patch_id(repo.cwd(), source["hash"].as_str().unwrap())
                .await
                .unwrap(),
            patch_id(repo.cwd(), different["hash"].as_str().unwrap())
                .await
                .unwrap()
        );
        repo.git(&["commit", "--allow-empty", "-m", "空提交"]).await;
        let empty = repo.git(&["rev-parse", "HEAD"]).await;
        assert!(patch_id(repo.cwd(), &empty).await.unwrap().is_none());
        repo.write("image.bin", b"changed\0image");
        repo.commit("恢复图片以验证无冲突合并").await;
        repo.git(&["merge", "--no-ff", "feature", "-m", "合并"])
            .await;
        let merge = repo.git(&["rev-parse", "HEAD"]).await;
        assert!(patch_id(repo.cwd(), &merge).await.unwrap().is_none());
    });
}

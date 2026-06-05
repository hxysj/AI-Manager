use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::process::Command;

static REPO_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoInput {
    #[serde(default)]
    repo_id: String,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: String,
}

pub async fn add_repo(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let input: RepoInput = serde_json::from_value(payload)?;
    let repo_type = normalize_repo_type(&input.r#type);
    let source = normalize_repo_source(&repo_type, &input.source)?;
    let name = normalize_repo_name(&input.name, &source);
    let repo_id = create_repo_id();
    let local_path = if repo_type == "local" {
        if !directory_exists(&source).await? {
            return Err(ManagerError::System(format!(
                "本地仓库目录不存在：{}",
                source
            )));
        }

        source.clone()
    } else {
        let target_dir = Path::new(&paths.repos_dir).join(format!(
            "{}-{}",
            non_empty_slug(&name),
            &repo_id[..6]
        ));

        run_git(["clone", &source, &path_text(&target_dir)]).await?;
        path_text(target_dir)
    };
    let now = now_millis();
    let repo = json!({
      "id": repo_id,
      "name": name,
      "type": repo_type,
      "source": source,
      "localPath": local_path,
      "createdAt": now,
      "updatedAt": now,
      "lastSyncedAt": now,
      "status": "ready"
    });
    let mut repos = read_repos(paths)?;

    repos.insert(0, repo);
    write_repos(paths, repos).await
}

pub async fn sync_repo(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let input: RepoInput = serde_json::from_value(payload)?;
    let mut repos = read_repos(paths)?;
    let Some(repo) = repos
        .iter_mut()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(input.repo_id.as_str()))
    else {
        return Err(ManagerError::System("仓库不存在".to_string()));
    };

    if repo.get("type").and_then(Value::as_str) != Some("local") {
        let local_path = repo.get("localPath").and_then(Value::as_str).unwrap_or("");
        run_git(["-C", local_path, "pull"]).await?;
    }

    let now = now_millis();
    repo["updatedAt"] = json!(now);
    repo["lastSyncedAt"] = json!(now);
    repo["status"] = json!("ready");
    write_repos(paths, repos).await
}

pub async fn sync_all_repos(paths: &AppPaths) -> Result<(), ManagerError> {
    let repos = read_repos(paths)?;
    let repo_ids = repos
        .iter()
        .filter_map(|repo| repo.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();

    for repo_id in repo_ids {
        sync_repo(paths, json!({ "repoId": repo_id })).await?;
    }

    Ok(())
}

pub async fn remove_repo(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let input: RepoInput = serde_json::from_value(payload)?;
    let repos = read_repos(paths)?;
    let mut removed_repo = None;
    let next_repos = repos
        .into_iter()
        .filter(|repo| {
            if repo.get("id").and_then(Value::as_str) == Some(input.repo_id.as_str()) {
                removed_repo = Some(repo.clone());
                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();

    write_repos(paths, next_repos).await?;

    if let Some(repo) = removed_repo {
        if repo.get("type").and_then(Value::as_str) != Some("local") {
            let local_path = repo.get("localPath").and_then(Value::as_str).unwrap_or("");

            if !local_path.is_empty() {
                remove_dir_all_if_exists(local_path).await?;
            }
        }
    }

    Ok(())
}

fn read_repos(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    match std::fs::read_to_string(&paths.storage_files.repos) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn write_repos(paths: &AppPaths, repos: Vec<Value>) -> Result<(), ManagerError> {
    if let Some(parent) = Path::new(&paths.storage_files.repos).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        &paths.storage_files.repos,
        format!("{}\n", serde_json::to_string_pretty(&repos)?),
    )
    .await?;
    Ok(())
}

fn normalize_repo_type(value: &str) -> String {
    match value {
        "github" => "github".to_string(),
        "git" => "git".to_string(),
        _ => "local".to_string(),
    }
}

fn normalize_repo_source(repo_type: &str, source: &str) -> Result<String, ManagerError> {
    let source = source.trim();

    if repo_type == "github" {
        if source.is_empty() {
            return Err(ManagerError::System("GitHub 仓库地址不能为空".to_string()));
        }

        if source.starts_with("http://") || source.starts_with("https://") {
            return Ok(source.to_string());
        }

        return Ok(format!("https://github.com/{}.git", source));
    }

    if source.is_empty() {
        return Err(ManagerError::System("仓库来源不能为空".to_string()));
    }

    Ok(source.to_string())
}

fn normalize_repo_name(name: &str, source: &str) -> String {
    let name = name.trim();

    if !name.is_empty() {
        return name.to_string();
    }

    Path::new(source.trim_end_matches(".git"))
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn non_empty_slug(value: &str) -> String {
    let slug = slugify_name(value);

    if slug.is_empty() {
        "repo".to_string()
    } else {
        slug
    }
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

fn create_repo_id() -> String {
    let now = now_millis();
    let counter = REPO_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let process_id = std::process::id() as u128;
    let seed = now ^ (counter as u128) ^ (process_id << 32);
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

async fn directory_exists(target_path: &str) -> Result<bool, ManagerError> {
    match tokio::fs::metadata(target_path).await {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn remove_dir_all_if_exists(target_path: &str) -> Result<(), ManagerError> {
    match tokio::fs::remove_dir_all(target_path).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn run_git<const N: usize>(args: [&str; N]) -> Result<(), ManagerError> {
    let output = Command::new("git").args(args).output().await?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();

        return Err(ManagerError::System(if message.is_empty() {
            "Git 命令执行失败".to_string()
        } else {
            message
        }));
    }

    Ok(())
}

fn path_text(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().to_string()
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

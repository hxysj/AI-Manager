use crate::core::error::ManagerError;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

static SKILL_OPERATION: Mutex<()> = Mutex::const_new(());

fn error(message: &str) -> ManagerError {
    ManagerError::System(message.to_string())
}

pub(super) fn directory_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if "<>\"|?*\\/".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect()
}

fn manifest_path(target: &Value, name: &str) -> Result<PathBuf, ManagerError> {
    let skills = super::string_value(target.get("skillsPath"));
    if skills.is_empty() {
        return Err(error(
            target["skillsHint"]
                .as_str()
                .unwrap_or("未识别 Desktop 个人 Skills 目录，请刷新应用状态"),
        ));
    }
    let directory = directory_name(name);
    if directory.is_empty()
        || directory.ends_with(['.', ' '])
        || directory.chars().any(char::is_control)
    {
        return Err(error("Skill 名称不能映射为 Desktop 个人技能目录"));
    }
    let skills = Path::new(&skills);
    if skills.file_name().and_then(|name| name.to_str()) != Some("skills") {
        return Err(error("Desktop 仍使用旧插件目录，请刷新应用状态后重新安装"));
    }
    for ancestor in skills.ancestors() {
        if std::fs::symlink_metadata(ancestor)?
            .file_type()
            .is_symlink()
        {
            return Err(error("Desktop 个人 Skills 目录的祖先是链接，已拒绝写入"));
        }
    }
    let root = skills
        .parent()
        .ok_or_else(|| error("Desktop 个人 Skills 路径无效"))?;
    let plugin: Value =
        serde_json::from_slice(&std::fs::read(root.join(".claude-plugin/plugin.json"))?)?;
    if plugin["name"] != "anthropic-skills" {
        return Err(error("目标不是 Desktop 个人 Skills 插件目录"));
    }
    let manifest = root.join("manifest.json");
    if !std::fs::symlink_metadata(&manifest)?.file_type().is_file() {
        return Err(error("Desktop Skills 清单不是普通文件，已拒绝写入"));
    }
    Ok(manifest)
}

async fn read_manifest(path: &Path) -> Result<(Vec<u8>, Value), ManagerError> {
    let bytes = tokio::fs::read(path).await?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    if !manifest["skills"].is_array() {
        return Err(error(
            "Desktop Skills 清单格式无效，已停止操作以免覆盖现有技能",
        ));
    }
    Ok((bytes, manifest))
}

fn managed_entry<'a>(manifest: &'a Value, name: &str) -> Result<Option<&'a Value>, ManagerError> {
    let directory = directory_name(name);
    let mut found = None;
    for entry in manifest["skills"].as_array().unwrap() {
        let entry_name = entry["name"]
            .as_str()
            .ok_or_else(|| error("Desktop Skills 清单包含无效名称"))?;
        if !directory_name(entry_name).eq_ignore_ascii_case(&directory) {
            continue;
        }
        if entry_name != name
            || found.is_some()
            || entry["creatorType"] != "user"
            || entry["syncManaged"] != false
            || entry["managedBy"] != "ai-manager"
        {
            return Err(error(
                "Desktop 已有同名内置或个人 Skill，不能覆盖非本项目管理的内容",
            ));
        }
        found = Some(entry);
    }
    Ok(found)
}

async fn managed_link(target: &Value, path: &Path) -> Result<Option<PathBuf>, ManagerError> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(cause) => return Err(cause.into()),
    };
    if !metadata.file_type().is_symlink() {
        return Err(error(
            "Desktop Skill 目标已被真实目录占用，已拒绝覆盖或删除",
        ));
    }
    let link = tokio::fs::read_link(path).await?;
    let root = super::string_value(target.get("managedSkillsPath"));
    let snapshot = link
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| error("Desktop Skill 链接不属于本项目"))?;
    let signature = snapshot.file_name().unwrap_or_default().to_string_lossy();
    if root.is_empty()
        || signature.len() != 40
        || !signature.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !super::same_path(
            &snapshot
                .parent()
                .unwrap_or(Path::new(""))
                .components()
                .collect::<PathBuf>(),
            &Path::new(&root).components().collect::<PathBuf>(),
        )
        || link.file_name().and_then(|name| name.to_str()) != Some("skill")
        || link
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some("skills")
    {
        return Err(error("Desktop Skill 链接指向其他内容，已拒绝覆盖或删除"));
    }
    Ok(Some(link))
}

async fn write_manifest(
    path: &Path,
    original: &[u8],
    manifest: &mut Value,
) -> Result<(), ManagerError> {
    manifest["lastUpdated"] = json!(super::now_millis() as u64);
    let temporary = path.with_file_name(format!(".ai-manager-{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, serde_json::to_vec_pretty(manifest)?).await?;
    let result = async {
        if tokio::fs::read(path).await? != original {
            return Err(error("Desktop 正在更新 Skills 清单，请刷新后重试"));
        }
        tokio::fs::rename(&temporary, path).await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

pub(super) async fn install_state(
    target: &Value,
    skill: &Value,
) -> Result<&'static str, ManagerError> {
    let name = super::string_value(skill.get("name"));
    let manifest_path = manifest_path(target, &name)?;
    let (_, manifest) = read_manifest(&manifest_path).await?;
    let entry = managed_entry(&manifest, &name)?;
    let path = super::skill_target_path(target, &name);
    let link = managed_link(target, &path).await?;
    if link.is_none() && entry.is_none() {
        return Ok("not-installed");
    }
    let Some(entry) = entry else {
        return Ok("broken-link");
    };
    if entry["enabled"] != true {
        return Ok("disabled");
    }
    if link.is_none() || !Path::new(&super::string_value(skill.get("sourcePath"))).is_dir() {
        return Ok("broken-link");
    }
    let expected = super::desktop_skill_snapshot(target, skill)?.join("skills/skill");
    match (
        tokio::fs::canonicalize(&path).await,
        tokio::fs::canonicalize(expected).await,
    ) {
        (Ok(actual), Ok(expected))
            if super::same_path(&actual, &expected) && path.join("SKILL.md").is_file() =>
        {
            Ok("installed")
        }
        _ => Ok("broken-link"),
    }
}

pub(super) async fn install(target: &Value, skill: &Value) -> Result<(), ManagerError> {
    let _guard = SKILL_OPERATION.lock().await;
    let name = super::string_value(skill.get("name"));
    let manifest_path = manifest_path(target, &name)?;
    let (original, mut manifest) = read_manifest(&manifest_path).await?;
    let mut entry = managed_entry(&manifest, &name)?
        .cloned()
        .unwrap_or_else(|| json!({}));
    let path = super::skill_target_path(target, &name);
    let previous = managed_link(target, &path).await?;
    let source = super::prepare_desktop_skill(target, skill)
        .await?
        .join("skills/skill");
    if previous.is_some() {
        super::remove_managed_link(&path).await?;
    }
    if let Err(cause) = super::create_junction(&source, &path).await {
        if let Some(previous) = &previous {
            super::create_junction(previous, &path).await?;
        }
        return Err(cause);
    }
    entry["skillId"] = json!(name);
    entry["name"] = json!(name);
    entry["description"] = skill["description"].as_str().unwrap_or("").into();
    entry["creatorType"] = json!("user");
    entry["syncManaged"] = json!(false);
    entry["managedBy"] = json!("ai-manager");
    entry["updatedAt"] = json!(chrono::Utc::now().to_rfc3339());
    entry["enabled"] = json!(true);
    let entries = manifest["skills"].as_array_mut().unwrap();
    entries.retain(|entry| entry["name"] != name);
    entries.push(entry);
    if let Err(cause) = write_manifest(&manifest_path, &original, &mut manifest).await {
        super::remove_managed_link(&path).await?;
        if let Some(previous) = &previous {
            super::create_junction(previous, &path).await?;
        }
        return Err(cause);
    }
    Ok(())
}

pub(super) async fn uninstall(target: &Value, name: &str) -> Result<(), ManagerError> {
    let _guard = SKILL_OPERATION.lock().await;
    let manifest_path = manifest_path(target, name)?;
    let (original, mut manifest) = read_manifest(&manifest_path).await?;
    managed_entry(&manifest, name)?;
    let path = super::skill_target_path(target, name);
    let previous = managed_link(target, &path).await?;
    if previous.is_some() {
        super::remove_managed_link(&path).await?;
    }
    manifest["skills"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["name"] != name);
    if let Err(cause) = write_manifest(&manifest_path, &original, &mut manifest).await {
        if let Some(previous) = &previous {
            super::create_junction(previous, &path).await?;
        }
        return Err(cause);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_desktop_skills_preserve_corrupt_and_concurrently_changed_manifests() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "ai-manager-desktop-manifest-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join("manifest.json");
            std::fs::write(&path, b"{invalid").unwrap();
            assert!(read_manifest(&path).await.is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"{invalid");
            std::fs::write(&path, br#"{"skills":[]}"#).unwrap();
            let (original, mut manifest) = read_manifest(&path).await.unwrap();
            let concurrent = br#"{"skills":[{"name":"new-personal-skill"}],"preserve":true}"#;
            std::fs::write(&path, concurrent).unwrap();
            assert!(write_manifest(&path, &original, &mut manifest)
                .await
                .unwrap_err()
                .to_string()
                .contains("正在更新"));
            assert_eq!(std::fs::read(&path).unwrap(), concurrent);
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ai-manager-desktop-manifest-"));
            std::fs::remove_dir_all(resolved).unwrap();
        });
    }
}

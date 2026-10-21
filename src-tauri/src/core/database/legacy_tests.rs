use super::*;
use crate::core::paths::resolve_app_paths;
use crate::core::{
    image_prompt_store, image_store, provider_store, rule_store, skill_store, translation_store,
    usage_store,
};
use serde_json::json;

struct LegacyData {
    root: std::path::PathBuf,
    paths: AppPaths,
}

impl LegacyData {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("legacy-upgrade-{}", uuid::Uuid::new_v4()));
        let paths = resolve_app_paths(&root);
        let connection = open(&paths).unwrap();
        connection
            .execute_batch(include_str!("fixtures/legacy-0be86b3.sql"))
            .unwrap();
        Self { root, paths }
    }

    fn initialize(&self) -> Result<(), ManagerError> {
        // 与安装包启动相同的前置迁移，再验证首屏涉及的各存储模块。
        translation_store::initialize(&self.paths)?;
        image_prompt_store::initialize(&self.paths)?;
        image_store::initialize(&self.paths)?;
        provider_store::initialize(&self.paths)?;
        skill_store::initialize(&self.paths)?;
        usage_store::initialize(&self.paths)?;
        rule_store::initialize(&self.paths)?;
        Ok(())
    }

    fn initial_state(&self) -> serde_json::Value {
        let settings = crate::core::settings::normalize_app_settings(
            self.root.join("app-settings.json"),
            Some(json!({"dataPath":self.root,"cliConfigPaths":{
                "claude":self.root.join("claude"),"codex":self.root.join("codex")
            }})),
        );
        crate::core::storage_state::create_initial_state(&self.paths, &settings).unwrap()
    }
}

impl Drop for LegacyData {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn legacy_upgrade_empty_release_schema() {
    let data = LegacyData::new();
    data.initialize().unwrap();
    data.initialize().unwrap();
}

#[test]
fn legacy_upgrade_uuid_desktop_keys_and_active_account() {
    let data = LegacyData::new();
    let desktop_id = uuid::Uuid::new_v4().to_string();
    let account_id = uuid::Uuid::new_v4().to_string();
    let encrypted = crate::api::runtime_provider::encrypt_provider_key("legacy-secret").unwrap();
    let hash = crate::core::provider_key_usage::fingerprint("legacy-secret");
    let connection = open(&data.paths).unwrap();
    for (table, item) in [
        (
            "claude_desktop_providers",
            json!({"id":desktop_id,"name":"旧桌面供应商","mode":"custom"}),
        ),
        (
            "codex_accounts",
            json!({"id":account_id,"email":"legacy@example.test","auth":{"accessToken":"legacy-token"}}),
        ),
    ] {
        connection
            .execute(
                &format!("INSERT INTO {table} VALUES(?1,0,?2)"),
                params![item["id"].as_str().unwrap(), item.to_string()],
            )
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO provider_keys VALUES(?1,?2)",
            params![
                format!("claude-desktop:{desktop_id}"),
                json!({"keys":[{"id":"legacy-key","value":encrypted}],"activeKeyId":"legacy-key"})
                    .to_string()
            ],
        )
        .unwrap();
    connection.execute("INSERT INTO provider_key_usage(provider_id,key_id,key_hash,request_count,success_count) VALUES(?1,'legacy-key',?2,7,7)",
        params![format!("claude-desktop:{desktop_id}"),hash]).unwrap();
    connection
        .execute(
            "INSERT INTO codex_active_account VALUES(1,?1)",
            [&account_id],
        )
        .unwrap();
    drop(connection);
    data.initialize().unwrap();
    assert_eq!(
        provider_store::read_keys(&data.paths).unwrap()[&format!("claude-desktop:{desktop_id}")]
            ["activeKeyId"],
        "legacy-key"
    );
    assert_eq!(
        provider_store::read_active_codex_account_id(&data.paths).unwrap(),
        account_id
    );
    data.initialize().unwrap();
    let connection = open(&data.paths).unwrap();
    assert_eq!(connection.query_row("SELECT request_count FROM provider_keys WHERE provider_id=?1 AND id='legacy-key' AND is_current=1",
        [&desktop_id], |row| row.get::<_,i64>(0)).unwrap(), 7);
    drop(connection);
    assert_eq!(data.initial_state()["codexAccounts"][0]["id"], account_id);
    crate::api::runtime_provider::capture_key_bindings(&data.paths).unwrap();
}

#[test]
fn legacy_upgrade_blob_images_and_reference_files() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    let task = json!({"id":"legacy-task","status":"completed","request":{"conversationId":"legacy-chat","mode":"edit","prompt":"旧图片"},"images":[{"format":"png","width":1,"height":1}]});
    connection
        .execute(
            "INSERT INTO image_tasks VALUES('legacy-task',100,'completed',?1,'')",
            [task.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO image_outputs VALUES('legacy-task',0,'png',?1)",
            [b"legacy-output".as_slice()],
        )
        .unwrap();
    connection.execute("INSERT INTO image_inputs VALUES('legacy-task',0,'data:image/png;base64,bGVnYWN5LWlucHV0')", []).unwrap();
    connection.execute("INSERT INTO image_inputs VALUES('legacy-task',-1,'data:image/png;base64,bGVnYWN5LW1hc2s=')", []).unwrap();
    drop(connection);
    data.initialize().unwrap();
    let result = image_store::detail(&data.paths, "legacy-task").unwrap();
    assert_eq!(result["request"]["conversationId"], "legacy-chat");
    assert_eq!(result["editMode"], true);
    let connection = open(&data.paths).unwrap();
    let images: String = connection
        .query_row(
            "SELECT images_json FROM image_tasks WHERE id='legacy-task'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let images: serde_json::Value = serde_json::from_str(&images).unwrap();
    let id = images[0]["imageId"].as_str().unwrap();
    assert_eq!(
        std::fs::read(
            std::path::Path::new(&data.paths.workspace_root)
                .join(format!("image-outputs/legacy-chat/legacy-task-{id}.png"))
        )
        .unwrap(),
        b"legacy-output"
    );
    let references: String = connection
        .query_row(
            "SELECT reference_image_ids FROM image_tasks WHERE id='legacy-task'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let reference = references.split(',').next().unwrap();
    assert!(!reference.is_empty());
    assert_eq!(
        std::fs::read(
            std::path::Path::new(&data.paths.workspace_root)
                .join(format!("image-inputs/legacy-task/{reference}.png"))
        )
        .unwrap(),
        b"legacy-input"
    );
    drop(connection);
    data.initialize().unwrap();
    let inputs = image_store::inputs(&data.paths, "legacy-task").unwrap();
    assert_eq!(
        inputs["images"],
        json!(["data:image/png;base64,bGVnYWN5LWlucHV0"])
    );
    assert_eq!(inputs["mask"], "data:image/png;base64,bGVnYWN5LW1hc2s=");
}

#[test]
fn legacy_upgrade_prompt_categories_without_old_index() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    connection
        .execute_batch("DROP INDEX idx_image_prompt_category;")
        .unwrap();
    connection
        .execute(
            "INSERT INTO image_prompt_templates VALUES('old-prompt',0,'旧词库',?1,?2)",
            params![
                json!({"title":"中文标题","categoryIds":["Category:old"]}).to_string(),
                json!({"id":"old-prompt","title":"Original","prompt":"旧提示词正文"}).to_string(),
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO image_prompt_categories VALUES('old-prompt','Category:old')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO image_prompt_catalog VALUES(1,?1)",
            [json!({"categories":[
                {"id":"Category:old","group":"分类","title":"旧分类","originalTitle":"Old category"}
            ]})
            .to_string()],
        )
        .unwrap();
    drop(connection);
    data.initialize().unwrap();
    data.initialize().unwrap();
    let catalog = image_prompt_store::dispatch(
        &data.paths,
        Path::new("unused-seed.json"),
        "tools:image-prompts-catalog",
        json!({}),
    )
    .unwrap();
    assert_eq!(catalog["categories"][0]["title"], "旧分类");
    assert_eq!(catalog["categories"][0]["group"], "分类");
    let item = image_prompt_store::dispatch(
        &data.paths,
        Path::new("unused-seed.json"),
        "tools:image-prompts-detail",
        json!({"id":"old-prompt"}),
    )
    .unwrap();
    assert_eq!(item["prompt"], "旧提示词正文");
}

#[test]
fn legacy_upgrade_prompt_failure_rolls_back_all_source_tables() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    connection
        .execute(
            "INSERT INTO image_prompt_templates VALUES('old-prompt',0,'旧词库','{}','broken-json')",
            [],
        )
        .unwrap();
    connection
        .execute("INSERT INTO image_prompt_catalog VALUES(1,'{}')", [])
        .unwrap();
    assert!(image_prompt_store::initialize(&data.paths).is_err());
    assert_eq!(
        connection
            .query_row(
                "SELECT payload_json FROM image_prompt_templates",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "broken-json"
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM image_prompt_catalog", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='image_prompt_templates_legacy'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    connection
        .execute("UPDATE image_prompt_templates SET payload_json='{}'", [])
        .unwrap();
    drop(connection);
    data.initialize().unwrap();
}

#[test]
fn legacy_upgrade_image_write_failure_can_resume_without_losing_blob() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    for index in 0..12 {
        let id = format!("old-{index}");
        let payload = json!({"request":{"conversationId":"old-chat"}});
        connection
            .execute(
                "INSERT INTO image_tasks VALUES(?1,100,'completed',?2,'')",
                params![id, payload.to_string()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO image_outputs VALUES(?1,0,'png',?2)",
                params![id, vec![index as u8; 1024]],
            )
            .unwrap();
    }
    let output_root = Path::new(&data.paths.workspace_root).join("image-outputs");
    std::fs::write(&output_root, b"blocked-directory").unwrap();
    assert!(image_store::initialize(&data.paths).is_err());
    assert_eq!(
        connection
            .query_row("SELECT SUM(length(bytes)) FROM image_outputs", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        12 * 1024
    );
    std::fs::remove_file(&output_root).unwrap();
    drop(connection);
    data.initialize().unwrap();
    data.initialize().unwrap();
    let files = std::fs::read_dir(output_root.join("old-chat"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 12);
    let contents = files
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect::<Vec<_>>();
    for index in 0..12 {
        assert!(contents.contains(&vec![index; 1024]));
    }
}

#[test]
fn legacy_upgrade_populated_skills_rules_usage_and_translation() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    for (table, key, item) in [
        (
            "skills",
            "old-skill",
            json!({"id":"old-skill","name":"old-skill","description":"技能描述","content":"旧正文","installedTargets":["claude"]}),
        ),
        (
            "skill_groups",
            "old-group",
            json!({"id":"old-group","name":"旧分组","skillIds":["old-skill"]}),
        ),
        (
            "skill_repositories",
            "old-repo",
            json!({"id":"old-repo","name":"旧仓库","source":"https://example.test/repo.git"}),
        ),
        (
            "skill_repository_cache",
            "old-repo",
            json!({"id":"old-repo","status":"ready","skills":[{"name":"cached-skill","description":"缓存描述"}],"lastSyncedAt":100}),
        ),
    ] {
        connection
            .execute(
                &format!("INSERT INTO {table} VALUES(?1,0,?2)"),
                params![key, item.to_string()],
            )
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO rule_prompts VALUES('old-rule','common','old.md',100,?1)",
            [json!({"id":"old-rule","name":"旧规则"}).to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO rule_profiles VALUES('claude',?1)",
            [json!({"activePromptId":"old-rule"}).to_string()],
        )
        .unwrap();
    connection.execute("INSERT INTO usage_logs(request_id,raw_path,created_at,app_type,payload_json) VALUES('old-request','old-session',100,'claude',?1)",
        [json!({"inputTokens":123,"outputTokens":45,"totalCostUsd":0.25,"costLockedAt":100}).to_string()]).unwrap();
    connection
        .execute(
            "INSERT INTO usage_request_records VALUES('old-request',100,?1)",
            [
                json!({"requestId":"old-request","providerId":"old-provider","apiKeyId":"old-key"})
                    .to_string(),
            ],
        )
        .unwrap();
    connection
        .execute("INSERT INTO usage_pricing_config VALUES(1,7.5)", [])
        .unwrap();
    connection.execute("INSERT INTO codex_quota_stages VALUES('old-stage','old-account','primary',1000,100,200,0,?1)",
        [json!({"id":"old-stage","usedPercent":25.0,"limitWindowSeconds":900}).to_string()]).unwrap();
    connection
        .execute(
            "INSERT INTO translation_history VALUES('old-translation',100,?1)",
            [
                json!({"sourceText":"原文","translatedText":"translation","status":"completed"})
                    .to_string(),
            ],
        )
        .unwrap();
    connection.execute("INSERT INTO translation_requests VALUES('old-translation-request','old-translation',100,?1)",
        [json!({"inputTokens":10,"outputTokens":20,"status":"completed"}).to_string()]).unwrap();
    drop(connection);
    data.initialize().unwrap();
    data.initialize().unwrap();
    let skills = skill_store::read_skills(&data.paths).unwrap();
    assert_eq!(skills[0]["description"], "技能描述");
    assert_eq!(skills[0]["groupId"], "old-group");
    assert!(skills[0].get("content").is_none());
    assert_eq!(
        skill_store::read_repositories(&data.paths).unwrap()[0]["skills"][0]["name"],
        "cached-skill"
    );
    assert_eq!(
        rule_store::read_profiles(&data.paths).unwrap()["claude"]["activePromptId"],
        "old-rule"
    );
    let logs = usage_store::read_all_logs(&data.paths).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["inputTokens"], 123);
    assert_eq!(logs[0]["providerId"], "old-provider");
    assert_eq!(logs[0]["apiKeyId"], "old-key");
    assert_eq!(logs[0]["totalCostUsd"], 0.25);
    assert_eq!(
        usage_store::read_pricing(&data.paths).unwrap()["exchangeRate"],
        7.5
    );
    assert_eq!(
        translation_store::list(&data.paths, &json!({})).unwrap()["items"][0]["sourceText"],
        "原文"
    );
    let connection = open(&data.paths).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT used_percent FROM codex_quota_stages WHERE id='old-stage'",
                [],
                |row| row.get::<_, f64>(0)
            )
            .unwrap(),
        25.0
    );
    assert_eq!(connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN (
        'providers_legacy','image_outputs','image_inputs','image_assets','image_prompt_catalog',
        'rule_prompts','skill_installs','skill_trash','usage_logs','usage_request_records','translation_history','translation_requests')",
        [], |row|row.get::<_,i64>(0)).unwrap(),0);
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(connection);
    let state = data.initial_state();
    assert_eq!(state["skills"][0]["description"], "技能描述");
    assert_eq!(state["rules"]["prompts"][0]["id"], "old-rule");
}

#[test]
fn legacy_upgrade_pre_database_json_and_separate_usage_database() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    for name in read_table_names(&connection, "main").unwrap() {
        connection
            .execute_batch(&format!("DROP TABLE {}", quote_identifier(&name)))
            .unwrap();
    }
    drop(connection);
    let files = &data.paths.storage_files;
    for (path, value) in [
        (
            &files.providers,
            json!([{"id":"json-provider","name":"旧供应商","cli":"codex"}]),
        ),
        (
            &files.skills,
            json!([{"id":"json-skill","name":"json-skill","description":"JSON 技能"}]),
        ),
        (
            &files.skill_groups,
            json!([{"id":"json-group","name":"JSON 分组","skillIds":["json-skill"]}]),
        ),
        (
            &files.runtime_provider_keys,
            json!({"json-provider":{"keys":[{"id":"json-key","value":"encrypted"}],"activeKeyId":"json-key"}}),
        ),
        (
            &files.runtime_profiles,
            json!([{"id":"json-profile","cli":"codex","providerId":"json-provider","model":"old-model"}]),
        ),
        (
            &files.usage_request_records,
            json!([{"requestId":"json-request","providerId":"json-provider","apiKeyId":"json-key","createdAt":100}]),
        ),
        (
            &files.usage_logs,
            json!([{"requestId":"json-request","inputTokens":11,"totalCostUsd":0.2}]),
        ),
    ] {
        std::fs::create_dir_all(Path::new(path).parent().unwrap()).unwrap();
        std::fs::write(path, value.to_string()).unwrap();
    }
    std::fs::create_dir_all(Path::new(&files.usage_database).parent().unwrap()).unwrap();
    let usage = Connection::open(&files.usage_database).unwrap();
    usage
        .execute_batch(include_str!("fixtures/legacy-0be86b3.sql"))
        .unwrap();
    usage.execute("INSERT INTO usage_logs(request_id,raw_path,created_at,payload_json) VALUES('separate-request','old-file',100,?1)",
        [json!({"inputTokens":42}).to_string()]).unwrap();
    drop(usage);
    data.initialize().unwrap();
    data.initialize().unwrap();
    assert_eq!(
        provider_store::read_providers(&data.paths).unwrap()[0]["id"],
        "json-provider"
    );
    assert_eq!(
        provider_store::read_keys(&data.paths).unwrap()["json-provider"]["activeKeyId"],
        "json-key"
    );
    assert_eq!(
        skill_store::read_skills(&data.paths).unwrap()[0]["groupId"],
        "json-group"
    );
    let logs = usage_store::read_all_logs(&data.paths).unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(
        logs.iter()
            .find(|log| log["requestId"] == "separate-request")
            .unwrap()["inputTokens"],
        42
    );
    let json_log = logs
        .iter()
        .find(|log| log["requestId"] == "json-request")
        .unwrap();
    assert_eq!(json_log["providerId"], "json-provider");
    assert_eq!(json_log["apiKeyId"], "json-key");
    assert_eq!(json_log["inputTokens"], 11);
    assert_eq!(json_log["totalCostUsd"], 0.2);
    assert!(!Path::new(&files.providers).exists());
    assert!(!Path::new(&files.usage_database).exists());
}

#[test]
fn legacy_upgrade_web_sessions_keep_latest_reuse_and_deleted_task_state() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    connection.execute_batch("CREATE TABLE image_web_sessions(local_id TEXT NOT NULL,account_id TEXT NOT NULL,payload_json TEXT NOT NULL,PRIMARY KEY(local_id,account_id));").unwrap();
    for (local, updated) in [("older", 10), ("newer", 20)] {
        connection.execute("INSERT INTO image_web_sessions VALUES(?1,'old-account',?2)",
            params![local,json!({"conversationId":"reused-upstream","updatedAt":updated,"parentMessageId":local}).to_string()]).unwrap();
    }
    let task = json!({"request":{"accountId":"old-account","generationMode":"web","conversationId":"local-chat"},"recovery":{"conversationId":"deleted-upstream"}});
    connection
        .execute(
            "INSERT INTO image_tasks VALUES('old-web-task',100,'completed',?1,'')",
            [task.to_string()],
        )
        .unwrap();
    connection.execute("INSERT INTO image_web_conversations VALUES('old-record','old-account','local-chat','deleted-upstream','parent',100,200,200,1)", []).unwrap();
    drop(connection);
    data.initialize().unwrap();
    data.initialize().unwrap();
    let sessions = image_store::list_web_conversations(&data.paths).unwrap();
    assert_eq!(sessions.as_array().unwrap().len(), 1);
    assert_eq!(sessions[0]["parentMessageId"], "newer");
    let task = image_store::detail(&data.paths, "old-web-task").unwrap();
    assert_eq!(task["webSessionDeleted"], true);
    assert_eq!(task["webSessionId"], "deleted-upstream");
}

#[test]
fn legacy_upgrade_resumes_partially_copied_tasks() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    for id in ["copied-task", "pending-task"] {
        connection
            .execute(
                "INSERT INTO image_tasks VALUES(?1,100,'completed',?2,'')",
                params![id, json!({"request":{"prompt":id}}).to_string()],
            )
            .unwrap();
    }
    drop(connection);
    data.initialize().unwrap();
    let connection = open(&data.paths).unwrap();
    // 模拟旧版本在复制部分任务后退出，既有的新记录应保留，遗漏记录应补齐。
    connection
        .execute_batch(
            "CREATE TABLE image_tasks_legacy AS SELECT * FROM image_tasks;
        DELETE FROM image_tasks WHERE id='pending-task';
        UPDATE image_tasks SET prompt='已更新的新记录' WHERE id='copied-task';",
        )
        .unwrap();
    drop(connection);
    data.initialize().unwrap();
    data.initialize().unwrap();
    assert_eq!(
        image_store::detail(&data.paths, "copied-task").unwrap()["request"]["prompt"],
        "已更新的新记录"
    );
    assert_eq!(
        image_store::detail(&data.paths, "pending-task").unwrap()["request"]["prompt"],
        "pending-task"
    );
    let connection = open(&data.paths).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='image_tasks_legacy'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn legacy_upgrade_resumes_renamed_prompt_tables() {
    let data = LegacyData::new();
    let connection = open(&data.paths).unwrap();
    connection
        .execute(
            "INSERT INTO image_prompt_templates VALUES('pending-prompt',0,'旧词库','{}',?1)",
            [json!({"title":"待迁移提示词","prompt":"保留正文"}).to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO image_prompt_categories VALUES('pending-prompt','Category:pending')",
            [],
        )
        .unwrap();
    connection
        .execute_batch(
            "ALTER TABLE image_prompt_templates RENAME TO image_prompt_templates_legacy;
        ALTER TABLE image_prompt_categories RENAME TO image_prompt_categories_legacy;",
        )
        .unwrap();
    drop(connection);
    data.initialize().unwrap();
    data.initialize().unwrap();
    let item = image_prompt_store::dispatch(
        &data.paths,
        Path::new("unused-seed.json"),
        "tools:image-prompts-detail",
        json!({"id":"pending-prompt"}),
    )
    .unwrap();
    assert_eq!(item["prompt"], "保留正文");
    let connection = open(&data.paths).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT category_ids FROM image_prompt_templates WHERE id='pending-prompt'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "Category:pending"
    );
    assert_eq!(connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('image_prompt_templates_legacy','image_prompt_categories_legacy')", [], |row|row.get::<_,i64>(0)).unwrap(),0);
}

#[test]
fn legacy_upgrade_preserves_structured_web_sessions_without_task_list() {
    let data = LegacyData::new();
    data.initialize().unwrap();
    let connection = open(&data.paths).unwrap();
    connection.execute_batch("ALTER TABLE image_web_sessions DROP COLUMN task_ids;
        INSERT INTO image_web_sessions VALUES('session','account','local','upstream','parent',100,200);").unwrap();
    drop(connection);
    data.initialize().unwrap();
    data.initialize().unwrap();
    let sessions = image_store::list_web_conversations(&data.paths).unwrap();
    assert_eq!(sessions.as_array().unwrap().len(), 1);
    assert_eq!(sessions[0]["conversationId"], "upstream");
    assert_eq!(sessions[0]["parentMessageId"], "parent");
}

use super::*;
use crate::api::{multi_protocol_gateway::service, service_provider};
use crate::core::{paths::resolve_app_paths, provider_store};
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    paths: AppPaths,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ai-manager-service-test-{}", id()));
        let paths = resolve_app_paths(&root);
        initialize(&paths).unwrap();
        Self { root, paths }
    }
    fn provider(&self) -> String {
        text(&save(&self.paths,&json!({"name":"测试服务商","baseUrl":"http://127.0.0.1:1/v1","apiKey":"test-secret",
            "models":[{"upstreamModelId":"model","inputPriceMicros":1_000_000,"cachedInputPriceMicros":500_000,"outputPriceMicros":2_000_000}],
            "bindings":[{"agentType":"codex","enabled":true},{"agentType":"claude","enabled":true}]})).unwrap(),"id").into()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.parent() == Some(std::env::temp_dir().as_path())
            && self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ai-manager-service-test-")
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

#[test]
fn service_enable_automatically_copies_codex_and_official_accounts_without_overwriting_edits() {
    let f = Fixture::new();
    let keys = Map::from_iter([("codex-api".into(), json!({"apiKey":"legacy-secret"}))]);
    provider_store::write_provider_bundle(&f.paths, &[
        json!({"id":"codex-api","cli":"codex","name":"旧 Codex","baseUrl":"https://example.test/v1",
            "runtimeConfig":{"mainModel":"model"}}),
        json!({"id":"claude-api","cli":"claude","name":"旧 Claude","baseUrl":"https://example.test"})
    ], &[], &[], &keys).unwrap();
    provider_store::write_codex_accounts(&f.paths, &[json!({
        "id":"official","type":"codex","email":"official@example.test","account_id":"external",
        "access_token":"access","refresh_token":"refresh","id_token":"id"
    })]).unwrap();
    let c = database::open(&f.paths).unwrap();
    let before = query(&c, "SELECT * FROM providers ORDER BY id", &[]).unwrap();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let copied = state(&f.paths).unwrap();
    assert_eq!(copied["providers"].as_array().unwrap().len(), 2);
    assert!(copied["providers"].as_array().unwrap().iter().any(|p| p["id"] == "codex-api"));
    assert_eq!(account(&c, "account:official").unwrap()["accessToken"], "access");
    c.execute("UPDATE service_provider_configs SET name='新模式修改' WHERE id='codex-api'", []).unwrap();
    let runs: i64 = c.query_row("SELECT count(*) FROM service_provider_migrations", [], |r| r.get(0)).unwrap();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    assert_eq!(c.query_row("SELECT count(*) FROM service_provider_migrations", [], |r| r.get::<_,i64>(0)).unwrap(), runs);
    service_provider::switch_backend(&f.paths, "legacy").unwrap();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    assert_eq!(config(&c, "codex-api").unwrap()["name"], "新模式修改");
    assert_eq!(query(&c, "SELECT * FROM providers ORDER BY id", &[]).unwrap(), before);
}

#[test]
fn service_enable_keeps_legacy_mode_when_automatic_migration_fails() {
    let f = Fixture::new();
    provider_store::write_provider_bundle(&f.paths, &[
        json!({"id":"broken","cli":"codex","name":"损坏配置","baseUrl":"https://example.test/v1"})
    ], &[], &[], &Map::new()).unwrap();
    let c = database::open(&f.paths).unwrap();
    c.execute("UPDATE providers SET model_catalog='not-json' WHERE id='broken'", []).unwrap();
    assert!(service_provider::switch_backend(&f.paths, "service").is_err());
    assert!(!active(&f.paths).unwrap());
    assert_eq!(query(&c, "SELECT status FROM service_provider_migrations", &[]).unwrap()[0]["status"], "failed");
    assert!(state(&f.paths).unwrap()["providers"].as_array().unwrap().is_empty());
}

#[test]
fn service_schema_enforces_model_ownership_and_configuration_cas() {
    let f = Fixture::new();
    let a = f.provider();
    let b = f.provider();
    let c = database::open(&f.paths).unwrap();
    let model: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1",
            [&a],
            |r| r.get(0),
        )
        .unwrap();
    assert!(c
        .execute(
            "UPDATE service_provider_agent_bindings SET default_model_id=?2 WHERE provider_id=?1",
            params![b, model]
        )
        .is_err());
    assert!(c
        .execute(
            "UPDATE service_provider_models SET input_price_micros=-1 WHERE id=?1",
            [&model]
        )
        .is_err());
    let mut value = config(&c, &a).unwrap();
    value["name"] = json!("新名称");
    value["enabled"] = json!(true);
    save(&f.paths, &value).unwrap();
    assert!(save(&f.paths, &value).is_err());
    assert!(save(
        &f.paths,
        &json!({"name":"错误地址","baseUrl":"https://user:secret@example.com"})
    )
    .is_err());
}

#[test]
fn service_initial_state_and_refresh_work_without_legacy_provider_tables() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let settings = crate::core::settings::normalize_app_settings(
        f.root.join("app-settings.json"),
        Some(json!({
            "dataPath": f.root,
            "cliConfigPaths": {
                "codex": f.root.join("codex"),
                "claude": f.root.join("claude")
            }
        })),
    );
    // 新模式的主状态入口必须直接使用新表，旧 Provider 初始化在此模式会拒绝执行。
    service_provider::recover(&f.paths).unwrap();
    crate::api::runtime_provider::capture_key_bindings(&f.paths).unwrap();
    let mut state = crate::core::storage_state::create_initial_state(&f.paths, &settings).unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(crate::api::runtime_provider::refresh_drift(
            &f.paths,
            &state["cliTargets"],
        ))
        .unwrap();
    crate::core::storage_state::refresh_runtime_provider_state(&f.paths, &mut state).unwrap();
    assert_eq!(state["serviceProviders"]["backend"], "service");
    assert!(state["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == provider));
    assert!(state["runtimeModels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["providerId"] == provider));
    let c = database::open(&f.paths).unwrap();
    assert!(!table_exists(&c, "providers").unwrap());
    assert!(!table_exists(&c, "provider_keys").unwrap());
    assert!(!f.root.join("codex/config.toml").exists());
    assert!(!f.root.join("claude/settings.json").exists());
}

#[test]
fn service_keys_restore_existing_version_for_a_b_a_and_never_expose_secret() {
    let f = Fixture::new();
    let p = f.provider();
    let mut c = database::open(&f.paths).unwrap();
    for secret in ["a", "b", "a"] {
        let tx = c.transaction().unwrap();
        save_key(&tx, &p, "main", "主 Key", secret).unwrap();
        tx.commit().unwrap();
    }
    let rows=query(&c,"SELECT fingerprint,is_current,secret_ciphertext FROM service_provider_keys WHERE provider_id=?1 AND logical_id='main'",&[&p]).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|v| v["is_current"] == 1).count(), 1);
    assert!(rows
        .iter()
        .filter(|v| v["is_current"] == 0)
        .all(|v| v["secret_ciphertext"] == ""));
    assert_eq!(credential(&c, &p).unwrap()["secret"], "a");
    let public = state(&f.paths).unwrap().to_string();
    assert!(!public.contains("test-secret"));
    assert!(!public.contains("secret_ciphertext"));
}

#[test]
fn service_migration_is_idempotent_and_source_tables_are_read_only() {
    let f = Fixture::new();
    let keys = Map::from_iter([("old-codex".into(), json!({"apiKey":"legacy-secret"}))]);
    provider_store::write_provider_bundle(&f.paths,&[
        json!({"id":"old-codex","cli":"codex","name":"旧 Codex","icon":"alibabacloud.svg","baseUrl":"https://example.test/v1","runtimeConfig":{"mainModel":"m",
            "modelCatalog":{"models":[{"slug":"m","context_window":10000,"custom_flag":true}]}}}),
        json!({"id":"old-claude","cli":"claude","name":"旧 Claude","baseUrl":"https://example.test"})
    ],&[],&[],&keys).unwrap();
    let c = database::open(&f.paths).unwrap();
    let before = query(&c, "SELECT * FROM providers ORDER BY id", &[]).unwrap();
    for table in ["providers", "provider_keys"] {
        for event in ["INSERT", "UPDATE", "DELETE"] {
            c.execute_batch(&format!("CREATE TRIGGER protect_{table}_{event} BEFORE {event} ON {table} BEGIN SELECT RAISE(ABORT,'禁止写旧表'); END;")).unwrap();
        }
    }
    migrate(&f.paths).unwrap();
    let first = state(&f.paths).unwrap();
    assert_eq!(first["providers"].as_array().unwrap().len(), 1);
    assert_eq!(first["providers"][0]["icon"], "alibabacloud.svg");
    let provider = text(&first["providers"][0], "id");
    assert_eq!(first["models"][0]["catalog"]["custom_flag"], true);
    c.execute(
        "UPDATE service_provider_configs SET name='新模式已编辑',revision=revision+1 WHERE id=?1",
        [provider],
    )
    .unwrap();
    migrate(&f.paths).unwrap();
    assert_eq!(
        state(&f.paths).unwrap()["providers"][0]["name"],
        "新模式已编辑"
    );
    assert_eq!(
        query(&c, "SELECT * FROM providers ORDER BY id", &[]).unwrap(),
        before
    );
    service_provider::switch_backend(&f.paths, "service").unwrap();
    assert!(provider_store::initialize(&f.paths).is_err());
    remove(&f.paths, provider, 2).unwrap();
    migrate(&f.paths).unwrap();
    assert!(state(&f.paths).unwrap()["providers"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn service_icons_upgrade_existing_migration_without_writing_legacy_tables() {
    let f = Fixture::new();
    let custom = "data:image/png;base64,aWNvbg==";
    provider_store::write_provider_bundle(
        &f.paths,
        &[
            json!({"id":"builtin","cli":"codex","name":"内置图标","icon":"alibabacloud.svg","baseUrl":"https://example.test/v1","runtimeConfig":{"mainModel":"model"}}),
            json!({"id":"custom","cli":"codex","name":"上传图标","icon":custom,"baseUrl":"https://example.test/v1"}),
        ],
        &[],
        &[],
        &Map::new(),
    )
    .unwrap();
    migrate(&f.paths).unwrap();
    let c = database::open(&f.paths).unwrap();
    let before = query(&c, "SELECT * FROM providers ORDER BY id", &[]).unwrap();
    let models = query(&c, "SELECT * FROM service_provider_models ORDER BY id", &[]).unwrap();
    // 模拟已经启用新模式的 v1 数据库，升级过程禁止对旧表执行任何写入。
    c.execute_batch(
        "ALTER TABLE service_provider_configs DROP COLUMN icon;
        UPDATE app_metadata SET value='1' WHERE key='service_provider:schema_version';
        INSERT INTO app_metadata(key,value) VALUES('service_provider:active_backend','service');
        UPDATE service_provider_configs SET name='新模式已编辑',revision=5 WHERE id='builtin';",
    )
    .unwrap();
    for event in ["INSERT", "UPDATE", "DELETE"] {
        c.execute_batch(&format!("CREATE TRIGGER protect_icons_{event} BEFORE {event} ON providers BEGIN SELECT RAISE(ABORT,'禁止写旧表'); END;")).unwrap();
    }
    initialize(&f.paths).unwrap();
    assert_eq!(config(&c, "builtin").unwrap()["icon"], "alibabacloud.svg");
    assert_eq!(config(&c, "custom").unwrap()["icon"], custom);
    assert_eq!(config(&c, "builtin").unwrap()["name"], "新模式已编辑");
    assert_eq!(config(&c, "builtin").unwrap()["revision"], 5);
    assert_eq!(
        query(&c, "SELECT * FROM providers ORDER BY id", &[]).unwrap(),
        before
    );
    assert_eq!(
        query(&c, "SELECT * FROM service_provider_models ORDER BY id", &[]).unwrap(),
        models
    );
    assert!(state(&f.paths).unwrap()["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "custom" && p["icon"] == custom));
    assert!(public_catalog(&f.paths).unwrap()["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "builtin" && p["icon"] == "alibabacloud.svg"));
    c.execute_batch(
        "UPDATE service_provider_configs SET icon='codex.svg' WHERE id='builtin';
        UPDATE app_metadata SET value='1' WHERE key='service_provider:schema_version';",
    )
    .unwrap();
    initialize(&f.paths).unwrap();
    assert_eq!(config(&c, "builtin").unwrap()["icon"], "codex.svg");
    // 后续主动清空图标后，重复初始化和迁移不能把旧图标重新覆盖回来。
    c.execute(
        "UPDATE service_provider_configs SET icon='' WHERE id='builtin'",
        [],
    )
    .unwrap();
    migrate(&f.paths).unwrap();
    assert_eq!(config(&c, "builtin").unwrap()["icon"], "");
}

#[test]
fn service_icons_save_preserves_omitted_field_and_accepts_custom_or_empty_icon() {
    let f = Fixture::new();
    let saved = save(
        &f.paths,
        &json!({
            "name":"图标配置","baseUrl":"https://example.test/v1","icon":"alibabacloud.svg"
        }),
    )
    .unwrap();
    let c = database::open(&f.paths).unwrap();
    let provider = text(&saved, "id");
    let mut value = config(&c, provider).unwrap();
    assert_eq!(value["icon"], "alibabacloud.svg");
    value.as_object_mut().unwrap().remove("icon");
    value["name"] = json!("仅修改名称");
    save(&f.paths, &value).unwrap();
    let mut value = config(&c, provider).unwrap();
    assert_eq!(value["icon"], "alibabacloud.svg");
    value["icon"] = json!("data:image/png;base64,aWNvbg==");
    save(&f.paths, &value).unwrap();
    let mut saved = config(&c, provider).unwrap();
    assert_eq!(saved["icon"], value["icon"]);
    saved["icon"] = json!("");
    save(&f.paths, &saved).unwrap();
    assert_eq!(config(&c, provider).unwrap()["icon"], "");
}

#[test]
fn service_takeover_restores_original_and_detects_external_file_changes() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let c = database::open(&f.paths).unwrap();
    let model: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1",
            [&provider],
            |r| r.get(0),
        )
        .unwrap();
    let directory = f.root.join("client");
    std::fs::create_dir_all(&directory).unwrap();
    let original = "model = \"old\"\n[projects.\"D:/project\"]\ntrust_level = \"trusted\"\n";
    std::fs::write(directory.join("config.toml"), original).unwrap();
    let value =
        json!({"agentType":"codex","providerId":provider,"modelId":model,"configPath":directory});
    let instance = service_provider::activate(&f.paths, &json!([]), &value).unwrap();
    let applied = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(applied.contains("supports_websockets = false"));
    assert!(applied.contains("trust_level = \"trusted\""));
    assert!(!applied.contains("test-secret"));
    std::fs::write(
        directory.join("config.toml"),
        format!("{applied}# 用户修改\n"),
    )
    .unwrap();
    assert!(service_provider::switch_backend(&f.paths, "legacy").is_err());
    assert!(active(&f.paths).unwrap());
    assert!(std::fs::read_to_string(directory.join("config.toml"))
        .unwrap()
        .contains("# 用户修改"));
    std::fs::write(directory.join("config.toml"), applied).unwrap();
    service_provider::detach(&f.paths, text(&instance, "instanceId")).unwrap();
    assert_eq!(
        std::fs::read_to_string(directory.join("config.toml")).unwrap(),
        original
    );
    assert!(!directory.join(".ai_manager_service_models.json").exists());
    service_provider::switch_backend(&f.paths, "legacy").unwrap();
    assert!(!active(&f.paths).unwrap());
}

#[test]
fn service_ledger_settlement_is_once_and_unknown_usage_stays_null() {
    let f = Fixture::new();
    let provider = f.provider();
    let c = database::open(&f.paths).unwrap();
    let model: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1",
            [&provider],
            |r| r.get(0),
        )
        .unwrap();
    for (request, attempt) in [("request-a", "attempt-a"), ("request-b", "attempt-b")] {
        c.execute("INSERT INTO service_gateway_requests(id,generation,protocol_in,request_path,state,created_at) VALUES(?1,0,'responses','/responses','routed',1)",[request]).unwrap();
        c.execute("INSERT INTO service_gateway_attempts(id,request_id,attempt_no,provider_id,provider_name,credential_id,model_id,upstream_model,protocol_out,state,started_at,
            price_snapshot_json) VALUES(?1,?2,1,?3,'测试','key',?4,'model','responses','started',1,?5)",params![attempt,request,provider,model,
            json!({"inputPriceMicros":1_000_000,"cachedInputPriceMicros":500_000,"outputPriceMicros":2_000_000}).to_string()]).unwrap();
    }
    let record = json!({"serviceRequestId":"request-a","attemptId":"attempt-a","ok":true,"statusCode":200,
        "measuredInput":100,"measuredOutput":10,"measuredCache":20});
    service::settle(&f.paths, &record).unwrap();
    // 重复结束事件必须忽略，不重复增加计数或覆盖已有用量。
    let mut repeated = record.clone();
    repeated["measuredInput"] = json!(999);
    service::settle(&f.paths, &repeated).unwrap();
    service::settle(&f.paths,&json!({"serviceRequestId":"request-b","attemptId":"attempt-b","ok":false,"statusCode":502})).unwrap();
    let rows = query(
        &c,
        "SELECT * FROM service_gateway_attempts ORDER BY id",
        &[],
    )
    .unwrap();
    assert_eq!(rows[0]["input_tokens"], 100);
    assert_eq!(rows[0]["estimated_cost_micros"], 110);
    assert!(rows[1]["input_tokens"].is_null());
    assert!(rows[1]["estimated_cost_micros"].is_null());
    assert_eq!(usage(&c).unwrap()["attempts"]["attemptCount"], 2);
}

#[test]
fn service_authorization_is_bound_to_agent_and_provider_enablement() {
    let f = Fixture::new();
    let provider = f.provider();
    let c = database::open(&f.paths).unwrap();
    let model: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1",
            [&provider],
            |r| r.get(0),
        )
        .unwrap();
    c.execute("INSERT INTO service_agent_instances(id,agent_type,name,config_path,client_protocol,provider_id,model_id,status,sync_status,created_at,updated_at)
        VALUES('instance','codex','测试','test:path','responses',?1,?2,'active','synced',1,1)",params![provider,model]).unwrap();
    c.execute("INSERT INTO service_gateway_tokens(id,token_hash,token_prefix,instance_id,created_at) VALUES('token',?1,'prefix','instance',1)",[hash("local-token")]).unwrap();
    assert!(service::authorize(&c, "bad-token").is_err());
    assert_eq!(
        service::authorize(&c, "local-token").unwrap()["agentType"],
        "codex"
    );
    c.execute(
        "UPDATE service_provider_agent_bindings SET enabled=0 WHERE provider_id=?1",
        [&provider],
    )
    .unwrap();
    assert!(service::authorize(&c, "local-token").is_err());
}

#[test]
fn service_migration_copies_official_credentials_and_reports_atomic_failure() {
    let f = Fixture::new();
    provider_store::write_codex_accounts(&f.paths,&[json!({"id":"official","type":"codex","email":"test@example.test",
        "account_id":"external","access_token":"access","refresh_token":"refresh","id_token":"id","auth":{"expiresAt":9999999999999_u64},
        "usage":{"plan_type":"plus"}})]).unwrap();
    migrate(&f.paths).unwrap();
    let c = database::open(&f.paths).unwrap();
    let copied = state(&f.paths).unwrap();
    let provider = text(&copied["providers"][0], "id");
    let credentials = account(&c, provider).unwrap();
    assert_eq!(credentials["accessToken"], "access");
    assert_eq!(credentials["refreshToken"], "refresh");
    assert_eq!(credentials["external_account_id"], "external");
    assert_eq!(copied["accounts"][0]["quota"]["plan_type"], "plus");
    assert_eq!(provider, "account:official");
    assert_eq!(source_provider_id(&c, "official").unwrap(), provider);
    assert_eq!(
        source_provider_id(&c, "account:official").unwrap(),
        provider
    );
    let catalog = public_catalog(&f.paths).unwrap();
    assert_eq!(catalog["codexAccounts"][0]["id"], "official");
    assert_eq!(catalog["codexAccounts"][0]["serviceProviderId"], provider);
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let auth = runtime
        .block_on(crate::api::codex_account::get_proxy_auth(
            &f.paths,
            "official",
            &json!({}),
        ))
        .unwrap();
    assert_eq!(auth["accessToken"], "access");
    let new_provider = text(
        &save(
            &f.paths,
            &json!({"name":"新官方账号","kind":"codex_account"}),
        )
        .unwrap(),
        "id",
    )
    .to_string();
    assert_eq!(
        source_provider_id(&c, &format!("account:{new_provider}")).unwrap(),
        new_provider
    );
    assert!(
        query(&c, "SELECT id FROM providers WHERE id=?1", &[&provider])
            .unwrap()
            .len()
            == 1
    );
    let bad = Fixture::new();
    let bad_c = database::open(&bad.paths).unwrap();
    bad_c
        .execute_batch("CREATE TABLE providers(id TEXT PRIMARY KEY);")
        .unwrap();
    assert!(migrate(&bad.paths).is_err());
    assert_eq!(
        query(
            &bad_c,
            "SELECT status FROM service_provider_migrations",
            &[]
        )
        .unwrap()[0]["status"],
        "failed"
    );
    assert!(state(&bad.paths).unwrap()["providers"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn service_account_rotation_keeps_quota_only_for_the_same_official_identity() {
    let f = Fixture::new();
    let provider = save(
        &f.paths,
        &json!({"name":"官方账号轮换","kind":"codex_account"}),
    )
    .unwrap();
    let provider = text(&provider, "id");
    let c = database::open(&f.paths).unwrap();
    save_account(
        &c,
        provider,
        &json!({
            "account_id":"account-a","access_token":"access-a","refresh_token":"refresh-a"
        }),
    )
    .unwrap();
    c.execute(
        "UPDATE service_provider_accounts SET quota_json='{\"plan_type\":\"plus\"}' WHERE provider_id=?1",
        [provider],
    ).unwrap();
    save_account(
        &c,
        provider,
        &json!({
            "account_id":"account-a","access_token":"access-a-new","refresh_token":"refresh-a-new"
        }),
    )
    .unwrap();
    assert_eq!(
        state(&f.paths).unwrap()["accounts"][0]["quota"]["plan_type"],
        "plus"
    );
    save_account(
        &c,
        provider,
        &json!({
            "account_id":"account-b","access_token":"access-b","refresh_token":"refresh-b"
        }),
    )
    .unwrap();
    let account = account(&c, provider).unwrap();
    assert_eq!(account["accessToken"], "access-b");
    assert_eq!(account["token_generation"], 2);
    assert_eq!(state(&f.paths).unwrap()["accounts"][0]["quota"], json!({}));
}

#[test]
fn service_period_usage_keeps_partial_tokens_and_mixed_currencies_unknown() {
    let f = Fixture::new();
    let provider = f.provider();
    let c = database::open(&f.paths).unwrap();
    let model: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1",
            [&provider],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        period_usage(&f.paths, &json!({})).unwrap()["data"]["summary"]["actualTokens"],
        0
    );
    for (id, agent, tokens, currency) in [
        ("a", "codex", Some(10), "USD"),
        ("b", "codex", None, "USD"),
        ("c", "claude", Some(20), "CNY"),
    ] {
        c.execute("INSERT INTO service_gateway_requests(id,agent_type,generation,protocol_in,request_path,state,created_at)
            VALUES(?1,?2,0,'responses','/responses','completed',100)",params![id,agent]).unwrap();
        c.execute("INSERT INTO service_gateway_attempts(id,request_id,attempt_no,provider_id,provider_name,credential_id,
            model_id,upstream_model,protocol_out,state,started_at,input_tokens,output_tokens,estimated_cost_micros,currency)
            VALUES(?1,?1,1,?2,'测试','key',?3,'model','responses','succeeded',100,?4,1,100,?5)",params![id,provider,model,tokens,currency]).unwrap();
    }
    let all = period_usage(&f.paths, &json!({})).unwrap();
    assert!(all["data"]["summary"]["actualTokens"].is_null());
    assert!(all["data"]["summary"]["totalCostUsd"].is_null());
    assert_eq!(all["data"]["summary"]["measuredCount"], 2);
    assert_eq!(all["data"]["summary"]["unknownUsageCount"], 1);
    let ledger = usage(&c).unwrap();
    assert_eq!(ledger["attempts"]["measuredCount"], 2);
    assert_eq!(
        ledger["byProvider"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["unknownUsageCount"].as_i64().unwrap())
            .sum::<i64>(),
        1
    );
    let codex = period_usage(&f.paths, &json!({"appType":"codex"})).unwrap();
    assert_eq!(codex["data"]["summary"]["attemptCount"], 2);
    assert_eq!(codex["data"]["summary"]["totalCostUsd"], 0.0002);
    let empty = period_usage(&f.paths, &json!({"startAt":101})).unwrap();
    assert_eq!(empty["data"]["summary"]["actualTokens"], 0);
}

#[test]
fn service_takeover_recovery_and_switch_refuse_inflight_requests() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let session =
        service_provider::internal_session(&f.paths, &provider, "model", "responses").unwrap();
    let c = database::open(&f.paths).unwrap();
    c.execute("INSERT INTO service_gateway_requests(id,instance_id,generation,protocol_in,request_path,state,created_at)
        VALUES('pending',?1,1,'responses','/responses','received',1)",[text(&session,"instanceId")]).unwrap();
    assert!(service_provider::detach(&f.paths, text(&session, "instanceId")).is_err());
    assert!(service::authorize(&c, text(&session, "token")).is_ok());
    assert!(service_provider::switch_backend(&f.paths, "legacy").is_err());
    assert!(active(&f.paths).unwrap());
    service_provider::recover(&f.paths).unwrap();
    assert_eq!(
        query(&c, "SELECT state FROM service_gateway_requests", &[]).unwrap()[0]["state"],
        "unknown"
    );
    assert!(service::authorize(&c, text(&session, "token")).is_err());
    let model: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1",
            [&provider],
            |r| r.get(0),
        )
        .unwrap();
    let dir = f.root.join("interrupted-client");
    let result = service_provider::activate(
        &f.paths,
        &json!([]),
        &json!({"providerId":provider,"modelId":model,"agentType":"codex","configPath":dir}),
    )
    .unwrap();
    c.execute(
        "UPDATE service_managed_files SET write_status='prepared' WHERE instance_id=?1",
        [text(&result, "instanceId")],
    )
    .unwrap();
    service_provider::recover(&f.paths).unwrap();
    assert!(!dir.join("config.toml").exists());
    service_provider::switch_backend(&f.paths, "legacy").unwrap();
}

#[test]
fn service_legacy_gate_blocks_switch_until_all_requests_finish() {
    let f = Fixture::new();
    let request = std::sync::Arc::new(legacy_request(&f.paths).unwrap());
    let stream = request.clone();
    assert!(service_provider::switch_backend(&f.paths, "service").is_err());
    drop(request);
    assert!(service_provider::switch_backend(&f.paths, "service").is_err());
    drop(stream);
    let gate = pause_legacy(&f.paths).unwrap();
    assert!(legacy_request(&f.paths).is_err());
    drop(gate);
    service_provider::switch_backend(&f.paths, "service").unwrap();
    assert!(legacy_request(&f.paths).is_err());
    service_provider::switch_backend(&f.paths, "legacy").unwrap();
    assert!(legacy_request(&f.paths).is_ok());
}

#[test]
fn service_quick_switch_uses_default_directory_and_restores_original() {
    let f = Fixture::new();
    let first = f.provider();
    let second = f.provider();
    let dir = f.root.join("quick-client");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), "model = \"original\"\n").unwrap();
    let targets = json!([{"id":"codex","configPath":dir,"version":"test"}]);
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let a = service_provider::quick_switch(
        &f.paths,
        &targets,
        &json!({"agentType":"codex","providerId":first}),
    )
    .unwrap();
    let b = service_provider::quick_switch(
        &f.paths,
        &targets,
        &json!({"agentType":"codex","providerId":second}),
    )
    .unwrap();
    assert_eq!(a["instanceId"], b["instanceId"]);
    let c = database::open(&f.paths).unwrap();
    assert_eq!(
        query(
            &c,
            "SELECT provider_id FROM service_agent_instances WHERE status='active'",
            &[]
        )
        .unwrap()[0]["provider_id"],
        second
    );
    service_provider::detach(&f.paths, text(&b, "instanceId")).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("config.toml")).unwrap(),
        "model = \"original\"\n"
    );
}

#[test]
fn service_business_backup_omits_secrets_and_restores_natural_model_relationships() {
    let source = Fixture::new();
    let provider = source.provider();
    service_provider::switch_backend(&source.paths, "service").unwrap();
    let c = database::open(&source.paths).unwrap();
    let model: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1",
            [&provider],
            |r| r.get(0),
        )
        .unwrap();
    c.execute(
        "UPDATE service_provider_agent_bindings SET default_model_id=?2 WHERE provider_id=?1",
        params![provider, model],
    )
    .unwrap();
    c.execute("INSERT INTO service_agent_model_routes(id,provider_id,agent_type,client_model_name,model_id)
        VALUES('route',?1,'codex','alias',?2)",params![provider,model]).unwrap();
    let bytes = database::backup(&source.paths).unwrap();
    let backup_path = source.root.join("backup-check.db");
    std::fs::write(&backup_path, &bytes).unwrap();
    let backup = Connection::open(backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM service_provider_keys", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        backup
            .query_row("SELECT enabled FROM service_provider_configs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let target = Fixture::new();
    service_provider::switch_backend(&target.paths, "service").unwrap();
    database::restore_merged(
        &target.paths,
        &bytes,
        &Map::from_iter([("*".into(), json!("backup"))]),
    )
    .unwrap();
    let target_c = database::open(&target.paths).unwrap();
    let mut fk = target_c.prepare("PRAGMA foreign_key_check").unwrap();
    assert!(!fk.exists([]).unwrap());
    assert!(credential(&target_c, &provider).is_err());
    // 同一个模型在另一设备拥有不同主键时，恢复绑定必须指向本机模型。
    target_c
        .execute(
            "UPDATE service_provider_agent_bindings SET default_model_id=NULL",
            [],
        )
        .unwrap();
    target_c
        .execute("DELETE FROM service_agent_model_routes", [])
        .unwrap();
    target_c
        .execute("DELETE FROM service_provider_models", [])
        .unwrap();
    target_c.execute("INSERT INTO service_provider_models(id,provider_id,upstream_model_id,created_at,updated_at) VALUES('local-model',?1,'model',1,1)",[&provider]).unwrap();
    database::restore_merged(
        &target.paths,
        &bytes,
        &Map::from_iter([("*".into(), json!("backup"))]),
    )
    .unwrap();
    assert_eq!(
        target_c
            .query_row(
                "SELECT model_id FROM service_agent_model_routes WHERE id='route'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "local-model"
    );
    assert_eq!(target_c.query_row("SELECT default_model_id FROM service_provider_agent_bindings WHERE agent_type='codex'",[],|r|r.get::<_,String>(0)).unwrap(),"local-model");
    assert!(!fk.exists([]).unwrap());
}
